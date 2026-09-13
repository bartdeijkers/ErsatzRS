//! Durable M01 item work, independent from publication leases and file execution.
use crate::{
    addon_media_list_options::load_draft_item_options,
    db::{Db, DbPool},
    repositories::{common::encode_datetime, SqlxResultExt as _},
};
use chrono::{DateTime, Duration, Utc};
use ersatzrs_addon_contract::{
    validate_tool_version, AddonCapability, AddonMediaListOptionValues,
    MediaListAccessCheckOutcome, MediaListAccessCheckRequest, MAX_MEDIA_LIST_IMPORT_REQUEST_BYTES,
};
use ersatzrs_core::addon_media_list_playback::AddonMediaListPlaybackTimelinePlan;
use ersatzrs_core::repositories::{AddonMediaListOptionSchemaRef, RepositoryError, Result};
use ersatzrs_domain::RemoteStreamLiveness;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use sqlx::Row;
use std::{fmt::Write as _, ops::Range};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemOperationKind {
    Access,
    Preparation,
    IntervalMetadata,
    Subtitles,
}
impl ItemOperationKind {
    fn code(self) -> i32 {
        match self {
            Self::Access => 0,
            Self::Preparation => 1,
            Self::IntervalMetadata => 2,
            Self::Subtitles => 3,
        }
    }
}

/// Complete effective options are frozen; invocation filters by operation target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemOperationInput {
    pub source_url: String,
    pub provider_id: String,
    pub context_fingerprint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<AddonMediaListOptionSchemaRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition: Option<AddonCapability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<AddonMediaListOptionValues>,
    /// Explicit identity for schema-less preparation or subtitle acquisition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub addon_id: Option<String>,
    /// Absent for all legacy v1 jobs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preparation_v2: Option<bool>,
    pub package_identity: String,
    pub tool_version: String,
    pub item_is_finite: bool,
    pub max_video_height: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_timing: Option<ItemOperationTiming>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requested_subtitle_languages: Vec<String>,
}

impl ItemOperationInput {
    pub(crate) fn base_fingerprint(&self) -> Result<String> {
        let fingerprint = self.options_fingerprint()?;
        if self.requested_subtitle_languages.is_empty() {
            return Ok(fingerprint);
        }
        let mut hash = Sha256::new();
        hash.update(b"ersatzrs:addon-media-list:subtitle-languages:v1\0");
        hash.update(fingerprint.as_bytes());
        hash.update(
            serde_json::to_vec(&self.requested_subtitle_languages).map_err(|_| conflict())?,
        );
        let mut result = String::with_capacity(64);
        for byte in hash.finalize() {
            write!(result, "{byte:02x}").map_err(|_| conflict())?;
        }
        Ok(result)
    }

    fn options_fingerprint(&self) -> Result<String> {
        if let Some(options) = self.resolved_options()? {
            let fingerprint = options.context_fingerprint(
                &self.source_url,
                &self.provider_id,
                &self.package_identity,
                &self.tool_version,
                self.max_video_height,
            )?;
            return if self.preparation_v2 == Some(true) {
                let mut hash = Sha256::new();
                hash.update(b"ersatzrs:addon-media-list:generic-preparation:v2\0");
                hash.update(fingerprint.as_bytes());
                let mut result = String::with_capacity(64);
                for byte in hash.finalize() {
                    write!(result, "{byte:02x}").map_err(|_| conflict())?;
                }
                Ok(result)
            } else {
                Ok(fingerprint)
            };
        }
        let bytes = serde_json::to_vec(&(
            self.addon_id()?,
            &self.source_url,
            &self.provider_id,
            &self.package_identity,
            &self.tool_version,
            self.max_video_height,
        ))
        .map_err(|_| conflict())?;
        let mut hash = Sha256::new();
        if self.preparation_v2 == Some(true) {
            hash.update(b"ersatzrs:addon-media-list:preparation-without-options:v2\0");
        } else {
            hash.update(b"ersatzrs:addon-media-list:subtitles-without-options:v1\0");
        }
        hash.update(bytes);
        let mut result = String::with_capacity(64);
        for byte in hash.finalize() {
            write!(result, "{byte:02x}").map_err(|_| conflict())?;
        }
        Ok(result)
    }
    pub(crate) fn addon_id(&self) -> Result<&str> {
        match (&self.schema, &self.addon_id) {
            (Some(schema), None) => Ok(&schema.addon_id),
            (None, Some(id))
                if self.preparation_v2 == Some(true)
                    || !self.requested_subtitle_languages.is_empty() =>
            {
                Ok(id)
            }
            _ => Err(conflict()),
        }
    }

    pub(crate) fn resolved_options(
        &self,
    ) -> Result<Option<crate::addon_media_list_options::ResolvedAddonItemOptions>> {
        match (&self.schema, &self.definition, &self.options) {
            (Some(schema), Some(definition), Some(values)) => Ok(Some(
                crate::addon_media_list_options::ResolvedAddonItemOptions {
                    schema: schema.clone(),
                    definition: definition.clone(),
                    values: values.clone(),
                },
            )),
            (None, None, None)
                if self.preparation_v2 == Some(true)
                    || !self.requested_subtitle_languages.is_empty() =>
            {
                Ok(None)
            }
            _ => Err(conflict()),
        }
    }
}

/// Saved draft timing bound into interval metadata request identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemOperationTiming {
    pub item_duration_seconds: Option<u64>,
    pub fragment_start_seconds: Option<u64>,
    pub fragment_end_seconds: Option<u64>,
}

impl ItemOperationTiming {
    /// Bind exact saved timing to the existing publication context fingerprint.
    ///
    /// # Errors
    /// Rejects malformed fingerprints and timing outside signed millisecond bounds.
    pub fn bind_context_fingerprint(&self, base: &str) -> Result<String> {
        if !digest(base) {
            return Err(conflict());
        }
        for seconds in [
            self.item_duration_seconds,
            self.fragment_start_seconds,
            self.fragment_end_seconds,
        ]
        .into_iter()
        .flatten()
        {
            milliseconds(seconds)?;
        }
        let mut hash = Sha256::new();
        hash.update(b"ersatzrs:addon-media-list:interval-timing:v1\0");
        hash.update(base.as_bytes());
        hash.update(serde_json::to_vec(self).map_err(|_| conflict())?);
        let mut result = String::with_capacity(64);
        for byte in hash.finalize() {
            write!(result, "{byte:02x}").map_err(|_| conflict())?;
        }
        Ok(result)
    }

