//! Factory-owned final resource words. No resource/storage order is execution time.

use std::collections::{BTreeMap, BTreeSet};

use ersatzrs_domain::{
    TeletextAttributeTransmissionError, TeletextAttributeTransmissionEvidence,
    TeletextAttributeTransmissionPosition, TeletextEnhancementCommand,
    TeletextEnhancementObjectCatalog, TeletextObjectIdentity, TeletextObjectScope,
    TeletextResourceIdentity, TeletextSemanticService, TeletextSpatialEvent, TeletextSpatialTrace,
};

use super::enhancement_program::{LoweringState, ResourceContext};

use super::{
    EncodedTeletextAuthoredPage, TeletextMainComposition, TeletextMainCompositionError,
    TeletextMainCompositionFailure, TeletextMainCompositionSource,
    TeletextResourceObjectAllocations,
};

/// Exact producer correspondence failure, without invented resource ownership.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("resource transmission at {resource:?}, target {target:?}, command {command:?}: {reason}")]
pub struct TeletextResourceTransmissionError {
    pub resource: Option<TeletextResourceIdentity>,
    pub target: Option<TeletextObjectIdentity>,
    pub command: Option<usize>,
    pub reason: TeletextResourceTransmissionFailure,
}

/// Bounded correspondence rejection; resource ownership is optional when the
/// raw input fence fails before any particular object is examined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextResourceTransmissionFailure {
    #[error("{kind} count {count} exceeds trusted maximum {maximum}")]
    Budget {
        kind: &'static str,
        count: usize,
        maximum: usize,
    },
    #[error("resource transmission storage/work arithmetic exceeds supported range")]
    AllocationSize,
    #[error("encoded resource words, definition or allocation differ from the bound catalogue")]
    Correspondence,
    #[error(transparent)]
    Lowering(super::TeletextLocalProgramEncodingFailure),
}

type Error = TeletextResourceTransmissionError;
type Failure = TeletextResourceTransmissionFailure;

#[derive(Debug, Clone, Copy)]
struct Command {
    value: TeletextEnhancementCommand,
    emitted: bool,
    word: Option<usize>,
}

/// Borrows only pages encoded by the complete-service factory from its original
/// catalogue and global reservations. It is never caller-supplied word authority.
pub(super) struct ResourceTransmissionContext<'a> {
    catalog: &'a TeletextEnhancementObjectCatalog,
    allocations: &'a TeletextResourceObjectAllocations,
    pages: &'a [EncodedTeletextAuthoredPage],
    commands: BTreeMap<(TeletextObjectIdentity, usize), Command>,
    max_objects: usize,
}

impl<'a> ResourceTransmissionContext<'a> {
    pub(super) fn new(
        catalog: &'a TeletextEnhancementObjectCatalog,
        allocations: &'a TeletextResourceObjectAllocations,
        pages: &'a [EncodedTeletextAuthoredPage],
        max_commands: usize,
        max_objects: usize,
        max_pages: usize,
    ) -> Result<Self, Error> {
        preflight(
            catalog,
            allocations,
            pages,
            max_commands,
            max_objects,
            max_pages,
        )?;
        let mut context = Self {
            catalog,
            allocations,
            pages,
            commands: BTreeMap::new(),
            max_objects,
        };
        context.index_commands()?;
        context.bind_pages(context.pages)?;
        for (&(target, command), record) in &context.commands {
            if record.emitted != record.word.is_some() {
                return Err(correspondence(Some(target), Some(command)));
            }
        }
        Ok(context)
    }

    fn index_commands(&mut self) -> Result<(), Error> {
        for definition in self.catalog.definitions() {
            if definition.identity.scope() == TeletextObjectScope::Local {
                continue;
            }
            let mut state = LoweringState::new();
            for (ordinal, &value) in definition.program.commands().iter().enumerate() {
                // Reuse the actual resource lowering policy. This extra bounded
                // pass classifies real no-words; it never invents a timestamp.
                let words = state
                    .lower(
                        value,
                        Some(ResourceContext::object_definition(self.allocations)),
                    )
                    .map_err(|reason| Error {
                        reason: Failure::Lowering(reason),
                        ..correspondence(Some(definition.identity), Some(ordinal))
                    })?;
                self.commands.insert(
                    (definition.identity, ordinal),
                    Command {
                        value,
                        emitted: !words.words().is_empty(),
                        word: None,
                    },
                );
            }
        }
        Ok(())
    }

