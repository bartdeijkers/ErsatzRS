#[cfg(test)]
mod draft_membership_tests;
mod editor;
mod editor_artwork;
mod publication_projection;
mod publication_source;
pub(crate) use editor_artwork::stage as stage_editor_artwork_in_tx;
#[cfg(test)]
mod visibility_tests;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ersatzrs_core::repositories::{
    AddonMediaListAdminForm, AddonMediaListAdminRow, AddonMediaListAdminSortDirection,
    AddonMediaListAdminSortField, AddonMediaListArtwork, AddonMediaListArtworkRole,
    AddonMediaListArtworkUpload, AddonMediaListImportAction, AddonMediaListItemAdminRow,
    AddonMediaListItemCounts, AddonMediaListItemDetail, AddonMediaListItemReviewEdit,
    AddonMediaListLifecycleRepository, AddonMediaListMetadataConflict, AddonMediaListNfoForm,
    AddonMediaListNfoMetadata, AddonMediaListNfoOwnership, AddonMediaListPublication,
    AddonMediaListPublicationBlocker, AddonMediaListPublicationBlockerKind,
    AddonMediaListPublicationContinuation, AddonMediaListPublicationReadiness,
    AddonMediaListReplacementCandidate, AddonMediaListRepository, AddonMediaListRevisionFile,
    AddonMediaListRevisionSnapshot, RepositoryError, Result,
};
use ersatzrs_domain::{
    AddonMediaListArtifactState, AddonMediaListContentKind, AddonMediaListDetailPolicy,
    AddonMediaListDetailState, AddonMediaListItemAvailability, AddonMediaListItemKind,
    AddonMediaListItemRole, AddonMediaListPresentationKind,
    AddonMediaListPublicationFilesystemTransition, AddonMediaListPublicationOutcome,
    AddonMediaListRefreshCadence, AddonMediaListReviewState, EntityId, RemoteStreamLiveness,
};
use sha2::{Digest as _, Sha256};
use sqlx::{Pool, Row};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::addon_media_list::InstalledAddonMediaListProvider;
use crate::addon_media_list_import::AddonMediaListImportCoordinator;
use crate::addon_media_list_sync::{AddonMediaListRemoteSynchronizer, AddonMediaListSynchronizer};
use crate::metadata::{
    artwork_identity, artwork_kind, replace_artwork, replace_guids, replace_metadata_languages,
    replace_people, replace_strings, ArtworkRef, PersonRef,
};
use crate::repositories::addon_publication_fs::{
    self, ArchivedRevisionFile, PublicationFsIdentity, PublicationFsPaths, PublicationTreeRole,
    RestoreProgress,
};
use crate::repositories::common::{
    decode_bool_column, decode_datetime, decode_optional_datetime, decode_optional_enum_column,
    encode_datetime,
};
use crate::repositories::SqlxResultExt as _;

const PAGE_SIZE_KEY: &str = "pages.addon_media_lists.page_size";
const REVISION_BLOB_STORE_ID_KEY: &str = "addon_media_list.revision_blob_store_id";

fn parse_admin_datetime(value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| RepositoryError::Decode {
            kind: "add-on media-list refresh timestamp",
            value: error.to_string(),
        })
}
const DETAIL_PROJECTION_BATCH_SECONDS: i64 = 30;

/// Project one quiet batch of safe post-publication detail updates.
///
/// The pending timestamp is refreshed by every merge. Clearing it uses the
/// observed value as a compare-and-swap, so a detail that lands during
/// projection always leaves another batch pending. The caller queues one
/// library scan for the returned library rather than scanning once per item.
///
/// # Errors
///
/// Returns an error when the pending batch cannot be loaded, projected, or
/// cleared with its compare-and-swap guard.
pub async fn project_one_pending_detail_batch(
    pool: &Pool<crate::db::Db>,
) -> Result<Option<(EntityId, EntityId)>> {
    let cutoff = crate::repositories::common::encode_datetime(
        chrono::Utc::now() - chrono::Duration::seconds(DETAIL_PROJECTION_BATCH_SECONDS),
    );
    let row = sqlx::query(
        r#"SELECT "list"."Id", "list"."RequestedPresentationKind",
                  "list"."DetailProjectionPendingAt", "managed"."LibraryId"
           FROM "AddonMediaList" AS "list"
           JOIN "AddonMediaListLibrary" AS "managed"
             ON "managed"."AddonMediaListId" = "list"."Id"
           WHERE "list"."ReviewState" = ?
             AND ("list"."PublicationJournalJson" IS NULL
                  OR TRIM("list"."PublicationJournalJson") = '')
             AND "list"."DetailProjectionPendingAt" IS NOT NULL
             AND "list"."DetailProjectionPendingAt" <= ?
           ORDER BY "list"."DetailProjectionPendingAt", "list"."Id" LIMIT 1"#,
    )
    .bind(crate::repositories::common::encode_enum(
        AddonMediaListReviewState::Published,
    ))
    .bind(&cutoff)
    .fetch_optional(pool)
    .await
    .repository()?;
    let Some(row) = row else {
        return Ok(None);
    };
    let list_id: EntityId = row.try_get("Id").repository()?;
    let library_id: EntityId = row.try_get("LibraryId").repository()?;
    let pending_at: String = row.try_get("DetailProjectionPendingAt").repository()?;
    let requested = crate::repositories::common::decode_enum_column(
        "AddonMediaListPresentationKind",
        &row,
        "RequestedPresentationKind",
    )?;
    let effective = if requested == AddonMediaListPresentationKind::Auto {
        resolve_auto_presentation(pool, list_id).await?
    } else {
        requested
    };

    if parent_nfo_for(effective).is_some() {
        ensure_generated_parent_nfo(pool, list_id, effective, None, None).await?;
    }
    reconcile_projection(pool, list_id, requested, None).await?;
    AddonMediaListSynchronizer::new(pool.clone())
        .match_list_items(list_id)
        .await?;

    sqlx::query(
        r#"UPDATE "AddonMediaList" SET "DetailProjectionPendingAt" = NULL
           WHERE "Id" = ? AND "DetailProjectionPendingAt" = ?"#,
    )
    .bind(list_id)
    .bind(pending_at)
    .execute(pool)
    .await
    .repository()?;
    Ok(Some((list_id, library_id)))
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
struct PublicationJournal {
    #[serde(default)]
    operation_id: Option<EntityId>,
    #[serde(default)]
    generation: Option<i32>,
    #[serde(default)]
    saved_revision: Option<i32>,
    previous_review_state: AddonMediaListReviewState,
    previous_presentation_kind: AddonMediaListPresentationKind,
    previous_items: Vec<PublishedItemSnapshot>,
    previous_playlist: Option<PlaylistSnapshot>,
    #[serde(default)]
    managed_folder: Option<String>,
    #[serde(default)]
    backup_folder: Option<String>,
    #[serde(default)]
    failed_folder: Option<String>,
    #[serde(default)]
    previous_published_revision: Option<i32>,
    #[serde(default)]
    target_published_revision: Option<i32>,
    #[serde(default)]
    previous_materializations: Vec<ArtworkMaterializationSnapshot>,
    #[serde(default)]
    target_database: Option<PublicationDatabaseSnapshot>,
    /// None identifies a journal written before prepared-media selection existed.
    #[serde(default)]
    prepared_media: Option<std::collections::BTreeMap<String, EntityId>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    subtitles: Option<std::collections::BTreeMap<String, Vec<EntityId>>>,
    /// Frozen selection; None is a journal created before interval streaming.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    streaming_plans: Option<
        std::collections::BTreeMap<
            String,
            ersatzrs_core::addon_media_list_playback::AddonMediaListPlaybackTimelinePlan,
        >,
    >,
}

#[derive(Clone, Copy, Debug)]
struct PublicationFence<'a> {
    operation_id: EntityId,
    generation: i32,
    saved_revision: i32,
    owner_token: &'a str,
    lease_owner: &'a str,
}

impl SqliteAddonMediaListRepository {
    /// Resolve the existing database-owned retained-file store without creating it.
    pub(crate) async fn revision_disk_storage(&self) -> Result<(PathBuf, String)> {
        revision_blob_storage(&self.0).await
    }

    /// Read the exact playback selections owned by an active publication.
    /// Legacy journals without selections return empty maps.
    ///
    /// # Errors
    /// Rejects missing/inactive or malformed journals; callers must not substitute
    /// the previous publication when staging candidate files fails.
    pub async fn publication_playback_selection(
        &self,
        list_id: EntityId,
    ) -> Result<ersatzrs_core::repositories::AddonMediaListRevisionArtifacts> {
        let journal: Option<String> = sqlx::query_scalar(
        r#"SELECT "PublicationJournalJson" FROM "AddonMediaList" WHERE "Id"=? AND "ReviewState"=3"#,
    )
    .bind(list_id)
    .fetch_optional(&self.0)
    .await
    .repository()?
    .flatten();
        let journal = journal.ok_or_else(|| {
            RepositoryError::Conflict("the active publication journal is unavailable".into())
        })?;
        let journal: PublicationJournal = serde_json::from_str(&journal).map_err(|_| {
            RepositoryError::Conflict("the active publication journal is invalid".into())
        })?;
        Ok(
            ersatzrs_core::repositories::AddonMediaListRevisionArtifacts {
                prepared_media: journal.prepared_media.unwrap_or_default(),
                subtitles: journal.subtitles.unwrap_or_default(),
                streaming_plans: journal.streaming_plans.unwrap_or_default(),
                ..Default::default()
            },
        )
    }
}

#[derive(Clone, Copy, Debug)]
struct PublicationClaimFence<'a> {
    operation_id: EntityId,
    generation: i32,
    owner_token: &'a str,
    lease_owner: &'a str,
}

impl<'a> PublicationFence<'a> {
    const fn claim(self) -> PublicationClaimFence<'a> {
        PublicationClaimFence {
            operation_id: self.operation_id,
            generation: self.generation,
            owner_token: self.owner_token,
            lease_owner: self.lease_owner,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
struct ArtworkMaterializationSnapshot {
    id: EntityId,
    item_id: Option<EntityId>,
    scope_item_id: EntityId,
    role: i32,
    source: String,
    file_name: String,
    sha256: String,
    published_revision: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
struct PublicationDatabaseSnapshot {
    presentation_kind: AddonMediaListPresentationKind,
    published_item_count: i64,
    items: Vec<PublishedItemSnapshot>,
    playlist: Option<PlaylistSnapshot>,
    materializations: Vec<ArtworkMaterializationSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
struct PublishedItemSnapshot {
    id: EntityId,
    published: bool,
    media_item_id: Option<EntityId>,
    #[serde(default)]
    identity_captured: bool,
    #[serde(default)]
    kind: Option<AddonMediaListItemKind>,
    #[serde(default)]
    provider_id: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    year: Option<i32>,
    #[serde(default)]
    season: Option<i32>,
    #[serde(default)]
    episode: Option<i32>,
    #[serde(default)]
    source_url: Option<String>,
    #[serde(default)]
    rank: Option<i32>,
    #[serde(default)]
    guids: Option<Vec<String>>,
    #[serde(default)]
    published_liveness: RemoteStreamLiveness,
    #[serde(default)]
    published_availability: AddonMediaListItemAvailability,
    #[serde(default)]
    published_availability_reason: Option<String>,
    #[serde(default)]
    published_availability_reason_code: Option<ersatzrs_domain::AddonMediaListAvailabilityReason>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
struct PlaylistSnapshot {
    id: EntityId,
    is_system: bool,
    name: Option<String>,
    playlist_group_id: EntityId,
    items: Vec<PlaylistItemSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
struct PlaylistItemSnapshot {
    id: EntityId,
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

impl PublishedItemSnapshot {
    fn has_same_restored_content(&self, other: &Self) -> bool {
        self.id == other.id
            && self.published == other.published
            && self.published_liveness == other.published_liveness
            && (!other.identity_captured
                || (self.kind == other.kind
                    && self.provider_id == other.provider_id
                    && self.title == other.title
                    && self.year == other.year
                    && self.season == other.season
                    && self.episode == other.episode
                    && self.source_url == other.source_url
                    && self.rank == other.rank
                    && self.guids == other.guids))
    }
}

impl ArtworkMaterializationSnapshot {
    fn has_same_content(&self, other: &Self) -> bool {
        self.item_id == other.item_id
            && self.scope_item_id == other.scope_item_id
            && self.role == other.role
            && self.source == other.source
            && self.file_name == other.file_name
            && self.sha256 == other.sha256
            && self.published_revision == other.published_revision
    }
}

impl PlaylistSnapshot {
    fn has_same_restored_content(&self, other: &Self) -> bool {
        self.id == other.id
            && self.is_system == other.is_system
            && self.name == other.name
            && self.playlist_group_id == other.playlist_group_id
            && self.items.len() == other.items.len()
            && self
                .items
                .iter()
                .zip(&other.items)
                .all(|(left, right)| left.has_same_restored_content(right))
    }

    fn has_same_published_content(&self, other: &Self) -> bool {
        self.id == other.id
            && self.is_system == other.is_system
            && self.name == other.name
            && self.playlist_group_id == other.playlist_group_id
            && self.items.len() == other.items.len()
            && self
                .items
                .iter()
                .zip(&other.items)
                .all(|(left, right)| left.has_same_published_content(right))
    }
}

impl PlaylistItemSnapshot {
    fn has_same_restored_content(&self, other: &Self) -> bool {
        self.index == other.index
            && self.collection_type == other.collection_type
            && self.collection_id == other.collection_id
            && self.multi_collection_id == other.multi_collection_id
            && self.smart_collection_id == other.smart_collection_id
            && self.include_in_program_guide == other.include_in_program_guide
            && self.playback_order == other.playback_order
            && self.play_all == other.play_all
            && self.count == other.count
            && self.preferred_audio_language_code == other.preferred_audio_language_code
            && self.preferred_audio_title == other.preferred_audio_title
            && self.preferred_subtitle_language_code == other.preferred_subtitle_language_code
            && self.subtitle_mode == other.subtitle_mode
            && self.video_stream_index == other.video_stream_index
            && self.audio_stream_index == other.audio_stream_index
            && self.subtitle_stream_index == other.subtitle_stream_index
    }

    fn has_same_published_content(&self, other: &Self) -> bool {
        self.media_item_id == other.media_item_id
            && self.media_version_id == other.media_version_id
            && self.has_same_restored_content(other)
    }
}

#[derive(Clone)]
pub struct SqliteAddonMediaListRepository(pub Pool<crate::db::Db>);

async fn require_delivery_capability(
    pool: &Pool<crate::db::Db>,
    addon_id: &str,
    mode: ersatzrs_domain::AddonMediaListDeliveryMode,
) -> Result<()> {
    if mode == ersatzrs_domain::AddonMediaListDeliveryMode::Stream {
        return Ok(());
    }
    let installed = crate::addons::load_installed_addon(pool, addon_id)
        .await
        .map_err(|_| {
            RepositoryError::unavailable(
                "set add-on delivery mode",
                "installed add-on is unavailable",
            )
        })?;
    if installed.is_some_and(|addon| {
        addon
            .manifest
            .has_capability(ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_PREPARE_V2)
            && addon
                .manifest
                .has_capability(ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_ITEM_RUNTIME_V1)
    }) {
        Ok(())
    } else {
        Err(RepositoryError::unavailable(
            "set add-on delivery mode",
            "the installed add-on does not declare generic Download ahead support",
        ))
    }
}

#[async_trait]
#[allow(clippy::too_many_lines)]
impl AddonMediaListRepository for SqliteAddonMediaListRepository {
    async fn list_admin(
        &self,
        page: u32,
        page_size: u32,
        sort_field: AddonMediaListAdminSortField,
        sort_direction: AddonMediaListAdminSortDirection,
    ) -> Result<(Vec<AddonMediaListAdminRow>, u64)> {
        self.list_admin_visibility(page, page_size, sort_field, sort_direction, false)
            .await
    }

    async fn list_admin_visibility(
        &self,
        page: u32,
        page_size: u32,
        sort_field: AddonMediaListAdminSortField,
        sort_direction: AddonMediaListAdminSortDirection,
        hidden: bool,
    ) -> Result<(Vec<AddonMediaListAdminRow>, u64)> {
        let total: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaList" WHERE "AdminHidden" = ?"#)
                .bind(hidden)
                .fetch_one(&self.0)
                .await
                .repository()?;
        let sort_field = match sort_field {
            AddonMediaListAdminSortField::Name => 0_i32,
            AddonMediaListAdminSortField::Id => 1_i32,
        };
        let sort_direction = match sort_direction {
            AddonMediaListAdminSortDirection::Ascending => 0_i32,
            AddonMediaListAdminSortDirection::Descending => 1_i32,
        };
        let rows = sqlx::query(
            r#"SELECT "list"."Id", "list"."AddonId", "list"."ProviderId",
                      "list"."SourceUrl", COALESCE("list"."Name", '') AS "Name",
                      "list"."ItemCount", "list"."RefreshCadence", "list"."GeneratePlaylist",
                      "list"."LastUpdate", "list"."LastRefreshSucceededAt",
                      "list"."NextRefreshAt", "list"."RefreshRetryAt",
                      "list"."LastMatch", "list"."LastError",
                      "list"."PresentationKind", "list"."RequestedPresentationKind",
                      "list"."PresentationError", "list"."ReviewState", "list"."ReviewError",
                      "list"."PublishedRevision", "list"."DraftRevision",
                      "list"."DetailImportPolicy",
                      COUNT("matched"."Id") AS "MatchCount"
               FROM "AddonMediaList" AS "list"
               LEFT JOIN "AddonMediaListItem" AS "matched"
                 ON "matched"."AddonMediaListId" = "list"."Id"
                AND "matched"."InDraft" = 1
                AND "matched"."MediaItemId" IS NOT NULL
               WHERE "list"."AdminHidden" = ?5
               GROUP BY "list"."Id", "list"."AddonId", "list"."ProviderId",
                        "list"."SourceUrl", "list"."Name", "list"."ItemCount",
                        "list"."RefreshCadence", "list"."GeneratePlaylist",
                        "list"."LastUpdate", "list"."LastRefreshSucceededAt",
                        "list"."NextRefreshAt", "list"."RefreshRetryAt",
                        "list"."LastMatch", "list"."LastError",
                        "list"."PresentationKind", "list"."RequestedPresentationKind",
                        "list"."PresentationError", "list"."ReviewState", "list"."ReviewError",
                        "list"."PublishedRevision", "list"."DraftRevision",
                        "list"."DetailImportPolicy"
               ORDER BY
                 CASE WHEN ?1 = 0 AND ?2 = 0
                      THEN COALESCE("list"."Name", '') END COLLATE NOCASE ASC,
                 CASE WHEN ?1 = 0 AND ?2 = 1
                      THEN COALESCE("list"."Name", '') END COLLATE NOCASE DESC,
                 CASE WHEN ?1 = 1 AND ?2 = 0 THEN "list"."AddonId" END ASC,
                 CASE WHEN ?1 = 1 AND ?2 = 1 THEN "list"."AddonId" END DESC,
                 CASE WHEN ?1 = 1 AND ?2 = 0 THEN "list"."ProviderId" END ASC,
                 CASE WHEN ?1 = 1 AND ?2 = 1 THEN "list"."ProviderId" END DESC,
                 CASE WHEN ?2 = 0 THEN "list"."Id" END ASC,
                 CASE WHEN ?2 = 1 THEN "list"."Id" END DESC
               LIMIT ?3 OFFSET ?4"#,
        )
        .bind(sort_field)
        .bind(sort_direction)
        .bind(if page_size == 0 {
            -1
        } else {
            i64::from(page_size)
        })
        .bind(if page_size == 0 {
            0
        } else {
            i64::from(page).min(total.saturating_sub(1) / i64::from(page_size))
                * i64::from(page_size)
        })
        .bind(hidden)
        .fetch_all(&self.0)
        .await
        .repository()?;

        let rows = rows
            .iter()
            .map(|row| {
                let match_count: i64 = row.try_get("MatchCount").repository()?;
                Ok(AddonMediaListAdminRow {
                    id: row.try_get("Id").repository()?,
                    addon_id: row.try_get("AddonId").repository()?,
                    provider_id: row.try_get("ProviderId").repository()?,
                    source_url: row.try_get("SourceUrl").repository()?,
                    name: row.try_get("Name").repository()?,
                    item_count: row.try_get("ItemCount").repository()?,
                    match_count: i32::try_from(match_count).map_err(|_| {
                        RepositoryError::Decode {
                            kind: "Add-on media list match count",
                            value: match_count.to_string(),
                        }
                    })?,
                    refresh_cadence: crate::repositories::common::decode_enum_column(
                        "AddonMediaListRefreshCadence",
                        row,
                        "RefreshCadence",
                    )?,
                    generate_playlist: decode_bool_column(row, "GeneratePlaylist")?,
                    last_update: decode_optional_datetime(
                        "AddonMediaList.LastUpdate",
                        row.try_get("LastUpdate").repository()?,
                    )?
                    .map(|value| value.to_rfc3339()),
                    last_refresh_succeeded_at: decode_optional_datetime(
                        "AddonMediaList.LastRefreshSucceededAt",
                        row.try_get("LastRefreshSucceededAt").repository()?,
                    )?
                    .map(|value| value.to_rfc3339()),
                    next_refresh_at: decode_optional_datetime(
                        "AddonMediaList.NextRefreshAt",
                        row.try_get("NextRefreshAt").repository()?,
                    )?
                    .map(|value| value.to_rfc3339()),
                    refresh_retry_at: decode_optional_datetime(
                        "AddonMediaList.RefreshRetryAt",
                        row.try_get("RefreshRetryAt").repository()?,
                    )?
                    .map(|value| value.to_rfc3339()),
                    last_match: decode_optional_datetime(
                        "AddonMediaList.LastMatch",
                        row.try_get("LastMatch").repository()?,
                    )?
                    .map(|value| value.to_rfc3339()),
                    last_error: row.try_get("LastError").repository()?,
                    presentation_kind: crate::repositories::common::decode_enum_column(
                        "AddonMediaListPresentationKind",
                        row,
                        "PresentationKind",
                    )?,
                    requested_presentation_kind: crate::repositories::common::decode_enum_column(
                        "AddonMediaListPresentationKind",
                        row,
                        "RequestedPresentationKind",
                    )?,
                    presentation_error: row.try_get("PresentationError").repository()?,
                    review_state: crate::repositories::common::decode_enum_column(
                        "AddonMediaListReviewState",
                        row,
                        "ReviewState",
                    )?,
                    review_error: row.try_get("ReviewError").repository()?,
                    published_revision: row.try_get("PublishedRevision").repository()?,
                    draft_revision: row.try_get("DraftRevision").repository()?,
                    detail_import_policy: crate::repositories::common::decode_enum_column(
                        "AddonMediaListDetailPolicy",
                        row,
                        "DetailImportPolicy",
                    )?,
                    import_progress: None,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let mut rows = rows;
        let coordinator = AddonMediaListImportCoordinator::new(self.0.clone());
        for row in &mut rows {
            row.import_progress = coordinator.progress(row.id).await?;
        }
        let total = u64::try_from(total).map_err(|_| RepositoryError::Decode {
            kind: "Add-on media list count",
            value: total.to_string(),
        })?;
        Ok((rows, total))
    }

    async fn get_admin_form(&self, id: EntityId) -> Result<Option<AddonMediaListAdminForm>> {
        let row = sqlx::query(
            r#"SELECT "list"."Id", "list"."Name", "list"."AddonId", "list"."SourceUrl",
                      "list"."RefreshCadence", "list"."GeneratePlaylist", "list"."RefreshPublicationPolicy",
                      "list"."DeliveryMode", "list"."DeliveryCutoffAt",
                      "list"."LastRefreshSucceededAt", "list"."NextRefreshAt",
                      "list"."RefreshRetryAt",
                      "list"."ContentKindOverride", "list"."PresentationKind",
                      "list"."RequestedPresentationKind", "list"."PresentationError",
                      "list"."ChapterInput", "list"."ReviewState", "list"."ReviewError",
                      "list"."PublishedRevision", "list"."DraftRevision",
                      "list"."DetailImportPolicy",
                      COALESCE("managed"."FolderName", '') AS "FolderName"
               FROM "AddonMediaList" AS "list"
               LEFT JOIN "AddonMediaListLibrary" AS "managed"
                 ON "managed"."AddonMediaListId" = "list"."Id"
               WHERE "list"."Id" = ?"#,
        )
        .bind(id)
        .fetch_optional(&self.0)
        .await
        .repository()?;
        let mut form = row
            .map(|row| {
                Ok(AddonMediaListAdminForm {
                    auto_publication_hold: None,
                    delivery_mode: crate::repositories::common::decode_enum_column(
                        "AddonMediaListDeliveryMode",
                        &row,
                        "DeliveryMode",
                    )?,
                    delivery_cutoff_at: decode_optional_datetime(
                        "AddonMediaList.DeliveryCutoffAt",
                        row.try_get("DeliveryCutoffAt").repository()?,
                    )?
                    .map(|value| value.to_rfc3339()),
                    id: row.try_get("Id").repository()?,
                    name: row.try_get("Name").repository()?,
                    addon_id: row.try_get("AddonId").repository()?,
                    source_url: row.try_get("SourceUrl").repository()?,
                    folder_name: row.try_get("FolderName").repository()?,
                    refresh_publication_policy: crate::repositories::common::decode_enum_column(
                        "AddonMediaListRefreshPublicationPolicy",
                        &row,
                        "RefreshPublicationPolicy",
                    )?,
                    refresh_cadence: crate::repositories::common::decode_enum_column(
                        "AddonMediaListRefreshCadence",
                        &row,
                        "RefreshCadence",
                    )?,
                    last_refresh_succeeded_at: decode_optional_datetime(
                        "AddonMediaList.LastRefreshSucceededAt",
                        row.try_get("LastRefreshSucceededAt").repository()?,
                    )?
                    .map(|value| value.to_rfc3339()),
                    next_refresh_at: decode_optional_datetime(
                        "AddonMediaList.NextRefreshAt",
                        row.try_get("NextRefreshAt").repository()?,
                    )?
                    .map(|value| value.to_rfc3339()),
                    refresh_retry_at: decode_optional_datetime(
                        "AddonMediaList.RefreshRetryAt",
                        row.try_get("RefreshRetryAt").repository()?,
                    )?
                    .map(|value| value.to_rfc3339()),
                    generate_playlist: decode_bool_column(&row, "GeneratePlaylist")?,
                    content_kind_override: crate::repositories::common::decode_enum_column(
                        "AddonMediaListContentKind",
                        &row,
                        "ContentKindOverride",
                    )?,
                    presentation_kind: crate::repositories::common::decode_enum_column(
                        "AddonMediaListPresentationKind",
                        &row,
                        "PresentationKind",
                    )?,
                    requested_presentation_kind: crate::repositories::common::decode_enum_column(
                        "AddonMediaListPresentationKind",
                        &row,
                        "RequestedPresentationKind",
                    )?,
                    presentation_error: row.try_get("PresentationError").repository()?,
                    review_state: crate::repositories::common::decode_enum_column(
                        "AddonMediaListReviewState",
                        &row,
                        "ReviewState",
                    )?,
                    review_error: row.try_get("ReviewError").repository()?,
                    published_revision: row.try_get("PublishedRevision").repository()?,
                    draft_revision: row.try_get("DraftRevision").repository()?,
                    detail_import_policy: crate::repositories::common::decode_enum_column(
                        "AddonMediaListDetailPolicy",
                        &row,
                        "DetailImportPolicy",
                    )?,
                    import_progress: None,
                    legacy_chapter_input: row
                        .try_get::<Option<String>, _>("ChapterInput")
                        .repository()?
                        .unwrap_or_default(),
                })
            })
            .transpose()?;
        if let Some(form) = &mut form {
            form.auto_publication_hold =
                crate::addon_media_list_auto_publication::load_hold(&self.0, id).await?;
            form.import_progress = AddonMediaListImportCoordinator::new(self.0.clone())
                .progress(form.id)
                .await?;
        }
        Ok(form)
    }

    async fn list_items(&self, id: EntityId) -> Result<Vec<AddonMediaListItemAdminRow>> {
        self.query_items(id, None, 0, None, true).await
    }

    async fn publication_readiness(
        &self,
        id: EntityId,
    ) -> Result<AddonMediaListPublicationReadiness> {
        let form = self
            .get_admin_form(id)
            .await?
            .ok_or(RepositoryError::NotFound {
                kind: "add-on media list",
                id,
            })?;
        let items = self.query_items(id, None, 0, None, true).await?;
        addon_media_list_publication_readiness(&self.0, &form, &items).await
    }

    async fn list_items_page(
        &self,
        id: EntityId,
        availability: AddonMediaListItemAvailability,
        page: u32,
        page_size: Option<u32>,
    ) -> Result<Vec<AddonMediaListItemAdminRow>> {
        self.query_items(id, Some(availability), page, page_size, true)
            .await
    }

    async fn item_counts(&self, id: EntityId) -> Result<AddonMediaListItemCounts> {
        let rows = sqlx::query(
            r#"SELECT "Availability", COUNT(*) AS "Total"
               FROM "AddonMediaListItem"
               WHERE "AddonMediaListId" = ? AND "InDraft" = 1
               GROUP BY "Availability""#,
        )
        .bind(id)
        .fetch_all(&self.0)
        .await
        .repository()?;
        let mut counts = AddonMediaListItemCounts::default();
        for row in rows {
            let total = u64::try_from(row.try_get::<i64, _>("Total").repository()?).unwrap_or(0);
            match crate::repositories::common::decode_enum_column(
                "AddonMediaListItemAvailability",
                &row,
                "Availability",
            )? {
                AddonMediaListItemAvailability::Available => counts.available = total,
                AddonMediaListItemAvailability::Unavailable => counts.unavailable = total,
                AddonMediaListItemAvailability::Unknown => counts.unknown = total,
            }
        }
        Ok(counts)
    }

    async fn update_item_content_kind(
        &self,
        item_id: EntityId,
        content_kind: AddonMediaListContentKind,
    ) -> Result<()> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let row =
            sqlx::query(r#"SELECT "AddonMediaListId" FROM "AddonMediaListItem" WHERE "Id" = ? AND "InDraft" = 1"#)
                .bind(item_id)
                .fetch_optional(&mut *tx)
                .await
                .repository()?
                .ok_or(RepositoryError::NotFound {
                    kind: "add-on media list item",
                    id: item_id,
                })?;
        let list_id: EntityId = row.try_get("AddonMediaListId").repository()?;
        let updated =
            sqlx::query(r#"UPDATE "AddonMediaListItem" SET "ContentKind" = ? WHERE "Id" = ?"#)
                .bind(crate::repositories::common::encode_enum(content_kind))
                .bind(item_id)
                .execute(&mut *tx)
                .await
                .repository()?;
        if updated.rows_affected() == 0 {
            return Err(RepositoryError::NotFound {
                kind: "add-on media list item",
                id: item_id,
            });
        }
        mark_list_ready_tx(&mut tx, list_id).await?;
        tx.commit().await.repository()?;
        Ok(())
    }

    async fn set_item_liveness_override(
        &self,
        list_id: EntityId,
        item_id: EntityId,
        liveness: Option<RemoteStreamLiveness>,
    ) -> Result<()> {
        let encoded = liveness.map(crate::repositories::common::encode_enum);
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let updated = sqlx::query(
            r#"UPDATE "AddonMediaListItem"
               SET "LivenessOverride" = ?,
                   "EffectiveLiveness" = COALESCE(?, "ProviderLiveness"),
                   "LivenessConflict" = 0
               WHERE "Id" = ? AND "AddonMediaListId" = ? AND "Kind" = 4 AND "InDraft" = 1"#,
        )
        .bind(encoded)
        .bind(encoded)
        .bind(item_id)
        .bind(list_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        if updated.rows_affected() != 1 {
            return Err(RepositoryError::NotFound {
                kind: "Remote Stream add-on media list item",
                id: item_id,
            });
        }
        mark_list_ready_tx(&mut tx, list_id).await?;
        tx.commit().await.repository()?;
        Ok(())
    }

    async fn apply_list_liveness_override(
        &self,
        id: EntityId,
        liveness: Option<RemoteStreamLiveness>,
        item_ids: &[EntityId],
    ) -> Result<u64> {
        const MAX_BULK_ITEMS: usize = 10_000;
        let ids = item_ids
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        if ids.len() > MAX_BULK_ITEMS {
            return Err(RepositoryError::Conflict(format!(
                "a liveness bulk edit may contain at most {MAX_BULK_ITEMS} items"
            )));
        }
        let encoded = liveness.map(crate::repositories::common::encode_enum);
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let affected = if ids.is_empty() {
            sqlx::query(
                r#"UPDATE "AddonMediaListItem"
                   SET "LivenessOverride" = ?,
                       "EffectiveLiveness" = COALESCE(?, "ProviderLiveness"),
                       "LivenessConflict" = 0
                   WHERE "AddonMediaListId" = ? AND "Kind" = 4
                     AND "InDraft" = 1
                     AND "ParentProviderId" IS NULL"#,
            )
            .bind(encoded)
            .bind(encoded)
            .bind(id)
            .execute(&mut *tx)
            .await
            .repository()?
            .rows_affected()
        } else {
            let mut affected = 0_u64;
            for item_id in ids {
                require_draft_item_tx(&mut tx, id, item_id).await?;
                affected = affected.saturating_add(
                    sqlx::query(
                        r#"UPDATE "AddonMediaListItem"
                           SET "LivenessOverride" = ?,
                               "EffectiveLiveness" = COALESCE(?, "ProviderLiveness"),
                               "LivenessConflict" = 0
                           WHERE "Id" = ? AND "AddonMediaListId" = ? AND "Kind" = 4 AND "InDraft" = 1"#,
                    )
                    .bind(encoded)
                    .bind(encoded)
                    .bind(item_id)
                    .bind(id)
                    .execute(&mut *tx)
                    .await
                    .repository()?
                    .rows_affected(),
                );
            }
            affected
        };
        mark_list_ready_tx(&mut tx, id).await?;
        tx.commit().await.repository()?;
        Ok(affected)
    }

    async fn apply_list_content_kind(
        &self,
        id: EntityId,
        content_kind: AddonMediaListContentKind,
    ) -> Result<u64> {
        let encoded = crate::repositories::common::encode_enum(content_kind);
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        // The list value is the bulk default, so it is stored as well: entries
        // enumerated later inherit the same classification.
        sqlx::query(r#"UPDATE "AddonMediaList" SET "ContentKindOverride" = ? WHERE "Id" = ?"#)
            .bind(encoded)
            .bind(id)
            .execute(&mut *tx)
            .await
            .repository()?;
        let updated = sqlx::query(
            r#"UPDATE "AddonMediaListItem" SET "ContentKind" = ? WHERE "AddonMediaListId" = ? AND "InDraft" = 1"#,
        )
        .bind(encoded)
        .bind(id)
        .execute(&mut *tx)
        .await
        .repository()?;
        mark_list_ready_tx(&mut tx, id).await?;
        tx.commit().await.repository()?;
        Ok(updated.rows_affected())
    }

    async fn apply_item_review_edits(
        &self,
        id: EntityId,
        edits: &[AddonMediaListItemReviewEdit],
    ) -> Result<u64> {
        const MAX_BULK_ITEMS: usize = 10_000;
        if edits.len() > MAX_BULK_ITEMS {
            return Err(RepositoryError::Conflict(format!(
                "a review-table edit may contain at most {MAX_BULK_ITEMS} items"
            )));
        }
        let unique_ids = edits
            .iter()
            .map(|edit| edit.item_id)
            .collect::<std::collections::BTreeSet<_>>();
        if unique_ids.len() != edits.len() {
            return Err(RepositoryError::Conflict(
                "a review-table edit contains duplicate item identities".to_owned(),
            ));
        }

        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        crate::addon_media_list_sync::ensure_not_publishing_tx(&mut tx, id).await?;

        for edit in edits {
            let row = sqlx::query(
                r#"SELECT "ProviderMetadataJson", "MetadataOverrideJson", "MetadataConflictJson"
                   FROM "AddonMediaListItem"
                   WHERE "Id" = ? AND "AddonMediaListId" = ? AND "InDraft" = 1"#,
            )
            .bind(edit.item_id)
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .repository()?
            .ok_or(RepositoryError::NotFound {
                kind: "add-on media list item",
                id: edit.item_id,
            })?;
            let provider_json: String = row.try_get("ProviderMetadataJson").repository()?;
            let override_json: String = row.try_get("MetadataOverrideJson").repository()?;
            let conflict_json: Option<String> = row.try_get("MetadataConflictJson").repository()?;
            let mut form =
                metadata_form_from_json(&provider_json, &override_json, conflict_json.as_deref())?;
            form.metadata.season = edit.season;
            form.metadata.episode = edit.episode;
            form.conflicts.remove("season");
            form.conflicts.remove("episode");
            let overrides =
                metadata_override_json(&metadata_json_value(&provider_json)?, &form.metadata)?;
            let conflicts = if form.conflicts.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&form.conflicts).map_err(|error| {
                    RepositoryError::Decode {
                        kind: "add-on metadata conflicts",
                        value: error.to_string(),
                    }
                })?)
            };
            let content_kind = crate::repositories::common::encode_enum(edit.content_kind);
            let liveness = edit
                .liveness_override
                .map(crate::repositories::common::encode_enum);
            let updated = sqlx::query(
                r#"UPDATE "AddonMediaListItem"
                   SET "ContentKind" = ?, "Included" = ?,
                       "LivenessOverride" = CASE WHEN "Kind" = 4 THEN ? ELSE "LivenessOverride" END,
                       "EffectiveLiveness" = CASE WHEN "Kind" = 4 THEN COALESCE(?, "ProviderLiveness") ELSE "EffectiveLiveness" END,
                       "LivenessConflict" = CASE WHEN "Kind" = 4 THEN 0 ELSE "LivenessConflict" END,
                       "MetadataOverrideJson" = ?, "MetadataConflictJson" = ?
                   WHERE "Id" = ? AND "AddonMediaListId" = ?"#,
            )
            .bind(content_kind)
            .bind(edit.included)
            .bind(liveness)
            .bind(liveness)
            .bind(overrides)
            .bind(conflicts)
            .bind(edit.item_id)
            .bind(id)
            .execute(&mut *tx)
            .await
            .repository()?;
            if updated.rows_affected() != 1 {
                return Err(RepositoryError::NotFound {
                    kind: "add-on media list item",
                    id: edit.item_id,
                });
            }
        }

        if !edits.is_empty() {
            mark_list_ready_tx(&mut tx, id).await?;
        }
        tx.commit().await.repository()?;
        u64::try_from(edits.len()).map_err(|_| RepositoryError::Decode {
            kind: "updated review-table item count",
            value: edits.len().to_string(),
        })
    }

    async fn assign_episode_numbers(
        &self,
        id: EntityId,
        season: i32,
        starting_episode: i32,
        item_count: Option<u32>,
        missing_only: bool,
    ) -> Result<u64> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        crate::addon_media_list_sync::ensure_not_publishing_tx(&mut tx, id).await?;

        let rows = sqlx::query(
            r#"SELECT "Id", "ProviderMetadataJson", "MetadataOverrideJson",
                      "MetadataConflictJson"
               FROM "AddonMediaListItem"
               WHERE "AddonMediaListId" = ? AND "ParentProviderId" IS NULL
                 AND "Included" = 1 AND "InDraft" = 1
               ORDER BY "Rank", "Id""#,
        )
        .bind(id)
        .fetch_all(&mut *tx)
        .await
        .repository()?;
        let limit = item_count.map_or(usize::MAX, |count| {
            usize::try_from(count).unwrap_or(usize::MAX)
        });
        let mut occupied_episodes = std::collections::BTreeSet::new();
        if missing_only || limit < rows.len() {
            let retained_rows = if missing_only {
                rows.as_slice()
            } else {
                &rows[limit..]
            };
            for row in retained_rows {
                let provider_json: String = row.try_get("ProviderMetadataJson").repository()?;
                let override_json: String = row.try_get("MetadataOverrideJson").repository()?;
                let conflict_json: Option<String> =
                    row.try_get("MetadataConflictJson").repository()?;
                let metadata = metadata_form_from_json(
                    &provider_json,
                    &override_json,
                    conflict_json.as_deref(),
                )?
                .metadata;
                if metadata.season == Some(season) {
                    if let Some(episode) = metadata.episode.filter(|value| *value >= 1) {
                        occupied_episodes.insert(episode);
                    }
                }
            }
        }
        let mut assigned = 0_usize;
        let mut next_episode = starting_episode;
        for row in rows {
            if assigned == limit {
                break;
            }
            let item_id: EntityId = row.try_get("Id").repository()?;
            let provider_json: String = row.try_get("ProviderMetadataJson").repository()?;
            let override_json: String = row.try_get("MetadataOverrideJson").repository()?;
            let conflict_json: Option<String> = row.try_get("MetadataConflictJson").repository()?;
            let mut form =
                metadata_form_from_json(&provider_json, &override_json, conflict_json.as_deref())?;
            let has_valid_numbering = form.metadata.season.is_some_and(|value| value >= 0)
                && form.metadata.episode.is_some_and(|value| value >= 1);
            if missing_only && has_valid_numbering {
                continue;
            }

            while occupied_episodes.contains(&next_episode) {
                next_episode = next_episode.checked_add(1).ok_or_else(|| {
                    RepositoryError::Conflict(
                        "episode numbering exceeds the supported range".to_owned(),
                    )
                })?;
            }
            form.metadata.season = Some(season);
            form.metadata.episode = Some(next_episode);
            occupied_episodes.insert(next_episode);
            form.conflicts.remove("season");
            form.conflicts.remove("episode");
            let overrides =
                metadata_override_json(&metadata_json_value(&provider_json)?, &form.metadata)?;
            let conflicts = if form.conflicts.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&form.conflicts).map_err(|error| {
                    RepositoryError::Decode {
                        kind: "add-on metadata conflicts",
                        value: error.to_string(),
                    }
                })?)
            };
            sqlx::query(
                r#"UPDATE "AddonMediaListItem"
                   SET "MetadataOverrideJson" = ?, "MetadataConflictJson" = ?
                   WHERE "Id" = ? AND "AddonMediaListId" = ?"#,
            )
            .bind(overrides)
            .bind(conflicts)
            .bind(item_id)
            .bind(id)
            .execute(&mut *tx)
            .await
            .repository()?;
            assigned += 1;
            next_episode = next_episode.saturating_add(1);
        }

        if assigned > 0 {
            mark_list_ready_tx(&mut tx, id).await?;
        }
        tx.commit().await.repository()?;
        u64::try_from(assigned).map_err(|_| RepositoryError::Decode {
            kind: "assigned episode count",
            value: assigned.to_string(),
        })
    }

    async fn get_item_detail(
        &self,
        list_id: EntityId,
        item_id: EntityId,
    ) -> Result<Option<AddonMediaListItemDetail>> {
        let items = self.list_items(list_id).await?;
        let Some(item) = items.iter().find(|item| item.id == item_id).cloned() else {
            return Ok(None);
        };
        let path = item_nfo_path_for(&self.0, list_id, item_id).await?;
        // The editor always reads the durable provider-plus-override draft.
        // The NFO is a published output and may intentionally still represent
        // the previous revision while this draft is being reviewed.
        let mut nfo = load_item_metadata_form(&self.0, list_id, item_id).await?;
        nfo.path_available = path.as_deref().is_some_and(Path::is_file);
        let child_ids = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "FragmentItemId" FROM "AddonMediaListChapter"
               WHERE "SourceItemId" = ? ORDER BY "ChapterIndex", "Id""#,
        )
        .bind(item_id)
        .fetch_all(&self.0)
        .await
        .repository()?;
        let chapters = child_ids
            .into_iter()
            .filter_map(|id| items.iter().find(|item| item.id == id).cloned())
            .collect();
        Ok(Some(AddonMediaListItemDetail {
            item,
            nfo,
            chapters,
        }))
    }

    async fn get_show_nfo(&self, id: EntityId) -> Result<AddonMediaListNfoForm> {
        let path = self.parent_nfo_path(id).await?;
        let mut nfo = load_list_metadata_form(&self.0, id).await?;
        nfo.path_available = path.as_deref().is_some_and(Path::is_file);
        Ok(nfo)
    }

    async fn search_replacement_candidates(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<AddonMediaListReplacementCandidate>> {
        let pattern = format!("%{}%", query.trim());
        let rows = sqlx::query(replacement_candidate_query(true))
            .bind(&pattern)
            .bind(i64::from(limit.clamp(1, 100)))
            .fetch_all(&self.0)
            .await
            .repository()?;
        rows.iter().map(decode_replacement_candidate).collect()
    }

    async fn set_replacement(
        &self,
        item_id: EntityId,
        replacement: Option<(EntityId, EntityId)>,
    ) -> Result<()> {
        let list_id = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "AddonMediaListId" FROM "AddonMediaListItem" WHERE "Id" = ? AND "InDraft" = 1"#,
        )
        .bind(item_id)
        .fetch_optional(&self.0)
        .await
        .repository()?
        .ok_or(RepositoryError::NotFound {
            kind: "add-on media list item",
            id: item_id,
        })?;
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        require_draft_item_tx(&mut tx, list_id, item_id).await?;
        if let Some((media_item_id, media_version_id)) = replacement {
            if load_replacement_candidate(&self.0, media_item_id, media_version_id)
                .await?
                .is_none()
            {
                return Err(RepositoryError::Conflict(
                    "replacement must be an available Local video version".to_owned(),
                ));
            }
            let now = chrono::Utc::now().to_rfc3339();
            let updated = sqlx::query(
                r#"UPDATE "AddonMediaListItemReplacement"
                   SET "MediaItemId" = ?, "MediaVersionId" = ?, "UpdatedAt" = ?
                   WHERE "AddonMediaListItemId" = ?"#,
            )
            .bind(media_item_id)
            .bind(media_version_id)
            .bind(&now)
            .bind(item_id)
            .execute(&mut *tx)
            .await
            .repository()?;
            if updated.rows_affected() == 0 {
                sqlx::query(
                    r#"INSERT INTO "AddonMediaListItemReplacement" (
                        "AddonMediaListItemId", "MediaItemId", "MediaVersionId", "CreatedAt", "UpdatedAt"
                       ) VALUES (?, ?, ?, ?, ?)"#,
                )
                .bind(item_id)
                .bind(media_item_id)
                .bind(media_version_id)
                .bind(&now)
                .bind(&now)
                .execute(&mut *tx)
                .await
                .repository()?;
            }
        } else {
            sqlx::query(
                r#"DELETE FROM "AddonMediaListItemReplacement" WHERE "AddonMediaListItemId" = ?"#,
            )
            .bind(item_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        }
        sqlx::query(
            r#"UPDATE "AddonMediaListItem" SET "SavedReplacementJson" = NULL WHERE "Id" = ?"#,
        )
        .bind(item_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        mark_list_ready_tx(&mut tx, list_id).await?;
        tx.commit().await.repository()?;
        Ok(())
    }

    async fn add_or_refresh(
        &self,
        addon_id: &str,
        source_url: &str,
        requested_folder: Option<&str>,
        content_kind_override: AddonMediaListContentKind,
        chapter_input: &str,
    ) -> Result<EntityId> {
        let provider = InstalledAddonMediaListProvider::new(self.0.clone());
        let id = AddonMediaListRemoteSynchronizer::new(self.0.clone(), provider)
            .add_or_refresh(
                addon_id,
                source_url,
                requested_folder,
                content_kind_override,
                chapter_input,
            )
            .await?;
        if self
            .get_admin_form(id)
            .await?
            .is_some_and(|form| form.review_state == AddonMediaListReviewState::Published)
        {
            adopt_derived_presentation(&self.0, id).await?;
        }
        Ok(id)
    }

    async fn add_or_refresh_with_policy(
        &self,
        addon_id: &str,
        source_url: &str,
        requested_folder: Option<&str>,
        content_kind_override: AddonMediaListContentKind,
        chapter_input: &str,
        detail_import_policy: AddonMediaListDetailPolicy,
        delivery_mode: ersatzrs_domain::AddonMediaListDeliveryMode,
    ) -> Result<EntityId> {
        require_delivery_capability(&self.0, addon_id, delivery_mode).await?;
        AddonMediaListImportCoordinator::new(self.0.clone())
            .with_delivery_mode(delivery_mode)
            .add_or_refresh(
                addon_id,
                source_url,
                requested_folder,
                content_kind_override,
                chapter_input,
                detail_import_policy,
            )
            .await
    }

    async fn set_detail_import_policy(
        &self,
        id: EntityId,
        policy: AddonMediaListDetailPolicy,
    ) -> Result<()> {
        AddonMediaListImportCoordinator::new(self.0.clone())
            .set_detail_policy(id, policy)
            .await
    }

    async fn request_item_details(&self, list_id: EntityId, item_id: EntityId) -> Result<()> {
        AddonMediaListImportCoordinator::new(self.0.clone())
            .request_item_details(list_id, item_id)
            .await
    }

    async fn control_import(&self, id: EntityId, action: AddonMediaListImportAction) -> Result<()> {
        AddonMediaListImportCoordinator::new(self.0.clone())
            .control_import(id, action)
            .await
    }

    async fn managed_library_id(&self, id: EntityId) -> Result<Option<EntityId>> {
        crate::addon_media_storage::list_library_id(&self.0, id).await
    }

    async fn match_items(&self, id: EntityId) -> Result<()> {
        let form = self
            .get_admin_form(id)
            .await?
            .ok_or(RepositoryError::NotFound {
                kind: "add-on media list",
                id,
            })?;
        if form.review_state == AddonMediaListReviewState::Publishing {
            return Err(publication_in_progress());
        }
        if form.review_state != AddonMediaListReviewState::Published {
            return Ok(());
        }
        AddonMediaListSynchronizer::new(self.0.clone())
            .match_list_items(id)
            .await
    }

    async fn prepare_publication_details(&self, id: EntityId) -> Result<u64> {
        self.prepare_publication_details_impl(id, false).await
    }

    async fn prepare_publication_media(&self, id: EntityId, saved_revision: i32) -> Result<u64> {
        let mut excluded = self
            .list_items(id)
            .await?
            .into_iter()
            .filter(|item| item.included && access_restricted_without_replacement(item))
            .map(|item| item.id)
            .collect::<Vec<_>>();
        excluded.extend(sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "tombstone"."AddonMediaListItemId" FROM "AddonMediaListItemTombstone" AS "tombstone"
               JOIN "AddonMediaListItem" AS "item" ON "item"."Id"="tombstone"."AddonMediaListItemId"
               WHERE "item"."AddonMediaListId"=? AND "tombstone"."ConfirmedAt" IS NOT NULL"#,
        ).bind(id).fetch_all(&self.0).await.repository()?);
        crate::addon_media_list_publication_preparation::queue_required_media(
            &self.0,
            id,
            saved_revision,
            &excluded,
        )
        .await
    }

    async fn stage_publication(&self, id: EntityId) -> Result<AddonMediaListPublication> {
        stage_publication_impl(self, id, None).await
    }

    async fn stage_publication_revision(
        &self,
        id: EntityId,
        saved_revision: i32,
        operation_id: EntityId,
        generation: i32,
        owner_token: &str,
        lease_owner: &str,
    ) -> Result<AddonMediaListPublication> {
        stage_publication_impl(
            self,
            id,
            Some(PublicationFence {
                operation_id,
                generation,
                saved_revision,
                owner_token,
                lease_owner,
            }),
        )
        .await
    }

    async fn commit_staged_publication(&self, id: EntityId) -> Result<()> {
        commit_staged_publication_impl(self, id, None, None).await
    }

    async fn commit_staged_publication_revision(
        &self,
        id: EntityId,
        target_published_revision: i32,
        operation_id: EntityId,
        generation: i32,
        owner_token: &str,
        lease_owner: &str,
    ) -> Result<AddonMediaListPublicationContinuation> {
        let row = sqlx::query(
            r#"SELECT "ReviewState", "PublishedRevision", "PublicationJournalJson",
                      "PublicationStartedAt"
               FROM "AddonMediaList" WHERE "Id" = ?"#,
        )
        .bind(id)
        .fetch_optional(&self.0)
        .await
        .repository()?
        .ok_or(RepositoryError::NotFound {
            kind: "add-on media list",
            id,
        })?;
        let review_state: AddonMediaListReviewState =
            crate::repositories::common::decode_enum_column(
                "AddonMediaListReviewState",
                &row,
                "ReviewState",
            )?;
        let published_revision: i32 = row.try_get("PublishedRevision").repository()?;
        let journal: Option<String> = row.try_get("PublicationJournalJson").repository()?;
        if published_revision > target_published_revision {
            return Err(RepositoryError::Conflict(
                "the published add-on media-list revision changed before commit".to_owned(),
            ));
        }
        let fence = PublicationClaimFence {
            operation_id,
            generation,
            owner_token,
            lease_owner,
        };
        ensure_current_publication_claim(&self.0, id, fence, false).await?;
        let Some(journal) = journal.filter(|value| !value.trim().is_empty()) else {
            let publication_started_at: Option<String> =
                row.try_get("PublicationStartedAt").repository()?;
            if published_revision == target_published_revision
                && review_state == AddonMediaListReviewState::Published
                && publication_started_at.is_none()
            {
                ensure_current_publication_claim(&self.0, id, fence, false).await?;
                return Ok(AddonMediaListPublicationContinuation::CompletedLegacy);
            }
            return Err(RepositoryError::Conflict(
                "the add-on media list has no publication journal".to_owned(),
            ));
        };
        let publication_journal: PublicationJournal =
            serde_json::from_str(&journal).map_err(|error| RepositoryError::Decode {
                kind: "add-on publication journal",
                value: error.to_string(),
            })?;
        if publication_journal.operation_id != Some(operation_id)
            || publication_journal.generation != Some(generation)
            || publication_journal.target_published_revision != Some(target_published_revision)
        {
            return Err(RepositoryError::Conflict(
                "the staged add-on media-list publication belongs to another generation".to_owned(),
            ));
        }
        ensure_current_publication_claim(&self.0, id, fence, false).await?;
        if published_revision == target_published_revision {
            let now = crate::repositories::common::encode_datetime(chrono::Utc::now());
            let retained = sqlx::query(
                r#"UPDATE "AddonMediaList" SET "ReviewState" = ?
                   WHERE "Id" = ? AND "PublicationJournalJson" = ?
                     AND "PublishedRevision" = ? AND "ReviewState" IN (?, ?)
                     AND EXISTS (
                         SELECT 1
                         FROM "AddonMediaListPublicationJob" AS "job"
                         JOIN "ManagedLibraryOperation" AS "operation"
                           ON "operation"."Id" = "job"."OperationId"
                         JOIN "ManagedLibraryLease" AS "lease"
                           ON "lease"."OperationId" = "operation"."Id"
                         WHERE "job"."AddonMediaListId" = "AddonMediaList"."Id"
                           AND "job"."OperationId" = ? AND "job"."Generation" = ?
                           AND "operation"."State" = 1
                           AND "operation"."OwnerToken" = ?
                           AND "operation"."LeaseOwner" = ?
                           AND "lease"."OwnerToken" = ?
                           AND "lease"."LeaseOwner" = ?
                           AND "lease"."LeaseExpiresAt" > ?
                     )"#,
            )
            .bind(crate::repositories::common::encode_enum(
                AddonMediaListReviewState::Publishing,
            ))
            .bind(id)
            .bind(&journal)
            .bind(target_published_revision)
            .bind(crate::repositories::common::encode_enum(
                AddonMediaListReviewState::Published,
            ))
            .bind(crate::repositories::common::encode_enum(
                AddonMediaListReviewState::Publishing,
            ))
            .bind(operation_id)
            .bind(generation)
            .bind(owner_token)
            .bind(lease_owner)
            .bind(owner_token)
            .bind(lease_owner)
            .bind(now)
            .execute(&self.0)
            .await
            .repository()?;
            if retained.rows_affected() != 1 {
                return Err(RepositoryError::Conflict(
                    "the committed add-on publication changed before verification".to_owned(),
                ));
            }
            return Ok(AddonMediaListPublicationContinuation::VerificationRequired);
        }
        commit_staged_publication_impl(self, id, Some(target_published_revision), Some(fence))
            .await?;
        Ok(AddonMediaListPublicationContinuation::VerificationRequired)
    }

    async fn restore_staged_publication(&self, id: EntityId, message: &str) -> Result<()> {
        restore_staged_publication_impl(self, id, message, None).await
    }

    async fn restore_staged_publication_revision(
        &self,
        id: EntityId,
        message: &str,
        operation_id: EntityId,
        generation: i32,
        owner_token: &str,
        lease_owner: &str,
    ) -> Result<()> {
        let fence = PublicationClaimFence {
            operation_id,
            generation,
            owner_token,
            lease_owner,
        };
        ensure_current_publication_claim(&self.0, id, fence, true).await?;
        restore_staged_publication_impl(self, id, message, Some(fence)).await
    }

    async fn reconcile_restored_publication(&self, id: EntityId) -> Result<()> {
        reconcile_restored_publication_impl(self, id, None).await
    }

    async fn reconcile_restored_publication_revision(
        &self,
        id: EntityId,
        operation_id: EntityId,
        generation: i32,
        owner_token: &str,
        lease_owner: &str,
    ) -> Result<AddonMediaListPublicationContinuation> {
        let fence = PublicationClaimFence {
            operation_id,
            generation,
            owner_token,
            lease_owner,
        };
        ensure_current_publication_claim(&self.0, id, fence, false).await?;
        let row = sqlx::query(
            r#"SELECT "list"."ReviewState", "list"."PublishedRevision",
                      "list"."PublicationJournalJson", "list"."PublicationStartedAt",
                      "job"."TargetPublishedRevision"
               FROM "AddonMediaList" AS "list"
               JOIN "AddonMediaListPublicationJob" AS "job"
                 ON "job"."AddonMediaListId" = "list"."Id"
               WHERE "list"."Id" = ? AND "job"."OperationId" = ?
                 AND "job"."Generation" = ?"#,
        )
        .bind(id)
        .bind(operation_id)
        .bind(generation)
        .fetch_optional(&self.0)
        .await
        .repository()?
        .ok_or_else(|| {
            RepositoryError::Conflict(
                "the restored add-on publication job is no longer current".to_owned(),
            )
        })?;
        let journal: Option<String> = row.try_get("PublicationJournalJson").repository()?;
        if journal
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
        {
            let review_state: AddonMediaListReviewState =
                crate::repositories::common::decode_enum_column(
                    "AddonMediaListReviewState",
                    &row,
                    "ReviewState",
                )?;
            let published_revision: i32 = row.try_get("PublishedRevision").repository()?;
            let target_revision: i32 = row.try_get("TargetPublishedRevision").repository()?;
            let publication_started_at: Option<String> =
                row.try_get("PublicationStartedAt").repository()?;
            if review_state != AddonMediaListReviewState::Publishing
                && published_revision == target_revision.saturating_sub(1)
                && publication_started_at.is_none()
            {
                ensure_current_publication_claim(&self.0, id, fence, false).await?;
                return Ok(AddonMediaListPublicationContinuation::CompletedLegacy);
            }
            return Err(RepositoryError::Conflict(
                "the publishing list lost its publication journal".to_owned(),
            ));
        }
        ensure_current_publication_claim(&self.0, id, fence, true).await?;
        reconcile_restored_publication_impl(self, id, Some(fence)).await?;
        Ok(AddonMediaListPublicationContinuation::VerificationRequired)
    }

    async fn prepare_publication_verification_scan(
        &self,
        id: EntityId,
        outcome: AddonMediaListPublicationOutcome,
        operation_id: EntityId,
        generation: i32,
        owner_token: &str,
        lease_owner: &str,
    ) -> Result<()> {
        let fence = PublicationClaimFence {
            operation_id,
            generation,
            owner_token,
            lease_owner,
        };
        ensure_current_publication_claim(&self.0, id, fence, true).await?;
        let (journal, _) = load_publication_journal_value(&self.0, id).await?;
        // Journals predating tree markers retain their existing adoption path.
        if journal.failed_folder.is_none()
            || journal.previous_published_revision.is_none()
            || journal.target_published_revision.is_none()
        {
            return Ok(());
        }
        let (paths, identity) =
            publication_fs_context(&self.0, id, &journal, Some(operation_id), Some(generation))
                .await?;
        let role = publication_outcome_role(outcome);
        tokio::task::spawn_blocking(move || {
            addon_publication_fs::upgrade_managed_fingerprint(&paths, identity, role)
        })
        .await
        .map_err(|error| {
            RepositoryError::unavailable(
                "prepare publication verification",
                format!("marker upgrade task failed: {error}"),
            )
        })?
        .map_err(|error| publication_fs_repository_error(&error))?;
        ensure_current_publication_claim(&self.0, id, fence, true).await
    }

    async fn verify_staged_publication_revision(
        &self,
        id: EntityId,
        outcome: AddonMediaListPublicationOutcome,
        operation_id: EntityId,
        generation: i32,
        owner_token: &str,
        lease_owner: &str,
    ) -> Result<()> {
        let fence = PublicationClaimFence {
            operation_id,
            generation,
            owner_token,
            lease_owner,
        };
        ensure_current_publication_claim(&self.0, id, fence, true).await?;
        let (journal, _) = load_publication_journal_value(&self.0, id).await?;
        let (paths, identity) =
            publication_fs_context(&self.0, id, &journal, Some(operation_id), Some(generation))
                .await?;
        let legacy_marker_adoption = journal.failed_folder.is_none()
            || journal.previous_published_revision.is_none()
            || journal.target_published_revision.is_none();
        if legacy_marker_adoption {
            let legacy_paths = paths.clone();
            tokio::task::spawn_blocking(move || {
                addon_publication_fs::bootstrap_legacy_markers(&legacy_paths, identity)
            })
            .await
            .map_err(|error| {
                RepositoryError::unavailable(
                    "verify add-on publication",
                    format!("publication marker recovery task failed: {error}"),
                )
            })?
            .map_err(|error| publication_fs_repository_error(&error))?;
            if outcome == AddonMediaListPublicationOutcome::Published {
                ensure_current_publication_claim(&self.0, id, fence, true).await?;
                let prepare_paths = paths.clone();
                let prepared = tokio::task::spawn_blocking(move || {
                    addon_publication_fs::prepare_managed_candidate_seal(&prepare_paths, identity)
                })
                .await
                .map_err(|error| {
                    RepositoryError::unavailable(
                        "verify add-on publication",
                        format!("legacy publication fingerprint task failed: {error}"),
                    )
                })?
                .map_err(|error| publication_fs_repository_error(&error))?;
                ensure_current_publication_claim(&self.0, id, fence, true).await?;
                let persist_paths = paths.clone();
                tokio::task::spawn_blocking(move || {
                    addon_publication_fs::persist_prepared_candidate_seal(
                        &persist_paths,
                        identity,
                        &prepared,
                    )
                })
                .await
                .map_err(|error| {
                    RepositoryError::unavailable(
                        "verify add-on publication",
                        format!("legacy publication sealing task failed: {error}"),
                    )
                })?
                .map_err(|error| publication_fs_repository_error(&error))?;
            }
        }
        let role = publication_outcome_role(outcome);
        tokio::task::spawn_blocking(move || {
            addon_publication_fs::verify_managed_complete(&paths, identity, role)
        })
        .await
        .map_err(|error| {
            RepositoryError::unavailable(
                "verify add-on publication",
                format!("publication verification task failed: {error}"),
            )
        })?
        .map_err(|error| publication_fs_repository_error(&error))?;
        ensure_current_publication_claim(&self.0, id, fence, true).await?;
        verify_publication_database(
            &self.0,
            id,
            &journal,
            identity.previous_revision,
            identity.target_revision,
            outcome,
        )
        .await
    }

    async fn finalize_staged_publication_revision(
        &self,
        id: EntityId,
        outcome: AddonMediaListPublicationOutcome,
        operation_id: EntityId,
        generation: i32,
        owner_token: &str,
        lease_owner: &str,
    ) -> Result<()> {
        let fence = PublicationClaimFence {
            operation_id,
            generation,
            owner_token,
            lease_owner,
        };
        ensure_current_publication_claim(&self.0, id, fence, false).await?;
        let (journal, journal_json) = match load_publication_journal_value(&self.0, id).await {
            Ok(value) => value,
            Err(RepositoryError::Conflict(_)) => {
                let finalized: i64 = sqlx::query_scalar(
                    r#"SELECT COUNT(*)
                       FROM "AddonMediaList" AS "list"
                       JOIN "AddonMediaListPublicationJob" AS "job"
                         ON "job"."AddonMediaListId" = "list"."Id"
                       WHERE "list"."Id" = ?
                         AND "list"."PublicationJournalJson" IS NULL
                         AND "list"."PublicationStartedAt" IS NULL
                         AND "job"."OperationId" = ? AND "job"."Generation" = ?
                         AND "job"."Phase" = ? AND "job"."Outcome" = ?"#,
                )
                .bind(id)
                .bind(operation_id)
                .bind(generation)
                .bind(crate::repositories::common::encode_enum(
                    ersatzrs_domain::AddonMediaListPublicationPhase::CleaningUp,
                ))
                .bind(crate::repositories::common::encode_enum(outcome))
                .fetch_one(&self.0)
                .await
                .repository()?;
                if finalized == 1 {
                    return Ok(());
                }
                return Err(RepositoryError::Conflict(
                    "the add-on publication journal disappeared before finalization".to_owned(),
                ));
            }
            Err(error) => return Err(error),
        };
        ensure_current_publication_claim(&self.0, id, fence, true).await?;
        let (paths, identity) =
            publication_fs_context(&self.0, id, &journal, Some(operation_id), Some(generation))
                .await?;
        let role = publication_outcome_role(outcome);
        let verify_paths = paths.clone();
        tokio::task::spawn_blocking(move || {
            addon_publication_fs::verify_managed_complete(&verify_paths, identity, role)
        })
        .await
        .map_err(|error| {
            RepositoryError::unavailable(
                "finalize add-on publication",
                format!("publication verification task failed: {error}"),
            )
        })?
        .map_err(|error| publication_fs_repository_error(&error))?;
        ensure_current_publication_claim(&self.0, id, fence, true).await?;
        verify_publication_database(
            &self.0,
            id,
            &journal,
            identity.previous_revision,
            identity.target_revision,
            outcome,
        )
        .await?;
        ensure_current_publication_claim(&self.0, id, fence, true).await?;
        if outcome == AddonMediaListPublicationOutcome::Published {
            retain_published_revision_before_cleanup(self, id, &journal, &paths, identity, fence)
                .await?;
            ensure_current_publication_claim(&self.0, id, fence, true).await?;
        }
        tokio::task::spawn_blocking(move || {
            addon_publication_fs::cleanup_after_verification(&paths, identity, role)
        })
        .await
        .map_err(|error| {
            RepositoryError::unavailable(
                "finalize add-on publication",
                format!("publication cleanup task failed: {error}"),
            )
        })?
        .map_err(|error| publication_fs_repository_error(&error))?;
        ensure_current_publication_claim(&self.0, id, fence, true).await?;

        let now = crate::repositories::common::encode_datetime(chrono::Utc::now());
        let terminal_review_state = match outcome {
            AddonMediaListPublicationOutcome::Published => AddonMediaListReviewState::Published,
            AddonMediaListPublicationOutcome::RestoredAfterFailure => journal.previous_review_state,
        };
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        if outcome == AddonMediaListPublicationOutcome::Published {
            accept_reconcile_candidate_in_tx(&mut tx, id, &now).await?;
            crate::addon_media_list_auto_publication::published_in_tx(&mut tx, id).await?;
        }
        let cleared = sqlx::query(
            r#"UPDATE "AddonMediaList"
               SET "ReviewState" = ?, "PublicationJournalJson" = NULL,
                   "PublicationStartedAt" = NULL
               WHERE "Id" = ? AND "PublicationJournalJson" = ?
                 AND EXISTS (
                     SELECT 1
                     FROM "AddonMediaListPublicationJob" AS "job"
                     JOIN "ManagedLibraryOperation" AS "operation"
                       ON "operation"."Id" = "job"."OperationId"
                     JOIN "ManagedLibraryLease" AS "lease"
                       ON "lease"."OperationId" = "operation"."Id"
                     WHERE "job"."AddonMediaListId" = "AddonMediaList"."Id"
                       AND "job"."OperationId" = ? AND "job"."Generation" = ?
                       AND "job"."Phase" = ? AND "job"."Outcome" = ?
                       AND "operation"."State" = 1
                       AND "operation"."OwnerToken" = ?
                       AND "operation"."LeaseOwner" = ?
                       AND "lease"."OwnerToken" = ?
                       AND "lease"."LeaseOwner" = ?
                       AND "lease"."LeaseExpiresAt" > ?
                 )"#,
        )
        .bind(crate::repositories::common::encode_enum(
            terminal_review_state,
        ))
        .bind(id)
        .bind(&journal_json)
        .bind(operation_id)
        .bind(generation)
        .bind(crate::repositories::common::encode_enum(
            ersatzrs_domain::AddonMediaListPublicationPhase::CleaningUp,
        ))
        .bind(crate::repositories::common::encode_enum(outcome))
        .bind(owner_token)
        .bind(lease_owner)
        .bind(owner_token)
        .bind(lease_owner)
        .bind(now)
        .execute(&mut *tx)
        .await
        .repository()?;
        if cleared.rows_affected() != 1 {
            return Err(RepositoryError::Conflict(
                "the verified add-on publication changed before finalization".to_owned(),
            ));
        }
        tx.commit().await.repository()
    }

    async fn stage_editor_artwork(
        &self,
        list_id: EntityId,
        item_id: Option<EntityId>,
        role: AddonMediaListArtworkRole,
        content_type: &str,
        bytes: &[u8],
    ) -> Result<String> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let url =
            editor_artwork::stage(&mut tx, list_id, item_id, role, content_type, bytes).await?;
        tx.commit().await.repository()?;
        Ok(url)
    }

    async fn upload_artwork(
        &self,
        list_id: EntityId,
        item_id: Option<EntityId>,
        role: AddonMediaListArtworkRole,
        content_type: &str,
        bytes: &[u8],
        expected_etag: &str,
    ) -> Result<AddonMediaListNfoForm> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        crate::addon_media_list_sync::ensure_not_publishing_tx(&mut tx, list_id).await?;
        if let Some(item_id) = item_id {
            require_draft_item_tx(&mut tx, list_id, item_id).await?;
        }
        let current = editor_artwork::metadata(&mut tx, list_id, item_id).await?;
        if current.etag != expected_etag {
            return Err(RepositoryError::Conflict(
                "metadata changed since the editor was loaded".into(),
            ));
        }
        let url =
            editor_artwork::stage(&mut tx, list_id, item_id, role, content_type, bytes).await?;
        let mut metadata = current.metadata;
        metadata.artwork.retain(|candidate| candidate.role != role);
        metadata.artwork.push(AddonMediaListArtwork {
            url,
            role,
            width: None,
            height: None,
            language: None,
        });
        let provider_json: String = if let Some(item_id) = item_id {
            sqlx::query_scalar(
                r#"SELECT "ProviderMetadataJson" FROM "AddonMediaListItem"
                   WHERE "Id" = ? AND "AddonMediaListId" = ?"#,
            )
            .bind(item_id)
            .bind(list_id)
            .fetch_one(&mut *tx)
            .await
            .repository()?
        } else {
            sqlx::query_scalar(
                r#"SELECT "ProviderMetadataJson" FROM "AddonMediaList" WHERE "Id" = ?"#,
            )
            .bind(list_id)
            .fetch_one(&mut *tx)
            .await
            .repository()?
        };
        let overrides = metadata_override_json(&metadata_json_value(&provider_json)?, &metadata)?;
        if let Some(item_id) = item_id {
            sqlx::query(
                r#"UPDATE "AddonMediaListItem"
                   SET "MetadataOverrideJson" = ?, "MetadataConflictJson" = NULL
                   WHERE "Id" = ? AND "AddonMediaListId" = ?"#,
            )
            .bind(overrides)
            .bind(item_id)
            .bind(list_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        } else {
            sqlx::query(
                r#"UPDATE "AddonMediaList"
                   SET "MetadataOverrideJson" = ?, "MetadataConflictJson" = NULL
                   WHERE "Id" = ?"#,
            )
            .bind(overrides)
            .bind(list_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        }
        mark_list_ready_tx(&mut tx, list_id).await?;
        tx.commit().await.repository()?;
        match item_id {
            Some(item_id) => load_item_metadata_form(&self.0, list_id, item_id).await,
            None => load_list_metadata_form(&self.0, list_id).await,
        }
    }

    async fn get_artwork_upload(
        &self,
        upload_id: EntityId,
    ) -> Result<Option<AddonMediaListArtworkUpload>> {
        sqlx::query_as::<_, (String, Vec<u8>)>(
            r#"SELECT "ContentType", "Bytes"
               FROM "AddonMediaListArtworkUpload" WHERE "Id" = ?"#,
        )
        .bind(upload_id)
        .fetch_optional(&self.0)
        .await
        .repository()
        .map(|row| {
            row.map(|(content_type, bytes)| AddonMediaListArtworkUpload {
                content_type,
                bytes,
            })
        })
    }

    async fn set_item_included(
        &self,
        list_id: EntityId,
        item_id: EntityId,
        included: bool,
    ) -> Result<()> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let updated = sqlx::query(
            r#"UPDATE "AddonMediaListItem" SET "Included" = ?
               WHERE "Id" = ? AND "AddonMediaListId" = ? AND "InDraft" = 1"#,
        )
        .bind(included)
        .bind(item_id)
        .bind(list_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        if updated.rows_affected() == 0 {
            return Err(RepositoryError::NotFound {
                kind: "add-on media list item",
                id: item_id,
            });
        }
        mark_list_ready_tx(&mut tx, list_id).await?;
        tx.commit().await.repository()?;
        Ok(())
    }

    async fn set_item_collection_role(
        &self,
        list_id: EntityId,
        item_id: EntityId,
        role: AddonMediaListItemRole,
    ) -> Result<()> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        if role == AddonMediaListItemRole::Primary {
            // One movie collection can have only one feature. Clearing the old
            // primary is explicit and never changes the provider order.
            sqlx::query(
                r#"UPDATE "AddonMediaListItem" SET "CollectionRole" = 2
                   WHERE "AddonMediaListId" = ? AND "CollectionRole" = 1 AND "InDraft" = 1"#,
            )
            .bind(list_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        }
        let updated = sqlx::query(
            r#"UPDATE "AddonMediaListItem" SET "CollectionRole" = ?
               WHERE "Id" = ? AND "AddonMediaListId" = ? AND "InDraft" = 1"#,
        )
        .bind(crate::repositories::common::encode_enum(role))
        .bind(item_id)
        .bind(list_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        if updated.rows_affected() == 0 {
            return Err(RepositoryError::NotFound {
                kind: "add-on media list item",
                id: item_id,
            });
        }
        mark_list_ready_tx(&mut tx, list_id).await?;
        tx.commit().await.repository()?;
        Ok(())
    }

    async fn change_addon(&self, id: EntityId, addon_id: &str) -> Result<()> {
        let policy = crate::addon_media_list_delivery::load(&self.0, id).await?;
        require_delivery_capability(&self.0, addon_id, policy.mode).await?;
        crate::addon_media_list_import::AddonMediaListImportCoordinator::new(self.0.clone())
            .change_addon(id, addon_id)
            .await
    }

    async fn change_editor_addon(
        &self,
        id: EntityId,
        addon_id: &str,
        expected_revision: i32,
    ) -> Result<()> {
        let policy = crate::addon_media_list_delivery::load(&self.0, id).await?;
        require_delivery_capability(&self.0, addon_id, policy.mode).await?;
        crate::addon_media_list_import::AddonMediaListImportCoordinator::new(self.0.clone())
            .with_editor_revision(expected_revision)
            .change_addon(id, addon_id)
            .await
    }

    async fn update_settings(
        &self,
        id: EntityId,
        refresh_cadence: AddonMediaListRefreshCadence,
        refresh_publication_policy: ersatzrs_domain::AddonMediaListRefreshPublicationPolicy,
        delivery_mode: ersatzrs_domain::AddonMediaListDeliveryMode,
        generate_playlist: bool,
        content_kind_override: AddonMediaListContentKind,
        chapter_input: &str,
    ) -> Result<()> {
        let existing = self
            .get_admin_form(id)
            .await?
            .ok_or(RepositoryError::NotFound {
                kind: "add-on media list",
                id,
            })?;
        let cadence_changed = existing.refresh_cadence != refresh_cadence;
        require_delivery_capability(&self.0, &existing.addon_id, delivery_mode).await?;
        if cadence_changed
            && refresh_cadence == AddonMediaListRefreshCadence::Adaptive
            && !InstalledAddonMediaListProvider::new(self.0.clone())
                .supports_source_check(&existing.addon_id)
                .await?
        {
            return Err(RepositoryError::unavailable(
                "enable adaptive add-on list refresh",
                "the selected add-on does not declare media-list.source-check.v1",
            ));
        }
        let now = Utc::now();
        let next_refresh_at = if cadence_changed {
            match refresh_cadence {
                AddonMediaListRefreshCadence::Never => None,
                AddonMediaListRefreshCadence::Adaptive => Some(encode_datetime(now)),
                cadence => existing
                    .last_refresh_succeeded_at
                    .as_deref()
                    .map(parse_admin_datetime)
                    .transpose()?
                    .and_then(|anchor| {
                        crate::addon_media_list_refresh::next_fixed_refresh_at(cadence, anchor)
                    })
                    .map_or_else(
                        || Some(encode_datetime(now)),
                        |due| Some(encode_datetime(due)),
                    ),
            }
        } else {
            existing.next_refresh_at.clone()
        };
        // A library-type edit changes only the local publication projection.
        // The imported provider snapshot is already durable, so changing its
        // list-wide content-kind override must not execute the add-on again.
        // Legacy chapter input remains provider input and therefore still
        // requires re-enumeration when that separate setting changes.
        if existing.legacy_chapter_input != chapter_input {
            let provider = InstalledAddonMediaListProvider::new(self.0.clone());
            AddonMediaListRemoteSynchronizer::new(self.0.clone(), provider)
                .add_or_refresh(
                    &existing.addon_id,
                    &existing.source_url,
                    None,
                    content_kind_override,
                    chapter_input,
                )
                .await?;
        }
        AddonMediaListSynchronizer::new(self.0.clone())
            .update_settings(
                id,
                refresh_cadence,
                refresh_publication_policy,
                delivery_mode,
                next_refresh_at.as_deref(),
                cadence_changed,
                generate_playlist,
                content_kind_override,
                chapter_input,
            )
            .await
    }

    async fn request_presentation(
        &self,
        id: EntityId,
        presentation_kind: AddonMediaListPresentationKind,
        _show_title: Option<&str>,
    ) -> Result<()> {
        let _form = self
            .get_admin_form(id)
            .await?
            .ok_or(RepositoryError::NotFound {
                kind: "add-on media list",
                id,
            })?;
        // A presentation without a projection would be stored and then quietly
        // do nothing, leaving the list claiming a library kind it never entered.
        if !presentation_kind.is_projected() {
            return Err(RepositoryError::Conflict(format!(
                "presentation {presentation_kind:?} has no projection yet"
            )));
        }
        crate::addon_media_list_sync::ensure_not_publishing(&self.0, id).await?;
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let updated = sqlx::query(
            r#"UPDATE "AddonMediaList"
               SET "RequestedPresentationKind" = ?, "PresentationError" = NULL
               WHERE "Id" = ?"#,
        )
        .bind(crate::repositories::common::encode_enum(presentation_kind))
        .bind(id)
        .execute(&mut *tx)
        .await
        .repository()?;
        if updated.rows_affected() != 1 {
            return Err(publication_in_progress());
        }
        mark_list_ready_tx(&mut tx, id).await?;
        tx.commit().await.repository()?;
        Ok(())
    }

    async fn save_editor_task(
        &self,
        save: &ersatzrs_core::repositories::AddonMediaListEditorSave,
    ) -> Result<i32> {
        editor::save(&self.0, save).await
    }

    async fn save_show_nfo(
        &self,
        id: EntityId,
        metadata: &AddonMediaListNfoMetadata,
        expected_etag: &str,
    ) -> Result<()> {
        let current = load_list_metadata_form(&self.0, id).await?;
        let accepted_etag = if current.etag == expected_etag {
            current.etag
        } else {
            let path = self.parent_nfo_path(id).await?;
            let disk = crate::addon_media_nfo::read_form(path.as_deref()).map_err(map_nfo_error)?;
            if disk.etag != expected_etag {
                return Err(RepositoryError::Conflict(
                    "metadata changed since the editor was loaded".to_owned(),
                ));
            }
            current.etag
        };
        save_list_metadata_override(&self.0, id, metadata, &accepted_etag).await
    }

    async fn save_item_nfo(
        &self,
        list_id: EntityId,
        item_id: EntityId,
        metadata: &AddonMediaListNfoMetadata,
        expected_etag: &str,
    ) -> Result<()> {
        let current = load_item_metadata_form(&self.0, list_id, item_id).await?;
        let accepted_etag = if current.etag == expected_etag {
            current.etag
        } else {
            let path = item_nfo_path_for(&self.0, list_id, item_id).await?;
            let disk = crate::addon_media_nfo::read_form(path.as_deref()).map_err(map_nfo_error)?;
            if disk.etag != expected_etag {
                return Err(RepositoryError::Conflict(
                    "metadata changed since the editor was loaded".to_owned(),
                ));
            }
            current.etag
        };
        save_item_metadata_override(&self.0, list_id, item_id, metadata, &accepted_etag).await
    }

    async fn reset_nfo(&self, list_id: EntityId, item_id: Option<EntityId>) -> Result<()> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        if let Some(item_id) = item_id {
            require_draft_item_tx(&mut tx, list_id, item_id).await?;
            sqlx::query(
                r#"UPDATE "AddonMediaListItem"
                   SET "MetadataOverrideJson" = '{}', "MetadataConflictJson" = NULL
                   WHERE "Id" = ? AND "AddonMediaListId" = ?"#,
            )
            .bind(item_id)
            .bind(list_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        } else {
            sqlx::query(
                r#"UPDATE "AddonMediaList"
                   SET "MetadataOverrideJson" = '{}', "MetadataConflictJson" = NULL
                   WHERE "Id" = ?"#,
            )
            .bind(list_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        }
        mark_list_ready_tx(&mut tx, list_id).await?;
        tx.commit().await.repository()?;
        Ok(())
    }

    async fn update_item_chapters(&self, item_id: EntityId, chapter_input: &str) -> Result<()> {
        let row = sqlx::query(
            r#"SELECT "item"."AddonMediaListId", "item"."ParentProviderId",
                      "list"."AddonId", "list"."SourceUrl", "list"."ContentKindOverride"
               FROM "AddonMediaListItem" AS "item"
               JOIN "AddonMediaList" AS "list" ON "list"."Id" = "item"."AddonMediaListId"
               WHERE "item"."Id" = ? AND "item"."InDraft" = 1"#,
        )
        .bind(item_id)
        .fetch_optional(&self.0)
        .await
        .repository()?
        .ok_or(RepositoryError::NotFound {
            kind: "add-on media list item",
            id: item_id,
        })?;
        if row
            .try_get::<Option<String>, _>("ParentProviderId")
            .repository()?
            .is_some()
        {
            return Err(RepositoryError::Conflict(
                "chapters can only be edited on a full source item".to_owned(),
            ));
        }
        let list_id: EntityId = row.try_get("AddonMediaListId").repository()?;
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        require_draft_item_tx(&mut tx, list_id, item_id).await?;
        sqlx::query(r#"UPDATE "AddonMediaListItem" SET "ChapterInput" = ? WHERE "Id" = ?"#)
            .bind((!chapter_input.trim().is_empty()).then_some(chapter_input.trim()))
            .bind(item_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        mark_list_ready_tx(&mut tx, list_id).await?;
        tx.commit().await.repository()?;
        let provider = InstalledAddonMediaListProvider::new(self.0.clone());
        AddonMediaListRemoteSynchronizer::new(self.0.clone(), provider)
            .add_or_refresh(
                row.try_get::<String, _>("AddonId").repository()?.as_str(),
                row.try_get::<String, _>("SourceUrl").repository()?.as_str(),
                None,
                crate::repositories::common::decode_enum_column(
                    "AddonMediaListContentKind",
                    &row,
                    "ContentKindOverride",
                )?,
                "",
            )
            .await?;
        Ok(())
    }

    async fn set_admin_hidden(&self, id: EntityId, hidden: bool) -> Result<()> {
        let updated =
            sqlx::query(r#"UPDATE "AddonMediaList" SET "AdminHidden" = ? WHERE "Id" = ?"#)
                .bind(hidden)
                .bind(id)
                .execute(&self.0)
                .await
                .repository()?;
        if updated.rows_affected() == 0 {
            return Err(RepositoryError::NotFound {
                kind: "add-on media list",
                id,
            });
        }
        Ok(())
    }

    async fn delete(&self, id: EntityId) -> Result<()> {
        AddonMediaListSynchronizer::new(self.0.clone())
            .delete_list(id)
            .await
    }

    async fn get_page_size(&self) -> Result<u32> {
        let value: Option<String> =
            sqlx::query_scalar(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
                .bind(PAGE_SIZE_KEY)
                .fetch_optional(&self.0)
                .await
                .repository()?;
        Ok(value
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|value| [0, 10, 25, 50, 100].contains(value))
            .unwrap_or(10))
    }

    async fn set_page_size(&self, page_size: u32) -> Result<()> {
        let updated = sqlx::query(r#"UPDATE "ConfigElement" SET "Value" = ? WHERE "Key" = ?"#)
            .bind(page_size.to_string())
            .bind(PAGE_SIZE_KEY)
            .execute(&self.0)
            .await
            .repository()?;
        if updated.rows_affected() == 0 {
            sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
                .bind(PAGE_SIZE_KEY)
                .bind(page_size.to_string())
                .execute(&self.0)
                .await
                .repository()?;
        }
        Ok(())
    }
}

fn decode_optional_u64(row: &crate::db::DbRow, column: &'static str) -> Result<Option<u64>> {
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

fn resolve_included_presentation(
    items: &[&AddonMediaListItemAdminRow],
) -> AddonMediaListPresentationKind {
    let mut kinds = items
        .iter()
        .filter(|item| item.parent_provider_id.is_none())
        .map(|item| item.content_kind);
    let Some(only) = kinds.next() else {
        return AddonMediaListPresentationKind::RemoteStreams;
    };
    if kinds.any(|kind| kind != only) {
        return AddonMediaListPresentationKind::RemoteStreams;
    }
    AddonMediaListPresentationKind::from_content_kind(only)
        .filter(|presentation| presentation.is_projected())
        .unwrap_or(AddonMediaListPresentationKind::RemoteStreams)
}

async fn load_publication_journal(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
) -> Result<PublicationJournal> {
    load_publication_journal_value(pool, list_id)
        .await
        .map(|(journal, _)| journal)
}

async fn load_publication_journal_value(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
) -> Result<(PublicationJournal, String)> {
    let value = sqlx::query_scalar::<_, Option<String>>(
        r#"SELECT "PublicationJournalJson" FROM "AddonMediaList" WHERE "Id" = ?"#,
    )
    .bind(list_id)
    .fetch_optional(pool)
    .await
    .repository()?
    .ok_or(RepositoryError::NotFound {
        kind: "add-on media list",
        id: list_id,
    })?
    .filter(|value| !value.trim().is_empty())
    .ok_or_else(|| {
        RepositoryError::Conflict("the add-on media list has no publication journal".to_owned())
    })?;
    let journal = serde_json::from_str(&value).map_err(|error| RepositoryError::Decode {
        kind: "add-on publication journal",
        value: error.to_string(),
    })?;
    Ok((journal, value))
}

#[allow(clippy::too_many_arguments)]
async fn ensure_current_publication_fence(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    operation_id: EntityId,
    generation: i32,
    owner_token: &str,
    lease_owner: &str,
    require_journal: bool,
) -> Result<()> {
    ensure_current_publication_claim(
        pool,
        list_id,
        PublicationClaimFence {
            operation_id,
            generation,
            owner_token,
            lease_owner,
        },
        require_journal,
    )
    .await
}

async fn ensure_current_publication_claim(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    fence: PublicationClaimFence<'_>,
    require_journal: bool,
) -> Result<()> {
    let now = crate::repositories::common::encode_datetime(chrono::Utc::now());
    let current = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*)
           FROM "AddonMediaListPublicationJob" AS "job"
           JOIN "ManagedLibraryOperation" AS "operation"
             ON "operation"."Id" = "job"."OperationId"
           JOIN "ManagedLibraryLease" AS "lease"
             ON "lease"."OperationId" = "operation"."Id"
           WHERE "job"."AddonMediaListId" = ?
             AND "job"."OperationId" = ? AND "job"."Generation" = ?
             AND "operation"."State" = 1
             AND "operation"."OwnerToken" = ? AND "operation"."LeaseOwner" = ?
             AND "lease"."OwnerToken" = ? AND "lease"."LeaseOwner" = ?
             AND "lease"."LeaseExpiresAt" > ?"#,
    )
    .bind(list_id)
    .bind(fence.operation_id)
    .bind(fence.generation)
    .bind(fence.owner_token)
    .bind(fence.lease_owner)
    .bind(fence.owner_token)
    .bind(fence.lease_owner)
    .bind(now)
    .fetch_one(pool)
    .await
    .repository()?;
    if current != 1 {
        return Err(RepositoryError::Conflict(
            "the add-on publication lease is no longer current".to_owned(),
        ));
    }
    if require_journal {
        let journal = load_publication_journal(pool, list_id).await?;
        let legacy_unfenced = journal.operation_id.is_none() && journal.generation.is_none();
        if !legacy_unfenced
            && (journal.operation_id != Some(fence.operation_id)
                || journal.generation != Some(fence.generation))
        {
            return Err(RepositoryError::Conflict(
                "the staged add-on media-list publication belongs to another generation".to_owned(),
            ));
        }
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
async fn commit_staged_publication_impl(
    repository: &SqliteAddonMediaListRepository,
    id: EntityId,
    target_published_revision: Option<i32>,
    fence: Option<PublicationClaimFence<'_>>,
) -> Result<()> {
    let (mut journal, journal_json) = load_publication_journal_value(&repository.0, id).await?;
    let form = repository
        .get_admin_form(id)
        .await?
        .ok_or(RepositoryError::NotFound {
            kind: "add-on media list",
            id,
        })?;
    if form.review_state != AddonMediaListReviewState::Publishing {
        return Err(RepositoryError::Conflict(
            "the add-on media list has no staged publication".to_owned(),
        ));
    }
    if let Some(fence) = fence {
        ensure_current_publication_claim(&repository.0, id, fence, true).await?;
    }
    let (source, materialized_as_draft) = repository.publication_commit_source(id).await?;
    let preserve_draft = source.is_some() && !materialized_as_draft;
    let generate_playlist = source
        .as_ref()
        .map_or(form.generate_playlist, |source| source.generate_playlist);
    let effective_presentation = if let Some(source) = &source {
        source.presentation_kind
    } else if form.requested_presentation_kind == AddonMediaListPresentationKind::Auto {
        resolve_auto_presentation(&repository.0, id).await?
    } else {
        form.requested_presentation_kind
    };
    let synchronizer = AddonMediaListSynchronizer::new(repository.0.clone());
    let generated_playlist_removal = if generate_playlist {
        None
    } else {
        synchronizer.prepare_generated_playlist_removal(id).await?
    };
    if let Some(fence) = fence {
        ensure_current_publication_claim(&repository.0, id, fence, true).await?;
    }
    if matches!(
        effective_presentation,
        AddonMediaListPresentationKind::TelevisionShow
            | AddonMediaListPresentationKind::MusicVideos
            | AddonMediaListPresentationKind::Movies
            | AddonMediaListPresentationKind::OtherVideos
            | AddonMediaListPresentationKind::Songs
            | AddonMediaListPresentationKind::Images
    ) {
        // A first publication has scanned sources but ordinary matching is frozen
        // by its journal. Bind before choosing parent and item NFO paths: scanned
        // streams may live in a subfolder of the list manifest's directory.
        synchronizer
            .bind_list_items_for_publication(id, source.as_ref())
            .await?;
        if let Some(fence) = fence {
            ensure_current_publication_claim(&repository.0, id, fence, true).await?;
        }
    }
    if parent_nfo_for(effective_presentation).is_some() {
        ensure_parent_nfo_path_preserves_metadata(
            &repository.0,
            id,
            effective_presentation,
            source.as_ref(),
        )
        .await?;
        ensure_generated_parent_nfo(
            &repository.0,
            id,
            effective_presentation,
            None,
            source.as_ref(),
        )
        .await?;
        if let Some(fence) = fence {
            ensure_current_publication_claim(&repository.0, id, fence, true).await?;
        }
    }
    reconcile_projection(
        &repository.0,
        id,
        form.requested_presentation_kind,
        source.as_ref(),
    )
    .await?;
    if let Some(fence) = fence {
        ensure_current_publication_claim(&repository.0, id, fence, true).await?;
    }
    synchronizer
        .match_list_items_for_publication(id, source.as_ref())
        .await?;
    if let Some(fence) = fence {
        ensure_current_publication_claim(&repository.0, id, fence, true).await?;
    }

    let publication_revision =
        target_published_revision.unwrap_or_else(|| form.published_revision.saturating_add(1));
    let mut target_materializations = load_artwork_materializations(&repository.0, id).await?;
    for materialization in &mut target_materializations {
        materialization.published_revision = publication_revision;
    }
    let next_journal_json = if fence.is_some() {
        let published_item_count: i64 = if let Some(source) = &source {
            i64::try_from(
                source
                    .items
                    .iter()
                    .filter(|item| item.published && item.parent_provider_id.is_none())
                    .count(),
            )
            .map_err(|_| {
                RepositoryError::Conflict("published item count exceeds supported limits".into())
            })?
        } else {
            sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM "AddonMediaListItem"
               WHERE "AddonMediaListId" = ? AND "ParentProviderId" IS NULL
                 AND "Published" = 1"#,
            )
            .bind(id)
            .fetch_one(&repository.0)
            .await
            .repository()?
        };
        journal.target_database = Some(PublicationDatabaseSnapshot {
            presentation_kind: effective_presentation,
            published_item_count,
            items: load_published_item_snapshots(&repository.0, id).await?,
            playlist: if generate_playlist {
                load_playlist_snapshot(&repository.0, id).await?
            } else {
                None
            },
            materializations: target_materializations.clone(),
        });
        Some(
            serde_json::to_string(&journal).map_err(|error| RepositoryError::Decode {
                kind: "add-on publication journal",
                value: error.to_string(),
            })?,
        )
    } else {
        None
    };
    if let Some(fence) = fence {
        ensure_current_publication_claim(&repository.0, id, fence, true).await?;
    }
    let (paths, identity) = publication_fs_context(
        &repository.0,
        id,
        &journal,
        fence.map(|fence| fence.operation_id),
        fence.map(|fence| fence.generation),
    )
    .await?;
    if let Some(fence) = fence {
        ensure_current_publication_claim(&repository.0, id, fence, true).await?;
    }
    let prepare_paths = paths.clone();
    let prepared = tokio::task::spawn_blocking(move || {
        addon_publication_fs::prepare_managed_candidate_seal(&prepare_paths, identity)
    })
    .await
    .map_err(|error| {
        RepositoryError::unavailable(
            "seal add-on publication",
            format!("publication fingerprint task failed: {error}"),
        )
    })?
    .map_err(|error| publication_fs_repository_error(&error))?;
    if let Some(fence) = fence {
        ensure_current_publication_claim(&repository.0, id, fence, true).await?;
    }
    let persist_paths = paths.clone();
    tokio::task::spawn_blocking(move || {
        addon_publication_fs::persist_prepared_candidate_seal(&persist_paths, identity, &prepared)
    })
    .await
    .map_err(|error| {
        RepositoryError::unavailable(
            "seal add-on publication",
            format!("publication sealing task failed: {error}"),
        )
    })?
    .map_err(|error| publication_fs_repository_error(&error))?;
    if let Some(fence) = fence {
        ensure_current_publication_claim(&repository.0, id, fence, true).await?;
    }

    let mut commit = crate::db::begin_write(&repository.0).await.repository()?;
    if let Some(removal) = &generated_playlist_removal {
        synchronizer
            .commit_generated_playlist_removal_in_tx(&mut commit, id, removal)
            .await?;
    }
    sqlx::query(
        r#"UPDATE "AddonMediaListArtworkMaterialization"
           SET "PublishedRevision" = ? WHERE "AddonMediaListId" = ?"#,
    )
    .bind(publication_revision)
    .bind(id)
    .execute(&mut *commit)
    .await
    .repository()?;
    let now = crate::repositories::common::encode_datetime(chrono::Utc::now());
    let updated = if let (Some(fence), Some(target_published_revision)) =
        (fence, target_published_revision)
    {
        sqlx::query(
            r#"UPDATE "AddonMediaList"
               SET "ReviewState" = 3, "PublishedRevision" = ?,
                   "ReviewError" = NULL, "PresentationError" = NULL,
                   "PublicationJournalJson" = ?,
                   "DetailImportAuthorizedAt" = CASE WHEN ? THEN "DetailImportAuthorizedAt"
                       ELSE COALESCE("DetailImportAuthorizedAt", ?) END,
                   "DetailProjectionPendingAt" = CASE WHEN ? THEN "DetailProjectionPendingAt" ELSE NULL END
               WHERE "Id" = ? AND "PublicationJournalJson" = ?
                 AND "PublishedRevision" = ?
                 AND EXISTS (
                     SELECT 1
                     FROM "AddonMediaListPublicationJob" AS "job"
                     JOIN "ManagedLibraryOperation" AS "operation"
                       ON "operation"."Id" = "job"."OperationId"
                     JOIN "ManagedLibraryLease" AS "lease"
                       ON "lease"."OperationId" = "operation"."Id"
                     WHERE "job"."AddonMediaListId" = "AddonMediaList"."Id"
                       AND "job"."OperationId" = ? AND "job"."Generation" = ?
                       AND "operation"."State" = 1
                       AND "operation"."OwnerToken" = ?
                       AND "operation"."LeaseOwner" = ?
                       AND "lease"."OwnerToken" = ?
                       AND "lease"."LeaseOwner" = ?
                       AND "lease"."LeaseExpiresAt" > ?
                 )"#,
        )
        .bind(target_published_revision)
        .bind(next_journal_json.as_deref().ok_or_else(|| {
            RepositoryError::Conflict(
                "the durable publication target snapshot was not recorded".to_owned(),
            )
        })?)
        .bind(preserve_draft)
        .bind(&now)
        .bind(preserve_draft)
        .bind(id)
        .bind(&journal_json)
        .bind(journal.previous_published_revision.ok_or_else(|| {
            RepositoryError::Conflict(
                "the durable publication journal has no previous revision".into(),
            )
        })?)
        .bind(fence.operation_id)
        .bind(fence.generation)
        .bind(fence.owner_token)
        .bind(fence.lease_owner)
        .bind(fence.owner_token)
        .bind(fence.lease_owner)
        .bind(&now)
        .execute(&mut *commit)
        .await
        .repository()?
    } else {
        sqlx::query(
            r#"UPDATE "AddonMediaList"
               SET "ReviewState" = 2, "PublishedRevision" = "PublishedRevision" + 1,
                   "ReviewError" = NULL, "PresentationError" = NULL,
                   "PublicationJournalJson" = NULL, "PublicationStartedAt" = NULL,
                   "DetailImportAuthorizedAt" = CASE WHEN ? THEN "DetailImportAuthorizedAt"
                       ELSE COALESCE("DetailImportAuthorizedAt", ?) END,
                   "DetailProjectionPendingAt" = CASE WHEN ? THEN "DetailProjectionPendingAt" ELSE NULL END
               WHERE "Id" = ? AND "PublicationJournalJson" = ?"#,
        )
        .bind(preserve_draft)
        .bind(&now)
        .bind(preserve_draft)
        .bind(id)
        .bind(&journal_json)
        .execute(&mut *commit)
        .await
        .repository()?
    };
    if updated.rows_affected() != 1 {
        return Err(RepositoryError::Conflict(
            "the staged add-on media-list publication changed before commit".to_owned(),
        ));
    }
    let detail_now = crate::repositories::common::encode_datetime(chrono::Utc::now());
    sqlx::query(
        r#"UPDATE "AddonMediaListDetailJob"
           SET "State" = ?, "RequestedAt" = COALESCE("RequestedAt", ?),
               "UpdatedAt" = ?
           WHERE "AddonMediaListId" = ? AND "State" = ? AND NOT ?
             AND EXISTS (
                 SELECT 1 FROM "AddonMediaList"
                 WHERE "Id" = ? AND "DetailImportPolicy" = ?
             )"#,
    )
    .bind(crate::repositories::common::encode_enum(
        AddonMediaListDetailState::Queued,
    ))
    .bind(&detail_now)
    .bind(&detail_now)
    .bind(id)
    .bind(crate::repositories::common::encode_enum(
        AddonMediaListDetailState::NotRequested,
    ))
    .bind(preserve_draft)
    .bind(id)
    .bind(crate::repositories::common::encode_enum(
        AddonMediaListDetailPolicy::Automatic,
    ))
    .execute(&mut *commit)
    .await
    .repository()?;
    commit.commit().await.repository()?;
    if fence.is_some() {
        Ok(())
    } else {
        cleanup_publication_backup_async(id, journal).await
    }
}

#[allow(clippy::too_many_lines)]
async fn restore_staged_publication_impl(
    repository: &SqliteAddonMediaListRepository,
    id: EntityId,
    message: &str,
    fence: Option<PublicationClaimFence<'_>>,
) -> Result<()> {
    let (journal, journal_json) = load_publication_journal_value(&repository.0, id).await?;
    if let Some(fence) = fence {
        ensure_current_publication_claim(&repository.0, id, fence, true).await?;
    }
    restore_publication_files(&repository.0, id, &journal, fence).await?;
    if let Some(fence) = fence {
        ensure_current_publication_claim(&repository.0, id, fence, true).await?;
    }

    let mut tx = crate::db::begin_write(&repository.0).await.repository()?;
    for item in &journal.previous_items {
        sqlx::query(
            r#"UPDATE "AddonMediaListItem"
               SET "Published" = ?, "MediaItemId" = ?, "PublishedLiveness" = ?,
                   "PublishedAvailability" = ?, "PublishedAvailabilityReason" = ?,
                   "PublishedAvailabilityReasonCode" = ?
               WHERE "Id" = ?"#,
        )
        .bind(item.published)
        .bind(item.media_item_id)
        .bind(crate::repositories::common::encode_enum(
            item.published_liveness,
        ))
        .bind(crate::repositories::common::encode_enum(
            item.published_availability,
        ))
        .bind(&item.published_availability_reason)
        .bind(
            item.published_availability_reason_code
                .map(crate::repositories::common::encode_enum),
        )
        .bind(item.id)
        .execute(&mut *tx)
        .await
        .repository()?;
    }
    sqlx::query(
        r#"DELETE FROM "AddonMediaListArtworkMaterialization"
           WHERE "AddonMediaListId" = ?"#,
    )
    .bind(id)
    .execute(&mut *tx)
    .await
    .repository()?;
    for materialization in &journal.previous_materializations {
        sqlx::query(
            r#"INSERT INTO "AddonMediaListArtworkMaterialization" (
                   "Id", "AddonMediaListId", "AddonMediaListItemId", "ScopeItemId", "Role",
                   "Source", "FileName", "Sha256", "PublishedRevision"
               ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(materialization.id)
        .bind(id)
        .bind(materialization.item_id)
        .bind(materialization.scope_item_id)
        .bind(materialization.role)
        .bind(&materialization.source)
        .bind(&materialization.file_name)
        .bind(&materialization.sha256)
        .bind(materialization.published_revision)
        .execute(&mut *tx)
        .await
        .repository()?;
    }
    let updated = if let Some(fence) = fence {
        let now = crate::repositories::common::encode_datetime(chrono::Utc::now());
        sqlx::query(
            r#"UPDATE "AddonMediaList"
               SET "ReviewState" = 3, "ReviewError" = ?,
                   "PublishedRevision" = COALESCE(?, "PublishedRevision")
               WHERE "Id" = ? AND "PublicationJournalJson" = ?
                 AND EXISTS (
                     SELECT 1
                     FROM "AddonMediaListPublicationJob" AS "job"
                     JOIN "ManagedLibraryOperation" AS "operation"
                       ON "operation"."Id" = "job"."OperationId"
                     JOIN "ManagedLibraryLease" AS "lease"
                       ON "lease"."OperationId" = "operation"."Id"
                     WHERE "job"."AddonMediaListId" = "AddonMediaList"."Id"
                       AND "job"."OperationId" = ? AND "job"."Generation" = ?
                       AND "operation"."State" = 1
                       AND "operation"."OwnerToken" = ?
                       AND "operation"."LeaseOwner" = ?
                       AND "lease"."OwnerToken" = ?
                       AND "lease"."LeaseOwner" = ?
                       AND "lease"."LeaseExpiresAt" > ?
                 )"#,
        )
        .bind(message)
        .bind(journal.previous_published_revision)
        .bind(id)
        .bind(&journal_json)
        .bind(fence.operation_id)
        .bind(fence.generation)
        .bind(fence.owner_token)
        .bind(fence.lease_owner)
        .bind(fence.owner_token)
        .bind(fence.lease_owner)
        .bind(now)
        .execute(&mut *tx)
        .await
        .repository()?
    } else {
        sqlx::query(
            r#"UPDATE "AddonMediaList"
               SET "ReviewState" = ?, "ReviewError" = ?,
                   "PublishedRevision" = COALESCE(?, "PublishedRevision")
               WHERE "Id" = ? AND "PublicationJournalJson" = ?"#,
        )
        .bind(crate::repositories::common::encode_enum(
            journal.previous_review_state,
        ))
        .bind(message)
        .bind(journal.previous_published_revision)
        .bind(id)
        .bind(&journal_json)
        .execute(&mut *tx)
        .await
        .repository()?
    };
    if updated.rows_affected() != 1 {
        return Err(RepositoryError::Conflict(
            "the staged add-on media-list publication changed before restoration".to_owned(),
        ));
    }
    tx.commit().await.repository()?;
    Ok(())
}

async fn reconcile_restored_publication_impl(
    repository: &SqliteAddonMediaListRepository,
    id: EntityId,
    fence: Option<PublicationClaimFence<'_>>,
) -> Result<()> {
    let (journal, journal_json) = match load_publication_journal_value(&repository.0, id).await {
        Ok(journal) => journal,
        Err(RepositoryError::Conflict(_)) => {
            let review_state = sqlx::query_scalar::<_, i32>(
                r#"SELECT "ReviewState" FROM "AddonMediaList" WHERE "Id" = ?"#,
            )
            .bind(id)
            .fetch_optional(&repository.0)
            .await
            .repository()?
            .ok_or(RepositoryError::NotFound {
                kind: "add-on media list",
                id,
            })?;
            if review_state
                != crate::repositories::common::encode_enum(AddonMediaListReviewState::Publishing)
            {
                return Ok(());
            }
            return Err(RepositoryError::Conflict(
                "the publishing list lost its publication journal".to_owned(),
            ));
        }
        Err(error) => return Err(error),
    };
    if let Some(fence) = fence {
        ensure_current_publication_claim(&repository.0, id, fence, true).await?;
    }
    // The attempted source is not the previous publication being restored.
    let previous_source = if let Some(revision) = journal
        .previous_published_revision
        .filter(|revision| *revision > 0)
    {
        repository.revision_snapshot(id, revision).await?
    } else {
        None
    };
    reconcile_projection(
        &repository.0,
        id,
        journal.previous_presentation_kind,
        previous_source.as_ref(),
    )
    .await?;
    if let Some(fence) = fence {
        ensure_current_publication_claim(&repository.0, id, fence, true).await?;
    }
    AddonMediaListSynchronizer::new(repository.0.clone())
        .match_list_items_for_publication(id, previous_source.as_ref())
        .await?;
    if let Some(fence) = fence {
        ensure_current_publication_claim(&repository.0, id, fence, true).await?;
    }
    restore_playlist_snapshot(&repository.0, id, journal.previous_playlist.as_ref()).await?;
    if let Some(playlist) = &journal.previous_playlist {
        AddonMediaListSynchronizer::new(repository.0.clone())
            .rebuild_restored_playlist_items(id, playlist.id, previous_source.as_ref())
            .await?;
    }

    if let Some(fence) = fence {
        ensure_current_publication_claim(&repository.0, id, fence, true).await?;
        return Ok(());
    }

    let cleared = sqlx::query(
        r#"UPDATE "AddonMediaList"
           SET "PublicationJournalJson" = NULL, "PublicationStartedAt" = NULL
           WHERE "Id" = ? AND "PublicationJournalJson" = ?"#,
    )
    .bind(id)
    .bind(&journal_json)
    .execute(&repository.0)
    .await
    .repository()?;
    if cleared.rows_affected() != 1 {
        return Err(RepositoryError::Conflict(
            "the restored add-on media-list publication changed before reconciliation".to_owned(),
        ));
    }
    Ok(())
}

/// Recheck automatic candidates under the same write transaction that starts
/// staging. Detail results can change item data without changing its revision.
async fn ensure_automatic_publication_safe_in_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    list_id: EntityId,
    operation_id: EntityId,
) -> Result<()> {
    use super::addon_media_list_auto_eligibility::{
        assess_auto_publication_in_tx, AutoPublicationAssessment,
    };

    let automatic: bool = sqlx::query_scalar(
        r#"SELECT EXISTS (
            SELECT 1 FROM "AddonMediaListPublicationEvent" AS "event"
            JOIN "AddonMediaListPublicationJob" AS "job"
              ON "job"."Id" = "event"."PublicationJobId"
            WHERE "job"."OperationId" = ? AND "event"."EnqueueTrigger" = ?
        )"#,
    )
    .bind(operation_id)
    .bind(crate::repositories::common::encode_enum(
        ersatzrs_domain::AddonMediaListPublicationEnqueueTrigger::AutomaticRefresh,
    ))
    .fetch_one(&mut **tx)
    .await
    .repository()?;
    if !automatic {
        return Ok(());
    }
    let authorized: bool = sqlx::query_scalar(
        r#"SELECT EXISTS (
            SELECT 1 FROM "AddonMediaListAutoPublication" AS "auto"
            JOIN "AddonMediaList" AS "list" ON "list"."Id" = "auto"."AddonMediaListId"
            WHERE "list"."Id" = ? AND "list"."RefreshPublicationPolicy" = 1
              AND "auto"."OperationId" = ? AND "auto"."State" = 'queued'
              AND "auto"."DraftRevision" = "list"."DraftRevision"
              AND "auto"."PublishedRevision" = "list"."PublishedRevision"
        )"#,
    )
    .bind(list_id)
    .bind(operation_id)
    .fetch_one(&mut **tx)
    .await
    .repository()?;
    if !authorized
        || !matches!(
            assess_auto_publication_in_tx(tx, list_id).await?,
            AutoPublicationAssessment::Eligible
        )
    {
        return Err(RepositoryError::Conflict(
            "automatic refresh publication requires review before staging".to_owned(),
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
async fn stage_publication_impl(
    repository: &SqliteAddonMediaListRepository,
    id: EntityId,
    fence: Option<PublicationFence<'_>>,
) -> Result<AddonMediaListPublication> {
    let form = repository
        .get_admin_form(id)
        .await?
        .ok_or(RepositoryError::NotFound {
            kind: "add-on media list",
            id,
        })?;
    let existing_journal = sqlx::query_scalar::<_, Option<String>>(
        r#"SELECT "PublicationJournalJson" FROM "AddonMediaList" WHERE "Id" = ?"#,
    )
    .bind(id)
    .fetch_one(&repository.0)
    .await
    .repository()?;
    if let Some(value) = existing_journal.filter(|value| !value.trim().is_empty()) {
        let journal: PublicationJournal =
            serde_json::from_str(&value).map_err(|error| RepositoryError::Decode {
                kind: "add-on publication journal",
                value: error.to_string(),
            })?;
        if let Some(fence) = fence {
            if journal.operation_id == Some(fence.operation_id)
                && journal.generation == Some(fence.generation)
                && journal.saved_revision == Some(fence.saved_revision)
            {
                materialize_retained_revision_candidate_if_needed(repository, id, &journal, fence)
                    .await?;
                let library_id = repository.managed_library_id(id).await?.ok_or_else(|| {
                    RepositoryError::Conflict("managed list storage is unavailable".to_owned())
                })?;
                return Ok(AddonMediaListPublication {
                    list_id: id,
                    library_id,
                });
            }
        }
        return Err(publication_in_progress());
    }

    if let Some(fence) = fence {
        ensure_current_publication_fence(
            &repository.0,
            id,
            fence.operation_id,
            fence.generation,
            fence.owner_token,
            fence.lease_owner,
            false,
        )
        .await?;
    }
    let (source_snapshot, saved_folder) = if let Some(fence) = fence {
        repository
            .source_snapshot_and_origin_for_fence(id, fence)
            .await?
    } else {
        (None, false)
    };
    if !saved_folder && fence.is_some_and(|fence| form.draft_revision != fence.saved_revision) {
        return Err(RepositoryError::Conflict(
            "the saved add-on media-list revision changed before publication staging".to_owned(),
        ));
    }
    // A retained publication has already selected its own membership and
    // playback. Newer draft blockers or tombstones must not change that input.
    let publication_excluded_ids = if source_snapshot.is_some() {
        Vec::new()
    } else {
        let mut readiness = repository.publication_readiness(id).await?;
        if fence.is_some() {
            readiness.blockers.retain(|blocker| {
                blocker.kind != AddonMediaListPublicationBlockerKind::PublicationInProgress
            });
        }
        if let Some(blocker) = readiness.blockers.first() {
            let message = publication_blocker_message(blocker);
            record_publication_error(&repository.0, id, &message).await?;
            return Err(RepositoryError::Conflict(message));
        }
        if readiness.required_playback_detail_count > 0 {
            let message = format!(
                "{} included finite item(s) still require playback details",
                readiness.required_playback_detail_count
            );
            record_publication_error(&repository.0, id, &message).await?;
            return Err(RepositoryError::Conflict(message));
        }
        let mut publication_excluded_ids = repository
            .list_items(id)
            .await?
            .into_iter()
            .filter(|item| item.included && access_restricted_without_replacement(item))
            .map(|item| item.id)
            .collect::<Vec<_>>();
        publication_excluded_ids.extend(
            sqlx::query_scalar::<_, EntityId>(
                r#"SELECT "tombstone"."AddonMediaListItemId"
               FROM "AddonMediaListItemTombstone" AS "tombstone"
               JOIN "AddonMediaListItem" AS "item"
                 ON "item"."Id" = "tombstone"."AddonMediaListItemId"
               WHERE "item"."AddonMediaListId" = ?
                 AND "tombstone"."ConfirmedAt" IS NOT NULL"#,
            )
            .bind(id)
            .fetch_all(&repository.0)
            .await
            .repository()?,
        );
        publication_excluded_ids.sort_unstable();
        publication_excluded_ids.dedup();
        publication_excluded_ids
    };
    let playback_selection = select_publication_playback(
        repository,
        id,
        form.draft_revision,
        source_snapshot.as_ref(),
        &publication_excluded_ids,
    )
    .await?;
    let library_id = repository.managed_library_id(id).await?.ok_or_else(|| {
        RepositoryError::Conflict("managed list storage is unavailable".to_owned())
    })?;
    let previous_items = load_published_item_snapshots(&repository.0, id).await?;
    let previous_playlist = load_playlist_snapshot(&repository.0, id).await?;
    let previous_materializations = load_artwork_materializations(&repository.0, id).await?;
    let previous_published_revision = form.published_revision;
    let target_published_revision = if let Some(fence) = fence {
        sqlx::query_scalar::<_, i32>(
            r#"SELECT "TargetPublishedRevision"
               FROM "AddonMediaListPublicationJob"
               WHERE "OperationId" = ? AND "AddonMediaListId" = ? AND "Generation" = ?"#,
        )
        .bind(fence.operation_id)
        .bind(id)
        .bind(fence.generation)
        .fetch_one(&repository.0)
        .await
        .repository()?
    } else {
        previous_published_revision.checked_add(1).ok_or_else(|| {
            RepositoryError::Conflict(
                "the published add-on media-list revision is exhausted".into(),
            )
        })?
    };
    if target_published_revision <= previous_published_revision {
        return Err(RepositoryError::Conflict(
            "the queued add-on publication target is not newer than the current revision".into(),
        ));
    }
    let publication_paths = create_publication_backup(
        &repository.0,
        id,
        fence,
        previous_published_revision,
        target_published_revision,
    )
    .await?;
    let mut journal = PublicationJournal {
        operation_id: fence.map(|value| value.operation_id),
        generation: fence.map(|value| value.generation),
        saved_revision: fence.map(|value| value.saved_revision),
        previous_review_state: form.review_state,
        previous_presentation_kind: form.presentation_kind,
        previous_items,
        previous_playlist,
        managed_folder: Some(publication_paths.managed.to_string_lossy().into_owned()),
        backup_folder: Some(publication_paths.backup.to_string_lossy().into_owned()),
        failed_folder: Some(publication_paths.failed.to_string_lossy().into_owned()),
        previous_published_revision: Some(previous_published_revision),
        target_published_revision: Some(target_published_revision),
        previous_materializations,
        target_database: None,
        prepared_media: Some(playback_selection.prepared_media),
        subtitles: Some(playback_selection.subtitles),
        streaming_plans: Some(playback_selection.streaming_plans),
    };
    let journal_json = match serde_json::to_string(&journal) {
        Ok(value) => value,
        Err(error) => {
            abort_publication_staging(publication_paths, publication_fs_identity(id, &journal))
                .await;
            return Err(RepositoryError::Decode {
                kind: "add-on publication journal",
                value: error.to_string(),
            });
        }
    };
    let staged = async {
        let mut tx = crate::db::begin_write(&repository.0).await.repository()?;
        // The filesystem backup may overlap a newer Save before this write
        // transaction. Rollback must restore that Save's review state, not the
        // form read before the backup. Item rollback writes only publication
        // bindings, never the captured draft identity fields.
        let journal_json = if saved_folder {
            let row = sqlx::query(
                r#"SELECT "ReviewState" FROM "AddonMediaList" WHERE "Id"=?"#,
            )
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .repository()?;
            journal.previous_review_state = crate::repositories::common::decode_enum_column(
                "AddonMediaListReviewState", &row, "ReviewState",
            )?;
            journal.previous_items = load_published_item_snapshots_on_connection(&mut tx, id).await?;
            serde_json::to_string(&journal).map_err(|error| RepositoryError::Decode {
                kind: "add-on publication journal",
                value: error.to_string(),
            })?
        } else {
            journal_json
        };
        if let Some(fence) = fence {
            ensure_automatic_publication_safe_in_tx(&mut tx, id, fence.operation_id).await?;
        }
        let updated = if let Some(fence) = fence {
            sqlx::query(
                r#"UPDATE "AddonMediaList"
                   SET "ReviewState" = 3, "ReviewError" = NULL,
                       "PublicationJournalJson" = ?, "PublicationStartedAt" = ?
                   WHERE "Id" = ? AND "ReviewState" <> 3
                     AND (? = 1 OR "DraftRevision" = ?)
                     AND ("PublicationJournalJson" IS NULL OR TRIM("PublicationJournalJson") = '')
                     AND EXISTS (
                         SELECT 1
                         FROM "AddonMediaListPublicationJob" AS "job"
                         JOIN "ManagedLibraryOperation" AS "operation"
                           ON "operation"."Id" = "job"."OperationId"
                         JOIN "ManagedLibraryLease" AS "lease"
                           ON "lease"."OperationId" = "operation"."Id"
                         WHERE "job"."AddonMediaListId" = "AddonMediaList"."Id"
                           AND "job"."OperationId" = ? AND "job"."Generation" = ?
                           AND "operation"."State" = 1
                           AND "operation"."OwnerToken" = ?
                           AND "operation"."LeaseOwner" = ?
                           AND "lease"."OwnerToken" = ?
                           AND "lease"."LeaseOwner" = ?
                           AND "lease"."LeaseExpiresAt" > ?
                     )"#,
            )
            .bind(&journal_json)
            .bind(crate::repositories::common::encode_datetime(
                chrono::Utc::now(),
            ))
            .bind(id)
            .bind(saved_folder)
            .bind(fence.saved_revision)
            .bind(fence.operation_id)
            .bind(fence.generation)
            .bind(fence.owner_token)
            .bind(fence.lease_owner)
            .bind(fence.owner_token)
            .bind(fence.lease_owner)
            .bind(crate::repositories::common::encode_datetime(
                chrono::Utc::now(),
            ))
            .execute(&mut *tx)
            .await
            .repository()?
        } else {
            sqlx::query(
                r#"UPDATE "AddonMediaList"
                   SET "ReviewState" = 3, "ReviewError" = NULL,
                       "PublicationJournalJson" = ?, "PublicationStartedAt" = ?
                   WHERE "Id" = ? AND "ReviewState" <> 3
                     AND ("PublicationJournalJson" IS NULL OR TRIM("PublicationJournalJson") = '')"#,
            )
            .bind(&journal_json)
            .bind(crate::repositories::common::encode_datetime(
                chrono::Utc::now(),
            ))
            .bind(id)
            .execute(&mut *tx)
            .await
            .repository()?
        };
        if updated.rows_affected() != 1 {
            return Err(publication_in_progress());
        }
        if let Some(snapshot) = &source_snapshot {
            stage_snapshot_membership_in_tx(&mut tx, snapshot).await?;
        } else {
        sqlx::query(
            r#"UPDATE "AddonMediaListItem"
               SET "Published" = ("InDraft" = 1 AND "Included" = 1),
                   "PublishedLiveness" = "EffectiveLiveness",
                   "PublishedAvailability" = "Availability",
                   "PublishedAvailabilityReason" = "AvailabilityReason",
                   "PublishedAvailabilityReasonCode" = "AvailabilityReasonCode"
               WHERE "AddonMediaListId" = ?"#,
        )
        .bind(id)
        .execute(&mut *tx)
        .await
        .repository()?;
        }
        for item_id in publication_excluded_ids {
            sqlx::query(
                r#"UPDATE "AddonMediaListItem"
                   SET "Published" = 0
                   WHERE "AddonMediaListId" = ? AND "Id" = ?"#,
            )
            .bind(id)
            .bind(item_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        }
        tx.commit().await.repository()
    }
    .await;
    if let Err(error) = staged {
        abort_publication_staging(publication_paths, publication_fs_identity(id, &journal)).await;
        return Err(error);
    }
    if let Some(fence) = fence {
        materialize_retained_revision_candidate_if_needed(repository, id, &journal, fence).await?;
    }
    Ok(AddonMediaListPublication {
        list_id: id,
        library_id,
    })
}

/// Assign only active publication fields; saved draft choices remain untouched.
async fn stage_snapshot_membership_in_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    snapshot: &ersatzrs_core::repositories::AddonMediaListRevisionSnapshot,
) -> Result<()> {
    sqlx::query(r#"UPDATE "AddonMediaListItem" SET "Published"=0 WHERE "AddonMediaListId"=?"#)
        .bind(snapshot.list_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    for item in snapshot.items.iter().filter(|item| item.published) {
        let updated = sqlx::query(
            r#"UPDATE "AddonMediaListItem" SET "Published"=1, "PublishedLiveness"=?,
               "PublishedAvailability"=?, "PublishedAvailabilityReason"=?,
               "PublishedAvailabilityReasonCode"=?
               WHERE "AddonMediaListId"=? AND "ProviderId"=?"#,
        )
        .bind(crate::repositories::common::encode_enum(
            item.effective_liveness,
        ))
        .bind(crate::repositories::common::encode_enum(item.availability))
        .bind(&item.availability_reason)
        .bind(
            item.availability_reason_code
                .map(crate::repositories::common::encode_enum),
        )
        .bind(snapshot.list_id)
        .bind(&item.provider_id)
        .execute(&mut **tx)
        .await
        .repository()?;
        if updated.rows_affected() != 1 {
            return Err(RepositoryError::Conflict(
                "the retained publication item has no receiving owner".into(),
            ));
        }
    }
    Ok(())
}

async fn select_publication_playback(
    repository: &SqliteAddonMediaListRepository,
    list_id: EntityId,
    saved_revision: i32,
    source_snapshot: Option<&ersatzrs_core::repositories::AddonMediaListRevisionSnapshot>,
    excluded_item_ids: &[EntityId],
) -> Result<ersatzrs_core::repositories::AddonMediaListRevisionArtifacts> {
    if let Some(snapshot) = source_snapshot {
        let mut selection = ersatzrs_core::repositories::AddonMediaListRevisionArtifacts::default();
        for item in snapshot.items.iter().filter(|item| item.published) {
            if !item.subtitle_artifact_ids.is_empty() {
                selection
                    .subtitles
                    .insert(item.provider_id.clone(), item.subtitle_artifact_ids.clone());
            }
            if let Some(artifact_id) = item.prepared_artifact_id {
                selection
                    .prepared_media
                    .insert(item.provider_id.clone(), artifact_id);
            }
            if let Some(plan) = &item.playback_timeline {
                selection
                    .streaming_plans
                    .insert(item.provider_id.clone(), plan.clone());
            }
        }
        return Ok(selection);
    }
    let prepared_media = crate::addon_media_list_prepared_selection::select_prepared_media(
        &repository.0,
        list_id,
        saved_revision,
        excluded_item_ids,
    )
    .await?;
    let streaming_plans = crate::addon_media_list_prepared_selection::select_streaming_plans(
        &repository.0,
        list_id,
        saved_revision,
        excluded_item_ids,
    )
    .await?;
    let subtitles = crate::addon_media_list_subtitle_selection::select_subtitle_artifacts(
        &repository.0,
        list_id,
        saved_revision,
        excluded_item_ids,
    )
    .await?;
    Ok(
        ersatzrs_core::repositories::AddonMediaListRevisionArtifacts {
            ids: prepared_media
                .values()
                .copied()
                .chain(subtitles.values().flatten().copied())
                .collect(),
            subtitles,
            streaming_plans,
            prepared_media,
        },
    )
}

async fn materialize_retained_revision_candidate_if_needed(
    repository: &SqliteAddonMediaListRepository,
    list_id: EntityId,
    journal: &PublicationJournal,
    fence: PublicationFence<'_>,
) -> Result<()> {
    let row = sqlx::query(
        r#"SELECT "SourcePublishedRevision", "SavedRevision", "TargetPublishedRevision",
                  EXISTS(SELECT 1 FROM "AddonMediaListPublicationEvent" AS "event"
                         WHERE "event"."PublicationJobId"="job"."Id"
                           AND "event"."DedupeKey"='enqueued' AND "event"."EnqueueTrigger"=?
                           AND "event"."Kind"=?) AS "SavedFolder"
           FROM "AddonMediaListPublicationJob" AS "job"
           WHERE "OperationId" = ? AND "AddonMediaListId" = ? AND "Generation" = ?"#,
    )
    .bind(crate::repositories::common::encode_enum(
        ersatzrs_domain::AddonMediaListPublicationEnqueueTrigger::SavedFolder,
    ))
    .bind(crate::repositories::common::encode_enum(
        ersatzrs_domain::AddonMediaListPublicationEventKind::Enqueued,
    ))
    .bind(fence.operation_id)
    .bind(list_id)
    .bind(fence.generation)
    .fetch_optional(&repository.0)
    .await
    .repository()?
    .ok_or_else(|| {
        RepositoryError::Conflict(
            "the add-on publication job disappeared before restore staging".to_owned(),
        )
    })?;
    let source_revision: Option<i32> = row.try_get("SourcePublishedRevision").repository()?;
    let Some(source_revision) = source_revision else {
        return Ok(());
    };
    let saved_revision: i32 = row.try_get("SavedRevision").repository()?;
    let target_published_revision: i32 = row.try_get("TargetPublishedRevision").repository()?;
    let identity = publication_fs_identity(list_id, journal);
    if saved_revision != fence.saved_revision
        || target_published_revision != identity.target_revision
        || journal.operation_id != Some(fence.operation_id)
        || journal.generation != Some(fence.generation)
    {
        return Err(RepositoryError::Conflict(
            "the retained revision restore identity changed before staging".to_owned(),
        ));
    }
    if row.try_get::<bool, _>("SavedFolder").repository()? {
        // The receiving snapshot regenerates local files from its content. It
        // has no sender archive or authority to clear the copied folder.
        return Ok(());
    }
    let snapshot = repository
        .revision_snapshot(list_id, source_revision)
        .await?
        .ok_or_else(|| RepositoryError::Conflict("restore revision is not restorable".into()))?;
    let files = snapshot
        .files
        .into_iter()
        .map(|file| ArchivedRevisionFile {
            published_relative_path: file.published_relative_path,
            storage_relative_path: file.storage_relative_path,
            sha256: file.sha256,
            byte_length: file.byte_length,
        })
        .collect::<Vec<_>>();
    ensure_current_publication_fence(
        &repository.0,
        list_id,
        fence.operation_id,
        fence.generation,
        fence.owner_token,
        fence.lease_owner,
        true,
    )
    .await?;
    let paths = publication_paths_from_journal(journal)?;
    let (storage_parent, storage_owner) = revision_blob_storage(&repository.0).await?;
    tokio::task::spawn_blocking(move || {
        addon_publication_fs::materialize_archived_revision(
            &paths,
            identity,
            &storage_parent,
            &storage_owner,
            &files,
        )
    })
    .await
    .map_err(|error| {
        RepositoryError::unavailable(
            "stage retained add-on publication revision",
            format!("revision materialization task failed: {error}"),
        )
    })?
    .map_err(|error| publication_fs_repository_error(&error))?;
    ensure_current_publication_fence(
        &repository.0,
        list_id,
        fence.operation_id,
        fence.generation,
        fence.owner_token,
        fence.lease_owner,
        true,
    )
    .await
}

async fn revision_blob_storage(pool: &Pool<crate::db::Db>) -> Result<(PathBuf, String)> {
    let storage_owner: String =
        sqlx::query_scalar(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
            .bind(REVISION_BLOB_STORE_ID_KEY)
            .fetch_optional(pool)
            .await
            .repository()?
            .ok_or_else(|| {
                RepositoryError::Conflict(
                    "the retained revision storage identity is unavailable".into(),
                )
            })?;
    if storage_owner.len() != 32
        || !storage_owner
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(RepositoryError::Conflict(
            "the retained revision storage identity is invalid".into(),
        ));
    }

    let rows = sqlx::query("PRAGMA database_list")
        .fetch_all(pool)
        .await
        .repository()?;
    let mut main_database = None;
    for row in rows {
        if row.try_get::<String, _>("name").repository()? == "main" {
            main_database = Some(row.try_get::<String, _>("file").repository()?);
            break;
        }
    }
    let Some(database_file) = main_database.filter(|path| !path.trim().is_empty()) else {
        // In-memory databases are used by focused repository tests. Their
        // revision bytes cannot outlive the process any more than the database
        // can, but they still need an isolated filesystem root while it runs.
        return Ok((std::env::temp_dir(), storage_owner));
    };
    let storage_parent = Path::new(&database_file)
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    Ok((storage_parent, storage_owner))
}

async fn load_artwork_materializations(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
) -> Result<Vec<ArtworkMaterializationSnapshot>> {
    sqlx::query(
        r#"SELECT "Id", "AddonMediaListItemId", "ScopeItemId", "Role", "Source",
                  "FileName", "Sha256", "PublishedRevision"
           FROM "AddonMediaListArtworkMaterialization"
           WHERE "AddonMediaListId" = ? ORDER BY "Id""#,
    )
    .bind(list_id)
    .fetch_all(pool)
    .await
    .repository()?
    .iter()
    .map(|row| {
        Ok(ArtworkMaterializationSnapshot {
            id: row.try_get("Id").repository()?,
            item_id: row.try_get("AddonMediaListItemId").repository()?,
            scope_item_id: row.try_get("ScopeItemId").repository()?,
            role: row.try_get("Role").repository()?,
            source: row.try_get("Source").repository()?,
            file_name: row.try_get("FileName").repository()?,
            sha256: row.try_get("Sha256").repository()?,
            published_revision: row.try_get("PublishedRevision").repository()?,
        })
    })
    .collect()
}

async fn create_publication_backup(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    fence: Option<PublicationFence<'_>>,
    previous_published_revision: i32,
    target_published_revision: i32,
) -> Result<PublicationFsPaths> {
    let manifest: String = sqlx::query_scalar(
        r#"SELECT "ManifestPath" FROM "AddonMediaListLibrary"
           WHERE "AddonMediaListId" = ?"#,
    )
    .bind(list_id)
    .fetch_one(pool)
    .await
    .repository()?;
    if let Some(fence) = fence {
        ensure_current_publication_claim(pool, list_id, fence.claim(), false).await?;
    }
    let operation_id = fence.map(|value| value.operation_id);
    let generation = fence.map(|value| value.generation);
    let identity = PublicationFsIdentity {
        list_id,
        operation_id,
        generation,
        previous_revision: previous_published_revision,
        target_revision: target_published_revision,
    };
    let result = tokio::task::spawn_blocking(move || {
        let manifest_path = Path::new(&manifest);
        let managed_path = if manifest_path.is_dir() {
            manifest_path
        } else {
            manifest_path
                .parent()
                .ok_or(addon_publication_fs::PublicationFsError::UnsafeState(
                    "the managed manifest has no parent folder",
                ))?
        };
        let managed = managed_path.canonicalize().map_err(|source| {
            addon_publication_fs::PublicationFsError::Io {
                action: "canonicalize managed publication folder",
                source,
            }
        })?;
        let paths =
            addon_publication_fs::publication_paths(managed, list_id, operation_id, generation)?;
        addon_publication_fs::create_backup(&paths, identity)?;
        Ok::<_, addon_publication_fs::PublicationFsError>(paths)
    })
    .await
    .map_err(|error| {
        RepositoryError::unavailable(
            "snapshot add-on publication",
            format!("publication backup task failed: {error}"),
        )
    })?
    .map_err(|error| publication_fs_repository_error(&error))?;
    if let Some(fence) = fence {
        if let Err(error) =
            ensure_current_publication_claim(pool, list_id, fence.claim(), false).await
        {
            abort_publication_staging(result, identity).await;
            return Err(error);
        }
    }
    Ok(result)
}

fn publication_paths_from_journal(journal: &PublicationJournal) -> Result<PublicationFsPaths> {
    let managed = journal
        .managed_folder
        .as_deref()
        .map(PathBuf::from)
        .ok_or_else(|| {
            RepositoryError::unavailable(
                "restore add-on publication",
                "the publication journal has no managed folder",
            )
        })?;
    let backup = journal
        .backup_folder
        .as_deref()
        .map(PathBuf::from)
        .ok_or_else(|| {
            RepositoryError::unavailable(
                "restore add-on publication",
                "the publication journal has no backup folder",
            )
        })?;
    let failed = if let Some(value) = journal.failed_folder.as_deref() {
        PathBuf::from(value)
    } else {
        let suffix = backup
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_prefix(".ersatzrs-publication-"))
            .ok_or_else(|| {
                RepositoryError::unavailable(
                    "restore add-on publication",
                    "the legacy publication backup name failed its ownership check",
                )
            })?;
        managed
            .parent()
            .ok_or_else(|| {
                RepositoryError::unavailable(
                    "restore add-on publication",
                    "the managed publication folder has no parent",
                )
            })?
            .join(format!(".ersatzrs-failed-publication-{suffix}"))
    };
    Ok(PublicationFsPaths {
        managed,
        backup,
        failed,
    })
}

async fn cleanup_publication_backup_async(
    list_id: EntityId,
    journal: PublicationJournal,
) -> Result<()> {
    let paths = publication_paths_from_journal(&journal)?;
    let identity = publication_fs_identity(list_id, &journal);
    tokio::task::spawn_blocking(move || {
        addon_publication_fs::verify_managed_complete(
            &paths,
            identity,
            PublicationTreeRole::Candidate,
        )?;
        addon_publication_fs::cleanup_after_verification(
            &paths,
            identity,
            PublicationTreeRole::Candidate,
        )
    })
    .await
    .map_err(|error| {
        RepositoryError::unavailable(
            "complete add-on publication",
            format!("publication cleanup task failed: {error}"),
        )
    })?
    .map_err(|error| publication_fs_repository_error(&error))
}

#[allow(clippy::too_many_lines)] // Keep archival validation and retention before cleanup together.
async fn retain_published_revision_before_cleanup(
    repository: &SqliteAddonMediaListRepository,
    list_id: EntityId,
    journal: &PublicationJournal,
    paths: &PublicationFsPaths,
    identity: PublicationFsIdentity,
    fence: PublicationClaimFence<'_>,
) -> Result<()> {
    let revision = sqlx::query(
        r#"SELECT "job"."SavedRevision", "job"."TargetPublishedRevision",
                  "job"."SourcePublishedRevision", "job"."RestoreMaterializedAt",
                  COALESCE(
                      (SELECT "event"."OccurredAt"
                       FROM "AddonMediaListPublicationEvent" AS "event"
                       WHERE "event"."PublicationJobId" = "job"."Id"
                         AND "event"."FilesystemTransition" = ?
                       ORDER BY "event"."Sequence" DESC LIMIT 1),
                      (SELECT "event"."OccurredAt"
                       FROM "AddonMediaListPublicationEvent" AS "event"
                       WHERE "event"."PublicationJobId" = "job"."Id"
                         AND "event"."DedupeKey" = 'existing_job_adopted'
                       LIMIT 1)
                  ) AS "RevisionVerifiedAt"
           FROM "AddonMediaListPublicationJob" AS "job"
           WHERE "job"."OperationId" = ? AND "job"."AddonMediaListId" = ?
             AND "job"."Generation" = ?"#,
    )
    .bind(crate::repositories::common::encode_enum(
        AddonMediaListPublicationFilesystemTransition::RevisionVerified,
    ))
    .bind(fence.operation_id)
    .bind(list_id)
    .bind(fence.generation)
    .fetch_optional(&repository.0)
    .await
    .repository()?
    .ok_or_else(|| {
        RepositoryError::Conflict(
            "the add-on publication job disappeared before revision retention".to_owned(),
        )
    })?;
    let source_draft_revision: i32 = revision.try_get("SavedRevision").repository()?;
    let target_published_revision: i32 =
        revision.try_get("TargetPublishedRevision").repository()?;
    if target_published_revision != identity.target_revision
        || journal
            .saved_revision
            .is_some_and(|saved| saved != source_draft_revision)
    {
        return Err(RepositoryError::Conflict(
            "the add-on publication revision identity changed before retention".to_owned(),
        ));
    }

    let (storage_parent, storage_owner) = revision_blob_storage(&repository.0).await?;
    let archive_paths = paths.clone();
    let archive_storage_parent = storage_parent.clone();
    let archive_storage_owner = storage_owner.clone();
    let archived = tokio::task::spawn_blocking(move || {
        addon_publication_fs::archive_managed_revision(
            &archive_paths,
            identity,
            &archive_storage_parent,
            &archive_storage_owner,
        )
    })
    .await
    .map_err(|error| {
        RepositoryError::unavailable(
            "retain add-on publication revision",
            format!("revision archive task failed: {error}"),
        )
    })?
    .map_err(|error| publication_fs_repository_error(&error))?;
    ensure_current_publication_claim(&repository.0, list_id, fence, true).await?;

    let completed_at_raw: Option<String> = revision.try_get("RevisionVerifiedAt").repository()?;
    let completed_at = decode_datetime(
        "AddonMediaListPublicationEvent.RevisionVerifiedAt",
        completed_at_raw.as_deref().ok_or_else(|| {
            RepositoryError::Conflict(
                "the verified publication has no stable revision timestamp".into(),
            )
        })?,
    )?;
    let mut files = Vec::with_capacity(archived.len());
    for file in archived {
        let blob = repository
            .register_revision_file_blob(
                list_id,
                &file.storage_relative_path,
                &file.sha256,
                file.byte_length,
                completed_at,
            )
            .await?;
        files.push(AddonMediaListRevisionFile {
            blob_id: blob.id,
            published_relative_path: file.published_relative_path,
            provider_id: None,
            storage_relative_path: blob.storage_relative_path,
            sha256: blob.sha256,
            byte_length: blob.byte_length,
        });
    }
    let artifact_ids = current_published_artifact_ids(
        &repository.0,
        list_id,
        journal.prepared_media.as_ref(),
        journal.subtitles.as_ref(),
    )
    .await?;
    ensure_current_publication_claim(&repository.0, list_id, fence, true).await?;
    let source_revision = if revision
        .try_get::<Option<String>, _>("RestoreMaterializedAt")
        .repository()?
        .is_none()
    {
        revision
            .try_get::<Option<i32>, _>("SourcePublishedRevision")
            .repository()?
    } else {
        None
    };
    repository
        .retain_publication_source_revision(
            list_id,
            identity.target_revision,
            source_draft_revision,
            completed_at,
            &files,
            &ersatzrs_core::repositories::AddonMediaListRevisionArtifacts {
                ids: artifact_ids,
                prepared_media: journal.prepared_media.clone().unwrap_or_default(),
                subtitles: journal.subtitles.clone().unwrap_or_default(),
                streaming_plans: journal.streaming_plans.clone().unwrap_or_default(),
            },
            source_revision,
        )
        .await?;
    Ok(())
}

async fn current_published_artifact_ids(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    prepared_media: Option<&std::collections::BTreeMap<String, EntityId>>,
    subtitles: Option<&std::collections::BTreeMap<String, Vec<EntityId>>>,
) -> Result<Vec<EntityId>> {
    let rows = sqlx::query_as::<_, (EntityId, i32)>(
        r#"SELECT "artifact"."Id", "artifact"."Kind"
           FROM "AddonMediaListArtifact" AS "artifact"
           WHERE "artifact"."AddonMediaListId" = ? AND "artifact"."State" = ?
             AND (
                 "artifact"."AddonMediaListItemId" IS NULL
                 OR EXISTS (
                     SELECT 1 FROM "AddonMediaListItem" AS "item"
                     WHERE "item"."Id" = "artifact"."AddonMediaListItemId"
                       AND "item"."AddonMediaListId" = "artifact"."AddonMediaListId"
                       AND "item"."Published" = 1
                 )
             )
           ORDER BY "artifact"."Id""#,
    )
    .bind(list_id)
    .bind(crate::repositories::common::encode_enum(
        AddonMediaListArtifactState::Ready,
    ))
    .fetch_all(pool)
    .await
    .repository()?;
    Ok(rows
        .into_iter()
        .filter(|(id, kind)| {
            if *kind == 0
                && subtitles.is_some_and(|selected| {
                    !selected
                        .values()
                        .flatten()
                        .any(|selected_id| selected_id == id)
                })
            {
                return false;
            }
            prepared_media.is_none_or(|selected| {
                *kind
                    != crate::repositories::common::encode_enum(
                        ersatzrs_domain::AddonMediaListArtifactKind::ProcessedMedia,
                    )
                    || selected.values().any(|selected_id| selected_id == id)
            })
        })
        .map(|(id, _)| id)
        .collect())
}

fn publication_fs_identity(
    list_id: EntityId,
    journal: &PublicationJournal,
) -> PublicationFsIdentity {
    let previous_revision = journal.previous_published_revision.unwrap_or_default();
    PublicationFsIdentity {
        list_id,
        operation_id: journal.operation_id,
        generation: journal.generation,
        previous_revision,
        target_revision: match journal.target_published_revision {
            Some(value) => value,
            None => previous_revision.saturating_add(1),
        },
    }
}

async fn publication_fs_context(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    journal: &PublicationJournal,
    operation_id: Option<EntityId>,
    generation: Option<i32>,
) -> Result<(PublicationFsPaths, PublicationFsIdentity)> {
    let operation_id = journal.operation_id.or(operation_id);
    let generation = journal.generation.or(generation);
    let target_revision = match journal.target_published_revision {
        Some(value) => value,
        None => match (operation_id, generation) {
            (Some(operation_id), Some(generation)) => sqlx::query_scalar::<_, i32>(
                r#"SELECT "TargetPublishedRevision"
                   FROM "AddonMediaListPublicationJob"
                   WHERE "OperationId" = ? AND "AddonMediaListId" = ? AND "Generation" = ?"#,
            )
            .bind(operation_id)
            .bind(list_id)
            .bind(generation)
            .fetch_one(pool)
            .await
            .repository()?,
            _ => sqlx::query_scalar::<_, i32>(
                r#"SELECT "PublishedRevision" + 1 FROM "AddonMediaList" WHERE "Id" = ?"#,
            )
            .bind(list_id)
            .fetch_one(pool)
            .await
            .repository()?,
        },
    };
    let identity = PublicationFsIdentity {
        list_id,
        operation_id,
        generation,
        previous_revision: journal
            .previous_published_revision
            .unwrap_or_else(|| target_revision.saturating_sub(1)),
        target_revision,
    };
    Ok((publication_paths_from_journal(journal)?, identity))
}

const fn publication_outcome_role(
    outcome: AddonMediaListPublicationOutcome,
) -> PublicationTreeRole {
    match outcome {
        AddonMediaListPublicationOutcome::Published => PublicationTreeRole::Candidate,
        AddonMediaListPublicationOutcome::RestoredAfterFailure => PublicationTreeRole::Previous,
    }
}

fn publication_fs_repository_error(
    error: &addon_publication_fs::PublicationFsError,
) -> RepositoryError {
    let (category, os_error_code) = error.redacted_cause();
    let message = error.to_string();
    RepositoryError::unavailable_with_publication_cause(
        "add-on publication filesystem",
        message,
        category,
        os_error_code,
    )
}

async fn abort_publication_staging(paths: PublicationFsPaths, identity: PublicationFsIdentity) {
    let result =
        tokio::task::spawn_blocking(move || addon_publication_fs::abort_staging(&paths, identity))
            .await;
    match result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            tracing::warn!(%error, "unable to remove an uncommitted publication backup");
        }
        Err(error) => {
            tracing::warn!(%error, "publication staging cleanup task failed");
        }
    }
}

async fn restore_publication_files(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    journal: &PublicationJournal,
    fence: Option<PublicationClaimFence<'_>>,
) -> Result<()> {
    let operation_id = fence
        .map(|value| value.operation_id)
        .or(journal.operation_id);
    let generation = fence.map(|value| value.generation).or(journal.generation);
    let (paths, identity) =
        publication_fs_context(pool, list_id, journal, operation_id, generation).await?;

    if journal.failed_folder.is_none()
        || journal.previous_published_revision.is_none()
        || journal.target_published_revision.is_none()
    {
        let paths = paths.clone();
        tokio::task::spawn_blocking(move || {
            addon_publication_fs::bootstrap_legacy_markers(&paths, identity)
        })
        .await
        .map_err(|error| {
            RepositoryError::unavailable(
                "restore add-on publication",
                format!("publication marker recovery task failed: {error}"),
            )
        })?
        .map_err(|error| publication_fs_repository_error(&error))?;
    }

    let operation_seed = operation_id.unwrap_or(list_id);
    restore_publication_tree_with_retry(paths, identity, operation_seed, || async {
        if let Some(fence) = fence {
            ensure_current_publication_claim(pool, list_id, fence, true).await?;
        }
        Ok(())
    })
    .await
}

async fn restore_publication_tree_with_retry<Check, CheckFuture>(
    paths: PublicationFsPaths,
    identity: PublicationFsIdentity,
    operation_seed: EntityId,
    mut check_claim: Check,
) -> Result<()>
where
    Check: FnMut() -> CheckFuture,
    CheckFuture: std::future::Future<Output = Result<()>>,
{
    let mut retry_attempt = 0_u32;
    let mut transitions = 0_u8;
    loop {
        check_claim().await?;
        let step_paths = paths.clone();
        let result = tokio::task::spawn_blocking(move || {
            addon_publication_fs::restore_step(&step_paths, identity)
        })
        .await
        .map_err(|error| {
            RepositoryError::unavailable(
                "restore add-on publication",
                format!("publication restore task failed: {error}"),
            )
        })?;
        match result {
            Ok(RestoreProgress::Complete) => return Ok(()),
            Ok(RestoreProgress::Advanced) => {
                transitions = transitions.saturating_add(1);
                if transitions > 2 {
                    return Err(RepositoryError::unavailable(
                        "restore add-on publication",
                        "publication restore exceeded its bounded transition count",
                    ));
                }
                retry_attempt = 0;
            }
            Err(error)
                if cfg!(windows)
                    && addon_publication_fs::is_retryable_windows_code(error.raw_os_error())
                    && retry_attempt < 6 =>
            {
                let delay = addon_publication_fs::retry_delay_millis(operation_seed, retry_attempt);
                retry_attempt = retry_attempt.saturating_add(1);
                tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
            }
            Err(error) => return Err(publication_fs_repository_error(&error)),
        }
    }
}

#[allow(clippy::too_many_lines)] // Candidate acceptance and archive promotion share one transaction.
async fn accept_reconcile_candidate_in_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    list_id: EntityId,
    accepted_at: &str,
) -> Result<()> {
    let candidate = sqlx::query(
        r#"SELECT "import"."Id" AS "ImportJobId",
                  "import"."Generation" AS "ImportGeneration",
                  "import"."CheckpointId"
           FROM "AddonMediaListLifecycleJob" AS "lifecycle"
           JOIN "AddonMediaListImportJob" AS "import"
             ON "import"."Id" = "lifecycle"."CandidateImportJobId"
           JOIN "AddonMediaListImportCheckpoint" AS "checkpoint"
             ON "checkpoint"."Id" = "import"."CheckpointId"
           WHERE "lifecycle"."AddonMediaListId" = ?
             AND "lifecycle"."Kind" = ? AND "lifecycle"."Phase" = ?
             AND "import"."Purpose" = ? AND "import"."Mode" = ?
             AND "import"."State" = ? AND "checkpoint"."Committed" = 0
             AND "import"."Generation" = (
                 SELECT MAX("newest"."Generation")
                 FROM "AddonMediaListImportJob" AS "newest"
                 WHERE "newest"."AddonMediaListId" = ? AND "newest"."State" = ?
             )
           ORDER BY "lifecycle"."Generation" DESC, "lifecycle"."Id" DESC
           LIMIT 1"#,
    )
    .bind(list_id)
    .bind(crate::repositories::common::encode_enum(
        ersatzrs_domain::AddonMediaListLifecycleJobKind::Reconcile,
    ))
    .bind(crate::repositories::common::encode_enum(
        ersatzrs_domain::AddonMediaListLifecyclePhase::Complete,
    ))
    .bind(crate::repositories::common::encode_enum(
        ersatzrs_domain::AddonMediaListImportPurpose::Reconcile,
    ))
    .bind(crate::repositories::common::encode_enum(
        ersatzrs_domain::AddonMediaListImportMode::Full,
    ))
    .bind(crate::repositories::common::encode_enum(
        ersatzrs_domain::AddonMediaListImportState::Ready,
    ))
    .bind(list_id)
    .bind(crate::repositories::common::encode_enum(
        ersatzrs_domain::AddonMediaListImportState::Ready,
    ))
    .fetch_optional(&mut **tx)
    .await
    .repository()?;

    let unaccepted_reconcile_count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*)
           FROM "AddonMediaListLifecycleJob" AS "lifecycle"
           JOIN "AddonMediaListImportJob" AS "import"
             ON "import"."Id" = "lifecycle"."CandidateImportJobId"
           LEFT JOIN "AddonMediaListImportCheckpoint" AS "checkpoint"
             ON "checkpoint"."Id" = "import"."CheckpointId"
           WHERE "lifecycle"."AddonMediaListId" = ?
             AND "lifecycle"."Kind" = ? AND "lifecycle"."Phase" = ?
             AND "import"."Purpose" = ? AND "import"."Mode" = ?
             AND "import"."State" = ?
             AND ("import"."CheckpointId" IS NULL OR "checkpoint"."Committed" = 0)"#,
    )
    .bind(list_id)
    .bind(crate::repositories::common::encode_enum(
        ersatzrs_domain::AddonMediaListLifecycleJobKind::Reconcile,
    ))
    .bind(crate::repositories::common::encode_enum(
        ersatzrs_domain::AddonMediaListLifecyclePhase::Complete,
    ))
    .bind(crate::repositories::common::encode_enum(
        ersatzrs_domain::AddonMediaListImportPurpose::Reconcile,
    ))
    .bind(crate::repositories::common::encode_enum(
        ersatzrs_domain::AddonMediaListImportMode::Full,
    ))
    .bind(crate::repositories::common::encode_enum(
        ersatzrs_domain::AddonMediaListImportState::Ready,
    ))
    .fetch_one(&mut **tx)
    .await
    .repository()?;
    if candidate.is_none() && unaccepted_reconcile_count > 0 {
        return Err(RepositoryError::Conflict(
            "the completed reconciliation no longer matches a promotable staged archive".to_owned(),
        ));
    }

    let accepted_generation = if let Some(candidate) = candidate {
        let import_job_id: EntityId = candidate.try_get("ImportJobId").repository()?;
        let import_generation: i32 = candidate.try_get("ImportGeneration").repository()?;
        let checkpoint_id: EntityId = candidate.try_get("CheckpointId").repository()?;

        sqlx::query(
            r#"UPDATE "AddonMediaListImportJob" SET "CheckpointId" = NULL
               WHERE "AddonMediaListId" = ? AND "Id" <> ?"#,
        )
        .bind(list_id)
        .bind(import_job_id)
        .execute(&mut **tx)
        .await
        .repository()?;
        sqlx::query(
            r#"UPDATE "AddonMediaListImportCheckpoint" SET "Committed" = 0
               WHERE "AddonMediaListId" = ? AND "Id" <> ? AND "Committed" = 1"#,
        )
        .bind(list_id)
        .bind(checkpoint_id)
        .execute(&mut **tx)
        .await
        .repository()?;
        let promoted = sqlx::query(
            r#"UPDATE "AddonMediaListImportCheckpoint"
               SET "Committed" = 1, "Generation" = ?, "PromotedAt" = ?
               WHERE "Id" = ? AND "AddonMediaListId" = ? AND "Committed" = 0"#,
        )
        .bind(import_generation)
        .bind(accepted_at)
        .bind(checkpoint_id)
        .bind(list_id)
        .execute(&mut **tx)
        .await
        .repository()?;
        if promoted.rows_affected() != 1 {
            return Err(RepositoryError::Conflict(
                "the reconciliation archive changed before publication acceptance".to_owned(),
            ));
        }
        Some(import_generation)
    } else {
        None
    };

    sqlx::query(
        r#"DELETE FROM "AddonMediaListItem"
           WHERE "AddonMediaListId" = ? AND "Id" IN (
               SELECT "AddonMediaListItemId" FROM "AddonMediaListItemTombstone"
               WHERE "ConfirmedAt" IS NOT NULL
    )"#,
    )
    .bind(list_id)
    .execute(&mut **tx)
    .await
    .repository()?;
    sqlx::query(
        r#"DELETE FROM "AddonMediaListItemTombstone"
           WHERE "ReappearedAt" IS NOT NULL
             AND "AddonMediaListItemId" IN (
                 SELECT "Id" FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ?
             )"#,
    )
    .bind(list_id)
    .execute(&mut **tx)
    .await
    .repository()?;
    sqlx::query(r#"UPDATE "AddonMediaList" SET "ItemCount" = (SELECT COUNT(*) FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ? AND "InDraft" = 1) WHERE "Id" = ?"#)
        .bind(list_id)
        .bind(list_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    if let Some(import_generation) = accepted_generation {
        sqlx::query(
            r#"UPDATE "AddonMediaListLifecycleJob"
               SET "CandidateImportJobId" = NULL, "UpdatedAt" = ?
               WHERE "AddonMediaListId" = ? AND "Kind" = ? AND "Phase" = ?
                 AND "CandidateImportJobId" IN (
                     SELECT "Id" FROM "AddonMediaListImportJob"
                     WHERE "AddonMediaListId" = ? AND "Generation" <= ?
                       AND "Purpose" = ? AND "Mode" = ? AND "State" = ?
                 )"#,
        )
        .bind(accepted_at)
        .bind(list_id)
        .bind(crate::repositories::common::encode_enum(
            ersatzrs_domain::AddonMediaListLifecycleJobKind::Reconcile,
        ))
        .bind(crate::repositories::common::encode_enum(
            ersatzrs_domain::AddonMediaListLifecyclePhase::Complete,
        ))
        .bind(list_id)
        .bind(import_generation)
        .bind(crate::repositories::common::encode_enum(
            ersatzrs_domain::AddonMediaListImportPurpose::Reconcile,
        ))
        .bind(crate::repositories::common::encode_enum(
            ersatzrs_domain::AddonMediaListImportMode::Full,
        ))
        .bind(crate::repositories::common::encode_enum(
            ersatzrs_domain::AddonMediaListImportState::Ready,
        ))
        .execute(&mut **tx)
        .await
        .repository()?;
    }
    Ok(())
}

async fn load_published_item_snapshots(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
) -> Result<Vec<PublishedItemSnapshot>> {
    let mut connection = pool.acquire().await.repository()?;
    load_published_item_snapshots_on_connection(&mut connection, list_id).await
}

async fn load_published_item_snapshots_on_connection(
    connection: &mut <crate::db::Db as sqlx::Database>::Connection,
    list_id: EntityId,
) -> Result<Vec<PublishedItemSnapshot>> {
    let rows = sqlx::query(
        r#"SELECT "Id", "Published", "MediaItemId", "Kind", "ProviderId", "Title",
                  "Year", "Season", "Episode", "SourceUrl", "Rank", "PublishedLiveness",
                  "PublishedAvailability", "PublishedAvailabilityReason",
                  "PublishedAvailabilityReasonCode"
           FROM "AddonMediaListItem"
           WHERE "AddonMediaListId" = ?
           ORDER BY "Id""#,
    )
    .bind(list_id)
    .fetch_all(&mut *connection)
    .await
    .repository()?;
    let mut snapshots = Vec::with_capacity(rows.len());
    for row in rows {
        let id: EntityId = row.try_get("Id").repository()?;
        let guids = sqlx::query_scalar::<_, String>(
            r#"SELECT "Guid" FROM "AddonMediaListItemGuid"
               WHERE "AddonMediaListItemId" = ? ORDER BY "Id""#,
        )
        .bind(id)
        .fetch_all(&mut *connection)
        .await
        .repository()?;
        snapshots.push(PublishedItemSnapshot {
            id,
            published: decode_bool_column(&row, "Published")?,
            media_item_id: row.try_get("MediaItemId").repository()?,
            identity_captured: true,
            kind: Some(crate::repositories::common::decode_enum_column(
                "AddonMediaListItemKind",
                &row,
                "Kind",
            )?),
            provider_id: row.try_get("ProviderId").repository()?,
            title: row.try_get("Title").repository()?,
            year: row.try_get("Year").repository()?,
            season: row.try_get("Season").repository()?,
            episode: row.try_get("Episode").repository()?,
            source_url: row.try_get("SourceUrl").repository()?,
            rank: row.try_get("Rank").repository()?,
            guids: Some(guids),
            published_liveness: crate::repositories::common::decode_enum_column(
                "RemoteStreamLiveness",
                &row,
                "PublishedLiveness",
            )?,
            published_availability: crate::repositories::common::decode_enum_column(
                "AddonMediaListItemAvailability",
                &row,
                "PublishedAvailability",
            )?,
            published_availability_reason: row
                .try_get("PublishedAvailabilityReason")
                .repository()?,
            published_availability_reason_code:
                crate::repositories::common::decode_optional_enum_column(
                    "AddonMediaListAvailabilityReason",
                    &row,
                    "PublishedAvailabilityReasonCode",
                )?,
        });
    }
    Ok(snapshots)
}

#[allow(clippy::too_many_lines)]
async fn verify_published_publication_database(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    target_revision: i32,
    target: &PublicationDatabaseSnapshot,
) -> Result<()> {
    let row = sqlx::query(
        r#"SELECT "ReviewState", "PublishedRevision", "PresentationKind",
                  "RequestedPresentationKind", "GeneratePlaylist", "PlaylistId"
           FROM "AddonMediaList" WHERE "Id" = ?"#,
    )
    .bind(list_id)
    .fetch_optional(pool)
    .await
    .repository()?
    .ok_or(RepositoryError::NotFound {
        kind: "add-on media list",
        id: list_id,
    })?;
    let review_state: AddonMediaListReviewState = crate::repositories::common::decode_enum_column(
        "AddonMediaListReviewState",
        &row,
        "ReviewState",
    )?;
    let published_revision: i32 = row.try_get("PublishedRevision").repository()?;
    let presentation: AddonMediaListPresentationKind =
        crate::repositories::common::decode_enum_column(
            "AddonMediaListPresentationKind",
            &row,
            "PresentationKind",
        )?;
    let requested: AddonMediaListPresentationKind =
        crate::repositories::common::decode_enum_column(
            "AddonMediaListPresentationKind",
            &row,
            "RequestedPresentationKind",
        )?;
    let resolved_presentation = if requested == AddonMediaListPresentationKind::Auto {
        resolve_auto_presentation(pool, list_id).await?
    } else {
        requested
    };
    let generate_playlist = decode_bool_column(&row, "GeneratePlaylist")?;
    let playlist_id: Option<EntityId> = row.try_get("PlaylistId").repository()?;

    let counts = sqlx::query(
        r#"SELECT
             COUNT(*) AS "ItemCount",
             COALESCE(SUM(CASE WHEN "Published" = 1 THEN 1 ELSE 0 END), 0) AS "PublishedCount",
             COALESCE(SUM(CASE WHEN "Published" = 1
                           AND ("PublishedLiveness" <> "EffectiveLiveness"
                                OR "PublishedAvailability" <> "Availability"
                                OR COALESCE("PublishedAvailabilityReason", '')
                                   <> COALESCE("AvailabilityReason", '')
                                OR COALESCE("PublishedAvailabilityReasonCode", -1)
                                   <> COALESCE("AvailabilityReasonCode", -1))
                      THEN 1 ELSE 0 END), 0) AS "SnapshotMismatchCount",
             COALESCE(SUM(CASE WHEN "Published" = 1 AND (
                           "MediaItemId" IS NULL OR NOT EXISTS (
                               SELECT 1 FROM "MediaItem"
                               WHERE "Id" = "AddonMediaListItem"."MediaItemId"
                           )
                      ) THEN 1 ELSE 0 END), 0) AS "UnmatchedCount"
           FROM "AddonMediaListItem"
           WHERE "AddonMediaListId" = ? AND "ParentProviderId" IS NULL"#,
    )
    .bind(list_id)
    .fetch_one(pool)
    .await
    .repository()?;
    let item_count: i64 = counts.try_get("ItemCount").repository()?;
    let published_count: i64 = counts.try_get("PublishedCount").repository()?;
    let snapshot_mismatch_count: i64 = counts.try_get("SnapshotMismatchCount").repository()?;
    let unmatched_count: i64 = counts.try_get("UnmatchedCount").repository()?;
    let no_retired_items_published: bool = sqlx::query_scalar(
        r#"SELECT NOT EXISTS(SELECT 1 FROM "AddonMediaListItem"
           WHERE "AddonMediaListId" = ? AND "InDraft" = 0 AND "Published" = 1)"#,
    )
    .bind(list_id)
    .fetch_one(pool)
    .await
    .repository()?;
    let items = load_published_item_snapshots(pool, list_id).await?;
    let repository = SqliteAddonMediaListRepository(pool.clone());
    let source = repository.publication_source_snapshot(list_id).await?;
    let publication_selection_complete = if let Some(source) = &source {
        // A copied publication may intentionally include items excluded from
        // the newer draft. Verify its exact membership, not draft inclusion.
        let expected: std::collections::BTreeSet<_> = source
            .items
            .iter()
            .filter(|item| item.published)
            .map(|item| item.provider_id.as_str())
            .collect();
        let actual: std::collections::BTreeSet<_> = items
            .iter()
            .filter(|item| item.published)
            .filter_map(|item| item.provider_id.as_deref())
            .collect();
        expected == actual
    } else {
        no_retired_items_published
            && repository
                .query_items(list_id, None, 0, None, true)
                .await?
                .iter()
                .all(|item| {
                    item.published
                        == (item.included && !access_restricted_without_replacement(item))
                })
    };
    // Provider identity and replacement playback are deliberately separate:
    // `AddonMediaListItem.MediaItemId` owns the provider match, while an
    // unavailable item's configured replacement is selected only in the
    // generated playlist below.
    let stable_match_mismatch_count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*)
           FROM "AddonMediaListItem" AS "item"
           JOIN "AddonMediaList" AS "list" ON "list"."Id" = "item"."AddonMediaListId"
           LEFT JOIN "RemoteStream" AS "stream" ON "stream"."Id" = "item"."MediaItemId"
           WHERE "item"."AddonMediaListId" = ? AND "item"."ParentProviderId" IS NULL
             AND "item"."Published" = 1 AND "item"."Kind" = ?
             AND ("stream"."Id" IS NULL OR "stream"."AddonId" <> "list"."AddonId"
                  OR "stream"."ProviderId" <> "item"."ProviderId")"#,
    )
    .bind(list_id)
    .bind(crate::repositories::common::encode_enum(
        AddonMediaListItemKind::RemoteStream,
    ))
    .fetch_one(pool)
    .await
    .repository()?;

    let item_snapshot_complete = items == target.items;
    let projection_complete =
        verify_published_projection_database(pool, list_id, target.presentation_kind).await?;
    let playlist_complete = verify_published_playlist_database(
        pool,
        list_id,
        generate_playlist,
        playlist_id,
        target.playlist.as_ref(),
    )
    .await?;
    let materialization_complete = verify_published_materialization_database(
        pool,
        list_id,
        target.presentation_kind,
        target_revision,
        &target.materializations,
    )
    .await?;

    let mismatches: Vec<_> = [
        (
            review_state == AddonMediaListReviewState::Publishing,
            "review state",
        ),
        (published_revision == target_revision, "published revision"),
        (
            presentation == resolved_presentation,
            "requested presentation",
        ),
        (
            presentation == target.presentation_kind,
            "target presentation",
        ),
        (item_count > 0, "item count"),
        (
            published_count == target.published_item_count,
            "published count",
        ),
        (publication_selection_complete, "publication selection"),
        (snapshot_mismatch_count == 0, "published availability"),
        (unmatched_count == 0, "matched items"),
        (stable_match_mismatch_count == 0, "provider identity"),
        (item_snapshot_complete, "item snapshot"),
        (projection_complete, "projection"),
        (playlist_complete, "playlist"),
        (materialization_complete, "materialization"),
    ]
    .into_iter()
    .filter_map(|(valid, name)| (!valid).then_some(name))
    .collect();
    if !mismatches.is_empty() {
        return Err(RepositoryError::Conflict(format!(
            "the published add-on media-list revision failed convergence verification: {}",
            mismatches.join(", ")
        )));
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
async fn verify_published_projection_database(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    presentation: AddonMediaListPresentationKind,
) -> Result<bool> {
    let expected_projection_count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "AddonMediaListItem"
           WHERE "AddonMediaListId" = ? AND "ParentProviderId" IS NULL
             AND "Published" = 1 AND "Kind" = ?"#,
    )
    .bind(list_id)
    .bind(crate::repositories::common::encode_enum(
        AddonMediaListItemKind::RemoteStream,
    ))
    .fetch_one(pool)
    .await
    .repository()?;
    let total_projection_count: i64 = sqlx::query_scalar(
        r#"SELECT
             (SELECT COUNT(*) FROM "AddonMediaListItemEpisode" AS "projection"
                JOIN "AddonMediaListItem" AS "item"
                  ON "item"."Id" = "projection"."AddonMediaListItemId"
                WHERE "item"."AddonMediaListId" = ?)
             + (SELECT COUNT(*) FROM "AddonMediaListItemMovie" AS "projection"
                JOIN "AddonMediaListItem" AS "item"
                  ON "item"."Id" = "projection"."AddonMediaListItemId"
                WHERE "item"."AddonMediaListId" = ?)
             + (SELECT COUNT(*) FROM "AddonMediaListItemOtherVideo" AS "projection"
                JOIN "AddonMediaListItem" AS "item"
                  ON "item"."Id" = "projection"."AddonMediaListItemId"
                WHERE "item"."AddonMediaListId" = ?)
             + (SELECT COUNT(*) FROM "AddonMediaListItemSong" AS "projection"
                JOIN "AddonMediaListItem" AS "item"
                  ON "item"."Id" = "projection"."AddonMediaListItemId"
                WHERE "item"."AddonMediaListId" = ?)
             + (SELECT COUNT(*) FROM "AddonMediaListItemImage" AS "projection"
                JOIN "AddonMediaListItem" AS "item"
                  ON "item"."Id" = "projection"."AddonMediaListItemId"
                WHERE "item"."AddonMediaListId" = ?)
             + (SELECT COUNT(*) FROM "AddonMediaListItemMusicVideo" AS "projection"
                JOIN "AddonMediaListItem" AS "item"
                  ON "item"."Id" = "projection"."AddonMediaListItemId"
                WHERE "item"."AddonMediaListId" = ?)"#,
    )
    .bind(list_id)
    .bind(list_id)
    .bind(list_id)
    .bind(list_id)
    .bind(list_id)
    .bind(list_id)
    .fetch_one(pool)
    .await
    .repository()?;
    let parent_projection_count: i64 = sqlx::query_scalar(
        r#"SELECT
             (SELECT COUNT(*) FROM "AddonMediaListTelevisionShow"
                WHERE "AddonMediaListId" = ?)
             + (SELECT COUNT(*) FROM "AddonMediaListArtist"
                WHERE "AddonMediaListId" = ?)"#,
    )
    .bind(list_id)
    .bind(list_id)
    .fetch_one(pool)
    .await
    .repository()?;

    let (link_table, link_column, metadata_table, metadata_column, expected_parent_count) =
        match presentation {
            AddonMediaListPresentationKind::RemoteStreams
            | AddonMediaListPresentationKind::Auto => {
                return Ok(total_projection_count == 0 && parent_projection_count == 0);
            }
            AddonMediaListPresentationKind::TelevisionShow => (
                "AddonMediaListItemEpisode",
                "EpisodeId",
                "EpisodeMetadata",
                "EpisodeId",
                1_i64,
            ),
            AddonMediaListPresentationKind::Movies => (
                "AddonMediaListItemMovie",
                "MovieId",
                "MovieMetadata",
                "MovieId",
                0,
            ),
            AddonMediaListPresentationKind::OtherVideos => (
                "AddonMediaListItemOtherVideo",
                "OtherVideoId",
                "OtherVideoMetadata",
                "OtherVideoId",
                0,
            ),
            AddonMediaListPresentationKind::Songs => (
                "AddonMediaListItemSong",
                "SongId",
                "SongMetadata",
                "SongId",
                0,
            ),
            AddonMediaListPresentationKind::Images => (
                "AddonMediaListItemImage",
                "ImageId",
                "ImageMetadata",
                "ImageId",
                0,
            ),
            AddonMediaListPresentationKind::MusicVideos => (
                "AddonMediaListItemMusicVideo",
                "MusicVideoId",
                "MusicVideoMetadata",
                "MusicVideoId",
                1,
            ),
        };
    let incomplete_count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        r#"SELECT COUNT(*)
           FROM "AddonMediaListItem" AS "item"
           LEFT JOIN "{link_table}" AS "projection"
             ON "projection"."AddonMediaListItemId" = "item"."Id"
           LEFT JOIN "MediaItem" AS "media"
             ON "media"."Id" = "projection"."{link_column}"
           WHERE "item"."AddonMediaListId" = ?
             AND "item"."ParentProviderId" IS NULL AND "item"."Published" = 1
             AND "item"."Kind" = ?
             AND ("projection"."AddonMediaListItemId" IS NULL OR "media"."Id" IS NULL
                  OR ("item"."Kind" = ? AND NOT EXISTS (
                      SELECT 1 FROM "MediaVersion" AS "version"
                      WHERE "version"."{link_column}" = "projection"."{link_column}"
                        AND "version"."RemoteStreamId" = "item"."MediaItemId"
                  ))
                  OR ("item"."Kind" <> ?
                      AND "projection"."{link_column}" <> "item"."MediaItemId"))"#
    )))
    .bind(list_id)
    .bind(crate::repositories::common::encode_enum(
        AddonMediaListItemKind::RemoteStream,
    ))
    .bind(crate::repositories::common::encode_enum(
        AddonMediaListItemKind::RemoteStream,
    ))
    .bind(crate::repositories::common::encode_enum(
        AddonMediaListItemKind::RemoteStream,
    ))
    .fetch_one(pool)
    .await
    .repository()?;
    let metadata_mismatch_count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        r#"SELECT COUNT(*)
           FROM "AddonMediaListItem" AS "item"
           JOIN "{link_table}" AS "projection"
             ON "projection"."AddonMediaListItemId" = "item"."Id"
           WHERE "item"."AddonMediaListId" = ?
             AND "item"."ParentProviderId" IS NULL AND "item"."Published" = 1
             AND (SELECT COUNT(*) FROM "{metadata_table}" AS "metadata"
                  WHERE "metadata"."{metadata_column}" = "projection"."{link_column}") <> 1"#
    )))
    .bind(list_id)
    .fetch_one(pool)
    .await
    .repository()?;
    let parent_ownership_mismatch_count: i64 = match presentation {
        AddonMediaListPresentationKind::TelevisionShow => sqlx::query_scalar(
            r#"SELECT COUNT(*)
               FROM "AddonMediaListItem" AS "item"
               JOIN "AddonMediaListItemEpisode" AS "projection"
                 ON "projection"."AddonMediaListItemId" = "item"."Id"
               JOIN "Episode" AS "episode" ON "episode"."Id" = "projection"."EpisodeId"
               JOIN "Season" AS "season" ON "season"."Id" = "episode"."SeasonId"
               LEFT JOIN "AddonMediaListTelevisionShow" AS "owned"
                 ON "owned"."AddonMediaListId" = "item"."AddonMediaListId"
               WHERE "item"."AddonMediaListId" = ?
                 AND ("owned"."ShowId" IS NULL OR "season"."ShowId" <> "owned"."ShowId")"#,
        )
        .bind(list_id)
        .fetch_one(pool)
        .await
        .repository()?,
        AddonMediaListPresentationKind::MusicVideos => sqlx::query_scalar(
            r#"SELECT COUNT(*)
               FROM "AddonMediaListItem" AS "item"
               JOIN "AddonMediaListItemMusicVideo" AS "projection"
                 ON "projection"."AddonMediaListItemId" = "item"."Id"
               JOIN "MusicVideo" AS "video" ON "video"."Id" = "projection"."MusicVideoId"
               LEFT JOIN "AddonMediaListArtist" AS "owned"
                 ON "owned"."AddonMediaListId" = "item"."AddonMediaListId"
               WHERE "item"."AddonMediaListId" = ?
                 AND ("owned"."ArtistId" IS NULL OR "video"."ArtistId" <> "owned"."ArtistId")"#,
        )
        .bind(list_id)
        .fetch_one(pool)
        .await
        .repository()?,
        _ => 0,
    };
    let parent_metadata_mismatch_count: i64 = match presentation {
        AddonMediaListPresentationKind::TelevisionShow => sqlx::query_scalar(
            r#"SELECT
                 (SELECT COUNT(*) FROM "AddonMediaListTelevisionShow" AS "owned"
                  WHERE "owned"."AddonMediaListId" = ?
                    AND (SELECT COUNT(*) FROM "ShowMetadata" AS "metadata"
                         WHERE "metadata"."ShowId" = "owned"."ShowId") <> 1)
                 +
                 (SELECT COUNT(*) FROM "AddonMediaListItemEpisode" AS "projection"
                  JOIN "AddonMediaListItem" AS "item"
                    ON "item"."Id" = "projection"."AddonMediaListItemId"
                  JOIN "Episode" AS "episode" ON "episode"."Id" = "projection"."EpisodeId"
                  WHERE "item"."AddonMediaListId" = ? AND "item"."Published" = 1
                    AND (SELECT COUNT(*) FROM "SeasonMetadata" AS "metadata"
                         WHERE "metadata"."SeasonId" = "episode"."SeasonId") <> 1)"#,
        )
        .bind(list_id)
        .bind(list_id)
        .fetch_one(pool)
        .await
        .repository()?,
        AddonMediaListPresentationKind::MusicVideos => sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "AddonMediaListArtist" AS "owned"
               WHERE "owned"."AddonMediaListId" = ?
                 AND (SELECT COUNT(*) FROM "ArtistMetadata" AS "metadata"
                      WHERE "metadata"."ArtistId" = "owned"."ArtistId") <> 1"#,
        )
        .bind(list_id)
        .fetch_one(pool)
        .await
        .repository()?,
        _ => 0,
    };
    Ok(total_projection_count == expected_projection_count
        && parent_projection_count == expected_parent_count
        && incomplete_count == 0
        && metadata_mismatch_count == 0
        && parent_ownership_mismatch_count == 0
        && parent_metadata_mismatch_count == 0)
}

async fn verify_published_playlist_database(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    generate_playlist: bool,
    playlist_id: Option<EntityId>,
    target: Option<&PlaylistSnapshot>,
) -> Result<bool> {
    let (Some(playlist_id), Some(target)) = (playlist_id, target) else {
        return Ok(!generate_playlist && playlist_id.is_none() && target.is_none());
    };
    if !generate_playlist || playlist_id != target.id {
        return Ok(false);
    }
    let current = load_playlist_snapshot(pool, list_id).await?;
    let Some(current) = current else {
        return Ok(false);
    };
    let target_count = i64::try_from(target.items.len()).unwrap_or(i64::MAX);
    Ok(current.has_same_published_content(target)
        && verify_playlist_membership_database(pool, list_id, playlist_id, target_count).await?)
}

async fn verify_playlist_membership_database(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    playlist_id: EntityId,
    expected_count: i64,
) -> Result<bool> {
    let row = sqlx::query(
        r#"SELECT COUNT(*) AS "ItemCount",
                  COALESCE(SUM(CASE WHEN "item"."MediaItemId" IS NULL OR "media"."Id" IS NULL
                           THEN 1 ELSE 0 END), 0) AS "InvalidMediaCount"
           FROM "PlaylistItem" AS "item"
           LEFT JOIN "MediaItem" AS "media" ON "media"."Id" = "item"."MediaItemId"
           WHERE "item"."PlaylistId" = ?
             AND EXISTS (SELECT 1 FROM "Playlist" WHERE "Id" = ?)
             AND EXISTS (SELECT 1 FROM "AddonMediaList"
                         WHERE "Id" = ? AND "PlaylistId" = ?)"#,
    )
    .bind(playlist_id)
    .bind(playlist_id)
    .bind(list_id)
    .bind(playlist_id)
    .fetch_one(pool)
    .await
    .repository()?;
    let item_count: i64 = row.try_get("ItemCount").repository()?;
    let invalid_media_count: i64 = row.try_get("InvalidMediaCount").repository()?;
    let wrong_membership_count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*)
           FROM "PlaylistItem" AS "playlist_item"
           WHERE "playlist_item"."PlaylistId" = ?
             AND NOT EXISTS (
                 SELECT 1
                 FROM "AddonMediaListItem" AS "item"
                 LEFT JOIN "AddonMediaListItemReplacement" AS "replacement"
                   ON "replacement"."AddonMediaListItemId" = "item"."Id"
                 WHERE "item"."AddonMediaListId" = ?
                   AND "item"."ParentProviderId" IS NULL AND "item"."Published" = 1
                   AND "playlist_item"."Index" = "item"."Rank"
                   AND (
                     ("item"."Availability" <> ? AND (
                        "playlist_item"."MediaItemId" = COALESCE(
                            (SELECT "EpisodeId" FROM "AddonMediaListItemEpisode"
                             WHERE "AddonMediaListItemId" = "item"."Id"),
                            "item"."MediaItemId"
                        )
                     ))
                     OR ("item"."Availability" = ?
                         AND "playlist_item"."MediaItemId" = "replacement"."MediaItemId"
                         AND "playlist_item"."MediaVersionId" = "replacement"."MediaVersionId")
                   )
             )"#,
    )
    .bind(playlist_id)
    .bind(list_id)
    .bind(crate::repositories::common::encode_enum(
        AddonMediaListItemAvailability::Unavailable,
    ))
    .bind(crate::repositories::common::encode_enum(
        AddonMediaListItemAvailability::Unavailable,
    ))
    .fetch_one(pool)
    .await
    .repository()?;
    Ok(item_count == expected_count && invalid_media_count == 0 && wrong_membership_count == 0)
}

async fn verify_published_materialization_database(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    presentation: AddonMediaListPresentationKind,
    target_revision: i32,
    target: &[ArtworkMaterializationSnapshot],
) -> Result<bool> {
    let current = load_artwork_materializations(pool, list_id).await?;
    if current.len() != target.len()
        || current
            .iter()
            .any(|materialization| materialization.published_revision != target_revision)
    {
        return Ok(false);
    }
    let mut matched = vec![false; target.len()];
    for current in &current {
        let Some((index, _)) = target
            .iter()
            .enumerate()
            .find(|(index, expected)| !matched[*index] && current.has_same_content(expected))
        else {
            return Ok(false);
        };
        matched[index] = true;
    }
    let managed_root: String = sqlx::query_scalar(
        r#"SELECT "ManifestPath" FROM "AddonMediaListLibrary"
           WHERE "AddonMediaListId" = ?"#,
    )
    .bind(list_id)
    .fetch_one(pool)
    .await
    .repository()?;
    let manifest_path = Path::new(&managed_root);
    let managed_root = if manifest_path.is_dir() {
        manifest_path
    } else {
        let Some(parent) = manifest_path.parent() else {
            return Ok(false);
        };
        parent
    };
    if managed_root.as_os_str().is_empty() {
        return Ok(false);
    }
    for materialization in &current {
        let relative = Path::new(&materialization.file_name);
        if relative.is_absolute()
            || relative
                .components()
                .any(|component| !matches!(component, std::path::Component::Normal(_)))
        {
            return Ok(false);
        }
        let Some(path) = publication_artwork_path_for(
            pool,
            list_id,
            presentation,
            materialization,
            managed_root,
        )
        .await?
        else {
            return Ok(false);
        };
        if !path.is_file() || !file_has_sha256(&path, &materialization.sha256) {
            return Ok(false);
        }
    }
    let expected_parent_roles = if parent_nfo_for(presentation).is_some() {
        selected_parent_artwork(&load_list_metadata_form(pool, list_id).await?.metadata)
            .into_iter()
            .filter_map(|candidate| match candidate.role {
                AddonMediaListArtworkRole::Poster
                | AddonMediaListArtworkRole::Fanart
                | AddonMediaListArtworkRole::Thumb => Some(artwork_role_value(candidate.role)),
                AddonMediaListArtworkRole::Other => None,
            })
            .collect::<std::collections::BTreeSet<_>>()
    } else {
        std::collections::BTreeSet::new()
    };
    let current_parent_roles = current
        .iter()
        .filter(|materialization| {
            materialization.item_id.is_none() && materialization.scope_item_id == 0
        })
        .map(|materialization| materialization.role)
        .collect::<std::collections::BTreeSet<_>>();
    let current_parent_count = current
        .iter()
        .filter(|materialization| {
            materialization.item_id.is_none() && materialization.scope_item_id == 0
        })
        .count();
    Ok(current_parent_roles == expected_parent_roles
        && current_parent_count == current_parent_roles.len())
}

/// Artwork is written beside its owner's NFO, which can be in a playlist's
/// nested media folder rather than beside the manifest itself.
async fn publication_artwork_path_for(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    presentation: AddonMediaListPresentationKind,
    materialization: &ArtworkMaterializationSnapshot,
    managed_root: &Path,
) -> Result<Option<PathBuf>> {
    let owner_nfo = if let Some(item_id) = materialization.item_id {
        item_nfo_path_for(pool, list_id, item_id).await?
    } else if let Some((file_name, _)) = parent_nfo_for(presentation) {
        parent_nfo_path_for(pool, list_id, file_name).await?
    } else {
        None
    };
    let Some(owner_directory) = owner_nfo.as_deref().and_then(Path::parent) else {
        return Ok(None);
    };
    let Ok(root) = managed_root.canonicalize() else {
        return Ok(None);
    };
    let Ok(path) = owner_directory
        .join(&materialization.file_name)
        .canonicalize()
    else {
        return Ok(None);
    };
    Ok(path.starts_with(root).then_some(path))
}

async fn verify_publication_database(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    journal: &PublicationJournal,
    previous_revision: i32,
    target_revision: i32,
    outcome: AddonMediaListPublicationOutcome,
) -> Result<()> {
    let row = sqlx::query(
        r#"SELECT "ReviewState", "PublishedRevision"
           FROM "AddonMediaList" WHERE "Id" = ?"#,
    )
    .bind(list_id)
    .fetch_optional(pool)
    .await
    .repository()?
    .ok_or(RepositoryError::NotFound {
        kind: "add-on media list",
        id: list_id,
    })?;
    let review_state: AddonMediaListReviewState = crate::repositories::common::decode_enum_column(
        "AddonMediaListReviewState",
        &row,
        "ReviewState",
    )?;
    let published_revision: i32 = row.try_get("PublishedRevision").repository()?;

    match outcome {
        AddonMediaListPublicationOutcome::Published => {
            if let Some(target) = journal.target_database.as_ref() {
                verify_published_publication_database(pool, list_id, target_revision, target)
                    .await?;
            } else if review_state != AddonMediaListReviewState::Publishing
                || published_revision != target_revision
            {
                return Err(RepositoryError::Conflict(
                    "the legacy published add-on media-list revision failed terminal verification"
                        .to_owned(),
                ));
            }
        }
        AddonMediaListPublicationOutcome::RestoredAfterFailure => {
            let items = load_published_item_snapshots(pool, list_id).await?;
            let materializations = load_artwork_materializations(pool, list_id).await?;
            let playlist = load_playlist_snapshot(pool, list_id).await?;
            let items_match = items.len() == journal.previous_items.len()
                && items
                    .iter()
                    .zip(&journal.previous_items)
                    .all(|(current, previous)| current.has_same_restored_content(previous));
            let playlist_matches =
                restored_playlists_match(playlist.as_ref(), journal.previous_playlist.as_ref());
            let restored_structure_matches =
                verify_restored_publication_structure(pool, list_id, journal, playlist.as_ref())
                    .await?;
            let mut mismatches = Vec::new();
            if review_state != AddonMediaListReviewState::Publishing {
                mismatches.push("review state");
            }
            if published_revision != previous_revision {
                mismatches.push("published revision");
            }
            if !items_match {
                mismatches.push("published items");
            }
            if materializations != journal.previous_materializations {
                mismatches.push("artwork materializations");
            }
            if !playlist_matches {
                mismatches.push("generated playlist");
            }
            if !restored_structure_matches {
                mismatches.push("matching or projection structure");
            }
            if !mismatches.is_empty() {
                return Err(RepositoryError::Conflict(format!(
                    "the restored add-on media-list revision failed convergence verification: {}",
                    mismatches.join(", ")
                )));
            }
        }
    }
    Ok(())
}

async fn verify_restored_publication_structure(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    journal: &PublicationJournal,
    playlist: Option<&PlaylistSnapshot>,
) -> Result<bool> {
    let presentation: AddonMediaListPresentationKind =
        crate::repositories::common::decode_enum_column(
            "AddonMediaListPresentationKind",
            &sqlx::query(r#"SELECT "PresentationKind" FROM "AddonMediaList" WHERE "Id" = ?"#)
                .bind(list_id)
                .fetch_one(pool)
                .await
                .repository()?,
            "PresentationKind",
        )?;
    let rows = sqlx::query(
        r#"SELECT "item"."Id", "item"."Kind", "item"."MediaItemId",
                  "stream"."AddonId" AS "StreamAddonId",
                  "stream"."ProviderId" AS "StreamProviderId",
                  "list"."AddonId", "item"."ProviderId"
           FROM "AddonMediaListItem" AS "item"
           JOIN "AddonMediaList" AS "list" ON "list"."Id" = "item"."AddonMediaListId"
           LEFT JOIN "MediaItem" AS "media" ON "media"."Id" = "item"."MediaItemId"
           LEFT JOIN "RemoteStream" AS "stream" ON "stream"."Id" = "item"."MediaItemId"
           WHERE "item"."AddonMediaListId" = ? AND "item"."ParentProviderId" IS NULL
             AND "item"."Published" = 1 AND "media"."Id" IS NOT NULL
           ORDER BY "item"."Id""#,
    )
    .bind(list_id)
    .fetch_all(pool)
    .await
    .repository()?;
    let expected_published: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "AddonMediaListItem"
           WHERE "AddonMediaListId" = ? AND "ParentProviderId" IS NULL
             AND "Published" = 1"#,
    )
    .bind(list_id)
    .fetch_one(pool)
    .await
    .repository()?;
    if i64::try_from(rows.len()).unwrap_or(i64::MAX) != expected_published {
        return Ok(false);
    }
    for row in &rows {
        let id: EntityId = row.try_get("Id").repository()?;
        let kind: AddonMediaListItemKind =
            crate::repositories::common::decode_enum_column("AddonMediaListItemKind", row, "Kind")?;
        let media_item_id: Option<EntityId> = row.try_get("MediaItemId").repository()?;
        let Some(previous) = journal.previous_items.iter().find(|item| item.id == id) else {
            return Ok(false);
        };
        let matches = if kind == AddonMediaListItemKind::RemoteStream {
            row.try_get::<Option<String>, _>("StreamAddonId")
                .repository()?
                == Some(row.try_get("AddonId").repository()?)
                && row
                    .try_get::<Option<String>, _>("StreamProviderId")
                    .repository()?
                    == Some(row.try_get("ProviderId").repository()?)
        } else {
            media_item_id == previous.media_item_id
        };
        if !matches {
            return Ok(false);
        }
    }
    if presentation != journal.previous_presentation_kind
        || !verify_published_projection_database(pool, list_id, presentation).await?
    {
        return Ok(false);
    }
    match (playlist, journal.previous_playlist.as_ref()) {
        (None, None) => Ok(true),
        (Some(current), Some(previous)) if current.id == previous.id => {
            verify_playlist_membership_database(
                pool,
                list_id,
                current.id,
                i64::try_from(previous.items.len()).unwrap_or(i64::MAX),
            )
            .await
        }
        _ => Ok(false),
    }
}

fn restored_playlists_match(
    current: Option<&PlaylistSnapshot>,
    previous: Option<&PlaylistSnapshot>,
) -> bool {
    match (current, previous) {
        (None, None) => true,
        (Some(current), Some(previous)) => current.has_same_restored_content(previous),
        _ => false,
    }
}

async fn load_playlist_snapshot(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
) -> Result<Option<PlaylistSnapshot>> {
    let playlist_id = sqlx::query_scalar::<_, Option<EntityId>>(
        r#"SELECT "PlaylistId" FROM "AddonMediaList" WHERE "Id" = ?"#,
    )
    .bind(list_id)
    .fetch_one(pool)
    .await
    .repository()?;
    let Some(playlist_id) = playlist_id else {
        return Ok(None);
    };
    let playlist = sqlx::query(
        r#"SELECT "Id", "IsSystem", "Name", "PlaylistGroupId"
           FROM "Playlist" WHERE "Id" = ?"#,
    )
    .bind(playlist_id)
    .fetch_one(pool)
    .await
    .repository()?;
    let items = sqlx::query(
        r#"SELECT "Id", "Index", "CollectionType", "CollectionId", "MediaItemId",
                  "MultiCollectionId", "SmartCollectionId", "IncludeInProgramGuide",
                  "PlaybackOrder", "PlayAll", "Count", "PreferredAudioLanguageCode",
                  "PreferredAudioTitle", "PreferredSubtitleLanguageCode", "SubtitleMode",
                  "MediaVersionId", "VideoStreamIndex", "AudioStreamIndex",
                  "SubtitleStreamIndex"
           FROM "PlaylistItem" WHERE "PlaylistId" = ? ORDER BY "Index", "Id""#,
    )
    .bind(playlist_id)
    .fetch_all(pool)
    .await
    .repository()?
    .iter()
    .map(|row| {
        Ok(PlaylistItemSnapshot {
            id: row.try_get("Id").repository()?,
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
            preferred_audio_language_code: row
                .try_get("PreferredAudioLanguageCode")
                .repository()?,
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
    })
    .collect::<Result<Vec<_>>>()?;
    Ok(Some(PlaylistSnapshot {
        id: playlist.try_get("Id").repository()?,
        is_system: decode_bool_column(&playlist, "IsSystem")?,
        name: playlist.try_get("Name").repository()?,
        playlist_group_id: playlist.try_get("PlaylistGroupId").repository()?,
        items,
    }))
}

async fn restore_playlist_snapshot(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    snapshot: Option<&PlaylistSnapshot>,
) -> Result<()> {
    let current_id = sqlx::query_scalar::<_, Option<EntityId>>(
        r#"SELECT "PlaylistId" FROM "AddonMediaList" WHERE "Id" = ?"#,
    )
    .bind(list_id)
    .fetch_one(pool)
    .await
    .repository()?;
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    match snapshot {
        None => {
            if let Some(current_id) = current_id {
                sqlx::query(r#"DELETE FROM "Playlist" WHERE "Id" = ?"#)
                    .bind(current_id)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
            }
            sqlx::query(r#"UPDATE "AddonMediaList" SET "PlaylistId" = NULL WHERE "Id" = ?"#)
                .bind(list_id)
                .execute(&mut *tx)
                .await
                .repository()?;
        }
        Some(snapshot) => {
            if current_id.is_some_and(|current_id| current_id != snapshot.id) {
                sqlx::query(r#"DELETE FROM "Playlist" WHERE "Id" = ?"#)
                    .bind(current_id)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
            }
            let exists: i64 =
                sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Playlist" WHERE "Id" = ?"#)
                    .bind(snapshot.id)
                    .fetch_one(&mut *tx)
                    .await
                    .repository()?;
            if exists == 0 {
                sqlx::query(
                    r#"INSERT INTO "Playlist" ("Id", "IsSystem", "Name", "PlaylistGroupId")
                       VALUES (?, ?, ?, ?)"#,
                )
                .bind(snapshot.id)
                .bind(snapshot.is_system)
                .bind(&snapshot.name)
                .bind(snapshot.playlist_group_id)
                .execute(&mut *tx)
                .await
                .repository()?;
            } else {
                sqlx::query(
                    r#"UPDATE "Playlist"
                       SET "IsSystem" = ?, "Name" = ?, "PlaylistGroupId" = ?
                       WHERE "Id" = ?"#,
                )
                .bind(snapshot.is_system)
                .bind(&snapshot.name)
                .bind(snapshot.playlist_group_id)
                .bind(snapshot.id)
                .execute(&mut *tx)
                .await
                .repository()?;
            }
            sqlx::query(r#"UPDATE "AddonMediaList" SET "PlaylistId" = ? WHERE "Id" = ?"#)
                .bind(snapshot.id)
                .bind(list_id)
                .execute(&mut *tx)
                .await
                .repository()?;
            sqlx::query(r#"DELETE FROM "PlaylistItem" WHERE "PlaylistId" = ?"#)
                .bind(snapshot.id)
                .execute(&mut *tx)
                .await
                .repository()?;
        }
    }
    tx.commit().await.repository()?;
    Ok(())
}

fn map_nfo_error(error: crate::addon_media_nfo::AddonMediaNfoError) -> RepositoryError {
    match error {
        crate::addon_media_nfo::AddonMediaNfoError::Conflict => {
            RepositoryError::Conflict(error.to_string())
        }
        crate::addon_media_nfo::AddonMediaNfoError::Invalid(message) => RepositoryError::Decode {
            kind: "managed add-on NFO",
            value: message,
        },
        _ => RepositoryError::unavailable("managed add-on NFO", error.to_string()),
    }
}

async fn load_list_metadata_form(
    pool: &Pool<crate::db::Db>,
    id: EntityId,
) -> Result<AddonMediaListNfoForm> {
    let row = sqlx::query(
        r#"SELECT "ProviderMetadataJson", "MetadataOverrideJson", "MetadataConflictJson"
           FROM "AddonMediaList" WHERE "Id" = ?"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .repository()?
    .ok_or(RepositoryError::NotFound {
        kind: "add-on media list",
        id,
    })?;
    let provider_json: String = row.try_get("ProviderMetadataJson").repository()?;
    let override_json: String = row.try_get("MetadataOverrideJson").repository()?;
    let conflict_json: Option<String> = row.try_get("MetadataConflictJson").repository()?;
    metadata_form_from_json(&provider_json, &override_json, conflict_json.as_deref())
}

async fn require_draft_item_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    list_id: EntityId,
    item_id: EntityId,
) -> Result<()> {
    let exists: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM "AddonMediaListItem"
           WHERE "Id" = ? AND "AddonMediaListId" = ? AND "InDraft" = 1)"#,
    )
    .bind(item_id)
    .bind(list_id)
    .fetch_one(&mut **tx)
    .await
    .repository()?;
    if !exists {
        return Err(RepositoryError::NotFound {
            kind: "add-on media list item",
            id: item_id,
        });
    }
    Ok(())
}

async fn load_item_metadata_form(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    item_id: EntityId,
) -> Result<AddonMediaListNfoForm> {
    let row = sqlx::query(
        r#"SELECT "ProviderMetadataJson", "MetadataOverrideJson", "MetadataConflictJson"
           FROM "AddonMediaListItem"
           WHERE "Id" = ? AND "AddonMediaListId" = ? AND "InDraft" = 1"#,
    )
    .bind(item_id)
    .bind(list_id)
    .fetch_optional(pool)
    .await
    .repository()?
    .ok_or(RepositoryError::NotFound {
        kind: "add-on media list item",
        id: item_id,
    })?;
    let provider_json: String = row.try_get("ProviderMetadataJson").repository()?;
    let override_json: String = row.try_get("MetadataOverrideJson").repository()?;
    let conflict_json: Option<String> = row.try_get("MetadataConflictJson").repository()?;
    metadata_form_from_json(&provider_json, &override_json, conflict_json.as_deref())
}

fn metadata_form_from_json(
    provider_json: &str,
    override_json: &str,
    conflict_json: Option<&str>,
) -> Result<AddonMediaListNfoForm> {
    let provider = metadata_json_value(provider_json)?;
    let overrides = metadata_json_value(override_json)?;
    let effective = merge_metadata_values(provider.clone(), &overrides);
    let metadata = serde_json::from_value(effective).map_err(|error| RepositoryError::Decode {
        kind: "add-on metadata",
        value: error.to_string(),
    })?;
    let conflicts = conflict_json
        .filter(|value| !value.trim().is_empty())
        .map(
            serde_json::from_str::<
                std::collections::BTreeMap<String, AddonMediaListMetadataConflict>,
            >,
        )
        .transpose()
        .map_err(|error| RepositoryError::Decode {
            kind: "add-on metadata conflicts",
            value: error.to_string(),
        })?
        .unwrap_or_default();
    Ok(AddonMediaListNfoForm {
        metadata,
        etag: metadata_json_etag(&provider, &overrides),
        ownership: if overrides.as_object().is_some_and(serde_json::Map::is_empty) {
            AddonMediaListNfoOwnership::Generated
        } else {
            AddonMediaListNfoOwnership::User
        },
        error: None,
        path_available: false,
        conflicts,
    })
}

fn metadata_json_value(json: &str) -> Result<serde_json::Value> {
    serde_json::from_str(json).map_err(|error| RepositoryError::Decode {
        kind: "add-on metadata JSON",
        value: error.to_string(),
    })
}

fn merge_metadata_values(
    mut provider: serde_json::Value,
    overrides: &serde_json::Value,
) -> serde_json::Value {
    if let (Some(provider), Some(overrides)) = (provider.as_object_mut(), overrides.as_object()) {
        for (field, value) in overrides {
            provider.insert(field.clone(), value.clone());
        }
    }
    provider
}

fn metadata_json_etag(provider: &serde_json::Value, overrides: &serde_json::Value) -> String {
    let mut digest = Sha256::new();
    digest.update(serde_json::to_vec(provider).unwrap_or_default());
    digest.update([0]);
    digest.update(serde_json::to_vec(overrides).unwrap_or_default());
    let mut etag = String::with_capacity(64);
    for byte in digest.finalize() {
        write!(&mut etag, "{byte:02x}").expect("writing to a String cannot fail");
    }
    etag
}

fn hex_hash(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    let mut value = String::with_capacity(64);
    for byte in digest.finalize() {
        write!(&mut value, "{byte:02x}").expect("writing to a String cannot fail");
    }
    value
}

pub(crate) const fn artwork_role_value(role: AddonMediaListArtworkRole) -> i32 {
    match role {
        AddonMediaListArtworkRole::Poster => 1,
        AddonMediaListArtworkRole::Fanart => 2,
        AddonMediaListArtworkRole::Thumb => 3,
        AddonMediaListArtworkRole::Other => 0,
    }
}

/// Move an already published list back to review while preserving its last
/// published item snapshot and library projection. A never-published import
/// remains a draft until its first explicit publication.
///
/// Refused while a publication holds a live lease, and also once that publication
/// has selected the revision it will verify. From `begin_publication_verification`
/// until the recovery journal is cleared, the state machine checks the restored or committed
/// revision against `Publishing`, so an edit that overwrote it would leave
/// verification unable to converge and retrying forever. A live lease alone does
/// not cover that window: a publication between verification retries holds no
/// lease. An abandoned publication that never reached verification stays
/// editable, so an expired lease still releases the draft.
async fn mark_list_ready_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    list_id: EntityId,
) -> Result<()> {
    let now = encode_datetime(Utc::now());
    let updated = sqlx::query(
        r#"UPDATE "AddonMediaList"
           SET "ReviewState" = CASE WHEN "ReviewState" = 0 THEN 0 ELSE 1 END,
               "ReviewError" = NULL,
               "DraftRevision" = "DraftRevision" + 1
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
    .bind(list_id)
    .bind(crate::repositories::common::encode_enum(
        ersatzrs_domain::ManagedLibraryOperationState::Running,
    ))
    .bind(now)
    .execute(&mut **tx)
    .await
    .repository()?;
    if updated.rows_affected() != 1 {
        return Err(publication_in_progress());
    }
    Ok(())
}

fn publication_in_progress() -> RepositoryError {
    RepositoryError::Conflict(
        "the add-on media list is being published; wait for publication to finish".to_owned(),
    )
}

#[allow(clippy::too_many_lines)] // Keep streamed and downloaded readiness in one shared classification.
async fn addon_media_list_publication_readiness(
    pool: &Pool<crate::db::Db>,
    form: &AddonMediaListAdminForm,
    items: &[AddonMediaListItemAdminRow],
) -> Result<AddonMediaListPublicationReadiness> {
    let list_id = form.id;
    let removal_candidate_ids = sqlx::query_scalar::<_, EntityId>(
        r#"SELECT "AddonMediaListItemId" FROM "AddonMediaListItemTombstone"
           WHERE "ConfirmedAt" IS NOT NULL
             AND "AddonMediaListItemId" IN (
                 SELECT "Id" FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ?
             )"#,
    )
    .bind(list_id)
    .fetch_all(pool)
    .await
    .repository()?
    .into_iter()
    .collect::<std::collections::BTreeSet<_>>();
    let included = items
        .iter()
        .filter(|item| item.included && !removal_candidate_ids.contains(&item.id))
        .collect::<Vec<_>>();
    let attempted = included
        .iter()
        .copied()
        .filter(|item| !access_restricted_without_replacement(item))
        .collect::<Vec<_>>();
    let presentation = if form.requested_presentation_kind == AddonMediaListPresentationKind::Auto {
        resolve_included_presentation(&attempted)
    } else {
        form.requested_presentation_kind
    };
    let mut blockers = list_publication_blockers(pool, form, presentation, &attempted).await?;
    blockers.extend(
        included
            .iter()
            .filter(|item| item.replacement_unresolved)
            .map(|item| {
                publication_item_blocker(
                    AddonMediaListPublicationBlockerKind::ReplacementUnresolved,
                    item,
                )
            }),
    );
    let detail_states = required_playback_detail_states(pool, list_id).await?;
    let mut item_classification = PublicationItemClassification::default();
    let excluded = items
        .iter()
        .filter(|item| {
            removal_candidate_ids.contains(&item.id) || access_restricted_without_replacement(item)
        })
        .map(|item| item.id)
        .collect::<Vec<_>>();
    let preparation = crate::addon_media_list_prepared_selection::interval_metadata_requirements(
        pool,
        list_id,
        form.draft_revision,
        &excluded,
    )
    .await?;
    let downloads = crate::addon_media_list_prepared_selection::preparation_requirements(
        pool,
        list_id,
        form.draft_revision,
        &excluded,
    )
    .await?;
    item_classification.preparation_required = preparation
        .iter()
        .map(|item| item.item_id)
        .chain(downloads.iter().map(|item| item.item_id))
        .collect();
    let subtitles = crate::addon_media_list_subtitle_selection::subtitle_requirements(
        pool,
        list_id,
        form.draft_revision,
        &excluded,
    )
    .await?;
    let required_prepared_media_count = subtitles
        .iter()
        .filter(|item| item.artifacts.is_none())
        .count() as u64
        + preparation
            .iter()
            .filter(|item| item.plan.is_none())
            .count() as u64
        + downloads
            .iter()
            .filter(|item| item.artifact_id.is_none())
            .count() as u64;
    let required_prepared_media_pending = subtitles
        .iter()
        .filter(|item| item.artifacts.is_none() && item.pending)
        .count() as u64
        + preparation
            .iter()
            .filter(|item| item.plan.is_none() && item.pending)
            .count() as u64
        + downloads
            .iter()
            .filter(|item| item.artifact_id.is_none() && item.pending)
            .count() as u64;
    let missing_processing = downloads
        .iter()
        .filter(|item| item.artifact_id.is_none() && !item.pending)
        .map(|item| (item.item_id, 1))
        .chain(
            preparation
                .iter()
                .filter(|item| item.plan.is_none() && !item.pending)
                .map(|item| (item.item_id, 2)),
        )
        .chain(
            subtitles
                .iter()
                .filter(|item| item.artifacts.is_none() && !item.pending)
                .map(|item| (item.item_id, 3)),
        )
        .collect();
    let (required_prepared_media_failed, prepared_media_failures) =
        crate::addon_media_list_preparation_status::preparation_failures(
            pool,
            list_id,
            form.draft_revision,
            &missing_processing,
        )
        .await?;
    for item in &included {
        // Receiving replacement intent must be resolved before considering the
        // original source's access, playback, or metadata requirements.
        if item.replacement_unresolved {
            continue;
        }
        if access_restricted_without_replacement(item) {
            item_classification.restricted_excluded_count = item_classification
                .restricted_excluded_count
                .saturating_add(1);
            item_classification.recommendation_count =
                item_classification.recommendation_count.saturating_add(1);
            continue;
        }
        classify_item_advisory_and_playback(
            presentation,
            item,
            detail_states.get(&item.id).copied().flatten(),
            &mut item_classification,
        );
        classify_item_metadata(pool, list_id, presentation, item, &mut item_classification).await?;
    }
    blockers.extend(item_classification.blockers);
    let progress = form.import_progress.as_ref();
    let restricted_excluded_count = item_classification.restricted_excluded_count;
    Ok(AddonMediaListPublicationReadiness {
        included_count: u64::try_from(attempted.len()).unwrap_or(u64::MAX),
        restricted_excluded_count,
        blockers,
        recommendation_count: item_classification.recommendation_count,
        required_playback_detail_count: item_classification.required_playback_detail_count,
        required_playback_detail_pending: item_classification.required_playback_detail_pending,
        required_prepared_media_count,
        required_prepared_media_pending,
        required_prepared_media_failed,
        prepared_media_failures,
        optional_detail_total: progress.map_or(0, |progress| progress.detail_total),
        optional_detail_complete: progress.map_or(0, |progress| progress.detail_complete),
        optional_detail_pending: progress.map_or(0, |progress| progress.detail_pending),
        saved_revision: form.draft_revision,
        review_state: form.review_state,
        publication: None,
    })
}

async fn publication_operation_blockers(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
) -> Result<Vec<AddonMediaListPublicationBlocker>> {
    let mut blockers = Vec::new();
    let now = encode_datetime(Utc::now());
    let has_live_publication = sqlx::query_scalar::<_, bool>(
        r#"SELECT EXISTS (
               SELECT 1
               FROM "AddonMediaListPublicationJob" AS "job"
               JOIN "ManagedLibraryOperation" AS "operation"
                 ON "operation"."Id" = "job"."OperationId"
               JOIN "ManagedLibraryLease" AS "lease"
                 ON "lease"."OperationId" = "operation"."Id"
                AND "lease"."OwnerToken" = "operation"."OwnerToken"
               WHERE "job"."AddonMediaListId" = ?
                 AND "operation"."State" = ?
                 AND "lease"."LeaseExpiresAt" > ?
           )"#,
    )
    .bind(list_id)
    .bind(crate::repositories::common::encode_enum(
        ersatzrs_domain::ManagedLibraryOperationState::Running,
    ))
    .bind(&now)
    .fetch_one(pool)
    .await
    .repository()?;
    if has_live_publication {
        blockers.push(publication_blocker(
            AddonMediaListPublicationBlockerKind::PublicationInProgress,
            None,
            None,
            1,
        ));
    }
    let active_journal = sqlx::query_scalar::<_, Option<String>>(
        r#"SELECT "PublicationJournalJson" FROM "AddonMediaList" WHERE "Id" = ?"#,
    )
    .bind(list_id)
    .fetch_one(pool)
    .await
    .repository()?;
    if !has_live_publication
        && active_journal
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
    {
        blockers.push(publication_blocker(
            AddonMediaListPublicationBlockerKind::RecoveryRequired,
            None,
            None,
            1,
        ));
    }
    Ok(blockers)
}

async fn list_publication_blockers(
    pool: &Pool<crate::db::Db>,
    form: &AddonMediaListAdminForm,
    presentation: AddonMediaListPresentationKind,
    included: &[&AddonMediaListItemAdminRow],
) -> Result<Vec<AddonMediaListPublicationBlocker>> {
    let list_id = form.id;
    let mut blockers = publication_operation_blockers(pool, list_id).await?;
    if form
        .import_progress
        .as_ref()
        .is_some_and(|progress| progress.state != ersatzrs_domain::AddonMediaListImportState::Ready)
    {
        blockers.push(publication_blocker(
            AddonMediaListPublicationBlockerKind::OverviewIncomplete,
            None,
            None,
            1,
        ));
    }

    let list_metadata = load_list_metadata_form(pool, list_id).await?.metadata;
    if list_metadata.title.trim().is_empty() {
        blockers.push(publication_blocker(
            AddonMediaListPublicationBlockerKind::CollectionTitleMissing,
            None,
            None,
            1,
        ));
    }
    if invalid_artwork_message("collection", &list_metadata).is_some() {
        blockers.push(publication_blocker(
            AddonMediaListPublicationBlockerKind::InvalidArtwork,
            None,
            Some("collection".to_owned()),
            1,
        ));
    }
    if included.is_empty() {
        blockers.push(publication_blocker(
            AddonMediaListPublicationBlockerKind::NoIncludedItems,
            None,
            None,
            0,
        ));
    }

    if presentation == AddonMediaListPresentationKind::Movies {
        let primary_count = included
            .iter()
            .filter(|item| {
                item.parent_provider_id.is_none()
                    && item.collection_role == AddonMediaListItemRole::Primary
            })
            .count();
        if primary_count != 1 {
            blockers.push(publication_blocker(
                AddonMediaListPublicationBlockerKind::MoviePrimaryCount,
                None,
                None,
                u64::try_from(primary_count).unwrap_or(u64::MAX),
            ));
        }
    }
    Ok(blockers)
}

#[derive(Default)]
struct PublicationItemClassification {
    preparation_required: std::collections::BTreeSet<EntityId>,
    blockers: Vec<AddonMediaListPublicationBlocker>,
    recommendation_count: u64,
    required_playback_detail_count: u64,
    required_playback_detail_pending: u64,
    restricted_excluded_count: u64,
}

fn access_restricted_without_replacement(item: &AddonMediaListItemAdminRow) -> bool {
    item.availability == AddonMediaListItemAvailability::Unavailable
        && matches!(
            item.availability_reason_code,
            Some(
                ersatzrs_domain::AddonMediaListAvailabilityReason::AuthenticationRequired
                    | ersatzrs_domain::AddonMediaListAvailabilityReason::ContentRestricted
            )
        )
        && !item.replacement_valid
}

fn classify_item_advisory_and_playback(
    presentation: AddonMediaListPresentationKind,
    item: &AddonMediaListItemAdminRow,
    detail_state: Option<AddonMediaListDetailState>,
    classification: &mut PublicationItemClassification,
) {
    let advisory = item.metadata_conflict_count > 0
        || item.liveness_conflict
        || item.availability == AddonMediaListItemAvailability::Unknown
        || (item.availability == AddonMediaListItemAvailability::Unavailable
            && item.replacement_valid)
        || (item.replacement_media_version_id.is_some() && !item.replacement_valid)
        || (presentation == AddonMediaListPresentationKind::Movies
            && item.collection_role == AddonMediaListItemRole::Auto);
    if advisory {
        classification.recommendation_count = classification.recommendation_count.saturating_add(1);
    }
    if item.kind == AddonMediaListItemKind::RemoteStream && !item.replacement_valid {
        match item.effective_liveness {
            RemoteStreamLiveness::Unknown => {
                classification.blockers.push(publication_item_blocker(
                    AddonMediaListPublicationBlockerKind::PlaybackDecisionRequired,
                    item,
                ));
            }
            RemoteStreamLiveness::Finite
                if item.duration_seconds.is_none_or(|duration| duration == 0)
                    && !classification.preparation_required.contains(&item.id) =>
            {
                classify_required_playback_detail(item, detail_state, classification);
            }
            RemoteStreamLiveness::Finite | RemoteStreamLiveness::Live => {}
        }
    }
    if item.availability == AddonMediaListItemAvailability::Unavailable
        && !item.replacement_valid
        && !matches!(
            item.availability_reason_code,
            Some(
                ersatzrs_domain::AddonMediaListAvailabilityReason::AuthenticationRequired
                    | ersatzrs_domain::AddonMediaListAvailabilityReason::ContentRestricted
            )
        )
    {
        classification.blockers.push(publication_item_blocker(
            AddonMediaListPublicationBlockerKind::UnavailableWithoutReplacement,
            item,
        ));
    }
}

fn classify_required_playback_detail(
    item: &AddonMediaListItemAdminRow,
    detail_state: Option<AddonMediaListDetailState>,
    classification: &mut PublicationItemClassification,
) {
    match detail_state {
        None => classification.blockers.push(publication_item_blocker(
            AddonMediaListPublicationBlockerKind::PlaybackDetailsUnavailable,
            item,
        )),
        Some(AddonMediaListDetailState::Complete | AddonMediaListDetailState::Skipped) => {
            classification.blockers.push(publication_item_blocker(
                AddonMediaListPublicationBlockerKind::PlaybackDetailsFailed,
                item,
            ));
        }
        Some(state) => {
            classification.required_playback_detail_count = classification
                .required_playback_detail_count
                .saturating_add(1);
            if matches!(
                state,
                AddonMediaListDetailState::Queued
                    | AddonMediaListDetailState::WaitingForSlot
                    | AddonMediaListDetailState::Running
                    | AddonMediaListDetailState::WaitingToRetry
            ) {
                classification.required_playback_detail_pending = classification
                    .required_playback_detail_pending
                    .saturating_add(1);
            }
        }
    }
}

async fn classify_item_metadata(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    presentation: AddonMediaListPresentationKind,
    item: &AddonMediaListItemAdminRow,
    classification: &mut PublicationItemClassification,
) -> Result<()> {
    let metadata = load_item_metadata_form(pool, list_id, item.id)
        .await?
        .metadata;
    if metadata.title.trim().is_empty() {
        classification.blockers.push(publication_item_blocker(
            AddonMediaListPublicationBlockerKind::ItemTitleMissing,
            item,
        ));
    }
    if invalid_artwork_message(&item.display_title, &metadata).is_some() {
        classification.blockers.push(publication_item_blocker(
            AddonMediaListPublicationBlockerKind::InvalidArtwork,
            item,
        ));
    }
    if presentation == AddonMediaListPresentationKind::TelevisionShow {
        match (metadata.season, metadata.episode) {
            (None, None) | (Some(0), Some(0)) => {
                classification.recommendation_count =
                    classification.recommendation_count.saturating_add(1);
            }
            (Some(season), Some(episode)) if season >= 0 && episode >= 1 => {}
            _ => classification.blockers.push(publication_item_blocker(
                AddonMediaListPublicationBlockerKind::SeasonEpisodeMissing,
                item,
            )),
        }
    }
    if matches!(
        presentation,
        AddonMediaListPresentationKind::MusicVideos | AddonMediaListPresentationKind::Songs
    ) && metadata
        .artists
        .iter()
        .all(|artist| artist.trim().is_empty())
    {
        classification.blockers.push(publication_item_blocker(
            AddonMediaListPublicationBlockerKind::ArtistMissing,
            item,
        ));
    }
    Ok(())
}

fn publication_blocker(
    kind: AddonMediaListPublicationBlockerKind,
    item_rank: Option<i32>,
    item_title: Option<String>,
    count: u64,
) -> AddonMediaListPublicationBlocker {
    AddonMediaListPublicationBlocker {
        kind,
        item_rank,
        item_title,
        count,
    }
}

fn publication_item_blocker(
    kind: AddonMediaListPublicationBlockerKind,
    item: &AddonMediaListItemAdminRow,
) -> AddonMediaListPublicationBlocker {
    publication_blocker(
        kind,
        Some(item.rank.saturating_add(1)),
        Some(item.display_title.clone()),
        1,
    )
}

async fn required_playback_detail_states(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
) -> Result<std::collections::BTreeMap<EntityId, Option<AddonMediaListDetailState>>> {
    let rows = sqlx::query(
        r#"SELECT "item"."Id", "detail"."State" AS "DetailState"
           FROM "AddonMediaListItem" AS "item"
           LEFT JOIN "AddonMediaListDetailJob" AS "detail"
             ON "detail"."AddonMediaListItemId" = "item"."Id"
           WHERE "item"."AddonMediaListId" = ? AND "item"."Included" = 1
             AND "item"."Kind" = ? AND "item"."EffectiveLiveness" = ?
             AND ("item"."DurationSeconds" IS NULL OR "item"."DurationSeconds" <= 0)"#,
    )
    .bind(list_id)
    .bind(crate::repositories::common::encode_enum(
        AddonMediaListItemKind::RemoteStream,
    ))
    .bind(crate::repositories::common::encode_enum(
        RemoteStreamLiveness::Finite,
    ))
    .fetch_all(pool)
    .await
    .repository()?;
    rows.iter()
        .map(|row| {
            Ok((
                row.try_get("Id").repository()?,
                decode_optional_enum_column::<AddonMediaListDetailState>(
                    "AddonMediaListDetailState",
                    row,
                    "DetailState",
                )?,
            ))
        })
        .collect()
}

fn publication_blocker_message(blocker: &AddonMediaListPublicationBlocker) -> String {
    let title = blocker.item_title.as_deref().unwrap_or("item");
    match blocker.kind {
        AddonMediaListPublicationBlockerKind::OverviewIncomplete => {
            "the complete add-on overview must be verified before publication".to_owned()
        }
        AddonMediaListPublicationBlockerKind::PublicationInProgress => {
            "the add-on media list is being published; wait for publication to finish".to_owned()
        }
        AddonMediaListPublicationBlockerKind::RecoveryRequired => {
            "a previous publication is still awaiting recovery".to_owned()
        }
        AddonMediaListPublicationBlockerKind::CollectionTitleMissing => {
            "collection title is required before publication".to_owned()
        }
        AddonMediaListPublicationBlockerKind::InvalidArtwork => {
            format!("{title} contains an invalid artwork URL; use a bounded HTTPS URL")
        }
        AddonMediaListPublicationBlockerKind::NoIncludedItems => {
            "at least one item must be included before publication".to_owned()
        }
        AddonMediaListPublicationBlockerKind::MoviePrimaryCount => format!(
            "movie publication requires exactly one included main movie; found {}",
            blocker.count
        ),
        AddonMediaListPublicationBlockerKind::PlaybackDecisionRequired => {
            format!("{title} requires a finite or live playback decision")
        }
        AddonMediaListPublicationBlockerKind::PlaybackDetailsUnavailable => format!(
            "{title} is finite media but its add-on does not provide item-detail enrichment"
        ),
        AddonMediaListPublicationBlockerKind::PlaybackDetailsFailed => {
            format!("provider details for {title} do not include the required positive duration")
        }
        AddonMediaListPublicationBlockerKind::UnavailableWithoutReplacement => {
            format!("{title} is unavailable and has no valid local replacement")
        }
        AddonMediaListPublicationBlockerKind::ReplacementUnresolved => {
            format!("{title} has an unresolved saved replacement; select or remove the replacement")
        }
        AddonMediaListPublicationBlockerKind::ItemTitleMissing => format!(
            "item {} requires a title before publication",
            blocker.item_rank.unwrap_or_default()
        ),
        AddonMediaListPublicationBlockerKind::SeasonEpisodeMissing => {
            format!("{title} requires a valid season and episode number")
        }
        AddonMediaListPublicationBlockerKind::ArtistMissing => {
            format!("{title} requires at least one artist")
        }
    }
}

fn invalid_artwork_message(owner: &str, metadata: &AddonMediaListNfoMetadata) -> Option<String> {
    metadata
        .artwork
        .iter()
        .map(|candidate| candidate.url.as_str())
        .chain(metadata.artwork_urls.iter().map(String::as_str))
        .find(|url| {
            crate::addon_artwork::upload_id(url).is_none() && !is_safe_remote_artwork_url(url)
        })
        .map(|_| format!("{owner} contains an invalid artwork URL; use a bounded HTTPS URL"))
}

fn is_safe_remote_artwork_url(value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() || value.len() > 2_048 {
        return false;
    }
    url::Url::parse(value).is_ok_and(|url| {
        url.scheme() == "https"
            && url.has_host()
            && url.username().is_empty()
            && url.password().is_none()
    })
}

async fn record_publication_error(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    message: &str,
) -> Result<()> {
    sqlx::query(r#"UPDATE "AddonMediaList" SET "ReviewError" = ? WHERE "Id" = ?"#)
        .bind(message)
        .bind(list_id)
        .execute(pool)
        .await
        .repository()?;
    Ok(())
}

fn metadata_override_json(
    provider: &serde_json::Value,
    metadata: &AddonMediaListNfoMetadata,
) -> Result<String> {
    let submitted = serde_json::to_value(metadata).map_err(|error| RepositoryError::Decode {
        kind: "add-on metadata",
        value: error.to_string(),
    })?;
    let mut overrides = serde_json::Map::new();
    if let Some(fields) = submitted.as_object() {
        for (field, value) in fields {
            if provider.get(field) != Some(value) {
                overrides.insert(field.clone(), value.clone());
            }
        }
    }
    serde_json::to_string(&overrides).map_err(|error| RepositoryError::Decode {
        kind: "add-on metadata override",
        value: error.to_string(),
    })
}

async fn save_list_metadata_override(
    pool: &Pool<crate::db::Db>,
    id: EntityId,
    metadata: &AddonMediaListNfoMetadata,
    expected_etag: &str,
) -> Result<()> {
    crate::addon_media_list_sync::ensure_not_publishing(pool, id).await?;
    let current = load_list_metadata_form(pool, id).await?;
    if current.etag != expected_etag {
        return Err(RepositoryError::Conflict(
            "metadata changed since the editor was loaded".to_owned(),
        ));
    }
    let provider_json: String =
        sqlx::query_scalar(r#"SELECT "ProviderMetadataJson" FROM "AddonMediaList" WHERE "Id" = ?"#)
            .bind(id)
            .fetch_one(pool)
            .await
            .repository()?;
    let overrides = metadata_override_json(&metadata_json_value(&provider_json)?, metadata)?;
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    let updated = sqlx::query(
        r#"UPDATE "AddonMediaList"
           SET "MetadataOverrideJson" = ?, "MetadataConflictJson" = NULL
           WHERE "Id" = ?"#,
    )
    .bind(overrides)
    .bind(id)
    .execute(&mut *tx)
    .await
    .repository()?;
    if updated.rows_affected() != 1 {
        return Err(publication_in_progress());
    }
    mark_list_ready_tx(&mut tx, id).await?;
    tx.commit().await.repository()?;
    Ok(())
}

async fn save_item_metadata_override(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    item_id: EntityId,
    metadata: &AddonMediaListNfoMetadata,
    expected_etag: &str,
) -> Result<()> {
    let current = load_item_metadata_form(pool, list_id, item_id).await?;
    if current.etag != expected_etag {
        return Err(RepositoryError::Conflict(
            "metadata changed since the editor was loaded".to_owned(),
        ));
    }
    let provider_json: String = sqlx::query_scalar(
        r#"SELECT "ProviderMetadataJson" FROM "AddonMediaListItem"
           WHERE "Id" = ? AND "AddonMediaListId" = ?"#,
    )
    .bind(item_id)
    .bind(list_id)
    .fetch_one(pool)
    .await
    .repository()?;
    let overrides = metadata_override_json(&metadata_json_value(&provider_json)?, metadata)?;
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    require_draft_item_tx(&mut tx, list_id, item_id).await?;
    sqlx::query(
        r#"UPDATE "AddonMediaListItem"
           SET "MetadataOverrideJson" = ?, "MetadataConflictJson" = NULL
           WHERE "Id" = ? AND "AddonMediaListId" = ?"#,
    )
    .bind(overrides)
    .bind(item_id)
    .bind(list_id)
    .execute(&mut *tx)
    .await
    .repository()?;
    mark_list_ready_tx(&mut tx, list_id).await?;
    tx.commit().await.repository()?;
    Ok(())
}

/// Path of the list-level NFO document for a presentation with a parent.
///
/// Kodi names the parent document after the parent kind, so a television list
/// owns `tvshow.nfo` and a music-video list owns `artist.nfo`.
async fn parent_nfo_path_for(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    file_name: &str,
) -> Result<Option<PathBuf>> {
    let scanned_path = sqlx::query_scalar::<_, Option<String>>(
        r#"SELECT "file"."Path"
           FROM "AddonMediaListItem" AS "item"
           JOIN "MediaVersion" AS "version"
             ON "version"."RemoteStreamId" = "item"."MediaItemId"
           JOIN "MediaFile" AS "file" ON "file"."MediaVersionId" = "version"."Id"
           WHERE "item"."AddonMediaListId" = ?
             AND "item"."ParentProviderId" IS NULL
           ORDER BY "item"."Rank", "item"."Id", "version"."Id", "file"."Id"
           LIMIT 1"#,
    )
    .bind(list_id)
    .fetch_optional(pool)
    .await
    .repository()?
    .flatten();
    if let Some(path) = scanned_path {
        return Ok(Path::new(&path)
            .parent()
            .map(|parent| parent.join(file_name)));
    }
    manifest_parent_nfo_path(pool, list_id, file_name).await
}

async fn parent_nfo_path_for_source(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    file_name: &str,
    source: Option<&AddonMediaListRevisionSnapshot>,
) -> Result<Option<PathBuf>> {
    let Some(source) = source else {
        return parent_nfo_path_for(pool, list_id, file_name).await;
    };
    let mut parents = source
        .items
        .iter()
        .filter(|item| item.published && item.parent_provider_id.is_none())
        .collect::<Vec<_>>();
    parents.sort_by_key(|item| item.rank);
    for item in parents {
        let path: Option<String> = sqlx::query_scalar(
            r#"SELECT "file"."Path" FROM "AddonMediaListItem" AS "item"
               JOIN "MediaVersion" AS "version" ON "version"."RemoteStreamId"="item"."MediaItemId"
               JOIN "MediaFile" AS "file" ON "file"."MediaVersionId"="version"."Id"
               WHERE "item"."AddonMediaListId"=? AND "item"."ProviderId"=?
               ORDER BY "version"."Id", "file"."Id" LIMIT 1"#,
        )
        .bind(list_id)
        .bind(&item.provider_id)
        .fetch_optional(pool)
        .await
        .repository()?
        .flatten();
        if let Some(path) = path {
            return Ok(Path::new(&path)
                .parent()
                .map(|folder| folder.join(file_name)));
        }
    }
    manifest_parent_nfo_path(pool, list_id, file_name).await
}

async fn manifest_parent_nfo_path(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    file_name: &str,
) -> Result<Option<PathBuf>> {
    // A newly imported managed list has a manifest folder before its first
    // library scan has created MediaVersion/MediaFile rows. The review and
    // auto-presentation paths still need a parent NFO in that folder.
    let manifest_path = sqlx::query_scalar::<_, String>(
        r#"SELECT "ManifestPath" FROM "AddonMediaListLibrary"
           WHERE "AddonMediaListId" = ?"#,
    )
    .bind(list_id)
    .fetch_optional(pool)
    .await
    .repository()?;
    Ok(manifest_path.and_then(|path| {
        Path::new(&path)
            .parent()
            .map(|parent| parent.join(file_name))
    }))
}

/// The list-level NFO document and root a presentation writes, if it has one.
const fn parent_nfo_for(
    presentation: AddonMediaListPresentationKind,
) -> Option<(&'static str, &'static str)> {
    match presentation {
        AddonMediaListPresentationKind::TelevisionShow => Some(("tvshow.nfo", "tvshow")),
        AddonMediaListPresentationKind::MusicVideos => Some(("artist.nfo", "artist")),
        _ => None,
    }
}

async fn item_nfo_path_for(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    item_id: EntityId,
) -> Result<Option<PathBuf>> {
    let path = sqlx::query_scalar::<_, Option<String>>(
        r#"SELECT "file"."Path"
           FROM "AddonMediaListItem" AS "item"
           LEFT JOIN "MediaVersion" AS "version"
             ON "version"."RemoteStreamId" = "item"."MediaItemId"
           LEFT JOIN "MediaFile" AS "file" ON "file"."MediaVersionId" = "version"."Id"
           WHERE "item"."AddonMediaListId" = ? AND "item"."Id" = ?
           ORDER BY "version"."Id", "file"."Id" LIMIT 1"#,
    )
    .bind(list_id)
    .bind(item_id)
    .fetch_optional(pool)
    .await
    .repository()?
    .flatten();
    Ok(crate::addon_media_nfo::item_nfo_path(path.as_deref()))
}

struct ProjectedEpisodeInput {
    item_id: EntityId,
    metadata: AddonMediaListNfoMetadata,
    nfo_path: PathBuf,
    state: i32,
}

/// Related metadata tables supported by one projected metadata record.
///
/// The shared tables use sparse foreign keys. Keep the real column and the two
/// non-uniform credit capabilities here rather than assuming each kind has the
/// same schema: `Director` does not support songs or images, `Writer` also does
/// not support music videos, and `MusicVideoArtist` is music-video-only.
#[derive(Clone, Copy)]
struct ProjectedMetadataRelations {
    fk_column: &'static str,
    directors: bool,
    writers: bool,
    music_video_artists: bool,
}

impl ProjectedMetadataRelations {
    const ARTIST: Self = Self {
        fk_column: "ArtistMetadataId",
        directors: false,
        writers: false,
        music_video_artists: false,
    };
    const SHOW: Self = Self {
        fk_column: "ShowMetadataId",
        directors: false,
        writers: false,
        music_video_artists: false,
    };
    const EPISODE: Self = Self {
        fk_column: "EpisodeMetadataId",
        directors: true,
        writers: true,
        music_video_artists: false,
    };
    const MOVIE: Self = Self {
        fk_column: "MovieMetadataId",
        directors: true,
        writers: true,
        music_video_artists: false,
    };
    const MUSIC_VIDEO: Self = Self {
        fk_column: "MusicVideoMetadataId",
        directors: true,
        writers: false,
        music_video_artists: true,
    };
    const OTHER_VIDEO: Self = Self {
        fk_column: "OtherVideoMetadataId",
        directors: true,
        writers: true,
        music_video_artists: false,
    };
    const SONG: Self = Self {
        fk_column: "SongMetadataId",
        directors: false,
        writers: false,
        music_video_artists: false,
    };
    const IMAGE: Self = Self {
        fk_column: "ImageMetadataId",
        directors: false,
        writers: false,
        music_video_artists: false,
    };
}

/// Replace every related row represented by a managed add-on-list NFO.
///
/// Projected metadata rows are recreated on refresh, but these shared tables
/// deserve the same explicit replacement semantics: it keeps retries safe and
/// means a future metadata upsert cannot leave duplicate facets behind. Remote
/// Stream metadata remains separately scanner-owned; this writes the distinct
/// metadata row of the requested library presentation.
async fn replace_projected_metadata_relations(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    relations: ProjectedMetadataRelations,
    metadata_id: EntityId,
    metadata: &AddonMediaListNfoMetadata,
    nfo_path: &Path,
) -> Result<()> {
    let fk_column = relations.fk_column;
    replace_strings(tx, "Genre", fk_column, metadata_id, &metadata.genres).await?;
    replace_strings(tx, "Tag", fk_column, metadata_id, &metadata.tags).await?;
    replace_strings(tx, "Studio", fk_column, metadata_id, &metadata.studios).await?;
    replace_strings(
        tx,
        "MusicLabel",
        fk_column,
        metadata_id,
        &metadata.music_labels,
    )
    .await?;
    replace_metadata_languages(tx, fk_column, metadata_id, &metadata.languages).await?;

    let actors = metadata
        .people
        .iter()
        .filter(|person| !person.name.trim().is_empty())
        .map(|person| PersonRef::with_role(person.name.clone(), person.role.clone(), person.order))
        .collect::<Vec<_>>();
    replace_people(tx, "Actor", fk_column, metadata_id, &actors).await?;

    if relations.directors {
        let directors = metadata
            .directors
            .iter()
            .cloned()
            .map(PersonRef::new)
            .collect::<Vec<_>>();
        replace_people(tx, "Director", fk_column, metadata_id, &directors).await?;
    }
    if relations.writers {
        let writers = metadata
            .writers
            .iter()
            .cloned()
            .map(PersonRef::new)
            .collect::<Vec<_>>();
        replace_people(tx, "Writer", fk_column, metadata_id, &writers).await?;
    }
    if relations.music_video_artists {
        replace_strings(
            tx,
            "MusicVideoArtist",
            fk_column,
            metadata_id,
            &metadata.artists,
        )
        .await?;
    }
    replace_guids(tx, fk_column, metadata_id, &metadata.guids).await?;

    // Slice 1 materializes the first provider image at this scanner-known
    // basename. Until it is available, keep the NFO URL so a later scan has a
    // source rather than silently dropping provider artwork.
    let artwork_path = ["jpg", "jpeg", "png", "webp"]
        .iter()
        .map(|extension| nfo_path.with_extension(extension))
        .find(|path| path.is_file())
        .map(|path| path.display().to_string());
    let mut artwork = metadata
        .artwork
        .iter()
        .map(|candidate| ArtworkRef {
            kind: match candidate.role {
                ersatzrs_core::repositories::AddonMediaListArtworkRole::Poster => {
                    artwork_kind::POSTER
                }
                ersatzrs_core::repositories::AddonMediaListArtworkRole::Fanart => {
                    artwork_kind::FAN_ART
                }
                ersatzrs_core::repositories::AddonMediaListArtworkRole::Thumb
                | ersatzrs_core::repositories::AddonMediaListArtworkRole::Other => {
                    artwork_kind::THUMBNAIL
                }
            },
            path: projected_artwork_path(nfo_path, &candidate.url),
            date_added: chrono::Utc::now(),
        })
        .collect::<Vec<_>>();
    append_missing_thumbnail(
        &mut artwork,
        artwork_path.or_else(|| metadata.artwork_urls.first().cloned()),
    );
    replace_artwork(
        tx,
        fk_column,
        metadata_id,
        &[
            artwork_kind::POSTER,
            artwork_kind::THUMBNAIL,
            artwork_kind::FAN_ART,
        ],
        &artwork,
    )
    .await
}

fn append_missing_thumbnail(artwork: &mut Vec<ArtworkRef>, path: Option<String>) {
    let Some(path) = path else {
        return;
    };
    let fallback_identity = artwork_identity(artwork_kind::THUMBNAIL, &path);
    if artwork
        .iter()
        .any(|item| artwork_identity(item.kind, &item.path) == fallback_identity)
    {
        return;
    }
    artwork.push(ArtworkRef {
        kind: artwork_kind::THUMBNAIL,
        path,
        date_added: chrono::Utc::now(),
    });
}

/// Convert a publication-local NFO artwork filename into the absolute path
/// consumed by the shared metadata tables. Provider HTTPS URLs are retained
/// for operator-owned NFOs whose images have not been materialized by us.
fn projected_artwork_path(nfo_path: &Path, value: &str) -> String {
    let relative = Path::new(value);
    let is_plain_file_name = relative
        .components()
        .all(|component| matches!(component, std::path::Component::Normal(_)))
        && relative.components().count() == 1;
    if is_plain_file_name {
        if let Some(parent) = nfo_path.parent() {
            let candidate = parent.join(relative);
            if candidate.is_file() {
                return candidate.display().to_string();
            }
        }
    }
    value.to_owned()
}

/// Season title in the spelling scanned television sources already use.
fn season_title(season_number: i32) -> String {
    if season_number == 0 {
        "Specials".to_owned()
    } else {
        format!("Season {season_number}")
    }
}

impl SqliteAddonMediaListRepository {
    pub(crate) async fn prepare_automatic_publication_details(&self, id: EntityId) -> Result<u64> {
        self.prepare_publication_details_impl(id, true).await
    }

    #[allow(clippy::too_many_lines)] // Manual and automatic admission share one atomic detail update.
    async fn prepare_publication_details_impl(
        &self,
        id: EntityId,
        preserve_retry: bool,
    ) -> Result<u64> {
        let form = self
            .get_admin_form(id)
            .await?
            .ok_or(RepositoryError::NotFound {
                kind: "add-on media list",
                id,
            })?;
        if form.review_state == AddonMediaListReviewState::Publishing {
            return Err(publication_in_progress());
        }
        let rows = sqlx::query(
            r#"SELECT "item"."Id", "item"."DisplayTitle",
                      "detail"."State" AS "DetailState", "detail"."ResultRecordJson"
               FROM "AddonMediaListItem" AS "item"
               LEFT JOIN "AddonMediaListDetailJob" AS "detail"
                 ON "detail"."AddonMediaListItemId" = "item"."Id"
               WHERE "item"."AddonMediaListId" = ? AND "item"."Included" = 1
                 AND "item"."InDraft" = 1
                 AND NOT EXISTS (SELECT 1 FROM "AddonMediaListItemTombstone" AS "tombstone"
                     WHERE "tombstone"."AddonMediaListItemId"="item"."Id" AND "tombstone"."ConfirmedAt" IS NOT NULL)
                 AND ("item"."AvailabilityReasonCode" IS NULL
                      OR "item"."AvailabilityReasonCode" NOT IN (?, ?))
                 AND "item"."Kind" = ? AND COALESCE("item"."LivenessOverride", "item"."ProviderLiveness") = ?
                 AND ("item"."DurationSeconds" IS NULL OR "item"."DurationSeconds" <= 0)
               ORDER BY "item"."Rank", "item"."Id""#,
        )
        .bind(id)
        .bind(crate::repositories::common::encode_enum(
            ersatzrs_domain::AddonMediaListAvailabilityReason::AuthenticationRequired,
        ))
        .bind(crate::repositories::common::encode_enum(
            ersatzrs_domain::AddonMediaListAvailabilityReason::ContentRestricted,
        ))
        .bind(crate::repositories::common::encode_enum(
            AddonMediaListItemKind::RemoteStream,
        ))
        .bind(crate::repositories::common::encode_enum(
            RemoteStreamLiveness::Finite,
        ))
        .fetch_all(&self.0)
        .await
        .repository()?;
        let mut required_media =
            crate::addon_media_list_prepared_selection::interval_metadata_requirements(
                &self.0,
                id,
                form.draft_revision,
                &[],
            )
            .await?
            .into_iter()
            .map(|item| item.item_id)
            .collect::<std::collections::BTreeSet<_>>();
        required_media.extend(
            crate::addon_media_list_prepared_selection::preparation_requirements(
                &self.0,
                id,
                form.draft_revision,
                &[],
            )
            .await?
            .into_iter()
            .map(|item| item.item_id),
        );
        // Interval metadata or prepared files supply the positive duration, without an
        // additional provider detail request for the same finite item.
        let mut detail_rows = Vec::new();
        let valid_replacements = self
            .list_items(id)
            .await?
            .into_iter()
            .filter(|item| item.replacement_valid)
            .map(|item| item.id)
            .collect::<std::collections::BTreeSet<_>>();
        for row in rows {
            let item_id: i64 = row.try_get("Id").repository()?;
            if !required_media.contains(&item_id) && !valid_replacements.contains(&item_id) {
                detail_rows.push(row);
            }
        }
        let rows = detail_rows;
        if rows.is_empty() {
            return Ok(0);
        }

        let has_saved_options: bool = sqlx::query_scalar(
            r#"SELECT "DraftOptionSchemaId" IS NOT NULL FROM "AddonMediaList" WHERE "Id"=?"#,
        )
        .bind(id)
        .fetch_one(&self.0)
        .await
        .repository()?;
        let coordinator =
            crate::addon_media_list_import::AddonMediaListImportCoordinator::new(self.0.clone());
        for row in &rows {
            let title: String = row.try_get("DisplayTitle").repository()?;
            let mut state = decode_optional_enum_column::<AddonMediaListDetailState>(
                "AddonMediaListDetailState",
                row,
                "DetailState",
            )?;
            if has_saved_options && state.is_some() {
                let item_id: EntityId = row.try_get("Id").repository()?;
                coordinator
                    .request_prepublication_details(id, item_id)
                    .await?;
                let refreshed = sqlx::query(
                    r#"SELECT "State" AS "DetailState" FROM "AddonMediaListDetailJob"
                       WHERE "AddonMediaListId"=? AND "AddonMediaListItemId"=?"#,
                )
                .bind(id)
                .bind(item_id)
                .fetch_one(&self.0)
                .await
                .repository()?;
                state = decode_optional_enum_column(
                    "AddonMediaListDetailState",
                    &refreshed,
                    "DetailState",
                )?;
            }
            let Some(state) = state else {
                return Err(RepositoryError::Conflict(format!(
                    "{title} is finite media but its add-on does not provide item-detail enrichment"
                )));
            };
            if matches!(
                state,
                AddonMediaListDetailState::Complete | AddonMediaListDetailState::Skipped
            ) {
                return Err(RepositoryError::Conflict(format!(
                    "provider details for {title} do not include the required positive duration"
                )));
            }
        }

        let now = crate::repositories::common::encode_datetime(chrono::Utc::now());
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        for row in &rows {
            let item_id: EntityId = row.try_get("Id").repository()?;
            let updated = sqlx::query(
                r#"UPDATE "AddonMediaListDetailJob"
                   SET "State" = CASE
                           WHEN ? AND "State" IN (?, ?) THEN "State"
                           WHEN "State" IN (?, ?, ?) THEN "State"
                           WHEN "ResultRecordJson" IS NOT NULL THEN ?
                           ELSE ?
                       END,
                       "ManualPriority" = 1,
                       "RequestedAt" = COALESCE("RequestedAt", ?),
                       "NextEligibleAt" = CASE WHEN ? THEN "NextEligibleAt" ELSE NULL END,
                       "Diagnostic" = CASE WHEN ? THEN "Diagnostic" ELSE NULL END, "UpdatedAt" = ?
                   WHERE "AddonMediaListId" = ? AND "AddonMediaListItemId" = ?"#,
            )
            .bind(preserve_retry)
            .bind(crate::repositories::common::encode_enum(
                AddonMediaListDetailState::WaitingToRetry,
            ))
            .bind(crate::repositories::common::encode_enum(
                AddonMediaListDetailState::Failed,
            ))
            .bind(crate::repositories::common::encode_enum(
                AddonMediaListDetailState::Running,
            ))
            .bind(crate::repositories::common::encode_enum(
                AddonMediaListDetailState::Complete,
            ))
            .bind(crate::repositories::common::encode_enum(
                AddonMediaListDetailState::Skipped,
            ))
            .bind(crate::repositories::common::encode_enum(
                AddonMediaListDetailState::Running,
            ))
            .bind(crate::repositories::common::encode_enum(
                AddonMediaListDetailState::Queued,
            ))
            .bind(&now)
            .bind(preserve_retry)
            .bind(preserve_retry)
            .bind(&now)
            .bind(id)
            .bind(item_id)
            .execute(&mut *tx)
            .await
            .repository()?;
            if updated.rows_affected() != 1 {
                return Err(RepositoryError::Conflict(
                    "the required item-detail work changed while publication was being prepared"
                        .to_owned(),
                ));
            }
        }
        tx.commit().await.repository()?;
        u64::try_from(rows.len()).map_err(|error| RepositoryError::Decode {
            kind: "add-on publication detail count",
            value: error.to_string(),
        })
    }

    /// Path of the list-level NFO for whichever parent the list requested.
    async fn parent_nfo_path(&self, id: EntityId) -> Result<Option<PathBuf>> {
        let requested = self
            .get_admin_form(id)
            .await?
            .map(|form| form.requested_presentation_kind)
            .unwrap_or_default();
        let (file_name, _) = parent_nfo_for(requested).unwrap_or(("tvshow.nfo", "tvshow"));
        parent_nfo_path_for(&self.0, id, file_name).await
    }

    /// Load list entries, optionally restricted to one availability group.
    ///
    /// One query shape serves the unfiltered internal callers and the paged
    /// editor sections so their row decoding cannot drift apart.
    #[allow(clippy::too_many_lines)]
    async fn query_items(
        &self,
        id: EntityId,
        availability: Option<AddonMediaListItemAvailability>,
        page: u32,
        page_size: Option<u32>,
        draft_only: bool,
    ) -> Result<Vec<AddonMediaListItemAdminRow>> {
        // One static statement serves every combination: a NULL availability
        // matches every group, and an absent page size becomes an unbounded
        // limit both backends accept.
        let rows = sqlx::query(
            r#"SELECT "item"."Id", "item"."Rank", "item"."DisplayTitle", "item"."Kind",
                      "item"."Title", "item"."Year", "item"."Season", "item"."Episode",
                      "item"."SourceUrl", "item"."DurationSeconds", "item"."ParentProviderId",
                      "item"."ChapterInput", "item"."ProviderChapterInput",
                       "item"."ContentKind", "item"."CollectionRole",
                       "item"."Included", "item"."Published",
                       "item"."ProviderMetadataJson", "item"."MetadataOverrideJson",
                       "item"."MetadataConflictJson",
                       "item"."ProviderLiveness", "item"."LivenessOverride",
                       "item"."EffectiveLiveness", "item"."LivenessConflict",
                       "item"."PublishedLiveness",
                      "detail"."State" AS "DetailState",
                      "detail"."Diagnostic" AS "DetailDiagnostic",
                      "projection"."EpisodeId" AS "ProjectedEpisodeId",
                      (SELECT "Id" FROM "RemoteStream" WHERE "Id" = "item"."MediaItemId") AS "RemoteStreamId",
                      "item"."Availability", "item"."AvailabilityReason",
                      "item"."AvailabilityReasonCode",
                      "replacement"."MediaItemId" AS "ReplacementMediaItemId",
                      "item"."SavedReplacementJson" IS NOT NULL AS "ReplacementUnresolved",
                      "replacement"."MediaVersionId" AS "ReplacementMediaVersionId"
               FROM "AddonMediaListItem" AS "item"
               LEFT JOIN "AddonMediaListItemReplacement" AS "replacement"
                 ON "replacement"."AddonMediaListItemId" = "item"."Id"
               LEFT JOIN "AddonMediaListItemEpisode" AS "projection"
                 ON "projection"."AddonMediaListItemId" = "item"."Id"
               LEFT JOIN "AddonMediaListDetailJob" AS "detail"
                 ON "detail"."AddonMediaListItemId" = "item"."Id"
               WHERE "item"."AddonMediaListId" = ?
                 AND (? = 0 OR "item"."InDraft" = 1)
                 AND (? IS NULL OR "item"."Availability" = ?)
               ORDER BY "item"."Rank", "item"."Id"
               LIMIT ? OFFSET ?"#,
        )
        .bind(id)
        .bind(draft_only)
        .bind(availability.map(crate::repositories::common::encode_enum))
        .bind(availability.map(crate::repositories::common::encode_enum))
        .bind(page_size.map_or(i64::MAX, |size| i64::from(size.max(1))))
        .bind(page_size.map_or(0, |size| i64::from(page) * i64::from(size.max(1))))
        .fetch_all(&self.0)
        .await
        .repository()?;

        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            let provider_json: String = row.try_get("ProviderMetadataJson").repository()?;
            let override_json: String = row.try_get("MetadataOverrideJson").repository()?;
            let conflict_json: Option<String> = row.try_get("MetadataConflictJson").repository()?;
            let metadata_form =
                metadata_form_from_json(&provider_json, &override_json, conflict_json.as_deref())?;
            let metadata_conflict_count =
                u32::try_from(metadata_form.conflicts.len()).unwrap_or(u32::MAX);
            let metadata = metadata_form.metadata;
            let effective_title = metadata.title.clone();
            let effective_year = metadata.year;
            let effective_season = metadata.season;
            let effective_episode = metadata.episode;
            let first_aired = metadata.release_date.clone();
            let replacement_media_item_id = row
                .try_get::<Option<EntityId>, _>("ReplacementMediaItemId")
                .repository()?;
            let replacement_media_version_id = row
                .try_get::<Option<EntityId>, _>("ReplacementMediaVersionId")
                .repository()?;
            let candidate = match replacement_media_item_id.zip(replacement_media_version_id) {
                Some((media_item_id, media_version_id)) => {
                    load_replacement_candidate(&self.0, media_item_id, media_version_id).await?
                }
                None => None,
            };
            result.push(AddonMediaListItemAdminRow {
                id: row.try_get("Id").repository()?,
                rank: row.try_get("Rank").repository()?,
                kind: crate::repositories::common::decode_enum_column(
                    "AddonMediaListItemKind",
                    &row,
                    "Kind",
                )?,
                display_title: if effective_title.trim().is_empty() {
                    row.try_get("DisplayTitle").repository()?
                } else {
                    effective_title.clone()
                },
                availability: crate::repositories::common::decode_enum_column(
                    "AddonMediaListItemAvailability",
                    &row,
                    "Availability",
                )?,
                availability_reason: row.try_get("AvailabilityReason").repository()?,
                availability_reason_code: crate::repositories::common::decode_optional_enum_column(
                    "AddonMediaListAvailabilityReason",
                    &row,
                    "AvailabilityReasonCode",
                )?,
                replacement_media_item_id,
                replacement_media_version_id,
                replacement_title: candidate.as_ref().map(|item| item.title.clone()),
                replacement_path: candidate.as_ref().map(|item| item.path.clone()),
                replacement_valid: candidate.is_some(),
                replacement_unresolved: row.try_get("ReplacementUnresolved").repository()?,
                title: effective_title,
                year: effective_year,
                season: effective_season,
                episode: effective_episode,
                first_aired,
                source_url: row.try_get("SourceUrl").repository()?,
                duration_seconds: decode_optional_u64(&row, "DurationSeconds")?,
                provider_liveness: crate::repositories::common::decode_enum_column(
                    "RemoteStreamLiveness",
                    &row,
                    "ProviderLiveness",
                )?,
                liveness_override: crate::repositories::common::decode_optional_enum_column(
                    "RemoteStreamLiveness",
                    &row,
                    "LivenessOverride",
                )?,
                effective_liveness: crate::repositories::common::decode_enum_column(
                    "RemoteStreamLiveness",
                    &row,
                    "EffectiveLiveness",
                )?,
                liveness_conflict: decode_bool_column(&row, "LivenessConflict")?,
                published_liveness: crate::repositories::common::decode_enum_column(
                    "RemoteStreamLiveness",
                    &row,
                    "PublishedLiveness",
                )?,
                parent_provider_id: row.try_get("ParentProviderId").repository()?,
                chapter_input: row
                    .try_get::<Option<String>, _>("ChapterInput")
                    .repository()?
                    .unwrap_or_default(),
                provider_chapter_input: row
                    .try_get::<Option<String>, _>("ProviderChapterInput")
                    .repository()?
                    .unwrap_or_default(),
                projected_episode_id: row.try_get("ProjectedEpisodeId").repository()?,
                remote_stream_id: row.try_get("RemoteStreamId").repository()?,
                content_kind: crate::repositories::common::decode_enum_column(
                    "AddonMediaListContentKind",
                    &row,
                    "ContentKind",
                )?,
                collection_role: crate::repositories::common::decode_enum_column(
                    "AddonMediaListItemRole",
                    &row,
                    "CollectionRole",
                )?,
                thumbnail_url: metadata
                    .artwork
                    .iter()
                    .find(|candidate| {
                        matches!(
                            candidate.role,
                            ersatzrs_core::repositories::AddonMediaListArtworkRole::Thumb
                                | ersatzrs_core::repositories::AddonMediaListArtworkRole::Poster
                        )
                    })
                    .map(|candidate| candidate.url.clone())
                    .or_else(|| metadata.artwork_urls.first().cloned()),
                tags: metadata.tags,
                people: metadata
                    .people
                    .into_iter()
                    .map(|person| person.name)
                    .collect(),
                included: decode_bool_column(&row, "Included")?,
                published: decode_bool_column(&row, "Published")?,
                metadata_conflict_count,
                detail_state: row
                    .try_get::<Option<i32>, _>("DetailState")
                    .repository()?
                    .map(|value| {
                        crate::repositories::common::decode_enum(
                            "AddonMediaListDetailState",
                            &value.to_string(),
                        )
                    })
                    .transpose()?
                    .unwrap_or(AddonMediaListDetailState::NotRequested),
                detail_diagnostic: row.try_get("DetailDiagnostic").repository()?,
            });
        }
        Ok(result)
    }
}

/// Give a freshly imported list the presentation its entries imply.
///
/// The add-ons classify each entry, so an operator who pasted a URL should not
/// have to restate what the list obviously is. A projection that needs
/// something the import cannot supply — a television list has no show title yet
/// — is recorded as the presentation error rather than failing the import: the
/// entries are already stored and playable as Remote Streams either way.
async fn adopt_derived_presentation(pool: &Pool<crate::db::Db>, id: EntityId) -> Result<()> {
    let derived = resolve_auto_presentation(pool, id).await?;
    if derived == AddonMediaListPresentationKind::RemoteStreams {
        return Ok(());
    }
    ensure_generated_parent_nfo(pool, id, derived, None, None).await?;
    sqlx::query(r#"UPDATE "AddonMediaList" SET "RequestedPresentationKind" = ? WHERE "Id" = ?"#)
        .bind(crate::repositories::common::encode_enum(
            AddonMediaListPresentationKind::Auto,
        ))
        .bind(id)
        .execute(pool)
        .await
        .repository()?;
    if let Err(error) =
        reconcile_projection(pool, id, AddonMediaListPresentationKind::Auto, None).await
    {
        let _ =
            sqlx::query(r#"UPDATE "AddonMediaList" SET "PresentationError" = ? WHERE "Id" = ?"#)
                .bind(error.to_string())
                .bind(id)
                .execute(pool)
                .await;
        return Ok(());
    }
    AddonMediaListSynchronizer::new(pool.clone())
        .match_list_items(id)
        .await
}

/// Materialize parent metadata from the provider list header before applying a
/// parented library presentation. Managed add-on imports must never require an
/// operator to create an empty `tvshow.nfo` or `artist.nfo` by hand.
async fn ensure_generated_parent_nfo(
    pool: &Pool<crate::db::Db>,
    id: EntityId,
    presentation: AddonMediaListPresentationKind,
    title_override: Option<&str>,
    source: Option<&AddonMediaListRevisionSnapshot>,
) -> Result<()> {
    let Some((file_name, root)) = parent_nfo_for(presentation) else {
        return Ok(());
    };
    let path = parent_nfo_path_for_source(pool, id, file_name, source)
        .await?
        .ok_or_else(|| {
            RepositoryError::Conflict("managed list storage is unavailable".to_owned())
        })?;
    let mut metadata =
        generated_parent_metadata(pool, id, file_name, title_override, source).await?;
    materialize_parent_artwork(pool, id, &path, &mut metadata).await?;
    let generated = crate::addon_media_nfo::serialize(root, &metadata).map_err(map_nfo_error)?;
    crate::addon_media_nfo::reconcile_generated(&path, &generated).map_err(map_nfo_error)?;
    Ok(())
}

async fn ensure_parent_nfo_path_preserves_metadata(
    pool: &Pool<crate::db::Db>,
    id: EntityId,
    presentation: AddonMediaListPresentationKind,
    source: Option<&AddonMediaListRevisionSnapshot>,
) -> Result<()> {
    let Some((file_name, root)) = parent_nfo_for(presentation) else {
        return Ok(());
    };
    let selected = parent_nfo_path_for_source(pool, id, file_name, source).await?;
    let manifest: Option<String> = sqlx::query_scalar(
        r#"SELECT "ManifestPath" FROM "AddonMediaListLibrary" WHERE "AddonMediaListId"=?"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .repository()?;
    let fallback = manifest
        .as_deref()
        .and_then(|path| Path::new(path).parent())
        .map(|folder| folder.join(file_name));
    let (Some(fallback), Some(selected)) = (fallback, selected) else {
        return Ok(());
    };
    if fallback == selected {
        return Ok(());
    }
    // Existing directories may have different lexical spellings (for example
    // a manifest path containing `..`) without changing metadata authority.
    if let (Some(fallback_parent), Some(selected_parent)) = (fallback.parent(), selected.parent()) {
        if let (Ok(fallback_parent), Ok(selected_parent)) = (
            std::fs::canonicalize(fallback_parent),
            std::fs::canonicalize(selected_parent),
        ) {
            if fallback_parent == selected_parent {
                return Ok(());
            }
        }
    }
    // Compare on every commit, including retries after bindings were persisted.
    // Do not download or materialize artwork merely to prove an untracked file
    // generated: uncertain materialized artwork is conservatively preserved.
    let metadata = generated_parent_metadata(pool, id, file_name, None, source).await?;
    let generated = crate::addon_media_nfo::serialize(root, &metadata).map_err(map_nfo_error)?;
    if crate::addon_media_nfo::requires_preservation(&fallback, &generated)
        .map_err(map_nfo_error)?
    {
        return Err(RepositoryError::Conflict(
            "parent_nfo_preservation_required".to_owned(),
        ));
    }
    Ok(())
}

async fn generated_parent_metadata(
    pool: &Pool<crate::db::Db>,
    id: EntityId,
    file_name: &str,
    title_override: Option<&str>,
    source: Option<&AddonMediaListRevisionSnapshot>,
) -> Result<AddonMediaListNfoMetadata> {
    let (provider_title, description, effective) = if let Some(source) = source {
        (
            source.name.clone(),
            source.description.clone(),
            metadata_json_value(&source.metadata_json)?,
        )
    } else {
        let row = sqlx::query(
            r#"SELECT COALESCE("Name", '') AS "Name", "Description", "ProviderMetadataJson",
                  "MetadataOverrideJson"
           FROM "AddonMediaList" WHERE "Id" = ?"#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await
        .repository()?
        .ok_or(RepositoryError::NotFound {
            kind: "add-on media list",
            id,
        })?;
        let provider_json: String = row.try_get("ProviderMetadataJson").repository()?;
        let override_json: String = row.try_get("MetadataOverrideJson").repository()?;
        let effective = merge_metadata_values(
            metadata_json_value(&provider_json)?,
            &metadata_json_value(&override_json)?,
        );
        (
            row.try_get::<String, _>("Name").repository()?,
            row.try_get::<Option<String>, _>("Description")
                .repository()?,
            effective,
        )
    };
    let mut metadata: AddonMediaListNfoMetadata =
        serde_json::from_value(effective).map_err(|error| RepositoryError::Decode {
            kind: "add-on list metadata",
            value: error.to_string(),
        })?;
    let title = title_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| (!metadata.title.trim().is_empty()).then_some(metadata.title.trim()))
        .unwrap_or_else(|| provider_title.trim());
    if title.is_empty() {
        return Err(RepositoryError::Conflict(format!(
            "{file_name} cannot be generated because the add-on supplied no title"
        )));
    }
    metadata.title = title.to_owned();
    if metadata.plot.is_none() {
        metadata.plot = description;
    }
    Ok(metadata)
}

#[allow(clippy::too_many_lines)]
async fn materialize_parent_artwork(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    nfo_path: &Path,
    metadata: &mut AddonMediaListNfoMetadata,
) -> Result<()> {
    let folder = nfo_path.parent().ok_or_else(|| {
        RepositoryError::unavailable(
            "publish add-on artwork",
            "the parent NFO has no containing folder",
        )
    })?;
    let selected = selected_parent_artwork(metadata);
    let client = crate::addon_artwork::client()
        .map_err(|error| RepositoryError::unavailable("publish add-on artwork", error))?;
    let revision: i32 =
        sqlx::query_scalar(r#"SELECT "PublishedRevision" FROM "AddonMediaList" WHERE "Id" = ?"#)
            .bind(list_id)
            .fetch_one(pool)
            .await
            .repository()?;
    let mut published = Vec::new();
    for role in [
        AddonMediaListArtworkRole::Poster,
        AddonMediaListArtworkRole::Fanart,
        AddonMediaListArtworkRole::Thumb,
    ] {
        let Some(mut candidate) = selected
            .iter()
            .find(|candidate| candidate.role == role)
            .cloned()
        else {
            remove_parent_materialization(pool, list_id, folder, role).await?;
            continue;
        };
        let source = candidate.url.trim();
        let (bytes, extension) =
            load_parent_artwork_bytes(pool, &client, list_id, role, source).await?;
        let existing = load_parent_materialization(pool, list_id, role).await?;
        if let Some(existing) = &existing {
            let path = folder.join(&existing.file_name);
            if existing.source == source && file_has_sha256(&path, &existing.sha256) {
                retain_parent_materialization(
                    pool,
                    list_id,
                    role,
                    source,
                    existing,
                    revision.saturating_add(1),
                )
                .await?;
                candidate.url.clone_from(&existing.file_name);
                published.push(candidate);
                continue;
            }
        }
        if let Some((file_name, sha256)) =
            existing_parent_operator_artwork(folder, role, existing.as_ref())
        {
            upsert_parent_materialization(
                pool,
                list_id,
                role,
                source,
                &file_name,
                &sha256,
                revision.saturating_add(1),
            )
            .await?;
            candidate.url = file_name;
            published.push(candidate);
            continue;
        }
        let file_name = parent_artwork_file_name(role, extension);
        let path = folder.join(&file_name);
        if path.parent() != Some(folder) {
            return Err(RepositoryError::unavailable(
                "publish add-on artwork",
                "the artwork path escaped its managed folder",
            ));
        }
        crate::addon_artwork::write_atomically(&path, &bytes)
            .map_err(|error| RepositoryError::unavailable("publish add-on artwork", error))?;
        let sha256 = hex_hash(&bytes);
        if let Some(existing) = existing.filter(|existing| existing.file_name != file_name) {
            let old_path = folder.join(existing.file_name);
            if file_has_sha256(&old_path, &existing.sha256) {
                std::fs::remove_file(old_path).map_err(|error| {
                    RepositoryError::unavailable("remove stale add-on artwork", error.to_string())
                })?;
            }
        }
        upsert_parent_materialization(
            pool,
            list_id,
            role,
            source,
            &file_name,
            &sha256,
            revision.saturating_add(1),
        )
        .await?;
        candidate.url = file_name;
        published.push(candidate);
    }
    metadata.artwork = published;
    metadata.artwork_urls.clear();
    Ok(())
}

async fn retain_parent_materialization(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    role: AddonMediaListArtworkRole,
    source: &str,
    existing: &ArtworkMaterializationSnapshot,
    revision: i32,
) -> Result<()> {
    upsert_parent_materialization(
        pool,
        list_id,
        role,
        source,
        &existing.file_name,
        &existing.sha256,
        revision,
    )
    .await
}

fn selected_parent_artwork(metadata: &AddonMediaListNfoMetadata) -> Vec<AddonMediaListArtwork> {
    let mut selected = metadata.artwork.clone();
    if !selected
        .iter()
        .any(|candidate| candidate.role == AddonMediaListArtworkRole::Thumb)
    {
        if let Some(url) = metadata.artwork_urls.first() {
            selected.push(AddonMediaListArtwork {
                url: url.clone(),
                role: AddonMediaListArtworkRole::Thumb,
                width: None,
                height: None,
                language: None,
            });
        }
    }
    selected
}

async fn load_parent_artwork_bytes(
    pool: &Pool<crate::db::Db>,
    client: &reqwest::Client,
    list_id: EntityId,
    role: AddonMediaListArtworkRole,
    source: &str,
) -> Result<(Vec<u8>, &'static str)> {
    if let Some(upload_id) = crate::addon_artwork::upload_id(source) {
        let row = sqlx::query_as::<_, (Vec<u8>, String)>(
            r#"SELECT "Bytes", "ContentType" FROM "AddonMediaListArtworkUpload"
               WHERE "Id" = ? AND "AddonMediaListId" = ?
                 AND "ScopeItemId" = 0 AND "Role" = ?"#,
        )
        .bind(upload_id)
        .bind(list_id)
        .bind(artwork_role_value(role))
        .fetch_optional(pool)
        .await
        .repository()?
        .ok_or_else(|| {
            RepositoryError::Conflict(
                "selected artwork upload does not belong to this list and role".to_owned(),
            )
        })?;
        let (_, extension) = crate::addon_artwork::validate_image(&row.1, &row.0)
            .map_err(RepositoryError::Conflict)?;
        return Ok((row.0, extension));
    }
    if !is_safe_remote_artwork_url(source) {
        return Err(RepositoryError::Conflict(
            "selected artwork must use a bounded HTTPS URL".to_owned(),
        ));
    }
    crate::addon_artwork::download(client, source)
        .await
        .map_err(|error| RepositoryError::unavailable("download add-on artwork", error))
}

async fn load_parent_materialization(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    role: AddonMediaListArtworkRole,
) -> Result<Option<ArtworkMaterializationSnapshot>> {
    let row = sqlx::query(
        r#"SELECT "Id", "AddonMediaListItemId", "ScopeItemId", "Role", "Source",
                  "FileName", "Sha256", "PublishedRevision"
           FROM "AddonMediaListArtworkMaterialization"
           WHERE "AddonMediaListId" = ? AND "ScopeItemId" = 0 AND "Role" = ?"#,
    )
    .bind(list_id)
    .bind(artwork_role_value(role))
    .fetch_optional(pool)
    .await
    .repository()?;
    row.map(|row| {
        Ok(ArtworkMaterializationSnapshot {
            id: row.try_get("Id").repository()?,
            item_id: row.try_get("AddonMediaListItemId").repository()?,
            scope_item_id: row.try_get("ScopeItemId").repository()?,
            role: row.try_get("Role").repository()?,
            source: row.try_get("Source").repository()?,
            file_name: row.try_get("FileName").repository()?,
            sha256: row.try_get("Sha256").repository()?,
            published_revision: row.try_get("PublishedRevision").repository()?,
        })
    })
    .transpose()
}

fn existing_parent_operator_artwork(
    folder: &Path,
    role: AddonMediaListArtworkRole,
    previous: Option<&ArtworkMaterializationSnapshot>,
) -> Option<(String, String)> {
    ["jpg", "jpeg", "png", "webp"]
        .into_iter()
        .find_map(|extension| {
            let file_name = parent_artwork_file_name(role, extension);
            let path = folder.join(&file_name);
            if !path.is_file()
                || previous.is_some_and(|previous| {
                    previous.file_name == file_name && file_has_sha256(&path, &previous.sha256)
                })
            {
                return None;
            }
            std::fs::read(path)
                .ok()
                .map(|bytes| (file_name, hex_hash(&bytes)))
        })
}

async fn upsert_parent_materialization(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    role: AddonMediaListArtworkRole,
    source: &str,
    file_name: &str,
    sha256: &str,
    revision: i32,
) -> Result<()> {
    let role = artwork_role_value(role);
    let updated = sqlx::query(
        r#"UPDATE "AddonMediaListArtworkMaterialization"
           SET "Source" = ?, "FileName" = ?, "Sha256" = ?, "PublishedRevision" = ?
           WHERE "AddonMediaListId" = ? AND "ScopeItemId" = 0 AND "Role" = ?"#,
    )
    .bind(source)
    .bind(file_name)
    .bind(sha256)
    .bind(revision)
    .bind(list_id)
    .bind(role)
    .execute(pool)
    .await
    .repository()?;
    if updated.rows_affected() == 0 {
        sqlx::query(
            r#"INSERT INTO "AddonMediaListArtworkMaterialization" (
                   "AddonMediaListId", "AddonMediaListItemId", "ScopeItemId", "Role",
                   "Source", "FileName", "Sha256", "PublishedRevision"
               ) VALUES (?, NULL, 0, ?, ?, ?, ?, ?)"#,
        )
        .bind(list_id)
        .bind(role)
        .bind(source)
        .bind(file_name)
        .bind(sha256)
        .bind(revision)
        .execute(pool)
        .await
        .repository()?;
    }
    Ok(())
}

async fn remove_parent_materialization(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    folder: &Path,
    role: AddonMediaListArtworkRole,
) -> Result<()> {
    if let Some(existing) = load_parent_materialization(pool, list_id, role).await? {
        let path = folder.join(&existing.file_name);
        if file_has_sha256(&path, &existing.sha256) {
            std::fs::remove_file(path).map_err(|error| {
                RepositoryError::unavailable("remove stale add-on artwork", error.to_string())
            })?;
        }
        sqlx::query(r#"DELETE FROM "AddonMediaListArtworkMaterialization" WHERE "Id" = ?"#)
            .bind(existing.id)
            .execute(pool)
            .await
            .repository()?;
    }
    Ok(())
}

fn parent_artwork_file_name(role: AddonMediaListArtworkRole, extension: &str) -> String {
    let name = match role {
        AddonMediaListArtworkRole::Poster => "poster",
        AddonMediaListArtworkRole::Fanart => "fanart",
        AddonMediaListArtworkRole::Thumb | AddonMediaListArtworkRole::Other => "thumb",
    };
    // `extension` comes exclusively from signature validation.
    format!("{name}.{extension}")
}

fn file_has_sha256(path: &Path, expected: &str) -> bool {
    std::fs::read(path).is_ok_and(|bytes| hex_hash(&bytes) == expected)
}

/// Apply the requested presentation, reverting whatever the list had before.
///
/// `Auto` is resolved from the entries themselves; every other kind is applied
/// as asked. Projections other than the effective one are always reverted
/// first, so switching between them cannot leave two live hierarchies over the
/// same provider items.
async fn reconcile_projection(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    requested: AddonMediaListPresentationKind,
    source: Option<&ersatzrs_core::repositories::AddonMediaListRevisionSnapshot>,
) -> Result<()> {
    let effective = if let Some(snapshot) = source {
        if snapshot.list_id != list_id {
            return Err(RepositoryError::Conflict(
                "the publication projection belongs to another list".into(),
            ));
        }
        snapshot.presentation_kind
    } else if requested == AddonMediaListPresentationKind::Auto {
        resolve_auto_presentation(pool, list_id).await?
    } else {
        requested
    };
    match effective {
        AddonMediaListPresentationKind::RemoteStreams | AddonMediaListPresentationKind::Auto => {
            revert_projections(pool, list_id).await
        }
        AddonMediaListPresentationKind::TelevisionShow => {
            revert_music_video_projection(pool, list_id).await?;
            revert_flat_projections(pool, list_id).await?;
            apply_tv_projection(pool, list_id, source).await
        }
        AddonMediaListPresentationKind::Movies => {
            apply_flat(pool, list_id, FlatProjection::MOVIES, source).await
        }
        AddonMediaListPresentationKind::OtherVideos => {
            apply_flat(pool, list_id, FlatProjection::OTHER_VIDEOS, source).await
        }
        AddonMediaListPresentationKind::Songs => {
            apply_flat(pool, list_id, FlatProjection::SONGS, source).await
        }
        AddonMediaListPresentationKind::Images => {
            apply_flat(pool, list_id, FlatProjection::IMAGES, source).await
        }
        AddonMediaListPresentationKind::MusicVideos => {
            revert_tv_projection(pool, list_id).await?;
            revert_flat_projections(pool, list_id).await?;
            apply_music_video_projection(pool, list_id, source).await
        }
    }
}

/// Detach the Music Video projection, its managed Artist, and their media items.
async fn revert_music_video_projection(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
) -> Result<()> {
    let references: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*)
           FROM "AddonMediaListItemMusicVideo" AS "projection"
           WHERE "projection"."AddonMediaListItemId" IN (
               SELECT "Id" FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ?
           ) AND (
               EXISTS (
                   SELECT 1 FROM "CollectionItem"
                   WHERE "MediaItemId" = "projection"."MusicVideoId"
               )
               OR EXISTS (
                   SELECT 1 FROM "PlaylistItem"
                   WHERE "MediaItemId" = "projection"."MusicVideoId"
                     AND "PlaylistId" <> COALESCE(
                         (SELECT "PlaylistId" FROM "AddonMediaList" WHERE "Id" = ?), -1
                     )
               )
               OR EXISTS (
                   SELECT 1 FROM "PlayoutItem"
                   WHERE "MediaItemId" = "projection"."MusicVideoId"
               )
           )"#,
    )
    .bind(list_id)
    .bind(list_id)
    .fetch_one(pool)
    .await
    .repository()?;
    if references > 0 {
        return Err(RepositoryError::Conflict(
            "projected music videos are still referenced by a collection, playlist, or playout"
                .to_owned(),
        ));
    }

    let music_video_ids = sqlx::query_scalar::<_, EntityId>(
        r#"SELECT "projection"."MusicVideoId"
           FROM "AddonMediaListItemMusicVideo" AS "projection"
           JOIN "AddonMediaListItem" AS "item"
             ON "item"."Id" = "projection"."AddonMediaListItemId"
           WHERE "item"."AddonMediaListId" = ?"#,
    )
    .bind(list_id)
    .fetch_all(pool)
    .await
    .repository()?;
    let artist_id = sqlx::query_scalar::<_, EntityId>(
        r#"SELECT "ArtistId" FROM "AddonMediaListArtist" WHERE "AddonMediaListId" = ?"#,
    )
    .bind(list_id)
    .fetch_optional(pool)
    .await
    .repository()?;
    if music_video_ids.is_empty() && artist_id.is_none() {
        return Ok(());
    }

    let mut tx = crate::db::begin_write(pool).await.repository()?;
    for music_video_id in music_video_ids {
        // The Remote Stream version is shared, not owned.
        sqlx::query(r#"UPDATE "MediaVersion" SET "MusicVideoId" = NULL WHERE "MusicVideoId" = ?"#)
            .bind(music_video_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        sqlx::query(r#"DELETE FROM "MediaItem" WHERE "Id" = ?"#)
            .bind(music_video_id)
            .execute(&mut *tx)
            .await
            .repository()?;
    }
    if let Some(artist_id) = artist_id {
        sqlx::query(r#"DELETE FROM "MediaItem" WHERE "Id" = ?"#)
            .bind(artist_id)
            .execute(&mut *tx)
            .await
            .repository()?;
    }
    sqlx::query(
        r#"UPDATE "AddonMediaList"
           SET "PresentationKind" = 0, "PresentationError" = NULL WHERE "Id" = ?"#,
    )
    .bind(list_id)
    .execute(&mut *tx)
    .await
    .repository()?;
    tx.commit().await.repository()?;
    Ok(())
}

/// Project one list as an Artist with Music Videos over its Remote Streams.
///
/// `MusicVideo.ArtistId` is NOT NULL, so this is the only remaining kind with a
/// parent: one managed Artist named by `artist.nfo`, the way a television list
/// is named by `tvshow.nfo`.
#[allow(clippy::too_many_lines)]
async fn apply_music_video_projection(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    source: Option<&ersatzrs_core::repositories::AddonMediaListRevisionSnapshot>,
) -> Result<()> {
    let artist_path = parent_nfo_path_for_source(pool, list_id, "artist.nfo", source)
        .await?
        .ok_or_else(|| {
            RepositoryError::Conflict("managed list storage is unavailable".to_owned())
        })?;
    let artist_form =
        crate::addon_media_nfo::read_form(Some(&artist_path)).map_err(map_nfo_error)?;
    if artist_form.metadata.title.trim().is_empty() {
        return Err(RepositoryError::Conflict(
            "artist.nfo must contain an artist name".to_owned(),
        ));
    }

    let rows = publication_projection::load_items(pool, list_id, source).await?;
    let mut projected = Vec::with_capacity(rows.len());
    for row in rows {
        let item_id = row.id;
        let path = item_nfo_path_for(pool, list_id, item_id)
            .await?
            .ok_or_else(|| {
                RepositoryError::Conflict(format!(
                    "item {item_id} must be scanned before music video conversion"
                ))
            })?;
        let form = crate::addon_media_nfo::read_form(Some(&path)).map_err(map_nfo_error)?;
        let mut metadata = form.metadata;
        if metadata.title.trim().is_empty() {
            metadata.title = row.title;
        }
        metadata.year = metadata.year.or(row.year);
        crate::addon_media_nfo::save_user(&path, "musicvideo", &metadata, &form.etag)
            .map_err(map_nfo_error)?;
        projected.push(ProjectedEpisodeInput {
            item_id,
            metadata,
            nfo_path: path,
            state: i32::from(row.availability == AddonMediaListItemAvailability::Unavailable) * 2,
        });
    }

    let library_path_id = sqlx::query_scalar::<_, EntityId>(
        r#"SELECT "path"."Id"
           FROM "AddonMediaListLibrary" AS "managed"
           JOIN "LibraryPath" AS "path" ON "path"."LibraryId" = "managed"."LibraryId"
           WHERE "managed"."AddonMediaListId" = ? ORDER BY "path"."Id" LIMIT 1"#,
    )
    .bind(list_id)
    .fetch_optional(pool)
    .await
    .repository()?
    .ok_or_else(|| RepositoryError::Conflict("managed library path is unavailable".to_owned()))?;
    let now_text = crate::repositories::common::encode_datetime(chrono::Utc::now());
    let mut tx = crate::db::begin_write(pool).await.repository()?;

    let artist_id = if let Some(artist_id) = sqlx::query_scalar::<_, EntityId>(
        r#"SELECT "ArtistId" FROM "AddonMediaListArtist" WHERE "AddonMediaListId" = ?"#,
    )
    .bind(list_id)
    .fetch_optional(&mut *tx)
    .await
    .repository()?
    {
        artist_id
    } else {
        let result =
            sqlx::query(r#"INSERT INTO "MediaItem" ("LibraryPathId", "State") VALUES (?, 0)"#)
                .bind(library_path_id)
                .execute(&mut *tx)
                .await
                .repository()?;
        let artist_id = crate::db::inserted_id(&mut tx, result).await.repository()?;
        sqlx::query(r#"INSERT INTO "Artist" ("Id") VALUES (?)"#)
            .bind(artist_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        sqlx::query(
            r#"INSERT INTO "AddonMediaListArtist" ("AddonMediaListId", "ArtistId")
               VALUES (?, ?)"#,
        )
        .bind(list_id)
        .bind(artist_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        artist_id
    };
    // Consumers join artist metadata to resolve the name and artwork, so an
    // artist without one is invisible to them.
    sqlx::query(r#"DELETE FROM "ArtistMetadata" WHERE "ArtistId" = ?"#)
        .bind(artist_id)
        .execute(&mut *tx)
        .await
        .repository()?;
    let result = sqlx::query(
        r#"INSERT INTO "ArtistMetadata" (
              "ArtistId", "MetadataKind", "Title", "OriginalTitle", "SortTitle",
              "Biography", "Year", "ReleaseDate", "DateAdded", "DateUpdated"
           ) VALUES (?, 1, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(artist_id)
    .bind(&artist_form.metadata.title)
    .bind(&artist_form.metadata.original_title)
    .bind(&artist_form.metadata.sort_title)
    .bind(&artist_form.metadata.plot)
    .bind(artist_form.metadata.year)
    .bind(&artist_form.metadata.release_date)
    .bind(&now_text)
    .bind(&now_text)
    .execute(&mut *tx)
    .await
    .repository()?;
    let artist_metadata_id = crate::db::inserted_id(&mut tx, result).await.repository()?;
    replace_projected_metadata_relations(
        &mut tx,
        ProjectedMetadataRelations::ARTIST,
        artist_metadata_id,
        &artist_form.metadata,
        &artist_path,
    )
    .await?;

    for input in projected {
        let music_video_id = if let Some(music_video_id) = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "MusicVideoId" FROM "AddonMediaListItemMusicVideo"
               WHERE "AddonMediaListItemId" = ?"#,
        )
        .bind(input.item_id)
        .fetch_optional(&mut *tx)
        .await
        .repository()?
        {
            sqlx::query(r#"UPDATE "MusicVideo" SET "ArtistId" = ? WHERE "Id" = ?"#)
                .bind(artist_id)
                .bind(music_video_id)
                .execute(&mut *tx)
                .await
                .repository()?;
            music_video_id
        } else {
            let result =
                sqlx::query(r#"INSERT INTO "MediaItem" ("LibraryPathId", "State") VALUES (?, ?)"#)
                    .bind(library_path_id)
                    .bind(input.state)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
            let music_video_id = crate::db::inserted_id(&mut tx, result).await.repository()?;
            sqlx::query(r#"INSERT INTO "MusicVideo" ("Id", "ArtistId") VALUES (?, ?)"#)
                .bind(music_video_id)
                .bind(artist_id)
                .execute(&mut *tx)
                .await
                .repository()?;
            sqlx::query(
                r#"INSERT INTO "AddonMediaListItemMusicVideo"
                       ("AddonMediaListItemId", "MusicVideoId")
                   VALUES (?, ?)"#,
            )
            .bind(input.item_id)
            .bind(music_video_id)
            .execute(&mut *tx)
            .await
            .repository()?;
            music_video_id
        };
        sqlx::query(r#"UPDATE "MediaItem" SET "State" = ? WHERE "Id" = ?"#)
            .bind(input.state)
            .bind(music_video_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        sqlx::query(
            r#"UPDATE "MediaVersion" SET "MusicVideoId" = ?
               WHERE "RemoteStreamId" = (
                   SELECT "MediaItemId" FROM "AddonMediaListItem" WHERE "Id" = ?
               )"#,
        )
        .bind(music_video_id)
        .bind(input.item_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        sqlx::query(r#"DELETE FROM "MusicVideoMetadata" WHERE "MusicVideoId" = ?"#)
            .bind(music_video_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        let result = sqlx::query(
            r#"INSERT INTO "MusicVideoMetadata" (
                  "MusicVideoId", "MetadataKind", "Title", "OriginalTitle", "SortTitle",
                  "Plot", "Year", "ReleaseDate", "DateAdded", "DateUpdated"
               ) VALUES (?, 1, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(music_video_id)
        .bind(&input.metadata.title)
        .bind(&input.metadata.original_title)
        .bind(&input.metadata.sort_title)
        .bind(&input.metadata.plot)
        .bind(input.metadata.year)
        .bind(&input.metadata.release_date)
        .bind(&now_text)
        .bind(&now_text)
        .execute(&mut *tx)
        .await
        .repository()?;
        let metadata_id = crate::db::inserted_id(&mut tx, result).await.repository()?;
        replace_projected_metadata_relations(
            &mut tx,
            ProjectedMetadataRelations::MUSIC_VIDEO,
            metadata_id,
            &input.metadata,
            &input.nfo_path,
        )
        .await?;
    }

    sqlx::query(
        r#"UPDATE "AddonMediaList"
           SET "PresentationKind" = ?, "PresentationError" = NULL WHERE "Id" = ?"#,
    )
    .bind(crate::repositories::common::encode_enum(
        AddonMediaListPresentationKind::MusicVideos,
    ))
    .bind(list_id)
    .execute(&mut *tx)
    .await
    .repository()?;
    tx.commit().await.repository()?;
    Ok(())
}

/// Apply one flat projection after detaching whatever the list had before.
async fn apply_flat(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    projection: FlatProjection,
    source: Option<&ersatzrs_core::repositories::AddonMediaListRevisionSnapshot>,
) -> Result<()> {
    revert_tv_projection(pool, list_id).await?;
    revert_music_video_projection(pool, list_id).await?;
    for other in FlatProjection::ALL {
        if other.link_table != projection.link_table {
            revert_flat_projection(pool, list_id, other).await?;
        }
    }
    apply_flat_projection(pool, list_id, projection, source).await
}

/// Derive the presentation from how the entries classify themselves.
///
/// A list only auto-projects when every entry agrees and none is still
/// unclassified; anything else stays plain Remote Streams, because guessing a
/// hierarchy for a mixed list would be worse than leaving it alone.
async fn resolve_auto_presentation(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
) -> Result<AddonMediaListPresentationKind> {
    let kinds = sqlx::query_scalar::<_, i32>(
        r#"SELECT DISTINCT "ContentKind" FROM "AddonMediaListItem"
           WHERE "AddonMediaListId" = ? AND "ParentProviderId" IS NULL
             AND "Published" = 1"#,
    )
    .bind(list_id)
    .fetch_all(pool)
    .await
    .repository()?;
    let [only] = kinds.as_slice() else {
        return Ok(AddonMediaListPresentationKind::RemoteStreams);
    };
    let Some(kind) =
        <AddonMediaListContentKind as crate::repositories::common::DbEnum>::from_db_int(*only)
    else {
        return Ok(AddonMediaListPresentationKind::RemoteStreams);
    };
    let derived = AddonMediaListPresentationKind::from_content_kind(kind)
        .unwrap_or(AddonMediaListPresentationKind::RemoteStreams);
    // A kind whose projection does not exist yet must not be entered silently.
    Ok(if derived.is_projected() {
        derived
    } else {
        AddonMediaListPresentationKind::RemoteStreams
    })
}

/// Revert one list to plain Remote Streams, detaching every projection.
///
/// Reverting is refused while a projected media item is referenced from
/// outside the list, so a schedule or collection cannot lose its target.
async fn revert_projections(pool: &Pool<crate::db::Db>, list_id: EntityId) -> Result<()> {
    revert_music_video_projection(pool, list_id).await?;
    revert_flat_projections(pool, list_id).await?;
    revert_tv_projection(pool, list_id).await
}

/// One parentless media kind an add-on list can be projected into.
///
/// Movies, Other Videos, Songs, and Images all become one media item per entry
/// with no container above them, so they differ only in which tables they write.
#[derive(Clone, Copy)]
struct FlatProjection {
    /// Link table joining one list entry to its projected media item.
    link_table: &'static str,
    /// Column naming the projected media item inside the link table.
    link_column: &'static str,
    /// Concrete media table, which shares its primary key with `MediaItem`.
    media_table: &'static str,
    /// Column naming the projected media item inside the metadata table.
    metadata_owner_column: &'static str,
    /// Metadata table written from each entry's NFO.
    metadata_table: &'static str,
    /// Shared related-row capabilities for this metadata record.
    metadata_relations: ProjectedMetadataRelations,
    /// NFO document root this kind is stored under.
    nfo_root: &'static str,
    /// Whether the metadata table has the video description and rating columns.
    ///
    /// `SongMetadata` and `ImageMetadata` do not, so writing them would name a
    /// column that does not exist.
    descriptive_metadata: bool,
    presentation: AddonMediaListPresentationKind,
}

impl FlatProjection {
    const MOVIES: Self = Self {
        link_table: "AddonMediaListItemMovie",
        link_column: "MovieId",
        media_table: "Movie",
        metadata_owner_column: "MovieId",
        metadata_table: "MovieMetadata",
        metadata_relations: ProjectedMetadataRelations::MOVIE,
        nfo_root: "movie",
        descriptive_metadata: true,
        presentation: AddonMediaListPresentationKind::Movies,
    };
    const OTHER_VIDEOS: Self = Self {
        link_table: "AddonMediaListItemOtherVideo",
        link_column: "OtherVideoId",
        media_table: "OtherVideo",
        metadata_owner_column: "OtherVideoId",
        metadata_table: "OtherVideoMetadata",
        metadata_relations: ProjectedMetadataRelations::OTHER_VIDEO,
        nfo_root: "movie",
        descriptive_metadata: true,
        presentation: AddonMediaListPresentationKind::OtherVideos,
    };

    const SONGS: Self = Self {
        link_table: "AddonMediaListItemSong",
        link_column: "SongId",
        media_table: "Song",
        metadata_owner_column: "SongId",
        metadata_table: "SongMetadata",
        metadata_relations: ProjectedMetadataRelations::SONG,
        // Kodi defines no song NFO document; `musicvideo` is the nearest root it
        // does define, and the only music-shaped one `serialize` accepts.
        nfo_root: "musicvideo",
        descriptive_metadata: false,
        presentation: AddonMediaListPresentationKind::Songs,
    };

    const IMAGES: Self = Self {
        link_table: "AddonMediaListItemImage",
        link_column: "ImageId",
        media_table: "Image",
        metadata_owner_column: "ImageId",
        metadata_table: "ImageMetadata",
        metadata_relations: ProjectedMetadataRelations::IMAGE,
        // Kodi defines no still-image NFO document either; the generic root is
        // the only one `serialize` accepts for a non-music item.
        nfo_root: "movie",
        descriptive_metadata: false,
        presentation: AddonMediaListPresentationKind::Images,
    };

    /// Every flat projection, so a revert can detach whichever one is live.
    const ALL: [Self; 4] = [Self::MOVIES, Self::OTHER_VIDEOS, Self::SONGS, Self::IMAGES];
}

/// Detach every flat projection a list may have, in turn.
async fn revert_flat_projections(pool: &Pool<crate::db::Db>, list_id: EntityId) -> Result<()> {
    for projection in FlatProjection::ALL {
        revert_flat_projection(pool, list_id, projection).await?;
    }
    Ok(())
}

/// Detach one flat projection and delete the media items it created.
///
/// Mirrors the television revert: a projected item still referenced from a
/// collection, another playlist, or a playout blocks the change instead of
/// silently removing a schedule's target.
///
/// The identifiers come from a closed set of compile-time constants, never from
/// operator input, so the statements are assembled rather than parameterized.
async fn revert_flat_projection(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    projection: FlatProjection,
) -> Result<()> {
    let FlatProjection {
        link_table,
        link_column,
        ..
    } = projection;
    let references: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        r#"SELECT COUNT(*)
           FROM "{link_table}" AS "projection"
           WHERE "projection"."AddonMediaListItemId" IN (
               SELECT "Id" FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ?
           ) AND (
               EXISTS (SELECT 1 FROM "CollectionItem" WHERE "MediaItemId" = "projection"."{link_column}")
               OR EXISTS (
                   SELECT 1 FROM "PlaylistItem"
                   WHERE "MediaItemId" = "projection"."{link_column}"
                     AND "PlaylistId" <> COALESCE(
                         (SELECT "PlaylistId" FROM "AddonMediaList" WHERE "Id" = ?), -1
                     )
               )
               OR EXISTS (SELECT 1 FROM "PlayoutItem" WHERE "MediaItemId" = "projection"."{link_column}")
           )"#
    )))
    .bind(list_id)
    .bind(list_id)
    .fetch_one(pool)
    .await
    .repository()?;
    if references > 0 {
        return Err(RepositoryError::Conflict(format!(
            "projected {} items are still referenced by a collection, playlist, or playout",
            projection.media_table
        )));
    }

    let media_ids = sqlx::query_scalar::<_, EntityId>(sqlx::AssertSqlSafe(format!(
        r#"SELECT "projection"."{link_column}"
           FROM "{link_table}" AS "projection"
           JOIN "AddonMediaListItem" AS "item"
             ON "item"."Id" = "projection"."AddonMediaListItemId"
           WHERE "item"."AddonMediaListId" = ?"#
    )))
    .bind(list_id)
    .fetch_all(pool)
    .await
    .repository()?;
    if media_ids.is_empty() {
        return Ok(());
    }

    let mut tx = crate::db::begin_write(pool).await.repository()?;
    for media_id in media_ids {
        // The Remote Stream version is shared, not owned, so it is detached
        // rather than deleted: the provider item remains playable.
        sqlx::query(sqlx::AssertSqlSafe(format!(
            r#"UPDATE "MediaVersion" SET "{link_column}" = NULL WHERE "{link_column}" = ?"#
        )))
        .bind(media_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        sqlx::query(r#"DELETE FROM "MediaItem" WHERE "Id" = ?"#)
            .bind(media_id)
            .execute(&mut *tx)
            .await
            .repository()?;
    }
    sqlx::query(
        r#"UPDATE "AddonMediaList"
           SET "PresentationKind" = 0, "PresentationError" = NULL WHERE "Id" = ?"#,
    )
    .bind(list_id)
    .execute(&mut *tx)
    .await
    .repository()?;
    tx.commit().await.repository()?;
    Ok(())
}

/// Project every entry of one list as a Movie over its Remote Stream version.
///
/// A movie list has no parent container, so this is the television projection
/// without the Show and Season levels: each entry's NFO owns its metadata and
/// the scanned Remote Stream `MediaVersion` is attached to the projected movie.
/// Project every entry of one list into a parentless media kind.
///
/// This is the television projection without the Show and Season levels: each
/// entry's NFO owns its metadata and the scanned Remote Stream `MediaVersion` is
/// attached to the projected item. The identifiers come from a closed set of
/// compile-time constants, never from operator input.
#[allow(clippy::too_many_lines)]
async fn apply_flat_projection(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    projection: FlatProjection,
    source: Option<&ersatzrs_core::repositories::AddonMediaListRevisionSnapshot>,
) -> Result<()> {
    let FlatProjection {
        link_table,
        link_column,
        media_table,
        metadata_owner_column,
        metadata_table,
        metadata_relations,
        nfo_root,
        descriptive_metadata,
        presentation,
    } = projection;
    let rows = publication_projection::load_items(pool, list_id, source).await?;

    let mut projected = Vec::with_capacity(rows.len());
    for row in rows {
        let item_id = row.id;
        let path = item_nfo_path_for(pool, list_id, item_id)
            .await?
            .ok_or_else(|| {
                RepositoryError::Conflict(format!(
                    "item {item_id} must be scanned before {media_table} conversion"
                ))
            })?;
        let form = crate::addon_media_nfo::read_form(Some(&path)).map_err(map_nfo_error)?;
        let mut metadata = form.metadata;
        if metadata.title.trim().is_empty() {
            metadata.title = row.title;
        }
        metadata.year = metadata.year.or(row.year);
        crate::addon_media_nfo::save_user(&path, nfo_root, &metadata, &form.etag)
            .map_err(map_nfo_error)?;
        projected.push(ProjectedEpisodeInput {
            item_id,
            metadata,
            nfo_path: path,
            state: i32::from(row.availability == AddonMediaListItemAvailability::Unavailable) * 2,
        });
    }

    let library_path_id = sqlx::query_scalar::<_, EntityId>(
        r#"SELECT "path"."Id"
           FROM "AddonMediaListLibrary" AS "managed"
           JOIN "LibraryPath" AS "path" ON "path"."LibraryId" = "managed"."LibraryId"
           WHERE "managed"."AddonMediaListId" = ? ORDER BY "path"."Id" LIMIT 1"#,
    )
    .bind(list_id)
    .fetch_optional(pool)
    .await
    .repository()?
    .ok_or_else(|| RepositoryError::Conflict("managed library path is unavailable".to_owned()))?;
    let now_text = crate::repositories::common::encode_datetime(chrono::Utc::now());
    let mut tx = crate::db::begin_write(pool).await.repository()?;

    for input in projected {
        let media_id = if let Some(media_id) =
            sqlx::query_scalar::<_, EntityId>(sqlx::AssertSqlSafe(format!(
                r#"SELECT "{link_column}" FROM "{link_table}"
                   WHERE "AddonMediaListItemId" = ?"#
            )))
            .bind(input.item_id)
            .fetch_optional(&mut *tx)
            .await
            .repository()?
        {
            media_id
        } else {
            let result =
                sqlx::query(r#"INSERT INTO "MediaItem" ("LibraryPathId", "State") VALUES (?, ?)"#)
                    .bind(library_path_id)
                    .bind(input.state)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
            let media_id = crate::db::inserted_id(&mut tx, result).await.repository()?;
            sqlx::query(sqlx::AssertSqlSafe(format!(
                r#"INSERT INTO "{media_table}" ("Id") VALUES (?)"#
            )))
            .bind(media_id)
            .execute(&mut *tx)
            .await
            .repository()?;
            sqlx::query(sqlx::AssertSqlSafe(format!(
                r#"INSERT INTO "{link_table}" ("AddonMediaListItemId", "{link_column}")
                   VALUES (?, ?)"#
            )))
            .bind(input.item_id)
            .bind(media_id)
            .execute(&mut *tx)
            .await
            .repository()?;
            media_id
        };
        sqlx::query(r#"UPDATE "MediaItem" SET "State" = ? WHERE "Id" = ?"#)
            .bind(input.state)
            .bind(media_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        sqlx::query(sqlx::AssertSqlSafe(format!(
            r#"UPDATE "MediaVersion" SET "{link_column}" = ?
               WHERE "RemoteStreamId" = (
                   SELECT "MediaItemId" FROM "AddonMediaListItem" WHERE "Id" = ?
               )"#
        )))
        .bind(media_id)
        .bind(input.item_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        sqlx::query(sqlx::AssertSqlSafe(format!(
            r#"DELETE FROM "{metadata_table}" WHERE "{metadata_owner_column}" = ?"#
        )))
        .bind(media_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        // Only the video kinds carry a rating and the long-form description
        // columns; a song or an image metadata table simply has no such columns.
        let metadata_id = if descriptive_metadata {
            let result = sqlx::query(sqlx::AssertSqlSafe(format!(
                r#"INSERT INTO "{metadata_table}" (
                      "{metadata_owner_column}", "MetadataKind", "Title", "OriginalTitle",
                      "SortTitle", "Outline", "Plot", "Tagline", "Year", "ReleaseDate",
                      "ContentRating", "DateAdded", "DateUpdated"
                   ) VALUES (?, 1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#
            )))
            .bind(media_id)
            .bind(&input.metadata.title)
            .bind(&input.metadata.original_title)
            .bind(&input.metadata.sort_title)
            .bind(&input.metadata.outline)
            .bind(&input.metadata.plot)
            .bind(&input.metadata.tagline)
            .bind(input.metadata.year)
            .bind(&input.metadata.release_date)
            .bind(input.metadata.content_ratings.first())
            .bind(&now_text)
            .bind(&now_text)
            .execute(&mut *tx)
            .await
            .repository()?;
            crate::db::inserted_id(&mut tx, result).await.repository()?
        } else {
            let result = sqlx::query(sqlx::AssertSqlSafe(format!(
                r#"INSERT INTO "{metadata_table}" (
                      "{metadata_owner_column}", "MetadataKind", "Title", "OriginalTitle",
                      "SortTitle", "Year", "ReleaseDate", "DateAdded", "DateUpdated"
                   ) VALUES (?, 1, ?, ?, ?, ?, ?, ?, ?)"#
            )))
            .bind(media_id)
            .bind(&input.metadata.title)
            .bind(&input.metadata.original_title)
            .bind(&input.metadata.sort_title)
            .bind(input.metadata.year)
            .bind(&input.metadata.release_date)
            .bind(&now_text)
            .bind(&now_text)
            .execute(&mut *tx)
            .await
            .repository()?;
            crate::db::inserted_id(&mut tx, result).await.repository()?
        };
        replace_projected_metadata_relations(
            &mut tx,
            metadata_relations,
            metadata_id,
            &input.metadata,
            &input.nfo_path,
        )
        .await?;
    }

    sqlx::query(
        r#"UPDATE "AddonMediaList"
           SET "PresentationKind" = ?, "PresentationError" = NULL WHERE "Id" = ?"#,
    )
    .bind(crate::repositories::common::encode_enum(presentation))
    .bind(list_id)
    .execute(&mut *tx)
    .await
    .repository()?;
    tx.commit().await.repository()?;
    Ok(())
}

#[allow(clippy::too_many_lines)]
async fn revert_tv_projection(pool: &Pool<crate::db::Db>, list_id: EntityId) -> Result<()> {
    {
        let references: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*)
               FROM "AddonMediaListItemEpisode" AS "projection"
               WHERE "projection"."AddonMediaListItemId" IN (
                   SELECT "Id" FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ?
               ) AND (
                   EXISTS (SELECT 1 FROM "CollectionItem" WHERE "MediaItemId" = "projection"."EpisodeId")
                   OR EXISTS (
                       SELECT 1 FROM "PlaylistItem"
                       WHERE "MediaItemId" = "projection"."EpisodeId"
                         AND "PlaylistId" <> COALESCE(
                             (SELECT "PlaylistId" FROM "AddonMediaList" WHERE "Id" = ?), -1
                         )
                   )
                   OR EXISTS (SELECT 1 FROM "PlayoutItem" WHERE "MediaItemId" = "projection"."EpisodeId")
               )"#,
        )
        .bind(list_id)
        .bind(list_id)
        .fetch_one(pool)
        .await
        .repository()?;
        if references > 0 {
            return Err(RepositoryError::Conflict(
                "projected episodes are still referenced by a collection, playlist, or playout"
                    .to_owned(),
            ));
        }
        let show_id = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "ShowId" FROM "AddonMediaListTelevisionShow"
               WHERE "AddonMediaListId" = ?"#,
        )
        .bind(list_id)
        .fetch_optional(pool)
        .await
        .repository()?;
        let mut tx = crate::db::begin_write(pool).await.repository()?;
        if let Some(show_id) = show_id {
            let episode_ids = sqlx::query_scalar::<_, EntityId>(
                r#"SELECT "projection"."EpisodeId"
                   FROM "AddonMediaListItemEpisode" AS "projection"
                   JOIN "AddonMediaListItem" AS "item"
                     ON "item"."Id" = "projection"."AddonMediaListItemId"
                   WHERE "item"."AddonMediaListId" = ?"#,
            )
            .bind(list_id)
            .fetch_all(&mut *tx)
            .await
            .repository()?;
            for episode_id in episode_ids {
                sqlx::query(
                    r#"UPDATE "MediaVersion" SET "EpisodeId" = NULL WHERE "EpisodeId" = ?"#,
                )
                .bind(episode_id)
                .execute(&mut *tx)
                .await
                .repository()?;
                sqlx::query(r#"DELETE FROM "MediaItem" WHERE "Id" = ?"#)
                    .bind(episode_id)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
            }
            let season_ids = sqlx::query_scalar::<_, EntityId>(
                r#"SELECT "Id" FROM "Season" WHERE "ShowId" = ?"#,
            )
            .bind(show_id)
            .fetch_all(&mut *tx)
            .await
            .repository()?;
            for season_id in season_ids {
                sqlx::query(r#"DELETE FROM "MediaItem" WHERE "Id" = ?"#)
                    .bind(season_id)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
            }
            sqlx::query(r#"DELETE FROM "MediaItem" WHERE "Id" = ?"#)
                .bind(show_id)
                .execute(&mut *tx)
                .await
                .repository()?;
        }
        sqlx::query(
            r#"UPDATE "AddonMediaList"
               SET "PresentationKind" = ?, "PresentationError" = NULL WHERE "Id" = ?"#,
        )
        .bind(crate::repositories::common::encode_enum(
            AddonMediaListPresentationKind::RemoteStreams,
        ))
        .bind(list_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        tx.commit().await.repository()?;
        Ok(())
    }
}

#[allow(clippy::too_many_lines)]
async fn apply_tv_projection(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    source: Option<&ersatzrs_core::repositories::AddonMediaListRevisionSnapshot>,
) -> Result<()> {
    let show_path = parent_nfo_path_for_source(pool, list_id, "tvshow.nfo", source)
        .await?
        .ok_or_else(|| {
            RepositoryError::Conflict("managed list storage is unavailable".to_owned())
        })?;
    let show_form = crate::addon_media_nfo::read_form(Some(&show_path)).map_err(map_nfo_error)?;
    if show_form.metadata.title.trim().is_empty() {
        return Err(RepositoryError::Conflict(
            "tvshow.nfo must contain a show title".to_owned(),
        ));
    }
    let show_title = show_form.metadata.title.trim().to_owned();
    let rows = publication_projection::load_items(pool, list_id, source).await?;
    let mut episodes = Vec::with_capacity(rows.len());
    for row in rows {
        let item_id = row.id;
        let path = item_nfo_path_for(pool, list_id, item_id)
            .await?
            .ok_or_else(|| {
                RepositoryError::Conflict(format!(
                    "item {item_id} must be scanned before television conversion"
                ))
            })?;
        let form = crate::addon_media_nfo::read_form(Some(&path)).map_err(map_nfo_error)?;
        let mut metadata = form.metadata;
        if metadata.title.trim().is_empty() {
            metadata.title = row.title;
        }
        metadata.show_title = Some(show_title.clone());
        metadata.season = metadata.season.or(row.season);
        metadata.episode = metadata.episode.or(row.episode);
        metadata.year = metadata.year.or(row.year);
        crate::addon_media_nfo::save_user(&path, "episodedetails", &metadata, &form.etag)
            .map_err(map_nfo_error)?;
        episodes.push(ProjectedEpisodeInput {
            item_id,
            metadata,
            nfo_path: path,
            state: i32::from(row.availability == AddonMediaListItemAvailability::Unavailable) * 2,
        });
    }

    let library_path_id = sqlx::query_scalar::<_, EntityId>(
        r#"SELECT "path"."Id"
           FROM "AddonMediaListLibrary" AS "managed"
           JOIN "LibraryPath" AS "path" ON "path"."LibraryId" = "managed"."LibraryId"
           WHERE "managed"."AddonMediaListId" = ? ORDER BY "path"."Id" LIMIT 1"#,
    )
    .bind(list_id)
    .fetch_optional(pool)
    .await
    .repository()?
    .ok_or_else(|| RepositoryError::Conflict("managed library path is unavailable".to_owned()))?;
    let now = chrono::Utc::now();
    let now_text = crate::repositories::common::encode_datetime(now);
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    let show_id = if let Some(show_id) = sqlx::query_scalar::<_, EntityId>(
        r#"SELECT "ShowId" FROM "AddonMediaListTelevisionShow"
           WHERE "AddonMediaListId" = ?"#,
    )
    .bind(list_id)
    .fetch_optional(&mut *tx)
    .await
    .repository()?
    {
        show_id
    } else {
        let result =
            sqlx::query(r#"INSERT INTO "MediaItem" ("LibraryPathId", "State") VALUES (?, 0)"#)
                .bind(library_path_id)
                .execute(&mut *tx)
                .await
                .repository()?;
        let show_id = crate::db::inserted_id(&mut tx, result).await.repository()?;
        sqlx::query(r#"INSERT INTO "Show" ("Id") VALUES (?)"#)
            .bind(show_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        sqlx::query(
            r#"INSERT INTO "AddonMediaListTelevisionShow" ("AddonMediaListId", "ShowId")
               VALUES (?, ?)"#,
        )
        .bind(list_id)
        .bind(show_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        show_id
    };
    sqlx::query(r#"DELETE FROM "ShowMetadata" WHERE "ShowId" = ?"#)
        .bind(show_id)
        .execute(&mut *tx)
        .await
        .repository()?;
    let result = sqlx::query(
        r#"INSERT INTO "ShowMetadata" (
              "ShowId", "MetadataKind", "Title", "OriginalTitle", "SortTitle",
              "Outline", "Plot", "Tagline", "Year", "ReleaseDate",
              "ContentRating", "DateAdded", "DateUpdated"
           ) VALUES (?, 1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(show_id)
    .bind(&show_form.metadata.title)
    .bind(&show_form.metadata.original_title)
    .bind(&show_form.metadata.sort_title)
    .bind(&show_form.metadata.outline)
    .bind(&show_form.metadata.plot)
    .bind(&show_form.metadata.tagline)
    .bind(show_form.metadata.year)
    .bind(&show_form.metadata.release_date)
    .bind(show_form.metadata.content_ratings.first())
    .bind(&now_text)
    .bind(&now_text)
    .execute(&mut *tx)
    .await
    .repository()?;
    let show_metadata_id = crate::db::inserted_id(&mut tx, result).await.repository()?;
    replace_projected_metadata_relations(
        &mut tx,
        ProjectedMetadataRelations::SHOW,
        show_metadata_id,
        &show_form.metadata,
        &show_path,
    )
    .await?;

    for input in episodes {
        let season_number = input.metadata.season.unwrap_or(0);
        let season_id = if let Some(season_id) = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "Id" FROM "Season" WHERE "ShowId" = ? AND "SeasonNumber" = ?
               ORDER BY "Id" LIMIT 1"#,
        )
        .bind(show_id)
        .bind(season_number)
        .fetch_optional(&mut *tx)
        .await
        .repository()?
        {
            season_id
        } else {
            let result =
                sqlx::query(r#"INSERT INTO "MediaItem" ("LibraryPathId", "State") VALUES (?, 0)"#)
                    .bind(library_path_id)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
            let season_id = crate::db::inserted_id(&mut tx, result).await.repository()?;
            sqlx::query(
                r#"INSERT INTO "Season" ("Id", "ShowId", "SeasonNumber") VALUES (?, ?, ?)"#,
            )
            .bind(season_id)
            .bind(show_id)
            .bind(season_number)
            .execute(&mut *tx)
            .await
            .repository()?;
            season_id
        };
        // Every other season source carries metadata, and consumers join it to
        // resolve the season title and artwork, so a projected season without
        // one is invisible to them — including the show detail page. This also
        // repairs seasons projected before the metadata row was written.
        sqlx::query(
            r#"INSERT INTO "SeasonMetadata"
                   ("SeasonId", "MetadataKind", "Title", "SortTitle",
                    "DateAdded", "DateUpdated")
               SELECT ?, 0, ?, ?, ?, ?
               WHERE NOT EXISTS (
                   SELECT 1 FROM "SeasonMetadata" WHERE "SeasonId" = ?
               )"#,
        )
        .bind(season_id)
        .bind(season_title(season_number))
        .bind(format!("{season_number:02}"))
        .bind(&now_text)
        .bind(&now_text)
        .bind(season_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        let episode_id = if let Some(episode_id) = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "EpisodeId" FROM "AddonMediaListItemEpisode"
               WHERE "AddonMediaListItemId" = ?"#,
        )
        .bind(input.item_id)
        .fetch_optional(&mut *tx)
        .await
        .repository()?
        {
            sqlx::query(r#"UPDATE "Episode" SET "SeasonId" = ? WHERE "Id" = ?"#)
                .bind(season_id)
                .bind(episode_id)
                .execute(&mut *tx)
                .await
                .repository()?;
            episode_id
        } else {
            let result =
                sqlx::query(r#"INSERT INTO "MediaItem" ("LibraryPathId", "State") VALUES (?, ?)"#)
                    .bind(library_path_id)
                    .bind(input.state)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
            let episode_id = crate::db::inserted_id(&mut tx, result).await.repository()?;
            sqlx::query(r#"INSERT INTO "Episode" ("Id", "SeasonId") VALUES (?, ?)"#)
                .bind(episode_id)
                .bind(season_id)
                .execute(&mut *tx)
                .await
                .repository()?;
            sqlx::query(
                r#"INSERT INTO "AddonMediaListItemEpisode" ("AddonMediaListItemId", "EpisodeId")
                   VALUES (?, ?)"#,
            )
            .bind(input.item_id)
            .bind(episode_id)
            .execute(&mut *tx)
            .await
            .repository()?;
            episode_id
        };
        sqlx::query(r#"UPDATE "MediaItem" SET "State" = ? WHERE "Id" = ?"#)
            .bind(input.state)
            .bind(episode_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        sqlx::query(
            r#"UPDATE "MediaVersion" SET "EpisodeId" = ?
               WHERE "RemoteStreamId" = (
                   SELECT "MediaItemId" FROM "AddonMediaListItem" WHERE "Id" = ?
               )"#,
        )
        .bind(episode_id)
        .bind(input.item_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        sqlx::query(r#"DELETE FROM "EpisodeMetadata" WHERE "EpisodeId" = ?"#)
            .bind(episode_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        let result = sqlx::query(
            r#"INSERT INTO "EpisodeMetadata" (
                  "EpisodeId", "MetadataKind", "Title", "OriginalTitle", "SortTitle",
                  "Outline", "Plot", "Tagline", "Year", "ReleaseDate", "EpisodeNumber",
                  "DateAdded", "DateUpdated"
               ) VALUES (?, 1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(episode_id)
        .bind(&input.metadata.title)
        .bind(&input.metadata.original_title)
        .bind(&input.metadata.sort_title)
        .bind(&input.metadata.outline)
        .bind(&input.metadata.plot)
        .bind(&input.metadata.tagline)
        .bind(input.metadata.year)
        .bind(&input.metadata.release_date)
        .bind(input.metadata.episode.unwrap_or(0))
        .bind(&now_text)
        .bind(&now_text)
        .execute(&mut *tx)
        .await
        .repository()?;
        let metadata_id = crate::db::inserted_id(&mut tx, result).await.repository()?;
        replace_projected_metadata_relations(
            &mut tx,
            ProjectedMetadataRelations::EPISODE,
            metadata_id,
            &input.metadata,
            &input.nfo_path,
        )
        .await?;
    }
    let empty_seasons = sqlx::query_scalar::<_, EntityId>(
        r#"SELECT "season"."Id" FROM "Season" AS "season"
           WHERE "season"."ShowId" = ?
             AND NOT EXISTS (SELECT 1 FROM "Episode" WHERE "SeasonId" = "season"."Id")"#,
    )
    .bind(show_id)
    .fetch_all(&mut *tx)
    .await
    .repository()?;
    for season_id in empty_seasons {
        sqlx::query(r#"DELETE FROM "MediaItem" WHERE "Id" = ?"#)
            .bind(season_id)
            .execute(&mut *tx)
            .await
            .repository()?;
    }
    sqlx::query(
        r#"UPDATE "AddonMediaList"
           SET "PresentationKind" = ?, "PresentationError" = NULL WHERE "Id" = ?"#,
    )
    .bind(crate::repositories::common::encode_enum(
        AddonMediaListPresentationKind::TelevisionShow,
    ))
    .bind(list_id)
    .execute(&mut *tx)
    .await
    .repository()?;
    tx.commit().await.repository()?;
    Ok(())
}

fn replacement_candidate_query(search: bool) -> &'static str {
    if search {
        r#"SELECT "item"."Id" AS "MediaItemId", "version"."Id" AS "MediaVersionId",
                  COALESCE(
                    (SELECT "Title" FROM "MovieMetadata" WHERE "MovieId" = "item"."Id" ORDER BY "Id" LIMIT 1),
                    (SELECT "Title" FROM "EpisodeMetadata" WHERE "EpisodeId" = "item"."Id" ORDER BY "Id" LIMIT 1),
                    (SELECT "Title" FROM "MusicVideoMetadata" WHERE "MusicVideoId" = "item"."Id" ORDER BY "Id" LIMIT 1),
                    (SELECT "Title" FROM "OtherVideoMetadata" WHERE "OtherVideoId" = "item"."Id" ORDER BY "Id" LIMIT 1),
                    "file"."Path", '') AS "Title",
                  COALESCE("file"."Path", '') AS "Path",
                  CASE
                    WHEN "version"."MovieId" IS NOT NULL THEN 10
                    WHEN "version"."EpisodeId" IS NOT NULL THEN 20
                    WHEN "version"."MusicVideoId" IS NOT NULL THEN 30
                    WHEN "version"."OtherVideoId" IS NOT NULL THEN 40
                  END AS "CollectionType",
                  (SELECT MIN("stream"."Index") FROM "MediaStream" AS "stream"
                   WHERE "stream"."MediaVersionId" = "version"."Id"
                     AND "stream"."MediaStreamKind" = 1 AND "stream"."AttachedPic" = 0)
                    AS "VideoStreamIndex"
           FROM "MediaItem" AS "item"
           JOIN "LibraryPath" AS "path" ON "path"."Id" = "item"."LibraryPathId"
           JOIN "Library" AS "library" ON "library"."Id" = "path"."LibraryId"
           JOIN "LocalMediaSource" AS "local" ON "local"."Id" = "library"."MediaSourceId"
           JOIN "MediaVersion" AS "version" ON (
                "version"."MovieId" = "item"."Id"
                OR "version"."EpisodeId" = "item"."Id"
                OR "version"."MusicVideoId" = "item"."Id"
                OR "version"."OtherVideoId" = "item"."Id")
           JOIN "MediaFile" AS "file" ON "file"."MediaVersionId" = "version"."Id"
           WHERE "item"."State" = 0
             AND EXISTS (
                SELECT 1 FROM "MediaStream" AS "stream"
                WHERE "stream"."MediaVersionId" = "version"."Id"
                  AND "stream"."MediaStreamKind" = 1 AND "stream"."AttachedPic" = 0)
             AND LOWER(COALESCE(
                    (SELECT "Title" FROM "MovieMetadata" WHERE "MovieId" = "item"."Id" ORDER BY "Id" LIMIT 1),
                    (SELECT "Title" FROM "EpisodeMetadata" WHERE "EpisodeId" = "item"."Id" ORDER BY "Id" LIMIT 1),
                    (SELECT "Title" FROM "MusicVideoMetadata" WHERE "MusicVideoId" = "item"."Id" ORDER BY "Id" LIMIT 1),
                    (SELECT "Title" FROM "OtherVideoMetadata" WHERE "OtherVideoId" = "item"."Id" ORDER BY "Id" LIMIT 1),
                    "file"."Path", '')) LIKE LOWER(?)
           ORDER BY "Title", "file"."Path", "version"."Id"
           LIMIT ?"#
    } else {
        r#"SELECT "item"."Id" AS "MediaItemId", "version"."Id" AS "MediaVersionId",
                  COALESCE(
                    (SELECT "Title" FROM "MovieMetadata" WHERE "MovieId" = "item"."Id" ORDER BY "Id" LIMIT 1),
                    (SELECT "Title" FROM "EpisodeMetadata" WHERE "EpisodeId" = "item"."Id" ORDER BY "Id" LIMIT 1),
                    (SELECT "Title" FROM "MusicVideoMetadata" WHERE "MusicVideoId" = "item"."Id" ORDER BY "Id" LIMIT 1),
                    (SELECT "Title" FROM "OtherVideoMetadata" WHERE "OtherVideoId" = "item"."Id" ORDER BY "Id" LIMIT 1),
                    "file"."Path", '') AS "Title",
                  COALESCE("file"."Path", '') AS "Path",
                  CASE
                    WHEN "version"."MovieId" IS NOT NULL THEN 10
                    WHEN "version"."EpisodeId" IS NOT NULL THEN 20
                    WHEN "version"."MusicVideoId" IS NOT NULL THEN 30
                    WHEN "version"."OtherVideoId" IS NOT NULL THEN 40
                  END AS "CollectionType",
                  (SELECT MIN("stream"."Index") FROM "MediaStream" AS "stream"
                   WHERE "stream"."MediaVersionId" = "version"."Id"
                     AND "stream"."MediaStreamKind" = 1 AND "stream"."AttachedPic" = 0)
                    AS "VideoStreamIndex"
           FROM "MediaItem" AS "item"
           JOIN "LibraryPath" AS "path" ON "path"."Id" = "item"."LibraryPathId"
           JOIN "Library" AS "library" ON "library"."Id" = "path"."LibraryId"
           JOIN "LocalMediaSource" AS "local" ON "local"."Id" = "library"."MediaSourceId"
           JOIN "MediaVersion" AS "version" ON (
                "version"."MovieId" = "item"."Id"
                OR "version"."EpisodeId" = "item"."Id"
                OR "version"."MusicVideoId" = "item"."Id"
                OR "version"."OtherVideoId" = "item"."Id")
           JOIN "MediaFile" AS "file" ON "file"."MediaVersionId" = "version"."Id"
           WHERE "item"."Id" = ? AND "version"."Id" = ? AND "item"."State" = 0
             AND EXISTS (
                SELECT 1 FROM "MediaStream" AS "stream"
                WHERE "stream"."MediaVersionId" = "version"."Id"
                  AND "stream"."MediaStreamKind" = 1 AND "stream"."AttachedPic" = 0)
           LIMIT 1"#
    }
}

pub(crate) async fn load_replacement_candidate(
    pool: &Pool<crate::db::Db>,
    media_item_id: EntityId,
    media_version_id: EntityId,
) -> Result<Option<AddonMediaListReplacementCandidate>> {
    sqlx::query(replacement_candidate_query(false))
        .bind(media_item_id)
        .bind(media_version_id)
        .fetch_optional(pool)
        .await
        .repository()?
        .as_ref()
        .map(decode_replacement_candidate)
        .transpose()
}

fn decode_replacement_candidate(
    row: &crate::db::DbRow,
) -> Result<AddonMediaListReplacementCandidate> {
    Ok(AddonMediaListReplacementCandidate {
        media_item_id: row.try_get("MediaItemId").repository()?,
        media_version_id: row.try_get("MediaVersionId").repository()?,
        title: row.try_get("Title").repository()?,
        path: row.try_get("Path").repository()?,
        collection_type: row.try_get("CollectionType").repository()?,
        video_stream_index: row.try_get("VideoStreamIndex").repository()?,
    })
}

#[cfg(test)]
mod tests {
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::Executor as _;

    use super::{accept_reconcile_candidate_in_tx, is_safe_remote_artwork_url, FlatProjection};

    #[test]
    fn stale_concrete_replacement_remains_advisory() {
        let item = super::AddonMediaListItemAdminRow {
            replacement_media_item_id: Some(40),
            replacement_media_version_id: Some(41),
            replacement_valid: false,
            replacement_unresolved: false,
            availability: super::AddonMediaListItemAvailability::Available,
            ..Default::default()
        };
        let mut classification = super::PublicationItemClassification::default();
        super::classify_item_advisory_and_playback(
            super::AddonMediaListPresentationKind::TelevisionShow,
            &item,
            None,
            &mut classification,
        );
        assert_eq!(classification.recommendation_count, 1);
        assert!(classification.blockers.is_empty());
    }

    /// One list whose publication has already selected the revision it verifies.
    ///
    /// This is the state a failed publication sits in for most of its life: the
    /// restore picked `RestoredAfterFailure`, the operation holds no lease while
    /// it waits between verification attempts, and the list must stay
    /// `Publishing` for that verification to converge.
    async fn publishing_list_between_verification_attempts(
        outcome: Option<i32>,
    ) -> sqlx::Pool<crate::db::Db> {
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
                   "Id", "AddonId", "ProviderId", "SourceUrl", "Name",
                   "DraftRevision", "PublishedRevision", "ReviewState",
                   "PublicationJournalJson"
               ) VALUES (1, 'fixture.addon', 'fixture-list',
                         'https://media.example.test/list', 'Fixture list',
                         14, 0, 3, '{"previous_review_state":"draft"}')"#,
        )
        .execute(&pool)
        .await
        .expect("seed publishing list");
        sqlx::query(
            r#"INSERT INTO "ManagedLibraryOperation" (
                   "Id", "LibraryId", "OwnerToken", "Kind", "State", "AttemptCount",
                   "CreatedAt", "UpdatedAt"
               ) VALUES (100, 1, 'fixture-owner', 6, 2, 9,
                         '2026-09-07T20:52:46Z', '2026-09-07T21:01:48Z')"#,
        )
        .execute(&pool)
        .await
        .expect("seed waiting publication operation");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListPublicationJob" (
                   "Id", "OperationId", "AddonMediaListId", "Generation",
                   "SavedRevision", "TargetPublishedRevision", "Phase", "Outcome",
                   "CreatedAt", "UpdatedAt"
               ) VALUES (1, 100, 1, 1, 14, 1, 8, ?,
                         '2026-09-07T20:52:46Z', '2026-09-07T21:01:48Z')"#,
        )
        .bind(outcome)
        .execute(&pool)
        .await
        .expect("seed publication job");
        pool
    }

    async fn review_state(pool: &sqlx::Pool<crate::db::Db>) -> i32 {
        sqlx::query_scalar(r#"SELECT "ReviewState" FROM "AddonMediaList" WHERE "Id" = 1"#)
            .fetch_one(pool)
            .await
            .expect("read review state")
    }

    /// A publication between verification retries holds no lease and is not
    /// `Running`, so a lease-only guard let an ordinary edit overwrite
    /// `Publishing` with `Ready`. Verification checks the selected revision
    /// against `Publishing` and could then never converge, retrying forever.
    #[tokio::test]
    async fn edits_cannot_leave_publishing_once_a_revision_is_being_verified() {
        // 0 published, 1 restored after failure: both are selected revisions.
        for outcome in [0, 1] {
            let pool = publishing_list_between_verification_attempts(Some(outcome)).await;
            let mut tx = crate::db::begin_write(&pool).await.expect("begin");

            let error = super::mark_list_ready_tx(&mut tx, 1)
                .await
                .expect_err("a revision under verification must refuse the edit");

            assert!(
                matches!(
                    error,
                    ersatzrs_core::repositories::RepositoryError::Conflict(_)
                ),
                "outcome {outcome} produced {error:?}"
            );
            drop(tx);
            assert_eq!(review_state(&pool).await, 3);
        }
    }

    #[tokio::test]
    async fn exhausted_publication_keeps_edits_blocked_until_journal_finalization() {
        for outcome in [0, 1] {
            let pool = publishing_list_between_verification_attempts(Some(outcome)).await;
            sqlx::query(r#"UPDATE "AddonMediaListPublicationJob" SET "CompletedAt" = '2026-09-07T22:00:00Z'"#)
                .execute(&pool).await.expect("complete failed job");
            let mut tx = crate::db::begin_write(&pool).await.expect("begin");
            assert!(matches!(
                super::mark_list_ready_tx(&mut tx, 1).await,
                Err(ersatzrs_core::repositories::RepositoryError::Conflict(_))
            ));
            tx.rollback().await.expect("rollback");
            assert_eq!(review_state(&pool).await, 3);
            let synchronizer = super::AddonMediaListSynchronizer::new(pool.clone());
            assert!(matches!(
                synchronizer
                    .update_settings(
                        1,
                        ersatzrs_domain::AddonMediaListRefreshCadence::default(),
                        ersatzrs_domain::AddonMediaListRefreshPublicationPolicy::default(),
                        ersatzrs_domain::AddonMediaListDeliveryMode::default(),
                        None,
                        false,
                        false,
                        ersatzrs_domain::AddonMediaListContentKind::default(),
                        "",
                    )
                    .await,
                Err(ersatzrs_core::repositories::RepositoryError::Conflict(_))
            ));
            assert_eq!(review_state(&pool).await, 3);
            sqlx::query(r#"UPDATE "AddonMediaList" SET "PublicationJournalJson" = NULL"#)
                .execute(&pool)
                .await
                .expect("finalize journal");
            let mut tx = crate::db::begin_write(&pool).await.expect("begin");
            super::mark_list_ready_tx(&mut tx, 1)
                .await
                .expect("finalized recovery releases edits");
            tx.commit().await.expect("commit");
            assert_eq!(review_state(&pool).await, 1);
        }
    }

    /// An abandoned publication that never chose a revision must still release
    /// the draft, so an expired lease cannot lock a list forever.
    #[tokio::test]
    async fn edits_still_release_a_publication_that_never_reached_verification() {
        let pool = publishing_list_between_verification_attempts(None).await;
        // A previous generation's outcome must not lock an abandoned new draft.
        sqlx::query(
            r#"INSERT INTO "AddonMediaListPublicationJob" (
            "Id", "OperationId", "AddonMediaListId", "Generation", "SavedRevision",
            "TargetPublishedRevision", "Phase", "Outcome", "CreatedAt", "UpdatedAt", "CompletedAt"
        ) VALUES (0, 99, 1, 0, 13, 0, 10, 0,
            '2026-09-06T00:00:00Z', '2026-09-06T00:00:00Z', '2026-09-06T00:00:00Z')"#,
        )
        .execute(&pool)
        .await
        .expect("previous completed generation");
        let mut tx = crate::db::begin_write(&pool).await.expect("begin");

        super::mark_list_ready_tx(&mut tx, 1)
            .await
            .expect("an abandoned publication must not keep the draft locked");

        tx.commit().await.expect("commit");
        assert_eq!(review_state(&pool).await, 1);
    }

    // Reuse the synthetic POSIX package fixture; Windows runner acceptance is separate.
    #[cfg(unix)]
    #[tokio::test]
    async fn normal_publication_selects_interval_jobs_without_prepared_files() {
        use crate::addon_media_list_item_jobs::{
            AddonMediaListItemJobQueue, ItemOperationCompletion, ItemOperationKind,
        };
        use ersatzrs_core::{
            addon_media_list_playback::AddonMediaListPlaybackTimelinePlan,
            repositories::AddonMediaListRepository as _,
        };

        let temporary = tempfile::tempdir().unwrap();
        let pool = crate::addon_media_list_publication_preparation::tests::streaming_fixture(
            &temporary.path().join("package"),
        )
        .await;
        let repository = super::SqliteAddonMediaListRepository(pool.clone());
        assert!(
            super::select_publication_playback(&repository, 1, 0, None, &[])
                .await
                .is_err()
        );
        assert_eq!(repository.prepare_publication_media(1, 0).await.unwrap(), 1);
        let queue = AddonMediaListItemJobQueue::new(pool);
        let job = queue
            .claim_kind(chrono::Utc::now(), 90, ItemOperationKind::IntervalMetadata)
            .await
            .unwrap()
            .unwrap();
        let plan = AddonMediaListPlaybackTimelinePlan::new(
            10_000,
            0..10_000,
            &[2_000..3_000, 4_000..5_000],
            job.input.context_fingerprint.clone(),
        )
        .unwrap();
        queue
            .complete(
                &job,
                ItemOperationCompletion::IntervalPlan { plan: plan.clone() },
                chrono::Utc::now(),
            )
            .await
            .unwrap();
        let selection = super::select_publication_playback(&repository, 1, 0, None, &[])
            .await
            .unwrap();
        assert_eq!(selection.streaming_plans.get("item"), Some(&plan));
        assert!(selection.prepared_media.is_empty());
        let excluded = super::select_publication_playback(&repository, 1, 0, None, &[10])
            .await
            .unwrap();
        assert!(excluded.streaming_plans.is_empty());
        assert!(excluded.prepared_media.is_empty());
    }

    #[tokio::test]
    async fn streaming_revision_restore_freezes_source_plan_in_publication_journal() {
        use ersatzrs_core::{
            addon_media_list_playback::AddonMediaListPlaybackTimelinePlan,
            repositories::{
                AddonMediaListLifecycleRepository as _, AddonMediaListRevisionArtifacts,
            },
        };
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
        pool.execute("PRAGMA foreign_keys = OFF").await.unwrap();
        sqlx::raw_sql(r#"INSERT INTO "AddonMediaList"
                ("Id","AddonId","ProviderId","SourceUrl","Name","DraftRevision","PublishedRevision")
                VALUES (1,'fixture.addon','list','https://example.invalid/list','List',1,1);
            INSERT INTO "AddonMediaListItem"
                ("Id","AddonMediaListId","ProviderId","DisplayTitle","Title","Kind","Rank","Published","EffectiveLiveness")
                VALUES (10,1,'item','Item','Item',4,0,1,1);"#)
            .execute(&pool).await.unwrap();
        let repository = super::SqliteAddonMediaListRepository(pool.clone());
        let plan = AddonMediaListPlaybackTimelinePlan::new(
            10_000,
            0..10_000,
            &[2_000..3_000, 4_000..5_000],
            "a".repeat(64),
        )
        .unwrap();
        repository
            .retain_published_revision(
                1,
                1,
                1,
                chrono::Utc::now(),
                &[],
                &AddonMediaListRevisionArtifacts {
                    streaming_plans: [("item".into(), plan.clone())].into(),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        sqlx::raw_sql(r#"DELETE FROM "AddonMediaListItem" WHERE "Id"=10;
            INSERT INTO "AddonMediaListPublicationJob"
                ("OperationId","AddonMediaListId","Generation","SavedRevision","TargetPublishedRevision",
                 "Phase","SourcePublishedRevision","CreatedAt","UpdatedAt")
                VALUES (100,1,1,2,2,0,1,'2026-09-06T07:00:00Z','2026-09-06T07:00:00Z');"#)
            .execute(&pool).await.unwrap();
        let source = repository
            .source_snapshot_for_fence(
                1,
                super::PublicationFence {
                    operation_id: 100,
                    generation: 1,
                    saved_revision: 2,
                    owner_token: "fixture",
                    lease_owner: "fixture",
                },
            )
            .await
            .unwrap()
            .unwrap();
        let selection = super::select_publication_playback(&repository, 1, 2, Some(&source), &[])
            .await
            .unwrap();
        assert_eq!(selection.streaming_plans.get("item"), Some(&plan));
        assert!(selection.prepared_media.is_empty());
        let journal: super::PublicationJournal = serde_json::from_value(serde_json::json!({
            "previous_review_state": super::AddonMediaListReviewState::Ready,
            "previous_presentation_kind": super::AddonMediaListPresentationKind::RemoteStreams,
            "previous_items": [], "previous_playlist": null,
            "streaming_plans": selection.streaming_plans,
        }))
        .unwrap();
        let frozen = serde_json::to_string(&journal).unwrap();
        let replay: super::PublicationJournal = serde_json::from_str(&frozen).unwrap();
        assert_eq!(replay.streaming_plans.unwrap().get("item"), Some(&plan));
        let mut legacy = serde_json::to_value(&journal).unwrap();
        legacy.as_object_mut().unwrap().remove("streaming_plans");
        assert!(serde_json::from_value::<super::PublicationJournal>(legacy)
            .unwrap()
            .streaming_plans
            .is_none());
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)] // Keep the competing generations and atomic promotion assertions in one scenario.
    async fn reconcile_acceptance_atomically_promotes_the_matching_import_generation() {
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
        sqlx::raw_sql(
            r#"INSERT INTO "AddonMediaList" (
                   "Id", "AddonId", "ProviderId", "SourceUrl", "Name", "ItemCount"
               ) VALUES (1, 'fixture.addon', 'fixture-list',
                         'https://media.example.test/list', 'Fixture list', 3);
               INSERT INTO "AddonMediaListItem" (
                   "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind",
                   "Rank", "Title"
               ) VALUES
                   (20, 1, 'missing-item', 'Missing item', 4, 0, 'Missing item'),
                   (21, 1, 'returned-item', 'Returned item', 4, 1, 'Returned item'),
                   (22, 1, 'pending-item', 'Pending item', 4, 2, 'Pending item');
               INSERT INTO "AddonMediaListImportCheckpoint" (
                   "Id", "AddonMediaListId", "RelativePath", "Generation", "Sha256",
                   "EntryCount", "PromotedAt", "Committed"
               ) VALUES
                   (30, 1, 'checkpoints/current.txt', 6, 'current', 1,
                    '2026-09-04T10:00:00Z', 1),
                   (31, 1, 'checkpoints/reconcile.txt', 7, 'candidate', 0,
                    '2026-09-04T11:00:00Z', 0);"#,
        )
        .execute(&pool)
        .await
        .expect("seed list and checkpoints");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListImportJob" (
                   "Id", "AddonMediaListId", "Generation", "State", "RecordCapability",
                   "ImportCapability", "Purpose", "Mode", "CheckpointId", "CreatedAt", "UpdatedAt"
               ) VALUES (40, 1, 6, ?, 'media-list.record.v1', 'media-list.import.v2', ?, ?, 30,
                         '2026-09-04T10:00:00Z', '2026-09-04T10:00:00Z'),
                        (41, 1, 7, ?, 'media-list.record.v1', 'media-list.import.v2', ?, ?, 31,
                         '2026-09-04T11:00:00Z', '2026-09-04T11:00:00Z')"#,
        )
        .bind(crate::repositories::common::encode_enum(
            ersatzrs_domain::AddonMediaListImportState::Ready,
        ))
        .bind(crate::repositories::common::encode_enum(
            ersatzrs_domain::AddonMediaListImportPurpose::Refresh,
        ))
        .bind(crate::repositories::common::encode_enum(
            ersatzrs_domain::AddonMediaListImportMode::Incremental,
        ))
        .bind(crate::repositories::common::encode_enum(
            ersatzrs_domain::AddonMediaListImportState::Ready,
        ))
        .bind(crate::repositories::common::encode_enum(
            ersatzrs_domain::AddonMediaListImportPurpose::Reconcile,
        ))
        .bind(crate::repositories::common::encode_enum(
            ersatzrs_domain::AddonMediaListImportMode::Full,
        ))
        .execute(&pool)
        .await
        .expect("seed import generations");
        sqlx::query(
            r#"INSERT INTO "ManagedLibraryOperation" (
                   "Id", "LibraryId", "OwnerToken", "Kind", "State", "CreatedAt", "UpdatedAt"
               ) VALUES (50, 99, 'owner', ?, ?, '2026-09-04T11:00:00Z', '2026-09-04T11:00:00Z')"#,
        )
        .bind(crate::repositories::common::encode_enum(
            ersatzrs_domain::ManagedLibraryOperationKind::Reconcile,
        ))
        .bind(crate::repositories::common::encode_enum(
            ersatzrs_domain::ManagedLibraryOperationState::Completed,
        ))
        .execute(&pool)
        .await
        .expect("seed reconcile operation");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListLifecycleJob" (
                   "Id", "OperationId", "AddonMediaListId", "Generation", "Kind", "Phase",
                   "SavedDraftRevision", "SavedPublishedRevision", "CandidateImportJobId",
                   "CreatedAt", "UpdatedAt", "CompletedAt"
               ) VALUES (60, 50, 1, 2, ?, ?, 3, 1, 41,
                         '2026-09-04T11:00:00Z', '2026-09-04T11:00:00Z',
                         '2026-09-04T11:00:00Z')"#,
        )
        .bind(crate::repositories::common::encode_enum(
            ersatzrs_domain::AddonMediaListLifecycleJobKind::Reconcile,
        ))
        .bind(crate::repositories::common::encode_enum(
            ersatzrs_domain::AddonMediaListLifecyclePhase::Complete,
        ))
        .execute(&pool)
        .await
        .expect("seed completed reconcile");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListItemTombstone" (
                   "AddonMediaListItemId", "MissingSince", "LastObservedAt", "ObservationCount",
                   "LastReconcileGeneration", "ConfirmedAt", "ReappearedAt"
               ) VALUES
                   (20, '2026-09-03T10:00:00Z', '2026-09-04T11:00:00Z', 2, 7,
                    '2026-09-04T11:00:00Z', NULL),
                   (21, '2026-09-02T10:00:00Z', '2026-09-03T11:00:00Z', 1, 7,
                    NULL, '2026-09-04T11:00:00Z'),
                   (22, '2026-09-04T11:00:00Z', '2026-09-04T11:00:00Z', 1, 7,
                    NULL, NULL)"#,
        )
        .execute(&pool)
        .await
        .expect("seed confirmed absence");

        let mut rolled_back = crate::db::begin_write(&pool).await.expect("begin rollback");
        accept_reconcile_candidate_in_tx(&mut rolled_back, 1, "2026-09-04T12:00:00Z")
            .await
            .expect("stage reconcile acceptance");
        rolled_back.rollback().await.expect("rollback acceptance");
        let before: (i64, i64, i64) = sqlx::query_as(
            r#"SELECT
                   (SELECT "Committed" FROM "AddonMediaListImportCheckpoint" WHERE "Id" = 30),
                   (SELECT "Committed" FROM "AddonMediaListImportCheckpoint" WHERE "Id" = 31),
                   (SELECT COUNT(*) FROM "AddonMediaListItem" WHERE "Id" = 20)"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load rolled-back state");
        assert_eq!(before, (1, 0, 1));

        sqlx::query(
            r#"UPDATE "AddonMediaListImportJob" SET "CheckpointId" = NULL WHERE "Id" = 41"#,
        )
        .execute(&pool)
        .await
        .expect("detach stale candidate archive");
        let mut stale = crate::db::begin_write(&pool)
            .await
            .expect("begin stale acceptance");
        let error = accept_reconcile_candidate_in_tx(&mut stale, 1, "2026-09-04T12:00:00Z")
            .await
            .expect_err("a detached candidate must not publish silently");
        assert!(matches!(
            error,
            ersatzrs_core::repositories::RepositoryError::Conflict(_)
        ));
        stale.rollback().await.expect("rollback stale acceptance");
        sqlx::query(r#"UPDATE "AddonMediaListImportJob" SET "CheckpointId" = 31 WHERE "Id" = 41"#)
            .execute(&pool)
            .await
            .expect("restore candidate archive");

        let mut committed = crate::db::begin_write(&pool)
            .await
            .expect("begin acceptance");
        accept_reconcile_candidate_in_tx(&mut committed, 1, "2026-09-04T12:00:00Z")
            .await
            .expect("accept reconcile candidate");
        committed.commit().await.expect("commit acceptance");
        let after: (i64, i64, Option<i64>, i64, i64, i64, i64, i64, Option<i64>) = sqlx::query_as(
            r#"SELECT
                   (SELECT "Committed" FROM "AddonMediaListImportCheckpoint" WHERE "Id" = 30),
                   (SELECT "Committed" FROM "AddonMediaListImportCheckpoint" WHERE "Id" = 31),
                   (SELECT "CheckpointId" FROM "AddonMediaListImportJob" WHERE "Id" = 40),
                   (SELECT COUNT(*) FROM "AddonMediaListItem" WHERE "Id" = 20),
                   (SELECT COUNT(*) FROM "AddonMediaListItem" WHERE "Id" = 21),
                   (SELECT COUNT(*) FROM "AddonMediaListItem" WHERE "Id" = 22),
                   (SELECT COUNT(*) FROM "AddonMediaListItemTombstone"
                    WHERE "AddonMediaListItemId" = 21),
                   (SELECT "ItemCount" FROM "AddonMediaList" WHERE "Id" = 1),
                   (SELECT "CandidateImportJobId" FROM "AddonMediaListLifecycleJob"
                    WHERE "Id" = 60)"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load committed state");
        assert_eq!(after, (0, 1, None, 0, 1, 1, 0, 2, None));

        sqlx::query(
            r#"UPDATE "AddonMediaListItemTombstone"
               SET "ConfirmedAt" = '2026-09-04T12:30:00Z'
               WHERE "AddonMediaListItemId" = 22"#,
        )
        .execute(&pool)
        .await
        .expect("confirm the pending removal after its archive was accepted");
        let mut later_publication = crate::db::begin_write(&pool)
            .await
            .expect("begin later publication acceptance");
        accept_reconcile_candidate_in_tx(&mut later_publication, 1, "2026-09-04T13:00:00Z")
            .await
            .expect("publish a confirmed removal without another reconcile archive");
        later_publication
            .commit()
            .await
            .expect("commit later removal acceptance");
        let later: (i64, i64) = sqlx::query_as(
            r#"SELECT
                   (SELECT COUNT(*) FROM "AddonMediaListItem" WHERE "Id" = 22),
                   (SELECT "ItemCount" FROM "AddonMediaList" WHERE "Id" = 1)"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load later removal state");
        assert_eq!(later, (0, 1));
    }

    /// Every classification must map to a root the NFO serializer accepts.
    /// `Song` once mapped to an invented `song` root, so saving the NFO of any
    /// song-classified entry failed with "unsupported NFO root".
    #[test]
    fn every_projection_uses_a_serializable_nfo_root() {
        let roots = FlatProjection::ALL
            .into_iter()
            .map(|projection| projection.nfo_root)
            .chain(["tvshow", "episodedetails", "musicvideo"]);

        for root in roots {
            crate::addon_media_nfo::serialize(
                root,
                &ersatzrs_core::repositories::AddonMediaListNfoMetadata {
                    title: "Fixture".to_owned(),
                    ..Default::default()
                },
            )
            .unwrap_or_else(|error| panic!("projection uses unusable root {root}: {error}"));
        }
    }

    #[test]
    fn artwork_urls_require_bounded_https_without_credentials() {
        assert!(is_safe_remote_artwork_url(
            "https://media.example.test/poster.jpg"
        ));
        assert!(!is_safe_remote_artwork_url(
            "http://media.example.test/poster.jpg"
        ));
        assert!(!is_safe_remote_artwork_url(
            "https://user:secret@media.example.test/poster.jpg"
        ));
        assert!(!is_safe_remote_artwork_url("file:///tmp/poster.jpg"));
        assert!(!is_safe_remote_artwork_url(&format!(
            "https://media.example.test/{}",
            "x".repeat(2_048)
        )));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn windows_publication_restore_retries_until_a_held_handle_drains() {
        use std::fs::{self, OpenOptions};
        use std::os::windows::fs::OpenOptionsExt as _;
        use std::time::Duration;

        use super::{PublicationFsIdentity, RepositoryError};

        const FILE_SHARE_READ: u32 = 0x0000_0001;
        let temp = tempfile::tempdir().expect("publication recovery fixture");
        let managed = temp.path().join("managed");
        fs::create_dir(&managed).expect("managed directory");
        fs::write(managed.join("item.yml"), b"previous").expect("previous item");
        let identity = PublicationFsIdentity {
            list_id: 7,
            operation_id: Some(11),
            generation: Some(2),
            previous_revision: 3,
            target_revision: 4,
        };
        let paths = super::addon_publication_fs::publication_paths(
            managed,
            identity.list_id,
            identity.operation_id,
            identity.generation,
        )
        .expect("publication paths");
        super::addon_publication_fs::create_backup(&paths, identity)
            .expect("create publication backup");
        fs::write(paths.managed.join("item.yml"), b"candidate").expect("candidate item");
        let held = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(paths.managed.join("item.yml"))
            .expect("held item without delete sharing");
        let boundary_error = super::publication_fs_repository_error(
            &super::addon_publication_fs::restore_step(&paths, identity)
                .expect_err("held handle blocks the restore boundary"),
        );
        let boundary_cause = boundary_error
            .publication_failure_cause()
            .expect("held handle retains a typed publication cause");
        assert_eq!(
            boundary_cause.category,
            ersatzrs_domain::AddonMediaListPublicationErrorCategory::SharingViolation
        );
        assert!(boundary_cause.os_error_code.is_some());
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(180));
            drop(held);
        });

        super::restore_publication_tree_with_retry(paths.clone(), identity, 11, || async {
            Ok::<(), RepositoryError>(())
        })
        .await
        .expect("bounded sharing retry restores the previous tree");
        release.join().expect("release held handle");
        assert_eq!(
            fs::read(paths.managed.join("item.yml")).expect("restored item"),
            b"previous"
        );
    }
}
