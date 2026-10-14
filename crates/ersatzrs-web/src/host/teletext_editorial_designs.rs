//! Retained editor baselines to qualified concepts, without broadcast mutation.

use std::{fmt, fmt::Write as _, time::Instant};

use ersatzrs_app::{
    teletext_design_documents::{
        append_teletext_design_editorial_overlay,
        append_teletext_effective_design_editorial_overlay,
        EncodedEffectiveTeletextDesignDocuments, EncodedTeletextDesignDocuments,
        TeletextDesignDocumentBudget, TeletextDesignDocumentError,
        TeletextRestoredDesignDocumentParts, TeletextSavedEditorialOverlay,
        TeletextSavedEditorialProvenance,
    },
    teletext_design_exports::TeletextDesignExportStoreError,
    teletext_designs::TeletextDesignStoreError,
    teletext_export_project::{
        parse_teletext_export_receipt_body, parse_teletext_export_sidecar, TeletextExportFile,
        TeletextExportProjectBudget, TeletextExportProjectError, TeletextExportReceiptBody,
    },
    teletext_tti::TeletextTtiBudget,
    ApplicationServices,
};
use ersatzrs_core::repositories::{TeletextDesignMutation, TeletextDesignRejection};
use ersatzrs_domain::{
    EntityId, TeletextDesignExportReceipt, TeletextDesignHead, TeletextDesignRevision,
    TeletextPageAddress, TeletextPageNumber, TeletextPhysicalBaseOverlay, TeletextSubcode,
    MAX_TELETEXT_DESIGN_NAME_BYTES, TELETEXT_DESIGN_EXPORT_RECEIPT_VERSION,
};
use sha2::{Digest, Sha256};

use super::{
    teletext_channel_documents::TeletextChannelDocumentEncodingPolicy,
    teletext_design_exports::NativeTeletextExportPolicy,
    teletext_designs::{
        native_design_confirmation, NativeTeletextDesignError, RestoredOriginalTeletextDesign,
    },
    teletext_editorial_effects::validate_teletext_editorial_effects,
    teletext_editorial_tti::{
        reconcile_teletext_editorial_tti, reconcile_teletext_editorial_tti_with_status,
    },
    teletext_effective_designs::{native_effective_confirmation, RestoredEffectiveOriginalDesign},
    teletext_export_project::{
        build_native_teletext_export_project, editor_primary_for_page,
        NativeTeletextExportProjectError, TeletextEditorCarrier,
    },
    teletext_stored_designs::{RestoredStoredTeletextDesign, StoredDesignParts},
};
pub use super::{
    teletext_editorial_effects::{TeletextEditorialEffectsError, TeletextEditorialEffectsFailure},
    teletext_editorial_tti::{NativeTeletextEditorialTtiError, NativeTeletextEditorialTtiFailure},
};

/// Untrusted complete exported set, including unchanged protected assets.
/// The observed live head is independent of the historical export's head fence.
pub struct NativeTeletextEditorialImportRequest {
    pub name: String,
    pub receipt_id: EntityId,
    pub receipt_digest: String,
    pub expected_design_head: i64,
    pub sidecar: Vec<u8>,
    pub files: Vec<TeletextExportFile>,
}
impl fmt::Debug for NativeTeletextEditorialImportRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeTeletextEditorialImportRequest")
            .field("receipt_id", &self.receipt_id)
            .field("expected_design_head", &self.expected_design_head)
            .field("files", &self.files.len())
            .finish_non_exhaustive()
    }
}

/// Immutable qualified concept identity; publication remains a separate action.
pub struct SavedEditorialTeletextDesign {
    head: TeletextDesignHead,
    digest: String,
}
impl fmt::Debug for SavedEditorialTeletextDesign {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SavedEditorialTeletextDesign")
            .field("head", &self.head)
            .finish_non_exhaustive()
    }
}
impl SavedEditorialTeletextDesign {
    #[must_use]
    pub const fn head(&self) -> &TeletextDesignHead {
        &self.head
    }
    #[must_use]
    pub fn digest(&self) -> &str {
        &self.digest
    }
}

