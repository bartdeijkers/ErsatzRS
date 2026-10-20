//! Immutable coded-plane trace under the effective presentation, before object
//! display realization. Mapped colours and enhanced spacing responses are not a
//! selected Level 1/1.5 rendering; the original coded rows remain its input.
//!
//! This bounded value retains coded spacing cells, their decoded attributes and
//! separate authored intent. It is not a generic teletext renderer, visible-row
//! suppression result, designer document or publication/readiness capability.

use super::{
    TeletextColour, TeletextColourReference, TeletextEnhancementPosition,
    TeletextLevel25PresentationDefaults, TeletextMosaic, TeletextNationalOption,
    TeletextPageAddress, TeletextPageCharsetDesignation, TeletextStyle, TELETEXT_COLUMNS,
    TELETEXT_ROWS,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextSpacingTiming {
    SetAt,
    SetAfter,
}

/// Checked controls in authored fallback layouts and exact physical Base rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextBaseSpacingControl {
    AlphaColour { colour: TeletextColour },
    MosaicColour { colour: TeletextColour },
    Flash,
    Steady,
    EndBox,
    StartBox,
    NormalHeight,
    DoubleHeight,
    DoubleWidth,
    DoubleSize,
    Conceal,
    ContiguousMosaics,
    SeparatedMosaics,
    Escape,
    HoldMosaics,
    ReleaseMosaics,
    BlackBackground,
    NewBackground,
}

impl TeletextBaseSpacingControl {
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::AlphaColour { colour } => colour_index(colour),
            Self::MosaicColour { colour } => 0x10 | colour_index(colour),
            Self::Flash => 0x08,
            Self::Steady => 0x09,
            Self::EndBox => 0x0a,
            Self::StartBox => 0x0b,
            Self::NormalHeight => 0x0c,
            Self::DoubleHeight => 0x0d,
            Self::DoubleWidth => 0x0e,
            Self::DoubleSize => 0x0f,
            Self::Conceal => 0x18,
            Self::ContiguousMosaics => 0x19,
            Self::SeparatedMosaics => 0x1a,
            Self::Escape => 0x1b,
            Self::HoldMosaics => 0x1e,
            Self::ReleaseMosaics => 0x1f,
            Self::BlackBackground => 0x1c,
            Self::NewBackground => 0x1d,
        }
    }
    #[must_use]
    pub const fn timing(self) -> TeletextSpacingTiming {
        match self {
            Self::AlphaColour { .. }
            | Self::MosaicColour { .. }
            | Self::Flash
            | Self::EndBox
            | Self::StartBox
            | Self::Escape
            | Self::ReleaseMosaics
            | Self::DoubleHeight
            | Self::DoubleWidth
            | Self::DoubleSize => TeletextSpacingTiming::SetAfter,
            _ => TeletextSpacingTiming::SetAt,
        }
    }
}

const fn colour_index(colour: TeletextColour) -> u8 {
    match colour {
        TeletextColour::Black => 0,
        TeletextColour::Red => 1,
        TeletextColour::Green => 2,
        TeletextColour::Yellow => 3,
        TeletextColour::Blue => 4,
        TeletextColour::Magenta => 5,
        TeletextColour::Cyan => 6,
        TeletextColour::White => 7,
    }
}

/// Attribute origin. Set-after at column 39 starts at the row's end (40).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextBaseAttributeSource {
    RowDefault {
        row: u8,
        start_column: u8,
    },
    Spacing {
        position: TeletextEnhancementPosition,
        control: TeletextBaseSpacingControl,
        timing: TeletextSpacingTiming,
    },
}
impl TeletextBaseAttributeSource {
    #[must_use]
    pub const fn effective_start_column(self) -> u8 {
        match self {
            Self::RowDefault { start_column, .. } => start_column,
            Self::Spacing {
                position, timing, ..
            } => {
                position.column()
                    + match timing {
                        TeletextSpacingTiming::SetAt => 0,
                        TeletextSpacingTiming::SetAfter => 1,
                    }
            }
        }
    }
    fn validate(self, position: TeletextEnhancementPosition) -> bool {
        match self {
            Self::RowDefault { row, start_column } => {
                row == position.row() && start_column == if row == 0 { 8 } else { 0 }
            }
            Self::Spacing {
                position: origin,
                control,
                timing,
            } => {
                origin.row() == position.row()
                    && control.timing() == timing
                    && origin.column() >= if origin.row() == 0 { 8 } else { 0 }
                    && self.effective_start_column() <= position.column()
            }
        }
    }
}

