mod common;

use ersatzrs_core::repositories::{ConfigRepository, GraphicsRepository};
use ersatzrs_domain::{
    Channel, ChannelMusicVideoCreditsMode, ChannelSubtitleMode, ChannelWatermarkImageSource,
    FillerKind, GraphicsElement, GraphicsElementConfig, GraphicsElementKind, ImageGraphicsConfig,
    MotionEndBehavior, MotionGraphicsConfig, Resolution, StreamingMode, TextGraphicsConfig,
    TextGraphicsRenderer, CONFIG_KEY_FFMPEG_FALLBACK_SUBTITLE_LANGUAGE_CODE,
    CONFIG_KEY_FFMPEG_GLOBAL_WATERMARK_ID, CONFIG_KEY_FFMPEG_PREFERRED_SUBTITLE_LANGUAGE_CODE,
};
use ersatzrs_ffmpeg::{
    DesiredVideoState, FrameRate, FrameSize, GraphicsPipelineOverlay, InputFile, PipelineBuilder,
    PipelineRequest, SoftwarePipelineBuilder, SourceVideoCodec, SourceVideoStream, SubtitleMethod,
};
use ersatzrs_infra::ffmpeg_config::{
    playback_language_preferences, CONFIG_KEY_FFMPEG_USE_EMBEDDED_SUBTITLES,
};
use ersatzrs_infra::file_system_layout::data_path;
use ersatzrs_infra::graphics_import::upsert_graphics_element_from_yaml;
use ersatzrs_infra::graphics_template::{
    evaluate_graphics_number, graphics_template_variables, render_graphics_template,
    GraphicsTemplateContext,
};
use ersatzrs_infra::playback::{
    render_playback_graphics_overlays, select_channel_playback_subtitle,
    select_pinned_playback_subtitle, select_playback_graphics, select_playback_subtitle,
    select_playback_watermarks, subtitle_input_for_stream, watermark_fade_points_for_program,
    PlaybackGraphicsArtifacts, PlaybackGraphicsRenderContext, ResolvedPlaybackGraphicsElement,
};
use ersatzrs_infra::repositories::make_repositories;
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

async fn seed_movie_with_subtitles(pool: &SqlitePool) {
    sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
           VALUES (1, 'Movies', 1, 1)"#,
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(r#"INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path") VALUES (1, 1, '/media')"#)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (10, 1, 0)"#)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "Movie" ("Id") VALUES (10)"#)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO "MovieMetadata" (
               "Id", "MovieId", "Title", "MetadataKind", "DateAdded", "DateUpdated"
           ) VALUES (20, 10, 'Movie', 0, '2026-05-31 00:00:00', '2026-05-31 00:00:00')"#,
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "Subtitle" (
               "Id", "MovieMetadataId", "SubtitleKind", "Title", "StreamIndex",
               "Codec", "Default", "Forced", "SDH", "Language", "IsExtracted",
               "Path", "DateAdded", "DateUpdated"
           ) VALUES
               (1, 20, 0, 'English default', 2, 'subrip', 1, 0, 0, 'eng', 1,
                'AA/BB/default.srt', '2026-05-31 00:00:00', '2026-05-31 00:00:00'),
               (2, 20, 1, 'Dutch forced', 3, 'subrip', 0, 1, 0, 'nld', 0,
                'movie.nl.forced.srt', '2026-05-31 00:00:00', '2026-05-31 00:00:00'),
               (3, 20, 0, 'Japanese PGS', 4, 'hdmv_pgs_subtitle', 0, 0, 0, 'jpn', 0,
                NULL, '2026-05-31 00:00:00', '2026-05-31 00:00:00')"#,
    )
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_music_video(pool: &SqlitePool) {
    common::seed_media_path(pool, 2, 2, 2).await;
    common::seed_media_item(pool, 30, 2).await;
    common::seed_media_item(pool, 31, 2).await;
    sqlx::query(r#"INSERT INTO "Artist" ("Id") VALUES (31)"#)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO "ArtistMetadata" (
               "Id", "ArtistId", "Title", "MetadataKind", "DateAdded", "DateUpdated"
           ) VALUES (32, 31, 'Solo Artist', 0, '2026-05-31 00:00:00', '2026-05-31 00:00:00')"#,
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(r#"INSERT INTO "MusicVideo" ("Id", "ArtistId") VALUES (30, 31)"#)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO "MusicVideoMetadata" (
               "Id", "MusicVideoId", "Title", "Album", "Plot", "ReleaseDate",
               "Track", "MetadataKind", "DateAdded", "DateUpdated"
           ) VALUES (
               33, 30, 'Credit Song', 'Credit Album', 'Plot', '2026-05-31',
               7, 0, '2026-05-31 00:00:00', '2026-05-31 00:00:00'
           )"#,
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "MusicVideoArtist" ("Id", "Name", "MusicVideoMetadataId")
           VALUES (34, 'Artist One', 33), (35, 'Artist Two', 33)"#,
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "MediaVersion" (
               "Id", "DateAdded", "DateUpdated", "Duration", "Height",
               "MusicVideoId", "VideoScanKind", "Width"
           ) VALUES (36, '2026-05-31 00:00:00', '2026-05-31 00:00:00', '00:12:00', 1080, 30, 0, 1920)"#,
    )
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn selects_forced_subtitle_by_expanded_language_code() {
    let pool = common::test_db().await;
    seed_movie_with_subtitles(&pool).await;

    let selected = select_playback_subtitle(
        &pool,
        10,
        &["nl".to_owned()],
        ChannelSubtitleMode::Forced,
        StreamingMode::TransportStream,
    )
    .await
    .unwrap()
    .expect("selected subtitle");

    assert_eq!(selected.id, 2);
    let input = subtitle_input_for_stream(
        &selected,
        "/media/movie.mkv",
        ersatzrs_ffmpeg::OutputFormat::MpegTs,
        StreamingMode::TransportStream,
    )
    .expect("subtitle input");
    assert_eq!(
        input.path,
        Path::new("/media")
            .join("movie.nl.forced.srt")
            .to_string_lossy()
    );
}

#[tokio::test]
async fn subtitle_language_fallback_is_ranked_after_primary_for_requested_mode() {
    let pool = common::test_db().await;
    seed_movie_with_subtitles(&pool).await;

    let selected = select_playback_subtitle(
        &pool,
        10,
        &["eng".to_owned(), "nld".to_owned()],
        ChannelSubtitleMode::Forced,
        StreamingMode::TransportStream,
    )
    .await
    .unwrap()
    .expect("fallback forced subtitle");

    assert_eq!(selected.id, 2);
}

#[tokio::test]
async fn configured_subtitle_languages_do_not_fall_back_to_an_unrelated_track() {
    let pool = common::test_db().await;
    seed_movie_with_subtitles(&pool).await;

    let selected = select_playback_subtitle(
        &pool,
        10,
        &["fra".to_owned(), "deu".to_owned()],
        ChannelSubtitleMode::Any,
        StreamingMode::HttpLiveStreamingDirect,
    )
    .await
    .unwrap();

    assert_eq!(selected, None);
}

#[tokio::test]
async fn unset_subtitle_languages_preserve_unfiltered_mode_ranking() {
    let pool = common::test_db().await;
    seed_movie_with_subtitles(&pool).await;

    let selected = select_playback_subtitle(
        &pool,
        10,
        &[],
        ChannelSubtitleMode::Default,
        StreamingMode::HttpLiveStreamingDirect,
    )
    .await
    .unwrap()
    .expect("default subtitle");

    assert_eq!(selected.id, 1);
}

#[tokio::test]
async fn undefined_subtitle_preference_matches_und_and_missing_languages() {
    let pool = common::test_db().await;
    seed_movie_with_subtitles(&pool).await;
    sqlx::query(
        r#"INSERT INTO "Subtitle" (
               "Id", "MovieMetadataId", "SubtitleKind", "Title", "StreamIndex",
               "Codec", "Default", "Forced", "SDH", "Language", "IsExtracted",
               "Path", "DateAdded", "DateUpdated"
           ) VALUES
               (4, 20, 1, 'Undefined', 5, 'subrip', 0, 0, 0, 'und', 0,
                'movie.srt', '2026-05-31 00:00:00', '2026-05-31 00:00:00'),
               (5, 20, 1, 'Missing language', 6, 'subrip', 1, 0, 0, NULL, 0,
                'movie.default.srt', '2026-05-31 00:00:00', '2026-05-31 00:00:00')"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    let selected = select_playback_subtitle(
        &pool,
        10,
        &["und".to_owned()],
        ChannelSubtitleMode::Default,
        StreamingMode::HttpLiveStreamingDirect,
    )
    .await
    .unwrap()
    .expect("undefined subtitle");

    assert_eq!(selected.id, 5);
}

