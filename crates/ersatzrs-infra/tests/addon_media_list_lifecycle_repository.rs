mod common;

use std::collections::BTreeMap;
use std::fmt::Write as _;

use chrono::{TimeZone, Utc};
use ersatzrs_core::repositories::{
    AddonMediaListDraftOptions, AddonMediaListItemOptionValues, AddonMediaListItemTombstone,
    AddonMediaListLifecycleRepository, AddonMediaListLifecycleSettings, AddonMediaListOptionSchema,
    AddonMediaListOptionSchemaRef, AddonProcessedArtifactPolicy, CreateAddonMediaListLifecycleJob,
    RepositoryError,
};
use ersatzrs_domain::{
    AddonMediaListArtifactKind, AddonMediaListArtifactState, AddonMediaListImportMode,
    AddonMediaListImportPurpose, AddonMediaListIncrementalStrategy, AddonMediaListLifecycleJobKind,
    AddonMediaListLifecyclePhase, AddonMediaListOptionValue, AddonMediaListRefreshCadence,
    AddonMediaListRefreshPublicationPolicy, AddonToolMaintenanceState, AddonToolMaintenanceTrigger,
};
use ersatzrs_infra::addon_media_list_sync::AddonMediaListSynchronizer;
use ersatzrs_infra::repositories::make_repositories;
use sha2::{Digest as _, Sha256};
use sqlx::Executor;

async fn register_option_schema_fixture(pool: &sqlx::SqlitePool) -> AddonMediaListOptionSchemaRef {
    let definition = schema_definition("Enable fixture behavior.");
    let schema_ref = AddonMediaListOptionSchemaRef {
        addon_id: "org.example.lifecycle".into(),
        capability_id: "media-list.options.v1".into(),
        schema_digest: sha256_hex(definition.as_bytes()),
    };
    let schema = AddonMediaListOptionSchema {
        schema: schema_ref.clone(),
        definition_json: definition,
        created_at: fixed_time(),
    };
    let repositories = make_repositories(pool.clone());
    repositories
        .addon_media_lists
        .register_option_schema(&schema)
        .await
        .unwrap();
    assert_eq!(
        repositories
            .addon_media_lists
            .option_schema(&schema_ref)
            .await
            .unwrap(),
        Some(schema)
    );
    schema_ref
}

fn draft_option_fixture(schema: &AddonMediaListOptionSchemaRef) -> AddonMediaListDraftOptions {
    AddonMediaListDraftOptions {
        list_id: 1,
        draft_revision: 0,
        schema: Some(schema.clone()),
        list_values: BTreeMap::from([
            ("enabled".into(), AddonMediaListOptionValue::Boolean(false)),
            (
                "profile".into(),
                AddonMediaListOptionValue::Enum("list".into()),
            ),
        ]),
        item_values: vec![
            AddonMediaListItemOptionValues {
                item_id: Some(10),
                provider_id: "item-10".into(),
                values: BTreeMap::from([(
                    "profile".into(),
                    AddonMediaListOptionValue::Enum("item".into()),
                )]),
            },
            AddonMediaListItemOptionValues {
                item_id: Some(11),
                provider_id: "a-item".into(),
                values: BTreeMap::from([(
                    "profile".into(),
                    AddonMediaListOptionValue::Enum("list".into()),
                )]),
            },
        ],
    }
}

#[tokio::test]
async fn lifecycle_options_round_trip_inherit_and_retain_immutable_revision() {
    let pool = common::test_db().await;
    let repositories = make_repositories(pool.clone());
    seed_list(&pool).await;
    sqlx::query(
        r#"INSERT INTO "AddonMediaListItem" (
               "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind", "Rank", "Title"
           ) VALUES (11, 1, 'a-item', 'A item', 4, 1, 'A item')"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    let schema_ref = register_option_schema_fixture(&pool).await;
    let draft = draft_option_fixture(&schema_ref);
    assert_eq!(
        repositories
            .addon_media_lists
            .save_draft_options(&draft)
            .await
            .unwrap(),
        1
    );

    let loaded = repositories
        .addon_media_lists
        .draft_options(1, Some(&schema_ref))
        .await
        .unwrap()
        .unwrap();
    let defaults = BTreeMap::from([
        ("enabled".into(), AddonMediaListOptionValue::Boolean(true)),
        (
            "profile".into(),
            AddonMediaListOptionValue::Enum("default".into()),
        ),
    ]);
    assert_eq!(loaded.draft_revision, 1);
    assert_eq!(
        loaded.effective_values(&defaults, 10),
        BTreeMap::from([
            ("enabled".into(), AddonMediaListOptionValue::Boolean(false)),
            (
                "profile".into(),
                AddonMediaListOptionValue::Enum("item".into()),
            ),
        ])
    );

    repositories
        .addon_media_lists
        .retain_published_options(1, 1, 1, precise_time())
        .await
        .unwrap();
    // An identical replay is idempotent and can load the saved binding without
    // supplying a current package schema.
    repositories
        .addon_media_lists
        .retain_published_options(1, 1, 1, precise_time())
        .await
        .unwrap();
    let published = repositories
        .addon_media_lists
        .published_options(1, 1, Some(&schema_ref))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(published.schema, Some(schema_ref));
    assert!(published
        .item_values
        .iter()
        .all(|item| item.item_id.is_none()));
    assert_eq!(published.item_values[0].provider_id, "a-item");
    assert_eq!(published.item_values[1].provider_id, "item-10");

    let mut newer_draft = loaded;
    newer_draft.list_values.insert(
        "profile".into(),
        AddonMediaListOptionValue::Enum("default".into()),
    );
    repositories
        .addon_media_lists
        .save_draft_options(&newer_draft)
        .await
        .unwrap();
    assert!(matches!(
        repositories
            .addon_media_lists
            .retain_published_options(1, 2, 1, fixed_time())
            .await,
        Err(RepositoryError::Conflict(_))
    ));
}

