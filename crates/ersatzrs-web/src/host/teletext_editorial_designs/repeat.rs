//! Bounded cumulative edits over one authenticated overlay-free original.

use super::super::teletext_editorial_effects::TeletextEditorialEdits;
use super::super::teletext_editorial_tti::TeletextEditorialChanges;
use super::{
    append_overlay, baseline_body, build_native_teletext_export_project, check_deadline,
    check_request, digest, import_limits, parse_teletext_export_sidecar, qualify_baseline,
    qualify_rows, read_editorial_baseline, reconcile_files, save_editorial_concept,
    validate_reconciled_files, ApplicationServices, EditorialBaseline, EncodedStoredDesign,
    Instant, NativeTeletextEditorialImportError, NativeTeletextEditorialImportRequest,
    NativeTeletextExportPolicy, QualifiedBaseline, Result, SavedEditorialTeletextDesign,
    StoredDesignParts, TeletextDesignHead, TeletextDesignMutation, TeletextDesignRevision,
    TeletextEditorCarrier, TeletextExportFile, TeletextExportReceiptBody, TeletextPageAddress,
    TeletextPageNumber, TeletextPhysicalBaseOverlay, TeletextSavedEditorialOverlay,
    TeletextSavedEditorialProvenance, TeletextSubcode,
};
use ersatzrs_core::locking::TeletextWorkHold;
use ersatzrs_domain::{
    TeletextColumnOperation, TeletextDrcsBitmapOverlay, TeletextLocalObjectOperandOverlay,
    TeletextLocalProgramOperandOverlay, TeletextPhysicalBaseRow, TeletextServiceBudget,
};
use std::collections::{BTreeMap, BTreeSet};

#[path = "repeat/object_operands.rs"]
mod object_operands;

