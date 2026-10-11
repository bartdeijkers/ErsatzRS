//! Finite identity-preserving national-option conversions, EN 300 706 tables 35–37/46.
//! Operands are literal primary-table identities, not Unicode/font comparisons.

use super::{
    TeletextColumnOperation, TeletextDiacriticIndex, TeletextEnhancementCharsetDesignation,
    TeletextG0Glyph, TeletextG0Repertoire, TeletextG2Glyph, TeletextG2Repertoire,
    TeletextGlyphCode, TeletextGlyphIdentity, TeletextNationalSubset,
};

/// No guessed replacement is emitted for an unsupported or malformed identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextNationalConversionError {
    #[error("glyph identity is not a national-option G0 glyph: {identity:?}")]
    NonNationalIdentity { identity: TeletextGlyphIdentity },
    #[error("national-option glyph has an invalid repertoire or position: {glyph:?}")]
    InvalidNationalIdentity { glyph: TeletextG0Glyph },
    /// Table 36 Polish 5/B and Turkish 2/3 have no proven operand.
    #[error("no proven national conversion for {subset:?} at {code:02X}")]
    UnsupportedIdentity {
        subset: TeletextNationalSubset,
        code: u8,
    },
    /// Romanian 5/D and 7/D do not yet have a proven breve/caron distinction.
    #[error("national conversion mark remains unproven for {subset:?} at {code:02X}")]
    UnprovenRecipe {
        subset: TeletextNationalSubset,
        code: u8,
    },
    #[error("national conversion table contains an invalid checked operand")]
    InvalidRecipe,
}

/// Exact original identity and its explicit enhanced representation.
/// This establishes only this finite table relation, not mask, page or client
/// equivalence. The caller still checks repertoire-pair budgets and execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextNationalConversion {
    original: TeletextGlyphIdentity,
    designation: TeletextEnhancementCharsetDesignation,
    operation: TeletextColumnOperation,
    result: TeletextGlyphIdentity,
}
impl TeletextNationalConversion {
    #[must_use]
    pub const fn original(self) -> TeletextGlyphIdentity {
        self.original
    }
    #[must_use]
    pub const fn designation(self) -> TeletextEnhancementCharsetDesignation {
        self.designation
    }
    #[must_use]
    pub const fn operation(self) -> TeletextColumnOperation {
        self.operation
    }
    #[must_use]
    pub const fn result(self) -> TeletextGlyphIdentity {
        self.result
    }
}

/// Resolve one Table 36 identity without national substitution of the result.
/// Operands explicitly select Latin G0/Latin G2 pair 0, except English 7/C,
/// which selects Hebrew G0/Arabic G2 pair 55. Both count toward pair admission.
/// # Errors
/// Rejects non-national identities, non-Latin/malformed national positions, and
/// the two glyphs without proven operands and two ambiguous Romanian marks.
pub fn resolve_teletext_national_conversion(
    original: TeletextGlyphIdentity,
) -> Result<TeletextNationalConversion, TeletextNationalConversionError> {
    let TeletextGlyphIdentity::G0 { glyph } = original else {
        return Err(TeletextNationalConversionError::NonNationalIdentity { identity: original });
    };
    let Some(subset) = glyph.national_subset() else {
        return Err(TeletextNationalConversionError::NonNationalIdentity { identity: original });
    };
    let slot = NATIONAL_CODES.iter().position(|&code| code == glyph.code());
    let Some(slot) = slot.filter(|_| glyph.repertoire() == TeletextG0Repertoire::Latin) else {
        return Err(TeletextNationalConversionError::InvalidNationalIdentity { glyph });
    };
    let recipe = recipes(subset)[slot];
    if matches!(recipe, Recipe::U) {
        return Err(TeletextNationalConversionError::UnsupportedIdentity {
            subset,
            code: glyph.code(),
        });
    }
    if matches!(recipe, Recipe::P) {
        return Err(TeletextNationalConversionError::UnprovenRecipe {
            subset,
            code: glyph.code(),
        });
    }
    let (operation, result) = operand(recipe)?;
    let designation =
        TeletextEnhancementCharsetDesignation::new(if matches!(recipe, Recipe::H(_)) {
            0x55
        } else {
            0
        })
        .map_err(|_| TeletextNationalConversionError::InvalidRecipe)?;
    Ok(TeletextNationalConversion {
        original,
        designation,
        operation,
        result,
    })
}

