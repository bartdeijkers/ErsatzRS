//! Fixed twelve-by-ten glyph shapes, independent of cell geometry and visibility.
//!
//! Bitmap data is derived from the pinned MIT teletextimager font assets; see
//! `font_data/PROVENANCE.md`. No upstream decoder or renderer is executed here.
//! Callers retain original intent/disposition and the authoritative service.

use super::{
    TeletextCharsetPair, TeletextColumnOperation, TeletextDrcsBitmap, TeletextExecutedDrcsGlyph,
    TeletextG0Glyph, TeletextG0Repertoire, TeletextG2Glyph, TeletextG2Repertoire,
    TeletextGlyphIdentity, TeletextNationalSubset,
};

const TABLE_BYTES: usize = 96 * 10 * 2;
const MASKS: &[u8; 28 * TABLE_BYTES] = include_bytes!("font_data/masks.bin");

/// Monochrome mask: row zero is top, bit eleven is left. No colour or sizing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextGlyphMask([u16; 10]);

impl TeletextGlyphMask {
    /// A checked authored DRCS bitmap has exactly the same mask geometry.
    #[must_use]
    pub const fn from_drcs_bitmap(bitmap: TeletextDrcsBitmap) -> Self {
        Self(bitmap.get())
    }

    #[must_use]
    pub const fn rows(self) -> [u16; 10] {
        self.0
    }

    /// Chosen twelve-by-ten font policy: underline fills the bottom mask row.
    /// The caller decides D5 applicability before fragment sizing; this method
    /// neither classifies a glyph family nor interprets a spacing attribute.
    #[must_use]
    pub fn with_underline(mut self) -> Self {
        self.0[9] = 0x0fff;
        self
    }

    /// Pixel coordinates outside twelve columns or ten rows return `None`.
    #[must_use]
    pub fn pixel(self, column: u8, row: u8) -> Option<bool> {
        if column >= 12 || row >= 10 {
            return None;
        }
        Some(self.0[usize::from(row)] & (1 << (11 - column)) != 0)
    }
}

/// Shape failure retains the exact repertoire/code rather than guessing a font.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextGlyphMaskError {
    #[error("glyph code {code:#04x} lies outside the fixed table")]
    InvalidGlyphCode { code: u8 },
    #[error("national subset is not applicable to this G0 position")]
    InvalidNationalSubset,
    #[error("glyph {identity:?} is explicitly prohibited by the primary table")]
    ProhibitedTablePosition { identity: TeletextGlyphIdentity },
    #[error("glyph {identity:?} has no established printed assignment")]
    UnmarkedTableCell { identity: TeletextGlyphIdentity },
    #[error("composition of {base:?} and {mark:?} has no verified shape policy")]
    UnverifiedComposition {
        base: TeletextG0Glyph,
        mark: TeletextG2Glyph,
    },
    #[error("DRCS glyph requires its exact resolved bitmap record")]
    MissingDrcs,
    #[error("resolved DRCS record does not match the requested scope/subtable/slot")]
    DrcsReferenceMismatch,
    #[error("replacement is not a legacy Latin G2 or G0-with-diacritic operation")]
    UnsupportedReplacement,
}

/// Look up an exact checked glyph identity without changing its semantic priority.
///
/// The caller must also require assigned original intent; successful table lookup
/// does not certify a generation or resolve charset/C.2/C.3 pending state. Only
/// G1 mosaics use `separated_mosaics`; D5 underline belongs to the cell renderer.
/// DRCS uses a matching immutable resolved record from that same service.
/// # Errors
/// Rejects prohibited/unmarked positions, malformed identities, absent/mismatched
/// DRCS and unverified composites. Composition retains the exact current G0/G2;
/// its vertical fitting is a font choice, not lossless atlas reproduction or
/// certification of a reference decoder's glyph shapes.
pub fn resolve_teletext_glyph_mask(
    identity: TeletextGlyphIdentity,
    drcs: Option<&TeletextExecutedDrcsGlyph>,
    separated_mosaics: bool,
) -> Result<TeletextGlyphMask, TeletextGlyphMaskError> {
    match identity {
        TeletextGlyphIdentity::G0 { glyph } => g0_mask(glyph),
        TeletextGlyphIdentity::G2 { glyph } => g2_mask(glyph),
        TeletextGlyphIdentity::G1Mosaic { mosaic } => {
            let blocks = mosaic.blocks();
            let code = 0x20 | (blocks & 0x1f) | ((blocks & 0x20) << 1);
            table_mask(if separated_mosaics { 25 } else { 24 }, code)
        }
        TeletextGlyphIdentity::G3 { code } => {
            if matches!(code.get(), 0x6e | 0x6f | 0x7e | 0x7f) {
                return Err(TeletextGlyphMaskError::ProhibitedTablePosition { identity });
            }
            table_mask(26, code.get())
        }
        TeletextGlyphIdentity::Drcs { reference } => {
            let record = drcs.ok_or(TeletextGlyphMaskError::MissingDrcs)?;
            if record.reference() != reference {
                return Err(TeletextGlyphMaskError::DrcsReferenceMismatch);
            }
            Ok(TeletextGlyphMask::from_drcs_bitmap(record.bitmap()))
        }
        TeletextGlyphIdentity::Composite { base, mark } => composite_mask(base, mark),
        TeletextGlyphIdentity::AtSymbol => table_mask(0, 0x40),
        TeletextGlyphIdentity::Blank | TeletextGlyphIdentity::SpacingControl { .. } => {
            Ok(TeletextGlyphMask([0; 10]))
        }
    }
}

