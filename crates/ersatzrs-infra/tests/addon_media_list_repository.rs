#![cfg(not(feature = "mysql"))]
mod common;

use ersatzrs_core::repositories::{AddonMediaListRepository, RepositoryError};
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

    let form = repositories
        .addon_media_lists
        .get_admin_form(1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(form.addon_id, "org.example.lists");
    assert_eq!(form.source_url, "https://example.invalid/alice/watchlist");
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
