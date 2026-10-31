//! One response-owned native preparation, without future selection authority.

use super::{
    observe_selected_teletext_output, Instant, NativeBackend, ReadFailure,
    SelectedTeletextOutputCandidate,
};
use crate::host::teletext_generations::{
    prepare_accepted_teletext_generation, PreparedTeletextOutputCandidate,
};
use crate::transport::iptv::teletext_selected_output::{
    observe_selected_teletext_output_with_guide, prepare_guide_refresh_base_output,
    GuideOutputIdentity,
};
use ersatzrs_domain::TeletextAcceptedGeneration;
use std::sync::Arc;

pub(super) struct ExpiryPreparation {
    identity: TeletextAcceptedGeneration,
    boundary: Instant,
    prepared: Option<PreparedTeletextOutputCandidate>,
    guide_identity: Option<GuideOutputIdentity>,
    prepared_refresh: Option<Arc<SelectedTeletextOutputCandidate>>,
}

impl NativeBackend {
    pub(super) async fn prepare_native_expiry(
        &self,
        candidate: &Arc<SelectedTeletextOutputCandidate>,
        deadline: Instant,
    ) -> Result<(), ReadFailure> {
        let boundary = match candidate.as_ref() {
            SelectedTeletextOutputCandidate::Effective(_, Some(boundary)) => boundary,
            SelectedTeletextOutputCandidate::Guide(_, Some(boundary))
                if candidate.effective_valid_until().is_some() =>
            {
                boundary
            }
            _ => return Ok(()),
        };
        if Instant::now() >= *boundary {
            return Ok(());
        }
        if !self.reserve_expiry_preparation(candidate, *boundary)? {
            return Ok(());
        }
        let guide_base = if self.context.guide.is_some() {
            self.context
                .application
                .accepted_teletext_generation(self.context.channel_id)
                .await
                .map_err(|_| ReadFailure)?
                .is_some_and(|generation| {
                    generation.artifact_version
                        == ersatzrs_core::repositories::TELETEXT_GUIDE_GENERATION_ARTIFACT_VERSION
                })
        } else {
            false
        };
        if guide_base
            && self
                .context
                .guide
                .as_ref()
                .is_some_and(|guide| guide.refresh)
        {
            let guide = self.context.guide.as_ref().ok_or(ReadFailure)?;
            let prepared = prepare_guide_refresh_base_output(
                &self.context.application,
                self.context.channel_id,
                self.context.budget,
                self.context.encoding.clone(),
                deadline.min(*boundary).into_std(),
                guide,
            )
            .await
            .map_err(|_| ReadFailure)?;
            if let Some(attempt) = self
                .expiry_preparation
                .lock()
                .map_err(|_| ReadFailure)?
                .as_mut()
            {
                attempt.prepared_refresh = prepared;
            }
            return Ok(());
        }
        let prepared = if guide_base {
            let guide = self.context.guide.as_ref().ok_or(ReadFailure)?;
            crate::host::teletext_guide_generations::prepare_accepted_guide_teletext_generation(
                &self.context.application,
                &guide.pool,
                self.context.channel_id,
                self.context.budget,
                self.context.encoding.clone(),
                guide.limits,
                deadline.min(*boundary).into_std(),
            )
            .await
        } else {
            prepare_accepted_teletext_generation(
                &self.context.application,
                self.context.channel_id,
                self.context.budget,
                self.context.encoding.clone(),
                deadline.min(*boundary).into_std(),
            )
            .await
        }
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

    fn reserve_expiry_preparation(
        &self,
        candidate: &SelectedTeletextOutputCandidate,
        boundary: Instant,
    ) -> Result<bool, ReadFailure> {
        let mut slot = self.expiry_preparation.lock().map_err(|_| ReadFailure)?;
        if slot.as_ref().is_some_and(|attempt| {
            attempt.identity == *candidate.generation()
                && attempt.boundary == boundary
                && attempt.guide_identity.as_ref() == candidate.guide_identity()
        }) {
            return Ok(false);
        }
        // Mark the sole attempt before awaiting. Timeout, absence or failure
        // cannot start another compiler on every polling tick.
        *slot = Some(ExpiryPreparation {
            identity: candidate.generation().clone(),
            boundary,
            prepared: None,
            guide_identity: candidate.guide_identity().cloned(),
            prepared_refresh: None,
        });
        Ok(true)
    }

    pub(super) async fn recover_prepared_expiry(
        &self,
        deadline: Instant,
    ) -> Result<Option<Arc<SelectedTeletextOutputCandidate>>, ReadFailure> {
        let (prepared, prepared_refresh) = {
            let mut slot = self.expiry_preparation.lock().map_err(|_| ReadFailure)?;
            if slot
                .as_ref()
                .is_none_or(|attempt| Instant::now() < attempt.boundary)
            {
                return Ok(None);
            }
            slot.take().map_or((None, None), |attempt| {
                (attempt.prepared, attempt.prepared_refresh)
            })
        };
        if let Some(prepared) = prepared_refresh {
            let observation = observe_selected_teletext_output_with_guide(
                &self.context.application,
                self.context.channel_id,
                self.context.guide.as_deref(),
                self.context.budget.max_serialized_bytes,
            )
            .await
            .map_err(|_| ReadFailure)?;
            *self.next_transition.lock().map_err(|_| ReadFailure)? = observation.next_transition;
            if observation.effective
                || observation.generation.as_ref() != Some(prepared.generation())
            {
                return Ok(None);
            }
            let Some(metadata) = observation.guide_refresh.as_ref() else {
                return Ok(None);
            };
            let candidate = prepared
                .reauthorize_guide(
                    metadata,
                    &self.context.application,
                    self.context.budget.max_serialized_bytes,
                    deadline.into_std(),
                )
                .await;
            if let Some(candidate) = &candidate {
                self.record_guide_validity(candidate)?;
                *self.next_transition.lock().map_err(|_| ReadFailure)? =
                    candidate.next_transition();
                super::trace_native_candidate(candidate, self.trace_id, "expiry_base_confirmed");
            }
            return Ok(candidate);
        }
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
            self.record_guide_validity(candidate)?;
            *self.next_transition.lock().map_err(|_| ReadFailure)? = candidate.next_transition();
            super::trace_native_candidate(candidate, self.trace_id, "expiry_base_confirmed");
        }
        Ok(candidate)
    }
}