/// Resolve only operations emitted by the existing Latin fallback layout.
///
/// This does not insert a Local candidate: the composition owner associates the
/// resulting mask with its selected original Base candidate. It does not infer
/// another primary repertoire or apply national substitution to enhanced glyphs.
/// # Errors
/// Rejects other operations and unassigned table/composite positions.
pub fn resolve_teletext_latin_replacement_mask(
    operation: TeletextColumnOperation,
) -> Result<TeletextGlyphMask, TeletextGlyphMaskError> {
    resolve_teletext_glyph_mask(
        resolve_teletext_latin_replacement_identity(operation)?,
        None,
        false,
    )
}

/// Exact identity of an operation emitted by the existing Latin fallback layout.
/// This performs no national substitution, font lookup or display composition.
/// # Errors
/// Rejects operations outside the existing G2/diacritic replacement vocabulary.
pub fn resolve_teletext_latin_replacement_identity(
    operation: TeletextColumnOperation,
) -> Result<TeletextGlyphIdentity, TeletextGlyphMaskError> {
    resolve_teletext_replacement_identity(operation, TeletextCharsetPair::LatinLatin)
}

/// Exact coded identity of a generated replacement in its retained charset.
/// This performs no national substitution, normalization or assignment/font
/// qualification. The caller must retain the operation's authoritative context.
/// # Errors
/// Rejects operations outside the existing G2/diacritic replacement vocabulary.
pub fn resolve_teletext_replacement_identity(
    operation: TeletextColumnOperation,
    charset: TeletextCharsetPair,
) -> Result<TeletextGlyphIdentity, TeletextGlyphMaskError> {
    let (g0, g2) = super::glyph_intent::repertoires(charset);
    let identity = match operation {
        TeletextColumnOperation::G2 { code } => TeletextGlyphIdentity::G2 {
            glyph: TeletextG2Glyph {
                repertoire: g2,
                code: code.get(),
            },
        },
        TeletextColumnOperation::G0Diacritic { code, diacritic } => {
            let base = TeletextG0Glyph {
                repertoire: g0,
                code: code.get(),
                national_subset: None,
            };
            if diacritic.get() == 0 {
                if code.get() == 0x2a {
                    TeletextGlyphIdentity::AtSymbol
                } else {
                    TeletextGlyphIdentity::G0 { glyph: base }
                }
            } else {
                TeletextGlyphIdentity::Composite {
                    base,
                    mark: TeletextG2Glyph {
                        repertoire: g2,
                        code: 0x40 | diacritic.get(),
                    },
                }
            }
        }
        _ => return Err(TeletextGlyphMaskError::UnsupportedReplacement),
    };
    Ok(identity)
}

fn table_mask(table: usize, code: u8) -> Result<TeletextGlyphMask, TeletextGlyphMaskError> {
    if !(0x20..=0x7f).contains(&code) {
        return Err(TeletextGlyphMaskError::InvalidGlyphCode { code });
    }
    let offset = table * TABLE_BYTES + usize::from(code - 0x20) * 20;
    let mut rows = [0; 10];
    for (index, row) in rows.iter_mut().enumerate() {
        let position = offset + index * 2;
        *row = u16::from_le_bytes([MASKS[position], MASKS[position + 1]]);
    }
    Ok(TeletextGlyphMask(rows))
}

fn g0_mask(glyph: TeletextG0Glyph) -> Result<TeletextGlyphMask, TeletextGlyphMaskError> {
    let table = match glyph.repertoire() {
        TeletextG0Repertoire::Latin => 0,
        TeletextG0Repertoire::Cyrillic1 => 1,
        TeletextG0Repertoire::Cyrillic2 => 2,
        TeletextG0Repertoire::Cyrillic3 => 3,
        TeletextG0Repertoire::Greek => 4,
        TeletextG0Repertoire::Arabic => 5,
        TeletextG0Repertoire::Hebrew => 6,
    };
    let table = if let Some(subset) = glyph.national_subset() {
        if glyph.repertoire() != TeletextG0Repertoire::Latin
            || !matches!(glyph.code(), 0x23 | 0x24 | 0x40 | 0x5b..=0x60 | 0x7b..=0x7e)
        {
            return Err(TeletextGlyphMaskError::InvalidNationalSubset);
        }
        subset_table(subset)
    } else {
        table
    };
    table_mask(table, glyph.code())
}

