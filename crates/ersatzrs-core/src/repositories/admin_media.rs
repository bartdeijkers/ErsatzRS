use async_trait::async_trait;
use ersatzrs_domain::{ContentKind, EntityId};

use crate::metadata_facets::MediaFacetSet;

use super::Result;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AdminMediaCard {
    pub id: EntityId,
    pub title: String,
    pub sort_title: String,
    pub subtitle: Option<String>,
    pub artwork_path: Option<String>,
    pub season_number: Option<i64>,
    pub content_kind: ContentKind,
    pub sub_kind: Option<String>,
    pub parent_title: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AdminActorCard {
    pub id: EntityId,
    pub name: String,
    pub role: Option<String>,
    pub artwork_path: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AdminAudioTrack {
    pub media_version_id: EntityId,
    pub stream_index: i64,
    pub title: String,
    pub language: Option<String>,
    pub codec: Option<String>,
    pub channels: i64,
    pub is_default: bool,
    pub is_commentary: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AdminCommentarySubtitle {
    pub id: EntityId,
    pub stream_index: i64,
    pub title: String,
    pub language: Option<String>,
    pub codec: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AdminMoviePlaybackStream {
    pub index: i64,
    pub kind: i32,
    pub title: Option<String>,
    pub language: Option<String>,
    pub codec: Option<String>,
    pub channels: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub is_default: bool,
    pub is_forced: bool,
    pub is_external: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AdminMoviePlaybackSource {
    pub media_version_id: EntityId,
    pub name: String,
    pub width: i64,
    pub height: i64,
    pub streams: Vec<AdminMoviePlaybackStream>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AdminMovieDetail {
    pub id: EntityId,
    pub title: String,
    pub year: Option<i64>,
    pub plot: Option<String>,
    pub tagline: Option<String>,
    pub content_ratings: Vec<String>,
    pub release_date: Option<String>,
    pub poster_path: Option<String>,
    pub fanart_path: Option<String>,
    pub file_path: Option<String>,
    pub local_path: Option<String>,
    pub media_item_state: i32,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub languages: Vec<String>,
    pub studios: Vec<String>,
    pub facets: MediaFacetSet,
    pub directors: Vec<String>,
    pub writers: Vec<String>,
    pub actors: Vec<AdminActorCard>,
    pub extras: Vec<AdminMediaCard>,
    pub alternate_versions: Vec<AdminMediaCard>,
    pub audio_tracks: Vec<AdminAudioTrack>,
    pub commentary_subtitles: Vec<AdminCommentarySubtitle>,
    pub playback_sources: Vec<AdminMoviePlaybackSource>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AdminSeasonCard {
    pub id: EntityId,
    pub season_number: i64,
    pub poster_path: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AdminShowDetail {
    pub id: EntityId,
    pub library_id: EntityId,
    pub media_source_kind: String,
    pub title: String,
    pub year: Option<i64>,
    pub plot: Option<String>,
    pub content_ratings: Vec<String>,
    pub poster_path: Option<String>,
    pub fanart_path: Option<String>,
    pub media_item_state: i32,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub languages: Vec<String>,
    pub studios: Vec<String>,
    pub facets: MediaFacetSet,
    pub networks: Vec<String>,
    pub actors: Vec<AdminActorCard>,
    pub seasons: Vec<AdminSeasonCard>,
    pub extras: Vec<AdminMediaCard>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AdminArtistMusicVideo {
    pub id: EntityId,
    pub title: String,
    pub sort_title: String,
    pub plot: Option<String>,
    pub album: Option<String>,
    pub thumbnail_path: Option<String>,
    pub media_item_state: i32,
    pub file_path: Option<String>,
    pub local_path: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AdminArtistDetail {
    pub id: EntityId,
    pub title: String,
    pub disambiguation: Option<String>,
    pub biography: Option<String>,
    pub thumbnail_path: Option<String>,
    pub fanart_path: Option<String>,
    pub genres: Vec<String>,
    pub styles: Vec<String>,
    pub moods: Vec<String>,
    pub languages: Vec<String>,
    pub music_videos: Vec<AdminArtistMusicVideo>,
    pub versions: Vec<AdminArtistMusicVideo>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AdminOtherVideoDetail {
    pub id: EntityId,
    pub title: String,
    pub original_title: Option<String>,
    pub year: Option<i64>,
    pub plot: Option<String>,
    pub tagline: Option<String>,
    pub content_ratings: Vec<String>,
    pub release_date: Option<String>,
    pub thumbnail_path: Option<String>,
    pub poster_path: Option<String>,
    pub fanart_path: Option<String>,
    pub file_path: Option<String>,
    pub local_path: Option<String>,
    pub media_item_state: i32,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub languages: Vec<String>,
    pub studios: Vec<String>,
    pub facets: MediaFacetSet,
    pub directors: Vec<String>,
    pub writers: Vec<String>,
    pub actors: Vec<AdminActorCard>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdminFacetedMediaKind {
    MusicVideo,
    Song,
    Image,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AdminFacetedMediaDetail {
    pub id: EntityId,
    pub title: String,
    pub original_title: Option<String>,
    pub year: Option<i64>,
    pub plot: Option<String>,
    pub release_date: Option<String>,
    pub artwork_path: Option<String>,
    pub fanart_path: Option<String>,
    pub file_path: Option<String>,
    pub local_path: Option<String>,
    pub media_item_state: i32,
    pub facets: MediaFacetSet,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AdminMediaInfoStream {
    pub index: Option<i64>,
    pub kind: i32,
    pub external_subtitle: bool,
    pub title: Option<String>,
    pub codec: Option<String>,
    pub profile: Option<String>,
    pub language: Option<String>,
    pub channels: Option<i64>,
    pub is_default: Option<bool>,
    pub is_forced: Option<bool>,
    pub attached_pic: Option<bool>,
    pub pixel_format: Option<String>,
    pub color_range: Option<String>,
    pub color_space: Option<String>,
    pub color_transfer: Option<String>,
    pub color_primaries: Option<String>,
    pub bits_per_raw_sample: Option<i64>,
    pub mime_type: Option<String>,
    pub file_name: Option<String>,
    pub is_extracted: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AdminMediaInfoChapter {
    pub title: String,
    pub start_time: String,
    pub end_time: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdminMediaInfoKind {
    Movie,
    OtherVideo,
    Episode,
    MusicVideo,
    Song,
    Image,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AdminMediaItemInfo {
    pub id: EntityId,
    pub title: String,
    pub kind: AdminMediaInfoKind,
    pub library_kind: String,
    pub server_name: Option<String>,
    pub library_name: String,
    pub state: i32,
    pub duration: String,
    pub sample_aspect_ratio: Option<String>,
    pub display_aspect_ratio: Option<String>,
    pub r_frame_rate: Option<String>,
    pub video_scan_kind: i32,
    pub interlaced_ratio: Option<f64>,
    pub width: i64,
    pub height: i64,
    pub streams: Vec<AdminMediaInfoStream>,
    pub chapters: Vec<AdminMediaInfoChapter>,
}

#[async_trait]
pub trait AdminMediaRepository: Send + Sync {
    async fn get_movie_detail_admin(&self, id: EntityId) -> Result<Option<AdminMovieDetail>>;
    async fn get_show_detail_admin(&self, id: EntityId) -> Result<Option<AdminShowDetail>>;
    async fn get_artist_detail_admin(&self, id: EntityId) -> Result<Option<AdminArtistDetail>>;
    async fn get_other_video_detail_admin(
        &self,
        id: EntityId,
    ) -> Result<Option<AdminOtherVideoDetail>>;
    async fn get_faceted_media_detail_admin(
        &self,
        id: EntityId,
        kind: AdminFacetedMediaKind,
    ) -> Result<Option<AdminFacetedMediaDetail>>;
    async fn list_extras_admin(&self, parent_id: EntityId) -> Result<Vec<AdminMediaCard>>;
    async fn get_media_item_info_admin(&self, id: EntityId) -> Result<Option<AdminMediaItemInfo>>;
}
