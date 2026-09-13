use super::*;

async fn option_save(
    pool: &DbPool,
    list_id: i64,
) -> ersatzrs_core::repositories::AddonMediaListDraftOptions {
    use ersatzrs_core::repositories::{AddonMediaListOptionSchema, AddonMediaListOptionSchemaRef};
    let capability: ersatzrs_addon_contract::AddonCapability = serde_json::from_value(serde_json::json!({
        "id":"media-list.options.v1", "options":[{
            "key":"enabled", "label":{"en-US":"Enabled"}, "description":{"en-US":"Fixture option"},
            "scope":"both", "operation_targets":["playback"], "applicability":"all",
            "kind":"boolean", "default":true
        }]
    })).unwrap();
    let manifest: String =
        sqlx::query_scalar(r#"SELECT "ManifestJson" FROM "AddonInstallation" WHERE "AddonId"=?"#)
            .bind(ADDON)
            .fetch_one(pool)
            .await
            .unwrap();
    let mut manifest: serde_json::Value = serde_json::from_str(&manifest).unwrap();
    manifest["manifest_version"] = serde_json::json!(5);
    manifest["capabilities"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::to_value(capability).unwrap());
    let installed: ersatzrs_addon_contract::AddonManifest =
        serde_json::from_value(manifest.clone()).unwrap();
    let definition_json = installed
        .media_list_option_schema_snapshot()
        .unwrap()
        .unwrap();
    sqlx::query(r#"UPDATE "AddonInstallation" SET "ManifestJson"=? WHERE "AddonId"=?"#)
        .bind(manifest.to_string())
        .bind(ADDON)
        .execute(pool)
        .await
        .unwrap();
    let schema = AddonMediaListOptionSchemaRef {
        addon_id: ADDON.into(),
        capability_id: "media-list.options.v1".into(),
        schema_digest: saved_sha256(definition_json.as_bytes()),
    };
    let repo = ersatzrs_infra::repositories::SqliteAddonMediaListRepository(pool.clone());
    repo.register_option_schema(&AddonMediaListOptionSchema {
        schema: schema.clone(),
        definition_json,
        created_at: Utc::now(),
    })
    .await
    .unwrap();
    let mut options = repo.draft_options(list_id, None).await.unwrap().unwrap();
    assert!(options.schema.is_none());
    assert!(options.list_values.is_empty());
    options.schema = Some(schema);
    options.list_values.insert(
        "enabled".into(),
        ersatzrs_domain::AddonMediaListOptionValue::Boolean(false),
    );
    options
}

#[tokio::test]
async fn saved_writer_direct_options_preserve_scheduled_scan_lease_and_queue_publication() {
    use ersatzrs_core::repositories::ManagedLibraryScanSpec;
    use ersatzrs_domain::{ManagedLibraryOperationKind, MediaSourceKind};
    let (_receiver, pool, copy, initial) = adopted_job().await;
    publish_received(&pool, &initial).await;
    let list_id = initial.job.list_id;
    let options = option_save(&pool, list_id).await;
    let repos = ersatzrs_infra::repositories::make_repositories(pool.clone());
    let ops = &repos.managed_library_operations;
    let scan = ops
        .enqueue_scan(
            ManagedLibraryOperationKind::ScheduledScan,
            initial.operation.library_id,
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
    let scan_claim = ops
        .claim_operation(scan.id, Utc::now())
        .await
        .unwrap()
        .unwrap();
    let lease_before: (i64, String, String) = sqlx::query_as(
        r#"SELECT "OperationId", "OwnerToken", "LeaseOwner" FROM "ManagedLibraryLease" WHERE "LibraryId"=?"#,
    ).bind(initial.operation.library_id).fetch_one(&pool).await.unwrap();
    let original: SavedListDocument =
        serde_json::from_slice(&std::fs::read(&copy).unwrap()).unwrap();
    let revision = repos
        .addon_media_lists
        .save_draft_options(&options)
        .await
        .unwrap();
    assert_eq!(revision, options.draft_revision + 1);
    let saved: SavedListDocument = serde_json::from_slice(&std::fs::read(&copy).unwrap()).unwrap();
    assert_eq!(saved.draft.options.list_values, options.list_values);
    assert_eq!(saved.published, original.published);
    let current: i32 =
        sqlx::query_scalar(r#"SELECT "PublishedRevision" FROM "AddonMediaList" WHERE "Id"=?"#)
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let publication = ops
        .enqueue_publication(list_id, initial.operation.library_id, revision, current + 1)
        .await
        .unwrap();
    assert!(ops
        .claim_operation(publication.operation.id, Utc::now())
        .await
        .unwrap()
        .is_none());
    let lease_after: (i64, String, String) = sqlx::query_as(
        r#"SELECT "OperationId", "OwnerToken", "LeaseOwner" FROM "ManagedLibraryLease" WHERE "LibraryId"=?"#,
    ).bind(initial.operation.library_id).fetch_one(&pool).await.unwrap();
    assert_eq!(lease_after, lease_before);
    assert!(ops.heartbeat(&scan_claim, Utc::now()).await.unwrap());
    assert!(ops.complete_operation(&scan_claim).await.unwrap());
    // Advance the supplied eligibility clock, not wall time, past the queued retry delay.
    let publication_claim = ops
        .claim_operation(
            publication.operation.id,
            Utc::now() + chrono::Duration::minutes(2),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(publication_claim.operation.id, publication.operation.id);
    assert_eq!(
        publication_claim.operation.owner_token,
        publication.operation.owner_token
    );
    let exact = ops
        .publication_job(publication.operation.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(exact.saved_revision, revision);
    assert_eq!(exact.generation, publication.job.generation);
}

#[tokio::test]
async fn saved_writer_direct_options_apply_and_undo_preserve_publication() {
    let (_receiver, pool, copy, queued) = adopted_job().await;
    let list_id = queued.job.list_id;
    let repo = ersatzrs_infra::repositories::SqliteAddonMediaListRepository(pool.clone());
    let original: SavedListDocument =
        serde_json::from_slice(&std::fs::read(&copy).unwrap()).unwrap();
    let options = option_save(&pool, list_id).await;
    let applied_revision = repo.save_draft_options(&options).await.unwrap();
    assert_eq!(applied_revision, options.draft_revision + 1);
    let applied_bytes = std::fs::read(&copy).unwrap();
    let applied: SavedListDocument = serde_json::from_slice(&applied_bytes).unwrap();
    assert!(applied.draft.options.schema.is_some());
    assert_eq!(applied.draft.options.list_values, options.list_values);
    assert_eq!(applied.published, original.published);
    assert!(repo.save_draft_options(&options).await.is_err());
    assert_eq!(std::fs::read(&copy).unwrap(), applied_bytes);
    let mut undo = repo.draft_options(list_id, None).await.unwrap().unwrap();
    undo.schema = None;
    undo.list_values.clear();
    undo.item_values.clear();
    assert_eq!(
        repo.save_draft_options(&undo).await.unwrap(),
        applied_revision + 1
    );
    let restored: SavedListDocument =
        serde_json::from_slice(&std::fs::read(&copy).unwrap()).unwrap();
    assert_eq!(restored.draft.options, original.draft.options);
    assert_eq!(restored.published, original.published);
}

#[tokio::test]
async fn saved_writer_direct_options_failure_recovers_before_stale_undo() {
    let (_receiver, pool, copy, queued) = adopted_job().await;
    let list_id = queued.job.list_id;
    let options = option_save(&pool, list_id).await;
    let repo = ersatzrs_infra::repositories::SqliteAddonMediaListRepository(pool.clone());
    let before = repo.draft_options(list_id, None).await.unwrap().unwrap();
    let original: SavedListDocument =
        serde_json::from_slice(&std::fs::read(&copy).unwrap()).unwrap();
    let state = database_state(&pool, list_id).await;
    sqlx::raw_sql(
        r#"CREATE TRIGGER "FixtureOptionsDigestFailure"
        BEFORE UPDATE OF "SavedDocumentDigest" ON "AddonMediaList"
        BEGIN SELECT RAISE(ABORT, 'fixture options projection failure'); END;"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    let error = repo
        .save_draft_options(&options)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("database refresh is pending"), "{error}");
    assert_eq!(
        repo.draft_options(list_id, None).await.unwrap().unwrap(),
        before
    );
    assert_eq!(database_state(&pool, list_id).await, state);
    let ahead = std::fs::read(&copy).unwrap();
    let saved: SavedListDocument = serde_json::from_slice(&ahead).unwrap();
    assert_eq!(saved.draft.options.list_values, options.list_values);
    assert_eq!(saved.published, original.published);
    sqlx::query(r#"DROP TRIGGER "FixtureOptionsDigestFailure""#)
        .execute(&pool)
        .await
        .unwrap();
    let error = repo
        .save_draft_options(&before)
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("database was refreshed from the saved list folder"),
        "{error}"
    );
    assert_eq!(std::fs::read(&copy).unwrap(), ahead);
    let mut recovered = repo.draft_options(list_id, None).await.unwrap().unwrap();
    assert_eq!(recovered.schema, options.schema);
    assert_eq!(recovered.list_values, options.list_values);
    assert_eq!(recovered.draft_revision, before.draft_revision + 1);
    recovered.schema = None;
    recovered.list_values.clear();
    recovered.item_values.clear();
    assert_eq!(
        repo.save_draft_options(&recovered).await.unwrap(),
        before.draft_revision + 2
    );
    let restored: SavedListDocument =
        serde_json::from_slice(&std::fs::read(copy).unwrap()).unwrap();
    assert_eq!(restored.draft.options, original.draft.options);
    assert_eq!(restored.published, original.published);
}

fn saved_sha256(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    use std::fmt::Write as _;
    let mut hex = String::with_capacity(64);
    for byte in sha2::Sha256::digest(bytes) {
        write!(hex, "{byte:02x}").unwrap();
    }
    hex
}

#[tokio::test]
async fn saved_writer_completion_retries_failure_before_file_replacement() {
    Box::pin(completion_failure(false)).await;
}

#[tokio::test]
async fn saved_writer_completion_retries_failure_after_file_replacement() {
    Box::pin(completion_failure(true)).await;
}

async fn ordinary_job() -> (
    tempfile::TempDir,
    DbPool,
    PathBuf,
    ersatzrs_core::repositories::AddonMediaListPublicationEnqueue,
) {
    let (receiver, pool, copy, initial) = adopted_job().await;
    publish_received(&pool, &initial).await;
    let list_id = initial.job.list_id;
    let repos = ersatzrs_infra::repositories::make_repositories(pool.clone());
    let repo = &repos.addon_media_lists;
    let ops = &repos.managed_library_operations;
    let mut form = repo.get_show_nfo(list_id).await.unwrap();
    form.metadata.title = "Ordinary publication metadata".into();
    let before = database_state(&pool, list_id).await;
    let saved_revision = repo
        .save_editor_task(&AddonMediaListEditorSave {
            metadata: Some(ersatzrs_core::repositories::AddonMediaListEditorMetadata {
                metadata: form.metadata,
                etag: form.etag,
            }),
            disk_warning_gb: None,
            ..warning_save(list_id, before.2)
        })
        .await
        .unwrap();
    let current: i32 =
        sqlx::query_scalar(r#"SELECT "PublishedRevision" FROM "AddonMediaList" WHERE "Id"=?"#)
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let queued = ops
        .enqueue_publication(
            list_id,
            initial.operation.library_id,
            saved_revision,
            current + 1,
        )
        .await
        .unwrap();
    assert!(queued.job.source_published_revision.is_none());
    (receiver, pool, copy, queued)
}

#[expect(
    clippy::too_many_lines,
    reason = "Keep failure, recovery and unchanged-publication assertions in operation order"
)]
async fn completion_failure(after_write: bool) {
    let (_receiver, pool, copy, queued) = ordinary_job().await;
    let list_id = queued.job.list_id;
    let repos = ersatzrs_infra::repositories::make_repositories(pool.clone());
    let repo = &repos.addon_media_lists;
    let ops = &repos.managed_library_operations;
    let file_before = std::fs::read(&copy).unwrap();
    let document_before: SavedListDocument = serde_json::from_slice(&file_before).unwrap();
    let retained_episode: i64 = sqlx::query_scalar(
        r#"SELECT "projection"."EpisodeId" FROM "AddonMediaListItemEpisode" AS "projection"
           JOIN "AddonMediaListItem" AS "item" ON "item"."Id"="projection"."AddonMediaListItemId"
           WHERE "item"."AddonMediaListId"=? AND "item"."ProviderId"='published-item'"#,
    )
    .bind(list_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let claim = publish_through_cleanup(&pool, &queued).await;
    let job = &queued.job;
    let committed_revision: i32 =
        sqlx::query_scalar(r#"SELECT "PublishedRevision" FROM "AddonMediaList" WHERE "Id"=?"#)
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(committed_revision, job.target_published_revision);
    let trigger = if after_write {
        r#"CREATE TRIGGER "FixtureCompletionFailure"
           BEFORE UPDATE OF "SavedDocumentDigest" ON "AddonMediaList"
           BEGIN SELECT RAISE(ABORT, 'fixture completion digest failure'); END;"#
    } else {
        r#"CREATE TRIGGER "FixtureCompletionFailure"
           BEFORE UPDATE OF "PublicationJournalJson" ON "AddonMediaList"
           WHEN OLD."PublicationJournalJson" IS NOT NULL AND NEW."PublicationJournalJson" IS NULL
           BEGIN SELECT RAISE(ABORT, 'fixture completion journal failure'); END;"#
    };
    sqlx::raw_sql(trigger).execute(&pool).await.unwrap();
    let failure = repo
        .finalize_staged_publication_revision(
            list_id,
            Outcome::Published,
            job.operation_id,
            job.generation,
            &claim.operation.owner_token,
            &claim.lease_owner,
        )
        .await
        .unwrap_err();
    if after_write {
        assert!(
            failure.to_string().contains("database refresh is pending"),
            "{failure}"
        );
    }
    let retained_journal: Option<String> =
        sqlx::query_scalar(r#"SELECT "PublicationJournalJson" FROM "AddonMediaList" WHERE "Id"=?"#)
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(retained_journal.is_some());
    let file_after_failure = std::fs::read(&copy).unwrap();
    if after_write {
        assert_ne!(file_after_failure, file_before);
    } else {
        assert_eq!(file_after_failure, file_before);
    }
    let document: SavedListDocument = serde_json::from_slice(&file_after_failure).unwrap();
    assert_eq!(document.draft, document_before.draft);
    if after_write {
        let state = database_state(&pool, list_id).await;
        let jobs = publication_job_count(&pool, list_id).await;
        let discovery =
            ersatzrs_infra::addon_media_list_saved::admission::discover_copied_lists(&pool)
                .await
                .unwrap();
        assert_eq!(discovery.deferred, 1);
        assert_eq!(discovery.refused, 0);
        assert_eq!(discovery.repaired, 0);
        assert!(discovery.admitted.is_empty());
        assert_eq!(database_state(&pool, list_id).await, state);
        assert_eq!(publication_job_count(&pool, list_id).await, jobs);
        assert_eq!(std::fs::read(&copy).unwrap(), file_after_failure);
        let journal: Option<String> = sqlx::query_scalar(
            r#"SELECT "PublicationJournalJson" FROM "AddonMediaList" WHERE "Id"=?"#,
        )
        .bind(list_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(journal, retained_journal);
    }
    sqlx::query(r#"DROP TRIGGER "FixtureCompletionFailure""#)
        .execute(&pool)
        .await
        .unwrap();
    repo.finalize_staged_publication_revision(
        list_id,
        Outcome::Published,
        job.operation_id,
        job.generation,
        &claim.operation.owner_token,
        &claim.lease_owner,
    )
    .await
    .unwrap();
    assert!(ops
        .complete_publication(&claim, Outcome::Published)
        .await
        .unwrap());
    let final_bytes = std::fs::read(&copy).unwrap();
    if after_write {
        assert_eq!(final_bytes, file_after_failure);
    }
    let final_document: SavedListDocument = serde_json::from_slice(&final_bytes).unwrap();
    assert_eq!(final_document.draft, document_before.draft);
    assert_ne!(final_document.published, document_before.published);
    let state: (i32, Option<String>, String, String) = sqlx::query_as(
        r#"SELECT "PublishedRevision", "PublicationJournalJson", "SavedDocumentDigest", "SavedPublicationDigest"
           FROM "AddonMediaList" WHERE "Id"=?"#,
    ).bind(list_id).fetch_one(&pool).await.unwrap();
    assert_eq!(state.0, committed_revision);
    assert!(state.1.is_none());
    let retained_after: (i64, bool) = sqlx::query_as(
        r#"SELECT "projection"."EpisodeId", "item"."Published" FROM "AddonMediaListItemEpisode" AS "projection"
           JOIN "AddonMediaListItem" AS "item" ON "item"."Id"="projection"."AddonMediaListItemId"
           WHERE "item"."AddonMediaListId"=? AND "item"."ProviderId"='published-item'"#,
    ).bind(list_id).fetch_one(&pool).await.unwrap();
    assert_eq!(retained_after, (retained_episode, false));
    let playlist: Option<i64> = sqlx::query_scalar(
        r#"SELECT "PlaylistId" FROM "AddonMediaList" WHERE "Id"=? AND "GeneratePlaylist"=1"#,
    )
    .bind(list_id)
    .fetch_optional(&pool)
    .await
    .unwrap()
    .flatten();
    if let Some(playlist_id) = playlist {
        let members: Vec<String> = sqlx::query_scalar(
            r#"SELECT "item"."ProviderId" FROM "PlaylistItem" AS "playlist_item"
               JOIN "AddonMediaListItemEpisode" AS "projection" ON "projection"."EpisodeId"="playlist_item"."MediaItemId"
               JOIN "AddonMediaListItem" AS "item" ON "item"."Id"="projection"."AddonMediaListItemId"
               WHERE "playlist_item"."PlaylistId"=? ORDER BY "playlist_item"."Index""#,
        ).bind(playlist_id).fetch_all(&pool).await.unwrap();
        assert_eq!(members, vec!["draft-only"]);
    }
    assert_eq!(state.2, saved_sha256(&final_bytes));
    let draft_state: (i32, i32) = sqlx::query_as(
        r#"SELECT "DraftRevision", "ReviewState" FROM "AddonMediaList" WHERE "Id"=?"#,
    )
    .bind(list_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(draft_state, (job.saved_revision, 2));
    assert_eq!(
        state.3,
        saved_sha256(&serde_json::to_vec(&final_document.published).unwrap())
    );
    let jobs = publication_job_count(&pool, list_id).await;
    let discovered =
        ersatzrs_infra::addon_media_list_saved::admission::discover_copied_lists(&pool)
            .await
            .unwrap();
    assert_eq!(discovered.refused, 0);
    assert_eq!(discovered.repaired, 0);
    assert!(discovered.admitted.is_empty());
    assert_eq!(publication_job_count(&pool, list_id).await, jobs);
    assert_eq!(
        database_state(&pool, list_id).await.1,
        saved_sha256(&final_bytes)
    );
    assert_eq!(std::fs::read(&copy).unwrap(), final_bytes);
    let after_discovery: (i32, i32) = sqlx::query_as(
        r#"SELECT "DraftRevision", "ReviewState" FROM "AddonMediaList" WHERE "Id"=?"#,
    )
    .bind(list_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(after_discovery, draft_state);
}

async fn adopted_list() -> (tempfile::TempDir, DbPool, PathBuf, i64) {
    let (receiver, pool, copy, queued) = adopted_job().await;
    (receiver, pool, copy, queued.job.list_id)
}

#[tokio::test]
async fn saved_writer_completion_preserves_external_draft_until_mapped_repair() {
    Box::pin(concurrent_completion_edit(false)).await;
}

#[tokio::test]
async fn saved_writer_completion_refuses_unrelated_external_publication() {
    Box::pin(concurrent_completion_edit(true)).await;
}

#[expect(
    clippy::too_many_lines,
    reason = "Keep concurrent file edit and publication reconciliation proof together"
)]
async fn concurrent_completion_edit(change_publication: bool) {
    let (_receiver, pool, copy, queued) = ordinary_job().await;
    let list_id = queued.job.list_id;
    let claim = publish_through_cleanup(&pool, &queued).await;
    let repos = ersatzrs_infra::repositories::make_repositories(pool.clone());
    let db_before = database_state(&pool, list_id).await;
    let draft_before = captured_draft(&pool, list_id).await;
    let mut document: SavedListDocument =
        serde_json::from_slice(&std::fs::read(&copy).unwrap()).unwrap();
    if change_publication {
        document.published.as_mut().unwrap().metadata.insert(
            "title".into(),
            serde_json::json!("Unrelated incoming publication"),
        );
    } else {
        document.draft.disk_warning_gb = 42;
        document.draft.presentation.name = "Draft saved during publication".into();
    }
    let edited = serde_json::to_vec(&document).unwrap();
    std::fs::write(&copy, &edited).unwrap();
    let result = repos
        .addon_media_lists
        .finalize_staged_publication_revision(
            list_id,
            Outcome::Published,
            queued.job.operation_id,
            queued.job.generation,
            &claim.operation.owner_token,
            &claim.lease_owner,
        )
        .await;
    if change_publication {
        let expected = "the list folder contains a different saved publication; resolve the folder before retrying";
        let error = result.unwrap_err().to_string();
        assert!(error.contains(expected), "{error}");
        let retry = repos
            .addon_media_lists
            .finalize_staged_publication_revision(
                list_id,
                Outcome::Published,
                queued.job.operation_id,
                queued.job.generation,
                &claim.operation.owner_token,
                &claim.lease_owner,
            )
            .await
            .unwrap_err()
            .to_string();
        assert!(retry.contains(expected), "{retry}");
        assert_eq!(std::fs::read(&copy).unwrap(), edited);
        assert_eq!(database_state(&pool, list_id).await, db_before);
        assert_eq!(captured_draft(&pool, list_id).await, draft_before);
        let journal: Option<String> = sqlx::query_scalar(
            r#"SELECT "PublicationJournalJson" FROM "AddonMediaList" WHERE "Id"=?"#,
        )
        .bind(list_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(journal.is_some());
        return;
    }
    result.unwrap();
    assert!(repos
        .managed_library_operations
        .complete_publication(&claim, Outcome::Published)
        .await
        .unwrap());
    let bytes = std::fs::read(&copy).unwrap();
    let completed: SavedListDocument = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(completed.draft, document.draft);
    assert_ne!(completed.published, document.published);
    assert_eq!(captured_draft(&pool, list_id).await, draft_before);
    assert_eq!(database_state(&pool, list_id).await, db_before);
    assert_ne!(db_before.1, saved_sha256(&bytes));
    let publication_digest: String =
        sqlx::query_scalar(r#"SELECT "SavedPublicationDigest" FROM "AddonMediaList" WHERE "Id"=?"#)
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        publication_digest,
        saved_sha256(&serde_json::to_vec(&completed.published).unwrap())
    );
    let jobs = publication_job_count(&pool, list_id).await;
    for expected_repairs in [1, 0] {
        let report =
            ersatzrs_infra::addon_media_list_saved::admission::discover_copied_lists(&pool)
                .await
                .unwrap();
        assert_eq!(report.refused, 0);
        assert_eq!(report.deferred, 0);
        assert_eq!(report.repaired, expected_repairs);
        assert!(report.admitted.is_empty());
        assert_eq!(publication_job_count(&pool, list_id).await, jobs);
        assert_eq!(std::fs::read(&copy).unwrap(), bytes);
    }
    let projected = captured_draft(&pool, list_id).await;
    assert_eq!(
        projected.presentation.name,
        document.draft.presentation.name
    );
    assert_eq!(projected.disk_warning_gb, 42);
    assert_eq!(projected.revision, draft_before.revision + 1);
    assert_eq!(database_state(&pool, list_id).await.1, saved_sha256(&bytes));
}

async fn adopted_job() -> (
    tempfile::TempDir,
    DbPool,
    PathBuf,
    ersatzrs_core::repositories::AddonMediaListPublicationEnqueue,
) {
    let source = tempfile::tempdir().unwrap();
    let (source_pool, installed, source_root) = profile(source.path()).await;
    let (saved_path, _) = source_document(&source_pool, &installed, &source_root, false).await;
    let receiver = tempfile::tempdir().unwrap();
    let (pool, _, root) = profile(receiver.path()).await;
    let folder = root.join("Copied list");
    std::fs::create_dir(&folder).unwrap();
    let copy = folder.join(SAVED_LIST_FILE_NAME);
    std::fs::copy(saved_path, &copy).unwrap();
    let mut discovered =
        ersatzrs_infra::addon_media_list_saved::admission::discover_copied_lists(&pool)
            .await
            .unwrap();
    assert_eq!(discovered.refused, 0);
    assert_eq!(discovered.admitted.len(), 1);
    (receiver, pool, copy, discovered.admitted.remove(0))
}

async fn database_state(pool: &DbPool, list_id: i64) -> (i64, String, i32) {
    sqlx::query_as(
        r#"SELECT "DiskUsageWarningGb", "SavedDocumentDigest", "DraftRevision"
           FROM "AddonMediaList" WHERE "Id"=?"#,
    )
    .bind(list_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

fn warning_save(list_id: i64, expected_revision: i32) -> AddonMediaListEditorSave {
    AddonMediaListEditorSave {
        list_id,
        expected_revision,
        metadata: None,
        settings: None,
        delivery_mode: None,
        disk_warning_gb: Some(42),
        options: None,
        items: Vec::new(),
    }
}

#[tokio::test]
async fn saved_writer_file_first_failure_repairs_database_before_next_edit() {
    let (_receiver, pool, copy, list_id) = adopted_list().await;
    let before = database_state(&pool, list_id).await;
    let original: SavedListDocument =
        serde_json::from_slice(&std::fs::read(&copy).unwrap()).unwrap();
    assert_ne!(original.draft.disk_warning_gb, 42);
    sqlx::raw_sql(
        r#"CREATE TRIGGER "FixtureRejectSavedDigest"
           BEFORE UPDATE OF "SavedDocumentDigest" ON "AddonMediaList"
           BEGIN SELECT RAISE(ABORT, 'fixture projection failure'); END;"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    let repo = ersatzrs_infra::repositories::SqliteAddonMediaListRepository(pool.clone());
    let save = warning_save(list_id, before.2);
    let error = repo.save_editor_task(&save).await.unwrap_err().to_string();
    assert!(error.contains("saved to the list folder"), "{error}");
    assert!(error.contains("database refresh is pending"), "{error}");
    assert_eq!(database_state(&pool, list_id).await, before);
    let saved_bytes = std::fs::read(&copy).unwrap();
    let saved: SavedListDocument = serde_json::from_slice(&saved_bytes).unwrap();
    assert_eq!(saved.draft.disk_warning_gb, 42);
    assert_eq!(saved.published, original.published);

    // The next edit first repairs SQLite, but cannot submit stale form values
    // over the recovered draft. A refreshed form can save normally afterward.
    sqlx::query(r#"DROP TRIGGER "FixtureRejectSavedDigest""#)
        .execute(&pool)
        .await
        .unwrap();
    let error = repo.save_editor_task(&save).await.unwrap_err().to_string();
    assert!(
        error.contains("database was refreshed from the saved list folder"),
        "{error}"
    );
    assert_eq!(std::fs::read(&copy).unwrap(), saved_bytes);
    let repaired = database_state(&pool, list_id).await;
    assert_eq!(repaired.0, 42);
    assert_ne!(repaired.1, before.1);
    assert_eq!(repaired.2, before.2 + 1);
    let repeated = ersatzrs_infra::addon_media_list_saved::admission::discover_copied_lists(&pool)
        .await
        .unwrap();
    assert_eq!(repeated.refused, 0);
    assert_eq!(repeated.repaired, 0);
    assert!(repeated.admitted.is_empty());
    assert_eq!(database_state(&pool, list_id).await, repaired);
    assert_eq!(
        repo.save_editor_task(&warning_save(list_id, repaired.2))
            .await
            .unwrap(),
        repaired.2
    );
    let jobs: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "AddonMediaListPublicationJob" WHERE "AddonMediaListId"=?"#,
    )
    .bind(list_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        jobs, 1,
        "draft-only repair must not enqueue a second publication"
    );
}

#[tokio::test]
async fn saved_writer_warning_only_save_updates_file_and_digest_without_revision_advance() {
    let (_receiver, pool, copy, list_id) = adopted_list().await;
    let before = database_state(&pool, list_id).await;
    let original: SavedListDocument =
        serde_json::from_slice(&std::fs::read(&copy).unwrap()).unwrap();
    let repo = ersatzrs_infra::repositories::SqliteAddonMediaListRepository(pool.clone());
    assert_eq!(
        repo.save_editor_task(&warning_save(list_id, before.2))
            .await
            .unwrap(),
        before.2
    );
    let after = database_state(&pool, list_id).await;
    assert_eq!(after.0, 42);
    assert_ne!(after.1, before.1);
    assert_eq!(after.2, before.2);
    let saved: SavedListDocument = serde_json::from_slice(&std::fs::read(copy).unwrap()).unwrap();
    assert_eq!(saved.draft.disk_warning_gb, 42);
    assert_eq!(saved.published, original.published);
}

async fn publication_job_count(pool: &DbPool, list_id: i64) -> i64 {
    sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "AddonMediaListPublicationJob" WHERE "AddonMediaListId"=?"#,
    )
    .bind(list_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn saved_writer_mapped_repair_defers_until_live_publication_finishes() {
    let (_receiver, pool, copy, queued) = adopted_job().await;
    let claim = publish_through_cleanup(&pool, &queued).await;
    let list_id = queued.job.list_id;
    let state = database_state(&pool, list_id).await;
    let jobs = publication_job_count(&pool, list_id).await;
    let journal: Option<String> =
        sqlx::query_scalar(r#"SELECT "PublicationJournalJson" FROM "AddonMediaList" WHERE "Id"=?"#)
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(journal.is_some());
    let leased: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM "ManagedLibraryLease"
           WHERE "OperationId"=? AND "OwnerToken"=? AND "LeaseOwner"=? AND "LeaseExpiresAt">?)"#,
    )
    .bind(queued.job.operation_id)
    .bind(&claim.operation.owner_token)
    .bind(&claim.lease_owner)
    .bind(Utc::now().format("%Y-%m-%d %H:%M:%S%.f").to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(leased);
    let mut document: SavedListDocument =
        serde_json::from_slice(&std::fs::read(&copy).unwrap()).unwrap();
    document.draft.disk_warning_gb = 42;
    let changed = serde_json::to_vec(&document).unwrap();
    std::fs::write(&copy, &changed).unwrap();
    let report = ersatzrs_infra::addon_media_list_saved::admission::discover_copied_lists(&pool)
        .await
        .unwrap();
    assert_eq!(report.deferred, 1);
    assert_eq!(report.refused, 0);
    assert_eq!(report.repaired, 0);
    assert!(report.admitted.is_empty());
    assert_eq!(database_state(&pool, list_id).await, state);
    assert_eq!(publication_job_count(&pool, list_id).await, jobs);
    assert_eq!(std::fs::read(&copy).unwrap(), changed);
    let journal_after: Option<String> =
        sqlx::query_scalar(r#"SELECT "PublicationJournalJson" FROM "AddonMediaList" WHERE "Id"=?"#)
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(journal_after, journal);
    let repos = ersatzrs_infra::repositories::make_repositories(pool.clone());
    repos
        .addon_media_lists
        .finalize_staged_publication_revision(
            list_id,
            Outcome::Published,
            queued.job.operation_id,
            queued.job.generation,
            &claim.operation.owner_token,
            &claim.lease_owner,
        )
        .await
        .unwrap();
    assert!(repos
        .managed_library_operations
        .complete_publication(&claim, Outcome::Published)
        .await
        .unwrap());
    let repaired = ersatzrs_infra::addon_media_list_saved::admission::discover_copied_lists(&pool)
        .await
        .unwrap();
    assert_eq!(repaired.deferred, 0);
    assert_eq!(repaired.refused, 0);
    assert_eq!(repaired.repaired, 1);
    assert!(repaired.admitted.is_empty());
    assert_eq!(database_state(&pool, list_id).await.0, 42);
    assert_eq!(publication_job_count(&pool, list_id).await, jobs);
    assert_eq!(std::fs::read(&copy).unwrap(), changed);
}

async fn captured_draft(
    pool: &DbPool,
    list_id: i64,
) -> ersatzrs_infra::addon_media_list_saved::model::SavedListDraft {
    let mut tx = pool.begin().await.unwrap();
    let draft = capture::draft_in_tx(&mut tx, list_id).await.unwrap().draft;
    tx.commit().await.unwrap();
    draft
}

#[tokio::test]
async fn saved_writer_mapped_draft_repair_preserves_active_publication_and_hidden_state() {
    let (_receiver, pool, copy, queued) = adopted_job().await;
    publish_received(&pool, &queued).await;
    let list_id = queued.job.list_id;
    let repo = ersatzrs_infra::repositories::SqliteAddonMediaListRepository(pool.clone());
    let active = repo
        .active_publication_snapshot(list_id)
        .await
        .unwrap()
        .unwrap();
    let jobs = publication_job_count(&pool, list_id).await;
    repo.set_admin_hidden(list_id, true).await.unwrap();
    let mut document: SavedListDocument =
        serde_json::from_slice(&std::fs::read(&copy).unwrap()).unwrap();
    let published = document.published.clone();
    document.draft.presentation.name = "Externally saved draft".into();
    document.draft.disk_warning_gb = 42;
    std::fs::write(&copy, serde_json::to_vec(&document).unwrap()).unwrap();
    let report = ersatzrs_infra::addon_media_list_saved::admission::discover_copied_lists(&pool)
        .await
        .unwrap();
    assert_eq!(report.refused, 0);
    assert_eq!(report.repaired, 1);
    assert!(report.admitted.is_empty());
    assert_eq!(publication_job_count(&pool, list_id).await, jobs);
    assert_eq!(
        repo.active_publication_snapshot(list_id).await.unwrap(),
        Some(active)
    );
    let draft = captured_draft(&pool, list_id).await;
    assert_eq!(draft.presentation.name, "Externally saved draft");
    assert_eq!(draft.disk_warning_gb, 42);
    let hidden: bool =
        sqlx::query_scalar(r#"SELECT "AdminHidden" FROM "AddonMediaList" WHERE "Id"=?"#)
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(hidden);
    let saved: SavedListDocument = serde_json::from_slice(&std::fs::read(&copy).unwrap()).unwrap();
    assert_eq!(saved.published, published);
    let repeat = ersatzrs_infra::addon_media_list_saved::admission::discover_copied_lists(&pool)
        .await
        .unwrap();
    assert_eq!(repeat.refused, 0);
    assert_eq!(repeat.repaired, 0);
    assert!(repeat.admitted.is_empty());
    assert_eq!(captured_draft(&pool, list_id).await, draft);
}

#[tokio::test]
async fn saved_writer_mapped_publication_change_queues_once_and_preserves_draft() {
    let (_receiver, pool, copy, queued) = adopted_job().await;
    publish_received(&pool, &queued).await;
    let list_id = queued.job.list_id;
    let repo = ersatzrs_infra::repositories::SqliteAddonMediaListRepository(pool.clone());
    let active = repo
        .active_publication_snapshot(list_id)
        .await
        .unwrap()
        .unwrap();
    let jobs = publication_job_count(&pool, list_id).await;
    let draft_before = captured_draft(&pool, list_id).await;
    let mut document: SavedListDocument =
        serde_json::from_slice(&std::fs::read(&copy).unwrap()).unwrap();
    let saved_draft = document.draft.clone();
    let item = &mut document.published.as_mut().unwrap().items[0];
    let provider_id = item.content.provider_id.clone();
    item.content.title = "New independent publication".into();
    item.metadata.insert(
        "title".into(),
        serde_json::json!("New independent publication"),
    );
    std::fs::write(&copy, serde_json::to_vec(&document).unwrap()).unwrap();
    let mut report =
        ersatzrs_infra::addon_media_list_saved::admission::discover_copied_lists(&pool)
            .await
            .unwrap();
    assert_eq!(report.refused, 0);
    assert_eq!(report.repaired, 1);
    assert_eq!(report.admitted.len(), 1);
    assert_eq!(publication_job_count(&pool, list_id).await, jobs + 1);
    let next = report.admitted.remove(0);
    let saved_folder: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM "AddonMediaListPublicationEvent"
           WHERE "PublicationJobId"=? AND "DedupeKey"='enqueued' AND "Kind"=1 AND "EnqueueTrigger"=3)"#,
    ).bind(next.job.id).fetch_one(&pool).await.unwrap();
    assert!(saved_folder);
    let draft_queued = captured_draft(&pool, list_id).await;
    // Projection allocates its own local edit revision; compare content, not sender counters.
    let mut expected_draft = draft_before;
    expected_draft.revision = draft_queued.revision;
    assert_eq!(draft_queued, expected_draft);
    publish_received(&pool, &next).await;
    let updated = repo
        .active_publication_snapshot(list_id)
        .await
        .unwrap()
        .unwrap();
    assert!(updated.revision > active.revision);
    let item = updated
        .items
        .iter()
        .find(|item| item.provider_id == provider_id)
        .unwrap();
    assert_eq!(item.title, "New independent publication");
    let metadata: serde_json::Value = serde_json::from_str(&item.metadata_json).unwrap();
    assert_eq!(metadata["title"], "New independent publication");
    assert_eq!(captured_draft(&pool, list_id).await, draft_queued);
    let saved: SavedListDocument = serde_json::from_slice(&std::fs::read(&copy).unwrap()).unwrap();
    assert_eq!(saved.draft, saved_draft);
    let repeat = ersatzrs_infra::addon_media_list_saved::admission::discover_copied_lists(&pool)
        .await
        .unwrap();
    assert_eq!(repeat.refused, 0);
    assert_eq!(repeat.repaired, 0);
    assert!(repeat.admitted.is_empty());
    assert_eq!(publication_job_count(&pool, list_id).await, jobs + 1);
}
