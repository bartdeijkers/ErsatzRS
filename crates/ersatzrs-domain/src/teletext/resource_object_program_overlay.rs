//! Checked POP/GPOP structure; catalogue identity never becomes a Local alias.
use super::{
    local_object_glyph_program::{
        validate_inserted_position_correspondence, validate_invocation_correspondence,
        validate_invocation_structure_correspondence, validate_style_correspondence,
    },
    TeletextEnhancementCommand, TeletextEnhancementProgram,
    TeletextLocalObjectGlyphProgramOverlayError, TeletextObjectIdentity,
    TeletextResourceObjectOperandOverlay, TeletextServiceBudget,
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
    #[error("teletext resource position insertion requires a new position command")]
    MissingInsertedPosition,
    #[error("teletext position restoration requires checked original-position intent")]
    OriginalPositionIntent,
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
    position_intent: PositionIntent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PositionIntent {
    Ordinary,
    Original,
    Restoration,
}

#[derive(Clone, Copy)]
enum InvocationPolicy {
    Protected,
    Retained,
    Structure,
    InsertedPositions,
    OriginalPositions,
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

    /// Reconstruct inserted positions for a server-authenticated added definition.
    ///
    /// Existing position and context anchors remain mandatory and exact. This is
    /// transient intent: hosts must authenticate the added identity and fold the
    /// replacement into its complete definition before storing a concept. Legacy
    /// resource-program overlay formats do not support this intent.
    /// # Errors
    /// Rejects absent inserted positions, changed or omitted anchors, invalid
    /// correspondence, crossed glyph families, Local identities and budget excess.
    pub fn new_with_inserted_positions(
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
            InvocationPolicy::InsertedPositions,
        )
    }

    /// Preserve explicit positive-row insertions in an original resource body.
    ///
    /// Existing context and position anchors remain mandatory and exact. This
    /// distinct intent requires the versioned original-position storage role;
    /// it grants no receipt, shared-consumer or publication authority. Row-zero
    /// insertion and broader position editing remain separate authored coverage.
    /// # Errors
    /// Rejects absent new positions, inserted row zero, invalid correspondence,
    /// changed protected anchors, Local identities and exhausted budgets.
    pub fn new_with_original_positions(
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
            InvocationPolicy::OriginalPositions,
        )
    }

    /// Reverse one checked original-position edit as ephemeral root restoration.
    ///
    /// The inverse correspondence comes solely from the prior checked map;
    /// producer position/context anchors remain retained. No uploaded map or
    /// replacement is accepted. Hosts must authenticate the current/root bodies
    /// and fold this intent into cumulative root state before saving. No saved
    /// overlay format supports this restoration role.
    /// # Errors
    /// Rejects transient, ordinary or restoration priors and exhausted fresh
    /// aggregate command/structure budgets before allocating the inverse map.
    pub fn restore_original_positions(
        previous: &Self,
        budget: TeletextServiceBudget,
    ) -> Result<Self, TeletextResourceObjectProgramOverlayError> {
        Self::restore_original_positions_to(previous, &previous.expected, budget)
    }

    /// Reverse checked position history and compatible retained scalar changes.
    ///
    /// The target keeps the prior expected command count, columns and non-scalar
    /// anchors. Differences use the existing resource scalar validator; inverse
    /// correspondence still comes exclusively from the checked prior map.
    /// This checks structure only. The host must authenticate the exact immutable
    /// root, fresh receipt, current body and all shared consumer permissions, then
    /// fold restoration before persistence. No saved format accepts this intent.
    /// # Errors
    /// Rejects unqualified priors, changed anchors, incompatible scalar families,
    /// columns, command counts and exhausted fresh budgets before allocation.
    pub fn restore_original_positions_to(
        previous: &Self,
        original: &TeletextEnhancementProgram,
        budget: TeletextServiceBudget,
    ) -> Result<Self, TeletextResourceObjectProgramOverlayError> {
        use TeletextResourceObjectProgramOverlayError as Error;
        if !previous.is_original_position_intent() {
            return Err(Error::OriginalPositionIntent);
        }
        if budget.max_objects == 0
            || previous
                .expected
                .commands()
                .len()
                .checked_add(previous.replacement.commands().len())
                .is_none_or(|count| count > budget.max_commands)
            || previous.expected.commands().len() > budget.max_structure_items
        {
            return Err(Error::Budget);
        }
        if original.commands().len() != previous.expected.commands().len() {
            return Err(Error::OriginalPositionIntent);
        }
        for (ordinal, (root, expected)) in original
            .commands()
            .iter()
            .zip(previous.expected.commands())
            .enumerate()
        {
            if root == expected {
                continue;
            }
            let (
                TeletextEnhancementCommand::AtColumn {
                    column,
                    operation: before,
                },
                TeletextEnhancementCommand::AtColumn {
                    column: after_column,
                    operation: after,
                },
            ) = (root, expected)
            else {
                return Err(Error::OriginalPositionIntent);
            };
            if column != after_column {
                return Err(Error::OriginalPositionIntent);
            }
            TeletextResourceObjectOperandOverlay::new(
                previous.identity,
                ordinal,
                *column,
                *before,
                *after,
            )
            .map_err(|_| Error::OriginalPositionIntent)?;
        }
        let mut inverse = vec![None; previous.expected.commands().len()];
        for (ordinal, original) in previous.retained_commands.iter().enumerate() {
            if let Some(original) = original {
                let target = inverse
                    .get_mut(*original)
                    .ok_or(Error::OriginalPositionIntent)?;
                if target.replace(ordinal).is_some() {
                    return Err(Error::OriginalPositionIntent);
                }
            }
        }
        Ok(Self {
            identity: previous.identity,
            expected: previous.replacement.clone(),
            replacement: original.clone(),
            retained_commands: inverse,
            position_intent: PositionIntent::Restoration,
        })
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
            InvocationPolicy::InsertedPositions | InvocationPolicy::OriginalPositions => {
                validate_inserted_position_correspondence(
                    &expected,
                    &replacement,
                    &retained_commands,
                )?;
            }
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
            position_intent: if matches!(invocations, InvocationPolicy::OriginalPositions) {
                PositionIntent::Original
            } else {
                PositionIntent::Ordinary
            },
        };
        if matches!(invocations, InvocationPolicy::Retained) && !overlay.has_invocation_changes() {
            return Err(Error::MissingInvocationChange);
        }
        if matches!(invocations, InvocationPolicy::Structure)
            && !overlay.has_invocation_structure_changes()
        {
            return Err(Error::MissingInvocationStructure);
        }
        if matches!(
            invocations,
            InvocationPolicy::InsertedPositions | InvocationPolicy::OriginalPositions
        ) && !overlay.has_inserted_positions()
        {
            return Err(Error::MissingInsertedPosition);
        }
        if overlay.is_original_position_intent() && overlay.has_inserted_zero_row() {
            return Err(Error::Correspondence(
                TeletextLocalObjectGlyphProgramOverlayError::ProtectedCommand,
            ));
        }
        Ok(overlay)
    }

    /// Explicit original-position storage policy, distinct from transient Add intent.
    #[must_use]
    pub const fn is_original_position_intent(&self) -> bool {
        matches!(self.position_intent, PositionIntent::Original)
    }

    /// Ephemeral inverse intent which must never be serialized as a saved overlay.
    #[must_use]
    pub const fn is_position_restoration(&self) -> bool {
        matches!(self.position_intent, PositionIntent::Restoration)
    }

    fn has_inserted_zero_row(&self) -> bool {
        self.replacement.commands().iter().zip(&self.retained_commands)
            .any(|(command, original)| original.is_none()
                && matches!(command, TeletextEnhancementCommand::SetPosition { position } if position.row() == 0))
    }

    /// Whether correspondence inserts a position, independently of storage policy.
    #[must_use]
    pub fn has_inserted_positions(&self) -> bool {
        self.replacement
            .commands()
            .iter()
            .zip(&self.retained_commands)
            .any(|(command, original)| {
                original.is_none()
                    && matches!(command, TeletextEnhancementCommand::SetPosition { .. })
            })
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
