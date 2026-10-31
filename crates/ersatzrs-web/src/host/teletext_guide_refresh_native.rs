//! Native guide runtime snapshots, distinct from immutable manual authority.

use std::{fmt::Write as _, sync::Arc, time::Instant};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ersatzrs_app::{
    teletext_design_documents::TeletextDesignDocumentBudget,
    teletext_guide_refresh::TeletextGuideRefreshValidator,
    teletext_publications::TeletextPublicationValidationError, ApplicationServices,
};
use ersatzrs_core::{
    locking::TeletextWorkHold,
    repositories::{
        TeletextGuideGenerationCheck, TeletextGuideGenerationQualification,
        TeletextGuideRefreshCandidate, TeletextGuideRefreshClaim, TeletextGuideRefreshFailure,
        TeletextGuideRefreshRole, TeletextGuideRefreshSeed, TeletextGuideRefreshSelection,
        TeletextGuideRefreshStatus, TeletextGuideRuntimeSnapshot, TeletextGuideRuntimeSnapshotSave,
        TeletextGuideSourceCheckLimits, TELETEXT_GUIDE_RUNTIME_SNAPSHOT_VERSION,
    },
};
use ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService;
use ersatzrs_infra::db::DbPool;
use sha2::{Digest as _, Sha256};

use super::{
    teletext_authoring::{
        prepare_guide_refresh_baseline, GuideRefreshPreparationContext, TeletextAuthoringError,
        TeletextAuthoringStatus,
    },
    teletext_channel_documents::TeletextChannelDocumentEncodingPolicy,
    teletext_guide_refresh_editorial::{
        load_guide_refresh_editorial_intent, GuideRefreshEditorialError,
    },
    teletext_work,
};

mod compilation;
mod document;
mod seed;
pub(crate) use seed::reload_guide_refresh_seed;

/// Redacted outcome categories, independent of producer stderr/source data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum GuideRefreshNativeError {
    #[error("guide refresh requires review")]
    NeedsReview,
    #[error("guide refresh source is unavailable")]
    Unavailable,
    #[error("guide refresh operation expired")]
    TimedOut,
    #[error("guide refresh work is busy")]
    Busy,
    #[error("guide refresh policy is unavailable or invalid")]
    InvalidPolicy,
    #[error("guide refresh producer failed")]
    PreparationFailed,
    #[error("guide refresh native worker failed")]
    WorkerFailed,
}
impl GuideRefreshNativeError {
    pub(super) fn check_deadline(deadline: Instant) -> Result<(), Self> {
        if Instant::now() >= deadline {
            Err(Self::TimedOut)
        } else {
            Ok(())
        }
    }

    pub(super) fn from_authoring(error: &TeletextAuthoringError) -> Self {
        match error.status {
            TeletextAuthoringStatus::TimedOut => Self::TimedOut,
            TeletextAuthoringStatus::Busy => Self::Busy,
            TeletextAuthoringStatus::MissingPolicy | TeletextAuthoringStatus::InvalidPolicy => {
                Self::InvalidPolicy
            }
            TeletextAuthoringStatus::WorkerFailed => Self::WorkerFailed,
            TeletextAuthoringStatus::ProducerFailed => Self::PreparationFailed,
            TeletextAuthoringStatus::Unavailable | TeletextAuthoringStatus::StorageFailed => {
                Self::Unavailable
            }
            _ => Self::NeedsReview,
        }
    }

    pub(super) fn from_design(error: &super::teletext_designs::NativeTeletextDesignError) -> Self {
        use super::teletext_designs::NativeTeletextDesignError;
        match error {
            NativeTeletextDesignError::TimedOut => Self::TimedOut,
            NativeTeletextDesignError::WorkerFailed => Self::WorkerFailed,
            NativeTeletextDesignError::Storage(_) => Self::Unavailable,
            _ => Self::NeedsReview,
        }
    }

