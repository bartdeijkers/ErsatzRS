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
pub enum ContentKind {
    #[default]
    Primary,
    Extra,
    AlternateVersion,
}

impl ContentKind {
    /// Database integer for `MediaItem.ContentKind`.
    #[must_use]
    pub const fn db_value(self) -> i32 {
        match self {
            Self::Primary => 0,
            Self::Extra => 1,
            Self::AlternateVersion => 2,
        }
    }

    /// Stable token exposed through the search contract.
    #[must_use]
    pub const fn search_token(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Extra => "extra",
            Self::AlternateVersion => "alternate_version",
        }
    }

    /// Parse a search token or friendly alias into a content kind.
    #[must_use]
    pub fn from_search_token(value: &str) -> Option<Self> {
        let token = normalize_sub_kind_token(value)?;
        match token.as_str() {
            "primary" => Some(Self::Primary),
            "extra" | "special_feature" | "special_features" => Some(Self::Extra),
            "alternate" | "alternateversion" | "alternate_version" | "version" | "versions" => {
                Some(Self::AlternateVersion)
            }
            _ => None,
        }
    }
}

/// Normalize an open content-variant subtype token.
#[must_use]
pub fn normalize_sub_kind_token(value: &str) -> Option<String> {
    let mut token = String::new();
    let mut last_was_separator = false;
    for ch in value.trim().chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            token.push(ch);
            last_was_separator = false;
        } else if !token.is_empty() && !last_was_separator {
            token.push('_');
            last_was_separator = true;
        }
    }
    while token.ends_with('_') {
        let _ = token.pop();
    }

    if token.is_empty() {
        return None;
    }

    let canonical = match token.as_str() {
        "behindthescenes" | "behind_the_scenes" => "behind_the_scenes",
        "deleted" | "deleted_scene" | "deleted_scenes" => "deleted_scene",
        "featurettes" | "featurette" => "featurette",
        "interviews" | "interview" => "interview",
        "scenes" | "scene" => "scene",
        "shorts" | "short" => "short",
        "trailers" | "trailer" => "trailer",
        "extras" | "specials" | "other" => "other",
        "officialvideo" | "official_video" => "official_video",
        "live" => "live",
        "unplugged" => "unplugged",
        "acoustic" => "acoustic",
        "remixes" | "remix" => "remix",
        "remastered" | "remasters" | "remaster" => "remaster",
        "extended_cut" | "extended_mix" | "extended_version" | "extended" => "extended",
        "directorscut" | "director_cut" | "director_s_cut" | "directors_cut" => "directors_cut",
        _ => token.as_str(),
    };

    Some(canonical.to_owned())
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

/// Exact media-version and stream tuple selected for one reusable playback
/// entry. A present selection pins the video stream; absent audio means the
/// source is silent and absent subtitle means subtitles are explicitly off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PinnedStreamSelection {
    pub media_version_id: EntityId,
    pub video_stream_index: i32,
    pub audio_stream_index: Option<i32>,
    pub subtitle_stream_index: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum VideoScanKind {
    #[default]
    Unknown,
    Progressive,
    Interlaced,
}

impl VideoScanKind {
    /// Database integer for `MediaVersion.VideoScanKind`, matching ErsatzTV's
    /// `VideoScanKind` enum (`Unknown = 0, Progressive = 1, Interlaced = 2`).
    /// Every writer must use this so scan-kind reads (deinterlace decisions,
    /// media browser labels) agree with ErsatzTV-compatible databases.
    #[must_use]
    pub const fn db_value(self) -> i32 {
        match self {
            Self::Unknown => 0,
            Self::Progressive => 1,
            Self::Interlaced => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MediaItem {
    pub id: EntityId,
    pub library_path_id: EntityId,
    pub collection_ids: Vec<EntityId>,
    pub trakt_list_item_ids: Vec<EntityId>,
    pub state: MediaItemState,
    pub content_kind: ContentKind,
    pub sub_kind: Option<String>,
    pub parent_media_item_id: Option<EntityId>,
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
    pub duration: Option<Duration>,
    pub fallback_query: Option<String>,
    pub script: Option<String>,
    pub url: Option<String>,
    pub is_live: bool,
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
