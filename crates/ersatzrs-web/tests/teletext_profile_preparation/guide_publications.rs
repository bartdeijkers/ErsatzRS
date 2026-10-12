//! Actual SQL capture to saved originals, preview and atomic pending source checks.
//! The parent intentionally limits real POSIX producers to Unix.

use super::{channel_documents, designs, documents::DocumentFixture, guide_capture, *};
use ersatzrs_core::repositories::{
    TeletextGuideSourceCheckLimits, TeletextPublicationMutation, TeletextPublicationRejection,
};
use ersatzrs_domain::teletext::{TeletextColourPixelOptions, TeletextFlashPhase};
use ersatzrs_infra::xmltv::GuideSnapshotDigestLimits;
use ersatzrs_web::host::{
    teletext_designs::{
        reload_original_teletext_design_revision, request_original_teletext_design_publication,
        request_source_bound_original_teletext_design_publication,
        RequestedOriginalTeletextPublication, SavedOriginalTeletextDesign,
    },
    teletext_guide::{capture_source_bound_teletext_guide, TeletextGuideCaptureError},
    teletext_preview::{project_teletext_preview, TeletextPreviewBudget},
};

fn limits() -> TeletextGuideSourceCheckLimits {
    TeletextGuideSourceCheckLimits {
        max_records: 1000,
        max_source_bytes: 1024 * 1024,
        max_model_nodes: 10_000,
        max_model_bytes: 16 * 1024 * 1024,
        max_digest_fields: 100_000,
        max_digest_bytes: 16 * 1024 * 1024,
    }
}

async fn source_design() -> (Fixture, SavedOriginalTeletextDesign) {
    let (fixture, _) = designs::persistent_pair(DocumentFixture::Authored).await;
    guide_capture::seed_guide(&fixture).await;
    let mut budget = guide_capture::read_budget();
    let captured = capture_source_bound_teletext_guide(
        &fixture.state.pool,
        &[2],
        guide(&[]).captured_at,
        &mut budget,
        designs::deadline(),
        GuideSnapshotDigestLimits {
            max_fields: limits().max_digest_fields,
            max_bytes: limits().max_digest_bytes,
        },
    )
    .await
    .unwrap();
    let prepared = captured
        .prepare_document_candidate(
            &fixture.state.application,
            1,
            Some(SECONDARY),
            GENERATION,
            channel_documents::policy(),
            &mut budget,
        )
        .await
        .unwrap();
    assert_eq!(prepared.guide_source(), captured.guide_source());
    assert_eq!(prepared.guide_source().unwrap().channel_ids, [2]);
    assert_eq!(prepared.selection().channel_id, 1);
    let saved = designs::save(&fixture, prepared, None).await;
    (fixture, saved)
}

async fn request(
    fixture: &Fixture,
    saved: &SavedOriginalTeletextDesign,
    previous: Option<i64>,
    source_limits: TeletextGuideSourceCheckLimits,
) -> TeletextPublicationMutation<RequestedOriginalTeletextPublication> {
    request_source_bound_original_teletext_design_publication(
        &fixture.state.application,
        saved.head().id,
        saved.head().revision,
        saved.digest().to_owned(),
        saved.head().revision,
        previous,
        designs::budget(),
        channel_documents::policy(),
        source_limits,
        designs::deadline(),
    )
    .await
    .unwrap()
}

