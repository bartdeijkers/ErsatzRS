use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use ersatzrs_core::repositories::{
    AddonMediaListLifecycleJob, AddonMediaListPublicationEnqueue, AddonMediaListPublicationEvent,
    AddonMediaListPublicationFailureCause, AddonMediaListPublicationHistory,
    AddonMediaListPublicationHistoryJob, AddonMediaListPublicationJob,
    AddonMediaListPublicationStatus, AddonMediaListReconcileEnqueue, AddonMediaListResetEnqueue,
    ManagedLibraryOperation, ManagedLibraryOperationClaim, ManagedLibraryOperationRepository,
    ManagedLibraryScanSpec, RepositoryError, Result,
};
use ersatzrs_domain::{
    AddonMediaListLifecycleJobKind, AddonMediaListLifecyclePhase,
    AddonMediaListPublicationEnqueueTrigger, AddonMediaListPublicationErrorCategory,
    AddonMediaListPublicationEventKind, AddonMediaListPublicationFilesystemTransition,
    AddonMediaListPublicationOutcome, AddonMediaListPublicationPhase,
    AddonMediaListPublicationRetryClass, AddonMediaListPublicationState,
    AddonMediaListRecoveryAction, EntityId, ManagedLibraryOperationKind,
    ManagedLibraryOperationState, MediaSourceKind,
};
use sqlx::Row as _;
use uuid::Uuid;

use crate::db::{inserted_id, DbPool};
use crate::repositories::SqlxResultExt as _;

use super::addon_media_list_lifecycle::load_valid_revision_snapshot_in_tx;
use crate::repositories::common::{
    decode_bool_column, decode_datetime, decode_enum_column, decode_optional_datetime,
    decode_optional_enum_column, encode_datetime, encode_enum,
};

#[allow(clippy::too_many_lines)] // One transaction owns admission and its causal history.
pub(crate) async fn enqueue_publication_in_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    list_id: EntityId,
    library_id: EntityId,
    saved_revision: i32,
    expected_next_published_revision: i32,
    trigger: AddonMediaListPublicationEnqueueTrigger,
) -> Result<AddonMediaListPublicationEnqueue> {
    let now = encode_datetime(Utc::now());
    if let Some(row) = active_publication_row(tx, list_id).await? {
        let (job, operation) = decode_publication_row(&row)?;
        return Ok(AddonMediaListPublicationEnqueue {
            job,
            operation,
            existing: true,
        });
    }
    let row = sqlx::query(
        r#"SELECT "DraftRevision", "PublishedRevision"
               FROM "AddonMediaList" WHERE "Id" = ?"#,
    )
    .bind(list_id)
    .fetch_optional(&mut **tx)
    .await
    .repository()?
    .ok_or(RepositoryError::NotFound {
        kind: "add-on media list",
        id: list_id,
    })?;
    let current_saved: i32 = row.try_get("DraftRevision").repository()?;
    let current_published: i32 = row.try_get("PublishedRevision").repository()?;
    if current_saved != saved_revision {
        return Err(RepositoryError::Conflict(
            "the saved add-on media-list revision changed before publication was queued".to_owned(),
        ));
    }
    if let Some(row) =
        completed_publication_row(tx, list_id, saved_revision, current_published).await?
    {
        let (job, operation) = decode_publication_row(&row)?;
        return Ok(AddonMediaListPublicationEnqueue {
            job,
            operation,
            existing: true,
        });
    }
    if current_published.checked_add(1) != Some(expected_next_published_revision) {
        return Err(RepositoryError::Conflict(
            "the published add-on media-list revision changed before publication was queued"
                .to_owned(),
        ));
    }
    let target_published_revision =
        next_publication_revision_tx(tx, list_id, current_published).await?;
    let generation = sqlx::query_scalar::<_, i32>(
        r#"SELECT COALESCE(MAX("Generation"), 0) + 1
               FROM "AddonMediaListPublicationJob" WHERE "AddonMediaListId" = ?"#,
    )
    .bind(list_id)
    .fetch_one(&mut **tx)
    .await
    .repository()?;
    let owner_token = Uuid::new_v4().to_string();
    let operation_result = sqlx::query(
        r#"INSERT INTO "ManagedLibraryOperation" (
                   "LibraryId", "OwnerToken", "Kind", "State", "CreatedAt", "UpdatedAt"
               ) VALUES (?, ?, ?, ?, ?, ?)"#,
    )
    .bind(library_id)
    .bind(&owner_token)
    .bind(encode_enum(ManagedLibraryOperationKind::Publication))
    .bind(encode_enum(ManagedLibraryOperationState::Queued))
    .bind(&now)
    .bind(&now)
    .execute(&mut **tx)
    .await
    .repository()?;
    let operation_id = inserted_id(tx, operation_result).await.repository()?;
    let job_result = sqlx::query(
        r#"INSERT INTO "AddonMediaListPublicationJob" (
                   "OperationId", "AddonMediaListId", "Generation", "SavedRevision",
                   "TargetPublishedRevision", "Phase", "CreatedAt", "UpdatedAt"
               ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(operation_id)
    .bind(list_id)
    .bind(generation)
    .bind(saved_revision)
    .bind(target_published_revision)
    .bind(encode_enum(AddonMediaListPublicationPhase::AwaitingLease))
    .bind(&now)
    .bind(&now)
    .execute(&mut **tx)
    .await
    .repository()?;
    let job_id = inserted_id(tx, job_result).await.repository()?;
    append_publication_event_for_job_tx(
        tx,
        job_id,
        "enqueued",
        PublicationEventWrite {
            kind: AddonMediaListPublicationEventKind::Enqueued,
            enqueue_trigger: Some(trigger),
            phase: Some(AddonMediaListPublicationPhase::AwaitingLease),
            occurred_at: &now,
            ..PublicationEventWrite::default()
        },
    )
    .await?;
    if let Some(owner_operation_id) =
        competing_lease_operation_tx(tx, library_id, operation_id, &now).await?
    {
        append_publication_event_for_job_tx(
            tx,
            job_id,
            "lease_wait:0",
            PublicationEventWrite {
                kind: AddonMediaListPublicationEventKind::LeaseWaitStarted,
                phase: Some(AddonMediaListPublicationPhase::AwaitingLease),
                attempt: Some(0),
                related_operation_id: Some(owner_operation_id),
                occurred_at: &now,
                ..PublicationEventWrite::default()
            },
        )
        .await?;
    }
    prune_publication_history_tx(tx, &now).await?;
    Ok(AddonMediaListPublicationEnqueue {
        job: AddonMediaListPublicationJob {
            id: job_id,
            operation_id,
            list_id,
            generation,
            saved_revision,
            target_published_revision,
            source_published_revision: None,
            restore_materialized_at: None,
            phase: AddonMediaListPublicationPhase::AwaitingLease,
            child_scan_operation_id: None,
            original_failure_code: None,
            original_failure_diagnostic: None,
            recovery_diagnostic: None,
            outcome: None,
        },
        operation: ManagedLibraryOperation {
            id: operation_id,
            library_id,
            parent_operation_id: None,
            owner_token,
            kind: ManagedLibraryOperationKind::Publication,
            state: ManagedLibraryOperationState::Queued,
            scan: None,
            attempt_count: 0,
            next_eligible_at: None,
        },
        existing: false,
    })
}

const LEASE_SECONDS: i64 = 90;
const PUBLICATION_HISTORY_LIMIT: i32 = 256;
const PUBLICATION_TERMINAL_HISTORY_DAYS: i64 = 30;

/// `SQLite` implementation of the AMM-J managed-library operation boundary.
#[derive(Clone)]
pub struct SqliteManagedLibraryOperationRepository(pub DbPool);

#[async_trait]
#[allow(clippy::too_many_lines)]
impl ManagedLibraryOperationRepository for SqliteManagedLibraryOperationRepository {
    async fn enqueue_scan(
        &self,
        kind: ManagedLibraryOperationKind,
        library_id: EntityId,
        scan: ManagedLibraryScanSpec,
        coalesce: bool,
    ) -> Result<Option<ManagedLibraryOperation>> {
        let now = encode_datetime(Utc::now());
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        if coalesce {
            let active = sqlx::query_scalar::<_, i64>(
                r#"SELECT COUNT(*) FROM "ManagedLibraryOperation"
                   WHERE "LibraryId" = ? AND "ParentOperationId" IS NULL
                     AND "Kind" IN (?, ?, ?, ?)
                     AND "State" IN (?, ?, ?)"#,
            )
            .bind(library_id)
            .bind(encode_enum(ManagedLibraryOperationKind::ManualScan))
            .bind(encode_enum(ManagedLibraryOperationKind::ScheduledScan))
            .bind(encode_enum(ManagedLibraryOperationKind::StartupReplayScan))
            .bind(encode_enum(ManagedLibraryOperationKind::ProviderRefresh))
            .bind(encode_enum(ManagedLibraryOperationState::Queued))
            .bind(encode_enum(ManagedLibraryOperationState::Running))
            .bind(encode_enum(ManagedLibraryOperationState::WaitingToRetry))
            .fetch_one(&mut *tx)
            .await
            .repository()?;
            if active > 0 {
                tx.commit().await.repository()?;
                return Ok(None);
            }
        }
        let owner_token = Uuid::new_v4().to_string();
        let result = sqlx::query(
            r#"INSERT INTO "ManagedLibraryOperation" (
                   "LibraryId", "OwnerToken", "Kind", "State", "Provider", "ShowId",
                   "DeepScan", "CreatedAt", "UpdatedAt"
               ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(library_id)
        .bind(&owner_token)
        .bind(encode_enum(kind))
        .bind(encode_enum(ManagedLibraryOperationState::Queued))
        .bind(encode_enum(scan.provider))
        .bind(scan.show_id)
        .bind(scan.deep_scan)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .repository()?;
        let id = inserted_id(&mut tx, result).await.repository()?;
        tx.commit().await.repository()?;
        Ok(Some(ManagedLibraryOperation {
            id,
            library_id,
            parent_operation_id: None,
            owner_token,
            kind,
            state: ManagedLibraryOperationState::Queued,
            scan: Some(scan),
            attempt_count: 0,
            next_eligible_at: None,
        }))
    }

    async fn enqueue_publication(
        &self,
        list_id: EntityId,
        library_id: EntityId,
        saved_revision: i32,
        expected_next_published_revision: i32,
    ) -> Result<AddonMediaListPublicationEnqueue> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let result = enqueue_publication_in_tx(
            &mut tx,
            list_id,
            library_id,
            saved_revision,
            expected_next_published_revision,
            AddonMediaListPublicationEnqueueTrigger::Operator,
        )
        .await?;
        tx.commit().await.repository()?;
        Ok(result)
    }

    async fn enqueue_revision_restore(
        &self,
        list_id: EntityId,
        library_id: EntityId,
        source_published_revision: i32,
        expected_draft_revision: i32,
        expected_published_revision: i32,
    ) -> Result<AddonMediaListPublicationEnqueue> {
        let now = encode_datetime(Utc::now());
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        if let Some(row) = active_publication_row(&mut tx, list_id).await? {
            let (job, operation) = decode_publication_row(&row)?;
            if job.source_published_revision == Some(source_published_revision)
                && expected_draft_revision.checked_add(1) == Some(job.saved_revision)
            {
                tx.commit().await.repository()?;
                return Ok(AddonMediaListPublicationEnqueue {
                    job,
                    operation,
                    existing: true,
                });
            }
            return Err(RepositoryError::Conflict(
                "another add-on media-list publication is already active".to_owned(),
            ));
        }

        let row = sqlx::query(
            r#"SELECT "list"."DraftRevision", "list"."PublishedRevision",
                      "binding"."LibraryId"
               FROM "AddonMediaList" AS "list"
               JOIN "AddonMediaListLibrary" AS "binding"
                 ON "binding"."AddonMediaListId" = "list"."Id"
               WHERE "list"."Id" = ?"#,
        )
        .bind(list_id)
        .fetch_optional(&mut *tx)
        .await
        .repository()?
        .ok_or(RepositoryError::NotFound {
            kind: "add-on media list",
            id: list_id,
        })?;
        let current_draft: i32 = row.try_get("DraftRevision").repository()?;
        let current_published: i32 = row.try_get("PublishedRevision").repository()?;
        let current_library: EntityId = row.try_get("LibraryId").repository()?;
        if current_library != library_id {
            return Err(RepositoryError::Conflict(
                "the managed add-on media-list library changed before restore was queued"
                    .to_owned(),
            ));
        }
        if current_draft != expected_draft_revision {
            return Err(RepositoryError::Conflict(
                "the saved add-on media-list revision changed before restore was queued".to_owned(),
            ));
        }
        if current_published != expected_published_revision {
            return Err(RepositoryError::Conflict(
                "the published add-on media-list revision changed before restore was queued"
                    .to_owned(),
            ));
        }
        if source_published_revision >= current_published {
            return Err(RepositoryError::Conflict(
                "only an older add-on media-list revision can be restored".to_owned(),
            ));
        }

        let retained =
            match load_valid_revision_snapshot_in_tx(&mut tx, list_id, source_published_revision)
                .await
            {
                Ok(Some(_)) => true,
                Ok(None) | Err(RepositoryError::Decode { .. }) => false,
                Err(error) => return Err(error),
            };
        if !retained {
            return Err(RepositoryError::Conflict(
                "the selected add-on media-list revision is not a valid retained snapshot"
                    .to_owned(),
            ));
        }

        let generation = sqlx::query_scalar::<_, i32>(
            r#"SELECT COALESCE(MAX("Generation"), 0) + 1
               FROM "AddonMediaListPublicationJob" WHERE "AddonMediaListId" = ?"#,
        )
        .bind(list_id)
        .fetch_one(&mut *tx)
        .await
        .repository()?;
        let owner_token = Uuid::new_v4().to_string();
        let operation_result = sqlx::query(
            r#"INSERT INTO "ManagedLibraryOperation" (
                   "LibraryId", "OwnerToken", "Kind", "State", "CreatedAt", "UpdatedAt"
               ) VALUES (?, ?, ?, ?, ?, ?)"#,
        )
        .bind(library_id)
        .bind(&owner_token)
        .bind(encode_enum(ManagedLibraryOperationKind::Publication))
        .bind(encode_enum(ManagedLibraryOperationState::Queued))
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .repository()?;
        let operation_id = inserted_id(&mut tx, operation_result).await.repository()?;
        let saved_revision = expected_draft_revision.checked_add(1).ok_or_else(|| {
            RepositoryError::Conflict("the saved add-on media-list revision is exhausted".into())
        })?;
        let target_published_revision =
            next_publication_revision_tx(&mut tx, list_id, current_published).await?;
        let job_result = sqlx::query(
            r#"INSERT INTO "AddonMediaListPublicationJob" (
                   "OperationId", "AddonMediaListId", "Generation", "SavedRevision",
                   "TargetPublishedRevision", "SourcePublishedRevision", "Phase",
                   "CreatedAt", "UpdatedAt"
               ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(operation_id)
        .bind(list_id)
        .bind(generation)
        .bind(saved_revision)
        .bind(target_published_revision)
        .bind(source_published_revision)
        .bind(encode_enum(AddonMediaListPublicationPhase::AwaitingLease))
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .repository()?;
        let job_id = inserted_id(&mut tx, job_result).await.repository()?;
        append_publication_event_for_job_tx(
            &mut tx,
            job_id,
            "enqueued",
            PublicationEventWrite {
                kind: AddonMediaListPublicationEventKind::Enqueued,
                enqueue_trigger: Some(AddonMediaListPublicationEnqueueTrigger::RestoreRevision),
                phase: Some(AddonMediaListPublicationPhase::AwaitingLease),
                occurred_at: &now,
                ..PublicationEventWrite::default()
            },
        )
        .await?;
        if let Some(owner_operation_id) =
            competing_lease_operation_tx(&mut tx, library_id, operation_id, &now).await?
        {
            append_publication_event_for_job_tx(
                &mut tx,
                job_id,
                "lease_wait:0",
                PublicationEventWrite {
                    kind: AddonMediaListPublicationEventKind::LeaseWaitStarted,
                    phase: Some(AddonMediaListPublicationPhase::AwaitingLease),
                    attempt: Some(0),
                    related_operation_id: Some(owner_operation_id),
                    occurred_at: &now,
                    ..PublicationEventWrite::default()
                },
            )
            .await?;
        }
        prune_publication_history_tx(&mut tx, &now).await?;
        tx.commit().await.repository()?;
        Ok(AddonMediaListPublicationEnqueue {
            job: AddonMediaListPublicationJob {
                id: job_id,
                operation_id,
                list_id,
                generation,
                saved_revision,
                target_published_revision,
                source_published_revision: Some(source_published_revision),
                restore_materialized_at: None,
                phase: AddonMediaListPublicationPhase::AwaitingLease,
                child_scan_operation_id: None,
                original_failure_code: None,
                original_failure_diagnostic: None,
                recovery_diagnostic: None,
                outcome: None,
            },
            operation: ManagedLibraryOperation {
                id: operation_id,
                library_id,
                parent_operation_id: None,
                owner_token,
                kind: ManagedLibraryOperationKind::Publication,
                state: ManagedLibraryOperationState::Queued,
                scan: None,
                attempt_count: 0,
                next_eligible_at: None,
            },
            existing: false,
        })
    }

    async fn enqueue_reset_reload(
        &self,
        list_id: EntityId,
        confirmed_name: &str,
    ) -> Result<AddonMediaListResetEnqueue> {
        enqueue_reset_reload(&self.0, list_id, confirmed_name, None).await
    }

    async fn enqueue_editor_reset_reload(
        &self,
        list_id: EntityId,
        confirmed_name: &str,
        expected_revision: i32,
    ) -> Result<AddonMediaListResetEnqueue> {
        enqueue_reset_reload(&self.0, list_id, confirmed_name, Some(expected_revision)).await
    }

