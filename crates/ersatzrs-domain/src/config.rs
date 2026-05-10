use serde::{Deserialize, Serialize};

use crate::EntityId;

pub const CONFIG_KEY_MINIMUM_LOG_LEVEL: &str = "log.minimum_level";
pub const CONFIG_KEY_MINIMUM_LOG_LEVEL_SCANNING: &str = "log.minimum_level.scanning";
pub const CONFIG_KEY_MINIMUM_LOG_LEVEL_SCHEDULING: &str = "log.minimum_level.scheduling";
pub const CONFIG_KEY_MINIMUM_LOG_LEVEL_SEARCHING: &str = "log.minimum_level.searching";
pub const CONFIG_KEY_MINIMUM_LOG_LEVEL_STREAMING: &str = "log.minimum_level.streaming";
pub const CONFIG_KEY_MINIMUM_LOG_LEVEL_HTTP: &str = "log.minimum_level.http";
pub const CONFIG_KEY_FFMPEG_GLOBAL_WATERMARK_ID: &str = "ffmpeg.global_watermark_id";
pub const CONFIG_KEY_FFMPEG_GLOBAL_FALLBACK_FILLER_ID: &str = "ffmpeg.global_fallback_filler_id";
pub const CONFIG_KEY_XMLTV_BLOCK_BEHAVIOR: &str = "xmltv.block_behavior";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ConfigElement {
    pub id: EntityId,
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConfigElementKey(pub String);
