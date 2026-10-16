//! Explicit scoped acceptance and exact recovery; no producer or Lua replay.

use std::{fmt, fmt::Write as _, sync::Arc, time::Instant};

use async_trait::async_trait;
use ersatzrs_app::{
    teletext_design_documents::TeletextDesignDocumentBudget,
    teletext_generations::TeletextEffectiveGenerationValidator,
    teletext_publications::TeletextPublicationValidationError, ApplicationServices,
};
use ersatzrs_core::{
    locking::TeletextWorkHold,
    repositories::{
        TeletextEffectiveGenerationSave, TeletextGenerationMutation as M,
        TeletextGenerationRejection as R, TeletextPublicationRejection as P,
        TELETEXT_EFFECTIVE_GENERATION_ARTIFACT_VERSION,
        TELETEXT_EFFECTIVE_LIFECYCLE_GENERATION_ARTIFACT_VERSION,
        TELETEXT_LIFECYCLE_GENERATION_ARTIFACT_VERSION,
    },
};
use ersatzrs_domain::{
    EntityId, TeletextAcceptedGeneration, TeletextEffectiveBindingSnapshot,
    TeletextEffectiveChannelSelectionSnapshot,
};
use ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService;
use sha2::{Digest as _, Sha256};

use super::{
    teletext_channel_documents::TeletextChannelDocumentEncodingPolicy,
    teletext_designs::NativeTeletextDesignError,
    teletext_effective_designs::{
        reload_effective_original_for_publication_with_hold, RestoredEffectiveOriginalDesign,
    },
    teletext_generations::{
        artifact_digest, artifact_digest_for_version, NativeTeletextGenerationError,
        TeletextGenerationTarget,
    },
};

/// Exact durable scoped identity, complete fence and explicit service presence.
/// Genuine all-Off absence never restores the operator base output implicitly.
pub struct AcceptedEffectiveTeletextOutputCandidate {
    generation: TeletextAcceptedGeneration,
    selection: Arc<TeletextEffectiveChannelSelectionSnapshot>,
    service: Option<Arc<EncodedTeletextAuthoredService>>,
}
impl AcceptedEffectiveTeletextOutputCandidate {
    #[must_use]
    pub const fn generation(&self) -> &TeletextAcceptedGeneration {
        &self.generation
    }
    #[must_use]
    pub fn selection(&self) -> &TeletextEffectiveChannelSelectionSnapshot {
        &self.selection
    }
    #[must_use]
    pub const fn service(&self) -> Option<&Arc<EncodedTeletextAuthoredService>> {
        self.service.as_ref()
    }
}
impl fmt::Debug for AcceptedEffectiveTeletextOutputCandidate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AcceptedEffectiveTeletextOutputCandidate")
            .field("generation", &self.generation)
            .field("selection", &self.selection)
            .field("service_present", &self.service.is_some())
            .finish_non_exhaustive()
    }
}

pub(super) fn operation_deadline(deadline: Instant) -> Result<Instant, NativeTeletextDesignError> {
    let now = Instant::now();
    let deadline =
        deadline.min(now + ersatzrs_app::teletext_channel_documents::PREPARATION_TIMEOUT);
    if now >= deadline {
        Err(NativeTeletextDesignError::TimedOut)
    } else {
        Ok(deadline)
    }
}
pub(super) fn frozen_original(design: &RestoredEffectiveOriginalDesign) -> bool {
    design.guide_source().is_none()
        && design
            .guide_inputs()
            .is_some_and(|inputs| !inputs.has_guide_channels())
}
pub(super) fn compiled_shape(design: &RestoredEffectiveOriginalDesign) -> bool {
    if !design
        .selection()
        .bindings
        .iter()
        .any(|b| matches!(b, TeletextEffectiveBindingSnapshot::Scripted(_)))
    {
        return false;
    }
    let count = design
        .selection()
        .bindings
        .iter()
        .filter(|b| b.enabled())
        .count();
    if count == 0 {
        design.encoded().is_none()
            && design.producer_bindings().is_empty()
            && design.sources().is_empty()
            && design.metadata().is_none()
    } else {
        design.encoded().is_some()
            && design.producer_bindings().len() == count
            && design.sources().len() == count
    }
}

