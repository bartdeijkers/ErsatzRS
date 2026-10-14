//! Natural published Scripted expiry on original finite ordinary responses.
//! Native transport/decoding is independent of still-required headed-client proof.

use super::super::{
    authoring::AuthoringFixture,
    channel_documents, client_capture, designs,
    editorial_imports::live_updates::{live_tcp as shared, Capture},
    repeated_editorial_imports, scripted_controls, Fixture, ADDON,
};
use super::live_client as cold;
use super::{export, import, original, stable_state};
use chrono::{DateTime, Utc};
use ersatzrs_app::teletext_design_documents::decode_teletext_effective_design_documents;
use ersatzrs_core::{
    repositories::PlayoutRepository as _,
    scheduling::{
        truncate_to_persisted_precision, PlayoutBuildMode, SchedulingMediaItem,
        SchedulingReferenceData, SchedulingRequest, SchedulingResult, ScriptedTeletextControl,
    },
};
use ersatzrs_domain::{TeletextDesignRevision, TeletextOverrideState};
use ersatzrs_web::host::{
    teletext_authoring::{prepare_static_teletext_design, TeletextAuthoringIdentity},
    teletext_effective_generations::AcceptedEffectiveTeletextOutputCandidate,
    teletext_generations::{reload_accepted_teletext_generation, AcceptedTeletextOutputCandidate},
};
use futures_util::FutureExt as _;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io,
    panic::AssertUnwindSafe,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::time::{timeout_at, Instant};

const DIRECTORY_ENV: &str = "ERSATZRS_TELETEXT_NATURAL_EXPIRY_DIR";
const MAX_SOURCE: usize = 48 * 1024 * 1024;

#[tokio::test]
#[ignore = "requires explicit retained natural-expiry captures, create-new offline output and pinned FFmpeg9/TSDuck; proves no live cleanup or headed acceptance"]
async fn offline_natural_expiry_raw_captures_verify_clock_transport_av_and_pages200201() {
    let input = PathBuf::from(client_capture::selected(
        "ERSATZRS_TELETEXT_NATURAL_EXPIRY_OFFLINE_INPUT_DIR",
    ));
    let output = PathBuf::from(client_capture::selected(
        "ERSATZRS_TELETEXT_NATURAL_EXPIRY_OFFLINE_OUTPUT_DIR",
    ));
    let ffmpeg = client_capture::selected("ERSATZRS_TELETEXT_TEST_FFMPEG");
    let ffprobe = client_capture::selected("FFPROBE_PATH");
    let tsanalyze = client_capture::selected("ERSATZRS_TELETEXT_TEST_TSANALYZE");
    client_capture::check_tools(&ffmpeg, &ffprobe).await;
    shared::verify_saved_native_pages(
        &input,
        &output,
        (&ffmpeg, &ffprobe, &tsanalyze),
        &["200", "201"],
        false,
    )
    .await;
}

struct Base {
    fixture: AuthoringFixture,
    accepted: [AcceptedTeletextOutputCandidate; 2],
}

async fn schedule(
    fixture: &Fixture,
    channel: i64,
    script: &str,
    previous: Option<&SchedulingResult>,
) -> SchedulingResult {
    let path = fixture.root.path().join(format!("controls-{channel}.lua"));
    std::fs::write(&path, script).unwrap();
    let repository =
        ersatzrs_infra::repositories::SqlitePlayoutRepository(fixture.state.pool.clone());
    let mut playout = repository.get_with_anchor(92 + channel).await.unwrap();
    assert_eq!(playout.channel_id, channel);
    playout.schedule_file = Some(path.to_str().unwrap().to_owned());
    let start = previous.map_or_else(
        || truncate_to_persisted_precision(Utc::now() - chrono::Duration::seconds(1)),
        |result| result.anchor.as_ref().unwrap().next_start,
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
                vec![SchedulingMediaItem::new(71, chrono::Duration::seconds(60))],
            )]),
            existing_items: previous.map_or_else(Vec::new, |result| result.added_items.clone()),
            existing_history: previous.map_or_else(Vec::new, |result| result.added_history.clone()),
            existing_scripted_checkpoints: previous
                .map_or_else(Vec::new, |result| result.scripted_checkpoints.clone()),
            ..Default::default()
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
        .all(|item| item.playout_id == 92 + channel && item.media_item_id == 71));
    scripted_controls::publish_result(fixture, &result).await;
    result
}

