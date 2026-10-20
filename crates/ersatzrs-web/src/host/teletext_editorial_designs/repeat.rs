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

#[path = "repeat/base_text.rs"]
mod base_text;
#[path = "repeat/main_programs.rs"]
mod main_programs;
#[path = "repeat/object_operands.rs"]
mod object_operands;
#[path = "repeat/object_programs.rs"]
mod object_programs;
#[path = "repeat/resource_additions.rs"]
mod resource_additions;
#[path = "repeat/resource_banks.rs"]
mod resource_banks;
#[path = "repeat/resource_operands.rs"]
mod resource_operands;
#[path = "repeat/resource_programs.rs"]
mod resource_programs;

struct PreparedImport {
    root: QualifiedBaseline,
    latest: Option<QualifiedBaseline>,
    incoming: TeletextEditorialChanges,
    first_body_additions: Vec<ersatzrs_domain::TeletextResourceObjectDefinitionAddition>,
    original_position_history: Vec<ersatzrs_domain::TeletextResourceObjectProgramOverlay>,
    cumulative: TeletextEditorialChanges,
    provenance: TeletextSavedEditorialProvenance,
    root_export: Option<TeletextExportReceiptBody>,
    qualified: Option<(EncodedStoredDesign, String)>,
    next_revision: i64,
    policy: NativeTeletextExportPolicy,
    files: Vec<TeletextExportFile>,
}

pub(super) fn provenance(baseline: &EditorialBaseline) -> TeletextSavedEditorialProvenance {
    TeletextSavedEditorialProvenance {
        export_receipt_id: baseline.receipt.metadata.id,
        export_receipt_digest: baseline.receipt.metadata.digest.clone(),
        original_design_id: baseline.original.design_id,
        original_revision: baseline.original.revision,
        original_digest: baseline.original.digest.clone(),
    }
}

pub(super) async fn read_root(
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

pub(super) fn qualify_latest(
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
    let previous = from_overlay(overlay);

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

/// Borrowed getter restoration preserves the root-relative stored addition role.
pub(super) fn from_overlay(overlay: &TeletextSavedEditorialOverlay) -> TeletextEditorialChanges {
    TeletextEditorialChanges {
        object_resource_additions: overlay.object_resource_additions().to_vec(),
        object_subpage_announcements: overlay.object_subpage_announcements().to_vec(),
        base_text_edits: overlay.base_text_edits().to_vec(),
        resource_additions: overlay.resource_object_definition_additions().to_vec(),
        resource_operands: overlay.resource_object_operands().to_vec(),
        resource_programs: overlay.resource_object_program_overlays().to_vec(),
        rows: overlay.pages().to_vec(),
        operands: overlay.local_program_operands().to_vec(),
        object_operands: overlay.local_object_operands().to_vec(),
        object_programs: overlay.local_object_glyph_program_overlays().to_vec(),
        main_programs: overlay.main_local_invocation_program_overlays().to_vec(),
        bitmaps: overlay.drcs_bitmap_overlays().to_vec(),
    }
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
        .chain(cumulative.object_programs.iter().map(ersatzrs_domain::TeletextLocalObjectGlyphProgramOverlay::owner))
        .chain(cumulative.main_programs.iter().map(ersatzrs_domain::TeletextMainLocalInvocationProgramOverlay::owner))
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
                deadline,
            )
            .map_err(|error| NativeTeletextEditorialImportError::Mapping(Box::new(error)))?,
        )
        .map_err(|error| NativeTeletextEditorialImportError::Mapping(Box::new(error)))?;
        root.body = body.clone();
        Some(body)
    };
    Ok(exported)
}

