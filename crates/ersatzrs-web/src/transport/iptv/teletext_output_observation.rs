//! Diagnostic projection of actual pump receipts; no new output authority.

use std::sync::Arc;

use ersatzrs_app::{ApplicationServices, TeletextOutputObserver};
use ersatzrs_domain::{
    TeletextOutputGenerationObservation, TeletextOutputObservation,
    TeletextOutputObservationPhase as Phase, TeletextOutputSelectionObservation as Selection,
};

#[derive(Clone)]
pub(super) struct ObservationContext {
    pub application: Arc<ApplicationServices>,
    pub channel_id: i64,
    pub initial: Selection,
}

/// Startup-only policies share the same receipt observer without a live monitor.
pub(super) fn startup_control(
    application: &ApplicationServices,
    identity: &ersatzrs_domain::TeletextAcceptedGeneration,
) -> super::teletext_binding::TeletextOutputControl {
    let (_updates, mut control, _status) =
        super::teletext_binding::teletext_output_control_channel();
    control.observe_native_output(ObservationContext {
        // Cloning services shares the injected runtime; it does not create a
        // second native registry or start any source/monitor/package work.
        application: Arc::new(application.clone()),
        channel_id: identity.request.channel_id,
        initial: Selection {
            sequence: 0,
            generation: generation(identity),
            authored: true,
        },
    });
    control
}

pub(super) struct PumpObservation {
    owner: Box<dyn TeletextOutputObserver>,
    state: TeletextOutputObservation,
    initial: Option<Selection>,
    installed: Option<Selection>,
}

impl PumpObservation {
    pub(super) fn new(context: &ObservationContext, epoch: uuid::Uuid) -> Option<Self> {
        let state = TeletextOutputObservation {
            epoch,
            requested: context.initial.clone(),
            applied: None,
            phase: Phase::Waiting,
        };
        let owner = context
            .application
            .observe_teletext_output(context.channel_id, state.clone())?;
        Some(Self {
            owner,
            state,
            initial: Some(context.initial.clone()),
            installed: None,
        })
    }

    pub(super) fn requested(&mut self, selection: Selection) {
        self.state.requested = selection;
        self.state.phase = Phase::Waiting;
        self.owner.record(self.state.clone());
    }

    pub(super) fn installed(&mut self, selection: Selection) {
        // Only a successful gate installation transfers the physical content.
        self.initial = None;
        self.installed = Some(selection);
    }

    pub(super) fn initial_receipt(&mut self, complete: bool, absent: bool) {
        if self
            .initial
            .as_ref()
            .is_some_and(|initial| if initial.authored { complete } else { absent })
        {
            if let Some(initial) = self.initial.take() {
                self.applied(initial);
            }
        }
    }

    pub(super) fn acknowledge(&mut self, sequence: u64) {
        if self
            .installed
            .as_ref()
            .is_some_and(|installed| installed.sequence == sequence)
        {
            if let Some(installed) = self.installed.take() {
                self.applied(installed);
            }
        }
    }

    fn applied(&mut self, selection: Selection) {
        if self.state.requested == selection {
            self.state.phase = Phase::Applied;
        }
        self.state.applied = Some(selection);
        self.owner.record(self.state.clone());
    }

    /// Keep the terminal failure for the channel after this output ends.
    pub(super) fn failed(&self, kind: ersatzrs_domain::TeletextOutputFailureKind) {
        self.owner.failed(kind);
    }

    pub(super) fn reject(&mut self, sequence: u64) {
        if self.state.requested.sequence == sequence {
            self.state.phase = Phase::Rejected;
            self.owner.record(self.state.clone());
        }
    }
}

