use super::*;
use ersatzrs_core::repositories::{
    AddonMediaListLifecycleRepository as _, AddonMediaListRevisionArtifacts,
};

async fn retain(pool: &DbPool) -> AddonMediaListRevisionSnapshot {
    sqlx::query(
        r#"UPDATE "AddonMediaList" SET "DraftRevision"=1,"PublishedRevision"=1 WHERE "Id"=?"#,
    )
    .bind(LIST_ID)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(r#"UPDATE "AddonMediaListItem" SET "ProviderLiveness"=1,"EffectiveLiveness"=1 WHERE "Id"=20"#)
        .execute(pool).await.unwrap();
    crate::repositories::SqliteAddonMediaListRepository(pool.clone())
        .retain_published_revision(
            LIST_ID,
            1,
            1,
            Utc::now(),
            &[],
            &AddonMediaListRevisionArtifacts::default(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn source_playlist_uses_retained_identity_and_enabled_policy_without_rewriting_draft() {
    let pool = pool().await;
    seed_generated_list(&pool).await;
    let source = retain(&pool).await;
    detach_seeded_playlist(&pool).await;
    sqlx::raw_sql(r#"UPDATE "AddonMediaList" SET "GeneratePlaylist"=0,"Name"='New draft',"DraftRevision"=7 WHERE "Id"=10;
        UPDATE "AddonMediaListItem" SET "Title"='Draft movie',"Kind"=0,"Rank"=99,"SourceUrl"='https://example.invalid/draft',"MediaItemId"=NULL WHERE "Id"=20;"#)
        .execute(&pool).await.unwrap();
    let sync = AddonMediaListSynchronizer::new(pool.clone());
    sync.bind_list_items_for_publication(LIST_ID, Some(&source))
        .await
        .unwrap();
    sync.match_list_items_for_publication(LIST_ID, Some(&source))
        .await
        .unwrap();
    let playlist = linked_playlist_id(&pool).await.unwrap();
    let identity = playlist_item_identity(&pool, playlist).await;
    assert_eq!(identity.1, 1);
    let row: (i32, String) = sqlx::query_as(r#"SELECT "item"."Index", "playlist"."Name" FROM "PlaylistItem" AS "item" JOIN "Playlist" AS "playlist" ON "playlist"."Id"="item"."PlaylistId" WHERE "playlist"."Id"=?"#)
        .bind(playlist).fetch_one(&pool).await.unwrap();
    assert_eq!(row, (0, "Fixture list".into()));
    sync.match_list_items_for_publication(LIST_ID, Some(&source))
        .await
        .unwrap();
    assert_eq!(playlist_item_identity(&pool, playlist).await, identity);
    let draft: (bool, String, i32, String, i32, i32) = sqlx::query_as(r#"SELECT "list"."GeneratePlaylist","list"."Name","list"."DraftRevision","item"."Title","item"."Kind","item"."Rank" FROM "AddonMediaList" AS "list" JOIN "AddonMediaListItem" AS "item" ON "item"."AddonMediaListId"="list"."Id" WHERE "item"."Id"=20"#)
        .fetch_one(&pool).await.unwrap();
    assert_eq!(
        draft,
        (false, "New draft".into(), 7, "Draft movie".into(), 0, 99)
    );
}

#[tokio::test]
async fn source_playlist_uses_exact_retained_replacement_not_new_draft_choice() {
    let pool = pool().await;
    seed_generated_list(&pool).await;
    sqlx::raw_sql(r#"INSERT INTO "MediaSource" ("Id") VALUES (50);
        INSERT INTO "LocalMediaSource" ("Id") VALUES (50);
        INSERT INTO "Library" ("Id","Name","MediaKind","MediaSourceId") VALUES (50,'Local',1,50);
        INSERT INTO "LibraryPath" ("Id","LibraryId","Path") VALUES (50,50,'fixture-local');
        INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES (900,50,0);
        INSERT INTO "Movie" ("Id") VALUES (900);
        INSERT INTO "MovieMetadata" ("MovieId","MetadataKind","Title","DateAdded","DateUpdated") VALUES (900,1,'Replacement','2026-09-13','2026-09-13');
        INSERT INTO "MediaVersion" ("Id","MovieId","Duration","DateAdded","DateUpdated","Height","Width","VideoScanKind") VALUES
            (901,900,'00:01:00','2026-09-13','2026-09-13',1080,1920,0),
            (902,900,'00:02:00','2026-09-13','2026-09-13',1080,1920,0);
        INSERT INTO "MediaFile" ("MediaVersionId","Path","PathHash") VALUES (901,'fixture-local/old.mkv','old'),(902,'fixture-local/new.mkv','new');
        INSERT INTO "MediaStream" ("MediaVersionId","Index","MediaStreamKind","Channels","Default","Forced","AttachedPic") VALUES (901,0,1,0,1,0,0),(902,0,1,0,1,0,0);
        INSERT INTO "AddonMediaListItemReplacement" ("AddonMediaListItemId","MediaItemId","MediaVersionId","CreatedAt","UpdatedAt") VALUES (20,900,901,'2026-09-13','2026-09-13');"#)
        .execute(&pool).await.unwrap();
    let source = retain(&pool).await;
    sqlx::query(r#"UPDATE "AddonMediaListItemReplacement" SET "MediaVersionId"=902 WHERE "AddonMediaListItemId"=20"#)
        .execute(&pool).await.unwrap();
    let sync = AddonMediaListSynchronizer::new(pool.clone());
    sync.match_list_items_for_publication(LIST_ID, Some(&source))
        .await
        .unwrap();
    let selected: (i64, i64) = sqlx::query_as(
        r#"SELECT "MediaItemId","MediaVersionId" FROM "PlaylistItem" WHERE "PlaylistId"=?"#,
    )
    .bind(PLAYLIST_ID)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(selected, (900, 901));
    let current: i64 = sqlx::query_scalar(r#"SELECT "MediaVersionId" FROM "AddonMediaListItemReplacement" WHERE "AddonMediaListItemId"=20"#)
        .fetch_one(&pool).await.unwrap();
    assert_eq!(current, 902);
}
