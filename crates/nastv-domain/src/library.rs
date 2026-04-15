use serde::{Deserialize, Serialize};

use crate::{EntityId, MediaSourceKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LibraryMediaKind {
    #[default]
    Movies,
    Shows,
    MusicVideos,
    OtherVideos,
    Songs,
    Images,
    RemoteStreams,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Library {
    pub id: EntityId,
    pub name: String,
    pub media_kind: LibraryMediaKind,
    pub media_source_id: EntityId,
    pub media_source_kind: MediaSourceKind,
    pub folder_ids: Vec<EntityId>,
    pub path_ids: Vec<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LocalLibrary {
    pub library: Library,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PlexLibrary {
    pub library: Library,
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct JellyfinLibrary {
    pub library: Library,
    pub item_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EmbyLibrary {
    pub library: Library,
    pub item_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LibraryFolder {
    pub id: EntityId,
    pub library_id: EntityId,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LibraryPath {
    pub id: EntityId,
    pub library_id: EntityId,
    pub path: String,
    pub network_path: Option<String>,
}
