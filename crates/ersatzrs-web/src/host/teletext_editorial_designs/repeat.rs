//! Bounded cumulative edits over one authenticated overlay-free original.

use super::{
    append_overlay, baseline_body, build_native_teletext_export_project, check_deadline,
    check_request, digest, import_limits, parse_teletext_export_sidecar, qualify_baseline,
    qualify_rows, read_editorial_baseline, reconcile_files, save_editorial_concept,
    ApplicationServices, EditorialBaseline, EncodedStoredDesign, Instant,
    NativeTeletextEditorialImportError, NativeTeletextEditorialImportRequest,
    NativeTeletextExportPolicy, QualifiedBaseline, Result, SavedEditorialTeletextDesign,
    StoredDesignParts, TeletextDesignHead, TeletextDesignMutation, TeletextDesignRevision,
    TeletextEditorCarrier, TeletextExportReceiptBody, TeletextPageAddress, TeletextPageNumber,
    TeletextPhysicalBaseOverlay, TeletextSavedEditorialOverlay, TeletextSavedEditorialProvenance,
    TeletextSubcode,
};
use ersatzrs_core::locking::TeletextWorkHold;
use ersatzrs_domain::TeletextPhysicalBaseRow;
use std::collections::BTreeSet;

struct PreparedImport {
    root: QualifiedBaseline,
    latest: Option<QualifiedBaseline>,
    incoming: Vec<TeletextPhysicalBaseOverlay>,
    cumulative: Vec<TeletextPhysicalBaseOverlay>,
    provenance: TeletextSavedEditorialProvenance,
    root_export: Option<TeletextExportReceiptBody>,
    qualified: Option<(EncodedStoredDesign, String)>,
    next_revision: i64,
    policy: NativeTeletextExportPolicy,
}

fn provenance(baseline: &EditorialBaseline) -> TeletextSavedEditorialProvenance {
    TeletextSavedEditorialProvenance {
        export_receipt_id: baseline.receipt.metadata.id,
        export_receipt_digest: baseline.receipt.metadata.digest.clone(),
        original_design_id: baseline.original.design_id,
        original_revision: baseline.original.revision,
        original_digest: baseline.original.digest.clone(),
    }
}

async fn read_root(
    application: &ApplicationServices,
    latest: &EditorialBaseline,
    policy: &NativeTeletextExportPolicy,
    admission: (Instant, TeletextWorkHold),
) -> Result<Option<EditorialBaseline>> {
    let (deadline, hold) = admission;
    let stored = latest.original.clone();
    let budget = policy.document;
    let overlay = super::super::teletext_work::spawn_blocking(hold, move || {
        check_deadline(deadline)?;
        let parts = StoredDesignParts::decode(&stored, budget)?;
        check_deadline(deadline)?;
        Ok::<_, NativeTeletextEditorialImportError>(
            parts.overlay().map(|value| value.provenance().clone()),
        )
    })
    .await
    .map_err(|_| NativeTeletextEditorialImportError::WorkerFailed)??;
    let Some(overlay) = overlay else {
        return Ok(None);
    };
    // Exactly one root lookup: qualify_baseline rejects a root containing an
    // overlay, rather than recursively walking an unbounded revision chain.
    read_editorial_baseline(
        application,
        &overlay,
        latest.original.design_id,
        Some(latest.original.revision),
        policy.document,
        &policy.encoding,
        deadline,
    )
    .await
    .map(Some)
}

fn addresses(body: &TeletextExportReceiptBody) -> Result<Vec<TeletextPageAddress>> {
    body.project
        .pages
        .iter()
        .map(|mapping| {
            Ok(TeletextPageAddress {
                page: TeletextPageNumber::new(mapping.page.page)
                    .map_err(|_| NativeTeletextEditorialImportError::Association)?,
                subcode: TeletextSubcode::new(mapping.page.subcode)
                    .map_err(|_| NativeTeletextEditorialImportError::Association)?,
            })
        })
        .collect()
}

fn qualify_latest(
    latest: EditorialBaseline,
    root: &QualifiedBaseline,
    policy: &NativeTeletextExportPolicy,
    deadline: Instant,
) -> Result<(QualifiedBaseline, Vec<TeletextPhysicalBaseOverlay>)> {
    let (project, tti) = import_limits(policy);
    let body = baseline_body(&latest, project)?;
    let carrier = TeletextEditorCarrier::from_profile(&body.project.editor_profile)
        .ok_or(NativeTeletextEditorialImportError::Association)?;
    let parts = StoredDesignParts::decode(&latest.original, policy.document)?;
    let overlay = parts
        .overlay()
        .ok_or(NativeTeletextEditorialImportError::Association)?;
    let expected = append_overlay(
        root.original.view().document_version(),
        &root.original_document,
        overlay,
        policy.document,
    )?;
    if expected.bytes() != latest.original.document {
        return Err(NativeTeletextEditorialImportError::Association);
    }
    let previous = overlay.pages().to_vec();
    let original_document = latest.original.document.clone();
    let original = qualify_rows(
        latest.original,
        parts,
        root,
        policy.encoding.clone(),
        deadline,
    )?;
    let generated = build_native_teletext_export_project(
        original.view(),
        body.project.original.expected_head,
        Some(&addresses(&body)?),
        project,
        tti,
        deadline,
        carrier,
    )
    .map_err(|error| NativeTeletextEditorialImportError::Mapping(Box::new(error)))?;
    if body != generated {
        return Err(NativeTeletextEditorialImportError::Association);
    }
    Ok((
        QualifiedBaseline {
            body,
            carrier,
            original,
            original_document,
        },
        previous,
    ))
}

