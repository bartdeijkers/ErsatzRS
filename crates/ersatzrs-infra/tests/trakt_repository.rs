#![cfg(not(feature = "mysql"))]
mod common;

use ersatzrs_core::repositories::{RepositoryError, TraktRepository};
use ersatzrs_infra::repositories::make_repositories;
use sqlx::Executor;

#[tokio::test]
async fn trakt_admin_repository_lists_strict_rows_and_persists_page_size() {
    let pool = common::test_db().await;
    let repositories = make_repositories(pool.clone());
    pool.execute(
        r#"INSERT INTO "TraktList" (
            "Id", "AutoRefresh", "GeneratePlaylist", "ItemCount", "List",
            "Name", "TraktId", "User"
           ) VALUES (1, 1, 0, 2, 'watchlist', 'Watchlist', 99, 'alice')"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "TraktListItem" (
            "Id", "TraktListId", "Kind", "Rank", "Title", "TraktId", "MediaItemId"
           ) VALUES (10, 1, 10, 1, 'Movie', 100, NULL)"#,
    )
    .await
    .unwrap();

    repositories.trakt.set_page_size(25).await.unwrap();
    assert_eq!(repositories.trakt.get_page_size().await.unwrap(), 25);

    let (rows, total) = repositories.trakt.list_admin(0, 25).await.unwrap();
    assert_eq!(total, 1);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].user, "alice");
    assert_eq!(rows[0].list, "watchlist");
    assert_eq!(rows[0].match_count, 0);
    assert!(rows[0].auto_refresh);
    assert!(!rows[0].generate_playlist);

    let form = repositories.trakt.get_admin_form(1).await.unwrap().unwrap();
    assert_eq!(form.user, "alice");
    assert_eq!(form.list, "watchlist");
}

#[tokio::test]
async fn trakt_admin_repository_propagates_closed_pool_failures() {
    let pool = common::test_db().await;
    let repositories = make_repositories(pool.clone());
    pool.close().await;

    assert!(matches!(
        repositories.trakt.list_admin(0, 10).await,
        Err(RepositoryError::Unavailable { .. })
    ));
}
