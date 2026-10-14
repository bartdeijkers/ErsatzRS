//! Opt-in actual ordinary Router/media/Lua/scoped-output integration.
//! Native decoding and transport are independent; headed-client evidence is separate.

use super::*;
use super::{
    client_capture, designs, effective_channel_documents as effective,
    effective_designs as originals, effective_generations as generations,
    effective_publications as publications, ordinary_stream_flow as ordinary, scripted_controls,
};
use ersatzrs_core::{
    repositories::PlayoutRepository as _,
    scheduling::{
        PlayoutBuildMode, SchedulingMediaItem, SchedulingReferenceData, SchedulingRequest,
        SchedulingResult, ScriptedTeletextControl,
    },
};
use ersatzrs_domain::PlayoutScheduleKind;
use ersatzrs_web::host::teletext_effective_generations::AcceptedEffectiveTeletextOutputCandidate;
use serde_json::{json, Value};
use std::{
    io,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::io::AsyncWriteExt as _;

const MAX_TERMINAL_LOG: usize = 512 * 1024;

#[derive(Clone)]
struct TerminalLog(Arc<Mutex<(std::fs::File, usize)>>);
impl io::Write for TerminalLog {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut state = self.0.lock().unwrap();
        let count = bytes.len().min(MAX_TERMINAL_LOG - state.1);
        state.0.write_all(&bytes[..count])?;
        state.1 += count;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.lock().unwrap().0.flush()
    }
}
fn terminal_trace(prefix: &str, stage: &str) -> io::Result<()> {
    use tracing_subscriber::{layer::SubscriberExt as _, util::SubscriberInitExt as _};
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(format!("{prefix}.{stage}.stream-terminal.log"))?;
    let writer = TerminalLog(Arc::new(Mutex::new((file, 0))));
    // Each ignored native gate owns its nextest process. Capture only the
    // existing transport terminal diagnostics, without changing host logging.
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::filter::Targets::new()
                .with_target(
                    "ersatzrs_web::transport::iptv::stream",
                    tracing::Level::WARN,
                )
                .with_target(
                    "ersatzrs_web::transport::iptv::teletext_output",
                    tracing::Level::WARN,
                ),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_timer(tracing_subscriber::fmt::time::uptime())
                .with_writer(move || writer.clone()),
        )
        .try_init()
        .map_err(|_| {
            io::Error::other("isolated effective terminal subscriber could not be installed")
        })
}

async fn source_fixture(tools: &ordinary::Tools) -> (Fixture, Vec<u8>) {
    let (fixture, source) = ordinary::installed_profiles(tools).await;
    // Replace the same ordinary owners, never add a second channel playout.
    for channel in [1, 2] {
        let file = fixture
            .root
            .path()
            .join(format!("ordinary-controls-{channel}.lua"));
        sqlx::query(r#"UPDATE "Playout" SET "ScheduleKind"=?,"ScheduleFile"=? WHERE "Id"=? AND "ChannelId"=?"#)
            .bind(PlayoutScheduleKind::Scripted as i32).bind(file.to_str().unwrap()).bind(channel).bind(channel).execute(&fixture.state.pool).await.unwrap();
    }
    seed_playable_media(&fixture).await;
    let mut policy: Value =
        serde_json::from_str(include_str!("../fixtures/teletext-output-policy-v1.json")).unwrap();
    policy["version"] = json!(2);
    policy["live_updates"] = json!({"poll_interval_ms":25,"recovery_timeout_ms":2000});
    ordinary::setting(
        &fixture,
        "teletext.output.policy",
        &serde_json::to_string(&policy).unwrap(),
    )
    .await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM "Playout""#)
            .fetch_one(&fixture.state.pool)
            .await
            .unwrap(),
        2
    );
    (fixture, source)
}