const NATIONAL_CODES: [u8; 13] = [
    0x23, 0x24, 0x40, 0x5b, 0x5c, 0x5d, 0x5e, 0x5f, 0x60, 0x7b, 0x7c, 0x7d, 0x7e,
];

#[derive(Clone, Copy)]
enum Recipe {
    G(u8),
    S(u8),
    D(u8, u8),
    H(u8),
    U,
    P,
}

fn operand(
    recipe: Recipe,
) -> Result<(TeletextColumnOperation, TeletextGlyphIdentity), TeletextNationalConversionError> {
    let raw_code = match recipe {
        Recipe::G(code) | Recipe::S(code) | Recipe::D(_, code) | Recipe::H(code) => code,
        Recipe::U | Recipe::P => return Err(TeletextNationalConversionError::InvalidRecipe),
    };
    let code = TeletextGlyphCode::new(raw_code)
        .map_err(|_| TeletextNationalConversionError::InvalidRecipe)?;
    let base = TeletextG0Glyph {
        repertoire: if matches!(recipe, Recipe::H(_)) {
            TeletextG0Repertoire::Hebrew
        } else {
            TeletextG0Repertoire::Latin
        },
        code: raw_code,
        national_subset: None,
    };
    Ok(match recipe {
        Recipe::G(_) | Recipe::H(_) => (
            TeletextColumnOperation::G0 { code },
            TeletextGlyphIdentity::G0 { glyph: base },
        ),
        Recipe::S(_) => (
            TeletextColumnOperation::G2 { code },
            TeletextGlyphIdentity::G2 {
                glyph: TeletextG2Glyph {
                    repertoire: TeletextG2Repertoire::Latin,
                    code: raw_code,
                },
            },
        ),
        Recipe::D(index, _) => {
            let index = TeletextDiacriticIndex::new(index)
                .map_err(|_| TeletextNationalConversionError::InvalidRecipe)?;
            (
                TeletextColumnOperation::G0Diacritic {
                    code,
                    diacritic: index,
                },
                TeletextGlyphIdentity::Composite {
                    base,
                    mark: TeletextG2Glyph {
                        repertoire: TeletextG2Repertoire::Latin,
                        code: 0x40 | index.get(),
                    },
                },
            )
        }
        Recipe::U | Recipe::P => return Err(TeletextNationalConversionError::InvalidRecipe),
    })
}

// Printed Table 36 p107, read against Latin G0 Table 35 p106 and G2 Table 37
// p108. Romanian 5/E is I+circumflex, not a cached decoder's acute mapping.
// Table 46 p117 assigns the same double vertical line at Hebrew G0 7/C.
// U retains Polish stroked capital Z and Turkish TL as unsupported; no
// approximate symbol or resource is silently substituted.
use Recipe::{D, G, H, P, S, U};

