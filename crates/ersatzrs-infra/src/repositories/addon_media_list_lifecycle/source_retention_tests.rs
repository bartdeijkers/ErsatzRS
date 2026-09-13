use super::*;

fn completed_at() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339("2026-09-13T12:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc)
}

async fn fixture() -> (
    SqliteAddonMediaListRepository,
    AddonMediaListRevisionSnapshot,
) {
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
           VALUES ('org.example.retention', '1', 1, 'fixture', 'fixture', '{}', 'addons/fixture',
                   '2026-09-13', '2026-09-13');
           INSERT INTO "MediaSource" ("Id") VALUES (1);
           INSERT INTO "Library" ("Id", "MediaKind", "MediaSourceId", "Name")
           VALUES (1, 3, 1, 'Fixture');
           INSERT INTO "AddonMediaList"
           ("Id", "AddonId", "ProviderId", "SourceUrl", "Name", "DraftRevision", "PublishedRevision")
           VALUES (1, 'org.example.retention', 'list', 'https://example.invalid/list', 'Retained list', 4, 2);
           INSERT INTO "AddonMediaListItem"
           ("Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Title", "Kind", "Rank", "Published")
           VALUES (10, 1, 'retained-item', 'Retained item', 'Retained item', 4, 0, 1);"#,
    ).execute(&pool).await.unwrap();
    let repo = SqliteAddonMediaListRepository(pool);
    let source = repo
        .retain_published_revision(
            1,
            2,
            4,
            completed_at(),
            &[],
            &AddonMediaListRevisionArtifacts::default(),
        )
        .await
        .unwrap();
    sqlx::raw_sql(
        r#"UPDATE "AddonMediaList" SET "Name"='New draft', "DraftRevision"=7,
               "PublishedRevision"=3, "MetadataOverrideJson"='{"title":"Draft metadata"}' WHERE "Id"=1;
           UPDATE "AddonMediaListItem" SET "InDraft"=0, "Included"=0,
               "Title"='Draft title' WHERE "Id"=10;
           INSERT INTO "AddonMediaListItem"
           ("Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Title", "Kind", "Rank")
           VALUES (20, 1, 'draft-only', 'New draft item', 'New draft item', 4, 1);
           INSERT INTO "ManagedLibraryOperation"
           ("Id", "LibraryId", "OwnerToken", "Kind", "State", "LeaseOwner",
            "LeaseExpiresAt", "CreatedAt", "UpdatedAt")
           VALUES (100, 1, 'owner', 3, 1, 'worker', '2026-09-13T14:00:00Z', '2026-09-13', '2026-09-13');
           INSERT INTO "ManagedLibraryLease"
           ("LibraryId", "OperationId", "OwnerToken", "LeaseOwner", "LeaseExpiresAt", "HeartbeatAt", "UpdatedAt")
           VALUES (1, 100, 'owner', 'worker', '2026-09-13T14:00:00Z', '2026-09-13', '2026-09-13');
           INSERT INTO "AddonMediaListPublicationJob"
           ("Id", "OperationId", "AddonMediaListId", "Generation", "SavedRevision",
            "TargetPublishedRevision", "SourcePublishedRevision", "Phase", "CreatedAt", "UpdatedAt")
           VALUES (100, 100, 1, 1, 7, 3, 2, 1, '2026-09-13', '2026-09-13');
           INSERT INTO "AddonMediaListPublicationEvent"
           ("PublicationJobId", "Sequence", "DedupeKey", "Kind", "EnqueueTrigger", "OccurredAt")
           VALUES (100, 1, 'enqueued', 1, 3, '2026-09-13T12:00:00Z');"#,
    ).execute(&repo.0).await.unwrap();
    (repo, source)
}

async fn draft_state(
    repo: &SqliteAddonMediaListRepository,
) -> (String, i32, String, Vec<(String, String, bool)>) {
    let (name, revision, metadata): (String, i32, String) = sqlx::query_as(
        r#"SELECT "Name", "DraftRevision", "MetadataOverrideJson" FROM "AddonMediaList" WHERE "Id"=1"#,
    ).fetch_one(&repo.0).await.unwrap();
    let items = sqlx::query_as(
        r#"SELECT "ProviderId", "Title", "InDraft" FROM "AddonMediaListItem" WHERE "AddonMediaListId"=1 ORDER BY "Id""#,
    ).fetch_all(&repo.0).await.unwrap();
    (name, revision, metadata, items)
}

