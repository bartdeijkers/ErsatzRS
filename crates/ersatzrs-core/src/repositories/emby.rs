use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use ersatzrs_domain::{LibraryMediaKind, MediaChapter, MediaStream};

use super::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbyConnectionConfig {
    pub address: String,
    pub api_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbyLibrarySummary {
    pub item_id: String,
    pub name: String,
    pub media_kind: LibraryMediaKind,
    pub path_infos: Vec<EmbyPathInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbyPathInfo {
    pub path: String,
    pub network_path: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmbyMediaItem {
    pub item_id: String,
    pub etag: Option<String>,
    pub name: String,
    pub sort_title: String,
    pub plot: Option<String>,
    pub year: Option<i32>,
    pub tagline: Option<String>,
    pub content_rating: Option<String>,
    pub release_date: Option<DateTime<Utc>>,
    pub episode_number: Option<i32>,
    pub poster_artwork_path: Option<String>,
    pub thumb_artwork_path: Option<String>,
    pub fanart_artwork_path: Option<String>,
    pub provider_guids: Vec<String>,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub actors: Vec<EmbyActor>,
    pub directors: Vec<String>,
    pub writers: Vec<String>,
    pub path: String,
    pub date_created: DateTime<Utc>,
    pub duration: Duration,
    pub chapters: Vec<MediaChapter>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbyActor {
    pub name: String,
    pub role: Option<String>,
    pub order: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbyShowItem {
    pub item_id: String,
    pub etag: Option<String>,
    pub name: String,
    pub sort_title: String,
    pub plot: Option<String>,
    pub year: Option<i32>,
    pub tagline: Option<String>,
    pub content_rating: Option<String>,
    pub release_date: Option<DateTime<Utc>>,
    pub poster_artwork_path: Option<String>,
    pub thumb_artwork_path: Option<String>,
    pub fanart_artwork_path: Option<String>,
    pub provider_guids: Vec<String>,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub studios: Vec<String>,
    pub actors: Vec<EmbyActor>,
    pub date_created: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbySeasonItem {
    pub item_id: String,
    pub etag: Option<String>,
    pub name: String,
    pub sort_title: String,
    pub year: Option<i32>,
    pub release_date: Option<DateTime<Utc>>,
    pub poster_artwork_path: Option<String>,
    pub fanart_artwork_path: Option<String>,
    pub provider_guids: Vec<String>,
    pub date_created: DateTime<Utc>,
    pub season_number: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbyCollectionItem {
    pub item_id: String,
    pub etag: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbyCollectionMediaKind {
    Movie,
    Show,
    Season,
    Episode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbyCollectionMediaItem {
    pub item_id: String,
    pub kind: EmbyCollectionMediaKind,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmbyPlaybackMetadata {
    pub duration: Duration,
    pub width: i32,
    pub height: i32,
    pub sample_aspect_ratio: Option<String>,
    pub display_aspect_ratio: Option<String>,
    pub r_frame_rate: Option<String>,
    pub streams: Vec<MediaStream>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmbyItemPage {
    pub total_record_count: i32,
    pub items: Vec<EmbyMediaItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbyShowPage {
    pub total_record_count: i32,
    pub items: Vec<EmbyShowItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbySeasonPage {
    pub total_record_count: i32,
    pub items: Vec<EmbySeasonItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbyCollectionPage {
    pub total_record_count: i32,
    /// Number of raw response entries before supported-kind projection.
    pub received_count: usize,
    pub items: Vec<EmbyCollectionItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbyCollectionMediaPage {
    pub total_record_count: i32,
    /// Number of raw response entries before supported-kind projection.
    pub received_count: usize,
    pub items: Vec<EmbyCollectionMediaItem>,
}

#[async_trait]
pub trait EmbyService: Send + Sync {
    async fn list_libraries(
        &self,
        config: &EmbyConnectionConfig,
    ) -> Result<Vec<EmbyLibrarySummary>>;

    async fn fetch_movie_page(
        &self,
        config: &EmbyConnectionConfig,
        parent_id: &str,
        start_index: i32,
        limit: i32,
    ) -> Result<EmbyItemPage>;

    async fn fetch_show_page(
        &self,
        config: &EmbyConnectionConfig,
        parent_id: &str,
        start_index: i32,
        limit: i32,
    ) -> Result<EmbyShowPage>;

    async fn fetch_season_page(
        &self,
        config: &EmbyConnectionConfig,
        parent_id: &str,
        start_index: i32,
        limit: i32,
    ) -> Result<EmbySeasonPage>;

    async fn fetch_episode_page(
        &self,
        config: &EmbyConnectionConfig,
        show_id: &str,
        season_id: &str,
        start_index: i32,
        limit: i32,
    ) -> Result<EmbyItemPage>;

    async fn fetch_collection_page(
        &self,
        config: &EmbyConnectionConfig,
        parent_id: &str,
        start_index: i32,
        limit: i32,
    ) -> Result<EmbyCollectionPage>;

    async fn fetch_collection_items_page(
        &self,
        config: &EmbyConnectionConfig,
        parent_id: &str,
        start_index: i32,
        limit: i32,
    ) -> Result<EmbyCollectionMediaPage>;

    async fn stream_metadata(
        &self,
        config: &EmbyConnectionConfig,
        item_id: &str,
    ) -> Result<EmbyPlaybackMetadata>;
}
