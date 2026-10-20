//! Closed V20 retained-call and V21 call-structure intent with existing siblings.

use super::{
    invalid, local_object_glyph_programs, resource_object_additions, resource_object_operands,
    resource_object_programs, ProgramPolicy, StoredOverlay, TeletextDesignDocumentBudget,
    TeletextDesignDocumentError, TeletextProfileDocument, TeletextSavedEditorialOverlay,
    TeletextSavedEditorialProvenance, LOCAL_OBJECT_INVOCATION_OVERLAY_VERSION,
    LOCAL_OBJECT_INVOCATION_STRUCTURE_OVERLAY_VERSION, RESOURCE_OBJECT_POSITION_OVERLAY_VERSION,
};
use ersatzrs_domain::TeletextLocalObjectGlyphProgramOverlay;

pub(super) fn install(
    mut value: TeletextSavedEditorialOverlay,
    programs: Vec<TeletextLocalObjectGlyphProgramOverlay>,
) -> Result<TeletextSavedEditorialOverlay, TeletextDesignDocumentError> {
    if programs.is_empty()
        || programs.iter().any(|program| {
            !program.has_invocation_changes() || program.has_invocation_structure_changes()
        })
    {
        return Err(invalid());
    }
    value.local_object_glyph_program_overlays.extend(programs);
    local_object_glyph_programs::distinct(
        &value.local_object_glyph_program_overlays,
        &value.local_object_operands,
        ProgramPolicy::InvocationChanges,
    )?;
    value.schema_version = if !value.base_text_edits.is_empty() {
        super::base_text::version(&value)
    } else if value.main_local_invocation_program_overlays.is_empty() {
        LOCAL_OBJECT_INVOCATION_OVERLAY_VERSION
    } else {
        super::MAIN_LOCAL_INVOCATION_OVERLAY_VERSION
    };
    Ok(value)
}

pub(super) fn install_structure(
    mut value: TeletextSavedEditorialOverlay,
    programs: Vec<TeletextLocalObjectGlyphProgramOverlay>,
) -> Result<TeletextSavedEditorialOverlay, TeletextDesignDocumentError> {
    if programs.is_empty()
        || programs.iter().any(|program| {
            !program.has_invocation_structure_changes() && !program.has_invocation_changes()
        })
    {
        return Err(invalid());
    }
    value.local_object_glyph_program_overlays.extend(programs);
    local_object_glyph_programs::distinct(
        &value.local_object_glyph_program_overlays,
        &value.local_object_operands,
        ProgramPolicy::InvocationStructure,
    )?;
    value.schema_version = if !value.base_text_edits.is_empty() {
        super::base_text::version(&value)
    } else if value.main_local_invocation_program_overlays.is_empty() {
        LOCAL_OBJECT_INVOCATION_STRUCTURE_OVERLAY_VERSION
    } else {
        super::MAIN_LOCAL_INVOCATION_OVERLAY_VERSION
    };
    Ok(value)
}

pub(super) fn check_version(value: &StoredOverlay) -> Result<(), TeletextDesignDocumentError> {
    if value
        .local_object_glyph_program_overlays
        .0
        .as_ref()
        .is_none_or(Vec::is_empty)
        || value
            .local_program_operands
            .0
            .as_ref()
            .is_some_and(Vec::is_empty)
        || value
            .local_object_operands
            .0
            .as_ref()
            .is_some_and(Vec::is_empty)
        || value
            .drcs_bitmap_overlays
            .0
            .as_ref()
            .is_some_and(Vec::is_empty)
        || value
            .resource_object_operands
            .0
            .as_ref()
            .is_some_and(Vec::is_empty)
        || value
            .resource_object_program_overlays
            .0
            .as_ref()
            .is_some_and(Vec::is_empty)
        || value
            .resource_object_definition_additions
            .0
            .as_ref()
            .is_some_and(Vec::is_empty)
    {
        return Err(invalid());
    }
    Ok(())
}

pub(super) fn restore(
    stored: StoredOverlay,
    documents: &[TeletextProfileDocument],
    budget: TeletextDesignDocumentBudget,
) -> Result<TeletextSavedEditorialOverlay, TeletextDesignDocumentError> {
    restore_for_policy(
        stored,
        documents,
        budget,
        Some(ProgramPolicy::InvocationChanges),
    )
}

