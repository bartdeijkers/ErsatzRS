//! Ephemeral observations of individual outputs, never publication authority.

use serde::{Deserialize, Serialize};

use crate::EntityId;

/// A sealed generation associated with a proposal by the native runtime.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeletextOutputGenerationObservation {
    pub generation_id: EntityId,
    pub request_id: EntityId,
    pub design_id: EntityId,
    pub revision: i64,
    pub effective: bool,
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
