use async_trait::async_trait;
use ersatzrs_domain::{
    AddonMediaListContentKind, AddonMediaListDetailPolicy, AddonMediaListDetailState,
    AddonMediaListImportState, AddonMediaListItemAvailability, AddonMediaListItemRole,
    AddonMediaListPresentationKind, AddonMediaListReviewState, EntityId, RemoteStreamLiveness,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::{RepositoryError, Result};

/// Provider-neutral row displayed by the add-on media-list manager.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListAdminRow {
    /// Persisted host identity.
    pub id: EntityId,
    /// Installed provider add-on identity.
    pub addon_id: String,
    /// Stable provider list identity.
    pub provider_id: String,
    /// Provider source hyperlink used for refresh.
    pub source_url: String,
    /// Human-readable provider list name.
    pub name: String,
    /// Number of persisted provider items.
    pub item_count: i32,
    /// Number of items matched to local media.
    pub match_count: i32,
    /// Whether daily provider refresh is enabled.
    pub auto_refresh: bool,
    /// Whether a host-owned generated playlist is maintained.
    pub generate_playlist: bool,
    /// Last successful provider refresh timestamp.
    pub last_update: Option<String>,
    /// Last successful local matching timestamp.
    pub last_match: Option<String>,
    /// Last provider refresh failure, if any.
    pub last_error: Option<String>,
    /// Active library presentation.
    pub presentation_kind: AddonMediaListPresentationKind,
    /// Presentation requested by the administrator; may differ while a scan is pending.
    pub requested_presentation_kind: AddonMediaListPresentationKind,
    /// Last projection failure, if conversion has not completed.
    pub presentation_error: Option<String>,
    /// Review lifecycle; new imports remain drafts until explicitly published.
    pub review_state: AddonMediaListReviewState,
    /// Last publication validation or projection failure, if any.
    pub review_error: Option<String>,
    /// Monotonic publication revision used by refresh and audit flows.
    pub published_revision: i32,
    /// Whether post-publication detail work is automatic or manual-only.
    pub detail_import_policy: AddonMediaListDetailPolicy,
    /// Current durable overview/detail activity, if this list uses import v1.
    pub import_progress: Option<AddonMediaListImportProgress>,
}

/// Editable settings for one persisted add-on media list.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListAdminForm {
    /// Persisted host identity.
    pub id: EntityId,
    /// Installed provider add-on identity.
    pub addon_id: String,
    /// Provider source hyperlink.
    pub source_url: String,
    /// Safe host-managed library folder below the configured add-on root.
    pub folder_name: String,
    /// Whether daily provider refresh is enabled.
    pub auto_refresh: bool,
    /// Whether a host-owned generated playlist is maintained.
    pub generate_playlist: bool,
    /// Optional list-wide content classification; `Auto` keeps each
    /// provider-supplied item classification.
    pub content_kind_override: AddonMediaListContentKind,
    /// Active library presentation.
    pub presentation_kind: AddonMediaListPresentationKind,
    /// Requested library presentation.
    pub requested_presentation_kind: AddonMediaListPresentationKind,
    /// Projection error retained while active and requested kinds differ.
    pub presentation_error: Option<String>,
    pub review_state: AddonMediaListReviewState,
    pub review_error: Option<String>,
    pub published_revision: i32,
    pub detail_import_policy: AddonMediaListDetailPolicy,
    pub import_progress: Option<AddonMediaListImportProgress>,
    /// Legacy list-wide chapter input retained only for ambiguous migrations.
    pub legacy_chapter_input: String,
}

