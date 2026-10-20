//! Checked-context glyph identities, independent of fonts, Unicode and rasterization.

use super::{
    TeletextCharsetPair, TeletextDrcsGlyphReference, TeletextExecutedGlyphCharset,
    TeletextGlyphCode, TeletextMosaic, TeletextPageAddress,
};

/// Primary alphanumeric table identity (EN 300 706 tables 35/38–40/42/44/46).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextG0Repertoire {
    Latin,
    Cyrillic1,
    Cyrillic2,
    Cyrillic3,
    Greek,
    Arabic,
    Hebrew,
}
/// Supplementary table identity (tables 37/41/43/45).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextG2Repertoire {
    Latin,
    Cyrillic,
    Greek,
    Arabic,
}
/// Actual table 36 subset, not a masked designation or language inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextNationalSubset {
    English,
    German,
    SwedishFinnishHungarian,
    Italian,
    French,
    PortugueseSpanish,
    CzechSlovak,
    Polish,
    Turkish,
    SerbianCroatianSlovenian,
    Romanian,
    Estonian,
    LatvianLithuanian,
}

/// Exact G0 position; a national subset is present only at a substituted Base code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextG0Glyph {
    pub(super) repertoire: TeletextG0Repertoire,
    pub(super) code: u8,
    pub(super) national_subset: Option<TeletextNationalSubset>,
}
impl TeletextG0Glyph {
    #[must_use]
    pub const fn repertoire(self) -> TeletextG0Repertoire {
        self.repertoire
    }
    #[must_use]
    pub const fn code(self) -> u8 {
        self.code
    }
    /// Enhanced G0/G1 alphabetic positions never use national substitution.
    #[must_use]
    pub const fn national_subset(self) -> Option<TeletextNationalSubset> {
        self.national_subset
    }
}

/// Exact supplementary position; never derived from secondary G0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextG2Glyph {
    pub(super) repertoire: TeletextG2Repertoire,
    pub(super) code: u8,
}
impl TeletextG2Glyph {
    #[must_use]
    pub const fn repertoire(self) -> TeletextG2Repertoire {
        self.repertoire
    }
    #[must_use]
    pub const fn code(self) -> u8 {
        self.code
    }
}

/// Original glyph identity or meaningful non-glyph Base role.
/// No variant asserts a Unicode mapping, bitmap shape, visibility or readiness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextGlyphIdentity {
    G0 {
        glyph: TeletextG0Glyph,
    },
    G2 {
        glyph: TeletextG2Glyph,
    },
    G1Mosaic {
        mosaic: TeletextMosaic,
    },
    G3 {
        code: TeletextGlyphCode,
    },
    Drcs {
        reference: TeletextDrcsGlyphReference,
    },
    /// Base G0 and current G2 column-4 entry retained without mark normalization.
    Composite {
        base: TeletextG0Glyph,
        mark: TeletextG2Glyph,
    },
    /// Table 29's special zero-diacritic code 2/A, independently of current G0.
    AtSymbol,
    Blank,
    SpacingControl {
        code: u8,
    },
}

/// Assignment evidence, not a renderer/publication acceptance outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextGlyphDisposition {
    Assigned,
    Unassigned,
    ShouldNotTransmit,
    Pending,
}
/// Why a glyph cannot yet be assigned or composed with proven semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextGlyphIntentReason {
    /// Index into the supplied original charset execution's pending registry.
    CharsetOrder { pending: usize },
    /// A table cell has no printed glyph or explicit SPACE/prohibition meaning.
    UnmarkedTableCell,
    /// Current G0/mark combination lacks established alphabetic/composition evidence.
    UnverifiedComposition,
    /// A nonzero diacritic requires an alphabetic G0 base (table 29).
    NonAlphabeticBase,
    /// The primary table explicitly marks this position unassigned/prohibited.
    ProhibitedTablePosition,
}

/// One candidate-aligned record; all original losing sources and Base intent survive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextExecutedGlyphIntent {
    pub(super) context: TeletextExecutedGlyphCharset,
    pub(super) disposition: TeletextGlyphDisposition,
    pub(super) glyph: Option<TeletextGlyphIdentity>,
    pub(super) reason: Option<TeletextGlyphIntentReason>,
    pub(super) selected_pair: Option<TeletextCharsetPair>,
    pub(super) held_mosaic: Option<super::TeletextHeldMosaic>,
}
impl TeletextExecutedGlyphIntent {
    /// Derived Base-only shape; original spacing control/context stays unchanged.
    #[must_use]
    pub const fn held_mosaic(&self) -> Option<super::TeletextHeldMosaic> {
        self.held_mosaic
    }
    /// Unchanged original candidate/context, including opaque pending-order index.
    #[must_use]
    pub const fn context(&self) -> &TeletextExecutedGlyphCharset {
        &self.context
    }
    #[must_use]
    pub const fn disposition(&self) -> TeletextGlyphDisposition {
        self.disposition
    }
    /// Retained exact identity also for unassigned/prohibited positions. Unknown
    /// designation order returns none when applicable repertoire pairs disagree.
    #[must_use]
    pub const fn glyph(&self) -> Option<TeletextGlyphIdentity> {
        self.glyph
    }
    #[must_use]
    pub const fn reason(&self) -> Option<TeletextGlyphIntentReason> {
        self.reason
    }

    /// Required repertoire pair, known directly or agreed by every applicable
    /// unresolved designation. This chooses no raw alias or declaration winner;
    /// the original context and pending registry remain unchanged. Fixed-family
    /// and charset-independent glyphs have no required pair.
    #[must_use]
    pub const fn selected_pair(&self) -> Option<TeletextCharsetPair> {
        self.selected_pair
    }
}

/// Immutable lookup results; candidate order/cardinality matches charset execution.
/// The authoritative caller retains that same charset execution alongside this
/// result, so pending indexes reuse its shared registry without cloning it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextGlyphIntent {
    pub(super) owner: TeletextPageAddress,
    pub(super) records: Vec<TeletextExecutedGlyphIntent>,
}
impl TeletextGlyphIntent {
    #[must_use]
    pub const fn owner(&self) -> TeletextPageAddress {
        self.owner
    }
    #[must_use]
    pub fn records(&self) -> &[TeletextExecutedGlyphIntent] {
        &self.records
    }
}
