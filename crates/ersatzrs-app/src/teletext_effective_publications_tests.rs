//! Scoped admission rejects missing native/repository capability without base fallback.

use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

use ersatzrs_core::repositories::{
    RepositoryError, Result as RepositoryResult, TELETEXT_GUIDE_CONTEXT_DIGEST_VERSION,
    TELETEXT_GUIDE_SOURCE_FENCE_VERSION,
};
use ersatzrs_domain::{
    TeletextEffectiveBindingSnapshot, TeletextEffectiveChannelSelectionSnapshot,
    TeletextOverrideInterval, TeletextOverrideState, TeletextPublishedOverride,
};

use super::*;

struct LegacyOnly;
#[async_trait]
impl TeletextPublicationRepository for LegacyOnly {
    async fn request(
        &self,
        _: &TeletextPublicationRequestSave,
        _: usize,
    ) -> RepositoryResult<TeletextPublicationMutation<TeletextPublicationRequest>> {
        panic!("effective admission fell back to base request")
    }
    async fn current(&self, _: EntityId) -> RepositoryResult<Option<TeletextPublicationRequest>> {
        panic!("effective read fell back to base pointer")
    }
}

fn request() -> TeletextEffectivePublicationRequestSave {
    let start = "2026-10-14T12:00:00Z".parse().unwrap();
    let finish = "2026-10-14T13:00:00Z".parse().unwrap();
    TeletextEffectivePublicationRequestSave {
        design_id: 11,
        revision: 2,
        digest: "private-digest-fixture".to_owned(),
        expected_design_head: 3,
        expected_request_id: Some(7),
        selection: TeletextEffectiveChannelSelectionSnapshot {
            channel_id: 9,
            bindings: vec![TeletextEffectiveBindingSnapshot::Scripted(
                TeletextPublishedOverride {
                    playout_id: 42,
                    interval: TeletextOverrideInterval {
                        start,
                        finish,
                        state: TeletextOverrideState::Off {
                            channel_id: 9,
                            addon_id: "org.example.teletext".to_owned(),
                        },
                    },
                },
            )],
            valid_until: Some(finish),
        },
    }
}

struct ExactNative {
    expected: TeletextEffectivePublicationRequestSave,
    deadline: Instant,
    calls: AtomicUsize,
}
#[async_trait]
impl TeletextEffectivePublicationValidator for ExactNative {
    async fn confirm_request(
        &self,
        request: &TeletextEffectivePublicationRequestSave,
        deadline: Instant,
    ) -> Result<(), TeletextPublicationValidationError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(
            deadline, self.deadline,
            "application renewed native admission"
        );
        if *request == self.expected {
            Ok(())
        } else {
            Err(TeletextPublicationValidationError::Association)
        }
    }
}

fn native(deadline: Instant) -> ExactNative {
    ExactNative {
        expected: request(),
        deadline,
        calls: AtomicUsize::new(0),
    }
}
fn fence() -> TeletextGuideSourceFence {
    TeletextGuideSourceFence {
        version: TELETEXT_GUIDE_SOURCE_FENCE_VERSION,
        context_digest_version: TELETEXT_GUIDE_CONTEXT_DIGEST_VERSION,
        context_digest: [0; 32],
        channel_ids: vec![17],
        captured_at: "2026-10-14T12:00:00Z".parse().unwrap(),
        start: "2026-10-14T00:00:00Z".parse().unwrap(),
        finish: "2026-10-17T00:00:00Z".parse().unwrap(),
        locale: "en".to_owned(),
        timezone: "UTC".to_owned(),
    }
}

#[tokio::test]
async fn effective_request_requires_exact_native_scope_before_repository_admission() {
    let deadline = Instant::now() + Duration::from_secs(30);
    let validator = native(deadline);
    let mut changed = request();
    changed.selection.channel_id = 10;
    let result =
        request_effective_with_repository(&LegacyOnly, &changed, &validator, 1024, deadline).await;
    assert!(matches!(
        result,
        Err(TeletextPublicationStoreError::Validation(
            TeletextPublicationValidationError::Association
        ))
    ));
    assert_eq!(validator.calls.load(Ordering::SeqCst), 1);
    let result =
        request_effective_with_repository(&LegacyOnly, &request(), &validator, 1024, deadline)
            .await;
    assert!(matches!(
        result,
        Err(TeletextPublicationStoreError::StorageUnavailable)
    ));
    assert_eq!(validator.calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn expired_effective_admission_never_calls_native_or_base_repository() {
    let deadline = Instant::now();
    let validator = native(deadline);
    let result =
        request_effective_with_repository(&LegacyOnly, &request(), &validator, 1024, deadline)
            .await;
    assert!(matches!(
        result,
        Err(TeletextPublicationStoreError::Validation(
            TeletextPublicationValidationError::TimedOut
        ))
    ));
    assert_eq!(validator.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn source_fence_cannot_fall_back_to_static_effective_native_validation() {
    let deadline = Instant::now() + Duration::from_secs(30);
    let validator = native(deadline);
    assert_eq!(
        validator
            .confirm_request_with_source(&request(), &fence(), deadline)
            .await,
        Err(TeletextPublicationValidationError::Association)
    );
    assert_eq!(validator.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn optional_scoped_repository_ports_fail_honestly_without_legacy_fallback() {
    let deadline = Instant::now() + Duration::from_secs(30);
    let limits = TeletextGuideSourceCheckLimits {
        max_records: 1,
        max_source_bytes: 1024,
        max_model_nodes: 1024,
        max_model_bytes: 1024,
        max_digest_fields: 1024,
        max_digest_bytes: 1024,
    };
    assert!(matches!(
        LegacyOnly
            .request_effective(&request(), 1024, deadline)
            .await,
        Err(RepositoryError::Unavailable { .. })
    ));
    assert!(matches!(
        LegacyOnly
            .request_effective_with_source(&request(), &fence(), limits, 1024, deadline)
            .await,
        Err(RepositoryError::Unavailable { .. })
    ));
    assert!(matches!(
        LegacyOnly.current_effective(9).await,
        Err(RepositoryError::Unavailable { .. })
    ));
    let debug = format!("{:?}", request());
    assert!(!debug.contains("private-digest-fixture"));
    assert!(!debug.contains("org.example.teletext"));
}
