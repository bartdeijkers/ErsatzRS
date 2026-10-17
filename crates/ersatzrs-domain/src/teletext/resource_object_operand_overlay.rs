//! Exact-original resource-definition scalars, without display or edit authority.

use super::{
    local_program_operand_overlay::validate_scalar_operand, TeletextColumnOperation,
    TeletextLocalProgramOperandOverlayError, TeletextObjectIdentity,
};

/// Rejected resource operand without authored text or packet contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextResourceObjectOperandOverlayError {
    #[error("teletext resource object operand requires a Public POP or Global GPOP identity")]
    Scope,
    #[error(transparent)]
    Operand(#[from] TeletextLocalProgramOperandOverlayError),
}

/// One same-family scalar replacement in an existing POP/GPOP definition.
///
/// Full scope, resource page, logical S1 and object ID remain exact; this value
/// has no synthetic display owner. Order, command column, type, invocation targets
/// and resources remain unchanged. Hosts must authenticate compiler provenance
/// and check every shared consumer before saving or publishing an edited service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextResourceObjectOperandOverlay {
    identity: TeletextObjectIdentity,
    command: usize,
    column: u8,
    expected: TeletextColumnOperation,
    replacement: TeletextColumnOperation,
}

impl TeletextResourceObjectOperandOverlay {
    /// Check an exact resource identity and the existing scalar family contract.
    ///
    /// Colours, attributes, flash, assigned charset and all checked glyph families
    /// are supported. Attribute size flags and G3 compatibility stay fixed. DRCS
    /// selections retain complete scope/subtable/slot and require full-service
    /// resolution. This neither creates resources nor grants edit permissions.
    /// # Errors
    /// Rejects Local identities, invalid columns, crossed families, size changes
    /// and no-op operands through the unchanged shared scalar validator.
    pub fn new(
        identity: TeletextObjectIdentity,
        command: usize,
        column: u8,
        expected: TeletextColumnOperation,
        replacement: TeletextColumnOperation,
    ) -> Result<Self, TeletextResourceObjectOperandOverlayError> {
        if identity.resource().is_none() {
            return Err(TeletextResourceObjectOperandOverlayError::Scope);
        }
        validate_scalar_operand(column, expected, replacement)?;
        Ok(Self {
            identity,
            command,
            column,
            expected,
            replacement,
        })
    }

    /// Complete checked Public/Global resource identity, including logical S1.
    #[must_use]
    pub const fn identity(self) -> TeletextObjectIdentity {
        self.identity
    }
    /// Original definition ordinal before invocation expansion or packing.
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

/// Application failure at the full resource definition and original ordinal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextResourceObjectOperandFailure {
    #[error("the exact resource object definition is absent")]
    MissingObject,
    #[error("the original resource object command no longer matches")]
    CommandMismatch,
    #[error("the exact resource object command is repeated")]
    Duplicate,
}
