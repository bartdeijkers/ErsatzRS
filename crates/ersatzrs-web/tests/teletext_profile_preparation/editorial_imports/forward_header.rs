//! Native forward-header scalar edits; no new external-editor/client claim.

use super::*;
use ersatzrs_app::teletext_design_documents::decode_teletext_design_documents;
use ersatzrs_domain::{
    TeletextColumnOperation, TeletextGlyphCode, TeletextLocalProgramOperandOverlay,
};
use ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService;
use ersatzrs_web::host::teletext_editorial_designs::{
    NativeTeletextEditorialImportError, TeletextEditorialEffectsFailure,
};
use serde_json::{json, Value};

#[path = "forward_header/object_call.rs"]
mod object_call;

const GLYPHS: [(usize, u8); 2] = [(4, 8), (6, 39)];

fn response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = external_editor_probe::editor_response(theme, page, subcode);
    for page in value["service"]["page_enhancements"]
        .as_array_mut()
        .unwrap()
    {
        let original = page["local_program"]["commands"].take();
        let mut commands = json!([
            {"kind":"set_row_colour","row":0,"colour":4,"following_rows":false},
            {"kind":"set_position","position":{"row":0,"column":8}},
            {"kind":"at_column","column":8,"operation":{"kind":"foreground","colour":18}},
            {"kind":"at_column","column":8,"operation":{"kind":"background","colour":2}},
            {"kind":"at_column","column":8,"operation":{"kind":"g0","code":65}},
            {"kind":"set_position","position":{"row":0,"column":39}},
            {"kind":"at_column","column":39,"operation":{"kind":"g0","code":65}}
        ])
        .as_array()
        .unwrap()
        .clone();
        commands.extend(original.as_array().unwrap().iter().cloned());
        page["local_program"]["commands"] = json!(commands);
    }
    for page in value["authoring"]["pages"].as_array_mut().unwrap() {
        page["regions"].as_array_mut().unwrap().extend([
            json!({"id":"free-header","kind":"free","area":"main","row":0,"column":8,"width":1,"height":1}),
            json!({"id":"zone-header","kind":"zone","area":"main","row":0,"column":39,"width":1,"height":1}),
        ]);
    }
    value
}
fn protected(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = response(theme, page, subcode);
    for page in value["authoring"]["pages"].as_array_mut().unwrap() {
        page["regions"]
            .as_array_mut()
            .unwrap()
            .retain(|region| region["id"] != "zone-header");
    }
    value
}
fn g0(code: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::G0 {
        code: TeletextGlyphCode::new(code).unwrap(),
    }
}
fn overlays() -> Vec<TeletextLocalProgramOperandOverlay> {
    GLYPHS
        .map(|(command, column)| {
            TeletextLocalProgramOperandOverlay::new(
                channel_documents::address(124, 32),
                command,
                column,
                g0(b'A'),
                g0(b'B'),
            )
            .unwrap()
        })
        .into()
}
fn display(
    service: &EncodedTeletextAuthoredService,
) -> &ersatzrs_ffmpeg::teletext::EncodedTeletextCompiledDisplayPage {
    service
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page)
                if page.page().address() == channel_documents::address(124, 32) =>
            {
                Some(page.as_ref())
            }
            _ => None,
        })
        .unwrap()
}
fn positions(service: &EncodedTeletextAuthoredService) -> [usize; 2] {
    let encoded = display(service).normalized().encoded();
    for ordinal in [1, 5] {
        assert!(
            !encoded
                .command_positions()
                .iter()
                .any(|position| position.original_local_command() == Some(ordinal)),
            "forward Header SetPosition is authenticated but emits no physical word"
        );
    }
    GLYPHS.map(|(command, _)| {
        let mut positions = encoded
            .command_positions()
            .iter()
            .filter(|position| position.original_local_command() == Some(command));
        let position = positions.next().unwrap();
        assert!(positions.next().is_none());
        position.semantic_word()
    })
}
fn request(flow: &Flow) -> NativeTeletextEditorialImportRequest {
    let indexes = positions(flow.accepted[0].service());
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    let sidecar: Value = serde_json::from_slice(&request.sidecar).unwrap();
    let name = sidecar["project"]["pages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|mapping| mapping["page"]["page"] == 124)
        .unwrap()["name"]
        .as_str()
        .unwrap();
    let file = request
        .files
        .iter_mut()
        .find(|file| file.name == name)
        .unwrap();
    let mut records = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    let mut changed = 0;
    for record in &mut records {
        if let TeletextTtiRecord::Packet {
            row: 26,
            designation,
            words,
        } = record
        {
            for (index, (_, column)) in indexes.into_iter().zip(GLYPHS) {
                if usize::from(*designation) == index / 13 {
                    let original = words[index % 13];
                    assert_eq!(original & 63, u32::from(column));
                    assert_eq!(original >> 11, 65);
                    assert_eq!((original >> 6) & 31, 9);
                    words[index % 13] = (original & 0x7ff) | (66 << 11);
                    changed += 1;
                }
            }
        }
    }
    assert_eq!(changed, 2);
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    request
}
fn assert_raster(before: &EncodedTeletextAuthoredService, after: &EncodedTeletextAuthoredService) {
    let old = external_editor_probe::preview_page(before, 124);
    let new = external_editor_probe::preview_page(after, 124);
    assert_eq!((old.width(), old.height()), (672, 250));
    assert_eq!((new.width(), new.height()), (672, 250));
    let mut changed = [false; 2];
    for (index, (a, b)) in old.pixels().iter().zip(new.pixels()).enumerate() {
        if a == b {
            continue;
        }
        let (row, column) = (index / new.width(), index % new.width());
        assert!(row < 10, "only Header glyph pixels may change");
        let cell = column / 12;
        let target = match cell {
            12 => 0,
            43 => 1,
            _ => panic!("protected Header/body/panel changed at {row},{column}"),
        };
        changed[target] = true;
    }
    assert_eq!(changed, [true, true]);
}
fn assert_theme(service: &EncodedTeletextAuthoredService, colour: [u8; 3]) {
    let raster = external_editor_probe::preview_page(service, 124);
    for column in [12, 43] {
        for expected in [colour, [0, 15, 0]] {
            assert!((column*12..(column+1)*12).any(|x| (0..10).any(|y| matches!(raster.pixel(x,y),
                Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(rgb))) if rgb.get()==expected))));
        }
    }
}
async fn assert_concept(fixture: &Fixture, flow: &Flow, concept: &SavedEditorialTeletextDesign) {
    let stored = drcs_bitmap::stored(fixture, concept).await;
    let document = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    let overlay = document.editorial_overlay().unwrap();
    assert_eq!(overlay.schema_version(), 4);
    assert!(
        overlay.pages().is_empty()
            && overlay.local_object_operands().is_empty()
            && overlay.drcs_bitmap_overlays().is_empty()
            && overlay.local_object_glyph_program_overlays().is_empty()
    );
    assert_eq!(overlay.local_program_operands(), overlays());
    let restored = drcs_bitmap::restored(fixture, concept).await;
    let source = flow.saved[0].candidate();
    assert_eq!(restored.selection(), source.selection());
    assert_eq!(restored.metadata(), source.metadata());
    assert_eq!(restored.transmissions(), source.transmissions());
    assert_eq!(restored.sources().len(), source.sources().len());
    for (new, old) in restored.sources().iter().zip(source.sources()) {
        assert_eq!(new.bytes(), old.bytes());
        assert_eq!(new.authoring(), old.authoring());
    }
    let expected = flow.accepted[0]
        .service()
        .declared()
        .clone()
        .with_local_program_operand_overlays(&overlays(), policy().encoding.validation.service)
        .unwrap();
    let encoded = restored.encoded().unwrap();
    assert_eq!(encoded.declared(), &expected);
    assert_eq!(
        encoded.magazine_units(),
        flow.accepted[0].service().magazine_units()
    );
    assert_eq!(encoded.inventory(), flow.accepted[0].service().inventory());
    for (old, new) in flow.accepted[0]
        .service()
        .pages()
        .iter()
        .zip(encoded.pages())
    {
        assert_eq!(old.resource_header(), new.resource_header());
        if !matches!(old,EncodedTeletextAuthoredPage::CompiledDisplay(page) if page.page().address()==channel_documents::address(124,32))
        {
            assert_eq!(old.data_units(), new.data_units());
        }
    }
    assert_raster(flow.accepted[0].service(), encoded);
    assert_theme(encoded, [15, 0, 0]);
}