    pub(crate) const fn failure(self) -> TeletextGuideRefreshFailure {
        match self {
            Self::NeedsReview => TeletextGuideRefreshFailure::NeedsReview,
            Self::Unavailable | Self::InvalidPolicy => {
                TeletextGuideRefreshFailure::SourceUnavailable
            }
            Self::TimedOut | Self::Busy | Self::PreparationFailed | Self::WorkerFailed => {
                TeletextGuideRefreshFailure::PreparationFailed
            }
        }
    }
}
impl From<GuideRefreshEditorialError> for GuideRefreshNativeError {
    fn from(error: GuideRefreshEditorialError) -> Self {
        match error {
            GuideRefreshEditorialError::TimedOut => Self::TimedOut,
            GuideRefreshEditorialError::Worker => Self::WorkerFailed,
            GuideRefreshEditorialError::Storage => Self::Unavailable,
            GuideRefreshEditorialError::NeedsReview(_)
            | GuideRefreshEditorialError::Authentication(_) => Self::NeedsReview,
        }
    }
}

pub(super) struct FreshGuideBaseline {
    pub(super) document_version: u16,
    pub(super) document: Vec<u8>,
    pub(super) qualification: TeletextGuideGenerationQualification,
    pub(super) selection: TeletextGuideRefreshSelection,
}

/// Native compilation evidence; only this module constructs successful values.
/// A completed preparation is not itself durable publication or stream authority.
pub(crate) struct PreparedGuideRefresh {
    claim: TeletextGuideRefreshClaim,
    candidate: TeletextGuideRefreshCandidate,
    deadline: Instant,
    output_deadline: Instant,
    limits: TeletextGuideSourceCheckLimits,
    hold: TeletextWorkHold,
}
impl PreparedGuideRefresh {
    pub(crate) fn candidate(&self) -> &TeletextGuideRefreshCandidate {
        &self.candidate
    }
    pub(crate) fn source_check(&self) -> TeletextGuideGenerationCheck {
        TeletextGuideGenerationCheck {
            limits: self.limits,
            deadline: self.deadline.min(self.output_deadline),
            hold: self.hold.clone(),
        }
    }
}
#[async_trait]
impl TeletextGuideRefreshValidator for PreparedGuideRefresh {
    async fn confirm_guide_refresh(
        &self,
        claim: &TeletextGuideRefreshClaim,
        candidate: &TeletextGuideRefreshCandidate,
        deadline: Instant,
    ) -> Result<(), TeletextPublicationValidationError> {
        let deadline = deadline.min(self.deadline).min(self.output_deadline);
        if Instant::now() >= deadline {
            return Err(TeletextPublicationValidationError::TimedOut);
        }
        if !same_claim(&self.claim, claim) || &self.candidate != candidate {
            return Err(TeletextPublicationValidationError::Association);
        }
        candidate
            .snapshot
            .qualification
            .validate_current(Utc::now())
            .map_err(|_| TeletextPublicationValidationError::Association)?;
        if Instant::now() >= deadline {
            return Err(TeletextPublicationValidationError::TimedOut);
        }
        Ok(())
    }
}
fn same_claim(left: &TeletextGuideRefreshClaim, right: &TeletextGuideRefreshClaim) -> bool {
    // Heartbeats may extend the persisted lease but never this operation's budget.
    left.seed == right.seed
        && left.revision == right.revision
        && left.expected_runtime_id == right.expected_runtime_id
        && left.lease_owner == right.lease_owner
}

pub(crate) async fn prepare_current_guide_refresh(
    application: &ApplicationServices,
    pool: &DbPool,
    claim: &TeletextGuideRefreshClaim,
    context: GuideRefreshPreparationContext,
) -> Result<PreparedGuideRefresh, GuideRefreshNativeError> {
    GuideRefreshNativeError::check_deadline(context.deadline())?;
    let encoding = context.encoding_policy();
    // Authenticate old edits before invoking a producer; unsupported intent is
    // retained and refused, never silently removed from a regenerated guide.
    let intent = load_guide_refresh_editorial_intent(
        application,
        &claim.seed,
        context.design_budget(),
        encoding.clone(),
        context.deadline(),
        context.hold(),
    )
    .await?;
    let baseline = prepare_guide_refresh_baseline(application, pool, claim, &context).await?;
    let limits = context.guide_limits()?;
    let budget = context.design_budget();
    let deadline = context.deadline();
    let seed = claim.seed.clone();
    let (candidate, output_deadline) = teletext_work::spawn_blocking(context.hold(), move || {
        GuideRefreshNativeError::check_deadline(deadline)?;
        let document = document::encode(&seed, &baseline, budget.max_serialized_bytes)?;
        let decoded = compilation::decode(&seed, &baseline, budget)?;
        let service = compilation::compile(decoded, intent.as_ref(), &encoding, deadline)?;
        let output_deadline =
            output_deadline(&baseline.qualification, baseline.qualification.valid_until)?;
        let document_digest = digest(&document);
        let artifact_digest = fingerprint(
            &seed,
            &baseline.qualification,
            &document_digest,
            &service,
            deadline,
        )?;
        let candidate = TeletextGuideRefreshCandidate {
            selection: baseline.selection,
            snapshot: TeletextGuideRuntimeSnapshotSave {
                version: TELETEXT_GUIDE_RUNTIME_SNAPSHOT_VERSION,
                document_version: document::VERSION,
                document,
                document_digest,
                qualification: baseline.qualification,
                artifact_digest,
            },
        };
        GuideRefreshNativeError::check_deadline(deadline)?;
        Ok::<_, GuideRefreshNativeError>((candidate, output_deadline))
    })
    .await
    .map_err(|_| GuideRefreshNativeError::WorkerFailed)??;
    Ok(PreparedGuideRefresh {
        claim: claim.clone(),
        candidate,
        deadline,
        output_deadline,
        limits,
        hold: context.hold(),
    })
}

