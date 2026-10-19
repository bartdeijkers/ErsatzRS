//! Exact authored Unicode Base-cell changes, never invented Local commands.

use super::{TeletextBaseTextSpelling, TeletextEnhancementPosition, TeletextPageAddress};
use std::fmt;

/// Content-free rejection of a Base text edit or its original association.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextBaseTextEditError {
    #[error("Base text edit overlaps reserved header identity columns")]
    Header,
    #[error("Base text edit requires non-control Unicode scalars")]
    Scalar,
    #[error("Base text edit does not change its original scalar")]
    Unchanged,
    #[error("Base text edit repeats a target")]
    Duplicate,
    #[error("Base text edit has no matching original Text cell")]
    Target,
    #[error("Base text edit differs from its exact original scalar")]
    Expected,
    #[error("Base text edit native spelling differs from its exact replacement cell")]
    Spelling,
}

/// One cell of an existing Unicode Text run, separate from coded row/Local edits.
/// The native host must still authenticate compiler correspondence, permissions
/// and every protected effect. This value grants no save/publication authority.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct TeletextBaseTextEdit {
    owner: TeletextPageAddress,
    position: TeletextEnhancementPosition,
    expected: char,
    replacement: char,
    native_spelling: Option<TeletextBaseTextSpelling>,
}

impl TeletextBaseTextEdit {
    /// Preserve the exact scalar and cell; no string normalization or resizing.
    /// Encoding, repertoire assignment and physical spacing are checked by the
    /// ordinary native compiler, not inferred from a received glyph or bitmap.
    /// # Errors
    /// Rejects header identity writes, controls and unchanged scalars.
    pub fn new(
        owner: TeletextPageAddress,
        position: TeletextEnhancementPosition,
        expected: char,
        replacement: char,
    ) -> Result<Self, TeletextBaseTextEditError> {
        if position.row() == 0 && position.column() < 8 {
            return Err(TeletextBaseTextEditError::Header);
        }
        if expected.is_control() || replacement.is_control() {
            return Err(TeletextBaseTextEditError::Scalar);
        }
        if expected == replacement {
            return Err(TeletextBaseTextEditError::Unchanged);
        }
        Ok(Self {
            owner,
            position,
            expected,
            replacement,
            native_spelling: None,
        })
    }

    /// Retain a native spelling bound to this exact replacement, without Local
    /// identity or edit/publication authority. The compiler checks its meaning.
    /// # Errors
    /// Rejects a different owner, cell or logical replacement character.
    pub fn with_native_spelling(
        mut self,
        spelling: TeletextBaseTextSpelling,
    ) -> Result<Self, TeletextBaseTextEditError> {
        if (spelling.owner(), spelling.position(), spelling.character())
            != (self.owner, self.position, self.replacement)
        {
            return Err(TeletextBaseTextEditError::Spelling);
        }
        self.native_spelling = Some(spelling);
        Ok(self)
    }

    #[must_use]
    pub const fn native_spelling(self) -> Option<TeletextBaseTextSpelling> {
        self.native_spelling
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
    pub const fn expected(self) -> char {
        self.expected
    }
    #[must_use]
    pub const fn replacement(self) -> char {
        self.replacement
    }
}

impl fmt::Debug for TeletextBaseTextEdit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextBaseTextEdit")
            .field("owner", &self.owner)
            .field("position", &self.position)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests;