    async fn enqueue_reconcile(&self, list_id: EntityId) -> Result<AddonMediaListReconcileEnqueue> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let result = enqueue_reconcile_tx(&mut tx, list_id).await?;
        tx.commit().await.repository()?;
        Ok(result)
    }

    async fn operation(&self, operation_id: EntityId) -> Result<Option<ManagedLibraryOperation>> {
        let row = sqlx::query(r#"SELECT * FROM "ManagedLibraryOperation" WHERE "Id" = ?"#)
            .bind(operation_id)
            .fetch_optional(&self.0)
            .await
            .repository()?;
        row.as_ref().map(decode_operation_row).transpose()
    }

    async fn active_scan_operation(
        &self,
        library_id: EntityId,
    ) -> Result<Option<ManagedLibraryOperation>> {
        let row = sqlx::query(
            r#"SELECT * FROM "ManagedLibraryOperation"
               WHERE "LibraryId" = ? AND "ParentOperationId" IS NULL
                 AND "Kind" IN (?, ?, ?, ?)
                 AND "State" IN (?, ?, ?)
               ORDER BY "CreatedAt", "Id"
               LIMIT 1"#,
        )
        .bind(library_id)
        .bind(encode_enum(ManagedLibraryOperationKind::ManualScan))
        .bind(encode_enum(ManagedLibraryOperationKind::ScheduledScan))
        .bind(encode_enum(ManagedLibraryOperationKind::StartupReplayScan))
        .bind(encode_enum(ManagedLibraryOperationKind::ProviderRefresh))
        .bind(encode_enum(ManagedLibraryOperationState::Queued))
        .bind(encode_enum(ManagedLibraryOperationState::Running))
        .bind(encode_enum(ManagedLibraryOperationState::WaitingToRetry))
        .fetch_optional(&self.0)
        .await
        .repository()?;
        row.as_ref().map(decode_operation_row).transpose()
    }

    async fn claim_next(&self, now: DateTime<Utc>) -> Result<Option<ManagedLibraryOperationClaim>> {
        let now_text = encode_datetime(now);
        let lease_expires = encode_datetime(now + Duration::seconds(LEASE_SECONDS));
        let lease_owner = Uuid::new_v4().to_string();
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        sqlx::query(r#"DELETE FROM "ManagedLibraryLease" WHERE "LeaseExpiresAt" <= ?"#)
            .bind(&now_text)
            .execute(&mut *tx)
            .await
            .repository()?;
        let row = sqlx::query(
            r#"SELECT "operation".*
               FROM "ManagedLibraryOperation" AS "operation"
               LEFT JOIN "ManagedLibraryLease" AS "lease"
                 ON "lease"."LibraryId" = "operation"."LibraryId"
               WHERE "operation"."ParentOperationId" IS NULL
                 AND "operation"."State" IN (?, ?, ?)
                 AND ("operation"."NextEligibleAt" IS NULL OR "operation"."NextEligibleAt" <= ?)
                 AND ("operation"."LeaseExpiresAt" IS NULL OR "operation"."LeaseExpiresAt" <= ?)
                 AND "lease"."LibraryId" IS NULL
               ORDER BY CASE WHEN "operation"."State" = ? THEN 0 ELSE 1 END,
                        "operation"."CreatedAt", "operation"."Id"
               LIMIT 1"#,
        )
        .bind(encode_enum(ManagedLibraryOperationState::Queued))
        .bind(encode_enum(ManagedLibraryOperationState::Running))
        .bind(encode_enum(ManagedLibraryOperationState::WaitingToRetry))
        .bind(&now_text)
        .bind(&now_text)
        .bind(encode_enum(ManagedLibraryOperationState::Running))
        .fetch_optional(&mut *tx)
        .await
        .repository()?;
        let Some(row) = row else {
            tx.commit().await.repository()?;
            return Ok(None);
        };
        let mut operation = decode_operation_row(&row)?;
        let inserted = sqlx::query(
            r#"INSERT INTO "ManagedLibraryLease" (
                   "LibraryId", "OperationId", "OwnerToken", "LeaseOwner",
                   "LeaseExpiresAt", "HeartbeatAt", "UpdatedAt"
               ) VALUES (?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(operation.library_id)
        .bind(operation.id)
        .bind(&operation.owner_token)
        .bind(&lease_owner)
        .bind(&lease_expires)
        .bind(&now_text)
        .bind(&now_text)
        .execute(&mut *tx)
        .await
        .repository()?;
        if inserted.rows_affected() != 1 {
            tx.rollback().await.repository()?;
            return Ok(None);
        }
        let updated = sqlx::query(
            r#"UPDATE "ManagedLibraryOperation"
               SET "State" = ?, "AttemptCount" = "AttemptCount" + 1,
                   "LeaseOwner" = ?, "LeaseExpiresAt" = ?, "HeartbeatAt" = ?,
                   "NextEligibleAt" = NULL, "UpdatedAt" = ?
               WHERE "Id" = ?
                 AND "State" IN (?, ?, ?)"#,
        )
        .bind(encode_enum(ManagedLibraryOperationState::Running))
        .bind(&lease_owner)
        .bind(&lease_expires)
        .bind(&now_text)
        .bind(&now_text)
        .bind(operation.id)
        .bind(encode_enum(ManagedLibraryOperationState::Queued))
        .bind(encode_enum(ManagedLibraryOperationState::Running))
        .bind(encode_enum(ManagedLibraryOperationState::WaitingToRetry))
        .execute(&mut *tx)
        .await
        .repository()?;
        if updated.rows_affected() != 1 {
            tx.rollback().await.repository()?;
            return Ok(None);
        }
        let attempt = operation.attempt_count.saturating_add(1);
        append_publication_event_for_operation_tx(
            &mut tx,
            operation.id,
            &format!("lease:{attempt}"),
            PublicationEventWrite {
                kind: if operation.attempt_count == 0 {
                    AddonMediaListPublicationEventKind::LeaseAcquired
                } else {
                    AddonMediaListPublicationEventKind::LeaseReclaimed
                },
                attempt: Some(attempt),
                occurred_at: &now_text,
                ..PublicationEventWrite::default()
            },
        )
        .await?;
        record_waiting_publications_for_owner_tx(
            &mut tx,
            operation.library_id,
            operation.id,
            &now_text,
        )
        .await?;
        tx.commit().await.repository()?;
        operation.state = ManagedLibraryOperationState::Running;
        operation.attempt_count = operation.attempt_count.saturating_add(1);
        operation.next_eligible_at = None;
        Ok(Some(ManagedLibraryOperationClaim {
            operation,
            lease_owner,
        }))
    }

    async fn claim_operation(
        &self,
        operation_id: EntityId,
        now: DateTime<Utc>,
    ) -> Result<Option<ManagedLibraryOperationClaim>> {
        let now_text = encode_datetime(now);
        let lease_expires = encode_datetime(now + Duration::seconds(LEASE_SECONDS));
        let lease_owner = Uuid::new_v4().to_string();
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        sqlx::query(r#"DELETE FROM "ManagedLibraryLease" WHERE "LeaseExpiresAt" <= ?"#)
            .bind(&now_text)
            .execute(&mut *tx)
            .await
            .repository()?;
        let row = sqlx::query(
            r#"SELECT "operation".*
               FROM "ManagedLibraryOperation" AS "operation"
               LEFT JOIN "ManagedLibraryLease" AS "lease"
                 ON "lease"."LibraryId" = "operation"."LibraryId"
               WHERE "operation"."Id" = ?
                 AND "operation"."ParentOperationId" IS NULL
                 AND "operation"."State" IN (?, ?, ?)
                 AND ("operation"."NextEligibleAt" IS NULL OR "operation"."NextEligibleAt" <= ?)
                 AND ("operation"."LeaseExpiresAt" IS NULL OR "operation"."LeaseExpiresAt" <= ?)
                 AND "lease"."LibraryId" IS NULL"#,
        )
        .bind(operation_id)
        .bind(encode_enum(ManagedLibraryOperationState::Queued))
        .bind(encode_enum(ManagedLibraryOperationState::Running))
        .bind(encode_enum(ManagedLibraryOperationState::WaitingToRetry))
        .bind(&now_text)
        .bind(&now_text)
        .fetch_optional(&mut *tx)
        .await
        .repository()?;
        let Some(row) = row else {
            tx.commit().await.repository()?;
            return Ok(None);
        };
        let mut operation = decode_operation_row(&row)?;
        sqlx::query(
            r#"INSERT INTO "ManagedLibraryLease" (
                   "LibraryId", "OperationId", "OwnerToken", "LeaseOwner",
                   "LeaseExpiresAt", "HeartbeatAt", "UpdatedAt"
               ) VALUES (?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(operation.library_id)
        .bind(operation.id)
        .bind(&operation.owner_token)
        .bind(&lease_owner)
        .bind(&lease_expires)
        .bind(&now_text)
        .bind(&now_text)
        .execute(&mut *tx)
        .await
        .repository()?;
        let updated = sqlx::query(
            r#"UPDATE "ManagedLibraryOperation"
               SET "State" = ?, "AttemptCount" = "AttemptCount" + 1,
                   "LeaseOwner" = ?, "LeaseExpiresAt" = ?, "HeartbeatAt" = ?,
                   "NextEligibleAt" = NULL, "UpdatedAt" = ?
               WHERE "Id" = ? AND "State" IN (?, ?, ?)"#,
        )
        .bind(encode_enum(ManagedLibraryOperationState::Running))
        .bind(&lease_owner)
        .bind(&lease_expires)
        .bind(&now_text)
        .bind(&now_text)
        .bind(operation.id)
        .bind(encode_enum(ManagedLibraryOperationState::Queued))
        .bind(encode_enum(ManagedLibraryOperationState::Running))
        .bind(encode_enum(ManagedLibraryOperationState::WaitingToRetry))
        .execute(&mut *tx)
        .await
        .repository()?;
        if updated.rows_affected() != 1 {
            tx.rollback().await.repository()?;
            return Ok(None);
        }
        let attempt = operation.attempt_count.saturating_add(1);
        append_publication_event_for_operation_tx(
            &mut tx,
            operation.id,
            &format!("lease:{attempt}"),
            PublicationEventWrite {
                kind: if operation.attempt_count == 0 {
                    AddonMediaListPublicationEventKind::LeaseAcquired
                } else {
                    AddonMediaListPublicationEventKind::LeaseReclaimed
                },
                attempt: Some(attempt),
                occurred_at: &now_text,
                ..PublicationEventWrite::default()
            },
        )
        .await?;
        record_waiting_publications_for_owner_tx(
            &mut tx,
            operation.library_id,
            operation.id,
            &now_text,
        )
        .await?;
        tx.commit().await.repository()?;
        operation.state = ManagedLibraryOperationState::Running;
        operation.attempt_count = operation.attempt_count.saturating_add(1);
        operation.next_eligible_at = None;
        Ok(Some(ManagedLibraryOperationClaim {
            operation,
            lease_owner,
        }))
    }

    async fn heartbeat(
        &self,
        claim: &ManagedLibraryOperationClaim,
        now: DateTime<Utc>,
    ) -> Result<bool> {
        let now_text = encode_datetime(now);
        let expires = encode_datetime(now + Duration::seconds(LEASE_SECONDS));
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let lease = sqlx::query(
            r#"UPDATE "ManagedLibraryLease"
               SET "LeaseExpiresAt" = ?, "HeartbeatAt" = ?, "UpdatedAt" = ?
               WHERE "OperationId" = ? AND "OwnerToken" = ? AND "LeaseOwner" = ?
                 AND "LeaseExpiresAt" > ?"#,
        )
        .bind(&expires)
        .bind(&now_text)
        .bind(&now_text)
        .bind(claim.operation.id)
        .bind(&claim.operation.owner_token)
        .bind(&claim.lease_owner)
        .bind(&now_text)
        .execute(&mut *tx)
        .await
        .repository()?;
        if lease.rows_affected() != 1 {
            tx.rollback().await.repository()?;
            return Ok(false);
        }
        sqlx::query(
            r#"UPDATE "ManagedLibraryOperation"
               SET "LeaseExpiresAt" = ?, "HeartbeatAt" = ?, "UpdatedAt" = ?
               WHERE "Id" = ? AND "OwnerToken" = ? AND "LeaseOwner" = ?"#,
        )
        .bind(&expires)
        .bind(&now_text)
        .bind(&now_text)
        .bind(claim.operation.id)
        .bind(&claim.operation.owner_token)
        .bind(&claim.lease_owner)
        .execute(&mut *tx)
        .await
        .repository()?;
        tx.commit().await.repository()?;
        Ok(true)
    }

    async fn publication_job(
        &self,
        operation_id: EntityId,
    ) -> Result<Option<AddonMediaListPublicationJob>> {
        let row = publication_select()
            .bind(operation_id)
            .fetch_optional(&self.0)
            .await
            .repository()?;
        row.map(|row| decode_publication_row(&row).map(|(job, _)| job))
            .transpose()
    }

    async fn reset_reload_job(
        &self,
        operation_id: EntityId,
    ) -> Result<Option<AddonMediaListLifecycleJob>> {
        let row =
            sqlx::query(r#"SELECT * FROM "AddonMediaListLifecycleJob" WHERE "OperationId" = ?"#)
                .bind(operation_id)
                .fetch_optional(&self.0)
                .await
                .repository()?;
        row.as_ref().map(decode_reset_job_row).transpose()
    }

    async fn reconcile_job(
        &self,
        operation_id: EntityId,
    ) -> Result<Option<AddonMediaListLifecycleJob>> {
        let row = sqlx::query(
            r#"SELECT * FROM "AddonMediaListLifecycleJob"
               WHERE "OperationId" = ? AND "Kind" = ?"#,
        )
        .bind(operation_id)
        .bind(encode_enum(AddonMediaListLifecycleJobKind::Reconcile))
        .fetch_optional(&self.0)
        .await
        .repository()?;
        row.as_ref().map(decode_reset_job_row).transpose()
    }

    async fn transition_reset_reload(
        &self,
        claim: &ManagedLibraryOperationClaim,
        expected: AddonMediaListLifecyclePhase,
        next: AddonMediaListLifecyclePhase,
    ) -> Result<bool> {
        let now = encode_datetime(Utc::now());
        let result = sqlx::query(
            r#"UPDATE "AddonMediaListLifecycleJob" SET "Phase" = ?, "UpdatedAt" = ?
               WHERE "OperationId" = ? AND "Kind" = ? AND "Phase" = ?
                 AND EXISTS (
                   SELECT 1 FROM "ManagedLibraryLease"
                   WHERE "OperationId" = ? AND "OwnerToken" = ? AND "LeaseOwner" = ?
                     AND "LeaseExpiresAt" > ?
                 )"#,
        )
        .bind(encode_enum(next))
        .bind(&now)
        .bind(claim.operation.id)
        .bind(encode_enum(AddonMediaListLifecycleJobKind::ResetReload))
        .bind(encode_enum(expected))
        .bind(claim.operation.id)
        .bind(&claim.operation.owner_token)
        .bind(&claim.lease_owner)
        .bind(&now)
        .execute(&self.0)
        .await
        .repository()?;
        Ok(result.rows_affected() == 1)
    }

    async fn transition_reconcile(
        &self,
        claim: &ManagedLibraryOperationClaim,
        expected: AddonMediaListLifecyclePhase,
        next: AddonMediaListLifecyclePhase,
    ) -> Result<bool> {
        let now = encode_datetime(Utc::now());
        let result = sqlx::query(
            r#"UPDATE "AddonMediaListLifecycleJob" SET "Phase" = ?, "UpdatedAt" = ?
               WHERE "OperationId" = ? AND "Kind" = ? AND "Phase" = ?
                 AND EXISTS (
                   SELECT 1 FROM "ManagedLibraryLease"
                   WHERE "OperationId" = ? AND "OwnerToken" = ? AND "LeaseOwner" = ?
                     AND "LeaseExpiresAt" > ?
                 )"#,
        )
        .bind(encode_enum(next))
        .bind(&now)
        .bind(claim.operation.id)
        .bind(encode_enum(AddonMediaListLifecycleJobKind::Reconcile))
        .bind(encode_enum(expected))
        .bind(claim.operation.id)
        .bind(&claim.operation.owner_token)
        .bind(&claim.lease_owner)
        .bind(&now)
        .execute(&self.0)
        .await
        .repository()?;
        Ok(result.rows_affected() == 1)
    }

    async fn apply_reset_reload_phase(
        &self,
        claim: &ManagedLibraryOperationClaim,
        job: &AddonMediaListLifecycleJob,
        phase: AddonMediaListLifecyclePhase,
    ) -> Result<()> {
        crate::repositories::addon_media_list_reset::apply_phase(&self.0, claim, job, phase).await
    }

    async fn ensure_reset_reload_child_scan(
        &self,
        claim: &ManagedLibraryOperationClaim,
        list_id: EntityId,
    ) -> Result<ManagedLibraryOperation> {
        ensure_reset_child_scan(&self.0, claim, list_id).await
    }

    async fn queue_reset_reload_import(
        &self,
        claim: &ManagedLibraryOperationClaim,
        job: &AddonMediaListLifecycleJob,
    ) -> Result<EntityId> {
        crate::repositories::addon_media_list_reset::queue_initial_import(&self.0, claim, job).await
    }

    async fn complete_reset_reload(
        &self,
        claim: &ManagedLibraryOperationClaim,
        candidate_import_job_id: EntityId,
    ) -> Result<bool> {
        let now = encode_datetime(Utc::now());
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        if !claim_is_current(&mut tx, claim).await? {
            return Ok(false);
        }
        let updated = sqlx::query(
            r#"UPDATE "AddonMediaListLifecycleJob"
               SET "CandidateImportJobId" = ?, "Phase" = ?, "UpdatedAt" = ?, "CompletedAt" = ?
               WHERE "OperationId" = ? AND "Kind" = ? AND "Phase" = ?
                 AND EXISTS (
                   SELECT 1 FROM "AddonMediaListImportJob"
                   WHERE "Id" = ? AND "AddonMediaListId" = "AddonMediaListLifecycleJob"."AddonMediaListId"
                     AND "Generation" = 1 AND "Purpose" = ? AND "Mode" = ?
                 )"#,
        )
        .bind(candidate_import_job_id)
        .bind(encode_enum(AddonMediaListLifecyclePhase::Complete))
        .bind(&now)
        .bind(&now)
        .bind(claim.operation.id)
        .bind(encode_enum(AddonMediaListLifecycleJobKind::ResetReload))
        .bind(encode_enum(AddonMediaListLifecyclePhase::QueueingInitialImport))
        .bind(candidate_import_job_id)
        .bind(encode_enum(ersatzrs_domain::AddonMediaListImportPurpose::ResetReload))
        .bind(encode_enum(ersatzrs_domain::AddonMediaListImportMode::Full))
        .execute(&mut *tx)
        .await
        .repository()?;
        if updated.rows_affected() != 1 {
            tx.rollback().await.repository()?;
            return Ok(false);
        }
        finish_operation_tx(
            &mut tx,
            claim,
            ManagedLibraryOperationState::Completed,
            None,
            &now,
        )
        .await?;
        tx.commit().await.repository()?;
        Ok(true)
    }

    async fn queue_reconcile_import(
        &self,
        claim: &ManagedLibraryOperationClaim,
        job: &AddonMediaListLifecycleJob,
    ) -> Result<EntityId> {
        if job.operation_id != claim.operation.id
            || job.kind != AddonMediaListLifecycleJobKind::Reconcile
            || job.phase != AddonMediaListLifecyclePhase::EnumeratingSource
        {
            return Err(RepositoryError::Conflict(
                "the reconcile identity or phase changed".to_owned(),
            ));
        }
        crate::addon_media_list_import::AddonMediaListImportCoordinator::new(self.0.clone())
            .queue_reconcile(
                job.list_id,
                claim.operation.id,
                &claim.operation.owner_token,
                &claim.lease_owner,
            )
            .await
    }

    async fn complete_reconcile(
        &self,
        claim: &ManagedLibraryOperationClaim,
        candidate_import_job_id: EntityId,
    ) -> Result<bool> {
        let now = encode_datetime(Utc::now());
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        if !claim_is_current(&mut tx, claim).await? {
            return Ok(false);
        }
        let updated = sqlx::query(
            r#"UPDATE "AddonMediaListLifecycleJob"
               SET "Phase" = ?, "UpdatedAt" = ?, "CompletedAt" = ?
               WHERE "OperationId" = ? AND "Kind" = ? AND "Phase" = ?
                 AND "CandidateImportJobId" = ?
                 AND EXISTS (
                   SELECT 1 FROM "AddonMediaListImportJob"
                   WHERE "Id" = ?
                     AND "AddonMediaListId" = "AddonMediaListLifecycleJob"."AddonMediaListId"
                     AND "Purpose" = ? AND "Mode" = ?
                 )"#,
        )
        .bind(encode_enum(AddonMediaListLifecyclePhase::Complete))
        .bind(&now)
        .bind(&now)
        .bind(claim.operation.id)
        .bind(encode_enum(AddonMediaListLifecycleJobKind::Reconcile))
        .bind(encode_enum(
            AddonMediaListLifecyclePhase::ComparingCandidate,
        ))
        .bind(candidate_import_job_id)
        .bind(candidate_import_job_id)
        .bind(encode_enum(
            ersatzrs_domain::AddonMediaListImportPurpose::Reconcile,
        ))
        .bind(encode_enum(ersatzrs_domain::AddonMediaListImportMode::Full))
        .execute(&mut *tx)
        .await
        .repository()?;
        if updated.rows_affected() != 1 {
            tx.rollback().await.repository()?;
            return Ok(false);
        }
        finish_operation_tx(
            &mut tx,
            claim,
            ManagedLibraryOperationState::Completed,
            None,
            &now,
        )
        .await?;
        tx.commit().await.repository()?;
        Ok(true)
    }

    async fn publication_status(
        &self,
        list_id: EntityId,
        now: DateTime<Utc>,
    ) -> Result<Option<AddonMediaListPublicationStatus>> {
        let row = latest_publication_status_row(&self.0, list_id, &encode_datetime(now)).await?;
        row.as_ref().map(decode_publication_status_row).transpose()
    }

    async fn publication_history(
        &self,
        list_id: EntityId,
        now: DateTime<Utc>,
    ) -> Result<AddonMediaListPublicationHistory> {
        load_publication_history(&self.0, list_id, &encode_datetime(now)).await
    }

    async fn prune_publication_history(&self, now: DateTime<Utc>) -> Result<u64> {
        let now = encode_datetime(now);
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let deleted = prune_publication_history_tx(&mut tx, &now).await?;
        tx.commit().await.repository()?;
        Ok(deleted)
    }

    async fn recover_publication(
        &self,
        list_id: EntityId,
        job_id: EntityId,
        generation: i32,
        action: AddonMediaListRecoveryAction,
        now: DateTime<Utc>,
    ) -> Result<AddonMediaListPublicationStatus> {
        let now_text = encode_datetime(now);
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let row = latest_publication_status_row_tx(&mut tx, list_id, &now_text)
            .await?
            .ok_or(RepositoryError::NotFound {
                kind: "add-on media-list publication",
                id: job_id,
            })?;
        let status = decode_publication_status_row(&row)?;
        if status.job_id != job_id || status.generation != generation {
            return Err(RepositoryError::Conflict(
                "the add-on media-list publication generation changed; reload before recovering"
                    .to_owned(),
            ));
        }
        if matches!(
            status.state,
            AddonMediaListPublicationState::Publishing | AddonMediaListPublicationState::Restoring
        ) {
            return Err(RepositoryError::Conflict(
                "the add-on media-list publication still has a live lease".to_owned(),
            ));
        }
        if matches!(status.state, AddonMediaListPublicationState::Complete) {
            return Err(RepositoryError::Conflict(
                "the add-on media-list publication is already complete".to_owned(),
            ));
        }
        if matches!(action, AddonMediaListRecoveryAction::Retry) && !status.can_retry {
            return Err(RepositoryError::Conflict(
                "the add-on media-list publication is not awaiting recovery".to_owned(),
            ));
        }
        if matches!(
            action,
            AddonMediaListRecoveryAction::RestorePreviousRevision
        ) && !status.can_restore
        {
            return Err(RepositoryError::Conflict(
                "the previous add-on media-list revision has no recovery journal".to_owned(),
            ));
        }

        sqlx::query(
            r#"DELETE FROM "ManagedLibraryLease"
               WHERE "OperationId" = ? AND "LeaseExpiresAt" <= ?"#,
        )
        .bind(status.operation_id)
        .bind(&now_text)
        .execute(&mut *tx)
        .await
        .repository()?;

        if matches!(
            action,
            AddonMediaListRecoveryAction::RestorePreviousRevision
        ) {
            let updated = sqlx::query(
                r#"UPDATE "AddonMediaListPublicationJob"
                   SET "Phase" = ?, "ChildScanOperationId" = NULL, "Outcome" = NULL,
                       "OriginalFailureCode" = COALESCE("OriginalFailureCode", ?),
                       "RecoveryDiagnostic" = NULL, "UpdatedAt" = ?, "CompletedAt" = NULL
                   WHERE "Id" = ? AND "AddonMediaListId" = ? AND "Generation" = ?"#,
            )
            .bind(encode_enum(AddonMediaListPublicationPhase::RestoringFiles))
            .bind("operator_restore_requested")
            .bind(&now_text)
            .bind(job_id)
            .bind(list_id)
            .bind(generation)
            .execute(&mut *tx)
            .await
            .repository()?;
            if updated.rows_affected() != 1 {
                return Err(RepositoryError::Conflict(
                    "the add-on media-list publication changed before restore".to_owned(),
                ));
            }
        } else {
            let updated = sqlx::query(
                r#"UPDATE "AddonMediaListPublicationJob"
                   SET "RecoveryDiagnostic" = NULL, "UpdatedAt" = ?, "CompletedAt" = NULL
                   WHERE "Id" = ? AND "AddonMediaListId" = ? AND "Generation" = ?"#,
            )
            .bind(&now_text)
            .bind(job_id)
            .bind(list_id)
            .bind(generation)
            .execute(&mut *tx)
            .await
            .repository()?;
            if updated.rows_affected() != 1 {
                return Err(RepositoryError::Conflict(
                    "the add-on media-list publication changed before retry".to_owned(),
                ));
            }
        }

        let updated = sqlx::query(
            r#"UPDATE "ManagedLibraryOperation"
               SET "Kind" = ?, "State" = ?, "NextEligibleAt" = NULL,
                   "LeaseOwner" = NULL, "LeaseExpiresAt" = NULL,
                   "DiagnosticCode" = NULL, "Diagnostic" = NULL,
                   "UpdatedAt" = ?, "CompletedAt" = NULL
               WHERE "Id" = ?
                 AND NOT EXISTS (
                     SELECT 1 FROM "ManagedLibraryLease"
                     WHERE "OperationId" = ? AND "LeaseExpiresAt" > ?
                 )"#,
        )
        .bind(encode_enum(ManagedLibraryOperationKind::Recovery))
        .bind(encode_enum(ManagedLibraryOperationState::Queued))
        .bind(&now_text)
        .bind(status.operation_id)
        .bind(status.operation_id)
        .bind(&now_text)
        .execute(&mut *tx)
        .await
        .repository()?;
        if updated.rows_affected() != 1 {
            return Err(RepositoryError::Conflict(
                "the add-on media-list publication lease changed before recovery".to_owned(),
            ));
        }
        let phase = if matches!(
            action,
            AddonMediaListRecoveryAction::RestorePreviousRevision
        ) {
            AddonMediaListPublicationPhase::RestoringFiles
        } else {
            status.phase
        };
        let next_attempt = status.attempt_count.saturating_add(1);
        let action_key = encode_enum(action);
        append_publication_event_for_job_tx(
            &mut tx,
            job_id,
            &format!("recovery:{next_attempt}:{action_key}"),
            PublicationEventWrite {
                kind: AddonMediaListPublicationEventKind::RecoveryRequested,
                phase: Some(phase),
                attempt: Some(next_attempt),
                recovery_action: Some(action),
                occurred_at: &now_text,
                ..PublicationEventWrite::default()
            },
        )
        .await?;
        if let Some(owner_operation_id) =
            competing_lease_for_operation_tx(&mut tx, status.operation_id, &now_text).await?
        {
            append_publication_event_for_job_tx(
                &mut tx,
                job_id,
                &format!("lease_wait:{next_attempt}"),
                PublicationEventWrite {
                    kind: AddonMediaListPublicationEventKind::LeaseWaitStarted,
                    phase: Some(phase),
                    attempt: Some(next_attempt),
                    related_operation_id: Some(owner_operation_id),
                    occurred_at: &now_text,
                    ..PublicationEventWrite::default()
                },
            )
            .await?;
        }
        tx.commit().await.repository()?;
        self.publication_status(list_id, now)
            .await?
            .ok_or(RepositoryError::NotFound {
                kind: "add-on media-list publication",
                id: job_id,
            })
    }

    async fn transition_publication(
        &self,
        claim: &ManagedLibraryOperationClaim,
        expected: AddonMediaListPublicationPhase,
        next: AddonMediaListPublicationPhase,
    ) -> Result<bool> {
        let now = encode_datetime(Utc::now());
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let result = sqlx::query(
            r#"UPDATE "AddonMediaListPublicationJob"
               SET "Phase" = ?, "UpdatedAt" = ?
               WHERE "OperationId" = ? AND "Phase" = ?
                 AND EXISTS (
                     SELECT 1 FROM "ManagedLibraryLease"
                     WHERE "OperationId" = ? AND "OwnerToken" = ? AND "LeaseOwner" = ?
                       AND "LeaseExpiresAt" > ?
                 )"#,
        )
        .bind(encode_enum(next))
        .bind(&now)
        .bind(claim.operation.id)
        .bind(encode_enum(expected))
        .bind(claim.operation.id)
        .bind(&claim.operation.owner_token)
        .bind(&claim.lease_owner)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .repository()?;
        if result.rows_affected() != 1 {
            tx.rollback().await.repository()?;
            return Ok(false);
        }
        let filesystem_transition = publication_filesystem_transition(expected, next);
        let counts = if filesystem_transition.is_some() {
            publication_counts_tx(&mut tx, claim.operation.id).await?
        } else {
            PublicationCounts::default()
        };
        append_publication_event_for_operation_tx(
            &mut tx,
            claim.operation.id,
            &format!(
                "phase:{}:{}:{}",
                claim.operation.attempt_count,
                encode_enum(expected),
                encode_enum(next)
            ),
            PublicationEventWrite {
                kind: AddonMediaListPublicationEventKind::PhaseAdvanced,
                phase: Some(next),
                attempt: Some(claim.operation.attempt_count),
                filesystem_transition,
                published_item_count: counts.published_items,
                playlist_item_count: counts.playlist_items,
                artwork_count: counts.artwork,
                occurred_at: &now,
                ..PublicationEventWrite::default()
            },
        )
        .await?;
        tx.commit().await.repository()?;
        Ok(true)
    }

    async fn ensure_publication_child_scan(
        &self,
        claim: &ManagedLibraryOperationClaim,
        list_id: EntityId,
    ) -> Result<ManagedLibraryOperation> {
        let now = encode_datetime(Utc::now());
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        if !claim_is_current(&mut tx, claim).await? {
            return Err(RepositoryError::Conflict(
                "the managed-library lease changed before the publication child scan".to_owned(),
            ));
        }
        let existing = sqlx::query(
            r#"SELECT "operation".*
               FROM "AddonMediaListPublicationJob" AS "job"
               JOIN "ManagedLibraryOperation" AS "operation"
                 ON "operation"."Id" = "job"."ChildScanOperationId"
               WHERE "job"."OperationId" = ? AND "job"."AddonMediaListId" = ?"#,
        )
        .bind(claim.operation.id)
        .bind(list_id)
        .fetch_optional(&mut *tx)
        .await
        .repository()?;
        if let Some(row) = existing {
            let mut operation = decode_operation_row(&row)?;
            if operation.state != ManagedLibraryOperationState::Completed {
                let updated = sqlx::query(
                    r#"UPDATE "ManagedLibraryOperation"
                       SET "State" = ?, "AttemptCount" = "AttemptCount" + 1,
                           "LeaseOwner" = ?,
                           "LeaseExpiresAt" = (
                               SELECT "LeaseExpiresAt" FROM "ManagedLibraryLease"
                               WHERE "OperationId" = ?
                           ),
                           "HeartbeatAt" = ?, "DiagnosticCode" = NULL,
                           "Diagnostic" = NULL, "CompletedAt" = NULL, "UpdatedAt" = ?
                       WHERE "Id" = ? AND "ParentOperationId" = ? AND "OwnerToken" = ?"#,
                )
                .bind(encode_enum(ManagedLibraryOperationState::Running))
                .bind(&claim.lease_owner)
                .bind(claim.operation.id)
                .bind(&now)
                .bind(&now)
                .bind(operation.id)
                .bind(claim.operation.id)
                .bind(&claim.operation.owner_token)
                .execute(&mut *tx)
                .await
                .repository()?;
                if updated.rows_affected() != 1 {
                    return Err(RepositoryError::Conflict(
                        "the publication child scan changed before recovery".to_owned(),
                    ));
                }
                operation.state = ManagedLibraryOperationState::Running;
                operation.attempt_count = operation.attempt_count.saturating_add(1);
                append_publication_event_for_operation_tx(
                    &mut tx,
                    claim.operation.id,
                    &format!("child:{}:attempt:{}", operation.id, operation.attempt_count),
                    PublicationEventWrite {
                        kind: AddonMediaListPublicationEventKind::ChildScanStarted,
                        phase: Some(AddonMediaListPublicationPhase::Scanning),
                        attempt: Some(operation.attempt_count),
                        related_operation_id: Some(operation.id),
                        occurred_at: &now,
                        ..PublicationEventWrite::default()
                    },
                )
                .await?;
            }
            tx.commit().await.repository()?;
            return Ok(operation);
        }
        let result = sqlx::query(
            r#"INSERT INTO "ManagedLibraryOperation" (
                   "LibraryId", "ParentOperationId", "OwnerToken", "Kind", "State",
                   "Provider", "DeepScan", "AttemptCount", "LeaseOwner", "LeaseExpiresAt",
                   "HeartbeatAt", "CreatedAt", "UpdatedAt"
               ) VALUES (?, ?, ?, ?, ?, ?, 1, 1, ?,
                         (SELECT "LeaseExpiresAt" FROM "ManagedLibraryLease" WHERE "OperationId" = ?),
                         ?, ?, ?)"#,
        )
        .bind(claim.operation.library_id)
        .bind(claim.operation.id)
        .bind(&claim.operation.owner_token)
        .bind(encode_enum(ManagedLibraryOperationKind::PublicationChildScan))
        .bind(encode_enum(ManagedLibraryOperationState::Running))
        .bind(encode_enum(MediaSourceKind::Local))
        .bind(&claim.lease_owner)
        .bind(claim.operation.id)
        .bind(&now)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .repository()?;
        let child_id = inserted_id(&mut tx, result).await.repository()?;
        sqlx::query(
            r#"UPDATE "AddonMediaListPublicationJob"
               SET "ChildScanOperationId" = ?, "UpdatedAt" = ?
               WHERE "OperationId" = ? AND "AddonMediaListId" = ?"#,
        )
        .bind(child_id)
        .bind(&now)
        .bind(claim.operation.id)
        .bind(list_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        append_publication_event_for_operation_tx(
            &mut tx,
            claim.operation.id,
            &format!("child:{child_id}:started"),
            PublicationEventWrite {
                kind: AddonMediaListPublicationEventKind::ChildScanStarted,
                phase: Some(AddonMediaListPublicationPhase::Scanning),
                attempt: Some(1),
                related_operation_id: Some(child_id),
                occurred_at: &now,
                ..PublicationEventWrite::default()
            },
        )
        .await?;
        tx.commit().await.repository()?;
        Ok(ManagedLibraryOperation {
            id: child_id,
            library_id: claim.operation.library_id,
            parent_operation_id: Some(claim.operation.id),
            owner_token: claim.operation.owner_token.clone(),
            kind: ManagedLibraryOperationKind::PublicationChildScan,
            state: ManagedLibraryOperationState::Running,
            scan: Some(ManagedLibraryScanSpec {
                provider: MediaSourceKind::Local,
                show_id: None,
                deep_scan: true,
            }),
            attempt_count: 1,
            next_eligible_at: None,
        })
    }

    async fn ensure_publication_verification_scan(
        &self,
        claim: &ManagedLibraryOperationClaim,
        list_id: EntityId,
    ) -> Result<ManagedLibraryOperation> {
        let now = encode_datetime(Utc::now());
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        if !claim_is_current(&mut tx, claim).await? {
            return Err(RepositoryError::Conflict(
                "the managed-library lease changed before publication verification".to_owned(),
            ));
        }
        let existing = sqlx::query(
            r#"SELECT "operation".*
               FROM "AddonMediaListPublicationJob" AS "job"
               JOIN "ManagedLibraryOperation" AS "operation"
                 ON "operation"."Id" = "job"."ChildScanOperationId"
               WHERE "job"."OperationId" = ? AND "job"."AddonMediaListId" = ?
                 AND "job"."Phase" = ?"#,
        )
        .bind(claim.operation.id)
        .bind(list_id)
        .bind(encode_enum(AddonMediaListPublicationPhase::Verifying))
        .fetch_optional(&mut *tx)
        .await
        .repository()?;
        if let Some(row) = existing {
            let deep_scan = decode_bool_column(&row, "DeepScan")?;
            let mut operation = decode_operation_row(&row)?;
            if !deep_scan {
                if operation.state != ManagedLibraryOperationState::Completed {
                    let updated = sqlx::query(
                        r#"UPDATE "ManagedLibraryOperation"
                           SET "State" = ?, "AttemptCount" = "AttemptCount" + 1,
                               "LeaseOwner" = ?,
                               "LeaseExpiresAt" = (
                                   SELECT "LeaseExpiresAt" FROM "ManagedLibraryLease"
                                   WHERE "OperationId" = ?
                               ),
                               "HeartbeatAt" = ?, "DiagnosticCode" = NULL,
                               "Diagnostic" = NULL, "CompletedAt" = NULL, "UpdatedAt" = ?
                           WHERE "Id" = ? AND "ParentOperationId" = ? AND "OwnerToken" = ?"#,
                    )
                    .bind(encode_enum(ManagedLibraryOperationState::Running))
                    .bind(&claim.lease_owner)
                    .bind(claim.operation.id)
                    .bind(&now)
                    .bind(&now)
                    .bind(operation.id)
                    .bind(claim.operation.id)
                    .bind(&claim.operation.owner_token)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
                    if updated.rows_affected() != 1 {
                        return Err(RepositoryError::Conflict(
                            "the publication verification scan changed before recovery".to_owned(),
                        ));
                    }
                    operation.state = ManagedLibraryOperationState::Running;
                    operation.attempt_count = operation.attempt_count.saturating_add(1);
                    append_publication_event_for_operation_tx(
                        &mut tx,
                        claim.operation.id,
                        &format!("child:{}:attempt:{}", operation.id, operation.attempt_count),
                        PublicationEventWrite {
                            kind: AddonMediaListPublicationEventKind::ChildScanStarted,
                            phase: Some(AddonMediaListPublicationPhase::Verifying),
                            attempt: Some(operation.attempt_count),
                            related_operation_id: Some(operation.id),
                            occurred_at: &now,
                            ..PublicationEventWrite::default()
                        },
                    )
                    .await?;
                }
                tx.commit().await.repository()?;
                return Ok(operation);
            }
            if !matches!(
                operation.state,
                ManagedLibraryOperationState::Completed | ManagedLibraryOperationState::Failed
            ) {
                return Err(RepositoryError::Conflict(
                    "the original publication scan has not drained before verification".to_owned(),
                ));
            }
        }

        let result = sqlx::query(
            r#"INSERT INTO "ManagedLibraryOperation" (
                   "LibraryId", "ParentOperationId", "OwnerToken", "Kind", "State",
                   "Provider", "DeepScan", "AttemptCount", "LeaseOwner", "LeaseExpiresAt",
                   "HeartbeatAt", "CreatedAt", "UpdatedAt"
               ) VALUES (?, ?, ?, ?, ?, ?, 0, 1, ?,
                         (SELECT "LeaseExpiresAt" FROM "ManagedLibraryLease" WHERE "OperationId" = ?),
                         ?, ?, ?)"#,
        )
        .bind(claim.operation.library_id)
        .bind(claim.operation.id)
        .bind(&claim.operation.owner_token)
        .bind(encode_enum(ManagedLibraryOperationKind::PublicationChildScan))
        .bind(encode_enum(ManagedLibraryOperationState::Running))
        .bind(encode_enum(MediaSourceKind::Local))
        .bind(&claim.lease_owner)
        .bind(claim.operation.id)
        .bind(&now)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .repository()?;
        let child_id = inserted_id(&mut tx, result).await.repository()?;
        let updated = sqlx::query(
            r#"UPDATE "AddonMediaListPublicationJob"
               SET "ChildScanOperationId" = ?, "UpdatedAt" = ?
               WHERE "OperationId" = ? AND "AddonMediaListId" = ? AND "Phase" = ?"#,
        )
        .bind(child_id)
        .bind(&now)
        .bind(claim.operation.id)
        .bind(list_id)
        .bind(encode_enum(AddonMediaListPublicationPhase::Verifying))
        .execute(&mut *tx)
        .await
        .repository()?;
        if updated.rows_affected() != 1 {
            return Err(RepositoryError::Conflict(
                "the publication phase changed before verification scan admission".to_owned(),
            ));
        }
        append_publication_event_for_operation_tx(
            &mut tx,
            claim.operation.id,
            &format!("child:{child_id}:started"),
            PublicationEventWrite {
                kind: AddonMediaListPublicationEventKind::ChildScanStarted,
                phase: Some(AddonMediaListPublicationPhase::Verifying),
                attempt: Some(1),
                related_operation_id: Some(child_id),
                occurred_at: &now,
                ..PublicationEventWrite::default()
            },
        )
        .await?;
        tx.commit().await.repository()?;
        Ok(ManagedLibraryOperation {
            id: child_id,
            library_id: claim.operation.library_id,
            parent_operation_id: Some(claim.operation.id),
            owner_token: claim.operation.owner_token.clone(),
            kind: ManagedLibraryOperationKind::PublicationChildScan,
            state: ManagedLibraryOperationState::Running,
            scan: Some(ManagedLibraryScanSpec {
                provider: MediaSourceKind::Local,
                show_id: None,
                deep_scan: false,
            }),
            attempt_count: 1,
            next_eligible_at: None,
        })
    }

    async fn begin_publication_verification(
        &self,
        claim: &ManagedLibraryOperationClaim,
        expected: AddonMediaListPublicationPhase,
        outcome: AddonMediaListPublicationOutcome,
    ) -> Result<bool> {
        let now = encode_datetime(Utc::now());
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let result = sqlx::query(
            r#"UPDATE "AddonMediaListPublicationJob"
               SET "Phase" = ?, "Outcome" = ?, "UpdatedAt" = ?
               WHERE "OperationId" = ? AND "Phase" = ?
                 AND EXISTS (
                     SELECT 1 FROM "ManagedLibraryLease"
                     WHERE "OperationId" = ? AND "OwnerToken" = ? AND "LeaseOwner" = ?
                       AND "LeaseExpiresAt" > ?
                 )"#,
        )
        .bind(encode_enum(AddonMediaListPublicationPhase::Verifying))
        .bind(encode_enum(outcome))
        .bind(&now)
        .bind(claim.operation.id)
        .bind(encode_enum(expected))
        .bind(claim.operation.id)
        .bind(&claim.operation.owner_token)
        .bind(&claim.lease_owner)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .repository()?;
        if result.rows_affected() != 1 {
            tx.rollback().await.repository()?;
            return Ok(false);
        }
        let filesystem_transition = matches!(expected, AddonMediaListPublicationPhase::Committing)
            .then_some(AddonMediaListPublicationFilesystemTransition::CandidateCommitted);
        let counts = publication_counts_tx(&mut tx, claim.operation.id).await?;
        append_publication_event_for_operation_tx(
            &mut tx,
            claim.operation.id,
            &format!(
                "verification:{}:{}:{}",
                claim.operation.attempt_count,
                encode_enum(expected),
                encode_enum(outcome)
            ),
            PublicationEventWrite {
                kind: AddonMediaListPublicationEventKind::PhaseAdvanced,
                phase: Some(AddonMediaListPublicationPhase::Verifying),
                attempt: Some(claim.operation.attempt_count),
                filesystem_transition,
                published_item_count: counts.published_items,
                playlist_item_count: counts.playlist_items,
                artwork_count: counts.artwork,
                outcome: Some(outcome),
                occurred_at: &now,
                ..PublicationEventWrite::default()
            },
        )
        .await?;
        tx.commit().await.repository()?;
        Ok(true)
    }

    async fn record_publication_failure(
        &self,
        claim: &ManagedLibraryOperationClaim,
        expected: AddonMediaListPublicationPhase,
        code: &str,
        diagnostic: &str,
        cause: Option<AddonMediaListPublicationFailureCause>,
    ) -> Result<bool> {
        let now = encode_datetime(Utc::now());
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let result = sqlx::query(
            r#"UPDATE "AddonMediaListPublicationJob"
               SET "Phase" = ?,
                   "OriginalFailureCode" = COALESCE("OriginalFailureCode", ?),
                   "OriginalFailureDiagnostic" = COALESCE("OriginalFailureDiagnostic", ?),
                   "UpdatedAt" = ?
               WHERE "OperationId" = ? AND "Phase" = ?
                 AND EXISTS (
                     SELECT 1 FROM "ManagedLibraryLease"
                     WHERE "OperationId" = ? AND "OwnerToken" = ? AND "LeaseOwner" = ?
                       AND "LeaseExpiresAt" > ?
                 )"#,
        )
        .bind(encode_enum(AddonMediaListPublicationPhase::RestoringFiles))
        .bind(code)
        .bind(bounded_diagnostic(diagnostic))
        .bind(&now)
        .bind(claim.operation.id)
        .bind(encode_enum(expected))
        .bind(claim.operation.id)
        .bind(&claim.operation.owner_token)
        .bind(&claim.lease_owner)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .repository()?;
        if result.rows_affected() != 1 {
            tx.rollback().await.repository()?;
            return Ok(false);
        }
        append_publication_event_for_operation_tx(
            &mut tx,
            claim.operation.id,
            &format!(
                "original_failure:{}:{}",
                claim.operation.attempt_count,
                encode_enum(expected)
            ),
            PublicationEventWrite {
                kind: AddonMediaListPublicationEventKind::OriginalFailure,
                phase: Some(expected),
                attempt: Some(claim.operation.attempt_count),
                error_category: cause.map(|value| value.category),
                os_error_code: cause.and_then(|value| value.os_error_code),
                occurred_at: &now,
                ..PublicationEventWrite::default()
            },
        )
        .await?;
        tx.commit().await.repository()?;
        Ok(true)
    }

    async fn schedule_publication_recovery_retry(
        &self,
        claim: &ManagedLibraryOperationClaim,
        diagnostic: &str,
        cause: Option<AddonMediaListPublicationFailureCause>,
        next_eligible_at: DateTime<Utc>,
    ) -> Result<bool> {
        let now = encode_datetime(Utc::now());
        let next = encode_datetime(next_eligible_at);
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        if !claim_is_current(&mut tx, claim).await? {
            return Ok(false);
        }
        let phase = publication_phase_tx(&mut tx, claim.operation.id).await?;
        sqlx::query(
            r#"UPDATE "AddonMediaListPublicationJob"
               SET "RecoveryDiagnostic" = ?, "UpdatedAt" = ?
               WHERE "OperationId" = ?"#,
        )
        .bind(bounded_diagnostic(diagnostic))
        .bind(&now)
        .bind(claim.operation.id)
        .execute(&mut *tx)
        .await
        .repository()?;
        append_publication_event_for_operation_tx(
            &mut tx,
            claim.operation.id,
            &format!("recovery_failure:{}", claim.operation.attempt_count),
            PublicationEventWrite {
                kind: AddonMediaListPublicationEventKind::RecoveryFailure,
                phase,
                attempt: Some(claim.operation.attempt_count),
                error_category: cause.map(|value| value.category),
                os_error_code: cause.and_then(|value| value.os_error_code),
                occurred_at: &now,
                ..PublicationEventWrite::default()
            },
        )
        .await?;
        append_publication_event_for_operation_tx(
            &mut tx,
            claim.operation.id,
            &format!("retry:{}", claim.operation.attempt_count),
            PublicationEventWrite {
                kind: AddonMediaListPublicationEventKind::RetryScheduled,
                phase,
                attempt: Some(claim.operation.attempt_count),
                retry_class: Some(AddonMediaListPublicationRetryClass::TransientRecovery),
                occurred_at: &now,
                ..PublicationEventWrite::default()
            },
        )
        .await?;
        sqlx::query(
            r#"UPDATE "ManagedLibraryOperation"
               SET "State" = ?, "NextEligibleAt" = ?, "LeaseOwner" = NULL,
                   "LeaseExpiresAt" = NULL, "UpdatedAt" = ?
               WHERE "Id" = ? AND "OwnerToken" = ? AND "LeaseOwner" = ?"#,
        )
        .bind(encode_enum(ManagedLibraryOperationState::WaitingToRetry))
        .bind(next)
        .bind(&now)
        .bind(claim.operation.id)
        .bind(&claim.operation.owner_token)
        .bind(&claim.lease_owner)
        .execute(&mut *tx)
        .await
        .repository()?;
        sqlx::query(
            r#"DELETE FROM "ManagedLibraryLease"
               WHERE "OperationId" = ? AND "OwnerToken" = ? AND "LeaseOwner" = ?"#,
        )
        .bind(claim.operation.id)
        .bind(&claim.operation.owner_token)
        .bind(&claim.lease_owner)
        .execute(&mut *tx)
        .await
        .repository()?;
        tx.commit().await.repository()?;
        Ok(true)
    }

    async fn fail_publication(
        &self,
        claim: &ManagedLibraryOperationClaim,
        code: &str,
        diagnostic: &str,
        cause: Option<AddonMediaListPublicationFailureCause>,
    ) -> Result<bool> {
        let now = encode_datetime(Utc::now());
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        if !claim_is_current(&mut tx, claim).await? {
            return Ok(false);
        }
        let phase = publication_phase_tx(&mut tx, claim.operation.id).await?;
        let Some(phase) = phase else {
            return Err(RepositoryError::NotFound {
                kind: "add-on media-list publication",
                id: claim.operation.id,
            });
        };
        let updated = sqlx::query(
            r#"UPDATE "AddonMediaListPublicationJob"
               SET "OriginalFailureCode" = COALESCE("OriginalFailureCode", ?),
                   "OriginalFailureDiagnostic" = COALESCE("OriginalFailureDiagnostic", ?),
                   "UpdatedAt" = ?, "CompletedAt" = ?
               WHERE "OperationId" = ?"#,
        )
        .bind(code)
        .bind(bounded_diagnostic(diagnostic))
        .bind(&now)
        .bind(&now)
        .bind(claim.operation.id)
        .execute(&mut *tx)
        .await
        .repository()?;
        if updated.rows_affected() != 1 {
            return Ok(false);
        }
        append_publication_event_for_operation_tx(
            &mut tx,
            claim.operation.id,
            &format!("terminal_failure:{}", claim.operation.attempt_count),
            PublicationEventWrite {
                kind: AddonMediaListPublicationEventKind::OriginalFailure,
                phase: Some(phase),
                attempt: Some(claim.operation.attempt_count),
                error_category: cause.map(|value| value.category),
                os_error_code: cause.and_then(|value| value.os_error_code),
                occurred_at: &now,
                ..PublicationEventWrite::default()
            },
        )
        .await?;
        finish_operation_tx(
            &mut tx,
            claim,
            ManagedLibraryOperationState::Failed,
            Some((code, bounded_diagnostic(diagnostic))),
            &now,
        )
        .await?;
        tx.commit().await.repository()?;
        Ok(true)
    }

    async fn complete_publication(
        &self,
        claim: &ManagedLibraryOperationClaim,
        outcome: AddonMediaListPublicationOutcome,
    ) -> Result<bool> {
        let now = encode_datetime(Utc::now());
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        if !claim_is_current(&mut tx, claim).await? {
            return Ok(false);
        }
        sqlx::query(
            r#"UPDATE "AddonMediaListPublicationJob"
               SET "Phase" = ?, "Outcome" = ?, "UpdatedAt" = ?, "CompletedAt" = ?
               WHERE "OperationId" = ?"#,
        )
        .bind(encode_enum(AddonMediaListPublicationPhase::Complete))
        .bind(encode_enum(outcome))
        .bind(&now)
        .bind(&now)
        .bind(claim.operation.id)
        .execute(&mut *tx)
        .await
        .repository()?;
        let counts = publication_counts_tx(&mut tx, claim.operation.id).await?;
        append_publication_event_for_operation_tx(
            &mut tx,
            claim.operation.id,
            &format!("completed:{}", encode_enum(outcome)),
            PublicationEventWrite {
                kind: AddonMediaListPublicationEventKind::Completed,
                phase: Some(AddonMediaListPublicationPhase::Complete),
                attempt: Some(claim.operation.attempt_count),
                filesystem_transition: Some(
                    AddonMediaListPublicationFilesystemTransition::CleanupCompleted,
                ),
                published_item_count: counts.published_items,
                playlist_item_count: counts.playlist_items,
                artwork_count: counts.artwork,
                outcome: Some(outcome),
                occurred_at: &now,
                ..PublicationEventWrite::default()
            },
        )
        .await?;
        finish_operation_tx(
            &mut tx,
            claim,
            ManagedLibraryOperationState::Completed,
            None,
            &now,
        )
        .await?;
        prune_publication_history_tx(&mut tx, &now).await?;
        tx.commit().await.repository()?;
        Ok(true)
    }

    async fn complete_operation(&self, claim: &ManagedLibraryOperationClaim) -> Result<bool> {
        finish_operation(
            &self.0,
            claim,
            ManagedLibraryOperationState::Completed,
            None,
        )
        .await
    }

    async fn fail_operation(
        &self,
        claim: &ManagedLibraryOperationClaim,
        code: &str,
        diagnostic: &str,
    ) -> Result<bool> {
        finish_operation(
            &self.0,
            claim,
            ManagedLibraryOperationState::Failed,
            Some((code, bounded_diagnostic(diagnostic))),
        )
        .await
    }
}

const PUBLICATION_STATUS_SQL: &str = r#"SELECT "job"."Id" AS "PublicationJobId",
              "job"."OperationId", "job"."AddonMediaListId", "job"."Generation",
              "job"."SavedRevision", "job"."TargetPublishedRevision", "job"."Phase",
              "job"."OriginalFailureCode", "job"."RecoveryDiagnostic",
              "job"."Outcome", "job"."UpdatedAt" AS "JobUpdatedAt",
              "operation"."State" AS "OperationState",
              "operation"."AttemptCount", "operation"."DiagnosticCode",
              "list"."PublicationJournalJson",
              EXISTS (
                  SELECT 1 FROM "ManagedLibraryLease" AS "lease"
                  WHERE "lease"."OperationId" = "operation"."Id"
                    AND "lease"."OwnerToken" = "operation"."OwnerToken"
                    AND "lease"."LeaseExpiresAt" > ?
              ) AS "LiveLease"
       FROM "AddonMediaListPublicationJob" AS "job"
       JOIN "ManagedLibraryOperation" AS "operation"
         ON "operation"."Id" = "job"."OperationId"
       JOIN "AddonMediaList" AS "list"
         ON "list"."Id" = "job"."AddonMediaListId"
       WHERE "job"."AddonMediaListId" = ?
       ORDER BY "job"."Generation" DESC, "job"."Id" DESC
       LIMIT 1"#;

#[derive(Clone, Copy)]
struct PublicationEventWrite<'a> {
    kind: AddonMediaListPublicationEventKind,
    enqueue_trigger: Option<AddonMediaListPublicationEnqueueTrigger>,
    phase: Option<AddonMediaListPublicationPhase>,
    attempt: Option<i32>,
    related_operation_id: Option<EntityId>,
    filesystem_transition: Option<AddonMediaListPublicationFilesystemTransition>,
    retry_class: Option<AddonMediaListPublicationRetryClass>,
    recovery_action: Option<AddonMediaListRecoveryAction>,
    error_category: Option<AddonMediaListPublicationErrorCategory>,
    os_error_code: Option<i32>,
    published_item_count: Option<i64>,
    playlist_item_count: Option<i64>,
    artwork_count: Option<i64>,
    outcome: Option<AddonMediaListPublicationOutcome>,
    occurred_at: &'a str,
}

impl Default for PublicationEventWrite<'_> {
    fn default() -> Self {
        Self {
            kind: AddonMediaListPublicationEventKind::ExistingJobAdopted,
            enqueue_trigger: None,
            phase: None,
            attempt: None,
            related_operation_id: None,
            filesystem_transition: None,
            retry_class: None,
            recovery_action: None,
            error_category: None,
            os_error_code: None,
            published_item_count: None,
            playlist_item_count: None,
            artwork_count: None,
            outcome: None,
            occurred_at: "",
        }
    }
}

#[derive(Clone, Copy, Default)]
struct PublicationCounts {
    published_items: Option<i64>,
    playlist_items: Option<i64>,
    artwork: Option<i64>,
}

async fn append_publication_event_for_operation_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    operation_id: EntityId,
    dedupe_key: &str,
    event: PublicationEventWrite<'_>,
) -> Result<()> {
    let job_id = sqlx::query_scalar::<_, i64>(
        r#"SELECT "Id" FROM "AddonMediaListPublicationJob" WHERE "OperationId" = ?"#,
    )
    .bind(operation_id)
    .fetch_optional(&mut **tx)
    .await
    .repository()?;
    if let Some(job_id) = job_id {
        append_publication_event_for_job_tx(tx, job_id, dedupe_key, event).await?;
    }
    Ok(())
}

async fn append_publication_event_for_job_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    job_id: EntityId,
    dedupe_key: &str,
    event: PublicationEventWrite<'_>,
) -> Result<bool> {
    let exists = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM "AddonMediaListPublicationEvent"
           WHERE "PublicationJobId" = ? AND "DedupeKey" = ?"#,
    )
    .bind(job_id)
    .bind(dedupe_key)
    .fetch_one(&mut **tx)
    .await
    .repository()?;
    if exists > 0 {
        return Ok(false);
    }
    let sequence = sqlx::query_scalar::<_, i32>(
        r#"SELECT COALESCE(MAX("Sequence"), 0) + 1
           FROM "AddonMediaListPublicationEvent" WHERE "PublicationJobId" = ?"#,
    )
    .bind(job_id)
    .fetch_one(&mut **tx)
    .await
    .repository()?;
    if sequence > PUBLICATION_HISTORY_LIMIT {
        return Ok(false);
    }
    let truncated = sequence == PUBLICATION_HISTORY_LIMIT;
    let (dedupe_key, event) = if truncated {
        (
            "history_truncated",
            PublicationEventWrite {
                kind: AddonMediaListPublicationEventKind::HistoryTruncated,
                occurred_at: event.occurred_at,
                ..PublicationEventWrite::default()
            },
        )
    } else {
        (dedupe_key, event)
    };
    sqlx::query(
        r#"INSERT INTO "AddonMediaListPublicationEvent" (
               "PublicationJobId", "Sequence", "DedupeKey", "Kind", "EnqueueTrigger",
               "Phase", "Attempt", "RelatedOperationId", "FilesystemTransition",
               "RetryClass", "RecoveryAction", "ErrorCategory", "OsErrorCode",
               "PublishedItemCount", "PlaylistItemCount", "ArtworkCount", "Outcome",
               "OccurredAt"
           ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(job_id)
    .bind(sequence)
    .bind(dedupe_key)
    .bind(encode_enum(event.kind))
    .bind(event.enqueue_trigger.map(encode_enum))
    .bind(event.phase.map(encode_enum))
    .bind(event.attempt)
    .bind(event.related_operation_id)
    .bind(event.filesystem_transition.map(encode_enum))
    .bind(event.retry_class.map(encode_enum))
    .bind(event.recovery_action.map(encode_enum))
    .bind(event.error_category.map(encode_enum))
    .bind(event.os_error_code)
    .bind(event.published_item_count)
    .bind(event.playlist_item_count)
    .bind(event.artwork_count)
    .bind(event.outcome.map(encode_enum))
    .bind(event.occurred_at)
    .execute(&mut **tx)
    .await
    .repository()?;
    Ok(true)
}

