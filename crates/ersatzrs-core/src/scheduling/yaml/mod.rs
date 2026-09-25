#![allow(clippy::similar_names)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
use ersatzrs_domain::{EntityId, FillerKind, PlaybackOrder, Playout, PlayoutAnchor, PlayoutItem};
use serde::{Deserialize, Serialize};

use super::{
    cleanup_history, history, history_details_for_media_item, history_key_for_content,
    lookup_media_source, parse_duration, parse_hhmm, playout_item, shuffle_with_seed,
    CurrentHistoryContext, MediaEnumerator, PlaylistEnumerator, PlayoutBuildMode,
    PlayoutSchedulingEngine, SchedulingContentGuid, SchedulingContentResolver,
    SchedulingContentSource, SchedulingError, SchedulingMediaItem, SchedulingPlaylistItem,
    SchedulingReferenceData, SchedulingRequest, SchedulingResult,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub struct YamlContentGuid {
    pub source: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct YamlPlayoutDefinition {
    #[serde(default)]
    pub import: Vec<String>,
    #[serde(default, rename = "cooldown", skip_serializing)]
    pub legacy_smart_shuffle: super::SchedulingSmartShufflePolicy,
    #[serde(default)]
    pub content: Vec<YamlContentItem>,
    #[serde(default)]
    pub sequence: Vec<YamlPlayoutSequenceItem>,
    #[serde(default)]
    pub reset: Vec<YamlInstruction>,
    #[serde(default)]
    pub playout: Vec<YamlInstruction>,
    #[serde(default)]
    pub schedules: Vec<YamlPlayoutScheduleItem>,
}

/// A named date-range override for the default Sequential YAML playout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct YamlPlayoutScheduleItem {
    pub name: String,
    pub start_date: String,
    pub end_date: String,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub playout: Vec<YamlInstruction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct YamlContentItem {
    pub key: String,
    #[serde(
        default,
        skip_serializing_if = "super::SchedulingSmartShufflePolicy::is_disabled"
    )]
    pub smart_shuffle: super::SchedulingSmartShufflePolicy,
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
    pub guids: Vec<YamlContentGuid>,
    #[serde(default)]
    pub marathon: Option<String>,
    #[serde(default)]
    pub searches: Vec<String>,
    #[serde(default)]
    pub group_by: Option<String>,
    #[serde(default)]
    pub shuffle_groups: bool,
    #[serde(default)]
    pub item_order: Option<String>,
    #[serde(default)]
    pub play_all_items: bool,
    #[serde(default)]
    pub media_item: Option<EntityId>,
    #[serde(default)]
    pub item: Option<EntityId>,
}

impl YamlContentItem {
    fn playback_order(&self) -> PlaybackOrder {
        self.order.as_deref().map_or_else(
            || {
                if self.playlist.is_some() || self.marathon.is_some() {
                    PlaybackOrder::None
                } else {
                    PlaybackOrder::Shuffle
                }
            },
            parse_playback_order,
        )
    }

    fn marathon_item_order(&self) -> PlaybackOrder {
        self.item_order
            .as_deref()
            .map_or(PlaybackOrder::Shuffle, parse_playback_order)
    }

    fn scheduling_guids(&self) -> Vec<SchedulingContentGuid> {
        self.guids
            .iter()
            .map(|guid| SchedulingContentGuid {
                source: guid.source.clone(),
                value: guid.value.clone(),
            })
            .collect()
    }

    fn history_key(&self) -> String {
        if self.marathon.is_some() {
            serde_json::json!({
                "Key": self.key,
                "Order": format!("{:?}", PlaybackOrder::None),
                "ItemOrder": format!("{:?}", self.marathon_item_order()),
                "GroupBy": self.group_by.clone().unwrap_or_default(),
            })
            .to_string()
        } else {
            history_key_for_content(&self.key, self.playback_order())
        }
    }

