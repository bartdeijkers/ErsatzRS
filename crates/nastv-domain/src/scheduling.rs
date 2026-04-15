use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    ChannelSubtitleMode, CollectionType, EntityId, FillWithGroupMode, GuideMode, MarathonGroupBy,
    MultipleMode, PlaybackOrder, StartType, TailMode,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FixedStartTimeBehavior {
    #[default]
    Strict,
    Flexible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PlayoutMode {
    Flood,
    One,
    Multiple,
    #[default]
    Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BlockStopScheduling {
    #[default]
    AfterDurationEnd,
    BeforeDurationEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DecoBreakPlacement {
    #[default]
    BlockStart,
    BlockFinish,
    BetweenBlockItems,
    ChapterMarkers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DecoMode {
    #[default]
    Inherit,
    Disable,
    Override,
    Merge,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProgramSchedule {
    pub id: EntityId,
    pub name: String,
    pub keep_multi_part_episodes_together: bool,
    pub treat_collections_as_shows: bool,
    pub shuffle_schedule_items: bool,
    pub random_start_point: bool,
    pub fixed_start_time_behavior: FixedStartTimeBehavior,
    pub item_ids: Vec<EntityId>,
    pub playout_ids: Vec<EntityId>,
    pub alternate_ids: Vec<EntityId>,
}

impl Default for ProgramSchedule {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            keep_multi_part_episodes_together: true,
            treat_collections_as_shows: false,
            shuffle_schedule_items: false,
            random_start_point: false,
            fixed_start_time_behavior: FixedStartTimeBehavior::default(),
            item_ids: Vec::new(),
            playout_ids: Vec::new(),
            alternate_ids: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ProgramScheduleItem {
    pub id: EntityId,
    pub program_schedule_id: EntityId,
    pub collection_type: CollectionType,
    pub collection_id: Option<EntityId>,
    pub media_item_id: Option<EntityId>,
    pub multi_collection_id: Option<EntityId>,
    pub smart_collection_id: Option<EntityId>,
    pub rerun_collection_id: Option<EntityId>,
    pub playlist_id: Option<EntityId>,
    pub search_query: Option<String>,
    pub playout_mode: PlayoutMode,
    pub playback_order: PlaybackOrder,
    pub multiple_mode: MultipleMode,
    pub multiple_count: Option<String>,
    pub duration: Option<Duration>,
    pub start_type: StartType,
    pub start_time: Option<Duration>,
    pub fixed_start_time_behavior: Option<FixedStartTimeBehavior>,
    pub guide_mode: GuideMode,
    pub tail_mode: TailMode,
    pub fill_with_group_mode: FillWithGroupMode,
    pub marathon_group_by: MarathonGroupBy,
    pub marathon_shuffle_items: bool,
    pub custom_title: Option<String>,
    pub preferred_audio_language_code: Option<String>,
    pub preferred_audio_title: Option<String>,
    pub preferred_subtitle_language_code: Option<String>,
    pub subtitle_mode: Option<ChannelSubtitleMode>,
    pub sort_order: i32,
    pub watermark_ids: Vec<EntityId>,
    pub graphics_element_ids: Vec<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ProgramScheduleItemFlood {
    pub item: ProgramScheduleItem,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ProgramScheduleItemOne {
    pub item: ProgramScheduleItem,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ProgramScheduleItemMultiple {
    pub item: ProgramScheduleItem,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ProgramScheduleItemDuration {
    pub item: ProgramScheduleItem,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ProgramScheduleAlternate {
    pub id: EntityId,
    pub program_schedule_id: EntityId,
    pub playout_id: Option<EntityId>,
    pub schedule_item_id: EntityId,
    pub collection_type: CollectionType,
    pub collection_id: Option<EntityId>,
    pub media_item_id: Option<EntityId>,
    pub multi_collection_id: Option<EntityId>,
    pub smart_collection_id: Option<EntityId>,
    pub playlist_id: Option<EntityId>,
    pub fake_collection_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct BlockGroup {
    pub id: EntityId,
    pub name: String,
    pub block_ids: Vec<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Block {
    pub id: EntityId,
    pub block_group_id: Option<EntityId>,
    pub name: String,
    pub duration: Duration,
    pub stop_scheduling: BlockStopScheduling,
    pub item_ids: Vec<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct BlockItem {
    pub id: EntityId,
    pub block_id: EntityId,
    pub collection_type: CollectionType,
    pub collection_id: Option<EntityId>,
    pub media_item_id: Option<EntityId>,
    pub multi_collection_id: Option<EntityId>,
    pub smart_collection_id: Option<EntityId>,
    pub playlist_id: Option<EntityId>,
    pub search_query: Option<String>,
    pub playback_order: PlaybackOrder,
    pub sort_order: i32,
    pub watermark_ids: Vec<EntityId>,
    pub graphics_element_ids: Vec<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DecoGroup {
    pub id: EntityId,
    pub name: String,
    pub deco_ids: Vec<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Deco {
    pub id: EntityId,
    pub deco_group_id: Option<EntityId>,
    pub name: String,
    pub mode: DecoMode,
    pub break_content_id: Option<EntityId>,
    pub watermark_ids: Vec<EntityId>,
    pub graphics_element_ids: Vec<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DecoBreakContent {
    pub id: EntityId,
    pub deco_id: EntityId,
    pub placement: DecoBreakPlacement,
    pub collection_type: CollectionType,
    pub collection_id: Option<EntityId>,
    pub media_item_id: Option<EntityId>,
    pub playlist_id: Option<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TemplateGroup {
    pub id: EntityId,
    pub name: String,
    pub template_ids: Vec<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Template {
    pub id: EntityId,
    pub template_group_id: Option<EntityId>,
    pub name: String,
    pub item_ids: Vec<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TemplateItem {
    pub id: EntityId,
    pub template_id: EntityId,
    pub block_id: EntityId,
    pub sort_order: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DecoTemplateGroup {
    pub id: EntityId,
    pub name: String,
    pub template_ids: Vec<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DecoTemplate {
    pub id: EntityId,
    pub deco_template_group_id: Option<EntityId>,
    pub name: String,
    pub item_ids: Vec<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DecoTemplateItem {
    pub id: EntityId,
    pub deco_template_id: EntityId,
    pub deco_id: EntityId,
    pub sort_order: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PlayoutTemplate {
    pub playout_id: EntityId,
    pub template_id: EntityId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PlayoutHistory {
    pub id: EntityId,
    pub playout_id: EntityId,
    pub key: String,
    pub child_key: Option<String>,
    pub when: DateTime<Utc>,
    pub finish: Option<DateTime<Utc>>,
    pub is_current_child: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RerunHistory {
    pub id: EntityId,
    pub rerun_collection_id: EntityId,
    pub media_item_id: EntityId,
    pub date: DateTime<Utc>,
}
