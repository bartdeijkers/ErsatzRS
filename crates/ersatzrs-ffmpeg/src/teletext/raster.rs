//! Bounded diagnostic raster from the exact unpublished encoder composition.
//!
//! Known cells produce RGB4/video pixels. Unresolved cells retain located
//! diagnostics instead of replacement glyphs or guessed attribute order. This
//! is a preview intermediate, not a wire/readiness/publication certificate.

use std::mem::size_of;

use ersatzrs_domain::{
    resolve_teletext_glyph_mask, resolve_teletext_latin_replacement_mask,
    TeletextColourCompositionError, TeletextColourPixel, TeletextColourPixelOptions,
    TeletextDisplayFragment, TeletextDisplayPending, TeletextEnhancementSource,
    TeletextGlyphDisposition, TeletextGlyphIdentity, TeletextGlyphIntentReason, TeletextGlyphMask,
    TeletextGlyphMaskError, TeletextPageAddress, TeletextSpatialInvocationId,
    TeletextSpatialRootSource,
};

use super::TeletextMainComposition;

const CELL_WIDTH: usize = 12;
const CELL_HEIGHT: usize = 10;
const ROWS: usize = 25;

/// Host resource policy, never a serialized producer capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextRasterBudget {
    pub max_pixels: usize,
    pub max_diagnostics: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextRasterPixel {
    Colour(TeletextColourPixel),
    /// Receiver-owned header area. Exact page/subpage metadata comes from
    /// `TeletextRaster::owner`; no authored glyph or missing-data claim.
    ReceiverHeader,
    /// Index into this raster's original located diagnostic registry.
    Unresolved(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextRaster {
    owner: TeletextPageAddress,
    width: usize,
    pixels: Vec<TeletextRasterPixel>,
    diagnostics: Vec<TeletextRasterDiagnostic>,
}
impl TeletextRaster {
    #[must_use]
    pub const fn owner(&self) -> TeletextPageAddress {
        self.owner
    }
    #[must_use]
    pub const fn width(&self) -> usize {
        self.width
    }
    #[must_use]
    pub const fn height(&self) -> usize {
        ROWS * CELL_HEIGHT
    }
    #[must_use]
    pub fn pixels(&self) -> &[TeletextRasterPixel] {
        &self.pixels
    }
    #[must_use]
    pub fn pixel(&self, x: usize, y: usize) -> Option<TeletextRasterPixel> {
        (x < self.width && y < self.height()).then(|| self.pixels[y * self.width + x])
    }
    #[must_use]
    pub fn diagnostics(&self) -> &[TeletextRasterDiagnostic] {
        &self.diagnostics
    }
}

/// Original coded position/source and visual cell remain distinct for panels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextRasterDiagnostic {
    pub owner: TeletextPageAddress,
    pub visual_position: (u8, u8),
    pub candidate: Option<usize>,
    pub original_position: Option<(u16, u16)>,
    pub location: Option<TeletextEnhancementSource>,
    pub root_source: Option<TeletextSpatialRootSource>,
    pub invocation: Option<TeletextSpatialInvocationId>,
    pub reason: TeletextRasterCellFailure,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TeletextRasterCellFailure {
    #[error("this ordinary display cell lacks its original display evidence")]
    MissingDisplayCell,
    #[error("display selection remains unresolved")]
    Display {
        pending: Option<Box<TeletextDisplayPending>>,
    },
    #[error("glyph assignment remains {disposition:?}: {reason:?}")]
    GlyphIntent {
        disposition: TeletextGlyphDisposition,
        reason: Option<TeletextGlyphIntentReason>,
    },
    #[error("original selected glyph/candidate context is absent")]
    MissingGlyph,
    #[error("resolved fragment references a pixel outside its glyph mask")]
    MaskPosition,
    #[error(transparent)]
    Mask(#[from] TeletextGlyphMaskError),
    #[error("separation or underline attributes remain unresolved")]
    GlyphAttribute {
        pending: Option<Box<TeletextDisplayPending>>,
    },
    #[error(transparent)]
    Colour(#[from] TeletextColourCompositionError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextRasterError {
    #[error("pixel count {count} exceeds host budget {maximum}")]
    PixelBudget { count: usize, maximum: usize },
    #[error("diagnostic count {count} exceeds host budget {maximum}")]
    DiagnosticBudget { count: usize, maximum: usize },
    #[error("raster allocation size exceeds storage range")]
    AllocationSize,
    #[error("raster storage allocation failed")]
    Allocation,
}

/// Rasterize selected fragments without silently filling unresolved cells.
///
/// Pixel/storage arithmetic, exact pixel allocation and maximum diagnostic
/// storage are fenced before emission. At most one located diagnostic is retained
/// per visual cell; its 120 pixels refer to that diagnostic. Receiver-owned
/// header areas remain a distinct role for client chrome. EN 300 706 section
/// 12.5.2 permits header reformatting; E.7 does not mandate a page-ID glyph layout.
/// # Errors
/// Returns no partial raster on an exceeded host budget or allocation failure.
pub fn render_teletext_composition(
    composition: &TeletextMainComposition,
    options: TeletextColourPixelOptions,
    budget: TeletextRasterBudget,
) -> Result<TeletextRaster, TeletextRasterError> {
    let display = composition.display_resolution();
    let width = usize::from(display.width()) * CELL_WIDTH;
    let count = width * ROWS * CELL_HEIGHT;
    if count > budget.max_pixels {
        return Err(TeletextRasterError::PixelBudget {
            count,
            maximum: budget.max_pixels,
        });
    }
    let cell_count = usize::from(display.width()) * ROWS;
    storage_fits::<TeletextRasterPixel>(count)?;
    storage_fits::<TeletextRasterDiagnostic>(cell_count.min(budget.max_diagnostics))?;
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(count)
        .map_err(|_| TeletextRasterError::Allocation)?;
    pixels.resize(count, TeletextRasterPixel::Unresolved(0));
    let mut diagnostics = Vec::new();
    diagnostics
        .try_reserve_exact(cell_count.min(budget.max_diagnostics))
        .map_err(|_| TeletextRasterError::Allocation)?;
    for row in 0..25 {
        for column in 0..display.width() {
            let result = render_cell(composition, row, column, options);
            let cell_pixels = match result {
                Ok(value) => value,
                Err(reason) => {
                    let index = diagnostics.len();
                    if index >= budget.max_diagnostics {
                        return Err(TeletextRasterError::DiagnosticBudget {
                            count: index + 1,
                            maximum: budget.max_diagnostics,
                        });
                    }
                    diagnostics.push(diagnostic(composition, row, column, reason));
                    [TeletextRasterPixel::Unresolved(index); CELL_WIDTH * CELL_HEIGHT]
                }
            };
            for y in 0..CELL_HEIGHT {
                let start =
                    (usize::from(row) * CELL_HEIGHT + y) * width + usize::from(column) * CELL_WIDTH;
                pixels[start..start + CELL_WIDTH]
                    .copy_from_slice(&cell_pixels[y * CELL_WIDTH..(y + 1) * CELL_WIDTH]);
            }
        }
    }
    Ok(TeletextRaster {
        owner: composition.owner(),
        width,
        pixels,
        diagnostics,
    })
}

fn storage_fits<T>(count: usize) -> Result<(), TeletextRasterError> {
    if count
        .checked_mul(size_of::<T>())
        .is_none_or(|bytes| isize::try_from(bytes).is_err())
    {
        Err(TeletextRasterError::AllocationSize)
    } else {
        Ok(())
    }
}

fn render_cell(
    composition: &TeletextMainComposition,
    row: u8,
    column: u8,
    options: TeletextColourPixelOptions,
) -> Result<[TeletextRasterPixel; CELL_WIDTH * CELL_HEIGHT], TeletextRasterCellFailure> {
    let display = composition.display_resolution();
    let Some(cell) = display.cell(row, column) else {
        return if row == 0 {
            Ok([TeletextRasterPixel::ReceiverHeader; CELL_WIDTH * CELL_HEIGHT])
        } else {
            Err(TeletextRasterCellFailure::MissingDisplayCell)
        };
    };
    if cell.fragment() == TeletextDisplayFragment::Pending {
        let pending = cell
            .origin()
            .and_then(|origin| display.origins().get(origin))
            .and_then(ersatzrs_domain::TeletextDisplayOrigin::pending)
            .and_then(|index| display.pending().get(index))
            .copied()
            .map(Box::new);
        return Err(TeletextRasterCellFailure::Display { pending });
    }
    let mask = if cell.fragment() == TeletextDisplayFragment::BackgroundOnly {
        None
    } else {
        Some(selected_mask(composition, cell)?)
    };
    let mut pixels = [TeletextRasterPixel::Unresolved(0); CELL_WIDTH * CELL_HEIGHT];
    for y in 0..10_u8 {
        for x in 0..12_u8 {
            let coverage = if let Some(mask) = mask {
                let (source_x, source_y) = fragment_position(cell.fragment(), x, y);
                mask.pixel(source_x, source_y)
                    .ok_or(TeletextRasterCellFailure::MaskPosition)?
            } else {
                false
            };
            pixels[usize::from(y) * CELL_WIDTH + usize::from(x)] = TeletextRasterPixel::Colour(
                composition
                    .colour_composition()
                    .pixel(display, row, column, coverage, options)?,
            );
        }
    }
    Ok(pixels)
}

fn selected_mask(
    composition: &TeletextMainComposition,
    cell: ersatzrs_domain::TeletextDisplayCell<'_>,
) -> Result<TeletextGlyphMask, TeletextRasterCellFailure> {
    let candidate_index = cell
        .origin()
        .and_then(|index| composition.display_resolution().origins().get(index))
        .ok_or(TeletextRasterCellFailure::MissingGlyph)?
        .candidate_index();
    let intent = composition
        .glyph_intent()
        .records()
        .get(candidate_index)
        .ok_or(TeletextRasterCellFailure::MissingGlyph)?;
    let d5 = cell.attributes().underline_or_separated_mosaics();
    let d5_value = d5
        .value()
        .ok_or(TeletextRasterCellFailure::GlyphAttribute {
            pending: d5.pending().copied().map(Box::new),
        })?;
    // A generated Latin replacement retains the selected Base's priority and
    // exact authored position. It never becomes a competing Local glyph.
    if let Some(base) = intent.context().candidate().base() {
        if let Some(operation) =
            composition.base_replacement(base.position().row(), base.position().column())
        {
            let mask = resolve_teletext_latin_replacement_mask(operation)?;
            return Ok(if d5_value {
                mask.with_underline()
            } else {
                mask
            });
        }
    }
    if intent.disposition() != TeletextGlyphDisposition::Assigned {
        return Err(TeletextRasterCellFailure::GlyphIntent {
            disposition: intent.disposition(),
            reason: intent.reason(),
        });
    }
    let identity = intent
        .glyph()
        .ok_or(TeletextRasterCellFailure::MissingGlyph)?;
    let spacing = cell.attributes().spacing_separated();
    let separated = if matches!(identity, TeletextGlyphIdentity::G1Mosaic { .. }) {
        d5_value
            || spacing
                .value()
                .ok_or(TeletextRasterCellFailure::GlyphAttribute {
                    pending: spacing.pending().copied().map(Box::new),
                })?
    } else {
        false
    };
    let drcs = composition
        .drcs_execution()
        .records()
        .binary_search_by_key(
            &candidate_index,
            ersatzrs_domain::TeletextExecutedDrcsGlyph::candidate_index,
        )
        .ok()
        .map(|index| &composition.drcs_execution().records()[index]);
    let mask = resolve_teletext_glyph_mask(identity, drcs, separated)?;
    // Table 29 leaves G3/DRCS D5 response undefined: keep their masks unchanged.
    // Annex G.2 permits decoder-specific underline shaping. Alphanumerics use
    // the bottom row of this raster's selected 12x10 bitmap grid.
    Ok(
        if d5_value
            && matches!(
                identity,
                TeletextGlyphIdentity::G0 { .. }
                    | TeletextGlyphIdentity::G2 { .. }
                    | TeletextGlyphIdentity::Composite { .. }
                    | TeletextGlyphIdentity::AtSymbol
                    | TeletextGlyphIdentity::Blank
                    | TeletextGlyphIdentity::SpacingControl { .. }
            )
        {
            mask.with_underline()
        } else {
            mask
        },
    )
}

fn fragment_position(fragment: TeletextDisplayFragment, x: u8, y: u8) -> (u8, u8) {
    use TeletextDisplayFragment as F;
    match fragment {
        F::DoubleWidthLeft => (x / 2, y),
        F::DoubleWidthRight => (u8::midpoint(x, 12), y),
        F::DoubleHeightTop => (x, y / 2),
        F::DoubleHeightBottom => (x, u8::midpoint(y, 10)),
        F::DoubleSizeTopLeft => (x / 2, y / 2),
        F::DoubleSizeTopRight => (u8::midpoint(x, 12), y / 2),
        F::DoubleSizeBottomLeft => (x / 2, u8::midpoint(y, 10)),
        F::DoubleSizeBottomRight => (u8::midpoint(x, 12), u8::midpoint(y, 10)),
        _ => (x, y),
    }
}

fn diagnostic(
    composition: &TeletextMainComposition,
    row: u8,
    column: u8,
    reason: TeletextRasterCellFailure,
) -> TeletextRasterDiagnostic {
    let candidate = composition
        .display_resolution()
        .cell(row, column)
        .and_then(ersatzrs_domain::TeletextDisplayCell::origin)
        .and_then(|index| composition.display_resolution().origins().get(index))
        .map(ersatzrs_domain::TeletextDisplayOrigin::candidate_index);
    let original = candidate
        .and_then(|index| composition.glyph_intent().records().get(index))
        .map(|record| record.context().candidate());
    TeletextRasterDiagnostic {
        owner: composition.owner(),
        visual_position: (row, column),
        candidate,
        original_position: original.map(ersatzrs_domain::TeletextGlyphCandidate::position),
        location: original.and_then(ersatzrs_domain::TeletextGlyphCandidate::source),
        root_source: original.and_then(ersatzrs_domain::TeletextGlyphCandidate::root_source),
        invocation: original.and_then(ersatzrs_domain::TeletextGlyphCandidate::invocation),
        reason,
    }
}
