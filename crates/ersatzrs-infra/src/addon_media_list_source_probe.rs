//! Durable, non-mutating header and first-page probes for add-on media lists.

use std::time::Instant;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use ersatzrs_addon_contract::{
    sanitize_addon_diagnostic_code, validate_operator_source_url, AddonMediaListRecord,
    AddonOperationError, ADDON_OPERATION_ERROR_MARKER, CAPABILITY_MEDIA_LIST_IMPORT_V1,
};
use ersatzrs_core::repositories::{
    AddonMediaListSourceProbe, AddonMediaListSourceProbeOutcome,
    AddonMediaListSourceProbeRepository, AddonMediaListSourceProbeState, RepositoryError, Result,
};
use ersatzrs_domain::EntityId;
use sha2::{Digest as _, Sha256};
use sqlx::Row as _;
use uuid::Uuid;

use crate::addon_media_list::{InstalledAddonMediaListProvider, InstalledMediaListImportContext};
use crate::addons::load_installed_addon;
use crate::db::{inserted_id, DbPool, DbRow};
use crate::remote_stream::sanitize_remote_stream_diagnostic;
use crate::repositories::common::{
    decode_bool_column, decode_datetime, decode_optional_datetime, encode_datetime,
};
use crate::repositories::SqlxResultExt as _;

const QUEUED_STATE: i32 = 0;
const RUNNING_STATE: i32 = 1;
const COMPLETE_STATE: i32 = 2;
const AVAILABLE_OUTCOME: i32 = 0;
const UNSUPPORTED_OUTCOME: i32 = 1;
const FAILED_OUTCOME: i32 = 2;
const LEASE_SECONDS: i64 = 90;
const MAX_ATTEMPTS: i32 = 3;
const MAX_PROBE_TITLE_BYTES: usize = 512;

/// One adapter serves the application command/query port and the host worker.
#[derive(Clone)]
pub struct AddonMediaListSourceProbeCoordinator {
    pool: DbPool,
    provider: InstalledAddonMediaListProvider,
}

