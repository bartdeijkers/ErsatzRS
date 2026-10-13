//! Durable frozen-original publication and exact native recovery.
//!
//! Current guide-role proof is still required for guide-bearing originals.
//! Frozen inputs certify no current clock/guide data. Selected durable state
//! never certifies delivery, receiver cache retirement or all output epochs.

use std::{fmt, fmt::Write as _, sync::Arc, time::Instant};

use async_trait::async_trait;
use ersatzrs_app::{
    teletext_design_documents::TeletextDesignDocumentBudget,
    teletext_generations::{TeletextGenerationStoreError, TeletextGenerationValidator},
    teletext_publications::TeletextPublicationValidationError,
    ApplicationServices,
};
use ersatzrs_core::repositories::{
    TeletextGenerationMutation as M, TeletextGenerationRejection as R, TeletextGenerationSave,
    TeletextPublicationRejection as P, TELETEXT_GENERATION_ARTIFACT_VERSION,
};
use ersatzrs_domain::{
    EntityId, TeletextAcceptedGeneration, TeletextChannelSelectionSnapshot,
    TeletextPublicationRequest,
};
use ersatzrs_ffmpeg::teletext::{EncodedTeletextAuthoredService, TeletextServiceDescriptor};
use sha2::{Digest as _, Sha256};

use super::{
    teletext_channel_documents::TeletextChannelDocumentEncodingPolicy,
    teletext_designs::{
        reload_original_teletext_design_revision_with_hold, NativeTeletextDesignError,
        RestoredOriginalTeletextDesign,
    },
};

/// Explicit reviewed pending target and independent accepted-pointer CAS.
#[derive(Debug)]
pub struct TeletextGenerationTarget {
    /// Exact pending request presented for explicit promotion.
    pub request: TeletextPublicationRequest,
    /// Live design-head revision, independently of the selected original.
    pub expected_design_head: i64,
    /// Previous accepted generation, or absence for first publication.
    pub expected_generation_id: Option<EntityId>,
}

#[derive(Debug, thiserror::Error)]
pub enum NativeTeletextGenerationError {
    #[error(transparent)]
    Design(#[from] NativeTeletextDesignError),
    #[error(transparent)]
    Storage(#[from] TeletextGenerationStoreError),
    #[error("teletext generation artifact differs from its accepted compilation")]
    ArtifactMismatch,
    #[error("teletext guide content requires current-role freshness before activation")]
    GuideFreshnessRequired,
    #[error("teletext generation worker failed")]
    WorkerFailed,
}

/// Exact shared service with durable selected identity and runtime snapshot proof.
/// This is not an acknowledgement from any particular forwarding gate.
pub struct AcceptedTeletextOutputCandidate {
    generation: TeletextAcceptedGeneration,
    selection: TeletextChannelSelectionSnapshot,
    service: Arc<EncodedTeletextAuthoredService>,
}
impl AcceptedTeletextOutputCandidate {
    #[must_use]
    pub const fn generation(&self) -> &TeletextAcceptedGeneration {
        &self.generation
    }
    #[must_use]
    pub const fn selection(&self) -> &TeletextChannelSelectionSnapshot {
        &self.selection
    }
    #[must_use]
    pub const fn service(&self) -> &Arc<EncodedTeletextAuthoredService> {
        &self.service
    }
}
impl fmt::Debug for AcceptedTeletextOutputCandidate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AcceptedTeletextOutputCandidate")
            .field("generation", &self.generation)
            .field("binding_count", &self.selection.bindings.len())
            .finish_non_exhaustive()
    }
}

fn check_deadline(deadline: Instant) -> Result<(), NativeTeletextGenerationError> {
    if Instant::now() >= deadline {
        Err(NativeTeletextDesignError::TimedOut.into())
    } else {
        Ok(())
    }
}

pub(super) fn frozen_original(design: &RestoredOriginalTeletextDesign) -> bool {
    design.guide_source().is_none()
        && design
            .guide_inputs()
            .is_some_and(|inputs| !inputs.has_guide_channels())
}

