//! Production producer against real SQL facts and the saved publication-intent fence.
//! Ordinary output activation remains a separate integration boundary.

use super::*;
use crate::{channel_documents, designs, guide_capture};
use ersatzrs_core::repositories::{
    TeletextGuideSourceCheckLimits, TeletextPublicationMutation, TeletextPublicationRejection,
};
use ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService;
use ersatzrs_infra::xmltv::GuideSnapshotDigestLimits;
use ersatzrs_web::host::{
    teletext_designs::{
        reload_original_teletext_design_revision,
        request_source_bound_original_teletext_design_publication,
        RequestedOriginalTeletextPublication, SavedOriginalTeletextDesign,
    },
    teletext_guide::capture_source_bound_teletext_guide,
    teletext_preview::{project_teletext_preview, TeletextPreviewBudget},
};

fn source_limits() -> TeletextGuideSourceCheckLimits {
    TeletextGuideSourceCheckLimits {
        max_records: 1000,
        max_source_bytes: 1024 * 1024,
        max_model_nodes: 10_000,
        max_model_bytes: 16 * 1024 * 1024,
        max_digest_fields: 100_000,
        max_digest_bytes: 16 * 1024 * 1024,
    }
}

async fn capture_and_save(
    fixture: &Fixture,
    channel: i64,
    existing: Option<(i64, i64)>,
) -> SavedOriginalTeletextDesign {
    let mut read = guide_capture::read_budget();
    let captured = capture_source_bound_teletext_guide(
        &fixture.state.application,
        &fixture.state.pool,
        &[2],
        guide(&[]).captured_at,
        &mut read,
        designs::deadline(),
        GuideSnapshotDigestLimits {
            max_fields: source_limits().max_digest_fields,
            max_bytes: source_limits().max_digest_bytes,
        },
    )
    .await
    .unwrap();
    let candidate = captured
        .prepare_document_candidate(
            &fixture.state.application,
            channel,
            Some(TVGUIDE),
            GENERATION,
            channel_documents::policy(),
            &mut read,
        )
        .await
        .unwrap();
    assert_eq!(candidate.guide_source(), captured.guide_source());
    assert_eq!(candidate.guide_source().unwrap().channel_ids, [2]);
    designs::save(fixture, candidate, existing).await
}

fn text(encoded: &EncodedTeletextAuthoredService) -> String {
    encoded
        .declared()
        .semantic()
        .base()
        .pages
        .iter()
        .flat_map(|page| &page.rows)
        .flat_map(|row| &row.runs)
        .filter_map(|run| match &run.content {
            TeletextRunContent::Text { text } => Some(text.as_str()),
            TeletextRunContent::Mosaics { .. } => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

async fn request(
    fixture: &Fixture,
    saved: &SavedOriginalTeletextDesign,
    previous: Option<i64>,
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
        source_limits(),
        designs::deadline(),
    )
    .await
    .unwrap()
}

async fn assert_reopened_original(fixture: &Fixture, saved: &SavedOriginalTeletextDesign) {
    let restored = reload_original_teletext_design_revision(
        &fixture.state.application,
        saved.head().id,
        saved.head().revision,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(restored.guide_source(), saved.candidate().guide_source());
    assert!(restored.metadata().is_some());
    assert_eq!(restored.metadata(), saved.candidate().metadata());
    assert_eq!(
        designs::units(restored.encoded().unwrap()),
        designs::units(saved.candidate().encoded().unwrap())
    );
    let encoded = restored.encoded().unwrap();
    let page = encoded.declared().semantic().base().pages[0].address;
    let render = |encoded| {
        project_teletext_preview(
            encoded,
            page,
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
    };
    let original = render(saved.candidate().encoded().unwrap());
    let reopened = render(encoded);
    assert_eq!(reopened.raster(), original.raster());
    assert_eq!(reopened.navigation(), original.navigation());
}

async fn source_designs() -> (
    Fixture,
    SavedOriginalTeletextDesign,
    SavedOriginalTeletextDesign,
) {
    let fixture = installed_fixture().await;
    let day = profile(&fixture, "Day", &[("HEADER", "DAY")]).await;
    let night = profile(
        &fixture,
        "Night",
        &[
            ("PAGE_START", "400"),
            ("PAGE_END", "499"),
            ("INDEX_PAGE", "410"),
            ("THEME", "night"),
            ("HEADER", "NIGHT"),
        ],
    )
    .await;
    fixture.bind(1, &day, None).await;
    fixture.bind(2, &night, None).await;
    guide_capture::seed_guide(&fixture).await;
    let first = capture_and_save(&fixture, 1, None).await;
    let other = capture_and_save(&fixture, 2, None).await;
    (fixture, first, other)
}

#[tokio::test]
#[ignore = "requires an explicit production bundle and isolated state/data directories"]
async fn production_guide_sql_refresh_reopens_and_fences_explicit_publication_intent() {
    let (mut fixture, first, other) = source_designs().await;
    for saved in [&first, &other] {
        assert!(text(saved.candidate().encoded().unwrap()).contains("Captured Guide"));
        assert!(fixture
            .state
            .application
            .teletext_publication_request(saved.candidate().selection().channel_id)
            .await
            .unwrap()
            .is_none());
    }
    assert_ne!(first.digest(), other.digest());
    assert_eq!(
        first
            .candidate()
            .encoded()
            .unwrap()
            .declared()
            .semantic()
            .base()
            .initial_page
            .get(),
        200
    );
    assert_eq!(
        other
            .candidate()
            .encoded()
            .unwrap()
            .declared()
            .semantic()
            .base()
            .initial_page
            .get(),
        410
    );
    fixture = fixture.reopen().await;
    assert_reopened_original(&fixture, &first).await;
    assert_reopened_original(&fixture, &other).await;
    let TeletextPublicationMutation::Requested(initial) = request(&fixture, &first, None).await
    else {
        panic!("unchanged production guide must admit explicit pending intent");
    };
    let previous = initial.request().clone();
    sqlx::query(r#"UPDATE "MovieMetadata" SET "Title"='Refreshed Guide' WHERE "Id"=30"#)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    assert!(matches!(
        request(&fixture, &first, Some(previous.id)).await,
        TeletextPublicationMutation::Rejected(TeletextPublicationRejection::GuideChanged)
    ));
    assert_eq!(
        fixture
            .state
            .application
            .teletext_publication_request(1)
            .await
            .unwrap(),
        Some(previous.clone())
    );
    let updated =
        capture_and_save(&fixture, 1, Some((first.head().id, first.head().revision))).await;
    assert_eq!(updated.head().revision, 2);
    assert!(text(updated.candidate().encoded().unwrap()).contains("Refreshed Guide"));
    assert!(!text(updated.candidate().encoded().unwrap()).contains("Captured Guide"));
    assert_reopened_original(&fixture, &first).await;
    assert_reopened_original(&fixture, &other).await;
    let TeletextPublicationMutation::Requested(renewed) =
        request(&fixture, &updated, Some(previous.id)).await
    else {
        panic!("fresh production guide must admit replacement intent only after explicit request");
    };
    let renewed = renewed.request().clone();
    fixture = fixture.reopen().await;
    assert_eq!(
        fixture
            .state
            .application
            .teletext_publication_request(1)
            .await
            .unwrap(),
        Some(renewed)
    );
    assert!(fixture
        .state
        .application
        .teletext_publication_request(2)
        .await
        .unwrap()
        .is_none());
    assert_reopened_original(&fixture, &updated).await;
    assert_reopened_original(&fixture, &other).await;
    assert!(fixture.playout.try_recv().is_err());
}
