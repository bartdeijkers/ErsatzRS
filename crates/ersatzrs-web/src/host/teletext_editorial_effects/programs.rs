//! Borrowed correspondence for distinct Local and resource program identities.
use super::TeletextEditorialEdits;
use ersatzrs_domain::{
    TeletextEnhancementProgram, TeletextLocalObjectGlyphProgramOverlay, TeletextObjectIdentity,
    TeletextResourceObjectProgramOverlay,
};

#[derive(Clone, Copy)]
pub(super) enum Program<'a> {
    Local(&'a TeletextLocalObjectGlyphProgramOverlay),
    Resource(&'a TeletextResourceObjectProgramOverlay),
}
impl<'a> Program<'a> {
    pub(super) fn has_invocation_changes(self) -> bool {
        match self {
            Self::Local(value) => value.has_invocation_changes(),
            Self::Resource(value) => value.has_invocation_changes(),
        }
    }
    pub(super) fn identity(self) -> TeletextObjectIdentity {
        match self {
            Self::Local(value) => value.identity(),
            Self::Resource(value) => value.identity(),
        }
    }
    pub(super) fn expected(self) -> &'a TeletextEnhancementProgram {
        match self {
            Self::Local(value) => value.expected(),
            Self::Resource(value) => value.expected(),
        }
    }
    pub(super) fn replacement(self) -> &'a TeletextEnhancementProgram {
        match self {
            Self::Local(value) => value.replacement(),
            Self::Resource(value) => value.replacement(),
        }
    }
    pub(super) fn original_command(self, ordinal: usize) -> Option<usize> {
        match self {
            Self::Local(value) => value.original_command(ordinal),
            Self::Resource(value) => value.original_command(ordinal),
        }
    }
    pub(super) fn replacement_command(self, ordinal: usize) -> Option<usize> {
        match self {
            Self::Local(value) => value.replacement_command(ordinal),
            Self::Resource(value) => value.replacement_command(ordinal),
        }
    }
}
pub(super) fn all(edits: TeletextEditorialEdits<'_>) -> impl Iterator<Item = Program<'_>> {
    edits
        .object_programs
        .iter()
        .map(Program::Local)
        .chain(edits.resource_programs.iter().map(Program::Resource))
}