async fn publication_phase_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    operation_id: EntityId,
) -> Result<Option<AddonMediaListPublicationPhase>> {
    let row = sqlx::query(
        r#"SELECT "Phase" FROM "AddonMediaListPublicationJob" WHERE "OperationId" = ?"#,
    )
    .bind(operation_id)
    .fetch_optional(&mut **tx)
    .await
    .repository()?;
    row.as_ref()
        .map(|row| decode_enum_column("AddonMediaListPublicationPhase", row, "Phase"))
        .transpose()
}

async fn competing_lease_operation_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    library_id: EntityId,
    operation_id: EntityId,
    now: &str,
) -> Result<Option<EntityId>> {
    sqlx::query_scalar(
        r#"SELECT "OperationId" FROM "ManagedLibraryLease"
           WHERE "LibraryId" = ? AND "OperationId" <> ? AND "LeaseExpiresAt" > ?
           ORDER BY "OperationId" LIMIT 1"#,
    )
    .bind(library_id)
    .bind(operation_id)
    .bind(now)
    .fetch_optional(&mut **tx)
    .await
    .repository()
}

async fn competing_lease_for_operation_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    operation_id: EntityId,
    now: &str,
) -> Result<Option<EntityId>> {
    sqlx::query_scalar(
        r#"SELECT "lease"."OperationId"
           FROM "ManagedLibraryOperation" AS "operation"
           JOIN "ManagedLibraryLease" AS "lease"
             ON "lease"."LibraryId" = "operation"."LibraryId"
           WHERE "operation"."Id" = ? AND "lease"."OperationId" <> ?
             AND "lease"."LeaseExpiresAt" > ?
           ORDER BY "lease"."OperationId" LIMIT 1"#,
    )
    .bind(operation_id)
    .bind(operation_id)
    .bind(now)
    .fetch_optional(&mut **tx)
    .await
    .repository()
}

