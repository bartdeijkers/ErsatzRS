//! Durable overview-first add-on list imports and deferred item enrichment.

use std::fmt::Write as _;

use chrono::{Duration, Utc};
use ersatzrs_addon_contract::{
    AddonMediaListRecord, MediaListDiscoveryPage, MediaListEnrichOutcomeKind,
    CAPABILITY_MEDIA_LIST_LIST_V3, CAPABILITY_MEDIA_LIST_LIST_V4, CAPABILITY_MEDIA_LIST_LIST_V5,
};
use ersatzrs_core::repositories::{
    AddonMediaListImportAction, AddonMediaListImportProgress, RepositoryError, Result,
};
use ersatzrs_domain::{
    AddonMediaList, AddonMediaListContentKind, AddonMediaListDetailPolicy,
    AddonMediaListDetailState, AddonMediaListDetailWaitRange, AddonMediaListImportState,
    AddonMediaListPresentationKind, EntityId,
};
use sha2::{Digest as _, Sha256};
use sqlx::Row as _;
use url::Url;
use uuid::Uuid;

use crate::addon_media_list::{
    decode_media_list_output_for_capability, InstalledAddonMediaListProvider,
    InstalledMediaListImportContext,
};
use crate::addon_media_list_sync::{AddonMediaListRemoteSynchronizer, AddonMediaListSynchronizer};
use crate::addon_media_storage::validated_folder_name;
use crate::db::{inserted_id, DbPool};
use crate::repositories::common::{
    decode_bool_column, decode_enum_column, encode_datetime, encode_enum,
};
use crate::repositories::SqlxResultExt as _;

const IMPORT_LEASE_SECONDS: i64 = 90;
const DETAIL_LEASE_SECONDS: i64 = 90;
const MAX_IMPORT_ATTEMPTS: i32 = 5;
const MAX_DETAIL_ATTEMPTS: i32 = 5;
const MAX_UNVERSIONED_VERIFICATION_PASS: i32 = 2;

/// Host coordinator for durable import and detail jobs.
#[derive(Clone)]
pub struct AddonMediaListImportCoordinator {
    pool: DbPool,
    provider: InstalledAddonMediaListProvider,
}

