use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ersatzrs_domain::EntityId;
use serde::{Deserialize, Serialize};

use super::Result;

/// Durable state exposed for the latest non-mutating source probe.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListSourceProbeState {
    Queued,
    Running,
    Complete,
    /// The saved inputs no longer match the list, package, or defaults.
    Stale,
}

/// Typed access result from one bounded header and first-page probe.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListSourceProbeOutcome {
    Available,
    Unsupported,
    Failed,
}

/// Operator-safe projection of one source probe.
///
/// The source URL and input fingerprint intentionally stay behind the
/// repository boundary. `State::Stale` is the only verdict consumers need.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListSourceProbe {
    pub id: EntityId,
    pub list_id: EntityId,
    pub state: AddonMediaListSourceProbeState,
    pub saved_draft_revision: i32,
    /// Opaque provider revision returned by the bounded first page.
    pub source_revision: Option<String>,
    pub capabilities: Vec<String>,
    pub outcome: Option<AddonMediaListSourceProbeOutcome>,
    pub provider_id: Option<String>,
    pub list_title: Option<String>,
    pub count_hint: Option<u32>,
    pub returned_item_count: Option<u32>,
    pub page_complete: Option<bool>,
    pub duration_milliseconds: Option<u64>,
    pub diagnostic_code: Option<String>,
    pub diagnostic: Option<String>,
    pub attempt_count: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// One dependency for enqueueing, observing, and hosting durable source probes.
#[async_trait]
pub trait AddonMediaListSourceProbeRepository: Send + Sync {
    /// Enqueue a probe only for the exact draft revision shown to the operator.
    async fn enqueue_source_probe(
        &self,
        list_id: EntityId,
        expected_draft_revision: i32,
    ) -> Result<EntityId>;

    /// Return the latest probe, with saved-input freshness evaluated now.
    async fn source_probe_status(
        &self,
        list_id: EntityId,
    ) -> Result<Option<AddonMediaListSourceProbe>>;

    /// Claim and process at most one bounded source-probe job.
    async fn process_one_source_probe(&self, now: DateTime<Utc>) -> Result<bool>;
}
