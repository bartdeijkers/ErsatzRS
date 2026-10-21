//! Checked correspondence to final emitted words, not receiver/publication authority.

use super::{
    TeletextAttributeSource, TeletextEffectiveProgramSource, TeletextEnhancementSource,
    TeletextObjectScope, TeletextSpatialInvocationId, TeletextSpatialRootSource,
    TeletextSpatialTrace,
};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

/// A producer's final semantic word, after all compatibility insertions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextAttributeTransmissionPosition {
    pub event_ordinal: usize,
    pub source: TeletextEnhancementSource,
    pub invocation: Option<TeletextSpatialInvocationId>,
    pub word: usize,
}

/// Immutable exact-trace evidence. The caller must derive words from the actual
/// final producer. Correspondence checking does not authenticate arbitrary input.
/// Ordinary sparse entries establish only emitter-local order. An explicit
/// checked same-page table may additionally compare exact Local emitters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextAttributeTransmissionEvidence {
    original: TeletextSpatialTrace,
    words: Vec<Option<usize>>,
    same_page_words: Option<Vec<Option<usize>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextAttributeTransmissionError {
    #[error("transmission evidence exceeds trusted command budget {maximum}")]
    Budget { maximum: usize },
    #[error("transmission event {event} has inconsistent or duplicate source/word")]
    Correspondence { event: usize },
    #[error("transmission evidence storage/work arithmetic exceeds supported range")]
    AllocationSize,
}

impl TeletextAttributeTransmissionEvidence {
    /// Validate bounded source uniqueness and retain the exact original trace.
    /// # Errors
    /// Rejects oversized input before indexing/allocation, unknown events,
    /// mismatched sources/instances and duplicate words within one emitter.
    pub fn new(
        original: &TeletextSpatialTrace,
        positions: &[TeletextAttributeTransmissionPosition],
        maximum: usize,
    ) -> Result<Self, TeletextAttributeTransmissionError> {
        if original.events().len() > maximum
            || original.invocations().len() > maximum
            || positions.len() > original.events().len()
        {
            return Err(TeletextAttributeTransmissionError::Budget { maximum });
        }
        let bounded = original
            .events()
            .len()
            .checked_mul(
                std::mem::size_of::<super::TeletextSpatialEvent>()
                    + std::mem::size_of::<Option<usize>>(),
            )
            .and_then(|bytes| {
                original
                    .invocations()
                    .len()
                    .checked_mul(std::mem::size_of::<super::TeletextSpatialInvocation>())
                    .and_then(|other| bytes.checked_add(other))
            })
            .and_then(|bytes| {
                positions
                    .len()
                    .checked_mul(std::mem::size_of::<TeletextAttributeTransmissionPosition>())
                    .and_then(|other| bytes.checked_add(other))
            });
        if bounded.is_none_or(|bytes| bytes > isize::MAX as usize) {
            return Err(TeletextAttributeTransmissionError::AllocationSize);
        }
        let mut words = vec![None; original.events().len()];
        let mut used = BTreeSet::new();
        for position in positions {
            let event = original.events().get(position.event_ordinal);
            if event.is_none_or(|event| {
                event.source() != position.source || event.invocation() != position.invocation
            }) || words[position.event_ordinal].is_some()
                || !used.insert((
                    position.invocation.map(TeletextSpatialInvocationId::get),
                    position.word,
                ))
            {
                return Err(TeletextAttributeTransmissionError::Correspondence {
                    event: position.event_ordinal,
                });
            }
            words[position.event_ordinal] = Some(position.word);
        }
        Ok(Self {
            original: original.clone(),
            words,
            same_page_words: None,
        })
    }

    /// Add actual absolute words from one explicit Local page's native packing.
    /// Ordinary positions retain their emitter-relative meaning. Every page
    /// entry must also be an ordinary witnessed event, with an exact Local root,
    /// owner and unambiguous instance. The native producer must authenticate
    /// these coordinates; this constructor does not authenticate packet bytes.
    /// # Errors
    /// Rejects oversized combined storage, foreign/default/resource sources,
    /// repeated object instances, missing ordinary witnesses and duplicate words.
    pub fn new_with_same_page_order(
        original: &TeletextSpatialTrace,
        positions: &[TeletextAttributeTransmissionPosition],
        page_positions: &[TeletextAttributeTransmissionPosition],
        maximum: usize,
    ) -> Result<Self, TeletextAttributeTransmissionError> {
        if page_positions.len() > original.events().len() || page_positions.len() > maximum {
            return Err(TeletextAttributeTransmissionError::Budget { maximum });
        }
        if page_positions.is_empty() {
            return Self::new(original, positions, maximum);
        }
        let combined = same_page_storage(original, positions.len(), page_positions.len());
        if combined.is_none_or(|bytes| bytes > isize::MAX as usize) {
            return Err(TeletextAttributeTransmissionError::AllocationSize);
        }
        let mut result = Self::new(original, positions, maximum)?;
        let eligible = same_page_events(original);
        let mut origins = BTreeMap::new();
        let mut words = vec![None; original.events().len()];
        let mut used = BTreeSet::new();
        for position in page_positions {
            let ordinal = position.event_ordinal;
            let event = original.events().get(ordinal);
            if event.is_none_or(|event| {
                event.source() != position.source || event.invocation() != position.invocation
            }) || result.words.get(ordinal).is_none_or(Option::is_none)
                || !eligible[ordinal]
                || !same_origin(position, result.words[ordinal], &mut origins)
                || words[ordinal].replace(position.word).is_some()
                || !used.insert(position.word)
            {
                return Err(TeletextAttributeTransmissionError::Correspondence { event: ordinal });
            }
        }
        result.same_page_words = Some(words);
        Ok(result)
    }

