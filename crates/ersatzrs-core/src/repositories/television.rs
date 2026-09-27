use async_trait::async_trait;
use ersatzrs_domain::EntityId;

use super::{ArtworkReference, Result};

/// Season-detail projection used by the admin TV season page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeasonDetail {
    pub id: EntityId,
    pub show_id: EntityId,
    pub show_title: String,
    pub show_year: Option<i64>,
    pub season_number: i32,
    pub season_poster_path: Option<ArtworkReference>,
    pub show_fanart_path: Option<ArtworkReference>,
}

/// Episode card projection used by the admin TV season page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpisodeCard {
    pub episode_id: EntityId,
    pub episode_number: i32,
    pub title: String,
    pub sort_title: String,
    pub plot: Option<String>,
    pub release_date: Option<String>,
    pub thumbnail_path: Option<ArtworkReference>,
    pub state: i32,
    pub file_path: Option<String>,
    pub local_path: Option<String>,
    pub directors: Vec<String>,
    pub writers: Vec<String>,
    pub show_title: String,
    pub show_id: EntityId,
    pub season_id: EntityId,
    pub season_number: i32,
}

/// Episode media-info projection equivalent to the movie branch used by the
/// web `MediaItemInfoDialog`.
#[derive(Clone, Debug, PartialEq)]
pub struct EpisodeMediaItemInfo {
    pub id: EntityId,
    pub title: String,
    pub show_title: String,
    pub season_number: i32,
    pub episode_number: i32,
    pub metadata_id: EntityId,
    pub library_kind: String,
    pub server_name: Option<String>,
    pub library_name: String,
    pub state: i32,
    pub media_version_id: Option<EntityId>,
    pub duration: Option<String>,
    pub sample_aspect_ratio: Option<String>,
    pub display_aspect_ratio: Option<String>,
    pub r_frame_rate: Option<String>,
    pub video_scan_kind: i32,
    pub interlaced_ratio: Option<f64>,
    pub width: i64,
    pub height: i64,
    pub is_local_library: bool,
}

/// Repository surface for TV show season and episode reads.
#[async_trait]
pub trait TelevisionRepository: Send + Sync {
    async fn get_season(&self, season_id: EntityId) -> Result<Option<SeasonDetail>>;

    async fn get_episode_count(&self, season_id: EntityId) -> Result<i64>;

    async fn get_paged_episodes(
        &self,
        season_id: EntityId,
        page_number: i64,
        page_size: i64,
    ) -> Result<Vec<EpisodeCard>>;

    async fn get_episode_media_item_info(
        &self,
        episode_id: EntityId,
    ) -> Result<Option<EpisodeMediaItemInfo>>;
}
