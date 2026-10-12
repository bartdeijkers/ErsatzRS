//! Bounded selected-model comparison, not authored lineage or wire equivalence.

use ersatzrs_domain::{
    TeletextColourCompositionError, TeletextColourPixelOptions, TeletextColourReference,
    TeletextDisplayCell, TeletextDisplayFragment, TeletextDisplayPending,
    TeletextDisplayResolution, TeletextEnhancementSource, TeletextFlashPhase,
    TeletextGlyphDisposition, TeletextPageAddress, TeletextSpatialInvocationId,
    TeletextSpatialRootSource,
};

use super::main_composition::{TeletextMainComposition, TeletextPhysicalDisplayReplay};
use super::raster::{
    comparison_original_glyph, comparison_physical_glyph, fragment_position, ComparisonGlyph,
    TeletextRasterCellFailure,
};

/// Trusted aggregate record, cell, diagnostic and actual operation limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextPhysicalComparisonBudget {
    pub max_records: usize,
    pub max_cells: usize,
    pub max_diagnostics: usize,
    pub max_updates: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextPhysicalComparisonSide {
    Original,
    Physical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextPhysicalComparisonField {
    Width,
    PanelWidths,
    Palette { reference: u8 },
    ScreenPlane,
    RowPlane { row: u8 },
    ReceiverHeader,
    CellPresence,
    Fragment,
    OriginAnchor,
    EffectiveSize,
    ForegroundReference,
    BackgroundReference,
    Flash,
    Boxed,
    Conceal,
    Invert,
    GlyphEffects,
    GlyphMask,
    ColourResponse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextPhysicalComparisonPendingKind {
    Origin,
    OriginAttributes,
    SelectedAttributes,
    Charset,
    GlyphIntent,
    Fragment,
}

/// Both sides retain their own source spaces; instance IDs are never equated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextPhysicalComparisonSource {
    pub origin: Option<usize>,
    pub candidate: Option<usize>,
    pub raw_position: Option<(u16, u16)>,
    pub location: Option<TeletextEnhancementSource>,
    pub root_source: Option<TeletextSpatialRootSource>,
    pub invocation: Option<TeletextSpatialInvocationId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TeletextPhysicalComparisonFinding {
    Difference(TeletextPhysicalComparisonField),
    Pending {
        side: TeletextPhysicalComparisonSide,
        kind: TeletextPhysicalComparisonPendingKind,
        index: Option<usize>,
    },
    Glyph {
        side: TeletextPhysicalComparisonSide,
        error: Box<TeletextRasterCellFailure>,
    },
    Colour {
        side: TeletextPhysicalComparisonSide,
        error: Box<TeletextColourCompositionError>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextPhysicalComparisonDiagnostic {
    /// Visual cells/rows; palette and screen diagnostics have no invented cell.
    pub position: Option<(u8, u8)>,
    pub original: TeletextPhysicalComparisonSource,
    pub physical: TeletextPhysicalComparisonSource,
    pub finding: TeletextPhysicalComparisonFinding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextPhysicalComparisonFailure {
    #[error("original and physical exact owners differ")]
    OwnerMismatch,
    #[error("{kind} count {count} exceeds trusted budget {maximum}")]
    Budget {
        kind: &'static str,
        count: usize,
        maximum: usize,
    },
    #[error("comparison storage/work arithmetic exceeds supported range")]
    AllocationSize,
    #[error("comparison diagnostic allocation failed")]
    Allocation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("teletext comparison for {owner:?} at {position:?}: {reason}")]
pub struct TeletextPhysicalComparisonError {
    pub owner: TeletextPageAddress,
    pub position: Option<(u8, u8)>,
    pub reason: TeletextPhysicalComparisonFailure,
}

/// Diagnostic view bound to the two borrowed immutable selected models.
///
/// Zero diagnostics does NOT certify authored glyph identity, source lineage,
/// matching service generations, independent receiver equivalence, object/service
/// admission or readiness/publication. Equal masks may represent distinct glyph
/// identities. A future producer-owned source map must establish transformations.
/// Pending histories are retained separately from selected/retained findings: a superseded
/// C.2 writer can remain in a log without making the current selection unknown.
#[derive(Debug)]
pub struct TeletextPhysicalComparison<'a> {
    original: &'a TeletextMainComposition,
    physical: &'a TeletextPhysicalDisplayReplay,
    compared_cells: usize,
    diagnostics: Vec<TeletextPhysicalComparisonDiagnostic>,
}
impl TeletextPhysicalComparison<'_> {
    #[must_use]
    pub const fn original(&self) -> &TeletextMainComposition {
        self.original
    }
    #[must_use]
    pub const fn physical(&self) -> &TeletextPhysicalDisplayReplay {
        self.physical
    }
    #[must_use]
    pub const fn compared_cells(&self) -> usize {
        self.compared_cells
    }
    #[must_use]
    pub fn diagnostics(&self) -> &[TeletextPhysicalComparisonDiagnostic] {
        &self.diagnostics
    }
    /// Full display history, including superseded writers and nonvisible origins.
    #[must_use]
    pub fn original_pending(&self) -> &[TeletextDisplayPending] {
        self.original.display_resolution().pending()
    }
    #[must_use]
    pub fn physical_pending(&self) -> &[TeletextDisplayPending] {
        self.physical.display_resolution().pending()
    }
}

/// Compare every selected visual cell and all selected palette/plane values.
///
/// Original Base masks include authored Unicode replacement intent. Physical Base
/// masks use only actual coded fallback; enhancement masks use actual glyph intent.
/// Requested size flags and raw Base/enhanced separation mechanisms are not blindly
/// equated: effective fragment geometry and glyph effects determine this view.
/// Full flash rate and otherwise invisible flags remain structural observations.
/// Retained origin/charset/glyph uncertainty is observed even without any seeds;
/// nonvisible origin uncertainty is distinct from selected-cell uncertainty.
/// Plane getters compare composed RGB/video, not raw plane-reference identity; the
/// retaining replay factory separately guards its original plane context. Same-owner
/// caller pairing does not prove matching generations.
/// Full pending histories and independent admission facts remain accessible through
/// the borrowed inputs. This is a comparison precursor, not an equivalence proof.
/// # Errors
/// Fences aggregate records, cells, diagnostic storage and operations; returns no
/// partial result on failure. Inputs are never mutated.
pub fn compare_teletext_physical_display<'a>(
    original: &'a TeletextMainComposition,
    physical: &'a TeletextPhysicalDisplayReplay,
    budget: TeletextPhysicalComparisonBudget,
) -> Result<TeletextPhysicalComparison<'a>, TeletextPhysicalComparisonError> {
    compare_teletext_physical_display_with_usage(original, physical, budget).0
}

/// Compare unchanged display evidence and return admitted updates on either outcome.
/// Usage includes the existing conservative bulk reservations, not CPU time or
/// total memory. A denied reservation consumes no credit; earlier admitted work
/// is retained after later failures. No partial comparison escapes on failure.
/// # Errors
/// The embedded result retains all existing owner, located budget and allocation errors.
pub fn compare_teletext_physical_display_with_usage<'a>(
    original: &'a TeletextMainComposition,
    physical: &'a TeletextPhysicalDisplayReplay,
    budget: TeletextPhysicalComparisonBudget,
) -> (
    Result<TeletextPhysicalComparison<'a>, TeletextPhysicalComparisonError>,
    usize,
) {
    let mut builder = Builder {
        result: TeletextPhysicalComparison {
            original,
            physical,
            compared_cells: 0,
            diagnostics: Vec::new(),
        },
        budget,
        remaining: budget.max_updates,
    };
    let outcome = (|| {
        builder.preflight()?;
        builder.global_pending(TeletextPhysicalComparisonSide::Original)?;
        builder.global_pending(TeletextPhysicalComparisonSide::Physical)?;
        builder.planes()?;
        let width = original
            .display_resolution()
            .width()
            .max(physical.display_resolution().width());
        for row in 0..25 {
            for column in 0..width {
                builder.cell(row, column)?;
                builder.result.compared_cells += 1;
            }
        }
        Ok(())
    })();
    let admitted = budget.max_updates - builder.remaining;
    (outcome.map(|()| builder.result), admitted)
}

