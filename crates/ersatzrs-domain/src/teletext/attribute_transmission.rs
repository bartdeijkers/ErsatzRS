//! Checked correspondence to final emitted words, not receiver/publication authority.

use super::{
    TeletextAttributeSource, TeletextEnhancementSource, TeletextSpatialInvocationId,
    TeletextSpatialTrace,
};
use std::cmp::Ordering;
use std::collections::BTreeSet;

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
/// Sparse entries never establish order between different executing instances.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextAttributeTransmissionEvidence {
    original: TeletextSpatialTrace,
    words: Vec<Option<usize>>,
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
        })
    }

    #[must_use]
    pub fn matches(&self, original: &TeletextSpatialTrace) -> bool {
        &self.original == original
    }

    /// Compare only original Enhancement writers from the same exact executing
    /// instance (or original Local main). Base/default writers cannot gain time
    /// priority here. Arbitrary resource/storage order is never compared.
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
            return None;
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
