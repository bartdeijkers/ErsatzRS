use chrono::Duration;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{EntityId, FFmpegProfile, FillerPreset, GraphicsElement, StreamingMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ChannelIdleBehavior {
    #[default]
    StopOnDisconnect,
    KeepRunning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ChannelMusicVideoCreditsMode {
    #[default]
    None,
    GenerateSubtitles,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ChannelPlayoutMode {
    #[default]
    Continuous,
    OnDemand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ChannelPlayoutSource {
    #[default]
    Generated,
    Mirror,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ChannelSongVideoMode {
    #[default]
    Default,
    WithProgress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ChannelStreamSelectorMode {
    #[default]
    Default,
    Custom,
    Troubleshooting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ChannelSubtitleMode {
    #[default]
    None,
    Forced,
    Default,
    Any,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ChannelTranscodeMode {
    #[default]
    OnDemand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ChannelWatermarkMode {
    #[default]
    None,
    Permanent,
    Intermittent,
    OpacityExpression,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ChannelWatermarkImageSource {
    #[default]
    Custom,
    ChannelLogo,
    Resource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum WatermarkLocation {
    #[default]
    BottomRight,
    BottomLeft,
    TopRight,
    TopLeft,
    TopMiddle,
    RightMiddle,
    BottomMiddle,
    LeftMiddle,
    MiddleCenter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum WatermarkSize {
    #[default]
    Scaled,
    ActualSize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Channel {
    pub id: EntityId,
    pub unique_id: Uuid,
    pub number: String,
    pub sort_number: f64,
    pub name: String,
    pub group: Option<String>,
    pub categories: Option<String>,
    pub ffmpeg_profile_id: EntityId,
    pub ffmpeg_profile: Option<FFmpegProfile>,
    pub slug_seconds: Option<f64>,
    pub watermark_id: Option<EntityId>,
    pub fallback_filler_id: Option<EntityId>,
    pub fallback_filler: Option<FillerPreset>,
    pub streaming_mode: StreamingMode,
    pub playout_ids: Vec<EntityId>,
    pub artwork_ids: Vec<EntityId>,
    pub stream_selector_mode: ChannelStreamSelectorMode,
    pub stream_selector: Option<String>,
    pub preferred_audio_language_code: Option<String>,
    pub preferred_audio_title: Option<String>,
    pub preferred_subtitle_language_code: Option<String>,
    pub subtitle_mode: ChannelSubtitleMode,
    pub music_video_credits_mode: ChannelMusicVideoCreditsMode,
    pub music_video_credits_template: Option<String>,
    pub song_video_mode: ChannelSongVideoMode,
    pub playout_source: ChannelPlayoutSource,
    pub playout_mode: ChannelPlayoutMode,
    pub mirror_source_channel_id: Option<EntityId>,
    pub playout_offset: Option<Duration>,
    pub transcode_mode: ChannelTranscodeMode,
    pub idle_behavior: ChannelIdleBehavior,
    pub is_enabled: bool,
    pub show_in_epg: bool,
}

impl Channel {
    pub const NUMBER_PATTERN: &'static str = r"^[0-9]+(\.[0-9]{1,2})?$";

    #[must_use]
    pub fn new(unique_id: Uuid) -> Self {
        Self {
            unique_id,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn web_encoded_name(&self) -> String {
        urlencoding::encode(&self.name).into_owned()
    }
}

impl Default for Channel {
    fn default() -> Self {
        Self {
            id: 0,
            unique_id: Uuid::nil(),
            number: String::new(),
            sort_number: 0.0,
            name: String::new(),
            group: None,
            categories: None,
            ffmpeg_profile_id: 0,
            ffmpeg_profile: None,
            slug_seconds: None,
            watermark_id: None,
            fallback_filler_id: None,
            fallback_filler: None,
            streaming_mode: StreamingMode::default(),
            playout_ids: Vec::new(),
            artwork_ids: Vec::new(),
            stream_selector_mode: ChannelStreamSelectorMode::default(),
            stream_selector: None,
            preferred_audio_language_code: None,
            preferred_audio_title: None,
            preferred_subtitle_language_code: None,
            subtitle_mode: ChannelSubtitleMode::default(),
            music_video_credits_mode: ChannelMusicVideoCreditsMode::default(),
            music_video_credits_template: None,
            song_video_mode: ChannelSongVideoMode::default(),
            playout_source: ChannelPlayoutSource::default(),
            playout_mode: ChannelPlayoutMode::default(),
            mirror_source_channel_id: None,
            playout_offset: None,
            transcode_mode: ChannelTranscodeMode::default(),
            idle_behavior: ChannelIdleBehavior::default(),
            is_enabled: true,
            show_in_epg: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChannelWatermark {
    pub id: EntityId,
    pub name: String,
    pub mode: ChannelWatermarkMode,
    pub image_source: ChannelWatermarkImageSource,
    pub image: Option<String>,
    pub original_content_type: Option<String>,
    pub location: WatermarkLocation,
    pub size: WatermarkSize,
    pub width_percent: f64,
    pub horizontal_margin_percent: f64,
    pub vertical_margin_percent: f64,
    pub frequency_minutes: i32,
    pub duration_seconds: i32,
    pub opacity: i32,
    pub place_within_source_content: bool,
    pub opacity_expression: Option<String>,
    pub z_index: i32,
}

impl Default for ChannelWatermark {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            mode: ChannelWatermarkMode::default(),
            image_source: ChannelWatermarkImageSource::default(),
            image: None,
            original_content_type: None,
            location: WatermarkLocation::default(),
            size: WatermarkSize::default(),
            width_percent: 10.0,
            horizontal_margin_percent: 0.0,
            vertical_margin_percent: 0.0,
            frequency_minutes: 0,
            duration_seconds: 0,
            opacity: 100,
            place_within_source_content: false,
            opacity_expression: None,
            z_index: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PlayoutItemWatermark {
    pub playout_item_id: EntityId,
    pub watermark_id: EntityId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProgramScheduleItemWatermark {
    pub program_schedule_item_id: EntityId,
    pub watermark_id: EntityId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BlockItemWatermark {
    pub block_item_id: EntityId,
    pub watermark_id: EntityId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DecoWatermark {
    pub deco_id: EntityId,
    pub watermark_id: EntityId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WatermarkGraphicsState {
    pub watermark_ids: Vec<EntityId>,
    pub graphics_elements: Vec<GraphicsElement>,
}
