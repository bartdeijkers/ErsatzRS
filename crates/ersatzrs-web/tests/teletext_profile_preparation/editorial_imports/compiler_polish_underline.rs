//! Exact underlined Polish Base edit through the existing native design flow.
//! Genuine editor evidence is opt-in; ordinary and client proof stay separate.

use super::*;
use ersatzrs_domain::teletext::*;
use ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService;
use serde_json::{json, Value};

const UNDERLINED: [u16; 10] = [0, 0x3fc, 0x18, 0x30, 0x1f8, 0xc0, 0x180, 0x3fc, 0, 0xfff];

pub(super) fn response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = compiler_polish::response(theme, page, subcode);
    if theme != "day" || page != 124 {
        return value;
    }
    let owner = value["service"]["pages"][0]["address"].clone();
    let adaptive = value["service"]["objects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|object| {
            object["identity"]["object_id"] == 0xfffe
                && object["identity"]["owner"] == owner
                && object["object_type"] == "adaptive"
        })
        .unwrap();
    let commands = adaptive["program"]["commands"].as_array_mut().unwrap();
    assert_eq!(commands[0]["column"], 0);
    assert_eq!(commands[0]["operation"]["kind"], "attributes");
    assert_eq!(commands[0]["operation"]["double_width"], true);
    assert_eq!(commands[0]["operation"]["double_height"], false);
    assert_eq!(
        commands[0]["operation"]["underline_or_separated_mosaics"],
        false
    );
    assert_eq!(commands[1]["column"], 2);
    assert_eq!(
        commands[1]["operation"]["underline_or_separated_mosaics"],
        false
    );
    commands[0]["operation"]["underline_or_separated_mosaics"] = json!(true);
    value
}

pub(super) fn assert_originals(flow: &Flow) {
    assert_eq!(
        response("night", 124, 32),
        compiler_polish::response("night", 124, 32)
    );
    for (index, original) in flow.accepted.iter().enumerate() {
        compiler_polish::assert_authored(original.service());
        assert!(original.service().compiler_drcs_resources().is_empty());
        let composition = polish_carrier::composition(original.service());
        polish_carrier::assert_base(composition, b'A');
        let column = composition.display_geometry().left_width() + 7;
        let cell = composition.display_resolution().cell(1, column).unwrap();
        assert_eq!(
            cell.attributes().underline_or_separated_mosaics().value(),
            Some(index == 0)
        );
    }
    let day = &flow.accepted[0].selection().bindings[0];
    let night = &flow.accepted[1].selection().bindings[0];
    assert_eq!(day.addon_id, night.addon_id);
    assert_ne!(day.profile_id, night.profile_id);
}

fn assert_fallback(flow: &Flow, service: &EncodedTeletextAuthoredService) {
    let rows = |service: &EncodedTeletextAuthoredService| {
        let page = service
            .pages()
            .iter()
            .find_map(|page| match page {
                EncodedTeletextAuthoredPage::CompiledDisplay(page)
                    if page.page().address().page.get() == 124 =>
                {
                    Some(page.page())
                }
                _ => None,
            })
            .unwrap();
        project_encoded_teletext_display(page, 128)
            .unwrap()
            .records
            .into_iter()
            .filter(|record| matches!(record, TeletextEncodedDisplayRecord::BaseRow { .. }))
            .collect::<Vec<_>>()
    };
    let mut expected = rows(flow.accepted[0].service());
    let cells = expected
        .iter_mut()
        .find_map(|record| match record {
            TeletextEncodedDisplayRecord::BaseRow { row: 1, cells } => Some(cells),
            _ => None,
        })
        .unwrap();
    assert_eq!(cells[7], b'A');
    cells[7] = 0x5b;
    assert_eq!(rows(service), expected);
}

