//! Explicit durable selection after fresh native confirmation, never draft flags.

use async_trait::async_trait;
use ersatzrs_core::repositories::{
    TeletextEffectiveGenerationSave, TeletextEffectiveGuideGenerationSave,
    TeletextGenerationMutation, TeletextGenerationRepository, TeletextGenerationSave,
    TeletextGuideGenerationCheck, TeletextGuideGenerationQualification,
    TeletextGuideGenerationSave,
};
use ersatzrs_domain::{
    EntityId, TeletextAcceptedGeneration, TeletextChannelSelectionSnapshot,
    TeletextEffectiveChannelSelectionSnapshot,
};
use std::time::Instant;

use crate::{teletext_publications::TeletextPublicationValidationError, ApplicationServices};

/// Trusted complete compiler confirmation; caller-supplied digests are insufficient.
#[async_trait]
pub trait TeletextGenerationValidator: Send + Sync {
    /// Confirm exact revision, artifact and original complete selection, including
    /// absence of source-dependent guide content for frozen-original promotion.
    /// # Errors
    /// Rejects uncompiled/mismatched input or expired native admission.
    async fn confirm_generation(
        &self,
        save: &TeletextGenerationSave,
    ) -> Result<(), TeletextPublicationValidationError>;
}

/// Complete exact scoped artifact proof, including an explicit all-Off absence.
/// Saved originals and historical previews remain insufficient live authority.
#[async_trait]
pub trait TeletextEffectiveGenerationValidator: Send + Sync {
    /// Confirm exact effective revision/scope and artifact-v2 under the original
    /// deadline. Guide-bearing content still requires current-role freshness.
    /// # Errors
    /// Rejects incomplete/mismatched native proof or expired admission.
    async fn confirm_generation(
        &self,
        save: &TeletextEffectiveGenerationSave,
        deadline: Instant,
    ) -> Result<(), TeletextPublicationValidationError>;
}

/// Complete native Base guide-role/source/artifact proof, separate from static
/// original validators. The repository independently recaptures current source.
#[async_trait]
pub trait TeletextGuideGenerationValidator: Send + Sync {
    /// Confirm the exact immutable original, canonical saved role semantics and
    /// deterministic finite validity under the original operation deadline.
    /// # Errors
    /// Rejects missing/mismatched native proof, expired admission or guide roles.
    async fn confirm_guide_generation(
        &self,
        save: &TeletextGuideGenerationSave,
        deadline: Instant,
    ) -> Result<(), TeletextPublicationValidationError>;
}

/// Exact native scoped guide proof, additionally bound to the Scripted interval.
#[async_trait]
pub trait TeletextEffectiveGuideGenerationValidator: Send + Sync {
    /// Confirm exact scope/native artifact and original source/current roles.
    /// # Errors
    /// Rejects missing/mismatched proof, expired guide/scope or admission.
    async fn confirm_guide_generation(
        &self,
        save: &TeletextEffectiveGuideGenerationSave,
        deadline: Instant,
    ) -> Result<(), TeletextPublicationValidationError>;
}

#[derive(Debug, thiserror::Error)]
pub enum TeletextGenerationStoreError {
    #[error("durable teletext generation selection is unavailable")]
    Unavailable,
    #[error("durable teletext generation selection storage failed")]
    StorageUnavailable,
    #[error(transparent)]
    Validation(#[from] TeletextPublicationValidationError),
}

impl ApplicationServices {
    /// Accept reviewed Base guide output only after native current-role proof
    /// and transactional source/request/head/selection/accepted-pointer checks.
    /// # Errors
    /// Reports unsupported ports, expired admission or redacted proof/storage failures.
    pub async fn accept_guide_teletext_generation(
        &self,
        save: TeletextGuideGenerationSave,
        validator: &dyn TeletextGuideGenerationValidator,
        check: TeletextGuideGenerationCheck,
        max_document_bytes: usize,
    ) -> Result<TeletextGenerationMutation<TeletextAcceptedGeneration>, TeletextGenerationStoreError>
    {
        let repository = self
            .ports()
            .teletext_generations()
            .ok_or(TeletextGenerationStoreError::Unavailable)?;
        check_guide_admission(&save.qualification, check.deadline)?;
        validator
            .confirm_guide_generation(&save, check.deadline)
            .await?;
        check_guide_admission(&save.qualification, check.deadline)?;
        repository
            .accept_guide(&save, &check, max_document_bytes)
            .await
            .map_err(|_| TeletextGenerationStoreError::StorageUnavailable)
    }

