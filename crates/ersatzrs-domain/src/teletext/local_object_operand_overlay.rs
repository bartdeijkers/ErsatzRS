//! Exact-original operand changes in retained local object definitions.
//!
//! These values identify authored commands, not expanded calls or generated
//! carrier objects. Hosts must establish original compiler provenance, recompile
//! all consumers and enforce captured permissions; this grants no edit authority.

use super::{
    TeletextColumnOperation, TeletextLocalProgramOperandOverlay,
    TeletextLocalProgramOperandOverlayError, TeletextObjectIdentity, TeletextPageAddress,
};

/// Rejected local-object intent, without text or raw packet contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextLocalObjectOperandOverlayError {
    #[error("teletext object operand requires an exact local owner")]
    Scope,
    #[error(transparent)]
    Operand(#[from] TeletextLocalProgramOperandOverlayError),
}

/// One same-family colour, attribute or checked glyph replacement in a local definition.
///
/// Object identity/type, command order, geometry, invocation targets and
/// all other commands are immutable. G1 alphabetic edits retain their operation
/// family and G3 retains its compatibility. DRCS slot changes retain their original
/// table/subtable; resource declarations and bitmap contents remain immutable.
/// Existing Flash or `DesignateCharset` operands may change within their typed
/// family; rebuilding the full service still qualifies repertoire/context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextLocalObjectOperandOverlay {
    identity: TeletextObjectIdentity,
    operand: TeletextLocalProgramOperandOverlay,
}

impl TeletextLocalObjectOperandOverlay {
    /// Check an exact-original local operand through the shared page validator.
    /// Colours, attributes, flash, charset and checked glyphs retain their families;
    /// attribute changes preserve both original character-size flags.
    /// # Errors
    /// Rejects nonlocal identities, other families, size changes, invalid columns and no-op edits.
    pub fn new(
        identity: TeletextObjectIdentity,
        command: usize,
        column: u8,
        expected: TeletextColumnOperation,
        replacement: TeletextColumnOperation,
    ) -> Result<Self, TeletextLocalObjectOperandOverlayError> {
        let owner = identity
            .owner()
            .ok_or(TeletextLocalObjectOperandOverlayError::Scope)?;
        Ok(Self {
            identity,
            operand: TeletextLocalProgramOperandOverlay::new(
                owner,
                command,
                column,
                expected,
                replacement,
            )?,
        })
    }

    #[must_use]
    pub const fn identity(self) -> TeletextObjectIdentity {
        self.identity
    }

    /// Exact display/subcode owner; never inferred from a physical packet page.
    #[must_use]
    pub const fn owner(self) -> TeletextPageAddress {
        self.operand.owner()
    }

    /// Zero-based ordinal in the retained definition, before invocation expansion.
    #[must_use]
    pub const fn command(self) -> usize {
        self.operand.command()
    }

    #[must_use]
    pub const fn column(self) -> u8 {
        self.operand.column()
    }

    #[must_use]
    pub const fn expected(self) -> TeletextColumnOperation {
        self.operand.expected()
    }

    #[must_use]
    pub const fn replacement(self) -> TeletextColumnOperation {
        self.operand.replacement()
    }
}

/// Application failure at the exact retained definition and command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextLocalObjectOperandFailure {
    #[error("the exact local object definition is absent")]
    MissingObject,
    #[error("the original local object command no longer matches")]
    CommandMismatch,
    #[error("the exact local object command is repeated")]
    Duplicate,
}