struct PreparedImport {
    root: QualifiedBaseline,
    latest: Option<QualifiedBaseline>,
    incoming: TeletextEditorialChanges,
    cumulative: TeletextEditorialChanges,
    provenance: TeletextSavedEditorialProvenance,
    root_export: Option<TeletextExportReceiptBody>,
    qualified: Option<(EncodedStoredDesign, String)>,
    next_revision: i64,
    policy: NativeTeletextExportPolicy,
    files: Vec<TeletextExportFile>,
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
) -> Result<(QualifiedBaseline, TeletextEditorialChanges)> {
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
    let previous = TeletextEditorialChanges {
        rows: overlay.pages().to_vec(),
        operands: overlay.local_program_operands().to_vec(),
        object_operands: overlay.local_object_operands().to_vec(),
        bitmaps: overlay.drcs_bitmap_overlays().to_vec(),
    };
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

fn merge_operands(
    previous: &[TeletextLocalProgramOperandOverlay],
    incoming: &[TeletextLocalProgramOperandOverlay],
    service: TeletextServiceBudget,
    deadline: Instant,
) -> Result<Vec<TeletextLocalProgramOperandOverlay>> {
    let maximum = service.max_commands.min(service.max_structure_items);
    if previous.len() > maximum || incoming.len() > maximum {
        return Err(NativeTeletextEditorialImportError::Budget);
    }
    let mut merged = BTreeMap::new();
    for operand in previous {
        check_deadline(deadline)?;
        if merged
            .insert((operand.owner(), operand.command()), *operand)
            .is_some()
        {
            return Err(NativeTeletextEditorialImportError::Association);
        }
    }
    let mut seen = BTreeSet::new();
    for operand in incoming {
        check_deadline(deadline)?;
        let key = (operand.owner(), operand.command());
        if !seen.insert(key) {
            return Err(NativeTeletextEditorialImportError::Association);
        }
        if let Some(previous) = merged.get(&key).copied() {
            if previous.column() != operand.column() || previous.replacement() != operand.expected()
            {
                return Err(NativeTeletextEditorialImportError::Association);
            }
            if operand.replacement() == previous.expected() {
                // Revert removes root-relative intent. The incoming transition
                // remains separately checked against latest-export permissions.
                merged.remove(&key);
            } else {
                let cumulative = TeletextLocalProgramOperandOverlay::new(
                    operand.owner(),
                    operand.command(),
                    operand.column(),
                    previous.expected(),
                    operand.replacement(),
                )
                .map_err(|_| NativeTeletextEditorialImportError::Association)?;
                merged.insert(key, cumulative);
            }
        } else {
            if merged.len() >= maximum {
                return Err(NativeTeletextEditorialImportError::Budget);
            }
            // Reconciliation authenticated this against the latest service;
            // attachment independently matches it to the immutable root.
            merged.insert(key, *operand);
        }
    }
    Ok(merged.into_values().collect())
}

fn merge_bitmaps(
    previous: &[TeletextDrcsBitmapOverlay],
    incoming: &[TeletextDrcsBitmapOverlay],
    service: TeletextServiceBudget,
    deadline: Instant,
) -> Result<Vec<TeletextDrcsBitmapOverlay>> {
    let maximum = service.max_structure_items;
    if previous.len() > maximum || incoming.len() > maximum {
        return Err(NativeTeletextEditorialImportError::Budget);
    }
    let mut merged = BTreeMap::new();
    for bitmap in previous {
        check_deadline(deadline)?;
        if merged
            .insert((bitmap.identity(), bitmap.slot()), *bitmap)
            .is_some()
        {
            return Err(NativeTeletextEditorialImportError::Association);
        }
    }
    let mut seen = BTreeSet::new();
    for bitmap in incoming {
        check_deadline(deadline)?;
        let key = (bitmap.identity(), bitmap.slot());
        if !seen.insert(key) {
            return Err(NativeTeletextEditorialImportError::Association);
        }
        if let Some(previous) = merged.get(&key).copied() {
            if previous.replacement() != bitmap.expected() {
                return Err(NativeTeletextEditorialImportError::Association);
            }
            if bitmap.replacement() == previous.expected() {
                merged.remove(&key);
            } else {
                let cumulative = TeletextDrcsBitmapOverlay::new(
                    bitmap.identity(),
                    bitmap.slot(),
                    previous.expected(),
                    bitmap.replacement(),
                )
                .map_err(|_| NativeTeletextEditorialImportError::Association)?;
                merged.insert(key, cumulative);
            }
        } else {
            if merged.len() >= maximum {
                return Err(NativeTeletextEditorialImportError::Budget);
            }
            // Reconciliation authenticates against latest; compilation below
            // independently authenticates this same expectation against root.
            merged.insert(key, *bitmap);
        }
    }
    Ok(merged.into_values().collect())
}

fn bitmap_consumer_owners(
    service: &ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService,
    bitmaps: &[TeletextDrcsBitmapOverlay],
    deadline: Instant,
) -> Result<BTreeSet<TeletextPageAddress>> {
    use ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredPage;
    let targets: BTreeSet<_> = bitmaps
        .iter()
        .map(|edit| (edit.identity(), edit.slot()))
        .collect();
    let mut owners = BTreeSet::new();
    for page in service.pages() {
        check_deadline(deadline)?;
        let composition = match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page) => page.normalized().original(),
            EncodedTeletextAuthoredPage::NativeDisplay(page) => page.composition(),
            _ => continue,
        };
        // Candidate-aligned execution retains object/default invocations,
        // repeated references and losing contenders, not just visible winners.
        for reference in composition.drcs_execution().records() {
            check_deadline(deadline)?;
            if targets.contains(&(reference.resource(), reference.reference().slot)) {
                owners.insert(composition.owner());
                break;
            }
        }
    }
    Ok(owners)
}

