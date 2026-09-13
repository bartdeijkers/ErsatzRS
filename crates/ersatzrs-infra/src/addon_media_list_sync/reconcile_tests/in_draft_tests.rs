use super::*;

async fn seed_draft_jobs(pool: &DbPool) {
    sqlx::raw_sql(
        r#"INSERT INTO "AddonMediaListDetailJob" (
               "AddonMediaListItemId", "AddonMediaListId", "ProviderHost",
               "OverviewFingerprint", "OverviewRecordJson", "UpdatedAt"
           ) VALUES (20, 10, 'media.example.test', 'old', '{}', '2026-09-01');
           INSERT INTO "AddonMediaListItemOperationJob" (
               "AddonMediaListId", "AddonMediaListItemId", "Kind", "SavedDraftRevision",
               "InputJson", "ContextFingerprint", "LeaseOwner", "CreatedAt", "UpdatedAt"
           ) VALUES (10, 20, 1, 4, '{}', 'old', 'worker', '2026-09-01', '2026-09-01');
           INSERT INTO "AddonMediaListItemOperationJob" (
               "AddonMediaListId", "AddonMediaListItemId", "Kind", "SavedDraftRevision",
               "InputJson", "ContextFingerprint", "State", "CreatedAt", "UpdatedAt"
           ) VALUES (10, 20, 0, 4, '{}', 'old', 2, '2026-09-01', '2026-09-01');"#,
    )
    .execute(pool)
    .await
    .expect("seed historic draft jobs");
}

async fn assert_jobs_removed_artifact_retained(pool: &DbPool) {
    let counts: (i64, i64, i64) = sqlx::query_as(
        r#"SELECT
           (SELECT COUNT(*) FROM "AddonMediaListDetailJob" WHERE "AddonMediaListItemId"=20),
           (SELECT "State" FROM "AddonMediaListItemOperationJob" WHERE "AddonMediaListItemId"=20 AND "Kind"=1),
           (SELECT COUNT(*) FROM "AddonMediaListArtifact" WHERE "Id"=30)"#,
    )
    .fetch_one(pool)
    .await
    .expect("inspect retained artifact and removed jobs");
    assert_eq!(counts, (0, 4, 1));
    let history: (String, i32) = sqlx::query_as(
        r#"SELECT
           (SELECT "LeaseOwner" FROM "AddonMediaListItemOperationJob" WHERE "AddonMediaListItemId"=20 AND "Kind"=1),
           (SELECT "State" FROM "AddonMediaListItemOperationJob" WHERE "AddonMediaListItemId"=20 AND "Kind"=0)"#,
    ).fetch_one(pool).await.expect("load process owner and terminal history");
    assert_eq!(history, ("worker".to_owned(), 2));
}

