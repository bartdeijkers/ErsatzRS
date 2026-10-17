//! Protected editorial effects on privately rebuilt, complete native services.
//!
//! The export project must come from the authoritative original receipt, with
//! producer ownership already checked by the native import adapter. Uploaded
//! sidecars never establish permissions. This stage neither compiles an extra
//! service nor authorizes concept storage or publication.

use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};

use ersatzrs_addon_contract::{TeletextAuthoringRegionAreaWire, TeletextAuthoringRegionKindWire};
use ersatzrs_app::teletext_export_project::TeletextExportProject;
use ersatzrs_domain::{
    TeletextBaseDisplayCellKind, TeletextDisplayAttribute, TeletextDisplayCell,
    TeletextDisplayFragment, TeletextDisplayGeometry, TeletextDrcsBitmapOverlay,
    TeletextDrcsCatalog, TeletextEnhancementCommand, TeletextLocalObjectGlyphProgramOverlay,
    TeletextLocalObjectOperandOverlay, TeletextLocalProgramOperandOverlay, TeletextPageAddress,
    TeletextPhysicalBaseOverlay, TeletextResolvedPageEnhancementDraft,
    TeletextResourceObjectOperandOverlay,
};
use ersatzrs_ffmpeg::teletext::{
    encode_teletext_drcs_page, project_encoded_teletext_display, project_teletext_comparison_glyph,
    EncodedTeletextAuthoredPage, EncodedTeletextAuthoredService, EncodedTeletextPage,
    TeletextAuthoredServiceEncodingBudget, TeletextEncodedDisplayRecord, TeletextMainComposition,
    TeletextResourceFullUpdate,
};

mod charset_context;
mod compiler_resources;
mod definition_additions;
mod lineage;
mod object_operands;
mod programs;
mod resource_objects;
#[cfg(test)]
mod tests;

/// Static located rejection; no page content, settings or package data escape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("teletext editorial effects for page {page:?}, position {position:?}: {reason:?}")]
pub struct TeletextEditorialEffectsError {
    /// Exact original authored page/subcode, when a page is identified.
    pub page: Option<TeletextPageAddress>,
    /// Visual coordinates include side panels; byte errors use Main coordinates.
    pub position: Option<(u8, u8)>,
    /// Static rejection classification suitable for localized host diagnostics.
    pub reason: TeletextEditorialEffectsFailure,
}

/// Rejection categories without retaining authored content or private settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextEditorialEffectsFailure {
    /// The original operation deadline expired.
    TimedOut,
    /// Existing native count/storage limits were exhausted.
    Budget,
    /// Inputs lack exact original/overlay correspondence.
    Association,
    /// Immutable service, resources, navigation or declarations changed.
    ProtectedStructure,
    /// A physical cell outside captured editable regions changed.
    ProtectedByte,
    /// Protected coded role, spacing attributes or provenance changed.
    ProtectedBase,
    /// Protected lower-row suppression or size footprints changed.
    ProtectedFootprint,
    /// Protected selected glyph, geometry or enhanced attributes changed.
    ProtectedDisplay,
    /// A semantic effect lacks settled decoding or glyph evidence.
    Unresolved,
}

type Result<T> = std::result::Result<T, TeletextEditorialEffectsError>;
fn error(
    page: Option<TeletextPageAddress>,
    position: Option<(u8, u8)>,
    reason: TeletextEditorialEffectsFailure,
) -> TeletextEditorialEffectsError {
    TeletextEditorialEffectsError {
        page,
        position,
        reason,
    }
}
fn check_deadline(deadline: Instant) -> Result<()> {
    if Instant::now() >= deadline {
        return Err(error(None, None, TeletextEditorialEffectsFailure::TimedOut));
    }
    Ok(())
}

/// Compare completed services derived from the same original documents plus
/// exactly the supplied overlay rows, using captured original permissions.
/// Existing encoder page/unit/cell bounds and the original deadline stay in
/// force. No screenshot-only, Unicode inversion or package mutation is used.
#[cfg(test)]
pub(super) fn validate_teletext_editorial_effects(
    original: &EncodedTeletextAuthoredService,
    edited: &EncodedTeletextAuthoredService,
    project: &TeletextExportProject,
    overlays: &[TeletextPhysicalBaseOverlay],
    budget: &TeletextAuthoredServiceEncodingBudget,
    deadline: Instant,
) -> Result<()> {
    validate_teletext_editorial_transition(
        original, edited, project, overlays, overlays, budget, deadline,
    )
}

/// Checked semantic changes and physical rows share the original receipt's
/// permissions. These borrowed intents never grant edit authority themselves.
#[derive(Clone, Copy)]
pub(super) struct TeletextEditorialEdits<'a> {
    pub rows: &'a [TeletextPhysicalBaseOverlay],
    pub operands: &'a [TeletextLocalProgramOperandOverlay],
    pub object_operands: &'a [TeletextLocalObjectOperandOverlay],
    pub resource_operands: &'a [TeletextResourceObjectOperandOverlay],
    pub resource_programs: &'a [ersatzrs_domain::TeletextResourceObjectProgramOverlay],
    pub resource_additions: &'a [ersatzrs_domain::TeletextResourceObjectDefinitionAddition],
    pub object_programs: &'a [TeletextLocalObjectGlyphProgramOverlay],
    pub bitmaps: &'a [TeletextDrcsBitmapOverlay],
}

