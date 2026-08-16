use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{EntityId, PlaybackOrder};

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

/// Media kinds accepted from a provider-neutral add-on list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListItemKind {
    #[default]
    Movie,
    Show,
    Season,
    Episode,
    RemoteStream,
}

/// Provider-reported availability for an add-on list item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListItemAvailability {
    /// The provider confirms that the item can currently be played.
    #[default]
    Available,
    /// The provider confirms that the item cannot currently be played.
    Unavailable,
    /// The provider could not determine availability without a costly probe.
    Unknown,
}

/// Descriptive content classification for an add-on supplied stream.
///
/// This does not change the storage identity: generated streams remain Remote
/// Streams. It controls metadata projection and compatible NFO roots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListContentKind {
    /// Let the provider classify each item, retaining `Auto` when uncertain.
    #[default]
    Auto,
    /// One television episode; season and episode numbers remain optional.
    TelevisionEpisode,
    Movie,
    MusicVideo,
    /// Includes commercials, archive fragments, and uncategorized videos.
    OtherVideo,
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
    pub pinned_stream_selection: Option<crate::PinnedStreamSelection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PlexTag {
    pub id: EntityId,
    pub name: String,
    pub key: String,
    pub plex_media_source_id: EntityId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AddonMediaList {
    pub id: EntityId,
    pub addon_id: String,
    pub source_url: String,
    pub provider_id: String,
    pub name: String,
    pub description: Option<String>,
    pub item_count: i32,
    pub auto_refresh: bool,
    pub generate_playlist: bool,
    pub content_kind_override: AddonMediaListContentKind,
    pub chapter_input: String,
    pub playlist_id: Option<EntityId>,
    pub last_update: Option<DateTime<Utc>>,
    pub last_match: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AddonMediaListItem {
    pub id: EntityId,
    pub addon_media_list_id: EntityId,
    pub provider_id: String,
    pub rank: i32,
    pub display_title: String,
    pub title: String,
    pub year: Option<i32>,
    pub season: Option<i32>,
    pub episode: Option<i32>,
    pub kind: AddonMediaListItemKind,
    pub content_kind: AddonMediaListContentKind,
    pub availability: AddonMediaListItemAvailability,
    pub availability_reason: Option<String>,
    pub media_item_id: Option<EntityId>,
    pub duration_seconds: Option<u64>,
    pub parent_provider_id: Option<String>,
    pub fragment_start_seconds: Option<u64>,
    pub fragment_end_seconds: Option<u64>,
    pub additional_image_urls: Vec<String>,
}

/// One user-authored chapter linked to its full and fragment add-on items.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AddonMediaListChapter {
    pub id: EntityId,
    pub addon_media_list_id: EntityId,
    pub source_item_id: EntityId,
    pub fragment_item_id: EntityId,
    pub chapter_index: i32,
    pub title: String,
    pub start_seconds: u64,
    pub end_seconds: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AddonMediaListItemGuid {
    pub id: EntityId,
    pub addon_media_list_item_id: EntityId,
    pub guid: String,
}