fn extend_root_export(
    root: &mut QualifiedBaseline,
    cumulative: &TeletextEditorialChanges,
    expected_head: i64,
    policy: &NativeTeletextExportPolicy,
    deadline: Instant,
) -> Result<Option<TeletextExportReceiptBody>> {
    let (project, tti) = import_limits(policy);
    let resource_owners = if cumulative.bitmaps.is_empty() {
        BTreeSet::new()
    } else {
        bitmap_consumer_owners(
            root.original
                .view()
                .encoded()
                .ok_or(NativeTeletextEditorialImportError::Association)?,
            &cumulative.bitmaps,
            deadline,
        )?
    };
    let owners: Vec<_> = cumulative
        .rows
        .iter()
        .map(TeletextPhysicalBaseOverlay::owner)
        .chain(cumulative.operands.iter().map(|operand| operand.owner()))
        .chain(cumulative.object_operands.iter().map(|operand| operand.owner()))
        // Preserve every actual resource consumer without exporting unrelated
        // pages or widening the incoming receipt's editable surface.
        .chain(resource_owners)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let covered = owners.iter().all(|owner| {
        root.body.project.pages.iter().any(|mapping| {
            mapping.page.page == owner.page.get() && mapping.page.subcode == owner.subcode.get()
        })
    });
    let exported = if covered {
        None
    } else {
        let body = build_native_teletext_export_project(
            root.original.view(),
            expected_head,
            Some(&owners),
            project,
            tti,
            deadline,
            super::super::teletext_export_project::editor_presentation::carrier(
                root.original.view(),
                Some(&owners),
            ),
        )
        .map_err(|error| NativeTeletextEditorialImportError::Mapping(Box::new(error)))?;
        root.body = body.clone();
        Some(body)
    };
    Ok(exported)
}

fn check_operand_budget(
    changes: &TeletextEditorialChanges,
    budget: TeletextServiceBudget,
) -> Result<()> {
    if changes
        .operands
        .len()
        .checked_add(changes.object_operands.len())
        .is_none_or(|count| count > budget.max_commands || count > budget.max_structure_items)
    {
        return Err(NativeTeletextEditorialImportError::Budget);
    }
    Ok(())
}

fn prepare(
    request: NativeTeletextEditorialImportRequest,
    latest: EditorialBaseline,
    root: Option<EditorialBaseline>,
    policy: NativeTeletextExportPolicy,
    deadline: Instant,
) -> Result<Box<PreparedImport>> {
    let (project, tti) = import_limits(&policy);
    check_request(&request, project)?;
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
                TeletextEditorialChanges {
                    rows: Vec::new(),
                    operands: Vec::new(),
                    object_operands: Vec::new(),
                    bitmaps: Vec::new(),
                },
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
    for changes in [&previous, &incoming] {
        check_operand_budget(changes, policy.document.validation.service)?;
    }
    let cumulative = TeletextEditorialChanges {
        rows: merge_rows(&previous.rows, &incoming.rows, &policy, deadline)?,
        operands: merge_operands(
            &previous.operands,
            &incoming.operands,
            policy.document.validation.service,
            deadline,
        )?,
        object_operands: object_operands::merge(
            &previous.object_operands,
            &incoming.object_operands,
            policy.document.validation.service,
            deadline,
        )?,
        bitmaps: merge_bitmaps(
            &previous.bitmaps,
            &incoming.bitmaps,
            policy.document.validation.service,
            deadline,
        )?,
    };
    check_operand_budget(&cumulative, policy.document.validation.service)?;
    let root_export = extend_root_export(
        &mut root,
        &cumulative,
        request.expected_design_head,
        &policy,
        deadline,
    )?;
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
        files: request.files,
    });
    // Fully qualify content before a possible durable union-owner root receipt.
    prepared.qualified = Some(make_document(&prepared, deadline)?);
    Ok(prepared)
}

