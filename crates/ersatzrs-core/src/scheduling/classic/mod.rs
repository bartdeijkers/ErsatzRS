use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, BinaryHeap, VecDeque},
};

use chrono::{DateTime, Duration, Timelike, Utc};
use ersatzrs_domain::{
    ChannelSubtitleMode, CollectionEnumeratorState, EntityId, FillWithGroupMode, FillerKind,
    FillerMode, FixedStartTimeBehavior, GuideMode, MultipleMode, PlaybackOrder, PlayoutAnchor,
    PlayoutItem, PlayoutProgramScheduleAnchor, PlayoutScheduleItemFillGroupIndex,
    ProgramScheduleItem, StartType, TailMode,
};
use serde::{Deserialize, Serialize};

use super::{
    cleanup_history, history, history_details_for_media_item, history_key_for_collection_key,
    playout_item, shuffle_with_seed, yaml, MediaEnumerator, PlaylistEnumerator, PlayoutBuildMode,
    PlayoutSchedulingEngine, SchedulingCollectionKey, SchedulingError, SchedulingMediaItem,
    SchedulingReferenceData, SchedulingRequest, SchedulingResult,
    SchedulingSmartShuffleRecordContext, SchedulingSmartShuffleState, SchedulingSmartShuffleUnit,
};

/// ErsatzTV-compatible scheduler for Classic program schedules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassicSchedulingEngine;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ClassicSchedulingContext {
    pub scheduler: &'static str,
    pub schedule_id: EntityId,
    pub item_id: EntityId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filler_preset_id: Option<EntityId>,
    pub enumerator: &'static str,
    pub seed: i32,
    pub index: i32,
}

#[must_use]
pub fn classic_schedule_item_key(item: &ProgramScheduleItem) -> String {
    item_key(item)
}

/// Summary of persisted Classic enumerator state compared with the active schedule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ClassicRuntimeStateAudit {
    /// Number of unique content enumerators required by the active schedule.
    pub expected_anchor_count: usize,
    /// Number of undated content enumerators currently persisted.
    pub actual_anchor_count: usize,
    /// Number of fill-group enumerators required by the active schedule.
    pub expected_fill_group_count: usize,
    /// Number of fill-group enumerators currently persisted.
    pub actual_fill_group_count: usize,
    /// Whether persisted content enumerators exactly match the active schedule.
    pub anchors_match: bool,
    /// Whether persisted fill-group enumerators exactly match the active schedule.
    pub fill_groups_match: bool,
}

impl ClassicRuntimeStateAudit {
    /// Return whether every active Classic content key and fill group has exactly one state row.
    #[must_use]
    pub const fn is_complete(self) -> bool {
        self.anchors_match && self.fill_groups_match
    }
}

/// Compare persisted Classic runtime-state identities with the active schedule definition.
#[must_use]
pub fn audit_classic_runtime_state(
    reference_data: &SchedulingReferenceData,
) -> ClassicRuntimeStateAudit {
    let expected_anchor_keys = reference_data
        .classic_schedule_items
        .iter()
        .map(classic_schedule_item_key)
        .collect::<BTreeSet<_>>();
    let actual_anchor_keys = reference_data
        .existing_program_schedule_anchors
        .iter()
        .filter(|anchor| anchor.anchor_date.is_none())
        .map(|anchor| history_key_for_collection_key(&collection_key_for_anchor(anchor)))
        .collect::<Vec<_>>();
    let actual_anchor_key_set = actual_anchor_keys.iter().cloned().collect::<BTreeSet<_>>();

    let expected_fill_group_ids = reference_data
        .classic_schedule_items
        .iter()
        .filter(|item| item.fill_with_group_mode != FillWithGroupMode::None)
        .map(|item| item.id)
        .collect::<BTreeSet<_>>();
    let actual_fill_group_ids = reference_data
        .existing_fill_group_indices
        .iter()
        .map(|index| index.program_schedule_item_id)
        .collect::<Vec<_>>();
    let actual_fill_group_id_set = actual_fill_group_ids
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();

    ClassicRuntimeStateAudit {
        expected_anchor_count: expected_anchor_keys.len(),
        actual_anchor_count: actual_anchor_keys.len(),
        expected_fill_group_count: expected_fill_group_ids.len(),
        actual_fill_group_count: actual_fill_group_ids.len(),
        anchors_match: actual_anchor_keys.len() == actual_anchor_key_set.len()
            && actual_anchor_key_set == expected_anchor_keys,
        fill_groups_match: actual_fill_group_ids.len() == actual_fill_group_id_set.len()
            && actual_fill_group_id_set == expected_fill_group_ids,
    }
}

/// Return the stable identity key used for an item's Smart Shuffle attachment.
#[must_use]
pub fn classic_schedule_item_policy_key(item: &ProgramScheduleItem) -> String {
    item.id.to_string()
}

#[derive(Debug, Clone)]
struct ClassicState {
    current_time: DateTime<Utc>,
    schedule_state: CollectionEnumeratorState,
    multiple_remaining: Option<i32>,
    duration_finish: Option<DateTime<Utc>>,
    in_flood: bool,
    in_duration_filler: bool,
    next_guide_group: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ClassicAnchorContext {
    #[serde(default = "classic_anchor_context_version")]
    version: u32,
    #[serde(default, alias = "cooldown")]
    smart_shuffle: SchedulingSmartShuffleState,
    #[serde(default)]
    random_rotation_cycle_predecessors: BTreeMap<String, EntityId>,
}

fn classic_anchor_context_version() -> u32 {
    2
}

impl PlayoutSchedulingEngine for ClassicSchedulingEngine {
    fn schedule(&self, request: SchedulingRequest) -> Result<SchedulingResult, SchedulingError> {
        let mut items = request.reference_data.classic_schedule_items.clone();
        if items.is_empty() {
            return Ok(SchedulingResult::default());
        }

        items.sort_by_key(|item| (item.sort_order, item.id));
        let state = state_for_mode(&request, &items);

        let mut context = ClassicContext::new(request, items, state);
        context.apply_existing_collection_anchors();
        context.schedule();
        Ok(context.into_result())
    }
}

impl ClassicSchedulingEngine {
    #[must_use]
    pub fn get_start_time_after(
        current_time: DateTime<Utc>,
        schedule_item: &ProgramScheduleItem,
        schedule_fixed_start_time_behavior: FixedStartTimeBehavior,
        state: &PlayoutAnchor,
        is_peek: bool,
    ) -> DateTime<Utc> {
        get_start_time_after(
            current_time,
            schedule_item,
            schedule_fixed_start_time_behavior,
            &ClassicState {
                current_time,
                schedule_state: state.schedule_items_enumerator_state,
                multiple_remaining: state.multiple_remaining,
                duration_finish: state.duration_finish,
                in_flood: state.in_flood,
                in_duration_filler: state.in_duration_filler,
                next_guide_group: state.next_guide_group,
            },
            is_peek,
        )
    }
}

struct ClassicContext {
    request: SchedulingRequest,
    schedule_items: Vec<ProgramScheduleItem>,
    schedule_order: Vec<usize>,
    state: ClassicState,
    media: BTreeMap<String, MediaEnumerator>,
    playlists: BTreeMap<String, PlaylistEnumerator>,
    media_states: BTreeMap<String, CollectionEnumeratorState>,
    collection_anchors: Vec<PlayoutProgramScheduleAnchor>,
    filler_media: BTreeMap<EntityId, MediaEnumerator>,
    fill_group_states: BTreeMap<EntityId, CollectionEnumeratorState>,
    smart_shuffle_state: SchedulingSmartShuffleState,
    random_draw_states: BTreeMap<String, u64>,
    random_rotation_cycle_predecessors: BTreeMap<String, EntityId>,
    result: SchedulingResult,
}

impl ClassicContext {
    #[allow(clippy::too_many_lines)]
    fn new(
        request: SchedulingRequest,
        schedule_items: Vec<ProgramScheduleItem>,
        state: ClassicState,
    ) -> Self {
        let mut media = BTreeMap::new();
        let mut playlists = BTreeMap::new();
        let mut media_states = BTreeMap::new();
        let mut random_draw_states = BTreeMap::new();
        let collection_anchors = retained_collection_anchors(&request);
        let anchor_context = if request.mode == PlayoutBuildMode::Continue {
            request
                .playout
                .anchor
                .as_ref()
                .and_then(|anchor| anchor.context.as_deref())
                .and_then(|context| serde_json::from_str::<ClassicAnchorContext>(context).ok())
                .unwrap_or_default()
        } else {
            ClassicAnchorContext::default()
        };
        let smart_shuffle_state = anchor_context.smart_shuffle;
        let mut random_rotation_cycle_predecessors =
            anchor_context.random_rotation_cycle_predecessors;
        for item in &schedule_items {
            let key = item_key(item);
            let seed = request.playout.seed;
            if let Some(playlist_items) = request.reference_data.playlist_items_by_key.get(&key) {
                playlists.insert(key.clone(), PlaylistEnumerator::new(playlist_items.clone()));
                media_states.insert(key, CollectionEnumeratorState { seed, index: 0 });
                continue;
            }
            let mut enumerator = MediaEnumerator::new(
                key.clone(),
                request
                    .reference_data
                    .media_items_for_source(&super::SchedulingContentSource::direct(&key)),
            )
            .with_playback_order(item.playback_order);
            let has_retained_anchor = collection_anchors.iter().any(|anchor| {
                history_key_for_collection_key(&collection_key_for_anchor(anchor)) == key
            });
            if has_retained_anchor {
                media_states.insert(key.clone(), CollectionEnumeratorState { seed, index: 0 });
            } else {
                prepare_enumerator_items(
                    &mut enumerator,
                    item.playback_order,
                    seed_to_u64(request.playout.seed),
                    None,
                );
                // Matching ErsatzTV: Random Start Point only affects orders
                // that would otherwise always start from a fixed position
                // (the sorted-by-date/episode orders, and ShuffleInOrder's
                // own per-cycle start offset); Random/Shuffle/RandomRotation/
                // Marathon are already randomized by construction, so it
                // would be a no-op there.
                let random_start_point_applies = matches!(
                    item.playback_order,
                    PlaybackOrder::Chronological
                        | PlaybackOrder::SeasonEpisode
                        | PlaybackOrder::ShuffleInOrder
                );
                if request.reference_data.classic_random_start_point
                    && random_start_point_applies
                    && !enumerator.is_empty()
                {
                    let index = usize::try_from(
                        seed_to_u64(request.playout.seed) % enumerator.len() as u64,
                    )
                    .unwrap_or(0);
                    enumerator.set_index(index);
                    media_states.insert(
                        key.clone(),
                        CollectionEnumeratorState {
                            seed,
                            index: i32::try_from(index).unwrap_or_default(),
                        },
                    );
                } else {
                    media_states.insert(key.clone(), CollectionEnumeratorState { seed, index: 0 });
                }
                if item.playback_order == PlaybackOrder::Random && !enumerator.is_empty() {
                    let random_state = select_random_draw(&mut enumerator, seed_to_u64(seed), 0);
                    random_draw_states.insert(key.clone(), random_state);
                }
            }
            media.insert(key, enumerator);
        }
        random_rotation_cycle_predecessors.retain(|key, _| {
            media.get(key).is_some_and(|enumerator| {
                enumerator.playback_order == PlaybackOrder::RandomRotation
            })
        });
        let filler_media = request
            .reference_data
            .classic_filler_presets
            .iter()
            .map(|(id, preset)| {
                (
                    *id,
                    MediaEnumerator::new(
                        format!("classic-filler:{id}"),
                        preset.media_items.clone(),
                    )
                    .with_playback_order(PlaybackOrder::Chronological),
                )
            })
            .collect();

        // Mirror ErsatzTV `PlayoutBuilder.BuildPlayoutItems`:
        // `trimBefore = playoutStart.AddHours(-4)`, i.e. `now - 4h`. This trims
        // only genuinely-old items while retaining a short history window and
        // ALL future items. Using `state.current_time` is wrong for Continue
        // builds, where it equals the anchor's far-future `next_start` and would
        // delete the entire already-scheduled timeline.
        let remove_before = request.start - Duration::hours(4);
        let mut fill_group_states = BTreeMap::new();
        for index in &request.reference_data.existing_fill_group_indices {
            fill_group_states.insert(index.program_schedule_item_id, index.enumerator_state);
        }
        for item in &schedule_items {
            if item.fill_with_group_mode != FillWithGroupMode::None {
                fill_group_states
                    .entry(item.id)
                    .or_insert(CollectionEnumeratorState {
                        seed: request.playout.seed,
                        index: 0,
                    });
            }
        }

        let mut context = Self {
            request,
            schedule_items,
            schedule_order: Vec::new(),
            state,
            media,
            playlists,
            media_states,
            collection_anchors,
            filler_media,
            fill_group_states,
            smart_shuffle_state,
            random_draw_states,
            random_rotation_cycle_predecessors,
            result: SchedulingResult {
                remove_before: Some(remove_before),
                ..SchedulingResult::default()
            },
        };
        context.refresh_schedule_order();
        context
    }

    fn apply_existing_collection_anchors(&mut self) {
        let mut selected_anchors = BTreeMap::<String, PlayoutProgramScheduleAnchor>::new();
        for anchor in &self.collection_anchors {
            let key = history_key_for_collection_key(&collection_key_for_anchor(anchor));
            if selected_anchors
                .get(&key)
                .is_none_or(|existing| should_prefer_enumerator_anchor(anchor, existing))
            {
                selected_anchors.insert(key, anchor.clone());
            }
        }

        for anchor in selected_anchors.values() {
            let key = history_key_for_collection_key(&SchedulingCollectionKey {
                collection_type: anchor.collection_type,
                collection_id: anchor.collection_id,
                multi_collection_id: anchor.multi_collection_id,
                smart_collection_id: anchor.smart_collection_id,
                media_item_id: anchor.media_item_id,
                playlist_id: anchor.playlist_id,
                search_query: anchor.search_query.clone(),
                fake_collection_key: anchor.fake_collection_key.clone(),
            });
            if let Some(enumerator) = self.media.get_mut(&key) {
                let seed = seed_to_u64(anchor.enumerator_state.seed);
                let index = usize::try_from(anchor.enumerator_state.index).unwrap_or(0);
                match enumerator.playback_order {
                    PlaybackOrder::Random => {
                        let random_state = select_random_draw(enumerator, seed, index);
                        self.random_draw_states.insert(key.clone(), random_state);
                    }
                    PlaybackOrder::RandomRotation => {
                        let previous_key = self
                            .random_rotation_cycle_predecessors
                            .get(&key)
                            .and_then(|media_id| {
                                enumerator.items.iter().find(|item| item.id == *media_id)
                            })
                            .map(shuffle_group_key);
                        prepare_enumerator_items(
                            enumerator,
                            enumerator.playback_order,
                            seed,
                            previous_key,
                        );
                        enumerator.set_index(index);
                    }
                    _ => {
                        prepare_enumerator_items(enumerator, enumerator.playback_order, seed, None);
                        enumerator.set_index(index);
                    }
                }
                self.media_states.insert(key, anchor.enumerator_state);
            }
        }
    }

    fn schedule(&mut self) {
        let mut time_count = BTreeMap::<DateTime<Utc>, usize>::new();
        while self.state.current_time < self.request.finish {
            let count = time_count.entry(self.state.current_time).or_default();
            *count += 1;
            if *count > 6 {
                self.result.warnings.push(format!(
                    "classic scheduling stopped after repeated timestamp {}",
                    self.state.current_time
                ));
                break;
            }

            let index = self.current_schedule_index();
            let schedule_item = self.schedule_items[index].clone();
            let next_schedule_item = self.schedule_items[self.peek_schedule_index(1)].clone();
            let before = self.state.current_time;
            match schedule_item.playout_mode {
                ersatzrs_domain::PlayoutMode::One => {
                    self.schedule_one(&schedule_item, &next_schedule_item);
                }
                ersatzrs_domain::PlayoutMode::Multiple => {
                    self.schedule_multiple(&schedule_item, &next_schedule_item);
                }
                ersatzrs_domain::PlayoutMode::Duration => {
                    self.schedule_duration(&schedule_item);
                }
                ersatzrs_domain::PlayoutMode::Flood => {
                    self.schedule_flood(&schedule_item, &next_schedule_item);
                }
            }

            if self.state.current_time == before && self.current_schedule_index() == index {
                self.advance_schedule_item();
            }
        }
    }

    fn schedule_one(
        &mut self,
        schedule_item: &ProgramScheduleItem,
        next_schedule_item: &ProgramScheduleItem,
    ) {
        let item_start_time = self.start_time_after(schedule_item, false);
        if item_start_time >= self.request.finish {
            self.state.current_time = self.request.finish;
            return;
        }

        if self.schedule_current_media_item(schedule_item, item_start_time, None, "One") {
            self.advance_schedule_item();
            self.advance_media(schedule_item);
            self.advance_fill_group(schedule_item);
            self.state.current_time = self
                .result
                .added_items
                .last()
                .map_or(item_start_time, |item| item.finish);
            self.add_tail_and_fallback(schedule_item, self.filler_start_after(next_schedule_item));
            self.state.current_time = self
                .state
                .current_time
                .min(self.filler_start_after(next_schedule_item));
            self.state.next_guide_group += 1;
        }
    }

