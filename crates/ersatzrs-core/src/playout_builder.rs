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
    pub fn build(
        &self,
        mut request: SchedulingRequest,
    ) -> Result<SchedulingResult, SchedulingError> {
        let populated_classic_sources = populated_classic_source_ids(&request.reference_data);
        let dropped = drop_unschedulable_remote_streams(&mut request.reference_data);
        validate_populated_classic_sources(&request.reference_data, &populated_classic_sources)?;
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
        result.warnings.extend(dropped);
        Ok(result)
    }
}

/// Capture only populated primary Classic sources before Remote Stream filtering.
///
/// ErsatzTV rejects a referenced collection when its zero-duration cleanup
/// leaves that collection empty. Snapshotting first keeps this guard scoped to
/// the filter transition: pre-existing empty sources retain their established
/// behavior, and optional filler/fallback collections are not promoted to
/// fatal primary sources.
fn populated_classic_source_ids(reference: &SchedulingReferenceData) -> BTreeSet<EntityId> {
    reference
        .classic_schedule_items
        .iter()
        .filter(|item| classic_source_has_media(reference, item))
        .map(|item| item.id)
        .collect()
}

fn validate_populated_classic_sources(
    reference: &SchedulingReferenceData,
    populated_source_ids: &BTreeSet<EntityId>,
) -> Result<(), SchedulingError> {
    if let Some(item) = reference.classic_schedule_items.iter().find(|item| {
        populated_source_ids.contains(&item.id) && !classic_source_has_media(reference, item)
    }) {
        return Err(SchedulingError::EmptyClassicSourceAfterRemoteStreamFilter(
            item.id,
        ));
    }
    Ok(())
}

fn classic_source_has_media(
    reference: &SchedulingReferenceData,
    item: &ersatzrs_domain::ProgramScheduleItem,
) -> bool {
    let key = classic_schedule_item_key(item);
    reference
        .media_by_key
        .get(&key)
        .is_some_and(|items| !items.is_empty())
        || reference
            .playlist_items_by_key
            .get(&key)
            .is_some_and(|items| items.iter().any(|item| !item.media_items.is_empty()))
}

/// Drop Remote Streams that cannot be placed on a timeline, mirroring the
/// `zeroItems` pass in C# `PlayoutBuilder.CheckForEmptyCollections`.
///
/// `ErsatzTV` logs and removes each media item whose duration is unknown or
/// zero and keeps building; only a collection left with no items at all stops
/// the build. A Remote Stream qualifies when its playback behavior is unknown,
/// or when it is finite without a positive duration — the C# `isZero` arm for
/// `RemoteStream`. Returning the removals as warnings keeps them visible on the
/// playout instead of failing the whole channel over one unusable item.
fn drop_unschedulable_remote_streams(reference: &mut SchedulingReferenceData) -> Vec<String> {
    let unschedulable = scheduling_media_items(reference)
        .filter(|item| match item.liveness {
            Some(RemoteStreamLiveness::Unknown) => true,
            Some(RemoteStreamLiveness::Finite) => item.duration <= Duration::zero(),
            Some(RemoteStreamLiveness::Live) | None => false,
        })
        .map(|item| item.id)
        .collect::<BTreeSet<_>>();
    if unschedulable.is_empty() {
        return Vec::new();
    }

    let retain = |items: &mut Vec<SchedulingMediaItem>| {
        items.retain(|item| !unschedulable.contains(&item.id));
    };
    reference
        .media_items_by_id
        .retain(|id, _| !unschedulable.contains(id));
    for items in reference
        .media_by_key
        .values_mut()
        .chain(reference.collections_by_name.values_mut())
        .chain(reference.multi_collections_by_name.values_mut())
        .chain(reference.smart_collections_by_name.values_mut())
        .chain(reference.playlists_by_group_and_name.values_mut())
        .chain(reference.searches_by_query.values_mut())
        .chain(reference.shows_by_name.values_mut())
        .chain(reference.media_by_show_guid.values_mut())
    {
        retain(items);
    }
    for playlist in reference
        .playlist_items_by_group_and_name
        .values_mut()
        .chain(reference.playlist_items_by_key.values_mut())
        .flatten()
    {
        retain(&mut playlist.media_items);
    }
    for preset in reference.classic_filler_presets.values_mut() {
        retain(&mut preset.media_items);
    }
    if let Some(fallback) = reference.global_fallback.as_mut() {
        retain(&mut fallback.media_items);
    }

    unschedulable
        .into_iter()
        .map(|id| format!("Skipped Remote Stream {id} with unknown or zero duration"))
        .collect()
}