fn merge_rows(
    previous: &[TeletextPhysicalBaseOverlay],
    incoming: &[TeletextPhysicalBaseOverlay],
    policy: &NativeTeletextExportPolicy,
    deadline: Instant,
) -> Result<Vec<TeletextPhysicalBaseOverlay>> {
    let service = policy.document.validation.service;
    let maximum = service.max_records.min(policy.encoding.encoding.max_pages);
    let mut owners = BTreeSet::new();
    for overlay in previous.iter().chain(incoming) {
        check_deadline(deadline)?;
        if !owners.contains(&overlay.owner()) && owners.len() >= maximum {
            return Err(NativeTeletextEditorialImportError::Budget);
        }
        owners.insert(overlay.owner());
    }
    let mut rows = 0usize;
    for owner in &owners {
        let mut occupied = [false; 25];
        for overlay in previous
            .iter()
            .chain(incoming)
            .filter(|value| value.owner() == *owner)
        {
            for row in overlay.rows() {
                occupied[usize::from(row.row())] = true;
            }
        }
        rows = rows
            .checked_add(occupied.into_iter().filter(|present| *present).count())
            .ok_or(NativeTeletextEditorialImportError::Budget)?;
    }
    if owners
        .len()
        .checked_add(rows)
        .is_none_or(|count| count > service.max_structure_items)
        || rows
            .checked_mul(40)
            .is_none_or(|count| count > service.max_base_content_bytes)
    {
        return Err(NativeTeletextEditorialImportError::Budget);
    }
    let mut result = Vec::with_capacity(owners.len());
    for owner in owners {
        check_deadline(deadline)?;
        let mut merged: [Option<&TeletextPhysicalBaseRow>; 25] = [None; 25];
        for overlay in previous
            .iter()
            .chain(incoming)
            .filter(|value| value.owner() == owner)
        {
            for row in overlay.rows() {
                merged[usize::from(row.row())] = Some(row);
            }
        }
        // Keep explicit root-equal rows: undo is an edit relative to the latest
        // files, and a complete revert still has one nonempty cumulative overlay.
        let rows = merged.into_iter().flatten().cloned().collect();
        result.push(
            TeletextPhysicalBaseOverlay::new(owner, rows)
                .map_err(|_| NativeTeletextEditorialImportError::Association)?,
        );
    }
    Ok(result)
}

fn prepare(
    request: &NativeTeletextEditorialImportRequest,
    latest: EditorialBaseline,
    root: Option<EditorialBaseline>,
    policy: NativeTeletextExportPolicy,
    deadline: Instant,
) -> Result<Box<PreparedImport>> {
    let (project, tti) = import_limits(&policy);
    check_request(request, project)?;
    let sidecar = parse_teletext_export_sidecar(&request.sidecar, project)?;
    if sidecar.receipt_id != request.receipt_id || sidecar.receipt_digest != request.receipt_digest
    {
        return Err(NativeTeletextEditorialImportError::Association);
    }
    let (mut root, latest, previous, provenance) = match root {
        None => {
            let provenance = provenance(&latest);
            (
                qualify_baseline(
                    latest,
                    policy.document,
                    policy.encoding.clone(),
                    project,
                    tti,
                    deadline,
                )?,
                None,
                Vec::new(),
                provenance,
            )
        }
        Some(root) => {
            let provenance = provenance(&root);
            let root = qualify_baseline(
                root,
                policy.document,
                policy.encoding.clone(),
                project,
                tti,
                deadline,
            )?;
            let (latest, previous) = qualify_latest(latest, &root, &policy, deadline)?;
            (root, Some(latest), previous, provenance)
        }
    };
    let baseline = latest.as_ref().unwrap_or(&root);
    if sidecar.project != baseline.body.project {
        return Err(NativeTeletextEditorialImportError::Association);
    }
    // NoChanges compares latest files, before cumulative merge/root effects.
    let incoming = reconcile_files(baseline, &request.files, tti, deadline)?;
    let cumulative = merge_rows(&previous, &incoming, &policy, deadline)?;
    let owners: Vec<_> = cumulative
        .iter()
        .map(TeletextPhysicalBaseOverlay::owner)
        .collect();
    let covered = owners.iter().all(|owner| {
        root.body.project.pages.iter().any(|mapping| {
            mapping.page.page == owner.page.get() && mapping.page.subcode == owner.subcode.get()
        })
    });
    let root_export = if covered {
        None
    } else {
        let body = build_native_teletext_export_project(
            root.original.view(),
            request.expected_design_head,
            Some(&owners),
            project,
            tti,
            deadline,
            TeletextEditorCarrier::V4,
        )
        .map_err(|error| NativeTeletextEditorialImportError::Mapping(Box::new(error)))?;
        root.body = body.clone();
        Some(body)
    };
    let mut prepared = Box::new(PreparedImport {
        root,
        latest,
        incoming,
        cumulative,
        provenance,
        root_export,
        qualified: None,
        next_revision: request
            .expected_design_head
            .checked_add(1)
            .ok_or(NativeTeletextEditorialImportError::Budget)?,
        policy,
    });
    // Fully qualify content before a possible durable union-owner root receipt.
    prepared.qualified = Some(make_document(&prepared, deadline)?);
    Ok(prepared)
}