    pub fn source(&self) -> SchedulingContentSource {
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
        } else if self.marathon.is_some() {
            SchedulingContentSource::Marathon {
                name: self.marathon.clone(),
                guids: self.scheduling_guids(),
                searches: self.searches.clone(),
                group_by: self.group_by.clone().unwrap_or_default(),
                shuffle_groups: self.shuffle_groups,
                item_order: self.marathon_item_order(),
                play_all_items: self.play_all_items,
            }
        } else if self.show.is_some() || !self.guids.is_empty() {
            SchedulingContentSource::Show {
                name: self.show.clone(),
                guids: self.scheduling_guids(),
            }
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
    pub repeat: Option<YamlRepeat>,
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

/// `repeat` is a Boolean control instruction or a sequence repetition count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum YamlRepeat {
    Control(bool),
    Sequence(i32),
}

impl YamlRepeat {
    fn is_control(self) -> bool {
        matches!(self, Self::Control(true))
    }

    fn sequence_count(self) -> usize {
        match self {
            Self::Sequence(count) if count > 0 => usize::try_from(count).unwrap_or(1),
            _ => 1,
        }
    }
}

mod expressions;
mod instructions;
mod playlist_history;
mod schedules;
mod sequences;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

pub(super) use expressions::eval_count_expression;
use instructions::dispatch_instruction;
#[cfg(test)]
use instructions::YamlInstructionKind;
use schedules::select_schedule;
use sequences::{detect_cycle, flatten_sequences};

#[derive(Debug, Clone, Default)]
pub struct YamlPlayoutEngine;

impl YamlPlayoutEngine {
    /// Parse a sequential YAML playout definition.
    ///
    /// # Errors
    ///
    /// Returns an error when the YAML cannot be deserialized.
    pub fn definition_from_str(yaml: &str) -> Result<YamlPlayoutDefinition, SchedulingError> {
        let value: serde_yaml::Value = serde_yaml::from_str(yaml)
            .map_err(|error| SchedulingError::InvalidYaml(error.to_string()))?;
        let explicit_item_policies = yaml_content_smart_shuffle_presence(&value);
        let mut definition: YamlPlayoutDefinition = serde_yaml::from_str(yaml)
            .map_err(|error| SchedulingError::InvalidYaml(error.to_string()))?;
        if definition.legacy_smart_shuffle.is_active() {
            for (index, content) in definition.content.iter_mut().enumerate() {
                if !explicit_item_policies.get(index).copied().unwrap_or(false)
                    && content.smart_shuffle.is_disabled()
                {
                    content.smart_shuffle = definition.legacy_smart_shuffle.clone();
                }
            }
            definition.legacy_smart_shuffle = super::SchedulingSmartShufflePolicy::default();
        }
        Ok(definition)
    }

    /// Merge an imported YAML definition into `base`.
    ///
    /// Content and sequence items from `imported` are appended only when the
    /// `base` does not already contain an item with the same key
    /// (case-insensitive), matching `ErsatzTV`'s
    /// `SequentialPlayoutBuilder.Build` import logic.
    pub fn merge_import(base: &mut YamlPlayoutDefinition, imported: YamlPlayoutDefinition) {
        for content in imported.content {
            if base
                .content
                .iter()
                .all(|c| !c.key.eq_ignore_ascii_case(&content.key))
            {
                base.content.push(content);
            }
        }
        for sequence in imported.sequence {
            if base
                .sequence
                .iter()
                .all(|s| !s.key.eq_ignore_ascii_case(&sequence.key))
            {
                base.sequence.push(sequence);
            }
        }
    }

    fn definition_for_playout(playout: &Playout) -> Result<YamlPlayoutDefinition, SchedulingError> {
        let content = playout
            .schedule_file
            .as_ref()
            .ok_or(SchedulingError::MissingScheduleFile)?;
        // ErsatzRS divergence: `ScheduleFile` stores raw YAML content inline
        // instead of a filesystem path (the ErsatzTV convention).
        // Detection: Linux filesystem paths always start with `/` or `~/`;
        // YAML mapping documents (the only valid playout definition form)
        // always start with a key, `---`, or whitespace — never `/` or `~`.
        // This is robust even for single-line YAML like `{}`.
        if !content.starts_with('/') && !content.starts_with('~') {
            let mut definition = Self::definition_from_str(content)?;
            // Imports with absolute paths still resolve correctly; relative
            // imports are not meaningful without a file-system base so we pass
            // an empty base and let them fail gracefully as `ScheduleFileMissing`.
            Self::load_imports(&mut definition, "")?;
            return Ok(definition);
        }
        // Filesystem path (ErsatzTV legacy or explicit overrides).
        let yaml = fs::read_to_string(content)
            .map_err(|_| SchedulingError::ScheduleFileMissing(content.clone()))?;
        let mut definition = Self::definition_from_str(&yaml)?;
        Self::load_imports(&mut definition, content)?;
        Ok(definition)
    }

