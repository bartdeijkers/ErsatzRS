//! Exact native preview from synthetic complete compiler output, without UI.

use super::channel_documents::address;
use super::documents::{self, DocumentFixture};
use super::*;
use ersatzrs_domain::teletext::{
    TeletextColourPixel, TeletextColourPixelOptions, TeletextFlashPhase,
};
use ersatzrs_ffmpeg::teletext::{
    encode_teletext_authored_service, render_teletext_composition, EncodedTeletextAuthoredPage,
    EncodedTeletextAuthoredService, TeletextRasterBudget, TeletextRasterPixel,
};
use ersatzrs_web::host::teletext_preview::{
    project_teletext_preview, TeletextPreviewBudget, TeletextPreviewFailure,
};
use serde_json::{json, Value};

fn budget() -> TeletextPreviewBudget {
    TeletextPreviewBudget {
        max_pages: 16,
        max_model_records: 32_768,
        max_pixels: 168_000,
        max_diagnostics: 1400,
        max_work: 50_000_000,
    }
}

fn options() -> TeletextColourPixelOptions {
    TeletextColourPixelOptions {
        reveal_concealed: false,
        flash_phase: TeletextFlashPhase::Original,
    }
}

fn encode(value: &Value) -> EncodedTeletextAuthoredService {
    let limits = documents::validation_budget();
    let original = ersatzrs_app::teletext_pages_v2::parse_declared_service(
        &serde_json::to_vec(value).unwrap(),
        GENERATION,
        limits.service,
        limits.declarations,
    )
    .unwrap();
    encode_teletext_authored_service(original, &[], &documents::policy().encoding).unwrap()
}

fn subpages() -> Value {
    let mut value = documents::document(DocumentFixture::Authored, "night");
    let mut second = value["pages"][0].clone();
    second["address"]["subcode"] = json!(32);
    value["pages"].as_array_mut().unwrap().push(second);
    let mut enhancement = value["page_enhancements"][0].clone();
    enhancement["address"]["subcode"] = json!(32);
    enhancement["local_program"]["commands"][2]["operation"]["code"] = json!(66);
    value["page_enhancements"]
        .as_array_mut()
        .unwrap()
        .push(enhancement);
    value["mot_selections"].as_array_mut().unwrap().push(json!({
        "display":{"page":123,"subcode":32}, "mot":null,
    }));
    value["pages"][0]["navigation"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "role":"red", "target":{"page":123,"subcode":32},
        }));
    value
}

#[test]
fn exact_subpages_keep_original_navigation_and_enhanced_pixel_identity() {
    let service = encode(&subpages());
    let first = project_teletext_preview(&service, address(123, 16), options(), budget()).unwrap();
    let second = project_teletext_preview(&service, address(123, 32), options(), budget()).unwrap();
    assert_eq!(first.raster().owner(), address(123, 16));
    assert_eq!(second.raster().owner(), address(123, 32));
    assert_eq!(first.navigation()[0].target.subcode, None);
    assert_eq!(first.navigation()[1].target.subcode.unwrap().get(), 32);
    assert_eq!(second.navigation().len(), 1);
    assert!(first.raster().diagnostics().is_empty());
    assert!(second.raster().diagnostics().is_empty());
    assert_ne!(first.raster().pixels(), second.raster().pixels());
    let EncodedTeletextAuthoredPage::CompiledDisplay(page) = &service.pages()[0] else {
        panic!("original Local program requires compiled display");
    };
    let canonical = render_teletext_composition(
        page.normalized().original(),
        options(),
        TeletextRasterBudget {
            max_pixels: 168_000,
            max_diagnostics: 1400,
        },
    )
    .unwrap();
    assert_eq!(first.raster(), &canonical);
    assert_eq!(first.raster().width(), 56 * 12);
    assert!((144..156).any(|x| (10..20).any(|y| matches!(first.raster().pixel(x,y),
        Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(colour))) if colour.get() == [0,0,15]))));
    assert_eq!(
        first.raster().pixel(4 * 12, 0),
        Some(TeletextRasterPixel::ReceiverHeader)
    );
    assert!(!format!("{first:?}").contains("ZZ"));
}

