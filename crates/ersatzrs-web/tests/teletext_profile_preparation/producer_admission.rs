//! Whole-channel producer admission precedes invocation and saved-source parsing.

use super::{channel_documents, designs, documents::DocumentFixture, generations, *};
use ersatzrs_app::{
    teletext_channel_documents::ChannelTeletextDocumentError,
    teletext_design_documents::{
        decode_teletext_design_documents, TeletextDesignDocumentError,
        TeletextDesignDocumentFailure,
    },
};
use ersatzrs_domain::TeletextDesignRevision;
use ersatzrs_ffmpeg::teletext::{EncodedTeletextAuthoredPage, EncodedTeletextAuthoredService};
use ersatzrs_web::host::{
    teletext_channel_documents::{
        ChannelTeletextDocumentEncodingError, TeletextChannelDocumentEncodingPolicy,
    },
    teletext_designs::{
        reload_original_teletext_design_revision, NativeTeletextDesignError,
        SavedOriginalTeletextDesign,
    },
    teletext_generations::{reload_accepted_teletext_generation, NativeTeletextGenerationError},
};

fn policy(input: usize, composition: usize) -> TeletextChannelDocumentEncodingPolicy {
    let mut policy = channel_documents::policy();
    policy.input.max_producers = input;
    policy.composition.max_producers = composition;
    policy
}

fn assert_no_requests(fixture: &Fixture) {
    for file in [
        "channel-calls",
        "channel-input-first.json",
        "channel-input-second.json",
    ] {
        assert!(
            !fixture.root.path().join(file).exists(),
            "producer request must not exist: {file}"
        );
    }
}

fn calls(fixture: &Fixture) -> Vec<u8> {
    std::fs::read(fixture.root.path().join("channel-calls")).unwrap()
}

fn assert_count_budget(error: TeletextDesignDocumentError) {
    assert!(
        matches!(
            error.reason,
            TeletextDesignDocumentFailure::Budget {
                dimension: "bindings" | "documents",
                count: 2,
                maximum: 1,
            }
        ),
        "expected whole-channel saved-input count rejection: {error:?}"
    );
    assert_eq!(error.binding, None);
}

fn assert_design_count_budget(error: &NativeTeletextDesignError) {
    let NativeTeletextDesignError::Document(error) = error else {
        panic!("producer count must fail before native parsing/encoding: {error:?}");
    };
    assert_count_budget(*error);
}

async fn revision(
    fixture: &Fixture,
    saved: &SavedOriginalTeletextDesign,
) -> TeletextDesignRevision {
    fixture
        .state
        .application
        .teletext_design_revision(
            saved.head().id,
            saved.head().revision,
            designs::budget().max_serialized_bytes,
        )
        .await
        .unwrap()
        .unwrap()
}

fn assert_same_service(
    actual: &EncodedTeletextAuthoredService,
    expected: &EncodedTeletextAuthoredService,
) {
    assert_eq!(actual.declared(), expected.declared());
    assert_eq!(actual.inventory(), expected.inventory());
    assert_eq!(designs::units(actual), designs::units(expected));
    assert_eq!(actual.magazine_units(), expected.magazine_units());
    for (actual, expected) in actual.pages().iter().zip(expected.pages()) {
        match (actual, expected) {
            (
                EncodedTeletextAuthoredPage::CompiledDisplay(actual),
                EncodedTeletextAuthoredPage::CompiledDisplay(expected),
            ) => {
                assert_eq!(
                    actual.normalized().original(),
                    expected.normalized().original()
                );
                assert_eq!(
                    actual.normalized().physical(),
                    expected.normalized().physical()
                );
            }
            (
                EncodedTeletextAuthoredPage::NativeDisplay(actual),
                EncodedTeletextAuthoredPage::NativeDisplay(expected),
            ) => assert_eq!(actual.composition(), expected.composition()),
            _ => panic!("synthetic admission fixture must preserve both display paths"),
        }
    }
    let actual = generations::preview(actual);
    let expected = generations::preview(expected);
    assert_eq!(actual.raster(), expected.raster());
    assert_eq!(actual.navigation(), expected.navigation());
}