#[tokio::test]
async fn pinned_subtitle_is_resolved_for_the_selected_media_version() {
    let pool = common::test_db().await;
    seed_movie_with_subtitles(&pool).await;
    sqlx::query(
        r#"INSERT INTO "MediaVersion" (
               "Id", "DateAdded", "DateUpdated", "Duration", "MovieId",
               "Height", "Width", "VideoScanKind"
           ) VALUES
               (101, '2026-07-28 00:00:00', '2026-07-28 00:00:00',
                '02:00:00', 10, 1080, 1920, 0),
               (102, '2026-07-28 00:00:00', '2026-07-28 00:00:00',
                '02:01:00', 10, 2160, 3840, 0)"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "MediaStream" (
               "Id", "Index", "Codec", "MediaStreamKind", "Language",
               "Channels", "Title", "Default", "Forced", "MediaVersionId"
           ) VALUES
               (201, 7, 'subrip', 3, 'eng', 0, 'Version A subtitles', 0, 0, 101),
               (202, 7, 'ass', 3, 'nld', 0, 'Version B subtitles', 0, 0, 102)"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "Subtitle" (
               "Id", "MovieMetadataId", "SubtitleKind", "Title", "StreamIndex",
               "Codec", "Default", "Forced", "SDH", "Language", "IsExtracted",
               "Path", "DateAdded", "DateUpdated"
           ) VALUES
               (11, 20, 0, 'Version A subtitles', 7, 'subrip', 0, 0, 0, 'eng', 1,
                'AA/BB/version-a.srt', '2026-07-28 00:00:00', '2026-07-28 00:00:00'),
               (12, 20, 0, 'Version B subtitles', 7, 'ass', 0, 0, 0, 'nld', 1,
                'AA/BB/version-b.ass', '2026-07-28 00:00:00', '2026-07-28 00:00:00'),
               (13, 20, 1, 'Sidecar with reused index', 7, 'subrip', 0, 0, 0, 'eng', 1,
                'movie.external.srt', '2026-07-28 00:00:00', '2026-07-28 00:00:00')"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    let selected =
        select_pinned_playback_subtitle(&pool, 10, 102, 7, StreamingMode::HttpLiveStreamingDirect)
            .await
            .unwrap();

    assert_eq!(selected.id, 12);
    assert_eq!(selected.path.as_deref(), Some("AA/BB/version-b.ass"));
}

#[tokio::test]
async fn ambiguous_pinned_subtitle_rows_fail_closed() {
    let pool = common::test_db().await;
    seed_movie_with_subtitles(&pool).await;
    sqlx::query(
        r#"INSERT INTO "MediaVersion" (
               "Id", "DateAdded", "DateUpdated", "Duration", "MovieId",
               "Height", "Width", "VideoScanKind"
           ) VALUES (
               102, '2026-07-28 00:00:00', '2026-07-28 00:00:00',
               '02:01:00', 10, 2160, 3840, 0
           )"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "MediaStream" (
               "Id", "Index", "Codec", "MediaStreamKind", "Language",
               "Channels", "Title", "Default", "Forced", "MediaVersionId"
           ) VALUES (
               202, 7, 'ass', 3, 'nld', 0, 'Version B subtitles', 0, 0, 102
           )"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "Subtitle" (
               "Id", "MovieMetadataId", "SubtitleKind", "Title", "StreamIndex",
               "Codec", "Default", "Forced", "SDH", "Language", "IsExtracted",
               "Path", "DateAdded", "DateUpdated"
           ) VALUES
               (11, 20, 0, 'Version B subtitles', 7, 'ass', 0, 0, 0, 'nld', 1,
                'AA/BB/version-b-1.ass', '2026-07-28 00:00:00', '2026-07-28 00:00:00'),
               (12, 20, 0, 'Version B subtitles', 7, 'ass', 0, 0, 0, 'nld', 1,
                'AA/BB/version-b-2.ass', '2026-07-28 00:00:00', '2026-07-28 00:00:00')"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    assert!(select_pinned_playback_subtitle(
        &pool,
        10,
        102,
        7,
        StreamingMode::HttpLiveStreamingDirect,
    )
    .await
    .is_err());
}

#[tokio::test]
async fn persisted_subtitle_language_preferences_load_in_primary_fallback_order() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    repos
        .config
        .set(CONFIG_KEY_FFMPEG_PREFERRED_SUBTITLE_LANGUAGE_CODE, "nld")
        .await
        .unwrap();
    repos
        .config
        .set(CONFIG_KEY_FFMPEG_FALLBACK_SUBTITLE_LANGUAGE_CODE, "eng")
        .await
        .unwrap();

    let preferences = playback_language_preferences(&pool).await.unwrap();

    assert_eq!(
        preferences.subtitle_languages(None, None),
        vec!["nld".to_owned(), "eng".to_owned()]
    );
}

#[tokio::test]
async fn filters_embedded_subtitles_when_config_disables_them() {
    let pool = common::test_db().await;
    seed_movie_with_subtitles(&pool).await;
    let repos = make_repositories(pool.clone());
    repos
        .config
        .set(CONFIG_KEY_FFMPEG_USE_EMBEDDED_SUBTITLES, "False")
        .await
        .unwrap();

    let selected = select_playback_subtitle(
        &pool,
        10,
        &["eng".to_owned()],
        ChannelSubtitleMode::Default,
        StreamingMode::TransportStream,
    )
    .await
    .unwrap();

    assert_eq!(selected, None);
}

#[tokio::test]
async fn hls_direct_can_copy_embedded_image_subtitle_for_mkv_output() {
    let pool = common::test_db().await;
    seed_movie_with_subtitles(&pool).await;

    let selected = select_playback_subtitle(
        &pool,
        10,
        &["jpn".to_owned()],
        ChannelSubtitleMode::Any,
        StreamingMode::HttpLiveStreamingDirect,
    )
    .await
    .unwrap()
    .expect("selected subtitle");

    assert_eq!(selected.id, 3);
    let input = subtitle_input_for_stream(
        &selected,
        "/media/movie.mkv",
        ersatzrs_ffmpeg::OutputFormat::Mkv,
        StreamingMode::HttpLiveStreamingDirect,
    )
    .expect("subtitle input");
    assert_eq!(input.path, "/media/movie.mkv");
    assert_eq!(input.subtitle_method, Some(SubtitleMethod::Copy));
    assert_eq!(input.streams[0].index, 4);
}

#[tokio::test]
async fn channel_music_video_credits_generate_template_subtitle() {
    let pool = common::test_db().await;
    seed_music_video(&pool).await;
    let channel = Channel {
        music_video_credits_mode: ChannelMusicVideoCreditsMode::GenerateSubtitles,
        music_video_credits_template: Some("_ArtistTitleAlbum_CenterTop".to_owned()),
        ..Channel::default()
    };
    let resolution = Resolution {
        id: 1,
        name: "1080p".to_owned(),
        is_custom: false,
        width: 1920,
        height: 1080,
    };

    let selected = select_channel_playback_subtitle(
        &pool,
        30,
        &[],
        ChannelSubtitleMode::None,
        StreamingMode::TransportStream,
        &channel,
        &resolution,
        60,
    )
    .await
    .unwrap()
    .expect("generated music-video credits subtitle");

    assert_eq!(selected.subtitle_kind, 99);
    assert!(selected.default);
    assert!(selected.forced);
    assert_eq!(selected.codec.as_deref(), Some("ass"));
    let path = selected.path.as_ref().expect("generated subtitle path");
    let rendered = std::fs::read_to_string(path).expect("generated subtitle can be read");
    assert!(rendered.contains("PlayResX: 1920"));
    assert!(rendered.contains("Dialogue: 0,00:01:09.00,00:01:16.00"));
    assert!(rendered.contains(r"Artist One | Artist Two\NCredit Song\NCredit Album"));
    cleanup_files([PathBuf::from(path)]);
}