#[test]
fn legal_native_subtitle_window_retains_video_plane_instead_of_fabricating_rgb() {
    let mut value = documents::document(DocumentFixture::Authored, "day");
    value["pages"][0]["kind"] = json!("subtitle");
    value["pages"][0]["rows"][0]["runs"][0]["style"]["boxed"] = json!(true);
    value["page_enhancements"][0]["local_program"] = Value::Null;
    let service = encode(&value);
    let view = project_teletext_preview(&service, address(123, 16), options(), budget()).unwrap();
    assert!(view.raster().diagnostics().is_empty());
    assert!(view
        .raster()
        .pixels()
        .contains(&TeletextRasterPixel::Colour(TeletextColourPixel::Video)));
    assert!(view
        .raster()
        .pixels()
        .contains(&TeletextRasterPixel::ReceiverHeader));
}

#[test]
fn native_assigned_arabic_cell_renders_without_diagnostics_or_latin_substitution() {
    let mut value = documents::document(DocumentFixture::Authored, "day");
    value["page_enhancements"][0]["local_program"] = Value::Null;
    value["magazines"][0]["presentation"]["primary"] = json!(0x47);
    // The existing German fallback spells7E; the declared Arabic primary owns
    // its actual glyph. Table44 prints7/E, so pending/tilde substitution is wrong.
    value["pages"][0]["rows"][0]["runs"][0]["content"]["text"] = json!("ß");
    let service = encode(&value);
    assert!(matches!(
        &service.pages()[0],
        EncodedTeletextAuthoredPage::NativeDisplay(_)
    ));
    let view = project_teletext_preview(
        &service,
        address(123, 16),
        options(),
        TeletextPreviewBudget {
            max_diagnostics: 0,
            ..budget()
        },
    )
    .unwrap();
    assert!(view.raster().diagnostics().is_empty());
    // Existing font policy, with the retained panel offset (8 ->12). This is
    // physical coded interpretation, not proof of logical Arabic Text support.
    let mask = [0_u16, 390, 198, 102, 54, 28, 102, 254, 0, 0];
    for (y, bits) in mask.iter().enumerate() {
        for x in 0..12 {
            let Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(rgb))) =
                view.raster().pixel(144 + x, 10 + y)
            else {
                panic!("assigned Arabic7/E must have resolved pixels")
            };
            assert_eq!(
                rgb.get(),
                if bits & (1 << (11 - x)) == 0 {
                    [0; 3]
                } else {
                    [15; 3]
                }
            );
        }
    }
}

#[test]
fn missing_exact_owner_and_host_budgets_return_no_partial_or_mutated_preview() {
    let service = encode(&subpages());
    let before = service
        .pages()
        .iter()
        .map(|page| page.data_units().to_vec())
        .collect::<Vec<_>>();
    for owner in [address(123, 0), address(124, 16)] {
        let error = project_teletext_preview(&service, owner, options(), budget()).unwrap_err();
        assert_eq!(error.owner, owner);
        assert!(matches!(
            error.reason,
            TeletextPreviewFailure::MissingDisplay
        ));
    }
    for (category, limited) in [
        (
            "encoded pages",
            TeletextPreviewBudget {
                max_pages: 1,
                ..budget()
            },
        ),
        (
            "model records",
            TeletextPreviewBudget {
                max_model_records: 0,
                ..budget()
            },
        ),
        (
            "pixels",
            TeletextPreviewBudget {
                max_pixels: 167_999,
                ..budget()
            },
        ),
        (
            "work",
            TeletextPreviewBudget {
                max_work: 0,
                ..budget()
            },
        ),
    ] {
        let error =
            project_teletext_preview(&service, address(123, 16), options(), limited).unwrap_err();
        assert_eq!(error.owner, address(123, 16));
        assert!(
            matches!(error.reason, TeletextPreviewFailure::Budget {category: actual, ..} if actual == category)
        );
    }
    assert_eq!(
        service
            .pages()
            .iter()
            .map(|page| page.data_units().to_vec())
            .collect::<Vec<_>>(),
        before
    );
    assert_eq!(
        project_teletext_preview(&service, address(123, 16), options(), budget())
            .unwrap()
            .raster()
            .pixels()
            .len(),
        168_000
    );
}

