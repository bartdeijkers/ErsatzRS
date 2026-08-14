#![cfg(not(feature = "mysql"))]

mod common;

use async_trait::async_trait;
use ersatzrs_core::addon_media_list::{
    AddonMediaListProvider, EnumeratedAddonMediaList, EnumeratedAddonMediaListItem,
};
use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::AddonMediaListItemKind;
use ersatzrs_infra::addon_media_list_sync::{
    AddonMediaListRemoteSynchronizer, AddonMediaListSynchronizer,
};
use sqlx::{Executor, Row};

#[derive(Clone)]
struct FakeListProvider;

#[async_trait]
impl AddonMediaListProvider for FakeListProvider {
    async fn enumerate(
        &self,
        _addon_id: &str,
        _source_url: &str,
    ) -> Result<EnumeratedAddonMediaList> {
        Ok(EnumeratedAddonMediaList {
            provider_id: "api-user/api-list".to_string(),
            name: "API List".to_string(),
            description: Some("From add-on".to_string()),
            items: vec![EnumeratedAddonMediaListItem {
                provider_id: "movie:100".to_string(),
                rank: 1,
                display_title: "Fixture Movie (1999)".to_string(),
                title: "Fixture Movie".to_string(),
                year: Some(1999),
                season: None,
                episode: None,
                kind: AddonMediaListItemKind::Movie,
                guids: vec!["tmdb://123".to_string()],
                source_url: None,
            }],
        })
    }
}

#[derive(Clone)]
struct FakeRemoteStreamListProvider;

#[async_trait]
impl AddonMediaListProvider for FakeRemoteStreamListProvider {
    async fn enumerate(
        &self,
        _addon_id: &str,
        _source_url: &str,
    ) -> Result<EnumeratedAddonMediaList> {
        Ok(EnumeratedAddonMediaList {
            provider_id: "remote/list".to_string(),
            name: "Remote List".to_string(),
            description: None,
            items: vec![EnumeratedAddonMediaListItem {
                provider_id: "remote:100".to_string(),
                rank: 0,
                display_title: "Fixture Remote Stream".to_string(),
                title: "Fixture Remote Stream".to_string(),
                year: None,
                season: None,
                episode: None,
                kind: AddonMediaListItemKind::RemoteStream,
                guids: Vec::new(),
                source_url: Some("https://example.invalid/watch/100".to_string()),
            }],
        })
    }
}

#[derive(Clone)]
struct FailingListProvider;

#[async_trait]
impl AddonMediaListProvider for FailingListProvider {
    async fn enumerate(
        &self,
        _addon_id: &str,
        _source_url: &str,
    ) -> Result<EnumeratedAddonMediaList> {
        Err(RepositoryError::Conflict(
            "deterministic remote failure".to_string(),
        ))
    }
}