/// All limits/deadlines come from the current explicit runtime4 owner.
pub(crate) struct GuideRefreshReloadContext<'a> {
    pub(crate) application: &'a ApplicationServices,
    pub(crate) pool: &'a DbPool,
    pub(crate) budget: TeletextDesignDocumentBudget,
    pub(crate) encoding: TeletextChannelDocumentEncodingPolicy,
    pub(crate) limits: TeletextGuideSourceCheckLimits,
    pub(crate) deadline: Instant,
    pub(crate) hold: TeletextWorkHold,
}

/// Read only the privately framed configuration on an identity-cache miss.
/// The caller must bind this snapshot to its current fenced metadata before use.
/// This does not invoke a producer or establish publication/output authority.
pub(crate) async fn observe_guide_refresh_snapshot_configuration(
    seed: &TeletextGuideRefreshSeed,
    snapshot: &TeletextGuideRuntimeSnapshot,
    budget: TeletextDesignDocumentBudget,
    deadline: Instant,
    hold: TeletextWorkHold,
) -> Result<(TeletextGuideRefreshSelection, usize), GuideRefreshNativeError> {
    GuideRefreshNativeError::check_deadline(deadline)?;
    if seed.id <= 0
        || snapshot.id <= 0
        || snapshot.seed_id != seed.id
        || snapshot.saved.version != TELETEXT_GUIDE_RUNTIME_SNAPSHOT_VERSION
        || snapshot.saved.document.len() > budget.max_serialized_bytes
    {
        return Err(GuideRefreshNativeError::NeedsReview);
    }
    let seed = seed.clone();
    let snapshot = snapshot.clone();
    teletext_work::spawn_blocking(hold, move || {
        GuideRefreshNativeError::check_deadline(deadline)?;
        if snapshot.saved.document_digest != digest(&snapshot.saved.document) {
            return Err(GuideRefreshNativeError::NeedsReview);
        }
        let charge = snapshot.saved.document.len();
        let baseline = document::decode(&seed, &snapshot, budget)?;
        let channel = match (&baseline.selection, seed.role) {
            (TeletextGuideRefreshSelection::Base(selection), TeletextGuideRefreshRole::Base) => {
                selection.channel_id
            }
            (
                TeletextGuideRefreshSelection::Effective(selection),
                TeletextGuideRefreshRole::Effective,
            ) => selection.channel_id,
            _ => return Err(GuideRefreshNativeError::NeedsReview),
        };
        if channel != seed.generation.request.channel_id
            || baseline.qualification.source.channel_ids != seed.qualification.source.channel_ids
        {
            return Err(GuideRefreshNativeError::NeedsReview);
        }
        GuideRefreshNativeError::check_deadline(deadline)?;
        Ok((baseline.selection, charge))
    })
    .await
    .map_err(|_| GuideRefreshNativeError::WorkerFailed)?
}

