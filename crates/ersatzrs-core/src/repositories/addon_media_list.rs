use async_trait::async_trait;
use ersatzrs_domain::EntityId;
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
    /// Import or refresh a provider list and rerun local matching.
    async fn add_or_refresh(&self, addon_id: &str, source_url: &str) -> Result<EntityId>;
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
    ) -> Result<()>;
    /// Delete one list and its generated playlist.
    async fn delete(&self, id: EntityId) -> Result<()>;
    /// Load the persisted list-page size.
    async fn get_page_size(&self) -> Result<u32>;
    /// Persist the list-page size.
    async fn set_page_size(&self, page_size: u32) -> Result<()>;
}