impl AddonMediaListSourceProbeCoordinator {
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self {
            provider: InstalledAddonMediaListProvider::new(pool.clone()),
            pool,
        }
    }

    async fn load_input(&self, list_id: EntityId) -> Result<SourceProbeInput> {
        let row = sqlx::query(
            r#"SELECT "list"."AddonId", "list"."SourceUrl", "list"."DraftRevision",
                      "list"."DraftOptionValuesJson", "schema"."SchemaDigest"
               FROM "AddonMediaList" AS "list"
               LEFT JOIN "AddonMediaListOptionSchema" AS "schema"
                 ON "schema"."Id" = "list"."DraftOptionSchemaId"
               WHERE "list"."Id" = ?"#,
        )
        .bind(list_id)
        .fetch_optional(&self.pool)
        .await
        .repository()?
        .ok_or(RepositoryError::NotFound {
            kind: "add-on media list",
            id: list_id,
        })?;
        let addon_id: String = row.try_get("AddonId").repository()?;
        let source_url: String = row.try_get("SourceUrl").repository()?;
        let source_revision: i32 = row.try_get("DraftRevision").repository()?;
        let option_values_json: String = row.try_get("DraftOptionValuesJson").repository()?;
        let option_schema_digest: Option<String> = row.try_get("SchemaDigest").repository()?;
        let installation = load_installed_addon(&self.pool, &addon_id)
            .await
            .map_err(|error| {
                RepositoryError::unavailable("load source-probe add-on", error.to_string())
            })?
            .ok_or_else(|| {
                RepositoryError::unavailable(
                    "load source-probe add-on",
                    "the selected add-on is not installed",
                )
            })?;

        let supports_import = installation
            .manifest
            .has_capability(CAPABILITY_MEDIA_LIST_IMPORT_V1);
        // Unsupported packages still receive a truthful durable result. Use
        // the same URL canonicalizer without imposing a capability-specific
        // host declaration on a process that will never be launched.
        let canonical_source_url = validate_operator_source_url(
            &source_url,
            if supports_import {
                &installation.manifest.permissions.network_hosts
            } else {
                &[]
            },
        )
        .map_err(|error| {
            RepositoryError::unavailable(
                "validate source-probe URL",
                AddonOperationError::new("unsupported-url", error.to_string()).to_string(),
            )
        })?;
        let import_context = if supports_import {
            self.provider.import_context(&addon_id, &source_url).await?
        } else {
            None
        };
        let record_capability = import_context
            .as_ref()
            .map(|context| context.record_capability.clone());
        let mut capabilities = installation
            .manifest
            .capabilities
            .iter()
            .map(|capability| capability.id.clone())
            .collect::<Vec<_>>();
        capabilities.sort_unstable();
        capabilities.dedup();

        let setting_rows = sqlx::query(
            r#"SELECT "Key", "Value", "ReferenceKind", "Reference"
               FROM "AddonSetting" WHERE "AddonId" = ? ORDER BY "Key""#,
        )
        .bind(&addon_id)
        .fetch_all(&self.pool)
        .await
        .repository()?;
        let mut fingerprint = Sha256::new();
        hash_field(&mut fingerprint, addon_id.as_bytes());
        hash_field(&mut fingerprint, canonical_source_url.as_bytes());
        hash_field(
            &mut fingerprint,
            installation.version.to_string().as_bytes(),
        );
        hash_field(&mut fingerprint, installation.package_sha256.as_bytes());
        hash_field(&mut fingerprint, installation.content_sha256.as_bytes());
        hash_field(&mut fingerprint, installation.state_revision.as_bytes());
        hash_field(&mut fingerprint, &source_revision.to_be_bytes());
        hash_optional_field(&mut fingerprint, record_capability.as_deref());
        hash_optional_field(&mut fingerprint, option_schema_digest.as_deref());
        hash_field(&mut fingerprint, option_values_json.as_bytes());
        for setting in setting_rows {
            hash_field(
                &mut fingerprint,
                setting.try_get::<String, _>("Key").repository()?.as_bytes(),
            );
            hash_optional_field(
                &mut fingerprint,
                setting
                    .try_get::<Option<String>, _>("Value")
                    .repository()?
                    .as_deref(),
            );
            hash_optional_field(
                &mut fingerprint,
                setting
                    .try_get::<Option<String>, _>("ReferenceKind")
                    .repository()?
                    .as_deref(),
            );
            hash_optional_field(
                &mut fingerprint,
                setting
                    .try_get::<Option<String>, _>("Reference")
                    .repository()?
                    .as_deref(),
            );
        }

        Ok(SourceProbeInput {
            fingerprint: hex_digest(fingerprint.finalize()),
            addon_id,
            source_revision,
            record_capability,
            capabilities,
            import_context,
        })
    }

    async fn enqueue(&self, list_id: EntityId, expected_draft_revision: i32) -> Result<EntityId> {
        let current_revision: Option<i32> =
            sqlx::query_scalar(r#"SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id" = ?"#)
                .bind(list_id)
                .fetch_optional(&self.pool)
                .await
                .repository()?;
        if current_revision != Some(expected_draft_revision) {
            return Err(RepositoryError::Conflict(
                "the saved list changed before its source probe was queued".to_owned(),
            ));
        }
        let input = self.load_input(list_id).await?;
        let capabilities_json = serde_json::to_string(&input.capabilities).map_err(|error| {
            RepositoryError::Decode {
                kind: "source-probe capabilities",
                value: error.to_string(),
            }
        })?;
        let now = encode_datetime(Utc::now());
        let mut transaction = crate::db::begin_write(&self.pool).await.repository()?;
        let current_revision: Option<i32> =
            sqlx::query_scalar(r#"SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id" = ?"#)
                .bind(list_id)
                .fetch_optional(&mut *transaction)
                .await
                .repository()?;
        if current_revision != Some(expected_draft_revision) {
            return Err(RepositoryError::Conflict(
                "the saved list changed before its source probe was queued".to_owned(),
            ));
        }
        if let Some(row) = sqlx::query(
            r#"SELECT "Id", "InputFingerprint"
               FROM "AddonMediaListSourceProbeJob"
               WHERE "AddonMediaListId" = ? AND "State" IN (?, ?)
               ORDER BY "Id" DESC LIMIT 1"#,
        )
        .bind(list_id)
        .bind(QUEUED_STATE)
        .bind(RUNNING_STATE)
        .fetch_optional(&mut *transaction)
        .await
        .repository()?
        {
            let existing_id: EntityId = row.try_get("Id").repository()?;
            let existing_fingerprint: String = row.try_get("InputFingerprint").repository()?;
            if existing_fingerprint == input.fingerprint {
                transaction.commit().await.repository()?;
                return Ok(existing_id);
            }
            return Err(RepositoryError::Conflict(
                "another source probe is already queued for this list".to_owned(),
            ));
        }
        let result = sqlx::query(
            r#"INSERT INTO "AddonMediaListSourceProbeJob" (
                   "AddonMediaListId", "InputFingerprint", "SavedDraftRevision",
                   "RecordCapability", "CapabilitiesJson", "State", "CreatedAt", "UpdatedAt"
               ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(list_id)
        .bind(&input.fingerprint)
        .bind(input.source_revision)
        .bind(&input.record_capability)
        .bind(capabilities_json)
        .bind(QUEUED_STATE)
        .bind(&now)
        .bind(&now)
        .execute(&mut *transaction)
        .await
        .repository()?;
        let id = inserted_id(&mut transaction, result).await.repository()?;
        transaction.commit().await.repository()?;
        Ok(id)
    }

    async fn status(&self, list_id: EntityId) -> Result<Option<AddonMediaListSourceProbe>> {
        let Some(row) = sqlx::query(
            r#"SELECT "Id", "AddonMediaListId", "InputFingerprint", "SavedDraftRevision",
                      "CapabilitiesJson", "State", "AttemptCount", "AccessOutcome",
                      "ProviderId", "ListTitle", "CountHint", "ReturnedItemCount",
                      "PageComplete", "RevisionToken", "DurationMilliseconds",
                      "DiagnosticCode", "Diagnostic",
                      "CreatedAt", "UpdatedAt", "CompletedAt"
               FROM "AddonMediaListSourceProbeJob"
               WHERE "AddonMediaListId" = ? ORDER BY "Id" DESC LIMIT 1"#,
        )
        .bind(list_id)
        .fetch_optional(&self.pool)
        .await
        .repository()?
        else {
            return Ok(None);
        };
        let saved_fingerprint: String = row.try_get("InputFingerprint").repository()?;
        // The saved probe row is already operator-safe. If the current package
        // or input can no longer be reconstructed, it is stale by definition;
        // do not replace that durable result with a transient load error.
        let is_current = self
            .load_input(list_id)
            .await
            .is_ok_and(|input| input.fingerprint == saved_fingerprint);
        decode_status(&row, is_current).map(Some)
    }

    async fn process_one(&self, now: DateTime<Utc>) -> Result<bool> {
        self.finish_exhausted(now).await?;
        let Some(job) = self.claim(now).await? else {
            return Ok(false);
        };
        let input = match self.load_input(job.list_id).await {
            Ok(input) if input.fingerprint == job.input_fingerprint => input,
            Ok(_) => {
                self.finish_terminal(
                    &job,
                    now,
                    TerminalProbe::failed(
                        "source-input-changed",
                        "The saved source inputs changed before this probe ran.",
                    ),
                )
                .await?;
                return Ok(true);
            }
            Err(error) => {
                let (code, diagnostic) = redacted_probe_diagnostic(&error);
                self.finish_terminal(&job, now, TerminalProbe::failed(code, diagnostic))
                    .await?;
                return Ok(true);
            }
        };
        let Some(context) = input.import_context.as_ref() else {
            self.finish_terminal(
                &job,
                now,
                TerminalProbe::unsupported(
                    "source-probe-unsupported",
                    "The selected add-on does not support bounded source testing.",
                ),
            )
            .await?;
            return Ok(true);
        };
        if Some(context.record_capability.as_str()) != job.record_capability.as_deref() {
            self.finish_terminal(
                &job,
                now,
                TerminalProbe::failed(
                    "source-input-changed",
                    "The saved source inputs changed before this probe ran.",
                ),
            )
            .await?;
            return Ok(true);
        }

        let started = Instant::now();
        let result = self.provider.discover(&input.addon_id, context, None).await;
        let duration = duration_milliseconds(started.elapsed());
        let completed_at =
            now + Duration::milliseconds(i64::try_from(duration).unwrap_or(i64::MAX));
        match result {
            Ok(page) => {
                let header = page
                    .records
                    .iter()
                    .find_map(|record| {
                        if let AddonMediaListRecord::List {
                            provider_id, name, ..
                        } = record
                        {
                            Some((provider_id.clone(), name.clone()))
                        } else {
                            None
                        }
                    })
                    .ok_or_else(|| RepositoryError::Decode {
                        kind: "source-probe page",
                        value: "the first page omitted its list header".to_owned(),
                    });
                match header {
                    Ok((provider_id, list_title)) => {
                        let item_count = page
                            .records
                            .iter()
                            .filter(|record| matches!(record, AddonMediaListRecord::Item { .. }))
                            .count();
                        self.finish_terminal(
                            &job,
                            completed_at,
                            TerminalProbe::available(
                                provider_id,
                                bound_utf8(&list_title, MAX_PROBE_TITLE_BYTES),
                                page.total_hint,
                                u32::try_from(item_count).unwrap_or(u32::MAX),
                                page.complete,
                                page.revision_token,
                                duration,
                            ),
                        )
                        .await?;
                    }
                    Err(error) => {
                        let (code, diagnostic) = redacted_probe_diagnostic(&error);
                        self.finish_terminal(
                            &job,
                            completed_at,
                            TerminalProbe::failed(code, diagnostic).with_duration(duration),
                        )
                        .await?;
                    }
                }
            }
            Err(error) => {
                let (code, diagnostic) = redacted_probe_diagnostic(&error);
                self.finish_terminal(
                    &job,
                    completed_at,
                    TerminalProbe::failed(code, diagnostic).with_duration(duration),
                )
                .await?;
            }
        }
        Ok(true)
    }

    async fn finish_exhausted(&self, now: DateTime<Utc>) -> Result<()> {
        let now = encode_datetime(now);
        sqlx::query(
            r#"UPDATE "AddonMediaListSourceProbeJob"
               SET "State" = ?, "AccessOutcome" = ?, "LeaseOwner" = NULL,
                   "LeaseExpiresAt" = NULL, "DiagnosticCode" = 'source-probe-attempts-exhausted',
                   "Diagnostic" = 'The source probe stopped repeatedly before saving a result.',
                   "UpdatedAt" = ?, "CompletedAt" = ?
               WHERE "State" = ? AND "AttemptCount" >= ?
                 AND "LeaseExpiresAt" IS NOT NULL AND "LeaseExpiresAt" <= ?"#,
        )
        .bind(COMPLETE_STATE)
        .bind(FAILED_OUTCOME)
        .bind(&now)
        .bind(&now)
        .bind(RUNNING_STATE)
        .bind(MAX_ATTEMPTS)
        .bind(&now)
        .execute(&self.pool)
        .await
        .repository()?;
        Ok(())
    }

    async fn claim(&self, now: DateTime<Utc>) -> Result<Option<ClaimedProbe>> {
        let now_text = encode_datetime(now);
        let lease_expires = encode_datetime(now + Duration::seconds(LEASE_SECONDS));
        let mut transaction = crate::db::begin_write(&self.pool).await.repository()?;
        let row = sqlx::query(
            r#"SELECT "Id", "AddonMediaListId", "InputFingerprint", "RecordCapability"
               FROM "AddonMediaListSourceProbeJob"
               WHERE "AttemptCount" < ? AND (
                   ("State" = ? AND ("NextEligibleAt" IS NULL OR "NextEligibleAt" <= ?))
                   OR ("State" = ? AND "LeaseExpiresAt" IS NOT NULL AND "LeaseExpiresAt" <= ?)
               )
               ORDER BY "Id" LIMIT 1"#,
        )
        .bind(MAX_ATTEMPTS)
        .bind(QUEUED_STATE)
        .bind(&now_text)
        .bind(RUNNING_STATE)
        .bind(&now_text)
        .fetch_optional(&mut *transaction)
        .await
        .repository()?;
        let Some(row) = row else {
            transaction.commit().await.repository()?;
            return Ok(None);
        };
        let id: EntityId = row.try_get("Id").repository()?;
        let lease_owner = Uuid::new_v4().to_string();
        let updated = sqlx::query(
            r#"UPDATE "AddonMediaListSourceProbeJob"
               SET "State" = ?, "AttemptCount" = "AttemptCount" + 1,
                   "LeaseOwner" = ?, "LeaseExpiresAt" = ?, "HeartbeatAt" = ?,
                   "StartedAt" = COALESCE("StartedAt", ?), "UpdatedAt" = ?
               WHERE "Id" = ? AND "AttemptCount" < ? AND (
                   ("State" = ? AND ("NextEligibleAt" IS NULL OR "NextEligibleAt" <= ?))
                   OR ("State" = ? AND "LeaseExpiresAt" IS NOT NULL AND "LeaseExpiresAt" <= ?)
               )"#,
        )
        .bind(RUNNING_STATE)
        .bind(&lease_owner)
        .bind(&lease_expires)
        .bind(&now_text)
        .bind(&now_text)
        .bind(&now_text)
        .bind(id)
        .bind(MAX_ATTEMPTS)
        .bind(QUEUED_STATE)
        .bind(&now_text)
        .bind(RUNNING_STATE)
        .bind(&now_text)
        .execute(&mut *transaction)
        .await
        .repository()?;
        if updated.rows_affected() != 1 {
            return Err(RepositoryError::Conflict(
                "the source-probe claim changed before it was saved".to_owned(),
            ));
        }
        let claimed = ClaimedProbe {
            id,
            list_id: row.try_get("AddonMediaListId").repository()?,
            input_fingerprint: row.try_get("InputFingerprint").repository()?,
            record_capability: row.try_get("RecordCapability").repository()?,
            lease_owner,
        };
        transaction.commit().await.repository()?;
        Ok(Some(claimed))
    }

    async fn finish_terminal(
        &self,
        job: &ClaimedProbe,
        completed_at: DateTime<Utc>,
        result: TerminalProbe,
    ) -> Result<()> {
        let completed_at = encode_datetime(completed_at);
        let updated = sqlx::query(
            r#"UPDATE "AddonMediaListSourceProbeJob"
               SET "State" = ?, "AccessOutcome" = ?, "ProviderId" = ?, "ListTitle" = ?,
                   "CountHint" = ?, "ReturnedItemCount" = ?, "PageComplete" = ?,
                   "RevisionToken" = ?,
                   "DurationMilliseconds" = ?, "DiagnosticCode" = ?, "Diagnostic" = ?,
                   "LeaseOwner" = NULL, "LeaseExpiresAt" = NULL,
                   "UpdatedAt" = ?, "CompletedAt" = ?
               WHERE "Id" = ? AND "State" = ? AND "LeaseOwner" = ?
                 AND "LeaseExpiresAt" IS NOT NULL AND "LeaseExpiresAt" > ?"#,
        )
        .bind(COMPLETE_STATE)
        .bind(result.outcome)
        .bind(result.provider_id)
        .bind(result.list_title)
        .bind(result.count_hint.map(i64::from))
        .bind(result.returned_item_count.map(i64::from))
        .bind(result.page_complete)
        .bind(result.revision_token)
        .bind(
            result
                .duration_milliseconds
                .map(|value| i64::try_from(value).unwrap_or(i64::MAX)),
        )
        .bind(result.diagnostic_code)
        .bind(result.diagnostic)
        .bind(&completed_at)
        .bind(&completed_at)
        .bind(job.id)
        .bind(RUNNING_STATE)
        .bind(&job.lease_owner)
        .bind(&completed_at)
        .execute(&self.pool)
        .await
        .repository()?;
        if updated.rows_affected() != 1 {
            return Err(RepositoryError::Conflict(
                "the source-probe lease expired before its result was saved".to_owned(),
            ));
        }
        Ok(())
    }
}

#[async_trait]
impl AddonMediaListSourceProbeRepository for AddonMediaListSourceProbeCoordinator {
    async fn enqueue_source_probe(
        &self,
        list_id: EntityId,
        expected_draft_revision: i32,
    ) -> Result<EntityId> {
        self.enqueue(list_id, expected_draft_revision).await
    }

    async fn source_probe_status(
        &self,
        list_id: EntityId,
    ) -> Result<Option<AddonMediaListSourceProbe>> {
        self.status(list_id).await
    }

    async fn process_one_source_probe(&self, now: DateTime<Utc>) -> Result<bool> {
        self.process_one(now).await
    }
}

struct SourceProbeInput {
    fingerprint: String,
    addon_id: String,
    source_revision: i32,
    record_capability: Option<String>,
    capabilities: Vec<String>,
    import_context: Option<InstalledMediaListImportContext>,
}

struct ClaimedProbe {
    id: EntityId,
    list_id: EntityId,
    input_fingerprint: String,
    record_capability: Option<String>,
    lease_owner: String,
}

struct TerminalProbe {
    outcome: i32,
    provider_id: Option<String>,
    list_title: Option<String>,
    count_hint: Option<u32>,
    returned_item_count: Option<u32>,
    page_complete: Option<bool>,
    revision_token: Option<String>,
    duration_milliseconds: Option<u64>,
    diagnostic_code: Option<String>,
    diagnostic: Option<String>,
}

impl TerminalProbe {
    fn available(
        provider_id: String,
        list_title: String,
        count_hint: Option<u32>,
        returned_item_count: u32,
        page_complete: bool,
        revision_token: Option<String>,
        duration_milliseconds: u64,
    ) -> Self {
        Self {
            outcome: AVAILABLE_OUTCOME,
            provider_id: Some(provider_id),
            list_title: Some(list_title),
            count_hint,
            returned_item_count: Some(returned_item_count),
            page_complete: Some(page_complete),
            revision_token,
            duration_milliseconds: Some(duration_milliseconds),
            diagnostic_code: None,
            diagnostic: None,
        }
    }

    fn unsupported(code: impl Into<String>, diagnostic: impl Into<String>) -> Self {
        Self {
            outcome: UNSUPPORTED_OUTCOME,
            provider_id: None,
            list_title: None,
            count_hint: None,
            returned_item_count: None,
            page_complete: None,
            revision_token: None,
            duration_milliseconds: Some(0),
            diagnostic_code: Some(code.into()),
            diagnostic: Some(diagnostic.into()),
        }
    }

    fn failed(code: impl Into<String>, diagnostic: impl Into<String>) -> Self {
        Self {
            outcome: FAILED_OUTCOME,
            provider_id: None,
            list_title: None,
            count_hint: None,
            returned_item_count: None,
            page_complete: None,
            revision_token: None,
            duration_milliseconds: None,
            diagnostic_code: Some(sanitize_addon_diagnostic_code(&code.into())),
            diagnostic: Some(sanitize_remote_stream_diagnostic(&diagnostic.into())),
        }
    }

    fn with_duration(mut self, duration_milliseconds: u64) -> Self {
        self.duration_milliseconds = Some(duration_milliseconds);
        self
    }
}

fn decode_status(row: &DbRow, is_current: bool) -> Result<AddonMediaListSourceProbe> {
    let stored_state: i32 = row.try_get("State").repository()?;
    let state = if !is_current {
        AddonMediaListSourceProbeState::Stale
    } else {
        match stored_state {
            QUEUED_STATE => AddonMediaListSourceProbeState::Queued,
            RUNNING_STATE => AddonMediaListSourceProbeState::Running,
            COMPLETE_STATE => AddonMediaListSourceProbeState::Complete,
            value => {
                return Err(RepositoryError::Decode {
                    kind: "source-probe state",
                    value: value.to_string(),
                });
            }
        }
    };
    let outcome = match row
        .try_get::<Option<i32>, _>("AccessOutcome")
        .repository()?
    {
        None => None,
        Some(AVAILABLE_OUTCOME) => Some(AddonMediaListSourceProbeOutcome::Available),
        Some(UNSUPPORTED_OUTCOME) => Some(AddonMediaListSourceProbeOutcome::Unsupported),
        Some(FAILED_OUTCOME) => Some(AddonMediaListSourceProbeOutcome::Failed),
        Some(value) => {
            return Err(RepositoryError::Decode {
                kind: "source-probe outcome",
                value: value.to_string(),
            });
        }
    };
    let capabilities_json: String = row.try_get("CapabilitiesJson").repository()?;
    let capabilities =
        serde_json::from_str::<Vec<String>>(&capabilities_json).map_err(|error| {
            RepositoryError::Decode {
                kind: "source-probe capabilities",
                value: error.to_string(),
            }
        })?;
    Ok(AddonMediaListSourceProbe {
        id: row.try_get("Id").repository()?,
        list_id: row.try_get("AddonMediaListId").repository()?,
        state,
        saved_draft_revision: row.try_get("SavedDraftRevision").repository()?,
        source_revision: row.try_get("RevisionToken").repository()?,
        capabilities,
        outcome,
        provider_id: row.try_get("ProviderId").repository()?,
        list_title: row.try_get("ListTitle").repository()?,
        count_hint: optional_u32(row, "CountHint")?,
        returned_item_count: optional_u32(row, "ReturnedItemCount")?,
        page_complete: match row.try_get::<Option<i64>, _>("PageComplete").repository()? {
            Some(_) => Some(decode_bool_column(row, "PageComplete")?),
            None => None,
        },
        duration_milliseconds: optional_u64(row, "DurationMilliseconds")?,
        diagnostic_code: row.try_get("DiagnosticCode").repository()?,
        diagnostic: row.try_get("Diagnostic").repository()?,
        attempt_count: required_u32(row, "AttemptCount")?,
        created_at: decode_datetime(
            "source-probe created timestamp",
            &row.try_get::<String, _>("CreatedAt").repository()?,
        )?,
        updated_at: decode_datetime(
            "source-probe updated timestamp",
            &row.try_get::<String, _>("UpdatedAt").repository()?,
        )?,
        completed_at: decode_optional_datetime(
            "source-probe completion timestamp",
            row.try_get("CompletedAt").repository()?,
        )?,
    })
}

fn redacted_probe_diagnostic(error: &RepositoryError) -> (String, String) {
    let rendered = error.to_string();
    if let Some(json) = rendered
        .split_once(ADDON_OPERATION_ERROR_MARKER)
        .map(|(_, json)| json)
    {
        if let Ok(diagnostic) = serde_json::from_str::<AddonOperationError>(json) {
            if diagnostic.validate().is_ok() {
                return (
                    sanitize_addon_diagnostic_code(&diagnostic.code),
                    sanitize_remote_stream_diagnostic(&diagnostic.message),
                );
            }
        }
    }
    (
        "source-probe-failed".to_owned(),
        sanitize_remote_stream_diagnostic(&rendered),
    )
}

fn hash_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
    hasher.update(value);
}

fn hash_optional_field(hasher: &mut Sha256, value: Option<&str>) {
    match value {
        Some(value) => {
            hasher.update([1]);
            hash_field(hasher, value.as_bytes());
        }
        None => hasher.update([0]),
    }
}

fn hex_digest(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn duration_milliseconds(duration: std::time::Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn bound_utf8(value: &str, maximum_bytes: usize) -> String {
    if value.len() <= maximum_bytes {
        return value.to_owned();
    }
    let mut end = maximum_bytes;
    while !value.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    value[..end].to_owned()
}

fn required_u32(row: &DbRow, column: &'static str) -> Result<u32> {
    let value: i64 = row.try_get(column).repository()?;
    u32::try_from(value).map_err(|_| RepositoryError::Decode {
        kind: "source-probe count",
        value: value.to_string(),
    })
}

fn optional_u32(row: &DbRow, column: &'static str) -> Result<Option<u32>> {
    row.try_get::<Option<i64>, _>(column)
        .repository()?
        .map(|value| {
            u32::try_from(value).map_err(|_| RepositoryError::Decode {
                kind: "source-probe count",
                value: value.to_string(),
            })
        })
        .transpose()
}

fn optional_u64(row: &DbRow, column: &'static str) -> Result<Option<u64>> {
    row.try_get::<Option<i64>, _>(column)
        .repository()?
        .map(|value| {
            u64::try_from(value).map_err(|_| RepositoryError::Decode {
                kind: "source-probe duration",
                value: value.to_string(),
            })
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Executor as _;

    async fn fixture(
        supports_import: bool,
    ) -> (
        tempfile::TempDir,
        DbPool,
        AddonMediaListSourceProbeCoordinator,
    ) {
        let temporary = tempfile::tempdir().expect("temporary source-probe database");
        let database_url = format!("sqlite:{}", temporary.path().join("probe.db").display());
        let pool = crate::db::init(&database_url)
            .await
            .expect("initialize source-probe database");
        let mut capabilities = vec![
            serde_json::json!({"id": "addon.check.v1"}),
            serde_json::json!({"id": "media-list.list.v6"}),
        ];
        if supports_import {
            capabilities.push(serde_json::json!({"id": "media-list.import.v1"}));
        }
        let manifest = serde_json::json!({
            "manifest_version": 4,
            "id": "org.example.source-probe",
            "version": "1.0.0",
            "name": {"en-US": "Source probe fixture"},
            "summary": {"en-US": "Source probe fixture"},
            "license": "MIT",
            "source_url": "https://example.test/source",
            "host_version": "*",
            "capabilities": capabilities,
            "entrypoints": {
                "any": {"path": "fixture", "kind": "posix_shell"}
            },
            "permissions": {
                "network_hosts": ["example.test"],
                "media_read": false,
                "managed_ffmpeg": false,
                "external_commands": []
            },
            "settings": [],
            "default_source_hosts": [],
            "panels": []
        });
        let now = encode_datetime(Utc::now());
        sqlx::query(
            r#"INSERT INTO "AddonInstallation" (
                   "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
                   "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt"
               ) VALUES ('org.example.source-probe', '1.0.0', 1, 'package', 'content',
                         ?, ?, ?, ?)"#,
        )
        .bind(manifest.to_string())
        .bind(
            temporary
                .path()
                .join("package")
                .to_string_lossy()
                .into_owned(),
        )
        .bind(&now)
        .bind(&now)
        .execute(&pool)
        .await
        .expect("seed source-probe add-on");
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" (
                   "Id", "AddonId", "ProviderId", "SourceUrl", "Name", "DraftRevision"
               ) VALUES (41, 'org.example.source-probe', 'saved/list',
                         'https://example.test/list?private=value', 'Saved list', 7)"#,
        )
        .execute(&pool)
        .await
        .expect("seed source-probe list");
        let coordinator = AddonMediaListSourceProbeCoordinator::new(pool.clone());
        (temporary, pool, coordinator)
    }

    #[tokio::test]
    async fn unsupported_probe_is_durable_non_mutating_and_expires_with_defaults() {
        let (_temporary, pool, coordinator) = fixture(false).await;
        assert!(matches!(
            coordinator.enqueue_source_probe(41, 6).await,
            Err(RepositoryError::Conflict(_))
        ));
        let job_id = coordinator
            .enqueue_source_probe(41, 7)
            .await
            .expect("queue source probe");
        assert_eq!(
            coordinator
                .enqueue_source_probe(41, 7)
                .await
                .expect("deduplicate queued probe"),
            job_id
        );
        let queued = coordinator
            .source_probe_status(41)
            .await
            .expect("load queued probe")
            .expect("queued probe");
        assert_eq!(queued.state, AddonMediaListSourceProbeState::Queued);
        assert_eq!(queued.saved_draft_revision, 7);

        coordinator
            .process_one_source_probe(Utc::now())
            .await
            .expect("process unsupported probe");
        let completed = coordinator
            .source_probe_status(41)
            .await
            .expect("load completed probe")
            .expect("completed probe");
        assert_eq!(completed.state, AddonMediaListSourceProbeState::Complete);
        assert_eq!(
            completed.outcome,
            Some(AddonMediaListSourceProbeOutcome::Unsupported)
        );
        assert_eq!(completed.attempt_count, 1);
        assert!(completed.provider_id.is_none());
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                r#"SELECT "SourceUrl" FROM "AddonMediaList" WHERE "Id" = 41"#
            )
            .fetch_one(&pool)
            .await
            .expect("source URL remains present"),
            "https://example.test/list?private=value"
        );
        let persisted_probe: String = sqlx::query_scalar(
            r#"SELECT GROUP_CONCAT(COALESCE("ProviderId", '') || COALESCE("Diagnostic", ''))
               FROM "AddonMediaListSourceProbeJob" WHERE "Id" = ?"#,
        )
        .bind(job_id)
        .fetch_one(&pool)
        .await
        .expect("load persisted safe result");
        assert!(!persisted_probe.contains("private=value"));

        pool.execute(
            r#"INSERT INTO "AddonSetting" ("AddonId", "Key", "Value")
               VALUES ('org.example.source-probe', 'region', 'nl')"#,
        )
        .await
        .expect("change a saved provider default");
        let stale = coordinator
            .source_probe_status(41)
            .await
            .expect("load stale probe")
            .expect("stale probe");
        assert_eq!(stale.state, AddonMediaListSourceProbeState::Stale);
    }

    #[tokio::test]
    async fn saved_probe_becomes_stale_when_the_current_package_cannot_load() {
        let (_temporary, pool, coordinator) = fixture(false).await;
        coordinator
            .enqueue_source_probe(41, 7)
            .await
            .expect("queue source probe");
        coordinator
            .process_one_source_probe(Utc::now())
            .await
            .expect("complete source probe");
        sqlx::query(
            r#"UPDATE "AddonInstallation" SET "ManifestJson" = '{}'
               WHERE "AddonId" = 'org.example.source-probe'"#,
        )
        .execute(&pool)
        .await
        .expect("make current package unloadable");

        let stale = coordinator
            .source_probe_status(41)
            .await
            .expect("return saved probe despite unloadable current package")
            .expect("saved probe");

        assert_eq!(stale.state, AddonMediaListSourceProbeState::Stale);
        assert_eq!(
            stale.outcome,
            Some(AddonMediaListSourceProbeOutcome::Unsupported)
        );
        assert_eq!(stale.saved_draft_revision, 7);
    }

    #[tokio::test]
    async fn expired_claim_is_recovered_without_holding_the_database_transaction() {
        let (_temporary, _pool, coordinator) = fixture(true).await;
        coordinator
            .enqueue_source_probe(41, 7)
            .await
            .expect("queue source probe");
        let started = Utc::now();
        let first = coordinator
            .claim(started)
            .await
            .expect("claim probe")
            .expect("first claim");
        assert!(coordinator
            .claim(started + Duration::seconds(89))
            .await
            .expect("probe remains leased")
            .is_none());
        let recovered = coordinator
            .claim(started + Duration::seconds(91))
            .await
            .expect("recover expired probe")
            .expect("recovered claim");
        assert_eq!(recovered.id, first.id);
        assert_ne!(recovered.lease_owner, first.lease_owner);
    }

    #[test]
    fn diagnostics_remove_source_urls_paths_and_credentials() {
        let error = RepositoryError::unavailable(
            "probe source",
            "https://example.test/private?q=token Authorization: Bearer secret /private/path",
        );
        let (_code, diagnostic) = redacted_probe_diagnostic(&error);
        assert!(diagnostic.contains("https://example.test"));
        assert!(!diagnostic.contains("/private"));
        assert!(!diagnostic.contains("q=token"));
        assert!(!diagnostic.contains("Bearer"));
        assert!(!diagnostic.contains("secret"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn successful_probe_persists_only_the_first_page_summary() {
        use std::os::unix::fs::PermissionsExt as _;

        let (temporary, pool, coordinator) = fixture(true).await;
        let package = temporary.path().join("package");
        tokio::fs::create_dir_all(&package)
            .await
            .expect("create fixture package");
        let script = package.join("fixture");
        tokio::fs::write(
            &script,
            r#"#!/bin/sh
set -eu
test "$1" = "discover"
cat >/dev/null
printf '%s\n' '{"record_type":"page","complete":false,"next_cursor":"page-2","total_hint":9,"revision_token":"source-r4"}'
printf '%s\n' '{"record_type":"list","provider_id":"provider/list","name":"Provider title"}'
printf '%s\n' '{"record_type":"item","provider_id":"item-1","rank":0,"display_title":"Private item title","title":"Private item title","kind":"remote_stream","source_url":"https://media.example.test/private"}'
"#,
        )
        .await
        .expect("write fixture entrypoint");
        let mut permissions = tokio::fs::metadata(&script)
            .await
            .expect("fixture entrypoint metadata")
            .permissions();
        permissions.set_mode(0o755);
        tokio::fs::set_permissions(&script, permissions)
            .await
            .expect("make fixture entrypoint executable");
        let content_sha256 = crate::addons::calculate_addon_content_sha256(&package)
            .await
            .expect("hash fixture package");
        sqlx::query(
            r#"UPDATE "AddonInstallation" SET "ContentSha256" = ?
               WHERE "AddonId" = 'org.example.source-probe'"#,
        )
        .bind(content_sha256)
        .execute(&pool)
        .await
        .expect("bind fixture package digest");

        coordinator
            .enqueue_source_probe(41, 7)
            .await
            .expect("queue source probe");
        coordinator
            .process_one_source_probe(Utc::now())
            .await
            .expect("process source probe");
        let probe = coordinator
            .source_probe_status(41)
            .await
            .expect("load source probe")
            .expect("completed source probe");
        assert_eq!(probe.state, AddonMediaListSourceProbeState::Complete);
        assert_eq!(
            probe.outcome,
            Some(AddonMediaListSourceProbeOutcome::Available)
        );
        assert_eq!(probe.provider_id.as_deref(), Some("provider/list"));
        assert_eq!(probe.list_title.as_deref(), Some("Provider title"));
        assert_eq!(probe.count_hint, Some(9));
        assert_eq!(probe.returned_item_count, Some(1));
        assert_eq!(probe.page_complete, Some(false));
        assert_eq!(probe.source_revision.as_deref(), Some("source-r4"));
        let raw_job: String = sqlx::query_scalar(
            r#"SELECT COALESCE("ProviderId", '') || COALESCE("ListTitle", '') ||
                      COALESCE("Diagnostic", '')
               FROM "AddonMediaListSourceProbeJob" WHERE "Id" = ?"#,
        )
        .bind(probe.id)
        .fetch_one(&pool)
        .await
        .expect("load persisted probe result");
        assert!(!raw_job.contains("Private item title"));
        assert!(!raw_job.contains("private=value"));
    }
}
