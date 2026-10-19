//! Native text rows and exact positioned Latin replacement glyphs.
//!
//! Derived G0/G2 intent uses the existing Latin region-zero tables. A later
//! composer must establish/restore repertoire and header-row colour context;
//! the legacy selector stream is not a context-free program to prepend.

use ersatzrs_domain::{
    TeletextCharsetPair, TeletextColour, TeletextColumnOperation, TeletextDiacriticIndex,
    TeletextEnhancementPosition, TeletextGlyphCode, TeletextHeight, TeletextPageDraft,
    TeletextPageKind, TeletextRunContent, TeletextSemanticService, TeletextStructureError,
    TeletextStyle, TELETEXT_COLUMNS, TELETEXT_ROWS,
};

use super::{charset, TeletextEncodingError};

pub(super) struct Layout {
    pub rows: [[u8; TELETEXT_COLUMNS]; TELETEXT_ROWS],
    base_glyphs: Vec<PositionedBaseGlyph>,
    physical_rows: [bool; TELETEXT_ROWS],
    text_charset: TextCharset,
}

#[derive(Clone, Copy)]
enum TextCharset {
    Latin(Option<u8>),
    Greek,
}

impl Layout {
    pub(super) fn encode_character(&self, character: char) -> Option<charset::Character> {
        match self.text_charset {
            TextCharset::Latin(Some(national)) => charset::encode(character, national),
            TextCharset::Latin(None) => charset::encode_without_national_subset(character),
            TextCharset::Greek => charset::encode_greek_g0(character),
        }
    }
}

/// One exact Western Latin replacement for a fallback cell, before composition.
/// These are derived glyphs, not an authored enhancement program or readiness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PositionedBaseGlyph {
    position: TeletextEnhancementPosition,
    glyph: BaseGlyph,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BaseGlyph {
    G2(TeletextGlyphCode),
    G0Diacritic {
        code: TeletextGlyphCode,
        diacritic: TeletextDiacriticIndex,
    },
}

impl PositionedBaseGlyph {
    pub(super) const fn position(self) -> TeletextEnhancementPosition {
        self.position
    }

    fn new(row: u8, column: u8, mode: u8, data: u8) -> Option<Self> {
        let position = TeletextEnhancementPosition::new(row, column).ok()?;
        let code = TeletextGlyphCode::new(data).ok()?;
        let glyph = match mode {
            0x0f => BaseGlyph::G2(code),
            0x10..=0x1f => BaseGlyph::G0Diacritic {
                code,
                diacritic: TeletextDiacriticIndex::new(mode & 15).ok()?,
            },
            _ => return None,
        };
        Some(Self { position, glyph })
    }

    fn legacy_triplet(self) -> u32 {
        // Exhaustive private glyph variants prevent unrelated authored modes
        // from being silently accepted or dropped by the legacy builder.
        let (mode, data) = match self.glyph {
            BaseGlyph::G2(code) => (0x0f, code.get()),
            BaseGlyph::G0Diacritic { code, diacritic } => (0x10 | diacritic.get(), code.get()),
        };
        triplet(self.position().column(), mode, data)
    }
}

impl From<&PositionedBaseGlyph> for TeletextColumnOperation {
    fn from(value: &PositionedBaseGlyph) -> Self {
        match value.glyph {
            BaseGlyph::G2(code) => Self::G2 { code },
            BaseGlyph::G0Diacritic { code, diacritic } => Self::G0Diacritic { code, diacritic },
        }
    }
}

impl Layout {
    /// Coded editor rows have no inferred original Unicode or mosaic intent.
    pub(super) fn is_physical_row(&self, row: u8) -> bool {
        self.physical_rows[usize::from(row)]
    }

    /// Sorted exact row/column replacements; unchanged fallback cells remain in rows.
    pub(super) fn base_glyphs(&self) -> &[PositionedBaseGlyph] {
        &self.base_glyphs
    }

    /// Legacy selectors and glyphs derived from the sole positioned glyph stream.
    /// The packet encoder separately appends its unchanged data-zero terminator.
    pub(super) fn legacy_enhancements(&self) -> Vec<u32> {
        let mut words = Vec::with_capacity(self.legacy_word_count());
        let mut previous_row = None;
        for glyph in self.base_glyphs() {
            let row = glyph.position().row();
            if previous_row != Some(row) {
                let (address, mode) = match row {
                    0 => (63, 7),
                    24 => (40, 4),
                    _ => (row + 40, 4),
                };
                words.push(triplet(address, mode, 0));
                previous_row = Some(row);
            }
            words.push(glyph.legacy_triplet());
        }
        words
    }

    fn legacy_word_count(&self) -> usize {
        let mut count = self.base_glyphs.len();
        let mut previous_row = None;
        for glyph in self.base_glyphs() {
            let row = glyph.position().row();
            if previous_row != Some(row) {
                count += 1;
                previous_row = Some(row);
            }
        }
        count
    }
}

