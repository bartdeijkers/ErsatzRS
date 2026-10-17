//! Bounded exact-original glyph structure changes in retained Local definitions.

use super::{
    TeletextColumnGlyphFamily, TeletextColumnOperation, TeletextEnhancementCommand,
    TeletextEnhancementProgram, TeletextObjectIdentity, TeletextPageAddress,
};

const MAX_COMMANDS: usize = 16 * 13;

#[derive(Clone, Copy)]
enum GlyphPolicy {
    Legacy,
    All,
    StyleCommands,
}

/// Rejected structural intent, without retaining authored content in diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextLocalObjectGlyphProgramOverlayError {
    #[error("teletext glyph program requires an exact Local owner")]
    Scope,
    #[error("teletext glyph program must change the original program")]
    Unchanged,
    #[error("teletext glyph program exceeds the bounded command capacity")]
    Capacity,
    #[error("teletext glyph program has invalid retained-command correspondence")]
    Correspondence,
    #[error("teletext glyph program changes a protected command or glyph family")]
    ProtectedCommand,
}

/// One checked replacement of an existing Local definition's ordered program.
///
/// The legacy constructor permits G0/G1 structure changes. The explicit complete
/// glyph constructor supports all typed glyph families, retaining G3 compatibility.
/// Every nonglyph command survives exactly and in order, retaining position,
/// context, attributes and invocation anchors. Retained glyphs cannot cross an
/// unchanged anchor. The map records checked correspondence, not edit permission:
/// hosts must establish original provenance and qualify every shared consumer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextLocalObjectGlyphProgramOverlay {
    identity: TeletextObjectIdentity,
    owner: TeletextPageAddress,
    expected: TeletextEnhancementProgram,
    replacement: TeletextEnhancementProgram,
    retained_commands: Vec<Option<usize>>,
}

impl TeletextLocalObjectGlyphProgramOverlay {
    /// Check explicit original ordinals for every replacement command.
    ///
    /// `None` denotes an inserted G0/G1 command. `Some` ordinals must increase
    /// strictly; omitted originals must also be G0/G1. Programs and map are each
    /// bounded by 208 commands before scanning. Physical packing, full execution,
    /// publication and Free/Zone permissions remain separate qualification gates.
    /// # Errors
    /// Rejects nonlocal/no-op intents, oversized programs, invalid correspondence,
    /// altered anchors and cross-family retained glyph changes.
    pub fn new(
        identity: TeletextObjectIdentity,
        expected: TeletextEnhancementProgram,
        replacement: TeletextEnhancementProgram,
        retained_commands: Vec<Option<usize>>,
    ) -> Result<Self, TeletextLocalObjectGlyphProgramOverlayError> {
        Self::new_for_policy(
            identity,
            expected,
            replacement,
            retained_commands,
            GlyphPolicy::Legacy,
        )
    }

    /// Check structural changes across all checked glyph families.
    ///
    /// Retained glyphs keep their semantic family and G3 compatibility. Inserted
    /// and omitted commands may be any typed glyph; nonglyph anchors remain exact.
    /// DRCS operations keep complete explicit table/subtable/slot selections; the
    /// full service must resolve them against its original links and catalogues.
    /// This creates no resources, edit permissions or publication authority.
    /// # Errors
    /// Rejects nonlocal/no-op intents, over208 inputs, invalid correspondence,
    /// changed anchors, cross-family matches and G3 compatibility changes.
    pub fn new_with_all_glyphs(
        identity: TeletextObjectIdentity,
        expected: TeletextEnhancementProgram,
        replacement: TeletextEnhancementProgram,
        retained_commands: Vec<Option<usize>>,
    ) -> Result<Self, TeletextLocalObjectGlyphProgramOverlayError> {
        Self::new_for_policy(
            identity,
            expected,
            replacement,
            retained_commands,
            GlyphPolicy::All,
        )
    }

    /// Check all glyph changes and inserted or deleted Local style commands.
    ///
    /// Only checked `AtColumn` foreground, background and attribute operations
    /// may additionally be inserted or omitted. Retained style commands remain
    /// exact, including their column and every attribute flag. Position, charset,
    /// flash and invocation anchors remain mandatory and exact. Full service
    /// execution and shared-consumer permissions are still host responsibilities.
    /// # Errors
    /// Rejects nonlocal/no-op intents, over208 inputs, invalid correspondence,
    /// changed retained styles or protected anchors and crossed glyph families.
    pub fn new_with_style_commands(
        identity: TeletextObjectIdentity,
        expected: TeletextEnhancementProgram,
        replacement: TeletextEnhancementProgram,
        retained_commands: Vec<Option<usize>>,
    ) -> Result<Self, TeletextLocalObjectGlyphProgramOverlayError> {
        Self::new_for_policy(
            identity,
            expected,
            replacement,
            retained_commands,
            GlyphPolicy::StyleCommands,
        )
    }

    fn new_for_policy(
        identity: TeletextObjectIdentity,
        expected: TeletextEnhancementProgram,
        replacement: TeletextEnhancementProgram,
        retained_commands: Vec<Option<usize>>,
        policy: GlyphPolicy,
    ) -> Result<Self, TeletextLocalObjectGlyphProgramOverlayError> {
        let owner = identity
            .owner()
            .ok_or(TeletextLocalObjectGlyphProgramOverlayError::Scope)?;
        if expected.commands().len() > MAX_COMMANDS
            || replacement.commands().len() > MAX_COMMANDS
            || retained_commands.len() > MAX_COMMANDS
        {
            return Err(TeletextLocalObjectGlyphProgramOverlayError::Capacity);
        }
        if expected == replacement {
            return Err(TeletextLocalObjectGlyphProgramOverlayError::Unchanged);
        }
        validate_correspondence(&expected, &replacement, &retained_commands, policy)?;
        Ok(Self {
            identity,
            owner,
            expected,
            replacement,
            retained_commands,
        })
    }

