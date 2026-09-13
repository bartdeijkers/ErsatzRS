use super::*;
use ersatzrs_infra::addon_media_list_sync::AddonMediaListSynchronizer;

fn records() -> Vec<ersatzrs_addon_contract::PlaylistEntryMetadata> {
    vec![serde_json::from_value(serde_json::json!({
        "provider_id": "published-item", "chapter_input": "0:00 Opening\n0:30 Second chapter"
    }))
    .unwrap()]
}

async fn state(pool: &DbPool, list_id: i64) -> (i32, i32, String, Option<String>) {
    sqlx::query_as(
        r#"SELECT "list"."DraftRevision", "list"."ReviewState", "list"."SavedDocumentDigest",
                  "item"."ProviderChapterInput"
           FROM "AddonMediaList" AS "list" JOIN "AddonMediaListItem" AS "item"
             ON "item"."AddonMediaListId"="list"."Id" AND "item"."ProviderId"='published-item'
           WHERE "list"."Id"=?"#,
    )
    .bind(list_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn manifest(pool: &DbPool, list_id: i64) -> PathBuf {
    let path: String = sqlx::query_scalar(
        r#"SELECT "ManifestPath" FROM "AddonMediaListLibrary" WHERE "AddonMediaListId"=?"#,
    )
    .bind(list_id)
    .fetch_one(pool)
    .await
    .unwrap();
    path.into()
}

#[tokio::test]
async fn saved_scan_chapters_preserve_publication_revision_and_live_root_scan() {
    use ersatzrs_core::repositories::ManagedLibraryScanSpec;
    use ersatzrs_domain::{ManagedLibraryOperationKind, MediaSourceKind};
    let (_receiver, pool, copy, queued) = saved_writer_tests::adopted_job().await;
    publish_received(&pool, &queued).await;
    let id = queued.job.list_id;
    let path = manifest(&pool, id).await;
    let original: SavedListDocument =
        serde_json::from_slice(&std::fs::read(&copy).unwrap()).unwrap();
    let before = state(&pool, id).await;
    let repos = ersatzrs_infra::repositories::make_repositories(pool.clone());
    let operation = repos
        .managed_library_operations
        .enqueue_scan(
            ManagedLibraryOperationKind::ScheduledScan,
            queued.operation.library_id,
            ManagedLibraryScanSpec {
                provider: MediaSourceKind::Local,
                show_id: None,
                deep_scan: false,
            },
            true,
        )
        .await
        .unwrap()
        .unwrap();
    let claim = repos
        .managed_library_operations
        .claim_operation(operation.id, Utc::now())
        .await
        .unwrap()
        .unwrap();

    // Exercise the scanner entry point, not merely the new infrastructure helper.
    persist_provider_chapter_inputs(&pool, &path, ADDON, &records()).await;
    let after = state(&pool, id).await;
    assert_eq!((after.0, after.1), (before.0, before.1));
    assert_eq!(after.3, records()[0].chapter_input);
    assert_ne!(after.2, before.2);
    let bytes = std::fs::read(&copy).unwrap();
    let saved: SavedListDocument = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(saved.published, original.published);
    // The receiving installation keeps its own revision, not the sender's number.
    assert_eq!(saved.draft.revision, before.0);
    assert_eq!(
        saved
            .draft
            .items
            .iter()
            .find(|item| item.content.provider_id == "published-item")
            .unwrap()
            .content
            .provider_chapter_input,
        records()[0].chapter_input.clone().unwrap()
    );
    let lease: (i64, String, String) = sqlx::query_as(
        r#"SELECT "OperationId", "OwnerToken", "LeaseOwner" FROM "ManagedLibraryLease" WHERE "LibraryId"=?"#,
    ).bind(queued.operation.library_id).fetch_one(&pool).await.unwrap();
    assert_eq!(
        lease,
        (operation.id, claim.operation.owner_token, claim.lease_owner)
    );
    persist_provider_chapter_inputs(&pool, &path, ADDON, &records()).await;
    assert_eq!(state(&pool, id).await, after);
    assert_eq!(std::fs::read(&copy).unwrap(), bytes);
}

#[tokio::test]
async fn saved_scan_chapters_file_first_failure_recovers_without_changing_publication() {
    let (_receiver, pool, copy, queued) = saved_writer_tests::adopted_job().await;
    publish_received(&pool, &queued).await;
    let id = queued.job.list_id;
    let path = manifest(&pool, id).await;
    let original: SavedListDocument =
        serde_json::from_slice(&std::fs::read(&copy).unwrap()).unwrap();
    let before = state(&pool, id).await;
    sqlx::raw_sql(r#"CREATE TRIGGER "FixtureChapterSaveFailure" BEFORE UPDATE OF "SavedDocumentDigest" ON "AddonMediaList"
        BEGIN SELECT RAISE(ABORT, 'fixture chapter save failure'); END;"#).execute(&pool).await.unwrap();
    let sync = AddonMediaListSynchronizer::new(pool.clone());
    let error = sync
        .save_provider_chapter_inputs(&path, ADDON, &records())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("database refresh is pending"));
    assert_eq!(state(&pool, id).await, before);
    let bytes = std::fs::read(&copy).unwrap();
    let saved: SavedListDocument = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(saved.published, original.published);
    assert_eq!(
        saved
            .draft
            .items
            .iter()
            .find(|item| item.content.provider_id == "published-item")
            .unwrap()
            .content
            .provider_chapter_input,
        records()[0].chapter_input.clone().unwrap()
    );
    sqlx::query(r#"DROP TRIGGER "FixtureChapterSaveFailure""#)
        .execute(&pool)
        .await
        .unwrap();
    assert!(sync
        .save_provider_chapter_inputs(&path, ADDON, &records())
        .await
        .is_err());
    assert_eq!(state(&pool, id).await, before);
    assert_eq!(std::fs::read(&copy).unwrap(), bytes);
    let repaired = ersatzrs_infra::addon_media_list_saved::admission::discover_copied_lists(&pool)
        .await
        .unwrap();
    assert_eq!((repaired.repaired, repaired.refused), (1, 0));
    sync.save_provider_chapter_inputs(&path, ADDON, &records())
        .await
        .unwrap();
    assert_eq!(state(&pool, id).await.3, records()[0].chapter_input);
    assert_eq!(std::fs::read(&copy).unwrap(), bytes);
}
