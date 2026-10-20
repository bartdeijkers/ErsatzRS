//! Exact optional Main presence; an explicit empty program suppresses defaults.
use super::{TeletextEnhancementProgram, TeletextPageAddress};

/// Invalid presence intent never aliases absence with an explicit empty program.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextMainProgramPresenceOverlayError {
    #[error("Main presence requires a genuine absent/present transition")]
    NotTransition,
}

/// One original-bound absent/present transition, without edit or publication authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextMainProgramPresenceOverlay {
    owner: TeletextPageAddress,
    expected: Option<TeletextEnhancementProgram>,
    replacement: Option<TeletextEnhancementProgram>,
}
impl TeletextMainProgramPresenceOverlay {
    /// Capture only None-to-Some or Some-to-None, including Some(empty).
    /// # Errors
    /// Rejects None-to-None and every Some-to-Some edit; existing program roles
    /// retain their separate checked correspondence and command contracts.
    pub fn new(
        owner: TeletextPageAddress,
        expected: Option<TeletextEnhancementProgram>,
        replacement: Option<TeletextEnhancementProgram>,
    ) -> Result<Self, TeletextMainProgramPresenceOverlayError> {
        if expected.is_some() == replacement.is_some() {
            return Err(TeletextMainProgramPresenceOverlayError::NotTransition);
        }
        Ok(Self {
            owner,
            expected,
            replacement,
        })
    }
    #[must_use]
    pub const fn owner(&self) -> TeletextPageAddress {
        self.owner
    }
    #[must_use]
    pub const fn expected(&self) -> Option<&TeletextEnhancementProgram> {
        self.expected.as_ref()
    }
    #[must_use]
    pub const fn replacement(&self) -> Option<&TeletextEnhancementProgram> {
        self.replacement.as_ref()
    }
}

/// Original optional presence attachment failure, without authored text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextMainProgramPresenceFailure {
    #[error("Main presence repeats an original owner")]
    Duplicate,
    #[error("Main presence owner is absent")]
    MissingOwner,
    #[error("Main presence differs from the exact original optional program")]
    OriginalMismatch,
    #[error("Main presence references an unknown, mistyped or foreign-owner object")]
    Target,
}

/// Atomic semantic/declaration rejection, preserving the unmodified input.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TeletextMainProgramPresenceApplyError {
    #[error(transparent)]
    Service(#[from] super::TeletextServiceError),
    #[error(transparent)]
    Declaration(#[from] super::TeletextDeclaredServiceError),
}
