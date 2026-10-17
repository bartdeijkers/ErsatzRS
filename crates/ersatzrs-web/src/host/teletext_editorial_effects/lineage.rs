//! Exact before/after source correspondence for checked Local glyph structure.
//! Inserted commands never become missing provenance or an original source.
use super::{
    check_deadline, error,
    programs::{self, Program},
    Result, TeletextEditorialEdits, TeletextEditorialEffectsFailure,
};
use ersatzrs_domain::{
    TeletextAttributeSource, TeletextEnhancementSource, TeletextObjectIdentity,
    TeletextSpatialEvent, TeletextSpatialRootSource,
};
use ersatzrs_ffmpeg::teletext::{TeletextAuthoredServiceEncodingBudget, TeletextMainComposition};
use std::{collections::BTreeMap, time::Instant};

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
    events: Option<Vec<Option<usize>>>,
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
        for program in programs::all(edits).filter(|program| match program {
            Program::Local(value) => value.owner() == before.owner(),
            Program::Resource(_) => true,
        }) {
            check_deadline(deadline)?;
            if result
                .programs
                .insert(program.identity(), program)
                .is_some()
            {
                return Err(association());
            }
        }
        if result.programs.is_empty() {
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
            || old.invocations().len() != new.invocations().len()
        {
            return Err(association());
        }
        for (a, b) in old.invocations().iter().zip(new.invocations()) {
            check_deadline(deadline)?;
            // Glyph edits may move AP before a call. Their visible effects are
            // still checked at every protected cell; no call identity is changed.
            if a.id() != b.id()
                || a.parent() != b.parent()
                || a.target() != b.target()
                || a.object_type() != b.object_type()
                || a.offset() != b.offset()
                || a.root_source() != b.root_source()
                || result.source(b.source()) != CanonicalSource::Existing(a.source())
            {
                return Err(association());
            }
        }
        let mut indexed = BTreeMap::new();
        for (ordinal, event) in old.events().iter().enumerate() {
            check_deadline(deadline)?;
            if indexed
                .insert(event_key(*event, event.source()), ordinal)
                .is_some()
            {
                return Err(association());
            }
        }
        let mut events = Vec::with_capacity(new.events().len());
        for event in new.events() {
            check_deadline(deadline)?;
            let source = event.source();
            if let Some(program) = source
                .object
                .and_then(|object| result.programs.get(&object))
            {
                if program
                    .replacement()
                    .commands()
                    .get(source.command)
                    .is_none()
                {
                    return Err(association());
                }
            }
            events.push(match result.source(source) {
                CanonicalSource::Inserted(_) => None,
                CanonicalSource::Existing(source) => Some(
                    *indexed
                        .get(&event_key(*event, source))
                        .ok_or_else(association)?,
                ),
            });
        }
        result.events = Some(events);
        Ok(result)
    }

    pub(super) fn source(&self, source: TeletextEnhancementSource) -> CanonicalSource {
        let Some(program) = source.object.and_then(|object| self.programs.get(&object)) else {
            return CanonicalSource::Existing(source);
        };
        program
            .original_command(source.command)
            .map_or(CanonicalSource::Inserted(source), |command| {
                CanonicalSource::Existing(TeletextEnhancementSource { command, ..source })
            })
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
            && a_invocation == b_invocation
            && a_start == b_start
            && a_placed == b_placed
    }
}
fn event_key(event: TeletextSpatialEvent, source: TeletextEnhancementSource) -> EventKey {
    let root = match event.root_source() {
        TeletextSpatialRootSource::Local { command } => (0, command),
        TeletextSpatialRootSource::DefaultSlot { slot } => (1, usize::from(slot)),
    };
    (
        source.object,
        source.command,
        root,
        event
            .invocation()
            .map(ersatzrs_domain::TeletextSpatialInvocationId::get),
    )
}
fn association() -> super::TeletextEditorialEffectsError {
    error(None, None, TeletextEditorialEffectsFailure::Association)
}
