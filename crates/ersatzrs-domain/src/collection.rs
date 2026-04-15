use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{CollectionEnumeratorState, EntityId, PlaybackOrder};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CollectionType {
    #[default]
    Collection,
    TelevisionShow,
    TelevisionSeason,
    Artist,
    MultiCollection,
    SmartCollection,
    Playlist,
    RerunFirstRun,
    RerunRerun,
    SearchQuery,
    Movie,
    Episode,
    MusicVideo,
    OtherVideo,
    Song,
    Image,
    RemoteStream,
    FakeCollection,
    FakePlaylistItem,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TraktListItemKind {
    #[default]
    Movie,
    Show,
    Season,
    Episode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Collection {
    pub id: EntityId,
    pub name: String,
    pub collection_type: CollectionType,
    pub media_item_ids: Vec<EntityId>,
    pub item_ids: Vec<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PlexCollection {
    pub collection: Collection,
    pub key: String,
    pub plex_media_source_id: EntityId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct JellyfinCollection {
    pub collection: Collection,
    pub item_id: String,
    pub jellyfin_media_source_id: EntityId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EmbyCollection {
    pub collection: Collection,
    pub item_id: String,
    pub emby_media_source_id: EntityId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CollectionItem {
    pub id: EntityId,
    pub collection_id: EntityId,
    pub media_item_id: EntityId,
    pub sort_order: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MultiCollection {
    pub id: EntityId,
    pub name: String,
    pub item_ids: Vec<EntityId>,
    pub smart_item_ids: Vec<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MultiCollectionItem {
    pub id: EntityId,
    pub multi_collection_id: EntityId,
    pub collection_type: CollectionType,
    pub collection_id: Option<EntityId>,
    pub media_item_id: Option<EntityId>,
    pub playlist_id: Option<EntityId>,
    pub playback_order: PlaybackOrder,
    pub sort_order: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MultiCollectionSmartItem {
    pub id: EntityId,
    pub multi_collection_id: EntityId,
    pub smart_collection_id: EntityId,
    pub sort_order: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SmartCollection {
    pub id: EntityId,
    pub name: String,
    pub query: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RerunCollection {
    pub id: EntityId,
    pub name: String,
    pub collection_type: CollectionType,
    pub collection_id: Option<EntityId>,
    pub media_item_id: Option<EntityId>,
    pub multi_collection_id: Option<EntityId>,
    pub smart_collection_id: Option<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Playlist {
    pub id: EntityId,
    pub name: String,
    pub item_ids: Vec<EntityId>,
    pub group_ids: Vec<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PlaylistGroup {
    pub id: EntityId,
    pub playlist_id: EntityId,
    pub name: String,
    pub sort_order: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PlaylistItem {
    pub id: EntityId,
    pub playlist_id: EntityId,
    pub playlist_group_id: Option<EntityId>,
    pub collection_type: CollectionType,
    pub collection_id: Option<EntityId>,
    pub media_item_id: Option<EntityId>,
    pub multi_collection_id: Option<EntityId>,
    pub smart_collection_id: Option<EntityId>,
    pub playback_order: PlaybackOrder,
    pub sort_order: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PlexTag {
    pub id: EntityId,
    pub name: String,
    pub key: String,
    pub plex_media_source_id: EntityId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TraktList {
    pub id: EntityId,
    pub name: String,
    pub user: String,
    pub list: String,
    pub updated_at: Option<DateTime<Utc>>,
    pub item_ids: Vec<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TraktListItem {
    pub id: EntityId,
    pub trakt_list_id: EntityId,
    pub kind: TraktListItemKind,
    pub media_item_id: Option<EntityId>,
    pub season: Option<i32>,
    pub episode: Option<i32>,
    pub guid_ids: Vec<EntityId>,
    pub enumerator_state: CollectionEnumeratorState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TraktListItemGuid {
    pub id: EntityId,
    pub trakt_list_item_id: EntityId,
    pub source: String,
    pub value: String,
}