const fn subset_table(subset: TeletextNationalSubset) -> usize {
    match subset {
        TeletextNationalSubset::CzechSlovak => 11,
        TeletextNationalSubset::English => 12,
        TeletextNationalSubset::Estonian => 13,
        TeletextNationalSubset::French => 14,
        TeletextNationalSubset::German => 15,
        TeletextNationalSubset::Italian => 16,
        TeletextNationalSubset::LatvianLithuanian => 17,
        TeletextNationalSubset::Polish => 18,
        TeletextNationalSubset::PortugueseSpanish => 19,
        TeletextNationalSubset::Romanian => 20,
        TeletextNationalSubset::SerbianCroatianSlovenian => 21,
        TeletextNationalSubset::SwedishFinnishHungarian => 22,
        TeletextNationalSubset::Turkish => 23,
    }
}

fn g2_mask(glyph: TeletextG2Glyph) -> Result<TeletextGlyphMask, TeletextGlyphMaskError> {
    let identity = TeletextGlyphIdentity::G2 { glyph };
    let table = match glyph.repertoire() {
        TeletextG2Repertoire::Latin => 7,
        TeletextG2Repertoire::Cyrillic => 8,
        TeletextG2Repertoire::Greek => 9,
        TeletextG2Repertoire::Arabic => 10,
    };
    match (glyph.repertoire(), glyph.code()) {
        (TeletextG2Repertoire::Latin, 0x59 | 0x5a | 0x5b | 0x65)
        | (TeletextG2Repertoire::Cyrillic, 0x26 | 0x28) => {
            return Err(TeletextGlyphMaskError::ProhibitedTablePosition { identity });
        }
        (
            TeletextG2Repertoire::Latin
            | TeletextG2Repertoire::Cyrillic
            | TeletextG2Repertoire::Greek,
            0x40,
        )
        | (TeletextG2Repertoire::Arabic, 0x7f) => {
            return Err(TeletextGlyphMaskError::UnmarkedTableCell { identity });
        }
        _ => {}
    }
    table_mask(table, glyph.code())
}

fn composite_mask(
    base: TeletextG0Glyph,
    mark: TeletextG2Glyph,
) -> Result<TeletextGlyphMask, TeletextGlyphMaskError> {
    if !matches!(
        mark.repertoire(),
        TeletextG2Repertoire::Latin | TeletextG2Repertoire::Cyrillic | TeletextG2Repertoire::Greek
    ) || base.national_subset().is_some()
        || super::glyph_intent::alphabetic(base) != Some(true)
        || !(0x41..=0x4f).contains(&mark.code())
    {
        return Err(TeletextGlyphMaskError::UnverifiedComposition { base, mark });
    }
    let accent = g2_mask(mark)?;
    let below = matches!(mark.code(), 0x49 | 0x4b | 0x4c | 0x4e);
    // EN 300 706 §15.6 and informative G.2 leave precise shapes to the decoder.
    // Below-letter marks retain the full current base. Above-letter Latin keeps
    // the existing font policy; ONLY its capitals have a supplied reduced font.
    let mut shaped = if below {
        g0_mask(base)?
    } else if base.repertoire() == TeletextG0Repertoire::Latin {
        table_mask(
            if base.code().is_ascii_uppercase() {
                27
            } else {
                0
            },
            base.code(),
        )?
    } else {
        fit_above_mark(g0_mask(base)?, accent)
            .ok_or(TeletextGlyphMaskError::UnverifiedComposition { base, mark })?
    };
    for (row, accent_row) in shaped.0.iter_mut().zip(accent.0) {
        *row |= accent_row;
    }
    Ok(shaped)
}

/// Fit the actual repertoire's occupied rows below the unchanged actual mark.
/// Translation is lossless when it fits. Otherwise row-union vertical fitting
/// deliberately changes this font's shape; no source row or horizontal bit is
/// discarded, and no Latin glyph replaces the original repertoire. The ring
/// occupies THREE top rows, so a fixed two-row reservation would be incorrect.
fn fit_above_mark(base: TeletextGlyphMask, mark: TeletextGlyphMask) -> Option<TeletextGlyphMask> {
    let first = base.0.iter().position(|&row| row != 0)?;
    let last = base.0.iter().rposition(|&row| row != 0)?;
    let start = mark.0.iter().rposition(|&row| row != 0)? + 1;
    let available = 10_usize.checked_sub(start).filter(|&height| height > 0)?;
    let height = last - first + 1;
    let mut fitted = TeletextGlyphMask([0; 10]);
    for (index, &row) in base.0[first..=last].iter().enumerate() {
        let target = if height <= available {
            index
        } else {
            index * available / height
        };
        fitted.0[start + target] |= row;
    }
    Some(fitted)
}
