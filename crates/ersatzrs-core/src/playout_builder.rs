//! Top-level playout build orchestrator.
//!
//! Mirrors the C# `IPlayoutBuilder` dispatch hierarchy:
//! - `Block`      → [`BlockSchedulingEngine`]
//! - `Classic`    → [`ClassicSchedulingEngine`]
//! - `Sequential` → [`YamlPlayoutEngine`]
//! - `Scripted`   → [`ScriptedSchedulingEngine`]
//! - `ExternalJson` → [`ExternalJsonSchedulingEngine`]
//!
//! The C# entry point for the Classic engine is
//! `ErsatzTV.Core/Scheduling/PlayoutBuilder.cs`.  Block, YAML, and Scripted
//! counterparts are in the `BlockScheduling/` and `YamlScheduling/` and
//! `ScriptedScheduling/` sub-namespaces respectively.

use std::collections::{BTreeMap, BTreeSet};

use chrono::Duration;
use ersatzrs_domain::{
    EntityId, FillerKind, PlayoutItem, PlayoutMode, PlayoutScheduleKind, RemoteStreamLiveness,
};

use crate::scheduling::{
    classic_schedule_item_key, BlockSchedulingEngine, ClassicSchedulingEngine,
    ExternalJsonSchedulingEngine, SchedulingError, SchedulingFallbackPreset, SchedulingMediaItem,
    SchedulingReferenceData, SchedulingRequest, SchedulingResult, ScriptedSchedulingEngine,
    YamlPlayoutEngine,
};

/// Routes playout build requests to the engine registered for each
/// [`PlayoutScheduleKind`].
///
/// Construct via [`PlayoutBuilder::default`] to use the built-in engines, or
/// via [`PlayoutBuilder::new`] to inject custom implementations.
pub struct PlayoutBuilder {
    classic: Box<dyn EngineDispatch + Send + Sync>,
    block: Box<dyn EngineDispatch + Send + Sync>,
    yaml: Box<dyn EngineDispatch + Send + Sync>,
    scripted: Box<dyn EngineDispatch + Send + Sync>,
    external_json: Box<dyn EngineDispatch + Send + Sync>,
}

impl std::fmt::Debug for PlayoutBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlayoutBuilder").finish_non_exhaustive()
    }
}

impl Default for PlayoutBuilder {
    fn default() -> Self {
        Self {
            classic: Box::new(ClassicSchedulingEngine),
            block: Box::new(BlockSchedulingEngine),
            yaml: Box::new(YamlPlayoutEngine),
            scripted: Box::new(ScriptedSchedulingEngine),
            external_json: Box::new(ExternalJsonSchedulingEngine),
        }
    }
}

impl PlayoutBuilder {
    /// Construct a builder with one engine per supported schedule kind.
    #[must_use]
    pub fn new(
        classic: Box<dyn EngineDispatch + Send + Sync>,
        block: Box<dyn EngineDispatch + Send + Sync>,
        yaml: Box<dyn EngineDispatch + Send + Sync>,
        scripted: Box<dyn EngineDispatch + Send + Sync>,
        external_json: Box<dyn EngineDispatch + Send + Sync>,
    ) -> Self {
        Self {
            classic,
            block,
            yaml,
            scripted,
            external_json,
        }
    }

    /// Build a playout timeline.
    ///
    /// Dispatches to the engine registered for `request.playout.schedule_kind`.
    /// The [`crate::scheduling::PlayoutBuildMode`] carried in `request.mode`
    /// mirrors C# `PlayoutBuildMode`: `Reset`, `Continue`, and `Refresh`.
    ///
    /// # Errors
    ///
    /// Returns [`SchedulingError::UnsupportedScheduleKind`] for `None`, which
    /// is not a buildable schedule kind.
    pub fn build(&self, request: SchedulingRequest) -> Result<SchedulingResult, SchedulingError> {
        validate_remote_stream_placements(&request)?;
        let options = ConfiguredPlayoutOptions::from_request(&request);
        let mut result = match request.playout.schedule_kind {
            PlayoutScheduleKind::Classic => self.classic.schedule(request),
            PlayoutScheduleKind::Block => self.block.schedule(request),
            PlayoutScheduleKind::Sequential => self.yaml.schedule(request),
            PlayoutScheduleKind::Scripted => self.scripted.schedule(request),
            PlayoutScheduleKind::ExternalJson => self.external_json.schedule(request),
            PlayoutScheduleKind::None => Err(SchedulingError::UnsupportedScheduleKind(
                PlayoutScheduleKind::None,
            )),
        }?;

        apply_configured_playout_options(&mut result, &options);
        Ok(result)
    }
}

