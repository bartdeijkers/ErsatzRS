#![cfg(not(feature = "mysql"))]

mod common;

use async_trait::async_trait;
use ersatzrs_core::addon_media_list::{
    AddonMediaListProvider, EnumeratedAddonMediaList, EnumeratedAddonMediaListItem,
};
use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::{
    AddonMediaListContentKind, AddonMediaListItemAvailability, AddonMediaListItemKind,
};
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
        _chapter_input: &str,
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
                availability: AddonMediaListItemAvailability::Available,
                availability_reason: None,
                content_kind: AddonMediaListContentKind::Movie,
                duration_seconds: None,
                parent_provider_id: None,
                fragment_start_seconds: None,
                fragment_end_seconds: None,
                additional_image_urls: Vec::new(),
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
        _chapter_input: &str,
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
                availability: AddonMediaListItemAvailability::Available,
                availability_reason: None,
                content_kind: AddonMediaListContentKind::Auto,
                duration_seconds: None,
                parent_provider_id: None,
                fragment_start_seconds: None,
                fragment_end_seconds: None,
                additional_image_urls: Vec::new(),
            }],
        })
    }
}

#[derive(Clone)]
struct EmptyRemoteStreamListProvider;

#[async_trait]
impl AddonMediaListProvider for EmptyRemoteStreamListProvider {
    async fn enumerate(
        &self,
        _addon_id: &str,
        _source_url: &str,
        _chapter_input: &str,
    ) -> Result<EnumeratedAddonMediaList> {
        Ok(EnumeratedAddonMediaList {
            provider_id: "remote/list".to_string(),
            name: "Remote List".to_string(),
            description: None,
            items: Vec::new(),
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
        _chapter_input: &str,
    ) -> Result<EnumeratedAddonMediaList> {
        Err(RepositoryError::Conflict(
            "deterministic remote failure".to_string(),
        ))
    }
}

#[derive(Clone)]
struct ChapterListProvider;

#[async_trait]
impl AddonMediaListProvider for ChapterListProvider {
    async fn enumerate(
        &self,
        _addon_id: &str,
        _source_url: &str,
        chapter_input: &str,
    ) -> Result<EnumeratedAddonMediaList> {
        let full_provider_id = "episode:42".to_owned();
        let mut items = vec![EnumeratedAddonMediaListItem {
            provider_id: full_provider_id.clone(),
            rank: 0,
            display_title: "Fixture Episode".to_owned(),
            title: "Fixture Episode".to_owned(),
            year: None,
            season: None,
            episode: None,
            kind: AddonMediaListItemKind::RemoteStream,
            guids: vec!["beeldengeluid://42".to_owned()],
            source_url: Some("https://example.invalid/episode/42".to_owned()),
            availability: AddonMediaListItemAvailability::Available,
            availability_reason: None,
            content_kind: AddonMediaListContentKind::TelevisionEpisode,
            duration_seconds: Some(600),
            parent_provider_id: None,
            fragment_start_seconds: None,
            fragment_end_seconds: None,
            additional_image_urls: Vec::new(),
        }];
        for chapter in ersatzrs_core::media_chapters::parse_chapter_input(chapter_input, Some(600))
            .expect("test chapter input")
        {
            items.push(EnumeratedAddonMediaListItem {
                provider_id: format!("{full_provider_id}:chapter:{}", chapter.start_seconds),
                rank: i32::try_from(items.len()).expect("test rank"),
                display_title: chapter.title.clone(),
                title: chapter.title,
                year: None,
                season: None,
                episode: None,
                kind: AddonMediaListItemKind::RemoteStream,
                guids: Vec::new(),
                source_url: Some(format!(
                    "https://example.invalid/episode/42?start={}",
                    chapter.start_seconds
                )),
                availability: AddonMediaListItemAvailability::Available,
                availability_reason: None,
                content_kind: AddonMediaListContentKind::TelevisionEpisode,
                duration_seconds: chapter.end_seconds.map(|end| end - chapter.start_seconds),
                parent_provider_id: Some(full_provider_id.clone()),
                fragment_start_seconds: Some(chapter.start_seconds),
                fragment_end_seconds: chapter.end_seconds,
                additional_image_urls: Vec::new(),
            });
        }
        Ok(EnumeratedAddonMediaList {
            provider_id: "episode-list:42".to_owned(),
            name: "Fixture Episode".to_owned(),
            description: None,
            items,
        })
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
            AddonMediaListContentKind::Auto,
            "",
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
        .update_settings(list_id, true, true, AddonMediaListContentKind::Auto, "")
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
        .add_or_refresh(
            "org.example.lists",
            "https://example.invalid/list",
            None,
            AddonMediaListContentKind::Auto,
            "",
        )
        .await
        .unwrap();
    AddonMediaListSynchronizer::new(pool.clone())
        .update_settings(list_id, false, true, AddonMediaListContentKind::Auto, "")
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
async fn unavailable_remote_item_uses_exact_local_replacement_until_provider_recovers() {
    let pool = common::test_db().await;
    seed_system_playlist_group(&pool).await;
    seed_addon(&pool).await;
    common::seed_media_graph(&pool).await;
    seed_movie_with_guid(&pool).await;
    seed_local_movie_version(&pool).await;
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
        .add_or_refresh(
            "org.example.lists",
            "https://example.invalid/list",
            None,
            AddonMediaListContentKind::Auto,
            "",
        )
        .await
        .unwrap();
    let list_item_id: i64 =
        sqlx::query_scalar(r#"SELECT "Id" FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ?"#)
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query(
        r#"INSERT INTO "AddonMediaListItemReplacement" (
               "AddonMediaListItemId", "MediaItemId", "MediaVersionId", "CreatedAt", "UpdatedAt"
           ) VALUES (?, 1, 10, '2026-08-15T00:00:00Z', '2026-08-15T00:00:00Z')"#,
    )
    .bind(list_item_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(r#"UPDATE "AddonMediaListItem" SET "Availability" = 1 WHERE "Id" = ?"#)
        .bind(list_item_id)
        .execute(&pool)
        .await
        .unwrap();

    let synchronizer = AddonMediaListSynchronizer::new(pool.clone());
    synchronizer
        .update_settings(list_id, false, true, AddonMediaListContentKind::Auto, "")
        .await
        .unwrap();
    assert_playlist_selection(&pool, list_id, (10, 1, Some(10), Some(3))).await;

    sqlx::query(r#"UPDATE "AddonMediaListItem" SET "Availability" = 2 WHERE "Id" = ?"#)
        .bind(list_item_id)
        .execute(&pool)
        .await
        .unwrap();
    synchronizer.match_list_items(list_id).await.unwrap();
    assert_playlist_selection(&pool, list_id, (70, 2, None, None)).await;

    sqlx::query(r#"UPDATE "AddonMediaListItem" SET "Availability" = 0 WHERE "Id" = ?"#)
        .bind(list_item_id)
        .execute(&pool)
        .await
        .unwrap();
    synchronizer.match_list_items(list_id).await.unwrap();
    assert_playlist_selection(&pool, list_id, (70, 2, None, None)).await;
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn omitted_provider_item_retains_replacement_tombstone_and_recovers_identity() {
    let pool = common::test_db().await;
    seed_system_playlist_group(&pool).await;
    seed_addon(&pool).await;
    common::seed_media_graph(&pool).await;
    seed_movie_with_guid(&pool).await;
    seed_local_movie_version(&pool).await;
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

    let source_url = "https://example.invalid/list";
    let list_id = AddonMediaListRemoteSynchronizer::new(pool.clone(), FakeRemoteStreamListProvider)
        .add_or_refresh(
            "org.example.lists",
            source_url,
            None,
            AddonMediaListContentKind::Auto,
            "",
        )
        .await
        .unwrap();
    let item_id: i64 =
        sqlx::query_scalar(r#"SELECT "Id" FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ?"#)
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query(
        r#"INSERT INTO "AddonMediaListItemReplacement" (
               "AddonMediaListItemId", "MediaItemId", "MediaVersionId", "CreatedAt", "UpdatedAt"
           ) VALUES (?, 1, 10, '2026-08-15T00:00:00Z', '2026-08-15T00:00:00Z')"#,
    )
    .bind(item_id)
    .execute(&pool)
    .await
    .unwrap();
    AddonMediaListSynchronizer::new(pool.clone())
        .update_settings(list_id, false, true, AddonMediaListContentKind::Auto, "")
        .await
        .unwrap();

    AddonMediaListRemoteSynchronizer::new(pool.clone(), EmptyRemoteStreamListProvider)
        .add_or_refresh(
            "org.example.lists",
            source_url,
            None,
            AddonMediaListContentKind::Auto,
            "",
        )
        .await
        .unwrap();
    let tombstone = sqlx::query(
        r#"SELECT "Id", "Availability", "AvailabilityReason"
           FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ?"#,
    )
    .bind(list_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(tombstone.get::<i64, _>("Id"), item_id);
    assert_eq!(tombstone.get::<i32, _>("Availability"), 1);
    assert_eq!(
        tombstone.get::<String, _>("AvailabilityReason"),
        "omitted_by_provider"
    );
    assert_playlist_selection(&pool, list_id, (10, 1, Some(10), Some(3))).await;

    AddonMediaListRemoteSynchronizer::new(pool.clone(), FakeRemoteStreamListProvider)
        .add_or_refresh(
            "org.example.lists",
            source_url,
            None,
            AddonMediaListContentKind::Auto,
            "",
        )
        .await
        .unwrap();
    let recovered = sqlx::query(
        r#"SELECT "Id", "Availability" FROM "AddonMediaListItem"
           WHERE "AddonMediaListId" = ?"#,
    )
    .bind(list_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(recovered.get::<i64, _>("Id"), item_id);
    assert_eq!(recovered.get::<i32, _>("Availability"), 0);
    assert_playlist_selection(&pool, list_id, (70, 2, None, None)).await;
    let replacement_count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "AddonMediaListItemReplacement"
           WHERE "AddonMediaListItemId" = ?"#,
    )
    .bind(item_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(replacement_count, 1);
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

    local
        .update_settings(50, true, true, AddonMediaListContentKind::Auto, "")
        .await
        .unwrap();
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

    local
        .update_settings(50, true, false, AddonMediaListContentKind::Auto, "")
        .await
        .unwrap();
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

    local
        .update_settings(50, false, true, AddonMediaListContentKind::Auto, "")
        .await
        .unwrap();
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
            AddonMediaListContentKind::Auto,
            "",
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
async fn chapter_refresh_reconciles_structured_records_without_duplicates() {
    let pool = common::test_db().await;
    seed_addon(&pool).await;
    let remote = AddonMediaListRemoteSynchronizer::new(pool.clone(), ChapterListProvider);
    let list_id = remote
        .add_or_refresh(
            "org.example.lists",
            "https://example.invalid/episode/42",
            None,
            AddonMediaListContentKind::Auto,
            "0:00 Intro\n1:00 Topic",
        )
        .await
        .expect("first chapter import");
    remote
        .add_or_refresh(
            "org.example.lists",
            "https://example.invalid/episode/42",
            None,
            AddonMediaListContentKind::Auto,
            "0:00 Introduction\n1:30 Topic\n3:00 Finish",
        )
        .await
        .expect("chapter refresh");

    let item_count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ?"#,
    )
    .bind(list_id)
    .fetch_one(&pool)
    .await
    .expect("item count");
    let chapter_count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "AddonMediaListChapter" WHERE "AddonMediaListId" = ?"#,
    )
    .bind(list_id)
    .fetch_one(&pool)
    .await
    .expect("chapter count");
    let starts = sqlx::query_scalar::<_, i64>(
        r#"SELECT "StartSeconds" FROM "AddonMediaListChapter"
           WHERE "AddonMediaListId" = ? ORDER BY "ChapterIndex""#,
    )
    .bind(list_id)
    .fetch_all(&pool)
    .await
    .expect("chapter starts");
    let duplicate_provider_ids: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM (
               SELECT "ProviderId" FROM "AddonMediaListItem"
               WHERE "AddonMediaListId" = ? GROUP BY "ProviderId" HAVING COUNT(*) > 1
           ) AS "duplicates""#,
    )
    .bind(list_id)
    .fetch_one(&pool)
    .await
    .expect("duplicate provider ids");

    assert_eq!(item_count, 4);
    assert_eq!(chapter_count, 3);
    assert_eq!(starts, vec![0, 90, 180]);
    assert_eq!(duplicate_provider_ids, 0);
}

#[tokio::test]
async fn remote_failure_preserves_the_complete_existing_list() {
    let pool = common::test_db().await;
    seed_local_addon_media_list(&pool, 60, false).await;

    let remote = AddonMediaListRemoteSynchronizer::new(pool.clone(), FailingListProvider);
    let error = remote
        .add_or_refresh(
            "org.example.lists",
            "https://example.invalid/local-list",
            None,
            AddonMediaListContentKind::Auto,
            "",
        )
        .await
        .expect_err("remote failure must reject the complete refresh");
    assert!(matches!(error, RepositoryError::Conflict(_)));

    let list = sqlx::query(
        r#"SELECT "ProviderId", "Name", "ItemCount", "LastError"
           FROM "AddonMediaList" WHERE "Id" = 60"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(list.get::<String, _>("ProviderId"), "local-user/local-list");
    assert_eq!(list.get::<String, _>("Name"), "Local List");
    assert_eq!(list.get::<i32, _>("ItemCount"), 1);
    assert_eq!(
        list.get::<Option<String>, _>("LastError").as_deref(),
        Some("conflict: deterministic remote failure")
    );

    let item = sqlx::query(
        r#"SELECT "ProviderId", "DisplayTitle", "Rank"
           FROM "AddonMediaListItem" WHERE "AddonMediaListId" = 60"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(item.get::<String, _>("ProviderId"), "movie:100");
    assert_eq!(
        item.get::<String, _>("DisplayTitle"),
        "Fixture Movie (1999)"
    );
    assert_eq!(item.get::<i32, _>("Rank"), 3);
    let guids: Vec<String> = sqlx::query_scalar(
        r#"SELECT "Guid" FROM "AddonMediaListItemGuid"
           WHERE "AddonMediaListItemId" = 500 ORDER BY "Guid""#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(guids, ["tmdb://123"]);
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

async fn seed_local_movie_version(pool: &sqlx::SqlitePool) {
    pool.execute(r#"UPDATE "MediaItem" SET "State" = 0 WHERE "Id" = 1"#)
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "MediaVersion" (
               "Id", "DateAdded", "DateUpdated", "Duration", "Height", "MovieId",
               "VideoScanKind", "Width"
           ) VALUES (
               10, '2026-08-15T00:00:00Z', '2026-08-15T00:00:00Z',
               '00:10:00', 1080, 1, 0, 1920
           )"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "MediaFile" ("MediaVersionId", "Path", "PathHash")
           VALUES (10, '/media/replacement.mkv', 'replacement-hash')"#,
    )
    .await
    .unwrap();
    // `MediaStreamKind` is 1-based, so the video stream is 1. Seeding 0 makes
    // the replacement resolver find no video stream and select nothing, which
    // looks exactly like a broken generated playlist.
    pool.execute(
        r#"INSERT INTO "MediaStream" (
               "Index", "MediaStreamKind", "Channels", "Default", "Forced",
               "MediaVersionId", "BitsPerRawSample", "AttachedPic"
           ) VALUES (3, 1, 0, 1, 0, 10, 0, 0)"#,
    )
    .await
    .unwrap();
}

async fn assert_playlist_selection(
    pool: &sqlx::SqlitePool,
    list_id: i64,
    expected: (i32, i64, Option<i64>, Option<i32>),
) {
    let row = sqlx::query(
        r#"SELECT "item"."CollectionType", "item"."MediaItemId",
                  "item"."MediaVersionId", "item"."VideoStreamIndex"
           FROM "PlaylistItem" AS "item"
           JOIN "AddonMediaList" AS "list" ON "list"."PlaylistId" = "item"."PlaylistId"
           WHERE "list"."Id" = ?"#,
    )
    .bind(list_id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(
        (
            row.get::<i32, _>("CollectionType"),
            row.get::<i64, _>("MediaItemId"),
            row.get::<Option<i64>, _>("MediaVersionId"),
            row.get::<Option<i32>, _>("VideoStreamIndex"),
        ),
        expected
    );
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

/// An operator who pastes a URL should not have to restate what the list
/// obviously is: the add-ons classify every entry, so the import adopts the
/// presentation they imply. A projection the import cannot complete is recorded
/// rather than failing the import, because the entries are already playable as
/// Remote Streams either way.
#[tokio::test]
async fn import_adopts_the_presentation_the_entries_imply() {
    let pool = common::test_db().await;
    seed_system_playlist_group(&pool).await;
    seed_addon(&pool).await;
    common::seed_media_graph(&pool).await;

    let remote = AddonMediaListRemoteSynchronizer::new(pool.clone(), FakeRemoteStreamListProvider);
    let list_id = remote
        .add_or_refresh(
            "org.example.lists",
            "https://example.invalid/list",
            None,
            AddonMediaListContentKind::Auto,
            "",
        )
        .await
        .unwrap();

    // The fake provider classifies nothing, so nothing is implied and the list
    // stays on Remote Streams without a spurious error.
    let (requested, error): (i32, Option<String>) = sqlx::query_as(
        r#"SELECT "RequestedPresentationKind", "PresentationError"
           FROM "AddonMediaList" WHERE "Id" = ?"#,
    )
    .bind(list_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        requested, 0,
        "an unclassified list must not be re-presented"
    );
    assert!(error.is_none(), "{error:?}");
}