/// Recompiled bytes still require the caller's current seed/snapshot/config CAS.
/// No preparation work hold is retained for the duration of playback.
pub(crate) struct NativeGuideRuntimeOutput {
    seed: TeletextGuideRefreshSeed,
    origin: NativeGuideOutputOrigin,
    selection: TeletextGuideRefreshSelection,
    service: Arc<EncodedTeletextAuthoredService>,
    deadline: Instant,
    status: TeletextGuideRefreshStatus,
    available_until: DateTime<Utc>,
}
enum NativeGuideOutputOrigin {
    Seed,
    Runtime(Box<TeletextGuideRuntimeSnapshot>),
}
impl NativeGuideRuntimeOutput {
    pub(crate) const fn seed(&self) -> &TeletextGuideRefreshSeed {
        &self.seed
    }
    pub(crate) fn snapshot(&self) -> Option<&TeletextGuideRuntimeSnapshot> {
        match &self.origin {
            NativeGuideOutputOrigin::Seed => None,
            NativeGuideOutputOrigin::Runtime(snapshot) => Some(snapshot.as_ref()),
        }
    }
    pub(crate) const fn selection(&self) -> &TeletextGuideRefreshSelection {
        &self.selection
    }
    pub(crate) fn service(&self) -> &Arc<EncodedTeletextAuthoredService> {
        &self.service
    }
    pub(crate) const fn output_deadline(&self) -> Instant {
        self.deadline
    }
    pub(crate) const fn status(&self) -> TeletextGuideRefreshStatus {
        self.status
    }
    pub(crate) const fn available_until(&self) -> DateTime<Utc> {
        self.available_until
    }
}

pub(crate) async fn reload_current_guide_refresh(
    seed: &TeletextGuideRefreshSeed,
    snapshot: &TeletextGuideRuntimeSnapshot,
    status: TeletextGuideRefreshStatus,
    available_until: DateTime<Utc>,
    context: GuideRefreshReloadContext<'_>,
) -> Result<NativeGuideRuntimeOutput, GuideRefreshNativeError> {
    GuideRefreshNativeError::check_deadline(context.deadline)?;
    if !matches!(
        status,
        TeletextGuideRefreshStatus::Fresh | TeletextGuideRefreshStatus::Stale
    ) || snapshot.id <= 0
        || snapshot.seed_id != seed.id
        || snapshot.saved.version != TELETEXT_GUIDE_RUNTIME_SNAPSHOT_VERSION
        || snapshot.saved.document.len() > context.budget.max_serialized_bytes
    {
        return Err(GuideRefreshNativeError::NeedsReview);
    }
    let deadline = output_deadline(&snapshot.saved.qualification, available_until)?;
    let (baseline, decoded) = decode_runtime_snapshot(seed, snapshot, &context, deadline).await?;
    let captured = if status == TeletextGuideRefreshStatus::Fresh {
        let source_context = super::teletext_guide_generations::GuideGenerationContext {
            application: context.application,
            pool: context.pool,
            budget: context.budget,
            encoding: context.encoding.clone(),
            limits: context.limits,
            deadline: context.deadline.min(deadline),
            hold: context.hold.clone(),
        };
        let captured = super::teletext_guide_generations::capture(
            &source_context,
            &baseline.qualification.source,
        )
        .await
        .map_err(|_| GuideRefreshNativeError::Unavailable)?;
        Some(captured)
    } else {
        None
    };
    let intent = load_guide_refresh_editorial_intent(
        context.application,
        seed,
        context.budget,
        context.encoding.clone(),
        context.deadline,
        context.hold.clone(),
    )
    .await?;
    let seed_owned = seed.clone();
    let expected = snapshot.saved.artifact_digest.clone();
    let digest = snapshot.saved.document_digest.clone();
    let encoding = context.encoding;
    let work_deadline = context.deadline.min(deadline);
    let selection = baseline.selection;
    let qualification = baseline.qualification;
    let service = teletext_work::spawn_blocking(context.hold, move || {
        if let Some(captured) = captured {
            let qualified = super::teletext_guide_generations::qualify(
                &captured,
                &qualification.source,
                decoded.guide_inputs(),
                decoded.scope_expiry(),
                work_deadline,
            )
            .map_err(|_| GuideRefreshNativeError::Unavailable)?;
            if qualified != qualification {
                return Err(GuideRefreshNativeError::NeedsReview);
            }
        }
        let service = compilation::compile(decoded, intent.as_ref(), &encoding, work_deadline)?;
        if fingerprint(
            &seed_owned,
            &qualification,
            &digest,
            &service,
            work_deadline,
        )? != expected
        {
            return Err(GuideRefreshNativeError::NeedsReview);
        }
        GuideRefreshNativeError::check_deadline(work_deadline)?;
        Ok::<_, GuideRefreshNativeError>(Arc::new(service))
    })
    .await
    .map_err(|_| GuideRefreshNativeError::WorkerFailed)??;
    Ok(NativeGuideRuntimeOutput {
        seed: seed.clone(),
        origin: NativeGuideOutputOrigin::Runtime(Box::new(snapshot.clone())),
        selection,
        service,
        deadline,
        status,
        available_until,
    })
}

