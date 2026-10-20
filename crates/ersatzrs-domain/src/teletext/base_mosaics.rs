//! Row-local physical Base hold/release execution (Table 26, Annex G.3.3).

use super::{
    TeletextBaseAttributeSource, TeletextBaseDisplayCell, TeletextBaseDisplayCellKind,
    TeletextBaseSpacingControl, TeletextDisplaySize, TeletextEnhancementPosition, TeletextMosaic,
    TeletextValueError,
};

/// Original Base source of the held shape, including a standard-defined reset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextHeldMosaicSource {
    RowDefault {
        row: u8,
    },
    Mosaic {
        position: TeletextEnhancementPosition,
    },
    SpacingReset {
        source: TeletextBaseAttributeSource,
    },
}

/// Sealed derived glyph over an unchanged original spacing-control cell.
/// The original mosaic's form survives later spacing separation controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextHeldMosaic {
    mosaic: TeletextMosaic,
    separated: bool,
    source: TeletextHeldMosaicSource,
    hold: TeletextEnhancementPosition,
}
impl TeletextHeldMosaic {
    #[must_use]
    pub const fn mosaic(self) -> TeletextMosaic {
        self.mosaic
    }
    #[must_use]
    pub const fn separated(self) -> bool {
        self.separated
    }
    /// Original glyph or reset that populated the hold register.
    #[must_use]
    pub const fn source(self) -> TeletextHeldMosaicSource {
        self.source
    }
    /// Original `SetAt` control that enabled hold on this row.
    #[must_use]
    pub const fn hold(self) -> TeletextEnhancementPosition {
        self.hold
    }
}

/// Bounded row-local register over checked, row-major physical Base cells.
/// Authoritative callers retain the same immutable trace; this structural
/// executor is not a generation, publication or source-ownership capability.
pub struct TeletextBaseMosaicState {
    row: Option<u8>,
    hold: Option<TeletextEnhancementPosition>,
    mosaic: TeletextMosaic,
    separated: bool,
    source: TeletextHeldMosaicSource,
    graphics: bool,
    size: TeletextDisplaySize,
}
impl Default for TeletextBaseMosaicState {
    fn default() -> Self {
        Self {
            row: None,
            hold: None,
            mosaic: TeletextMosaic::default(),
            separated: false,
            source: TeletextHeldMosaicSource::RowDefault { row: 0 },
            graphics: false,
            size: TeletextDisplaySize::Normal,
        }
    }
}
impl TeletextBaseMosaicState {
    /// Consume one checked Base cell before any X26/object glyphs.
    /// # Errors
    /// Rejects an invalid six-block pattern; no enhancement changes this register.
    pub fn advance(
        &mut self,
        cell: TeletextBaseDisplayCell,
    ) -> Result<Option<TeletextHeldMosaic>, TeletextValueError> {
        let row = cell.position().row();
        if self.row != Some(row) {
            *self = Self {
                row: Some(row),
                source: TeletextHeldMosaicSource::RowDefault { row },
                ..Self::default()
            };
        }
        let attrs = cell.attributes();
        let size = match (attrs.double_width(), attrs.double_height()) {
            (false, false) => TeletextDisplaySize::Normal,
            (true, false) => TeletextDisplaySize::DoubleWidth,
            (false, true) => TeletextDisplaySize::DoubleHeight,
            (true, true) => TeletextDisplaySize::DoubleSize,
        };
        // Checked cell attributes already incorporate the correct SetAt/After
        // boundary. A reinforcement with the same mode/size is not a reset.
        let reset = if self.graphics != attrs.graphics() {
            Some(attrs.sources().graphics)
        } else if self.size.dimensions().1 != size.dimensions().1 {
            Some(attrs.sources().height)
        } else if self.size.dimensions().0 != size.dimensions().0 {
            Some(attrs.sources().width)
        } else {
            None
        };
        if let Some(source) = reset {
            self.mosaic = TeletextMosaic::default();
            self.separated = false;
            self.source = TeletextHeldMosaicSource::SpacingReset { source };
        }
        self.graphics = attrs.graphics();
        self.size = size;
        let control = match cell.kind() {
            TeletextBaseDisplayCellKind::SpacingControl { control, .. } => Some(control),
            _ => None,
        };
        if control == Some(TeletextBaseSpacingControl::HoldMosaics) {
            self.hold = Some(cell.position());
        }
        let result = self
            .hold
            .filter(|_| control.is_some() && self.graphics)
            .map(|hold| TeletextHeldMosaic {
                mosaic: self.mosaic,
                separated: self.separated,
                source: self.source,
                hold,
            });
        if self.graphics && cell.code() >= 0x20 && cell.code() & 0x20 != 0 {
            // G1 alphabetic codes40..5F never replace the held mosaic. Literal
            // G1 SPACE does: it is a real empty mosaic, not a hold substitution.
            self.mosaic = TeletextMosaic::new((cell.code() & 0x1f) | ((cell.code() & 0x40) >> 1))?;
            self.separated = attrs.separated();
            self.source = TeletextHeldMosaicSource::Mosaic {
                position: cell.position(),
            };
        }
        if control == Some(TeletextBaseSpacingControl::ReleaseMosaics) {
            self.hold = None;
        }
        Ok(result)
    }
}
