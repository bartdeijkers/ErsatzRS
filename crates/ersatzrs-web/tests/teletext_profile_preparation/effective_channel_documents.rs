//! Real Lua publication to unpublished effective native candidates.
//! Generation promotion and live activation remain separate acceptance criteria.

use super::*;
use super::{channel_documents, documents, scripted_controls};
use ersatzrs_domain::{
    TeletextColourPixelOptions, TeletextEffectiveBindingSnapshot as Binding, TeletextFlashPhase,
    TeletextOverrideState, TeletextPageContract,
};
use ersatzrs_ffmpeg::teletext::{
    render_teletext_composition, EncodedTeletextAuthoredPage, TeletextRasterBudget,
};
use ersatzrs_web::host::{
    teletext_channel_documents::ChannelTeletextDocumentEncodingError,
    teletext_effective_channel_documents::{
        prepare_effective_channel_teletext_document_candidate,
        EncodedEffectiveChannelTeletextDocumentCandidate,
    },
    teletext_preview::{project_teletext_preview, TeletextPreviewBudget},
};

async fn publish(
    fixture: &Fixture,
    channel: i64,
    target: Option<&TeletextProfile>,
) -> ersatzrs_domain::TeletextOverrideInterval {
    let command = target.map_or_else(
        || format!("assert(teletext_enabled('{ADDON}', false).ok)"),
        |profile| format!("assert(teletext_profile('{ADDON}', {}).ok)\nassert(teletext_enabled('{ADDON}', true).ok)", profile.id),
    );
    let start = ersatzrs_core::scheduling::truncate_to_persisted_precision(
        Utc::now() - chrono::Duration::seconds(1),
    );
    let result = scripted_controls::build_at(
        fixture,
        &format!("{command}\nadd_count('clip', 1)"),
        channel,
        true,
        start,
    )
    .await
    .unwrap();
    assert_eq!(result.teletext_intervals.len(), 1);
    let interval = result.teletext_intervals[0].clone();
    assert_eq!(interval.start, result.added_items[0].start);
    assert_eq!(interval.finish, result.added_items[0].finish);
    assert_eq!(
        interval.finish - interval.start,
        chrono::Duration::minutes(1)
    );
    scripted_controls::publish_result(fixture, &result).await;
    interval
}

async fn candidate(
    fixture: &Fixture,
    channel: i64,
    entry: Option<&str>,
) -> Result<EncodedEffectiveChannelTeletextDocumentCandidate, ChannelTeletextDocumentEncodingError>
{
    prepare_effective_channel_teletext_document_candidate(
        &fixture.state.application,
        channel,
        entry,
        &guide(&[]),
        channel_documents::policy(),
    )
    .await
}

async fn installed(
    persistent: bool,
) -> (Fixture, TeletextProfile, TeletextProfile, TeletextProfile) {
    let fixture = if persistent {
        Fixture::new_persistent().await
    } else {
        Fixture::new().await
    };
    documents::configure(&fixture, documents::DocumentFixture::Authored).await;
    let base = fixture.profile("Operator base", "day", "good").await;
    let day = fixture.profile("Explicit day", "day", "good").await;
    let night = fixture.profile("Explicit night", "night", "good").await;
    fixture.bind(1, &base, None).await;
    fixture.bind(2, &base, None).await;
    scripted_controls::seed_publication_inputs(&fixture).await;
    (fixture, base, day, night)
}

fn raster(
    candidate: &EncodedEffectiveChannelTeletextDocumentCandidate,
) -> Vec<ersatzrs_ffmpeg::teletext::TeletextRasterPixel> {
    let encoded = candidate.encoded().unwrap();
    let owner = channel_documents::address(123, 16);
    let options = TeletextColourPixelOptions {
        reveal_concealed: false,
        flash_phase: TeletextFlashPhase::Original,
    };
    let view = project_teletext_preview(
        encoded,
        owner,
        options,
        TeletextPreviewBudget {
            max_pages: 16,
            max_model_records: 32_768,
            max_pixels: 168_000,
            max_diagnostics: 1400,
            max_work: 50_000_000,
        },
    )
    .unwrap();
    let EncodedTeletextAuthoredPage::CompiledDisplay(page) = &encoded.pages()[0] else {
        panic!("actual Local enhancement must use the shared complete compiler")
    };
    let canonical = render_teletext_composition(
        page.normalized().original(),
        options,
        TeletextRasterBudget {
            max_pixels: 168_000,
            max_diagnostics: 1400,
        },
    )
    .unwrap();
    assert_eq!(view.raster(), &canonical);
    assert!(view.raster().diagnostics().is_empty());
    assert_eq!(view.raster().width(), 56 * 12);
    view.raster().pixels().to_vec()
}

