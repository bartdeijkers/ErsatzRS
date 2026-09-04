use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ersatzrs_domain::{
    AddonMediaListPublicationOutcome, AddonMediaListPublicationPhase,
    AddonMediaListPublicationState, AddonMediaListRecoveryAction, EntityId,
    ManagedLibraryOperationKind, ManagedLibraryOperationState, MediaSourceKind,
};

use super::Result;
use serde::{Deserialize, Serialize};

/// Durable scan payload stored with one managed-library operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManagedLibraryScanSpec {
    pub provider: MediaSourceKind,
    pub show_id: Option<EntityId>,
    pub deep_scan: bool,
}

/// Persisted operation summary returned to application and transport layers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagedLibraryOperation {
    pub id: EntityId,
    pub library_id: EntityId,
    pub parent_operation_id: Option<EntityId>,
    pub owner_token: String,
    pub kind: ManagedLibraryOperationKind,
    pub state: ManagedLibraryOperationState,
    pub scan: Option<ManagedLibraryScanSpec>,
    pub attempt_count: i32,
    pub next_eligible_at: Option<DateTime<Utc>>,
}

/// One transactionally acquired operation and its ephemeral fencing token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagedLibraryOperationClaim {
    pub operation: ManagedLibraryOperation,
    pub lease_owner: String,
}

/// Durable state for one exact saved add-on-list revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddonMediaListPublicationJob {
    pub id: EntityId,
    pub operation_id: EntityId,
    pub list_id: EntityId,
    pub generation: i32,
    pub saved_revision: i32,
    pub target_published_revision: i32,
    pub phase: AddonMediaListPublicationPhase,
    pub child_scan_operation_id: Option<EntityId>,
    pub original_failure_code: Option<String>,
    pub original_failure_diagnostic: Option<String>,
    pub recovery_diagnostic: Option<String>,
    pub outcome: Option<AddonMediaListPublicationOutcome>,
}

/// Result of deduplicating a publication click against persisted work.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddonMediaListPublicationEnqueue {
    pub job: AddonMediaListPublicationJob,
    pub operation: ManagedLibraryOperation,
    pub existing: bool,
}

/// Redacted operator-facing status for the newest publication generation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListPublicationStatus {
    pub job_id: EntityId,
    pub operation_id: EntityId,
    pub list_id: EntityId,
    pub generation: i32,
    pub saved_revision: i32,
    pub target_published_revision: i32,
    pub phase: AddonMediaListPublicationPhase,
    pub state: AddonMediaListPublicationState,
    pub attempt_count: i32,
    pub last_progress_at: DateTime<Utc>,
    pub diagnostic_code: Option<String>,
    pub recovery_diagnostic_present: bool,
    pub can_retry: bool,
    pub can_restore: bool,
}

/// Storage boundary for durable managed-library admission and publication.
#[async_trait]
pub trait ManagedLibraryOperationRepository: Send + Sync {
    async fn enqueue_scan(
        &self,
        kind: ManagedLibraryOperationKind,
        library_id: EntityId,
        scan: ManagedLibraryScanSpec,
        coalesce: bool,
    ) -> Result<Option<ManagedLibraryOperation>>;

    async fn enqueue_publication(
        &self,
        list_id: EntityId,
        library_id: EntityId,
        saved_revision: i32,
        target_published_revision: i32,
    ) -> Result<AddonMediaListPublicationEnqueue>;

    async fn operation(&self, operation_id: EntityId) -> Result<Option<ManagedLibraryOperation>>;

    /// Return the oldest active root scan that would coalesce a new scan for
    /// `library_id`, ordered deterministically by creation time and id.
    ///
    /// Completed and failed operations are terminal and must not be returned.
    async fn active_scan_operation(
        &self,
        library_id: EntityId,
    ) -> Result<Option<ManagedLibraryOperation>>;

    async fn claim_next(&self, now: DateTime<Utc>) -> Result<Option<ManagedLibraryOperationClaim>>;

    async fn claim_operation(
        &self,
        operation_id: EntityId,
        now: DateTime<Utc>,
    ) -> Result<Option<ManagedLibraryOperationClaim>>;

    async fn heartbeat(
        &self,
        claim: &ManagedLibraryOperationClaim,
        now: DateTime<Utc>,
    ) -> Result<bool>;

    async fn publication_job(
        &self,
        operation_id: EntityId,
    ) -> Result<Option<AddonMediaListPublicationJob>>;

    /// Return the newest generation with lease liveness classified at `now`.
    async fn publication_status(
        &self,
        list_id: EntityId,
        now: DateTime<Utc>,
    ) -> Result<Option<AddonMediaListPublicationStatus>>;

    /// Requeue one exact failed or expired generation without bypassing its
    /// list, generation, or live-lease fence.
    async fn recover_publication(
        &self,
        list_id: EntityId,
        job_id: EntityId,
        generation: i32,
        action: AddonMediaListRecoveryAction,
        now: DateTime<Utc>,
    ) -> Result<AddonMediaListPublicationStatus>;

    async fn transition_publication(
        &self,
        claim: &ManagedLibraryOperationClaim,
        expected: AddonMediaListPublicationPhase,
        next: AddonMediaListPublicationPhase,
    ) -> Result<bool>;

    async fn ensure_publication_child_scan(
        &self,
        claim: &ManagedLibraryOperationClaim,
        list_id: EntityId,
    ) -> Result<ManagedLibraryOperation>;

    /// Create or resume the post-commit/restore verification scan.
    async fn ensure_publication_verification_scan(
        &self,
        claim: &ManagedLibraryOperationClaim,
        list_id: EntityId,
    ) -> Result<ManagedLibraryOperation>;

    /// Persist the selected result before entering the shared verification phases.
    async fn begin_publication_verification(
        &self,
        claim: &ManagedLibraryOperationClaim,
        expected: AddonMediaListPublicationPhase,
        outcome: AddonMediaListPublicationOutcome,
    ) -> Result<bool>;

    async fn record_publication_failure(
        &self,
        claim: &ManagedLibraryOperationClaim,
        expected: AddonMediaListPublicationPhase,
        code: &str,
        diagnostic: &str,
    ) -> Result<bool>;

    async fn schedule_publication_recovery_retry(
        &self,
        claim: &ManagedLibraryOperationClaim,
        diagnostic: &str,
        next_eligible_at: DateTime<Utc>,
    ) -> Result<bool>;

    async fn complete_publication(
        &self,
        claim: &ManagedLibraryOperationClaim,
        outcome: AddonMediaListPublicationOutcome,
    ) -> Result<bool>;

    async fn complete_operation(&self, claim: &ManagedLibraryOperationClaim) -> Result<bool>;

    async fn fail_operation(
        &self,
        claim: &ManagedLibraryOperationClaim,
        code: &str,
        diagnostic: &str,
    ) -> Result<bool>;
}
