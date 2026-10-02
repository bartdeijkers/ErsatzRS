mod clock;
#[cfg(test)]
mod fixed_start_tests;
pub use clock::ClassicClock;

use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, BinaryHeap, VecDeque},
};

use chrono::{DateTime, Duration, Timelike, Utc};
use ersatzrs_domain::{
    ChannelSubtitleMode, CollectionEnumeratorState, EntityId, FillWithGroupMode, FillerKind,
    FillerMode, FixedStartTimeBehavior, GuideMode, MultipleMode, PinnedStreamSelection,
    PlaybackOrder, PlayoutAnchor, PlayoutClassicCheckpoint, PlayoutItem,
    PlayoutProgramScheduleAnchor, PlayoutScheduleItemFillGroupIndex, ProgramScheduleItem,
    StartType, TailMode,
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

/// Safety audit for the dated state required before a destructive Classic
/// Refresh can replace an existing materialized window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassicRefreshCheckpointAudit {
    pub checkpoint_date: Option<DateTime<Utc>>,
    pub expected_source_count: usize,
    pub actual_source_count: usize,
    pub diagnostic: Option<String>,
}

impl ClassicRefreshCheckpointAudit {
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        self.diagnostic.is_none()
    }
}

/// Number of nanoseconds in one .NET tick, the precision every persisted
/// ErsatzTV timestamp is stored at.
const NANOS_PER_TICK: u32 = 100;

/// Floor a timestamp to the 100-nanosecond tick precision used by persisted
/// ErsatzTV timestamp columns.
///
/// `PlayoutClassicCheckpoint."CheckpointDate"` is written through the shared
/// datetime encoder, which floors to .NET ticks for ErsatzTV parity, while the
/// same instant inside `"Context"` is serialized by serde at full nanosecond
/// precision. A checkpoint must therefore be generated at tick precision or the
/// two representations disagree after a database round trip, and
/// [`audit_classic_refresh_checkpoint`] rejects every later Refresh with
/// "checkpoint date and scheduler continuation time do not match".
#[must_use]
pub fn truncate_to_persisted_precision(value: DateTime<Utc>) -> DateTime<Utc> {
    // Subtract rather than call `with_nanosecond`, which fails on the leap
    // second that `timestamp_subsec_nanos` reports as `1_000_000_000 + n`.
    let remainder = value.timestamp_subsec_nanos() % NANOS_PER_TICK;
    value - Duration::nanoseconds(i64::from(remainder))
}

