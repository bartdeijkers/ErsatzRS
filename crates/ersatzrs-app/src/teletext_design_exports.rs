//! Durable export baselines, independent of editor input and publication.
//!
//! ADR 0123 requires server-owned receipts. Opaque persistence does not certify
//! file mapping, resource preservation or edit permissions; a trusted native
//! validator must confirm the exact receipt against the stored original first.

use async_trait::async_trait;
use ersatzrs_core::repositories::{
    TeletextDesignExportSave, TeletextDesignMutation, TeletextDesignRejection,
};
use ersatzrs_domain::{
    EntityId, TeletextDesignExportMetadata, TeletextDesignExportReceipt, TeletextDesignRevision,
};

use crate::ApplicationServices;

/// Redacted confirmation failure without authored content or file paths.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TeletextDesignExportValidationError {
    #[error("teletext export baseline does not match its original design")]
    Association,
    #[error("teletext export cannot preserve the selected content or edit policy")]
    Unsupported,
    #[error("complete teletext export validation is unavailable")]
    Unavailable,
    #[error("teletext export validation exceeded its operation deadline")]
    TimedOut,
    #[error("teletext export validation worker failed")]
    WorkerFailed,
}

/// Trusted confirmation of complete export mapping and edit-policy provenance.
///
/// A host implementation must authenticate the exact original's native model,
/// file mapping, resource dependencies and captured addon-owned edit policy.
/// User-supplied sidecar fields cannot grant permissions. Neither this port nor
/// a persisted receipt installs packages, edits profiles or selects output.
#[async_trait]
pub trait TeletextDesignExportValidator: Send + Sync {
    /// Confirm these exact receipt bytes against the repository-read original.
    ///
    /// Both inputs remain borrowed and cannot be replaced by confirmation.
    /// Native work must retain the host's existing worker lease and deadline.
    /// # Errors
    /// Rejects unavailable, unsupported, incomplete or mismatched full exports.
    async fn confirm_export(
        &self,
        original: &TeletextDesignRevision,
        export: &TeletextDesignExportSave,
    ) -> Result<(), TeletextDesignExportValidationError>;
}

/// Redacted storage and full-export confirmation failures.
#[derive(Debug, thiserror::Error)]
pub enum TeletextDesignExportStoreError {
    #[error("durable teletext export storage is unavailable")]
    Unavailable,
    #[error("durable teletext export storage failed")]
    StorageUnavailable,
    #[error(transparent)]
    Validation(#[from] TeletextDesignExportValidationError),
}

impl ApplicationServices {
    /// Persist an exact server-owned export baseline after native confirmation.
    ///
    /// The final immediate storage transaction rechecks the observed design
    /// head and original digest. Confirmation is not publication or authority
    /// to import arbitrary editor bytes. No addon is invoked by this operation.
    /// # Errors
    /// Reports unavailable/corrupt storage or failed native confirmation.
    /// Missing originals and stale digest/head are explicit mutation rejections.
    pub async fn create_teletext_design_export(
        &self,
        export: TeletextDesignExportSave,
        validator: &dyn TeletextDesignExportValidator,
        max_receipt_bytes: usize,
        max_document_bytes: usize,
    ) -> Result<TeletextDesignMutation<TeletextDesignExportMetadata>, TeletextDesignExportStoreError>
    {
        if !export.is_valid(max_receipt_bytes, max_document_bytes) {
            return Ok(TeletextDesignMutation::Rejected(
                TeletextDesignRejection::InvalidValues,
            ));
        }
        let repository = self
            .ports()
            .teletext_designs()
            .ok_or(TeletextDesignExportStoreError::Unavailable)?;
        let original = repository
            .revision(export.design_id, export.revision, max_document_bytes)
            .await
            .map_err(|_| TeletextDesignExportStoreError::StorageUnavailable)?;
        let Some(original) = original else {
            return Ok(TeletextDesignMutation::Rejected(
                TeletextDesignRejection::MissingDesign,
            ));
        };
        if original.digest != export.design_digest {
            return Ok(TeletextDesignMutation::Rejected(
                TeletextDesignRejection::Conflict,
            ));
        }
        validator.confirm_export(&original, &export).await?;
        repository
            .create_export(&export, max_receipt_bytes, max_document_bytes)
            .await
            .map_err(|_| TeletextDesignExportStoreError::StorageUnavailable)
    }

    /// Read a bounded immutable export receipt, including after design deletion.
    ///
    /// The repository checks stored digests and the original revision; returned
    /// bytes remain opaque historical data, not checked import/edit authority.
    /// # Errors
    /// Reports unavailable storage and invalid/corrupt/oversized persisted data.
    pub async fn teletext_design_export_receipt(
        &self,
        id: EntityId,
        max_receipt_bytes: usize,
        max_document_bytes: usize,
    ) -> Result<Option<TeletextDesignExportReceipt>, TeletextDesignExportStoreError> {
        self.ports()
            .teletext_designs()
            .ok_or(TeletextDesignExportStoreError::Unavailable)?
            .export(id, max_receipt_bytes, max_document_bytes)
            .await
            .map_err(|_| TeletextDesignExportStoreError::StorageUnavailable)
    }
}
