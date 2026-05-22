#![cfg(not(feature = "mysql"))]
mod common;

use ersatzrs_core::repositories::TelevisionRepository;
use ersatzrs_infra::repositories::make_repositories;
use sqlx::{Executor, SqlitePool};

#[tokio::test]
async fn television_repository_reads_season_episodes_and_episode_media_info() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_tv_fixture(&pool).await;

    let season = repos.television.get_season(20).await.unwrap().unwrap();
    assert_eq!(season.id, 20);
    assert_eq!(season.show_id, 10);
    assert_eq!(season.show_title, "Example Show");
    assert_eq!(season.show_year, Some(2026));
    assert_eq!(season.season_number, 1);
    assert_eq!(
        season.season_poster_path.as_deref(),
        Some("season-poster.jpg")
    );
    assert_eq!(season.show_fanart_path.as_deref(), Some("show-fanart.jpg"));

    assert_eq!(repos.television.get_episode_count(20).await.unwrap(), 2);

    let episodes = repos
        .television
        .get_paged_episodes(20, 1, 100)
        .await
        .unwrap();
    assert_eq!(episodes.len(), 2);
    assert_eq!(episodes[0].episode_id, 30);
    assert_eq!(episodes[0].episode_number, 1);
    assert_eq!(episodes[0].title, "Pilot");
    assert_eq!(episodes[0].sort_title, "pilot");
    assert_eq!(
        episodes[0].thumbnail_path.as_deref(),
        Some("pilot-thumb.jpg")
    );
    assert_eq!(episodes[0].state, 1);
    assert_eq!(
        episodes[0].file_path.as_deref(),
        Some("/media/show/s01e01.mkv")
    );
    assert_eq!(
        episodes[0].local_path.as_deref(),
        Some("/media/show/s01e01.mkv")
    );
    assert_eq!(episodes[0].directors, vec!["Director A", "Director B"]);
    assert_eq!(episodes[0].writers, vec!["Writer A"]);
    assert_eq!(episodes[1].episode_id, 31);
    assert_eq!(episodes[1].episode_number, 2);

    let paged = repos.television.get_paged_episodes(20, 2, 1).await.unwrap();
    assert_eq!(paged.len(), 1);
    assert_eq!(paged[0].episode_id, 31);

    let info = repos
        .television
        .get_episode_media_item_info(30)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(info.id, 30);
    assert_eq!(info.title, "Pilot");
    assert_eq!(info.show_title, "Example Show");
    assert_eq!(info.season_number, 1);
    assert_eq!(info.episode_number, 1);
    assert_eq!(info.metadata_id, 300);
    assert_eq!(info.library_kind, "LocalLibrary");
    assert_eq!(info.library_name, "TV");
    assert_eq!(info.state, 1);
    assert_eq!(info.media_version_id, Some(3000));
    assert_eq!(info.duration.as_deref(), Some("00:22:00"));
    assert_eq!(info.width, 1920);
    assert_eq!(info.height, 1080);
    assert!(info.is_local_library);
}

#[tokio::test]
async fn television_repository_paginates_unique_episodes_when_versions_have_multiple_files() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_tv_fixture(&pool).await;

    let episodes = repos
        .television
        .get_paged_episodes(20, 1, 100)
        .await
        .unwrap();
    assert_eq!(
        episodes
            .iter()
            .map(|episode| episode.episode_id)
            .collect::<Vec<_>>(),
        vec![30, 31]
    );

    let second_page = repos.television.get_paged_episodes(20, 2, 1).await.unwrap();
    assert_eq!(second_page.len(), 1);
    assert_eq!(second_page[0].episode_id, 31);
}

#[tokio::test]
async fn television_repository_applies_remote_path_replacements_to_local_path() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_remote_tv_fixture(&pool).await;

    let episodes = repos
        .television
        .get_paged_episodes(120, 1, 100)
        .await
        .unwrap();
    assert_eq!(episodes.len(), 1);
    assert_eq!(
        episodes[0].file_path.as_deref(),
        Some("/plex/tv/Example Show/S01E01.mkv")
    );
    assert_eq!(
        episodes[0].local_path.as_deref(),
        Some("/mnt/tv/Example Show/S01E01.mkv")
    );
}