pub(super) fn assert_concept(flow: &Flow, service: &EncodedTeletextAuthoredService) {
    compiler_polish::assert_authored(service);
    let expected = flow.accepted[0]
        .service()
        .declared()
        .clone()
        .with_physical_base_overlays(
            vec![compiler_polish::expected_overlay(flow, 0x5b)],
            channel_documents::policy().validation.service,
        )
        .unwrap();
    assert_eq!(service.declared(), &expected);
    assert_eq!(
        service.magazine_units(),
        flow.accepted[0].service().magazine_units()
    );
    assert_eq!(
        external_editor_probe::preview_page(service, 125),
        external_editor_probe::preview_page(flow.accepted[0].service(), 125)
    );
    assert_fallback(flow, service);
    assert_binding(service);
    assert_pixels(service);
}

fn assert_binding(service: &EncodedTeletextAuthoredService) {
    assert_eq!(service.compiler_drcs_resources().len(), 1);
    assert_eq!(
        service
            .pages()
            .iter()
            .filter(|page| page.resource_header().is_some())
            .count(),
        1
    );
    let resource = &service.compiler_drcs_resources()[0];
    assert_eq!(resource.consumers(), [channel_documents::address(124, 32)]);
    let identity = resource.table().identity();
    assert_eq!(identity.kind(), TeletextResourceKind::Drcs);
    assert_eq!(identity.page().magazine(), 1);
    assert_eq!(identity.page().page(), 0xa0);
    assert_eq!(identity.logical_subpage().get(), 0);
    assert_eq!(resource.table().glyphs().len(), 1);
    assert_eq!(resource.table().glyphs()[0].slot.get(), 1);
    assert_eq!(resource.table().glyphs()[0].bitmap.get(), UNDERLINED);
    let page = service
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page)
                if page.page().address() == channel_documents::address(124, 32) =>
            {
                Some(page)
            }
            _ => None,
        })
        .unwrap();
    let original = page.normalized().original();
    polish_carrier::assert_base(original, 0x5b);
    let column = original.display_geometry().left_width() + 7;
    let actual = page.normalized().physical().replay();
    for column in [column, column + 1] {
        let first = original.display_resolution().cell(1, column).unwrap();
        let second = actual.display_resolution().cell(1, column).unwrap();
        assert_eq!(
            first.attributes().underline_or_separated_mosaics().value(),
            Some(true)
        );
        assert_eq!(
            second.attributes().underline_or_separated_mosaics().value(),
            Some(false)
        );
        assert_eq!(first.fragment(), second.fragment());
    }
    let origin = actual
        .display_resolution()
        .cell(1, column)
        .unwrap()
        .origin()
        .unwrap();
    assert_eq!(
        actual.display_resolution().origins()[origin].effective_size(),
        Some(TeletextDisplaySize::DoubleWidth)
    );
    let candidate = actual.display_resolution().origins()[origin].candidate_index();
    let execution = actual
        .drcs_execution()
        .records()
        .iter()
        .find(|record| record.candidate_index() == candidate)
        .unwrap();
    assert_eq!(execution.resource(), identity);
    assert_eq!(execution.reference().table, TeletextDrcsTableScope::Normal);
    assert_eq!(execution.reference().subtable.get(), 0);
    assert_eq!(execution.reference().slot.get(), 1);
    assert_eq!(execution.bitmap().get(), UNDERLINED);
}

fn assert_pixels(service: &EncodedTeletextAuthoredService) {
    let left = usize::from(
        polish_carrier::composition(service)
            .display_geometry()
            .left_width(),
    );
    let raster = external_editor_probe::preview_page(service, 124);
    for (row, mask) in UNDERLINED.into_iter().enumerate() {
        for x in 0..24 {
            let expected = if mask & (1 << (11 - x / 2)) == 0 {
                [0, 0, 0]
            } else {
                [15, 15, 15]
            };
            assert!(
                matches!(raster.pixel((left+7)*12+x,10+row), Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(value))) if value.get() == expected),
                "underlined Polish pixel{x},{row}"
            );
        }
    }
}

