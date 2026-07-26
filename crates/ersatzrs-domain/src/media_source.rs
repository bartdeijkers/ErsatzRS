use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::EntityId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MediaSourceKind {
    #[default]
    Local,
    Plex,
    Jellyfin,
    Emby,
}

impl MediaSourceKind {
    pub const ALL: [Self; 4] = [Self::Local, Self::Plex, Self::Jellyfin, Self::Emby];

    #[must_use]
    pub const fn provider_name(self) -> &'static str {
        match self {
            Self::Local => "Local",
            Self::Plex => "Plex",
            Self::Jellyfin => "Jellyfin",
            Self::Emby => "Emby",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MediaServerConnectionParameters {
    pub address: String,
    pub access_token: Option<String>,
    pub client_identifier: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MediaSource {
    pub id: EntityId,
    pub name: String,
    pub kind: MediaSourceKind,
    pub last_scan: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LocalMediaSource {
    pub media_source: MediaSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PlexMediaSource {
    pub media_source: MediaSource,
    pub server_name: Option<String>,
    pub machine_identifier: Option<String>,
    pub connections: Vec<PlexConnection>,
    pub path_replacements: Vec<PlexPathReplacement>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PlexConnection {
    pub id: EntityId,
    pub is_active: bool,
    pub uri: String,
    pub plex_media_source_id: EntityId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PlexPathReplacement {
    pub id: EntityId,
    pub plex_media_source_id: EntityId,
    pub path: String,
    pub replacement_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct JellyfinMediaSource {
    pub media_source: MediaSource,
    pub server_name: Option<String>,
    pub operating_system: Option<String>,
    pub connections: Vec<JellyfinConnection>,
    pub path_replacements: Vec<JellyfinPathReplacement>,
    pub last_collections_scan: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct JellyfinConnection {
    pub id: EntityId,
    pub address: String,
    pub jellyfin_media_source_id: EntityId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct JellyfinPathReplacement {
    pub id: EntityId,
    pub jellyfin_media_source_id: EntityId,
    pub path: String,
    pub replacement_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EmbyMediaSource {
    pub media_source: MediaSource,
    pub server_name: Option<String>,
    pub operating_system: Option<String>,
    pub connections: Vec<EmbyConnection>,
    pub path_replacements: Vec<EmbyPathReplacement>,
    pub last_collections_scan: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EmbyConnection {
    pub id: EntityId,
    pub address: String,
    pub emby_media_source_id: EntityId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EmbyPathReplacement {
    pub id: EntityId,
    pub emby_media_source_id: EntityId,
    pub path: String,
    pub replacement_path: String,
}

#[cfg(test)]
mod tests {
    use super::MediaSourceKind;

    #[test]
    fn media_source_registry_covers_local_and_every_remote_provider() {
        assert_eq!(
            MediaSourceKind::ALL.map(MediaSourceKind::provider_name),
            ["Local", "Plex", "Jellyfin", "Emby"]
        );
    }
}
