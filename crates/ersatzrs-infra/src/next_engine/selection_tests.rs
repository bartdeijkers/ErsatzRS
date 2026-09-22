use super::*;
use chrono::{Duration, Utc};
use ersatzrs_domain::{ChannelSubtitleMode, PinnedStreamSelection};
use serde_json::Value;

pub(crate) async fn fixture_pool() -> DbPool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    for sql in [
        r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#,
        r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId") VALUES (1, 'Synthetic', 1, 1)"#,
        r#"INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path") VALUES (1, 1, 'media')"#,
        r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (10, 1, 0)"#,
        r#"INSERT INTO "Movie" ("Id") VALUES (10)"#,
        r#"INSERT INTO "MovieMetadata" ("Id", "MovieId", "Title", "MetadataKind", "DateAdded", "DateUpdated")
            VALUES (20, 10, 'Synthetic', 0, '2026-09-22 00:00:00', '2026-09-22 00:00:00')"#,
        r#"INSERT INTO "MediaVersion" ("Id", "DateAdded", "DateUpdated", "Duration", "MovieId", "Height", "Width", "VideoScanKind")
            VALUES (100, '2026-09-22 00:00:00', '2026-09-22 00:00:00', '00:01:00', 10, 180, 320, 0)"#,
        r#"INSERT INTO "MediaStream" ("Id", "Index", "Codec", "MediaStreamKind", "Language", "Channels", "Title", "Default", "Forced", "MediaVersionId") VALUES
            (1, 0, 'h264', 1, NULL, 0, NULL, 1, 0, 100),
            (2, 1, 'aac', 2, 'eng', 2, 'English', 1, 0, 100),
            (3, 2, 'aac', 2, 'nld', 2, 'Dutch', 0, 0, 100),
            (4, 3, 'hdmv_pgs_subtitle', 3, 'jpn', 0, 'Japanese', 0, 0, 100)"#,
        r#"INSERT INTO "Subtitle" ("Id", "MovieMetadataId", "SubtitleKind", "Title", "StreamIndex", "Codec", "Default", "Forced", "SDH", "Language", "IsExtracted", "Path", "DateAdded", "DateUpdated") VALUES
            (1, 20, 0, 'English', 4, 'subrip', 1, 0, 0, 'eng', 1, 'synthetic/english.srt', '2026-09-22 00:00:00', '2026-09-22 00:00:00'),
            (2, 20, 1, 'Dutch', 9, 'subrip', 1, 0, 0, 'nld', 0, 'fixture.nl.srt', '2026-09-22 00:00:00', '2026-09-22 00:00:00'),
            (3, 20, 0, 'Japanese', 3, 'hdmv_pgs_subtitle', 0, 0, 0, 'jpn', 0, NULL, '2026-09-22 00:00:00', '2026-09-22 00:00:00')"#,
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    pool
}

pub(crate) fn item() -> PlayoutItem {
    let start = Utc::now() - Duration::seconds(2);
    PlayoutItem {
        id: 42,
        media_item_id: 10,
        start,
        finish: start + Duration::seconds(60),
        in_point: Duration::seconds(1),
        out_point: Duration::seconds(61),
        ..Default::default()
    }
}

async fn project(
    pool: &DbPool,
    channel: &Channel,
    item: &PlayoutItem,
) -> Result<wire::PlayoutItem> {
    project_item(
        pool,
        channel,
        item,
        100,
        wire::Source::Local {
            path: "media/fixture.mkv".to_owned(),
            in_point_ms: None,
            out_point_ms: None,
        },
        &Resolution {
            width: 320,
            height: 180,
            ..Default::default()
        },
    )
    .await
}