async fn decode_runtime_snapshot(
    seed: &TeletextGuideRefreshSeed,
    snapshot: &TeletextGuideRuntimeSnapshot,
    context: &GuideRefreshReloadContext<'_>,
    deadline: Instant,
) -> Result<(FreshGuideBaseline, compilation::DecodedBaseline), GuideRefreshNativeError> {
    let seed_for_read = seed.clone();
    let snapshot_for_read = snapshot.clone();
    let budget = context.budget;
    let work_deadline = context.deadline.min(deadline);
    teletext_work::spawn_blocking(context.hold.clone(), move || {
        GuideRefreshNativeError::check_deadline(work_deadline)?;
        if snapshot_for_read.saved.document_digest != digest(&snapshot_for_read.saved.document) {
            return Err(GuideRefreshNativeError::NeedsReview);
        }
        let baseline = document::decode(&seed_for_read, &snapshot_for_read, budget)?;
        let decoded = compilation::decode(&seed_for_read, &baseline, budget)?;
        GuideRefreshNativeError::check_deadline(work_deadline)?;
        Ok::<_, GuideRefreshNativeError>((baseline, decoded))
    })
    .await
    .map_err(|_| GuideRefreshNativeError::WorkerFailed)?
}

fn output_deadline(
    qualification: &TeletextGuideGenerationQualification,
    available_until: DateTime<Utc>,
) -> Result<Instant, GuideRefreshNativeError> {
    let monotonic = Instant::now();
    let now = Utc::now();
    qualification
        .validate_current(now)
        .map_err(|_| GuideRefreshNativeError::Unavailable)?;
    if available_until > qualification.valid_until || available_until <= now {
        return Err(GuideRefreshNativeError::Unavailable);
    }
    monotonic
        .checked_add(
            available_until
                .signed_duration_since(now)
                .to_std()
                .map_err(|_| GuideRefreshNativeError::Unavailable)?,
        )
        .ok_or(GuideRefreshNativeError::Unavailable)
}

fn fingerprint(
    seed: &TeletextGuideRefreshSeed,
    qualification: &TeletextGuideGenerationQualification,
    document_digest: &str,
    service: &EncodedTeletextAuthoredService,
    deadline: Instant,
) -> Result<String, GuideRefreshNativeError> {
    GuideRefreshNativeError::check_deadline(deadline)?;
    let role = match seed.role {
        TeletextGuideRefreshRole::Base => {
            ersatzrs_core::repositories::TELETEXT_GUIDE_GENERATION_ARTIFACT_VERSION
        }
        TeletextGuideRefreshRole::Effective => {
            ersatzrs_core::repositories::TELETEXT_EFFECTIVE_GUIDE_GENERATION_ARTIFACT_VERSION
        }
    };
    let compiled =
        super::teletext_guide_generations::fingerprint(service, qualification, role, deadline)
            .map_err(|_| GuideRefreshNativeError::NeedsReview)?;
    let mut hash = Sha256::new();
    hash.update(b"ErsatzRS.Teletext.GuideRuntimeSnapshot.v1\0");
    hash.update(TELETEXT_GUIDE_RUNTIME_SNAPSHOT_VERSION.to_be_bytes());
    hash.update(seed.id.to_be_bytes());
    hash.update(seed.role.code().to_be_bytes());
    hash.update(seed.generation.id.to_be_bytes());
    hash.update(seed.generation.request.id.to_be_bytes());
    hash.update(seed.generation.request.digest.as_bytes());
    hash.update(document_digest.as_bytes());
    hash.update(compiled.as_bytes());
    let mut result = String::with_capacity(64);
    for byte in hash.finalize() {
        let _ = write!(result, "{byte:02x}");
    }
    GuideRefreshNativeError::check_deadline(deadline)?;
    Ok(result)
}

pub(super) fn digest(bytes: &[u8]) -> String {
    let mut value = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        let _ = write!(value, "{byte:02x}");
    }
    value
}
