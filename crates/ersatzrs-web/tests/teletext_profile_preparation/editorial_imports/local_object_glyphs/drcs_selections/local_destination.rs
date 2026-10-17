//! A Local Global-to-Normal reference can reuse its own earlier Normal selector.
use super::*;

fn response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = super::response(theme, page, subcode);
    for metadata in value["service"]["page_enhancements"]
        .as_array_mut()
        .unwrap()
    {
        metadata["local_program"]["commands"][5]["operation"] =
            serde_json::to_value(reference(TeletextDrcsTableScope::Normal, 1)).unwrap();
    }
    for object in value["service"]["objects"].as_array_mut().unwrap() {
        object["program"]["commands"][3]["operation"] =
            serde_json::to_value(reference(TeletextDrcsTableScope::Global, 0)).unwrap();
    }
    value
}
fn change() -> TeletextLocalObjectOperandOverlay {
    TeletextLocalObjectOperandOverlay::new(
        identity(124),
        3,
        1,
        reference(TeletextDrcsTableScope::Global, 0),
        reference(TeletextDrcsTableScope::Normal, 1),
    )
    .unwrap()
}
fn position(service: &EncodedTeletextAuthoredService) -> (usize, usize) {
    let program = service
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page)
                if page.page().address() == channel_documents::address(124, 32) =>
            {
                Some(page.normalized().encoded())
            }
            _ => None,
        })
        .unwrap();
    let mut positions = program.command_positions().iter().filter(|position| {
        position.source().object == Some(identity(124)) && position.source().command == 3
    });
    let position = positions.next().unwrap();
    assert!(positions.next().is_none());
    assert!(position.original_local_command().is_none());
    (position.first_word(), position.semantic_word())
}
fn request(flow: &Flow) -> NativeTeletextEditorialImportRequest {
    let (prefix, glyph) = position(flow.accepted[0].service());
    assert!(prefix < glyph);
    // Only QTM's glyph-scope/PTU word changes. The original Global prefix
    // remains untouched in the upload but becomes unused in this Local body.
    let targets = [(glyph, 0x00341, 0x20b41)];
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    let name = &flow.exported[0]
        .sidecar()
        .project
        .pages
        .iter()
        .find(|page| page.page.page == 124 && page.page.subcode == 32)
        .unwrap()
        .name;
    let file = request
        .files
        .iter_mut()
        .find(|file| &file.name == name)
        .unwrap();
    let original = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
    let mut records = original.records().to_vec();
    let mut seen = [0; 1];
    for record in &mut records {
        if let TeletextTtiRecord::Packet {
            row: 26,
            designation,
            words,
        } = record
        {
            for (target, (position, before, after)) in targets.into_iter().enumerate() {
                if usize::from(*designation) == position / 13 {
                    assert_eq!(words[position % 13], before);
                    words[position % 13] = after;
                    seen[target] += 1;
                }
            }
        }
    }
    assert_eq!(seen, [1; 1]);
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    request
}
async fn assert_concept(fixture: &Fixture, flow: &Flow, concept: &SavedEditorialTeletextDesign) {
    let revision = drcs_bitmap::stored(fixture, concept).await;
    let decoded = decode_teletext_design_documents(&revision.document, designs::budget()).unwrap();
    let overlay = decoded.editorial_overlay().unwrap();
    assert_eq!(overlay.schema_version(), 10);
    assert!(overlay.pages().is_empty());
    assert!(overlay.local_program_operands().is_empty());
    assert!(overlay.drcs_bitmap_overlays().is_empty());
    assert_eq!(overlay.local_object_operands(), [change()].as_slice());
    let restored = drcs_bitmap::restored(fixture, concept).await;
    let before = flow.accepted[0].service();
    let after = restored.encoded().unwrap();
    let expected = before
        .declared()
        .clone()
        .with_local_object_operand_overlays(&[change()], policy().encoding.validation.service)
        .unwrap();
    assert_eq!(after.declared(), &expected);
    assert_eq!(
        before.declared().semantic().drcs(),
        after.declared().semantic().drcs()
    );
    let (prefix, glyph) = position(after);
    assert_eq!(
        prefix, glyph,
        "the Local NormalS0 destination was selected by its preceding original command"
    );
    oracle::assert_fallback_unchanged(before, after);
    let original = external_editor_probe::preview_page(before, 124);
    let edited = external_editor_probe::preview_page(after, 124);
    let mut changes = [false; 2];
    for (offset, (old, new)) in original.pixels().iter().zip(edited.pixels()).enumerate() {
        if old != new {
            let (row, column) = (offset / original.width(), offset % original.width());
            let cell = [45, 47]
                .iter()
                .position(|visual| *visual == column / 12)
                .expect("only both final shared Local glyph consumers may change");
            assert!((10..20).contains(&row));
            changes[cell] = true;
        }
    }
    assert_eq!(changes, [true; 2]);
}
#[tokio::test]
async fn retained_drcs_selection_local_reuses_normal_destination_and_compacts_prefix() {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        response,
        Fixture::new_persistent().await,
        encoding,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let concept = import(&fixture, request(&flow)).await;
    assert_eq!(concept.head().revision, 2);
    assert_concept(&fixture, &flow, &concept).await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    assert_concept(&fixture, &flow, &concept).await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
