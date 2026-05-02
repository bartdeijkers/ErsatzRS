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

use ersatzrs_domain::PlayoutScheduleKind;

use crate::scheduling::{
    BlockSchedulingEngine, ClassicSchedulingEngine, ExternalJsonSchedulingEngine, SchedulingError,
    SchedulingRequest, SchedulingResult, ScriptedSchedulingEngine, YamlPlayoutEngine,
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
        match request.playout.schedule_kind {
            PlayoutScheduleKind::Classic => self.classic.schedule(request),
            PlayoutScheduleKind::Block => self.block.schedule(request),
            PlayoutScheduleKind::Sequential => self.yaml.schedule(request),
            PlayoutScheduleKind::Scripted => self.scripted.schedule(request),
            PlayoutScheduleKind::ExternalJson => self.external_json.schedule(request),
            PlayoutScheduleKind::None => Err(SchedulingError::UnsupportedScheduleKind(
                PlayoutScheduleKind::None,
            )),
        }
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

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use ersatzrs_domain::{Playout, PlayoutScheduleKind};

    use super::*;
    use crate::scheduling::{PlayoutBuildMode, SchedulingReferenceData};

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
}