pub(super) fn generation(
    identity: &ersatzrs_domain::TeletextAcceptedGeneration,
) -> Option<TeletextOutputGenerationObservation> {
    use ersatzrs_core::repositories::{
        TELETEXT_EFFECTIVE_GENERATION_ARTIFACT_VERSION as LEGACY_EFFECTIVE,
        TELETEXT_EFFECTIVE_GUIDE_GENERATION_ARTIFACT_VERSION as GUIDE_EFFECTIVE,
        TELETEXT_EFFECTIVE_LIFECYCLE_GENERATION_ARTIFACT_VERSION as EFFECTIVE,
        TELETEXT_GENERATION_ARTIFACT_VERSION as LEGACY_BASE,
        TELETEXT_GUIDE_GENERATION_ARTIFACT_VERSION as GUIDE_BASE,
        TELETEXT_LIFECYCLE_GENERATION_ARTIFACT_VERSION as BASE,
    };
    let effective = match identity.artifact_version {
        EFFECTIVE | LEGACY_EFFECTIVE | GUIDE_EFFECTIVE => true,
        BASE | LEGACY_BASE | GUIDE_BASE => false,
        _ => return None,
    };
    Some(TeletextOutputGenerationObservation {
        generation_id: identity.id,
        request_id: identity.request.id,
        design_id: identity.request.design_id,
        revision: identity.request.revision,
        effective,
        guide_refresh: None,
        custom_fields: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Recorder(Arc<Mutex<Vec<TeletextOutputObservation>>>);
    impl TeletextOutputObserver for Recorder {
        fn record(&self, value: TeletextOutputObservation) {
            self.0.lock().unwrap().push(value);
        }
    }

    fn observer(authored: bool) -> PumpObservation {
        let initial = Selection {
            sequence: 0,
            generation: None,
            authored,
        };
        PumpObservation {
            owner: Box::new(Recorder(Arc::default())),
            state: TeletextOutputObservation {
                epoch: uuid::Uuid::new_v4(),
                requested: initial.clone(),
                applied: None,
                phase: Phase::Waiting,
            },
            initial: Some(initial),
            installed: None,
        }
    }

    #[tokio::test]
    async fn startup_only_control_shares_the_host_registry_and_ends_with_the_actual_pump() {
        let state = crate::host::state::AppState::for_tests(
            crate::host::state::IptvAccessTokenVerifier::from_optional_secret(None),
        );
        let identity = ersatzrs_domain::TeletextAcceptedGeneration {
            id: 3,
            request: ersatzrs_domain::TeletextPublicationRequest {
                id: 2,
                channel_id: 44,
                design_id: 1,
                revision: 1,
                digest: "a".repeat(64),
            },
            artifact_version:
                ersatzrs_core::repositories::TELETEXT_LIFECYCLE_GENERATION_ARTIFACT_VERSION,
            artifact_digest: "b".repeat(64),
        };
        let control = startup_control(&state.application, &identity);
        assert!(state
            .application
            .teletext_output_observations(44)
            .unwrap()
            .outputs
            .is_empty());
        let pump = super::super::teletext_binding::control::PumpControl::new(control);
        let outputs = state.application.teletext_output_observations(44).unwrap();
        assert_eq!(outputs.outputs.len(), 1);
        assert_eq!(outputs.outputs[0].phase, Phase::Waiting);
        assert!(outputs.outputs[0].applied.is_none());
        assert_eq!(
            outputs.outputs[0]
                .requested
                .generation
                .as_ref()
                .unwrap()
                .generation_id,
            3
        );
        drop(pump);
        assert!(state
            .application
            .teletext_output_observations(44)
            .unwrap()
            .outputs
            .is_empty());
    }

    #[test]
    fn generation_observation_preserves_known_legacy_roles_and_refuses_unknown_versions() {
        let mut identity = ersatzrs_domain::TeletextAcceptedGeneration {
            id: 3,
            request: ersatzrs_domain::TeletextPublicationRequest {
                id: 2,
                channel_id: 44,
                design_id: 1,
                revision: 5,
                digest: "a".repeat(64),
            },
            artifact_version: 0,
            artifact_digest: "b".repeat(64),
        };
        for (version, effective) in [
            (1, false),
            (2, true),
            (3, false),
            (4, true),
            (5, false),
            (6, true),
        ] {
            identity.artifact_version = version;
            let observed = generation(&identity).unwrap();
            assert_eq!(
                (
                    observed.generation_id,
                    observed.request_id,
                    observed.design_id,
                    observed.revision
                ),
                (3, 2, 1, 5)
            );
            assert_eq!(observed.effective, effective);
        }
        for version in [0, 7, u16::MAX] {
            identity.artifact_version = version;
            assert!(generation(&identity).is_none());
        }
    }

    #[test]
    fn initial_presence_and_absence_require_their_own_receipt() {
        for authored in [false, true] {
            let mut observation = observer(authored);
            observation.initial_receipt(false, false);
            assert!(observation.state.applied.is_none());
            observation.initial_receipt(!authored, authored);
            assert!(observation.state.applied.is_none());
            observation.initial_receipt(authored, !authored);
            assert_eq!(observation.state.phase, Phase::Applied);
            assert_eq!(observation.state.applied.as_ref().unwrap().sequence, 0);
        }
    }

    #[test]
    fn superseded_receipts_and_rejections_cannot_confirm_the_latest_request() {
        let mut observation = observer(true);
        let mut first = observation.state.requested.clone();
        first.sequence = 1;
        observation.requested(first.clone());
        observation.installed(first);
        let mut latest = observation.state.requested.clone();
        latest.sequence = 2;
        observation.requested(latest.clone());
        observation.acknowledge(2);
        assert!(observation.state.applied.is_none());
        observation.acknowledge(1);
        assert_eq!(observation.state.applied.as_ref().unwrap().sequence, 1);
        assert_eq!(observation.state.phase, Phase::Waiting);
        observation.reject(1);
        assert_eq!(observation.state.phase, Phase::Waiting);
        observation.reject(2);
        assert_eq!(observation.state.phase, Phase::Rejected);
        observation.installed(latest);
        assert_eq!(observation.state.phase, Phase::Rejected);
        observation.acknowledge(2);
        assert_eq!(observation.state.phase, Phase::Applied);
        assert_eq!(observation.state.applied.as_ref().unwrap().sequence, 2);
    }
    #[tokio::test]
    async fn field_failure_reaches_channel_health_only_after_the_pump_receipt() {
        use ersatzrs_domain::{
            TeletextCustomFieldOutputObservation, TeletextOutputGenerationObservation,
        };
        let state = crate::host::state::AppState::for_tests(
            crate::host::state::IptvAccessTokenVerifier::from_optional_secret(None),
        );
        let context = ObservationContext {
            application: Arc::clone(&state.application),
            channel_id: 44,
            initial: Selection {
                sequence: 0,
                authored: true,
                generation: Some(TeletextOutputGenerationObservation {
                    generation_id: 3,
                    request_id: 2,
                    design_id: 1,
                    revision: 1,
                    effective: false,
                    guide_refresh: None,
                    custom_fields: Some(TeletextCustomFieldOutputObservation {
                        revision: 1,
                        failed_pages: vec![150],
                    }),
                }),
            },
        };
        let snapshot = || {
            context
                .application
                .teletext_output_observations(44)
                .unwrap()
        };
        let mut pump = PumpObservation::new(&context, uuid::Uuid::new_v4()).unwrap();
        pump.initial_receipt(false, false);
        assert!(snapshot().last_field_failure.is_none());
        pump.initial_receipt(true, false);
        assert_eq!(snapshot().last_field_failure.unwrap().pages, [150]);
        let mut recovered = context.initial.clone();
        recovered.sequence = 1;
        recovered
            .generation
            .as_mut()
            .unwrap()
            .custom_fields
            .as_mut()
            .unwrap()
            .failed_pages
            .clear();
        pump.requested(recovered.clone());
        pump.installed(recovered);
        assert!(snapshot().last_field_failure.is_some());
        pump.acknowledge(0);
        assert!(snapshot().last_field_failure.is_some());
        pump.acknowledge(1);
        assert!(snapshot().last_field_failure.is_none());
    }
}
