//! Sealed base or scoped output, selected without producer/Lua replay.

use std::{sync::Arc, time::Instant};

use chrono::{DateTime, Utc};
use ersatzrs_app::{teletext_design_documents::TeletextDesignDocumentBudget, ApplicationServices};
use ersatzrs_core::repositories::{TeletextGenerationMutation, TeletextProfileMutation};
use ersatzrs_domain::{TeletextAcceptedGeneration, TeletextEffectiveBindingSnapshot};
use ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService;

use crate::host::{
    teletext_channel_documents::TeletextChannelDocumentEncodingPolicy,
    teletext_effective_generations::{
        reload_accepted_effective_teletext_generation, AcceptedEffectiveTeletextOutputCandidate,
    },
    teletext_generations::{reload_accepted_teletext_generation, AcceptedTeletextOutputCandidate},
};

/// Only existing sealed native candidates can construct these role variants.
/// Scoped absence is retained for caching/confirmation, rather than interpreted
/// as missing acceptance or permission to fall back to the operator base.
pub(crate) enum SelectedTeletextOutputCandidate {
    Base(
        Arc<AcceptedTeletextOutputCandidate>,
        Option<tokio::time::Instant>,
    ),
    Effective(
        AcceptedEffectiveTeletextOutputCandidate,
        Option<tokio::time::Instant>,
    ),
}

impl SelectedTeletextOutputCandidate {
    pub(crate) const fn next_transition(&self) -> Option<tokio::time::Instant> {
        match self {
            Self::Base(_, next) | Self::Effective(_, next) => *next,
        }
    }

    pub(crate) fn generation(&self) -> &TeletextAcceptedGeneration {
        match self {
            Self::Base(candidate, _) => candidate.generation(),
            Self::Effective(candidate, _) => candidate.generation(),
        }
    }

    pub(crate) fn channel_id(&self) -> i64 {
        match self {
            Self::Base(candidate, _) => candidate.selection().channel_id,
            Self::Effective(candidate, _) => candidate.selection().channel_id,
        }
    }

    pub(crate) fn service(&self) -> Option<&Arc<EncodedTeletextAuthoredService>> {
        match self {
            Self::Base(candidate, _) => Some(candidate.service()),
            Self::Effective(candidate, _) => candidate.service(),
        }
    }

    pub(crate) async fn confirm(
        &self,
        application: &ApplicationServices,
        maximum: usize,
        deadline: Instant,
    ) -> bool {
        let confirmation = async {
            match self {
                Self::Base(candidate, _) => {
                    application
                        .confirm_current_teletext_generation(
                            candidate.generation(),
                            candidate.selection(),
                            maximum,
                        )
                        .await
                }
                Self::Effective(candidate, _) => {
                    application
                        .confirm_effective_teletext_generation(
                            candidate.generation(),
                            candidate.selection(),
                            maximum,
                            deadline,
                        )
                        .await
                }
            }
        };
        matches!(
            tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), confirmation).await,
            Ok(Ok(TeletextGenerationMutation::Accepted(())))
        ) && Instant::now() < deadline
    }
}

/// Static internal failure; invalid scope never substitutes a base generation.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SelectedTeletextReadFailure;

pub(crate) struct SelectedTeletextObservation {
    pub(crate) generation: Option<TeletextAcceptedGeneration>,
    pub(crate) effective: bool,
    pub(crate) next_transition: Option<tokio::time::Instant>,
}

/// Preserve a conservative monotonic boundary from the original observed clock.
/// A late read leaves a past boundary, so the monitor wakes immediately.
fn transition_deadline(
    monotonic: tokio::time::Instant,
    observed: DateTime<Utc>,
    boundary: Option<DateTime<Utc>>,
) -> Option<tokio::time::Instant> {
    boundary.map(|boundary| {
        boundary
            .signed_duration_since(observed)
            .to_std()
            .ok()
            .and_then(|remaining| monotonic.checked_add(remaining))
            .unwrap_or(monotonic)
    })
}

pub(crate) async fn observe_selected_teletext_output(
    application: &ApplicationServices,
    channel_id: i64,
) -> Result<SelectedTeletextObservation, SelectedTeletextReadFailure> {
    let monotonic = tokio::time::Instant::now();
    let observed = Utc::now();
    let scope = application
        .observe_effective_channel_teletext_selection(channel_id, observed)
        .await
        .map_err(|_| SelectedTeletextReadFailure)?;
    let TeletextProfileMutation::Saved(scope) = scope else {
        return Err(SelectedTeletextReadFailure);
    };
    let effective = scope
        .bindings
        .iter()
        .any(|binding| matches!(binding, TeletextEffectiveBindingSnapshot::Scripted(_)));
    let generation = if effective {
        application
            .effective_teletext_accepted_generation(channel_id)
            .await
    } else {
        application.accepted_teletext_generation(channel_id).await
    }
    .map_err(|_| SelectedTeletextReadFailure)?;
    Ok(SelectedTeletextObservation {
        generation,
        effective,
        next_transition: transition_deadline(monotonic, observed, scope.valid_until),
    })
}

/// Select applicable accepted scope before recovery. Pending, stale or invalid
/// scoped output cannot fall through to base; expiry restores independent base
/// authority only through its original native validation and final confirmation.
pub(crate) async fn load_selected_teletext_output_candidate(
    application: &ApplicationServices,
    channel_id: i64,
    budget: TeletextDesignDocumentBudget,
    encoding: TeletextChannelDocumentEncodingPolicy,
    deadline: Instant,
) -> Result<Option<Arc<SelectedTeletextOutputCandidate>>, SelectedTeletextReadFailure> {
    let work = async {
        let observation = observe_selected_teletext_output(application, channel_id).await?;
        if observation.generation.is_none() {
            return Ok(None);
        }
        let candidate = if observation.effective {
            reload_accepted_effective_teletext_generation(
                application,
                channel_id,
                budget,
                encoding,
                deadline,
            )
            .await
            .map_err(|_| SelectedTeletextReadFailure)?
            .map(|candidate| {
                SelectedTeletextOutputCandidate::Effective(candidate, observation.next_transition)
            })
        } else {
            reload_accepted_teletext_generation(application, channel_id, budget, encoding, deadline)
                .await
                .map_err(|_| SelectedTeletextReadFailure)?
                .map(|candidate| {
                    SelectedTeletextOutputCandidate::Base(
                        Arc::new(candidate),
                        observation.next_transition,
                    )
                })
        };
        Ok(candidate.map(Arc::new))
    };
    match tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), work).await {
        Ok(result) if Instant::now() < deadline => result,
        _ => Err(SelectedTeletextReadFailure),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_uses_original_clock_and_expired_boundary_is_immediate() {
        let clock = Utc::now();
        let monotonic = tokio::time::Instant::now();
        assert_eq!(transition_deadline(monotonic, clock, None), None);
        assert_eq!(
            transition_deadline(monotonic, clock, Some(clock)),
            Some(monotonic)
        );
        assert_eq!(
            transition_deadline(monotonic, clock, Some(clock - chrono::Duration::seconds(1))),
            Some(monotonic)
        );
        assert_eq!(
            transition_deadline(monotonic, clock, Some(clock + chrono::Duration::seconds(7))),
            Some(monotonic + std::time::Duration::from_secs(7))
        );
    }
}
