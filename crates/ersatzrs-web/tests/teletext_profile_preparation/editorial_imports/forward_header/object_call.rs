//! Generated Header position writers never grant original scalar edit authority.

use super::*;
use ersatzrs_domain::{
    TeletextEnhancementCommand, TeletextLocalObjectOperandOverlay, TeletextObjectId,
    TeletextObjectIdentity, TeletextObjectType,
};
use ersatzrs_ffmpeg::teletext::TeletextMainCompositionSource;

fn identity() -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(
        channel_documents::address(124, 32),
        TeletextObjectId::new(65535),
    )
}
fn response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = super::response(theme, page, subcode);
    let mut objects = Vec::new();
    for page in value["service"]["page_enhancements"]
        .as_array_mut()
        .unwrap()
    {
        let target = json!({"scope":"local","owner":page["address"],"object_id":65535});
        let old = page["local_program"]["commands"].take();
        let mut commands = json!([
            {"kind":"set_row_colour","row":0,"colour":4,"following_rows":false},
            {"kind":"set_position","position":{"row":0,"column":8}},
            {"kind":"invoke_object","target":target,"object_type":"passive","offset":null},
            {"kind":"set_position","position":{"row":0,"column":39}},
            {"kind":"invoke_object","target":target,"object_type":"passive","offset":null}
        ])
        .as_array()
        .unwrap()
        .clone();
        commands.extend(old.as_array().unwrap()[7..].iter().cloned());
        page["local_program"]["commands"] = json!(commands);
        objects.push(
            json!({"identity":target,"object_type":"passive","program":{"commands":[
                {"kind":"at_column","column":0,"operation":{"kind":"foreground","colour":18}},
                {"kind":"at_column","column":0,"operation":{"kind":"background","colour":2}},
                {"kind":"at_column","column":0,"operation":{"kind":"g0","code":65}}
            ]}}),
        );
    }
    assert!(value["service"]["objects"].as_array().unwrap().is_empty());
    value["service"]["objects"] = json!(objects);
    value
}
fn seeds(service: &EncodedTeletextAuthoredService) -> [usize; 2] {
    let original_main = service.declared().semantic().page_enhancements()
        [&channel_documents::address(124, 32)]
        .local_program
        .as_ref()
        .unwrap();
    for ordinal in [2, 4] {
        assert_eq!(
            original_main.commands()[ordinal],
            TeletextEnhancementCommand::InvokeObject {
                target: identity(),
                object_type: TeletextObjectType::Passive,
                offset: None,
            }
        );
    }
    let normalized = display(service).normalized();
    let original = normalized.original();
    let mut found = Vec::new();
    for position in normalized.encoded().command_positions() {
        if position.source().object.is_some() {
            continue;
        }
        let Some(TeletextMainCompositionSource::HeaderPosition { command }) =
            original.provenance().get(position.source().command)
        else {
            continue;
        };
        assert_eq!(position.original_local_command(), None);
        let column = match command {
            1 => 8,
            3 => 39,
            _ => panic!("unexpected Header source ordinal {command}"),
        };
        assert_eq!(
            original.program().commands()[position.source().command],
            TeletextEnhancementCommand::AtColumn {
                column,
                operation: TeletextColumnOperation::Foreground {
                    colour: ersatzrs_domain::TeletextColourReference::new(7).unwrap()
                }
            }
        );
        found.push(position.semantic_word());
    }
    assert_eq!(
        found.len(),
        2,
        "both derived positions must remain non-authored"
    );
    [found[0], found[1]]
}
fn object_glyph(service: &EncodedTeletextAuthoredService) -> usize {
    let mut matching = display(service)
        .normalized()
        .encoded()
        .command_positions()
        .iter()
        .filter(|position| {
            position.source().object == Some(identity()) && position.source().command == 2
        });
    let position = matching.next().unwrap();
    assert!(matching.next().is_none());
    assert_eq!(position.original_local_command(), None);
    position.semantic_word()
}
fn edited(
    flow: &Flow,
    index: usize,
    column: u8,
    mode: u32,
    before: u32,
    after: u32,
) -> NativeTeletextEditorialImportRequest {
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    let sidecar: Value = serde_json::from_slice(&request.sidecar).unwrap();
    let name = sidecar["project"]["pages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|page| page["page"]["page"] == 124)
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
    let mut count = 0;
    for record in &mut records {
        if let TeletextTtiRecord::Packet {
            row: 26,
            designation,
            words,
        } = record
        {
            if usize::from(*designation) == index / 13 {
                assert_eq!(
                    words[index % 13],
                    u32::from(column) | (mode << 6) | (before << 11)
                );
                words[index % 13] = u32::from(column) | (mode << 6) | (after << 11);
                count += 1;
            }
        }
    }
    assert_eq!(count, 1);
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    request
}
async fn assert_rejected_seed(fixture: &Fixture, flow: &Flow, index: usize, column: u8) {
    let error = import_teletext_editorial_design(
        &fixture.state.application,
        edited(flow, index, column, 0, 7, 6),
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(error, NativeTeletextEditorialImportError::Tti(_)),
        "derived Header writer must not become an original operand: {error:?}"
    );
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
    assert_original_pointers(fixture, flow).await;
}
fn overlay() -> TeletextLocalObjectOperandOverlay {
    TeletextLocalObjectOperandOverlay::new(identity(), 2, 0, g0(b'A'), g0(b'B')).unwrap()
}
async fn assert_concept(fixture: &Fixture, flow: &Flow, concept: &SavedEditorialTeletextDesign) {
    let stored = drcs_bitmap::stored(fixture, concept).await;
    let document = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    let overlay = document.editorial_overlay().unwrap();
    assert_eq!(overlay.schema_version(), 4);
    assert!(
        overlay.pages().is_empty()
            && overlay.local_program_operands().is_empty()
            && overlay.drcs_bitmap_overlays().is_empty()
            && overlay.local_object_glyph_program_overlays().is_empty()
    );
    assert_eq!(overlay.local_object_operands(), [self::overlay()]);
    let mut original: Value = serde_json::from_slice(&stored.document).unwrap();
    original
        .as_object_mut()
        .unwrap()
        .remove("editorial_overlay");
    assert_eq!(
        original,
        serde_json::from_slice::<Value>(&flow.originals[0].document).unwrap()
    );
    let current = drcs_bitmap::restored(fixture, concept).await;
    let source = flow.saved[0].candidate();
    assert_eq!(current.selection(), source.selection());
    assert_eq!(current.metadata(), source.metadata());
    assert_eq!(current.transmissions(), source.transmissions());
    assert_eq!(current.sources().len(), source.sources().len());
    for (a, b) in current.sources().iter().zip(source.sources()) {
        assert_eq!(a.bytes(), b.bytes());
        assert_eq!(a.authoring(), b.authoring());
    }
    let expected = flow.accepted[0]
        .service()
        .declared()
        .clone()
        .with_local_object_operand_overlays(
            &[self::overlay()],
            policy().encoding.validation.service,
        )
        .unwrap();
    let after = current.encoded().unwrap();
    assert_eq!(after.declared(), &expected);
    assert_eq!(seeds(after), seeds(flow.accepted[0].service()));
    assert_eq!(
        after.magazine_units(),
        flow.accepted[0].service().magazine_units()
    );
    assert_eq!(after.inventory(), flow.accepted[0].service().inventory());
    assert_eq!(
        after.pages().len(),
        flow.accepted[0].service().pages().len()
    );
    for (a, b) in flow.accepted[0].service().pages().iter().zip(after.pages()) {
        assert_eq!(a.resource_header(), b.resource_header());
        if !matches!(a,EncodedTeletextAuthoredPage::CompiledDisplay(page) if page.page().address()==channel_documents::address(124,32))
        {
            assert_eq!(a.data_units(), b.data_units());
        }
    }
    assert_raster(flow.accepted[0].service(), after);
    assert_theme(after, [15, 0, 0]);
}

#[tokio::test]
async fn header_object_native_seed_protection_shared_scalar_preview_reopen_publication() {
    let (mut fixture, flow) = Box::pin(setup_with_response(response)).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    assert_theme(flow.accepted[0].service(), [15, 0, 0]);
    assert_theme(flow.accepted[1].service(), [0, 0, 15]);
    for (index, column) in seeds(flow.accepted[0].service()).into_iter().zip([8, 39]) {
        Box::pin(assert_rejected_seed(&fixture, &flow, index, column)).await;
        retained.assert_unchanged(&fixture, &flow).await;
    }
    let request = edited(
        &flow,
        object_glyph(flow.accepted[0].service()),
        0,
        9,
        65,
        66,
    );
    let concept = import(&fixture, request).await;
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
    let current = drcs_bitmap::restored(&fixture, &concept).await;
    assert_eq!(
        designs::units(accepted.service()),
        designs::units(current.encoded().unwrap())
    );
    fixture = fixture.reopen().await;
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