    fn load_imports(
        definition: &mut YamlPlayoutDefinition,
        schedule_file: &str,
    ) -> Result<(), SchedulingError> {
        let import_paths: Vec<String> = definition.import.clone();
        for import_path in import_paths {
            let resolved = if fs::metadata(&import_path).is_ok() {
                import_path.clone()
            } else {
                let parent = std::path::Path::new(schedule_file)
                    .parent()
                    .and_then(|p| p.to_str())
                    .unwrap_or("");
                format!("{parent}/{import_path}")
            };
            let yaml = fs::read_to_string(&resolved)
                .map_err(|_| SchedulingError::ScheduleFileMissing(resolved.clone()))?;
            let imported = Self::definition_from_str(&yaml)
                .map_err(|_| SchedulingError::InvalidYaml(format!("import {import_path}")))?;
            Self::merge_import(definition, imported);
        }
        Ok(())
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
        mut request: SchedulingRequest,
    ) -> Result<SchedulingResult, SchedulingError> {
        detect_cycle(&definition)?;
        flatten_sequences(&mut definition)?;
        request.reference_data.smart_shuffle_policy = definition.legacy_smart_shuffle.clone();
        for content in &definition.content {
            let policy = if content.smart_shuffle.is_disabled() {
                definition.legacy_smart_shuffle.clone()
            } else {
                content.smart_shuffle.clone()
            };
            request
                .reference_data
                .smart_shuffle_policies_by_unit
                .insert(content.key.clone(), policy);
        }

        let mut result = SchedulingResult {
            remove_before: Some(request.start),
            clear_items: matches!(
                request.mode,
                PlayoutBuildMode::Refresh | PlayoutBuildMode::Reset
            ),
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
        if request.mode == PlayoutBuildMode::Continue {
            if let Some(anchor) = &request.playout.anchor {
                context.restore(anchor)?;
            }
        }
        context.apply_history(&definition, &result.history_to_remove, request.mode)?;

        if request.mode == PlayoutBuildMode::Reset {
            for instruction in &definition.reset {
                dispatch_instruction(&mut context, instruction, request.mode, &definition)?;
            }
            apply_pending_shuffles(&mut context, &mut definition);
        }

        run_playout(&mut context, &mut definition, request.finish, request.mode)?;

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
        result
            .history_to_remove
            .extend(playlist_history::cleanup_with_playlist_snapshots(
                &request.reference_data.existing_history,
                request.start,
                request.reference_data.max_playout_offset,
            ));
        result.history_to_remove.sort_unstable();
        result.history_to_remove.dedup();
        Ok(result)
    }
}

fn run_playout(
    context: &mut YamlContext,
    definition: &mut YamlPlayoutDefinition,
    finish: DateTime<Utc>,
    mode: PlayoutBuildMode,
) -> Result<(), SchedulingError> {
    let mut repeat_guard = 0usize;
    while context.current_time < finish {
        let selected_schedule = select_schedule(&definition.schedules, context.current_time)
            .map(|schedule| schedule.name.clone());
        if selected_schedule != context.active_schedule {
            context.switch_to_schedule(selected_schedule);
            context.advance_guide_group();
            repeat_guard = 0;
        }
        let instructions = context.current_instructions(definition);
        if context.instruction_index >= instructions.len() {
            break;
        }
        let before_items = context.added_items.len();
        let instruction = instructions[context.instruction_index].clone();
        let changes_index = instruction.repeat.is_some_and(YamlRepeat::is_control);

        dispatch_instruction(context, &instruction, mode, definition)?;
        apply_pending_shuffles(context, definition);
        if context.added_items.len() > before_items {
            repeat_guard = 0;
        }
        if !changes_index {
            context.instruction_index += 1;
        } else if context.added_items.len() == before_items {
            repeat_guard += 1;
            if repeat_guard
                > context
                    .current_instructions(definition)
                    .len()
                    .saturating_add(1)
            {
                return Err(SchedulingError::InvalidYaml(
                    "YAML repeat loop did not add items".to_owned(),
                ));
            }
        }
    }
    Ok(())
}

fn apply_pending_shuffles(context: &mut YamlContext, definition: &mut YamlPlayoutDefinition) {
    type SequenceGroup = (Option<String>, Vec<(usize, YamlInstruction)>);
    for key in std::mem::take(&mut context.pending_shuffles) {
        let playout = context
            .active_schedule
            .as_ref()
            .and_then(|name| {
                definition
                    .schedules
                    .iter_mut()
                    .find(|item| item.name == *name)
            })
            .map_or(&mut definition.playout, |item| &mut item.playout);
        let mut groups: Vec<SequenceGroup> = Vec::new();
        for (index, instruction) in playout.iter().enumerate() {
            if instruction.sequence_key.as_deref() != Some(key.as_str()) {
                continue;
            }
            if let Some((_, members)) = groups
                .iter_mut()
                .find(|(guid, _)| guid == &instruction.sequence_guid)
            {
                members.push((index, instruction.clone()));
            } else {
                groups.push((
                    instruction.sequence_guid.clone(),
                    vec![(index, instruction.clone())],
                ));
            }
        }
        for (guid, mut members) in groups {
            let Some(first) = playout.iter().position(|item| {
                item.sequence_guid == guid && item.sequence_key.as_deref() == Some(key.as_str())
            }) else {
                continue;
            };
            let last_original_index = members.last().map_or(first, |(index, _)| *index);
            shuffle_with_seed(
                &mut members,
                super::next_playout_seed().cast_unsigned().into(),
            );
            if members.len() > 1 && members[0].0 == last_original_index {
                members.rotate_left(1);
            }
            let finish = first.saturating_add(members.len()).min(playout.len());
            playout.splice(first..finish, members.into_iter().map(|(_, item)| item));
        }
    }
}

fn yaml_content_smart_shuffle_presence(value: &serde_yaml::Value) -> Vec<bool> {
    let serde_yaml::Value::Mapping(root) = value else {
        return Vec::new();
    };
    let content_key = serde_yaml::Value::String("content".to_owned());
    let smart_shuffle_key = serde_yaml::Value::String("smart_shuffle".to_owned());
    root.get(&content_key)
        .and_then(serde_yaml::Value::as_sequence)
        .map(|content| {
            content
                .iter()
                .map(|item| {
                    item.as_mapping()
                        .is_some_and(|mapping| mapping.contains_key(&smart_shuffle_key))
                })
                .collect()
        })
        .unwrap_or_default()
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
    #[serde(
        default,
        alias = "ActiveSchedule",
        skip_serializing_if = "Option::is_none"
    )]
    active_schedule: Option<String>,
    #[serde(
        default,
        alias = "ScheduleIndices",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    schedule_indices: BTreeMap<String, usize>,
    #[serde(
        default,
        alias = "cooldown",
        skip_serializing_if = "super::SchedulingSmartShuffleState::is_empty"
    )]
    smart_shuffle: super::SchedulingSmartShuffleState,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    playlist_states: BTreeMap<String, super::classic::ClassicPlaylistCheckpoint>,
}

