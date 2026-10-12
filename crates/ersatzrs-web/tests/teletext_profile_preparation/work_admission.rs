//! Process-wide native admission, retaining ordinary playback and accepted state.
//! Actual worker cancellation/settlement ownership is covered by the infra and
//! host blocking-worker tests; this slice checks the installed application flow.

use super::{channel_documents, designs, generations, *};
use ersatzrs_app::commands::SetAdminSetting;
use ersatzrs_web::host::{
    teletext_channel_documents::ChannelTeletextDocumentEncodingError,
    teletext_designs::{reload_original_teletext_design_revision, NativeTeletextDesignError},
    teletext_generations::{
        accept_original_teletext_generation, NativeTeletextGenerationError,
        TeletextGenerationTarget,
    },
    teletext_work::{begin, TeletextWorkError, MAX_CONCURRENT_SETTING},
};
use std::time::Instant;

async fn configure_limit(fixture: &Fixture, value: &str) {
    fixture
        .state
        .application
        .set_admin_setting(SetAdminSetting {
            key: MAX_CONCURRENT_SETTING,
            value: value.to_owned(),
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn saturated_preparation_and_acceptance_preserve_the_exact_accepted_output() {
    let (mut fixture, saved, pending) = generations::prepared().await;
    assert!(fixture
        .state
        .application
        .get_optional_setting(MAX_CONCURRENT_SETTING)
        .await
        .unwrap()
        .is_none());
    let original = generations::accepted(generations::accept(&fixture, &pending, 1, None).await);
    let before = designs::config(&fixture).await;
    let calls = std::fs::read(fixture.root.path().join("channel-calls")).unwrap();
    let guard = begin(&fixture.state.application, None).await.unwrap();
    let failed = channel_documents::candidate(&fixture, channel_documents::policy()).await;
    assert!(matches!(
        failed,
        Err(ChannelTeletextDocumentEncodingError::Work(
            TeletextWorkError::Saturated
        ))
    ));
    let failed = accept_original_teletext_generation(
        &fixture.state.application,
        TeletextGenerationTarget {
            request: pending,
            expected_design_head: saved.head().revision,
            expected_generation_id: Some(original.generation().id),
        },
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await;
    assert!(matches!(
        failed,
        Err(NativeTeletextGenerationError::Design(
            NativeTeletextDesignError::Work(TeletextWorkError::Saturated)
        ))
    ));
    assert_eq!(
        fixture
            .state
            .application
            .accepted_teletext_generation(1)
            .await
            .unwrap(),
        Some(original.generation().clone())
    );
    assert_eq!(
        std::fs::read(fixture.root.path().join("channel-calls")).unwrap(),
        calls
    );
    assert_eq!(designs::config(&fixture).await, before);
    assert!(fixture.playout.try_recv().is_err());
    drop(guard);
    generations::assert_same_output(&generations::recover(&fixture).await.unwrap(), &original);
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn saturation_precedes_missing_revision_lookup_and_releases_after_owner() {
    let mut fixture = Fixture::new_persistent().await;
    let before = designs::config(&fixture).await;
    let guard = begin(&fixture.state.application, None).await.unwrap();
    let result = reload_original_teletext_design_revision(
        &fixture.state.application,
        999,
        1,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await;
    assert!(matches!(
        result,
        Err(NativeTeletextDesignError::Work(
            TeletextWorkError::Saturated
        ))
    ));
    assert!(matches!(
        begin(&fixture.state.application, None).await,
        Err(TeletextWorkError::Saturated)
    ));
    drop(guard);
    let result = reload_original_teletext_design_revision(
        &fixture.state.application,
        999,
        1,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    assert!(result.is_none());
    let guard = begin(&fixture.state.application, None).await.unwrap();
    drop(guard);
    assert_eq!(designs::config(&fixture).await, before);
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn invalid_configuration_rejects_installed_work_without_invocation_or_slot() {
    let (mut fixture, _) = designs::persistent_pair(documents::DocumentFixture::Authored).await;
    for value in [
        "0",
        "not-a-number",
        "-1",
        " 1",
        "1 ",
        "184467440737095516160",
    ] {
        configure_limit(&fixture, value).await;
        let before = designs::config(&fixture).await;
        assert!(matches!(
            begin(&fixture.state.application, None).await,
            Err(TeletextWorkError::InvalidConfiguration)
        ));
        assert!(matches!(
            channel_documents::candidate(&fixture, channel_documents::policy()).await,
            Err(ChannelTeletextDocumentEncodingError::Work(
                TeletextWorkError::InvalidConfiguration
            ))
        ));
        assert!(!fixture.root.path().join("channel-calls").exists());
        assert_eq!(designs::config(&fixture).await, before);
        assert!(fixture.playout.try_recv().is_err());
    }
    configure_limit(&fixture, "1").await;
    let guard = begin(&fixture.state.application, None).await.unwrap();
    assert!(matches!(
        begin(&fixture.state.application, None).await,
        Err(TeletextWorkError::Saturated)
    ));
    drop(guard);
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn lowering_configured_capacity_blocks_new_work_without_preempting_owners() {
    let mut fixture = Fixture::new_persistent().await;
    configure_limit(&fixture, "2").await;
    let first = begin(&fixture.state.application, None).await.unwrap();
    let shared_application = fixture.state.application.clone();
    let second = begin(&shared_application, None).await.unwrap();
    assert!(matches!(
        begin(&fixture.state.application, None).await,
        Err(TeletextWorkError::Saturated)
    ));
    configure_limit(&fixture, "1").await;
    let before = designs::config(&fixture).await;
    assert!(matches!(
        begin(&shared_application, None).await,
        Err(TeletextWorkError::Saturated)
    ));
    drop(first);
    assert!(matches!(
        begin(&fixture.state.application, None).await,
        Err(TeletextWorkError::Saturated)
    ));
    drop(second);
    let admitted = begin(&shared_application, None).await.unwrap();
    assert!(matches!(
        begin(&fixture.state.application, None).await,
        Err(TeletextWorkError::Saturated)
    ));
    drop(admitted);
    assert_eq!(designs::config(&fixture).await, before);
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn expired_deadline_precedes_configuration_and_does_not_consume_capacity() {
    let mut fixture = Fixture::new_persistent().await;
    let holder = begin(&fixture.state.application, None).await.unwrap();
    configure_limit(&fixture, "0").await;
    let before = designs::config(&fixture).await;
    assert!(matches!(
        begin(&fixture.state.application, Some(Instant::now())).await,
        Err(TeletextWorkError::TimedOut)
    ));
    assert_eq!(designs::config(&fixture).await, before);
    configure_limit(&fixture, "1").await;
    assert!(matches!(
        begin(&fixture.state.application, None).await,
        Err(TeletextWorkError::Saturated)
    ));
    drop(holder);
    let admitted = begin(&fixture.state.application, Some(designs::deadline()))
        .await
        .unwrap();
    drop(admitted);
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
