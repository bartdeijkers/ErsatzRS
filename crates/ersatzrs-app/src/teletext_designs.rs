//! Durable original drafts, independent of publication (ADR 0123).

use async_trait::async_trait;
use ersatzrs_core::repositories::{TeletextDesignMutation, TeletextDesignSave};
use ersatzrs_domain::{EntityId, TeletextDesignHead, TeletextDesignRevision};

use crate::{
    teletext_design_documents::{
        EncodedEffectiveTeletextDesignDocuments, EncodedTeletextDesignDocuments,
    },
    ApplicationServices,
};

/// Static confirmation failure without producer content or private settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TeletextOriginalDesignValidationError {
    #[error("teletext original document does not match its complete native validation")]
    Association,
    #[error("complete native teletext validation is unavailable")]
    Unavailable,
    #[error("teletext original document validation exceeded its operation deadline")]
    TimedOut,
    #[error("teletext original document validation worker failed")]
    WorkerFailed,
}

/// Trusted host confirmation of the exact document's complete engine admission.
///
/// Structural codec success alone is insufficient. The native implementation
/// must associate these immutable bytes with the complete checked candidate,
/// including original entry, region and explicit resource lifecycle. A request
/// flag or saved receipt cannot substitute for this confirmation. Confirmation
/// grants no publication, current-selection or resource-update authority.
#[async_trait]
pub trait TeletextOriginalDesignValidator: Send + Sync {
    /// Confirm complete native admission of these exact original-document bytes.
    /// # Errors
    /// Rejects any missing or mismatched complete engine validation.
    async fn confirm_original(
        &self,
        document: EncodedTeletextDesignDocuments,
    ) -> Result<EncodedTeletextDesignDocuments, TeletextOriginalDesignValidationError>;
}

/// Complete host admission of exact effective originals, distinct from base
/// bindings. Confirmation retains whole origins, Off and producer associations;
/// it grants no current-selection, expiry, publication or editor authority.
#[async_trait]
pub trait TeletextEffectiveOriginalDesignValidator: Send + Sync {
    /// Confirm these exact bytes against their complete native candidate.
    /// # Errors
    /// Rejects missing, expired or mismatched complete native validation.
    async fn confirm_original(
        &self,
        document: EncodedEffectiveTeletextDesignDocuments,
    ) -> Result<EncodedEffectiveTeletextDesignDocuments, TeletextOriginalDesignValidationError>;
}

/// Storage failures retain no raw repository, producer or settings diagnostics.
#[derive(Debug, thiserror::Error)]
pub enum TeletextDesignStoreError {
    #[error("durable teletext draft storage is unavailable")]
    Unavailable,
    #[error("durable teletext draft storage failed")]
    StorageUnavailable,
    #[error(transparent)]
    Validation(#[from] TeletextOriginalDesignValidationError),
}

impl ApplicationServices {
    /// Save effective originals as an unpublished immutable draft after complete
    /// native confirmation. Historical saving does not select a current scope,
    /// notify playout, activate packages or change accepted base/scoped pointers.
    /// # Errors
    /// Rejects missing native confirmation, unavailable storage and storage errors;
    /// bounds, CAS and deletion rejections remain explicit mutation results.
    pub async fn save_effective_original_teletext_design(
        &self,
        name: String,
        existing: Option<(EntityId, i64)>,
        document: EncodedEffectiveTeletextDesignDocuments,
        validator: &dyn TeletextEffectiveOriginalDesignValidator,
        max_document_bytes: usize,
    ) -> Result<TeletextDesignMutation<TeletextDesignHead>, TeletextDesignStoreError> {
        let repository = self
            .ports()
            .teletext_designs()
            .ok_or(TeletextDesignStoreError::Unavailable)?;
        let document = validator.confirm_original(document).await?;
        let document_version = document.version();
        repository
            .save(
                &TeletextDesignSave {
                    name,
                    existing,
                    document_version,
                    document: document.into_bytes(),
                },
                max_document_bytes,
            )
            .await
            .map_err(|_| TeletextDesignStoreError::StorageUnavailable)
    }