/// Located failures without supplied filenames, text, hashes or private settings.
#[derive(Debug, thiserror::Error)]
pub enum NativeTeletextEditorialImportError {
    #[error("teletext editorial operation exceeded its deadline")]
    TimedOut,
    #[error("teletext editorial worker failed")]
    WorkerFailed,
    #[error("teletext editorial input exceeds trusted bounds")]
    Budget,
    #[error("the retained teletext export receipt is unavailable")]
    MissingReceipt,
    #[error("teletext editorial data does not match the retained original export")]
    Association,
    #[error("teletext editor input contains no changed display rows")]
    NoChanges,
    #[error(transparent)]
    Storage(#[from] TeletextDesignStoreError),
    #[error(transparent)]
    ExportStorage(#[from] TeletextDesignExportStoreError),
    #[error(transparent)]
    Document(#[from] TeletextDesignDocumentError),
    #[error(transparent)]
    Project(#[from] TeletextExportProjectError),
    #[error("teletext original export projection: {0}")]
    Mapping(Box<NativeTeletextExportProjectError>),
    #[error("teletext native qualification: {0}")]
    Design(Box<NativeTeletextDesignError>),
    #[error(transparent)]
    Tti(#[from] NativeTeletextEditorialTtiError),
    #[error("teletext protected edit effects: {0}")]
    Effects(Box<TeletextEditorialEffectsError>),
}
#[path = "teletext_editorial_designs/repeat.rs"]
mod repeat;

type Result<T> = std::result::Result<T, NativeTeletextEditorialImportError>;
fn check_deadline(deadline: Instant) -> Result<()> {
    if Instant::now() >= deadline {
        Err(NativeTeletextEditorialImportError::TimedOut)
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

/// Reuse explicit existing caps so import cannot save a concept that its same-
/// policy reload cannot authenticate. No independent product defaults are added.
fn reload_limits(
    document: TeletextDesignDocumentBudget,
    policy: &TeletextChannelDocumentEncodingPolicy,
) -> (TeletextExportProjectBudget, TeletextTtiBudget) {
    (
        TeletextExportProjectBudget {
            max_bytes: document.max_serialized_bytes,
            max_files: policy.encoding.max_units,
            max_producers: document.max_documents,
            max_file_bytes: document.max_serialized_bytes,
            max_units: policy.encoding.max_units,
        },
        TeletextTtiBudget {
            max_bytes: document.max_serialized_bytes,
            max_records: policy.encoding.max_units,
        },
    )
}
fn import_limits(
    policy: &NativeTeletextExportPolicy,
) -> (TeletextExportProjectBudget, TeletextTtiBudget) {
    let (mut project, mut tti) = reload_limits(policy.document, &policy.encoding);
    project.max_bytes = project.max_bytes.min(policy.project.max_bytes);
    project.max_files = project.max_files.min(policy.project.max_files);
    project.max_producers = project.max_producers.min(policy.project.max_producers);
    project.max_file_bytes = project.max_file_bytes.min(policy.project.max_file_bytes);
    project.max_units = project.max_units.min(policy.project.max_units);
    tti.max_bytes = tti.max_bytes.min(policy.tti.max_bytes);
    tti.max_records = tti.max_records.min(policy.tti.max_records);
    (project, tti)
}

pub(super) struct EditorialBaseline {
    receipt: TeletextDesignExportReceipt,
    original: TeletextDesignRevision,
}

/// Repository data is authoritative; uploaded provenance only selects a lookup.
pub(super) async fn read_editorial_baseline(
    application: &ApplicationServices,
    provenance: &TeletextSavedEditorialProvenance,
    design_id: EntityId,
    concept_revision: Option<i64>,
    budget: TeletextDesignDocumentBudget,
    policy: &TeletextChannelDocumentEncodingPolicy,
    deadline: Instant,
) -> Result<EditorialBaseline> {
    check_deadline(deadline)?;
    if provenance.original_design_id != design_id
        || concept_revision.is_some_and(|revision| provenance.original_revision >= revision)
    {
        return Err(NativeTeletextEditorialImportError::Association);
    }
    let (limits, _) = reload_limits(budget, policy);
    let receipt = application
        .teletext_design_export_receipt(
            provenance.export_receipt_id,
            limits.max_bytes,
            budget.max_serialized_bytes,
        )
        .await?
        .ok_or(NativeTeletextEditorialImportError::MissingReceipt)?;
    let meta = &receipt.metadata;
    if meta.id != provenance.export_receipt_id
        || meta.digest != provenance.export_receipt_digest
        || meta.design_id != design_id
        || meta.revision != provenance.original_revision
        || meta.design_digest != provenance.original_digest
        || meta.receipt_version != TELETEXT_DESIGN_EXPORT_RECEIPT_VERSION
    {
        return Err(NativeTeletextEditorialImportError::Association);
    }
    let original = application
        .teletext_design_revision(meta.design_id, meta.revision, budget.max_serialized_bytes)
        .await?
        .ok_or(NativeTeletextEditorialImportError::Association)?;
    if original.digest != provenance.original_digest {
        return Err(NativeTeletextEditorialImportError::Association);
    }
    check_deadline(deadline)?;
    Ok(EditorialBaseline { receipt, original })
}

struct QualifiedBaseline {
    body: TeletextExportReceiptBody,
    carrier: TeletextEditorCarrier,
    original: RestoredStoredTeletextDesign,
    original_document: Vec<u8>,
}
fn baseline_body(
    baseline: &EditorialBaseline,
    project: TeletextExportProjectBudget,
) -> Result<TeletextExportReceiptBody> {
    let body = parse_teletext_export_receipt_body(&baseline.receipt.receipt, project)?;
    let meta = &baseline.receipt.metadata;
    let captured = &body.project.original;
    if captured.design_id != meta.design_id
        || captured.revision != meta.revision
        || captured.digest != meta.design_digest
        || captured.expected_head != meta.expected_design_head
        || captured.document_version != baseline.original.document_version
        || digest(&baseline.original.document) != meta.design_digest
        || digest(&baseline.receipt.receipt) != meta.digest
    {
        return Err(NativeTeletextEditorialImportError::Association);
    }
    Ok(body)
}

fn qualify_baseline(
    baseline: EditorialBaseline,
    budget: TeletextDesignDocumentBudget,
    policy: TeletextChannelDocumentEncodingPolicy,
    project: TeletextExportProjectBudget,
    tti: TeletextTtiBudget,
    deadline: Instant,
) -> Result<QualifiedBaseline> {
    check_deadline(deadline)?;
    let body = baseline_body(&baseline, project)?;
    let carrier = TeletextEditorCarrier::from_profile(&body.project.editor_profile)
        .ok_or(NativeTeletextEditorialImportError::Association)?;
    let meta = &baseline.receipt.metadata;
    let parts = StoredDesignParts::decode(&baseline.original, budget)?;
    if parts.overlay().is_some() {
        return Err(NativeTeletextEditorialImportError::Association);
    }
    let selected = body
        .project
        .pages
        .iter()
        .map(|page| {
            Ok(TeletextPageAddress {
                page: TeletextPageNumber::new(page.page.page)
                    .map_err(|_| NativeTeletextEditorialImportError::Association)?,
                subcode: TeletextSubcode::new(page.page.subcode)
                    .map_err(|_| NativeTeletextEditorialImportError::Association)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let original_document = baseline.original.document.clone();
    let original = parts
        .compile(baseline.original, policy, deadline, Vec::new())
        .map_err(|error| NativeTeletextEditorialImportError::Design(Box::new(error)))?;
    let generated = build_native_teletext_export_project(
        original.view(),
        meta.expected_design_head,
        Some(&selected),
        project,
        tti,
        deadline,
        carrier,
    )
    .map_err(|error| NativeTeletextEditorialImportError::Mapping(Box::new(error)))?;
    if body != generated {
        return Err(NativeTeletextEditorialImportError::Association);
    }
    Ok(QualifiedBaseline {
        body,
        carrier,
        original,
        original_document,
    })
}

fn qualify_rows(
    stored: TeletextDesignRevision,
    parts: StoredDesignParts,
    baseline: &QualifiedBaseline,
    policy: TeletextChannelDocumentEncodingPolicy,
    deadline: Instant,
) -> Result<RestoredStoredTeletextDesign> {
    let overlays = parts
        .overlay()
        .ok_or(NativeTeletextEditorialImportError::Association)?
        .pages()
        .to_vec();
    let encoding = policy.encoding;
    let edited = parts
        .compile(stored, policy, deadline, overlays.clone())
        .map_err(|error| NativeTeletextEditorialImportError::Design(Box::new(error)))?;
    validate_teletext_editorial_effects(
        baseline
            .original
            .view()
            .encoded()
            .ok_or(NativeTeletextEditorialImportError::Association)?,
        edited
            .view()
            .encoded()
            .ok_or(NativeTeletextEditorialImportError::Association)?,
        &baseline.body.project,
        &overlays,
        &encoding,
        deadline,
    )
    .map_err(|error| NativeTeletextEditorialImportError::Effects(Box::new(error)))?;
    check_deadline(deadline)?;
    Ok(edited)
}

/// Authenticate the stored body before admitting its overlay to native replay.
pub(super) fn restore_editorial_design(
    stored: TeletextDesignRevision,
    parts: TeletextRestoredDesignDocumentParts,
    baseline: EditorialBaseline,
    budget: TeletextDesignDocumentBudget,
    policy: TeletextChannelDocumentEncodingPolicy,
    deadline: Instant,
) -> Result<RestoredOriginalTeletextDesign> {
    match restore_stored_editorial_design(
        stored,
        StoredDesignParts::Base(parts),
        baseline,
        budget,
        policy,
        deadline,
    )? {
        RestoredStoredTeletextDesign::Base(value) => Ok(value),
        RestoredStoredTeletextDesign::Effective(_) => {
            Err(NativeTeletextEditorialImportError::Association)
        }
    }
}

pub(super) fn restore_editorial_effective_design(
    stored: TeletextDesignRevision,
    parts: ersatzrs_app::teletext_design_documents::TeletextRestoredEffectiveDesignDocumentParts,
    baseline: EditorialBaseline,
    budget: TeletextDesignDocumentBudget,
    policy: TeletextChannelDocumentEncodingPolicy,
    deadline: Instant,
) -> Result<RestoredEffectiveOriginalDesign> {
    match restore_stored_editorial_design(
        stored,
        StoredDesignParts::Effective(parts),
        baseline,
        budget,
        policy,
        deadline,
    )? {
        RestoredStoredTeletextDesign::Effective(value) => Ok(value),
        RestoredStoredTeletextDesign::Base(_) => {
            Err(NativeTeletextEditorialImportError::Association)
        }
    }
}

fn restore_stored_editorial_design(
    stored: TeletextDesignRevision,
    parts: StoredDesignParts,
    baseline: EditorialBaseline,
    budget: TeletextDesignDocumentBudget,
    policy: TeletextChannelDocumentEncodingPolicy,
    deadline: Instant,
) -> Result<RestoredStoredTeletextDesign> {
    let (project, tti) = reload_limits(budget, &policy);
    let baseline = qualify_baseline(baseline, budget, policy.clone(), project, tti, deadline)?;
    let overlay = parts
        .overlay()
        .ok_or(NativeTeletextEditorialImportError::Association)?;
    let expected = append_overlay(
        baseline.original.view().document_version(),
        &baseline.original_document,
        overlay,
        budget,
    )?;
    if expected.bytes() != stored.document {
        return Err(NativeTeletextEditorialImportError::Association);
    }
    qualify_rows(stored, parts, &baseline, policy, deadline)
}

fn check_request(
    request: &NativeTeletextEditorialImportRequest,
    limits: TeletextExportProjectBudget,
) -> Result<()> {
    if request.name.trim().is_empty()
        || request.name.len() > MAX_TELETEXT_DESIGN_NAME_BYTES
        || request.receipt_id <= 0
        || request.expected_design_head <= 0
        || request.receipt_digest.len() != 64
        || request.sidecar.len() > limits.max_bytes
        || request.files.len() > limits.max_files
    {
        return Err(NativeTeletextEditorialImportError::Budget);
    }
    let mut bytes = request.sidecar.len();
    for (ordinal, file) in request.files.iter().enumerate() {
        bytes = bytes
            .checked_add(file.name.len())
            .and_then(|value| value.checked_add(file.bytes.len()))
            .ok_or(NativeTeletextEditorialImportError::Budget)?;
        if file.bytes.len() > limits.max_file_bytes
            || bytes > limits.max_bytes
            || request.files[..ordinal]
                .iter()
                .any(|old| old.name == file.name)
        {
            return Err(NativeTeletextEditorialImportError::Budget);
        }
    }
    Ok(())
}
fn reconcile_files(
    baseline: &QualifiedBaseline,
    files: &[TeletextExportFile],
    tti: TeletextTtiBudget,
    deadline: Instant,
) -> Result<Vec<TeletextPhysicalBaseOverlay>> {
    let body = &baseline.body;
    if files.len() != body.files.len() {
        return Err(NativeTeletextEditorialImportError::Association);
    }
    let mut overlays = Vec::new();
    for original in &body.files {
        check_deadline(deadline)?;
        let edited = files
            .iter()
            .find(|file| file.name == original.name)
            .ok_or(NativeTeletextEditorialImportError::Association)?;
        if let Some(mapping) = body
            .project
            .pages
            .iter()
            .find(|page| page.name == original.name)
        {
            let address = TeletextPageAddress {
                page: TeletextPageNumber::new(mapping.page.page)
                    .map_err(|_| NativeTeletextEditorialImportError::Association)?,
                subcode: TeletextSubcode::new(mapping.page.subcode)
                    .map_err(|_| NativeTeletextEditorialImportError::Association)?,
            };
            let primary = editor_primary_for_page(baseline.original.view(), address)
                .map_err(|error| NativeTeletextEditorialImportError::Mapping(Box::new(error)))?;
            let expected_status = baseline.carrier.status(mapping.header_controls, primary);
            let overlay = match baseline.carrier {
                TeletextEditorCarrier::Legacy => reconcile_teletext_editorial_tti(
                    mapping,
                    &original.bytes,
                    &edited.bytes,
                    tti,
                    deadline,
                ),
                TeletextEditorCarrier::V2
                | TeletextEditorCarrier::V3
                | TeletextEditorCarrier::V4 => reconcile_teletext_editorial_tti_with_status(
                    mapping,
                    &original.bytes,
                    &edited.bytes,
                    tti,
                    deadline,
                    expected_status,
                ),
            }?;
            if let Some(overlay) = overlay {
                overlays.push(overlay);
            }
        } else if original.bytes != edited.bytes {
            return Err(NativeTeletextEditorialImportError::Association);
        }
    }
    if overlays.is_empty() {
        return Err(NativeTeletextEditorialImportError::NoChanges);
    }
    Ok(overlays)
}

enum EncodedStoredDesign {
    Base(EncodedTeletextDesignDocuments),
    Effective(EncodedEffectiveTeletextDesignDocuments),
}
impl EncodedStoredDesign {
    fn bytes(&self) -> &[u8] {
        match self {
            Self::Base(value) => value.bytes(),
            Self::Effective(value) => value.bytes(),
        }
    }
    fn version(&self) -> u16 {
        match self {
            Self::Base(value) => value.version(),
            Self::Effective(value) => value.version(),
        }
    }
}
fn append_overlay(
    version: u16,
    original: &[u8],
    overlay: &TeletextSavedEditorialOverlay,
    budget: TeletextDesignDocumentBudget,
) -> Result<EncodedStoredDesign> {
    match version {
        ersatzrs_domain::TELETEXT_DESIGN_DOCUMENT_VERSION => Ok(EncodedStoredDesign::Base(
            append_teletext_design_editorial_overlay(original, overlay, budget)?,
        )),
        ersatzrs_domain::TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION => {
            Ok(EncodedStoredDesign::Effective(
                append_teletext_effective_design_editorial_overlay(original, overlay, budget)?,
            ))
        }
        _ => Err(NativeTeletextEditorialImportError::Association),
    }
}

/// Import actual editor files as a new immutable revision of their owning design.
/// The retained receipt and full native protected effects are authenticated before
/// the existing head CAS. Neither pending/accepted output nor packages change.
/// # Errors
/// Returns located unsupported/tampered/editor/compiler/bound/deadline failures.
/// Missing, deleted and stale heads are explicit mutation rejections.
pub async fn import_teletext_editorial_design(
    application: &ApplicationServices,
    request: NativeTeletextEditorialImportRequest,
    policy: NativeTeletextExportPolicy,
    deadline: Instant,
) -> Result<TeletextDesignMutation<SavedEditorialTeletextDesign>> {
    check_deadline(deadline)?;
    let (limits, _) = import_limits(&policy);
    if request.name.len() > MAX_TELETEXT_DESIGN_NAME_BYTES
        || request.receipt_id <= 0
        || request.expected_design_head <= 0
        || request.receipt_digest.len() != 64
        || request.sidecar.len() > limits.max_bytes
        || request.files.len() > limits.max_files
    {
        return Err(NativeTeletextEditorialImportError::Budget);
    }
    let guard = super::teletext_work::begin(application, Some(deadline))
        .await
        .map_err(|error| NativeTeletextEditorialImportError::Design(Box::new(error.into())))?;
    let receipt = application
        .teletext_design_export_receipt(
            request.receipt_id,
            limits.max_bytes,
            policy.document.max_serialized_bytes,
        )
        .await?
        .ok_or(NativeTeletextEditorialImportError::MissingReceipt)?;
    if receipt.metadata.digest != request.receipt_digest {
        return Err(NativeTeletextEditorialImportError::Association);
    }
    let Some(head) = application
        .teletext_design_head(receipt.metadata.design_id)
        .await?
    else {
        return Ok(TeletextDesignMutation::Rejected(
            TeletextDesignRejection::MissingDesign,
        ));
    };
    if head.deleted {
        return Ok(TeletextDesignMutation::Rejected(
            TeletextDesignRejection::DeletedDesign,
        ));
    }
    if head.revision != request.expected_design_head {
        return Ok(TeletextDesignMutation::Rejected(
            TeletextDesignRejection::Conflict,
        ));
    }
    let provenance = TeletextSavedEditorialProvenance {
        export_receipt_id: receipt.metadata.id,
        export_receipt_digest: receipt.metadata.digest.clone(),
        original_design_id: receipt.metadata.design_id,
        original_revision: receipt.metadata.revision,
        original_digest: receipt.metadata.design_digest.clone(),
    };
    let baseline = read_editorial_baseline(
        application,
        &provenance,
        head.id,
        None,
        policy.document,
        &policy.encoding,
        deadline,
    )
    .await?;
    Box::pin(repeat::import_and_save(
        application,
        request,
        baseline,
        head,
        policy,
        (deadline, guard.hold_work()),
    ))
    .await
}

async fn save_editorial_concept(
    application: &ApplicationServices,
    name: String,
    existing: (EntityId, i64),
    document: EncodedStoredDesign,
    document_digest: String,
    maximum: usize,
    admission: (Instant, ersatzrs_core::locking::TeletextWorkHold),
) -> Result<TeletextDesignMutation<SavedEditorialTeletextDesign>> {
    let (deadline, hold) = admission;
    let mutation = match document {
        EncodedStoredDesign::Base(document) => {
            let confirmation =
                native_design_confirmation(document_digest.clone(), deadline, hold.clone());
            application
                .save_original_teletext_design(
                    name,
                    Some(existing),
                    document,
                    &confirmation,
                    maximum,
                )
                .await?
        }
        EncodedStoredDesign::Effective(document) => {
            let confirmation =
                native_effective_confirmation(document_digest.clone(), deadline, hold.clone());
            application
                .save_effective_original_teletext_design(
                    name,
                    Some(existing),
                    document,
                    &confirmation,
                    maximum,
                )
                .await?
        }
    };
    match mutation {
        TeletextDesignMutation::Saved(head) => Ok(TeletextDesignMutation::Saved(
            SavedEditorialTeletextDesign {
                head,
                digest: document_digest,
            },
        )),
        TeletextDesignMutation::Rejected(reason) => Ok(TeletextDesignMutation::Rejected(reason)),
    }
}
