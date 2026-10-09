use async_trait::async_trait;
use ersatzrs_domain::{Channel, EntityId};

use super::Result;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TroubleshootingOptionAdmin {
    pub id: EntityId,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BlockPlayoutOptionAdmin {
    pub playout_id: EntityId,
    pub channel_number: String,
    pub channel_name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TroubleshootingBlockAdmin {
    pub id: EntityId,
    pub group_id: EntityId,
    pub group_name: String,
    pub name: String,
    pub minutes: i32,
    pub stop_scheduling: i32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BlockPlayoutHistoryAdmin {
    pub id: EntityId,
    pub start: String,
    pub finish: Option<String>,
    pub key: String,
    pub details: String,
    pub playback_order: i32,
    pub collection_name: String,
    pub media_item_type: String,
    pub media_item_title: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlaybackMediaSummaryAdmin {
    pub kind: String,
    pub title: String,
    pub duration: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlaybackChannelSummaryAdmin {
    pub name: String,
    pub streaming_engine: ersatzrs_domain::StreamingEngine,
    pub ffmpeg_profile_id: EntityId,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlaybackSubtitleAdmin {
    pub id: EntityId,
    pub language: String,
    pub title: String,
    pub codec: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlaybackTroubleshootingAdmin {
    pub media: Option<PlaybackMediaSummaryAdmin>,
    pub channel: Option<PlaybackChannelSummaryAdmin>,
    pub ffmpeg_profiles: Vec<TroubleshootingOptionAdmin>,
    pub watermarks: Vec<TroubleshootingOptionAdmin>,
    pub graphics_elements: Vec<TroubleshootingOptionAdmin>,
    pub subtitles: Vec<PlaybackSubtitleAdmin>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MpegTroubleshootingChannelAdmin {
    pub number: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TroubleshootingChannelSummaryAdmin {
    pub id: EntityId,
    pub number: String,
    pub name: String,
    pub ffmpeg_profile_id: EntityId,
    pub streaming_mode: i32,
    pub stream_selector_mode: i32,
    pub stream_selector: Option<String>,
    pub is_enabled: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TroubleshootingFfmpegProfileSummaryAdmin {
    pub id: EntityId,
    pub name: String,
    pub hardware_acceleration: i32,
    pub video_format: i32,
    pub audio_format: i32,
    pub resolution_id: EntityId,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TroubleshootingPreviewMediaAdmin {
    pub media_item_id: EntityId,
    pub media_version_id: EntityId,
    pub path: String,
    pub kind: String,
    pub duration_seconds: f64,
}

#[async_trait]
pub trait TroubleshootingRepository: Send + Sync {
    async fn list_block_playout_options_admin(&self) -> Result<Vec<BlockPlayoutOptionAdmin>>;
    async fn list_blocks_for_playout_admin(
        &self,
        playout_id: EntityId,
    ) -> Result<Vec<TroubleshootingBlockAdmin>>;
    async fn list_block_playout_history_admin(
        &self,
        playout_id: EntityId,
        block_id: EntityId,
    ) -> Result<Vec<BlockPlayoutHistoryAdmin>>;
    async fn load_playback_context_admin(
        &self,
        media_item_id: EntityId,
        channel_id: EntityId,
    ) -> Result<PlaybackTroubleshootingAdmin>;
    async fn list_mpeg_channels_admin(&self) -> Result<Vec<MpegTroubleshootingChannelAdmin>>;
    async fn list_report_channels_admin(&self) -> Result<Vec<TroubleshootingChannelSummaryAdmin>>;
    async fn list_report_ffmpeg_profiles_admin(
        &self,
    ) -> Result<Vec<TroubleshootingFfmpegProfileSummaryAdmin>>;
    async fn load_preview_media_admin(
        &self,
        media_item_id: EntityId,
        media_version_id: Option<EntityId>,
    ) -> Result<Option<TroubleshootingPreviewMediaAdmin>>;
    async fn load_preview_channel_admin(&self, channel_id: EntityId) -> Result<Option<Channel>>;
    async fn load_media_item_path_admin(&self, media_item_id: EntityId) -> Result<Option<String>>;
}