async fn base_original(fixture: &AuthoringFixture, channel: i64) -> TeletextAuthoringIdentity {
    let mut request = AuthoringFixture::request(channel, fixture.observe(channel).await);
    let mut input: Value = serde_json::from_str(&request.input_json).unwrap();
    input["entry"] = json!({"page":200,"subcode":0});
    request.input_json = serde_json::to_string(&input).unwrap();
    let prepared = prepare_static_teletext_design(
        &fixture.fixture.state.application,
        request,
        designs::deadline(),
    )
    .await
    .unwrap();
    assert!(prepared.preview.unwrap().raster().is_some());
    prepared.identity
}

async fn recover_base(fixture: &Fixture, channel: i64) -> AcceptedTeletextOutputCandidate {
    reload_accepted_teletext_generation(
        &fixture.state.application,
        channel,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap()
}

async fn bases(fixture: AuthoringFixture) -> Base {
    for (index, channel) in [1, 2].into_iter().enumerate() {
        let binding = fixture
            .fixture
            .state
            .application
            .channel_teletext_bindings(channel)
            .await
            .unwrap()
            .into_iter()
            .find(|binding| binding.binding.addon_id == ADDON)
            .unwrap()
            .binding;
        let saved = fixture
            .fixture
            .bind(channel, &fixture.profiles[index], Some(binding.revision))
            .await;
        assert!(saved.enabled);
        let no_commands = schedule(&fixture.fixture, channel, "add_count('clip',4)", None).await;
        assert!(no_commands.teletext_intervals.is_empty());
        assert_eq!(no_commands.added_items.len(), 4);
        assert!(no_commands
            .added_items
            .iter()
            .all(|item| item.finish - item.start == chrono::Duration::seconds(60)));
        cold::publish(&fixture, base_original(&fixture, channel).await).await;
    }
    let accepted = [
        recover_base(&fixture.fixture, 1).await,
        recover_base(&fixture.fixture, 2).await,
    ];
    Base { fixture, accepted }
}

struct Scoped {
    accepted: AcceptedEffectiveTeletextOutputCandidate,
    original: TeletextDesignRevision,
    finish: DateTime<Utc>,
}

async fn short_on(base: &Base) -> Scoped {
    let fixture = &base.fixture.fixture;
    let script=format!("assert(teletext_profile('{ADDON}',{}).ok)\nassert(teletext_enabled('{ADDON}',true).ok)\nadd_duration('clip','00:00:12',{{trim=true,stop_before_end=true}})",base.fixture.profiles[0].id);
    let short = schedule(fixture, 1, &script, None).await;
    assert_eq!(short.added_items.len(), 1);
    assert_eq!(
        short.added_items[0].finish - short.added_items[0].start,
        chrono::Duration::seconds(12)
    );
    assert_eq!(short.teletext_intervals.len(), 1);
    let interval = &short.teletext_intervals[0];
    assert!(matches!(interval.state, TeletextOverrideState::Enabled(_)));
    assert_eq!(interval.finish, short.added_items[0].finish);
    let tail = schedule(fixture, 1, "add_count('clip',4)", Some(&short)).await;
    assert!(tail.teletext_intervals.is_empty());
    assert_eq!(tail.added_items[0].start, interval.finish);
    assert!(tail
        .added_items
        .iter()
        .all(|item| item.finish - item.start == chrono::Duration::seconds(60)));
    assert!(
        ersatzrs_infra::repositories::SqlitePlayoutRepository(fixture.state.pool.clone())
            .published_teletext_intervals_at(1, interval.finish)
            .await
            .unwrap()
            .is_empty()
    );
    let identity = cold::prepare(&base.fixture, 1).await;
    let root = original(fixture, identity).await;
    let exported = export(fixture, &root).await;
    let concept = import(
        fixture,
        repeated_editorial_imports::request(
            &exported,
            1,
            &[(channel_documents::address(200, 0), 2, b'Q')],
        ),
    )
    .await;
    let identity = TeletextAuthoringIdentity {
        channel_id: 1,
        design_id: concept.head().id,
        revision: concept.head().revision,
    };
    cold::publish(&base.fixture, identity).await;
    let accepted = cold::recover(fixture, 1).await;
    let original = original(fixture, identity).await;
    let saved_scope =
        decode_teletext_effective_design_documents(&original.document, designs::budget()).unwrap();
    assert_eq!(saved_scope.selection().valid_until, Some(interval.finish));
    assert_eq!(accepted.selection(), saved_scope.selection());
    assert!(
        Utc::now() < interval.finish,
        "actual On preparation exceeded its genuine short programme"
    );
    Scoped {
        accepted,
        original,
        finish: interval.finish,
    }
}

async fn retained(fixture: &Fixture) -> Vec<Vec<String>> {
    let mut rows = stable_state(fixture).await;
    for sql in [
        r#"SELECT json_array("Key","Value") FROM "ConfigElement" ORDER BY "Key""#,
        r#"SELECT json_array("Id","AddonId","Name","Revision","IsDeleted","InstallationStateRevision","InstallationContentSha256") FROM "TeletextProfile" ORDER BY "Id""#,
        r#"SELECT json_array("ProfileId","Key","Kind","Value","Reference") FROM "TeletextProfileSetting" ORDER BY "ProfileId","Key""#,
        r#"SELECT json_array("DesignId","Revision","DocumentVersion","Digest",hex("Document")) FROM "TeletextDesignRevision" ORDER BY "DesignId","Revision""#,
        r#"SELECT json_array("Id","DesignId","Revision","Digest",hex("Receipt")) FROM "TeletextDesignExportReceipt" ORDER BY "Id""#,
    ] {
        rows.push(
            sqlx::query_scalar(sql)
                .fetch_all(&fixture.state.pool)
                .await
                .unwrap(),
        );
    }
    rows
}

async fn base_wire(views: &[Capture; 3], after: [usize; 3], base: &Base) {
    for (index, view) in views.iter().enumerate() {
        view.service_encoded(
            after[index],
            base.accepted[usize::from(index == 2)].service(),
        )
        .await;
    }
}

struct Expiry {
    scoped: Scoped,
    offsets: [usize; 3],
    base_offsets: [usize; 3],
    next_base_offsets: [usize; 3],
    observed_utc: DateTime<Utc>,
    restored_base_utc: DateTime<Utc>,
    next_base_utc: DateTime<Utc>,
}

async fn expire_on_same_responses(
    base: &Base,
    origin: &shared::Origin,
    deadline: Instant,
) -> io::Result<Expiry> {
    shared::wait_attached(origin, deadline).await?;
    let views = origin.captures();
    base_wire(views, [0; 3], base).await;
    let positions = views.each_ref().map(Capture::position);
    let scoped = short_on(base).await;
    for (index, view) in views[..2].iter().enumerate() {
        view.service_encoded(positions[index], scoped.accepted.service().unwrap())
            .await;
    }
    views[2]
        .service_encoded(positions[2], base.accepted[1].service())
        .await;
    let before = retained(&base.fixture.fixture).await;
    let calls = base.fixture.calls();
    let package = ersatzrs_infra::addons::calculate_addon_content_sha256(
        &base.fixture.fixture.root.path().join("package"),
    )
    .await
    .unwrap();
    tokio::time::sleep((scoped.finish - Utc::now()).to_std().unwrap_or_default()).await;
    let observed_utc = Utc::now();
    assert!(observed_utc >= scoped.finish);
    let expiry_offsets = views.each_ref().map(Capture::position);
    base_wire(views, expiry_offsets, base).await;
    let base_offsets = views.each_ref().map(Capture::position);
    let restored_base_utc = Utc::now();
    // Require another complete model cycle on the same bodies before cleanup.
    base_wire(views, base_offsets, base).await;
    let next_base_offsets = views.each_ref().map(Capture::position);
    let next_base_utc = Utc::now();
    // Preserve the exact expiry prefix and the decoder's existing packet floor.
    // This waits for actual original-body data, under the same outer deadline.
    while views
        .iter()
        .enumerate()
        .any(|(index, view)| view.position() - expiry_offsets[index] < 188 * 1000)
    {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(retained(&base.fixture.fixture).await, before);
    assert_eq!(base.fixture.calls(), calls);
    assert_eq!(
        ersatzrs_infra::addons::calculate_addon_content_sha256(
            &base.fixture.fixture.root.path().join("package")
        )
        .await
        .unwrap(),
        package
    );
    assert!(origin.single_requests());
    Ok(Expiry {
        scoped,
        offsets: expiry_offsets,
        base_offsets,
        next_base_offsets,
        observed_utc,
        restored_base_utc,
        next_base_utc,
    })
}

async fn verify_after_expiry(
    directory: &Path,
    ffmpeg: &str,
    views: &[Capture; 3],
    expiry: &Expiry,
) {
    for (index, view) in views.iter().enumerate() {
        let bytes = view.bytes();
        let complete = bytes.len() / 188 * 188;
        let fragment = &bytes[expiry.offsets[index]..complete];
        assert!(fragment.len() >= 188 * 1000);
        shared::write_new(
            &directory.join(format!(
                "{}-post-expiry.ts",
                ["day", "observer", "night"][index]
            )),
            fragment,
        )
        .unwrap();
        for page in ["200", "201"] {
            let text =
                client_capture::decode_page_text_with_discovery(ffmpeg, fragment, page, true).await;
            assert!(
                text.contains("AZ"),
                "independent post-expiry decoder lost base page"
            );
            assert!(
                !text.contains('Q'),
                "expired scoped Q remained in the raw post-expiry fragment"
            );
            assert!(!text.contains('R'));
            shared::write_new(
                &directory.join(format!(
                    "{}-post-expiry-{page}.srt",
                    ["day", "observer", "night"][index]
                )),
                text.as_bytes(),
            )
            .unwrap();
        }
    }
}

#[tokio::test]
async fn genuine_trimmed_scope_and_continue_tail_preserve_fresh_accepted_bases() {
    let base = bases(cold::fixture().await).await;
    let scoped = short_on(&base).await;
    for (index, expected) in base.accepted.iter().enumerate() {
        // Active Lua blocks current base admission; its independent accepted
        // history must remain retained for recovery after the genuine expiry.
        assert_eq!(
            base.fixture
                .fixture
                .state
                .application
                .accepted_teletext_generation(i64::try_from(index + 1).unwrap())
                .await
                .unwrap()
                .as_ref(),
            Some(expected.generation())
        );
    }
    assert_eq!(scoped.original.document_version, 2);
    assert_eq!(scoped.accepted.generation().request.revision, 2);
    assert_eq!(scoped.accepted.selection().valid_until, Some(scoped.finish));
    base.fixture.fixture.state.pool.close().await;
}

async fn native_trial(
    directory: &Path,
    base: &Base,
    source: &[u8],
    origin: &shared::Origin,
) -> io::Result<Expiry> {
    let deadline = Instant::now() + Duration::from_mins(3);
    let expiry = timeout_at(deadline, expire_on_same_responses(base, origin, deadline))
        .await
        .map_err(|_| io::Error::other("natural-expiry trial exceeded its single deadline"))??;
    shared::write_new(&directory.join("expiry.json"),&serde_json::to_vec_pretty(&json!({
        "probe_version":1,"scope":"Natural UTC expiry on three original native responses; no headed-client evidence",
        "source_sha256":shared::hash(source),"source_bytes":source.len(),"source_duration_seconds":60,
        "scoped_design_id":expiry.scoped.original.design_id,"scoped_revision":expiry.scoped.original.revision,"scoped_digest":expiry.scoped.original.digest,
        "scoped_generation_id":expiry.scoped.accepted.generation().id,"original_valid_until":expiry.scoped.finish.to_rfc3339(),
        "observed_utc":expiry.observed_utc.to_rfc3339(),"expiry_offsets":expiry.offsets,"restored_base_offsets":expiry.base_offsets,
        "restored_base_utc":expiry.restored_base_utc.to_rfc3339(),"next_base_offsets":expiry.next_base_offsets,"next_base_utc":expiry.next_base_utc.to_rfc3339(),
        "base_generation_ids":base.accepted.iter().map(|value|value.generation().id).collect::<Vec<_>>(),
        "source_requests_per_viewer":1,"immutable_scope_and_publication_state_across_expiry":true,
        "limitations":"Native wire/decoder evidence only; actual headed clearing/navigation, full Level 2.5 and platform acceptance remain open."})).unwrap())?;
    Ok(expiry)
}

#[tokio::test]
#[ignore = "Requires pinned FFmpeg9/TSDuck and create-new natural-expiry evidence directory; native-only, no headed-client acceptance"]
async fn original_ordinary_responses_restore_accepted_base_after_natural_scoped_expiry() {
    let directory = PathBuf::from(client_capture::selected(DIRECTORY_ENV));
    std::fs::create_dir(&directory).unwrap();
    shared::terminal_trace(&directory).unwrap();
    let ffmpeg = client_capture::selected("ERSATZRS_TELETEXT_TEST_FFMPEG");
    let ffprobe = client_capture::selected("FFPROBE_PATH");
    let tsanalyze = client_capture::selected("ERSATZRS_TELETEXT_TEST_TSANALYZE");
    client_capture::check_tools(&ffmpeg, &ffprobe).await;
    let fixture = cold::fixture().await;
    let source_path =
        client_capture::generate_source_with_duration(&ffmpeg, fixture.fixture.root.path(), 60)
            .await;
    cold::configure_source(&fixture.fixture, &source_path).await;
    let source = shared::read_bounded(&source_path, MAX_SOURCE).unwrap();
    shared::write_new(&directory.join("source.ts"), &source).unwrap();
    let base = bases(fixture).await;
    let mut origin = shared::Origin::start(&base.fixture.fixture, &directory)
        .await
        .unwrap();
    let mut viewers = [
        shared::Observer::start_for(&origin, 1, "day"),
        shared::Observer::start(&origin),
        shared::Observer::start_for(&origin, 2, "night"),
    ];
    let outcome = AssertUnwindSafe(native_trial(&directory, &base, &source, &origin))
        .catch_unwind()
        .await;
    let mut closed = Vec::new();
    for viewer in &mut viewers {
        closed.push(viewer.close().await);
    }
    let server_closed = origin.close().await;
    let cleanup = shared::wait_for_cleanup().await;
    let captures = shared::save_captures(&directory, &origin);
    let expiry = match outcome {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => {
            base.fixture.fixture.state.pool.close().await;
            panic!("natural-expiry trial failed: {error}")
        }
        Err(panic) => {
            base.fixture.fixture.state.pool.close().await;
            std::panic::resume_unwind(panic)
        }
    };
    for (index, expected) in base.accepted.iter().enumerate() {
        let actual = recover_base(&base.fixture.fixture, i64::try_from(index + 1).unwrap()).await;
        assert_eq!(actual.generation(), expected.generation());
        assert_eq!(
            designs::units(actual.service()),
            designs::units(expected.service())
        );
    }
    assert_eq!(
        original(
            &base.fixture.fixture,
            TeletextAuthoringIdentity {
                channel_id: 1,
                design_id: expiry.scoped.original.design_id,
                revision: expiry.scoped.original.revision
            }
        )
        .await,
        expiry.scoped.original
    );
    assert_eq!(
        shared::read_bounded(&source_path, MAX_SOURCE).unwrap(),
        source
    );
    base.fixture.fixture.state.pool.close().await;
    for result in closed {
        result.unwrap();
    }
    server_closed.unwrap();
    cleanup.unwrap();
    captures.unwrap();
    verify_after_expiry(&directory, &ffmpeg, origin.captures(), &expiry).await;
    shared::verify_native_pages(
        &directory,
        (&ffmpeg, &ffprobe, &tsanalyze),
        &source,
        origin.captures(),
        &["200", "201"],
        false,
    )
    .await;
    shared::write_new(&directory.join("success.json"),&serde_json::to_vec_pretty(&json!({"probe_version":1,"native_natural_expiry_same_responses":true,"owned_producers_bodies_server_closed":true,"post_expiry_pages200201_AZ_without_Q":true,"headed_client_proof":false,"scope":"Representative native-only authored pages, not complete engine or platform acceptance"})).unwrap()).unwrap();
}
