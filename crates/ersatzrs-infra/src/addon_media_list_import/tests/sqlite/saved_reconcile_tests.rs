//! File-first reconciliation finalization on an explicitly adopted test baseline.

use super::*;
use crate::addon_media_list_saved::model::{SavedListDocument, SAVED_LIST_FILE_NAME};
use crate::addon_media_list_saved::{
    assembly, capture, document_io, document_store, mapped, published,
};

async fn adopt_baseline(
    pool: &DbPool,
    root: &std::path::Path,
    list_id: i64,
    source: &str,
) -> std::path::PathBuf {
    std::fs::create_dir_all(root).unwrap();
    let installed: ersatzrs_addon_contract::AddonManifest = serde_json::from_value(serde_json::json!({
        "manifest_version":5,"id":"org.example.fixture","version":"1.0.0","name":{"en-US":"Fixture"},
        "summary":{"en-US":"Fixture"},"license":"MIT","source_url":"https://example.invalid/source",
        "host_version":"*","entrypoints":{},"capabilities":[{"id":"media-list.list.v5"}],
        "settings":[{"key":"MEDIA_STORAGE_PATH","label":{"en-US":"Storage"},"kind":"path","required":false}],
        "media_list_storage":{"path_setting":"MEDIA_STORAGE_PATH","default_subdirectory":"media","source_name":"Fixture"}
    })).unwrap();
    sqlx::query(
        r#"UPDATE "AddonInstallation" SET "ManifestJson"=? WHERE "AddonId"='org.example.fixture'"#,
    )
    .bind(serde_json::to_string(&installed).unwrap())
    .execute(pool)
    .await
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    crate::addon_media_storage::ensure_media_source_in_transaction(
        &mut tx,
        "org.example.fixture",
        installed.media_list_storage.as_ref().unwrap(),
        root,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    crate::addon_media_storage::ensure_list_library(
        pool,
        list_id,
        "org.example.fixture",
        "Fixture",
        source,
        "",
        None,
    )
    .await
    .unwrap();
    let manifest: String = sqlx::query_scalar(
        r#"SELECT "ManifestPath" FROM "AddonMediaListLibrary" WHERE "AddonMediaListId"=?"#,
    )
    .bind(list_id)
    .fetch_one(pool)
    .await
    .unwrap();
    let folder = std::path::Path::new(&manifest).parent().unwrap();
    sqlx::query(r#"UPDATE "AddonMediaListItem" SET "Published"=1,"DurationSeconds"=600 WHERE "AddonMediaListId"=?"#)
        .bind(list_id).execute(pool).await.unwrap();
    sqlx::query(r#"UPDATE "AddonMediaList" SET "PublishedRevision"=1 WHERE "Id"=?"#)
        .bind(list_id)
        .execute(pool)
        .await
        .unwrap();
    let revision: i32 =
        sqlx::query_scalar(r#"SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id"=?"#)
            .bind(list_id)
            .fetch_one(pool)
            .await
            .unwrap();
    SqliteAddonMediaListRepository(pool.clone())
        .retain_published_revision(
            list_id,
            1,
            revision,
            Utc::now(),
            &[],
            &ersatzrs_core::repositories::AddonMediaListRevisionArtifacts::default(),
        )
        .await
        .unwrap();
    let tree = assembly::lock_managed_tree(folder).unwrap();
    let mut tx = pool.begin().await.unwrap();
    let draft = capture::draft_in_tx(&mut tx, list_id).await.unwrap();
    let publication = published::in_tx(&mut tx, list_id).await.unwrap();
    tx.commit().await.unwrap();
    let document = assembly::assemble_initial(
        pool,
        &assembly::AssemblyContext {
            list_id,
            tree: tree.clone(),
            storage_parent: root,
            storage_owner: "fixture",
            artifact_root: root,
            installed: &installed,
        },
        draft,
        publication,
    )
    .await
    .unwrap();
    let path = folder.join(SAVED_LIST_FILE_NAME);
    document_io::write_document(&mut std::fs::File::create(&path).unwrap(), &document).unwrap();
    let (_, digest) = document_store::read(&tree).unwrap();
    // Explicit fixture adoption only: no claim of runtime legacy-list adoption.
    sqlx::query(r#"UPDATE "AddonMediaList" SET "SavedDocumentDigest"=?,"SavedPublicationDigest"=? WHERE "Id"=?"#)
        .bind(digest).bind(document_store::publication_digest(document.published.as_ref()).unwrap()).bind(list_id).execute(pool).await.unwrap();
    path
}

fn read(path: &std::path::Path) -> SavedListDocument {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Trace one real archived generation through phase-four failure and repair.
async fn saved_reconcile_finalization_recovers_same_generation_after_file_first_failure() {
    let (temporary, pool, coordinator) = fixture().await;
    let data = temporary.path().join("private-data");
    let source = "https://media.example.test/saved-reconcile";
    let list_id = queue_v2(&coordinator, source, &data, None).await;
    let initial = coordinator.claim_import_job().await.unwrap().unwrap();
    let archive = seal_v2_archive(
        &coordinator,
        &initial,
        &data,
        b"fixture first\nfixture second\n",
    )
    .await;
    commit_complete_v2_page(
        &coordinator,
        &initial,
        &archive,
        2,
        2,
        0,
        vec![
            list_record("fixture/saved-reconcile"),
            item_record("first", 0, "First"),
            item_record("second", 1, "Second"),
        ],
    )
    .await;
    let path = adopt_baseline(&pool, &temporary.path().join("media"), list_id, source).await;
    let baseline = read(&path);
    assert!(baseline.published.is_some());
    queue_v2(&coordinator, source, &data, Some(list_id)).await;
    sqlx::query(r#"UPDATE "AddonMediaListImportJob" SET "Purpose"=?,"Mode"=?,"CheckpointId"=NULL
        WHERE "AddonMediaListId"=? AND "Generation"=(SELECT MAX("Generation") FROM "AddonMediaListImportJob" WHERE "AddonMediaListId"=?)"#)
        .bind(encode_enum(AddonMediaListImportPurpose::Reconcile)).bind(encode_enum(AddonMediaListImportMode::Full))
        .bind(list_id).bind(list_id).execute(&pool).await.unwrap();
    let job = coordinator.claim_import_job().await.unwrap().unwrap();
    let archive = seal_v2_archive(&coordinator, &job, &data, b"fixture first\n").await;
    let outcome = coordinator
        .commit_import_page_checkpoint(
            &job,
            MediaListDiscoveryV2Page {
                complete: true,
                next_cursor: None,
                total_hint: Some(1),
                revision_token: Some("saved-reconcile".into()),
                strategy: WireIncrementalStrategy::SkipKnown,
                examined_count: 1,
                archived_skipped_count: 0,
                emitted_count: 1,
                records: vec![
                    list_record("fixture/saved-reconcile"),
                    item_record("first", 0, "First"),
                ],
            }
            .into(),
            Some(&archive),
        )
        .await
        .unwrap();
    assert!(matches!(
        outcome,
        PageCommitOutcome::Committed { complete: true }
    ));
    // Only phase four fails; prior header/items/detail checkpoints remain real.
    sqlx::raw_sql(r#"CREATE TRIGGER "FixtureSavedReconcileFailure" BEFORE UPDATE OF "SavedDocumentDigest" ON "AddonMediaList"
        WHEN EXISTS(SELECT 1 FROM "AddonMediaListImportJob" WHERE "AddonMediaListId"=NEW."Id" AND "FinalizationPhase"=4 AND "OverviewComplete"=0 AND "Generation"=(SELECT MAX("Generation") FROM "AddonMediaListImportJob" WHERE "AddonMediaListId"=NEW."Id"))
        BEGIN SELECT RAISE(ABORT,'fixture saved reconciliation failure'); END;"#).execute(&pool).await.unwrap();
    let error = coordinator
        .finalize_import_pass(&job)
        .await
        .expect_err("post-file finalization fails");
    coordinator
        .fail_import(&job, &error.to_string(), false)
        .await
        .unwrap();
    let state: (i32,i32,Option<String>,Option<String>,bool) = sqlx::query_as(r#"SELECT "State","FinalizationPhase","LeaseOwner","LeaseExpiresAt","OverviewComplete" FROM "AddonMediaListImportJob" WHERE "Id"=?"#)
        .bind(job.id).fetch_one(&pool).await.unwrap();
    assert_eq!(
        state,
        (
            encode_enum(AddonMediaListImportState::NeedsAttention),
            4,
            None,
            None,
            true
        )
    );
    let file = read(&path);
    assert_eq!(file.published, baseline.published);
    assert_eq!(
        file.draft
            .items
            .iter()
            .find(|item| item.content.provider_id == "second")
            .unwrap()
            .source_observation
            .as_ref()
            .unwrap()
            .observation_count,
        1
    );
    let observations: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaListItemTombstone" WHERE "AddonMediaListItemId" IN (SELECT "Id" FROM "AddonMediaListItem" WHERE "AddonMediaListId"=?)"#)
        .bind(list_id).fetch_one(&pool).await.unwrap();
    assert_eq!(observations, 0);
    let checkpoint: (i64,bool) = sqlx::query_as(r#"SELECT "checkpoint"."Id","checkpoint"."Committed" FROM "AddonMediaListImportCheckpoint" AS "checkpoint" JOIN "AddonMediaListImportJob" AS "job" ON "job"."CheckpointId"="checkpoint"."Id" WHERE "job"."Id"=?"#)
        .bind(job.id).fetch_one(&pool).await.unwrap();
    assert!(!checkpoint.1);
    let bytes = std::fs::read(&path).unwrap();
    sqlx::query(r#"DROP TRIGGER "FixtureSavedReconcileFailure""#)
        .execute(&pool)
        .await
        .unwrap();
    let mut mapped = mapped::MappedList::load(&pool, list_id)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        mapped.repair(&pool).await.unwrap(),
        mapped::Repair::Updated(None)
    ));
    drop(mapped);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    coordinator
        .control_import(list_id, AddonMediaListImportAction::Retry)
        .await
        .unwrap();
    let retried = coordinator.claim_import_job().await.unwrap().unwrap();
    assert_eq!((retried.id, retried.generation), (job.id, job.generation));
    assert!(retried.overview_complete);
    // This is the coordinator's overview-complete branch: no provider fetch.
    coordinator.finalize_import_pass(&retried).await.unwrap();
    assert_eq!(read(&path).published, baseline.published);
    assert_eq!(
        read(&path)
            .draft
            .items
            .iter()
            .find(|item| item.content.provider_id == "second")
            .unwrap()
            .source_observation
            .as_ref()
            .unwrap()
            .observation_count,
        1
    );
    let committed: bool = sqlx::query_scalar(
        r#"SELECT "Committed" FROM "AddonMediaListImportCheckpoint" WHERE "Id"=?"#,
    )
    .bind(checkpoint.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    // Reconciliation archives remain staged until explicit publication.
    assert!(!committed);
    let completed: (i32, bool, Option<String>) = sqlx::query_as(
        r#"SELECT "State", "OverviewComplete", "LeaseOwner" FROM "AddonMediaListImportJob" WHERE "Id"=?"#,
    )
    .bind(job.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        completed,
        (encode_enum(AddonMediaListImportState::Ready), false, None)
    );

    // Recovery suppresses replay of the pending observation, not future evidence.
    queue_v2(&coordinator, source, &data, Some(list_id)).await;
    sqlx::query(r#"UPDATE "AddonMediaListImportJob" SET "Purpose"=?,"Mode"=?,"CheckpointId"=NULL
        WHERE "AddonMediaListId"=? AND "Generation"=(SELECT MAX("Generation") FROM "AddonMediaListImportJob" WHERE "AddonMediaListId"=?)"#)
        .bind(encode_enum(AddonMediaListImportPurpose::Reconcile))
        .bind(encode_enum(AddonMediaListImportMode::Full))
        .bind(list_id).bind(list_id).execute(&pool).await.unwrap();
    let next = coordinator.claim_import_job().await.unwrap().unwrap();
    assert_ne!(next.id, job.id);
    assert_eq!(next.generation, job.generation + 1);
    let archive = seal_v2_archive(&coordinator, &next, &data, b"fixture first\n").await;
    commit_complete_v2_page(
        &coordinator,
        &next,
        &archive,
        1,
        1,
        0,
        vec![
            list_record("fixture/saved-reconcile"),
            item_record("first", 0, "First"),
        ],
    )
    .await;
    let after = read(&path);
    assert_eq!(after.published, baseline.published);
    let observation = after
        .draft
        .items
        .iter()
        .find(|item| item.content.provider_id == "second")
        .unwrap()
        .source_observation
        .as_ref()
        .unwrap();
    let original = file
        .draft
        .items
        .iter()
        .find(|item| item.content.provider_id == "second")
        .unwrap()
        .source_observation
        .as_ref()
        .unwrap();
    assert_eq!(observation.observation_count, 2);
    assert_eq!(observation.missing_since, original.missing_since);
    assert!(observation.confirmed_at.is_none());
    assert!(observation.reappeared_at.is_none());
    let evidence: (i32, i32, Option<String>) = sqlx::query_as(
        r#"SELECT "tombstone"."ObservationCount", "tombstone"."LastReconcileGeneration", "tombstone"."ConfirmedAt"
           FROM "AddonMediaListItemTombstone" AS "tombstone"
           JOIN "AddonMediaListItem" AS "item" ON "item"."Id"="tombstone"."AddonMediaListItemId"
           WHERE "item"."AddonMediaListId"=? AND "item"."ProviderId"='second'"#,
    )
    .bind(list_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(evidence, (2, next.generation, None));
}
