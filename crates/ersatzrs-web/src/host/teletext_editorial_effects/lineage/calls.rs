//! Changed call subtrees are new writers, never aliases for protected originals.
use super::{association, CanonicalSource, ProtectedLineage};
use crate::host::teletext_editorial_effects::{check_deadline, Result};
use ersatzrs_domain::{
    TeletextEnhancementCommand, TeletextObjectIdentity, TeletextSpatialInvocation,
    TeletextSpatialInvocationId, TeletextSpatialRootSource, TeletextSpatialTrace,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};

type Key = (
    Option<usize>,
    (u8, usize),
    Option<TeletextObjectIdentity>,
    usize,
);

fn root(source: TeletextSpatialRootSource) -> (u8, usize) {
    match source {
        TeletextSpatialRootSource::Local { command } => (0, command),
        TeletextSpatialRootSource::DefaultSlot { slot } => (1, usize::from(slot)),
    }
}
fn command(call: TeletextSpatialInvocation) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::InvokeObject {
        target: call.target(),
        object_type: call.object_type(),
        offset: call.offset(),
    }
}

fn inserted_source(call: TeletextSpatialInvocation, lineage: &ProtectedLineage<'_>) -> bool {
    let source = call.source();
    if let Some(definition) = source
        .object
        .and_then(|object| lineage.additions.get(&object))
    {
        return definition.program.commands().get(source.command) == Some(&command(call));
    }
    source
        .object
        .and_then(|object| lineage.programs.get(&object))
        .is_some_and(|program| match program {
            super::Program::Resource(resource) => {
                resource.has_invocation_structure_changes()
                    && resource.original_command(source.command).is_none()
                    && resource.replacement().commands().get(source.command) == Some(&command(call))
            }
            super::Program::Local(local) => {
                local.has_invocation_structure_changes()
                    && local.original_command(source.command).is_none()
                    && local.replacement().commands().get(source.command) == Some(&command(call))
            }
        })
}

fn omitted_source(call: TeletextSpatialInvocation, lineage: &ProtectedLineage<'_>) -> bool {
    let source = call.source();
    source
        .object
        .and_then(|object| lineage.programs.get(&object))
        .is_some_and(|program| match program {
            super::Program::Resource(resource) => {
                resource.has_invocation_structure_changes()
                    && resource.replacement_command(source.command).is_none()
                    && resource.expected().commands().get(source.command) == Some(&command(call))
            }
            super::Program::Local(local) => {
                local.has_invocation_structure_changes()
                    && local.replacement_command(source.command).is_none()
                    && local.expected().commands().get(source.command) == Some(&command(call))
            }
        })
}

fn check_retired(
    before: &TeletextSpatialTrace,
    used: &BTreeSet<usize>,
    retired: &mut BTreeSet<usize>,
    lineage: &ProtectedLineage<'_>,
    deadline: Instant,
) -> Result<()> {
    for call in before.invocations() {
        check_deadline(deadline)?;
        if call
            .parent()
            .is_some_and(|parent| retired.contains(&parent.get()))
            || (!used.contains(&call.id().get()) && omitted_source(*call, lineage))
        {
            retired.insert(call.id().get());
        }
        if !used.contains(&call.id().get()) && !retired.contains(&call.id().get()) {
            return Err(association());
        }
    }
    Ok(())
}

#[derive(Default)]
pub(super) struct Calls {
    originals: BTreeMap<usize, TeletextSpatialInvocationId>,
    inserted: BTreeSet<usize>,
}
impl Calls {
    pub(super) fn inserted(&self, id: Option<TeletextSpatialInvocationId>) -> bool {
        id.is_some_and(|id| self.inserted.contains(&id.get()))
    }
    pub(super) fn original(
        &self,
        id: Option<TeletextSpatialInvocationId>,
    ) -> Option<TeletextSpatialInvocationId> {
        id.map(|id| self.originals.get(&id.get()).copied().unwrap_or(id))
    }
    pub(super) fn new(
        before: &TeletextSpatialTrace,
        after: &TeletextSpatialTrace,
        lineage: &ProtectedLineage<'_>,
        deadline: Instant,
    ) -> Result<Self> {
        let mut old = BTreeMap::<Key, _>::new();
        for call in before.invocations() {
            check_deadline(deadline)?;
            let source = call.source();
            let key = (
                call.parent().map(TeletextSpatialInvocationId::get),
                root(call.root_source()),
                source.object,
                source.command,
            );
            if old.insert(key, *call).is_some() {
                return Err(association());
            }
        }
        let mut result = Self::default();
        let mut used = BTreeSet::new();
        let mut retired = BTreeSet::new();
        for call in after.invocations() {
            check_deadline(deadline)?;
            if result.inserted(call.parent()) {
                result.inserted.insert(call.id().get());
                continue;
            }
            let parent = match call.parent() {
                Some(parent) => Some(
                    result
                        .originals
                        .get(&parent.get())
                        .ok_or_else(association)?
                        .get(),
                ),
                None => None,
            };
            let source = match lineage.source(call.source()) {
                CanonicalSource::Existing(source) => source,
                CanonicalSource::Inserted(_) if inserted_source(*call, lineage) => {
                    result.inserted.insert(call.id().get());
                    continue;
                }
                CanonicalSource::Inserted(_) => return Err(association()),
            };
            let key = (
                parent,
                root(call.root_source()),
                source.object,
                source.command,
            );
            let original = old.get(&key).ok_or_else(association)?;
            if !used.insert(original.id().get())
                || result
                    .originals
                    .insert(call.id().get(), original.id())
                    .is_some()
            {
                return Err(association());
            }
            if command(*original) == command(*call) {
                continue;
            }
            let program = source
                .object
                .and_then(|object| lineage.programs.get(&object))
                .ok_or_else(association)?;
            if !program.has_invocation_changes()
                || program.expected().commands().get(source.command) != Some(&command(*original))
                || program.replacement().commands().get(call.source().command)
                    != Some(&command(*call))
            {
                return Err(association());
            }
            // A moved or retargeted callee and every descendant lose old writer
            // authority even if they happen to paint the same glyph or colour.
            result.inserted.insert(call.id().get());
            retired.insert(original.id().get());
        }
        check_retired(before, &used, &mut retired, lineage, deadline)?;
        Ok(result)
    }
}