    #[must_use]
    pub fn matches(&self, original: &TeletextSpatialTrace) -> bool {
        &self.original == original
    }

    /// Compare original Enhancement writers within their exact emitter, or
    /// across explicitly witnessed native same-page coordinates. Base/default
    /// writers and arbitrary resource/storage order never gain time priority.
    #[must_use]
    pub fn compare(
        &self,
        incoming: TeletextAttributeSource,
        current: TeletextAttributeSource,
    ) -> Option<Ordering> {
        let incoming = self.event(incoming)?;
        let current = self.event(current)?;
        let first = self.original.events()[incoming];
        let second = self.original.events()[current];
        if first.invocation() != second.invocation()
            || first.source().object != second.source().object
        {
            let words = self.same_page_words.as_ref()?;
            return Some(words[incoming]?.cmp(&words[current]?));
        }
        if first.invocation().is_none()
            && !matches!(
                self.original.original_program_source(),
                super::TeletextEffectiveProgramSource::Local
            )
        {
            return None;
        }
        Some(self.words[incoming]?.cmp(&self.words[current]?))
    }

    fn event(&self, source: TeletextAttributeSource) -> Option<usize> {
        let TeletextAttributeSource::Enhancement {
            event_ordinal,
            location,
            root_source,
            invocation,
            ..
        } = source
        else {
            return None;
        };
        let event = self.original.events().get(event_ordinal)?;
        (event.source() == location
            && event.root_source() == root_source
            && event.invocation() == invocation)
            .then_some(event_ordinal)
    }
}

fn same_page_storage(
    original: &TeletextSpatialTrace,
    ordinary_count: usize,
    page_count: usize,
) -> Option<usize> {
    original
        .events()
        .len()
        .checked_mul(
            std::mem::size_of::<super::TeletextSpatialEvent>()
                + 2 * std::mem::size_of::<Option<usize>>(),
        )
        .and_then(|bytes| {
            original
                .invocations()
                .len()
                .checked_mul(std::mem::size_of::<super::TeletextSpatialInvocation>())
                .and_then(|other| bytes.checked_add(other))
        })
        .and_then(|bytes| {
            ordinary_count
                .checked_add(page_count)
                .and_then(|count| {
                    count.checked_mul(std::mem::size_of::<TeletextAttributeTransmissionPosition>())
                })
                .and_then(|other| bytes.checked_add(other))
        })
        .and_then(|bytes| {
            original
                .events()
                .len()
                .checked_mul(std::mem::size_of::<bool>())
                .and_then(|other| bytes.checked_add(other))
        })
        .and_then(|bytes| {
            original
                .invocations()
                .len()
                .checked_mul(std::mem::size_of::<(super::TeletextObjectIdentity, usize)>())
                .and_then(|other| bytes.checked_add(other))
        })
        .and_then(|bytes| {
            page_count
                .checked_mul(std::mem::size_of::<(usize, usize)>())
                .and_then(|other| bytes.checked_add(other))
        })
}

fn same_page_events(original: &TeletextSpatialTrace) -> Vec<bool> {
    if !matches!(
        original.original_program_source(),
        TeletextEffectiveProgramSource::Local
    ) {
        return vec![false; original.events().len()];
    }
    let mut counts = BTreeMap::new();
    for call in original.invocations() {
        *counts.entry(call.target()).or_insert(0_usize) += 1;
    }
    original
        .events()
        .iter()
        .map(|event| {
            if !matches!(event.root_source(), TeletextSpatialRootSource::Local { .. }) {
                return false;
            }
            match (event.source().object, event.invocation()) {
                (None, None) => true,
                (Some(target), Some(id)) => {
                    target.scope() == TeletextObjectScope::Local
                        && target.owner() == Some(original.owner())
                        && original
                            .invocations()
                            .get(id.get())
                            .is_some_and(|invocation| {
                                invocation.id() == id
                                    && invocation.target() == target
                                    && invocation.root_source() == event.root_source()
                            })
                        && counts.get(&target) == Some(&1)
                }
                _ => false,
            }
        })
        .collect()
}

fn same_origin(
    position: &TeletextAttributeTransmissionPosition,
    ordinary: Option<usize>,
    origins: &mut BTreeMap<usize, usize>,
) -> bool {
    let Some(origin) = ordinary.and_then(|word| position.word.checked_sub(word)) else {
        return false;
    };
    let Some(invocation) = position.invocation else {
        return origin == 0;
    };
    origins
        .insert(invocation.get(), origin)
        .is_none_or(|previous| previous == origin)
}

#[cfg(test)]
mod tests;