#[tokio::test]
async fn installed_original_save_exact_preview_and_pending_request_keep_one_native_model() {
    use super::channel_documents::{candidate, policy as encoding_policy};
    use super::designs::{
        budget as document_budget, config, deadline, persistent_pair, save, units,
    };
    use ersatzrs_core::repositories::TeletextPublicationMutation;
    use ersatzrs_web::host::teletext_designs::request_original_teletext_design_publication;

    let (mut fixture, _) = persistent_pair(DocumentFixture::Authored).await;
    let config_before = config(&fixture).await;
    let bindings = fixture
        .state
        .application
        .channel_teletext_bindings(1)
        .await
        .unwrap();
    let prepared = candidate(&fixture, encoding_policy()).await.unwrap();
    let saved = save(&fixture, prepared, None).await;
    let selected = saved.candidate().selection();
    assert!(fixture
        .state
        .application
        .teletext_publication_request(1)
        .await
        .unwrap()
        .is_none());
    let original = saved.candidate().encoded().unwrap();
    let preview =
        project_teletext_preview(original, address(124, 32), options(), budget()).unwrap();
    assert_eq!(preview.raster().owner(), address(124, 32));
    assert!(preview.raster().diagnostics().is_empty());
    let requested = request_original_teletext_design_publication(
        &fixture.state.application,
        saved.head().id,
        saved.head().revision,
        saved.digest().to_owned(),
        saved.head().revision,
        None,
        document_budget(),
        encoding_policy(),
        deadline(),
    )
    .await
    .unwrap();
    let TeletextPublicationMutation::Requested(requested) = requested else {
        panic!("complete unchanged originals must yield a pending request");
    };
    assert_eq!(requested.request().channel_id, 1);
    assert_eq!(requested.request().design_id, saved.head().id);
    assert_eq!(requested.request().revision, saved.head().revision);
    assert_eq!(requested.design().selection(), selected);
    let guide_inputs = requested.design().guide_inputs().unwrap();
    for contract in [
        ersatzrs_domain::TeletextPageContract::V1,
        ersatzrs_domain::TeletextPageContract::V2,
    ] {
        assert_eq!(
            guide_inputs.request(contract),
            saved.candidate().requests().request(contract)
        );
    }
    let actual = requested.design().encoded().unwrap();
    let recompiled =
        project_teletext_preview(actual, address(124, 32), options(), budget()).unwrap();
    assert_eq!(recompiled.raster(), preview.raster());
    assert_eq!(recompiled.navigation(), preview.navigation());
    assert_eq!(units(actual), units(original));
    assert_eq!(actual.magazine_units(), original.magazine_units());
    let original_themes = saved.candidate().producers()[1]
        .original()
        .semantic()
        .magazine_presentations();
    assert!(!original_themes.is_empty());
    assert_eq!(
        requested.design().producers()[1]
            .original()
            .semantic()
            .magazine_presentations(),
        original_themes
    );
    assert_eq!(
        fixture
            .state
            .application
            .teletext_publication_request(1)
            .await
            .unwrap(),
        Some(requested.request().clone())
    );
    assert_eq!(config(&fixture).await, config_before);
    assert_eq!(
        fixture
            .state
            .application
            .channel_teletext_bindings(1)
            .await
            .unwrap(),
        bindings
    );
    super::channel_documents::assert_requests(&fixture, 1, 2);
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
