//! Bounded diagnostics owned by actual output lifetimes, isolated per host.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

use ersatzrs_app::TeletextOutputObserver;
use ersatzrs_domain::{
    TeletextChannelOutputObservations, TeletextCustomFieldFailure, TeletextOutputFailure,
    TeletextOutputFailureKind, TeletextOutputObservation, TeletextOutputObservationPhase,
};

// Diagnostic storage only. Reaching this bound never refuses ordinary playback.
const MAX_OBSERVATIONS: usize = 256;

#[derive(Default, Clone)]
pub(super) struct OutputObservations(Arc<Registry>);

#[derive(Default)]
struct Registry {
    entries: Mutex<BTreeMap<uuid::Uuid, (i64, TeletextOutputObservation)>>,
    overflow: AtomicUsize,
    // At most one per channel, so bounded by the number of channels; it
    // outlives the failed output so the admin can still explain the stop.
    failures: Mutex<BTreeMap<i64, TeletextOutputFailure>>,
    field_failures: Mutex<BTreeMap<i64, TeletextCustomFieldFailure>>,
}

struct Owner {
    registry: Arc<Registry>,
    channel_id: i64,
    epoch: uuid::Uuid,
    retained: bool,
}

impl OutputObservations {
    pub(super) fn register(
        &self,
        channel_id: i64,
        initial: TeletextOutputObservation,
    ) -> Option<Box<dyn TeletextOutputObserver>> {
        if channel_id <= 0 {
            return None;
        }
        let mut entries = self.0.entries.lock().ok()?;
        if entries.contains_key(&initial.epoch) {
            return None;
        }
        let epoch = initial.epoch;
        let retained = entries.len() < MAX_OBSERVATIONS;
        if retained {
            entries.insert(epoch, (channel_id, initial));
        } else {
            // Relaxed is sufficient: this counter conveys incompleteness only,
            // not ownership/publication authority or access to entry contents.
            self.0.overflow.fetch_add(1, Ordering::Relaxed);
        }
        Some(Box::new(Owner {
            registry: self.0.clone(),
            channel_id,
            epoch,
            retained,
        }))
    }

    pub(super) fn snapshot(&self, channel_id: i64) -> Option<TeletextChannelOutputObservations> {
        let last_failure = self.0.failures.lock().ok()?.get(&channel_id).cloned();
        let last_field_failure = self.0.field_failures.lock().ok()?.get(&channel_id).cloned();
        let entries = self.0.entries.lock().ok()?;
        Some(TeletextChannelOutputObservations {
            outputs: entries
                .values()
                .filter(|(channel, _)| *channel == channel_id)
                .map(|(_, observation)| observation.clone())
                .collect(),
            // Conservatively report host-wide overflow, even on another channel.
            incomplete: self.0.overflow.load(Ordering::Relaxed) != 0,
            last_failure,
            last_field_failure,
        })
    }
}

impl OutputObservations {
    pub(super) fn field_failures(&self) -> Vec<(i64, TeletextCustomFieldFailure)> {
        self.0.field_failures.lock().map_or_else(
            |_| Vec::new(),
            |failures| {
                failures
                    .iter()
                    .map(|(id, failure)| (*id, failure.clone()))
                    .collect()
            },
        )
    }

    /// Channels whose last authored output stopped on a transport failure.
    pub(super) fn failures(&self) -> Vec<(i64, TeletextOutputFailure)> {
        self.0.failures.lock().map_or_else(
            |_| Vec::new(),
            |failures| {
                failures
                    .iter()
                    .map(|(channel, failure)| (*channel, failure.clone()))
                    .collect()
            },
        )
    }
}

impl TeletextOutputObserver for Owner {
    fn record(&self, observation: TeletextOutputObservation) {
        if !self.retained || observation.epoch != self.epoch {
            return;
        }
        let delivered = observation.phase == TeletextOutputObservationPhase::Applied
            && observation
                .applied
                .as_ref()
                .is_some_and(|applied| applied.authored);
        if let Ok(mut entries) = self.registry.entries.lock() {
            if let Some((channel, entry)) = entries.get_mut(&self.epoch) {
                if *channel == self.channel_id {
                    // Only a new sink receipt changes retained field health.
                    // A request with an old applied value cannot imply recovery.
                    let acknowledged =
                        observation.applied.is_some() && entry.applied != observation.applied;
                    *entry = observation;
                    if acknowledged {
                        self.record_field_delivery(&entries);
                    }
                }
            }
        }
        if delivered {
            // A later output delivered teletext again: the old stop is history.
            if let Ok(mut failures) = self.registry.failures.lock() {
                failures.remove(&self.channel_id);
            }
        }
    }

