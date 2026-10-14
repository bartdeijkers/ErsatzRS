//! Actual Lua/native originals survive historical reload without live authority.

use super::*;
use super::{
    authoring_metadata, channel_documents, designs, documents,
    effective_channel_documents as effective, scripted_controls,
};
use ersatzrs_app::teletext_design_documents::{
    TeletextDesignDocumentBudget, TeletextSavedEditPolicy,
};
use ersatzrs_core::repositories::{TeletextDesignMutation, TeletextDesignRejection};
use ersatzrs_domain::{
    TeletextColourPixelOptions, TeletextEffectiveBindingSnapshot as Binding, TeletextFlashPhase,
    TeletextOverrideState, TeletextPageContract,
};
use ersatzrs_ffmpeg::teletext::{EncodedTeletextAuthoredService, TeletextRasterPixel};
use ersatzrs_web::host::{
    teletext_designs::{
        reload_original_teletext_design_revision, request_original_teletext_design_publication,
        NativeTeletextDesignError,
    },
    teletext_effective_channel_documents::EncodedEffectiveChannelTeletextDocumentCandidate,
    teletext_effective_designs::{
        reload_effective_original_teletext_design_revision,
        save_effective_original_teletext_design_candidate, RestoredEffectiveOriginalDesign,
        SavedEffectiveOriginalDesign,
    },
    teletext_preview::{project_teletext_preview, TeletextPreviewBudget},
};

