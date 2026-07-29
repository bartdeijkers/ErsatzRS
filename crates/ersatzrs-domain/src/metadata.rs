use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::EntityId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MetadataKind {
    #[default]
    Fallback,
    Sidecar,
    External,
    Embedded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MetadataSource {
    #[default]
    Fallback,
    Sidecar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ArtworkKind {
    #[default]
    Poster,
    Thumbnail,
    Logo,
    FanArt,
    Watermark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SubtitleKind {
    #[default]
    Embedded,
    Sidecar,
    Generated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Artwork {
    pub id: EntityId,
    pub path: String,
    pub kind: ArtworkKind,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub sort_order: i32,
    pub metadata_id: Option<EntityId>,
    pub channel_id: Option<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MetadataGuid {
    pub id: EntityId,
    pub metadata_id: EntityId,
    pub source: String,
    pub guid: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Genre {
    pub id: EntityId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Tag {
    pub id: EntityId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Studio {
    pub id: EntityId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Actor {
    pub id: EntityId,
    pub name: String,
    pub role: Option<String>,
    pub order: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Director {
    pub id: EntityId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Writer {
    pub id: EntityId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Mood {
    pub id: EntityId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Style {
    pub id: EntityId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Subtitle {
    pub id: EntityId,
    pub metadata_id: EntityId,
    pub path: Option<String>,
    pub language: Option<String>,
    pub title: Option<String>,
    pub kind: SubtitleKind,
    pub is_default: bool,
    pub is_forced: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Metadata {
    pub id: EntityId,
    pub metadata_kind: MetadataKind,
    pub title: String,
    pub original_title: Option<String>,
    pub sort_title: Option<String>,
    pub year: Option<i32>,
    pub release_date: Option<DateTime<Utc>>,
    pub date_added: DateTime<Utc>,
    pub date_updated: DateTime<Utc>,
    pub artwork_ids: Vec<EntityId>,
    pub genre_ids: Vec<EntityId>,
    pub tag_ids: Vec<EntityId>,
    pub studio_ids: Vec<EntityId>,
    pub actor_ids: Vec<EntityId>,
    pub guid_ids: Vec<EntityId>,
    pub subtitle_ids: Vec<EntityId>,
}

impl Default for Metadata {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            id: 0,
            metadata_kind: MetadataKind::default(),
            title: String::new(),
            original_title: None,
            sort_title: None,
            year: None,
            release_date: None,
            date_added: now,
            date_updated: now,
            artwork_ids: Vec::new(),
            genre_ids: Vec::new(),
            tag_ids: Vec::new(),
            studio_ids: Vec::new(),
            actor_ids: Vec::new(),
            guid_ids: Vec::new(),
            subtitle_ids: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MovieMetadata {
    pub metadata: Metadata,
    pub outline: Option<String>,
    pub plot: Option<String>,
    pub tagline: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ShowMetadata {
    pub metadata: Metadata,
    pub outline: Option<String>,
    pub plot: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SeasonMetadata {
    pub metadata: Metadata,
    pub season_number: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EpisodeMetadata {
    pub metadata: Metadata,
    pub season_number: i32,
    pub episode_number: i32,
    pub outline: Option<String>,
    pub plot: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MusicVideoMetadata {
    pub metadata: Metadata,
    pub album: Option<String>,
    pub track: Option<i32>,
    pub artist_ids: Vec<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SongMetadata {
    pub metadata: Metadata,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub track: Option<i32>,
    pub disc: Option<i32>,
    pub artist_ids: Vec<EntityId>,
    pub mood_ids: Vec<EntityId>,
    pub style_ids: Vec<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ArtistMetadata {
    pub metadata: Metadata,
    pub biography: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ImageMetadata {
    pub metadata: Metadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OtherVideoMetadata {
    pub metadata: Metadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RemoteStreamMetadata {
    pub metadata: Metadata,
    pub content_rating: Option<String>,
    pub plot: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MediaServerItemEtag {
    pub media_item_id: EntityId,
    pub etag: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MusicVideoArtist {
    pub music_video_metadata_id: EntityId,
    pub artist_id: EntityId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SongTag {
    pub song_metadata_id: EntityId,
    pub tag_id: EntityId,
}