async fn assert_unpublished(fixture: &Fixture) {
    for query in [
        r#"SELECT COUNT(*) FROM "TeletextPublicationRequest""#,
        r#"SELECT COUNT(*) FROM "TeletextAcceptedGeneration""#,
    ] {
        let count: i64 = sqlx::query_scalar(query)
            .fetch_one(&fixture.state.pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
}

async fn assert_origin(
    fixture: &Fixture,
    value: &EncodedEffectiveChannelTeletextDocumentCandidate,
    channel: i64,
    profile: &TeletextProfile,
    interval: &ersatzrs_domain::TeletextOverrideInterval,
) {
    assert_eq!(value.selection().channel_id, channel);
    assert_eq!(value.selection().valid_until, Some(interval.finish));
    assert_eq!(value.producer_bindings(), [0]);
    let Binding::Scripted(scope) = &value.selection().bindings[0] else {
        panic!("base substituted")
    };
    assert_eq!(scope.playout_id, 92 + channel);
    assert!(
        matches!(&scope.interval.state, TeletextOverrideState::Enabled(selected)
        if selected.profile_id == profile.id && selected.channel_id == channel)
    );
    assert_eq!(scope.interval.start, interval.start);
    assert_eq!(scope.interval.finish, interval.finish);
    assert_eq!(value.sources()[0].contract(), TeletextPageContract::V2);
    assert_eq!(
        value.sources()[0].bytes(),
        serde_json::to_vec(&documents::document(
            documents::DocumentFixture::Authored,
            if channel == 1 { "day" } else { "night" },
        ))
        .unwrap()
    );
    assert_eq!(
        saved(
            fixture
                .state
                .application
                .recheck_effective_channel_teletext_selection(value.selection(), Utc::now())
                .await
                .unwrap()
        ),
        *value.selection()
    );
    assert_eq!(
        fixture
            .state
            .application
            .recheck_effective_channel_teletext_selection(value.selection(), interval.finish)
            .await
            .unwrap(),
        TeletextProfileMutation::Rejected(TeletextProfileRejection::Conflict)
    );
}

#[tokio::test]
async fn actual_lua_two_channels_reopen_into_shared_native_compiler_and_distinct_rasters() {
    let (fixture, base, day, night) = installed(true).await;
    // Use the existing two-slot policy for concurrent preparation. The absent
    // setting deliberately admits only one owner; do not bypass that admission.
    fixture
        .state
        .application
        .set_admin_setting(ersatzrs_app::commands::SetAdminSetting {
            key: ersatzrs_web::host::teletext_work::MAX_CONCURRENT_SETTING,
            value: "2".to_owned(),
        })
        .await
        .unwrap();
    let first = publish(&fixture, 1, Some(&day)).await;
    let second = publish(&fixture, 2, Some(&night)).await;
    sqlx::query(r#"UPDATE "TeletextProfile" SET "IsDeleted" = 1 WHERE "Id" = ?"#)
        .bind(base.id)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    sqlx::query(
        r#"UPDATE "ChannelTeletextBinding" SET "Enabled" = 0, "Revision" = "Revision" + 1"#,
    )
    .execute(&fixture.state.pool)
    .await
    .unwrap();
    let operator: Vec<(i64, i64, bool, i64)> = sqlx::query_as(
        r#"SELECT "ChannelId","ProfileId","Enabled","Revision" FROM "ChannelTeletextBinding" ORDER BY "ChannelId""#)
        .fetch_all(&fixture.state.pool).await.unwrap();
    let (a, b) = tokio::join!(
        candidate(&fixture, 1, Some(ADDON)),
        candidate(&fixture, 2, Some(ADDON))
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    for (value, channel, profile, interval) in [(&a, 1, &day, &first), (&b, 2, &night, &second)] {
        assert_origin(&fixture, value, channel, profile, interval).await;
    }
    let day_pixels = raster(&a);
    let night_pixels = raster(&b);
    assert_ne!(day_pixels, night_pixels);
    // One producer preserves its magazine palette as M/29. Equal display page
    // bytes are correct; the full services and their shared preview differ.
    assert_eq!(
        a.encoded().unwrap().pages()[0].data_units(),
        b.encoded().unwrap().pages()[0].data_units(),
    );
    assert!(!a.encoded().unwrap().magazine_units().is_empty());
    assert_ne!(
        a.encoded().unwrap().magazine_units(),
        b.encoded().unwrap().magazine_units(),
    );
    assert_unpublished(&fixture).await;
    let fixture = fixture.reopen().await;
    let reopened = candidate(&fixture, 2, Some(ADDON)).await.unwrap();
    assert_eq!(reopened.selection(), b.selection());
    assert_eq!(raster(&reopened), night_pixels);
    let after: Vec<(i64, i64, bool, i64)> = sqlx::query_as(
        r#"SELECT "ChannelId","ProfileId","Enabled","Revision" FROM "ChannelTeletextBinding" ORDER BY "ChannelId""#)
        .fetch_all(&fixture.state.pool).await.unwrap();
    assert_eq!(operator, after);
    assert_unpublished(&fixture).await;
}

#[tokio::test]
async fn explicit_off_skips_unhealthy_package_but_retains_membership_and_unaffected_producer() {
    let (fixture, base, _, _) = installed(false).await;
    install_secondary(&fixture).await;
    channel_documents::install_contract(&fixture, SECONDARY, "second", 1).await;
    channel_documents::write_response(
        &fixture,
        "second",
        &channel_documents::response(documents::DocumentFixture::Base, "day", 223, 0, "fra"),
    );
    let other = fixture
        .profile_for(SECONDARY, "Unaffected", "day", "good")
        .await;
    fixture.bind(1, &other, None).await;
    let interval = publish(&fixture, 1, None).await;
    sqlx::query(r#"UPDATE "TeletextProfile" SET "IsDeleted"=1 WHERE "Id"=?"#)
        .bind(base.id)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "AddonInstallation" SET "Enabled"=0 WHERE "AddonId"=?"#)
        .bind(ADDON)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    let value = candidate(&fixture, 1, Some(SECONDARY)).await.unwrap();
    assert_eq!(value.selection().bindings.len(), 2);
    assert_eq!(value.producer_bindings(), [1]);
    assert!(!value.selection().bindings[0].enabled());
    assert!(matches!(
        value.selection().bindings[0],
        Binding::Scripted(_)
    ));
    assert!(
        matches!(&value.selection().bindings[1], Binding::Base(selection) if selection.profile_id == other.id)
    );
    assert_eq!(value.selection().valid_until, Some(interval.finish));
    assert_eq!(value.sources().len(), 1);
    assert_eq!(
        value.encoded().unwrap().declared().semantic().base().pages[0].address,
        channel_documents::address(223, 0)
    );
    for theme in ["day", "night"] {
        assert!(!fixture
            .root
            .path()
            .join(format!("document-calls-{theme}"))
            .exists());
    }
    assert_unpublished(&fixture).await;
}

#[tokio::test]
async fn explicit_off_only_returns_absent_and_skips_unused_encoding_policy() {
    let (fixture, _, _, _) = installed(false).await;
    publish(&fixture, 1, None).await;
    sqlx::query(r#"UPDATE "AddonInstallation" SET "Enabled"=0 WHERE "AddonId"=?"#)
        .bind(ADDON)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    let mut policy = channel_documents::policy();
    policy.input.max_producers = 0;
    policy.encoding.max_pages = 0;
    policy.composition.max_producers = 0;
    let value = prepare_effective_channel_teletext_document_candidate(
        &fixture.state.application,
        1,
        Some("unused"),
        &guide(&[]),
        policy,
    )
    .await
    .unwrap();
    assert_eq!(value.selection().bindings.len(), 1);
    assert!(value.producer_bindings().is_empty());
    assert!(value.sources().is_empty());
    assert!(value.encoded().is_none());
    assert!(value.descriptor().is_none());
    assert!(value.metadata().is_none());
    assert_unpublished(&fixture).await;
}

#[tokio::test]
async fn invalid_explicit_on_rejects_before_package_invocation_without_base_fallback() {
    for change in [
        r#"UPDATE "TeletextProfile" SET "Revision"="Revision"+1 WHERE "Name"='Explicit night'"#,
        r#"UPDATE "AddonInstallation" SET "Enabled"=0"#,
        r#"DELETE FROM "ChannelTeletextBinding" WHERE "ChannelId"=1"#,
    ] {
        let (fixture, _, _, night) = installed(false).await;
        publish(&fixture, 1, Some(&night)).await;
        sqlx::query(change)
            .execute(&fixture.state.pool)
            .await
            .unwrap();
        assert!(candidate(&fixture, 1, Some(ADDON)).await.is_err());
        for theme in ["day", "night"] {
            assert!(!fixture
                .root
                .path()
                .join(format!("document-calls-{theme}"))
                .exists());
        }
        assert_unpublished(&fixture).await;
    }
}

#[tokio::test]
async fn actual_selected_package_response_failure_never_returns_operator_base_content() {
    let (fixture, _, _, night) = installed(false).await;
    publish(&fixture, 1, Some(&night)).await;
    let mut response = documents::document(documents::DocumentFixture::Authored, "night");
    response["generation"] = serde_json::json!("wrong-correlation");
    std::fs::write(
        fixture.root.path().join("document-response-night.json"),
        serde_json::to_vec(&response).unwrap(),
    )
    .unwrap();
    assert!(candidate(&fixture, 1, Some(ADDON)).await.is_err());
    assert_eq!(
        std::fs::read(fixture.root.path().join("document-calls-night")).unwrap(),
        b"teletext.pages.v2\n"
    );
    assert!(!fixture.root.path().join("document-calls-day").exists());
    assert_unpublished(&fixture).await;
}
