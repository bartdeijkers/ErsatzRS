//! Protected editorial effects on privately rebuilt, complete native services.
//!
//! The export project must come from the authoritative original receipt, with
//! producer ownership already checked by the native import adapter. Uploaded
//! sidecars never establish permissions. This stage neither compiles an extra
//! service nor authorizes concept storage or publication.

use std::time::Instant;

use ersatzrs_addon_contract::{TeletextAuthoringRegionAreaWire, TeletextAuthoringRegionKindWire};
use ersatzrs_app::teletext_export_project::TeletextExportProject;
use ersatzrs_domain::{
    TeletextBaseDisplayCellKind, TeletextDisplayAttribute, TeletextDisplayCell,
    TeletextDisplayFragment, TeletextPageAddress, TeletextPhysicalBaseOverlay,
};
use ersatzrs_ffmpeg::teletext::{
    project_encoded_teletext_display, project_teletext_comparison_glyph,
    EncodedTeletextAuthoredPage, EncodedTeletextAuthoredService, EncodedTeletextPage,
    TeletextAuthoredServiceEncodingBudget, TeletextEncodedDisplayRecord, TeletextMainComposition,
};

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

/// Latest-export permissions apply only to incoming owners; the final service
/// must independently contain exactly the complete cumulative replacement set.
pub(super) fn validate_teletext_editorial_transition(
    original: &EncodedTeletextAuthoredService,
    edited: &EncodedTeletextAuthoredService,
    project: &TeletextExportProject,
    incoming: &[TeletextPhysicalBaseOverlay],
    cumulative: &[TeletextPhysicalBaseOverlay],
    budget: &TeletextAuthoredServiceEncodingBudget,
    deadline: Instant,
) -> Result<()> {
    check_deadline(deadline)?;
    check_service(original, edited, cumulative, budget)?;
    for (before, after) in original.pages().iter().zip(edited.pages()) {
        check_deadline(deadline)?;
        let (Some((old_page, old)), Some((new_page, new))) = (display(before), display(after))
        else {
            if before.resource_header() != after.resource_header()
                || before.data_units() != after.data_units()
            {
                return Err(error(
                    None,
                    None,
                    TeletextEditorialEffectsFailure::ProtectedStructure,
                ));
            }
            continue;
        };
        let owner = old.owner();
        let Some(overlay) = incoming.iter().find(|value| value.owner() == owner) else {
            if before.data_units() != after.data_units() {
                return Err(error(
                    Some(owner),
                    None,
                    TeletextEditorialEffectsFailure::ProtectedStructure,
                ));
            }
            continue;
        };
        let allowed = permission_mask(project, owner, budget.max_pages)?;
        check_packets(old_page, new_page, &allowed, budget.max_units)?;
        check_overlay(old, new, overlay)?;
        check_resolved(new, budget, deadline)?;
        compare_protected(old, new, &allowed, budget, deadline)?;
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

fn check_service(
    original: &EncodedTeletextAuthoredService,
    edited: &EncodedTeletextAuthoredService,
    overlays: &[TeletextPhysicalBaseOverlay],
    budget: &TeletextAuthoredServiceEncodingBudget,
) -> Result<()> {
    if budget.max_pages == 0
        || budget.max_units == 0
        || original.pages().len() > budget.max_pages
        || edited.pages().len() > budget.max_pages
        || original.unit_count() > budget.max_units
        || edited.unit_count() > budget.max_units
        || overlays.len() > budget.max_pages
    {
        return Err(error(None, None, TeletextEditorialEffectsFailure::Budget));
    }
    let old = original.declared();
    let new = edited.declared();
    let before = old.semantic();
    let after = new.semantic();
    if original.inventory() != edited.inventory()
        || original.magazine_units() != edited.magazine_units()
        || before.base() != after.base()
        || before.page_enhancements() != after.page_enhancements()
        || before.magazine_presentations() != after.magazine_presentations()
        || before.objects() != after.objects()
        || before.object_companions() != after.object_companions()
        || before.drcs() != after.drcs()
        || old.mots() != new.mots()
        || old.selections() != new.selections()
        || old.empty_object_pages() != new.empty_object_pages()
        || old.resolutions() != new.resolutions()
        || old.resource_inventory() != new.resource_inventory()
        || old.defaults() != new.defaults()
        || original.pages().len() != edited.pages().len()
    {
        return Err(error(
            None,
            None,
            TeletextEditorialEffectsFailure::ProtectedStructure,
        ));
    }
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

type PermissionMask = [[bool; 40]; 25];
fn permission_mask(
    project: &TeletextExportProject,
    owner: TeletextPageAddress,
    max_pages: usize,
) -> Result<PermissionMask> {
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
    let mut result = [[false; 40]; 25];
    let Some(metadata) = metadata else {
        return Ok(result);
    };
    for region in &metadata.regions {
        if region.area != TeletextAuthoringRegionAreaWire::Main
            || region.kind == TeletextAuthoringRegionKindWire::Generated
        {
            continue;
        }
        let end_row = usize::from(region.row) + usize::from(region.height);
        let end_column = usize::from(region.column) + usize::from(region.width);
        if region.height == 0
            || region.width == 0
            || end_row > 25
            || end_column > 40
            || (region.row == 0 && region.column < 8)
        {
            return Err(error(
                Some(owner),
                None,
                TeletextEditorialEffectsFailure::Association,
            ));
        }
        for row in &mut result[usize::from(region.row)..end_row] {
            row[usize::from(region.column)..end_column].fill(true);
        }
    }
    Ok(result)
}

fn check_packets(
    old: &EncodedTeletextPage,
    new: &EncodedTeletextPage,
    allowed: &PermissionMask,
    max_units: usize,
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
        || before.records.len() != after.records.len()
    {
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
    let changed_cells = before
        .records
        .iter()
        .zip(&after.records)
        .any(|(old, new)| changed_page_cells(old, new));
    for (before, after) in before.records.iter().zip(&after.records) {
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
        if old != new && !allowed[usize::from(row)][usize::from(column)] {
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
            if !allowed[usize::from(row)][usize::from(column)] {
                compare_base(old, new, row, column)?;
            }
        }
        for column in 0_u8..width {
            let main_column = column
                .checked_sub(before.left_width())
                .filter(|&value| value < 40);
            if main_column.is_none_or(|main| !allowed[usize::from(row)][usize::from(main)]) {
                compare_cell(old, new, row, column)?;
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
) -> bool {
    old.value().is_some() && old.value() == new.value() && old.writer() == new.writer()
}
fn same_attributes(old: TeletextDisplayCell<'_>, new: TeletextDisplayCell<'_>) -> bool {
    let before = old.attributes();
    let after = new.attributes();
    same_attribute(before.foreground(), after.foreground())
        && same_attribute(before.background(), after.background())
        && same_attribute(before.flash(), after.flash())
        && same_attribute(before.double_height(), after.double_height())
        && same_attribute(before.double_width(), after.double_width())
        && same_attribute(before.boxed(), after.boxed())
        && same_attribute(before.conceal(), after.conceal())
        && same_attribute(before.invert(), after.invert())
        && same_attribute(
            before.underline_or_separated_mosaics(),
            after.underline_or_separated_mosaics(),
        )
        && same_attribute(before.spacing_separated(), after.spacing_separated())
}
fn compare_cell(
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
    if before.fragment() != after.fragment() || !same_attributes(before, after) {
        return Err(mismatch());
    }
    if origin_signature(old, before) != origin_signature(new, after) {
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
    source: Option<ersatzrs_domain::TeletextEnhancementSource>,
    root: Option<ersatzrs_domain::TeletextSpatialRootSource>,
    invocation: Option<ersatzrs_domain::TeletextSpatialInvocationId>,
}
fn origin_signature(
    composition: &TeletextMainComposition,
    cell: TeletextDisplayCell<'_>,
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
        source: candidate.source(),
        root: candidate.root_source(),
        invocation: candidate.invocation(),
    })
}
