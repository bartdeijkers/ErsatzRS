use std::collections::{BTreeMap, BTreeSet, VecDeque};

use chrono::{DateTime, Duration, Timelike, Utc};
use ersatzrs_domain::{
    CollectionEnumeratorState, EntityId, FillWithGroupMode, FillerKind, FillerMode,
    FixedStartTimeBehavior, GuideMode, MultipleMode, PlaybackOrder, PlayoutAnchor, PlayoutItem,
    PlayoutProgramScheduleAnchor, PlayoutScheduleItemFillGroupIndex, ProgramScheduleItem,
    StartType, TailMode,
};
use serde::{Deserialize, Serialize};

use super::{
    cleanup_history, history, history_details_for_media_item, history_key_for_collection_key,
    playout_item, shuffle_with_seed, yaml, MediaEnumerator, PlaylistEnumerator, PlayoutBuildMode,
    PlayoutSchedulingEngine, SchedulingCollectionKey, SchedulingError, SchedulingMediaItem,
    SchedulingReferenceData, SchedulingRequest, SchedulingResult,
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
    result: SchedulingResult,
    next_anchor_id: EntityId,
    next_fill_group_id: EntityId,
}

impl ClassicContext {
    fn new(
        request: SchedulingRequest,
        schedule_items: Vec<ProgramScheduleItem>,
        state: ClassicState,
    ) -> Self {
        let mut media = BTreeMap::new();
        let mut playlists = BTreeMap::new();
        let mut media_states = BTreeMap::new();
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
            prepare_enumerator_items(
                &mut enumerator,
                item.playback_order,
                u64::try_from(request.playout.seed).unwrap_or_default(),
            );
            if request.reference_data.classic_random_start_point && !enumerator.is_empty() {
                let index =
                    usize::try_from(request.playout.seed).unwrap_or_default() % enumerator.len();
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
            media.insert(key, enumerator);
        }
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

        let collection_anchors = retained_collection_anchors(&request);
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
            result: SchedulingResult {
                remove_before: Some(remove_before),
                ..SchedulingResult::default()
            },
            next_anchor_id: 0,
            next_fill_group_id: 0,
        };
        context.refresh_schedule_order();
        context
    }

    fn apply_existing_collection_anchors(&mut self) {
        self.next_anchor_id = self
            .request
            .reference_data
            .existing_program_schedule_anchors
            .iter()
            .map(|anchor| anchor.id)
            .max()
            .unwrap_or_default()
            + 1;
        self.next_fill_group_id = self
            .request
            .reference_data
            .existing_fill_group_indices
            .iter()
            .map(|index| index.id)
            .max()
            .unwrap_or_default()
            + 1;

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
                let seed = u64::try_from(anchor.enumerator_state.seed).unwrap_or_default();
                prepare_enumerator_items(enumerator, enumerator.playback_order, seed);
                enumerator.set_index(usize::try_from(anchor.enumerator_state.index).unwrap_or(0));
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

            if self.schedule_media_item(schedule_item, &media_item, item_start_time, None, "Flood")
            {
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
                    (
                        enumerator.scheduling_context_name(),
                        seed,
                        i32::try_from(enumerator.current_index()).unwrap_or_default(),
                    )
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
        let mut next_id = self.next_anchor_id;
        for schedule_item in &self.schedule_items {
            let key = collection_key(schedule_item);
            let key_string = history_key_for_collection_key(&key);
            let Some(enumerator) = self.media.get(&key_string) else {
                continue;
            };
            let existing = self
                .collection_anchors
                .iter()
                .find(|anchor| anchor.anchor_date.is_none() && anchor_matches_key(anchor, &key));
            let id = existing.map_or_else(
                || {
                    let id = next_id;
                    next_id += 1;
                    id
                },
                |anchor| anchor.id,
            );
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
                    index: i32::try_from(enumerator.current_index()).unwrap_or_default(),
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
        let mut next_id = self.next_fill_group_id;
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
            let id = existing.map_or_else(
                || {
                    let id = next_id;
                    next_id += 1;
                    id
                },
                |index| index.id,
            );
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
            self.state.schedule_state.index = 0;
            if self.request.reference_data.classic_shuffle_schedule_items {
                self.state.schedule_state.seed = next_i32_seed(self.state.schedule_state.seed);
                self.refresh_schedule_order();
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

    fn current_media(&self, schedule_item: &ProgramScheduleItem) -> Option<SchedulingMediaItem> {
        let key = item_key(schedule_item);
        self.playlists
            .get(&key)
            .and_then(PlaylistEnumerator::current)
            .cloned()
            .or_else(|| {
                self.media
                    .get(&key)
                    .and_then(MediaEnumerator::current)
                    .cloned()
            })
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
            let tail_media_id = enumerator.current().map(|item| item.id);
            enumerator.advance();
            let index = i32::try_from(enumerator.current_index()).unwrap_or_default();
            let should_reshuffle = enumerator.current_index() == 0
                && matches!(
                    enumerator.playback_order,
                    PlaybackOrder::Random
                        | PlaybackOrder::Shuffle
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
                let seed = prepare_next_cycle(enumerator, playback_order, seed, tail_media_id);
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
        self.media_states
            .get(&item_key(schedule_item))
            .map_or_else(|| self.request.playout.seed, |state| state.seed)
            .try_into()
            .unwrap_or_default()
    }

    fn refresh_schedule_order(&mut self) {
        self.schedule_order = (0..self.schedule_items.len()).collect();
        if self.request.reference_data.classic_shuffle_schedule_items {
            let seed = u64::try_from(self.state.schedule_state.seed).unwrap_or_default();
            shuffle_with_seed(&mut self.schedule_order, seed);
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
        (Some(_), None) | (None, None) => false,
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
        rerun_collection_id: anchor.rerun_collection_id,
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
        PlaybackOrder::Shuffle
        | PlaybackOrder::ShuffleInOrder
        | PlaybackOrder::MultiEpisodeShuffle => {
            shuffle_with_artist_distance(&mut enumerator.items, seed);
        }
        PlaybackOrder::Random | PlaybackOrder::RandomRotation | PlaybackOrder::Marathon => {
            shuffle_with_seed(&mut enumerator.items, seed);
        }
        PlaybackOrder::None => {}
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShuffleGroupKey {
    Artist(EntityId),
    Item(EntityId),
}

fn shuffle_group_key(item: &SchedulingMediaItem) -> ShuffleGroupKey {
    item.artist_id
        .map_or(ShuffleGroupKey::Item(item.id), ShuffleGroupKey::Artist)
}

fn shuffle_with_artist_distance(items: &mut Vec<SchedulingMediaItem>, seed: u64) {
    shuffle_with_seed(items, seed);

    let mut seen_artists = BTreeSet::new();
    if !items
        .iter()
        .filter_map(|item| item.artist_id)
        .any(|artist_id| !seen_artists.insert(artist_id))
    {
        return;
    }

    let mut groups: Vec<(ShuffleGroupKey, VecDeque<SchedulingMediaItem>)> = Vec::new();
    for item in items.drain(..) {
        let key = shuffle_group_key(&item);
        if let Some((_, group_items)) = groups.iter_mut().find(|(group_key, _)| *group_key == key) {
            group_items.push_back(item);
        } else {
            groups.push((key, VecDeque::from([item])));
        }
    }

    let total_len = groups.iter().map(|(_, group)| group.len()).sum();
    let mut arranged = Vec::with_capacity(total_len);
    let mut previous_key = None;
    while arranged.len() < total_len {
        let candidate = artist_distance_candidate(&groups, previous_key);
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

fn artist_distance_candidate(
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
) -> i32 {
    prepare_enumerator_items(
        enumerator,
        playback_order,
        u64::try_from(seed).unwrap_or_default(),
    );

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
        prepare_enumerator_items(
            enumerator,
            playback_order,
            u64::try_from(seed).unwrap_or_default(),
        );
    }

    seed
}

fn next_i32_seed(seed: i32) -> i32 {
    let next = u64::try_from(seed)
        .unwrap_or_default()
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
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
        prepare_enumerator_items(&mut expected, PlaybackOrder::Shuffle, 42);
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
    fn classic_shuffle_spreads_same_artist_music_videos() {
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap();
        let schedule_item = ProgramScheduleItem {
            playback_order: PlaybackOrder::Shuffle,
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