#[tokio::test]
async fn published_option_snapshot_rechecks_revision_after_a_concurrent_edit() {
    let temporary = tempfile::tempdir().unwrap();
    let database_url = format!("sqlite:{}", temporary.path().join("lifecycle.db").display());
    let pool = ersatzrs_infra::db::init(&database_url).await.unwrap();
    let repositories = make_repositories(pool.clone());
    seed_list(&pool).await;
    let definition = schema_definition("Enable fixture behavior.");
    let schema_ref = AddonMediaListOptionSchemaRef {
        addon_id: "org.example.lifecycle".into(),
        capability_id: "media-list.options.v1".into(),
        schema_digest: sha256_hex(definition.as_bytes()),
    };
    repositories
        .addon_media_lists
        .register_option_schema(&AddonMediaListOptionSchema {
            schema: schema_ref.clone(),
            definition_json: definition,
            created_at: fixed_time(),
        })
        .await
        .unwrap();
    repositories
        .addon_media_lists
        .save_draft_options(&AddonMediaListDraftOptions {
            list_id: 1,
            draft_revision: 0,
            schema: Some(schema_ref),
            list_values: BTreeMap::from([(
                "enabled".into(),
                AddonMediaListOptionValue::Boolean(true),
            )]),
            item_values: Vec::new(),
        })
        .await
        .unwrap();

    let mut edit = ersatzrs_infra::db::begin_write(&pool).await.unwrap();
    sqlx::query(
        r#"UPDATE "AddonMediaList"
           SET "DraftRevision" = 2, "DraftOptionValuesJson" = '{"enabled":false}'
           WHERE "Id" = 1"#,
    )
    .execute(&mut *edit)
    .await
    .unwrap();
    let repository = repositories.addon_media_lists.clone();
    let retain = tokio::spawn(async move {
        repository
            .retain_published_options(1, 1, 1, fixed_time())
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    edit.commit().await.unwrap();

    let result = tokio::time::timeout(std::time::Duration::from_secs(5), retain)
        .await
        .expect("publication snapshot must resume after the writer commits")
        .expect("publication snapshot task must not panic");
    assert!(matches!(result, Err(RepositoryError::Conflict(_))));
    let retained_count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "AddonMediaListRevision"
           WHERE "AddonMediaListId" = 1 AND "Revision" = 1"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(retained_count, 0);
}

#[tokio::test]
async fn option_schema_digest_and_saved_binding_fail_closed() {
    let pool = common::test_db().await;
    let repositories = make_repositories(pool.clone());
    seed_list(&pool).await;
    let definition = schema_definition("Enable fixture behavior.");
    let valid = AddonMediaListOptionSchema {
        schema: AddonMediaListOptionSchemaRef {
            addon_id: "org.example.lifecycle".into(),
            capability_id: "media-list.options.v1".into(),
            schema_digest: sha256_hex(definition.as_bytes()),
        },
        definition_json: definition,
        created_at: fixed_time(),
    };
    repositories
        .addon_media_lists
        .register_option_schema(&valid)
        .await
        .unwrap();

    let wrong_digest = AddonMediaListOptionSchema {
        schema: AddonMediaListOptionSchemaRef {
            addon_id: "org.example.lifecycle".into(),
            capability_id: "media-list.options.v2".into(),
            schema_digest: "not-the-definition-digest".into(),
        },
        ..valid.clone()
    };
    assert!(matches!(
        repositories
            .addon_media_lists
            .register_option_schema(&wrong_digest)
            .await,
        Err(RepositoryError::Conflict(_))
    ));

    let changed_definition = schema_definition("Changed fixture description.");
    assert!(matches!(
        repositories
            .addon_media_lists
            .register_option_schema(&AddonMediaListOptionSchema {
                schema: AddonMediaListOptionSchemaRef {
                    addon_id: "org.example.lifecycle".into(),
                    capability_id: "media-list.options.v1".into(),
                    schema_digest: sha256_hex(changed_definition.as_bytes()),
                },
                definition_json: changed_definition,
                ..valid
            })
            .await,
        Err(RepositoryError::Conflict(_))
    ));

    assert!(matches!(
        repositories
            .addon_media_lists
            .draft_options(
                1,
                Some(&AddonMediaListOptionSchemaRef {
                    addon_id: "org.example.lifecycle".into(),
                    capability_id: "media-list.options.v1".into(),
                    schema_digest: "different".into(),
                })
            )
            .await,
        Err(RepositoryError::Conflict(_))
    ));

    let invalid_values = AddonMediaListDraftOptions {
        list_id: 1,
        draft_revision: 0,
        schema: Some(AddonMediaListOptionSchemaRef {
            addon_id: "org.example.lifecycle".into(),
            capability_id: "media-list.options.v1".into(),
            schema_digest: sha256_hex(schema_definition("Enable fixture behavior.").as_bytes()),
        }),
        list_values: BTreeMap::from([("unknown".into(), AddonMediaListOptionValue::Boolean(true))]),
        item_values: Vec::new(),
    };
    assert!(matches!(
        repositories
            .addon_media_lists
            .save_draft_options(&invalid_values)
            .await,
        Err(RepositoryError::Conflict(_))
    ));

    let wrong_type = AddonMediaListDraftOptions {
        list_values: BTreeMap::from([(
            "enabled".into(),
            AddonMediaListOptionValue::Enum("true".into()),
        )]),
        ..invalid_values
    };
    assert!(matches!(
        repositories
            .addon_media_lists
            .save_draft_options(&wrong_type)
            .await,
        Err(RepositoryError::Conflict(_))
    ));
}

#[tokio::test]
async fn lifecycle_and_global_artifact_policy_round_trip_without_enabling_behavior() {
    let pool = common::test_db().await;
    let repositories = make_repositories(pool.clone());
    seed_list(&pool).await;
    let settings = AddonMediaListLifecycleSettings {
        cadence: AddonMediaListRefreshCadence::Monthly,
        publication_policy: AddonMediaListRefreshPublicationPolicy::AutoPublishAdditions,
        next_refresh_at: Some(fixed_time()),
        retry_at: None,
        last_success_at: Some(fixed_time()),
        processed_artifact_byte_limit: Some(5_000_000),
    };
    repositories
        .addon_media_lists
        .save_lifecycle_settings(1, &settings)
        .await
        .unwrap();
    assert_eq!(
        repositories
            .addon_media_lists
            .lifecycle_settings(1)
            .await
            .unwrap(),
        Some(settings)
    );
    // The frozen Boolean scheduler must not run a monthly cadence as daily.
    let legacy_auto_refresh: bool =
        sqlx::query_scalar(r#"SELECT "AutoRefresh" FROM "AddonMediaList" WHERE "Id" = 1"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!legacy_auto_refresh);

    let policy = AddonProcessedArtifactPolicy {
        global_byte_limit: Some(10_000_000),
        minimum_free_bytes: Some(1_000_000),
        retention_seconds: Some(86_400),
        max_preparation_concurrency: Some(2),
    };
    repositories
        .addon_media_lists
        .save_processed_artifact_policy(&policy)
        .await
        .unwrap();
    assert_eq!(
        repositories
            .addon_media_lists
            .processed_artifact_policy()
            .await
            .unwrap(),
        policy
    );
}

#[tokio::test]
async fn normal_list_delete_unlinks_retained_artifacts_before_owned_rows_cascade() {
    let pool = common::test_db().await;
    let repositories = make_repositories(pool.clone());
    seed_list(&pool).await;
    repositories
        .addon_media_lists
        .retain_published_options(1, 1, 0, fixed_time())
        .await
        .unwrap();
    let artifact = repositories
        .addon_media_lists
        .create_artifact(
            1,
            Some(10),
            "delete-list-artifact",
            AddonMediaListArtifactKind::ProcessedMedia,
            1,
            fixed_time(),
        )
        .await
        .unwrap();
    repositories
        .addon_media_lists
        .link_revision_artifact(1, 1, artifact.id)
        .await
        .unwrap();

    AddonMediaListSynchronizer::new(pool.clone())
        .delete_list(1)
        .await
        .expect("delete list after explicitly unlinking retained artifacts");

    let remaining: i64 = sqlx::query_scalar(
        r#"SELECT
               (SELECT COUNT(*) FROM "AddonMediaList" WHERE "Id" = 1) +
               (SELECT COUNT(*) FROM "AddonMediaListArtifact" WHERE "Id" = ?) +
               (SELECT COUNT(*) FROM "AddonMediaListRevisionArtifact")"#,
    )
    .bind(artifact.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(remaining, 0);
}

async fn seed_storage_primitive_dependencies(pool: &sqlx::SqlitePool) {
    sqlx::raw_sql(
        r#"INSERT INTO "MediaSource" ("Id") VALUES (20);
           INSERT INTO "MediaSource" ("Id") VALUES (21);
           INSERT INTO "Library" ("Id", "MediaKind", "MediaSourceId", "Name")
             VALUES (20, 0, 20, 'Lifecycle library');
           INSERT INTO "Library" ("Id", "MediaKind", "MediaSourceId", "Name")
             VALUES (21, 0, 21, 'Other lifecycle library');
           INSERT INTO "AddonMediaList" (
               "Id", "AddonId", "ProviderId", "SourceUrl", "Name"
           ) VALUES
               (2, 'org.example.lifecycle', 'other-list',
                'https://example.invalid/other-list', 'Other fixture list'),
               (3, 'org.example.lifecycle', 'unbound-list',
                'https://example.invalid/unbound-list', 'Unbound fixture list');
           INSERT INTO "AddonMediaListItem" (
               "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind", "Rank", "Title"
           ) VALUES (20, 2, 'item-20', 'Item 20', 4, 0, 'Item 20');
           INSERT INTO "AddonMediaListLibrary" (
               "AddonMediaListId", "LibraryId", "FolderName", "ManifestPath"
           ) VALUES
               (1, 20, 'Lifecycle list', 'managed/lifecycle-list'),
               (2, 21, 'Other lifecycle list', 'managed/other-lifecycle-list');
           INSERT INTO "ManagedLibraryOperation" (
               "Id", "LibraryId", "OwnerToken", "Kind", "State", "CreatedAt", "UpdatedAt"
           ) VALUES (20, 20, 'lifecycle-owner', 8, 0,
                     '2026-09-04 12:00:00', '2026-09-04 12:00:00');
           INSERT INTO "ManagedLibraryOperation" (
               "Id", "LibraryId", "OwnerToken", "Kind", "State", "CreatedAt", "UpdatedAt"
           ) VALUES (21, 21, 'other-lifecycle-owner', 8, 0,
                     '2026-09-04 12:00:00', '2026-09-04 12:00:00');
           INSERT INTO "ManagedLibraryOperation" (
               "Id", "LibraryId", "OwnerToken", "Kind", "State", "CreatedAt", "UpdatedAt"
           ) VALUES (22, 20, 'stale-lifecycle-owner', 8, 0,
                     '2026-09-04 12:00:00', '2026-09-04 12:00:00');
           INSERT INTO "ManagedLibraryOperation" (
               "Id", "LibraryId", "OwnerToken", "Kind", "State", "CreatedAt", "UpdatedAt"
           ) VALUES (23, 20, 'completed-lifecycle-owner', 8, 3,
                     '2026-09-04 12:00:00', '2026-09-04 12:00:00');
           INSERT INTO "AddonMediaListImportJob" (
               "Id", "AddonMediaListId", "Generation", "State", "RecordCapability",
               "LeaseOwner", "LeaseExpiresAt", "CreatedAt", "UpdatedAt"
           ) VALUES
               (30, 1, 1, 1, 'media-list.import.v1', 'import-owner',
                '2026-09-04 12:10:00', '2026-09-04 12:00:00', '2026-09-04 12:00:00'),
               (31, 2, 1, 1, 'media-list.import.v1', 'other-import-owner',
                '2026-09-04 12:10:00', '2026-09-04 12:00:00', '2026-09-04 12:00:00');"#,
    )
    .execute(pool)
    .await
    .unwrap();
}

async fn assert_checkpoint_conflict(
    pool: &sqlx::SqlitePool,
    relative_path: &str,
    generation: i32,
    digest: &str,
    entry_count: u64,
) {
    let repositories = make_repositories(pool.clone());
    assert!(matches!(
        repositories
            .addon_media_lists
            .replace_import_checkpoint(
                1,
                relative_path,
                generation,
                digest,
                entry_count,
                fixed_time(),
            )
            .await,
        Err(RepositoryError::Conflict(_))
    ));
}

async fn assert_checkpoint_storage(pool: &sqlx::SqlitePool) -> (i64, i64) {
    let repositories = make_repositories(pool.clone());
    let checkpoint = repositories
        .addon_media_lists
        .replace_import_checkpoint(
            1,
            "checkpoints/archive.txt",
            1,
            "checkpoint-digest",
            17,
            fixed_time(),
        )
        .await
        .unwrap();
    assert_eq!(
        repositories
            .addon_media_lists
            .import_checkpoint(1)
            .await
            .unwrap(),
        Some(checkpoint.clone())
    );
    assert_eq!(
        repositories
            .addon_media_lists
            .replace_import_checkpoint(
                1,
                "checkpoints/archive.txt",
                1,
                "checkpoint-digest",
                17,
                fixed_time() + chrono::Duration::minutes(1),
            )
            .await
            .unwrap(),
        checkpoint
    );
    let checkpoint = repositories
        .addon_media_lists
        .replace_import_checkpoint(
            1,
            "checkpoints/archive-next.txt",
            2,
            "checkpoint-digest-next",
            20,
            fixed_time() + chrono::Duration::minutes(2),
        )
        .await
        .unwrap();
    assert_checkpoint_conflict(pool, "checkpoints/archive.txt", 1, "checkpoint-digest", 17).await;
    assert_checkpoint_conflict(
        pool,
        "checkpoints/stale-page.txt",
        2,
        "checkpoint-stale-page",
        19,
    )
    .await;
    assert_checkpoint_conflict(
        pool,
        "checkpoints/different-page.txt",
        2,
        "different-digest",
        20,
    )
    .await;
    let other_checkpoint = repositories
        .addon_media_lists
        .replace_import_checkpoint(
            2,
            "checkpoints/other.txt",
            1,
            "other-checkpoint-digest",
            3,
            fixed_time(),
        )
        .await
        .unwrap();
    (checkpoint.id, other_checkpoint.id)
}

async fn assert_import_lifecycle_storage(
    pool: &sqlx::SqlitePool,
    checkpoint_id: i64,
    other_checkpoint_id: i64,
) {
    let repositories = make_repositories(pool.clone());
    let mut import = repositories
        .addon_media_lists
        .import_lifecycle(30)
        .await
        .unwrap()
        .unwrap();
    import.purpose = AddonMediaListImportPurpose::Reconcile;
    import.mode = AddonMediaListImportMode::Incremental;
    import.strategy = AddonMediaListIncrementalStrategy::SkipKnown;
    import.examined_count = 19;
    import.archived_skipped_count = 7;
    import.emitted_count = 12;
    import.checkpoint_id = Some(checkpoint_id);
    repositories
        .addon_media_lists
        .save_import_lifecycle(&import, "import-owner", fixed_time())
        .await
        .unwrap();
    assert_eq!(
        repositories
            .addon_media_lists
            .import_lifecycle(30)
            .await
            .unwrap(),
        Some(import)
    );
    let mut update = repositories
        .addon_media_lists
        .import_lifecycle(30)
        .await
        .unwrap()
        .unwrap();
    update.checkpoint_id = Some(other_checkpoint_id);
    assert!(matches!(
        repositories
            .addon_media_lists
            .save_import_lifecycle(&update, "import-owner", fixed_time())
            .await,
        Err(RepositoryError::Conflict(_))
    ));
    update.checkpoint_id = Some(checkpoint_id);
    assert!(matches!(
        repositories
            .addon_media_lists
            .save_import_lifecycle(&update, "stale-owner", fixed_time())
            .await,
        Err(RepositoryError::Conflict(_))
    ));
    assert!(matches!(
        repositories
            .addon_media_lists
            .save_import_lifecycle(
                &update,
                "import-owner",
                fixed_time() + chrono::Duration::minutes(11),
            )
            .await,
        Err(RepositoryError::Conflict(_))
    ));
}

async fn assert_lifecycle_create_conflict(
    pool: &sqlx::SqlitePool,
    command: CreateAddonMediaListLifecycleJob,
) {
    let repositories = make_repositories(pool.clone());
    assert!(matches!(
        repositories
            .addon_media_lists
            .create_lifecycle_job(&command)
            .await,
        Err(RepositoryError::Conflict(_))
    ));
}

async fn assert_lifecycle_job_storage(pool: &sqlx::SqlitePool) {
    let repositories = make_repositories(pool.clone());
    assert_lifecycle_create_conflict(
        pool,
        lifecycle_job_command(20, 1, 1, AddonMediaListLifecycleJobKind::ResetReload, 0, 0),
    )
    .await;
    assert_lifecycle_create_conflict(
        pool,
        lifecycle_job_command(23, 1, 1, AddonMediaListLifecycleJobKind::Reconcile, 0, 0),
    )
    .await;
    assert_lifecycle_create_conflict(
        pool,
        lifecycle_job_command(22, 1, 1, AddonMediaListLifecycleJobKind::Reconcile, 1, 0),
    )
    .await;
    assert_lifecycle_create_conflict(
        pool,
        lifecycle_job_command(22, 1, 1, AddonMediaListLifecycleJobKind::Reconcile, 0, 1),
    )
    .await;
    let mut lifecycle = repositories
        .addon_media_lists
        .create_lifecycle_job(&lifecycle_job_command(
            20,
            1,
            1,
            AddonMediaListLifecycleJobKind::Reconcile,
            0,
            0,
        ))
        .await
        .unwrap();
    assert_lifecycle_create_conflict(
        pool,
        lifecycle_job_command(21, 1, 2, AddonMediaListLifecycleJobKind::Reconcile, 0, 0),
    )
    .await;
    assert_lifecycle_create_conflict(
        pool,
        lifecycle_job_command(21, 3, 1, AddonMediaListLifecycleJobKind::Reconcile, 0, 0),
    )
    .await;
    let mut cross_list_lifecycle = lifecycle.clone();
    cross_list_lifecycle.candidate_import_job_id = Some(31);
    cross_list_lifecycle.updated_at = fixed_time() + chrono::Duration::minutes(1);
    assert!(matches!(
        repositories
            .addon_media_lists
            .save_lifecycle_job(&cross_list_lifecycle, fixed_time())
            .await,
        Err(RepositoryError::Conflict(_))
    ));
    lifecycle.phase = AddonMediaListLifecyclePhase::ComparingCandidate;
    lifecycle.candidate_import_job_id = Some(30);
    lifecycle.updated_at = fixed_time() + chrono::Duration::minutes(1);
    repositories
        .addon_media_lists
        .save_lifecycle_job(&lifecycle, fixed_time())
        .await
        .unwrap();
    assert_eq!(
        repositories
            .addon_media_lists
            .lifecycle_job(20)
            .await
            .unwrap(),
        Some(lifecycle.clone())
    );
    let mut stale_lifecycle = lifecycle;
    stale_lifecycle.phase = AddonMediaListLifecyclePhase::EnumeratingSource;
    stale_lifecycle.updated_at = fixed_time() + chrono::Duration::minutes(2);
    assert!(matches!(
        repositories
            .addon_media_lists
            .save_lifecycle_job(&stale_lifecycle, fixed_time())
            .await,
        Err(RepositoryError::Conflict(_))
    ));
}

fn tombstone_fixture() -> AddonMediaListItemTombstone {
    AddonMediaListItemTombstone {
        item_id: 10,
        missing_since: fixed_time(),
        last_observed_at: fixed_time() + chrono::Duration::days(1),
        observation_count: 2,
        last_reconcile_generation: 2,
        confirmed_at: None,
    }
}

async fn assert_tombstone_observation_storage(pool: &sqlx::SqlitePool) {
    let repositories = make_repositories(pool.clone());
    let tombstone = tombstone_fixture();
    let forged_initial_tombstone = AddonMediaListItemTombstone {
        confirmed_at: Some(fixed_time() + chrono::Duration::days(3)),
        ..tombstone.clone()
    };
    assert!(matches!(
        repositories
            .addon_media_lists
            .save_item_tombstone(&forged_initial_tombstone, None)
            .await,
        Err(RepositoryError::Conflict(_))
    ));
    assert!(repositories
        .addon_media_lists
        .item_tombstone(10)
        .await
        .unwrap()
        .is_none());
    repositories
        .addon_media_lists
        .save_item_tombstone(&tombstone, None)
        .await
        .unwrap();
    assert_eq!(
        repositories
            .addon_media_lists
            .item_tombstone(10)
            .await
            .unwrap(),
        Some(tombstone.clone())
    );
    repositories
        .addon_media_lists
        .save_item_tombstone(&tombstone, Some(2))
        .await
        .unwrap();
    let mut divergent_same_generation = tombstone.clone();
    divergent_same_generation.observation_count = 3;
    assert!(matches!(
        repositories
            .addon_media_lists
            .save_item_tombstone(&divergent_same_generation, Some(2))
            .await,
        Err(RepositoryError::Conflict(_))
    ));
    let mut newer_generation_without_observation = tombstone.clone();
    newer_generation_without_observation.last_reconcile_generation = 3;
    assert!(matches!(
        repositories
            .addon_media_lists
            .save_item_tombstone(&newer_generation_without_observation, Some(2))
            .await,
        Err(RepositoryError::Conflict(_))
    ));
    let mut newer_tombstone = tombstone.clone();
    newer_tombstone.missing_since = fixed_time() + chrono::Duration::hours(1);
    newer_tombstone.last_observed_at = fixed_time() + chrono::Duration::days(2);
    newer_tombstone.observation_count = 3;
    newer_tombstone.last_reconcile_generation = 3;
    repositories
        .addon_media_lists
        .save_item_tombstone(&newer_tombstone, Some(2))
        .await
        .unwrap();
    let stored = repositories
        .addon_media_lists
        .item_tombstone(10)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.missing_since, tombstone.missing_since);
    assert_eq!(stored.last_reconcile_generation, 3);
}

async fn assert_tombstone_confirmation_storage(pool: &sqlx::SqlitePool) {
    let repositories = make_repositories(pool.clone());
    let tombstone = tombstone_fixture();
    let confirmed_at = fixed_time() + chrono::Duration::days(3);
    assert!(matches!(
        repositories
            .addon_media_lists
            .confirm_item_tombstone(10, 2, confirmed_at)
            .await,
        Err(RepositoryError::Conflict(_))
    ));
    repositories
        .addon_media_lists
        .confirm_item_tombstone(10, 3, confirmed_at)
        .await
        .unwrap();
    assert!(matches!(
        repositories
            .addon_media_lists
            .confirm_item_tombstone(10, 3, confirmed_at)
            .await,
        Err(RepositoryError::Conflict(_))
    ));
    let mut forward_observation = tombstone.clone();
    forward_observation.last_reconcile_generation = 4;
    forward_observation.observation_count = 4;
    forward_observation.last_observed_at = fixed_time() + chrono::Duration::days(4);
    assert!(matches!(
        repositories
            .addon_media_lists
            .save_item_tombstone(&forward_observation, Some(3))
            .await,
        Err(RepositoryError::Conflict(_))
    ));
    let mut changed_confirmation = forward_observation.clone();
    changed_confirmation.confirmed_at = Some(confirmed_at + chrono::Duration::minutes(1));
    assert!(matches!(
        repositories
            .addon_media_lists
            .save_item_tombstone(&changed_confirmation, Some(3))
            .await,
        Err(RepositoryError::Conflict(_))
    ));
    forward_observation.confirmed_at = Some(confirmed_at);
    repositories
        .addon_media_lists
        .save_item_tombstone(&forward_observation, Some(3))
        .await
        .unwrap();
    assert_eq!(
        repositories
            .addon_media_lists
            .item_tombstone(10)
            .await
            .unwrap()
            .unwrap()
            .confirmed_at,
        Some(confirmed_at)
    );
    assert!(matches!(
        repositories
            .addon_media_lists
            .save_item_tombstone(&tombstone, Some(2))
            .await,
        Err(RepositoryError::Conflict(_))
    ));
    assert!(matches!(
        repositories
            .addon_media_lists
            .clear_item_tombstone(10, 2)
            .await,
        Err(RepositoryError::Conflict(_))
    ));
    repositories
        .addon_media_lists
        .clear_item_tombstone(10, 4)
        .await
        .unwrap();
    assert!(repositories
        .addon_media_lists
        .item_tombstone(10)
        .await
        .unwrap()
        .is_none());
}

async fn assert_artifact_storage(pool: &sqlx::SqlitePool) {
    let repositories = make_repositories(pool.clone());
    assert!(matches!(
        repositories
            .addon_media_lists
            .create_artifact(
                1,
                Some(20),
                "cross-list-artifact",
                AddonMediaListArtifactKind::ProcessedMedia,
                1,
                fixed_time(),
            )
            .await,
        Err(RepositoryError::Conflict(_))
    ));
    let mut artifact = repositories
        .addon_media_lists
        .create_artifact(
            1,
            Some(10),
            "artifact-cache-key",
            AddonMediaListArtifactKind::ProcessedMedia,
            4_096,
            fixed_time(),
        )
        .await
        .unwrap();
    artifact.state = AddonMediaListArtifactState::Ready;
    artifact.relative_path = Some("artifacts/ready.bin".into());
    artifact.byte_length = Some(2_048);
    artifact.sha256 = Some("artifact-digest".into());
    artifact.updated_at = fixed_time() + chrono::Duration::minutes(2);
    artifact.completed_at = Some(artifact.updated_at);
    repositories
        .addon_media_lists
        .save_artifact(&artifact, fixed_time())
        .await
        .unwrap();
    let mut stale_artifact = artifact.clone();
    stale_artifact.updated_at = fixed_time() + chrono::Duration::minutes(3);
    stale_artifact.relative_path = Some("artifacts/stale.bin".into());
    assert!(matches!(
        repositories
            .addon_media_lists
            .save_artifact(&stale_artifact, fixed_time())
            .await,
        Err(RepositoryError::Conflict(_))
    ));
    repositories
        .addon_media_lists
        .retain_published_options(1, 1, 0, fixed_time())
        .await
        .unwrap();
    repositories
        .addon_media_lists
        .link_revision_artifact(1, 1, artifact.id)
        .await
        .unwrap();
    assert_eq!(
        repositories
            .addon_media_lists
            .revision_artifact_ids(1, 1)
            .await
            .unwrap(),
        vec![artifact.id]
    );
    assert!(
        sqlx::query(r#"DELETE FROM "AddonMediaListArtifact" WHERE "Id" = ?"#)
            .bind(artifact.id)
            .execute(pool)
            .await
            .is_err()
    );
    repositories
        .addon_media_lists
        .unlink_revision_artifact(1, 1, artifact.id)
        .await
        .unwrap();
}

async fn assert_tool_maintenance_storage(pool: &sqlx::SqlitePool) {
    let repositories = make_repositories(pool.clone());
    let mut first_attempt = repositories
        .addon_media_lists
        .create_tool_maintenance_attempt(
            "org.example.lifecycle",
            "fixture-tool",
            AddonToolMaintenanceTrigger::Daily,
            fixed_time(),
        )
        .await
        .unwrap();
    first_attempt.state = AddonToolMaintenanceState::WaitingToRetry;
    first_attempt.attempt_count = 1;
    first_attempt.updated_at = fixed_time() + chrono::Duration::minutes(1);
    repositories
        .addon_media_lists
        .save_tool_maintenance_attempt(&first_attempt, fixed_time())
        .await
        .unwrap();
    let mut stale_attempt = first_attempt.clone();
    stale_attempt.attempt_count = 2;
    stale_attempt.updated_at = fixed_time() + chrono::Duration::minutes(2);
    assert!(matches!(
        repositories
            .addon_media_lists
            .save_tool_maintenance_attempt(&stale_attempt, fixed_time())
            .await,
        Err(RepositoryError::Conflict(_))
    ));
    let mut restart_attempt = repositories
        .addon_media_lists
        .create_tool_maintenance_attempt(
            "org.example.lifecycle",
            "fixture-tool",
            AddonToolMaintenanceTrigger::Startup,
            fixed_time() + chrono::Duration::minutes(1),
        )
        .await
        .unwrap();
    restart_attempt.state = AddonToolMaintenanceState::Running;
    restart_attempt.updated_at = fixed_time() + chrono::Duration::minutes(2);
    repositories
        .addon_media_lists
        .save_tool_maintenance_attempt(
            &restart_attempt,
            fixed_time() + chrono::Duration::minutes(1),
        )
        .await
        .unwrap();
    let mut overlapping_attempt = repositories
        .addon_media_lists
        .create_tool_maintenance_attempt(
            "org.example.lifecycle",
            "fixture-tool",
            AddonToolMaintenanceTrigger::Manual,
            fixed_time() + chrono::Duration::minutes(2),
        )
        .await
        .unwrap();
    overlapping_attempt.state = AddonToolMaintenanceState::Running;
    overlapping_attempt.updated_at = fixed_time() + chrono::Duration::minutes(3);
    assert!(repositories
        .addon_media_lists
        .save_tool_maintenance_attempt(
            &overlapping_attempt,
            fixed_time() + chrono::Duration::minutes(2),
        )
        .await
        .is_err());
    let history = repositories
        .addon_media_lists
        .tool_maintenance_history("org.example.lifecycle", "fixture-tool")
        .await
        .unwrap();
    assert_eq!(history.len(), 3);
    assert_eq!(history[0].trigger, AddonToolMaintenanceTrigger::Manual);
    assert_eq!(history[1].trigger, AddonToolMaintenanceTrigger::Startup);
    assert_eq!(history[2].state, AddonToolMaintenanceState::WaitingToRetry);
}

#[tokio::test]
async fn lifecycle_storage_primitives_round_trip_and_preserve_ownership() {
    let pool = common::test_db().await;
    seed_list(&pool).await;
    seed_storage_primitive_dependencies(&pool).await;

    let (checkpoint_id, other_checkpoint_id) = assert_checkpoint_storage(&pool).await;

    assert_import_lifecycle_storage(&pool, checkpoint_id, other_checkpoint_id).await;

    assert_lifecycle_job_storage(&pool).await;

    assert_tombstone_observation_storage(&pool).await;
    assert_tombstone_confirmation_storage(&pool).await;

    assert_artifact_storage(&pool).await;
    assert_tool_maintenance_storage(&pool).await;
}

async fn seed_list(pool: &sqlx::SqlitePool) {
    pool.execute(
        r#"INSERT INTO "AddonInstallation" (
              "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
              "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt"
           ) VALUES (
              'org.example.lifecycle', '1.0.0', 1, 'package', 'content', '{}',
              'addons/lifecycle', '2026-09-04T00:00:00Z', '2026-09-04T00:00:00Z'
           )"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "AddonMediaList" (
              "Id", "AddonId", "ProviderId", "SourceUrl", "Name"
           ) VALUES (
              1, 'org.example.lifecycle', 'fixture-list',
              'https://example.invalid/list', 'Fixture list'
           )"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "AddonMediaListItem" (
              "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind",
              "Rank", "Title"
           ) VALUES (10, 1, 'item-10', 'Item 10', 4, 0, 'Item 10')"#,
    )
    .await
    .unwrap();
}

fn fixed_time() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 4, 12, 0, 0).unwrap()
}

