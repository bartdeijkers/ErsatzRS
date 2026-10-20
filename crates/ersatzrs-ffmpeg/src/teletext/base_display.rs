//! Decode the actual authored Layout spacing subset into a bounded base trace.
//!
//! Original Unicode/mosaic intent remains separate from its coded fallback and
//! later generated X26 replacement. This does not resolve enhanced objects,
//! side panels, full repertoire rendering, lower-row suppression or readiness.

use ersatzrs_domain::{
    TeletextBaseAttributeSource, TeletextBaseAttributeSources, TeletextBaseAttributeValues,
    TeletextBaseAuthoredContent, TeletextBaseAuthoredGlyph, TeletextBaseDisplay,
    TeletextBaseDisplayAttributes, TeletextBaseDisplayCell, TeletextBaseDisplayCellKind,
    TeletextBaseDisplayValueError, TeletextBaseMosaicState, TeletextBaseSpacingControl,
    TeletextCharsetValueError, TeletextColour, TeletextColourReference,
    TeletextEffectivePresentation, TeletextEnhancementPosition, TeletextPageAddress,
    TeletextPageDraft, TeletextPageKind, TeletextRunContent, TeletextSpacingTiming,
    TELETEXT_COLUMNS, TELETEXT_ROWS,
};

use super::{layout::Layout, TeletextColourContext, TeletextEncodingError};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TeletextBaseDisplayFailure {
    #[error("base display colour context belongs to {actual:?}")]
    ContextOwner { actual: TeletextPageAddress },
    #[error("header fallback national bits disagree with the original page")]
    HeaderNational,
    #[error("fallback layout has unsupported spacing code {code:02X}")]
    UnsupportedControl { code: u8 },
    #[error("Base ESC requires an explicit second-G0 declaration; receiver-dependent Header/POP context is not inferred")]
    DecoderDependentSecondary,
    #[error("authored glyph expects fallback {expected:02X}, found {actual:02X}")]
    FallbackMismatch { expected: u8, actual: u8 },
    #[error("bounded base display array has an inconsistent shape")]
    Shape,
    #[error(transparent)]
    Layout(#[from] TeletextEncodingError),
    #[error(transparent)]
    Value(#[from] TeletextBaseDisplayValueError),
    #[error(transparent)]
    Charset(#[from] TeletextCharsetValueError),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("teletext page {owner:?}, cell {position:?}: {reason}")]
pub struct TeletextBaseDisplayError {
    pub owner: TeletextPageAddress,
    pub position: Option<TeletextEnhancementPosition>,
    pub reason: TeletextBaseDisplayFailure,
}

/// Consume the already checked actual layout belonging to this page. Header
/// placeholders stay broadcast-owned; decoding starts at column eight. Row
/// defaults reset raw spacing state and materialize mapped colours separately.
/// New Background uses the current raw foreground index in the background CLUT;
/// implicit/Black Background substitution uses original resolved row colour.
/// This is the coded plane interpreted under the effective enhanced presentation,
/// not a selected Level 1/1.5 renderer. Original bytes remain the fallback input.
/// The fixed trace labels size origins but does not suppress lower rows.
pub(super) fn trace_teletext_base_display(
    page: &TeletextPageDraft,
    layout: &Layout,
    presentation: TeletextEffectivePresentation<'_>,
    colours: &TeletextColourContext,
) -> Result<TeletextBaseDisplay, TeletextBaseDisplayError> {
    let owner = page.address;
    page.validate_structure()
        .map_err(|reason| error(owner, None, TeletextEncodingError::Structure(reason).into()))?;
    if colours.owner() != owner {
        return Err(error(
            owner,
            None,
            TeletextBaseDisplayFailure::ContextOwner {
                actual: colours.owner(),
            },
        ));
    }
    if matches!(presentation, TeletextEffectivePresentation::Header { national_option }
        | TeletextEffectivePresentation::MotFallback { national_option, .. }
        if national_option != page.national_option)
    {
        return Err(error(
            owner,
            None,
            TeletextBaseDisplayFailure::HeaderNational,
        ));
    }
    let primary = presentation
        .primary_designation()
        .map_err(|reason| error(owner, None, reason.into()))?;
    if !matches!(
        presentation,
        TeletextEffectivePresentation::Extension { .. }
    ) {
        for row in 0_u8..25 {
            for column in (if row == 0 { 8 } else { 0 })..40 {
                if layout.rows[usize::from(row)][usize::from(column)] == 0x1b {
                    return Err(error(
                        owner,
                        TeletextEnhancementPosition::new(row, column).ok(),
                        TeletextBaseDisplayFailure::DecoderDependentSecondary,
                    ));
                }
            }
        }
    }
    let declaration = match presentation {
        TeletextEffectivePresentation::Extension { defaults, .. }
        | TeletextEffectivePresentation::MotFallback { defaults, .. } => Some(*defaults),
        TeletextEffectivePresentation::Header { .. } => None,
    };
    let authored = authored_cells(page, layout)?;
    let mut rows = Vec::with_capacity(TELETEXT_ROWS);
    for row in 0_u8..25 {
        let row_colour = colours
            .row_colour(row)
            .ok_or_else(|| error(owner, None, TeletextBaseDisplayFailure::Shape))?;
        let mut state = RowState::new(
            row,
            declaration.map_or((0, 0), |value| value.remapping.cluts()),
            declaration.is_some_and(|value| value.black_background_substitution),
            row_colour,
        );
        let mut cells = Vec::with_capacity(TELETEXT_COLUMNS);
        for column in 0_u8..40 {
            let position = TeletextEnhancementPosition::new(row, column)
                .map_err(|_| error(owner, None, TeletextBaseDisplayFailure::Shape))?;
            cells.push(trace_cell(
                owner,
                position,
                layout.rows[usize::from(row)][usize::from(column)],
                authored[usize::from(row)][usize::from(column)],
                &layout.rows[usize::from(row)],
                &mut state,
            )?);
        }
        rows.push(
            cells
                .try_into()
                .map_err(|_| error(owner, None, TeletextBaseDisplayFailure::Shape))?,
        );
    }
    let cells = rows
        .into_boxed_slice()
        .try_into()
        .map_err(|_| error(owner, None, TeletextBaseDisplayFailure::Shape))?;
    TeletextBaseDisplay::new(owner, primary, page.national_option, declaration, cells)
        .map_err(|reason| error(owner, None, reason.into()))
}

fn error(
    owner: TeletextPageAddress,
    position: Option<TeletextEnhancementPosition>,
    reason: TeletextBaseDisplayFailure,
) -> TeletextBaseDisplayError {
    TeletextBaseDisplayError {
        owner,
        position,
        reason,
    }
}

type AuthoredCells = [[Option<TeletextBaseAuthoredGlyph>; TELETEXT_COLUMNS]; TELETEXT_ROWS];

fn authored_cells(
    page: &TeletextPageDraft,
    layout: &Layout,
) -> Result<Box<AuthoredCells>, TeletextBaseDisplayError> {
    let mut cells: Box<AuthoredCells> = vec![[None; TELETEXT_COLUMNS]; TELETEXT_ROWS]
        .into_boxed_slice()
        .try_into()
        .map_err(|_| error(page.address, None, TeletextBaseDisplayFailure::Shape))?;
    for row in &page.rows {
        if layout.is_physical_row(row.row) {
            continue;
        }
        for run in &row.runs {
            match &run.content {
                TeletextRunContent::Text { text } => {
                    for (column, character) in (run.column..).zip(text.chars()) {
                        let encoded = layout
                            .encode_character_at(row.row, column, character)
                            .ok_or_else(|| {
                                error(
                                    page.address,
                                    None,
                                    TeletextEncodingError::Character {
                                        address: page.address,
                                        row: row.row,
                                        column,
                                        codepoint: u32::from(character),
                                    }
                                    .into(),
                                )
                            })?;
                        insert_authored(
                            page,
                            layout,
                            &mut cells,
                            AuthoredInput {
                                position: (row.row, column),
                                run_column: run.column,
                                style: run.style,
                                content: TeletextBaseAuthoredContent::Text { character },
                                code: encoded.fallback,
                            },
                        )?;
                    }
                }
                TeletextRunContent::Mosaics {
                    cells: mosaics,
                    separated,
                } => {
                    for (column, mosaic) in (run.column..).zip(mosaics) {
                        let bits = mosaic.blocks();
                        let code = 0x20 | (bits & 31) | ((bits & 32) << 1);
                        insert_authored(
                            page,
                            layout,
                            &mut cells,
                            AuthoredInput {
                                position: (row.row, column),
                                run_column: run.column,
                                style: run.style,
                                content: TeletextBaseAuthoredContent::Mosaic {
                                    mosaic: *mosaic,
                                    separated: *separated,
                                },
                                code,
                            },
                        )?;
                    }
                }
            }
        }
    }
    Ok(cells)
}

#[derive(Clone, Copy)]
struct AuthoredInput {
    position: (u8, u8),
    run_column: u8,
    style: ersatzrs_domain::TeletextStyle,
    content: TeletextBaseAuthoredContent,
    code: u8,
}

fn insert_authored(
    page: &TeletextPageDraft,
    layout: &Layout,
    cells: &mut AuthoredCells,
    input: AuthoredInput,
) -> Result<(), TeletextBaseDisplayError> {
    let position = TeletextEnhancementPosition::new(input.position.0, input.position.1)
        .map_err(|_| error(page.address, None, TeletextBaseDisplayFailure::Shape))?;
    let actual = layout.rows[usize::from(position.row())][usize::from(position.column())];
    if actual != input.code {
        return Err(error(
            page.address,
            Some(position),
            TeletextBaseDisplayFailure::FallbackMismatch {
                expected: input.code,
                actual,
            },
        ));
    }
    let glyph =
        TeletextBaseAuthoredGlyph::new(position, input.run_column, input.style, input.content)
            .map_err(|reason| error(page.address, Some(position), reason.into()))?;
    cells[usize::from(position.row())][usize::from(position.column())] = Some(glyph);
    Ok(())
}

fn trace_cell(
    owner: TeletextPageAddress,
    position: TeletextEnhancementPosition,
    code: u8,
    authored: Option<TeletextBaseAuthoredGlyph>,
    row: &[u8; TELETEXT_COLUMNS],
    state: &mut RowState,
) -> Result<TeletextBaseDisplayCell, TeletextBaseDisplayError> {
    let control = if code < 0x20 && !(position.row() == 0 && position.column() < 8) {
        Some(control(code).ok_or_else(|| {
            error(
                owner,
                Some(position),
                TeletextBaseDisplayFailure::UnsupportedControl { code },
            )
        })?)
    } else {
        None
    };
    let source = control.map(|control| TeletextBaseAttributeSource::Spacing {
        position,
        control,
        timing: control.timing(),
    });
    apply_before(state, control.zip(source));
    let attributes = state
        .attributes()
        .map_err(|reason| error(owner, Some(position), reason.into()))?;
    let kind = if position.row() == 0 && position.column() < 8 {
        TeletextBaseDisplayCellKind::HeaderOwned
    } else if let Some(control) = control {
        TeletextBaseDisplayCellKind::SpacingControl {
            control,
            timing: control.timing(),
        }
    } else if code == b' ' && authored.is_none() {
        TeletextBaseDisplayCellKind::Blank
    } else {
        TeletextBaseDisplayCellKind::Glyph
    };
    let cell = TeletextBaseDisplayCell::new(position, code, kind, authored, attributes)
        .map_err(|reason| error(owner, Some(position), reason.into()))?;
    apply_after(state, control.zip(source), row, position.column());
    Ok(cell)
}

fn apply_before(
    state: &mut RowState,
    control: Option<(TeletextBaseSpacingControl, TeletextBaseAttributeSource)>,
) {
    if let Some((control, source)) = control {
        if control.timing() == TeletextSpacingTiming::SetAt {
            state.apply(control, source);
        }
    }
}

fn apply_after(
    state: &mut RowState,
    control: Option<(TeletextBaseSpacingControl, TeletextBaseAttributeSource)>,
    row: &[u8; TELETEXT_COLUMNS],
    column: u8,
) {
    if let Some((control, source)) = control {
        if control.timing() == TeletextSpacingTiming::SetAfter
            && (control != TeletextBaseSpacingControl::StartBox
                || row.get(usize::from(column) + 1) == Some(&0x0b))
        {
            state.apply(control, source);
        }
    }
}

/// Check the same current spacing subset and state timing before either
/// packet assembly or preview. Physical rows do not bypass header or caption
/// visibility rules merely because they contain no original Unicode runs.
pub(super) fn validate_physical_base_row(
    page: &TeletextPageDraft,
    row: u8,
    cells: &[u8; TELETEXT_COLUMNS],
) -> Result<(), TeletextEncodingError> {
    let start = if row == 0 { 8_u8 } else { 0 };
    let mut state = RowState::new(row, (0, 0), false, TeletextColourReference::BLACK);
    let mut mosaics = TeletextBaseMosaicState::default();
    for column in start..40 {
        let code = cells[usize::from(column)];
        let invalid = || TeletextEncodingError::PhysicalBaseControl {
            address: page.address,
            row,
            column,
            code,
        };
        let position = TeletextEnhancementPosition::new(row, column).map_err(|_| invalid())?;
        let cell = trace_cell(page.address, position, code, None, cells, &mut state)
            .map_err(|_| invalid())?;
        if row == 0 && matches!(code, 0x0d | 0x0f) {
            return Err(TeletextEncodingError::HeaderHeight {
                address: page.address,
            });
        }
        let held = mosaics.advance(cell).map_err(|_| invalid())?;
        let content = code > b' ' || held.is_some_and(|held| held.mosaic().blocks() != 0);
        if content
            && page.kind != TeletextPageKind::FullPage
            && (row == 0 || !cell.attributes().boxed())
        {
            return Err(TeletextEncodingError::HiddenContent {
                address: page.address,
                row,
                column,
            });
        }
    }
    Ok(())
}

fn control(code: u8) -> Option<TeletextBaseSpacingControl> {
    use TeletextBaseSpacingControl as C;
    let colour = |index| match index {
        0 => Some(TeletextColour::Black),
        1 => Some(TeletextColour::Red),
        2 => Some(TeletextColour::Green),
        3 => Some(TeletextColour::Yellow),
        4 => Some(TeletextColour::Blue),
        5 => Some(TeletextColour::Magenta),
        6 => Some(TeletextColour::Cyan),
        7 => Some(TeletextColour::White),
        _ => None,
    };
    Some(match code {
        0..=7 => C::AlphaColour {
            colour: colour(code)?,
        },
        0x10..=0x17 => C::MosaicColour {
            colour: colour(code & 7)?,
        },
        0x08 => C::Flash,
        0x09 => C::Steady,
        0x0a => C::EndBox,
        0x0b => C::StartBox,
        0x0c => C::NormalHeight,
        0x0d => C::DoubleHeight,
        0x0e => C::DoubleWidth,
        0x0f => C::DoubleSize,
        0x18 => C::Conceal,
        0x19 => C::ContiguousMosaics,
        0x1a => C::SeparatedMosaics,
        0x1b => C::Escape,
        0x1c => C::BlackBackground,
        0x1d => C::NewBackground,
        0x1e => C::HoldMosaics,
        0x1f => C::ReleaseMosaics,
        _ => return None,
    })
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "the row tracks independent standard-defined spacing flags and substitution policy"
)]
struct RowState {
    raw_foreground: u8,
    raw_background: u8,
    graphics: bool,
    separated: bool,
    double_height: bool,
    double_width: bool,
    boxed: bool,
    flash: bool,
    conceal: bool,
    sources: TeletextBaseAttributeSources,
    cluts: (u8, u8),
    black_substitution: bool,
    row_colour: TeletextColourReference,
}
impl RowState {
    fn new(
        row: u8,
        cluts: (u8, u8),
        black_substitution: bool,
        row_colour: TeletextColourReference,
    ) -> Self {
        let default = TeletextBaseAttributeSource::RowDefault {
            row,
            start_column: if row == 0 { 8 } else { 0 },
        };
        Self {
            raw_foreground: 7,
            raw_background: 0,
            graphics: false,
            separated: false,
            double_height: false,
            double_width: false,
            boxed: false,
            flash: false,
            conceal: false,
            sources: TeletextBaseAttributeSources {
                foreground: default,
                background: default,
                graphics: default,
                separated: default,
                height: default,
                width: default,
                boxed: default,
                flash: default,
                conceal: default,
            },
            cluts,
            black_substitution,
            row_colour,
        }
    }
    fn attributes(&self) -> Result<TeletextBaseDisplayAttributes, TeletextBaseDisplayValueError> {
        let foreground = TeletextColourReference::new(self.cluts.0 * 8 + self.raw_foreground)
            .map_err(|_| TeletextBaseDisplayValueError::RawColour)?;
        let substitute = self.black_substitution
            && !matches!(
                self.sources.background,
                TeletextBaseAttributeSource::Spacing {
                    control: TeletextBaseSpacingControl::NewBackground,
                    ..
                }
            );
        let background = if substitute {
            self.row_colour
        } else {
            TeletextColourReference::new(self.cluts.1 * 8 + self.raw_background)
                .map_err(|_| TeletextBaseDisplayValueError::RawColour)?
        };
        TeletextBaseDisplayAttributes::new(
            TeletextBaseAttributeValues {
                raw_foreground: self.raw_foreground,
                raw_background: self.raw_background,
                foreground,
                background,
                graphics: self.graphics,
                separated: self.separated,
                double_height: self.double_height,
                double_width: self.double_width,
                boxed: self.boxed,
                flash: self.flash,
                conceal: self.conceal,
            },
            self.sources,
        )
    }
    fn apply(&mut self, control: TeletextBaseSpacingControl, source: TeletextBaseAttributeSource) {
        use TeletextBaseSpacingControl as C;
        match control {
            // The separate charset execution retains the row-local G0 toggle.
            C::Escape | C::HoldMosaics | C::ReleaseMosaics => {}
            C::AlphaColour { .. } | C::MosaicColour { .. } => {
                self.raw_foreground = control.code() & 7;
                self.graphics = matches!(control, C::MosaicColour { .. });
                self.conceal = false;
                self.sources.foreground = source;
                self.sources.graphics = source;
                self.sources.conceal = source;
            }
            C::BlackBackground => {
                self.raw_background = 0;
                self.sources.background = source;
            }
            C::NewBackground => {
                self.raw_background = self.raw_foreground;
                self.sources.background = source;
            }
            C::Flash | C::Steady => {
                self.flash = control == C::Flash;
                self.sources.flash = source;
            }
            C::StartBox | C::EndBox => {
                self.boxed = control == C::StartBox;
                self.sources.boxed = source;
            }
            C::NormalHeight | C::DoubleHeight | C::DoubleWidth | C::DoubleSize => {
                self.double_height = matches!(control, C::DoubleHeight | C::DoubleSize);
                self.double_width = matches!(control, C::DoubleWidth | C::DoubleSize);
                self.sources.height = source;
                self.sources.width = source;
            }
            C::Conceal => {
                self.conceal = true;
                self.sources.conceal = source;
            }
            C::ContiguousMosaics | C::SeparatedMosaics => {
                self.separated = control == C::SeparatedMosaics;
                self.sources.separated = source;
            }
        }
    }
}