#[tokio::test]
async fn readoption_reuses_identity_without_historic_draft_choices() {
    let pool = fixture().await;
    sqlx::query(
        r#"UPDATE "AddonMediaListItem" SET "InDraft"=0,
           "PublishedAvailabilityReason"='Published reason',
           "ChapterInput"='00:00 Old chapter', "ProviderChapterInput"='00:00 Provider chapter'
           WHERE "Id"=20"#,
    )
    .execute(&pool)
    .await
    .expect("retire published owner");
    seed_draft_jobs(&pool).await;
    AddonMediaListSynchronizer::new(pool.clone())
        .save_list_items_with_mode(
            10,
            &[source_item("Fresh provider title", 0)],
            false,
            None,
            None,
        )
        .await
        .expect("readopt provider identity");
    let row =
        sqlx::query(r#"SELECT * FROM "AddonMediaListItem" WHERE "ProviderId"='provider-item'"#)
            .fetch_one(&pool)
            .await
            .expect("load readopted owner");
    assert_eq!(row.get::<i64, _>("Id"), 20);
    assert!(row.get::<bool, _>("InDraft"));
    assert!(row.get::<bool, _>("Included"));
    assert!(row.get::<bool, _>("Published"));
    assert_eq!(row.get::<Option<i64>, _>("MediaItemId"), Some(321));
    assert_eq!(
        row.get::<String, _>("PublishedAvailabilityReason"),
        "Published reason"
    );
    assert_eq!(row.get::<String, _>("Title"), "Fresh provider title");
    assert_eq!(row.get::<Option<i32>, _>("Season"), Some(1));
    assert_eq!(row.get::<String, _>("MetadataOverrideJson"), "{}");
    assert_eq!(row.get::<String, _>("DraftOptionOverridesJson"), "{}");
    assert_eq!(row.get::<Option<String>, _>("MetadataConflictJson"), None);
    assert_eq!(row.get::<Option<String>, _>("ChapterInput"), None);
    assert_eq!(row.get::<Option<String>, _>("ProviderChapterInput"), None);
    assert_eq!(row.get::<Option<i32>, _>("LivenessOverride"), None);
    assert_eq!(
        row.get::<i32, _>("EffectiveLiveness"),
        encode_enum(RemoteStreamLiveness::Finite)
    );
    assert_eq!(
        row.get::<i32, _>("CollectionRole"),
        encode_enum(ersatzrs_domain::AddonMediaListItemRole::Auto)
    );
    let replacement_count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "AddonMediaListItemReplacement" WHERE "AddonMediaListItemId"=20"#,
    )
    .fetch_one(&pool)
    .await
    .expect("count historic replacement");
    assert_eq!(replacement_count, 0);
    assert_jobs_removed_artifact_retained(&pool).await;
    let revision: i32 =
        sqlx::query_scalar(r#"SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id"=10"#)
            .fetch_one(&pool)
            .await
            .expect("load fresh draft generation");
    assert_eq!(revision, 5);
}

#[tokio::test]
async fn publication_only_owner_is_not_counted_ranked_enriched_or_observed_missing() {
    let pool = fixture().await;
    sqlx::query(r#"UPDATE "AddonMediaListItem" SET "InDraft"=0, "Rank"=7 WHERE "Id"=20"#)
        .execute(&pool)
        .await
        .expect("retire owner");
    let synchronizer = AddonMediaListSynchronizer::new(pool.clone());
    let mut new_item = source_item("New item", 0);
    new_item.provider_id = "new-item".to_owned();
    synchronizer
        .save_list_items_with_mode(10, &[new_item], true, None, None)
        .await
        .expect("save draft delta");
    synchronizer
        .save_list_items_with_mode(10, &[], false, Some(5), None)
        .await
        .expect("observe draft absence");
    let evidence = AddonMediaListRemoteSynchronizer::new(pool.clone(), UnusedProvider);
    finalize_evidence(&evidence, &pool, 5).await;
    let state: (i32, i32, i64) = sqlx::query_as(
        r#"SELECT (SELECT "ItemCount" FROM "AddonMediaList" WHERE "Id"=10),
           (SELECT "Rank" FROM "AddonMediaListItem" WHERE "Id"=20),
           (SELECT COUNT(*) FROM "AddonMediaListItemTombstone" WHERE "AddonMediaListItemId"=20)"#,
    )
    .fetch_one(&pool)
    .await
    .expect("load draft counts and historic owner");
    assert_eq!(state, (1, 7, 0));
    assert!(synchronizer
        .merge_detail_item(10, &source_item("Stale detail", 0))
        .await
        .is_err());
}

#[tokio::test]
async fn omitted_published_chapter_retains_owner_but_removes_draft_jobs() {
    let pool = fixture().await;
    sqlx::query(
        r#"UPDATE "AddonMediaListItem" SET "ParentProviderId"='source',
           "FragmentStartSeconds"=0, "FragmentEndSeconds"=10 WHERE "Id"=20"#,
    )
    .execute(&pool)
    .await
    .expect("make published chapter");
    seed_draft_jobs(&pool).await;
    let synchronizer = AddonMediaListSynchronizer::new(pool.clone());
    synchronizer
        .save_list_items_with_mode(10, &[], false, None, None)
        .await
        .expect("remove chapter from draft");
    let owner: (bool, bool, Option<i64>) = sqlx::query_as(
        r#"SELECT "InDraft", "Published", "MediaItemId" FROM "AddonMediaListItem" WHERE "Id"=20"#,
    )
    .fetch_one(&pool)
    .await
    .expect("retained published owner");
    assert_eq!(owner, (false, true, Some(321)));
    assert_jobs_removed_artifact_retained(&pool).await;
    synchronizer
        .save_list_items_with_mode(10, &[], false, None, None)
        .await
        .expect("repeat regular save preserves retired owner");
    let count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaListItem" WHERE "Id"=20"#)
            .fetch_one(&pool)
            .await
            .expect("count retained owner");
    assert_eq!(count, 1);
}

#[tokio::test]
async fn omitted_unpublished_chapter_can_still_be_deleted() {
    let pool = fixture().await;
    sqlx::query(r#"UPDATE "AddonMediaListItem" SET "Published"=0, "ParentProviderId"='source' WHERE "Id"=20"#)
        .execute(&pool).await.expect("make unpublished chapter");
    AddonMediaListSynchronizer::new(pool.clone())
        .save_list_items_with_mode(10, &[], false, None, None)
        .await
        .expect("remove unpublished chapter");
    let count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaListItem" WHERE "Id"=20"#)
            .fetch_one(&pool)
            .await
            .expect("count deleted owner");
    assert_eq!(count, 0);
}
