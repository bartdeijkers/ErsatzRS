//! EN 300 706 table 30 colours and C.1 planes, without glyph raster authority.

use super::{
    TeletextAttributeSource, TeletextColourReference, TeletextDisplayAttribute,
    TeletextDisplayFragment, TeletextDisplayOrigin, TeletextDisplayPending,
    TeletextDisplayResolution, TeletextEffectivePresentation, TeletextEnhancementSource,
    TeletextFlashMode, TeletextLevel25Palette, TeletextPageAddress, TeletextPageKind,
};

/// Output-only four-bit components; device colour-space conversion is separate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextColourRgb([u8; 3]);
impl TeletextColourRgb {
    #[must_use]
    pub const fn get(self) -> [u8; 3] {
        self.0
    }
}

/// Transparent is a plane selection, not an RGB colour or an assumed alpha clear.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextPaletteColour {
    Rgb(TeletextColourRgb),
    Transparent,
}

/// Final C.1 choice; video is supplied by the presentation consumer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextColourPixel {
    Rgb(TeletextColourRgb),
    Video,
}

/// Explicit observation of the selected rate/phase, not an invented clock epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextFlashPhase {
    Original,
    Alternate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextColourPixelOptions {
    pub reveal_concealed: bool,
    pub flash_phase: TeletextFlashPhase,
}

/// Selected RGB map and original plane metadata; no duplicate cell execution.
/// Background substitution is already materialized in display attributes. This
/// result does not certify table 4's explicit-background parameter requirement
/// for window or colour-table-flash functions, nor geometry/raster readiness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextColourComposition {
    owner: TeletextPageAddress,
    palette: [TeletextPaletteColour; 32],
    rows: [TeletextColourReference; 25],
    screen: TeletextColourReference,
    page_kind: TeletextPageKind,
}

/// Resolve checked declarations without reapplying spacing remapping.
/// Absence uses table 30 receiver colours without manufacturing an authored
/// declaration. The retaining factory supplies exact generation ownership.
#[must_use]
pub fn resolve_teletext_colour_composition(
    display: &TeletextDisplayResolution,
    presentation: TeletextEffectivePresentation<'_>,
    row_colours: &[TeletextColourReference; 25],
    screen_colour: TeletextColourReference,
    page_kind: TeletextPageKind,
) -> TeletextColourComposition {
    let selected = match presentation {
        TeletextEffectivePresentation::Extension { defaults, .. } => defaults.palette,
        TeletextEffectivePresentation::Header { .. } => TeletextLevel25Palette::standard(),
    };
    TeletextColourComposition {
        owner: display.owner(),
        palette: std::array::from_fn(|index| palette_entry(index, selected)),
        rows: *row_colours,
        screen: screen_colour,
        page_kind,
    }
}

fn palette_entry(index: usize, selected: TeletextLevel25Palette) -> TeletextPaletteColour {
    const FIXED: [[u8; 3]; 16] = [
        [0, 0, 0],
        [15, 0, 0],
        [0, 15, 0],
        [15, 15, 0],
        [0, 0, 15],
        [15, 0, 15],
        [0, 15, 15],
        [15, 15, 15],
        [0, 0, 0],
        [7, 0, 0],
        [0, 7, 0],
        [7, 7, 0],
        [0, 0, 7],
        [7, 0, 7],
        [0, 7, 7],
        [7, 7, 7],
    ];
    if index == 8 {
        TeletextPaletteColour::Transparent
    } else {
        let components = match index {
            0..=15 => FIXED[index],
            16..=23 => selected.clut2[index - 16].get(),
            _ => selected.clut3[index - 24].get(),
        };
        TeletextPaletteColour::Rgb(TeletextColourRgb(components))
    }
}

impl TeletextColourComposition {
    #[must_use]
    pub const fn owner(&self) -> TeletextPageAddress {
        self.owner
    }
    #[must_use]
    pub fn palette_colour(&self, colour: TeletextColourReference) -> TeletextPaletteColour {
        self.palette[usize::from(colour.get())]
    }
    /// Border plane above/below display rows, rather than a synthetic cell.
    #[must_use]
    pub fn screen_pixel(&self) -> TeletextColourPixel {
        self.lower_plane(self.screen)
    }
    #[must_use]
    pub fn row_pixel(&self, row: u8) -> Option<TeletextColourPixel> {
        self.rows
            .get(usize::from(row))
            .map(|colour| self.lower_plane(*colour))
    }
    fn lower_plane(&self, colour: TeletextColourReference) -> TeletextColourPixel {
        match self.palette_colour(colour) {
            TeletextPaletteColour::Rgb(rgb) => TeletextColourPixel::Rgb(rgb),
            TeletextPaletteColour::Transparent => TeletextColourPixel::Video,
        }
    }
    /// All sixteen table C.1 combinations, without glyph interpretation/remapping.
    #[must_use]
    pub fn compose_plane_pixel(
        &self,
        colour: TeletextColourReference,
        row_colour: TeletextColourReference,
        boxed: bool,
    ) -> TeletextColourPixel {
        self.compose_index(colour.get(), row_colour, boxed)
    }
    fn compose_index(
        &self,
        colour: u8,
        row_colour: TeletextColourReference,
        boxed: bool,
    ) -> TeletextColourPixel {
        let news_or_subtitle = self.page_kind != TeletextPageKind::FullPage;
        if news_or_subtitle && !boxed {
            return TeletextColourPixel::Video;
        }
        match self.palette[usize::from(colour)] {
            TeletextPaletteColour::Rgb(rgb) => TeletextColourPixel::Rgb(rgb),
            TeletextPaletteColour::Transparent if boxed == news_or_subtitle => {
                self.lower_plane(row_colour)
            }
            TeletextPaletteColour::Transparent => TeletextColourPixel::Video,
        }
    }

