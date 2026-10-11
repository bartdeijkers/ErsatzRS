//! Producer-paired invocation correspondence, not display admission or generation.
use super::{
    compile_teletext_physical_display_program, EncodedTeletextPhysicalDisplayProgram,
    TeletextAugmentedOriginProgram, TeletextAugmentedOriginSource, TeletextMainCompositionSource,
    TeletextPhysicalDisplayEncodingError, TeletextResourceObjectAllocations,
};
use ersatzrs_domain::{
    TeletextEnhancementCollectionBudget, TeletextEnhancementSource, TeletextObjectCatalogBudget,
    TeletextObjectType, TeletextPageAddress, TeletextSemanticService, TeletextSpatialInvocation,
    TeletextSpatialInvocationId, TeletextSpatialRootSource,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextAugmentedInvocationBudget {
    pub max_original_invocations: usize,
    pub max_actual_invocations: usize,
    pub max_carriers: usize,
    /// Ancestry scans including emitter lookup; physical replay has its own budget.
    pub max_work: usize,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextAugmentedInvocationSource {
    Retained {
        original: TeletextSpatialInvocationId,
    },
    Carrier {
        carrier: usize,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextAugmentedInvocation {
    actual: TeletextSpatialInvocationId,
    source: TeletextAugmentedInvocationSource,
}
impl TeletextAugmentedInvocation {
    #[must_use]
    pub const fn actual(self) -> TeletextSpatialInvocationId {
        self.actual
    }
    #[must_use]
    pub const fn source(self) -> TeletextAugmentedInvocationSource {
        self.source
    }
}
/// Exact borrowed candidate plus this factory's owned physical replay. These
/// mappings establish correspondence only, never display equivalence/admission.
#[derive(Debug)]
pub struct EncodedTeletextAugmentedOriginProgram<'a> {
    augmented: &'a TeletextAugmentedOriginProgram<'a>,
    physical: EncodedTeletextPhysicalDisplayProgram,
    invocations: Vec<TeletextAugmentedInvocation>,
    originals: Vec<Option<TeletextSpatialInvocationId>>,
    carriers: Vec<Option<TeletextSpatialInvocationId>>,
}
impl EncodedTeletextAugmentedOriginProgram<'_> {
    #[must_use]
    pub const fn augmented(&self) -> &TeletextAugmentedOriginProgram<'_> {
        self.augmented
    }
    #[must_use]
    pub const fn physical(&self) -> &EncodedTeletextPhysicalDisplayProgram {
        &self.physical
    }
    #[must_use]
    pub fn invocations(&self) -> &[TeletextAugmentedInvocation] {
        &self.invocations
    }
    #[must_use]
    pub fn actual_for_original(
        &self,
        original: TeletextSpatialInvocationId,
    ) -> Option<TeletextSpatialInvocationId> {
        self.originals.get(original.get()).copied().flatten()
    }
    #[must_use]
    pub fn actual_for_carrier(&self, carrier: usize) -> Option<TeletextSpatialInvocationId> {
        self.carriers.get(carrier).copied().flatten()
    }
    /// Extracts the owned diagnostic; no ancestry authority is transferred.
    #[must_use]
    pub fn into_physical(self) -> EncodedTeletextPhysicalDisplayProgram {
        self.physical
    }
}
#[derive(Debug, thiserror::Error)]
pub enum TeletextAugmentedOriginCompilationFailure {
    #[error("{kind} count {count} exceeds trusted maximum {maximum}")]
    InputBudget {
        kind: &'static str,
        count: usize,
        maximum: usize,
    },
    #[error("ancestry work exceeds trusted maximum {maximum}")]
    WorkBudget { maximum: usize },
    #[error("ancestry storage/work arithmetic exceeds supported range")]
    AllocationSize,
    #[error("actual invocation differs from its retained callsite or generated carrier")]
    Correspondence,
    #[error("not every original invocation or carrier occurred exactly once")]
    Coverage,
    #[error("physical compilation failed: {0}")]
    Physical(Box<TeletextPhysicalDisplayEncodingError>),
}
#[derive(Debug, thiserror::Error)]
#[error("augmented invocation compilation for {owner:?}, actual {actual:?}: {reason}")]
pub struct TeletextAugmentedOriginCompilationError {
    pub owner: TeletextPageAddress,
    /// Actual supplied-program ordinal, never mislabeled as an original source.
    pub location: Option<TeletextEnhancementSource>,
    pub root_source: Option<TeletextSpatialRootSource>,
    pub actual: Option<TeletextSpatialInvocationId>,
    pub original: Option<TeletextSpatialInvocationId>,
    pub reason: TeletextAugmentedOriginCompilationFailure,
}
/// Packs the exact private candidate, then matches calls from its own replay.
/// The immutable service/composition pairing remains a caller invariant as in
/// the physical factory. No selected-link/default ownership exemption is added.
/// Original/carrier raw fences precede packing; actual fences and storage/work
/// arithmetic precede ancestry allocations/scans. Packing has separate budgets.
/// # Errors
/// Returns existing physical failures or located correspondence/budget errors.
pub fn compile_teletext_augmented_origin_program<'a>(
    service: &TeletextSemanticService,
    augmented: &'a TeletextAugmentedOriginProgram<'a>,
    allocations: &TeletextResourceObjectAllocations,
    replay_budget: TeletextEnhancementCollectionBudget,
    packing_budget: TeletextObjectCatalogBudget,
    ancestry_budget: TeletextAugmentedInvocationBudget,
) -> Result<EncodedTeletextAugmentedOriginProgram<'a>, TeletextAugmentedOriginCompilationError> {
    let composition = augmented.placement().plan().composition();
    let original = composition.spatial_trace().invocations();
    let mut state = Ancestry {
        augmented,
        budget: ancestry_budget,
        work: 0,
    };
    state
        .limit(
            "original invocations",
            original.len(),
            ancestry_budget.max_original_invocations,
        )
        .map_err(|mut error| {
            error.original = original
                .get(ancestry_budget.max_original_invocations)
                .map(|call| call.id());
            error
        })?;
    state.limit(
        "carriers",
        augmented.carriers().len(),
        ancestry_budget.max_carriers,
    )?;
    let physical = compile_teletext_physical_display_program(
        service,
        composition,
        augmented.program(),
        augmented.catalog(),
        allocations,
        replay_budget,
        packing_budget,
    )
    .map_err(|error| {
        state.error(
            None,
            None,
            TeletextAugmentedOriginCompilationFailure::Physical(Box::new(error)),
        )
    })?;
    let actual = physical.replay().spatial_trace().invocations();
    state
        .limit(
            "actual invocations",
            actual.len(),
            ancestry_budget.max_actual_invocations,
        )
        .map_err(|mut error| {
            if let Some(call) = actual.get(ancestry_budget.max_actual_invocations) {
                error.actual = Some(call.id());
                error.location = Some(call.source());
                error.root_source = Some(call.root_source());
            }
            error
        })?;
    state.storage(original.len(), actual.len())?;
    // At least one visit per output/correspondence entry is unavoidable. Fence
    // this allocation-related minimum first; detailed scans are charged below.
    let minimum = actual
        .len()
        .checked_add(original.len())
        .and_then(|count| count.checked_add(augmented.carriers().len()))
        .ok_or_else(|| {
            state.error(
                None,
                None,
                TeletextAugmentedOriginCompilationFailure::AllocationSize,
            )
        })?;
    if minimum > ancestry_budget.max_work {
        return Err(state.error(
            actual.first().copied(),
            None,
            TeletextAugmentedOriginCompilationFailure::WorkBudget {
                maximum: ancestry_budget.max_work,
            },
        ));
    }
    let maps = state.map_all(original, actual)?;
    Ok(EncodedTeletextAugmentedOriginProgram {
        augmented,
        physical,
        invocations: maps.records,
        originals: maps.originals,
        carriers: maps.carriers,
    })
}

