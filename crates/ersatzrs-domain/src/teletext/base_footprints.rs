//! Fixed coded-Base fragments before enhanced layering and C.3 replay.
//!
//! Positions refer to the original `BaseDisplay` retained by the same immutable
//! factory. Original coded cells, attributes and authored intent remain there,
//! even when lower-row data is ignored. This value is not a generation proof,
//! enhanced occupied-area resolver, final background or rendering certificate.

use super::{
    TeletextBaseAttributeSource, TeletextBaseAttributeSources, TeletextBaseAttributeValues,
    TeletextBaseDisplay, TeletextBaseDisplayAttributes, TeletextBaseDisplayCell,
    TeletextBaseDisplayCellKind, TeletextBaseDisplayValueError, TeletextBaseSpacingControl,
    TeletextColourReference, TeletextEnhancementPosition, TeletextPageAddress, TELETEXT_COLUMNS,
    TELETEXT_ROWS,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextBaseFragmentKind {
    HeaderOwned,
    Normal,
    DoubleHeightTop,
    DoubleHeightBottom,
    /// No lower Base foreground. Final background still needs upper-row
    /// Base/Active/Local replay, not the ignored lower coded attributes.
    BackgroundOnly,
    PendingHeightBoundary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextBaseFootprintPendingReason {
    /// Spacing double height in rows 0, 23 or 24 has discretionary decoder
    /// behavior. No universal clipping or suppression is inferred here.
    IllegalDoubleHeightRow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextBaseFootprintPending {
    cause: TeletextEnhancementPosition,
}
impl TeletextBaseFootprintPending {
    #[must_use]
    pub const fn cause(self) -> TeletextEnhancementPosition {
        self.cause
    }
    #[must_use]
    pub const fn reason(self) -> TeletextBaseFootprintPendingReason {
        TeletextBaseFootprintPendingReason::IllegalDoubleHeightRow
    }
}

/// A lightweight reference to original coded data and the fragment's origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextBaseFootprintCell {
    coded_position: TeletextEnhancementPosition,
    origin: Option<TeletextEnhancementPosition>,
    kind: TeletextBaseFragmentKind,
    pending: Option<TeletextBaseFootprintPending>,
}
impl TeletextBaseFootprintCell {
    /// Original data remains accessible at this position even when suppressed.
    #[must_use]
    pub const fn coded_position(self) -> TeletextEnhancementPosition {
        self.coded_position
    }
    /// Own cell for Normal/Top, upper cell for Bottom/BackgroundOnly; absent
    /// for broadcast-owned or unresolved fragments.
    #[must_use]
    pub const fn origin(self) -> Option<TeletextEnhancementPosition> {
        self.origin
    }
    #[must_use]
    pub const fn kind(self) -> TeletextBaseFragmentKind {
        self.kind
    }
    #[must_use]
    pub const fn pending(self) -> Option<TeletextBaseFootprintPending> {
        self.pending
    }
}

/// Original spacing evidence and independent row-reset baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextBaseRowFootprint {
    row: u8,
    baseline: TeletextBaseDisplayAttributes,
    first_double_height_control: Option<TeletextEnhancementPosition>,
    suppressed_by: Option<TeletextEnhancementPosition>,
    pending: Option<TeletextBaseFootprintPending>,
}
impl TeletextBaseRowFootprint {
    #[must_use]
    pub const fn row(self) -> u8 {
        self.row
    }
    /// Exact reset attributes, independent of ignored lower-row control data.
    #[must_use]
    pub const fn baseline(self) -> TeletextBaseDisplayAttributes {
        self.baseline
    }
    /// Raw first 0D is retained even in an ignored row. Its presence there does
    /// not cause suppression or cascading into another row.
    #[must_use]
    pub const fn first_double_height_control(self) -> Option<TeletextEnhancementPosition> {
        self.first_double_height_control
    }
    /// Proven whole-lower-main-row suppression by an earlier original 0D.
    #[must_use]
    pub const fn suppressed_by(self) -> Option<TeletextEnhancementPosition> {
        self.suppressed_by
    }
    #[must_use]
    pub const fn pending(self) -> Option<TeletextBaseFootprintPending> {
        self.pending
    }
}

/// Fixed 25 by 40 coded-Base topology. Side panels are not erased or modeled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextBaseFootprints {
    owner: TeletextPageAddress,
    cells: Vec<TeletextBaseFootprintCell>,
    rows: Vec<TeletextBaseRowFootprint>,
}
impl TeletextBaseFootprints {
    #[must_use]
    pub const fn owner(&self) -> TeletextPageAddress {
        self.owner
    }
    /// Exactly 1000 cells in original row/column order, including header ownership.
    #[must_use]
    pub fn cells(&self) -> &[TeletextBaseFootprintCell] {
        &self.cells
    }
    #[must_use]
    pub fn cell(&self, row: u8, column: u8) -> Option<&TeletextBaseFootprintCell> {
        if usize::from(row) >= TELETEXT_ROWS || usize::from(column) >= TELETEXT_COLUMNS {
            return None;
        }
        self.cells
            .get(usize::from(row) * TELETEXT_COLUMNS + usize::from(column))
    }
    #[must_use]
    pub fn rows(&self) -> &[TeletextBaseRowFootprint] {
        &self.rows
    }
    #[must_use]
    pub fn row(&self, row: u8) -> Option<&TeletextBaseRowFootprint> {
        self.rows.get(usize::from(row))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("teletext Base footprint for {owner:?} at {location:?}: {reason}")]
pub struct TeletextBaseFootprintError {
    pub owner: TeletextPageAddress,
    pub location: Option<TeletextEnhancementPosition>,
    pub reason: TeletextBaseFootprintFailure,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextBaseFootprintFailure {
    #[error("row baseline colour reference exceeds supported CLUT range")]
    ColourReference,
    #[error("row baseline attributes are invalid: {0}")]
    Baseline(TeletextBaseDisplayValueError),
}

/// Resolve only the emitted spacing subset's coded-Base occupied fragments.
///
/// Raw 0D anywhere in a processed legal row suppresses the entire next main row,
/// including 0D at column 39 or immediately followed by Normal Height. Ignored
/// lower-row controls do not cascade. Height itself follows Double Height
/// `SetAfter` and Normal Height `SetAt`, independently of enhanced observations.
/// Illegal height-row uncertainty is retained in the affected height boundary
/// and next row; an uncertain lower row's own 0D propagates uncertainty rather
/// than falsely establishing whether that control was processed or ignored.
///
/// The authoritative caller retains the matching original `BaseDisplay` and full
/// row colours. Fixed storage is 1000 small references and 25 row facts, not a
/// producer-input RSS bound. Panels, enhanced footprints, deferred attributes,
/// A.7 validity and rendering/publication readiness remain separate obligations.
/// # Errors
/// Returns a located checked-baseline construction error; illegal raw height
/// rows remain pending rather than adding an authoring prohibition.
pub fn resolve_teletext_base_footprints(
    base: &TeletextBaseDisplay,
    row_colours: &[TeletextColourReference; TELETEXT_ROWS],
) -> Result<TeletextBaseFootprints, TeletextBaseFootprintError> {
    let mut cells = Vec::with_capacity(TELETEXT_ROWS * TELETEXT_COLUMNS);
    let mut rows = Vec::with_capacity(TELETEXT_ROWS);
    let mut suppressed_by = None;
    let mut inherited_pending = None;
    for (index, coded) in base.cells().iter().enumerate() {
        let row = coded[0].position().row();
        let first_control = coded.iter().find_map(|cell| {
            matches!(
                cell.kind(),
                TeletextBaseDisplayCellKind::SpacingControl {
                    control: TeletextBaseSpacingControl::DoubleHeight,
                    ..
                }
            )
            .then_some(cell.position())
        });
        let illegal = if suppressed_by.is_none() && matches!(row, 0 | 23 | 24) {
            first_control.map(|cause| TeletextBaseFootprintPending { cause })
        } else {
            None
        };
        let pending = inherited_pending.or(illegal);
        append_row(&mut cells, coded, suppressed_by, inherited_pending, illegal);
        rows.push(TeletextBaseRowFootprint {
            row,
            baseline: row_baseline(base, row, row_colours[index])?,
            first_double_height_control: first_control,
            suppressed_by,
            pending,
        });
        if pending.is_some() {
            inherited_pending = if inherited_pending.is_none() || first_control.is_some() {
                pending
            } else {
                None
            };
            suppressed_by = None;
        } else if suppressed_by.is_some() {
            suppressed_by = None;
        } else {
            suppressed_by = first_control;
        }
    }
    Ok(TeletextBaseFootprints {
        owner: base.owner(),
        cells,
        rows,
    })
}

fn append_row(
    output: &mut Vec<TeletextBaseFootprintCell>,
    coded: &[TeletextBaseDisplayCell; TELETEXT_COLUMNS],
    suppressed_by: Option<TeletextEnhancementPosition>,
    inherited_pending: Option<TeletextBaseFootprintPending>,
    illegal: Option<TeletextBaseFootprintPending>,
) {
    let row_start = output.len();
    let mut double_height = false;
    for &cell in coded {
        let position = cell.position();
        let pending = inherited_pending
            .or(illegal.filter(|pending| position.column() >= pending.cause.column()));
        let (kind, origin) = if cell.kind() == TeletextBaseDisplayCellKind::HeaderOwned {
            (TeletextBaseFragmentKind::HeaderOwned, None)
        } else if pending.is_some() {
            (TeletextBaseFragmentKind::PendingHeightBoundary, None)
        } else if suppressed_by.is_some() {
            // Only a prior processed legal row creates suppression, so a full
            // preceding row exists here and column is checked within 0..40.
            let upper = output[row_start - TELETEXT_COLUMNS + usize::from(position.column())];
            let kind = if upper.kind == TeletextBaseFragmentKind::DoubleHeightTop {
                TeletextBaseFragmentKind::DoubleHeightBottom
            } else {
                TeletextBaseFragmentKind::BackgroundOnly
            };
            (kind, Some(upper.coded_position))
        } else {
            if matches!(
                cell.kind(),
                TeletextBaseDisplayCellKind::SpacingControl {
                    control: TeletextBaseSpacingControl::NormalHeight,
                    ..
                }
            ) {
                double_height = false;
            }
            let kind = if double_height {
                TeletextBaseFragmentKind::DoubleHeightTop
            } else {
                TeletextBaseFragmentKind::Normal
            };
            (kind, Some(position))
        };
        output.push(TeletextBaseFootprintCell {
            coded_position: position,
            origin,
            kind,
            pending,
        });
        if suppressed_by.is_none()
            && pending.is_none()
            && matches!(
                cell.kind(),
                TeletextBaseDisplayCellKind::SpacingControl {
                    control: TeletextBaseSpacingControl::DoubleHeight,
                    ..
                }
            )
        {
            double_height = true;
        }
    }
}

fn row_baseline(
    base: &TeletextBaseDisplay,
    row: u8,
    row_colour: TeletextColourReference,
) -> Result<TeletextBaseDisplayAttributes, TeletextBaseFootprintError> {
    let fail = |reason| TeletextBaseFootprintError {
        owner: base.owner(),
        location: base
            .cell(row, if row == 0 { 8 } else { 0 })
            .map(|cell| cell.position()),
        reason,
    };
    let presentation = base.presentation();
    let (foreground_clut, background_clut) = presentation.map_or((0, 0), |p| p.remapping.cluts());
    let foreground = TeletextColourReference::new(foreground_clut * 8 + 7)
        .map_err(|_| fail(TeletextBaseFootprintFailure::ColourReference))?;
    let background = if presentation.is_some_and(|p| p.black_background_substitution) {
        row_colour
    } else {
        TeletextColourReference::new(background_clut * 8)
            .map_err(|_| fail(TeletextBaseFootprintFailure::ColourReference))?
    };
    let source = TeletextBaseAttributeSource::RowDefault {
        row,
        start_column: if row == 0 { 8 } else { 0 },
    };
    TeletextBaseDisplayAttributes::new(
        TeletextBaseAttributeValues {
            raw_foreground: 7,
            raw_background: 0,
            foreground,
            background,
            graphics: false,
            separated: false,
            double_height: false,
            boxed: false,
            flash: false,
            conceal: false,
        },
        TeletextBaseAttributeSources {
            foreground: source,
            background: source,
            graphics: source,
            separated: source,
            height: source,
            boxed: source,
            flash: source,
            conceal: source,
        },
    )
    .map_err(|error| fail(TeletextBaseFootprintFailure::Baseline(error)))
}
