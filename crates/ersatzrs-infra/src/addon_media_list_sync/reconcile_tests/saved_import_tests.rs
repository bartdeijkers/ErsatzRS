//! Real import phase transactions on an adopted folder, not activation proof.

use super::saved_detail_tests::{document, saved_fixture, SavedFixture};
use super::*;
use crate::addon_media_list_saved::{admission, document_io};

const OWNER: &str = "saved-import-owner";

async fn fixture() -> SavedFixture {
    let fixture = saved_fixture().await;
    // The copy fixture queues activation. Explicitly terminate that queued
    // operation before testing independent import transactions; no lease exists.
    let leases: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "ManagedLibraryLease""#)
        .fetch_one(&fixture.pool)
        .await
        .unwrap();
    assert_eq!(leases, 0);
    sqlx::query(r#"UPDATE "ManagedLibraryOperation" SET "State"=4, "CompletedAt"=?
        WHERE "Id" IN (SELECT "OperationId" FROM "AddonMediaListPublicationJob" WHERE "AddonMediaListId"=?)"#)
        .bind(encode_datetime(Utc::now())).bind(fixture.list_id).execute(&fixture.pool).await.unwrap();
    fixture
}

async fn job(f: &SavedFixture, generation: i32, reconcile: bool) -> i64 {
    sqlx::query_scalar(r#"INSERT INTO "AddonMediaListImportJob"
        ("AddonMediaListId","Generation","State","RecordCapability","ImportCapability",
         "Purpose","Mode","OverviewComplete","FinalizationPhase","LeaseOwner","LeaseExpiresAt","CreatedAt","UpdatedAt")
        VALUES (?,?,2,'media-list.list.v1','media-list.import.v2',?,0,1,0,?,?,?,?) RETURNING "Id""#)
        .bind(f.list_id).bind(generation)
        .bind(encode_enum(if reconcile { ersatzrs_domain::AddonMediaListImportPurpose::Reconcile } else { ersatzrs_domain::AddonMediaListImportPurpose::Refresh }))
        .bind(OWNER).bind(encode_datetime(Utc::now()+chrono::Duration::hours(1)))
        .bind(encode_datetime(Utc::now())).bind(encode_datetime(Utc::now()))
        .fetch_one(&f.pool).await.unwrap()
}

async fn phase(f: &SavedFixture, job_id: i64) -> i32 {
    sqlx::query_scalar(r#"SELECT "FinalizationPhase" FROM "AddonMediaListImportJob" WHERE "Id"=?"#)
        .bind(job_id)
        .fetch_one(&f.pool)
        .await
        .unwrap()
}

async fn header(f: &SavedFixture, sync: &AddonMediaListSynchronizer, job_id: i64, name: &str) {
    let mut list = sync.load_list(f.list_id).await.unwrap();
    list.name = name.into();
    let metadata = AddonMediaListNfoMetadata {
        title: name.into(),
        ..Default::default()
    };
    assert!(sync
        .save_import_list_header(job_id, "stale", &list, &metadata, false, true)
        .await
        .is_err());
    assert_eq!(phase(f, job_id).await, 0);
    sync.save_import_list_header(job_id, OWNER, &list, &metadata, false, true)
        .await
        .unwrap();
    assert_eq!(phase(f, job_id).await, 1);
    assert_eq!(document(f).draft.presentation.name, name);
    assert_eq!(document(f).draft.metadata.provider["title"], name);
}

fn new_item() -> EnumeratedAddonMediaListItem {
    let mut item = source_item("New imported item", 1);
    item.provider_id = "new-item".into();
    item.source_url = Some("https://media.example.test/new".into());
    item.guids = vec!["fixture:new-item".into()];
    item
}

#[tokio::test]
async fn saved_import_header_identity_file_first_failure_repairs_before_same_phase_retry() {
    let f = fixture().await;
    let sync = AddonMediaListSynchronizer::new(f.pool.clone());
    let job_id = job(&f, 1, false).await;
    let before = document(&f);
    let mut list = sync.load_list(f.list_id).await.unwrap();
    list.provider_id = "renamed-provider-list".into();
    list.name = "Renamed imported list".into();
    let metadata = AddonMediaListNfoMetadata {
        title: list.name.clone(),
        ..Default::default()
    };
    sqlx::raw_sql(r#"CREATE TRIGGER "FixtureHeaderDigestFailure" BEFORE UPDATE OF "SavedDocumentDigest" ON "AddonMediaList"
        BEGIN SELECT RAISE(ABORT,'fixture header failure'); END;"#).execute(&f.pool).await.unwrap();
    assert!(sync
        .save_import_list_header(job_id, OWNER, &list, &metadata, false, true)
        .await
        .is_err());
    assert_eq!(phase(&f, job_id).await, 0);
    let unchanged = sync.load_list(f.list_id).await.unwrap();
    assert_eq!(unchanged.provider_id, before.provider_id);
    assert_eq!(unchanged.name, before.draft.presentation.name);
    let ahead = document(&f);
    assert_eq!(ahead.provider_id, list.provider_id);
    assert_eq!(ahead.draft.presentation.name, list.name);
    assert_eq!(ahead.draft.items, before.draft.items);
    assert_eq!(ahead.published, before.published);
    let bytes = std::fs::read(&f.path).unwrap();
    sqlx::query(r#"DROP TRIGGER "FixtureHeaderDigestFailure""#)
        .execute(&f.pool)
        .await
        .unwrap();
    assert!(sync
        .save_import_list_header(job_id, OWNER, &list, &metadata, false, true)
        .await
        .is_err());
    assert_eq!(phase(&f, job_id).await, 0);
    assert_eq!(std::fs::read(&f.path).unwrap(), bytes);
    let repaired = admission::discover_copied_lists(&f.pool).await.unwrap();
    assert_eq!(repaired.refused, 0);
    assert_eq!(repaired.repaired, 1);
    assert!(repaired.admitted.is_empty());
    assert_eq!(
        sync.load_list(f.list_id).await.unwrap().provider_id,
        list.provider_id
    );
    sync.save_import_list_header(job_id, OWNER, &list, &metadata, false, true)
        .await
        .unwrap();
    assert_eq!(phase(&f, job_id).await, 1);
    assert_eq!(document(&f).published, before.published);
    assert_eq!(document(&f).draft.items, before.draft.items);
}

#[tokio::test]
async fn saved_import_phases_preserve_publication_and_readopt_and_remove_draft_members() {
    let f = fixture().await;
    let mut saved = document(&f);
    let published = saved.published.clone();
    // Receiving an independent empty draft keeps the publication-only owner.
    saved.draft.items.clear();
    document_io::write_document(&mut std::fs::File::create(&f.path).unwrap(), &saved).unwrap();
    let repaired = admission::discover_copied_lists(&f.pool).await.unwrap();
    assert_eq!(repaired.repaired, 1);
    assert_eq!(repaired.refused, 0);
    assert!(!sqlx::query_scalar::<_, bool>(
        r#"SELECT "InDraft" FROM "AddonMediaListItem" WHERE "Id"=?"#
    )
    .bind(f.item_id)
    .fetch_one(&f.pool)
    .await
    .unwrap());

    let sync = AddonMediaListSynchronizer::new(f.pool.clone());
    let first = job(&f, 1, false).await;
    header(&f, &sync, first, "Imported header").await;
    assert_eq!(document(&f).published, published);
    let mut fragment = chapter_fragment(2);
    fragment.provider_id = "provider-item:chapter:0".into();
    sync.save_import_items(
        first,
        OWNER,
        f.list_id,
        &[source_item("Re-adopted", 0), new_item(), fragment],
        false,
    )
    .await
    .unwrap();
    assert_eq!(phase(&f, first).await, 2);
    let saved = document(&f);
    assert_eq!(saved.published, published);
    assert_eq!(saved.draft.items.len(), 3);
    assert!(saved
        .draft
        .items
        .iter()
        .any(|item| item.content.provider_id == "new-item"));
    let (id, in_draft): (i64, bool) = sqlx::query_as(r#"SELECT "Id","InDraft" FROM "AddonMediaListItem" WHERE "AddonMediaListId"=? AND "ProviderId"='provider-item'"#)
        .bind(f.list_id).fetch_one(&f.pool).await.unwrap();
    assert_eq!(id, f.item_id);
    assert!(in_draft);

    let second = job(&f, 2, false).await;
    header(&f, &sync, second, "Next header").await;
    sync.save_import_items(
        second,
        OWNER,
        f.list_id,
        &[source_item("Re-adopted", 0), new_item()],
        false,
    )
    .await
    .unwrap();
    assert_eq!(phase(&f, second).await, 2);
    let saved = document(&f);
    assert_eq!(saved.published, published);
    assert_eq!(saved.draft.items.len(), 2);
    assert!(!saved
        .draft
        .items
        .iter()
        .any(|item| item.content.parent_provider_id.is_some()));
}

#[tokio::test]
async fn saved_reconcile_item_phase_file_first_failure_requires_repair_before_retry() {
    let f = fixture().await;
    let sync = AddonMediaListSynchronizer::new(f.pool.clone());
    let job_id = job(&f, 1, true).await;
    header(&f, &sync, job_id, "Reconcile header").await;
    let published = document(&f).published;
    let before: (i32, String) = sqlx::query_as(
        r#"SELECT "DraftRevision","SavedDocumentDigest" FROM "AddonMediaList" WHERE "Id"=?"#,
    )
    .bind(f.list_id)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    sqlx::raw_sql(r#"CREATE TRIGGER "FixtureImportDigestFailure" BEFORE UPDATE OF "SavedDocumentDigest" ON "AddonMediaList"
        BEGIN SELECT RAISE(ABORT,'fixture import failure'); END;"#).execute(&f.pool).await.unwrap();
    let incoming = [source_item("Reconciled", 0), new_item()];
    assert!(sync
        .save_reconcile_items(job_id, OWNER, f.list_id, 1, &incoming)
        .await
        .is_err());
    assert_eq!(phase(&f, job_id).await, 1);
    let actual: (i32, String) = sqlx::query_as(
        r#"SELECT "DraftRevision","SavedDocumentDigest" FROM "AddonMediaList" WHERE "Id"=?"#,
    )
    .bind(f.list_id)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(actual, before);
    let new_items: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaListItem" WHERE "AddonMediaListId"=? AND "ProviderId"='new-item'"#)
        .bind(f.list_id).fetch_one(&f.pool).await.unwrap();
    assert_eq!(new_items, 0, "the new owner must roll back with its phase");
    assert_eq!(document(&f).published, published);
    assert_eq!(document(&f).draft.items.len(), 2);
    let bytes = std::fs::read(&f.path).unwrap();
    sqlx::query(r#"DROP TRIGGER "FixtureImportDigestFailure""#)
        .execute(&f.pool)
        .await
        .unwrap();
    assert!(sync
        .save_reconcile_items(job_id, OWNER, f.list_id, 1, &incoming)
        .await
        .is_err());
    assert_eq!(phase(&f, job_id).await, 1);
    assert_eq!(std::fs::read(&f.path).unwrap(), bytes);
    let repaired = admission::discover_copied_lists(&f.pool).await.unwrap();
    assert_eq!(repaired.refused, 0);
    assert_eq!(repaired.repaired, 1);
    assert!(repaired.admitted.is_empty());
    sync.save_reconcile_items(job_id, OWNER, f.list_id, 1, &incoming)
        .await
        .unwrap();
    assert_eq!(phase(&f, job_id).await, 2);
    let saved = document(&f);
    assert_eq!(saved.published, published);
    assert_eq!(saved.draft.items.len(), 2);
    assert!(saved
        .draft
        .items
        .iter()
        .any(|item| item.content.provider_id == "new-item"));
}
