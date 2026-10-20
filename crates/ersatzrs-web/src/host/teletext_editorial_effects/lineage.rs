//! Exact before/after source correspondence for checked Local glyph structure.
//! Inserted commands never become missing provenance or an original source.
use super::{
    check_deadline, error,
    programs::{self, Program},
    Result, TeletextEditorialEdits, TeletextEditorialEffectsFailure,
};
use ersatzrs_domain::{
    TeletextAttributeSource, TeletextEnhancementObjectDefinition, TeletextEnhancementSource,
    TeletextObjectIdentity, TeletextSpatialRootSource,
};
use ersatzrs_ffmpeg::teletext::{TeletextAuthoredServiceEncodingBudget, TeletextMainComposition};
use std::{collections::BTreeMap, time::Instant};

mod calls;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CanonicalSource {
    Existing(TeletextEnhancementSource),
    Inserted(TeletextEnhancementSource),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CanonicalRoot {
    Existing(TeletextSpatialRootSource),
    Inserted(TeletextSpatialRootSource),
}

type EventKey = (
    Option<TeletextObjectIdentity>,
    usize,
    (u8, usize),
    Option<usize>,
);

#[derive(Default)]
pub(super) struct ProtectedLineage<'a> {
    main_program: Option<&'a ersatzrs_domain::TeletextMainLocalInvocationProgramOverlay>,
    programs: BTreeMap<TeletextObjectIdentity, Program<'a>>,
    additions: BTreeMap<TeletextObjectIdentity, &'a TeletextEnhancementObjectDefinition>,
    events: Option<Vec<Option<usize>>>,
    calls: calls::Calls,
}
impl<'a> ProtectedLineage<'a> {
    pub(super) fn new(
        before: &TeletextMainComposition,
        after: &TeletextMainComposition,
        edits: TeletextEditorialEdits<'a>,
        budget: &TeletextAuthoredServiceEncodingBudget,
        deadline: Instant,
    ) -> Result<Self> {
        let mut result = Self::from_edits(before.owner(), edits, deadline)?;
        if result.programs.is_empty()
            && result.additions.is_empty()
            && result.main_program.is_none()
        {
            return Ok(result);
        }
        let old = before.spatial_trace();
        let new = after.spatial_trace();
        let maximum = budget.normalization.composition.collection.max_commands;
        if old.events().len() > maximum
            || new.events().len() > maximum
            || old.invocations().len() > maximum
            || new.invocations().len() > maximum
        {
            return Err(error(
                Some(before.owner()),
                None,
                TeletextEditorialEffectsFailure::Budget,
            ));
        }
        if old.owner() != new.owner()
            || old.original_program_source() != new.original_program_source()
        {
            return Err(association());
        }
        result.calls = calls::Calls::new(old, new, &result, deadline)?;
        let mut indexed = BTreeMap::new();
        for (ordinal, event) in old.events().iter().enumerate() {
            check_deadline(deadline)?;
            if indexed
                .insert(
                    event_key(event.root_source(), event.source(), event.invocation()),
                    ordinal,
                )
                .is_some()
            {
                return Err(association());
            }
        }
        let mut events = Vec::with_capacity(new.events().len());
        for event in new.events() {
            check_deadline(deadline)?;
            let source = event.source();
            result.check_source_command(source)?;
            events.push(
                match result.source_at_invocation(source, event.invocation()) {
                    CanonicalSource::Inserted(_) => None,
                    CanonicalSource::Existing(source) => Some(
                        *indexed
                            .get(&event_key(
                                result
                                    .root_source(event.root_source())
                                    .ok_or_else(association)?,
                                source,
                                result.canonical_invocation(event.invocation()),
                            ))
                            .ok_or_else(association)?,
                    ),
                },
            );
        }
        result.events = Some(events);
        Ok(result)
    }

    fn from_edits(
        owner: ersatzrs_domain::TeletextPageAddress,
        edits: TeletextEditorialEdits<'a>,
        deadline: Instant,
    ) -> Result<Self> {
        let mut result = Self::default();
        for program in edits
            .main_programs
            .iter()
            .filter(|program| program.owner() == owner)
        {
            check_deadline(deadline)?;
            if result.main_program.replace(program).is_some() {
                return Err(association());
            }
        }
        for addition in edits.resource_additions {
            check_deadline(deadline)?;
            if result
                .additions
                .insert(addition.identity(), addition.definition())
                .is_some()
            {
                return Err(association());
            }
        }
        for definition in edits
            .object_resource_additions
            .iter()
            .flat_map(|bank| bank.addition().definitions())
        {
            check_deadline(deadline)?;
            if result
                .additions
                .insert(definition.identity, definition)
                .is_some()
            {
                return Err(association());
            }
        }
        for program in programs::all(edits).filter(|program| match program {
            Program::Local(value) => value.owner() == owner,
            Program::Resource(_) => true,
        }) {
            check_deadline(deadline)?;
            if result
                .programs
                .insert(program.identity(), program)
                .is_some()
                || result.additions.contains_key(&program.identity())
            {
                return Err(association());
            }
        }
        Ok(result)
    }