pub(super) fn check_operand_budget(
    changes: &TeletextEditorialChanges,
    budget: TeletextServiceBudget,
) -> Result<()> {
    resource_banks::check_budget(changes, budget)?;
    let local_work = changes.object_programs.iter().map(|p| {
        [
            p.expected().commands().len(),
            p.replacement().commands().len(),
            p.retained_commands().len(),
        ]
    });
    let resource_work = changes.resource_programs.iter().map(|p| {
        [
            p.expected().commands().len(),
            p.replacement().commands().len(),
            p.retained_commands().len(),
        ]
    });
    let addition_work = changes
        .resource_additions
        .iter()
        .flat_map(|addition| [1, addition.definition().program.commands().len()]);
    let bank_work = changes
        .object_resource_additions
        .iter()
        .flat_map(|bank| bank.addition().definitions())
        .flat_map(|definition| [1, definition.program.commands().len()]);
    let program_work = local_work
        .chain(resource_work)
        .chain(changes.main_programs.iter().map(|p| {
            [
                p.expected().commands().len(),
                p.replacement().commands().len(),
                p.retained_commands().len(),
            ]
        }))
        .flatten()
        .chain(addition_work)
        .chain(bank_work)
        .chain(changes.object_subpage_announcements.iter().map(|_| 1))
        .try_fold(0usize, usize::checked_add);
    if program_work.is_none_or(|count| count > budget.max_structure_items)
        || changes
            .object_programs
            .len()
            .checked_add(changes.resource_programs.len())
            .and_then(|count| count.checked_add(changes.main_programs.len()))
            .and_then(|count| count.checked_add(changes.resource_additions.len()))
            .and_then(|count| count.checked_add(changes.object_resource_additions.len()))
            .and_then(|count| count.checked_add(changes.object_subpage_announcements.len()))
            .is_none_or(|n| n > budget.max_records)
        || changes.resource_additions.len() > budget.max_objects
    {
        return Err(NativeTeletextEditorialImportError::Budget);
    }
    if changes
        .operands
        .len()
        .checked_add(changes.object_operands.len())
        .and_then(|count| count.checked_add(changes.resource_operands.len()))
        .and_then(|count| {
            changes
                .resource_additions
                .iter()
                .map(ersatzrs_domain::TeletextResourceObjectDefinitionAddition::definition)
                .chain(
                    changes
                        .object_resource_additions
                        .iter()
                        .flat_map(|bank| bank.addition().definitions()),
                )
                .try_fold(count, |count, definition| {
                    count.checked_add(definition.program.commands().len())
                })
        })
        .is_none_or(|count| count > budget.max_commands || count > budget.max_structure_items)
    {
        return Err(NativeTeletextEditorialImportError::Budget);
    }
    Ok(())
}

fn merge_object_changes(
    root: &QualifiedBaseline,
    previous: &TeletextEditorialChanges,
    incoming: &TeletextEditorialChanges,
    budget: TeletextServiceBudget,
    deadline: Instant,
) -> Result<(
    Vec<TeletextLocalObjectOperandOverlay>,
    Vec<ersatzrs_domain::TeletextLocalObjectGlyphProgramOverlay>,
)> {
    Ok(
        if previous.object_programs.is_empty() && incoming.object_programs.is_empty() {
            (
                object_operands::merge(
                    &previous.object_operands,
                    &incoming.object_operands,
                    budget,
                    deadline,
                )?,
                Vec::new(),
            )
        } else {
            let catalog = root
                .original
                .view()
                .encoded()
                .ok_or(NativeTeletextEditorialImportError::Association)?
                .declared()
                .semantic()
                .objects();
            object_programs::merge(catalog, previous, incoming, budget, deadline)?
        },
    )
}

fn merge_resource_changes(
    root: &QualifiedBaseline,
    previous: &TeletextEditorialChanges,
    incoming: &TeletextEditorialChanges,
    budget: TeletextServiceBudget,
    deadline: Instant,
) -> Result<resource_banks::MergedResources> {
    let catalog = root
        .original
        .view()
        .encoded()
        .ok_or(NativeTeletextEditorialImportError::Association)?
        .declared()
        .semantic()
        .objects();
    if !previous.object_resource_additions.is_empty() {
        return resource_banks::merge(catalog, previous, incoming, budget, deadline);
    }
    let (operands, programs, additions) = if previous.resource_additions.is_empty() {
        let (operands, programs) =
            if previous.resource_programs.is_empty() && incoming.resource_programs.is_empty() {
                (
                    resource_operands::merge(
                        &previous.resource_operands,
                        &incoming.resource_operands,
                        budget,
                        deadline,
                    )?,
                    Vec::new(),
                )
            } else {
                resource_programs::merge(catalog, previous, incoming, budget, deadline)?
            };
        (operands, programs, Vec::new())
    } else {
        resource_additions::merge(catalog, previous, incoming, budget, deadline)?
    };
    Ok((operands, programs, additions, Vec::new()))
}