fn make_document(
    prepared: &PreparedImport,
    deadline: Instant,
) -> Result<(EncodedStoredDesign, String)> {
    check_deadline(deadline)?;
    let overlay = TeletextSavedEditorialOverlay::new(
        prepared.provenance.clone(),
        prepared.cumulative.clone(),
    )?;
    let document = append_overlay(
        prepared.root.original.view().document_version(),
        &prepared.root.original_document,
        &overlay,
        prepared.policy.document,
    )?;
    let document_digest = digest(document.bytes());
    let stored = TeletextDesignRevision {
        design_id: prepared.root.original.view().design_id(),
        revision: prepared.next_revision,
        document_version: document.version(),
        digest: document_digest.clone(),
        document: document.bytes().to_vec(),
    };
    let parts = StoredDesignParts::decode(&stored, prepared.policy.document)?;
    let edited = qualify_rows(
        stored,
        parts,
        &prepared.root,
        prepared.policy.encoding.clone(),
        deadline,
    )?;
    let baseline = prepared.latest.as_ref().unwrap_or(&prepared.root);
    super::super::teletext_editorial_effects::validate_teletext_editorial_transition(
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
        &prepared.incoming,
        &prepared.cumulative,
        &prepared.policy.encoding.encoding,
        deadline,
    )
    .map_err(|error| NativeTeletextEditorialImportError::Effects(Box::new(error)))?;
    check_deadline(deadline)?;
    Ok((document, document_digest))
}

fn export_error(
    error: super::super::teletext_design_exports::NativeTeletextExportError,
) -> NativeTeletextEditorialImportError {
    use super::super::teletext_design_exports::NativeTeletextExportError as Error;
    match error {
        Error::Design(error) => NativeTeletextEditorialImportError::Design(Box::new(error)),
        Error::Storage(error) => NativeTeletextEditorialImportError::ExportStorage(error),
        Error::Mapping(error) => NativeTeletextEditorialImportError::Mapping(error),
        Error::Project(error) => NativeTeletextEditorialImportError::Project(error),
        Error::TimedOut => NativeTeletextEditorialImportError::TimedOut,
        Error::WorkerFailed => NativeTeletextEditorialImportError::WorkerFailed,
    }
}

pub(super) async fn import_and_save(
    application: &ApplicationServices,
    request: NativeTeletextEditorialImportRequest,
    baseline: EditorialBaseline,
    head: TeletextDesignHead,
    policy: NativeTeletextExportPolicy,
    admission: (Instant, TeletextWorkHold),
) -> Result<TeletextDesignMutation<SavedEditorialTeletextDesign>> {
    let (deadline, hold) = admission;
    let name = request.name.clone();
    let root = read_root(application, &baseline, &policy, (deadline, hold.clone())).await?;
    let mut prepared = super::super::teletext_work::spawn_blocking(hold.clone(), move || {
        prepare(&request, baseline, root, policy, deadline)
    })
    .await
    .map_err(|_| NativeTeletextEditorialImportError::WorkerFailed)??;
    if let Some(body) = prepared.root_export.take() {
        let (project, _) = import_limits(&prepared.policy);
        let mutation = super::super::teletext_design_exports::retain_native_root_export(
            application,
            body,
            prepared.policy.document,
            project,
            (deadline, hold.clone()),
        )
        .await
        .map_err(export_error)?;
        let metadata = match mutation {
            TeletextDesignMutation::Saved(metadata) => metadata,
            TeletextDesignMutation::Rejected(reason) => {
                return Ok(TeletextDesignMutation::Rejected(reason))
            }
        };
        prepared.provenance.export_receipt_id = metadata.id;
        prepared.provenance.export_receipt_digest = metadata.digest;
        prepared.qualified = None;
    }
    let maximum = prepared.policy.document.max_serialized_bytes;
    let (document, document_digest) = match prepared.qualified.take() {
        Some(qualified) => qualified,
        None => super::super::teletext_work::spawn_blocking(hold.clone(), move || {
            make_document(&prepared, deadline)
        })
        .await
        .map_err(|_| NativeTeletextEditorialImportError::WorkerFailed)??,
    };
    save_editorial_concept(
        application,
        name,
        (head.id, head.revision),
        document,
        document_digest,
        maximum,
        (deadline, hold),
    )
    .await
}