#[test]
fn intermittent_watermark_fade_points_repeat_for_program_duration() {
    let points = watermark_fade_points_for_program(Duration::from_secs(130), 1, 15);

    assert_eq!(points.len(), 6);
    assert_eq!(points[0].time, Duration::from_secs(0));
    assert_eq!(points[1].time, Duration::from_secs(16));
    assert_eq!(points[2].time, Duration::from_mins(1));
    assert_eq!(points[3].time, Duration::from_secs(76));
    assert_eq!(points[4].time, Duration::from_mins(2));
    assert_eq!(points[5].time, Duration::from_secs(136));
}

#[tokio::test]
async fn playback_watermarks_prefer_item_over_channel_and_global() {
    let pool = common::test_db().await;
    seed_movie_with_subtitles(&pool).await;
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 101).await;
    seed_playout_item(&pool, 101, None, 1000, 10).await;
    let global = seed_custom_watermark(&pool, 201, "global", 30).await;
    let channel = seed_custom_watermark(&pool, 202, "channel", 20).await;
    let item_front = seed_custom_watermark(&pool, 203, "item-front", 40).await;
    let item_back = seed_custom_watermark(&pool, 204, "item-back", 10).await;
    set_channel_watermark(&pool, 101, 202).await;
    repos
        .config
        .set(CONFIG_KEY_FFMPEG_GLOBAL_WATERMARK_ID, "201")
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO "PlayoutItemWatermark" ("PlayoutItemId", "WatermarkId")
           VALUES (1000, 203), (1000, 204)"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    let selected = select_playback_watermarks(
        &pool,
        &Channel {
            id: 101,
            watermark_id: Some(202),
            streaming_mode: StreamingMode::TransportStream,
            ..Channel::default()
        },
        1000,
        false,
        common::fixed_time(),
    )
    .await
    .unwrap();

    assert_eq!(
        selected
            .iter()
            .map(|watermark| watermark.watermark.id)
            .collect::<Vec<_>>(),
        vec![204, 203]
    );
    cleanup_files([global, channel, item_front, item_back]);
}

#[tokio::test]
async fn playback_watermarks_resolve_channel_logo_and_defer_resource() {
    let pool = common::test_db().await;
    seed_movie_with_subtitles(&pool).await;
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 102).await;
    seed_playout_item(&pool, 102, None, 1001, 10).await;
    let logo = write_test_file(data_path("iptv/logos/watermark-test-logo.png"));
    seed_watermark(
        &pool,
        205,
        "logo",
        ChannelWatermarkImageSource::ChannelLogo,
        None,
        0,
    )
    .await;
    seed_watermark(
        &pool,
        206,
        "resource",
        ChannelWatermarkImageSource::Resource,
        Some("song-progress.png"),
        0,
    )
    .await;
    sqlx::query(
        r#"INSERT INTO "Artwork" (
               "Id", "ArtworkKind", "ChannelId", "Path", "DateAdded", "DateUpdated"
           ) VALUES (900, 2, 102, 'iptv/logos/watermark-test-logo.png',
               '2026-06-03 00:00:00', '2026-06-03 00:00:00')"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "PlayoutItemWatermark" ("PlayoutItemId", "WatermarkId")
           VALUES (1001, 205), (1001, 206)"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    let selected = select_playback_watermarks(
        &pool,
        &Channel {
            id: 102,
            streaming_mode: StreamingMode::TransportStream,
            ..Channel::default()
        },
        1001,
        false,
        common::fixed_time(),
    )
    .await
    .unwrap();

    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].watermark.id, 205);
    assert_eq!(
        selected[0].image_path,
        data_path("iptv/logos/watermark-test-logo.png")
    );
    cleanup_files([logo]);
}

#[tokio::test]
async fn playback_watermarks_apply_deco_precedence() {
    let pool = common::test_db().await;
    seed_movie_with_subtitles(&pool).await;
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 103).await;
    let playout_deco = seed_custom_watermark(&pool, 207, "playout-deco", 0).await;
    let template_deco = seed_custom_watermark(&pool, 208, "template-deco", 0).await;
    let item = seed_custom_watermark(&pool, 209, "item", 0).await;
    seed_deco(&pool, 301, 3, true, 207).await;
    seed_deco(&pool, 302, 2, true, 208).await;
    seed_playout_item(&pool, 103, Some(301), 1002, 10).await;
    seed_active_deco_template(&pool, 103, 302).await;
    sqlx::query(
        r#"INSERT INTO "PlayoutItemWatermark" ("PlayoutItemId", "WatermarkId")
           VALUES (1002, 209)"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    let selected = select_playback_watermarks(
        &pool,
        &Channel {
            id: 103,
            streaming_mode: StreamingMode::TransportStream,
            ..Channel::default()
        },
        1002,
        false,
        common::fixed_time(),
    )
    .await
    .unwrap();

    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].watermark.id, 208);
    cleanup_files([playout_deco, template_deco, item]);
}

#[tokio::test]
async fn playback_watermarks_preserve_merged_deco_with_item_channel_global_or_no_fallback() {
    for template in [false, true] {
        let pool = common::test_db().await;
        seed_movie_with_subtitles(&pool).await;
        let repos = make_repositories(pool.clone());
        common::seed_profile_and_channel(&repos, 2300).await;
        let merged = seed_custom_watermark(&pool, 2301, "merged", 20).await;
        let fallback = seed_custom_watermark(&pool, 2302, "fallback", 10).await;
        seed_deco(&pool, 2303, 3, true, 2301).await;
        seed_playout_item(&pool, 2300, (!template).then_some(2303), 2304, 10).await;
        if template {
            seed_active_deco_template(&pool, 2300, 2303).await;
        }
        let mut channel = Channel {
            id: 2300,
            ..Channel::default()
        };
        // Upstream SelectWatermarksTestCases: a merge remains present without
        // fallback, and appends to item/channel/global choices instead of replacing them.
        for fallback_kind in 0..4 {
            sqlx::query(r#"DELETE FROM "PlayoutItemWatermark" WHERE "PlayoutItemId"=2304"#)
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key"=?"#)
                .bind(CONFIG_KEY_FFMPEG_GLOBAL_WATERMARK_ID)
                .execute(&pool)
                .await
                .unwrap();
            channel.watermark_id = (fallback_kind == 2).then_some(2302);
            match fallback_kind {
                1 => {
                    sqlx::query(r#"INSERT INTO "PlayoutItemWatermark" ("PlayoutItemId", "WatermarkId") VALUES (2304,2302)"#)
                        .execute(&pool).await.unwrap();
                }
                3 => repos
                    .config
                    .set(CONFIG_KEY_FFMPEG_GLOBAL_WATERMARK_ID, "2302")
                    .await
                    .unwrap(),
                _ => {}
            }
            let selected =
                select_playback_watermarks(&pool, &channel, 2304, false, common::fixed_time())
                    .await
                    .unwrap();
            let ids: Vec<_> = selected.iter().map(|value| value.watermark.id).collect();
            assert_eq!(
                ids,
                if fallback_kind == 0 {
                    vec![2301]
                } else {
                    vec![2302, 2301]
                },
                "template={template}, fallback={fallback_kind}; preserve z-index order"
            );
        }
        cleanup_files([merged, fallback]);
    }
}

#[tokio::test]
async fn playback_watermark_merge_still_respects_item_disable_and_filler_exclusion() {
    let pool = common::test_db().await;
    seed_movie_with_subtitles(&pool).await;
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 2310).await;
    let merged = seed_custom_watermark(&pool, 2311, "merged-filler", 0).await;
    let fallback = seed_custom_watermark(&pool, 2312, "fallback-filler", 0).await;
    seed_deco(&pool, 2313, 3, false, 2311).await;
    seed_playout_item(&pool, 2310, Some(2313), 2314, 10).await;
    let channel = Channel {
        id: 2310,
        watermark_id: Some(2312),
        ..Channel::default()
    };
    assert!(
        select_playback_watermarks(&pool, &channel, 2314, true, common::fixed_time())
            .await
            .unwrap()
            .is_empty()
    );
    sqlx::query(r#"UPDATE "PlayoutItem" SET "FillerKind"=1 WHERE "Id"=2314"#)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        select_playback_watermarks(&pool, &channel, 2314, false, common::fixed_time())
            .await
            .unwrap()
            .is_empty()
    );
    cleanup_files([merged, fallback]);
}

#[tokio::test]
async fn playback_watermarks_match_deco_template_json_filters_and_year_range() {
    let pool = common::test_db().await;
    seed_movie_with_subtitles(&pool).await;
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 106).await;
    let expired = seed_custom_watermark(&pool, 212, "expired-template-deco", 0).await;
    let current = seed_custom_watermark(&pool, 213, "current-template-deco", 0).await;
    seed_deco(&pool, 303, 2, true, 212).await;
    seed_deco(&pool, 304, 2, true, 213).await;
    seed_playout_item(&pool, 106, None, 1005, 10).await;
    seed_filtered_deco_template(&pool, 106, 303, 601, 602, 603, 604, 605, 0, 2025).await;
    seed_filtered_deco_template(&pool, 106, 304, 611, 612, 613, 614, 615, 1, 2026).await;

    let selected = select_playback_watermarks(
        &pool,
        &Channel {
            id: 106,
            streaming_mode: StreamingMode::TransportStream,
            ..Channel::default()
        },
        1005,
        false,
        common::fixed_time(),
    )
    .await
    .unwrap();

    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].watermark.id, 213);
    cleanup_files([expired, current]);
}

