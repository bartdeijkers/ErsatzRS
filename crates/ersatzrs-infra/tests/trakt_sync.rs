#![cfg(not(feature = "mysql"))]

mod common;

use async_trait::async_trait;
use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_core::trakt::{TraktApiClient, TraktListItemWithGuids, TraktListLocator};
use ersatzrs_domain::{TraktList, TraktListItemKind};
use ersatzrs_infra::trakt_sync::{TraktRemoteSynchronizer, TraktSynchronizer};
use sqlx::{Executor, Row};

#[derive(Clone)]
struct FakeTraktApi;

#[async_trait]
impl TraktApiClient for FakeTraktApi {
    async fn get_user_list(&self, _user: &str, _list: &str) -> Result<TraktList> {
        Ok(TraktList {
            trakt_id: 77,
            user: "api-user".to_string(),
            list: "api-list".to_string(),
            name: "API List".to_string(),
            description: Some("From Trakt".to_string()),
            item_count: 1,
            ..TraktList::default()
        })
    }

    async fn get_user_list_items(
        &self,
        _user: &str,
        _list: &str,
    ) -> Result<Vec<TraktListItemWithGuids>> {
        Ok(vec![TraktListItemWithGuids {
            trakt_id: 100,
            rank: 1,
            display_title: "Fixture Movie (1999)".to_string(),
            title: "Fixture Movie".to_string(),
            year: Some(1999),
            season: 0,
            episode: 0,
            kind: TraktListItemKind::Movie,
            guids: vec!["tmdb://123".to_string()],
        }])
    }
}

#[derive(Clone)]
struct FailingTraktApi;

#[async_trait]
impl TraktApiClient for FailingTraktApi {
    async fn get_user_list(&self, _user: &str, _list: &str) -> Result<TraktList> {
        Err(RepositoryError::Conflict(
            "deterministic remote failure".to_string(),
        ))
    }

    async fn get_user_list_items(
        &self,
        _user: &str,
        _list: &str,
    ) -> Result<Vec<TraktListItemWithGuids>> {
        Err(RepositoryError::Conflict(
            "deterministic remote failure".to_string(),
        ))
    }
}

