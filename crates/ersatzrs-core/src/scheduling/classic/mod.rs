use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Timelike, Utc};
use ersatzrs_domain::{
    CollectionEnumeratorState, EntityId, FillWithGroupMode, FillerKind, FixedStartTimeBehavior,
    GuideMode, MultipleMode, PlaybackOrder, PlayoutAnchor, PlayoutProgramScheduleAnchor,
    PlayoutScheduleItemFillGroupIndex, ProgramScheduleItem, StartType, TailMode,
};
use serde::{Deserialize, Serialize};

use super::{
    cleanup_history, history, history_details_for_media_item, history_key_for_collection_key,
    playout_item, MediaEnumerator, PlayoutSchedulingEngine, SchedulingCollectionKey,
    SchedulingError, SchedulingMediaItem, SchedulingReferenceData, SchedulingRequest,
    SchedulingResult,
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
        let state = request.playout.anchor.as_ref().map_or_else(
            || start_state(&request, &items),
            |anchor| anchor_state(anchor, &request),
        );

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
    state: ClassicState,
    media: BTreeMap<String, MediaEnumerator>,
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
        for item in &schedule_items {
            let key = item_key(item);
            let mut enumerator = MediaEnumerator::new(
                key.clone(),
                request
                    .reference_data
                    .media_items_for_source(&super::SchedulingContentSource::direct(&key)),
            )
            .with_playback_order(item.playback_order);
            sort_enumerator_items(&mut enumerator, item.playback_order);
            media.insert(key, enumerator);
        }

        let remove_before = state.current_time;

        Self {
            request,
            schedule_items,
            state,
            media,
            result: SchedulingResult {
                remove_before: Some(remove_before),
                ..SchedulingResult::default()
            },
            next_anchor_id: 0,
            next_fill_group_id: 0,
        }
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

        for anchor in &self
            .request
            .reference_data
            .existing_program_schedule_anchors
        {
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
                enumerator.set_index(usize::try_from(anchor.enumerator_state.index).unwrap_or(0));
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
            self.state.current_time = self
                .result
                .added_items
                .last()
                .map_or(item_start_time, |item| item.finish);
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

        if self.state.multiple_remaining.is_none() {
            self.state.multiple_remaining =
                Some(multiple_count(schedule_item, self.media(schedule_item)));
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
        }

        self.state.current_time = self
            .state
            .current_time
            .min(self.filler_start_after(next_schedule_item));
        self.state.next_guide_group += 1;
    }

    fn schedule_duration(&mut self, schedule_item: &ProgramScheduleItem) {
        let duration = schedule_item.duration.unwrap_or_else(Duration::zero);
        if duration <= Duration::zero() {
            self.advance_schedule_item();
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
            let Some(media_item) = self.media(schedule_item).current().cloned() else {
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
            if self.schedule_media_item(
                schedule_item,
                &media_item,
                item_start_time,
                Some(finish),
                "Duration",
            ) {
                self.advance_media(schedule_item);
                self.state.current_time = item_start_time + media_item.duration;
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
        } else if matches!(
            schedule_item.tail_mode,
            TailMode::Offline | TailMode::Filler
        ) {
            if let Some(until) = duration_until {
                self.state.current_time = until;
            }
        }
        self.state.next_guide_group += 1;
    }

    fn schedule_flood(
        &mut self,
        schedule_item: &ProgramScheduleItem,
        next_schedule_item: &ProgramScheduleItem,
    ) {
        let mut scheduled_any = false;
        while self.state.current_time < self.request.finish {
            let item_start_time = self.start_time_after(schedule_item, false);
            if item_start_time >= self.request.finish {
                self.state.current_time = self.request.finish;
                break;
            }

            let Some(media_item) = self.media(schedule_item).current().cloned() else {
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
        }
        self.state.next_guide_group += 1;
    }

    fn schedule_current_media_item(
        &mut self,
        schedule_item: &ProgramScheduleItem,
        start: DateTime<Utc>,
        guide_finish: Option<DateTime<Utc>>,
        scheduler: &'static str,
    ) -> bool {
        let Some(media_item) = self.media(schedule_item).current().cloned() else {
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
        let key = item_key(schedule_item);
        let (enumerator_name, enumerator_seed, enumerator_index) =
            self.media
                .get(&key)
                .map_or(("Random", self.request.playout.seed, 0), |enumerator| {
                    (
                        enumerator.scheduling_context_name(),
                        self.request.playout.seed,
                        i32::try_from(enumerator.current_index()).unwrap_or_default(),
                    )
                });
        let scheduling_context = serde_json::to_string(&ClassicSchedulingContext {
            scheduler,
            schedule_id: schedule_item.program_schedule_id,
            item_id: schedule_item.id,
            filler_preset_id: None,
            enumerator: enumerator_name,
            seed: enumerator_seed,
            index: enumerator_index,
        })
        .ok();

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
        item.guide_finish = guide_finish;
        item.preferred_audio_language_code
            .clone_from(&schedule_item.preferred_audio_language_code);
        item.preferred_audio_title
            .clone_from(&schedule_item.preferred_audio_title);
        item.preferred_subtitle_language_code
            .clone_from(&schedule_item.preferred_subtitle_language_code);
        item.subtitle_mode = schedule_item.subtitle_mode;
        item.collection_key = Some(key.clone());

        self.result.added_history.push(history(
            self.request.playout.id,
            key,
            &item,
            None,
            usize::try_from(enumerator_index).unwrap_or_default(),
            history_details_for_media_item(media_item),
            false,
        ));
        self.result.added_items.push(item);
        true
    }

    fn into_result(mut self) -> SchedulingResult {
        let current_item = self.schedule_items[self.current_schedule_index()].clone();
        let next_start = self.start_time_after(&current_item, false);
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
                .request
                .reference_data
                .existing_program_schedule_anchors
                .iter()
                .find(|anchor| anchor_matches_key(anchor, &key));
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
                    seed: self.request.playout.seed,
                    index: i32::try_from(enumerator.current_index()).unwrap_or_default(),
                },
            });
        }
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
                enumerator_state: CollectionEnumeratorState {
                    seed: self.request.playout.seed,
                    index: 0,
                },
            });
        }
        result
    }

    fn current_schedule_index(&self) -> usize {
        let len = self.schedule_items.len();
        if len == 0 {
            0
        } else {
            usize::try_from(self.state.schedule_state.index).unwrap_or(0) % len
        }
    }

    fn peek_schedule_index(&self, offset: usize) -> usize {
        (self.current_schedule_index() + offset) % self.schedule_items.len()
    }

    fn advance_schedule_item(&mut self) {
        let len = i32::try_from(self.schedule_items.len()).unwrap_or(1).max(1);
        self.state.schedule_state.index = (self.state.schedule_state.index + 1) % len;
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

    fn media_mut(&mut self, schedule_item: &ProgramScheduleItem) -> Option<&mut MediaEnumerator> {
        self.media.get_mut(&item_key(schedule_item))
    }

    fn advance_media(&mut self, schedule_item: &ProgramScheduleItem) {
        if let Some(enumerator) = self.media_mut(schedule_item) {
            enumerator.advance();
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
}

fn start_state(request: &SchedulingRequest, items: &[ProgramScheduleItem]) -> ClassicState {
    let index = items
        .iter()
        .position(|item| item.start_type == StartType::Fixed)
        .unwrap_or(0);
    let next_start = items
        .get(index)
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
            index: i32::try_from(index).unwrap_or_default(),
        },
        multiple_remaining: None,
        duration_finish: None,
        in_flood: false,
        in_duration_filler: false,
        next_guide_group: 1,
    }
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

fn multiple_count(schedule_item: &ProgramScheduleItem, enumerator: &MediaEnumerator) -> i32 {
    if schedule_item.multiple_mode == MultipleMode::CollectionSize {
        return i32::try_from(enumerator.len()).unwrap_or(i32::MAX).max(1);
    }
    schedule_item
        .multiple_count
        .as_deref()
        .and_then(|value| value.trim().parse::<i32>().ok())
        .unwrap_or(1)
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

fn sort_enumerator_items(enumerator: &mut MediaEnumerator, playback_order: PlaybackOrder) {
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
        _ => {}
    }
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
    use chrono::{TimeZone, Utc};
    use ersatzrs_domain::{CollectionType, Playout, PlayoutMode, PlayoutScheduleKind};
    use insta::assert_json_snapshot;

    use super::*;
    use crate::scheduling::{PlayoutBuildMode, SchedulingReferenceData};

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