async fn request_count(fixture: &Fixture) -> i64 {
    sqlx::query_scalar(r#"SELECT COUNT(*) FROM "TeletextPublicationRequest""#)
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap()
}

fn preview(
    encoded: &ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService,
) -> ersatzrs_web::host::teletext_preview::TeletextNativePreview<'_> {
    project_teletext_preview(
        encoded,
        channel_documents::address(124, 32),
        TeletextColourPixelOptions {
            reveal_concealed: false,
            flash_phase: TeletextFlashPhase::Original,
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

fn assert_exact_inputs(
    restored: &ersatzrs_web::host::teletext_designs::RestoredOriginalTeletextDesign,
    saved: &SavedOriginalTeletextDesign,
) {
    for contract in [
        ersatzrs_domain::TeletextPageContract::V1,
        ersatzrs_domain::TeletextPageContract::V2,
    ] {
        assert_eq!(
            restored.guide_inputs().unwrap().request(contract),
            saved.candidate().requests().request(contract)
        );
    }
}

#[tokio::test]
async fn sql_source_association_reopens_with_exact_inputs_preview_and_pending_intent() {
    let (mut fixture, saved) = source_design().await;
    let source = saved.candidate().guide_source().unwrap().clone();
    let originals = designs::units(saved.candidate().encoded().unwrap());
    let original_preview = preview(saved.candidate().encoded().unwrap());
    let config = designs::config(&fixture).await;
    let bindings = fixture
        .state
        .application
        .channel_teletext_bindings(1)
        .await
        .unwrap();
    let calls = std::fs::read(fixture.root.path().join("channel-calls")).unwrap();
    assert_eq!(request_count(&fixture).await, 0);
    let legacy = request_original_teletext_design_publication(
        &fixture.state.application,
        saved.head().id,
        1,
        saved.digest().to_owned(),
        1,
        None,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    assert!(matches!(
        legacy,
        TeletextPublicationMutation::Rejected(TeletextPublicationRejection::GuideFenceRequired)
    ));
    assert_eq!(request_count(&fixture).await, 0);
    fixture = fixture.reopen().await;
    let restored = reload_original_teletext_design_revision(
        &fixture.state.application,
        saved.head().id,
        1,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(restored.guide_source(), Some(&source));
    assert_exact_inputs(&restored, &saved);
    assert_eq!(designs::units(restored.encoded().unwrap()), originals);
    let reopened_preview = preview(restored.encoded().unwrap());
    assert_eq!(reopened_preview.raster(), original_preview.raster());
    assert_eq!(reopened_preview.navigation(), original_preview.navigation());
    let TeletextPublicationMutation::Requested(queued) =
        request(&fixture, &saved, None, limits()).await
    else {
        panic!("unchanged historical SQL context must admit pending intent");
    };
    assert_eq!(queued.design().guide_source(), Some(&source));
    assert_eq!(request_count(&fixture).await, 1);
    assert_eq!(
        designs::units(queued.design().encoded().unwrap()),
        originals
    );
    assert_eq!(
        std::fs::read(fixture.root.path().join("channel-calls")).unwrap(),
        calls
    );
    assert_eq!(designs::config(&fixture).await, config);
    assert_eq!(
        fixture
            .state
            .application
            .channel_teletext_bindings(1)
            .await
            .unwrap(),
        bindings
    );
    assert!(fixture.playout.try_recv().is_err());
    let intent = queued.request().clone();
    drop(queued);
    fixture = fixture.reopen().await;
    assert_eq!(
        fixture
            .state
            .application
            .teletext_publication_request(1)
            .await
            .unwrap(),
        Some(intent)
    );
    assert!(fixture
        .state
        .application
        .teletext_publication_request(2)
        .await
        .unwrap()
        .is_none());
    assert!(fixture.playout.try_recv().is_err());
}

#[tokio::test]
async fn changed_guide_after_save_preserves_original_preview_and_previous_pending_pointer() {
    for sql in [
        r#"UPDATE "MovieMetadata" SET "Title"='Changed Guide' WHERE "Id"=30"#,
        r#"UPDATE "PlayoutItem" SET "GuideGroup"=8 WHERE "Id"=41"#,
        // Same resolved locale, different original raw configuration.
        r#"UPDATE "ConfigElement" SET "Value"='nl' WHERE "Key"='pages.language'"#,
    ] {
        let (mut fixture, saved) = source_design().await;
        let TeletextPublicationMutation::Requested(first) =
            request(&fixture, &saved, None, limits()).await
        else {
            panic!("unchanged initial context must admit pending intent");
        };
        let previous = first.request().clone();
        sqlx::query(sql).execute(&fixture.state.pool).await.unwrap();
        assert!(matches!(
            request(&fixture, &saved, Some(previous.id), limits()).await,
            TeletextPublicationMutation::Rejected(TeletextPublicationRejection::GuideChanged)
        ));
        assert_eq!(request_count(&fixture).await, 1);
        assert_eq!(
            fixture
                .state
                .application
                .teletext_publication_request(1)
                .await
                .unwrap(),
            Some(previous)
        );
        let historical = reload_original_teletext_design_revision(
            &fixture.state.application,
            saved.head().id,
            1,
            designs::budget(),
            channel_documents::policy(),
            designs::deadline(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            designs::units(historical.encoded().unwrap()),
            designs::units(saved.candidate().encoded().unwrap())
        );
        assert!(fixture.playout.try_recv().is_err());
    }
}

#[tokio::test]
async fn source_fingerprint_and_fresh_admission_limits_fail_without_pending_writes() {
    let (fixture, saved) = source_design().await;
    for constrained in [
        TeletextGuideSourceCheckLimits {
            max_records: 0,
            ..limits()
        },
        TeletextGuideSourceCheckLimits {
            max_model_nodes: 0,
            ..limits()
        },
        TeletextGuideSourceCheckLimits {
            max_digest_fields: 0,
            ..limits()
        },
    ] {
        assert!(matches!(
            request(&fixture, &saved, None, constrained).await,
            TeletextPublicationMutation::Rejected(TeletextPublicationRejection::GuideUnavailable)
        ));
        assert_eq!(request_count(&fixture).await, 0);
    }
    let mut read = guide_capture::read_budget();
    let failed = capture_source_bound_teletext_guide(
        &fixture.state.pool,
        &[2],
        guide(&[]).captured_at,
        &mut read,
        designs::deadline(),
        GuideSnapshotDigestLimits {
            max_fields: 0,
            max_bytes: 0,
        },
    )
    .await;
    assert!(matches!(failed, Err(TeletextGuideCaptureError::Digest(_))));
    assert_eq!(request_count(&fixture).await, 0);
}
