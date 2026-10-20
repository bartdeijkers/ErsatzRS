//! Exact retained Main resource-call targets, separate from Local call structure.
use super::{TeletextEnhancementCommand, TeletextObjectScope, TeletextPageAddress};

/// A rejected Main target intent, without source contents in diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextMainResourceInvocationOverlayError {
    #[error("Main resource retargeting requires existing Public/Global calls")]
    ResourceCall,
    #[error("Main resource retargeting must change the original target")]
    Unchanged,
    #[error("Main resource retargeting must preserve object type and offset")]
    ProtectedCall,
}

/// One exact-original Main invocation; no insertion, deletion or body authority.
///
/// Only the retained Public/Global target changes. Type, offset, ordinal, other
/// commands, default suppression and owner stay exact. Complete final graph,
/// source provenance, protected effects and publication remain host obligations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextMainResourceInvocationOverlay {
    owner: TeletextPageAddress,
    command: usize,
    expected: TeletextEnhancementCommand,
    replacement: TeletextEnhancementCommand,
}
impl TeletextMainResourceInvocationOverlay {
    /// Bind a retained original resource call to a changed Public/Global target.
    ///
    /// Targets may cross POP/GPOP roles; the complete transaction must prove the
    /// final selected roles, definitions and every executing consumer. Ordinals
    /// are checked against the bounded original program without allocating a map
    /// or inferring a command from the uploaded value.
    /// # Errors
    /// Rejects noncalls, Local targets, no-ops and changed type or offset.
    pub fn new(
        owner: TeletextPageAddress,
        command: usize,
        expected: TeletextEnhancementCommand,
        replacement: TeletextEnhancementCommand,
    ) -> Result<Self, TeletextMainResourceInvocationOverlayError> {
        use TeletextEnhancementCommand::InvokeObject;
        use TeletextMainResourceInvocationOverlayError as Error;
        let (
            InvokeObject {
                target: old,
                object_type: old_type,
                offset: old_offset,
            },
            InvokeObject {
                target: new,
                object_type: new_type,
                offset: new_offset,
            },
        ) = (expected, replacement)
        else {
            return Err(Error::ResourceCall);
        };
        if [old, new]
            .iter()
            .any(|target| target.scope() == TeletextObjectScope::Local)
        {
            return Err(Error::ResourceCall);
        }
        if old_type != new_type || old_offset != new_offset {
            return Err(Error::ProtectedCall);
        }
        if old == new {
            return Err(Error::Unchanged);
        }
        Ok(Self {
            owner,
            command,
            expected,
            replacement,
        })
    }
    #[must_use]
    pub const fn owner(self) -> TeletextPageAddress {
        self.owner
    }
    #[must_use]
    pub const fn command(self) -> usize {
        self.command
    }
    #[must_use]
    pub const fn expected(self) -> TeletextEnhancementCommand {
        self.expected
    }
    #[must_use]
    pub const fn replacement(self) -> TeletextEnhancementCommand {
        self.replacement
    }
}

/// Exact original attachment failure, independent of host edit permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextMainResourceInvocationFailure {
    #[error("Main resource retargeting repeats the exact owner and command")]
    Duplicate,
    #[error("Main resource retargeting has no original explicit Main program")]
    MissingProgram,
    #[error("Main resource retargeting has no original command at this ordinal")]
    MissingCommand,
    #[error("Main resource retargeting does not match the exact original call")]
    OriginalMismatch,
}