    /// Scoped guide counterpart; native and repository proof retain both expiry
    /// boundaries without changing the independent accepted Base pointer.
    /// # Errors
    /// Reports unsupported ports, expired admission or redacted proof/storage failures.
    pub async fn accept_effective_guide_teletext_generation(
        &self,
        save: TeletextEffectiveGuideGenerationSave,
        validator: &dyn TeletextEffectiveGuideGenerationValidator,
        check: TeletextGuideGenerationCheck,
        max_document_bytes: usize,
    ) -> Result<TeletextGenerationMutation<TeletextAcceptedGeneration>, TeletextGenerationStoreError>
    {
        let repository = self
            .ports()
            .teletext_generations()
            .ok_or(TeletextGenerationStoreError::Unavailable)?;
        check_guide_admission(&save.qualification, check.deadline)?;
        validator
            .confirm_guide_generation(&save, check.deadline)
            .await?;
        check_guide_admission(&save.qualification, check.deadline)?;
        repository
            .accept_effective_guide(&save, &check, max_document_bytes)
            .await
            .map_err(|_| TeletextGenerationStoreError::StorageUnavailable)
    }

    /// Confirm an exact accepted guide without renewing its original role bound.
    /// # Errors
    /// Reports unsupported ports or redacted storage/admission failures.
    pub async fn confirm_current_guide_teletext_generation(
        &self,
        generation: &TeletextAcceptedGeneration,
        selection: &TeletextChannelSelectionSnapshot,
        qualification: &TeletextGuideGenerationQualification,
        check: TeletextGuideGenerationCheck,
        max_document_bytes: usize,
    ) -> Result<TeletextGenerationMutation<()>, TeletextGenerationStoreError> {
        let repository = self
            .ports()
            .teletext_generations()
            .ok_or(TeletextGenerationStoreError::Unavailable)?;
        check_guide_admission(qualification, check.deadline)?;
        let result = repository
            .confirm_current_guide(
                generation,
                selection,
                qualification,
                &check,
                max_document_bytes,
            )
            .await
            .map_err(|_| TeletextGenerationStoreError::StorageUnavailable)?;
        check_guide_admission(qualification, check.deadline)?;
        Ok(result)
    }

    /// Confirm current scoped guide output; an expired scope is never Base fallback.
    /// # Errors
    /// Reports unsupported ports or redacted storage/admission failures.
    pub async fn confirm_current_effective_guide_teletext_generation(
        &self,
        generation: &TeletextAcceptedGeneration,
        selection: &TeletextEffectiveChannelSelectionSnapshot,
        qualification: &TeletextGuideGenerationQualification,
        check: TeletextGuideGenerationCheck,
        max_document_bytes: usize,
    ) -> Result<TeletextGenerationMutation<()>, TeletextGenerationStoreError> {
        let repository = self
            .ports()
            .teletext_generations()
            .ok_or(TeletextGenerationStoreError::Unavailable)?;
        check_guide_admission(qualification, check.deadline)?;
        let result = repository
            .confirm_current_effective_guide(
                generation,
                selection,
                qualification,
                &check,
                max_document_bytes,
            )
            .await
            .map_err(|_| TeletextGenerationStoreError::StorageUnavailable)?;
        check_guide_admission(qualification, check.deadline)?;
        Ok(result)
    }

    /// Accept native-qualified scoped output under independent scoped CAS.
    /// Deadline checks bound native validation and repository admission; an
    /// admitted write returns its truthful commit result. Base pointers are retained.
    /// # Errors
    /// Reports missing proof/ports, expired admission or redacted storage failures.
    pub async fn accept_effective_teletext_generation(
        &self,
        save: TeletextEffectiveGenerationSave,
        validator: &dyn TeletextEffectiveGenerationValidator,
        max_document_bytes: usize,
        deadline: Instant,
    ) -> Result<TeletextGenerationMutation<TeletextAcceptedGeneration>, TeletextGenerationStoreError>
    {
        let repository = self
            .ports()
            .teletext_generations()
            .ok_or(TeletextGenerationStoreError::Unavailable)?;
        accept_effective_with_repository(repository, &save, validator, max_document_bytes, deadline)
            .await
    }

    /// Read retained scoped metadata; it is not current scope or native artifact proof.
    /// # Errors
    /// Reports unsupported/corrupt storage without accepted-base fallback.
    pub async fn effective_teletext_accepted_generation(
        &self,
        channel_id: EntityId,
    ) -> Result<Option<TeletextAcceptedGeneration>, TeletextGenerationStoreError> {
        self.ports()
            .teletext_generations()
            .ok_or(TeletextGenerationStoreError::Unavailable)?
            .current_effective(channel_id)
            .await
            .map_err(|_| TeletextGenerationStoreError::StorageUnavailable)
    }