async fn seed_playable_media(fixture: &Fixture) {
    for statement in [
        r#"INSERT INTO "MediaSource" ("Id") VALUES (71)"#,
        r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (71)"#,
        r#"INSERT INTO "Library" ("Id","MediaSourceId","MediaKind","Name") VALUES (71,71,1,'Synthetic local movie')"#,
        r#"INSERT INTO "LocalLibrary" ("Id") VALUES (71)"#,
        r#"INSERT INTO "LibraryPath" ("Id","LibraryId") VALUES (71,71)"#,
        r#"INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES (71,71,0)"#,
        r#"INSERT INTO "Movie" ("Id") VALUES (71)"#,
        r#"INSERT INTO "MovieMetadata" ("DateAdded","DateUpdated","MetadataKind","MovieId","Title") VALUES ('2026-01-01','2026-01-01',0,71,'Synthetic red video and tone')"#,
        r#"INSERT INTO "MediaVersion" ("Id","DateAdded","DateUpdated","Duration","Height","MovieId","Name","RFrameRate","VideoScanKind","Width") VALUES (71,'2026-01-01','2026-01-01','00:00:12',90,71,'Synthetic','25/1',1,160)"#,
        r#"INSERT INTO "MediaStream" ("Index","Codec","MediaStreamKind","Channels","Default","Forced","MediaVersionId","PixelFormat") VALUES (0,'mpeg2video',1,0,1,0,71,'yuv420p')"#,
        r#"INSERT INTO "MediaStream" ("Index","Codec","MediaStreamKind","Language","Channels","Default","Forced","MediaVersionId") VALUES (1,'aac',2,'eng',1,1,0,71)"#,
    ] {
        sqlx::query(statement)
            .execute(&fixture.state.pool)
            .await
            .unwrap();
    }
    sqlx::query(r#"UPDATE "LibraryPath" SET "Path"=? WHERE "Id"=71"#)
        .bind(fixture.root.path().to_str().unwrap())
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    let source = fixture.root.path().join("synthetic-client-source.ts");
    sqlx::query(r#"INSERT INTO "MediaFile" ("Id","MediaVersionId","Path","PathHash") VALUES (71,71,?,'synthetic-source')"#)
        .bind(source.to_str().unwrap()).execute(&fixture.state.pool).await.unwrap();
}

async fn build_and_publish(
    fixture: &Fixture,
    channel: i64,
    controls: &str,
    count: usize,
    previous: Option<&SchedulingResult>,
) -> SchedulingResult {
    let file = fixture
        .root
        .path()
        .join(format!("ordinary-controls-{channel}.lua"));
    std::fs::write(&file, format!("{controls}\nadd_count('clip',{count})")).unwrap();
    let repository =
        ersatzrs_infra::repositories::SqlitePlayoutRepository(fixture.state.pool.clone());
    let mut playout = repository.get_with_anchor(channel).await.unwrap();
    assert_eq!(playout.channel_id, channel);
    assert_eq!(playout.schedule_kind, PlayoutScheduleKind::Scripted);
    let start = previous.map_or_else(
        || ersatzrs_core::scheduling::truncate_to_persisted_precision(Utc::now()),
        |value| value.anchor.as_ref().unwrap().next_start,
    );
    if let Some(previous) = previous {
        playout.anchor = previous.anchor.clone();
    }
    let request = SchedulingRequest {
        playout,
        start,
        finish: start + chrono::Duration::minutes(5),
        mode: if previous.is_some() {
            PlayoutBuildMode::Continue
        } else {
            PlayoutBuildMode::Reset
        },
        reference_data: SchedulingReferenceData {
            media_by_key: BTreeMap::from([(
                "clip".to_owned(),
                vec![SchedulingMediaItem::new(71, chrono::Duration::seconds(12))],
            )]),
            existing_items: previous.map_or_else(Vec::new, |value| value.added_items.clone()),
            existing_history: previous.map_or_else(Vec::new, |value| value.added_history.clone()),
            ..SchedulingReferenceData::default()
        },
    };
    let controls: Arc<dyn ScriptedTeletextControl> = fixture.state.application.clone();
    let builder = ersatzrs_infra::scripted_scheduling::playout_builder_with_teletext(
        fixture.state.pool.clone(),
        tokio::runtime::Handle::current(),
        None,
        Some(controls),
    );
    let result = tokio::task::spawn_blocking(move || builder.build(request))
        .await
        .unwrap()
        .unwrap();
    assert!(result
        .added_items
        .iter()
        .all(|item| item.media_item_id == 71 && item.playout_id == channel));
    assert!(result
        .added_items
        .iter()
        .all(|item| item.finish - item.start == chrono::Duration::seconds(12)));
    scripted_controls::publish_result(fixture, &result).await;
    result
}

async fn capture_pair(fixture: &Fixture) -> [Vec<u8>; 2] {
    // Align with the next genuine published item, avoiding an arbitrary short
    // source tail. This reads the timeline; it never rewrites it or Lua state.
    let start:Option<String>=sqlx::query_scalar(r#"SELECT "Start" FROM "PlayoutItem" WHERE "PlayoutId"=1 AND julianday("Start")>julianday(?) ORDER BY "Start" LIMIT 1"#)
        .bind(Utc::now().to_rfc3339())
        .fetch_optional(&fixture.state.pool).await.unwrap();
    if let Some(start) = start {
        let start = chrono::NaiveDateTime::parse_from_str(&start, "%Y-%m-%d %H:%M:%S%.f")
            .unwrap()
            .and_utc();
        let wait = (start - Utc::now()).to_std().unwrap_or_default();
        assert!(wait <= Duration::from_secs(12));
        tokio::time::sleep(wait + Duration::from_millis(25)).await;
    }
    let (first, second) =
        tokio::join!(ordinary::capture(fixture, 1), ordinary::capture(fixture, 2));
    [first, second]
}

async fn checkpoints(fixture: &Fixture) -> Vec<(i64, String, String)> {
    sqlx::query_as(r#"SELECT "PlayoutId","CheckpointDate","Context" FROM "PlayoutScriptedCheckpoint" ORDER BY "PlayoutId","CheckpointDate""#)
        .fetch_all(&fixture.state.pool).await.unwrap()
}

async fn verify_pair(
    tools: &ordinary::Tools,
    source: &[u8],
    bytes: &[Vec<u8>; 2],
    selected: [&AcceptedEffectiveTeletextOutputCandidate; 2],
) {
    for (index, value) in selected.into_iter().enumerate() {
        ordinary::verify_authored(tools, source, &bytes[index], value.service().unwrap()).await;
    }
}
async fn write_new(prefix: &str, label: &str, bytes: &[u8]) {
    let path = PathBuf::from(format!("{prefix}.{label}.ts"));
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .await
        .unwrap();
    file.write_all(bytes).await.unwrap();
    file.flush().await.unwrap();
}

async fn scoped_request(
    fixture: &Fixture,
    channel: i64,
) -> ersatzrs_domain::TeletextPublicationRequest {
    let candidate = effective::candidate(fixture, channel, Some(ADDON))
        .await
        .unwrap();
    let saved = originals::saved_design(
        originals::save(fixture, candidate, None, designs::budget())
            .await
            .unwrap(),
    );
    publications::request(fixture, &saved, None).await
}

#[tokio::test]
#[ignore = "requires pinned FFmpeg/ffprobe9,TSDuck and explicit create-new effective capture prefix"]
async fn actual_scripted_ordinary_route_keeps_two_scoped_themes_exact_before_and_after_reopen() {
    let prefix = client_capture::selected("ERSATZRS_TELETEXT_EFFECTIVE_CAPTURE");
    terminal_trace(&prefix, "scoped").unwrap();
    let tools = ordinary::Tools::load().await;
    let (mut fixture, source) = source_fixture(&tools).await;
    write_new(&prefix, "source", &source).await;
    for channel in [1, 2] {
        build_and_publish(&fixture, channel, "", 20, None).await;
    }
    let base = [
        ordinary::accept(&fixture, 1).await,
        ordinary::accept(&fixture, 2).await,
    ];
    let baseline = capture_pair(&fixture).await;
    for (bytes, selected) in baseline.iter().zip(&base) {
        ordinary::verify_authored(&tools, &source, bytes, selected.service()).await;
    }
    let profiles = [
        fixture.profile("Scoped night", "night", "good").await,
        fixture.profile("Scoped day", "day", "good").await,
    ];
    for (channel, profile) in [1, 2].into_iter().zip(&profiles) {
        build_and_publish(&fixture,channel,&format!("assert(teletext_profile('{ADDON}',{}).ok)\nassert(teletext_enabled('{ADDON}',true).ok)",profile.id),20,None).await;
    }
    let pending = [
        scoped_request(&fixture, 1).await,
        scoped_request(&fixture, 2).await,
    ];
    let calls = tokio::fs::read(fixture.root.path().join("ordinary-calls"))
        .await
        .unwrap();
    let before = designs::config(&fixture).await;
    let checkpoint_before = checkpoints(&fixture).await;
    let plain = capture_pair(&fixture).await;
    for bytes in &plain {
        ordinary::verify_plain(&tools, &source, bytes).await;
    }
    let selected = [
        generations::accepted(
            generations::accept(&fixture, &pending[0], 1, None)
                .await
                .unwrap(),
        ),
        generations::accepted(
            generations::accept(&fixture, &pending[1], 1, None)
                .await
                .unwrap(),
        ),
    ];
    assert_ne!(
        selected[0].service().unwrap().magazine_units(),
        selected[1].service().unwrap().magazine_units()
    );
    assert_eq!(
        ordinary::raster(selected[0].service().unwrap(), 200),
        ordinary::raster(base[1].service(), 200)
    );
    assert_eq!(
        ordinary::raster(selected[1].service().unwrap(), 200),
        ordinary::raster(base[0].service(), 200)
    );
    let captured = capture_pair(&fixture).await;
    verify_pair(&tools, &source, &captured, [&selected[0], &selected[1]]).await;
    fixture = fixture.reopen().await;
    let reopened = capture_pair(&fixture).await;
    verify_pair(&tools, &source, &reopened, [&selected[0], &selected[1]]).await;
    assert_eq!(designs::config(&fixture).await, before);
    assert_eq!(checkpoints(&fixture).await, checkpoint_before);
    assert_eq!(
        tokio::fs::read(fixture.root.path().join("ordinary-calls"))
            .await
            .unwrap(),
        calls
    );
    for (channel, value) in [1, 2].into_iter().zip(&base) {
        assert_eq!(
            fixture
                .state
                .application
                .accepted_teletext_generation(channel)
                .await
                .unwrap(),
            Some(value.generation().clone())
        );
    }
    assert!(fixture.playout.try_recv().is_err());
    for (label, bytes) in [
        ("base.1", &baseline[0]),
        ("base.2", &baseline[1]),
        ("pending.1", &plain[0]),
        ("pending.2", &plain[1]),
        ("scoped.1", &captured[0]),
        ("scoped.2", &captured[1]),
        ("reopened.1", &reopened[0]),
        ("reopened.2", &reopened[1]),
    ] {
        write_new(&prefix, label, bytes).await;
    }
}

#[tokio::test]
#[ignore = "requires pinned FFmpeg/ffprobe9,TSDuck and actual Lua continuation/expiry ordinary capture"]
async fn actual_scripted_off_ordinary_absence_expires_into_retained_accepted_base() {
    let prefix = client_capture::selected("ERSATZRS_TELETEXT_EFFECTIVE_CAPTURE");
    terminal_trace(&prefix, "off").unwrap();
    let tools = ordinary::Tools::load().await;
    let (mut fixture, source) = source_fixture(&tools).await;
    write_new(&prefix, "off.source", &source).await;
    build_and_publish(&fixture, 1, "", 20, None).await;
    build_and_publish(&fixture, 2, "", 20, None).await;
    let base = ordinary::accept(&fixture, 1).await;
    let off = build_and_publish(
        &fixture,
        1,
        &format!("assert(teletext_enabled('{ADDON}',false).ok)"),
        1,
        None,
    )
    .await;
    let finish = off.teletext_intervals[0].finish;
    let tail = build_and_publish(&fixture, 1, "", 20, Some(&off)).await;
    assert_eq!(tail.added_items[0].start, finish);
    assert!(tail.teletext_intervals.is_empty());
    assert!(tail.added_items.iter().any(|item| item.start >= finish));
    let candidate = effective::candidate(&fixture, 1, None).await.unwrap();
    let value = originals::saved_design(
        originals::save(&fixture, candidate, None, designs::budget())
            .await
            .unwrap(),
    );
    let pending = publications::request(&fixture, &value, None).await;
    let scoped = generations::accepted(
        generations::accept(&fixture, &pending, 1, None)
            .await
            .unwrap(),
    );
    assert!(scoped.service().is_none());
    let calls = tokio::fs::read(fixture.root.path().join("ordinary-calls"))
        .await
        .unwrap();
    let before = designs::config(&fixture).await;
    let checkpoint_before = checkpoints(&fixture).await;
    let absent = ordinary::capture(&fixture, 1).await;
    write_new(&prefix, "off.absent", &absent).await;
    ordinary::verify_plain(&tools, &source, &absent).await;
    tokio::time::sleep(
        (finish - Utc::now()).to_std().unwrap_or_default() + Duration::from_millis(25),
    )
    .await;
    assert!(Utc::now() >= finish);
    let recovered = ordinary::capture(&fixture, 1).await;
    write_new(&prefix, "off.expired-base", &recovered).await;
    ordinary::verify_authored(&tools, &source, &recovered, base.service()).await;
    assert_eq!(
        fixture
            .state
            .application
            .effective_teletext_accepted_generation(1)
            .await
            .unwrap(),
        Some(scoped.generation().clone())
    );
    assert_eq!(
        fixture
            .state
            .application
            .accepted_teletext_generation(1)
            .await
            .unwrap(),
        Some(base.generation().clone())
    );
    assert_eq!(
        tokio::fs::read(fixture.root.path().join("ordinary-calls"))
            .await
            .unwrap(),
        calls
    );
    assert_eq!(designs::config(&fixture).await, before);
    assert_eq!(checkpoints(&fixture).await, checkpoint_before);
    assert!(fixture.playout.try_recv().is_err());
}