pub(super) fn restore_structure(
    stored: StoredOverlay,
    documents: &[TeletextProfileDocument],
    budget: TeletextDesignDocumentBudget,
) -> Result<TeletextSavedEditorialOverlay, TeletextDesignDocumentError> {
    restore_for_policy(
        stored,
        documents,
        budget,
        Some(ProgramPolicy::InvocationStructure),
    )
}

pub(super) fn restore_siblings(
    stored: StoredOverlay,
    documents: &[TeletextProfileDocument],
    budget: TeletextDesignDocumentBudget,
) -> Result<TeletextSavedEditorialOverlay, TeletextDesignDocumentError> {
    restore_for_policy(stored, documents, budget, None)
}

fn restore_for_policy(
    stored: StoredOverlay,
    documents: &[TeletextProfileDocument],
    budget: TeletextDesignDocumentBudget,
    policy: Option<ProgramPolicy>,
) -> Result<TeletextSavedEditorialOverlay, TeletextDesignDocumentError> {
    let programs = local_object_glyph_programs::restore(
        stored
            .local_object_glyph_program_overlays
            .0
            .unwrap_or_default(),
        policy.unwrap_or(ProgramPolicy::InvocationStructure),
    )?;
    let (calls, siblings) = programs.into_iter().partition(|program| {
        program.has_invocation_structure_changes() || program.has_invocation_changes()
    });
    let provenance = TeletextSavedEditorialProvenance {
        export_receipt_id: stored.export_receipt_id,
        export_receipt_digest: stored.export_receipt_digest,
        original_design_id: stored.original_design_id,
        original_revision: stored.original_revision,
        original_digest: stored.original_digest,
    };
    // The existing parent streaming preflight bounds every vector before DTO
    // allocation; both roles reuse existing fields and aggregate budgets.
    let pages = stored
        .pages
        .into_iter()
        .map(super::restore_page)
        .collect::<Result<Vec<_>, _>>()?;
    let operands = stored
        .local_program_operands
        .0
        .unwrap_or_default()
        .iter()
        .map(super::restore_operand)
        .collect::<Result<Vec<_>, _>>()?;
    let objects =
        super::object_operands::restore(stored.local_object_operands.0.unwrap_or_default())?;
    let bitmaps = stored
        .drcs_bitmap_overlays
        .0
        .unwrap_or_default()
        .iter()
        .map(super::restore_bitmap)
        .collect::<Result<Vec<_>, _>>()?;
    let mut value = TeletextSavedEditorialOverlay::new_resource_parts(
        provenance,
        pages,
        operands,
        objects,
        bitmaps,
        siblings,
        resource_object_operands::restore(stored.resource_object_operands.0.unwrap_or_default())?,
    )?;
    // These are the already accepted resource roles through V19. The explicit
    // selector preserves original-position intent; transient Add-position
    // authority still cannot be serialized as an unrelated resource program.
    value.resource_object_program_overlays = resource_object_programs::restore_programs(
        stored
            .resource_object_program_overlays
            .0
            .unwrap_or_default(),
        budget,
        RESOURCE_OBJECT_POSITION_OVERLAY_VERSION,
    )?;
    if value
        .resource_object_program_overlays
        .iter()
        .any(ersatzrs_domain::TeletextResourceObjectProgramOverlay::is_original_position_intent)
    {
        resource_object_programs::distinct_original_positions(
            &value.resource_object_program_overlays,
            &value.resource_object_operands,
        )?;
        value.schema_version = RESOURCE_OBJECT_POSITION_OVERLAY_VERSION;
    } else {
        resource_object_programs::distinct_targets(
            &value.resource_object_program_overlays,
            &value.resource_object_operands,
        )?;
    }
    if let Some(additions) = stored.resource_object_definition_additions.0 {
        value = resource_object_additions::restore(value, additions, budget)?;
    }
    let value = if policy.is_none() && calls.is_empty() {
        value
    } else if matches!(policy, Some(ProgramPolicy::InvocationStructure))
        || (policy.is_none()
            && calls
                .iter()
                .any(TeletextLocalObjectGlyphProgramOverlay::has_invocation_structure_changes))
    {
        install_structure(value, calls)?
    } else {
        install(value, calls)?
    };
    super::validate_association_shape(&value, documents, budget)?;
    Ok(value)
}