#[tokio::test]
async fn item_preferences_and_relative_sidecars_preserve_indices_and_seek() {
    let pool = fixture_pool().await;
    let channel = Channel {
        preferred_audio_language_code: Some("eng".to_owned()),
        preferred_subtitle_language_code: Some("eng".to_owned()),
        subtitle_mode: ChannelSubtitleMode::Default,
        ..Default::default()
    };
    let item = PlayoutItem {
        preferred_audio_language_code: Some("nld".to_owned()),
        preferred_subtitle_language_code: Some("nld".to_owned()),
        ..item()
    };
    let projected = project(&pool, &channel, &item).await.unwrap();
    let value = serde_json::to_value(projected).unwrap();
    assert!(value.get("source").is_none());
    assert_eq!(value["tracks"]["audio"]["stream_index"], 2);
    assert_eq!(value["tracks"]["video"]["stream_index"], 0);
    assert_eq!(value["tracks"]["subtitle"]["stream_index"], 0);
    assert_eq!(
        value["tracks"]["subtitle"]["source"]["path"],
        std::path::Path::new("media")
            .join("fixture.nl.srt")
            .to_str()
            .unwrap()
    );
    for kind in ["video", "audio", "subtitle"] {
        assert_eq!(value["tracks"][kind]["source"]["in_point_ms"], 1000);
        assert_eq!(value["tracks"][kind]["source"]["out_point_ms"], 61_000);
    }
}

#[tokio::test]
async fn channel_preferences_override_global_defaults_and_extracted_paths_use_cache() {
    let pool = fixture_pool().await;
    sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, 'nld')"#)
        .bind(ersatzrs_domain::CONFIG_KEY_FFMPEG_PREFERRED_LANGUAGE_CODE)
        .execute(&pool)
        .await
        .unwrap();
    let mut channel = Channel::default();
    let global: Value =
        serde_json::to_value(project(&pool, &channel, &item()).await.unwrap()).unwrap();
    assert_eq!(global["tracks"]["audio"]["stream_index"], 2);
    channel.preferred_audio_language_code = Some("eng".to_owned());
    channel.preferred_subtitle_language_code = Some("eng".to_owned());
    channel.subtitle_mode = ChannelSubtitleMode::Default;
    let value = serde_json::to_value(project(&pool, &channel, &item()).await.unwrap()).unwrap();
    assert_eq!(value["tracks"]["audio"]["stream_index"], 1);
    let expected = crate::file_system_layout::subtitle_cache_folder().join("synthetic/english.srt");
    assert_eq!(
        value["tracks"]["subtitle"]["source"]["path"],
        expected.to_str().unwrap()
    );
}

#[tokio::test]
async fn pinned_silence_and_subtitles_off_never_restore_preferred_tracks() {
    let pool = fixture_pool().await;
    let channel = Channel {
        preferred_audio_language_code: Some("nld".to_owned()),
        subtitle_mode: ChannelSubtitleMode::Default,
        ..Default::default()
    };
    let item = PlayoutItem {
        pinned_stream_selection: Some(PinnedStreamSelection {
            media_version_id: 100,
            video_stream_index: 0,
            audio_stream_index: None,
            subtitle_stream_index: None,
        }),
        ..item()
    };
    let value = serde_json::to_value(project(&pool, &channel, &item).await.unwrap()).unwrap();
    assert!(value.get("source").is_none());
    assert!(value["tracks"].get("subtitle").is_none());
    let audio = &value["tracks"]["audio"]["source"];
    assert_eq!(audio["source_type"], "lavfi");
    assert_eq!(audio["probe_hint"]["duration_ms"], 60_000);
    assert_eq!(audio["probe_hint"]["audio"][0]["codec"], "pcm_s16le");
}

#[tokio::test]
async fn stale_pins_fail_without_selecting_another_version_or_stream() {
    let pool = fixture_pool().await;
    let mut item = item();
    let base = PinnedStreamSelection {
        media_version_id: 100,
        video_stream_index: 0,
        audio_stream_index: Some(1),
        subtitle_stream_index: Some(3),
    };
    item.pinned_stream_selection = Some(base);
    let good = project(&pool, &Channel::default(), &item).await.unwrap();
    assert_eq!(good.tracks.unwrap().subtitle.unwrap().stream_index, Some(3));
    for pin in [
        PinnedStreamSelection {
            media_version_id: 101,
            ..base
        },
        PinnedStreamSelection {
            video_stream_index: 77,
            ..base
        },
        PinnedStreamSelection {
            audio_stream_index: Some(77),
            ..base
        },
        PinnedStreamSelection {
            subtitle_stream_index: Some(77),
            ..base
        },
    ] {
        item.pinned_stream_selection = Some(pin);
        assert!(project(&pool, &Channel::default(), &item).await.is_err());
    }
}