/// One persisted provider item and its optional local replacement.
///
/// These booleans are independent projections for replacement validity,
/// liveness conflict, inclusion, and publication; they do not encode one state
/// machine and are consumed independently by the admin read model.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListItemAdminRow {
    pub id: EntityId,
    pub rank: i32,
    pub kind: ersatzrs_domain::AddonMediaListItemKind,
    pub display_title: String,
    pub availability: AddonMediaListItemAvailability,
    pub availability_reason: Option<String>,
    pub replacement_media_item_id: Option<EntityId>,
    pub replacement_media_version_id: Option<EntityId>,
    pub replacement_title: Option<String>,
    pub replacement_path: Option<String>,
    pub replacement_valid: bool,
    pub title: String,
    pub year: Option<i32>,
    pub season: Option<i32>,
    pub episode: Option<i32>,
    /// Effective provider/operator release date shown as the first-air date.
    pub first_aired: Option<String>,
    pub source_url: Option<String>,
    pub duration_seconds: Option<u64>,
    /// Latest provider-owned playback behavior.
    pub provider_liveness: RemoteStreamLiveness,
    /// Optional operator decision; when present it owns the effective value.
    pub liveness_override: Option<RemoteStreamLiveness>,
    /// Current value that will be copied into the next published snapshot.
    pub effective_liveness: RemoteStreamLiveness,
    /// Whether the provider changed beneath an operator override.
    pub liveness_conflict: bool,
    /// Value used by the currently published library projection.
    pub published_liveness: RemoteStreamLiveness,
    pub parent_provider_id: Option<String>,
    pub chapter_input: String,
    /// Latest provider value, retained separately from the operator override.
    pub provider_chapter_input: String,
    pub projected_episode_id: Option<EntityId>,
    /// Descriptive classification of this entry.
    ///
    /// The list-wide value is only the default applied to new entries; this is
    /// the effective kind for the item.
    pub content_kind: AddonMediaListContentKind,
    /// Primary/extra classification used by collection-aware publication.
    pub collection_role: AddonMediaListItemRole,
    /// First effective provider/operator artwork candidate for compact review.
    pub thumbnail_url: Option<String>,
    /// Effective tags shown as a bounded summary without opening the editor.
    pub tags: Vec<String>,
    /// Effective credited people shown as a bounded summary in the overview.
    pub people: Vec<String>,
    /// Operator-controlled inclusion; defaults to true.
    pub included: bool,
    /// Whether this entry belongs to the currently published revision.
    pub published: bool,
    /// Number of operator-owned fields whose provider value changed on refresh.
    pub metadata_conflict_count: u32,
    /// Durable state of optional provider detail enrichment.
    pub detail_state: AddonMediaListDetailState,
    /// Bounded provider-neutral diagnostic retained after retry/failure.
    pub detail_diagnostic: Option<String>,
}

/// Exact operator-owned fields written by one quick review-table edit.
///
/// Keeping the complete small snapshot makes the same atomic operation usable
/// for inline edits, bounded bulk edits, episode numbering, and one-step undo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListItemReviewEdit {
    pub item_id: EntityId,
    pub content_kind: AddonMediaListContentKind,
    pub included: bool,
    pub liveness_override: Option<RemoteStreamLiveness>,
    pub season: Option<i32>,
    pub episode: Option<i32>,
}

/// Durable progress reconstructed from overview and item-detail job tables.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListImportProgress {
    pub state: AddonMediaListImportState,
    pub started_at: String,
    pub discovered_count: u64,
    pub total_hint: Option<u64>,
    pub detail_total: u64,
    pub detail_complete: u64,
    pub detail_skipped: u64,
    pub detail_failed: u64,
    pub detail_pending: u64,
    pub current_title: Option<String>,
    pub last_progress_at: Option<String>,
    pub diagnostic: Option<String>,
}

/// Stable, localizable publication blockers returned by the authoritative
/// add-on-list readiness projection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListPublicationBlockerKind {
    OverviewIncomplete,
    PublicationInProgress,
    RecoveryRequired,
    CollectionTitleMissing,
    InvalidArtwork,
    NoIncludedItems,
    MoviePrimaryCount,
    PlaybackDecisionRequired,
    PlaybackDetailsUnavailable,
    PlaybackDetailsFailed,
    UnavailableWithoutReplacement,
    ItemTitleMissing,
    SeasonEpisodeMissing,
    ArtistMissing,
}

/// One publication blocker with bounded context for localized admin copy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListPublicationBlocker {
    pub kind: AddonMediaListPublicationBlockerKind,
    pub item_rank: Option<i32>,
    pub item_title: Option<String>,
    pub count: u64,
}

/// Application-facing readiness state for one saved add-on-list revision.
///
/// Recommendations and optional detail progress are deliberately separate
/// from `blockers`: ADR 0079 permits a verified overview to publish before
/// optional enrichment completes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListPublicationReadiness {
    pub included_count: u64,
    pub blockers: Vec<AddonMediaListPublicationBlocker>,
    pub recommendation_count: u64,
    pub required_playback_detail_count: u64,
    pub required_playback_detail_pending: u64,
    pub optional_detail_total: u64,
    pub optional_detail_complete: u64,
    pub optional_detail_pending: u64,
    pub saved_revision: i32,
    pub review_state: AddonMediaListReviewState,
}