#[tokio::test]
async fn add_refresh_match_and_generate_playlist() {
    let pool = common::test_db().await;
    seed_system_playlist_group(&pool).await;
    common::seed_media_graph(&pool).await;
    seed_movie_with_guid(&pool).await;

    let remote = TraktRemoteSynchronizer::new(pool.clone(), FakeTraktApi);
    let list_id = remote
        .add_or_refresh(&TraktListLocator {
            user: "SomeUser".to_string(),
            list: "some-list".to_string(),
        })
        .await
        .unwrap();

    let list = sqlx::query(r#"SELECT * FROM "TraktList" WHERE "Id" = ?"#)
        .bind(list_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(list.get::<i32, _>("TraktId"), 77);
    assert_eq!(list.get::<String, _>("User"), "someuser");
    assert_eq!(list.get::<String, _>("List"), "api-list");

    let item = sqlx::query(r#"SELECT * FROM "TraktListItem" WHERE "TraktListId" = ?"#)
        .bind(list_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(item.get::<i64, _>("MediaItemId"), 1);

    TraktSynchronizer::new(pool.clone())
        .update_settings(list_id, true, true)
        .await
        .unwrap();

    let playlist_item = sqlx::query(
        r#"SELECT "pi"."CollectionType", "pi"."MediaItemId", "pi"."PlaybackOrder"
           FROM "PlaylistItem" AS "pi"
           JOIN "TraktList" AS "tl" ON "tl"."PlaylistId" = "pi"."PlaylistId"
           WHERE "tl"."Id" = ?"#,
    )
    .bind(list_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(playlist_item.get::<i32, _>("CollectionType"), 10);
    assert_eq!(playlist_item.get::<i64, _>("MediaItemId"), 1);
    assert_eq!(playlist_item.get::<i32, _>("PlaybackOrder"), 1);
}

#[tokio::test]
async fn local_matching_requires_no_remote_capability_and_maintains_playlist_lifecycle() {
    let pool = common::test_db().await;
    seed_system_playlist_group(&pool).await;
    common::seed_media_graph(&pool).await;
    seed_movie_with_guid(&pool).await;
    seed_local_trakt_list(&pool, 50, true).await;

    let local = TraktSynchronizer::new(pool.clone());
    assert_eq!(local.match_due_lists().await.unwrap(), 1);

    let item = sqlx::query(r#"SELECT "MediaItemId" FROM "TraktListItem" WHERE "TraktListId" = 50"#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(item.get::<i64, _>("MediaItemId"), 1);
    let last_match: Option<String> =
        sqlx::query_scalar(r#"SELECT "LastMatch" FROM "TraktList" WHERE "Id" = 50"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(last_match.is_some());

    local.update_settings(50, true, true).await.unwrap();
    let playlist_id: Option<i64> =
        sqlx::query_scalar(r#"SELECT "PlaylistId" FROM "TraktList" WHERE "Id" = 50"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    let playlist_id = playlist_id.expect("generated playlist");
    let ranks: Vec<i64> = sqlx::query_scalar(
        r#"SELECT "Index" FROM "PlaylistItem" WHERE "PlaylistId" = ? ORDER BY "Index""#,
    )
    .bind(playlist_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(ranks, vec![3]);

    local.update_settings(50, true, false).await.unwrap();
    let linked_playlist: Option<i64> =
        sqlx::query_scalar(r#"SELECT "PlaylistId" FROM "TraktList" WHERE "Id" = 50"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(linked_playlist, None);
    let playlist_count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Playlist" WHERE "Id" = ?"#)
            .bind(playlist_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(playlist_count, 0);

    local.update_settings(50, false, true).await.unwrap();
    let replacement_playlist_id: Option<i64> =
        sqlx::query_scalar(r#"SELECT "PlaylistId" FROM "TraktList" WHERE "Id" = 50"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    let replacement_playlist_id = replacement_playlist_id.expect("replacement playlist");
    local.delete_list(50).await.unwrap();
    let list_count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "TraktList" WHERE "Id" = 50"#)
        .fetch_one(&pool)
        .await
        .unwrap();
    let playlist_count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Playlist" WHERE "Id" = ?"#)
            .bind(replacement_playlist_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((list_count, playlist_count), (0, 0));
}

#[tokio::test]
async fn remote_failure_is_typed_and_persists_no_plausible_list() {
    let pool = common::test_db().await;
    let remote = TraktRemoteSynchronizer::new(pool.clone(), FailingTraktApi);
    let error = remote
        .add_or_refresh(&TraktListLocator {
            user: "some-user".to_string(),
            list: "some-list".to_string(),
        })
        .await
        .expect_err("remote failure must propagate");
    assert!(matches!(error, RepositoryError::Conflict(_)));
    let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "TraktList""#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

async fn seed_system_playlist_group(pool: &sqlx::SqlitePool) {
    pool.execute(
        r#"INSERT INTO "PlaylistGroup" ("Id", "Name", "IsSystem")
           VALUES (99, 'Trakt Lists', 1)"#,
    )
    .await
    .unwrap();
}

async fn seed_movie_with_guid(pool: &sqlx::SqlitePool) {
    pool.execute(r#"INSERT INTO "Movie" ("Id") VALUES (1)"#)
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "MovieMetadata" (
            "Id", "MovieId", "MetadataKind", "Title", "OriginalTitle", "SortTitle",
            "Year", "ReleaseDate", "DateAdded", "DateUpdated"
        ) VALUES (
            10, 1, 0, 'Fixture Movie', NULL, 'Fixture Movie',
            1999, NULL, '2026-04-21 00:00:00', '2026-04-21 00:00:00'
        )"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "MetadataGuid" ("Guid", "MovieMetadataId") VALUES ('tmdb://123', 10)"#,
    )
    .await
    .unwrap();
}

async fn seed_local_trakt_list(pool: &sqlx::SqlitePool, id: i64, generate_playlist: bool) {
    sqlx::query(
        r#"INSERT INTO "TraktList" (
            "Id", "AutoRefresh", "Description", "GeneratePlaylist", "ItemCount",
            "LastMatch", "LastUpdate", "List", "Name", "PlaylistId", "TraktId", "User"
        ) VALUES (?, 1, NULL, ?, 1, NULL, NULL, 'local-list', 'Local List', NULL, 77, 'local-user')"#,
    )
    .bind(id)
    .bind(generate_playlist)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "TraktListItem" (
            "Id", "Episode", "Kind", "MediaItemId", "Rank", "Season", "Title",
            "TraktId", "TraktListId", "Year"
        ) VALUES (500, NULL, 0, NULL, 3, NULL, 'Fixture Movie', 100, ?, 1999)"#,
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "TraktListItemGuid" ("Guid", "TraktListItemId")
           VALUES ('tmdb://123', 500)"#,
    )
    .execute(pool)
    .await
    .unwrap();
}