#[tokio::test]
async fn independent_source_retention_preserves_draft_and_is_idempotent() {
    let (repo, mut expected) = fixture().await;
    let draft = draft_state(&repo).await;
    let completed = completed_at() + chrono::Duration::minutes(5);
    let blob = repo
        .register_revision_file_blob(1, "retained/verified.yml", &"a".repeat(64), 12, completed)
        .await
        .unwrap();
    let files = vec![AddonMediaListRevisionFile {
        blob_id: blob.id,
        provider_id: None,
        published_relative_path: "retained.yml".into(),
        storage_relative_path: blob.storage_relative_path,
        sha256: blob.sha256,
        byte_length: blob.byte_length,
    }];
    expected.revision = 3;
    expected.source_draft_revision = 7;
    expected.completed_at = completed;
    expected.options.revision = 3;
    expected.options.source_draft_revision = Some(7);
    expected.options.completed_at = Some(completed);
    expected.files.clone_from(&files);
    expected.snapshot_digest = revision_snapshot_digest(&expected).unwrap();
    for _ in 0..2 {
        let retained = repo
            .retain_publication_source_revision(
                1,
                3,
                7,
                completed,
                &files,
                &AddonMediaListRevisionArtifacts::default(),
                Some(2),
            )
            .await
            .unwrap();
        assert_eq!(retained, expected);
        assert_eq!(
            repo.revision_snapshot(1, 3).await.unwrap(),
            Some(expected.clone())
        );
        assert_eq!(draft_state(&repo).await, draft);
    }
    assert_eq!(repo.revision_history(1).await.unwrap().len(), 2);
}

#[tokio::test]
async fn source_retention_rejects_unowned_inventory_and_missing_source() {
    let (repo, _) = fixture().await;
    let draft = draft_state(&repo).await;
    let artifacts = AddonMediaListRevisionArtifacts {
        ids: vec![999],
        ..Default::default()
    };
    assert!(repo
        .retain_publication_source_revision(1, 3, 7, completed_at(), &[], &artifacts, Some(2),)
        .await
        .is_err());
    assert!(repo
        .retain_publication_source_revision(
            1,
            3,
            7,
            completed_at(),
            &[],
            &AddonMediaListRevisionArtifacts::default(),
            Some(1),
        )
        .await
        .is_err());
    assert!(repo.revision_snapshot(1, 3).await.unwrap().is_none());
    assert_eq!(draft_state(&repo).await, draft);
}

fn fence() -> AddonMediaListRevisionRestoreFence {
    AddonMediaListRevisionRestoreFence {
        list_id: 1,
        source_revision: 2,
        target_draft_revision: 7,
        operation_id: 100,
        generation: 1,
        owner_token: "owner".into(),
        lease_owner: "worker".into(),
        active_at: completed_at(),
    }
}

#[tokio::test]
async fn saved_folder_materialization_is_fenced_noop_without_restore_marker() {
    let (repo, _) = fixture().await;
    let draft = draft_state(&repo).await;
    for _ in 0..2 {
        assert_eq!(
            repo.materialize_revision_as_draft(&fence()).await.unwrap(),
            7
        );
        assert_eq!(draft_state(&repo).await, draft);
    }
    let marker: Option<String> = sqlx::query_scalar(
        r#"SELECT "RestoreMaterializedAt" FROM "AddonMediaListPublicationJob" WHERE "Id"=100"#,
    )
    .fetch_one(&repo.0)
    .await
    .unwrap();
    assert!(marker.is_none());
    let mut stale = fence();
    stale.owner_token = "stale".into();
    assert!(repo.materialize_revision_as_draft(&stale).await.is_err());
    sqlx::query(r#"UPDATE "AddonMediaListPublicationEvent" SET "DedupeKey"='unrelated'"#)
        .execute(&repo.0)
        .await
        .unwrap();
    assert!(repo.materialize_revision_as_draft(&fence()).await.is_err());
    assert_eq!(draft_state(&repo).await, draft);
}

#[tokio::test]
async fn saved_folder_materialization_preserves_save_after_enqueue() {
    let (repo, _) = fixture().await;
    sqlx::query(
        r#"UPDATE "AddonMediaList" SET "DraftRevision"=8,
               "Name"='Saved after enqueue', "MetadataOverrideJson"='{"title":"Later save"}'
           WHERE "Id"=1"#,
    )
    .execute(&repo.0)
    .await
    .unwrap();
    let draft = draft_state(&repo).await;
    for _ in 0..2 {
        assert_eq!(
            repo.materialize_revision_as_draft(&fence()).await.unwrap(),
            8
        );
        assert_eq!(draft_state(&repo).await, draft);
    }
    let job: (i32, Option<String>) = sqlx::query_as(
        r#"SELECT "SavedRevision", "RestoreMaterializedAt"
           FROM "AddonMediaListPublicationJob" WHERE "Id"=100"#,
    )
    .fetch_one(&repo.0)
    .await
    .unwrap();
    assert_eq!(job, (7, None));

    sqlx::query(r#"UPDATE "AddonMediaListPublicationJob" SET "RestoreMaterializedAt"='2026-09-13T12:00:00Z' WHERE "Id"=100"#)
        .execute(&repo.0).await.unwrap();
    assert!(repo.materialize_revision_as_draft(&fence()).await.is_err());
    sqlx::query(
        r#"UPDATE "AddonMediaListPublicationJob" SET "RestoreMaterializedAt"=NULL WHERE "Id"=100"#,
    )
    .execute(&repo.0)
    .await
    .unwrap();
    sqlx::query(r#"UPDATE "AddonMediaListPublicationEvent" SET "EnqueueTrigger"=1 WHERE "PublicationJobId"=100"#)
        .execute(&repo.0).await.unwrap();
    assert!(repo.materialize_revision_as_draft(&fence()).await.is_err());
    assert_eq!(draft_state(&repo).await, draft);
}