impl AddonMediaListPublicationReadiness {
    #[must_use]
    pub fn can_start_publication(&self) -> bool {
        self.blockers.is_empty()
            && self.required_playback_detail_count == 0
            && self.review_state != AddonMediaListReviewState::Publishing
    }

    #[must_use]
    pub fn needs_playback_preparation(&self) -> bool {
        self.blockers.is_empty() && self.required_playback_detail_count > 0
    }
}

/// Operator action for one durable overview generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListImportAction {
    Pause,
    Resume,
    Retry,
    Cancel,
}

/// How many entries of one list fall into each availability group.
///
/// The editor renders one section per group and needs the totals before it
/// loads any page of rows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListItemCounts {
    pub available: u64,
    pub unavailable: u64,
    pub unknown: u64,
}

/// One person row persisted in a Kodi-compatible NFO document.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListNfoPerson {
    pub name: String,
    pub role: Option<String>,
    pub order: Option<i32>,
    pub thumb: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListArtworkRole {
    Poster,
    Fanart,
    Thumb,
    #[default]
    Other,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListArtwork {
    pub url: String,
    pub role: AddonMediaListArtworkRole,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub language: Option<String>,
}

/// Stored operator artwork returned to a transport adapter for previewing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AddonMediaListArtworkUpload {
    pub content_type: String,
    pub bytes: Vec<u8>,
}

/// One provider refresh that changed a field beneath an operator override.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListMetadataConflict {
    pub provider_before: serde_json::Value,
    pub provider_after: serde_json::Value,
    pub operator_value: serde_json::Value,
}

/// Complete descriptive metadata supported by managed add-on-list NFO files.
///
/// Operational identity, source URL, availability, and runtime deliberately
/// remain outside this structure because YAML/provider state owns them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AddonMediaListNfoMetadata {
    pub title: String,
    pub original_title: Option<String>,
    pub sort_title: Option<String>,
    pub outline: Option<String>,
    pub plot: Option<String>,
    pub tagline: Option<String>,
    pub show_title: Option<String>,
    pub season: Option<i32>,
    pub episode: Option<i32>,
    pub year: Option<i32>,
    pub release_date: Option<String>,
    pub content_ratings: Vec<String>,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub studios: Vec<String>,
    pub music_labels: Vec<String>,
    pub languages: Vec<String>,
    pub people: Vec<AddonMediaListNfoPerson>,
    pub writers: Vec<String>,
    pub directors: Vec<String>,
    pub artists: Vec<String>,
    pub producers: Vec<String>,
    pub original_broadcasters: Vec<String>,
    pub broadcasters: Vec<String>,
    pub collection: Option<String>,
    /// Role-aware candidates supplied through the shared v4 contract.
    pub artwork: Vec<AddonMediaListArtwork>,
    /// Legacy untyped artwork URLs retained for v1-v3 compatibility.
    pub artwork_urls: Vec<String>,
    pub guids: Vec<String>,
}

/// Managed NFO ownership shown in administration and health output.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListNfoOwnership {
    #[default]
    Generated,
    User,
    Invalid,
}

/// One NFO-backed metadata form with optimistic-concurrency state.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListNfoForm {
    pub metadata: AddonMediaListNfoMetadata,
    pub etag: String,
    pub ownership: AddonMediaListNfoOwnership,
    pub error: Option<String>,
    pub path_available: bool,
    /// Field-keyed refresh conflicts retained until the operator saves or restores metadata.
    pub conflicts: BTreeMap<String, AddonMediaListMetadataConflict>,
}

/// Staged publication returned to the application coordinator.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListPublication {
    pub list_id: EntityId,
    pub library_id: EntityId,
}

/// Complete administration detail for one provider or chapter-fragment item.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListItemDetail {
    pub item: AddonMediaListItemAdminRow,
    pub nfo: AddonMediaListNfoForm,
    pub chapters: Vec<AddonMediaListItemAdminRow>,
}

/// One eligible, already-scanned local video version.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListReplacementCandidate {
    pub media_item_id: EntityId,
    pub media_version_id: EntityId,
    pub title: String,
    pub path: String,
    /// Concrete Local collection type used by scheduling.
    pub collection_type: i32,
    /// First eligible non-attached video stream for the exact version.
    pub video_stream_index: i32,
}