#[derive(Debug, Clone)]
struct YamlContext {
    playout: Playout,
    reference_data: SchedulingReferenceData,
    current_time: DateTime<Utc>,
    instruction_index: usize,
    active_schedule: Option<String>,
    list_states: BTreeMap<String, YamlListState>,
    visited_instructions: BTreeSet<usize>,
    guide_group: i32,
    guide_group_locked: bool,
    watermark_ids: BTreeSet<EntityId>,
    graphics_elements: BTreeMap<EntityId, Option<String>>,
    pre_roll_sequence: Option<String>,
    post_roll_sequence: Option<String>,
    mid_roll_sequence: Option<(String, String)>,
    filler_stack: Vec<FillerKind>,
    pending_shuffles: Vec<String>,
    added_items: Vec<ersatzrs_domain::PlayoutItem>,
    added_history: Vec<ersatzrs_domain::PlayoutHistory>,
    gaps: Vec<ersatzrs_domain::PlayoutGap>,
    enumerators: BTreeMap<String, MediaEnumerator>,
    playlist_enumerators: BTreeMap<String, PlaylistEnumerator>,
    restored_playlist_states: BTreeMap<String, super::classic::ClassicPlaylistCheckpoint>,
    playlist_history_positions: BTreeMap<String, (u64, usize)>,
    content_sources: BTreeMap<String, ResolvedYamlContent>,
    smart_shuffle_state: super::SchedulingSmartShuffleState,
}

