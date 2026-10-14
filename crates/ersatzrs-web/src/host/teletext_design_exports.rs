//! Native, retained editor baselines without concept or publication authority.

use std::{fmt, fmt::Write as _, time::Instant};

use async_trait::async_trait;
use ersatzrs_app::{
    teletext_design_documents::TeletextDesignDocumentBudget,
    teletext_design_exports::{
        TeletextDesignExportStoreError, TeletextDesignExportValidationError,
        TeletextDesignExportValidator,
    },
    teletext_export_project::{
        write_teletext_export_receipt_body, write_teletext_export_sidecar, TeletextExportFile,
        TeletextExportOriginal, TeletextExportProject, TeletextExportProjectBudget,
        TeletextExportSidecar,
    },
    teletext_tti::TeletextTtiBudget,
    ApplicationServices,
};
use ersatzrs_core::{
    locking::TeletextWorkHold,
    repositories::{TeletextDesignExportSave, TeletextDesignMutation, TeletextDesignRejection},
};
use ersatzrs_domain::{
    EntityId, TeletextDesignExportMetadata, TeletextDesignRevision, TeletextPageAddress,
    TELETEXT_DESIGN_EXPORT_RECEIPT_VERSION,
};
use sha2::{Digest, Sha256};

pub use super::teletext_export_project::NativeTeletextExportProjectError;
use super::{
    teletext_channel_documents::TeletextChannelDocumentEncodingPolicy,
    teletext_designs::NativeTeletextDesignError,
    teletext_export_project::{build_native_teletext_export_project, TeletextEditorCarrier},
};

/// Explicit historical revision and live-head fence; absence selects all displays.
pub struct NativeTeletextExportRequest {
    pub design_id: EntityId,
    pub revision: i64,
    pub design_digest: String,
    pub expected_design_head: i64,
    pub pages: Option<Vec<TeletextPageAddress>>,
}
impl NativeTeletextExportRequest {
    fn is_valid(&self, maximum_files: usize) -> bool {
        self.design_id > 0
            && self.revision > 0
            && self.expected_design_head >= self.revision
            && self.design_digest.len() == 64
            && self
                .design_digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
            && self
                .pages
                .as_ref()
                .is_none_or(|pages| !pages.is_empty() && pages.len() <= maximum_files)
    }
}
impl fmt::Debug for NativeTeletextExportRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeTeletextExportRequest")
            .field("design_id", &self.design_id)
            .field("revision", &self.revision)
            .field("expected_design_head", &self.expected_design_head)
            .field("selected_pages", &self.pages.as_ref().map(Vec::len))
            .finish_non_exhaustive()
    }
}

/// Caller bounds reuse existing native compilation, TTI and storage contracts.
pub struct NativeTeletextExportPolicy {
    pub document: TeletextDesignDocumentBudget,
    pub encoding: TeletextChannelDocumentEncodingPolicy,
    pub project: TeletextExportProjectBudget,
    pub tti: TeletextTtiBudget,
}

/// Exact exported files and descriptive metadata backed by a durable receipt.
/// This value grants no editing, installation or active-output authority.
pub struct ExportedOriginalTeletextDesign {
    metadata: TeletextDesignExportMetadata,
    sidecar: TeletextExportSidecar,
    files: Vec<TeletextExportFile>,
}
impl ExportedOriginalTeletextDesign {
    #[must_use]
    pub const fn metadata(&self) -> &TeletextDesignExportMetadata {
        &self.metadata
    }
    #[must_use]
    pub const fn sidecar(&self) -> &TeletextExportSidecar {
        &self.sidecar
    }
    #[must_use]
    pub fn files(&self) -> &[TeletextExportFile] {
        &self.files
    }
}
impl fmt::Debug for ExportedOriginalTeletextDesign {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExportedOriginalTeletextDesign")
            .field("metadata", &self.metadata)
            .field("files", &self.files.len())
            .finish_non_exhaustive()
    }
}

