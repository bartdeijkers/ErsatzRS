use super::{common, make_repositories, seed_managed_list, LIST_ID};
use chrono::Utc;
use ersatzrs_core::repositories::{
    AddonMediaListLifecycleRepository as _, AddonMediaListRepository as _,
    AddonMediaListRevisionArtifacts, ManagedLibraryOperationRepository as _,
};
use ersatzrs_domain::AddonMediaListPublicationPhase;

#[tokio::test]
async fn publication_source_staging_keeps_newer_draft_and_uses_retained_membership() {
    let pool = common::test_db().await;
    let _directory = seed_managed_list(&pool).await;
    let repositories = make_repositories(pool.clone());
    sqlx::raw_sql(
        r#"UPDATE "AddonMediaList" SET "DraftRevision"=1 WHERE "Id"=1;
           UPDATE "AddonMediaListItem" SET "Published"=0 WHERE "Id"=12;"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    repositories
        .addon_media_lists
        .retain_published_revision(
            LIST_ID,
            1,
            1,
            Utc::now(),
            &[],
            &AddonMediaListRevisionArtifacts::default(),
        )
        .await
        .unwrap();
    sqlx::raw_sql(
        r#"UPDATE "AddonMediaList" SET "DraftRevision"=2, "PublishedRevision"=2,
             "Name"='Newer saved draft', "DetailProjectionPendingAt"='2026-09-13T12:00:00Z'
           WHERE "Id"=1;
           UPDATE "AddonMediaListItem" SET "Title"='Newer item draft', "Included"=0,
             "SavedReplacementJson"='{"file":null}' WHERE "Id"=10;
           UPDATE "AddonMediaListItem" SET "InDraft"=0 WHERE "Id"=11;"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    let queued = repositories
        .managed_library_operations
        .enqueue_revision_restore(LIST_ID, 1, 1, 2, 2)
        .await
        .unwrap();
    // Receiving projection will establish this local fence without the explicit
    // Restore action's draft rewrite. Test that staging itself is independent.
    sqlx::query(r#"UPDATE "AddonMediaList" SET "DraftRevision"=? WHERE "Id"=1"#)
        .bind(queued.job.saved_revision)
        .execute(&pool)
        .await
        .unwrap();
    let claim = repositories
        .managed_library_operations
        .claim_operation(queued.operation.id, Utc::now())
        .await
        .unwrap()
        .unwrap();
    assert!(repositories
        .managed_library_operations
        .transition_publication(
            &claim,
            AddonMediaListPublicationPhase::AwaitingLease,
            AddonMediaListPublicationPhase::Staging,
        )
        .await
        .unwrap());
    for _ in 0..2 {
        repositories
            .addon_media_lists
            .stage_publication_revision(
                LIST_ID,
                queued.job.saved_revision,
                queued.operation.id,
                queued.job.generation,
                &claim.operation.owner_token,
                &claim.lease_owner,
            )
            .await
            .unwrap();
        let rows: Vec<(i64, bool, bool, bool)> = sqlx::query_as(
            r#"SELECT "Id", "InDraft", "Included", "Published"
               FROM "AddonMediaListItem" WHERE "AddonMediaListId"=1 ORDER BY "Id""#,
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            rows,
            [
                (10, true, false, true),
                (11, false, true, true),
                (12, true, true, false)
            ]
        );
        assert_saved_draft_unchanged(&pool, queued.job.saved_revision).await;
    }
}

async fn assert_saved_draft_unchanged(pool: &sqlx::SqlitePool, revision: i32) {
    let draft: (String, i32, Option<String>) = sqlx::query_as(
            r#"SELECT "Name", "DraftRevision", "DetailProjectionPendingAt" FROM "AddonMediaList" WHERE "Id"=1"#,
        ).fetch_one(pool).await.unwrap();
    assert_eq!(
        draft,
        (
            "Newer saved draft".into(),
            revision,
            Some("2026-09-13T12:00:00Z".into())
        )
    );
    let item: (String, Option<String>) = sqlx::query_as(
        r#"SELECT "Title", "SavedReplacementJson" FROM "AddonMediaListItem" WHERE "Id"=10"#,
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(
        item,
        ("Newer item draft".into(), Some("{\"file\":null}".into()))
    );
}