pub(super) async fn installed(
    v3: bool,
) -> (Fixture, TeletextProfile, TeletextProfile, TeletextProfile) {
    let fixture = Fixture::new_persistent().await;
    if v3 {
        authoring_metadata::install(&fixture).await;
        for theme in ["day", "night"] {
            authoring_metadata::write(
                &fixture,
                theme,
                &authoring_metadata::response(theme, 123, 16),
            );
        }
    } else {
        documents::configure(&fixture, documents::DocumentFixture::Authored).await;
        sqlx::query(r#"UPDATE "AddonInstallation" SET "PackageSha256"=? WHERE "AddonId"=?"#)
            .bind("a".repeat(64))
            .bind(ADDON)
            .execute(&fixture.state.pool)
            .await
            .unwrap();
    }
    let base = fixture.profile("Operator base", "day", "good").await;
    let day = fixture.profile("Explicit day", "day", "good").await;
    let night = fixture.profile("Explicit night", "night", "good").await;
    fixture.bind(1, &base, None).await;
    fixture.bind(2, &base, None).await;
    scripted_controls::seed_publication_inputs(&fixture).await;
    (fixture, base, day, night)
}

pub(super) async fn save(
    fixture: &Fixture,
    candidate: EncodedEffectiveChannelTeletextDocumentCandidate,
    existing: Option<(i64, i64)>,
    budget: TeletextDesignDocumentBudget,
) -> Result<TeletextDesignMutation<SavedEffectiveOriginalDesign>, NativeTeletextDesignError> {
    save_effective_original_teletext_design_candidate(
        &fixture.state.application,
        "Synthetic effective original".to_owned(),
        existing,
        candidate,
        budget,
        designs::deadline(),
    )
    .await
}
pub(super) fn saved_design(
    value: TeletextDesignMutation<SavedEffectiveOriginalDesign>,
) -> SavedEffectiveOriginalDesign {
    match value {
        TeletextDesignMutation::Saved(value) => value,
        TeletextDesignMutation::Rejected(reason) => panic!("unexpected rejection {reason:?}"),
    }
}
pub(super) async fn reload(
    fixture: &Fixture,
    id: i64,
    revision: i64,
) -> RestoredEffectiveOriginalDesign {
    reload_effective_original_teletext_design_revision(
        &fixture.state.application,
        id,
        revision,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap()
}
pub(super) fn pixels(
    encoded: &EncodedTeletextAuthoredService,
    page: u16,
    subcode: u16,
) -> Vec<TeletextRasterPixel> {
    project_teletext_preview(
        encoded,
        channel_documents::address(page, subcode),
        TeletextColourPixelOptions {
            reveal_concealed: false,
            flash_phase: TeletextFlashPhase::Original,
        },
        TeletextPreviewBudget {
            max_pages: 16,
            max_model_records: 32768,
            max_pixels: 168_000,
            max_diagnostics: 1400,
            max_work: 50_000_000,
        },
    )
    .unwrap()
    .raster()
    .pixels()
    .to_vec()
}
async fn revision_count(fixture: &Fixture) -> i64 {
    sqlx::query_scalar(r#"SELECT COUNT(*) FROM "TeletextDesignRevision""#)
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap()
}
async fn assert_no_native_publication(fixture: &Fixture) {
    let requests: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "TeletextPublicationRequest""#)
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
    let accepted: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "TeletextAcceptedGeneration""#)
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
    assert_eq!((requests, accepted), (0, 0));
}
fn assert_original(
    restored: &RestoredEffectiveOriginalDesign,
    saved: &SavedEffectiveOriginalDesign,
) {
    let candidate = saved.candidate();
    assert_eq!(restored.selection(), candidate.selection());
    assert_eq!(restored.producer_bindings(), candidate.producer_bindings());
    assert_eq!(restored.generation(), candidate.generation());
    assert_eq!(restored.entry_addon_id(), candidate.entry_addon_id());
    assert_eq!(restored.digest(), saved.digest());
    assert_eq!(
        restored.edit_policy(),
        TeletextSavedEditPolicy::OriginalOnly
    );
    assert_eq!(restored.transmissions(), candidate.transmissions());
    assert_eq!(restored.v1_region(), candidate.v1_region());
    assert_eq!(restored.metadata(), candidate.metadata());
    assert_eq!(restored.guide_source(), None);
    for contract in [
        TeletextPageContract::V1,
        TeletextPageContract::V2,
        TeletextPageContract::V3,
    ] {
        assert_eq!(
            restored.guide_inputs().unwrap().request(contract),
            candidate.requests().request(contract)
        );
    }
    for (source, original) in restored.sources().iter().zip(candidate.sources()) {
        assert_eq!(source.bytes(), original.bytes());
        assert_eq!(source.authoring(), original.authoring());
        assert_eq!(source.contract(), original.contract());
    }
    assert_eq!(
        designs::units(restored.encoded().unwrap()),
        designs::units(candidate.encoded().unwrap())
    );
    assert_eq!(
        restored.encoded().unwrap().magazine_units(),
        candidate.encoded().unwrap().magazine_units()
    );
}

#[tokio::test]
async fn actual_lua_v3_effective_originals_save_and_reopen_without_invocation_or_profile_health() {
    let (mut fixture, _, day, night) = installed(true).await;
    effective::publish(&fixture, 1, Some(&day)).await;
    effective::publish(&fixture, 2, Some(&night)).await;
    let before = designs::config(&fixture).await;
    let first = saved_design(
        save(
            &fixture,
            effective::candidate(&fixture, 1, Some(ADDON))
                .await
                .unwrap(),
            None,
            designs::budget(),
        )
        .await
        .unwrap(),
    );
    let second = saved_design(
        save(
            &fixture,
            effective::candidate(&fixture, 2, Some(ADDON))
                .await
                .unwrap(),
            None,
            designs::budget(),
        )
        .await
        .unwrap(),
    );
    for (value, profile) in [(&first, &day), (&second, &night)] {
        let Binding::Scripted(published) = &value.candidate().selection().bindings[0] else {
            panic!("base substituted")
        };
        assert!(
            matches!(&published.interval.state, TeletextOverrideState::Enabled(candidate) if candidate.profile_id == profile.id)
        );
    }
    let day_pixels = pixels(first.candidate().encoded().unwrap(), 123, 16);
    let night_pixels = pixels(second.candidate().encoded().unwrap(), 123, 16);
    assert_ne!(day_pixels, night_pixels);
    assert_eq!(
        first.candidate().sources()[0].authoring().unwrap().theme.id,
        "day"
    );
    assert_eq!(
        second.candidate().sources()[0]
            .authoring()
            .unwrap()
            .theme
            .id,
        "night"
    );
    let calls = std::fs::read(fixture.root.path().join("authoring-calls")).unwrap();
    for value in [&first, &second] {
        let restored = reload(&fixture, value.head().id, 1).await;
        assert_original(&restored, value);
        assert!(matches!(
            restored.selection().bindings[0],
            Binding::Scripted(_)
        ));
    }
    sqlx::query(r#"DELETE FROM "ChannelTeletextBinding""#)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "TeletextProfile" SET "IsDeleted"=1"#)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    sqlx::query(r#"DELETE FROM "AddonInstallation""#)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    fixture = fixture.reopen().await;
    for (value, expected) in [(&first, &day_pixels), (&second, &night_pixels)] {
        let restored = reload(&fixture, value.head().id, 1).await;
        assert_original(&restored, value);
        assert_eq!(&pixels(restored.encoded().unwrap(), 123, 16), expected);
    }
    assert_eq!(
        std::fs::read(fixture.root.path().join("authoring-calls")).unwrap(),
        calls
    );
    assert_eq!(designs::config(&fixture).await, before);
    assert_no_native_publication(&fixture).await;
    assert!(fixture.playout.try_recv().is_err());
}

#[tokio::test]
async fn effective_off_member_and_unaffected_base_preserve_exact_producer_map_and_v1_region() {
    let (mut fixture, base, _, _) = installed(false).await;
    install_secondary(&fixture).await;
    sqlx::query(r#"UPDATE "AddonInstallation" SET "PackageSha256"=? WHERE "AddonId"=?"#)
        .bind("b".repeat(64))
        .bind(SECONDARY)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
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
    effective::publish(&fixture, 1, None).await;
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
    let value = saved_design(
        save(
            &fixture,
            effective::candidate(&fixture, 1, Some(SECONDARY))
                .await
                .unwrap(),
            None,
            designs::budget(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(value.candidate().producer_bindings(), [1]);
    let calls = std::fs::read(fixture.root.path().join("channel-calls")).unwrap();
    let restored = reload(&fixture, value.head().id, 1).await;
    assert_original(&restored, &value);
    assert_eq!(restored.selection().bindings.len(), 2);
    assert!(!restored.selection().bindings[0].enabled());
    assert!(matches!(
        restored.selection().bindings[0],
        Binding::Scripted(_)
    ));
    assert_eq!(restored.sources().len(), 1);
    assert_eq!(
        std::fs::read(fixture.root.path().join("channel-calls")).unwrap(),
        calls
    );
    for theme in ["day", "night"] {
        assert!(!fixture
            .root
            .path()
            .join(format!("document-calls-{theme}"))
            .exists());
    }
    assert_no_native_publication(&fixture).await;
    assert!(fixture.playout.try_recv().is_err());
}

#[tokio::test]
async fn effective_original_expiry_remains_historical_and_base_publication_cannot_consume_v2() {
    let (mut fixture, _, day, _) = installed(true).await;
    let start = ersatzrs_core::scheduling::truncate_to_persisted_precision(
        Utc::now() - chrono::Duration::seconds(55),
    );
    let result=scripted_controls::build_at(&fixture,
        &format!("assert(teletext_profile('{ADDON}', {}).ok)\nassert(teletext_enabled('{ADDON}', true).ok)\nadd_count('clip',1)",day.id),1,true,start).await.unwrap();
    let finish = result.teletext_intervals[0].finish;
    scripted_controls::publish_result(&fixture, &result).await;
    let value = saved_design(
        save(
            &fixture,
            effective::candidate(&fixture, 1, Some(ADDON))
                .await
                .unwrap(),
            None,
            designs::budget(),
        )
        .await
        .unwrap(),
    );
    let expected = pixels(value.candidate().encoded().unwrap(), 123, 16);
    let expired_candidate = effective::candidate(&fixture, 1, Some(ADDON))
        .await
        .unwrap();
    let calls = std::fs::read(fixture.root.path().join("authoring-calls")).unwrap();
    let remaining = (finish - Utc::now()).to_std().unwrap_or_default();
    tokio::time::sleep(remaining + Duration::from_millis(1)).await;
    assert!(Utc::now() >= finish);
    // A fresh save allowance cannot renew this earlier invocation's authority.
    assert!(matches!(
        save(&fixture, expired_candidate, None, designs::budget()).await,
        Err(NativeTeletextDesignError::TimedOut)
    ));
    assert_eq!(revision_count(&fixture).await, 1);
    let restored = reload(&fixture, value.head().id, 1).await;
    assert_original(&restored, &value);
    assert_eq!(pixels(restored.encoded().unwrap(), 123, 16), expected);
    assert_eq!(
        fixture
            .state
            .application
            .recheck_effective_channel_teletext_selection(restored.selection(), Utc::now())
            .await
            .unwrap(),
        TeletextProfileMutation::Rejected(TeletextProfileRejection::Conflict)
    );
    assert!(reload_original_teletext_design_revision(
        &fixture.state.application,
        value.head().id,
        1,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline()
    )
    .await
    .is_err());
    assert!(request_original_teletext_design_publication(
        &fixture.state.application,
        value.head().id,
        1,
        value.digest().to_owned(),
        1,
        None,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline()
    )
    .await
    .is_err());
    assert_eq!(
        std::fs::read(fixture.root.path().join("authoring-calls")).unwrap(),
        calls
    );
    assert_no_native_publication(&fixture).await;
    assert!(fixture.playout.try_recv().is_err());
}

#[tokio::test]
async fn effective_original_save_bounds_stale_cas_and_reload_caps_preserve_history() {
    let (mut fixture, _, day, _) = installed(true).await;
    effective::publish(&fixture, 1, Some(&day)).await;
    let mut limited = designs::budget();
    limited.max_serialized_bytes = 1;
    assert!(matches!(
        save(
            &fixture,
            effective::candidate(&fixture, 1, Some(ADDON))
                .await
                .unwrap(),
            None,
            limited
        )
        .await,
        Err(NativeTeletextDesignError::Document(_))
    ));
    assert_eq!(revision_count(&fixture).await, 0);
    let first = saved_design(
        save(
            &fixture,
            effective::candidate(&fixture, 1, Some(ADDON))
                .await
                .unwrap(),
            None,
            designs::budget(),
        )
        .await
        .unwrap(),
    );
    let id = first.head().id;
    let second = saved_design(
        save(
            &fixture,
            effective::candidate(&fixture, 1, Some(ADDON))
                .await
                .unwrap(),
            Some((id, 1)),
            designs::budget(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(second.head().revision, 2);
    assert!(matches!(
        save(
            &fixture,
            effective::candidate(&fixture, 1, Some(ADDON))
                .await
                .unwrap(),
            Some((id, 1)),
            designs::budget()
        )
        .await
        .unwrap(),
        TeletextDesignMutation::Rejected(TeletextDesignRejection::Conflict)
    ));
    assert_eq!(revision_count(&fixture).await, 2);
    assert_original(&reload(&fixture, id, 1).await, &first);
    let mut policy = channel_documents::policy();
    policy.input.max_producers = 0;
    assert!(reload_effective_original_teletext_design_revision(
        &fixture.state.application,
        id,
        1,
        designs::budget(),
        policy,
        designs::deadline()
    )
    .await
    .is_err());
    assert!(reload_effective_original_teletext_design_revision(
        &fixture.state.application,
        id,
        99,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline()
    )
    .await
    .unwrap()
    .is_none());
    assert_eq!(revision_count(&fixture).await, 2);
    assert_no_native_publication(&fixture).await;
    assert!(fixture.playout.try_recv().is_err());
}

#[tokio::test]
async fn effective_original_reload_rejects_corruption_and_native_caps_without_invocation() {
    let (mut fixture, _, day, _) = installed(true).await;
    effective::publish(&fixture, 1, Some(&day)).await;
    let value = saved_design(
        save(
            &fixture,
            effective::candidate(&fixture, 1, Some(ADDON))
                .await
                .unwrap(),
            None,
            designs::budget(),
        )
        .await
        .unwrap(),
    );
    let id = value.head().id;
    let calls = std::fs::read(fixture.root.path().join("authoring-calls")).unwrap();
    let mut limited = designs::budget();
    limited.max_serialized_bytes = 1;
    assert!(matches!(
        reload_effective_original_teletext_design_revision(
            &fixture.state.application,
            id,
            1,
            limited,
            channel_documents::policy(),
            designs::deadline()
        )
        .await,
        Err(NativeTeletextDesignError::Storage(_))
    ));
    let mut policy = channel_documents::policy();
    policy.encoding.max_units = 0;
    assert!(matches!(
        reload_effective_original_teletext_design_revision(
            &fixture.state.application,
            id,
            1,
            designs::budget(),
            policy,
            designs::deadline()
        )
        .await,
        Err(NativeTeletextDesignError::Encoding(_))
    ));
    // Deliberate corruption fixture only; production revisions remain immutable.
    sqlx::query(r#"DROP TRIGGER "TR_TeletextDesignRevision_ImmutableUpdate""#)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    assert_declared_version_guard(&fixture, id).await;
    sqlx::query(r#"UPDATE "TeletextDesignRevision" SET "Digest"=? WHERE "DesignId"=?"#)
        .bind("0".repeat(64))
        .bind(id)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    assert!(matches!(
        reload_effective_original_teletext_design_revision(
            &fixture.state.application,
            id,
            1,
            designs::budget(),
            channel_documents::policy(),
            designs::deadline()
        )
        .await,
        Err(NativeTeletextDesignError::Storage(_))
    ));
    assert_eq!(
        std::fs::read(fixture.root.path().join("authoring-calls")).unwrap(),
        calls
    );
    assert_no_native_publication(&fixture).await;
    assert!(fixture.playout.try_recv().is_err());
}

#[tokio::test]
async fn effective_off_only_saves_absent_content_without_revalidating_unhealthy_profile() {
    let (mut fixture, base, _, _) = installed(false).await;
    effective::publish(&fixture, 1, None).await;
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
    let candidate = effective::candidate(&fixture, 1, None).await.unwrap();
    let expected = candidate.selection().clone();
    let value = saved_design(
        save(&fixture, candidate, None, designs::budget())
            .await
            .unwrap(),
    );
    let restored = reload(&fixture, value.head().id, 1).await;
    assert_eq!(restored.selection(), &expected);
    assert!(restored.producer_bindings().is_empty());
    assert!(restored.sources().is_empty());
    assert!(restored.encoded().is_none());
    assert!(restored.metadata().is_none());
    assert_eq!(restored.entry_addon_id(), None);
    assert_eq!(
        restored.edit_policy(),
        TeletextSavedEditPolicy::OriginalOnly
    );
    assert_eq!(restored.digest(), value.digest());
    assert!(!restored.selection().bindings[0].enabled());
    for theme in ["day", "night"] {
        assert!(!fixture
            .root
            .path()
            .join(format!("document-calls-{theme}"))
            .exists());
    }
    assert_no_native_publication(&fixture).await;
    assert!(fixture.playout.try_recv().is_err());
}

async fn assert_declared_version_guard(fixture: &Fixture, id: i64) {
    // The bytes and digest remain genuine v2 native originals, but the opaque
    // metadata falsely declares the legacy role. Neither role may be inferred.
    sqlx::query(r#"UPDATE "TeletextDesignRevision" SET "DocumentVersion"=1 WHERE "DesignId"=?"#)
        .bind(id)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    let mut policy = channel_documents::policy();
    policy.encoding.max_units = 0;
    let error = reload_effective_original_teletext_design_revision(
        &fixture.state.application,
        id,
        1,
        designs::budget(),
        policy,
        designs::deadline(),
    )
    .await
    .unwrap_err();
    let NativeTeletextDesignError::Document(error) = error else {
        panic!("declared role was ignored")
    };
    assert_eq!(
        error.reason,
        ersatzrs_app::teletext_design_documents::TeletextDesignDocumentFailure::Version
    );
    assert_eq!(error.binding, None);
    sqlx::query(r#"UPDATE "TeletextDesignRevision" SET "DocumentVersion"=2 WHERE "DesignId"=?"#)
        .bind(id)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
}