    /// Save an original-only draft after complete host admission, using CAS.
    ///
    /// The caller supplies trusted byte bounds and native confirmation. Saving
    /// does not notify playout, activate profiles, install packages or publish.
    /// # Errors
    /// Rejects unavailable storage, incomplete native validation or storage
    /// failures. CAS/bounds/deletion rejections remain explicit mutation results.
    pub async fn save_original_teletext_design(
        &self,
        name: String,
        existing: Option<(EntityId, i64)>,
        document: EncodedTeletextDesignDocuments,
        validator: &dyn TeletextOriginalDesignValidator,
        max_document_bytes: usize,
    ) -> Result<TeletextDesignMutation<TeletextDesignHead>, TeletextDesignStoreError> {
        let repository = self
            .ports()
            .teletext_designs()
            .ok_or(TeletextDesignStoreError::Unavailable)?;
        let document = validator.confirm_original(document).await?;
        let document_version = document.version();
        repository
            .save(
                &TeletextDesignSave {
                    name,
                    existing,
                    document_version,
                    document: document.into_bytes(),
                },
                max_document_bytes,
            )
            .await
            .map_err(|_| TeletextDesignStoreError::StorageUnavailable)
    }

    /// Read an immutable revision, including a tombstoned or historical design.
    /// # Errors
    /// Unavailable storage and corrupt/version/digest/bound failures are errors,
    /// distinct from an absent design or revision.
    pub async fn teletext_design_revision(
        &self,
        id: EntityId,
        revision: i64,
        max_document_bytes: usize,
    ) -> Result<Option<TeletextDesignRevision>, TeletextDesignStoreError> {
        self.ports()
            .teletext_designs()
            .ok_or(TeletextDesignStoreError::Unavailable)?
            .revision(id, revision, max_document_bytes)
            .await
            .map_err(|_| TeletextDesignStoreError::StorageUnavailable)
    }

    /// Read a design head without reading its potentially large private body.
    /// # Errors
    /// Reports unavailable storage or corrupt metadata as errors.
    pub async fn teletext_design_head(
        &self,
        id: EntityId,
    ) -> Result<Option<TeletextDesignHead>, TeletextDesignStoreError> {
        self.ports()
            .teletext_designs()
            .ok_or(TeletextDesignStoreError::Unavailable)?
            .head(id)
            .await
            .map_err(|_| TeletextDesignStoreError::StorageUnavailable)
    }

    /// List bounded heads using ascending identity keyset pagination.
    /// # Errors
    /// Reports unavailable storage, invalid bounds or corrupt metadata.
    pub async fn teletext_design_heads(
        &self,
        after_id: Option<EntityId>,
        limit: usize,
    ) -> Result<Vec<TeletextDesignHead>, TeletextDesignStoreError> {
        self.ports()
            .teletext_designs()
            .ok_or(TeletextDesignStoreError::Unavailable)?
            .heads(after_id, limit)
            .await
            .map_err(|_| TeletextDesignStoreError::StorageUnavailable)
    }

    /// Tombstone an exact head while retaining immutable originals.
    /// # Errors
    /// Reports unavailable storage or storage failures. A stale/deleted head is
    /// an explicit rejection and cannot change the live broadcast.
    pub async fn tombstone_teletext_design(
        &self,
        id: EntityId,
        expected_revision: i64,
    ) -> Result<TeletextDesignMutation<TeletextDesignHead>, TeletextDesignStoreError> {
        self.ports()
            .teletext_designs()
            .ok_or(TeletextDesignStoreError::Unavailable)?
            .tombstone(id, expected_revision)
            .await
            .map_err(|_| TeletextDesignStoreError::StorageUnavailable)
    }
}
