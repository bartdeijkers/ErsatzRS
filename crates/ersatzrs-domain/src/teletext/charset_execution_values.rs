//! Immutable charset-context records; no glyph assignment or raster authority.

use super::{
    TeletextBaseDisplayCellKind, TeletextCharsetPair, TeletextDiacriticIndex,
    TeletextEnhancementCharsetDesignation, TeletextEnhancementPosition, TeletextEnhancementSource,
    TeletextGlyphCandidate, TeletextNationalOption, TeletextPageAddress,
    TeletextPageCharsetDesignation, TeletextSpatialInvocationId, TeletextSpatialRootSource,
};

/// Preserve a raw assigned page or enhancement designation, including aliases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextCharsetSelection {
    PagePrimary(TeletextPageCharsetDesignation),
    Enhancement(TeletextEnhancementCharsetDesignation),
}
impl TeletextCharsetSelection {
    /// Primary G0/G2 repertoire pair; never interpret secondary G0 through this.
    #[must_use]
    pub const fn pair(self) -> TeletextCharsetPair {
        match self {
            Self::PagePrimary(value) => value.pair(),
            Self::Enhancement(value) => value.pair(),
        }
    }
}

/// Known designation or an index into the execution's immutable pending registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextExecutedCharsetSelection {
    pub(super) value: Option<TeletextCharsetSelection>,
    pub(super) pending: Option<usize>,
    pub(super) declaration: Option<usize>,
}
impl TeletextExecutedCharsetSelection {
    /// A designation is returned only when its ordering is established.
    #[must_use]
    pub const fn value(self) -> Option<TeletextCharsetSelection> {
        self.value
    }
    /// Unresolved physical-order record; logical event preorder cannot settle it.
    #[must_use]
    pub const fn pending(self) -> Option<usize> {
        self.pending
    }
    /// Index of the original declaration for a known enhancement context.
    /// Page-primary defaults have no enhancement declaration.
    #[must_use]
    pub const fn declaration(self) -> Option<usize> {
        self.declaration
    }
    pub(super) const fn known(value: TeletextCharsetSelection) -> Self {
        Self {
            value: Some(value),
            pending: None,
            declaration: None,
        }
    }
    pub(super) const fn unresolved(index: usize) -> Self {
        Self {
            value: None,
            pending: Some(index),
            declaration: None,
        }
    }
}

/// Which parts of a charset context an original coded operation requires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextGlyphCharsetUsage {
    /// Actual Base code/role and authored intent remain separate in the candidate.
    BaseCoded,
    G0,
    /// G1 alphabetic positions use current G0, without national substitution.
    G1Alpha,
    G2,
    /// Both current G0 and G2 are retained, even for zero diacritic.
    G0Diacritic {
        diacritic: TeletextDiacriticIndex,
        special_at: bool,
    },
    G1Mosaic,
    G3,
    Drcs,
}
impl TeletextGlyphCharsetUsage {
    #[must_use]
    pub const fn uses_g0(self) -> bool {
        matches!(
            self,
            Self::BaseCoded | Self::G0 | Self::G1Alpha | Self::G0Diacritic { .. }
        )
    }
    #[must_use]
    pub const fn uses_g2(self) -> bool {
        matches!(self, Self::G2 | Self::G0Diacritic { .. })
    }
}

/// Original source of a row-local Base G0 selection; never a G2 designation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextBaseG0Origin {
    RowPrimary {
        row: u8,
    },
    EscapePrimary {
        position: TeletextEnhancementPosition,
    },
    EscapeSecondary {
        position: TeletextEnhancementPosition,
    },
}

/// Sealed Base-only G0 context, retaining the raw alias and last original ESC.
/// The primary G2 and all X26/object charset state remain independent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextBaseG0Selection {
    pub(super) designation: TeletextPageCharsetDesignation,
    pub(super) origin: TeletextBaseG0Origin,
}
impl TeletextBaseG0Selection {
    /// Original Table32/33 G0 designation, including its national-option alias.
    #[must_use]
    pub const fn designation(self) -> TeletextPageCharsetDesignation {
        self.designation
    }
    /// Row default or the original set-after ESC that established this context.
    #[must_use]
    pub const fn origin(self) -> TeletextBaseG0Origin {
        self.origin
    }
}

/// One immutable record per original glyph-layer candidate, including losers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextExecutedGlyphCharset {
    pub(super) candidate: TeletextGlyphCandidate,
    pub(super) usage: TeletextGlyphCharsetUsage,
    pub(super) selection: Option<TeletextExecutedCharsetSelection>,
    pub(super) base_g0: Option<TeletextBaseG0Selection>,
}
impl TeletextExecutedGlyphCharset {
    /// Original code, operation, authored intent and source; no generated glyphs.
    #[must_use]
    pub const fn candidate(&self) -> &TeletextGlyphCandidate {
        &self.candidate
    }
    #[must_use]
    pub const fn usage(&self) -> TeletextGlyphCharsetUsage {
        self.usage
    }
    /// Fixed G1 mosaic, G3 and DRCS operations do not require G0/G2 designation.
    #[must_use]
    pub const fn selection(&self) -> Option<TeletextExecutedCharsetSelection> {
        self.selection
    }
    /// Row-local Base G0, never an inferred enhancement G0/G2 pair.
    #[must_use]
    pub const fn base_g0(&self) -> Option<TeletextBaseG0Selection> {
        self.base_g0
    }
    /// National substitution applies only to actual coded body Base G0 glyphs.
    /// Graphics-mode alphabetic codes 0x40–0x5f still use G0; mosaic, control
    /// and blank roles do not. This is metadata, not glyph decoding.
    #[must_use]
    pub const fn national_substitution(&self) -> bool {
        match self.candidate.base() {
            Some(cell) => {
                cell.position().row() != 0
                    && matches!(cell.kind(), TeletextBaseDisplayCellKind::Glyph)
                    && !(cell.attributes().graphics()
                        && matches!(cell.code(), 0x20..=0x3f | 0x60..=0x7f))
            }
            None => false,
        }
    }
}

