//! Bounded exact-original glyph structure changes in retained Local definitions.

use super::{
    TeletextColumnOperation, TeletextEnhancementCommand, TeletextEnhancementProgram,
    TeletextObjectIdentity, TeletextPageAddress,
};

const MAX_COMMANDS: usize = 16 * 13;

#[derive(PartialEq, Eq)]
enum GlyphFamily {
    G0,
    G1,
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
/// Only G0/G1 commands may be inserted, omitted or changed within their original
/// family. Every other command survives exactly and in order, retaining position,
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
        validate_correspondence(&expected, &replacement, &retained_commands)?;
        Ok(Self {
            identity,
            owner,
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

fn validate_correspondence(
    expected: &TeletextEnhancementProgram,
    replacement: &TeletextEnhancementProgram,
    retained: &[Option<usize>],
) -> Result<(), TeletextLocalObjectGlyphProgramOverlayError> {
    use TeletextLocalObjectGlyphProgramOverlayError as Error;
    if retained.len() != replacement.commands().len() {
        return Err(Error::Correspondence);
    }
    let mut next_original = 0;
    for (command, original) in replacement.commands().iter().zip(retained) {
        let Some(ordinal) = *original else {
            if glyph_family(command).is_none() {
                return Err(Error::ProtectedCommand);
            }
            continue;
        };
        if ordinal < next_original || ordinal >= expected.commands().len() {
            return Err(Error::Correspondence);
        }
        if expected.commands()[next_original..ordinal]
            .iter()
            .any(|command| glyph_family(command).is_none())
        {
            return Err(Error::ProtectedCommand);
        }
        let before = &expected.commands()[ordinal];
        match (glyph_family(before), glyph_family(command)) {
            (Some(before), Some(after)) if before == after => {}
            (None, None) if before == command => {}
            _ => return Err(Error::ProtectedCommand),
        }
        next_original = ordinal + 1;
    }
    if expected.commands()[next_original..]
        .iter()
        .any(|command| glyph_family(command).is_none())
    {
        return Err(Error::ProtectedCommand);
    }
    Ok(())
}

fn glyph_family(command: &TeletextEnhancementCommand) -> Option<GlyphFamily> {
    match command {
        TeletextEnhancementCommand::AtColumn {
            operation: TeletextColumnOperation::G0 { .. },
            ..
        } => Some(GlyphFamily::G0),
        TeletextEnhancementCommand::AtColumn {
            operation: TeletextColumnOperation::G1 { .. },
            ..
        } => Some(GlyphFamily::G1),
        _ => None,
    }
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