#[tokio::test]
async fn either_lower_producer_cap_rejects_complete_selection_before_any_request() {
    let (mut fixture, _) = designs::persistent_pair(DocumentFixture::Authored).await;
    let before = designs::config(&fixture).await;
    let bindings = fixture
        .state
        .application
        .channel_teletext_bindings(1)
        .await
        .unwrap();
    let accepted = fixture
        .state
        .application
        .accepted_teletext_generation(1)
        .await
        .unwrap();
    for (input, composition) in [(1, 8), (8, 1)] {
        let error = channel_documents::candidate(&fixture, policy(input, composition))
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            ChannelTeletextDocumentEncodingError::Preparation(
                ChannelTeletextDocumentError::ProducerBudget {
                    count: 2,
                    maximum: 1
                }
            )
        ));
        assert_no_requests(&fixture);
    }
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
    assert_eq!(
        fixture
            .state
            .application
            .accepted_teletext_generation(1)
            .await
            .unwrap(),
        accepted
    );
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn exact_two_producer_cap_preserves_saved_native_models_and_preview() {
    let (mut fixture, _) = designs::persistent_pair(DocumentFixture::Authored).await;
    let before = designs::config(&fixture).await;
    let candidate = channel_documents::candidate(&fixture, policy(2, 2))
        .await
        .unwrap();
    assert_eq!(candidate.selection().bindings.len(), 2);
    assert_eq!(candidate.producers().len(), 2);
    assert_eq!(candidate.sources().len(), 2);
    let saved = designs::save(&fixture, candidate, None).await;
    for (input, composition) in [(2, 2), (8, 8)] {
        let restored = reload_original_teletext_design_revision(
            &fixture.state.application,
            saved.head().id,
            saved.head().revision,
            designs::budget(),
            policy(input, composition),
            designs::deadline(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(restored.selection(), saved.candidate().selection());
        assert_eq!(restored.digest(), saved.digest());
        assert_same_service(
            restored.encoded().unwrap(),
            saved.candidate().encoded().unwrap(),
        );
    }
    channel_documents::assert_requests(&fixture, 1, 2);
    assert_eq!(designs::config(&fixture).await, before);
    assert!(fixture
        .state
        .application
        .accepted_teletext_generation(1)
        .await
        .unwrap()
        .is_none());
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn off_accepts_zero_producers_without_requests_or_native_output() {
    let (mut fixture, _) = designs::persistent_pair(DocumentFixture::Authored).await;
    for selected in fixture
        .state
        .application
        .channel_teletext_bindings(1)
        .await
        .unwrap()
    {
        saved(
            fixture
                .state
                .application
                .disable_channel_teletext_binding(
                    1,
                    &selected.binding.addon_id,
                    selected.binding.revision,
                )
                .await
                .unwrap(),
        );
    }
    let before = designs::config(&fixture).await;
    let result = channel_documents::candidate(&fixture, policy(0, 0))
        .await
        .unwrap();
    assert!(result.selection().bindings.is_empty());
    assert!(result.producers().is_empty());
    assert!(result.sources().is_empty());
    assert!(result.encoded().is_none());
    assert!(result.metadata().is_none());
    assert!(result.descriptor().is_none());
    assert_no_requests(&fixture);
    assert_eq!(designs::config(&fixture).await, before);
    assert!(fixture
        .state
        .application
        .accepted_teletext_generation(1)
        .await
        .unwrap()
        .is_none());
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn lower_reload_caps_preserve_exact_revision_and_accepted_generation() {
    let (mut fixture, saved, pending) = generations::prepared().await;
    let accepted = generations::accepted(
        generations::accept(&fixture, &pending, saved.head().revision, None).await,
    );
    let before_revision = revision(&fixture, &saved).await;
    let before_generation = fixture
        .state
        .application
        .accepted_teletext_generation(1)
        .await
        .unwrap();
    let before_config = designs::config(&fixture).await;
    let before_calls = calls(&fixture);
    for (input, composition, documents) in [(1, 8, 8), (8, 1, 8), (8, 8, 1)] {
        let mut budget = designs::budget();
        budget.max_documents = documents;
        let mut limited = policy(input, composition);
        limited.encoding.max_units = 0; // Count rejection must precede encoding too.
        let error = reload_original_teletext_design_revision(
            &fixture.state.application,
            saved.head().id,
            saved.head().revision,
            budget,
            limited,
            designs::deadline(),
        )
        .await
        .unwrap_err();
        assert_design_count_budget(&error);
        let error = reload_accepted_teletext_generation(
            &fixture.state.application,
            1,
            budget,
            policy(input, composition),
            designs::deadline(),
        )
        .await
        .unwrap_err();
        let NativeTeletextGenerationError::Design(error) = error else {
            panic!("accepted reload must retain saved-input count diagnostics: {error:?}");
        };
        assert_design_count_budget(&error);
    }
    let recovered = reload_accepted_teletext_generation(
        &fixture.state.application,
        1,
        designs::budget(),
        policy(2, 2),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(recovered.generation(), accepted.generation());
    assert_same_service(recovered.service(), accepted.service());
    assert_eq!(revision(&fixture, &saved).await, before_revision);
    assert_eq!(
        fixture
            .state
            .application
            .accepted_teletext_generation(1)
            .await
            .unwrap(),
        before_generation
    );
    assert_eq!(designs::config(&fixture).await, before_config);
    assert_eq!(calls(&fixture), before_calls);
    channel_documents::assert_requests(&fixture, 1, 2);
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn saved_count_prescan_precedes_malformed_embedded_response_without_storage_mutation() {
    let (mut fixture, saved, _) = generations::prepared().await;
    let before_revision = revision(&fixture, &saved).await;
    let before_calls = calls(&fixture);
    let before_config = designs::config(&fixture).await;
    let mut envelope: serde_json::Value =
        serde_json::from_slice(&before_revision.document).unwrap();
    envelope["documents"][0]["response"] = serde_json::Value::String("{".to_owned());
    let fork = serde_json::to_vec(&envelope).unwrap();
    let mut budget = designs::budget();
    budget.max_documents = 1;
    assert_count_budget(decode_teletext_design_documents(&fork, budget).unwrap_err());
    budget.max_documents = 2;
    let error = decode_teletext_design_documents(&fork, budget).unwrap_err();
    assert_eq!(error.binding, Some(0));
    assert_eq!(error.reason, TeletextDesignDocumentFailure::Document);
    let restored = reload_original_teletext_design_revision(
        &fixture.state.application,
        saved.head().id,
        saved.head().revision,
        designs::budget(),
        policy(2, 2),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_same_service(
        restored.encoded().unwrap(),
        saved.candidate().encoded().unwrap(),
    );
    assert_eq!(revision(&fixture, &saved).await, before_revision);
    assert_eq!(designs::config(&fixture).await, before_config);
    assert_eq!(calls(&fixture), before_calls);
    assert!(fixture
        .state
        .application
        .accepted_teletext_generation(1)
        .await
        .unwrap()
        .is_none());
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
