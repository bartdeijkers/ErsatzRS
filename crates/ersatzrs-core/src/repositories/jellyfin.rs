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
}

#[derive(Debug, Clone, PartialEq)]
pub struct JellyfinMediaItem {
    pub item_id: String,
    pub etag: Option<String>,
    pub name: String,
    pub path: String,
    pub date_created: DateTime<Utc>,
    pub duration: Duration,
    pub chapters: Vec<MediaChapter>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JellyfinPlaybackMetadata {
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

    async fn stream_metadata(
        &self,
        config: &JellyfinConnectionConfig,
        item_id: &str,
    ) -> Result<JellyfinPlaybackMetadata>;
}