    /// Confirm current scoped pointer and whole interval/configuration fence.
    /// Caller separately confirms native artifact-v2; no expired fallback is granted.
    /// # Errors
    /// Reports expired admission or redacted unsupported/corrupt storage failures.
    pub async fn confirm_effective_teletext_generation(
        &self,
        generation: &TeletextAcceptedGeneration,
        selection: &TeletextEffectiveChannelSelectionSnapshot,
        max_document_bytes: usize,
        deadline: Instant,
    ) -> Result<TeletextGenerationMutation<()>, TeletextGenerationStoreError> {
        let repository = self
            .ports()
            .teletext_generations()
            .ok_or(TeletextGenerationStoreError::Unavailable)?;
        check_effective_deadline(deadline)?;
        let result = repository
            .confirm_current_effective(generation, selection, max_document_bytes, deadline)
            .await
            .map_err(|_| TeletextGenerationStoreError::StorageUnavailable)?;
        check_effective_deadline(deadline)?;
        Ok(result)
    }

    /// Explicitly select a fully compiled generation after independent pointer CAS.
    /// Repository admission repeats exact pending/design/channel fences atomically.
    /// Acceptance selects durable output; stream delivery remains per-output.
    /// # Errors
    /// Reports missing native proof/ports or redacted storage failures.
    pub async fn accept_teletext_generation(
        &self,
        save: TeletextGenerationSave,
        validator: &dyn TeletextGenerationValidator,
        max_document_bytes: usize,
    ) -> Result<TeletextGenerationMutation<TeletextAcceptedGeneration>, TeletextGenerationStoreError>
    {
        let repository = self
            .ports()
            .teletext_generations()
            .ok_or(TeletextGenerationStoreError::Unavailable)?;
        validator.confirm_generation(&save).await?;
        repository
            .accept(&save, max_document_bytes)
            .await
            .map_err(|_| TeletextGenerationStoreError::StorageUnavailable)
    }

    /// Read retained accepted metadata. Pending requests never substitute.
    /// # Errors
    /// Reports absent injection or corrupt storage without changing output.
    pub async fn accepted_teletext_generation(
        &self,
        channel_id: EntityId,
    ) -> Result<Option<TeletextAcceptedGeneration>, TeletextGenerationStoreError> {
        self.ports()
            .teletext_generations()
            .ok_or(TeletextGenerationStoreError::Unavailable)?
            .current(channel_id)
            .await
            .map_err(|_| TeletextGenerationStoreError::StorageUnavailable)
    }

    /// Recheck exact accepted state and effective channel selection in one snapshot.
    /// Caller must also recompile and compare the retained native artifact.
    /// # Errors
    /// Reports unavailable/corrupt state; typed mismatches grant no authority.
    pub async fn confirm_current_teletext_generation(
        &self,
        generation: &TeletextAcceptedGeneration,
        selection: &TeletextChannelSelectionSnapshot,
        max_document_bytes: usize,
    ) -> Result<TeletextGenerationMutation<()>, TeletextGenerationStoreError> {
        self.ports()
            .teletext_generations()
            .ok_or(TeletextGenerationStoreError::Unavailable)?
            .confirm_current(generation, selection, max_document_bytes)
            .await
            .map_err(|_| TeletextGenerationStoreError::StorageUnavailable)
    }
}

async fn accept_effective_with_repository(
    repository: &dyn TeletextGenerationRepository,
    save: &TeletextEffectiveGenerationSave,
    validator: &dyn TeletextEffectiveGenerationValidator,
    max_document_bytes: usize,
    deadline: Instant,
) -> Result<TeletextGenerationMutation<TeletextAcceptedGeneration>, TeletextGenerationStoreError> {
    check_effective_deadline(deadline)?;
    validator.confirm_generation(save, deadline).await?;
    check_effective_deadline(deadline)?;
    repository
        .accept_effective(save, max_document_bytes, deadline)
        .await
        .map_err(|_| TeletextGenerationStoreError::StorageUnavailable)
}

fn check_effective_deadline(deadline: Instant) -> Result<(), TeletextPublicationValidationError> {
    if Instant::now() >= deadline {
        Err(TeletextPublicationValidationError::TimedOut)
    } else {
        Ok(())
    }
}

fn check_guide_admission(
    qualification: &TeletextGuideGenerationQualification,
    deadline: Instant,
) -> Result<(), TeletextPublicationValidationError> {
    check_effective_deadline(deadline)?;
    qualification
        .validate_current(chrono::Utc::now())
        .map_err(|_| TeletextPublicationValidationError::Association)
}

#[cfg(test)]
#[path = "teletext_effective_generations_tests.rs"]
mod effective_tests;
