//! Frozen-original acceptance/recovery and a synthetic source/output seam.
//! These tests do not prove real playback, suitable-client rendering or full acceptance.
//! Installed shell fixtures inherit the parent's intentional Unix-only scope.

use super::{channel_documents, designs, documents::DocumentFixture, *};
use ersatzrs_core::repositories::{
    TeletextGenerationMutation as Mutation, TeletextGenerationRejection as Rejection,
    TeletextPublicationMutation,
};
use ersatzrs_domain::TeletextPublicationRequest;
use ersatzrs_web::host::{
    teletext_designs::{
        request_original_teletext_design_publication, NativeTeletextDesignError,
        SavedOriginalTeletextDesign,
    },
    teletext_generations::{
        accept_original_teletext_generation, reload_accepted_teletext_generation,
        AcceptedTeletextOutputCandidate, NativeTeletextGenerationError, TeletextGenerationTarget,
    },
    teletext_preview::{project_teletext_preview, TeletextPreviewBudget},
};

// This module connects the native SQLite fixture to a real command policy and
// independently constructed/observed TS bytes. It intentionally runs no FFmpeg
// process and makes no actual-client or complete Level 2.5 acceptance claim.
#[path = "accepted_transport_flow.rs"]
mod accepted_transport_flow;

async fn request(
    fixture: &Fixture,
    saved: &SavedOriginalTeletextDesign,
    previous: Option<i64>,
) -> TeletextPublicationRequest {
    match request_original_teletext_design_publication(
        &fixture.state.application,
        saved.head().id,
        saved.head().revision,
        saved.digest().to_owned(),
        saved.head().revision,
        previous,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    {
        TeletextPublicationMutation::Requested(value) => value.request().clone(),
        TeletextPublicationMutation::Rejected(reason) => panic!("unexpected rejection: {reason:?}"),
    }
}

async fn accept(
    fixture: &Fixture,
    request: &TeletextPublicationRequest,
    head: i64,
    previous: Option<i64>,
) -> Mutation<AcceptedTeletextOutputCandidate> {
    accept_original_teletext_generation(
        &fixture.state.application,
        TeletextGenerationTarget {
            request: request.clone(),
            expected_design_head: head,
            expected_generation_id: previous,
        },
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
}

fn accepted(result: Mutation<AcceptedTeletextOutputCandidate>) -> AcceptedTeletextOutputCandidate {
    match result {
        Mutation::Accepted(value) => value,
        Mutation::Rejected(reason) => panic!("unexpected generation rejection: {reason:?}"),
    }
}

async fn recover(fixture: &Fixture) -> Option<AcceptedTeletextOutputCandidate> {
    reload_accepted_teletext_generation(
        &fixture.state.application,
        1,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
}

async fn count(fixture: &Fixture) -> i64 {
    sqlx::query_scalar(r#"SELECT COUNT(*) FROM "TeletextAcceptedGeneration""#)
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap()
}

fn calls(fixture: &Fixture) -> Vec<u8> {
    std::fs::read(fixture.root.path().join("channel-calls")).unwrap()
}

fn assert_same_output(
    actual: &AcceptedTeletextOutputCandidate,
    expected: &AcceptedTeletextOutputCandidate,
) {
    assert_eq!(actual.generation(), expected.generation());
    assert_eq!(actual.selection(), expected.selection());
    assert_eq!(
        designs::units(actual.service()),
        designs::units(expected.service())
    );
    assert_eq!(
        actual.service().magazine_units(),
        expected.service().magazine_units()
    );
    let first = preview(actual.service());
    let second = preview(expected.service());
    assert_eq!(first.raster(), second.raster());
    assert_eq!(first.navigation(), second.navigation());
    assert_eq!(first.raster().width(), 56 * 12);
    assert!(first.raster().diagnostics().is_empty());
}

fn preview(
    service: &ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService,
) -> ersatzrs_web::host::teletext_preview::TeletextNativePreview<'_> {
    project_teletext_preview(
        service,
        channel_documents::address(124, 32),
        ersatzrs_domain::TeletextColourPixelOptions {
            reveal_concealed: false,
            flash_phase: ersatzrs_domain::TeletextFlashPhase::Original,
        },
        TeletextPreviewBudget {
            max_pages: 16,
            max_model_records: 32_768,
            max_pixels: 168_000,
            max_diagnostics: 1400,
            max_work: 50_000_000,
        },
    )
    .unwrap()
}

async fn prepared() -> (
    Fixture,
    SavedOriginalTeletextDesign,
    TeletextPublicationRequest,
) {
    let (fixture, _) = designs::persistent_pair(DocumentFixture::Authored).await;
    let candidate = channel_documents::candidate(&fixture, channel_documents::policy())
        .await
        .unwrap();
    let saved = designs::save(&fixture, candidate, None).await;
    let pending = request(&fixture, &saved, None).await;
    (fixture, saved, pending)
}

#[tokio::test]
async fn pending_does_not_activate_and_explicit_acceptance_recovers_exact_native_service() {
    let (mut fixture, saved, pending) = prepared().await;
    let before = designs::config(&fixture).await;
    let bindings = fixture
        .state
        .application
        .channel_teletext_bindings(1)
        .await
        .unwrap();
    let invoked = calls(&fixture);
    assert!(recover(&fixture).await.is_none());
    assert_eq!(count(&fixture).await, 0);
    let result = accepted(accept(&fixture, &pending, 1, None).await);
    assert_eq!(result.generation().request, pending);
    assert_eq!(result.generation().artifact_version, 1);
    assert_eq!(result.generation().artifact_digest.len(), 64);
    assert_eq!(result.selection(), saved.candidate().selection());
    assert_eq!(
        designs::units(result.service()),
        designs::units(saved.candidate().encoded().unwrap())
    );
    for private in [
        saved.digest(),
        result.generation().artifact_digest.as_str(),
        "THEME",
        "night",
        "response",
    ] {
        assert!(!format!("{result:?}").contains(private));
    }
    fixture = fixture.reopen().await;
    let recovered = recover(&fixture).await.unwrap();
    assert_same_output(&recovered, &result);
    assert_eq!(calls(&fixture), invoked);
    assert_eq!(designs::config(&fixture).await, before);
    assert_eq!(
        fixture
            .state
            .application
            .channel_teletext_bindings(1)
            .await
            .unwrap(),
        bindings
    );
    assert!(fixture
        .state
        .application
        .accepted_teletext_generation(2)
        .await
        .unwrap()
        .is_none());
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn new_draft_and_pending_leave_old_output_until_independent_generation_cas_accepts() {
    let (mut fixture, first, pending) = prepared().await;
    let old = accepted(accept(&fixture, &pending, 1, None).await);
    let next = channel_documents::candidate(&fixture, channel_documents::policy())
        .await
        .unwrap();
    let next = designs::save(&fixture, next, Some((first.head().id, 1))).await;
    assert_same_output(&recover(&fixture).await.unwrap(), &old);
    let new_pending = request(&fixture, &next, Some(pending.id)).await;
    let invoked = calls(&fixture);
    assert_same_output(&recover(&fixture).await.unwrap(), &old);
    assert!(matches!(
        accept(&fixture, &new_pending, 2, None).await,
        Mutation::Rejected(Rejection::GenerationConflict)
    ));
    assert_eq!(count(&fixture).await, 1);
    assert_same_output(&recover(&fixture).await.unwrap(), &old);
    let new = accepted(accept(&fixture, &new_pending, 2, Some(old.generation().id)).await);
    assert_ne!(new.generation().id, old.generation().id);
    assert_eq!(new.generation().request.revision, 2);
    assert_eq!(
        new.generation().artifact_digest,
        old.generation().artifact_digest
    );
    assert_eq!(count(&fixture).await, 2);
    assert!(matches!(
        accept(&fixture, &new_pending, 2, Some(old.generation().id)).await,
        Mutation::Rejected(Rejection::AlreadyAccepted)
    ));
    fixture = fixture.reopen().await;
    assert_same_output(&recover(&fixture).await.unwrap(), &new);
    assert_eq!(calls(&fixture), invoked);
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn changed_profile_off_and_invalid_installation_deny_recovery_without_erasing_history() {
    for mode in 0..3 {
        let (mut fixture, saved, pending) = prepared().await;
        let result = accepted(accept(&fixture, &pending, 1, None).await);
        let before = designs::config(&fixture).await;
        let invoked = calls(&fixture);
        let original = &saved.candidate().selection().bindings[0];
        match mode {
            0 => {
                saved_profile_change(&fixture, original).await;
            }
            1 => {
                let disabled = fixture
                    .state
                    .application
                    .save_channel_teletext_binding(&ChannelTeletextBindingSave {
                        channel_id: 1,
                        addon_id: original.addon_id.clone(),
                        profile_id: original.profile_id,
                        enabled: false,
                        expected_installation: original.installation.clone(),
                        expected_profile_revision: original.profile_revision,
                        expected_binding_revision: Some(original.binding_revision),
                    })
                    .await
                    .unwrap();
                assert!(matches!(disabled, TeletextProfileMutation::Saved(_)));
            }
            _ => {
                sqlx::query(
                    r#"UPDATE "AddonInstallation" SET "ManifestJson"='{}' WHERE "AddonId"=?"#,
                )
                .bind(&original.addon_id)
                .execute(&fixture.state.pool)
                .await
                .unwrap();
            }
        }
        fixture = fixture.reopen().await;
        assert!(recover(&fixture).await.is_none());
        assert_eq!(count(&fixture).await, 1);
        assert_eq!(
            fixture
                .state
                .application
                .accepted_teletext_generation(1)
                .await
                .unwrap(),
            Some(result.generation().clone())
        );
        assert_eq!(designs::config(&fixture).await, before);
        assert_eq!(calls(&fixture), invoked);
        assert!(fixture.playout.try_recv().is_err());
        fixture.state.pool.close().await;
    }
}

async fn saved_profile_change(
    fixture: &Fixture,
    original: &ersatzrs_domain::TeletextSelectionSnapshot,
) {
    let result = fixture
        .state
        .application
        .save_teletext_profile(&TeletextProfileSave {
            addon_id: original.addon_id.clone(),
            name: "Changed after acceptance".to_owned(),
            settings: vec![TeletextProfileSetting {
                key: "THEME".to_owned(),
                value: TeletextProfileValue::Literal("day".to_owned()),
            }],
            expected_installation: original.installation.clone(),
            existing: Some((original.profile_id, original.profile_revision)),
        })
        .await
        .unwrap();
    assert!(matches!(result, TeletextProfileMutation::Saved(_)));
}

#[tokio::test]
async fn compiler_and_deadline_failures_preserve_the_exact_accepted_pointer() {
    let (mut fixture, saved, pending) = prepared().await;
    let original = accepted(accept(&fixture, &pending, 1, None).await);
    let before = designs::config(&fixture).await;
    let invoked = calls(&fixture);
    let target = || TeletextGenerationTarget {
        request: pending.clone(),
        expected_design_head: saved.head().revision,
        expected_generation_id: Some(original.generation().id),
    };
    let expired = accept_original_teletext_generation(
        &fixture.state.application,
        target(),
        designs::budget(),
        channel_documents::policy(),
        std::time::Instant::now(),
    )
    .await;
    assert!(matches!(
        expired,
        Err(NativeTeletextGenerationError::Design(
            NativeTeletextDesignError::TimedOut
        ))
    ));
    let mut policy = channel_documents::policy();
    policy.encoding.max_pages = 0;
    let invalid = accept_original_teletext_generation(
        &fixture.state.application,
        target(),
        designs::budget(),
        policy,
        designs::deadline(),
    )
    .await;
    assert!(matches!(
        invalid,
        Err(NativeTeletextGenerationError::Design(
            NativeTeletextDesignError::Encoding(_)
        ))
    ));
    let mut policy = channel_documents::policy();
    policy.encoding.max_display_updates = Some(0);
    let exhausted = accept_original_teletext_generation(
        &fixture.state.application,
        target(),
        designs::budget(),
        policy,
        designs::deadline(),
    )
    .await;
    assert!(matches!(
        exhausted,
        Err(NativeTeletextGenerationError::Design(
            NativeTeletextDesignError::Encoding(_)
        ))
    ));
    assert_eq!(count(&fixture).await, 1);
    assert_same_output(&recover(&fixture).await.unwrap(), &original);
    assert_eq!(designs::config(&fixture).await, before);
    assert_eq!(calls(&fixture), invoked);
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn corrupted_recorded_artifact_never_recovers_as_absent_or_unchanged_output() {
    let (fixture, _, pending) = prepared().await;
    let accepted = accepted(accept(&fixture, &pending, 1, None).await);
    let before = calls(&fixture);
    // Deliberate test-only corruption bypasses immutable storage enforcement.
    sqlx::query(r#"DROP TRIGGER "TR_TeletextAcceptedGeneration_ImmutableUpdate""#)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "TeletextAcceptedGeneration" SET "ArtifactDigest"=? WHERE "Id"=?"#)
        .bind("0".repeat(64))
        .bind(accepted.generation().id)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    let result = reload_accepted_teletext_generation(
        &fixture.state.application,
        1,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await;
    assert!(matches!(
        result,
        Err(NativeTeletextGenerationError::ArtifactMismatch)
    ));
    assert_eq!(count(&fixture).await, 1);
    assert_eq!(calls(&fixture), before);
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn source_less_nonempty_guide_never_gains_frozen_original_authority() {
    let (mut fixture, _) = designs::persistent_pair(DocumentFixture::Authored).await;
    let channels = [TeletextGuideChannel {
        id: 2,
        number: "2",
        name: "Synthetic guide source",
        programmes: &[],
    }];
    let candidate = ersatzrs_web::host::teletext_channel_documents::prepare_channel_teletext_document_candidate(
        &fixture.state.application, 1, Some(SECONDARY), &guide(&channels), channel_documents::policy(),
    ).await.unwrap();
    assert!(candidate.guide_source().is_none());
    let saved = designs::save(&fixture, candidate, None).await;
    let pending = request(&fixture, &saved, None).await;
    let before = calls(&fixture);
    let config = designs::config(&fixture).await;
    assert!(matches!(
        accept(&fixture, &pending, 1, None).await,
        Mutation::Rejected(Rejection::GuideFreshnessRequired)
    ));
    assert!(recover(&fixture).await.is_none());
    assert_eq!(count(&fixture).await, 0);
    assert_eq!(calls(&fixture), before);
    assert_eq!(designs::config(&fixture).await, config);
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn historical_source_bound_pending_requires_current_guide_role_before_acceptance() {
    use ersatzrs_core::repositories::TeletextGuideSourceCheckLimits;
    use ersatzrs_infra::xmltv::GuideSnapshotDigestLimits;
    use ersatzrs_web::host::{
        teletext_designs::request_source_bound_original_teletext_design_publication,
        teletext_guide::capture_source_bound_teletext_guide,
    };
    let (mut fixture, _) = designs::persistent_pair(DocumentFixture::Authored).await;
    super::guide_capture::seed_guide(&fixture).await;
    let limits = TeletextGuideSourceCheckLimits {
        max_records: 1000,
        max_source_bytes: 1024 * 1024,
        max_model_nodes: 10_000,
        max_model_bytes: 16 * 1024 * 1024,
        max_digest_fields: 100_000,
        max_digest_bytes: 16 * 1024 * 1024,
    };
    let mut read = super::guide_capture::read_budget();
    let captured = capture_source_bound_teletext_guide(
        &fixture.state.pool,
        &[2],
        guide(&[]).captured_at,
        &mut read,
        designs::deadline(),
        GuideSnapshotDigestLimits {
            max_fields: limits.max_digest_fields,
            max_bytes: limits.max_digest_bytes,
        },
    )
    .await
    .unwrap();
    let candidate = captured
        .prepare_document_candidate(
            &fixture.state.application,
            1,
            Some(SECONDARY),
            GENERATION,
            channel_documents::policy(),
            &mut read,
        )
        .await
        .unwrap();
    assert!(candidate.guide_source().is_some());
    let saved = designs::save(&fixture, candidate, None).await;
    let pending = match request_source_bound_original_teletext_design_publication(
        &fixture.state.application,
        saved.head().id,
        1,
        saved.digest().to_owned(),
        1,
        None,
        designs::budget(),
        channel_documents::policy(),
        limits,
        designs::deadline(),
    )
    .await
    .unwrap()
    {
        TeletextPublicationMutation::Requested(value) => value.request().clone(),
        TeletextPublicationMutation::Rejected(reason) => {
            panic!("unexpected source rejection: {reason:?}")
        }
    };
    let before = calls(&fixture);
    assert_eq!(pending.channel_id, 1);
    assert!(matches!(
        accept(&fixture, &pending, 1, None).await,
        Mutation::Rejected(Rejection::GuideFreshnessRequired)
    ));
    assert_eq!(count(&fixture).await, 0);
    assert!(recover(&fixture).await.is_none());
    assert_eq!(calls(&fixture), before);
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