pub(super) async fn assert_restored(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
) -> TeletextRaster {
    let restored = drcs_bitmap::restored(fixture, concept).await;
    assert_eq!(restored.selection(), flow.saved[0].candidate().selection());
    assert_eq!(restored.metadata(), flow.saved[0].candidate().metadata());
    assert_eq!(
        restored.transmissions(),
        flow.saved[0].candidate().transmissions()
    );
    assert_eq!(
        restored.sources().len(),
        flow.saved[0].candidate().sources().len()
    );
    for (before, after) in flow.saved[0]
        .candidate()
        .sources()
        .iter()
        .zip(restored.sources())
    {
        assert_eq!(before.bytes(), after.bytes());
        assert_eq!(before.authoring(), after.authoring());
    }
    assert_concept(flow, restored.encoded().unwrap());
    external_editor_probe::preview_page(restored.encoded().unwrap(), 124)
}

#[tokio::test]
async fn no_bank_polish_underline_concept_previews_publishes_and_reopens() {
    Box::pin(run(None)).await;
}

/// The existing editor-output selector prepares a fresh native-original-1 using
/// the regular case; this opt-in case authenticates that entire input inventory.
#[tokio::test]
#[ignore = "requires fresh underlined no-bank export, pinned actual QTM Save As and editor-proof.json; no ordinary/client claim"]
async fn actual_qtm_no_bank_polish_underline_edit_previews_publishes_and_reopens() {
    let directory =
        std::path::PathBuf::from(client_capture::selected("ERSATZRS_QTM_COMPILER_POLISH_DIR"));
    Box::pin(run(Some(&directory))).await;
}

async fn run(actual: Option<&Path>) {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        response,
        Fixture::new_persistent().await,
        channel_documents::policy,
    ))
    .await;
    assert_originals(&flow);
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let (request, proof, hashes) = if let Some(directory) = actual {
        let (request, proof) = compiler_polish::actual_request(&flow, directory);
        (
            request,
            proof,
            compiler_polish::actual_input_hashes(&flow, directory),
        )
    } else {
        polish_carrier::write_editor_inputs(&flow);
        (request(&flow, 1, 7, 0x5b), Value::Null, vec![])
    };
    let concept = import(&fixture, request).await;
    assert_eq!(concept.head().id, flow.saved[0].head().id);
    assert_eq!(concept.head().revision, 2);
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    let preview = assert_restored(&fixture, &flow, &concept).await;
    fixture = fixture.reopen().await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert_eq!(assert_restored(&fixture, &flow, &concept).await, preview);
    let accepted = publish_and_accept(&fixture, &flow, &concept).await;
    assert_concept(&flow, accepted.service());
    assert_eq!(
        external_editor_probe::preview_page(accepted.service(), 124),
        preview
    );
    fixture = fixture.reopen().await;
    compiler_polish::assert_pointers(&fixture, &accepted, &flow.accepted[1]).await;
    assert_concept(&flow, recover(&fixture, 1).await.service());
    retained.assert_unchanged(&fixture, &flow).await;
    if let Some(directory) = actual {
        assert_eq!(
            compiler_polish::actual_input_hashes(&flow, directory),
            hashes
        );
        polish_delivery::write_json(
            directory,
            "native-underlined-proof.json",
            &json!({
            "probe_version":1,"scope":"genuine fresh QTM no-bank underlined Polish concept, shared preview, explicit native publication and reopen",
            "editor":proof,"input_hashes":hashes,"design_id":concept.head().id,"revision":concept.head().revision,"digest":concept.digest(),
            "generation_id":accepted.generation().id,"request_id":accepted.generation().request.id,
            "normal_resource_page_hex":"1A0","s1":0,"slot":1,"literal_12x10_mask":UNDERLINED,"wide_pixels_checked":240,
            "original_d5":true,"physical_drcs_d5":false,"same_addon_distinct_profiles":true,
            "originals_profiles_bindings_package_config_receipts_and_night_unchanged":true,
            "ordinary_delivery_proven":false,"independent_decoding_proven":false,"actual_client_proven":false,"full_level25_proven":false,
            "limitations":"Actual GUI ownership/images are external evidence. No headed admin, ordinary stream, automatic receiver acquisition, warm transition, suitable client or full-engine/platform acceptance claim"}),
        );
    }
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
