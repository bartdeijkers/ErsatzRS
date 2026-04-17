//! Top-level playout build orchestrator.
//!
//! Mirrors the C# `IPlayoutBuilder` dispatch hierarchy:
//! - `Block`      → [`BlockSchedulingEngine`]
//! - `Sequential` → [`YamlPlayoutEngine`]
//! - `Scripted`   → [`ScriptedSchedulingEngine`]
//!
//! `Classic` and `ExternalJson` are not yet implemented; calls for those kinds
//! return [`SchedulingError::UnsupportedScheduleKind`].
//!
//! The C# entry point for the Classic engine is
//! `ErsatzTV.Core/Scheduling/PlayoutBuilder.cs`.  Block, YAML, and Scripted
//! counterparts are in the `BlockScheduling/` and `YamlScheduling/` and
//! `ScriptedScheduling/` sub-namespaces respectively.

use ersatzrs_domain::PlayoutScheduleKind;

use crate::scheduling::{
    BlockSchedulingEngine, SchedulingError, SchedulingRequest, SchedulingResult,
    ScriptedSchedulingEngine, YamlPlayoutEngine,
};

/// Routes playout build requests to the engine registered for each
/// [`PlayoutScheduleKind`].
///
/// Construct via [`PlayoutBuilder::default`] to use the built-in engines, or
/// via [`PlayoutBuilder::new`] to inject custom implementations.
pub struct PlayoutBuilder {
    block: Box<dyn EngineDispatch + Send + Sync>,
    yaml: Box<dyn EngineDispatch + Send + Sync>,
    scripted: Box<dyn EngineDispatch + Send + Sync>,
}

impl std::fmt::Debug for PlayoutBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlayoutBuilder").finish_non_exhaustive()
    }
}

impl Default for PlayoutBuilder {
    fn default() -> Self {
        Self {
            block: Box::new(BlockSchedulingEngine),
            yaml: Box::new(YamlPlayoutEngine),
            scripted: Box::new(ScriptedSchedulingEngine),
        }
    }
}

impl PlayoutBuilder {
    /// Construct a builder with one engine per supported schedule kind.
    #[must_use]
    pub fn new(
        block: Box<dyn EngineDispatch + Send + Sync>,
        yaml: Box<dyn EngineDispatch + Send + Sync>,
        scripted: Box<dyn EngineDispatch + Send + Sync>,
    ) -> Self {
        Self {
            block,
            yaml,
            scripted,
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
    /// Returns [`SchedulingError::UnsupportedScheduleKind`] for `Classic`,
    /// `ExternalJson`, and `None`, which are not implemented in Phase 3.
    pub fn build(&self, request: SchedulingRequest) -> Result<SchedulingResult, SchedulingError> {
        match request.playout.schedule_kind {
            PlayoutScheduleKind::Block => self.block.schedule(request),
            PlayoutScheduleKind::Sequential => self.yaml.schedule(request),
            PlayoutScheduleKind::Scripted => self.scripted.schedule(request),
            kind => Err(SchedulingError::UnsupportedScheduleKind(kind)),
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
