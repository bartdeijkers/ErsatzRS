use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::{EntityId, MetadataSource};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MediaItemState {
    #[default]
    Normal,
    FileNotFound,
    Unavailable,
    RemoteOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MediaStreamKind {
    #[default]
    Video,
    Audio,
    Subtitle,
    Attachment,
    ExternalSubtitle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum VideoScanKind {
    #[default]
    Unknown,
    Progressive,
    Interlaced,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MediaItem {
    pub id: EntityId,
    pub library_path_id: EntityId,
    pub collection_ids: Vec<EntityId>,
    pub trakt_list_item_ids: Vec<EntityId>,
    pub state: MediaItemState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Movie {
    pub item: MediaItem,
    pub metadata_id: Option<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Show {
    pub item: MediaItem,
    pub metadata_id: Option<EntityId>,
    pub season_ids: Vec<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Season {
    pub item: MediaItem,
    pub show_id: Option<EntityId>,
    pub metadata_id: Option<EntityId>,
    pub season_number: i32,
    pub episode_ids: Vec<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Episode {
    pub item: MediaItem,
    pub show_id: Option<EntityId>,
    pub season_id: Option<EntityId>,
    pub metadata_id: Option<EntityId>,
    pub season_number: i32,
    pub episode_number: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MusicVideo {
    pub item: MediaItem,
    pub metadata_id: Option<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OtherVideo {
    pub item: MediaItem,
    pub metadata_id: Option<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Song {
    pub item: MediaItem,
    pub metadata_id: Option<EntityId>,
    pub artist_id: Option<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Artist {
    pub item: MediaItem,
    pub metadata_id: Option<EntityId>,
    pub song_ids: Vec<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Image {
    pub item: MediaItem,
    pub metadata_id: Option<EntityId>,
    pub duration_id: Option<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RemoteStream {
    pub item: MediaItem,
    pub metadata_id: Option<EntityId>,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ChapterMediaItem {
    pub item: MediaItem,
    pub source_media_item_id: EntityId,
    pub chapter_id: EntityId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PlexMovie {
    pub movie: Movie,
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PlexShow {
    pub show: Show,
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PlexSeason {
    pub season: Season,
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PlexEpisode {
    pub episode: Episode,
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PlexOtherVideo {
    pub other_video: OtherVideo,
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct JellyfinMovie {
    pub movie: Movie,
    pub item_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct JellyfinShow {
    pub show: Show,
    pub item_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct JellyfinSeason {
    pub season: Season,
    pub item_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct JellyfinEpisode {
    pub episode: Episode,
    pub item_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EmbyMovie {
    pub movie: Movie,
    pub item_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EmbyShow {
    pub show: Show,
    pub item_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EmbySeason {
    pub season: Season,
    pub item_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EmbyEpisode {
    pub episode: Episode,
    pub item_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaVersion {
    pub id: EntityId,
    pub name: String,
    pub media_file_ids: Vec<EntityId>,
    pub stream_ids: Vec<EntityId>,
    pub chapter_ids: Vec<EntityId>,
    pub duration: Duration,
    pub sample_aspect_ratio: Option<String>,
    pub display_aspect_ratio: Option<String>,
    pub r_frame_rate: Option<String>,
    pub video_scan_kind: VideoScanKind,
    pub interlaced_ratio: Option<f64>,
    pub date_added: DateTime<Utc>,
    pub date_updated: DateTime<Utc>,
    pub width: i32,
    pub height: i32,
}

impl Default for MediaVersion {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            id: 0,
            name: String::new(),
            media_file_ids: Vec::new(),
            stream_ids: Vec::new(),
            chapter_ids: Vec::new(),
            duration: Duration::zero(),
            sample_aspect_ratio: None,
            display_aspect_ratio: None,
            r_frame_rate: None,
            video_scan_kind: VideoScanKind::default(),
            interlaced_ratio: None,
            date_added: now,
            date_updated: now,
            width: 0,
            height: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FallbackMediaVersion {
    pub version: MediaVersion,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CoverArtMediaVersion {
    pub version: MediaVersion,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct BackgroundImageMediaVersion {
    pub version: MediaVersion,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ChapterMediaVersion {
    pub version: MediaVersion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MediaFile {
    pub id: EntityId,
    pub path: String,
    pub media_version_id: EntityId,
    pub part: i32,
    pub size: Option<i64>,
    pub date_updated: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PlexMediaFile {
    pub media_file: MediaFile,
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MediaStream {
    pub id: EntityId,
    pub media_version_id: EntityId,
    pub index: i32,
    pub kind: MediaStreamKind,
    pub codec: Option<String>,
    pub profile: Option<String>,
    pub language: Option<String>,
    pub title: Option<String>,
    pub default: bool,
    pub forced: bool,
    pub channels: Option<i32>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub pixel_format: Option<String>,
    pub color_range: Option<String>,
    pub color_space: Option<String>,
    pub color_transfer: Option<String>,
    pub color_primaries: Option<String>,
    pub bits_per_raw_sample: i32,
    pub attached_pic: bool,
    pub file_name: Option<String>,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MediaChapter {
    pub id: EntityId,
    pub media_version_id: EntityId,
    pub chapter_id: i64,
    pub title: String,
    pub start_time: Duration,
    pub end_time: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ImageFolderDuration {
    pub id: EntityId,
    pub path: String,
    pub duration: Duration,
    pub metadata_source: MetadataSource,
}
