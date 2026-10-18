//! Closed V20 Local retained-call intent with unchanged sibling storage roles.

use super::{
    invalid, local_object_glyph_programs, resource_object_additions, resource_object_operands,
    resource_object_programs, ProgramPolicy, StoredOverlay, TeletextDesignDocumentBudget,
    TeletextDesignDocumentError, TeletextProfileDocument, TeletextSavedEditorialOverlay,
    TeletextSavedEditorialProvenance, LOCAL_OBJECT_INVOCATION_OVERLAY_VERSION,
    RESOURCE_OBJECT_POSITION_OVERLAY_VERSION,
};
use ersatzrs_domain::TeletextLocalObjectGlyphProgramOverlay;

pub(super) fn install(
    mut value: TeletextSavedEditorialOverlay,
    programs: Vec<TeletextLocalObjectGlyphProgramOverlay>,
) -> Result<TeletextSavedEditorialOverlay, TeletextDesignDocumentError> {
    if programs.is_empty()
        || programs
            .iter()
            .any(|program| !program.has_invocation_changes())
    {
        return Err(invalid());
    }
    value.local_object_glyph_program_overlays.extend(programs);
    local_object_glyph_programs::distinct(
        &value.local_object_glyph_program_overlays,
        &value.local_object_operands,
        ProgramPolicy::InvocationChanges,
    )?;
    value.schema_version = LOCAL_OBJECT_INVOCATION_OVERLAY_VERSION;
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
    let programs = local_object_glyph_programs::restore(
        stored
            .local_object_glyph_program_overlays
            .0
            .unwrap_or_default(),
        ProgramPolicy::InvocationChanges,
    )?;
    let (calls, siblings) = programs
        .into_iter()
        .partition(TeletextLocalObjectGlyphProgramOverlay::has_invocation_changes);
    let provenance = TeletextSavedEditorialProvenance {
        export_receipt_id: stored.export_receipt_id,
        export_receipt_digest: stored.export_receipt_digest,
        original_design_id: stored.original_design_id,
        original_revision: stored.original_revision,
        original_digest: stored.original_digest,
    };
    // The existing parent streaming preflight bounds every vector before DTO
    // allocation; V20 reuses its fields rather than opening an unbounded payload.
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
    let value = install(value, calls)?;
    super::validate_association(&value, documents, budget)?;
    Ok(value)
}
