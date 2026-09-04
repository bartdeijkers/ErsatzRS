use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use ersatzrs_core::repositories::{
    AddonMediaListPublicationEnqueue, AddonMediaListPublicationJob,
    AddonMediaListPublicationStatus, ManagedLibraryOperation, ManagedLibraryOperationClaim,
    ManagedLibraryOperationRepository, ManagedLibraryScanSpec, RepositoryError, Result,
};
use ersatzrs_domain::{
    AddonMediaListPublicationOutcome, AddonMediaListPublicationPhase,
    AddonMediaListPublicationState, AddonMediaListRecoveryAction, EntityId,
    ManagedLibraryOperationKind, ManagedLibraryOperationState, MediaSourceKind,
};
use sqlx::Row as _;
use uuid::Uuid;

use crate::db::{inserted_id, DbPool};
use crate::repositories::common::{
    decode_bool_column, decode_datetime, decode_enum_column, decode_optional_datetime,
    encode_datetime, encode_enum,
};
use crate::repositories::SqlxResultExt as _;

const LEASE_SECONDS: i64 = 90;

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
        target_published_revision: i32,
    ) -> Result<AddonMediaListPublicationEnqueue> {
        let now = encode_datetime(Utc::now());
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        if let Some(row) = active_publication_row(&mut tx, list_id).await? {
            let (job, operation) = decode_publication_row(&row)?;
            tx.commit().await.repository()?;
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
        .fetch_optional(&mut *tx)
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
                "the saved add-on media-list revision changed before publication was queued"
                    .to_owned(),
            ));
        }
        if let Some(row) =
            completed_publication_row(&mut tx, list_id, saved_revision, current_published).await?
        {
            let (job, operation) = decode_publication_row(&row)?;
            tx.commit().await.repository()?;
            return Ok(AddonMediaListPublicationEnqueue {
                job,
                operation,
                existing: true,
            });
        }
        if current_published.saturating_add(1) != target_published_revision {
            return Err(RepositoryError::Conflict(
                "the published add-on media-list revision changed before publication was queued"
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
        .execute(&mut *tx)
        .await
        .repository()?;
        let job_id = inserted_id(&mut tx, job_result).await.repository()?;
        tx.commit().await.repository()?;
        Ok(AddonMediaListPublicationEnqueue {
            job: AddonMediaListPublicationJob {
                id: job_id,
                operation_id,
                list_id,
                generation,
                saved_revision,
                target_published_revision,
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

    async fn publication_status(
        &self,
        list_id: EntityId,
        now: DateTime<Utc>,
    ) -> Result<Option<AddonMediaListPublicationStatus>> {
        let row = latest_publication_status_row(&self.0, list_id, &encode_datetime(now)).await?;
        row.as_ref().map(decode_publication_status_row).transpose()
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
        .execute(&self.0)
        .await
        .repository()?;
        Ok(result.rows_affected() == 1)
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
                }
                tx.commit().await.repository()?;
                return Ok(operation);
            }
            if operation.state != ManagedLibraryOperationState::Completed {
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
        .execute(&self.0)
        .await
        .repository()?;
        Ok(result.rows_affected() == 1)
    }

    async fn record_publication_failure(
        &self,
        claim: &ManagedLibraryOperationClaim,
        expected: AddonMediaListPublicationPhase,
        code: &str,
        diagnostic: &str,
    ) -> Result<bool> {
        let now = encode_datetime(Utc::now());
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
        .execute(&self.0)
        .await
        .repository()?;
        Ok(result.rows_affected() == 1)
    }

    async fn schedule_publication_recovery_retry(
        &self,
        claim: &ManagedLibraryOperationClaim,
        diagnostic: &str,
        next_eligible_at: DateTime<Utc>,
    ) -> Result<bool> {
        let now = encode_datetime(Utc::now());
        let next = encode_datetime(next_eligible_at);
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        if !claim_is_current(&mut tx, claim).await? {
            return Ok(false);
        }
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
    let state = match operation_state {
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
    };
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

#[cfg(test)]
mod tests {
    use super::*;

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
    async fn publication_failure_phase_and_retry_eligibility_survive_reclaim() {
        let pool = pool_with_library().await;
        sqlx::query(
            r#"INSERT INTO "AddonInstallation" (
                   "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
                   "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt"
               ) VALUES ('org.example.recovery', '1.0.0', 1, 'package', 'content', '{}',
                         'addons/recovery', '2026-09-02T00:00:00Z', '2026-09-02T00:00:00Z')"#,
        )
        .execute(&pool)
        .await
        .expect("seed installation");
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" (
                   "Id", "AddonId", "ProviderId", "SourceUrl", "Name",
                   "DraftRevision", "PublishedRevision"
               ) VALUES (1, 'org.example.recovery', 'list', 'https://example.invalid/list',
                         'List', 3, 1)"#,
        )
        .execute(&pool)
        .await
        .expect("seed list");
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
            )
            .await
            .expect("record original failure"));
        let retry_at = started + Duration::seconds(10);
        assert!(repository
            .schedule_publication_recovery_retry(&claim, &long_diagnostic, retry_at)
            .await
            .expect("schedule retry"));
        assert!(repository
            .claim_next(started + Duration::seconds(9))
            .await
            .expect("early claim")
            .is_none());

        let reclaimed = repository
            .claim_next(started + Duration::seconds(11))
            .await
            .expect("reclaim retry")
            .expect("eligible retry");
        assert_eq!(reclaimed.operation.id, queued.operation.id);
        assert_eq!(reclaimed.operation.attempt_count, 2);
        let job = repository
            .publication_job(queued.operation.id)
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
    #[allow(clippy::too_many_lines)]
    async fn publication_verification_scan_is_distinct_and_resumes_after_reclaim() {
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
            .complete_operation(&initial_claim)
            .await
            .expect("complete initial scan"));
        assert!(repository
            .begin_publication_verification(
                &claim,
                AddonMediaListPublicationPhase::Scanning,
                AddonMediaListPublicationOutcome::Published,
            )
            .await
            .expect("enter verification"));
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
