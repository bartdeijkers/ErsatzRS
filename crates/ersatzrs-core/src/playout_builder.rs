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

use chrono::Duration;
use ersatzrs_domain::{EntityId, FillerKind, PlayoutItem, PlayoutScheduleKind};

use crate::scheduling::{
    BlockSchedulingEngine, ClassicSchedulingEngine, ExternalJsonSchedulingEngine, SchedulingError,
    SchedulingFallbackPreset, SchedulingRequest, SchedulingResult, ScriptedSchedulingEngine,
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
    use ersatzrs_domain::{FillerKind, Playout, PlayoutGap, PlayoutScheduleKind};

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