    fn bind_pages(&mut self, pages: &[EncodedTeletextAuthoredPage]) -> Result<(), Error> {
        let mut resources = BTreeSet::new();
        let mut targets = BTreeSet::new();
        for page in pages {
            let header = page
                .resource_header()
                .ok_or_else(|| correspondence(None, None))?;
            if !resources.insert(header.identity()) {
                return Err(Error {
                    resource: Some(header.identity()),
                    ..correspondence(None, None)
                });
            }
            let EncodedTeletextAuthoredPage::Object(page) = page else {
                continue;
            };
            for position in page.definition_positions() {
                let target = position.target();
                let definition = self
                    .catalog
                    .definition(target)
                    .ok_or_else(|| correspondence(Some(target), None))?;
                let allocation = self
                    .allocations
                    .lookup(target, definition.object_type)
                    .ok_or_else(|| correspondence(Some(target), None))?;
                if target.resource() != Some(header.identity())
                    || definition.object_type != position.object_type()
                    || allocation.object_number() != position.object_number()
                    || !targets.insert(target)
                {
                    return Err(correspondence(Some(target), None));
                }
            }
            for position in page.command_positions() {
                let source = position.source();
                let Some(target) = source.object else {
                    return Err(correspondence(None, Some(source.command)));
                };
                if target.resource() != Some(page.header().identity()) || !targets.contains(&target)
                {
                    return Err(correspondence(Some(target), Some(source.command)));
                }
                let record = self
                    .commands
                    .get_mut(&(target, source.command))
                    .ok_or_else(|| correspondence(Some(target), Some(source.command)))?;
                if !record.emitted
                    || position.first_word() > position.semantic_word()
                    || record.word.replace(position.semantic_word()).is_some()
                {
                    return Err(correspondence(Some(target), Some(source.command)));
                }
            }
        }
        for definition in self.catalog.definitions() {
            if definition.identity.resource().is_some() && !targets.contains(&definition.identity) {
                return Err(correspondence(Some(definition.identity), None));
            }
        }
        Ok(())
    }

    /// Generated definitions may be Local only. Every original resource body,
    /// type and exact pointer must still describe the pages this factory emits.
    pub(super) fn validate_catalog(
        &self,
        catalog: &TeletextEnhancementObjectCatalog,
        allocations: &TeletextResourceObjectAllocations,
    ) -> Result<(), Error> {
        bounded(
            "actual objects",
            catalog.definitions().len(),
            self.max_objects,
        )?;
        bounded(
            "actual allocations",
            allocations.entries().len(),
            self.max_objects,
        )?;
        for definition in catalog
            .definitions()
            .iter()
            .chain(self.catalog.definitions())
        {
            if definition.identity.scope() == TeletextObjectScope::Local {
                continue;
            }
            if catalog.definition(definition.identity)
                != self.catalog.definition(definition.identity)
                || allocations.lookup(definition.identity, definition.object_type)
                    != self
                        .allocations
                        .lookup(definition.identity, definition.object_type)
            {
                return Err(correspondence(Some(definition.identity), None));
            }
        }
        if allocations.entries() != self.allocations.entries() {
            let target = allocations
                .entries()
                .iter()
                .chain(self.allocations.entries())
                .find(|entry| {
                    allocations.lookup(entry.target(), entry.object_type())
                        != self.allocations.lookup(entry.target(), entry.object_type())
                })
                .map(super::TeletextResourceObjectAllocation::target);
            return Err(correspondence(target, None));
        }
        Ok(())
    }

    pub(super) fn word(
        &self,
        trace: &TeletextSpatialTrace,
        event: TeletextSpatialEvent,
        ordinal: usize,
    ) -> Result<Option<usize>, TeletextAttributeTransmissionError> {
        let fail = || TeletextAttributeTransmissionError::Correspondence { event: ordinal };
        let source = event.source();
        let target = source.object.ok_or_else(fail)?;
        let record = self
            .commands
            .get(&(target, source.command))
            .ok_or_else(fail)?;
        let invocation = event
            .invocation()
            .and_then(|id| trace.invocations().get(id.get()))
            .ok_or_else(fail)?;
        let definition = self.catalog.definition(target).ok_or_else(fail)?;
        if invocation.target() != target
            || invocation.object_type() != definition.object_type
            || event.command() != record.value
        {
            return Err(fail());
        }
        Ok(record.word)
    }
}

fn bounded(kind: &'static str, count: usize, maximum: usize) -> Result<(), Error> {
    if count > maximum {
        return Err(error(Failure::Budget {
            kind,
            count,
            maximum,
        }));
    }
    Ok(())
}

