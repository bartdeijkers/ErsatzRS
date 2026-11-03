//! Real production package lifecycle through the ordinary selected-guide loader.
//! Current SSR proves sealed output authorization, not a new AV/PMT/client trial.

use super::*;
use ersatzrs_domain::{
    Channel, FFmpegProfile, Playout, TeletextInstallationSnapshot, TeletextProfileAvailability,
};

async fn runtime(fixture: &Fixture) {
    let mut value: Value = serde_json::from_str(include_str!(
        "../../../../fixtures/teletext-output-policy-v1.json"
    ))
    .unwrap();
    value["version"] = json!(4);
    value["guide_limits"] = policy()["guide_limits"].clone();
    value["live_updates"] = json!({"poll_interval_ms":25,"recovery_timeout_ms":2000});
    authoring::setting(fixture, "teletext.output.policy", &value.to_string()).await;
}

#[derive(Debug, PartialEq)]
struct Playback {
    channels: Vec<Channel>,
    profiles: Vec<FFmpegProfile>,
    playouts: Vec<Playout>,
    items: Vec<PlayoutItem>,
    settings: Vec<(Option<String>, Option<String>)>,
}

async fn playback(fixture: &Fixture) -> Playback {
    let repositories = &fixture.state.repositories;
    let channels = vec![
        repositories.channel.get_with_profile(1).await.unwrap(),
        repositories.channel.get_with_profile(2).await.unwrap(),
    ];
    let ids: Vec<i64> = sqlx::query_scalar("SELECT Id FROM Playout ORDER BY Id")
        .fetch_all(&fixture.state.pool)
        .await
        .unwrap();
    let mut playouts = Vec::new();
    let mut items = Vec::new();
    let now = Utc::now();
    for id in ids {
        playouts.push(repositories.playout.get_with_anchor(id).await.unwrap());
        items.extend(
            repositories
                .playout
                .items_in_range(
                    id,
                    now - chrono::Duration::days(1),
                    now + chrono::Duration::days(1),
                )
                .await
                .unwrap(),
        );
    }
    Playback {
        channels,
        profiles: repositories.ffmpeg_profile.list().await.unwrap(),
        playouts,
        items,
        settings: designs::config(fixture).await,
    }
}

async fn current(fixture: &Fixture, generation: i64, available: bool) {
    let html = editorial_http::rendered(
        fixture,
        "/channels/1/teletext?guide_preview=1&guide_page=200&guide_subcode=0000",
    )
    .await;
    assert!(html.contains("data-testid=\"teletext-current-guide\""));
    assert_eq!(
        html.contains("data-testid=\"teletext-current-guide-model\""),
        available
    );
    if available {
        assert!(html.contains(&format!("data-generation=\"{generation}\"")));
    } else {
        assert!(html.contains("role=\"alert\""));
    }
    // The same native admission is immediately reusable after the real read.
    drop(begin(&fixture.state.application, None).await.unwrap());
}

async fn availability(fixture: &Fixture, expected: TeletextProfileAvailability) {
    let profiles = fixture
        .state
        .application
        .list_teletext_profiles(TVGUIDE)
        .await
        .unwrap();
    assert_eq!(profiles.len(), 2);
    for profile in profiles {
        assert_eq!(profile.availability, expected);
    }
    for channel in [1, 2] {
        let bindings = fixture
            .state
            .application
            .channel_teletext_bindings(channel)
            .await
            .unwrap();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].availability, expected);
    }
}

