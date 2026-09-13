use crate::addon_media_storage::{
    ensure_list_library, has_media_source, list_library_id, validated_folder_name,
};
use crate::db::inserted_id;
use crate::repositories::SqlxResultExt as _;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use chrono::{DateTime, Duration, Utc};
use ersatzrs_core::addon_media_list::{
    AddonMediaListProvider, EnumeratedAddonMediaList, EnumeratedAddonMediaListItem,
};
use ersatzrs_core::repositories::{
    AddonMediaListMetadataConflict, AddonMediaListRevisionItem, AddonMediaListRevisionSnapshot,
    RepositoryError, Result,
};
use ersatzrs_domain::{
    AddonMediaList, AddonMediaListItemAvailability, AddonMediaListItemKind, EntityId,
    ManagedLibraryOperationState, RemoteStreamLiveness, ScanPlayoutRefresh,
    ScanPlayoutRefreshReason,
};
use sqlx::{Pool, Row, Transaction};

use crate::db::{Db, DbPool};
use crate::repositories::common::{
    decode_bool_column, decode_enum_column, decode_optional_datetime, decode_optional_enum_column,
    encode_datetime, encode_enum,
};

const PLAYBACK_ORDER_CHRONOLOGICAL: i32 = 1;
const COLLECTION_TYPE_SHOW: i32 = 1;
const COLLECTION_TYPE_SEASON: i32 = 2;
const COLLECTION_TYPE_MOVIE: i32 = 10;
const COLLECTION_TYPE_EPISODE: i32 = 20;
const COLLECTION_TYPE_REMOTE_STREAM: i32 = 70;
const OMITTED_BY_PROVIDER_REASON: &str = "omitted_by_provider";

/// Result of matching one or more add-on media lists.
///
/// Refreshes are coalesced by playout id and sorted so a host can enqueue the
/// exact committed work immediately without repeating dependency resolution.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AddonMediaListMatchReport {
    pub matched_list_count: usize,
    pub playout_refreshes: Vec<ScanPlayoutRefresh>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DetailMergeOutcome {
    Applied,
    DeferredForPublication,
}

#[derive(Clone, Copy)]
struct ImportFinalizationFence<'a> {
    job_id: EntityId,
    lease_owner: &'a str,
    expected_phase: i32,
    next_phase: i32,
}

async fn begin_import_finalization_phase(
    tx: &mut Transaction<'_, Db>,
    fence: ImportFinalizationFence<'_>,
) -> Result<()> {
    let lease_expires = encode_datetime(
        Utc::now() + Duration::seconds(crate::addon_media_list_import::IMPORT_LEASE_SECONDS),
    );
    let updated = sqlx::query(
        r#"UPDATE "AddonMediaListImportJob" SET "LeaseExpiresAt" = ?, "HeartbeatAt" = ?
           WHERE "Id" = ? AND "LeaseOwner" = ? AND "OverviewComplete" = 1
             AND "CancellationRequested" = 0 AND "FinalizationPhase" = ?"#,
    )
    .bind(&lease_expires)
    .bind(encode_datetime(Utc::now()))
    .bind(fence.job_id)
    .bind(fence.lease_owner)
    .bind(fence.expected_phase)
    .execute(&mut **tx)
    .await
    .repository()?;
    if updated.rows_affected() != 1 {
        return Err(RepositoryError::Conflict(
            "the import finalization lease or phase changed".to_owned(),
        ));
    }
    Ok(())
}

async fn complete_import_finalization_phase(
    tx: &mut Transaction<'_, Db>,
    fence: ImportFinalizationFence<'_>,
) -> Result<()> {
    let updated = sqlx::query(
        r#"UPDATE "AddonMediaListImportJob" SET "FinalizationPhase" = ?, "UpdatedAt" = ?
           WHERE "Id" = ? AND "LeaseOwner" = ? AND "OverviewComplete" = 1
             AND "CancellationRequested" = 0 AND "FinalizationPhase" = ?"#,
    )
    .bind(fence.next_phase)
    .bind(encode_datetime(Utc::now()))
    .bind(fence.job_id)
    .bind(fence.lease_owner)
    .bind(fence.expected_phase)
    .execute(&mut **tx)
    .await
    .repository()?;
    if updated.rows_affected() != 1 {
        return Err(RepositoryError::Conflict(
            "the import finalization lease or phase changed".to_owned(),
        ));
    }
    Ok(())
}

