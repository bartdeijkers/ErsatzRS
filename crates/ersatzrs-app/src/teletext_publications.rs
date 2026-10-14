//! Explicit pending requests, distinct from draft saving or output promotion.

use async_trait::async_trait;
use ersatzrs_core::repositories::{
    TeletextEffectivePublicationRequestSave, TeletextGuideSourceCheckLimits,
    TeletextGuideSourceFence, TeletextPublicationMutation, TeletextPublicationRepository,
    TeletextPublicationRequestSave,
};
use ersatzrs_domain::{EntityId, TeletextPublicationRequest};
use std::time::Instant;

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

    /// Confirm the exact historical fence as part of complete native validation.
    /// Static-only validators reject instead of discarding the source association.
    /// # Errors
    /// Rejects mismatched native/source association or expired admission.
    async fn confirm_request_with_source(
        &self,
        _request: &TeletextPublicationRequestSave,
        _fence: &TeletextGuideSourceFence,
    ) -> Result<(), TeletextPublicationValidationError> {
        Err(TeletextPublicationValidationError::Association)
    }
}

/// Exact native admission of effective originals, distinct from base controls.
/// Historical preview or a saved digest never grants current scoped authority.
#[async_trait]
pub trait TeletextEffectivePublicationValidator: Send + Sync {
    /// Confirm complete compilation and exact whole-scope request association
    /// under the original admission deadline, including an all-Off absence.
    /// # Errors
    /// Rejects uncompiled/mismatched requests or expired native admission.
    async fn confirm_request(
        &self,
        request: &TeletextEffectivePublicationRequestSave,
        deadline: Instant,
    ) -> Result<(), TeletextPublicationValidationError>;

    /// Confirm the historical source fence without discarding its association.
    /// # Errors
    /// Rejects unsupported source validation, mismatches or expired admission.
    async fn confirm_request_with_source(
        &self,
        _request: &TeletextEffectivePublicationRequestSave,
        _fence: &TeletextGuideSourceFence,
        _deadline: Instant,
    ) -> Result<(), TeletextPublicationValidationError> {
        Err(TeletextPublicationValidationError::Association)
    }
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
    /// Request exact effective originals through independent scoped pointer CAS.
    /// No base configuration, accepted output or live stream changes. Once the
    /// repository admits a write, its atomic settlement result remains truthful.
    /// # Errors
    /// Rejects missing native proof/ports, expired admission or redacted storage failures.
    pub async fn request_effective_teletext_design_publication(
        &self,
        request: TeletextEffectivePublicationRequestSave,
        validator: &dyn TeletextEffectivePublicationValidator,
        max_document_bytes: usize,
        deadline: Instant,
    ) -> Result<
        TeletextPublicationMutation<TeletextPublicationRequest>,
        TeletextPublicationStoreError,
    > {
        let repository = self
            .ports()
            .teletext_publications()
            .ok_or(TeletextPublicationStoreError::Unavailable)?;
        request_effective_with_repository(
            repository,
            &request,
            validator,
            max_document_bytes,
            deadline,
        )
        .await
    }

    /// Preserve guide association and same-transaction semantic recapture during
    /// scoped request admission. This does not establish current Now/Next role.
    /// # Errors
    /// Rejects missing source proof, expired admission or redacted storage failures.
    pub async fn request_source_bound_effective_teletext_design_publication(
        &self,
        request: TeletextEffectivePublicationRequestSave,
        validator: &dyn TeletextEffectivePublicationValidator,
        fence: &TeletextGuideSourceFence,
        limits: TeletextGuideSourceCheckLimits,
        max_document_bytes: usize,
        deadline: Instant,
    ) -> Result<
        TeletextPublicationMutation<TeletextPublicationRequest>,
        TeletextPublicationStoreError,
    > {
        let repository = self
            .ports()
            .teletext_publications()
            .ok_or(TeletextPublicationStoreError::Unavailable)?;
        check_source_deadline(deadline)?;
        validator
            .confirm_request_with_source(&request, fence, deadline)
            .await?;
        check_source_deadline(deadline)?;
        repository
            .request_effective_with_source(&request, fence, limits, max_document_bytes, deadline)
            .await
            .map_err(|_| TeletextPublicationStoreError::StorageUnavailable)
    }