fn effective_artifact_digest(
    design: &RestoredEffectiveOriginalDesign,
    version: u16,
    deadline: Instant,
) -> Result<String, NativeTeletextGenerationError> {
    operation_deadline(deadline)?;
    if !compiled_shape(design)
        || !matches!(
            version,
            TELETEXT_EFFECTIVE_GENERATION_ARTIFACT_VERSION
                | TELETEXT_EFFECTIVE_LIFECYCLE_GENERATION_ARTIFACT_VERSION
        )
    {
        return Err(NativeTeletextGenerationError::ArtifactMismatch);
    }
    let mut hash = Sha256::new();
    hash.update(b"ErsatzRS.Teletext.EffectiveCompiledService");
    hash.update(version.to_be_bytes());
    match design.encoded() {
        Some(service) => {
            hash.update([1]);
            let digest = if version == TELETEXT_EFFECTIVE_GENERATION_ARTIFACT_VERSION {
                artifact_digest(service, deadline)?
            } else {
                artifact_digest_for_version(
                    service,
                    TELETEXT_LIFECYCLE_GENERATION_ARTIFACT_VERSION,
                    deadline,
                )?
            };
            hash.update(digest.as_bytes());
        }
        None => hash.update([0]),
    }
    let mut result = String::with_capacity(64);
    for byte in hash.finalize() {
        let _ = write!(result, "{byte:02x}");
    }
    operation_deadline(deadline)?;
    Ok(result)
}

async fn fingerprint(
    design: RestoredEffectiveOriginalDesign,
    version: u16,
    deadline: Instant,
    hold: TeletextWorkHold,
) -> Result<(RestoredEffectiveOriginalDesign, String), NativeTeletextGenerationError> {
    super::teletext_work::spawn_blocking(hold, move || {
        let design = if version == TELETEXT_EFFECTIVE_LIFECYCLE_GENERATION_ARTIFACT_VERSION {
            design.with_resource_lifecycle_variants(deadline)?
        } else {
            design
        };
        let digest = effective_artifact_digest(&design, version, deadline)?;
        Ok((design, digest))
    })
    .await
    .map_err(|_| NativeTeletextGenerationError::WorkerFailed)?
}
struct NativeEffectiveGenerationConfirmation<'a> {
    design: &'a RestoredEffectiveOriginalDesign,
    digest: &'a str,
    deadline: Instant,
}
#[async_trait]
impl TeletextEffectiveGenerationValidator for NativeEffectiveGenerationConfirmation<'_> {
    async fn confirm_generation(
        &self,
        save: &TeletextEffectiveGenerationSave,
        deadline: Instant,
    ) -> Result<(), TeletextPublicationValidationError> {
        if Instant::now() >= self.deadline.min(deadline) {
            return Err(TeletextPublicationValidationError::TimedOut);
        }
        if !frozen_original(self.design)
            || !compiled_shape(self.design)
            || save.request.design_id != self.design.design_id()
            || save.request.revision != self.design.revision()
            || save.request.digest != self.design.digest()
            || save.request.channel_id != self.design.selection().channel_id
            || save.selection != *self.design.selection()
            || save.artifact_version != TELETEXT_EFFECTIVE_LIFECYCLE_GENERATION_ARTIFACT_VERSION
            || save.artifact_digest != self.digest
        {
            return Err(TeletextPublicationValidationError::Association);
        }
        if Instant::now() >= self.deadline.min(deadline) {
            return Err(TeletextPublicationValidationError::TimedOut);
        }
        Ok(())
    }
}

/// Accept exactly one reviewed scoped pending target, independently of base output.
/// Saved transition expiry caps fresh admission; current guide-role proof is still
/// required for guide-bearing originals. This creates no stream acknowledgement.
/// # Errors
/// Native/guide/expiry/storage failures are redacted; CAS rejections write nothing.
pub async fn accept_effective_original_teletext_generation(
    application: &ApplicationServices,
    target: TeletextGenerationTarget,
    budget: TeletextDesignDocumentBudget,
    policy: TeletextChannelDocumentEncodingPolicy,
    deadline: Instant,
) -> Result<M<AcceptedEffectiveTeletextOutputCandidate>, NativeTeletextGenerationError> {
    let deadline = operation_deadline(deadline)?;
    let guard = super::teletext_work::begin(application, Some(deadline))
        .await
        .map_err(NativeTeletextDesignError::from)?;
    accept_effective_original_teletext_generation_with_hold(
        application,
        target,
        budget,
        policy,
        deadline,
        guard.hold_work(),
    )
    .await
}

