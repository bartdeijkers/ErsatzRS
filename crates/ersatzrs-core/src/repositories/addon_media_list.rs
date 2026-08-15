use async_trait::async_trait;
use ersatzrs_domain::{AddonMediaListContentKind, AddonMediaListItemAvailability, EntityId};
use serde::{Deserialize, Serialize};

use super::Result;

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
    /// Whether daily provider refresh is enabled.
    pub auto_refresh: bool,
    /// Whether a host-owned generated playlist is maintained.
    pub generate_playlist: bool,
    /// Optional list-wide content classification; `Auto` keeps each
    /// provider-supplied item classification.
    pub content_kind_override: AddonMediaListContentKind,
}

/// One persisted provider item and its optional local replacement.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListItemAdminRow {
    pub id: EntityId,
    pub rank: i32,
    pub display_title: String,
    pub availability: AddonMediaListItemAvailability,
    pub availability_reason: Option<String>,
    pub replacement_media_item_id: Option<EntityId>,
    pub replacement_media_version_id: Option<EntityId>,
    pub replacement_title: Option<String>,
    pub replacement_path: Option<String>,
    pub replacement_valid: bool,
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
    ) -> Result<EntityId>;
    /// Return the managed Local library projected for a provider list.
    async fn managed_library_id(&self, id: EntityId) -> Result<Option<EntityId>>;
    /// Rerun local matching without invoking the provider.
    async fn match_items(&self, id: EntityId) -> Result<()>;
    /// Update refresh and generated-playlist settings.
    async fn update_settings(
        &self,
        id: EntityId,
        auto_refresh: bool,
        generate_playlist: bool,
        content_kind_override: AddonMediaListContentKind,
    ) -> Result<()>;
    /// Delete one list and its generated playlist.
    async fn delete(&self, id: EntityId) -> Result<()>;
    /// Load the persisted list-page size.
    async fn get_page_size(&self) -> Result<u32>;
    /// Persist the list-page size.
    async fn set_page_size(&self, page_size: u32) -> Result<()>;
}