/// Redacted export failures; rejection never changes accepted output.
#[derive(Debug, thiserror::Error)]
pub enum NativeTeletextExportError {
    #[error(transparent)]
    Design(#[from] NativeTeletextDesignError),
    #[error(transparent)]
    Storage(#[from] TeletextDesignExportStoreError),
    #[error("native teletext editor mapping: {0}")]
    Mapping(Box<NativeTeletextExportProjectError>),
    #[error(transparent)]
    Project(#[from] ersatzrs_app::teletext_export_project::TeletextExportProjectError),
    #[error("teletext export operation exceeded its deadline")]
    TimedOut,
    #[error("teletext export worker failed")]
    WorkerFailed,
}
fn check_deadline(deadline: Instant) -> Result<(), NativeTeletextExportError> {
    if Instant::now() >= deadline {
        Err(NativeTeletextExportError::TimedOut)
    } else {
        Ok(())
    }
}
fn digest(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        let _ = write!(text, "{byte:02x}");
    }
    text
}

fn preflight_sidecar(
    project: &TeletextExportProject,
    receipt_digest: &str,
    budget: TeletextExportProjectBudget,
) -> Result<(), NativeTeletextExportError> {
    // Every positive SQLite identity is at most this many digits. Prove public
    // metadata fits before commit; the actual sidecar is returned typed.
    write_teletext_export_sidecar(
        &TeletextExportSidecar {
            schema_version: 1,
            receipt_id: i64::MAX,
            receipt_digest: receipt_digest.to_owned(),
            project: project.clone(),
        },
        budget,
    )?;
    Ok(())
}

// Constructed only alongside full native mapping of the exact restored original.
// Uploaded projects, files or sidecars cannot create this confirmation.
struct NativeExportConfirmation {
    original: TeletextExportOriginal,
    receipt_digest: String,
    max_document_bytes: usize,
    max_receipt_bytes: usize,
    deadline: Instant,
    hold: TeletextWorkHold,
}
#[async_trait]
impl TeletextDesignExportValidator for NativeExportConfirmation {
    async fn confirm_export(
        &self,
        original: &TeletextDesignRevision,
        export: &TeletextDesignExportSave,
    ) -> Result<(), TeletextDesignExportValidationError> {
        use TeletextDesignExportValidationError as Error;
        if Instant::now() >= self.deadline {
            return Err(Error::TimedOut);
        }
        if original.design_id != self.original.design_id
            || original.revision != self.original.revision
            || original.document_version != self.original.document_version
            || original.digest != self.original.digest
            || export.design_id != self.original.design_id
            || export.revision != self.original.revision
            || export.design_digest != self.original.digest
            || export.expected_design_head != self.original.expected_head
            || export.receipt_version != TELETEXT_DESIGN_EXPORT_RECEIPT_VERSION
            || original.document.len() > self.max_document_bytes
            || export.receipt.len() > self.max_receipt_bytes
        {
            return Err(Error::Association);
        }
        // Borrowed immutable port inputs have checked byte bounds. Their hashing
        // worker retains the original admission even if this requester is dropped.
        let document = original.document.clone();
        let receipt = export.receipt.clone();
        let expected_document = self.original.digest.clone();
        let expected_receipt = self.receipt_digest.clone();
        let deadline = self.deadline;
        super::teletext_work::spawn_blocking(self.hold.clone(), move || {
            if Instant::now() >= deadline {
                return Err(Error::TimedOut);
            }
            if digest(&document) != expected_document || digest(&receipt) != expected_receipt {
                return Err(Error::Association);
            }
            if Instant::now() >= deadline {
                return Err(Error::TimedOut);
            }
            Ok(())
        })
        .await
        .map_err(|_| Error::WorkerFailed)?
    }
}

/// Retain an already qualified root projection under the observed live head.
/// Only the native editorial adapter calls this after both protected effect
/// comparisons; receipt settlement does not publish or advance that head.
pub(super) async fn retain_native_root_export(
    application: &ApplicationServices,
    body: ersatzrs_app::teletext_export_project::TeletextExportReceiptBody,
    document: TeletextDesignDocumentBudget,
    project: TeletextExportProjectBudget,
    admission: (Instant, TeletextWorkHold),
) -> Result<TeletextDesignMutation<TeletextDesignExportMetadata>, NativeTeletextExportError> {
    let (deadline, hold) = admission;
    let (original, receipt, receipt_digest) =
        super::teletext_work::spawn_blocking(hold.clone(), move || {
            check_deadline(deadline)?;
            let receipt = write_teletext_export_receipt_body(&body, project)?;
            let receipt_digest = digest(&receipt);
            preflight_sidecar(&body.project, &receipt_digest, project)?;
            check_deadline(deadline)?;
            Ok::<_, NativeTeletextExportError>((body.project.original, receipt, receipt_digest))
        })
        .await
        .map_err(|_| NativeTeletextExportError::WorkerFailed)??;
    let confirmation = NativeExportConfirmation {
        original,
        receipt_digest,
        max_document_bytes: document.max_serialized_bytes,
        max_receipt_bytes: project.max_bytes,
        deadline,
        hold,
    };
    application
        .create_teletext_design_export(
            TeletextDesignExportSave {
                design_id: confirmation.original.design_id,
                revision: confirmation.original.revision,
                design_digest: confirmation.original.digest.clone(),
                expected_design_head: confirmation.original.expected_head,
                receipt_version: TELETEXT_DESIGN_EXPORT_RECEIPT_VERSION,
                receipt,
            },
            &confirmation,
            project.max_bytes,
            document.max_serialized_bytes,
        )
        .await
        .map_err(Into::into)
}

/// Export privately restored native originals and retain an exact server baseline.
///
/// No package runs, design head advances, concept is saved or output is selected.
/// CPU work shares one existing admission/deadline. Storage retains its atomic
/// transaction timeout; expiry cannot undo or misreport a committed save.
/// Selected displays retain all service resource/magazine dependencies. This is
/// an export seam, not proof of a complete editor round trip.
/// # Errors
/// Reports native mapping, codec, deadline, worker or storage failures. Missing,
/// mismatched or concurrently changed originals return typed mutation rejection.
pub async fn export_original_teletext_design(
    application: &ApplicationServices,
    request: NativeTeletextExportRequest,
    policy: NativeTeletextExportPolicy,
    deadline: Instant,
) -> Result<TeletextDesignMutation<ExportedOriginalTeletextDesign>, NativeTeletextExportError> {
    check_deadline(deadline)?;
    if !request.is_valid(policy.project.max_files) {
        return Ok(TeletextDesignMutation::Rejected(
            TeletextDesignRejection::InvalidValues,
        ));
    }
    let guard = super::teletext_work::begin(application, Some(deadline))
        .await
        .map_err(NativeTeletextDesignError::from)?;
    let Some(original) = super::teletext_stored_designs::reload_stored_with_hold(
        application,
        request.design_id,
        request.revision,
        policy.document,
        policy.encoding,
        deadline,
        guard.hold_work(),
    )
    .await?
    else {
        return Ok(TeletextDesignMutation::Rejected(
            TeletextDesignRejection::MissingDesign,
        ));
    };
    if original.view().digest() != request.design_digest {
        return Ok(TeletextDesignMutation::Rejected(
            TeletextDesignRejection::Conflict,
        ));
    }
    let project_budget = policy.project;
    let (project, files, receipt, receipt_digest) =
        super::teletext_work::spawn_blocking(guard.hold_work(), move || {
            check_deadline(deadline)?;
            let body = build_native_teletext_export_project(
                original.view(),
                request.expected_design_head,
                request.pages.as_deref(),
                project_budget,
                policy.tti,
                deadline,
                TeletextEditorCarrier::V4,
            )
            .map_err(|error| NativeTeletextExportError::Mapping(Box::new(error)))?;
            let receipt = write_teletext_export_receipt_body(&body, project_budget)?;
            let receipt_digest = digest(&receipt);
            preflight_sidecar(&body.project, &receipt_digest, project_budget)?;
            check_deadline(deadline)?;
            Ok::<_, NativeTeletextExportError>((body.project, body.files, receipt, receipt_digest))
        })
        .await
        .map_err(|_| NativeTeletextExportError::WorkerFailed)??;
    let confirmation = NativeExportConfirmation {
        original: project.original.clone(),
        receipt_digest,
        max_document_bytes: policy.document.max_serialized_bytes,
        max_receipt_bytes: project_budget.max_bytes,
        deadline,
        hold: guard.hold_work(),
    };
    let save = TeletextDesignExportSave {
        design_id: confirmation.original.design_id,
        revision: confirmation.original.revision,
        design_digest: confirmation.original.digest.clone(),
        expected_design_head: confirmation.original.expected_head,
        receipt_version: TELETEXT_DESIGN_EXPORT_RECEIPT_VERSION,
        receipt,
    };
    match application
        .create_teletext_design_export(
            save,
            &confirmation,
            project_budget.max_bytes,
            policy.document.max_serialized_bytes,
        )
        .await?
    {
        TeletextDesignMutation::Rejected(reason) => Ok(TeletextDesignMutation::Rejected(reason)),
        TeletextDesignMutation::Saved(metadata) => {
            // No fallible encoding or deadline gate follows the durable commit.
            let sidecar = TeletextExportSidecar {
                schema_version: 1,
                receipt_id: metadata.id,
                receipt_digest: metadata.digest.clone(),
                project,
            };
            Ok(TeletextDesignMutation::Saved(
                ExportedOriginalTeletextDesign {
                    metadata,
                    sidecar,
                    files,
                },
            ))
        }
    }
}

#[cfg(test)]
#[path = "teletext_design_exports/tests.rs"]
mod tests;