#[derive(Debug, Clone)]
struct YamlListState {
    instruction_index: usize,
    visited_instructions: BTreeSet<usize>,
    watermark_ids: BTreeSet<EntityId>,
    graphics_elements: BTreeMap<EntityId, Option<String>>,
    pre_roll_sequence: Option<String>,
    post_roll_sequence: Option<String>,
    mid_roll_sequence: Option<(String, String)>,
    filler_stack: Vec<FillerKind>,
}

impl YamlListState {
    fn at_index(instruction_index: usize) -> Self {
        Self {
            instruction_index,
            visited_instructions: BTreeSet::new(),
            watermark_ids: BTreeSet::new(),
            graphics_elements: BTreeMap::new(),
            pre_roll_sequence: None,
            post_roll_sequence: None,
            mid_roll_sequence: None,
            filler_stack: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
struct ResolvedYamlContent {
    source: SchedulingContentSource,
    playback_order: PlaybackOrder,
    history_key: String,
}

impl YamlContext {
    fn new(
        playout: Playout,
        reference_data: SchedulingReferenceData,
        current_time: DateTime<Utc>,
        content_sources: BTreeMap<String, ResolvedYamlContent>,
    ) -> Self {
        Self {
            playout,
            reference_data,
            current_time,
            instruction_index: 0,
            active_schedule: None,
            list_states: BTreeMap::new(),
            visited_instructions: BTreeSet::new(),
            guide_group: 1,
            guide_group_locked: false,
            watermark_ids: BTreeSet::new(),
            graphics_elements: BTreeMap::new(),
            pre_roll_sequence: None,
            post_roll_sequence: None,
            mid_roll_sequence: None,
            filler_stack: Vec::new(),
            pending_shuffles: Vec::new(),
            added_items: Vec::new(),
            added_history: Vec::new(),
            gaps: Vec::new(),
            enumerators: BTreeMap::new(),
            playlist_enumerators: BTreeMap::new(),
            restored_playlist_states: BTreeMap::new(),
            playlist_history_positions: BTreeMap::new(),
            content_sources,
            smart_shuffle_state: super::SchedulingSmartShuffleState::default(),
        }
    }

    fn serialize(&self) -> Result<String, SchedulingError> {
        let mut schedule_indices = self
            .list_states
            .iter()
            .map(|(name, state)| (name.clone(), state.instruction_index))
            .collect::<BTreeMap<_, _>>();
        if self.active_schedule.is_some() || !schedule_indices.is_empty() {
            schedule_indices.insert(
                self.active_schedule.clone().unwrap_or_default(),
                self.instruction_index,
            );
        }
        let mut playlist_states = self.restored_playlist_states.clone();
        playlist_states.extend(
            self.playlist_enumerators.iter().map(|(key, playlist)| {
                (key.clone(), super::classic::playlist_checkpoint(playlist))
            }),
        );
        let state = SerializedYamlContext {
            instruction_index: self.instruction_index,
            guide_group: self.guide_group,
            guide_group_locked: self.guide_group_locked,
            watermark_ids: self.watermark_ids.iter().copied().collect(),
            pre_roll_sequence: self.pre_roll_sequence.clone(),
            active_schedule: self.active_schedule.clone(),
            schedule_indices,
            smart_shuffle: self.smart_shuffle_state.clone(),
            playlist_states,
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
                self.restored_playlist_states = state.playlist_states;
                self.active_schedule = state.active_schedule;
                self.list_states = state
                    .schedule_indices
                    .into_iter()
                    .map(|(name, index)| (name, YamlListState::at_index(index)))
                    .collect();
                self.smart_shuffle_state = state.smart_shuffle;
            }
        }
        Ok(())
    }

    fn current_instructions<'a>(
        &self,
        definition: &'a YamlPlayoutDefinition,
    ) -> &'a [YamlInstruction] {
        self.active_schedule
            .as_ref()
            .and_then(|name| definition.schedules.iter().find(|item| item.name == *name))
            .map_or(&definition.playout, |item| item.playout.as_slice())
    }