pub(super) fn validate_teletext_editorial_effects_with_operands(
    original: &EncodedTeletextAuthoredService,
    edited: &EncodedTeletextAuthoredService,
    project: &TeletextExportProject,
    edits: TeletextEditorialEdits<'_>,
    budget: &TeletextAuthoredServiceEncodingBudget,
    deadline: Instant,
) -> Result<()> {
    validate_teletext_editorial_transition_with_operands(
        original, edited, project, edits, edits, budget, deadline,
    )
}

/// Latest-export permissions apply only to incoming owners; the final service
/// must independently contain exactly the complete cumulative replacement set.
#[cfg(test)]
pub(super) fn validate_teletext_editorial_transition(
    original: &EncodedTeletextAuthoredService,
    edited: &EncodedTeletextAuthoredService,
    project: &TeletextExportProject,
    incoming: &[TeletextPhysicalBaseOverlay],
    cumulative: &[TeletextPhysicalBaseOverlay],
    budget: &TeletextAuthoredServiceEncodingBudget,
    deadline: Instant,
) -> Result<()> {
    validate_teletext_editorial_transition_with_operands(
        original,
        edited,
        project,
        TeletextEditorialEdits {
            resource_additions: &[],
            rows: incoming,
            operands: &[],
            object_operands: &[],
            resource_operands: &[],
            resource_programs: &[],
            object_programs: &[],
            bitmaps: &[],
        },
        TeletextEditorialEdits {
            resource_additions: &[],
            rows: cumulative,
            operands: &[],
            object_operands: &[],
            resource_operands: &[],
            resource_programs: &[],
            object_programs: &[],
            bitmaps: &[],
        },
        budget,
        deadline,
    )
}

/// `original` is the actual incoming export baseline: the immutable root on
/// first import and the qualified latest concept on a repeated import. Thus
/// incoming operand expectations are matched to that baseline, while cumulative
/// rows and final operand replacements are checked against the completed result.
/// Root-to-final qualification separately authenticates cumulative provenance.
pub(super) fn validate_teletext_editorial_transition_with_operands(
    original: &EncodedTeletextAuthoredService,
    edited: &EncodedTeletextAuthoredService,
    project: &TeletextExportProject,
    incoming: TeletextEditorialEdits<'_>,
    cumulative: TeletextEditorialEdits<'_>,
    budget: &TeletextAuthoredServiceEncodingBudget,
    deadline: Instant,
) -> Result<()> {
    check_deadline(deadline)?;
    check_service(original, edited, incoming, cumulative, budget, deadline)?;
    let resource_context = if incoming.resource_operands.is_empty()
        && incoming.resource_programs.is_empty()
        && incoming.resource_additions.is_empty()
    {
        None
    } else {
        Some(resource_objects::Context::new(
            original,
            edited,
            incoming.resource_additions,
            budget,
            deadline,
        )?)
    };
    for (before, after) in
        compiler_resources::authored_pages(original).zip(compiler_resources::authored_pages(edited))
    {
        check_deadline(deadline)?;
        let (Some((old_page, old)), Some((new_page, new))) = (display(before), display(after))
        else {
            check_resource(
                before,
                after,
                edited,
                incoming,
                resource_context.as_ref(),
                deadline,
            )?;
            continue;
        };
        let owner = old.owner();
        let overlay = incoming.rows.iter().find(|value| value.owner() == owner);
        let operand_change = incoming.operands.iter().any(|value| value.owner() == owner)
            || incoming
                .object_operands
                .iter()
                .any(|value| value.owner() == owner);
        // A checked resource program can change compiler-owned repaint packing
        // for every consumer; uploaded words and protected cells remain checked.
        let structural_change = incoming
            .object_programs
            .iter()
            .any(|value| value.owner() == owner)
            || !incoming.resource_programs.is_empty()
            || !incoming.resource_additions.is_empty();
        let operand_change =
            operand_change || structural_change || !incoming.resource_operands.is_empty();
        let compiler_links = compiler_resources::links_changed(original, edited, owner)?;
        if overlay.is_none() && !operand_change && incoming.bitmaps.is_empty() && !compiler_links {
            if before.data_units() != after.data_units() {
                return Err(error(
                    Some(owner),
                    None,
                    TeletextEditorialEffectsFailure::ProtectedStructure,
                ));
            }
            continue;
        }
        let allowed = permission_mask(project, old.display_geometry(), budget.max_pages)?;
        check_packets(
            old_page,
            new_page,
            &allowed,
            operand_change,
            overlay.is_some() || structural_change,
            budget.max_units,
            compiler_links,
        )?;
        if let Some(overlay) = overlay {
            check_overlay(old, new, overlay)?;
        }
        check_resolved(new, budget, deadline)?;
        // Inspect every main and panel cell, including attributes extending to
        // the right and lower-row footprints. The command column is no permit.
        let lineage = lineage::ProtectedLineage::new(old, new, incoming, budget, deadline)?;
        compare_protected(old, new, &allowed, &lineage, budget, deadline)?;
    }
    check_deadline(deadline)
}

