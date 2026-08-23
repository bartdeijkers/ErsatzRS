use serde::{Deserialize, Serialize};

use crate::EntityId;

pub const CONFIG_KEY_MINIMUM_LOG_LEVEL: &str = "log.minimum_level";
pub const CONFIG_KEY_MINIMUM_LOG_LEVEL_SCANNING: &str = "log.minimum_level.scanning";
pub const CONFIG_KEY_MINIMUM_LOG_LEVEL_SCHEDULING: &str = "log.minimum_level.scheduling";
pub const CONFIG_KEY_MINIMUM_LOG_LEVEL_SEARCHING: &str = "log.minimum_level.searching";
pub const CONFIG_KEY_MINIMUM_LOG_LEVEL_STREAMING: &str = "log.minimum_level.streaming";
pub const CONFIG_KEY_MINIMUM_LOG_LEVEL_HTTP: &str = "log.minimum_level.http";
pub const CONFIG_KEY_FFMPEG_PATH: &str = "ffmpeg.ffmpeg_path";
pub const CONFIG_KEY_FFPROBE_PATH: &str = "ffmpeg.ffprobe_path";
/// Whether the persisted `FFmpeg` paths are owned by managed runtime bootstrap.
pub const CONFIG_KEY_FFMPEG_PATHS_MANAGED: &str = "ffmpeg.paths_managed";
pub const CONFIG_KEY_FFMPEG_DEFAULT_PROFILE_ID: &str = "ffmpeg.default_profile_id";
pub const CONFIG_KEY_FFMPEG_SAVE_REPORTS: &str = "ffmpeg.save_reports";
pub const CONFIG_KEY_FFMPEG_USE_EMBEDDED_SUBTITLES: &str = "ffmpeg.use_embedded_subtitles";
pub const CONFIG_KEY_FFMPEG_EXTRACT_EMBEDDED_SUBTITLES: &str = "ffmpeg.extract_embedded_subtitles";
pub const CONFIG_KEY_FFMPEG_PROBE_FOR_INTERLACED_FRAMES: &str =
    "ffmpeg.probe_for_interlaced_frames";
pub const CONFIG_KEY_FFMPEG_PREFERRED_LANGUAGE_CODE: &str = "ffmpeg.preferred_language_code";
pub const CONFIG_KEY_FFMPEG_PREFERRED_SUBTITLE_LANGUAGE_CODE: &str =
    "ffmpeg.preferred_subtitle_language_code";
pub const CONFIG_KEY_FFMPEG_FALLBACK_SUBTITLE_LANGUAGE_CODE: &str =
    "ffmpeg.fallback_subtitle_language_code";
pub const CONFIG_KEY_FFMPEG_GLOBAL_WATERMARK_ID: &str = "ffmpeg.global_watermark_id";
pub const CONFIG_KEY_FFMPEG_GLOBAL_FALLBACK_FILLER_ID: &str = "ffmpeg.global_fallback_filler_id";
pub const CONFIG_KEY_FFMPEG_SEGMENTER_TIMEOUT: &str = "ffmpeg.segmenter.timeout_seconds";
pub const CONFIG_KEY_FFMPEG_WORK_AHEAD_SEGMENTERS: &str = "ffmpeg.segmenter.work_ahead_limit";
pub const CONFIG_KEY_FFMPEG_ACTIVE_TRANSCODE_LIMIT: &str =
    "ffmpeg.segmenter.active_transcode_limit";
pub const CONFIG_KEY_FFMPEG_INITIAL_SEGMENT_COUNT: &str = "ffmpeg.segmenter.initial_segment_count";
pub const CONFIG_KEY_FFMPEG_HLS_DIRECT_OUTPUT_FORMAT: &str = "ffmpeg.hls_direct.output_format";
pub const CONFIG_KEY_FFMPEG_DEFAULT_MPEG_TS_SCRIPT: &str = "ffmpeg.default_mpegts_script";
pub const CONFIG_KEY_HDHR_TUNER_COUNT: &str = "hdhr.tuner_count";
pub const CONFIG_KEY_HDHR_UUID: &str = "hdhr.uuid";
pub const CONFIG_KEY_PAGES_IS_DARK_MODE: &str = "pages.is_dark_mode";
pub const CONFIG_KEY_PAGES_LANGUAGE: &str = "pages.language";
pub const CONFIG_KEY_LIBRARY_REFRESH_INTERVAL: &str = "scanner.library_refresh_interval";
pub const CONFIG_KEY_ADDON_DETAIL_WAIT_MINIMUM_SECONDS: &str = "addons.detail_wait.minimum_seconds";
pub const CONFIG_KEY_ADDON_DETAIL_WAIT_MAXIMUM_SECONDS: &str = "addons.detail_wait.maximum_seconds";
pub const DEFAULT_ADDON_DETAIL_WAIT_MINIMUM_SECONDS: u32 = 2;
pub const DEFAULT_ADDON_DETAIL_WAIT_MAXIMUM_SECONDS: u32 = 16;
pub const CONFIG_KEY_PLAYOUT_DAYS_TO_BUILD: &str = "playout.days_to_build";
pub const CONFIG_KEY_PLAYOUT_SKIP_MISSING_ITEMS: &str = "playout.skip_missing_items";
pub const CONFIG_KEY_PLAYOUT_SCRIPTED_SCHEDULE_TIMEOUT_SECONDS: &str =
    "playout.scripted_schedule_timeout_seconds";
pub const CONFIG_KEY_XMLTV_TIME_ZONE: &str = "xmltv.time_zone";
pub const CONFIG_KEY_XMLTV_DAYS_TO_BUILD: &str = "xmltv.days_to_build";
pub const CONFIG_KEY_XMLTV_BLOCK_BEHAVIOR: &str = "xmltv.block_behavior";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ConfigElement {
    pub id: EntityId,
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConfigElementKey(pub String);

/// Global fair-use window used between add-on item-detail operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListDetailWaitRange {
    pub minimum_seconds: u32,
    pub maximum_seconds: u32,
}

impl AddonMediaListDetailWaitRange {
    #[must_use]
    pub const fn new(minimum_seconds: u32, maximum_seconds: u32) -> Option<Self> {
        if minimum_seconds == 0 || maximum_seconds == 0 || minimum_seconds > maximum_seconds {
            return None;
        }
        Some(Self {
            minimum_seconds,
            maximum_seconds,
        })
    }
}

impl Default for AddonMediaListDetailWaitRange {
    fn default() -> Self {
        Self {
            minimum_seconds: DEFAULT_ADDON_DETAIL_WAIT_MINIMUM_SECONDS,
            maximum_seconds: DEFAULT_ADDON_DETAIL_WAIT_MAXIMUM_SECONDS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addon_detail_wait_range_requires_positive_ordered_bounds() {
        assert_eq!(AddonMediaListDetailWaitRange::default().minimum_seconds, 2);
        assert_eq!(AddonMediaListDetailWaitRange::default().maximum_seconds, 16);
        assert!(AddonMediaListDetailWaitRange::new(1, 1).is_some());
        assert!(AddonMediaListDetailWaitRange::new(0, 1).is_none());
        assert!(AddonMediaListDetailWaitRange::new(2, 1).is_none());
    }
}
