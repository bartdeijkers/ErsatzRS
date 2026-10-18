//! One response-owned native preparation, without future selection authority.

use super::{
    observe_selected_teletext_output, Instant, NativeBackend, ReadFailure,
    SelectedTeletextOutputCandidate,
};
use crate::host::teletext_generations::{
    prepare_accepted_teletext_generation, PreparedTeletextOutputCandidate,
};
use ersatzrs_domain::TeletextAcceptedGeneration;
use std::sync::Arc;

pub(super) struct ExpiryPreparation {
    identity: TeletextAcceptedGeneration,
    boundary: Instant,
    prepared: Option<PreparedTeletextOutputCandidate>,
}

impl NativeBackend {
    pub(super) async fn prepare_native_expiry(
        &self,
        candidate: &Arc<SelectedTeletextOutputCandidate>,
        deadline: Instant,
    ) -> Result<(), ReadFailure> {
        let SelectedTeletextOutputCandidate::Effective(_, Some(boundary)) = candidate.as_ref()
        else {
            return Ok(());
        };
        if Instant::now() >= *boundary {
            return Ok(());
        }
        {
            let mut slot = self.expiry_preparation.lock().map_err(|_| ReadFailure)?;
            if slot.as_ref().is_some_and(|attempt| {
                attempt.identity == *candidate.generation() && attempt.boundary == *boundary
            }) {
                return Ok(());
            }
            // Mark the sole attempt before awaiting. Timeout, absence or failure
            // cannot start another compiler on every polling tick.
            *slot = Some(ExpiryPreparation {
                identity: candidate.generation().clone(),
                boundary: *boundary,
                prepared: None,
            });
        }
        let prepared = prepare_accepted_teletext_generation(
            &self.context.application,
            self.context.channel_id,
            self.context.budget,
            self.context.encoding.clone(),
            deadline.min(*boundary).into_std(),
        )
        .await
        .map_err(|_| ReadFailure)?;
        tracing::debug!(
            target: "ersatzrs_web::transport::iptv::teletext_provenance",
            event = "expiry_base_prepared",
            proposal_slot = %self.trace_id,
            channel_id = self.context.channel_id,
            generation_id = ?prepared.as_ref().map(|value| value.generation().id),
            original_transition_remaining_ns = boundary.saturating_duration_since(Instant::now()).as_nanos(),
            observed_utc = %chrono::Utc::now().to_rfc3339(),
            "native Base preparation grants no output authority"
        );
        if let Some(attempt) = self
            .expiry_preparation
            .lock()
            .map_err(|_| ReadFailure)?
            .as_mut()
        {
            attempt.prepared = prepared;
        }
        Ok(())
    }

    pub(super) async fn recover_prepared_expiry(
        &self,
        deadline: Instant,
    ) -> Result<Option<Arc<SelectedTeletextOutputCandidate>>, ReadFailure> {
        let prepared = {
            let mut slot = self.expiry_preparation.lock().map_err(|_| ReadFailure)?;
            if slot
                .as_ref()
                .is_none_or(|attempt| Instant::now() < attempt.boundary)
            {
                return Ok(None);
            }
            slot.take().and_then(|attempt| attempt.prepared)
        };
        let Some(prepared) = prepared else {
            return Ok(None);
        };
        let observation =
            observe_selected_teletext_output(&self.context.application, self.context.channel_id)
                .await
                .map_err(|_| ReadFailure)?;
        *self.next_transition.lock().map_err(|_| ReadFailure)? = observation.next_transition;
        if observation.effective || observation.generation.as_ref() != Some(prepared.generation()) {
            return Ok(None);
        }
        // The existing Base transaction also rejects a newly active Scripted
        // interval, stale bindings, deleted revisions or changed accepted state.
        // Neither cached bytes nor an earlier role observation bypass that fence.
        let deadline = observation
            .next_transition
            .map_or(deadline, |next| deadline.min(next));
        let accepted = prepared
            .confirm(
                &self.context.application,
                self.context.budget.max_serialized_bytes,
                deadline.into_std(),
            )
            .await
            .map_err(|_| ReadFailure)?;
        let candidate = accepted.map(|candidate| {
            Arc::new(SelectedTeletextOutputCandidate::Base(
                Arc::new(candidate),
                observation.next_transition,
            ))
        });
        if let Some(candidate) = &candidate {
            super::trace_native_candidate(candidate, self.trace_id, "expiry_base_confirmed");
        }
        Ok(candidate)
    }
}