fn display(
    page: &EncodedTeletextAuthoredPage,
) -> Option<(&EncodedTeletextPage, &TeletextMainComposition)> {
    match page {
        EncodedTeletextAuthoredPage::NativeDisplay(value) => {
            Some((value.encoded().page(), value.composition()))
        }
        EncodedTeletextAuthoredPage::CompiledDisplay(value) => {
            Some((value.page(), value.normalized().original()))
        }
        EncodedTeletextAuthoredPage::Object(_) | EncodedTeletextAuthoredPage::Resource(_) => None,
    }
}

fn bounded_editorial_commands(edits: TeletextEditorialEdits<'_>, maximum: usize) -> bool {
    let mut remaining = maximum;
    let counts = [
        edits.operands.len(),
        edits.object_operands.len(),
        edits.resource_operands.len(),
    ]
    .into_iter()
    .chain(edits.object_programs.iter().flat_map(|program| {
        [
            program.expected().commands().len(),
            program.replacement().commands().len(),
            program.retained_commands().len(),
        ]
    }))
    .chain(edits.resource_programs.iter().flat_map(|program| {
        [
            program.expected().commands().len(),
            program.replacement().commands().len(),
            program.retained_commands().len(),
        ]
    }))
    .chain(
        edits
            .resource_additions
            .iter()
            .map(|addition| addition.definition().program.commands().len()),
    );
    for count in counts {
        let Some(next) = remaining.checked_sub(count) else {
            return false;
        };
        remaining = next;
    }
    true
}

fn check_service(
    original: &EncodedTeletextAuthoredService,
    edited: &EncodedTeletextAuthoredService,
    incoming: TeletextEditorialEdits<'_>,
    cumulative: TeletextEditorialEdits<'_>,
    budget: &TeletextAuthoredServiceEncodingBudget,
    deadline: Instant,
) -> Result<()> {
    let overlays = cumulative.rows;
    // Bitmaps replace existing definitions and add no commands. The checked
    // catalog has at most48 slots per bounded resource; cap the raw proposals
    // before indexing without spending the unrelated command quota.
    let maximum_bitmaps: usize = original
        .declared()
        .semantic()
        .drcs()
        .subtables()
        .iter()
        .map(|table| table.glyphs().len())
        .sum();
    if budget.max_pages == 0
        || budget.max_units == 0
        || original.pages().len() > budget.max_pages
        || edited.pages().len() > budget.max_pages
        || original.unit_count() > budget.max_units
        || edited.unit_count() > budget.max_units
        || overlays.len() > budget.max_pages
        || incoming.resource_additions.len() > budget.normalization.max_resource_objects
        || cumulative.resource_additions.len() > budget.normalization.max_resource_objects
        || !bounded_editorial_commands(incoming, budget.max_commands)
        || !bounded_editorial_commands(cumulative, budget.max_commands)
        || incoming.bitmaps.len() > maximum_bitmaps
        || cumulative.bitmaps.len() > maximum_bitmaps
    {
        return Err(error(None, None, TeletextEditorialEffectsFailure::Budget));
    }
    let old = original.declared();
    let new = edited.declared();
    let before = old.semantic();
    let after = new.semantic();
    compiler_resources::check(original, edited, incoming.rows, deadline)?;
    if original.magazine_units() != edited.magazine_units()
        || before.base() != after.base()
        || before.magazine_presentations() != after.magazine_presentations()
        || old.mots() != new.mots()
        || old.selections() != new.selections()
        || old.resolutions() != new.resolutions()
        || old.resource_inventory() != new.resource_inventory()
        || old.defaults() != new.defaults()
    {
        return Err(error(
            None,
            None,
            TeletextEditorialEffectsFailure::ProtectedStructure,
        ));
    }
    definition_additions::check_declarations(old, new, incoming.resource_additions, deadline)?;
    object_operands::check(
        before.objects(),
        after.objects(),
        incoming,
        cumulative,
        deadline,
    )?;
    check_bitmaps(
        before.drcs(),
        after.drcs(),
        incoming.bitmaps,
        cumulative.bitmaps,
        deadline,
    )?;
    check_enhancements(
        before.page_enhancements(),
        after.page_enhancements(),
        incoming.operands,
        cumulative.operands,
        deadline,
    )?;
    if after.physical_base_overlays().len() != overlays.len() {
        return Err(error(
            None,
            None,
            TeletextEditorialEffectsFailure::Association,
        ));
    }
    for (ordinal, overlay) in overlays.iter().enumerate() {
        if overlays[..ordinal]
            .iter()
            .any(|previous| previous.owner() == overlay.owner())
            || after.physical_base_overlay(overlay.owner()) != Some(overlay)
        {
            return Err(error(
                Some(overlay.owner()),
                None,
                TeletextEditorialEffectsFailure::Association,
            ));
        }
    }
    Ok(())
}