type Side = TeletextPhysicalComparisonSide;
type Field = TeletextPhysicalComparisonField;
type Finding = TeletextPhysicalComparisonFinding;
type Error = TeletextPhysicalComparisonError;

struct Builder<'a> {
    result: TeletextPhysicalComparison<'a>,
    budget: TeletextPhysicalComparisonBudget,
    remaining: usize,
}
impl Builder<'_> {
    fn error(
        &self,
        position: Option<(u8, u8)>,
        reason: TeletextPhysicalComparisonFailure,
    ) -> Error {
        Error {
            owner: self.result.original.owner(),
            position,
            reason,
        }
    }
    fn charge(&mut self, position: Option<(u8, u8)>) -> Result<(), Error> {
        self.remaining = self.remaining.checked_sub(1).ok_or_else(|| {
            self.error(
                position,
                TeletextPhysicalComparisonFailure::Budget {
                    kind: "updates",
                    count: self.budget.max_updates.saturating_add(1),
                    maximum: self.budget.max_updates,
                },
            )
        })?;
        Ok(())
    }
    fn charge_many(&mut self, count: usize, position: Option<(u8, u8)>) -> Result<(), Error> {
        self.remaining = self.remaining.checked_sub(count).ok_or_else(|| {
            self.error(
                position,
                TeletextPhysicalComparisonFailure::Budget {
                    kind: "updates",
                    count: self.budget.max_updates.saturating_add(1),
                    maximum: self.budget.max_updates,
                },
            )
        })?;
        Ok(())
    }
    fn preflight(&mut self) -> Result<(), Error> {
        if self.result.original.owner() != self.result.physical.owner() {
            return Err(self.error(None, TeletextPhysicalComparisonFailure::OwnerMismatch));
        }
        let records = record_count(
            self.result.original.spatial_trace().events().len(),
            self.result.original.glyph_intent().records().len(),
            self.result.original.display_resolution(),
            self.result.original.charset_execution().pending().len(),
        )
        .and_then(|first| {
            record_count(
                self.result.physical.spatial_trace().events().len(),
                self.result.physical.glyph_intent().records().len(),
                self.result.physical.display_resolution(),
                self.result.physical.charset_execution().pending().len(),
            )
            .and_then(|second| first.checked_add(second))
        })
        .ok_or_else(|| self.error(None, TeletextPhysicalComparisonFailure::AllocationSize))?;
        let cells = 25
            * usize::from(
                self.result
                    .original
                    .display_resolution()
                    .width()
                    .max(self.result.physical.display_resolution().width()),
            );
        for (kind, count, maximum) in [
            ("records", records, self.budget.max_records),
            ("cells", cells, self.budget.max_cells),
        ] {
            if count > maximum {
                return Err(self.error(
                    None,
                    TeletextPhysicalComparisonFailure::Budget {
                        kind,
                        count,
                        maximum,
                    },
                ));
            }
        }
        if self.budget.max_updates < cells {
            return Err(self.error(
                None,
                TeletextPhysicalComparisonFailure::Budget {
                    kind: "updates",
                    count: cells,
                    maximum: self.budget.max_updates,
                },
            ));
        }
        let capacity = records
            .checked_mul(2)
            .and_then(|n| cells.checked_mul(40).and_then(|cells| n.checked_add(cells)))
            .and_then(|n| n.checked_add(60))
            .ok_or_else(|| self.error(None, TeletextPhysicalComparisonFailure::AllocationSize))?
            .min(self.budget.max_diagnostics);
        if capacity
            .checked_mul(std::mem::size_of::<TeletextPhysicalComparisonDiagnostic>())
            .is_none_or(|bytes| isize::try_from(bytes).is_err())
            || cells
                .checked_mul(260)
                .and_then(|n| n.checked_add(records))
                .is_none()
        {
            return Err(self.error(None, TeletextPhysicalComparisonFailure::AllocationSize));
        }
        self.result
            .diagnostics
            .try_reserve_exact(capacity)
            .map_err(|_| self.error(None, TeletextPhysicalComparisonFailure::Allocation))?;
        Ok(())
    }
    fn push(
        &mut self,
        position: Option<(u8, u8)>,
        original: TeletextPhysicalComparisonSource,
        physical: TeletextPhysicalComparisonSource,
        finding: Finding,
    ) -> Result<(), Error> {
        if self.result.diagnostics.len() == self.budget.max_diagnostics {
            return Err(self.error(
                position,
                TeletextPhysicalComparisonFailure::Budget {
                    kind: "diagnostics",
                    count: self.budget.max_diagnostics.saturating_add(1),
                    maximum: self.budget.max_diagnostics,
                },
            ));
        }
        // The preflight bound covers <=2 findings/retained record, <=40 per
        // visual cell and 60 plane/palette findings. Keep growth fallible even
        // if that bound changes in a later diagnostic extension.
        if self.result.diagnostics.len() == self.result.diagnostics.capacity() {
            self.result
                .diagnostics
                .try_reserve(1)
                .map_err(|_| self.error(position, TeletextPhysicalComparisonFailure::Allocation))?;
        }
        self.result
            .diagnostics
            .push(TeletextPhysicalComparisonDiagnostic {
                position,
                original,
                physical,
                finding,
            });
        Ok(())
    }
    fn difference(&mut self, position: Option<(u8, u8)>, field: Field) -> Result<(), Error> {
        let original = cell_source(
            self.result.original.display_resolution(),
            self.result.original.glyph_intent(),
            position,
        );
        let physical = cell_source(
            self.result.physical.display_resolution(),
            self.result.physical.glyph_intent(),
            position,
        );
        self.push(position, original, physical, Finding::Difference(field))
    }
    fn global_pending(&mut self, side: Side) -> Result<(), Error> {
        let (display, intent) = match side {
            Side::Original => (
                self.result.original.display_resolution(),
                self.result.original.glyph_intent(),
            ),
            Side::Physical => (
                self.result.physical.display_resolution(),
                self.result.physical.glyph_intent(),
            ),
        };
        for (index, origin) in display.origins().iter().enumerate() {
            self.charge(origin.visual_position())?;
            let source = origin_source(display, intent, index);
            let finding = if let Some(pending) = origin.pending() {
                Some((TeletextPhysicalComparisonPendingKind::Origin, Some(pending)))
            } else if display
                .origin_attributes(index)
                .is_some_and(attributes_pending)
            {
                Some((
                    TeletextPhysicalComparisonPendingKind::OriginAttributes,
                    None,
                ))
            } else {
                None
            };
            if let Some((kind, index)) = finding {
                self.side_finding(
                    side,
                    origin.visual_position(),
                    source,
                    Finding::Pending { side, kind, index },
                )?;
            }
        }
        for (index, record) in intent.records().iter().enumerate() {
            self.charge(None)?;
            // Raw alias-order history can remain unresolved while every
            // applicable pair proves one glyph identity. Compare actual glyph
            // uncertainty; retaining that history is not a display failure.
            let selection = match record.reason() {
                Some(ersatzrs_domain::TeletextGlyphIntentReason::CharsetOrder { pending }) => {
                    Some(pending)
                }
                _ => None,
            };
            let kind = if selection.is_some() {
                Some(TeletextPhysicalComparisonPendingKind::Charset)
            } else if record.disposition() != TeletextGlyphDisposition::Assigned {
                Some(TeletextPhysicalComparisonPendingKind::GlyphIntent)
            } else {
                None
            };
            if let Some(kind) = kind {
                let candidate = record.context().candidate();
                let source = candidate_source(candidate, Some(index), None);
                self.side_finding(
                    side,
                    None,
                    source,
                    Finding::Pending {
                        side,
                        kind,
                        index: selection.or(Some(index)),
                    },
                )?;
            }
        }
        Ok(())
    }
    fn side_finding(
        &mut self,
        side: Side,
        position: Option<(u8, u8)>,
        source: TeletextPhysicalComparisonSource,
        finding: Finding,
    ) -> Result<(), Error> {
        let empty = empty_source();
        let (original, physical) = match side {
            Side::Original => (source, empty),
            Side::Physical => (empty, source),
        };
        self.push(position, original, physical, finding)
    }
    fn planes(&mut self) -> Result<(), Error> {
        let original = self.result.original;
        let physical = self.result.physical;
        self.charge(None)?;
        if original.display_resolution().width() != physical.display_resolution().width() {
            self.difference(None, Field::Width)?;
        }
        if (
            original.display_geometry().left_width(),
            original.display_geometry().right_width(),
        ) != (
            physical.display_geometry().left_width(),
            physical.display_geometry().right_width(),
        ) {
            self.difference(None, Field::PanelWidths)?;
        }
        for reference in 0..32 {
            self.charge(None)?;
            // Palette references are a finite internal enumeration, not authored input.
            let reference_value = TeletextColourReference::new(reference)
                .map_err(|_| self.error(None, TeletextPhysicalComparisonFailure::AllocationSize))?;
            if original
                .colour_composition()
                .palette_colour(reference_value)
                != physical
                    .colour_composition()
                    .palette_colour(reference_value)
            {
                self.difference(None, Field::Palette { reference })?;
            }
        }
        if original.colour_composition().screen_pixel()
            != physical.colour_composition().screen_pixel()
        {
            self.difference(None, Field::ScreenPlane)?;
        }
        for row in 0..25 {
            self.charge(Some((row, 0)))?;
            if original.colour_composition().row_pixel(row)
                != physical.colour_composition().row_pixel(row)
            {
                self.difference(None, Field::RowPlane { row })?;
            }
        }
        Ok(())
    }
    fn cell(&mut self, row: u8, column: u8) -> Result<(), Error> {
        let position = Some((row, column));
        self.charge(position)?;
        let first = self.result.original.display_resolution().cell(row, column);
        let second = self.result.physical.display_resolution().cell(row, column);
        let (Some(first), Some(second)) = (first, second) else {
            if first.is_some() != second.is_some() {
                self.difference(
                    position,
                    if row == 0 {
                        Field::ReceiverHeader
                    } else {
                        Field::CellPresence
                    },
                )?;
            }
            return Ok(());
        };
        if first.fragment() == TeletextDisplayFragment::Pending
            || second.fragment() == TeletextDisplayFragment::Pending
        {
            for (side, cell) in [(Side::Original, first), (Side::Physical, second)] {
                if cell.fragment() == TeletextDisplayFragment::Pending {
                    self.pending_cell(
                        side,
                        position,
                        TeletextPhysicalComparisonPendingKind::Fragment,
                    )?;
                }
            }
            return Ok(());
        }
        if first.fragment() != second.fragment() {
            self.difference(position, Field::Fragment)?;
        }
        self.origins(position, first, second)?;
        self.attributes(position, first, second)?;
        let original_glyph = comparison_original_glyph(self.result.original, first);
        let physical_glyph = comparison_physical_glyph(self.result.physical, second);
        let (Ok(first_glyph), Ok(second_glyph)) = (&original_glyph, &physical_glyph) else {
            for (side, result) in [
                (Side::Original, original_glyph),
                (Side::Physical, physical_glyph),
            ] {
                if let Err(error) = result {
                    self.cell_finding(
                        side,
                        position,
                        Finding::Glyph {
                            side,
                            error: Box::new(error),
                        },
                    )?;
                }
            }
            return Ok(());
        };
        if first_glyph.effects != second_glyph.effects {
            self.difference(position, Field::GlyphEffects)?;
        }
        if first_glyph.mask != second_glyph.mask {
            self.difference(position, Field::GlyphMask)?;
        }
        self.colours(row, column, first, *first_glyph, second, *second_glyph)
    }
    fn origins(
        &mut self,
        position: Option<(u8, u8)>,
        first: TeletextDisplayCell<'_>,
        second: TeletextDisplayCell<'_>,
    ) -> Result<(), Error> {
        let first = first
            .origin()
            .and_then(|i| self.result.original.display_resolution().origins().get(i));
        let second = second
            .origin()
            .and_then(|i| self.result.physical.display_resolution().origins().get(i));
        if first.and_then(ersatzrs_domain::TeletextDisplayOrigin::visual_position)
            != second.and_then(ersatzrs_domain::TeletextDisplayOrigin::visual_position)
        {
            self.difference(position, Field::OriginAnchor)?;
        }
        if first.and_then(ersatzrs_domain::TeletextDisplayOrigin::effective_size)
            != second.and_then(ersatzrs_domain::TeletextDisplayOrigin::effective_size)
        {
            self.difference(position, Field::EffectiveSize)?;
        }
        Ok(())
    }
    fn attributes(
        &mut self,
        position: Option<(u8, u8)>,
        first: TeletextDisplayCell<'_>,
        second: TeletextDisplayCell<'_>,
    ) -> Result<(), Error> {
        let first = first.attributes();
        let second = second.attributes();
        for (side, attrs) in [(Side::Original, first), (Side::Physical, second)] {
            if attributes_pending(attrs) {
                self.pending_cell(
                    side,
                    position,
                    TeletextPhysicalComparisonPendingKind::SelectedAttributes,
                )?;
            }
        }
        for (field, different) in [
            (
                Field::ForegroundReference,
                first.foreground().value() != second.foreground().value(),
            ),
            (
                Field::BackgroundReference,
                first.background().value() != second.background().value(),
            ),
            (
                Field::Flash,
                first.flash().value() != second.flash().value(),
            ),
            (
                Field::Boxed,
                first.boxed().value() != second.boxed().value(),
            ),
            (
                Field::Conceal,
                first.conceal().value() != second.conceal().value(),
            ),
            (
                Field::Invert,
                first.invert().value() != second.invert().value(),
            ),
        ] {
            self.charge(position)?;
            if different {
                self.difference(position, field)?;
            }
        }
        Ok(())
    }
    fn pending_cell(
        &mut self,
        side: Side,
        position: Option<(u8, u8)>,
        kind: TeletextPhysicalComparisonPendingKind,
    ) -> Result<(), Error> {
        self.cell_finding(
            side,
            position,
            Finding::Pending {
                side,
                kind,
                index: None,
            },
        )
    }
    fn cell_finding(
        &mut self,
        side: Side,
        position: Option<(u8, u8)>,
        finding: Finding,
    ) -> Result<(), Error> {
        let source = match side {
            Side::Original => cell_source(
                self.result.original.display_resolution(),
                self.result.original.glyph_intent(),
                position,
            ),
            Side::Physical => cell_source(
                self.result.physical.display_resolution(),
                self.result.physical.glyph_intent(),
                position,
            ),
        };
        self.side_finding(side, position, source, finding)
    }
    fn colours(
        &mut self,
        row: u8,
        column: u8,
        first: TeletextDisplayCell<'_>,
        first_glyph: ComparisonGlyph,
        second: TeletextDisplayCell<'_>,
        second_glyph: ComparisonGlyph,
    ) -> Result<(), Error> {
        // Each mask is inspected over exactly its 120 actual fragment pixels.
        self.charge_many(
            120 * (usize::from(first_glyph.mask.is_some())
                + usize::from(second_glyph.mask.is_some())),
            Some((row, column)),
        )?;
        let first_coverage = coverage(first.fragment(), first_glyph);
        let second_coverage = coverage(second.fragment(), second_glyph);
        for reveal_concealed in [false, true] {
            for flash_phase in [TeletextFlashPhase::Original, TeletextFlashPhase::Alternate] {
                for covered in [false, true] {
                    if !first_coverage[usize::from(covered)]
                        && !second_coverage[usize::from(covered)]
                    {
                        continue;
                    }
                    self.charge(Some((row, column)))?;
                    let options = TeletextColourPixelOptions {
                        reveal_concealed,
                        flash_phase,
                    };
                    let original = first_coverage[usize::from(covered)].then(|| {
                        self.result.original.colour_composition().pixel(
                            self.result.original.display_resolution(),
                            row,
                            column,
                            covered,
                            options,
                        )
                    });
                    let physical = second_coverage[usize::from(covered)].then(|| {
                        self.result.physical.colour_composition().pixel(
                            self.result.physical.display_resolution(),
                            row,
                            column,
                            covered,
                            options,
                        )
                    });
                    match (original, physical) {
                        (Some(Ok(first)), Some(Ok(second))) if first != second => {
                            self.difference(Some((row, column)), Field::ColourResponse)?;
                        }
                        (original, physical) => {
                            for (side, result) in
                                [(Side::Original, original), (Side::Physical, physical)]
                            {
                                if let Some(Err(error)) = result {
                                    self.cell_finding(
                                        side,
                                        Some((row, column)),
                                        Finding::Colour {
                                            side,
                                            error: Box::new(error),
                                        },
                                    )?;
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

fn record_count(
    events: usize,
    intent: usize,
    display: &TeletextDisplayResolution,
    charset_pending: usize,
) -> Option<usize> {
    events
        .checked_add(intent)?
        .checked_add(display.origins().len())?
        .checked_add(display.writers().len())?
        .checked_add(display.pending().len())?
        .checked_add(charset_pending)
}
fn attributes_pending(attrs: ersatzrs_domain::TeletextDisplayAttributes<'_>) -> bool {
    attrs.foreground().value().is_none()
        || attrs.background().value().is_none()
        || attrs.flash().value().is_none()
        || attrs.double_height().value().is_none()
        || attrs.double_width().value().is_none()
        || attrs.boxed().value().is_none()
        || attrs.conceal().value().is_none()
        || attrs.invert().value().is_none()
        || attrs.underline_or_separated_mosaics().value().is_none()
        || attrs.spacing_separated().value().is_none()
}
fn empty_source() -> TeletextPhysicalComparisonSource {
    TeletextPhysicalComparisonSource {
        origin: None,
        candidate: None,
        raw_position: None,
        location: None,
        root_source: None,
        invocation: None,
    }
}
fn candidate_source(
    candidate: &ersatzrs_domain::TeletextGlyphCandidate,
    index: Option<usize>,
    origin: Option<usize>,
) -> TeletextPhysicalComparisonSource {
    TeletextPhysicalComparisonSource {
        origin,
        candidate: index,
        raw_position: Some(candidate.position()),
        location: candidate.source(),
        root_source: candidate.root_source(),
        invocation: candidate.invocation(),
    }
}
fn origin_source(
    display: &TeletextDisplayResolution,
    intent: &ersatzrs_domain::TeletextGlyphIntent,
    index: usize,
) -> TeletextPhysicalComparisonSource {
    let candidate = display
        .origins()
        .get(index)
        .map(ersatzrs_domain::TeletextDisplayOrigin::candidate_index);
    candidate
        .and_then(|candidate| {
            intent.records().get(candidate).map(|record| {
                candidate_source(record.context().candidate(), Some(candidate), Some(index))
            })
        })
        .unwrap_or_else(empty_source)
}
fn cell_source(
    display: &TeletextDisplayResolution,
    intent: &ersatzrs_domain::TeletextGlyphIntent,
    position: Option<(u8, u8)>,
) -> TeletextPhysicalComparisonSource {
    position
        .and_then(|(row, column)| display.cell(row, column))
        .and_then(TeletextDisplayCell::origin)
        .map_or_else(empty_source, |index| origin_source(display, intent, index))
}
fn coverage(fragment: TeletextDisplayFragment, glyph: ComparisonGlyph) -> [bool; 2] {
    let Some(mask) = glyph.mask else {
        return [true, false];
    };
    let mut result = [false; 2];
    for row in 0..10 {
        for column in 0..12 {
            let (x, y) = fragment_position(fragment, column, row);
            if let Some(covered) = mask.pixel(x, y) {
                result[usize::from(covered)] = true;
            }
        }
    }
    result
}