/// Ordinary raw Level 1 indices remain independent of mapped Level 2.5 colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "spacing attributes are independent standard-defined flags, not exclusive states"
)]
pub struct TeletextBaseAttributeValues {
    pub raw_foreground: u8,
    pub raw_background: u8,
    pub foreground: TeletextColourReference,
    pub background: TeletextColourReference,
    pub graphics: bool,
    pub separated: bool,
    pub double_height: bool,
    pub double_width: bool,
    pub boxed: bool,
    pub flash: bool,
    pub conceal: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextBaseAttributeSources {
    pub foreground: TeletextBaseAttributeSource,
    pub background: TeletextBaseAttributeSource,
    pub graphics: TeletextBaseAttributeSource,
    pub separated: TeletextBaseAttributeSource,
    pub height: TeletextBaseAttributeSource,
    pub width: TeletextBaseAttributeSource,
    pub boxed: TeletextBaseAttributeSource,
    pub flash: TeletextBaseAttributeSource,
    pub conceal: TeletextBaseAttributeSource,
}
impl TeletextBaseAttributeSources {
    fn all(self) -> [TeletextBaseAttributeSource; 9] {
        [
            self.foreground,
            self.background,
            self.graphics,
            self.separated,
            self.height,
            self.width,
            self.boxed,
            self.flash,
            self.conceal,
        ]
    }
    fn validate(self, position: TeletextEnhancementPosition) -> bool {
        [
            self.foreground,
            self.background,
            self.graphics,
            self.separated,
            self.height,
            self.width,
            self.boxed,
            self.flash,
            self.conceal,
        ]
        .into_iter()
        .all(|source| source.validate(position))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextBaseDisplayAttributes {
    values: TeletextBaseAttributeValues,
    sources: TeletextBaseAttributeSources,
}
impl TeletextBaseDisplayAttributes {
    /// Check raw colour widths; the containing trace also checks source locations.
    /// # Errors
    /// Rejects a raw Level 1 colour outside its original three-bit index.
    pub fn new(
        values: TeletextBaseAttributeValues,
        sources: TeletextBaseAttributeSources,
    ) -> Result<Self, TeletextBaseDisplayValueError> {
        if values.raw_foreground > 7 || values.raw_background > 7 {
            return Err(TeletextBaseDisplayValueError::RawColour);
        }
        Ok(Self { values, sources })
    }
    #[must_use]
    pub const fn values(self) -> TeletextBaseAttributeValues {
        self.values
    }
    #[must_use]
    pub const fn sources(self) -> TeletextBaseAttributeSources {
        self.sources
    }
    #[must_use]
    pub const fn raw_foreground(self) -> u8 {
        self.values.raw_foreground
    }
    #[must_use]
    pub const fn raw_background(self) -> u8 {
        self.values.raw_background
    }
    #[must_use]
    pub const fn foreground(self) -> TeletextColourReference {
        self.values.foreground
    }
    #[must_use]
    pub const fn background(self) -> TeletextColourReference {
        self.values.background
    }
    #[must_use]
    pub const fn graphics(self) -> bool {
        self.values.graphics
    }
    #[must_use]
    pub const fn separated(self) -> bool {
        self.values.separated
    }
    #[must_use]
    pub const fn double_height(self) -> bool {
        self.values.double_height
    }
    #[must_use]
    pub const fn double_width(self) -> bool {
        self.values.double_width
    }
    #[must_use]
    pub const fn boxed(self) -> bool {
        self.values.boxed
    }
    #[must_use]
    pub const fn flash(self) -> bool {
        self.values.flash
    }
    #[must_use]
    pub const fn conceal(self) -> bool {
        self.values.conceal
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextBaseAuthoredContent {
    Text {
        character: char,
    },
    Mosaic {
        mosaic: TeletextMosaic,
        separated: bool,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextBaseAuthoredGlyph {
    position: TeletextEnhancementPosition,
    run_column: u8,
    style: TeletextStyle,
    content: TeletextBaseAuthoredContent,
}
impl TeletextBaseAuthoredGlyph {
    /// Retain exact intent rather than reverse-decoding a fallback byte.
    /// # Errors
    /// Rejects a run origin after its glyph or inside broadcast-owned header cells.
    pub fn new(
        position: TeletextEnhancementPosition,
        run_column: u8,
        style: TeletextStyle,
        content: TeletextBaseAuthoredContent,
    ) -> Result<Self, TeletextBaseDisplayValueError> {
        if run_column > position.column() || (position.row() == 0 && run_column < 8) {
            return Err(TeletextBaseDisplayValueError::AuthoredOrigin);
        }
        Ok(Self {
            position,
            run_column,
            style,
            content,
        })
    }
    #[must_use]
    pub const fn position(self) -> TeletextEnhancementPosition {
        self.position
    }
    #[must_use]
    pub const fn run_column(self) -> u8 {
        self.run_column
    }
    #[must_use]
    pub const fn style(self) -> TeletextStyle {
        self.style
    }
    #[must_use]
    pub const fn content(self) -> TeletextBaseAuthoredContent {
        self.content
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextBaseDisplayCellKind {
    /// Layout placeholders later replaced by Hamming-coded header fields.
    HeaderOwned,
    SpacingControl {
        control: TeletextBaseSpacingControl,
        timing: TeletextSpacingTiming,
    },
    Blank,
    Glyph,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextBaseDisplayCell {
    position: TeletextEnhancementPosition,
    code: u8,
    kind: TeletextBaseDisplayCellKind,
    authored: Option<TeletextBaseAuthoredGlyph>,
    attributes: TeletextBaseDisplayAttributes,
}
impl TeletextBaseDisplayCell {
    /// Validate coded role, original ownership and checked attribute origins.
    /// # Errors
    /// Rejects invalid codes, false header ownership, contradictory control roles,
    /// misplaced authored glyphs or attribute origins that have not taken effect.
    pub fn new(
        position: TeletextEnhancementPosition,
        code: u8,
        kind: TeletextBaseDisplayCellKind,
        authored: Option<TeletextBaseAuthoredGlyph>,
        attributes: TeletextBaseDisplayAttributes,
    ) -> Result<Self, TeletextBaseDisplayValueError> {
        if code > 127 {
            return Err(TeletextBaseDisplayValueError::Code);
        }
        let header = position.row() == 0 && position.column() < 8;
        if header != matches!(kind, TeletextBaseDisplayCellKind::HeaderOwned) {
            return Err(TeletextBaseDisplayValueError::HeaderOwnership);
        }
        if header {
            if code != b' ' || authored.is_some() {
                return Err(TeletextBaseDisplayValueError::HeaderOwnership);
            }
        } else {
            if !attributes.sources.validate(position) {
                return Err(TeletextBaseDisplayValueError::AttributeOrigin);
            }
            match kind {
                TeletextBaseDisplayCellKind::SpacingControl { control, timing }
                    if control.code() == code
                        && control.timing() == timing
                        && authored.is_none() => {}
                TeletextBaseDisplayCellKind::Blank if code == b' ' && authored.is_none() => {}
                TeletextBaseDisplayCellKind::Glyph if code >= 0x20 => {}
                _ => return Err(TeletextBaseDisplayValueError::CellRole),
            }
        }
        if authored.is_some_and(|source| source.position() != position) {
            return Err(TeletextBaseDisplayValueError::AuthoredOrigin);
        }
        Ok(Self {
            position,
            code,
            kind,
            authored,
            attributes,
        })
    }
    #[must_use]
    pub const fn position(self) -> TeletextEnhancementPosition {
        self.position
    }
    #[must_use]
    pub const fn code(self) -> u8 {
        self.code
    }
    #[must_use]
    pub const fn kind(self) -> TeletextBaseDisplayCellKind {
        self.kind
    }
    #[must_use]
    pub const fn authored(self) -> Option<TeletextBaseAuthoredGlyph> {
        self.authored
    }
    #[must_use]
    pub const fn attributes(self) -> TeletextBaseDisplayAttributes {
        self.attributes
    }
    /// The exact top cell of a two-row footprint; no lower-row suppression or
    /// clipping is performed here. Its size-control provenance remains separate.
    #[must_use]
    pub const fn double_height_origin(self) -> Option<TeletextEnhancementPosition> {
        if self.attributes.values.double_height {
            Some(self.position)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextBaseDisplayValueError {
    #[error("base display contains a non-seven-bit fallback code")]
    Code,
    #[error("base display raw colour must be between zero and seven")]
    RawColour,
    #[error("base display header cells have contradictory ownership")]
    HeaderOwnership,
    #[error("base display role does not match its coded cell")]
    CellRole,
    #[error("base display authored glyph origin does not match its cell")]
    AuthoredOrigin,
    #[error("base display attribute origin has not taken effect in this cell")]
    AttributeOrigin,
    #[error("base display array position does not match its cell")]
    CellPosition,
    #[error("base display primary/national/presentation identity disagrees")]
    Presentation,
}

/// Fixed, structurally checked 25 by 40 base trace; no external serde bypass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextBaseDisplay {
    owner: TeletextPageAddress,
    primary: TeletextPageCharsetDesignation,
    national_option: TeletextNationalOption,
    presentation: Option<TeletextLevel25PresentationDefaults>,
    cells: Box<[[TeletextBaseDisplayCell; TELETEXT_COLUMNS]; TELETEXT_ROWS]>,
}
impl TeletextBaseDisplay {
    /// Assemble a fixed trace after the transport adapter has decoded its actual
    /// emitted spacing subset. This validates record structure, not full rendering.
    /// # Errors
    /// Rejects reserved fallback national bits without an assigned matching
    /// presentation, conflicting presentation identity,
    /// cells placed at another row/column, fictitious coded attribute sources,
    /// or an unconfirmed paired Start Box origin in the fixed array.
    pub fn new(
        owner: TeletextPageAddress,
        primary: TeletextPageCharsetDesignation,
        national_option: TeletextNationalOption,
        presentation: Option<TeletextLevel25PresentationDefaults>,
        cells: Box<[[TeletextBaseDisplayCell; TELETEXT_COLUMNS]; TELETEXT_ROWS]>,
    ) -> Result<Self, TeletextBaseDisplayValueError> {
        let assigned_seven =
            national_option.get() == 7 && primary.get() & 7 == 7 && presentation.is_some();
        if (national_option.get() >= 7 && !assigned_seven)
            || presentation.map_or_else(
                || primary.get() != national_option.get(),
                |value| value.primary != primary,
            )
        {
            return Err(TeletextBaseDisplayValueError::Presentation);
        }
        let trace_cells = &cells;
        for (row, cells) in cells.iter().enumerate() {
            for (column, cell) in cells.iter().enumerate() {
                if usize::from(cell.position.row()) != row
                    || usize::from(cell.position.column()) != column
                {
                    return Err(TeletextBaseDisplayValueError::CellPosition);
                }
                if !matches!(cell.kind, TeletextBaseDisplayCellKind::HeaderOwned) {
                    validate_size_sources(cell.attributes)?;
                    for source in cell.attributes.sources.all() {
                        validate_coded_source(trace_cells, source)?;
                    }
                }
            }
        }
        Ok(Self {
            owner,
            primary,
            national_option,
            presentation,
            cells,
        })
    }
    #[must_use]
    pub const fn owner(&self) -> TeletextPageAddress {
        self.owner
    }
    #[must_use]
    pub const fn primary(&self) -> TeletextPageCharsetDesignation {
        self.primary
    }
    #[must_use]
    pub const fn national_option(&self) -> TeletextNationalOption {
        self.national_option
    }
    #[must_use]
    pub const fn presentation(&self) -> Option<TeletextLevel25PresentationDefaults> {
        self.presentation
    }
    #[must_use]
    pub fn cells(&self) -> &[[TeletextBaseDisplayCell; TELETEXT_COLUMNS]; TELETEXT_ROWS] {
        &self.cells
    }
    #[must_use]
    pub fn cell(&self, row: u8, column: u8) -> Option<&TeletextBaseDisplayCell> {
        self.cells.get(usize::from(row))?.get(usize::from(column))
    }
}

fn validate_coded_source(
    cells: &[[TeletextBaseDisplayCell; TELETEXT_COLUMNS]; TELETEXT_ROWS],
    source: TeletextBaseAttributeSource,
) -> Result<(), TeletextBaseDisplayValueError> {
    let TeletextBaseAttributeSource::Spacing {
        position,
        control,
        timing,
    } = source
    else {
        return Ok(());
    };
    let cell = cells[usize::from(position.row())][usize::from(position.column())];
    if cell.code != control.code()
        || cell.kind != (TeletextBaseDisplayCellKind::SpacingControl { control, timing })
    {
        return Err(TeletextBaseDisplayValueError::AttributeOrigin);
    }
    if control == TeletextBaseSpacingControl::StartBox {
        let next = cells[usize::from(position.row())].get(usize::from(position.column()) + 1);
        if !next.is_some_and(|cell| {
            cell.code == 0x0b
                && cell.kind == (TeletextBaseDisplayCellKind::SpacingControl { control, timing })
        }) {
            return Err(TeletextBaseDisplayValueError::AttributeOrigin);
        }
    }
    Ok(())
}

// Size controls reset both axes. Validate each independent field against its
// real control, so a valid coded origin cannot authenticate a forged size value.
fn validate_size_sources(
    attributes: TeletextBaseDisplayAttributes,
) -> Result<(), TeletextBaseDisplayValueError> {
    let sources = attributes.sources();
    for (value, source, vertical) in [
        (attributes.double_height(), sources.height, true),
        (attributes.double_width(), sources.width, false),
    ] {
        let expected = match source {
            TeletextBaseAttributeSource::RowDefault { .. } => false,
            TeletextBaseAttributeSource::Spacing { control, .. } => match control {
                TeletextBaseSpacingControl::NormalHeight => false,
                TeletextBaseSpacingControl::DoubleHeight => vertical,
                TeletextBaseSpacingControl::DoubleWidth => !vertical,
                TeletextBaseSpacingControl::DoubleSize => true,
                _ => return Err(TeletextBaseDisplayValueError::AttributeOrigin),
            },
        };
        if value != expected {
            return Err(TeletextBaseDisplayValueError::AttributeOrigin);
        }
    }
    Ok(())
}
