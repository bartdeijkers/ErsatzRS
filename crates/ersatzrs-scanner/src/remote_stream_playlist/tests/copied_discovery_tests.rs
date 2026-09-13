use super::*;
use ersatzrs_core::repositories::{
    AddonMediaListEditorItem, AddonMediaListEditorSave, AddonMediaListItemReviewEdit,
    AddonMediaListLifecycleRepository as _, AddonMediaListRepository as _,
    AddonMediaListRevisionArtifacts, AddonMediaListRevisionRestoreFence,
    ManagedLibraryOperationClaim, ManagedLibraryOperationRepository as _,
};
use ersatzrs_domain::{
    AddonMediaListPublicationOutcome as Outcome, AddonMediaListPublicationPhase as Phase,
};
use ersatzrs_infra::addon_media_list_saved::{
    assembly::{assemble_initial, lock_managed_tree, AssemblyContext},
    capture, document_io,
    model::{SavedListDocument, SAVED_LIST_FILE_NAME},
    published,
};

const ADDON: &str = "org.ersatzrs.addon.fixture";

async fn profile(directory: &Path) -> (DbPool, ersatzrs_addon_contract::AddonManifest, PathBuf) {
    let package = directory.join("package");
    std::fs::create_dir_all(&package).unwrap();
    let script = package.join("fixture.sh");
    write_fixture_playlist_script(&script, "https://example.test/must-not-enumerate", false);
    std::fs::write(&script, "#!/bin/sh\nexit 1\n").unwrap();
    let pool = fixture_addon_pool(&package).await;
    let raw: String = sqlx::query_scalar(r#"SELECT "ManifestJson" FROM "AddonInstallation""#)
        .fetch_one(&pool)
        .await
        .unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&raw).unwrap();
    value["manifest_version"] = serde_json::json!(3);
    value["capabilities"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"id":"media-list.list.v1"}));
    value["settings"] = serde_json::json!([{"key":"MEDIA_STORAGE_PATH","label":{"en-US":"Storage"},"kind":"path","required":false}]);
    value["media_list_storage"] = serde_json::json!({"path_setting":"MEDIA_STORAGE_PATH","default_subdirectory":"fixture_media","source_name":"Fixture"});
    let installed: ersatzrs_addon_contract::AddonManifest =
        serde_json::from_value(value.clone()).unwrap();
    sqlx::query(r#"UPDATE "AddonInstallation" SET "ManifestJson"=?"#)
        .bind(value.to_string())
        .execute(&pool)
        .await
        .unwrap();
    let root = directory.join("media");
    std::fs::create_dir_all(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    let mut tx = pool.begin().await.unwrap();
    ersatzrs_infra::addon_media_storage::ensure_media_source_in_transaction(
        &mut tx,
        ADDON,
        installed.media_list_storage.as_ref().unwrap(),
        &root,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    (pool, installed, root)
}

async fn source_document(
    pool: &DbPool,
    installed: &ersatzrs_addon_contract::AddonManifest,
    root: &Path,
    whole_folder: bool,
) -> (PathBuf, SavedListDocument) {
    sqlx::raw_sql(r#"INSERT INTO "AddonMediaList" ("Id","AddonId","ProviderId","SourceUrl","Name","DraftRevision","PublishedRevision","RequestedPresentationKind","PresentationKind","ProviderMetadataJson")
        VALUES (10,'org.ersatzrs.addon.fixture','fixture/list','https://example.test/playlist','Published list',1,1,1,1,'{"title":"Published list"}');
        INSERT INTO "AddonMediaListItem" ("Id","AddonMediaListId","ProviderId","DisplayTitle","Title","Kind","Rank","SourceUrl","DurationSeconds","Published","ProviderLiveness","EffectiveLiveness","PublishedLiveness","ProviderMetadataJson")
        VALUES (20,10,'published-item','Published item','Published item',4,0,'https://example.test/published',60,1,1,1,1,'{"title":"Published item"}');"#)
        .execute(pool).await.unwrap();
    let repo = ersatzrs_infra::repositories::SqliteAddonMediaListRepository(pool.clone());
    repo.retain_published_revision(
        10,
        1,
        1,
        Utc::now(),
        &[],
        &AddonMediaListRevisionArtifacts::default(),
    )
    .await
    .unwrap();
    let folder = root.join("Copied list");
    if whole_folder {
        let library_id = ersatzrs_infra::addon_media_storage::ensure_list_library(
            pool,
            10,
            ADDON,
            "Published list",
            "https://example.test/playlist",
            "",
            Some("Copied list"),
        )
        .await
        .unwrap();
        let report = crate::scan_local_library(
            pool,
            &crate::LocalScanConfig {
                library_id,
                ffprobe_path: "unused-for-remote-definitions".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(report.failed_files, 0, "{:?}", report.issues);
        assert_eq!(report.scanned_files, 1);
        assert!(folder.join("streams/.ersatzrs-playlist.json").is_file());
    } else {
        std::fs::create_dir(&folder).unwrap();
    }
    sqlx::raw_sql(r#"UPDATE "AddonMediaList" SET "Name"='New draft list',"DraftRevision"=2,"MetadataOverrideJson"='{"title":"New draft list"}' WHERE "Id"=10;
        UPDATE "AddonMediaListItem" SET "Included"=0,"MetadataOverrideJson"='{"title":"New draft item"}' WHERE "Id"=20;
        INSERT INTO "AddonMediaListItem" ("Id","AddonMediaListId","ProviderId","DisplayTitle","Title","Kind","Rank","SourceUrl","DurationSeconds","Published","ProviderLiveness","EffectiveLiveness","ProviderMetadataJson")
        VALUES (21,10,'draft-only','Draft only','Draft only',4,1,'https://example.test/draft',90,0,1,1,'{"title":"Draft only"}');"#)
        .execute(pool).await.unwrap();
    if whole_folder {
        let chapters = "00:00 Opening\n00:30 New draft chapter";
        sqlx::query(r#"UPDATE "AddonMediaList" SET "ChapterInput"=? WHERE "Id"=10"#)
            .bind(chapters)
            .execute(pool)
            .await
            .unwrap();
        ersatzrs_infra::addon_media_storage::ensure_list_library(
            pool,
            10,
            ADDON,
            "New draft list",
            "https://example.test/playlist",
            chapters,
            Some("Copied list"),
        )
        .await
        .unwrap();
    }
    let tree = lock_managed_tree(&folder).unwrap();
    let mut tx = pool.begin().await.unwrap();
    let draft = capture::draft_in_tx(&mut tx, 10).await.unwrap();
    let publication = published::in_tx(&mut tx, 10).await.unwrap();
    tx.commit().await.unwrap();
    let document = assemble_initial(
        pool,
        &AssemblyContext {
            list_id: 10,
            tree,
            storage_parent: root,
            storage_owner: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            artifact_root: root,
            installed,
        },
        draft,
        publication,
    )
    .await
    .unwrap();
    let path = folder.join(SAVED_LIST_FILE_NAME);
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
        .unwrap();
    document_io::write_document(&mut file, &document).unwrap();
    file.sync_all().unwrap();
    (path, document)
}

async fn child_scan(
    pool: &DbPool,
    claim: &ManagedLibraryOperationClaim,
    list_id: i64,
    verification: bool,
) {
    let repos = ersatzrs_infra::repositories::make_repositories(pool.clone());
    let child = if verification {
        repos
            .managed_library_operations
            .ensure_publication_verification_scan(claim, list_id)
            .await
            .unwrap()
    } else {
        repos
            .managed_library_operations
            .ensure_publication_child_scan(claim, list_id)
            .await
            .unwrap()
    };
    let report = crate::scan_local_library(
        pool,
        &crate::LocalScanConfig {
            library_id: child.library_id,
            ffprobe_path: "unused-for-remote-definitions".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(report.failed_files, 0, "{:?}", report.issues);
    assert!(repos
        .managed_library_operations
        .complete_operation(&ManagedLibraryOperationClaim {
            operation: child,
            lease_owner: claim.lease_owner.clone()
        })
        .await
        .unwrap());
}

async fn publish_through_cleanup(
    pool: &DbPool,
    queued: &ersatzrs_core::repositories::AddonMediaListPublicationEnqueue,
) -> ManagedLibraryOperationClaim {
    let repos = ersatzrs_infra::repositories::make_repositories(pool.clone());
    let ops = &repos.managed_library_operations;
    let repo = &repos.addon_media_lists;
    let job = &queued.job;
    let claim = ops
        .claim_operation(queued.operation.id, Utc::now())
        .await
        .unwrap()
        .unwrap();
    assert!(ops
        .transition_publication(&claim, Phase::AwaitingLease, Phase::Staging)
        .await
        .unwrap());
    if let Some(source_revision) = job.source_published_revision {
        repos
            .addon_media_lists
            .materialize_revision_as_draft(&AddonMediaListRevisionRestoreFence {
                list_id: job.list_id,
                source_revision,
                target_draft_revision: job.saved_revision,
                operation_id: job.operation_id,
                generation: job.generation,
                owner_token: claim.operation.owner_token.clone(),
                lease_owner: claim.lease_owner.clone(),
                active_at: Utc::now(),
            })
            .await
            .unwrap();
    }
    repo.stage_publication_revision(
        job.list_id,
        job.saved_revision,
        job.operation_id,
        job.generation,
        &claim.operation.owner_token,
        &claim.lease_owner,
    )
    .await
    .unwrap();
    assert!(ops
        .transition_publication(&claim, Phase::Staging, Phase::Scanning)
        .await
        .unwrap());
    child_scan(pool, &claim, job.list_id, false).await;
    assert!(ops
        .transition_publication(&claim, Phase::Scanning, Phase::Committing)
        .await
        .unwrap());
    repo.commit_staged_publication_revision(
        job.list_id,
        job.target_published_revision,
        job.operation_id,
        job.generation,
        &claim.operation.owner_token,
        &claim.lease_owner,
    )
    .await
    .unwrap();
    assert!(ops
        .begin_publication_verification(&claim, Phase::Committing, Outcome::Published)
        .await
        .unwrap());
    repo.prepare_publication_verification_scan(
        job.list_id,
        Outcome::Published,
        job.operation_id,
        job.generation,
        &claim.operation.owner_token,
        &claim.lease_owner,
    )
    .await
    .unwrap();
    child_scan(pool, &claim, job.list_id, true).await;
    let verified = repo
        .verify_staged_publication_revision(
            job.list_id,
            Outcome::Published,
            job.operation_id,
            job.generation,
            &claim.operation.owner_token,
            &claim.lease_owner,
        )
        .await;
    if let Err(error) = verified {
        let projections: Vec<(String, bool, Option<i64>, Option<i64>)> = sqlx::query_as(
            r#"SELECT "item"."ProviderId", "item"."Published", "item"."MediaItemId", "projection"."EpisodeId"
               FROM "AddonMediaListItem" AS "item"
               LEFT JOIN "AddonMediaListItemEpisode" AS "projection" ON "projection"."AddonMediaListItemId"="item"."Id"
               WHERE "item"."AddonMediaListId"=? ORDER BY "item"."Id""#,
        ).bind(job.list_id).fetch_all(pool).await.unwrap();
        panic!("publication verification failed: {error}; fixture projections: {projections:?}");
    }
    assert!(ops
        .transition_publication(&claim, Phase::Verifying, Phase::CleaningUp)
        .await
        .unwrap());
    claim
}

async fn publish_received(
    pool: &DbPool,
    queued: &ersatzrs_core::repositories::AddonMediaListPublicationEnqueue,
) {
    let claim = publish_through_cleanup(pool, queued).await;
    let repos = ersatzrs_infra::repositories::make_repositories(pool.clone());
    let ops = &repos.managed_library_operations;
    let repo = &repos.addon_media_lists;
    let job = &queued.job;
    repo.finalize_staged_publication_revision(
        job.list_id,
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
}

#[tokio::test]
async fn copied_stream_saved_folder_discovers_publishes_and_replays_without_draft_changes() {
    copied_stream_round_trip(false, false).await;
}

#[tokio::test]
async fn copied_stream_whole_folder_preserves_draft_manifest_and_replays_published_content() {
    copied_stream_round_trip(true, false).await;
}

#[tokio::test]
async fn copied_stream_queued_publication_preserves_later_operator_save_and_replays_published_content(
) {
    copied_stream_round_trip(false, true).await;
}

#[expect(
    clippy::too_many_lines,
    reason = "Keep the source-copy-activation-replay assertions in one scenario"
)]
async fn copied_stream_round_trip(whole_folder: bool, save_after_enqueue: bool) {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::WARN)
        .try_init();
    let source = tempfile::tempdir().unwrap();
    let (source_pool, installed, source_root) = profile(source.path()).await;
    let (saved_path, document) =
        source_document(&source_pool, &installed, &source_root, whole_folder).await;
    assert!(document.draft.files.is_empty());
    assert!(document.published.as_ref().unwrap().files.is_empty());
    let receiver = tempfile::tempdir().unwrap();
    let (pool, _, root) = profile(receiver.path()).await;
    let folder = root.join("Copied list");
    std::fs::create_dir(&folder).unwrap();
    let copy = folder.join(SAVED_LIST_FILE_NAME);
    let copied_manifest = if whole_folder {
        let source_folder = saved_path.parent().unwrap();
        // Test-only whole-folder copy. Every fixture entry must be an ordinary
        // contained file or directory; no execution state is synthesized here.
        let mut pending = vec![(source_folder.to_path_buf(), folder.clone())];
        while let Some((from, to)) = pending.pop() {
            for entry in std::fs::read_dir(from).unwrap() {
                let entry = entry.unwrap();
                let kind = entry.file_type().unwrap();
                let destination = to.join(entry.file_name());
                if kind.is_dir() {
                    std::fs::create_dir(&destination).unwrap();
                    pending.push((entry.path(), destination));
                } else {
                    assert!(kind.is_file());
                    std::fs::copy(entry.path(), destination).unwrap();
                }
            }
        }
        assert!(folder.join("streams/.ersatzrs-playlist.json").is_file());
        let bytes = std::fs::read(folder.join("list.playlist.yml")).unwrap();
        let manifest: serde_yaml::Value = serde_yaml::from_slice(&bytes).unwrap();
        assert_eq!(manifest["name"].as_str(), Some("New draft list"));
        assert!(manifest["chapters"]
            .as_str()
            .unwrap()
            .contains("New draft chapter"));
        Some(bytes)
    } else {
        std::fs::copy(saved_path, &copy).unwrap();
        assert!(!folder.join("list.playlist.yml").exists());
        None
    };
    let copied_bytes = std::fs::read(&copy).unwrap();
    let mut discovered =
        ersatzrs_infra::addon_media_list_saved::admission::discover_copied_lists(&pool)
            .await
            .unwrap();
    assert_eq!(discovered.refused, 0);
    assert_eq!(discovered.admitted.len(), 1);
    let queued = discovered.admitted.remove(0);
    let repo = ersatzrs_infra::repositories::SqliteAddonMediaListRepository(pool.clone());
    let admitted = repo.list_items(queued.job.list_id).await.unwrap();
    assert_eq!(admitted.len(), 2);
    let owners: std::collections::BTreeMap<String, i64> = sqlx::query_as(
        r#"SELECT "ProviderId", "Id" FROM "AddonMediaListItem" WHERE "AddonMediaListId"=?"#,
    )
    .bind(queued.job.list_id)
    .fetch_all(&pool)
    .await
    .unwrap()
    .into_iter()
    .collect();
    let draft_only = admitted
        .iter()
        .find(|item| item.id == owners["draft-only"])
        .unwrap();
    assert_eq!(draft_only.title, "Draft only");
    assert!(draft_only.included);
    let published_item = admitted
        .iter()
        .find(|item| item.id == owners["published-item"])
        .unwrap();
    assert_eq!(published_item.title, "New draft item");
    assert!(!published_item.included);
    let metadata: String =
        sqlx::query_scalar(r#"SELECT "MetadataOverrideJson" FROM "AddonMediaList" WHERE "Id"=?"#)
            .bind(queued.job.list_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&metadata).unwrap()["title"],
        "New draft list"
    );
    let expected_draft_revision = if save_after_enqueue {
        let item = repo
            .list_items(queued.job.list_id)
            .await
            .unwrap()
            .into_iter()
            .find(|item| item.title == "Draft only")
            .unwrap();
        assert!(item.included);
        let revision = repo
            .save_editor_task(&AddonMediaListEditorSave {
                list_id: queued.job.list_id,
                expected_revision: queued.job.saved_revision,
                metadata: None,
                settings: None,
                delivery_mode: None,
                disk_warning_gb: None,
                options: None,
                items: vec![AddonMediaListEditorItem {
                    review: AddonMediaListItemReviewEdit {
                        item_id: item.id,
                        content_kind: item.content_kind,
                        included: false,
                        liveness_override: item.liveness_override,
                        season: item.season,
                        episode: item.episode,
                    },
                    expected_review: None,
                    role: None,
                    metadata: None,
                    chapters: None,
                    replacement: None,
                }],
            })
            .await
            .unwrap();
        assert_eq!(revision, queued.job.saved_revision + 1);
        revision
    } else {
        queued.job.saved_revision
    };
    let before = repo.list_items(queued.job.list_id).await.unwrap();
    if save_after_enqueue {
        assert!(
            !before
                .iter()
                .find(|item| item.title == "Draft only")
                .unwrap()
                .included
        );
    }
    publish_received(&pool, &queued).await;
    let repeat = ersatzrs_infra::addon_media_list_saved::admission::discover_copied_lists(&pool)
        .await
        .unwrap();
    assert_eq!(repeat.refused, 0);
    assert!(repeat.admitted.is_empty());
    let report = crate::scan_local_library(
        &pool,
        &crate::LocalScanConfig {
            library_id: queued.operation.library_id,
            ffprobe_path: "unused-for-remote-definitions".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(report.failed_files, 0, "{:?}", report.issues);
    let after = repo.list_items(queued.job.list_id).await.unwrap();
    for (old, new) in before.iter().zip(&after) {
        assert_eq!(
            (&old.title, old.included, old.rank),
            (&new.title, new.included, new.rank)
        );
    }
    assert_eq!(before.len(), after.len());
    let active = repo
        .active_publication_snapshot(queued.job.list_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(active.name, "Published list");
    assert_eq!(
        active
            .items
            .iter()
            .filter(|item| item.published)
            .map(|item| item.provider_id.as_str())
            .collect::<Vec<_>>(),
        ["published-item"]
    );
    let form = repo
        .get_admin_form(queued.job.list_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(form.name, "New draft list");
    assert_eq!(form.draft_revision, expected_draft_revision);
    let projected: Vec<(String, String)> = sqlx::query_as(
        r#"SELECT "stream"."ProviderId", "metadata"."Title" FROM "RemoteStream" AS "stream"
           JOIN "RemoteStreamMetadata" AS "metadata" ON "metadata"."RemoteStreamId"="stream"."Id"
           WHERE "stream"."AddonId"=? ORDER BY "stream"."ProviderId""#,
    )
    .bind(ADDON)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        projected,
        vec![("published-item".into(), "Published item".into())]
    );
    let after_bytes = std::fs::read(copy).unwrap();
    if save_after_enqueue {
        assert_ne!(after_bytes, copied_bytes);
        let saved: SavedListDocument = serde_json::from_slice(&after_bytes).unwrap();
        assert_eq!(saved.published, document.published);
        assert!(
            !saved
                .draft
                .items
                .iter()
                .find(|item| item.content.provider_id == "draft-only")
                .unwrap()
                .content
                .included
        );
        assert_eq!(saved.draft.revision, expected_draft_revision);
    } else {
        assert_eq!(after_bytes, copied_bytes);
    }
    if let Some(bytes) = copied_manifest {
        assert_eq!(
            std::fs::read(folder.join("list.playlist.yml")).unwrap(),
            bytes
        );
    }
}

#[path = "saved_writer_tests.rs"]
mod saved_writer_tests;

#[path = "saved_scan_chapter_tests.rs"]
mod saved_scan_chapter_tests;
