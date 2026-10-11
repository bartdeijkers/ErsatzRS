//! EN 300 706 table 30 colours and C.1 planes, without glyph raster authority.

use super::{
    TeletextAttributeSource, TeletextBaseAttributeSource, TeletextBaseSpacingControl,
    TeletextColourReference, TeletextDisplayAttribute, TeletextDisplayAttributes,
    TeletextDisplayFragment, TeletextDisplayOrigin, TeletextDisplayPending,
    TeletextDisplayResolution, TeletextDisplayWriter, TeletextEffectivePresentation,
    TeletextEnhancementSource, TeletextFlashMode, TeletextLevel25Palette, TeletextPageAddress,
    TeletextPageKind,
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
/// result checks definite consumed window transparency and inverted adjacent-CLUT
/// parameter dependencies. Other table 4 authoring requirements and complete
/// geometry/raster readiness remain separate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextColourComposition {
    owner: TeletextPageAddress,
    palette: [TeletextPaletteColour; 32],
    rows: [TeletextColourReference; 25],
    screen: TeletextColourReference,
    page_kind: TeletextPageKind,
    black_background_substitution: bool,
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
    let (selected, black_background_substitution) = match presentation {
        TeletextEffectivePresentation::Extension { defaults, .. } => {
            (defaults.palette, defaults.black_background_substitution)
        }
        TeletextEffectivePresentation::Header { .. } => (TeletextLevel25Palette::standard(), false),
    };
    TeletextColourComposition {
        owner: display.owner(),
        palette: std::array::from_fn(|index| palette_entry(index, selected)),
        rows: *row_colours,
        screen: screen_colour,
        page_kind,
        black_background_substitution,
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
    /// Preserves unresolved original provenance, reports absent/foreign cells, and
    /// rejects the definite table 4 parameter dependencies described above.
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
        // EN 300 706 table 29 D4 exchanges the display colours; mode 07 affects
        // foreground only. Select that exchanged foreground's flash colour
        // before C.1 transparency. QTeletextMaker cac2167 decode.cpp 1481-1498
        // corroborates the adjacent-CLUT case, render.cpp 337-360 normal/inverted.
        let (foreground, background) = if invert { (bg, fg) } else { (fg, bg) };
        let covered = glyph_foreground
            && (!conceal || options.reveal_concealed)
            && cell.fragment() != TeletextDisplayFragment::BackgroundOnly;
        let colour = if covered {
            flash_colour(foreground, background, flash.mode(), options.flash_phase)
        } else {
            background.get()
        };
        self.check_background_parameter(
            position,
            attrs,
            boxed,
            invert,
            flash.mode(),
            colour == 8
                && bg.get() == 8
                && selects_original_background(covered, invert, flash.mode(), options.flash_phase),
        )?;
        let plane_row = self.plane_row(display, row, cell.fragment(), cell.origin(), position)?;
        Ok(self.compose_index(colour, self.rows[usize::from(plane_row)], boxed))
    }

    fn plane_row(
        &self,
        display: &TeletextDisplayResolution,
        row: u8,
        fragment: TeletextDisplayFragment,
        origin: Option<usize>,
        position: (u16, u16),
    ) -> Result<u8, TeletextColourCompositionError> {
        // C.3(4) retains the origin's presentation for enlarged bottom parts.
        // QTeletextMaker cac2167 decode.cpp 1508-1515 and imager 2b0125
        // teletextdecoder.py 1426-1429 corroborate the full-row plane selection.
        // BackgroundOnly is not an enlarged character part and keeps this row.
        if matches!(
            fragment,
            TeletextDisplayFragment::DoubleHeightBottom
                | TeletextDisplayFragment::DoubleSizeBottomLeft
                | TeletextDisplayFragment::DoubleSizeBottomRight
        ) {
            origin
                .and_then(|index| display.origins().get(index))
                .and_then(TeletextDisplayOrigin::visual_position)
                .map(|(origin_row, _)| origin_row)
                .filter(|origin_row| *origin_row < 25)
                .ok_or_else(|| {
                    self.error(
                        position,
                        None,
                        TeletextColourCompositionFailure::MissingOrigin,
                    )
                })
        } else {
            Ok(row)
        }
    }

    // Table 4 (9.4.2.2) requires explicitly transmitted background parameters.
    // Table 29 D1/C.1 identify window transparency; D4 exchanges the colours
    // before adjacent-CLUT pairing. These are definite dependencies only, not
    // whole-page authoring admission or a blanket BBS/window prohibition.
    fn check_background_parameter(
        &self,
        position: (u16, u16),
        attrs: TeletextDisplayAttributes<'_>,
        boxed: bool,
        invert: bool,
        mode: TeletextFlashMode,
        transparent_original_background: bool,
    ) -> Result<(), TeletextColourCompositionError> {
        let Some(background_source) = attrs
            .background()
            .writer()
            .map(TeletextDisplayWriter::source)
        else {
            return Ok(());
        };
        if !self.black_background_substitution || !substituted_background(background_source) {
            return Ok(());
        }
        let (usage, consumer_source) = if invert && mode == TeletextFlashMode::AdjacentClut {
            // The authored paired-CLUT parameter is required in BOTH phases.
            (
                TeletextBackgroundParameterUse::InvertedAdjacentClut,
                attrs.flash().writer().map(TeletextDisplayWriter::source),
            )
        } else if self.page_kind == TeletextPageKind::FullPage
            && boxed
            && transparent_original_background
        {
            (
                TeletextBackgroundParameterUse::WindowTransparency,
                attrs.boxed().writer().map(TeletextDisplayWriter::source),
            )
        } else {
            return Ok(());
        };
        let invert_source = attrs.invert().writer().map(TeletextDisplayWriter::source);
        let mut error = self.error(
            position,
            None,
            TeletextColourCompositionFailure::BackgroundParameterFromSubstitution { usage },
        );
        error.location = consumer_source
            .and_then(enhancement_location)
            .or_else(|| invert_source.and_then(enhancement_location))
            .or_else(|| enhancement_location(background_source));
        error.parameter = Some(Box::new(TeletextBackgroundParameterEvidence {
            background_source,
            consumer_source,
            invert_source,
        }));
        Err(error)
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
            parameter: None,
            reason,
        }
    }
}