    fn failed(&self, kind: TeletextOutputFailureKind) {
        if let Ok(mut failures) = self.registry.failures.lock() {
            failures.insert(
                self.channel_id,
                TeletextOutputFailure {
                    kind,
                    at: chrono::Utc::now(),
                },
            );
        }
    }
}

impl Owner {
    fn record_field_delivery(
        &self,
        entries: &BTreeMap<uuid::Uuid, (i64, TeletextOutputObservation)>,
    ) {
        let pages: BTreeSet<_> = entries
            .values()
            .filter(|(channel, _)| *channel == self.channel_id)
            .filter_map(|(_, entry)| entry.applied.as_ref())
            .filter(|selection| selection.authored)
            .filter_map(|selection| selection.generation.as_ref())
            .filter_map(|generation| generation.custom_fields.as_ref())
            .flat_map(|fields| fields.failed_pages.iter().copied())
            .collect();
        if let Ok(mut failures) = self.registry.field_failures.lock() {
            if pages.is_empty() {
                failures.remove(&self.channel_id);
            } else {
                failures.insert(
                    self.channel_id,
                    TeletextCustomFieldFailure {
                        pages: pages.into_iter().collect(),
                        at: chrono::Utc::now(),
                    },
                );
            }
        }
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        if self.retained {
            if let Ok(mut entries) = self.registry.entries.lock() {
                entries.remove(&self.epoch);
            }
        } else {
            self.registry.overflow.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_domain::{
        TeletextOutputObservationPhase as Phase, TeletextOutputSelectionObservation,
    };

    fn initial() -> TeletextOutputObservation {
        TeletextOutputObservation {
            epoch: uuid::Uuid::new_v4(),
            requested: TeletextOutputSelectionObservation {
                sequence: 0,
                generation: None,
                authored: false,
            },
            applied: None,
            phase: Phase::Waiting,
        }
    }

    #[test]
    fn observations_are_owned_isolated_and_cannot_be_replaced_by_another_epoch() {
        let registry = OutputObservations::default();
        let first = initial();
        let owner = registry.register(1, first.clone()).unwrap();
        assert!(registry.register(2, first.clone()).is_none());
        let second = registry.register(1, initial()).unwrap();
        let other_channel = registry.register(2, initial()).unwrap();
        let mut applied = first.clone();
        applied.phase = Phase::Applied;
        applied.applied = Some(applied.requested.clone());
        owner.record(applied.clone());
        owner.record(initial());
        assert!(registry.snapshot(1).unwrap().outputs.contains(&applied));
        assert_eq!(registry.snapshot(1).unwrap().outputs.len(), 2);
        assert_eq!(registry.snapshot(2).unwrap().outputs.len(), 1);
        assert!(OutputObservations::default()
            .snapshot(1)
            .unwrap()
            .outputs
            .is_empty());
        drop(owner);
        assert_eq!(registry.snapshot(1).unwrap().outputs.len(), 1);
        drop((second, other_channel));
        assert!(registry.snapshot(1).unwrap().outputs.is_empty());
        assert!(registry.snapshot(2).unwrap().outputs.is_empty());
    }

    #[test]
    fn a_terminal_failure_outlives_its_output_until_teletext_is_delivered_again() {
        let registry = OutputObservations::default();
        let failed = registry.register(1, initial()).unwrap();
        let other = registry.register(2, initial()).unwrap();
        failed.failed(TeletextOutputFailureKind::TransportCapacity);
        drop(failed);
        let snapshot = registry.snapshot(1).unwrap();
        assert!(snapshot.outputs.is_empty());
        assert_eq!(
            snapshot.last_failure.unwrap().kind,
            TeletextOutputFailureKind::TransportCapacity
        );
        // Channel isolation: another channel never inherits the failure.
        assert!(registry.snapshot(2).unwrap().last_failure.is_none());
        let mut ordinary = initial();
        ordinary.phase = Phase::Applied;
        ordinary.applied = Some(ordinary.requested.clone());
        other.record(ordinary);
        let mut delivered = initial();
        let retry = registry.register(1, delivered.clone()).unwrap();
        delivered.requested.authored = true;
        delivered.phase = Phase::Applied;
        delivered.applied = Some(delivered.requested.clone());
        retry.record(delivered);
        assert!(registry.snapshot(1).unwrap().last_failure.is_none());
    }

    #[tokio::test]
    async fn bounded_diagnostics_preserve_overflow_visibility_and_cancellation_cleanup() {
        let registry = OutputObservations::default();
        let owners: Vec<_> = (0..MAX_OBSERVATIONS)
            .map(|_| registry.register(1, initial()).unwrap())
            .collect();
        let overflow = registry.register(2, initial()).unwrap();
        assert_eq!(
            registry.snapshot(1).unwrap().outputs.len(),
            MAX_OBSERVATIONS
        );
        assert!(registry.snapshot(1).unwrap().incomplete);
        assert!(registry.snapshot(2).unwrap().outputs.is_empty());
        let task = tokio::spawn(async move {
            let _owner = overflow;
            std::future::pending::<()>().await;
        });
        tokio::task::yield_now().await;
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(!registry.snapshot(1).unwrap().incomplete);
        drop(owners);
        assert!(registry.snapshot(1).unwrap().outputs.is_empty());
    }
    fn field_output(pages: &[u16]) -> TeletextOutputObservation {
        let mut output = initial();
        output.requested.authored = true;
        output.requested.generation = Some(ersatzrs_domain::TeletextOutputGenerationObservation {
            generation_id: 3,
            request_id: 2,
            design_id: 1,
            revision: 1,
            effective: false,
            guide_refresh: None,
            custom_fields: Some(ersatzrs_domain::TeletextCustomFieldOutputObservation {
                revision: 1,
                failed_pages: pages.to_vec(),
            }),
        });
        output
    }

    #[test]
    fn field_failure_requires_a_new_receipt_and_survives_proposals_and_output_end() {
        let registry = OutputObservations::default();
        let mut output = field_output(&[150, 151]);
        let owner = registry.register(7, output.clone()).unwrap();
        owner.record(output.clone());
        assert!(registry.snapshot(7).unwrap().last_field_failure.is_none());
        // A superseded receipt still describes actual delivery, not the latest request.
        output.applied = Some(output.requested.clone());
        output.requested.sequence = 2;
        output
            .requested
            .generation
            .as_mut()
            .unwrap()
            .custom_fields
            .as_mut()
            .unwrap()
            .failed_pages
            .clear();
        owner.record(output.clone());
        assert_eq!(
            registry
                .snapshot(7)
                .unwrap()
                .last_field_failure
                .unwrap()
                .pages,
            [150, 151]
        );
        output.phase = Phase::Rejected;
        owner.record(output.clone());
        assert_eq!(
            registry
                .snapshot(7)
                .unwrap()
                .last_field_failure
                .unwrap()
                .pages,
            [150, 151]
        );
        drop(owner);
        assert_eq!(
            registry
                .snapshot(7)
                .unwrap()
                .last_field_failure
                .unwrap()
                .pages,
            [150, 151]
        );
        assert!(registry.snapshot(8).unwrap().last_field_failure.is_none());
        let mut recovered = field_output(&[]);
        let retry = registry.register(7, recovered.clone()).unwrap();
        // Historical failure remains while a healthy request is merely waiting.
        assert!(registry.snapshot(7).unwrap().last_field_failure.is_some());
        recovered.applied = Some(recovered.requested.clone());
        recovered.phase = Phase::Applied;
        retry.record(recovered);
        assert!(registry.snapshot(7).unwrap().last_field_failure.is_none());
    }

    #[test]
    fn a_healthy_viewer_cannot_hide_another_active_viewers_confirmed_blank_fields() {
        let registry = OutputObservations::default();
        let mut failed = field_output(&[150]);
        let owner = registry.register(7, failed.clone()).unwrap();
        failed.applied = Some(failed.requested.clone());
        failed.phase = Phase::Applied;
        owner.record(failed);
        let mut healthy = field_output(&[]);
        let other = registry.register(7, healthy.clone()).unwrap();
        healthy.applied = Some(healthy.requested.clone());
        healthy.phase = Phase::Applied;
        other.record(healthy.clone());
        assert_eq!(
            registry
                .snapshot(7)
                .unwrap()
                .last_field_failure
                .unwrap()
                .pages,
            [150]
        );
        drop(owner);
        healthy.requested.sequence = 1;
        healthy.phase = Phase::Waiting;
        other.record(healthy.clone());
        assert!(registry.snapshot(7).unwrap().last_field_failure.is_some());
        healthy.applied = Some(healthy.requested.clone());
        healthy.phase = Phase::Applied;
        other.record(healthy);
        assert!(registry.snapshot(7).unwrap().last_field_failure.is_none());
    }
}