    fn schedule_multiple(
        &mut self,
        schedule_item: &ProgramScheduleItem,
        next_schedule_item: &ProgramScheduleItem,
    ) {
        let first_start = self.start_time_after(schedule_item, false);
        if first_start >= self.request.finish {
            self.state.current_time = self.request.finish;
            return;
        }

        let start_len = self.result.added_items.len();
        if self.state.multiple_remaining.is_none() {
            self.state.multiple_remaining = Some(multiple_count(
                schedule_item,
                self.enumerator_len(schedule_item),
                self.media_seed(schedule_item),
            ));
        }
        self.state.current_time = first_start;

        while self.state.current_time < self.request.finish
            && self.state.multiple_remaining.unwrap_or_default() > 0
        {
            let item_start_time = self.start_time_after(schedule_item, false);
            if !self.schedule_current_media_item(schedule_item, item_start_time, None, "Multiple") {
                break;
            }
            self.advance_media(schedule_item);
            self.state.current_time = self
                .result
                .added_items
                .last()
                .map_or(item_start_time, |item| item.finish);
            self.state.multiple_remaining = self.state.multiple_remaining.map(|value| value - 1);
            if schedule_item
                .custom_title
                .as_deref()
                .is_none_or(str::is_empty)
            {
                self.state.next_guide_group += 1;
            }
        }

        if self.state.multiple_remaining == Some(0) {
            self.state.multiple_remaining = None;
            self.advance_schedule_item();
            self.advance_fill_group(schedule_item);
        }

        self.add_tail_and_fallback(schedule_item, self.filler_start_after(next_schedule_item));
        self.state.current_time = self
            .state
            .current_time
            .min(self.filler_start_after(next_schedule_item));
        self.decrement_guide_group_if_multi_item_group(start_len);
        self.state.next_guide_group += 1;
    }

    fn schedule_duration(&mut self, schedule_item: &ProgramScheduleItem) {
        let start_len = self.result.added_items.len();
        let duration = schedule_item.duration.unwrap_or_else(Duration::zero);
        if duration <= Duration::zero() {
            self.advance_schedule_item();
            self.advance_fill_group(schedule_item);
            return;
        }

        let mut duration_until = self.state.duration_finish;
        while self.state.current_time < self.request.finish
            && self.state.current_time
                < self
                    .state
                    .duration_finish
                    .unwrap_or(DateTime::<Utc>::MAX_UTC)
        {
            let item_start_time = self.start_time_after(schedule_item, false);
            if self.state.duration_finish.is_none() {
                self.state.duration_finish = Some(item_start_time + duration);
                duration_until = self.state.duration_finish;
            }
            let Some(finish) = self.state.duration_finish else {
                break;
            };
            if item_start_time >= finish || item_start_time >= self.request.finish {
                self.state.current_time = self.request.finish;
                break;
            }
            let Some(media_item) = self.current_media(schedule_item) else {
                break;
            };
            if media_item.duration > duration {
                self.advance_media(schedule_item);
                continue;
            }
            if item_start_time + media_item.duration > finish {
                self.state.current_time = finish;
                self.state.duration_finish = None;
                self.advance_schedule_item();
                break;
            }
            let block_start_len = self.result.added_items.len();
            if self.schedule_media_item(
                schedule_item,
                &media_item,
                item_start_time,
                Some(finish),
                "Duration",
            ) {
                self.advance_media(schedule_item);
                self.state.current_time = self.result.added_items[block_start_len..]
                    .iter()
                    .map(|item| item.finish)
                    .max()
                    .unwrap_or(item_start_time + media_item.duration);
                if schedule_item
                    .custom_title
                    .as_deref()
                    .is_none_or(str::is_empty)
                {
                    self.state.next_guide_group += 1;
                }
            } else {
                break;
            }
        }

        if self
            .state
            .duration_finish
            .is_some_and(|finish| self.state.current_time >= finish)
        {
            self.state.current_time = self
                .state
                .duration_finish
                .unwrap_or(self.state.current_time);
            self.state.duration_finish = None;
            self.advance_schedule_item();
            self.advance_fill_group(schedule_item);
        } else if matches!(
            schedule_item.tail_mode,
            TailMode::Offline | TailMode::Filler
        ) {
            if let Some(until) = duration_until {
                match schedule_item.tail_mode {
                    TailMode::Filler => self.add_tail_and_fallback(schedule_item, until),
                    TailMode::Offline => self.add_fallback(schedule_item, until),
                    TailMode::None | TailMode::Slate => {}
                }
                self.state.current_time = until;
            }
        }
        self.decrement_guide_group_if_multi_item_group(start_len);
        self.state.next_guide_group += 1;
    }

    fn schedule_flood(
        &mut self,
        schedule_item: &ProgramScheduleItem,
        next_schedule_item: &ProgramScheduleItem,
    ) {
        let mut scheduled_any = false;
        let start_len = self.result.added_items.len();
        while self.state.current_time < self.request.finish {
            let item_start_time = self.start_time_after(schedule_item, false);
            if item_start_time >= self.request.finish {
                self.state.current_time = self.request.finish;
                break;
            }

            let Some(media_item) = self.current_media(schedule_item) else {
                break;
            };
            let peek_start = if schedule_item.id != next_schedule_item.id
                && next_schedule_item.start_type == StartType::Fixed
            {
                let mut peek_state = self.state.clone();
                peek_state.in_flood = true;
                get_start_time_after(
                    self.state.current_time,
                    next_schedule_item,
                    self.schedule_fixed_start_time_behavior(),
                    &peek_state,
                    true,
                )
            } else {
                DateTime::<Utc>::MAX_UTC
            };

            let duration = if media_item.duration == Duration::zero() {
                if peek_start != item_start_time && peek_start < self.request.finish {
                    peek_start - item_start_time
                } else {
                    self.request.finish - item_start_time
                }
            } else {
                media_item.duration
            };

            if peek_start >= item_start_time && peek_start < item_start_time + duration {
                break;
            }

            let effective_media_item = if duration == media_item.duration {
                media_item.clone()
            } else {
                SchedulingMediaItem {
                    duration,
                    ..media_item.clone()
                }
            };
            if self.schedule_media_item(
                schedule_item,
                &effective_media_item,
                item_start_time,
                None,
                "Flood",
            ) {
                scheduled_any = true;
                self.advance_media(schedule_item);
                self.state.current_time = item_start_time + duration;
                self.state.in_flood = true;
                if schedule_item
                    .custom_title
                    .as_deref()
                    .is_none_or(str::is_empty)
                {
                    self.state.next_guide_group += 1;
                }
            } else {
                break;
            }
        }

        self.state.in_flood = scheduled_any && self.state.current_time >= self.request.finish;
        if !self.state.in_flood && scheduled_any {
            self.advance_schedule_item();
            self.advance_fill_group(schedule_item);
        }
        self.decrement_guide_group_if_multi_item_group(start_len);
        self.add_tail_and_fallback(schedule_item, self.filler_start_after(next_schedule_item));
        self.state.next_guide_group += 1;
    }

    fn schedule_current_media_item(
        &mut self,
        schedule_item: &ProgramScheduleItem,
        start: DateTime<Utc>,
        guide_finish: Option<DateTime<Utc>>,
        scheduler: &'static str,
    ) -> bool {
        let Some(media_item) = self.current_media(schedule_item) else {
            self.result.warnings.push(format!(
                "classic schedule item {} skipped empty content",
                schedule_item.id
            ));
            return false;
        };
        self.schedule_media_item(schedule_item, &media_item, start, guide_finish, scheduler)
    }

    fn schedule_media_item(
        &mut self,
        schedule_item: &ProgramScheduleItem,
        media_item: &SchedulingMediaItem,
        start: DateTime<Utc>,
        guide_finish: Option<DateTime<Utc>>,
        scheduler: &'static str,
    ) -> bool {
        if start >= self.request.finish {
            return false;
        }
        let Some(mut items) =
            self.build_media_item_block(schedule_item, media_item, start, guide_finish, scheduler)
        else {
            return false;
        };
        let key = item_key(schedule_item);
        for item in items.drain(..) {
            let history_key = item
                .scheduling_context
                .as_deref()
                .and_then(filler_id_from_context)
                .map_or_else(|| key.clone(), |id| format!("classic-filler:{id}"));
            let history_media = if item.media_item_id == media_item.id {
                media_item.clone()
            } else {
                SchedulingMediaItem::new(item.media_item_id, item.finish - item.start)
            };
            self.result.added_history.push(history(
                self.request.playout.id,
                history_key,
                &item,
                None,
                item.scheduling_context
                    .as_deref()
                    .and_then(index_from_context)
                    .unwrap_or_default(),
                history_details_for_media_item(&history_media),
                false,
            ));
            self.record_smart_shuffle_item(schedule_item, &item);
            self.result.added_items.push(item);
        }
        true
    }

    fn build_media_item_block(
        &mut self,
        schedule_item: &ProgramScheduleItem,
        media_item: &SchedulingMediaItem,
        start: DateTime<Utc>,
        guide_finish: Option<DateTime<Utc>>,
        scheduler: &'static str,
    ) -> Option<Vec<PlayoutItem>> {
        let before_filler_media = self.filler_media.clone();
        let mut items = Vec::new();
        let mut current = start;
        if let Some(pre_roll_id) = schedule_item.pre_roll_filler_id {
            let filler = self.add_count_or_duration_filler(
                schedule_item,
                pre_roll_id,
                current,
                FillerKind::PreRoll,
            );
            current = filler.last().map_or(current, |item| item.finish);
            items.extend(filler);
        }

        let mut item = self.primary_playout_item(schedule_item, media_item, current, scheduler);
        item.guide_finish = guide_finish;
        current = item.finish;
        items.push(item);

        for filler_id in [
            schedule_item.mid_roll_filler_id,
            schedule_item.post_roll_filler_id,
        ]
        .into_iter()
        .flatten()
        {
            let filler = self.add_count_or_duration_filler(
                schedule_item,
                filler_id,
                current,
                FillerKind::PostRoll,
            );
            current = filler.last().map_or(current, |item| item.finish);
            items.extend(filler);
        }

        if self.pad_filler_count(schedule_item) > 1 {
            self.filler_media = before_filler_media;
            self.result.warnings.push(format!(
                "classic schedule item {} skipped filler because multiple pad fillers are configured",
                schedule_item.id
            ));
            return Some(vec![self.primary_playout_item(
                schedule_item,
                media_item,
                start,
                scheduler,
            )]);
        }
        if let Some(pad_filler_id) = self.pad_filler_id(schedule_item) {
            let pad = self.add_pad_filler(schedule_item, pad_filler_id, start, current);
            let Some(insert_after_primary) = items.iter().rposition(|item| {
                item.scheduling_context
                    .as_deref()
                    .and_then(filler_id_from_context)
                    .is_none()
            }) else {
                items.extend(pad);
                current = items.last().map_or(current, |item| item.finish);
                if guide_finish.is_some_and(|finish| current > finish) {
                    self.filler_media = before_filler_media;
                    return None;
                }
                return Some(items);
            };
            if self
                .request
                .reference_data
                .classic_filler_presets
                .get(&pad_filler_id)
                .is_some_and(|preset| preset.filler_kind == FillerKind::PreRoll)
            {
                items.splice(0..0, pad);
            } else {
                let insert_at = insert_after_primary + 1;
                items.splice(insert_at..insert_at, pad);
            }
            current = start;
            for item in &mut items {
                let duration = item.finish - item.start;
                item.start = current;
                item.finish = current + duration;
                item.in_point = Duration::zero();
                item.out_point = duration;
                current = item.finish;
            }
        }

        if guide_finish.is_some_and(|finish| current > finish) {
            self.filler_media = before_filler_media;
            return None;
        }

        Some(items)
    }

    fn primary_playout_item(
        &self,
        schedule_item: &ProgramScheduleItem,
        media_item: &SchedulingMediaItem,
        start: DateTime<Utc>,
        scheduler: &'static str,
    ) -> PlayoutItem {
        let key = item_key(schedule_item);
        let (enumerator_name, enumerator_seed, enumerator_index) =
            self.media
                .get(&key)
                .map_or(("Random", self.request.playout.seed, 0), |enumerator| {
                    let seed = self
                        .media_states
                        .get(&key)
                        .map_or_else(|| self.request.playout.seed, |state| state.seed);
                    let index = if enumerator.playback_order == PlaybackOrder::Random {
                        self.media_states.get(&key).map_or(0, |state| state.index)
                    } else {
                        i32::try_from(enumerator.current_index()).unwrap_or_default()
                    };
                    (enumerator.scheduling_context_name(), seed, index)
                });
        let scheduling_context = Self::scheduling_context(
            schedule_item,
            None,
            scheduler,
            enumerator_name,
            enumerator_seed,
            enumerator_index,
        );
        let mut item = playout_item(
            self.request.playout.id,
            media_item,
            start,
            media_item.duration,
            self.state.next_guide_group,
            if schedule_item.guide_mode == GuideMode::Filler {
                FillerKind::GuideMode
            } else {
                FillerKind::None
            },
            schedule_item.custom_title.clone(),
            false,
            schedule_item.watermark_ids.clone(),
            schedule_item.graphics_element_ids.clone(),
            scheduling_context,
        );
        if schedule_item.preferred_audio_language_code.is_some() {
            item.preferred_audio_language_code
                .clone_from(&schedule_item.preferred_audio_language_code);
        }
        if schedule_item.preferred_audio_title.is_some() {
            item.preferred_audio_title
                .clone_from(&schedule_item.preferred_audio_title);
        }
        if schedule_item.preferred_subtitle_language_code.is_some() {
            item.preferred_subtitle_language_code
                .clone_from(&schedule_item.preferred_subtitle_language_code);
        }
        if schedule_item.subtitle_mode.is_some() {
            item.subtitle_mode = schedule_item.subtitle_mode;
        }
        if schedule_item.pinned_stream_selection.is_some() {
            item.pinned_stream_selection = schedule_item.pinned_stream_selection;
        }
        item
    }

    fn scheduling_context(
        schedule_item: &ProgramScheduleItem,
        filler_preset_id: Option<EntityId>,
        scheduler: &'static str,
        enumerator_name: &'static str,
        enumerator_seed: i32,
        enumerator_index: i32,
    ) -> Option<String> {
        serde_json::to_string(&ClassicSchedulingContext {
            scheduler,
            schedule_id: schedule_item.program_schedule_id,
            item_id: schedule_item.id,
            filler_preset_id,
            enumerator: enumerator_name,
            seed: enumerator_seed,
            index: enumerator_index,
        })
        .ok()
    }

    fn add_count_or_duration_filler(
        &mut self,
        schedule_item: &ProgramScheduleItem,
        filler_id: EntityId,
        start: DateTime<Utc>,
        no_chapter_kind: FillerKind,
    ) -> Vec<PlayoutItem> {
        let Some(preset) = self
            .request
            .reference_data
            .classic_filler_presets
            .get(&filler_id)
            .cloned()
        else {
            return Vec::new();
        };
        let filler_kind = Self::effective_filler_kind(schedule_item, no_chapter_kind);
        match preset.filler_mode {
            FillerMode::Count => self.add_count_filler(
                schedule_item,
                filler_id,
                start,
                filler_kind,
                preset.count.unwrap_or_default().max(0),
                preset.allow_watermarks,
            ),
            FillerMode::RandomCount => {
                let max = preset.count.unwrap_or_default().max(0);
                let count = if max == 0 {
                    0
                } else {
                    let filler_seed = i32::try_from(filler_id).unwrap_or_default();
                    next_i32_seed(self.request.playout.seed.wrapping_add(filler_seed))
                        .unsigned_abs()
                        % u32::try_from(max + 1).unwrap_or(1)
                };
                self.add_count_filler(
                    schedule_item,
                    filler_id,
                    start,
                    filler_kind,
                    i32::try_from(count).unwrap_or_default(),
                    preset.allow_watermarks,
                )
            }
            FillerMode::Duration => self.add_duration_filler(
                schedule_item,
                filler_id,
                start,
                filler_kind,
                preset.duration.unwrap_or_else(Duration::zero),
                preset.allow_watermarks,
            ),
            FillerMode::Pad | FillerMode::None => Vec::new(),
        }
    }

    fn add_count_filler(
        &mut self,
        schedule_item: &ProgramScheduleItem,
        filler_id: EntityId,
        start: DateTime<Utc>,
        filler_kind: FillerKind,
        count: i32,
        allow_watermarks: bool,
    ) -> Vec<PlayoutItem> {
        let mut items = Vec::new();
        let mut current = start;
        for _ in 0..count {
            let Some(item) = self.current_filler_item(
                schedule_item,
                filler_id,
                current,
                filler_kind,
                allow_watermarks,
            ) else {
                break;
            };
            current = item.finish;
            items.push(item);
            self.advance_filler(filler_id);
        }
        items
    }

    fn add_duration_filler(
        &mut self,
        schedule_item: &ProgramScheduleItem,
        filler_id: EntityId,
        start: DateTime<Utc>,
        filler_kind: FillerKind,
        duration: Duration,
        allow_watermarks: bool,
    ) -> Vec<PlayoutItem> {
        let mut items = Vec::new();
        let mut current = start;
        let mut remaining = duration;
        let mut attempts = 0usize;
        while remaining > Duration::zero() {
            attempts += 1;
            if attempts > 64 {
                self.result.warnings.push(format!(
                    "classic schedule item {} stopped filler {} after repeated skips",
                    schedule_item.id, filler_id
                ));
                break;
            }
            let Some(media_item) = self
                .filler_media
                .get(&filler_id)
                .and_then(MediaEnumerator::current)
                .cloned()
            else {
                break;
            };
            if media_item.duration <= Duration::zero() || media_item.duration > remaining {
                self.advance_filler(filler_id);
                continue;
            }
            let Some(item) = self.current_filler_item(
                schedule_item,
                filler_id,
                current,
                filler_kind,
                allow_watermarks,
            ) else {
                break;
            };
            remaining -= item.finish - item.start;
            current = item.finish;
            items.push(item);
            self.advance_filler(filler_id);
        }
        items
    }