/// Match every table and glyph against exactly the incoming bitmap changes.
/// Cumulative intents describe the immutable root separately: their expected
/// bitmap need not match this latest-export baseline, but their final value must.
fn check_bitmaps(
    before: &TeletextDrcsCatalog,
    after: &TeletextDrcsCatalog,
    incoming: &[TeletextDrcsBitmapOverlay],
    cumulative: &[TeletextDrcsBitmapOverlay],
    deadline: Instant,
) -> Result<()> {
    let association = || error(None, None, TeletextEditorialEffectsFailure::Association);
    let structure = || {
        error(
            None,
            None,
            TeletextEditorialEffectsFailure::ProtectedStructure,
        )
    };
    let bitmap = |catalog: &TeletextDrcsCatalog, change: &TeletextDrcsBitmapOverlay| {
        catalog
            .subtables()
            .iter()
            .find(|table| table.identity() == change.identity())
            .and_then(|table| {
                table
                    .glyphs()
                    .iter()
                    .find(|glyph| glyph.slot == change.slot())
            })
            .map(|glyph| glyph.bitmap)
    };
    let mut changes = BTreeMap::new();
    for change in incoming {
        check_deadline(deadline)?;
        if changes
            .insert((change.identity(), change.slot()), change)
            .is_some()
            || bitmap(before, change) != Some(change.expected())
        {
            return Err(association());
        }
    }
    let mut final_changes = BTreeSet::new();
    for change in cumulative {
        check_deadline(deadline)?;
        if !final_changes.insert((change.identity(), change.slot()))
            || bitmap(after, change) != Some(change.replacement())
        {
            return Err(association());
        }
    }
    if before.subtables().len() != after.subtables().len() {
        return Err(structure());
    }
    for (old, new) in before.subtables().iter().zip(after.subtables()) {
        check_deadline(deadline)?;
        if old.identity() != new.identity() || old.glyphs().len() != new.glyphs().len() {
            return Err(structure());
        }
        for (old_glyph, new_glyph) in old.glyphs().iter().zip(new.glyphs()) {
            check_deadline(deadline)?;
            let expected = changes
                .get(&(old.identity(), old_glyph.slot))
                .map_or(old_glyph.bitmap, |change| change.replacement());
            if old_glyph.slot != new_glyph.slot || new_glyph.bitmap != expected {
                return Err(structure());
            }
        }
    }
    Ok(())
}

/// Resource packets may differ only from fully encoding the exact authorized
/// semantic bitmap intent. Header identity, lifecycle, mode and X28/3 remain
/// fixed; untouched resources retain every byte. This is a bounded one-resource
/// encoding, not a second complete-service compilation or permission source.
fn check_resource(
    before: &EncodedTeletextAuthoredPage,
    after: &EncodedTeletextAuthoredPage,
    edited: &EncodedTeletextAuthoredService,
    incoming: TeletextEditorialEdits<'_>,
    resource_context: Option<&resource_objects::Context<'_>>,
    deadline: Instant,
) -> Result<()> {
    check_deadline(deadline)?;
    let failure = || {
        error(
            None,
            None,
            TeletextEditorialEffectsFailure::ProtectedStructure,
        )
    };
    if std::mem::discriminant(before) != std::mem::discriminant(after) {
        return Err(failure());
    }
    let Some(header) = before.resource_header() else {
        return Err(failure());
    };
    if !incoming.resource_additions.is_empty()
        && matches!(before, EncodedTeletextAuthoredPage::Object(_))
    {
        return resource_context
            .ok_or_else(failure)?
            .check(before, after, deadline);
    }
    if incoming
        .resource_operands
        .iter()
        .any(|change| change.identity().resource() == Some(header.identity()))
        || incoming
            .resource_programs
            .iter()
            .any(|change| change.identity().resource() == Some(header.identity()))
        || incoming
            .resource_additions
            .iter()
            .any(|change| change.identity().resource() == Some(header.identity()))
    {
        return resource_context
            .ok_or_else(failure)?
            .check(before, after, deadline);
    }
    if before.resource_header() != after.resource_header() {
        return Err(failure());
    }
    if !incoming
        .bitmaps
        .iter()
        .any(|change| change.identity() == header.identity())
    {
        return if before.data_units() == after.data_units() {
            Ok(())
        } else {
            Err(failure())
        };
    }
    let (EncodedTeletextAuthoredPage::Resource(old), EncodedTeletextAuthoredPage::Resource(new)) =
        (before, after)
    else {
        return Err(failure());
    };
    if old.transmission_mode() != new.transmission_mode() {
        return Err(failure());
    }
    let table = edited
        .declared()
        .semantic()
        .drcs()
        .subtables()
        .iter()
        .find(|table| table.identity() == header.identity())
        .ok_or_else(failure)?;
    let update = if header.transmission().update_indicator() {
        TeletextResourceFullUpdate::Changed
    } else {
        TeletextResourceFullUpdate::Unchanged
    };
    let expected = encode_teletext_drcs_page(
        table,
        header.update_counter(),
        update,
        old.transmission_mode(),
    )
    .map_err(|_| failure())?;
    check_deadline(deadline)?;
    if expected.header() != header || expected.data_units() != after.data_units() {
        return Err(failure());
    }
    Ok(())
}

