#![cfg(not(feature = "mysql"))]

mod common;

use async_trait::async_trait;
use ersatzrs_core::repositories::Result;
use ersatzrs_core::trakt::{TraktApiClient, TraktListItemWithGuids, TraktListLocator};
use ersatzrs_domain::{TraktList, TraktListItemKind};
use ersatzrs_infra::trakt_sync::TraktSynchronizer;
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

#[tokio::test]
async fn add_refresh_match_and_generate_playlist() {
    let pool = common::test_db().await;
    seed_system_playlist_group(&pool).await;
    common::seed_media_graph(&pool).await;
    seed_movie_with_guid(&pool).await;

    let sync = TraktSynchronizer::new(pool.clone(), FakeTraktApi);
    let list_id = sync
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

    sync.update_settings(list_id, true, true).await.unwrap();

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