pub(super) async fn accept_effective_original_teletext_generation_with_hold(
    application: &ApplicationServices,
    target: TeletextGenerationTarget,
    budget: TeletextDesignDocumentBudget,
    policy: TeletextChannelDocumentEncodingPolicy,
    deadline: Instant,
    hold: TeletextWorkHold,
) -> Result<M<AcceptedEffectiveTeletextOutputCandidate>, NativeTeletextGenerationError> {
    let deadline = operation_deadline(deadline)?;
    let Some((design, deadline)) = reload_effective_original_for_publication_with_hold(
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
    if design.selection().channel_id != target.request.channel_id {
        return Ok(M::Rejected(R::Publication(P::InvalidValues)));
    }
    if design.digest() != target.request.digest {
        return Ok(M::Rejected(R::Publication(P::DigestConflict)));
    }
    if !frozen_original(&design) {
        return Ok(M::Rejected(R::GuideFreshnessRequired));
    }
    if !compiled_shape(&design) {
        return Ok(M::Rejected(R::Publication(P::InvalidValues)));
    }
    let (design, digest) = fingerprint(
        design,
        TELETEXT_EFFECTIVE_LIFECYCLE_GENERATION_ARTIFACT_VERSION,
        deadline,
        hold.clone(),
    )
    .await?;
    let save = TeletextEffectiveGenerationSave {
        request: target.request,
        expected_design_head: target.expected_design_head,
        expected_generation_id: target.expected_generation_id,
        selection: design.selection().clone(),
        artifact_version: TELETEXT_EFFECTIVE_LIFECYCLE_GENERATION_ARTIFACT_VERSION,
        artifact_digest: digest,
    };
    let selection = Arc::new(save.selection.clone());
    let validation_digest = save.artifact_digest.clone();
    let validator = NativeEffectiveGenerationConfirmation {
        design: &design,
        digest: &validation_digest,
        deadline,
    };
    let mutation = application
        .accept_effective_teletext_generation(
            save,
            &validator,
            budget.max_serialized_bytes,
            deadline,
        )
        .await?;
    drop(hold);
    Ok(match mutation {
        M::Accepted(generation) => M::Accepted(AcceptedEffectiveTeletextOutputCandidate {
            generation,
            selection,
            service: design.into_encoded_service().map(Arc::new),
        }),
        M::Rejected(reason) => M::Rejected(reason),
    })
}

/// Recover only the exact accepted scoped pointer, never a new draft or pending
/// request. Recompile and compare presence/artifact, then confirm whole scope and
/// accepted identity in one repository snapshot; no producer or Lua runs.
/// # Errors
/// Missing pointer returns None; guide/expiry/corruption/native failures are errors.
pub async fn reload_accepted_effective_teletext_generation(
    application: &ApplicationServices,
    channel_id: EntityId,
    budget: TeletextDesignDocumentBudget,
    policy: TeletextChannelDocumentEncodingPolicy,
    deadline: Instant,
) -> Result<Option<AcceptedEffectiveTeletextOutputCandidate>, NativeTeletextGenerationError> {
    let deadline = operation_deadline(deadline)?;
    let guard = super::teletext_work::begin(application, Some(deadline))
        .await
        .map_err(NativeTeletextDesignError::from)?;
    let hold = guard.hold_work();
    let Some(generation) = application
        .effective_teletext_accepted_generation(channel_id)
        .await?
    else {
        return Ok(None);
    };
    if !matches!(
        generation.artifact_version,
        TELETEXT_EFFECTIVE_GENERATION_ARTIFACT_VERSION
            | TELETEXT_EFFECTIVE_LIFECYCLE_GENERATION_ARTIFACT_VERSION
    ) {
        return Err(NativeTeletextGenerationError::ArtifactMismatch);
    }
    let Some((design, deadline)) = reload_effective_original_for_publication_with_hold(
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
    let (design, actual) =
        fingerprint(design, generation.artifact_version, deadline, hold.clone()).await?;
    if actual != generation.artifact_digest {
        return Err(NativeTeletextGenerationError::ArtifactMismatch);
    }
    let confirmed = application
        .confirm_effective_teletext_generation(
            &generation,
            design.selection(),
            budget.max_serialized_bytes,
            deadline,
        )
        .await?;
    operation_deadline(deadline)?;
    if !matches!(confirmed, M::Accepted(())) {
        return Ok(None);
    }
    let selection = Arc::new(design.selection().clone());
    drop(hold);
    Ok(Some(AcceptedEffectiveTeletextOutputCandidate {
        generation,
        selection,
        service: design.into_encoded_service().map(Arc::new),
    }))
}
