//! Scoped generation admission keeps exact native proof and legacy pointers separate.

use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

use ersatzrs_core::repositories::{
    RepositoryError, Result as RepositoryResult, TELETEXT_EFFECTIVE_GENERATION_ARTIFACT_VERSION,
    TELETEXT_GENERATION_ARTIFACT_VERSION,
};
use ersatzrs_domain::TeletextPublicationRequest;

use super::*;

struct LegacyOnly;
#[async_trait]
impl TeletextGenerationRepository for LegacyOnly {
    async fn accept(
        &self,
        _: &TeletextGenerationSave,
        _: usize,
    ) -> RepositoryResult<TeletextGenerationMutation<TeletextAcceptedGeneration>> {
        panic!("scoped generation fell back to base acceptance")
    }
    async fn current(&self, _: EntityId) -> RepositoryResult<Option<TeletextAcceptedGeneration>> {
        panic!("scoped read fell back to base pointer")
    }
    async fn confirm_current(
        &self,
        _: &TeletextAcceptedGeneration,
        _: &TeletextChannelSelectionSnapshot,
        _: usize,
    ) -> RepositoryResult<TeletextGenerationMutation<()>> {
        panic!("scoped confirmation fell back to base authority")
    }
}

fn save() -> TeletextEffectiveGenerationSave {
    TeletextEffectiveGenerationSave {
        request: TeletextPublicationRequest {
            id: 7,
            channel_id: 9,
            design_id: 11,
            revision: 2,
            digest: "private-document-fixture".to_owned(),
        },
        expected_design_head: 3,
        expected_generation_id: Some(5),
        selection: TeletextEffectiveChannelSelectionSnapshot {
            channel_id: 9,
            bindings: vec![],
            valid_until: None,
        },
        artifact_version: TELETEXT_EFFECTIVE_GENERATION_ARTIFACT_VERSION,
        artifact_digest: "private-artifact-fixture".to_owned(),
    }
}

struct ExactNative {
    expected: TeletextEffectiveGenerationSave,
    deadline: Instant,
    calls: AtomicUsize,
}
#[async_trait]
impl TeletextEffectiveGenerationValidator for ExactNative {
    async fn confirm_generation(
        &self,
        save: &TeletextEffectiveGenerationSave,
        deadline: Instant,
    ) -> Result<(), TeletextPublicationValidationError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(
            deadline, self.deadline,
            "application renewed native admission"
        );
        if *save == self.expected {
            Ok(())
        } else {
            Err(TeletextPublicationValidationError::Association)
        }
    }
}

#[tokio::test]
async fn effective_generation_rejects_other_artifact_or_channel_before_storage() {
    let deadline = Instant::now() + Duration::from_secs(30);
    let validator = ExactNative {
        expected: save(),
        deadline,
        calls: AtomicUsize::new(0),
    };
    for changed in [
        TeletextEffectiveGenerationSave {
            artifact_version: TELETEXT_GENERATION_ARTIFACT_VERSION,
            ..save()
        },
        TeletextEffectiveGenerationSave {
            artifact_digest: "another-native-artifact".to_owned(),
            ..save()
        },
        TeletextEffectiveGenerationSave {
            selection: TeletextEffectiveChannelSelectionSnapshot {
                channel_id: 10,
                ..save().selection
            },
            ..save()
        },
    ] {
        let result =
            accept_effective_with_repository(&LegacyOnly, &changed, &validator, 1024, deadline)
                .await;
        assert!(matches!(
            result,
            Err(TeletextGenerationStoreError::Validation(
                TeletextPublicationValidationError::Association
            ))
        ));
    }
    assert_eq!(validator.calls.load(Ordering::SeqCst), 3);
    let result =
        accept_effective_with_repository(&LegacyOnly, &save(), &validator, 1024, deadline).await;
    assert!(matches!(
        result,
        Err(TeletextGenerationStoreError::StorageUnavailable)
    ));
}

#[tokio::test]
async fn effective_generation_expired_before_native_does_not_use_base_storage() {
    let deadline = Instant::now();
    let validator = ExactNative {
        expected: save(),
        deadline,
        calls: AtomicUsize::new(0),
    };
    let result =
        accept_effective_with_repository(&LegacyOnly, &save(), &validator, 1024, deadline).await;
    assert!(matches!(
        result,
        Err(TeletextGenerationStoreError::Validation(
            TeletextPublicationValidationError::TimedOut
        ))
    ));
    assert_eq!(validator.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn optional_scoped_generation_ports_fail_without_legacy_fallback() {
    let deadline = Instant::now() + Duration::from_secs(30);
    let save = save();
    let generation = TeletextAcceptedGeneration {
        id: 12,
        request: save.request.clone(),
        artifact_version: save.artifact_version,
        artifact_digest: save.artifact_digest.clone(),
    };
    assert!(matches!(
        LegacyOnly.accept_effective(&save, 1024, deadline).await,
        Err(RepositoryError::Unavailable { .. })
    ));
    assert!(matches!(
        LegacyOnly.current_effective(9).await,
        Err(RepositoryError::Unavailable { .. })
    ));
    assert!(matches!(
        LegacyOnly
            .confirm_current_effective(&generation, &save.selection, 1024, deadline)
            .await,
        Err(RepositoryError::Unavailable { .. })
    ));
    let debug = format!("{save:?}");
    assert!(!debug.contains("private-document-fixture"));
    assert!(!debug.contains("private-artifact-fixture"));
    assert_eq!(TELETEXT_GENERATION_ARTIFACT_VERSION, 1);
    assert_eq!(TELETEXT_EFFECTIVE_GENERATION_ARTIFACT_VERSION, 2);
}