/// Match the exact typed transformation without allocating another service or
/// inventing a second domain validation budget. All untouched enhancement fields
/// and every unselected command must remain byte-for-byte semantic equals.
fn check_enhancements(
    before: &BTreeMap<TeletextPageAddress, TeletextResolvedPageEnhancementDraft>,
    after: &BTreeMap<TeletextPageAddress, TeletextResolvedPageEnhancementDraft>,
    incoming: &[TeletextLocalProgramOperandOverlay],
    cumulative: &[TeletextLocalProgramOperandOverlay],
    deadline: Instant,
) -> Result<()> {
    let mut changes = BTreeMap::new();
    for change in incoming {
        check_deadline(deadline)?;
        if changes
            .insert((change.owner(), change.command()), change)
            .is_some()
            || local_command(before, change)
                != Some(TeletextEnhancementCommand::AtColumn {
                    column: change.column(),
                    operation: change.expected(),
                })
        {
            return Err(error(
                Some(change.owner()),
                None,
                TeletextEditorialEffectsFailure::Association,
            ));
        }
    }
    let mut final_changes = BTreeSet::new();
    for change in cumulative {
        check_deadline(deadline)?;
        if !final_changes.insert((change.owner(), change.command()))
            || local_command(after, change)
                != Some(TeletextEnhancementCommand::AtColumn {
                    column: change.column(),
                    operation: change.replacement(),
                })
        {
            return Err(error(
                Some(change.owner()),
                None,
                TeletextEditorialEffectsFailure::Association,
            ));
        }
    }
    if before.len() != after.len() {
        return Err(error(
            None,
            None,
            TeletextEditorialEffectsFailure::ProtectedStructure,
        ));
    }
    for (owner, old) in before {
        check_deadline(deadline)?;
        let failure = || {
            error(
                Some(*owner),
                None,
                TeletextEditorialEffectsFailure::ProtectedStructure,
            )
        };
        let new = after.get(owner).ok_or_else(failure)?;
        if old.address != new.address
            || old.presentation != new.presentation
            || old.compositional_links != new.compositional_links
            || old.resources != new.resources
        {
            return Err(failure());
        }
        match (&old.local_program, &new.local_program) {
            (None, None) => {}
            (Some(old), Some(new)) if old.commands().len() == new.commands().len() => {
                for (ordinal, (old, new)) in old.commands().iter().zip(new.commands()).enumerate() {
                    check_deadline(deadline)?;
                    let expected = changes.get(&(*owner, ordinal)).map_or(*old, |change| {
                        TeletextEnhancementCommand::AtColumn {
                            column: change.column(),
                            operation: change.replacement(),
                        }
                    });
                    if *new != expected {
                        return Err(failure());
                    }
                }
            }
            _ => return Err(failure()),
        }
    }
    Ok(())
}

fn local_command(
    pages: &BTreeMap<TeletextPageAddress, TeletextResolvedPageEnhancementDraft>,
    change: &TeletextLocalProgramOperandOverlay,
) -> Option<TeletextEnhancementCommand> {
    pages
        .get(&change.owner())?
        .local_program
        .as_ref()?
        .commands()
        .get(change.command())
        .copied()
}

// Level 2.5 has forty Main columns and at most sixteen selected panel columns.
struct PermissionMask {
    main_start: usize,
    visual: [[bool; 56]; 25],
}
impl PermissionMask {
    fn allows_main(&self, row: u8, column: u8) -> bool {
        self.visual[usize::from(row)][self.main_start + usize::from(column)]
    }
}
fn permission_mask(
    project: &TeletextExportProject,
    geometry: &TeletextDisplayGeometry,
    max_pages: usize,
) -> Result<PermissionMask> {
    let owner = geometry.owner();
    if project.pages.len() > max_pages || project.producers.len() > max_pages {
        return Err(error(
            Some(owner),
            None,
            TeletextEditorialEffectsFailure::Budget,
        ));
    }
    let mapping = project.pages.iter().find(|mapping| {
        mapping.page.page == owner.page.get() && mapping.page.subcode == owner.subcode.get()
    });
    let metadata = mapping
        .and_then(|mapping| {
            project
                .producers
                .iter()
                .find(|producer| producer.ordinal == mapping.producer_ordinal)
        })
        .and_then(|producer| producer.authoring.as_ref())
        .and_then(|metadata| {
            metadata.pages.iter().find(|page| {
                page.page.page == owner.page.get() && page.page.subcode == owner.subcode.get()
            })
        });
    let left = usize::from(geometry.left_width());
    if usize::from(geometry.total_width()) > 56 {
        return Err(error(
            Some(owner),
            None,
            TeletextEditorialEffectsFailure::Association,
        ));
    }
    let mut result = PermissionMask {
        main_start: left,
        visual: [[false; 56]; 25],
    };
    let Some(metadata) = metadata else {
        return Ok(result);
    };
    for region in &metadata.regions {
        if region.kind == TeletextAuthoringRegionKindWire::Generated {
            continue;
        }
        let (offset, columns) = match region.area {
            TeletextAuthoringRegionAreaWire::Main => (left, 40),
            TeletextAuthoringRegionAreaWire::LeftPanel => (0, left),
            TeletextAuthoringRegionAreaWire::RightPanel => {
                (left + 40, usize::from(geometry.right_width()))
            }
        };
        let end_row = usize::from(region.row) + usize::from(region.height);
        let end_column = usize::from(region.column) + usize::from(region.width);
        if region.height == 0
            || region.width == 0
            || end_row > 25
            || end_column > columns
            || (region.area == TeletextAuthoringRegionAreaWire::Main
                && region.row == 0
                && region.column < 8)
        {
            return Err(error(
                Some(owner),
                None,
                TeletextEditorialEffectsFailure::Association,
            ));
        }
        for row in &mut result.visual[usize::from(region.row)..end_row] {
            row[offset + usize::from(region.column)..offset + end_column].fill(true);
        }
    }
    Ok(result)
}