#[tokio::test]
async fn add_refresh_match_and_generate_playlist() {
    let pool = common::test_db().await;
    seed_system_playlist_group(&pool).await;
    seed_addon(&pool).await;
    common::seed_media_graph(&pool).await;
    seed_movie_with_guid(&pool).await;

    let remote = AddonMediaListRemoteSynchronizer::new(pool.clone(), FakeListProvider);
    let list_id = remote
        .add_or_refresh(
            "org.example.lists",
            "https://example.invalid/some-list",
            None,
        )
        .await
        .unwrap();

    let list = sqlx::query(r#"SELECT * FROM "AddonMediaList" WHERE "Id" = ?"#)
        .bind(list_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(list.get::<String, _>("AddonId"), "org.example.lists");
    assert_eq!(list.get::<String, _>("ProviderId"), "api-user/api-list");

    let item = sqlx::query(r#"SELECT * FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ?"#)
        .bind(list_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(item.get::<i64, _>("MediaItemId"), 1);

    AddonMediaListSynchronizer::new(pool.clone())
        .update_settings(list_id, true, true)
        .await
        .unwrap();

    let playlist_item = sqlx::query(
        r#"SELECT "pi"."CollectionType", "pi"."MediaItemId", "pi"."PlaybackOrder"
           FROM "PlaylistItem" AS "pi"
           JOIN "AddonMediaList" AS "tl" ON "tl"."PlaylistId" = "pi"."PlaylistId"
           WHERE "tl"."Id" = ?"#,
    )
    .bind(list_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(playlist_item.get::<i32, _>("CollectionType"), 10);
    assert_eq!(playlist_item.get::<i64, _>("MediaItemId"), 1);
    assert_eq!(playlist_item.get::<i32, _>("PlaybackOrder"), 1);

    sqlx::query(r#"DELETE FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ?"#)
        .bind(list_id)
        .execute(&pool)
        .await
        .unwrap();
    AddonMediaListSynchronizer::new(pool.clone())
        .match_list_items(list_id)
        .await
        .unwrap();
    let stale_playlist_items: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*)
           FROM "PlaylistItem" AS "item"
           JOIN "AddonMediaList" AS "list" ON "list"."PlaylistId" = "item"."PlaylistId"
           WHERE "list"."Id" = ?"#,
    )
    .bind(list_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stale_playlist_items, 0);
}

#[tokio::test]
async fn remote_stream_items_match_by_url_and_use_remote_stream_collection_type() {
    let pool = common::test_db().await;
    seed_system_playlist_group(&pool).await;
    seed_addon(&pool).await;
    common::seed_media_graph(&pool).await;
    sqlx::query(r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (2, 1, 0)"#)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO "RemoteStream" ("Id", "Url", "IsLive")
           VALUES (2, 'https://example.invalid/watch/100', 0)"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    let remote = AddonMediaListRemoteSynchronizer::new(pool.clone(), FakeRemoteStreamListProvider);
    let list_id = remote
        .add_or_refresh("org.example.lists", "https://example.invalid/list", None)
        .await
        .unwrap();
    AddonMediaListSynchronizer::new(pool.clone())
        .update_settings(list_id, false, true)
        .await
        .unwrap();

    let playlist_item = sqlx::query(
        r#"SELECT "pi"."CollectionType", "pi"."MediaItemId"
           FROM "PlaylistItem" AS "pi"
           JOIN "AddonMediaList" AS "list" ON "list"."PlaylistId" = "pi"."PlaylistId"
           WHERE "list"."Id" = ?"#,
    )
    .bind(list_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(playlist_item.get::<i32, _>("CollectionType"), 70);
    assert_eq!(playlist_item.get::<i64, _>("MediaItemId"), 2);
}

#[tokio::test]
async fn local_matching_requires_no_remote_capability_and_maintains_playlist_lifecycle() {
    let pool = common::test_db().await;
    seed_system_playlist_group(&pool).await;
    seed_addon(&pool).await;
    common::seed_media_graph(&pool).await;
    seed_movie_with_guid(&pool).await;
    seed_local_addon_media_list(&pool, 50, true).await;

    let local = AddonMediaListSynchronizer::new(pool.clone());
    assert_eq!(local.match_due_lists().await.unwrap(), 1);

    let item = sqlx::query(
        r#"SELECT "MediaItemId" FROM "AddonMediaListItem" WHERE "AddonMediaListId" = 50"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(item.get::<i64, _>("MediaItemId"), 1);
    let last_match: Option<String> =
        sqlx::query_scalar(r#"SELECT "LastMatch" FROM "AddonMediaList" WHERE "Id" = 50"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(last_match.is_some());

    local.update_settings(50, true, true).await.unwrap();
    let playlist_id: Option<i64> =
        sqlx::query_scalar(r#"SELECT "PlaylistId" FROM "AddonMediaList" WHERE "Id" = 50"#)
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
        sqlx::query_scalar(r#"SELECT "PlaylistId" FROM "AddonMediaList" WHERE "Id" = 50"#)
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
        sqlx::query_scalar(r#"SELECT "PlaylistId" FROM "AddonMediaList" WHERE "Id" = 50"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    let replacement_playlist_id = replacement_playlist_id.expect("replacement playlist");
    local.delete_list(50).await.unwrap();
    let list_count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaList" WHERE "Id" = 50"#)
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
    let remote = AddonMediaListRemoteSynchronizer::new(pool.clone(), FailingListProvider);
    let error = remote
        .add_or_refresh(
            "org.example.lists",
            "https://example.invalid/some-list",
            None,
        )
        .await
        .expect_err("remote failure must propagate");
    assert!(matches!(error, RepositoryError::Conflict(_)));
    let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaList""#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn automatic_refresh_skips_lists_owned_by_disabled_addons() {
    let pool = common::test_db().await;
    seed_local_addon_media_list(&pool, 60, false).await;
    sqlx::query(r#"UPDATE "AddonInstallation" SET "Enabled" = 0 WHERE "AddonId" = ?"#)
        .bind("org.example.lists")
        .execute(&pool)
        .await
        .unwrap();

    let refresh = AddonMediaListRemoteSynchronizer::new(pool.clone(), FailingListProvider);
    assert_eq!(refresh.refresh_due_auto_lists().await.unwrap(), 0);

    let last_error: Option<String> =
        sqlx::query_scalar(r#"SELECT "LastError" FROM "AddonMediaList" WHERE "Id" = 60"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(last_error, None);
}

async fn seed_system_playlist_group(pool: &sqlx::SqlitePool) {
    pool.execute(
        r#"INSERT INTO "PlaylistGroup" ("Id", "Name", "IsSystem")
           VALUES (99, 'Add-on Lists', 1)"#,
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

async fn seed_local_addon_media_list(pool: &sqlx::SqlitePool, id: i64, generate_playlist: bool) {
    seed_addon(pool).await;
    sqlx::query(
        r#"INSERT INTO "AddonMediaList" (
            "Id", "AddonId", "ProviderId", "SourceUrl", "Name", "AutoRefresh",
            "Description", "GeneratePlaylist", "ItemCount", "LastMatch", "LastUpdate", "PlaylistId"
        ) VALUES (
            ?, 'org.example.lists', 'local-user/local-list',
            'https://example.invalid/local-list', 'Local List', 1,
            NULL, ?, 1, NULL, NULL, NULL
        )"#,
    )
    .bind(id)
    .bind(generate_playlist)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "AddonMediaListItem" (
            "Id", "ProviderId", "DisplayTitle", "Episode", "Kind", "MediaItemId",
            "Rank", "Season", "Title", "AddonMediaListId", "Year"
        ) VALUES (
            500, 'movie:100', 'Fixture Movie (1999)', NULL, 0, NULL,
            3, NULL, 'Fixture Movie', ?, 1999
        )"#,
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "AddonMediaListItemGuid" ("Guid", "AddonMediaListItemId")
           VALUES ('tmdb://123', 500)"#,
    )
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_addon(pool: &sqlx::SqlitePool) {
    pool.execute(
        r#"INSERT OR IGNORE INTO "AddonInstallation" (
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
