//! A destination already selected in Main makes its later original prefix redundant.
use super::*;

fn reference(table: TeletextDrcsTableScope, slot: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::Drcs {
        reference: TeletextDrcsGlyphReference {
            table,
            subtable: TeletextDrcsSubtableIndex::new(0).unwrap(),
            slot: TeletextDrcsGlyphSlot::new(slot).unwrap(),
        },
    }
}
fn response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = free(theme, page, subcode);
    for metadata in value["service"]["page_enhancements"]
        .as_array_mut()
        .unwrap()
    {
        metadata["local_program"]["commands"][5]["operation"] =
            serde_json::to_value(reference(TeletextDrcsTableScope::Global, 1)).unwrap();
    }
    for object in value["service"]["objects"].as_array_mut().unwrap() {
        for (command, slot) in [(2, 0), (3, 1)] {
            object["program"]["commands"][command]["operation"] =
                serde_json::to_value(reference(TeletextDrcsTableScope::Normal, slot)).unwrap();
        }
    }
    value
}
fn change() -> TeletextLocalProgramOperandOverlay {
    TeletextLocalProgramOperandOverlay::new(
        channel_documents::address(124, 32),
        5,
        9,
        reference(TeletextDrcsTableScope::Global, 1),
        reference(TeletextDrcsTableScope::Normal, 1),
    )
    .unwrap()
}
fn request(flow: &Flow) -> NativeTeletextEditorialImportRequest {
    let service = flow.accepted[0].service();
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
    let positions: Vec<_> = program
        .command_positions()
        .iter()
        .filter(|position| position.original_local_command() == Some(5))
        .collect();
    assert_eq!(positions.len(), 1);
    let position = positions[0];
    assert!(position.first_word() < position.semantic_word());
    let targets = [
        (position.first_word(), 0x08628, 0x28628),
        (position.semantic_word(), 0x00b49, 0x20b49),
    ];
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
    let mut seen = [0; 2];
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
    assert_eq!(seen, [1; 2]);
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
    assert_eq!(overlay.local_program_operands(), [change()].as_slice());
    assert!(overlay.pages().is_empty());
    assert!(overlay.local_object_operands().is_empty());
    assert!(overlay.drcs_bitmap_overlays().is_empty());
    let restored = drcs_bitmap::restored(fixture, concept).await;
    let after = restored.encoded().unwrap();
    let before = flow.accepted[0].service();
    let expected = before
        .declared()
        .clone()
        .with_local_program_operand_overlays(&[change()], policy().encoding.validation.service)
        .unwrap();
    assert_eq!(after.declared(), &expected);
    assert_eq!(
        after.declared().semantic().drcs(),
        before.declared().semantic().drcs()
    );
    oracle::assert_fallback_unchanged(before, after);
    let position = after
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page)
                if page.page().address() == channel_documents::address(124, 32) =>
            {
                page.normalized()
                    .encoded()
                    .command_positions()
                    .iter()
                    .find(|position| position.original_local_command() == Some(5))
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(position.first_word(),position.semantic_word(),"destination NormalS0 is already selected: the redundant prefix disappears in native recompile");
    let original = external_editor_probe::preview_page(before, 124);
    let edited = external_editor_probe::preview_page(after, 124);
    assert_ne!(original, edited);
    for (position, (old, new)) in original.pixels().iter().zip(edited.pixels()).enumerate() {
        if old != new {
            assert!(
                (10..20).contains(&(position / original.width()))
                    && (13 * 12..14 * 12).contains(&(position % original.width())),
                "only the selected Main glyph may change"
            );
        }
    }
}
#[tokio::test]
async fn retained_drcs_selection_uses_existing_destination_and_compacts_redundant_prefix() {
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

#[path = "local_destination.rs"]
mod local_destination;