fn check_packets(
    old: &EncodedTeletextPage,
    new: &EncodedTeletextPage,
    allowed: &PermissionMask,
    operand_change: bool,
    x26_repacking: bool,
    max_units: usize,
    compiler_links: bool,
) -> Result<()> {
    let owner = old.address();
    let before = project_encoded_teletext_display(old, max_units).map_err(|_| {
        error(
            Some(owner),
            None,
            TeletextEditorialEffectsFailure::Unresolved,
        )
    })?;
    let after = project_encoded_teletext_display(new, max_units).map_err(|_| {
        error(
            Some(owner),
            None,
            TeletextEditorialEffectsFailure::Unresolved,
        )
    })?;
    if before.address != after.address
        || before.kind != after.kind
        || before.language != after.language
        || before.header != after.header
    {
        return Err(error(
            Some(owner),
            None,
            TeletextEditorialEffectsFailure::ProtectedStructure,
        ));
    }
    // check_service/check_enhancements already authenticate the retained
    // authored commands and exactly the supplied semantic overlays. A Base edit
    // can change compiler-owned repaint recipes, selectors and X26 packing even
    // though uploaded X26 and every authored command remain unchanged. A checked
    // Local/resource program intent likewise permits native X26 repacking; uploaded
    // structure was authenticated separately. Compare all other completed packets
    // exactly; protected cells are checked below.
    // An uploaded X26 edit still needs the original TTI/operand reconciliation.
    // Generated X27/4 changes have separately matched exact private compiler
    // link descriptors; uploaded X27 stays immutable against its old receipt.
    let protected = |record: &&TeletextEncodedDisplayRecord| {
        protected_packet(record, x26_repacking, compiler_links)
    };
    let old_records = before.records.iter().filter(protected);
    let new_records = after.records.iter().filter(protected);
    if old_records.clone().count() != new_records.clone().count() {
        return Err(error(
            Some(owner),
            None,
            TeletextEditorialEffectsFailure::ProtectedStructure,
        ));
    }
    // Native X27/0's final two bytes are derived from actual page cells by
    // packets::editorial_links, rather than authored navigation. Both inputs
    // are private completed encoder outputs: retain all 38 link/control
    // nibbles and allow a rebuilt CRC only when header/base cells changed.
    let changed_cells = old_records
        .clone()
        .zip(new_records.clone())
        .any(|(old, new)| changed_page_cells(old, new));
    for (before, after) in old_records.zip(new_records) {
        match (before, after) {
            (
                TeletextEncodedDisplayRecord::BaseRow {
                    row: first,
                    cells: old,
                },
                TeletextEncodedDisplayRecord::BaseRow {
                    row: second,
                    cells: new,
                },
            ) if first == second => {
                check_row_bytes(owner, *first, 0, old, new, allowed)?;
            }
            (
                TeletextEncodedDisplayRecord::HeaderText(old),
                TeletextEncodedDisplayRecord::HeaderText(new),
            ) => {
                check_row_bytes(owner, 0, 8, old, new, allowed)?;
            }
            (
                TeletextEncodedDisplayRecord::EditorialLinks {
                    nibbles: old,
                    crc: old_crc,
                },
                TeletextEncodedDisplayRecord::EditorialLinks {
                    nibbles: new,
                    crc: new_crc,
                },
            ) if old == new && (old_crc == new_crc || changed_cells) => {}
            (
                TeletextEncodedDisplayRecord::Triplets {
                    row: 26,
                    designation: old,
                    ..
                },
                TeletextEncodedDisplayRecord::Triplets {
                    row: 26,
                    designation: new,
                    ..
                },
            ) if operand_change && old == new => {}
            _ if before == after => {}
            _ => {
                return Err(error(
                    Some(owner),
                    None,
                    TeletextEditorialEffectsFailure::ProtectedStructure,
                ))
            }
        }
    }
    Ok(())
}
fn protected_packet(
    record: &TeletextEncodedDisplayRecord,
    x26_repacking: bool,
    compiler_links: bool,
) -> bool {
    !((compiler_links
        && matches!(
            record,
            TeletextEncodedDisplayRecord::Triplets {
                row: 27,
                designation: 4,
                ..
            }
        ))
        || (x26_repacking
            && matches!(
                record,
                TeletextEncodedDisplayRecord::Triplets { row: 26, .. }
            )))
}

