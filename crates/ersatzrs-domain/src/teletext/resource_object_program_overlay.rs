//! Checked POP/GPOP structure; catalogue identity never becomes a Local alias.
use super::{
    local_object_glyph_program::{
        validate_invocation_correspondence, validate_invocation_structure_correspondence,
        validate_style_correspondence,
    },
    TeletextEnhancementCommand, TeletextEnhancementProgram,
    TeletextLocalObjectGlyphProgramOverlayError, TeletextObjectIdentity, TeletextServiceBudget,
};

/// Invalid resource structure without retaining authored contents in errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextResourceObjectProgramOverlayError {
    #[error("teletext resource program requires a Public POP or Global GPOP identity")]
    Scope,
    #[error("teletext resource program must change the original program")]
    Unchanged,
    #[error("teletext resource invocation changes require a changed original invocation")]
    MissingInvocationChange,
    #[error("teletext resource invocation structure requires an inserted or omitted invocation")]
    MissingInvocationStructure,
    #[error("teletext resource program exceeds the trusted command or structure budget")]
    Budget,
    #[error(transparent)]
    Correspondence(#[from] TeletextLocalObjectGlyphProgramOverlayError),
}

/// Exact-original ordered resource program replacement with explicit survivors.
///
/// All checked glyphs and foreground/background/attribute commands may be
/// inserted or omitted. Retained glyphs keep family and G3 compatibility;
/// retained styles and all context/position anchors remain exact. Calls remain
/// exact unless the explicit invocation-change constructor is used. Full
/// resource dependencies, native packing and every shared consumer still require
/// complete service/host validation. This grants no edit or publication authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextResourceObjectProgramOverlay {
    identity: TeletextObjectIdentity,
    expected: TeletextEnhancementProgram,
    replacement: TeletextEnhancementProgram,
    retained_commands: Vec<Option<usize>>,
}

#[derive(Clone, Copy)]
enum InvocationPolicy {
    Protected,
    Retained,
    Structure,
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
        Self::new_for_policy(
            identity,
            expected,
            replacement,
            retained_commands,
            budget,
            InvocationPolicy::Protected,
        )
    }

    /// Change existing retained calls alongside the checked glyph/style contract.
    ///
    /// At least one original `InvokeObject` must change target, type or offset.
    /// Calls cannot be inserted or omitted; every other protected anchor remains
    /// exact. This does not validate target availability, type edges, resolved
    /// dependencies or execution geometry: complete service reconstruction and
    /// host provenance/effects checks remain mandatory.
    /// # Errors
    /// Rejects Local identities, no-ops, absent invocation changes, invalid maps,
    /// inserted/deleted calls, changed protected anchors and exhausted budgets.
    pub fn new_with_invocation_changes(
        identity: TeletextObjectIdentity,
        expected: TeletextEnhancementProgram,
        replacement: TeletextEnhancementProgram,
        retained_commands: Vec<Option<usize>>,
        budget: TeletextServiceBudget,
    ) -> Result<Self, TeletextResourceObjectProgramOverlayError> {
        Self::new_for_policy(
            identity,
            expected,
            replacement,
            retained_commands,
            budget,
            InvocationPolicy::Retained,
        )
    }

    /// Insert or omit resource calls while retaining exact original correspondence.
    ///
    /// At least one call must be inserted or omitted. Retained calls may also
    /// change target, type or offset. All other protected anchors remain exact;
    /// this does not grant target, consumer, package or publication authority.
    /// Complete service reconstruction still validates every graph/type edge,
    /// selected resource, expanded consumer and native packing limit.
    /// # Errors
    /// Rejects Local identities, no-ops, missing call structure, invalid maps,
    /// changed noncall anchors, crossed glyph families and exhausted budgets.
    pub fn new_with_invocation_structure(
        identity: TeletextObjectIdentity,
        expected: TeletextEnhancementProgram,
        replacement: TeletextEnhancementProgram,
        retained_commands: Vec<Option<usize>>,
        budget: TeletextServiceBudget,
    ) -> Result<Self, TeletextResourceObjectProgramOverlayError> {
        Self::new_for_policy(
            identity,
            expected,
            replacement,
            retained_commands,
            budget,
            InvocationPolicy::Structure,
        )
    }

    fn new_for_policy(
        identity: TeletextObjectIdentity,
        expected: TeletextEnhancementProgram,
        replacement: TeletextEnhancementProgram,
        retained_commands: Vec<Option<usize>>,
        budget: TeletextServiceBudget,
        invocations: InvocationPolicy,
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
        match invocations {
            InvocationPolicy::Protected => {
                validate_style_correspondence(&expected, &replacement, &retained_commands)?;
            }
            InvocationPolicy::Retained => {
                validate_invocation_correspondence(&expected, &replacement, &retained_commands)?;
            }
            InvocationPolicy::Structure => validate_invocation_structure_correspondence(
                &expected,
                &replacement,
                &retained_commands,
            )?,
        }
        let overlay = Self {
            identity,
            expected,
            replacement,
            retained_commands,
        };
        if matches!(invocations, InvocationPolicy::Retained) && !overlay.has_invocation_changes() {
            return Err(Error::MissingInvocationChange);
        }
        if matches!(invocations, InvocationPolicy::Structure)
            && !overlay.has_invocation_structure_changes()
        {
            return Err(Error::MissingInvocationStructure);
        }
        Ok(overlay)
    }

    /// Whether retained original calls change, without granting target authority.
    #[must_use]
    pub fn has_invocation_changes(&self) -> bool {
        self.replacement
            .commands()
            .iter()
            .zip(&self.retained_commands)
            .any(|(after, original)| {
                original.is_some_and(|ordinal| {
                    self.expected.commands().get(ordinal).is_some_and(|before| {
                        before != after
                            && matches!(
                                (before, after),
                                (
                                    TeletextEnhancementCommand::InvokeObject { .. },
                                    TeletextEnhancementCommand::InvokeObject { .. }
                                )
                            )
                    })
                })
            })
    }

    /// Whether validated correspondence inserts or omits an original call.
    /// This linear scan does not broaden retained-call change semantics or grant
    /// permission for an invocation's target, effects or consuming displays.
    #[must_use]
    pub fn has_invocation_structure_changes(&self) -> bool {
        let mut originals = self.expected.commands().iter().enumerate();
        for (after, original) in self
            .replacement
            .commands()
            .iter()
            .zip(&self.retained_commands)
        {
            if let Some(ordinal) = original {
                for (index, command) in originals.by_ref() {
                    if index == *ordinal {
                        break;
                    }
                    if matches!(command, TeletextEnhancementCommand::InvokeObject { .. }) {
                        return true;
                    }
                }
            } else if matches!(after, TeletextEnhancementCommand::InvokeObject { .. }) {
                return true;
            }
        }
        originals
            .any(|(_, command)| matches!(command, TeletextEnhancementCommand::InvokeObject { .. }))
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