async fn deny(
    fixture: &Fixture,
    identity: TeletextAuthoringIdentity,
    original_before: &Original,
    generation: &TeletextAcceptedGeneration,
) {
    let before = state(fixture).await;
    let metadata = fixture
        .state
        .application
        .observe_current_guide_refresh(
            1,
            ersatzrs_core::repositories::TeletextGuideRefreshRole::Base,
            designs::budget().max_serialized_bytes,
        )
        .await
        .unwrap()
        .unwrap();
    // Denial must be immediate even while the previous role could otherwise
    // authorize last-good output. Coordinator revocation is separate bookkeeping.
    assert!(metadata.available_until() > Utc::now());
    assert_eq!(&metadata.seed.generation, generation);
    current(fixture, generation.id, false).await;
    let observed = observe(fixture, identity, 200).await;
    assert_eq!(
        observed.state,
        TeletextAuthoringPublicationState::Unavailable
    );
    assert!(!observed.can_request && !observed.can_publish);
    for publish in [false, true] {
        let response = post(fixture, identity, 200, publish).await;
        assert!(response.contains_key("error"));
        assert!(!response.contains_key("saved"));
    }
    assert_eq!(state(fixture).await, before);
    assert_eq!(&accepted(fixture, false).await, generation);
    assert_eq!(
        &original(fixture, identity, TeletextAuthoringScope::Base, 200).await,
        original_before
    );
    drop(begin(&fixture.state.application, None).await.unwrap());
}

async fn repair_profile(fixture: &Fixture) {
    let binding = fixture
        .state
        .application
        .channel_teletext_bindings(1)
        .await
        .unwrap()[0]
        .binding
        .clone();
    let profiles = fixture
        .state
        .application
        .list_teletext_profiles(TVGUIDE)
        .await
        .unwrap();
    let previous = &profiles
        .iter()
        .find(|read| read.profile.id == binding.profile_id)
        .unwrap()
        .profile;
    let installation = fixture
        .state
        .application
        .teletext_installation_snapshot(TVGUIDE)
        .await
        .unwrap()
        .snapshot
        .unwrap();
    let saved = saved(
        fixture
            .state
            .application
            .save_teletext_profile(&TeletextProfileSave {
                addon_id: TVGUIDE.to_owned(),
                name: previous.name.clone(),
                settings: previous.settings.clone(),
                expected_installation: installation,
                existing: Some((previous.id, previous.revision)),
            })
            .await
            .unwrap(),
    )
    .profile;
    assert_eq!(saved.id, previous.id);
    assert_eq!(saved.revision, previous.revision + 1);
    assert_eq!(saved.settings, previous.settings);
    let current = fixture
        .state
        .application
        .channel_teletext_bindings(1)
        .await
        .unwrap();
    fixture
        .bind(1, &saved, Some(current[0].binding.revision))
        .await;
    assert_eq!(
        fixture
            .state
            .application
            .channel_teletext_bindings(1)
            .await
            .unwrap()[0]
            .availability,
        TeletextProfileAvailability::Configured
    );
}

async fn republish(
    fixture: &Fixture,
) -> (
    TeletextAuthoringIdentity,
    Original,
    TeletextAcceptedGeneration,
) {
    let before = state(fixture).await;
    let identity = http_prepare(fixture).await;
    let captured = original(fixture, identity, TeletextAuthoringScope::Base, 200).await;
    assert_eq!(
        state(fixture).await,
        before,
        "preparation never selects output"
    );
    let observed = observe(fixture, identity, 200).await;
    assert_eq!(
        observed.state,
        TeletextAuthoringPublicationState::Unrequested
    );
    assert!(observed.can_request && !observed.can_publish);
    for (publish, expected) in [
        (false, "publication-requested"),
        (true, "publication-selected"),
    ] {
        let response = post(fixture, identity, 200, publish).await;
        assert_eq!(response.get("saved").map(String::as_str), Some(expected));
        assert!(!response.contains_key("error"));
    }
    let generation = accepted(fixture, false).await;
    assert_eq!(generation.request.design_id, identity.design_id);
    assert_eq!(generation.artifact_version, 5);
    let after = state(fixture).await;
    assert_eq!(after.requests.len(), before.requests.len() + 1);
    assert_eq!(
        &after.requests[..before.requests.len()],
        before.requests.as_slice()
    );
    assert_eq!(after.accepted.len(), before.accepted.len() + 1);
    assert_eq!(
        &after.accepted[..before.accepted.len()],
        before.accepted.as_slice()
    );
    current(fixture, generation.id, true).await;
    (identity, captured, generation)
}