fn glyph_family_flags(changes: &TeletextEditorialChanges) -> (bool, bool, bool) {
    changes
        .operands
        .iter()
        .map(|operand| operand.expected())
        .chain(
            changes
                .object_operands
                .iter()
                .map(|operand| operand.expected()),
        )
        .fold(
            (false, false, false),
            |(drcs, remaining, extended), operation| {
                (
                    drcs || matches!(operation, TeletextColumnOperation::Drcs { .. }),
                    remaining
                        || matches!(
                            operation,
                            TeletextColumnOperation::G1Alpha { .. }
                                | TeletextColumnOperation::G3 { .. }
                        ),
                    extended
                        || matches!(
                            operation,
                            TeletextColumnOperation::G2 { .. }
                                | TeletextColumnOperation::G0Diacritic { .. }
                        ),
                )
            },
        )
}

fn cumulative_overlay(prepared: &PreparedImport) -> Result<TeletextSavedEditorialOverlay> {
    let has_glyphs = prepared.cumulative.operands.iter().any(|operand| {
        matches!(
            operand.expected(),
            TeletextColumnOperation::G0 { .. } | TeletextColumnOperation::G1 { .. }
        )
    });
    let has_object_attributes = prepared.cumulative.object_operands.iter().any(|operand| {
        matches!(
            operand.expected(),
            TeletextColumnOperation::Foreground { .. }
                | TeletextColumnOperation::Background { .. }
                | TeletextColumnOperation::Attributes { .. }
        )
    });
    let (has_drcs_references, has_remaining_glyphs, has_extended_glyphs) =
        glyph_family_flags(&prepared.cumulative);
    let overlay = if has_drcs_references {
        TeletextSavedEditorialOverlay::new_with_drcs_reference_slots(
            prepared.provenance.clone(),
            prepared.cumulative.rows.clone(),
            prepared.cumulative.operands.clone(),
            prepared.cumulative.object_operands.clone(),
            prepared.cumulative.bitmaps.clone(),
        )?
    } else if has_remaining_glyphs {
        TeletextSavedEditorialOverlay::new_with_remaining_glyph_operands(
            prepared.provenance.clone(),
            prepared.cumulative.rows.clone(),
            prepared.cumulative.operands.clone(),
            prepared.cumulative.object_operands.clone(),
            prepared.cumulative.bitmaps.clone(),
        )?
    } else if has_extended_glyphs {
        TeletextSavedEditorialOverlay::new_with_extended_glyph_operands(
            prepared.provenance.clone(),
            prepared.cumulative.rows.clone(),
            prepared.cumulative.operands.clone(),
            prepared.cumulative.object_operands.clone(),
            prepared.cumulative.bitmaps.clone(),
        )?
    } else if has_object_attributes {
        TeletextSavedEditorialOverlay::new_with_local_object_attributes(
            prepared.provenance.clone(),
            prepared.cumulative.rows.clone(),
            prepared.cumulative.operands.clone(),
            prepared.cumulative.object_operands.clone(),
            prepared.cumulative.bitmaps.clone(),
        )?
    } else if !prepared.cumulative.object_operands.is_empty() || has_glyphs {
        TeletextSavedEditorialOverlay::new_with_local_object_operands(
            prepared.provenance.clone(),
            prepared.cumulative.rows.clone(),
            prepared.cumulative.operands.clone(),
            prepared.cumulative.object_operands.clone(),
            prepared.cumulative.bitmaps.clone(),
        )?
    } else if !prepared.cumulative.bitmaps.is_empty() {
        TeletextSavedEditorialOverlay::new_with_drcs_bitmap_overlays(
            prepared.provenance.clone(),
            prepared.cumulative.rows.clone(),
            prepared.cumulative.operands.clone(),
            prepared.cumulative.bitmaps.clone(),
        )?
    } else if prepared.cumulative.operands.is_empty() {
        TeletextSavedEditorialOverlay::new(
            prepared.provenance.clone(),
            prepared.cumulative.rows.clone(),
        )?
    } else {
        TeletextSavedEditorialOverlay::new_with_local_program_operands(
            prepared.provenance.clone(),
            prepared.cumulative.rows.clone(),
            prepared.cumulative.operands.clone(),
        )?
    };
    Ok(overlay)
}