    /// Whether this checked value also satisfies the unchanged G0/G1 contract.
    ///
    /// Borrowed validation neither clones programs nor treats unchanged extended
    /// glyph anchors as new features. Versioned storage must retain this boundary.
    #[must_use]
    pub fn is_legacy_compatible(&self) -> bool {
        validate_correspondence(
            &self.expected,
            &self.replacement,
            &self.retained_commands,
            GlyphPolicy::Legacy,
        )
        .is_ok()
    }

    /// Whether the unchanged all-glyph contract accepts this correspondence.
    ///
    /// Borrowed validation preserves the version12 boundary without cloning or
    /// treating an unchanged retained style as a structural style edit.
    #[must_use]
    pub fn is_all_glyphs_compatible(&self) -> bool {
        validate_correspondence(
            &self.expected,
            &self.replacement,
            &self.retained_commands,
            GlyphPolicy::All,
        )
        .is_ok()
    }

    #[must_use]
    pub const fn identity(&self) -> TeletextObjectIdentity {
        self.identity
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

    /// Replacement ordinal to original ordinal; `None` denotes an insertion.
    #[must_use]
    pub fn retained_commands(&self) -> &[Option<usize>] {
        &self.retained_commands
    }

    /// Original ordinal for a retained replacement command, or no correspondence.
    #[must_use]
    pub fn original_command(&self, replacement_command: usize) -> Option<usize> {
        self.retained_commands
            .get(replacement_command)
            .copied()
            .flatten()
    }

    /// Replacement ordinal for a retained original command, or no correspondence.
    #[must_use]
    pub fn replacement_command(&self, original_command: usize) -> Option<usize> {
        self.retained_commands
            .iter()
            .position(|command| *command == Some(original_command))
    }
}

pub(super) fn validate_style_correspondence(
    expected: &TeletextEnhancementProgram,
    replacement: &TeletextEnhancementProgram,
    retained: &[Option<usize>],
) -> Result<(), TeletextLocalObjectGlyphProgramOverlayError> {
    validate_correspondence(expected, replacement, retained, GlyphPolicy::StyleCommands)
}

fn validate_correspondence(
    expected: &TeletextEnhancementProgram,
    replacement: &TeletextEnhancementProgram,
    retained: &[Option<usize>],
    policy: GlyphPolicy,
) -> Result<(), TeletextLocalObjectGlyphProgramOverlayError> {
    use TeletextLocalObjectGlyphProgramOverlayError as Error;
    if retained.len() != replacement.commands().len() {
        return Err(Error::Correspondence);
    }
    let mut next_original = 0;
    for (command, original) in replacement.commands().iter().zip(retained) {
        let Some(ordinal) = *original else {
            if !editable(command, policy) {
                return Err(Error::ProtectedCommand);
            }
            continue;
        };
        if ordinal < next_original || ordinal >= expected.commands().len() {
            return Err(Error::Correspondence);
        }
        if expected.commands()[next_original..ordinal]
            .iter()
            .any(|command| !editable(command, policy))
        {
            return Err(Error::ProtectedCommand);
        }
        let before = &expected.commands()[ordinal];
        match (glyph_family(before, policy), glyph_family(command, policy)) {
            (Some(before), Some(after)) if before == after => {}
            (None, None) if before == command => {}
            _ => return Err(Error::ProtectedCommand),
        }
        next_original = ordinal + 1;
    }
    if expected.commands()[next_original..]
        .iter()
        .any(|command| !editable(command, policy))
    {
        return Err(Error::ProtectedCommand);
    }
    Ok(())
}

fn editable(command: &TeletextEnhancementCommand, policy: GlyphPolicy) -> bool {
    glyph_family(command, policy).is_some()
        || matches!(
            (policy, command),
            (
                GlyphPolicy::StyleCommands,
                TeletextEnhancementCommand::AtColumn {
                    operation: TeletextColumnOperation::Foreground { .. }
                        | TeletextColumnOperation::Background { .. }
                        | TeletextColumnOperation::Attributes { .. },
                    ..
                }
            )
        )
}

fn glyph_family(
    command: &TeletextEnhancementCommand,
    policy: GlyphPolicy,
) -> Option<TeletextColumnGlyphFamily> {
    let TeletextEnhancementCommand::AtColumn { operation, .. } = command else {
        return None;
    };
    let family = operation.glyph_family()?;
    if matches!(policy, GlyphPolicy::Legacy)
        && !matches!(
            family,
            TeletextColumnGlyphFamily::G0 | TeletextColumnGlyphFamily::G1
        )
    {
        return None;
    }
    Some(family)
}

/// Exact Local definition failure while applying a checked program replacement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextLocalObjectGlyphProgramFailure {
    #[error("the exact Local object definition is absent")]
    MissingObject,
    #[error("the original Local object program no longer matches")]
    ProgramMismatch,
    #[error("the exact Local object program is repeated")]
    Duplicate,
}
