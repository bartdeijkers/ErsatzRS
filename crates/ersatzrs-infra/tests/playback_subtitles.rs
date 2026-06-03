#![cfg(not(feature = "mysql"))]
mod common;

use ersatzrs_core::repositories::ConfigRepository;
use ersatzrs_domain::{
    Channel, ChannelSubtitleMode, ChannelWatermarkImageSource, StreamingMode,
    CONFIG_KEY_FFMPEG_GLOBAL_WATERMARK_ID,
};
use ersatzrs_ffmpeg::SubtitleMethod;
use ersatzrs_infra::ffmpeg_config::CONFIG_KEY_FFMPEG_USE_EMBEDDED_SUBTITLES;
use ersatzrs_infra::playback::{
    select_playback_subtitle, select_playback_watermarks, subtitle_input_for_stream,
    watermark_fade_points_for_program,
};
use ersatzrs_infra::repositories::make_repositories;
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use std::time::Duration;

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

#[tokio::test]
async fn selects_forced_subtitle_by_expanded_language_code() {
    let pool = common::test_db().await;
    seed_movie_with_subtitles(&pool).await;

    let selected = select_playback_subtitle(
        &pool,
        10,
        Some("nl"),
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
    assert_eq!(input.path, "/media/movie.nl.forced.srt");
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
        Some("eng"),
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
        Some("jpn"),
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
    let logo = write_test_file("data/iptv/logos/watermark-test-logo.png");
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
        PathBuf::from("data/iptv/logos/watermark-test-logo.png")
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

async fn seed_custom_watermark(pool: &SqlitePool, id: i64, name: &str, z_index: i32) -> PathBuf {
    let filename = format!("watermark-test-{id}.png");
    let path = write_test_file(Path::new("data/watermarks").join(&filename));
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