fn matches_original(
    old: TeletextSpatialInvocation,
    source: TeletextAugmentedOriginSource,
    parent: Option<TeletextAugmentedInvocation>,
) -> bool {
    match (source, parent.map(|value| value.source)) {
        (TeletextAugmentedOriginSource::RetainedMain { provenance, .. }, None)
            if old.parent().is_none() && old.source().object.is_none() =>
        {
            match provenance {
                TeletextMainCompositionSource::OriginalPage { command } => {
                    old.source().command == command
                        && old.root_source() == (TeletextSpatialRootSource::Local { command })
                }
                TeletextMainCompositionSource::DefaultSlot { slot } => {
                    old.root_source() == (TeletextSpatialRootSource::DefaultSlot { slot })
                }
                _ => false,
            }
        }
        (
            TeletextAugmentedOriginSource::RetainedObject { identity, command },
            Some(TeletextAugmentedInvocationSource::Retained { original }),
        ) => {
            old.parent() == Some(original)
                && old.source()
                    == (TeletextEnhancementSource {
                        object: Some(identity),
                        command,
                    })
        }
        _ => false,
    }
}
fn same_facts(old: TeletextSpatialInvocation, actual: TeletextSpatialInvocation) -> bool {
    old.target() == actual.target()
        && old.object_type() == actual.object_type()
        && old.origin() == actual.origin()
        && old.caller_position() == actual.caller_position()
        && old.offset() == actual.offset()
}
struct InvocationMaps {
    records: Vec<TeletextAugmentedInvocation>,
    originals: Vec<Option<TeletextSpatialInvocationId>>,
    carriers: Vec<Option<TeletextSpatialInvocationId>>,
}
struct Ancestry<'a> {
    augmented: &'a TeletextAugmentedOriginProgram<'a>,
    budget: TeletextAugmentedInvocationBudget,
    work: usize,
}
impl Ancestry<'_> {
    fn map_all(
        &mut self,
        original: &[TeletextSpatialInvocation],
        actual: &[TeletextSpatialInvocation],
    ) -> Result<InvocationMaps, TeletextAugmentedOriginCompilationError> {
        let mut maps = InvocationMaps {
            records: Vec::with_capacity(actual.len()),
            originals: vec![None; original.len()],
            carriers: vec![None; self.augmented.carriers().len()],
        };
        for (ordinal, &call) in actual.iter().enumerate() {
            self.charge(1, Some(call))?;
            let parent = self.caller(call, ordinal, actual, &maps.records)?;
            let source = self.map_call(call, original, parent, &mut maps)?;
            maps.records.push(TeletextAugmentedInvocation {
                actual: call.id(),
                source,
            });
        }
        self.coverage(original, &maps)?;
        Ok(maps)
    }
    fn caller(
        &self,
        call: TeletextSpatialInvocation,
        ordinal: usize,
        actual: &[TeletextSpatialInvocation],
        records: &[TeletextAugmentedInvocation],
    ) -> Result<Option<TeletextAugmentedInvocation>, TeletextAugmentedOriginCompilationError> {
        let fail = || {
            self.error(
                Some(call),
                None,
                TeletextAugmentedOriginCompilationFailure::Correspondence,
            )
        };
        if call.id().get() != ordinal {
            return Err(fail());
        }
        if let Some(parent) = call.parent() {
            if parent.get() >= ordinal {
                return Err(fail());
            }
            let parent_call = actual.get(parent.get()).ok_or_else(fail)?;
            let mapped = records.get(parent.get()).copied().ok_or_else(fail)?;
            if call.source().object != Some(parent_call.target())
                || call.root_source() != parent_call.root_source()
            {
                return Err(fail());
            }
            Ok(Some(mapped))
        } else {
            if call.source().object.is_some()
                || call.root_source()
                    != (TeletextSpatialRootSource::Local {
                        command: call.source().command,
                    })
            {
                return Err(fail());
            }
            Ok(None)
        }
    }
    fn map_call(
        &mut self,
        call: TeletextSpatialInvocation,
        original: &[TeletextSpatialInvocation],
        parent: Option<TeletextAugmentedInvocation>,
        maps: &mut InvocationMaps,
    ) -> Result<TeletextAugmentedInvocationSource, TeletextAugmentedOriginCompilationError> {
        self.charge(self.lookup_cost()?, Some(call))?;
        match self.augmented.emitter_source(call.source()) {
            Some(TeletextAugmentedOriginSource::CarrierCall { carrier }) if parent.is_none() => {
                self.carrier(call, carrier)?;
                let entry = maps.carriers.get_mut(carrier).ok_or_else(|| {
                    self.error(
                        Some(call),
                        None,
                        TeletextAugmentedOriginCompilationFailure::Correspondence,
                    )
                })?;
                if entry.replace(call.id()).is_some() {
                    return Err(self.error(
                        Some(call),
                        None,
                        TeletextAugmentedOriginCompilationFailure::Coverage,
                    ));
                }
                Ok(TeletextAugmentedInvocationSource::Carrier { carrier })
            }
            Some(
                source @ (TeletextAugmentedOriginSource::RetainedMain { .. }
                | TeletextAugmentedOriginSource::RetainedObject { .. }),
            ) => {
                let index = self.retained(call, original, source, parent)?;
                let old = original[index];
                if maps.originals[index].replace(call.id()).is_some() {
                    return Err(self.error(
                        Some(call),
                        Some(old.id()),
                        TeletextAugmentedOriginCompilationFailure::Coverage,
                    ));
                }
                Ok(TeletextAugmentedInvocationSource::Retained { original: old.id() })
            }
            _ => Err(self.error(
                Some(call),
                None,
                TeletextAugmentedOriginCompilationFailure::Correspondence,
            )),
        }
    }
    fn retained(
        &mut self,
        call: TeletextSpatialInvocation,
        original: &[TeletextSpatialInvocation],
        source: TeletextAugmentedOriginSource,
        parent: Option<TeletextAugmentedInvocation>,
    ) -> Result<usize, TeletextAugmentedOriginCompilationError> {
        let mut found = None;
        for (index, &old) in original.iter().enumerate() {
            self.charge(1, Some(call))?;
            if matches_original(old, source, parent) && found.replace(index).is_some() {
                return Err(self.error(
                    Some(call),
                    Some(old.id()),
                    TeletextAugmentedOriginCompilationFailure::Correspondence,
                ));
            }
        }
        let index = found.ok_or_else(|| {
            self.error(
                Some(call),
                None,
                TeletextAugmentedOriginCompilationFailure::Correspondence,
            )
        })?;
        let old = original[index];
        if old.id().get() != index || !same_facts(old, call) {
            return Err(self.error(
                Some(call),
                Some(old.id()),
                TeletextAugmentedOriginCompilationFailure::Correspondence,
            ));
        }
        Ok(index)
    }
    fn coverage(
        &mut self,
        original: &[TeletextSpatialInvocation],
        maps: &InvocationMaps,
    ) -> Result<(), TeletextAugmentedOriginCompilationError> {
        for (index, &entry) in maps.originals.iter().enumerate() {
            self.charge(1, None)?;
            if entry.is_none() {
                return Err(self.error(
                    None,
                    original.get(index).map(|call| call.id()),
                    TeletextAugmentedOriginCompilationFailure::Coverage,
                ));
            }
        }
        for &entry in &maps.carriers {
            self.charge(1, None)?;
            if entry.is_none() {
                return Err(self.error(
                    None,
                    None,
                    TeletextAugmentedOriginCompilationFailure::Coverage,
                ));
            }
        }
        Ok(())
    }

    fn error(
        &self,
        actual: Option<TeletextSpatialInvocation>,
        original: Option<TeletextSpatialInvocationId>,
        reason: TeletextAugmentedOriginCompilationFailure,
    ) -> TeletextAugmentedOriginCompilationError {
        TeletextAugmentedOriginCompilationError {
            owner: self.augmented.placement().plan().composition().owner(),
            location: actual.map(TeletextSpatialInvocation::source),
            root_source: actual.map(TeletextSpatialInvocation::root_source),
            actual: actual.map(TeletextSpatialInvocation::id),
            original,
            reason,
        }
    }
    fn limit(
        &self,
        kind: &'static str,
        count: usize,
        maximum: usize,
    ) -> Result<(), TeletextAugmentedOriginCompilationError> {
        if count > maximum {
            return Err(self.error(
                None,
                None,
                TeletextAugmentedOriginCompilationFailure::InputBudget {
                    kind,
                    count,
                    maximum,
                },
            ));
        }
        Ok(())
    }
    fn lookup_cost(&self) -> Result<usize, TeletextAugmentedOriginCompilationError> {
        self.augmented
            .carriers()
            .len()
            .checked_add(self.augmented.placement().catalog().definitions().len())
            .and_then(|count| count.checked_add(1))
            .ok_or_else(|| {
                self.error(
                    None,
                    None,
                    TeletextAugmentedOriginCompilationFailure::AllocationSize,
                )
            })
    }
    fn storage(
        &self,
        original: usize,
        actual: usize,
    ) -> Result<(), TeletextAugmentedOriginCompilationError> {
        let bytes = original
            .checked_add(self.augmented.carriers().len())
            .and_then(|count| {
                count.checked_mul(std::mem::size_of::<Option<TeletextSpatialInvocationId>>())
            })
            .and_then(|bytes| {
                actual
                    .checked_mul(std::mem::size_of::<TeletextAugmentedInvocation>())
                    .and_then(|records| bytes.checked_add(records))
            });
        let work = original
            .checked_add(self.lookup_cost()?)
            .and_then(|count| count.checked_add(1))
            .and_then(|count| count.checked_mul(actual))
            .and_then(|count| count.checked_add(original))
            .and_then(|count| count.checked_add(self.augmented.carriers().len()));
        if bytes.is_none_or(|bytes| bytes > isize::MAX as usize) || work.is_none() {
            return Err(self.error(
                None,
                None,
                TeletextAugmentedOriginCompilationFailure::AllocationSize,
            ));
        }
        // Arithmetic is fenced before allocation; only visited items consume work.
        Ok(())
    }
    fn charge(
        &mut self,
        count: usize,
        call: Option<TeletextSpatialInvocation>,
    ) -> Result<(), TeletextAugmentedOriginCompilationError> {
        self.work = self.work.checked_add(count).ok_or_else(|| {
            self.error(
                call,
                None,
                TeletextAugmentedOriginCompilationFailure::AllocationSize,
            )
        })?;
        if self.work > self.budget.max_work {
            return Err(self.error(
                call,
                None,
                TeletextAugmentedOriginCompilationFailure::WorkBudget {
                    maximum: self.budget.max_work,
                },
            ));
        }
        Ok(())
    }
    fn carrier(
        &self,
        call: TeletextSpatialInvocation,
        index: usize,
    ) -> Result<(), TeletextAugmentedOriginCompilationError> {
        let fail = || {
            self.error(
                Some(call),
                None,
                TeletextAugmentedOriginCompilationFailure::Correspondence,
            )
        };
        let value = self.augmented.carriers().get(index).ok_or_else(fail)?;
        let component = self
            .augmented
            .placement()
            .components()
            .get(value.component_index())
            .ok_or_else(fail)?;
        let gap = component.gap().ok_or_else(fail)?;
        let placement = component.calls().get(value.call_index()).ok_or_else(fail)?;
        let raw = placement.raw_position();
        let caller = call.caller_position();
        let origin = call.origin();
        if call.target() != value.identity()
            || call.object_type() != TeletextObjectType::Passive
            || placement.original_origin_index() != value.original_origin_index()
            || (caller.row(), caller.column()) != gap.caller_position()
            || (origin.row(), origin.column()) != (u16::from(raw.0), u16::from(raw.1))
            || call.offset() != Some(placement.offset())
        {
            return Err(fail());
        }
        Ok(())
    }
}
