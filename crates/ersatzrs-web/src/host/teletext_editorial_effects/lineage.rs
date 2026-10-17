//! Exact before/after source correspondence for checked Local glyph structure.
//! Inserted commands never become missing provenance or an original source.
use super::{
    check_deadline, error,
    programs::{self, Program},
    Result, TeletextEditorialEdits, TeletextEditorialEffectsFailure,
};
use ersatzrs_domain::{
    TeletextAttributeSource, TeletextEnhancementObjectDefinition, TeletextEnhancementSource,
    TeletextObjectIdentity, TeletextSpatialEvent, TeletextSpatialRootSource,
};
use ersatzrs_ffmpeg::teletext::{TeletextAuthoredServiceEncodingBudget, TeletextMainComposition};
use std::{collections::BTreeMap, time::Instant};

mod calls;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CanonicalSource {
    Existing(TeletextEnhancementSource),
    Inserted(TeletextEnhancementSource),
}

type EventKey = (
    Option<TeletextObjectIdentity>,
    usize,
    (u8, usize),
    Option<usize>,
);

#[derive(Default)]
pub(super) struct ProtectedLineage<'a> {
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
        let mut result = Self::default();
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
        for program in programs::all(edits).filter(|program| match program {
            Program::Local(value) => value.owner() == before.owner(),
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
        if result.programs.is_empty() && result.additions.is_empty() {
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
                    event_key(*event, event.source(), event.invocation()),
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
                                *event,
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

    fn check_source_command(&self, source: TeletextEnhancementSource) -> Result<()> {
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
            && a_root == b_root
            && !self.calls.inserted(b_invocation)
            && a_invocation == self.canonical_invocation(b_invocation)
            && a_start == b_start
            && a_placed == b_placed
    }
}
fn event_key(
    event: TeletextSpatialEvent,
    source: TeletextEnhancementSource,
    invocation: Option<ersatzrs_domain::TeletextSpatialInvocationId>,
) -> EventKey {
    let root = match event.root_source() {
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