// Frame exact wire bodies without concatenating or cloning the service. Original
// document digest independently retains its preview/layout/semantic identity.
fn artifact_digest(
    service: &EncodedTeletextAuthoredService,
    deadline: Instant,
) -> Result<String, NativeTeletextGenerationError> {
    let mut hash = Sha256::new();
    let frame =
        |hash: &mut Sha256, tag: u8, bytes: &[u8]| -> Result<(), NativeTeletextGenerationError> {
            hash.update([tag]);
            hash.update(
                u64::try_from(bytes.len())
                    .map_err(|_| NativeTeletextGenerationError::ArtifactMismatch)?
                    .to_be_bytes(),
            );
            hash.update(bytes);
            Ok(())
        };
    check_deadline(deadline)?;
    frame(&mut hash, 0, b"ErsatzRS.Teletext.CompiledService")?;
    frame(
        &mut hash,
        1,
        &TELETEXT_GENERATION_ARTIFACT_VERSION.to_be_bytes(),
    )?;
    frame(
        &mut hash,
        2,
        TeletextServiceDescriptor::from_authored_service(service).bytes(),
    )?;
    frame(
        &mut hash,
        3,
        &u64::try_from(service.magazine_units().len())
            .map_err(|_| NativeTeletextGenerationError::ArtifactMismatch)?
            .to_be_bytes(),
    )?;
    for unit in service.magazine_units() {
        check_deadline(deadline)?;
        frame(&mut hash, 4, unit)?;
    }
    frame(
        &mut hash,
        5,
        &u64::try_from(service.pages().len())
            .map_err(|_| NativeTeletextGenerationError::ArtifactMismatch)?
            .to_be_bytes(),
    )?;
    for page in service.pages() {
        frame(
            &mut hash,
            6,
            &u64::try_from(page.data_units().len())
                .map_err(|_| NativeTeletextGenerationError::ArtifactMismatch)?
                .to_be_bytes(),
        )?;
        for unit in page.data_units() {
            check_deadline(deadline)?;
            frame(&mut hash, 7, unit)?;
        }
    }
    let mut digest = String::with_capacity(64);
    for byte in hash.finalize() {
        let _ = write!(digest, "{byte:02x}");
    }
    check_deadline(deadline)?;
    Ok(digest)
}

struct NativeGenerationConfirmation<'a> {
    design: &'a RestoredOriginalTeletextDesign,
    artifact_digest: &'a str,
    deadline: Instant,
}
#[async_trait]
impl TeletextGenerationValidator for NativeGenerationConfirmation<'_> {
    async fn confirm_generation(
        &self,
        save: &TeletextGenerationSave,
    ) -> Result<(), TeletextPublicationValidationError> {
        if Instant::now() >= self.deadline {
            return Err(TeletextPublicationValidationError::TimedOut);
        }
        if !frozen_original(self.design)
            || self.design.encoded().is_none()
            || save.request.design_id != self.design.design_id()
            || save.request.revision != self.design.revision()
            || save.request.digest != self.design.digest()
            || save.request.channel_id != self.design.selection().channel_id
            || save.selection != *self.design.selection()
            || save.artifact_version != TELETEXT_GENERATION_ARTIFACT_VERSION
            || save.artifact_digest != self.artifact_digest
        {
            return Err(TeletextPublicationValidationError::Association);
        }
        if Instant::now() >= self.deadline {
            return Err(TeletextPublicationValidationError::TimedOut);
        }
        Ok(())
    }
}

async fn fingerprint(
    design: RestoredOriginalTeletextDesign,
    deadline: Instant,
    hold: ersatzrs_core::locking::TeletextWorkHold,
) -> Result<(RestoredOriginalTeletextDesign, String), NativeTeletextGenerationError> {
    super::teletext_work::spawn_blocking(hold, move || {
        let digest = artifact_digest(
            design
                .encoded()
                .ok_or(NativeTeletextGenerationError::ArtifactMismatch)?,
            deadline,
        )?;
        Ok((design, digest))
    })
    .await
    .map_err(|_| NativeTeletextGenerationError::WorkerFailed)?
}

/// Existing explicit acceptance target plus a channel-scoped route fence.
/// Absence is used only by the compatible historical public entrypoint.
pub(super) struct TeletextGenerationAcceptanceTarget {
    pub(super) target: TeletextGenerationTarget,
    pub(super) expected_channel_id: Option<EntityId>,
}

/// Explicitly accept frozen originals, recompiling under a fresh operation bound.
/// Packages never run here. Current pending/selection/head/generation CAS is
/// repeated atomically after native validation. This cannot promote guide content
/// until current-role freshness is implemented; it is not an authoring restriction.
/// # Errors
/// Native/compiler/storage failures are redacted; typed rejections write nothing.
pub async fn accept_original_teletext_generation(
    application: &ApplicationServices,
    target: TeletextGenerationTarget,
    budget: TeletextDesignDocumentBudget,
    policy: TeletextChannelDocumentEncodingPolicy,
    deadline: Instant,
) -> Result<M<AcceptedTeletextOutputCandidate>, NativeTeletextGenerationError> {
    check_deadline(deadline)?;
    let guard = super::teletext_work::begin(application, Some(deadline))
        .await
        .map_err(NativeTeletextDesignError::from)?;
    accept_original_teletext_generation_with_hold(
        application,
        TeletextGenerationAcceptanceTarget {
            target,
            expected_channel_id: None,
        },
        budget,
        policy,
        deadline,
        guard.hold_work(),
    )
    .await
}