#[tokio::test]
async fn playback_watermarks_disable_when_item_disables() {
    let pool = common::test_db().await;
    seed_movie_with_subtitles(&pool).await;
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 104).await;
    let channel = seed_custom_watermark(&pool, 210, "channel-disabled", 0).await;
    set_channel_watermark(&pool, 104, 210).await;
    seed_playout_item(&pool, 104, None, 1003, 10).await;

    let selected = select_playback_watermarks(
        &pool,
        &Channel {
            id: 104,
            watermark_id: Some(210),
            streaming_mode: StreamingMode::TransportStream,
            ..Channel::default()
        },
        1003,
        true,
        common::fixed_time(),
    )
    .await
    .unwrap();

    assert!(selected.is_empty());
    cleanup_files([channel]);
}

#[tokio::test]
async fn playback_watermarks_skip_missing_custom_image() {
    let pool = common::test_db().await;
    seed_movie_with_subtitles(&pool).await;
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 105).await;
    seed_playout_item(&pool, 105, None, 1004, 10).await;
    seed_watermark(
        &pool,
        211,
        "missing",
        ChannelWatermarkImageSource::Custom,
        Some("definitely-missing-watermark.png"),
        0,
    )
    .await;
    set_channel_watermark(&pool, 105, 211).await;

    let selected = select_playback_watermarks(
        &pool,
        &Channel {
            id: 105,
            watermark_id: Some(211),
            streaming_mode: StreamingMode::TransportStream,
            ..Channel::default()
        },
        1004,
        false,
        common::fixed_time(),
    )
    .await
    .unwrap();

    assert!(selected.is_empty());
}

#[tokio::test]
async fn playback_graphics_select_item_elements_with_variables_in_z_order() {
    let pool = common::test_db().await;
    seed_movie_with_subtitles(&pool).await;
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 107).await;
    seed_playout_item(&pool, 107, None, 1006, 10).await;
    seed_graphics_element(&repos, 401, "front", 20).await;
    seed_graphics_element(&repos, 402, "back", 5).await;
    sqlx::query(
        r#"INSERT INTO "PlayoutItemGraphicsElement" (
               "PlayoutItemId", "GraphicsElementId", "Variables"
           ) VALUES (1006, 401, '{"slot":"front"}'), (1006, 402, '{"slot":"back"}')"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    let selected = select_playback_graphics(
        &pool,
        &Channel {
            id: 107,
            streaming_mode: StreamingMode::TransportStream,
            ..Channel::default()
        },
        1006,
        common::fixed_time(),
    )
    .await
    .unwrap();

    assert_eq!(
        selected
            .iter()
            .map(|graphics| (graphics.element.id, graphics.variables.as_deref()))
            .collect::<Vec<_>>(),
        vec![
            (402, Some(r#"{"slot":"back"}"#)),
            (401, Some(r#"{"slot":"front"}"#))
        ]
    );
}

#[tokio::test]
async fn playback_graphics_template_deco_override_preempts_playout_and_item() {
    let pool = common::test_db().await;
    seed_movie_with_subtitles(&pool).await;
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 108).await;
    seed_graphics_element(&repos, 403, "playout-deco", 10).await;
    seed_graphics_element(&repos, 404, "template-deco", 30).await;
    seed_graphics_element(&repos, 405, "item", 20).await;
    seed_graphics_deco(&pool, 305, 3, true, 403).await;
    seed_graphics_deco(&pool, 306, 2, true, 404).await;
    seed_playout_item(&pool, 108, Some(305), 1007, 10).await;
    seed_active_deco_template(&pool, 108, 306).await;
    sqlx::query(
        r#"INSERT INTO "PlayoutItemGraphicsElement" ("PlayoutItemId", "GraphicsElementId")
           VALUES (1007, 405)"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    let selected = select_playback_graphics(
        &pool,
        &Channel {
            id: 108,
            streaming_mode: StreamingMode::TransportStream,
            ..Channel::default()
        },
        1007,
        common::fixed_time(),
    )
    .await
    .unwrap();

    assert_eq!(
        selected
            .iter()
            .map(|graphics| graphics.element.id)
            .collect::<Vec<_>>(),
        vec![404]
    );
}

#[tokio::test]
async fn playback_graphics_deco_disables_during_filler() {
    let pool = common::test_db().await;
    seed_movie_with_subtitles(&pool).await;
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 109).await;
    seed_graphics_element(&repos, 406, "playout-deco", 10).await;
    seed_graphics_element(&repos, 407, "item", 20).await;
    seed_graphics_deco(&pool, 307, 3, false, 406).await;
    seed_playout_item(&pool, 109, Some(307), 1008, 10).await;
    set_playout_item_filler_kind(&pool, 1008, FillerKind::PreRoll).await;
    sqlx::query(
        r#"INSERT INTO "PlayoutItemGraphicsElement" ("PlayoutItemId", "GraphicsElementId")
           VALUES (1008, 407)"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    let selected = select_playback_graphics(
        &pool,
        &Channel {
            id: 109,
            streaming_mode: StreamingMode::TransportStream,
            ..Channel::default()
        },
        1008,
        common::fixed_time(),
    )
    .await
    .unwrap();

    assert!(selected.is_empty());
}

#[tokio::test]
async fn graphics_template_variables_include_epg_duration_and_per_item_overrides() {
    let pool = common::test_db().await;
    seed_movie_with_subtitles(&pool).await;
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 110).await;
    seed_playout_item(&pool, 110, None, 1009, 10).await;
    let context = GraphicsTemplateContext {
        channel_id: 110,
        channel_number: "110".to_owned(),
        media_item_id: 10,
        media_path: "/media/movie.mkv".to_owned(),
        channel_start_time: common::fixed_time(),
        content_start_time: common::fixed_time(),
        stream_seek_seconds: 12.0,
        duration_seconds: 3600.0,
        frame_width: 1920,
        frame_height: 1080,
        scaled_frame_width: 1920,
        scaled_frame_height: 1080,
        r_frame_rate: Some("30000/1001".to_owned()),
        frame_rate: Some(29.97),
    };

    let variables = graphics_template_variables(
        &pool,
        &context,
        1,
        Some(r#"{"MediaItem_Title":"Override","Custom":"Slot"}"#),
    )
    .await
    .unwrap();

    assert_eq!(
        render_graphics_template(
            "{{ MediaItem_Title }} {{ Custom }} {{ Epg[0].Title }}",
            &variables
        )
        .unwrap(),
        "Override Slot Movie"
    );
    let start_seconds =
        evaluate_graphics_number("MediaItem_Duration.TotalSeconds - 5", &variables).unwrap();
    assert!((start_seconds - 3595.0).abs() < f64::EPSILON);
    assert_eq!(
        render_graphics_template("{{ Resolution.Width }}x{{ Resolution.Height }}", &variables)
            .unwrap(),
        "1920x1080"
    );
}

#[tokio::test]
async fn imported_image_graphics_use_persisted_epg_context_without_source_yaml() {
    let pool = common::test_db().await;
    seed_movie_with_subtitles(&pool).await;
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 110).await;
    seed_playout_item(&pool, 110, None, 1009, 10).await;
    let temp = tempfile::tempdir().unwrap();
    let missing_yaml = temp.path().join("missing-image.yml");
    let yaml = "name: Guide image\nimage: '{{ Epg[0].Title }}.ppm'\n";
    let initial = upsert_graphics_element_from_yaml(
        &repos.graphics,
        702,
        GraphicsElementKind::Image,
        missing_yaml.to_str().unwrap(),
        yaml,
    )
    .await
    .unwrap();
    assert_eq!(initial.epg_entries, 0);
    upsert_graphics_element_from_yaml(
        &repos.graphics,
        702,
        GraphicsElementKind::Image,
        missing_yaml.to_str().unwrap(),
        &format!("{yaml}epg_entries: 1\n"),
    )
    .await
    .unwrap();
    let element = repos.graphics.get_element(702).await.unwrap();
    assert_eq!(element.epg_entries, 1);
    assert!(!missing_yaml.exists());
    let context = PlaybackGraphicsRenderContext {
        channel_id: 110,
        channel_number: "110".to_owned(),
        media_item_id: 10,
        media_path: "/media/movie.mkv".to_owned(),
        channel_start_time: common::fixed_time(),
        content_start_time: common::fixed_time(),
        stream_seek_seconds: 0.0,
        duration_seconds: 3600.0,
        frame_size: FrameSize::new(320, 180),
        square_pixel_frame_size: FrameSize::new(320, 180),
        frame_rate: Some(FrameRate::DEFAULT),
    };
    let graphics = [ResolvedPlaybackGraphicsElement {
        element,
        variables: None,
    }];
    let artifacts =
        render_playback_graphics_overlays(&pool, &graphics, &context, temp.path().join("rendered"))
            .await
            .unwrap();
    let [GraphicsPipelineOverlay::NativeImage { input, .. }] = artifacts.overlays.as_slice() else {
        panic!("expected one image overlay");
    };
    assert_eq!(input.path, "Movie.ppm");
    artifacts.shutdown().await;
}