/// Prove that the earliest checkpoint on the Refresh day contains one state
/// for every unique schedule source plus compatible scheduler-wide context.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn audit_classic_refresh_checkpoint(
    reference_data: &SchedulingReferenceData,
    playout_id: EntityId,
    start: DateTime<Utc>,
) -> ClassicRefreshCheckpointAudit {
    let expected_keys = reference_data
        .classic_schedule_items
        .iter()
        .map(classic_schedule_item_key)
        .collect::<BTreeSet<_>>();
    let fail = |checkpoint_date, actual_source_count, reason: String| {
        ClassicRefreshCheckpointAudit {
            checkpoint_date,
            expected_source_count: expected_keys.len(),
            actual_source_count,
            diagnostic: Some(format!(
                "Classic Refresh refused: {reason}. Use Reset Playout once to create a complete checkpoint before retrying Refresh"
            )),
        }
    };
    let start_day = midnight(start);
    let next_day = start_day + Duration::days(1);
    let mut candidates = reference_data
        .existing_classic_checkpoints
        .iter()
        .filter(|checkpoint| {
            checkpoint.playout_id == playout_id
                && checkpoint.checkpoint_date >= start_day
                && checkpoint.checkpoint_date < next_day
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|checkpoint| checkpoint.checkpoint_date);
    let Some(checkpoint) = candidates.first() else {
        return fail(
            None,
            0,
            format!(
                "no complete dated checkpoint exists for {} (continuation-only legacy state is not safe to reuse)",
                start_day.date_naive()
            ),
        );
    };
    if candidates
        .iter()
        .filter(|candidate| candidate.checkpoint_date == checkpoint.checkpoint_date)
        .count()
        != 1
    {
        return fail(
            Some(checkpoint.checkpoint_date),
            0,
            "duplicate checkpoint rows exist at the selected seam".to_owned(),
        );
    }
    let Ok(context) = serde_json::from_str::<ClassicCheckpointContext>(&checkpoint.context) else {
        return fail(
            Some(checkpoint.checkpoint_date),
            0,
            "the selected checkpoint context is unreadable".to_owned(),
        );
    };
    if context.version != CLASSIC_CHECKPOINT_VERSION {
        return fail(
            Some(checkpoint.checkpoint_date),
            context.sources.len(),
            format!(
                "checkpoint version {} is incompatible with version {CLASSIC_CHECKPOINT_VERSION}",
                context.version
            ),
        );
    }
    if context.anchor.next_start != checkpoint.checkpoint_date {
        return fail(
            Some(checkpoint.checkpoint_date),
            context.sources.len(),
            "checkpoint date and scheduler continuation time do not match".to_owned(),
        );
    }
    let source_keys = context.sources.keys().cloned().collect::<BTreeSet<_>>();
    if source_keys != expected_keys
        || context.sources.values().any(|source| {
            usize::from(source.media.is_some()) + usize::from(source.playlist.is_some()) != 1
        })
    {
        return fail(
            Some(checkpoint.checkpoint_date),
            context.sources.len(),
            "checkpoint sources are partial, stale, or duplicated".to_owned(),
        );
    }
    if context.sources.iter().any(|(key, source)| {
        reference_data.playlist_items_by_key.contains_key(key)
            && source
                .playlist
                .as_ref()
                .is_none_or(|playlist| playlist.remaining_media_ids.is_none())
    }) {
        return fail(
            Some(checkpoint.checkpoint_date),
            context.sources.len(),
            "playlist checkpoint is missing playable-cycle state".to_owned(),
        );
    }

    let filler_context = context
        .anchor
        .context
        .as_deref()
        .and_then(|value| serde_json::from_str::<ClassicAnchorContext>(value).ok())
        .unwrap_or_default();
    for id in reference_data.classic_filler_presets.keys() {
        let previously_used = filler_context.filler_indices.contains_key(id)
            || reference_data.existing_items.iter().any(|item| {
                item.scheduling_context
                    .as_deref()
                    .and_then(filler_id_from_context)
                    == Some(*id)
            })
            || reference_data
                .existing_history
                .iter()
                .any(|history| history.key == format!("classic-filler:{id}"));
        if previously_used && !filler_context.filler_sources.contains_key(id) {
            return fail(
                Some(checkpoint.checkpoint_date),
                context.sources.len(),
                format!("filler identity checkpoint is missing for preset {id}"),
            );
        }
    }

    let expected_schedule_item_ids = reference_data
        .classic_schedule_items
        .iter()
        .map(|item| item.id)
        .collect::<BTreeSet<_>>();
    let checkpoint_schedule_item_ids = context
        .schedule_order
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    if context.schedule_order.len() != checkpoint_schedule_item_ids.len()
        || checkpoint_schedule_item_ids != expected_schedule_item_ids
    {
        return fail(
            Some(checkpoint.checkpoint_date),
            context.sources.len(),
            "schedule-item checkpoint order is partial, stale, or duplicated".to_owned(),
        );
    }

    let dated = reference_data
        .existing_program_schedule_anchors
        .iter()
        .filter(|anchor| anchor.anchor_date == Some(checkpoint.checkpoint_date))
        .collect::<Vec<_>>();
    let dated_keys = dated
        .iter()
        .map(|anchor| history_key_for_collection_key(&collection_key_for_anchor(anchor)))
        .collect::<Vec<_>>();
    let dated_key_set = dated_keys.iter().cloned().collect::<BTreeSet<_>>();
    if dated_keys.len() != dated_key_set.len() || dated_key_set != expected_keys {
        return fail(
            Some(checkpoint.checkpoint_date),
            dated.len(),
            "dated source anchors are partial, stale, or duplicated".to_owned(),
        );
    }
    if dated.iter().any(|anchor| {
        let key = history_key_for_collection_key(&collection_key_for_anchor(anchor));
        context
            .sources
            .get(&key)
            .is_none_or(|source| source.enumerator_state != anchor.enumerator_state)
    }) {
        return fail(
            Some(checkpoint.checkpoint_date),
            dated.len(),
            "dated source anchors disagree with the complete checkpoint context".to_owned(),
        );
    }

    let expected_fill_groups = reference_data
        .classic_schedule_items
        .iter()
        .filter(|item| item.fill_with_group_mode != FillWithGroupMode::None)
        .map(|item| item.id)
        .collect::<BTreeSet<_>>();
    let checkpoint_fill_groups = context
        .fill_group_states
        .keys()
        .copied()
        .collect::<BTreeSet<_>>();
    if checkpoint_fill_groups != expected_fill_groups {
        return fail(
            Some(checkpoint.checkpoint_date),
            dated.len(),
            "fill-group checkpoint state is partial or stale".to_owned(),
        );
    }

    ClassicRefreshCheckpointAudit {
        checkpoint_date: Some(checkpoint.checkpoint_date),
        expected_source_count: expected_keys.len(),
        actual_source_count: dated.len(),
        diagnostic: None,
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
        // Alternate schedules retain their own undated source progress while
        // inactive. Audit the current schedule's exact state without treating
        // those dormant anchors as a duplicate or stale active source.
        .filter(|key| expected_anchor_keys.contains(key))
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    #[serde(default)]
    filler_indices: BTreeMap<EntityId, usize>,
    #[serde(default)]
    filler_sources: BTreeMap<EntityId, ClassicMediaCheckpoint>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    playlist_sources: BTreeMap<String, ClassicPlaylistCheckpoint>,
}

fn classic_anchor_context_version() -> u32 {
    2
}

/// Check that persisted Continue context contains every active playlist's
/// complete cycle state. Legacy flattened collection indices cannot restore it.
#[must_use]
pub fn classic_playlist_continuation_is_complete(
    reference_data: &SchedulingReferenceData,
    context: Option<&str>,
) -> bool {
    if reference_data.playlist_items_by_key.is_empty() {
        return true;
    }
    let Some(context) =
        context.and_then(|value| serde_json::from_str::<ClassicAnchorContext>(value).ok())
    else {
        return false;
    };
    context.version == classic_anchor_context_version()
        && reference_data.playlist_items_by_key.keys().all(|key| {
            context
                .playlist_sources
                .get(key)
                .is_some_and(|source| source.remaining_media_ids.is_some())
        })
}

impl PlayoutSchedulingEngine for ClassicSchedulingEngine {
    fn schedule(
        &self,
        mut request: SchedulingRequest,
    ) -> Result<SchedulingResult, SchedulingError> {
        let mut items = request.reference_data.classic_schedule_items.clone();
        if items.is_empty() {
            return Ok(SchedulingResult::default());
        }

        items.sort_by_key(|item| (item.sort_order, item.id));
        for item in &mut items {
            if item.playback_order == PlaybackOrder::Marathon {
                // MarathonHelper.GetEnumerator returns None for an unsupported
                // grouping mode; PlayoutBuilder then uses Random, not an empty
                // playlist. Change only this build's copy of the schedule item.
                if item.marathon_group_by == ersatzrs_domain::MarathonGroupBy::None {
                    tracing::warn!(
                        schedule_item_id = item.id,
                        "Unable to build marathon enumerator; falling back to random"
                    );
                    item.playback_order = PlaybackOrder::Random;
                    continue;
                }
                let key = item_key(item);
                let media = request
                    .reference_data
                    .media_by_key
                    .get(&key)
                    .cloned()
                    .unwrap_or_default();
                request
                    .reference_data
                    .playlist_items_by_key
                    .insert(key, super::classic_marathon_playlist_items(item, media));
            }
        }
        if request.mode == PlayoutBuildMode::Refresh
            && !request.reference_data.existing_items.is_empty()
        {
            let audit = audit_classic_refresh_checkpoint(
                &request.reference_data,
                request.playout.id,
                request.start,
            );
            if let Some(diagnostic) = audit.diagnostic {
                return Err(SchedulingError::UnsafeClassicRefresh(diagnostic));
            }
        }
        let state = state_for_mode(&request, &items);

        let mut context = ClassicContext::new(request, items, state);
        context.apply_existing_collection_anchors();
        context.restore_checkpoint_sources();
        context.schedule();
        if let Some(error) = context.error.take() {
            return Err(error);
        }
        Ok(context.into_result())
    }
}

const CLASSIC_CHECKPOINT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ClassicCheckpointContext {
    version: u32,
    anchor: PlayoutAnchor,
    schedule_order: Vec<EntityId>,
    fill_group_states: BTreeMap<EntityId, CollectionEnumeratorState>,
    smart_shuffle: SchedulingSmartShuffleState,
    random_rotation_cycle_predecessors: BTreeMap<String, EntityId>,
    sources: BTreeMap<String, ClassicCheckpointSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ClassicCheckpointSource {
    enumerator_state: CollectionEnumeratorState,
    #[serde(skip_serializing_if = "Option::is_none")]
    media: Option<ClassicMediaCheckpoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    playlist: Option<ClassicPlaylistCheckpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct ClassicMediaCheckpoint {
    playback_order: PlaybackOrder,
    current_index: usize,
    ordered_items: Vec<ClassicMediaIdentity>,
}

impl ClassicMediaCheckpoint {
    pub(super) fn is_valid(&self) -> bool {
        self.current_index < self.ordered_items.len().max(1)
    }

    pub(super) fn matches_order(&self, order: PlaybackOrder) -> bool {
        self.playback_order == order
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct ClassicPlaylistCheckpoint {
    index: usize,
    child_index: usize,
    #[serde(default)]
    shuffle_seed: Option<i32>,
    #[serde(default)]
    remaining_media_ids: Option<BTreeSet<EntityId>>,
    children: Vec<ClassicPlaylistChildCheckpoint>,
}

impl ClassicPlaylistCheckpoint {
    pub(super) fn is_valid(&self) -> bool {
        let ids = self
            .children
            .iter()
            .flat_map(|child| child.ordered_items.iter().map(|item| item.media_item_id))
            .collect::<BTreeSet<_>>();
        self.child_index < self.children.len().max(1)
            && self
                .children
                .iter()
                .all(|child| child.current_index < child.ordered_items.len().max(1))
            && self
                .remaining_media_ids
                .as_ref()
                .is_some_and(|remaining| remaining.is_subset(&ids))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ClassicPlaylistChildCheckpoint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    shuffle_seed: Option<i32>,
    collection_key: SchedulingCollectionKey,
    items_taken_from_current: usize,
    current_index: usize,
    ordered_items: Vec<ClassicMediaIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ClassicMediaIdentity {
    media_item_id: EntityId,
    preferred_audio_language_code: Option<String>,
    preferred_audio_title: Option<String>,
    preferred_subtitle_language_code: Option<String>,
    subtitle_mode: Option<ChannelSubtitleMode>,
    pinned_stream_selection: Option<PinnedStreamSelection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    chapter_start: Option<Duration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    chapter_duration: Option<Duration>,
}

impl ClassicSchedulingEngine {
    /// Calculate a fixed start using an explicit UTC clock.
    ///
    /// # Errors
    /// Returns an error if the civil date is outside the clock's supported range.
    pub fn get_start_time_after(
        current_time: DateTime<Utc>,
        schedule_item: &ProgramScheduleItem,
        schedule_fixed_start_time_behavior: FixedStartTimeBehavior,
        state: &PlayoutAnchor,
        is_peek: bool,
    ) -> Result<DateTime<Utc>, SchedulingError> {
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
            &ClassicClock::default(),
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
    retained_playlist_sources: BTreeMap<String, ClassicPlaylistCheckpoint>,
    media_states: BTreeMap<String, CollectionEnumeratorState>,
    collection_anchors: Vec<PlayoutProgramScheduleAnchor>,
    filler_media: BTreeMap<EntityId, MediaEnumerator>,
    fill_group_states: BTreeMap<EntityId, CollectionEnumeratorState>,
    smart_shuffle_state: SchedulingSmartShuffleState,
    random_draw_states: BTreeMap<String, u64>,
    random_rotation_cycle_predecessors: BTreeMap<String, EntityId>,
    restored_checkpoint: Option<ClassicCheckpointContext>,
    generated_dated_anchors: Vec<PlayoutProgramScheduleAnchor>,
    generated_checkpoints: Vec<PlayoutClassicCheckpoint>,
    result: SchedulingResult,
    error: Option<SchedulingError>,
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
        let restored_checkpoint = (request.mode == PlayoutBuildMode::Refresh)
            .then(|| selected_refresh_checkpoint(&request))
            .flatten()
            .map(|(_, context)| context);
        let anchor_context = match request.mode {
            PlayoutBuildMode::Continue => request
                .playout
                .anchor
                .as_ref()
                .and_then(|anchor| anchor.context.as_deref())
                .and_then(|context| serde_json::from_str::<ClassicAnchorContext>(context).ok())
                .unwrap_or_default(),
            PlayoutBuildMode::Refresh => restored_checkpoint
                .as_ref()
                .map(|checkpoint| ClassicAnchorContext {
                    version: classic_anchor_context_version(),
                    smart_shuffle: checkpoint.smart_shuffle.clone(),
                    random_rotation_cycle_predecessors: checkpoint
                        .random_rotation_cycle_predecessors
                        .clone(),
                    ..checkpoint
                        .anchor
                        .context
                        .as_deref()
                        .and_then(|context| {
                            serde_json::from_str::<ClassicAnchorContext>(context).ok()
                        })
                        .unwrap_or_default()
                })
                .unwrap_or_default(),
            PlayoutBuildMode::Reset => ClassicAnchorContext::default(),
        };
        let smart_shuffle_state = anchor_context.smart_shuffle;
        let mut random_rotation_cycle_predecessors =
            anchor_context.random_rotation_cycle_predecessors;
        for item in &schedule_items {
            let key = item_key(item);
            let retained_state = collection_anchors
                .iter()
                .filter(|anchor| {
                    history_key_for_collection_key(&collection_key_for_anchor(anchor)) == key
                })
                .fold(None, |selected, anchor| {
                    if selected
                        .is_none_or(|existing| should_prefer_enumerator_anchor(anchor, existing))
                    {
                        Some(anchor)
                    } else {
                        selected
                    }
                })
                .map(|anchor| anchor.enumerator_state);
            let seed = retained_state.map_or(request.playout.seed, |state| state.seed);
            let started = retained_state.is_some_and(|state| state.started);
            if let Some(playlist_items) = request.reference_data.playlist_items_by_key.get(&key) {
                let mut playlist = PlaylistEnumerator::new(playlist_items.clone());
                let mut prepared_children =
                    Vec::<(SchedulingCollectionKey, MediaEnumerator)>::new();
                for (child_index, child) in playlist.children.iter_mut().enumerate() {
                    if let Some((_, enumerator)) = prepared_children
                        .iter()
                        .find(|(key, _)| *key == child.collection_key)
                    {
                        child.enumerator.clone_from(enumerator);
                        continue;
                    }
                    let order = child.enumerator.playback_order;
                    let child_seed = seed_to_u64(seed).wrapping_add(child_index as u64);
                    if order == PlaybackOrder::Shuffle {
                        child.enumerator.restore_shuffle_seed(
                            seed.wrapping_add(i32::try_from(child_index).unwrap_or_default()),
                        );
                    } else {
                        prepare_enumerator_items(&mut child.enumerator, order, child_seed, None);
                    }
                    if request.reference_data.classic_random_start_point
                        && item.playback_order != PlaybackOrder::Marathon
                        && matches!(
                            order,
                            PlaybackOrder::Chronological
                                | PlaybackOrder::SeasonEpisode
                                | PlaybackOrder::ShuffleInOrder
                        )
                        && !child.enumerator.is_empty()
                    {
                        child.enumerator.set_index(
                            usize::try_from(child_seed % child.enumerator.len() as u64)
                                .unwrap_or_default(),
                        );
                    }
                    prepared_children
                        .push((child.collection_key.clone(), child.enumerator.clone()));
                }
                if item.marathon_shuffle_groups {
                    playlist.shuffle_groups(seed);
                }
                if request.mode == PlayoutBuildMode::Continue {
                    if let Some(checkpoint) = anchor_context.playlist_sources.get(&key) {
                        restore_playlist_checkpoint(&mut playlist, checkpoint);
                    } else if let Some(state) = retained_state.filter(|state| state.started) {
                        let target = usize::try_from(state.index).unwrap_or_default();
                        playlist.replay_to(target);
                    }
                }
                playlists.insert(key.clone(), playlist);
                media_states.insert(
                    key,
                    retained_state.unwrap_or(CollectionEnumeratorState {
                        seed,
                        index: 0,
                        started: false,
                    }),
                );
                continue;
            }
            let mut enumerator = MediaEnumerator::new(
                key.clone(),
                request
                    .reference_data
                    .media_items_for_source(&super::SchedulingContentSource::direct(&key)),
            )
            .with_playback_order(item.playback_order);
            if started || restored_checkpoint.is_some() {
                media_states.insert(
                    key.clone(),
                    retained_state.unwrap_or(CollectionEnumeratorState {
                        seed,
                        index: 0,
                        started: false,
                    }),
                );
            } else {
                prepare_enumerator_items(
                    &mut enumerator,
                    item.playback_order,
                    seed_to_u64(seed),
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
                    let index = usize::try_from(seed_to_u64(seed) % enumerator.len() as u64)
                        .unwrap_or_default();
                    enumerator.set_index(index);
                    media_states.insert(
                        key.clone(),
                        CollectionEnumeratorState {
                            seed,
                            index: i32::try_from(index).unwrap_or_default(),
                            started: false,
                        },
                    );
                } else {
                    media_states.insert(
                        key.clone(),
                        CollectionEnumeratorState {
                            seed,
                            index: 0,
                            started: false,
                        },
                    );
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
                let mut enumerator = MediaEnumerator::new(
                    format!("classic-filler:{id}"),
                    preset.media_items.clone(),
                )
                .with_playback_order(PlaybackOrder::Chronological);
                if let Some(source) = anchor_context.filler_sources.get(id) {
                    restore_media_checkpoint(&mut enumerator, source);
                } else {
                    enumerator.set_index(
                        anchor_context
                            .filler_indices
                            .get(id)
                            .copied()
                            .unwrap_or_default(),
                    );
                }
                (*id, enumerator)
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
                        started: false,
                    });
            }
        }
        if let Some(checkpoint) = &restored_checkpoint {
            fill_group_states.clone_from(&checkpoint.fill_group_states);
        }

        let mut context = Self {
            request,
            schedule_items,
            schedule_order: Vec::new(),
            state,
            media,
            playlists,
            retained_playlist_sources: anchor_context.playlist_sources,
            media_states,
            collection_anchors,
            filler_media,
            fill_group_states,
            smart_shuffle_state,
            random_draw_states,
            random_rotation_cycle_predecessors,
            restored_checkpoint,
            generated_dated_anchors: Vec::new(),
            generated_checkpoints: Vec::new(),
            error: None,
            result: SchedulingResult {
                remove_before: Some(remove_before),
                ..SchedulingResult::default()
            },
        };
        context.refresh_schedule_order();
        context.restore_checkpoint_schedule_order();
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
            if !anchor.enumerator_state.started {
                continue;
            }
            let key = history_key_for_collection_key(&collection_key_for_anchor(anchor));
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
            } else if self.playlists.contains_key(&key) {
                self.media_states.insert(key, anchor.enumerator_state);
            }
        }
    }

    fn restore_checkpoint_sources(&mut self) {
        let Some(checkpoint) = self.restored_checkpoint.clone() else {
            return;
        };

        for (key, source) in checkpoint.sources {
            let mut state = source.enumerator_state;
            if let (Some(enumerator), Some(media_checkpoint)) =
                (self.media.get_mut(&key), source.media.as_ref())
            {
                let membership_changed = restore_media_checkpoint(enumerator, media_checkpoint);
                if enumerator.playback_order == PlaybackOrder::Random {
                    let draw_index = usize::try_from(state.index).unwrap_or_default();
                    let random_state =
                        select_random_draw(enumerator, seed_to_u64(state.seed), draw_index);
                    self.random_draw_states.insert(key.clone(), random_state);
                } else if membership_changed {
                    state.index = i32::try_from(enumerator.current_index()).unwrap_or_default();
                }
            }
            if let (Some(playlist), Some(playlist_checkpoint)) =
                (self.playlists.get_mut(&key), source.playlist.as_ref())
            {
                restore_playlist_checkpoint(playlist, playlist_checkpoint);
                state.index = i32::try_from(playlist.index).unwrap_or_default();
            }
            self.media_states.insert(key, state);
        }
    }

    fn restore_checkpoint_schedule_order(&mut self) {
        let Some(checkpoint) = &self.restored_checkpoint else {
            return;
        };
        let restored = checkpoint
            .schedule_order
            .iter()
            .filter_map(|item_id| {
                self.schedule_items
                    .iter()
                    .position(|item| item.id == *item_id)
            })
            .collect::<Vec<_>>();
        if restored.len() == self.schedule_items.len() {
            self.schedule_order = restored;
        }
    }

    fn schedule(&mut self) {
        let final_finish = self.request.finish;
        self.capture_checkpoint();
        let mut chunk_finish = midnight(self.state.current_time) + Duration::days(1);
        while chunk_finish < final_finish && self.error.is_none() {
            self.request.finish = chunk_finish;
            self.schedule_until_finish();
            self.capture_checkpoint();
            chunk_finish += Duration::days(1);
        }
        self.request.finish = final_finish;
        self.schedule_until_finish();
    }

    fn schedule_until_finish(&mut self) {
        let mut time_count = BTreeMap::<DateTime<Utc>, usize>::new();
        while self.state.current_time < self.request.finish && self.error.is_none() {
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
            let before_count = self.result.added_items.len();
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

            // A pass can extend beyond its nominal midnight (and the final
            // partial pass has no following pass boundary). Freeze progress
            // immediately after the block that first crosses the day seam.
            if self.result.added_items.len() > before_count
                && self.state.current_time.date_naive() > before.date_naive()
            {
                self.capture_checkpoint();
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
            let filler_start = self.filler_start_after(next_schedule_item);
            self.add_tail_and_fallback(schedule_item, filler_start);
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

        let filler_start = self.filler_start_after(next_schedule_item);
        self.add_tail_and_fallback(schedule_item, filler_start);
        self.state.current_time = self
            .state
            .current_time
            .min(self.filler_start_after(next_schedule_item));
        self.decrement_guide_group_if_advanced(start_len, schedule_item);
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
                // Preserve the real cursor for tail/fallback generation below.
                // Advancing it first hides the remaining duration-block gap.
                if !matches!(
                    schedule_item.tail_mode,
                    TailMode::Offline | TailMode::Filler
                ) {
                    self.state.current_time = finish;
                    self.state.duration_finish = None;
                    self.advance_schedule_item();
                }
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
        self.decrement_guide_group_if_advanced(start_len, schedule_item);
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
                    &self.request.reference_data.classic_clock,
                )
                .unwrap_or_else(|error| {
                    self.error = Some(error);
                    self.request.finish
                })
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
        self.decrement_guide_group_if_advanced(start_len, schedule_item);
        let filler_start = self.filler_start_after(next_schedule_item);
        self.add_tail_and_fallback(schedule_item, filler_start);
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
        let primary_finish = items
            .iter()
            .filter(|item| {
                item.scheduling_context
                    .as_deref()
                    .and_then(filler_id_from_context)
                    .is_none()
            })
            .map(|item| item.finish)
            .max();
        let mut recorded_primary = false;
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
            if item
                .scheduling_context
                .as_deref()
                .and_then(filler_id_from_context)
                .is_some()
            {
                self.record_smart_shuffle_item(schedule_item, &item);
            } else if !recorded_primary {
                // Chapter slices are one selection unit, not repeated selections.
                let mut unit = item.clone();
                unit.finish = primary_finish.unwrap_or(item.finish);
                self.record_smart_shuffle_item(schedule_item, &unit);
                recorded_primary = true;
            }
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
        let mut chapters = self
            .request
            .reference_data
            .classic_chapters_by_media_id
            .get(&media_item.chapter_key(schedule_item.pinned_stream_selection))
            .cloned()
            .unwrap_or_default();
        if chapters.len() > 1 {
            if let Some(preset) = schedule_item
                .mid_roll_filler_id
                .and_then(|id| self.request.reference_data.classic_filler_presets.get(&id))
            {
                if let Ok(filtered) = chapter_expression::filter_chapters(
                    preset.expression.as_deref().unwrap_or_default(),
                    &chapters,
                    media_item.duration,
                ) {
                    chapters = filtered;
                } else {
                    self.error = Some(SchedulingError::InvalidClassicFillerExpression(
                        schedule_item.id,
                    ));
                    return None;
                }
                if chapters.len() <= 1 && preset.filler_mode != FillerMode::Pad {
                    return self.build_unsplit_block(
                        schedule_item,
                        media_item,
                        start,
                        guide_finish,
                        scheduler,
                        false,
                    );
                }
            }
        }
        if schedule_item.mid_roll_filler_id.is_some() && chapters.len() > 1 {
            return self.build_chapter_block(
                schedule_item,
                media_item,
                &chapters,
                start,
                guide_finish,
                scheduler,
            );
        }
        self.build_unsplit_block(
            schedule_item,
            media_item,
            start,
            guide_finish,
            scheduler,
            true,
        )
    }

    fn build_unsplit_block(
        &mut self,
        schedule_item: &ProgramScheduleItem,
        media_item: &SchedulingMediaItem,
        start: DateTime<Utc>,
        guide_finish: Option<DateTime<Utc>>,
        scheduler: &'static str,
        include_mid_roll: bool,
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
            schedule_item
                .mid_roll_filler_id
                .filter(|_| include_mid_roll),
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
            let kind = self.effective_pad_kind(schedule_item, media_item, pad_filler_id)?;
            let pad = self.add_pad_filler(schedule_item, pad_filler_id, start, current, kind);
            if kind == FillerKind::PreRoll {
                items.splice(0..0, pad);
            } else {
                items.extend(pad);
            }
            current = start;
            for item in &mut items {
                let duration = item.finish - item.start;
                item.start = current;
                item.finish = current + duration;
                item.out_point = item.in_point + duration;
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
        let mut consecutive_skips = 0usize;
        let candidate_count = self
            .filler_media
            .get(&filler_id)
            .map_or(0, MediaEnumerator::len);
        if candidate_count == 0 {
            return items;
        }
        while remaining > Duration::zero() {
            if consecutive_skips >= candidate_count {
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
                consecutive_skips += 1;
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
            consecutive_skips = 0;
            remaining -= item.finish - item.start;
            current = item.finish;
            items.push(item);
            self.advance_filler(filler_id);
        }
        items
    }

    fn effective_pad_kind(
        &mut self,
        schedule: &ProgramScheduleItem,
        media: &SchedulingMediaItem,
        filler_id: EntityId,
    ) -> Option<FillerKind> {
        let preset = self
            .request
            .reference_data
            .classic_filler_presets
            .get(&filler_id)?;
        let chapters = schedule.mid_roll_filler_id.and_then(|_| {
            self.request
                .reference_data
                .classic_chapters_by_media_id
                .get(&media.chapter_key(schedule.pinned_stream_selection))
        });
        if let Some(chapters) = chapters.filter(|chapters| chapters.len() > 1) {
            match chapter_expression::filter_chapters(
                preset.expression.as_deref().unwrap_or_default(),
                chapters,
                media.duration,
            ) {
                Ok(filtered) if filtered.len() <= 1 => return Some(FillerKind::PostRoll),
                Ok(_) => {}
                Err(_) => {
                    self.error = Some(SchedulingError::InvalidClassicFillerExpression(schedule.id));
                    return None;
                }
            }
        }
        Some(preset.filler_kind)
    }

    fn add_pad_filler(
        &mut self,
        schedule_item: &ProgramScheduleItem,
        filler_id: EntityId,
        block_start: DateTime<Utc>,
        block_finish: DateTime<Utc>,
        kind: FillerKind,
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
        let remaining = mid_roll::pad_budget(
            block_start,
            block_finish - block_start,
            Some(nearest_minute),
        );
        let target = block_finish + remaining;
        if remaining <= Duration::zero() {
            return Vec::new();
        }
        let mut items = self.add_duration_filler(
            schedule_item,
            filler_id,
            block_finish,
            Self::effective_filler_kind(schedule_item, kind),
            remaining,
            preset.allow_watermarks,
        );
        let current = items.last().map_or(block_finish, |item| item.finish);
        let mut fallback = self.fallback_items(schedule_item, current, target);
        if kind == FillerKind::PreRoll {
            fallback.extend(items);
            fallback
        } else {
            items.extend(fallback);
            items
        }
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
        let items = self.fallback_items(schedule_item, self.state.current_time, until);
        self.append_filler_items(schedule_item, items);
    }

    // Upstream #2867: consume complete clips, then trim only the final clip.
    // An invalid-duration collection must terminate without manufacturing time.
    fn fallback_items(
        &mut self,
        schedule_item: &ProgramScheduleItem,
        start: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> Vec<PlayoutItem> {
        let Some(id) = schedule_item.fallback_filler_id else {
            return Vec::new();
        };
        let allow_watermarks = self
            .request
            .reference_data
            .classic_filler_presets
            .get(&id)
            .is_none_or(|preset| preset.allow_watermarks);
        let count = self
            .request
            .reference_data
            .classic_filler_presets
            .get(&id)
            .map_or(0, |preset| preset.media_items.len());
        let mut skipped = 0;
        let mut current = start;
        let mut result = Vec::new();
        while current < until {
            let Some(mut item) = self.current_filler_item(
                schedule_item,
                id,
                current,
                FillerKind::Fallback,
                allow_watermarks,
            ) else {
                break;
            };
            self.advance_filler(id);
            if item.finish <= current {
                skipped += 1;
                if skipped >= count {
                    self.result.warnings.push(format!(
                        "classic fallback filler {id} contains no positive-duration items"
                    ));
                    break;
                }
                continue;
            }
            skipped = 0;
            item.finish = item.finish.min(until);
            item.out_point = item.in_point + (item.finish - current);
            current = item.finish;
            result.push(item);
        }
        result
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
        if self.request.mode == PlayoutBuildMode::Refresh {
            self.result.retain_through = self
                .restored_checkpoint
                .as_ref()
                .map(|checkpoint| checkpoint.anchor.next_start);
        }
        let current_item = self.schedule_items[self.current_schedule_index()].clone();
        let mut next_start = self.start_time_after(&current_item, false);
        if let Some(last_finish) = self.result.added_items.iter().map(|item| item.finish).max() {
            next_start = next_start.max(last_finish);
        }
        self.result.anchor = Some(self.playout_anchor(next_start));
        self.result.program_schedule_anchors = self.program_schedule_anchors();
        self.result.classic_checkpoints = self.classic_checkpoints();
        self.result.fill_group_indices = self.fill_group_indices();
        self.result.history_to_remove.extend(cleanup_history(
            &self.request.reference_data.existing_history,
            self.request.start,
            self.request.reference_data.max_playout_offset,
        ));
        self.result
    }

    fn playout_anchor(&self, next_start: DateTime<Utc>) -> PlayoutAnchor {
        let next_start = truncate_to_persisted_precision(next_start);
        PlayoutAnchor {
            schedule_items_enumerator_state: self.state.schedule_state,
            next_start,
            multiple_remaining: self.state.multiple_remaining,
            duration_finish: self
                .state
                .duration_finish
                .map(truncate_to_persisted_precision)
                .filter(|finish| *finish > next_start),
            in_flood: self.state.in_flood,
            in_duration_filler: self.state.in_duration_filler,
            next_guide_group: self.state.next_guide_group,
            context: (self.request.reference_data.has_active_smart_shuffle()
                || !self.random_rotation_cycle_predecessors.is_empty()
                || !self.playlists.is_empty()
                || !self.retained_playlist_sources.is_empty()
                || !self.filler_media.is_empty())
            .then(|| {
                let mut playlist_sources = self.retained_playlist_sources.clone();
                playlist_sources.extend(
                    self.playlists
                        .iter()
                        .map(|(key, playlist)| (key.clone(), playlist_checkpoint(playlist))),
                );
                serde_json::to_string(&ClassicAnchorContext {
                    version: classic_anchor_context_version(),
                    playlist_sources,
                    filler_indices: self
                        .filler_media
                        .iter()
                        .map(|(id, enumerator)| (*id, enumerator.current_index()))
                        .collect(),
                    filler_sources: self
                        .filler_media
                        .iter()
                        .map(|(id, enumerator)| {
                            (
                                *id,
                                ClassicMediaCheckpoint {
                                    playback_order: enumerator.playback_order,
                                    current_index: enumerator.current_index(),
                                    ordered_items: enumerator
                                        .items
                                        .iter()
                                        .map(media_identity)
                                        .collect(),
                                },
                            )
                        })
                        .collect(),
                    smart_shuffle: self.persisted_smart_shuffle_state(),
                    random_rotation_cycle_predecessors: self
                        .random_rotation_cycle_predecessors
                        .clone(),
                })
                .unwrap_or_default()
            }),
            ..PlayoutAnchor::default()
        }
    }

    fn capture_checkpoint(&mut self) {
        let checkpoint_date = truncate_to_persisted_precision(
            self.result
                .added_items
                .iter()
                .map(|item| item.finish)
                .max()
                .unwrap_or(self.state.current_time)
                .max(self.state.current_time),
        );
        if self.generated_checkpoints.iter().any(|checkpoint| {
            checkpoint.checkpoint_date.date_naive() == checkpoint_date.date_naive()
        }) || (self.request.mode == PlayoutBuildMode::Continue
            && self
                .request
                .reference_data
                .existing_classic_checkpoints
                .iter()
                .any(|checkpoint| {
                    checkpoint.playout_id == self.request.playout.id
                        && checkpoint.checkpoint_date.date_naive() == checkpoint_date.date_naive()
                }))
        {
            return;
        }

        let context = ClassicCheckpointContext {
            version: CLASSIC_CHECKPOINT_VERSION,
            anchor: self.playout_anchor(checkpoint_date),
            schedule_order: self
                .schedule_order
                .iter()
                .map(|index| self.schedule_items[*index].id)
                .collect(),
            fill_group_states: self.fill_group_states.clone(),
            smart_shuffle: self.persisted_smart_shuffle_state(),
            random_rotation_cycle_predecessors: self.random_rotation_cycle_predecessors.clone(),
            sources: self.checkpoint_sources(),
        };
        self.generated_checkpoints.push(PlayoutClassicCheckpoint {
            playout_id: self.request.playout.id,
            checkpoint_date,
            context: serde_json::to_string(&context).unwrap_or_default(),
        });
        self.generated_dated_anchors
            .extend(self.program_schedule_anchors_at(Some(checkpoint_date)));
    }

    fn persisted_smart_shuffle_state(&self) -> SchedulingSmartShuffleState {
        let mut state = self.smart_shuffle_state.clone();
        state.normalize_timestamps(truncate_to_persisted_precision);
        state
    }

    fn checkpoint_sources(&self) -> BTreeMap<String, ClassicCheckpointSource> {
        let mut result = BTreeMap::new();
        for schedule_item in &self.schedule_items {
            let key = item_key(schedule_item);
            if result.contains_key(&key) {
                continue;
            }
            let enumerator_state =
                self.media_states
                    .get(&key)
                    .copied()
                    .unwrap_or(CollectionEnumeratorState {
                        seed: self.request.playout.seed,
                        index: 0,
                        started: false,
                    });
            let media = self
                .media
                .get(&key)
                .map(|enumerator| ClassicMediaCheckpoint {
                    playback_order: enumerator.playback_order,
                    current_index: enumerator.current_index(),
                    ordered_items: enumerator.items.iter().map(media_identity).collect(),
                });
            let playlist = self.playlists.get(&key).map(playlist_checkpoint);
            result.insert(
                key,
                ClassicCheckpointSource {
                    enumerator_state,
                    media,
                    playlist,
                },
            );
        }
        result
    }

    fn program_schedule_anchors(&self) -> Vec<PlayoutProgramScheduleAnchor> {
        let mut result = BTreeMap::new();
        let active_keys = self
            .schedule_items
            .iter()
            .map(item_key)
            .collect::<BTreeSet<_>>();
        for anchor in self.collection_anchors.iter().filter(|anchor| {
            anchor.anchor_date.is_none()
                && !active_keys.contains(&history_key_for_collection_key(
                    &collection_key_for_anchor(anchor),
                ))
        }) {
            let key = history_key_for_collection_key(&collection_key_for_anchor(anchor));
            result.insert((anchor.anchor_date, key), anchor.clone());
        }
        for anchor in self
            .collection_anchors
            .iter()
            .filter(|anchor| anchor.anchor_date.is_some())
        {
            let key = history_key_for_collection_key(&collection_key_for_anchor(anchor));
            result.insert((anchor.anchor_date, key), anchor.clone());
        }
        for anchor in self.program_schedule_anchors_at(None) {
            let key = history_key_for_collection_key(&collection_key_for_anchor(&anchor));
            result.insert((anchor.anchor_date, key), anchor);
        }
        for anchor in &self.generated_dated_anchors {
            let key = history_key_for_collection_key(&collection_key_for_anchor(anchor));
            result.insert((anchor.anchor_date, key), anchor.clone());
        }
        result.into_values().collect()
    }

    fn program_schedule_anchors_at(
        &self,
        anchor_date: Option<DateTime<Utc>>,
    ) -> Vec<PlayoutProgramScheduleAnchor> {
        let mut result = Vec::new();
        let mut seen_keys = BTreeSet::new();
        for schedule_item in &self.schedule_items {
            let key = collection_key(schedule_item);
            let key_string = history_key_for_collection_key(&key);
            if !seen_keys.insert(key_string.clone()) {
                continue;
            }
            let existing = self.collection_anchors.iter().find(|anchor| {
                anchor.anchor_date == anchor_date && anchor_matches_key(anchor, &key)
            });
            // Match EF entity semantics: zero means new and lets the database
            // allocate the globally unique primary key. Existing entities keep
            // their persisted identity.
            let id = existing.map_or(0, |anchor| anchor.id);
            result.push(PlayoutProgramScheduleAnchor {
                id,
                playout_id: self.request.playout.id,
                anchor_date,
                collection_type: key.collection_type,
                collection_id: key.collection_id,
                multi_collection_id: key.multi_collection_id,
                smart_collection_id: key.smart_collection_id,
                rerun_collection_id: key.rerun_collection_id,
                media_item_id: key.media_item_id,
                playlist_id: key.playlist_id,
                search_query: key.search_query,
                fake_collection_key: key.fake_collection_key,
                enumerator_state: self.enumerator_state_for_key(&key_string),
            });
        }
        result
    }

    fn enumerator_state_for_key(&self, key: &str) -> CollectionEnumeratorState {
        let mut state = self
            .media_states
            .get(key)
            .copied()
            .unwrap_or(CollectionEnumeratorState {
                seed: self.request.playout.seed,
                index: 0,
                started: false,
            });
        if let Some(playlist) = self.playlists.get(key) {
            state.index = i32::try_from(playlist.index).unwrap_or_default();
            if let Some(seed) = playlist.shuffle_seed {
                state.seed = seed;
            }
        } else if let Some(enumerator) = self.media.get(key) {
            if enumerator.playback_order != PlaybackOrder::Random {
                state.index = i32::try_from(enumerator.current_index()).unwrap_or_default();
            }
        }
        state
    }

    fn classic_checkpoints(&self) -> Vec<PlayoutClassicCheckpoint> {
        let mut checkpoints = BTreeMap::new();
        if self.request.mode == PlayoutBuildMode::Continue {
            let start_day = midnight(self.request.start);
            for checkpoint in &self.request.reference_data.existing_classic_checkpoints {
                if checkpoint.playout_id == self.request.playout.id
                    && checkpoint.checkpoint_date >= start_day
                {
                    checkpoints.insert(checkpoint.checkpoint_date, checkpoint.clone());
                }
            }
        }
        for checkpoint in &self.generated_checkpoints {
            checkpoints.insert(checkpoint.checkpoint_date, checkpoint.clone());
        }
        checkpoints.into_values().collect()
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
                        started: false,
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
        self.state.schedule_state.started = true;
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
                if let Some(seed) = playlist.shuffle_seed {
                    state.seed = seed;
                }
                state.started = true;
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
                            started: false,
                        });
                let draw_index = usize::try_from(media_state.index).unwrap_or_default();
                let random_state = self.random_draw_states.entry(key).or_insert_with(|| {
                    random_state_for_draw(seed_to_u64(media_state.seed), draw_index)
                });
                *random_state = next_random_state(*random_state);
                enumerator.set_index(random_index(*random_state, enumerator.len()));
                media_state.index = media_state.index.saturating_add(1);
                media_state.started = true;
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
                state.started = true;
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
        &mut self,
        schedule_item: &ProgramScheduleItem,
        is_peek: bool,
    ) -> DateTime<Utc> {
        get_start_time_after(
            self.state.current_time,
            schedule_item,
            self.schedule_fixed_start_time_behavior(),
            &self.state,
            is_peek,
            &self.request.reference_data.classic_clock,
        )
        .unwrap_or_else(|error| {
            self.error = Some(error);
            self.request.finish
        })
    }

    fn filler_start_after(&mut self, schedule_item: &ProgramScheduleItem) -> DateTime<Utc> {
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
        state.started = true;
        if schedule_item.fill_with_group_mode == FillWithGroupMode::FillWithShuffledGroups {
            state.seed = next_i32_seed(state.seed);
        }
    }

    fn decrement_guide_group_if_advanced(
        &mut self,
        start_len: usize,
        schedule_item: &ProgramScheduleItem,
    ) {
        let groups = self.result.added_items[start_len..]
            .iter()
            .map(|item| item.guide_group)
            .collect::<BTreeSet<_>>();
        if groups.len() > 1
            || (!groups.is_empty()
                && schedule_item
                    .custom_title
                    .as_deref()
                    .is_none_or(str::is_empty))
        {
            self.state.next_guide_group -= 1;
        }
    }
}

fn start_state(request: &SchedulingRequest, _items: &[ProgramScheduleItem]) -> ClassicState {
    let index = 0;

    ClassicState {
        current_time: request.start,
        schedule_state: CollectionEnumeratorState {
            seed: request.playout.seed,
            index,
            started: false,
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
    selected_refresh_checkpoint(request).map_or_else(
        || start_state(request, items),
        |(_, checkpoint)| checkpoint_state(&checkpoint.anchor),
    )
}

fn checkpoint_state(anchor: &PlayoutAnchor) -> ClassicState {
    ClassicState {
        current_time: anchor.next_start,
        schedule_state: anchor.schedule_items_enumerator_state,
        multiple_remaining: anchor.multiple_remaining,
        duration_finish: anchor.duration_finish,
        in_flood: anchor.in_flood,
        in_duration_filler: anchor.in_duration_filler,
        next_guide_group: anchor.next_guide_group.max(1),
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

fn retained_collection_anchors(request: &SchedulingRequest) -> Vec<PlayoutProgramScheduleAnchor> {
    match request.mode {
        PlayoutBuildMode::Continue => retained_continue_anchors(request),
        PlayoutBuildMode::Refresh => retained_refresh_anchors(request),
        PlayoutBuildMode::Reset => Vec::new(),
    }
}

fn selected_refresh_checkpoint(
    request: &SchedulingRequest,
) -> Option<(DateTime<Utc>, ClassicCheckpointContext)> {
    let start_day = midnight(request.start);
    let next_day = start_day + Duration::days(1);
    request
        .reference_data
        .existing_classic_checkpoints
        .iter()
        .filter(|checkpoint| {
            checkpoint.playout_id == request.playout.id
                && checkpoint.checkpoint_date >= start_day
                && checkpoint.checkpoint_date < next_day
        })
        .filter_map(|checkpoint| {
            let context =
                serde_json::from_str::<ClassicCheckpointContext>(&checkpoint.context).ok()?;
            (context.version == CLASSIC_CHECKPOINT_VERSION
                && context.anchor.next_start == checkpoint.checkpoint_date)
                .then_some((checkpoint.checkpoint_date, context))
        })
        .min_by_key(|(checkpoint_date, _)| *checkpoint_date)
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
    clock: &ClassicClock,
) -> Result<DateTime<Utc>, SchedulingError> {
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
        let (result, tomorrow) = clock.occurrences(start_time, item_start_time)?;
        let behavior = schedule_item
            .fixed_start_time_behavior
            .unwrap_or(schedule_fixed_start_time_behavior);
        match behavior {
            FixedStartTimeBehavior::Flexible => {
                if is_peek && state.in_flood && start_time > result {
                    start_time = tomorrow;
                } else {
                    let next = if result >= start_time {
                        result
                    } else {
                        tomorrow
                    };
                    if next - start_time <= Duration::hours(12) {
                        start_time = next;
                    }
                }
            }
            FixedStartTimeBehavior::Strict => {
                let local = clock.local_time(start_time)?;
                let time_of_day = Duration::seconds(i64::from(local.num_seconds_from_midnight()))
                    + Duration::nanoseconds(i64::from(local.nanosecond()));
                start_time = if time_of_day > item_start_time {
                    result + Duration::days(1)
                } else {
                    result
                };
            }
        }
    }

    Ok(start_time)
}

fn midnight(value: DateTime<Utc>) -> DateTime<Utc> {
    value
        - Duration::hours(i64::from(value.hour()))
        - Duration::minutes(i64::from(value.minute()))
        - Duration::seconds(i64::from(value.second()))
        - Duration::nanoseconds(i64::from(value.nanosecond()))
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
        rerun_collection_id: item.rerun_collection_id,
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

fn media_identity(item: &SchedulingMediaItem) -> ClassicMediaIdentity {
    ClassicMediaIdentity {
        media_item_id: item.id,
        preferred_audio_language_code: item.preferred_audio_language_code.clone(),
        preferred_audio_title: item.preferred_audio_title.clone(),
        preferred_subtitle_language_code: item.preferred_subtitle_language_code.clone(),
        subtitle_mode: item.subtitle_mode,
        pinned_stream_selection: item.pinned_stream_selection,
        chapter_start: item.chapter.as_ref().map(|chapter| chapter.start),
        chapter_duration: item.chapter.as_ref().map(|_| item.duration),
    }
}

fn take_matching_media_item(
    available: &mut [Option<SchedulingMediaItem>],
    identity: &ClassicMediaIdentity,
) -> Option<SchedulingMediaItem> {
    let index = available.iter().position(|candidate| {
        candidate
            .as_ref()
            .is_some_and(|item| media_identity(item) == *identity)
    })?;
    available[index].take()
}

/// Restore the exact in-cycle ordering when membership is unchanged. When a
/// scan added or removed members, keep every unconsumed surviving item ahead
/// of newly-added items and already-consumed items so a shifted raw index can
/// never replay a different shuffled row.
pub(super) fn restore_media_checkpoint(
    enumerator: &mut MediaEnumerator,
    checkpoint: &ClassicMediaCheckpoint,
) -> bool {
    if enumerator.playback_order != checkpoint.playback_order {
        return true;
    }

    let current_items = std::mem::take(&mut enumerator.items);
    let mut exact_available = current_items.iter().cloned().map(Some).collect::<Vec<_>>();
    let exact = checkpoint
        .ordered_items
        .iter()
        .filter_map(|identity| take_matching_media_item(&mut exact_available, identity))
        .collect::<Vec<_>>();
    let same_membership = exact.len() == checkpoint.ordered_items.len()
        && exact.len() == current_items.len()
        && exact_available.iter().all(Option::is_none);
    if same_membership {
        enumerator.items = exact;
        enumerator.set_index(checkpoint.current_index);
        return false;
    }

    let mut available = current_items.into_iter().map(Some).collect::<Vec<_>>();
    if checkpoint.playback_order == PlaybackOrder::Random {
        let mut restored = checkpoint
            .ordered_items
            .iter()
            .filter_map(|identity| take_matching_media_item(&mut available, identity))
            .collect::<Vec<_>>();
        restored.extend(available.into_iter().flatten());
        enumerator.items = restored;
        enumerator.set_index(0);
        return true;
    }

    let split = checkpoint.current_index.min(checkpoint.ordered_items.len());
    let mut remaining = checkpoint.ordered_items[split..]
        .iter()
        .filter_map(|identity| take_matching_media_item(&mut available, identity))
        .collect::<Vec<_>>();
    let consumed = checkpoint.ordered_items[..split]
        .iter()
        .filter_map(|identity| take_matching_media_item(&mut available, identity))
        .collect::<Vec<_>>();
    remaining.extend(available.into_iter().flatten());
    remaining.extend(consumed);
    enumerator.items = remaining;
    enumerator.set_index(0);
    true
}

pub(super) fn media_checkpoint(enumerator: &MediaEnumerator) -> ClassicMediaCheckpoint {
    ClassicMediaCheckpoint {
        playback_order: enumerator.playback_order,
        current_index: enumerator.current_index(),
        ordered_items: enumerator.items.iter().map(media_identity).collect(),
    }
}

pub(super) fn playlist_checkpoint(playlist: &PlaylistEnumerator) -> ClassicPlaylistCheckpoint {
    ClassicPlaylistCheckpoint {
        index: playlist.index,
        child_index: playlist.child_index,
        shuffle_seed: playlist.shuffle_seed,
        remaining_media_ids: Some(playlist.remaining_media_ids.clone()),
        children: playlist
            .children
            .iter()
            .map(|child| ClassicPlaylistChildCheckpoint {
                shuffle_seed: child.enumerator.shuffle_seed,
                collection_key: child.collection_key.clone(),
                items_taken_from_current: child.items_taken_from_current,
                current_index: child.enumerator.current_index(),
                ordered_items: child.enumerator.items.iter().map(media_identity).collect(),
            })
            .collect(),
    }
}

pub(super) fn restore_playlist_checkpoint(
    playlist: &mut PlaylistEnumerator,
    checkpoint: &ClassicPlaylistCheckpoint,
) {
    if playlist.shuffle_seed.is_some() {
        playlist.shuffle_seed = checkpoint.shuffle_seed.or(playlist.shuffle_seed);
    }
    let old_media_ids = checkpoint
        .children
        .iter()
        .flat_map(|child| child.ordered_items.iter().map(|item| item.media_item_id))
        .collect::<BTreeSet<_>>();
    if let Some(remaining) = &checkpoint.remaining_media_ids {
        playlist.remaining_media_ids = remaining
            .intersection(&playlist.all_media_ids)
            .copied()
            .chain(playlist.all_media_ids.difference(&old_media_ids).copied())
            .collect();
    }
    let mut available = std::mem::take(&mut playlist.children)
        .into_iter()
        .map(Some)
        .collect::<Vec<_>>();
    let mut children = Vec::new();
    let mut restored_child_index = None;

    for (checkpoint_index, child_checkpoint) in checkpoint.children.iter().enumerate() {
        let Some(index) = available.iter().position(|candidate| {
            candidate
                .as_ref()
                .is_some_and(|child| child.collection_key == child_checkpoint.collection_key)
        }) else {
            continue;
        };
        let Some(mut child) = available[index].take() else {
            continue;
        };
        let media_checkpoint = ClassicMediaCheckpoint {
            playback_order: child.enumerator.playback_order,
            current_index: child_checkpoint.current_index,
            ordered_items: child_checkpoint.ordered_items.clone(),
        };
        restore_media_checkpoint(&mut child.enumerator, &media_checkpoint);
        child.enumerator.shuffle_seed = child_checkpoint
            .shuffle_seed
            .or(child.enumerator.shuffle_seed);
        child.items_taken_from_current = child_checkpoint.items_taken_from_current;
        if checkpoint_index == checkpoint.child_index {
            restored_child_index = Some(children.len());
        }
        children.push(child);
    }
    children.extend(available.into_iter().flatten());
    playlist.children = children;
    playlist.index = checkpoint.index;
    playlist.child_index = if playlist.children.is_empty() {
        0
    } else {
        restored_child_index.unwrap_or_default() % playlist.children.len()
    };
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
        && anchor.rerun_collection_id == key.rerun_collection_id
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

pub(super) fn next_i32_seed(seed: i32) -> i32 {
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

    fn started_state(seed: i32, index: i32) -> CollectionEnumeratorState {
        CollectionEnumeratorState {
            seed,
            index,
            started: true,
        }
    }

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

        assert_eq!(
            result
                .program_schedule_anchors
                .iter()
                .filter(|anchor| anchor.anchor_date.is_none())
                .count(),
            1
        );
        assert_eq!(
            result
                .program_schedule_anchors
                .iter()
                .filter(|anchor| anchor.anchor_date.is_some())
                .count(),
            1
        );
        assert!(result
            .program_schedule_anchors
            .iter()
            .all(|anchor| anchor.id == 0));
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
        )
        .unwrap();

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
        let mut live = SchedulingMediaItem::new(101, Duration::zero());
        live.liveness = Some(ersatzrs_domain::RemoteStreamLiveness::Live);
        reference_data.media_by_key.insert(key, vec![live]);

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
    fn classic_playlist_random_start_point_applies_to_fresh_children() {
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap();
        let schedule_item = ProgramScheduleItem {
            collection_type: CollectionType::Playlist,
            collection_id: None,
            playlist_id: Some(99),
            playout_mode: PlayoutMode::Flood,
            ..schedule_item(1, PlayoutMode::Flood, 1, None)
        };
        let key = item_key(&schedule_item);
        let build = |playback_order, random_start_point, seed, retained_started: Option<bool>| {
            ClassicSchedulingEngine
                .schedule(SchedulingRequest {
                    playout: Playout {
                        id: 50,
                        schedule_kind: PlayoutScheduleKind::Classic,
                        seed,
                        ..Playout::default()
                    },
                    start,
                    finish: start + Duration::minutes(10),
                    mode: if retained_started.is_some() {
                        PlayoutBuildMode::Continue
                    } else {
                        PlayoutBuildMode::Reset
                    },
                    reference_data: SchedulingReferenceData {
                        classic_schedule_items: vec![schedule_item.clone()],
                        classic_random_start_point: random_start_point,
                        existing_program_schedule_anchors: if let Some(started) = retained_started {
                            vec![PlayoutProgramScheduleAnchor {
                                playout_id: 50,
                                collection_type: CollectionType::Playlist,
                                playlist_id: Some(99),
                                enumerator_state: CollectionEnumeratorState {
                                    seed,
                                    index: 0,
                                    started,
                                },
                                ..PlayoutProgramScheduleAnchor::default()
                            }]
                        } else {
                            Vec::new()
                        },
                        playlist_items_by_key: BTreeMap::from([(
                            key.clone(),
                            vec![SchedulingPlaylistItem {
                                collection_key: SchedulingCollectionKey::fake_playlist_item(1),
                                playback_order,
                                play_all: true,
                                count: None,
                                media_items: vec![
                                    SchedulingMediaItem::new(101, Duration::minutes(10)),
                                    SchedulingMediaItem::new(102, Duration::minutes(10)),
                                    SchedulingMediaItem::new(103, Duration::minutes(10)),
                                ],
                            }],
                        )]),
                        ..SchedulingReferenceData::default()
                    },
                })
                .expect("Classic playlist should build")
                .added_items
                .first()
                .expect("playlist should schedule one item")
                .media_item_id
        };

        for order in [
            PlaybackOrder::Chronological,
            PlaybackOrder::SeasonEpisode,
            PlaybackOrder::ShuffleInOrder,
        ] {
            // ShuffleInOrder prepares its seeded group order even without a
            // random starting offset; chronological orders begin at 101.
            let expected = if order == PlaybackOrder::ShuffleInOrder {
                102
            } else {
                101
            };
            assert_eq!(build(order, false, 4, None), expected, "{order:?}");
            assert!(
                (1..=16)
                    .any(|seed| build(order, true, seed, None) != build(order, false, seed, None)),
                "{order:?} should permit a nonzero start"
            );
        }
        assert_eq!(build(PlaybackOrder::Chronological, true, 5, None), 103);
        assert_eq!(
            build(PlaybackOrder::Chronological, true, 5, Some(false)),
            103
        );
        assert_eq!(
            build(PlaybackOrder::Chronological, true, 5, Some(true)),
            103
        );
    }

    #[test]
    fn classic_random_playlist_continues_after_rebuilding_from_saved_state() {
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap();
        let schedule_item = ProgramScheduleItem {
            collection_type: CollectionType::Playlist,
            collection_id: None,
            playlist_id: Some(99),
            playout_mode: PlayoutMode::Flood,
            ..schedule_item(1, PlayoutMode::Flood, 1, None)
        };
        let key = item_key(&schedule_item);
        let playlist_items = (0..3)
            .map(|child| SchedulingPlaylistItem {
                collection_key: SchedulingCollectionKey::fake_playlist_item(child + 1),
                playback_order: PlaybackOrder::Chronological,
                play_all: false,
                count: None,
                media_items: (0..5)
                    .map(|index| {
                        SchedulingMediaItem::new(100 + child * 5 + index, Duration::minutes(1))
                    })
                    .collect(),
            })
            .collect::<Vec<_>>();
        let build = |build_start,
                     finish,
                     mode,
                     anchor: Option<PlayoutAnchor>,
                     anchors: Vec<PlayoutProgramScheduleAnchor>| {
            ClassicSchedulingEngine
                .schedule(SchedulingRequest {
                    playout: Playout {
                        id: 50,
                        schedule_kind: PlayoutScheduleKind::Classic,
                        seed: 5,
                        anchor,
                        ..Playout::default()
                    },
                    start: build_start,
                    finish,
                    mode,
                    reference_data: SchedulingReferenceData {
                        classic_schedule_items: vec![schedule_item.clone()],
                        classic_random_start_point: true,
                        playlist_items_by_key: BTreeMap::from([(
                            key.clone(),
                            playlist_items.clone(),
                        )]),
                        existing_program_schedule_anchors: anchors,
                        ..SchedulingReferenceData::default()
                    },
                })
                .expect("Classic playlist should build")
        };
        let seam = start + Duration::minutes(6);
        let oracle = build(
            start,
            start + Duration::minutes(12),
            PlayoutBuildMode::Reset,
            None,
            vec![],
        );
        let first = build(start, seam, PlayoutBuildMode::Reset, None, vec![]);
        let second = build(
            seam,
            start + Duration::minutes(12),
            PlayoutBuildMode::Continue,
            first.anchor.clone(),
            first.program_schedule_anchors.clone(),
        );
        let stitched = first
            .added_items
            .iter()
            .chain(second.added_items.iter())
            .map(|item| (item.media_item_id, item.start, item.finish))
            .collect::<Vec<_>>();
        assert_eq!(stitched.len(), 12);
        assert_eq!(
            stitched,
            oracle
                .added_items
                .iter()
                .map(|item| (item.media_item_id, item.start, item.finish))
                .collect::<Vec<_>>()
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
                enumerator_state: CollectionEnumeratorState {
                    seed: 42,
                    index: 1,
                    started: true,
                },
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
                enumerator_state: CollectionEnumeratorState {
                    seed: 42,
                    index: 1,
                    started: true,
                },
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
                            started: true,
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
                    enumerator_state: started_state(99, 0),
                    ..PlayoutProgramScheduleAnchor::default()
                },
                PlayoutProgramScheduleAnchor {
                    id: 901,
                    playout_id: 50,
                    collection_type: CollectionType::Collection,
                    collection_id: Some(1),
                    anchor_date: Some(start),
                    enumerator_state: started_state(42, 1),
                    ..PlayoutProgramScheduleAnchor::default()
                },
                PlayoutProgramScheduleAnchor {
                    id: 902,
                    playout_id: 50,
                    collection_type: CollectionType::Collection,
                    collection_id: Some(1),
                    anchor_date: Some(start + Duration::days(1)),
                    enumerator_state: started_state(7, 2),
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
    fn classic_fresh_multi_day_build_generates_dated_source_checkpoints() {
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap();
        let first = ProgramScheduleItem {
            playback_order: PlaybackOrder::Shuffle,
            ..schedule_item(1, PlayoutMode::One, 1, None)
        };
        let duplicate_source = ProgramScheduleItem {
            playback_order: PlaybackOrder::Shuffle,
            ..schedule_item(2, PlayoutMode::One, 1, None)
        };
        let key = item_key(&first);
        assert_eq!(key, item_key(&duplicate_source));

        let mut reference_data = SchedulingReferenceData {
            classic_schedule_items: vec![first, duplicate_source],
            ..SchedulingReferenceData::default()
        };
        reference_data.media_by_key.insert(
            key,
            vec![
                SchedulingMediaItem::new(101, Duration::hours(1)),
                SchedulingMediaItem::new(102, Duration::hours(1)),
                SchedulingMediaItem::new(103, Duration::hours(1)),
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
                finish: start + Duration::days(2),
                mode: PlayoutBuildMode::Continue,
                reference_data,
            })
            .expect("fresh multi-day Classic schedule should build");

        let dated = result
            .program_schedule_anchors
            .iter()
            .filter_map(|anchor| anchor.anchor_date)
            .collect::<Vec<_>>();
        assert!(
            dated.len() >= 2,
            "fresh builds must persist one deduplicated source checkpoint at the initial and day seams: {dated:?}"
        );
        assert_eq!(
            result
                .program_schedule_anchors
                .iter()
                .filter(|anchor| anchor.anchor_date.is_none())
                .count(),
            1,
            "duplicate schedule sources share one continuation enumerator"
        );
    }

    #[test]
    fn classic_hourly_continue_keeps_checkpoints_for_the_full_build_window() {
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 20, 0, 0).unwrap();
        let items = (1..=3)
            .map(|id| schedule_item(id, PlayoutMode::Multiple, 1, Some("5")))
            .collect::<Vec<_>>();
        let key = item_key(&items[0]);
        let mut reference_data = SchedulingReferenceData {
            classic_schedule_items: items,
            classic_shuffle_schedule_items: true,
            ..SchedulingReferenceData::default()
        };
        reference_data.media_by_key.insert(
            key,
            (101..=400)
                .map(|id| SchedulingMediaItem::new(id, Duration::minutes(100)))
                .collect(),
        );
        let mut playout = Playout {
            id: 50,
            schedule_kind: PlayoutScheduleKind::Classic,
            seed: 7,
            ..Playout::default()
        };
        for hour in 0..24 * 3 {
            let now = start + Duration::hours(hour);
            let finish = now + Duration::days(7) + Duration::hours(1);
            let result = ClassicSchedulingEngine
                .schedule(SchedulingRequest {
                    playout: playout.clone(),
                    start: now,
                    finish,
                    mode: PlayoutBuildMode::Continue,
                    reference_data: reference_data.clone(),
                })
                .expect("hourly Continue build");
            playout.anchor = result.anchor;
            reference_data.existing_items.extend(result.added_items);
            reference_data.existing_program_schedule_anchors = result.program_schedule_anchors;
            reference_data.existing_classic_checkpoints = result.classic_checkpoints;
            reference_data.existing_fill_group_indices = result.fill_group_indices;
            let first_day = (start + Duration::days(1))
                .date_naive()
                .max(now.date_naive());
            for day_offset in 0..=7 {
                let day = first_day + Duration::days(day_offset);
                if day > finish.date_naive() {
                    break;
                }
                assert!(
                    reference_data
                        .existing_classic_checkpoints
                        .iter()
                        .any(|checkpoint| checkpoint.checkpoint_date.date_naive() == day),
                    "missing checkpoint for {day} at {now}; dates: {:?}",
                    reference_data
                        .existing_classic_checkpoints
                        .iter()
                        .map(|checkpoint| checkpoint.checkpoint_date)
                        .collect::<Vec<_>>()
                );
            }
        }
    }

    #[test]
    fn classic_restart_refresh_matches_uninterrupted_checkpoint_timeline() {
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap();
        for playback_order in [
            PlaybackOrder::Shuffle,
            PlaybackOrder::Random,
            PlaybackOrder::RandomRotation,
        ] {
            let schedule_item = ProgramScheduleItem {
                playback_order,
                subtitle_mode: Some(ChannelSubtitleMode::Forced),
                ..schedule_item(1, PlayoutMode::One, 1, None)
            };
            let key = item_key(&schedule_item);
            let mut reference_data = SchedulingReferenceData {
                classic_schedule_items: vec![schedule_item],
                ..SchedulingReferenceData::default()
            };
            reference_data.media_by_key.insert(
                key.clone(),
                (101..=108)
                    .map(|id| SchedulingMediaItem::new(id, Duration::hours(1)))
                    .collect(),
            );
            let playout = Playout {
                id: 50,
                schedule_kind: PlayoutScheduleKind::Classic,
                seed: 17,
                ..Playout::default()
            };
            let initial = ClassicSchedulingEngine
                .schedule(SchedulingRequest {
                    playout: playout.clone(),
                    start,
                    finish: start + Duration::days(2),
                    mode: PlayoutBuildMode::Continue,
                    reference_data: reference_data.clone(),
                })
                .expect("initial production-shaped build should complete");

            let refresh_finish = start + Duration::hours(7);
            let expected = initial
                .added_items
                .iter()
                .filter(|item| item.start < refresh_finish)
                .map(|item| {
                    (
                        item.media_item_id,
                        item.start,
                        item.finish,
                        item.scheduling_context.clone(),
                    )
                })
                .collect::<Vec<_>>();
            let mut restarted_reference_data = reference_data;
            restarted_reference_data.classic_schedule_items[0].subtitle_mode =
                Some(ChannelSubtitleMode::Any);
            restarted_reference_data.existing_items = initial.added_items.clone();
            restarted_reference_data.existing_program_schedule_anchors =
                initial.program_schedule_anchors;
            restarted_reference_data.existing_classic_checkpoints = initial.classic_checkpoints;
            restarted_reference_data.existing_fill_group_indices = initial.fill_group_indices;

            let refreshed = ClassicSchedulingEngine
                .schedule(SchedulingRequest {
                    playout: Playout {
                        anchor: initial.anchor,
                        ..playout
                    },
                    start: start + Duration::hours(3),
                    finish: refresh_finish,
                    mode: PlayoutBuildMode::Refresh,
                    reference_data: restarted_reference_data,
                })
                .expect("restart Refresh should restore the complete checkpoint");
            let actual = refreshed
                .added_items
                .iter()
                .map(|item| {
                    (
                        item.media_item_id,
                        item.start,
                        item.finish,
                        item.scheduling_context.clone(),
                    )
                })
                .collect::<Vec<_>>();

            assert_eq!(
                actual, expected,
                "{playback_order:?} Refresh diverged from uninterrupted scheduling"
            );
            assert!(refreshed
                .added_items
                .iter()
                .all(|item| item.subtitle_mode == Some(ChannelSubtitleMode::Any)));
        }
    }

    #[test]
    fn classic_refresh_refuses_legacy_continuation_only_state() {
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap();
        let schedule_item = ProgramScheduleItem {
            playback_order: PlaybackOrder::Shuffle,
            ..schedule_item(1, PlayoutMode::One, 1, None)
        };
        let key = item_key(&schedule_item);
        let mut reference_data = SchedulingReferenceData {
            classic_schedule_items: vec![schedule_item],
            existing_items: vec![PlayoutItem {
                playout_id: 50,
                media_item_id: 101,
                start,
                finish: start + Duration::hours(1),
                ..PlayoutItem::default()
            }],
            existing_program_schedule_anchors: vec![PlayoutProgramScheduleAnchor {
                playout_id: 50,
                collection_type: CollectionType::Collection,
                collection_id: Some(1),
                enumerator_state: CollectionEnumeratorState {
                    seed: 7,
                    index: 1,
                    started: true,
                },
                ..PlayoutProgramScheduleAnchor::default()
            }],
            ..SchedulingReferenceData::default()
        };
        reference_data.media_by_key.insert(
            key,
            vec![
                SchedulingMediaItem::new(101, Duration::hours(1)),
                SchedulingMediaItem::new(102, Duration::hours(1)),
            ],
        );

        let error = ClassicSchedulingEngine
            .schedule(SchedulingRequest {
                playout: Playout {
                    id: 50,
                    schedule_kind: PlayoutScheduleKind::Classic,
                    seed: 7,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::hours(4),
                mode: PlayoutBuildMode::Refresh,
                reference_data,
            })
            .expect_err("legacy continuation-only state must not be cleared");

        assert!(error.to_string().contains("Classic Refresh refused"));
        assert!(error.to_string().contains("Reset Playout"));
    }

    #[test]
    fn classic_refresh_reconciles_shuffle_membership_without_consumed_replay() {
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap();
        let checkpoint_day = Utc.with_ymd_and_hms(2026, 4, 27, 0, 0, 0).unwrap();
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
            key.clone(),
            (101..=108)
                .map(|id| SchedulingMediaItem::new(id, Duration::hours(1)))
                .collect(),
        );
        let playout = Playout {
            id: 50,
            schedule_kind: PlayoutScheduleKind::Classic,
            seed: 17,
            ..Playout::default()
        };
        let initial = ClassicSchedulingEngine
            .schedule(SchedulingRequest {
                playout: playout.clone(),
                start,
                finish: start + Duration::days(2),
                mode: PlayoutBuildMode::Continue,
                reference_data: reference_data.clone(),
            })
            .expect("initial shuffled build");
        let checkpoint = initial
            .classic_checkpoints
            .iter()
            .find(|checkpoint| checkpoint.checkpoint_date == checkpoint_day)
            .expect("midnight checkpoint");
        let checkpoint_context =
            serde_json::from_str::<ClassicCheckpointContext>(&checkpoint.context)
                .expect("checkpoint context");
        let media_checkpoint = checkpoint_context.sources[&key]
            .media
            .as_ref()
            .expect("media checkpoint");
        assert!(media_checkpoint.current_index > 0);
        let consumed_before_checkpoint = media_checkpoint.ordered_items
            [..media_checkpoint.current_index]
            .iter()
            .map(|identity| identity.media_item_id)
            .collect::<BTreeSet<_>>();
        let removed_id =
            media_checkpoint.ordered_items[media_checkpoint.current_index].media_item_id;

        let mut restarted = reference_data;
        restarted
            .media_by_key
            .get_mut(&key)
            .expect("source media")
            .retain(|item| item.id != removed_id);
        restarted
            .media_by_key
            .get_mut(&key)
            .expect("source media")
            .push(SchedulingMediaItem::new(999, Duration::hours(1)));
        restarted.existing_items = initial.added_items;
        restarted.existing_program_schedule_anchors = initial.program_schedule_anchors;
        restarted.existing_classic_checkpoints = initial.classic_checkpoints;
        restarted.existing_fill_group_indices = initial.fill_group_indices;

        let refresh_time = checkpoint_day + Duration::hours(3);
        let refreshed = ClassicSchedulingEngine
            .schedule(SchedulingRequest {
                playout: Playout {
                    anchor: initial.anchor,
                    ..playout
                },
                start: refresh_time,
                finish: checkpoint_day + Duration::hours(6),
                mode: PlayoutBuildMode::Refresh,
                reference_data: restarted,
            })
            .expect("membership-changing Refresh should reconcile the checkpoint");
        let rebuilt_before_refresh = refreshed
            .added_items
            .iter()
            .filter(|item| item.start < refresh_time)
            .map(|item| item.media_item_id)
            .collect::<BTreeSet<_>>();
        let future_ids = refreshed
            .added_items
            .iter()
            .filter(|item| item.start >= refresh_time)
            .map(|item| item.media_item_id)
            .collect::<Vec<_>>();

        assert!(!future_ids.is_empty());
        assert!(future_ids.iter().all(|id| {
            !consumed_before_checkpoint.contains(id) && !rebuilt_before_refresh.contains(id)
        }));
    }

    #[test]
    fn classic_refresh_restores_shuffled_schedule_order_with_duplicate_sources() {
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 20, 0, 0).unwrap();
        let checkpoint_day = Utc.with_ymd_and_hms(2026, 4, 27, 0, 0, 0).unwrap();
        let schedule_items = (1..=3)
            .map(|id| schedule_item(id, PlayoutMode::One, 1, None))
            .collect::<Vec<_>>();
        let key = item_key(&schedule_items[0]);
        assert!(schedule_items.iter().all(|item| item_key(item) == key));
        let mut reference_data = SchedulingReferenceData {
            classic_schedule_items: schedule_items,
            classic_shuffle_schedule_items: true,
            ..SchedulingReferenceData::default()
        };
        reference_data.media_by_key.insert(
            key,
            (101..=105)
                .map(|id| SchedulingMediaItem::new(id, Duration::minutes(1)))
                .collect(),
        );
        let playout = Playout {
            id: 50,
            schedule_kind: PlayoutScheduleKind::Classic,
            seed: 17,
            ..Playout::default()
        };
        let initial = ClassicSchedulingEngine
            .schedule(SchedulingRequest {
                playout: playout.clone(),
                start,
                finish: checkpoint_day + Duration::hours(1),
                mode: PlayoutBuildMode::Continue,
                reference_data: reference_data.clone(),
            })
            .expect("initial shuffled schedule-item build");
        let expected = initial
            .added_items
            .iter()
            .filter(|item| item.start >= checkpoint_day)
            .map(|item| (item.media_item_id, item.scheduling_context.clone()))
            .collect::<Vec<_>>();

        let mut restarted = reference_data;
        restarted.existing_items = initial.added_items;
        restarted.existing_program_schedule_anchors = initial.program_schedule_anchors;
        restarted.existing_classic_checkpoints = initial.classic_checkpoints;
        restarted.existing_fill_group_indices = initial.fill_group_indices;
        let refreshed = ClassicSchedulingEngine
            .schedule(SchedulingRequest {
                playout: Playout {
                    anchor: initial.anchor,
                    ..playout
                },
                start: checkpoint_day + Duration::minutes(15),
                finish: checkpoint_day + Duration::hours(1),
                mode: PlayoutBuildMode::Refresh,
                reference_data: restarted,
            })
            .expect("Refresh should restore shuffled schedule-item state");
        let actual = refreshed
            .added_items
            .iter()
            .map(|item| (item.media_item_id, item.scheduling_context.clone()))
            .collect::<Vec<_>>();

        assert_eq!(actual, expected);
        assert_eq!(
            refreshed
                .program_schedule_anchors
                .iter()
                .filter(|anchor| anchor.anchor_date.is_none())
                .count(),
            1,
            "duplicate schedule items must share one source enumerator"
        );
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
    fn classic_continue_preserves_last_flat_shuffle_index_for_grouped_show_items() {
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap();
        let schedule_item = ProgramScheduleItem {
            playback_order: PlaybackOrder::Shuffle,
            ..schedule_item(1, PlayoutMode::One, 1, None)
        };
        let key = item_key(&schedule_item);
        let items = (101..=110)
            .map(|id| SchedulingMediaItem {
                show_id: Some(7),
                ..SchedulingMediaItem::new(id, Duration::minutes(10))
            })
            .collect::<Vec<_>>();
        let mut expected =
            MediaEnumerator::new(&key, items.clone()).with_playback_order(PlaybackOrder::Shuffle);
        prepare_enumerator_items(
            &mut expected,
            PlaybackOrder::Shuffle,
            seed_to_u64(1234),
            None,
        );
        expected.set_index(9);
        let expected_id = expected.current().expect("last item").id;

        let reference_data = SchedulingReferenceData {
            classic_schedule_items: vec![schedule_item],
            media_by_key: BTreeMap::from([(key, items)]),
            existing_program_schedule_anchors: vec![PlayoutProgramScheduleAnchor {
                id: 21,
                playout_id: 50,
                collection_type: CollectionType::Collection,
                collection_id: Some(1),
                enumerator_state: CollectionEnumeratorState {
                    seed: 1234,
                    index: 9,
                    started: true,
                },
                ..PlayoutProgramScheduleAnchor::default()
            }],
            ..SchedulingReferenceData::default()
        };
        let result = ClassicSchedulingEngine
            .schedule(SchedulingRequest {
                playout: Playout {
                    id: 50,
                    schedule_kind: PlayoutScheduleKind::Classic,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::minutes(10),
                mode: PlayoutBuildMode::Continue,
                reference_data,
            })
            .expect("Classic Continue with last valid index");
        assert_eq!(result.added_items[0].media_item_id, expected_id);
    }

    #[test]
    fn classic_continue_keeps_inactive_alternate_collection_progress() {
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap();
        let active = schedule_item(1, PlayoutMode::One, 1, None);
        let key = item_key(&active);
        let inactive = PlayoutProgramScheduleAnchor {
            id: 22,
            playout_id: 50,
            collection_type: CollectionType::Collection,
            collection_id: Some(2),
            enumerator_state: CollectionEnumeratorState {
                seed: 1234,
                index: 9,
                started: true,
            },
            ..PlayoutProgramScheduleAnchor::default()
        };
        let reference_data = SchedulingReferenceData {
            classic_schedule_items: vec![active],
            media_by_key: BTreeMap::from([(
                key,
                vec![SchedulingMediaItem::new(101, Duration::minutes(10))],
            )]),
            existing_program_schedule_anchors: vec![inactive.clone()],
            ..SchedulingReferenceData::default()
        };
        let result = ClassicSchedulingEngine
            .schedule(SchedulingRequest {
                playout: Playout {
                    id: 50,
                    schedule_kind: PlayoutScheduleKind::Classic,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::minutes(10),
                mode: PlayoutBuildMode::Continue,
                reference_data,
            })
            .expect("Continue with inactive alternate");
        assert!(result.program_schedule_anchors.contains(&inactive));
    }

    #[test]
    fn classic_runtime_audit_accepts_inactive_alternate_anchor() {
        let active = schedule_item(1, PlayoutMode::One, 1, None);
        let anchors = [1, 2].map(|collection_id| PlayoutProgramScheduleAnchor {
            playout_id: 50,
            collection_type: CollectionType::Collection,
            collection_id: Some(collection_id),
            enumerator_state: CollectionEnumeratorState {
                seed: 1234,
                index: 9,
                started: true,
            },
            ..PlayoutProgramScheduleAnchor::default()
        });
        let reference_data = SchedulingReferenceData {
            classic_schedule_items: vec![active],
            existing_program_schedule_anchors: anchors.to_vec(),
            ..SchedulingReferenceData::default()
        };
        let audit = audit_classic_runtime_state(&reference_data);
        assert!(audit.anchors_match);
        assert_eq!(audit.expected_anchor_count, 1);
        assert_eq!(audit.actual_anchor_count, 1);
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
                enumerator_state: CollectionEnumeratorState {
                    seed: 11,
                    index: 4,
                    started: true,
                },
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
    fn classic_nonpositive_tail_and_fallback_do_not_stall_builds() {
        let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap();
        for duration in [Duration::zero(), Duration::seconds(-1)] {
            let first = ProgramScheduleItem {
                start_type: StartType::Fixed,
                start_time: Some(Duration::hours(6)),
                tail_filler_id: Some(93),
                fallback_filler_id: Some(94),
                ..schedule_item(1, PlayoutMode::One, 1, None)
            };
            let next = ProgramScheduleItem {
                start_type: StartType::Fixed,
                start_time: Some(Duration::hours(7)),
                ..schedule_item(2, PlayoutMode::One, 2, None)
            };
            let mut reference_data = SchedulingReferenceData {
                classic_schedule_items: vec![first, next],
                ..SchedulingReferenceData::default()
            };
            for item in &reference_data.classic_schedule_items {
                reference_data.media_by_key.insert(
                    item_key(item),
                    vec![SchedulingMediaItem::new(
                        item.id * 100 + 1,
                        Duration::minutes(10),
                    )],
                );
            }
            for (id, kind) in [(93, FillerKind::Tail), (94, FillerKind::Fallback)] {
                let mut preset = filler_preset(
                    id,
                    kind,
                    FillerMode::Duration,
                    None,
                    Some(Duration::hours(1)),
                );
                preset
                    .media_items
                    .push(SchedulingMediaItem::new(id, duration));
                reference_data.classic_filler_presets.insert(id, preset);
            }
            let result = ClassicSchedulingEngine
                .schedule(SchedulingRequest {
                    playout: Playout {
                        id: 50,
                        schedule_kind: PlayoutScheduleKind::Classic,
                        ..Playout::default()
                    },
                    start,
                    finish: start + Duration::minutes(70),
                    mode: PlayoutBuildMode::Continue,
                    reference_data,
                })
                .unwrap();
            assert!(result
                .added_items
                .iter()
                .any(|item| item.media_item_id == 201));
            assert!(result
                .added_items
                .iter()
                .all(|item| item.finish > item.start && matches!(item.media_item_id, 101 | 201)));
        }
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

    pub(super) fn schedule_item(
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

    pub(super) fn filler_preset(
        id: EntityId,
        filler_kind: FillerKind,
        filler_mode: FillerMode,
        count: Option<i32>,
        duration: Option<Duration>,
    ) -> SchedulingFillerPreset {
        SchedulingFillerPreset {
            expression: None,
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
                "anchor_date": anchor.anchor_date,
                "index": anchor.enumerator_state.index,
            })).collect::<Vec<_>>(),
            "classic_checkpoints": result.classic_checkpoints.iter().map(|checkpoint| checkpoint.checkpoint_date).collect::<Vec<_>>(),
            "warnings": result.warnings,
        })
    }
}

#[cfg(test)]
mod fallback_tests;

mod mid_roll;
#[cfg(test)]
mod playlist_tests;

mod chapter_expression;
