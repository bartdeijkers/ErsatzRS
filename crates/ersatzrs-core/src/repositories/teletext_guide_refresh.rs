//! Explicitly authorized guide refresh; manual originals remain immutable.

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use ersatzrs_domain::{
    EntityId, TeletextAcceptedGeneration, TeletextChannelSelectionSnapshot,
    TeletextEffectiveChannelSelectionSnapshot,
};

use super::{
    Result, TeletextGenerationMutation, TeletextGuideGenerationCheck,
    TeletextGuideGenerationQualification,
};

/// Runtime-snapshot framing is independent of manual generation artifact roles.
pub const TELETEXT_GUIDE_RUNTIME_SNAPSHOT_VERSION: u16 = 1;
/// Existing selected-channel bound also caps refresh discovery per call.
pub const MAX_TELETEXT_GUIDE_REFRESH_BATCH: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TeletextGuideRefreshRole {
    Base,
    Effective,
}

impl TeletextGuideRefreshRole {
    #[must_use]
    pub const fn code(self) -> i64 {
        match self {
            Self::Base => 1,
            Self::Effective => 2,
        }
    }
}

/// Immutable authorization origin, registered only by explicit guide acceptance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TeletextGuideRefreshSeed {
    pub id: EntityId,
    pub role: TeletextGuideRefreshRole,
    pub generation: TeletextAcceptedGeneration,
    pub expected_design_head: i64,
    pub qualification: TeletextGuideGenerationQualification,
}

/// No duplicate selection DTO is persisted: the bounded native document binds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TeletextGuideRefreshSelection {
    Base(TeletextChannelSelectionSnapshot),
    Effective(TeletextEffectiveChannelSelectionSnapshot),
}

/// Complete immutable output, separately framed from any manual generation.
#[derive(Clone, PartialEq, Eq)]
pub struct TeletextGuideRuntimeSnapshotSave {
    pub version: u16,
    pub document_version: u16,
    pub document: Vec<u8>,
    pub document_digest: String,
    pub qualification: TeletextGuideGenerationQualification,
    pub artifact_digest: String,
}

impl std::fmt::Debug for TeletextGuideRuntimeSnapshotSave {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TeletextGuideRuntimeSnapshotSave")
            .field("version", &self.version)
            .field("document_version", &self.document_version)
            .field("document_bytes", &self.document.len())
            .field("qualification", &self.qualification)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TeletextGuideRuntimeSnapshot {
    pub id: EntityId,
    pub seed_id: EntityId,
    pub saved: TeletextGuideRuntimeSnapshotSave,
}

/// Cheap immutable identity/qualification observation; never contains document bytes.
#[derive(Clone, PartialEq, Eq)]
pub struct TeletextGuideRuntimeSnapshotMetadata {
    pub id: EntityId,
    pub seed_id: EntityId,
    pub version: u16,
    pub document_version: u16,
    pub document_digest: String,
    pub qualification: TeletextGuideGenerationQualification,
    pub artifact_digest: String,
}

impl std::fmt::Debug for TeletextGuideRuntimeSnapshotMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TeletextGuideRuntimeSnapshotMetadata")
            .field("id", &self.id)
            .field("seed_id", &self.seed_id)
            .field("version", &self.version)
            .field("document_version", &self.document_version)
            .field("qualification", &self.qualification)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TeletextGuideRefreshCurrentMetadata {
    pub seed: TeletextGuideRefreshSeed,
    pub revision: i64,
    pub runtime: Option<TeletextGuideRuntimeSnapshotMetadata>,
    pub next_due_at: DateTime<Utc>,
    pub last_success_at: DateTime<Utc>,
    pub first_failure_at: Option<DateTime<Utc>>,
    pub failure: Option<TeletextGuideRefreshFailure>,
    pub revoked: bool,
    /// A persisted owner still has work or unconfirmed cleanup; not a failure.
    pub refreshing: bool,
}

impl TeletextGuideRefreshCurrentMetadata {
    #[must_use]
    pub fn available_until(&self) -> DateTime<Utc> {
        let original = self
            .runtime
            .as_ref()
            .map_or(self.seed.qualification.valid_until, |snapshot| {
                snapshot.qualification.valid_until
            });
        teletext_guide_refresh_stale_ceiling(original, self.first_failure_at)
    }

