//! Explicit durable selection after fresh native confirmation, never draft flags.

use async_trait::async_trait;
use ersatzrs_core::repositories::{TeletextGenerationMutation, TeletextGenerationSave};
use ersatzrs_domain::{EntityId, TeletextAcceptedGeneration, TeletextChannelSelectionSnapshot};

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