fn validate_remote_stream_placements(request: &SchedulingRequest) -> Result<(), SchedulingError> {
    let items = unique_scheduling_media_items(&request.reference_data);
    let live_ids = items
        .values()
        .filter_map(|item| match item.liveness {
            Some(RemoteStreamLiveness::Unknown) => {
                Some(Err(SchedulingError::UnknownRemoteStreamLiveness(item.id)))
            }
            Some(RemoteStreamLiveness::Finite) if item.duration <= Duration::zero() => Some(Err(
                SchedulingError::FiniteRemoteStreamMissingDuration(item.id),
            )),
            Some(RemoteStreamLiveness::Live) => Some(Ok(item.id)),
            Some(RemoteStreamLiveness::Finite) | None => None,
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    if live_ids.is_empty() {
        return Ok(());
    }

    if request.playout.schedule_kind != PlayoutScheduleKind::Classic {
        return Err(SchedulingError::UnsupportedLiveSourcePlacement(
            *live_ids.iter().next().expect("live id exists"),
        ));
    }

    let mut flood_ids = BTreeSet::new();
    let mut incompatible_ids = BTreeSet::new();
    for schedule_item in &request.reference_data.classic_schedule_items {
        let key = classic_schedule_item_key(schedule_item);
        let target = if schedule_item.playout_mode == PlayoutMode::Flood {
            &mut flood_ids
        } else {
            &mut incompatible_ids
        };
        if let Some(media_items) = request.reference_data.media_by_key.get(&key) {
            target.extend(media_items.iter().map(|item| item.id));
        }
        if let Some(playlist_items) = request.reference_data.playlist_items_by_key.get(&key) {
            target.extend(
                playlist_items
                    .iter()
                    .flat_map(|playlist| playlist.media_items.iter())
                    .map(|item| item.id),
            );
        }
    }

    live_ids
        .iter()
        .find(|id| incompatible_ids.contains(id) || !flood_ids.contains(id))
        .map_or(Ok(()), |id| {
            Err(SchedulingError::UnsupportedLiveSourcePlacement(*id))
        })
}

fn unique_scheduling_media_items(
    reference: &SchedulingReferenceData,
) -> BTreeMap<EntityId, SchedulingMediaItem> {
    let mut result = BTreeMap::new();
    let mut insert = |item: &SchedulingMediaItem| {
        result.entry(item.id).or_insert_with(|| item.clone());
    };
    for item in reference.media_items_by_id.values() {
        insert(item);
    }
    for items in reference
        .media_by_key
        .values()
        .chain(reference.collections_by_name.values())
        .chain(reference.multi_collections_by_name.values())
        .chain(reference.smart_collections_by_name.values())
        .chain(reference.playlists_by_group_and_name.values())
        .chain(reference.searches_by_query.values())
        .chain(reference.shows_by_name.values())
        .chain(reference.media_by_show_guid.values())
    {
        for item in items {
            insert(item);
        }
    }
    for playlist in reference
        .playlist_items_by_group_and_name
        .values()
        .chain(reference.playlist_items_by_key.values())
        .flatten()
    {
        for item in &playlist.media_items {
            insert(item);
        }
    }
    for preset in reference.classic_filler_presets.values() {
        for item in &preset.media_items {
            insert(item);
        }
    }
    if let Some(fallback) = &reference.global_fallback {
        for item in &fallback.media_items {
            insert(item);
        }
    }
    result
}

/// Object-safe wrapper around [`crate::scheduling::PlayoutSchedulingEngine`].
pub trait EngineDispatch {
    /// Build a playout timeline for the supplied request.
    ///
    /// # Errors
    ///
    /// Returns a [`SchedulingError`] when the engine cannot evaluate its
    /// schedule definition.
    fn schedule(&self, request: SchedulingRequest) -> Result<SchedulingResult, SchedulingError>;
}

impl<E: crate::scheduling::PlayoutSchedulingEngine> EngineDispatch for E {
    fn schedule(&self, request: SchedulingRequest) -> Result<SchedulingResult, SchedulingError> {
        crate::scheduling::PlayoutSchedulingEngine::schedule(self, request)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct ConfiguredPlayoutOptions {
    global_watermark_id: Option<EntityId>,
    global_fallback: Option<SchedulingFallbackPreset>,
}

impl ConfiguredPlayoutOptions {
    fn from_request(request: &SchedulingRequest) -> Self {
        Self {
            global_watermark_id: request.reference_data.global_watermark_id,
            global_fallback: request.reference_data.global_fallback.clone(),
        }
    }
}

fn apply_configured_playout_options(
    result: &mut SchedulingResult,
    options: &ConfiguredPlayoutOptions,
) {
    apply_global_fallback(result, options.global_fallback.as_ref());
    apply_global_watermark(result, options.global_watermark_id);
}

fn apply_global_watermark(result: &mut SchedulingResult, global_watermark_id: Option<EntityId>) {
    let Some(global_watermark_id) = global_watermark_id else {
        return;
    };

    for item in &mut result.added_items {
        if !item.disable_watermarks && item.watermark_ids.is_empty() {
            item.watermark_ids.push(global_watermark_id);
        }
    }
}

fn apply_global_fallback(
    result: &mut SchedulingResult,
    global_fallback: Option<&SchedulingFallbackPreset>,
) {
    let Some(global_fallback) = global_fallback else {
        return;
    };
    if global_fallback.media_items.is_empty() || result.added_gaps.is_empty() {
        return;
    }

    let mut fallback_items = Vec::new();
    for gap in &result.added_gaps {
        let mut current = gap.start;
        let mut media_index = 0usize;
        while current < gap.finish {
            let media =
                &global_fallback.media_items[media_index % global_fallback.media_items.len()];
            let remaining = gap.finish - current;
            let duration = min_positive_duration(media.duration, remaining);
            if duration <= Duration::zero() {
                break;
            }

            let mut item = PlayoutItem {
                playout_id: gap.playout_id,
                media_item_id: media.id,
                start: current,
                finish: current + duration,
                guide_start: Some(gap.start),
                guide_finish: Some(gap.finish),
                filler_kind: FillerKind::Fallback,
                in_point: Duration::zero(),
                out_point: duration,
                disable_watermarks: !global_fallback.allow_watermarks,
                ..PlayoutItem::default()
            };
            item.guide_group = 0;
            fallback_items.push(item);

            current += duration;
            media_index += 1;
        }
    }

    if !fallback_items.is_empty() {
        result.added_items.extend(fallback_items);
        result.added_items.sort_by(|left, right| {
            left.start
                .cmp(&right.start)
                .then(left.finish.cmp(&right.finish))
                .then(left.media_item_id.cmp(&right.media_item_id))
        });
        result.added_gaps.clear();
    }
}

fn min_positive_duration(left: Duration, right: Duration) -> Duration {
    if left <= Duration::zero() {
        right
    } else if right <= Duration::zero() {
        Duration::zero()
    } else {
        left.min(right)
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use ersatzrs_domain::{
        FillerKind, Playout, PlayoutGap, PlayoutMode, PlayoutScheduleKind, ProgramScheduleItem,
        RemoteStreamLiveness,
    };

    use super::*;
    use crate::scheduling::{
        PlayoutBuildMode, SchedulingFallbackPreset, SchedulingMediaItem, SchedulingReferenceData,
    };

    #[test]
    fn external_json_build_is_successful_noop() {
        let start = Utc::now();
        let result = PlayoutBuilder::default()
            .build(SchedulingRequest {
                playout: Playout {
                    schedule_kind: PlayoutScheduleKind::ExternalJson,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::hours(1),
                mode: PlayoutBuildMode::Continue,
                reference_data: SchedulingReferenceData::default(),
            })
            .expect("external json build should succeed");

        assert_eq!(result, SchedulingResult::default());
    }

    #[derive(Debug)]
    struct StaticEngine(SchedulingResult);

    impl EngineDispatch for StaticEngine {
        fn schedule(
            &self,
            _request: SchedulingRequest,
        ) -> Result<SchedulingResult, SchedulingError> {
            Ok(self.0.clone())
        }
    }

    #[test]
    fn global_watermark_applies_when_item_has_no_watermarks() {
        let start = Utc::now();
        let item = PlayoutItem {
            media_item_id: 10,
            playout_id: 1,
            start,
            finish: start + Duration::minutes(30),
            in_point: Duration::zero(),
            out_point: Duration::minutes(30),
            ..PlayoutItem::default()
        };
        let result = SchedulingResult {
            added_items: vec![item],
            ..SchedulingResult::default()
        };

        let builder = builder_with_static_result(result);
        let output = builder
            .build(SchedulingRequest {
                playout: Playout {
                    id: 1,
                    schedule_kind: PlayoutScheduleKind::Block,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::hours(1),
                mode: PlayoutBuildMode::Continue,
                reference_data: SchedulingReferenceData {
                    global_watermark_id: Some(99),
                    ..SchedulingReferenceData::default()
                },
            })
            .expect("build should succeed");

        assert_eq!(output.added_items[0].watermark_ids, vec![99]);
    }

    #[test]
    fn global_fallback_fills_generated_gaps() {
        let start = Utc::now();
        let gap = PlayoutGap {
            playout_id: 1,
            start,
            finish: start + Duration::minutes(45),
            ..PlayoutGap::default()
        };
        let result = SchedulingResult {
            added_gaps: vec![gap],
            ..SchedulingResult::default()
        };

        let builder = builder_with_static_result(result);
        let output = builder
            .build(SchedulingRequest {
                playout: Playout {
                    id: 1,
                    schedule_kind: PlayoutScheduleKind::Block,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::hours(1),
                mode: PlayoutBuildMode::Continue,
                reference_data: SchedulingReferenceData {
                    global_fallback: Some(SchedulingFallbackPreset {
                        id: 7,
                        allow_watermarks: false,
                        media_items: vec![SchedulingMediaItem::new(42, Duration::minutes(20))],
                    }),
                    global_watermark_id: Some(99),
                    ..SchedulingReferenceData::default()
                },
            })
            .expect("build should succeed");

        assert!(output.added_gaps.is_empty());
        assert_eq!(output.added_items.len(), 3);
        assert!(output
            .added_items
            .iter()
            .all(|item| item.filler_kind == FillerKind::Fallback));
        assert!(output
            .added_items
            .iter()
            .all(|item| item.disable_watermarks && item.watermark_ids.is_empty()));
        assert_eq!(output.added_items[2].finish, start + Duration::minutes(45));
    }

    #[test]
    #[allow(clippy::too_many_lines)] // One matrix keeps every liveness/scheduler refusal adjacent.
    fn live_sources_require_classic_flood_and_unknown_never_schedules() {
        let start = Utc::now();
        let mut live = SchedulingMediaItem::new(42, Duration::zero());
        live.liveness = Some(RemoteStreamLiveness::Live);
        for schedule_kind in [
            PlayoutScheduleKind::None,
            PlayoutScheduleKind::Block,
            PlayoutScheduleKind::Sequential,
            PlayoutScheduleKind::Scripted,
            PlayoutScheduleKind::ExternalJson,
        ] {
            let request = SchedulingRequest {
                playout: Playout {
                    schedule_kind,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::hours(1),
                mode: PlayoutBuildMode::Continue,
                reference_data: SchedulingReferenceData {
                    media_items_by_id: [(live.id, live.clone())].into(),
                    ..SchedulingReferenceData::default()
                },
            };
            assert!(matches!(
                PlayoutBuilder::default().build(request),
                Err(SchedulingError::UnsupportedLiveSourcePlacement(42))
            ));
        }

        for playout_mode in [
            PlayoutMode::One,
            PlayoutMode::Multiple,
            PlayoutMode::Duration,
        ] {
            let schedule_item = ProgramScheduleItem {
                id: 7,
                playout_mode,
                media_item_id: Some(live.id),
                ..ProgramScheduleItem::default()
            };
            let key = classic_schedule_item_key(&schedule_item);
            let request = SchedulingRequest {
                playout: Playout {
                    schedule_kind: PlayoutScheduleKind::Classic,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::hours(1),
                mode: PlayoutBuildMode::Continue,
                reference_data: SchedulingReferenceData {
                    classic_schedule_items: vec![schedule_item],
                    media_by_key: [(key, vec![live.clone()])].into(),
                    ..SchedulingReferenceData::default()
                },
            };
            assert!(matches!(
                PlayoutBuilder::default().build(request),
                Err(SchedulingError::UnsupportedLiveSourcePlacement(42))
            ));
        }

        let schedule_item = ProgramScheduleItem {
            id: 7,
            playout_mode: PlayoutMode::Flood,
            media_item_id: Some(live.id),
            ..ProgramScheduleItem::default()
        };
        let key = classic_schedule_item_key(&schedule_item);
        let classic_request = SchedulingRequest {
            playout: Playout {
                schedule_kind: PlayoutScheduleKind::Classic,
                ..Playout::default()
            },
            start,
            finish: start + Duration::hours(1),
            mode: PlayoutBuildMode::Continue,
            reference_data: SchedulingReferenceData {
                classic_schedule_items: vec![schedule_item],
                media_by_key: [(key, vec![live.clone()])].into(),
                ..SchedulingReferenceData::default()
            },
        };
        PlayoutBuilder::default()
            .build(classic_request)
            .expect("Classic Flood should accept a durationless live source");

        let mut unknown = SchedulingMediaItem::new(99, Duration::minutes(1));
        unknown.liveness = Some(RemoteStreamLiveness::Unknown);
        let unknown_request = SchedulingRequest {
            playout: Playout {
                schedule_kind: PlayoutScheduleKind::Classic,
                ..Playout::default()
            },
            start,
            finish: start + Duration::hours(1),
            mode: PlayoutBuildMode::Continue,
            reference_data: SchedulingReferenceData {
                media_items_by_id: [(unknown.id, unknown)].into(),
                ..SchedulingReferenceData::default()
            },
        };
        assert!(matches!(
            PlayoutBuilder::default().build(unknown_request),
            Err(SchedulingError::UnknownRemoteStreamLiveness(99))
        ));

        let mut durationless_finite = SchedulingMediaItem::new(100, Duration::zero());
        durationless_finite.liveness = Some(RemoteStreamLiveness::Finite);
        let finite_request = SchedulingRequest {
            playout: Playout {
                schedule_kind: PlayoutScheduleKind::Classic,
                ..Playout::default()
            },
            start,
            finish: start + Duration::hours(1),
            mode: PlayoutBuildMode::Continue,
            reference_data: SchedulingReferenceData {
                media_items_by_id: [(durationless_finite.id, durationless_finite)].into(),
                ..SchedulingReferenceData::default()
            },
        };
        assert!(matches!(
            PlayoutBuilder::default().build(finite_request),
            Err(SchedulingError::FiniteRemoteStreamMissingDuration(100))
        ));
    }

    fn builder_with_static_result(result: SchedulingResult) -> PlayoutBuilder {
        PlayoutBuilder::new(
            Box::new(StaticEngine(SchedulingResult::default())),
            Box::new(StaticEngine(result)),
            Box::new(StaticEngine(SchedulingResult::default())),
            Box::new(StaticEngine(SchedulingResult::default())),
            Box::new(StaticEngine(SchedulingResult::default())),
        )
    }
}
