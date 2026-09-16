//! Durable overview-first add-on list imports and deferred item enrichment.

use std::fmt::Write as _;

use chrono::{Duration, Utc};
use ersatzrs_addon_contract::{
    AddonMediaListRecord, MediaListArchiveDescriptor, MediaListDiscoveryPage,
    MediaListDiscoveryV2Page, MediaListEnrichOutcomeKind, MediaListImportMode as WireImportMode,
    MediaListIncrementalStrategy as WireIncrementalStrategy, CAPABILITY_MEDIA_LIST_IMPORT_V2,
};
use ersatzrs_core::repositories::{
    AddonMediaListImportAction, AddonMediaListImportCheckpoint, AddonMediaListImportProgress,
    RepositoryError, Result,
};
use ersatzrs_domain::{
    AddonMediaList, AddonMediaListContentKind, AddonMediaListDetailPolicy,
    AddonMediaListDetailState, AddonMediaListDetailWaitRange, AddonMediaListImportMode,
    AddonMediaListImportPurpose, AddonMediaListImportState, AddonMediaListIncrementalStrategy,
    AddonMediaListLifecyclePhase, AddonMediaListPresentationKind, EntityId,
};
use sha2::{Digest as _, Sha256};
use sqlx::Row as _;
use url::Url;
use uuid::Uuid;

use crate::addon_media_list::{
    decode_media_list_output_for_capability, InstalledAddonMediaListProvider,
    InstalledMediaListImportContext,
};
use crate::addon_media_list_archive::{
    prepare_import_archive, remove_relative_archive, SealedImportArchive, MAX_IMPORT_ARCHIVE_BYTES,
    MAX_IMPORT_ARCHIVE_ENTRIES,
};
use crate::addon_media_list_sync::{
    AddonMediaListRemoteSynchronizer, AddonMediaListSynchronizer, DetailMergeOutcome,
};
use crate::addon_media_storage::validated_folder_name;
use crate::db::{inserted_id, DbPool};
use crate::repositories::common::{
    decode_bool_column, decode_datetime, decode_enum_column, encode_datetime, encode_enum,
};
use crate::repositories::SqlxResultExt as _;

pub(crate) const IMPORT_LEASE_SECONDS: i64 = 90;
const DETAIL_LEASE_SECONDS: i64 = 90;
const DETAIL_PUBLICATION_RETRY_SECONDS: i64 = 5;
const MAX_IMPORT_ATTEMPTS: i32 = 5;
const MAX_DETAIL_ATTEMPTS: i32 = 5;
const MAX_UNVERSIONED_VERIFICATION_PASS: i32 = 2;

/// Host coordinator for durable import and detail jobs.
#[derive(Clone)]
pub struct AddonMediaListImportCoordinator {
    pool: DbPool,
    provider: InstalledAddonMediaListProvider,
    scheduled_refresh: bool,
    editor_revision: Option<i32>,
    delivery_mode: Option<ersatzrs_domain::AddonMediaListDeliveryMode>,
}

impl AddonMediaListImportCoordinator {
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self {
            provider: InstalledAddonMediaListProvider::new(pool.clone()),
            pool,
            scheduled_refresh: false,
            editor_revision: None,
            delivery_mode: None,
        }
    }

    #[must_use]
    pub(crate) fn with_editor_revision(mut self, revision: i32) -> Self {
        self.editor_revision = Some(revision);
        self
    }

    pub(crate) fn for_scheduled_refresh(pool: DbPool) -> Self {
        Self {
            scheduled_refresh: true,
            ..Self::new(pool)
        }
    }

    /// Apply an operator-selected mode before first-import identities are saved.
    #[must_use]
    pub(crate) fn with_delivery_mode(
        mut self,
        mode: ersatzrs_domain::AddonMediaListDeliveryMode,
    ) -> Self {
        self.delivery_mode = Some(mode);
        self
    }

    /// Queue the first full import after a reset has durably verified emptiness.
    pub(crate) async fn queue_reset_reload(
        &self,
        list_id: EntityId,
        operation_id: EntityId,
        owner_token: &str,
        lease_owner: &str,
    ) -> Result<EntityId> {
        let row =
            sqlx::query(r#"SELECT "AddonId", "SourceUrl" FROM "AddonMediaList" WHERE "Id" = ?"#)
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
        let context = self
            .provider
            .import_context(&addon_id, &source_url)
            .await?
            .ok_or_else(|| {
                RepositoryError::unavailable(
                    "queue add-on media-list reset import",
                    "the retained add-on does not support resumable imports",
                )
            })?;
        persist_reset_reload_import(
            &self.pool,
            list_id,
            operation_id,
            owner_token,
            lease_owner,
            &context.record_capability,
            &context.import_capability,
        )
        .await
    }

    /// Queue a complete scratch import for one leased reconcile operation.
    pub(crate) async fn queue_reconcile(
        &self,
        list_id: EntityId,
        operation_id: EntityId,
        owner_token: &str,
        lease_owner: &str,
    ) -> Result<EntityId> {
        let row =
            sqlx::query(r#"SELECT "AddonId", "SourceUrl" FROM "AddonMediaList" WHERE "Id" = ?"#)
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
        let context = self
            .provider
            .import_context(&addon_id, &source_url)
            .await?
            .filter(|context| context.import_capability == CAPABILITY_MEDIA_LIST_IMPORT_V2)
            .ok_or_else(|| {
                RepositoryError::unavailable(
                    "queue add-on media-list reconcile import",
                    "the retained add-on does not support complete staged imports",
                )
            })?;
        persist_reconcile_import(
            &self.pool,
            list_id,
            operation_id,
            owner_token,
            lease_owner,
            &context.record_capability,
            &context.import_capability,
        )
        .await
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
            let automatic = if self.scheduled_refresh {
                crate::addon_media_list_auto_publication::legacy_baseline(
                    &self.pool, addon_id, source_url,
                )
                .await?
            } else {
                crate::addon_media_list_auto_publication::manual_refresh(
                    &self.pool, addon_id, source_url,
                )
                .await?;
                None
            };
            let provider = InstalledAddonMediaListProvider::new(self.pool.clone());
            let id = AddonMediaListRemoteSynchronizer::new(self.pool.clone(), provider)
                .with_delivery_mode(self.delivery_mode)
                .add_or_refresh(
                    addon_id,
                    source_url,
                    requested_folder,
                    content_kind_override,
                    chapter_input,
                )
                .await?;
            self.set_detail_policy(id, detail_policy).await?;
            if let Some(baseline) = automatic {
                crate::addon_media_list_auto_publication::legacy_ready(&self.pool, id, baseline)
                    .await?;
            }
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
}

pub(crate) async fn persist_reset_reload_import(
    pool: &DbPool,
    list_id: EntityId,
    operation_id: EntityId,
    owner_token: &str,
    lease_owner: &str,
    record_capability: &str,
    import_capability: &str,
) -> Result<EntityId> {
    let now = encode_datetime(Utc::now());
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    let current: bool = sqlx::query_scalar(
        r#"SELECT EXISTS (
                   SELECT 1 FROM "AddonMediaListLifecycleJob" AS "job"
                   JOIN "ManagedLibraryLease" AS "lease"
                     ON "lease"."OperationId" = "job"."OperationId"
                   WHERE "job"."OperationId" = ? AND "job"."AddonMediaListId" = ?
                     AND "job"."Phase" = ? AND "lease"."OwnerToken" = ?
                     AND "lease"."LeaseOwner" = ? AND "lease"."LeaseExpiresAt" > ?
               )"#,
    )
    .bind(operation_id)
    .bind(list_id)
    .bind(encode_enum(
        AddonMediaListLifecyclePhase::QueueingInitialImport,
    ))
    .bind(owner_token)
    .bind(lease_owner)
    .bind(&now)
    .fetch_one(&mut *tx)
    .await
    .repository()?;
    if !current {
        return Err(RepositoryError::Conflict(
            "the reset lease or phase changed before queuing its import".to_owned(),
        ));
    }
    if let Some(existing) = sqlx::query_scalar::<_, EntityId>(
        r#"SELECT "Id" FROM "AddonMediaListImportJob"
               WHERE "AddonMediaListId" = ? AND "Generation" = 1
                 AND "Purpose" = ? AND "Mode" = ?"#,
    )
    .bind(list_id)
    .bind(encode_enum(AddonMediaListImportPurpose::ResetReload))
    .bind(encode_enum(AddonMediaListImportMode::Full))
    .fetch_optional(&mut *tx)
    .await
    .repository()?
    {
        tx.commit().await.repository()?;
        return Ok(existing);
    }
    let result = sqlx::query(
        r#"INSERT INTO "AddonMediaListImportJob" (
                   "AddonMediaListId", "Generation", "State", "RecordCapability", "ImportCapability",
                   "Purpose", "Mode", "IncrementalStrategy", "CreatedAt", "UpdatedAt",
                   "LastProgressAt"
               ) VALUES (?, 1, ?, ?, ?, ?, ?, 0, ?, ?, ?)"#,
    )
    .bind(list_id)
    .bind(encode_enum(AddonMediaListImportState::Queued))
    .bind(record_capability)
    .bind(import_capability)
    .bind(encode_enum(AddonMediaListImportPurpose::ResetReload))
    .bind(encode_enum(AddonMediaListImportMode::Full))
    .bind(&now)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .repository()?;
    let id = inserted_id(&mut tx, result).await.repository()?;
    tx.commit().await.repository()?;
    Ok(id)
}

#[allow(clippy::too_many_arguments)] // The durable lease and frozen contract are one fence.
#[allow(clippy::too_many_lines)] // Keep lease validation and candidate creation in one transaction.
async fn persist_reconcile_import(
    pool: &DbPool,
    list_id: EntityId,
    operation_id: EntityId,
    owner_token: &str,
    lease_owner: &str,
    record_capability: &str,
    import_capability: &str,
) -> Result<EntityId> {
    let now = encode_datetime(Utc::now());
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    let lifecycle = sqlx::query(
        r#"SELECT "job"."CandidateImportJobId"
           FROM "AddonMediaListLifecycleJob" AS "job"
           JOIN "AddonMediaList" AS "list" ON "list"."Id" = "job"."AddonMediaListId"
           JOIN "ManagedLibraryLease" AS "lease" ON "lease"."OperationId" = "job"."OperationId"
           WHERE "job"."OperationId" = ? AND "job"."AddonMediaListId" = ?
             AND "job"."Kind" = ? AND "job"."Phase" = ?
             AND "job"."SavedDraftRevision" = "list"."DraftRevision"
             AND "job"."SavedPublishedRevision" = "list"."PublishedRevision"
             AND "lease"."OwnerToken" = ? AND "lease"."LeaseOwner" = ?
             AND "lease"."LeaseExpiresAt" > ?"#,
    )
    .bind(operation_id)
    .bind(list_id)
    .bind(encode_enum(
        ersatzrs_domain::AddonMediaListLifecycleJobKind::Reconcile,
    ))
    .bind(encode_enum(AddonMediaListLifecyclePhase::EnumeratingSource))
    .bind(owner_token)
    .bind(lease_owner)
    .bind(&now)
    .fetch_optional(&mut *tx)
    .await
    .repository()?
    .ok_or_else(|| {
        RepositoryError::Conflict(
            "the reconcile lease, phase, or revision fence changed".to_owned(),
        )
    })?;
    if let Some(existing) = lifecycle
        .try_get::<Option<EntityId>, _>("CandidateImportJobId")
        .repository()?
    {
        let valid: bool = sqlx::query_scalar(
            r#"SELECT EXISTS (
                   SELECT 1 FROM "AddonMediaListImportJob"
                   WHERE "Id" = ? AND "AddonMediaListId" = ?
                     AND "Purpose" = ? AND "Mode" = ?
               )"#,
        )
        .bind(existing)
        .bind(list_id)
        .bind(encode_enum(AddonMediaListImportPurpose::Reconcile))
        .bind(encode_enum(AddonMediaListImportMode::Full))
        .fetch_one(&mut *tx)
        .await
        .repository()?;
        if !valid {
            return Err(RepositoryError::Conflict(
                "the reconcile candidate import no longer matches its lifecycle job".to_owned(),
            ));
        }
        tx.commit().await.repository()?;
        return Ok(existing);
    }

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
               "CheckpointId" = NULL, "UpdatedAt" = ?
           WHERE "AddonMediaListId" = ? AND "State" IN (?, ?, ?, ?, ?, ?)"#,
    )
    .bind(encode_enum(AddonMediaListImportState::Superseded))
    .bind(&now)
    .bind(list_id)
    .bind(encode_enum(AddonMediaListImportState::Queued))
    .bind(encode_enum(AddonMediaListImportState::Discovering))
    .bind(encode_enum(AddonMediaListImportState::Verifying))
    .bind(encode_enum(AddonMediaListImportState::Paused))
    .bind(encode_enum(AddonMediaListImportState::NeedsAttention))
    .bind(encode_enum(AddonMediaListImportState::Cancelled))
    .execute(&mut *tx)
    .await
    .repository()?;
    let result = sqlx::query(
        r#"INSERT INTO "AddonMediaListImportJob" (
               "AddonMediaListId", "Generation", "State", "RecordCapability",
               "ImportCapability", "Purpose", "Mode", "IncrementalStrategy",
               "CreatedAt", "UpdatedAt", "LastProgressAt"
           ) VALUES (?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?)"#,
    )
    .bind(list_id)
    .bind(generation)
    .bind(encode_enum(AddonMediaListImportState::Queued))
    .bind(record_capability)
    .bind(import_capability)
    .bind(encode_enum(AddonMediaListImportPurpose::Reconcile))
    .bind(encode_enum(AddonMediaListImportMode::Full))
    .bind(&now)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .repository()?;
    let import_job_id = inserted_id(&mut tx, result).await.repository()?;
    let linked = sqlx::query(
        r#"UPDATE "AddonMediaListLifecycleJob"
           SET "CandidateImportJobId" = ?, "UpdatedAt" = ?
           WHERE "OperationId" = ? AND "AddonMediaListId" = ?
             AND "Kind" = ? AND "Phase" = ? AND "CandidateImportJobId" IS NULL"#,
    )
    .bind(import_job_id)
    .bind(&now)
    .bind(operation_id)
    .bind(list_id)
    .bind(encode_enum(
        ersatzrs_domain::AddonMediaListLifecycleJobKind::Reconcile,
    ))
    .bind(encode_enum(AddonMediaListLifecyclePhase::EnumeratingSource))
    .execute(&mut *tx)
    .await
    .repository()?;
    if linked.rows_affected() != 1 {
        return Err(RepositoryError::Conflict(
            "the reconcile lifecycle changed before its candidate was linked".to_owned(),
        ));
    }
    tx.commit().await.repository()?;
    Ok(import_job_id)
}