    #[must_use]
    pub fn status(&self, now: DateTime<Utc>) -> TeletextGuideRefreshStatus {
        let captured = self
            .runtime
            .as_ref()
            .map_or(self.seed.qualification.source.captured_at, |snapshot| {
                snapshot.qualification.source.captured_at
            });
        teletext_guide_refresh_status(
            now,
            captured,
            self.available_until(),
            self.revoked,
            self.refreshing,
            self.first_failure_at,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TeletextGuideRefreshCandidate {
    pub snapshot: TeletextGuideRuntimeSnapshotSave,
    pub selection: TeletextGuideRefreshSelection,
}

/// Persisted, redacted reason; never a producer stderr or source-bearing string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TeletextGuideRefreshFailure {
    /// Source freshness is unavailable; this does not imply producer failure.
    SourceUnavailable,
    PreparationFailed,
    NeedsReview,
    Disabled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TeletextGuideRefreshStatus {
    Fresh,
    Stale,
    Unavailable,
    NeedsReview,
    Revoked,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TeletextGuideRefreshCurrent {
    pub seed: TeletextGuideRefreshSeed,
    pub revision: i64,
    pub runtime: Option<TeletextGuideRuntimeSnapshot>,
    pub next_due_at: DateTime<Utc>,
    pub last_success_at: DateTime<Utc>,
    pub first_failure_at: Option<DateTime<Utc>>,
    pub failure: Option<TeletextGuideRefreshFailure>,
    pub revoked: bool,
    /// A persisted owner still has work or unconfirmed cleanup; not a failure.
    pub refreshing: bool,
}

impl TeletextGuideRefreshCurrent {
    /// Serving is an explicit stale authority, not a claim that old sources match.
    /// Source/role/scope expiry is never renewed by failure, retry or restart.
    #[must_use]
    pub fn available_until(&self) -> DateTime<Utc> {
        let original = self
            .runtime
            .as_ref()
            .map_or(self.seed.qualification.valid_until, |snapshot| {
                snapshot.saved.qualification.valid_until
            });
        teletext_guide_refresh_stale_ceiling(original, self.first_failure_at)
    }

    #[must_use]
    pub fn status(&self, now: DateTime<Utc>) -> TeletextGuideRefreshStatus {
        let captured = self
            .runtime
            .as_ref()
            .map_or(self.seed.qualification.source.captured_at, |snapshot| {
                snapshot.saved.qualification.source.captured_at
            });
        teletext_guide_refresh_status(
            now,
            captured,
            self.available_until(),
            self.revoked,
            self.refreshing,
            self.first_failure_at,
        )
    }
}

/// Classify a stored last-good cap without extending its original role boundary.
#[must_use]
pub fn teletext_guide_refresh_stale_ceiling(
    original: DateTime<Utc>,
    first_failure: Option<DateTime<Utc>>,
) -> DateTime<Utc> {
    first_failure
        .and_then(|at| at.checked_add_signed(Duration::minutes(15)))
        .map_or(original, |ceiling| original.min(ceiling))
}

/// Pure maintenance classification only; caller must validate metadata and
/// separately establish database/native/output authority.
#[must_use]
pub fn teletext_guide_refresh_status(
    now: DateTime<Utc>,
    captured: DateTime<Utc>,
    until: DateTime<Utc>,
    revoked: bool,
    refreshing: bool,
    first_failure: Option<DateTime<Utc>>,
) -> TeletextGuideRefreshStatus {
    if revoked {
        return TeletextGuideRefreshStatus::Revoked;
    }
    if now < captured || now >= until {
        return TeletextGuideRefreshStatus::Unavailable;
    }
    if first_failure.is_some() || refreshing {
        TeletextGuideRefreshStatus::Stale
    } else {
        TeletextGuideRefreshStatus::Fresh
    }
}

/// Durable current revision and a fresh lease token fence actual completion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TeletextGuideRefreshClaim {
    pub seed: TeletextGuideRefreshSeed,
    pub revision: i64,
    pub expected_runtime_id: Option<EntityId>,
    pub lease_owner: String,
    pub lease_until: DateTime<Utc>,
}

/// Exact native cached-output association; neither a seed nor its generation ID
/// stands in for a separately compiled runtime snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TeletextGuideRefreshOutputConfirmation {
    pub seed: TeletextGuideRefreshSeed,
    pub current_revision: i64,
    pub runtime: Option<TeletextGuideRuntimeSnapshotMetadata>,
    pub selection: TeletextGuideRefreshSelection,
    pub qualification: TeletextGuideGenerationQualification,
    pub status: TeletextGuideRefreshStatus,
    pub available_until: DateTime<Utc>,
}

/// Fresh output hashes current sources under real admitted work. Already sealed
/// stale output has only bounded read-only authority and acquires no new slot.
#[derive(Clone)]
pub enum TeletextGuideRefreshOutputCheck {
    Fresh(TeletextGuideGenerationCheck),
    Stale { deadline: std::time::Instant },
}

impl TeletextGuideRefreshOutputCheck {
    #[must_use]
    pub const fn deadline(&self) -> std::time::Instant {
        match self {
            Self::Fresh(check) => check.deadline,
            Self::Stale { deadline } => *deadline,
        }
    }
}

/// Additive optional port: old adapters remain explicitly unsupported.
#[async_trait]
pub trait TeletextGuideRefreshRepository: Send + Sync {
    async fn confirm_output(
        &self,
        confirmation: &TeletextGuideRefreshOutputConfirmation,
        now: DateTime<Utc>,
        check: &TeletextGuideRefreshOutputCheck,
        max_metadata_bytes: usize,
    ) -> Result<TeletextGenerationMutation<()>>;
    /// Bounded keyset discovery includes future origins without loading documents.
    async fn scan_current(
        &self,
        after_seed_id: Option<EntityId>,
        limit: usize,
    ) -> Result<Vec<TeletextGuideRefreshSeed>>;
    async fn observe_current(
        &self,
        channel_id: EntityId,
        role: TeletextGuideRefreshRole,
        max_metadata_bytes: usize,
    ) -> Result<Option<TeletextGuideRefreshCurrentMetadata>>;
    async fn current(
        &self,
        channel_id: EntityId,
        role: TeletextGuideRefreshRole,
        max_document_bytes: usize,
    ) -> Result<Option<TeletextGuideRefreshCurrent>>;
    async fn due(&self, now: DateTime<Utc>, limit: usize) -> Result<Vec<TeletextGuideRefreshSeed>>;
    async fn changed(
        &self,
        seed_id: EntityId,
        now: DateTime<Utc>,
    ) -> Result<TeletextGenerationMutation<()>>;
    async fn claim(
        &self,
        seed_id: EntityId,
        now: DateTime<Utc>,
        lease_until: DateTime<Utc>,
        check: &TeletextGuideGenerationCheck,
        max_document_bytes: usize,
    ) -> Result<TeletextGenerationMutation<Option<TeletextGuideRefreshClaim>>>;
    async fn heartbeat(
        &self,
        claim: &TeletextGuideRefreshClaim,
        now: DateTime<Utc>,
        lease_until: DateTime<Utc>,
        check: &TeletextGuideGenerationCheck,
    ) -> Result<TeletextGenerationMutation<()>>;
    async fn complete(
        &self,
        claim: &TeletextGuideRefreshClaim,
        candidate: &TeletextGuideRefreshCandidate,
        now: DateTime<Utc>,
        check: &TeletextGuideGenerationCheck,
        max_document_bytes: usize,
    ) -> Result<TeletextGenerationMutation<TeletextGuideRuntimeSnapshot>>;
    async fn fail(
        &self,
        claim: &TeletextGuideRefreshClaim,
        now: DateTime<Utc>,
        next_due_at: DateTime<Utc>,
        failure: TeletextGuideRefreshFailure,
    ) -> Result<TeletextGenerationMutation<()>>;
    async fn revoke(
        &self,
        seed_id: EntityId,
        now: DateTime<Utc>,
    ) -> Result<TeletextGenerationMutation<()>>;
}