fn validate_remote_stream_placements(request: &SchedulingRequest) -> Result<(), SchedulingError> {
    let items = unique_scheduling_media_items(&request.reference_data);
    // Unknown and zero-duration Remote Streams are already gone: they are
    // dropped by `drop_unschedulable_remote_streams` the way C#
    // `CheckForEmptyCollections` drops `zeroItems`. Anything still carrying
    // them here reached the engine without that pass, so refusing stays the
    // safe answer rather than scheduling an item with no known length.
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

/// Visit every media item reachable from the reference data, including
/// duplicates across collections.
fn scheduling_media_items(
    reference: &SchedulingReferenceData,
) -> impl Iterator<Item = &SchedulingMediaItem> {
    reference
        .media_items_by_id
        .values()
        .chain(
            reference
                .media_by_key
                .values()
                .chain(reference.collections_by_name.values())
                .chain(reference.multi_collections_by_name.values())
                .chain(reference.smart_collections_by_name.values())
                .chain(reference.playlists_by_group_and_name.values())
                .chain(reference.searches_by_query.values())
                .chain(reference.shows_by_name.values())
                .chain(reference.media_by_show_guid.values())
                .flatten(),
        )
        .chain(
            reference
                .playlist_items_by_group_and_name
                .values()
                .chain(reference.playlist_items_by_key.values())
                .flatten()
                .flat_map(|playlist| &playlist.media_items),
        )
        .chain(
            reference
                .classic_filler_presets
                .values()
                .flat_map(|preset| &preset.media_items),
        )
        .chain(
            reference
                .global_fallback
                .iter()
                .flat_map(|fallback| &fallback.media_items),
        )
}

fn unique_scheduling_media_items(
    reference: &SchedulingReferenceData,
) -> BTreeMap<EntityId, SchedulingMediaItem> {
    let mut result = BTreeMap::new();
    for item in scheduling_media_items(reference) {
        result.entry(item.id).or_insert_with(|| item.clone());
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
        FillerKind, FillerMode, Playout, PlayoutGap, PlayoutMode, PlayoutScheduleKind,
        ProgramScheduleItem, RemoteStreamLiveness,
    };

    use super::*;
    use crate::scheduling::{
        PlayoutBuildMode, SchedulingFallbackPreset, SchedulingFillerPreset, SchedulingMediaItem,
        SchedulingReferenceData,
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
        let unknown_result = PlayoutBuilder::default()
            .build(unknown_request)
            .expect("an unschedulable Remote Stream is dropped, not fatal");
        assert!(unknown_result.added_items.is_empty());
        assert_eq!(
            unknown_result.warnings,
            vec!["Skipped Remote Stream 99 with unknown or zero duration"]
        );

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
        let finite_result = PlayoutBuilder::default()
            .build(finite_request)
            .expect("a durationless finite Remote Stream is dropped, not fatal");
        assert!(finite_result.added_items.is_empty());
        assert_eq!(
            finite_result.warnings,
            vec!["Skipped Remote Stream 100 with unknown or zero duration"]
        );
    }

    /// Regression: one unschedulable Remote Stream must not take down the
    /// whole channel.
    ///
    /// C# `PlayoutBuilder.CheckForEmptyCollections` drops each zero-duration
    /// item from its collection and keeps building. ErsatzRS used to refuse the
    /// entire build with `UnknownRemoteStreamLiveness`, so a single add-on
    /// episode that resolved without a duration left its channel with no
    /// playout items at all and nothing to stream.
    #[test]
    fn one_unschedulable_remote_stream_does_not_block_its_collection() {
        let start = Utc::now();
        let mut unschedulable = SchedulingMediaItem::new(99, Duration::zero());
        unschedulable.liveness = Some(RemoteStreamLiveness::Unknown);
        let schedule_item = ProgramScheduleItem {
            id: 7,
            playout_mode: PlayoutMode::One,
            ..ProgramScheduleItem::default()
        };
        let key = classic_schedule_item_key(&schedule_item);
        let result = PlayoutBuilder::default()
            .build(SchedulingRequest {
                playout: Playout {
                    schedule_kind: PlayoutScheduleKind::Classic,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::hours(3),
                mode: PlayoutBuildMode::Continue,
                reference_data: SchedulingReferenceData {
                    classic_schedule_items: vec![schedule_item],
                    media_by_key: [(
                        key,
                        vec![
                            SchedulingMediaItem::new(1, Duration::minutes(30)),
                            unschedulable,
                            SchedulingMediaItem::new(2, Duration::minutes(30)),
                        ],
                    )]
                    .into(),
                    ..SchedulingReferenceData::default()
                },
            })
            .expect("the healthy items in the collection must still schedule");

        assert!(
            !result.added_items.is_empty(),
            "the channel must still receive a playout window"
        );
        assert!(
            result
                .added_items
                .iter()
                .all(|item| item.media_item_id != 99),
            "the unschedulable Remote Stream must never be placed"
        );
        assert_eq!(
            result.warnings,
            vec!["Skipped Remote Stream 99 with unknown or zero duration"]
        );
    }

    #[test]
    fn required_classic_source_emptied_by_filter_is_refused_before_destructive_build() {
        let start = Utc::now();
        let mut unschedulable = SchedulingMediaItem::new(99, Duration::zero());
        unschedulable.liveness = Some(RemoteStreamLiveness::Unknown);
        let schedule_item = ProgramScheduleItem {
            id: 7,
            playout_mode: PlayoutMode::One,
            ..ProgramScheduleItem::default()
        };
        let key = classic_schedule_item_key(&schedule_item);

        for mode in [PlayoutBuildMode::Reset, PlayoutBuildMode::Refresh] {
            let error = PlayoutBuilder::default()
                .build(SchedulingRequest {
                    playout: Playout {
                        schedule_kind: PlayoutScheduleKind::Classic,
                        ..Playout::default()
                    },
                    start,
                    finish: start + Duration::hours(3),
                    mode,
                    reference_data: SchedulingReferenceData {
                        classic_schedule_items: vec![schedule_item.clone()],
                        media_by_key: [(key.clone(), vec![unschedulable.clone()])].into(),
                        ..SchedulingReferenceData::default()
                    },
                })
                .expect_err("an emptied required source must refuse the build");

            assert!(matches!(
                error,
                SchedulingError::EmptyClassicSourceAfterRemoteStreamFilter(7)
            ));
        }
    }

    #[test]
    fn unschedulable_optional_filler_does_not_block_healthy_classic_source() {
        let start = Utc::now();
        let mut unschedulable = SchedulingMediaItem::new(99, Duration::zero());
        unschedulable.liveness = Some(RemoteStreamLiveness::Unknown);
        let schedule_item = ProgramScheduleItem {
            id: 7,
            playout_mode: PlayoutMode::One,
            ..ProgramScheduleItem::default()
        };
        let key = classic_schedule_item_key(&schedule_item);

        let result = PlayoutBuilder::default()
            .build(SchedulingRequest {
                playout: Playout {
                    schedule_kind: PlayoutScheduleKind::Classic,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::hours(1),
                mode: PlayoutBuildMode::Reset,
                reference_data: SchedulingReferenceData {
                    classic_schedule_items: vec![schedule_item],
                    media_by_key: [(
                        key,
                        vec![SchedulingMediaItem::new(1, Duration::minutes(30))],
                    )]
                    .into(),
                    classic_filler_presets: [(
                        8,
                        SchedulingFillerPreset {
                            id: 8,
                            filler_kind: FillerKind::PostRoll,
                            filler_mode: FillerMode::Count,
                            duration: None,
                            count: Some(1),
                            pad_to_nearest_minute: None,
                            allow_watermarks: false,
                            media_items: vec![unschedulable],
                        },
                    )]
                    .into(),
                    ..SchedulingReferenceData::default()
                },
            })
            .expect("optional filler filtering must not refuse healthy primary content");

        assert!(!result.added_items.is_empty());
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