async fn remove_or_disable(fixture: &Fixture, uninstall: bool) {
    let application = &fixture.state.application;
    let expected = if uninstall {
        application
            .uninstall_addon(TVGUIDE.to_owned(), false)
            .await
            .unwrap();
        TeletextProfileAvailability::MissingInstallation
    } else {
        application.disable_addon(TVGUIDE.to_owned()).await.unwrap();
        TeletextProfileAvailability::DisabledInstallation
    };
    availability(fixture, expected).await;
}

async fn restore_package(
    fixture: &Fixture,
    uninstall: bool,
    source: &str,
    previous: &TeletextInstallationSnapshot,
) {
    let application = &fixture.state.application;
    if uninstall {
        application
            .install_addon(source.to_owned(), TVGUIDE.to_owned(), false)
            .await
            .unwrap();
        availability(fixture, TeletextProfileAvailability::DisabledInstallation).await;
    }
    application.enable_addon(TVGUIDE.to_owned()).await.unwrap();
    availability(fixture, TeletextProfileAvailability::StaleInstallation).await;
    let current = application
        .teletext_installation_snapshot(TVGUIDE)
        .await
        .unwrap()
        .snapshot
        .unwrap();
    assert_ne!(current.state_revision, previous.state_revision);
    assert_eq!(current.version, "0.1.2");
    assert_eq!(current.package_sha256, previous.package_sha256);
    assert_eq!(current.content_sha256, previous.content_sha256);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the exact production addon bundle and isolated state/data directories"]
async fn production_guide_package_disable_remove_and_explicit_recovery_preserve_originals() {
    let (mut fixture, _) = fixture().await;
    runtime(&fixture).await;
    let unchanged = playback(&fixture).await;
    let source: String =
        sqlx::query_scalar("SELECT SourceId FROM AddonInstallation WHERE AddonId=?")
            .bind(TVGUIDE)
            .fetch_one(&fixture.state.pool)
            .await
            .unwrap();
    let installation = fixture
        .state
        .application
        .teletext_installation_snapshot(TVGUIDE)
        .await
        .unwrap()
        .snapshot
        .unwrap();
    let (mut identity, mut captured, mut generation) = base(&fixture).await;
    current(&fixture, generation.id, true).await;
    let mut originals = Vec::new();
    for uninstall in [false, true] {
        let before = state(&fixture).await;
        remove_or_disable(&fixture, uninstall).await;
        deny(&fixture, identity, &captured, &generation).await;
        assert_eq!(state(&fixture).await, before);
        restore_package(&fixture, uninstall, &source, &installation).await;
        deny(&fixture, identity, &captured, &generation).await;
        repair_profile(&fixture).await;
        // A valid repaired profile and explicit rebind do not select old pages.
        deny(&fixture, identity, &captured, &generation).await;
        originals.push((identity, captured));
        (identity, captured, generation) = republish(&fixture).await;
        assert_eq!(playback(&fixture).await, unchanged);
        assert!(fixture.playout.try_recv().is_err());
    }
    originals.push((identity, captured));
    let persisted = state(&fixture).await;
    fixture = fixture.reopen().await;
    assert_eq!(state(&fixture).await, persisted);
    for (identity, expected) in originals {
        assert_eq!(
            original(&fixture, identity, TeletextAuthoringScope::Base, 200).await,
            expected
        );
    }
    assert_eq!(accepted(&fixture, false).await, generation);
    current(&fixture, generation.id, true).await;
    assert_eq!(playback(&fixture).await, unchanged);
    assert!(fixture.playout.try_recv().is_err());
}