async fn record_waiting_publications_for_owner_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    library_id: EntityId,
    owner_operation_id: EntityId,
    now: &str,
) -> Result<()> {
    let rows = sqlx::query(
        r#"SELECT "job"."Id" AS "PublicationJobId", "job"."Phase",
                  "operation"."AttemptCount"
           FROM "AddonMediaListPublicationJob" AS "job"
           JOIN "ManagedLibraryOperation" AS "operation"
             ON "operation"."Id" = "job"."OperationId"
           WHERE "operation"."LibraryId" = ? AND "operation"."Id" <> ?
             AND "operation"."State" IN (?, ?, ?)
             AND ("operation"."NextEligibleAt" IS NULL OR "operation"."NextEligibleAt" <= ?)
             AND ("operation"."LeaseExpiresAt" IS NULL OR "operation"."LeaseExpiresAt" <= ?)"#,
    )
    .bind(library_id)
    .bind(owner_operation_id)
    .bind(encode_enum(ManagedLibraryOperationState::Queued))
    .bind(encode_enum(ManagedLibraryOperationState::Running))
    .bind(encode_enum(ManagedLibraryOperationState::WaitingToRetry))
    .bind(now)
    .bind(now)
    .fetch_all(&mut **tx)
    .await
    .repository()?;
    for row in rows {
        let job_id: EntityId = row.try_get("PublicationJobId").repository()?;
        let phase = decode_enum_column("AddonMediaListPublicationPhase", &row, "Phase")?;
        let attempt: i32 = row.try_get("AttemptCount").repository()?;
        let appended = append_publication_event_for_job_tx(
            tx,
            job_id,
            &format!("lease_wait:{attempt}:{owner_operation_id}"),
            PublicationEventWrite {
                kind: AddonMediaListPublicationEventKind::LeaseWaitStarted,
                phase: Some(phase),
                attempt: Some(attempt),
                related_operation_id: Some(owner_operation_id),
                occurred_at: now,
                ..PublicationEventWrite::default()
            },
        )
        .await?;
        if appended {
            sqlx::query(
                r#"UPDATE "AddonMediaListPublicationJob" SET "UpdatedAt" = ? WHERE "Id" = ?"#,
            )
            .bind(now)
            .bind(job_id)
            .execute(&mut **tx)
            .await
            .repository()?;
        }
    }
    Ok(())
}

const fn publication_filesystem_transition(
    expected: AddonMediaListPublicationPhase,
    next: AddonMediaListPublicationPhase,
) -> Option<AddonMediaListPublicationFilesystemTransition> {
    match (expected, next) {
        (AddonMediaListPublicationPhase::Staging, AddonMediaListPublicationPhase::Scanning) => {
            Some(AddonMediaListPublicationFilesystemTransition::PreviousBackedUp)
        }
        (
            AddonMediaListPublicationPhase::RestoringFiles,
            AddonMediaListPublicationPhase::RestoringProjection,
        ) => Some(AddonMediaListPublicationFilesystemTransition::PreviousRestored),
        (
            AddonMediaListPublicationPhase::RestoringProjection,
            AddonMediaListPublicationPhase::Reconciling,
        ) => Some(AddonMediaListPublicationFilesystemTransition::ProjectionRestored),
        (AddonMediaListPublicationPhase::Verifying, AddonMediaListPublicationPhase::CleaningUp) => {
            Some(AddonMediaListPublicationFilesystemTransition::RevisionVerified)
        }
        _ => None,
    }
}

async fn publication_counts_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    operation_id: EntityId,
) -> Result<PublicationCounts> {
    let row = sqlx::query(
        r#"SELECT
               (SELECT COUNT(*) FROM "AddonMediaListItem" AS "item"
                WHERE "item"."AddonMediaListId" = "job"."AddonMediaListId"
                  AND "item"."Published" = 1) AS "PublishedItemCount",
               (SELECT COUNT(*) FROM "PlaylistItem" AS "playlist_item"
                WHERE "playlist_item"."PlaylistId" = "list"."PlaylistId")
                   AS "PlaylistItemCount",
               (SELECT COUNT(*) FROM "AddonMediaListArtworkMaterialization" AS "artwork"
                WHERE "artwork"."AddonMediaListId" = "job"."AddonMediaListId")
                   AS "ArtworkCount"
           FROM "AddonMediaListPublicationJob" AS "job"
           JOIN "AddonMediaList" AS "list" ON "list"."Id" = "job"."AddonMediaListId"
           WHERE "job"."OperationId" = ?"#,
    )
    .bind(operation_id)
    .fetch_optional(&mut **tx)
    .await
    .repository()?;
    let Some(row) = row else {
        return Ok(PublicationCounts::default());
    };
    Ok(PublicationCounts {
        published_items: Some(row.try_get("PublishedItemCount").repository()?),
        playlist_items: Some(row.try_get("PlaylistItemCount").repository()?),
        artwork: Some(row.try_get("ArtworkCount").repository()?),
    })
}

async fn prune_publication_history_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    now: &str,
) -> Result<u64> {
    let result = sqlx::query(
        r#"WITH "CompletedJobs" AS (
               SELECT "job"."Id", COALESCE("job"."CompletedAt", "operation"."CompletedAt")
                          AS "CompletedAt",
                      ROW_NUMBER() OVER (
                          PARTITION BY "job"."AddonMediaListId"
                          ORDER BY COALESCE("job"."CompletedAt", "operation"."CompletedAt") DESC,
                                   "job"."Id" DESC
                      ) AS "RetentionRank"
               FROM "AddonMediaListPublicationJob" AS "job"
               JOIN "ManagedLibraryOperation" AS "operation"
                 ON "operation"."Id" = "job"."OperationId"
               WHERE "operation"."State" = ?
           )
           DELETE FROM "AddonMediaListPublicationEvent"
           WHERE "PublicationJobId" IN (
               SELECT "Id" FROM "CompletedJobs"
               WHERE julianday("CompletedAt") < julianday(?) - ? OR "RetentionRank" > 50
           )"#,
    )
    .bind(encode_enum(ManagedLibraryOperationState::Completed))
    .bind(now)
    .bind(PUBLICATION_TERMINAL_HISTORY_DAYS)
    .execute(&mut **tx)
    .await
    .repository()?;
    Ok(result.rows_affected())
}

async fn load_publication_history(
    pool: &DbPool,
    list_id: EntityId,
    now: &str,
) -> Result<AddonMediaListPublicationHistory> {
    let rows = sqlx::query(
        r#"SELECT "job"."Id" AS "PublicationJobId", "job"."OperationId",
                  "job"."Generation", "job"."SavedRevision",
                  "job"."TargetPublishedRevision", "job"."Phase", "job"."Outcome",
                  "operation"."State" AS "OperationState",
                  EXISTS (
                      SELECT 1 FROM "ManagedLibraryLease" AS "lease"
                      WHERE "lease"."OperationId" = "operation"."Id"
                        AND "lease"."OwnerToken" = "operation"."OwnerToken"
                        AND "lease"."LeaseExpiresAt" > ?
                  ) AS "LiveLease"
           FROM "AddonMediaListPublicationJob" AS "job"
           JOIN "ManagedLibraryOperation" AS "operation"
             ON "operation"."Id" = "job"."OperationId"
           WHERE "job"."AddonMediaListId" = ?
             AND EXISTS (
                 SELECT 1 FROM "AddonMediaListPublicationEvent" AS "event"
                 WHERE "event"."PublicationJobId" = "job"."Id"
             )
           ORDER BY "job"."Generation" DESC, "job"."Id" DESC"#,
    )
    .bind(now)
    .bind(list_id)
    .fetch_all(pool)
    .await
    .repository()?;
    let mut jobs = Vec::with_capacity(rows.len());
    for row in rows {
        let job_id: EntityId = row.try_get("PublicationJobId").repository()?;
        let phase = decode_enum_column("AddonMediaListPublicationPhase", &row, "Phase")?;
        let operation_state =
            decode_enum_column("ManagedLibraryOperationState", &row, "OperationState")?;
        let live_lease = decode_bool_column(&row, "LiveLease")?;
        let outcome =
            decode_optional_enum_column("AddonMediaListPublicationOutcome", &row, "Outcome")?;
        let event_rows = sqlx::query(
            r#"SELECT * FROM "AddonMediaListPublicationEvent"
               WHERE "PublicationJobId" = ? ORDER BY "Sequence""#,
        )
        .bind(job_id)
        .fetch_all(pool)
        .await
        .repository()?;
        let events = event_rows
            .iter()
            .map(decode_publication_event_row)
            .collect::<Result<Vec<_>>>()?;
        jobs.push(AddonMediaListPublicationHistoryJob {
            job_id,
            operation_id: row.try_get("OperationId").repository()?,
            generation: row.try_get("Generation").repository()?,
            saved_revision: row.try_get("SavedRevision").repository()?,
            target_published_revision: row.try_get("TargetPublishedRevision").repository()?,
            phase,
            state: publication_state(operation_state, phase, live_lease),
            outcome,
            events,
        });
    }
    Ok(AddonMediaListPublicationHistory { list_id, jobs })
}