#[tokio::test]
async fn render_playback_graphics_overlays_uses_per_element_variables() {
    let pool = common::test_db().await;
    seed_movie_with_subtitles(&pool).await;
    let output_dir = std::env::temp_dir().join(format!(
        "ersatzrs-infra-graphics-render-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&output_dir);

    let graphics = vec![ResolvedPlaybackGraphicsElement {
        element: GraphicsElement {
            id: 701,
            name: "title".to_owned(),
            kind: GraphicsElementKind::Text,
            path: Some("templates/graphics-elements/title.yml".to_owned()),
            z_index: 12,
            config: GraphicsElementConfig::Text(TextGraphicsConfig {
                text: Some("{{ MediaItem_Title }} / {{ Custom }}".to_owned()),
                ..TextGraphicsConfig::default()
            }),
            ..GraphicsElement::default()
        },
        variables: Some(r#"{"Custom":"Slot"}"#.to_owned()),
    }];
    let context = PlaybackGraphicsRenderContext {
        channel_id: 120,
        channel_number: "120".to_owned(),
        media_item_id: 10,
        media_path: "/media/movie.mkv".to_owned(),
        channel_start_time: common::fixed_time(),
        content_start_time: common::fixed_time(),
        stream_seek_seconds: 0.0,
        duration_seconds: 3600.0,
        frame_size: FrameSize::new(1920, 1080),
        square_pixel_frame_size: FrameSize::new(1920, 1080),
        frame_rate: Some(FrameRate::DEFAULT),
    };

    let artifacts = render_playback_graphics_overlays(&pool, &graphics, &context, &output_dir)
        .await
        .unwrap();

    assert_eq!(artifacts.overlays.len(), 1);
    let GraphicsPipelineOverlay::Drawvg { z_index, filter } = &artifacts.overlays[0] else {
        panic!("expected drawvg overlay");
    };
    assert_eq!(*z_index, 12);
    assert!(filter.contains("701-title.vgs"));
    let rendered_script = std::fs::read_to_string(output_dir.join("701-title.vgs")).unwrap();
    assert!(rendered_script.contains("setcolor"));
    assert!(rendered_script.contains("rect"));

    artifacts.shutdown().await;
    std::fs::remove_dir_all(output_dir).unwrap();
}

#[test]
fn legacy_text_graphics_json_defaults_to_drawvg() {
    let config: TextGraphicsConfig = serde_json::from_str(
        r#"{"text":"{{ MediaItem_Title }}","text_fit":"none","text_align":"Left"}"#,
    )
    .unwrap();

    assert_eq!(config.renderer, TextGraphicsRenderer::Drawvg);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn drawtext_graphics_renders_initial_text_and_refreshes_playback_elapsed() {
    let pool = common::test_db().await;
    seed_movie_with_subtitles(&pool).await;
    let output_dir = tempfile::tempdir().unwrap();
    let graphics = vec![ResolvedPlaybackGraphicsElement {
        element: GraphicsElement {
            id: 703,
            name: "live-title".to_owned(),
            kind: GraphicsElementKind::Text,
            z_index: 13,
            config: GraphicsElementConfig::Text(TextGraphicsConfig {
                text: Some(
                    "{{ MediaItem_Title }}|{{ Custom }}|{{ Playback.ElapsedMilliseconds }}"
                        .to_owned(),
                ),
                renderer: TextGraphicsRenderer::Drawtext,
                drawtext: ersatzrs_domain::DrawtextGraphicsConfig {
                    refresh_interval_milliseconds: 1,
                    ..ersatzrs_domain::DrawtextGraphicsConfig::default()
                },
                ..TextGraphicsConfig::default()
            }),
            ..GraphicsElement::default()
        },
        variables: Some(r#"{"Custom":"Dynamic"}"#.to_owned()),
    }];
    let context = PlaybackGraphicsRenderContext {
        channel_id: 120,
        channel_number: "120".to_owned(),
        media_item_id: 10,
        media_path: "/media/movie.mkv".to_owned(),
        channel_start_time: common::fixed_time(),
        content_start_time: common::fixed_time(),
        stream_seek_seconds: 12.0,
        duration_seconds: 3_600.0,
        frame_size: FrameSize::new(1920, 1080),
        square_pixel_frame_size: FrameSize::new(1920, 1080),
        frame_rate: Some(FrameRate::DEFAULT),
    };

    let artifacts =
        render_playback_graphics_overlays(&pool, &graphics, &context, output_dir.path())
            .await
            .unwrap();

    assert_eq!(artifacts.overlays.len(), 1);
    assert_eq!(artifacts.dynamic_text_controllers.len(), 1);
    let GraphicsPipelineOverlay::Drawtext { z_index, filter } = &artifacts.overlays[0] else {
        panic!("expected drawtext overlay");
    };
    assert_eq!(*z_index, 13);
    assert!(filter.contains("textfile="));
    let text_file = artifacts.dynamic_text_controllers[0]
        .text_file()
        .to_path_buf();
    let initial = tokio::fs::read_to_string(&text_file).await.unwrap();
    assert!(initial.starts_with("Movie|Dynamic|12000"));

    let mut refreshed = initial.clone();
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        refreshed = tokio::fs::read_to_string(&text_file).await.unwrap();
        if refreshed != initial {
            break;
        }
    }
    assert_ne!(refreshed, initial);
    assert!(refreshed.starts_with("Movie|Dynamic|"));

    artifacts.shutdown().await;
}

#[tokio::test]
async fn invalid_drawtext_element_does_not_suppress_valid_static_graphics() {
    let pool = common::test_db().await;
    seed_movie_with_subtitles(&pool).await;
    let output_dir = tempfile::tempdir().unwrap();
    let graphics = vec![
        ResolvedPlaybackGraphicsElement {
            element: GraphicsElement {
                id: 704,
                name: "broken-dynamic".to_owned(),
                kind: GraphicsElementKind::Text,
                config: GraphicsElementConfig::Text(TextGraphicsConfig {
                    text: Some("{{".to_owned()),
                    renderer: TextGraphicsRenderer::Drawtext,
                    ..TextGraphicsConfig::default()
                }),
                ..GraphicsElement::default()
            },
            variables: None,
        },
        ResolvedPlaybackGraphicsElement {
            element: GraphicsElement {
                id: 705,
                name: "valid-static".to_owned(),
                kind: GraphicsElementKind::Text,
                z_index: 14,
                config: GraphicsElementConfig::Text(TextGraphicsConfig {
                    text: Some("Static title".to_owned()),
                    ..TextGraphicsConfig::default()
                }),
                ..GraphicsElement::default()
            },
            variables: None,
        },
    ];
    let context = PlaybackGraphicsRenderContext {
        channel_id: 120,
        channel_number: "120".to_owned(),
        media_item_id: 10,
        media_path: "/media/movie.mkv".to_owned(),
        channel_start_time: common::fixed_time(),
        content_start_time: common::fixed_time(),
        stream_seek_seconds: 0.0,
        duration_seconds: 3_600.0,
        frame_size: FrameSize::new(1920, 1080),
        square_pixel_frame_size: FrameSize::new(1920, 1080),
        frame_rate: Some(FrameRate::DEFAULT),
    };

    let artifacts =
        render_playback_graphics_overlays(&pool, &graphics, &context, output_dir.path())
            .await
            .unwrap();

    assert_eq!(artifacts.overlays.len(), 1);
    assert!(artifacts.dynamic_text_controllers.is_empty());
    assert!(matches!(
        artifacts.overlays[0],
        GraphicsPipelineOverlay::Drawvg { z_index: 14, .. }
    ));
    artifacts.shutdown().await;
}

#[tokio::test]
async fn render_playback_graphics_overlays_bounds_hold_motion_when_probe_fails() {
    let pool = common::test_db().await;
    seed_movie_with_subtitles(&pool).await;
    let output_dir = std::env::temp_dir().join(format!(
        "ersatzrs-infra-graphics-hold-render-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&output_dir);
    let missing_motion = output_dir.join("missing-motion.webm");

    let graphics = vec![ResolvedPlaybackGraphicsElement {
        element: GraphicsElement {
            id: 702,
            name: "hold".to_owned(),
            kind: GraphicsElementKind::Motion,
            path: Some("templates/graphics-elements/hold.yml".to_owned()),
            z_index: 14,
            config: GraphicsElementConfig::Motion(MotionGraphicsConfig {
                video_path: Some(missing_motion.to_string_lossy().into_owned()),
                start_seconds: Some(4.0),
                hold_seconds: Some(2.5),
                end_behavior: MotionEndBehavior::Hold,
            }),
            ..GraphicsElement::default()
        },
        variables: None,
    }];
    let context = PlaybackGraphicsRenderContext {
        channel_id: 120,
        channel_number: "120".to_owned(),
        media_item_id: 10,
        media_path: "/media/movie.mkv".to_owned(),
        channel_start_time: common::fixed_time(),
        content_start_time: common::fixed_time(),
        stream_seek_seconds: 0.0,
        duration_seconds: 3600.0,
        frame_size: FrameSize::new(1920, 1080),
        square_pixel_frame_size: FrameSize::new(1920, 1080),
        frame_rate: Some(FrameRate::DEFAULT),
    };

    let artifacts = render_playback_graphics_overlays(&pool, &graphics, &context, &output_dir)
        .await
        .unwrap();

    assert_eq!(artifacts.overlays.len(), 1);
    let GraphicsPipelineOverlay::NativeImage {
        z_index,
        input,
        overlay,
    } = &artifacts.overlays[0]
    else {
        panic!("expected native motion overlay");
    };
    assert_eq!(*z_index, 14);
    assert_eq!(input.path, missing_motion.to_string_lossy());
    assert_eq!(input.filter_steps, vec!["setpts=PTS+4/TB"]);
    assert_eq!(overlay.enable.as_deref(), Some("between(t,4,6.5)"));
    assert!(!overlay.eof_action_pass);

    artifacts.shutdown().await;
    let _ = std::fs::remove_dir_all(output_dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires ffmpeg with drawvg and libass on PATH"]
async fn scheduled_yaml_graphics_composite_all_rendered_kinds_in_final_video() {
    let ffmpeg = ersatzrs_ffmpeg::ffmpeg_path_from_env();
    if !ffmpeg_has_drawvg(&ffmpeg).await {
        eprintln!("skipping: ffmpeg drawvg filter not available");
        return;
    }

    let pool = common::test_db().await;
    seed_movie_with_subtitles(&pool).await;
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 121).await;
    seed_playout_item(&pool, 121, None, 1010, 10).await;

    let temp = tempfile::tempdir().expect("tempdir");
    let assets = ScheduledGraphicsAssets::create(&ffmpeg, temp.path()).await;
    import_scheduled_graphics_yaml(&repos.graphics, &assets).await;
    attach_scheduled_graphics(&pool, 1010).await;

    let selected = select_scheduled_graphics(&pool).await;
    let artifacts = render_scheduled_graphics(&pool, &selected, &assets, temp.path()).await;
    assert_eq!(
        artifacts
            .overlays
            .iter()
            .map(graphics_z_index)
            .collect::<Vec<_>>(),
        vec![10, 20, 30, 40]
    );

    let stream = encode_scheduled_graphics_stream(&assets.input, artifacts.overlays.clone()).await;
    artifacts.shutdown().await;
    let stream_path = temp.path().join("scheduled-graphics.ts");
    std::fs::write(&stream_path, stream).expect("write scheduled graphics stream");

    assert_rgb_pixel(&ffmpeg, &stream_path, 16, 16, [255, 0, 0]).await;
    assert_rgb_pixel(&ffmpeg, &stream_path, 84, 32, [255, 255, 255]).await;
    assert_rgb_pixel(&ffmpeg, &stream_path, 170, 140, [0, 255, 0]).await;
    assert_rgb_pixel(&ffmpeg, &stream_path, 304, 16, [0, 0, 255]).await;
}

async fn seed_playout_item(
    pool: &SqlitePool,
    channel_id: i64,
    deco_id: Option<i64>,
    playout_item_id: i64,
    media_item_id: i64,
) {
    sqlx::query(
        r#"INSERT INTO "Playout" (
               "Id", "ChannelId", "DecoId", "ScheduleKind", "Seed"
           ) VALUES (?, ?, ?, 0, 1)"#,
    )
    .bind(channel_id)
    .bind(channel_id)
    .bind(deco_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "PlayoutItem" (
               "Id", "PlayoutId", "MediaItemId", "Start", "Finish", "InPoint", "OutPoint",
               "DisableWatermarks", "FillerKind", "GuideGroup"
           ) VALUES (?, ?, ?, '2026-04-15 10:00:00', '2026-04-15 11:00:00',
               '00:00:00', '01:00:00', 0, 0, 0)"#,
    )
    .bind(playout_item_id)
    .bind(channel_id)
    .bind(media_item_id)
    .execute(pool)
    .await
    .unwrap();
}

struct ScheduledGraphicsAssets {
    input: PathBuf,
    image: PathBuf,
    motion: PathBuf,
    subtitle_template: PathBuf,
}

impl ScheduledGraphicsAssets {
    async fn create(ffmpeg: &str, dir: &Path) -> Self {
        let input = dir.join("black.mp4");
        let image = dir.join("Movie.ppm");
        let motion = dir.join("blue.mp4");
        let subtitle_template = dir.join("subtitle-template.ass");

        generate_color_video(ffmpeg, "black", "320x180", &input).await;
        generate_color_video(ffmpeg, "blue", "32x32", &motion).await;
        write_ppm(&image, 32, 32, [255, 0, 0]);
        std::fs::write(&subtitle_template, subtitle_box_ass()).expect("write subtitle template");

        Self {
            input,
            image,
            motion,
            subtitle_template,
        }
    }
}

async fn ffmpeg_has_drawvg(ffmpeg: &str) -> bool {
    Command::new(ffmpeg)
        .args(["-hide_banner", "-filters"])
        .output()
        .await
        .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).contains("drawvg"))
}

async fn generate_color_video(ffmpeg: &str, color: &str, size: &str, output: &Path) {
    let color_filter = format!("color=c={color}:s={size}:r=25:d=1");
    let generated = Command::new(ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            &color_filter,
            "-f",
            "lavfi",
            "-i",
            "anullsrc=channel_layout=stereo:sample_rate=48000",
            "-t",
            "1",
            "-shortest",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            output.to_string_lossy().as_ref(),
        ])
        .output()
        .await
        .expect("generate graphics proof video");
    assert!(
        generated.status.success(),
        "ffmpeg input generation failed: {}",
        String::from_utf8_lossy(&generated.stderr)
    );
}

fn write_ppm(path: &Path, width: usize, height: usize, rgb: [u8; 3]) {
    let mut ppm = format!("P6\n{width} {height}\n255\n").into_bytes();
    ppm.extend(std::iter::repeat_n(rgb, width * height).flatten());
    std::fs::write(path, ppm).expect("write ppm");
}

fn subtitle_box_ass() -> &'static str {
    r"[Script Info]
ScriptType: v4.00+
ScaledBorderAndShadow: yes
PlayResX: 320
PlayResY: 180

[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding
Style: Box,Arial,20,&H0000FF00,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,0,0,7,0,0,0,1

[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
Dialogue: 0,0:00:00.00,0:00:01.00,Box,,0,0,0,,{\p1\pos(150,120)\c&H0000FF00&}m 0 0 l 40 0 40 40 0 40
"
}

async fn import_scheduled_graphics_yaml(
    repository: &impl GraphicsRepository,
    assets: &ScheduledGraphicsAssets,
) {
    upsert_graphics_element_from_yaml(
        repository,
        801,
        GraphicsElementKind::Image,
        "graphics/image.yml",
        &format!(
            "name: image\nimage: '{}'\nepg_entries: 1\nlocation: TopLeft\nscale: false\nzIndex: 10\n",
            assets.image.with_file_name("{{ Epg[0].Title }}.ppm").display()
        ),
    )
    .await
    .expect("import image graphics yaml");
    upsert_graphics_element_from_yaml(
        repository,
        802,
        GraphicsElementKind::Text,
        "graphics/text.yml",
        "name: text\ntext: T\nlocationX: 80\nlocationY: 28\nzIndex: 20\nstyles:\n  - name: base\n    font_size: 56\n    text_color: '#ffffff'\nbaseStyle: base\n",
    )
    .await
    .expect("import text graphics yaml");
    upsert_graphics_element_from_yaml(
        repository,
        803,
        GraphicsElementKind::Subtitle,
        "graphics/subtitle.yml",
        &format!(
            "name: subtitle\ntemplate: '{}'\nzIndex: 30\n",
            assets.subtitle_template.display()
        ),
    )
    .await
    .expect("import subtitle graphics yaml");
    upsert_graphics_element_from_yaml(
        repository,
        804,
        GraphicsElementKind::Motion,
        "graphics/motion.yml",
        &format!(
            "name: motion\nvideoPath: '{}'\nlocation: TopRight\nscale: false\nstartSeconds: 0\nendBehavior: Loop\nzIndex: 40\n",
            assets.motion.display()
        ),
    )
    .await
    .expect("import motion graphics yaml");
}

async fn attach_scheduled_graphics(pool: &SqlitePool, playout_item_id: i64) {
    sqlx::query(
        r#"INSERT INTO "PlayoutItemGraphicsElement" ("PlayoutItemId", "GraphicsElementId", "Variables")
           VALUES (?, 801, '{"Slot":"image"}'), (?, 802, '{"Slot":"text"}'),
                  (?, 803, '{"Slot":"subtitle"}'), (?, 804, '{"Slot":"motion"}')"#,
    )
    .bind(playout_item_id)
    .bind(playout_item_id)
    .bind(playout_item_id)
    .bind(playout_item_id)
    .execute(pool)
    .await
    .expect("attach scheduled graphics");
}

async fn select_scheduled_graphics(pool: &SqlitePool) -> Vec<ResolvedPlaybackGraphicsElement> {
    select_playback_graphics(
        pool,
        &Channel {
            id: 121,
            streaming_mode: StreamingMode::TransportStream,
            ..Channel::default()
        },
        1010,
        common::fixed_time(),
    )
    .await
    .expect("select scheduled graphics")
}

async fn render_scheduled_graphics(
    pool: &SqlitePool,
    selected: &[ResolvedPlaybackGraphicsElement],
    assets: &ScheduledGraphicsAssets,
    temp: &Path,
) -> PlaybackGraphicsArtifacts {
    render_playback_graphics_overlays(
        pool,
        selected,
        &PlaybackGraphicsRenderContext {
            channel_id: 121,
            channel_number: "121".to_owned(),
            media_item_id: 10,
            media_path: assets.input.to_string_lossy().into_owned(),
            channel_start_time: common::fixed_time(),
            content_start_time: common::fixed_time(),
            stream_seek_seconds: 0.0,
            duration_seconds: 1.0,
            frame_size: FrameSize::new(320, 180),
            square_pixel_frame_size: FrameSize::new(320, 180),
            frame_rate: Some(FrameRate::new(25, 1)),
        },
        temp.join("rendered"),
    )
    .await
    .expect("render scheduled graphics overlays")
}

fn graphics_z_index(overlay: &GraphicsPipelineOverlay) -> i32 {
    match overlay {
        GraphicsPipelineOverlay::Drawvg { z_index, .. }
        | GraphicsPipelineOverlay::Drawtext { z_index, .. }
        | GraphicsPipelineOverlay::NativeImage { z_index, .. } => *z_index,
    }
}

async fn encode_scheduled_graphics_stream(
    input: &Path,
    overlays: Vec<GraphicsPipelineOverlay>,
) -> Vec<u8> {
    let mut desired = DesiredVideoState::h264_1080p();
    desired.scaled_size = FrameSize::new(320, 180);
    desired.frame_size = FrameSize::new(320, 180);
    desired.frame_rate = Some(FrameRate::new(25, 1));
    desired.video_bitrate = Some(1_000);
    desired.video_buffer_size = Some(2_000);

    let mut request = PipelineRequest::new(InputFile::video(input.to_string_lossy()), desired)
        .with_source_video_stream(SourceVideoStream::new(SourceVideoCodec::H264));
    request.graphics_overlays = overlays;
    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    let mut command = pipeline.command(&request.inputs);
    let output_url_index = command.arguments.len().saturating_sub(1);
    command.arguments.insert(output_url_index, "-t".to_owned());
    command
        .arguments
        .insert(output_url_index + 1, "1".to_owned());
    let mut process = Command::new(&command.executable);
    process
        .args(&command.arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    for variable in command.environment {
        process.env(variable.name, variable.value);
    }
    let output = process
        .output()
        .await
        .expect("run scheduled graphics stream");
    assert!(
        output.status.success(),
        "ffmpeg scheduled graphics failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !output.stdout.is_empty(),
        "expected scheduled MPEG-TS bytes"
    );
    output.stdout
}

async fn assert_rgb_pixel(ffmpeg: &str, stream: &Path, x: u32, y: u32, expected: [u8; 3]) {
    let crop = format!("format=rgb24,crop=1:1:{x}:{y}");
    let decoded = Command::new(ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-i",
            stream.to_string_lossy().as_ref(),
            "-vf",
            &crop,
            "-frames:v",
            "1",
            "-f",
            "rawvideo",
            "pipe:1",
        ])
        .output()
        .await
        .expect("decode scheduled graphics pixel");
    assert!(
        decoded.status.success(),
        "ffmpeg scheduled pixel decode failed: {}",
        String::from_utf8_lossy(&decoded.stderr)
    );
    assert_eq!(decoded.stdout.len(), 3);
    assert!(
        rgb_close(&decoded.stdout, expected),
        "expected rgb near {expected:?} at ({x},{y}), got {:?}",
        decoded.stdout
    );
}

fn rgb_close(actual: &[u8], expected: [u8; 3]) -> bool {
    actual
        .iter()
        .zip(expected)
        .all(|(actual, expected)| actual.abs_diff(expected) <= 90)
}

async fn seed_graphics_element(
    repos: &ersatzrs_infra::repositories::Repositories,
    id: i64,
    name: &str,
    z_index: i32,
) {
    repos
        .graphics
        .upsert_element(&GraphicsElement {
            id,
            name: name.to_owned(),
            kind: GraphicsElementKind::Image,
            z_index,
            config: GraphicsElementConfig::Image(ImageGraphicsConfig {
                image: Some(format!("{name}.png")),
            }),
            ..GraphicsElement::default()
        })
        .await
        .unwrap();
}

async fn seed_graphics_deco(
    pool: &SqlitePool,
    id: i64,
    graphics_mode: i32,
    during_filler: bool,
    graphics_element_id: i64,
) {
    sqlx::query(r#"INSERT OR IGNORE INTO "DecoGroup" ("Id", "Name") VALUES (1, 'group')"#)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO "Deco" (
               "Id", "DecoGroupId", "Name", "BreakContentMode", "DeadAirFallbackCollectionType",
               "DeadAirFallbackMode", "DefaultFillerCollectionType", "DefaultFillerMode",
               "DefaultFillerTrimToFit", "GraphicsElementsMode", "UseGraphicsElementsDuringFiller",
               "UseWatermarkDuringFiller", "WatermarkMode"
           ) VALUES (?, 1, ?, 0, 0, 0, 0, 0, 0, ?, ?, 0, 0)"#,
    )
    .bind(id)
    .bind(format!("graphics deco {id}"))
    .bind(graphics_mode)
    .bind(during_filler)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "DecoGraphicsElement" ("DecoId", "GraphicsElementId")
           VALUES (?, ?)"#,
    )
    .bind(id)
    .bind(graphics_element_id)
    .execute(pool)
    .await
    .unwrap();
}

async fn set_playout_item_filler_kind(
    pool: &SqlitePool,
    playout_item_id: i64,
    filler_kind: FillerKind,
) {
    let filler_kind = match filler_kind {
        FillerKind::None => 0,
        FillerKind::PreRoll => 1,
        FillerKind::MidRoll => 2,
        FillerKind::PostRoll => 3,
        FillerKind::Tail => 4,
        FillerKind::Fallback => 5,
        FillerKind::GuideMode => 6,
        FillerKind::DecoDefault => 7,
    };
    sqlx::query(r#"UPDATE "PlayoutItem" SET "FillerKind" = ? WHERE "Id" = ?"#)
        .bind(filler_kind)
        .bind(playout_item_id)
        .execute(pool)
        .await
        .unwrap();
}

async fn seed_custom_watermark(pool: &SqlitePool, id: i64, name: &str, z_index: i32) -> PathBuf {
    let filename = format!("watermark-test-{id}.png");
    let path = write_test_file(data_path(Path::new("watermarks").join(&filename)));
    seed_watermark(
        pool,
        id,
        name,
        ChannelWatermarkImageSource::Custom,
        Some(&filename),
        z_index,
    )
    .await;
    path
}

async fn seed_watermark(
    pool: &SqlitePool,
    id: i64,
    name: &str,
    image_source: ChannelWatermarkImageSource,
    image: Option<&str>,
    z_index: i32,
) {
    let image_source = match image_source {
        ChannelWatermarkImageSource::Custom => 0,
        ChannelWatermarkImageSource::ChannelLogo => 1,
        ChannelWatermarkImageSource::Resource => 100,
    };
    sqlx::query(
        r#"INSERT INTO "ChannelWatermark" (
               "Id", "Name", "Mode", "ImageSource", "Image", "Location", "Size",
               "WidthPercent", "HorizontalMarginPercent", "VerticalMarginPercent",
               "FrequencyMinutes", "DurationSeconds", "Opacity", "PlaceWithinSourceContent",
               "ZIndex"
           ) VALUES (?, ?, 1, ?, ?, 0, 0, 10.0, 2.0, 2.0, 15, 15, 100, 0, ?)"#,
    )
    .bind(id)
    .bind(name)
    .bind(image_source)
    .bind(image)
    .bind(z_index)
    .execute(pool)
    .await
    .unwrap();
}

async fn set_channel_watermark(pool: &SqlitePool, channel_id: i64, watermark_id: i64) {
    sqlx::query(r#"UPDATE "Channel" SET "WatermarkId" = ? WHERE "Id" = ?"#)
        .bind(watermark_id)
        .bind(channel_id)
        .execute(pool)
        .await
        .unwrap();
}

async fn seed_deco(
    pool: &SqlitePool,
    id: i64,
    watermark_mode: i32,
    during_filler: bool,
    watermark_id: i64,
) {
    sqlx::query(r#"INSERT OR IGNORE INTO "DecoGroup" ("Id", "Name") VALUES (1, 'group')"#)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO "Deco" (
               "Id", "DecoGroupId", "Name", "BreakContentMode", "DeadAirFallbackCollectionType",
               "DeadAirFallbackMode", "DefaultFillerCollectionType", "DefaultFillerMode",
               "DefaultFillerTrimToFit", "GraphicsElementsMode", "UseGraphicsElementsDuringFiller",
               "UseWatermarkDuringFiller", "WatermarkMode"
           ) VALUES (?, 1, ?, 0, 0, 0, 0, 0, 0, 0, 0, ?, ?)"#,
    )
    .bind(id)
    .bind(format!("deco {id}"))
    .bind(during_filler)
    .bind(watermark_mode)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(r#"INSERT INTO "DecoWatermark" ("DecoId", "WatermarkId") VALUES (?, ?)"#)
        .bind(id)
        .bind(watermark_id)
        .execute(pool)
        .await
        .unwrap();
}

async fn seed_active_deco_template(pool: &SqlitePool, playout_id: i64, deco_id: i64) {
    sqlx::query(r#"INSERT INTO "TemplateGroup" ("Id", "Name") VALUES (501, 'templates')"#)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO "Template" ("Id", "TemplateGroupId", "Name", "DateUpdated")
           VALUES (502, 501, 'template', '2026-04-15 00:00:00')"#,
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(r#"INSERT INTO "DecoTemplateGroup" ("Id", "Name") VALUES (503, 'deco templates')"#)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO "DecoTemplate" ("Id", "DecoTemplateGroupId", "Name", "DateUpdated")
           VALUES (504, 503, 'deco template', '2026-04-15 00:00:00')"#,
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "DecoTemplateItem" ("Id", "DecoTemplateId", "DecoId", "StartTime", "EndTime")
           VALUES (505, 504, ?, '09:00:00', '11:00:00')"#,
    )
    .bind(deco_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "PlayoutTemplate" (
               "Id", "PlayoutId", "TemplateId", "DateUpdated", "DecoTemplateId", "Index",
               "LimitToDateRange", "StartMonth", "StartDay", "EndMonth", "EndDay"
           ) VALUES (506, ?, 502, '2026-04-15 00:00:00', 504, 0, 0, 1, 1, 12, 31)"#,
    )
    .bind(playout_id)
    .execute(pool)
    .await
    .unwrap();
}

#[allow(clippy::too_many_arguments)]
async fn seed_filtered_deco_template(
    pool: &SqlitePool,
    playout_id: i64,
    deco_id: i64,
    template_group_id: i64,
    template_id: i64,
    deco_template_group_id: i64,
    deco_template_id: i64,
    deco_template_item_id: i64,
    index: i32,
    year: i32,
) {
    sqlx::query(r#"INSERT INTO "TemplateGroup" ("Id", "Name") VALUES (?, ?)"#)
        .bind(template_group_id)
        .bind(format!("templates {template_group_id}"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO "Template" ("Id", "TemplateGroupId", "Name", "DateUpdated")
           VALUES (?, ?, ?, '2026-04-15 00:00:00')"#,
    )
    .bind(template_id)
    .bind(template_group_id)
    .bind(format!("template {template_id}"))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(r#"INSERT INTO "DecoTemplateGroup" ("Id", "Name") VALUES (?, ?)"#)
        .bind(deco_template_group_id)
        .bind(format!("deco templates {deco_template_group_id}"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO "DecoTemplate" ("Id", "DecoTemplateGroupId", "Name", "DateUpdated")
           VALUES (?, ?, ?, '2026-04-15 00:00:00')"#,
    )
    .bind(deco_template_id)
    .bind(deco_template_group_id)
    .bind(format!("deco template {deco_template_id}"))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "DecoTemplateItem" ("Id", "DecoTemplateId", "DecoId", "StartTime", "EndTime")
           VALUES (?, ?, ?, '09:00:00', '11:00:00')"#,
    )
    .bind(deco_template_item_id)
    .bind(deco_template_id)
    .bind(deco_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "PlayoutTemplate" (
               "Id", "PlayoutId", "TemplateId", "DateUpdated", "DecoTemplateId", "Index",
               "DaysOfWeek", "DaysOfMonth", "MonthsOfYear",
               "LimitToDateRange", "StartMonth", "StartDay", "StartYear",
               "EndMonth", "EndDay", "EndYear"
           ) VALUES (?, ?, ?, '2026-04-15 00:00:00', ?, ?, '[3]', '[15]', '[4]',
               1, 4, 15, ?, 4, 15, ?)"#,
    )
    .bind(template_id + 10_000)
    .bind(playout_id)
    .bind(template_id)
    .bind(deco_template_id)
    .bind(index)
    .bind(year)
    .bind(year)
    .execute(pool)
    .await
    .unwrap();
}

fn write_test_file(path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref().to_path_buf();
    std::fs::create_dir_all(path.parent().expect("test path parent")).unwrap();
    std::fs::write(&path, [0x89, b'P', b'N', b'G']).unwrap();
    path
}

fn cleanup_files(paths: impl IntoIterator<Item = PathBuf>) {
    for path in paths {
        let _ = std::fs::remove_file(path);
    }
}
