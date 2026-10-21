use async_trait::async_trait;
use ersatzrs_domain::{
    EmbyMediaSource, EntityId, JellyfinMediaSource, MediaSource, PlexMediaSource,
};

use super::Result;

/// Untrusted provider-selected source and explicitly reported stream indexes.
///
/// This declaration certifies neither file bytes nor native stream identity.
/// Acquisition adapters must independently match the selected file, native
/// layout, current source revision and durable parents before using it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderMediaSourceDeclaration {
    pub source_id: String,
    pub path: String,
    pub stream_indexes: Vec<i32>,
}

impl ProviderMediaSourceDeclaration {
    /// Retain a complete, unambiguous declaration without inferring indexes.
    #[must_use]
    pub fn new(
        source_id: String,
        path: String,
        indexes: impl IntoIterator<Item = Option<i32>>,
    ) -> Option<Self> {
        if source_id.trim().is_empty() || path.trim().is_empty() {
            return None;
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut stream_indexes = Vec::new();
        for index in indexes {
            let index = index?;
            if index < 0 || !seen.insert(index) {
                return None;
            }
            stream_indexes.push(index);
        }
        if stream_indexes.is_empty() {
            return None;
        }
        Some(Self {
            source_id,
            path,
            stream_indexes,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteMediaSourceKind {
    Jellyfin,
    Emby,
    Plex,
}

impl RemoteMediaSourceKind {
    pub const ALL: [Self; 3] = [Self::Plex, Self::Jellyfin, Self::Emby];

    #[must_use]
    pub const fn provider_name(self) -> &'static str {
        match self {
            Self::Plex => "Plex",
            Self::Jellyfin => "Jellyfin",
            Self::Emby => "Emby",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalLibraryAdminRow {
    pub id: EntityId,
    pub name: String,
    pub media_kind: i32,
    pub path_count: u64,
    pub managed_by_addon: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagedAddonMediaSourceAdminRow {
    pub addon_id: String,
    pub name: String,
    pub root_path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalLibraryPathAdmin {
    pub id: EntityId,
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalLibraryAdmin {
    pub id: EntityId,
    pub name: String,
    pub media_kind: i32,
    pub paths: Vec<LocalLibraryPathAdmin>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteSourceAdminRow {
    pub id: EntityId,
    pub name: String,
    pub address: String,
    pub missing_since: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteConnectionAdmin {
    pub source_id: EntityId,
    pub address: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlexArtworkConnectionAdmin {
    pub address: String,
    pub token: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryPreferenceAdmin {
    pub id: EntityId,
    pub name: String,
    pub media_kind: i32,
    pub should_sync: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathReplacementAdmin {
    pub id: EntityId,
    pub remote_path: String,
    pub local_path: String,
}

#[async_trait]
pub trait MediaSourceRepository: Send + Sync {
    async fn get_plex(&self, id: EntityId) -> Result<PlexMediaSource>;
    async fn get_jellyfin(&self, id: EntityId) -> Result<JellyfinMediaSource>;
    async fn get_emby(&self, id: EntityId) -> Result<EmbyMediaSource>;
    async fn list_all(&self) -> Result<Vec<MediaSource>>;
    async fn upsert_plex(&self, source: &PlexMediaSource) -> Result<()>;
    async fn upsert_jellyfin(&self, source: &JellyfinMediaSource) -> Result<()>;
    async fn upsert_emby(&self, source: &EmbyMediaSource) -> Result<()>;
    async fn plex_artwork_connection_admin(
        &self,
        source_id: EntityId,
    ) -> Result<Option<PlexArtworkConnectionAdmin>>;
    async fn first_jellyfin_address_admin(&self) -> Result<Option<String>>;
    async fn first_emby_address_admin(&self) -> Result<Option<String>>;
    async fn list_local_libraries_admin(&self) -> Result<Vec<LocalLibraryAdminRow>>;
    async fn list_managed_addon_media_sources_admin(
        &self,
    ) -> Result<Vec<ManagedAddonMediaSourceAdminRow>>;
    async fn get_local_library_admin(&self, id: EntityId) -> Result<Option<LocalLibraryAdmin>>;
    async fn save_local_library_admin(&self, library: &LocalLibraryAdmin) -> Result<EntityId>;
    async fn delete_local_library_admin(&self, id: EntityId) -> Result<()>;
    async fn list_remote_sources_admin(
        &self,
        kind: RemoteMediaSourceKind,
    ) -> Result<Vec<RemoteSourceAdminRow>>;
    async fn get_remote_connection_admin(
        &self,
        kind: RemoteMediaSourceKind,
    ) -> Result<Option<RemoteConnectionAdmin>>;
    async fn save_remote_connection_admin(
        &self,
        kind: RemoteMediaSourceKind,
        connection: &RemoteConnectionAdmin,
        server_name: &str,
        operating_system: &str,
    ) -> Result<EntityId>;
    async fn delete_remote_source_admin(&self, id: EntityId) -> Result<()>;
    async fn delete_missing_plex_source_admin(&self, id: EntityId) -> Result<()>;
    async fn list_library_preferences_admin(
        &self,
        kind: RemoteMediaSourceKind,
        source_id: EntityId,
    ) -> Result<Vec<LibraryPreferenceAdmin>>;
    async fn save_library_preferences_admin(
        &self,
        kind: RemoteMediaSourceKind,
        preferences: &[LibraryPreferenceAdmin],
    ) -> Result<()>;
    async fn list_path_replacements_admin(
        &self,
        kind: RemoteMediaSourceKind,
        source_id: EntityId,
    ) -> Result<Vec<PathReplacementAdmin>>;
    async fn save_path_replacements_admin(
        &self,
        kind: RemoteMediaSourceKind,
        source_id: EntityId,
        paths: &[PathReplacementAdmin],
    ) -> Result<()>;
    async fn delete_all_plex_sources_admin(&self) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::RemoteMediaSourceKind;

    #[test]
    fn remote_media_source_registry_covers_every_provider() {
        assert_eq!(
            RemoteMediaSourceKind::ALL.map(RemoteMediaSourceKind::provider_name),
            ["Plex", "Jellyfin", "Emby"]
        );
    }
}
