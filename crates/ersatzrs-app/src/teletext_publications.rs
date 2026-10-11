//! Explicit pending requests, distinct from draft saving or output promotion.

use async_trait::async_trait;
use ersatzrs_core::repositories::{TeletextPublicationMutation, TeletextPublicationRequestSave};
use ersatzrs_domain::{EntityId, TeletextPublicationRequest};

use crate::ApplicationServices;

/// Static native confirmation error, without authored content or settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TeletextPublicationValidationError {
    #[error("teletext publication request differs from its complete native validation")]
    Association,
    #[error("teletext publication validation exceeded its admission deadline")]
    TimedOut,
}

/// Trusted native admission of an exact revision and its original selection.
/// A saved digest, imported flag or preview receipt alone grants no admission.
#[async_trait]
pub trait TeletextPublicationValidator: Send + Sync {
    /// Confirm complete native compilation and exact request association.
    /// # Errors
    /// Rejects missing compilation, association mismatch or expired admission.
    async fn confirm_request(
        &self,
        request: &TeletextPublicationRequestSave,
    ) -> Result<(), TeletextPublicationValidationError>;
}

/// Redacted storage/admission failures, separate from typed CAS rejections.
#[derive(Debug, thiserror::Error)]
pub enum TeletextPublicationStoreError {
    #[error("durable teletext publication requests are unavailable")]
    Unavailable,
    #[error("durable teletext publication request storage failed")]
    StorageUnavailable,
    #[error(transparent)]
    Validation(#[from] TeletextPublicationValidationError),
}

impl ApplicationServices {
    /// Explicitly request an exact compiled revision, using all repository fences.
    /// This writes only a pending intent. It neither installs packages nor changes
    /// playback, accepted generations, Lua intervals or the live channel binding.
    /// Native admission precedes the normal database transaction timeout contract.
    /// # Errors
    /// Reports unavailable storage, native admission or redacted storage failures.
    pub async fn request_teletext_design_publication(
        &self,
        request: TeletextPublicationRequestSave,
        validator: &dyn TeletextPublicationValidator,
        max_document_bytes: usize,
    ) -> Result<
        TeletextPublicationMutation<TeletextPublicationRequest>,
        TeletextPublicationStoreError,
    > {
        let repository = self
            .ports()
            .teletext_publications()
            .ok_or(TeletextPublicationStoreError::Unavailable)?;
        validator.confirm_request(&request).await?;
        repository
            .request(&request, max_document_bytes)
            .await
            .map_err(|_| TeletextPublicationStoreError::StorageUnavailable)
    }

    /// Read the current pending request, without invoking or promoting it.
    /// # Errors
    /// Reports unavailable storage or corrupt metadata as a redacted error.
    pub async fn teletext_publication_request(
        &self,
        channel_id: EntityId,
    ) -> Result<Option<TeletextPublicationRequest>, TeletextPublicationStoreError> {
        self.ports()
            .teletext_publications()
            .ok_or(TeletextPublicationStoreError::Unavailable)?
            .current(channel_id)
            .await
            .map_err(|_| TeletextPublicationStoreError::StorageUnavailable)
    }
}
