//! Exact-original local X26 operand changes, without edit or publication authority.
//!
//! An attribute can affect cells beyond its command column. Hosts must recompile
//! and check every protected effect; a checked operand is not a permission mask.

use super::{TeletextColumnOperation, TeletextG3Compatibility, TeletextPageAddress};

/// Rejected operand shape, without exposing page text or raw packet data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextLocalProgramOperandOverlayError {
    #[error("teletext local operand column {column} is outside 0..=39")]
    Column { column: u8 },
    #[error("teletext local operand must retain a supported family and its G3 compatibility")]
    Family,
    #[error("teletext local operand replacement does not change the original")]
    Unchanged,
    #[error("teletext local attributes replacement must preserve character size")]
    Geometry,
}

/// One immutable replacement of an existing local `AtColumn` operand.
///
/// The exact owner, original command ordinal, column and original operation are
/// retained. Charset, positions, invocation/resource identities and command
/// ordering cannot be changed through this value. This does not grant permission
/// to modify any affected cell or to publish an edited service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextLocalProgramOperandOverlay {
    owner: TeletextPageAddress,
    command: usize,
    column: u8,
    expected: TeletextColumnOperation,
    replacement: TeletextColumnOperation,
}

impl TeletextLocalProgramOperandOverlay {
    /// Check same-family colour, attribute or checked glyph replacement.
    /// G1 alphabetic intent cannot become mosaics; G3 keeps its original
    /// Level 1.5/2.5 compatibility rather than changing its transmitted mode.
    ///
    /// A G0-diacritic replacement may change its code, selector, or both. The
    /// original charset context and every other command remain unchanged; valid
    /// scalar ranges do not establish assigned glyphs or rendering readiness.
    ///
    /// Service application must still match the exact original local command.
    /// # Errors
    /// Rejects invalid columns, all other operation families, size changes and no-op changes.
    pub fn new(
        owner: TeletextPageAddress,
        command: usize,
        column: u8,
        expected: TeletextColumnOperation,
        replacement: TeletextColumnOperation,
    ) -> Result<Self, TeletextLocalProgramOperandOverlayError> {
        if column > 39 {
            return Err(TeletextLocalProgramOperandOverlayError::Column { column });
        }
        if !matches!(
            (expected, replacement),
            (
                TeletextColumnOperation::Foreground { .. },
                TeletextColumnOperation::Foreground { .. }
            ) | (
                TeletextColumnOperation::Background { .. },
                TeletextColumnOperation::Background { .. }
            ) | (
                TeletextColumnOperation::Attributes { .. },
                TeletextColumnOperation::Attributes { .. }
            ) | (
                TeletextColumnOperation::G0 { .. },
                TeletextColumnOperation::G0 { .. }
            ) | (
                TeletextColumnOperation::G1 { .. },
                TeletextColumnOperation::G1 { .. }
            ) | (
                TeletextColumnOperation::G2 { .. },
                TeletextColumnOperation::G2 { .. }
            ) | (
                TeletextColumnOperation::G0Diacritic { .. },
                TeletextColumnOperation::G0Diacritic { .. }
            ) | (
                TeletextColumnOperation::G1Alpha { .. },
                TeletextColumnOperation::G1Alpha { .. }
            ) | (
                TeletextColumnOperation::G3 {
                    compatibility: TeletextG3Compatibility::Level15,
                    ..
                },
                TeletextColumnOperation::G3 {
                    compatibility: TeletextG3Compatibility::Level15,
                    ..
                }
            ) | (
                TeletextColumnOperation::G3 {
                    compatibility: TeletextG3Compatibility::Level25,
                    ..
                },
                TeletextColumnOperation::G3 {
                    compatibility: TeletextG3Compatibility::Level25,
                    ..
                }
            )
        ) {
            return Err(TeletextLocalProgramOperandOverlayError::Family);
        }
        if let (
            TeletextColumnOperation::Attributes {
                double_height: old_height,
                double_width: old_width,
                ..
            },
            TeletextColumnOperation::Attributes {
                double_height: new_height,
                double_width: new_width,
                ..
            },
        ) = (expected, replacement)
        {
            if old_height != new_height || old_width != new_width {
                return Err(TeletextLocalProgramOperandOverlayError::Geometry);
            }
        }
        if expected == replacement {
            return Err(TeletextLocalProgramOperandOverlayError::Unchanged);
        }
        Ok(Self {
            owner,
            command,
            column,
            expected,
            replacement,
        })
    }

    #[must_use]
    pub const fn owner(self) -> TeletextPageAddress {
        self.owner
    }

    /// Zero-based ordinal in the original local program, not expanded commands.
    #[must_use]
    pub const fn command(self) -> usize {
        self.command
    }

    #[must_use]
    pub const fn column(self) -> u8 {
        self.column
    }

    #[must_use]
    pub const fn expected(self) -> TeletextColumnOperation {
        self.expected
    }

    #[must_use]
    pub const fn replacement(self) -> TeletextColumnOperation {
        self.replacement
    }
}

/// Application failure bound to an exact local program location.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextLocalProgramOperandFailure {
    #[error("the exact page/subcode has no explicit local program")]
    MissingLocalProgram,
    #[error("the original local command no longer matches")]
    CommandMismatch,
    #[error("the exact local command is repeated")]
    Duplicate,
}