fn changed_page_cells(
    old: &TeletextEncodedDisplayRecord,
    new: &TeletextEncodedDisplayRecord,
) -> bool {
    match (old, new) {
        (
            TeletextEncodedDisplayRecord::HeaderText(old),
            TeletextEncodedDisplayRecord::HeaderText(new),
        ) => old != new,
        (
            TeletextEncodedDisplayRecord::BaseRow {
                row: first,
                cells: old,
            },
            TeletextEncodedDisplayRecord::BaseRow {
                row: second,
                cells: new,
            },
        ) if first == second => old != new,
        _ => false,
    }
}
fn check_row_bytes(
    owner: TeletextPageAddress,
    row: u8,
    start: u8,
    old: &[u8],
    new: &[u8],
    allowed: &PermissionMask,
) -> Result<()> {
    for ((old, new), column) in old.iter().zip(new).zip(start..) {
        if old != new && !allowed.allows_main(row, column) {
            return Err(error(
                Some(owner),
                Some((row, column)),
                TeletextEditorialEffectsFailure::ProtectedByte,
            ));
        }
    }
    Ok(())
}
fn check_overlay(
    old: &TeletextMainComposition,
    new: &TeletextMainComposition,
    overlay: &TeletextPhysicalBaseOverlay,
) -> Result<()> {
    for row in overlay.rows() {
        for column in 0_u8..40 {
            if new
                .base_display()
                .cell(row.row(), column)
                .is_none_or(|cell| cell.code() != row.cells()[usize::from(column)])
            {
                return Err(error(
                    Some(old.owner()),
                    Some((row.row(), column)),
                    TeletextEditorialEffectsFailure::Association,
                ));
            }
        }
    }
    Ok(())
}

fn compare_protected(
    old: &TeletextMainComposition,
    new: &TeletextMainComposition,
    allowed: &PermissionMask,
    lineage: &lineage::ProtectedLineage<'_>,
    budget: &TeletextAuthoredServiceEncodingBudget,
    deadline: Instant,
) -> Result<()> {
    let owner = old.owner();
    let before = old.display_geometry();
    let after = new.display_geometry();
    if before.panels() != after.panels()
        || before.total_width() != after.total_width()
        || old.display_resolution().width() != new.display_resolution().width()
    {
        return Err(error(
            Some(owner),
            None,
            TeletextEditorialEffectsFailure::ProtectedStructure,
        ));
    }
    let width = before.total_width();
    if usize::from(width) * 25 > budget.normalization.display_comparison.max_cells {
        return Err(error(
            Some(owner),
            None,
            TeletextEditorialEffectsFailure::Budget,
        ));
    }
    for row in 0_u8..25 {
        check_deadline(deadline)?;
        for column in 0_u8..40 {
            if !allowed.allows_main(row, column) {
                compare_base(old, new, row, column)?;
            }
        }
        for column in 0_u8..width {
            if !allowed.visual[usize::from(row)][usize::from(column)] {
                compare_cell(old, new, row, column, lineage, deadline)?;
            }
        }
    }
    Ok(())
}

fn check_resolved(
    composition: &TeletextMainComposition,
    budget: &TeletextAuthoredServiceEncodingBudget,
    deadline: Instant,
) -> Result<()> {
    let owner = composition.owner();
    let display = composition.display_resolution();
    if usize::from(display.width()) * 25 > budget.normalization.display_comparison.max_cells {
        return Err(error(
            Some(owner),
            None,
            TeletextEditorialEffectsFailure::Budget,
        ));
    }
    for row in 0_u8..25 {
        check_deadline(deadline)?;
        for column in 0_u8..display.width() {
            let unresolved = || {
                error(
                    Some(owner),
                    Some((row, column)),
                    TeletextEditorialEffectsFailure::Unresolved,
                )
            };
            let Some(cell) = display.cell(row, column) else {
                if row == 0 {
                    continue;
                }
                return Err(unresolved());
            };
            let attributes = cell.attributes();
            if cell.fragment() == TeletextDisplayFragment::Pending
                || attributes.foreground().value().is_none()
                || attributes.background().value().is_none()
                || attributes.flash().value().is_none()
                || attributes.double_height().value().is_none()
                || attributes.double_width().value().is_none()
                || attributes.boxed().value().is_none()
                || attributes.conceal().value().is_none()
                || attributes.invert().value().is_none()
                || attributes
                    .underline_or_separated_mosaics()
                    .value()
                    .is_none()
                || attributes.spacing_separated().value().is_none()
            {
                return Err(unresolved());
            }
            project_teletext_comparison_glyph(composition, cell).map_err(|_| unresolved())?;
        }
    }
    Ok(())
}
fn compare_base(
    old: &TeletextMainComposition,
    new: &TeletextMainComposition,
    row: u8,
    column: u8,
) -> Result<()> {
    let owner = old.owner();
    let mismatch = || {
        error(
            Some(owner),
            Some((row, column)),
            TeletextEditorialEffectsFailure::ProtectedBase,
        )
    };
    let before = old.base_display().cell(row, column).ok_or_else(mismatch)?;
    let after = new.base_display().cell(row, column).ok_or_else(mismatch)?;
    let equivalent_kind = before.kind() == after.kind()
        || (before.code() == b' '
            && after.code() == b' '
            && matches!(
                before.kind(),
                TeletextBaseDisplayCellKind::Glyph | TeletextBaseDisplayCellKind::Blank
            )
            && matches!(
                after.kind(),
                TeletextBaseDisplayCellKind::Glyph | TeletextBaseDisplayCellKind::Blank
            ));
    if before.code() != after.code()
        || !equivalent_kind
        || before.attributes() != after.attributes()
    {
        return Err(mismatch());
    }
    let before = old
        .base_footprints()
        .cell(row, column)
        .ok_or_else(mismatch)?;
    let after = new
        .base_footprints()
        .cell(row, column)
        .ok_or_else(mismatch)?;
    if before.pending().is_some() || after.pending().is_some() {
        return Err(error(
            Some(owner),
            Some((row, column)),
            TeletextEditorialEffectsFailure::Unresolved,
        ));
    }
    if before != after {
        return Err(error(
            Some(owner),
            Some((row, column)),
            TeletextEditorialEffectsFailure::ProtectedFootprint,
        ));
    }
    Ok(())
}