fn decode_publication_event_row(row: &crate::db::DbRow) -> Result<AddonMediaListPublicationEvent> {
    Ok(AddonMediaListPublicationEvent {
        sequence: row.try_get("Sequence").repository()?,
        kind: decode_enum_column("AddonMediaListPublicationEventKind", row, "Kind")?,
        enqueue_trigger: decode_optional_enum_column(
            "AddonMediaListPublicationEnqueueTrigger",
            row,
            "EnqueueTrigger",
        )?,
        phase: decode_optional_enum_column("AddonMediaListPublicationPhase", row, "Phase")?,
        attempt: row.try_get("Attempt").repository()?,
        related_operation_id: row.try_get("RelatedOperationId").repository()?,
        filesystem_transition: decode_optional_enum_column(
            "AddonMediaListPublicationFilesystemTransition",
            row,
            "FilesystemTransition",
        )?,
        retry_class: decode_optional_enum_column(
            "AddonMediaListPublicationRetryClass",
            row,
            "RetryClass",
        )?,
        recovery_action: decode_optional_enum_column(
            "AddonMediaListRecoveryAction",
            row,
            "RecoveryAction",
        )?,
        error_category: decode_optional_enum_column(
            "AddonMediaListPublicationErrorCategory",
            row,
            "ErrorCategory",
        )?,
        os_error_code: row.try_get("OsErrorCode").repository()?,
        published_item_count: row.try_get("PublishedItemCount").repository()?,
        playlist_item_count: row.try_get("PlaylistItemCount").repository()?,
        artwork_count: row.try_get("ArtworkCount").repository()?,
        outcome: decode_optional_enum_column("AddonMediaListPublicationOutcome", row, "Outcome")?,
        occurred_at: decode_datetime(
            "add-on media-list publication event",
            row.try_get::<String, _>("OccurredAt")
                .repository()?
                .as_str(),
        )?,
    })
}

fn publication_state(
    operation_state: ManagedLibraryOperationState,
    phase: AddonMediaListPublicationPhase,
    live_lease: bool,
) -> AddonMediaListPublicationState {
    match operation_state {
        ManagedLibraryOperationState::Completed => AddonMediaListPublicationState::Complete,
        ManagedLibraryOperationState::Failed => AddonMediaListPublicationState::Failed,
        ManagedLibraryOperationState::Queued => AddonMediaListPublicationState::Queued,
        ManagedLibraryOperationState::WaitingToRetry => AddonMediaListPublicationState::Waiting,
        ManagedLibraryOperationState::Running if !live_lease => {
            AddonMediaListPublicationState::Expired
        }
        ManagedLibraryOperationState::Running
            if matches!(
                phase,
                AddonMediaListPublicationPhase::RestoringFiles
                    | AddonMediaListPublicationPhase::RestoringProjection
                    | AddonMediaListPublicationPhase::Reconciling
            ) =>
        {
            AddonMediaListPublicationState::Restoring
        }
        ManagedLibraryOperationState::Running => AddonMediaListPublicationState::Publishing,
    }
}

async fn latest_publication_status_row(
    pool: &DbPool,
    list_id: EntityId,
    now: &str,
) -> Result<Option<crate::db::DbRow>> {
    sqlx::query(PUBLICATION_STATUS_SQL)
        .bind(now)
        .bind(list_id)
        .fetch_optional(pool)
        .await
        .repository()
}

async fn latest_publication_status_row_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    list_id: EntityId,
    now: &str,
) -> Result<Option<crate::db::DbRow>> {
    sqlx::query(PUBLICATION_STATUS_SQL)
        .bind(now)
        .bind(list_id)
        .fetch_optional(&mut **tx)
        .await
        .repository()
}

fn decode_publication_status_row(
    row: &crate::db::DbRow,
) -> Result<AddonMediaListPublicationStatus> {
    let phase = decode_enum_column("AddonMediaListPublicationPhase", row, "Phase")?;
    let operation_state =
        decode_enum_column("ManagedLibraryOperationState", row, "OperationState")?;
    let live_lease = decode_bool_column(row, "LiveLease")?;
    let original_failure_code: Option<String> = row.try_get("OriginalFailureCode").repository()?;
    let recovery_diagnostic: Option<String> = row.try_get("RecoveryDiagnostic").repository()?;
    let operation_diagnostic_code: Option<String> = row.try_get("DiagnosticCode").repository()?;
    let journal: Option<String> = row.try_get("PublicationJournalJson").repository()?;
    let state = publication_state(operation_state, phase, live_lease);
    let recovery_required = matches!(
        state,
        AddonMediaListPublicationState::Waiting
            | AddonMediaListPublicationState::Expired
            | AddonMediaListPublicationState::Failed
    );
    let diagnostic_code = original_failure_code
        .or(operation_diagnostic_code)
        .map(|value| redacted_diagnostic_code(&value))
        .or_else(|| match state {
            AddonMediaListPublicationState::Expired => Some("lease_expired".to_owned()),
            AddonMediaListPublicationState::Waiting => Some("waiting_to_retry".to_owned()),
            AddonMediaListPublicationState::Failed => Some("publication_failed".to_owned()),
            _ => None,
        });
    Ok(AddonMediaListPublicationStatus {
        outcome: decode_optional_enum_column("AddonMediaListPublicationOutcome", row, "Outcome")?,
        job_id: row.try_get("PublicationJobId").repository()?,
        operation_id: row.try_get("OperationId").repository()?,
        list_id: row.try_get("AddonMediaListId").repository()?,
        generation: row.try_get("Generation").repository()?,
        saved_revision: row.try_get("SavedRevision").repository()?,
        target_published_revision: row.try_get("TargetPublishedRevision").repository()?,
        phase,
        state,
        attempt_count: row.try_get("AttemptCount").repository()?,
        last_progress_at: decode_datetime(
            "add-on media-list publication progress",
            row.try_get::<String, _>("JobUpdatedAt")
                .repository()?
                .as_str(),
        )?,
        diagnostic_code,
        recovery_diagnostic_present: recovery_diagnostic
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty()),
        can_retry: recovery_required,
        can_restore: recovery_required
            && journal
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty()),
    })
}

fn redacted_diagnostic_code(value: &str) -> String {
    let normalized = value.trim().to_ascii_lowercase();
    if !normalized.is_empty()
        && normalized.len() <= 96
        && normalized.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
    {
        normalized
    } else {
        "publication_failed".to_owned()
    }
}

async fn active_reset_row(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    list_id: EntityId,
) -> Result<Option<crate::db::DbRow>> {
    active_lifecycle_row(
        tx,
        list_id,
        AddonMediaListLifecycleJobKind::ResetReload,
        ManagedLibraryOperationKind::Reset,
    )
    .await
}

async fn active_lifecycle_row(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    list_id: EntityId,
    lifecycle_kind: AddonMediaListLifecycleJobKind,
    operation_kind: ManagedLibraryOperationKind,
) -> Result<Option<crate::db::DbRow>> {
    sqlx::query(
        r#"SELECT "job".*, "operation".*,
                  "job"."Id" AS "LifecycleJobId",
                  "job"."Kind" AS "LifecycleKind", "job"."Phase" AS "LifecyclePhase",
                  "job"."CreatedAt" AS "LifecycleCreatedAt",
                  "job"."UpdatedAt" AS "LifecycleUpdatedAt",
                  "job"."CompletedAt" AS "LifecycleCompletedAt",
                  "operation"."Id" AS "ManagedOperationId"
           FROM "AddonMediaListLifecycleJob" AS "job"
           JOIN "ManagedLibraryOperation" AS "operation"
             ON "operation"."Id" = "job"."OperationId"
           WHERE "job"."AddonMediaListId" = ? AND "job"."Kind" = ?
             AND "operation"."Kind" = ? AND "operation"."State" IN (?, ?, ?)
           ORDER BY "job"."Generation" DESC LIMIT 1"#,
    )
    .bind(list_id)
    .bind(encode_enum(lifecycle_kind))
    .bind(encode_enum(operation_kind))
    .bind(encode_enum(ManagedLibraryOperationState::Queued))
    .bind(encode_enum(ManagedLibraryOperationState::Running))
    .bind(encode_enum(ManagedLibraryOperationState::WaitingToRetry))
    .fetch_optional(&mut **tx)
    .await
    .repository()
}

async fn reset_row_by_job_id(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    job_id: EntityId,
) -> Result<Option<crate::db::DbRow>> {
    sqlx::query(
        r#"SELECT "job".*, "operation".*,
                  "job"."Id" AS "LifecycleJobId",
                  "job"."Kind" AS "LifecycleKind", "job"."Phase" AS "LifecyclePhase",
                  "job"."CreatedAt" AS "LifecycleCreatedAt",
                  "job"."UpdatedAt" AS "LifecycleUpdatedAt",
                  "job"."CompletedAt" AS "LifecycleCompletedAt",
                  "operation"."Id" AS "ManagedOperationId"
           FROM "AddonMediaListLifecycleJob" AS "job"
           JOIN "ManagedLibraryOperation" AS "operation"
             ON "operation"."Id" = "job"."OperationId"
           WHERE "job"."Id" = ?"#,
    )
    .bind(job_id)
    .fetch_optional(&mut **tx)
    .await
    .repository()
}

fn decode_reset_row(row: &crate::db::DbRow, existing: bool) -> Result<AddonMediaListResetEnqueue> {
    Ok(AddonMediaListResetEnqueue {
        job: decode_reset_job_row_with_id(row, "LifecycleJobId")?,
        operation: decode_operation_row_with_id(row, "ManagedOperationId")?,
        existing,
    })
}

fn decode_reconcile_row(
    row: &crate::db::DbRow,
    existing: bool,
) -> Result<AddonMediaListReconcileEnqueue> {
    Ok(AddonMediaListReconcileEnqueue {
        job: decode_reset_job_row_with_id(row, "LifecycleJobId")?,
        operation: decode_operation_row_with_id(row, "ManagedOperationId")?,
        existing,
    })
}

fn decode_reset_job_row(row: &crate::db::DbRow) -> Result<AddonMediaListLifecycleJob> {
    decode_reset_job_row_with_id(row, "Id")
}

fn decode_reset_job_row_with_id(
    row: &crate::db::DbRow,
    id_column: &'static str,
) -> Result<AddonMediaListLifecycleJob> {
    let joined = id_column == "LifecycleJobId";
    let kind_column = if joined { "LifecycleKind" } else { "Kind" };
    let phase_column = if joined { "LifecyclePhase" } else { "Phase" };
    let created_column = if joined {
        "LifecycleCreatedAt"
    } else {
        "CreatedAt"
    };
    let updated_column = if joined {
        "LifecycleUpdatedAt"
    } else {
        "UpdatedAt"
    };
    let completed_column = if joined {
        "LifecycleCompletedAt"
    } else {
        "CompletedAt"
    };
    Ok(AddonMediaListLifecycleJob {
        id: row.try_get(id_column).repository()?,
        operation_id: row.try_get("OperationId").repository()?,
        list_id: row.try_get("AddonMediaListId").repository()?,
        generation: row.try_get("Generation").repository()?,
        kind: decode_enum_column("AddonMediaListLifecycleJobKind", row, kind_column)?,
        phase: decode_enum_column("AddonMediaListLifecyclePhase", row, phase_column)?,
        saved_draft_revision: row.try_get("SavedDraftRevision").repository()?,
        saved_published_revision: row.try_get("SavedPublishedRevision").repository()?,
        candidate_import_job_id: row.try_get("CandidateImportJobId").repository()?,
        created_at: decode_datetime(
            "add-on media-list lifecycle creation",
            row.try_get::<String, _>(created_column)
                .repository()?
                .as_str(),
        )?,
        updated_at: decode_datetime(
            "add-on media-list lifecycle update",
            row.try_get::<String, _>(updated_column)
                .repository()?
                .as_str(),
        )?,
        completed_at: decode_optional_datetime(
            "add-on media-list lifecycle completion",
            row.try_get(completed_column).repository()?,
        )?,
    })
}

async fn next_publication_revision_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    list_id: EntityId,
    current_published_revision: i32,
) -> Result<i32> {
    let retained_revision: Option<i32> = sqlx::query_scalar(
        r#"SELECT MAX("Revision") FROM "AddonMediaListRevision"
           WHERE "AddonMediaListId" = ?"#,
    )
    .bind(list_id)
    .fetch_one(&mut **tx)
    .await
    .repository()?;
    current_published_revision
        .max(retained_revision.unwrap_or(current_published_revision))
        .checked_add(1)
        .ok_or_else(|| {
            RepositoryError::Conflict(
                "the published add-on media-list revision is exhausted".into(),
            )
        })
}

async fn active_publication_row(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    list_id: EntityId,
) -> Result<Option<crate::db::DbRow>> {
    sqlx::query(
        r#"SELECT "job".*, "operation".*,
                  "job"."Id" AS "PublicationJobId",
                  "operation"."Id" AS "ManagedOperationId"
           FROM "AddonMediaListPublicationJob" AS "job"
           JOIN "ManagedLibraryOperation" AS "operation"
             ON "operation"."Id" = "job"."OperationId"
           WHERE "job"."AddonMediaListId" = ?
             AND "operation"."State" IN (?, ?, ?)
           ORDER BY "job"."Generation" DESC LIMIT 1"#,
    )
    .bind(list_id)
    .bind(encode_enum(ManagedLibraryOperationState::Queued))
    .bind(encode_enum(ManagedLibraryOperationState::Running))
    .bind(encode_enum(ManagedLibraryOperationState::WaitingToRetry))
    .fetch_optional(&mut **tx)
    .await
    .repository()
}

async fn completed_publication_row(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    list_id: EntityId,
    saved_revision: i32,
    published_revision: i32,
) -> Result<Option<crate::db::DbRow>> {
    sqlx::query(
        r#"SELECT "job".*, "operation".*,
                  "job"."Id" AS "PublicationJobId",
                  "operation"."Id" AS "ManagedOperationId"
           FROM "AddonMediaListPublicationJob" AS "job"
           JOIN "ManagedLibraryOperation" AS "operation"
             ON "operation"."Id" = "job"."OperationId"
           WHERE "job"."AddonMediaListId" = ?
             AND "job"."SavedRevision" = ?
             AND "job"."TargetPublishedRevision" = ?
             AND "job"."Outcome" = ?
             AND "operation"."State" = ?
           ORDER BY "job"."Generation" DESC LIMIT 1"#,
    )
    .bind(list_id)
    .bind(saved_revision)
    .bind(published_revision)
    .bind(encode_enum(AddonMediaListPublicationOutcome::Published))
    .bind(encode_enum(ManagedLibraryOperationState::Completed))
    .fetch_optional(&mut **tx)
    .await
    .repository()
}

fn publication_select() -> sqlx::query::Query<'static, crate::db::Db, sqlx::sqlite::SqliteArguments>
{
    sqlx::query(
        r#"SELECT "job".*, "operation".*,
                  "job"."Id" AS "PublicationJobId",
                  "operation"."Id" AS "ManagedOperationId"
           FROM "AddonMediaListPublicationJob" AS "job"
           JOIN "ManagedLibraryOperation" AS "operation"
             ON "operation"."Id" = "job"."OperationId"
           WHERE "job"."OperationId" = ?"#,
    )
}

fn decode_publication_row(
    row: &crate::db::DbRow,
) -> Result<(AddonMediaListPublicationJob, ManagedLibraryOperation)> {
    let operation = decode_operation_row_with_id(row, "ManagedOperationId")?;
    let outcome = match row.try_get::<Option<i32>, _>("Outcome").repository()? {
        Some(value) => Some(
            <AddonMediaListPublicationOutcome as crate::repositories::common::DbEnum>::from_db_int(
                value,
            )
            .ok_or_else(|| RepositoryError::Decode {
                kind: "add-on publication outcome",
                value: value.to_string(),
            })?,
        ),
        None => None,
    };
    Ok((
        AddonMediaListPublicationJob {
            id: row.try_get("PublicationJobId").repository()?,
            operation_id: operation.id,
            list_id: row.try_get("AddonMediaListId").repository()?,
            generation: row.try_get("Generation").repository()?,
            saved_revision: row.try_get("SavedRevision").repository()?,
            target_published_revision: row.try_get("TargetPublishedRevision").repository()?,
            source_published_revision: row.try_get("SourcePublishedRevision").repository()?,
            restore_materialized_at: decode_optional_datetime(
                "AddonMediaListPublicationJob.RestoreMaterializedAt",
                row.try_get("RestoreMaterializedAt").repository()?,
            )?,
            phase: decode_enum_column("AddonMediaListPublicationPhase", row, "Phase")?,
            child_scan_operation_id: row.try_get("ChildScanOperationId").repository()?,
            original_failure_code: row.try_get("OriginalFailureCode").repository()?,
            original_failure_diagnostic: row.try_get("OriginalFailureDiagnostic").repository()?,
            recovery_diagnostic: row.try_get("RecoveryDiagnostic").repository()?,
            outcome,
        },
        operation,
    ))
}

fn decode_operation_row(row: &crate::db::DbRow) -> Result<ManagedLibraryOperation> {
    decode_operation_row_with_id(row, "Id")
}

fn decode_operation_row_with_id(
    row: &crate::db::DbRow,
    id_column: &'static str,
) -> Result<ManagedLibraryOperation> {
    let provider = match row.try_get::<Option<i32>, _>("Provider").repository()? {
        Some(value) => Some(
            <MediaSourceKind as crate::repositories::common::DbEnum>::from_db_int(value)
                .ok_or_else(|| RepositoryError::Decode {
                    kind: "managed-library provider",
                    value: value.to_string(),
                })?,
        ),
        None => None,
    };
    let show_id = row.try_get("ShowId").repository()?;
    let deep_scan = decode_bool_column(row, "DeepScan")?;
    Ok(ManagedLibraryOperation {
        id: row.try_get(id_column).repository()?,
        library_id: row.try_get("LibraryId").repository()?,
        parent_operation_id: row.try_get("ParentOperationId").repository()?,
        owner_token: row.try_get("OwnerToken").repository()?,
        kind: decode_enum_column("ManagedLibraryOperationKind", row, "Kind")?,
        state: decode_enum_column("ManagedLibraryOperationState", row, "State")?,
        scan: provider.map(|provider| ManagedLibraryScanSpec {
            provider,
            show_id,
            deep_scan,
        }),
        attempt_count: row.try_get("AttemptCount").repository()?,
        next_eligible_at: decode_optional_datetime(
            "managed-library next eligibility",
            row.try_get("NextEligibleAt").repository()?,
        )?,
    })
}

async fn claim_is_current(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    claim: &ManagedLibraryOperationClaim,
) -> Result<bool> {
    let now = encode_datetime(Utc::now());
    let lease_operation_id = claim
        .operation
        .parent_operation_id
        .unwrap_or(claim.operation.id);
    let count = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM "ManagedLibraryLease"
           WHERE "OperationId" = ? AND "OwnerToken" = ? AND "LeaseOwner" = ?
             AND "LeaseExpiresAt" > ?"#,
    )
    .bind(lease_operation_id)
    .bind(&claim.operation.owner_token)
    .bind(&claim.lease_owner)
    .bind(now)
    .fetch_one(&mut **tx)
    .await
    .repository()?;
    Ok(count == 1)
}

async fn finish_operation(
    pool: &DbPool,
    claim: &ManagedLibraryOperationClaim,
    state: ManagedLibraryOperationState,
    diagnostic: Option<(&str, String)>,
) -> Result<bool> {
    let now = encode_datetime(Utc::now());
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    if !claim_is_current(&mut tx, claim).await? {
        return Ok(false);
    }
    finish_operation_tx(&mut tx, claim, state, diagnostic, &now).await?;
    tx.commit().await.repository()?;
    Ok(true)
}

