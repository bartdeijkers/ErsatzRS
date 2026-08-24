use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    ChannelSubtitleMode, CollectionEnumeratorState, CollectionType, EntityId, FillerKind,
    MediaChapter, PlayoutItemGraphicsElement, PlayoutTemplate,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PlayoutScheduleKind {
    #[default]
    None,
    Classic,
    Block,
    Sequential,
    Scripted,
    ExternalJson,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Playout {
    pub id: EntityId,
    pub channel_id: EntityId,
    pub program_schedule_id: Option<EntityId>,
    pub schedule_file: Option<String>,
    pub program_schedule_alternate_ids: Vec<EntityId>,
    pub schedule_kind: PlayoutScheduleKind,
    pub item_ids: Vec<EntityId>,
    pub gap_ids: Vec<EntityId>,
    pub anchor: Option<PlayoutAnchor>,
    pub program_schedule_anchor_ids: Vec<EntityId>,
    pub fill_group_index_ids: Vec<EntityId>,
    pub templates: Vec<PlayoutTemplate>,
    pub playout_history_ids: Vec<EntityId>,
    pub seed: i32,
    pub daily_rebuild_time: Option<Duration>,
    pub deco_id: Option<EntityId>,
    pub on_demand_checkpoint: Option<DateTime<Utc>>,
    pub build_status: Option<PlayoutBuildStatus>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PlayoutAnchor {
    pub schedule_items_enumerator_state: CollectionEnumeratorState,
    pub next_start: DateTime<Utc>,
    pub multiple_remaining: Option<i32>,
    pub duration_finish: Option<DateTime<Utc>>,
    pub in_flood: bool,
    pub in_duration_filler: bool,
    pub next_guide_group: i32,
    pub next_instruction_index: i32,
    pub context: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PlayoutBuildStatus {
    pub playout_id: EntityId,
    pub last_build: DateTime<Utc>,
    pub success: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PlayoutGap {
    pub id: EntityId,
    pub playout_id: EntityId,
    pub start: DateTime<Utc>,
    pub finish: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayoutItem {
    pub id: EntityId,
    pub media_item_id: EntityId,
    pub start: DateTime<Utc>,
    pub finish: DateTime<Utc>,
    pub guide_start: Option<DateTime<Utc>>,
    pub guide_finish: Option<DateTime<Utc>>,
    pub custom_title: Option<String>,
    pub guide_group: i32,
    pub filler_kind: FillerKind,
    pub playout_id: EntityId,
    pub in_point: Duration,
    pub out_point: Duration,
    pub chapter_title: Option<String>,
    pub watermark_ids: Vec<EntityId>,
    pub disable_watermarks: bool,
    pub preferred_audio_language_code: Option<String>,
    pub preferred_audio_title: Option<String>,
    pub preferred_subtitle_language_code: Option<String>,
    pub subtitle_mode: Option<ChannelSubtitleMode>,
    pub pinned_stream_selection: Option<crate::PinnedStreamSelection>,
    pub block_key: Option<String>,
    pub collection_key: Option<String>,
    pub collection_etag: Option<String>,
    pub graphics_element_ids: Vec<EntityId>,
    pub graphics_elements: Vec<PlayoutItemGraphicsElement>,
    pub scheduling_context: Option<String>,
}

impl PlayoutItem {
    #[must_use]
    pub fn for_chapter(&self, chapter: &MediaChapter) -> Self {
        Self {
            finish: self.start + (chapter.end_time - chapter.start_time),
            in_point: chapter.start_time,
            out_point: chapter.end_time,
            chapter_title: Some(chapter.title.clone()),
            ..self.clone()
        }
    }

    #[must_use]
    pub fn display_duration(&self) -> String {
        let duration = self.finish - self.start;
        let total_seconds = duration.num_seconds().max(0);
        let hours = total_seconds / 3_600;
        let minutes = (total_seconds % 3_600) / 60;
        let seconds = total_seconds % 60;

        if hours > 0 {
            format!("{hours}:{minutes:02}:{seconds:02}")
        } else {
            format!("{minutes:02}:{seconds:02}")
        }
    }
}

impl Default for PlayoutItem {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            id: 0,
            media_item_id: 0,
            start: now,
            finish: now,
            guide_start: None,
            guide_finish: None,
            custom_title: None,
            guide_group: 0,
            filler_kind: FillerKind::default(),
            playout_id: 0,
            in_point: Duration::zero(),
            out_point: Duration::zero(),
            chapter_title: None,
            watermark_ids: Vec::new(),
            disable_watermarks: false,
            preferred_audio_language_code: None,
            preferred_audio_title: None,
            preferred_subtitle_language_code: None,
            subtitle_mode: None,
            pinned_stream_selection: None,
            block_key: None,
            collection_key: None,
            collection_etag: None,
            graphics_element_ids: Vec::new(),
            graphics_elements: Vec::new(),
            scheduling_context: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PlayoutProgramScheduleAnchor {
    pub id: EntityId,
    pub playout_id: EntityId,
    pub anchor_date: Option<DateTime<Utc>>,
    pub collection_type: CollectionType,
    pub collection_id: Option<EntityId>,
    pub multi_collection_id: Option<EntityId>,
    pub smart_collection_id: Option<EntityId>,
    pub rerun_collection_id: Option<EntityId>,
    pub media_item_id: Option<EntityId>,
    pub playlist_id: Option<EntityId>,
    pub search_query: Option<String>,
    pub fake_collection_key: Option<String>,
    pub enumerator_state: CollectionEnumeratorState,
}

/// Versioned Classic scheduler state captured at a dated rebuild seam.
///
/// Program-schedule anchors retain ErsatzTV's per-source enumerator rows. This
/// companion row owns the scheduler-wide state needed to prove that a Refresh
/// can restore the complete seam before replacing materialized playout items.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PlayoutClassicCheckpoint {
    pub playout_id: EntityId,
    pub checkpoint_date: DateTime<Utc>,
    pub context: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PlayoutScheduleItemFillGroupIndex {
    pub id: EntityId,
    pub playout_id: EntityId,
    pub program_schedule_item_id: EntityId,
    pub enumerator_state: CollectionEnumeratorState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PlayoutItemWithPath {
    pub playout_item_id: EntityId,
    pub path: String,
}