pub(super) fn compile(page: &TeletextPageDraft) -> Result<Layout, TeletextEncodingError> {
    let layout = compile_for_composition(page)?;
    // Sixteen designation codes, thirteen triplets each, including termination.
    if layout.legacy_word_count() >= 16 * 13 {
        return Err(TeletextEncodingError::Enhancements {
            address: page.address,
        });
    }
    Ok(layout)
}

/// Build checked cells before the enhanced composer decides which replacement
/// glyphs survive. Its final merged program owns physical X26 capacity checks;
/// an authored compatible replacement can remove a redundant base event.
pub(super) fn compile_for_composition(
    page: &TeletextPageDraft,
) -> Result<Layout, TeletextEncodingError> {
    page.validate_structure()?;
    if page.national_option.get() >= 7 {
        return Err(TeletextEncodingError::NationalOption {
            address: page.address,
        });
    }
    compile_with_national_subset(page, Some(page.national_option.get()))
}

fn compile_with_national_subset(
    page: &TeletextPageDraft,
    national: Option<u8>,
) -> Result<Layout, TeletextEncodingError> {
    compile_with_text_charset(page, TextCharset::Latin(national))
}

fn compile_with_text_charset(
    page: &TeletextPageDraft,
    text_charset: TextCharset,
) -> Result<Layout, TeletextEncodingError> {
    page.validate_structure()?;
    let mut layout = Layout {
        rows: [[b' '; TELETEXT_COLUMNS]; TELETEXT_ROWS],
        base_glyphs: Vec::new(),
        physical_rows: [false; TELETEXT_ROWS],
        text_charset,
    };
    let mut rows: Vec<_> = page.rows.iter().collect();
    rows.sort_by_key(|row| row.row);
    for row in rows {
        let mut runs: Vec<_> = row.runs.iter().collect();
        runs.sort_by_key(|run| run.column);
        let mut state = State::default();
        let mut end = if row.row == 0 { 8 } else { 0 };
        for run in runs {
            let error = || TeletextEncodingError::ControlCells {
                address: page.address,
                row: row.row,
                column: run.column,
            };
            if page.kind != TeletextPageKind::FullPage && (row.row == 0 || !run.style.boxed) {
                return Err(TeletextEncodingError::HiddenContent {
                    address: page.address,
                    row: row.row,
                    column: run.column,
                });
            }
            if row.row == 0 && run.style.height != TeletextHeight::Normal {
                return Err(TeletextEncodingError::HeaderHeight {
                    address: page.address,
                });
            }
            if run.style.foreground == TeletextColour::Black {
                return Err(TeletextEncodingError::BlackForeground {
                    address: page.address,
                    row: row.row,
                    column: run.column,
                });
            }
            let (graphics, separated) = match &run.content {
                TeletextRunContent::Text { .. } => (false, false),
                TeletextRunContent::Mosaics { separated, .. } => (true, *separated),
            };
            let controls = state.transition(run.style, graphics, separated);
            let start = usize::from(run.column);
            if controls.len() > start - end {
                return Err(error());
            }
            layout.rows[usize::from(row.row)][start - controls.len()..start]
                .copy_from_slice(&controls);
            match &run.content {
                TeletextRunContent::Text { text } => {
                    for (column, character) in (run.column..).zip(text.chars()) {
                        let encoded = layout.encode_character(character).ok_or(
                            TeletextEncodingError::Character {
                                address: page.address,
                                row: row.row,
                                column,
                                codepoint: u32::from(character),
                            },
                        )?;
                        layout.rows[usize::from(row.row)][usize::from(column)] = encoded.fallback;
                        if let Some((mode, data)) = encoded.enhancement {
                            layout.base_glyphs.push(
                                PositionedBaseGlyph::new(row.row, column, mode, data).ok_or(
                                    TeletextEncodingError::Character {
                                        address: page.address,
                                        row: row.row,
                                        column,
                                        codepoint: u32::from(character),
                                    },
                                )?,
                            );
                        }
                        end = usize::from(column) + 1;
                    }
                }
                TeletextRunContent::Mosaics { cells, .. } => {
                    for (column, cell) in (run.column..).zip(cells) {
                        let bits = cell.blocks();
                        layout.rows[usize::from(row.row)][usize::from(column)] =
                            0x20 | (bits & 0x1f) | ((bits & 0x20) << 1);
                        end = usize::from(column) + 1;
                    }
                }
            }
        }
    }
    Ok(layout)
}