#[allow(clippy::too_many_lines)]
async fn seed_tv_fixture(pool: &SqlitePool) {
    let now = "2026-05-22T00:00:00Z";
    pool.execute(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
        .await
        .unwrap();
    pool.execute(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (1)"#)
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
           VALUES (1, 'TV', 1, 1)"#,
    )
    .await
    .unwrap();
    pool.execute(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (1)"#)
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId") VALUES (1, '/media', 1)"#,
    )
    .await
    .unwrap();

    for id in [10, 20, 30, 31] {
        sqlx::query(r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (?, 1, ?)"#)
            .bind(id)
            .bind(i32::from(id == 30))
            .execute(pool)
            .await
            .unwrap();
    }

    pool.execute(r#"INSERT INTO "Show" ("Id") VALUES (10)"#)
        .await
        .unwrap();
    pool.execute(r#"INSERT INTO "Season" ("Id", "ShowId", "SeasonNumber") VALUES (20, 10, 1)"#)
        .await
        .unwrap();
    pool.execute(r#"INSERT INTO "Episode" ("Id", "SeasonId") VALUES (30, 20), (31, 20)"#)
        .await
        .unwrap();

    sqlx::query(
        r#"INSERT INTO "ShowMetadata" (
               "Id", "ShowId", "MetadataKind", "Title", "SortTitle", "Year",
               "DateAdded", "DateUpdated"
           ) VALUES (100, 10, 0, 'Example Show', 'example show', 2026, ?, ?)"#,
    )
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "SeasonMetadata" (
               "Id", "SeasonId", "MetadataKind", "Title", "SortTitle",
               "DateAdded", "DateUpdated"
           ) VALUES (200, 20, 0, 'Season 1', 'season 1', ?, ?)"#,
    )
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "EpisodeMetadata" (
               "Id", "EpisodeId", "MetadataKind", "Title", "SortTitle",
               "ReleaseDate", "DateAdded", "DateUpdated", "EpisodeNumber", "Plot"
           ) VALUES
               (300, 30, 0, 'Pilot', 'pilot', '2026-05-01T00:00:00Z', ?, ?, 1, 'The beginning.'),
               (301, 31, 0, 'Finale', 'finale', NULL, ?, ?, 2, 'The ending.')"#,
    )
    .bind(now)
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        r#"INSERT INTO "Artwork" (
               "Id", "ArtworkKind", "Path", "SeasonMetadataId", "ShowMetadataId",
               "EpisodeMetadataId", "DateAdded", "DateUpdated"
           ) VALUES
               (1, 0, 'season-poster.jpg', 200, NULL, NULL, ?, ?),
               (2, 3, 'show-fanart.jpg', NULL, 100, NULL, ?, ?),
               (3, 1, 'pilot-thumb.jpg', NULL, NULL, 300, ?, ?)"#,
    )
    .bind(now)
    .bind(now)
    .bind(now)
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();

    pool.execute(
        r#"INSERT INTO "Director" ("EpisodeMetadataId", "Name")
           VALUES (300, 'Director A'), (300, 'Director B')"#,
    )
    .await
    .unwrap();
    pool.execute(r#"INSERT INTO "Writer" ("EpisodeMetadataId", "Name") VALUES (300, 'Writer A')"#)
        .await
        .unwrap();

    sqlx::query(
        r#"INSERT INTO "MediaVersion" (
               "Id", "DateAdded", "DateUpdated", "Duration", "EpisodeId",
               "Height", "Width", "VideoScanKind", "SampleAspectRatio",
               "DisplayAspectRatio", "RFrameRate"
           ) VALUES
               (3000, ?, ?, '00:22:00', 30, 1080, 1920, 1, '1:1', '16:9', '24000/1001'),
               (3001, ?, ?, '00:22:00', 30, 720, 1280, 1, '1:1', '16:9', '24000/1001')"#,
    )
    .bind(now)
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "MediaFile" ("Id", "MediaVersionId", "Path", "PathHash")
           VALUES
               (4000, 3000, '/media/show/s01e01.mkv', 'hash'),
               (4001, 3000, '/media/show/s01e01-alt-audio.mkv', 'hash-alt-audio'),
               (4002, 3001, '/media/show/s01e01-720p.mkv', 'hash-720p')"#,
    )
    .await
    .unwrap();
}

#[allow(clippy::too_many_lines)]
async fn seed_remote_tv_fixture(pool: &SqlitePool) {
    let now = "2026-05-22T00:00:00Z";
    pool.execute(r#"INSERT INTO "MediaSource" ("Id") VALUES (2)"#)
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "PlexMediaSource" ("Id", "ServerName", "Platform")
           VALUES (2, 'Plex', 'Linux')"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "PlexPathReplacement" (
               "Id", "PlexMediaSourceId", "PlexPath", "LocalPath"
           ) VALUES (2, 2, '/plex/tv', '/mnt/tv')"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
           VALUES (2, 'Remote TV', 1, 2)"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId") VALUES (2, '/plex/tv', 2)"#,
    )
    .await
    .unwrap();

    for id in [110, 120, 130] {
        sqlx::query(r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (?, 2, 2)"#)
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
    }

    pool.execute(r#"INSERT INTO "Show" ("Id") VALUES (110)"#)
        .await
        .unwrap();
    pool.execute(r#"INSERT INTO "Season" ("Id", "ShowId", "SeasonNumber") VALUES (120, 110, 1)"#)
        .await
        .unwrap();
    pool.execute(r#"INSERT INTO "Episode" ("Id", "SeasonId") VALUES (130, 120)"#)
        .await
        .unwrap();
    pool.execute(r#"INSERT INTO "PlexEpisode" ("Id", "Key") VALUES (130, 'plex-episode')"#)
        .await
        .unwrap();

    sqlx::query(
        r#"INSERT INTO "ShowMetadata" (
               "Id", "ShowId", "MetadataKind", "Title", "SortTitle", "Year",
               "DateAdded", "DateUpdated"
           ) VALUES (1100, 110, 0, 'Example Show', 'example show', 2026, ?, ?)"#,
    )
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "EpisodeMetadata" (
               "Id", "EpisodeId", "MetadataKind", "Title", "SortTitle",
               "DateAdded", "DateUpdated", "EpisodeNumber"
           ) VALUES (1300, 130, 0, 'Remote Pilot', 'remote pilot', ?, ?, 1)"#,
    )
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        r#"INSERT INTO "MediaVersion" (
               "Id", "DateAdded", "DateUpdated", "Duration", "EpisodeId",
               "Height", "Width", "VideoScanKind"
           ) VALUES (13000, ?, ?, '00:22:00', 130, 1080, 1920, 1)"#,
    )
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "MediaFile" ("Id", "MediaVersionId", "Path", "PathHash")
           VALUES (14000, 13000, '/plex/tv/Example Show/S01E01.mkv', 'remote-hash')"#,
    )
    .await
    .unwrap();
}
