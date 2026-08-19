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

/// The editor renders one section per availability group, so each group must be
/// countable and pageable on its own. A regression here shows up as a section
/// listing another group's entries, or as a count that disagrees with its rows.
#[tokio::test]
async fn item_sections_count_and_page_each_availability_group_separately() {
    use ersatzrs_domain::AddonMediaListItemAvailability;

    let pool = common::test_db().await;
    let repositories = make_repositories(pool.clone());
    seed_addon(&pool).await;
    pool.execute(
        r#"INSERT INTO "AddonMediaList" (
            "Id", "AddonId", "ProviderId", "SourceUrl", "Name",
            "AutoRefresh", "GeneratePlaylist", "ItemCount"
           ) VALUES (
            1, 'org.example.lists', 'alice/mixed',
            'https://example.invalid/alice/mixed', 'Mixed', 0, 0, 6
           )"#,
    )
    .await
    .unwrap();
    // Three unavailable, two available, one unknown, deliberately interleaved so
    // a filter that ignored availability would still return the first rows.
    for (id, rank, availability) in [
        (10, 1, 1),
        (11, 2, 0),
        (12, 3, 1),
        (13, 4, 2),
        (14, 5, 0),
        (15, 6, 1),
    ] {
        pool.execute(
            sqlx::query(
                r#"INSERT INTO "AddonMediaListItem" (
                    "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind",
                    "Rank", "Title", "MediaItemId", "Availability"
                   ) VALUES (?, 1, ?, ?, 0, ?, 'Item', NULL, ?)"#,
            )
            .bind(id)
            .bind(format!("item:{id}"))
            .bind(format!("Item {id}"))
            .bind(rank)
            .bind(availability),
        )
        .await
        .unwrap();
    }

    let counts = repositories.addon_media_lists.item_counts(1).await.unwrap();
    assert_eq!(counts.unavailable, 3);
    assert_eq!(counts.available, 2);
    assert_eq!(counts.unknown, 1);

    let first = repositories
        .addon_media_lists
        .list_items_page(1, AddonMediaListItemAvailability::Unavailable, 0, Some(2))
        .await
        .unwrap();
    assert_eq!(
        first.iter().map(|item| item.id).collect::<Vec<_>>(),
        [10, 12]
    );
    let second = repositories
        .addon_media_lists
        .list_items_page(1, AddonMediaListItemAvailability::Unavailable, 1, Some(2))
        .await
        .unwrap();
    assert_eq!(second.iter().map(|item| item.id).collect::<Vec<_>>(), [15]);

    // A missing page size means "every row in this group", which the editor
    // offers explicitly and must not silently truncate.
    let all = repositories
        .addon_media_lists
        .list_items_page(1, AddonMediaListItemAvailability::Unavailable, 0, None)
        .await
        .unwrap();
    assert_eq!(all.len(), 3);

    let unknown = repositories
        .addon_media_lists
        .list_items_page(1, AddonMediaListItemAvailability::Unknown, 0, Some(10))
        .await
        .unwrap();
    assert_eq!(unknown.iter().map(|item| item.id).collect::<Vec<_>>(), [13]);
}

/// The entry owns its classification; the list value is only a bulk default.
/// Before this, `list_items` never read the column at all, so an add-on that
/// classified every entry correctly still showed up as unclassified.
#[tokio::test]
async fn item_content_kind_is_readable_settable_and_bulk_appliable() {
    use ersatzrs_domain::{AddonMediaListContentKind, AddonMediaListItemAvailability};

    let pool = common::test_db().await;
    let repositories = make_repositories(pool.clone());
    seed_addon(&pool).await;
    pool.execute(
        r#"INSERT INTO "AddonMediaList" (
            "Id", "AddonId", "ProviderId", "SourceUrl", "Name",
            "AutoRefresh", "GeneratePlaylist", "ItemCount", "ContentKindOverride"
           ) VALUES (
            1, 'org.example.lists', 'alice/kinds',
            'https://example.invalid/alice/kinds', 'Kinds', 0, 0, 2, 0
           )"#,
    )
    .await
    .unwrap();
    // The add-on classified one entry and left the other unclassified.
    pool.execute(
        r#"INSERT INTO "AddonMediaListItem" (
            "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind",
            "Rank", "Title", "MediaItemId", "Availability", "ContentKind"
           ) VALUES
            (10, 1, 'item:10', 'Episode', 0, 1, 'Episode', NULL, 0, 1),
            (11, 1, 'item:11', 'Unknown kind', 0, 2, 'Unknown', NULL, 0, 0)"#,
    )
    .await
    .unwrap();

    let rows = repositories.addon_media_lists.list_items(1).await.unwrap();
    assert_eq!(
        rows.iter()
            .map(|item| item.content_kind)
            .collect::<Vec<_>>(),
        [
            AddonMediaListContentKind::TelevisionEpisode,
            AddonMediaListContentKind::Auto
        ]
    );

    repositories
        .addon_media_lists
        .update_item_content_kind(11, AddonMediaListContentKind::Song)
        .await
        .unwrap();
    let rows = repositories
        .addon_media_lists
        .list_items_page(1, AddonMediaListItemAvailability::Available, 0, None)
        .await
        .unwrap();
    assert_eq!(rows[1].content_kind, AddonMediaListContentKind::Song);

    // The bulk default overwrites every entry and is stored on the list too, so
    // entries enumerated later inherit it.
    let updated = repositories
        .addon_media_lists
        .apply_list_content_kind(1, AddonMediaListContentKind::Movie)
        .await
        .unwrap();
    assert_eq!(updated, 2);
    let rows = repositories.addon_media_lists.list_items(1).await.unwrap();
    assert!(rows
        .iter()
        .all(|item| item.content_kind == AddonMediaListContentKind::Movie));
    assert_eq!(
        repositories
            .addon_media_lists
            .get_admin_form(1)
            .await
            .unwrap()
            .unwrap()
            .content_kind_override,
        AddonMediaListContentKind::Movie
    );

    // A missing entry is reported rather than silently ignored.
    assert!(matches!(
        repositories
            .addon_media_lists
            .update_item_content_kind(999, AddonMediaListContentKind::Movie)
            .await,
        Err(RepositoryError::NotFound { .. })
    ));
}
