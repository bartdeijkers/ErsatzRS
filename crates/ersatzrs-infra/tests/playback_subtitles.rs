#![cfg(not(feature = "mysql"))]
mod common;

use ersatzrs_core::repositories::ConfigRepository;
use ersatzrs_domain::{ChannelSubtitleMode, StreamingMode};
use ersatzrs_ffmpeg::SubtitleMethod;
use ersatzrs_infra::ffmpeg_config::CONFIG_KEY_FFMPEG_USE_EMBEDDED_SUBTITLES;
use ersatzrs_infra::playback::{select_playback_subtitle, subtitle_input_for_stream};
use ersatzrs_infra::repositories::make_repositories;
use sqlx::SqlitePool;

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
