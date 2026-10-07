use super::*;
use crate::{db::DbPool, next_engine::schedule};
use ersatzrs_core::repositories::PlayoutRepository as _;

struct Fixture {
    root: tempfile::TempDir,
    pool: DbPool,
    host: NextHost,
    media: ScheduledMedia,
    channel: Channel,
    profile: PlaybackProfile,
}

impl Fixture {
    async fn new() -> Self {
        let (pool, locker, mut channel, _) = schedule::tests::fixture().await;
        let root = tempfile::tempdir().unwrap();
        let media_path = root.path().join("fixture.mkv");
        let subtitle_path = root.path().join("fixture.nl.srt");
        tokio::fs::write(&media_path, b"synthetic source identity")
            .await
            .unwrap();
        tokio::fs::write(
            &subtitle_path,
            b"1\n00:00:00,000 --> 00:01:00,000\nFixture\n",
        )
        .await
        .unwrap();
        sqlx::query(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (1)"#)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(r#"UPDATE "MediaFile" SET "Path"=? WHERE "Id"=1"#)
            .bind(media_path.to_str().unwrap())
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(r#"UPDATE "Subtitle" SET "Path"=? WHERE "Id"=2"#)
            .bind(subtitle_path.to_str().unwrap())
            .execute(&pool)
            .await
            .unwrap();
        channel.preferred_audio_language_code = Some("nld".into());
        let media = schedule::bind_media(&pool, crate::next_engine::selection::tests::item())
            .await
            .unwrap();
        let profile = playback::load_playback_profile(&pool, 1)
            .await
            .unwrap()
            .unwrap();
        let host = NextHost::new(pool.clone(), locker, root.path().join("next"));
        Self {
            root,
            pool,
            host,
            media,
            channel,
            profile,
        }
    }

    fn request(&self, subtitle_id: Option<i64>) -> TroubleshootingPreview<'_> {
        TroubleshootingPreview {
            media: &self.media,
            channel: &self.channel,
            profile: &self.profile,
            subtitle_id,
            selection_time: None,
            watermark_ids: &[],
            graphics_ids: &[],
            replacements: &[],
            ffmpeg: Path::new("unused-ffmpeg"),
            deadline: Instant::now() + std::time::Duration::from_secs(10),
            diagnostics: None,
        }
    }

    fn start(&self) -> super::super::host::NextStart {
        super::super::host::NextStart {
            channel_id: self.channel.id,
            number: self.channel.number.clone(),
            preview: true,
            executable: "unused-next".into(),
            output: self.root.path().join("output"),
            ffmpeg: "unused-ffmpeg".into(),
            ffprobe: "unused-ffprobe".into(),
            replacements: vec![],
            idle_timeout: None,
            deadline: Instant::now() + std::time::Duration::from_secs(10),
        }
    }
}

#[tokio::test]
async fn troubleshooting_local_sessions_preserve_tracks_and_isolate_canvas_lifetimes() {
    use super::super::canvas::{CanvasBinding, CanvasRequest};
    let mut fixture = Fixture::new().await;
    fixture.media.item.id = 0;
    let before: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "PlayoutItem""#)
        .fetch_one(&fixture.pool)
        .await
        .unwrap();
    let mut sessions = Vec::new();
    for _ in 0..2 {
        let (instance, item) = fixture
            .host
            .prepare_troubleshooting_session(fixture.request(Some(2)), fixture.start())
            .await
            .unwrap();
        let json = serde_json::to_value(item).unwrap();
        assert_eq!(json["tracks"]["audio"]["stream_index"], 2);
        assert_eq!(json["tracks"]["subtitle"]["stream_index"], 0);
        assert_eq!(json["tracks"]["subtitle"]["source"]["in_point_ms"], 1000);
        assert_eq!(json["tracks"]["video"]["source"]["in_point_ms"], 1000);
        sessions.push(instance);
    }
    assert_ne!(sessions[0], sessions[1]);
    let binding = CanvasBinding::new(
        fixture.channel.id,
        &fixture.media,
        chrono::Utc::now() + chrono::TimeDelta::hours(1),
    )
    .for_session(sessions[0]);
    let timing = CanvasRequest::parse("0", "1000", "25").unwrap();
    assert!(fixture
        .host
        .canvas_session(sessions[0], &binding, &timing)
        .await
        .is_ok());
    assert!(fixture
        .host
        .canvas_session(sessions[1], &binding, &timing)
        .await
        .is_err());
    fixture
        .host
        .finish_troubleshooting_session(sessions[0])
        .await
        .unwrap();
    assert!(fixture
        .host
        .canvas_session(sessions[0], &binding, &timing)
        .await
        .is_err());
    let second_binding = binding.for_session(sessions[1]);
    assert!(fixture
        .host
        .canvas_session(sessions[1], &second_binding, &timing)
        .await
        .is_ok());
    fixture
        .host
        .finish_troubleshooting_session(sessions[1])
        .await
        .unwrap();
    assert!(fixture.host.dynamic_contexts.lock().await.is_empty());
    let after: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "PlayoutItem""#)
        .fetch_one(&fixture.pool)
        .await
        .unwrap();
    assert_eq!(before, after);
}

#[tokio::test]
async fn troubleshooting_local_failed_preparation_never_admits_a_session() {
    let fixture = Fixture::new().await;
    assert!(fixture
        .host
        .prepare_troubleshooting_session(fixture.request(Some(999)), fixture.start())
        .await
        .is_err());
    let mut expired = fixture.request(Some(0));
    expired.deadline = Instant::now() - std::time::Duration::from_secs(1);
    assert!(fixture
        .host
        .prepare_troubleshooting_session(expired, fixture.start())
        .await
        .is_err());
    let mut foreign = fixture.start();
    foreign.channel_id += 1;
    assert!(fixture
        .host
        .prepare_troubleshooting_session(fixture.request(Some(0)), foreign)
        .await
        .is_err());
    assert!(fixture.host.dynamic_contexts.lock().await.is_empty());
}

#[tokio::test]
async fn troubleshooting_custom_rules_see_only_explicit_subtitle_candidates() {
    use std::io::Write as _;
    let mut fixture = Fixture::new().await;
    let folder = crate::file_system_layout::channel_stream_selectors_folder();
    std::fs::create_dir_all(&folder).unwrap();
    let mut selector = tempfile::Builder::new()
        .prefix("ucu94-")
        .suffix(".yml")
        .tempfile_in(folder)
        .unwrap();
    selector.write_all(b"items:\n  - audio_language: [eng]\n    subtitle_language: [eng]\n  - audio_language: [nld]\n    subtitle_language: [nld]\n    subtitle_condition: forced\n  - audio_language: [eng]\n    disable_subtitles: true\n").unwrap();
    selector.flush().unwrap();
    fixture.channel.stream_selector_mode = ersatzrs_domain::ChannelStreamSelectorMode::Custom;
    fixture.channel.stream_selector = Some(
        selector
            .path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned(),
    );
    let log = super::super::diagnostics::SelectionLog::default();
    let mut request = fixture.request(Some(2));
    request.diagnostics = Some(log.clone());
    let snapshot = fixture
        .host
        .troubleshooting_snapshot(&request)
        .await
        .unwrap();
    let tracks = snapshot.projection.item.tracks.unwrap();
    assert_eq!(
        tracks.audio.unwrap().stream_index,
        Some(2),
        "filter candidates before selecting the audio rule"
    );
    let subtitle = snapshot.projection.selected_subtitle.unwrap();
    assert_eq!(subtitle.id, 2);
    assert!(
        !subtitle.forced,
        "forced matching must not change stored provenance"
    );
    let messages = log.text();
    assert!(messages.contains("audio stream 1, language=eng, matched=true"));
    assert!(messages.contains("subtitle 2, language=nld, matched=false"));
    assert!(messages.contains("Custom selector rule 1: audio matched=true, subtitle matched=false"));
    assert!(messages.contains("Custom selector rule 2: audio matched=true, subtitle matched=true"));
    assert!(messages.contains("audio stream=Some(2), subtitle id=Some(2)"));
    let off = fixture
        .host
        .troubleshooting_snapshot(&fixture.request(Some(0)))
        .await
        .unwrap();
    assert!(off.projection.selected_subtitle.is_none());
    assert_eq!(
        off.projection
            .item
            .tracks
            .unwrap()
            .audio
            .unwrap()
            .stream_index,
        Some(1)
    );
}

#[tokio::test]
async fn troubleshooting_media_context_decodes_the_persisted_duration() {
    use ersatzrs_core::repositories::TroubleshootingRepository as _;
    let fixture = Fixture::new().await;
    let repositories = crate::repositories::make_repositories(fixture.pool.clone());
    for (duration, expected) in [("00:01:00", 60.0), ("00:01:00.125", 60.125)] {
        sqlx::query(r#"UPDATE "MediaVersion" SET "Duration"=? WHERE "Id"=100"#)
            .bind(duration)
            .execute(&fixture.pool)
            .await
            .unwrap();
        let media = repositories
            .troubleshooting
            .load_preview_media_admin(10, None)
            .await
            .unwrap()
            .unwrap();
        assert!((media.duration_seconds - expected).abs() < f64::EPSILON);
    }
}

#[tokio::test]
async fn troubleshooting_preserves_explicit_tracks_and_seek_without_persisting_a_playout() {
    let fixture = Fixture::new().await;
    let snapshot = fixture
        .host
        .troubleshooting_snapshot(&fixture.request(Some(2)))
        .await
        .unwrap();
    let stored = playback::load_subtitles_for_media_item(&fixture.pool, 10)
        .await
        .unwrap()
        .into_iter()
        .find(|subtitle| subtitle.id == 2)
        .unwrap();
    assert_eq!(snapshot.projection.selected_subtitle, Some(stored));
    let count_before: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "PlayoutItem""#)
        .fetch_one(&fixture.pool)
        .await
        .unwrap();
    let projected = fixture
        .host
        .prepare_troubleshooting(fixture.request(Some(2)))
        .await
        .unwrap();
    let json = serde_json::to_value(projected).unwrap();
    assert_eq!(json["tracks"]["audio"]["stream_index"], 2);
    assert_eq!(json["tracks"]["subtitle"]["stream_index"], 0);
    assert_eq!(json["tracks"]["subtitle"]["source"]["in_point_ms"], 1000);
    assert_eq!(json["tracks"]["video"]["source"]["in_point_ms"], 1000);
    assert!(json["tracks"]["subtitle"]["source"]["path"]
        .as_str()
        .unwrap()
        .ends_with("fixture.nl.srt"));
    let off = fixture
        .host
        .prepare_troubleshooting(fixture.request(Some(0)))
        .await
        .unwrap();
    assert!(off.tracks.unwrap().subtitle.is_none());
    let count_after: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "PlayoutItem""#)
        .fetch_one(&fixture.pool)
        .await
        .unwrap();
    assert_eq!(count_before, count_after);
}

#[tokio::test]
async fn troubleshooting_rejects_stale_subtitle_version_and_expired_deadline() {
    let fixture = Fixture::new().await;
    sqlx::query(r#"DELETE FROM "MediaStream" WHERE "MediaVersionId"=100 AND "Index"=3"#)
        .execute(&fixture.pool)
        .await
        .unwrap();
    for id in [-1, 3, 999] {
        assert!(fixture
            .host
            .prepare_troubleshooting(fixture.request(Some(id)))
            .await
            .is_err());
    }
    let mut request = fixture.request(Some(0));
    request.deadline = Instant::now();
    assert!(fixture.host.prepare_troubleshooting(request).await.is_err());
}

#[tokio::test]
async fn troubleshooting_fetches_selected_provider_text_and_refreshes_changed_content() {
    use ersatzrs_core::repositories::RemoteMediaSourceKind;
    let fixture = Fixture::new().await;
    let server = httpmock::MockServer::start_async().await;
    sqlx::query(r#"DELETE FROM "LocalLibrary""#)
        .execute(&fixture.pool)
        .await
        .unwrap();
    crate::next_engine::source::tests::seed_provider(
        &fixture.pool,
        RemoteMediaSourceKind::Jellyfin,
    )
    .await;
    sqlx::query(r#"UPDATE "JellyfinConnection" SET "Address"=? WHERE "Id"=1"#)
        .bind(server.base_url())
        .execute(&fixture.pool)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, '<api-key>')"#)
        .bind(format!("jellyfin.api_key.{}", server.base_url()))
        .execute(&fixture.pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "Subtitle" SET "Path"=NULL WHERE "Id"=2"#)
        .execute(&fixture.pool)
        .await
        .unwrap();
    let mut previous = None;
    for cue in ["FIRST", "SECOND"] {
        let body = format!("1\n00:00:00,000 --> 00:01:00,000\n{cue}\n");
        let download = server
            .mock_async(|when, then| {
                when.method(httpmock::Method::GET)
                    .path("/Videos/fixture-item/fixture-item/Subtitles/9/9/Stream.srt")
                    .header("Authorization", "MediaBrowser Token=\"<api-key>\"");
                then.status(200).body(&body);
            })
            .await;
        let item = fixture
            .host
            .prepare_troubleshooting(fixture.request(Some(2)))
            .await
            .unwrap();
        let json = serde_json::to_value(item).unwrap();
        let path = json["tracks"]["subtitle"]["source"]["path"]
            .as_str()
            .unwrap();
        assert_eq!(tokio::fs::read_to_string(path).await.unwrap(), body);
        assert_eq!(json["tracks"]["subtitle"]["source"]["in_point_ms"], 1000);
        assert_eq!(json["tracks"]["audio"]["stream_index"], 2);
        assert!(!json.to_string().contains("<api-key>"));
        if let Some(previous) = previous.as_ref() {
            assert_ne!(
                path, previous,
                "changed provider text must use a new artifact"
            );
        }
        previous = Some(path.to_owned());
        download.assert_calls_async(1).await;
        download.delete_async().await;
    }
}

#[tokio::test]
async fn troubleshooting_publishes_one_canvas_for_all_selected_graphics() {
    use ersatzrs_core::repositories::GraphicsRepository as _;
    use ersatzrs_domain::{GraphicsElement, GraphicsElementConfig, ImageGraphicsConfig};
    let fixture = Fixture::new().await;
    sqlx::raw_sql(
        r#"INSERT INTO "ChannelWatermark" (
        "Id","Name","Mode","ImageSource","Location","Size","WidthPercent",
        "HorizontalMarginPercent","VerticalMarginPercent","FrequencyMinutes",
        "DurationSeconds","Opacity","PlaceWithinSourceContent","ZIndex") VALUES
        (1,'Upper',1,1,0,0,20,5,5,0,0,100,0,10),
        (2,'Lower',2,1,8,0,30,5,5,1,10,70,0,-10);
        INSERT INTO "Artwork" ("ArtworkKind","ChannelId","Path","DateAdded","DateUpdated")
        VALUES (2,1,'https://example.test/logo.png','2026-09-26','2026-09-26');"#,
    )
    .execute(&fixture.pool)
    .await
    .unwrap();
    let graphics = crate::repositories::SqliteGraphicsRepository(fixture.pool.clone());
    let mut image = GraphicsElement {
        name: "Templated image".into(),
        z_index: -10,
        scale: true,
        config: GraphicsElementConfig::Image(ImageGraphicsConfig {
            image: Some("https://example.test/{{ Channel_Number }}.png".into()),
        }),
        ..GraphicsElement::default()
    };
    let image_id = graphics.create_element(&image).await.unwrap();
    let mut request = fixture.request(Some(0));
    request.watermark_ids = &[1, 2];
    let ids = [image_id];
    request.graphics_ids = &ids;
    let snapshot = fixture
        .host
        .troubleshooting_snapshot(&request)
        .await
        .unwrap();
    let identity = snapshot.identity.descriptor;
    assert!(snapshot.projection.canvas.is_some());
    fixture
        .host
        .media
        .configure("127.0.0.1:18419".parse().unwrap())
        .unwrap();
    let (instance, projected) = fixture
        .host
        .prepare_troubleshooting_session(request, fixture.start())
        .await
        .unwrap();
    let item = serde_json::to_value(projected).unwrap();
    assert!(item.get("watermark").is_none());
    assert_eq!(item["tracks"]["audio"]["stream_index"], 2);
    assert_eq!(item["graphics"].as_array().unwrap().len(), 1);
    let canvas = &item["graphics"][0];
    assert_eq!(canvas["kind"], "canvas");
    assert_eq!(canvas["source"]["probe_hint"]["format_name"], "nut");
    assert_eq!(canvas["source"]["probe_hint"]["video"][0]["codec"], "ffv1");
    assert_eq!(
        canvas["source"]["probe_hint"]["video"][0]["pix_fmt"],
        "bgra"
    );
    assert!(canvas.get("opacity_percent").is_none());
    let url = url::Url::parse(canvas["source"]["uri"].as_str().unwrap()).unwrap();
    let binding: super::super::canvas::CanvasBinding = serde_json::from_str(
        &url.query_pairs()
            .find(|(key, _)| key == "binding")
            .unwrap()
            .1,
    )
    .unwrap();
    assert_eq!(binding.session, Some(instance));
    let mut request = fixture.request(Some(0));
    request.watermark_ids = &[1, 2];
    request.graphics_ids = &ids;
    image.id = image_id;
    image.opacity_percent = Some(45);
    graphics.upsert_element(&image).await.unwrap();
    let changed = fixture
        .host
        .troubleshooting_snapshot(&request)
        .await
        .unwrap();
    assert_eq!(
        identity, changed.identity.descriptor,
        "current graphics are read by the callback without invalidating track preparation"
    );
    let timing = super::super::canvas::CanvasRequest::parse("0", "1000", "25").unwrap();
    let selected = fixture
        .host
        .canvas_session(instance, &binding, &timing)
        .await
        .unwrap();
    assert_eq!(selected.graphics[0].element.opacity_percent, Some(45));
    assert_eq!(selected.watermarks.len(), 2);
    fixture
        .host
        .finish_troubleshooting_session(instance)
        .await
        .unwrap();
}

fn channel_occurrence(
    fixture: &Fixture,
    id: i64,
    start: chrono::DateTime<chrono::Utc>,
    finish: chrono::DateTime<chrono::Utc>,
) -> TroubleshootingChannelOccurrence {
    let mut media = fixture.media.clone();
    media.item.id = id;
    media.item.playout_id = 1;
    media.item.start = start;
    media.item.finish = finish;
    media.item.in_point = chrono::TimeDelta::seconds(1);
    media.item.out_point = media.item.in_point + (finish - start);
    TroubleshootingChannelOccurrence {
        media,
        channel: fixture.channel.clone(),
        subtitle_id: Some(0),
        credits: None,
    }
}

async fn channel_manifest(
    fixture: &Fixture,
    occurrences: &[TroubleshootingChannelOccurrence],
    start: chrono::DateTime<chrono::Utc>,
) -> (uuid::Uuid, protocol::Playout) {
    let repositories = crate::repositories::make_repositories(fixture.pool.clone());
    for occurrence in occurrences {
        repositories
            .playout
            .upsert_item(&occurrence.media.item)
            .await
            .unwrap();
    }
    let finish = start + chrono::TimeDelta::seconds(30);
    let before = persisted_window_state(&fixture.pool, start, finish).await;
    let prepared = fixture
        .host
        .prepare_troubleshooting_window(
            TroubleshootingChannelWindow {
                channel: &fixture.channel,
                profile: &fixture.profile,
                occurrences,
                start,
                finish,
                replacements: &[],
                ffmpeg: Path::new("unused-ffmpeg"),
                deadline: Instant::now() + std::time::Duration::from_secs(10),
                diagnostics: None,
            },
            fixture.start(),
        )
        .await
        .unwrap();
    assert_eq!(
        before,
        persisted_window_state(&fixture.pool, start, finish).await,
        "window preparation must preserve scheduled rows and their exact state"
    );
    prepared
}

pub(super) async fn persisted_window_state(
    pool: &DbPool,
    start: chrono::DateTime<chrono::Utc>,
    finish: chrono::DateTime<chrono::Utc>,
) -> (i64, Vec<ersatzrs_domain::PlayoutItem>) {
    let count = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "PlayoutItem""#)
        .fetch_one(pool)
        .await
        .unwrap();
    let items = crate::repositories::make_repositories(pool.clone())
        .playout
        .items_in_range(1, start, finish)
        .await
        .unwrap();
    (count, items)
}

#[tokio::test]
async fn troubleshooting_channel_window_preserves_overlap_and_fills_every_outer_gap() {
    let fixture = Fixture::new().await;
    fixture
        .host
        .media
        .configure("127.0.0.1:8409".parse().unwrap())
        .unwrap();
    let start =
        chrono::DateTime::from_timestamp(chrono::Utc::now().timestamp() - 2 * 86_400, 0).unwrap();
    let occurrences = [
        channel_occurrence(
            &fixture,
            11,
            start + chrono::TimeDelta::seconds(3),
            start + chrono::TimeDelta::seconds(8),
        ),
        channel_occurrence(
            &fixture,
            12,
            start + chrono::TimeDelta::seconds(6),
            start + chrono::TimeDelta::seconds(9),
        ),
        channel_occurrence(
            &fixture,
            13,
            start + chrono::TimeDelta::seconds(20),
            start + chrono::TimeDelta::seconds(25),
        ),
    ];
    let (instance, playout) = channel_manifest(&fixture, &occurrences, start).await;
    let bounds: Vec<_> = playout
        .items
        .iter()
        .map(|item| {
            (
                (item.start - start).num_seconds(),
                (item.finish - start).num_seconds(),
            )
        })
        .collect();
    assert_eq!(
        bounds,
        vec![(0, 3), (3, 8), (6, 9), (9, 20), (20, 25), (25, 30)]
    );
    for item in playout
        .items
        .iter()
        .filter(|item| !item.id.starts_with("fallback-"))
    {
        assert_eq!(
            serde_json::to_value(item).unwrap()["tracks"]["audio"]["stream_index"],
            2
        );
    }
    fixture
        .host
        .finish_troubleshooting_session(instance)
        .await
        .unwrap();
    let fixture = Fixture::new().await;
    fixture
        .host
        .media
        .configure("127.0.0.1:8409".parse().unwrap())
        .unwrap();
    let (instance, empty) = channel_manifest(&fixture, &[], start).await;
    assert_eq!(empty.items.len(), 1);
    assert_eq!(
        (empty.items[0].start, empty.items[0].finish),
        (start, start + chrono::TimeDelta::seconds(30))
    );
    fixture
        .host
        .finish_troubleshooting_session(instance)
        .await
        .unwrap();
}

#[tokio::test]
async fn troubleshooting_window_second_occurrence_canvas_is_bound_and_revoked() {
    use super::super::canvas::{CanvasBinding, CanvasRequest};
    let fixture = Fixture::new().await;
    fixture
        .host
        .media
        .configure("127.0.0.1:8409".parse().unwrap())
        .unwrap();
    let start =
        chrono::DateTime::from_timestamp(chrono::Utc::now().timestamp() - 2 * 86_400, 0).unwrap();
    let occurrences = [
        channel_occurrence(&fixture, 11, start, start + chrono::TimeDelta::seconds(10)),
        channel_occurrence(
            &fixture,
            12,
            start + chrono::TimeDelta::seconds(10),
            start + chrono::TimeDelta::seconds(30),
        ),
    ];
    let (first, _) = channel_manifest(&fixture, &occurrences, start).await;
    let (other, _) = channel_manifest(&fixture, &occurrences, start).await;
    let binding = CanvasBinding::new(
        fixture.channel.id,
        &occurrences[1].media,
        callback_expiry(occurrences[1].media.item.finish).unwrap(),
    )
    .for_session(first);
    let timing = CanvasRequest::parse("0", "1000", "25").unwrap();
    let selected = fixture
        .host
        .canvas_session(first, &binding, &timing)
        .await
        .unwrap();
    assert_eq!(selected.media.item.id, 12);
    assert_eq!(
        selected.media.media_version_id,
        occurrences[1].media.media_version_id
    );
    assert!(fixture
        .host
        .canvas_session(other, &binding, &timing)
        .await
        .is_err());
    let mut wrong = binding.clone();
    wrong.item = 99;
    assert!(fixture
        .host
        .canvas_session(first, &wrong, &timing)
        .await
        .is_err());
    fixture
        .host
        .finish_troubleshooting_session(first)
        .await
        .unwrap();
    assert!(fixture
        .host
        .canvas_session(first, &binding, &timing)
        .await
        .is_err());
    assert!(fixture
        .host
        .canvas_session(other, &binding.for_session(other), &timing)
        .await
        .is_ok());
    fixture
        .host
        .finish_troubleshooting_session(other)
        .await
        .unwrap();
}

#[test]
fn troubleshooting_historical_callback_expiry_is_future_without_removing_expiration() {
    let now = chrono::Utc::now();
    let historical = now - chrono::TimeDelta::days(3);
    let expiry = callback_expiry(historical).unwrap();
    assert!(expiry >= now + chrono::TimeDelta::hours(2));
    assert!(expiry < now + chrono::TimeDelta::hours(2) + chrono::TimeDelta::seconds(2));
    let future = now + chrono::TimeDelta::hours(4);
    assert_eq!(
        callback_expiry(future).unwrap(),
        future + chrono::TimeDelta::hours(2)
    );
}

#[tokio::test]
async fn troubleshooting_empty_window_fallback_uses_preview_profile_and_requires_its_session() {
    use ersatzrs_core::repositories::ChannelRepository as _;
    let mut fixture = Fixture::new().await;
    fixture
        .host
        .media
        .configure("127.0.0.1:8409".parse().unwrap())
        .unwrap();
    sqlx::query(r#"INSERT INTO "FillerPreset" ("Id","AllowWatermarks","CollectionType","MediaItemId","FillerKind","FillerMode","UseChaptersAsMediaItems") VALUES (1,0,10,10,4,0,0)"#).execute(&fixture.pool).await.unwrap();
    fixture.channel.fallback_filler_id = Some(1);
    fixture.channel.subtitle_mode = ersatzrs_domain::ChannelSubtitleMode::None;
    // A stored Native channel explicitly previewed by Next must still work.
    let mut stored = fixture.channel.clone();
    stored.streaming_engine = ersatzrs_domain::StreamingEngine::Legacy;
    crate::repositories::make_repositories(fixture.pool.clone())
        .channel
        .upsert(&stored)
        .await
        .unwrap();
    fixture.profile.resolution.width = 640;
    fixture.profile.resolution.height = 360;
    let start = chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap();
    let (instance, playout) = channel_manifest(&fixture, &[], start).await;
    let item = &playout.items[0];
    let ticks = |t: chrono::DateTime<chrono::Utc>| {
        t.timestamp() * 10_000_000 + i64::from(t.timestamp_subsec_nanos() / 100)
    };
    let resolved = fixture
        .host
        .resolve_fallback(
            fixture.channel.id,
            ticks(item.start),
            ticks(item.finish),
            &fixture.channel.number,
            instance,
            &item.id,
            start,
            item.finish,
        )
        .await
        .unwrap();
    assert_eq!(resolved.start, start);
    assert!(resolved.finish <= item.finish);
    let context = fixture.host.preview_session(instance).await.unwrap();
    assert_eq!(context.profile.resolution.width, 640);
    fixture
        .host
        .finish_troubleshooting_session(instance)
        .await
        .unwrap();
    assert!(fixture
        .host
        .resolve_fallback(
            fixture.channel.id,
            ticks(item.start),
            ticks(item.finish),
            &fixture.channel.number,
            instance,
            &item.id,
            start,
            item.finish
        )
        .await
        .is_err());
}

#[tokio::test]
async fn troubleshooting_generated_credits_keep_timing_through_fresh_track_materialization() {
    let fixture = Fixture::new().await;
    let mut request = fixture.request(None);
    let mut channel = fixture.channel.clone();
    channel.subtitle_mode = ersatzrs_domain::ChannelSubtitleMode::Default;
    channel.music_video_credits_mode =
        ersatzrs_domain::ChannelMusicVideoCreditsMode::GenerateSubtitles;
    request.channel = &channel;
    let mut context = Context::new(&request);
    context.media.item.pinned_stream_selection = None;
    context.credits = Some(protocol::Track {
        stream_index: Some(0),
        source: Some(protocol::Source::Local {
            path: "owned-preview/generated-credits.ass".into(),
            in_point_ms: Some(0),
            out_point_ms: None,
            probe_hint: None,
        }),
    });
    let mut projection = fixture
        .host
        .troubleshooting_snapshot(&request)
        .await
        .unwrap()
        .projection;
    context.attach_credits(&mut projection).unwrap();
    let origin =
        super::super::remote_projection::Origin::Troubleshooting(std::sync::Arc::new(context));
    let now = projection.item.start + chrono::TimeDelta::seconds(2);
    assert_replaced_caption_does_not_resurrect_credits(&origin, &mut projection, now);
    let credits = origin.credits_at(&projection, now).unwrap().unwrap();
    let protocol::Source::Local { in_point_ms, .. } = credits.source.as_ref().unwrap() else {
        panic!("generated caption");
    };
    assert_eq!(*in_point_ms, Some(2000));
    let previous =
        playback::load_source_audio_streams(&fixture.pool, fixture.media.media_version_id)
            .await
            .unwrap();
    let mut probe = super::super::probe_hint::tests::metadata();
    probe.streams[1].language = Some("nld".into());
    probe.streams[1].title = Some("Dutch".into());
    let mut materialized = super::super::retained_host::install_fresh_tracks(
        projection.item,
        &probe,
        &previous,
        Some(2),
        protocol::Source::Local {
            path: "prepared.nut".into(),
            in_point_ms: Some(0),
            out_point_ms: None,
            probe_hint: None,
        },
        None,
        now,
    )
    .unwrap();
    assert_eq!(
        materialized
            .tracks
            .as_ref()
            .unwrap()
            .audio
            .as_ref()
            .unwrap()
            .stream_index,
        Some(23)
    );
    assert!(materialized.tracks.as_ref().unwrap().subtitle.is_none());
    super::super::retained_host::install_generated_credits(&mut materialized, Some(credits))
        .unwrap();
    let protocol::Source::Local {
        path, in_point_ms, ..
    } = materialized
        .tracks
        .as_ref()
        .unwrap()
        .subtitle
        .as_ref()
        .unwrap()
        .source
        .as_ref()
        .unwrap()
    else {
        panic!("restored caption");
    };
    assert_eq!(path, "owned-preview/generated-credits.ass");
    assert_eq!(*in_point_ms, Some(2000));
}

fn assert_replaced_caption_does_not_resurrect_credits(
    origin: &super::super::remote_projection::Origin,
    projection: &mut super::super::selection::DeferredProjection,
    now: chrono::DateTime<chrono::Utc>,
) {
    // A removed/expired natural Custom caption must not resurrect the Context's
    // generated overlay after natural caption preparation clears its selection.
    let original = projection
        .item
        .tracks
        .as_mut()
        .unwrap()
        .subtitle
        .as_mut()
        .unwrap()
        .source
        .replace(protocol::Source::Local {
            path: "owned-preview/selected-dutch.ass".into(),
            in_point_ms: Some(0),
            out_point_ms: None,
            probe_hint: None,
        });
    assert!(projection.selected_subtitle.is_none());
    assert!(origin.credits_at(projection, now).unwrap().is_none());
    projection
        .item
        .tracks
        .as_mut()
        .unwrap()
        .subtitle
        .as_mut()
        .unwrap()
        .source = original;
}