fn substituted_background(source: TeletextAttributeSource) -> bool {
    matches!(
        source,
        TeletextAttributeSource::Base {
            source: TeletextBaseAttributeSource::RowDefault { .. }
                | TeletextBaseAttributeSource::Spacing {
                    control: TeletextBaseSpacingControl::BlackBackground,
                    ..
                },
            ..
        }
    )
}

fn enhancement_location(source: TeletextAttributeSource) -> Option<TeletextEnhancementSource> {
    match source {
        TeletextAttributeSource::Enhancement { location, .. } => Some(location),
        _ => None,
    }
}

fn selects_original_background(
    covered: bool,
    invert: bool,
    mode: TeletextFlashMode,
    phase: TeletextFlashPhase,
) -> bool {
    let effective_background = !covered
        || matches!(
            (mode, phase),
            (TeletextFlashMode::Normal, TeletextFlashPhase::Alternate)
                | (TeletextFlashMode::Inverted, TeletextFlashPhase::Original)
        );
    if effective_background {
        !invert
    } else {
        invert
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
    #[error("enlarged bottom fragment has no checked visual origin")]
    MissingOrigin,
    #[error("display fragment has unresolved original provenance")]
    DisplayPending,
    #[error("background parameter depends on black-background substitution: {usage:?}")]
    BackgroundParameterFromSubstitution {
        usage: TeletextBackgroundParameterUse,
    },
    #[error("original colour attribute is unresolved: {field:?}")]
    AttributePending { field: TeletextColourField },
}

/// Proven background-parameter uses; this is not an exhaustive authoring validator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextBackgroundParameterUse {
    WindowTransparency,
    InvertedAdjacentClut,
}

/// Original winning writers, before colour exchange or paired-CLUT selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextBackgroundParameterEvidence {
    pub background_source: TeletextAttributeSource,
    pub consumer_source: Option<TeletextAttributeSource>,
    pub invert_source: Option<TeletextAttributeSource>,
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
    /// Allocated only for a definite table 4 parameter-source failure.
    pub parameter: Option<Box<TeletextBackgroundParameterEvidence>>,
    pub reason: TeletextColourCompositionFailure,
}