fn encode_cumulative_concept(prepared: &PreparedImport) -> Result<EncodedStoredDesign> {
    let document = if prepared.cumulative.rows.is_empty()
        && prepared.cumulative.operands.is_empty()
        && prepared.cumulative.object_operands.is_empty()
        && prepared.cumulative.bitmaps.is_empty()
    {
        // An explicit edit back to the immutable root is still a new concept
        // revision. Preserve the authenticated original bytes without inventing
        // a no-op operand or an empty overlay schema.
        let bytes = prepared.root.original_document.clone();
        match prepared.root.original.view().document_version() {
            ersatzrs_domain::TELETEXT_DESIGN_DOCUMENT_VERSION => {
                EncodedStoredDesign::Base(super::EncodedTeletextDesignDocuments::from_saved_bytes(
                    bytes,
                    prepared.policy.document,
                )?)
            }
            ersatzrs_domain::TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION => {
                EncodedStoredDesign::Effective(
                    super::EncodedEffectiveTeletextDesignDocuments::from_saved_bytes(
                        bytes,
                        prepared.policy.document,
                    )?,
                )
            }
            _ => return Err(NativeTeletextEditorialImportError::Association),
        }
    } else {
        let overlay = cumulative_overlay(prepared)?;
        append_overlay(
            prepared.root.original.view().document_version(),
            &prepared.root.original_document,
            &overlay,
            prepared.policy.document,
        )?
    };
    Ok(document)
}

fn make_document(
    prepared: &PreparedImport,
    deadline: Instant,
) -> Result<(EncodedStoredDesign, String)> {
    check_deadline(deadline)?;
    let document = encode_cumulative_concept(prepared)?;
    let document_digest = digest(document.bytes());
    let stored = TeletextDesignRevision {
        design_id: prepared.root.original.view().design_id(),
        revision: prepared.next_revision,
        document_version: document.version(),
        digest: document_digest.clone(),
        document: document.bytes().to_vec(),
    };
    let parts = StoredDesignParts::decode(&stored, prepared.policy.document)?;
    let edited = if parts.overlay().is_some() {
        qualify_rows(
            stored,
            parts,
            &prepared.root,
            prepared.policy.encoding.clone(),
            deadline,
        )?
    } else {
        parts
            .compile(
                stored,
                prepared.policy.encoding.clone(),
                deadline,
                Vec::new(),
            )
            .map_err(|error| NativeTeletextEditorialImportError::Design(Box::new(error)))?
    };
    let baseline = prepared.latest.as_ref().unwrap_or(&prepared.root);
    super::super::teletext_editorial_effects::validate_teletext_editorial_transition_with_operands(
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
        TeletextEditorialEdits {
            rows: &prepared.incoming.rows,
            operands: &prepared.incoming.operands,
            object_operands: &prepared.incoming.object_operands,
            bitmaps: &prepared.incoming.bitmaps,
        },
        TeletextEditorialEdits {
            rows: &prepared.cumulative.rows,
            operands: &prepared.cumulative.operands,
            object_operands: &prepared.cumulative.object_operands,
            bitmaps: &prepared.cumulative.bitmaps,
        },
        &prepared.policy.encoding.encoding,
        deadline,
    )
    .map_err(|error| NativeTeletextEditorialImportError::Effects(Box::new(error)))?;
    validate_reconciled_files(
        baseline,
        &prepared.files,
        import_limits(&prepared.policy).1,
        deadline,
        edited
            .view()
            .encoded()
            .ok_or(NativeTeletextEditorialImportError::Association)?,
        &prepared.incoming,
    )?;
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
        prepare(request, baseline, root, policy, deadline)
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
                return Ok(TeletextDesignMutation::Rejected(reason));
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

#[cfg(test)]
#[path = "repeat_tests.rs"]
mod tests;
