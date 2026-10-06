//! Durable cadence admission for add-on media-list refreshes.
//!
//! ErsatzTV's Trakt scheduler provides the daily/off compatibility baseline.
//! ADR 0085 extends that behavior with explicit fixed cadences and an opt-in,
//! provider-neutral source-change check. Incremental archive and reconciliation
//! semantics remain owned by AMM-L-02 through AMM-L-05.

use chrono::{DateTime, Months, TimeDelta, Utc};
use ersatzrs_addon_contract::{
    MediaListSourceCheckOutcome as WireSourceCheckOutcome, MAX_MEDIA_LIST_SOURCE_UPDATE_INSTANTS,
};
use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::{
    AddonMediaListContentKind, AddonMediaListDetailPolicy, AddonMediaListRefreshCadence,
    AddonMediaListRefreshState, AddonMediaListSourceCheckOutcome, EntityId,
};
use sqlx::Row as _;
use url::Url;
use uuid::Uuid;

use crate::addon_media_list::InstalledAddonMediaListProvider;
use crate::addon_media_list_import::{
    load_detail_wait_range, sample_provider_delay_seconds, AddonMediaListImportCoordinator,
};
use crate::db::DbPool;
use crate::repositories::common::{
    decode_datetime, decode_enum_column, encode_datetime, encode_enum,
};
use crate::repositories::SqlxResultExt as _;

pub(crate) const REFRESH_CLAIM_LEASE_SECONDS: i64 = 120;

const MIN_ADAPTIVE_DELAY_SECONDS: i64 = 60 * 60;
const MAX_ADAPTIVE_DELAY_SECONDS: i64 = 30 * 24 * 60 * 60;
const COLD_START_DELAY_SECONDS: i64 = 24 * 60 * 60;
const MAX_FAILURE_RETRY_SECONDS: i64 = 60 * 60;
// Five bounded retries (5, 10, 20, 40, and 60 minutes) follow the first
// failure; the sixth consecutive failure becomes terminal.
const MAX_REFRESH_ATTEMPTS: i32 = 6;

/// One restart-safe scheduler for fixed refreshes and adaptive source checks.
#[derive(Clone)]
pub struct AddonMediaListRefreshCoordinator {
    pool: DbPool,
    provider: InstalledAddonMediaListProvider,
    imports: AddonMediaListImportCoordinator,
}