#[allow(clippy::too_many_lines)] // Admission and restart recovery share one transaction.
async fn ensure_reset_child_scan(
    pool: &DbPool,
    claim: &ManagedLibraryOperationClaim,
    list_id: EntityId,
) -> Result<ManagedLibraryOperation> {
    let now = encode_datetime(Utc::now());
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    if !claim_is_current(&mut tx, claim).await? {
        return Err(RepositoryError::Conflict(
            "the managed-library lease changed before the reset child scan".to_owned(),
        ));
    }
    let owns_job: bool = sqlx::query_scalar(
        r#"SELECT EXISTS (
               SELECT 1 FROM "AddonMediaListLifecycleJob"
               WHERE "OperationId" = ? AND "AddonMediaListId" = ? AND "Kind" = ?
                 AND "Phase" = ?
           )"#,
    )
    .bind(claim.operation.id)
    .bind(list_id)
    .bind(encode_enum(AddonMediaListLifecycleJobKind::ResetReload))
    .bind(encode_enum(
        AddonMediaListLifecyclePhase::UnpublishingProjection,
    ))
    .fetch_one(&mut *tx)
    .await
    .repository()?;
    if !owns_job {
        return Err(RepositoryError::Conflict(
            "the reset phase changed before its child scan".to_owned(),
        ));
    }
    let existing = sqlx::query(
        r#"SELECT * FROM "ManagedLibraryOperation"
           WHERE "ParentOperationId" = ? AND "Kind" = ? ORDER BY "Id" LIMIT 1"#,
    )
    .bind(claim.operation.id)
    .bind(encode_enum(
        ManagedLibraryOperationKind::PublicationChildScan,
    ))
    .fetch_optional(&mut *tx)
    .await
    .repository()?;
    if let Some(row) = existing {
        let mut operation = decode_operation_row(&row)?;
        if operation.state != ManagedLibraryOperationState::Completed {
            let updated = sqlx::query(
                r#"UPDATE "ManagedLibraryOperation"
                   SET "State" = ?, "AttemptCount" = "AttemptCount" + 1,
                       "LeaseOwner" = ?,
                       "LeaseExpiresAt" = (SELECT "LeaseExpiresAt" FROM "ManagedLibraryLease"
                                            WHERE "OperationId" = ?),
                       "HeartbeatAt" = ?, "DiagnosticCode" = NULL, "Diagnostic" = NULL,
                       "CompletedAt" = NULL, "UpdatedAt" = ?
                   WHERE "Id" = ? AND "ParentOperationId" = ? AND "OwnerToken" = ?"#,
            )
            .bind(encode_enum(ManagedLibraryOperationState::Running))
            .bind(&claim.lease_owner)
            .bind(claim.operation.id)
            .bind(&now)
            .bind(&now)
            .bind(operation.id)
            .bind(claim.operation.id)
            .bind(&claim.operation.owner_token)
            .execute(&mut *tx)
            .await
            .repository()?;
            if updated.rows_affected() != 1 {
                return Err(RepositoryError::Conflict(
                    "the reset child scan changed before recovery".to_owned(),
                ));
            }
            operation.state = ManagedLibraryOperationState::Running;
            operation.attempt_count = operation.attempt_count.saturating_add(1);
        }
        tx.commit().await.repository()?;
        return Ok(operation);
    }
    let result = sqlx::query(
        r#"INSERT INTO "ManagedLibraryOperation" (
               "LibraryId", "ParentOperationId", "OwnerToken", "Kind", "State",
               "Provider", "DeepScan", "AttemptCount", "LeaseOwner", "LeaseExpiresAt",
               "HeartbeatAt", "CreatedAt", "UpdatedAt"
           ) VALUES (?, ?, ?, ?, ?, ?, 1, 1, ?,
                     (SELECT "LeaseExpiresAt" FROM "ManagedLibraryLease" WHERE "OperationId" = ?),
                     ?, ?, ?)"#,
    )
    .bind(claim.operation.library_id)
    .bind(claim.operation.id)
    .bind(&claim.operation.owner_token)
    .bind(encode_enum(
        ManagedLibraryOperationKind::PublicationChildScan,
    ))
    .bind(encode_enum(ManagedLibraryOperationState::Running))
    .bind(encode_enum(MediaSourceKind::Local))
    .bind(&claim.lease_owner)
    .bind(claim.operation.id)
    .bind(&now)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .repository()?;
    let id = inserted_id(&mut tx, result).await.repository()?;
    tx.commit().await.repository()?;
    Ok(ManagedLibraryOperation {
        id,
        library_id: claim.operation.library_id,
        parent_operation_id: Some(claim.operation.id),
        owner_token: claim.operation.owner_token.clone(),
        kind: ManagedLibraryOperationKind::PublicationChildScan,
        state: ManagedLibraryOperationState::Running,
        scan: Some(ManagedLibraryScanSpec {
            provider: MediaSourceKind::Local,
            show_id: None,
            deep_scan: true,
        }),
        attempt_count: 1,
        next_eligible_at: None,
    })
}

async fn finish_operation_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    claim: &ManagedLibraryOperationClaim,
    state: ManagedLibraryOperationState,
    diagnostic: Option<(&str, String)>,
    now: &str,
) -> Result<()> {
    let (code, message) =
        diagnostic.map_or((None, None), |(code, message)| (Some(code), Some(message)));
    sqlx::query(
        r#"UPDATE "ManagedLibraryOperation"
           SET "State" = ?, "DiagnosticCode" = ?, "Diagnostic" = ?,
               "LeaseOwner" = NULL, "LeaseExpiresAt" = NULL,
               "UpdatedAt" = ?, "CompletedAt" = ?
           WHERE "Id" = ? AND "OwnerToken" = ? AND "LeaseOwner" = ?"#,
    )
    .bind(encode_enum(state))
    .bind(code)
    .bind(message)
    .bind(now)
    .bind(now)
    .bind(claim.operation.id)
    .bind(&claim.operation.owner_token)
    .bind(&claim.lease_owner)
    .execute(&mut **tx)
    .await
    .repository()?;
    if let Some(parent_operation_id) = claim.operation.parent_operation_id {
        let phase = publication_phase_tx(tx, parent_operation_id).await?;
        let kind = if state == ManagedLibraryOperationState::Completed {
            AddonMediaListPublicationEventKind::ChildScanCompleted
        } else {
            AddonMediaListPublicationEventKind::ChildScanFailed
        };
        append_publication_event_for_operation_tx(
            tx,
            parent_operation_id,
            &format!(
                "child:{}:terminal:{}:{}",
                claim.operation.id,
                claim.operation.attempt_count,
                encode_enum(state)
            ),
            PublicationEventWrite {
                kind,
                phase,
                attempt: Some(claim.operation.attempt_count),
                related_operation_id: Some(claim.operation.id),
                occurred_at: now,
                ..PublicationEventWrite::default()
            },
        )
        .await?;
    }
    if claim.operation.parent_operation_id.is_none() {
        sqlx::query(
            r#"DELETE FROM "ManagedLibraryLease"
               WHERE "OperationId" = ? AND "OwnerToken" = ? AND "LeaseOwner" = ?"#,
        )
        .bind(claim.operation.id)
        .bind(&claim.operation.owner_token)
        .bind(&claim.lease_owner)
        .execute(&mut **tx)
        .await
        .repository()?;
    }
    Ok(())
}

fn bounded_diagnostic(value: &str) -> String {
    value.chars().take(512).collect()
}

/// Reuse unstarted work at the latest revision; never retarget a claimed worker.
async fn refresh_queued_reconcile(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    enqueue: AddonMediaListReconcileEnqueue,
    revision: i32,
    published: i32,
    now: &str,
) -> Result<AddonMediaListReconcileEnqueue> {
    if enqueue.job.saved_draft_revision != revision
        || enqueue.job.saved_published_revision != published
    {
        if enqueue.operation.state != ManagedLibraryOperationState::Queued
            || enqueue.job.phase != AddonMediaListLifecyclePhase::AwaitingLease
            || enqueue.job.candidate_import_job_id.is_some()
        {
            return Err(RepositoryError::Conflict(
                "the running reconciliation still owns its saved revision".to_owned(),
            ));
        }
        sqlx::query(
            r#"UPDATE "AddonMediaListLifecycleJob"
                SET "SavedDraftRevision"=?, "SavedPublishedRevision"=?, "UpdatedAt"=?
                WHERE "Id"=?"#,
        )
        .bind(revision)
        .bind(published)
        .bind(now)
        .bind(enqueue.job.id)
        .execute(&mut **tx)
        .await
        .repository()?;
        let row = reset_row_by_job_id(tx, enqueue.job.id)
            .await?
            .ok_or_else(|| {
                RepositoryError::Conflict("the queued reconciliation disappeared".to_owned())
            })?;
        return decode_reconcile_row(&row, true);
    }
    Ok(enqueue)
}

/// Admit reconciliation in the same transaction as the operator chapter intent.
#[expect(
    clippy::too_many_lines,
    reason = "Admission keeps revision, library and operation checks in one transaction"
)]
pub(crate) async fn enqueue_reconcile_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    list_id: EntityId,
) -> Result<AddonMediaListReconcileEnqueue> {
    let now = encode_datetime(Utc::now());
    let list = sqlx::query(
        r#"SELECT "list"."DraftRevision", "list"."PublishedRevision",
                      "binding"."LibraryId"
               FROM "AddonMediaList" AS "list"
               JOIN "AddonMediaListLibrary" AS "binding"
                 ON "binding"."AddonMediaListId" = "list"."Id"
               WHERE "list"."Id" = ?"#,
    )
    .bind(list_id)
    .fetch_optional(&mut **tx)
    .await
    .repository()?
    .ok_or(RepositoryError::NotFound {
        kind: "add-on media list",
        id: list_id,
    })?;
    if let Some(row) = active_lifecycle_row(
        tx,
        list_id,
        AddonMediaListLifecycleJobKind::Reconcile,
        ManagedLibraryOperationKind::Reconcile,
    )
    .await?
    {
        return refresh_queued_reconcile(
            tx,
            decode_reconcile_row(&row, true)?,
            list.try_get("DraftRevision").repository()?,
            list.try_get("PublishedRevision").repository()?,
            &now,
        )
        .await;
    }
    let library_id: EntityId = list.try_get("LibraryId").repository()?;
    let competing_operation: bool = sqlx::query_scalar(
        r#"SELECT EXISTS (
                   SELECT 1 FROM "ManagedLibraryOperation"
                   WHERE "LibraryId" = ? AND "ParentOperationId" IS NULL
                     AND "State" IN (?, ?, ?)
               )"#,
    )
    .bind(library_id)
    .bind(encode_enum(ManagedLibraryOperationState::Queued))
    .bind(encode_enum(ManagedLibraryOperationState::Running))
    .bind(encode_enum(ManagedLibraryOperationState::WaitingToRetry))
    .fetch_one(&mut **tx)
    .await
    .repository()?;
    if competing_operation {
        return Err(RepositoryError::Conflict(
            "the managed add-on media-list already has an active operation".to_owned(),
        ));
    }
    let generation: i32 = sqlx::query_scalar(
        r#"SELECT COALESCE(MAX("Generation"), 0) + 1
               FROM "AddonMediaListLifecycleJob" WHERE "AddonMediaListId" = ?"#,
    )
    .bind(list_id)
    .fetch_one(&mut **tx)
    .await
    .repository()?;
    let owner_token = Uuid::new_v4().to_string();
    let operation_result = sqlx::query(
        r#"INSERT INTO "ManagedLibraryOperation" (
                   "LibraryId", "OwnerToken", "Kind", "State", "CreatedAt", "UpdatedAt"
               ) VALUES (?, ?, ?, ?, ?, ?)"#,
    )
    .bind(library_id)
    .bind(&owner_token)
    .bind(encode_enum(ManagedLibraryOperationKind::Reconcile))
    .bind(encode_enum(ManagedLibraryOperationState::Queued))
    .bind(&now)
    .bind(&now)
    .execute(&mut **tx)
    .await
    .repository()?;
    let operation_id = inserted_id(tx, operation_result).await.repository()?;
    let lifecycle_result = sqlx::query(
        r#"INSERT INTO "AddonMediaListLifecycleJob" (
                   "OperationId", "AddonMediaListId", "Generation", "Kind", "Phase",
                   "SavedDraftRevision", "SavedPublishedRevision", "CreatedAt", "UpdatedAt"
               ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(operation_id)
    .bind(list_id)
    .bind(generation)
    .bind(encode_enum(AddonMediaListLifecycleJobKind::Reconcile))
    .bind(encode_enum(AddonMediaListLifecyclePhase::AwaitingLease))
    .bind(list.try_get::<i32, _>("DraftRevision").repository()?)
    .bind(list.try_get::<i32, _>("PublishedRevision").repository()?)
    .bind(&now)
    .bind(&now)
    .execute(&mut **tx)
    .await
    .repository()?;
    let job_id = inserted_id(tx, lifecycle_result).await.repository()?;
    let row = reset_row_by_job_id(tx, job_id).await?.ok_or_else(|| {
        RepositoryError::Conflict("the reconcile admission disappeared".to_owned())
    })?;
    let enqueue = decode_reconcile_row(&row, false)?;
    Ok(enqueue)
}

