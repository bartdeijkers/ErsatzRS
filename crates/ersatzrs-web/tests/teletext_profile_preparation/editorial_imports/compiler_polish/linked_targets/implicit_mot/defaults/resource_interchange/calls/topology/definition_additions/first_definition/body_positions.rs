//! Runtime acceptance for a first row-position edit in a genuinely empty addition.
//! Expected native bodies come from independent installed-producer originals.
use super::*;
use ersatzrs_domain::{
    TeletextColumnOperation, TeletextEnhancementCommand, TeletextEnhancementPosition,
    TeletextGlyphCode,
};

const POSITION: u32 = 0x29 | (4 << 6);
const TERMINATION: u32 = 0x2fff;

#[path = "body_positions/actual_editor.rs"]
mod actual_editor;

#[path = "body_positions/forward_boundaries.rs"]
mod forward_boundaries;

#[path = "body_positions/extended_body.rs"]
mod extended_body;

fn glyph(code: u8) -> u32 {
    8 | (9 << 6) | (u32::from(code) << 11)
}
fn body_response(theme: &str, page: u16, subcode: u16, code: u8) -> Value {
    let mut value = with_definitions(theme, page, subcode, 1);
    value["service"]["objects"][0]["program"]["commands"] = json!([
        {"kind":"set_position","position":{"row":1,"column":0}},
        {"kind":"at_column","column":8,"operation":{"kind":"g0","code":code}}
    ]);
    validate_response(&value);
    value
}
fn body_p(theme: &str, page: u16, subcode: u16) -> Value {
    body_response(theme, page, subcode, b'P')
}
fn body_q(theme: &str, page: u16, subcode: u16) -> Value {
    body_response(theme, page, subcode, b'Q')
}
fn object_name(exported: &ExportedOriginalTeletextDesign) -> &str {
    &exported
        .sidecar()
        .project
        .assets
        .iter()
        .find(|asset| {
            matches!(
                asset.scope,
                TeletextExportAssetScope::ObjectTtiV1 { object_id: 25, .. }
            )
        })
        .expect("specific single-object V6 companion")
        .name
}
struct BodyExpected {
    definition: TeletextEnhancementObjectDefinition,
    records: Vec<TeletextTtiRecord>,
    units: Vec<Vec<[u8; 46]>>,
}
async fn qualify_body(code: u8) -> BodyExpected {
    let response: fn(&str, u16, u16) -> Value = if code == b'P' { body_p } else { body_q };
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        response,
        Fixture::new_persistent().await,
        encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let service = flow.accepted[0].service();
    let definition = service
        .declared()
        .semantic()
        .objects()
        .definition(identity(0))
        .unwrap()
        .clone();
    assert_eq!(
        definition.program.commands(),
        [
            TeletextEnhancementCommand::SetPosition {
                position: TeletextEnhancementPosition::new(1, 0).unwrap()
            },
            TeletextEnhancementCommand::AtColumn {
                column: 8,
                operation: TeletextColumnOperation::G0 {
                    code: TeletextGlyphCode::new(code).unwrap()
                }
            },
        ]
    );
    let records = file_records(&flow.exported[0], object_name(&flow.exported[0]));
    assert_eq!(
        [word(&records, 3), word(&records, 4), word(&records, 5)],
        [POSITION, glyph(code), TERMINATION]
    );
    let result = BodyExpected {
        definition,
        records,
        units: service
            .pages()
            .iter()
            .map(|page| page.data_units().to_vec())
            .collect(),
    };
    fixture.state.pool.close().await;
    result
}
async fn export_stored(
    fixture: &Fixture,
    stored: &TeletextDesignRevision,
) -> ExportedOriginalTeletextDesign {
    let mutation = export_original_teletext_design(
        &fixture.state.application,
        NativeTeletextExportRequest {
            design_id: stored.design_id,
            revision: stored.revision,
            design_digest: stored.digest.clone(),
            expected_design_head: stored.revision,
            pages: None,
        },
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextDesignMutation::Saved(exported) = mutation else {
        panic!("current body concept must export")
    };
    assert_eq!(
        exported.sidecar().project.editor_profile,
        "qteletextmaker-1.0-carrier-v6"
    );
    exported
}
fn edited_request(
    exported: &ExportedOriginalTeletextDesign,
    receipt: &TeletextDesignExportReceipt,
    replacements: &[(usize, u32)],
) -> NativeTeletextEditorialImportRequest {
    let mut request = drcs_bitmap::from_receipt(receipt, receipt.metadata.revision);
    let originals = request.files.clone();
    let name = object_name(exported);
    let file = request
        .files
        .iter_mut()
        .find(|file| file.name == name)
        .unwrap();
    let mut records = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    for &(index, value) in replacements {
        let mut changed = false;
        for record in &mut records {
            if let TeletextTtiRecord::Packet {
                row: 26,
                designation,
                words,
            } = record
            {
                if usize::from(*designation) == index / 13 {
                    words[index % 13] = value;
                    changed = true;
                }
            }
        }
        assert!(changed, "edit an actual existing companion packet");
    }
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    for original in originals.iter().filter(|file| file.name != name) {
        assert_eq!(
            request
                .files
                .iter()
                .find(|file| file.name == original.name)
                .unwrap(),
            original
        );
    }
    request
}
async fn assert_body(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    expected: &BodyExpected,
) -> TeletextDesignRevision {
    let stored = drcs_bitmap::stored(fixture, concept).await;
    let document = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    let overlay = document.editorial_overlay().unwrap();
    assert_eq!(overlay.schema_version(), 18);
    let [addition] = overlay.resource_object_definition_additions() else {
        panic!("one folded complete V18 addition")
    };
    assert_eq!(addition.definition(), &expected.definition);
    assert!(overlay.resource_object_program_overlays().is_empty());
    assert!(overlay.resource_object_operands().is_empty());
    assert!(
        overlay.pages().is_empty()
            && overlay.local_program_operands().is_empty()
            && overlay.local_object_operands().is_empty()
            && overlay.local_object_glyph_program_overlays().is_empty()
            && overlay.drcs_bitmap_overlays().is_empty()
    );
    let restored = reload_original_teletext_design_revision(
        &fixture.state.application,
        stored.design_id,
        stored.revision,
        designs::budget(),
        encoding(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    let original = flow.saved[0].candidate();
    assert_eq!(restored.selection(), original.selection());
    assert_eq!(restored.metadata(), original.metadata());
    assert_eq!(restored.transmissions(), original.transmissions());
    assert_eq!(restored.sources().len(), original.sources().len());
    for (after, before) in restored.sources().iter().zip(original.sources()) {
        assert_eq!(after.bytes(), before.bytes());
        assert_eq!(after.authoring(), before.authoring());
    }
    assert_eq!(
        restored.producers()[0].original(),
        original.producers()[0].original()
    );
    let service = restored.encoded().unwrap();
    assert_eq!(service.declared().resource_inventory(), resources());
    assert_eq!(
        service
            .pages()
            .iter()
            .map(|page| page.data_units().to_vec())
            .collect::<Vec<_>>(),
        expected.units
    );
    assert_eq!(
        generations::preview(service).raster(),
        generations::preview(flow.accepted[0].service()).raster(),
        "editing an uncalled definition changes no display"
    );
    stored
}
async fn no_revision_three(fixture: &Fixture, flow: &Flow, before: &Retained) {
    stored(fixture, flow, 1).await;
    unchanged(fixture, flow, before).await;
}
async fn reject_invalid_positions(
    fixture: &Fixture,
    flow: &Flow,
    before: &Retained,
    exported: &ExportedOriginalTeletextDesign,
    receipt: &TeletextDesignExportReceipt,
) {
    for position in [0x27 | (4 << 6), 0x29 | (4 << 6) | (40 << 11)] {
        let request = edited_request(exported, receipt, &[(3, position), (4, glyph(b'P'))]);
        let result = import_teletext_editorial_design(
            &fixture.state.application,
            request,
            policy(),
            designs::deadline(),
        )
        .await;
        assert!(
            result.is_err(),
            "invalid AP row-group/column cannot save a native body"
        );
        no_revision_three(fixture, flow, before).await;
    }
    let mut request = edited_request(exported, receipt, &[(3, POSITION), (4, glyph(b'P'))]);
    let resource = exported
        .sidecar()
        .project
        .assets
        .iter()
        .find(|asset| matches!(asset.scope, TeletextExportAssetScope::Resource { .. }))
        .unwrap();
    request
        .files
        .iter_mut()
        .find(|file| file.name == resource.name)
        .unwrap()
        .bytes[0] ^= 1;
    assert!(import_teletext_editorial_design(
        &fixture.state.application,
        request,
        policy(),
        designs::deadline()
    )
    .await
    .is_err());
    no_revision_three(fixture, flow, before).await;
}
async fn repeat_and_publish(
    mut fixture: Fixture,
    flow: &Flow,
    before: &Retained,
    first: &SavedEditorialTeletextDesign,
    p: &BodyExpected,
    q: &BodyExpected,
) {
    fixture = fixture.reopen().await;
    let stored = Box::pin(assert_body(&fixture, flow, first, p)).await;
    unchanged(&fixture, flow, before).await;
    let exported = Box::pin(export_stored(&fixture, &stored)).await;
    assert_eq!(file_records(&exported, object_name(&exported)), p.records);
    let latest = receipt(&fixture, &exported).await;
    let repeat = edited_request(&exported, &latest, &[(4, glyph(b'Q'))]);
    let repeated = import(&fixture, repeat).await;
    assert_eq!(repeated.head().revision, 4);
    let stored = Box::pin(assert_body(&fixture, flow, &repeated, q)).await;
    unchanged(&fixture, flow, before).await;
    let exported = Box::pin(export_stored(&fixture, &stored)).await;
    assert_eq!(file_records(&exported, object_name(&exported)), q.records);
    fixture = fixture.reopen().await;
    Box::pin(assert_body(&fixture, flow, &repeated, q)).await;
    unchanged(&fixture, flow, before).await;
    let accepted = publish_and_accept(&fixture, flow, &repeated).await;
    assert_eq!(
        accepted
            .service()
            .pages()
            .iter()
            .map(|page| page.data_units().to_vec())
            .collect::<Vec<_>>(),
        q.units
    );
    assert_eq!(
        fixture
            .state
            .application
            .accepted_teletext_generation(1)
            .await
            .unwrap(),
        Some(accepted.generation().clone())
    );
    generations::assert_same_output_width(&recover(&fixture, 2).await, &flow.accepted[1], 40 * 12);
    assert_retained_storage(&fixture, flow).await;
    assert_eq!(retained(&fixture).await, *before);
    assert!(fixture.playout.is_empty());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn first_empty_definition_body_position_import_repeat_reopen_and_explicit_publication() {
    let empty = Box::pin(qualify_empty(1)).await;
    let p = Box::pin(qualify_body(b'P')).await;
    let q = Box::pin(qualify_body(b'Q')).await;
    let fixture = Fixture::new_persistent().await;
    http_routes::configure_authoring_policy(&fixture).await;
    let (fixture, flow) = Box::pin(setup_empty(fixture)).await;
    let before = retained(&fixture).await;
    assert_eq!(
        submit(&fixture, &fields(&flow.originals[0], 0)).await["saved"],
        "draft"
    );
    let added = Box::pin(assert_concept(&fixture, &flow, &empty)).await;
    let exported = Box::pin(export_stored(&fixture, &added)).await;
    let baseline = receipt(&fixture, &exported).await;
    let records = file_records(&exported, object_name(&exported));
    assert_eq!(
        [word(&records, 3), word(&records, 4), word(&records, 5)],
        [TERMINATION; 3]
    );
    Box::pin(reject_invalid_positions(
        &fixture, &flow, &before, &exported, &baseline,
    ))
    .await;
    let request = edited_request(&exported, &baseline, &[(3, POSITION), (4, glyph(b'P'))]);
    let actual = request
        .files
        .iter()
        .find(|file| file.name == object_name(&exported))
        .unwrap();
    assert_eq!(
        parse_teletext_tti(&actual.bytes, policy().tti)
            .unwrap()
            .records(),
        p.records,
        "the actual checked TTI edit equals the entire independent producer companion"
    );
    // This is the intended runtime RED until the compiler can qualify the new
    // row-position command for this first complete V18-added body.
    let concept = import(&fixture, request).await;
    assert_eq!(concept.head().revision, 3);
    Box::pin(assert_body(&fixture, &flow, &concept, &p)).await;
    unchanged(&fixture, &flow, &before).await;
    Box::pin(repeat_and_publish(
        fixture, &flow, &before, &concept, &p, &q,
    ))
    .await;
}
