//! Literal Turkish Base23 through fresh receipt-bound concepts and publication.
//! Genuine QTM uses the separately owned live ordinary-flow fixture.

use super::*;
use ersatzrs_domain::teletext::*;
use ersatzrs_ffmpeg::teletext::{EncodedTeletextAuthoredService, TeletextMainComposition};
use serde_json::{json, Value};
use std::path::PathBuf;

const MASK: [u16; 10] = [0, 0, 0x3f0, 0xc0, 0xd8, 0xd8, 0xd8, 0xde, 0, 0];

pub(super) fn response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = compiler_polish::response(theme, page, subcode);
    if theme == "day" && page == 124 {
        value["service"]["page_enhancements"][0]["presentation"]["primary"] = json!(0x16);
    }
    value
}

fn assert_base(composition: &TeletextMainComposition, primary: u8, code: u8) {
    assert_eq!(composition.base_display().primary().get(), primary);
    let column = composition.display_geometry().left_width() + 7;
    let display = composition.display_resolution();
    let origin = display.cell(1, column).unwrap().origin().unwrap();
    let candidate =
        composition.glyph_layers().candidates()[display.origins()[origin].candidate_index()];
    assert_eq!(candidate.priority(), TeletextGlyphPriority::Base);
    assert_eq!(candidate.base().unwrap().code(), code);
    assert_eq!(candidate.source(), None);
    assert_eq!(candidate.root_source(), None);
    assert_eq!(candidate.invocation(), None);
    let intent = composition.glyph_intent().records()[display.origins()[origin].candidate_index()];
    assert_eq!(intent.disposition(), TeletextGlyphDisposition::Assigned);
    assert!(
        matches!(intent.glyph(), Some(TeletextGlyphIdentity::G0 {glyph})
        if glyph.repertoire()==TeletextG0Repertoire::Latin && glyph.code()==code
            && glyph.national_subset()==if code==0x23 {Some(TeletextNationalSubset::Turkish)} else {None})
    );
    assert_eq!(
        display.origins()[origin].effective_size(),
        Some(TeletextDisplaySize::DoubleWidth)
    );
    let owned = (0..25)
        .flat_map(|row| (0..display.width()).map(move |col| (row, col)))
        .filter(|&(row, col)| {
            display.cell(row, col).and_then(TeletextDisplayCell::origin) == Some(origin)
        })
        .collect::<Vec<_>>();
    assert_eq!(owned, vec![(1, column), (1, column + 1)]);
}

pub(super) fn assert_originals(flow: &Flow) {
    assert_eq!(
        response("night", 124, 32),
        compiler_polish::response("night", 124, 32)
    );
    let original_response = compiler_polish::response("day", 124, 32);
    let mut changed = response("day", 124, 32);
    changed["service"]["page_enhancements"][0]["presentation"]["primary"] = json!(8);
    assert_eq!(changed, original_response);
    for (index, original) in flow.accepted.iter().enumerate() {
        compiler_polish::assert_authored(original.service());
        assert!(original.service().compiler_drcs_resources().is_empty());
        assert_base(
            polish_carrier::composition(original.service()),
            if index == 0 { 0x16 } else { 8 },
            b'A',
        );
    }
    compiler_polish_underline_delivery::assert_profiles(&flow.accepted);
}

#[path = "compiler_turkish/oracle.rs"]
mod oracle;

pub(super) fn assert_concept(flow: &Flow, service: &EncodedTeletextAuthoredService) {
    compiler_polish::assert_authored(service);
    let expected = flow.accepted[0]
        .service()
        .declared()
        .clone()
        .with_physical_base_overlays(
            vec![compiler_polish::expected_overlay(flow, 0x23)],
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
    oracle::assert_fallback(flow, service);
    oracle::assert_generated(service);
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

fn actual_sidecar(flow: &Flow, directory: &Path) -> Vec<u8> {
    let bytes = live_client::read_bounded(
        &directory.join("native-original-1/project.ersatzrs.json"),
        policy().project.max_bytes,
    )
    .unwrap();
    assert_eq!(
        bytes,
        write_teletext_export_sidecar(flow.exported[0].sidecar(), policy().project).unwrap(),
        "retained editor project metadata must equal this complete fresh export"
    );
    bytes
}

pub(super) fn actual_request(
    flow: &Flow,
    directory: &Path,
) -> (NativeTeletextEditorialImportRequest, Value) {
    let _ = actual_sidecar(flow, directory);
    compiler_polish::actual_request_for_code(flow, directory, 0x23)
}

pub(super) fn actual_input_hashes(flow: &Flow, directory: &Path) -> Vec<(String, String)> {
    let mut hashes = compiler_polish::actual_input_hashes(flow, directory);
    hashes.push((
        "native-original-1/project.ersatzrs.json".into(),
        live_client::hash(&actual_sidecar(flow, directory)),
    ));
    hashes
}

#[expect(
    clippy::disallowed_methods,
    reason = "Optional explicit create-new genuine-editor fixture destination, matching the existing editor-input writer"
)]
fn write_editor_inputs(flow: &Flow) {
    let Some(directory) = std::env::var_os("ERSATZRS_QTM_TURKISH_DIR") else {
        return;
    };
    let directory = PathBuf::from(directory);
    write_editor_inputs_at(flow, &directory);
}

pub(super) fn write_editor_inputs_at(flow: &Flow, directory: &Path) {
    std::fs::create_dir(directory).expect("Turkish editor destination must be new");
    let original = directory.join("native-original-1");
    std::fs::create_dir(&original).unwrap();
    for file in flow.exported[0].files() {
        live_client::write_new(&original.join(&file.name), &file.bytes).unwrap();
    }
    live_client::write_new(
        &original.join("project.ersatzrs.json"),
        &write_teletext_export_sidecar(flow.exported[0].sidecar(), policy().project).unwrap(),
    )
    .unwrap();
}

#[tokio::test]
async fn no_bank_turkish_concept_previews_publishes_and_reopens() {
    Box::pin(run()).await;
}

async fn run() {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        response,
        Fixture::new_persistent().await,
        channel_documents::policy,
    ))
    .await;
    assert_originals(&flow);
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    write_editor_inputs(&flow);
    let concept = import(&fixture, request(&flow, 1, 7, 0x23)).await;
    assert_eq!(concept.head().id, flow.saved[0].head().id);
    assert_eq!(concept.head().revision, 2);
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    let preview = assert_restored(&fixture, &flow, &concept).await;
    fixture = fixture.reopen().await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert_eq!(assert_restored(&fixture, &flow, &concept).await, preview);
    let day = publish_and_accept(&fixture, &flow, &concept).await;
    assert_concept(&flow, day.service());
    assert_eq!(
        external_editor_probe::preview_page(day.service(), 124),
        preview
    );
    let accepted = [day, recover(&fixture, 2).await];
    compiler_polish_underline_delivery::assert_profiles(&accepted);
    fixture = fixture.reopen().await;
    compiler_polish::assert_pointers(&fixture, &accepted[0], &accepted[1]).await;
    assert_concept(&flow, recover(&fixture, 1).await.service());
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
