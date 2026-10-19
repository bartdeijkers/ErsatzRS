//! Exact-owner Main call structure, without catalogue or publication authority.
use super::{
    TeletextEnhancementCommand as Command, TeletextEnhancementProgram, TeletextObjectScope,
    TeletextPageAddress,
};

/// Rejected Main correspondence, without retaining authored data in diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextMainLocalInvocationProgramOverlayError {
    #[error("teletext Main call structure exceeds 208 commands")]
    Capacity,
    #[error("teletext Main call structure must change its original program")]
    Unchanged,
    #[error("teletext Main call structure has invalid correspondence")]
    Correspondence,
    #[error("teletext Main call structure changes a protected command")]
    ProtectedCommand,
}

/// A replacement of an existing explicit Main program on one page/subpage.
/// Only same-owner Local calls may be inserted or omitted. All retained commands
/// remain exact; separately validated scalar edits must be applied first. Actual
/// targets/types, complete execution, native packing and permissions remain
/// attachment/host checks, never authority conveyed by this value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextMainLocalInvocationProgramOverlay {
    owner: TeletextPageAddress,
    expected: TeletextEnhancementProgram,
    replacement: TeletextEnhancementProgram,
    retained_commands: Vec<Option<usize>>,
}
impl TeletextMainLocalInvocationProgramOverlay {
    /// Check a strictly ordered exact-original map with genuine call structure.
    /// # Errors
    /// Rejects no-ops, over208 programs/maps, malformed maps, altered retained
    /// commands and inserted/omitted commands other than same-owner Local calls.
    pub fn new(
        owner: TeletextPageAddress,
        expected: TeletextEnhancementProgram,
        replacement: TeletextEnhancementProgram,
        retained_commands: Vec<Option<usize>>,
    ) -> Result<Self, TeletextMainLocalInvocationProgramOverlayError> {
        use TeletextMainLocalInvocationProgramOverlayError as Error;
        if expected.commands().len() > 208
            || replacement.commands().len() > 208
            || retained_commands.len() > 208
        {
            return Err(Error::Capacity);
        }
        if expected == replacement {
            return Err(Error::Unchanged);
        }
        if retained_commands.len() != replacement.commands().len() {
            return Err(Error::Correspondence);
        }
        let local = |command: &Command| {
            matches!(command, Command::InvokeObject { target, .. }
            if target.scope() == TeletextObjectScope::Local && target.owner() == Some(owner))
        };
        let mut next = 0;
        for (command, source) in replacement.commands().iter().zip(&retained_commands) {
            if let Some(ordinal) = *source {
                if ordinal < next || ordinal >= expected.commands().len() {
                    return Err(Error::Correspondence);
                }
                if expected.commands()[next..ordinal]
                    .iter()
                    .any(|before| !local(before))
                    || expected.commands()[ordinal] != *command
                {
                    return Err(Error::ProtectedCommand);
                }
                next = ordinal + 1;
            } else if !local(command) {
                return Err(Error::ProtectedCommand);
            }
        }
        if expected.commands()[next..]
            .iter()
            .any(|command| !local(command))
        {
            return Err(Error::ProtectedCommand);
        }
        Ok(Self {
            owner,
            expected,
            replacement,
            retained_commands,
        })
    }
    #[must_use]
    pub const fn owner(&self) -> TeletextPageAddress {
        self.owner
    }
    #[must_use]
    pub const fn expected(&self) -> &TeletextEnhancementProgram {
        &self.expected
    }
    #[must_use]
    pub const fn replacement(&self) -> &TeletextEnhancementProgram {
        &self.replacement
    }
    #[must_use]
    pub fn retained_commands(&self) -> &[Option<usize>] {
        &self.retained_commands
    }
    /// Original ordinal for a retained replacement command; None means inserted.
    #[must_use]
    pub fn original_command(&self, replacement: usize) -> Option<usize> {
        self.retained_commands.get(replacement).copied().flatten()
    }
    /// Replacement ordinal for an original command; None means omitted.
    #[must_use]
    pub fn replacement_command(&self, original: usize) -> Option<usize> {
        self.retained_commands
            .iter()
            .position(|source| *source == Some(original))
    }
}

/// Exact-owner failure when attaching Main structure to its complete service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextMainLocalInvocationProgramFailure {
    #[error("the exact page has no explicit Main program")]
    MissingProgram,
    #[error("the original Main program no longer matches")]
    ProgramMismatch,
    #[error("the exact Main owner is repeated")]
    Duplicate,
    #[error("an editable Main call has no original same-owner Local target/type")]
    Target,
}