    fn add_pad_filler(
        &mut self,
        schedule_item: &ProgramScheduleItem,
        filler_id: EntityId,
        block_start: DateTime<Utc>,
        block_finish: DateTime<Utc>,
    ) -> Vec<PlayoutItem> {
        let Some(preset) = self
            .request
            .reference_data
            .classic_filler_presets
            .get(&filler_id)
            .cloned()
        else {
            return Vec::new();
        };
        let Some(nearest_minute) = preset.pad_to_nearest_minute.filter(|value| *value > 0) else {
            self.result.warnings.push(format!(
                "classic schedule item {} skipped pad filler {} without pad target",
                schedule_item.id, filler_id
            ));
            return Vec::new();
        };
        let elapsed = block_finish - block_start;
        let elapsed_minutes = elapsed.num_minutes();
        let target_minutes = ((elapsed_minutes + i64::from(nearest_minute))
            / i64::from(nearest_minute))
            * i64::from(nearest_minute);
        let target = block_start + Duration::minutes(target_minutes);
        let remaining = target - block_finish;
        if remaining <= Duration::zero() {
            return Vec::new();
        }
        self.add_duration_filler(
            schedule_item,
            filler_id,
            block_finish,
            Self::effective_filler_kind(schedule_item, preset.filler_kind),
            remaining,
            preset.allow_watermarks,
        )
    }

    fn current_filler_item(
        &self,
        schedule_item: &ProgramScheduleItem,
        filler_id: EntityId,
        start: DateTime<Utc>,
        filler_kind: FillerKind,
        allow_watermarks: bool,
    ) -> Option<PlayoutItem> {
        let enumerator = self.filler_media.get(&filler_id)?;
        let media_item = enumerator.current()?;
        let scheduling_context = Self::scheduling_context(
            schedule_item,
            Some(filler_id),
            "Filler",
            enumerator.scheduling_context_name(),
            self.request.playout.seed,
            i32::try_from(enumerator.current_index()).unwrap_or_default(),
        );
        Some(playout_item(
            self.request.playout.id,
            media_item,
            start,
            media_item.duration,
            self.state.next_guide_group,
            filler_kind,
            None,
            !allow_watermarks,
            Vec::new(),
            Vec::new(),
            scheduling_context,
        ))
    }

    fn add_tail_and_fallback(&mut self, schedule_item: &ProgramScheduleItem, until: DateTime<Utc>) {
        if let Some(tail_filler_id) = schedule_item.tail_filler_id {
            let filler = self.add_duration_filler(
                schedule_item,
                tail_filler_id,
                self.state.current_time,
                Self::effective_filler_kind(schedule_item, FillerKind::Tail),
                until - self.state.current_time,
                self.request
                    .reference_data
                    .classic_filler_presets
                    .get(&tail_filler_id)
                    .is_none_or(|preset| preset.allow_watermarks),
            );
            self.append_filler_items(schedule_item, filler);
        }
        self.add_fallback(schedule_item, until);
    }

    fn add_fallback(&mut self, schedule_item: &ProgramScheduleItem, until: DateTime<Utc>) {
        if self.state.current_time >= until {
            return;
        }
        let Some(fallback_filler_id) = schedule_item.fallback_filler_id else {
            return;
        };
        let allow_watermarks = self
            .request
            .reference_data
            .classic_filler_presets
            .get(&fallback_filler_id)
            .is_none_or(|preset| preset.allow_watermarks);
        let Some(mut item) = self.current_filler_item(
            schedule_item,
            fallback_filler_id,
            self.state.current_time,
            FillerKind::Fallback,
            allow_watermarks,
        ) else {
            return;
        };
        item.finish = until;
        item.out_point = Duration::zero();
        self.advance_filler(fallback_filler_id);
        self.append_filler_items(schedule_item, vec![item]);
    }

    fn append_filler_items(
        &mut self,
        schedule_item: &ProgramScheduleItem,
        items: Vec<PlayoutItem>,
    ) {
        for item in items {
            let media_item = SchedulingMediaItem::new(item.media_item_id, item.finish - item.start);
            let history_key = item
                .scheduling_context
                .as_deref()
                .and_then(filler_id_from_context)
                .map_or_else(
                    || item_key(schedule_item),
                    |id| format!("classic-filler:{id}"),
                );
            self.state.current_time = item.finish;
            self.result.added_history.push(history(
                self.request.playout.id,
                history_key,
                &item,
                None,
                item.scheduling_context
                    .as_deref()
                    .and_then(index_from_context)
                    .unwrap_or_default(),
                history_details_for_media_item(&media_item),
                false,
            ));
            self.result.added_items.push(item);
        }
    }

    fn advance_filler(&mut self, filler_id: EntityId) {
        if let Some(enumerator) = self.filler_media.get_mut(&filler_id) {
            enumerator.advance();
        }
    }

    fn effective_filler_kind(
        schedule_item: &ProgramScheduleItem,
        filler_kind: FillerKind,
    ) -> FillerKind {
        if schedule_item.guide_mode == GuideMode::Filler {
            FillerKind::GuideMode
        } else if filler_kind == FillerKind::MidRoll {
            FillerKind::PostRoll
        } else {
            filler_kind
        }
    }

    fn pad_filler_count(&self, schedule_item: &ProgramScheduleItem) -> usize {
        [
            schedule_item.pre_roll_filler_id,
            schedule_item.mid_roll_filler_id,
            schedule_item.post_roll_filler_id,
        ]
        .into_iter()
        .flatten()
        .filter(|id| {
            self.request
                .reference_data
                .classic_filler_presets
                .get(id)
                .is_some_and(|preset| preset.filler_mode == FillerMode::Pad)
        })
        .count()
    }

    fn pad_filler_id(&self, schedule_item: &ProgramScheduleItem) -> Option<EntityId> {
        [
            schedule_item.pre_roll_filler_id,
            schedule_item.mid_roll_filler_id,
            schedule_item.post_roll_filler_id,
        ]
        .into_iter()
        .flatten()
        .find(|id| {
            self.request
                .reference_data
                .classic_filler_presets
                .get(id)
                .is_some_and(|preset| preset.filler_mode == FillerMode::Pad)
        })
    }

    fn into_result(mut self) -> SchedulingResult {
        self.result.clear_items = matches!(
            self.request.mode,
            PlayoutBuildMode::Refresh | PlayoutBuildMode::Reset
        );
        let current_item = self.schedule_items[self.current_schedule_index()].clone();
        let mut next_start = self.start_time_after(&current_item, false);
        if let Some(last_finish) = self.result.added_items.iter().map(|item| item.finish).max() {
            next_start = next_start.max(last_finish);
        }
        self.result.anchor = Some(PlayoutAnchor {
            schedule_items_enumerator_state: self.state.schedule_state,
            next_start,
            multiple_remaining: self.state.multiple_remaining,
            duration_finish: self
                .state
                .duration_finish
                .filter(|finish| *finish > next_start),
            in_flood: self.state.in_flood,
            in_duration_filler: self.state.in_duration_filler,
            next_guide_group: self.state.next_guide_group,
            context: (self.request.reference_data.has_active_smart_shuffle()
                || !self.random_rotation_cycle_predecessors.is_empty())
            .then(|| {
                serde_json::to_string(&ClassicAnchorContext {
                    version: classic_anchor_context_version(),
                    smart_shuffle: self.smart_shuffle_state.clone(),
                    random_rotation_cycle_predecessors: self
                        .random_rotation_cycle_predecessors
                        .clone(),
                })
                .unwrap_or_default()
            }),
            ..PlayoutAnchor::default()
        });
        self.result.program_schedule_anchors = self.program_schedule_anchors();
        self.result.fill_group_indices = self.fill_group_indices();
        self.result.history_to_remove.extend(cleanup_history(
            &self.request.reference_data.existing_history,
            self.request.start,
            self.request.reference_data.max_playout_offset,
        ));
        self.result
    }

    fn program_schedule_anchors(&self) -> Vec<PlayoutProgramScheduleAnchor> {
        let mut result = Vec::new();
        let mut seen_keys = BTreeSet::new();
        for schedule_item in &self.schedule_items {
            let key = collection_key(schedule_item);
            let key_string = history_key_for_collection_key(&key);
            if !seen_keys.insert(key_string.clone()) {
                continue;
            }
            let Some(enumerator) = self.media.get(&key_string) else {
                continue;
            };
            let existing = self
                .collection_anchors
                .iter()
                .find(|anchor| anchor.anchor_date.is_none() && anchor_matches_key(anchor, &key));
            // Match EF entity semantics: zero means new and lets the database
            // allocate the globally unique primary key. Existing entities keep
            // their persisted identity.
            let id = existing.map_or(0, |anchor| anchor.id);
            result.push(PlayoutProgramScheduleAnchor {
                id,
                playout_id: self.request.playout.id,
                anchor_date: None,
                collection_type: key.collection_type,
                collection_id: key.collection_id,
                multi_collection_id: key.multi_collection_id,
                smart_collection_id: key.smart_collection_id,
                rerun_collection_id: None,
                media_item_id: key.media_item_id,
                playlist_id: key.playlist_id,
                search_query: key.search_query,
                fake_collection_key: key.fake_collection_key,
                enumerator_state: CollectionEnumeratorState {
                    seed: self
                        .media_states
                        .get(&key_string)
                        .map_or_else(|| self.request.playout.seed, |state| state.seed),
                    index: if enumerator.playback_order == PlaybackOrder::Random {
                        self.media_states
                            .get(&key_string)
                            .map_or(0, |state| state.index)
                    } else {
                        i32::try_from(enumerator.current_index()).unwrap_or_default()
                    },
                },
            });
        }
        result.extend(
            self.collection_anchors
                .iter()
                .filter(|anchor| anchor.anchor_date.is_some())
                .cloned(),
        );
        result
    }

    fn fill_group_indices(&self) -> Vec<PlayoutScheduleItemFillGroupIndex> {
        let mut result = Vec::new();
        for schedule_item in self
            .schedule_items
            .iter()
            .filter(|item| item.fill_with_group_mode != FillWithGroupMode::None)
        {
            let existing = self
                .request
                .reference_data
                .existing_fill_group_indices
                .iter()
                .find(|index| index.program_schedule_item_id == schedule_item.id);
            let id = existing.map_or(0, |index| index.id);
            result.push(PlayoutScheduleItemFillGroupIndex {
                id,
                playout_id: self.request.playout.id,
                program_schedule_item_id: schedule_item.id,
                enumerator_state: self.fill_group_states.get(&schedule_item.id).map_or_else(
                    || CollectionEnumeratorState {
                        seed: self.request.playout.seed,
                        index: 0,
                    },
                    |state| *state,
                ),
            });
        }
        result
    }

    fn current_schedule_index(&self) -> usize {
        let len = self.schedule_order.len();
        if len == 0 {
            0
        } else {
            self.schedule_order[usize::try_from(self.state.schedule_state.index).unwrap_or(0) % len]
        }
    }

    fn peek_schedule_index(&self, offset: usize) -> usize {
        let len = self.schedule_order.len();
        let logical_index =
            (usize::try_from(self.state.schedule_state.index).unwrap_or(0) + offset) % len;
        self.schedule_order[logical_index]
    }

    fn advance_schedule_item(&mut self) {
        let len = i32::try_from(self.schedule_items.len()).unwrap_or(1).max(1);
        let next = self.state.schedule_state.index + 1;
        if next >= len {
            // Capture which schedule item was just played (the last position
            // of the outgoing schedule_order) before reshuffling, so the new
            // cycle can avoid starting with the same item again.
            let previous_last = self.schedule_order.last().copied();
            self.state.schedule_state.index = 0;
            if self.request.reference_data.classic_shuffle_schedule_items {
                self.state.schedule_state.seed = next_i32_seed(self.state.schedule_state.seed);
                self.refresh_schedule_order();
                // Mirror prepare_next_cycle's anti-repeat guard for the
                // per-collection shuffle, at the schedule-item level: reroll
                // until the new first schedule item isn't the one that just
                // finished playing.
                while self.schedule_order.len() > 1
                    && self.schedule_order.first().copied() == previous_last
                {
                    self.state.schedule_state.seed = next_i32_seed(self.state.schedule_state.seed);
                    self.refresh_schedule_order();
                }
            }
        } else {
            self.state.schedule_state.index = next;
        }
        self.state.in_flood = false;
    }

    fn media(&self, schedule_item: &ProgramScheduleItem) -> &MediaEnumerator {
        self.media.get(&item_key(schedule_item)).unwrap_or_else(|| {
            self.media
                .values()
                .next()
                .expect("classic media map is initialized")
        })
    }

    fn current_media(
        &mut self,
        schedule_item: &ProgramScheduleItem,
    ) -> Option<SchedulingMediaItem> {
        let content_key = item_key(schedule_item);
        let policy_key = classic_schedule_item_policy_key(schedule_item);
        let policy = self
            .request
            .reference_data
            .smart_shuffle_policy_for_unit(&policy_key)
            .clone();
        let tags_by_media_id = self
            .request
            .reference_data
            .smart_shuffle_tags_for_unit(&policy_key)
            .clone();
        let scope_key = policy.scope_key(
            self.request.playout.id,
            Some(schedule_item.id),
            Some(&content_key),
        );
        if let Some(playlist) = self.playlists.get_mut(&content_key) {
            playlist.select_with_smart_shuffle(
                &policy,
                &self.smart_shuffle_state,
                &scope_key,
                self.state.current_time,
                &tags_by_media_id,
            );
        } else if let Some(enumerator) = self.media.get_mut(&content_key) {
            enumerator.select_with_smart_shuffle(
                &policy,
                &self.smart_shuffle_state,
                &scope_key,
                self.state.current_time,
                &tags_by_media_id,
            );
        }
        self.playlists
            .get(&content_key)
            .and_then(PlaylistEnumerator::current)
            .cloned()
            .or_else(|| {
                self.media
                    .get(&content_key)
                    .and_then(MediaEnumerator::current)
                    .cloned()
            })
    }

    fn record_smart_shuffle_item(
        &mut self,
        schedule_item: &ProgramScheduleItem,
        item: &PlayoutItem,
    ) {
        let content_key = item_key(schedule_item);
        let policy_key = classic_schedule_item_policy_key(schedule_item);
        let policy = self
            .request
            .reference_data
            .smart_shuffle_policy_for_unit(&policy_key)
            .clone();
        let tags_by_media_id = self
            .request
            .reference_data
            .smart_shuffle_tags_for_unit(&policy_key);
        let scope_key = policy.scope_key(
            self.request.playout.id,
            Some(schedule_item.id),
            Some(&content_key),
        );
        let playback_order = self.playlists.get(&content_key).map_or(
            schedule_item.playback_order,
            PlaylistEnumerator::current_playback_order,
        );
        let unit = if let Some(playlist) = self.playlists.get(&content_key) {
            let Some(unit) = playlist.smart_shuffle_unit(tags_by_media_id) else {
                return;
            };
            unit
        } else {
            SchedulingSmartShuffleUnit::single(
                item.media_item_id,
                self.request
                    .reference_data
                    .smart_shuffle_tags_for_media(&policy_key, item.media_item_id)
                    .clone(),
                item.finish - item.start,
            )
        };
        self.smart_shuffle_state.record_unit(
            &policy,
            self.request.playout.id,
            &scope_key,
            item.start,
            &unit,
            SchedulingSmartShuffleRecordContext::new(
                super::is_randomized_playback_order(playback_order),
                item.filler_kind != FillerKind::None,
            ),
        );
    }

    fn enumerator_len(&self, schedule_item: &ProgramScheduleItem) -> usize {
        let key = item_key(schedule_item);
        self.playlists
            .get(&key)
            .map_or_else(|| self.media(schedule_item).len(), PlaylistEnumerator::len)
    }

    fn advance_media(&mut self, schedule_item: &ProgramScheduleItem) {
        let key = item_key(schedule_item);
        if let Some(playlist) = self.playlists.get_mut(&key) {
            playlist.advance();
            if let Some(state) = self.media_states.get_mut(&key) {
                state.index = i32::try_from(playlist.index).unwrap_or_default();
            }
            return;
        }
        if let Some(enumerator) = self.media.get_mut(&key) {
            if enumerator.playback_order == PlaybackOrder::Random {
                if enumerator.is_empty() {
                    return;
                }
                let media_state =
                    self.media_states
                        .entry(key.clone())
                        .or_insert(CollectionEnumeratorState {
                            seed: self.request.playout.seed,
                            index: 0,
                        });
                let draw_index = usize::try_from(media_state.index).unwrap_or_default();
                let random_state = self.random_draw_states.entry(key).or_insert_with(|| {
                    random_state_for_draw(seed_to_u64(media_state.seed), draw_index)
                });
                *random_state = next_random_state(*random_state);
                enumerator.set_index(random_index(*random_state, enumerator.len()));
                media_state.index = media_state.index.saturating_add(1);
                return;
            }

            let tail_media_id = enumerator.current().map(|item| item.id);
            let tail_group_key = enumerator.current().map(shuffle_group_key);
            enumerator.advance();
            let index = i32::try_from(enumerator.current_index()).unwrap_or_default();
            let should_reshuffle = enumerator.current_index() == 0
                && matches!(
                    enumerator.playback_order,
                    PlaybackOrder::Shuffle
                        | PlaybackOrder::ShuffleInOrder
                        | PlaybackOrder::RandomRotation
                        | PlaybackOrder::Marathon
                        | PlaybackOrder::MultiEpisodeShuffle
                );
            let playback_order = enumerator.playback_order;
            let mut next_seed = None;
            if let Some(state) = self.media_states.get_mut(&key) {
                state.index = index;
                if should_reshuffle {
                    state.seed = next_i32_seed(state.seed);
                    next_seed = Some(state.seed);
                }
            }
            if let Some(seed) = next_seed {
                let seed = prepare_next_cycle(
                    enumerator,
                    playback_order,
                    seed,
                    tail_media_id,
                    tail_group_key,
                );
                if playback_order == PlaybackOrder::RandomRotation {
                    if let Some(tail_media_id) = tail_media_id {
                        self.random_rotation_cycle_predecessors
                            .insert(key.clone(), tail_media_id);
                    }
                }
                if let Some(state) = self.media_states.get_mut(&key) {
                    state.seed = seed;
                }
            }
        }
    }

    fn start_time_after(
        &self,
        schedule_item: &ProgramScheduleItem,
        is_peek: bool,
    ) -> DateTime<Utc> {
        get_start_time_after(
            self.state.current_time,
            schedule_item,
            self.schedule_fixed_start_time_behavior(),
            &self.state,
            is_peek,
        )
    }