/// Persistence and synchronization boundary for add-on media-list administration.
#[async_trait]
pub trait AddonMediaListRepository: Send + Sync {
    /// Return one pageable list of administrative rows and its total count.
    async fn list_admin(
        &self,
        page: u32,
        page_size: u32,
    ) -> Result<(Vec<AddonMediaListAdminRow>, u64)>;
    /// Load the editable settings for one list.
    async fn get_admin_form(&self, id: EntityId) -> Result<Option<AddonMediaListAdminForm>>;
    /// List provider items and replacement state in provider order.
    async fn list_items(&self, id: EntityId) -> Result<Vec<AddonMediaListItemAdminRow>>;
    /// Classify the exact saved revision before publication without mutating
    /// import, detail, projection, or library state.
    async fn publication_readiness(
        &self,
        id: EntityId,
    ) -> Result<AddonMediaListPublicationReadiness> {
        let _ = id;
        Err(RepositoryError::unavailable(
            "add-on publication readiness",
            "not implemented",
        ))
    }
    /// Load one item, its NFO metadata, and derived chapter children.
    async fn get_item_detail(
        &self,
        list_id: EntityId,
        item_id: EntityId,
    ) -> Result<Option<AddonMediaListItemDetail>> {
        let _ = (list_id, item_id);
        Err(RepositoryError::unavailable(
            "add-on item detail",
            "not implemented",
        ))
    }
    /// Load list-level television-show NFO metadata.
    async fn get_show_nfo(&self, id: EntityId) -> Result<AddonMediaListNfoForm> {
        let _ = id;
        Err(RepositoryError::unavailable(
            "add-on show NFO",
            "not implemented",
        ))
    }
    /// Search eligible local video versions for the replacement picker.
    async fn search_replacement_candidates(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<AddonMediaListReplacementCandidate>>;
    /// Set or clear the local replacement for one provider item.
    async fn set_replacement(
        &self,
        item_id: EntityId,
        replacement: Option<(EntityId, EntityId)>,
    ) -> Result<()>;
    /// Import or refresh a provider list and rerun local matching.
    async fn add_or_refresh(
        &self,
        addon_id: &str,
        source_url: &str,
        requested_folder: Option<&str>,
        content_kind_override: AddonMediaListContentKind,
        chapter_input: &str,
    ) -> Result<EntityId>;
    /// Start a resumable import when supported, preserving the legacy bounded
    /// one-shot path for older packages.
    async fn add_or_refresh_with_policy(
        &self,
        addon_id: &str,
        source_url: &str,
        requested_folder: Option<&str>,
        content_kind_override: AddonMediaListContentKind,
        chapter_input: &str,
        detail_import_policy: AddonMediaListDetailPolicy,
    ) -> Result<EntityId> {
        let id = self
            .add_or_refresh(
                addon_id,
                source_url,
                requested_folder,
                content_kind_override,
                chapter_input,
            )
            .await?;
        self.set_detail_import_policy(id, detail_import_policy)
            .await?;
        Ok(id)
    }
    /// Change automatic/manual-only behavior without discarding completed data.
    async fn set_detail_import_policy(
        &self,
        id: EntityId,
        policy: AddonMediaListDetailPolicy,
    ) -> Result<()> {
        let _ = (id, policy);
        Err(RepositoryError::unavailable(
            "set add-on detail-import policy",
            "not implemented",
        ))
    }
    /// Queue or retry detail enrichment for one top-level provider item.
    async fn request_item_details(&self, list_id: EntityId, item_id: EntityId) -> Result<()> {
        let _ = (list_id, item_id);
        Err(RepositoryError::unavailable(
            "request add-on item details",
            "not implemented",
        ))
    }
    /// Pause, resume, retry, or cancel the current overview generation.
    async fn control_import(&self, id: EntityId, action: AddonMediaListImportAction) -> Result<()> {
        let _ = (id, action);
        Err(RepositoryError::unavailable(
            "control add-on media-list import",
            "not implemented",
        ))
    }
    /// Return the managed Local library projected for a provider list.
    async fn managed_library_id(&self, id: EntityId) -> Result<Option<EntityId>>;
    /// Rerun local matching without invoking the provider.
    async fn match_items(&self, id: EntityId) -> Result<()>;
    /// Queue durable detail work required to make the included playback
    /// snapshot publishable, returning the number of items still being
    /// prepared. A zero count means publication can proceed immediately.
    async fn prepare_publication_details(&self, id: EntityId) -> Result<u64> {
        let _ = id;
        Ok(0)
    }
    /// Stage the included draft for managed file generation.
    async fn stage_publication(&self, id: EntityId) -> Result<AddonMediaListPublication> {
        let _ = id;
        Err(RepositoryError::unavailable(
            "stage add-on media-list publication",
            "not implemented",
        ))
    }
    /// Commit one staged revision after its managed library has been scanned.
    async fn commit_staged_publication(&self, id: EntityId) -> Result<()> {
        let _ = id;
        Err(RepositoryError::unavailable(
            "commit add-on media-list publication",
            "not implemented",
        ))
    }
    /// Restore the last published item snapshot after staging or commit failed.
    async fn restore_staged_publication(&self, id: EntityId, message: &str) -> Result<()> {
        let _ = (id, message);
        Err(RepositoryError::unavailable(
            "restore add-on media-list publication",
            "not implemented",
        ))
    }
    /// Rebuild the prior database projection after its managed files were restored.
    async fn reconcile_restored_publication(&self, id: EntityId) -> Result<()> {
        let _ = id;
        Err(RepositoryError::unavailable(
            "reconcile restored add-on media-list publication",
            "not implemented",
        ))
    }
    /// Publish directly when managed files already exist (mainly repository tests).
    ///
    /// The native application service uses the staged methods and runs the
    /// scanner between them, which is required for a first import.
    async fn publish(&self, id: EntityId) -> Result<()> {
        self.stage_publication(id).await?;
        match self.commit_staged_publication(id).await {
            Ok(()) => Ok(()),
            Err(error) => {
                let message = error.to_string();
                self.restore_staged_publication(id, &message).await?;
                self.reconcile_restored_publication(id).await?;
                Err(error)
            }
        }
    }
    /// Store one bounded operator image and select it for a list or item role.
    async fn upload_artwork(
        &self,
        list_id: EntityId,
        item_id: Option<EntityId>,
        role: AddonMediaListArtworkRole,
        content_type: &str,
        bytes: &[u8],
        expected_etag: &str,
    ) -> Result<AddonMediaListNfoForm> {
        let _ = (list_id, item_id, role, content_type, bytes, expected_etag);
        Err(RepositoryError::unavailable(
            "upload add-on metadata artwork",
            "not implemented",
        ))
    }
    /// Load one opaque, database-backed artwork upload for an editor preview.
    async fn get_artwork_upload(
        &self,
        upload_id: EntityId,
    ) -> Result<Option<AddonMediaListArtworkUpload>> {
        let _ = upload_id;
        Err(RepositoryError::unavailable(
            "load add-on metadata artwork",
            "not implemented",
        ))
    }
    /// Include or exclude one item from the next and current publication.
    async fn set_item_included(
        &self,
        list_id: EntityId,
        item_id: EntityId,
        included: bool,
    ) -> Result<()> {
        let _ = (list_id, item_id, included);
        Err(RepositoryError::unavailable(
            "set add-on item inclusion",
            "not implemented",
        ))
    }
    /// Set the semantic role of one source without changing provider order.
    async fn set_item_collection_role(
        &self,
        list_id: EntityId,
        item_id: EntityId,
        role: AddonMediaListItemRole,
    ) -> Result<()> {
        let _ = (list_id, item_id, role);
        Err(RepositoryError::unavailable(
            "set add-on item collection role",
            "not implemented",
        ))
    }
    /// Return one page of entries in one availability group.
    ///
    /// `page_size` of `None` returns every row in the group, which the editor
    /// offers explicitly; callers that page must pass a bounded size.
    async fn list_items_page(
        &self,
        id: EntityId,
        availability: AddonMediaListItemAvailability,
        page: u32,
        page_size: Option<u32>,
    ) -> Result<Vec<AddonMediaListItemAdminRow>>;
    /// Count the entries of one list per availability group.
    async fn item_counts(&self, id: EntityId) -> Result<AddonMediaListItemCounts>;
    /// Set the descriptive classification of one entry.
    async fn update_item_content_kind(
        &self,
        item_id: EntityId,
        content_kind: AddonMediaListContentKind,
    ) -> Result<()>;
    /// Set or clear one Remote Stream playback-behavior override.
    async fn set_item_liveness_override(
        &self,
        list_id: EntityId,
        item_id: EntityId,
        liveness: Option<RemoteStreamLiveness>,
    ) -> Result<()> {
        let _ = (list_id, item_id, liveness);
        Err(RepositoryError::unavailable(
            "set add-on item liveness",
            "not implemented",
        ))
    }
    /// Apply or clear one override for a bounded set of Remote Stream items.
    /// An empty ID set means every top-level Remote Stream item in the list.
    async fn apply_list_liveness_override(
        &self,
        id: EntityId,
        liveness: Option<RemoteStreamLiveness>,
        item_ids: &[EntityId],
    ) -> Result<u64> {
        let _ = (id, liveness, item_ids);
        Err(RepositoryError::unavailable(
            "apply add-on item liveness",
            "not implemented",
        ))
    }
    /// Apply one classification to every entry of a list as a bulk default.
    async fn apply_list_content_kind(
        &self,
        id: EntityId,
        content_kind: AddonMediaListContentKind,
    ) -> Result<u64>;
    /// Atomically apply exact operator-owned review-table snapshots.
    async fn apply_item_review_edits(
        &self,
        id: EntityId,
        edits: &[AddonMediaListItemReviewEdit],
    ) -> Result<u64> {
        let _ = (id, edits);
        Err(RepositoryError::unavailable(
            "apply add-on item review edits",
            "not implemented",
        ))
    }
    /// Assign one season and consecutive episode numbers in provider order.
    ///
    /// Only included top-level entries participate. When `missing_only` is
    /// true, entries that already have both a valid season and episode number
    /// are skipped without consuming a number.
    async fn assign_episode_numbers(
        &self,
        id: EntityId,
        season: i32,
        starting_episode: i32,
        item_count: Option<u32>,
        missing_only: bool,
    ) -> Result<u64> {
        let _ = (id, season, starting_episode, item_count, missing_only);
        Err(RepositoryError::unavailable(
            "assign add-on episode numbers",
            "not implemented",
        ))
    }
    /// Move one list to a different add-on and re-import it through that
    /// add-on.
    ///
    /// Item identity is provider-scoped, so items enumerated by the new add-on
    /// replace the previous ones rather than being migrated.
    async fn change_addon(&self, id: EntityId, addon_id: &str) -> Result<()>;
    /// Update refresh and generated-playlist settings.
    async fn update_settings(
        &self,
        id: EntityId,
        auto_refresh: bool,
        generate_playlist: bool,
        content_kind_override: AddonMediaListContentKind,
        chapter_input: &str,
    ) -> Result<()>;
    /// Request a library-presentation conversion.
    async fn request_presentation(
        &self,
        id: EntityId,
        presentation_kind: AddonMediaListPresentationKind,
        show_title: Option<&str>,
    ) -> Result<()> {
        let _ = (id, presentation_kind, show_title);
        Err(RepositoryError::unavailable(
            "add-on list presentation",
            "not implemented",
        ))
    }
    /// Persist list-level descriptive metadata to `tvshow.nfo`.
    async fn save_show_nfo(
        &self,
        id: EntityId,
        metadata: &AddonMediaListNfoMetadata,
        expected_etag: &str,
    ) -> Result<()> {
        let _ = (id, metadata, expected_etag);
        Err(RepositoryError::unavailable(
            "add-on show NFO",
            "not implemented",
        ))
    }
    /// Persist item descriptive metadata to its same-stem NFO.
    async fn save_item_nfo(
        &self,
        list_id: EntityId,
        item_id: EntityId,
        metadata: &AddonMediaListNfoMetadata,
        expected_etag: &str,
    ) -> Result<()> {
        let _ = (list_id, item_id, metadata, expected_etag);
        Err(RepositoryError::unavailable(
            "add-on item NFO",
            "not implemented",
        ))
    }
    /// Return one NFO to generated/provider ownership.
    async fn reset_nfo(&self, list_id: EntityId, item_id: Option<EntityId>) -> Result<()> {
        let _ = (list_id, item_id);
        Err(RepositoryError::unavailable(
            "add-on NFO reset",
            "not implemented",
        ))
    }
    /// Update chapters for one full source item and refresh only that list.
    async fn update_item_chapters(&self, item_id: EntityId, chapter_input: &str) -> Result<()> {
        let _ = (item_id, chapter_input);
        Err(RepositoryError::unavailable(
            "add-on item chapters",
            "not implemented",
        ))
    }
    /// Delete one list and its generated playlist.
    async fn delete(&self, id: EntityId) -> Result<()>;
    /// Load the persisted list-page size.
    async fn get_page_size(&self) -> Result<u32>;
    /// Persist the list-page size.
    async fn set_page_size(&self, page_size: u32) -> Result<()>;
}