    /// Compose binary glyph coverage using original final display attributes.
    /// Coverage uses the effective fragment geometry, not requested size flags.
    /// Conceal substitutes SPACE before selecting foreground/background.
    /// # Errors
    /// Preserves unresolved original provenance and reports absent/foreign cells
    /// or the unverified joint invert/non-steady-flash ordering.
    pub fn pixel(
        &self,
        display: &TeletextDisplayResolution,
        row: u8,
        column: u8,
        glyph_foreground: bool,
        options: TeletextColourPixelOptions,
    ) -> Result<TeletextColourPixel, TeletextColourCompositionError> {
        let position = (u16::from(row), u16::from(column));
        if display.owner() != self.owner {
            return Err(self.error(
                position,
                None,
                TeletextColourCompositionFailure::OwnerMismatch,
            ));
        }
        let cell = display.cell(row, column).ok_or_else(|| {
            self.error(
                position,
                None,
                TeletextColourCompositionFailure::MissingCell,
            )
        })?;
        if cell.fragment() == TeletextDisplayFragment::Pending {
            let pending = cell
                .origin()
                .and_then(|index| display.origins().get(index))
                .and_then(TeletextDisplayOrigin::pending)
                .and_then(|index| display.pending().get(index));
            return Err(self.error(
                position,
                pending.copied(),
                TeletextColourCompositionFailure::DisplayPending,
            ));
        }
        let attrs = cell.attributes();
        let fg = self.known(
            position,
            TeletextColourField::Foreground,
            attrs.foreground(),
        )?;
        let bg = self.known(
            position,
            TeletextColourField::Background,
            attrs.background(),
        )?;
        let boxed = self.known(position, TeletextColourField::Boxed, attrs.boxed())?;
        let conceal = self.known(position, TeletextColourField::Conceal, attrs.conceal())?;
        let invert = self.known(position, TeletextColourField::Invert, attrs.invert())?;
        let flash = self.known(position, TeletextColourField::Flash, attrs.flash())?;
        if invert && flash.mode() != TeletextFlashMode::Steady {
            let mut error = self.error(
                position,
                None,
                TeletextColourCompositionFailure::UnverifiedInvertFlashOrder,
            );
            error.location = attrs
                .invert()
                .writer()
                .and_then(|writer| source_location(writer.source()));
            return Err(error);
        }
        let (foreground, background) = if invert { (bg, fg) } else { (fg, bg) };
        let covered = glyph_foreground
            && (!conceal || options.reveal_concealed)
            && cell.fragment() != TeletextDisplayFragment::BackgroundOnly;
        let colour = if covered {
            flash_colour(foreground, background, flash.mode(), options.flash_phase)
        } else {
            background.get()
        };
        Ok(self.compose_index(colour, self.rows[usize::from(row)], boxed))
    }

    fn known<T: Copy>(
        &self,
        position: (u16, u16),
        field: TeletextColourField,
        attribute: TeletextDisplayAttribute<'_, T>,
    ) -> Result<T, TeletextColourCompositionError> {
        attribute.value().ok_or_else(|| {
            self.error(
                position,
                attribute.pending().copied(),
                TeletextColourCompositionFailure::AttributePending { field },
            )
        })
    }
    fn error(
        &self,
        position: (u16, u16),
        pending: Option<TeletextDisplayPending>,
        reason: TeletextColourCompositionFailure,
    ) -> TeletextColourCompositionError {
        TeletextColourCompositionError {
            owner: self.owner,
            position,
            location: pending.and_then(|value| value.location()),
            pending: pending.map(Box::new),
            reason,
        }
    }
}

fn source_location(source: TeletextAttributeSource) -> Option<TeletextEnhancementSource> {
    match source {
        TeletextAttributeSource::Enhancement { location, .. } => Some(location),
        _ => None,
    }
}

fn flash_colour(
    fg: TeletextColourReference,
    bg: TeletextColourReference,
    mode: TeletextFlashMode,
    phase: TeletextFlashPhase,
) -> u8 {
    match (mode, phase) {
        (TeletextFlashMode::Normal, TeletextFlashPhase::Alternate)
        | (TeletextFlashMode::Inverted, TeletextFlashPhase::Original) => bg.get(),
        (TeletextFlashMode::AdjacentClut, TeletextFlashPhase::Alternate) => fg.get() ^ 8,
        _ => fg.get(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextColourField {
    Foreground,
    Background,
    Boxed,
    Conceal,
    Invert,
    Flash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextColourCompositionFailure {
    #[error("display owner does not match the colour composition")]
    OwnerMismatch,
    #[error("display cell is absent")]
    MissingCell,
    #[error("display fragment has unresolved original provenance")]
    DisplayPending,
    #[error("original colour attribute is unresolved: {field:?}")]
    AttributePending { field: TeletextColourField },
    #[error("joint invert and non-steady flash ordering requires validation")]
    UnverifiedInvertFlashOrder,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("teletext colour composition failed at {position:?}: {reason}")]
pub struct TeletextColourCompositionError {
    pub owner: TeletextPageAddress,
    /// Visual position; original pending location remains separate.
    pub position: (u16, u16),
    pub location: Option<TeletextEnhancementSource>,
    /// Original registry links remain valid in the retained immutable display.
    pub pending: Option<Box<TeletextDisplayPending>>,
    pub reason: TeletextColourCompositionFailure,
}
