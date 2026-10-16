//! Exact-original glyph changes in retained local object definitions.
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

/// One same-family G0/G1 replacement in an existing local object definition.
///
/// Object identity/type, command order, geometry, charset, invocation targets and
/// all other commands are immutable. G1 alpha, G2/G3 and resource edits are absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextLocalObjectOperandOverlay {
    identity: TeletextObjectIdentity,
    operand: TeletextLocalProgramOperandOverlay,
}

impl TeletextLocalObjectOperandOverlay {
    /// Check a local, same-family G0 or G1 intent under exact original expectations.
    /// # Errors
    /// Rejects nonlocal identities, other families, invalid columns and no-op edits.
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
        if !matches!(
            (expected, replacement),
            (
                TeletextColumnOperation::G0 { .. },
                TeletextColumnOperation::G0 { .. }
            ) | (
                TeletextColumnOperation::G1 { .. },
                TeletextColumnOperation::G1 { .. }
            )
        ) {
            return Err(TeletextLocalProgramOperandOverlayError::Family.into());
        }
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
