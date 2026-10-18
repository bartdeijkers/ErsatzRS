//! Explicit package versions change current templates without rewriting history.
use super::*;
use ersatzrs_core::repositories::{TeletextPublicationMutation, TeletextPublicationRejection};
use ersatzrs_web::host::teletext_designs::request_original_teletext_design_publication;

#[path = "template_revisions/history.rs"]
mod history;
#[path = "template_revisions/package.rs"]
mod package;
#[path = "template_revisions/resource_exports.rs"]
mod resource_exports;
#[path = "template_revisions/resources.rs"]
mod resources;

async fn revalidate(fixture: &Fixture, profile: &TeletextProfile) -> TeletextProfile {
    let installation = fixture
        .state
        .application
        .teletext_installation_snapshot(ADDON)
        .await
        .unwrap()
        .snapshot
        .unwrap();
    saved(
        fixture
            .state
            .application
            .save_teletext_profile(&TeletextProfileSave {
                addon_id: ADDON.to_owned(),
                name: profile.name.clone(),
                settings: profile.settings.clone(),
                expected_installation: installation,
                existing: Some((profile.id, profile.revision)),
            })
            .await
            .unwrap(),
    )
    .profile
}
async fn stale_publication(
    fixture: &Fixture,
    design: &SavedOriginalTeletextDesign,
    previous: Option<i64>,
    expected: TeletextPublicationRejection,
) {
    match request_original_teletext_design_publication(
        &fixture.state.application,
        design.head().id,
        1,
        design.digest().to_owned(),
        1,
        previous,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    {
        TeletextPublicationMutation::Rejected(actual) => assert_eq!(actual, expected),
        TeletextPublicationMutation::Requested(_) => panic!("stale template cannot become pending"),
    }
}

#[tokio::test]
#[ignore = "requires fresh ERSATZRS_APP_STATE_DIR and contained ERSATZRS_APP_DATA_DIR for real installation"]
async fn explicit_addon_template_revision_update_preserves_history_and_requires_fresh_publication()
{
    let mut fixture = Box::pin(package::first()).await;
    let day = fixture.profile("Versioned day", "day", "good").await;
    let night = fixture.profile("Versioned night", "night", "good").await;
    let day_binding = fixture.bind(1, &day, None).await;
    fixture.bind(2, &night, None).await;
    let first_package = package::installed(&fixture).await;
    let first_files = package::files(&first_package.install_path).unwrap();
    let first = Box::pin(history::capture(&fixture, 1, 1, "day")).await;
    let old_night = Box::pin(history::capture(&fixture, 2, 1, "night")).await;
    let request = generations::request(&fixture, &first.design, None).await;
    let accepted = generations::accepted(generations::accept(&fixture, &request, 1, None).await);
    let pointers = history::pointers(&fixture).await;
    let settings = history::settings(&fixture).await;
    let before_update_calls = package::calls(&fixture).await;
    Box::pin(package::update(&fixture, 2)).await;
    let second_package = package::installed(&fixture).await;
    assert_eq!(second_package.version, semver::Version::new(2, 0, 0));
    assert_ne!(first_package.install_path, second_package.install_path);
    assert_ne!(first_package.package_sha256, second_package.package_sha256);
    assert_ne!(first_package.content_sha256, second_package.content_sha256);
    assert_ne!(first_package.state_revision, second_package.state_revision);
    if let Some(files) = package::files(&first_package.install_path) {
        assert_eq!(files, first_files);
    }
    assert_eq!(history::settings(&fixture).await, settings);
    assert_eq!(history::pointers(&fixture).await, pointers);
    assert_eq!(package::calls(&fixture).await, before_update_calls);
    Box::pin(history::stale(&fixture, &day, &night, &day_binding)).await;
    Box::pin(stale_publication(
        &fixture,
        &first.design,
        Some(request.id),
        TeletextPublicationRejection::SelectionUnavailable(TeletextProfileRejection::Conflict),
    ))
    .await;
    assert_eq!(history::pointers(&fixture).await, pointers);
    Box::pin(history::verify(&fixture, &first)).await;
    Box::pin(history::verify(&fixture, &old_night)).await;
    let changed = Box::pin(revalidate(&fixture, &day)).await;
    assert_eq!(changed.settings, day.settings);
    assert_eq!(changed.revision, day.revision + 1);
    assert_ne!(changed.validated_installation, day.validated_installation);
    let current_binding = fixture
        .state
        .application
        .channel_teletext_bindings(1)
        .await
        .unwrap()[0]
        .binding
        .revision;
    let _ = fixture.bind(1, &changed, Some(current_binding)).await;
    assert_eq!(history::pointers(&fixture).await, pointers);
    let second = Box::pin(history::capture(&fixture, 1, 2, "day")).await;
    assert_eq!(
        second.design.candidate().selection().bindings[0].installation,
        changed.validated_installation
    );
    assert_ne!(
        first.design.candidate().selection(),
        second.design.candidate().selection()
    );
    Box::pin(stale_publication(
        &fixture,
        &first.design,
        Some(request.id),
        TeletextPublicationRejection::SelectionChanged,
    ))
    .await;
    assert_eq!(history::pointers(&fixture).await, pointers);
    let next_request = generations::request(&fixture, &second.design, Some(request.id)).await;
    assert_eq!(
        fixture
            .state
            .application
            .accepted_teletext_generation(1)
            .await
            .unwrap(),
        Some(accepted.generation().clone())
    );
    let next = generations::accepted(
        generations::accept(&fixture, &next_request, 1, Some(accepted.generation().id)).await,
    );
    let calls = package::calls(&fixture).await;
    fixture = Box::pin(history::reopen(
        fixture,
        [&first, &old_night, &second],
        &next,
        &next_request,
        &settings,
        &calls,
    ))
    .await;
    fixture.state.pool.close().await;
}
