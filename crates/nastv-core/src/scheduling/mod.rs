mod block;
mod scripted;
mod yaml;

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Utc};
use nastv_domain::{EntityId, Playout, PlayoutAnchor, PlayoutGap, PlayoutHistory, PlayoutItem};
use serde::{Deserialize, Serialize};

pub use block::{BlockSchedulingEngine, EffectiveBlock, EffectiveBlockItem};
pub use scripted::ScriptedSchedulingEngine;
pub use yaml::{
    YamlInstruction, YamlPlayoutDefinition, YamlPlayoutEngine, YamlPlayoutSequenceItem,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayoutBuildMode {
    Refresh,
    Reset,
    #[default]
    Continue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulingMediaItem {
    pub id: EntityId,
    pub duration: Duration,
    pub title: Option<String>,
    pub season: Option<i32>,
    pub episode: Option<i32>,
}

impl SchedulingMediaItem {
    #[must_use]
    pub fn new(id: EntityId, duration: Duration) -> Self {
        Self {
            id,
            duration,
            title: None,
            season: None,
            episode: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SchedulingReferenceData {
    pub media_by_key: BTreeMap<String, Vec<SchedulingMediaItem>>,
    pub watermark_ids_by_name: BTreeMap<String, EntityId>,
    pub graphics_ids_by_path: BTreeMap<String, EntityId>,
    pub effective_blocks: Vec<EffectiveBlock>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SchedulingRequest {
    pub playout: Playout,
    pub start: DateTime<Utc>,
    pub finish: DateTime<Utc>,
    pub mode: PlayoutBuildMode,
    pub reference_data: SchedulingReferenceData,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SchedulingResult {
    pub clear_items: bool,
    pub remove_before: Option<DateTime<Utc>>,
    pub added_items: Vec<PlayoutItem>,
    pub added_gaps: Vec<PlayoutGap>,
    pub added_history: Vec<PlayoutHistory>,
    pub history_to_remove: Vec<EntityId>,
    pub items_to_remove: Vec<EntityId>,
    pub anchor: Option<PlayoutAnchor>,
    pub warnings: Vec<String>,
}

pub trait PlayoutSchedulingEngine {
    /// Build a playout timeline for the supplied request.
    ///
    /// # Errors
    ///
    /// Returns an error when the engine cannot load or evaluate its schedule
    /// definition.
    fn schedule(&self, request: SchedulingRequest) -> Result<SchedulingResult, SchedulingError>;
}

#[derive(Debug, thiserror::Error)]
pub enum SchedulingError {
    #[error("missing schedule file")]
    MissingScheduleFile,
    #[error("schedule file {0} does not exist")]
    ScheduleFileMissing(String),
    #[error("invalid YAML schedule: {0}")]
    InvalidYaml(String),
    #[error("YAML sequence contains a cycle")]
    SequenceCycle,
    #[error("YAML sequence nesting is too deep")]
    SequenceNestingTooDeep,
    #[error("unsupported YAML instruction")]
    UnsupportedYamlInstruction,
    #[error("invalid duration: {0}")]
    InvalidDuration(String),
    #[error("invalid time: {0}")]
    InvalidTime(String),
    #[error("script error: {0}")]
    Script(String),
}

#[derive(Debug, Clone)]
struct MediaEnumerator {
    key: String,
    items: Vec<SchedulingMediaItem>,
    index: usize,
}

impl MediaEnumerator {
    fn new(key: impl Into<String>, items: Vec<SchedulingMediaItem>) -> Self {
        Self {
            key: key.into(),
            items,
            index: 0,
        }
    }

    fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    fn len(&self) -> usize {
        self.items.len()
    }

    fn current(&self) -> Option<&SchedulingMediaItem> {
        if self.items.is_empty() {
            None
        } else {
            self.items.get(self.index % self.items.len())
        }
    }

    fn advance(&mut self) {
        if !self.items.is_empty() {
            self.index = (self.index + 1) % self.items.len();
        }
    }
}

fn lookup_media(reference_data: &SchedulingReferenceData, key: &str) -> MediaEnumerator {
    MediaEnumerator::new(
        key,
        reference_data
            .media_by_key
            .get(key)
            .cloned()
            .unwrap_or_default(),
    )
}

#[allow(clippy::too_many_arguments)]
fn playout_item(
    playout_id: EntityId,
    media_item: &SchedulingMediaItem,
    start: DateTime<Utc>,
    duration: Duration,
    guide_group: i32,
    filler_kind: nastv_domain::FillerKind,
    custom_title: Option<String>,
    disable_watermarks: bool,
    watermark_ids: Vec<EntityId>,
    graphics_element_ids: Vec<EntityId>,
    scheduling_context: Option<String>,
) -> PlayoutItem {
    PlayoutItem {
        playout_id,
        media_item_id: media_item.id,
        start,
        finish: start + duration,
        guide_group,
        in_point: Duration::zero(),
        out_point: duration,
        filler_kind,
        custom_title,
        disable_watermarks,
        watermark_ids,
        graphics_element_ids,
        scheduling_context,
        ..PlayoutItem::default()
    }
}

fn history(
    playout_id: EntityId,
    key: impl Into<String>,
    item: &PlayoutItem,
    child_key: Option<String>,
) -> PlayoutHistory {
    PlayoutHistory {
        playout_id,
        key: key.into(),
        child_key,
        when: item.start,
        finish: Some(item.finish),
        ..PlayoutHistory::default()
    }
}

fn parse_duration(value: &str) -> Result<Duration, SchedulingError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(SchedulingError::InvalidDuration(value.to_owned()));
    }

    let parts = trimmed
        .split(':')
        .map(str::parse::<i64>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| SchedulingError::InvalidDuration(value.to_owned()))?;

    match parts.as_slice() {
        [seconds] => Ok(Duration::seconds(*seconds)),
        [minutes, seconds] => Ok(Duration::minutes(*minutes) + Duration::seconds(*seconds)),
        [hours, minutes, seconds] => {
            Ok(Duration::hours(*hours) + Duration::minutes(*minutes) + Duration::seconds(*seconds))
        }
        _ => Err(SchedulingError::InvalidDuration(value.to_owned())),
    }
}

fn parse_hhmm(value: &str) -> Result<(u32, u32, u32), SchedulingError> {
    let parts = value
        .trim()
        .split(':')
        .map(str::parse::<u32>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| SchedulingError::InvalidTime(value.to_owned()))?;

    match parts.as_slice() {
        [hour, minute] if *hour < 24 && *minute < 60 => Ok((*hour, *minute, 0)),
        [hour, minute, second] if *hour < 24 && *minute < 60 && *second < 60 => {
            Ok((*hour, *minute, *second))
        }
        _ => Err(SchedulingError::InvalidTime(value.to_owned())),
    }
}