#[tokio::test]
async fn forward_header_free_and_zone_scalars_import_preview_reopen_and_publish() {
    let (mut fixture, flow) = Box::pin(setup_with_response(response)).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    assert_theme(flow.accepted[0].service(), [15, 0, 0]);
    assert_theme(flow.accepted[1].service(), [0, 0, 15]);
    let concept = import(&fixture, request(&flow)).await;
    assert_eq!(
        (concept.head().id, concept.head().revision),
        (flow.saved[0].head().id, 2)
    );
    Box::pin(assert_concept(&fixture, &flow, &concept)).await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    Box::pin(assert_concept(&fixture, &flow, &concept)).await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    let accepted = publish_and_accept(&fixture, &flow, &concept).await;
    let restored = drcs_bitmap::restored(&fixture, &concept).await;
    assert_eq!(
        designs::units(accepted.service()),
        designs::units(restored.encoded().unwrap())
    );
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn forward_header_protected_last_column_rejects_atomically() {
    let (mut fixture, flow) = Box::pin(setup_with_response(protected)).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let error = import_teletext_editorial_design(
        &fixture.state.application,
        request(&flow),
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap_err();
    let NativeTeletextEditorialImportError::Effects(error) = error else {
        panic!("protected Header edit must have a located effects refusal");
    };
    assert_eq!(
        error.reason,
        TeletextEditorialEffectsFailure::ProtectedDisplay
    );
    assert_eq!(error.page, Some(channel_documents::address(124, 32)));
    assert_eq!(error.position, Some((0, 43)));
    assert_eq!(
        fixture
            .state
            .application
            .teletext_design_head(flow.saved[0].head().id)
            .await
            .unwrap()
            .unwrap()
            .revision,
        1
    );
    assert!(fixture
        .state
        .application
        .teletext_design_revision(
            flow.saved[0].head().id,
            2,
            designs::budget().max_serialized_bytes
        )
        .await
        .unwrap()
        .is_none());
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
