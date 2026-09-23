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
            (4, 3, 'hdmv_pgs_subtitle', 3, 'jpn', 0, 'Japanese', 0, 0, 100),
            (5, 4, 'subrip', 3, 'eng', 0, 'English', 1, 0, 100)"#,
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
async fn channel_preferences_override_global_defaults_and_burn_uses_extracted_sidecar() {
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
    assert_eq!(
        value["tracks"]["subtitle"]["source"]["path"],
        crate::file_system_layout::subtitle_cache_folder()
            .join("synthetic/english.srt")
            .to_str()
            .unwrap()
    );
    assert_eq!(value["tracks"]["subtitle"]["stream_index"], 0);
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
async fn http_sources_retain_transport_options_when_audio_is_missing() {
    let pool = fixture_pool().await;
    let channel = Channel {
        preferred_audio_language_code: Some("nld".to_owned()),
        preferred_subtitle_language_code: Some("jpn".to_owned()),
        subtitle_mode: ChannelSubtitleMode::Default,
        ..Default::default()
    };
    let source = wire::Source::Http {
        uri: "https://media.example.test/video.mkv".to_owned(),
        headers: vec!["X-Fixture: synthetic".to_owned()],
        user_agent: Some("ErsatzRS-fixture".to_owned()),
        keep_alive: None,
        reconnect: Some(true),
        reconnect_delay_max: Some(2),
        timeout_us: Some(5_000_000),
        in_point_ms: None,
        out_point_ms: None,
    };
    let mut expected = serde_json::to_value(&source).unwrap();
    expected["in_point_ms"] = 1000.into();
    expected["out_point_ms"] = 61_000.into();
    for has_audio in [true, false] {
        if !has_audio {
            sqlx::query(r#"DELETE FROM "MediaStream" WHERE "MediaStreamKind" = 2"#)
                .execute(&pool)
                .await
                .unwrap();
        }
        let projected = project_item(
            &pool,
            &channel,
            &item(),
            100,
            source.clone(),
            &Resolution {
                width: 320,
                height: 180,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let value = serde_json::to_value(projected).unwrap();
        assert!(value.get("source").is_none());
        assert_eq!(value["tracks"]["video"]["source"], expected);
        assert_eq!(value["tracks"]["subtitle"]["source"], expected);
        assert_eq!(value["tracks"]["subtitle"]["stream_index"], 3);
        if has_audio {
            assert_eq!(value["tracks"]["audio"]["source"], expected);
            assert_eq!(value["tracks"]["audio"]["stream_index"], 2);
        } else {
            let audio = &value["tracks"]["audio"]["source"];
            assert_eq!(audio["source_type"], "lavfi");
            assert_eq!(audio["probe_hint"]["duration_ms"], 60_000);
        }
    }
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

#[tokio::test]
async fn item_audio_title_overrides_channel_title_in_published_tracks() {
    let pool = fixture_pool().await;
    sqlx::query(r#"UPDATE "MediaStream" SET "Language"='eng' WHERE "MediaStreamKind"=2"#)
        .execute(&pool)
        .await
        .unwrap();
    let channel = Channel {
        preferred_audio_language_code: Some("eng".into()),
        preferred_audio_title: Some("English".into()),
        ..Default::default()
    };
    let mut item = item();
    let channel_choice =
        serde_json::to_value(project(&pool, &channel, &item).await.unwrap()).unwrap();
    assert_eq!(channel_choice["tracks"]["audio"]["stream_index"], 1);
    item.preferred_audio_title = Some("Dutch".into());
    let item_choice = serde_json::to_value(project(&pool, &channel, &item).await.unwrap()).unwrap();
    assert_eq!(item_choice["tracks"]["audio"]["stream_index"], 2);
}

#[tokio::test]
async fn custom_audio_selection_precedes_preferences_but_never_explicit_pins() {
    let pool = fixture_pool().await;
    let channel = Channel {
        stream_selector_mode: ChannelStreamSelectorMode::Custom,
        preferred_audio_language_code: Some("nld".into()),
        ..Default::default()
    };
    let mut item = item();
    let streams = load_source_audio_streams(&pool, 100).await.unwrap();
    let preferences = playback_language_preferences(&pool).await.unwrap();
    let custom = CustomStreamSelection {
        audio_stream_index: Some(1),
        subtitle: None,
    };
    assert_eq!(
        selected_audio(&item, &channel, &preferences, Some(&custom), &streams).unwrap(),
        Some(1)
    );
    let no_match = CustomStreamSelection {
        audio_stream_index: None,
        subtitle: None,
    };
    assert_eq!(
        selected_audio(&item, &channel, &preferences, Some(&no_match), &streams).unwrap(),
        Some(2)
    );
    // Missing custom files must also fall back during actual item projection.
    let published = serde_json::to_value(project(&pool, &channel, &item).await.unwrap()).unwrap();
    assert_eq!(published["tracks"]["audio"]["stream_index"], 2);
    item.pinned_stream_selection = Some(PinnedStreamSelection {
        media_version_id: 100,
        video_stream_index: 0,
        audio_stream_index: Some(2),
        subtitle_stream_index: None,
    });
    assert_eq!(
        selected_audio(&item, &channel, &preferences, Some(&custom), &streams).unwrap(),
        Some(2)
    );
}

// Called only by the isolated real-engine child, whose data/state directories
// are explicitly redirected. Never mutate process environment in async tests.
pub(crate) async fn assert_javascript_precedence() {
    use ersatzrs_domain::ChannelStreamSelectorMode;
    let pool = fixture_pool().await;
    let mut channel = Channel {
        preferred_audio_language_code: Some("eng".into()),
        ..Default::default()
    };
    assert_direct_audio_policy(&pool, &channel).await;
    let mut item = item();
    let projected = project(&pool, &channel, &item).await.unwrap();
    assert_eq!(
        projected.tracks.unwrap().audio.unwrap().stream_index,
        Some(2)
    );

    let folder = crate::file_system_layout::channel_stream_selectors_folder();
    tokio::fs::create_dir_all(&folder).await.unwrap();
    tokio::fs::write(
        folder.join("fixture.yml"),
        "items:\n  - audio_language: [eng]\n",
    )
    .await
    .unwrap();
    channel.stream_selector_mode = ChannelStreamSelectorMode::Custom;
    channel.stream_selector = Some("fixture.yml".into());
    let projected = project(&pool, &channel, &item).await.unwrap();
    assert_eq!(
        projected.tracks.unwrap().audio.unwrap().stream_index,
        Some(1),
        "YAML must precede JavaScript"
    );

    item.pinned_stream_selection = Some(PinnedStreamSelection {
        media_version_id: 100,
        video_stream_index: 0,
        audio_stream_index: Some(2),
        subtitle_stream_index: None,
    });
    let projected = project(&pool, &channel, &item).await.unwrap();
    assert_eq!(
        projected.tracks.unwrap().audio.unwrap().stream_index,
        Some(2),
        "pins must precede YAML"
    );
    item.pinned_stream_selection
        .as_mut()
        .unwrap()
        .audio_stream_index = None;
    let projected = project(&pool, &channel, &item).await.unwrap();
    assert!(
        matches!(
            projected.tracks.unwrap().audio.unwrap().source,
            Some(wire::Source::Lavfi { .. })
        ),
        "pinned silence must precede all selectors"
    );
    item.pinned_stream_selection = None;
    channel.stream_selector = None;
    let script_path =
        crate::file_system_layout::scripts_folder().join("audio-stream-selector/movie.js");
    let original = tokio::fs::read(&script_path).await.unwrap();
    // Real JavaScript exposes clock/random APIs, but a published choice must
    // survive callbacks without evaluating the selector a second time.
    tokio::fs::write(&script_path,
        "function selectMovieAudioStreamIndex(){if(typeof Date.now !== 'function' || typeof Math.random !== 'function') throw Error('intrinsics');return 2}")
        .await.unwrap();
    assert_published_audio_bypasses_script(&pool, &channel, &item).await;
    for script in [
        "function selectMovieAudioStreamIndex(){return null}",
        "function selectMovieAudioStreamIndex(){throw Error('fixture')}",
        "function selectMovieAudioStreamIndex(){return 0}",
        // A finite wait keeps this regression bounded even if a dependency
        // accidentally enables blocking Atomics in a future update.
        "function selectMovieAudioStreamIndex(){Atomics.wait(new Int32Array(new SharedArrayBuffer(4)),0,0,2000);return 2}",
    ] {
        tokio::fs::write(&script_path, script).await.unwrap();
        let projected = project(&pool, &channel, &item).await.unwrap();
        assert_eq!(
            projected.tracks.unwrap().audio.unwrap().stream_index,
            Some(1),
            "failed or invalid callbacks must use language fallback"
        );
    }
    tokio::fs::write(&script_path, original).await.unwrap();
}

async fn assert_published_audio_bypasses_script(
    pool: &DbPool,
    channel: &Channel,
    item: &PlayoutItem,
) {
    let ordinary = project(pool, channel, item).await.unwrap();
    assert_eq!(
        ordinary.tracks.unwrap().audio.unwrap().stream_index,
        Some(2)
    );
    let overridden = project_deferred_item_with_audio(
        pool,
        channel,
        item,
        100,
        wire::Source::Local {
            path: "media/fixture.mkv".into(),
            in_point_ms: None,
            out_point_ms: None,
        },
        &Resolution::default(),
        AudioDirective::Published(Some(1)),
    )
    .await
    .unwrap();
    assert_eq!(
        overridden.item.tracks.unwrap().audio.unwrap().stream_index,
        Some(1)
    );
}

async fn assert_direct_audio_policy(pool: &DbPool, channel: &Channel) {
    use crate::audio_script::{select, AudioScriptPreferences};
    let audio = load_source_audio_streams(pool, 100).await.unwrap();
    for (language, title, mode, expected) in [
        (None, None, StreamingMode::HttpLiveStreamingDirect, None),
        (
            Some(" "),
            Some(" "),
            StreamingMode::HttpLiveStreamingDirect,
            None,
        ),
        (
            Some("eng"),
            None,
            StreamingMode::HttpLiveStreamingDirect,
            Some(2),
        ),
        (
            Some("und"),
            None,
            StreamingMode::HttpLiveStreamingDirect,
            Some(2),
        ),
        (
            None,
            Some("English"),
            StreamingMode::HttpLiveStreamingDirect,
            Some(2),
        ),
        (
            None,
            None,
            StreamingMode::HttpLiveStreamingSegmenter,
            Some(2),
        ),
    ] {
        assert_eq!(
            select(
                pool,
                channel,
                10,
                100,
                AudioScriptPreferences {
                    language,
                    title,
                    streaming_mode: mode,
                },
                &audio
            )
            .await,
            expected,
            "raw HLS Direct must retain all audio until an effective preference selects one track"
        );
    }
}

async fn add_stale_image_subtitle_candidates(pool: &DbPool) {
    // A second metadata record/version advertises indices absent from or
    // semantically different in the resolved file. Neither may outrank index 3.
    for sql in [
        r#"UPDATE "MediaStream" SET "Index" = 8 WHERE "MediaVersionId" = 100 AND "Index" = 2"#,
        r#"INSERT INTO "MovieMetadata" ("Id", "MovieId", "Title", "MetadataKind", "DateAdded", "DateUpdated")
            VALUES (21, 10, 'Other metadata', 0, '2026-09-22', '2026-09-22')"#,
        r#"INSERT INTO "MediaVersion" ("Id", "MovieId", "DateAdded", "DateUpdated", "Duration", "Height", "Width", "VideoScanKind")
            VALUES (101, 10, '2026-09-22', '2026-09-22', '00:01:00', 180, 320, 0)"#,
        r#"INSERT INTO "MediaStream" ("Index", "Codec", "MediaStreamKind", "Language", "Channels", "Title", "Default", "Forced", "MediaVersionId")
            VALUES (0, 'hdmv_pgs_subtitle', 3, 'jpn', 0, 'Stale image', 1, 1, 101),
                   (2, 'hdmv_pgs_subtitle', 3, 'eng', 0, 'English image', 0, 0, 100)"#,
        r#"INSERT INTO "Subtitle" ("MovieMetadataId", "SubtitleKind", "Title", "StreamIndex", "Codec", "Default", "Forced", "SDH", "Language", "IsExtracted", "DateAdded", "DateUpdated")
            VALUES (21, 0, 'Stale image', 0, 'hdmv_pgs_subtitle', 1, 1, 0, 'jpn', 0, '2026-09-22', '2026-09-22'),
                   (21, 0, 'Wrong language/version', 2, 'hdmv_pgs_subtitle', 0, 0, 0, 'jpn', 0, '2026-09-22', '2026-09-22')"#,
    ] {
        sqlx::query(sql).execute(pool).await.unwrap();
    }
}

#[tokio::test]
async fn automatic_image_subtitles_rank_only_matching_resolved_version_candidates() {
    let pool = fixture_pool().await;
    add_stale_image_subtitle_candidates(&pool).await;
    let mut channel = Channel {
        preferred_subtitle_language_code: Some("jpn".into()),
        subtitle_mode: ChannelSubtitleMode::Default,
        ..Default::default()
    };
    let projected = project(&pool, &channel, &item()).await.unwrap();
    assert_eq!(
        projected.tracks.unwrap().subtitle.unwrap().stream_index,
        Some(3)
    );
    channel.subtitle_mode = ChannelSubtitleMode::Forced;
    let projected = project(&pool, &channel, &item()).await.unwrap();
    assert!(
        projected.tracks.unwrap().subtitle.is_none(),
        "the other version's forced subtitle must not be selected"
    );
    channel.subtitle_mode = ChannelSubtitleMode::Default;
    sqlx::query(r#"DELETE FROM "MediaStream" WHERE "MediaVersionId" = 100 AND "Index" = 3"#)
        .execute(&pool)
        .await
        .unwrap();
    let projected = project(&pool, &channel, &item()).await.unwrap();
    assert!(
        projected.tracks.unwrap().subtitle.is_none(),
        "unavailable embedded subtitles must not be projected"
    );
}

#[tokio::test]
async fn custom_image_subtitle_selection_scopes_candidates_and_preserves_pins_and_off() {
    use std::io::Write as _;
    let pool = fixture_pool().await;
    add_stale_image_subtitle_candidates(&pool).await;
    let folder = crate::file_system_layout::channel_stream_selectors_folder();
    std::fs::create_dir_all(&folder).unwrap();
    let mut selector = tempfile::Builder::new()
        .prefix("ucu23-")
        .suffix(".yml")
        .tempfile_in(folder)
        .unwrap();
    selector
        .write_all(b"items:\n  - audio_language: [eng]\n    subtitle_language: [jpn]\n")
        .unwrap();
    selector.flush().unwrap();
    let mut channel = Channel {
        stream_selector_mode: ChannelStreamSelectorMode::Custom,
        stream_selector: Some(
            selector
                .path()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .into(),
        ),
        preferred_subtitle_language_code: Some("eng".into()),
        subtitle_mode: ChannelSubtitleMode::Default,
        ..Default::default()
    };
    let projected = project(&pool, &channel, &item()).await.unwrap();
    assert_eq!(
        projected.tracks.unwrap().subtitle.unwrap().stream_index,
        Some(3)
    );
    let mut pinned = item();
    pinned.pinned_stream_selection = Some(PinnedStreamSelection {
        media_version_id: 100,
        video_stream_index: 0,
        audio_stream_index: Some(1),
        subtitle_stream_index: None,
    });
    assert!(project(&pool, &channel, &pinned)
        .await
        .unwrap()
        .tracks
        .unwrap()
        .subtitle
        .is_none());
    selector.as_file_mut().set_len(0).unwrap();
    std::io::Seek::rewind(selector.as_file_mut()).unwrap();
    selector
        .write_all(b"items:\n  - audio_language: [eng]\n    disable_subtitles: true\n")
        .unwrap();
    selector.flush().unwrap();
    assert!(project(&pool, &channel, &item())
        .await
        .unwrap()
        .tracks
        .unwrap()
        .subtitle
        .is_none());
    // Missing custom selector still falls back to automatic preferences.
    channel.stream_selector = None;
    assert!(project(&pool, &channel, &item())
        .await
        .unwrap()
        .tracks
        .unwrap()
        .subtitle
        .is_some());
}

async fn unextracted_text_fixture() -> DbPool {
    let pool = fixture_pool().await;
    sqlx::query(r#"UPDATE "Subtitle" SET "IsExtracted"=0, "Path"=NULL WHERE "Id"=1"#)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, 'false')"#)
        .bind(crate::ffmpeg_config::CONFIG_KEY_FFMPEG_USE_EMBEDDED_SUBTITLES)
        .execute(&pool)
        .await
        .unwrap();
    pool
}

#[tokio::test]
async fn next_embedded_text_bypasses_native_enable_and_extraction_restrictions() {
    let pool = unextracted_text_fixture().await;
    let channel = Channel {
        next_engine_text_subtitle_mode: ersatzrs_domain::NextEngineTextSubtitleMode::Convert,
        preferred_subtitle_language_code: Some("eng".into()),
        subtitle_mode: ChannelSubtitleMode::Default,
        ..Default::default()
    };
    let projected = project(&pool, &channel, &item()).await.unwrap();
    let track = projected.tracks.unwrap().subtitle.unwrap();
    assert_eq!(track.stream_index, Some(4));
    assert!(
        matches!(track.source, Some(wire::Source::Local { path, .. }) if path == "media/fixture.mkv")
    );
    assert!(crate::playback::select_playback_subtitle(
        &pool,
        10,
        &["eng".into()],
        ChannelSubtitleMode::Default,
        StreamingMode::HttpLiveStreamingSegmenter
    )
    .await
    .unwrap()
    .is_none());
    sqlx::query(r#"UPDATE "ConfigElement" SET "Value"='true' WHERE "Key"=?"#)
        .bind(crate::ffmpeg_config::CONFIG_KEY_FFMPEG_USE_EMBEDDED_SUBTITLES)
        .execute(&pool)
        .await
        .unwrap();
    assert!(crate::playback::select_playback_subtitle(
        &pool,
        10,
        &["eng".into()],
        ChannelSubtitleMode::Default,
        StreamingMode::HttpLiveStreamingSegmenter
    )
    .await
    .unwrap()
    .is_none());
    let mut pinned = item();
    pinned.pinned_stream_selection = Some(PinnedStreamSelection {
        media_version_id: 100,
        video_stream_index: 0,
        audio_stream_index: Some(2),
        subtitle_stream_index: Some(4),
    });
    assert_eq!(
        project(&pool, &channel, &pinned)
            .await
            .unwrap()
            .tracks
            .unwrap()
            .subtitle
            .unwrap()
            .stream_index,
        Some(4)
    );
    assert!(crate::playback::select_pinned_playback_subtitle(
        &pool,
        10,
        100,
        4,
        StreamingMode::HttpLiveStreamingSegmenter
    )
    .await
    .is_err());
    pinned
        .pinned_stream_selection
        .as_mut()
        .unwrap()
        .subtitle_stream_index = None;
    assert!(project(&pool, &channel, &pinned)
        .await
        .unwrap()
        .tracks
        .unwrap()
        .subtitle
        .is_none());
}

#[tokio::test]
async fn next_embedded_text_keeps_http_transport_and_resolved_version_identity() {
    let pool = unextracted_text_fixture().await;
    let channel = Channel {
        next_engine_text_subtitle_mode: ersatzrs_domain::NextEngineTextSubtitleMode::Convert,
        preferred_subtitle_language_code: Some("eng".into()),
        subtitle_mode: ChannelSubtitleMode::Default,
        ..Default::default()
    };
    let source = wire::Source::Http {
        uri: "https://media.example.test/fixture.mkv".into(),
        headers: vec!["X-Fixture: synthetic".into()],
        user_agent: Some("fixture".into()),
        keep_alive: None,
        reconnect: Some(true),
        reconnect_delay_max: Some(2),
        timeout_us: Some(5_000_000),
        in_point_ms: None,
        out_point_ms: None,
    };
    let projected = project_item(
        &pool,
        &channel,
        &item(),
        100,
        source,
        &Resolution::default(),
    )
    .await
    .unwrap();
    let tracks = projected.tracks.unwrap();
    assert_eq!(
        serde_json::to_value(tracks.video.unwrap().source).unwrap(),
        serde_json::to_value(tracks.subtitle.unwrap().source).unwrap()
    );
    sqlx::query(r#"UPDATE "MediaStream" SET "Title"='Other version identity' WHERE "Id"=5"#)
        .execute(&pool)
        .await
        .unwrap();
    assert!(project(&pool, &channel, &item())
        .await
        .unwrap()
        .tracks
        .unwrap()
        .subtitle
        .is_none());
}

#[tokio::test]
async fn next_custom_embedded_text_is_available_without_extraction_and_native_is_unchanged() {
    use std::io::Write as _;
    let pool = unextracted_text_fixture().await;
    let folder = crate::file_system_layout::channel_stream_selectors_folder();
    std::fs::create_dir_all(&folder).unwrap();
    let mut selector = tempfile::Builder::new()
        .prefix("ucu24-")
        .suffix(".yml")
        .tempfile_in(folder)
        .unwrap();
    selector
        .write_all(b"items:\n  - audio_language: [eng]\n    subtitle_language: [eng]\n")
        .unwrap();
    selector.flush().unwrap();
    let channel = Channel {
        next_engine_text_subtitle_mode: ersatzrs_domain::NextEngineTextSubtitleMode::Convert,
        stream_selector_mode: ChannelStreamSelectorMode::Custom,
        stream_selector: Some(
            selector
                .path()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .into(),
        ),
        subtitle_mode: ChannelSubtitleMode::Default,
        ..Default::default()
    };
    assert_eq!(
        project(&pool, &channel, &item())
            .await
            .unwrap()
            .tracks
            .unwrap()
            .subtitle
            .unwrap()
            .stream_index,
        Some(4)
    );
    assert!(crate::playback::select_custom_playback_streams(
        &pool,
        100,
        10,
        &channel,
        item().start,
        StreamingMode::HttpLiveStreamingSegmenter
    )
    .await
    .unwrap()
    .is_none());
}

#[tokio::test]
async fn next_excludes_external_bitmaps_and_rejects_their_exact_pins() {
    let pool = fixture_pool().await;
    sqlx::query(r#"UPDATE "Subtitle" SET "Codec"='hdmv_pgs_subtitle' WHERE "Id"=2"#)
        .execute(&pool)
        .await
        .unwrap();
    let channel = Channel {
        next_engine_text_subtitle_mode: ersatzrs_domain::NextEngineTextSubtitleMode::Convert,
        preferred_subtitle_language_code: Some("nld".into()),
        subtitle_mode: ChannelSubtitleMode::Default,
        ..Default::default()
    };
    assert!(project(&pool, &channel, &item())
        .await
        .unwrap()
        .tracks
        .unwrap()
        .subtitle
        .is_none());
    let mut pinned = item();
    pinned.pinned_stream_selection = Some(PinnedStreamSelection {
        media_version_id: 100,
        video_stream_index: 0,
        audio_stream_index: Some(2),
        subtitle_stream_index: Some(9),
    });
    assert!(project(&pool, &channel, &pinned).await.is_err());
}

#[tokio::test]
async fn next_burn_rejects_unextracted_embedded_text_before_publication() {
    let pool = unextracted_text_fixture().await;
    let channel = Channel {
        preferred_subtitle_language_code: Some("eng".into()),
        subtitle_mode: ChannelSubtitleMode::Default,
        ..Default::default()
    };
    let error = project(&pool, &channel, &item()).await.err().unwrap();
    assert!(error.to_string().contains("require an extracted sidecar"));
    assert!(!error.to_string().contains("media/"));
}

#[tokio::test]
async fn deferred_burn_preserves_exact_pin_and_seek_without_restoring_subtitles_off() {
    let pool = unextracted_text_fixture().await;
    let channel = Channel {
        preferred_subtitle_language_code: Some("nld".into()),
        subtitle_mode: ChannelSubtitleMode::Default,
        ..Default::default()
    };
    let mut pinned = item();
    pinned.pinned_stream_selection = Some(PinnedStreamSelection {
        media_version_id: 100,
        video_stream_index: 0,
        audio_stream_index: Some(2),
        subtitle_stream_index: Some(4),
    });
    for subtitles_on in [true, false] {
        pinned
            .pinned_stream_selection
            .as_mut()
            .unwrap()
            .subtitle_stream_index = subtitles_on.then_some(4);
        let projected = project_deferred_item(
            &pool,
            &channel,
            &pinned,
            100,
            wire::Source::Local {
                path: "media/fixture.mkv".into(),
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
        .unwrap();
        let tracks = projected.item.tracks.unwrap();
        assert_eq!(tracks.audio.unwrap().stream_index, Some(2));
        if subtitles_on {
            let pending = projected.pending_burn.unwrap();
            assert_eq!(pending.subtitle.id, 1);
            assert_eq!(pending.subtitle.stream_index, 4);
            let source = serde_json::to_value(&pending.source).unwrap();
            assert_eq!(source["in_point_ms"], 1000);
            assert_eq!(source["out_point_ms"], 61_000);
            let track = tracks.subtitle.unwrap();
            assert_eq!(track.stream_index, Some(4));
            assert_eq!(serde_json::to_value(track.source.unwrap()).unwrap(), source);
            assert!(project(&pool, &channel, &pinned).await.is_err());
        } else {
            assert!(projected.pending_burn.is_none());
            assert!(tracks.subtitle.is_none());
        }
    }
}

#[tokio::test]
async fn published_audio_override_preserves_silence_and_rejects_stale_or_conflicting_pins() {
    let pool = fixture_pool().await;
    let channel = Channel::default();
    let mut item = item();
    for (published, pin, succeeds) in [
        (None, None, true),
        (Some(2), None, true),
        (Some(99), None, false),
        (Some(1), Some(2), false),
        (None, Some(2), false),
        (Some(2), Some(2), true),
    ] {
        item.pinned_stream_selection = pin.map(|index| PinnedStreamSelection {
            media_version_id: 100,
            video_stream_index: 0,
            audio_stream_index: Some(index),
            subtitle_stream_index: None,
        });
        let result = project_deferred_item_with_audio(
            &pool,
            &channel,
            &item,
            100,
            wire::Source::Local {
                path: "media/fixture.mkv".into(),
                in_point_ms: None,
                out_point_ms: None,
            },
            &Resolution::default(),
            AudioDirective::Published(published),
        )
        .await;
        assert_eq!(result.is_ok(), succeeds);
        if let Ok(projected) = result {
            let track = projected.item.tracks.unwrap().audio.unwrap();
            if let Some(index) = published {
                assert_eq!(track.stream_index, Some(u32::try_from(index).unwrap()));
            } else {
                assert!(matches!(track.source, Some(wire::Source::Lavfi { .. })));
            }
        }
    }
}

#[path = "provider_subtitle_tests.rs"]
mod provider_subtitle_tests;