fn same_attribute<T: Copy + PartialEq>(
    old: TeletextDisplayAttribute<'_, T>,
    new: TeletextDisplayAttribute<'_, T>,
    lineage: &lineage::ProtectedLineage<'_>,
) -> bool {
    old.value().is_some()
        && old.value() == new.value()
        && match (old.writer(), new.writer()) {
            (Some(a), Some(b)) => {
                a.same_payload(b) && lineage.same_attribute_source(a.source(), b.source())
            }
            (None, None) => true,
            _ => false,
        }
}
fn same_attributes(
    old: TeletextDisplayCell<'_>,
    new: TeletextDisplayCell<'_>,
    lineage: &lineage::ProtectedLineage<'_>,
) -> bool {
    let before = old.attributes();
    let after = new.attributes();
    same_attribute(before.foreground(), after.foreground(), lineage)
        && same_attribute(before.background(), after.background(), lineage)
        && same_attribute(before.flash(), after.flash(), lineage)
        && same_attribute(before.double_height(), after.double_height(), lineage)
        && same_attribute(before.double_width(), after.double_width(), lineage)
        && same_attribute(before.boxed(), after.boxed(), lineage)
        && same_attribute(before.conceal(), after.conceal(), lineage)
        && same_attribute(before.invert(), after.invert(), lineage)
        && same_attribute(
            before.underline_or_separated_mosaics(),
            after.underline_or_separated_mosaics(),
            lineage,
        )
        && same_attribute(
            before.spacing_separated(),
            after.spacing_separated(),
            lineage,
        )
}
fn compare_cell(
    old: &TeletextMainComposition,
    new: &TeletextMainComposition,
    row: u8,
    column: u8,
    lineage: &lineage::ProtectedLineage<'_>,
    deadline: Instant,
) -> Result<()> {
    let owner = old.owner();
    let mismatch = || {
        error(
            Some(owner),
            Some((row, column)),
            TeletextEditorialEffectsFailure::ProtectedDisplay,
        )
    };
    let before = old.display_resolution().cell(row, column);
    let after = new.display_resolution().cell(row, column);
    let (Some(before), Some(after)) = (before, after) else {
        return if before.is_none() && after.is_none() && row == 0 {
            Ok(())
        } else {
            Err(mismatch())
        };
    };
    if before.fragment() == TeletextDisplayFragment::Pending
        || after.fragment() == TeletextDisplayFragment::Pending
    {
        return Err(error(
            Some(owner),
            Some((row, column)),
            TeletextEditorialEffectsFailure::Unresolved,
        ));
    }
    if before.fragment() != after.fragment() || !same_attributes(before, after, lineage) {
        return Err(mismatch());
    }
    if origin_signature(old, before, None) != origin_signature(new, after, Some(lineage)) {
        return Err(mismatch());
    }
    if !charset_context::same_cell(old, new, before, after, lineage, deadline)? {
        return Err(mismatch());
    }
    let unresolved = || {
        error(
            Some(owner),
            Some((row, column)),
            TeletextEditorialEffectsFailure::Unresolved,
        )
    };
    let before = project_teletext_comparison_glyph(old, before).map_err(|_| unresolved())?;
    let after = project_teletext_comparison_glyph(new, after).map_err(|_| unresolved())?;
    if before != after {
        return Err(mismatch());
    }
    Ok(())
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OriginSignature {
    visual: Option<(u8, u8)>,
    requested: Option<ersatzrs_domain::TeletextDisplaySize>,
    effective: Option<ersatzrs_domain::TeletextDisplaySize>,
    position: (u16, u16),
    priority: ersatzrs_domain::TeletextGlyphPriority,
    operation: Option<ersatzrs_domain::TeletextColumnOperation>,
    source: Option<lineage::CanonicalSource>,
    root: Option<ersatzrs_domain::TeletextSpatialRootSource>,
    invocation: Option<ersatzrs_domain::TeletextSpatialInvocationId>,
}
fn origin_signature(
    composition: &TeletextMainComposition,
    cell: TeletextDisplayCell<'_>,
    lineage: Option<&lineage::ProtectedLineage<'_>>,
) -> Option<OriginSignature> {
    let origin = composition
        .display_resolution()
        .origins()
        .get(cell.origin()?)?;
    let candidate = composition
        .glyph_layers()
        .candidates()
        .get(origin.candidate_index())?;
    Some(OriginSignature {
        visual: origin.visual_position(),
        requested: origin.requested_size(),
        effective: origin.effective_size(),
        position: candidate.position(),
        priority: candidate.priority(),
        operation: candidate.operation(),
        source: candidate.source().map(|source| {
            lineage.map_or(lineage::CanonicalSource::Existing(source), |lineage| {
                lineage.source_at_invocation(source, candidate.invocation())
            })
        }),
        root: candidate.root_source(),
        invocation: lineage.map_or(candidate.invocation(), |lineage| {
            lineage.canonical_invocation(candidate.invocation())
        }),
    })
}