fn lifecycle_job_command(
    operation_id: i64,
    list_id: i64,
    generation: i32,
    kind: AddonMediaListLifecycleJobKind,
    saved_draft_revision: i32,
    saved_published_revision: i32,
) -> CreateAddonMediaListLifecycleJob {
    CreateAddonMediaListLifecycleJob {
        operation_id,
        list_id,
        generation,
        kind,
        saved_draft_revision,
        saved_published_revision,
        created_at: fixed_time(),
    }
}

fn precise_time() -> chrono::DateTime<Utc> {
    fixed_time() + chrono::Duration::nanoseconds(987_654_321)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

fn schema_definition(enabled_description: &str) -> String {
    serde_json::json!({
        "id": "media-list.options.v1",
        "options": [
            {
                "key": "enabled",
                "label": {"en-US": "Enabled"},
                "description": {"en-US": enabled_description},
                "scope": "both",
                "operation_targets": ["playback"],
                "applicability": "all",
                "kind": "boolean",
                "default": true
            },
            {
                "key": "profile",
                "label": {"en-US": "Profile"},
                "description": {"en-US": "Choose a fixture profile."},
                "scope": "both",
                "operation_targets": ["playback"],
                "applicability": "all",
                "kind": "enum",
                "default": "default",
                "choices": [
                    {"value": "default", "label": {"en-US": "Default"}},
                    {"value": "list", "label": {"en-US": "List"}},
                    {"value": "item", "label": {"en-US": "Item"}}
                ]
            }
        ]
    })
    .to_string()
}