impl AddonMediaListImportCoordinator {
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
                      "DetailImportPolicy", "DraftRevision"
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
        let revision: i32 = row.try_get("DraftRevision").repository()?;
        if self
            .editor_revision
            .is_some_and(|expected| expected != revision)
        {
            return Err(RepositoryError::Conflict(
                "the saved draft changed before provider confirmation".into(),
            ));
        }
        let current_addon_id: String = row.try_get("AddonId").repository()?;
        let addon_id = addon_id.trim();
        if current_addon_id == addon_id {
            let _ = self
                .cleanup_unreferenced_import_archives_for_list(list_id)
                .await;
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
            AddonMediaListRemoteSynchronizer::new(self.pool.clone(), self.provider.clone())
                .with_editor_revision(self.editor_revision)
                .change_addon(list_id, addon_id)
                .await?;
            let _ = self
                .cleanup_unreferenced_import_archives_for_list(list_id)
                .await;
            return Ok(());
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
            sqlx::query(
                r#"SELECT "Id", "AddonId", "ReviewState", "PublicationJournalJson", "DraftRevision"
                   FROM "AddonMediaList" WHERE "Id" = ?"#,
            )
            .bind(list_id)
            .fetch_optional(&mut *tx)
            .await
            .repository()?
        } else {
            sqlx::query(
                r#"SELECT "Id", "AddonId", "ReviewState", "PublicationJournalJson", "DraftRevision" FROM "AddonMediaList"
                   WHERE "AddonId" = ? AND "SourceUrl" = ?
                   ORDER BY "Id" LIMIT 1"#,
            )
            .bind(addon_id)
            .bind(&context.source_url)
            .fetch_optional(&mut *tx)
            .await
            .repository()?
        };
        let (list_id, same_addon) = if let Some(row) = existing {
            let revision: i32 = row.try_get("DraftRevision").repository()?;
            if self
                .editor_revision
                .is_some_and(|expected| expected != revision)
            {
                return Err(RepositoryError::Conflict(
                    "the saved draft changed during provider validation".into(),
                ));
            }
            let id: EntityId = row.try_get("Id").repository()?;
            let previous_addon_id: String = row.try_get("AddonId").repository()?;
            let active_finalization: bool = sqlx::query_scalar(
                r#"SELECT EXISTS (
                       SELECT 1 FROM "AddonMediaListImportJob"
                       WHERE "AddonMediaListId" = ? AND "OverviewComplete" = 1
                         AND "LeaseOwner" IS NOT NULL
                   )"#,
            )
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .repository()?;
            if active_finalization {
                return Err(RepositoryError::Conflict(
                    "the add-on media list import is finalizing; wait for it to finish".to_owned(),
                ));
            }
            let review_state: i32 = row.try_get("ReviewState").repository()?;
            let publication_journal: Option<String> =
                row.try_get("PublicationJournalJson").repository()?;
            let active_publication: bool = sqlx::query_scalar(
                r#"SELECT EXISTS (
                       SELECT 1
                       FROM "AddonMediaListPublicationJob" AS "job"
                       JOIN "ManagedLibraryOperation" AS "operation"
                         ON "operation"."Id" = "job"."OperationId"
                       WHERE "job"."AddonMediaListId" = ?
                         AND "operation"."State" IN (?, ?, ?)
                   )"#,
            )
            .bind(id)
            .bind(encode_enum(
                ersatzrs_domain::ManagedLibraryOperationState::Queued,
            ))
            .bind(encode_enum(
                ersatzrs_domain::ManagedLibraryOperationState::Running,
            ))
            .bind(encode_enum(
                ersatzrs_domain::ManagedLibraryOperationState::WaitingToRetry,
            ))
            .fetch_one(&mut *tx)
            .await
            .repository()?;
            if review_state == 3
                || active_publication
                || publication_journal
                    .as_deref()
                    .is_some_and(|journal| !journal.trim().is_empty())
            {
                return Err(RepositoryError::Conflict(
                    "the add-on media list is being published; wait for publication to finish"
                        .to_owned(),
                ));
            }
            let updated = sqlx::query(
                r#"UPDATE "AddonMediaList"
                   SET "AddonId" = ?, "ContentKindOverride" = ?, "DetailImportPolicy" = ?,
                       "LastError" = NULL,
                       "DraftRevision" = "DraftRevision" + CASE
                           WHEN "AddonId" <> ? OR "ContentKindOverride" <> ?
                                OR "DetailImportPolicy" <> ? THEN 1 ELSE 0 END
                   WHERE "Id" = ? AND "ReviewState" <> 3
                     AND ("PublicationJournalJson" IS NULL
                          OR TRIM("PublicationJournalJson") = '')
                     AND NOT EXISTS (
                         SELECT 1
                         FROM "AddonMediaListPublicationJob" AS "job"
                         JOIN "ManagedLibraryOperation" AS "operation"
                           ON "operation"."Id" = "job"."OperationId"
                         WHERE "job"."AddonMediaListId" = "AddonMediaList"."Id"
                           AND "operation"."State" IN (?, ?, ?)
                     )"#,
            )
            .bind(addon_id)
            .bind(encode_enum(content_kind_override))
            .bind(encode_enum(detail_policy))
            .bind(addon_id)
            .bind(encode_enum(content_kind_override))
            .bind(encode_enum(detail_policy))
            .bind(id)
            .bind(encode_enum(
                ersatzrs_domain::ManagedLibraryOperationState::Queued,
            ))
            .bind(encode_enum(
                ersatzrs_domain::ManagedLibraryOperationState::Running,
            ))
            .bind(encode_enum(
                ersatzrs_domain::ManagedLibraryOperationState::WaitingToRetry,
            ))
            .execute(&mut *tx)
            .await
            .repository()?;
            if updated.rows_affected() != 1 {
                return Err(RepositoryError::Conflict(
                    "the add-on media list is being published; wait for publication to finish"
                        .to_owned(),
                ));
            }
            crate::addon_media_list_sync::reconcile_changed_addon_state_in_tx(
                &mut tx,
                id,
                &previous_addon_id,
                addon_id,
            )
            .await?;
            (id, previous_addon_id == addon_id)
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
                    "DetailImportPolicy", "RefreshPublicationPolicy"
                   ) VALUES (?, ?, ?, ?, NULL, 0, 0, 0, NULL, NULL, NULL, NULL, ?, NULL, '{}', '{}', ?, 2)"#,
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
            let id = inserted_id(&mut tx, result).await.repository()?;
            crate::repositories::SqliteAddonMediaListRepository::initialize_option_defaults_in_tx(
                &mut tx, id,
            )
            .await?;
            (id, false)
        };

        if let Some(mode) = self.delivery_mode {
            let previous = crate::addon_media_list_delivery::load_in_tx(&mut tx, list_id).await?;
            crate::addon_media_list_delivery::save_in_tx(&mut tx, list_id, mode, Utc::now())
                .await?;
            if previous.mode != mode {
                sqlx::query(
                    r#"UPDATE "AddonMediaList" SET "DraftRevision" = "DraftRevision" + 1
                    WHERE "Id" = ?"#,
                )
                .bind(list_id)
                .execute(&mut *tx)
                .await
                .repository()?;
            }
        }
        if !self.scheduled_refresh {
            crate::addon_media_list_auto_publication::manual_refresh_in_tx(&mut tx, list_id)
                .await?;
        }
        if same_addon && context.import_capability == CAPABILITY_MEDIA_LIST_IMPORT_V2 {
            let active_v2: bool = sqlx::query_scalar(
                r#"SELECT EXISTS (
                       SELECT 1 FROM "AddonMediaListImportJob"
                       WHERE "AddonMediaListId" = ? AND "ImportCapability" = ?
                         AND "State" IN (?, ?, ?, ?, ?)
                   )"#,
            )
            .bind(list_id)
            .bind(CAPABILITY_MEDIA_LIST_IMPORT_V2)
            .bind(encode_enum(AddonMediaListImportState::Queued))
            .bind(encode_enum(AddonMediaListImportState::Discovering))
            .bind(encode_enum(AddonMediaListImportState::Verifying))
            .bind(encode_enum(AddonMediaListImportState::Paused))
            .bind(encode_enum(AddonMediaListImportState::NeedsAttention))
            .fetch_one(&mut *tx)
            .await
            .repository()?;
            if active_v2 {
                tx.commit().await.repository()?;
                return Ok(list_id);
            }
        }

        let has_checkpoint =
            if same_addon && context.import_capability == CAPABILITY_MEDIA_LIST_IMPORT_V2 {
                sqlx::query_scalar::<_, bool>(
                    r#"SELECT EXISTS (
                       SELECT 1 FROM "AddonMediaListImportCheckpoint"
                       WHERE "AddonMediaListId" = ? AND "Committed" = 1
                   )"#,
                )
                .bind(list_id)
                .fetch_one(&mut *tx)
                .await
                .repository()?
            } else {
                false
            };
        let mode = if has_checkpoint {
            AddonMediaListImportMode::Incremental
        } else {
            AddonMediaListImportMode::Full
        };
        let retained_header = if mode == AddonMediaListImportMode::Incremental {
            sqlx::query_scalar::<_, String>(
                r#"SELECT "job"."ListRecordJson"
                   FROM "AddonMediaListImportJob" AS "job"
                   JOIN "AddonMediaListImportCheckpoint" AS "checkpoint"
                     ON "checkpoint"."Id" = "job"."CheckpointId"
                    AND "checkpoint"."Committed" = 1
                   WHERE "job"."AddonMediaListId" = ? AND "job"."State" = ?
                     AND "job"."ListRecordJson" IS NOT NULL
                   ORDER BY "job"."Generation" DESC LIMIT 1"#,
            )
            .bind(list_id)
            .bind(encode_enum(AddonMediaListImportState::Ready))
            .fetch_optional(&mut *tx)
            .await
            .repository()?
        } else {
            None
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
                   "CheckpointId" = NULL, "UpdatedAt" = ?
               WHERE "AddonMediaListId" = ? AND "State" IN (?, ?, ?, ?, ?, ?)"#,
        )
        .bind(encode_enum(AddonMediaListImportState::Superseded))
        .bind(&now)
        .bind(list_id)
        .bind(encode_enum(AddonMediaListImportState::Queued))
        .bind(encode_enum(AddonMediaListImportState::Discovering))
        .bind(encode_enum(AddonMediaListImportState::Verifying))
        .bind(encode_enum(AddonMediaListImportState::Paused))
        .bind(encode_enum(AddonMediaListImportState::NeedsAttention))
        .bind(encode_enum(AddonMediaListImportState::Cancelled))
        .execute(&mut *tx)
        .await
        .repository()?;
        let inserted = sqlx::query(
            r#"INSERT INTO "AddonMediaListImportJob" (
                "AddonMediaListId", "Generation", "State", "RecordCapability",
                "ImportCapability", "Purpose", "Mode", "IncrementalStrategy",
                "ListRecordJson", "RequestedFolder", "CreatedAt", "UpdatedAt", "LastProgressAt"
               ) VALUES (?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?, ?, ?)"#,
        )
        .bind(list_id)
        .bind(generation)
        .bind(encode_enum(AddonMediaListImportState::Queued))
        .bind(&context.record_capability)
        .bind(&context.import_capability)
        .bind(encode_enum(AddonMediaListImportPurpose::Refresh))
        .bind(encode_enum(mode))
        .bind(retained_header)
        .bind(requested_folder)
        .bind(&now)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .repository()?;
        let import_id = inserted_id(&mut tx, inserted).await.repository()?;
        if self.scheduled_refresh {
            crate::addon_media_list_auto_publication::admit_in_tx(
                &mut tx,
                list_id,
                Some(import_id),
            )
            .await?;
        }
        tx.commit().await.repository()?;
        let _ = self
            .cleanup_unreferenced_import_archives_for_list(list_id)
            .await;
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
        let _ = self.cleanup_one_unreferenced_import_archive(None).await;
        let Some(job) = self.claim_import_job().await? else {
            return Ok(false);
        };
        if job.overview_complete {
            match self.finalize_import_pass(&job).await {
                Ok(()) => {
                    let _ = self.cleanup_one_unreferenced_import_archive(None).await;
                }
                Err(error) => self.fail_import(&job, &error.to_string(), false).await?,
            }
            return Ok(true);
        }
        let Some(provider_slot) = self.claim_import_provider_slot(&job).await? else {
            return Ok(false);
        };
        let context = match self
            .provider
            .import_context_for_contract(
                &job.addon_id,
                &job.source_url,
                &job.import_capability,
                &job.record_capability,
            )
            .await
        {
            Ok(Some(context)) => context,
            Ok(None) => {
                self.release_import_provider_slot(&provider_slot).await?;
                self.fail_import(
                    &job,
                    "the installed add-on no longer supports the frozen import capability",
                    false,
                )
                .await?;
                return Ok(true);
            }
            Err(error) => {
                self.release_import_provider_slot(&provider_slot).await?;
                self.fail_import(&job, &error.to_string(), true).await?;
                return Ok(true);
            }
        };
        if job.import_capability == CAPABILITY_MEDIA_LIST_IMPORT_V2 {
            self.process_v2_import_page(&job, &context, &provider_slot)
                .await?;
        } else {
            let result = self
                .provider
                .discover(&job.addon_id, &context, job.cursor.as_deref())
                .await;
            self.release_import_provider_slot(&provider_slot).await?;
            match result {
                Ok(page) => match self
                    .commit_import_page_checkpoint(&job, page.into(), None)
                    .await
                {
                    Ok(PageCommitOutcome::Committed { complete }) if complete => {
                        if let Err(error) = self.finalize_import_pass(&job).await {
                            self.fail_import(&job, &error.to_string(), false).await?;
                        }
                    }
                    Ok(_) => {}
                    Err(error) => self.fail_import(&job, &error.to_string(), false).await?,
                },
                Err(error) => self.fail_import(&job, &error.to_string(), true).await?,
            }
        }
        Ok(true)
    }

    #[allow(clippy::too_many_lines)]
    async fn process_v2_import_page(
        &self,
        job: &ImportJob,
        context: &InstalledMediaListImportContext,
        provider_slot: &ImportProviderSlot,
    ) -> Result<()> {
        let checkpoint = self.resume_import_checkpoint(job).await?;
        let needs_base_checkpoint =
            job.mode == AddonMediaListImportMode::Incremental || job.cursor.is_some();
        if needs_base_checkpoint && checkpoint.is_none() {
            self.release_import_provider_slot(provider_slot).await?;
            self.fail_import(
                job,
                "the incremental import checkpoint is unavailable; reset and reload the list",
                false,
            )
            .await?;
            return Ok(());
        }
        let base_checkpoint = if needs_base_checkpoint {
            checkpoint.as_ref()
        } else {
            None
        };
        let prepared = match prepare_import_archive(
            context.data_dir.clone(),
            job.list_id,
            job.generation,
            base_checkpoint,
        )
        .await
        {
            Ok(prepared) => prepared,
            Err(error) => {
                self.release_import_provider_slot(provider_slot).await?;
                self.fail_import(job, &error.to_string(), false).await?;
                return Ok(());
            }
        };
        let Some(staged_path) = prepared.staged_path().to_str().map(str::to_owned) else {
            prepared.discard().await;
            self.release_import_provider_slot(provider_slot).await?;
            self.fail_import(
                job,
                "the private import archive path is not valid UTF-8",
                false,
            )
            .await?;
            return Ok(());
        };
        let descriptor = MediaListArchiveDescriptor {
            staged_path,
            generation: prepared.generation()?,
            entry_count: prepared.entry_count(),
            sha256: prepared.sha256().to_owned(),
            max_bytes: u32::try_from(MAX_IMPORT_ARCHIVE_BYTES).unwrap_or(u32::MAX),
            max_entries: MAX_IMPORT_ARCHIVE_ENTRIES,
        };
        let mode = match job.mode {
            AddonMediaListImportMode::Full => WireImportMode::Full,
            AddonMediaListImportMode::Incremental => WireImportMode::Incremental,
        };
        let result = self
            .provider
            .discover_v2(
                &job.addon_id,
                context,
                job.cursor.as_deref(),
                mode,
                descriptor,
            )
            .await;
        self.release_import_provider_slot(provider_slot).await?;
        let page = match result {
            Ok(page) => page,
            Err(error) => {
                prepared.discard().await;
                self.fail_import(job, &error.to_string(), true).await?;
                return Ok(());
            }
        };
        let sealed = match prepared.seal().await {
            Ok(sealed) => sealed,
            Err(error) => {
                self.fail_import(job, &error.to_string(), false).await?;
                return Ok(());
            }
        };
        if let Err(error) = validate_archive_advance(&sealed, page.emitted_count) {
            sealed.discard_candidate().await;
            self.fail_import(job, &error.to_string(), false).await?;
            return Ok(());
        }
        let page = ImportPage::from(page);
        match self
            .commit_import_page_checkpoint(job, page, Some(&sealed))
            .await
        {
            Ok(PageCommitOutcome::Cancelled) => sealed.discard_candidate().await,
            Ok(PageCommitOutcome::Committed { complete }) => {
                let _ = self
                    .cleanup_one_unreferenced_import_archive(Some(job.list_id))
                    .await;
                if complete {
                    match self.finalize_import_pass(job).await {
                        Ok(()) => {
                            let _ = self.cleanup_one_unreferenced_import_archive(None).await;
                        }
                        Err(error) => self.fail_import(job, &error.to_string(), false).await?,
                    }
                }
            }
            Err(error) => {
                sealed.discard_candidate().await;
                self.fail_import(job, &error.to_string(), false).await?;
            }
        }
        Ok(())
    }

    async fn current_import_checkpoint(
        &self,
        list_id: EntityId,
    ) -> Result<Option<AddonMediaListImportCheckpoint>> {
        let row = sqlx::query(
            r#"SELECT "Id", "AddonMediaListId", "RelativePath", "Generation",
                      "Sha256", "EntryCount", "PromotedAt"
               FROM "AddonMediaListImportCheckpoint"
               WHERE "AddonMediaListId" = ? AND "Committed" = 1"#,
        )
        .bind(list_id)
        .fetch_optional(&self.pool)
        .await
        .repository()?;
        row.map(|row| decode_import_checkpoint(&row)).transpose()
    }

    async fn resume_import_checkpoint(
        &self,
        job: &ImportJob,
    ) -> Result<Option<AddonMediaListImportCheckpoint>> {
        let row = sqlx::query(
            r#"SELECT "checkpoint"."Id", "checkpoint"."AddonMediaListId",
                      "checkpoint"."RelativePath", "checkpoint"."Generation",
                      "checkpoint"."Sha256", "checkpoint"."EntryCount",
                      "checkpoint"."PromotedAt"
               FROM "AddonMediaListImportJob" AS "job"
               JOIN "AddonMediaListImportCheckpoint" AS "checkpoint"
                 ON "checkpoint"."Id" = "job"."CheckpointId"
               WHERE "job"."Id" = ?"#,
        )
        .bind(job.id)
        .fetch_optional(&self.pool)
        .await
        .repository()?;
        if let Some(row) = row {
            return decode_import_checkpoint(&row).map(Some);
        }
        if job.mode == AddonMediaListImportMode::Incremental {
            return self.current_import_checkpoint(job.list_id).await;
        }
        Ok(None)
    }

    async fn cleanup_one_unreferenced_import_archive(
        &self,
        list_id: Option<EntityId>,
    ) -> Result<bool> {
        let row = sqlx::query(
            r#"SELECT "checkpoint"."Id", "checkpoint"."DataRoot",
                      "checkpoint"."RelativePath"
               FROM "AddonMediaListImportCheckpoint" AS "checkpoint"
               WHERE "checkpoint"."Committed" = 0
                 AND (? IS NULL OR "checkpoint"."AddonMediaListId" = ?)
                 AND NOT EXISTS (
                     SELECT 1 FROM "AddonMediaListImportJob" AS "job"
                     WHERE "job"."CheckpointId" = "checkpoint"."Id"
                 )
               ORDER BY "checkpoint"."Id" LIMIT 1"#,
        )
        .bind(list_id)
        .bind(list_id)
        .fetch_optional(&self.pool)
        .await
        .repository()?;
        let Some(row) = row else {
            return Ok(false);
        };
        let id: EntityId = row.try_get("Id").repository()?;
        let data_root: Option<String> = row.try_get("DataRoot").repository()?;
        let relative_path: String = row.try_get("RelativePath").repository()?;
        if let Some(data_root) = data_root {
            remove_relative_archive(std::path::Path::new(&data_root), &relative_path).await?;
        }
        sqlx::query(
            r#"DELETE FROM "AddonMediaListImportCheckpoint"
               WHERE "Id" = ? AND "Committed" = 0
                 AND NOT EXISTS (
                     SELECT 1 FROM "AddonMediaListImportJob"
                     WHERE "CheckpointId" = ?
                 )"#,
        )
        .bind(id)
        .bind(id)
        .execute(&self.pool)
        .await
        .repository()?;
        Ok(true)
    }

    async fn cleanup_unreferenced_import_archives_for_list(&self, list_id: EntityId) -> Result<()> {
        while self
            .cleanup_one_unreferenced_import_archive(Some(list_id))
            .await?
        {}
        Ok(())
    }

    async fn claim_import_provider_slot(
        &self,
        job: &ImportJob,
    ) -> Result<Option<ImportProviderSlot>> {
        let range = load_detail_wait_range(&self.pool).await?;
        let delay = sample_provider_delay_seconds(Uuid::new_v4().as_bytes(), range);
        let provider_host = provider_host(&job.source_url)?;
        let now = Utc::now();
        let now_text = encode_datetime(now);
        let lease_expires = encode_datetime(now + Duration::seconds(IMPORT_LEASE_SECONDS));
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        if let Some(blocked_until) = crate::addon_media_list_refresh::claim_provider_host(
            &mut tx,
            &provider_host,
            &now_text,
            &job.lease_owner,
            &lease_expires,
            delay,
        )
        .await?
        {
            sqlx::query(
                r#"UPDATE "AddonMediaListImportJob"
                   SET "State" = ?, "NextEligibleAt" = ?, "LeaseOwner" = NULL,
                       "LeaseExpiresAt" = NULL, "UpdatedAt" = ?
                   WHERE "Id" = ? AND "LeaseOwner" = ?"#,
            )
            .bind(encode_enum(AddonMediaListImportState::Queued))
            .bind(blocked_until)
            .bind(&now_text)
            .bind(job.id)
            .bind(&job.lease_owner)
            .execute(&mut *tx)
            .await
            .repository()?;
            tx.commit().await.repository()?;
            return Ok(None);
        }
        tx.commit().await.repository()?;
        Ok(Some(ImportProviderSlot {
            provider_host,
            lease_owner: job.lease_owner.clone(),
            delay,
        }))
    }

    async fn release_import_provider_slot(&self, slot: &ImportProviderSlot) -> Result<()> {
        let completed = Utc::now();
        let now = encode_datetime(completed);
        let next = encode_datetime(completed + Duration::seconds(i64::from(slot.delay)));
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        crate::addon_media_list_refresh::release_provider_host_lease(
            &mut tx,
            &slot.provider_host,
            &slot.lease_owner,
            &now,
            &next,
            slot.delay,
        )
        .await?;
        tx.commit().await.repository()?;
        Ok(())
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
            r#"SELECT "job"."Id", "job"."AddonMediaListId", "job"."Generation",
                      "job"."RecordCapability", "job"."ImportCapability", "job"."Purpose",
                      "job"."Mode",
                      "job"."Cursor",
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
            generation: row.try_get("Generation").repository()?,
            addon_id: row.try_get("AddonId").repository()?,
            source_url: row.try_get("SourceUrl").repository()?,
            record_capability: row.try_get("RecordCapability").repository()?,
            import_capability: row.try_get("ImportCapability").repository()?,
            purpose: decode_enum_column("AddonMediaListImportPurpose", &row, "Purpose")?,
            mode: decode_enum_column("AddonMediaListImportMode", &row, "Mode")?,
            cursor: row.try_get("Cursor").repository()?,
            verification_pass,
            expected_identity_hash: row.try_get("ExpectedIdentityHash").repository()?,
            overview_complete: decode_bool_column(&row, "OverviewComplete")?,
            lease_owner,
        };
        tx.commit().await.repository()?;
        Ok(Some(job))
    }

    #[allow(clippy::too_many_lines)] // The page and every cursor/item checkpoint commit atomically.
    async fn commit_import_page_checkpoint(
        &self,
        job: &ImportJob,
        page: ImportPage,
        sealed_archive: Option<&SealedImportArchive>,
    ) -> Result<PageCommitOutcome> {
        let ImportPage {
            complete,
            next_cursor,
            total_hint,
            revision_token,
            records,
            v2,
        } = page;
        let mut list_record = None;
        let mut item_records = Vec::new();
        for record in records {
            match record {
                AddonMediaListRecord::List { .. } => list_record = Some(record),
                AddonMediaListRecord::Item { .. } => item_records.push(record),
            }
        }
        let now = encode_datetime(Utc::now());
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        let current = sqlx::query(
            r#"SELECT "Cursor", "RevisionToken", "ListRecordJson", "DiscoveredCount",
                      "VerificationPass", "IncrementalStrategy", "ExaminedCount",
                      "ArchivedSkippedCount", "EmittedCount", "CheckpointId",
                      "LeaseOwner", "ImportCapability", "Purpose", "Mode",
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
            return Ok(PageCommitOutcome::Cancelled);
        }
        let persisted_lease_owner: Option<String> = current.try_get("LeaseOwner").repository()?;
        let persisted_import_capability: String =
            current.try_get("ImportCapability").repository()?;
        let persisted_mode: AddonMediaListImportMode =
            decode_enum_column("AddonMediaListImportMode", &current, "Mode")?;
        let persisted_purpose: AddonMediaListImportPurpose =
            decode_enum_column("AddonMediaListImportPurpose", &current, "Purpose")?;
        if persisted_lease_owner.as_deref() != Some(job.lease_owner.as_str())
            || persisted_import_capability != job.import_capability
            || persisted_purpose != job.purpose
            || persisted_mode != job.mode
        {
            return Err(RepositoryError::Conflict(
                "the import lease or transport changed while a page was running".to_owned(),
            ));
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
        if (persisted_count > 0 || job.cursor.is_some()) && persisted_revision != revision_token {
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
                if previous != json
                    && !(job.mode == AddonMediaListImportMode::Incremental && job.cursor.is_none())
                {
                    return Err(RepositoryError::Conflict(
                        "the provider list header changed during overview verification".to_owned(),
                    ));
                }
                if previous != json {
                    sqlx::query(
                        r#"UPDATE "AddonMediaListImportJob" SET "ListRecordJson" = ? WHERE "Id" = ?"#,
                    )
                    .bind(json)
                    .bind(job.id)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
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

        if let Some(next_cursor) = next_cursor.as_deref() {
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
        let (strategy, examined_count, archived_skipped_count, emitted_count, checkpoint_id) =
            if let Some(v2) = v2 {
                let sealed = sealed_archive.ok_or_else(|| {
                    RepositoryError::Conflict(
                        "the v2 import page has no sealed archive candidate".to_owned(),
                    )
                })?;
                let strategy = match v2.strategy {
                    WireIncrementalStrategy::SkipKnown => {
                        AddonMediaListIncrementalStrategy::SkipKnown
                    }
                    WireIncrementalStrategy::NewestFirstEarlyStop => {
                        AddonMediaListIncrementalStrategy::NewestFirstEarlyStop
                    }
                };
                let previous_strategy: AddonMediaListIncrementalStrategy = decode_enum_column(
                    "AddonMediaListIncrementalStrategy",
                    &current,
                    "IncrementalStrategy",
                )?;
                if previous_strategy != AddonMediaListIncrementalStrategy::None
                    && previous_strategy != strategy
                {
                    return Err(RepositoryError::Conflict(
                        "the provider changed incremental strategy between pages".to_owned(),
                    ));
                }
                let current_checkpoint_id: Option<EntityId> =
                    current.try_get("CheckpointId").repository()?;
                let expected_checkpoint = sqlx::query(
                    r#"SELECT "Id", "RelativePath", "Generation"
                       FROM "AddonMediaListImportCheckpoint"
                       WHERE "AddonMediaListId" = ?
                         AND ("Id" = ? OR (
                             ? IS NULL AND ? = ? AND "Committed" = 1
                         ))
                       ORDER BY CASE WHEN "Id" = ? THEN 0 ELSE 1 END LIMIT 1"#,
                )
                .bind(job.list_id)
                .bind(current_checkpoint_id)
                .bind(current_checkpoint_id)
                .bind(encode_enum(job.mode))
                .bind(encode_enum(AddonMediaListImportMode::Incremental))
                .bind(current_checkpoint_id)
                .fetch_optional(&mut *tx)
                .await
                .repository()?;
                match (
                    sealed.previous_relative_path.as_deref(),
                    expected_checkpoint.as_ref(),
                ) {
                    (Some(expected), Some(row)) => {
                        let checkpoint_path: String = row.try_get("RelativePath").repository()?;
                        if checkpoint_path != expected {
                            return Err(RepositoryError::Conflict(
                                "the import archive changed while a page was running".to_owned(),
                            ));
                        }
                    }
                    (None, None)
                        if job.mode == AddonMediaListImportMode::Full && job.cursor.is_none() => {}
                    _ if job.mode == AddonMediaListImportMode::Incremental => {
                        return Err(RepositoryError::Conflict(
                            "the incremental import archive changed while a page was running"
                                .to_owned(),
                        ));
                    }
                    _ => {
                        return Err(RepositoryError::Conflict(
                            "the full import archive changed while a page was running".to_owned(),
                        ));
                    }
                }
                let checkpoint_id = if sealed.reused_previous {
                    sqlx::query_scalar(
                        r#"SELECT "Id" FROM "AddonMediaListImportCheckpoint"
                       WHERE "AddonMediaListId" = ? AND "RelativePath" = ?"#,
                    )
                    .bind(job.list_id)
                    .bind(&sealed.relative_path)
                    .fetch_one(&mut *tx)
                    .await
                    .repository()?
                } else {
                    let data_root = sealed.data_root().to_str().ok_or_else(|| {
                        RepositoryError::Conflict(
                            "the private import archive root is not valid UTF-8".to_owned(),
                        )
                    })?;
                    let result = sqlx::query(
                        r#"INSERT INTO "AddonMediaListImportCheckpoint" (
                               "AddonMediaListId", "RelativePath", "Generation", "Sha256",
                               "EntryCount", "PromotedAt", "DataRoot", "Committed"
                           ) VALUES (?, ?, ?, ?, ?, ?, ?, 0)"#,
                    )
                    .bind(job.list_id)
                    .bind(&sealed.relative_path)
                    .bind(job.generation)
                    .bind(&sealed.sha256)
                    .bind(i64::from(sealed.entry_count))
                    .bind(encode_datetime(Utc::now()))
                    .bind(data_root)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
                    inserted_id(&mut tx, result).await.repository()?
                };
                let persisted_examined: i64 = current.try_get("ExaminedCount").repository()?;
                let persisted_skipped: i64 =
                    current.try_get("ArchivedSkippedCount").repository()?;
                let persisted_emitted: i64 = current.try_get("EmittedCount").repository()?;
                (
                    strategy,
                    persisted_examined.saturating_add(i64::from(v2.examined_count)),
                    persisted_skipped.saturating_add(i64::from(v2.archived_skipped_count)),
                    persisted_emitted.saturating_add(i64::from(v2.emitted_count)),
                    Some(checkpoint_id),
                )
            } else {
                if sealed_archive.is_some() {
                    return Err(RepositoryError::Conflict(
                        "a v1 import page cannot promote an archive".to_owned(),
                    ));
                }
                (
                    AddonMediaListIncrementalStrategy::None,
                    current.try_get("ExaminedCount").repository()?,
                    current.try_get("ArchivedSkippedCount").repository()?,
                    current.try_get("EmittedCount").repository()?,
                    current.try_get("CheckpointId").repository()?,
                )
            };
        sqlx::query(
            r#"UPDATE "AddonMediaListImportJob"
               SET "Cursor" = ?, "RevisionToken" = ?, "TotalHint" = ?,
                   "DiscoveredCount" = ?, "OverviewComplete" = ?,
                   "IncrementalStrategy" = ?, "ExaminedCount" = ?,
                   "ArchivedSkippedCount" = ?, "EmittedCount" = ?, "CheckpointId" = ?,
                   "Diagnostic" = NULL, "AttemptCount" = 0,
                   "NextEligibleAt" = NULL, "HeartbeatAt" = ?, "LastProgressAt" = ?,
                   "UpdatedAt" = ?,
                   "LeaseOwner" = CASE WHEN ? THEN "LeaseOwner" ELSE NULL END,
                   "LeaseExpiresAt" = CASE WHEN ? THEN "LeaseExpiresAt" ELSE NULL END
               WHERE "Id" = ?"#,
        )
        .bind(next_cursor.as_deref())
        .bind(revision_token.as_deref())
        .bind(total_hint.map(i64::from))
        .bind(discovered_count)
        .bind(complete)
        .bind(encode_enum(strategy))
        .bind(examined_count)
        .bind(archived_skipped_count)
        .bind(emitted_count)
        .bind(checkpoint_id)
        .bind(&now)
        .bind(&now)
        .bind(&now)
        .bind(complete)
        .bind(complete)
        .bind(job.id)
        .execute(&mut *tx)
        .await
        .repository()?;
        tx.commit().await.repository()?;
        Ok(PageCommitOutcome::Committed { complete })
    }

    #[cfg(test)]
    async fn commit_import_page(
        &self,
        job: &ImportJob,
        page: MediaListDiscoveryPage,
    ) -> Result<()> {
        match self
            .commit_import_page_checkpoint(job, page.into(), None)
            .await?
        {
            PageCommitOutcome::Committed { complete: true } => self.finalize_import_pass(job).await,
            PageCommitOutcome::Cancelled | PageCommitOutcome::Committed { complete: false } => {
                Ok(())
            }
        }
    }

    async fn finalize_import_pass(&self, job: &ImportJob) -> Result<()> {
        let row = sqlx::query(
            r#"SELECT CAST(CASE WHEN "OverviewComplete" = 1 THEN 1 ELSE 0 END AS SIGNED)
                          AS "OverviewComplete",
                      "RevisionToken", "LeaseOwner"
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
        let lease_owner: Option<String> = row.try_get("LeaseOwner").repository()?;
        if lease_owner.as_deref() != Some(job.lease_owner.as_str()) {
            return Err(RepositoryError::Conflict(
                "the import lease changed before finalization".to_owned(),
            ));
        }
        let revision_token: Option<String> = row.try_get("RevisionToken").repository()?;
        let now = encode_datetime(Utc::now());
        if job.import_capability == CAPABILITY_MEDIA_LIST_IMPORT_V2 {
            return self
                .promote_complete_overview(job, job.verification_pass)
                .await;
        }
        let identity_hash = self.identity_hash(job.id, job.verification_pass).await?;
        if revision_token.is_none() && job.verification_pass == 0 {
            self.start_unversioned_verification(job, 1, &identity_hash, &now)
                .await?;
            return Ok(());
        }
        if job.verification_pass >= 1
            && job.expected_identity_hash.as_deref() != Some(identity_hash.as_str())
        {
            if job.verification_pass < MAX_UNVERSIONED_VERIFICATION_PASS {
                self.start_unversioned_verification(
                    job,
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
        job: &ImportJob,
        verification_pass: i32,
        expected_identity_hash: &str,
        now: &str,
    ) -> Result<()> {
        let updated = sqlx::query(
            r#"UPDATE "AddonMediaListImportJob"
               SET "State" = ?, "VerificationPass" = ?, "ExpectedIdentityHash" = ?,
                   "Cursor" = NULL, "RevisionToken" = NULL, "DiscoveredCount" = 0,
                   "TotalHint" = NULL, "OverviewComplete" = 0,
                   "FinalizationPhase" = 0,
                   "UpdatedAt" = ?, "LastProgressAt" = ?,
                   "LeaseOwner" = NULL, "LeaseExpiresAt" = NULL
               WHERE "Id" = ? AND "LeaseOwner" = ?"#,
        )
        .bind(encode_enum(AddonMediaListImportState::Verifying))
        .bind(verification_pass)
        .bind(expected_identity_hash)
        .bind(now)
        .bind(now)
        .bind(job.id)
        .bind(&job.lease_owner)
        .execute(&self.pool)
        .await
        .repository()?;
        if updated.rows_affected() != 1 {
            return Err(RepositoryError::Conflict(
                "the import lease changed before verification".to_owned(),
            ));
        }
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
        let mut enumerated =
            decode_media_list_output_for_capability(&output, &job.record_capability)?;
        self.overlay_completed_details(job, pass, &mut enumerated)
            .await?;

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
            refresh_cadence: ersatzrs_domain::AddonMediaListRefreshCadence::Never,
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
        let synchronizer =
            AddonMediaListRemoteSynchronizer::new(self.pool.clone(), self.provider.clone());
        if job.purpose == AddonMediaListImportPurpose::Reconcile {
            synchronizer
                .persist_reconcile_generation(
                    job.id,
                    &job.lease_owner,
                    job.generation,
                    &list,
                    &mut enumerated,
                )
                .await?;
        } else {
            synchronizer
                .persist_import_generation(
                    job.id,
                    &job.lease_owner,
                    &list,
                    &mut enumerated,
                    job.mode == AddonMediaListImportMode::Incremental,
                )
                .await?;
        }
        synchronizer
            .complete_import_projection(
                job.id,
                &job.lease_owner,
                job.list_id,
                requested_folder.as_deref(),
            )
            .await?;
        self.initialize_detail_jobs(job, job.list_id, pass).await?;
        let completed_at = Utc::now();
        let now = encode_datetime(completed_at);
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        let fenced = sqlx::query(
            r#"UPDATE "AddonMediaListImportJob" SET "HeartbeatAt" = ?
               WHERE "Id" = ? AND "LeaseOwner" = ? AND "OverviewComplete" = 1
                 AND "CancellationRequested" = 0 AND "FinalizationPhase" = 4"#,
        )
        .bind(&now)
        .bind(job.id)
        .bind(&job.lease_owner)
        .execute(&mut *tx)
        .await
        .repository()?;
        if fenced.rows_affected() != 1 {
            return Err(RepositoryError::Conflict(
                "the import finalization lease or phase changed".to_owned(),
            ));
        }
        if job.purpose == AddonMediaListImportPurpose::Reconcile {
            let has_candidate: bool = sqlx::query_scalar(
                r#"SELECT EXISTS (
                       SELECT 1
                       FROM "AddonMediaListImportJob" AS "job"
                       JOIN "AddonMediaListImportCheckpoint" AS "checkpoint"
                         ON "checkpoint"."Id" = "job"."CheckpointId"
                       WHERE "job"."Id" = ? AND "job"."LeaseOwner" = ?
                         AND "job"."Purpose" = ? AND "job"."Mode" = ?
                         AND "checkpoint"."Committed" = 0
                   )"#,
            )
            .bind(job.id)
            .bind(&job.lease_owner)
            .bind(encode_enum(AddonMediaListImportPurpose::Reconcile))
            .bind(encode_enum(AddonMediaListImportMode::Full))
            .fetch_one(&mut *tx)
            .await
            .repository()?;
            if !has_candidate {
                return Err(RepositoryError::Conflict(
                    "the completed reconciliation has no matching staged archive".to_owned(),
                ));
            }
            synchronizer
                .finalize_reconcile_evidence_in_tx(
                    &mut tx,
                    job.list_id,
                    job.generation,
                    completed_at,
                )
                .await?;
        }
        if job.import_capability == CAPABILITY_MEDIA_LIST_IMPORT_V2
            && job.purpose != AddonMediaListImportPurpose::Reconcile
        {
            let checkpoint_id: Option<EntityId> = sqlx::query_scalar(
                r#"SELECT "CheckpointId" FROM "AddonMediaListImportJob"
                   WHERE "Id" = ? AND "LeaseOwner" = ? AND "FinalizationPhase" = 4"#,
            )
            .bind(job.id)
            .bind(&job.lease_owner)
            .fetch_optional(&mut *tx)
            .await
            .repository()?
            .flatten();
            let checkpoint_id = checkpoint_id.ok_or_else(|| {
                RepositoryError::Conflict(
                    "the completed v2 import has no resumable archive checkpoint".to_owned(),
                )
            })?;
            sqlx::query(
                r#"UPDATE "AddonMediaListImportJob" SET "CheckpointId" = NULL
                   WHERE "AddonMediaListId" = ? AND "Id" <> ?"#,
            )
            .bind(job.list_id)
            .bind(job.id)
            .execute(&mut *tx)
            .await
            .repository()?;
            sqlx::query(
                r#"UPDATE "AddonMediaListImportCheckpoint" SET "Committed" = 0
                   WHERE "AddonMediaListId" = ? AND "Id" <> ? AND "Committed" = 1"#,
            )
            .bind(job.list_id)
            .bind(checkpoint_id)
            .execute(&mut *tx)
            .await
            .repository()?;
            let promoted = sqlx::query(
                r#"UPDATE "AddonMediaListImportCheckpoint"
                   SET "Committed" = 1, "Generation" = ?, "PromotedAt" = ?
                   WHERE "Id" = ? AND "AddonMediaListId" = ?"#,
            )
            .bind(job.generation)
            .bind(&now)
            .bind(checkpoint_id)
            .bind(job.list_id)
            .execute(&mut *tx)
            .await
            .repository()?;
            if promoted.rows_affected() != 1 {
                return Err(RepositoryError::Conflict(
                    "the v2 import archive checkpoint changed before finalization".to_owned(),
                ));
            }
        }
        let completed = sqlx::query(
            r#"UPDATE "AddonMediaListImportJob"
               SET "State" = ?, "CompletedAt" = ?, "UpdatedAt" = ?,
                   "LastProgressAt" = ?, "LeaseOwner" = NULL, "LeaseExpiresAt" = NULL,
                   "Diagnostic" = NULL, "OverviewComplete" = 0
               WHERE "Id" = ? AND "LeaseOwner" = ? AND "FinalizationPhase" = 4
                 AND "CancellationRequested" = 0"#,
        )
        .bind(encode_enum(AddonMediaListImportState::Ready))
        .bind(&now)
        .bind(&now)
        .bind(&now)
        .bind(job.id)
        .bind(&job.lease_owner)
        .execute(&mut *tx)
        .await
        .repository()?;
        if completed.rows_affected() != 1 {
            return Err(RepositoryError::Conflict(
                "the import lease changed before finalization".to_owned(),
            ));
        }
        crate::addon_media_list_auto_publication::ready_in_tx(&mut tx, job.id).await?;
        tx.commit().await.repository()?;
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
                      "detail"."AttemptCount", "detail"."OptionPurpose", "detail"."OptionContextJson"
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
            let item_id: EntityId = row.try_get("ItemId").repository()?;
            let purpose: i32 = row.try_get("OptionPurpose").repository()?;
            let context: Option<String> = row.try_get("OptionContextJson").repository()?;
            let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
            let valid = crate::addon_media_list_detail_options::validate(
                &mut tx,
                job.list_id,
                item_id,
                purpose,
                context.as_deref(),
            )
            .await;
            tx.commit().await.repository()?;
            if valid.is_err() {
                continue;
            }
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
                option_context: None,
                option_purpose: 0,
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
                lease_owner: None,
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
        job: &ImportJob,
        list_id: EntityId,
        pass: i32,
    ) -> Result<()> {
        let phase: i32 = sqlx::query_scalar(
            r#"SELECT "FinalizationPhase" FROM "AddonMediaListImportJob"
               WHERE "Id" = ? AND "LeaseOwner" = ? AND "OverviewComplete" = 1
                 AND "CancellationRequested" = 0"#,
        )
        .bind(job.id)
        .bind(&job.lease_owner)
        .fetch_optional(&self.pool)
        .await
        .repository()?
        .ok_or_else(|| {
            RepositoryError::Conflict("the import finalization lease changed".to_owned())
        })?;
        if phase == 4 {
            return Ok(());
        }
        if phase != 3 {
            return Err(RepositoryError::Conflict(
                "the import finalization phase is invalid for detail initialization".to_owned(),
            ));
        }
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
        .bind(job.id)
        .bind(pass)
        .fetch_all(&self.pool)
        .await
        .repository()?;
        let now = encode_datetime(Utc::now());
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        let lease_expires = encode_datetime(Utc::now() + Duration::seconds(IMPORT_LEASE_SECONDS));
        let fenced = sqlx::query(
            r#"UPDATE "AddonMediaListImportJob" SET "LeaseExpiresAt" = ?, "HeartbeatAt" = ?
               WHERE "Id" = ? AND "LeaseOwner" = ? AND "OverviewComplete" = 1
                 AND "CancellationRequested" = 0 AND "FinalizationPhase" = 3"#,
        )
        .bind(&lease_expires)
        .bind(&now)
        .bind(job.id)
        .bind(&job.lease_owner)
        .execute(&mut *tx)
        .await
        .repository()?;
        if fenced.rows_affected() != 1 {
            return Err(RepositoryError::Conflict(
                "the import finalization lease or phase changed".to_owned(),
            ));
        }
        for row in rows {
            let item_id: EntityId = row.try_get("Id").repository()?;
            let fingerprint: String = row.try_get("OverviewFingerprint").repository()?;
            let record_json: String = row.try_get("RecordJson").repository()?;
            let existing = sqlx::query(
                r#"SELECT "OverviewFingerprint", "State", "OptionPurpose", "OptionContextJson"
                   FROM "AddonMediaListDetailJob" WHERE "AddonMediaListItemId" = ?"#,
            )
            .bind(item_id)
            .fetch_optional(&mut *tx)
            .await
            .repository()?;
            if let Some(existing) = existing {
                let previous: String = existing.try_get("OverviewFingerprint").repository()?;
                let purpose: i32 = existing.try_get("OptionPurpose").repository()?;
                let context: Option<String> = existing.try_get("OptionContextJson").repository()?;
                let options_current = crate::addon_media_list_detail_options::validate(
                    &mut tx,
                    list_id,
                    item_id,
                    purpose,
                    context.as_deref(),
                )
                .await
                .is_ok();
                if previous == fingerprint && options_current {
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
                               "Outcome" = NULL, "ResultRecordJson" = NULL, "OptionContextJson" = NULL, "OptionPurpose" = 0,
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
        let advanced = sqlx::query(
            r#"UPDATE "AddonMediaListImportJob" SET "FinalizationPhase" = 4, "UpdatedAt" = ?
               WHERE "Id" = ? AND "LeaseOwner" = ? AND "OverviewComplete" = 1
                 AND "CancellationRequested" = 0 AND "FinalizationPhase" = 3"#,
        )
        .bind(&now)
        .bind(job.id)
        .bind(&job.lease_owner)
        .execute(&mut *tx)
        .await
        .repository()?;
        if advanced.rows_affected() != 1 {
            return Err(RepositoryError::Conflict(
                "the import finalization lease or phase changed".to_owned(),
            ));
        }
        tx.commit().await.repository()?;
        Ok(())
    }

    async fn fail_import(&self, job: &ImportJob, diagnostic: &str, retryable: bool) -> Result<()> {
        let now = Utc::now();
        let now_text = encode_datetime(now);
        let row = sqlx::query(
            r#"SELECT "AttemptCount", "AddonMediaListId"
               FROM "AddonMediaListImportJob"
               WHERE "Id" = ? AND "LeaseOwner" = ? AND "LeaseExpiresAt" > ?
                 AND "State" IN (?, ?)"#,
        )
        .bind(job.id)
        .bind(&job.lease_owner)
        .bind(&now_text)
        .bind(encode_enum(AddonMediaListImportState::Discovering))
        .bind(encode_enum(AddonMediaListImportState::Verifying))
        .fetch_optional(&self.pool)
        .await
        .repository()?;
        let Some(row) = row else {
            return Ok(());
        };
        let attempts: i32 = row.try_get("AttemptCount").repository()?;
        let list_id: Option<EntityId> = row.try_get("AddonMediaListId").repository()?;
        let attempts = attempts.saturating_add(1);
        let retry = retryable && attempts < MAX_IMPORT_ATTEMPTS;
        let retry_at =
            retry.then(|| encode_datetime(now + Duration::seconds(import_retry_seconds(attempts))));
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        let updated = sqlx::query(
            r#"UPDATE "AddonMediaListImportJob"
               SET "State" = ?, "AttemptCount" = ?, "Diagnostic" = ?,
                   "NextEligibleAt" = ?, "LeaseOwner" = NULL, "LeaseExpiresAt" = NULL,
                   "UpdatedAt" = ?
               WHERE "Id" = ? AND "LeaseOwner" = ? AND "LeaseExpiresAt" > ?
                 AND "State" IN (?, ?)"#,
        )
        .bind(encode_enum(if retry {
            AddonMediaListImportState::Queued
        } else {
            AddonMediaListImportState::NeedsAttention
        }))
        .bind(attempts)
        .bind(bound_diagnostic(diagnostic))
        .bind(retry_at.as_deref())
        .bind(&now_text)
        .bind(job.id)
        .bind(&job.lease_owner)
        .bind(&now_text)
        .bind(encode_enum(AddonMediaListImportState::Discovering))
        .bind(encode_enum(AddonMediaListImportState::Verifying))
        .execute(&mut *tx)
        .await
        .repository()?;
        if updated.rows_affected() != 1 {
            tx.rollback().await.repository()?;
            return Ok(());
        }
        if let Some(list_id) = list_id {
            sqlx::query(r#"UPDATE "AddonMediaList" SET "RefreshRetryAt" = ? WHERE "Id" = ?"#)
                .bind(retry_at.as_deref())
                .bind(list_id)
                .execute(&mut *tx)
                .await
                .repository()?;
        }
        tx.commit().await.repository()?;
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

    /// Queue saved-draft detail work before publication, including already published items.
    ///
    /// # Errors
    /// Rejects missing item context and database failures.
    pub async fn request_prepublication_details(
        &self,
        list_id: EntityId,
        item_id: EntityId,
    ) -> Result<()> {
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        let context =
            crate::addon_media_list_detail_options::capture(&mut tx, list_id, item_id, 1).await?;
        // An identical request preserves its checkpoint, retry delay and active lease.
        sqlx::query(
            r#"UPDATE "AddonMediaListDetailJob"
            SET "OptionPurpose"=1, "OptionContextJson"=?, "State"=?, "ManualPriority"=1,
                "ResultRecordJson"=NULL, "Outcome"=NULL, "LeaseOwner"=NULL, "LeaseExpiresAt"=NULL,
                "AttemptCount"=0, "NextEligibleAt"=NULL, "Diagnostic"=NULL, "UpdatedAt"=?
            WHERE "AddonMediaListId"=? AND "AddonMediaListItemId"=?
              AND ("OptionPurpose"<>1 OR "OptionContextJson" IS NULL OR "OptionContextJson"<>?)"#,
        )
        .bind(&context)
        .bind(encode_enum(AddonMediaListDetailState::Queued))
        .bind(encode_datetime(Utc::now()))
        .bind(list_id)
        .bind(item_id)
        .bind(&context)
        .execute(&mut *tx)
        .await
        .repository()?;
        tx.commit().await.repository()?;
        Ok(())
    }

    async fn resolve_detail_options(
        &self,
        job: &DetailJob,
    ) -> Result<(
        Option<crate::addon_media_list_options::ResolvedAddonItemOptions>,
        ersatzrs_domain::RemoteStreamLiveness,
    )> {
        let context = job.option_context.as_deref().ok_or_else(|| {
            RepositoryError::Conflict("the detail option context is missing".into())
        })?;
        let resolved = crate::addon_media_list_detail_options::resolve(
            &self.pool,
            job.list_id,
            job.item_id,
            context,
        )
        .await?;
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        crate::addon_media_list_detail_options::validate(
            &mut tx,
            job.list_id,
            job.item_id,
            job.option_purpose,
            Some(context),
        )
        .await?;
        tx.commit().await.repository()?;
        Ok(resolved)
    }

    /// Queue one manual request without bypassing the shared host cooldown.
    ///
    /// # Errors
    ///
    /// Returns an error when the item has no durable detail job or its request
    /// state cannot be persisted.
    pub async fn request_item_details(&self, list_id: EntityId, item_id: EntityId) -> Result<()> {
        let now = encode_datetime(Utc::now());
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        let context =
            crate::addon_media_list_detail_options::capture(&mut tx, list_id, item_id, 0).await?;
        sqlx::query(r#"UPDATE "AddonMediaListDetailJob"
            SET "OptionPurpose"=0, "OptionContextJson"=?, "State"=?, "ResultRecordJson"=NULL,
                "Outcome"=NULL, "LeaseOwner"=NULL, "LeaseExpiresAt"=NULL, "AttemptCount"=0
            WHERE "AddonMediaListId"=? AND "AddonMediaListItemId"=?
              AND ("OptionPurpose"<>0 OR ("OptionContextJson" IS NOT NULL AND "OptionContextJson"<>?))"#)
            .bind(&context).bind(encode_enum(AddonMediaListDetailState::Queued)).bind(list_id).bind(item_id).bind(&context)
            .execute(&mut *tx).await.repository()?;
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
        .execute(&mut *tx)
        .await
        .repository()?;
        if updated.rows_affected() != 1 {
            return Err(RepositoryError::NotFound {
                kind: "add-on media-list item detail job",
                id: item_id,
            });
        }
        tx.commit().await.repository()?;
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
        if action == AddonMediaListImportAction::Retry {
            return self.retry_import(list_id).await;
        }
        let now = encode_datetime(Utc::now());
        let (state, cancellation) = match action {
            AddonMediaListImportAction::Pause => (AddonMediaListImportState::Paused, false),
            AddonMediaListImportAction::Resume => (AddonMediaListImportState::Queued, false),
            AddonMediaListImportAction::Cancel => (AddonMediaListImportState::Cancelled, true),
            AddonMediaListImportAction::Retry => unreachable!("retry returned above"),
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
               ) AND NOT ("OverviewComplete" = 1 AND "LeaseOwner" IS NOT NULL)"#,
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
            let finalizing: bool = sqlx::query_scalar(
                r#"SELECT EXISTS (
                       SELECT 1 FROM "AddonMediaListImportJob"
                       WHERE "Id" = (
                           SELECT "Id" FROM "AddonMediaListImportJob"
                           WHERE "AddonMediaListId" = ?
                           ORDER BY "Generation" DESC LIMIT 1
                       ) AND "OverviewComplete" = 1 AND "LeaseOwner" IS NOT NULL
                   )"#,
            )
            .bind(list_id)
            .fetch_one(&self.pool)
            .await
            .repository()?;
            if finalizing {
                return Err(RepositoryError::Conflict(
                    "the add-on media list import is finalizing; wait for it to finish".to_owned(),
                ));
            }
            return Err(RepositoryError::NotFound {
                kind: "add-on media-list import",
                id: list_id,
            });
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    async fn retry_import(&self, list_id: EntityId) -> Result<()> {
        let now = encode_datetime(Utc::now());
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        let previous = sqlx::query(
            r#"SELECT "Id", "Generation", "State", "RecordCapability", "ImportCapability",
                      "Purpose", "Mode", "ListRecordJson", "RequestedFolder", "DiscoveredCount",
                      "OverviewComplete", "FinalizationPhase"
               FROM "AddonMediaListImportJob"
               WHERE "AddonMediaListId" = ? ORDER BY "Generation" DESC LIMIT 1"#,
        )
        .bind(list_id)
        .fetch_optional(&mut *tx)
        .await
        .repository()?
        .ok_or(RepositoryError::NotFound {
            kind: "add-on media-list import",
            id: list_id,
        })?;
        let previous_id: EntityId = previous.try_get("Id").repository()?;
        let previous_generation: i32 = previous.try_get("Generation").repository()?;
        let previous_state = decode_enum_column("AddonMediaListImportState", &previous, "State")?;
        let previous_purpose: AddonMediaListImportPurpose =
            decode_enum_column("AddonMediaListImportPurpose", &previous, "Purpose")?;
        let discovered_count: i32 = previous.try_get("DiscoveredCount").repository()?;
        let retryable = matches!(
            previous_state,
            AddonMediaListImportState::NeedsAttention | AddonMediaListImportState::Cancelled
        ) || (previous_state == AddonMediaListImportState::Ready
            && discovered_count == 0
            && previous_purpose != AddonMediaListImportPurpose::Reconcile);
        if !retryable {
            return Err(RepositoryError::Conflict(
                "only failed, cancelled, or empty completed imports can be retried".to_owned(),
            ));
        }
        let import_capability: String = previous.try_get("ImportCapability").repository()?;
        let overview_complete: bool = previous.try_get("OverviewComplete").repository()?;
        let finalization_phase: i32 = previous.try_get("FinalizationPhase").repository()?;
        if (import_capability == CAPABILITY_MEDIA_LIST_IMPORT_V2
            || overview_complete
            || finalization_phase > 0)
            && matches!(
                previous_state,
                AddonMediaListImportState::NeedsAttention | AddonMediaListImportState::Cancelled
            )
        {
            let resumed = sqlx::query(
                r#"UPDATE "AddonMediaListImportJob"
                   SET "State" = ?, "CancellationRequested" = 0, "AttemptCount" = 0,
                       "Diagnostic" = NULL, "NextEligibleAt" = NULL,
                       "LeaseOwner" = NULL, "LeaseExpiresAt" = NULL,
                       "UpdatedAt" = ?, "LastProgressAt" = ?
                   WHERE "Id" = ? AND "State" = ?"#,
            )
            .bind(encode_enum(AddonMediaListImportState::Queued))
            .bind(&now)
            .bind(&now)
            .bind(previous_id)
            .bind(encode_enum(previous_state))
            .execute(&mut *tx)
            .await
            .repository()?;
            if resumed.rows_affected() != 1 {
                return Err(RepositoryError::Conflict(
                    "the import changed while it was being retried".to_owned(),
                ));
            }
            sqlx::query(r#"UPDATE "AddonMediaList" SET "LastError" = NULL WHERE "Id" = ?"#)
                .bind(list_id)
                .execute(&mut *tx)
                .await
                .repository()?;
            tx.commit().await.repository()?;
            return Ok(());
        }
        let generation = previous_generation.checked_add(1).ok_or_else(|| {
            RepositoryError::Conflict("the import generation limit was reached".to_owned())
        })?;
        let record_capability: String = previous.try_get("RecordCapability").repository()?;
        let mode: i32 = previous.try_get("Mode").repository()?;
        let retained_header: Option<String> = previous.try_get("ListRecordJson").repository()?;
        let requested_folder: Option<String> = previous.try_get("RequestedFolder").repository()?;
        let superseded = sqlx::query(
            r#"UPDATE "AddonMediaListImportJob"
               SET "State" = ?, "LeaseOwner" = NULL, "LeaseExpiresAt" = NULL,
                   "UpdatedAt" = ?
               WHERE "Id" = ? AND "State" = ?"#,
        )
        .bind(encode_enum(AddonMediaListImportState::Superseded))
        .bind(&now)
        .bind(previous_id)
        .bind(encode_enum(previous_state))
        .execute(&mut *tx)
        .await
        .repository()?;
        if superseded.rows_affected() != 1 {
            return Err(RepositoryError::Conflict(
                "the import changed while it was being retried".to_owned(),
            ));
        }
        sqlx::query(
            r#"INSERT INTO "AddonMediaListImportJob" (
                "AddonMediaListId", "Generation", "State", "RecordCapability",
                "ImportCapability", "Purpose", "Mode", "ListRecordJson", "RequestedFolder",
                "CreatedAt", "UpdatedAt", "LastProgressAt"
               ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(list_id)
        .bind(generation)
        .bind(encode_enum(AddonMediaListImportState::Queued))
        .bind(record_capability)
        .bind(import_capability)
        .bind(encode_enum(AddonMediaListImportPurpose::Refresh))
        .bind(mode)
        .bind(retained_header)
        .bind(requested_folder)
        .bind(&now)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .repository()?;
        sqlx::query(r#"UPDATE "AddonMediaList" SET "LastError" = NULL WHERE "Id" = ?"#)
            .bind(list_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        tx.commit().await.repository()?;
        Ok(())
    }

    /// Process at most one eligible detail job.
    ///
    /// # Errors
    ///
    /// Returns an error when claiming, invoking, checkpointing, or merging the
    /// selected detail job fails.
    pub async fn process_one_detail(&self) -> Result<bool> {
        self.requeue_one_changed_package_detail().await?;
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
        let resolved = match self.resolve_detail_options(&job).await {
            Ok(options) => options,
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
                resolved.0.as_ref().map(|options| (options, resolved.1)),
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
                    let outcome = result.outcome;
                    self.checkpoint_detail_result(&job, outcome, result.code.as_deref(), &item)
                        .await?;
                }
                MediaListEnrichOutcomeKind::TransientFailure
                | MediaListEnrichOutcomeKind::PermanentFailure => {
                    let retryable = result.outcome == MediaListEnrichOutcomeKind::TransientFailure;
                    let diagnostic = result
                        .message
                        .as_deref()
                        .or(result.code.as_deref())
                        .unwrap_or(if retryable {
                            "the provider requested a retry"
                        } else {
                            "the provider rejected this item"
                        });
                    let retry_after = retryable.then_some(result.retry_after_seconds).flatten();
                    self.finish_detail_failure(&job, diagnostic, retryable, retry_after)
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

    /// A package can fix metadata mapping without changing overview identities.
    /// Reuse existing jobs and the shared cooldown; never reopen current-package failures.
    async fn requeue_one_changed_package_detail(&self) -> Result<()> {
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        let row: Option<(i64,i64)> = sqlx::query_as(r#"SELECT "detail"."AddonMediaListItemId", "list"."Id"
            FROM "AddonMediaListDetailJob" AS "detail"
            JOIN "AddonMediaList" AS "list" ON "list"."Id"="detail"."AddonMediaListId"
            JOIN "AddonMediaListItem" AS "item" ON "item"."Id"="detail"."AddonMediaListItemId"
            JOIN "AddonInstallation" AS "addon" ON "addon"."AddonId"="list"."AddonId"
            WHERE "list"."RefreshPublicationPolicy"=2 AND "list"."DetailImportPolicy"=0
              AND "list"."DetailImportAuthorizedAt" IS NOT NULL AND "addon"."Enabled"=1
              AND "list"."ReviewState"<>3 AND "list"."PublicationJournalJson" IS NULL
              AND "item"."Included"=1 AND "item"."Availability"<>1 AND "detail"."State" IN (5,6,7)
              AND json_valid("detail"."OptionContextJson")
              AND json_extract("detail"."OptionContextJson",'$.package_content') IS NOT NULL
              AND json_extract("detail"."OptionContextJson",'$.package_content')<>"addon"."ContentSha256"
              AND NOT EXISTS (SELECT 1 FROM "AddonMediaListPublicationJob" AS "job"
                JOIN "ManagedLibraryOperation" AS "operation" ON "operation"."Id"="job"."OperationId"
                WHERE "job"."AddonMediaListId"="list"."Id" AND "operation"."State" IN (0,1,2))
            ORDER BY "detail"."UpdatedAt","detail"."AddonMediaListItemId" LIMIT 1"#)
            .fetch_optional(&mut *tx).await.repository()?;
        if let Some((item_id, list_id)) = row {
            let now = encode_datetime(Utc::now());
            sqlx::query(
                r#"UPDATE "AddonMediaListDetailJob" SET "State"=1,"ManualPriority"=0,
                "OptionContextJson"=NULL,"OptionPurpose"=0,"AttemptCount"=0,"Outcome"=NULL,
                "ResultRecordJson"=NULL,"Diagnostic"=NULL,"NextEligibleAt"=NULL,
                "LeaseOwner"=NULL,"LeaseExpiresAt"=NULL,"StartedAt"=NULL,"CompletedAt"=NULL,
                "RequestedAt"=?,"UpdatedAt"=? WHERE "AddonMediaListItemId"=?"#,
            )
            .bind(&now)
            .bind(&now)
            .bind(item_id)
            .execute(&mut *tx)
            .await
            .repository()?;
            update_detail_progress(&mut tx, list_id, &now).await?;
        }
        tx.commit().await.repository()
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
                      "detail"."OptionPurpose", "detail"."OptionContextJson",
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
            option_context: row.try_get("OptionContextJson").repository()?,
            option_purpose: row.try_get("OptionPurpose").repository()?,
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
            lease_owner: Some(lease_owner),
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
                    .merge_detail_item_with_lease(
                        job.list_id,
                        job.item_id,
                        job.lease_owner.as_deref().ok_or_else(|| {
                            RepositoryError::Conflict(
                                "the checkpointed detail lease is missing".to_owned(),
                            )
                        })?,
                        &item,
                    )
                    .await
            }
            Err(error) => Err(error),
        };
        match result {
            Ok(DetailMergeOutcome::Applied) => {
                let state = if outcome == "unavailable" {
                    AddonMediaListDetailState::Skipped
                } else {
                    AddonMediaListDetailState::Complete
                };
                self.finish_checkpointed_detail(&job, state).await?;
            }
            Ok(DetailMergeOutcome::DeferredForPublication) => {
                self.defer_checkpointed_detail_for_publication(&job).await?;
            }
            Err(error) => {
                sqlx::query(
                    r#"UPDATE "AddonMediaListDetailJob"
                       SET "State" = ?, "Diagnostic" = ?, "LeaseOwner" = NULL,
                           "LeaseExpiresAt" = NULL, "UpdatedAt" = ?
                       WHERE "AddonMediaListItemId" = ? AND "LeaseOwner" = ?"#,
                )
                .bind(encode_enum(AddonMediaListDetailState::Failed))
                .bind(bound_diagnostic(&error.to_string()))
                .bind(encode_datetime(Utc::now()))
                .bind(job.item_id)
                .bind(job.lease_owner.as_deref())
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
        let mut decoded = decode_media_list_output_for_capability(&output, &job.record_capability)?;
        decoded.items.pop().ok_or_else(|| RepositoryError::Decode {
            kind: "media-list enrichment",
            value: "result omitted its item".to_owned(),
        })
    }

    #[allow(clippy::too_many_lines)] // Backend-portable compare-and-swap claim and host lease stay adjacent.
    async fn claim_detail_job(&self) -> Result<Option<DetailJob>> {
        crate::addon_media_list_refresh::ensure_provider_recovery_not_required(
            crate::addon_media_list_preparation::provider_process_recovery_required()
                || crate::addon_preparation_worker::recovery_required(&self.pool).await?,
        )?;
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
                      "detail"."OverviewRecordJson", "detail"."AttemptCount", "detail"."OptionPurpose",
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
            let item_id: EntityId = row.try_get("AddonMediaListItemId").repository()?;
            let option_purpose: i32 = row.try_get("OptionPurpose").repository()?;
            let list_id: EntityId = row.try_get("AddonMediaListId").repository()?;
            let option_context = match crate::addon_media_list_detail_options::capture(
                &mut tx,
                list_id,
                item_id,
                option_purpose,
            )
            .await
            {
                Ok(context) => context,
                Err(RepositoryError::Conflict(diagnostic)) => {
                    sqlx::query(r#"UPDATE "AddonMediaListDetailJob" SET "State"=?, "Diagnostic"=?, "UpdatedAt"=? WHERE "AddonMediaListItemId"=?"#)
                        .bind(encode_enum(AddonMediaListDetailState::Failed)).bind(bound_diagnostic(&diagnostic))
                        .bind(&now_text).bind(item_id).execute(&mut *tx).await.repository()?;
                    update_detail_progress(&mut tx, list_id, &now_text).await?;
                    continue;
                }
                Err(error) => return Err(error),
            };
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
            let claimed = sqlx::query(
                r#"UPDATE "AddonMediaListDetailJob"
                   SET "State" = ?, "LeaseOwner" = ?, "LeaseExpiresAt" = ?,
                       "StartedAt" = ?, "UpdatedAt" = ?, "OptionContextJson" = ?
                   WHERE "AddonMediaListItemId" = ? AND "State" IN (?, ?, ?)"#,
            )
            .bind(encode_enum(AddonMediaListDetailState::Running))
            .bind(&lease_owner)
            .bind(&lease_expires)
            .bind(&now_text)
            .bind(&now_text)
            .bind(&option_context)
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
                option_context: Some(option_context),
                option_purpose,
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
                lease_owner: Some(lease_owner.clone()),
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
        let lease_owner = job.lease_owner.as_deref().ok_or_else(|| {
            RepositoryError::Conflict("the provider detail lease is missing".to_owned())
        })?;
        crate::addon_media_list_detail_options::validate(
            &mut tx,
            job.list_id,
            job.item_id,
            job.option_purpose,
            job.option_context.as_deref(),
        )
        .await?;
        let updated = sqlx::query(
            r#"UPDATE "AddonMediaListDetailJob"
               SET "State" = ?, "AttemptCount" = ?, "Outcome" = ?,
                   "ResultRecordJson" = ?, "Diagnostic" = ?, "NextEligibleAt" = NULL,
                   "LeaseOwner" = NULL, "LeaseExpiresAt" = NULL, "UpdatedAt" = ?
               WHERE "AddonMediaListItemId" = ? AND "State" = ?
                 AND "LeaseOwner" = ? AND "LeaseExpiresAt" IS NOT NULL
                 AND "LeaseExpiresAt" > ?"#,
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
        .bind(encode_enum(AddonMediaListDetailState::Running))
        .bind(lease_owner)
        .bind(&now_text)
        .execute(&mut *tx)
        .await
        .repository()?;
        if updated.rows_affected() != 1 {
            return Err(RepositoryError::Conflict(
                "the provider detail lease changed before checkpoint".to_owned(),
            ));
        }
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
        let lease_owner = claimed_detail_lease(job)?;
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        let updated = sqlx::query(
            r#"UPDATE "AddonMediaListDetailJob"
               SET "State" = ?, "ManualPriority" = 0, "Diagnostic" = NULL,
                   "NextEligibleAt" = NULL, "LeaseOwner" = NULL,
                   "LeaseExpiresAt" = NULL, "CompletedAt" = ?, "UpdatedAt" = ?
               WHERE "AddonMediaListItemId" = ? AND "ResultRecordJson" IS NOT NULL
                 AND "State" = ? AND "LeaseOwner" = ?
                 AND "LeaseExpiresAt" IS NOT NULL AND "LeaseExpiresAt" > ?"#,
        )
        .bind(encode_enum(state))
        .bind(&now)
        .bind(&now)
        .bind(job.item_id)
        .bind(encode_enum(AddonMediaListDetailState::Running))
        .bind(lease_owner)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .repository()?;
        if updated.rows_affected() != 1 {
            return Err(RepositoryError::Conflict(
                "the detail lease changed before completion".to_owned(),
            ));
        }
        update_detail_progress(&mut tx, job.list_id, &now).await?;
        tx.commit().await.repository()?;
        Ok(())
    }

    async fn defer_checkpointed_detail_for_publication(&self, job: &DetailJob) -> Result<()> {
        let now = Utc::now();
        let retry_at = encode_datetime(now + Duration::seconds(DETAIL_PUBLICATION_RETRY_SECONDS));
        let now = encode_datetime(now);
        let lease_owner = claimed_detail_lease(job)?;
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        let updated = sqlx::query(
            r#"UPDATE "AddonMediaListDetailJob"
               SET "LeaseOwner" = NULL, "LeaseExpiresAt" = ?, "UpdatedAt" = ?
               WHERE "AddonMediaListItemId" = ? AND "State" = ?
                 AND "ResultRecordJson" IS NOT NULL AND "LeaseOwner" = ?
                 AND "LeaseExpiresAt" IS NOT NULL AND "LeaseExpiresAt" > ?"#,
        )
        .bind(&retry_at)
        .bind(&now)
        .bind(job.item_id)
        .bind(encode_enum(AddonMediaListDetailState::Running))
        .bind(lease_owner)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .repository()?;
        if updated.rows_affected() != 1 {
            return Err(RepositoryError::Conflict(
                "the checkpointed detail changed while publication deferral was persisted"
                    .to_owned(),
            ));
        }
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
        let lease_owner = claimed_detail_lease(job)?;
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        let updated = sqlx::query(
            r#"UPDATE "AddonMediaListDetailJob"
               SET "State" = ?, "AttemptCount" = ?, "Outcome" = ?, "Diagnostic" = ?,
                   "ResultRecordJson" = NULL, "NextEligibleAt" = ?,
                   "LeaseOwner" = NULL, "LeaseExpiresAt" = NULL,
                   "UpdatedAt" = ? WHERE "AddonMediaListItemId" = ? AND "State" = ?
                 AND "LeaseOwner" = ? AND "LeaseExpiresAt" IS NOT NULL
                 AND "LeaseExpiresAt" > ?"#,
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
        .bind(encode_enum(AddonMediaListDetailState::Running))
        .bind(lease_owner)
        .bind(&now_text)
        .execute(&mut *tx)
        .await
        .repository()?;
        if updated.rows_affected() != 1 {
            return Err(RepositoryError::Conflict(
                "the provider detail lease changed before failure was persisted".to_owned(),
            ));
        }
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
            r#"SELECT "State", "Purpose", "Mode", "IncrementalStrategy", "CreatedAt", "DiscoveredCount",
                      "ExaminedCount", "ArchivedSkippedCount", "EmittedCount",
                      CAST("TotalHint" AS SIGNED) AS "TotalHint",
                      CAST((SELECT COUNT(*) FROM "AddonMediaListItem" AS "item"
                            WHERE "item"."AddonMediaListId" = "job"."AddonMediaListId") AS SIGNED)
                          AS "RetainedItemCount",
                      "LastProgressAt", "Diagnostic"
               FROM "AddonMediaListImportJob" AS "job"
               WHERE "job"."AddonMediaListId" = ? ORDER BY "Generation" DESC LIMIT 1"#,
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
            purpose: decode_enum_column("AddonMediaListImportPurpose", &job, "Purpose")?,
            mode: decode_enum_column("AddonMediaListImportMode", &job, "Mode")?,
            strategy: decode_enum_column(
                "AddonMediaListIncrementalStrategy",
                &job,
                "IncrementalStrategy",
            )?,
            started_at: job.try_get("CreatedAt").repository()?,
            discovered_count: u64::try_from(job.try_get::<i32, _>("DiscoveredCount").repository()?)
                .unwrap_or_default(),
            examined_count: nonnegative_count(&job, "ExaminedCount")?,
            archived_skipped_count: nonnegative_count(&job, "ArchivedSkippedCount")?,
            emitted_count: nonnegative_count(&job, "EmittedCount")?,
            total_hint: job
                .try_get::<Option<i64>, _>("TotalHint")
                .repository()?
                .and_then(|value| u64::try_from(value).ok()),
            retained_item_count: nonnegative_count(&job, "RetainedItemCount")?,
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
struct ImportPage {
    complete: bool,
    next_cursor: Option<String>,
    total_hint: Option<u32>,
    revision_token: Option<String>,
    records: Vec<AddonMediaListRecord>,
    v2: Option<ImportV2Page>,
}

impl From<MediaListDiscoveryPage> for ImportPage {
    fn from(page: MediaListDiscoveryPage) -> Self {
        Self {
            complete: page.complete,
            next_cursor: page.next_cursor,
            total_hint: page.total_hint,
            revision_token: page.revision_token,
            records: page.records,
            v2: None,
        }
    }
}

impl From<MediaListDiscoveryV2Page> for ImportPage {
    fn from(page: MediaListDiscoveryV2Page) -> Self {
        Self {
            complete: page.complete,
            next_cursor: page.next_cursor,
            total_hint: page.total_hint,
            revision_token: page.revision_token,
            records: page.records,
            v2: Some(ImportV2Page {
                strategy: page.strategy,
                examined_count: page.examined_count,
                archived_skipped_count: page.archived_skipped_count,
                emitted_count: page.emitted_count,
            }),
        }
    }
}

#[derive(Debug)]
struct ImportV2Page {
    strategy: WireIncrementalStrategy,
    examined_count: u32,
    archived_skipped_count: u32,
    emitted_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PageCommitOutcome {
    Cancelled,
    Committed { complete: bool },
}

#[derive(Debug)]
struct ImportJob {
    id: EntityId,
    list_id: EntityId,
    generation: i32,
    addon_id: String,
    source_url: String,
    record_capability: String,
    import_capability: String,
    purpose: AddonMediaListImportPurpose,
    mode: AddonMediaListImportMode,
    cursor: Option<String>,
    verification_pass: i32,
    expected_identity_hash: Option<String>,
    overview_complete: bool,
    lease_owner: String,
}

#[derive(Debug)]
struct ImportProviderSlot {
    provider_host: String,
    lease_owner: String,
    delay: u32,
}

#[derive(Debug)]
struct DetailJob {
    option_context: Option<String>,
    option_purpose: i32,
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
    lease_owner: Option<String>,
}

fn claimed_detail_lease(job: &DetailJob) -> Result<&str> {
    job.lease_owner
        .as_deref()
        .ok_or_else(|| RepositoryError::Conflict("the provider detail lease is missing".to_owned()))
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

pub(crate) fn sample_provider_delay_seconds(
    seed: &[u8],
    range: AddonMediaListDetailWaitRange,
) -> u32 {
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

fn decode_import_checkpoint(row: &crate::db::DbRow) -> Result<AddonMediaListImportCheckpoint> {
    let promoted_at: String = row.try_get("PromotedAt").repository()?;
    Ok(AddonMediaListImportCheckpoint {
        id: row.try_get("Id").repository()?,
        list_id: row.try_get("AddonMediaListId").repository()?,
        relative_path: row.try_get("RelativePath").repository()?,
        generation: row.try_get("Generation").repository()?,
        sha256: row.try_get("Sha256").repository()?,
        entry_count: u64::try_from(row.try_get::<i64, _>("EntryCount").repository()?).map_err(
            |_| RepositoryError::Decode {
                kind: "add-on media-list import checkpoint entry count",
                value: "negative value".to_owned(),
            },
        )?,
        promoted_at: decode_datetime("add-on media-list checkpoint timestamp", &promoted_at)?,
    })
}

fn validate_archive_advance(archive: &SealedImportArchive, emitted_count: u32) -> Result<()> {
    let expected = archive
        .initial_entry_count
        .checked_add(emitted_count)
        .ok_or_else(|| {
            RepositoryError::Conflict(
                "the staged archive entry count exceeds its supported bound".to_owned(),
            )
        })?;
    if archive.entry_count != expected {
        return Err(RepositoryError::Conflict(
            "the staged archive advance does not match the emitted page items".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_LIST_V5;

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
        use ersatzrs_core::repositories::{
            AddonMediaListLifecycleRepository as _, AddonMediaListOptionSchemaRef,
            AddonMediaListRepository as _,
        };

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

        #[tokio::test]
        async fn editor_provider_import_rechecks_revision_before_admitting_a_generation() {
            let (_directory, pool, coordinator) = fixture().await;
            let source = "https://example.invalid/editor-fence";
            let id = queue(&coordinator, source, AddonMediaListDetailPolicy::ManualOnly).await;
            let revision: i32 =
                sqlx::query_scalar(r#"SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id"=?"#)
                    .bind(id)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            sqlx::query(
                r#"UPDATE "AddonMediaList" SET "DraftRevision"="DraftRevision"+1 WHERE "Id"=?"#,
            )
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
            let result = coordinator
                .with_editor_revision(revision)
                .queue_import(
                    "org.example.fixture",
                    &context(source),
                    None,
                    AddonMediaListContentKind::TelevisionEpisode,
                    AddonMediaListDetailPolicy::ManualOnly,
                    Some(id),
                )
                .await;
            assert!(matches!(result, Err(RepositoryError::Conflict(_))));
            let jobs: i64 = sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM "AddonMediaListImportJob" WHERE "AddonMediaListId"=?"#,
            )
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(jobs, 1);
        }

        fn context(source_url: &str) -> InstalledMediaListImportContext {
            InstalledMediaListImportContext {
                source_url: source_url.to_owned(),
                record_capability: CAPABILITY_MEDIA_LIST_LIST_V5.to_owned(),
                import_capability: ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_IMPORT_V1
                    .to_owned(),
                data_dir: std::path::PathBuf::from("unused-v1-data-dir"),
            }
        }

        fn v2_context(
            source_url: &str,
            data_dir: &std::path::Path,
        ) -> InstalledMediaListImportContext {
            InstalledMediaListImportContext {
                source_url: source_url.to_owned(),
                record_capability: CAPABILITY_MEDIA_LIST_LIST_V5.to_owned(),
                import_capability: CAPABILITY_MEDIA_LIST_IMPORT_V2.to_owned(),
                data_dir: data_dir.to_path_buf(),
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

        fn remote_stream_item_record(
            provider_id: &str,
            rank: i32,
            liveness: &str,
            duration_seconds: Option<u64>,
        ) -> AddonMediaListRecord {
            serde_json::from_value(serde_json::json!({
                "record_type": "item",
                "provider_id": provider_id,
                "rank": rank,
                "display_title": format!("Remote stream {}", rank + 1),
                "title": format!("Remote stream {}", rank + 1),
                "kind": "remote_stream",
                "source_url": format!("https://www.youtube.com/watch?v={provider_id}"),
                "availability": "available",
                "content_kind": "other_video",
                "liveness": liveness,
                "duration_seconds": duration_seconds
            }))
            .expect("remote stream item record")
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

        async fn queue_v2(
            coordinator: &AddonMediaListImportCoordinator,
            source_url: &str,
            data_dir: &std::path::Path,
            target_list_id: Option<EntityId>,
        ) -> EntityId {
            coordinator
                .queue_import(
                    "org.example.fixture",
                    &v2_context(source_url, data_dir),
                    None,
                    AddonMediaListContentKind::TelevisionEpisode,
                    AddonMediaListDetailPolicy::ManualOnly,
                    target_list_id,
                )
                .await
                .expect("queue v2 overview import")
        }

        async fn seal_v2_archive(
            coordinator: &AddonMediaListImportCoordinator,
            job: &ImportJob,
            data_dir: &std::path::Path,
            contents: &[u8],
        ) -> SealedImportArchive {
            let checkpoint = coordinator
                .resume_import_checkpoint(job)
                .await
                .expect("load current archive checkpoint");
            let prepared = prepare_import_archive(
                data_dir.to_path_buf(),
                job.list_id,
                job.generation,
                checkpoint.as_ref(),
            )
            .await
            .expect("prepare v2 archive");
            tokio::fs::write(prepared.staged_path(), contents)
                .await
                .expect("write staged v2 archive");
            prepared.seal().await.expect("seal v2 archive")
        }

        async fn commit_complete_v2_page(
            coordinator: &AddonMediaListImportCoordinator,
            job: &ImportJob,
            sealed: &SealedImportArchive,
            total_hint: u32,
            examined_count: u32,
            archived_skipped_count: u32,
            records: Vec<AddonMediaListRecord>,
        ) {
            let emitted_count = u32::try_from(
                records
                    .iter()
                    .filter(|record| matches!(record, AddonMediaListRecord::Item { .. }))
                    .count(),
            )
            .expect("bounded v2 item count");
            validate_archive_advance(sealed, emitted_count).expect("valid archive advance");
            let outcome = coordinator
                .commit_import_page_checkpoint(
                    job,
                    MediaListDiscoveryV2Page {
                        complete: true,
                        next_cursor: None,
                        total_hint: Some(total_hint),
                        revision_token: Some(format!("generation-{}", job.generation)),
                        strategy: WireIncrementalStrategy::SkipKnown,
                        examined_count,
                        archived_skipped_count,
                        emitted_count,
                        records,
                    }
                    .into(),
                    Some(sealed),
                )
                .await
                .expect("commit v2 page");
            assert!(matches!(
                outcome,
                PageCommitOutcome::Committed { complete: true }
            ));
            coordinator
                .cleanup_one_unreferenced_import_archive(Some(job.list_id))
                .await
                .expect("clean obsolete v2 archive");
            coordinator
                .finalize_import_pass(job)
                .await
                .expect("finalize v2 page");
        }

        fn option_schema_fixture() -> (String, AddonMediaListOptionSchemaRef) {
            let definition = serde_json::json!({
                "id": "media-list.options.v1",
                "options": [{
                    "key": "enabled",
                    "label": {"en-US": "Enabled"},
                    "description": {"en-US": "Enable fixture behavior."},
                    "scope": "both",
                    "operation_targets": ["playback"],
                    "applicability": "all",
                    "kind": "boolean",
                    "default": true
                }]
            })
            .to_string();
            let schema = AddonMediaListOptionSchemaRef {
                addon_id: "org.example.fixture".into(),
                capability_id: "media-list.options.v1".into(),
                schema_digest: sha256_hex(definition.as_bytes()),
            };
            (definition, schema)
        }

        async fn install_replacement_addon(temporary: &tempfile::TempDir, pool: &DbPool) {
            sqlx::query(
                r#"INSERT INTO "AddonInstallation" (
                       "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
                       "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt"
                   ) VALUES ('org.example.replacement', '1.0.0', 1, 'package-2', 'content-2',
                             '{}', ?, ?, ?)"#,
            )
            .bind(
                temporary
                    .path()
                    .join("replacement")
                    .to_string_lossy()
                    .into_owned(),
            )
            .bind(encode_datetime(Utc::now()))
            .bind(encode_datetime(Utc::now()))
            .execute(pool)
            .await
            .expect("replacement add-on");
        }

        async fn seed_option_state(
            pool: &DbPool,
            list_id: EntityId,
            definition: &str,
            schema: &AddonMediaListOptionSchemaRef,
        ) {
            sqlx::query(
                r#"INSERT INTO "AddonMediaListOptionSchema" (
                       "Id", "AddonId", "CapabilityId", "SchemaDigest", "DefinitionJson", "CreatedAt"
                   ) VALUES (101, 'org.example.fixture', 'media-list.options.v1', ?, ?,
                             '2026-09-04 12:00:00')"#,
            )
            .bind(&schema.schema_digest)
            .bind(definition)
            .execute(pool)
            .await
            .expect("seed option schema");
            sqlx::query(
                r#"UPDATE "AddonMediaList"
                   SET "DraftOptionSchemaId" = 101, "DraftOptionValuesJson" = '{"enabled":true}'
                   WHERE "Id" = ?"#,
            )
            .bind(list_id)
            .execute(pool)
            .await
            .expect("seed list options");
            sqlx::query(
                r#"INSERT INTO "AddonMediaListItem" (
                       "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind", "Rank", "Title",
                       "DraftOptionOverridesJson"
                   ) VALUES (102, ?, 'item', 'Item', 4, 0, 'Item', '{"enabled":false}')"#,
            )
            .bind(list_id)
            .execute(pool)
            .await
            .expect("seed item options");
            sqlx::query(
                r#"INSERT INTO "AddonMediaListRevision" (
                       "Id", "AddonMediaListId", "Revision", "OptionSchemaId", "OptionValuesJson"
                   ) VALUES (103, ?, 1, 101, '{"enabled":true}')"#,
            )
            .bind(list_id)
            .execute(pool)
            .await
            .expect("seed published options");
            sqlx::query(
                r#"INSERT INTO "AddonMediaListImportCheckpoint" (
                       "Id", "AddonMediaListId", "RelativePath", "Generation", "Sha256",
                       "EntryCount", "PromotedAt"
                   ) VALUES (104, ?, 'checkpoints/provider.txt', 1, 'checkpoint-digest', 1,
                             '2026-09-04 12:00:00')"#,
            )
            .bind(list_id)
            .execute(pool)
            .await
            .expect("seed provider checkpoint");
            sqlx::query(
                r#"UPDATE "AddonMediaListImportJob" SET "CheckpointId" = 104
                   WHERE "AddonMediaListId" = ?"#,
            )
            .bind(list_id)
            .execute(pool)
            .await
            .expect("attach provider checkpoint");
        }

        async fn draft_option_state(
            pool: &DbPool,
            list_id: EntityId,
        ) -> (Option<EntityId>, String, String) {
            sqlx::query_as(
                r#"SELECT "DraftOptionSchemaId", "DraftOptionValuesJson",
                          (SELECT "DraftOptionOverridesJson" FROM "AddonMediaListItem" WHERE "Id" = 102)
                   FROM "AddonMediaList" WHERE "Id" = ?"#,
            )
            .bind(list_id)
            .fetch_one(pool)
            .await
            .expect("load draft option state")
        }

        async fn checkpoint_count(pool: &DbPool, list_id: EntityId) -> i64 {
            sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM "AddonMediaListImportCheckpoint"
                   WHERE "AddonMediaListId" = ?"#,
            )
            .bind(list_id)
            .fetch_one(pool)
            .await
            .expect("load checkpoint count")
        }

        async fn assert_historical_options_survive_uninstall(
            pool: &DbPool,
            list_id: EntityId,
            schema: &AddonMediaListOptionSchemaRef,
        ) {
            sqlx::query(r#"DELETE FROM "AddonInstallation" WHERE "AddonId" = ?"#)
                .bind("org.example.fixture")
                .execute(pool)
                .await
                .expect("uninstall the replaced add-on");
            let repository = SqliteAddonMediaListRepository(pool.clone());
            let historical_schema = repository
                .option_schema(schema)
                .await
                .expect("load historical schema")
                .expect("historical schema survives uninstall");
            assert_eq!(&historical_schema.schema, schema);
            let historical_options = repository
                .published_options(list_id, 1, Some(schema))
                .await
                .expect("load historical published options")
                .expect("historical published options survive uninstall");
            assert_eq!(historical_options.schema.as_ref(), Some(schema));
            assert_eq!(
                historical_options.list_values.get("enabled"),
                Some(&ersatzrs_domain::AddonMediaListOptionValue::Boolean(true))
            );
        }

        #[tokio::test]
        async fn resumable_same_addon_refresh_preserves_and_switch_clears_draft_options() {
            let (temporary, pool, coordinator) = fixture().await;
            let source_url = "https://provider.example.test/options";
            let list_id = queue(
                &coordinator,
                source_url,
                AddonMediaListDetailPolicy::ManualOnly,
            )
            .await;
            let (option_definition, schema_ref) = option_schema_fixture();
            install_replacement_addon(&temporary, &pool).await;
            seed_option_state(&pool, list_id, &option_definition, &schema_ref).await;

            coordinator
                .queue_import(
                    "org.example.fixture",
                    &context(source_url),
                    None,
                    AddonMediaListContentKind::TelevisionEpisode,
                    AddonMediaListDetailPolicy::ManualOnly,
                    Some(list_id),
                )
                .await
                .expect("queue same-add-on refresh");
            assert_eq!(
                draft_option_state(&pool, list_id).await,
                (
                    Some(101),
                    r#"{"enabled":true}"#.into(),
                    r#"{"enabled":false}"#.into()
                )
            );
            assert_eq!(checkpoint_count(&pool, list_id).await, 1);
            let old_data_root = temporary.path().join("old provider data");
            let old_archive = old_data_root.join("checkpoints/provider.txt");
            tokio::fs::create_dir_all(old_archive.parent().unwrap())
                .await
                .expect("create old archive directory");
            tokio::fs::write(&old_archive, b"fixture item\n")
                .await
                .expect("write old provider archive");
            sqlx::query(
                r#"UPDATE "AddonMediaListImportCheckpoint" SET "DataRoot" = ?
                   WHERE "AddonMediaListId" = ?"#,
            )
            .bind(old_data_root.to_string_lossy().as_ref())
            .bind(list_id)
            .execute(&pool)
            .await
            .expect("record old archive ownership");
            sqlx::query(
                r#"UPDATE "AddonMediaListImportJob" SET "ImportCapability" = ?
                   WHERE "AddonMediaListId" = ?"#,
            )
            .bind(CAPABILITY_MEDIA_LIST_IMPORT_V2)
            .bind(list_id)
            .execute(&pool)
            .await
            .expect("mark old v2 generation");
            coordinator
                .queue_import(
                    "org.example.replacement",
                    &context(source_url),
                    None,
                    AddonMediaListContentKind::TelevisionEpisode,
                    AddonMediaListDetailPolicy::ManualOnly,
                    Some(list_id),
                )
                .await
                .expect("queue replacement add-on import");
            assert_eq!(
                draft_option_state(&pool, list_id).await,
                (None, "{}".into(), "{}".into())
            );
            let retained_revision: i64 = sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM "AddonMediaListRevision"
                   WHERE "Id" = 103 AND "OptionSchemaId" = 101"#,
            )
            .fetch_one(&pool)
            .await
            .expect("load retained published revision");
            assert_eq!(retained_revision, 1);
            assert_eq!(checkpoint_count(&pool, list_id).await, 0);
            assert!(!old_archive.exists());
            let attached_checkpoint_jobs: i64 = sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM "AddonMediaListImportJob"
                   WHERE "AddonMediaListId" = ? AND "CheckpointId" IS NOT NULL"#,
            )
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .expect("load cleared job checkpoint references");
            assert_eq!(attached_checkpoint_jobs, 0);

            assert_historical_options_survive_uninstall(&pool, list_id, &schema_ref).await;
        }

        #[tokio::test]
        async fn queue_import_defers_while_publication_is_verifying() {
            let (_temporary, pool, coordinator) = fixture().await;
            let source_url = "https://provider.example.test/verifying";
            let list_id = queue(
                &coordinator,
                source_url,
                AddonMediaListDetailPolicy::ManualOnly,
            )
            .await;
            sqlx::query(
                r#"UPDATE "AddonMediaList"
                   SET "ReviewState" = 3,
                       "PublicationJournalJson" = '{"phase":"Verifying"}'
                   WHERE "Id" = ?"#,
            )
            .bind(list_id)
            .execute(&pool)
            .await
            .expect("checkpoint verifying publication");
            let draft_before: i32 = sqlx::query_scalar(
                r#"SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id" = ?"#,
            )
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .expect("draft before blocked queue");
            let jobs_before: i64 = sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM "AddonMediaListImportJob"
                   WHERE "AddonMediaListId" = ?"#,
            )
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .expect("jobs before blocked queue");

            let error = coordinator
                .queue_import(
                    "org.example.fixture",
                    &context(source_url),
                    None,
                    AddonMediaListContentKind::TelevisionEpisode,
                    AddonMediaListDetailPolicy::Automatic,
                    Some(list_id),
                )
                .await
                .expect_err("publication verification freezes overview queue admission");
            assert!(error.to_string().contains("being published"));
            assert_eq!(
                sqlx::query_scalar::<_, i32>(
                    r#"SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id" = ?"#,
                )
                .bind(list_id)
                .fetch_one(&pool)
                .await
                .expect("unchanged draft after blocked queue"),
                draft_before
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    r#"SELECT COUNT(*) FROM "AddonMediaListImportJob"
                       WHERE "AddonMediaListId" = ?"#,
                )
                .bind(list_id)
                .fetch_one(&pool)
                .await
                .expect("unchanged jobs after blocked queue"),
                jobs_before
            );
        }

        #[tokio::test]
        async fn completed_overview_promotion_defers_at_a_verifying_checkpoint() {
            let (_temporary, pool, coordinator) = fixture().await;
            let source_url = "https://provider.example.test/promotion-race";
            let list_id = queue(
                &coordinator,
                source_url,
                AddonMediaListDetailPolicy::ManualOnly,
            )
            .await;
            let job = coordinator
                .claim_import_job()
                .await
                .expect("claim overview")
                .expect("queued overview");
            sqlx::query(
                r#"CREATE TRIGGER "CheckpointPublicationBeforeOverviewPromotion"
                   AFTER UPDATE OF "OverviewComplete" ON "AddonMediaListImportJob"
                   WHEN NEW."OverviewComplete" = 1
                   BEGIN
                     UPDATE "AddonMediaList"
                     SET "ReviewState" = 3,
                         "PublicationJournalJson" = '{"phase":"Verifying"}'
                     WHERE "Id" = NEW."AddonMediaListId";
                   END"#,
            )
            .execute(&pool)
            .await
            .expect("install promotion checkpoint trigger");

            let error = coordinator
                .commit_import_page(
                    &job,
                    MediaListDiscoveryPage {
                        complete: true,
                        next_cursor: None,
                        total_hint: Some(1),
                        revision_token: Some("revision-1".to_owned()),
                        records: vec![
                            list_record("provider/promoted"),
                            item_record("episode:1", 0, "Overview plot"),
                        ],
                    },
                )
                .await
                .expect_err("publication verification freezes overview promotion");
            assert!(error.to_string().contains("being published"));
            let persisted = sqlx::query_as::<_, (String, i32, i32)>(
                r#"SELECT "ProviderId", "ItemCount", "DraftRevision"
                   FROM "AddonMediaList" WHERE "Id" = ?"#,
            )
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .expect("load list after blocked promotion");
            assert!(persisted.0.starts_with("pending:"));
            assert_eq!((persisted.1, persisted.2), (0, 0));
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    r#"SELECT COUNT(*) FROM "AddonMediaListItem"
                       WHERE "AddonMediaListId" = ?"#,
                )
                .bind(list_id)
                .fetch_one(&pool)
                .await
                .expect("count unpromoted items"),
                0
            );
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
        async fn v2_checkpoint_skips_unchanged_items_and_merges_only_later_additions() {
            let (temporary, pool, coordinator) = fixture().await;
            let data_dir = temporary.path().join("add-on data with spaces");
            let source_url = "https://media.example.test/incremental";
            let list_id = queue_v2(&coordinator, source_url, &data_dir, None).await;
            let first_job = coordinator
                .claim_import_job()
                .await
                .expect("claim full v2 import")
                .expect("full v2 import");
            assert_eq!(first_job.mode, AddonMediaListImportMode::Full);
            let first_archive =
                seal_v2_archive(&coordinator, &first_job, &data_dir, b"fixture first\n").await;
            validate_archive_advance(&first_archive, 1).unwrap();
            let first_outcome = coordinator
                .commit_import_page_checkpoint(
                    &first_job,
                    MediaListDiscoveryV2Page {
                        complete: false,
                        next_cursor: Some("page-2".to_owned()),
                        total_hint: Some(2),
                        revision_token: Some("generation-1".to_owned()),
                        strategy: WireIncrementalStrategy::SkipKnown,
                        examined_count: 1,
                        archived_skipped_count: 0,
                        emitted_count: 1,
                        records: vec![
                            list_record("fixture/incremental"),
                            item_record("first", 0, "First overview"),
                        ],
                    }
                    .into(),
                    Some(&first_archive),
                )
                .await
                .expect("checkpoint first full page");
            assert!(matches!(
                first_outcome,
                PageCommitOutcome::Committed { complete: false }
            ));
            assert!(coordinator
                .current_import_checkpoint(list_id)
                .await
                .unwrap()
                .is_none());
            let second_job = coordinator
                .claim_import_job()
                .await
                .expect("claim second full v2 page")
                .expect("second full v2 page");
            let second_archive = seal_v2_archive(
                &coordinator,
                &second_job,
                &data_dir,
                b"fixture first\nfixture second\n",
            )
            .await;
            commit_complete_v2_page(
                &coordinator,
                &second_job,
                &second_archive,
                2,
                1,
                0,
                vec![item_record("second", 1, "Second overview")],
            )
            .await;
            let draft_after_full: i32 = sqlx::query_scalar(
                r#"SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id" = ?"#,
            )
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .expect("load full draft revision");

            sqlx::query(
                r#"UPDATE "AddonMediaList" SET "RefreshPublicationPolicy" = 1 WHERE "Id" = ?"#,
            )
            .bind(list_id)
            .execute(&pool)
            .await
            .unwrap();
            queue_v2(&coordinator, source_url, &data_dir, Some(list_id)).await;
            let unchanged_job = coordinator
                .claim_import_job()
                .await
                .expect("claim unchanged v2 import")
                .expect("unchanged v2 import");
            assert_eq!(unchanged_job.mode, AddonMediaListImportMode::Incremental);
            let unchanged_archive = seal_v2_archive(
                &coordinator,
                &unchanged_job,
                &data_dir,
                b"fixture first\nfixture second\n",
            )
            .await;
            assert!(unchanged_archive.reused_previous);
            commit_complete_v2_page(
                &coordinator,
                &unchanged_job,
                &unchanged_archive,
                2,
                2,
                2,
                Vec::new(),
            )
            .await;
            let draft_after_unchanged: i32 = sqlx::query_scalar(
                r#"SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id" = ?"#,
            )
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .expect("load unchanged draft revision");
            assert_eq!(draft_after_unchanged, draft_after_full);
            let unchanged_progress = coordinator.progress(list_id).await.unwrap().unwrap();
            assert_eq!(
                unchanged_progress.mode,
                AddonMediaListImportMode::Incremental
            );
            assert_eq!(unchanged_progress.examined_count, 2);
            assert_eq!(unchanged_progress.archived_skipped_count, 2);
            assert_eq!(unchanged_progress.emitted_count, 0);

            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    r#"SELECT COUNT(*) FROM "AddonMediaListAutoPublication""#
                )
                .fetch_one(&pool)
                .await
                .unwrap(),
                0,
                "manual refresh never authorizes publication"
            );
            let coordinator = AddonMediaListImportCoordinator::for_scheduled_refresh(pool.clone());
            queue_v2(&coordinator, source_url, &data_dir, Some(list_id)).await;
            let addition_job = coordinator
                .claim_import_job()
                .await
                .expect("claim addition v2 import")
                .expect("addition v2 import");
            let addition_archive = seal_v2_archive(
                &coordinator,
                &addition_job,
                &data_dir,
                b"fixture first\nfixture second\nfixture third\nfixture fourth\n",
            )
            .await;
            commit_complete_v2_page(
                &coordinator,
                &addition_job,
                &addition_archive,
                4,
                4,
                2,
                vec![
                    item_record("third", 0, "Third overview"),
                    item_record("fourth", 1, "Fourth overview"),
                ],
            )
            .await;

            let identities: Vec<(String, i32)> = sqlx::query_as(
                r#"SELECT "ProviderId", "Rank" FROM "AddonMediaListItem"
                   WHERE "AddonMediaListId" = ? ORDER BY "Rank", "Id""#,
            )
            .bind(list_id)
            .fetch_all(&pool)
            .await
            .expect("load incrementally merged items");
            assert_eq!(
                identities,
                vec![
                    ("third".to_owned(), 0),
                    ("fourth".to_owned(), 1),
                    ("first".to_owned(), 2),
                    ("second".to_owned(), 3),
                ]
            );
            let addition_progress = coordinator.progress(list_id).await.unwrap().unwrap();
            assert_eq!(addition_progress.examined_count, 4);
            assert_eq!(addition_progress.archived_skipped_count, 2);
            assert_eq!(addition_progress.emitted_count, 2);
            let automatic: (i64, i32, String) = sqlx::query_as(
                r#"SELECT "ImportJobId", "DraftRevision", "State" FROM "AddonMediaListAutoPublication" WHERE "AddonMediaListId" = ?"#,
            ).bind(list_id).fetch_one(&pool).await.unwrap();
            assert_eq!(
                automatic,
                (
                    addition_job.id,
                    draft_after_unchanged + 1,
                    "ready".to_owned()
                )
            );
            // Even a manual Refresh that coalesces with a scheduled generation
            // revokes its automatic authorization without needing a draft edit.
            queue_v2(&coordinator, source_url, &data_dir, Some(list_id)).await;
            let manual = AddonMediaListImportCoordinator::new(pool.clone());
            queue_v2(&manual, source_url, &data_dir, Some(list_id)).await;
            let state: String = sqlx::query_scalar(r#"SELECT "State" FROM "AddonMediaListAutoPublication" WHERE "AddonMediaListId" = ?"#)
                .bind(list_id).fetch_one(&pool).await.unwrap();
            assert_eq!(state, "held");
        }

        #[tokio::test]
        #[allow(clippy::too_many_lines)] // One lifecycle proves incremental and legacy full merges.
        async fn v2_empty_delta_refreshes_present_header_fields_without_erasing_rich_metadata() {
            let (temporary, pool, coordinator) = fixture().await;
            let data_dir = temporary.path().join("private header data");
            let source_url = "https://media.example.test/rich-header";
            let list_id = queue_v2(&coordinator, source_url, &data_dir, None).await;
            let full_job = coordinator.claim_import_job().await.unwrap().unwrap();
            let full_archive =
                seal_v2_archive(&coordinator, &full_job, &data_dir, b"fixture first\n").await;
            let original_header = serde_json::from_value(serde_json::json!({
                "record_type": "list",
                "provider_id": "fixture/rich-header",
                "name": "Original provider title",
                "description": "Original provider summary",
                "metadata": {
                    "title": "Original provider title",
                    "plot": "Original provider summary",
                    "tags": ["archive", "music"],
                    "people": [{"name": "Fixture channel", "role": "Channel", "order": 0}],
                    "artwork": [{
                        "url": "https://images.example.test/header.jpg",
                        "role": "fanart"
                    }],
                    "guids": ["yt-dlp-playlist://playlist-1"]
                }
            }))
            .expect("original rich list header");
            commit_complete_v2_page(
                &coordinator,
                &full_job,
                &full_archive,
                1,
                1,
                0,
                vec![original_header, item_record("first", 0, "First overview")],
            )
            .await;
            sqlx::query(r#"UPDATE "AddonMediaList" SET "MetadataOverrideJson" = ? WHERE "Id" = ?"#)
                .bind(r#"{"plot":"Operator summary"}"#)
                .bind(list_id)
                .execute(&pool)
                .await
                .expect("save operator metadata override");
            let revision_before: i32 = sqlx::query_scalar(
                r#"SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id" = ?"#,
            )
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .expect("load original draft revision");

            queue_v2(&coordinator, source_url, &data_dir, Some(list_id)).await;
            let incremental_job = coordinator.claim_import_job().await.unwrap().unwrap();
            let unchanged_archive = seal_v2_archive(
                &coordinator,
                &incremental_job,
                &data_dir,
                b"fixture first\n",
            )
            .await;
            let changed_sparse_header = serde_json::from_value(serde_json::json!({
                "record_type": "list",
                "provider_id": "fixture/rich-header",
                "name": "Renamed provider title",
                "description": "Changed provider summary",
                "metadata": {
                    "title": "Renamed provider title",
                    "plot": "Changed provider summary",
                    "tags": ["archive", "updated"]
                }
            }))
            .expect("changed sparse list header");
            commit_complete_v2_page(
                &coordinator,
                &incremental_job,
                &unchanged_archive,
                1,
                1,
                1,
                vec![changed_sparse_header],
            )
            .await;

            let row = sqlx::query(
                r#"SELECT "Name", "Description", "ItemCount", "DraftRevision",
                          "ProviderMetadataJson", "MetadataOverrideJson", "MetadataConflictJson"
                   FROM "AddonMediaList" WHERE "Id" = ?"#,
            )
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .expect("load merged provider header");
            assert_eq!(
                row.try_get::<String, _>("Name").unwrap(),
                "Renamed provider title"
            );
            assert_eq!(
                row.try_get::<Option<String>, _>("Description").unwrap(),
                Some("Changed provider summary".to_owned())
            );
            assert_eq!(row.try_get::<i32, _>("ItemCount").unwrap(), 1);
            assert_eq!(
                row.try_get::<i32, _>("DraftRevision").unwrap(),
                revision_before + 1
            );
            assert_eq!(
                row.try_get::<String, _>("MetadataOverrideJson").unwrap(),
                r#"{"plot":"Operator summary"}"#
            );
            let provider_metadata: serde_json::Value =
                serde_json::from_str(&row.try_get::<String, _>("ProviderMetadataJson").unwrap())
                    .expect("decode merged provider metadata");
            assert_eq!(provider_metadata["title"], "Renamed provider title");
            assert_eq!(provider_metadata["plot"], "Changed provider summary");
            assert_eq!(
                provider_metadata["tags"],
                serde_json::json!(["archive", "updated"])
            );
            assert_eq!(
                provider_metadata["people"],
                serde_json::json!([{
                    "name": "Fixture channel",
                    "role": "Channel",
                    "order": 0,
                    "thumb": null
                }])
            );
            assert_eq!(
                provider_metadata["artwork"],
                serde_json::json!([{
                    "url": "https://images.example.test/header.jpg",
                    "role": "fanart",
                    "width": null,
                    "height": null,
                    "language": null
                }])
            );
            assert_eq!(
                provider_metadata["guids"],
                serde_json::json!(["yt-dlp-playlist://playlist-1"])
            );
            let conflicts: serde_json::Value = serde_json::from_str(
                &row.try_get::<Option<String>, _>("MetadataConflictJson")
                    .unwrap()
                    .expect("provider change beneath override records a conflict"),
            )
            .expect("decode metadata conflict");
            assert_eq!(
                conflicts["plot"]["provider_before"],
                "Original provider summary"
            );
            assert_eq!(
                conflicts["plot"]["provider_after"],
                "Changed provider summary"
            );
            assert_eq!(conflicts["plot"]["operator_value"], "Operator summary");
            let item_count: i64 = sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ?"#,
            )
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .expect("count retained items");
            assert_eq!(item_count, 1);

            sqlx::query(
                r#"DELETE FROM "AddonMediaListImportCheckpoint" WHERE "AddonMediaListId" = ?"#,
            )
            .bind(list_id)
            .execute(&pool)
            .await
            .expect("remove the legacy list's unavailable v2 checkpoint");
            queue_v2(&coordinator, source_url, &data_dir, Some(list_id)).await;
            let full_upgrade_job = coordinator.claim_import_job().await.unwrap().unwrap();
            assert_eq!(full_upgrade_job.mode, AddonMediaListImportMode::Full);
            let full_upgrade_archive = seal_v2_archive(
                &coordinator,
                &full_upgrade_job,
                &data_dir,
                b"fixture first\n",
            )
            .await;
            let sparse_full_header = serde_json::from_value(serde_json::json!({
                "record_type": "list",
                "provider_id": "fixture/rich-header",
                "name": "Full upgrade title",
                "metadata": {
                    "title": "Full upgrade title",
                    "tags": ["full-upgrade"]
                }
            }))
            .expect("sparse same-provider full header");
            commit_complete_v2_page(
                &coordinator,
                &full_upgrade_job,
                &full_upgrade_archive,
                1,
                1,
                0,
                vec![
                    sparse_full_header,
                    item_record("first", 0, "Reloaded overview"),
                ],
            )
            .await;
            let reloaded = sqlx::query(
                r#"SELECT "Description", "ProviderMetadataJson", "MetadataConflictJson"
                   FROM "AddonMediaList" WHERE "Id" = ?"#,
            )
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .expect("load same-provider full metadata");
            assert_eq!(
                reloaded
                    .try_get::<Option<String>, _>("Description")
                    .unwrap(),
                Some("Changed provider summary".to_owned())
            );
            let reloaded_metadata: serde_json::Value = serde_json::from_str(
                &reloaded
                    .try_get::<String, _>("ProviderMetadataJson")
                    .unwrap(),
            )
            .expect("decode same-provider full metadata");
            assert_eq!(reloaded_metadata["title"], "Full upgrade title");
            assert_eq!(
                reloaded_metadata["tags"],
                serde_json::json!(["full-upgrade"])
            );
            assert_eq!(reloaded_metadata["people"], provider_metadata["people"]);
            assert_eq!(reloaded_metadata["artwork"], provider_metadata["artwork"]);
            assert_eq!(reloaded_metadata["guids"], provider_metadata["guids"]);
            let retained_conflicts: serde_json::Value = serde_json::from_str(
                &reloaded
                    .try_get::<Option<String>, _>("MetadataConflictJson")
                    .unwrap()
                    .expect("retain conflict for omitted provider field"),
            )
            .expect("decode retained full-import conflict");
            assert_eq!(
                retained_conflicts["plot"]["provider_after"],
                "Changed provider summary"
            );
        }

        #[tokio::test]
        #[allow(clippy::too_many_lines)] // Keep the checkpoint baseline and both rejected-page assertions together.
        async fn v2_invalid_and_cancelled_pages_do_not_advance_the_live_checkpoint() {
            let (temporary, pool, coordinator) = fixture().await;
            let data_dir = temporary.path().join("private data");
            let source_url = "https://media.example.test/checkpoint-fence";
            let list_id = queue_v2(&coordinator, source_url, &data_dir, None).await;
            let full_job = coordinator.claim_import_job().await.unwrap().unwrap();
            let full_archive =
                seal_v2_archive(&coordinator, &full_job, &data_dir, b"fixture first\n").await;
            commit_complete_v2_page(
                &coordinator,
                &full_job,
                &full_archive,
                1,
                1,
                0,
                vec![
                    list_record("fixture/checkpoint-fence"),
                    item_record("first", 0, "First overview"),
                ],
            )
            .await;
            let checkpoint_before = coordinator
                .current_import_checkpoint(list_id)
                .await
                .unwrap()
                .expect("live checkpoint");

            queue_v2(&coordinator, source_url, &data_dir, Some(list_id)).await;
            let incremental_job = coordinator.claim_import_job().await.unwrap().unwrap();
            let invalid_archive = seal_v2_archive(
                &coordinator,
                &incremental_job,
                &data_dir,
                b"fixture first\nfixture unreported\n",
            )
            .await;
            assert!(validate_archive_advance(&invalid_archive, 0).is_err());
            invalid_archive.discard_candidate().await;
            assert_eq!(
                coordinator
                    .current_import_checkpoint(list_id)
                    .await
                    .unwrap()
                    .unwrap()
                    .relative_path,
                checkpoint_before.relative_path
            );

            let cancelled_archive = seal_v2_archive(
                &coordinator,
                &incremental_job,
                &data_dir,
                b"fixture first\nfixture second\n",
            )
            .await;
            validate_archive_advance(&cancelled_archive, 1).unwrap();
            sqlx::query(
                r#"UPDATE "AddonMediaListImportJob" SET "CancellationRequested" = 1
                   WHERE "Id" = ?"#,
            )
            .bind(incremental_job.id)
            .execute(&pool)
            .await
            .expect("cancel running v2 page");
            let outcome = coordinator
                .commit_import_page_checkpoint(
                    &incremental_job,
                    MediaListDiscoveryV2Page {
                        complete: true,
                        next_cursor: None,
                        total_hint: Some(2),
                        revision_token: Some("cancelled".to_owned()),
                        strategy: WireIncrementalStrategy::SkipKnown,
                        examined_count: 2,
                        archived_skipped_count: 1,
                        emitted_count: 1,
                        records: vec![
                            serde_json::from_value(serde_json::json!({
                                "record_type": "list",
                                "provider_id": "fixture/cancel-between-pages",
                                "name": "Uncommitted header"
                            }))
                            .expect("changed incremental header"),
                            item_record("second", 1, "Second overview"),
                        ],
                    }
                    .into(),
                    Some(&cancelled_archive),
                )
                .await
                .expect("commit cancelled v2 page");
            assert!(matches!(outcome, PageCommitOutcome::Cancelled));
            cancelled_archive.discard_candidate().await;
            let checkpoint_after = coordinator
                .current_import_checkpoint(list_id)
                .await
                .unwrap()
                .expect("retained live checkpoint");
            assert_eq!(
                checkpoint_after.relative_path,
                checkpoint_before.relative_path
            );
            assert_eq!(checkpoint_after.sha256, checkpoint_before.sha256);
            assert_eq!(checkpoint_after.entry_count, checkpoint_before.entry_count);
        }

        #[tokio::test]
        async fn incomplete_or_cancelled_reconcile_does_not_record_missing_evidence() {
            let (temporary, pool, coordinator) = fixture().await;
            let data_dir = temporary.path().join("private data");
            let source_url = "https://media.example.test/incomplete-reconcile";
            let list_id = queue_v2(&coordinator, source_url, &data_dir, None).await;
            let full_job = coordinator.claim_import_job().await.unwrap().unwrap();
            let full_archive = seal_v2_archive(
                &coordinator,
                &full_job,
                &data_dir,
                b"fixture first\nfixture second\n",
            )
            .await;
            commit_complete_v2_page(
                &coordinator,
                &full_job,
                &full_archive,
                2,
                2,
                0,
                vec![
                    list_record("fixture/incomplete-reconcile"),
                    item_record("first", 0, "First overview"),
                    item_record("second", 1, "Second overview"),
                ],
            )
            .await;

            queue_v2(&coordinator, source_url, &data_dir, Some(list_id)).await;
            sqlx::query(
                r#"UPDATE "AddonMediaListImportJob"
                   SET "Purpose" = ?, "Mode" = ?, "CheckpointId" = NULL
                   WHERE "Id" = (
                       SELECT "Id" FROM "AddonMediaListImportJob"
                       WHERE "AddonMediaListId" = ? ORDER BY "Generation" DESC LIMIT 1
                   )"#,
            )
            .bind(encode_enum(AddonMediaListImportPurpose::Reconcile))
            .bind(encode_enum(AddonMediaListImportMode::Full))
            .bind(list_id)
            .execute(&pool)
            .await
            .expect("convert queued import to reconcile fixture");
            let reconcile = coordinator.claim_import_job().await.unwrap().unwrap();
            let staged =
                seal_v2_archive(&coordinator, &reconcile, &data_dir, b"fixture first\n").await;
            validate_archive_advance(&staged, 1).expect("valid partial archive");
            let outcome = coordinator
                .commit_import_page_checkpoint(
                    &reconcile,
                    MediaListDiscoveryV2Page {
                        complete: false,
                        next_cursor: Some("1".to_owned()),
                        total_hint: Some(2),
                        revision_token: Some("partial-reconcile".to_owned()),
                        strategy: WireIncrementalStrategy::SkipKnown,
                        examined_count: 1,
                        archived_skipped_count: 0,
                        emitted_count: 1,
                        records: vec![
                            list_record("fixture/incomplete-reconcile"),
                            item_record("first", 0, "First overview"),
                        ],
                    }
                    .into(),
                    Some(&staged),
                )
                .await
                .expect("commit incomplete reconcile page");
            assert!(matches!(
                outcome,
                PageCommitOutcome::Committed { complete: false }
            ));
            let before_cancel: i64 = sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM "AddonMediaListItemTombstone"
                   WHERE "AddonMediaListItemId" IN (
                       SELECT "Id" FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ?
                   )"#,
            )
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .expect("count incomplete reconcile evidence");
            assert_eq!(before_cancel, 0);

            coordinator
                .control_import(list_id, AddonMediaListImportAction::Cancel)
                .await
                .expect("cancel incomplete reconcile");
            let after_cancel: i64 = sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM "AddonMediaListItemTombstone"
                   WHERE "AddonMediaListItemId" IN (
                       SELECT "Id" FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ?
                   )"#,
            )
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .expect("count cancelled reconcile evidence");
            assert_eq!(after_cancel, 0);
            staged.discard_candidate().await;
        }

        #[tokio::test]
        #[allow(clippy::too_many_lines)] // Follow the complete staged reconciliation failure against its saved baseline.
        async fn late_reconcile_failure_does_not_advance_existing_missing_evidence() {
            let (temporary, pool, coordinator) = fixture().await;
            let data_dir = temporary.path().join("private data");
            let source_url = "https://media.example.test/late-reconcile-failure";
            let list_id = queue_v2(&coordinator, source_url, &data_dir, None).await;
            let initial = coordinator.claim_import_job().await.unwrap().unwrap();
            let initial_archive = seal_v2_archive(
                &coordinator,
                &initial,
                &data_dir,
                b"fixture first\nfixture second\n",
            )
            .await;
            commit_complete_v2_page(
                &coordinator,
                &initial,
                &initial_archive,
                2,
                2,
                0,
                vec![
                    list_record("fixture/late-reconcile-failure"),
                    item_record("first", 0, "First overview"),
                    item_record("second", 1, "Second overview"),
                ],
            )
            .await;
            let missing_item_id: EntityId = sqlx::query_scalar(
                r#"SELECT "Id" FROM "AddonMediaListItem"
                   WHERE "AddonMediaListId" = ? AND "ProviderId" = 'second'"#,
            )
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .expect("load future missing item");
            sqlx::query(
                r#"INSERT INTO "AddonMediaListItemTombstone" (
                       "AddonMediaListItemId", "MissingSince", "LastObservedAt",
                       "ObservationCount", "LastReconcileGeneration"
                   ) VALUES (?, '2026-09-01T00:00:00Z', '2026-09-01T00:00:00Z', 1, 1)"#,
            )
            .bind(missing_item_id)
            .execute(&pool)
            .await
            .expect("seed first missing observation");

            queue_v2(&coordinator, source_url, &data_dir, Some(list_id)).await;
            sqlx::query(
                r#"UPDATE "AddonMediaListImportJob"
                   SET "Purpose" = ?, "Mode" = ?, "CheckpointId" = NULL
                   WHERE "Id" = (
                       SELECT "Id" FROM "AddonMediaListImportJob"
                       WHERE "AddonMediaListId" = ? ORDER BY "Generation" DESC LIMIT 1
                   )"#,
            )
            .bind(encode_enum(AddonMediaListImportPurpose::Reconcile))
            .bind(encode_enum(AddonMediaListImportMode::Full))
            .bind(list_id)
            .execute(&pool)
            .await
            .expect("convert queued import to reconcile fixture");
            let reconcile = coordinator.claim_import_job().await.unwrap().unwrap();
            let staged =
                seal_v2_archive(&coordinator, &reconcile, &data_dir, b"fixture first\n").await;
            validate_archive_advance(&staged, 1).expect("valid reconcile archive");
            let outcome = coordinator
                .commit_import_page_checkpoint(
                    &reconcile,
                    MediaListDiscoveryV2Page {
                        complete: true,
                        next_cursor: None,
                        total_hint: Some(1),
                        revision_token: Some("late-failure".to_owned()),
                        strategy: WireIncrementalStrategy::SkipKnown,
                        examined_count: 1,
                        archived_skipped_count: 0,
                        emitted_count: 1,
                        records: vec![
                            list_record("fixture/late-reconcile-failure"),
                            item_record("first", 0, "First overview"),
                        ],
                    }
                    .into(),
                    Some(&staged),
                )
                .await
                .expect("checkpoint complete reconcile page");
            assert!(matches!(
                outcome,
                PageCommitOutcome::Committed { complete: true }
            ));
            sqlx::raw_sql(
                r#"CREATE TRIGGER "TR_AddonMediaListDetailJob_FailReconcileFixture"
                   BEFORE UPDATE ON "AddonMediaListDetailJob"
                   BEGIN
                       SELECT RAISE(ABORT, 'fixture detail failure');
                   END;"#,
            )
            .execute(&pool)
            .await
            .expect("install late-failure trigger");

            let error = coordinator
                .finalize_import_pass(&reconcile)
                .await
                .expect_err("late detail initialization must fail");
            coordinator
                .fail_import(&reconcile, &error.to_string(), false)
                .await
                .expect("record terminal reconcile failure");

            let evidence: (i32, i32, Option<String>, Option<String>) = sqlx::query_as(
                r#"SELECT "ObservationCount", "LastReconcileGeneration", "ConfirmedAt",
                          "ReappearedAt"
                   FROM "AddonMediaListItemTombstone"
                   WHERE "AddonMediaListItemId" = ?"#,
            )
            .bind(missing_item_id)
            .fetch_one(&pool)
            .await
            .expect("load unchanged missing evidence");
            assert_eq!(evidence, (1, 1, None, None));
            let state: i32 = sqlx::query_scalar(
                r#"SELECT "State" FROM "AddonMediaListImportJob" WHERE "Id" = ?"#,
            )
            .bind(reconcile.id)
            .fetch_one(&pool)
            .await
            .expect("load failed reconcile state");
            assert_eq!(
                state,
                encode_enum(AddonMediaListImportState::NeedsAttention)
            );
            let checkpoints: (i64, i64) = sqlx::query_as(
                r#"SELECT
                       SUM(CASE WHEN "Committed" = 1 THEN 1 ELSE 0 END),
                       SUM(CASE WHEN "Committed" = 0 THEN 1 ELSE 0 END)
                   FROM "AddonMediaListImportCheckpoint" WHERE "AddonMediaListId" = ?"#,
            )
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .expect("load archive checkpoint states");
            assert_eq!(checkpoints, (1, 1));

            sqlx::query(r#"DROP TRIGGER "TR_AddonMediaListDetailJob_FailReconcileFixture""#)
                .execute(&pool)
                .await
                .expect("remove late-failure trigger");
            coordinator
                .control_import(list_id, AddonMediaListImportAction::Retry)
                .await
                .expect("retry failed reconcile in place");
            let retried = coordinator
                .claim_import_job()
                .await
                .expect("claim retried reconcile")
                .expect("retried reconcile job");
            assert_eq!(retried.id, reconcile.id);
            assert_eq!(retried.purpose, AddonMediaListImportPurpose::Reconcile);
            coordinator
                .finalize_import_pass(&retried)
                .await
                .expect("finish retried reconcile");
            let accepted_evidence: (i32, i32, Option<String>) = sqlx::query_as(
                r#"SELECT "ObservationCount", "LastReconcileGeneration", "ConfirmedAt"
                   FROM "AddonMediaListItemTombstone"
                   WHERE "AddonMediaListItemId" = ?"#,
            )
            .bind(missing_item_id)
            .fetch_one(&pool)
            .await
            .expect("load successful second observation");
            assert_eq!(accepted_evidence.0, 2);
            assert_eq!(accepted_evidence.1, reconcile.generation);
            assert!(accepted_evidence.2.is_some());
            let staged_committed: i32 = sqlx::query_scalar(
                r#"SELECT "Committed" FROM "AddonMediaListImportCheckpoint"
                   WHERE "Id" = (
                       SELECT "CheckpointId" FROM "AddonMediaListImportJob" WHERE "Id" = ?
                   )"#,
            )
            .bind(reconcile.id)
            .fetch_one(&pool)
            .await
            .expect("load review-stage archive");
            assert_eq!(staged_committed, 0);
        }

        #[tokio::test]
        #[allow(clippy::too_many_lines)] // Keep cancellation and subsequent rediscovery in one lifecycle scenario.
        async fn cancelling_after_a_v2_page_keeps_unmerged_items_discoverable() {
            let (temporary, pool, coordinator) = fixture().await;
            let data_dir = temporary.path().join("private data");
            let source_url = "https://media.example.test/cancel-between-pages";
            let list_id = queue_v2(&coordinator, source_url, &data_dir, None).await;
            let full_job = coordinator.claim_import_job().await.unwrap().unwrap();
            let full_archive =
                seal_v2_archive(&coordinator, &full_job, &data_dir, b"fixture first\n").await;
            commit_complete_v2_page(
                &coordinator,
                &full_job,
                &full_archive,
                1,
                1,
                0,
                vec![
                    list_record("fixture/cancel-between-pages"),
                    item_record("first", 0, "First overview"),
                ],
            )
            .await;
            let committed_before = coordinator
                .current_import_checkpoint(list_id)
                .await
                .unwrap()
                .unwrap();

            queue_v2(&coordinator, source_url, &data_dir, Some(list_id)).await;
            let page_job = coordinator.claim_import_job().await.unwrap().unwrap();
            let page_archive = seal_v2_archive(
                &coordinator,
                &page_job,
                &data_dir,
                b"fixture first\nfixture second\n",
            )
            .await;
            validate_archive_advance(&page_archive, 1).unwrap();
            let outcome = coordinator
                .commit_import_page_checkpoint(
                    &page_job,
                    MediaListDiscoveryV2Page {
                        complete: false,
                        next_cursor: Some("page-2".to_owned()),
                        total_hint: Some(3),
                        revision_token: Some("generation-2".to_owned()),
                        strategy: WireIncrementalStrategy::SkipKnown,
                        examined_count: 2,
                        archived_skipped_count: 1,
                        emitted_count: 1,
                        records: vec![item_record("second", 1, "Second overview")],
                    }
                    .into(),
                    Some(&page_archive),
                )
                .await
                .expect("checkpoint first incremental page");
            assert!(matches!(
                outcome,
                PageCommitOutcome::Committed { complete: false }
            ));
            assert_eq!(
                coordinator
                    .current_import_checkpoint(list_id)
                    .await
                    .unwrap()
                    .unwrap()
                    .relative_path,
                committed_before.relative_path
            );

            coordinator
                .control_import(list_id, AddonMediaListImportAction::Cancel)
                .await
                .expect("cancel partial generation");
            queue_v2(&coordinator, source_url, &data_dir, Some(list_id)).await;
            let unchanged_job = coordinator.claim_import_job().await.unwrap().unwrap();
            let resumed_from = coordinator
                .resume_import_checkpoint(&unchanged_job)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(resumed_from.relative_path, committed_before.relative_path);
            assert_eq!(resumed_from.entry_count, 1);
            let unchanged_archive =
                seal_v2_archive(&coordinator, &unchanged_job, &data_dir, b"fixture first\n").await;
            commit_complete_v2_page(
                &coordinator,
                &unchanged_job,
                &unchanged_archive,
                1,
                1,
                1,
                Vec::new(),
            )
            .await;
            let retained_name: String =
                sqlx::query_scalar(r#"SELECT "Name" FROM "AddonMediaList" WHERE "Id" = ?"#)
                    .bind(list_id)
                    .fetch_one(&pool)
                    .await
                    .expect("load retained committed header");
            assert_eq!(retained_name, "Fixture programmes");

            queue_v2(&coordinator, source_url, &data_dir, Some(list_id)).await;
            let restarted_job = coordinator.claim_import_job().await.unwrap().unwrap();
            let restarted_archive = seal_v2_archive(
                &coordinator,
                &restarted_job,
                &data_dir,
                b"fixture first\nfixture second\n",
            )
            .await;
            commit_complete_v2_page(
                &coordinator,
                &restarted_job,
                &restarted_archive,
                2,
                2,
                1,
                vec![item_record("second", 1, "Second overview")],
            )
            .await;
            let identities: Vec<String> = sqlx::query_scalar(
                r#"SELECT "ProviderId" FROM "AddonMediaListItem"
                   WHERE "AddonMediaListId" = ? ORDER BY "Rank""#,
            )
            .bind(list_id)
            .fetch_all(&pool)
            .await
            .expect("load recovered delta");
            assert_eq!(identities, vec!["first".to_owned(), "second".to_owned()]);
        }

        #[tokio::test]
        async fn failed_archive_cleanup_keeps_its_locator_for_restart_retry() {
            let (temporary, pool, coordinator) = fixture().await;
            let data_dir = temporary.path().join("private data");
            let list_id = queue_v2(
                &coordinator,
                "https://media.example.test/archive-cleanup",
                &data_dir,
                None,
            )
            .await;
            let relative_path =
                format!("media-list-checkpoints/list-{list_id}/obsolete/archive.txt");
            let archive_path = data_dir.join(&relative_path);
            tokio::fs::create_dir_all(&archive_path)
                .await
                .expect("create directory that cannot be removed as a file");
            sqlx::query(
                r#"INSERT INTO "AddonMediaListImportCheckpoint" (
                       "AddonMediaListId", "RelativePath", "Generation", "Sha256",
                       "EntryCount", "PromotedAt", "DataRoot", "Committed"
                   ) VALUES (?, ?, 1, 'obsolete', 1, ?, ?, 0)"#,
            )
            .bind(list_id)
            .bind(&relative_path)
            .bind(encode_datetime(Utc::now()))
            .bind(data_dir.to_string_lossy().into_owned())
            .execute(&pool)
            .await
            .expect("persist obsolete archive locator");

            assert!(coordinator
                .cleanup_one_unreferenced_import_archive(Some(list_id))
                .await
                .is_err());
            let retained: i64 = sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM "AddonMediaListImportCheckpoint"
                   WHERE "AddonMediaListId" = ? AND "Committed" = 0"#,
            )
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .expect("count retained cleanup locator");
            assert_eq!(retained, 1);
            assert!(coordinator
                .process_one_import_page()
                .await
                .expect("cleanup failure must not block import work"));

            tokio::fs::remove_dir(&archive_path)
                .await
                .expect("replace blocking directory");
            tokio::fs::write(&archive_path, b"fixture obsolete\n")
                .await
                .expect("restore removable archive");
            let restarted = AddonMediaListImportCoordinator::new(pool.clone());
            assert!(restarted
                .cleanup_one_unreferenced_import_archive(Some(list_id))
                .await
                .expect("retry cleanup after restart"));
            assert!(!archive_path.exists());
            let remaining: i64 = sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM "AddonMediaListImportCheckpoint"
                   WHERE "AddonMediaListId" = ? AND "Committed" = 0"#,
            )
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .expect("count cleaned archive locators");
            assert_eq!(remaining, 0);
        }

        #[tokio::test]
        #[allow(clippy::too_many_lines)]
        async fn empty_completed_overview_retries_after_package_update_without_operator_state_loss()
        {
            let (_temporary, pool, coordinator) = fixture().await;
            let source_url = "https://www.youtube.com/watch?v=mhJRzQsLZGg";
            let list_id = queue(
                &coordinator,
                source_url,
                AddonMediaListDetailPolicy::ManualOnly,
            )
            .await;
            let empty_job = coordinator
                .claim_import_job()
                .await
                .expect("claim empty provider generation")
                .expect("empty provider generation");
            coordinator
                .commit_import_page(
                    &empty_job,
                    MediaListDiscoveryPage {
                        complete: true,
                        next_cursor: None,
                        total_hint: Some(0),
                        revision_token: Some("broken-package".to_owned()),
                        records: vec![list_record("youtube/mhJRzQsLZGg")],
                    },
                )
                .await
                .expect("persist legacy empty provider generation");
            let empty_progress = coordinator.progress(list_id).await.unwrap().unwrap();
            assert_eq!(empty_progress.state, AddonMediaListImportState::Ready);
            assert_eq!(empty_progress.discovered_count, 0);

            sqlx::query(
                r#"UPDATE "AddonMediaList"
                   SET "AutoRefresh" = 1, "GeneratePlaylist" = 1,
                       "MetadataOverrideJson" = '{"name":"Operator title"}',
                       "PresentationKind" = 1, "RequestedPresentationKind" = 1
                   WHERE "Id" = ?"#,
            )
            .bind(list_id)
            .execute(&pool)
            .await
            .expect("set operator-owned list state");
            sqlx::query(
                r#"UPDATE "AddonMediaListImportJob" SET "RequestedFolder" = 'operator-folder'
                   WHERE "Id" = ?"#,
            )
            .bind(empty_job.id)
            .execute(&pool)
            .await
            .expect("set requested folder");
            let operator_state = sqlx::query(
                r#"SELECT "AddonId", "SourceUrl", "AutoRefresh", "GeneratePlaylist",
                          "MetadataOverrideJson", "PresentationKind",
                          "RequestedPresentationKind", "DetailImportPolicy"
                   FROM "AddonMediaList" WHERE "Id" = ?"#,
            )
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .expect("operator state before retry");

            coordinator
                .control_import(list_id, AddonMediaListImportAction::Retry)
                .await
                .expect("retry empty provider generation");
            let generations = sqlx::query(
                r#"SELECT "Id", "Generation", "State", "RequestedFolder", "DiscoveredCount"
                   FROM "AddonMediaListImportJob"
                   WHERE "AddonMediaListId" = ? ORDER BY "Generation""#,
            )
            .bind(list_id)
            .fetch_all(&pool)
            .await
            .expect("retry generations");
            assert_eq!(generations.len(), 2);
            assert_eq!(
                generations[0].try_get::<i32, _>("State").unwrap(),
                encode_enum(AddonMediaListImportState::Superseded)
            );
            assert_eq!(generations[1].try_get::<i32, _>("Generation").unwrap(), 2);
            assert_eq!(
                generations[1].try_get::<i32, _>("State").unwrap(),
                encode_enum(AddonMediaListImportState::Queued)
            );
            assert_eq!(
                generations[1]
                    .try_get::<Option<String>, _>("RequestedFolder")
                    .unwrap()
                    .as_deref(),
                Some("operator-folder")
            );
            assert_eq!(
                generations[1].try_get::<i32, _>("DiscoveredCount").unwrap(),
                0
            );
            let retained_state = sqlx::query(
                r#"SELECT "AddonId", "SourceUrl", "AutoRefresh", "GeneratePlaylist",
                          "MetadataOverrideJson", "PresentationKind",
                          "RequestedPresentationKind", "DetailImportPolicy"
                   FROM "AddonMediaList" WHERE "Id" = ?"#,
            )
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .expect("operator state after retry");
            for column in ["AddonId", "SourceUrl", "MetadataOverrideJson"] {
                assert_eq!(
                    retained_state.try_get::<String, _>(column).unwrap(),
                    operator_state.try_get::<String, _>(column).unwrap(),
                    "retry must preserve {column}"
                );
            }
            for column in [
                "AutoRefresh",
                "GeneratePlaylist",
                "PresentationKind",
                "RequestedPresentationKind",
                "DetailImportPolicy",
            ] {
                assert_eq!(
                    retained_state.try_get::<i32, _>(column).unwrap(),
                    operator_state.try_get::<i32, _>(column).unwrap(),
                    "retry must preserve {column}"
                );
            }

            let repaired_job = coordinator
                .claim_import_job()
                .await
                .expect("claim repaired generation")
                .expect("repaired provider generation");
            assert_ne!(repaired_job.id, empty_job.id);
            assert!(repaired_job.cursor.is_none());
            assert_eq!(repaired_job.verification_pass, 0);
            coordinator
                .commit_import_page(
                    &repaired_job,
                    MediaListDiscoveryPage {
                        complete: true,
                        next_cursor: None,
                        total_hint: Some(1),
                        revision_token: Some("fixed-package".to_owned()),
                        records: vec![
                            list_record("youtube/mhJRzQsLZGg"),
                            item_record("mhJRzQsLZGg", 0, "NSF Live overview"),
                        ],
                    },
                )
                .await
                .expect("promote repaired provider generation");
            let repaired_progress = coordinator.progress(list_id).await.unwrap().unwrap();
            assert_eq!(repaired_progress.state, AddonMediaListImportState::Ready);
            assert_eq!(repaired_progress.discovered_count, 1);
            let repaired_items: Vec<String> = sqlx::query_scalar(
                r#"SELECT "ProviderId" FROM "AddonMediaListItem"
                   WHERE "AddonMediaListId" = ? ORDER BY "Rank""#,
            )
            .bind(list_id)
            .fetch_all(&pool)
            .await
            .expect("repaired review items");
            assert_eq!(repaired_items, ["mhJRzQsLZGg".to_owned()]);
        }

        #[tokio::test]
        async fn empty_ready_reconcile_cannot_use_the_generic_retry_path() {
            let (temporary, pool, coordinator) = fixture().await;
            let data_dir = temporary.path().join("private data");
            let list_id = queue_v2(
                &coordinator,
                "https://media.example.test/empty-reconcile",
                &data_dir,
                None,
            )
            .await;
            let job_id: EntityId = sqlx::query_scalar(
                r#"SELECT "Id" FROM "AddonMediaListImportJob"
                   WHERE "AddonMediaListId" = ?"#,
            )
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .expect("load reconcile fixture job");
            sqlx::query(
                r#"UPDATE "AddonMediaListImportJob"
                   SET "State" = ?, "Purpose" = ?, "Mode" = ?, "DiscoveredCount" = 0
                   WHERE "Id" = ?"#,
            )
            .bind(encode_enum(AddonMediaListImportState::Ready))
            .bind(encode_enum(AddonMediaListImportPurpose::Reconcile))
            .bind(encode_enum(AddonMediaListImportMode::Full))
            .bind(job_id)
            .execute(&pool)
            .await
            .expect("mark empty reconcile ready");

            let error = coordinator
                .control_import(list_id, AddonMediaListImportAction::Retry)
                .await
                .expect_err("generic retry must not replace a ready reconcile");
            assert!(matches!(error, RepositoryError::Conflict(_)));
            let row: (i64, i32, i32) = sqlx::query_as(
                r#"SELECT COUNT(*), MAX("Generation"), MAX("Purpose")
                   FROM "AddonMediaListImportJob" WHERE "AddonMediaListId" = ?"#,
            )
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .expect("load retained reconcile job");
            assert_eq!(
                row,
                (1, 1, encode_enum(AddonMediaListImportPurpose::Reconcile))
            );
        }

        #[tokio::test]
        #[allow(clippy::too_many_lines)]
        async fn official_source_url_matrix_reaches_nonempty_review_and_publication() {
            use ersatzrs_domain::RemoteStreamLiveness;

            let (temporary, pool, coordinator) = fixture().await;
            let managed_root = temporary.path().join("managed-matrix");
            std::fs::create_dir(&managed_root).expect("managed matrix root");
            configure_managed_storage(&pool, &managed_root).await;
            let matrix = vec![
                (
                    "https://www.youtube.com/watch?v=mhJRzQsLZGg",
                    "youtube/direct/mhJRzQsLZGg",
                    vec![remote_stream_item_record("mhJRzQsLZGg", 0, "live", None)],
                    Some(RemoteStreamLiveness::Live),
                ),
                (
                    "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
                    "youtube/direct/dQw4w9WgXcQ",
                    vec![remote_stream_item_record(
                        "dQw4w9WgXcQ",
                        0,
                        "finite",
                        Some(213),
                    )],
                    Some(RemoteStreamLiveness::Finite),
                ),
                (
                    "https://www.youtube.com/playlist?list=PLJ_TJFLc25JRdChhr9sp00ajl7FAeKDx-",
                    "youtube/playlist/PLJ_TJFLc25JRdChhr9sp00ajl7FAeKDx-",
                    vec![
                        remote_stream_item_record("playlist-one", 0, "finite", Some(60)),
                        remote_stream_item_record("playlist-two", 1, "live", None),
                        remote_stream_item_record("playlist-three", 2, "finite", Some(90)),
                    ],
                    Some(RemoteStreamLiveness::Finite),
                ),
                (
                    "https://schatkamer.beeldengeluid.nl/lijst/14df8d33-ce8a-4680-a83b-0cc2a9c58bcd",
                    "beeldengeluid/list/14df8d33-ce8a-4680-a83b-0cc2a9c58bcd",
                    vec![
                        item_record("shared-list-one", 0, "Shared list one"),
                        item_record("shared-list-two", 1, "Shared list two"),
                    ],
                    None,
                ),
                (
                    "https://schatkamer.beeldengeluid.nl/zoeken?q=reclame&startdatum=01-01-1995&einddatum=01-03-1995",
                    "beeldengeluid/search/reclame-1995",
                    vec![
                        item_record("dated-search-one", 0, "Dated search one"),
                        item_record("dated-search-two", 1, "Dated search two"),
                    ],
                    None,
                ),
                (
                    "https://schatkamer.beeldengeluid.nl/serie/2101608030022669331/de-fascinaties-van-boudewijn-buch",
                    "beeldengeluid/series/2101608030022669331",
                    vec![
                        item_record("series-one", 0, "Series one"),
                        item_record("series-two", 1, "Series two"),
                    ],
                    None,
                ),
                (
                    "https://schatkamer.beeldengeluid.nl/serie/2101608030021828731/ster-reclame/aflevering/2101608040033953431",
                    "beeldengeluid/episode/2101608040033953431",
                    vec![item_record(
                        "episode:2101608040033953431",
                        0,
                        "Direct episode",
                    )],
                    None,
                ),
            ];
            let repository = SqliteAddonMediaListRepository(pool.clone());

            for (matrix_rank, (source_url, provider_list_id, items, expected_liveness)) in
                matrix.into_iter().enumerate()
            {
                let expected_count = items.len();
                let records = std::iter::once(AddonMediaListRecord::list(
                    provider_list_id,
                    format!("Matrix source {matrix_rank}"),
                ))
                .chain(items)
                .collect::<Vec<_>>();
                let requested_folder = format!("matrix-{matrix_rank}");
                let list_id = coordinator
                    .queue_import(
                        "org.example.fixture",
                        &context(source_url),
                        Some(&requested_folder),
                        AddonMediaListContentKind::Auto,
                        AddonMediaListDetailPolicy::ManualOnly,
                        None,
                    )
                    .await
                    .expect("queue matrix source");
                let job = coordinator
                    .claim_import_job()
                    .await
                    .expect("claim matrix source")
                    .expect("queued matrix source");
                coordinator
                    .commit_import_page(
                        &job,
                        MediaListDiscoveryPage {
                            complete: true,
                            next_cursor: None,
                            total_hint: Some(
                                u32::try_from(expected_count).expect("matrix item count"),
                            ),
                            revision_token: Some(format!("matrix-{matrix_rank}")),
                            records,
                        },
                    )
                    .await
                    .expect("commit matrix overview");
                let progress = coordinator.progress(list_id).await.unwrap().unwrap();
                assert_eq!(progress.state, AddonMediaListImportState::Ready);
                assert_eq!(
                    progress.discovered_count,
                    u64::try_from(expected_count).expect("matrix item count")
                );
                let review = repository
                    .list_items(list_id)
                    .await
                    .expect("load matrix review");
                assert_eq!(review.len(), expected_count);
                assert_eq!(
                    review.iter().map(|item| item.rank).collect::<Vec<_>>(),
                    (0..i32::try_from(expected_count).expect("matrix ranks")).collect::<Vec<_>>()
                );
                if let Some(expected_liveness) = expected_liveness {
                    assert_eq!(review[0].effective_liveness, expected_liveness);
                }

                repository
                    .publish(list_id)
                    .await
                    .expect("publish matrix source");
                let published = repository
                    .list_items(list_id)
                    .await
                    .expect("load published matrix source");
                assert!(published.iter().all(|item| item.published));
                if let Some(expected_liveness) = expected_liveness {
                    assert_eq!(published[0].published_liveness, expected_liveness);
                }
            }
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

        async fn detail_option_fixture() -> (
            tempfile::TempDir,
            DbPool,
            AddonMediaListImportCoordinator,
            EntityId,
            EntityId,
        ) {
            detail_option_fixture_for(&["episode:1"]).await
        }

        async fn detail_option_fixture_for(
            providers: &[&str],
        ) -> (
            tempfile::TempDir,
            DbPool,
            AddonMediaListImportCoordinator,
            EntityId,
            EntityId,
        ) {
            let (temporary, pool, coordinator) = fixture().await;
            let list_id = complete_single_page(
                &coordinator,
                "https://media.example.test/options",
                AddonMediaListDetailPolicy::ManualOnly,
                "fixture/options",
                providers,
            )
            .await;
            let item_id: EntityId = sqlx::query_scalar(
                r#"SELECT "Id" FROM "AddonMediaListItem" WHERE "AddonMediaListId"=? ORDER BY "Id" LIMIT 1"#,
            )
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .unwrap();
            let (definition, schema) = option_schema_fixture();
            sqlx::query(r#"INSERT INTO "AddonMediaListOptionSchema" ("Id","AddonId","CapabilityId","SchemaDigest","DefinitionJson","CreatedAt")
                VALUES (101,'org.example.fixture','media-list.options.v1',?,?,?)"#)
                .bind(schema.schema_digest).bind(definition).bind(encode_datetime(Utc::now())).execute(&pool).await.unwrap();
            sqlx::query(r#"UPDATE "AddonMediaList" SET "DraftOptionSchemaId"=101,"DraftOptionValuesJson"='{"enabled":true}' WHERE "Id"=?"#)
                .bind(list_id).execute(&pool).await.unwrap();
            (temporary, pool, coordinator, list_id, item_id)
        }

        #[tokio::test]
        async fn sibling_draft_detail_completion_preserves_completed_item_context() {
            let (_temporary, pool, coordinator, list_id, _) =
                detail_option_fixture_for(&["episode:1", "episode:2"]).await;
            let item_ids: Vec<EntityId> = sqlx::query_scalar(
                r#"SELECT "Id" FROM "AddonMediaListItem" WHERE "AddonMediaListId"=? ORDER BY "Id""#,
            )
            .bind(list_id)
            .fetch_all(&pool)
            .await
            .unwrap();
            sqlx::query(r#"UPDATE "AddonMediaList" SET "PublishedRevision"=1 WHERE "Id"=?"#)
                .bind(list_id)
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query(
                r#"UPDATE "AddonMediaListItem" SET "Published"=1 WHERE "AddonMediaListId"=?"#,
            )
            .bind(list_id)
            .execute(&pool)
            .await
            .unwrap();
            let before: i32 =
                sqlx::query_scalar(r#"SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id"=?"#)
                    .bind(list_id)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            for item_id in &item_ids {
                coordinator
                    .request_prepublication_details(list_id, *item_id)
                    .await
                    .unwrap();
            }
            for expected_item_id in &item_ids {
                // Advance persisted eligibility, never sleep or invoke a provider.
                sqlx::query(r#"UPDATE "AddonProviderHostCooldown" SET "NextEligibleAt"=?"#)
                    .bind(encode_datetime(Utc::now() - Duration::seconds(1)))
                    .execute(&pool)
                    .await
                    .unwrap();
                let job = coordinator.claim_detail_job().await.unwrap().unwrap();
                assert_eq!(job.item_id, *expected_item_id);
                let mut record =
                    serde_json::to_value(item_record(&job.provider_id, 0, "Enriched draft"))
                        .unwrap();
                // Its own discovered source/liveness must be reflected atomically
                // in the completed context, not treated as a later operator edit.
                record["source_url"] =
                    serde_json::json!(format!("https://media.example.test/{}", job.provider_id));
                record["liveness"] = serde_json::json!("finite");
                record["duration_seconds"] = serde_json::json!(120);
                let record = serde_json::from_value(record).unwrap();
                coordinator
                    .checkpoint_detail_result(
                        &job,
                        MediaListEnrichOutcomeKind::Complete,
                        None,
                        &record,
                    )
                    .await
                    .unwrap();
                assert!(coordinator.apply_one_checkpointed_detail().await.unwrap());
                coordinator
                    .request_prepublication_details(list_id, job.item_id)
                    .await
                    .unwrap();
                let state: i32 = sqlx::query_scalar(r#"SELECT "State" FROM "AddonMediaListDetailJob" WHERE "AddonMediaListItemId"=?"#)
                    .bind(job.item_id).fetch_one(&pool).await.unwrap();
                assert_eq!(state, encode_enum(AddonMediaListDetailState::Complete));
            }
            let after: i32 =
                sqlx::query_scalar(r#"SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id"=?"#)
                    .bind(list_id)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(after, before + 2);
            // Repeating the complete two-item preflight must preserve A even
            // though B advanced the list-wide revision in the meantime.
            for item_id in &item_ids {
                coordinator
                    .request_prepublication_details(list_id, *item_id)
                    .await
                    .unwrap();
            }
            let states: Vec<i32> = sqlx::query_scalar(r#"SELECT "State" FROM "AddonMediaListDetailJob" WHERE "AddonMediaListId"=? ORDER BY "AddonMediaListItemId""#)
                .bind(list_id).fetch_all(&pool).await.unwrap();
            assert_eq!(
                states,
                vec![encode_enum(AddonMediaListDetailState::Complete); 2]
            );
        }

        #[tokio::test]
        async fn draft_option_edit_rejects_detail_checkpoint_and_replay() {
            let (_temporary, pool, coordinator, list_id, item_id) = detail_option_fixture().await;
            coordinator
                .request_prepublication_details(list_id, item_id)
                .await
                .unwrap();
            let job = coordinator.claim_detail_job().await.unwrap().unwrap();
            coordinator
                .checkpoint_detail_result(
                    &job,
                    MediaListEnrichOutcomeKind::Complete,
                    None,
                    &item_record("episode:1", 0, "Stale option result"),
                )
                .await
                .unwrap();
            sqlx::query(r#"UPDATE "AddonMediaList" SET "DraftOptionValuesJson"='{"enabled":false}', "DraftRevision"="DraftRevision"+1 WHERE "Id"=?"#)
                .bind(list_id).execute(&pool).await.unwrap();
            assert!(coordinator
                .checkpoint_detail_result(
                    &job,
                    MediaListEnrichOutcomeKind::Complete,
                    None,
                    &item_record("episode:1", 0, "Stale option result")
                )
                .await
                .is_err());
            assert!(coordinator.apply_one_checkpointed_detail().await.unwrap());
            let (state, title): (i32, String) = sqlx::query_as(r#"SELECT "detail"."State", "item"."Title"
                FROM "AddonMediaListDetailJob" AS "detail" JOIN "AddonMediaListItem" AS "item" ON "item"."Id"="detail"."AddonMediaListItemId"
                WHERE "item"."Id"=?"#).bind(item_id).fetch_one(&pool).await.unwrap();
            assert_eq!(state, encode_enum(AddonMediaListDetailState::Failed));
            assert_ne!(title, "Stale option result");
        }

        #[tokio::test]
        async fn published_detail_context_ignores_draft_edits_but_draft_purpose_does_not() {
            let (_temporary, pool, _coordinator, list_id, item_id) = detail_option_fixture().await;
            sqlx::query(r#"INSERT INTO "AddonMediaListRevision" ("AddonMediaListId","Revision","OptionSchemaId","OptionValuesJson","CompletedAt")
                VALUES (?,1,101,'{"enabled":true}',?)"#).bind(list_id).bind(encode_datetime(Utc::now())).execute(&pool).await.unwrap();
            sqlx::query(r#"UPDATE "AddonMediaList" SET "PublishedRevision"=1 WHERE "Id"=?"#)
                .bind(list_id)
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query(r#"UPDATE "AddonMediaListItem" SET "Published"=1 WHERE "Id"=?"#)
                .bind(item_id)
                .execute(&pool)
                .await
                .unwrap();
            let mut tx = crate::db::begin_write(&pool).await.unwrap();
            let published =
                crate::addon_media_list_detail_options::capture(&mut tx, list_id, item_id, 0)
                    .await
                    .unwrap();
            let draft =
                crate::addon_media_list_detail_options::capture(&mut tx, list_id, item_id, 1)
                    .await
                    .unwrap();
            sqlx::query(r#"UPDATE "AddonMediaList" SET "DraftOptionValuesJson"='{"enabled":false}',"DraftRevision"="DraftRevision"+1 WHERE "Id"=?"#)
                .bind(list_id).execute(&mut *tx).await.unwrap();
            assert!(crate::addon_media_list_detail_options::validate(
                &mut tx,
                list_id,
                item_id,
                0,
                Some(&published)
            )
            .await
            .is_ok());
            assert!(crate::addon_media_list_detail_options::validate(
                &mut tx,
                list_id,
                item_id,
                1,
                Some(&draft)
            )
            .await
            .is_err());
            tx.rollback().await.unwrap();
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
        async fn quiesced_detail_lease_rejects_stale_success_and_failure_responses() {
            let (_temporary, pool, coordinator) = fixture().await;
            let list_id = complete_single_page(
                &coordinator,
                "https://media.example.test/reset-detail-fence",
                AddonMediaListDetailPolicy::Automatic,
                "fixture/reset-detail-fence",
                &["episode:1"],
            )
            .await;
            coordinator
                .authorize_after_publish(list_id)
                .await
                .expect("authorize automatic detail");
            let detail = coordinator
                .claim_detail_job()
                .await
                .expect("claim detail")
                .expect("detail job");
            sqlx::query(
                r#"UPDATE "AddonMediaListDetailJob"
                   SET "State" = ?, "LeaseOwner" = NULL, "LeaseExpiresAt" = NULL,
                       "Diagnostic" = 'reset_quiesced'
                   WHERE "AddonMediaListItemId" = ?"#,
            )
            .bind(encode_enum(AddonMediaListDetailState::Failed))
            .bind(detail.item_id)
            .execute(&pool)
            .await
            .expect("quiesce detail job");

            let item = item_record("episode:1", 0, "Stale full detail");
            assert!(matches!(
                coordinator
                    .checkpoint_detail_result(
                        &detail,
                        MediaListEnrichOutcomeKind::Complete,
                        None,
                        &item,
                    )
                    .await,
                Err(RepositoryError::Conflict(_))
            ));
            assert!(matches!(
                coordinator
                    .finish_detail_failure(&detail, "stale failure", false, None)
                    .await,
                Err(RepositoryError::Conflict(_))
            ));
            let row = sqlx::query(
                r#"SELECT "Diagnostic", "ResultRecordJson" FROM "AddonMediaListDetailJob"
                   WHERE "AddonMediaListItemId" = ?"#,
            )
            .bind(detail.item_id)
            .fetch_one(&pool)
            .await
            .expect("load fenced detail state");
            assert_eq!(
                row.try_get::<String, _>("Diagnostic").unwrap(),
                "reset_quiesced"
            );
            assert!(row
                .try_get::<Option<String>, _>("ResultRecordJson")
                .unwrap()
                .is_none());
        }

        #[tokio::test]
        #[allow(clippy::too_many_lines)]
        async fn checkpointed_review_change_waits_for_publication_then_creates_one_draft() {
            let (_temporary, pool, coordinator) = fixture().await;
            let list_id = complete_single_page(
                &coordinator,
                "https://media.example.test/publication-detail-race",
                AddonMediaListDetailPolicy::ManualOnly,
                "fixture/publication-detail-race",
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
                    &item_record("episode:1", 0, "Changed provider detail"),
                )
                .await
                .expect("checkpoint changed provider detail");

            sqlx::query(
                r#"UPDATE "AddonMediaListItem"
                   SET "Published" = 1, "MetadataOverrideJson" = '{"plot":"Operator plot"}'
                   WHERE "Id" = ?"#,
            )
            .bind(item_id)
            .execute(&pool)
            .await
            .expect("seed published item override");
            sqlx::query(
                r#"UPDATE "AddonMediaList"
                   SET "ReviewState" = ?, "PublishedRevision" = 1
                   WHERE "Id" = ?"#,
            )
            .bind(encode_enum(
                ersatzrs_domain::AddonMediaListReviewState::Publishing,
            ))
            .bind(list_id)
            .execute(&pool)
            .await
            .expect("start publication");
            let draft_before: i32 = sqlx::query_scalar(
                r#"SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id" = ?"#,
            )
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .expect("draft before deferred merge");
            let metadata_before: String = sqlx::query_scalar(
                r#"SELECT "ProviderMetadataJson" FROM "AddonMediaListItem" WHERE "Id" = ?"#,
            )
            .bind(item_id)
            .fetch_one(&pool)
            .await
            .expect("metadata before deferred merge");

            assert!(coordinator
                .process_one_detail()
                .await
                .expect("defer checkpoint during publication"));
            let deferred: (i32, Option<String>, Option<String>) = sqlx::query_as(
                r#"SELECT "State", "ResultRecordJson", "LeaseExpiresAt"
                   FROM "AddonMediaListDetailJob" WHERE "AddonMediaListItemId" = ?"#,
            )
            .bind(item_id)
            .fetch_one(&pool)
            .await
            .expect("deferred detail state");
            assert_eq!(deferred.0, encode_enum(AddonMediaListDetailState::Running));
            assert!(
                deferred.1.is_some(),
                "the provider result must remain durable"
            );
            assert!(deferred.2.is_some(), "the retry must be paced");
            assert_eq!(
                sqlx::query_scalar::<_, i32>(
                    r#"SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id" = ?"#,
                )
                .bind(list_id)
                .fetch_one(&pool)
                .await
                .expect("unchanged draft revision"),
                draft_before
            );
            assert_eq!(
                sqlx::query_scalar::<_, String>(
                    r#"SELECT "ProviderMetadataJson" FROM "AddonMediaListItem" WHERE "Id" = ?"#,
                )
                .bind(item_id)
                .fetch_one(&pool)
                .await
                .expect("unchanged provider metadata"),
                metadata_before
            );
            assert!(
                !coordinator
                    .process_one_detail()
                    .await
                    .expect("paced deferral remains ineligible"),
                "a deferred checkpoint must not spin while publication is active"
            );

            sqlx::query(r#"UPDATE "AddonMediaList" SET "ReviewState" = ? WHERE "Id" = ?"#)
                .bind(encode_enum(
                    ersatzrs_domain::AddonMediaListReviewState::Published,
                ))
                .bind(list_id)
                .execute(&pool)
                .await
                .expect("complete publication");
            sqlx::query(
                r#"UPDATE "AddonMediaListDetailJob" SET "LeaseExpiresAt" = NULL
                   WHERE "AddonMediaListItemId" = ?"#,
            )
            .bind(item_id)
            .execute(&pool)
            .await
            .expect("make deferred checkpoint eligible");

            assert!(coordinator
                .process_one_detail()
                .await
                .expect("merge checkpoint after publication"));
            let final_list: (i32, i32) = sqlx::query_as(
                r#"SELECT "ReviewState", "DraftRevision"
                   FROM "AddonMediaList" WHERE "Id" = ?"#,
            )
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .expect("final list state");
            assert_eq!(
                final_list.0,
                encode_enum(ersatzrs_domain::AddonMediaListReviewState::Ready)
            );
            assert_eq!(final_list.1, draft_before.saturating_add(1));
            let final_detail: (i32, Option<String>) = sqlx::query_as(
                r#"SELECT "State", "ResultRecordJson" FROM "AddonMediaListDetailJob"
                   WHERE "AddonMediaListItemId" = ?"#,
            )
            .bind(item_id)
            .fetch_one(&pool)
            .await
            .expect("completed detail state");
            assert_eq!(
                final_detail.0,
                encode_enum(AddonMediaListDetailState::Complete)
            );
            assert!(final_detail.1.is_some());
            let final_metadata: String = sqlx::query_scalar(
                r#"SELECT "ProviderMetadataJson" FROM "AddonMediaListItem" WHERE "Id" = ?"#,
            )
            .bind(item_id)
            .fetch_one(&pool)
            .await
            .expect("merged provider metadata");
            assert!(final_metadata.contains("Changed provider detail"));
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
        async fn overview_import_respects_the_shared_provider_host_cooldown() {
            let (_temporary, pool, coordinator) = fixture().await;
            let list_id = queue(
                &coordinator,
                "https://media.example.test/shared-overview",
                AddonMediaListDetailPolicy::ManualOnly,
            )
            .await;
            let now = encode_datetime(Utc::now());
            let blocked_until = encode_datetime(Utc::now() + Duration::minutes(10));
            sqlx::query(
                r#"INSERT INTO "AddonProviderHostCooldown" (
                       "ProviderHost", "NextEligibleAt", "SampledDelaySeconds",
                       "LastAttemptAt", "UpdatedAt"
                   ) VALUES ('media.example.test', ?, 600, ?, ?)"#,
            )
            .bind(&blocked_until)
            .bind(&now)
            .bind(&now)
            .execute(&pool)
            .await
            .expect("seed shared provider cooldown");

            assert!(!coordinator.process_one_import_page().await.unwrap());

            let row = sqlx::query(
                r#"SELECT "State", "NextEligibleAt", "LeaseOwner", "LeaseExpiresAt"
                   FROM "AddonMediaListImportJob" WHERE "AddonMediaListId" = ?"#,
            )
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .expect("read deferred overview import");
            assert_eq!(
                row.try_get::<i32, _>("State").unwrap(),
                encode_enum(AddonMediaListImportState::Queued)
            );
            assert_eq!(
                row.try_get::<Option<String>, _>("NextEligibleAt")
                    .unwrap()
                    .as_deref(),
                Some(blocked_until.as_str())
            );
            assert!(row
                .try_get::<Option<String>, _>("LeaseOwner")
                .unwrap()
                .is_none());
            assert!(row
                .try_get::<Option<String>, _>("LeaseExpiresAt")
                .unwrap()
                .is_none());
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
        async fn v2_retry_resumes_the_same_checkpointed_generation() {
            let (_temporary, pool, coordinator) = fixture().await;
            let list_id = queue(
                &coordinator,
                "https://media.example.test/retry-v2",
                AddonMediaListDetailPolicy::ManualOnly,
            )
            .await;
            let job_id: EntityId = sqlx::query_scalar(
                r#"SELECT "Id" FROM "AddonMediaListImportJob"
                   WHERE "AddonMediaListId" = ?"#,
            )
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .expect("load queued import");
            sqlx::query(
                r#"UPDATE "AddonMediaListImportJob"
                   SET "ImportCapability" = ?, "Mode" = ?, "State" = ?,
                       "Cursor" = 'page-2', "OverviewComplete" = 1,
                       "CancellationRequested" = 1, "Diagnostic" = 'promotion failed'
                   WHERE "Id" = ?"#,
            )
            .bind(CAPABILITY_MEDIA_LIST_IMPORT_V2)
            .bind(encode_enum(AddonMediaListImportMode::Incremental))
            .bind(encode_enum(AddonMediaListImportState::Cancelled))
            .bind(job_id)
            .execute(&pool)
            .await
            .expect("checkpoint failed v2 import");

            coordinator
                .control_import(list_id, AddonMediaListImportAction::Retry)
                .await
                .expect("retry checkpointed v2 import");

            let rows = sqlx::query(
                r#"SELECT "Id", "Generation", "State", "ImportCapability", "Mode", "Cursor",
                          CAST(CASE WHEN "OverviewComplete" = 1 THEN 1 ELSE 0 END AS SIGNED)
                              AS "OverviewComplete",
                          CAST(CASE WHEN "CancellationRequested" = 1 THEN 1 ELSE 0 END AS SIGNED)
                              AS "CancellationRequested",
                          "Diagnostic"
                   FROM "AddonMediaListImportJob" WHERE "AddonMediaListId" = ?"#,
            )
            .bind(list_id)
            .fetch_all(&pool)
            .await
            .expect("load retried v2 import");
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].try_get::<EntityId, _>("Id").unwrap(), job_id);
            assert_eq!(rows[0].try_get::<i32, _>("Generation").unwrap(), 1);
            assert_eq!(
                rows[0].try_get::<i32, _>("State").unwrap(),
                encode_enum(AddonMediaListImportState::Queued)
            );
            assert_eq!(
                rows[0].try_get::<String, _>("ImportCapability").unwrap(),
                CAPABILITY_MEDIA_LIST_IMPORT_V2
            );
            assert_eq!(
                rows[0].try_get::<i32, _>("Mode").unwrap(),
                encode_enum(AddonMediaListImportMode::Incremental)
            );
            assert_eq!(
                rows[0]
                    .try_get::<Option<String>, _>("Cursor")
                    .unwrap()
                    .as_deref(),
                Some("page-2")
            );
            assert!(decode_bool_column(&rows[0], "OverviewComplete").unwrap());
            assert!(!decode_bool_column(&rows[0], "CancellationRequested").unwrap());
            assert!(rows[0]
                .try_get::<Option<String>, _>("Diagnostic")
                .unwrap()
                .is_none());
        }

        #[tokio::test]
        async fn v1_retry_resumes_a_partially_finalized_generation() {
            let (_temporary, pool, coordinator) = fixture().await;
            let list_id = queue(
                &coordinator,
                "https://media.example.test/retry-v1-finalization",
                AddonMediaListDetailPolicy::ManualOnly,
            )
            .await;
            let job_id: EntityId = sqlx::query_scalar(
                r#"SELECT "Id" FROM "AddonMediaListImportJob"
                   WHERE "AddonMediaListId" = ?"#,
            )
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .expect("load queued import");
            sqlx::query(
                r#"UPDATE "AddonMediaListImportJob"
                   SET "State" = ?, "OverviewComplete" = 1, "FinalizationPhase" = 2,
                       "Diagnostic" = 'projection failed'
                   WHERE "Id" = ?"#,
            )
            .bind(encode_enum(AddonMediaListImportState::NeedsAttention))
            .bind(job_id)
            .execute(&pool)
            .await
            .expect("checkpoint failed v1 finalization");

            coordinator
                .control_import(list_id, AddonMediaListImportAction::Retry)
                .await
                .expect("retry partial v1 finalization");

            let rows = sqlx::query(
                r#"SELECT "Id", "Generation", "State", "FinalizationPhase"
                   FROM "AddonMediaListImportJob" WHERE "AddonMediaListId" = ?"#,
            )
            .bind(list_id)
            .fetch_all(&pool)
            .await
            .expect("load retried v1 import");
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].try_get::<EntityId, _>("Id").unwrap(), job_id);
            assert_eq!(rows[0].try_get::<i32, _>("Generation").unwrap(), 1);
            assert_eq!(
                rows[0].try_get::<i32, _>("State").unwrap(),
                encode_enum(AddonMediaListImportState::Queued)
            );
            assert_eq!(rows[0].try_get::<i32, _>("FinalizationPhase").unwrap(), 2);
        }

        #[tokio::test]
        async fn finalization_lease_takeover_fences_stale_writes_and_list_mutations() {
            let (_temporary, pool, coordinator) = fixture().await;
            let source_url = "https://media.example.test/finalization-fence";
            let list_id = queue(
                &coordinator,
                source_url,
                AddonMediaListDetailPolicy::ManualOnly,
            )
            .await;
            let stale = coordinator.claim_import_job().await.unwrap().unwrap();
            let successor_owner = "successor-finalizer";
            sqlx::query(
                r#"UPDATE "AddonMediaListImportJob"
                   SET "OverviewComplete" = 1, "FinalizationPhase" = 0,
                       "LeaseOwner" = ?, "LeaseExpiresAt" = ?
                   WHERE "Id" = ?"#,
            )
            .bind(successor_owner)
            .bind(encode_datetime(Utc::now() + Duration::minutes(5)))
            .bind(stale.id)
            .execute(&pool)
            .await
            .expect("transfer finalization lease");

            let header = serde_json::to_string(&list_record("fixture/finalization-fence"))
                .expect("serialize finalization header");
            let item = serde_json::to_string(&item_record("episode:1", 0, "Overview"))
                .expect("serialize finalization item");
            let mut enumerated = decode_media_list_output_for_capability(
                format!("{header}\n{item}\n").as_bytes(),
                CAPABILITY_MEDIA_LIST_LIST_V5,
            )
            .expect("decode finalization snapshot");
            let list = AddonMediaList {
                id: list_id,
                addon_id: "org.example.fixture".to_owned(),
                source_url: source_url.to_owned(),
                provider_id: enumerated.provider_id.clone(),
                name: enumerated.name.clone(),
                description: enumerated.description.clone(),
                item_count: 1,
                refresh_cadence: ersatzrs_domain::AddonMediaListRefreshCadence::Never,
                generate_playlist: false,
                content_kind_override: AddonMediaListContentKind::TelevisionEpisode,
                presentation_kind: AddonMediaListPresentationKind::RemoteStreams,
                requested_presentation_kind: AddonMediaListPresentationKind::RemoteStreams,
                presentation_error: None,
                chapter_input: String::new(),
                playlist_id: None,
                last_update: None,
                last_match: None,
                last_error: None,
            };
            let stale_write =
                AddonMediaListRemoteSynchronizer::new(pool.clone(), coordinator.provider.clone())
                    .persist_import_generation(
                        stale.id,
                        &stale.lease_owner,
                        &list,
                        &mut enumerated,
                        false,
                    )
                    .await;
            assert!(matches!(stale_write, Err(RepositoryError::Conflict(_))));
            let item_count: i64 = sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ?"#,
            )
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .expect("count stale items");
            assert_eq!(item_count, 0);

            let cancel = coordinator
                .control_import(list_id, AddonMediaListImportAction::Cancel)
                .await;
            assert!(matches!(cancel, Err(RepositoryError::Conflict(_))));
            let requeue = coordinator
                .queue_import(
                    "org.example.fixture",
                    &context(source_url),
                    None,
                    AddonMediaListContentKind::TelevisionEpisode,
                    AddonMediaListDetailPolicy::ManualOnly,
                    Some(list_id),
                )
                .await;
            assert!(matches!(requeue, Err(RepositoryError::Conflict(_))));
            let delete = AddonMediaListSynchronizer::new(pool.clone())
                .delete_list(list_id)
                .await;
            assert!(matches!(delete, Err(RepositoryError::Conflict(_))));

            let current_owner: Option<String> = sqlx::query_scalar(
                r#"SELECT "LeaseOwner" FROM "AddonMediaListImportJob" WHERE "Id" = ?"#,
            )
            .bind(stale.id)
            .fetch_one(&pool)
            .await
            .expect("load current finalization owner");
            assert_eq!(current_owner.as_deref(), Some(successor_owner));
        }

        #[tokio::test]
        async fn stale_import_failures_cannot_overwrite_control_or_a_successor_lease() {
            let (_temporary, pool, coordinator) = fixture().await;
            let list_id = queue(
                &coordinator,
                "https://media.example.test/stale-worker",
                AddonMediaListDetailPolicy::ManualOnly,
            )
            .await;
            let stale = coordinator.claim_import_job().await.unwrap().unwrap();
            coordinator
                .control_import(list_id, AddonMediaListImportAction::Pause)
                .await
                .expect("pause claimed import");
            coordinator
                .fail_import(&stale, "stale provider failure", true)
                .await
                .expect("discard stale failure after pause");
            let paused: (i32, Option<String>) = sqlx::query_as(
                r#"SELECT "State", "Diagnostic" FROM "AddonMediaListImportJob"
                   WHERE "Id" = ?"#,
            )
            .bind(stale.id)
            .fetch_one(&pool)
            .await
            .expect("load paused import");
            assert_eq!(paused.0, encode_enum(AddonMediaListImportState::Paused));
            assert!(paused.1.is_none());

            coordinator
                .control_import(list_id, AddonMediaListImportAction::Resume)
                .await
                .expect("resume import");
            let expired = coordinator.claim_import_job().await.unwrap().unwrap();
            sqlx::query(
                r#"UPDATE "AddonMediaListImportJob" SET "LeaseExpiresAt" = '2000-01-01T00:00:00Z'
                   WHERE "Id" = ?"#,
            )
            .bind(expired.id)
            .execute(&pool)
            .await
            .expect("expire first claimant");
            let successor = coordinator.claim_import_job().await.unwrap().unwrap();
            assert_ne!(expired.lease_owner, successor.lease_owner);
            coordinator
                .fail_import(&expired, "late provider failure", false)
                .await
                .expect("discard stale failure after reclaim");
            let current: (i32, Option<String>, Option<String>) = sqlx::query_as(
                r#"SELECT "State", "LeaseOwner", "Diagnostic"
                   FROM "AddonMediaListImportJob" WHERE "Id" = ?"#,
            )
            .bind(expired.id)
            .fetch_one(&pool)
            .await
            .expect("load successor claim");
            assert_eq!(
                current.0,
                encode_enum(AddonMediaListImportState::Discovering)
            );
            assert_eq!(current.1.as_deref(), Some(successor.lease_owner.as_str()));
            assert!(current.2.is_none());
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
                .fail_import(&third, &error.to_string(), false)
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
        async fn automatic_package_update_reuses_detail_jobs_and_never_reopens_current_package_failures(
        ) {
            let (_temporary, pool, coordinator) = fixture().await;
            let list_id = complete_single_page(
                &coordinator,
                "https://media.example.test/package",
                AddonMediaListDetailPolicy::Automatic,
                "fixture/package",
                &["episode:1"],
            )
            .await;
            let addon: String =
                sqlx::query_scalar(r#"SELECT "AddonId" FROM "AddonMediaList" WHERE "Id"=?"#)
                    .bind(list_id)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            sqlx::query(r#"INSERT INTO "AddonInstallation" ("AddonId","Version","Enabled","PackageSha256","ContentSha256",
                "ManifestJson","InstallPath","InstalledAt","UpdatedAt") VALUES (?, '2.0.0',1,'new','new','{}',
                'addons/fixture','2026-09-16T00:00:00Z','2026-09-16T00:00:00Z') ON CONFLICT("AddonId") DO UPDATE SET "ContentSha256"='new'"#)
                .bind(addon).execute(&pool).await.unwrap();
            sqlx::query(r#"UPDATE "AddonMediaList" SET "RefreshPublicationPolicy"=2,"DetailImportAuthorizedAt"='2026-09-16T00:00:00Z' WHERE "Id"=?"#)
                .bind(list_id).execute(&pool).await.unwrap();
            sqlx::query(r#"UPDATE "AddonMediaListDetailJob" SET "State"=5,"OptionContextJson"='{"package_content":"old"}',
                "ResultRecordJson"='{}' WHERE "AddonMediaListId"=?"#)
                .bind(list_id).execute(&pool).await.unwrap();
            coordinator
                .requeue_one_changed_package_detail()
                .await
                .unwrap();
            let state: (i32, Option<String>, i32) = sqlx::query_as(
                r#"SELECT "State","ResultRecordJson","AttemptCount"
                FROM "AddonMediaListDetailJob" WHERE "AddonMediaListId"=?"#,
            )
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(state, (1, None, 0));
            sqlx::query(r#"UPDATE "AddonMediaListDetailJob" SET "State"=7,"AttemptCount"=4,"OptionContextJson"='{"package_content":"new"}' WHERE "AddonMediaListId"=?"#)
                .bind(list_id).execute(&pool).await.unwrap();
            coordinator
                .requeue_one_changed_package_detail()
                .await
                .unwrap();
            let state:(i32,i32)=sqlx::query_as(r#"SELECT "State","AttemptCount" FROM "AddonMediaListDetailJob" WHERE "AddonMediaListId"=?"#)
                .bind(list_id).fetch_one(&pool).await.unwrap();
            assert_eq!(state, (7, 4));
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
