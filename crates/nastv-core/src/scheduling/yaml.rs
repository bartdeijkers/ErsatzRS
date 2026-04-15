#![allow(clippy::similar_names)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use chrono::{DateTime, Duration, Timelike, Utc};
use nastv_domain::{EntityId, FillerKind, PlaybackOrder, Playout, PlayoutAnchor};
use serde::{Deserialize, Serialize};

use super::{
    cleanup_history, history, history_key_for_content, lookup_media_source, parse_duration,
    parse_hhmm, playout_item, MediaEnumerator, PlayoutBuildMode, PlayoutSchedulingEngine,
    SchedulingContentSource, SchedulingError, SchedulingMediaItem, SchedulingReferenceData,
    SchedulingRequest, SchedulingResult,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct YamlPlayoutDefinition {
    #[serde(default)]
    pub import: Vec<String>,
    #[serde(default)]
    pub content: Vec<YamlContentItem>,
    #[serde(default)]
    pub sequence: Vec<YamlPlayoutSequenceItem>,
    #[serde(default)]
    pub reset: Vec<YamlInstruction>,
    #[serde(default)]
    pub playout: Vec<YamlInstruction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct YamlContentItem {
    pub key: String,
    #[serde(default)]
    pub order: Option<String>,
    #[serde(default)]
    pub multi_part: bool,
    #[serde(default)]
    pub collection: Option<String>,
    #[serde(default)]
    pub multi_collection: Option<String>,
    #[serde(default)]
    pub smart_collection: Option<String>,
    #[serde(default)]
    pub playlist: Option<String>,
    #[serde(default)]
    pub playlist_group: Option<String>,
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub show: Option<String>,
    #[serde(default)]
    pub media_item: Option<EntityId>,
    #[serde(default)]
    pub item: Option<EntityId>,
}

impl YamlContentItem {
    fn playback_order(&self) -> PlaybackOrder {
        self.order.as_deref().map_or_else(
            || {
                if self.playlist.is_some() {
                    PlaybackOrder::None
                } else {
                    PlaybackOrder::Shuffle
                }
            },
            parse_playback_order,
        )
    }

    fn source(&self) -> SchedulingContentSource {
        if let Some(collection) = &self.collection {
            SchedulingContentSource::Collection(collection.clone())
        } else if let Some(multi_collection) = &self.multi_collection {
            SchedulingContentSource::MultiCollection(multi_collection.clone())
        } else if let Some(smart_collection) = &self.smart_collection {
            SchedulingContentSource::SmartCollection(smart_collection.clone())
        } else if let Some(playlist) = &self.playlist {
            SchedulingContentSource::Playlist {
                group: self.playlist_group.clone(),
                name: playlist.clone(),
            }
        } else if let Some(search) = &self.search {
            SchedulingContentSource::Search(search.clone())
        } else if let Some(media_item) = self.media_item.or(self.item) {
            SchedulingContentSource::MediaItem(media_item)
        } else {
            SchedulingContentSource::direct(&self.key)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct YamlPlayoutSequenceItem {
    pub key: String,
    #[serde(default)]
    pub items: Vec<YamlInstruction>,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct YamlInstruction {
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub filler_kind: Option<String>,
    #[serde(default)]
    pub custom_title: Option<String>,
    #[serde(default)]
    pub disable_watermarks: bool,
    #[serde(default)]
    pub all: Option<String>,
    #[serde(default)]
    pub count: Option<String>,
    #[serde(default)]
    pub duration: Option<String>,
    #[serde(default)]
    pub fallback: Option<String>,
    #[serde(default)]
    pub trim: bool,
    #[serde(default)]
    pub offline_tail: bool,
    #[serde(default)]
    pub discard_attempts: i32,
    #[serde(default = "default_stop_before_end")]
    pub stop_before_end: bool,
    #[serde(default)]
    pub pad_to_next: Option<i64>,
    #[serde(default)]
    pub pad_until: Option<String>,
    #[serde(default)]
    pub tomorrow: Option<String>,
    #[serde(default)]
    pub wait_until: Option<String>,
    #[serde(default)]
    pub rewind_on_reset: bool,
    #[serde(default)]
    pub rewind: Option<String>,
    #[serde(default)]
    pub repeat: Option<bool>,
    #[serde(default)]
    pub sequence: Option<String>,
    #[serde(default)]
    pub shuffle_sequence: Option<String>,
    #[serde(default)]
    pub pre_roll: Option<bool>,
    #[serde(default)]
    pub post_roll: Option<bool>,
    #[serde(default)]
    pub mid_roll: Option<bool>,
    #[serde(default = "default_mid_roll_expression")]
    pub expression: String,
    #[serde(default)]
    pub watermark: Option<bool>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub graphics_on: Option<String>,
    #[serde(default)]
    pub graphics_off: Option<String>,
    #[serde(default)]
    pub variables: BTreeMap<String, String>,
    #[serde(default)]
    pub epg_group: Option<bool>,
    #[serde(default)]
    pub advance: Option<bool>,
    #[serde(default)]
    pub skip_items: Option<String>,
    #[serde(default)]
    pub skip_to_item: Option<String>,
    #[serde(default)]
    pub season: Option<i32>,
    #[serde(default)]
    pub episode: Option<i32>,
    #[serde(default)]
    pub sequence_key: Option<String>,
    #[serde(default)]
    pub sequence_guid: Option<String>,
}

fn default_stop_before_end() -> bool {
    true
}

fn default_mid_roll_expression() -> String {
    "true".to_owned()
}

#[derive(Debug, Clone, Default)]
pub struct YamlPlayoutEngine;

impl YamlPlayoutEngine {
    /// Parse a sequential YAML playout definition.
    ///
    /// # Errors
    ///
    /// Returns an error when the YAML cannot be deserialized.
    pub fn definition_from_str(yaml: &str) -> Result<YamlPlayoutDefinition, SchedulingError> {
        serde_yaml::from_str(yaml).map_err(|error| SchedulingError::InvalidYaml(error.to_string()))
    }

    fn definition_for_playout(playout: &Playout) -> Result<YamlPlayoutDefinition, SchedulingError> {
        let path = playout
            .schedule_file
            .as_ref()
            .ok_or(SchedulingError::MissingScheduleFile)?;
        let yaml = fs::read_to_string(path)
            .map_err(|_| SchedulingError::ScheduleFileMissing(path.clone()))?;
        Self::definition_from_str(&yaml)
    }

    /// Build a timeline from an already loaded YAML definition.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid instruction arguments, sequence cycles, or
    /// unsupported instructions.
    #[allow(clippy::needless_pass_by_value)]
    pub fn schedule_definition(
        &self,
        mut definition: YamlPlayoutDefinition,
        request: SchedulingRequest,
    ) -> Result<SchedulingResult, SchedulingError> {
        detect_cycle(&definition)?;
        flatten_sequences(&mut definition)?;

        let mut result = SchedulingResult {
            remove_before: Some(request.start),
            clear_items: request.mode == PlayoutBuildMode::Reset,
            ..SchedulingResult::default()
        };
        if request.mode == PlayoutBuildMode::Reset {
            result.history_to_remove.extend(
                request
                    .reference_data
                    .existing_history
                    .iter()
                    .filter(|history| {
                        history.when > request.start
                            || history.finish.is_some_and(|finish| finish >= request.start)
                    })
                    .map(|history| history.id),
            );
        }

        let mut context = YamlContext::new(
            request.playout.clone(),
            request.reference_data.clone(),
            request.start,
            content_sources(&definition),
        );
        if request.mode != PlayoutBuildMode::Reset {
            if let Some(anchor) = &request.playout.anchor {
                context.restore(anchor)?;
            }
        }
        context.apply_history(&definition, &result.history_to_remove);

        if request.mode == PlayoutBuildMode::Reset {
            for instruction in &definition.reset {
                dispatch_instruction(&mut context, instruction, request.mode, &definition)?;
            }
        }

        let mut repeat_guard = 0usize;
        while context.current_time < request.finish {
            if context.instruction_index >= definition.playout.len() {
                break;
            }
            let before_items = context.added_items.len();
            let instruction = &definition.playout[context.instruction_index].clone();
            let changes_index = instruction.repeat.unwrap_or(false);

            dispatch_instruction(&mut context, instruction, request.mode, &definition)?;
            if !changes_index {
                context.instruction_index += 1;
            } else if context.added_items.len() == before_items {
                repeat_guard += 1;
                if repeat_guard > definition.playout.len().saturating_add(1) {
                    return Err(SchedulingError::InvalidYaml(
                        "YAML repeat loop did not add items".to_owned(),
                    ));
                }
            }
        }

        let max_time = context
            .added_items
            .iter()
            .map(|item| item.finish)
            .max()
            .unwrap_or(context.current_time);

        result.anchor = Some(PlayoutAnchor {
            next_start: max_time,
            next_instruction_index: i32::try_from(context.instruction_index).unwrap_or_default(),
            context: Some(context.serialize()?),
            ..PlayoutAnchor::default()
        });
        result.added_items = context.added_items;
        result.added_history = context.added_history;
        result.added_gaps = context.gaps;
        result.history_to_remove.extend(cleanup_history(
            &request.reference_data.existing_history,
            request.start,
            request.reference_data.max_playout_offset,
        ));
        result.history_to_remove.sort_unstable();
        result.history_to_remove.dedup();
        Ok(result)
    }
}

impl PlayoutSchedulingEngine for YamlPlayoutEngine {
    fn schedule(&self, request: SchedulingRequest) -> Result<SchedulingResult, SchedulingError> {
        let definition = Self::definition_for_playout(&request.playout)?;
        self.schedule_definition(definition, request)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SerializedYamlContext {
    instruction_index: usize,
    guide_group: i32,
    guide_group_locked: bool,
    watermark_ids: Vec<EntityId>,
    pre_roll_sequence: Option<String>,
}

#[derive(Debug, Clone)]
struct YamlContext {
    playout: Playout,
    reference_data: SchedulingReferenceData,
    current_time: DateTime<Utc>,
    instruction_index: usize,
    visited_instructions: BTreeSet<usize>,
    guide_group: i32,
    guide_group_locked: bool,
    watermark_ids: BTreeSet<EntityId>,
    graphics_element_ids: BTreeSet<EntityId>,
    pre_roll_sequence: Option<String>,
    post_roll_sequence: Option<String>,
    mid_roll_sequence: Option<(String, String)>,
    filler_stack: Vec<FillerKind>,
    added_items: Vec<nastv_domain::PlayoutItem>,
    added_history: Vec<nastv_domain::PlayoutHistory>,
    gaps: Vec<nastv_domain::PlayoutGap>,
    enumerators: BTreeMap<String, MediaEnumerator>,
    content_sources: BTreeMap<String, (SchedulingContentSource, PlaybackOrder)>,
}

impl YamlContext {
    fn new(
        playout: Playout,
        reference_data: SchedulingReferenceData,
        current_time: DateTime<Utc>,
        content_sources: BTreeMap<String, (SchedulingContentSource, PlaybackOrder)>,
    ) -> Self {
        Self {
            playout,
            reference_data,
            current_time,
            instruction_index: 0,
            visited_instructions: BTreeSet::new(),
            guide_group: 1,
            guide_group_locked: false,
            watermark_ids: BTreeSet::new(),
            graphics_element_ids: BTreeSet::new(),
            pre_roll_sequence: None,
            post_roll_sequence: None,
            mid_roll_sequence: None,
            filler_stack: Vec::new(),
            added_items: Vec::new(),
            added_history: Vec::new(),
            gaps: Vec::new(),
            enumerators: BTreeMap::new(),
            content_sources,
        }
    }

    fn serialize(&self) -> Result<String, SchedulingError> {
        let state = SerializedYamlContext {
            instruction_index: self.instruction_index,
            guide_group: self.guide_group,
            guide_group_locked: self.guide_group_locked,
            watermark_ids: self.watermark_ids.iter().copied().collect(),
            pre_roll_sequence: self.pre_roll_sequence.clone(),
        };
        serde_json::to_string(&state)
            .map_err(|error| SchedulingError::InvalidYaml(error.to_string()))
    }

    fn restore(&mut self, anchor: &PlayoutAnchor) -> Result<(), SchedulingError> {
        self.current_time = anchor.next_start;
        if let Some(context) = &anchor.context {
            if !context.trim().is_empty() {
                let state: SerializedYamlContext = serde_json::from_str(context)
                    .map_err(|error| SchedulingError::InvalidYaml(error.to_string()))?;
                self.instruction_index = state.instruction_index;
                self.guide_group = state.guide_group;
                self.guide_group_locked = state.guide_group_locked;
                self.watermark_ids = state.watermark_ids.into_iter().collect();
                self.pre_roll_sequence = state.pre_roll_sequence;
            }
        }
        Ok(())
    }

    fn enumerator(&mut self, content: &str) -> &mut MediaEnumerator {
        let (source, _) = self
            .content_sources
            .get(content)
            .cloned()
            .unwrap_or_else(|| {
                (
                    SchedulingContentSource::direct(content),
                    PlaybackOrder::None,
                )
            });
        self.enumerators
            .entry(content.to_owned())
            .or_insert_with(|| lookup_media_source(&self.reference_data, content, &source))
    }

    fn history_key(&self, content: &str) -> String {
        let playback_order = self
            .content_sources
            .get(content)
            .map_or(PlaybackOrder::None, |(_, order)| *order);
        history_key_for_content(content, playback_order)
    }

    fn apply_history(
        &mut self,
        definition: &YamlPlayoutDefinition,
        history_to_remove: &[EntityId],
    ) {
        let ignored = history_to_remove.iter().copied().collect::<BTreeSet<_>>();
        for content in &definition.content {
            let history_key = history_key_for_content(&content.key, content.playback_order());
            let prior_entries = self
                .reference_data
                .existing_history
                .iter()
                .filter(|history| !ignored.contains(&history.id))
                .filter(|history| history.key == history_key)
                .filter(|history| history.when < self.current_time)
                .count();
            if prior_entries > 0 {
                self.enumerator(&content.key).set_index(prior_entries);
            }
        }
    }

    fn peek_guide_group(&self) -> i32 {
        if self.guide_group_locked {
            self.guide_group
        } else if self.guide_group >= 1000 {
            1
        } else {
            self.guide_group + 1
        }
    }

    fn advance_guide_group(&mut self) {
        if !self.guide_group_locked {
            self.guide_group += 1;
            if self.guide_group > 1000 {
                self.guide_group = 1;
            }
        }
    }

    fn current_filler_kind(&self, instruction: &YamlInstruction) -> FillerKind {
        if let Some(kind) = self.filler_stack.last() {
            *kind
        } else if let Some(kind) = &instruction.filler_kind {
            parse_filler_kind(kind)
        } else {
            FillerKind::None
        }
    }
}

fn dispatch_instruction(
    context: &mut YamlContext,
    instruction: &YamlInstruction,
    mode: PlayoutBuildMode,
    definition: &YamlPlayoutDefinition,
) -> Result<(), SchedulingError> {
    context
        .visited_instructions
        .insert(context.instruction_index);
    if instruction.wait_until.is_some() {
        handle_wait_until(context, instruction, mode)
    } else if instruction.rewind.is_some() {
        handle_rewind(context, instruction)
    } else if instruction.epg_group.is_some() {
        handle_epg_group(context, instruction);
        Ok(())
    } else if instruction.watermark.is_some() {
        handle_watermark(context, instruction);
        Ok(())
    } else if instruction.graphics_on.is_some() {
        handle_graphics_on(context, instruction)?;
        Ok(())
    } else if instruction.graphics_off.is_some() {
        handle_graphics_off(context, instruction);
        Ok(())
    } else if instruction.pre_roll.is_some() {
        context.pre_roll_sequence =
            enabled_sequence(instruction.pre_roll, instruction.sequence.as_ref());
        Ok(())
    } else if instruction.post_roll.is_some() {
        context.post_roll_sequence =
            enabled_sequence(instruction.post_roll, instruction.sequence.as_ref());
        Ok(())
    } else if instruction.mid_roll.is_some() {
        context.mid_roll_sequence =
            enabled_sequence(instruction.mid_roll, instruction.sequence.as_ref())
                .map(|sequence| (sequence, instruction.expression.clone()));
        Ok(())
    } else if instruction.repeat.unwrap_or(false) {
        context.instruction_index = 0;
        Ok(())
    } else if let Some(sequence) = &instruction.sequence {
        execute_sequence(context, definition, sequence)
    } else if instruction.shuffle_sequence.is_some() {
        Ok(())
    } else if instruction.skip_items.is_some() {
        handle_skip_items(context, instruction);
        Ok(())
    } else if instruction.skip_to_item.is_some() {
        handle_skip_to_item(context, instruction);
        Ok(())
    } else if instruction.all.is_some() {
        handle_all(context, instruction, definition)
    } else if instruction.count.is_some() {
        handle_count(context, instruction, definition)
    } else if instruction.duration.is_some() {
        handle_duration(context, instruction, definition)
    } else if instruction.pad_to_next.is_some() {
        handle_pad_to_next(context, instruction, definition)
    } else if instruction.pad_until.is_some() {
        handle_pad_until(context, instruction, definition)
    } else {
        Err(SchedulingError::UnsupportedYamlInstruction)
    }
}

fn execute_sequence(
    context: &mut YamlContext,
    definition: &YamlPlayoutDefinition,
    sequence: &str,
) -> Result<(), SchedulingError> {
    let Some(sequence) = definition.sequence.iter().find(|item| item.key == sequence) else {
        return Ok(());
    };
    for instruction in &sequence.items {
        dispatch_instruction(context, instruction, PlayoutBuildMode::Continue, definition)?;
    }
    Ok(())
}

fn handle_wait_until(
    context: &mut YamlContext,
    instruction: &YamlInstruction,
    mode: PlayoutBuildMode,
) -> Result<(), SchedulingError> {
    let Some(wait_until) = &instruction.wait_until else {
        return Ok(());
    };
    let (hour, minute, second) = parse_hhmm(wait_until)?;
    let current_time = context.current_time;
    let current_seconds = current_time.num_seconds_from_midnight();
    let target_seconds = hour * 3600 + minute * 60 + second;
    let mut date = current_time.date_naive();
    if current_seconds > target_seconds {
        if instruction.tomorrow == Some("true".to_owned()) {
            date = date
                .succ_opt()
                .ok_or_else(|| SchedulingError::InvalidTime(wait_until.clone()))?;
        } else if !(instruction.rewind_on_reset && mode == PlayoutBuildMode::Reset) {
            return Ok(());
        }
    }
    let target = date
        .and_hms_opt(hour, minute, second)
        .ok_or_else(|| SchedulingError::InvalidTime(wait_until.clone()))?;
    context.current_time = DateTime::from_naive_utc_and_offset(target, Utc);
    Ok(())
}

fn handle_rewind(
    context: &mut YamlContext,
    instruction: &YamlInstruction,
) -> Result<(), SchedulingError> {
    if let Some(rewind) = &instruction.rewind {
        context.current_time -= parse_duration(rewind)?;
    }
    Ok(())
}

fn handle_epg_group(context: &mut YamlContext, instruction: &YamlInstruction) {
    if instruction.epg_group.unwrap_or(false) {
        if instruction.advance.unwrap_or(true) {
            context.advance_guide_group();
        }
        context.guide_group_locked = true;
    } else {
        context.guide_group_locked = false;
    }
}

fn handle_watermark(context: &mut YamlContext, instruction: &YamlInstruction) {
    if instruction.watermark.unwrap_or(false) {
        if let Some(name) = &instruction.name {
            if let Some(id) = context.reference_data.watermark_ids_by_name.get(name) {
                context.watermark_ids.insert(*id);
            }
        }
    } else if let Some(name) = &instruction.name {
        if let Some(id) = context.reference_data.watermark_ids_by_name.get(name) {
            context.watermark_ids.remove(id);
        }
    } else {
        context.watermark_ids.clear();
    }
}

fn handle_graphics_on(
    context: &mut YamlContext,
    instruction: &YamlInstruction,
) -> Result<(), SchedulingError> {
    if let Some(path) = &instruction.graphics_on {
        if let Some(id) = context.reference_data.graphics_ids_by_path.get(path) {
            context.graphics_element_ids.insert(*id);
        }
        serde_json::to_string(&instruction.variables)
            .map_err(|error| SchedulingError::InvalidYaml(error.to_string()))?;
    }
    Ok(())
}

fn handle_graphics_off(context: &mut YamlContext, instruction: &YamlInstruction) {
    if let Some(path) = &instruction.graphics_off {
        if path.trim().is_empty() {
            context.graphics_element_ids.clear();
        } else if let Some(id) = context.reference_data.graphics_ids_by_path.get(path) {
            context.graphics_element_ids.remove(id);
        }
    }
}

fn enabled_sequence(enabled: Option<bool>, sequence: Option<&String>) -> Option<String> {
    if enabled.unwrap_or(false) {
        sequence.filter(|value| !value.trim().is_empty()).cloned()
    } else {
        None
    }
}

fn handle_skip_items(context: &mut YamlContext, instruction: &YamlInstruction) {
    let Some(content) = instruction.content.as_deref() else {
        return;
    };
    let skip = instruction
        .skip_items
        .as_deref()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or_default();
    let enumerator = context.enumerator(content);
    for _ in 0..skip.min(enumerator.len()) {
        enumerator.advance();
    }
}

fn handle_skip_to_item(context: &mut YamlContext, instruction: &YamlInstruction) {
    let Some(content) = instruction.content.as_deref() else {
        return;
    };
    let season = instruction.season.unwrap_or_default();
    let episode = instruction.episode.unwrap_or_default();
    let enumerator = context.enumerator(content);
    for _ in 0..enumerator.len() {
        if enumerator
            .current()
            .is_some_and(|item| item.season == Some(season) && item.episode == Some(episode))
        {
            break;
        }
        enumerator.advance();
    }
}

fn handle_all(
    context: &mut YamlContext,
    instruction: &YamlInstruction,
    definition: &YamlPlayoutDefinition,
) -> Result<(), SchedulingError> {
    let Some(content) = instruction
        .content
        .as_deref()
        .or(instruction.all.as_deref())
    else {
        return Ok(());
    };
    let count = context.enumerator(content).len();
    for _ in 0..count {
        schedule_next_with_rolls(context, instruction, definition, content)?;
    }
    Ok(())
}

fn handle_count(
    context: &mut YamlContext,
    instruction: &YamlInstruction,
    definition: &YamlPlayoutDefinition,
) -> Result<(), SchedulingError> {
    let Some(content) = instruction.content.as_deref() else {
        return Ok(());
    };
    let count = instruction
        .count
        .as_deref()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or_default();
    for _ in 0..count {
        schedule_next_with_rolls(context, instruction, definition, content)?;
    }
    Ok(())
}

fn handle_duration(
    context: &mut YamlContext,
    instruction: &YamlInstruction,
    definition: &YamlPlayoutDefinition,
) -> Result<(), SchedulingError> {
    let duration = parse_duration(instruction.duration.as_deref().unwrap_or_default())?;
    let target = context.current_time + duration;
    schedule_until(context, instruction, definition, target)
}

fn handle_pad_to_next(
    context: &mut YamlContext,
    instruction: &YamlInstruction,
    definition: &YamlPlayoutDefinition,
) -> Result<(), SchedulingError> {
    let interval = instruction.pad_to_next.unwrap_or(1).max(1);
    let current_minute = i64::from(context.current_time.minute());
    let target_minute = ((current_minute + interval - 1) / interval) * interval;
    let base = context.current_time
        - Duration::minutes(current_minute)
        - Duration::seconds(i64::from(context.current_time.second()));
    let mut target = base + Duration::minutes(target_minute);
    if target <= context.current_time {
        target += Duration::minutes(interval);
    }
    schedule_until(context, instruction, definition, target)
}

fn handle_pad_until(
    context: &mut YamlContext,
    instruction: &YamlInstruction,
    definition: &YamlPlayoutDefinition,
) -> Result<(), SchedulingError> {
    let value = instruction.pad_until.as_deref().unwrap_or_default();
    let (hour, minute, second) = parse_hhmm(value)?;
    let mut date = context.current_time.date_naive();
    let target_seconds = hour * 3600 + minute * 60 + second;
    if context.current_time.num_seconds_from_midnight() > target_seconds
        && instruction.tomorrow.as_deref().unwrap_or("false") == "true"
    {
        date = date
            .succ_opt()
            .ok_or_else(|| SchedulingError::InvalidTime(value.to_owned()))?;
    }
    let target = date
        .and_hms_opt(hour, minute, second)
        .ok_or_else(|| SchedulingError::InvalidTime(value.to_owned()))?;
    schedule_until(
        context,
        instruction,
        definition,
        DateTime::from_naive_utc_and_offset(target, Utc),
    )
}

fn schedule_until(
    context: &mut YamlContext,
    instruction: &YamlInstruction,
    definition: &YamlPlayoutDefinition,
    target: DateTime<Utc>,
) -> Result<(), SchedulingError> {
    let Some(content) = instruction.content.as_deref() else {
        return Ok(());
    };
    while context.current_time < target {
        let Some(media_item) = context.enumerator(content).current().cloned() else {
            break;
        };
        let remaining = target - context.current_time;
        if media_item.duration <= remaining || !instruction.stop_before_end {
            schedule_next_with_rolls(context, instruction, definition, content)?;
        } else if instruction.discard_attempts > 0 {
            context.enumerator(content).advance();
        } else if instruction.trim {
            add_media_item(context, instruction, content, &media_item, remaining);
            context.enumerator(content).advance();
        } else if let Some(fallback) = &instruction.fallback {
            let fallback_item = context.enumerator(fallback).current().cloned();
            if let Some(fallback_item) = fallback_item {
                let duration = fallback_item.duration.min(remaining);
                let mut fallback_instruction = instruction.clone();
                fallback_instruction.filler_kind = Some("Fallback".to_owned());
                add_media_item(
                    context,
                    &fallback_instruction,
                    fallback,
                    &fallback_item,
                    duration,
                );
                context.enumerator(fallback).advance();
            }
            context.current_time = target;
        } else {
            let gap = nastv_domain::PlayoutGap {
                playout_id: context.playout.id,
                start: context.current_time,
                finish: target,
                ..nastv_domain::PlayoutGap::default()
            };
            context.gaps.push(gap);
            context.current_time = target;
        }
    }
    Ok(())
}

fn schedule_next_with_rolls(
    context: &mut YamlContext,
    instruction: &YamlInstruction,
    definition: &YamlPlayoutDefinition,
    content: &str,
) -> Result<(), SchedulingError> {
    if context.filler_stack.is_empty() {
        if let Some(sequence) = context.pre_roll_sequence.clone() {
            context.filler_stack.push(FillerKind::PreRoll);
            execute_sequence(context, definition, &sequence)?;
            context.filler_stack.pop();
        }
    }

    let media_item = context.enumerator(content).current().cloned();
    if let Some(media_item) = media_item {
        add_media_item(
            context,
            instruction,
            content,
            &media_item,
            media_item.duration,
        );
        context.enumerator(content).advance();
    }

    if context.filler_stack.is_empty() {
        if let Some((sequence, expression)) = context.mid_roll_sequence.clone() {
            if expression.trim().eq_ignore_ascii_case("true") {
                context.filler_stack.push(FillerKind::MidRoll);
                execute_sequence(context, definition, &sequence)?;
                context.filler_stack.pop();
            }
        }
    }

    if context.filler_stack.is_empty() {
        if let Some(sequence) = context.post_roll_sequence.clone() {
            context.filler_stack.push(FillerKind::PostRoll);
            execute_sequence(context, definition, &sequence)?;
            context.filler_stack.pop();
        }
    }
    Ok(())
}

fn add_media_item(
    context: &mut YamlContext,
    instruction: &YamlInstruction,
    content: &str,
    media_item: &SchedulingMediaItem,
    duration: Duration,
) {
    let item = playout_item(
        context.playout.id,
        media_item,
        context.current_time,
        duration,
        context.peek_guide_group(),
        context.current_filler_kind(instruction),
        instruction.custom_title.clone(),
        instruction.disable_watermarks,
        context.watermark_ids.iter().copied().collect(),
        context.graphics_element_ids.iter().copied().collect(),
        Some(format!(
            r#"{{"content":"{}","instructionIndex":{}}}"#,
            content, context.instruction_index
        )),
    );
    context.added_history.push(history(
        context.playout.id,
        context.history_key(content),
        &item,
        None,
    ));
    context.current_time = item.finish;
    context.added_items.push(item);
    context.advance_guide_group();
}

fn content_sources(
    definition: &YamlPlayoutDefinition,
) -> BTreeMap<String, (SchedulingContentSource, PlaybackOrder)> {
    definition
        .content
        .iter()
        .map(|content| {
            (
                content.key.clone(),
                (content.source(), content.playback_order()),
            )
        })
        .collect()
}

fn parse_playback_order(value: &str) -> PlaybackOrder {
    match value.to_ascii_lowercase().as_str() {
        "chronological" => PlaybackOrder::Chronological,
        "random" => PlaybackOrder::Random,
        "shuffle" => PlaybackOrder::Shuffle,
        "shuffleinorder" | "shuffle_in_order" => PlaybackOrder::ShuffleInOrder,
        "multiepisodeshuffle" | "multi_episode_shuffle" => PlaybackOrder::MultiEpisodeShuffle,
        "seasonepisode" | "season_episode" => PlaybackOrder::SeasonEpisode,
        "randomrotation" | "random_rotation" => PlaybackOrder::RandomRotation,
        "marathon" => PlaybackOrder::Marathon,
        _ => PlaybackOrder::None,
    }
}

fn parse_filler_kind(value: &str) -> FillerKind {
    match value.to_ascii_lowercase().as_str() {
        "preroll" | "pre_roll" => FillerKind::PreRoll,
        "midroll" | "mid_roll" => FillerKind::MidRoll,
        "postroll" | "post_roll" => FillerKind::PostRoll,
        "tail" => FillerKind::Tail,
        "fallback" => FillerKind::Fallback,
        "guidemode" | "guide_mode" => FillerKind::GuideMode,
        _ => FillerKind::None,
    }
}

fn detect_cycle(definition: &YamlPlayoutDefinition) -> Result<(), SchedulingError> {
    fn visit<'a>(
        key: &'a str,
        graph: &'a BTreeMap<&'a str, Vec<&'a str>>,
        visiting: &mut BTreeSet<&'a str>,
        visited: &mut BTreeSet<&'a str>,
    ) -> bool {
        if visiting.contains(key) {
            return true;
        }
        if !visited.insert(key) {
            return false;
        }
        visiting.insert(key);
        for child in graph.get(key).into_iter().flatten() {
            if visit(child, graph, visiting, visited) {
                return true;
            }
        }
        visiting.remove(key);
        false
    }

    let mut graph = BTreeMap::<&str, Vec<&str>>::new();
    for sequence in &definition.sequence {
        let children = sequence
            .items
            .iter()
            .filter_map(|instruction| instruction.sequence.as_deref())
            .collect::<Vec<_>>();
        graph.insert(&sequence.key, children);
    }
    let mut visited = BTreeSet::new();
    for key in graph.keys() {
        if visit(key, &graph, &mut BTreeSet::new(), &mut visited) {
            return Err(SchedulingError::SequenceCycle);
        }
    }
    Ok(())
}

fn flatten_sequences(definition: &mut YamlPlayoutDefinition) -> Result<(), SchedulingError> {
    let mut depth = 0;
    while definition.playout.iter().any(is_sequence_instruction) {
        if depth > 100 {
            return Err(SchedulingError::SequenceNestingTooDeep);
        }
        let original = definition.playout.clone();
        definition.playout.clear();
        for instruction in original {
            if is_sequence_instruction(&instruction) {
                let sequence_key = instruction.sequence.as_ref().expect("sequence exists");
                let repeat = instruction
                    .count
                    .as_deref()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1);
                if let Some(sequence) = definition
                    .sequence
                    .iter()
                    .find(|sequence| &sequence.key == sequence_key)
                {
                    for _ in 0..repeat {
                        definition.playout.extend(sequence.items.clone());
                    }
                }
            } else {
                definition.playout.push(instruction);
            }
        }
        depth += 1;
    }
    Ok(())
}

fn is_sequence_instruction(instruction: &YamlInstruction) -> bool {
    instruction.sequence.is_some()
        && instruction.pre_roll.is_none()
        && instruction.post_roll.is_none()
        && instruction.mid_roll.is_none()
}

#[cfg(test)]
#[allow(clippy::needless_raw_string_hashes)]
mod tests {
    use super::*;
    use crate::scheduling::{SchedulingMediaItem, SchedulingReferenceData};
    use nastv_domain::Playout;

    fn request(yaml: &str) -> (YamlPlayoutDefinition, SchedulingRequest) {
        let mut reference_data = SchedulingReferenceData::default();
        reference_data.media_by_key.insert(
            "main".to_owned(),
            vec![
                SchedulingMediaItem::new(1, Duration::minutes(10)),
                SchedulingMediaItem::new(2, Duration::minutes(15)),
            ],
        );
        reference_data.media_by_key.insert(
            "bumpers".to_owned(),
            vec![SchedulingMediaItem::new(9, Duration::minutes(1))],
        );
        reference_data
            .watermark_ids_by_name
            .insert("bug".to_owned(), 42);
        reference_data
            .graphics_ids_by_path
            .insert("lower-third".to_owned(), 77);
        let definition = YamlPlayoutEngine::definition_from_str(yaml).expect("valid yaml");
        let start = DateTime::parse_from_rfc3339("2026-04-15T10:07:00Z")
            .expect("valid time")
            .with_timezone(&Utc);
        (
            definition,
            SchedulingRequest {
                playout: Playout {
                    id: 100,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::hours(2),
                mode: PlayoutBuildMode::Reset,
                reference_data,
            },
        )
    }

    fn snapshot(yaml: &str) -> serde_json::Value {
        let (definition, request) = request(yaml);
        let result = YamlPlayoutEngine
            .schedule_definition(definition, request)
            .expect("schedule builds");
        serde_json::json!({
            "items": result.added_items.iter().map(|item| serde_json::json!({
                "media": item.media_item_id,
                "start": item.start.to_rfc3339(),
                "finish": item.finish.to_rfc3339(),
                "filler": format!("{:?}", item.filler_kind),
                "guide_group": item.guide_group,
                "watermarks": item.watermark_ids,
                "graphics": item.graphics_element_ids,
                "title": item.custom_title,
            })).collect::<Vec<_>>(),
            "gaps": result.added_gaps.iter().map(|gap| serde_json::json!({
                "start": gap.start.to_rfc3339(),
                "finish": gap.finish.to_rfc3339(),
            })).collect::<Vec<_>>(),
            "anchor": result.anchor.map(|anchor| anchor.next_start.to_rfc3339()),
        })
    }

    #[test]
    fn yaml_count_instruction_snapshot() {
        insta::assert_json_snapshot!(snapshot(
            r#"
playout:
  - count: "2"
    content: main
"#
        ));
    }

    #[test]
    fn yaml_duration_instruction_snapshot() {
        insta::assert_json_snapshot!(snapshot(
            r#"
playout:
  - duration: "00:20:00"
    content: main
    trim: true
"#
        ));
    }

    #[test]
    fn yaml_all_instruction_snapshot() {
        insta::assert_json_snapshot!(snapshot(
            r#"
playout:
  - all: main
"#
        ));
    }

    #[test]
    fn yaml_content_sources_snapshot() {
        let mut reference_data = SchedulingReferenceData::default();
        reference_data.collections_by_name.insert(
            "Movies".to_owned(),
            vec![SchedulingMediaItem::new(10, Duration::minutes(10))],
        );
        reference_data.multi_collections_by_name.insert(
            "Saturday".to_owned(),
            vec![SchedulingMediaItem::new(20, Duration::minutes(10))],
        );
        reference_data.smart_collections_by_name.insert(
            "Unwatched".to_owned(),
            vec![SchedulingMediaItem::new(30, Duration::minutes(10))],
        );
        reference_data.playlists_by_group_and_name.insert(
            (Some("Prime".to_owned()), "Block A".to_owned()),
            vec![SchedulingMediaItem::new(40, Duration::minutes(10))],
        );
        reference_data.searches_by_query.insert(
            "genre = horror".to_owned(),
            vec![SchedulingMediaItem::new(50, Duration::minutes(10))],
        );
        reference_data
            .media_items_by_id
            .insert(60, SchedulingMediaItem::new(60, Duration::minutes(10)));

        let definition = YamlPlayoutEngine::definition_from_str(
            r#"
content:
  - key: collection
    collection: Movies
    order: chronological
  - key: multi
    multi_collection: Saturday
    order: shuffle
  - key: smart
    smart_collection: Unwatched
    order: shuffle
  - key: playlist
    playlist_group: Prime
    playlist: Block A
  - key: search
    search: genre = horror
    order: shuffle
  - key: single
    media_item: 60
    order: none
playout:
  - count: "1"
    content: collection
  - count: "1"
    content: multi
  - count: "1"
    content: smart
  - count: "1"
    content: playlist
  - count: "1"
    content: search
  - count: "1"
    content: single
"#,
        )
        .expect("valid yaml");
        let start = DateTime::parse_from_rfc3339("2026-04-15T10:00:00Z")
            .expect("valid time")
            .with_timezone(&Utc);

        let result = YamlPlayoutEngine
            .schedule_definition(
                definition,
                SchedulingRequest {
                    playout: Playout {
                        id: 100,
                        ..Playout::default()
                    },
                    start,
                    finish: start + Duration::hours(2),
                    mode: PlayoutBuildMode::Reset,
                    reference_data,
                },
            )
            .expect("schedule builds");

        insta::assert_json_snapshot!(serde_json::json!({
            "media": result.added_items.iter().map(|item| item.media_item_id).collect::<Vec<_>>(),
            "history": result.added_history.iter().map(|history| &history.key).collect::<Vec<_>>(),
        }));
    }

    #[test]
    fn yaml_pad_to_next_instruction_snapshot() {
        insta::assert_json_snapshot!(snapshot(
            r#"
playout:
  - pad_to_next: 15
    content: bumpers
    trim: true
"#
        ));
    }

    #[test]
    fn yaml_pad_until_instruction_snapshot() {
        insta::assert_json_snapshot!(snapshot(
            r#"
playout:
  - pad_until: "10:10"
    content: bumpers
    trim: true
"#
        ));
    }

    #[test]
    fn yaml_wait_until_instruction_snapshot() {
        insta::assert_json_snapshot!(snapshot(
            r#"
playout:
  - wait_until: "10:30"
  - count: "1"
    content: main
"#
        ));
    }

    #[test]
    fn yaml_rewind_instruction_snapshot() {
        insta::assert_json_snapshot!(snapshot(
            r#"
playout:
  - count: "1"
    content: main
  - rewind: "00:05:00"
  - count: "1"
    content: main
"#
        ));
    }

    #[test]
    fn yaml_pre_and_post_roll_instruction_snapshot() {
        insta::assert_json_snapshot!(snapshot(
            r#"
sequence:
  - key: bumper
    items:
      - count: "1"
        content: bumpers
playout:
  - pre_roll: true
    sequence: bumper
  - post_roll: true
    sequence: bumper
  - count: "1"
    content: main
"#
        ));
    }

    #[test]
    fn yaml_mid_roll_instruction_snapshot() {
        insta::assert_json_snapshot!(snapshot(
            r#"
sequence:
  - key: bumper
    items:
      - count: "1"
        content: bumpers
playout:
  - mid_roll: true
    sequence: bumper
  - count: "1"
    content: main
"#
        ));
    }

    #[test]
    fn yaml_watermark_instruction_snapshot() {
        insta::assert_json_snapshot!(snapshot(
            r#"
playout:
  - watermark: true
    name: bug
  - count: "1"
    content: main
  - watermark: false
    name: bug
  - count: "1"
    content: main
"#
        ));
    }

    #[test]
    fn yaml_graphics_instruction_snapshot() {
        insta::assert_json_snapshot!(snapshot(
            r#"
playout:
  - graphics_on: lower-third
    variables:
      title: Now
  - count: "1"
    content: main
  - graphics_off: lower-third
  - count: "1"
    content: main
"#
        ));
    }

    #[test]
    fn yaml_epg_group_instruction_snapshot() {
        insta::assert_json_snapshot!(snapshot(
            r#"
playout:
  - epg_group: true
  - count: "2"
    content: main
  - epg_group: false
  - count: "1"
    content: main
"#
        ));
    }

    #[test]
    fn yaml_skip_items_instruction_snapshot() {
        insta::assert_json_snapshot!(snapshot(
            r#"
playout:
  - skip_items: "1"
    content: main
  - count: "1"
    content: main
"#
        ));
    }

    #[test]
    fn yaml_skip_to_item_instruction_snapshot() {
        let mut reference_data = SchedulingReferenceData::default();
        reference_data.media_by_key.insert(
            "main".to_owned(),
            vec![
                SchedulingMediaItem {
                    season: Some(1),
                    episode: Some(1),
                    ..SchedulingMediaItem::new(1, Duration::minutes(10))
                },
                SchedulingMediaItem {
                    season: Some(1),
                    episode: Some(2),
                    ..SchedulingMediaItem::new(2, Duration::minutes(10))
                },
            ],
        );
        let definition = YamlPlayoutEngine::definition_from_str(
            r#"
playout:
  - skip_to_item: main
    content: main
    season: 1
    episode: 2
  - count: "1"
    content: main
"#,
        )
        .expect("valid yaml");
        let start = DateTime::parse_from_rfc3339("2026-04-15T10:00:00Z")
            .expect("valid time")
            .with_timezone(&Utc);
        let result = YamlPlayoutEngine
            .schedule_definition(
                definition,
                SchedulingRequest {
                    playout: Playout {
                        id: 100,
                        ..Playout::default()
                    },
                    start,
                    finish: start + Duration::hours(1),
                    mode: PlayoutBuildMode::Reset,
                    reference_data,
                },
            )
            .expect("schedule builds");
        insta::assert_json_snapshot!(serde_json::json!({
            "media": result.added_items.iter().map(|item| item.media_item_id).collect::<Vec<_>>()
        }));
    }

    #[test]
    fn yaml_apply_history_and_cleanup_snapshot() {
        let start = DateTime::parse_from_rfc3339("2026-04-15T10:00:00Z")
            .expect("valid time")
            .with_timezone(&Utc);
        let history_key = history_key_for_content("collection", PlaybackOrder::Chronological);
        let mut reference_data = SchedulingReferenceData {
            max_playout_offset: Duration::zero(),
            ..SchedulingReferenceData::default()
        };
        reference_data.collections_by_name.insert(
            "Movies".to_owned(),
            vec![
                SchedulingMediaItem::new(1, Duration::minutes(10)),
                SchedulingMediaItem::new(2, Duration::minutes(10)),
                SchedulingMediaItem::new(3, Duration::minutes(10)),
            ],
        );
        reference_data.existing_history = vec![
            nastv_domain::PlayoutHistory {
                id: 10,
                key: history_key.clone(),
                when: start - Duration::hours(3),
                finish: Some(start - Duration::hours(3) + Duration::minutes(10)),
                ..nastv_domain::PlayoutHistory::default()
            },
            nastv_domain::PlayoutHistory {
                id: 11,
                key: history_key.clone(),
                when: start - Duration::hours(2),
                finish: Some(start - Duration::hours(2) + Duration::minutes(10)),
                ..nastv_domain::PlayoutHistory::default()
            },
            nastv_domain::PlayoutHistory {
                id: 12,
                key: history_key,
                when: start,
                finish: Some(start + Duration::minutes(5)),
                ..nastv_domain::PlayoutHistory::default()
            },
        ];

        let definition = YamlPlayoutEngine::definition_from_str(
            r#"
content:
  - key: collection
    collection: Movies
    order: chronological
playout:
  - count: "1"
    content: collection
"#,
        )
        .expect("valid yaml");
        let result = YamlPlayoutEngine
            .schedule_definition(
                definition,
                SchedulingRequest {
                    playout: Playout {
                        id: 100,
                        ..Playout::default()
                    },
                    start,
                    finish: start + Duration::hours(1),
                    mode: PlayoutBuildMode::Reset,
                    reference_data,
                },
            )
            .expect("schedule builds");

        insta::assert_json_snapshot!(serde_json::json!({
            "media": result.added_items.iter().map(|item| item.media_item_id).collect::<Vec<_>>(),
            "history_to_remove": result.history_to_remove,
        }));
    }

    #[test]
    fn yaml_repeat_instruction_detects_empty_loop() {
        let (definition, request) = request(
            r#"
playout:
  - repeat: true
"#,
        );
        let result = YamlPlayoutEngine.schedule_definition(definition, request);
        assert!(result.is_err());
    }
}