    fn filler_start_after(&self, schedule_item: &ProgramScheduleItem) -> DateTime<Utc> {
        self.start_time_after(schedule_item, false)
            .min(self.request.finish)
    }

    fn schedule_fixed_start_time_behavior(&self) -> FixedStartTimeBehavior {
        self.request
            .reference_data
            .classic_fixed_start_time_behavior
    }

    fn media_seed(&self, schedule_item: &ProgramScheduleItem) -> u64 {
        seed_to_u64(
            self.media_states
                .get(&item_key(schedule_item))
                .map_or_else(|| self.request.playout.seed, |state| state.seed),
        )
    }

    fn refresh_schedule_order(&mut self) {
        self.schedule_order = (0..self.schedule_items.len()).collect();
        if self.request.reference_data.classic_shuffle_schedule_items {
            shuffle_with_seed(
                &mut self.schedule_order,
                seed_to_u64(self.state.schedule_state.seed),
            );
        }
    }

    fn advance_fill_group(&mut self, schedule_item: &ProgramScheduleItem) {
        if schedule_item.fill_with_group_mode == FillWithGroupMode::None {
            return;
        }
        let Some(state) = self.fill_group_states.get_mut(&schedule_item.id) else {
            return;
        };
        state.index = state.index.saturating_add(1);
        if schedule_item.fill_with_group_mode == FillWithGroupMode::FillWithShuffledGroups {
            state.seed = next_i32_seed(state.seed);
        }
    }