/// Apply exact owner-scoped physical rows to the same checked base used by
/// preview, enhanced composition and native packet assembly. Generated X26
/// replacements remain protected until their editor changes are independently
/// validated; identical fallback cells retain their original replacement.
pub(super) fn compile_for_service_composition(
    service: &TeletextSemanticService,
    page: &TeletextPageDraft,
) -> Result<Layout, TeletextEncodingError> {
    // Header option seven is reserved only in the legacy region-zero context.
    // An explicit assigned Latin primary ending in seven establishes its own
    // context; never substitute another national table for its Unicode text.
    let primary = service
        .effective_presentation(page.address)
        .and_then(|value| value.primary_designation().ok());
    let latin = primary.filter(|primary| primary.pair() == TeletextCharsetPair::LatinLatin);
    let assigned_latin_seven =
        page.national_option.get() == 7 && latin.is_some_and(|primary| primary.get() & 7 == 7);
    let mut layout = if let Some(primary) =
        latin.filter(|_| page.national_option.get() < 7 || assigned_latin_seven)
    {
        compile_with_national_subset(page, charset::unicode_national_subset(primary))?
    } else if page.national_option.get() == 7
        && primary.is_some_and(|primary| primary.get() == 0x37)
    {
        // Matching Greek context admits its actual native Unicode scalars.
        // Unsupported/cross-script input stays a located character error.
        compile_with_text_charset(page, TextCharset::Greek)?
    } else if page.national_option.get() == 7
        && primary.is_some_and(|primary| primary.get() & 7 == 7)
        && page.rows.iter().all(|row| {
            row.runs
                .iter()
                .all(|run| matches!(run.content, TeletextRunContent::Mosaics { .. }))
        })
    {
        // Exact coded rows and G1 mosaics require no Latin Unicode conversion.
        // Assigned Arabic header7 is not reserved region-zero7. Keep
        // unqualified logical text on the original explicit rejection path.
        compile_with_national_subset(page, None)?
    } else {
        compile_for_composition(page)?
    };
    let Some(overlay) = service.physical_base_overlay(page.address) else {
        return Ok(layout);
    };
    for row in overlay.rows() {
        for glyph in layout.base_glyphs() {
            let position = glyph.position();
            if position.row() == row.row()
                && row.cells()[usize::from(position.column())]
                    != layout.rows[usize::from(row.row())][usize::from(position.column())]
            {
                return Err(TeletextEncodingError::PhysicalBaseGeneratedGlyph {
                    address: page.address,
                    row: row.row(),
                    column: position.column(),
                });
            }
        }
        super::base_display::validate_physical_base_row(page, row.row(), row.cells())?;
        layout.rows[usize::from(row.row())] = *row.cells();
        layout.physical_rows[usize::from(row.row())] = true;
    }
    validate_merged_height(page, &layout)?;
    Ok(layout)
}

/// Keep the logical no-hidden-row invariant after every physical row has been
/// applied. Unchanged authored rows retain their content identity, including
/// explicit blank glyphs; a physical all-space row explicitly clears content.
fn validate_merged_height(
    page: &TeletextPageDraft,
    layout: &Layout,
) -> Result<(), TeletextEncodingError> {
    let content: [bool; TELETEXT_ROWS] = std::array::from_fn(|row| {
        if layout.physical_rows[row] {
            layout.rows[row].iter().any(|&code| code != b' ')
        } else {
            page.rows
                .iter()
                .any(|original| usize::from(original.row) == row && !original.runs.is_empty())
        }
    });
    for row in 0_u8..25 {
        if !layout.rows[usize::from(row)].contains(&0x0d) {
            continue;
        }
        let invalid_row = if row >= 23 {
            Some(row)
        } else if content[usize::from(row) + 1] {
            Some(row + 1)
        } else {
            None
        };
        if let Some(row) = invalid_row {
            return Err(TeletextStructureError::DoubleHeight {
                address: page.address,
                row,
            }
            .into());
        }
    }
    Ok(())
}

pub(super) const fn triplet(address: u8, mode: u8, data: u8) -> u32 {
    (address as u32) | ((mode as u32) << 6) | ((data as u32) << 11)
}

#[derive(Default)]
struct State {
    style: TeletextStyle,
    graphics: bool,
    separated: bool,
}

impl State {
    fn colour(&mut self, colour: TeletextColour, graphics: bool, output: &mut Vec<u8>) {
        output.push(colour_code(colour) | if graphics { 0x10 } else { 0 });
        self.style.foreground = colour;
        self.style.conceal = false;
        self.graphics = graphics;
    }

    fn transition(&mut self, target: TeletextStyle, graphics: bool, separated: bool) -> Vec<u8> {
        let mut output = Vec::new();
        if self.style.background != target.background {
            if target.background == TeletextColour::Black {
                output.push(0x1c);
            } else {
                if self.style.foreground != target.background {
                    self.colour(target.background, graphics, &mut output);
                }
                output.push(0x1d);
            }
        }
        if self.style.foreground != target.foreground
            || self.graphics != graphics
            || (self.style.conceal && !target.conceal)
        {
            self.colour(target.foreground, graphics, &mut output);
        }
        if self.style.height != target.height {
            output.push(if target.height == TeletextHeight::Double {
                0x0d
            } else {
                0x0c
            });
        }
        if self.style.boxed != target.boxed {
            if target.boxed {
                output.extend([0x0b, 0x0b]);
            } else {
                output.push(0x0a);
            }
        }
        if self.style.flash != target.flash {
            output.push(if target.flash { 0x08 } else { 0x09 });
        }
        if graphics && self.separated != separated {
            output.push(if separated { 0x1a } else { 0x19 });
        }
        if target.conceal && !self.style.conceal {
            output.push(0x18);
        }
        self.style = target;
        if graphics {
            self.separated = separated;
        }
        output
    }
}

const fn colour_code(colour: TeletextColour) -> u8 {
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