#[expect(
    clippy::too_many_lines,
    reason = "One transaction validates confirmation and admits exactly one reset generation"
)]
async fn enqueue_reset_reload(
    pool: &DbPool,
    list_id: EntityId,
    confirmed_name: &str,
    expected_revision: Option<i32>,
) -> Result<AddonMediaListResetEnqueue> {
    let now = encode_datetime(Utc::now());
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    let list = sqlx::query(
        r#"SELECT "list"."Name", "list"."DraftRevision", "list"."PublishedRevision",
                      "binding"."LibraryId"
               FROM "AddonMediaList" AS "list"
               JOIN "AddonMediaListLibrary" AS "binding"
                 ON "binding"."AddonMediaListId" = "list"."Id"
               WHERE "list"."Id" = ?"#,
    )
    .bind(list_id)
    .fetch_optional(&mut *tx)
    .await
    .repository()?
    .ok_or(RepositoryError::NotFound {
        kind: "add-on media list",
        id: list_id,
    })?;
    let revision: i32 = list.try_get("DraftRevision").repository()?;
    if expected_revision.is_some_and(|expected| expected != revision) {
        return Err(RepositoryError::Conflict(
            "the saved draft changed before reset confirmation".into(),
        ));
    }
    let name: String = list.try_get("Name").repository()?;
    if name != confirmed_name {
        return Err(RepositoryError::Conflict(
            "the confirmation does not match the current add-on media-list name".to_owned(),
        ));
    }
    let library_id: EntityId = list.try_get("LibraryId").repository()?;
    if let Some(row) = active_reset_row(&mut tx, list_id).await? {
        let enqueue = decode_reset_row(&row, true)?;
        tx.commit().await.repository()?;
        return Ok(enqueue);
    }
    let competing_operation: bool = sqlx::query_scalar(
        r#"SELECT EXISTS (
                   SELECT 1 FROM "ManagedLibraryOperation"
                   WHERE "LibraryId" = ? AND "ParentOperationId" IS NULL
                     AND "State" IN (?, ?, ?)
               )"#,
    )
    .bind(library_id)
    .bind(encode_enum(ManagedLibraryOperationState::Queued))
    .bind(encode_enum(ManagedLibraryOperationState::Running))
    .bind(encode_enum(ManagedLibraryOperationState::WaitingToRetry))
    .fetch_one(&mut *tx)
    .await
    .repository()?;
    if competing_operation {
        return Err(RepositoryError::Conflict(
            "the managed add-on media-list already has an active operation".to_owned(),
        ));
    }
    let generation: i32 = sqlx::query_scalar(
        r#"SELECT COALESCE(MAX("Generation"), 0) + 1
               FROM "AddonMediaListLifecycleJob" WHERE "AddonMediaListId" = ?"#,
    )
    .bind(list_id)
    .fetch_one(&mut *tx)
    .await
    .repository()?;
    let owner_token = Uuid::new_v4().to_string();
    let operation_result = sqlx::query(
        r#"INSERT INTO "ManagedLibraryOperation" (
                   "LibraryId", "OwnerToken", "Kind", "State", "CreatedAt", "UpdatedAt"
               ) VALUES (?, ?, ?, ?, ?, ?)"#,
    )
    .bind(library_id)
    .bind(&owner_token)
    .bind(encode_enum(ManagedLibraryOperationKind::Reset))
    .bind(encode_enum(ManagedLibraryOperationState::Queued))
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .repository()?;
    let operation_id = inserted_id(&mut tx, operation_result).await.repository()?;
    let lifecycle_result = sqlx::query(
        r#"INSERT INTO "AddonMediaListLifecycleJob" (
                   "OperationId", "AddonMediaListId", "Generation", "Kind", "Phase",
                   "SavedDraftRevision", "SavedPublishedRevision", "CreatedAt", "UpdatedAt"
               ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(operation_id)
    .bind(list_id)
    .bind(generation)
    .bind(encode_enum(AddonMediaListLifecycleJobKind::ResetReload))
    .bind(encode_enum(AddonMediaListLifecyclePhase::AwaitingLease))
    .bind(list.try_get::<i32, _>("DraftRevision").repository()?)
    .bind(list.try_get::<i32, _>("PublishedRevision").repository()?)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .repository()?;
    let job_id = inserted_id(&mut tx, lifecycle_result).await.repository()?;
    let row = reset_row_by_job_id(&mut tx, job_id)
        .await?
        .ok_or_else(|| RepositoryError::Conflict("the reset admission disappeared".to_owned()))?;
    let enqueue = decode_reset_row(&row, false)?;
    tx.commit().await.repository()?;
    Ok(enqueue)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_core::repositories::AddonMediaListLifecycleRepository as _;

    async fn pool_with_library() -> DbPool {
        pool_with_library_at("sqlite::memory:").await
    }

    async fn pool_with_library_at(database_url: &str) -> DbPool {
        let pool = crate::db::init(database_url)
            .await
            .expect("migrate operation test database");
        sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
            .execute(&pool)
            .await
            .expect("seed media source");
        sqlx::query(
            r#"INSERT INTO "Library" ("Id", "MediaKind", "MediaSourceId", "Name")
               VALUES (1, 1, 1, 'Managed')"#,
        )
        .execute(&pool)
        .await
        .expect("seed library");
        pool
    }

    fn scan() -> ManagedLibraryScanSpec {
        ManagedLibraryScanSpec {
            provider: MediaSourceKind::Local,
            show_id: None,
            deep_scan: true,
        }
    }

    #[tokio::test]
    async fn one_library_lease_defers_follow_up_and_expiry_reclaims_the_owner() {
        let pool = pool_with_library().await;
        let repository = SqliteManagedLibraryOperationRepository(pool);
        let first = repository
            .enqueue_scan(ManagedLibraryOperationKind::ManualScan, 1, scan(), false)
            .await
            .expect("enqueue first")
            .expect("first operation");
        let second = repository
            .enqueue_scan(ManagedLibraryOperationKind::ManualScan, 1, scan(), false)
            .await
            .expect("enqueue second")
            .expect("second operation");
        let started = Utc::now();
        let first_claim = repository
            .claim_next(started)
            .await
            .expect("claim first")
            .expect("first claim");
        assert_eq!(first_claim.operation.id, first.id);
        assert!(repository
            .claim_next(started + Duration::seconds(30))
            .await
            .expect("blocked claim")
            .is_none());
        assert!(!repository
            .heartbeat(&first_claim, started + Duration::seconds(91))
            .await
            .expect("an expired lease cannot be revived"));

        let reclaimed = repository
            .claim_next(started + Duration::seconds(91))
            .await
            .expect("reclaim expired")
            .expect("expired owner reclaimed");
        assert_eq!(reclaimed.operation.id, first.id);
        assert_ne!(reclaimed.lease_owner, first_claim.lease_owner);
        assert!(!repository
            .complete_operation(&first_claim)
            .await
            .expect("stale completion is fenced"));
        assert!(repository
            .complete_operation(&reclaimed)
            .await
            .expect("complete reclaimed owner"));
        let follow_up = repository
            .claim_next(started + Duration::seconds(92))
            .await
            .expect("claim follow-up")
            .expect("follow-up claim");
        assert_eq!(follow_up.operation.id, second.id);
    }

    #[tokio::test]
    async fn provider_refresh_coalesces_while_an_existing_refresh_is_active() {
        let pool = pool_with_library().await;
        let repository = SqliteManagedLibraryOperationRepository(pool);

        let first = repository
            .enqueue_scan(
                ManagedLibraryOperationKind::ProviderRefresh,
                1,
                scan(),
                true,
            )
            .await
            .expect("enqueue first provider refresh");
        let duplicate = repository
            .enqueue_scan(
                ManagedLibraryOperationKind::ProviderRefresh,
                1,
                scan(),
                true,
            )
            .await
            .expect("coalesce duplicate provider refresh");

        let first = first.expect("first provider refresh");
        assert!(duplicate.is_none());
        assert_eq!(
            repository
                .active_scan_operation(1)
                .await
                .expect("load coalesced operation")
                .expect("active provider refresh")
                .id,
            first.id
        );
    }

    #[tokio::test]
    async fn duplicate_publication_click_returns_the_same_exact_revision_generation() {
        let pool = pool_with_library().await;
        sqlx::query(
            r#"INSERT INTO "AddonInstallation" (
                   "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
                   "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt"
               ) VALUES ('org.example.list', '1.0.0', 1, 'package', 'content', '{}',
                         'addons/example', '2026-09-02T00:00:00Z', '2026-09-02T00:00:00Z')"#,
        )
        .execute(&pool)
        .await
        .expect("seed installation");
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" (
                   "Id", "AddonId", "ProviderId", "SourceUrl", "Name",
                   "DraftRevision", "PublishedRevision"
               ) VALUES (1, 'org.example.list', 'list', 'https://example.invalid/list',
                         'List', 3, 1)"#,
        )
        .execute(&pool)
        .await
        .expect("seed list");
        let repository = SqliteManagedLibraryOperationRepository(pool);
        let first = repository
            .enqueue_publication(1, 1, 3, 2)
            .await
            .expect("enqueue publication");
        let duplicate = repository
            .enqueue_publication(1, 1, 3, 2)
            .await
            .expect("deduplicate publication");
        assert!(!first.existing);
        assert!(duplicate.existing);
        assert_eq!(duplicate.operation.id, first.operation.id);
        assert_eq!(duplicate.job.generation, first.job.generation);
        assert_eq!(duplicate.job.saved_revision, 3);
        let count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "AddonMediaListPublicationJob"
               WHERE "AddonMediaListId" = 1"#,
        )
        .fetch_one(&repository.0)
        .await
        .expect("count jobs");
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn revision_restore_accepts_any_valid_retained_snapshot_without_replacing_the_draft() {
        let pool = pool_with_library().await;
        seed_publication_list(&pool, "org.example.restore-revision").await;
        sqlx::query(
            r#"INSERT INTO "AddonMediaListLibrary" (
                   "AddonMediaListId", "LibraryId", "FolderName", "ManifestPath"
               ) VALUES (1, 1, 'list', 'managed/list/playlist.json')"#,
        )
        .execute(&pool)
        .await
        .expect("seed managed list binding");
        let lifecycle = crate::repositories::SqliteAddonMediaListRepository(pool.clone());
        for revision in 1..=4 {
            sqlx::query(
                r#"UPDATE "AddonMediaList" SET "DraftRevision" = ?, "PublishedRevision" = ?
                   WHERE "Id" = 1"#,
            )
            .bind(revision)
            .bind(revision)
            .execute(&pool)
            .await
            .expect("advance retained revision");
            lifecycle
                .retain_published_revision(
                    1,
                    revision,
                    revision,
                    Utc::now() + Duration::minutes(i64::from(revision)),
                    &[],
                    &ersatzrs_core::repositories::AddonMediaListRevisionArtifacts::default(),
                )
                .await
                .expect("retain valid revision snapshot");
        }
        sqlx::query(
            r#"UPDATE "AddonMediaListRevision" SET "SnapshotDigest" = 'tampered'
               WHERE "AddonMediaListId" = 1 AND "Revision" = 2"#,
        )
        .execute(&pool)
        .await
        .expect("corrupt one retained revision");
        sqlx::query(r#"UPDATE "AddonMediaList" SET "PublishedRevision" = 3 WHERE "Id" = 1"#)
            .execute(&pool)
            .await
            .expect("retain a future immutable revision after recovery");

        let repository = SqliteManagedLibraryOperationRepository(pool);
        assert!(matches!(
            repository.enqueue_revision_restore(1, 1, 3, 4, 3).await,
            Err(RepositoryError::Conflict(_))
        ));
        assert!(matches!(
            repository.enqueue_revision_restore(1, 1, 2, 4, 3).await,
            Err(RepositoryError::Conflict(_))
        ));
        let queued = repository
            .enqueue_revision_restore(1, 1, 1, 4, 3)
            .await
            .expect("queue revision restore");
        let duplicate = repository
            .enqueue_revision_restore(1, 1, 1, 4, 3)
            .await
            .expect("coalesce revision restore");

        assert!(!queued.existing);
        assert!(duplicate.existing);
        assert_eq!(duplicate.operation.id, queued.operation.id);
        assert_eq!(queued.job.saved_revision, 5);
        assert_eq!(queued.job.target_published_revision, 5);
        assert_eq!(queued.job.source_published_revision, Some(1));
        assert_eq!(queued.job.restore_materialized_at, None);
        let draft_revision: i32 =
            sqlx::query_scalar(r#"SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id" = 1"#)
                .fetch_one(&repository.0)
                .await
                .expect("load unchanged draft revision");
        assert_eq!(draft_revision, 4);
    }

    #[tokio::test]
    async fn publication_history_records_only_a_real_competing_lease_owner() {
        let pool = pool_with_library().await;
        seed_publication_list(&pool, "org.example.lease-wait").await;
        let repository = SqliteManagedLibraryOperationRepository(pool);
        let scan = repository
            .enqueue_scan(ManagedLibraryOperationKind::ManualScan, 1, scan(), false)
            .await
            .expect("enqueue scan")
            .expect("scan operation");
        repository
            .enqueue_publication(1, 1, 3, 2)
            .await
            .expect("enqueue publication");
        let claimed_at = Utc::now();
        let stale_updated_at = encode_datetime(claimed_at - Duration::hours(1));
        sqlx::query(
            r#"UPDATE "AddonMediaListPublicationJob" SET "UpdatedAt" = ?
               WHERE "AddonMediaListId" = 1"#,
        )
        .bind(stale_updated_at)
        .execute(&repository.0)
        .await
        .expect("age queued publication activity");
        let before_claim = repository
            .publication_history(1, claimed_at)
            .await
            .expect("history before lease");
        assert!(!before_claim.jobs[0]
            .events
            .iter()
            .any(|event| event.kind == AddonMediaListPublicationEventKind::LeaseWaitStarted));

        let claimed = repository
            .claim_next(claimed_at)
            .await
            .expect("claim scan")
            .expect("scan claim");
        assert_eq!(claimed.operation.id, scan.id);
        let first_activity: String = sqlx::query_scalar(
            r#"SELECT "UpdatedAt" FROM "AddonMediaListPublicationJob"
               WHERE "AddonMediaListId" = 1"#,
        )
        .fetch_one(&repository.0)
        .await
        .expect("load first lease-wait activity");
        assert_eq!(first_activity, encode_datetime(claimed_at));

        let mut replay = crate::db::begin_write(&repository.0)
            .await
            .expect("begin repeated lease-wait transaction");
        record_waiting_publications_for_owner_tx(
            &mut replay,
            1,
            scan.id,
            &encode_datetime(claimed_at + Duration::seconds(1)),
        )
        .await
        .expect("repeat lease wait observation");
        replay.commit().await.expect("commit repeated lease wait");
        let repeated_activity: String = sqlx::query_scalar(
            r#"SELECT "UpdatedAt" FROM "AddonMediaListPublicationJob"
               WHERE "AddonMediaListId" = 1"#,
        )
        .fetch_one(&repository.0)
        .await
        .expect("load repeated lease-wait activity");
        assert_eq!(repeated_activity, first_activity);

        let history = repository
            .publication_history(1, claimed_at + Duration::seconds(1))
            .await
            .expect("history with lease wait");
        let wait = history.jobs[0]
            .events
            .iter()
            .find(|event| event.kind == AddonMediaListPublicationEventKind::LeaseWaitStarted)
            .expect("lease wait event");
        assert_eq!(wait.related_operation_id, Some(scan.id));
    }

    #[tokio::test]
    async fn stale_publication_phase_transition_does_not_duplicate_its_event() {
        let pool = pool_with_library().await;
        seed_publication_list(&pool, "org.example.phase-dedupe").await;
        let repository = SqliteManagedLibraryOperationRepository(pool);
        let queued = repository
            .enqueue_publication(1, 1, 3, 2)
            .await
            .expect("enqueue publication");
        let claim = repository
            .claim_operation(queued.operation.id, Utc::now())
            .await
            .expect("claim publication")
            .expect("publication claim");
        assert!(repository
            .transition_publication(
                &claim,
                AddonMediaListPublicationPhase::AwaitingLease,
                AddonMediaListPublicationPhase::Staging,
            )
            .await
            .expect("advance publication"));
        assert!(!repository
            .transition_publication(
                &claim,
                AddonMediaListPublicationPhase::AwaitingLease,
                AddonMediaListPublicationPhase::Staging,
            )
            .await
            .expect("reject stale transition"));
        let history = repository
            .publication_history(1, Utc::now())
            .await
            .expect("load phase history");
        assert_eq!(
            history.jobs[0]
                .events
                .iter()
                .filter(|event| event.kind == AddonMediaListPublicationEventKind::PhaseAdvanced)
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn publication_phase_rolls_back_when_its_required_event_cannot_persist() {
        let pool = pool_with_library().await;
        seed_publication_list(&pool, "org.example.event-rollback").await;
        let repository = SqliteManagedLibraryOperationRepository(pool.clone());
        let queued = repository
            .enqueue_publication(1, 1, 3, 2)
            .await
            .expect("enqueue publication");
        let claim = repository
            .claim_operation(queued.operation.id, Utc::now())
            .await
            .expect("claim publication")
            .expect("publication claim");
        sqlx::query(
            r#"CREATE TRIGGER "TR_AddonMediaListPublicationEvent_AbortTest"
               BEFORE INSERT ON "AddonMediaListPublicationEvent"
               BEGIN
                   SELECT RAISE(ABORT, 'injected publication event failure');
               END"#,
        )
        .execute(&pool)
        .await
        .expect("install event failure trigger");

        assert!(repository
            .transition_publication(
                &claim,
                AddonMediaListPublicationPhase::AwaitingLease,
                AddonMediaListPublicationPhase::Staging,
            )
            .await
            .is_err());
        sqlx::query(r#"DROP TRIGGER "TR_AddonMediaListPublicationEvent_AbortTest""#)
            .execute(&pool)
            .await
            .expect("remove event failure trigger");
        let job = repository
            .publication_job(queued.operation.id)
            .await
            .expect("load publication job")
            .expect("publication job");
        assert_eq!(job.phase, AddonMediaListPublicationPhase::AwaitingLease);
        let phase_events: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "AddonMediaListPublicationEvent"
               WHERE "PublicationJobId" = ? AND "Kind" = ?"#,
        )
        .bind(queued.job.id)
        .bind(encode_enum(
            AddonMediaListPublicationEventKind::PhaseAdvanced,
        ))
        .fetch_one(&pool)
        .await
        .expect("count phase events");
        assert_eq!(phase_events, 0);
    }

    #[tokio::test]
    async fn delayed_duplicate_after_commit_does_not_create_a_second_generation() {
        let pool = pool_with_library().await;
        sqlx::query(
            r#"INSERT INTO "AddonInstallation" (
                   "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
                   "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt"
               ) VALUES ('org.example.completed', '1.0.0', 1, 'package', 'content', '{}',
                         'addons/completed', '2026-09-02T00:00:00Z', '2026-09-02T00:00:00Z')"#,
        )
        .execute(&pool)
        .await
        .expect("seed installation");
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" (
                   "Id", "AddonId", "ProviderId", "SourceUrl", "Name", "ReviewState",
                   "DraftRevision", "PublishedRevision"
               ) VALUES (1, 'org.example.completed', 'list', 'https://example.invalid/list',
                         'Completed', 1, 3, 1)"#,
        )
        .execute(&pool)
        .await
        .expect("seed list");
        let repository = SqliteManagedLibraryOperationRepository(pool.clone());
        let first = repository
            .enqueue_publication(1, 1, 3, 2)
            .await
            .expect("enqueue publication");
        let claim = repository
            .claim_operation(first.operation.id, Utc::now())
            .await
            .expect("claim publication")
            .expect("publication is claimable");
        sqlx::query(r#"UPDATE "AddonMediaList" SET "PublishedRevision" = 2 WHERE "Id" = 1"#)
            .execute(&pool)
            .await
            .expect("commit exact draft revision");
        assert!(repository
            .complete_publication(&claim, AddonMediaListPublicationOutcome::Published)
            .await
            .expect("complete publication"));

        let duplicate = repository
            .enqueue_publication(1, 1, 3, 3)
            .await
            .expect("deduplicate delayed request");
        assert!(duplicate.existing);
        assert_eq!(duplicate.operation.id, first.operation.id);
        assert_eq!(duplicate.job.generation, first.job.generation);
    }

    #[tokio::test]
    async fn request_cancellation_after_admission_preserves_the_durable_job_and_history() {
        let database_directory = tempfile::tempdir().expect("cancellation database directory");
        let database_path = database_directory.path().join("cancellation.db");
        let database_url = format!("sqlite://{}?mode=rwc", database_path.display());
        let pool = pool_with_library_at(&database_url).await;
        seed_publication_list(&pool, "org.example.request-cancellation").await;
        let repository = SqliteManagedLibraryOperationRepository(pool);

        let request_repository = repository.clone();
        let (admitted_tx, admitted_rx) = tokio::sync::oneshot::channel();
        let request = tokio::spawn(async move {
            let queued = request_repository
                .enqueue_publication(1, 1, 3, 2)
                .await
                .expect("admit durable publication");
            admitted_tx
                .send((queued.operation.id, queued.job.generation))
                .expect("report committed admission");
            std::future::pending::<()>().await;
        });
        let (operation_id, generation) = admitted_rx.await.expect("publication admitted");
        request.abort();
        assert!(request
            .await
            .expect_err("request task is cancelled")
            .is_cancelled());

        repository.0.close().await;
        drop(repository);
        let repository = SqliteManagedLibraryOperationRepository(
            crate::db::init(&database_url)
                .await
                .expect("reopen cancellation database"),
        );
        let operation = repository
            .operation(operation_id)
            .await
            .expect("load durable operation")
            .expect("durable operation survives request cancellation");
        assert_eq!(operation.state, ManagedLibraryOperationState::Queued);
        let history = repository
            .publication_history(1, Utc::now())
            .await
            .expect("load durable causal history");
        let job = history.jobs.first().expect("retained publication job");
        assert_eq!(job.operation_id, operation_id);
        assert_eq!(job.generation, generation);
        assert!(job
            .events
            .iter()
            .any(|event| event.kind == AddonMediaListPublicationEventKind::Enqueued));
    }

    #[tokio::test]
    async fn publication_failure_phase_and_retry_eligibility_survive_reclaim() {
        let database_directory = tempfile::tempdir().expect("recovery database directory");
        let database_path = database_directory.path().join("recovery.db");
        let database_url = format!("sqlite://{}?mode=rwc", database_path.display());
        let pool = pool_with_library_at(&database_url).await;
        seed_publication_list(&pool, "org.example.recovery").await;
        let repository = SqliteManagedLibraryOperationRepository(pool);
        let queued = repository
            .enqueue_publication(1, 1, 3, 2)
            .await
            .expect("enqueue publication");
        let started = Utc::now();
        let claim = repository
            .claim_next(started)
            .await
            .expect("claim publication")
            .expect("publication claim");
        assert!(repository
            .transition_publication(
                &claim,
                AddonMediaListPublicationPhase::AwaitingLease,
                AddonMediaListPublicationPhase::Staging,
            )
            .await
            .expect("advance to staging"));
        let long_diagnostic = "x".repeat(800);
        assert!(repository
            .record_publication_failure(
                &claim,
                AddonMediaListPublicationPhase::Staging,
                "injected_failure",
                &long_diagnostic,
                Some(AddonMediaListPublicationFailureCause {
                    category: AddonMediaListPublicationErrorCategory::PermissionDenied,
                    os_error_code: Some(13),
                }),
            )
            .await
            .expect("record original failure"));
        let retry_at = started + Duration::seconds(10);
        let held_handle_error = RepositoryError::unavailable_with_publication_cause(
            "add-on publication filesystem",
            "restore held by another process",
            AddonMediaListPublicationErrorCategory::SharingViolation,
            Some(32),
        );
        assert!(repository
            .schedule_publication_recovery_retry(
                &claim,
                &long_diagnostic,
                held_handle_error.publication_failure_cause(),
                retry_at,
            )
            .await
            .expect("schedule retry"));
        assert!(repository
            .claim_next(started + Duration::seconds(9))
            .await
            .expect("early claim")
            .is_none());

        repository.0.close().await;
        drop(repository);
        let repository = SqliteManagedLibraryOperationRepository(
            crate::db::init(&database_url)
                .await
                .expect("reopen recovery database"),
        );
        assert_reclaimed_publication_failure(&repository, queued.operation.id, started).await;
    }

    async fn assert_reclaimed_publication_failure(
        repository: &SqliteManagedLibraryOperationRepository,
        operation_id: EntityId,
        started: DateTime<Utc>,
    ) {
        let reclaimed = repository
            .claim_next(started + Duration::seconds(11))
            .await
            .expect("reclaim retry")
            .expect("eligible retry");
        assert_eq!(reclaimed.operation.id, operation_id);
        assert_eq!(reclaimed.operation.attempt_count, 2);
        let job = repository
            .publication_job(operation_id)
            .await
            .expect("load publication job")
            .expect("publication job");
        assert_eq!(job.phase, AddonMediaListPublicationPhase::RestoringFiles);
        assert_eq!(
            job.original_failure_code.as_deref(),
            Some("injected_failure")
        );
        assert_eq!(
            job.original_failure_diagnostic
                .as_deref()
                .expect("original diagnostic")
                .chars()
                .count(),
            512
        );
        let history = repository
            .publication_history(1, started + Duration::seconds(11))
            .await
            .expect("load redacted history");
        let events = &history
            .jobs
            .first()
            .expect("publication history job")
            .events;
        assert!(events
            .windows(2)
            .all(|pair| pair[0].sequence < pair[1].sequence));
        let original = events
            .iter()
            .find(|event| event.kind == AddonMediaListPublicationEventKind::OriginalFailure)
            .expect("original failure event");
        assert_eq!(
            original.error_category,
            Some(AddonMediaListPublicationErrorCategory::PermissionDenied)
        );
        assert_eq!(original.os_error_code, Some(13));
        let recovery = events
            .iter()
            .find(|event| event.kind == AddonMediaListPublicationEventKind::RecoveryFailure)
            .expect("recovery failure event");
        assert_eq!(
            recovery.error_category,
            Some(AddonMediaListPublicationErrorCategory::SharingViolation)
        );
        assert_eq!(recovery.os_error_code, Some(32));
        assert!(events
            .iter()
            .any(|event| event.kind == AddonMediaListPublicationEventKind::LeaseReclaimed));
        assert_eq!(
            job.recovery_diagnostic
                .as_deref()
                .expect("recovery diagnostic")
                .chars()
                .count(),
            512
        );
    }

    #[tokio::test]
    async fn publication_status_distinguishes_live_and_expired_lease_and_retries_exact_generation()
    {
        let pool = pool_with_library().await;
        seed_publication_list(&pool, "org.example.status").await;
        let repository = SqliteManagedLibraryOperationRepository(pool.clone());
        let queued = repository
            .enqueue_publication(1, 1, 3, 2)
            .await
            .expect("enqueue publication");
        let started = Utc::now();
        assert_eq!(
            repository
                .publication_status(1, started)
                .await
                .expect("queued status")
                .expect("status")
                .state,
            AddonMediaListPublicationState::Queued
        );
        let claim = repository
            .claim_operation(queued.operation.id, started)
            .await
            .expect("claim publication")
            .expect("claim");
        assert!(repository
            .transition_publication(
                &claim,
                AddonMediaListPublicationPhase::AwaitingLease,
                AddonMediaListPublicationPhase::Staging,
            )
            .await
            .expect("enter staging"));
        assert_eq!(
            repository
                .publication_status(1, started + Duration::seconds(1))
                .await
                .expect("live status")
                .expect("status")
                .state,
            AddonMediaListPublicationState::Publishing
        );
        let expired_at = started + Duration::seconds(91);
        let expired = repository
            .publication_status(1, expired_at)
            .await
            .expect("expired status")
            .expect("status");
        assert_eq!(expired.state, AddonMediaListPublicationState::Expired);
        assert!(expired.can_retry);
        let retried = repository
            .recover_publication(
                1,
                queued.job.id,
                queued.job.generation,
                AddonMediaListRecoveryAction::Retry,
                expired_at,
            )
            .await
            .expect("retry exact generation");
        assert_eq!(retried.job_id, queued.job.id);
        assert_eq!(retried.state, AddonMediaListPublicationState::Queued);
        assert_eq!(retried.phase, AddonMediaListPublicationPhase::Staging);
        assert!(repository
            .recover_publication(
                1,
                queued.job.id,
                queued.job.generation + 1,
                AddonMediaListRecoveryAction::Retry,
                expired_at,
            )
            .await
            .is_err());
    }

    #[tokio::test]
    async fn restore_command_requires_a_journal_and_requeues_at_restore_files() {
        let pool = pool_with_library().await;
        seed_publication_list(&pool, "org.example.restore").await;
        let repository = SqliteManagedLibraryOperationRepository(pool.clone());
        let queued = repository
            .enqueue_publication(1, 1, 3, 2)
            .await
            .expect("enqueue publication");
        let started = Utc::now();
        repository
            .claim_operation(queued.operation.id, started)
            .await
            .expect("claim publication")
            .expect("claim");
        let expired_at = started + Duration::seconds(91);
        assert!(repository
            .recover_publication(
                1,
                queued.job.id,
                queued.job.generation,
                AddonMediaListRecoveryAction::RestorePreviousRevision,
                expired_at,
            )
            .await
            .is_err());
        sqlx::query(
            r#"UPDATE "AddonMediaList" SET "PublicationJournalJson" = '{}' WHERE "Id" = 1"#,
        )
        .execute(&pool)
        .await
        .expect("seed recovery journal");
        let restored = repository
            .recover_publication(
                1,
                queued.job.id,
                queued.job.generation,
                AddonMediaListRecoveryAction::RestorePreviousRevision,
                expired_at,
            )
            .await
            .expect("restore exact generation");
        assert_eq!(restored.state, AddonMediaListPublicationState::Queued);
        assert_eq!(
            restored.phase,
            AddonMediaListPublicationPhase::RestoringFiles
        );
    }

    #[tokio::test]
    async fn publication_history_uses_the_last_slot_as_a_truncation_marker() {
        let pool = pool_with_library().await;
        seed_publication_list(&pool, "org.example.truncation").await;
        let repository = SqliteManagedLibraryOperationRepository(pool.clone());
        let queued = repository
            .enqueue_publication(1, 1, 3, 2)
            .await
            .expect("enqueue publication");
        let now = encode_datetime(Utc::now());
        let mut tx = crate::db::begin_write(&pool)
            .await
            .expect("begin history transaction");
        for index in 0..300 {
            append_publication_event_for_job_tx(
                &mut tx,
                queued.job.id,
                &format!("test:{index}"),
                PublicationEventWrite {
                    kind: AddonMediaListPublicationEventKind::PhaseAdvanced,
                    phase: Some(AddonMediaListPublicationPhase::Staging),
                    occurred_at: &now,
                    ..PublicationEventWrite::default()
                },
            )
            .await
            .expect("append bounded history event");
        }
        tx.commit().await.expect("commit bounded history");

        let history = repository
            .publication_history(1, Utc::now())
            .await
            .expect("load bounded history");
        let events = &history.jobs.first().expect("publication job").events;
        assert_eq!(events.len(), 256);
        assert_eq!(events.last().expect("last event").sequence, 256);
        assert_eq!(
            events.last().expect("last event").kind,
            AddonMediaListPublicationEventKind::HistoryTruncated
        );
    }

    #[tokio::test]
    async fn publication_history_prunes_completed_events_but_preserves_recovery_work() {
        let pool = pool_with_library().await;
        seed_publication_list(&pool, "org.example.retention").await;
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" (
                   "Id", "AddonId", "ProviderId", "SourceUrl", "Name",
                   "DraftRevision", "PublishedRevision"
               ) VALUES (2, 'org.example.retention', 'second',
                         'https://example.invalid/second', 'Second', 3, 1)"#,
        )
        .execute(&pool)
        .await
        .expect("seed second list");
        let mut tx = crate::db::begin_write(&pool)
            .await
            .expect("begin retention transaction");
        seed_completed_publication_history(&mut tx).await;
        seed_recovery_publication_history(&mut tx).await;
        tx.commit().await.expect("commit retention fixtures");

        let repository = SqliteManagedLibraryOperationRepository(pool.clone());
        let now = DateTime::parse_from_rfc3339("2026-09-04T00:00:00Z")
            .expect("retention time")
            .with_timezone(&Utc);
        assert_eq!(
            repository
                .prune_publication_history(now)
                .await
                .expect("prune completed history"),
            3
        );
        let list_one_events: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "AddonMediaListPublicationEvent" AS "event"
               JOIN "AddonMediaListPublicationJob" AS "job"
                 ON "job"."Id" = "event"."PublicationJobId"
               WHERE "job"."AddonMediaListId" = 1"#,
        )
        .fetch_one(&pool)
        .await
        .expect("count list-one history");
        assert_eq!(list_one_events, 50);
        let failed_events: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "AddonMediaListPublicationEvent"
               WHERE "PublicationJobId" = 202"#,
        )
        .fetch_one(&pool)
        .await
        .expect("count recovery history");
        assert_eq!(failed_events, 1);
        let job_count: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaListPublicationJob""#)
                .fetch_one(&pool)
                .await
                .expect("count retained job rows");
        assert_eq!(job_count, 54);
    }

    async fn seed_completed_publication_history(tx: &mut sqlx::Transaction<'_, crate::db::Db>) {
        for generation in 1_i32..=52 {
            let operation_id = i64::from(generation) + 100;
            let completed_at = if generation == 1 {
                "2026-07-01T00:00:00Z"
            } else {
                "2026-09-03T00:00:00Z"
            };
            sqlx::query(
                r#"INSERT INTO "ManagedLibraryOperation" (
                       "Id", "LibraryId", "OwnerToken", "Kind", "State",
                       "CreatedAt", "UpdatedAt", "CompletedAt"
                   ) VALUES (?, 1, ?, ?, ?, ?, ?, ?)"#,
            )
            .bind(operation_id)
            .bind(format!("retained-{generation}"))
            .bind(encode_enum(ManagedLibraryOperationKind::Publication))
            .bind(encode_enum(ManagedLibraryOperationState::Completed))
            .bind(completed_at)
            .bind(completed_at)
            .bind(completed_at)
            .execute(&mut **tx)
            .await
            .expect("seed completed operation");
            sqlx::query(
                r#"INSERT INTO "AddonMediaListPublicationJob" (
                       "Id", "OperationId", "AddonMediaListId", "Generation",
                       "SavedRevision", "TargetPublishedRevision", "Phase", "Outcome",
                       "CreatedAt", "UpdatedAt", "CompletedAt"
                   ) VALUES (?, ?, 1, ?, 3, 2, ?, ?, ?, ?, ?)"#,
            )
            .bind(operation_id)
            .bind(operation_id)
            .bind(generation)
            .bind(encode_enum(AddonMediaListPublicationPhase::Complete))
            .bind(encode_enum(AddonMediaListPublicationOutcome::Published))
            .bind(completed_at)
            .bind(completed_at)
            .bind(completed_at)
            .execute(&mut **tx)
            .await
            .expect("seed completed publication job");
            append_publication_event_for_job_tx(
                tx,
                operation_id,
                "completed",
                PublicationEventWrite {
                    kind: AddonMediaListPublicationEventKind::Completed,
                    phase: Some(AddonMediaListPublicationPhase::Complete),
                    outcome: Some(AddonMediaListPublicationOutcome::Published),
                    occurred_at: completed_at,
                    ..PublicationEventWrite::default()
                },
            )
            .await
            .expect("seed completed publication event");
        }
    }

    async fn seed_recovery_publication_history(tx: &mut sqlx::Transaction<'_, crate::db::Db>) {
        for (operation_id, state) in [
            (201_i64, ManagedLibraryOperationState::Completed),
            (202_i64, ManagedLibraryOperationState::Failed),
        ] {
            sqlx::query(
                r#"INSERT INTO "ManagedLibraryOperation" (
                       "Id", "LibraryId", "OwnerToken", "Kind", "State",
                       "CreatedAt", "UpdatedAt", "CompletedAt"
                   ) VALUES (?, 1, ?, ?, ?, '2026-07-01T00:00:00Z',
                             '2026-07-01T00:00:00Z', '2026-07-01T00:00:00Z')"#,
            )
            .bind(operation_id)
            .bind(format!("second-{operation_id}"))
            .bind(encode_enum(ManagedLibraryOperationKind::Publication))
            .bind(encode_enum(state))
            .execute(&mut **tx)
            .await
            .expect("seed second-list operation");
            sqlx::query(
                r#"INSERT INTO "AddonMediaListPublicationJob" (
                       "Id", "OperationId", "AddonMediaListId", "Generation",
                       "SavedRevision", "TargetPublishedRevision", "Phase",
                       "CreatedAt", "UpdatedAt", "CompletedAt"
                   ) VALUES (?, ?, 2, ?, 3, 2, ?, '2026-07-01T00:00:00Z',
                             '2026-07-01T00:00:00Z', '2026-07-01T00:00:00Z')"#,
            )
            .bind(operation_id)
            .bind(operation_id)
            .bind(i32::try_from(operation_id - 200).expect("generation"))
            .bind(encode_enum(
                if state == ManagedLibraryOperationState::Completed {
                    AddonMediaListPublicationPhase::Complete
                } else {
                    AddonMediaListPublicationPhase::RestoringFiles
                },
            ))
            .execute(&mut **tx)
            .await
            .expect("seed second-list publication job");
            append_publication_event_for_job_tx(
                tx,
                operation_id,
                "retained",
                PublicationEventWrite {
                    kind: if state == ManagedLibraryOperationState::Completed {
                        AddonMediaListPublicationEventKind::Completed
                    } else {
                        AddonMediaListPublicationEventKind::RecoveryFailure
                    },
                    occurred_at: "2026-07-01T00:00:00Z",
                    ..PublicationEventWrite::default()
                },
            )
            .await
            .expect("seed second-list event");
        }
    }

    async fn seed_publication_list(pool: &DbPool, addon_id: &str) {
        sqlx::query(
            r#"INSERT INTO "AddonInstallation" (
                   "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
                   "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt"
               ) VALUES (?, '1.0.0', 1, 'package', 'content', '{}',
                         'addons/fixture', '2026-09-02T00:00:00Z', '2026-09-02T00:00:00Z')"#,
        )
        .bind(addon_id)
        .execute(pool)
        .await
        .expect("seed installation");
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" (
                   "Id", "AddonId", "ProviderId", "SourceUrl", "Name",
                   "DraftRevision", "PublishedRevision"
               ) VALUES (1, ?, 'list', 'https://example.invalid/list', 'List', 3, 1)"#,
        )
        .bind(addon_id)
        .execute(pool)
        .await
        .expect("seed list");
    }

    #[tokio::test]
    async fn reset_admission_validates_name_and_atomically_coalesces_one_generation_with_editor_fence(
    ) {
        let pool = pool_with_library().await;
        seed_publication_list(&pool, "org.example.reset").await;
        sqlx::query(
            r#"INSERT INTO "AddonMediaListLibrary" (
                   "AddonMediaListId", "LibraryId", "FolderName", "ManifestPath"
               ) VALUES (1, 1, 'list', 'managed/list/playlist.json')"#,
        )
        .execute(&pool)
        .await
        .expect("seed list binding");
        let repository = SqliteManagedLibraryOperationRepository(pool.clone());

        let revision: i32 =
            sqlx::query_scalar(r#"SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id"=1"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(matches!(
            repository
                .enqueue_editor_reset_reload(1, "List", revision + 1)
                .await,
            Err(RepositoryError::Conflict(_))
        ));
        let mismatch = repository.enqueue_reset_reload(1, "list").await;
        assert!(matches!(mismatch, Err(RepositoryError::Conflict(_))));
        let first = repository
            .enqueue_reset_reload(1, "List")
            .await
            .expect("enqueue reset");
        let duplicate = repository
            .enqueue_reset_reload(1, "List")
            .await
            .expect("coalesce reset");
        assert!(!first.existing);
        assert!(duplicate.existing);
        assert_eq!(duplicate.operation.id, first.operation.id);
        assert_eq!(duplicate.job.id, first.job.id);
        assert_eq!(first.job.generation, 1);
        assert_eq!(first.job.phase, AddonMediaListLifecyclePhase::AwaitingLease);
        assert!(first.operation.scan.is_none());

        let operation_count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "ManagedLibraryOperation" WHERE "Kind" = ?"#,
        )
        .bind(encode_enum(ManagedLibraryOperationKind::Reset))
        .fetch_one(&pool)
        .await
        .expect("count reset operations");
        let job_count: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaListLifecycleJob""#)
                .fetch_one(&pool)
                .await
                .expect("count reset jobs");
        assert_eq!(operation_count, 1);
        assert_eq!(job_count, 1);
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)] // Keep admission, execution, and idempotent import assertions together.
    async fn reconcile_admission_and_execution_queue_one_full_scratch_import() {
        let pool = pool_with_library().await;
        seed_publication_list(&pool, "org.example.reconcile").await;
        let manifest = serde_json::json!({
            "manifest_version": 1,
            "id": "org.example.reconcile",
            "version": "1.0.0",
            "name": {"en-US": "Reconcile fixture"},
            "summary": {"en-US": "Reconcile fixture"},
            "license": "GPL-3.0-or-later",
            "source_url": "https://example.invalid/source",
            "host_version": "*",
            "capabilities": [
                {"id": "addon.check.v1"},
                {"id": "media-list.list.v1"},
                {"id": "media-list.import.v2"}
            ],
            "entrypoints": {
                "any": {"path": "fixture.sh", "kind": "posix_shell"}
            },
            "permissions": {
                "network_hosts": ["example.invalid"],
                "media_read": false,
                "managed_ffmpeg": true,
                "external_commands": []
            },
            "settings": [],
            "panels": []
        });
        sqlx::query(r#"UPDATE "AddonInstallation" SET "ManifestJson" = ? WHERE "AddonId" = ?"#)
            .bind(manifest.to_string())
            .bind("org.example.reconcile")
            .execute(&pool)
            .await
            .expect("enable staged import fixture");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListLibrary" (
                   "AddonMediaListId", "LibraryId", "FolderName", "ManifestPath"
               ) VALUES (1, 1, 'list', 'managed/list/playlist.json')"#,
        )
        .execute(&pool)
        .await
        .expect("seed list binding");
        let repository = SqliteManagedLibraryOperationRepository(pool.clone());

        let first = repository
            .enqueue_reconcile(1)
            .await
            .expect("enqueue reconcile");
        let duplicate = repository
            .enqueue_reconcile(1)
            .await
            .expect("coalesce reconcile");
        assert!(!first.existing);
        assert!(duplicate.existing);
        assert_eq!(duplicate.operation.id, first.operation.id);
        assert_eq!(first.operation.kind, ManagedLibraryOperationKind::Reconcile);
        assert_eq!(first.job.kind, AddonMediaListLifecycleJobKind::Reconcile);
        assert_eq!(first.job.saved_draft_revision, 3);
        assert_eq!(first.job.saved_published_revision, 1);

        let claim = repository
            .claim_next(Utc::now())
            .await
            .expect("claim reconcile")
            .expect("reconcile claim");
        assert!(repository
            .transition_reconcile(
                &claim,
                AddonMediaListLifecyclePhase::AwaitingLease,
                AddonMediaListLifecyclePhase::EnumeratingSource,
            )
            .await
            .expect("start source enumeration"));
        let enumerating = repository
            .reconcile_job(claim.operation.id)
            .await
            .expect("load reconcile job")
            .expect("reconcile job");
        let candidate = repository
            .queue_reconcile_import(&claim, &enumerating)
            .await
            .expect("queue complete scratch import");
        assert_eq!(
            repository
                .queue_reconcile_import(&claim, &enumerating)
                .await
                .expect("resume complete scratch import"),
            candidate
        );
        let import: (i32, i32, i32, i32) = sqlx::query_as(
            r#"SELECT "Generation", "Purpose", "Mode", "State"
               FROM "AddonMediaListImportJob" WHERE "Id" = ?"#,
        )
        .bind(candidate)
        .fetch_one(&pool)
        .await
        .expect("load reconcile import");
        assert_eq!(import.0, 1);
        assert_eq!(
            import.1,
            encode_enum(ersatzrs_domain::AddonMediaListImportPurpose::Reconcile)
        );
        assert_eq!(
            import.2,
            encode_enum(ersatzrs_domain::AddonMediaListImportMode::Full)
        );
        assert_eq!(
            import.3,
            encode_enum(ersatzrs_domain::AddonMediaListImportState::Queued)
        );
        assert!(repository
            .transition_reconcile(
                &claim,
                AddonMediaListLifecyclePhase::EnumeratingSource,
                AddonMediaListLifecyclePhase::ComparingCandidate,
            )
            .await
            .expect("move to candidate comparison"));
        assert!(repository
            .complete_reconcile(&claim, candidate)
            .await
            .expect("complete reconcile admission"));

        let revisions: (i32, i32) = sqlx::query_as(
            r#"SELECT "DraftRevision", "PublishedRevision" FROM "AddonMediaList" WHERE "Id" = 1"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load retained revisions");
        assert_eq!(revisions, (3, 1));
        let completed = repository
            .operation(claim.operation.id)
            .await
            .expect("load completed operation")
            .expect("completed operation");
        assert_eq!(completed.state, ManagedLibraryOperationState::Completed);
        let lifecycle = repository
            .reconcile_job(claim.operation.id)
            .await
            .expect("load completed reconcile")
            .expect("completed reconcile");
        assert_eq!(lifecycle.phase, AddonMediaListLifecyclePhase::Complete);
        assert_eq!(lifecycle.candidate_import_job_id, Some(candidate));
    }

    #[tokio::test]
    async fn reset_admission_rejects_another_active_list_operation() {
        let pool = pool_with_library().await;
        seed_publication_list(&pool, "org.example.reset-busy").await;
        sqlx::query(
            r#"INSERT INTO "AddonMediaListLibrary" (
                   "AddonMediaListId", "LibraryId", "FolderName", "ManifestPath"
               ) VALUES (1, 1, 'list', 'managed/list/playlist.json')"#,
        )
        .execute(&pool)
        .await
        .expect("seed list binding");
        let repository = SqliteManagedLibraryOperationRepository(pool.clone());
        let publication = repository
            .enqueue_publication(1, 1, 3, 2)
            .await
            .expect("enqueue competing publication");

        let rejected = repository.enqueue_reset_reload(1, "List").await;
        assert!(matches!(rejected, Err(RepositoryError::Conflict(_))));
        let reset_jobs: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaListLifecycleJob""#)
                .fetch_one(&pool)
                .await
                .expect("count rejected reset jobs");
        assert_eq!(reset_jobs, 0);

        sqlx::query(r#"UPDATE "ManagedLibraryOperation" SET "State" = ? WHERE "Id" = ?"#)
            .bind(encode_enum(ManagedLibraryOperationState::Completed))
            .bind(publication.operation.id)
            .execute(&pool)
            .await
            .expect("complete publication operation");
        sqlx::query(r#"UPDATE "AddonMediaList" SET "PublishedRevision" = 2 WHERE "Id" = 1"#)
            .execute(&pool)
            .await
            .expect("commit publication revision");

        let reset = repository
            .enqueue_reset_reload(1, "List")
            .await
            .expect("enqueue reset after publication completed");
        assert_eq!(reset.job.saved_draft_revision, 3);
        assert_eq!(reset.job.saved_published_revision, 2);
    }

    #[tokio::test]
    async fn publication_child_scan_inherits_parent_owner_and_cannot_open_a_second_lease() {
        let pool = pool_with_library().await;
        sqlx::query(
            r#"INSERT INTO "AddonInstallation" (
                   "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
                   "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt"
               ) VALUES ('org.example.child', '1.0.0', 1, 'package', 'content', '{}',
                         'addons/child', '2026-09-02T00:00:00Z', '2026-09-02T00:00:00Z')"#,
        )
        .execute(&pool)
        .await
        .expect("seed installation");
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" (
                   "Id", "AddonId", "ProviderId", "SourceUrl", "Name",
                   "DraftRevision", "PublishedRevision"
               ) VALUES (1, 'org.example.child', 'list', 'https://example.invalid/list',
                         'List', 1, 0)"#,
        )
        .execute(&pool)
        .await
        .expect("seed list");
        let repository = SqliteManagedLibraryOperationRepository(pool);
        repository
            .enqueue_publication(1, 1, 1, 1)
            .await
            .expect("enqueue publication");
        let started = Utc::now();
        let claim = repository
            .claim_next(started)
            .await
            .expect("claim publication")
            .expect("publication claim");
        let child = repository
            .ensure_publication_child_scan(&claim, 1)
            .await
            .expect("create child scan");
        assert_eq!(child.parent_operation_id, Some(claim.operation.id));
        assert_eq!(child.owner_token, claim.operation.owner_token);
        assert!(repository
            .claim_next(Utc::now())
            .await
            .expect("second claim")
            .is_none());
        let reclaimed = repository
            .claim_next(started + Duration::seconds(91))
            .await
            .expect("reclaim publication")
            .expect("expired parent is reclaimable");
        let resumed_child = repository
            .ensure_publication_child_scan(&reclaimed, 1)
            .await
            .expect("resume the same child scan");
        assert_eq!(resumed_child.id, child.id);
        assert_eq!(resumed_child.attempt_count, 2);
        let child_claim = ManagedLibraryOperationClaim {
            operation: resumed_child,
            lease_owner: reclaimed.lease_owner.clone(),
        };
        assert!(repository
            .complete_operation(&child_claim)
            .await
            .expect("complete child through parent lease"));
    }

    #[tokio::test]
    async fn publication_verification_scan_is_distinct_and_resumes_after_reclaim() {
        verify_publication_scan_after_terminal_child(false).await;
    }

    #[tokio::test]
    async fn publication_verification_scan_after_failed_child_is_distinct_and_resumes() {
        verify_publication_scan_after_terminal_child(true).await;
    }

    #[allow(clippy::too_many_lines)]
    async fn verify_publication_scan_after_terminal_child(failed: bool) {
        let database_directory = tempfile::tempdir().expect("verification database directory");
        let database_path = database_directory.path().join("verification.db");
        let database_url = format!("sqlite://{}?mode=rwc", database_path.display());
        let pool = pool_with_library_at(&database_url).await;
        sqlx::query(
            r#"INSERT INTO "AddonInstallation" (
                   "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
                   "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt"
               ) VALUES ('org.example.verify', '1.0.0', 1, 'package', 'content', '{}',
                         'addons/verify', '2026-09-02T00:00:00Z', '2026-09-02T00:00:00Z')"#,
        )
        .execute(&pool)
        .await
        .expect("seed installation");
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" (
                   "Id", "AddonId", "ProviderId", "SourceUrl", "Name",
                   "DraftRevision", "PublishedRevision"
               ) VALUES (1, 'org.example.verify', 'list', 'https://example.invalid/list',
                         'List', 1, 0)"#,
        )
        .execute(&pool)
        .await
        .expect("seed list");
        let repository = SqliteManagedLibraryOperationRepository(pool);
        let queued = repository
            .enqueue_publication(1, 1, 1, 1)
            .await
            .expect("enqueue publication");
        let started = Utc::now();
        let claim = repository
            .claim_next(started)
            .await
            .expect("claim publication")
            .expect("publication claim");
        assert!(repository
            .transition_publication(
                &claim,
                AddonMediaListPublicationPhase::AwaitingLease,
                AddonMediaListPublicationPhase::Scanning,
            )
            .await
            .expect("enter initial scan"));
        let initial = repository
            .ensure_publication_child_scan(&claim, 1)
            .await
            .expect("create initial child scan");
        let initial_claim = ManagedLibraryOperationClaim {
            operation: initial.clone(),
            lease_owner: claim.lease_owner.clone(),
        };
        assert!(repository
            .begin_publication_verification(
                &claim,
                AddonMediaListPublicationPhase::Scanning,
                if failed {
                    AddonMediaListPublicationOutcome::RestoredAfterFailure
                } else {
                    AddonMediaListPublicationOutcome::Published
                },
            )
            .await
            .expect("enter verification"));
        assert!(
            repository
                .ensure_publication_verification_scan(&claim, 1)
                .await
                .is_err(),
            "an active original scan must still drain before verification"
        );
        if failed {
            assert!(repository
                .fail_operation(&initial_claim, "scan_failed", "synthetic scan failure")
                .await
                .expect("fail original scan terminally"));
        } else {
            assert!(repository
                .complete_operation(&initial_claim)
                .await
                .expect("complete original scan"));
        }
        let verification = repository
            .ensure_publication_verification_scan(&claim, 1)
            .await
            .expect("create verification scan");
        assert_ne!(verification.id, initial.id);
        assert_eq!(verification.parent_operation_id, Some(claim.operation.id));
        assert_eq!(
            verification.scan,
            Some(ManagedLibraryScanSpec {
                provider: MediaSourceKind::Local,
                show_id: None,
                deep_scan: false,
            })
        );

        repository.0.close().await;
        drop(repository);
        let repository = SqliteManagedLibraryOperationRepository(
            crate::db::init(&database_url)
                .await
                .expect("reopen verification database"),
        );
        let reclaimed = repository
            .claim_operation(queued.operation.id, started + Duration::seconds(91))
            .await
            .expect("reclaim publication")
            .expect("expired parent is reclaimable");
        let resumed = repository
            .ensure_publication_verification_scan(&reclaimed, 1)
            .await
            .expect("resume verification scan");
        assert_eq!(resumed.id, verification.id);
        assert_eq!(resumed.attempt_count, 2);
        let resumed_claim = ManagedLibraryOperationClaim {
            operation: resumed,
            lease_owner: reclaimed.lease_owner.clone(),
        };
        assert!(repository
            .complete_operation(&resumed_claim)
            .await
            .expect("complete verification scan"));
        let completed = repository
            .ensure_publication_verification_scan(&reclaimed, 1)
            .await
            .expect("reuse completed verification scan");
        assert_eq!(completed.id, verification.id);
        assert_eq!(completed.state, ManagedLibraryOperationState::Completed);
    }
}
