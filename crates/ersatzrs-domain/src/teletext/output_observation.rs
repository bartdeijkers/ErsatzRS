//! Ephemeral observations of individual outputs, never publication authority.

use serde::{Deserialize, Serialize};

use crate::EntityId;

/// The selected manual origin, with an optional derived guide runtime snapshot.
/// These diagnostics do not grant publication or source authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeletextOutputGenerationObservation {
    pub generation_id: EntityId,
    pub request_id: EntityId,
    pub design_id: EntityId,
    pub revision: i64,
    pub effective: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guide_refresh: Option<TeletextGuideRefreshOutputObservation>,
}

/// Explicit automatic-guide association, separate from manual generation IDs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeletextGuideRefreshOutputObservation {
    pub seed_id: EntityId,
    /// Absence means the retained initial seed, not a fabricated runtime ID.
    pub snapshot_id: Option<EntityId>,
    pub revision: i64,
    pub captured_at: chrono::DateTime<chrono::Utc>,
    pub stale: bool,
}

/// Requested or last acknowledged output, including explicit absence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeletextOutputSelectionObservation {
    /// Zero identifies the initial seed, not a later proposal.
    pub sequence: u64,
    pub generation: Option<TeletextOutputGenerationObservation>,
    pub authored: bool,
}

/// Observation state for the latest request on one physical output.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeletextOutputObservationPhase {
    Waiting,
    Applied,
    Rejected,
}

/// A response-owned snapshot. Applied means a sink acknowledgement, not rendering.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeletextOutputObservation {
    pub epoch: uuid::Uuid,
    pub requested: TeletextOutputSelectionObservation,
    /// Historical last confirmed delivery; it may differ from the latest request.
    pub applied: Option<TeletextOutputSelectionObservation>,
    pub phase: TeletextOutputObservationPhase,
}

/// Bounded host-local observations. Missing entries never imply applied output.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeletextChannelOutputObservations {
    pub outputs: Vec<TeletextOutputObservation>,
    /// The host cannot account for every active output in this snapshot.
    pub incomplete: bool,
}