async fn update_incremental_import_list_header(
    tx: &mut Transaction<'_, Db>,
    list: &AddonMediaList,
    metadata: &ersatzrs_core::repositories::AddonMediaListNfoMetadata,
    bump_revision: bool,
) -> Result<()> {
    let current = sqlx::query(
        r#"SELECT "Name", "Description", "ProviderMetadataJson",
                  "MetadataOverrideJson", "MetadataConflictJson"
           FROM "AddonMediaList" WHERE "Id" = ? AND "AddonId" = ?"#,
    )
    .bind(list.id)
    .bind(&list.addon_id)
    .fetch_optional(&mut **tx)
    .await
    .repository()?
    .ok_or_else(|| {
        RepositoryError::Conflict(
            "the imported list identity changed before finalization".to_owned(),
        )
    })?;
    let previous_provider: String = current.try_get("ProviderMetadataJson").repository()?;
    let provider_metadata = merge_present_metadata(&previous_provider, metadata)?;
    let overrides: String = current.try_get("MetadataOverrideJson").repository()?;
    let existing_conflicts: Option<String> =
        current.try_get("MetadataConflictJson").repository()?;
    let conflicts = refresh_conflict_json(
        &previous_provider,
        &provider_metadata,
        &overrides,
        existing_conflicts.as_deref(),
    )?;
    let current_name: String = current.try_get("Name").repository()?;
    let current_description: Option<String> = current.try_get("Description").repository()?;
    let next_description = list
        .description
        .clone()
        .or_else(|| current_description.clone());
    let changed = current_name != list.name
        || current_description != next_description
        || previous_provider != provider_metadata
        || existing_conflicts != conflicts;
    if !changed {
        return Ok(());
    }
    let updated = sqlx::query(
        r#"UPDATE "AddonMediaList"
           SET "Name" = ?, "Description" = ?, "ProviderMetadataJson" = ?,
               "MetadataConflictJson" = ?, "LastError" = NULL,
               "ReviewState" = CASE WHEN "ReviewState" = 0 THEN 0 ELSE 1 END,
               "ReviewError" = NULL, "DraftRevision" = "DraftRevision" + ?
           WHERE "Id" = ? AND "AddonId" = ? AND "ReviewState" <> 3
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
    .bind(&list.name)
    .bind(next_description)
    .bind(provider_metadata)
    .bind(conflicts)
    .bind(i32::from(bump_revision))
    .bind(list.id)
    .bind(&list.addon_id)
    .bind(encode_enum(
        ersatzrs_domain::ManagedLibraryOperationState::Queued,
    ))
    .bind(encode_enum(
        ersatzrs_domain::ManagedLibraryOperationState::Running,
    ))
    .bind(encode_enum(
        ersatzrs_domain::ManagedLibraryOperationState::WaitingToRetry,
    ))
    .execute(&mut **tx)
    .await
    .repository()?;
    if updated.rows_affected() != 1 {
        return Err(publication_in_progress());
    }
    Ok(())
}

impl AddonMediaListMatchReport {
    fn one(playout_refreshes: Vec<ScanPlayoutRefresh>) -> Self {
        Self {
            matched_list_count: 1,
            playout_refreshes,
        }
    }

    fn merge(&mut self, other: Self) {
        self.matched_list_count = self
            .matched_list_count
            .saturating_add(other.matched_list_count);
        for refresh in other.playout_refreshes {
            match self
                .playout_refreshes
                .binary_search_by_key(&refresh.playout_id, |candidate| candidate.playout_id)
            {
                Ok(index) => {
                    self.playout_refreshes[index].reason =
                        self.playout_refreshes[index].reason.merge(refresh.reason);
                }
                Err(index) => self.playout_refreshes.insert(index, refresh),
            }
        }
    }
}

/// Maintains locally persisted Add-on media lists and generated playlists.
pub struct AddonMediaListSynchronizer {
    pool: DbPool,
}

impl AddonMediaListSynchronizer {
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    async fn has_active_publication(&self, list_id: EntityId) -> Result<bool> {
        sqlx::query_scalar::<_, bool>(
            r#"SELECT EXISTS (
                   SELECT 1 FROM "AddonMediaList"
                   WHERE "Id" = ? AND "PublicationJournalJson" IS NOT NULL
                     AND TRIM("PublicationJournalJson") <> ''
               )"#,
        )
        .bind(list_id)
        .fetch_one(&self.pool)
        .await
        .repository()
    }

    /// Match a saved Add-on media list against local metadata and update generated
    /// playlist rows when enabled.
    ///
    /// # Errors
    ///
    /// Returns an error when the list cannot be loaded or database writes fail.
    pub async fn match_list_items(&self, list_id: EntityId) -> Result<()> {
        self.match_list_items_with_report(list_id).await.map(|_| ())
    }

    /// Match as part of the fenced publication coordinator.
    ///
    /// Ordinary matchers freeze while a publication journal is active. Only
    /// the coordinator may bypass that freeze, and its caller revalidates the
    /// lease immediately after this method returns.
    pub(crate) async fn match_list_items_for_publication(
        &self,
        list_id: EntityId,
        source: Option<&AddonMediaListRevisionSnapshot>,
    ) -> Result<()> {
        self.match_list_items_with_report_inner(list_id, true, source)
            .await
            .map(|_| ())
    }

    /// Bind scanned sources before publication builds its typed media projection.
    /// The coordinator checks its lease before and after this operation. Playlist
    /// selection and refresh intents must wait until projection has completed.
    pub(crate) async fn bind_list_items_for_publication(
        &self,
        list_id: EntityId,
        source: Option<&AddonMediaListRevisionSnapshot>,
    ) -> Result<()> {
        let list = self.load_list(list_id).await?;
        self.match_persisted_items(&list, false, true, source)
            .await?;
        Ok(())
    }

    /// Rebuild a restored system playlist even when the current draft has
    /// disabled playlist generation.
    ///
    /// Recovery restores the previously published playlist, not the draft
    /// setting that caused the failed candidate. Reuse the canonical matcher
    /// so regenerated projection and replacement identities are selected with
    /// the same rules as an ordinary publication.
    pub(crate) async fn rebuild_restored_playlist_items(
        &self,
        list_id: EntityId,
        playlist_id: EntityId,
        source: Option<&AddonMediaListRevisionSnapshot>,
    ) -> Result<()> {
        let list = self.load_list(list_id).await?;
        let desired = self
            .match_persisted_items(&list, true, true, source)
            .await?;
        let persisted = load_generated_playlist_items(&self.pool, playlist_id).await?;
        if persisted == desired {
            return self.record_last_match(list_id, true).await;
        }
        let reason = generated_playlist_change_reason(&persisted, &desired);
        let refresh = self
            .prepare_generated_playlist_refresh(list_id, Some(playlist_id), reason)
            .await?;
        match self
            .commit_generated_playlist_match(list_id, playlist_id, &desired, &refresh, true)
            .await
        {
            Ok(_) => Ok(()),
            Err(error) => {
                self.fail_generated_playlist_refresh(&refresh, &error).await;
                Err(error)
            }
        }
    }

    /// Match one saved add-on media list and report the exact Classic playouts
    /// whose generated-playlist dependency changed.
    ///
    /// # Errors
    ///
    /// Returns an error when the list cannot be loaded, impact resolution
    /// fails, or the atomic playlist-and-intent commit cannot complete.
    pub async fn match_list_items_with_report(
        &self,
        list_id: EntityId,
    ) -> Result<AddonMediaListMatchReport> {
        self.match_list_items_with_report_inner(list_id, false, None)
            .await
    }

    async fn match_list_items_with_report_inner(
        &self,
        list_id: EntityId,
        publication_owned: bool,
        source: Option<&AddonMediaListRevisionSnapshot>,
    ) -> Result<AddonMediaListMatchReport> {
        if !publication_owned && self.has_active_publication(list_id).await? {
            return Ok(AddonMediaListMatchReport::default());
        }
        let mut list = self.load_list(list_id).await?;
        if let Some(source) = source {
            if !publication_owned || source.list_id != list_id {
                return Err(publication_in_progress());
            }
            list.name.clone_from(&source.name);
            list.generate_playlist = source.generate_playlist;
        }
        let desired_playlist_items = self
            .match_persisted_items(&list, list.generate_playlist, publication_owned, source)
            .await?;

        if !list.generate_playlist {
            self.record_last_match(list_id, publication_owned).await?;
            return Ok(AddonMediaListMatchReport::one(Vec::new()));
        }

        let Some(playlist_id) = list.playlist_id else {
            // A not-yet-created playlist cannot be referenced by a valid
            // schedule foreign key. Create its identity, link, complete item
            // projection, and terminal resolution state in one transaction so
            // a restart can never expose an empty generated playlist shell.
            let group_id = system_playlist_group_id(&self.pool).await?;
            let reason = ScanPlayoutRefreshReason::RemoteStreamMembershipAdded;
            let refresh = self
                .prepare_generated_playlist_refresh(list_id, None, reason)
                .await?;
            let result = self
                .commit_generated_playlist_creation(
                    &list,
                    group_id,
                    &desired_playlist_items,
                    &refresh,
                    publication_owned,
                    source.is_some(),
                )
                .await;
            if let Err(error) = result {
                self.fail_generated_playlist_refresh(&refresh, &error).await;
                return Err(error);
            }
            return Ok(AddonMediaListMatchReport::one(refresh.playout_refreshes()));
        };
        let persisted_playlist_items =
            load_generated_playlist_items(&self.pool, playlist_id).await?;
        if persisted_playlist_items == desired_playlist_items {
            self.record_last_match(list_id, publication_owned).await?;
            return Ok(AddonMediaListMatchReport::one(Vec::new()));
        }

        let reason =
            generated_playlist_change_reason(&persisted_playlist_items, &desired_playlist_items);
        let refresh = self
            .prepare_generated_playlist_refresh(list_id, Some(playlist_id), reason)
            .await?;

        let changed = self
            .commit_generated_playlist_match(
                list_id,
                playlist_id,
                &desired_playlist_items,
                &refresh,
                publication_owned,
            )
            .await;
        let changed = match changed {
            Ok(changed) => changed,
            Err(error) => {
                self.fail_generated_playlist_refresh(&refresh, &error).await;
                return Err(error);
            }
        };

        let playout_refreshes = if changed {
            refresh.playout_refreshes()
        } else {
            Vec::new()
        };

        Ok(AddonMediaListMatchReport::one(playout_refreshes))
    }

    async fn match_snapshot_items(
        &self,
        list: &AddonMediaList,
        source: &AddonMediaListRevisionSnapshot,
        generate_playlist_items: bool,
    ) -> Result<Vec<GeneratedPlaylistItem>> {
        let mut desired = Vec::new();
        for retained in source.items.iter().filter(|item| item.published) {
            let id: EntityId = sqlx::query_scalar(
                r#"SELECT "Id" FROM "AddonMediaListItem" WHERE "AddonMediaListId"=? AND "ProviderId"=?"#,
            ).bind(list.id).bind(&retained.provider_id).fetch_optional(&self.pool).await.repository()?
                .ok_or_else(publication_in_progress)?;
            let item = MatchItem {
                id,
                addon_id: list.addon_id.clone(),
                provider_id: retained.provider_id.clone(),
                kind: retained.kind,
                title: retained.title.clone(),
                year: retained.year,
                season: retained.season,
                episode: retained.episode,
                rank: retained.rank,
                source_url: retained.source_url.clone(),
                availability: retained.availability,
                duration_seconds: retained.duration_seconds,
                liveness: retained.effective_liveness,
                guids: retained.guids.clone(),
            };
            let matched_id = self.identify_item(&item).await?;
            let updated = sqlx::query(
                r#"UPDATE "AddonMediaListItem" SET "MediaItemId"=? WHERE "Id"=? AND "AddonMediaListId"=?"#,
            ).bind(matched_id).bind(id).bind(list.id).execute(&self.pool).await.repository()?;
            if updated.rows_affected() != 1 {
                return Err(publication_in_progress());
            }
            if generate_playlist_items {
                if let Some(selection) = self
                    .playlist_selection(&item, matched_id, Some(retained))
                    .await?
                {
                    desired.push(GeneratedPlaylistItem::new(&item, selection));
                }
            }
        }
        Ok(desired)
    }

    async fn match_persisted_items(
        &self,
        list: &AddonMediaList,
        generate_playlist_items: bool,
        publication_owned: bool,
        source: Option<&AddonMediaListRevisionSnapshot>,
    ) -> Result<Vec<GeneratedPlaylistItem>> {
        if let Some(source) = source {
            if !publication_owned || source.list_id != list.id {
                return Err(publication_in_progress());
            }
            return self
                .match_snapshot_items(list, source, generate_playlist_items)
                .await;
        }
        let rows = sqlx::query(
            r#"SELECT "Id", "ProviderId", "Kind", "Title", "Year", "Season", "Episode", "Rank", "SourceUrl",
                      "Availability", "PublishedAvailability", "Published", "DurationSeconds",
                      "PublishedLiveness"
               FROM "AddonMediaListItem"
               WHERE "AddonMediaListId" = ? AND ("InDraft" = 1 OR "Published" = 1)
               ORDER BY "Rank", "Id""#,
        )
        .bind(list.id)
        .fetch_all(&self.pool)
        .await
        .repository()?;
        let mut desired = Vec::new();
        for row in rows {
            let published = decode_bool_column(&row, "Published")?;
            let item = MatchItem {
                id: row.try_get("Id").repository()?,
                addon_id: list.addon_id.clone(),
                provider_id: row.try_get("ProviderId").repository()?,
                kind: decode_enum_column("AddonMediaListItemKind", &row, "Kind")?,
                title: row
                    .try_get::<Option<String>, _>("Title")
                    .repository()?
                    .unwrap_or_default(),
                year: row.try_get("Year").repository()?,
                season: row.try_get("Season").repository()?,
                episode: row.try_get("Episode").repository()?,
                rank: row.try_get("Rank").repository()?,
                source_url: row.try_get("SourceUrl").repository()?,
                availability: decode_enum_column(
                    "AddonMediaListItemAvailability",
                    &row,
                    if published {
                        "PublishedAvailability"
                    } else {
                        "Availability"
                    },
                )?,
                duration_seconds: decode_optional_seconds(&row, "DurationSeconds")?,
                liveness: decode_enum_column("RemoteStreamLiveness", &row, "PublishedLiveness")?,
                guids: self
                    .load_item_guids(row.try_get("Id").repository()?)
                    .await?,
            };
            let matched_id = self.identify_item(&item).await?;
            let updated = if publication_owned {
                sqlx::query(r#"UPDATE "AddonMediaListItem" SET "MediaItemId" = ? WHERE "Id" = ?"#)
                    .bind(matched_id)
                    .bind(item.id)
                    .execute(&self.pool)
                    .await
                    .repository()?
            } else {
                sqlx::query(
                    r#"UPDATE "AddonMediaListItem" SET "MediaItemId" = ?
                       WHERE "Id" = ? AND EXISTS (
                           SELECT 1 FROM "AddonMediaList" AS "list"
                           WHERE "list"."Id" = "AddonMediaListItem"."AddonMediaListId"
                             AND ("list"."PublicationJournalJson" IS NULL
                                  OR TRIM("list"."PublicationJournalJson") = '')
                       )"#,
                )
                .bind(matched_id)
                .bind(item.id)
                .execute(&self.pool)
                .await
                .repository()?
            };
            if updated.rows_affected() != 1 {
                return Err(publication_in_progress());
            }
            if generate_playlist_items && published {
                if let Some(selection) = self.playlist_selection(&item, matched_id, None).await? {
                    desired.push(GeneratedPlaylistItem::new(&item, selection));
                }
            }
        }
        Ok(desired)
    }

    /// Update user-editable Add-on media list settings.
    ///
    /// # Errors
    ///
    /// Returns an error when the list does not exist or persistence fails.
    #[allow(clippy::too_many_arguments)] // Mirrors the persisted operator settings edited as one unit.
    pub async fn update_settings(
        &self,
        list_id: EntityId,
        refresh_cadence: ersatzrs_domain::AddonMediaListRefreshCadence,
        refresh_publication_policy: ersatzrs_domain::AddonMediaListRefreshPublicationPolicy,
        delivery_mode: ersatzrs_domain::AddonMediaListDeliveryMode,
        next_refresh_at: Option<&str>,
        cadence_changed: bool,
        generate_playlist: bool,
        content_kind_override: ersatzrs_domain::AddonMediaListContentKind,
        chapter_input: &str,
    ) -> Result<()> {
        let _list = self.load_list(list_id).await?;
        ensure_not_publishing(&self.pool, list_id).await?;
        let now = crate::repositories::common::encode_datetime(chrono::Utc::now());
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        let updated = sqlx::query(
            r#"UPDATE "AddonMediaList"
               SET "AutoRefresh" = ?, "RefreshCadence" = ?, "RefreshPublicationPolicy" = ?,
                   "NextRefreshAt" = ?,
                   "RefreshRetryAt" = CASE WHEN ? THEN NULL ELSE "RefreshRetryAt" END,
                   "GeneratePlaylist" = ?, "ContentKindOverride" = ?,
                   "ChapterInput" = ?,
                   "ReviewState" = CASE WHEN "ReviewState" = 0 THEN 0 ELSE 1 END,
                   "ReviewError" = NULL, "DraftRevision" = "DraftRevision" + 1
               WHERE "Id" = ?
                 AND NOT EXISTS (
                     SELECT 1
                     FROM "AddonMediaListPublicationJob" AS "job"
                     WHERE "job"."AddonMediaListId" = "AddonMediaList"."Id"
                       AND "job"."Outcome" IS NOT NULL
                       AND "job"."Id" = (
                           SELECT MAX("latest"."Id") FROM "AddonMediaListPublicationJob" AS "latest"
                           WHERE "latest"."AddonMediaListId" = "AddonMediaList"."Id"
                       )
                       AND "AddonMediaList"."PublicationJournalJson" IS NOT NULL
                 )
                 AND NOT EXISTS (
                     SELECT 1
                     FROM "AddonMediaListPublicationJob" AS "job"
                     JOIN "ManagedLibraryOperation" AS "operation"
                       ON "operation"."Id" = "job"."OperationId"
                     JOIN "ManagedLibraryLease" AS "lease"
                       ON "lease"."OperationId" = "operation"."Id"
                      AND "lease"."OwnerToken" = "operation"."OwnerToken"
                     WHERE "job"."AddonMediaListId" = "AddonMediaList"."Id"
                       AND "operation"."State" = ?
                       AND "lease"."LeaseExpiresAt" > ?
                 )"#,
        )
        .bind(refresh_cadence == ersatzrs_domain::AddonMediaListRefreshCadence::Daily)
        .bind(encode_enum(refresh_cadence))
        .bind(encode_enum(refresh_publication_policy))
        .bind(next_refresh_at)
        .bind(cadence_changed)
        .bind(generate_playlist)
        .bind(encode_enum(content_kind_override))
        .bind((!chapter_input.is_empty()).then_some(chapter_input))
        .bind(list_id)
        .bind(encode_enum(
            ersatzrs_domain::ManagedLibraryOperationState::Running,
        ))
        .bind(&now)
        .execute(&mut *tx)
        .await
        .repository()?;
        if updated.rows_affected() != 1 {
            return Err(publication_in_progress());
        }

        crate::addon_media_list_delivery::save_in_tx(
            &mut tx,
            list_id,
            delivery_mode,
            chrono::Utc::now(),
        )
        .await?;

        if cadence_changed {
            sqlx::query(
                r#"UPDATE "AddonMediaListRefreshState"
                   SET "State" = ?, "MissCount" = 0, "AttemptCount" = 0,
                       "LeaseOwner" = NULL, "LeaseExpiresAt" = NULL,
                       "NextEligibleAt" = ?, "DiagnosticCode" = NULL,
                       "UpdatedAt" = ?
                   WHERE "AddonMediaListId" = ?"#,
            )
            .bind(encode_enum(
                ersatzrs_domain::AddonMediaListRefreshState::Idle,
            ))
            .bind(next_refresh_at)
            .bind(&now)
            .bind(list_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        }
        tx.commit().await.repository()?;

        Ok(())
    }

    /// Merge one deferred detail result without letting playback-sensitive
    /// values silently alter an already published revision.
    ///
    /// Provider metadata remains separate from operator overrides. Safe
    /// descriptive changes are marked for a coalesced projection; identity,
    /// classification, availability, source, duration, or liveness changes
    /// return the list to review while the published playback snapshot stays
    /// intact.
    #[allow(clippy::too_many_lines)] // Ownership comparison and the guarded merge are one unit.
    #[cfg(test)]
    pub(crate) async fn merge_detail_item(
        &self,
        list_id: EntityId,
        item: &EnumeratedAddonMediaListItem,
    ) -> Result<DetailMergeOutcome> {
        self.merge_detail_item_inner(list_id, item, None).await
    }

    /// Merge only while the detail worker still owns the exact persisted lease.
    pub(crate) async fn merge_detail_item_with_lease(
        &self,
        list_id: EntityId,
        item_id: EntityId,
        lease_owner: &str,
        item: &EnumeratedAddonMediaListItem,
    ) -> Result<DetailMergeOutcome> {
        self.merge_detail_item_inner(list_id, item, Some((item_id, lease_owner)))
            .await
    }

    #[allow(clippy::too_many_lines)]
    async fn merge_detail_item_inner(
        &self,
        list_id: EntityId,
        item: &EnumeratedAddonMediaListItem,
        detail_fence: Option<(EntityId, &str)>,
    ) -> Result<DetailMergeOutcome> {
        let row = sqlx::query(
            r#"SELECT "item".*, "list"."PublishedRevision"
               FROM "AddonMediaListItem" AS "item"
               JOIN "AddonMediaList" AS "list" ON "list"."Id" = "item"."AddonMediaListId"
               WHERE "item"."AddonMediaListId" = ? AND "item"."ProviderId" = ?
                 AND "item"."InDraft" = 1
                 AND "item"."ParentProviderId" IS NULL"#,
        )
        .bind(list_id)
        .bind(&item.provider_id)
        .fetch_optional(&self.pool)
        .await
        .repository()?
        .ok_or_else(|| {
            RepositoryError::Conflict(
                "the enriched item no longer belongs to the committed overview".to_owned(),
            )
        })?;
        let item_id: EntityId = row.try_get("Id").repository()?;
        let published_revision: i32 = row.try_get("PublishedRevision").repository()?;
        let published = published_revision > 0 && decode_bool_column(&row, "Published")?;
        let previous_metadata: String = row.try_get("ProviderMetadataJson").repository()?;
        let override_metadata: String = row.try_get("MetadataOverrideJson").repository()?;
        let previous_conflicts: Option<String> =
            row.try_get("MetadataConflictJson").repository()?;
        let next_metadata = encode_metadata(&item.metadata)?;
        let conflicts = refresh_conflict_json(
            &previous_metadata,
            &next_metadata,
            &override_metadata,
            previous_conflicts.as_deref(),
        )?;
        let previous_provider_liveness =
            decode_enum_column("RemoteStreamLiveness", &row, "ProviderLiveness")?;
        let liveness_override =
            decode_optional_enum_column("RemoteStreamLiveness", &row, "LivenessOverride")?;
        let previous_liveness_conflict = decode_bool_column(&row, "LivenessConflict")?;
        let (effective_liveness, liveness_conflict) = refresh_liveness(
            previous_provider_liveness,
            item.liveness,
            liveness_override,
            previous_liveness_conflict,
        );
        let previous_guids = self.load_item_guids(item_id).await?;
        let operational_change = row.try_get::<i32, _>("Kind").repository()?
            != encode_enum(item.kind)
            || row.try_get::<Option<String>, _>("SourceUrl").repository()? != item.source_url
            || decode_optional_seconds(&row, "DurationSeconds")? != item.duration_seconds
            || row.try_get::<i32, _>("ContentKind").repository()? != encode_enum(item.content_kind)
            || row.try_get::<i32, _>("CollectionRole").repository()?
                != encode_enum(item.collection_role)
            || decode_enum_column::<AddonMediaListItemAvailability>(
                "AddonMediaListItemAvailability",
                &row,
                "Availability",
            )? != item.availability
            || decode_optional_enum_column::<ersatzrs_domain::AddonMediaListAvailabilityReason>(
                "AddonMediaListAvailabilityReason",
                &row,
                "AvailabilityReasonCode",
            )? != item.availability_reason_code
            || previous_provider_liveness != item.liveness
            || previous_guids != item.guids;
        let mut review_required = published && (operational_change || conflicts.is_some());
        let image_urls = encode_image_urls(&item.additional_image_urls)?;
        let now = encode_datetime(Utc::now());
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        let still_in_draft: bool = sqlx::query_scalar(
            r#"SELECT EXISTS (SELECT 1 FROM "AddonMediaListItem"
               WHERE "Id"=? AND "AddonMediaListId"=? AND "InDraft"=1)"#,
        )
        .bind(item_id)
        .bind(list_id)
        .fetch_one(&mut *tx)
        .await
        .repository()?;
        if !still_in_draft {
            return Err(RepositoryError::Conflict(
                "the enriched item no longer belongs to the committed overview".to_owned(),
            ));
        }
        let mut draft_detail = false;
        if let Some((expected_item_id, lease_owner)) = detail_fence {
            if expected_item_id != item_id {
                return Err(RepositoryError::Conflict(
                    "the detail item identity changed before merge".to_owned(),
                ));
            }
            let current: bool = sqlx::query_scalar(
                r#"SELECT EXISTS (
                       SELECT 1 FROM "AddonMediaListDetailJob"
                       WHERE "AddonMediaListItemId" = ? AND "AddonMediaListId" = ?
                         AND "State" = ? AND "LeaseOwner" = ?
                         AND "LeaseExpiresAt" IS NOT NULL AND "LeaseExpiresAt" > ?
                   )"#,
            )
            .bind(item_id)
            .bind(list_id)
            .bind(encode_enum(
                ersatzrs_domain::AddonMediaListDetailState::Running,
            ))
            .bind(lease_owner)
            .bind(&now)
            .fetch_one(&mut *tx)
            .await
            .repository()?;
            if !current {
                return Err(RepositoryError::Conflict(
                    "the detail lease changed before merge".to_owned(),
                ));
            }
            let (purpose, context): (i32, Option<String>) = sqlx::query_as(
                r#"SELECT "OptionPurpose", "OptionContextJson" FROM "AddonMediaListDetailJob"
                   WHERE "AddonMediaListItemId"=? AND "AddonMediaListId"=?"#,
            )
            .bind(item_id)
            .bind(list_id)
            .fetch_one(&mut *tx)
            .await
            .repository()?;
            crate::addon_media_list_detail_options::validate(
                &mut tx,
                list_id,
                item_id,
                purpose,
                context.as_deref(),
            )
            .await?;
            draft_detail = purpose == 1;
            review_required |= published && draft_detail;
        }
        let publication_checkpoint = sqlx::query_as::<_, (i32, Option<String>)>(
            r#"SELECT "ReviewState", "PublicationJournalJson"
               FROM "AddonMediaList"
               WHERE "Id" = ?"#,
        )
        .bind(list_id)
        .fetch_optional(&mut *tx)
        .await
        .repository()?
        .ok_or(RepositoryError::NotFound {
            kind: "add-on media list",
            id: list_id,
        })?;
        if publication_checkpoint.0
            == encode_enum(ersatzrs_domain::AddonMediaListReviewState::Publishing)
            || publication_checkpoint
                .1
                .as_deref()
                .is_some_and(|journal| !journal.trim().is_empty())
        {
            tx.rollback().await.repository()?;
            return Ok(DetailMergeOutcome::DeferredForPublication);
        }
        if published && !draft_detail {
            sqlx::query(
                r#"UPDATE "AddonMediaListItem"
                   SET "DisplayTitle" = ?, "Title" = ?, "Year" = ?, "Season" = ?,
                       "Episode" = ?, "AdditionalImageUrls" = ?,
                       "Availability" = ?, "AvailabilityReason" = ?,
                       "AvailabilityReasonCode" = ?, "AvailabilityUpdated" = ?,
                       "ProviderMetadataJson" = ?, "MetadataConflictJson" = ?,
                       "ProviderLiveness" = ?, "EffectiveLiveness" = ?,
                       "LivenessConflict" = ?
                   WHERE "Id" = ?"#,
            )
            .bind(&item.display_title)
            .bind(&item.title)
            .bind(item.year)
            .bind(item.season)
            .bind(item.episode)
            .bind(image_urls)
            .bind(encode_enum(item.availability))
            .bind(&item.availability_reason)
            .bind(item.availability_reason_code.map(encode_enum))
            .bind(&now)
            .bind(&next_metadata)
            .bind(&conflicts)
            .bind(encode_enum(item.liveness))
            .bind(encode_enum(effective_liveness))
            .bind(liveness_conflict)
            .bind(item_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        } else {
            sqlx::query(
                r#"UPDATE "AddonMediaListItem"
                   SET "Kind" = ?, "DisplayTitle" = ?, "Title" = ?, "Year" = ?,
                       "Season" = ?, "Episode" = ?, "SourceUrl" = ?,
                       "Availability" = ?, "AvailabilityReason" = ?,
                       "AvailabilityReasonCode" = ?,
                       "AvailabilityUpdated" = ?, "ContentKind" = ?, "CollectionRole" = ?,
                       "DurationSeconds" = ?, "AdditionalImageUrls" = ?,
                       "ProviderMetadataJson" = ?, "MetadataConflictJson" = ?,
                       "ProviderLiveness" = ?, "EffectiveLiveness" = ?,
                       "LivenessConflict" = ?
                   WHERE "Id" = ?"#,
            )
            .bind(encode_enum(item.kind))
            .bind(&item.display_title)
            .bind(&item.title)
            .bind(item.year)
            .bind(item.season)
            .bind(item.episode)
            .bind(&item.source_url)
            .bind(encode_enum(item.availability))
            .bind(&item.availability_reason)
            .bind(item.availability_reason_code.map(encode_enum))
            .bind(&now)
            .bind(encode_enum(item.content_kind))
            .bind(encode_enum(item.collection_role))
            .bind(optional_fragment_bound(item.duration_seconds, "duration")?)
            .bind(image_urls)
            .bind(&next_metadata)
            .bind(&conflicts)
            .bind(encode_enum(item.liveness))
            .bind(encode_enum(effective_liveness))
            .bind(liveness_conflict)
            .bind(item_id)
            .execute(&mut *tx)
            .await
            .repository()?;
            sqlx::query(r#"DELETE FROM "AddonMediaListItemGuid" WHERE "AddonMediaListItemId" = ?"#)
                .bind(item_id)
                .execute(&mut *tx)
                .await
                .repository()?;
            insert_item_guids(&mut tx, item_id, &item.guids).await?;
        }
        if review_required {
            let updated = sqlx::query(
                r#"UPDATE "AddonMediaList"
                   SET "ReviewState" = ?, "ReviewError" = ?,
                       "DraftRevision" = "DraftRevision" + 1,
                       "DetailProjectionPendingAt" = CASE WHEN ? THEN NULL ELSE "DetailProjectionPendingAt" END
                   WHERE "Id" = ? AND "ReviewState" <> ?"#,
            )
            .bind(encode_enum(
                ersatzrs_domain::AddonMediaListReviewState::Ready,
            ))
            .bind("Provider details changed review-gated item values")
            .bind(draft_detail)
            .bind(list_id)
            .bind(encode_enum(
                ersatzrs_domain::AddonMediaListReviewState::Publishing,
            ))
            .execute(&mut *tx)
            .await
            .repository()?;
            if updated.rows_affected() != 1 {
                return Err(RepositoryError::Conflict(
                    "the add-on media list changed while provider details were merged".to_owned(),
                ));
            }
        } else if published && !draft_detail {
            sqlx::query(
                r#"UPDATE "AddonMediaList"
                   SET "DetailProjectionPendingAt" = ?
                   WHERE "Id" = ?"#,
            )
            .bind(&now)
            .bind(list_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        }
        if detail_fence.is_some() {
            // The merge itself can update the source and effective liveness.
            // Record its resulting owner atomically, so later refresh
            // reuse distinguishes that legitimate change from a subsequent edit.
            let context = crate::addon_media_list_detail_options::capture(
                &mut tx,
                list_id,
                item_id,
                i32::from(draft_detail),
            )
            .await?;
            sqlx::query(r#"UPDATE "AddonMediaListDetailJob" SET "OptionContextJson"=? WHERE "AddonMediaListItemId"=?"#)
                .bind(context).bind(item_id).execute(&mut *tx).await.repository()?;
        }
        tx.commit().await.repository()?;
        Ok(DetailMergeOutcome::Applied)
    }

    /// Delete a Add-on media list and its generated playlist if present.
    ///
    /// # Errors
    ///
    /// Returns an error when persistence fails.
    pub async fn delete_list(&self, list_id: EntityId) -> Result<()> {
        ensure_not_publishing(&self.pool, list_id).await?;
        ensure_not_import_finalizing(&self.pool, list_id).await?;
        let library_id = list_library_id(&self.pool, list_id).await?;
        let removal = self.prepare_generated_playlist_removal(list_id).await?;
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        ensure_not_publishing_tx(&mut tx, list_id).await?;
        ensure_not_import_finalizing_tx(&mut tx, list_id).await?;
        if let Some(removal) = &removal {
            self.remove_generated_playlist_in_tx(&mut tx, list_id, removal, None)
                .await?;
        }

        if let Some(library_id) = library_id {
            sqlx::query(
                r#"UPDATE "ScanPlayoutRefreshIntent" SET "LibraryId" = NULL
                   WHERE "LibraryId" = ?"#,
            )
            .bind(library_id)
            .execute(&mut *tx)
            .await
            .repository()?;
            sqlx::query(r#"DELETE FROM "ScanRefreshResolution" WHERE "LibraryId" = ?"#)
                .bind(library_id)
                .execute(&mut *tx)
                .await
                .repository()?;
            sqlx::query(r#"DELETE FROM "AddonMediaListLibrary" WHERE "AddonMediaListId" = ?"#)
                .bind(list_id)
                .execute(&mut *tx)
                .await
                .repository()?;
            sqlx::query(r#"DELETE FROM "Library" WHERE "Id" = ?"#)
                .bind(library_id)
                .execute(&mut *tx)
                .await
                .repository()?;
        }

        unlink_revision_artifacts_for_list_in_tx(&mut tx, list_id)
            .await
            .repository()?;
        let deleted = sqlx::query(r#"DELETE FROM "AddonMediaList" WHERE "Id" = ?"#)
            .bind(list_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        if deleted.rows_affected() != 1 {
            return Err(RepositoryError::NotFound {
                kind: "add-on media list",
                id: list_id,
            });
        }
        tx.commit().await.repository()?;
        Ok(())
    }

    /// Re-match lists whose last local match is older than one hour.
    ///
    /// # Errors
    ///
    /// Returns an error when list loading or database writes fail.
    pub async fn match_due_lists(&self) -> Result<usize> {
        self.match_due_lists_with_report()
            .await
            .map(|report| report.matched_list_count)
    }

    /// Re-match due lists and report committed generated-playlist refreshes.
    ///
    /// # Errors
    ///
    /// Returns an error when list loading, matching, impact resolution, or
    /// persistence fails.
    pub async fn match_due_lists_with_report(&self) -> Result<AddonMediaListMatchReport> {
        let cutoff = Utc::now() - Duration::hours(1);
        let rows = sqlx::query(
            r#"SELECT "Id"
               FROM "AddonMediaList"
               WHERE ("LastMatch" IS NULL OR "LastMatch" <= ?)
                 AND ("PublicationJournalJson" IS NULL
                      OR TRIM("PublicationJournalJson") = '')
               ORDER BY "Id""#,
        )
        .bind(encode_datetime(cutoff))
        .fetch_all(&self.pool)
        .await
        .repository()?;

        let mut report = AddonMediaListMatchReport::default();
        for row in rows {
            report.merge(
                self.match_list_items_with_report(row.try_get("Id").repository()?)
                    .await?,
            );
        }

        Ok(report)
    }

    /// Re-match every persisted list after a provider or Local library scan
    /// changes candidate identities, availability, or replacement validity.
    ///
    /// # Errors
    ///
    /// Returns an error when list loading, matching, or playlist projection fails.
    pub async fn match_all_lists(&self) -> Result<usize> {
        self.match_all_lists_with_report()
            .await
            .map(|report| report.matched_list_count)
    }

    /// Re-match every persisted list and report committed generated-playlist
    /// refreshes for immediate queue admission by the host.
    ///
    /// # Errors
    ///
    /// Returns an error when list loading, matching, impact resolution, or
    /// persistence fails.
    pub async fn match_all_lists_with_report(&self) -> Result<AddonMediaListMatchReport> {
        let ids = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "Id" FROM "AddonMediaList"
               WHERE "PublicationJournalJson" IS NULL
                  OR TRIM("PublicationJournalJson") = ''
               ORDER BY "Id""#,
        )
        .fetch_all(&self.pool)
        .await
        .repository()?;
        let mut report = AddonMediaListMatchReport::default();
        for id in ids {
            report.merge(self.match_list_items_with_report(id).await?);
        }
        Ok(report)
    }

    async fn save_list(
        &self,
        list: &AddonMediaList,
        metadata: &ersatzrs_core::repositories::AddonMediaListNfoMetadata,
    ) -> Result<EntityId> {
        let provider_metadata = encode_metadata(metadata)?;
        let maybe_existing = sqlx::query(
            r#"SELECT "Id", "ProviderMetadataJson", "MetadataOverrideJson",
                      "MetadataConflictJson"
               FROM "AddonMediaList"
               WHERE "AddonId" = ? AND "ProviderId" = ?
               ORDER BY "Id"
               LIMIT 1"#,
        )
        .bind(&list.addon_id)
        .bind(&list.provider_id)
        .fetch_optional(&self.pool)
        .await
        .repository()?;

        if let Some(row) = maybe_existing {
            let id = row.try_get("Id").repository()?;
            let previous_provider: String = row.try_get("ProviderMetadataJson").repository()?;
            let overrides: String = row.try_get("MetadataOverrideJson").repository()?;
            let existing_conflicts: Option<String> =
                row.try_get("MetadataConflictJson").repository()?;
            let conflicts = refresh_conflict_json(
                &previous_provider,
                &provider_metadata,
                &overrides,
                existing_conflicts.as_deref(),
            )?;
            let updated = sqlx::query(
                r#"UPDATE "AddonMediaList"
                   SET "SourceUrl" = ?, "Name" = ?, "Description" = ?, "ItemCount" = ?,
                       "ContentKindOverride" = ?, "ChapterInput" = ?,
                       "ProviderMetadataJson" = ?, "MetadataConflictJson" = ?,
                       "LastError" = NULL,
                       "ReviewState" = CASE WHEN "ReviewState" = 0 THEN 0 ELSE 1 END,
                       "ReviewError" = NULL, "DraftRevision" = "DraftRevision" + 1
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
            .bind(&list.source_url)
            .bind(&list.name)
            .bind(&list.description)
            .bind(list.item_count)
            .bind(encode_enum(list.content_kind_override))
            .bind((!list.chapter_input.is_empty()).then_some(&list.chapter_input))
            .bind(&provider_metadata)
            .bind(conflicts)
            .bind(id)
            .bind(encode_enum(ManagedLibraryOperationState::Queued))
            .bind(encode_enum(ManagedLibraryOperationState::Running))
            .bind(encode_enum(ManagedLibraryOperationState::WaitingToRetry))
            .execute(&self.pool)
            .await
            .repository()?;
            if updated.rows_affected() != 1 {
                return Err(publication_in_progress());
            }
            Ok(id)
        } else {
            let mut transaction = crate::db::begin_write(&self.pool).await.repository()?;
            let result = sqlx::query(
                r#"INSERT INTO "AddonMediaList" (
                    "AddonId", "SourceUrl", "ProviderId", "Name", "Description",
                    "AutoRefresh", "GeneratePlaylist", "ItemCount", "PlaylistId",
                    "LastUpdate", "LastMatch", "LastError", "ContentKindOverride", "ChapterInput",
                    "ProviderMetadataJson", "MetadataOverrideJson"
                   ) VALUES (?, ?, ?, ?, ?, 0, 0, ?, NULL, NULL, NULL, NULL, ?, ?, ?, '{}')"#,
            )
            .bind(&list.addon_id)
            .bind(&list.source_url)
            .bind(&list.provider_id)
            .bind(&list.name)
            .bind(&list.description)
            .bind(list.item_count)
            .bind(encode_enum(list.content_kind_override))
            .bind((!list.chapter_input.is_empty()).then_some(&list.chapter_input))
            .bind(&provider_metadata)
            .execute(&mut *transaction)
            .await
            .repository()?;
            let id = inserted_id(&mut transaction, result).await.repository()?;
            crate::repositories::SqliteAddonMediaListRepository::initialize_option_defaults_in_tx(
                &mut transaction,
                id,
            )
            .await?;
            transaction.commit().await.repository()?;
            Ok(id)
        }
    }

    #[allow(clippy::too_many_lines)] // Keep the fenced full-header replacement in one transaction.
    async fn save_import_list_header(
        &self,
        job_id: EntityId,
        lease_owner: &str,
        list: &AddonMediaList,
        metadata: &ersatzrs_core::repositories::AddonMediaListNfoMetadata,
        incremental: bool,
        bump_revision: bool,
    ) -> Result<()> {
        let fence = ImportFinalizationFence {
            job_id,
            lease_owner,
            expected_phase: 0,
            next_phase: 1,
        };
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        begin_import_finalization_phase(&mut tx, fence).await?;
        let previous_revision: i32 =
            sqlx::query_scalar(r#"SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id" = ?"#)
                .bind(list.id)
                .fetch_one(&mut *tx)
                .await
                .repository()?;
        if incremental {
            update_incremental_import_list_header(&mut tx, list, metadata, bump_revision).await?;
        } else {
            let current = sqlx::query(
                r#"SELECT "ProviderId", "Description", "ProviderMetadataJson",
                          "MetadataOverrideJson", "MetadataConflictJson"
                   FROM "AddonMediaList" WHERE "Id" = ? AND "AddonId" = ?"#,
            )
            .bind(list.id)
            .bind(&list.addon_id)
            .fetch_optional(&mut *tx)
            .await
            .repository()?
            .ok_or_else(|| {
                RepositoryError::Conflict(
                    "the imported list identity changed before finalization".to_owned(),
                )
            })?;
            let previous_provider: String = current.try_get("ProviderMetadataJson").repository()?;
            let current_provider_id: String = current.try_get("ProviderId").repository()?;
            let same_provider = current_provider_id == list.provider_id;
            let provider_metadata = if same_provider {
                merge_present_metadata(&previous_provider, metadata)?
            } else {
                encode_metadata(metadata)?
            };
            let description = if same_provider {
                list.description
                    .clone()
                    .or(current.try_get("Description").repository()?)
            } else {
                list.description.clone()
            };
            let overrides: String = current.try_get("MetadataOverrideJson").repository()?;
            let existing_conflicts: Option<String> =
                current.try_get("MetadataConflictJson").repository()?;
            let conflicts = refresh_conflict_json(
                &previous_provider,
                &provider_metadata,
                &overrides,
                existing_conflicts.as_deref(),
            )?;
            let updated = sqlx::query(
                r#"UPDATE "AddonMediaList"
                   SET "ProviderId" = ?, "SourceUrl" = ?, "Name" = ?, "Description" = ?,
                       "ItemCount" = ?, "ContentKindOverride" = ?, "ChapterInput" = ?,
                       "ProviderMetadataJson" = ?, "MetadataConflictJson" = ?,
                       "LastError" = NULL,
                       "ReviewState" = CASE WHEN "ReviewState" = 0 THEN 0 ELSE 1 END,
                       "ReviewError" = NULL, "DraftRevision" = "DraftRevision" + 1
                   WHERE "Id" = ? AND "AddonId" = ? AND "ReviewState" <> 3
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
            .bind(&list.provider_id)
            .bind(&list.source_url)
            .bind(&list.name)
            .bind(description)
            .bind(list.item_count)
            .bind(encode_enum(list.content_kind_override))
            .bind((!list.chapter_input.is_empty()).then_some(&list.chapter_input))
            .bind(provider_metadata)
            .bind(conflicts)
            .bind(list.id)
            .bind(&list.addon_id)
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
                return Err(publication_in_progress());
            }
        }
        let current_revision: i32 =
            sqlx::query_scalar(r#"SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id" = ?"#)
                .bind(list.id)
                .fetch_one(&mut *tx)
                .await
                .repository()?;
        crate::addon_media_list_auto_publication::advance_import_revision_in_tx(
            &mut tx,
            list.id,
            job_id,
            current_revision - previous_revision,
        )
        .await?;
        complete_import_finalization_phase(&mut tx, fence).await?;
        tx.commit().await.repository()
    }

    #[allow(clippy::too_many_lines)]
    async fn save_list_items(
        &self,
        list_id: EntityId,
        incoming: &[EnumeratedAddonMediaListItem],
    ) -> Result<AddonMediaList> {
        self.save_list_items_with_mode(list_id, incoming, false, None, None)
            .await
    }

    async fn save_import_items(
        &self,
        job_id: EntityId,
        lease_owner: &str,
        list_id: EntityId,
        incoming: &[EnumeratedAddonMediaListItem],
        delta: bool,
    ) -> Result<AddonMediaList> {
        self.save_list_items_with_mode(
            list_id,
            incoming,
            delta,
            None,
            Some(ImportFinalizationFence {
                job_id,
                lease_owner,
                expected_phase: 1,
                next_phase: 2,
            }),
        )
        .await
    }

    async fn save_reconcile_items(
        &self,
        job_id: EntityId,
        lease_owner: &str,
        list_id: EntityId,
        generation: i32,
        incoming: &[EnumeratedAddonMediaListItem],
    ) -> Result<AddonMediaList> {
        self.save_list_items_with_mode(
            list_id,
            incoming,
            false,
            Some(generation),
            Some(ImportFinalizationFence {
                job_id,
                lease_owner,
                expected_phase: 1,
                next_phase: 2,
            }),
        )
        .await
    }

    #[allow(clippy::too_many_lines)]
    async fn save_list_items_with_mode(
        &self,
        list_id: EntityId,
        incoming: &[EnumeratedAddonMediaListItem],
        delta: bool,
        reconcile_generation: Option<i32>,
        finalization: Option<ImportFinalizationFence<'_>>,
    ) -> Result<AddonMediaList> {
        let incoming_ids = incoming
            .iter()
            .map(|item| item.provider_id.as_str())
            .collect::<HashSet<_>>();
        let existing = self.load_existing_items(list_id).await?;
        let existing_ids = existing
            .iter()
            .map(|item| (item.provider_id.as_str(), item.id))
            .collect::<HashMap<_, _>>();
        let draft_ids = existing
            .iter()
            .filter(|item| item.in_draft)
            .map(|item| item.provider_id.as_str())
            .collect::<HashSet<_>>();
        let observed_at = Utc::now();
        let observed_at_text = encode_datetime(observed_at);

        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        if let Some(fence) = finalization {
            begin_import_finalization_phase(&mut tx, fence).await?;
        }
        ensure_not_publishing_tx(&mut tx, list_id).await?;
        if delta {
            let mut incoming_ranks = incoming
                .iter()
                .filter(|item| !draft_ids.contains(item.provider_id.as_str()))
                .map(|item| item.rank)
                .collect::<Vec<_>>();
            incoming_ranks.sort_unstable();
            for item in existing.iter().filter(|item| item.in_draft) {
                if incoming_ids.contains(item.provider_id.as_str()) {
                    continue;
                }
                let shifted_rank = shifted_rank_after_insertions(item.rank, &incoming_ranks);
                if shifted_rank != item.rank {
                    sqlx::query(r#"UPDATE "AddonMediaListItem" SET "Rank" = ? WHERE "Id" = ?"#)
                        .bind(shifted_rank)
                        .bind(item.id)
                        .execute(&mut *tx)
                        .await
                        .repository()?;
                }
            }
        } else {
            sqlx::query(r#"DELETE FROM "AddonMediaListChapter" WHERE "AddonMediaListId" = ?"#)
                .bind(list_id)
                .execute(&mut *tx)
                .await
                .repository()?;
        }
        let mut retained_count = 0_usize;
        let mut draft_membership_changed = false;
        let mut tombstone_rank = incoming
            .iter()
            .map(|item| item.rank)
            .max()
            .unwrap_or(-1)
            .saturating_add(1);
        for item in existing.iter().filter(|item| item.in_draft) {
            if delta {
                break;
            }
            if !incoming_ids.contains(item.provider_id.as_str()) {
                if item.parent_provider_id.is_some() && reconcile_generation.is_none() {
                    if item.published {
                        draft_membership_changed = true;
                        remove_retired_draft_jobs(&mut tx, list_id, item.id).await?;
                    }
                    let sql = if item.published {
                        r#"UPDATE "AddonMediaListItem" SET "InDraft"=0 WHERE "Id"=?"#
                    } else {
                        r#"DELETE FROM "AddonMediaListItem" WHERE "Id" = ?"#
                    };
                    sqlx::query(sql)
                        .bind(item.id)
                        .execute(&mut *tx)
                        .await
                        .repository()?;
                    continue;
                }
                // Provider snapshots can be incomplete because of pagination,
                // transient availability, or permission changes. Retain a
                // provider-neutral tombstone so an operator's pinned Local
                // replacement survives and the same provider ID can recover.
                sqlx::query(
                    r#"UPDATE "AddonMediaListItem"
                       SET "Rank" = ?, "Availability" = ?, "AvailabilityReason" = ?,
                           "AvailabilityReasonCode" = ?, "AvailabilityUpdated" = ?,
                           "MediaItemId" = CASE WHEN ? THEN "MediaItemId" ELSE NULL END
                       WHERE "Id" = ?"#,
                )
                .bind(tombstone_rank)
                .bind(encode_enum(AddonMediaListItemAvailability::Unavailable))
                .bind(OMITTED_BY_PROVIDER_REASON)
                .bind(encode_enum(
                    ersatzrs_domain::AddonMediaListAvailabilityReason::Unavailable,
                ))
                .bind(&observed_at_text)
                .bind(reconcile_generation.is_some())
                .bind(item.id)
                .execute(&mut *tx)
                .await
                .repository()?;
                tombstone_rank = tombstone_rank.saturating_add(1);
                retained_count += 1;
            }
        }

        let mut persisted_ids = HashMap::new();
        for item in incoming {
            let item_id = if let Some(existing_id) =
                existing_ids.get(item.provider_id.as_str()).copied()
            {
                let image_urls = encode_image_urls(&item.additional_image_urls)?;
                let provider_metadata = encode_metadata(&item.metadata)?;
                let existing_item = existing
                    .iter()
                    .find(|existing| existing.id == existing_id)
                    .ok_or_else(|| RepositoryError::Decode {
                        kind: "add-on media-list refresh identity",
                        value: existing_id.to_string(),
                    })?;
                if !existing_item.in_draft {
                    draft_membership_changed = true;
                    readopt_draft_item(&mut tx, list_id, existing_id, item.collection_role).await?;
                }
                let conflicts = if existing_item.in_draft {
                    refresh_conflict_json(
                        &existing_item.provider_metadata_json,
                        &provider_metadata,
                        &existing_item.metadata_override_json,
                        existing_item.metadata_conflict_json.as_deref(),
                    )?
                } else {
                    None
                };
                let (effective_liveness, liveness_conflict) = if existing_item.in_draft {
                    refresh_liveness(
                        existing_item.provider_liveness,
                        item.liveness,
                        existing_item.liveness_override,
                        existing_item.liveness_conflict,
                    )
                } else {
                    (item.liveness, false)
                };
                sqlx::query(
                    r#"UPDATE "AddonMediaListItem"
                       SET "InDraft" = 1, "Kind" = ?, "Rank" = ?, "Title" = ?, "Year" = ?,
                           "DisplayTitle" = ?,
                           "Season" = CASE WHEN ? THEN "Season" ELSE ? END,
                           "Episode" = CASE WHEN ? THEN "Episode" ELSE ? END,
                           "SourceUrl" = ?, "Availability" = ?,
                           "AvailabilityReason" = ?, "AvailabilityReasonCode" = ?,
                           "AvailabilityUpdated" = ?,
                           "ContentKind" = CASE WHEN ? THEN "ContentKind" ELSE ? END,
                           "DurationSeconds" = ?,
                           "ParentProviderId" = ?, "FragmentStartSeconds" = ?,
                           "FragmentEndSeconds" = ?, "AdditionalImageUrls" = ?,
                           "ProviderMetadataJson" = ?, "MetadataConflictJson" = ?,
                           "ProviderLiveness" = ?, "EffectiveLiveness" = ?,
                           "LivenessConflict" = ?,
                           "MediaItemId" = CASE WHEN ? THEN "MediaItemId" ELSE NULL END,
                           "LastReconcileGeneration" =
                               COALESCE(?, "LastReconcileGeneration")
                       WHERE "Id" = ?"#,
                )
                .bind(encode_enum(item.kind))
                .bind(item.rank)
                .bind(&item.title)
                .bind(item.year)
                .bind(&item.display_title)
                .bind(reconcile_generation.is_some() && existing_item.in_draft)
                .bind(item.season)
                .bind(reconcile_generation.is_some() && existing_item.in_draft)
                .bind(item.episode)
                .bind(&item.source_url)
                .bind(encode_enum(item.availability))
                .bind(&item.availability_reason)
                .bind(item.availability_reason_code.map(encode_enum))
                .bind(encode_datetime(Utc::now()))
                .bind(reconcile_generation.is_some() && existing_item.in_draft)
                .bind(encode_enum(item.content_kind))
                .bind(optional_fragment_bound(item.duration_seconds, "duration")?)
                .bind(&item.parent_provider_id)
                .bind(optional_fragment_bound(
                    item.fragment_start_seconds,
                    "fragment start",
                )?)
                .bind(optional_fragment_bound(
                    item.fragment_end_seconds,
                    "fragment end",
                )?)
                .bind(image_urls)
                .bind(provider_metadata)
                .bind(conflicts)
                .bind(encode_enum(item.liveness))
                .bind(encode_enum(effective_liveness))
                .bind(liveness_conflict)
                .bind(reconcile_generation.is_some() || !existing_item.in_draft)
                .bind(reconcile_generation)
                .bind(existing_id)
                .execute(&mut *tx)
                .await
                .repository()?;
                sqlx::query(
                    r#"DELETE FROM "AddonMediaListItemGuid" WHERE "AddonMediaListItemId" = ?"#,
                )
                .bind(existing_id)
                .execute(&mut *tx)
                .await
                .repository()?;
                insert_item_guids(&mut tx, existing_id, &item.guids).await?;
                existing_id
            } else {
                let item_id = insert_item(&mut tx, list_id, item, reconcile_generation).await?;
                insert_item_guids(&mut tx, item_id, &item.guids).await?;
                item_id
            };
            persisted_ids.insert(item.provider_id.as_str(), item_id);
        }

        let mut chapter_indexes: HashMap<&str, i32> = HashMap::new();
        for item in incoming {
            let Some(parent_provider_id) = item.parent_provider_id.as_deref() else {
                continue;
            };
            let source_item_id =
                persisted_ids
                    .get(parent_provider_id)
                    .copied()
                    .ok_or_else(|| {
                        RepositoryError::Conflict(format!(
                            "chapter fragment {} references missing parent {parent_provider_id}",
                            item.provider_id
                        ))
                    })?;
            let fragment_item_id = persisted_ids[&item.provider_id.as_str()];
            if delta {
                sqlx::query(r#"DELETE FROM "AddonMediaListChapter" WHERE "FragmentItemId" = ?"#)
                    .bind(fragment_item_id)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
            }
            let chapter_index = chapter_indexes.entry(parent_provider_id).or_default();
            sqlx::query(
                r#"INSERT INTO "AddonMediaListChapter" (
                    "AddonMediaListId", "SourceItemId", "FragmentItemId", "ChapterIndex",
                    "Title", "StartSeconds", "EndSeconds"
                   ) VALUES (?, ?, ?, ?, ?, ?, ?)"#,
            )
            .bind(list_id)
            .bind(source_item_id)
            .bind(fragment_item_id)
            .bind(*chapter_index)
            .bind(&item.title)
            .bind(fragment_bound(
                item.fragment_start_seconds,
                "fragment start",
            )?)
            .bind(optional_fragment_bound(
                item.fragment_end_seconds,
                "fragment end",
            )?)
            .execute(&mut *tx)
            .await
            .repository()?;
            *chapter_index = chapter_index.saturating_add(1);
        }

        crate::addon_media_list_refresh::record_refresh_success_tx(&mut tx, list_id, observed_at)
            .await?;
        let item_count = if delta {
            sqlx::query_scalar::<_, i64>(
                r#"SELECT COUNT(*) FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ? AND "InDraft" = 1"#,
            )
            .bind(list_id)
            .fetch_one(&mut *tx)
            .await
            .repository()?
        } else {
            i64::try_from(incoming.len().saturating_add(retained_count)).unwrap_or(i64::MAX)
        };
        let draft_changed = draft_membership_changed
            || reconcile_generation.is_some()
            || (delta && !incoming.is_empty());
        sqlx::query(
            r#"UPDATE "AddonMediaList"
               SET "ItemCount" = ?,
                   "ReviewState" = CASE
                       WHEN ? = 1 AND "ReviewState" NOT IN (0, 3) THEN 1
                       ELSE "ReviewState"
                   END,
                   "ReviewError" = CASE WHEN ? = 1 THEN NULL ELSE "ReviewError" END,
                   "DraftRevision" = "DraftRevision" + ?
               WHERE "Id" = ?"#,
        )
        .bind(item_count)
        .bind(draft_changed)
        .bind(draft_changed)
        .bind(i32::from(draft_changed))
        .bind(list_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        if let Some(fence) = finalization {
            crate::addon_media_list_auto_publication::advance_import_revision_in_tx(
                &mut tx,
                list_id,
                fence.job_id,
                i32::from(draft_changed),
            )
            .await?;
            complete_import_finalization_phase(&mut tx, fence).await?;
        }
        tx.commit().await.repository()?;

        self.load_list(list_id).await
    }

    async fn load_list(&self, list_id: EntityId) -> Result<AddonMediaList> {
        let row = sqlx::query(r#"SELECT * FROM "AddonMediaList" WHERE "Id" = ?"#)
            .bind(list_id)
            .fetch_optional(&self.pool)
            .await
            .repository()?
            .ok_or(RepositoryError::NotFound {
                kind: "add-on media list",
                id: list_id,
            })?;

        Ok(AddonMediaList {
            id: row.try_get("Id").repository()?,
            addon_id: row.try_get("AddonId").repository()?,
            source_url: row.try_get("SourceUrl").repository()?,
            provider_id: row.try_get("ProviderId").repository()?,
            name: row.try_get("Name").repository()?,
            description: row.try_get("Description").repository()?,
            item_count: row.try_get("ItemCount").repository()?,
            refresh_cadence: decode_enum_column(
                "AddonMediaListRefreshCadence",
                &row,
                "RefreshCadence",
            )?,
            generate_playlist: decode_bool_column(&row, "GeneratePlaylist")?,
            content_kind_override: decode_enum_column(
                "AddonMediaListContentKind",
                &row,
                "ContentKindOverride",
            )?,
            presentation_kind: decode_enum_column(
                "AddonMediaListPresentationKind",
                &row,
                "PresentationKind",
            )?,
            requested_presentation_kind: decode_enum_column(
                "AddonMediaListPresentationKind",
                &row,
                "RequestedPresentationKind",
            )?,
            presentation_error: row.try_get("PresentationError").repository()?,
            chapter_input: row
                .try_get::<Option<String>, _>("ChapterInput")
                .repository()?
                .unwrap_or_default(),
            playlist_id: row.try_get("PlaylistId").repository()?,
            last_update: decode_optional_datetime(
                "AddonMediaList.LastUpdate",
                row.try_get("LastUpdate").repository()?,
            )?,
            last_match: decode_optional_datetime(
                "AddonMediaList.LastMatch",
                row.try_get("LastMatch").repository()?,
            )?,
            last_error: row.try_get("LastError").repository()?,
        })
    }

    async fn load_existing_items(&self, list_id: EntityId) -> Result<Vec<ExistingItem>> {
        let rows = sqlx::query(
            r#"SELECT "Id", "InDraft", "Published", "ProviderId", "Rank", "ParentProviderId", "ProviderMetadataJson",
                      "MetadataOverrideJson", "MetadataConflictJson", "ProviderLiveness",
                      "LivenessOverride", "LivenessConflict"
               FROM "AddonMediaListItem"
               WHERE "AddonMediaListId" = ?
               ORDER BY "Rank", "Id""#,
        )
        .bind(list_id)
        .fetch_all(&self.pool)
        .await
        .repository()?;

        rows.iter()
            .map(|row| {
                Ok(ExistingItem {
                    id: row.try_get("Id").repository()?,
                    in_draft: decode_bool_column(row, "InDraft")?,
                    published: decode_bool_column(row, "Published")?,
                    provider_id: row.try_get("ProviderId").repository()?,
                    rank: row.try_get("Rank").repository()?,
                    parent_provider_id: row.try_get("ParentProviderId").repository()?,
                    provider_metadata_json: row.try_get("ProviderMetadataJson").repository()?,
                    metadata_override_json: row.try_get("MetadataOverrideJson").repository()?,
                    metadata_conflict_json: row.try_get("MetadataConflictJson").repository()?,
                    provider_liveness: decode_enum_column(
                        "RemoteStreamLiveness",
                        row,
                        "ProviderLiveness",
                    )?,
                    liveness_override: decode_optional_enum_column(
                        "RemoteStreamLiveness",
                        row,
                        "LivenessOverride",
                    )?,
                    liveness_conflict: decode_bool_column(row, "LivenessConflict")?,
                })
            })
            .collect()
    }

    async fn load_item_guids(&self, item_id: EntityId) -> Result<Vec<String>> {
        let rows = sqlx::query(
            r#"SELECT "Guid"
               FROM "AddonMediaListItemGuid"
               WHERE "AddonMediaListItemId" = ?
               ORDER BY "Id""#,
        )
        .bind(item_id)
        .fetch_all(&self.pool)
        .await
        .repository()?;

        rows.iter()
            .map(|row| {
                Ok(row
                    .try_get::<Option<String>, _>("Guid")
                    .repository()?
                    .unwrap_or_default())
            })
            .collect()
    }

    async fn identify_item(&self, item: &MatchItem) -> Result<Option<EntityId>> {
        match item.kind {
            AddonMediaListItemKind::Movie => self.identify_movie(item).await,
            AddonMediaListItemKind::Show => self.identify_show(item).await,
            AddonMediaListItemKind::Season => self.identify_season(item).await,
            AddonMediaListItemKind::Episode => self.identify_episode(item).await,
            AddonMediaListItemKind::RemoteStream => self.identify_remote_stream(item).await,
        }
    }

    async fn identify_remote_stream(&self, item: &MatchItem) -> Result<Option<EntityId>> {
        if let Some(id) = sqlx::query_scalar(
            r#"SELECT "Id" FROM "RemoteStream"
               WHERE "AddonId" = ? AND "ProviderId" = ?
               ORDER BY "Id" LIMIT 1"#,
        )
        .bind(&item.addon_id)
        .bind(&item.provider_id)
        .fetch_optional(&self.pool)
        .await
        .repository()?
        {
            return Ok(Some(id));
        }
        let Some(source_url) = item.source_url.as_deref() else {
            return Ok(None);
        };
        sqlx::query_scalar(
            r#"SELECT "Id" FROM "RemoteStream" WHERE "Url" = ? ORDER BY "Id" LIMIT 1"#,
        )
        .bind(source_url)
        .fetch_optional(&self.pool)
        .await
        .repository()
    }

    async fn identify_movie(&self, item: &MatchItem) -> Result<Option<EntityId>> {
        if let Some(id) = self
            .identify_by_guid("MovieMetadata", "MovieMetadataId", "MovieId", &item.guids)
            .await?
        {
            return Ok(Some(id));
        }

        sqlx::query_scalar(
            r#"SELECT "MovieId"
               FROM "MovieMetadata"
               WHERE "Title" = ? AND ("Year" = ? OR ("Year" IS NULL AND ? IS NULL))
               ORDER BY "MovieId"
               LIMIT 1"#,
        )
        .bind(&item.title)
        .bind(item.year)
        .bind(item.year)
        .fetch_optional(&self.pool)
        .await
        .repository()
    }

    async fn identify_show(&self, item: &MatchItem) -> Result<Option<EntityId>> {
        if let Some(id) = self
            .identify_by_guid("ShowMetadata", "ShowMetadataId", "ShowId", &item.guids)
            .await?
        {
            return Ok(Some(id));
        }

        sqlx::query_scalar(
            r#"SELECT "ShowId"
               FROM "ShowMetadata"
               WHERE "Title" = ? AND ("Year" = ? OR ("Year" IS NULL AND ? IS NULL))
               ORDER BY "ShowId"
               LIMIT 1"#,
        )
        .bind(&item.title)
        .bind(item.year)
        .bind(item.year)
        .fetch_optional(&self.pool)
        .await
        .repository()
    }

    async fn identify_season(&self, item: &MatchItem) -> Result<Option<EntityId>> {
        if let Some(id) = self
            .identify_by_guid(
                "SeasonMetadata",
                "SeasonMetadataId",
                "SeasonId",
                &item.guids,
            )
            .await?
        {
            return Ok(Some(id));
        }

        sqlx::query_scalar(
            r#"SELECT "sem"."SeasonId"
               FROM "SeasonMetadata" AS "sem"
               JOIN "Season" AS "s" ON "s"."Id" = "sem"."SeasonId"
               JOIN "ShowMetadata" AS "sm" ON "sm"."ShowId" = "s"."ShowId"
               WHERE "sm"."Title" = ?
                 AND ("sm"."Year" = ? OR ("sm"."Year" IS NULL AND ? IS NULL))
                 AND ("s"."SeasonNumber" = ? OR ("s"."SeasonNumber" IS NULL AND ? IS NULL))
               ORDER BY "sem"."SeasonId"
               LIMIT 1"#,
        )
        .bind(&item.title)
        .bind(item.year)
        .bind(item.year)
        .bind(item.season)
        .bind(item.season)
        .fetch_optional(&self.pool)
        .await
        .repository()
    }

    async fn identify_episode(&self, item: &MatchItem) -> Result<Option<EntityId>> {
        if let Some(id) = self
            .identify_by_guid(
                "EpisodeMetadata",
                "EpisodeMetadataId",
                "EpisodeId",
                &item.guids,
            )
            .await?
        {
            return Ok(Some(id));
        }

        sqlx::query_scalar(
            r#"SELECT "em"."EpisodeId"
               FROM "EpisodeMetadata" AS "em"
               JOIN "Episode" AS "e" ON "e"."Id" = "em"."EpisodeId"
               JOIN "Season" AS "s" ON "s"."Id" = "e"."SeasonId"
               JOIN "ShowMetadata" AS "sm" ON "sm"."ShowId" = "s"."ShowId"
               WHERE "sm"."Title" = ?
                 AND ("sm"."Year" = ? OR ("sm"."Year" IS NULL AND ? IS NULL))
                 AND ("s"."SeasonNumber" = ? OR ("s"."SeasonNumber" IS NULL AND ? IS NULL))
                 AND ("em"."EpisodeNumber" = ? OR ("em"."EpisodeNumber" IS NULL AND ? IS NULL))
               ORDER BY "em"."EpisodeId"
               LIMIT 1"#,
        )
        .bind(&item.title)
        .bind(item.year)
        .bind(item.year)
        .bind(item.season)
        .bind(item.season)
        .bind(item.episode)
        .bind(item.episode)
        .fetch_optional(&self.pool)
        .await
        .repository()
    }

    async fn identify_by_guid(
        &self,
        table: &str,
        guid_fk: &str,
        item_fk: &str,
        guids: &[String],
    ) -> Result<Option<EntityId>> {
        for guid in guids {
            let sql = format!(
                r#"SELECT "m"."{item_fk}"
                   FROM "MetadataGuid" AS "g"
                   JOIN "{table}" AS "m" ON "m"."Id" = "g"."{guid_fk}"
                   WHERE "g"."Guid" = ?
                   ORDER BY "m"."{item_fk}"
                   LIMIT 1"#
            );
            if let Some(id) = sqlx::query_scalar::<_, EntityId>(crate::db::assert_sql_safe(&sql))
                .bind(guid)
                .fetch_optional(&self.pool)
                .await
                .repository()?
            {
                return Ok(Some(id));
            }
        }

        Ok(None)
    }

    async fn prepare_generated_playlist_refresh(
        &self,
        list_id: EntityId,
        playlist_id: Option<EntityId>,
        reason: ScanPlayoutRefreshReason,
    ) -> Result<GeneratedPlaylistRefresh> {
        let library_id = list_library_id(&self.pool, list_id).await?;
        let playout_ids =
            crate::repositories::SqliteSchedulingContentRepository::new(self.pool.clone())
                .classic_playouts_affected_by_scan(&BTreeSet::new(), playlist_id)
                .await?;
        let generation = if let Some(library_id) = library_id {
            Some(
                crate::scan_refresh::begin_resolution(&self.pool, library_id, reason)
                    .await
                    .repository()?,
            )
        } else {
            None
        };
        Ok(GeneratedPlaylistRefresh {
            library_id,
            generation,
            reason,
            playout_ids,
        })
    }

    async fn fail_generated_playlist_refresh(
        &self,
        refresh: &GeneratedPlaylistRefresh,
        error: &RepositoryError,
    ) {
        if let (Some(library_id), Some(generation)) = (refresh.library_id, refresh.generation) {
            let _ = crate::scan_refresh::fail_resolution(
                &self.pool,
                library_id,
                generation,
                &error.to_string(),
            )
            .await;
        }
    }

    #[allow(clippy::too_many_arguments)] // Snapshot policy and publication ownership are separate guards.
    async fn commit_generated_playlist_creation(
        &self,
        list: &AddonMediaList,
        group_id: EntityId,
        desired: &[GeneratedPlaylistItem],
        refresh: &GeneratedPlaylistRefresh,
        publication_owned: bool,
        snapshot_owned: bool,
    ) -> Result<EntityId> {
        let mut transaction = crate::db::begin_write(&self.pool).await.repository()?;
        let result = sqlx::query(
            r#"INSERT INTO "Playlist" ("IsSystem", "Name", "PlaylistGroupId")
               VALUES (1, ?, ?)"#,
        )
        .bind(&list.name)
        .bind(group_id)
        .execute(&mut *transaction)
        .await
        .repository()?;
        let playlist_id = inserted_id(&mut transaction, result).await.repository()?;

        let linked = sqlx::query(
            r#"UPDATE "AddonMediaList" SET "PlaylistId" = ?
               WHERE "Id" = ? AND "PlaylistId" IS NULL AND ("GeneratePlaylist" = 1 OR ? = 1)"#,
        )
        .bind(playlist_id)
        .bind(list.id)
        .bind(snapshot_owned && publication_owned && list.generate_playlist)
        .execute(&mut *transaction)
        .await
        .repository()?;
        if linked.rows_affected() != 1 {
            return Err(RepositoryError::Conflict(
                "generated playlist state changed during matching".to_owned(),
            ));
        }
        replace_generated_playlist_items(&mut transaction, playlist_id, desired).await?;
        persist_generated_playlist_refresh_in_tx(&mut transaction, refresh, refresh.library_id)
            .await?;
        complete_generated_playlist_resolution_in_tx(&mut transaction, refresh).await?;
        let matched = if publication_owned {
            sqlx::query(r#"UPDATE "AddonMediaList" SET "LastMatch" = ? WHERE "Id" = ?"#)
                .bind(encode_datetime(Utc::now()))
                .bind(list.id)
                .execute(&mut *transaction)
                .await
                .repository()?
        } else {
            sqlx::query(
                r#"UPDATE "AddonMediaList" SET "LastMatch" = ?
                   WHERE "Id" = ? AND ("PublicationJournalJson" IS NULL
                                        OR TRIM("PublicationJournalJson") = '')"#,
            )
            .bind(encode_datetime(Utc::now()))
            .bind(list.id)
            .execute(&mut *transaction)
            .await
            .repository()?
        };
        if matched.rows_affected() != 1 {
            return Err(publication_in_progress());
        }
        transaction.commit().await.repository()?;

        Ok(playlist_id)
    }

    pub(crate) async fn prepare_generated_playlist_removal(
        &self,
        list_id: EntityId,
    ) -> Result<Option<GeneratedPlaylistRemoval>> {
        let list = self.load_list(list_id).await?;
        let Some(playlist_id) = list.playlist_id else {
            return Ok(None);
        };
        // Resolve before deleting the playlist: its schedule and filler
        // references cascade with the owned row and are no longer discoverable
        // afterward.
        let library_id = list_library_id(&self.pool, list_id).await?;
        let playout_ids =
            crate::repositories::SqliteSchedulingContentRepository::new(self.pool.clone())
                .classic_playouts_affected_by_scan(&BTreeSet::new(), Some(playlist_id))
                .await?;
        Ok(Some(GeneratedPlaylistRemoval {
            playlist_id,
            refresh: GeneratedPlaylistRefresh {
                library_id,
                generation: None,
                reason: ScanPlayoutRefreshReason::RemoteStreamMembershipRemoved,
                playout_ids,
            },
        }))
    }

    /// Remove only list membership for a reset, retaining the generated
    /// playlist identity and every schedule/filler/deco/anchor reference to it.
    pub(crate) async fn empty_generated_playlist_for_reset(&self, list_id: EntityId) -> Result<()> {
        let list = self.load_list(list_id).await?;
        let Some(playlist_id) = list.playlist_id else {
            return Ok(());
        };
        let refresh = self
            .prepare_generated_playlist_refresh(
                list_id,
                Some(playlist_id),
                ScanPlayoutRefreshReason::RemoteStreamMembershipRemoved,
            )
            .await?;
        let result = self
            .commit_generated_playlist_match(list_id, playlist_id, &[], &refresh, true)
            .await;
        if let Err(error) = &result {
            self.fail_generated_playlist_refresh(&refresh, error).await;
        }
        result.map(|_| ())
    }

    pub(crate) async fn commit_generated_playlist_removal_in_tx(
        &self,
        tx: &mut Transaction<'_, Db>,
        list_id: EntityId,
        removal: &GeneratedPlaylistRemoval,
    ) -> Result<()> {
        self.remove_generated_playlist_in_tx(tx, list_id, removal, removal.refresh.library_id)
            .await
    }

    async fn remove_generated_playlist_in_tx(
        &self,
        tx: &mut Transaction<'_, Db>,
        list_id: EntityId,
        removal: &GeneratedPlaylistRemoval,
        intent_library_id: Option<EntityId>,
    ) -> Result<()> {
        let current_playlist_id = sqlx::query_scalar::<_, Option<EntityId>>(
            r#"SELECT "PlaylistId" FROM "AddonMediaList" WHERE "Id" = ?"#,
        )
        .bind(list_id)
        .fetch_one(&mut **tx)
        .await
        .repository()?;
        if current_playlist_id != Some(removal.playlist_id) {
            return Err(RepositoryError::Conflict(
                "generated playlist ownership changed before removal".to_owned(),
            ));
        }

        let deleted = sqlx::query(r#"DELETE FROM "Playlist" WHERE "Id" = ?"#)
            .bind(removal.playlist_id)
            .execute(&mut **tx)
            .await
            .repository()?;
        if deleted.rows_affected() != 1 {
            return Err(RepositoryError::Conflict(
                "generated playlist disappeared before removal".to_owned(),
            ));
        }
        sqlx::query(r#"UPDATE "AddonMediaList" SET "PlaylistId" = NULL WHERE "Id" = ?"#)
            .bind(list_id)
            .execute(&mut **tx)
            .await
            .repository()?;
        persist_generated_playlist_refresh_in_tx(tx, &removal.refresh, intent_library_id).await
    }

    async fn record_last_match(&self, list_id: EntityId, publication_owned: bool) -> Result<()> {
        let matched = if publication_owned {
            sqlx::query(r#"UPDATE "AddonMediaList" SET "LastMatch" = ? WHERE "Id" = ?"#)
                .bind(encode_datetime(Utc::now()))
                .bind(list_id)
                .execute(&self.pool)
                .await
                .repository()?
        } else {
            sqlx::query(
                r#"UPDATE "AddonMediaList" SET "LastMatch" = ?
                   WHERE "Id" = ? AND ("PublicationJournalJson" IS NULL
                                        OR TRIM("PublicationJournalJson") = '')"#,
            )
            .bind(encode_datetime(Utc::now()))
            .bind(list_id)
            .execute(&self.pool)
            .await
            .repository()?
        };
        if matched.rows_affected() != 1 {
            return Err(publication_in_progress());
        }
        Ok(())
    }

    async fn commit_generated_playlist_match(
        &self,
        list_id: EntityId,
        playlist_id: EntityId,
        desired: &[GeneratedPlaylistItem],
        refresh: &GeneratedPlaylistRefresh,
        publication_owned: bool,
    ) -> Result<bool> {
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        let persisted = load_generated_playlist_items_in_tx(&mut tx, playlist_id).await?;
        let changed = persisted != desired;
        if changed {
            replace_generated_playlist_items(&mut tx, playlist_id, desired).await?;
            persist_generated_playlist_refresh_in_tx(&mut tx, refresh, refresh.library_id).await?;
        }

        complete_generated_playlist_resolution_in_tx(&mut tx, refresh).await?;
        let matched = if publication_owned {
            sqlx::query(r#"UPDATE "AddonMediaList" SET "LastMatch" = ? WHERE "Id" = ?"#)
                .bind(encode_datetime(Utc::now()))
                .bind(list_id)
                .execute(&mut *tx)
                .await
                .repository()?
        } else {
            sqlx::query(
                r#"UPDATE "AddonMediaList" SET "LastMatch" = ?
                   WHERE "Id" = ? AND ("PublicationJournalJson" IS NULL
                                        OR TRIM("PublicationJournalJson") = '')"#,
            )
            .bind(encode_datetime(Utc::now()))
            .bind(list_id)
            .execute(&mut *tx)
            .await
            .repository()?
        };
        if matched.rows_affected() != 1 {
            return Err(publication_in_progress());
        }
        tx.commit().await.repository()?;
        Ok(changed)
    }

    async fn playlist_selection(
        &self,
        item: &MatchItem,
        provider_media_item_id: Option<EntityId>,
        source: Option<&AddonMediaListRevisionItem>,
    ) -> Result<Option<PlaylistSelection>> {
        let retained_replacement = source.and_then(|item| {
            item.replacement_media_item_id
                .zip(item.replacement_media_version_id)
        });
        if source.is_some_and(|item| {
            item.replacement_media_item_id.is_some() != item.replacement_media_version_id.is_some()
        }) {
            return Err(publication_in_progress());
        }
        if item.kind == AddonMediaListItemKind::RemoteStream
            && retained_replacement.is_none()
            && (item.liveness == RemoteStreamLiveness::Unknown
                || (item.liveness == RemoteStreamLiveness::Finite
                    && item.duration_seconds.is_none_or(|duration| duration == 0)))
        {
            return Ok(None);
        }
        if item.availability != AddonMediaListItemAvailability::Unavailable {
            let projected_episode_id = sqlx::query_scalar::<_, EntityId>(
                r#"SELECT "EpisodeId" FROM "AddonMediaListItemEpisode"
                   WHERE "AddonMediaListItemId" = ?"#,
            )
            .bind(item.id)
            .fetch_optional(&self.pool)
            .await
            .repository()?;
            if let Some(episode_id) = projected_episode_id {
                return Ok(Some(PlaylistSelection {
                    media_item_id: episode_id,
                    media_version_id: None,
                    video_stream_index: None,
                    collection_type: Some(COLLECTION_TYPE_EPISODE),
                }));
            }
            if let Some((media, version)) = retained_replacement {
                return self.replacement_selection(media, version).await;
            }
            return Ok(provider_media_item_id.map(PlaylistSelection::provider));
        }

        if source.is_some() {
            return match retained_replacement {
                Some((media, version)) => self.replacement_selection(media, version).await,
                None => Ok(None),
            };
        }
        let row = sqlx::query(
            r#"SELECT "MediaItemId", "MediaVersionId"
               FROM "AddonMediaListItemReplacement"
               WHERE "AddonMediaListItemId" = ?"#,
        )
        .bind(item.id)
        .fetch_optional(&self.pool)
        .await
        .repository()?;
        let Some(row) = row else {
            return Ok(None);
        };
        let media_item_id = row.try_get("MediaItemId").repository()?;
        let media_version_id = row.try_get("MediaVersionId").repository()?;
        self.replacement_selection(media_item_id, media_version_id)
            .await
    }

    async fn replacement_selection(
        &self,
        media_item_id: EntityId,
        media_version_id: EntityId,
    ) -> Result<Option<PlaylistSelection>> {
        let candidate = crate::repositories::load_replacement_candidate(
            &self.pool,
            media_item_id,
            media_version_id,
        )
        .await?;
        Ok(candidate.map(|candidate| PlaylistSelection {
            media_item_id: candidate.media_item_id,
            media_version_id: Some(candidate.media_version_id),
            video_stream_index: Some(candidate.video_stream_index),
            collection_type: Some(candidate.collection_type),
        }))
    }
}

fn shifted_rank_after_insertions(rank: i32, sorted_incoming_ranks: &[i32]) -> i32 {
    sorted_incoming_ranks
        .iter()
        .fold(rank, |shifted, incoming| {
            if *incoming <= shifted {
                shifted.saturating_add(1)
            } else {
                shifted
            }
        })
}

#[cfg(test)]
mod incremental_rank_tests {
    use super::shifted_rank_after_insertions;

    #[test]
    fn consecutive_insertions_advance_the_retained_rank_each_time() {
        assert_eq!(shifted_rank_after_insertions(0, &[0, 1]), 2);
        assert_eq!(shifted_rank_after_insertions(3, &[0, 2, 5]), 6);
        assert_eq!(shifted_rank_after_insertions(3, &[4]), 3);
    }
}

/// Imports and refreshes provider data through an installed add-on.
pub struct AddonMediaListRemoteSynchronizer<P> {
    local: AddonMediaListSynchronizer,
    provider: P,
    editor_revision: Option<i32>,
    delivery_mode: Option<ersatzrs_domain::AddonMediaListDeliveryMode>,
}

impl<P> AddonMediaListRemoteSynchronizer<P>
where
    P: AddonMediaListProvider,
{
    #[must_use]
    pub fn new(pool: DbPool, provider: P) -> Self {
        Self {
            local: AddonMediaListSynchronizer::new(pool),
            provider,
            delivery_mode: None,
            editor_revision: None,
        }
    }

    #[must_use]
    pub(crate) fn with_editor_revision(mut self, revision: Option<i32>) -> Self {
        self.editor_revision = revision;
        self
    }

    /// Preserve refresh defaults unless the operator explicitly selected delivery.
    #[must_use]
    pub(crate) fn with_delivery_mode(
        mut self,
        mode: Option<ersatzrs_domain::AddonMediaListDeliveryMode>,
    ) -> Self {
        self.delivery_mode = mode;
        self
    }

    async fn import_finalization_phase(&self, job_id: EntityId, lease_owner: &str) -> Result<i32> {
        sqlx::query_scalar(
            r#"SELECT "FinalizationPhase" FROM "AddonMediaListImportJob"
               WHERE "Id" = ? AND "LeaseOwner" = ? AND "OverviewComplete" = 1
                 AND "CancellationRequested" = 0"#,
        )
        .bind(job_id)
        .bind(lease_owner)
        .fetch_optional(&self.local.pool)
        .await
        .repository()?
        .ok_or_else(|| {
            RepositoryError::Conflict("the import finalization lease changed".to_owned())
        })
    }

    pub(crate) async fn persist_import_generation(
        &self,
        job_id: EntityId,
        lease_owner: &str,
        list: &AddonMediaList,
        enumerated: &mut EnumeratedAddonMediaList,
        incremental: bool,
    ) -> Result<AddonMediaList> {
        let mut phase = self.import_finalization_phase(job_id, lease_owner).await?;
        if phase == 0 {
            self.local
                .save_import_list_header(
                    job_id,
                    lease_owner,
                    list,
                    &enumerated.metadata,
                    incremental,
                    true,
                )
                .await?;
            phase = 1;
        }
        if phase == 1 {
            if !incremental && list.chapter_input.trim().is_empty() {
                self.expand_item_chapters(list.id, &list.addon_id, enumerated)
                    .await?;
            }
            self.local
                .save_import_items(job_id, lease_owner, list.id, &enumerated.items, incremental)
                .await?;
            phase = 2;
        }
        if !(2..=4).contains(&phase) {
            return Err(RepositoryError::Conflict(
                "the import finalization phase is invalid for item persistence".to_owned(),
            ));
        }
        self.local.load_list(list.id).await
    }

    pub(crate) async fn persist_reconcile_generation(
        &self,
        job_id: EntityId,
        lease_owner: &str,
        generation: i32,
        list: &AddonMediaList,
        enumerated: &mut EnumeratedAddonMediaList,
    ) -> Result<AddonMediaList> {
        let mut phase = self.import_finalization_phase(job_id, lease_owner).await?;
        if phase == 0 {
            self.local
                .save_import_list_header(
                    job_id,
                    lease_owner,
                    list,
                    &enumerated.metadata,
                    true,
                    false,
                )
                .await?;
            phase = 1;
        }
        if phase == 1 {
            if list.chapter_input.trim().is_empty() {
                self.expand_item_chapters(list.id, &list.addon_id, enumerated)
                    .await?;
            }
            self.local
                .save_reconcile_items(job_id, lease_owner, list.id, generation, &enumerated.items)
                .await?;
            phase = 2;
        }
        if !(2..=4).contains(&phase) {
            return Err(RepositoryError::Conflict(
                "the reconcile finalization phase is invalid for item persistence".to_owned(),
            ));
        }
        self.local.load_list(list.id).await
    }

    /// Commit absence/reappearance evidence only at the same database boundary
    /// that makes a fully finalized reconciliation ready for review.
    pub(crate) async fn finalize_reconcile_evidence_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, crate::db::Db>,
        list_id: EntityId,
        generation: i32,
        observed_at: DateTime<Utc>,
    ) -> Result<()> {
        let automatic_confirmation_cutoff = encode_datetime(observed_at - Duration::hours(24));
        let observed_at = encode_datetime(observed_at);
        sqlx::query(
            r#"INSERT INTO "AddonMediaListItemTombstone" (
                   "AddonMediaListItemId", "MissingSince", "LastObservedAt",
                   "ObservationCount", "LastReconcileGeneration", "ConfirmedAt", "ReappearedAt"
               )
               SELECT "item"."Id", ?, ?, 1, ?, NULL, NULL
               FROM "AddonMediaListItem" AS "item"
               WHERE "item"."AddonMediaListId" = ?
                 AND "item"."InDraft" = 1
                 AND ("item"."LastReconcileGeneration" IS NULL
                      OR "item"."LastReconcileGeneration" <> ?)
               ON CONFLICT ("AddonMediaListItemId") DO UPDATE SET
                   "MissingSince" = CASE
                       WHEN "AddonMediaListItemTombstone"."ReappearedAt" IS NOT NULL
                           THEN excluded."MissingSince"
                       ELSE "AddonMediaListItemTombstone"."MissingSince"
                   END,
                   "LastObservedAt" = excluded."LastObservedAt",
                   "ObservationCount" = CASE
                       WHEN "AddonMediaListItemTombstone"."ReappearedAt" IS NOT NULL THEN 1
                       ELSE "AddonMediaListItemTombstone"."ObservationCount" + 1
                   END,
                   "LastReconcileGeneration" = excluded."LastReconcileGeneration",
                   "ConfirmedAt" = CASE
                       WHEN "AddonMediaListItemTombstone"."ReappearedAt" IS NOT NULL THEN NULL
                       WHEN "AddonMediaListItemTombstone"."ConfirmedAt" IS NOT NULL
                           THEN "AddonMediaListItemTombstone"."ConfirmedAt"
                       WHEN "AddonMediaListItemTombstone"."MissingSince" <= ?
                           THEN excluded."LastObservedAt"
                       ELSE NULL
                   END,
                   "ReappearedAt" = NULL
               WHERE "AddonMediaListItemTombstone"."LastReconcileGeneration" <
                     excluded."LastReconcileGeneration""#,
        )
        .bind(&observed_at)
        .bind(&observed_at)
        .bind(generation)
        .bind(list_id)
        .bind(generation)
        .bind(&automatic_confirmation_cutoff)
        .execute(&mut **tx)
        .await
        .repository()?;

        sqlx::query(
            r#"UPDATE "AddonMediaListItemTombstone" AS "tombstone"
               SET "ReappearedAt" = ?, "ConfirmedAt" = NULL,
                   "LastReconcileGeneration" = ?
               WHERE "tombstone"."ReappearedAt" IS NULL
                 AND "tombstone"."LastReconcileGeneration" < ?
                 AND EXISTS (
                     SELECT 1
                     FROM "AddonMediaListItem" AS "item"
                     WHERE "item"."Id" = "tombstone"."AddonMediaListItemId"
                       AND "item"."AddonMediaListId" = ?
                       AND "item"."InDraft" = 1
                       AND "item"."LastReconcileGeneration" = ?
                 )"#,
        )
        .bind(&observed_at)
        .bind(generation)
        .bind(generation)
        .bind(list_id)
        .bind(generation)
        .execute(&mut **tx)
        .await
        .repository()?;
        Ok(())
    }

    pub(crate) async fn complete_import_projection(
        &self,
        job_id: EntityId,
        lease_owner: &str,
        list_id: EntityId,
        requested_folder: Option<&str>,
    ) -> Result<()> {
        let phase = self.import_finalization_phase(job_id, lease_owner).await?;
        if (3..=4).contains(&phase) {
            return Ok(());
        }
        if phase != 2 {
            return Err(RepositoryError::Conflict(
                "the import finalization phase is invalid for list projection".to_owned(),
            ));
        }
        let fence = ImportFinalizationFence {
            job_id,
            lease_owner,
            expected_phase: 2,
            next_phase: 3,
        };
        let mut renewal = crate::db::begin_write(&self.local.pool)
            .await
            .repository()?;
        begin_import_finalization_phase(&mut renewal, fence).await?;
        renewal.commit().await.repository()?;

        let list = self.local.load_list(list_id).await?;
        if has_media_source(&self.local.pool, &list.addon_id).await? {
            ensure_list_library(
                &self.local.pool,
                list.id,
                &list.addon_id,
                &list.name,
                &list.source_url,
                &list.chapter_input,
                requested_folder,
            )
            .await?;
        }
        let review_state: i32 =
            sqlx::query_scalar(r#"SELECT "ReviewState" FROM "AddonMediaList" WHERE "Id" = ?"#)
                .bind(list.id)
                .fetch_one(&self.local.pool)
                .await
                .repository()?;
        if review_state == 2 {
            self.local.match_list_items(list.id).await?;
        }

        let mut tx = crate::db::begin_write(&self.local.pool)
            .await
            .repository()?;
        begin_import_finalization_phase(&mut tx, fence).await?;
        complete_import_finalization_phase(&mut tx, fence).await?;
        tx.commit().await.repository()
    }

    /// Add or refresh an add-on media list, then match its persisted items.
    ///
    /// `requested_folder` names the storage folder for a list imported for the
    /// first time; an empty value derives the name from the list itself, and a
    /// list that already owns a folder keeps it.
    ///
    /// # Errors
    ///
    /// Returns an error when the requested folder is unusable or taken, or when
    /// provider invocation or persistence fails.
    pub async fn add_or_refresh(
        &self,
        addon_id: &str,
        source_url: &str,
        requested_folder: Option<&str>,
        content_kind_override: ersatzrs_domain::AddonMediaListContentKind,
        chapter_input: &str,
    ) -> Result<EntityId> {
        // Validate before enumerating so a rejected folder name costs no
        // provider call and leaves no half-imported list behind.
        let requested_folder = match requested_folder
            .map(str::trim)
            .filter(|folder| !folder.is_empty())
        {
            Some(folder) => {
                Some(validated_folder_name(&self.local.pool, addon_id, source_url, folder).await?)
            }
            None => None,
        };
        let mut enumerated = match self
            .provider
            .enumerate(addon_id, source_url, chapter_input)
            .await
        {
            Ok(enumerated) => enumerated,
            Err(error) => {
                sqlx::query(
                    r#"UPDATE "AddonMediaList" SET "LastError" = ?
                       WHERE "AddonId" = ? AND "SourceUrl" = ? AND "ReviewState" <> 3
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
                .bind(error.to_string())
                .bind(addon_id)
                .bind(source_url)
                .bind(encode_enum(
                    ersatzrs_domain::ManagedLibraryOperationState::Queued,
                ))
                .bind(encode_enum(
                    ersatzrs_domain::ManagedLibraryOperationState::Running,
                ))
                .bind(encode_enum(
                    ersatzrs_domain::ManagedLibraryOperationState::WaitingToRetry,
                ))
                .execute(&self.local.pool)
                .await
                .repository()?;
                return Err(error);
            }
        };
        let list = AddonMediaList {
            id: 0,
            addon_id: addon_id.to_owned(),
            source_url: source_url.to_owned(),
            provider_id: enumerated.provider_id.clone(),
            name: enumerated.name.clone(),
            description: enumerated.description.clone(),
            item_count: i32::try_from(enumerated.items.len()).unwrap_or(i32::MAX),
            refresh_cadence: ersatzrs_domain::AddonMediaListRefreshCadence::Never,
            generate_playlist: false,
            content_kind_override,
            presentation_kind: ersatzrs_domain::AddonMediaListPresentationKind::RemoteStreams,
            requested_presentation_kind:
                ersatzrs_domain::AddonMediaListPresentationKind::RemoteStreams,
            presentation_error: None,
            chapter_input: chapter_input.to_owned(),
            playlist_id: None,
            last_update: None,
            last_match: None,
            last_error: None,
        };
        self.persist_enumerated(&list, &mut enumerated, requested_folder.as_deref())
            .await
    }

    /// Persist one enumerated snapshot, then project and match it.
    ///
    /// Shared by import and by an add-on change so both paths agree on chapter
    /// expansion, managed-library projection, and matching.
    pub(crate) async fn persist_enumerated(
        &self,
        list: &AddonMediaList,
        enumerated: &mut EnumeratedAddonMediaList,
        requested_folder: Option<&str>,
    ) -> Result<EntityId> {
        let list_id = self.local.save_list(list, &enumerated.metadata).await?;
        if let Some(mode) = self.delivery_mode {
            let mut tx = crate::db::begin_write(&self.local.pool)
                .await
                .repository()?;
            ensure_not_publishing_tx(&mut tx, list_id).await?;
            crate::addon_media_list_delivery::save_in_tx(
                &mut tx,
                list_id,
                mode,
                chrono::Utc::now(),
            )
            .await?;
            tx.commit().await.repository()?;
        }
        if list.chapter_input.trim().is_empty() {
            self.expand_item_chapters(list_id, &list.addon_id, enumerated)
                .await?;
        }
        let list = self
            .local
            .save_list_items(list_id, &enumerated.items)
            .await?;
        if has_media_source(&self.local.pool, &list.addon_id).await? {
            ensure_list_library(
                &self.local.pool,
                list.id,
                &list.addon_id,
                &list.name,
                &list.source_url,
                &list.chapter_input,
                requested_folder,
            )
            .await?;
        }
        let review_state: i32 =
            sqlx::query_scalar(r#"SELECT "ReviewState" FROM "AddonMediaList" WHERE "Id" = ?"#)
                .bind(list.id)
                .fetch_one(&self.local.pool)
                .await
                .repository()?;
        if review_state == 2 {
            self.local.match_list_items(list.id).await?;
        }
        Ok(list.id)
    }

    /// Move one saved list to a different add-on and re-import it.
    ///
    /// List identity is `(AddonId, ProviderId)`, so the row is re-pointed at the
    /// new identity before the snapshot is stored; otherwise the refresh would
    /// insert a second list and orphan the original. Items are keyed per
    /// provider, so operator item metadata is re-derived rather than migrated.
    ///
    /// # Errors
    ///
    /// Returns an error when the list is unknown, the new add-on cannot
    /// enumerate the source, or another list already owns the resulting
    /// identity.
    pub async fn change_addon(&self, id: EntityId, addon_id: &str) -> Result<()> {
        ensure_not_publishing(&self.local.pool, id).await?;
        let row = sqlx::query(
            r#"SELECT "AddonId", "SourceUrl", "ContentKindOverride", "ChapterInput"
               FROM "AddonMediaList" WHERE "Id" = ?"#,
        )
        .bind(id)
        .fetch_optional(&self.local.pool)
        .await
        .repository()?
        .ok_or(RepositoryError::NotFound {
            kind: "add-on media list",
            id,
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
        let content_kind_override = crate::repositories::common::decode_enum_column(
            "AddonMediaListContentKind",
            &row,
            "ContentKindOverride",
        )?;

        // Enumerating first means a failing add-on leaves the existing list
        // untouched instead of half-moved.
        let mut enumerated = self
            .provider
            .enumerate(addon_id, &source_url, &chapter_input)
            .await?;
        let conflicting: Option<EntityId> = sqlx::query_scalar(
            r#"SELECT "Id" FROM "AddonMediaList"
               WHERE "AddonId" = ? AND "ProviderId" = ? AND "Id" <> ?"#,
        )
        .bind(addon_id)
        .bind(&enumerated.provider_id)
        .bind(id)
        .fetch_optional(&self.local.pool)
        .await
        .repository()?;
        if let Some(conflicting) = conflicting {
            return Err(RepositoryError::unavailable(
                "change add-on media list add-on",
                format!("list {conflicting} already uses this add-on for the same source"),
            ));
        }
        self.update_addon_identity(id, addon_id, &enumerated.provider_id)
            .await?;

        let list = AddonMediaList {
            id,
            addon_id: addon_id.to_owned(),
            source_url,
            provider_id: enumerated.provider_id.clone(),
            name: enumerated.name.clone(),
            description: enumerated.description.clone(),
            item_count: i32::try_from(enumerated.items.len()).unwrap_or(i32::MAX),
            refresh_cadence: ersatzrs_domain::AddonMediaListRefreshCadence::Never,
            generate_playlist: false,
            content_kind_override,
            presentation_kind: ersatzrs_domain::AddonMediaListPresentationKind::RemoteStreams,
            requested_presentation_kind:
                ersatzrs_domain::AddonMediaListPresentationKind::RemoteStreams,
            presentation_error: None,
            chapter_input,
            playlist_id: None,
            last_update: None,
            last_match: None,
            last_error: None,
        };
        self.persist_enumerated(&list, &mut enumerated, None)
            .await?;
        Ok(())
    }

    async fn update_addon_identity(
        &self,
        id: EntityId,
        addon_id: &str,
        provider_id: &str,
    ) -> Result<()> {
        let mut tx = crate::db::begin_write(&self.local.pool)
            .await
            .repository()?;
        let previous = sqlx::query(
            r#"SELECT "AddonId", "DraftRevision" FROM "AddonMediaList" WHERE "Id" = ?"#,
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .repository()?
        .ok_or(RepositoryError::NotFound {
            kind: "add-on media list",
            id,
        })?;
        let previous_addon_id: String = previous.try_get("AddonId").repository()?;
        let revision: i32 = previous.try_get("DraftRevision").repository()?;
        if self
            .editor_revision
            .is_some_and(|expected| expected != revision)
        {
            return Err(RepositoryError::Conflict(
                "the saved draft changed during provider enumeration".into(),
            ));
        }
        let updated = sqlx::query(
            r#"UPDATE "AddonMediaList" SET "AddonId" = ?, "ProviderId" = ?, "LastError" = NULL,
                      "DraftRevision" = "DraftRevision" + 1
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
                 )
                 AND NOT EXISTS (
                     SELECT 1 FROM "AddonMediaListImportJob" AS "import"
                     WHERE "import"."AddonMediaListId" = "AddonMediaList"."Id"
                       AND "import"."OverviewComplete" = 1
                       AND "import"."LeaseOwner" IS NOT NULL
                 )"#,
        )
        .bind(addon_id)
        .bind(provider_id)
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
            let finalizing: bool = sqlx::query_scalar(
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
            if finalizing {
                return Err(RepositoryError::Conflict(
                    "the add-on media list import is finalizing; wait for it to finish".to_owned(),
                ));
            }
            return Err(publication_in_progress());
        }
        reconcile_changed_addon_state_in_tx(&mut tx, id, &previous_addon_id, addon_id).await?;
        tx.commit().await.repository()
    }

    async fn expand_item_chapters(
        &self,
        list_id: EntityId,
        addon_id: &str,
        enumerated: &mut EnumeratedAddonMediaList,
    ) -> Result<()> {
        let requests = sqlx::query(
            r#"SELECT "ProviderId", "SourceUrl", "ChapterInput"
               FROM "AddonMediaListItem"
               WHERE "AddonMediaListId" = ? AND "InDraft" = 1 AND "ParentProviderId" IS NULL
                 AND COALESCE("ChapterInput", '') <> ''
               ORDER BY "Rank", "Id""#,
        )
        .bind(list_id)
        .fetch_all(&self.local.pool)
        .await
        .repository()?;
        let mut next_rank = enumerated
            .items
            .iter()
            .map(|item| item.rank)
            .max()
            .unwrap_or(-1)
            .saturating_add(1);
        for request in requests {
            let provider_id: String = request.try_get("ProviderId").repository()?;
            let stored_url: Option<String> = request.try_get("SourceUrl").repository()?;
            let input: String = request.try_get("ChapterInput").repository()?;
            let source_url = enumerated
                .items
                .iter()
                .find(|item| item.provider_id == provider_id)
                .and_then(|item| item.source_url.as_deref())
                .or(stored_url.as_deref())
                .ok_or_else(|| {
                    RepositoryError::Conflict(format!(
                        "item '{provider_id}' has chapters but no source URL"
                    ))
                })?
                .to_owned();
            let expanded = self
                .provider
                .enumerate(addon_id, &source_url, &input)
                .await?;
            for mut fragment in expanded
                .items
                .into_iter()
                .filter(|item| item.parent_provider_id.is_some())
            {
                fragment.rank = next_rank;
                next_rank = next_rank.saturating_add(1);
                enumerated.items.push(fragment);
            }
        }
        Ok(())
    }
}

pub(crate) async fn reconcile_changed_addon_state_in_tx(
    tx: &mut Transaction<'_, Db>,
    list_id: EntityId,
    previous_addon_id: &str,
    current_addon_id: &str,
) -> Result<()> {
    if previous_addon_id == current_addon_id {
        return Ok(());
    }
    sqlx::query(
        r#"UPDATE "AddonMediaList"
           SET "DraftOptionSchemaId" = NULL, "DraftOptionValuesJson" = '{}'
           WHERE "Id" = ?"#,
    )
    .bind(list_id)
    .execute(&mut **tx)
    .await
    .repository()?;
    sqlx::query(
        r#"UPDATE "AddonMediaListItem" SET "DraftOptionOverridesJson" = '{}'
           WHERE "AddonMediaListId" = ? AND "InDraft" = 1"#,
    )
    .bind(list_id)
    .execute(&mut **tx)
    .await
    .repository()?;
    sqlx::query(
        r#"UPDATE "AddonMediaListImportJob" SET "CheckpointId" = NULL
           WHERE "AddonMediaListId" = ?"#,
    )
    .bind(list_id)
    .execute(&mut **tx)
    .await
    .repository()?;
    sqlx::query(
        r#"UPDATE "AddonMediaListImportCheckpoint" SET "Committed" = 0
           WHERE "AddonMediaListId" = ?"#,
    )
    .bind(list_id)
    .execute(&mut **tx)
    .await
    .repository()?;
    sqlx::query(
        r#"DELETE FROM "AddonMediaListSourceChangeObservation"
           WHERE "AddonMediaListId" = ?"#,
    )
    .bind(list_id)
    .execute(&mut **tx)
    .await
    .repository()?;
    sqlx::query(r#"DELETE FROM "AddonMediaListRefreshState" WHERE "AddonMediaListId" = ?"#)
        .bind(list_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    crate::repositories::SqliteAddonMediaListRepository::initialize_option_defaults_in_tx(
        tx, list_id,
    )
    .await
}

pub(crate) async fn unlink_revision_artifacts_for_list_in_tx(
    tx: &mut Transaction<'_, Db>,
    list_id: EntityId,
) -> std::result::Result<(), sqlx::Error> {
    sqlx::query(
        r#"DELETE FROM "AddonMediaListRevisionArtifact"
           WHERE "AddonMediaListRevisionId" IN (
               SELECT "Id" FROM "AddonMediaListRevision" WHERE "AddonMediaListId" = ?
           )"#,
    )
    .bind(list_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn system_playlist_group_id(pool: &Pool<Db>) -> Result<EntityId> {
    sqlx::query_scalar::<_, EntityId>(
        r#"SELECT "Id" FROM "PlaylistGroup" WHERE "IsSystem" = 1 ORDER BY "Id" LIMIT 1"#,
    )
    .fetch_optional(pool)
    .await
    .repository()?
    .ok_or_else(|| {
        RepositoryError::unavailable(
            "load system playlist group",
            "system playlist group is not initialized",
        )
    })
}

async fn insert_item(
    tx: &mut sqlx::Transaction<'_, Db>,
    list_id: EntityId,
    item: &EnumeratedAddonMediaListItem,
    reconcile_generation: Option<i32>,
) -> Result<EntityId> {
    let result = sqlx::query(
        r#"INSERT INTO "AddonMediaListItem" (
            "ProviderId", "DisplayTitle", "Episode", "Kind", "MediaItemId",
            "Rank", "Season", "Title", "AddonMediaListId", "Year", "SourceUrl",
            "Availability", "AvailabilityReason", "AvailabilityReasonCode",
            "AvailabilityUpdated", "ContentKind",
            "CollectionRole", "DurationSeconds", "ParentProviderId", "FragmentStartSeconds",
            "FragmentEndSeconds", "AdditionalImageUrls", "ProviderMetadataJson",
            "MetadataOverrideJson", "ProviderLiveness", "EffectiveLiveness",
            "LastReconcileGeneration"
           ) VALUES (?, ?, ?, ?, NULL, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, '{}', ?, ?, ?)"#,
    )
    .bind(&item.provider_id)
    .bind(&item.display_title)
    .bind(item.episode)
    .bind(encode_enum(item.kind))
    .bind(item.rank)
    .bind(item.season)
    .bind(&item.title)
    .bind(list_id)
    .bind(item.year)
    .bind(&item.source_url)
    .bind(encode_enum(item.availability))
    .bind(&item.availability_reason)
    .bind(item.availability_reason_code.map(encode_enum))
    .bind(encode_datetime(Utc::now()))
    .bind(encode_enum(item.content_kind))
    .bind(encode_enum(item.collection_role))
    .bind(optional_fragment_bound(item.duration_seconds, "duration")?)
    .bind(&item.parent_provider_id)
    .bind(optional_fragment_bound(
        item.fragment_start_seconds,
        "fragment start",
    )?)
    .bind(optional_fragment_bound(
        item.fragment_end_seconds,
        "fragment end",
    )?)
    .bind(encode_image_urls(&item.additional_image_urls)?)
    .bind(encode_metadata(&item.metadata)?)
    .bind(encode_enum(item.liveness))
    .bind(encode_enum(item.liveness))
    .bind(reconcile_generation)
    .execute(&mut **tx)
    .await
    .repository()?;
    inserted_id(tx, result).await.repository()
}

/// Reuse retained identity without importing its historic draft choices.
/// Published fields, local published bindings and immutable revision/artifact
/// records remain attached to the same item ID.
async fn readopt_draft_item(
    tx: &mut Transaction<'_, Db>,
    list_id: EntityId,
    item_id: EntityId,
    role: ersatzrs_domain::AddonMediaListItemRole,
) -> Result<()> {
    sqlx::query(
        r#"UPDATE "AddonMediaListItem" SET "Included"=1,
        "MetadataOverrideJson"='{}', "MetadataConflictJson"=NULL,
        "DraftOptionOverridesJson"='{}', "LivenessOverride"=NULL, "LivenessConflict"=0,
        "ChapterInput"=NULL, "ProviderChapterInput"=NULL, "CollectionRole"=?,
        "LastReconcileGeneration"=NULL, "SavedReplacementJson"=NULL WHERE "Id"=? AND "InDraft"=0"#,
    )
    .bind(encode_enum(role))
    .bind(item_id)
    .execute(&mut **tx)
    .await
    .repository()?;
    for statement in [
        r#"DELETE FROM "AddonMediaListItemReplacement" WHERE "AddonMediaListItemId"=?"#,
        r#"DELETE FROM "AddonMediaListItemTombstone" WHERE "AddonMediaListItemId"=?"#,
    ] {
        sqlx::query(statement)
            .bind(item_id)
            .execute(&mut **tx)
            .await
            .repository()?;
    }
    remove_retired_draft_jobs(tx, list_id, item_id).await?;
    Ok(())
}

/// Remove obsolete detail state and cancel active draft operations while
/// preserving process recovery evidence, published owners and artifact pins.
async fn remove_retired_draft_jobs(
    tx: &mut Transaction<'_, Db>,
    list_id: EntityId,
    item_id: EntityId,
) -> Result<()> {
    sqlx::query(r#"DELETE FROM "AddonMediaListDetailJob" WHERE "AddonMediaListItemId"=?"#)
        .bind(item_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    crate::addon_media_list_item_jobs::cancel_item_jobs_in_tx(tx, list_id, item_id, Utc::now())
        .await
}

async fn insert_item_guids(
    tx: &mut sqlx::Transaction<'_, Db>,
    item_id: EntityId,
    guids: &[String],
) -> Result<()> {
    for guid in guids {
        sqlx::query(r#"INSERT INTO "AddonMediaListItemGuid" ("Guid", "AddonMediaListItemId") VALUES (?, ?)"#)
            .bind(guid)
            .bind(item_id)
            .execute(&mut **tx)
            .await
            .repository()?;
    }
    Ok(())
}

#[derive(Debug)]
struct ExistingItem {
    id: EntityId,
    in_draft: bool,
    published: bool,
    provider_id: String,
    rank: i32,
    parent_provider_id: Option<String>,
    provider_metadata_json: String,
    metadata_override_json: String,
    metadata_conflict_json: Option<String>,
    provider_liveness: RemoteStreamLiveness,
    liveness_override: Option<RemoteStreamLiveness>,
    liveness_conflict: bool,
}

pub(crate) async fn ensure_not_publishing(pool: &Pool<Db>, list_id: EntityId) -> Result<()> {
    let has_live_lease = sqlx::query_scalar::<_, bool>(
        r#"SELECT EXISTS (
               SELECT 1
               FROM "AddonMediaListPublicationJob" AS "job"
               JOIN "ManagedLibraryOperation" AS "operation"
                 ON "operation"."Id" = "job"."OperationId"
               JOIN "ManagedLibraryLease" AS "lease"
                 ON "lease"."OperationId" = "operation"."Id"
                AND "lease"."OwnerToken" = "operation"."OwnerToken"
               WHERE "job"."AddonMediaListId" = "AddonMediaList"."Id"
                 AND "operation"."State" = ?
                 AND "lease"."LeaseExpiresAt" > ?
           )
           FROM "AddonMediaList" WHERE "Id" = ?"#,
    )
    .bind(encode_enum(
        ersatzrs_domain::ManagedLibraryOperationState::Running,
    ))
    .bind(crate::repositories::common::encode_datetime(
        chrono::Utc::now(),
    ))
    .bind(list_id)
    .fetch_optional(pool)
    .await
    .repository()?
    .ok_or(RepositoryError::NotFound {
        kind: "add-on media list",
        id: list_id,
    })?;
    if has_live_lease {
        return Err(publication_in_progress());
    }
    Ok(())
}

pub(crate) async fn ensure_not_import_finalizing(pool: &Pool<Db>, list_id: EntityId) -> Result<()> {
    let finalizing = sqlx::query_scalar::<_, bool>(
        r#"SELECT EXISTS (
               SELECT 1 FROM "AddonMediaListImportJob"
               WHERE "AddonMediaListId" = ? AND "OverviewComplete" = 1
                 AND "LeaseOwner" IS NOT NULL
           )"#,
    )
    .bind(list_id)
    .fetch_one(pool)
    .await
    .repository()?;
    if finalizing {
        return Err(RepositoryError::Conflict(
            "the add-on media list import is finalizing; wait for it to finish".to_owned(),
        ));
    }
    Ok(())
}

async fn ensure_not_import_finalizing_tx(
    tx: &mut Transaction<'_, Db>,
    list_id: EntityId,
) -> Result<()> {
    let finalizing = sqlx::query_scalar::<_, bool>(
        r#"SELECT EXISTS (
               SELECT 1 FROM "AddonMediaListImportJob"
               WHERE "AddonMediaListId" = ? AND "OverviewComplete" = 1
                 AND "LeaseOwner" IS NOT NULL
           )"#,
    )
    .bind(list_id)
    .fetch_one(&mut **tx)
    .await
    .repository()?;
    if finalizing {
        return Err(RepositoryError::Conflict(
            "the add-on media list import is finalizing; wait for it to finish".to_owned(),
        ));
    }
    Ok(())
}

pub(crate) async fn ensure_not_publishing_tx(
    tx: &mut Transaction<'_, Db>,
    list_id: EntityId,
) -> Result<()> {
    let has_live_lease = sqlx::query_scalar::<_, bool>(
        r#"SELECT EXISTS (
               SELECT 1
               FROM "AddonMediaListPublicationJob" AS "job"
               JOIN "ManagedLibraryOperation" AS "operation"
                 ON "operation"."Id" = "job"."OperationId"
               JOIN "ManagedLibraryLease" AS "lease"
                 ON "lease"."OperationId" = "operation"."Id"
                AND "lease"."OwnerToken" = "operation"."OwnerToken"
               WHERE "job"."AddonMediaListId" = "AddonMediaList"."Id"
                 AND "operation"."State" = ?
                 AND "lease"."LeaseExpiresAt" > ?
           )
           FROM "AddonMediaList" WHERE "Id" = ?"#,
    )
    .bind(encode_enum(
        ersatzrs_domain::ManagedLibraryOperationState::Running,
    ))
    .bind(crate::repositories::common::encode_datetime(
        chrono::Utc::now(),
    ))
    .bind(list_id)
    .fetch_optional(&mut **tx)
    .await
    .repository()?
    .ok_or(RepositoryError::NotFound {
        kind: "add-on media list",
        id: list_id,
    })?;
    if has_live_lease {
        return Err(publication_in_progress());
    }
    Ok(())
}

fn publication_in_progress() -> RepositoryError {
    RepositoryError::Conflict(
        "the add-on media list is being published; wait for publication to finish".to_owned(),
    )
}

fn refresh_conflict_json(
    previous_provider_json: &str,
    next_provider_json: &str,
    override_json: &str,
    existing_conflict_json: Option<&str>,
) -> Result<Option<String>> {
    let previous_provider: serde_json::Value = serde_json::from_str(previous_provider_json)
        .map_err(|error| RepositoryError::Decode {
            kind: "stored add-on provider metadata",
            value: error.to_string(),
        })?;
    let next_provider: serde_json::Value =
        serde_json::from_str(next_provider_json).map_err(|error| RepositoryError::Decode {
            kind: "refreshed add-on provider metadata",
            value: error.to_string(),
        })?;
    let overrides: serde_json::Value =
        serde_json::from_str(override_json).map_err(|error| RepositoryError::Decode {
            kind: "stored add-on metadata override",
            value: error.to_string(),
        })?;
    let mut conflicts = existing_conflict_json
        .filter(|value| !value.trim().is_empty())
        .map(
            serde_json::from_str::<
                std::collections::BTreeMap<String, AddonMediaListMetadataConflict>,
            >,
        )
        .transpose()
        .map_err(|error| RepositoryError::Decode {
            kind: "stored add-on metadata conflicts",
            value: error.to_string(),
        })?
        .unwrap_or_default();
    let Some(override_fields) = overrides.as_object() else {
        return Ok(None);
    };
    for (field, operator_value) in override_fields {
        let before = previous_provider
            .get(field)
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let after = next_provider
            .get(field)
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        if after == *operator_value {
            conflicts.remove(field);
        } else if before != after {
            conflicts.insert(
                field.clone(),
                AddonMediaListMetadataConflict {
                    provider_before: before,
                    provider_after: after,
                    operator_value: operator_value.clone(),
                },
            );
        } else if let Some(conflict) = conflicts.get_mut(field) {
            conflict.provider_after = after;
            conflict.operator_value.clone_from(operator_value);
        }
    }
    conflicts.retain(|field, _| override_fields.contains_key(field));
    if conflicts.is_empty() {
        Ok(None)
    } else {
        serde_json::to_string(&conflicts)
            .map(Some)
            .map_err(|error| RepositoryError::Decode {
                kind: "add-on metadata conflicts",
                value: error.to_string(),
            })
    }
}

fn refresh_liveness(
    previous_provider: RemoteStreamLiveness,
    next_provider: RemoteStreamLiveness,
    operator_override: Option<RemoteStreamLiveness>,
    existing_conflict: bool,
) -> (RemoteStreamLiveness, bool) {
    let Some(operator_value) = operator_override else {
        return (next_provider, false);
    };
    let conflict = if next_provider == operator_value {
        false
    } else if previous_provider != next_provider {
        true
    } else {
        existing_conflict
    };
    (operator_value, conflict)
}

fn encode_image_urls(urls: &[String]) -> Result<Option<String>> {
    if urls.is_empty() {
        return Ok(None);
    }
    serde_json::to_string(urls)
        .map(Some)
        .map_err(|error| RepositoryError::Decode {
            kind: "add-on media list image URLs",
            value: error.to_string(),
        })
}

fn encode_metadata(
    metadata: &ersatzrs_core::repositories::AddonMediaListNfoMetadata,
) -> Result<String> {
    serde_json::to_string(metadata).map_err(|error| RepositoryError::Decode {
        kind: "add-on metadata document",
        value: error.to_string(),
    })
}

fn merge_present_metadata(
    previous_json: &str,
    incoming: &ersatzrs_core::repositories::AddonMediaListNfoMetadata,
) -> Result<String> {
    let mut merged: serde_json::Value =
        serde_json::from_str(previous_json).map_err(|error| RepositoryError::Decode {
            kind: "stored add-on provider metadata",
            value: error.to_string(),
        })?;
    let previous = merged.clone();
    let incoming = serde_json::to_value(incoming).map_err(|error| RepositoryError::Decode {
        kind: "refreshed add-on provider metadata",
        value: error.to_string(),
    })?;
    let merged_fields = merged
        .as_object_mut()
        .ok_or_else(|| RepositoryError::Decode {
            kind: "stored add-on provider metadata",
            value: "expected an object".to_owned(),
        })?;
    let incoming_fields = incoming
        .as_object()
        .ok_or_else(|| RepositoryError::Decode {
            kind: "refreshed add-on provider metadata",
            value: "expected an object".to_owned(),
        })?;
    for (field, value) in incoming_fields {
        let present = match value {
            serde_json::Value::Null => false,
            serde_json::Value::String(value) => !value.trim().is_empty(),
            serde_json::Value::Array(value) => !value.is_empty(),
            serde_json::Value::Object(value) => !value.is_empty(),
            serde_json::Value::Bool(_) | serde_json::Value::Number(_) => true,
        };
        if present {
            merged_fields.insert(field.clone(), value.clone());
        }
    }
    if merged == previous {
        return Ok(previous_json.to_owned());
    }
    serde_json::to_string(&merged).map_err(|error| RepositoryError::Decode {
        kind: "merged add-on provider metadata",
        value: error.to_string(),
    })
}

fn fragment_bound(value: Option<u64>, kind: &'static str) -> Result<i64> {
    optional_fragment_bound(value, kind)?.ok_or_else(|| {
        RepositoryError::Conflict(format!("{kind} is required for a chapter fragment"))
    })
}

fn optional_fragment_bound(value: Option<u64>, kind: &'static str) -> Result<Option<i64>> {
    value
        .map(|value| {
            i64::try_from(value).map_err(|_| RepositoryError::Decode {
                kind,
                value: value.to_string(),
            })
        })
        .transpose()
}

fn decode_optional_seconds(row: &crate::db::DbRow, column: &'static str) -> Result<Option<u64>> {
    row.try_get::<Option<i64>, _>(column)
        .repository()?
        .map(|value| {
            u64::try_from(value).map_err(|_| RepositoryError::Decode {
                kind: column,
                value: value.to_string(),
            })
        })
        .transpose()
}

#[derive(Debug)]
struct GeneratedPlaylistRefresh {
    library_id: Option<EntityId>,
    generation: Option<i64>,
    reason: ScanPlayoutRefreshReason,
    playout_ids: Vec<EntityId>,
}

impl GeneratedPlaylistRefresh {
    fn playout_refreshes(&self) -> Vec<ScanPlayoutRefresh> {
        self.playout_ids
            .iter()
            .copied()
            .map(|playout_id| ScanPlayoutRefresh {
                playout_id,
                reason: self.reason,
            })
            .collect()
    }
}

#[derive(Debug)]
pub(crate) struct GeneratedPlaylistRemoval {
    playlist_id: EntityId,
    refresh: GeneratedPlaylistRefresh,
}

async fn persist_generated_playlist_refresh_in_tx(
    tx: &mut Transaction<'_, Db>,
    refresh: &GeneratedPlaylistRefresh,
    library_id: Option<EntityId>,
) -> Result<()> {
    if let Some(library_id) = library_id {
        crate::scan_refresh::persist_refresh_intents(
            tx,
            library_id,
            refresh.reason,
            &refresh.playout_ids,
        )
        .await
        .repository()?;
    } else {
        crate::scan_refresh::persist_unowned_refresh_intents(
            tx,
            refresh.reason,
            &refresh.playout_ids,
        )
        .await
        .repository()?;
    }
    Ok(())
}

async fn complete_generated_playlist_resolution_in_tx(
    tx: &mut Transaction<'_, Db>,
    refresh: &GeneratedPlaylistRefresh,
) -> Result<()> {
    if let (Some(library_id), Some(generation)) = (refresh.library_id, refresh.generation) {
        crate::scan_refresh::complete_resolution_in_tx(tx, library_id, generation, refresh.reason)
            .await
            .repository()?;
    }
    Ok(())
}

/// Canonical generated-row shape. The logical key is the complete row except
/// for the database-owned id; cardinality is exactly one entry per desired
/// published list item, ordered by `(Index, insertion order)`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct GeneratedPlaylistItem {
    index: i32,
    collection_type: i32,
    collection_id: Option<EntityId>,
    media_item_id: Option<EntityId>,
    multi_collection_id: Option<EntityId>,
    smart_collection_id: Option<EntityId>,
    include_in_program_guide: bool,
    playback_order: i32,
    play_all: bool,
    count: Option<i32>,
    preferred_audio_language_code: Option<String>,
    preferred_audio_title: Option<String>,
    preferred_subtitle_language_code: Option<String>,
    subtitle_mode: Option<i32>,
    media_version_id: Option<EntityId>,
    video_stream_index: Option<i32>,
    audio_stream_index: Option<i32>,
    subtitle_stream_index: Option<i32>,
}

impl GeneratedPlaylistItem {
    fn new(item: &MatchItem, selection: PlaylistSelection) -> Self {
        Self {
            index: item.rank,
            collection_type: selection
                .collection_type
                .unwrap_or_else(|| collection_type_for_kind(item.kind)),
            collection_id: None,
            media_item_id: Some(selection.media_item_id),
            multi_collection_id: None,
            smart_collection_id: None,
            include_in_program_guide: true,
            playback_order: PLAYBACK_ORDER_CHRONOLOGICAL,
            play_all: false,
            count: None,
            preferred_audio_language_code: None,
            preferred_audio_title: None,
            preferred_subtitle_language_code: None,
            subtitle_mode: None,
            media_version_id: selection.media_version_id,
            video_stream_index: selection.video_stream_index,
            audio_stream_index: None,
            subtitle_stream_index: None,
        }
    }

    fn content(&self) -> GeneratedPlaylistItemContent {
        GeneratedPlaylistItemContent {
            collection_type: self.collection_type,
            collection_id: self.collection_id,
            media_item_id: self.media_item_id,
            multi_collection_id: self.multi_collection_id,
            smart_collection_id: self.smart_collection_id,
            include_in_program_guide: self.include_in_program_guide,
            playback_order: self.playback_order,
            play_all: self.play_all,
            count: self.count,
            preferred_audio_language_code: self.preferred_audio_language_code.clone(),
            preferred_audio_title: self.preferred_audio_title.clone(),
            preferred_subtitle_language_code: self.preferred_subtitle_language_code.clone(),
            subtitle_mode: self.subtitle_mode,
            media_version_id: self.media_version_id,
            video_stream_index: self.video_stream_index,
            audio_stream_index: self.audio_stream_index,
            subtitle_stream_index: self.subtitle_stream_index,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct GeneratedPlaylistItemContent {
    collection_type: i32,
    collection_id: Option<EntityId>,
    media_item_id: Option<EntityId>,
    multi_collection_id: Option<EntityId>,
    smart_collection_id: Option<EntityId>,
    include_in_program_guide: bool,
    playback_order: i32,
    play_all: bool,
    count: Option<i32>,
    preferred_audio_language_code: Option<String>,
    preferred_audio_title: Option<String>,
    preferred_subtitle_language_code: Option<String>,
    subtitle_mode: Option<i32>,
    media_version_id: Option<EntityId>,
    video_stream_index: Option<i32>,
    audio_stream_index: Option<i32>,
    subtitle_stream_index: Option<i32>,
}

const GENERATED_PLAYLIST_ITEMS_SQL: &str = r#"SELECT "Index", "CollectionType", "CollectionId", "MediaItemId",
              "MultiCollectionId", "SmartCollectionId", "IncludeInProgramGuide",
              "PlaybackOrder", "PlayAll", "Count", "PreferredAudioLanguageCode",
              "PreferredAudioTitle", "PreferredSubtitleLanguageCode", "SubtitleMode",
              "MediaVersionId", "VideoStreamIndex", "AudioStreamIndex",
              "SubtitleStreamIndex"
       FROM "PlaylistItem" WHERE "PlaylistId" = ? ORDER BY "Index", "Id""#;

async fn load_generated_playlist_items(
    pool: &Pool<Db>,
    playlist_id: EntityId,
) -> Result<Vec<GeneratedPlaylistItem>> {
    let rows = sqlx::query(GENERATED_PLAYLIST_ITEMS_SQL)
        .bind(playlist_id)
        .fetch_all(pool)
        .await
        .repository()?;
    rows.iter().map(decode_generated_playlist_item).collect()
}

async fn load_generated_playlist_items_in_tx(
    tx: &mut Transaction<'_, Db>,
    playlist_id: EntityId,
) -> Result<Vec<GeneratedPlaylistItem>> {
    let rows = sqlx::query(GENERATED_PLAYLIST_ITEMS_SQL)
        .bind(playlist_id)
        .fetch_all(&mut **tx)
        .await
        .repository()?;
    rows.iter().map(decode_generated_playlist_item).collect()
}

fn decode_generated_playlist_item(row: &crate::db::DbRow) -> Result<GeneratedPlaylistItem> {
    Ok(GeneratedPlaylistItem {
        index: row.try_get("Index").repository()?,
        collection_type: row.try_get("CollectionType").repository()?,
        collection_id: row.try_get("CollectionId").repository()?,
        media_item_id: row.try_get("MediaItemId").repository()?,
        multi_collection_id: row.try_get("MultiCollectionId").repository()?,
        smart_collection_id: row.try_get("SmartCollectionId").repository()?,
        include_in_program_guide: decode_bool_column(row, "IncludeInProgramGuide")?,
        playback_order: row.try_get("PlaybackOrder").repository()?,
        play_all: decode_bool_column(row, "PlayAll")?,
        count: row.try_get("Count").repository()?,
        preferred_audio_language_code: row.try_get("PreferredAudioLanguageCode").repository()?,
        preferred_audio_title: row.try_get("PreferredAudioTitle").repository()?,
        preferred_subtitle_language_code: row
            .try_get("PreferredSubtitleLanguageCode")
            .repository()?,
        subtitle_mode: row.try_get("SubtitleMode").repository()?,
        media_version_id: row.try_get("MediaVersionId").repository()?,
        video_stream_index: row.try_get("VideoStreamIndex").repository()?,
        audio_stream_index: row.try_get("AudioStreamIndex").repository()?,
        subtitle_stream_index: row.try_get("SubtitleStreamIndex").repository()?,
    })
}

async fn replace_generated_playlist_items(
    tx: &mut Transaction<'_, Db>,
    playlist_id: EntityId,
    desired: &[GeneratedPlaylistItem],
) -> Result<()> {
    sqlx::query(r#"DELETE FROM "PlaylistItem" WHERE "PlaylistId" = ?"#)
        .bind(playlist_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    for item in desired {
        sqlx::query(
            r#"INSERT INTO "PlaylistItem" (
                   "Index", "PlaylistId", "CollectionType", "CollectionId",
                   "MediaItemId", "MultiCollectionId", "SmartCollectionId",
                   "IncludeInProgramGuide", "PlaybackOrder", "PlayAll", "Count",
                   "PreferredAudioLanguageCode", "PreferredAudioTitle",
                   "PreferredSubtitleLanguageCode", "SubtitleMode", "MediaVersionId",
                   "VideoStreamIndex", "AudioStreamIndex", "SubtitleStreamIndex"
               ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(item.index)
        .bind(playlist_id)
        .bind(item.collection_type)
        .bind(item.collection_id)
        .bind(item.media_item_id)
        .bind(item.multi_collection_id)
        .bind(item.smart_collection_id)
        .bind(item.include_in_program_guide)
        .bind(item.playback_order)
        .bind(item.play_all)
        .bind(item.count)
        .bind(&item.preferred_audio_language_code)
        .bind(&item.preferred_audio_title)
        .bind(&item.preferred_subtitle_language_code)
        .bind(item.subtitle_mode)
        .bind(item.media_version_id)
        .bind(item.video_stream_index)
        .bind(item.audio_stream_index)
        .bind(item.subtitle_stream_index)
        .execute(&mut **tx)
        .await
        .repository()?;
    }
    Ok(())
}

fn generated_playlist_change_reason(
    persisted: &[GeneratedPlaylistItem],
    desired: &[GeneratedPlaylistItem],
) -> ScanPlayoutRefreshReason {
    let persisted_counts = generated_playlist_content_counts(persisted);
    let desired_counts = generated_playlist_content_counts(desired);
    if persisted.len() == desired.len() {
        return if persisted_counts == desired_counts {
            ScanPlayoutRefreshReason::RemoteStreamMembershipReordered
        } else {
            ScanPlayoutRefreshReason::RemoteStreamMembershipReplaced
        };
    }
    if content_counts_are_subset(&persisted_counts, &desired_counts) {
        ScanPlayoutRefreshReason::RemoteStreamMembershipAdded
    } else if content_counts_are_subset(&desired_counts, &persisted_counts) {
        ScanPlayoutRefreshReason::RemoteStreamMembershipRemoved
    } else {
        ScanPlayoutRefreshReason::RemoteStreamMembershipMixed
    }
}

fn generated_playlist_content_counts(
    items: &[GeneratedPlaylistItem],
) -> BTreeMap<GeneratedPlaylistItemContent, usize> {
    let mut counts = BTreeMap::new();
    for item in items {
        *counts.entry(item.content()).or_default() += 1;
    }
    counts
}

fn content_counts_are_subset(
    candidate: &BTreeMap<GeneratedPlaylistItemContent, usize>,
    superset: &BTreeMap<GeneratedPlaylistItemContent, usize>,
) -> bool {
    candidate
        .iter()
        .all(|(item, count)| superset.get(item).is_some_and(|other| other >= count))
}

#[derive(Debug)]
struct MatchItem {
    id: EntityId,
    addon_id: String,
    provider_id: String,
    kind: AddonMediaListItemKind,
    title: String,
    year: Option<i32>,
    season: Option<i32>,
    episode: Option<i32>,
    rank: i32,
    guids: Vec<String>,
    source_url: Option<String>,
    availability: AddonMediaListItemAvailability,
    duration_seconds: Option<u64>,
    liveness: RemoteStreamLiveness,
}

#[derive(Clone, Copy, Debug)]
struct PlaylistSelection {
    media_item_id: EntityId,
    media_version_id: Option<EntityId>,
    video_stream_index: Option<i32>,
    collection_type: Option<i32>,
}

impl PlaylistSelection {
    const fn provider(media_item_id: EntityId) -> Self {
        Self {
            media_item_id,
            media_version_id: None,
            video_stream_index: None,
            collection_type: None,
        }
    }
}

const fn collection_type_for_kind(kind: AddonMediaListItemKind) -> i32 {
    match kind {
        AddonMediaListItemKind::Movie => COLLECTION_TYPE_MOVIE,
        AddonMediaListItemKind::Show => COLLECTION_TYPE_SHOW,
        AddonMediaListItemKind::Season => COLLECTION_TYPE_SEASON,
        AddonMediaListItemKind::Episode => COLLECTION_TYPE_EPISODE,
        AddonMediaListItemKind::RemoteStream => COLLECTION_TYPE_REMOTE_STREAM,
    }
}

#[cfg(test)]
mod reconcile_tests {
    mod in_draft_tests;
    use ersatzrs_core::repositories::AddonMediaListLifecycleRepository as _;
    use ersatzrs_core::repositories::AddonMediaListNfoMetadata;
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::{Executor as _, Row as _};

    use super::*;

    struct UnusedProvider;

    #[async_trait::async_trait]
    impl AddonMediaListProvider for UnusedProvider {
        async fn enumerate(
            &self,
            _addon_id: &str,
            _source_url: &str,
            _chapter_input: &str,
        ) -> ersatzrs_core::repositories::Result<EnumeratedAddonMediaList> {
            unreachable!("the evidence fixture does not invoke a provider")
        }
    }

    async fn finalize_evidence(
        synchronizer: &AddonMediaListRemoteSynchronizer<UnusedProvider>,
        pool: &DbPool,
        generation: i32,
    ) {
        let mut tx = crate::db::begin_write(pool)
            .await
            .expect("begin evidence transaction");
        synchronizer
            .finalize_reconcile_evidence_in_tx(&mut tx, 10, generation, Utc::now())
            .await
            .expect("finalize reconcile evidence");
        tx.commit().await.expect("commit reconcile evidence");
    }

    fn source_item(title: &str, rank: i32) -> EnumeratedAddonMediaListItem {
        EnumeratedAddonMediaListItem {
            provider_id: "provider-item".to_owned(),
            rank,
            display_title: title.to_owned(),
            title: title.to_owned(),
            year: Some(2026),
            season: Some(1),
            episode: Some(1),
            kind: AddonMediaListItemKind::RemoteStream,
            guids: vec!["fixture:provider-item".to_owned()],
            source_url: Some("https://media.example.test/item".to_owned()),
            availability: AddonMediaListItemAvailability::Available,
            availability_reason: None,
            availability_reason_code: None,
            content_kind: ersatzrs_domain::AddonMediaListContentKind::Movie,
            collection_role: ersatzrs_domain::AddonMediaListItemRole::Auto,
            duration_seconds: Some(600),
            liveness: RemoteStreamLiveness::Finite,
            parent_provider_id: None,
            fragment_start_seconds: None,
            fragment_end_seconds: None,
            additional_image_urls: Vec::new(),
            metadata: AddonMediaListNfoMetadata::default(),
        }
    }

    fn chapter_fragment(rank: i32) -> EnumeratedAddonMediaListItem {
        EnumeratedAddonMediaListItem {
            provider_id: "provider-item/chapter/0".to_owned(),
            rank,
            display_title: "Chapter one".to_owned(),
            title: "Chapter one".to_owned(),
            year: Some(2026),
            season: Some(1),
            episode: Some(1),
            kind: AddonMediaListItemKind::RemoteStream,
            guids: Vec::new(),
            source_url: Some("https://media.example.test/item".to_owned()),
            availability: AddonMediaListItemAvailability::Available,
            availability_reason: None,
            availability_reason_code: None,
            content_kind: ersatzrs_domain::AddonMediaListContentKind::Movie,
            collection_role: ersatzrs_domain::AddonMediaListItemRole::Auto,
            duration_seconds: Some(300),
            liveness: RemoteStreamLiveness::Finite,
            parent_provider_id: Some("provider-item".to_owned()),
            fragment_start_seconds: Some(0),
            fragment_end_seconds: Some(300),
            additional_image_urls: Vec::new(),
            metadata: AddonMediaListNfoMetadata::default(),
        }
    }

    async fn fixture() -> DbPool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect SQLite fixture");
        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .expect("migrate SQLite fixture");
        pool.execute("PRAGMA foreign_keys = OFF")
            .await
            .expect("disable fixture foreign keys");
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" (
                   "Id", "AddonId", "ProviderId", "SourceUrl", "Name", "ItemCount",
                   "ReviewState", "DraftRevision", "PublishedRevision"
               ) VALUES (10, 'fixture.addon', 'fixture-list',
                         'https://media.example.test/list', 'Fixture list', 1, ?, 4, 2)"#,
        )
        .bind(encode_enum(
            ersatzrs_domain::AddonMediaListReviewState::Published,
        ))
        .execute(&pool)
        .await
        .expect("seed list");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListItem" (
                   "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind",
                   "MediaItemId", "Rank", "Title", "Season", "Episode", "SourceUrl",
                   "Availability", "ContentKind", "Included", "Published",
                   "MetadataOverrideJson", "LivenessOverride", "EffectiveLiveness",
                   "DraftOptionOverridesJson", "CollectionRole"
               ) VALUES (
                   20, 10, 'provider-item', 'Operator item', ?, 321, 0, 'Operator item',
                   7, 8, 'https://media.example.test/old', ?, ?, 0, 1,
                   '{"title":"Operator title"}', ?, ?, '{"quality":"high"}', ?
               )"#,
        )
        .bind(encode_enum(AddonMediaListItemKind::RemoteStream))
        .bind(encode_enum(AddonMediaListItemAvailability::Available))
        .bind(encode_enum(
            ersatzrs_domain::AddonMediaListContentKind::TelevisionEpisode,
        ))
        .bind(encode_enum(RemoteStreamLiveness::Live))
        .bind(encode_enum(RemoteStreamLiveness::Live))
        .bind(encode_enum(
            ersatzrs_domain::AddonMediaListItemRole::Primary,
        ))
        .execute(&pool)
        .await
        .expect("seed operator-reviewed item");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListItemReplacement" (
                   "AddonMediaListItemId", "MediaItemId", "MediaVersionId", "CreatedAt", "UpdatedAt"
               ) VALUES (20, 400, 401, '2026-09-04T10:00:00Z', '2026-09-04T10:00:00Z')"#,
        )
        .execute(&pool)
        .await
        .expect("seed replacement");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListArtifact" (
                   "Id", "AddonMediaListId", "AddonMediaListItemId", "CacheKey", "Kind",
                   "State", "CreatedAt", "UpdatedAt"
               ) VALUES (30, 10, 20, 'fixture-artifact', ?, ?,
                         '2026-09-04T10:00:00Z', '2026-09-04T10:00:00Z')"#,
        )
        .bind(encode_enum(
            ersatzrs_domain::AddonMediaListArtifactKind::ProcessedMedia,
        ))
        .bind(encode_enum(
            ersatzrs_domain::AddonMediaListArtifactState::Ready,
        ))
        .execute(&pool)
        .await
        .expect("seed item artifact");
        pool
    }

    #[tokio::test]
    async fn unresolved_replacement_survives_merge_but_not_readoption() {
        let pool = fixture().await;
        sqlx::query(
            r#"DELETE FROM "AddonMediaListItemReplacement" WHERE "AddonMediaListItemId"=20"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(r#"UPDATE "AddonMediaListItem" SET "SavedReplacementJson"='{"file":null}' WHERE "Id"=20"#)
            .execute(&pool).await.unwrap();
        let synchronizer = AddonMediaListSynchronizer::new(pool.clone());
        synchronizer
            .save_list_items_with_mode(10, &[source_item("Updated", 0)], false, None, None)
            .await
            .unwrap();
        let preserved: Option<String> = sqlx::query_scalar(
            r#"SELECT "SavedReplacementJson" FROM "AddonMediaListItem" WHERE "Id"=20"#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(preserved.as_deref(), Some(r#"{"file":null}"#));
        sqlx::query(r#"UPDATE "AddonMediaListItem" SET "InDraft"=0 WHERE "Id"=20"#)
            .execute(&pool)
            .await
            .unwrap();
        synchronizer
            .save_list_items_with_mode(10, &[source_item("Readopted", 0)], false, None, None)
            .await
            .unwrap();
        let cleared: Option<String> = sqlx::query_scalar(
            r#"SELECT "SavedReplacementJson" FROM "AddonMediaListItem" WHERE "Id"=20"#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(cleared.is_none());
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)] // Keep successive observations and operator-state preservation in one scenario.
    async fn reconciliation_requires_two_observations_and_preserves_operator_state() {
        let pool = fixture().await;
        let synchronizer = AddonMediaListSynchronizer::new(pool.clone());
        let evidence = AddonMediaListRemoteSynchronizer::new(pool.clone(), UnusedProvider);

        synchronizer
            .save_list_items_with_mode(10, &[], false, Some(5), None)
            .await
            .expect("record first complete absence");
        finalize_evidence(&evidence, &pool, 5).await;
        synchronizer
            .save_list_items_with_mode(10, &[], false, Some(5), None)
            .await
            .expect("replay first generation idempotently");
        finalize_evidence(&evidence, &pool, 5).await;
        let first = sqlx::query(
            r#"SELECT "ObservationCount", "ConfirmedAt", "LastReconcileGeneration"
               FROM "AddonMediaListItemTombstone" WHERE "AddonMediaListItemId" = 20"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load first absence");
        assert_eq!(first.get::<i32, _>("ObservationCount"), 1);
        assert_eq!(first.get::<Option<String>, _>("ConfirmedAt"), None);
        assert_eq!(first.get::<i32, _>("LastReconcileGeneration"), 5);
        let first_diff = crate::repositories::SqliteAddonMediaListRepository(pool.clone())
            .saved_draft_diff(10)
            .await
            .expect("load first-absence diff");
        assert_eq!(first_diff.pending_removal_item_ids, [20]);
        assert_eq!(first_diff.pending_removal_generations.get(&20), Some(&5));

        #[allow(clippy::type_complexity)]
        // This test-local tuple mirrors the exact persisted-state SELECT below.
        let preserved: (
            Option<i64>,
            bool,
            bool,
            Option<i32>,
            Option<i32>,
            i32,
            String,
            Option<i32>,
            String,
            i32,
        ) = sqlx::query_as(
            r#"SELECT "MediaItemId", "Included", "Published", "Season", "Episode",
                          "ContentKind", "MetadataOverrideJson", "LivenessOverride",
                          "DraftOptionOverridesJson", "CollectionRole"
                   FROM "AddonMediaListItem" WHERE "Id" = 20"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load preserved operator state");
        assert_eq!(
            preserved,
            (
                Some(321),
                false,
                true,
                Some(7),
                Some(8),
                encode_enum(ersatzrs_domain::AddonMediaListContentKind::TelevisionEpisode),
                r#"{"title":"Operator title"}"#.to_owned(),
                Some(encode_enum(RemoteStreamLiveness::Live)),
                r#"{"quality":"high"}"#.to_owned(),
                encode_enum(ersatzrs_domain::AddonMediaListItemRole::Primary),
            )
        );
        let related: (i64, i64) = sqlx::query_as(
            r#"SELECT
                   (SELECT COUNT(*) FROM "AddonMediaListItemReplacement"
                    WHERE "AddonMediaListItemId" = 20),
                   (SELECT COUNT(*) FROM "AddonMediaListArtifact"
                    WHERE "AddonMediaListItemId" = 20)"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load retained replacement and artifact");
        assert_eq!(related, (1, 1));

        crate::repositories::SqliteAddonMediaListRepository(pool.clone())
            .confirm_item_tombstone(20, 5, Utc::now())
            .await
            .expect("confirm first absence explicitly");
        let confirmed: Option<String> = sqlx::query_scalar(
            r#"SELECT "ConfirmedAt" FROM "AddonMediaListItemTombstone"
               WHERE "AddonMediaListItemId" = 20"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load explicit confirmation");
        assert!(confirmed.is_some());
        synchronizer
            .save_list_items_with_mode(
                10,
                &[source_item("Provider return", 3)],
                false,
                Some(6),
                None,
            )
            .await
            .expect("clear explicit confirmation on reappearance");
        finalize_evidence(&evidence, &pool, 6).await;
        let reappeared: Option<String> = sqlx::query_scalar(
            r#"SELECT "ReappearedAt" FROM "AddonMediaListItemTombstone"
               WHERE "AddonMediaListItemId" = 20"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load explicit reappearance marker");
        assert!(reappeared.is_some());

        synchronizer
            .save_list_items_with_mode(10, &[], false, Some(7), None)
            .await
            .expect("record a new first absence");
        finalize_evidence(&evidence, &pool, 7).await;

        sqlx::query(
            r#"UPDATE "AddonMediaListItemTombstone"
               SET "MissingSince" = '2026-09-01T00:00:00Z'
               WHERE "AddonMediaListItemId" = 20"#,
        )
        .execute(&pool)
        .await
        .expect("age first observation");
        synchronizer
            .save_list_items_with_mode(10, &[], false, Some(8), None)
            .await
            .expect("record second complete absence");
        finalize_evidence(&evidence, &pool, 8).await;
        let second: (i32, Option<String>, i32) = sqlx::query_as(
            r#"SELECT "ObservationCount", "ConfirmedAt", "LastReconcileGeneration"
               FROM "AddonMediaListItemTombstone" WHERE "AddonMediaListItemId" = 20"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load confirmed absence");
        assert_eq!(second.0, 2);
        assert!(second.1.is_some());
        assert_eq!(second.2, 8);

        synchronizer
            .save_list_items_with_mode(
                10,
                &[source_item("Provider return", 3)],
                false,
                Some(9),
                None,
            )
            .await
            .expect("clear tombstone on reappearance");
        finalize_evidence(&evidence, &pool, 9).await;
        let reappeared: Option<String> = sqlx::query_scalar(
            r#"SELECT "ReappearedAt" FROM "AddonMediaListItemTombstone"
               WHERE "AddonMediaListItemId" = 20"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load reappearance marker");
        assert!(reappeared.is_some());
        let reappeared_diff = crate::repositories::SqliteAddonMediaListRepository(pool.clone())
            .saved_draft_diff(10)
            .await
            .expect("load reappearance diff");
        assert_eq!(reappeared_diff.reappeared_item_ids, [20]);
        let returned: (i64, i32, Option<i32>, Option<i32>, i32, bool, bool) = sqlx::query_as(
            r#"SELECT "Id", "Rank", "Season", "Episode", "ContentKind", "Included", "Published"
               FROM "AddonMediaListItem" WHERE "ProviderId" = 'provider-item'"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load reappeared item");
        assert_eq!(
            returned,
            (
                20,
                3,
                Some(7),
                Some(8),
                encode_enum(ersatzrs_domain::AddonMediaListContentKind::TelevisionEpisode),
                false,
                true,
            )
        );
    }

    #[tokio::test]
    async fn reconciliation_evidence_includes_expanded_chapter_fragments() {
        let pool = fixture().await;
        let synchronizer = AddonMediaListSynchronizer::new(pool.clone());
        let evidence = AddonMediaListRemoteSynchronizer::new(pool.clone(), UnusedProvider);

        synchronizer
            .save_list_items_with_mode(
                10,
                &[source_item("Provider item", 0), chapter_fragment(1)],
                false,
                Some(5),
                None,
            )
            .await
            .expect("persist expanded reconcile identities");
        finalize_evidence(&evidence, &pool, 5).await;

        let generations = sqlx::query(
            r#"SELECT "ProviderId", "LastReconcileGeneration"
               FROM "AddonMediaListItem"
               WHERE "AddonMediaListId" = 10
               ORDER BY "ProviderId""#,
        )
        .fetch_all(&pool)
        .await
        .expect("load expanded reconcile identities");
        assert_eq!(generations.len(), 2);
        assert!(generations
            .iter()
            .all(|row| row.get::<Option<i32>, _>("LastReconcileGeneration") == Some(5)));
        let tombstone_count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "AddonMediaListItemTombstone"
               WHERE "AddonMediaListItemId" IN (
                   SELECT "Id" FROM "AddonMediaListItem" WHERE "AddonMediaListId" = 10
               )"#,
        )
        .fetch_one(&pool)
        .await
        .expect("count false absence evidence");
        assert_eq!(tombstone_count, 0);
    }
}

#[cfg(test)]
mod generated_playlist_refresh_tests {
    mod source_tests;
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::Executor as _;

    use super::*;

    const LIST_ID: EntityId = 10;
    const LIBRARY_ID: EntityId = 500;
    const PLAYLIST_ID: EntityId = 100;
    const UNRELATED_PLAYLIST_ID: EntityId = 101;
    const RELATED_PLAYOUT_ID: EntityId = 900;
    const UNRELATED_PLAYOUT_ID: EntityId = 901;

    async fn pool() -> DbPool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect SQLite fixture");
        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .expect("migrate SQLite fixture");
        pool.execute("PRAGMA foreign_keys = OFF")
            .await
            .expect("disable fixture foreign keys");
        pool
    }

    async fn seed_legacy_option_namespace(pool: &DbPool) {
        sqlx::raw_sql(
            r#"INSERT INTO "AddonMediaListOptionSchema" (
                   "Id", "AddonId", "CapabilityId", "SchemaDigest", "DefinitionJson", "CreatedAt"
               ) VALUES (1, 'org.example.old', 'media-list.options.v1', 'digest', '{}',
                         '2026-09-04 12:00:00');
               INSERT INTO "AddonMediaList" (
                   "Id", "AddonId", "ProviderId", "SourceUrl", "Name", "DraftOptionSchemaId",
                   "DraftOptionValuesJson"
               ) VALUES (10, 'org.example.old', 'fixture', 'https://example.invalid/list',
                         'Fixture', 1, '{"enabled":true}');
               INSERT INTO "AddonMediaListItem" (
                   "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind", "Rank", "Title",
                   "DraftOptionOverridesJson"
               ) VALUES (11, 10, 'item', 'Item', 4, 0, 'Item', '{"enabled":false}');
               INSERT INTO "AddonMediaListRevision" (
                   "Id", "AddonMediaListId", "Revision", "OptionSchemaId", "OptionValuesJson"
               ) VALUES (12, 10, 1, 1, '{"enabled":true}');
               INSERT INTO "AddonMediaListImportCheckpoint" (
                   "Id", "AddonMediaListId", "RelativePath", "Generation", "Sha256",
                   "EntryCount", "PromotedAt", "Committed"
               ) VALUES (13, 10, 'checkpoints/old.txt', 1, 'old-digest', 1,
                         '2026-09-04 12:00:00', 1);"#,
        )
        .execute(pool)
        .await
        .expect("seed option namespace");
    }

    async fn restore_same_addon_option_namespace(pool: &DbPool) {
        sqlx::raw_sql(
            r#"UPDATE "AddonMediaList"
               SET "AddonId" = 'org.example.old', "DraftOptionSchemaId" = 1,
                   "DraftOptionValuesJson" = '{"enabled":true}'
               WHERE "Id" = 10;
               UPDATE "AddonMediaListItem" SET "DraftOptionOverridesJson" = '{"enabled":false}'
               WHERE "Id" = 11;
               UPDATE "AddonMediaListImportCheckpoint"
               SET "RelativePath" = 'checkpoints/current.txt', "Sha256" = 'current-digest',
                   "Committed" = 1
               WHERE "Id" = 13;"#,
        )
        .execute(pool)
        .await
        .expect("restore same-add-on draft options");
    }

    async fn draft_option_state(pool: &DbPool) -> (Option<EntityId>, String, String) {
        sqlx::query_as(
            r#"SELECT "DraftOptionSchemaId", "DraftOptionValuesJson",
                      (SELECT "DraftOptionOverridesJson" FROM "AddonMediaListItem" WHERE "Id" = 11)
               FROM "AddonMediaList" WHERE "Id" = 10"#,
        )
        .fetch_one(pool)
        .await
        .expect("load draft option state")
    }

    async fn checkpoint_state(pool: &DbPool) -> (i64, i64) {
        sqlx::query_as(
            r#"SELECT COUNT(*), COALESCE(SUM("Committed"), 0)
               FROM "AddonMediaListImportCheckpoint"
               WHERE "AddonMediaListId" = 10"#,
        )
        .fetch_one(pool)
        .await
        .expect("load retained and committed checkpoint counts")
    }

    #[tokio::test]
    async fn legacy_addon_switch_clears_only_draft_option_namespace_with_editor_fence() {
        let pool = pool().await;
        seed_legacy_option_namespace(&pool).await;

        let revision: i32 =
            sqlx::query_scalar(r#"SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id"=?"#)
                .bind(LIST_ID)
                .fetch_one(&pool)
                .await
                .unwrap();
        let before = draft_option_state(&pool).await;
        let stale = AddonMediaListRemoteSynchronizer::new(
            pool.clone(),
            crate::addon_media_list::InstalledAddonMediaListProvider::new(pool.clone()),
        )
        .with_editor_revision(Some(revision + 1));
        assert!(matches!(
            stale
                .update_addon_identity(LIST_ID, "org.example.new", "fixture")
                .await,
            Err(RepositoryError::Conflict(_))
        ));
        assert_eq!(draft_option_state(&pool).await, before);
        let synchronizer = AddonMediaListRemoteSynchronizer::new(
            pool.clone(),
            crate::addon_media_list::InstalledAddonMediaListProvider::new(pool.clone()),
        );
        synchronizer
            .update_addon_identity(LIST_ID, "org.example.new", "fixture")
            .await
            .expect("clear old option namespace");
        assert_eq!(
            draft_option_state(&pool).await,
            (None, "{}".into(), "{}".into())
        );
        let retained_revision: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "AddonMediaListRevision" WHERE "Id" = 12 AND "OptionSchemaId" = 1"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load retained revision");
        assert_eq!(retained_revision, 1);
        // Retain the file record for cleanup, but never reuse another add-on's archive.
        assert_eq!(checkpoint_state(&pool).await, (1, 0));

        restore_same_addon_option_namespace(&pool).await;
        synchronizer
            .update_addon_identity(LIST_ID, "org.example.old", "fixture")
            .await
            .expect("preserve same-add-on namespace");
        assert_eq!(
            draft_option_state(&pool).await,
            (
                Some(1),
                r#"{"enabled":true}"#.into(),
                r#"{"enabled":false}"#.into()
            )
        );
        assert_eq!(checkpoint_state(&pool).await, (1, 1));

        AddonMediaListSynchronizer::new(pool.clone())
            .save_list_items(LIST_ID, &[])
            .await
            .expect("persist successful refresh");
        let refresh_times: (Option<String>, Option<String>) = sqlx::query_as(
            r#"SELECT "LastUpdate", "LastRefreshSucceededAt"
               FROM "AddonMediaList" WHERE "Id" = 10"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load refresh timestamps");
        assert!(refresh_times.0.is_some());
        assert_eq!(refresh_times.0, refresh_times.1);
    }

    async fn seed_generated_list(pool: &DbPool) {
        pool.execute(
            r#"INSERT INTO "PlaylistGroup" ("Id", "Name", "IsSystem")
               VALUES (1, 'System', 1)"#,
        )
        .await
        .expect("seed system playlist group");
        sqlx::query(
            r#"INSERT INTO "Playlist" ("Id", "IsSystem", "Name", "PlaylistGroupId")
               VALUES (?, 1, 'Generated list', 1)"#,
        )
        .bind(PLAYLIST_ID)
        .execute(pool)
        .await
        .expect("seed generated playlist");
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" (
                   "Id", "AddonId", "ProviderId", "SourceUrl", "Name",
                   "GeneratePlaylist", "PlaylistId"
               ) VALUES (?, 'fixture.addon', 'fixture-list',
                         'https://example.invalid/list', 'Fixture list', 1, ?)"#,
        )
        .bind(LIST_ID)
        .bind(PLAYLIST_ID)
        .execute(pool)
        .await
        .expect("seed add-on list");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListLibrary" (
                   "AddonMediaListId", "LibraryId", "FolderName", "ManifestPath"
               ) VALUES (?, ?, 'fixture-list', 'fixture-list/list.yml')"#,
        )
        .bind(LIST_ID)
        .bind(LIBRARY_ID)
        .execute(pool)
        .await
        .expect("seed managed list library");
        for (media_item_id, provider_id) in [(1_i64, "provider-one"), (2, "provider-two")] {
            sqlx::query(
                r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State")
                   VALUES (?, 1, 0)"#,
            )
            .bind(media_item_id)
            .execute(pool)
            .await
            .expect("seed media item");
            sqlx::query(
                r#"INSERT INTO "RemoteStream" (
                       "Id", "Url", "IsLive", "AddonId", "ProviderId", "Liveness"
                   ) VALUES (?, ?, 0, 'fixture.addon', ?, 1)"#,
            )
            .bind(media_item_id)
            .bind(format!("https://example.invalid/media/{media_item_id}"))
            .bind(provider_id)
            .execute(pool)
            .await
            .expect("seed remote stream");
        }
        sqlx::query(
            r#"INSERT INTO "AddonMediaListItem" (
                   "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind",
                   "MediaItemId", "Rank", "Title", "SourceUrl", "Availability",
                   "DurationSeconds", "Published", "PublishedLiveness"
               ) VALUES (
                   20, ?, 'provider-one', 'Fixture item', 4, 1, 0, 'Fixture item',
                   'https://example.invalid/media/1', 0, 600, 1, 1
               )"#,
        )
        .bind(LIST_ID)
        .execute(pool)
        .await
        .expect("seed add-on list item");
        insert_playlist_item(pool, PLAYLIST_ID, 1)
            .await
            .expect("seed generated playlist item");
    }

    #[tokio::test]
    async fn detail_merge_defers_after_commit_while_publication_cleanup_is_pending() {
        let pool = pool().await;
        seed_generated_list(&pool).await;
        sqlx::query(
            r#"UPDATE "AddonMediaList"
               SET "ReviewState" = ?, "PublishedRevision" = 1,
                   "DraftRevision" = 1,
                   "PublicationJournalJson" = '{"phase":"CleaningUp"}'
               WHERE "Id" = ?"#,
        )
        .bind(encode_enum(
            ersatzrs_domain::AddonMediaListReviewState::Published,
        ))
        .bind(LIST_ID)
        .execute(&pool)
        .await
        .expect("checkpoint committed publication before cleanup");

        let outcome = AddonMediaListSynchronizer::new(pool.clone())
            .merge_detail_item(
                LIST_ID,
                &EnumeratedAddonMediaListItem {
                    availability_reason_code: None,
                    provider_id: "provider-one".to_owned(),
                    rank: 0,
                    display_title: "Changed provider title".to_owned(),
                    title: "Changed provider title".to_owned(),
                    year: Some(2026),
                    season: None,
                    episode: None,
                    kind: AddonMediaListItemKind::RemoteStream,
                    guids: Vec::new(),
                    source_url: Some("https://example.invalid/media/1".to_owned()),
                    availability: AddonMediaListItemAvailability::Available,
                    availability_reason: None,
                    content_kind: ersatzrs_domain::AddonMediaListContentKind::Auto,
                    collection_role: ersatzrs_domain::AddonMediaListItemRole::Auto,
                    duration_seconds: Some(600),
                    liveness: RemoteStreamLiveness::Live,
                    parent_provider_id: None,
                    fragment_start_seconds: None,
                    fragment_end_seconds: None,
                    additional_image_urls: vec!["https://example.invalid/poster.jpg".to_owned()],
                    metadata: ersatzrs_core::repositories::AddonMediaListNfoMetadata {
                        title: "Changed provider title".to_owned(),
                        ..Default::default()
                    },
                },
            )
            .await
            .expect("defer detail merge during publication cleanup");

        assert_eq!(outcome, DetailMergeOutcome::DeferredForPublication);
        let checkpoint = sqlx::query_as::<_, (String, String, i32, Option<String>, i32)>(
            r#"SELECT "item"."DisplayTitle", "item"."ProviderMetadataJson",
                      "item"."ProviderLiveness", "list"."DetailProjectionPendingAt",
                      "list"."DraftRevision"
               FROM "AddonMediaListItem" AS "item"
               JOIN "AddonMediaList" AS "list" ON "list"."Id" = "item"."AddonMediaListId"
               WHERE "item"."Id" = 20"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load unchanged publication checkpoint");
        assert_eq!(checkpoint.0, "Fixture item");
        assert_eq!(checkpoint.1, "{}");
        assert_eq!(checkpoint.2, encode_enum(RemoteStreamLiveness::Unknown));
        assert!(checkpoint.3.is_none());
        assert_eq!(checkpoint.4, 1);
    }

    async fn insert_playlist_item(
        pool: &DbPool,
        playlist_id: EntityId,
        media_item_id: EntityId,
    ) -> std::result::Result<(), sqlx::Error> {
        sqlx::query(
            r#"INSERT INTO "PlaylistItem" (
                   "Index", "PlaylistId", "CollectionType", "CollectionId",
                   "MediaItemId", "MultiCollectionId", "SmartCollectionId",
                   "IncludeInProgramGuide", "PlaybackOrder", "PlayAll",
                   "MediaVersionId", "VideoStreamIndex", "AudioStreamIndex",
                   "SubtitleStreamIndex"
               ) VALUES (0, ?, 70, NULL, ?, NULL, NULL, 1, 1, 0, NULL, NULL, NULL, NULL)"#,
        )
        .bind(playlist_id)
        .bind(media_item_id)
        .execute(pool)
        .await?;
        Ok(())
    }

    async fn seed_classic_playlist_playout(
        pool: &DbPool,
        playout_id: EntityId,
        playlist_id: EntityId,
    ) {
        let schedule_id = playout_id + 1_000;
        let schedule_item_id = playout_id + 2_000;
        sqlx::query(
            r#"INSERT INTO "ProgramSchedule" (
                   "Id", "Name", "KeepMultiPartEpisodesTogether", "TreatCollectionsAsShows",
                   "ShuffleScheduleItems", "RandomStartPoint", "FixedStartTimeBehavior"
               ) VALUES (?, ?, 1, 0, 0, 0, 'Strict')"#,
        )
        .bind(schedule_id)
        .bind(format!("Schedule {schedule_id}"))
        .execute(pool)
        .await
        .expect("seed program schedule");
        sqlx::query(
            r#"INSERT INTO "ProgramScheduleItem" (
                   "Id", "ProgramScheduleId", "CollectionType", "PlaylistId",
                   "PlaybackOrder", "FillWithGroupMode", "GuideMode", "MarathonGroupBy",
                   "MarathonShuffleGroups", "MarathonShuffleItems", "Index"
               ) VALUES (?, ?, 'Playlist', ?, 'Chronological', 'None', 'Normal',
                         'None', 0, 0, 0)"#,
        )
        .bind(schedule_item_id)
        .bind(schedule_id)
        .bind(playlist_id)
        .execute(pool)
        .await
        .expect("seed playlist schedule item");
        sqlx::query(r#"INSERT INTO "ProgramScheduleOneItem" ("Id") VALUES (?)"#)
            .bind(schedule_item_id)
            .execute(pool)
            .await
            .expect("seed one-item schedule mode");
        sqlx::query(
            r#"INSERT INTO "Playout" (
                   "Id", "ChannelId", "ProgramScheduleId", "ScheduleKind", "Seed"
               ) VALUES (?, ?, ?, 'Classic', 123)"#,
        )
        .bind(playout_id)
        .bind(playout_id)
        .bind(schedule_id)
        .execute(pool)
        .await
        .expect("seed Classic playout");
    }

    async fn select_second_provider(pool: &DbPool) {
        sqlx::query(
            r#"UPDATE "AddonMediaListItem"
               SET "ProviderId" = 'provider-two',
                   "SourceUrl" = 'https://example.invalid/media/2'
               WHERE "Id" = 20"#,
        )
        .execute(pool)
        .await
        .expect("select second provider item");
    }

    async fn playlist_item_identity(pool: &DbPool, playlist_id: EntityId) -> (EntityId, EntityId) {
        let row = sqlx::query(
            r#"SELECT "Id", "MediaItemId" FROM "PlaylistItem"
               WHERE "PlaylistId" = ? ORDER BY "Index", "Id" LIMIT 1"#,
        )
        .bind(playlist_id)
        .fetch_one(pool)
        .await
        .expect("load playlist item identity");
        (
            row.try_get("Id").expect("playlist item id"),
            row.try_get("MediaItemId").expect("playlist media item id"),
        )
    }

    async fn intent_playout_ids(pool: &DbPool) -> Vec<EntityId> {
        sqlx::query_scalar(
            r#"SELECT "PlayoutId" FROM "ScanPlayoutRefreshIntent" ORDER BY "PlayoutId""#,
        )
        .fetch_all(pool)
        .await
        .expect("load durable refresh intents")
    }

    async fn detach_seeded_playlist(pool: &DbPool) {
        sqlx::query(r#"DELETE FROM "PlaylistItem" WHERE "PlaylistId" = ?"#)
            .bind(PLAYLIST_ID)
            .execute(pool)
            .await
            .expect("remove seeded playlist items");
        sqlx::query(r#"UPDATE "AddonMediaList" SET "PlaylistId" = NULL WHERE "Id" = ?"#)
            .bind(LIST_ID)
            .execute(pool)
            .await
            .expect("detach seeded playlist");
        sqlx::query(r#"DELETE FROM "Playlist" WHERE "Id" = ?"#)
            .bind(PLAYLIST_ID)
            .execute(pool)
            .await
            .expect("remove seeded playlist");
    }

    async fn reject_refresh_intent_inserts(pool: &DbPool) {
        pool.execute(
            r#"CREATE TRIGGER "fail_scan_playout_refresh_intent"
               BEFORE INSERT ON "ScanPlayoutRefreshIntent"
               BEGIN
                   SELECT RAISE(ABORT, 'simulated refresh intent failure');
               END"#,
        )
        .await
        .expect("install refresh intent failure trigger");
    }

    async fn allow_refresh_intent_inserts(pool: &DbPool) {
        pool.execute(r#"DROP TRIGGER "fail_scan_playout_refresh_intent""#)
            .await
            .expect("remove refresh intent failure trigger");
    }

    async fn linked_playlist_id(pool: &DbPool) -> Option<EntityId> {
        sqlx::query_scalar(r#"SELECT "PlaylistId" FROM "AddonMediaList" WHERE "Id" = ?"#)
            .bind(LIST_ID)
            .fetch_one(pool)
            .await
            .expect("load generated playlist link")
    }

    async fn playlist_exists(pool: &DbPool, playlist_id: EntityId) -> bool {
        sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM "Playlist" WHERE "Id" = ?"#)
            .bind(playlist_id)
            .fetch_one(pool)
            .await
            .expect("count generated playlist")
            == 1
    }

    async fn intent_owner(pool: &DbPool, playout_id: EntityId) -> Option<EntityId> {
        sqlx::query_scalar(
            r#"SELECT "LibraryId" FROM "ScanPlayoutRefreshIntent" WHERE "PlayoutId" = ?"#,
        )
        .bind(playout_id)
        .fetch_one(pool)
        .await
        .expect("load refresh intent owner")
    }

    async fn intent_reason(pool: &DbPool, playout_id: EntityId) -> String {
        sqlx::query_scalar(
            r#"SELECT "Reason" FROM "ScanPlayoutRefreshIntent" WHERE "PlayoutId" = ?"#,
        )
        .bind(playout_id)
        .fetch_one(pool)
        .await
        .expect("load refresh intent reason")
    }

    #[tokio::test]
    async fn first_generated_playlist_creation_commits_complete_projection() {
        let pool = pool().await;
        seed_generated_list(&pool).await;
        detach_seeded_playlist(&pool).await;

        let report = AddonMediaListSynchronizer::new(pool.clone())
            .match_list_items_with_report(LIST_ID)
            .await
            .expect("create first generated playlist");

        let playlist_id = linked_playlist_id(&pool)
            .await
            .expect("generated playlist link");
        assert!(playlist_exists(&pool, playlist_id).await);
        assert_eq!(playlist_item_identity(&pool, playlist_id).await.1, 1);
        assert!(report.playout_refreshes.is_empty());
        assert!(intent_playout_ids(&pool).await.is_empty());
    }

    #[tokio::test]
    async fn first_generated_playlist_creation_rolls_back_with_its_intent() {
        let pool = pool().await;
        seed_generated_list(&pool).await;
        detach_seeded_playlist(&pool).await;
        seed_classic_playlist_playout(&pool, RELATED_PLAYOUT_ID, PLAYLIST_ID).await;
        let synchronizer = AddonMediaListSynchronizer::new(pool.clone());
        let list = synchronizer
            .load_list(LIST_ID)
            .await
            .expect("load generated list");
        let generation = crate::scan_refresh::begin_resolution(
            &pool,
            LIBRARY_ID,
            ScanPlayoutRefreshReason::RemoteStreamMembershipAdded,
        )
        .await
        .expect("begin generated playlist creation");
        let refresh = GeneratedPlaylistRefresh {
            library_id: Some(LIBRARY_ID),
            generation: Some(generation),
            reason: ScanPlayoutRefreshReason::RemoteStreamMembershipAdded,
            playout_ids: vec![RELATED_PLAYOUT_ID],
        };
        reject_refresh_intent_inserts(&pool).await;

        synchronizer
            .commit_generated_playlist_creation(
                &list,
                1,
                &[generated_item(1)],
                &refresh,
                false,
                false,
            )
            .await
            .expect_err("intent failure must roll back playlist creation");

        assert_eq!(linked_playlist_id(&pool).await, None);
        let playlist_count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Playlist""#)
            .fetch_one(&pool)
            .await
            .expect("count rolled-back playlists");
        assert_eq!(playlist_count, 0);
        assert!(intent_playout_ids(&pool).await.is_empty());
    }

    #[tokio::test]
    async fn disabling_generated_output_removes_playlist_and_intent_atomically() {
        let pool = pool().await;
        seed_generated_list(&pool).await;
        seed_classic_playlist_playout(&pool, RELATED_PLAYOUT_ID, PLAYLIST_ID).await;
        sqlx::query(r#"UPDATE "AddonMediaList" SET "GeneratePlaylist" = 0 WHERE "Id" = ?"#)
            .bind(LIST_ID)
            .execute(&pool)
            .await
            .expect("disable generated output");
        let synchronizer = AddonMediaListSynchronizer::new(pool.clone());
        let removal = synchronizer
            .prepare_generated_playlist_removal(LIST_ID)
            .await
            .expect("resolve generated playlist dependencies")
            .expect("generated playlist removal");
        reject_refresh_intent_inserts(&pool).await;

        let mut failed_tx = crate::db::begin_write(&pool)
            .await
            .expect("begin failed disable transaction");
        synchronizer
            .commit_generated_playlist_removal_in_tx(&mut failed_tx, LIST_ID, &removal)
            .await
            .expect_err("intent failure must roll back generated playlist removal");
        failed_tx
            .rollback()
            .await
            .expect("roll back failed disable transaction");

        assert_eq!(linked_playlist_id(&pool).await, Some(PLAYLIST_ID));
        assert!(playlist_exists(&pool, PLAYLIST_ID).await);
        assert!(intent_playout_ids(&pool).await.is_empty());

        allow_refresh_intent_inserts(&pool).await;
        let mut commit = crate::db::begin_write(&pool)
            .await
            .expect("begin disable transaction");
        synchronizer
            .commit_generated_playlist_removal_in_tx(&mut commit, LIST_ID, &removal)
            .await
            .expect("commit generated playlist removal");
        commit.commit().await.expect("commit disable transaction");

        assert_eq!(linked_playlist_id(&pool).await, None);
        assert!(!playlist_exists(&pool, PLAYLIST_ID).await);
        assert_eq!(intent_playout_ids(&pool).await, [RELATED_PLAYOUT_ID]);
        assert_eq!(
            intent_owner(&pool, RELATED_PLAYOUT_ID).await,
            Some(LIBRARY_ID)
        );
        assert_eq!(
            intent_reason(&pool, RELATED_PLAYOUT_ID).await,
            ScanPlayoutRefreshReason::RemoteStreamMembershipRemoved.as_str()
        );
    }

    #[tokio::test]
    async fn reset_empties_generated_playlist_and_preserves_scheduling_identity() {
        let pool = pool().await;
        seed_generated_list(&pool).await;
        seed_classic_playlist_playout(&pool, RELATED_PLAYOUT_ID, PLAYLIST_ID).await;
        sqlx::query(
            r#"INSERT INTO "FillerPreset" (
                   "Id", "Name", "FillerMode", "FillerKind", "CollectionType", "PlaylistId",
                   "AllowWatermarks", "UseChaptersAsMediaItems"
               ) VALUES (9001, 'Playlist filler', 0, 0, 70, ?, 0, 0)"#,
        )
        .bind(PLAYLIST_ID)
        .execute(&pool)
        .await
        .expect("seed playlist filler");
        sqlx::query(
            r#"INSERT INTO "PlayoutProgramScheduleAnchor" (
                   "Id", "PlayoutId", "CollectionType", "PlaylistId"
               ) VALUES (9002, ?, 70, ?)"#,
        )
        .bind(RELATED_PLAYOUT_ID)
        .bind(PLAYLIST_ID)
        .execute(&pool)
        .await
        .expect("seed playlist anchor");
        sqlx::query(r#"INSERT INTO "DecoGroup" ("Id", "Name") VALUES (9003, 'Reset')"#)
            .execute(&pool)
            .await
            .expect("seed deco group");
        sqlx::query(
            r#"INSERT INTO "Deco" (
                   "Id", "BreakContentMode", "DeadAirFallbackCollectionType",
                   "DeadAirFallbackMode", "DecoGroupId", "DefaultFillerCollectionType",
                   "DefaultFillerMode", "DefaultFillerTrimToFit", "GraphicsElementsMode",
                   "Name", "UseGraphicsElementsDuringFiller", "UseWatermarkDuringFiller",
                   "WatermarkMode"
               ) VALUES (9004, 0, 0, 0, 9003, 0, 0, 0, 0, 'Reset deco', 0, 0, 0)"#,
        )
        .execute(&pool)
        .await
        .expect("seed deco");
        sqlx::query(
            r#"INSERT INTO "DecoBreakContent" (
                   "Id", "DecoId", "CollectionType", "PlaylistId", "Placement"
               ) VALUES (9005, 9004, 70, ?, 0)"#,
        )
        .bind(PLAYLIST_ID)
        .execute(&pool)
        .await
        .expect("seed playlist deco content");
        let synchronizer = AddonMediaListSynchronizer::new(pool.clone());

        synchronizer
            .empty_generated_playlist_for_reset(LIST_ID)
            .await
            .expect("empty generated playlist");
        synchronizer
            .empty_generated_playlist_for_reset(LIST_ID)
            .await
            .expect("repeat empty generated playlist");

        assert!(playlist_exists(&pool, PLAYLIST_ID).await);
        let playlist_items: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "PlaylistItem" WHERE "PlaylistId" = ?"#)
                .bind(PLAYLIST_ID)
                .fetch_one(&pool)
                .await
                .expect("count playlist items");
        let schedule_references: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "ProgramScheduleItem" WHERE "PlaylistId" = ?"#,
        )
        .bind(PLAYLIST_ID)
        .fetch_one(&pool)
        .await
        .expect("count retained schedule references");
        assert_eq!(playlist_items, 0);
        assert_eq!(schedule_references, 1);
        for table in [
            "FillerPreset",
            "DecoBreakContent",
            "PlayoutProgramScheduleAnchor",
        ] {
            let sql = format!(r#"SELECT COUNT(*) FROM "{table}" WHERE "PlaylistId" = ?"#);
            let count: i64 = sqlx::query_scalar(crate::db::assert_sql_safe(&sql))
                .bind(PLAYLIST_ID)
                .fetch_one(&pool)
                .await
                .expect("count retained playlist reference");
            assert_eq!(count, 1, "{table} reference must survive reset");
        }
        assert_eq!(intent_playout_ids(&pool).await, [RELATED_PLAYOUT_ID]);
        assert_eq!(
            intent_reason(&pool, RELATED_PLAYOUT_ID).await,
            ScanPlayoutRefreshReason::RemoteStreamMembershipRemoved.as_str()
        );
    }

    #[tokio::test]
    async fn deleting_generated_list_removes_state_with_unowned_intent_atomically() {
        let pool = pool().await;
        seed_generated_list(&pool).await;
        seed_classic_playlist_playout(&pool, RELATED_PLAYOUT_ID, PLAYLIST_ID).await;
        let synchronizer = AddonMediaListSynchronizer::new(pool.clone());
        reject_refresh_intent_inserts(&pool).await;

        synchronizer
            .delete_list(LIST_ID)
            .await
            .expect_err("intent failure must roll back add-on list deletion");

        assert_eq!(linked_playlist_id(&pool).await, Some(PLAYLIST_ID));
        assert!(playlist_exists(&pool, PLAYLIST_ID).await);
        assert!(intent_playout_ids(&pool).await.is_empty());

        allow_refresh_intent_inserts(&pool).await;
        synchronizer
            .delete_list(LIST_ID)
            .await
            .expect("delete add-on list with durable refresh intent");

        let list_count: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaList" WHERE "Id" = ?"#)
                .bind(LIST_ID)
                .fetch_one(&pool)
                .await
                .expect("count deleted add-on list");
        assert_eq!(list_count, 0);
        assert!(!playlist_exists(&pool, PLAYLIST_ID).await);
        assert_eq!(intent_playout_ids(&pool).await, [RELATED_PLAYOUT_ID]);
        assert_eq!(intent_owner(&pool, RELATED_PLAYOUT_ID).await, None);
        assert_eq!(
            intent_reason(&pool, RELATED_PLAYOUT_ID).await,
            ScanPlayoutRefreshReason::RemoteStreamMembershipRemoved.as_str()
        );
    }

    #[tokio::test]
    async fn unchanged_generated_playlist_keeps_rows_and_creates_no_intent() {
        let pool = pool().await;
        seed_generated_list(&pool).await;
        let before = playlist_item_identity(&pool, PLAYLIST_ID).await;

        let report = AddonMediaListSynchronizer::new(pool.clone())
            .match_list_items_with_report(LIST_ID)
            .await
            .expect("match unchanged generated playlist");

        assert_eq!(report.matched_list_count, 1);
        assert!(report.playout_refreshes.is_empty());
        assert_eq!(playlist_item_identity(&pool, PLAYLIST_ID).await, before);
        assert!(intent_playout_ids(&pool).await.is_empty());
        let resolutions: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "ScanRefreshResolution" WHERE "LibraryId" = ?"#,
        )
        .bind(LIBRARY_ID)
        .fetch_one(&pool)
        .await
        .expect("count scan resolutions");
        assert_eq!(resolutions, 0);
    }

    #[tokio::test]
    async fn ordinary_match_retains_published_only_playlist_member() {
        let pool = pool().await;
        seed_generated_list(&pool).await;
        sqlx::query(r#"UPDATE "AddonMediaListItem" SET "InDraft"=0 WHERE "Id"=20"#)
            .execute(&pool)
            .await
            .expect("remove owner from draft only");
        let before = playlist_item_identity(&pool, PLAYLIST_ID).await;
        let synchronizer = AddonMediaListSynchronizer::new(pool.clone());

        let report = synchronizer
            .match_list_items_with_report(LIST_ID)
            .await
            .expect("match while published member is absent from draft");

        assert_eq!(report.matched_list_count, 1);
        assert!(report.playout_refreshes.is_empty());
        assert_eq!(playlist_item_identity(&pool, PLAYLIST_ID).await, before);
        assert!(intent_playout_ids(&pool).await.is_empty());
        let binding: Option<i64> =
            sqlx::query_scalar(r#"SELECT "MediaItemId" FROM "AddonMediaListItem" WHERE "Id"=20"#)
                .fetch_one(&pool)
                .await
                .expect("load retained published binding");
        assert_eq!(binding, Some(1));

        // Model the membership transition performed by explicit publication.
        sqlx::query(r#"UPDATE "AddonMediaListItem" SET "Published"=0 WHERE "Id"=20"#)
            .execute(&pool)
            .await
            .expect("remove owner from published membership");
        synchronizer
            .match_list_items_with_report(LIST_ID)
            .await
            .expect("match after published membership changes");
        assert!(load_generated_playlist_items(&pool, PLAYLIST_ID)
            .await
            .expect("load updated playlist")
            .is_empty());
    }

    #[tokio::test]
    async fn changed_generated_playlist_commits_one_exact_playout_intent() {
        let pool = pool().await;
        seed_generated_list(&pool).await;
        seed_classic_playlist_playout(&pool, RELATED_PLAYOUT_ID, PLAYLIST_ID).await;
        select_second_provider(&pool).await;

        let report = AddonMediaListSynchronizer::new(pool.clone())
            .match_list_items_with_report(LIST_ID)
            .await
            .expect("match changed generated playlist");

        assert_eq!(
            report.playout_refreshes,
            [ScanPlayoutRefresh {
                playout_id: RELATED_PLAYOUT_ID,
                reason: ScanPlayoutRefreshReason::RemoteStreamMembershipReplaced,
            }]
        );
        assert_eq!(intent_playout_ids(&pool).await, [RELATED_PLAYOUT_ID]);
        assert_eq!(playlist_item_identity(&pool, PLAYLIST_ID).await.1, 2);
        let resolution: (EntityId, i32) = sqlx::query_as(
            r#"SELECT "LibraryId", "State" FROM "ScanRefreshResolution"
               WHERE "LibraryId" = ?"#,
        )
        .bind(LIBRARY_ID)
        .fetch_one(&pool)
        .await
        .expect("load completed resolution");
        assert_eq!(resolution, (LIBRARY_ID, 1));
    }

    #[tokio::test]
    async fn shared_media_in_unrelated_playlist_creates_no_false_positive_intent() {
        let pool = pool().await;
        seed_generated_list(&pool).await;
        sqlx::query(
            r#"INSERT INTO "Playlist" ("Id", "IsSystem", "Name", "PlaylistGroupId")
               VALUES (?, 0, 'Unrelated', 1)"#,
        )
        .bind(UNRELATED_PLAYLIST_ID)
        .execute(&pool)
        .await
        .expect("seed unrelated playlist");
        insert_playlist_item(&pool, UNRELATED_PLAYLIST_ID, 1)
            .await
            .expect("seed shared media in unrelated playlist");
        seed_classic_playlist_playout(&pool, RELATED_PLAYOUT_ID, PLAYLIST_ID).await;
        seed_classic_playlist_playout(&pool, UNRELATED_PLAYOUT_ID, UNRELATED_PLAYLIST_ID).await;
        select_second_provider(&pool).await;

        let report = AddonMediaListSynchronizer::new(pool.clone())
            .match_list_items_with_report(LIST_ID)
            .await
            .expect("match changed generated playlist");

        assert_eq!(
            report
                .playout_refreshes
                .iter()
                .map(|refresh| refresh.playout_id)
                .collect::<Vec<_>>(),
            [RELATED_PLAYOUT_ID]
        );
        assert_eq!(intent_playout_ids(&pool).await, [RELATED_PLAYOUT_ID]);
    }

    #[tokio::test]
    async fn stale_resolution_rolls_back_playlist_replacement_and_intent() {
        let pool = pool().await;
        seed_generated_list(&pool).await;
        let before = playlist_item_identity(&pool, PLAYLIST_ID).await;
        let reason = ScanPlayoutRefreshReason::RemoteStreamMembershipReplaced;
        let stale_generation = crate::scan_refresh::begin_resolution(&pool, LIBRARY_ID, reason)
            .await
            .expect("begin stale resolution");
        let current_generation = crate::scan_refresh::begin_resolution(
            &pool,
            LIBRARY_ID,
            ScanPlayoutRefreshReason::RemoteStreamMembershipReordered,
        )
        .await
        .expect("supersede stale resolution");
        assert!(current_generation > stale_generation);
        let desired = [generated_item(2)];
        let refresh = GeneratedPlaylistRefresh {
            library_id: Some(LIBRARY_ID),
            generation: Some(stale_generation),
            reason,
            playout_ids: vec![RELATED_PLAYOUT_ID],
        };

        AddonMediaListSynchronizer::new(pool.clone())
            .commit_generated_playlist_match(LIST_ID, PLAYLIST_ID, &desired, &refresh, false)
            .await
            .expect_err("stale generation must reject the atomic commit");

        assert_eq!(playlist_item_identity(&pool, PLAYLIST_ID).await, before);
        assert!(intent_playout_ids(&pool).await.is_empty());
    }

    #[tokio::test]
    async fn changed_playlist_without_library_commits_nullable_owner_intent() {
        let pool = pool().await;
        seed_generated_list(&pool).await;
        let before = playlist_item_identity(&pool, PLAYLIST_ID).await;
        seed_classic_playlist_playout(&pool, RELATED_PLAYOUT_ID, PLAYLIST_ID).await;
        sqlx::query(r#"DELETE FROM "AddonMediaListLibrary" WHERE "AddonMediaListId" = ?"#)
            .bind(LIST_ID)
            .execute(&pool)
            .await
            .expect("remove managed library ownership");
        select_second_provider(&pool).await;

        let report = AddonMediaListSynchronizer::new(pool.clone())
            .match_list_items_with_report(LIST_ID)
            .await
            .expect("match changed legacy generated playlist");

        assert_eq!(
            report.playout_refreshes,
            [ScanPlayoutRefresh {
                playout_id: RELATED_PLAYOUT_ID,
                reason: ScanPlayoutRefreshReason::RemoteStreamMembershipReplaced,
            }]
        );
        assert_ne!(playlist_item_identity(&pool, PLAYLIST_ID).await, before);
        assert_eq!(intent_playout_ids(&pool).await, [RELATED_PLAYOUT_ID]);
        let owner: Option<EntityId> = sqlx::query_scalar(
            r#"SELECT "LibraryId" FROM "ScanPlayoutRefreshIntent" WHERE "PlayoutId" = ?"#,
        )
        .bind(RELATED_PLAYOUT_ID)
        .fetch_one(&pool)
        .await
        .expect("load nullable intent owner");
        assert_eq!(owner, None);
    }

    #[tokio::test]
    async fn list_without_generated_playlist_is_a_safe_noop() {
        let pool = pool().await;
        seed_generated_list(&pool).await;
        let before = playlist_item_identity(&pool, PLAYLIST_ID).await;
        sqlx::query(
            r#"UPDATE "AddonMediaList"
               SET "GeneratePlaylist" = 0, "PlaylistId" = NULL WHERE "Id" = ?"#,
        )
        .bind(LIST_ID)
        .execute(&pool)
        .await
        .expect("detach generated playlist");

        let report = AddonMediaListSynchronizer::new(pool.clone())
            .match_list_items_with_report(LIST_ID)
            .await
            .expect("match unmanaged list without generated playlist");

        assert!(report.playout_refreshes.is_empty());
        assert_eq!(playlist_item_identity(&pool, PLAYLIST_ID).await, before);
        assert!(intent_playout_ids(&pool).await.is_empty());
    }

    fn generated_item(media_item_id: EntityId) -> GeneratedPlaylistItem {
        GeneratedPlaylistItem {
            index: 0,
            collection_type: COLLECTION_TYPE_REMOTE_STREAM,
            collection_id: None,
            media_item_id: Some(media_item_id),
            multi_collection_id: None,
            smart_collection_id: None,
            include_in_program_guide: true,
            playback_order: PLAYBACK_ORDER_CHRONOLOGICAL,
            play_all: false,
            count: None,
            preferred_audio_language_code: None,
            preferred_audio_title: None,
            preferred_subtitle_language_code: None,
            subtitle_mode: None,
            media_version_id: None,
            video_stream_index: None,
            audio_stream_index: None,
            subtitle_stream_index: None,
        }
    }
}

#[cfg(test)]
mod metadata_conflict_tests {
    use super::*;

    fn conflicts(
        value: Option<&str>,
    ) -> std::collections::BTreeMap<String, AddonMediaListMetadataConflict> {
        serde_json::from_str(value.expect("conflict json")).expect("valid conflicts")
    }

    #[test]
    fn provider_change_beneath_override_creates_field_conflict() {
        let value = refresh_conflict_json(
            r#"{"title":"Provider one","plot":"Stable"}"#,
            r#"{"title":"Provider two","plot":"Stable"}"#,
            r#"{"title":"Operator title"}"#,
            None,
        )
        .expect("refresh conflicts");

        let conflicts = conflicts(value.as_deref());
        let title = conflicts.get("title").expect("title conflict");
        assert_eq!(title.provider_before, serde_json::json!("Provider one"));
        assert_eq!(title.provider_after, serde_json::json!("Provider two"));
        assert_eq!(title.operator_value, serde_json::json!("Operator title"));
    }

    #[test]
    fn conflict_persists_until_provider_matches_operator_or_override_is_removed() {
        let first = refresh_conflict_json(
            r#"{"title":"Provider one"}"#,
            r#"{"title":"Provider two"}"#,
            r#"{"title":"Operator title"}"#,
            None,
        )
        .expect("first refresh");
        let retained = refresh_conflict_json(
            r#"{"title":"Provider two"}"#,
            r#"{"title":"Provider two"}"#,
            r#"{"title":"Operator title"}"#,
            first.as_deref(),
        )
        .expect("unchanged refresh");
        assert!(retained.is_some());

        let resolved = refresh_conflict_json(
            r#"{"title":"Provider two"}"#,
            r#"{"title":"Operator title"}"#,
            r#"{"title":"Operator title"}"#,
            retained.as_deref(),
        )
        .expect("resolved refresh");
        assert!(resolved.is_none());

        let removed = refresh_conflict_json(
            r#"{"title":"Provider two"}"#,
            r#"{"title":"Provider three"}"#,
            "{}",
            retained.as_deref(),
        )
        .expect("override removed");
        assert!(removed.is_none());
    }

    #[test]
    fn liveness_override_wins_and_provider_change_creates_conflict() {
        let (effective, conflict) = refresh_liveness(
            RemoteStreamLiveness::Unknown,
            RemoteStreamLiveness::Live,
            Some(RemoteStreamLiveness::Finite),
            false,
        );
        assert_eq!(effective, RemoteStreamLiveness::Finite);
        assert!(conflict);

        let (effective, conflict) = refresh_liveness(
            RemoteStreamLiveness::Live,
            RemoteStreamLiveness::Finite,
            None,
            true,
        );
        assert_eq!(effective, RemoteStreamLiveness::Finite);
        assert!(!conflict);
    }
}