/// An original designation with complete trace-local source/instance provenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextCharsetDeclaration {
    pub(super) designation: TeletextEnhancementCharsetDesignation,
    pub(super) event_ordinal: usize,
    pub(super) position: (u16, u16),
    pub(super) source: TeletextEnhancementSource,
    pub(super) root_source: TeletextSpatialRootSource,
    pub(super) invocation: Option<TeletextSpatialInvocationId>,
}
impl TeletextCharsetDeclaration {
    #[must_use]
    pub const fn designation(&self) -> TeletextEnhancementCharsetDesignation {
        self.designation
    }
    /// Logical traversal ordinal, never a physical transmission-order tie-breaker.
    #[must_use]
    pub const fn event_ordinal(&self) -> usize {
        self.event_ordinal
    }
    #[must_use]
    pub const fn position(&self) -> (u16, u16) {
        self.position
    }
    #[must_use]
    pub const fn source(&self) -> TeletextEnhancementSource {
        self.source
    }
    #[must_use]
    pub const fn root_source(&self) -> TeletextSpatialRootSource {
        self.root_source
    }
    #[must_use]
    pub const fn invocation(&self) -> Option<TeletextSpatialInvocationId> {
        self.invocation
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PendingFilter {
    Final,
    Glyph {
        invocation: Option<TeletextSpatialInvocationId>,
        command: usize,
    },
}

/// Equal-placement declarations requiring physical order, with prior context.
/// A previous pending selection links to an earlier spatial group, not a copied
/// path/contender vector. All competing source records remain in one registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextPendingCharsetSelection {
    pub(super) position: (u16, u16),
    pub(super) previous: TeletextExecutedCharsetSelection,
    pub(super) start: usize,
    pub(super) finish: usize,
    pub(super) filter: PendingFilter,
}
impl TeletextPendingCharsetSelection {
    #[must_use]
    pub const fn position(&self) -> (u16, u16) {
        self.position
    }
    /// Context preceding this equal-position uncertainty; may itself be pending.
    #[must_use]
    pub const fn previous(&self) -> TeletextExecutedCharsetSelection {
        self.previous
    }
}

/// Original charset execution, not a generation certificate or renderable page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextCharsetExecution {
    pub(super) owner: TeletextPageAddress,
    pub(super) primary: TeletextPageCharsetDesignation,
    pub(super) secondary: Option<TeletextPageCharsetDesignation>,
    pub(super) national_option: TeletextNationalOption,
    pub(super) extension_present: bool,
    pub(super) records: Vec<TeletextExecutedGlyphCharset>,
    pub(super) declarations: Vec<TeletextCharsetDeclaration>,
    pub(super) shared: Vec<usize>,
    pub(super) pending: Vec<TeletextPendingCharsetSelection>,
}
impl TeletextCharsetExecution {
    #[must_use]
    pub const fn owner(&self) -> TeletextPageAddress {
        self.owner
    }
    /// Selected page primary; Base coded fallback is not certified Unicode here.
    #[must_use]
    pub const fn primary(&self) -> TeletextPageCharsetDesignation {
        self.primary
    }
    /// Retained secondary G0 metadata; no ESC is inferred and G2 stays primary.
    #[must_use]
    pub const fn secondary(&self) -> Option<TeletextPageCharsetDesignation> {
        self.secondary
    }
    /// Original header national bits, separate from an extension primary alias.
    #[must_use]
    pub const fn national_option(&self) -> TeletextNationalOption {
        self.national_option
    }
    #[must_use]
    pub const fn extension_present(&self) -> bool {
        self.extension_present
    }
    /// Exactly the order/cardinality of the supplied original glyph candidates.
    #[must_use]
    pub fn records(&self) -> &[TeletextExecutedGlyphCharset] {
        &self.records
    }
    #[must_use]
    pub fn declarations(&self) -> &[TeletextCharsetDeclaration] {
        &self.declarations
    }
    #[must_use]
    pub fn pending(&self) -> &[TeletextPendingCharsetSelection] {
        &self.pending
    }
    /// Borrow the competing declarations of one pending selection. For a glyph,
    /// future declarations in its own instance are excluded; cross-instance
    /// declarations at the same position have unknown placement relative to it.
    /// For a later position, only the final own-sequence write per instance can
    /// be the preceding group's final context. Invalid indexes yield no records.
    pub fn pending_contenders(
        &self,
        index: usize,
    ) -> impl Iterator<Item = &TeletextCharsetDeclaration> {
        let record = self.pending.get(index);
        let span = record
            .and_then(|record| self.shared.get(record.start..record.finish))
            .unwrap_or(&[]);
        span.iter()
            .enumerate()
            .filter_map(move |(ordinal, &index)| {
                let pending = record?;
                let declaration = &self.declarations[index];
                let next = span
                    .get(ordinal + 1)
                    .map(|&value| &self.declarations[value]);
                let included = match pending.filter {
                    PendingFilter::Final => {
                        next.is_none_or(|next| next.invocation != declaration.invocation)
                    }
                    PendingFilter::Glyph {
                        invocation,
                        command,
                    } => {
                        declaration.invocation != invocation
                            || (declaration.source.command < command
                                && next.is_none_or(|next| {
                                    next.invocation != invocation || next.source.command >= command
                                }))
                    }
                };
                included.then_some(declaration)
            })
    }
}
