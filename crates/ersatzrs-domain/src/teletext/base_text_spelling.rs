//! Explicit native spelling of an existing logical Text cell, separate from Local.
use super::{
    TeletextColumnOperation, TeletextEnhancementCharsetDesignation, TeletextEnhancementPosition,
    TeletextGlyphCode, TeletextPageAddress,
};
use std::fmt;

/// Static spelling/association rejection; no supplied characters are exposed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextBaseTextSpellingError {
    #[error("native Text spelling overlaps reserved header identity")]
    Header,
    #[error("native Text spelling requires a non-control scalar")]
    Scalar,
    #[error("native Text spelling requires a G2 or G0-diacritic glyph")]
    Operation,
    #[error("native Text spelling has no matching original Text cell")]
    Target,
    #[error("native Text spelling repeats an exact cell")]
    Duplicate,
}

/// A retained printable Level1 fallback and charset-bound enhancement for one
/// logical Unicode Text cell. The ordinary compiler must independently prove
/// exact character meaning in this context. This grants no original Local
/// command identity, edit permission, receipt, package or publication authority.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct TeletextBaseTextSpelling {
    owner: TeletextPageAddress,
    position: TeletextEnhancementPosition,
    character: char,
    fallback: TeletextGlyphCode,
    charset: TeletextEnhancementCharsetDesignation,
    operation: TeletextColumnOperation,
}
impl TeletextBaseTextSpelling {
    /// Capture an explicit glyph spelling without normalizing its raw fallback.
    /// # Errors
    /// Rejects header identity cells, controls and non-glyph operations.
    pub fn new(
        owner: TeletextPageAddress,
        position: TeletextEnhancementPosition,
        character: char,
        fallback: TeletextGlyphCode,
        charset: TeletextEnhancementCharsetDesignation,
        operation: TeletextColumnOperation,
    ) -> Result<Self, TeletextBaseTextSpellingError> {
        if position.row() == 0 && position.column() < 8 {
            return Err(TeletextBaseTextSpellingError::Header);
        }
        if character.is_control() {
            return Err(TeletextBaseTextSpellingError::Scalar);
        }
        if !matches!(
            operation,
            TeletextColumnOperation::G2 { .. } | TeletextColumnOperation::G0Diacritic { .. }
        ) {
            return Err(TeletextBaseTextSpellingError::Operation);
        }
        Ok(Self {
            owner,
            position,
            character,
            fallback,
            charset,
            operation,
        })
    }
    #[must_use]
    pub const fn owner(self) -> TeletextPageAddress {
        self.owner
    }
    #[must_use]
    pub const fn position(self) -> TeletextEnhancementPosition {
        self.position
    }
    #[must_use]
    pub const fn character(self) -> char {
        self.character
    }
    #[must_use]
    pub const fn fallback(self) -> TeletextGlyphCode {
        self.fallback
    }
    #[must_use]
    pub const fn charset(self) -> TeletextEnhancementCharsetDesignation {
        self.charset
    }
    #[must_use]
    pub const fn operation(self) -> TeletextColumnOperation {
        self.operation
    }
}
impl fmt::Debug for TeletextBaseTextSpelling {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextBaseTextSpelling")
            .field("owner", &self.owner)
            .field("position", &self.position)
            .finish_non_exhaustive()
    }
}