    fn switch_to_schedule(&mut self, schedule_name: Option<String>) {
        let current_key = self.active_schedule.clone().unwrap_or_default();
        self.list_states
            .insert(current_key, self.capture_list_state());
        let target_key = schedule_name.clone().unwrap_or_default();
        self.active_schedule = schedule_name;
        if let Some(state) = self.list_states.get(&target_key).cloned() {
            self.restore_list_state(state);
        } else {
            self.restore_list_state(YamlListState::at_index(0));
        }
    }

    fn capture_list_state(&self) -> YamlListState {
        YamlListState {
            instruction_index: self.instruction_index,
            visited_instructions: self.visited_instructions.clone(),
            watermark_ids: self.watermark_ids.clone(),
            graphics_elements: self.graphics_elements.clone(),
            pre_roll_sequence: self.pre_roll_sequence.clone(),
            post_roll_sequence: self.post_roll_sequence.clone(),
            mid_roll_sequence: self.mid_roll_sequence.clone(),
            filler_stack: self.filler_stack.clone(),
        }
    }

    fn restore_list_state(&mut self, state: YamlListState) {
        self.instruction_index = state.instruction_index;
        self.visited_instructions = state.visited_instructions;
        self.watermark_ids = state.watermark_ids;
        self.graphics_elements = state.graphics_elements;
        self.pre_roll_sequence = state.pre_roll_sequence;
        self.post_roll_sequence = state.post_roll_sequence;
        self.mid_roll_sequence = state.mid_roll_sequence;
        self.filler_stack = state.filler_stack;
    }

    fn enumerator(&mut self, content: &str) -> &mut MediaEnumerator {
        let resolved = self
            .content_sources
            .get(content)
            .cloned()
            .unwrap_or_else(|| ResolvedYamlContent {
                source: SchedulingContentSource::direct(content),
                playback_order: PlaybackOrder::None,
                history_key: history_key_for_content(content, PlaybackOrder::None),
            });
        self.enumerators
            .entry(content.to_owned())
            .or_insert_with(|| {
                lookup_media_source(&self.reference_data, content, &resolved.source)
                    .with_playback_order(resolved.playback_order)
            })
    }

    fn playlist_enumerator(&mut self, content: &str) -> Option<&mut PlaylistEnumerator> {
        if !self.playlist_enumerators.contains_key(content) {
            let items = self.playlist_items(content)?;

            // Preserve the cycle seed and canonical group order across builds.
            let should_shuffle = self.content_sources.get(content).is_some_and(|resolved| {
                matches!(
                    &resolved.source,
                    SchedulingContentSource::Marathon {
                        shuffle_groups: true,
                        ..
                    }
                )
            });
            let mut playlist = PlaylistEnumerator::new(items);
            if should_shuffle {
                playlist.shuffle_groups(self.playout.seed);
            }
            if let Some(state) = self.restored_playlist_states.get(content) {
                super::classic::restore_playlist_checkpoint(&mut playlist, state);
            }

            self.playlist_enumerators
                .insert(content.to_owned(), playlist);
        }
        self.playlist_enumerators.get_mut(content)
    }

    fn playlist_items(&self, content: &str) -> Option<Vec<SchedulingPlaylistItem>> {
        let resolved = self.content_sources.get(content)?;

        self.reference_data
            .playlist_items_by_key
            .get(content)
            .cloned()
            .or_else(|| {
                self.reference_data
                    .playlist_items_for_source(&resolved.source)
            })
    }

