//! Sealed base or scoped output, selected without producer/Lua replay.

use std::{sync::Arc, time::Instant};

use chrono::{DateTime, Utc};
use ersatzrs_app::{teletext_design_documents::TeletextDesignDocumentBudget, ApplicationServices};
use ersatzrs_core::repositories::{
    TeletextGenerationMutation, TeletextGuideGenerationCheck, TeletextGuideSourceCheckLimits,
    TeletextProfileMutation, TELETEXT_EFFECTIVE_GUIDE_GENERATION_ARTIFACT_VERSION,
    TELETEXT_EFFECTIVE_LIFECYCLE_GENERATION_ARTIFACT_VERSION,
    TELETEXT_GUIDE_GENERATION_ARTIFACT_VERSION, TELETEXT_LIFECYCLE_GENERATION_ARTIFACT_VERSION,
};
use ersatzrs_domain::{TeletextAcceptedGeneration, TeletextEffectiveBindingSnapshot};
use ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService;

use crate::host::{
    teletext_channel_documents::TeletextChannelDocumentEncodingPolicy,
    teletext_effective_generations::{
        reload_accepted_effective_teletext_generation, AcceptedEffectiveTeletextOutputCandidate,
    },
    teletext_generations::{reload_accepted_teletext_generation, AcceptedTeletextOutputCandidate},
    teletext_guide_generations::{
        reload_accepted_effective_guide_teletext_generation,
        reload_accepted_guide_teletext_generation, GuideOutputQualification,
    },
};

/// Explicit version-3 runtime source checking, owned by one response context.
/// Absence keeps the historical static contract and never guesses source bounds.
#[derive(Clone)]
pub(super) struct GuideRuntime {
    pub(super) pool: ersatzrs_infra::db::DbPool,
    pub(super) limits: TeletextGuideSourceCheckLimits,
}

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
    pub(crate) fn next_transition(&self) -> Option<tokio::time::Instant> {
        let scope = match self {
            Self::Base(_, next) | Self::Effective(_, next) => *next,
        };
        let guide = self
            .guide_qualification()
            .map(|proof| tokio::time::Instant::from_std(proof.deadline));
        match (scope, guide) {
            (Some(scope), Some(guide)) => Some(scope.min(guide)),
            (scope, guide) => scope.or(guide),
        }
    }

    pub(super) fn guide_qualification(&self) -> Option<&GuideOutputQualification> {
        match self {
            Self::Base(candidate, _) => candidate.guide_qualification(),
            Self::Effective(candidate, _) => candidate.guide_qualification(),
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
        self.confirm_selection_for(self.generation(), application, maximum, deadline)
            .await
    }

    /// Retain already active content only while a same-role successor is
    /// recovered under this exact still-current selection and original expiry.
    /// This never confirms the successor's native artifact or renews old scope.
    pub(crate) async fn confirm_successor_selection(
        &self,
        successor: &TeletextAcceptedGeneration,
        application: &ApplicationServices,
        maximum: usize,
        deadline: Instant,
    ) -> bool {
        if successor.artifact_version != self.generation().artifact_version
            || !matches!(
                successor.artifact_version,
                TELETEXT_LIFECYCLE_GENERATION_ARTIFACT_VERSION
                    | TELETEXT_EFFECTIVE_LIFECYCLE_GENERATION_ARTIFACT_VERSION
                    | TELETEXT_GUIDE_GENERATION_ARTIFACT_VERSION
                    | TELETEXT_EFFECTIVE_GUIDE_GENERATION_ARTIFACT_VERSION
            )
            || successor.request.channel_id != self.channel_id()
            || self
                .service()
                .is_none_or(|service| service.resource_lifecycle_variants().is_none())
        {
            return false;
        }
        self.confirm_selection_for(successor, application, maximum, deadline)
            .await
    }

    async fn confirm_selection_for(
        &self,
        generation: &TeletextAcceptedGeneration,
        application: &ApplicationServices,
        maximum: usize,
        deadline: Instant,
    ) -> bool {
        let deadline = self
            .next_transition()
            .map_or(deadline, |boundary| deadline.min(boundary.into_std()));
        if Instant::now() >= deadline {
            return false;
        }
        let confirmation = async {
            // Transaction-owned source hashing must retain the same admission
            // after cancellation. Static confirmations perform no guide CPU work.
            if let Some(proof) = self.guide_qualification() {
                let Ok(guard) =
                    crate::host::teletext_work::begin(application, Some(deadline)).await
                else {
                    return false;
                };
                let check = TeletextGuideGenerationCheck {
                    limits: proof.limits,
                    deadline,
                    hold: guard.hold_work(),
                };
                let result = match self {
                    Self::Base(candidate, _) => {
                        application
                            .confirm_current_guide_teletext_generation(
                                generation,
                                candidate.selection(),
                                &proof.qualification,
                                check,
                                maximum,
                            )
                            .await
                    }
                    Self::Effective(candidate, _) => {
                        application
                            .confirm_current_effective_guide_teletext_generation(
                                generation,
                                candidate.selection(),
                                &proof.qualification,
                                check,
                                maximum,
                            )
                            .await
                    }
                };
                return matches!(result, Ok(TeletextGenerationMutation::Accepted(())));
            }
            let result = match self {
                Self::Base(candidate, _) => {
                    application
                        .confirm_current_teletext_generation(
                            generation,
                            candidate.selection(),
                            maximum,
                        )
                        .await
                }
                Self::Effective(candidate, _) => {
                    application
                        .confirm_effective_teletext_generation(
                            generation,
                            candidate.selection(),
                            maximum,
                            deadline,
                        )
                        .await
                }
            };
            matches!(result, Ok(TeletextGenerationMutation::Accepted(())))
        };
        matches!(
            tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), confirmation).await,
            Ok(true)
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
    load_selected_teletext_output_candidate_with_guide(
        application,
        channel_id,
        budget,
        encoding,
        deadline,
        None,
    )
    .await
}