/// Accept one exact static pending target within the controller's admission.
/// No guide-freshness or atomic repository fence is bypassed.
/// # Errors
/// Preserves existing native/CAS failures; route mismatch cannot mutate output.
pub(super) async fn accept_original_teletext_generation_with_hold(
    application: &ApplicationServices,
    target: TeletextGenerationAcceptanceTarget,
    budget: TeletextDesignDocumentBudget,
    policy: TeletextChannelDocumentEncodingPolicy,
    deadline: Instant,
    hold: ersatzrs_core::locking::TeletextWorkHold,
) -> Result<M<AcceptedTeletextOutputCandidate>, NativeTeletextGenerationError> {
    check_deadline(deadline)?;
    let TeletextGenerationAcceptanceTarget {
        target,
        expected_channel_id,
    } = target;
    if expected_channel_id
        .is_some_and(|channel_id| channel_id <= 0 || target.request.channel_id != channel_id)
    {
        return Ok(M::Rejected(R::Publication(P::InvalidValues)));
    }
    let Some(design) = reload_original_teletext_design_revision_with_hold(
        application,
        target.request.design_id,
        target.request.revision,
        budget,
        policy,
        deadline,
        hold.clone(),
    )
    .await?
    else {
        return Ok(M::Rejected(R::Publication(P::MissingRevision)));
    };
    if expected_channel_id.is_some_and(|channel_id| design.selection().channel_id != channel_id) {
        return Ok(M::Rejected(R::Publication(P::InvalidValues)));
    }
    if design.digest() != target.request.digest {
        return Ok(M::Rejected(R::Publication(P::DigestConflict)));
    }
    if !frozen_original(&design) {
        return Ok(M::Rejected(R::GuideFreshnessRequired));
    }
    if design.encoded().is_none() {
        return Ok(M::Rejected(R::Publication(P::InvalidValues)));
    }
    let (design, artifact_digest) = fingerprint(design, deadline, hold.clone()).await?;
    let save = TeletextGenerationSave {
        request: target.request,
        expected_design_head: target.expected_design_head,
        expected_generation_id: target.expected_generation_id,
        selection: design.selection().clone(),
        artifact_version: TELETEXT_GENERATION_ARTIFACT_VERSION,
        artifact_digest,
    };
    let selection = save.selection.clone();
    let validation_digest = save.artifact_digest.clone();
    let validator = NativeGenerationConfirmation {
        design: &design,
        artifact_digest: &validation_digest,
        deadline,
    };
    let mutation = application
        .accept_teletext_generation(save, &validator, budget.max_serialized_bytes)
        .await?;
    drop(hold);
    Ok(match mutation {
        M::Accepted(generation) => M::Accepted(AcceptedTeletextOutputCandidate {
            generation,
            selection,
            service: Arc::new(
                design
                    .into_encoded_service()
                    .ok_or(NativeTeletextGenerationError::ArtifactMismatch)?,
            ),
        }),
        M::Rejected(reason) => M::Rejected(reason),
    })
}

/// Recover only recorded selected output; pending requests are never consumed.
/// Recompile exact originals, compare the complete artifact, then recheck accepted
/// identity and effective channel selection in one fresh read snapshot. New draft
/// heads or pending requests do not change accepted output. No package runs.
/// # Errors
/// Missing state returns None; corrupt or changed native artifacts are errors.
pub async fn reload_accepted_teletext_generation(
    application: &ApplicationServices,
    channel_id: EntityId,
    budget: TeletextDesignDocumentBudget,
    policy: TeletextChannelDocumentEncodingPolicy,
    deadline: Instant,
) -> Result<Option<AcceptedTeletextOutputCandidate>, NativeTeletextGenerationError> {
    check_deadline(deadline)?;
    let guard = super::teletext_work::begin(application, Some(deadline))
        .await
        .map_err(NativeTeletextDesignError::from)?;
    let hold = guard.hold_work();
    let Some(generation) = application.accepted_teletext_generation(channel_id).await? else {
        return Ok(None);
    };
    if generation.artifact_version != TELETEXT_GENERATION_ARTIFACT_VERSION {
        return Err(NativeTeletextGenerationError::ArtifactMismatch);
    }
    let Some(design) = reload_original_teletext_design_revision_with_hold(
        application,
        generation.request.design_id,
        generation.request.revision,
        budget,
        policy,
        deadline,
        hold.clone(),
    )
    .await?
    else {
        return Err(NativeTeletextGenerationError::ArtifactMismatch);
    };
    if design.digest() != generation.request.digest || design.selection().channel_id != channel_id {
        return Err(NativeTeletextGenerationError::ArtifactMismatch);
    }
    if !frozen_original(&design) {
        return Err(NativeTeletextGenerationError::GuideFreshnessRequired);
    }
    let (design, actual) = fingerprint(design, deadline, hold).await?;
    if actual != generation.artifact_digest {
        return Err(NativeTeletextGenerationError::ArtifactMismatch);
    }
    let confirmed = application
        .confirm_current_teletext_generation(
            &generation,
            design.selection(),
            budget.max_serialized_bytes,
        )
        .await?;
    check_deadline(deadline)?;
    if !matches!(confirmed, M::Accepted(())) {
        return Ok(None);
    }
    let selection = design.selection().clone();
    check_deadline(deadline)?;
    Ok(Some(AcceptedTeletextOutputCandidate {
        generation,
        selection,
        service: Arc::new(
            design
                .into_encoded_service()
                .ok_or(NativeTeletextGenerationError::ArtifactMismatch)?,
        ),
    }))
}