    /// Retained scoped pending metadata; it grants no active interval authority.
    /// # Errors
    /// Reports unsupported/corrupt storage without base pending-request fallback.
    pub async fn effective_teletext_publication_request(
        &self,
        channel_id: EntityId,
    ) -> Result<Option<TeletextPublicationRequest>, TeletextPublicationStoreError> {
        self.ports()
            .teletext_publications()
            .ok_or(TeletextPublicationStoreError::Unavailable)?
            .current_effective(channel_id)
            .await
            .map_err(|_| TeletextPublicationStoreError::StorageUnavailable)
    }

    /// Request a guide-bound exact revision after source-aware native validation.
    /// The repository compares stored association and recaptures semantic context
    /// within its transaction. This creates pending intent, never output authority.
    /// # Errors
    /// Rejects missing source-aware validation, expiry and redacted storage errors.
    pub async fn request_source_bound_teletext_design_publication(
        &self,
        request: TeletextPublicationRequestSave,
        validator: &dyn TeletextPublicationValidator,
        fence: &TeletextGuideSourceFence,
        limits: TeletextGuideSourceCheckLimits,
        max_document_bytes: usize,
        deadline: Instant,
    ) -> Result<
        TeletextPublicationMutation<TeletextPublicationRequest>,
        TeletextPublicationStoreError,
    > {
        let repository = self
            .ports()
            .teletext_publications()
            .ok_or(TeletextPublicationStoreError::Unavailable)?;
        check_source_deadline(deadline)?;
        validator
            .confirm_request_with_source(&request, fence)
            .await?;
        check_source_deadline(deadline)?;
        repository
            .request_with_source(&request, fence, limits, max_document_bytes, deadline)
            .await
            .map_err(|_| TeletextPublicationStoreError::StorageUnavailable)
    }

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

async fn request_effective_with_repository(
    repository: &dyn TeletextPublicationRepository,
    request: &TeletextEffectivePublicationRequestSave,
    validator: &dyn TeletextEffectivePublicationValidator,
    max_document_bytes: usize,
    deadline: Instant,
) -> Result<TeletextPublicationMutation<TeletextPublicationRequest>, TeletextPublicationStoreError>
{
    check_source_deadline(deadline)?;
    validator.confirm_request(request, deadline).await?;
    check_source_deadline(deadline)?;
    repository
        .request_effective(request, max_document_bytes, deadline)
        .await
        .map_err(|_| TeletextPublicationStoreError::StorageUnavailable)
}

fn check_source_deadline(deadline: Instant) -> Result<(), TeletextPublicationValidationError> {
    if Instant::now() >= deadline {
        Err(TeletextPublicationValidationError::TimedOut)
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "teletext_effective_publications_tests.rs"]
mod effective_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_core::repositories::{
        TELETEXT_GUIDE_CONTEXT_DIGEST_VERSION, TELETEXT_GUIDE_SOURCE_FENCE_VERSION,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct StaticValidator(AtomicUsize);

    #[async_trait]
    impl TeletextPublicationValidator for StaticValidator {
        async fn confirm_request(
            &self,
            _request: &TeletextPublicationRequestSave,
        ) -> Result<(), TeletextPublicationValidationError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn static_native_confirmation_never_silently_confirms_a_source_bound_request() {
        let request = TeletextPublicationRequestSave {
            design_id: 1,
            revision: 1,
            digest: "a".repeat(64),
            expected_design_head: 1,
            expected_request_id: None,
            selection: ersatzrs_domain::TeletextChannelSelectionSnapshot {
                channel_id: 9,
                bindings: vec![],
            },
        };
        let source = TeletextGuideSourceFence {
            version: TELETEXT_GUIDE_SOURCE_FENCE_VERSION,
            context_digest_version: TELETEXT_GUIDE_CONTEXT_DIGEST_VERSION,
            context_digest: [0; 32],
            channel_ids: vec![17],
            captured_at: "2026-10-11T12:00:00Z".parse().unwrap(),
            start: "2026-10-11T00:00:00Z".parse().unwrap(),
            finish: "2026-10-14T00:00:00Z".parse().unwrap(),
            locale: "en".into(),
            timezone: "UTC".into(),
        };
        source.validate().unwrap();
        let validator = StaticValidator(AtomicUsize::new(0));
        assert_eq!(
            validator
                .confirm_request_with_source(&request, &source)
                .await,
            Err(TeletextPublicationValidationError::Association)
        );
        assert_eq!(validator.0.load(Ordering::SeqCst), 0);
        validator.confirm_request(&request).await.unwrap();
        assert_eq!(validator.0.load(Ordering::SeqCst), 1);
    }
}