    /// Resolve the requested source window using the provider's probed duration.
    ///
    /// # Errors
    /// Rejects overflow, invalid source duration, and empty or out-of-source windows.
    pub fn source_window_milliseconds(&self, probed_duration: u64) -> Result<Range<u64>> {
        let start = self
            .fragment_start_seconds
            .map(milliseconds)
            .transpose()?
            .unwrap_or(0);
        let end = self
            .fragment_end_seconds
            .map(milliseconds)
            .transpose()?
            .unwrap_or(probed_duration);
        if probed_duration == 0
            || probed_duration > i64::MAX as u64
            || start >= end
            || end > probed_duration
        {
            return Err(conflict());
        }
        Ok(start..end)
    }
}

fn milliseconds(seconds: u64) -> Result<u64> {
    seconds
        .checked_mul(1_000)
        .filter(|value| i64::try_from(*value).is_ok())
        .ok_or_else(conflict)
}

fn validate_mode(kind: ItemOperationKind, input: &ItemOperationInput) -> Result<()> {
    input.addon_id()?;
    input.resolved_options()?;
    ersatzrs_addon_contract::validate_requested_subtitle_languages(
        &input.requested_subtitle_languages,
        true,
    )
    .map_err(|_| conflict())?;
    if !input.requested_subtitle_languages.is_empty()
        && !matches!(
            kind,
            ItemOperationKind::Subtitles | ItemOperationKind::Preparation
        )
    {
        return Err(conflict());
    }
    if input.preparation_v2.is_some()
        && (input.preparation_v2 != Some(true) || kind != ItemOperationKind::Preparation)
    {
        return Err(conflict());
    }
    if match kind {
        ItemOperationKind::IntervalMetadata => {
            !input.item_is_finite || input.requested_timing.is_none()
        }
        ItemOperationKind::Access => input.requested_timing.is_some(),
        ItemOperationKind::Preparation => !input.item_is_finite || input.requested_timing.is_some(),
        ItemOperationKind::Subtitles => {
            !input.item_is_finite
                || input.requested_timing.is_some()
                || input.requested_subtitle_languages.is_empty()
        }
    } {
        return Err(conflict());
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct ClaimedItemOperation {
    pub id: i64,
    pub list_id: i64,
    pub item_id: i64,
    pub saved_draft_revision: i32,
    pub kind: ItemOperationKind,
    pub generation: i64,
    pub lease_owner: String,
    pub input: ItemOperationInput,
}

pub use ersatzrs_core::repositories::AddonMediaListPreparationFailureReason as ItemOperationFailure;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemOperationCompletion {
    Access {
        outcome: MediaListAccessCheckOutcome,
        expires_at: DateTime<Utc>,
    },
    Prepared {
        artifact_id: i64,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        subtitle_artifact_ids: Vec<i64>,
    },
    IntervalPlan {
        plan: AddonMediaListPlaybackTimelinePlan,
    },
    Subtitles {
        artifact_ids: Vec<i64>,
    },
    Failed(ItemOperationFailure),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemOperationStatus {
    pub id: i64,
    pub context_fingerprint: String,
    pub tool_version: String,
    /// Queued=0, running=1, completed=2, failed=3, cancelled=4, stale=5.
    pub state: i32,
    pub attempt_count: i32,
    pub next_eligible_at: Option<DateTime<Utc>>,
    pub completion: Option<ItemOperationCompletion>,
}

#[derive(Clone)]
pub struct AddonMediaListItemJobQueue {
    pool: DbPool,
}
impl AddonMediaListItemJobQueue {
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Queue an operation for the saved draft, reusing an identical active job.
    ///
    /// # Errors
    /// Returns an error for invalid or changed input, a conflicting active lease,
    /// or a database failure.
    pub async fn enqueue(
        &self,
        list_id: i64,
        item_id: i64,
        expected_revision: i32,
        kind: ItemOperationKind,
        input: ItemOperationInput,
        now: DateTime<Utc>,
    ) -> Result<i64> {
        self.enqueue_with_retry(list_id, item_id, expected_revision, kind, input, now, true)
            .await
    }

    /// Admit automatic work once per exact saved input, including terminal attempts.
    /// Explicit manual admission remains the way to retry failed work.
    pub(crate) async fn enqueue_once(
        &self,
        list_id: i64,
        item_id: i64,
        expected_revision: i32,
        kind: ItemOperationKind,
        input: ItemOperationInput,
        now: DateTime<Utc>,
    ) -> Result<i64> {
        self.enqueue_with_retry(list_id, item_id, expected_revision, kind, input, now, false)
            .await
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)] // One transaction owns exact-input admission and retry policy.
    async fn enqueue_with_retry(
        &self,
        list_id: i64,
        item_id: i64,
        expected_revision: i32,
        kind: ItemOperationKind,
        input: ItemOperationInput,
        now: DateTime<Utc>,
        retry_terminal: bool,
    ) -> Result<i64> {
        validate_mode(kind, &input)?;
        let resolved = load_draft_item_options(&self.pool, list_id, item_id).await?;
        if input.resolved_options()? != resolved
            || (kind == ItemOperationKind::Preparation && !input.item_is_finite)
            || !digest(&input.package_identity)
            || !digest(&input.context_fingerprint)
            || input
                .max_video_height
                .is_some_and(|height| height == 0 || height > 4320)
        {
            return Err(conflict());
        }
        validate_tool_version(&input.tool_version).map_err(|_| conflict())?;
        let liveness = if input.item_is_finite {
            RemoteStreamLiveness::Finite
        } else {
            RemoteStreamLiveness::Unknown
        };
        if let Some(resolved) = &resolved {
            let options = resolved.for_operation(
                ersatzrs_addon_contract::AddonMediaListOptionOperation::TestAccess,
                liveness,
            )?;
            MediaListAccessCheckRequest {
                source_url: input.source_url.clone(),
                provider_id: input.provider_id.clone(),
                context_fingerprint: input.context_fingerprint.clone(),
                tool_version: input.tool_version.clone(),
                options,
            }
            .validate(&resolved.definition, input.item_is_finite)
            .map_err(|_| conflict())?;
        }
        let base_fingerprint = input.base_fingerprint()?;
        let expected_fingerprint = match &input.requested_timing {
            Some(timing) => timing.bind_context_fingerprint(&base_fingerprint)?,
            None => base_fingerprint,
        };
        if expected_fingerprint != input.context_fingerprint {
            return Err(conflict());
        }
        let json = serde_json::to_string(&input).map_err(|_| conflict())?;
        if json.len() > MAX_MEDIA_LIST_IMPORT_REQUEST_BYTES {
            return Err(conflict());
        }
        let time = encode_datetime(now);
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        current(
            &mut tx,
            list_id,
            item_id,
            expected_revision,
            kind.code(),
            &input,
        )
        .await?;
        if !retry_terminal {
            let admitted: Option<i64> = sqlx::query_scalar(
                r#"SELECT "Id" FROM "AddonMediaListItemOperationJob"
                WHERE "AddonMediaListId"=? AND "AddonMediaListItemId"=? AND "SavedDraftRevision"=?
                  AND "Kind"=? AND "InputJson"=? ORDER BY "Id" DESC LIMIT 1"#,
            )
            .bind(list_id)
            .bind(item_id)
            .bind(expected_revision)
            .bind(kind.code())
            .bind(&json)
            .fetch_optional(&mut *tx)
            .await
            .repository()?;
            if let Some(id) = admitted {
                tx.commit().await.repository()?;
                return Ok(id);
            }
        }
        let cancelled_running: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS(SELECT 1
                FROM "AddonMediaListItemOperationJob"
                WHERE "AddonMediaListItemId"=?
                AND "Kind"=?
                AND "State"=4
                AND "LeaseExpiresAt">?)
        "#,
        )
        .bind(item_id)
        .bind(kind.code())
        .bind(&time)
        .fetch_one(&mut *tx)
        .await
        .repository()?;
        if cancelled_running {
            return Err(conflict());
        }
        if let Some(row) = sqlx::query(
            r#"
            SELECT "Id", "InputJson", "SavedDraftRevision", "LeaseExpiresAt"
                FROM "AddonMediaListItemOperationJob"
                WHERE "AddonMediaListItemId"=?
                AND "Kind"=?
                AND "State" IN (0,1)
        "#,
        )
        .bind(item_id)
        .bind(kind.code())
        .fetch_optional(&mut *tx)
        .await
        .repository()?
        {
            let id: i64 = row.try_get("Id").repository()?;
            if row.try_get::<String, _>("InputJson").repository()? == json
                && row.try_get::<i32, _>("SavedDraftRevision").repository()? == expected_revision
            {
                tx.commit().await.repository()?;
                return Ok(id);
            }
            if row
                .try_get::<Option<String>, _>("LeaseExpiresAt")
                .repository()?
                .is_some_and(|expires| expires > time)
            {
                return Err(conflict());
            }
            sqlx::query(r#"UPDATE "AddonMediaListItemOperationJob" SET "State"=5,"CompletedAt"=?,"UpdatedAt"=? WHERE "Id"=?"#)
                .bind(&time).bind(&time).bind(id).execute(&mut *tx).await.repository()?;
        }
        let id = sqlx::query(
            r#"
            INSERT INTO "AddonMediaListItemOperationJob" ("AddonMediaListId",
                "AddonMediaListItemId",
                "Kind",
                "SavedDraftRevision",
                "InputJson",
                "ContextFingerprint",
                "CreatedAt",
                "UpdatedAt")
                VALUES (?,?,?,?,?,?,?,?)
        "#,
        )
        .bind(list_id)
        .bind(item_id)
        .bind(kind.code())
        .bind(expected_revision)
        .bind(json)
        .bind(&input.context_fingerprint)
        .bind(&time)
        .bind(&time)
        .execute(&mut *tx)
        .await
        .repository()?
        .last_insert_rowid();
        tx.commit().await.repository()?;
        Ok(id)
    }

    /// Claim the oldest eligible operation of any kind.
    ///
    /// # Errors
    /// Returns an error for an invalid lease duration, invalid persisted input,
    /// or a database failure.
    pub async fn claim(
        &self,
        now: DateTime<Utc>,
        lease_seconds: u32,
    ) -> Result<Option<ClaimedItemOperation>> {
        self.claim_selected(now, lease_seconds, None).await
    }

    /// Claim the oldest eligible operation of the requested kind.
    ///
    /// # Errors
    /// Returns an error for an invalid lease duration, invalid persisted input,
    /// or a database failure.
    pub async fn claim_kind(
        &self,
        now: DateTime<Utc>,
        lease_seconds: u32,
        kind: ItemOperationKind,
    ) -> Result<Option<ClaimedItemOperation>> {
        self.claim_selected(now, lease_seconds, Some(kind)).await
    }

    #[allow(clippy::too_many_lines)] // Reap, select, validate and claim under one write transaction.
    async fn claim_selected(
        &self,
        now: DateTime<Utc>,
        lease_seconds: u32,
        kind: Option<ItemOperationKind>,
    ) -> Result<Option<ClaimedItemOperation>> {
        let expires = lease_deadline(now, lease_seconds)?;
        let time = encode_datetime(now);
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        sqlx::query(
            r#"
            UPDATE "AddonMediaListItemOperationJob"
                SET "State"=3,
                "OutcomeJson"=?,
                "CompletedAt"=?,
                "UpdatedAt"=?
                WHERE "State" IN (0,1)
                AND "AttemptCount">=3
                AND ("LeaseExpiresAt" IS NULL
                OR "LeaseExpiresAt"<=?)
        "#,
        )
        .bind(
            serde_json::to_string(&ItemOperationCompletion::Failed(
                ItemOperationFailure::AttemptsExhausted,
            ))
            .map_err(|_| conflict())?,
        )
        .bind(&time)
        .bind(&time)
        .bind(&time)
        .execute(&mut *tx)
        .await
        .repository()?;
        let row = sqlx::query(
            r#"
            SELECT "Id",
                "AddonMediaListId",
                "AddonMediaListItemId",
                "Kind",
                "SavedDraftRevision",
                "InputJson",
                "Generation"
                FROM "AddonMediaListItemOperationJob"
                WHERE "AttemptCount"<3
                AND (("State"=0
                AND ("NextEligibleAt" IS NULL
                OR "NextEligibleAt"<=?))
                OR ("State"=1
                AND "LeaseExpiresAt"<=?))
                AND (? IS NULL OR "Kind"=?)
                AND ("Kind"<>2 OR NOT EXISTS (
                    SELECT 1 FROM "ConfigElement" WHERE "Key"=?))
                ORDER BY "Id" LIMIT 1
        "#,
        )
        .bind(&time)
        .bind(&time)
        .bind(kind.map(ItemOperationKind::code))
        .bind(kind.map(ItemOperationKind::code))
        .bind(crate::addon_media_list_interval_metadata::PROCESS_RECOVERY_KEY)
        .fetch_optional(&mut *tx)
        .await
        .repository()?;
        let Some(row) = row else {
            tx.commit().await.repository()?;
            return Ok(None);
        };
        let id: i64 = row.try_get("Id").repository()?;
        let job = ClaimedItemOperation {
            id,
            list_id: row.try_get("AddonMediaListId").repository()?,
            item_id: row.try_get("AddonMediaListItemId").repository()?,
            saved_draft_revision: row.try_get("SavedDraftRevision").repository()?,
            kind: match row.try_get::<i32, _>("Kind").repository()? {
                0 => ItemOperationKind::Access,
                1 => ItemOperationKind::Preparation,
                2 => ItemOperationKind::IntervalMetadata,
                3 => ItemOperationKind::Subtitles,
                _ => return Err(conflict()),
            },
            generation: row.try_get::<i64, _>("Generation").repository()? + 1,
            lease_owner: Uuid::new_v4().to_string(),
            input: serde_json::from_str(&row.try_get::<String, _>("InputJson").repository()?)
                .map_err(|_| conflict())?,
        };
        validate_mode(job.kind, &job.input)?;
        match current(
            &mut tx,
            job.list_id,
            job.item_id,
            job.saved_draft_revision,
            job.kind.code(),
            &job.input,
        )
        .await
        {
            Err(RepositoryError::Conflict(_)) => {
                sqlx::query(r#"UPDATE "AddonMediaListItemOperationJob" SET "State"=5,"CompletedAt"=?,"UpdatedAt"=? WHERE "Id"=?"#)
                .bind(&time).bind(&time).bind(id).execute(&mut *tx).await.repository()?;
                tx.commit().await.repository()?;
                return Ok(None);
            }
            Err(error) => return Err(error),
            Ok(()) => {}
        }
        sqlx::query(
            r#"
            UPDATE "AddonMediaListItemOperationJob"
                SET "State"=1,
                "Generation"=?,
                "LeaseOwner"=?,
                "LeaseExpiresAt"=?,
                "HeartbeatAt"=?,
                "UpdatedAt"=?,
                "AttemptCount"="AttemptCount"+1
                WHERE "Id"=?
        "#,
        )
        .bind(job.generation)
        .bind(&job.lease_owner)
        .bind(expires)
        .bind(&time)
        .bind(&time)
        .bind(id)
        .execute(&mut *tx)
        .await
        .repository()?;
        tx.commit().await.repository()?;
        Ok(Some(job))
    }

    /// Extend a still-owned, unexpired operation lease.
    ///
    /// # Errors
    /// Returns an error for an invalid lease duration, lost ownership or expiry,
    /// or a database failure.
    pub async fn heartbeat(
        &self,
        job: &ClaimedItemOperation,
        now: DateTime<Utc>,
        lease_seconds: u32,
    ) -> Result<()> {
        let expires = lease_deadline(now, lease_seconds)?;
        let time = encode_datetime(now);
        let changed = sqlx::query(
            r#"
            UPDATE "AddonMediaListItemOperationJob"
                SET "LeaseExpiresAt"=MAX("LeaseExpiresAt",?),
                "HeartbeatAt"=?,
                "UpdatedAt"=?
                WHERE "Id"=?
                AND "Generation"=?
                AND "LeaseOwner"=?
                AND "State"=1
                AND "LeaseExpiresAt">?
        "#,
        )
        .bind(expires)
        .bind(&time)
        .bind(&time)
        .bind(job.id)
        .bind(job.generation)
        .bind(&job.lease_owner)
        .bind(&time)
        .execute(&self.pool)
        .await
        .repository()?
        .rows_affected();
        if changed != 1 {
            return Err(conflict());
        }
        Ok(())
    }

    /// Release an owned attempt for a bounded provider backoff; never steals a lease.
    ///
    /// # Errors
    /// Returns an error for a backoff outside the next 24 hours, lost ownership
    /// or expiry, or a database failure.
    pub async fn defer(
        &self,
        job: &ClaimedItemOperation,
        now: DateTime<Utc>,
        next: DateTime<Utc>,
    ) -> Result<()> {
        if next <= now || next > now + Duration::hours(24) {
            return Err(conflict());
        }
        let time = encode_datetime(now);
        let changed = sqlx::query(
            r#"
            UPDATE "AddonMediaListItemOperationJob"
                SET "State"=0,
                "NextEligibleAt"=?,
                "LeaseOwner"=NULL,
                "LeaseExpiresAt"=NULL,
                "UpdatedAt"=?
                WHERE "Id"=?
                AND "Generation"=?
                AND "LeaseOwner"=?
                AND "State"=1
                AND "LeaseExpiresAt">?
        "#,
        )
        .bind(encode_datetime(next))
        .bind(&time)
        .bind(job.id)
        .bind(job.generation)
        .bind(&job.lease_owner)
        .bind(&time)
        .execute(&self.pool)
        .await
        .repository()?
        .rows_affected();
        if changed != 1 {
            return Err(conflict());
        }
        Ok(())
    }

    /// Provider admission refusal did not execute an attempt. Restore its budget.
    ///
    /// # Errors
    /// Returns an error for an invalid backoff, lost ownership or expiry, no
    /// attempt to restore, or a database failure.
    pub async fn defer_unstarted(
        &self,
        job: &ClaimedItemOperation,
        now: DateTime<Utc>,
        next: DateTime<Utc>,
    ) -> Result<()> {
        if next <= now || next > now + Duration::hours(24) {
            return Err(conflict());
        }
        let time = encode_datetime(now);
        let changed = sqlx::query(
            r#"
            UPDATE "AddonMediaListItemOperationJob"
            SET "State"=0, "NextEligibleAt"=?, "LeaseOwner"=NULL,
                "LeaseExpiresAt"=NULL, "UpdatedAt"=?, "AttemptCount"="AttemptCount"-1
            WHERE "Id"=? AND "Generation"=? AND "LeaseOwner"=?
              AND "State"=1 AND "LeaseExpiresAt">? AND "AttemptCount">0
        "#,
        )
        .bind(encode_datetime(next))
        .bind(&time)
        .bind(job.id)
        .bind(job.generation)
        .bind(&job.lease_owner)
        .bind(&time)
        .execute(&self.pool)
        .await
        .repository()?
        .rows_affected();
        if changed != 1 {
            return Err(conflict());
        }
        Ok(())
    }

    /// Requeue only the exact interrupted running attempt after proven cleanup.
    /// Manual cancellation, stale inputs, completed work and existing backoff survive.
    pub(crate) async fn recover_interrupted_after_cleanup(
        &self,
        attempt: &crate::addon_preparation_worker::AttemptIdentity,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        let row = sqlx::query(
            r#"SELECT "AddonMediaListId","AddonMediaListItemId",
                "SavedDraftRevision","Kind","InputJson" FROM "AddonMediaListItemOperationJob"
            WHERE "Id"=? AND "Generation"=? AND "LeaseOwner"=? AND "Kind" IN (1,3) AND "State"=1"#,
        )
        .bind(attempt.job_id)
        .bind(attempt.generation)
        .bind(&attempt.lease_owner)
        .fetch_optional(&mut *tx)
        .await
        .repository()?;
        let Some(row) = row else {
            return Ok(());
        };
        let input: ItemOperationInput =
            serde_json::from_str(&row.try_get::<String, _>("InputJson").repository()?)
                .map_err(|_| conflict())?;
        let state = match current(
            &mut tx,
            row.try_get("AddonMediaListId").repository()?,
            row.try_get("AddonMediaListItemId").repository()?,
            row.try_get("SavedDraftRevision").repository()?,
            row.try_get("Kind").repository()?,
            &input,
        )
        .await
        {
            Ok(()) => 0,
            Err(RepositoryError::Conflict(_)) => 5,
            Err(error) => return Err(error),
        };
        let time = encode_datetime(now);
        sqlx::query(r#"UPDATE "AddonMediaListItemOperationJob" SET "State"=?,
            "NextEligibleAt"=MAX(COALESCE("NextEligibleAt",''),?), "LeaseOwner"=NULL,"LeaseExpiresAt"=NULL,
            "AttemptCount"=MAX(0,"AttemptCount"-1),"UpdatedAt"=?,
            "CompletedAt"=CASE WHEN ?=5 THEN ? ELSE "CompletedAt" END
            WHERE "Id"=? AND "Generation"=? AND "LeaseOwner"=? AND "State"=1"#)
            .bind(state).bind(encode_datetime(now + Duration::seconds(1))).bind(&time)
            .bind(state).bind(&time).bind(attempt.job_id).bind(attempt.generation)
            .bind(&attempt.lease_owner).execute(&mut *tx).await.repository()?;
        tx.commit().await.repository()?;
        Ok(())
    }

    /// Persist an owned attempt's outcome after validating its completion context.
    ///
    /// # Errors
    /// Returns an error for a stale claim, an invalid outcome or artifact owner,
    /// changed successful input, or a database failure.
    #[allow(clippy::too_many_lines)] // Keep outcome validation and its atomic ownership fence together.
    pub async fn complete(
        &self,
        job: &ClaimedItemOperation,
        completion: ItemOperationCompletion,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        validate_mode(job.kind, &job.input)?;
        owned_claim(&mut tx, job, now).await?;
        // A failed owned attempt may report why its input became unusable.
        // Success still requires the current draft; every result retains the
        // owner/generation/state/expiry fence in the terminal UPDATE below.
        if !matches!(completion, ItemOperationCompletion::Failed(_)) {
            current(
                &mut tx,
                job.list_id,
                job.item_id,
                job.saved_draft_revision,
                job.kind.code(),
                &job.input,
            )
            .await?;
        }
        let (artifact, expires) = match &completion {
            ItemOperationCompletion::Access { expires_at, .. }
                if job.kind == ItemOperationKind::Access
                    && *expires_at > now
                    && *expires_at <= now + Duration::hours(24) =>
            {
                (None, Some(encode_datetime(*expires_at)))
            }
            ItemOperationCompletion::Prepared {
                artifact_id,
                subtitle_artifact_ids,
            } if job.kind == ItemOperationKind::Preparation => {
                let owns: bool = sqlx::query_scalar(
                    r#"
            SELECT EXISTS(SELECT 1
                FROM "AddonMediaListArtifact"
                WHERE "Id"=?
                AND "AddonMediaListId"=?
                AND "AddonMediaListItemId"=?
                AND "CacheKey"=?
                AND "Kind"=1
                AND "State"=2
                AND "CompletedAt" IS NOT NULL)
        "#,
                )
                .bind(artifact_id)
                .bind(job.list_id)
                .bind(job.item_id)
                .bind(&job.input.context_fingerprint)
                .fetch_one(&mut *tx)
                .await
                .repository()?;
                if !owns {
                    return Err(conflict());
                }
                let stored: String = sqlx::query_scalar(r#"SELECT COALESCE("PreparedSubtitleIdsJson",'[]') FROM "AddonMediaListArtifact" WHERE "Id"=?"#)
                    .bind(artifact_id).fetch_one(&mut *tx).await.repository()?;
                let stored: Vec<i64> = serde_json::from_str(&stored).map_err(|_| conflict())?;
                if stored != *subtitle_artifact_ids {
                    return Err(conflict());
                }
                validate_subtitle_artifacts(&mut tx, job, subtitle_artifact_ids, 1).await?;
                (Some(*artifact_id), None)
            }
            ItemOperationCompletion::IntervalPlan { plan }
                if job.kind == ItemOperationKind::IntervalMetadata =>
            {
                let timing = job.input.requested_timing.as_ref().ok_or_else(conflict)?;
                if plan.context_fingerprint() != job.input.context_fingerprint
                    || plan.source_window_milliseconds()
                        != timing.source_window_milliseconds(plan.source_duration_milliseconds())?
                {
                    return Err(conflict());
                }
                (None, None)
            }
            ItemOperationCompletion::Subtitles { artifact_ids }
                if job.kind == ItemOperationKind::Subtitles && artifact_ids.len() <= 2 =>
            {
                validate_subtitle_artifacts(&mut tx, job, artifact_ids, 0).await?;
                (None, None)
            }
            ItemOperationCompletion::Failed(_) => (None, None),
            _ => return Err(conflict()),
        };
        let state = if matches!(completion, ItemOperationCompletion::Failed(_)) {
            3
        } else {
            2
        };
        let json = serde_json::to_string(&completion).map_err(|_| conflict())?;
        if json.len() > 65_536 {
            return Err(conflict());
        }
        let time = encode_datetime(now);
        let changed = sqlx::query(
            r#"
            UPDATE "AddonMediaListItemOperationJob"
                SET "State"=?,
                "OutcomeJson"=?,
                "ArtifactId"=?,
                "ExpiresAt"=?,
                "CompletedAt"=?,
                "UpdatedAt"=?,
                "LeaseOwner"=NULL,
                "LeaseExpiresAt"=NULL
                WHERE "Id"=?
                AND "Generation"=?
                AND "LeaseOwner"=?
                AND "State"=1
                AND "LeaseExpiresAt">?
        "#,
        )
        .bind(state)
        .bind(json)
        .bind(artifact)
        .bind(expires)
        .bind(&time)
        .bind(&time)
        .bind(job.id)
        .bind(job.generation)
        .bind(&job.lease_owner)
        .bind(&time)
        .execute(&mut *tx)
        .await
        .repository()?
        .rows_affected();
        if changed != 1 {
            return Err(conflict());
        }
        tx.commit().await.repository()?;
        Ok(())
    }

    /// Cancellation invalidates completion but the caller still owns process-tree cleanup.
    ///
    /// # Errors
    /// Returns an error if the job is missing or no longer active, or the database
    /// update fails.
    pub async fn cancel(&self, id: i64, now: DateTime<Utc>) -> Result<()> {
        let time = encode_datetime(now);
        let changed = sqlx::query(
            r#"
            UPDATE "AddonMediaListItemOperationJob"
                SET "State"=4,
                "UpdatedAt"=?,
                "CompletedAt"=?
                WHERE "Id"=?
                AND "State" IN (0,1)
        "#,
        )
        .bind(&time)
        .bind(&time)
        .bind(id)
        .execute(&self.pool)
        .await
        .repository()?
        .rows_affected();
        if changed != 1 {
            return Err(conflict());
        }
        Ok(())
    }

    /// Expired or changed access evidence is hidden without rewriting history.
    ///
    /// # Errors
    /// Returns an error if persisted state cannot be decoded or the database
    /// query fails.
    pub async fn status(&self, id: i64, now: DateTime<Utc>) -> Result<Option<ItemOperationStatus>> {
        // This is a read-only transaction: stale evidence is hidden, not rewritten.
        let mut tx = self.pool.begin().await.repository()?;
        let row = sqlx::query(
            r#"SELECT "State", "AttemptCount", "OutcomeJson", "InputJson",
                      "AddonMediaListId", "AddonMediaListItemId", "SavedDraftRevision",
                      "NextEligibleAt", "Kind"
               FROM "AddonMediaListItemOperationJob" WHERE "Id"=?"#,
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .repository()?;
        let Some(row) = row else {
            tx.commit().await.repository()?;
            return Ok(None);
        };
        let input: ItemOperationInput = serde_json::from_str(
            &row.try_get::<String, _>("InputJson").repository()?,
        )
        .map_err(|_| RepositoryError::Decode {
            kind: "item operation input",
            value: "invalid persisted input".into(),
        })?;
        let mut status = ItemOperationStatus {
            id,
            context_fingerprint: input.context_fingerprint.clone(),
            tool_version: input.tool_version.clone(),
            state: row.try_get("State").repository()?,
            attempt_count: row.try_get("AttemptCount").repository()?,
            next_eligible_at: crate::repositories::common::decode_optional_datetime(
                "item operation eligibility",
                row.try_get("NextEligibleAt").repository()?,
            )?,
            completion: row
                .try_get::<Option<String>, _>("OutcomeJson")
                .repository()?
                .map(|json| {
                    if json.len() > 65_536 {
                        return Err(RepositoryError::Decode {
                            kind: "item operation outcome",
                            value: "persisted outcome exceeds size limit".into(),
                        });
                    }
                    serde_json::from_str(&json).map_err(|_| RepositoryError::Decode {
                        kind: "item operation outcome",
                        value: "invalid persisted outcome".into(),
                    })
                })
                .transpose()?,
        };
        let evidence_expired = match &status.completion {
            Some(ItemOperationCompletion::Access { expires_at, .. }) => Some(*expires_at <= now),
            Some(ItemOperationCompletion::IntervalPlan { .. }) => Some(false),
            _ => None,
        };
        if let (2, Some(expired)) = (status.state, evidence_expired) {
            let current = current(
                &mut tx,
                row.try_get("AddonMediaListId").repository()?,
                row.try_get("AddonMediaListItemId").repository()?,
                row.try_get("SavedDraftRevision").repository()?,
                row.try_get("Kind").repository()?,
                &input,
            )
            .await;
            let stale = match current {
                Ok(()) => expired,
                Err(RepositoryError::Conflict(_)) => true,
                Err(error) => return Err(error),
            };
            if stale {
                status.state = 5;
                status.completion = None;
            }
        }
        tx.commit().await.repository()?;
        Ok(Some(status))
    }
}

/// Share the exact transaction fence with artifact admission and promotion.
pub(crate) async fn validate_claim(
    tx: &mut sqlx::Transaction<'_, Db>,
    job: &ClaimedItemOperation,
    now: DateTime<Utc>,
) -> Result<()> {
    current(
        tx,
        job.list_id,
        job.item_id,
        job.saved_draft_revision,
        job.kind.code(),
        &job.input,
    )
    .await?;
    owned_claim(tx, job, now).await
}

async fn owned_claim(
    tx: &mut sqlx::Transaction<'_, Db>,
    job: &ClaimedItemOperation,
    now: DateTime<Utc>,
) -> Result<()> {
    let valid: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM "AddonMediaListItemOperationJob"
           WHERE "Id" = ? AND "Generation" = ? AND "LeaseOwner" = ?
             AND "State" = 1 AND "LeaseExpiresAt" > ?
             AND "Kind"=? AND "InputJson"=? AND "AddonMediaListId"=?
             AND "AddonMediaListItemId"=? AND "SavedDraftRevision"=?)"#,
    )
    .bind(job.id)
    .bind(job.generation)
    .bind(&job.lease_owner)
    .bind(encode_datetime(now))
    .bind(job.kind.code())
    .bind(serde_json::to_string(&job.input).map_err(|_| conflict())?)
    .bind(job.list_id)
    .bind(job.item_id)
    .bind(job.saved_draft_revision)
    .fetch_one(&mut **tx)
    .await
    .repository()?;
    if !valid {
        return Err(conflict());
    }
    Ok(())
}

#[allow(clippy::too_many_lines)] // Verify saved identity, installation, liveness and exact timing together.
async fn validate_subtitle_artifacts(
    tx: &mut sqlx::Transaction<'_, Db>,
    job: &ClaimedItemOperation,
    artifact_ids: &[i64],
    timeline: i32,
) -> Result<()> {
    if artifact_ids.len() > 2 || (artifact_ids.len() == 2 && artifact_ids[0] == artifact_ids[1]) {
        return Err(conflict());
    }
    let mut languages = std::collections::BTreeSet::new();
    for artifact_id in artifact_ids {
        let language: Option<String> = sqlx::query_scalar(
            r#"SELECT "SubtitleLanguage" FROM "AddonMediaListArtifact" WHERE "Id"=?
            AND "AddonMediaListId"=? AND "AddonMediaListItemId"=?
            AND "OperationContextFingerprint"=? AND "Kind"=0 AND "State"=2
            AND "SubtitleTimeline"=? AND "SubtitleLanguage" IS NOT NULL
            AND "CompletedAt" IS NOT NULL"#,
        )
        .bind(artifact_id)
        .bind(job.list_id)
        .bind(job.item_id)
        .bind(&job.input.context_fingerprint)
        .bind(timeline)
        .fetch_optional(&mut **tx)
        .await
        .repository()?;
        let language = language.ok_or_else(conflict)?;
        let primary = ersatzrs_addon_contract::subtitle_logical_language(&language)
            .map_err(|_| conflict())?;
        if !languages.insert(primary.clone())
            || !job
                .input
                .requested_subtitle_languages
                .iter()
                .any(|requested| {
                    ersatzrs_addon_contract::subtitle_logical_language(requested)
                        .is_ok_and(|requested| requested == primary)
                })
        {
            return Err(conflict());
        }
    }
    Ok(())
}

/// Cancel active draft work while retaining process ownership for worker cleanup.
pub(crate) async fn cancel_item_jobs_in_tx(
    tx: &mut sqlx::Transaction<'_, Db>,
    list_id: i64,
    item_id: i64,
    now: DateTime<Utc>,
) -> Result<()> {
    let time = encode_datetime(now);
    sqlx::query(
        r#"UPDATE "AddonMediaListItemOperationJob"
           SET "State"=4, "UpdatedAt"=?, "CompletedAt"=?
           WHERE "AddonMediaListId"=? AND "AddonMediaListItemId"=?
             AND "State" IN (0,1)"#,
    )
    .bind(&time)
    .bind(&time)
    .bind(list_id)
    .bind(item_id)
    .execute(&mut **tx)
    .await
    .repository()?;
    Ok(())
}

#[allow(clippy::too_many_lines)] // Validate the complete saved job identity in one transaction.
async fn current(
    tx: &mut sqlx::Transaction<'_, Db>,
    list_id: i64,
    item_id: i64,
    revision: i32,
    operation_kind: i32,
    input: &ItemOperationInput,
) -> Result<()> {
    if input.preparation_v2 == Some(true)
        && crate::addon_media_list_subtitle_selection::requested_preparation_languages_in_tx(
            tx, list_id,
        )
        .await?
            != input.requested_subtitle_languages
    {
        return Err(conflict());
    }
    if !input.requested_subtitle_languages.is_empty()
        && crate::addon_media_list_subtitles::requested_languages_in_tx(tx).await?
            != input.requested_subtitle_languages
    {
        return Err(conflict());
    }
    // Access (kind 0) is deliberate inspection; all media preparation must
    // wait for a receiving replacement choice, including previously queued work.
    let valid: bool = sqlx::query_scalar(r#"
            SELECT EXISTS(SELECT 1
                FROM "AddonMediaList" AS "list"
                JOIN "AddonMediaListItem" AS "item" ON "item"."AddonMediaListId"="list"."Id"
                LEFT JOIN "AddonMediaListOptionSchema" AS "schema" ON "schema"."Id"="list"."DraftOptionSchemaId"
                WHERE "list"."Id"=?
                AND "item"."Id"=?
                AND "item"."InDraft"=1
                AND (? = 0 OR "item"."SavedReplacementJson" IS NULL)
                AND "list"."DraftRevision"=?
                AND "item"."ProviderId"=?
                AND "item"."SourceUrl"=?
                AND "list"."AddonId"=?
                AND "schema"."SchemaDigest" IS ?
                AND "schema"."CapabilityId" IS ?)
        "#)
        .bind(list_id)
        .bind(item_id)
        .bind(operation_kind)
        .bind(revision)
        .bind(&input.provider_id)
        .bind(&input.source_url)
        .bind(input.addon_id()?)
        .bind(input.schema.as_ref().map(|schema| &schema.schema_digest))
        .bind(input.schema.as_ref().map(|schema| &schema.capability_id))
        .fetch_one(&mut **tx)
        .await
        .repository()?;
    if !valid {
        return Err(conflict());
    }
    let installation = sqlx::query(
        r#"
        SELECT "Version", "PackageSha256", "ContentSha256", "StateRevision"
        FROM "AddonInstallation" WHERE "AddonId"=? AND "Enabled"=1
    "#,
    )
    .bind(input.addon_id()?)
    .fetch_optional(&mut **tx)
    .await
    .repository()?
    .ok_or_else(conflict)?;
    let identity = crate::addon_media_list_item_runtime::package_identity_for_parts(
        input.addon_id()?,
        &installation.try_get::<String, _>("Version").repository()?,
        &installation
            .try_get::<String, _>("PackageSha256")
            .repository()?,
        &installation
            .try_get::<String, _>("ContentSha256")
            .repository()?,
        &installation
            .try_get::<String, _>("StateRevision")
            .repository()?,
    );
    if identity != input.package_identity {
        return Err(conflict());
    }
    let row = sqlx::query(
        r#"
            SELECT COALESCE("LivenessOverride", "ProviderLiveness") AS "Liveness",
                "DurationSeconds", "FragmentStartSeconds", "FragmentEndSeconds"
                FROM "AddonMediaListItem"
                WHERE "Id"=?
        "#,
    )
    .bind(item_id)
    .fetch_one(&mut **tx)
    .await
    .repository()?;
    let liveness: RemoteStreamLiveness =
        crate::repositories::common::decode_enum_column("RemoteStreamLiveness", &row, "Liveness")?;
    if (liveness == RemoteStreamLiveness::Finite) != input.item_is_finite {
        return Err(conflict());
    }
    if let Some(timing) = &input.requested_timing {
        for (column, expected) in [
            ("DurationSeconds", timing.item_duration_seconds),
            ("FragmentStartSeconds", timing.fragment_start_seconds),
            ("FragmentEndSeconds", timing.fragment_end_seconds),
        ] {
            let actual: Option<i64> = row.try_get(column).repository()?;
            let expected = expected
                .map(i64::try_from)
                .transpose()
                .map_err(|_| conflict())?;
            if actual != expected {
                return Err(conflict());
            }
        }
    }
    Ok(())
}
fn conflict() -> RepositoryError {
    RepositoryError::Conflict("the item operation context or lease changed".to_owned())
}
fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn lease_deadline(now: DateTime<Utc>, seconds: u32) -> Result<String> {
    if seconds == 0 || seconds > 3600 {
        return Err(conflict());
    }
    Ok(encode_datetime(now + Duration::seconds(i64::from(seconds))))
}

#[cfg(test)]
mod subtitle_input_tests {
    use super::*;

    #[test]
    fn legacy_prepared_completion_preserves_json_and_empty_subtitle_binding() {
        let encoded = r#"{"Prepared":{"artifact_id":42}}"#;
        let completion: ItemOperationCompletion = serde_json::from_str(encoded).unwrap();
        assert_eq!(
            completion,
            ItemOperationCompletion::Prepared {
                artifact_id: 42,
                subtitle_artifact_ids: Vec::new(),
            }
        );
        assert_eq!(serde_json::to_string(&completion).unwrap(), encoded);
    }

    fn legacy_input() -> ItemOperationInput {
        serde_json::from_value(serde_json::json!({
            "source_url":"https://example.test/item", "provider_id":"item",
            "context_fingerprint":"a".repeat(64),
            "addon_id":"org.ersatzrs.addon.fixture", "preparation_v2":true,
            "package_identity":"b".repeat(64), "tool_version":"1.0",
            "item_is_finite":true, "max_video_height":720
        }))
        .unwrap()
    }

    #[test]
    fn empty_subtitle_languages_preserve_legacy_serialization_and_fingerprint() {
        let input = legacy_input();
        assert!(input.requested_subtitle_languages.is_empty());
        assert_eq!(
            input.base_fingerprint().unwrap(),
            input.options_fingerprint().unwrap()
        );
        let encoded = serde_json::to_value(&input).unwrap();
        assert!(encoded.get("requested_subtitle_languages").is_none());
        let decoded: ItemOperationInput = serde_json::from_value(encoded).unwrap();
        assert_eq!(decoded, input);
        assert!(validate_mode(ItemOperationKind::Preparation, &input).is_ok());
    }

    #[test]
    fn subtitle_requests_bind_languages_without_pretending_to_be_preparation() {
        let mut input = legacy_input();
        input.preparation_v2 = None;
        input.requested_subtitle_languages = vec!["en".into(), "nl".into()];
        assert!(validate_mode(ItemOperationKind::Subtitles, &input).is_ok());
        assert!(input.resolved_options().unwrap().is_none());
        assert_eq!(input.addon_id().unwrap(), "org.ersatzrs.addon.fixture");
        let fingerprint = input.base_fingerprint().unwrap();
        input.requested_subtitle_languages.reverse();
        assert_ne!(input.base_fingerprint().unwrap(), fingerprint);
        assert!(validate_mode(ItemOperationKind::Access, &input).is_err());
        input.item_is_finite = false;
        assert!(validate_mode(ItemOperationKind::Subtitles, &input).is_err());
        input.item_is_finite = true;
        input.requested_subtitle_languages = vec!["en".into(), "en".into()];
        assert!(validate_mode(ItemOperationKind::Subtitles, &input).is_err());
        input.requested_subtitle_languages = vec!["en".into(), "nl".into(), "de".into()];
        assert!(validate_mode(ItemOperationKind::Subtitles, &input).is_err());
        input.requested_subtitle_languages.clear();
        assert!(validate_mode(ItemOperationKind::Subtitles, &input).is_err());
    }
}