pub(super) async fn load_selected_teletext_output_candidate_with_guide(
    application: &ApplicationServices,
    channel_id: i64,
    budget: TeletextDesignDocumentBudget,
    encoding: TeletextChannelDocumentEncodingPolicy,
    deadline: Instant,
    guide: Option<&GuideRuntime>,
) -> Result<Option<Arc<SelectedTeletextOutputCandidate>>, SelectedTeletextReadFailure> {
    let work = async {
        let observation = observe_selected_teletext_output(application, channel_id).await?;
        if observation.generation.is_none() {
            return Ok(None);
        }
        let candidate = if observation.effective {
            let guide_role = observation.generation.as_ref().is_some_and(|g| {
                g.artifact_version == TELETEXT_EFFECTIVE_GUIDE_GENERATION_ARTIFACT_VERSION
            });
            let recovered = if guide_role {
                let guide = guide.ok_or(SelectedTeletextReadFailure)?;
                reload_accepted_effective_guide_teletext_generation(
                    application,
                    &guide.pool,
                    channel_id,
                    budget,
                    encoding,
                    guide.limits,
                    deadline,
                )
                .await
            } else {
                reload_accepted_effective_teletext_generation(
                    application,
                    channel_id,
                    budget,
                    encoding,
                    deadline,
                )
                .await
            };
            recovered
                .map_err(|_| SelectedTeletextReadFailure)?
                .map(|candidate| {
                    SelectedTeletextOutputCandidate::Effective(
                        candidate,
                        observation.next_transition,
                    )
                })
        } else {
            let guide_role = observation
                .generation
                .as_ref()
                .is_some_and(|g| g.artifact_version == TELETEXT_GUIDE_GENERATION_ARTIFACT_VERSION);
            let recovered = if guide_role {
                let guide = guide.ok_or(SelectedTeletextReadFailure)?;
                reload_accepted_guide_teletext_generation(
                    application,
                    &guide.pool,
                    channel_id,
                    budget,
                    encoding,
                    guide.limits,
                    deadline,
                )
                .await
            } else {
                reload_accepted_teletext_generation(
                    application,
                    channel_id,
                    budget,
                    encoding,
                    deadline,
                )
                .await
            };
            recovered
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
    match tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), Box::pin(work)).await {
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