impl AddonMediaListRefreshCoordinator {
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self {
            provider: InstalledAddonMediaListProvider::new(pool.clone()),
            imports: AddonMediaListImportCoordinator::for_scheduled_refresh(pool.clone()),
            pool,
        }
    }

    /// Queue each currently due list at most once.
    ///
    /// The immediate startup tick and later hourly ticks both call this method.
    /// Provider I/O stays in [`Self::process_one`] so a host restart cannot lose
    /// the admitted generation.
    ///
    /// # Errors
    ///
    /// Returns an error when persisted cadence state cannot be read or queued.
    pub async fn admit_due(&self, now: DateTime<Utc>) -> Result<usize> {
        let now_text = encode_datetime(now);
        let rows = sqlx::query(
            r#"SELECT "list"."Id", "list"."AddonId", "list"."RefreshCadence"
               FROM "AddonMediaList" AS "list"
               JOIN "AddonInstallation" AS "addon"
                 ON "addon"."AddonId" = "list"."AddonId"
               LEFT JOIN "AddonMediaListRefreshState" AS "refresh"
                 ON "refresh"."AddonMediaListId" = "list"."Id"
               WHERE "addon"."Enabled" = 1
                 AND "list"."RefreshCadence" <> ?
                 AND NOT EXISTS (SELECT 1 FROM "AddonMediaListSelection" AS "selection"
                     WHERE "selection"."AddonMediaListId"="list"."Id" AND "selection"."RefreshPaused"=1)
                 AND (("list"."RefreshRetryAt" IS NOT NULL
                       AND "list"."RefreshRetryAt" <= ?)
                      OR ("list"."RefreshRetryAt" IS NULL
                          AND ("list"."NextRefreshAt" IS NULL
                               OR "list"."NextRefreshAt" <= ?)))
                 AND ("refresh"."AddonMediaListId" IS NULL
                      OR "refresh"."State" = ?
                      OR ("refresh"."State" = ?
                          AND "refresh"."DiagnosticCode" = 'capability-unavailable'))
                 AND ("list"."PublicationJournalJson" IS NULL
                      OR TRIM("list"."PublicationJournalJson") = '')
                 AND NOT EXISTS (
                     SELECT 1 FROM "AddonMediaListImportJob" AS "job"
                     WHERE "job"."AddonMediaListId" = "list"."Id"
                       AND "job"."State" IN (?, ?, ?, ?, ?)
                 )
                 AND NOT EXISTS (
                     SELECT 1
                     FROM "AddonMediaListPublicationJob" AS "publication"
                     JOIN "ManagedLibraryOperation" AS "operation"
                       ON "operation"."Id" = "publication"."OperationId"
                     WHERE "publication"."AddonMediaListId" = "list"."Id"
                       AND "operation"."State" IN (?, ?, ?)
                 )
               ORDER BY "list"."Id""#,
        )
        .bind(encode_enum(AddonMediaListRefreshCadence::Never))
        .bind(&now_text)
        .bind(&now_text)
        .bind(encode_enum(AddonMediaListRefreshState::Idle))
        .bind(encode_enum(AddonMediaListRefreshState::Failed))
        .bind(encode_enum(
            ersatzrs_domain::AddonMediaListImportState::Queued,
        ))
        .bind(encode_enum(
            ersatzrs_domain::AddonMediaListImportState::Discovering,
        ))
        .bind(encode_enum(
            ersatzrs_domain::AddonMediaListImportState::Verifying,
        ))
        .bind(encode_enum(
            ersatzrs_domain::AddonMediaListImportState::Paused,
        ))
        .bind(encode_enum(
            ersatzrs_domain::AddonMediaListImportState::NeedsAttention,
        ))
        .bind(encode_enum(
            ersatzrs_domain::ManagedLibraryOperationState::Queued,
        ))
        .bind(encode_enum(
            ersatzrs_domain::ManagedLibraryOperationState::Running,
        ))
        .bind(encode_enum(
            ersatzrs_domain::ManagedLibraryOperationState::WaitingToRetry,
        ))
        .fetch_all(&self.pool)
        .await
        .repository()?;

        let mut admitted = 0;
        for row in rows {
            let list_id: EntityId = row.try_get("Id").repository()?;
            let addon_id: String = row.try_get("AddonId").repository()?;
            let cadence: AddonMediaListRefreshCadence =
                decode_enum_column("AddonMediaListRefreshCadence", &row, "RefreshCadence")?;
            if cadence == AddonMediaListRefreshCadence::Adaptive
                && !self.provider.supports_source_check(&addon_id).await?
            {
                continue;
            }
            admitted += usize::from(self.enqueue_if_still_due(list_id, now).await?);
        }
        Ok(admitted)
    }

    async fn enqueue_if_still_due(&self, list_id: EntityId, now: DateTime<Utc>) -> Result<bool> {
        let now_text = encode_datetime(now);
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        let eligible: Option<i64> = sqlx::query_scalar(
            r#"SELECT "list"."Id"
               FROM "AddonMediaList" AS "list"
               JOIN "AddonInstallation" AS "addon"
                 ON "addon"."AddonId" = "list"."AddonId"
               LEFT JOIN "AddonMediaListRefreshState" AS "refresh"
                 ON "refresh"."AddonMediaListId" = "list"."Id"
               WHERE "list"."Id" = ? AND "addon"."Enabled" = 1
                 AND "list"."RefreshCadence" <> ?
                 AND NOT EXISTS (SELECT 1 FROM "AddonMediaListSelection" AS "selection"
                     WHERE "selection"."AddonMediaListId"="list"."Id" AND "selection"."RefreshPaused"=1)
                 AND (("list"."RefreshRetryAt" IS NOT NULL
                       AND "list"."RefreshRetryAt" <= ?)
                      OR ("list"."RefreshRetryAt" IS NULL
                          AND ("list"."NextRefreshAt" IS NULL
                               OR "list"."NextRefreshAt" <= ?)))
                 AND ("refresh"."AddonMediaListId" IS NULL
                      OR "refresh"."State" = ?
                      OR ("refresh"."State" = ?
                          AND "refresh"."DiagnosticCode" = 'capability-unavailable'))
                 AND ("list"."PublicationJournalJson" IS NULL
                      OR TRIM("list"."PublicationJournalJson") = '')
                 AND NOT EXISTS (
                     SELECT 1 FROM "AddonMediaListImportJob" AS "job"
                     WHERE "job"."AddonMediaListId" = "list"."Id"
                       AND "job"."State" IN (?, ?, ?, ?, ?)
                 )
                 AND NOT EXISTS (
                     SELECT 1
                     FROM "AddonMediaListPublicationJob" AS "publication"
                     JOIN "ManagedLibraryOperation" AS "operation"
                       ON "operation"."Id" = "publication"."OperationId"
                     WHERE "publication"."AddonMediaListId" = "list"."Id"
                       AND "operation"."State" IN (?, ?, ?)
                 )"#,
        )
        .bind(list_id)
        .bind(encode_enum(AddonMediaListRefreshCadence::Never))
        .bind(&now_text)
        .bind(&now_text)
        .bind(encode_enum(AddonMediaListRefreshState::Idle))
        .bind(encode_enum(AddonMediaListRefreshState::Failed))
        .bind(encode_enum(
            ersatzrs_domain::AddonMediaListImportState::Queued,
        ))
        .bind(encode_enum(
            ersatzrs_domain::AddonMediaListImportState::Discovering,
        ))
        .bind(encode_enum(
            ersatzrs_domain::AddonMediaListImportState::Verifying,
        ))
        .bind(encode_enum(
            ersatzrs_domain::AddonMediaListImportState::Paused,
        ))
        .bind(encode_enum(
            ersatzrs_domain::AddonMediaListImportState::NeedsAttention,
        ))
        .bind(encode_enum(
            ersatzrs_domain::ManagedLibraryOperationState::Queued,
        ))
        .bind(encode_enum(
            ersatzrs_domain::ManagedLibraryOperationState::Running,
        ))
        .bind(encode_enum(
            ersatzrs_domain::ManagedLibraryOperationState::WaitingToRetry,
        ))
        .fetch_optional(&mut *tx)
        .await
        .repository()?;
        if eligible.is_none() {
            tx.commit().await.repository()?;
            return Ok(false);
        }
        sqlx::query(
            r#"INSERT INTO "AddonMediaListRefreshState" (
                   "AddonMediaListId", "Generation", "State", "MissCount",
                   "AttemptCount", "NextEligibleAt", "UpdatedAt"
               ) VALUES (?, 1, ?, 0, 0, ?, ?)
               ON CONFLICT ("AddonMediaListId") DO UPDATE SET
                   "Generation" = "AddonMediaListRefreshState"."Generation" + 1,
                   "State" = excluded."State", "AttemptCount" = 0,
                   "LeaseOwner" = NULL, "LeaseExpiresAt" = NULL,
                   "NextEligibleAt" = excluded."NextEligibleAt",
                   "LastOutcome" = NULL, "DiagnosticCode" = NULL,
                   "UpdatedAt" = excluded."UpdatedAt""#,
        )
        .bind(list_id)
        .bind(encode_enum(AddonMediaListRefreshState::Queued))
        .bind(&now_text)
        .bind(&now_text)
        .execute(&mut *tx)
        .await
        .repository()?;
        tx.commit().await.repository()?;
        Ok(true)
    }

    /// Process at most one durable fixed refresh or adaptive source check.
    ///
    /// # Errors
    ///
    /// Returns an error when work cannot be claimed, invoked, or checkpointed.
    pub async fn process_one(&self, now: DateTime<Utc>) -> Result<bool> {
        let range = load_detail_wait_range(&self.pool).await?;
        let delay = sample_provider_delay_seconds(Uuid::new_v4().as_bytes(), range);
        let Some(work) = self.claim_one(now, delay).await? else {
            return Ok(false);
        };
        let checks_source = work.cadence == AddonMediaListRefreshCadence::Adaptive
            && work.last_outcome != Some(AddonMediaListSourceCheckOutcome::Changed);
        if checks_source {
            self.run_source_check(&work, now, delay).await?;
        } else {
            self.run_refresh(
                &work,
                now,
                delay,
                work.last_outcome == Some(AddonMediaListSourceCheckOutcome::Changed),
                work.validator.as_deref(),
            )
            .await?;
        }
        Ok(true)
    }

    #[allow(clippy::too_many_lines)] // Claiming, host pacing, and row decoding share one transaction.
    async fn claim_one(&self, now: DateTime<Utc>, delay: u32) -> Result<Option<RefreshWork>> {
        let now_text = encode_datetime(now);
        let lease_owner = Uuid::new_v4().to_string();
        let lease_expires = encode_datetime(now + TimeDelta::seconds(REFRESH_CLAIM_LEASE_SECONDS));
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        sqlx::query(
            r#"UPDATE "AddonMediaListRefreshState"
               SET "State" = ?, "LeaseOwner" = NULL, "LeaseExpiresAt" = NULL,
                   "NextEligibleAt" = ?, "UpdatedAt" = ?
               WHERE "State" = ? AND "LeaseExpiresAt" <= ?"#,
        )
        .bind(encode_enum(AddonMediaListRefreshState::WaitingToRetry))
        .bind(&now_text)
        .bind(&now_text)
        .bind(encode_enum(AddonMediaListRefreshState::Running))
        .bind(&now_text)
        .execute(&mut *tx)
        .await
        .repository()?;
        let rows = sqlx::query(
            r#"SELECT "refresh"."AddonMediaListId", "refresh"."Generation",
                      "refresh"."Validator", "refresh"."MissCount",
                      "refresh"."AttemptCount",
                      "refresh"."LastOutcome", "list"."AddonId", "list"."SourceUrl",
                      "list"."RefreshCadence", "list"."ContentKindOverride",
                      "list"."ChapterInput", "list"."DetailImportPolicy"
               FROM "AddonMediaListRefreshState" AS "refresh"
               JOIN "AddonMediaList" AS "list"
                 ON "list"."Id" = "refresh"."AddonMediaListId"
               JOIN "AddonInstallation" AS "addon"
                 ON "addon"."AddonId" = "list"."AddonId"
               WHERE "addon"."Enabled" = 1
                 AND "list"."RefreshCadence" <> ?
                 AND NOT EXISTS (SELECT 1 FROM "AddonMediaListSelection" AS "selection"
                     WHERE "selection"."AddonMediaListId"="list"."Id" AND "selection"."RefreshPaused"=1)
                 AND "refresh"."State" IN (?, ?)
                 AND ("refresh"."NextEligibleAt" IS NULL
                      OR "refresh"."NextEligibleAt" <= ?)
               ORDER BY "refresh"."NextEligibleAt", "refresh"."AddonMediaListId"
               LIMIT 32"#,
        )
        .bind(encode_enum(AddonMediaListRefreshCadence::Never))
        .bind(encode_enum(AddonMediaListRefreshState::Queued))
        .bind(encode_enum(AddonMediaListRefreshState::WaitingToRetry))
        .bind(&now_text)
        .fetch_all(&mut *tx)
        .await
        .repository()?;
        for row in rows {
            let source_url: String = row.try_get("SourceUrl").repository()?;
            let host = provider_host(&source_url)?;
            if let Some(blocked_until) = claim_provider_host(
                &mut tx,
                &host,
                &now_text,
                &lease_owner,
                &lease_expires,
                delay,
            )
            .await?
            {
                sqlx::query(
                    r#"UPDATE "AddonMediaListRefreshState"
                       SET "NextEligibleAt" = ?, "UpdatedAt" = ?
                       WHERE "AddonMediaListId" = ? AND "State" IN (?, ?)"#,
                )
                .bind(blocked_until)
                .bind(&now_text)
                .bind(
                    row.try_get::<EntityId, _>("AddonMediaListId")
                        .repository()?,
                )
                .bind(encode_enum(AddonMediaListRefreshState::Queued))
                .bind(encode_enum(AddonMediaListRefreshState::WaitingToRetry))
                .execute(&mut *tx)
                .await
                .repository()?;
                continue;
            }
            let list_id: EntityId = row.try_get("AddonMediaListId").repository()?;
            let claimed = sqlx::query(
                r#"UPDATE "AddonMediaListRefreshState"
                   SET "State" = ?, "LeaseOwner" = ?, "LeaseExpiresAt" = ?,
                       "UpdatedAt" = ?
                   WHERE "AddonMediaListId" = ? AND "Generation" = ?
                     AND "State" IN (?, ?)"#,
            )
            .bind(encode_enum(AddonMediaListRefreshState::Running))
            .bind(&lease_owner)
            .bind(&lease_expires)
            .bind(&now_text)
            .bind(list_id)
            .bind(row.try_get::<i32, _>("Generation").repository()?)
            .bind(encode_enum(AddonMediaListRefreshState::Queued))
            .bind(encode_enum(AddonMediaListRefreshState::WaitingToRetry))
            .execute(&mut *tx)
            .await
            .repository()?;
            if claimed.rows_affected() != 1 {
                continue;
            }
            let work = RefreshWork {
                list_id,
                generation: row.try_get("Generation").repository()?,
                addon_id: row.try_get("AddonId").repository()?,
                source_url,
                cadence: decode_enum_column(
                    "AddonMediaListRefreshCadence",
                    &row,
                    "RefreshCadence",
                )?,
                content_kind: decode_enum_column(
                    "AddonMediaListContentKind",
                    &row,
                    "ContentKindOverride",
                )?,
                chapter_input: row
                    .try_get::<Option<String>, _>("ChapterInput")
                    .repository()?
                    .unwrap_or_default(),
                detail_policy: decode_enum_column(
                    "AddonMediaListDetailPolicy",
                    &row,
                    "DetailImportPolicy",
                )?,
                validator: row.try_get("Validator").repository()?,
                miss_count: row.try_get("MissCount").repository()?,
                attempt_count: row.try_get("AttemptCount").repository()?,
                last_outcome: crate::repositories::common::decode_optional_enum_column(
                    "AddonMediaListSourceCheckOutcome",
                    &row,
                    "LastOutcome",
                )?,
                lease_owner: lease_owner.clone(),
                provider_host: host,
            };
            tx.commit().await.repository()?;
            return Ok(Some(work));
        }
        tx.commit().await.repository()?;
        Ok(None)
    }

    async fn run_source_check(
        &self,
        work: &RefreshWork,
        now: DateTime<Utc>,
        delay: u32,
    ) -> Result<()> {
        if !self.provider.supports_source_check(&work.addon_id).await? {
            return self
                .finish_failure(
                    work,
                    now,
                    delay,
                    false,
                    None,
                    "capability-unavailable",
                    false,
                )
                .await;
        }
        let Ok(result) = self
            .provider
            .source_check(&work.addon_id, &work.source_url, work.validator.as_deref())
            .await
        else {
            return self
                .finish_failure(work, now, delay, true, None, "source-check-failed", false)
                .await;
        };
        match result.outcome {
            WireSourceCheckOutcome::Unchanged => {
                self.finish_unchanged(work, now, delay, result.validator.as_deref())
                    .await
            }
            WireSourceCheckOutcome::TransientFailure => {
                self.finish_failure(
                    work,
                    now,
                    delay,
                    true,
                    result.retry_after_seconds,
                    result.code.as_deref().unwrap_or("source-check-failed"),
                    false,
                )
                .await
            }
            WireSourceCheckOutcome::PermanentFailure => {
                self.finish_failure(
                    work,
                    now,
                    delay,
                    false,
                    None,
                    result.code.as_deref().unwrap_or("source-check-failed"),
                    false,
                )
                .await
            }
            WireSourceCheckOutcome::Changed => {
                self.checkpoint_changed(
                    work,
                    now,
                    delay,
                    result.validator.as_deref(),
                    &result.source_updated_at,
                )
                .await?;
                self.run_refresh(work, now, delay, true, result.validator.as_deref())
                    .await
            }
        }
    }

    async fn run_refresh(
        &self,
        work: &RefreshWork,
        now: DateTime<Utc>,
        delay: u32,
        pending_change: bool,
        validator: Option<&str>,
    ) -> Result<()> {
        match self
            .imports
            .add_or_refresh(
                &work.addon_id,
                &work.source_url,
                None,
                work.content_kind,
                &work.chapter_input,
                work.detail_policy,
            )
            .await
        {
            Ok(_) => {
                self.finish_refresh_admission(work, now, delay, pending_change, validator)
                    .await
            }
            Err(_) => {
                self.finish_failure(
                    work,
                    now,
                    delay,
                    true,
                    None,
                    "refresh-admission-failed",
                    pending_change,
                )
                .await
            }
        }
    }

    async fn checkpoint_changed(
        &self,
        work: &RefreshWork,
        now: DateTime<Utc>,
        delay: u32,
        validator: Option<&str>,
        instants: &[String],
    ) -> Result<()> {
        let now_text = encode_datetime(now);
        let host_completed = Utc::now();
        let host_now = encode_datetime(host_completed);
        let next_host = encode_datetime(host_completed + TimeDelta::seconds(i64::from(delay)));
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        for instant in instants {
            let instant = encode_datetime(decode_datetime(
                "add-on source-change observation",
                instant,
            )?);
            sqlx::query(
                r#"INSERT OR IGNORE INTO "AddonMediaListSourceChangeObservation" (
                       "AddonMediaListId", "SourceUpdatedAt"
                   ) VALUES (?, ?)"#,
            )
            .bind(work.list_id)
            .bind(instant)
            .execute(&mut *tx)
            .await
            .repository()?;
        }
        if instants.is_empty() {
            sqlx::query(
                r#"INSERT OR IGNORE INTO "AddonMediaListSourceChangeObservation" (
                       "AddonMediaListId", "SourceUpdatedAt"
                   ) VALUES (?, ?)"#,
            )
            .bind(work.list_id)
            .bind(&now_text)
            .execute(&mut *tx)
            .await
            .repository()?;
        }
        prune_observations(&mut tx, work.list_id).await?;
        let updated = sqlx::query(
            r#"UPDATE "AddonMediaListRefreshState"
               SET "Validator" = ?, "MissCount" = 0, "AttemptCount" = 0,
                   "LastOutcome" = ?, "DiagnosticCode" = NULL, "UpdatedAt" = ?
               WHERE "AddonMediaListId" = ? AND "Generation" = ? AND "State" = ?
                 AND "LeaseOwner" = ? AND "LeaseExpiresAt" > ?"#,
        )
        .bind(validator)
        .bind(encode_enum(AddonMediaListSourceCheckOutcome::Changed))
        .bind(&now_text)
        .bind(work.list_id)
        .bind(work.generation)
        .bind(encode_enum(AddonMediaListRefreshState::Running))
        .bind(&work.lease_owner)
        .bind(&now_text)
        .execute(&mut *tx)
        .await
        .repository()?;
        ensure_fenced(updated.rows_affected())?;
        release_provider_host_lease(
            &mut tx,
            &work.provider_host,
            &work.lease_owner,
            &host_now,
            &next_host,
            delay,
        )
        .await?;
        tx.commit().await.repository()?;
        Ok(())
    }

    async fn finish_unchanged(
        &self,
        work: &RefreshWork,
        now: DateTime<Utc>,
        delay: u32,
        validator: Option<&str>,
    ) -> Result<()> {
        let observations = self.load_observations(work.list_id).await?;
        let miss_count = u32::try_from(work.miss_count.max(0)).unwrap_or_default();
        let next = next_adaptive_check_at(&observations, now, miss_count.saturating_add(1));
        self.finish_success(
            work,
            now,
            delay,
            Some(AddonMediaListSourceCheckOutcome::Unchanged),
            validator,
            miss_count.saturating_add(1),
            Some(next),
        )
        .await
    }

    async fn finish_refresh_admission(
        &self,
        work: &RefreshWork,
        now: DateTime<Utc>,
        delay: u32,
        pending_change: bool,
        validator: Option<&str>,
    ) -> Result<()> {
        self.finish_success(
            work,
            now,
            delay,
            pending_change.then_some(AddonMediaListSourceCheckOutcome::Changed),
            validator,
            0,
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn finish_success(
        &self,
        work: &RefreshWork,
        now: DateTime<Utc>,
        delay: u32,
        outcome: Option<AddonMediaListSourceCheckOutcome>,
        validator: Option<&str>,
        miss_count: u32,
        next: Option<DateTime<Utc>>,
    ) -> Result<()> {
        let now_text = encode_datetime(now);
        let next_text = next.map(encode_datetime);
        let host_completed = Utc::now();
        let host_now = encode_datetime(host_completed);
        let next_host = encode_datetime(host_completed + TimeDelta::seconds(i64::from(delay)));
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        let updated = sqlx::query(
            r#"UPDATE "AddonMediaListRefreshState"
               SET "State" = ?, "Validator" = ?, "MissCount" = ?,
                   "AttemptCount" = 0, "LeaseOwner" = NULL,
                   "LeaseExpiresAt" = NULL, "NextEligibleAt" = ?,
                   "LastOutcome" = ?, "DiagnosticCode" = NULL, "UpdatedAt" = ?
               WHERE "AddonMediaListId" = ? AND "Generation" = ? AND "State" = ?
                 AND "LeaseOwner" = ? AND "LeaseExpiresAt" > ?"#,
        )
        .bind(encode_enum(AddonMediaListRefreshState::Idle))
        .bind(validator)
        .bind(i32::try_from(miss_count).unwrap_or(i32::MAX))
        .bind(next_text.as_deref())
        .bind(outcome.map(encode_enum))
        .bind(&now_text)
        .bind(work.list_id)
        .bind(work.generation)
        .bind(encode_enum(AddonMediaListRefreshState::Running))
        .bind(&work.lease_owner)
        .bind(&now_text)
        .execute(&mut *tx)
        .await
        .repository()?;
        if updated.rows_affected() == 0 {
            // A synchronous legacy refresh records its success in the same
            // state row before returning. That newer persisted outcome wins.
            release_provider_host_lease(
                &mut tx,
                &work.provider_host,
                &work.lease_owner,
                &host_now,
                &next_host,
                delay,
            )
            .await?;
            tx.commit().await.repository()?;
            return Ok(());
        }
        sqlx::query(
            r#"UPDATE "AddonMediaList"
               SET "NextRefreshAt" = COALESCE(?, "NextRefreshAt"),
                   "RefreshRetryAt" = NULL
               WHERE "Id" = ?"#,
        )
        .bind(next_text.as_deref())
        .bind(work.list_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        release_provider_host_lease(
            &mut tx,
            &work.provider_host,
            &work.lease_owner,
            &host_now,
            &next_host,
            delay,
        )
        .await?;
        tx.commit().await.repository()?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)] // Keeps every persisted failure decision explicit at call sites.
    async fn finish_failure(
        &self,
        work: &RefreshWork,
        now: DateTime<Utc>,
        delay: u32,
        retryable: bool,
        retry_after_seconds: Option<u32>,
        diagnostic_code: &str,
        pending_change: bool,
    ) -> Result<()> {
        let attempts = work.attempt_count.saturating_add(1);
        let retry = retryable && attempts < MAX_REFRESH_ATTEMPTS;
        let retry_at = retry.then(|| {
            failure_retry_at(
                now,
                u32::try_from(attempts).unwrap_or(u32::MAX),
                retry_after_seconds,
            )
        });
        let now_text = encode_datetime(now);
        let retry_text = retry_at.map(encode_datetime);
        let host_completed = Utc::now();
        let host_now = encode_datetime(host_completed);
        let cooldown_at = host_completed + TimeDelta::seconds(i64::from(delay));
        let next_host =
            encode_datetime(retry_at.map_or(cooldown_at, |value| value.max(cooldown_at)));
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        let updated = sqlx::query(
            r#"UPDATE "AddonMediaListRefreshState"
               SET "State" = ?, "AttemptCount" = ?, "LeaseOwner" = NULL,
                   "LeaseExpiresAt" = NULL, "NextEligibleAt" = ?,
                   "LastOutcome" = ?, "DiagnosticCode" = ?, "UpdatedAt" = ?
               WHERE "AddonMediaListId" = ? AND "Generation" = ? AND "State" = ?
                 AND "LeaseOwner" = ? AND "LeaseExpiresAt" > ?"#,
        )
        .bind(encode_enum(if retry {
            AddonMediaListRefreshState::WaitingToRetry
        } else {
            AddonMediaListRefreshState::Failed
        }))
        .bind(attempts)
        .bind(retry_text.as_deref())
        .bind(if pending_change {
            Some(encode_enum(AddonMediaListSourceCheckOutcome::Changed))
        } else if work.cadence == AddonMediaListRefreshCadence::Adaptive {
            Some(encode_enum(if retryable {
                AddonMediaListSourceCheckOutcome::TransientFailure
            } else {
                AddonMediaListSourceCheckOutcome::PermanentFailure
            }))
        } else {
            None
        })
        .bind(diagnostic_code)
        .bind(&now_text)
        .bind(work.list_id)
        .bind(work.generation)
        .bind(encode_enum(AddonMediaListRefreshState::Running))
        .bind(&work.lease_owner)
        .bind(&now_text)
        .execute(&mut *tx)
        .await
        .repository()?;
        ensure_fenced(updated.rows_affected())?;
        sqlx::query(r#"UPDATE "AddonMediaList" SET "RefreshRetryAt" = ? WHERE "Id" = ?"#)
            .bind(retry_text.as_deref())
            .bind(work.list_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        release_provider_host_lease(
            &mut tx,
            &work.provider_host,
            &work.lease_owner,
            &host_now,
            &next_host,
            delay,
        )
        .await?;
        tx.commit().await.repository()?;
        Ok(())
    }

    async fn load_observations(&self, list_id: EntityId) -> Result<Vec<DateTime<Utc>>> {
        let rows = sqlx::query(
            r#"SELECT "SourceUpdatedAt" FROM "AddonMediaListSourceChangeObservation"
               WHERE "AddonMediaListId" = ? ORDER BY "SourceUpdatedAt""#,
        )
        .bind(list_id)
        .fetch_all(&self.pool)
        .await
        .repository()?;
        rows.iter()
            .map(|row| {
                let value: String = row.try_get("SourceUpdatedAt").repository()?;
                decode_datetime("add-on source-change observation", &value)
            })
            .collect()
    }
}

#[derive(Debug)]
struct RefreshWork {
    list_id: EntityId,
    generation: i32,
    addon_id: String,
    source_url: String,
    cadence: AddonMediaListRefreshCadence,
    content_kind: AddonMediaListContentKind,
    chapter_input: String,
    detail_policy: AddonMediaListDetailPolicy,
    validator: Option<String>,
    miss_count: i32,
    attempt_count: i32,
    last_outcome: Option<AddonMediaListSourceCheckOutcome>,
    lease_owner: String,
    provider_host: String,
}

fn provider_host(source_url: &str) -> Result<String> {
    Url::parse(source_url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
        .ok_or_else(|| RepositoryError::Decode {
            kind: "add-on provider host",
            value: "source URL has no host".to_owned(),
        })
}

pub(crate) async fn claim_provider_host(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    host: &str,
    now: &str,
    lease_owner: &str,
    lease_expires: &str,
    delay: u32,
) -> Result<Option<String>> {
    ensure_provider_recovery_not_required(
        crate::addon_media_list_preparation::provider_process_recovery_required()
            || crate::addon_preparation_worker::recovery_required_in_transaction(tx).await?,
    )?;
    let cooldown = sqlx::query(
        r#"SELECT "NextEligibleAt", "LeaseExpiresAt"
           FROM "AddonProviderHostCooldown" WHERE "ProviderHost" = ?"#,
    )
    .bind(host)
    .fetch_optional(&mut **tx)
    .await
    .repository()?;
    if let Some(cooldown) = cooldown {
        let next: String = cooldown.try_get("NextEligibleAt").repository()?;
        let lease: Option<String> = cooldown.try_get("LeaseExpiresAt").repository()?;
        if next.as_str() > now || lease.as_deref().is_some_and(|value| value > now) {
            let blocked_until = match lease {
                Some(value) if value.as_str() > now && value.as_str() > next.as_str() => value,
                _ => next,
            };
            return Ok(Some(blocked_until));
        }
        let claimed = sqlx::query(
            r#"UPDATE "AddonProviderHostCooldown"
               SET "LeaseOwner" = ?, "LeaseExpiresAt" = ?, "UpdatedAt" = ?
               WHERE "ProviderHost" = ? AND "NextEligibleAt" <= ?
                 AND ("LeaseExpiresAt" IS NULL OR "LeaseExpiresAt" <= ?)"#,
        )
        .bind(lease_owner)
        .bind(lease_expires)
        .bind(now)
        .bind(host)
        .bind(now)
        .bind(now)
        .execute(&mut **tx)
        .await
        .repository()?;
        if claimed.rows_affected() == 0 {
            return Ok(Some(now.to_owned()));
        }
    } else {
        sqlx::query(
            r#"INSERT INTO "AddonProviderHostCooldown" (
                   "ProviderHost", "NextEligibleAt", "SampledDelaySeconds",
                   "LastAttemptAt", "LeaseOwner", "LeaseExpiresAt", "UpdatedAt"
               ) VALUES (?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(host)
        .bind(now)
        .bind(i64::from(delay))
        .bind(now)
        .bind(lease_owner)
        .bind(lease_expires)
        .bind(now)
        .execute(&mut **tx)
        .await
        .repository()?;
    }
    Ok(None)
}

// This process-wide fence takes precedence over lease expiry and provider host.
// Keep its predicate injectable for tests without toggling the real global latch
// while unrelated provider tests run concurrently.
pub(crate) fn ensure_provider_recovery_not_required(recovery_required: bool) -> Result<()> {
    if recovery_required {
        return Err(RepositoryError::unavailable(
            "provider admission",
            "provider process cleanup requires host recovery",
        ));
    }
    Ok(())
}

pub(crate) async fn release_provider_host_lease(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    provider_host: &str,
    lease_owner: &str,
    now: &str,
    next: &str,
    delay: u32,
) -> Result<()> {
    sqlx::query(
        r#"UPDATE "AddonProviderHostCooldown"
           SET "NextEligibleAt" = CASE WHEN "NextEligibleAt" > ?
                   THEN "NextEligibleAt" ELSE ? END,
               "SampledDelaySeconds" = ?, "LastAttemptAt" = ?,
               "LeaseOwner" = NULL, "LeaseExpiresAt" = NULL, "UpdatedAt" = ?
           WHERE "ProviderHost" = ? AND "LeaseOwner" = ?"#,
    )
    .bind(next)
    .bind(next)
    .bind(i64::from(delay))
    .bind(now)
    .bind(now)
    .bind(provider_host)
    .bind(lease_owner)
    .execute(&mut **tx)
    .await
    .repository()?;
    Ok(())
}

async fn prune_observations(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    list_id: EntityId,
) -> Result<()> {
    sqlx::query(
        r#"DELETE FROM "AddonMediaListSourceChangeObservation"
           WHERE "AddonMediaListId" = ? AND "SourceUpdatedAt" NOT IN (
               SELECT "SourceUpdatedAt" FROM "AddonMediaListSourceChangeObservation"
               WHERE "AddonMediaListId" = ?
               ORDER BY "SourceUpdatedAt" DESC LIMIT ?
           )"#,
    )
    .bind(list_id)
    .bind(list_id)
    .bind(i64::try_from(MAX_MEDIA_LIST_SOURCE_UPDATE_INSTANTS).unwrap_or(16))
    .execute(&mut **tx)
    .await
    .repository()?;
    Ok(())
}

fn ensure_fenced(rows_affected: u64) -> Result<()> {
    if rows_affected == 1 {
        Ok(())
    } else {
        Err(RepositoryError::Conflict(
            "the add-on refresh lease changed before its outcome was persisted".to_owned(),
        ))
    }
}

/// Return the next fixed-cadence instant anchored to a successful refresh.
///
/// Month and year addition uses Chrono's checked calendar arithmetic, which
/// clamps an invalid target day to the last day of the target month.
#[must_use]
pub fn next_fixed_refresh_at(
    cadence: AddonMediaListRefreshCadence,
    anchor: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    match cadence {
        AddonMediaListRefreshCadence::Adaptive | AddonMediaListRefreshCadence::Never => None,
        AddonMediaListRefreshCadence::Daily => anchor.checked_add_signed(TimeDelta::days(1)),
        AddonMediaListRefreshCadence::Weekly => anchor.checked_add_signed(TimeDelta::days(7)),
        AddonMediaListRefreshCadence::Monthly => anchor.checked_add_months(Months::new(1)),
        AddonMediaListRefreshCadence::Yearly => anchor.checked_add_months(Months::new(12)),
    }
}

/// Derive the next adaptive source check from actual source-change instants.
///
/// Three positive intervals are required. Until then, the check falls back to
/// daily. Once a prediction has been missed, bounded increasing miss spacing
/// takes precedence over repeatedly checking the stale predicted instant.
#[must_use]
pub fn next_adaptive_check_at(
    observations: &[DateTime<Utc>],
    now: DateTime<Utc>,
    miss_count: u32,
) -> DateTime<Utc> {
    if miss_count > 0 {
        return now + TimeDelta::seconds(miss_retry_seconds(miss_count));
    }

    let mut observations = observations.to_vec();
    observations.sort_unstable();
    observations.dedup();
    let mut intervals = observations
        .windows(2)
        .filter_map(|pair| {
            let seconds = (pair[1] - pair[0]).num_seconds();
            (seconds > 0).then_some(seconds)
        })
        .collect::<Vec<_>>();
    if intervals.len() < 3 {
        return now + TimeDelta::seconds(COLD_START_DELAY_SECONDS);
    }
    intervals.sort_unstable();
    let median_seconds = intervals[intervals.len() / 2];
    let lead_seconds = (median_seconds / 10).clamp(1, MIN_ADAPTIVE_DELAY_SECONDS);
    let predicted = observations
        .last()
        .copied()
        .unwrap_or(now)
        .checked_add_signed(TimeDelta::seconds(median_seconds))
        .and_then(|value| value.checked_sub_signed(TimeDelta::seconds(lead_seconds)))
        .unwrap_or(now + TimeDelta::seconds(MAX_ADAPTIVE_DELAY_SECONDS));
    let delay = (predicted - now)
        .num_seconds()
        .clamp(MIN_ADAPTIVE_DELAY_SECONDS, MAX_ADAPTIVE_DELAY_SECONDS);
    now + TimeDelta::seconds(delay)
}

/// Increasing spacing after a successful-but-unchanged adaptive check.
#[must_use]
pub const fn miss_retry_seconds(miss_count: u32) -> i64 {
    match miss_count {
        0 | 1 => 60 * 60,
        2 => 2 * 60 * 60,
        3 => 4 * 60 * 60,
        4 => 8 * 60 * 60,
        5 => 16 * 60 * 60,
        _ => 24 * 60 * 60,
    }
}

/// Bounded refresh/source-check failure backoff, with Retry-After precedence.
#[must_use]
pub fn failure_retry_at(
    now: DateTime<Utc>,
    attempt_count: u32,
    retry_after_seconds: Option<u32>,
) -> DateTime<Utc> {
    let shift = attempt_count.saturating_sub(1).min(4);
    let local = (5_i64 * 60)
        .saturating_mul(1_i64 << shift)
        .min(MAX_FAILURE_RETRY_SECONDS);
    let delay = local.max(i64::from(retry_after_seconds.unwrap_or_default()));
    now + TimeDelta::seconds(delay)
}

/// Advance a list cadence only after its provider snapshot was committed.
pub(crate) async fn record_refresh_success_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    list_id: i64,
    now: DateTime<Utc>,
) -> Result<()> {
    let row = sqlx::query(r#"SELECT "RefreshCadence" FROM "AddonMediaList" WHERE "Id" = ?"#)
        .bind(list_id)
        .fetch_one(&mut **tx)
        .await
        .repository()?;
    let cadence = decode_enum_column("AddonMediaListRefreshCadence", &row, "RefreshCadence")?;
    let next = match cadence {
        AddonMediaListRefreshCadence::Adaptive => {
            let rows = sqlx::query(
                r#"SELECT "SourceUpdatedAt"
                   FROM "AddonMediaListSourceChangeObservation"
                   WHERE "AddonMediaListId" = ?
                   ORDER BY "SourceUpdatedAt""#,
            )
            .bind(list_id)
            .fetch_all(&mut **tx)
            .await
            .repository()?;
            let observations = rows
                .iter()
                .map(|row| {
                    let value: String = row.try_get("SourceUpdatedAt").repository()?;
                    decode_datetime("add-on source-change observation", &value)
                })
                .collect::<Result<Vec<_>>>()?;
            Some(next_adaptive_check_at(&observations, now, 0))
        }
        cadence => next_fixed_refresh_at(cadence, now),
    };
    let now = encode_datetime(now);
    let next = next.map(encode_datetime);
    sqlx::query(
        r#"UPDATE "AddonMediaList"
           SET "LastUpdate" = ?, "LastRefreshSucceededAt" = ?,
               "NextRefreshAt" = ?, "RefreshRetryAt" = NULL, "LastError" = NULL
           WHERE "Id" = ?"#,
    )
    .bind(&now)
    .bind(&now)
    .bind(next.as_deref())
    .bind(list_id)
    .execute(&mut **tx)
    .await
    .repository()?;
    sqlx::query(
        r#"UPDATE "AddonMediaListRefreshState"
           SET "State" = ?, "MissCount" = 0, "AttemptCount" = 0,
               "LeaseOwner" = NULL, "LeaseExpiresAt" = NULL,
               "NextEligibleAt" = ?, "DiagnosticCode" = NULL, "UpdatedAt" = ?
           WHERE "AddonMediaListId" = ?"#,
    )
    .bind(encode_enum(
        ersatzrs_domain::AddonMediaListRefreshState::Idle,
    ))
    .bind(next.as_deref())
    .bind(&now)
    .bind(list_id)
    .execute(&mut **tx)
    .await
    .repository()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::{TimeDelta, TimeZone as _, Utc};
    #[cfg(unix)]
    use ersatzrs_domain::AddonMediaListSourceCheckOutcome;
    use ersatzrs_domain::{AddonMediaListRefreshCadence, AddonMediaListRefreshState, EntityId};
    use sqlx::Row as _;

    use super::{
        encode_datetime, encode_enum, ensure_provider_recovery_not_required, failure_retry_at,
        next_adaptive_check_at, next_fixed_refresh_at, record_refresh_success_tx,
        AddonMediaListRefreshCoordinator,
    };
    #[cfg(unix)]
    use crate::repositories::common::decode_datetime;

    async fn fixture_pool() -> (tempfile::TempDir, crate::db::DbPool) {
        let temporary = tempfile::tempdir().expect("temporary refresh database");
        let database_url = format!("sqlite:{}", temporary.path().join("refresh.db").display());
        let pool = crate::db::init(&database_url)
            .await
            .expect("initialize refresh database");
        let now = encode_datetime(Utc::now());
        sqlx::query(
            r#"INSERT INTO "AddonInstallation" (
                   "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
                   "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt"
               ) VALUES ('org.example.refresh', '1.0.0', 1, 'package', 'content', '{}',
                         'fixture', ?, ?)"#,
        )
        .bind(&now)
        .bind(&now)
        .execute(&pool)
        .await
        .expect("seed refresh add-on");
        (temporary, pool)
    }

    #[cfg(unix)]
    async fn install_source_check_fixture(
        temporary: &tempfile::TempDir,
        pool: &crate::db::DbPool,
        script_source: &str,
    ) {
        use std::os::unix::fs::PermissionsExt as _;

        let package = temporary.path().join("package");
        tokio::fs::create_dir_all(&package)
            .await
            .expect("create source-check fixture package");
        let script = package.join("fixture.sh");
        tokio::fs::write(&script, script_source)
            .await
            .expect("write source-check fixture");
        let mut permissions = tokio::fs::metadata(&script)
            .await
            .expect("load source-check fixture metadata")
            .permissions();
        permissions.set_mode(0o755);
        tokio::fs::set_permissions(&script, permissions)
            .await
            .expect("make source-check fixture executable");

        let manifest = serde_json::json!({
            "manifest_version": 1,
            "id": "org.example.refresh",
            "version": "1.0.0",
            "name": {"en-US": "Refresh fixture"},
            "summary": {"en-US": "Adaptive source-check fixture"},
            "license": "GPL-3.0-or-later",
            "source_url": "https://example.test/source",
            "host_version": "*",
            "capabilities": [
                {"id": "addon.check.v1"},
                {"id": "media-list.list.v1"},
                {"id": "media-list.source-check.v1"}
            ],
            "entrypoints": {
                "any": {"path": "fixture.sh", "kind": "posix_shell"}
            },
            "permissions": {
                "network_hosts": ["example.test"],
                "media_read": false,
                "managed_ffmpeg": false,
                "external_commands": []
            },
            "settings": [],
            "panels": []
        });
        let digest = crate::addons::calculate_addon_content_sha256(&package)
            .await
            .expect("hash source-check fixture");
        sqlx::query(
            r#"UPDATE "AddonInstallation"
               SET "ContentSha256" = ?, "ManifestJson" = ?, "InstallPath" = ?
               WHERE "AddonId" = 'org.example.refresh'"#,
        )
        .bind(digest)
        .bind(manifest.to_string())
        .bind(package.to_string_lossy().into_owned())
        .execute(pool)
        .await
        .expect("install source-check fixture");
    }

    async fn seed_list(
        pool: &crate::db::DbPool,
        id: EntityId,
        cadence: AddonMediaListRefreshCadence,
        source_url: &str,
    ) {
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" (
                   "Id", "AddonId", "ProviderId", "SourceUrl", "Name", "RefreshCadence"
               ) VALUES (?, 'org.example.refresh', ?, ?, ?, ?)"#,
        )
        .bind(id)
        .bind(format!("provider-{id}"))
        .bind(source_url)
        .bind(format!("List {id}"))
        .bind(encode_enum(cadence))
        .execute(pool)
        .await
        .expect("seed refresh list");
    }

    #[test]
    fn fixed_calendar_cadences_clamp_month_end_and_leap_day() {
        let january_end = Utc.with_ymd_and_hms(2025, 1, 31, 12, 30, 0).unwrap();
        assert_eq!(
            next_fixed_refresh_at(AddonMediaListRefreshCadence::Monthly, january_end),
            Some(Utc.with_ymd_and_hms(2025, 2, 28, 12, 30, 0).unwrap())
        );
        let leap_day = Utc.with_ymd_and_hms(2024, 2, 29, 12, 30, 0).unwrap();
        assert_eq!(
            next_fixed_refresh_at(AddonMediaListRefreshCadence::Yearly, leap_day),
            Some(Utc.with_ymd_and_hms(2025, 2, 28, 12, 30, 0).unwrap())
        );
    }

    #[test]
    fn fixed_durations_remain_utc_across_dst_boundaries() {
        let before_dst = Utc.with_ymd_and_hms(2025, 3, 29, 23, 30, 0).unwrap();
        assert_eq!(
            next_fixed_refresh_at(AddonMediaListRefreshCadence::Daily, before_dst),
            Some(Utc.with_ymd_and_hms(2025, 3, 30, 23, 30, 0).unwrap())
        );
    }

    #[test]
    fn adaptive_prediction_uses_median_and_bounded_lead() {
        let now = Utc.with_ymd_and_hms(2025, 1, 4, 0, 0, 0).unwrap();
        let observations = [
            Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2025, 1, 2, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2025, 1, 3, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2025, 1, 4, 0, 0, 0).unwrap(),
        ];
        assert_eq!(
            next_adaptive_check_at(&observations, now, 0),
            Utc.with_ymd_and_hms(2025, 1, 4, 23, 0, 0).unwrap()
        );

        let outlier = [
            observations[0],
            observations[1],
            observations[2],
            Utc.with_ymd_and_hms(2025, 1, 13, 0, 0, 0).unwrap(),
        ];
        let outlier_now = Utc.with_ymd_and_hms(2025, 1, 13, 0, 0, 0).unwrap();
        assert_eq!(
            next_adaptive_check_at(&outlier, outlier_now, 0),
            Utc.with_ymd_and_hms(2025, 1, 13, 23, 0, 0).unwrap()
        );
    }

    #[test]
    fn adaptive_cold_start_misses_and_failures_are_bounded() {
        let now = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
        assert_eq!(
            next_adaptive_check_at(&[], now, 0),
            now + chrono::TimeDelta::days(1)
        );
        assert_eq!(
            next_adaptive_check_at(&[], now, 6),
            now + chrono::TimeDelta::hours(24)
        );
        assert_eq!(
            failure_retry_at(now, 1, None),
            now + chrono::TimeDelta::minutes(5)
        );
        assert_eq!(
            failure_retry_at(now, 9, None),
            now + chrono::TimeDelta::hours(1)
        );
        assert_eq!(
            failure_retry_at(now, 5, None),
            now + chrono::TimeDelta::hours(1)
        );
        assert_eq!(
            failure_retry_at(now, 1, Some(1_800)),
            now + chrono::TimeDelta::minutes(30)
        );
    }

    #[tokio::test]
    async fn selection_pause_survives_restart_and_preserves_queued_validator_retry() {
        let (directory, pool) = fixture_pool().await;
        let now = Utc::now();
        seed_list(
            &pool,
            1,
            AddonMediaListRefreshCadence::Daily,
            "https://one.example.test/list",
        )
        .await;
        seed_list(
            &pool,
            2,
            AddonMediaListRefreshCadence::Daily,
            "https://two.example.test/list",
        )
        .await;
        let coordinator = AddonMediaListRefreshCoordinator::new(pool.clone());
        assert_eq!(coordinator.admit_due(now).await.unwrap(), 2);
        let retry_at = encode_datetime(now + TimeDelta::hours(1));
        sqlx::query(r#"UPDATE "AddonMediaListRefreshState" SET "Validator"='source-etag',"AttemptCount"=3,"NextEligibleAt"=? WHERE "AddonMediaListId"=1"#)
            .bind(&retry_at).execute(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO "AddonMediaListSelection" ("AddonMediaListId","RefreshPaused") VALUES (1,1)"#).execute(&pool).await.unwrap();
        let claimed = coordinator.claim_one(now, 7).await.unwrap().unwrap();
        assert_eq!(claimed.list_id, 2);
        sqlx::query(r#"UPDATE "AddonMediaListRefreshState" SET "State"=?,"LeaseOwner"=NULL,"LeaseExpiresAt"=NULL WHERE "AddonMediaListId"=2"#)
            .bind(encode_enum(AddonMediaListRefreshState::Idle)).execute(&pool).await.unwrap();
        sqlx::query(r#"UPDATE "AddonMediaList" SET "NextRefreshAt"=? WHERE "Id"=2"#)
            .bind(encode_datetime(now + TimeDelta::days(1)))
            .execute(&pool)
            .await
            .unwrap();
        assert!(coordinator
            .claim_one(now + TimeDelta::hours(2), 7)
            .await
            .unwrap()
            .is_none());
        pool.close().await;
        let url = format!("sqlite:{}", directory.path().join("refresh.db").display());
        let pool = crate::db::init(&url).await.unwrap();
        let coordinator = AddonMediaListRefreshCoordinator::new(pool.clone());
        assert_eq!(coordinator.admit_due(now).await.unwrap(), 0);
        assert!(coordinator.claim_one(now, 7).await.unwrap().is_none());
        assert_eq!(sqlx::query_as::<_,(String,i32,String)>(r#"SELECT "Validator","AttemptCount","NextEligibleAt" FROM "AddonMediaListRefreshState" WHERE "AddonMediaListId"=1"#)
            .fetch_one(&pool).await.unwrap(),("source-etag".into(),3,retry_at));
        sqlx::query(
            r#"UPDATE "AddonMediaListSelection" SET "RefreshPaused"=0 WHERE "AddonMediaListId"=1"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(coordinator.claim_one(now, 7).await.unwrap().is_none());
        let resumed = coordinator
            .claim_one(now + TimeDelta::hours(2), 7)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(resumed.list_id, 1);
        assert_eq!(resumed.validator.as_deref(), Some("source-etag"));
        assert_eq!(resumed.attempt_count, 3);
    }

    #[tokio::test]
    async fn fixed_admission_is_durable_and_filters_disabled_and_never() {
        let (_temporary, pool) = fixture_pool().await;
        for (id, cadence) in [
            (1, AddonMediaListRefreshCadence::Daily),
            (2, AddonMediaListRefreshCadence::Weekly),
            (3, AddonMediaListRefreshCadence::Monthly),
            (4, AddonMediaListRefreshCadence::Yearly),
            (5, AddonMediaListRefreshCadence::Never),
        ] {
            seed_list(&pool, id, cadence, "https://example.test/list").await;
        }
        let now = Utc.with_ymd_and_hms(2026, 9, 4, 12, 0, 0).unwrap();
        let coordinator = AddonMediaListRefreshCoordinator::new(pool.clone());
        assert_eq!(coordinator.admit_due(now).await.unwrap(), 4);
        assert_eq!(coordinator.admit_due(now).await.unwrap(), 0);
        assert_eq!(
            AddonMediaListRefreshCoordinator::new(pool.clone())
                .admit_due(now)
                .await
                .unwrap(),
            0
        );
        let queued: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "AddonMediaListRefreshState" WHERE "State" = ?"#,
        )
        .bind(encode_enum(AddonMediaListRefreshState::Queued))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(queued, 4);

        sqlx::query(
            r#"UPDATE "AddonInstallation" SET "Enabled" = 0
               WHERE "AddonId" = 'org.example.refresh'"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        seed_list(
            &pool,
            6,
            AddonMediaListRefreshCadence::Daily,
            "https://other.test/list",
        )
        .await;
        assert_eq!(coordinator.admit_due(now).await.unwrap(), 0);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn restored_source_check_capability_requeues_the_retained_adaptive_cadence() {
        let (temporary, pool) = fixture_pool().await;
        install_source_check_fixture(&temporary, &pool, "#!/bin/sh\nexit 0\n").await;
        seed_list(
            &pool,
            1,
            AddonMediaListRefreshCadence::Adaptive,
            "https://example.test/list",
        )
        .await;
        let now = Utc.with_ymd_and_hms(2026, 9, 4, 12, 0, 0).unwrap();
        sqlx::query(r#"UPDATE "AddonMediaList" SET "NextRefreshAt" = ? WHERE "Id" = 1"#)
            .bind(encode_datetime(now))
            .execute(&pool)
            .await
            .expect("make retained adaptive cadence due");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListRefreshState" (
                   "AddonMediaListId", "Generation", "State", "DiagnosticCode", "UpdatedAt"
               ) VALUES (1, 1, ?, 'capability-unavailable', ?)"#,
        )
        .bind(encode_enum(AddonMediaListRefreshState::Failed))
        .bind(encode_datetime(now))
        .execute(&pool)
        .await
        .expect("seed capability-loss state");

        let coordinator = AddonMediaListRefreshCoordinator::new(pool.clone());
        assert_eq!(coordinator.admit_due(now).await.unwrap(), 1);
        let row = sqlx::query(
            r#"SELECT "Generation", "State", "DiagnosticCode"
               FROM "AddonMediaListRefreshState" WHERE "AddonMediaListId" = 1"#,
        )
        .fetch_one(&pool)
        .await
        .expect("read recovered adaptive state");
        assert_eq!(row.try_get::<i32, _>("Generation").unwrap(), 2);
        assert_eq!(
            row.try_get::<i32, _>("State").unwrap(),
            encode_enum(AddonMediaListRefreshState::Queued)
        );
        assert!(row
            .try_get::<Option<String>, _>("DiagnosticCode")
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn provider_host_claim_is_shared_between_lists() {
        let (_temporary, pool) = fixture_pool().await;
        seed_list(
            &pool,
            1,
            AddonMediaListRefreshCadence::Daily,
            "https://same.example.test/one",
        )
        .await;
        seed_list(
            &pool,
            2,
            AddonMediaListRefreshCadence::Daily,
            "https://same.example.test/two",
        )
        .await;
        let now = Utc.with_ymd_and_hms(2026, 9, 4, 12, 0, 0).unwrap();
        let coordinator = AddonMediaListRefreshCoordinator::new(pool.clone());
        assert_eq!(coordinator.admit_due(now).await.unwrap(), 2);
        let first = coordinator.claim_one(now, 7).await.unwrap().unwrap();
        assert_eq!(first.provider_host, "same.example.test");
        assert!(coordinator.claim_one(now, 9).await.unwrap().is_none());
        let cooldown = sqlx::query(
            r#"SELECT "SampledDelaySeconds", "LeaseOwner"
               FROM "AddonProviderHostCooldown" WHERE "ProviderHost" = ?"#,
        )
        .bind("same.example.test")
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(cooldown.get::<i64, _>("SampledDelaySeconds"), 7);
        assert_eq!(
            cooldown.get::<Option<String>, _>("LeaseOwner").as_deref(),
            Some(first.lease_owner.as_str())
        );
    }

    #[test]
    fn provider_recovery_fence_is_independent_of_cooldown_expiry() {
        // Recovery has no timeout: neither a different host nor an expired
        // database lease supplies proof that the old child process has exited.
        assert!(ensure_provider_recovery_not_required(false).is_ok());
        let failure = ensure_provider_recovery_not_required(true).unwrap_err();
        assert!(failure
            .to_string()
            .contains("provider process cleanup requires host recovery"));
    }

    #[tokio::test]
    async fn successful_refresh_anchors_the_next_fixed_due_time() {
        let (_temporary, pool) = fixture_pool().await;
        seed_list(
            &pool,
            1,
            AddonMediaListRefreshCadence::Daily,
            "https://example.test/list",
        )
        .await;
        let now = Utc.with_ymd_and_hms(2026, 9, 4, 12, 0, 0).unwrap();
        let mut tx = crate::db::begin_write(&pool).await.unwrap();
        record_refresh_success_tx(&mut tx, 1, now).await.unwrap();
        tx.commit().await.unwrap();
        let row = sqlx::query(
            r#"SELECT "LastRefreshSucceededAt", "NextRefreshAt"
               FROM "AddonMediaList" WHERE "Id" = 1"#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            row.get::<Option<String>, _>("LastRefreshSucceededAt")
                .as_deref(),
            Some(encode_datetime(now).as_str())
        );
        assert_eq!(
            row.get::<Option<String>, _>("NextRefreshAt").as_deref(),
            Some(encode_datetime(now + chrono::TimeDelta::days(1)).as_str())
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    #[allow(clippy::too_many_lines)] // One end-to-end fixture asserts the complete two-probe transition.
    async fn adaptive_changed_then_unchanged_forwards_validator_without_reimporting() {
        const SCRIPT: &str = r#"#!/bin/sh
set -eu
state='__STATE_DIR__'
case "$1" in
  check)
    request=$(cat)
    count=0
    test ! -f "$state/check-count" || count=$(cat "$state/check-count")
    if test "$count" -eq 0; then
      test "$request" = '{"source_url":"https://example.test/feed"}'
      printf '1' > "$state/check-count"
      printf '%s\n' '{"outcome":"changed","validator":"validator-v1","source_updated_at":["2026-08-01T12:00:00Z","2026-08-02T12:00:00Z","2026-08-03T12:00:00Z","2026-08-04T12:00:00Z","2026-08-05T12:00:00Z","2026-08-06T12:00:00Z","2026-08-07T12:00:00Z","2026-08-08T12:00:00Z","2026-08-09T12:00:00Z","2026-08-10T12:00:00Z","2026-08-11T12:00:00Z","2026-08-12T12:00:00Z","2026-08-13T12:00:00Z","2026-08-14T12:00:00Z","2026-08-15T12:00:00Z","2026-08-16T12:00:00Z"]}'
    else
      test "$request" = '{"source_url":"https://example.test/feed","validator":"validator-v1"}'
      printf '2' > "$state/check-count"
      printf '%s\n' '{"outcome":"unchanged","validator":"validator-v2"}'
    fi
    ;;
  list)
    count=0
    test ! -f "$state/list-count" || count=$(cat "$state/list-count")
    count=$((count + 1))
    printf '%s' "$count" > "$state/list-count"
    printf '%s\n' '{"record_type":"list","provider_id":"provider-1","name":"Fixture"}'
    printf '%s\n' '{"record_type":"item","provider_id":"movie:1","rank":0,"display_title":"Arrival","title":"Arrival","kind":"movie","guids":[]}'
    ;;
  *) exit 9 ;;
esac
"#;

        let (temporary, pool) = fixture_pool().await;
        let state = temporary.path().join("state");
        tokio::fs::create_dir(&state)
            .await
            .expect("create fixture state folder");
        let script = SCRIPT.replace(
            "__STATE_DIR__",
            state.to_str().expect("fixture state path is UTF-8"),
        );
        install_source_check_fixture(&temporary, &pool, &script).await;
        seed_list(
            &pool,
            1,
            AddonMediaListRefreshCadence::Adaptive,
            "https://example.test/feed",
        )
        .await;
        for day in 1..=5 {
            sqlx::query(
                r#"INSERT INTO "AddonMediaListSourceChangeObservation" (
                       "AddonMediaListId", "SourceUpdatedAt"
                   ) VALUES (1, ?)"#,
            )
            .bind(format!("2026-07-{day:02}T12:00:00Z"))
            .execute(&pool)
            .await
            .expect("seed older source-change observation");
        }

        let first = Utc.with_ymd_and_hms(2026, 9, 4, 12, 0, 0).unwrap();
        let coordinator = AddonMediaListRefreshCoordinator::new(pool.clone());
        assert_eq!(coordinator.admit_due(first).await.unwrap(), 1);
        assert!(coordinator.process_one(first).await.unwrap());

        let changed = sqlx::query(
            r#"SELECT "State", "Validator", "MissCount", "LastOutcome"
               FROM "AddonMediaListRefreshState" WHERE "AddonMediaListId" = 1"#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            changed.get::<i32, _>("State"),
            encode_enum(AddonMediaListRefreshState::Idle)
        );
        assert_eq!(changed.get::<String, _>("Validator"), "validator-v1");
        assert_eq!(changed.get::<i64, _>("MissCount"), 0);
        assert_eq!(
            changed.get::<i32, _>("LastOutcome"),
            encode_enum(AddonMediaListSourceCheckOutcome::Changed)
        );
        let history: (i64, String, String) = sqlx::query_as(
            r#"SELECT COUNT(*), MIN("SourceUpdatedAt"), MAX("SourceUpdatedAt")
               FROM "AddonMediaListSourceChangeObservation"
               WHERE "AddonMediaListId" = 1"#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(history.0, 16);
        assert_eq!(history.1, "2026-08-01 12:00:00");
        assert_eq!(history.2, "2026-08-16 12:00:00");
        let draft_after_change: i32 =
            sqlx::query_scalar(r#"SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id" = 1"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            tokio::fs::read_to_string(state.join("list-count"))
                .await
                .unwrap(),
            "1"
        );
        let import_jobs_after_change: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "AddonMediaListImportJob" WHERE "AddonMediaListId" = 1"#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        let next_due: String =
            sqlx::query_scalar(r#"SELECT "NextRefreshAt" FROM "AddonMediaList" WHERE "Id" = 1"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        let second = decode_datetime("adaptive next due", &next_due).unwrap()
            + chrono::TimeDelta::seconds(1);
        assert_eq!(coordinator.admit_due(second).await.unwrap(), 1);
        assert!(coordinator.process_one(second).await.unwrap());
        let unchanged = sqlx::query(
            r#"SELECT "State", "Validator", "MissCount", "LastOutcome"
               FROM "AddonMediaListRefreshState" WHERE "AddonMediaListId" = 1"#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            unchanged.get::<i32, _>("State"),
            encode_enum(AddonMediaListRefreshState::Idle)
        );
        assert_eq!(unchanged.get::<String, _>("Validator"), "validator-v2");
        assert_eq!(unchanged.get::<i64, _>("MissCount"), 1);
        assert_eq!(
            unchanged.get::<i32, _>("LastOutcome"),
            encode_enum(AddonMediaListSourceCheckOutcome::Unchanged)
        );
        assert_eq!(
            tokio::fs::read_to_string(state.join("check-count"))
                .await
                .unwrap(),
            "2"
        );
        assert_eq!(
            tokio::fs::read_to_string(state.join("list-count"))
                .await
                .unwrap(),
            "1"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i32>(
                r#"SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id" = 1"#,
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            draft_after_change
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                r#"SELECT COUNT(*) FROM "AddonMediaListImportJob"
                   WHERE "AddonMediaListId" = 1"#,
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            import_jobs_after_change
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                r#"SELECT COUNT(*) FROM "AddonMediaListSourceChangeObservation"
                   WHERE "AddonMediaListId" = 1"#,
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            16
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn adaptive_transient_failure_persists_retry_after() {
        const SCRIPT: &str = r#"#!/bin/sh
set -eu
test "$1" = 'check'
request=$(cat)
test "$request" = '{"source_url":"https://example.test/feed"}'
printf '%s\n' '{"outcome":"transient_failure","code":"provider-rate-limited","retry_after_seconds":1800}'
"#;

        let (temporary, pool) = fixture_pool().await;
        install_source_check_fixture(&temporary, &pool, SCRIPT).await;
        seed_list(
            &pool,
            1,
            AddonMediaListRefreshCadence::Adaptive,
            "https://example.test/feed",
        )
        .await;
        let now = Utc.with_ymd_and_hms(2026, 9, 4, 12, 0, 0).unwrap();
        let coordinator = AddonMediaListRefreshCoordinator::new(pool.clone());
        assert_eq!(coordinator.admit_due(now).await.unwrap(), 1);
        assert!(coordinator.process_one(now).await.unwrap());

        let row = sqlx::query(
            r#"SELECT "refresh"."State", "refresh"."AttemptCount",
                      "refresh"."NextEligibleAt", "refresh"."LastOutcome",
                      "refresh"."DiagnosticCode", "list"."RefreshRetryAt"
               FROM "AddonMediaListRefreshState" AS "refresh"
               JOIN "AddonMediaList" AS "list"
                 ON "list"."Id" = "refresh"."AddonMediaListId"
               WHERE "refresh"."AddonMediaListId" = 1"#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let expected_retry = encode_datetime(now + chrono::TimeDelta::minutes(30));
        assert_eq!(
            row.get::<i32, _>("State"),
            encode_enum(AddonMediaListRefreshState::WaitingToRetry)
        );
        assert_eq!(row.get::<i64, _>("AttemptCount"), 1);
        assert_eq!(row.get::<String, _>("NextEligibleAt"), expected_retry);
        assert_eq!(
            row.get::<i32, _>("LastOutcome"),
            encode_enum(AddonMediaListSourceCheckOutcome::TransientFailure)
        );
        assert_eq!(
            row.get::<String, _>("DiagnosticCode"),
            "provider-rate-limited"
        );
        assert_eq!(row.get::<String, _>("RefreshRetryAt"), expected_retry);
    }

    // External official package is optional in CI. This test installs its exact
    // bytes into a throwaway DB and uses only a synthetic native-command fixture.
    #[cfg(unix)]
    #[tokio::test]
    #[ignore = "requires ERSATZRS_YT_DLP_CANDIDATE package directory and Deno on PATH"]
    #[allow(clippy::too_many_lines)] // One installed-candidate scheduler transition.
    async fn installed_yt_dlp_candidate_adaptive_unchanged_then_changed() {
        use std::os::unix::fs::PermissionsExt as _;

        #[allow(clippy::disallowed_methods)]
        // Explicit manual-test fixture input, not application configuration.
        let candidate = std::path::PathBuf::from(
            std::env::var_os("ERSATZRS_YT_DLP_CANDIDATE").expect("candidate package directory"),
        );
        let manifest: ersatzrs_addon_contract::AddonManifest = toml::from_str(
            &tokio::fs::read_to_string(candidate.join("addon.toml"))
                .await
                .unwrap(),
        )
        .unwrap();
        manifest.validate().unwrap();
        // This is the exact manifest predicate projected into the admin provider
        // DTO's supports_adaptive_refresh flag.
        assert!(manifest.has_media_list_source_check_capability());
        assert_eq!(manifest.id, "org.ersatzrs.addon.yt-dlp");
        assert!(manifest
            .settings
            .iter()
            .any(|setting| setting.key == "YT_DLP_BIN"));
        let (temporary, pool) = fixture_pool().await;
        let package = temporary.path().join("installed");
        let mut pending = vec![(candidate, package.clone())];
        while let Some((source, target)) = pending.pop() {
            tokio::fs::create_dir_all(&target).await.unwrap();
            let mut entries = tokio::fs::read_dir(source).await.unwrap();
            while let Some(entry) = entries.next_entry().await.unwrap() {
                let destination = target.join(entry.file_name());
                if entry.file_type().await.unwrap().is_dir() {
                    pending.push((entry.path(), destination));
                } else {
                    tokio::fs::copy(entry.path(), destination).await.unwrap();
                }
            }
        }
        let digest = crate::addons::calculate_addon_content_sha256(&package)
            .await
            .unwrap();
        sqlx::query(
            r#"INSERT INTO "AddonInstallation" (
                   "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
                   "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt"
               ) VALUES (?, ?, 1, 'fixture', ?, ?, ?, ?, ?)"#,
        )
        .bind(&manifest.id)
        .bind(manifest.version.to_string())
        .bind(&digest)
        .bind(serde_json::to_string(&manifest).unwrap())
        .bind(package.to_string_lossy().as_ref())
        .bind(encode_datetime(Utc::now()))
        .bind(encode_datetime(Utc::now()))
        .execute(&pool)
        .await
        .unwrap();

        let provider_json = temporary.path().join("provider.json");
        let native = temporary.path().join("fixture-yt-dlp");
        // All paths are generated by tempfile; shell quoting remains explicit.
        let quoted = provider_json.to_string_lossy().replace('\'', "'\"'\"'");
        tokio::fs::write(&native, format!("#!/bin/sh\ncat '{quoted}'\n"))
            .await
            .unwrap();
        tokio::fs::set_permissions(&native, std::fs::Permissions::from_mode(0o755))
            .await
            .unwrap();
        sqlx::query(
            r#"INSERT INTO "AddonSetting" ("AddonId", "Key", "Value") VALUES (?, 'YT_DLP_BIN', ?)"#,
        )
        .bind(&manifest.id)
        .bind(native.to_string_lossy().as_ref())
        .execute(&pool)
        .await
        .unwrap();
        let source = "https://media.example.test/list";
        seed_list(&pool, 1, AddonMediaListRefreshCadence::Adaptive, source).await;
        sqlx::query(r#"UPDATE "AddonMediaList" SET "AddonId" = ? WHERE "Id" = 1"#)
            .bind(&manifest.id)
            .execute(&pool)
            .await
            .unwrap();
        // Runtime dependency validation reads every installed manifest. Remove
        // the generic seed row now that its only list belongs to the candidate.
        sqlx::query(r#"DELETE FROM "AddonInstallation" WHERE "AddonId" = 'org.example.refresh'"#)
            .execute(&pool)
            .await
            .unwrap();
        tokio::fs::write(&provider_json, r#"{"_type":"playlist","id":"fixture","title":"Before","extractor_key":"Generic","entries":[{"id":"one"}]}"#)
            .await.unwrap();
        let provider = crate::addon_media_list::InstalledAddonMediaListProvider::new(pool.clone());
        assert!(provider.supports_source_check(&manifest.id).await.unwrap());
        let first = provider
            .source_check(&manifest.id, source, None)
            .await
            .unwrap();
        assert_eq!(
            first.outcome,
            ersatzrs_addon_contract::MediaListSourceCheckOutcome::Changed,
            "source-check diagnostic: {:?}",
            first.code
        );
        assert!(first.source_updated_at.is_empty());

        let coordinator = AddonMediaListRefreshCoordinator::new(pool.clone());
        let now = Utc::now();
        assert_eq!(coordinator.admit_due(now).await.unwrap(), 1);
        sqlx::query(r#"UPDATE "AddonMediaListRefreshState" SET "Validator" = ? WHERE "AddonMediaListId" = 1"#)
            .bind(first.validator.as_deref()).execute(&pool).await.unwrap();
        assert!(coordinator.process_one(now).await.unwrap());
        let unchanged: (i32, i64) = sqlx::query_as(
            r#"SELECT "LastOutcome", "MissCount" FROM "AddonMediaListRefreshState" WHERE "AddonMediaListId" = 1"#,
        ).fetch_one(&pool).await.unwrap();
        assert_eq!(
            unchanged,
            (encode_enum(AddonMediaListSourceCheckOutcome::Unchanged), 1)
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM "AddonMediaListImportJob""#)
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
        let revision: i32 =
            sqlx::query_scalar(r#"SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id" = 1"#)
                .fetch_one(&pool)
                .await
                .unwrap();

        tokio::fs::write(&provider_json, r#"{"_type":"playlist","id":"fixture","title":"After","extractor_key":"Generic","entries":[{"id":"one"},{"id":"two"}]}"#)
            .await.unwrap();
        let due: String =
            sqlx::query_scalar(r#"SELECT "NextRefreshAt" FROM "AddonMediaList" WHERE "Id" = 1"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        let later =
            decode_datetime("candidate next due", &due).unwrap() + chrono::TimeDelta::seconds(1);
        assert_eq!(coordinator.admit_due(later).await.unwrap(), 1);
        assert!(coordinator.process_one(later).await.unwrap());
        let changed: (i32, String) = sqlx::query_as(
            r#"SELECT "LastOutcome", "Validator" FROM "AddonMediaListRefreshState" WHERE "AddonMediaListId" = 1"#,
        ).fetch_one(&pool).await.unwrap();
        assert_eq!(
            changed.0,
            encode_enum(AddonMediaListSourceCheckOutcome::Changed)
        );
        assert_ne!(Some(changed.1), first.validator);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                r#"SELECT COUNT(*) FROM "AddonMediaListImportJob" WHERE "AddonMediaListId" = 1"#
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i32>(
                r#"SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id" = 1"#
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            revision
        );
        assert_eq!(
            crate::addons::calculate_addon_content_sha256(&package)
                .await
                .unwrap(),
            digest
        );
    }
}
