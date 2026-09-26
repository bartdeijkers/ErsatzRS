use super::*;
use crate::{db::DbPool, next_engine::schedule};

struct Fixture {
    _root: tempfile::TempDir,
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
            _root: root,
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
            replacements: &[],
            ffmpeg: Path::new("unused-ffmpeg"),
            deadline: Instant::now() + std::time::Duration::from_secs(10),
            diagnostics: None,
        }
    }
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
async fn troubleshooting_projects_every_selected_watermark_in_z_order() {
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
    let mut request = fixture.request(Some(0));
    request.watermark_ids = &[1, 2];
    let snapshot = fixture
        .host
        .troubleshooting_snapshot(&request)
        .await
        .unwrap();
    let item = serde_json::to_value(snapshot.projection.item).unwrap();
    assert!(item.get("watermark").is_none());
    assert_eq!(item["graphics"].as_array().unwrap().len(), 2);
    assert_eq!(item["graphics"][0]["opacity_percent"], 70.0);
    assert_eq!(item["graphics"][0]["timing"]["hold_ms"], 10_000);
    assert_eq!(item["graphics"][1]["opacity_percent"], 100.0);
    assert!(item["graphics"][1].get("timing").is_none());
    assert_eq!(item["tracks"]["audio"]["stream_index"], 2);
}