fn preflight(
    catalog: &TeletextEnhancementObjectCatalog,
    allocations: &TeletextResourceObjectAllocations,
    pages: &[EncodedTeletextAuthoredPage],
    max_commands: usize,
    max_objects: usize,
    max_pages: usize,
) -> Result<(), Error> {
    bounded("objects", catalog.definitions().len(), max_objects)?;
    bounded("allocations", allocations.entries().len(), max_objects)?;
    bounded("resource pages", pages.len(), max_pages)?;
    let commands = catalog
        .definitions()
        .iter()
        .try_fold(0usize, |sum, definition| {
            sum.checked_add(definition.program.commands().len())
        })
        .ok_or_else(|| error(Failure::AllocationSize))?;
    bounded("commands", commands, max_commands)?;
    let markers = pages
        .iter()
        .try_fold(0usize, |sum, page| {
            sum.checked_add(match page {
                EncodedTeletextAuthoredPage::Object(page) => page.definition_positions().len(),
                _ => 0,
            })
        })
        .ok_or_else(|| error(Failure::AllocationSize))?;
    bounded("resource definitions", markers, max_objects)?;
    let positions = pages
        .iter()
        .try_fold(0usize, |sum, page| {
            sum.checked_add(match page {
                EncodedTeletextAuthoredPage::Object(page) => page.command_positions().len(),
                _ => 0,
            })
        })
        .ok_or_else(|| error(Failure::AllocationSize))?;
    bounded("resource command positions", positions, max_commands)?;
    let storage = commands
        .checked_mul(std::mem::size_of::<((TeletextObjectIdentity, usize), Command)>() * 4)
        .and_then(|bytes| {
            markers
                .checked_add(pages.len())
                .and_then(|count| count.checked_mul(128))
                .and_then(|other| bytes.checked_add(other))
        });
    let records = catalog
        .definitions()
        .len()
        .checked_add(allocations.entries().len())
        .and_then(|count| count.checked_add(pages.len()))
        .and_then(|count| count.checked_add(commands))
        .and_then(|count| count.checked_add(positions))
        .and_then(|count| count.checked_add(markers));
    // BTree lookup/insert paths are at most the exact aggregate record count.
    // Fence arithmetic from actual bounded inputs rather than trusted maxima.
    let work = records.and_then(|count| {
        count
            .checked_add(1)
            .and_then(|factor| count.checked_mul(factor))
    });
    if storage.is_none_or(|bytes| isize::try_from(bytes).is_err()) || work.is_none() {
        return Err(error(Failure::AllocationSize));
    }
    Ok(())
}

fn error(reason: Failure) -> Error {
    Error {
        resource: None,
        target: None,
        command: None,
        reason,
    }
}
fn correspondence(target: Option<TeletextObjectIdentity>, command: Option<usize>) -> Error {
    Error {
        resource: target.and_then(TeletextObjectIdentity::resource),
        target,
        command,
        reason: Failure::Correspondence,
    }
}

/// Native default roots have no Local packet word. Only their exact resource
/// bodies gain same-instance timing evidence; sparse default provenance survives.
pub(super) fn settle_native(
    composition: TeletextMainComposition,
    service: &TeletextSemanticService,
    resources: &ResourceTransmissionContext<'_>,
    maximum: usize,
) -> Result<TeletextMainComposition, TeletextMainCompositionError> {
    let trace = composition.spatial_trace();
    let fail = |reason| TeletextMainCompositionError {
        owner: composition.owner(),
        location: match reason {
            TeletextAttributeTransmissionError::Correspondence { event } => {
                trace.events().get(event).and_then(|event| {
                    event
                        .source()
                        .object
                        .map(|identity| TeletextMainCompositionSource::Object {
                            identity,
                            command: event.source().command,
                        })
                })
            }
            _ => None,
        },
        reason: TeletextMainCompositionFailure::Transmission(reason),
    };
    if trace.events().len() > maximum {
        return Err(fail(TeletextAttributeTransmissionError::Budget { maximum }));
    }
    if trace
        .events()
        .len()
        .checked_mul(std::mem::size_of::<TeletextAttributeTransmissionPosition>())
        .is_none_or(|bytes| isize::try_from(bytes).is_err())
    {
        return Err(fail(TeletextAttributeTransmissionError::AllocationSize));
    }
    let mut positions = Vec::with_capacity(trace.events().len());
    for (event_ordinal, &event) in trace.events().iter().enumerate() {
        if event
            .source()
            .object
            .is_some_and(|target| target.scope() != TeletextObjectScope::Local)
        {
            if let Some(word) = resources.word(trace, event, event_ordinal).map_err(&fail)? {
                positions.push(TeletextAttributeTransmissionPosition {
                    event_ordinal,
                    source: event.source(),
                    invocation: event.invocation(),
                    word,
                });
            }
        }
    }
    let evidence =
        TeletextAttributeTransmissionEvidence::new(trace, &positions, maximum).map_err(fail)?;
    composition.with_transmission(service, &evidence, maximum)
}

#[cfg(test)]
#[path = "resource_transmission_tests.rs"]
mod tests;