impl AddonMediaListImportCoordinator {
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self {
            provider: InstalledAddonMediaListProvider::new(pool.clone()),
            pool,
        }
    }

    /// Queue a resumable overview import, or use the typed legacy fallback.
    ///
    /// # Errors
    ///
    /// Returns an error when source/folder validation, legacy enumeration, or
    /// durable job creation fails.
    pub async fn add_or_refresh(
        &self,
        addon_id: &str,
        source_url: &str,
        requested_folder: Option<&str>,
        content_kind_override: AddonMediaListContentKind,
        chapter_input: &str,
        detail_policy: AddonMediaListDetailPolicy,
    ) -> Result<EntityId> {
        let context = self.provider.import_context(addon_id, source_url).await?;
        // Chapter expansion is itself a one-item operation whose frozen list
        // behavior predates import v1. Keep that explicit compatibility path
        // until a future contract versions chapter editing independently.
        let Some(context) = context.filter(|_| chapter_input.trim().is_empty()) else {
            let provider = InstalledAddonMediaListProvider::new(self.pool.clone());
            let id = AddonMediaListRemoteSynchronizer::new(self.pool.clone(), provider)
                .add_or_refresh(
                    addon_id,
                    source_url,
                    requested_folder,
                    content_kind_override,
                    chapter_input,
                )
                .await?;
            self.set_detail_policy(id, detail_policy).await?;
            return Ok(id);
        };

        let requested_folder = match requested_folder
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(folder) => Some(
                validated_folder_name(&self.pool, addon_id, &context.source_url, folder).await?,
            ),
            None => None,
        };
        self.queue_import(
            addon_id,
            &context,
            requested_folder.as_deref(),
            content_kind_override,
            detail_policy,
            None,
        )
        .await
    }

    /// Move an existing list to another provider and queue a resumable
    /// generation when that provider supports import v1.
    ///
    /// # Errors
    ///
    /// Returns an error when the list or provider is unavailable, the source
    /// conflicts with another list, or the generation cannot be persisted.
    pub async fn change_addon(&self, list_id: EntityId, addon_id: &str) -> Result<()> {
        let row = sqlx::query(
            r#"SELECT "AddonId", "SourceUrl", "ContentKindOverride", "ChapterInput",
                      "DetailImportPolicy"
               FROM "AddonMediaList" WHERE "Id" = ?"#,
        )
        .bind(list_id)
        .fetch_optional(&self.pool)
        .await
        .repository()?
        .ok_or(RepositoryError::NotFound {
            kind: "add-on media list",
            id: list_id,
        })?;
        let current_addon_id: String = row.try_get("AddonId").repository()?;
        let addon_id = addon_id.trim();
        if current_addon_id == addon_id {
            return Ok(());
        }
        let source_url: String = row.try_get("SourceUrl").repository()?;
        let chapter_input = row
            .try_get::<Option<String>, _>("ChapterInput")
            .repository()?
            .unwrap_or_default();
        let content_kind =
            decode_enum_column("AddonMediaListContentKind", &row, "ContentKindOverride")?;
        let detail_policy =
            decode_enum_column("AddonMediaListDetailPolicy", &row, "DetailImportPolicy")?;
        let context = self.provider.import_context(addon_id, &source_url).await?;
        let Some(context) = context.filter(|_| chapter_input.trim().is_empty()) else {
            return AddonMediaListRemoteSynchronizer::new(self.pool.clone(), self.provider.clone())
                .change_addon(list_id, addon_id)
                .await;
        };
        self.queue_import(
            addon_id,
            &context,
            None,
            content_kind,
            detail_policy,
            Some(list_id),
        )
        .await?;
        Ok(())
    }

    /// Queue due automatic refreshes through the resumable transport whenever
    /// the installed package supports it. An unfinished generation is never
    /// superseded by the hourly maintenance tick.
    ///
    /// # Errors
    ///
    /// Returns an error when due lists cannot be loaded or decoded. A failure
    /// to queue one individual provider is retained on that list and does not
    /// abort the remaining refresh candidates.
    pub async fn refresh_due_auto_lists(&self) -> Result<usize> {
        let cutoff = encode_datetime(Utc::now() - Duration::days(1));
        let rows = sqlx::query(
            r#"SELECT "list"."AddonId", "list"."SourceUrl",
                      "list"."ContentKindOverride", "list"."ChapterInput",
                      "list"."DetailImportPolicy"
               FROM "AddonMediaList" AS "list"
               JOIN "AddonInstallation" AS "addon"
                 ON "addon"."AddonId" = "list"."AddonId"
               WHERE "list"."AutoRefresh" = 1
                 AND "addon"."Enabled" = 1
                 AND ("list"."LastUpdate" IS NULL OR "list"."LastUpdate" <= ?)
                 AND NOT EXISTS (
                     SELECT 1 FROM "AddonMediaListImportJob" AS "job"
                     WHERE "job"."AddonMediaListId" = "list"."Id"
                       AND "job"."State" IN (?, ?, ?, ?, ?)
                 )
               ORDER BY "list"."Id""#,
        )
        .bind(&cutoff)
        .bind(encode_enum(AddonMediaListImportState::Queued))
        .bind(encode_enum(AddonMediaListImportState::Discovering))
        .bind(encode_enum(AddonMediaListImportState::Verifying))
        .bind(encode_enum(AddonMediaListImportState::Paused))
        .bind(encode_enum(AddonMediaListImportState::NeedsAttention))
        .fetch_all(&self.pool)
        .await
        .repository()?;

        let mut queued = 0_usize;
        for row in rows {
            let addon_id: String = row.try_get("AddonId").repository()?;
            let source_url: String = row.try_get("SourceUrl").repository()?;
            let content_kind =
                decode_enum_column("AddonMediaListContentKind", &row, "ContentKindOverride")?;
            let detail_policy =
                decode_enum_column("AddonMediaListDetailPolicy", &row, "DetailImportPolicy")?;
            let chapter_input = row
                .try_get::<Option<String>, _>("ChapterInput")
                .repository()?
                .unwrap_or_default();
            if self
                .add_or_refresh(
                    &addon_id,
                    &source_url,
                    None,
                    content_kind,
                    &chapter_input,
                    detail_policy,
                )
                .await
                .is_ok()
            {
                queued += 1;
            }
        }
        Ok(queued)
    }

    #[allow(clippy::too_many_lines)] // One transaction owns list identity and generation creation.
    async fn queue_import(
        &self,
        addon_id: &str,
        context: &InstalledMediaListImportContext,
        requested_folder: Option<&str>,
        content_kind_override: AddonMediaListContentKind,
        detail_policy: AddonMediaListDetailPolicy,
        target_list_id: Option<EntityId>,
    ) -> Result<EntityId> {
        let now = encode_datetime(Utc::now());
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        let existing = if let Some(list_id) = target_list_id {
            let conflict: Option<EntityId> = sqlx::query_scalar(
                r#"SELECT "Id" FROM "AddonMediaList"
                   WHERE "AddonId" = ? AND "SourceUrl" = ? AND "Id" <> ?
                   ORDER BY "Id" LIMIT 1"#,
            )
            .bind(addon_id)
            .bind(&context.source_url)
            .bind(list_id)
            .fetch_optional(&mut *tx)
            .await
            .repository()?;
            if let Some(conflict) = conflict {
                return Err(RepositoryError::Conflict(format!(
                    "add-on media list {conflict} already uses this provider and source"
                )));
            }
            sqlx::query(r#"SELECT "Id", "ReviewState" FROM "AddonMediaList" WHERE "Id" = ?"#)
                .bind(list_id)
                .fetch_optional(&mut *tx)
                .await
                .repository()?
        } else {
            sqlx::query(
                r#"SELECT "Id", "ReviewState" FROM "AddonMediaList"
                   WHERE "AddonId" = ? AND "SourceUrl" = ?
                   ORDER BY "Id" LIMIT 1"#,
            )
            .bind(addon_id)
            .bind(&context.source_url)
            .fetch_optional(&mut *tx)
            .await
            .repository()?
        };
        let list_id = if let Some(row) = existing {
            let id: EntityId = row.try_get("Id").repository()?;
            let review_state: i32 = row.try_get("ReviewState").repository()?;
            if review_state == 3 {
                return Err(RepositoryError::Conflict(
                    "the add-on media list is being published; wait for publication to finish"
                        .to_owned(),
                ));
            }
            sqlx::query(
                r#"UPDATE "AddonMediaList"
                   SET "AddonId" = ?, "ContentKindOverride" = ?, "DetailImportPolicy" = ?,
                       "LastError" = NULL
                   WHERE "Id" = ?"#,
            )
            .bind(addon_id)
            .bind(encode_enum(content_kind_override))
            .bind(encode_enum(detail_policy))
            .bind(id)
            .execute(&mut *tx)
            .await
            .repository()?;
            id
        } else {
            let host = Url::parse(&context.source_url)
                .ok()
                .and_then(|url| url.host_str().map(ToOwned::to_owned))
                .unwrap_or_else(|| "add-on list".to_owned());
            let pending_provider_id = format!("pending:{}", Uuid::new_v4());
            let result = sqlx::query(
                r#"INSERT INTO "AddonMediaList" (
                    "AddonId", "SourceUrl", "ProviderId", "Name", "Description",
                    "AutoRefresh", "GeneratePlaylist", "ItemCount", "PlaylistId",
                    "LastUpdate", "LastMatch", "LastError", "ContentKindOverride",
                    "ChapterInput", "ProviderMetadataJson", "MetadataOverrideJson",
                    "DetailImportPolicy"
                   ) VALUES (?, ?, ?, ?, NULL, 0, 0, 0, NULL, NULL, NULL, NULL, ?, NULL, '{}', '{}', ?)"#,
            )
            .bind(addon_id)
            .bind(&context.source_url)
            .bind(pending_provider_id)
            .bind(host)
            .bind(encode_enum(content_kind_override))
            .bind(encode_enum(detail_policy))
            .execute(&mut *tx)
            .await
            .repository()?;
            inserted_id(&mut tx, result).await.repository()?
        };

        let generation: i32 = sqlx::query_scalar(
            r#"SELECT COALESCE(MAX("Generation"), 0) + 1
               FROM "AddonMediaListImportJob" WHERE "AddonMediaListId" = ?"#,
        )
        .bind(list_id)
        .fetch_one(&mut *tx)
        .await
        .repository()?;
        sqlx::query(
            r#"UPDATE "AddonMediaListImportJob"
               SET "State" = ?, "LeaseOwner" = NULL, "LeaseExpiresAt" = NULL,
                   "UpdatedAt" = ?
               WHERE "AddonMediaListId" = ? AND "State" IN (?, ?, ?, ?, ?)"#,
        )
        .bind(encode_enum(AddonMediaListImportState::Superseded))
        .bind(&now)
        .bind(list_id)
        .bind(encode_enum(AddonMediaListImportState::Queued))
        .bind(encode_enum(AddonMediaListImportState::Discovering))
        .bind(encode_enum(AddonMediaListImportState::Verifying))
        .bind(encode_enum(AddonMediaListImportState::Paused))
        .bind(encode_enum(AddonMediaListImportState::NeedsAttention))
        .execute(&mut *tx)
        .await
        .repository()?;
        sqlx::query(
            r#"INSERT INTO "AddonMediaListImportJob" (
                "AddonMediaListId", "Generation", "State", "RecordCapability",
                "RequestedFolder", "CreatedAt", "UpdatedAt", "LastProgressAt"
               ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(list_id)
        .bind(generation)
        .bind(encode_enum(AddonMediaListImportState::Queued))
        .bind(&context.record_capability)
        .bind(requested_folder)
        .bind(&now)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .repository()?;
        tx.commit().await.repository()?;
        Ok(list_id)
    }

    /// Process at most one durable overview page.
    ///
    /// # Errors
    ///
    /// Returns an error only for host persistence failures. Provider and
    /// contract failures are checkpointed as retry/attention state.
    pub async fn process_one_import_page(&self) -> Result<bool> {
        self.recover_cancelled_imports().await?;
        let Some(job) = self.claim_import_job().await? else {
            return Ok(false);
        };
        if job.overview_complete {
            if let Err(error) = self.finalize_import_pass(&job).await {
                self.fail_import(job.id, &error.to_string(), false).await?;
            }
            return Ok(true);
        }
        let context = match self
            .provider
            .import_context(&job.addon_id, &job.source_url)
            .await
        {
            Ok(Some(context)) if context.record_capability == job.record_capability => context,
            Ok(Some(_)) => {
                self.fail_import(
                    job.id,
                    "the add-on record schema changed during this import",
                    false,
                )
                .await?;
                return Ok(true);
            }
            Ok(None) => {
                self.fail_import(
                    job.id,
                    "the installed add-on no longer supports media-list.import.v1",
                    false,
                )
                .await?;
                return Ok(true);
            }
            Err(error) => {
                self.fail_import(job.id, &error.to_string(), true).await?;
                return Ok(true);
            }
        };
        match self
            .provider
            .discover(&job.addon_id, &context, job.cursor.as_deref())
            .await
        {
            Ok(page) => {
                if let Err(error) = self.commit_import_page(&job, page).await {
                    self.fail_import(job.id, &error.to_string(), false).await?;
                }
            }
            Err(error) => self.fail_import(job.id, &error.to_string(), true).await?,
        }
        Ok(true)
    }

    async fn recover_cancelled_imports(&self) -> Result<()> {
        sqlx::query(
            r#"UPDATE "AddonMediaListImportJob"
               SET "State" = ?, "LeaseOwner" = NULL, "LeaseExpiresAt" = NULL,
                   "UpdatedAt" = ?
               WHERE "CancellationRequested" = 1 AND "State" IN (?, ?, ?, ?, ?)"#,
        )
        .bind(encode_enum(AddonMediaListImportState::Cancelled))
        .bind(encode_datetime(Utc::now()))
        .bind(encode_enum(AddonMediaListImportState::Queued))
        .bind(encode_enum(AddonMediaListImportState::Discovering))
        .bind(encode_enum(AddonMediaListImportState::Verifying))
        .bind(encode_enum(AddonMediaListImportState::Paused))
        .bind(encode_enum(AddonMediaListImportState::NeedsAttention))
        .execute(&self.pool)
        .await
        .repository()?;
        Ok(())
    }

    async fn claim_import_job(&self) -> Result<Option<ImportJob>> {
        let now = Utc::now();
        let now_text = encode_datetime(now);
        let lease_owner = Uuid::new_v4().to_string();
        let lease_expires = encode_datetime(now + Duration::seconds(IMPORT_LEASE_SECONDS));
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        let row = sqlx::query(
            r#"SELECT "job"."Id", "job"."AddonMediaListId",
                      "job"."RecordCapability", "job"."Cursor",
                      "job"."VerificationPass", "job"."ExpectedIdentityHash",
                      CAST(CASE WHEN "job"."OverviewComplete" = 1 THEN 1 ELSE 0 END AS SIGNED)
                          AS "OverviewComplete",
                      "list"."AddonId", "list"."SourceUrl"
               FROM "AddonMediaListImportJob" AS "job"
               JOIN "AddonMediaList" AS "list" ON "list"."Id" = "job"."AddonMediaListId"
               WHERE "job"."State" IN (?, ?, ?)
                 AND "job"."CancellationRequested" = 0
                 AND ("job"."NextEligibleAt" IS NULL OR "job"."NextEligibleAt" <= ?)
                 AND ("job"."LeaseExpiresAt" IS NULL OR "job"."LeaseExpiresAt" <= ?)
               ORDER BY "job"."UpdatedAt", "job"."Id" LIMIT 1"#,
        )
        .bind(encode_enum(AddonMediaListImportState::Queued))
        .bind(encode_enum(AddonMediaListImportState::Discovering))
        .bind(encode_enum(AddonMediaListImportState::Verifying))
        .bind(&now_text)
        .bind(&now_text)
        .fetch_optional(&mut *tx)
        .await
        .repository()?;
        let Some(row) = row else {
            tx.commit().await.repository()?;
            return Ok(None);
        };
        let id: EntityId = row.try_get("Id").repository()?;
        let verification_pass: i32 = row.try_get("VerificationPass").repository()?;
        let state = if verification_pass == 0 {
            AddonMediaListImportState::Discovering
        } else {
            AddonMediaListImportState::Verifying
        };
        let claimed = sqlx::query(
            r#"UPDATE "AddonMediaListImportJob"
               SET "State" = ?, "LeaseOwner" = ?, "LeaseExpiresAt" = ?,
                   "HeartbeatAt" = ?, "UpdatedAt" = ?
               WHERE "Id" = ? AND ("LeaseExpiresAt" IS NULL OR "LeaseExpiresAt" <= ?)"#,
        )
        .bind(encode_enum(state))
        .bind(&lease_owner)
        .bind(&lease_expires)
        .bind(&now_text)
        .bind(&now_text)
        .bind(id)
        .bind(&now_text)
        .execute(&mut *tx)
        .await
        .repository()?;
        if claimed.rows_affected() != 1 {
            tx.rollback().await.repository()?;
            return Ok(None);
        }
        let job = ImportJob {
            id,
            list_id: row.try_get("AddonMediaListId").repository()?,
            addon_id: row.try_get("AddonId").repository()?,
            source_url: row.try_get("SourceUrl").repository()?,
            record_capability: row.try_get("RecordCapability").repository()?,
            cursor: row.try_get("Cursor").repository()?,
            verification_pass,
            expected_identity_hash: row.try_get("ExpectedIdentityHash").repository()?,
            overview_complete: decode_bool_column(&row, "OverviewComplete")?,
        };
        tx.commit().await.repository()?;
        Ok(Some(job))
    }

    #[allow(clippy::too_many_lines)] // The page and every cursor/item checkpoint commit atomically.
    async fn commit_import_page(
        &self,
        job: &ImportJob,
        page: MediaListDiscoveryPage,
    ) -> Result<()> {
        let mut list_record = None;
        let mut item_records = Vec::new();
        for record in page.records {
            match record {
                AddonMediaListRecord::List { .. } => list_record = Some(record),
                AddonMediaListRecord::Item { .. } => item_records.push(record),
            }
        }
        let now = encode_datetime(Utc::now());
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        let current = sqlx::query(
            r#"SELECT "Cursor", "RevisionToken", "ListRecordJson", "DiscoveredCount",
                      "VerificationPass",
                      CAST(CASE WHEN "CancellationRequested" = 1 THEN 1 ELSE 0 END AS SIGNED)
                          AS "CancellationRequested"
               FROM "AddonMediaListImportJob" WHERE "Id" = ?"#,
        )
        .bind(job.id)
        .fetch_one(&mut *tx)
        .await
        .repository()?;
        if decode_bool_column(&current, "CancellationRequested")? {
            sqlx::query(
                r#"UPDATE "AddonMediaListImportJob"
                   SET "State" = ?, "LeaseOwner" = NULL, "LeaseExpiresAt" = NULL,
                       "UpdatedAt" = ? WHERE "Id" = ?"#,
            )
            .bind(encode_enum(AddonMediaListImportState::Cancelled))
            .bind(&now)
            .bind(job.id)
            .execute(&mut *tx)
            .await
            .repository()?;
            tx.commit().await.repository()?;
            return Ok(());
        }
        let persisted_pass: i32 = current.try_get("VerificationPass").repository()?;
        let persisted_cursor: Option<String> = current.try_get("Cursor").repository()?;
        if persisted_pass != job.verification_pass || persisted_cursor != job.cursor {
            return Err(RepositoryError::Conflict(
                "the overview checkpoint changed while a page was running".to_owned(),
            ));
        }
        let persisted_count: i32 = current.try_get("DiscoveredCount").repository()?;
        let persisted_revision: Option<String> = current.try_get("RevisionToken").repository()?;
        if (persisted_count > 0 || job.cursor.is_some())
            && persisted_revision != page.revision_token
        {
            return Err(RepositoryError::Conflict(
                "the provider revision changed between overview pages".to_owned(),
            ));
        }

        if let Some(record) = list_record {
            let json = serde_json::to_string(&record).map_err(|error| RepositoryError::Decode {
                kind: "media-list overview header",
                value: error.to_string(),
            })?;
            let previous: Option<String> = current.try_get("ListRecordJson").repository()?;
            if let Some(previous) = previous {
                if previous != json {
                    return Err(RepositoryError::Conflict(
                        "the provider list header changed during overview verification".to_owned(),
                    ));
                }
            } else {
                sqlx::query(
                    r#"UPDATE "AddonMediaListImportJob" SET "ListRecordJson" = ? WHERE "Id" = ?"#,
                )
                .bind(json)
                .bind(job.id)
                .execute(&mut *tx)
                .await
                .repository()?;
            }
        }

        for record in &item_records {
            let AddonMediaListRecord::Item {
                provider_id, rank, ..
            } = record
            else {
                unreachable!("item records were separated above")
            };
            let duplicate: Option<i32> = sqlx::query_scalar(
                r#"SELECT 1 FROM "AddonMediaListImportItem"
                   WHERE "AddonMediaListImportJobId" = ? AND "VerificationPass" = ?
                     AND ("ProviderId" = ? OR "Rank" = ?) LIMIT 1"#,
            )
            .bind(job.id)
            .bind(job.verification_pass)
            .bind(provider_id)
            .bind(*rank)
            .fetch_optional(&mut *tx)
            .await
            .repository()?;
            if duplicate.is_some() {
                return Err(RepositoryError::Conflict(
                    "the provider repeated an item identity or rank across overview pages"
                        .to_owned(),
                ));
            }
            let json = serde_json::to_string(record).map_err(|error| RepositoryError::Decode {
                kind: "media-list overview item",
                value: error.to_string(),
            })?;
            let fingerprint = sha256_hex(json.as_bytes());
            sqlx::query(
                r#"INSERT INTO "AddonMediaListImportItem" (
                    "AddonMediaListImportJobId", "VerificationPass", "ProviderId", "Rank",
                    "RecordJson", "OverviewFingerprint"
                   ) VALUES (?, ?, ?, ?, ?, ?)"#,
            )
            .bind(job.id)
            .bind(job.verification_pass)
            .bind(provider_id)
            .bind(*rank)
            .bind(json)
            .bind(fingerprint)
            .execute(&mut *tx)
            .await
            .repository()?;
        }

        if let Some(next_cursor) = page.next_cursor.as_deref() {
            let hash = sha256_hex(next_cursor.as_bytes());
            let seen: Option<i32> = sqlx::query_scalar(
                r#"SELECT 1 FROM "AddonMediaListImportCursor"
                   WHERE "AddonMediaListImportJobId" = ? AND "VerificationPass" = ?
                     AND "CursorHash" = ? LIMIT 1"#,
            )
            .bind(job.id)
            .bind(job.verification_pass)
            .bind(&hash)
            .fetch_optional(&mut *tx)
            .await
            .repository()?;
            if seen.is_some() {
                return Err(RepositoryError::Conflict(
                    "the provider repeated an overview continuation cursor".to_owned(),
                ));
            }
            sqlx::query(
                r#"INSERT INTO "AddonMediaListImportCursor" (
                    "AddonMediaListImportJobId", "VerificationPass", "CursorHash"
                   ) VALUES (?, ?, ?)"#,
            )
            .bind(job.id)
            .bind(job.verification_pass)
            .bind(hash)
            .execute(&mut *tx)
            .await
            .repository()?;
        }
        let discovered_count =
            persisted_count.saturating_add(i32::try_from(item_records.len()).unwrap_or(i32::MAX));
        sqlx::query(
            r#"UPDATE "AddonMediaListImportJob"
               SET "Cursor" = ?, "RevisionToken" = ?, "TotalHint" = ?,
                   "DiscoveredCount" = ?, "OverviewComplete" = ?,
                   "Diagnostic" = NULL, "AttemptCount" = 0,
                   "NextEligibleAt" = NULL, "HeartbeatAt" = ?, "LastProgressAt" = ?,
                   "UpdatedAt" = ?, "LeaseOwner" = NULL, "LeaseExpiresAt" = NULL
               WHERE "Id" = ?"#,
        )
        .bind(page.next_cursor.as_deref())
        .bind(page.revision_token.as_deref())
        .bind(page.total_hint.map(i64::from))
        .bind(discovered_count)
        .bind(page.complete)
        .bind(&now)
        .bind(&now)
        .bind(&now)
        .bind(job.id)
        .execute(&mut *tx)
        .await
        .repository()?;
        tx.commit().await.repository()?;

        if !page.complete {
            return Ok(());
        }
        self.finalize_import_pass(job).await
    }

    async fn finalize_import_pass(&self, job: &ImportJob) -> Result<()> {
        let row = sqlx::query(
            r#"SELECT CAST(CASE WHEN "OverviewComplete" = 1 THEN 1 ELSE 0 END AS SIGNED)
                          AS "OverviewComplete",
                      "RevisionToken"
               FROM "AddonMediaListImportJob" WHERE "Id" = ?"#,
        )
        .bind(job.id)
        .fetch_one(&self.pool)
        .await
        .repository()?;
        if !decode_bool_column(&row, "OverviewComplete")? {
            return Err(RepositoryError::Conflict(
                "the overview pass is not completely checkpointed".to_owned(),
            ));
        }
        let revision_token: Option<String> = row.try_get("RevisionToken").repository()?;
        let now = encode_datetime(Utc::now());
        let identity_hash = self.identity_hash(job.id, job.verification_pass).await?;
        if revision_token.is_none() && job.verification_pass == 0 {
            self.start_unversioned_verification(job.id, 1, &identity_hash, &now)
                .await?;
            return Ok(());
        }
        if job.verification_pass >= 1
            && job.expected_identity_hash.as_deref() != Some(identity_hash.as_str())
        {
            if job.verification_pass < MAX_UNVERSIONED_VERIFICATION_PASS {
                self.start_unversioned_verification(
                    job.id,
                    job.verification_pass.saturating_add(1),
                    &identity_hash,
                    &now,
                )
                .await?;
                return Ok(());
            }
            return Err(RepositoryError::Conflict(
                "the unversioned provider overview changed repeatedly during verification"
                    .to_owned(),
            ));
        }
        self.promote_complete_overview(job, job.verification_pass)
            .await
    }

    async fn start_unversioned_verification(
        &self,
        job_id: EntityId,
        verification_pass: i32,
        expected_identity_hash: &str,
        now: &str,
    ) -> Result<()> {
        sqlx::query(
            r#"UPDATE "AddonMediaListImportJob"
               SET "State" = ?, "VerificationPass" = ?, "ExpectedIdentityHash" = ?,
                   "Cursor" = NULL, "RevisionToken" = NULL, "DiscoveredCount" = 0,
                   "TotalHint" = NULL, "OverviewComplete" = 0,
                   "UpdatedAt" = ?, "LastProgressAt" = ?
               WHERE "Id" = ?"#,
        )
        .bind(encode_enum(AddonMediaListImportState::Verifying))
        .bind(verification_pass)
        .bind(expected_identity_hash)
        .bind(now)
        .bind(now)
        .bind(job_id)
        .execute(&self.pool)
        .await
        .repository()?;
        Ok(())
    }

    async fn identity_hash(&self, job_id: EntityId, pass: i32) -> Result<String> {
        let rows = sqlx::query(
            r#"SELECT "ProviderId", "Rank" FROM "AddonMediaListImportItem"
               WHERE "AddonMediaListImportJobId" = ? AND "VerificationPass" = ?
               ORDER BY "Rank", "ProviderId""#,
        )
        .bind(job_id)
        .bind(pass)
        .fetch_all(&self.pool)
        .await
        .repository()?;
        let mut digest = Sha256::new();
        for row in rows {
            let provider_id: String = row.try_get("ProviderId").repository()?;
            let rank: i32 = row.try_get("Rank").repository()?;
            digest.update(rank.to_be_bytes());
            digest.update([0]);
            digest.update(provider_id.as_bytes());
            digest.update(b"\n");
        }
        Ok(encode_hex(&digest.finalize()))
    }

    #[allow(clippy::too_many_lines)] // Promotion is one rollback-safe overview transaction.
    async fn promote_complete_overview(&self, job: &ImportJob, pass: i32) -> Result<()> {
        let row = sqlx::query(
            r#"SELECT "job"."ListRecordJson", "job"."RequestedFolder",
                      "list"."AddonId", "list"."SourceUrl", "list"."ContentKindOverride",
                      "list"."ChapterInput"
               FROM "AddonMediaListImportJob" AS "job"
               JOIN "AddonMediaList" AS "list" ON "list"."Id" = "job"."AddonMediaListId"
               WHERE "job"."Id" = ?"#,
        )
        .bind(job.id)
        .fetch_one(&self.pool)
        .await
        .repository()?;
        let header: String = row
            .try_get::<Option<String>, _>("ListRecordJson")
            .repository()?
            .ok_or_else(|| {
                RepositoryError::Conflict("the overview has no list header".to_owned())
            })?;
        let item_rows = sqlx::query(
            r#"SELECT "RecordJson" FROM "AddonMediaListImportItem"
               WHERE "AddonMediaListImportJobId" = ? AND "VerificationPass" = ?
               ORDER BY "Rank", "Id""#,
        )
        .bind(job.id)
        .bind(pass)
        .fetch_all(&self.pool)
        .await
        .repository()?;
        let mut output = Vec::new();
        output.extend_from_slice(header.as_bytes());
        output.push(b'\n');
        for item in &item_rows {
            let json: String = item.try_get("RecordJson").repository()?;
            output.extend_from_slice(json.as_bytes());
            output.push(b'\n');
        }
        let supports_v3 = matches!(
            job.record_capability.as_str(),
            CAPABILITY_MEDIA_LIST_LIST_V3
                | CAPABILITY_MEDIA_LIST_LIST_V4
                | CAPABILITY_MEDIA_LIST_LIST_V5
        );
        let supports_v4 = matches!(
            job.record_capability.as_str(),
            CAPABILITY_MEDIA_LIST_LIST_V4 | CAPABILITY_MEDIA_LIST_LIST_V5
        );
        let mut enumerated = decode_media_list_output_for_capability(
            &output,
            supports_v3,
            supports_v4,
            job.record_capability == CAPABILITY_MEDIA_LIST_LIST_V5,
        )?;
        self.overlay_completed_details(job, pass, &mut enumerated)
            .await?;

        let conflicting: Option<EntityId> = sqlx::query_scalar(
            r#"SELECT "Id" FROM "AddonMediaList"
               WHERE "AddonId" = ? AND "ProviderId" = ? AND "Id" <> ? LIMIT 1"#,
        )
        .bind(&job.addon_id)
        .bind(&enumerated.provider_id)
        .bind(job.list_id)
        .fetch_optional(&self.pool)
        .await
        .repository()?;
        if conflicting.is_some() {
            return Err(RepositoryError::Conflict(
                "another imported list already owns this provider identity".to_owned(),
            ));
        }
        sqlx::query(r#"UPDATE "AddonMediaList" SET "ProviderId" = ? WHERE "Id" = ?"#)
            .bind(&enumerated.provider_id)
            .bind(job.list_id)
            .execute(&self.pool)
            .await
            .repository()?;

        let chapter_input = row
            .try_get::<Option<String>, _>("ChapterInput")
            .repository()?
            .unwrap_or_default();
        let list = AddonMediaList {
            id: job.list_id,
            addon_id: row.try_get("AddonId").repository()?,
            source_url: row.try_get("SourceUrl").repository()?,
            provider_id: enumerated.provider_id.clone(),
            name: enumerated.name.clone(),
            description: enumerated.description.clone(),
            item_count: i32::try_from(enumerated.items.len()).unwrap_or(i32::MAX),
            auto_refresh: false,
            generate_playlist: false,
            content_kind_override: decode_enum_column(
                "AddonMediaListContentKind",
                &row,
                "ContentKindOverride",
            )?,
            presentation_kind: AddonMediaListPresentationKind::RemoteStreams,
            requested_presentation_kind: AddonMediaListPresentationKind::RemoteStreams,
            presentation_error: None,
            chapter_input,
            playlist_id: None,
            last_update: None,
            last_match: None,
            last_error: None,
        };
        let requested_folder: Option<String> = row.try_get("RequestedFolder").repository()?;
        AddonMediaListRemoteSynchronizer::new(self.pool.clone(), self.provider.clone())
            .persist_enumerated(&list, &mut enumerated, requested_folder.as_deref())
            .await?;
        self.initialize_detail_jobs(job.id, job.list_id, pass)
            .await?;
        let now = encode_datetime(Utc::now());
        sqlx::query(
            r#"UPDATE "AddonMediaListImportJob"
               SET "State" = ?, "CompletedAt" = ?, "UpdatedAt" = ?,
                   "LastProgressAt" = ?, "LeaseOwner" = NULL, "LeaseExpiresAt" = NULL,
                   "Diagnostic" = NULL, "OverviewComplete" = 0
               WHERE "Id" = ?"#,
        )
        .bind(encode_enum(AddonMediaListImportState::Ready))
        .bind(&now)
        .bind(&now)
        .bind(&now)
        .bind(job.id)
        .execute(&self.pool)
        .await
        .repository()?;
        Ok(())
    }

    async fn overlay_completed_details(
        &self,
        job: &ImportJob,
        pass: i32,
        enumerated: &mut ersatzrs_core::addon_media_list::EnumeratedAddonMediaList,
    ) -> Result<()> {
        let rows = sqlx::query(
            r#"SELECT "staged"."ProviderId", "detail"."ResultRecordJson",
                      "item"."Id" AS "ItemId", "detail"."ProviderHost",
                      "detail"."OverviewFingerprint", "detail"."OverviewRecordJson",
                      "detail"."AttemptCount"
               FROM "AddonMediaListImportItem" AS "staged"
               JOIN "AddonMediaListItem" AS "item"
                 ON "item"."AddonMediaListId" = ?
                AND "item"."ProviderId" = "staged"."ProviderId"
               JOIN "AddonMediaListDetailJob" AS "detail"
                 ON "detail"."AddonMediaListItemId" = "item"."Id"
                AND "detail"."OverviewFingerprint" = "staged"."OverviewFingerprint"
               WHERE "staged"."AddonMediaListImportJobId" = ?
                 AND "staged"."VerificationPass" = ?
                 AND "detail"."State" IN (?, ?)
                 AND "detail"."ResultRecordJson" IS NOT NULL"#,
        )
        .bind(job.list_id)
        .bind(job.id)
        .bind(pass)
        .bind(encode_enum(AddonMediaListDetailState::Complete))
        .bind(encode_enum(AddonMediaListDetailState::Skipped))
        .fetch_all(&self.pool)
        .await
        .repository()?;
        for row in rows {
            let provider_id: String = row.try_get("ProviderId").repository()?;
            let record_json: String = row.try_get("ResultRecordJson").repository()?;
            let record =
                serde_json::from_str::<AddonMediaListRecord>(&record_json).map_err(|error| {
                    RepositoryError::Decode {
                        kind: "preserved media-list enrichment",
                        value: error.to_string(),
                    }
                })?;
            let detail_job = DetailJob {
                item_id: row.try_get("ItemId").repository()?,
                list_id: job.list_id,
                addon_id: job.addon_id.clone(),
                source_url: job.source_url.clone(),
                record_capability: job.record_capability.clone(),
                provider_id: provider_id.clone(),
                provider_host: row.try_get("ProviderHost").repository()?,
                overview_fingerprint: row.try_get("OverviewFingerprint").repository()?,
                overview_record_json: row.try_get("OverviewRecordJson").repository()?,
                attempt_count: row.try_get("AttemptCount").repository()?,
            };
            let mut enriched = Self::decode_enriched_item(&detail_job, &record)?;
            if let Some(target) = enumerated
                .items
                .iter_mut()
                .find(|item| item.provider_id == provider_id)
            {
                enriched.rank = target.rank;
                *target = enriched;
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)] // Initialization reconciles every item state in one transaction.
    async fn initialize_detail_jobs(
        &self,
        import_job_id: EntityId,
        list_id: EntityId,
        pass: i32,
    ) -> Result<()> {
        let source_url: String =
            sqlx::query_scalar(r#"SELECT "SourceUrl" FROM "AddonMediaList" WHERE "Id" = ?"#)
                .bind(list_id)
                .fetch_one(&self.pool)
                .await
                .repository()?;
        let provider_host = provider_host(&source_url)?;
        let rows = sqlx::query(
            r#"SELECT "staged"."ProviderId", "staged"."OverviewFingerprint",
                      "staged"."RecordJson", "item"."Id"
               FROM "AddonMediaListImportItem" AS "staged"
               JOIN "AddonMediaListItem" AS "item"
                 ON "item"."AddonMediaListId" = ?
                AND "item"."ProviderId" = "staged"."ProviderId"
               WHERE "staged"."AddonMediaListImportJobId" = ?
                 AND "staged"."VerificationPass" = ?
                 AND "item"."ParentProviderId" IS NULL"#,
        )
        .bind(list_id)
        .bind(import_job_id)
        .bind(pass)
        .fetch_all(&self.pool)
        .await
        .repository()?;
        let now = encode_datetime(Utc::now());
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        for row in rows {
            let item_id: EntityId = row.try_get("Id").repository()?;
            let fingerprint: String = row.try_get("OverviewFingerprint").repository()?;
            let record_json: String = row.try_get("RecordJson").repository()?;
            let existing = sqlx::query(
                r#"SELECT "OverviewFingerprint", "State"
                   FROM "AddonMediaListDetailJob" WHERE "AddonMediaListItemId" = ?"#,
            )
            .bind(item_id)
            .fetch_optional(&mut *tx)
            .await
            .repository()?;
            if let Some(existing) = existing {
                let previous: String = existing.try_get("OverviewFingerprint").repository()?;
                if previous == fingerprint {
                    // The overview identity is unchanged. Preserve completed,
                    // queued, in-flight, and checkpointed result state exactly
                    // so a refresh cannot repeat detail requests.
                    sqlx::query(
                        r#"UPDATE "AddonMediaListDetailJob"
                           SET "AddonMediaListId" = ?, "ProviderHost" = ?,
                               "OverviewRecordJson" = ?, "UpdatedAt" = ?
                           WHERE "AddonMediaListItemId" = ?"#,
                    )
                    .bind(list_id)
                    .bind(&provider_host)
                    .bind(&record_json)
                    .bind(&now)
                    .bind(item_id)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
                } else {
                    sqlx::query(
                        r#"UPDATE "AddonMediaListDetailJob"
                           SET "AddonMediaListId" = ?, "ProviderHost" = ?, "State" = ?,
                               "ManualPriority" = 0, "OverviewFingerprint" = ?,
                               "OverviewRecordJson" = ?, "AttemptCount" = 0,
                               "Outcome" = NULL, "ResultRecordJson" = NULL,
                               "Diagnostic" = NULL, "NextEligibleAt" = NULL,
                               "LeaseOwner" = NULL, "LeaseExpiresAt" = NULL,
                               "RequestedAt" = NULL, "StartedAt" = NULL,
                               "CompletedAt" = NULL, "UpdatedAt" = ?
                           WHERE "AddonMediaListItemId" = ?"#,
                    )
                    .bind(list_id)
                    .bind(&provider_host)
                    .bind(encode_enum(AddonMediaListDetailState::NotRequested))
                    .bind(&fingerprint)
                    .bind(&record_json)
                    .bind(&now)
                    .bind(item_id)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
                }
            } else {
                sqlx::query(
                    r#"INSERT INTO "AddonMediaListDetailJob" (
                        "AddonMediaListItemId", "AddonMediaListId", "ProviderHost", "State",
                        "ManualPriority", "OverviewFingerprint", "OverviewRecordJson", "UpdatedAt"
                       ) VALUES (?, ?, ?, ?, 0, ?, ?, ?)"#,
                )
                .bind(item_id)
                .bind(list_id)
                .bind(&provider_host)
                .bind(encode_enum(AddonMediaListDetailState::NotRequested))
                .bind(&fingerprint)
                .bind(&record_json)
                .bind(&now)
                .execute(&mut *tx)
                .await
                .repository()?;
            }
        }
        tx.commit().await.repository()?;
        Ok(())
    }

    async fn fail_import(&self, job_id: EntityId, diagnostic: &str, retryable: bool) -> Result<()> {
        let attempts: i32 = sqlx::query_scalar(
            r#"SELECT "AttemptCount" FROM "AddonMediaListImportJob" WHERE "Id" = ?"#,
        )
        .bind(job_id)
        .fetch_one(&self.pool)
        .await
        .repository()?;
        let attempts = attempts.saturating_add(1);
        let retry = retryable && attempts < MAX_IMPORT_ATTEMPTS;
        let now = Utc::now();
        let retry_at =
            retry.then(|| encode_datetime(now + Duration::seconds(import_retry_seconds(attempts))));
        sqlx::query(
            r#"UPDATE "AddonMediaListImportJob"
               SET "State" = ?, "AttemptCount" = ?, "Diagnostic" = ?,
                   "NextEligibleAt" = ?, "LeaseOwner" = NULL, "LeaseExpiresAt" = NULL,
                   "UpdatedAt" = ? WHERE "Id" = ?"#,
        )
        .bind(encode_enum(if retry {
            AddonMediaListImportState::Queued
        } else {
            AddonMediaListImportState::NeedsAttention
        }))
        .bind(attempts)
        .bind(bound_diagnostic(diagnostic))
        .bind(retry_at)
        .bind(encode_datetime(now))
        .bind(job_id)
        .execute(&self.pool)
        .await
        .repository()?;
        Ok(())
    }

    /// Persist a reversible automatic/manual-only policy change.
    ///
    /// # Errors
    ///
    /// Returns an error when the list does not exist or the policy and queued
    /// detail states cannot be updated atomically.
    pub async fn set_detail_policy(
        &self,
        list_id: EntityId,
        policy: AddonMediaListDetailPolicy,
    ) -> Result<()> {
        let now = encode_datetime(Utc::now());
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        let updated =
            sqlx::query(r#"UPDATE "AddonMediaList" SET "DetailImportPolicy" = ? WHERE "Id" = ?"#)
                .bind(encode_enum(policy))
                .bind(list_id)
                .execute(&mut *tx)
                .await
                .repository()?;
        if updated.rows_affected() != 1 {
            return Err(RepositoryError::NotFound {
                kind: "add-on media list",
                id: list_id,
            });
        }
        match policy {
            AddonMediaListDetailPolicy::Automatic => {
                sqlx::query(
                    r#"UPDATE "AddonMediaListDetailJob"
                       SET "State" = ?, "RequestedAt" = COALESCE("RequestedAt", ?),
                           "UpdatedAt" = ?
                       WHERE "AddonMediaListId" = ? AND "State" = ?
                         AND EXISTS (
                             SELECT 1 FROM "AddonMediaList"
                             WHERE "Id" = ? AND "DetailImportAuthorizedAt" IS NOT NULL
                         )"#,
                )
                .bind(encode_enum(AddonMediaListDetailState::Queued))
                .bind(&now)
                .bind(&now)
                .bind(list_id)
                .bind(encode_enum(AddonMediaListDetailState::NotRequested))
                .bind(list_id)
                .execute(&mut *tx)
                .await
                .repository()?;
            }
            AddonMediaListDetailPolicy::ManualOnly => {
                sqlx::query(
                    r#"UPDATE "AddonMediaListDetailJob"
                       SET "State" = ?, "UpdatedAt" = ?
                       WHERE "AddonMediaListId" = ? AND "ManualPriority" = 0
                         AND "State" IN (?, ?, ?)"#,
                )
                .bind(encode_enum(AddonMediaListDetailState::NotRequested))
                .bind(&now)
                .bind(list_id)
                .bind(encode_enum(AddonMediaListDetailState::Queued))
                .bind(encode_enum(AddonMediaListDetailState::WaitingForSlot))
                .bind(encode_enum(AddonMediaListDetailState::WaitingToRetry))
                .execute(&mut *tx)
                .await
                .repository()?;
            }
        }
        tx.commit().await.repository()?;
        Ok(())
    }

    /// Authorize and queue automatic work after the first successful Publish.
    ///
    /// # Errors
    ///
    /// Returns an error when publication authorization and eligible detail
    /// jobs cannot be persisted in one transaction.
    pub async fn authorize_after_publish(&self, list_id: EntityId) -> Result<()> {
        let now = encode_datetime(Utc::now());
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        sqlx::query(
            r#"UPDATE "AddonMediaList"
               SET "DetailImportAuthorizedAt" = COALESCE("DetailImportAuthorizedAt", ?)
               WHERE "Id" = ?"#,
        )
        .bind(&now)
        .bind(list_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        sqlx::query(
            r#"UPDATE "AddonMediaListDetailJob"
               SET "State" = ?, "RequestedAt" = COALESCE("RequestedAt", ?),
                   "UpdatedAt" = ?
               WHERE "AddonMediaListId" = ? AND "State" = ?
                 AND EXISTS (
                     SELECT 1 FROM "AddonMediaList"
                     WHERE "Id" = ? AND "DetailImportPolicy" = ?
                 )"#,
        )
        .bind(encode_enum(AddonMediaListDetailState::Queued))
        .bind(&now)
        .bind(&now)
        .bind(list_id)
        .bind(encode_enum(AddonMediaListDetailState::NotRequested))
        .bind(list_id)
        .bind(encode_enum(AddonMediaListDetailPolicy::Automatic))
        .execute(&mut *tx)
        .await
        .repository()?;
        tx.commit().await.repository()?;
        Ok(())
    }

    /// Queue one manual request without bypassing the shared host cooldown.
    ///
    /// # Errors
    ///
    /// Returns an error when the item has no durable detail job or its request
    /// state cannot be persisted.
    pub async fn request_item_details(&self, list_id: EntityId, item_id: EntityId) -> Result<()> {
        let now = encode_datetime(Utc::now());
        let updated = sqlx::query(
            r#"UPDATE "AddonMediaListDetailJob"
               SET "State" = CASE
                       WHEN "State" = ? THEN "State"
                       WHEN "ResultRecordJson" IS NOT NULL THEN ?
                       ELSE ?
                   END,
                   "ManualPriority" = 1, "RequestedAt" = ?, "NextEligibleAt" = NULL,
                   "Diagnostic" = NULL, "UpdatedAt" = ?
               WHERE "AddonMediaListId" = ? AND "AddonMediaListItemId" = ?"#,
        )
        .bind(encode_enum(AddonMediaListDetailState::Complete))
        .bind(encode_enum(AddonMediaListDetailState::Running))
        .bind(encode_enum(AddonMediaListDetailState::Queued))
        .bind(&now)
        .bind(&now)
        .bind(list_id)
        .bind(item_id)
        .execute(&self.pool)
        .await
        .repository()?;
        if updated.rows_affected() != 1 {
            return Err(RepositoryError::NotFound {
                kind: "add-on media-list item detail job",
                id: item_id,
            });
        }
        Ok(())
    }

    /// Apply one operator action to the newest overview generation.
    ///
    /// # Errors
    ///
    /// Returns an error when the list has no import generation or the action
    /// cannot be persisted.
    pub async fn control_import(
        &self,
        list_id: EntityId,
        action: AddonMediaListImportAction,
    ) -> Result<()> {
        let now = encode_datetime(Utc::now());
        let (state, cancellation) = match action {
            AddonMediaListImportAction::Pause => (AddonMediaListImportState::Paused, false),
            AddonMediaListImportAction::Resume | AddonMediaListImportAction::Retry => {
                (AddonMediaListImportState::Queued, false)
            }
            AddonMediaListImportAction::Cancel => (AddonMediaListImportState::Cancelled, true),
        };
        let updated = sqlx::query(
            r#"UPDATE "AddonMediaListImportJob"
               SET "State" = ?, "CancellationRequested" = ?, "AttemptCount" = 0,
                   "Diagnostic" = CASE WHEN ? THEN "Diagnostic" ELSE NULL END,
                   "NextEligibleAt" = NULL, "LeaseOwner" = NULL, "LeaseExpiresAt" = NULL,
                   "UpdatedAt" = ?
               WHERE "Id" = (
                   SELECT "Id" FROM "AddonMediaListImportJob"
                   WHERE "AddonMediaListId" = ? ORDER BY "Generation" DESC LIMIT 1
               )"#,
        )
        .bind(encode_enum(state))
        .bind(cancellation)
        .bind(cancellation)
        .bind(&now)
        .bind(list_id)
        .execute(&self.pool)
        .await
        .repository()?;
        if updated.rows_affected() != 1 {
            return Err(RepositoryError::NotFound {
                kind: "add-on media-list import",
                id: list_id,
            });
        }
        Ok(())
    }

    /// Process at most one eligible detail job.
    ///
    /// # Errors
    ///
    /// Returns an error when claiming, invoking, checkpointing, or merging the
    /// selected detail job fails.
    pub async fn process_one_detail(&self) -> Result<bool> {
        if self.apply_one_checkpointed_detail().await? {
            return Ok(true);
        }
        let Some(job) = self.claim_detail_job().await? else {
            return Ok(false);
        };
        let context = match self
            .provider
            .import_context(&job.addon_id, &job.source_url)
            .await
        {
            Ok(Some(context)) if context.record_capability == job.record_capability => context,
            Ok(_) => {
                self.finish_detail_failure(
                    &job,
                    "the add-on no longer supports this import contract",
                    false,
                    None,
                )
                .await?;
                return Ok(true);
            }
            Err(error) => {
                self.finish_detail_failure(&job, &error.to_string(), true, None)
                    .await?;
                return Ok(true);
            }
        };
        let record = match serde_json::from_str::<AddonMediaListRecord>(&job.overview_record_json) {
            Ok(record) => record,
            Err(error) => {
                self.finish_detail_failure(&job, &error.to_string(), false, None)
                    .await?;
                return Ok(true);
            }
        };
        match self
            .provider
            .enrich(
                &job.addon_id,
                &context,
                &job.provider_id,
                &job.overview_fingerprint,
                record,
            )
            .await
        {
            Ok(result) => match result.outcome {
                MediaListEnrichOutcomeKind::Complete | MediaListEnrichOutcomeKind::Unavailable => {
                    let Some(item) = result.item else {
                        self.finish_detail_failure(
                            &job,
                            "complete result omitted its item",
                            false,
                            None,
                        )
                        .await?;
                        return Ok(true);
                    };
                    self.checkpoint_detail_result(
                        &job,
                        result.outcome,
                        result.code.as_deref(),
                        &item,
                    )
                    .await?;
                }
                MediaListEnrichOutcomeKind::TransientFailure => {
                    let diagnostic = result
                        .message
                        .as_deref()
                        .or(result.code.as_deref())
                        .unwrap_or("the provider requested a retry");
                    self.finish_detail_failure(&job, diagnostic, true, result.retry_after_seconds)
                        .await?;
                }
                MediaListEnrichOutcomeKind::PermanentFailure => {
                    let diagnostic = result
                        .message
                        .as_deref()
                        .or(result.code.as_deref())
                        .unwrap_or("the provider rejected this item");
                    self.finish_detail_failure(&job, diagnostic, false, None)
                        .await?;
                }
            },
            Err(error) => {
                self.finish_detail_failure(&job, &error.to_string(), true, None)
                    .await?;
            }
        }
        Ok(true)
    }

    /// Apply a validated provider result that was checkpointed before any
    /// local projection work. A crash after the provider request therefore
    /// retries only the idempotent merge and never re-fetches the programme.
    #[allow(clippy::too_many_lines)] // Claim, merge, and terminal state form one crash boundary.
    async fn apply_one_checkpointed_detail(&self) -> Result<bool> {
        let now = Utc::now();
        let now_text = encode_datetime(now);
        let lease_owner = Uuid::new_v4().to_string();
        let lease_expires = encode_datetime(now + Duration::seconds(DETAIL_LEASE_SECONDS));
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        let row = sqlx::query(
            r#"SELECT "detail"."AddonMediaListItemId", "detail"."AddonMediaListId",
                      "detail"."ProviderHost", "detail"."OverviewFingerprint",
                      "detail"."OverviewRecordJson", "detail"."AttemptCount",
                      "detail"."ResultRecordJson", "detail"."Outcome",
                      "item"."ProviderId", "list"."AddonId",
                      "list"."SourceUrl", "import"."RecordCapability"
               FROM "AddonMediaListDetailJob" AS "detail"
               JOIN "AddonMediaListItem" AS "item"
                 ON "item"."Id" = "detail"."AddonMediaListItemId"
               JOIN "AddonMediaList" AS "list"
                 ON "list"."Id" = "detail"."AddonMediaListId"
               JOIN "AddonMediaListImportJob" AS "import" ON "import"."Id" = (
                   SELECT "latest"."Id" FROM "AddonMediaListImportJob" AS "latest"
                   WHERE "latest"."AddonMediaListId" = "detail"."AddonMediaListId"
                     AND "latest"."State" = ?
                   ORDER BY "latest"."Generation" DESC LIMIT 1
               )
               WHERE "detail"."State" = ?
                 AND "detail"."ResultRecordJson" IS NOT NULL
                 AND ("detail"."LeaseExpiresAt" IS NULL OR "detail"."LeaseExpiresAt" <= ?)
               ORDER BY "detail"."UpdatedAt", "detail"."AddonMediaListItemId" LIMIT 1"#,
        )
        .bind(encode_enum(AddonMediaListImportState::Ready))
        .bind(encode_enum(AddonMediaListDetailState::Running))
        .bind(&now_text)
        .fetch_optional(&mut *tx)
        .await
        .repository()?;
        let Some(row) = row else {
            tx.commit().await.repository()?;
            return Ok(false);
        };
        let item_id: EntityId = row.try_get("AddonMediaListItemId").repository()?;
        let claimed = sqlx::query(
            r#"UPDATE "AddonMediaListDetailJob"
               SET "LeaseOwner" = ?, "LeaseExpiresAt" = ?, "UpdatedAt" = ?
               WHERE "AddonMediaListItemId" = ? AND "State" = ?
                 AND "ResultRecordJson" IS NOT NULL
                 AND ("LeaseExpiresAt" IS NULL OR "LeaseExpiresAt" <= ?)"#,
        )
        .bind(&lease_owner)
        .bind(&lease_expires)
        .bind(&now_text)
        .bind(item_id)
        .bind(encode_enum(AddonMediaListDetailState::Running))
        .bind(&now_text)
        .execute(&mut *tx)
        .await
        .repository()?;
        if claimed.rows_affected() != 1 {
            tx.rollback().await.repository()?;
            return Ok(false);
        }
        let job = DetailJob {
            item_id,
            list_id: row.try_get("AddonMediaListId").repository()?,
            addon_id: row.try_get("AddonId").repository()?,
            source_url: row.try_get("SourceUrl").repository()?,
            record_capability: row.try_get("RecordCapability").repository()?,
            provider_id: row.try_get("ProviderId").repository()?,
            provider_host: row.try_get("ProviderHost").repository()?,
            overview_fingerprint: row.try_get("OverviewFingerprint").repository()?,
            overview_record_json: row.try_get("OverviewRecordJson").repository()?,
            attempt_count: row.try_get("AttemptCount").repository()?,
        };
        let result_record_json: String = row.try_get("ResultRecordJson").repository()?;
        let outcome: String = row.try_get("Outcome").repository()?;
        tx.commit().await.repository()?;

        let result = serde_json::from_str::<AddonMediaListRecord>(&result_record_json)
            .map_err(|error| RepositoryError::Decode {
                kind: "checkpointed media-list enrichment",
                value: error.to_string(),
            })
            .and_then(|record| Self::decode_enriched_item(&job, &record));
        let result = match result {
            Ok(item) => {
                AddonMediaListSynchronizer::new(self.pool.clone())
                    .merge_detail_item(job.list_id, &item)
                    .await
            }
            Err(error) => Err(error),
        };
        match result {
            Ok(()) => {
                let state = if outcome == "unavailable" {
                    AddonMediaListDetailState::Skipped
                } else {
                    AddonMediaListDetailState::Complete
                };
                self.finish_checkpointed_detail(&job, state).await?;
            }
            Err(error) => {
                sqlx::query(
                    r#"UPDATE "AddonMediaListDetailJob"
                       SET "State" = ?, "Diagnostic" = ?, "LeaseOwner" = NULL,
                           "LeaseExpiresAt" = NULL, "UpdatedAt" = ?
                       WHERE "AddonMediaListItemId" = ?"#,
                )
                .bind(encode_enum(AddonMediaListDetailState::Failed))
                .bind(bound_diagnostic(&error.to_string()))
                .bind(encode_datetime(Utc::now()))
                .bind(job.item_id)
                .execute(&self.pool)
                .await
                .repository()?;
            }
        }
        Ok(true)
    }

    fn decode_enriched_item(
        job: &DetailJob,
        item: &AddonMediaListRecord,
    ) -> Result<ersatzrs_core::addon_media_list::EnumeratedAddonMediaListItem> {
        let header = AddonMediaListRecord::list("detail/merge", "Detail merge");
        let mut output = serde_json::to_vec(&header).map_err(|error| RepositoryError::Decode {
            kind: "media-list enrichment",
            value: error.to_string(),
        })?;
        output.push(b'\n');
        output.extend(
            serde_json::to_vec(&item).map_err(|error| RepositoryError::Decode {
                kind: "media-list enrichment",
                value: error.to_string(),
            })?,
        );
        output.push(b'\n');
        let supports_v3 = matches!(
            job.record_capability.as_str(),
            CAPABILITY_MEDIA_LIST_LIST_V3
                | CAPABILITY_MEDIA_LIST_LIST_V4
                | CAPABILITY_MEDIA_LIST_LIST_V5
        );
        let supports_v4 = matches!(
            job.record_capability.as_str(),
            CAPABILITY_MEDIA_LIST_LIST_V4 | CAPABILITY_MEDIA_LIST_LIST_V5
        );
        let mut decoded = decode_media_list_output_for_capability(
            &output,
            supports_v3,
            supports_v4,
            job.record_capability == CAPABILITY_MEDIA_LIST_LIST_V5,
        )?;
        decoded.items.pop().ok_or_else(|| RepositoryError::Decode {
            kind: "media-list enrichment",
            value: "result omitted its item".to_owned(),
        })
    }

    #[allow(clippy::too_many_lines)] // Backend-portable compare-and-swap claim and host lease stay adjacent.
    async fn claim_detail_job(&self) -> Result<Option<DetailJob>> {
        let now = Utc::now();
        let now_text = encode_datetime(now);
        let lease_owner = Uuid::new_v4().to_string();
        let lease_expires = encode_datetime(now + Duration::seconds(DETAIL_LEASE_SECONDS));
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        // A crashed process never owns an item forever.
        sqlx::query(
            r#"UPDATE "AddonMediaListDetailJob"
               SET "State" = ?, "LeaseOwner" = NULL, "LeaseExpiresAt" = NULL,
                   "UpdatedAt" = ?
               WHERE "State" = ? AND "ResultRecordJson" IS NULL
                 AND "LeaseExpiresAt" <= ?"#,
        )
        .bind(encode_enum(AddonMediaListDetailState::WaitingToRetry))
        .bind(&now_text)
        .bind(encode_enum(AddonMediaListDetailState::Running))
        .bind(&now_text)
        .execute(&mut *tx)
        .await
        .repository()?;
        let candidates = sqlx::query(
            r#"SELECT "detail"."AddonMediaListItemId", "detail"."AddonMediaListId",
                      "detail"."ProviderHost", "detail"."OverviewFingerprint",
                      "detail"."OverviewRecordJson", "detail"."AttemptCount",
                      "item"."ProviderId", "list"."AddonId",
                      "list"."SourceUrl", "import"."RecordCapability"
               FROM "AddonMediaListDetailJob" AS "detail"
               JOIN "AddonMediaListItem" AS "item"
                 ON "item"."Id" = "detail"."AddonMediaListItemId"
               JOIN "AddonMediaList" AS "list" ON "list"."Id" = "detail"."AddonMediaListId"
               JOIN "AddonMediaListImportJob" AS "import" ON "import"."Id" = (
                   SELECT "latest"."Id" FROM "AddonMediaListImportJob" AS "latest"
                   WHERE "latest"."AddonMediaListId" = "detail"."AddonMediaListId"
                     AND "latest"."State" = ?
                   ORDER BY "latest"."Generation" DESC LIMIT 1
               )
               WHERE "detail"."State" IN (?, ?, ?)
                 AND "detail"."ResultRecordJson" IS NULL
                 AND ("detail"."NextEligibleAt" IS NULL OR "detail"."NextEligibleAt" <= ?)
                 AND ("detail"."ManualPriority" = 1 OR (
                     "list"."DetailImportPolicy" = ?
                     AND "list"."DetailImportAuthorizedAt" IS NOT NULL
                 ))
               ORDER BY "detail"."ManualPriority" DESC,
                        COALESCE("detail"."RequestedAt", "detail"."UpdatedAt"),
                        "detail"."AddonMediaListItemId"
               LIMIT 32"#,
        )
        .bind(encode_enum(AddonMediaListImportState::Ready))
        .bind(encode_enum(AddonMediaListDetailState::Queued))
        .bind(encode_enum(AddonMediaListDetailState::WaitingForSlot))
        .bind(encode_enum(AddonMediaListDetailState::WaitingToRetry))
        .bind(&now_text)
        .bind(encode_enum(AddonMediaListDetailPolicy::Automatic))
        .fetch_all(&mut *tx)
        .await
        .repository()?;
        for row in candidates {
            let host: String = row.try_get("ProviderHost").repository()?;
            let cooldown = sqlx::query(
                r#"SELECT "NextEligibleAt", "LeaseExpiresAt"
                   FROM "AddonProviderHostCooldown" WHERE "ProviderHost" = ?"#,
            )
            .bind(&host)
            .fetch_optional(&mut *tx)
            .await
            .repository()?;
            if let Some(cooldown) = cooldown {
                let next: String = cooldown.try_get("NextEligibleAt").repository()?;
                let lease: Option<String> = cooldown.try_get("LeaseExpiresAt").repository()?;
                if next > now_text
                    || lease
                        .as_deref()
                        .is_some_and(|value| value > now_text.as_str())
                {
                    sqlx::query(
                        r#"UPDATE "AddonMediaListDetailJob" SET "State" = ?, "UpdatedAt" = ?
                           WHERE "AddonMediaListItemId" = ? AND "State" IN (?, ?)"#,
                    )
                    .bind(encode_enum(AddonMediaListDetailState::WaitingForSlot))
                    .bind(&now_text)
                    .bind(
                        row.try_get::<EntityId, _>("AddonMediaListItemId")
                            .repository()?,
                    )
                    .bind(encode_enum(AddonMediaListDetailState::Queued))
                    .bind(encode_enum(AddonMediaListDetailState::WaitingToRetry))
                    .execute(&mut *tx)
                    .await
                    .repository()?;
                    continue;
                }
                let locked = sqlx::query(
                    r#"UPDATE "AddonProviderHostCooldown"
                       SET "LeaseOwner" = ?, "LeaseExpiresAt" = ?, "UpdatedAt" = ?
                       WHERE "ProviderHost" = ? AND "NextEligibleAt" <= ?
                         AND ("LeaseExpiresAt" IS NULL OR "LeaseExpiresAt" <= ?)"#,
                )
                .bind(&lease_owner)
                .bind(&lease_expires)
                .bind(&now_text)
                .bind(&host)
                .bind(&now_text)
                .bind(&now_text)
                .execute(&mut *tx)
                .await
                .repository()?;
                if locked.rows_affected() != 1 {
                    continue;
                }
            } else {
                sqlx::query(
                    r#"INSERT INTO "AddonProviderHostCooldown" (
                        "ProviderHost", "NextEligibleAt", "SampledDelaySeconds",
                        "LastAttemptAt", "LeaseOwner", "LeaseExpiresAt", "UpdatedAt"
                       ) VALUES (?, ?, ?, ?, ?, ?, ?)"#,
                )
                .bind(&host)
                .bind(&now_text)
                .bind(i64::from(
                    ersatzrs_domain::DEFAULT_ADDON_DETAIL_WAIT_MINIMUM_SECONDS,
                ))
                .bind(&now_text)
                .bind(&lease_owner)
                .bind(&lease_expires)
                .bind(&now_text)
                .execute(&mut *tx)
                .await
                .repository()?;
            }
            let item_id: EntityId = row.try_get("AddonMediaListItemId").repository()?;
            let claimed = sqlx::query(
                r#"UPDATE "AddonMediaListDetailJob"
                   SET "State" = ?, "LeaseOwner" = ?, "LeaseExpiresAt" = ?,
                       "StartedAt" = ?, "UpdatedAt" = ?
                   WHERE "AddonMediaListItemId" = ? AND "State" IN (?, ?, ?)"#,
            )
            .bind(encode_enum(AddonMediaListDetailState::Running))
            .bind(&lease_owner)
            .bind(&lease_expires)
            .bind(&now_text)
            .bind(&now_text)
            .bind(item_id)
            .bind(encode_enum(AddonMediaListDetailState::Queued))
            .bind(encode_enum(AddonMediaListDetailState::WaitingForSlot))
            .bind(encode_enum(AddonMediaListDetailState::WaitingToRetry))
            .execute(&mut *tx)
            .await
            .repository()?;
            if claimed.rows_affected() != 1 {
                continue;
            }
            let job = DetailJob {
                item_id,
                list_id: row.try_get("AddonMediaListId").repository()?,
                addon_id: row.try_get("AddonId").repository()?,
                source_url: row.try_get("SourceUrl").repository()?,
                record_capability: row.try_get("RecordCapability").repository()?,
                provider_id: row.try_get("ProviderId").repository()?,
                provider_host: host,
                overview_fingerprint: row.try_get("OverviewFingerprint").repository()?,
                overview_record_json: row.try_get("OverviewRecordJson").repository()?,
                attempt_count: row.try_get("AttemptCount").repository()?,
            };
            tx.commit().await.repository()?;
            return Ok(Some(job));
        }
        tx.commit().await.repository()?;
        Ok(None)
    }

    async fn checkpoint_detail_result(
        &self,
        job: &DetailJob,
        outcome: MediaListEnrichOutcomeKind,
        code: Option<&str>,
        item: &AddonMediaListRecord,
    ) -> Result<()> {
        let now = Utc::now();
        let range = load_detail_wait_range(&self.pool).await?;
        let delay = sample_provider_delay_seconds(Uuid::new_v4().as_bytes(), range);
        let next = encode_datetime(now + Duration::seconds(i64::from(delay)));
        let now_text = encode_datetime(now);
        let record_json = serde_json::to_string(item).map_err(|error| RepositoryError::Decode {
            kind: "media-list enrichment checkpoint",
            value: error.to_string(),
        })?;
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        sqlx::query(
            r#"UPDATE "AddonMediaListDetailJob"
               SET "State" = ?, "AttemptCount" = ?, "Outcome" = ?,
                   "ResultRecordJson" = ?, "Diagnostic" = ?, "NextEligibleAt" = NULL,
                   "LeaseOwner" = NULL, "LeaseExpiresAt" = NULL, "UpdatedAt" = ?
               WHERE "AddonMediaListItemId" = ?"#,
        )
        .bind(encode_enum(AddonMediaListDetailState::Running))
        .bind(job.attempt_count.saturating_add(1))
        .bind(if outcome == MediaListEnrichOutcomeKind::Unavailable {
            "unavailable"
        } else {
            "complete"
        })
        .bind(record_json)
        .bind(code)
        .bind(&now_text)
        .bind(job.item_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        update_detail_progress(&mut tx, job.list_id, &now_text).await?;
        update_host_cooldown(&mut tx, &job.provider_host, &now_text, &next, delay).await?;
        tx.commit().await.repository()?;
        Ok(())
    }

    async fn finish_checkpointed_detail(
        &self,
        job: &DetailJob,
        state: AddonMediaListDetailState,
    ) -> Result<()> {
        let now = encode_datetime(Utc::now());
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        sqlx::query(
            r#"UPDATE "AddonMediaListDetailJob"
               SET "State" = ?, "ManualPriority" = 0, "Diagnostic" = NULL,
                   "NextEligibleAt" = NULL, "LeaseOwner" = NULL,
                   "LeaseExpiresAt" = NULL, "CompletedAt" = ?, "UpdatedAt" = ?
               WHERE "AddonMediaListItemId" = ? AND "ResultRecordJson" IS NOT NULL"#,
        )
        .bind(encode_enum(state))
        .bind(&now)
        .bind(&now)
        .bind(job.item_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        update_detail_progress(&mut tx, job.list_id, &now).await?;
        tx.commit().await.repository()?;
        Ok(())
    }

    async fn finish_detail_failure(
        &self,
        job: &DetailJob,
        diagnostic: &str,
        retryable: bool,
        retry_after_seconds: Option<u32>,
    ) -> Result<()> {
        let now = Utc::now();
        let attempts = job.attempt_count.saturating_add(1);
        let retry = retryable && attempts < MAX_DETAIL_ATTEMPTS;
        let range = load_detail_wait_range(&self.pool).await?;
        let delay = sample_provider_delay_seconds(Uuid::new_v4().as_bytes(), range);
        let cooldown_at = now + Duration::seconds(i64::from(delay));
        let backoff = detail_retry_seconds(attempts);
        let retry_seconds = i64::from(retry_after_seconds.unwrap_or(0)).max(backoff);
        let retry_at = now + Duration::seconds(retry_seconds);
        let next_host = if retry_at > cooldown_at {
            retry_at
        } else {
            cooldown_at
        };
        let now_text = encode_datetime(now);
        let next_text = encode_datetime(next_host);
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        sqlx::query(
            r#"UPDATE "AddonMediaListDetailJob"
               SET "State" = ?, "AttemptCount" = ?, "Outcome" = ?, "Diagnostic" = ?,
                   "ResultRecordJson" = NULL, "NextEligibleAt" = ?,
                   "LeaseOwner" = NULL, "LeaseExpiresAt" = NULL,
                   "UpdatedAt" = ? WHERE "AddonMediaListItemId" = ?"#,
        )
        .bind(encode_enum(if retry {
            AddonMediaListDetailState::WaitingToRetry
        } else {
            AddonMediaListDetailState::Failed
        }))
        .bind(attempts)
        .bind(if retry { "transient_failure" } else { "failed" })
        .bind(bound_diagnostic(diagnostic))
        .bind(retry.then_some(next_text.as_str()))
        .bind(&now_text)
        .bind(job.item_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        update_detail_progress(&mut tx, job.list_id, &now_text).await?;
        update_host_cooldown(&mut tx, &job.provider_host, &now_text, &next_text, delay).await?;
        tx.commit().await.repository()?;
        Ok(())
    }

    /// Load one list's newest durable progress snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error when the newest generation or its detail counters
    /// cannot be loaded or decoded.
    pub async fn progress(
        &self,
        list_id: EntityId,
    ) -> Result<Option<AddonMediaListImportProgress>> {
        let job = sqlx::query(
            r#"SELECT "State", "CreatedAt", "DiscoveredCount",
                      CAST("TotalHint" AS SIGNED) AS "TotalHint",
                      "LastProgressAt", "Diagnostic"
               FROM "AddonMediaListImportJob"
               WHERE "AddonMediaListId" = ? ORDER BY "Generation" DESC LIMIT 1"#,
        )
        .bind(list_id)
        .fetch_optional(&self.pool)
        .await
        .repository()?;
        let Some(job) = job else {
            return Ok(None);
        };
        let counts = sqlx::query(
            r#"SELECT CAST(COUNT(*) AS SIGNED) AS "Total",
                      CAST(COALESCE(SUM(CASE WHEN "State" = ? THEN 1 ELSE 0 END), 0) AS SIGNED)
                          AS "Complete",
                      CAST(COALESCE(SUM(CASE WHEN "State" = ? THEN 1 ELSE 0 END), 0) AS SIGNED)
                          AS "Skipped",
                      CAST(COALESCE(SUM(CASE WHEN "State" = ? THEN 1 ELSE 0 END), 0) AS SIGNED)
                          AS "Failed",
                      CAST(COALESCE(SUM(CASE WHEN "State" IN (?, ?, ?, ?) THEN 1 ELSE 0 END), 0) AS SIGNED)
                          AS "Pending"
               FROM "AddonMediaListDetailJob" WHERE "AddonMediaListId" = ?"#,
        )
        .bind(encode_enum(AddonMediaListDetailState::Complete))
        .bind(encode_enum(AddonMediaListDetailState::Skipped))
        .bind(encode_enum(AddonMediaListDetailState::Failed))
        .bind(encode_enum(AddonMediaListDetailState::Queued))
        .bind(encode_enum(AddonMediaListDetailState::WaitingForSlot))
        .bind(encode_enum(AddonMediaListDetailState::Running))
        .bind(encode_enum(AddonMediaListDetailState::WaitingToRetry))
        .bind(list_id)
        .fetch_one(&self.pool)
        .await
        .repository()?;
        let running_title = sqlx::query_scalar::<_, String>(
            r#"SELECT "item"."DisplayTitle"
               FROM "AddonMediaListDetailJob" AS "detail"
               JOIN "AddonMediaListItem" AS "item"
                 ON "item"."Id" = "detail"."AddonMediaListItemId"
               WHERE "detail"."AddonMediaListId" = ? AND "detail"."State" = ?
               ORDER BY "detail"."StartedAt" DESC LIMIT 1"#,
        )
        .bind(list_id)
        .bind(encode_enum(AddonMediaListDetailState::Running))
        .fetch_optional(&self.pool)
        .await
        .repository()?;
        Ok(Some(AddonMediaListImportProgress {
            state: decode_enum_column("AddonMediaListImportState", &job, "State")?,
            started_at: job.try_get("CreatedAt").repository()?,
            discovered_count: u64::try_from(job.try_get::<i32, _>("DiscoveredCount").repository()?)
                .unwrap_or_default(),
            total_hint: job
                .try_get::<Option<i64>, _>("TotalHint")
                .repository()?
                .and_then(|value| u64::try_from(value).ok()),
            detail_total: nonnegative_count(&counts, "Total")?,
            detail_complete: nonnegative_count(&counts, "Complete")?,
            detail_skipped: nonnegative_count(&counts, "Skipped")?,
            detail_failed: nonnegative_count(&counts, "Failed")?,
            detail_pending: nonnegative_count(&counts, "Pending")?,
            current_title: running_title,
            last_progress_at: job.try_get("LastProgressAt").repository()?,
            diagnostic: job.try_get("Diagnostic").repository()?,
        }))
    }
}

async fn update_detail_progress(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    list_id: EntityId,
    now: &str,
) -> Result<()> {
    sqlx::query(
        r#"UPDATE "AddonMediaListImportJob" SET "LastProgressAt" = ?
           WHERE "Id" = (
               SELECT "Id" FROM "AddonMediaListImportJob"
               WHERE "AddonMediaListId" = ? ORDER BY "Generation" DESC LIMIT 1
           )"#,
    )
    .bind(now)
    .bind(list_id)
    .execute(&mut **tx)
    .await
    .repository()?;
    Ok(())
}

#[derive(Debug)]
struct ImportJob {
    id: EntityId,
    list_id: EntityId,
    addon_id: String,
    source_url: String,
    record_capability: String,
    cursor: Option<String>,
    verification_pass: i32,
    expected_identity_hash: Option<String>,
    overview_complete: bool,
}

#[derive(Debug)]
struct DetailJob {
    item_id: EntityId,
    list_id: EntityId,
    addon_id: String,
    source_url: String,
    record_capability: String,
    provider_id: String,
    provider_host: String,
    overview_fingerprint: String,
    overview_record_json: String,
    attempt_count: i32,
}

async fn update_host_cooldown(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    provider_host: &str,
    now: &str,
    next: &str,
    delay: u32,
) -> Result<()> {
    let updated = sqlx::query(
        r#"UPDATE "AddonProviderHostCooldown"
           SET "NextEligibleAt" = CASE
                   WHEN "NextEligibleAt" > ? THEN "NextEligibleAt" ELSE ? END,
               "SampledDelaySeconds" = ?, "LastAttemptAt" = ?,
               "LeaseOwner" = NULL, "LeaseExpiresAt" = NULL, "UpdatedAt" = ?
           WHERE "ProviderHost" = ?"#,
    )
    .bind(next)
    .bind(next)
    .bind(i64::from(delay))
    .bind(now)
    .bind(now)
    .bind(provider_host)
    .execute(&mut **tx)
    .await
    .repository()?;
    if updated.rows_affected() != 1 {
        return Err(RepositoryError::Conflict(
            "the provider-host detail lease disappeared".to_owned(),
        ));
    }
    Ok(())
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

fn sha256_hex(bytes: &[u8]) -> String {
    encode_hex(&Sha256::digest(bytes))
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        write!(encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

fn import_retry_seconds(attempt: i32) -> i64 {
    5_i64.saturating_mul(1_i64 << u32::try_from(attempt.clamp(0, 8)).unwrap_or(0))
}

fn detail_retry_seconds(attempt: i32) -> i64 {
    15_i64
        .saturating_mul(1_i64 << u32::try_from(attempt.clamp(0, 8)).unwrap_or(0))
        .min(3_600)
}

pub(crate) async fn load_detail_wait_range(pool: &DbPool) -> Result<AddonMediaListDetailWaitRange> {
    let rows = sqlx::query(r#"SELECT "Key", "Value" FROM "ConfigElement" WHERE "Key" IN (?, ?)"#)
        .bind(ersatzrs_domain::CONFIG_KEY_ADDON_DETAIL_WAIT_MINIMUM_SECONDS)
        .bind(ersatzrs_domain::CONFIG_KEY_ADDON_DETAIL_WAIT_MAXIMUM_SECONDS)
        .fetch_all(pool)
        .await
        .repository()?;
    let mut range = AddonMediaListDetailWaitRange::default();
    for row in rows {
        let key: String = row.try_get("Key").repository()?;
        let raw: String = row.try_get("Value").repository()?;
        let value = raw.parse::<u32>().map_err(|_| RepositoryError::Decode {
            kind: "add-on detail wait setting",
            value: format!("{key}={raw}"),
        })?;
        match key.as_str() {
            ersatzrs_domain::CONFIG_KEY_ADDON_DETAIL_WAIT_MINIMUM_SECONDS => {
                range.minimum_seconds = value;
            }
            ersatzrs_domain::CONFIG_KEY_ADDON_DETAIL_WAIT_MAXIMUM_SECONDS => {
                range.maximum_seconds = value;
            }
            _ => {}
        }
    }
    AddonMediaListDetailWaitRange::new(range.minimum_seconds, range.maximum_seconds).ok_or_else(
        || RepositoryError::Decode {
            kind: "add-on detail wait settings",
            value: format!(
                "minimum={}, maximum={}",
                range.minimum_seconds, range.maximum_seconds
            ),
        },
    )
}

fn sample_provider_delay_seconds(seed: &[u8], range: AddonMediaListDetailWaitRange) -> u32 {
    let spread = u64::from(range.maximum_seconds) - u64::from(range.minimum_seconds) + 1;
    let value = seed.iter().fold(0_u64, |accumulator, byte| {
        accumulator.wrapping_mul(257) ^ u64::from(*byte)
    });
    range.minimum_seconds + u32::try_from(value % spread).unwrap_or_default()
}

fn bound_diagnostic(value: &str) -> String {
    if value.len() <= 2_048 {
        return value.to_owned();
    }
    let mut end = 2_048;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

fn nonnegative_count(row: &crate::db::DbRow, column: &'static str) -> Result<u64> {
    let value = row
        .try_get::<Option<i64>, _>(column)
        .repository()?
        .unwrap_or(0);
    u64::try_from(value).map_err(|_| RepositoryError::Decode {
        kind: "add-on import count",
        value: value.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fair_use_delay_is_always_inclusive_two_through_sixteen() {
        let range = AddonMediaListDetailWaitRange::default();
        let values = (0_u16..=u16::MAX)
            .map(|value| sample_provider_delay_seconds(&value.to_be_bytes(), range))
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(values.first().copied(), Some(2));
        assert_eq!(values.last().copied(), Some(16));
        assert_eq!(values.len(), 15);
    }

    #[test]
    fn fair_use_delay_uses_a_non_default_inclusive_range() {
        let range = AddonMediaListDetailWaitRange::new(7, 11).expect("valid test range");
        let values = (0_u16..=u16::MAX)
            .map(|value| sample_provider_delay_seconds(&value.to_be_bytes(), range))
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(values.into_iter().collect::<Vec<_>>(), [7, 8, 9, 10, 11]);
    }

    #[test]
    fn retry_backoff_never_shortens_the_provider_cooldown() {
        assert!(
            detail_retry_seconds(1)
                >= i64::from(ersatzrs_domain::DEFAULT_ADDON_DETAIL_WAIT_MAXIMUM_SECONDS)
        );
        assert!(detail_retry_seconds(20) <= 3_600);
    }

    mod sqlite {
        use super::*;
        use ersatzrs_core::repositories::AddonMediaListRepository as _;

        use crate::repositories::SqliteAddonMediaListRepository;

        async fn fixture() -> (tempfile::TempDir, DbPool, AddonMediaListImportCoordinator) {
            let temporary = tempfile::tempdir().expect("temporary import database");
            let database_url = format!("sqlite:{}", temporary.path().join("import.db").display());
            let pool = crate::db::init(&database_url)
                .await
                .expect("initialize import database");
            sqlx::query(
                r#"INSERT INTO "AddonInstallation" (
                       "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
                       "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt"
                   ) VALUES (?, '1.0.0', 1, 'package', 'content', '{}', ?, ?, ?)"#,
            )
            .bind("org.example.fixture")
            .bind(
                temporary
                    .path()
                    .join("addon")
                    .to_string_lossy()
                    .into_owned(),
            )
            .bind(encode_datetime(Utc::now()))
            .bind(encode_datetime(Utc::now()))
            .execute(&pool)
            .await
            .expect("fixture add-on installation");
            let coordinator = AddonMediaListImportCoordinator::new(pool.clone());
            (temporary, pool, coordinator)
        }

        fn context(source_url: &str) -> InstalledMediaListImportContext {
            InstalledMediaListImportContext {
                source_url: source_url.to_owned(),
                record_capability: CAPABILITY_MEDIA_LIST_LIST_V5.to_owned(),
            }
        }

        fn list_record(provider_id: &str) -> AddonMediaListRecord {
            serde_json::from_value(serde_json::json!({
                "record_type": "list",
                "provider_id": provider_id,
                "name": "Fixture programmes",
                "description": "Overview description"
            }))
            .expect("list record")
        }

        fn item_record(provider_id: &str, rank: i32, plot: &str) -> AddonMediaListRecord {
            serde_json::from_value(serde_json::json!({
                "record_type": "item",
                "provider_id": provider_id,
                "rank": rank,
                "display_title": format!("Programme {}", rank + 1),
                "title": format!("Programme {}", rank + 1),
                "kind": "episode",
                "source_url": format!("https://media.example.test/{provider_id}"),
                "metadata": {
                    "plot": plot,
                    "release_date": format!("2026-08-{:02}", rank.rem_euclid(28) + 1),
                    "artwork": [{
                        "url": format!("https://media.example.test/{provider_id}.jpg"),
                        "role": "thumb"
                    }]
                }
            }))
            .expect("item record")
        }

        async fn queue(
            coordinator: &AddonMediaListImportCoordinator,
            source_url: &str,
            policy: AddonMediaListDetailPolicy,
        ) -> EntityId {
            coordinator
                .queue_import(
                    "org.example.fixture",
                    &context(source_url),
                    None,
                    AddonMediaListContentKind::TelevisionEpisode,
                    policy,
                    None,
                )
                .await
                .expect("queue overview import")
        }

        async fn complete_single_page(
            coordinator: &AddonMediaListImportCoordinator,
            source_url: &str,
            policy: AddonMediaListDetailPolicy,
            provider_list_id: &str,
            item_ids: &[&str],
        ) -> EntityId {
            let records = std::iter::once(list_record(provider_list_id))
                .chain(item_ids.iter().enumerate().map(|(rank, provider_id)| {
                    item_record(
                        provider_id,
                        i32::try_from(rank).expect("fixture rank"),
                        "Overview plot",
                    )
                }))
                .collect::<Vec<_>>();
            complete_overview(coordinator, source_url, policy, records, "revision-1").await
        }

        async fn complete_overview(
            coordinator: &AddonMediaListImportCoordinator,
            source_url: &str,
            policy: AddonMediaListDetailPolicy,
            records: Vec<AddonMediaListRecord>,
            revision_token: &str,
        ) -> EntityId {
            let item_count = records
                .iter()
                .filter(|record| matches!(record, AddonMediaListRecord::Item { .. }))
                .count();
            let list_id = queue(coordinator, source_url, policy).await;
            let job = coordinator
                .claim_import_job()
                .await
                .expect("claim overview")
                .expect("queued overview");
            coordinator
                .commit_import_page(
                    &job,
                    MediaListDiscoveryPage {
                        complete: true,
                        next_cursor: None,
                        total_hint: Some(u32::try_from(item_count).expect("fixture count")),
                        revision_token: Some(revision_token.to_owned()),
                        records,
                    },
                )
                .await
                .expect("commit complete overview");
            list_id
        }

        fn beeldengeluid_item(
            provider_id: &str,
            rank: i32,
            title: &str,
            release_date: &str,
            unavailable: bool,
        ) -> AddonMediaListRecord {
            let mut value = serde_json::json!({
                "record_type": "item",
                "provider_id": provider_id,
                "rank": rank,
                "display_title": title,
                "title": title,
                "kind": "episode",
                "source_url": format!(
                    "https://schatkamer.beeldengeluid.nl/serie/2101608030022428031/strip-en-cartoontekenen/aflevering/{}",
                    provider_id.trim_start_matches("episode:")
                ),
                "metadata": {
                    "plot": format!("Overview for {title}"),
                    "release_date": release_date,
                    "guids": [format!(
                        "beeldengeluid://{}",
                        provider_id.trim_start_matches("episode:")
                    )]
                }
            });
            if unavailable {
                value["availability"] = serde_json::json!("unavailable");
                value["availability_reason"] = serde_json::json!("not_playable");
            }
            serde_json::from_value(value).expect("Beeld & Geluid overview item")
        }

        fn beeldengeluid_snapshot(include_lesson_seven: bool) -> Vec<AddonMediaListRecord> {
            let mut items = vec![
                ("episode:2101608050040897825", "Les 1", "1993-01-24", false),
                ("episode:2101608050040897826", "Les 2", "1993-01-31", false),
                ("episode:2101608050040897827", "Les 3", "1993-02-07", false),
                ("episode:2101608050040897828", "Les 4", "1993-02-14", false),
                ("episode:2101608050040897829", "Les 5", "1993-02-21", false),
                ("episode:2101608050040897830", "Les 6", "1993-02-28", false),
            ];
            if include_lesson_seven {
                items.push((
                    "episode:2101608050040897831",
                    "Les 7: Cartoon, strook en paginaverhaal",
                    "1993-03-07",
                    true,
                ));
            }
            items.push(("episode:2101608050040897832", "Les 8", "1993-03-14", false));

            std::iter::once(list_record(
                "serie/2101608030022428031/strip-en-cartoontekenen",
            ))
            .chain(items.into_iter().enumerate().map(
                |(rank, (provider_id, title, release_date, unavailable))| {
                    beeldengeluid_item(
                        provider_id,
                        i32::try_from(rank).expect("fixture rank"),
                        title,
                        release_date,
                        unavailable,
                    )
                },
            ))
            .collect()
        }

        async fn configure_managed_storage(pool: &DbPool, root: &std::path::Path) {
            sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (9000)"#)
                .execute(pool)
                .await
                .expect("managed media source");
            sqlx::query(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (9000)"#)
                .execute(pool)
                .await
                .expect("managed local source");
            sqlx::query(
                r#"INSERT INTO "AddonMediaSource" (
                       "AddonId", "MediaSourceId", "Name", "RootPath"
                   ) VALUES ('org.example.fixture', 9000, 'fixture add-on', ?)"#,
            )
            .bind(root.to_string_lossy().as_ref())
            .execute(pool)
            .await
            .expect("managed add-on source");
        }

        async fn seed_local_replacement(pool: &DbPool, root: &std::path::Path) {
            sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (9100)"#)
                .execute(pool)
                .await
                .expect("replacement media source");
            sqlx::query(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (9100)"#)
                .execute(pool)
                .await
                .expect("replacement local source");
            sqlx::query(
                r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
                   VALUES (9100, 'Local replacements', 'Movies', 9100)"#,
            )
            .execute(pool)
            .await
            .expect("replacement library");
            sqlx::query(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (9100)"#)
                .execute(pool)
                .await
                .expect("replacement local library");
            sqlx::query(
                r#"INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path")
                   VALUES (9100, 9100, ?)"#,
            )
            .bind(root.to_string_lossy().as_ref())
            .execute(pool)
            .await
            .expect("replacement library path");
            sqlx::query(
                r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State")
                   VALUES (9200, 9100, 0)"#,
            )
            .execute(pool)
            .await
            .expect("replacement media item");
            sqlx::query(r#"INSERT INTO "Movie" ("Id") VALUES (9200)"#)
                .execute(pool)
                .await
                .expect("replacement movie");
            sqlx::query(
                r#"INSERT INTO "MovieMetadata" (
                       "Id", "MovieId", "MetadataKind", "Title", "SortTitle",
                       "Year", "DateAdded", "DateUpdated"
                   ) VALUES (
                       9200, 9200, 0, 'Les 7 local replacement', 'Les 7 local replacement',
                       1993, '2026-08-25T00:00:00Z', '2026-08-25T00:00:00Z'
                   )"#,
            )
            .execute(pool)
            .await
            .expect("replacement metadata");
            sqlx::query(
                r#"INSERT INTO "MediaVersion" (
                       "Id", "DateAdded", "DateUpdated", "Duration", "Height", "MovieId",
                       "VideoScanKind", "Width"
                   ) VALUES (
                       9300, '2026-08-25T00:00:00Z', '2026-08-25T00:00:00Z',
                       '00:22:00', 1080, 9200, 0, 1920
                   )"#,
            )
            .execute(pool)
            .await
            .expect("replacement version");
            sqlx::query(
                r#"INSERT INTO "MediaFile" ("MediaVersionId", "Path", "PathHash")
                   VALUES (9300, ?, 'les-7-local-replacement')"#,
            )
            .bind(root.join("les-7.mkv").to_string_lossy().as_ref())
            .execute(pool)
            .await
            .expect("replacement file");
            sqlx::query(
                r#"INSERT INTO "MediaStream" (
                       "Index", "MediaStreamKind", "Channels", "Default", "Forced",
                       "MediaVersionId", "BitsPerRawSample", "AttachedPic"
                   ) VALUES (0, 1, 0, 1, 0, 9300, 0, 0)"#,
            )
            .execute(pool)
            .await
            .expect("replacement video stream");
        }

        #[tokio::test]
        async fn paginated_overview_resumes_from_checkpoint_and_promotes_only_when_complete() {
            let (_temporary, pool, coordinator) = fixture().await;
            let list_id = queue(
                &coordinator,
                "https://media.example.test/series",
                AddonMediaListDetailPolicy::Automatic,
            )
            .await;
            let first_job = coordinator
                .claim_import_job()
                .await
                .expect("claim first page")
                .expect("first page job");
            coordinator
                .commit_import_page(
                    &first_job,
                    MediaListDiscoveryPage {
                        complete: false,
                        next_cursor: Some("page-2".to_owned()),
                        total_hint: Some(2),
                        revision_token: Some("revision-1".to_owned()),
                        records: vec![
                            list_record("fixture/series"),
                            item_record("episode:1", 0, "Overview one"),
                        ],
                    },
                )
                .await
                .expect("checkpoint first page");
            let visible_items: i64 = sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ?"#,
            )
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .expect("count draft items");
            assert_eq!(visible_items, 0, "an incomplete generation stays staged");

            let restarted = AddonMediaListImportCoordinator::new(pool.clone());
            let second_job = restarted
                .claim_import_job()
                .await
                .expect("claim resumed page")
                .expect("resumed page job");
            assert_eq!(second_job.cursor.as_deref(), Some("page-2"));
            restarted
                .commit_import_page(
                    &second_job,
                    MediaListDiscoveryPage {
                        complete: true,
                        next_cursor: None,
                        total_hint: Some(2),
                        revision_token: Some("revision-1".to_owned()),
                        records: vec![item_record("episode:2", 1, "Overview two")],
                    },
                )
                .await
                .expect("complete resumed overview");

            let progress = restarted
                .progress(list_id)
                .await
                .expect("progress")
                .expect("import progress");
            assert_eq!(progress.state, AddonMediaListImportState::Ready);
            assert_eq!(progress.discovered_count, 2);
            let rows = sqlx::query(
                r#"SELECT "ProviderId", "Rank" FROM "AddonMediaListItem"
                   WHERE "AddonMediaListId" = ? ORDER BY "Rank""#,
            )
            .bind(list_id)
            .fetch_all(&pool)
            .await
            .expect("promoted items");
            assert_eq!(rows.len(), 2);
            assert_eq!(
                rows[0].try_get::<String, _>("ProviderId").unwrap(),
                "episode:1"
            );
            assert_eq!(rows[1].try_get::<i32, _>("Rank").unwrap(), 1);
        }

        #[tokio::test]
        #[allow(clippy::too_many_lines)]
        async fn beeldengeluid_refresh_repairs_missing_unavailable_episode_without_order_drift() {
            let (temporary, pool, coordinator) = fixture().await;
            let managed_root = temporary.path().join("managed");
            std::fs::create_dir(&managed_root).expect("managed fixture root");
            configure_managed_storage(&pool, &managed_root).await;
            let source_url = "https://schatkamer.beeldengeluid.nl/serie/2101608030022428031/strip-en-cartoontekenen?alleenafspeelbaar=nee";

            let list_id = complete_overview(
                &coordinator,
                source_url,
                AddonMediaListDetailPolicy::ManualOnly,
                beeldengeluid_snapshot(false),
                "seven-item-snapshot",
            )
            .await;
            let initial_ids: Vec<String> = sqlx::query_scalar(
                r#"SELECT "ProviderId" FROM "AddonMediaListItem"
                   WHERE "AddonMediaListId" = ? ORDER BY "Rank", "Id""#,
            )
            .bind(list_id)
            .fetch_all(&pool)
            .await
            .expect("initial seven-item snapshot");
            assert_eq!(initial_ids.len(), 7);
            assert!(!initial_ids
                .iter()
                .any(|provider_id| provider_id == "episode:2101608050040897831"));

            let repaired = complete_overview(
                &coordinator,
                source_url,
                AddonMediaListDetailPolicy::ManualOnly,
                beeldengeluid_snapshot(true),
                "eight-item-snapshot",
            )
            .await;
            assert_eq!(repaired, list_id);
            let repaired_rows = sqlx::query(
                r#"SELECT "Id", "ProviderId", "Rank", "DisplayTitle", "Availability",
                          "AvailabilityReason", "ProviderMetadataJson"
                   FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ?
                   ORDER BY "Rank", "Id""#,
            )
            .bind(list_id)
            .fetch_all(&pool)
            .await
            .expect("repaired eight-item snapshot");
            assert_eq!(repaired_rows.len(), 8);
            let repaired_ids = repaired_rows
                .iter()
                .map(|row| row.try_get::<String, _>("ProviderId").expect("provider id"))
                .collect::<Vec<_>>();
            assert_eq!(
                repaired_ids,
                [
                    "episode:2101608050040897825",
                    "episode:2101608050040897826",
                    "episode:2101608050040897827",
                    "episode:2101608050040897828",
                    "episode:2101608050040897829",
                    "episode:2101608050040897830",
                    "episode:2101608050040897831",
                    "episode:2101608050040897832",
                ]
            );
            let lesson_seven = &repaired_rows[6];
            let lesson_seven_id: EntityId = lesson_seven.try_get("Id").expect("Les 7 item id");
            assert_eq!(lesson_seven.try_get::<i32, _>("Rank").unwrap(), 6);
            assert_eq!(
                lesson_seven.try_get::<String, _>("DisplayTitle").unwrap(),
                "Les 7: Cartoon, strook en paginaverhaal"
            );
            assert_eq!(
                lesson_seven.try_get::<i32, _>("Availability").unwrap(),
                encode_enum(ersatzrs_domain::AddonMediaListItemAvailability::Unavailable)
            );
            assert_eq!(
                lesson_seven
                    .try_get::<Option<String>, _>("AvailabilityReason")
                    .unwrap()
                    .as_deref(),
                Some("not_playable")
            );
            let metadata: serde_json::Value = serde_json::from_str(
                &lesson_seven
                    .try_get::<String, _>("ProviderMetadataJson")
                    .expect("Les 7 provider metadata"),
            )
            .expect("decode Les 7 provider metadata");
            assert_eq!(metadata["release_date"], "1993-03-07");
            let detail_state: i32 = sqlx::query_scalar(
                r#"SELECT "State" FROM "AddonMediaListDetailJob"
                   WHERE "AddonMediaListItemId" = ?"#,
            )
            .bind(lesson_seven_id)
            .fetch_one(&pool)
            .await
            .expect("Les 7 detail state");
            assert_eq!(
                detail_state,
                encode_enum(AddonMediaListDetailState::NotRequested),
                "an explicitly unavailable overview item must not require a detail fetch"
            );

            let idempotent = complete_overview(
                &coordinator,
                source_url,
                AddonMediaListDetailPolicy::ManualOnly,
                beeldengeluid_snapshot(true),
                "eight-item-snapshot-repeat",
            )
            .await;
            assert_eq!(idempotent, list_id);
            let repeated: Vec<(EntityId, String, i32)> = sqlx::query_as(
                r#"SELECT "Id", "ProviderId", "Rank" FROM "AddonMediaListItem"
                   WHERE "AddonMediaListId" = ? ORDER BY "Rank", "Id""#,
            )
            .bind(list_id)
            .fetch_all(&pool)
            .await
            .expect("idempotent repaired snapshot");
            assert_eq!(repeated.len(), 8);
            assert_eq!(repeated[6].0, lesson_seven_id);
            assert_eq!(
                repeated
                    .iter()
                    .map(|(_, provider_id, rank)| (provider_id.as_str(), *rank))
                    .collect::<Vec<_>>(),
                repaired_ids
                    .iter()
                    .enumerate()
                    .map(|(rank, provider_id)| {
                        (
                            provider_id.as_str(),
                            i32::try_from(rank).expect("fixture rank"),
                        )
                    })
                    .collect::<Vec<_>>()
            );

            let repository = SqliteAddonMediaListRepository(pool.clone());
            let replacement_root = temporary.path().join("replacement");
            std::fs::create_dir(&replacement_root).expect("replacement fixture root");
            seed_local_replacement(&pool, &replacement_root).await;
            repository
                .set_replacement(lesson_seven_id, Some((9200, 9300)))
                .await
                .expect("assign Local replacement to unavailable Les 7");
            let review_item = repository
                .list_items(list_id)
                .await
                .expect("review repaired items")
                .into_iter()
                .find(|item| item.id == lesson_seven_id)
                .expect("review contains Les 7");
            assert_eq!(review_item.replacement_media_item_id, Some(9200));
            assert_eq!(review_item.replacement_media_version_id, Some(9300));
            assert!(review_item.replacement_valid);

            repository
                .publish(list_id)
                .await
                .expect("publish repaired overview");
            let published: bool = sqlx::query_scalar(
                r#"SELECT "Published" FROM "AddonMediaListItem" WHERE "Id" = ?"#,
            )
            .bind(lesson_seven_id)
            .fetch_one(&pool)
            .await
            .expect("published Les 7 row");
            assert!(published, "the unavailable row must survive publication");

            let item = repository
                .list_items(list_id)
                .await
                .expect("review repaired items")
                .into_iter()
                .find(|item| item.id == lesson_seven_id)
                .expect("review contains Les 7");
            assert_eq!(
                item.availability,
                ersatzrs_domain::AddonMediaListItemAvailability::Unavailable
            );
            assert_eq!(item.availability_reason.as_deref(), Some("not_playable"));
            assert_eq!(item.replacement_media_item_id, Some(9200));
            assert_eq!(item.replacement_media_version_id, Some(9300));
            assert!(item.replacement_valid);
        }

        #[tokio::test]
        async fn restart_finalizes_a_checkpointed_complete_page_without_provider_refetch() {
            let (_temporary, pool, coordinator) = fixture().await;
            let list_id = queue(
                &coordinator,
                "https://media.example.test/checkpointed-complete",
                AddonMediaListDetailPolicy::ManualOnly,
            )
            .await;
            let job = coordinator.claim_import_job().await.unwrap().unwrap();
            let header = serde_json::to_string(&list_record("fixture/checkpointed-complete"))
                .expect("serialize list header");
            let item = item_record("episode:1", 0, "Checkpointed overview");
            let item_json = serde_json::to_string(&item).expect("serialize overview item");
            sqlx::query(
                r#"INSERT INTO "AddonMediaListImportItem" (
                       "AddonMediaListImportJobId", "VerificationPass", "ProviderId", "Rank",
                       "RecordJson", "OverviewFingerprint"
                   ) VALUES (?, 0, 'episode:1', 0, ?, ?)"#,
            )
            .bind(job.id)
            .bind(&item_json)
            .bind(sha256_hex(item_json.as_bytes()))
            .execute(&pool)
            .await
            .expect("checkpoint final item");
            sqlx::query(
                r#"UPDATE "AddonMediaListImportJob"
                   SET "ListRecordJson" = ?, "RevisionToken" = 'revision-1',
                       "DiscoveredCount" = 1, "OverviewComplete" = 1,
                       "LeaseOwner" = NULL, "LeaseExpiresAt" = NULL
                   WHERE "Id" = ?"#,
            )
            .bind(header)
            .bind(job.id)
            .execute(&pool)
            .await
            .expect("checkpoint complete page before simulated restart");

            assert!(coordinator.process_one_import_page().await.unwrap());
            let progress = coordinator.progress(list_id).await.unwrap().unwrap();
            assert_eq!(progress.state, AddonMediaListImportState::Ready);
            assert_eq!(progress.discovered_count, 1);
            let provider_id: String = sqlx::query_scalar(
                r#"SELECT "ProviderId" FROM "AddonMediaListItem"
                   WHERE "AddonMediaListId" = ?"#,
            )
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .expect("promoted checkpointed item");
            assert_eq!(provider_id, "episode:1");
        }

        #[tokio::test]
        async fn unversioned_overview_requires_matching_second_pass() {
            let (_temporary, _pool, coordinator) = fixture().await;
            let list_id = queue(
                &coordinator,
                "https://media.example.test/unversioned",
                AddonMediaListDetailPolicy::Automatic,
            )
            .await;
            let first = coordinator
                .claim_import_job()
                .await
                .unwrap()
                .expect("first pass");
            let records = vec![
                list_record("fixture/unversioned"),
                item_record("episode:1", 0, "Overview"),
            ];
            coordinator
                .commit_import_page(
                    &first,
                    MediaListDiscoveryPage {
                        complete: true,
                        next_cursor: None,
                        total_hint: Some(1),
                        revision_token: None,
                        records: records.clone(),
                    },
                )
                .await
                .expect("first verification pass");
            let progress = coordinator.progress(list_id).await.unwrap().unwrap();
            assert_eq!(progress.state, AddonMediaListImportState::Verifying);
            let second = coordinator
                .claim_import_job()
                .await
                .unwrap()
                .expect("second pass");
            assert_eq!(second.verification_pass, 1);
            coordinator
                .commit_import_page(
                    &second,
                    MediaListDiscoveryPage {
                        complete: true,
                        next_cursor: None,
                        total_hint: Some(1),
                        revision_token: None,
                        records,
                    },
                )
                .await
                .expect("matching second pass");
            assert_eq!(
                coordinator.progress(list_id).await.unwrap().unwrap().state,
                AddonMediaListImportState::Ready
            );
        }

        #[tokio::test]
        async fn manual_policy_and_checkpointed_detail_survive_restart_without_provider_call() {
            let (_temporary, pool, coordinator) = fixture().await;
            let list_id = complete_single_page(
                &coordinator,
                "https://media.example.test/manual",
                AddonMediaListDetailPolicy::ManualOnly,
                "fixture/manual",
                &["episode:1"],
            )
            .await;
            coordinator
                .authorize_after_publish(list_id)
                .await
                .expect("publication authorization");
            let item_id: EntityId = sqlx::query_scalar(
                r#"SELECT "Id" FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ?"#,
            )
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .expect("item id");
            let state: i32 = sqlx::query_scalar(
                r#"SELECT "State" FROM "AddonMediaListDetailJob"
                   WHERE "AddonMediaListItemId" = ?"#,
            )
            .bind(item_id)
            .fetch_one(&pool)
            .await
            .expect("manual state");
            assert_eq!(state, encode_enum(AddonMediaListDetailState::NotRequested));

            coordinator
                .request_item_details(list_id, item_id)
                .await
                .expect("manual detail request");
            let job = coordinator
                .claim_detail_job()
                .await
                .expect("claim detail")
                .expect("manual detail job");
            coordinator
                .checkpoint_detail_result(
                    &job,
                    MediaListEnrichOutcomeKind::Complete,
                    None,
                    &item_record("episode:1", 0, "Full programme detail"),
                )
                .await
                .expect("checkpoint provider result");

            // No installation exists in this fixture. Success after restart is
            // therefore proof that the provider was not invoked a second time.
            let restarted = AddonMediaListImportCoordinator::new(pool.clone());
            assert!(restarted
                .process_one_detail()
                .await
                .expect("apply checkpointed detail"));
            let state: i32 = sqlx::query_scalar(
                r#"SELECT "State" FROM "AddonMediaListDetailJob"
                   WHERE "AddonMediaListItemId" = ?"#,
            )
            .bind(item_id)
            .fetch_one(&pool)
            .await
            .expect("completed state");
            assert_eq!(state, encode_enum(AddonMediaListDetailState::Complete));
            let metadata: String = sqlx::query_scalar(
                r#"SELECT "ProviderMetadataJson" FROM "AddonMediaListItem" WHERE "Id" = ?"#,
            )
            .bind(item_id)
            .fetch_one(&pool)
            .await
            .expect("merged metadata");
            assert!(metadata.contains("Full programme detail"));
        }

        #[tokio::test]
        async fn shared_host_cooldown_blocks_the_next_list_and_persists_sample() {
            let (_temporary, pool, coordinator) = fixture().await;
            sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, ?), (?, ?)"#)
                .bind(ersatzrs_domain::CONFIG_KEY_ADDON_DETAIL_WAIT_MINIMUM_SECONDS)
                .bind("7")
                .bind(ersatzrs_domain::CONFIG_KEY_ADDON_DETAIL_WAIT_MAXIMUM_SECONDS)
                .bind("11")
                .execute(&pool)
                .await
                .expect("configure detail wait after coordinator startup");
            let first_list = complete_single_page(
                &coordinator,
                "https://media.example.test/first",
                AddonMediaListDetailPolicy::ManualOnly,
                "fixture/first",
                &["first:1"],
            )
            .await;
            let second_list = complete_single_page(
                &coordinator,
                "https://media.example.test/second",
                AddonMediaListDetailPolicy::ManualOnly,
                "fixture/second",
                &["second:1"],
            )
            .await;
            for list_id in [first_list, second_list] {
                let item_id: EntityId = sqlx::query_scalar(
                    r#"SELECT "Id" FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ?"#,
                )
                .bind(list_id)
                .fetch_one(&pool)
                .await
                .expect("item id");
                coordinator
                    .request_item_details(list_id, item_id)
                    .await
                    .expect("manual request");
            }
            let first_job = coordinator
                .claim_detail_job()
                .await
                .expect("first claim")
                .expect("first detail");
            coordinator
                .finish_detail_failure(&first_job, "rate limited", true, Some(120))
                .await
                .expect("finish attempted detail");
            assert!(coordinator
                .claim_detail_job()
                .await
                .expect("second claim check")
                .is_none());
            let row = sqlx::query(
                r#"SELECT "SampledDelaySeconds", "NextEligibleAt"
                   FROM "AddonProviderHostCooldown" WHERE "ProviderHost" = ?"#,
            )
            .bind("media.example.test")
            .fetch_one(&pool)
            .await
            .expect("persisted cooldown");
            let delay: i32 = row.try_get("SampledDelaySeconds").unwrap();
            assert!((7..=11).contains(&delay));
            let next: String = row.try_get("NextEligibleAt").unwrap();
            let next = crate::repositories::common::decode_optional_datetime(
                "fixture cooldown",
                Some(next),
            )
            .expect("decode cooldown")
            .expect("cooldown timestamp");
            assert!(next >= Utc::now() + Duration::seconds(100));
        }

        #[tokio::test]
        async fn a_new_range_never_shortens_an_existing_provider_cooldown() {
            let (_temporary, pool, _coordinator) = fixture().await;
            let now = encode_datetime(Utc::now());
            let existing = encode_datetime(Utc::now() + Duration::seconds(90));
            let shorter = encode_datetime(Utc::now() + Duration::seconds(7));
            sqlx::query(
                r#"INSERT INTO "AddonProviderHostCooldown" (
                       "ProviderHost", "NextEligibleAt", "SampledDelaySeconds",
                       "LastAttemptAt", "UpdatedAt"
                   ) VALUES ('media.example.test', ?, 90, ?, ?)"#,
            )
            .bind(&existing)
            .bind(&now)
            .bind(&now)
            .execute(&pool)
            .await
            .expect("seed longer persisted cooldown");

            let mut transaction = crate::db::begin_write(&pool)
                .await
                .expect("begin cooldown transaction");
            update_host_cooldown(&mut transaction, "media.example.test", &now, &shorter, 7)
                .await
                .expect("apply newly sampled shorter range");
            transaction.commit().await.expect("commit cooldown update");

            let persisted: String = sqlx::query_scalar(
                r#"SELECT "NextEligibleAt" FROM "AddonProviderHostCooldown"
                   WHERE "ProviderHost" = 'media.example.test'"#,
            )
            .fetch_one(&pool)
            .await
            .expect("read preserved cooldown");
            assert_eq!(persisted, existing);
        }

        #[tokio::test]
        async fn automatic_details_are_not_claimable_until_publication_authorizes_them() {
            let (_temporary, pool, coordinator) = fixture().await;
            let list_id = complete_single_page(
                &coordinator,
                "https://media.example.test/automatic",
                AddonMediaListDetailPolicy::Automatic,
                "fixture/automatic",
                &["episode:1", "episode:2"],
            )
            .await;
            assert!(coordinator.claim_detail_job().await.unwrap().is_none());
            let states: Vec<i32> = sqlx::query_scalar(
                r#"SELECT "State" FROM "AddonMediaListDetailJob"
                   WHERE "AddonMediaListId" = ? ORDER BY "AddonMediaListItemId""#,
            )
            .bind(list_id)
            .fetch_all(&pool)
            .await
            .expect("pre-publication detail states");
            assert_eq!(
                states,
                vec![encode_enum(AddonMediaListDetailState::NotRequested); 2]
            );

            coordinator
                .authorize_after_publish(list_id)
                .await
                .expect("authorize automatic detail work");
            let progress = coordinator.progress(list_id).await.unwrap().unwrap();
            assert_eq!(progress.detail_pending, 2);
            assert!(coordinator.claim_detail_job().await.unwrap().is_some());
        }

        #[tokio::test]
        async fn pause_resume_cancel_and_concurrent_claims_are_durable() {
            let (_temporary, _pool, coordinator) = fixture().await;
            let list_id = queue(
                &coordinator,
                "https://media.example.test/controlled",
                AddonMediaListDetailPolicy::ManualOnly,
            )
            .await;
            coordinator
                .control_import(list_id, AddonMediaListImportAction::Pause)
                .await
                .expect("pause import");
            assert!(coordinator.claim_import_job().await.unwrap().is_none());
            coordinator
                .control_import(list_id, AddonMediaListImportAction::Resume)
                .await
                .expect("resume import");
            let other = coordinator.clone();
            let (left, right) =
                tokio::join!(coordinator.claim_import_job(), other.claim_import_job());
            assert_eq!(
                usize::from(left.unwrap().is_some()) + usize::from(right.unwrap().is_some()),
                1
            );

            coordinator
                .control_import(list_id, AddonMediaListImportAction::Cancel)
                .await
                .expect("cancel import");
            let progress = coordinator.progress(list_id).await.unwrap().unwrap();
            assert_eq!(progress.state, AddonMediaListImportState::Cancelled);
            assert!(coordinator.claim_import_job().await.unwrap().is_none());
        }

        #[tokio::test]
        async fn duplicate_identity_across_overview_pages_is_rejected() {
            let (_temporary, _pool, coordinator) = fixture().await;
            queue(
                &coordinator,
                "https://media.example.test/duplicates",
                AddonMediaListDetailPolicy::ManualOnly,
            )
            .await;
            let first = coordinator.claim_import_job().await.unwrap().unwrap();
            coordinator
                .commit_import_page(
                    &first,
                    MediaListDiscoveryPage {
                        complete: false,
                        next_cursor: Some("page-2".to_owned()),
                        total_hint: Some(2),
                        revision_token: Some("revision-1".to_owned()),
                        records: vec![
                            list_record("fixture/duplicates"),
                            item_record("episode:1", 0, "Overview"),
                        ],
                    },
                )
                .await
                .expect("first page");
            let second = coordinator.claim_import_job().await.unwrap().unwrap();
            let error = coordinator
                .commit_import_page(
                    &second,
                    MediaListDiscoveryPage {
                        complete: true,
                        next_cursor: None,
                        total_hint: Some(2),
                        revision_token: Some("revision-1".to_owned()),
                        records: vec![item_record("episode:1", 1, "Repeated identity")],
                    },
                )
                .await
                .expect_err("cross-page duplicate must fail");
            assert!(error.to_string().contains("repeated an item identity"));
        }

        #[tokio::test]
        async fn changing_unversioned_overview_reconciles_after_one_stable_pass() {
            let (_temporary, pool, coordinator) = fixture().await;
            let list_id = queue(
                &coordinator,
                "https://media.example.test/reconciled",
                AddonMediaListDetailPolicy::ManualOnly,
            )
            .await;
            let first = coordinator.claim_import_job().await.unwrap().unwrap();
            coordinator
                .commit_import_page(
                    &first,
                    MediaListDiscoveryPage {
                        complete: true,
                        next_cursor: None,
                        total_hint: Some(1),
                        revision_token: None,
                        records: vec![
                            list_record("fixture/reconciled"),
                            item_record("episode:1", 0, "First pass"),
                        ],
                    },
                )
                .await
                .expect("first pass");
            let second = coordinator.claim_import_job().await.unwrap().unwrap();
            coordinator
                .commit_import_page(
                    &second,
                    MediaListDiscoveryPage {
                        complete: true,
                        next_cursor: None,
                        total_hint: Some(1),
                        revision_token: None,
                        records: vec![
                            list_record("fixture/reconciled"),
                            item_record("episode:2", 0, "Changed pass"),
                        ],
                    },
                )
                .await
                .expect("changed pass becomes the reconciled baseline");
            let verifying = coordinator.progress(list_id).await.unwrap().unwrap();
            assert_eq!(verifying.state, AddonMediaListImportState::Verifying);

            let third = coordinator.claim_import_job().await.unwrap().unwrap();
            coordinator
                .commit_import_page(
                    &third,
                    MediaListDiscoveryPage {
                        complete: true,
                        next_cursor: None,
                        total_hint: Some(1),
                        revision_token: None,
                        records: vec![
                            list_record("fixture/reconciled"),
                            item_record("episode:2", 0, "Stable reconciled pass"),
                        ],
                    },
                )
                .await
                .expect("stable third pass promotes the overview");
            let progress = coordinator.progress(list_id).await.unwrap().unwrap();
            assert_eq!(progress.state, AddonMediaListImportState::Ready);
            let provider_id: String = sqlx::query_scalar(
                r#"SELECT "ProviderId" FROM "AddonMediaListItem"
                   WHERE "AddonMediaListId" = ?"#,
            )
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .expect("promoted reconciled item");
            assert_eq!(provider_id, "episode:2");
        }

        #[tokio::test]
        async fn repeatedly_changing_unversioned_overview_becomes_needs_attention() {
            let (_temporary, _pool, coordinator) = fixture().await;
            let list_id = queue(
                &coordinator,
                "https://media.example.test/churning",
                AddonMediaListDetailPolicy::ManualOnly,
            )
            .await;
            for (provider_id, title) in [("episode:1", "First pass"), ("episode:2", "Changed pass")]
            {
                let job = coordinator.claim_import_job().await.unwrap().unwrap();
                coordinator
                    .commit_import_page(
                        &job,
                        MediaListDiscoveryPage {
                            complete: true,
                            next_cursor: None,
                            total_hint: Some(1),
                            revision_token: None,
                            records: vec![
                                list_record("fixture/churning"),
                                item_record(provider_id, 0, title),
                            ],
                        },
                    )
                    .await
                    .expect("queue the next verification pass");
            }
            let third = coordinator.claim_import_job().await.unwrap().unwrap();
            let error = coordinator
                .commit_import_page(
                    &third,
                    MediaListDiscoveryPage {
                        complete: true,
                        next_cursor: None,
                        total_hint: Some(1),
                        revision_token: None,
                        records: vec![
                            list_record("fixture/churning"),
                            item_record("episode:3", 0, "Changed again"),
                        ],
                    },
                )
                .await
                .expect_err("repeatedly changing verification pass");
            coordinator
                .fail_import(third.id, &error.to_string(), false)
                .await
                .expect("checkpoint repeated churn failure");
            let progress = coordinator.progress(list_id).await.unwrap().unwrap();
            assert_eq!(progress.state, AddonMediaListImportState::NeedsAttention);
            assert!(progress
                .diagnostic
                .as_deref()
                .is_some_and(|value| value.contains("changed repeatedly")));
        }

        #[tokio::test]
        async fn unchanged_refresh_preserves_completed_details_and_changed_stub_resets_them() {
            let (_temporary, pool, coordinator) = fixture().await;
            let source_url = "https://media.example.test/refresh";
            let list_id = complete_single_page(
                &coordinator,
                source_url,
                AddonMediaListDetailPolicy::ManualOnly,
                "fixture/refresh",
                &["episode:1"],
            )
            .await;
            let item_id: EntityId = sqlx::query_scalar(
                r#"SELECT "Id" FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ?"#,
            )
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .expect("item id");
            coordinator
                .request_item_details(list_id, item_id)
                .await
                .expect("manual details");
            let detail = coordinator.claim_detail_job().await.unwrap().unwrap();
            coordinator
                .checkpoint_detail_result(
                    &detail,
                    MediaListEnrichOutcomeKind::Complete,
                    None,
                    &item_record("episode:1", 0, "Full programme detail"),
                )
                .await
                .expect("checkpoint full detail");
            assert!(coordinator.process_one_detail().await.unwrap());

            let refreshed = complete_single_page(
                &coordinator,
                source_url,
                AddonMediaListDetailPolicy::ManualOnly,
                "fixture/refresh",
                &["episode:1"],
            )
            .await;
            assert_eq!(refreshed, list_id);
            let metadata: String = sqlx::query_scalar(
                r#"SELECT "ProviderMetadataJson" FROM "AddonMediaListItem" WHERE "Id" = ?"#,
            )
            .bind(item_id)
            .fetch_one(&pool)
            .await
            .expect("preserved metadata");
            assert!(metadata.contains("Full programme detail"));
            let state: i32 = sqlx::query_scalar(
                r#"SELECT "State" FROM "AddonMediaListDetailJob"
                   WHERE "AddonMediaListItemId" = ?"#,
            )
            .bind(item_id)
            .fetch_one(&pool)
            .await
            .expect("preserved detail state");
            assert_eq!(state, encode_enum(AddonMediaListDetailState::Complete));

            queue(
                &coordinator,
                source_url,
                AddonMediaListDetailPolicy::ManualOnly,
            )
            .await;
            let changed = coordinator.claim_import_job().await.unwrap().unwrap();
            coordinator
                .commit_import_page(
                    &changed,
                    MediaListDiscoveryPage {
                        complete: true,
                        next_cursor: None,
                        total_hint: Some(1),
                        revision_token: Some("revision-2".to_owned()),
                        records: vec![
                            list_record("fixture/refresh"),
                            item_record("episode:1", 0, "Changed overview"),
                        ],
                    },
                )
                .await
                .expect("changed refresh");
            let state: i32 = sqlx::query_scalar(
                r#"SELECT "State" FROM "AddonMediaListDetailJob"
                   WHERE "AddonMediaListItemId" = ?"#,
            )
            .bind(item_id)
            .fetch_one(&pool)
            .await
            .expect("reset detail state");
            assert_eq!(state, encode_enum(AddonMediaListDetailState::NotRequested));
        }

        #[tokio::test]
        async fn many_page_overview_has_no_aggregate_invocation_deadline() {
            let (_temporary, pool, coordinator) = fixture().await;
            let list_id = queue(
                &coordinator,
                "https://media.example.test/large-series",
                AddonMediaListDetailPolicy::ManualOnly,
            )
            .await;
            let page_count = 75_i32;
            for rank in 0..page_count {
                let job = coordinator
                    .claim_import_job()
                    .await
                    .expect("claim overview page")
                    .expect("overview page remains queued");
                let complete = rank + 1 == page_count;
                let records =
                    std::iter::once((rank == 0).then(|| list_record("fixture/large-series")))
                        .flatten()
                        .chain(std::iter::once(item_record(
                            &format!("episode:{rank}"),
                            rank,
                            "Overview plot",
                        )))
                        .collect();
                coordinator
                    .commit_import_page(
                        &job,
                        MediaListDiscoveryPage {
                            complete,
                            next_cursor: (!complete).then(|| format!("page-{}", rank + 2)),
                            total_hint: Some(u32::try_from(page_count).expect("page count")),
                            revision_token: Some("revision-large".to_owned()),
                            records,
                        },
                    )
                    .await
                    .expect("checkpoint one independently bounded page");
            }

            let progress = coordinator.progress(list_id).await.unwrap().unwrap();
            assert_eq!(progress.state, AddonMediaListImportState::Ready);
            assert_eq!(
                progress.discovered_count,
                u64::try_from(page_count).unwrap()
            );
            let item_count: i64 = sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ?"#,
            )
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .expect("promoted item count");
            assert_eq!(item_count, i64::from(page_count));
        }

        #[tokio::test]
        async fn revision_changes_and_cursor_loops_are_rejected_before_promotion() {
            let (_temporary, _pool, coordinator) = fixture().await;
            let revision_list = queue(
                &coordinator,
                "https://media.example.test/revision-churn",
                AddonMediaListDetailPolicy::ManualOnly,
            )
            .await;
            let first = coordinator.claim_import_job().await.unwrap().unwrap();
            coordinator
                .commit_import_page(
                    &first,
                    MediaListDiscoveryPage {
                        complete: false,
                        next_cursor: Some("page-2".to_owned()),
                        total_hint: Some(2),
                        revision_token: Some("revision-1".to_owned()),
                        records: vec![
                            list_record("fixture/revision-churn"),
                            item_record("episode:1", 0, "Overview"),
                        ],
                    },
                )
                .await
                .expect("first revision page");
            let second = coordinator.claim_import_job().await.unwrap().unwrap();
            let error = coordinator
                .commit_import_page(
                    &second,
                    MediaListDiscoveryPage {
                        complete: true,
                        next_cursor: None,
                        total_hint: Some(2),
                        revision_token: Some("revision-2".to_owned()),
                        records: vec![item_record("episode:2", 1, "Overview")],
                    },
                )
                .await
                .expect_err("revision change must fail");
            assert!(error.to_string().contains("revision changed"));
            assert_ne!(
                coordinator
                    .progress(revision_list)
                    .await
                    .unwrap()
                    .unwrap()
                    .state,
                AddonMediaListImportState::Ready
            );

            let loop_list = queue(
                &coordinator,
                "https://media.example.test/cursor-loop",
                AddonMediaListDetailPolicy::ManualOnly,
            )
            .await;
            // Release the unrelated failed test lease so the newer queued job
            // can be claimed without waiting for its expiry.
            sqlx::query(
                r#"UPDATE "AddonMediaListImportJob"
                   SET "State" = ?, "LeaseOwner" = NULL, "LeaseExpiresAt" = NULL
                   WHERE "AddonMediaListId" = ?"#,
            )
            .bind(encode_enum(AddonMediaListImportState::NeedsAttention))
            .bind(revision_list)
            .execute(&coordinator.pool)
            .await
            .expect("retire revision fixture");
            let first = coordinator.claim_import_job().await.unwrap().unwrap();
            assert_eq!(first.list_id, loop_list);
            coordinator
                .commit_import_page(
                    &first,
                    MediaListDiscoveryPage {
                        complete: false,
                        next_cursor: Some("same-cursor".to_owned()),
                        total_hint: Some(2),
                        revision_token: Some("revision-loop".to_owned()),
                        records: vec![
                            list_record("fixture/cursor-loop"),
                            item_record("episode:1", 0, "Overview"),
                        ],
                    },
                )
                .await
                .expect("first cursor page");
            let second = coordinator.claim_import_job().await.unwrap().unwrap();
            let error = coordinator
                .commit_import_page(
                    &second,
                    MediaListDiscoveryPage {
                        complete: false,
                        next_cursor: Some("same-cursor".to_owned()),
                        total_hint: Some(2),
                        revision_token: Some("revision-loop".to_owned()),
                        records: vec![item_record("episode:2", 1, "Overview")],
                    },
                )
                .await
                .expect_err("cursor loop must fail");
            assert!(error
                .to_string()
                .contains("repeated an overview continuation cursor"));
        }

        #[tokio::test]
        async fn detail_policy_switches_only_queue_missing_work_and_preserve_completed_data() {
            let (_temporary, pool, coordinator) = fixture().await;
            let list_id = complete_single_page(
                &coordinator,
                "https://media.example.test/policy-switch",
                AddonMediaListDetailPolicy::Automatic,
                "fixture/policy-switch",
                &["episode:1", "episode:2", "episode:3"],
            )
            .await;
            coordinator
                .authorize_after_publish(list_id)
                .await
                .expect("authorize automatic details");
            let detail = coordinator
                .claim_detail_job()
                .await
                .expect("claim first detail")
                .expect("first detail job");
            let completed_item_id = detail.item_id;
            coordinator
                .checkpoint_detail_result(
                    &detail,
                    MediaListEnrichOutcomeKind::Complete,
                    None,
                    &item_record(&detail.provider_id, 0, "Completed detail"),
                )
                .await
                .expect("checkpoint completed detail");
            assert!(coordinator.process_one_detail().await.unwrap());

            coordinator
                .set_detail_policy(list_id, AddonMediaListDetailPolicy::ManualOnly)
                .await
                .expect("switch to overview-only mode");
            let states: Vec<(EntityId, i32)> = sqlx::query_as(
                r#"SELECT "AddonMediaListItemId", "State"
                   FROM "AddonMediaListDetailJob"
                   WHERE "AddonMediaListId" = ? ORDER BY "AddonMediaListItemId""#,
            )
            .bind(list_id)
            .fetch_all(&pool)
            .await
            .expect("manual-only states");
            assert_eq!(states.len(), 3);
            assert_eq!(
                states
                    .iter()
                    .find(|(item_id, _)| *item_id == completed_item_id)
                    .map(|(_, state)| *state),
                Some(encode_enum(AddonMediaListDetailState::Complete))
            );
            assert!(states.iter().all(|(item_id, state)| {
                *item_id == completed_item_id
                    || *state == encode_enum(AddonMediaListDetailState::NotRequested)
            }));
            let metadata: String = sqlx::query_scalar(
                r#"SELECT "ProviderMetadataJson" FROM "AddonMediaListItem" WHERE "Id" = ?"#,
            )
            .bind(completed_item_id)
            .fetch_one(&pool)
            .await
            .expect("completed metadata");
            assert!(metadata.contains("Completed detail"));

            coordinator
                .set_detail_policy(list_id, AddonMediaListDetailPolicy::Automatic)
                .await
                .expect("resume automatic details");
            let states: Vec<(EntityId, i32)> = sqlx::query_as(
                r#"SELECT "AddonMediaListItemId", "State"
                   FROM "AddonMediaListDetailJob"
                   WHERE "AddonMediaListId" = ? ORDER BY "AddonMediaListItemId""#,
            )
            .bind(list_id)
            .fetch_all(&pool)
            .await
            .expect("automatic states");
            assert_eq!(
                states
                    .iter()
                    .find(|(item_id, _)| *item_id == completed_item_id)
                    .map(|(_, state)| *state),
                Some(encode_enum(AddonMediaListDetailState::Complete))
            );
            assert!(states.iter().all(|(item_id, state)| {
                *item_id == completed_item_id
                    || *state == encode_enum(AddonMediaListDetailState::Queued)
            }));
        }
    }
}
