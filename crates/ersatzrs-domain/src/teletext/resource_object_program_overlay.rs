//! Checked POP/GPOP structure; catalogue identity never becomes a Local alias.
use super::{
    local_object_glyph_program::validate_style_correspondence, TeletextEnhancementProgram,
    TeletextLocalObjectGlyphProgramOverlayError, TeletextObjectIdentity, TeletextServiceBudget,
};

/// Invalid resource structure without retaining authored contents in errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextResourceObjectProgramOverlayError {
    #[error("teletext resource program requires a Public POP or Global GPOP identity")]
    Scope,
    #[error("teletext resource program must change the original program")]
    Unchanged,
    #[error("teletext resource program exceeds the trusted command or structure budget")]
    Budget,
    #[error(transparent)]
    Correspondence(#[from] TeletextLocalObjectGlyphProgramOverlayError),
}

/// Exact-original ordered resource program replacement with explicit survivors.
///
/// All checked glyphs and foreground/background/attribute commands may be
/// inserted or omitted. Retained glyphs keep family and G3 compatibility;
/// retained styles and all context/call/position anchors remain exact. Full
/// resource dependencies, native packing and every shared consumer still require
/// complete service/host validation. This grants no edit or publication authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextResourceObjectProgramOverlay {
    identity: TeletextObjectIdentity,
    expected: TeletextEnhancementProgram,
    replacement: TeletextEnhancementProgram,
    retained_commands: Vec<Option<usize>>,
}

impl TeletextResourceObjectProgramOverlay {
    /// Validate raw command/map bounds before scanning correspondence.
    ///
    /// Trusted authored/structure limits are independent of physical resource
    /// capacity: invisible header-position commands consume no packet words.
    /// # Errors
    /// Rejects Local identities, no-ops, exhausted trusted budgets, invalid maps,
    /// changed protected anchors and crossed retained glyph families.
    pub fn new(
        identity: TeletextObjectIdentity,
        expected: TeletextEnhancementProgram,
        replacement: TeletextEnhancementProgram,
        retained_commands: Vec<Option<usize>>,
        budget: TeletextServiceBudget,
    ) -> Result<Self, TeletextResourceObjectProgramOverlayError> {
        use TeletextResourceObjectProgramOverlayError as Error;
        if identity.resource().is_none() {
            return Err(Error::Scope);
        }
        if budget.max_objects == 0
            || expected
                .commands()
                .len()
                .checked_add(replacement.commands().len())
                .is_none_or(|count| count > budget.max_commands)
            || retained_commands.len() > budget.max_structure_items
        {
            return Err(Error::Budget);
        }
        if expected == replacement {
            return Err(Error::Unchanged);
        }
        validate_style_correspondence(&expected, &replacement, &retained_commands)?;
        Ok(Self {
            identity,
            expected,
            replacement,
            retained_commands,
        })
    }

    #[must_use]
    pub const fn identity(&self) -> TeletextObjectIdentity {
        self.identity
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
    /// Replacement ordinal to original ordinal; inserted commands return none.
    #[must_use]
    pub fn original_command(&self, replacement_command: usize) -> Option<usize> {
        self.retained_commands
            .get(replacement_command)
            .copied()
            .flatten()
    }
    /// Original ordinal to surviving replacement ordinal; deletions return none.
    #[must_use]
    pub fn replacement_command(&self, original_command: usize) -> Option<usize> {
        self.retained_commands
            .iter()
            .position(|command| *command == Some(original_command))
    }
}

/// Exact original resource definition failure, separate from permission checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextResourceObjectProgramFailure {
    #[error("the exact resource object definition is absent")]
    MissingObject,
    #[error("the original resource object program no longer matches")]
    ProgramMismatch,
    #[error("the exact resource object program is repeated")]
    Duplicate,
}
