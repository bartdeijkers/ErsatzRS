#![cfg(not(feature = "mysql"))]
mod common;

use ersatzrs_core::repositories::{AddonMediaListRepository, RepositoryError};
use ersatzrs_domain::AddonMediaListPresentationKind;
use ersatzrs_infra::repositories::make_repositories;
use sqlx::Executor;

#[tokio::test]
async fn addon_media_list_repository_lists_strict_rows_and_persists_page_size() {
    let pool = common::test_db().await;
    let repositories = make_repositories(pool.clone());
    seed_addon(&pool).await;
    pool.execute(
        r#"INSERT INTO "AddonMediaList" (
            "Id", "AddonId", "ProviderId", "SourceUrl", "Name",
            "AutoRefresh", "GeneratePlaylist", "ItemCount"
           ) VALUES (
            1, 'org.example.lists', 'alice/watchlist',
            'https://example.invalid/alice/watchlist', 'Watchlist', 1, 0, 2
           )"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "AddonMediaListItem" (
            "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind",
            "Rank", "Title", "MediaItemId"
           ) VALUES (10, 1, 'movie:100', 'Movie', 0, 1, 'Movie', NULL)"#,
    )
    .await
    .unwrap();

    repositories
        .addon_media_lists
        .set_page_size(25)
        .await
        .unwrap();
    assert_eq!(
        repositories
            .addon_media_lists
            .get_page_size()
            .await
            .unwrap(),
        25
    );

    let (rows, total) = repositories
        .addon_media_lists
        .list_admin(0, 25)
        .await
        .unwrap();
    assert_eq!(total, 1);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].addon_id, "org.example.lists");
    assert_eq!(rows[0].provider_id, "alice/watchlist");
    assert_eq!(rows[0].match_count, 0);
    assert!(rows[0].auto_refresh);
    assert!(!rows[0].generate_playlist);
    assert_eq!(
        rows[0].presentation_kind,
        AddonMediaListPresentationKind::RemoteStreams
    );

    let form = repositories
        .addon_media_lists
        .get_admin_form(1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(form.addon_id, "org.example.lists");
    assert_eq!(form.source_url, "https://example.invalid/alice/watchlist");
    assert_eq!(
        form.requested_presentation_kind,
        AddonMediaListPresentationKind::RemoteStreams
    );
}

#[tokio::test]
async fn addon_media_list_repository_propagates_closed_pool_failures() {
    let pool = common::test_db().await;
    let repositories = make_repositories(pool.clone());
    pool.close().await;

    assert!(matches!(
        repositories.addon_media_lists.list_admin(0, 10).await,
        Err(RepositoryError::Unavailable { .. })
    ));
}

async fn seed_addon(pool: &sqlx::SqlitePool) {
    pool.execute(
        r#"INSERT INTO "AddonInstallation" (
            "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
            "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt"
        ) VALUES (
            'org.example.lists', '1.0.0', 1, 'package', 'content', '{}',
            'addons/org.example.lists', '2026-08-14T00:00:00Z', '2026-08-14T00:00:00Z'
        )"#,
    )
    .await
    .unwrap();
}

/// The Local replacement picker matches a candidate on its persisted video
/// stream. ErsatzTV's `MediaStreamKind` is 1-based (`Video = 1`), so a fixture
/// or scanner that writes anything else makes the search return nothing at all
/// — which is how this feature once shipped completely unusable, and how a
/// browser spec seeded an invalid `0` and still looked like a product bug.
#[tokio::test]
async fn replacement_candidates_match_the_persisted_video_stream_kind() {
    let pool = common::test_db().await;
    let repositories = make_repositories(pool.clone());
    seed_local_movie(
        &pool,
        1,
        "Replacement Candidate",
        ersatztv_video_stream_kind(),
    )
    .await;

    let candidates = repositories
        .addon_media_lists
        .search_replacement_candidates("Replacement Candidate", 10)
        .await
        .unwrap();
    assert_eq!(
        candidates.len(),
        1,
        "the seeded local movie must be offered"
    );
    assert_eq!(candidates[0].title, "Replacement Candidate");
}

/// The same search must ignore an item whose only stream is audio, so a
/// music-only item is never offered as a video replacement.
#[tokio::test]
async fn replacement_candidates_ignore_items_without_a_video_stream() {
    let pool = common::test_db().await;
    let repositories = make_repositories(pool.clone());
    seed_local_movie(&pool, 1, "Audio Only", ersatztv_audio_stream_kind()).await;

    let candidates = repositories
        .addon_media_lists
        .search_replacement_candidates("Audio Only", 10)
        .await
        .unwrap();
    assert!(
        candidates.is_empty(),
        "an item with no video stream must not be offered: {candidates:?}"
    );
}

/// `ErsatzTV.Core/Domain/MediaItem/MediaStreamKind.cs` — the persisted enum is
/// 1-based, so `Video` is 1 and never 0.
const fn ersatztv_video_stream_kind() -> i32 {
    1
}

const fn ersatztv_audio_stream_kind() -> i32 {
    2
}

async fn seed_local_movie(pool: &sqlx::SqlitePool, id: i64, title: &str, stream_kind: i32) {
    pool.execute(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
        .await
        .unwrap();
    pool.execute(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (1)"#)
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "Library" ("Id", "MediaKind", "MediaSourceId", "Name")
           VALUES (1, 0, 1, 'Movies')"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId") VALUES (1, '/movies', 1)"#,
    )
    .await
    .unwrap();
    sqlx::query(r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (?, 1, 0)"#)
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "Movie" ("Id") VALUES (?)"#)
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO "MovieMetadata" ("MovieId", "MetadataKind", "Title", "DateAdded", "DateUpdated")
           VALUES (?, 1, ?, '2026-08-18T00:00:00Z', '2026-08-18T00:00:00Z')"#,
    )
    .bind(id)
    .bind(title)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "MediaVersion" (
              "Id", "DateAdded", "DateUpdated", "Duration", "Height", "Width",
              "VideoScanKind", "MovieId"
           ) VALUES (?, '2026-08-18T00:00:00Z', '2026-08-18T00:00:00Z', '01:30:00', 1080, 1920, 0, ?)"#,
    )
    .bind(id)
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "MediaFile" ("MediaVersionId", "Path", "PathHash") VALUES (?, ?, ?)"#,
    )
    .bind(id)
    .bind(format!("/movies/{title}.mkv"))
    .bind(format!("hash-{id}"))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "MediaStream" (
              "Index", "Codec", "MediaStreamKind", "Channels", "Default", "Forced",
              "MediaVersionId", "AttachedPic"
           ) VALUES (0, 'h264', ?, 0, 1, 0, ?, 0)"#,
    )
    .bind(stream_kind)
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
}