    fn check_source_command(&self, source: TeletextEnhancementSource) -> Result<()> {
        if source.object.is_none()
            && self.main_program.is_some_and(|program| {
                program
                    .replacement()
                    .commands()
                    .get(source.command)
                    .is_none()
            })
        {
            return Err(association());
        }
        if source
            .object
            .and_then(|object| self.additions.get(&object))
            .is_some_and(|definition| definition.program.commands().get(source.command).is_none())
        {
            return Err(association());
        }
        if let Some(program) = source.object.and_then(|object| self.programs.get(&object)) {
            if program
                .replacement()
                .commands()
                .get(source.command)
                .is_none()
            {
                return Err(association());
            }
        }
        Ok(())
    }

    pub(super) fn source(&self, source: TeletextEnhancementSource) -> CanonicalSource {
        if source.object.is_none() {
            if let Some(program) = self.main_program {
                return program.original_command(source.command).map_or(
                    CanonicalSource::Inserted(source),
                    |command| {
                        CanonicalSource::Existing(TeletextEnhancementSource { command, ..source })
                    },
                );
            }
        }
        if source
            .object
            .is_some_and(|object| self.additions.contains_key(&object))
        {
            return CanonicalSource::Inserted(source);
        }
        let Some(program) = source.object.and_then(|object| self.programs.get(&object)) else {
            return CanonicalSource::Existing(source);
        };
        program
            .original_command(source.command)
            .map_or(CanonicalSource::Inserted(source), |command| {
                CanonicalSource::Existing(TeletextEnhancementSource { command, ..source })
            })
    }

    pub(super) fn source_at_invocation(
        &self,
        source: TeletextEnhancementSource,
        invocation: Option<ersatzrs_domain::TeletextSpatialInvocationId>,
    ) -> CanonicalSource {
        if self.calls.inserted(invocation) {
            CanonicalSource::Inserted(source)
        } else {
            self.source(source)
        }
    }

    pub(super) fn canonical_invocation(
        &self,
        invocation: Option<ersatzrs_domain::TeletextSpatialInvocationId>,
    ) -> Option<ersatzrs_domain::TeletextSpatialInvocationId> {
        self.calls.original(invocation)
    }

    pub(super) fn same_source(
        &self,
        before: TeletextEnhancementSource,
        after: TeletextEnhancementSource,
    ) -> bool {
        self.source(after) == CanonicalSource::Existing(before)
    }

    pub(super) fn root_source(
        &self,
        root: TeletextSpatialRootSource,
    ) -> Option<TeletextSpatialRootSource> {
        match (root, self.main_program) {
            (TeletextSpatialRootSource::Local { command }, Some(program)) => program
                .original_command(command)
                .map(|command| TeletextSpatialRootSource::Local { command }),
            _ => Some(root),
        }
    }

    pub(super) fn same_attribute_source(
        &self,
        before: TeletextAttributeSource,
        after: TeletextAttributeSource,
    ) -> bool {
        if let (
            TeletextAttributeSource::PassiveDefault { invocation: before },
            TeletextAttributeSource::PassiveDefault { invocation: after },
        ) = (before, after)
        {
            return !self.calls.inserted(Some(after))
                && Some(before) == self.canonical_invocation(Some(after));
        }
        let (
            TeletextAttributeSource::Enhancement {
                event_ordinal: a_event,
                location: a_location,
                root_source: a_root,
                invocation: a_invocation,
                effective_start: a_start,
                placed_position: a_placed,
            },
            TeletextAttributeSource::Enhancement {
                event_ordinal: b_event,
                location: b_location,
                root_source: b_root,
                invocation: b_invocation,
                effective_start: b_start,
                placed_position: b_placed,
            },
        ) = (before, after)
        else {
            return before == after;
        };
        let same_event = self.events.as_ref().map_or(a_event == b_event, |events| {
            events.get(b_event).copied().flatten() == Some(a_event)
        });
        same_event
            && self.same_source(a_location, b_location)
            && Some(a_root) == self.root_source(b_root)
            && !self.calls.inserted(b_invocation)
            && a_invocation == self.canonical_invocation(b_invocation)
            && a_start == b_start
            && a_placed == b_placed
    }
}
fn event_key(
    root_source: TeletextSpatialRootSource,
    source: TeletextEnhancementSource,
    invocation: Option<ersatzrs_domain::TeletextSpatialInvocationId>,
) -> EventKey {
    let root = match root_source {
        TeletextSpatialRootSource::Local { command } => (0, command),
        TeletextSpatialRootSource::DefaultSlot { slot } => (1, usize::from(slot)),
    };
    (
        source.object,
        source.command,
        root,
        invocation.map(ersatzrs_domain::TeletextSpatialInvocationId::get),
    )
}
fn association() -> super::TeletextEditorialEffectsError {
    error(None, None, TeletextEditorialEffectsFailure::Association)
}
