//! Role-aware controller/form flow using actual Lua and installed producers.
//! This proves checked originals and metadata selection, not stream/client delivery.

use super::{
    authoring::{self, AuthoringFixture},
    channel_documents, designs, scripted_controls, *,
};
use chrono::Timelike as _;
use ersatzrs_domain::TeletextOverrideState;
use ersatzrs_ffmpeg::teletext::TeletextRaster;
use ersatzrs_web::host::teletext_authoring::{
    observe_effective_teletext_authoring_selection, prepare_static_teletext_design,
    preview_original_teletext_design, publish_static_teletext_generation,
    request_static_teletext_publication, TeletextAuthoringEffectiveObservation,
    TeletextAuthoringIdentity, TeletextAuthoringPrepare, TeletextAuthoringPreviewRequest,
    TeletextAuthoringPublicationObservation, TeletextAuthoringPublicationState,
    TeletextAuthoringPublicationTarget, TeletextAuthoringPublishTarget, TeletextAuthoringScope,
    TeletextAuthoringStatus,
};

pub(super) async fn lua(
    fixture: &AuthoringFixture,
    channel: i64,
    profile: Option<&TeletextProfile>,
) -> ersatzrs_domain::TeletextOverrideInterval {
    let command = profile.map_or_else(
        || format!("assert(teletext_enabled('{ADDON}',false).ok)"),
        |profile| format!("assert(teletext_profile('{ADDON}',{}).ok)\nassert(teletext_enabled('{ADDON}',true).ok)",profile.id),
    );
    let result = scripted_controls::build_at(
        &fixture.fixture,
        &format!("{command}\nadd_count('clip',1)"),
        channel,
        true,
        (Utc::now() - chrono::Duration::seconds(1))
            .with_nanosecond(0)
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(result.teletext_intervals.len(), 1);
    assert_eq!(
        result.teletext_intervals[0].finish,
        result.added_items[0].finish
    );
    assert_eq!(
        matches!(
            result.teletext_intervals[0].state,
            TeletextOverrideState::Enabled(_)
        ),
        profile.is_some()
    );
    scripted_controls::publish_result(&fixture.fixture, &result).await;
    result.teletext_intervals[0].clone()
}

async fn observe(
    fixture: &AuthoringFixture,
    channel: i64,
) -> TeletextAuthoringEffectiveObservation {
    observe_effective_teletext_authoring_selection(
        &fixture.fixture.state.application,
        channel,
        designs::deadline(),
    )
    .await
    .unwrap()
}

fn request(
    channel: i64,
    observed: TeletextAuthoringEffectiveObservation,
) -> TeletextAuthoringPrepare {
    let mut request = AuthoringFixture::request(channel, observed.selection_digest);
    request.scope = TeletextAuthoringScope::Effective;
    if observed.is_absent {
        request.input_json.clear();
    }
    request
}

pub(super) async fn prepare(
    fixture: &AuthoringFixture,
    channel: i64,
) -> (
    TeletextAuthoringIdentity,
    Option<TeletextRaster>,
    TeletextAuthoringPublicationObservation,
) {
    let saved = prepare_static_teletext_design(
        &fixture.fixture.state.application,
        request(channel, observe(fixture, channel).await),
        designs::deadline(),
    )
    .await
    .unwrap();
    let preview = saved.preview.unwrap();
    assert_eq!(preview.scope(), TeletextAuthoringScope::Effective);
    assert!(preview.valid_until().is_some());
    let raster = preview.raster().cloned();
    assert_eq!(preview.is_absent(), raster.is_none());
    let publication = preview.publication().unwrap().clone();
    drop(preview); // The next cap-one stage owns its own actual-work lease.
    (saved.identity, raster, publication)
}

pub(super) async fn preview(
    fixture: &AuthoringFixture,
    identity: TeletextAuthoringIdentity,
    absent: bool,
) -> ersatzrs_web::host::teletext_authoring::TeletextAuthoringPreview {
    preview_original_teletext_design(
        &fixture.fixture.state.application,
        TeletextAuthoringPreviewRequest {
            channel_id: identity.channel_id,
            design_id: identity.design_id,
            revision: identity.revision,
            owner: (!absent).then(|| channel_documents::address(123, 16)),
        },
        designs::deadline(),
    )
    .await
    .unwrap()
}

pub(super) fn target(
    identity: TeletextAuthoringIdentity,
    observed: &TeletextAuthoringPublicationObservation,
) -> TeletextAuthoringPublicationTarget {
    TeletextAuthoringPublicationTarget {
        identity,
        digest: observed.digest.clone(),
        expected_design_head: observed.current_design_head,
        expected_request_id: observed.current_pending_id,
    }
}

pub(super) async fn select(
    fixture: &AuthoringFixture,
    identity: TeletextAuthoringIdentity,
    observed: &TeletextAuthoringPublicationObservation,
    absent: bool,
) -> i64 {
    let pending = request_static_teletext_publication(
        &fixture.fixture.state.application,
        target(identity, observed),
        designs::deadline(),
    )
    .await
    .unwrap();
    assert!(fixture
        .fixture
        .state
        .application
        .effective_teletext_accepted_generation(identity.channel_id)
        .await
        .unwrap()
        .is_none());
    let rendered = preview(fixture, identity, absent).await;
    let observed = rendered.publication().unwrap().clone();
    assert_eq!(observed.state, TeletextAuthoringPublicationState::Pending);
    assert_eq!(observed.current_pending_id, Some(pending.request_id));
    drop(rendered);
    publish_static_teletext_generation(
        &fixture.fixture.state.application,
        TeletextAuthoringPublishTarget {
            publication: target(identity, &observed),
            expected_generation_id: observed.current_accepted_id,
        },
        designs::deadline(),
    )
    .await
    .unwrap()
    .generation_id
}

async fn history(fixture: &AuthoringFixture) -> Vec<Vec<String>> {
    let mut result = Vec::new();
    for query in [
        r#"SELECT json_array("ChannelId","RequestId") FROM "TeletextPublicationRequestCurrent" ORDER BY "ChannelId""#,
        r#"SELECT json_array("ChannelId","GenerationId") FROM "TeletextAcceptedGenerationCurrent" ORDER BY "ChannelId""#,
        r#"SELECT json_array("ChannelId","RequestId") FROM "TeletextEffectivePublicationRequestCurrent" ORDER BY "ChannelId""#,
        r#"SELECT json_array("ChannelId","GenerationId") FROM "TeletextEffectiveAcceptedGenerationCurrent" ORDER BY "ChannelId""#,
        r#"SELECT json_array("Id","ChannelId","DesignId","Revision","Digest") FROM "TeletextPublicationRequest" ORDER BY "Id""#,
        r#"SELECT json_array("Id","ChannelId","RequestId","ArtifactVersion","ArtifactDigest") FROM "TeletextAcceptedGeneration" ORDER BY "Id""#,
        r#"SELECT json_array("DesignId","Revision","DocumentVersion","Digest") FROM "TeletextDesignRevision" ORDER BY "DesignId","Revision""#,
        r#"SELECT json_array("Id","FFmpegProfileId","StreamingMode","TranscodeMode","SubtitleMode") FROM "Channel" ORDER BY "Id""#,
    ] {
        result.push(
            sqlx::query_scalar(query)
                .fetch_all(&fixture.fixture.state.pool)
                .await
                .unwrap(),
        );
    }
    result
}

pub(super) async fn base_selected(fixture: &AuthoringFixture) -> (i64, TeletextRaster) {
    let saved = prepare_static_teletext_design(
        &fixture.fixture.state.application,
        AuthoringFixture::request(1, fixture.observe(1).await),
        designs::deadline(),
    )
    .await
    .unwrap();
    let rendered = saved.preview.unwrap();
    assert_eq!(rendered.scope(), TeletextAuthoringScope::Base);
    let raster = rendered.raster().unwrap().clone();
    let observed = rendered.publication().unwrap().clone();
    drop(rendered);
    let pending = request_static_teletext_publication(
        &fixture.fixture.state.application,
        target(saved.identity, &observed),
        designs::deadline(),
    )
    .await
    .unwrap();
    let mut target = target(saved.identity, &observed);
    target.expected_request_id = Some(pending.request_id);
    let accepted = publish_static_teletext_generation(
        &fixture.fixture.state.application,
        TeletextAuthoringPublishTarget {
            publication: target,
            expected_generation_id: None,
        },
        designs::deadline(),
    )
    .await
    .unwrap();
    (accepted.generation_id, raster)
}

pub(super) async fn disable_base(fixture: &AuthoringFixture, channel: i64) {
    assert!(
        !saved(
            fixture
                .fixture
                .state
                .application
                .disable_channel_teletext_binding(channel, ADDON, 1)
                .await
                .unwrap()
        )
        .enabled
    );
}

#[tokio::test]
async fn actual_lua_over_saved_off_base_prepares_two_themes_and_explicitly_selects_stored_role_two()
{
    let fixture = AuthoringFixture::new().await;
    let (base_id, base_pixels) = base_selected(&fixture).await;
    let baseline = history(&fixture).await;
    for channel in [1, 2] {
        disable_base(&fixture, channel).await;
    }
    scripted_controls::seed_publication_inputs(&fixture.fixture).await;
    lua(&fixture, 1, Some(&fixture.profiles[1])).await;
    lua(&fixture, 2, Some(&fixture.profiles[0])).await;
    let (first, night, observed) = prepare(&fixture, 1).await;
    let (second, day, second_observed) = prepare(&fixture, 2).await;
    assert_ne!(night, day);
    assert_eq!(day.as_ref(), Some(&base_pixels));
    let before_calls = fixture.calls();
    let original = preview(&fixture, first, false).await;
    assert_eq!(original.raster(), night.as_ref());
    assert_eq!(original.pages(), [channel_documents::address(123, 16)]);
    assert!(original.raster().unwrap().diagnostics().is_empty());
    drop(original);
    let generation = select(&fixture, first, &observed, false).await;
    select(&fixture, second, &second_observed, false).await;
    let selected = fixture
        .fixture
        .state
        .application
        .effective_teletext_accepted_generation(1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(selected.id, generation);
    assert_eq!(selected.artifact_version, 4);
    assert_eq!(
        fixture
            .fixture
            .state
            .application
            .accepted_teletext_generation(1)
            .await
            .unwrap()
            .unwrap()
            .id,
        base_id
    );
    assert_eq!(fixture.calls(), before_calls);
    let after = history(&fixture).await;
    assert_eq!(&after[..2], &baseline[..2]);
    assert_eq!(after[7], baseline[7]);
    let original = preview(&fixture, first, false).await;
    assert_eq!(
        original.publication().unwrap().state,
        TeletextAuthoringPublicationState::Selected
    );
    drop(original);
}

#[tokio::test]
async fn all_off_has_no_owner_raster_or_fake_entry_and_explicit_acceptance_keeps_base_pointer() {
    let fixture = AuthoringFixture::new().await;
    let (base_id, _) = base_selected(&fixture).await;
    scripted_controls::seed_publication_inputs(&fixture.fixture).await;
    lua(&fixture, 1, None).await;
    sqlx::query(r#"UPDATE "AddonInstallation" SET "Enabled"=0 WHERE "AddonId"=?"#)
        .bind(ADDON)
        .execute(&fixture.fixture.state.pool)
        .await
        .unwrap();
    let calls = fixture.calls();
    let observation = observe(&fixture, 1).await;
    assert!(observation.is_absent && observation.producer_addon_ids.is_empty());
    let (identity, raster, observed) = prepare(&fixture, 1).await;
    assert!(raster.is_none());
    let original = preview(&fixture, identity, true).await;
    assert!(original.is_absent() && original.raster().is_none());
    assert!(original.pages().is_empty() && original.navigation().is_empty());
    assert!(original.publication().unwrap().can_request);
    drop(original);
    select(&fixture, identity, &observed, true).await;
    assert_eq!(fixture.calls(), calls);
    assert_eq!(
        fixture
            .fixture
            .state
            .application
            .accepted_teletext_generation(1)
            .await
            .unwrap()
            .unwrap()
            .id,
        base_id
    );
    let original = preview(&fixture, identity, true).await;
    assert_eq!(
        original.publication().unwrap().state,
        TeletextAuthoringPublicationState::Selected
    );
    drop(original);
    let before = history(&fixture).await;
    let bad = preview_original_teletext_design(
        &fixture.fixture.state.application,
        authoring::preview_request(identity),
        designs::deadline(),
    )
    .await;
    assert!(bad.is_err()); // An absent document cannot borrow an authored page owner.
    assert_eq!(history(&fixture).await, before);
}

#[tokio::test]
async fn rendered_effective_digest_and_wrong_role_are_rejected_before_any_producer_or_design() {
    let fixture = AuthoringFixture::new().await;
    let base_digest = fixture.observe(1).await;
    disable_base(&fixture, 1).await;
    scripted_controls::seed_publication_inputs(&fixture.fixture).await;
    lua(&fixture, 1, Some(&fixture.profiles[1])).await;
    let rendered = observe(&fixture, 1).await;
    let mut wrong_role = AuthoringFixture::request(1, base_digest);
    wrong_role.scope = TeletextAuthoringScope::Effective;
    let before = history(&fixture).await;
    assert_eq!(
        prepare_static_teletext_design(
            &fixture.fixture.state.application,
            wrong_role,
            designs::deadline()
        )
        .await
        .unwrap_err()
        .status,
        TeletextAuthoringStatus::SelectionChanged
    );
    sqlx::query(r#"UPDATE "PlayoutTeletextInterval" SET "Finish"=?"#)
        .bind(
            (Utc::now() + chrono::Duration::minutes(2))
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
        )
        .execute(&fixture.fixture.state.pool)
        .await
        .unwrap();
    assert_eq!(
        prepare_static_teletext_design(
            &fixture.fixture.state.application,
            request(1, rendered),
            designs::deadline()
        )
        .await
        .unwrap_err()
        .status,
        TeletextAuthoringStatus::SelectionChanged
    );
    assert!(fixture.calls().is_empty());
    assert_eq!(history(&fixture).await, before);
}

#[tokio::test]
async fn historical_expired_original_keeps_pixels_without_current_publication_or_package() {
    let fixture = AuthoringFixture::new().await;
    scripted_controls::seed_publication_inputs(&fixture.fixture).await;
    lua(&fixture, 1, Some(&fixture.profiles[1])).await;
    // Test-only retained-prefix clipping gives a bounded actual-clock expiry proof.
    let finish = (Utc::now() + chrono::Duration::seconds(4))
        .with_nanosecond(0)
        .unwrap();
    sqlx::query(r#"UPDATE "PlayoutTeletextInterval" SET "Finish"=?"#)
        .bind(finish.format("%Y-%m-%d %H:%M:%S").to_string())
        .execute(&fixture.fixture.state.pool)
        .await
        .unwrap();
    let (identity, raster, observed) = prepare(&fixture, 1).await;
    tokio::time::sleep(
        (finish - Utc::now()).to_std().unwrap_or_default() + Duration::from_millis(20),
    )
    .await;
    sqlx::query(r#"DELETE FROM "AddonInstallation" WHERE "AddonId"=?"#)
        .bind(ADDON)
        .execute(&fixture.fixture.state.pool)
        .await
        .unwrap();
    let before = history(&fixture).await;
    let calls = fixture.calls();
    let original = preview(&fixture, identity, false).await;
    assert_eq!(original.raster(), raster.as_ref());
    assert_eq!(
        original.publication().unwrap().state,
        TeletextAuthoringPublicationState::Unavailable
    );
    assert!(
        !original.publication().unwrap().can_request
            && !original.publication().unwrap().can_publish
    );
    drop(original);
    assert!(request_static_teletext_publication(
        &fixture.fixture.state.application,
        target(identity, &observed),
        designs::deadline()
    )
    .await
    .is_err());
    assert_eq!(history(&fixture).await, before);
    assert_eq!(fixture.calls(), calls);
}
