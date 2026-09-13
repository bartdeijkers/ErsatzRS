use super::*;
use ersatzrs_core::repositories::{
    AddonMediaListLifecycleRepository as _, AddonMediaListRevisionArtifacts,
};
use ersatzrs_domain::{AddonMediaListPresentationKind, AddonMediaListReviewState};

async fn fixture() -> SqliteAddonMediaListRepository {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    assert!(sqlx::query_scalar::<_, bool>("PRAGMA foreign_keys")
        .fetch_one(&pool)
        .await
        .unwrap());
    sqlx::raw_sql(
        r#"INSERT INTO "AddonInstallation"
           ("AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
            "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt")
           VALUES ('org.example.source', '1', 1, 'fixture', 'fixture', '{}', 'addons/fixture',
                   '2026-09-13', '2026-09-13');
           INSERT INTO "MediaSource" ("Id") VALUES (1);
           INSERT INTO "Library" ("Id", "MediaKind", "MediaSourceId", "Name") VALUES (1, 3, 1, 'Fixture');
           INSERT INTO "AddonMediaList"
           ("Id", "AddonId", "ProviderId", "SourceUrl", "Name", "DraftRevision", "PublishedRevision")
           VALUES (1, 'org.example.source', 'list', 'https://example.invalid/list', 'Retained title', 4, 2);
           INSERT INTO "AddonMediaListItem"
           ("Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Title", "Kind", "Rank", "Published")
           VALUES (10, 1, 'retained-item', 'Retained item', 'Retained item', 4, 0, 1);"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    let repository = SqliteAddonMediaListRepository(pool);
    repository
        .retain_published_revision(
            1,
            2,
            4,
            chrono::DateTime::parse_from_rfc3339("2026-09-13T12:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            &[],
            &AddonMediaListRevisionArtifacts::default(),
        )
        .await
        .unwrap();
    sqlx::raw_sql(
        r#"UPDATE "AddonMediaList" SET "Name"='New draft', "DraftRevision"=7 WHERE "Id"=1;
           UPDATE "AddonMediaListItem" SET "InDraft"=0, "Included"=0 WHERE "Id"=10;
           INSERT INTO "AddonMediaListItem"
           ("Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Title", "Kind", "Rank")
           VALUES (20, 1, 'draft-item', 'New draft item', 'New draft item', 4, 0);
           INSERT INTO "ManagedLibraryOperation"
           ("Id", "LibraryId", "OwnerToken", "Kind", "State", "CreatedAt", "UpdatedAt")
           VALUES (100, 1, 'fixture-owner', 1, 1, '2026-09-13', '2026-09-13');
           INSERT INTO "AddonMediaListPublicationJob"
           ("Id", "OperationId", "AddonMediaListId", "Generation", "SavedRevision",
            "TargetPublishedRevision", "SourcePublishedRevision", "Phase", "CreatedAt", "UpdatedAt")
           VALUES (100, 100, 1, 1, 7, 3, 2, 1, '2026-09-13', '2026-09-13');"#,
    )
    .execute(&repository.0)
    .await
    .unwrap();
    repository
}

#[tokio::test]
async fn saved_folder_origin_requires_exact_event_and_keeps_newer_draft() {
    let repo = fixture().await;
    let fence = PublicationFence {
        operation_id: 100,
        generation: 1,
        saved_revision: 7,
        owner_token: "fixture-owner",
        lease_owner: "fixture-lease",
    };
    sqlx::query(r#"UPDATE "AddonMediaList" SET "DraftRevision"=8 WHERE "Id"=1"#)
        .execute(&repo.0)
        .await
        .unwrap();
    assert!(
        !repo
            .source_snapshot_and_origin_for_fence(1, fence)
            .await
            .unwrap()
            .1
    );
    sqlx::query(
        r#"INSERT INTO "AddonMediaListPublicationEvent"
           ("PublicationJobId", "Sequence", "DedupeKey", "Kind", "EnqueueTrigger", "OccurredAt")
           VALUES (100,1,'enqueued',?,?,'2026-09-13')"#,
    )
    .bind(crate::repositories::common::encode_enum(
        ersatzrs_domain::AddonMediaListPublicationEventKind::Enqueued,
    ))
    .bind(crate::repositories::common::encode_enum(
        ersatzrs_domain::AddonMediaListPublicationEnqueueTrigger::SavedFolder,
    ))
    .execute(&repo.0)
    .await
    .unwrap();
    let (snapshot, saved_folder) = repo
        .source_snapshot_and_origin_for_fence(1, fence)
        .await
        .unwrap();
    assert!(saved_folder);
    assert_eq!(snapshot.unwrap().name, "Retained title");
    assert!(repo
        .source_snapshot_and_origin_for_fence(
            1,
            PublicationFence {
                generation: 2,
                ..fence
            }
        )
        .await
        .is_err());
    sqlx::query(r#"UPDATE "AddonMediaListPublicationJob" SET "SourcePublishedRevision"=NULL WHERE "Id"=100"#)
        .execute(&repo.0).await.unwrap();
    assert!(repo
        .source_snapshot_and_origin_for_fence(1, fence)
        .await
        .is_err());
    assert_eq!(
        sqlx::query_scalar::<_, i32>(
            r#"SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id"=1"#
        )
        .fetch_one(&repo.0)
        .await
        .unwrap(),
        8
    );
}

#[tokio::test]
async fn active_publication_snapshot_uses_exact_revision_without_changing_newer_draft() {
    let repo = fixture().await;
    let snapshot = repo.active_publication_snapshot(1).await.unwrap().unwrap();
    assert_eq!(snapshot.revision, 2);
    assert_eq!(snapshot.source_draft_revision, 4);
    assert_eq!(snapshot.name, "Retained title");
    assert_eq!(snapshot.items.len(), 1);
    assert_eq!(snapshot.items[0].provider_id, "retained-item");
    // A later retained snapshot also cannot supersede PublishedRevision.
    sqlx::query(r#"UPDATE "AddonMediaList" SET "PublishedRevision"=3 WHERE "Id"=1"#)
        .execute(&repo.0)
        .await
        .unwrap();
    repo.retain_published_revision(
        1,
        3,
        7,
        chrono::DateTime::parse_from_rfc3339("2026-09-13T13:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
        &[],
        &AddonMediaListRevisionArtifacts::default(),
    )
    .await
    .unwrap();
    sqlx::query(r#"UPDATE "AddonMediaList" SET "PublishedRevision"=2 WHERE "Id"=1"#)
        .execute(&repo.0)
        .await
        .unwrap();
    assert_eq!(
        repo.active_publication_snapshot(1).await.unwrap().unwrap(),
        snapshot
    );
    let draft: (String, i32, i32) = sqlx::query_as(
        r#"SELECT "Name", "DraftRevision", "PublishedRevision" FROM "AddonMediaList" WHERE "Id"=1"#,
    )
    .fetch_one(&repo.0)
    .await
    .unwrap();
    assert_eq!(draft, ("New draft".into(), 7, 2));
}

#[tokio::test]
async fn active_publication_snapshot_preserves_unbound_legacy_and_no_publication() {
    let repo = fixture().await;
    assert!(repo.active_publication_snapshot(999).await.is_err());
    for revision in [0, 999] {
        sqlx::query(r#"UPDATE "AddonMediaList" SET "PublishedRevision"=? WHERE "Id"=1"#)
            .bind(revision)
            .execute(&repo.0)
            .await
            .unwrap();
        assert!(repo.active_publication_snapshot(1).await.unwrap().is_none());
    }
    sqlx::raw_sql(
        r#"UPDATE "AddonMediaList" SET "PublishedRevision"=2 WHERE "Id"=1;
        UPDATE "AddonMediaListRevision" SET "SnapshotDigest"=NULL, "SnapshotVersion"=NULL WHERE "Revision"=2;"#,
    )
    .execute(&repo.0)
    .await
    .unwrap();
    assert!(repo.active_publication_snapshot(1).await.unwrap().is_none());
}

#[tokio::test]
async fn active_publication_snapshot_refuses_modern_stream_without_digest() {
    let repo = fixture().await;
    sqlx::query(r#"UPDATE "AddonMediaListRevision" SET "SnapshotDigest"=NULL WHERE "Revision"=2"#)
        .execute(&repo.0)
        .await
        .unwrap();
    assert!(repo.active_publication_snapshot(1).await.is_err());
}

#[tokio::test]
async fn active_publication_snapshot_refuses_corrupt_modern_content() {
    let repo = fixture().await;
    sqlx::query(
        r#"UPDATE "AddonMediaListRevision" SET "Name"='Corrupt snapshot' WHERE "Revision"=2"#,
    )
    .execute(&repo.0)
    .await
    .unwrap();
    assert!(repo.active_publication_snapshot(1).await.is_err());
}

#[tokio::test]
async fn active_publication_snapshot_refuses_missing_digest_with_playback_bindings() {
    for binding in [
        r#"UPDATE "AddonMediaListRevisionItem" SET "PreparedArtifactId"=50"#,
        r#"UPDATE "AddonMediaListRevisionItem" SET "PlaybackTimelineJson"='{}'"#,
        r#"UPDATE "AddonMediaListRevisionItem" SET "SubtitleArtifactIdsJson"='[50]'"#,
    ] {
        let repo = fixture().await;
        sqlx::raw_sql(r#"INSERT INTO "AddonMediaListArtifact"
            ("Id","AddonMediaListId","AddonMediaListItemId","CacheKey","Kind","State","CreatedAt","UpdatedAt")
            VALUES (50,1,10,'fixture-artifact',1,2,'2026-09-13','2026-09-13');
            UPDATE "AddonMediaListRevision" SET "SnapshotDigest"=NULL, "SnapshotVersion"=NULL WHERE "Revision"=2;"#)
            .execute(&repo.0).await.unwrap();
        sqlx::query(binding).execute(&repo.0).await.unwrap();
        assert!(
            repo.active_publication_snapshot(1).await.is_err(),
            "{binding}"
        );
    }
}

fn journal() -> serde_json::Value {
    serde_json::json!({
        "operation_id": 100, "generation": 1, "saved_revision": 7,
        "previous_review_state": AddonMediaListReviewState::Ready,
        "previous_presentation_kind": AddonMediaListPresentationKind::RemoteStreams,
        "previous_items": [], "previous_playlist": null
    })
}

async fn active(repository: &SqliteAddonMediaListRepository, value: &serde_json::Value) {
    sqlx::query(
        r#"UPDATE "AddonMediaList" SET "ReviewState"=3, "PublicationJournalJson"=? WHERE "Id"=1"#,
    )
    .bind(value.to_string())
    .execute(&repository.0)
    .await
    .unwrap();
}

#[tokio::test]
async fn publication_commit_source_distinguishes_receiving_from_materialized_restore() {
    let repo = fixture().await;
    active(&repo, &journal()).await;
    let before: (String, i32) =
        sqlx::query_as(r#"SELECT "Name", "DraftRevision" FROM "AddonMediaList" WHERE "Id"=1"#)
            .fetch_one(&repo.0)
            .await
            .unwrap();
    let (source, materialized) = repo.publication_commit_source(1).await.unwrap();
    assert!(source.is_some());
    assert!(!materialized);
    sqlx::query(r#"UPDATE "AddonMediaListPublicationJob" SET "RestoreMaterializedAt"='2026-09-13T12:00:00Z' WHERE "Id"=100"#)
        .execute(&repo.0).await.unwrap();
    let (restored, materialized) = repo.publication_commit_source(1).await.unwrap();
    assert_eq!(restored, source);
    assert!(materialized);
    assert_eq!(repo.publication_source_snapshot(1).await.unwrap(), source);
    let after: (String, i32) =
        sqlx::query_as(r#"SELECT "Name", "DraftRevision" FROM "AddonMediaList" WHERE "Id"=1"#)
            .fetch_one(&repo.0)
            .await
            .unwrap();
    assert_eq!(after, before);
    // A marker cannot manufacture a source for an ordinary publication.
    sqlx::query(r#"UPDATE "AddonMediaListPublicationJob" SET "SourcePublishedRevision"=NULL WHERE "Id"=100"#)
        .execute(&repo.0).await.unwrap();
    assert_eq!(
        repo.publication_commit_source(1).await.unwrap(),
        (None, false)
    );
}

#[tokio::test]
async fn publication_source_reads_fenced_snapshot_without_changing_draft() {
    let repo = fixture().await;
    let fence = PublicationFence {
        operation_id: 100,
        generation: 1,
        saved_revision: 7,
        owner_token: "fixture-owner",
        lease_owner: "fixture-lease",
    };
    let before_journal = repo
        .source_snapshot_for_fence(1, fence)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(before_journal.name, "Retained title");
    assert_eq!(before_journal.items[0].provider_id, "retained-item");
    active(&repo, &journal()).await;
    let active_snapshot = repo.publication_source_snapshot(1).await.unwrap().unwrap();
    assert_eq!(
        active_snapshot.snapshot_digest,
        before_journal.snapshot_digest
    );
    let draft: (String, i32) =
        sqlx::query_as(r#"SELECT "Name", "DraftRevision" FROM "AddonMediaList" WHERE "Id"=1"#)
            .fetch_one(&repo.0)
            .await
            .unwrap();
    assert_eq!(draft, ("New draft".into(), 7));
    let ids: Vec<i64> =
        sqlx::query_scalar(r#"SELECT "Id" FROM "AddonMediaListItem" WHERE "InDraft"=1"#)
            .fetch_all(&repo.0)
            .await
            .unwrap();
    assert_eq!(ids, [20]);
}

#[tokio::test]
async fn publication_source_ordinary_and_unfenced_legacy_have_no_snapshot() {
    let repo = fixture().await;
    assert!(repo.publication_source_snapshot(1).await.unwrap().is_none());
    let mut legacy = journal();
    for field in ["operation_id", "generation", "saved_revision"] {
        legacy.as_object_mut().unwrap().remove(field);
    }
    active(&repo, &legacy).await;
    assert!(repo.publication_source_snapshot(1).await.unwrap().is_none());
    sqlx::query(r#"UPDATE "AddonMediaListPublicationJob" SET "SourcePublishedRevision"=NULL"#)
        .execute(&repo.0)
        .await
        .unwrap();
    active(&repo, &journal()).await;
    assert!(repo.publication_source_snapshot(1).await.unwrap().is_none());
}

#[tokio::test]
async fn publication_source_refuses_partial_malformed_and_mismatched_scope() {
    let repo = fixture().await;
    for field in ["operation_id", "generation", "saved_revision"] {
        let mut partial = journal();
        partial.as_object_mut().unwrap().remove(field);
        active(&repo, &partial).await;
        assert!(repo.publication_source_snapshot(1).await.is_err());
        let mut mismatched = journal();
        mismatched[field] = serde_json::json!(999);
        active(&repo, &mismatched).await;
        assert!(repo.publication_source_snapshot(1).await.is_err());
    }
    for value in [Some("{"), None] {
        sqlx::query(r#"UPDATE "AddonMediaList" SET "PublicationJournalJson"=? WHERE "Id"=1"#)
            .bind(value)
            .execute(&repo.0)
            .await
            .unwrap();
        assert!(repo.publication_source_snapshot(1).await.is_err());
    }
}

#[tokio::test]
async fn publication_source_refuses_missing_or_corrupt_retained_snapshot() {
    let repo = fixture().await;
    active(&repo, &journal()).await;
    for revision in [0, 999] {
        sqlx::query(r#"UPDATE "AddonMediaListPublicationJob" SET "SourcePublishedRevision"=?"#)
            .bind(revision)
            .execute(&repo.0)
            .await
            .unwrap();
        assert!(repo.publication_source_snapshot(1).await.is_err());
    }
    sqlx::query(r#"UPDATE "AddonMediaListPublicationJob" SET "SourcePublishedRevision"=2"#)
        .execute(&repo.0)
        .await
        .unwrap();
    sqlx::query(
        r#"UPDATE "AddonMediaListRevision" SET "Name"='Corrupt retained title' WHERE "Revision"=2"#,
    )
    .execute(&repo.0)
    .await
    .unwrap();
    assert!(repo.publication_source_snapshot(1).await.is_err());
}
