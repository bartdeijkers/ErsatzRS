//! Exact-original DRCS bitmap changes, without edit or publication authority.
//!
//! A glyph can be shared by several pages and objects. Hosts must recompile and
//! check all protected effects; this value grants no permission to change them.

use super::{
    TeletextDrcsBitmap, TeletextDrcsGlyphSlot, TeletextResourceIdentity, TeletextResourceKind,
};

/// Rejected bitmap intent, without exposing authored bitmap contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextDrcsBitmapOverlayError {
    #[error("teletext bitmap replacement requires a DRCS or GDRCS resource")]
    Kind,
    #[error("teletext bitmap replacement does not change the original")]
    Unchanged,
}

/// One immutable replacement of a defined glyph in an exact resource and S1.
///
/// Neither identity, slot nor geometry can change. Applying this value still
/// requires a matching original bitmap and full service validation. Publication,
/// package ownership and permissions remain separate host responsibilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextDrcsBitmapOverlay {
    identity: TeletextResourceIdentity,
    slot: TeletextDrcsGlyphSlot,
    expected: TeletextDrcsBitmap,
    replacement: TeletextDrcsBitmap,
}

impl TeletextDrcsBitmapOverlay {
    /// Check a monochrome bitmap change retaining its exact identity.
    /// # Errors
    /// Rejects resource kinds other than DRCS/GDRCS and unchanged bitmaps.
    pub fn new(
        identity: TeletextResourceIdentity,
        slot: TeletextDrcsGlyphSlot,
        expected: TeletextDrcsBitmap,
        replacement: TeletextDrcsBitmap,
    ) -> Result<Self, TeletextDrcsBitmapOverlayError> {
        if !matches!(
            identity.kind(),
            TeletextResourceKind::Drcs | TeletextResourceKind::Gdrcs
        ) {
            return Err(TeletextDrcsBitmapOverlayError::Kind);
        }
        if expected == replacement {
            return Err(TeletextDrcsBitmapOverlayError::Unchanged);
        }
        Ok(Self {
            identity,
            slot,
            expected,
            replacement,
        })
    }

    #[must_use]
    pub const fn identity(self) -> TeletextResourceIdentity {
        self.identity
    }

    #[must_use]
    pub const fn slot(self) -> TeletextDrcsGlyphSlot {
        self.slot
    }

    #[must_use]
    pub const fn expected(self) -> TeletextDrcsBitmap {
        self.expected
    }

    #[must_use]
    pub const fn replacement(self) -> TeletextDrcsBitmap {
        self.replacement
    }
}

/// Application failure at an exact existing resource and glyph slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextDrcsBitmapFailure {
    #[error("the exact DRCS resource is absent")]
    MissingResource,
    #[error("the DRCS glyph slot is undefined")]
    MissingGlyph,
    #[error("the original DRCS bitmap no longer matches")]
    BitmapMismatch,
    #[error("the exact DRCS glyph target is repeated")]
    Duplicate,
}