const RECIPES: [[Recipe; 13]; 13] = [
    [
        S(0x23),
        S(0x24),
        G(0x40),
        S(0x2c),
        S(0x3d),
        S(0x2e),
        S(0x2d),
        G(0x23),
        S(0x50),
        S(0x3c),
        H(0x7c),
        S(0x3e),
        S(0x38),
    ],
    [
        G(0x23),
        S(0x24),
        S(0x27),
        D(0x8, 0x41),
        D(0x8, 0x4f),
        D(0x8, 0x55),
        G(0x5e),
        G(0x5f),
        S(0x30),
        D(0x8, 0x61),
        D(0x8, 0x6f),
        D(0x8, 0x75),
        S(0x7b),
    ],
    [
        G(0x23),
        G(0x24),
        D(0x2, 0x45),
        D(0x8, 0x41),
        D(0x8, 0x4f),
        D(0xa, 0x41),
        D(0x8, 0x55),
        G(0x5f),
        D(0x2, 0x65),
        D(0x8, 0x61),
        D(0x8, 0x6f),
        D(0xa, 0x61),
        D(0x8, 0x75),
    ],
    [
        S(0x23),
        S(0x24),
        D(0x2, 0x65),
        S(0x30),
        D(0xb, 0x63),
        S(0x2e),
        S(0x2d),
        G(0x23),
        D(0x1, 0x75),
        D(0x1, 0x61),
        D(0x1, 0x6f),
        D(0x1, 0x65),
        D(0x1, 0x69),
    ],
    [
        D(0x2, 0x65),
        D(0x8, 0x69),
        D(0x1, 0x61),
        D(0x8, 0x65),
        D(0x3, 0x65),
        D(0x1, 0x75),
        D(0x3, 0x69),
        G(0x23),
        D(0x1, 0x65),
        D(0x3, 0x61),
        D(0x3, 0x6f),
        D(0x3, 0x75),
        D(0xb, 0x63),
    ],
    [
        D(0xb, 0x63),
        S(0x24),
        S(0x21),
        D(0x2, 0x61),
        D(0x2, 0x65),
        D(0x2, 0x69),
        D(0x2, 0x6f),
        D(0x2, 0x75),
        S(0x3f),
        D(0x8, 0x75),
        D(0x4, 0x6e),
        D(0x1, 0x65),
        D(0x1, 0x61),
    ],
    [
        G(0x23),
        D(0xa, 0x75),
        D(0xf, 0x63),
        D(0xf, 0x74),
        D(0xf, 0x7a),
        D(0x2, 0x79),
        D(0x2, 0x69),
        D(0xf, 0x72),
        D(0x2, 0x65),
        D(0x2, 0x61),
        D(0xf, 0x65),
        D(0x2, 0x75),
        D(0xf, 0x73),
    ],
    [
        G(0x23),
        D(0x2, 0x6e),
        D(0xe, 0x61),
        U,
        D(0x2, 0x53),
        S(0x68),
        D(0x2, 0x63),
        D(0x2, 0x6f),
        D(0xe, 0x65),
        D(0x7, 0x7a),
        D(0x2, 0x73),
        S(0x78),
        D(0x2, 0x7a),
    ],
    [
        U,
        D(0x6, 0x67),
        D(0x7, 0x49),
        D(0xb, 0x53),
        D(0x8, 0x4f),
        D(0xb, 0x43),
        D(0x8, 0x55),
        D(0x6, 0x47),
        S(0x75),
        D(0xb, 0x73),
        D(0x8, 0x6f),
        D(0xb, 0x63),
        D(0x8, 0x75),
    ],
    [
        G(0x23),
        D(0x8, 0x45),
        D(0xf, 0x43),
        D(0x2, 0x43),
        D(0xf, 0x5a),
        S(0x62),
        D(0xf, 0x53),
        D(0x8, 0x65),
        D(0xf, 0x63),
        D(0x2, 0x63),
        D(0xf, 0x7a),
        S(0x72),
        D(0xf, 0x73),
    ],
    [
        G(0x23),
        G(0x24),
        D(0xb, 0x54),
        D(0x3, 0x41),
        D(0xb, 0x53),
        P,
        D(0x3, 0x49),
        S(0x75),
        D(0xb, 0x74),
        D(0x3, 0x61),
        D(0xb, 0x73),
        P,
        D(0x3, 0x69),
    ],
    [
        G(0x23),
        D(0x4, 0x6f),
        D(0xf, 0x53),
        D(0x8, 0x41),
        D(0x8, 0x4f),
        D(0xf, 0x5a),
        D(0x8, 0x55),
        D(0x4, 0x4f),
        D(0xf, 0x73),
        D(0x8, 0x61),
        D(0x8, 0x6f),
        D(0xf, 0x7a),
        D(0x8, 0x75),
    ],
    [
        G(0x23),
        S(0x24),
        D(0xf, 0x53),
        D(0x7, 0x65),
        D(0xb, 0x65),
        D(0xf, 0x5a),
        D(0xf, 0x63),
        D(0x5, 0x75),
        D(0xf, 0x73),
        D(0xe, 0x61),
        D(0xe, 0x75),
        D(0xf, 0x7a),
        D(0xe, 0x69),
    ],
];

fn recipes(subset: TeletextNationalSubset) -> [Recipe; 13] {
    let index = match subset {
        TeletextNationalSubset::English => 0,
        TeletextNationalSubset::German => 1,
        TeletextNationalSubset::SwedishFinnishHungarian => 2,
        TeletextNationalSubset::Italian => 3,
        TeletextNationalSubset::French => 4,
        TeletextNationalSubset::PortugueseSpanish => 5,
        TeletextNationalSubset::CzechSlovak => 6,
        TeletextNationalSubset::Polish => 7,
        TeletextNationalSubset::Turkish => 8,
        TeletextNationalSubset::SerbianCroatianSlovenian => 9,
        TeletextNationalSubset::Romanian => 10,
        TeletextNationalSubset::Estonian => 11,
        TeletextNationalSubset::LatvianLithuanian => 12,
    };
    RECIPES[index]
}
