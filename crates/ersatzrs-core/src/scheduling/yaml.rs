#![allow(clippy::similar_names)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
use ersatzrs_domain::{EntityId, FillerKind, PlaybackOrder, Playout, PlayoutAnchor};
use serde::{Deserialize, Serialize};

use super::{
    cleanup_history, history, history_details_for_media_item, history_key_for_collection_key,
    history_key_for_content, lookup_media_source, parse_duration, parse_hhmm, playout_item,
    MediaEnumerator, PlayoutBuildMode, PlayoutSchedulingEngine, SchedulingContentGuid,
    SchedulingContentResolver, SchedulingContentSource, SchedulingError, SchedulingMediaItem,
    SchedulingPlaylistItem, SchedulingReferenceData, SchedulingRequest, SchedulingResult,
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
        let path = playout
            .schedule_file
            .as_ref()
            .ok_or(SchedulingError::MissingScheduleFile)?;
        let yaml = fs::read_to_string(path)
            .map_err(|_| SchedulingError::ScheduleFileMissing(path.clone()))?;
        let mut definition = Self::definition_from_str(&yaml)?;
        Self::load_imports(&mut definition, path)?;
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
    added_items: Vec<ersatzrs_domain::PlayoutItem>,
    added_history: Vec<ersatzrs_domain::PlayoutHistory>,
    gaps: Vec<ersatzrs_domain::PlayoutGap>,
    enumerators: BTreeMap<String, MediaEnumerator>,
    playlist_enumerators: BTreeMap<String, PlaylistEnumerator>,
    content_sources: BTreeMap<String, ResolvedYamlContent>,
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
            playlist_enumerators: BTreeMap::new(),
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
            let mut items = self.playlist_items(content)?;

            // Shuffle groups deterministically when requested (Marathon only).
            // Seed mirrors ErsatzTV: playout-seed + instruction-index + day-of-year.
            let should_shuffle = self.content_sources.get(content).is_some_and(|resolved| {
                matches!(
                    &resolved.source,
                    SchedulingContentSource::Marathon {
                        shuffle_groups: true,
                        ..
                    }
                )
            });
            if should_shuffle {
                let seed = self
                    .playout
                    .id
                    .cast_unsigned()
                    .wrapping_add(self.instruction_index as u64)
                    .wrapping_add(u64::from(self.current_time.ordinal()));
                shuffle_with_seed(&mut items, seed);
            }

            self.playlist_enumerators
                .insert(content.to_owned(), PlaylistEnumerator::new(items));
        }
        self.playlist_enumerators.get_mut(content)
    }

    fn playlist_items(&self, content: &str) -> Option<Vec<SchedulingPlaylistItem>> {
        let resolved = self.content_sources.get(content)?;

        self.reference_data
            .playlist_items_for_source(&resolved.source)
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
    ) {
        let ignored = history_to_remove.iter().copied().collect::<BTreeSet<_>>();
        for content in &definition.content {
            let history_key = content.history_key();
            if self.playlist_items(&content.key).is_some() {
                self.apply_playlist_history(&content.key, &history_key, &ignored);
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
    }

    fn apply_playlist_history(
        &mut self,
        content: &str,
        history_key: &str,
        ignored: &BTreeSet<EntityId>,
    ) {
        let Some(latest_when) = self
            .reference_data
            .existing_history
            .iter()
            .filter(|history| !ignored.contains(&history.id))
            .filter(|history| history.key == history_key)
            .filter(|history| history.when < self.current_time)
            .map(|history| history.when)
            .max()
        else {
            return;
        };
        let history = self
            .reference_data
            .existing_history
            .iter()
            .filter(|history| !ignored.contains(&history.id))
            .filter(|history| history.key == history_key)
            .filter(|history| history.when == latest_when)
            .cloned()
            .collect::<Vec<_>>();

        let Some(playlist) = self.playlist_enumerator(content) else {
            return;
        };
        playlist.apply_history(&history);
    }

    fn current_media_for_content(
        &mut self,
        content: &str,
    ) -> Option<(SchedulingMediaItem, CurrentHistoryContext)> {
        if let Some(playlist) = self.playlist_enumerator(content) {
            playlist
                .current()
                .cloned()
                .map(|media_item| (media_item, playlist.current_history_context()))
        } else {
            let enumerator = self.enumerator(content);
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

#[derive(Debug, Clone)]
enum CurrentHistoryContext {
    Single {
        index: usize,
    },
    Playlist {
        index: usize,
        child_key: String,
        child_index: usize,
        child_details: String,
    },
}

#[derive(Debug, Clone)]
struct PlaylistChildEnumerator {
    collection_key: super::SchedulingCollectionKey,
    play_all: bool,
    count: Option<usize>,
    items_taken_from_current: usize,
    enumerator: MediaEnumerator,
}

#[derive(Debug, Clone)]
struct PlaylistEnumerator {
    index: usize,
    child_index: usize,
    children: Vec<PlaylistChildEnumerator>,
}

impl PlaylistEnumerator {
    fn new(playlist_items: Vec<SchedulingPlaylistItem>) -> Self {
        let children = playlist_items
            .into_iter()
            .enumerate()
            .map(|(index, item)| PlaylistChildEnumerator {
                collection_key: item.collection_key,
                play_all: item.play_all,
                count: item.count,
                items_taken_from_current: 0,
                enumerator: MediaEnumerator::new(
                    format!("playlist-child-{index}"),
                    item.media_items,
                )
                .with_playback_order(item.playback_order),
            })
            .filter(|child| !child.enumerator.is_empty())
            .collect();
        Self {
            index: 0,
            child_index: 0,
            children,
        }
    }

    fn len(&self) -> usize {
        self.children
            .iter()
            .map(|child| {
                if child.play_all {
                    child.enumerator.len().max(1)
                } else {
                    child.count.unwrap_or(1)
                }
            })
            .sum()
    }

    fn current_child(&self) -> Option<&PlaylistChildEnumerator> {
        if self.children.is_empty() {
            None
        } else {
            self.children.get(self.child_index % self.children.len())
        }
    }

    fn current_child_mut(&mut self) -> Option<&mut PlaylistChildEnumerator> {
        if self.children.is_empty() {
            None
        } else {
            let index = self.child_index % self.children.len();
            self.children.get_mut(index)
        }
    }

    fn current(&self) -> Option<&SchedulingMediaItem> {
        self.current_child()
            .and_then(|child| child.enumerator.current())
    }

    fn current_history_context(&self) -> CurrentHistoryContext {
        let Some(child) = self.current_child() else {
            return CurrentHistoryContext::Single { index: self.index };
        };
        let child_details = child
            .enumerator
            .current()
            .map(history_details_for_media_item)
            .unwrap_or_default();
        CurrentHistoryContext::Playlist {
            index: self.index,
            child_key: history_key_for_collection_key(&child.collection_key),
            child_index: child.enumerator.current_index(),
            child_details,
        }
    }

    fn advance(&mut self) {
        let Some(child) = self.current_child_mut() else {
            return;
        };
        child.enumerator.advance();
        child.items_taken_from_current = child.items_taken_from_current.saturating_add(1);

        let should_switch = if child.play_all {
            child.enumerator.current_index() == 0
        } else if let Some(count) = child.count {
            child.items_taken_from_current >= count
        } else {
            true
        };

        if should_switch {
            self.child_index = (self.child_index + 1) % self.children.len();
            if let Some(next_child) = self.current_child_mut() {
                next_child.items_taken_from_current = 0;
            }
        }
        self.index = self.index.saturating_add(1);
        if self.child_index == 0
            && self
                .children
                .first()
                .is_some_and(|child| child.enumerator.current_index() == 0)
        {
            self.index = 0;
        }
    }

    fn apply_history(&mut self, history: &[ersatzrs_domain::PlayoutHistory]) {
        let Some(primary) = history.iter().find(|history| history.child_key.is_none()) else {
            return;
        };
        self.index = usize::try_from(primary.index).unwrap_or_default();
        let mut has_set_child = false;
        for child_history in history.iter().filter(|history| history.child_key.is_some()) {
            let Some(child_key) = child_history.child_key.as_deref() else {
                continue;
            };
            let Some((child_index, child)) =
                self.children.iter_mut().enumerate().find(|(_, child)| {
                    history_key_for_collection_key(&child.collection_key) == child_key
                })
            else {
                continue;
            };
            let next_index = if child_history.is_current_child {
                child_history.index.saturating_add(1)
            } else {
                child_history.index
            };
            child
                .enumerator
                .set_index(usize::try_from(next_index).unwrap_or_default());
            if child.enumerator.playback_order == PlaybackOrder::Chronological {
                if child_history.is_current_child {
                    child
                        .enumerator
                        .move_to_history_details(&child_history.details);
                } else {
                    child
                        .enumerator
                        .move_after_history_details(&child_history.details);
                }
            }
            if child_history.is_current_child {
                self.child_index = child_index;
                has_set_child = true;
            }
        }
        if !has_set_child && !self.children.is_empty() {
            self.child_index =
                usize::try_from(primary.index).unwrap_or_default() % self.children.len();
        }
        self.advance();
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
    context.skip_content(content, skip);
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
    let count = context.content_len(content);
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
    let enumerator_len = context.content_len(content);
    let seed = context
        .playout
        .id
        .cast_unsigned()
        .wrapping_add(context.instruction_index as u64)
        .wrapping_add(u64::from(context.current_time.ordinal()));
    let count = instruction
        .count
        .as_deref()
        .map(|expr| eval_count_expression(expr, enumerator_len, seed))
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
        let Some((media_item, history_context)) = context.current_media_for_content(content) else {
            if schedule_fallback_item(context, instruction, target) {
                continue;
            }
            break;
        };
        let remaining = target - context.current_time;
        if media_item.duration <= remaining || !instruction.stop_before_end {
            schedule_next_with_rolls(context, instruction, definition, content)?;
        } else if instruction.discard_attempts > 0 {
            context.advance_content(content);
        } else if instruction.trim {
            add_media_item(
                context,
                instruction,
                content,
                &media_item,
                remaining,
                history_context,
            );
            context.advance_content(content);
        } else if !schedule_fallback_item(context, instruction, target) {
            let gap = ersatzrs_domain::PlayoutGap {
                playout_id: context.playout.id,
                start: context.current_time,
                finish: target,
                ..ersatzrs_domain::PlayoutGap::default()
            };
            context.gaps.push(gap);
            context.current_time = target;
        }
    }
    Ok(())
}

fn schedule_fallback_item(
    context: &mut YamlContext,
    instruction: &YamlInstruction,
    target: DateTime<Utc>,
) -> bool {
    let Some(fallback) = instruction.fallback.as_deref() else {
        return false;
    };
    let remaining = target - context.current_time;
    let Some((fallback_item, fallback_history_context)) =
        context.current_media_for_content(fallback)
    else {
        return false;
    };

    let mut fallback_instruction = instruction.clone();
    fallback_instruction.filler_kind = Some("Fallback".to_owned());
    add_media_item(
        context,
        &fallback_instruction,
        fallback,
        &fallback_item,
        fallback_item.duration.min(remaining),
        fallback_history_context,
    );
    context.advance_content(fallback);
    context.current_time = target;
    true
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

    let media_item = context.current_media_for_content(content);
    if let Some((media_item, history_context)) = media_item {
        add_media_item(
            context,
            instruction,
            content,
            &media_item,
            media_item.duration,
            history_context,
        );
        context.advance_content(content);
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
    history_context: CurrentHistoryContext,
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
    let history_key = context.history_key(content);
    match history_context {
        CurrentHistoryContext::Single { index } => {
            context.added_history.push(history(
                context.playout.id,
                history_key,
                &item,
                None,
                index,
                history_details_for_media_item(media_item),
                false,
            ));
        }
        CurrentHistoryContext::Playlist {
            index,
            child_key,
            child_index,
            child_details,
        } => {
            context.added_history.push(history(
                context.playout.id,
                history_key.clone(),
                &item,
                None,
                index,
                history_details_for_media_item(media_item),
                false,
            ));
            context.added_history.push(history(
                context.playout.id,
                history_key,
                &item,
                Some(child_key),
                child_index,
                child_details,
                true,
            ));
        }
    }
    context.current_time = item.finish;
    context.added_items.push(item);
    context.advance_guide_group();
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

// ── count expression evaluator ────────────────────────────────────────────────
//
// Mirrors `ErsatzTV`'s `CountExpression.Evaluate` (NCalc-based).
// Supported: integer literals, `count` (enumerator length), `random`
// (deterministic value mod count), and `+`, `-`, `*`, `/`, `%` operators with
// parentheses. Returns `0` on any parse failure.

fn expr_skip_ws(chars: &[u8], pos: usize) -> usize {
    let mut p = pos;
    while p < chars.len() && chars[p] == b' ' {
        p += 1;
    }
    p
}

fn expr_parse_expr(chars: &[u8], pos: usize, count: i64, random: i64) -> (i64, usize) {
    let (mut left, mut pos) = expr_parse_term(chars, pos, count, random);
    loop {
        let p = expr_skip_ws(chars, pos);
        if p >= chars.len() {
            break;
        }
        match chars[p] {
            b'+' => {
                let (right, next) = expr_parse_term(chars, p + 1, count, random);
                left = left.saturating_add(right);
                pos = next;
            }
            b'-' => {
                let (right, next) = expr_parse_term(chars, p + 1, count, random);
                left = left.saturating_sub(right);
                pos = next;
            }
            _ => break,
        }
    }
    (left, pos)
}

fn expr_parse_term(chars: &[u8], pos: usize, count: i64, random: i64) -> (i64, usize) {
    let (mut left, mut pos) = expr_parse_factor(chars, pos, count, random);
    loop {
        let p = expr_skip_ws(chars, pos);
        if p >= chars.len() {
            break;
        }
        match chars[p] {
            b'*' => {
                let (right, next) = expr_parse_factor(chars, p + 1, count, random);
                left = left.saturating_mul(right);
                pos = next;
            }
            b'/' => {
                let (right, next) = expr_parse_factor(chars, p + 1, count, random);
                left = if right != 0 { left / right } else { 0 };
                pos = next;
            }
            b'%' => {
                let (right, next) = expr_parse_factor(chars, p + 1, count, random);
                left = if right != 0 { left % right } else { 0 };
                pos = next;
            }
            _ => break,
        }
    }
    (left, pos)
}

fn expr_parse_factor(chars: &[u8], pos: usize, count: i64, random: i64) -> (i64, usize) {
    let p = expr_skip_ws(chars, pos);
    if p >= chars.len() {
        return (0, p);
    }
    if chars[p] == b'(' {
        let (val, mut next) = expr_parse_expr(chars, p + 1, count, random);
        next = expr_skip_ws(chars, next);
        if next < chars.len() && chars[next] == b')' {
            next += 1;
        }
        return (val, next);
    }
    let rest = &chars[p..];
    let is_not_ident = |c: &u8| !c.is_ascii_alphanumeric() && *c != b'_';
    if rest.starts_with(b"count") && rest.get(5).is_none_or(is_not_ident) {
        return (count, p + 5);
    }
    if rest.starts_with(b"random") && rest.get(6).is_none_or(is_not_ident) {
        return (random, p + 6);
    }
    // Integer literal (optional leading minus handled by term subtraction)
    let start = p;
    let mut end = p;
    while end < chars.len() && chars[end].is_ascii_digit() {
        end += 1;
    }
    if end > start {
        let s = std::str::from_utf8(&chars[start..end]).unwrap_or("0");
        return (s.parse::<i64>().unwrap_or(0), end);
    }
    (0, p)
}

/// Evaluate a count expression (see module-level helpers above).
fn eval_count_expression(expr: &str, enumerator_len: usize, seed: u64) -> usize {
    let count = i64::try_from(enumerator_len).unwrap_or(i64::MAX);
    let random: i64 = if enumerator_len > 0 {
        let r = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        i64::try_from(r % u64::try_from(enumerator_len).unwrap_or(u64::MAX)).unwrap_or(0)
    } else {
        0
    };
    let chars = expr.as_bytes();
    let (result, _) = expr_parse_expr(chars, 0, count, random);
    usize::try_from(result.max(0)).unwrap_or(0)
}

/// Fisher-Yates shuffle with a simple LCG for deterministic ordering.
fn shuffle_with_seed<T>(items: &mut [T], seed: u64) {
    let len = items.len();
    if len <= 1 {
        return;
    }
    let mut state = seed;
    for i in (1..len).rev() {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let j = (state >> 33) as usize % (i + 1);
        items.swap(i, j);
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
    use crate::scheduling::{
        SchedulingCollectionKey, SchedulingMediaItem, SchedulingPlaylistItem,
        SchedulingReferenceData,
    };
    use ersatzrs_domain::{PlaybackOrder, Playout};

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

    fn episode_item(
        id: EntityId,
        show_id: EntityId,
        season_id: EntityId,
        season: i32,
        episode: i32,
    ) -> SchedulingMediaItem {
        SchedulingMediaItem {
            show_id: Some(show_id),
            season_id: Some(season_id),
            season: Some(season),
            episode: Some(episode),
            ..SchedulingMediaItem::new(id, Duration::minutes(10))
        }
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
    fn yaml_show_and_guid_content_source_resolution_snapshot() {
        let mut reference_data = SchedulingReferenceData::default();
        reference_data.shows_by_name.insert(
            "Top Gear".to_owned(),
            vec![
                episode_item(101, 10, 1001, 1, 1),
                episode_item(102, 10, 1001, 1, 2),
            ],
        );
        reference_data.media_by_show_guid.insert(
            "tmdb://200".to_owned(),
            vec![
                episode_item(201, 20, 2001, 1, 1),
                episode_item(202, 20, 2001, 1, 2),
            ],
        );

        let definition = YamlPlayoutEngine::definition_from_str(
            r#"
content:
  - key: named_show
    show: Top Gear
    order: season_episode
  - key: guided_show
    guids:
      - source: tmdb
        value: 200
    order: season_episode
playout:
  - all: named_show
  - all: guided_show
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
            "history": result.added_history.iter().map(|history| serde_json::json!({
                "key": history.key,
                "index": history.index,
                "details": history.details,
            })).collect::<Vec<_>>(),
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
            ersatzrs_domain::PlayoutHistory {
                id: 10,
                key: history_key.clone(),
                when: start - Duration::hours(3),
                finish: Some(start - Duration::hours(3) + Duration::minutes(10)),
                ..ersatzrs_domain::PlayoutHistory::default()
            },
            ersatzrs_domain::PlayoutHistory {
                id: 11,
                key: history_key.clone(),
                when: start - Duration::hours(2),
                finish: Some(start - Duration::hours(2) + Duration::minutes(10)),
                ..ersatzrs_domain::PlayoutHistory::default()
            },
            ersatzrs_domain::PlayoutHistory {
                id: 12,
                key: history_key,
                when: start,
                finish: Some(start + Duration::minutes(5)),
                ..ersatzrs_domain::PlayoutHistory::default()
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
    fn yaml_chronological_history_resume_by_media_item_details_snapshot() {
        let start = DateTime::parse_from_rfc3339("2026-04-15T10:00:00Z")
            .expect("valid time")
            .with_timezone(&Utc);
        let history_key = history_key_for_content("collection", PlaybackOrder::Chronological);
        let mut reference_data = SchedulingReferenceData::default();
        let item_1 = SchedulingMediaItem::new(1, Duration::minutes(10));
        let item_2 = SchedulingMediaItem::new(2, Duration::minutes(10));
        let item_3 = SchedulingMediaItem::new(3, Duration::minutes(10));
        reference_data
            .collections_by_name
            .insert("Movies".to_owned(), vec![item_1, item_2.clone(), item_3]);
        reference_data.existing_history = vec![ersatzrs_domain::PlayoutHistory {
            id: 20,
            key: history_key,
            when: start - Duration::hours(1),
            finish: Some(start - Duration::hours(1) + Duration::minutes(10)),
            index: 0,
            details: history_details_for_media_item(&item_2),
            ..ersatzrs_domain::PlayoutHistory::default()
        }];

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
                    mode: PlayoutBuildMode::Continue,
                    reference_data,
                },
            )
            .expect("schedule builds");

        insta::assert_json_snapshot!(serde_json::json!({
            "media": result.added_items.iter().map(|item| item.media_item_id).collect::<Vec<_>>(),
            "history": result.added_history.iter().map(|history| serde_json::json!({
                "key": history.key,
                "index": history.index,
                "details": history.details,
            })).collect::<Vec<_>>(),
        }));
    }

    #[test]
    fn yaml_playlist_history_keys_and_current_child_snapshot() {
        let start = DateTime::parse_from_rfc3339("2026-04-15T10:00:00Z")
            .expect("valid time")
            .with_timezone(&Utc);
        let playlist_key = history_key_for_content("playlist", PlaybackOrder::None);
        let child_0_key = SchedulingCollectionKey::fake_playlist_item(0);
        let child_1_key = SchedulingCollectionKey::fake_playlist_item(1);
        let child_0_history_key = history_key_for_collection_key(&child_0_key);
        let mut reference_data = SchedulingReferenceData::default();
        let item_1 = SchedulingMediaItem {
            season: Some(1),
            episode: Some(1),
            ..SchedulingMediaItem::new(1, Duration::minutes(10))
        };
        reference_data.playlist_items_by_group_and_name.insert(
            (Some("Prime".to_owned()), "Block A".to_owned()),
            vec![
                SchedulingPlaylistItem {
                    collection_key: child_0_key,
                    playback_order: PlaybackOrder::Chronological,
                    play_all: false,
                    count: None,
                    media_items: vec![
                        item_1.clone(),
                        SchedulingMediaItem {
                            season: Some(1),
                            episode: Some(2),
                            ..SchedulingMediaItem::new(2, Duration::minutes(10))
                        },
                    ],
                },
                SchedulingPlaylistItem {
                    collection_key: child_1_key,
                    playback_order: PlaybackOrder::Chronological,
                    play_all: false,
                    count: None,
                    media_items: vec![SchedulingMediaItem::new(10, Duration::minutes(10))],
                },
            ],
        );
        reference_data.existing_history = vec![
            ersatzrs_domain::PlayoutHistory {
                id: 30,
                key: playlist_key.clone(),
                when: start - Duration::hours(1),
                finish: Some(start - Duration::hours(1) + Duration::minutes(10)),
                index: 0,
                details: history_details_for_media_item(&item_1),
                ..ersatzrs_domain::PlayoutHistory::default()
            },
            ersatzrs_domain::PlayoutHistory {
                id: 31,
                key: playlist_key,
                child_key: Some(child_0_history_key),
                when: start - Duration::hours(1),
                finish: Some(start - Duration::hours(1) + Duration::minutes(10)),
                index: 0,
                details: history_details_for_media_item(&item_1),
                is_current_child: true,
                ..ersatzrs_domain::PlayoutHistory::default()
            },
        ];

        let definition = YamlPlayoutEngine::definition_from_str(
            r#"
content:
  - key: playlist
    playlist_group: Prime
    playlist: Block A
playout:
  - count: "1"
    content: playlist
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
                    mode: PlayoutBuildMode::Continue,
                    reference_data,
                },
            )
            .expect("schedule builds");

        insta::assert_json_snapshot!(serde_json::json!({
            "media": result.added_items.iter().map(|item| item.media_item_id).collect::<Vec<_>>(),
            "history": result.added_history.iter().map(|history| serde_json::json!({
                "key": history.key,
                "child_key": history.child_key,
                "index": history.index,
                "details": history.details,
                "is_current_child": history.is_current_child,
            })).collect::<Vec<_>>(),
        }));
    }

    #[test]
    fn yaml_marathon_grouping_by_show_and_season_snapshot() {
        let mut reference_data = SchedulingReferenceData::default();
        reference_data.searches_by_query.insert(
            "sitcom".to_owned(),
            vec![
                episode_item(2, 10, 1001, 1, 2),
                episode_item(1, 10, 1001, 1, 1),
                episode_item(3, 20, 2001, 1, 1),
                episode_item(4, 10, 1002, 2, 1),
            ],
        );

        let definition = YamlPlayoutEngine::definition_from_str(
            r#"
content:
  - key: by_show
    marathon: Sitcom Marathon
    searches:
      - sitcom
    group_by: show
    item_order: season_episode
  - key: by_season
    marathon: Sitcom Marathon
    searches:
      - sitcom
    group_by: season
    item_order: season_episode
playout:
  - all: by_show
  - all: by_season
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
            "history_keys": result.added_history.iter().map(|history| history.key.clone()).collect::<Vec<_>>(),
        }));
    }

    #[test]
    fn yaml_marathon_child_collection_keys_and_history_details_snapshot() {
        let mut reference_data = SchedulingReferenceData::default();
        reference_data.searches_by_query.insert(
            "sitcom".to_owned(),
            vec![
                episode_item(2, 10, 1001, 1, 2),
                episode_item(1, 10, 1001, 1, 1),
                episode_item(3, 20, 2001, 1, 1),
            ],
        );

        let definition = YamlPlayoutEngine::definition_from_str(
            r#"
content:
  - key: by_show
    marathon: Sitcom Marathon
    searches:
      - sitcom
    group_by: show
    item_order: season_episode
playout:
  - count: "1"
    content: by_show
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
            "history": result.added_history.iter().map(|history| serde_json::json!({
                "key": history.key,
                "child_key": history.child_key,
                "index": history.index,
                "details": history.details,
                "is_current_child": history.is_current_child,
            })).collect::<Vec<_>>(),
        }));
    }

    #[test]
    fn yaml_marathon_play_all_items_and_item_order_snapshot() {
        let mut reference_data = SchedulingReferenceData::default();
        reference_data.searches_by_query.insert(
            "sitcom".to_owned(),
            vec![
                episode_item(2, 10, 1001, 1, 2),
                episode_item(1, 10, 1001, 1, 1),
                episode_item(3, 20, 2001, 1, 1),
                episode_item(4, 10, 1002, 2, 1),
            ],
        );

        let definition = YamlPlayoutEngine::definition_from_str(
            r#"
content:
  - key: by_show
    marathon: Sitcom Marathon
    searches:
      - sitcom
    group_by: show
    item_order: season_episode
  - key: by_show_all
    marathon: Sitcom Marathon
    searches:
      - sitcom
    group_by: show
    item_order: season_episode
    play_all_items: true
playout:
  - all: by_show
  - all: by_show_all
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
        }));
    }

    #[test]
    fn yaml_marathon_empty_source_uses_fallback_snapshot() {
        let mut reference_data = SchedulingReferenceData::default();
        reference_data.collections_by_name.insert(
            "Fallback".to_owned(),
            vec![SchedulingMediaItem::new(900, Duration::minutes(10))],
        );

        let definition = YamlPlayoutEngine::definition_from_str(
            r#"
content:
  - key: empty_marathon
    marathon: Missing Marathon
    searches:
      - missing
    group_by: show
    item_order: season_episode
  - key: fallback
    collection: Fallback
    order: chronological
playout:
  - duration: "00:05:00"
    content: empty_marathon
    fallback: fallback
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
            "items": result.added_items.iter().map(|item| serde_json::json!({
                "media": item.media_item_id,
                "start": item.start.to_rfc3339(),
                "finish": item.finish.to_rfc3339(),
                "filler": format!("{:?}", item.filler_kind),
            })).collect::<Vec<_>>(),
            "history": result.added_history.iter().map(|history| serde_json::json!({
                "key": history.key,
                "index": history.index,
                "details": history.details,
            })).collect::<Vec<_>>(),
        }));
    }

    #[test]
    fn yaml_reset_removes_future_and_current_history_snapshot() {
        let start = DateTime::parse_from_rfc3339("2026-04-15T10:00:00Z")
            .expect("valid time")
            .with_timezone(&Utc);
        let history_key = history_key_for_content("main", PlaybackOrder::None);
        let mut reference_data = SchedulingReferenceData::default();
        reference_data.media_by_key.insert(
            "main".to_owned(),
            vec![SchedulingMediaItem::new(1, Duration::minutes(10))],
        );
        reference_data.existing_history = vec![
            ersatzrs_domain::PlayoutHistory {
                id: 40,
                key: history_key.clone(),
                when: start - Duration::hours(2),
                finish: Some(start - Duration::hours(2) + Duration::minutes(10)),
                ..ersatzrs_domain::PlayoutHistory::default()
            },
            ersatzrs_domain::PlayoutHistory {
                id: 41,
                key: history_key.clone(),
                when: start - Duration::minutes(5),
                finish: Some(start + Duration::minutes(5)),
                ..ersatzrs_domain::PlayoutHistory::default()
            },
            ersatzrs_domain::PlayoutHistory {
                id: 42,
                key: history_key,
                when: start + Duration::minutes(5),
                finish: Some(start + Duration::minutes(15)),
                ..ersatzrs_domain::PlayoutHistory::default()
            },
        ];
        let definition = YamlPlayoutEngine::definition_from_str(
            r#"
playout:
  - count: "1"
    content: main
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
            "history_to_remove": result.history_to_remove,
        }));
    }

    #[test]
    fn yaml_cleanup_keeps_latest_completed_history_per_key_snapshot() {
        let start = DateTime::parse_from_rfc3339("2026-04-15T10:00:00Z")
            .expect("valid time")
            .with_timezone(&Utc);
        let key_a = history_key_for_content("a", PlaybackOrder::None);
        let key_b = history_key_for_content("b", PlaybackOrder::None);
        let mut reference_data = SchedulingReferenceData {
            max_playout_offset: Duration::zero(),
            ..SchedulingReferenceData::default()
        };
        reference_data.media_by_key.insert(
            "a".to_owned(),
            vec![SchedulingMediaItem::new(1, Duration::minutes(10))],
        );
        reference_data.existing_history = vec![
            ersatzrs_domain::PlayoutHistory {
                id: 50,
                key: key_a.clone(),
                when: start - Duration::hours(3),
                finish: Some(start - Duration::hours(3) + Duration::minutes(10)),
                ..ersatzrs_domain::PlayoutHistory::default()
            },
            ersatzrs_domain::PlayoutHistory {
                id: 51,
                key: key_a,
                when: start - Duration::hours(2),
                finish: Some(start - Duration::hours(2) + Duration::minutes(10)),
                ..ersatzrs_domain::PlayoutHistory::default()
            },
            ersatzrs_domain::PlayoutHistory {
                id: 52,
                key: key_b.clone(),
                when: start - Duration::hours(4),
                finish: Some(start - Duration::hours(4) + Duration::minutes(10)),
                ..ersatzrs_domain::PlayoutHistory::default()
            },
            ersatzrs_domain::PlayoutHistory {
                id: 53,
                key: key_b,
                when: start - Duration::hours(1),
                finish: Some(start - Duration::hours(1) + Duration::minutes(10)),
                ..ersatzrs_domain::PlayoutHistory::default()
            },
        ];
        let definition = YamlPlayoutEngine::definition_from_str(
            r#"
playout:
  - count: "1"
    content: a
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
                    mode: PlayoutBuildMode::Continue,
                    reference_data,
                },
            )
            .expect("schedule builds");

        insta::assert_json_snapshot!(serde_json::json!({
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

    // ── count expression tests ────────────────────────────────────────────────

    #[test]
    fn count_expression_literal() {
        // Plain integer literals still work (must not break existing snapshots).
        assert_eq!(eval_count_expression("3", 10, 0), 3);
        assert_eq!(eval_count_expression("0", 10, 0), 0);
    }

    #[test]
    fn count_expression_count_variable() {
        // "count" expands to the enumerator length.
        assert_eq!(eval_count_expression("count", 4, 0), 4);
        assert_eq!(eval_count_expression("count", 0, 0), 0);
    }

    #[test]
    fn count_expression_arithmetic() {
        assert_eq!(eval_count_expression("count / 2", 8, 0), 4);
        assert_eq!(eval_count_expression("count + 1", 3, 0), 4);
        assert_eq!(eval_count_expression("count * 2", 3, 0), 6);
        assert_eq!(eval_count_expression("2 + 3", 0, 0), 5);
    }

    #[test]
    fn count_expression_random_variable() {
        // "random" must be in [0, count). Same seed + same enumerator length ⇒
        // same result (determinism).
        let v1 = eval_count_expression("random", 10, 42);
        let v2 = eval_count_expression("random", 10, 42);
        assert_eq!(v1, v2);
        assert!(v1 < 10);
    }

    #[test]
    fn count_expression_zero_for_invalid() {
        // Non-parseable input falls back to 0.
        assert_eq!(eval_count_expression("bogus_var", 5, 0), 0);
        assert_eq!(eval_count_expression("", 5, 0), 0);
    }

    #[test]
    fn count_expression_in_schedule_uses_enumerator_length() {
        // count: "count" schedules all items in the enumerator.
        insta::assert_json_snapshot!(snapshot(
            r#"
playout:
  - count: "count"
    content: main
"#
        ));
    }

    #[test]
    fn count_expression_arithmetic_in_schedule_snapshot() {
        // count: "count / 2" with 2 items → schedules 1 item.
        insta::assert_json_snapshot!(snapshot(
            r#"
playout:
  - count: "count / 2"
    content: main
"#
        ));
    }

    // ── import merge tests ────────────────────────────────────────────────────

    #[test]
    fn yaml_import_merge_adds_content_from_imported_definition() {
        let mut base = YamlPlayoutEngine::definition_from_str(
            r#"
content:
  - key: main
    collection: Movies
"#,
        )
        .unwrap();
        let imported = YamlPlayoutEngine::definition_from_str(
            r#"
content:
  - key: bumpers
    collection: Bumpers
  - key: main
    collection: Ignored
"#,
        )
        .unwrap();

        YamlPlayoutEngine::merge_import(&mut base, imported);

        // "bumpers" must be added; "main" must NOT be overridden.
        assert_eq!(base.content.len(), 2);
        let main = base.content.iter().find(|c| c.key == "main").unwrap();
        assert_eq!(main.collection.as_deref(), Some("Movies"));
        let bumpers = base.content.iter().find(|c| c.key == "bumpers").unwrap();
        assert_eq!(bumpers.collection.as_deref(), Some("Bumpers"));
    }

    #[test]
    fn yaml_import_merge_adds_sequences_from_imported_definition() {
        let mut base = YamlPlayoutEngine::definition_from_str(
            r#"
sequence:
  - key: intro
    items:
      - count: "1"
        content: main
"#,
        )
        .unwrap();
        let imported = YamlPlayoutEngine::definition_from_str(
            r#"
sequence:
  - key: outro
    items:
      - count: "1"
        content: bumpers
  - key: intro
    items:
      - count: "99"
        content: ignored
"#,
        )
        .unwrap();

        YamlPlayoutEngine::merge_import(&mut base, imported);

        // "outro" is added; "intro" is kept from base.
        assert_eq!(base.sequence.len(), 2);
        let intro = base.sequence.iter().find(|s| s.key == "intro").unwrap();
        assert_eq!(
            intro.items[0].count.as_deref(),
            Some("1"),
            "base intro must not be overridden"
        );
    }

    #[test]
    fn yaml_import_merge_case_insensitive_key_comparison() {
        let mut base = YamlPlayoutEngine::definition_from_str(
            r#"
content:
  - key: Main
    collection: Movies
"#,
        )
        .unwrap();
        let imported = YamlPlayoutEngine::definition_from_str(
            r#"
content:
  - key: main
    collection: ShouldBeIgnored
"#,
        )
        .unwrap();

        YamlPlayoutEngine::merge_import(&mut base, imported);

        assert_eq!(
            base.content.len(),
            1,
            "duplicate key (different case) must not be added"
        );
    }

    #[test]
    fn yaml_import_merged_content_used_in_schedule_snapshot() {
        // Build a base definition manually and merge imported content into it,
        // then schedule to verify the merged content is used.
        let mut definition = YamlPlayoutEngine::definition_from_str(
            r#"
playout:
  - count: "2"
    content: imported
"#,
        )
        .unwrap();
        let imported_content = YamlPlayoutEngine::definition_from_str(
            r#"
content:
  - key: imported
    collection: Movies
"#,
        )
        .unwrap();
        YamlPlayoutEngine::merge_import(&mut definition, imported_content);

        let mut reference_data = SchedulingReferenceData::default();
        reference_data.collections_by_name.insert(
            "Movies".to_owned(),
            vec![
                SchedulingMediaItem::new(11, Duration::minutes(10)),
                SchedulingMediaItem::new(12, Duration::minutes(10)),
            ],
        );
        let start = DateTime::parse_from_rfc3339("2026-04-15T10:00:00Z")
            .unwrap()
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
            "media": result.added_items.iter().map(|item| item.media_item_id).collect::<Vec<_>>(),
        }));
    }

    // ── shuffle_groups tests ──────────────────────────────────────────────────

    #[test]
    fn yaml_marathon_shuffle_groups_produces_deterministic_order_snapshot() {
        // With shuffle_groups: true the children (groups) are reordered by seed.
        // The test verifies the ordering is stable across runs and distinct
        // from the insertion order of the groups when seed is non-trivial.
        let mut reference_data = SchedulingReferenceData::default();
        reference_data.searches_by_query.insert(
            "sitcom".to_owned(),
            vec![
                episode_item(1, 10, 1001, 1, 1),
                episode_item(2, 20, 2001, 1, 1),
                episode_item(3, 30, 3001, 1, 1),
            ],
        );

        let yaml = r#"
content:
  - key: by_show_shuffled
    marathon: Sitcom Marathon
    searches:
      - sitcom
    group_by: show
    item_order: season_episode
    shuffle_groups: true
playout:
  - all: by_show_shuffled
"#;
        let definition = YamlPlayoutEngine::definition_from_str(yaml).expect("valid yaml");
        let start = DateTime::parse_from_rfc3339("2026-04-15T10:00:00Z")
            .unwrap()
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
            "media": result.added_items.iter().map(|item| item.media_item_id).collect::<Vec<_>>(),
        }));
    }

    // ── playlist batch/count child semantics tests ────────────────────────────

    #[test]
    fn yaml_playlist_count_child_takes_n_items_before_switching_snapshot() {
        // A playlist child with count=2 takes 2 items before moving to the
        // next child, matching ErsatzTV's PlaylistEnumerator batch semantics.
        let child_0_key = SchedulingCollectionKey::fake_playlist_item(0);
        let child_1_key = SchedulingCollectionKey::fake_playlist_item(1);
        let mut reference_data = SchedulingReferenceData::default();
        reference_data.playlist_items_by_group_and_name.insert(
            (None, "MyPlaylist".to_owned()),
            vec![
                SchedulingPlaylistItem {
                    collection_key: child_0_key,
                    playback_order: PlaybackOrder::Chronological,
                    play_all: false,
                    count: Some(2),
                    media_items: vec![
                        SchedulingMediaItem::new(1, Duration::minutes(5)),
                        SchedulingMediaItem::new(2, Duration::minutes(5)),
                        SchedulingMediaItem::new(3, Duration::minutes(5)),
                    ],
                },
                SchedulingPlaylistItem {
                    collection_key: child_1_key,
                    playback_order: PlaybackOrder::Chronological,
                    play_all: false,
                    count: Some(1),
                    media_items: vec![SchedulingMediaItem::new(10, Duration::minutes(5))],
                },
            ],
        );

        let definition = YamlPlayoutEngine::definition_from_str(
            r#"
content:
  - key: pl
    playlist: MyPlaylist
playout:
  - count: "6"
    content: pl
"#,
        )
        .expect("valid yaml");
        let start = DateTime::parse_from_rfc3339("2026-04-15T10:00:00Z")
            .unwrap()
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

        // Expected interleaving: child0(1), child0(2), child1(10), child0(3),
        // child0(1), child1(10) — child0 wraps, count=2 each time.
        insta::assert_json_snapshot!(serde_json::json!({
            "media": result.added_items.iter().map(|item| item.media_item_id).collect::<Vec<_>>(),
        }));
    }

    #[test]
    fn yaml_playlist_play_all_child_exhausts_items_snapshot() {
        // A child with play_all=true exhausts its full enumerator before moving
        // to the next child (mirrors ErsatzTV play_all_items behavior).
        let child_0_key = SchedulingCollectionKey::fake_playlist_item(0);
        let child_1_key = SchedulingCollectionKey::fake_playlist_item(1);
        let mut reference_data = SchedulingReferenceData::default();
        reference_data.playlist_items_by_group_and_name.insert(
            (None, "AllPlaylist".to_owned()),
            vec![
                SchedulingPlaylistItem {
                    collection_key: child_0_key,
                    playback_order: PlaybackOrder::Chronological,
                    play_all: true,
                    count: None,
                    media_items: vec![
                        SchedulingMediaItem::new(1, Duration::minutes(5)),
                        SchedulingMediaItem::new(2, Duration::minutes(5)),
                    ],
                },
                SchedulingPlaylistItem {
                    collection_key: child_1_key,
                    playback_order: PlaybackOrder::Chronological,
                    play_all: false,
                    count: Some(1),
                    media_items: vec![SchedulingMediaItem::new(10, Duration::minutes(5))],
                },
            ],
        );

        let definition = YamlPlayoutEngine::definition_from_str(
            r#"
content:
  - key: pl
    playlist: AllPlaylist
playout:
  - count: "4"
    content: pl
"#,
        )
        .expect("valid yaml");
        let start = DateTime::parse_from_rfc3339("2026-04-15T10:00:00Z")
            .unwrap()
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

        // Expected: child0(1), child0(2) [play_all exhausts 2], child1(10), child0(1)
        insta::assert_json_snapshot!(serde_json::json!({
            "media": result.added_items.iter().map(|item| item.media_item_id).collect::<Vec<_>>(),
        }));
    }
}