    fn history_key(&self, content: &str) -> String {
        self.content_sources.get(content).map_or_else(
            || history_key_for_content(content, PlaybackOrder::None),
            |resolved| resolved.history_key.clone(),
        )
    }

    fn apply_history(
        &mut self,
        definition: &YamlPlayoutDefinition,
        history_to_remove: &[EntityId],
        mode: PlayoutBuildMode,
    ) -> Result<(), SchedulingError> {
        let ignored = history_to_remove.iter().copied().collect::<BTreeSet<_>>();
        for content in &definition.content {
            let history_key = content.history_key();
            if self.playlist_items(&content.key).is_some() {
                if mode != PlayoutBuildMode::Reset
                    && !self.restored_playlist_states.contains_key(&content.key)
                {
                    self.restore_playlist_history(&content.key, &history_key, &ignored)?;
                }
                continue;
            }

            let Some(history) = self
                .reference_data
                .existing_history
                .iter()
                .filter(|history| !ignored.contains(&history.id))
                .filter(|history| history.key == history_key)
                .filter(|history| history.when < self.current_time)
                .max_by_key(|history| history.when)
                .cloned()
            else {
                continue;
            };
            self.enumerator(&content.key).apply_history(&history);
        }
        Ok(())
    }

    fn current_media_for_content(
        &mut self,
        content: &str,
    ) -> Option<(SchedulingMediaItem, CurrentHistoryContext)> {
        let policy = self
            .reference_data
            .smart_shuffle_policy_for_unit(content)
            .clone();
        let smart_shuffle_state = self.smart_shuffle_state.clone();
        let tags_by_media_id = self
            .reference_data
            .smart_shuffle_tags_for_unit(content)
            .clone();
        let scope_key = policy.scope_key(self.playout.id, None, Some(content));
        let scheduled_at = self.current_time;
        if let Some(playlist) = self.playlist_enumerator(content) {
            let before = playlist.current().cloned();
            playlist.select_with_smart_shuffle(
                &policy,
                &smart_shuffle_state,
                &scope_key,
                scheduled_at,
                &tags_by_media_id,
            );
            let selected = playlist
                .current()
                .cloned()
                .map(|media_item| (media_item, playlist.current_history_context()));
            // A Smart Shuffle swap cannot be reconstructed by ordinary advance.
            if selected.as_ref().map(|(item, _)| item) != before.as_ref() {
                self.playlist_history_positions.remove(content);
            }
            selected
        } else {
            let enumerator = self.enumerator(content);
            enumerator.select_with_smart_shuffle(
                &policy,
                &smart_shuffle_state,
                &scope_key,
                scheduled_at,
                &tags_by_media_id,
            );
            enumerator.current().cloned().map(|media_item| {
                (
                    media_item,
                    CurrentHistoryContext::Single {
                        index: enumerator.current_index(),
                    },
                )
            })
        }
    }

    fn advance_content(&mut self, content: &str) {
        if let Some(playlist) = self.playlist_enumerator(content) {
            playlist.advance();
        } else {
            self.enumerator(content).advance();
        }
    }

    fn content_len(&mut self, content: &str) -> usize {
        if let Some(playlist) = self.playlist_enumerator(content) {
            playlist.len()
        } else {
            self.enumerator(content).len()
        }
    }

    fn count_expression_len(&mut self, content: &str) -> usize {
        if let Some(playlist) = self.playlist_enumerator(content) {
            playlist.unique_media_len()
        } else {
            self.enumerator(content).len()
        }
    }

    fn skip_content(&mut self, content: &str, skip: usize) {
        if let Some(playlist) = self.playlist_enumerator(content) {
            for _ in 0..skip.min(playlist.len()) {
                playlist.advance();
            }
        } else {
            let enumerator = self.enumerator(content);
            for _ in 0..skip.min(enumerator.len()) {
                enumerator.advance();
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

fn content_sources(definition: &YamlPlayoutDefinition) -> BTreeMap<String, ResolvedYamlContent> {
    definition
        .content
        .iter()
        .map(|content| {
            (
                content.key.clone(),
                ResolvedYamlContent {
                    source: content.source(),
                    playback_order: content.playback_order(),
                    history_key: content.history_key(),
                },
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
