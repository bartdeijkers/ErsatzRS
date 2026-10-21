use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use ersatzrs_domain::{LibraryMediaKind, MediaChapter, MediaStream};

use super::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JellyfinConnectionConfig {
    pub address: String,
    pub authorization_header: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JellyfinLibrarySummary {
    pub item_id: String,
    pub name: String,
    pub media_kind: LibraryMediaKind,
    pub path_infos: Vec<JellyfinPathInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JellyfinPathInfo {
    pub path: String,
    pub network_path: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JellyfinMediaItem {
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
    pub actors: Vec<JellyfinActor>,
    pub directors: Vec<String>,
    pub writers: Vec<String>,
    pub path: String,
    pub date_created: DateTime<Utc>,
    pub duration: Duration,
    pub chapters: Vec<MediaChapter>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JellyfinActor {
    pub name: String,
    pub role: Option<String>,
    pub order: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JellyfinShowItem {
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
    pub actors: Vec<JellyfinActor>,
    pub date_created: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JellyfinSeasonItem {
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
pub struct JellyfinCollectionItem {
    pub item_id: String,
    pub etag: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JellyfinCollectionMediaKind {
    Movie,
    Show,
    Season,
    Episode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JellyfinCollectionMediaItem {
    pub item_id: String,
    pub kind: JellyfinCollectionMediaKind,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JellyfinPlaybackMetadata {
    /// Provider declaration only; native acquisition must independently qualify it.
    pub source_declaration: Option<super::ProviderMediaSourceDeclaration>,
    pub duration: Duration,
    pub width: i32,
    pub height: i32,
    pub sample_aspect_ratio: Option<String>,
    pub display_aspect_ratio: Option<String>,
    pub r_frame_rate: Option<String>,
    pub streams: Vec<MediaStream>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JellyfinItemPage {
    pub total_record_count: i32,
    pub items: Vec<JellyfinMediaItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JellyfinShowPage {
    pub total_record_count: i32,
    pub items: Vec<JellyfinShowItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JellyfinSeasonPage {
    pub total_record_count: i32,
    pub items: Vec<JellyfinSeasonItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JellyfinCollectionPage {
    pub total_record_count: i32,
    /// Number of raw response entries before supported-kind projection.
    pub received_count: usize,
    pub items: Vec<JellyfinCollectionItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JellyfinCollectionMediaPage {
    pub total_record_count: i32,
    /// Number of raw response entries before supported-kind projection.
    pub received_count: usize,
    pub items: Vec<JellyfinCollectionMediaItem>,
}

#[async_trait]
pub trait JellyfinService: Send + Sync {
    async fn list_libraries(
        &self,
        config: &JellyfinConnectionConfig,
    ) -> Result<Vec<JellyfinLibrarySummary>>;

    async fn fetch_movie_page(
        &self,
        config: &JellyfinConnectionConfig,
        parent_id: &str,
        start_index: i32,
        limit: i32,
    ) -> Result<JellyfinItemPage>;

    async fn fetch_show_page(
        &self,
        config: &JellyfinConnectionConfig,
        parent_id: &str,
        start_index: i32,
        limit: i32,
    ) -> Result<JellyfinShowPage>;

    async fn fetch_season_page(
        &self,
        config: &JellyfinConnectionConfig,
        parent_id: &str,
        start_index: i32,
        limit: i32,
    ) -> Result<JellyfinSeasonPage>;

    async fn fetch_episode_page(
        &self,
        config: &JellyfinConnectionConfig,
        parent_id: &str,
        start_index: i32,
        limit: i32,
    ) -> Result<JellyfinItemPage>;

    async fn fetch_collection_page(
        &self,
        config: &JellyfinConnectionConfig,
        parent_id: &str,
        start_index: i32,
        limit: i32,
    ) -> Result<JellyfinCollectionPage>;

    async fn fetch_collection_items_page(
        &self,
        config: &JellyfinConnectionConfig,
        parent_id: &str,
        start_index: i32,
        limit: i32,
    ) -> Result<JellyfinCollectionMediaPage>;

    async fn stream_metadata(
        &self,
        config: &JellyfinConnectionConfig,
        item_id: &str,
    ) -> Result<JellyfinPlaybackMetadata>;
}