fn merge_changes(
    root: &QualifiedBaseline,
    previous: &TeletextEditorialChanges,
    incoming: &TeletextEditorialChanges,
    policy: &NativeTeletextExportPolicy,
    deadline: Instant,
) -> Result<TeletextEditorialChanges> {
    let (object_operands, object_programs) = merge_object_changes(
        root,
        previous,
        incoming,
        policy.document.validation.service,
        deadline,
    )?;
    let budget = policy.document.validation.service;
    if !incoming.resource_additions.is_empty()
        || !incoming.object_resource_additions.is_empty()
        || !incoming.object_subpage_announcements.is_empty()
    {
        return Err(NativeTeletextEditorialImportError::Association);
    }
    let (resource_operands, resource_programs, resource_additions, banks) =
        merge_resource_changes(root, previous, incoming, budget, deadline)?;
    let (operands, main_programs) =
        if previous.main_programs.is_empty() && incoming.main_programs.is_empty() {
            (
                merge_operands(&previous.operands, &incoming.operands, budget, deadline)?,
                Vec::new(),
            )
        } else {
            let pages = root
                .original
                .view()
                .encoded()
                .ok_or(NativeTeletextEditorialImportError::Association)?
                .declared()
                .semantic()
                .page_enhancements();
            main_programs::merge(pages, previous, incoming, budget, deadline)?
        };
    Ok(TeletextEditorialChanges {
        object_resource_additions: banks,
        object_subpage_announcements: previous.object_subpage_announcements.clone(),
        base_text_edits: base_text::merge(
            &previous.base_text_edits,
            &incoming.base_text_edits,
            budget,
            deadline,
        )?,
        resource_additions,
        resource_operands,
        resource_programs,
        rows: merge_rows(&previous.rows, &incoming.rows, policy, deadline)?,
        operands,
        main_programs,
        object_operands,
        object_programs,
        bitmaps: merge_bitmaps(
            &previous.bitmaps,
            &incoming.bitmaps,
            policy.document.validation.service,
            deadline,
        )?,
    })
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
                    object_resource_additions: Vec::new(),
                    object_subpage_announcements: Vec::new(),
                    base_text_edits: Vec::new(),
                    resource_additions: Vec::new(),
                    resource_operands: Vec::new(),
                    resource_programs: Vec::new(),
                    rows: Vec::new(),
                    operands: Vec::new(),
                    object_operands: Vec::new(),
                    object_programs: Vec::new(),
                    main_programs: Vec::new(),
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
    let incoming = reconcile_files(
        baseline,
        &request.files,
        tti,
        (
            policy.encoding.composition.service,
            super::super::teletext_editorial_tti::resource_objects::PositionScope {
                additions: &previous.resource_additions,
                programs: &previous.resource_programs,
                root: root.original.view().encoded(),
            },
        ),
        deadline,
    )?;
    for changes in [&previous, &incoming] {
        check_operand_budget(changes, policy.document.validation.service)?;
    }
    let cumulative = merge_changes(&root, &previous, &incoming, &policy, deadline)?;
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
        first_body_additions: previous.resource_additions,
        original_position_history: previous.resource_programs,
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

fn has_flash_charset(changes: &TeletextEditorialChanges) -> bool {
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
        .any(|operation| {
            matches!(
                operation,
                TeletextColumnOperation::Flash { .. }
                    | TeletextColumnOperation::DesignateCharset { .. }
            )
        })
}

fn has_object_attributes(changes: &TeletextEditorialChanges) -> bool {
    changes.object_operands.iter().any(|operand| {
        matches!(
            operand.expected(),
            TeletextColumnOperation::Foreground { .. }
                | TeletextColumnOperation::Background { .. }
                | TeletextColumnOperation::Attributes { .. }
        )
    })
}

fn program_overlay(
    provenance: &TeletextSavedEditorialProvenance,
    changes: &TeletextEditorialChanges,
) -> Result<TeletextSavedEditorialOverlay> {
    if changes
        .object_programs
        .iter()
        .any(|program| !program.is_all_glyphs_compatible())
    {
        TeletextSavedEditorialOverlay::new_with_local_object_style_programs(
            provenance.clone(),
            changes.rows.clone(),
            changes.operands.clone(),
            changes.object_operands.clone(),
            changes.bitmaps.clone(),
            changes.object_programs.clone(),
        )
        .map_err(Into::into)
    } else if changes
        .object_programs
        .iter()
        .any(|program| !program.is_legacy_compatible())
    {
        TeletextSavedEditorialOverlay::new_with_all_local_object_glyph_programs(
            provenance.clone(),
            changes.rows.clone(),
            changes.operands.clone(),
            changes.object_operands.clone(),
            changes.bitmaps.clone(),
            changes.object_programs.clone(),
        )
        .map_err(Into::into)
    } else {
        TeletextSavedEditorialOverlay::new_with_local_object_glyph_programs(
            provenance.clone(),
            changes.rows.clone(),
            changes.operands.clone(),
            changes.object_operands.clone(),
            changes.bitmaps.clone(),
            changes.object_programs.clone(),
        )
        .map_err(Into::into)
    }
}

fn resource_program_overlay(
    provenance: &TeletextSavedEditorialProvenance,
    changes: &TeletextEditorialChanges,
) -> Result<TeletextSavedEditorialOverlay> {
    let constructor = if changes
        .resource_programs
        .iter()
        .any(ersatzrs_domain::TeletextResourceObjectProgramOverlay::is_original_position_intent)
    {
        TeletextSavedEditorialOverlay::new_with_resource_object_position_overlays
    } else if changes.resource_programs.iter().any(
        ersatzrs_domain::TeletextResourceObjectProgramOverlay::has_invocation_structure_changes,
    ) {
        TeletextSavedEditorialOverlay::new_with_resource_object_invocation_structure_overlays
    } else if changes
        .resource_programs
        .iter()
        .any(ersatzrs_domain::TeletextResourceObjectProgramOverlay::has_invocation_changes)
    {
        TeletextSavedEditorialOverlay::new_with_resource_object_invocation_overlays
    } else {
        TeletextSavedEditorialOverlay::new_with_resource_object_program_overlays
    };
    constructor(
        provenance.clone(),
        changes.rows.clone(),
        changes.operands.clone(),
        changes.object_operands.clone(),
        changes.bitmaps.clone(),
        changes.object_programs.clone(),
        (
            changes.resource_operands.clone(),
            changes.resource_programs.clone(),
        ),
    )
    .map_err(Into::into)
}

pub(super) fn cumulative_overlay(
    provenance: &TeletextSavedEditorialProvenance,
    changes: &TeletextEditorialChanges,
) -> Result<TeletextSavedEditorialOverlay> {
    let has_prior = !changes.object_resource_additions.is_empty()
        || !changes.base_text_edits.is_empty()
        || !changes.rows.is_empty()
        || !changes.operands.is_empty()
        || !changes.object_operands.is_empty()
        || !changes.resource_operands.is_empty()
        || !changes.resource_programs.is_empty()
        || !changes.resource_additions.is_empty()
        || !changes.object_programs.is_empty()
        || !changes.main_programs.is_empty()
        || !changes.bitmaps.is_empty();
    if !changes.object_subpage_announcements.is_empty() {
        return if has_prior {
            bank_overlay(provenance, changes)?
                .with_object_subpage_announcements(changes.object_subpage_announcements.clone())
                .map_err(Into::into)
        } else {
            TeletextSavedEditorialOverlay::new_with_object_subpage_announcements(
                provenance.clone(),
                changes.object_subpage_announcements.clone(),
            )
            .map_err(Into::into)
        };
    }
    bank_overlay(provenance, changes)
}
fn bank_overlay(
    provenance: &TeletextSavedEditorialProvenance,
    changes: &TeletextEditorialChanges,
) -> Result<TeletextSavedEditorialOverlay> {
    if changes.object_resource_additions.is_empty() {
        return prior_overlay(provenance, changes);
    }
    let has_prior = !changes.base_text_edits.is_empty()
        || !changes.rows.is_empty()
        || !changes.operands.is_empty()
        || !changes.object_operands.is_empty()
        || !changes.resource_operands.is_empty()
        || !changes.resource_programs.is_empty()
        || !changes.resource_additions.is_empty()
        || !changes.object_programs.is_empty()
        || !changes.main_programs.is_empty()
        || !changes.bitmaps.is_empty();
    if has_prior {
        prior_overlay(provenance, changes)?
            .with_object_resource_additions(changes.object_resource_additions.clone())
            .map_err(Into::into)
    } else {
        TeletextSavedEditorialOverlay::new_with_object_resource_additions(
            provenance.clone(),
            changes.object_resource_additions.clone(),
        )
        .map_err(Into::into)
    }
}

fn prior_overlay(
    provenance: &TeletextSavedEditorialProvenance,
    changes: &TeletextEditorialChanges,
) -> Result<TeletextSavedEditorialOverlay> {
    if changes.base_text_edits.is_empty() {
        return cumulative_overlay_siblings(provenance, changes);
    }
    if changes.rows.is_empty()
        && changes.operands.is_empty()
        && changes.object_operands.is_empty()
        && changes.object_programs.is_empty()
        && changes.main_programs.is_empty()
        && changes.resource_operands.is_empty()
        && changes.resource_programs.is_empty()
        && changes.resource_additions.is_empty()
        && changes.bitmaps.is_empty()
    {
        return TeletextSavedEditorialOverlay::new_with_base_text_edits(
            provenance.clone(),
            changes.base_text_edits.clone(),
        )
        .map_err(Into::into);
    }
    cumulative_overlay_siblings(provenance, changes)?
        .with_base_text_edits(changes.base_text_edits.clone())
        .map_err(Into::into)
}

fn cumulative_overlay_siblings(
    provenance: &TeletextSavedEditorialProvenance,
    changes: &TeletextEditorialChanges,
) -> Result<TeletextSavedEditorialOverlay> {
    if !changes.main_programs.is_empty() {
        return main_programs::overlay(provenance, changes);
    }
    if changes.rows.is_empty()
        && changes.operands.is_empty()
        && changes.object_operands.is_empty()
        && changes.resource_operands.is_empty()
        && changes.resource_programs.is_empty()
        && changes.object_programs.is_empty()
        && changes.bitmaps.is_empty()
        && !changes.resource_additions.is_empty()
    {
        return TeletextSavedEditorialOverlay::new_with_resource_object_definition_additions(
            provenance.clone(),
            changes.resource_additions.clone(),
        )
        .map_err(Into::into);
    }
    let structure = changes.object_programs.iter().any(
        ersatzrs_domain::TeletextLocalObjectGlyphProgramOverlay::has_invocation_structure_changes,
    );
    let overlay = if structure
        || changes
            .object_programs
            .iter()
            .any(ersatzrs_domain::TeletextLocalObjectGlyphProgramOverlay::has_invocation_changes)
    {
        local_invocation_overlay(provenance, changes, structure)?
    } else {
        body_overlay(provenance, changes)?
    };
    if changes.resource_additions.is_empty() {
        Ok(overlay)
    } else {
        overlay
            .with_resource_object_definition_additions(changes.resource_additions.clone())
            .map_err(Into::into)
    }
}

fn body_overlay(
    provenance: &TeletextSavedEditorialProvenance,
    changes: &TeletextEditorialChanges,
) -> Result<TeletextSavedEditorialOverlay> {
    let has_glyphs = changes.operands.iter().any(|operand| {
        matches!(
            operand.expected(),
            TeletextColumnOperation::G0 { .. } | TeletextColumnOperation::G1 { .. }
        )
    });
    let (has_drcs_references, has_remaining_glyphs, has_extended_glyphs) =
        glyph_family_flags(changes);
    let overlay = if !changes.resource_programs.is_empty() {
        resource_program_overlay(provenance, changes)?
    } else if !changes.resource_operands.is_empty() {
        TeletextSavedEditorialOverlay::new_with_resource_object_operands(
            provenance.clone(),
            changes.rows.clone(),
            changes.operands.clone(),
            changes.object_operands.clone(),
            changes.bitmaps.clone(),
            changes.object_programs.clone(),
            changes.resource_operands.clone(),
        )?
    } else if !changes.object_programs.is_empty() {
        program_overlay(provenance, changes)?
    } else if changes.has_drcs_selection(None) {
        TeletextSavedEditorialOverlay::new_with_drcs_reference_selections(
            provenance.clone(),
            changes.rows.clone(),
            changes.operands.clone(),
            changes.object_operands.clone(),
            changes.bitmaps.clone(),
        )?
    } else if has_flash_charset(changes) {
        TeletextSavedEditorialOverlay::new_with_flash_charset_operands(
            provenance.clone(),
            changes.rows.clone(),
            changes.operands.clone(),
            changes.object_operands.clone(),
            changes.bitmaps.clone(),
        )?
    } else if has_drcs_references {
        TeletextSavedEditorialOverlay::new_with_drcs_reference_slots(
            provenance.clone(),
            changes.rows.clone(),
            changes.operands.clone(),
            changes.object_operands.clone(),
            changes.bitmaps.clone(),
        )?
    } else if has_remaining_glyphs {
        TeletextSavedEditorialOverlay::new_with_remaining_glyph_operands(
            provenance.clone(),
            changes.rows.clone(),
            changes.operands.clone(),
            changes.object_operands.clone(),
            changes.bitmaps.clone(),
        )?
    } else if has_extended_glyphs {
        TeletextSavedEditorialOverlay::new_with_extended_glyph_operands(
            provenance.clone(),
            changes.rows.clone(),
            changes.operands.clone(),
            changes.object_operands.clone(),
            changes.bitmaps.clone(),
        )?
    } else if has_object_attributes(changes) {
        TeletextSavedEditorialOverlay::new_with_local_object_attributes(
            provenance.clone(),
            changes.rows.clone(),
            changes.operands.clone(),
            changes.object_operands.clone(),
            changes.bitmaps.clone(),
        )?
    } else if !changes.object_operands.is_empty() || has_glyphs {
        TeletextSavedEditorialOverlay::new_with_local_object_operands(
            provenance.clone(),
            changes.rows.clone(),
            changes.operands.clone(),
            changes.object_operands.clone(),
            changes.bitmaps.clone(),
        )?
    } else if !changes.bitmaps.is_empty() {
        TeletextSavedEditorialOverlay::new_with_drcs_bitmap_overlays(
            provenance.clone(),
            changes.rows.clone(),
            changes.operands.clone(),
            changes.bitmaps.clone(),
        )?
    } else if changes.operands.is_empty() {
        TeletextSavedEditorialOverlay::new(provenance.clone(), changes.rows.clone())?
    } else {
        TeletextSavedEditorialOverlay::new_with_local_program_operands(
            provenance.clone(),
            changes.rows.clone(),
            changes.operands.clone(),
        )?
    };
    Ok(overlay)
}

fn local_invocation_overlay(
    provenance: &TeletextSavedEditorialProvenance,
    changes: &TeletextEditorialChanges,
    structure: bool,
) -> Result<TeletextSavedEditorialOverlay> {
    let (calls, ordinary): (Vec<_>, Vec<_>) =
        changes
            .object_programs
            .iter()
            .cloned()
            .partition(|program| {
                program.has_invocation_changes() || program.has_invocation_structure_changes()
            });
    if ordinary.is_empty()
        && changes.rows.is_empty()
        && changes.operands.is_empty()
        && changes.object_operands.is_empty()
        && changes.bitmaps.is_empty()
        && changes.resource_operands.is_empty()
        && changes.resource_programs.is_empty()
    {
        let constructor = if structure {
            TeletextSavedEditorialOverlay::new_with_local_object_invocation_structure_programs
        } else {
            TeletextSavedEditorialOverlay::new_with_local_object_invocation_programs
        };
        return constructor(
            provenance.clone(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            calls,
        )
        .map_err(Into::into);
    }
    // First qualify siblings through their unchanged stored-role constructors.
    // Resource additions are attached once by cumulative_overlay after this
    // explicit installation, which must preserve the newer role.
    let siblings = TeletextEditorialChanges {
        object_resource_additions: Vec::new(),
        object_subpage_announcements: Vec::new(),
        base_text_edits: Vec::new(),
        rows: changes.rows.clone(),
        operands: changes.operands.clone(),
        object_operands: changes.object_operands.clone(),
        bitmaps: changes.bitmaps.clone(),
        object_programs: ordinary,
        main_programs: Vec::new(),
        resource_operands: changes.resource_operands.clone(),
        resource_programs: changes.resource_programs.clone(),
        resource_additions: Vec::new(),
    };
    let overlay = body_overlay(provenance, &siblings)?;
    if structure {
        overlay.with_local_object_invocation_structure_programs(calls)
    } else {
        overlay.with_local_object_invocation_programs(calls)
    }
    .map_err(Into::into)
}

fn encode_cumulative_concept(prepared: &PreparedImport) -> Result<EncodedStoredDesign> {
    let document = if prepared.cumulative.object_resource_additions.is_empty()
        && prepared.cumulative.object_subpage_announcements.is_empty()
        && prepared.cumulative.base_text_edits.is_empty()
        && prepared.cumulative.rows.is_empty()
        && prepared.cumulative.operands.is_empty()
        && prepared.cumulative.object_operands.is_empty()
        && prepared.cumulative.resource_operands.is_empty()
        && prepared.cumulative.resource_programs.is_empty()
        && prepared.cumulative.resource_additions.is_empty()
        && prepared.cumulative.object_programs.is_empty()
        && prepared.cumulative.main_programs.is_empty()
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
        let overlay = cumulative_overlay(&prepared.provenance, &prepared.cumulative)?;
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
            object_resource_additions: &prepared.incoming.object_resource_additions,
            object_subpage_announcements: &prepared.incoming.object_subpage_announcements,
            base_text_edits: &prepared.incoming.base_text_edits,
            resource_additions: &prepared.incoming.resource_additions,
            resource_operands: &prepared.incoming.resource_operands,
            resource_programs: &prepared.incoming.resource_programs,
            rows: &prepared.incoming.rows,
            operands: &prepared.incoming.operands,
            object_operands: &prepared.incoming.object_operands,
            object_programs: &prepared.incoming.object_programs,
            main_programs: &prepared.incoming.main_programs,
            bitmaps: &prepared.incoming.bitmaps,
        },
        TeletextEditorialEdits {
            object_resource_additions: &prepared.cumulative.object_resource_additions,
            object_subpage_announcements: &prepared.cumulative.object_subpage_announcements,
            base_text_edits: &prepared.cumulative.base_text_edits,
            resource_additions: &prepared.cumulative.resource_additions,
            resource_operands: &prepared.cumulative.resource_operands,
            resource_programs: &prepared.cumulative.resource_programs,
            rows: &prepared.cumulative.rows,
            operands: &prepared.cumulative.operands,
            object_operands: &prepared.cumulative.object_operands,
            object_programs: &prepared.cumulative.object_programs,
            main_programs: &prepared.cumulative.main_programs,
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
        (
            prepared.policy.encoding.composition.service,
            super::super::teletext_editorial_tti::resource_objects::PositionScope {
                additions: &prepared.first_body_additions,
                programs: &prepared.original_position_history,
                root: prepared.root.original.view().encoded(),
            },
        ),
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
