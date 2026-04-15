use chrono::Duration;
use serde::{Deserialize, Serialize};

use crate::{EntityId, PlaybackOrder};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FillerKind {
    #[default]
    None,
    PreRoll,
    MidRoll,
    PostRoll,
    Tail,
    Fallback,
    GuideMode,
    DecoDefault,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FillerMode {
    #[default]
    None,
    Duration,
    Count,
    Pad,
    RandomCount,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FillerPreset {
    pub id: EntityId,
    pub name: String,
    pub filler_mode: FillerMode,
    pub collection_type: crate::CollectionType,
    pub collection_id: Option<EntityId>,
    pub media_item_id: Option<EntityId>,
    pub multi_collection_id: Option<EntityId>,
    pub smart_collection_id: Option<EntityId>,
    pub playlist_id: Option<EntityId>,
    pub duration: Option<Duration>,
    pub count: Option<i32>,
    pub pad_to_next_hour: bool,
    pub allow_watermarks: bool,
    pub playback_order: PlaybackOrder,
}