    fn decrement_guide_group_if_multi_item_group(&mut self, start_len: usize) {
        let groups = self.result.added_items[start_len..]
            .iter()
            .map(|item| item.guide_group)
            .collect::<BTreeSet<_>>();
        if groups.len() > 1 {
            self.state.next_guide_group -= 1;
        }
    }
}

fn start_state(request: &SchedulingRequest, items: &[ProgramScheduleItem]) -> ClassicState {
    let index = 0;
    let next_start = items
        .first()
        .map_or(request.start, |item| match item.start_type {
            StartType::Fixed => {
                midnight(request.start) + item.start_time.unwrap_or_else(Duration::zero)
            }
            StartType::Dynamic => midnight(request.start),
        });

    ClassicState {
        current_time: next_start.max(request.start),
        schedule_state: CollectionEnumeratorState {
            seed: request.playout.seed,
            index,
        },
        multiple_remaining: None,
        duration_finish: None,
        in_flood: false,
        in_duration_filler: false,
        next_guide_group: 1,
    }
}

fn state_for_mode(request: &SchedulingRequest, items: &[ProgramScheduleItem]) -> ClassicState {
    match request.mode {
        PlayoutBuildMode::Continue => request.playout.anchor.as_ref().map_or_else(
            || start_state(request, items),
            |anchor| anchor_state(anchor, request),
        ),
        PlayoutBuildMode::Refresh => refresh_state(request, items),
        PlayoutBuildMode::Reset => start_state(request, items),
    }
}

fn refresh_state(request: &SchedulingRequest, items: &[ProgramScheduleItem]) -> ClassicState {
    let mut state = start_state(request, items);
    // A Refresh clears the generated window. The playout anchor belongs to
    // the end of that old window, so only a dated collection checkpoint can
    // move the rebuild start; schedule-item state must be derived afresh for
    // that time-aligned start.
    if let Some(anchor_date) = retained_refresh_anchors(request)
        .iter()
        .filter_map(|anchor| anchor.anchor_date)
        .min()
    {
        state.current_time = anchor_date.max(request.start);
    }
    state
}

fn anchor_state(anchor: &PlayoutAnchor, request: &SchedulingRequest) -> ClassicState {
    ClassicState {
        current_time: anchor.next_start.max(request.start),
        schedule_state: anchor.schedule_items_enumerator_state,
        multiple_remaining: anchor.multiple_remaining,
        duration_finish: anchor.duration_finish,
        in_flood: anchor.in_flood,
        in_duration_filler: anchor.in_duration_filler,
        next_guide_group: anchor.next_guide_group.max(1),
    }
}

fn retained_collection_anchors(request: &SchedulingRequest) -> Vec<PlayoutProgramScheduleAnchor> {
    match request.mode {
        PlayoutBuildMode::Continue => retained_continue_anchors(request),
        PlayoutBuildMode::Refresh => retained_refresh_anchors(request),
        PlayoutBuildMode::Reset => Vec::new(),
    }
}

fn retained_continue_anchors(request: &SchedulingRequest) -> Vec<PlayoutProgramScheduleAnchor> {
    let start_day = midnight(request.start);
    request
        .reference_data
        .existing_program_schedule_anchors
        .iter()
        .filter(|anchor| {
            anchor
                .anchor_date
                .is_none_or(|anchor_date| anchor_date >= start_day)
        })
        .cloned()
        .collect()
}

fn retained_refresh_anchors(request: &SchedulingRequest) -> Vec<PlayoutProgramScheduleAnchor> {
    let start_day = midnight(request.start);
    let next_day = start_day + Duration::days(1);
    let mut by_key = BTreeMap::<String, PlayoutProgramScheduleAnchor>::new();

    for anchor in &request.reference_data.existing_program_schedule_anchors {
        let Some(anchor_date) = anchor.anchor_date else {
            continue;
        };
        if anchor_date < start_day || anchor_date >= next_day {
            continue;
        }

        let key = history_key_for_collection_key(&collection_key_for_anchor(anchor));
        match by_key.get(&key).and_then(|existing| existing.anchor_date) {
            Some(existing_date) if existing_date <= anchor_date => {}
            _ => {
                by_key.insert(key, anchor.clone());
            }
        }
    }

    by_key.into_values().collect()
}

fn should_prefer_enumerator_anchor(
    candidate: &PlayoutProgramScheduleAnchor,
    existing: &PlayoutProgramScheduleAnchor,
) -> bool {
    match (candidate.anchor_date, existing.anchor_date) {
        (None, Some(_)) => true,
        (Some(_) | None, None) => false,
        (Some(candidate_date), Some(existing_date)) => candidate_date > existing_date,
    }
}

fn get_start_time_after(
    current_time: DateTime<Utc>,
    schedule_item: &ProgramScheduleItem,
    schedule_fixed_start_time_behavior: FixedStartTimeBehavior,
    state: &ClassicState,
    is_peek: bool,
) -> DateTime<Utc> {
    let mut start_time = current_time;
    let is_incomplete = !is_peek
        && ((schedule_item.playout_mode == ersatzrs_domain::PlayoutMode::Multiple
            && state.multiple_remaining.is_some())
            || (schedule_item.playout_mode == ersatzrs_domain::PlayoutMode::Duration
                && state.duration_finish.is_some())
            || (schedule_item.playout_mode == ersatzrs_domain::PlayoutMode::Flood
                && state.in_flood)
            || (schedule_item.playout_mode == ersatzrs_domain::PlayoutMode::Duration
                && state.in_duration_filler));

    if schedule_item.start_type == StartType::Fixed && !is_incomplete {
        let item_start_time = schedule_item.start_time.unwrap_or_else(Duration::zero);
        let result = midnight(start_time) + item_start_time;
        let behavior = schedule_item
            .fixed_start_time_behavior
            .unwrap_or(schedule_fixed_start_time_behavior);
        match behavior {
            FixedStartTimeBehavior::Flexible => {
                if is_peek && state.in_flood && start_time > result {
                    start_time = result + Duration::days(1);
                } else if result.date_naive() == start_time.date_naive()
                    && result.time() > start_time.time()
                {
                    start_time = result;
                }
            }
            FixedStartTimeBehavior::Strict => {
                start_time = if seconds_since_midnight(start_time) > item_start_time {
                    result + Duration::days(1)
                } else {
                    result
                };
            }
        }
    }

    start_time
}

fn midnight(value: DateTime<Utc>) -> DateTime<Utc> {
    value
        - Duration::hours(i64::from(value.hour()))
        - Duration::minutes(i64::from(value.minute()))
        - Duration::seconds(i64::from(value.second()))
        - Duration::nanoseconds(i64::from(value.nanosecond()))
}

fn seconds_since_midnight(value: DateTime<Utc>) -> Duration {
    Duration::seconds(i64::from(value.time().num_seconds_from_midnight()))
}

fn multiple_count(schedule_item: &ProgramScheduleItem, enumerator_len: usize, seed: u64) -> i32 {
    let evaluated = schedule_item.multiple_count.as_deref().map_or(0, |value| {
        yaml::eval_count_expression(value, enumerator_len, seed)
    });
    let fallback = match schedule_item.multiple_mode {
        MultipleMode::CollectionSize
        | MultipleMode::PlaylistItemSize
        | MultipleMode::MultiEpisodeGroupSize => enumerator_len,
        MultipleMode::Count => 1,
    };
    i32::try_from(if evaluated == 0 { fallback } else { evaluated })
        .unwrap_or(i32::MAX)
        .max(1)
}

fn collection_key(item: &ProgramScheduleItem) -> SchedulingCollectionKey {
    SchedulingCollectionKey {
        collection_type: item.collection_type,
        collection_id: item.collection_id,
        multi_collection_id: item.multi_collection_id,
        smart_collection_id: item.smart_collection_id,
        media_item_id: item.media_item_id,
        playlist_id: item.playlist_id,
        search_query: item.search_query.clone(),
        fake_collection_key: None,
    }
}

fn collection_key_for_anchor(anchor: &PlayoutProgramScheduleAnchor) -> SchedulingCollectionKey {
    SchedulingCollectionKey {
        collection_type: anchor.collection_type,
        collection_id: anchor.collection_id,
        multi_collection_id: anchor.multi_collection_id,
        smart_collection_id: anchor.smart_collection_id,
        media_item_id: anchor.media_item_id,
        playlist_id: anchor.playlist_id,
        search_query: anchor.search_query.clone(),
        fake_collection_key: anchor.fake_collection_key.clone(),
    }
}

fn item_key(item: &ProgramScheduleItem) -> String {
    history_key_for_collection_key(&collection_key(item))
}

fn anchor_matches_key(
    anchor: &PlayoutProgramScheduleAnchor,
    key: &SchedulingCollectionKey,
) -> bool {
    anchor.collection_type == key.collection_type
        && anchor.collection_id == key.collection_id
        && anchor.multi_collection_id == key.multi_collection_id
        && anchor.smart_collection_id == key.smart_collection_id
        && anchor.media_item_id == key.media_item_id
        && anchor.playlist_id == key.playlist_id
        && anchor.search_query == key.search_query
        && anchor.fake_collection_key == key.fake_collection_key
}

fn filler_id_from_context(context: &str) -> Option<EntityId> {
    serde_json::from_str::<serde_json::Value>(context)
        .ok()
        .and_then(|context| {
            context
                .get("FillerPresetId")
                .and_then(serde_json::Value::as_i64)
        })
}

fn index_from_context(context: &str) -> Option<usize> {
    serde_json::from_str::<serde_json::Value>(context)
        .ok()
        .and_then(|context| context.get("Index").and_then(serde_json::Value::as_i64))
        .and_then(|index| usize::try_from(index).ok())
}

fn prepare_enumerator_items(
    enumerator: &mut MediaEnumerator,
    playback_order: PlaybackOrder,
    seed: u64,
    previous_group: Option<ShuffleGroupKey>,
) {
    match playback_order {
        PlaybackOrder::Chronological => enumerator.items.sort_by(|left, right| {
            left.release_date
                .cmp(&right.release_date)
                .then(left.season.cmp(&right.season))
                .then(left.episode.cmp(&right.episode))
                .then(left.id.cmp(&right.id))
        }),
        PlaybackOrder::SeasonEpisode => enumerator.items.sort_by(|left, right| {
            left.season
                .cmp(&right.season)
                .then(left.episode.cmp(&right.episode))
                .then(left.release_date.cmp(&right.release_date))
                .then(left.id.cmp(&right.id))
        }),
        PlaybackOrder::Shuffle => shuffle_with_seed(&mut enumerator.items, seed),
        PlaybackOrder::ShuffleInOrder | PlaybackOrder::MultiEpisodeShuffle => {
            shuffle_with_group_distance(&mut enumerator.items, seed);
        }
        PlaybackOrder::RandomRotation => {
            shuffle_with_random_rotation(&mut enumerator.items, seed, previous_group);
        }
        PlaybackOrder::Marathon => {
            shuffle_with_seed(&mut enumerator.items, seed);
        }
        PlaybackOrder::Random | PlaybackOrder::None => {}
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ShuffleGroupKey {
    Show(EntityId),
    Artist(EntityId),
    Item(EntityId),
}

/// Group items by show first (TV content), then artist (music videos), then
/// fall back to a singleton group per item. Used by the grouped/balanced
/// shuffle algorithms shared by `ShuffleInOrder`, `MultiEpisodeShuffle`, and
/// `RandomRotation` so TV collections get the same even-spacing benefit music
/// videos already had from artist grouping.
fn shuffle_group_key(item: &SchedulingMediaItem) -> ShuffleGroupKey {
    if let Some(show_id) = item.show_id {
        ShuffleGroupKey::Show(show_id)
    } else if let Some(artist_id) = item.artist_id {
        ShuffleGroupKey::Artist(artist_id)
    } else {
        ShuffleGroupKey::Item(item.id)
    }
}

fn has_repeated_group(items: &[SchedulingMediaItem]) -> bool {
    let mut seen = BTreeSet::new();
    items
        .iter()
        .map(shuffle_group_key)
        .any(|key| !seen.insert(key))
}

type RandomRotationItemKey<'a> = (
    EntityId,
    Option<&'a str>,
    Option<&'a str>,
    Option<&'a str>,
    Option<u8>,
);

/// Stable identity for ADR 0048 scheduling rows. The same media item may
/// appear more than once when playback overrides differ, so media ID alone
/// cannot deterministically reconstruct a persisted Random Rotation cycle.
fn random_rotation_item_key(item: &SchedulingMediaItem) -> RandomRotationItemKey<'_> {
    (
        item.id,
        item.preferred_audio_language_code.as_deref(),
        item.preferred_audio_title.as_deref(),
        item.preferred_subtitle_language_code.as_deref(),
        item.subtitle_mode.map(|mode| match mode {
            ChannelSubtitleMode::None => 0,
            ChannelSubtitleMode::Forced => 1,
            ChannelSubtitleMode::Default => 2,
            ChannelSubtitleMode::Any => 3,
        }),
    )
}

fn group_items_by_key(
    items: Vec<SchedulingMediaItem>,
) -> Vec<(ShuffleGroupKey, VecDeque<SchedulingMediaItem>)> {
    let mut groups: Vec<(ShuffleGroupKey, VecDeque<SchedulingMediaItem>)> = Vec::new();
    for item in items {
        let key = shuffle_group_key(&item);
        if let Some((_, group_items)) = groups.iter_mut().find(|(group_key, _)| *group_key == key) {
            group_items.push_back(item);
        } else {
            groups.push((key, VecDeque::from([item])));
        }
    }
    groups
}

/// `ShuffleInOrder`/`MultiEpisodeShuffle`'s balanced-shuffle analog: a plain
/// Fisher-Yates pass for tie-breaking, followed by a greedy interleave that
/// always advances whichever group (show or artist) still has the most
/// remaining items, skipping the group that was just used. This spreads a
/// low-cardinality show/artist evenly across a mixed collection rather than
/// letting a flat shuffle clump it, mirroring the spirit of ErsatzTV's
/// `ShuffleInOrderCollectionEnumerator` balanced-shuffle interleaving.
fn shuffle_with_group_distance(items: &mut Vec<SchedulingMediaItem>, seed: u64) {
    shuffle_with_seed(items, seed);

    if !has_repeated_group(items) {
        return;
    }

    let mut groups = group_items_by_key(std::mem::take(items));
    let total_len = groups.iter().map(|(_, group)| group.len()).sum();
    let mut arranged = Vec::with_capacity(total_len);
    let mut previous_key = None;
    while arranged.len() < total_len {
        let candidate = group_distance_candidate(&groups, previous_key);
        let Some(index) = candidate else {
            break;
        };
        let key = groups[index].0;
        if let Some(item) = groups[index].1.pop_front() {
            arranged.push(item);
            previous_key = Some(key);
        }
    }

    *items = arranged;
}

/// `RandomRotation`'s algorithm: bucket items by group (show/artist), then
/// materialize the cycle with a seeded tie order and a max-heap that advances
/// the largest remaining group, preferring a different group than the
/// previous pick when more than one still has items. This is a
/// materialize-then-walk adaptation of ErsatzTV's live
/// `RandomizedRotatingMediaCollectionEnumerator` "bump to next group on
/// repeat" behavior — ErsatzRS's `MediaEnumerator` materializes a full order
/// per cycle rather than tracking per-draw group state the way the upstream
/// live enumerator does.
fn shuffle_with_random_rotation(
    items: &mut Vec<SchedulingMediaItem>,
    seed: u64,
    previous_key: Option<ShuffleGroupKey>,
) {
    if items.len() <= 1 {
        shuffle_with_seed(items, seed);
        return;
    }

    // Canonicalize the source before every cycle so persisted seed plus
    // predecessor reconstructs the exact same materialized order, including
    // duplicate media rows carrying distinct playback overrides.
    items.sort_by(|left, right| {
        random_rotation_item_key(left).cmp(&random_rotation_item_key(right))
    });
    let has_repeated_group = has_repeated_group(items);
    shuffle_with_seed(items, seed);

    // Movies and other ungrouped media are singleton groups. After the shared
    // canonical sort and shuffle, keep this common path constant-time and only
    // move the first item when a persisted predecessor would otherwise repeat
    // across a cycle/build boundary.
    if !has_repeated_group {
        if previous_key.is_some_and(|key| {
            items
                .first()
                .is_some_and(|item| shuffle_group_key(item) == key)
        }) {
            items.swap(0, 1);
        }
        return;
    }

    // RandomRotation chooses both a group and an item within that group.
    // Bucketing through an ordered map avoids the quadratic Vec::find loop,
    // while the pre-shuffle above randomizes the queue within every group.
    let mut group_indices = BTreeMap::<ShuffleGroupKey, usize>::new();
    let mut groups = Vec::<(ShuffleGroupKey, VecDeque<SchedulingMediaItem>)>::new();
    for item in std::mem::take(items) {
        let key = shuffle_group_key(&item);
        if let Some(index) = group_indices.get(&key).copied() {
            groups[index].1.push_back(item);
        } else {
            group_indices.insert(key, groups.len());
            groups.push((key, VecDeque::from([item])));
        }
    }
    let total_len: usize = groups.iter().map(|(_, group)| group.len()).sum();
    let mut arranged = Vec::with_capacity(total_len);
    let mut active = groups
        .iter()
        .enumerate()
        .map(|(index, (_, items))| (items.len(), Reverse(index)))
        .collect::<BinaryHeap<_>>();
    let mut previous_key = previous_key;
    while let Some(mut candidate) = active.pop() {
        let mut group_index = candidate.1 .0;
        if Some(groups[group_index].0) == previous_key {
            if let Some(alternative) = active.pop() {
                active.push(candidate);
                candidate = alternative;
                group_index = candidate.1 .0;
            }
        }

        let key = groups[group_index].0;
        if let Some(item) = groups[group_index].1.pop_front() {
            arranged.push(item);
            previous_key = Some(key);
        }
        candidate.0 = candidate.0.saturating_sub(1);
        if candidate.0 > 0 {
            active.push(candidate);
        }
    }

    *items = arranged;
}

fn group_distance_candidate(
    groups: &[(ShuffleGroupKey, VecDeque<SchedulingMediaItem>)],
    previous_key: Option<ShuffleGroupKey>,
) -> Option<usize> {
    let mut candidate: Option<usize> = None;
    for (index, (key, group_items)) in groups.iter().enumerate() {
        if group_items.is_empty() || Some(*key) == previous_key {
            continue;
        }
        if candidate.is_none_or(|best| group_items.len() > groups[best].1.len()) {
            candidate = Some(index);
        }
    }

    candidate.or_else(|| {
        groups
            .iter()
            .position(|(_, group_items)| !group_items.is_empty())
    })
}

fn prepare_next_cycle(
    enumerator: &mut MediaEnumerator,
    playback_order: PlaybackOrder,
    mut seed: i32,
    tail_media_id: Option<EntityId>,
    tail_group_key: Option<ShuffleGroupKey>,
) -> i32 {
    if playback_order == PlaybackOrder::RandomRotation {
        shuffle_with_random_rotation(&mut enumerator.items, seed_to_u64(seed), tail_group_key);
        return seed;
    }

    prepare_enumerator_items(enumerator, playback_order, seed_to_u64(seed), None);

    if !matches!(
        playback_order,
        PlaybackOrder::Shuffle | PlaybackOrder::ShuffleInOrder | PlaybackOrder::MultiEpisodeShuffle
    ) || enumerator.len() <= 1
    {
        return seed;
    }

    while enumerator
        .current()
        .is_some_and(|item| Some(item.id) == tail_media_id)
    {
        seed = next_i32_seed(seed);
        prepare_enumerator_items(enumerator, playback_order, seed_to_u64(seed), None);
    }

    seed
}

const RANDOM_MULTIPLIER: u64 = 6_364_136_223_846_793_005;
const RANDOM_INCREMENT: u64 = 1_442_695_040_888_963_407;

fn next_random_state(state: u64) -> u64 {
    state
        .wrapping_mul(RANDOM_MULTIPLIER)
        .wrapping_add(RANDOM_INCREMENT)
}

fn random_index(state: u64, len: usize) -> usize {
    if len == 0 {
        0
    } else {
        (state >> 33) as usize % len
    }
}

/// Advance the scheduler LCG by `steps` in logarithmic time.
fn advance_random_state(mut state: u64, mut steps: u64) -> u64 {
    let mut accumulated_multiplier = 1_u64;
    let mut accumulated_increment = 0_u64;
    let mut current_multiplier = RANDOM_MULTIPLIER;
    let mut current_increment = RANDOM_INCREMENT;

    while steps > 0 {
        if steps & 1 == 1 {
            accumulated_multiplier = accumulated_multiplier.wrapping_mul(current_multiplier);
            accumulated_increment = accumulated_increment
                .wrapping_mul(current_multiplier)
                .wrapping_add(current_increment);
        }
        current_increment = current_multiplier
            .wrapping_add(1)
            .wrapping_mul(current_increment);
        current_multiplier = current_multiplier.wrapping_mul(current_multiplier);
        steps >>= 1;
    }

    state = accumulated_multiplier
        .wrapping_mul(state)
        .wrapping_add(accumulated_increment);
    state
}

fn random_state_for_draw(seed: u64, draw_index: usize) -> u64 {
    let steps = u64::try_from(draw_index)
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    advance_random_state(seed, steps)
}

fn select_random_draw(enumerator: &mut MediaEnumerator, seed: u64, draw_index: usize) -> u64 {
    let state = random_state_for_draw(seed, draw_index);
    enumerator.set_index(random_index(state, enumerator.len()));
    state
}

/// Bit-reinterpret an `i32` playout/enumerator seed as `u64` for use as LCG
/// state. Deliberately not `u64::try_from(seed).unwrap_or_default()`: that
/// silently collapses every negative seed to a constant `0`, destroying all
/// its entropy, whereas `as` sign-extends the value into a distinct,
/// deterministic `u64` for every possible `i32` input.
#[allow(clippy::cast_sign_loss)]
fn seed_to_u64(seed: i32) -> u64 {
    seed as u64
}

fn next_i32_seed(seed: i32) -> i32 {
    let next = next_random_state(seed_to_u64(seed));
    (next >> 33) as i32
}

trait DirectContentResolver {
    fn media_items_for_source(
        &self,
        source: &super::SchedulingContentSource,
    ) -> Vec<SchedulingMediaItem>;
}

impl DirectContentResolver for SchedulingReferenceData {
    fn media_items_for_source(
        &self,
        source: &super::SchedulingContentSource,
    ) -> Vec<SchedulingMediaItem> {
        super::SchedulingContentResolver::media_items_for_source(self, source)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chrono::{TimeZone, Utc};
    use ersatzrs_domain::{
        ChannelSubtitleMode, CollectionType, Playout, PlayoutMode, PlayoutScheduleKind,
    };
    use insta::assert_json_snapshot;

    use super::*;
    use crate::scheduling::{
        PlayoutBuildMode, SchedulingFillerPreset, SchedulingPlaylistItem, SchedulingReferenceData,
    };

    #[test]
    fn classic_anchor_context_v1_defaults_random_rotation_state() {
        let context: ClassicAnchorContext =
            serde_json::from_str(r#"{"version":1}"#).expect("version 1 context should deserialize");

        assert_eq!(context.version, 1);
        assert!(context.random_rotation_cycle_predecessors.is_empty());
    }

    #[test]
    fn classic_runtime_state_audit_requires_exact_unique_schedule_state() {
        let mut first = schedule_item(1, PlayoutMode::One, 99, None);
        first.fill_with_group_mode = FillWithGroupMode::FillWithOrderedGroups;
        let duplicate_source = schedule_item(2, PlayoutMode::One, 99, None);
        let mut reference_data = SchedulingReferenceData {
            classic_schedule_items: vec![first, duplicate_source],
            existing_program_schedule_anchors: vec![PlayoutProgramScheduleAnchor {
                id: 101,
                playout_id: 7,
                collection_type: CollectionType::Collection,
                collection_id: Some(99),
                ..PlayoutProgramScheduleAnchor::default()
            }],
            existing_fill_group_indices: vec![PlayoutScheduleItemFillGroupIndex {
                id: 201,
                playout_id: 7,
                program_schedule_item_id: 1,
                ..PlayoutScheduleItemFillGroupIndex::default()
            }],
            ..SchedulingReferenceData::default()
        };

        let audit = audit_classic_runtime_state(&reference_data);
        assert!(audit.is_complete());
        assert_eq!(audit.expected_anchor_count, 1);
        assert_eq!(audit.expected_fill_group_count, 1);

        reference_data
            .existing_program_schedule_anchors
            .push(reference_data.existing_program_schedule_anchors[0].clone());
        assert!(!audit_classic_runtime_state(&reference_data).is_complete());
    }

    #[test]
    fn classic_new_runtime_state_uses_database_identity_and_deduplicates_sources() {
        let start = Utc.with_ymd_and_hms(2026, 8, 2, 12, 0, 0).unwrap();
        let mut first = schedule_item(1, PlayoutMode::One, 99, None);
        first.fill_with_group_mode = FillWithGroupMode::FillWithOrderedGroups;
        let duplicate_source = schedule_item(2, PlayoutMode::One, 99, None);
        let key = item_key(&first);
        let mut reference_data = SchedulingReferenceData {
            classic_schedule_id: Some(10),
            classic_schedule_items: vec![first, duplicate_source],
            ..SchedulingReferenceData::default()
        };
        reference_data.media_by_key.insert(
            key,
            vec![SchedulingMediaItem::new(100, Duration::minutes(10))],
        );

        let result = ClassicSchedulingEngine
            .schedule(SchedulingRequest {
                playout: Playout {
                    id: 7,
                    schedule_kind: PlayoutScheduleKind::Classic,
                    seed: 42,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::minutes(20),
                mode: PlayoutBuildMode::Refresh,
                reference_data,
            })
            .expect("Classic schedule should build");

        assert_eq!(result.program_schedule_anchors.len(), 1);
        assert_eq!(result.program_schedule_anchors[0].id, 0);
        assert_eq!(result.fill_group_indices.len(), 1);
        assert_eq!(result.fill_group_indices[0].id, 0);
    }

    #[test]
    fn classic_one_and_multiple_schedule_items_snapshot() {
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap();
        let mut reference_data = SchedulingReferenceData {
            classic_schedule_id: Some(10),
            classic_schedule_items: vec![
                schedule_item(1, PlayoutMode::One, 1, None),
                schedule_item(2, PlayoutMode::Multiple, 2, Some("2")),
            ],
            ..SchedulingReferenceData::default()
        };
        reference_data.media_by_key.insert(
            item_key(&reference_data.classic_schedule_items[0]),
            vec![
                SchedulingMediaItem::new(101, Duration::minutes(20)),
                SchedulingMediaItem::new(102, Duration::minutes(25)),
            ],
        );
        reference_data.media_by_key.insert(
            item_key(&reference_data.classic_schedule_items[1]),
            vec![
                SchedulingMediaItem::new(201, Duration::minutes(10)),
                SchedulingMediaItem::new(202, Duration::minutes(10)),
                SchedulingMediaItem::new(203, Duration::minutes(10)),
            ],
        );

        let result = ClassicSchedulingEngine
            .schedule(SchedulingRequest {
                playout: Playout {
                    id: 50,
                    schedule_kind: PlayoutScheduleKind::Classic,
                    seed: 7,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::minutes(55),
                mode: PlayoutBuildMode::Continue,
                reference_data,
            })
            .expect("classic schedule should build");

        assert_json_snapshot!(snapshot(&result));
    }

    #[test]
    fn classic_multiple_then_one_repeats_three_to_one_sequence_on_refresh() {
        let start = Utc.with_ymd_and_hms(2026, 7, 19, 8, 0, 0).unwrap();
        let mut reference_data = SchedulingReferenceData {
            classic_schedule_id: Some(10),
            classic_schedule_items: vec![
                schedule_item(1, PlayoutMode::Multiple, 1, Some("3")),
                schedule_item(2, PlayoutMode::One, 2, None),
            ],
            ..SchedulingReferenceData::default()
        };
        reference_data.media_by_key.insert(
            item_key(&reference_data.classic_schedule_items[0]),
            vec![
                SchedulingMediaItem::new(101, Duration::minutes(1)),
                SchedulingMediaItem::new(102, Duration::minutes(1)),
                SchedulingMediaItem::new(103, Duration::minutes(1)),
                SchedulingMediaItem::new(104, Duration::minutes(1)),
            ],
        );
        reference_data.media_by_key.insert(
            item_key(&reference_data.classic_schedule_items[1]),
            vec![SchedulingMediaItem::new(201, Duration::minutes(1))],
        );

        let result = ClassicSchedulingEngine
            .schedule(SchedulingRequest {
                playout: Playout {
                    id: 50,
                    schedule_kind: PlayoutScheduleKind::Classic,
                    seed: 7,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::minutes(8),
                mode: PlayoutBuildMode::Refresh,
                reference_data,
            })
            .expect("classic schedule should build");

        assert!(result.clear_items);
        assert_eq!(
            result
                .added_items
                .iter()
                .map(|item| item.media_item_id)
                .collect::<Vec<_>>(),
            vec![101, 102, 103, 201, 104, 101, 102, 201]
        );
        assert_eq!(
            result
                .added_items
                .iter()
                .map(|item| {
                    item.scheduling_context
                        .as_deref()
                        .and_then(|context| serde_json::from_str::<serde_json::Value>(context).ok())
                        .and_then(|context| {
                            context.get("ItemId").and_then(serde_json::Value::as_i64)
                        })
                })
                .collect::<Vec<_>>(),
            vec![
                Some(1),
                Some(1),
                Some(1),
                Some(2),
                Some(1),
                Some(1),
                Some(1),
                Some(2)
            ]
        );
        assert!(result
            .added_items
            .iter()
            .all(|item| item.filler_kind == FillerKind::None));
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn classic_duplicate_sources_keep_item_policies_and_count_chronological_units() {
        use super::super::{
            SchedulingSmartShufflePolicy, SchedulingSmartShufflePreset, SchedulingSmartShuffleRule,
            SchedulingSmartShuffleRuleKind, SchedulingSmartShuffleScope,
        };

        let start = Utc.with_ymd_and_hms(2026, 7, 19, 8, 0, 0).unwrap();
        let mut active_item = schedule_item(1, PlayoutMode::One, 1, None);
        active_item.playback_order = PlaybackOrder::Shuffle;
        active_item.sort_order = 0;
        let mut shared_source_off = schedule_item(2, PlayoutMode::One, 1, None);
        shared_source_off.playback_order = PlaybackOrder::Shuffle;
        shared_source_off.sort_order = 1;
        let mut chronological_one = schedule_item(3, PlayoutMode::One, 2, None);
        chronological_one.sort_order = 2;
        let mut chronological_two = schedule_item(4, PlayoutMode::One, 3, None);
        chronological_two.sort_order = 3;
        let active_policy = SchedulingSmartShufflePolicy {
            enabled: true,
            preset: SchedulingSmartShufflePreset::Custom,
            after_items: Some(3),
            rules: vec![SchedulingSmartShuffleRule {
                id: "artist".to_owned(),
                kind: SchedulingSmartShuffleRuleKind::Artist,
                search: None,
            }],
            ..SchedulingSmartShufflePolicy::default()
        };
        let chronological_policy = SchedulingSmartShufflePolicy {
            scope: SchedulingSmartShuffleScope::ScheduleItem,
            ..active_policy.clone()
        };
        let mut reference_data = SchedulingReferenceData {
            classic_schedule_id: Some(10),
            classic_schedule_items: vec![
                active_item.clone(),
                shared_source_off.clone(),
                chronological_one.clone(),
                chronological_two.clone(),
            ],
            ..SchedulingReferenceData::default()
        };
        let shared_content_key = item_key(&active_item);
        assert_eq!(shared_content_key, item_key(&shared_source_off));
        reference_data.media_by_key.insert(
            shared_content_key,
            vec![
                SchedulingMediaItem::new(101, Duration::minutes(1)),
                SchedulingMediaItem::new(102, Duration::minutes(1)),
            ],
        );
        reference_data.media_by_key.insert(
            item_key(&chronological_one),
            vec![SchedulingMediaItem::new(201, Duration::minutes(1))],
        );
        reference_data.media_by_key.insert(
            item_key(&chronological_two),
            vec![SchedulingMediaItem::new(301, Duration::minutes(1))],
        );
        reference_data.smart_shuffle_policies_by_unit.insert(
            classic_schedule_item_policy_key(&active_item),
            active_policy.clone(),
        );
        reference_data.smart_shuffle_policies_by_unit.insert(
            classic_schedule_item_policy_key(&shared_source_off),
            SchedulingSmartShufflePolicy::default(),
        );
        for item in [&chronological_one, &chronological_two] {
            reference_data.smart_shuffle_policies_by_unit.insert(
                classic_schedule_item_policy_key(item),
                chronological_policy.clone(),
            );
            reference_data.smart_shuffle_tags_by_unit.insert(
                classic_schedule_item_policy_key(item),
                BTreeMap::from([(
                    if item.id == 3 { 201 } else { 301 },
                    BTreeSet::from([format!("artist:chronological-{}", item.id)]),
                )]),
            );
        }
        reference_data.smart_shuffle_tags_by_unit.insert(
            classic_schedule_item_policy_key(&active_item),
            BTreeMap::from([
                (101, BTreeSet::from(["artist:one".to_owned()])),
                (102, BTreeSet::from(["artist:two".to_owned()])),
            ]),
        );

        let result = ClassicSchedulingEngine
            .schedule(SchedulingRequest {
                playout: Playout {
                    id: 50,
                    schedule_kind: PlayoutScheduleKind::Classic,
                    seed: 7,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::minutes(5),
                mode: PlayoutBuildMode::Refresh,
                reference_data,
            })
            .expect("mixed-order Classic schedule should build");
        let media_ids = result
            .added_items
            .iter()
            .map(|item| item.media_item_id)
            .collect::<Vec<_>>();

        assert_eq!(media_ids.len(), 5);
        assert_eq!(media_ids[0], media_ids[4]);
        let anchor_context = result
            .anchor
            .and_then(|anchor| anchor.context)
            .expect("active Smart Shuffle persists anchor state");
        assert!(!anchor_context.contains("schedule_item:3"));
        assert!(!anchor_context.contains("schedule_item:4"));
    }

    #[test]
    fn classic_fixed_start_strict_wraps_to_next_day() {
        let current_time = Utc.with_ymd_and_hms(2026, 4, 26, 9, 0, 0).unwrap();
        let item = ProgramScheduleItem {
            start_type: StartType::Fixed,
            start_time: Some(Duration::hours(8)),
            ..schedule_item(1, PlayoutMode::One, 1, None)
        };
        let anchor = PlayoutAnchor::default();

        let next = ClassicSchedulingEngine::get_start_time_after(
            current_time,
            &item,
            FixedStartTimeBehavior::Strict,
            &anchor,
            false,
        );

        assert_eq!(next, Utc.with_ymd_and_hms(2026, 4, 27, 8, 0, 0).unwrap());
    }

    #[test]
    fn classic_dynamic_item_before_fixed_anchor_fills_pre_anchor_window() {
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 18, 0, 0).unwrap();
        let fixed_start = Utc.with_ymd_and_hms(2026, 4, 26, 22, 0, 0).unwrap();
        let mut reference_data = SchedulingReferenceData {
            classic_fixed_start_time_behavior: FixedStartTimeBehavior::Strict,
            classic_schedule_items: vec![
                schedule_item(1, PlayoutMode::Flood, 1, None),
                ProgramScheduleItem {
                    start_type: StartType::Fixed,
                    start_time: Some(Duration::hours(22)),
                    ..schedule_item(2, PlayoutMode::One, 2, None)
                },
            ],
            ..SchedulingReferenceData::default()
        };
        reference_data.media_by_key.insert(
            item_key(&reference_data.classic_schedule_items[0]),
            vec![
                SchedulingMediaItem::new(101, Duration::minutes(10)),
                SchedulingMediaItem::new(102, Duration::minutes(10)),
                SchedulingMediaItem::new(103, Duration::minutes(10)),
            ],
        );
        reference_data.media_by_key.insert(
            item_key(&reference_data.classic_schedule_items[1]),
            vec![SchedulingMediaItem::new(201, Duration::minutes(10))],
        );

        let result = ClassicSchedulingEngine
            .schedule(SchedulingRequest {
                playout: Playout {
                    id: 50,
                    schedule_kind: PlayoutScheduleKind::Classic,
                    seed: 7,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::minutes(30),
                mode: PlayoutBuildMode::Continue,
                reference_data,
            })
            .expect("classic schedule should build dynamic pre-anchor rows");

        assert_eq!(
            result
                .added_items
                .iter()
                .map(|item| (item.media_item_id, item.start, item.finish))
                .collect::<Vec<_>>(),
            vec![
                (101, start, start + Duration::minutes(10)),
                (
                    102,
                    start + Duration::minutes(10),
                    start + Duration::minutes(20),
                ),
                (
                    103,
                    start + Duration::minutes(20),
                    start + Duration::minutes(30),
                ),
            ]
        );
        assert!(result
            .added_items
            .iter()
            .all(|item| item.start < fixed_start));
    }

    #[test]
    fn classic_flood_expands_zero_duration_remote_stream_to_hard_stop() {
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 18, 0, 0).unwrap();
        let finish = start + Duration::minutes(30);
        let item = schedule_item(1, PlayoutMode::Flood, 1, None);
        let key = item_key(&item);
        let mut reference_data = SchedulingReferenceData {
            classic_schedule_items: vec![item],
            ..SchedulingReferenceData::default()
        };
        reference_data
            .media_by_key
            .insert(key, vec![SchedulingMediaItem::new(101, Duration::zero())]);

        let result = ClassicSchedulingEngine
            .schedule(SchedulingRequest {
                playout: Playout {
                    id: 50,
                    schedule_kind: PlayoutScheduleKind::Classic,
                    seed: 7,
                    ..Playout::default()
                },
                start,
                finish,
                mode: PlayoutBuildMode::Continue,
                reference_data,
            })
            .expect("zero-duration remote stream flood should build");

        let scheduled = result.added_items.first().expect("scheduled item");
        assert_eq!(scheduled.start, start);
        assert_eq!(scheduled.finish, finish);
        assert_eq!(scheduled.out_point, Duration::minutes(30));
    }

    #[test]
    fn classic_duration_does_not_overrun_duration_window() {
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap();
        let duration_item = ProgramScheduleItem {
            duration: Some(Duration::hours(1)),
            ..schedule_item(1, PlayoutMode::Duration, 1, None)
        };
        let mut reference_data = SchedulingReferenceData {
            classic_schedule_items: vec![duration_item],
            ..SchedulingReferenceData::default()
        };
        reference_data.media_by_key.insert(
            item_key(&reference_data.classic_schedule_items[0]),
            vec![SchedulingMediaItem::new(101, Duration::minutes(45))],
        );

        let result = ClassicSchedulingEngine
            .schedule(SchedulingRequest {
                playout: Playout {
                    id: 50,
                    schedule_kind: PlayoutScheduleKind::Classic,
                    seed: 7,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::minutes(50),
                mode: PlayoutBuildMode::Continue,
                reference_data,
            })
            .expect("classic duration schedule should build");

        assert_eq!(result.added_items.len(), 1);
        assert_eq!(
            result.anchor.and_then(|anchor| anchor.duration_finish),
            None
        );
    }

    #[test]
    fn classic_duration_post_roll_advances_before_next_primary() {
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap();
        let duration_item = ProgramScheduleItem {
            duration: Some(Duration::minutes(30)),
            post_roll_filler_id: Some(92),
            ..schedule_item(1, PlayoutMode::Duration, 1, None)
        };
        let mut reference_data = SchedulingReferenceData {
            classic_schedule_items: vec![duration_item],
            classic_filler_presets: BTreeMap::from([(
                92,
                filler_preset(92, FillerKind::PostRoll, FillerMode::Count, Some(1), None),
            )]),
            ..SchedulingReferenceData::default()
        };
        reference_data.media_by_key.insert(
            item_key(&reference_data.classic_schedule_items[0]),
            vec![
                SchedulingMediaItem::new(101, Duration::minutes(10)),
                SchedulingMediaItem::new(102, Duration::minutes(10)),
                SchedulingMediaItem::new(103, Duration::minutes(10)),
            ],
        );
        reference_data
            .classic_filler_presets
            .get_mut(&92)
            .expect("post-roll preset")
            .media_items
            .push(SchedulingMediaItem::new(892, Duration::minutes(5)));

        let result = ClassicSchedulingEngine
            .schedule(SchedulingRequest {
                playout: Playout {
                    id: 50,
                    schedule_kind: PlayoutScheduleKind::Classic,
                    seed: 7,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::minutes(30),
                mode: PlayoutBuildMode::Continue,
                reference_data,
            })
            .expect("classic duration schedule should build");

        assert_eq!(
            result
                .added_items
                .iter()
                .map(|item| (
                    item.media_item_id,
                    item.start,
                    item.finish,
                    item.filler_kind
                ))
                .collect::<Vec<_>>(),
            vec![
                (101, start, start + Duration::minutes(10), FillerKind::None,),
                (
                    892,
                    start + Duration::minutes(10),
                    start + Duration::minutes(15),
                    FillerKind::PostRoll,
                ),
                (
                    102,
                    start + Duration::minutes(15),
                    start + Duration::minutes(25),
                    FillerKind::None,
                ),
                (
                    892,
                    start + Duration::minutes(25),
                    start + Duration::minutes(30),
                    FillerKind::PostRoll,
                ),
            ]
        );
    }

    #[test]
    fn classic_playlist_schedule_uses_playlist_children() {
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap();
        let schedule_item = ProgramScheduleItem {
            collection_type: CollectionType::Playlist,
            collection_id: None,
            playlist_id: Some(99),
            playout_mode: PlayoutMode::Flood,
            ..schedule_item(1, PlayoutMode::Flood, 1, None)
        };
        let key = item_key(&schedule_item);
        let reference_data = SchedulingReferenceData {
            classic_schedule_items: vec![schedule_item],
            playlist_items_by_key: BTreeMap::from([(
                key,
                vec![
                    SchedulingPlaylistItem {
                        collection_key: SchedulingCollectionKey::fake_playlist_item(1),
                        playback_order: PlaybackOrder::Chronological,
                        play_all: false,
                        count: Some(2),
                        media_items: vec![
                            SchedulingMediaItem::new(101, Duration::minutes(10)),
                            SchedulingMediaItem::new(102, Duration::minutes(10)),
                        ],
                    },
                    SchedulingPlaylistItem {
                        collection_key: SchedulingCollectionKey::fake_playlist_item(2),
                        playback_order: PlaybackOrder::Chronological,
                        play_all: true,
                        count: None,
                        media_items: vec![
                            SchedulingMediaItem::new(201, Duration::minutes(10)),
                            SchedulingMediaItem::new(202, Duration::minutes(10)),
                        ],
                    },
                ],
            )]),
            ..SchedulingReferenceData::default()
        };

        let result = ClassicSchedulingEngine
            .schedule(SchedulingRequest {
                playout: Playout {
                    id: 50,
                    schedule_kind: PlayoutScheduleKind::Classic,
                    seed: 7,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::minutes(40),
                mode: PlayoutBuildMode::Reset,
                reference_data,
            })
            .expect("classic playlist preview should build");

        assert_eq!(
            result
                .added_items
                .iter()
                .map(|item| item.media_item_id)
                .collect::<Vec<_>>(),
            vec![101, 102, 201, 202]
        );
    }

    #[test]
    fn classic_reuses_existing_anchor_seed_and_omits_collection_key() {
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap();
        let schedule_item = ProgramScheduleItem {
            playback_order: PlaybackOrder::Shuffle,
            ..schedule_item(1, PlayoutMode::One, 1, None)
        };
        let key = item_key(&schedule_item);
        let mut reference_data = SchedulingReferenceData {
            classic_schedule_items: vec![schedule_item],
            existing_program_schedule_anchors: vec![PlayoutProgramScheduleAnchor {
                id: 900,
                playout_id: 50,
                collection_type: CollectionType::Collection,
                collection_id: Some(1),
                enumerator_state: CollectionEnumeratorState { seed: 42, index: 1 },
                ..PlayoutProgramScheduleAnchor::default()
            }],
            ..SchedulingReferenceData::default()
        };
        reference_data.media_by_key.insert(
            key,
            vec![
                SchedulingMediaItem::new(101, Duration::minutes(10)),
                SchedulingMediaItem::new(102, Duration::minutes(10)),
            ],
        );

        let result = ClassicSchedulingEngine
            .schedule(SchedulingRequest {
                playout: Playout {
                    id: 50,
                    schedule_kind: PlayoutScheduleKind::Classic,
                    seed: 7,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::minutes(10),
                mode: PlayoutBuildMode::Continue,
                reference_data,
            })
            .expect("classic schedule should build");

        let item = result.added_items.first().expect("scheduled item");
        assert!(item.collection_key.is_none());
        assert!(item
            .scheduling_context
            .as_deref()
            .is_some_and(|context| context.contains("\"Seed\":42")));
        assert_eq!(
            result
                .program_schedule_anchors
                .first()
                .map(|anchor| anchor.enumerator_state.seed),
            Some(next_i32_seed(42))
        );
    }

    #[test]
    fn classic_refresh_discards_undated_collection_anchor() {
        // An undated anchor is the Continue position at the end of the old
        // generated window. Refresh clears that window, so applying this
        // state at `start` would skip media that should be rebuilt.
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap();
        let schedule_item = ProgramScheduleItem {
            playback_order: PlaybackOrder::Shuffle,
            ..schedule_item(1, PlayoutMode::One, 1, None)
        };
        let key = item_key(&schedule_item);
        let mut reference_data = SchedulingReferenceData {
            classic_schedule_items: vec![schedule_item],
            existing_program_schedule_anchors: vec![PlayoutProgramScheduleAnchor {
                id: 900,
                playout_id: 50,
                collection_type: CollectionType::Collection,
                collection_id: Some(1),
                enumerator_state: CollectionEnumeratorState { seed: 42, index: 1 },
                ..PlayoutProgramScheduleAnchor::default()
            }],
            ..SchedulingReferenceData::default()
        };
        reference_data.media_by_key.insert(
            key,
            vec![
                SchedulingMediaItem::new(101, Duration::minutes(10)),
                SchedulingMediaItem::new(102, Duration::minutes(10)),
            ],
        );

        let result = ClassicSchedulingEngine
            .schedule(SchedulingRequest {
                playout: Playout {
                    id: 50,
                    schedule_kind: PlayoutScheduleKind::Classic,
                    seed: 7,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::minutes(10),
                mode: PlayoutBuildMode::Refresh,
                reference_data,
            })
            .expect("classic refresh should build");

        let item = result.added_items.first().expect("scheduled item");
        assert!(item
            .scheduling_context
            .as_deref()
            .is_some_and(|context| context.contains("\"Seed\":7")));
        assert_eq!(
            result
                .program_schedule_anchors
                .first()
                .map(|anchor| anchor.enumerator_state.seed),
            Some(7)
        );
    }

    #[test]
    fn classic_refresh_discards_end_of_horizon_schedule_item_state() {
        // The playout anchor points at the end of the old generated window.
        // Refresh clears that window and rebuilds from `start`, so carrying
        // index 1 back to `start` would incorrectly skip the first item.
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap();
        let item_a = schedule_item(1, PlayoutMode::One, 1, None);
        let item_b = schedule_item(2, PlayoutMode::One, 2, None);
        let key_a = item_key(&item_a);
        let key_b = item_key(&item_b);
        let mut reference_data = SchedulingReferenceData {
            classic_schedule_items: vec![item_a, item_b],
            ..SchedulingReferenceData::default()
        };
        reference_data.media_by_key.insert(
            key_a,
            vec![SchedulingMediaItem::new(101, Duration::minutes(10))],
        );
        reference_data.media_by_key.insert(
            key_b,
            vec![SchedulingMediaItem::new(201, Duration::minutes(10))],
        );

        let result = ClassicSchedulingEngine
            .schedule(SchedulingRequest {
                playout: Playout {
                    id: 50,
                    schedule_kind: PlayoutScheduleKind::Classic,
                    seed: 7,
                    anchor: Some(PlayoutAnchor {
                        schedule_items_enumerator_state: CollectionEnumeratorState {
                            seed: 7,
                            index: 1,
                        },
                        next_start: start + Duration::days(2),
                        ..PlayoutAnchor::default()
                    }),
                    ..Playout::default()
                },
                start,
                finish: start + Duration::minutes(10),
                mode: PlayoutBuildMode::Refresh,
                reference_data,
            })
            .expect("classic refresh should rebuild schedule-item order");

        let item = result.added_items.first().expect("scheduled item");
        assert_eq!(
            item.media_item_id, 101,
            "refresh must rebuild from a time-aligned schedule-item state"
        );
    }

    #[test]
    fn classic_shuffle_schedule_items_avoids_immediate_repeat_across_reshuffle_boundary() {
        // Mirrors classic_shuffle_avoids_immediate_repeat_across_reshuffle_boundary,
        // but for the "Shuffle Schedule Items" checkbox (schedule_order) rather
        // than a single collection's media order. Before the anti-repeat guard,
        // advance_schedule_item's reshuffle on cycle wraparound could pick a
        // schedule_order whose first entry was the same schedule item that had
        // just finished playing.
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap();
        let item_a = schedule_item(1, PlayoutMode::One, 1, None);
        let item_b = schedule_item(2, PlayoutMode::One, 2, None);
        let key_a = item_key(&item_a);
        let key_b = item_key(&item_b);
        let mut reference_data = SchedulingReferenceData {
            classic_schedule_items: vec![item_a, item_b],
            classic_shuffle_schedule_items: true,
            ..SchedulingReferenceData::default()
        };
        reference_data.media_by_key.insert(
            key_a,
            vec![SchedulingMediaItem::new(101, Duration::minutes(10))],
        );
        reference_data.media_by_key.insert(
            key_b,
            vec![SchedulingMediaItem::new(201, Duration::minutes(10))],
        );

        let result = ClassicSchedulingEngine
            .schedule(SchedulingRequest {
                playout: Playout {
                    id: 50,
                    schedule_kind: PlayoutScheduleKind::Classic,
                    seed: 1,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::minutes(80),
                mode: PlayoutBuildMode::Continue,
                reference_data,
            })
            .expect("classic schedule should build");

        let ids = result
            .added_items
            .iter()
            .map(|item| item.media_item_id)
            .collect::<Vec<_>>();
        assert_eq!(ids.len(), 8);
        assert!(
            ids.windows(2).all(|window| window[0] != window[1]),
            "shuffled schedule item repeated across reshuffle boundary: {ids:?}"
        );
    }

    #[test]
    fn classic_refresh_clears_and_rebuilds_from_checkpoint_anchor() {
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap();
        let schedule_item = ProgramScheduleItem {
            playback_order: PlaybackOrder::Shuffle,
            subtitle_mode: Some(ChannelSubtitleMode::Any),
            ..schedule_item(1, PlayoutMode::One, 1, None)
        };
        let key = item_key(&schedule_item);
        let media_items = vec![
            SchedulingMediaItem::new(101, Duration::minutes(10)),
            SchedulingMediaItem::new(102, Duration::minutes(10)),
            SchedulingMediaItem::new(103, Duration::minutes(10)),
            SchedulingMediaItem::new(104, Duration::minutes(10)),
        ];
        let mut expected = MediaEnumerator::new(key.clone(), media_items.clone())
            .with_playback_order(PlaybackOrder::Shuffle);
        prepare_enumerator_items(&mut expected, PlaybackOrder::Shuffle, 42, None);
        expected.set_index(1);
        let expected_media_ids = (0..3)
            .filter_map(|_| {
                let id = expected.current().map(|item| item.id);
                expected.advance();
                id
            })
            .collect::<Vec<_>>();

        let mut reference_data = SchedulingReferenceData {
            classic_schedule_items: vec![schedule_item],
            existing_program_schedule_anchors: vec![
                PlayoutProgramScheduleAnchor {
                    id: 900,
                    playout_id: 50,
                    collection_type: CollectionType::Collection,
                    collection_id: Some(1),
                    anchor_date: None,
                    enumerator_state: CollectionEnumeratorState { seed: 99, index: 0 },
                    ..PlayoutProgramScheduleAnchor::default()
                },
                PlayoutProgramScheduleAnchor {
                    id: 901,
                    playout_id: 50,
                    collection_type: CollectionType::Collection,
                    collection_id: Some(1),
                    anchor_date: Some(start),
                    enumerator_state: CollectionEnumeratorState { seed: 42, index: 1 },
                    ..PlayoutProgramScheduleAnchor::default()
                },
                PlayoutProgramScheduleAnchor {
                    id: 902,
                    playout_id: 50,
                    collection_type: CollectionType::Collection,
                    collection_id: Some(1),
                    anchor_date: Some(start + Duration::days(1)),
                    enumerator_state: CollectionEnumeratorState { seed: 7, index: 2 },
                    ..PlayoutProgramScheduleAnchor::default()
                },
            ],
            ..SchedulingReferenceData::default()
        };
        reference_data.media_by_key.insert(key, media_items);

        let result = ClassicSchedulingEngine
            .schedule(SchedulingRequest {
                playout: Playout {
                    id: 50,
                    schedule_kind: PlayoutScheduleKind::Classic,
                    seed: 7,
                    anchor: Some(PlayoutAnchor {
                        next_start: start + Duration::days(1),
                        ..PlayoutAnchor::default()
                    }),
                    ..Playout::default()
                },
                start,
                finish: start + Duration::minutes(30),
                mode: PlayoutBuildMode::Refresh,
                reference_data,
            })
            .expect("classic refresh should rebuild from checkpoint");

        assert!(result.clear_items);
        assert_eq!(
            result
                .added_items
                .iter()
                .map(|item| item.media_item_id)
                .collect::<Vec<_>>(),
            expected_media_ids
        );
        assert!(result
            .added_items
            .iter()
            .all(|item| item.subtitle_mode == Some(ChannelSubtitleMode::Any)));
        assert!(result
            .program_schedule_anchors
            .iter()
            .any(|anchor| anchor.id == 901 && anchor.anchor_date == Some(start)));
        assert!(!result
            .program_schedule_anchors
            .iter()
            .any(|anchor| anchor.id == 900 || anchor.id == 902));
    }

    #[test]
    fn classic_shuffle_avoids_immediate_repeat_across_reshuffle_boundary() {
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap();
        let schedule_item = ProgramScheduleItem {
            playback_order: PlaybackOrder::Shuffle,
            ..schedule_item(1, PlayoutMode::One, 1, None)
        };
        let key = item_key(&schedule_item);
        let mut reference_data = SchedulingReferenceData {
            classic_schedule_items: vec![schedule_item],
            ..SchedulingReferenceData::default()
        };
        reference_data.media_by_key.insert(
            key,
            vec![
                SchedulingMediaItem::new(101, Duration::minutes(10)),
                SchedulingMediaItem::new(102, Duration::minutes(10)),
            ],
        );

        let result = ClassicSchedulingEngine
            .schedule(SchedulingRequest {
                playout: Playout {
                    id: 50,
                    schedule_kind: PlayoutScheduleKind::Classic,
                    seed: 1,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::minutes(40),
                mode: PlayoutBuildMode::Continue,
                reference_data,
            })
            .expect("classic schedule should build");

        let ids = result
            .added_items
            .iter()
            .map(|item| item.media_item_id)
            .collect::<Vec<_>>();
        assert_eq!(ids.len(), 4);
        assert!(
            ids.windows(2).all(|window| window[0] != window[1]),
            "shuffled collection repeated across cycle boundary: {ids:?}"
        );
    }

    #[test]
    fn classic_negative_playout_seed_is_not_collapsed_to_zero() {
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap();
        let build_with_seed = |seed: i32| {
            let schedule_item = ProgramScheduleItem {
                playback_order: PlaybackOrder::Shuffle,
                ..schedule_item(1, PlayoutMode::One, 1, None)
            };
            let key = item_key(&schedule_item);
            let mut reference_data = SchedulingReferenceData {
                classic_schedule_items: vec![schedule_item],
                ..SchedulingReferenceData::default()
            };
            reference_data.media_by_key.insert(
                key,
                vec![
                    SchedulingMediaItem::new(101, Duration::minutes(10)),
                    SchedulingMediaItem::new(102, Duration::minutes(10)),
                    SchedulingMediaItem::new(103, Duration::minutes(10)),
                    SchedulingMediaItem::new(104, Duration::minutes(10)),
                ],
            );
            ClassicSchedulingEngine
                .schedule(SchedulingRequest {
                    playout: Playout {
                        id: 50,
                        schedule_kind: PlayoutScheduleKind::Classic,
                        seed,
                        ..Playout::default()
                    },
                    start,
                    finish: start + Duration::minutes(40),
                    mode: PlayoutBuildMode::Continue,
                    reference_data,
                })
                .expect("classic schedule should build")
                .added_items
                .iter()
                .map(|item| item.media_item_id)
                .collect::<Vec<_>>()
        };

        // Before the seed_to_u64 fix, `u64::try_from(seed).unwrap_or_default()`
        // silently collapsed every negative seed to 0, so a negative seed
        // produced the exact same shuffle as an explicit seed of 0.
        assert_ne!(
            build_with_seed(-1),
            build_with_seed(0),
            "a negative seed must not silently collapse to the same shuffle as seed 0"
        );
    }

    #[test]
    fn classic_shuffle_in_order_spreads_same_artist_music_videos() {
        // The artist/group-distance algorithm moved from `Shuffle` to
        // `ShuffleInOrder` (and `MultiEpisodeShuffle`) so `Shuffle` becomes a
        // genuinely distinct, ungrouped shuffle. See
        // classic_shuffle_in_order_and_shuffle_produce_different_orders for
        // proof the two orders now diverge.
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap();
        let schedule_item = ProgramScheduleItem {
            playback_order: PlaybackOrder::ShuffleInOrder,
            ..schedule_item(1, PlayoutMode::Multiple, 1, Some("6"))
        };
        let key = item_key(&schedule_item);
        let media_items = vec![
            music_video(101, 1),
            music_video(102, 1),
            music_video(103, 1),
            music_video(201, 2),
            music_video(301, 3),
            music_video(401, 4),
        ];
        let artists_by_id = media_items
            .iter()
            .map(|item| (item.id, item.artist_id))
            .collect::<BTreeMap<_, _>>();
        let mut reference_data = SchedulingReferenceData {
            classic_schedule_items: vec![schedule_item],
            ..SchedulingReferenceData::default()
        };
        reference_data.media_by_key.insert(key, media_items);

        let result = ClassicSchedulingEngine
            .schedule(SchedulingRequest {
                playout: Playout {
                    id: 50,
                    schedule_kind: PlayoutScheduleKind::Classic,
                    seed: 2,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::minutes(30),
                mode: PlayoutBuildMode::Continue,
                reference_data,
            })
            .expect("classic schedule should build");

        let artists = result
            .added_items
            .iter()
            .map(|item| artists_by_id.get(&item.media_item_id).copied().flatten())
            .collect::<Vec<_>>();
        assert_eq!(artists.len(), 6);
        assert!(
            artists.windows(2).all(|window| window[0] != window[1]),
            "shuffled collection placed same-artist videos too closely: {artists:?}"
        );
    }

    #[test]
    fn classic_shuffle_in_order_and_shuffle_produce_different_orders() {
        // Shuffle and ShuffleInOrder used to be byte-for-byte identical
        // (both ran the artist/group-distance algorithm). Shuffle is now a
        // plain flat shuffle while ShuffleInOrder keeps the group-distance
        // spread, so for the same seed/items they must diverge.
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap();
        let media_items = vec![
            music_video(101, 1),
            music_video(102, 1),
            music_video(103, 1),
            music_video(201, 2),
            music_video(301, 3),
            music_video(401, 4),
        ];
        let build_with_order = |playback_order: PlaybackOrder| {
            let schedule_item = ProgramScheduleItem {
                playback_order,
                ..schedule_item(1, PlayoutMode::Multiple, 1, Some("6"))
            };
            let key = item_key(&schedule_item);
            let mut reference_data = SchedulingReferenceData {
                classic_schedule_items: vec![schedule_item],
                ..SchedulingReferenceData::default()
            };
            reference_data.media_by_key.insert(key, media_items.clone());

            ClassicSchedulingEngine
                .schedule(SchedulingRequest {
                    playout: Playout {
                        id: 50,
                        schedule_kind: PlayoutScheduleKind::Classic,
                        seed: 2,
                        ..Playout::default()
                    },
                    start,
                    finish: start + Duration::minutes(30),
                    mode: PlayoutBuildMode::Continue,
                    reference_data,
                })
                .expect("classic schedule should build")
                .added_items
                .iter()
                .map(|item| item.media_item_id)
                .collect::<Vec<_>>()
        };

        assert_ne!(
            build_with_order(PlaybackOrder::Shuffle),
            build_with_order(PlaybackOrder::ShuffleInOrder),
            "Shuffle and ShuffleInOrder must no longer produce identical orders"
        );
    }

    #[test]
    fn classic_random_rotation_avoids_consecutive_same_group() {
        // RandomRotation used to be identical to Random (both plain seeded
        // shuffles); it now buckets by group and avoids drawing the same
        // group twice in a row when more than one group still has items.
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap();
        let schedule_item = ProgramScheduleItem {
            playback_order: PlaybackOrder::RandomRotation,
            ..schedule_item(1, PlayoutMode::Multiple, 1, Some("6"))
        };
        let key = item_key(&schedule_item);
        let media_items = vec![
            music_video(101, 1),
            music_video(102, 1),
            music_video(103, 1),
            music_video(201, 2),
            music_video(301, 3),
            music_video(401, 4),
        ];
        let artists_by_id = media_items
            .iter()
            .map(|item| (item.id, item.artist_id))
            .collect::<BTreeMap<_, _>>();
        let mut reference_data = SchedulingReferenceData {
            classic_schedule_items: vec![schedule_item],
            ..SchedulingReferenceData::default()
        };
        reference_data.media_by_key.insert(key, media_items);

        let result = ClassicSchedulingEngine
            .schedule(SchedulingRequest {
                playout: Playout {
                    id: 50,
                    schedule_kind: PlayoutScheduleKind::Classic,
                    seed: 2,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::minutes(30),
                mode: PlayoutBuildMode::Continue,
                reference_data,
            })
            .expect("classic schedule should build");

        let artists = result
            .added_items
            .iter()
            .map(|item| artists_by_id.get(&item.media_item_id).copied().flatten())
            .collect::<Vec<_>>();
        assert_eq!(artists.len(), 6);
        assert!(
            artists.windows(2).all(|window| window[0] != window[1]),
            "RandomRotation placed same-group videos too closely: {artists:?}"
        );
    }

    #[test]
    fn classic_random_rotation_randomizes_items_within_a_single_group() {
        let order_for_seed = |seed| {
            let mut items = vec![
                music_video(101, 1),
                music_video(102, 1),
                music_video(103, 1),
                music_video(104, 1),
            ];
            shuffle_with_random_rotation(&mut items, seed, None);
            items.into_iter().map(|item| item.id).collect::<Vec<_>>()
        };

        assert_ne!(
            order_for_seed(1),
            order_for_seed(2),
            "RandomRotation seed must affect item order even when all items share one group"
        );
    }

    #[test]
    fn classic_random_rotation_randomizes_items_within_each_group() {
        let source = vec![
            music_video(101, 1),
            music_video(102, 1),
            music_video(103, 1),
            music_video(201, 2),
            music_video(202, 2),
            music_video(203, 2),
        ];

        for (artist_id, repository_order) in [(1, vec![101, 102, 103]), (2, vec![201, 202, 203])] {
            let changes_order = (1..=16).any(|seed| {
                let mut items = source.clone();
                shuffle_with_random_rotation(&mut items, seed, None);
                let group_order = items
                    .into_iter()
                    .filter(|item| item.artist_id == Some(artist_id))
                    .map(|item| item.id)
                    .collect::<Vec<_>>();
                group_order != repository_order
            });
            assert!(
                changes_order,
                "RandomRotation retained repository order within artist group {artist_id}"
            );
        }
    }

    #[test]
    fn classic_random_rotation_separates_groups_across_cycle_boundaries() {
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap();
        let schedule_item = ProgramScheduleItem {
            playback_order: PlaybackOrder::RandomRotation,
            ..schedule_item(1, PlayoutMode::Multiple, 1, Some("12"))
        };
        let key = item_key(&schedule_item);
        let media_items = vec![
            music_video(101, 1),
            music_video(102, 1),
            music_video(201, 2),
            music_video(202, 2),
            music_video(301, 3),
            music_video(302, 3),
        ];
        let artists_by_id = media_items
            .iter()
            .map(|item| (item.id, item.artist_id))
            .collect::<BTreeMap<_, _>>();
        let mut reference_data = SchedulingReferenceData {
            classic_schedule_items: vec![schedule_item],
            ..SchedulingReferenceData::default()
        };
        reference_data.media_by_key.insert(key, media_items);

        let result = ClassicSchedulingEngine
            .schedule(SchedulingRequest {
                playout: Playout {
                    id: 50,
                    schedule_kind: PlayoutScheduleKind::Classic,
                    seed: 1,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::minutes(60),
                mode: PlayoutBuildMode::Continue,
                reference_data,
            })
            .expect("classic schedule should build");

        let artists = result
            .added_items
            .iter()
            .map(|item| artists_by_id.get(&item.media_item_id).copied().flatten())
            .collect::<Vec<_>>();
        assert_eq!(artists.len(), 12);
        assert!(
            artists.windows(2).all(|window| window[0] != window[1]),
            "RandomRotation repeated a group across a cycle boundary: {artists:?}"
        );
    }

    #[test]
    fn classic_random_rotation_large_unique_group_fast_path() {
        let source = (1..=10_000)
            .map(|id| SchedulingMediaItem::new(id, Duration::minutes(1)))
            .collect::<Vec<_>>();
        let mut plain_shuffle = source.clone();
        shuffle_with_seed(&mut plain_shuffle, 7);
        let predecessor = plain_shuffle.first().map(shuffle_group_key);

        let mut rotated = source;
        shuffle_with_random_rotation(&mut rotated, 7, predecessor);

        assert_eq!(rotated.len(), 10_000);
        assert_eq!(
            rotated
                .iter()
                .map(|item| item.id)
                .collect::<BTreeSet<_>>()
                .len(),
            10_000
        );
        assert_ne!(
            rotated.first().map(shuffle_group_key),
            predecessor,
            "the unique-group fast path must preserve boundary separation"
        );
    }

    #[test]
    fn classic_random_rotation_continue_reconstructs_cycle_boundary() {
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap();
        let schedule_item = ProgramScheduleItem {
            playback_order: PlaybackOrder::RandomRotation,
            ..schedule_item(1, PlayoutMode::Multiple, 1, Some("6"))
        };
        let key = item_key(&schedule_item);
        let media_items = vec![
            music_video(101, 1),
            music_video(102, 1),
            music_video(201, 2),
            music_video(202, 2),
            music_video(301, 3),
            music_video(302, 3),
        ];
        let artists_by_id = media_items
            .iter()
            .map(|item| (item.id, item.artist_id))
            .collect::<BTreeMap<_, _>>();
        let reference_data = {
            let mut reference_data = SchedulingReferenceData {
                classic_schedule_items: vec![schedule_item],
                ..SchedulingReferenceData::default()
            };
            reference_data.media_by_key.insert(key, media_items);
            reference_data
        };
        let playout = Playout {
            id: 50,
            schedule_kind: PlayoutScheduleKind::Classic,
            seed: 1,
            ..Playout::default()
        };

        let uninterrupted = ClassicSchedulingEngine
            .schedule(SchedulingRequest {
                playout: playout.clone(),
                start,
                finish: start + Duration::minutes(35),
                mode: PlayoutBuildMode::Continue,
                reference_data: reference_data.clone(),
            })
            .expect("uninterrupted RandomRotation schedule should build");
        let first_build = ClassicSchedulingEngine
            .schedule(SchedulingRequest {
                playout: playout.clone(),
                start,
                finish: start + Duration::minutes(30),
                mode: PlayoutBuildMode::Continue,
                reference_data: reference_data.clone(),
            })
            .expect("first RandomRotation build should complete one cycle");
        let first_anchor = first_build
            .anchor
            .clone()
            .expect("persisted playout anchor");
        assert!(first_anchor
            .context
            .as_deref()
            .is_some_and(|context| context.contains("random_rotation_cycle_predecessors")));

        let mut continued_reference_data = reference_data;
        continued_reference_data.existing_program_schedule_anchors =
            first_build.program_schedule_anchors;
        let continued = ClassicSchedulingEngine
            .schedule(SchedulingRequest {
                playout: Playout {
                    anchor: Some(first_anchor.clone()),
                    ..playout
                },
                start: first_anchor.next_start,
                finish: first_anchor.next_start + Duration::minutes(5),
                mode: PlayoutBuildMode::Continue,
                reference_data: continued_reference_data,
            })
            .expect("continued RandomRotation schedule should reconstruct the next cycle");

        let uninterrupted_ids = uninterrupted
            .added_items
            .iter()
            .map(|item| item.media_item_id)
            .collect::<Vec<_>>();
        let continued_id = continued
            .added_items
            .first()
            .map(|item| item.media_item_id)
            .expect("continued item");
        assert_eq!(continued_id, uninterrupted_ids[6]);
        assert_ne!(
            artists_by_id[&uninterrupted_ids[5]], artists_by_id[&continued_id],
            "RandomRotation repeated a group at a persisted build seam"
        );
    }

    #[test]
    fn classic_random_rotation_continue_preserves_duplicate_override_identity() {
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap();
        let schedule_item = ProgramScheduleItem {
            playback_order: PlaybackOrder::RandomRotation,
            ..schedule_item(1, PlayoutMode::Multiple, 1, Some("8"))
        };
        let key = item_key(&schedule_item);
        let default = music_video(101, 1);
        let commentary = SchedulingMediaItem {
            preferred_audio_language_code: Some("eng".to_owned()),
            preferred_audio_title: Some("Director Commentary".to_owned()),
            preferred_subtitle_language_code: Some("eng".to_owned()),
            subtitle_mode: Some(ChannelSubtitleMode::Any),
            ..music_video(101, 1)
        };
        let reference_data = {
            let mut reference_data = SchedulingReferenceData {
                classic_schedule_items: vec![schedule_item],
                ..SchedulingReferenceData::default()
            };
            reference_data.media_by_key.insert(
                key,
                vec![
                    default,
                    commentary,
                    music_video(201, 2),
                    music_video(301, 3),
                ],
            );
            reference_data
        };
        let item_identity = |item: &PlayoutItem| {
            (
                item.media_item_id,
                item.preferred_audio_language_code.clone(),
                item.preferred_audio_title.clone(),
                item.preferred_subtitle_language_code.clone(),
                item.subtitle_mode,
            )
        };

        for seed in 1..=32 {
            let playout = Playout {
                id: 50,
                schedule_kind: PlayoutScheduleKind::Classic,
                seed,
                ..Playout::default()
            };
            let uninterrupted = ClassicSchedulingEngine
                .schedule(SchedulingRequest {
                    playout: playout.clone(),
                    start,
                    finish: start + Duration::minutes(40),
                    mode: PlayoutBuildMode::Continue,
                    reference_data: reference_data.clone(),
                })
                .expect("uninterrupted RandomRotation schedule should build");
            let first_build = ClassicSchedulingEngine
                .schedule(SchedulingRequest {
                    playout: playout.clone(),
                    start,
                    finish: start + Duration::minutes(20),
                    mode: PlayoutBuildMode::Continue,
                    reference_data: reference_data.clone(),
                })
                .expect("first RandomRotation build should complete one cycle");
            let first_anchor = first_build
                .anchor
                .clone()
                .expect("persisted playout anchor");
            let mut continued_reference_data = reference_data.clone();
            continued_reference_data.existing_program_schedule_anchors =
                first_build.program_schedule_anchors;
            let continued = ClassicSchedulingEngine
                .schedule(SchedulingRequest {
                    playout: Playout {
                        anchor: Some(first_anchor.clone()),
                        ..playout
                    },
                    start: first_anchor.next_start,
                    finish: first_anchor.next_start + Duration::minutes(20),
                    mode: PlayoutBuildMode::Continue,
                    reference_data: continued_reference_data,
                })
                .expect("continued RandomRotation schedule should reconstruct variant order");

            let expected = uninterrupted
                .added_items
                .iter()
                .skip(4)
                .map(item_identity)
                .collect::<Vec<_>>();
            let actual = continued
                .added_items
                .iter()
                .map(item_identity)
                .collect::<Vec<_>>();
            assert_eq!(
                actual, expected,
                "RandomRotation changed a duplicate playback override at seed {seed}"
            );
        }
    }

    #[test]
    fn classic_random_draws_with_replacement_and_differs_from_shuffle() {
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap();
        let build = |playback_order| {
            let schedule_item = ProgramScheduleItem {
                playback_order,
                ..schedule_item(1, PlayoutMode::Multiple, 1, Some("4"))
            };
            let key = item_key(&schedule_item);
            let mut reference_data = SchedulingReferenceData {
                classic_schedule_items: vec![schedule_item],
                ..SchedulingReferenceData::default()
            };
            reference_data.media_by_key.insert(
                key,
                vec![
                    SchedulingMediaItem::new(101, Duration::minutes(1)),
                    SchedulingMediaItem::new(102, Duration::minutes(1)),
                    SchedulingMediaItem::new(103, Duration::minutes(1)),
                    SchedulingMediaItem::new(104, Duration::minutes(1)),
                ],
            );
            ClassicSchedulingEngine
                .schedule(SchedulingRequest {
                    playout: Playout {
                        id: 50,
                        schedule_kind: PlayoutScheduleKind::Classic,
                        seed: 1,
                        ..Playout::default()
                    },
                    start,
                    finish: start + Duration::minutes(4),
                    mode: PlayoutBuildMode::Continue,
                    reference_data,
                })
                .expect("classic schedule should build")
                .added_items
                .iter()
                .map(|item| item.media_item_id)
                .collect::<Vec<_>>()
        };

        let random = build(PlaybackOrder::Random);
        let shuffle = build(PlaybackOrder::Shuffle);
        assert_eq!(random, vec![103, 102, 101, 103]);
        assert_eq!(random.iter().copied().collect::<BTreeSet<_>>().len(), 3);
        assert_ne!(random, shuffle);
    }

    #[test]
    fn classic_random_continue_matches_uninterrupted_draws() {
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap();
        let schedule_item = ProgramScheduleItem {
            playback_order: PlaybackOrder::Random,
            ..schedule_item(1, PlayoutMode::Multiple, 1, Some("6"))
        };
        let key = item_key(&schedule_item);
        let mut reference_data = SchedulingReferenceData {
            classic_schedule_items: vec![schedule_item],
            ..SchedulingReferenceData::default()
        };
        reference_data.media_by_key.insert(
            key,
            vec![
                SchedulingMediaItem::new(101, Duration::minutes(1)),
                SchedulingMediaItem::new(102, Duration::minutes(1)),
                SchedulingMediaItem::new(103, Duration::minutes(1)),
                SchedulingMediaItem::new(104, Duration::minutes(1)),
            ],
        );
        let playout = Playout {
            id: 50,
            schedule_kind: PlayoutScheduleKind::Classic,
            seed: 1,
            ..Playout::default()
        };

        let uninterrupted = ClassicSchedulingEngine
            .schedule(SchedulingRequest {
                playout: playout.clone(),
                start,
                finish: start + Duration::minutes(6),
                mode: PlayoutBuildMode::Continue,
                reference_data: reference_data.clone(),
            })
            .expect("uninterrupted Random schedule should build");
        let first_build = ClassicSchedulingEngine
            .schedule(SchedulingRequest {
                playout: playout.clone(),
                start,
                finish: start + Duration::minutes(3),
                mode: PlayoutBuildMode::Continue,
                reference_data: reference_data.clone(),
            })
            .expect("first Random build should persist draw state");
        let first_anchor = first_build
            .anchor
            .clone()
            .expect("persisted playout anchor");
        let mut continued_reference_data = reference_data;
        continued_reference_data.existing_program_schedule_anchors =
            first_build.program_schedule_anchors;
        let continued = ClassicSchedulingEngine
            .schedule(SchedulingRequest {
                playout: Playout {
                    anchor: Some(first_anchor.clone()),
                    ..playout
                },
                start: first_anchor.next_start,
                finish: first_anchor.next_start + Duration::minutes(3),
                mode: PlayoutBuildMode::Continue,
                reference_data: continued_reference_data,
            })
            .expect("continued Random schedule should resume draw count");

        assert_eq!(
            continued
                .added_items
                .iter()
                .map(|item| item.media_item_id)
                .collect::<Vec<_>>(),
            uninterrupted
                .added_items
                .iter()
                .skip(3)
                .map(|item| item.media_item_id)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn classic_random_start_point_only_applies_to_chronological_season_episode_shuffle_in_order() {
        // ErsatzTV scopes Random Start Point to Chronological/SeasonEpisode/
        // ShuffleInOrder, since other orders are already randomized by
        // construction. Applying it unconditionally (the old ErsatzRS
        // behavior) was a no-op-in-spirit for Shuffle/Random but still wrong:
        // it means those orders' first cycle silently used a different
        // starting index than a fresh build without the flag.
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap();
        let starting_index = |playback_order: PlaybackOrder, random_start_point: bool| {
            let schedule_item = ProgramScheduleItem {
                playback_order,
                ..schedule_item(1, PlayoutMode::One, 1, None)
            };
            let key = item_key(&schedule_item);
            let mut reference_data = SchedulingReferenceData {
                classic_schedule_items: vec![schedule_item],
                classic_random_start_point: random_start_point,
                ..SchedulingReferenceData::default()
            };
            reference_data.media_by_key.insert(
                key,
                vec![
                    SchedulingMediaItem::new(101, Duration::minutes(10)),
                    SchedulingMediaItem::new(102, Duration::minutes(10)),
                    SchedulingMediaItem::new(103, Duration::minutes(10)),
                ],
            );
            ClassicSchedulingEngine
                .schedule(SchedulingRequest {
                    playout: Playout {
                        id: 50,
                        schedule_kind: PlayoutScheduleKind::Classic,
                        seed: 4,
                        ..Playout::default()
                    },
                    start,
                    finish: start + Duration::minutes(10),
                    mode: PlayoutBuildMode::Continue,
                    reference_data,
                })
                .expect("classic schedule should build")
                .program_schedule_anchors
                .first()
                .map_or(0, |anchor| anchor.enumerator_state.index)
        };

        assert_ne!(
            starting_index(PlaybackOrder::Chronological, false),
            starting_index(PlaybackOrder::Chronological, true),
            "Chronological should honor random_start_point"
        );
        assert_eq!(
            starting_index(PlaybackOrder::Shuffle, false),
            starting_index(PlaybackOrder::Shuffle, true),
            "Shuffle should ignore random_start_point (already randomized)"
        );
    }

    #[test]
    fn classic_continue_anchor_uses_last_item_finish_after_hard_stop_overshoot() {
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 23, 50, 0).unwrap();
        let finish = Utc.with_ymd_and_hms(2026, 4, 27, 0, 0, 0).unwrap();
        let schedule_item = schedule_item(1, PlayoutMode::One, 1, None);
        let key = item_key(&schedule_item);
        let mut reference_data = SchedulingReferenceData {
            classic_schedule_items: vec![schedule_item],
            ..SchedulingReferenceData::default()
        };
        reference_data.media_by_key.insert(
            key,
            vec![SchedulingMediaItem::new(101, Duration::minutes(20))],
        );

        let result = ClassicSchedulingEngine
            .schedule(SchedulingRequest {
                playout: Playout {
                    id: 50,
                    schedule_kind: PlayoutScheduleKind::Classic,
                    seed: 7,
                    ..Playout::default()
                },
                start,
                finish,
                mode: PlayoutBuildMode::Continue,
                reference_data,
            })
            .expect("classic schedule should build");

        let scheduled = result.added_items.first().expect("scheduled item");
        assert_eq!(scheduled.start, start);
        assert_eq!(scheduled.finish, start + Duration::minutes(20));
        assert_eq!(
            result.anchor.map(|anchor| anchor.next_start),
            Some(scheduled.finish)
        );
    }

    #[test]
    fn classic_preserves_and_advances_fill_group_state() {
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap();
        let schedule_item = ProgramScheduleItem {
            fill_with_group_mode: FillWithGroupMode::FillWithShuffledGroups,
            ..schedule_item(1, PlayoutMode::One, 1, None)
        };
        let key = item_key(&schedule_item);
        let mut reference_data = SchedulingReferenceData {
            classic_schedule_items: vec![schedule_item],
            existing_fill_group_indices: vec![PlayoutScheduleItemFillGroupIndex {
                id: 700,
                playout_id: 50,
                program_schedule_item_id: 1,
                enumerator_state: CollectionEnumeratorState { seed: 11, index: 4 },
            }],
            ..SchedulingReferenceData::default()
        };
        reference_data.media_by_key.insert(
            key,
            vec![SchedulingMediaItem::new(101, Duration::minutes(10))],
        );

        let result = ClassicSchedulingEngine
            .schedule(SchedulingRequest {
                playout: Playout {
                    id: 50,
                    schedule_kind: PlayoutScheduleKind::Classic,
                    seed: 7,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::minutes(10),
                mode: PlayoutBuildMode::Continue,
                reference_data,
            })
            .expect("classic schedule should build");

        let state = result
            .fill_group_indices
            .first()
            .map(|index| index.enumerator_state)
            .expect("fill group state");
        assert_eq!(state.index, 5);
        assert_ne!(state.seed, 11);
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn classic_schedule_item_filler_presets_wrap_content_and_tail_gap() {
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap();
        let first_item = ProgramScheduleItem {
            start_type: StartType::Fixed,
            start_time: Some(Duration::hours(6)),
            pre_roll_filler_id: Some(91),
            post_roll_filler_id: Some(92),
            tail_filler_id: Some(93),
            fallback_filler_id: Some(94),
            ..schedule_item(1, PlayoutMode::One, 1, None)
        };
        let fixed_next = ProgramScheduleItem {
            start_type: StartType::Fixed,
            start_time: Some(Duration::hours(7)),
            ..schedule_item(2, PlayoutMode::One, 2, None)
        };
        let mut reference_data = SchedulingReferenceData {
            classic_schedule_items: vec![first_item, fixed_next],
            classic_filler_presets: BTreeMap::from([
                (
                    91,
                    filler_preset(91, FillerKind::PreRoll, FillerMode::Count, Some(1), None),
                ),
                (
                    92,
                    filler_preset(92, FillerKind::PostRoll, FillerMode::Count, Some(1), None),
                ),
                (
                    93,
                    filler_preset(
                        93,
                        FillerKind::Tail,
                        FillerMode::Duration,
                        None,
                        Some(Duration::minutes(15)),
                    ),
                ),
                (
                    94,
                    filler_preset(94, FillerKind::Fallback, FillerMode::Count, Some(1), None),
                ),
            ]),
            ..SchedulingReferenceData::default()
        };
        for item in &reference_data.classic_schedule_items {
            reference_data.media_by_key.insert(
                item_key(item),
                vec![SchedulingMediaItem::new(
                    if item.collection_id == Some(1) {
                        101
                    } else {
                        201
                    },
                    Duration::minutes(if item.collection_id == Some(1) {
                        20
                    } else {
                        10
                    }),
                )],
            );
        }
        for (id, duration) in [
            (91, Duration::minutes(5)),
            (92, Duration::minutes(5)),
            (93, Duration::minutes(25)),
            (94, Duration::minutes(30)),
        ] {
            reference_data
                .classic_filler_presets
                .get_mut(&id)
                .expect("filler preset")
                .media_items
                .push(SchedulingMediaItem::new(800 + id, duration));
        }

        let result = ClassicSchedulingEngine
            .schedule(SchedulingRequest {
                playout: Playout {
                    id: 50,
                    schedule_kind: PlayoutScheduleKind::Classic,
                    seed: 7,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::hours(1) + Duration::minutes(10),
                mode: PlayoutBuildMode::Continue,
                reference_data,
            })
            .expect("classic schedule should build");

        assert_eq!(
            result
                .added_items
                .iter()
                .map(|item| (
                    item.media_item_id,
                    item.start,
                    item.finish,
                    item.filler_kind
                ))
                .collect::<Vec<_>>(),
            vec![
                (
                    891,
                    start,
                    start + Duration::minutes(5),
                    FillerKind::PreRoll,
                ),
                (
                    101,
                    start + Duration::minutes(5),
                    start + Duration::minutes(25),
                    FillerKind::None,
                ),
                (
                    892,
                    start + Duration::minutes(25),
                    start + Duration::minutes(30),
                    FillerKind::PostRoll,
                ),
                (
                    893,
                    start + Duration::minutes(30),
                    start + Duration::minutes(55),
                    FillerKind::Tail,
                ),
                (
                    894,
                    start + Duration::minutes(55),
                    start + Duration::hours(1),
                    FillerKind::Fallback,
                ),
                (
                    201,
                    start + Duration::hours(1),
                    start + Duration::hours(1) + Duration::minutes(10),
                    FillerKind::None,
                ),
            ]
        );
        assert!(result.added_items[0]
            .scheduling_context
            .as_deref()
            .is_some_and(|context| context.contains("\"FillerPresetId\":91")));
    }

    fn schedule_item(
        id: EntityId,
        mode: PlayoutMode,
        collection_id: EntityId,
        multiple_count: Option<&str>,
    ) -> ProgramScheduleItem {
        ProgramScheduleItem {
            id,
            program_schedule_id: 10,
            collection_type: CollectionType::Collection,
            collection_id: Some(collection_id),
            playout_mode: mode,
            playback_order: PlaybackOrder::Chronological,
            multiple_count: multiple_count.map(str::to_owned),
            sort_order: i32::try_from(id).unwrap_or_default(),
            ..ProgramScheduleItem::default()
        }
    }

    fn filler_preset(
        id: EntityId,
        filler_kind: FillerKind,
        filler_mode: FillerMode,
        count: Option<i32>,
        duration: Option<Duration>,
    ) -> SchedulingFillerPreset {
        SchedulingFillerPreset {
            id,
            filler_kind,
            filler_mode,
            duration,
            count,
            pad_to_nearest_minute: None,
            allow_watermarks: true,
            media_items: Vec::new(),
        }
    }

    fn music_video(id: EntityId, artist_id: EntityId) -> SchedulingMediaItem {
        SchedulingMediaItem {
            artist_id: Some(artist_id),
            ..SchedulingMediaItem::new(id, Duration::minutes(5))
        }
    }

    fn snapshot(result: &SchedulingResult) -> serde_json::Value {
        serde_json::json!({
            "items": result.added_items.iter().map(|item| serde_json::json!({
                "media_item_id": item.media_item_id,
                "start": item.start,
                "finish": item.finish,
                "guide_group": item.guide_group,
                "context": item.scheduling_context,
            })).collect::<Vec<_>>(),
            "anchor": result.anchor.as_ref().map(|anchor| serde_json::json!({
                "index": anchor.schedule_items_enumerator_state.index,
                "next_start": anchor.next_start,
                "multiple_remaining": anchor.multiple_remaining,
                "duration_finish": anchor.duration_finish,
                "in_flood": anchor.in_flood,
                "next_guide_group": anchor.next_guide_group,
            })),
            "program_schedule_anchors": result.program_schedule_anchors.iter().map(|anchor| serde_json::json!({
                "collection_id": anchor.collection_id,
                "index": anchor.enumerator_state.index,
            })).collect::<Vec<_>>(),
            "warnings": result.warnings,
        })
    }
}
