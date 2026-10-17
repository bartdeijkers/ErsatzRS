//! Actual pinned Normal PTU Save As to an unpublished Local schema8 concept.
//! No Global editor, ordinary delivery, client or full resource acceptance claim.
use super::*;
use ersatzrs_domain::{
    TeletextDrcsGlyphReference, TeletextDrcsGlyphSlot, TeletextDrcsSubtableIndex,
    TeletextDrcsTableScope, TeletextEnhancementCommand, TeletextObjectId, TeletextObjectIdentity,
    TeletextObjectOffset, TeletextObjectType,
};
use std::path::{Path, PathBuf};

// Fixed authored bitmaps, independently of received/editor pixels.
const PIXELS: [u16; 10] = [
    0x801, 0x800, 0x400, 0x200, 0x100, 0x080, 0x040, 0x020, 0x010, 0x009,
];
const MIRRORED: [u16; 10] = [
    0x801, 0x001, 0x002, 0x004, 0x008, 0x010, 0x020, 0x040, 0x080, 0x900,
];
fn operation(slot: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::Drcs {
        reference: TeletextDrcsGlyphReference {
            table: TeletextDrcsTableScope::Normal,
            subtable: TeletextDrcsSubtableIndex::new(0).unwrap(),
            slot: TeletextDrcsGlyphSlot::new(slot).unwrap(),
        },
    }
}
fn word(slot: u8) -> u32 {
    (13 << 6) | ((0x40 | u32::from(slot)) << 11)
}
fn change() -> TeletextLocalObjectOperandOverlay {
    TeletextLocalObjectOperandOverlay::new(identity(), 2, 0, operation(0), operation(1)).unwrap()
}
fn identity() -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(
        channel_documents::address(124, 32),
        TeletextObjectId::new(65520),
    )
}
fn response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = free(theme, page, subcode);
    for object in value["service"]["objects"].as_array_mut().unwrap() {
        assert_eq!(object["identity"]["scope"], "local");
        let commands = object["program"]["commands"].as_array_mut().unwrap();
        assert_eq!(commands.len(), 3);
        assert_eq!(
            commands[2]["operation"],
            serde_json::to_value(Family::G0.operation(0)).unwrap()
        );
        commands[2]["operation"] = serde_json::to_value(operation(0)).unwrap();
    }
    let tables = value["service"]["drcs"].as_array_mut().unwrap();
    assert_eq!(tables.len(), 1);
    let glyphs = tables[0]["glyphs"].as_array_mut().unwrap();
    assert_eq!(
        glyphs.as_slice(),
        [json!({"slot":0,"bitmap":PIXELS})].as_slice()
    );
    glyphs.push(json!({"slot":1,"bitmap":MIRRORED}));
    value
}
fn metadata(flow: &Flow) -> Value {
    let owner = channel_documents::address(124, 32);
    let identity = identity();
    let service = flow.accepted[0].service().declared().semantic();
    let definition = service.objects().definition(identity).unwrap();
    assert_eq!(definition.object_type, TeletextObjectType::Passive);
    assert_eq!(definition.program.commands().len(), 3);
    assert_eq!(
        definition.program.commands()[2],
        TeletextEnhancementCommand::AtColumn {
            column: 0,
            operation: operation(0)
        }
    );
    let calls: Vec<_> = service.page_enhancements()[&owner]
        .local_program
        .as_ref()
        .unwrap()
        .commands()
        .iter()
        .filter_map(|command| match command {
            TeletextEnhancementCommand::InvokeObject {
                target,
                object_type,
                offset,
            } if *target == identity => Some((*object_type, *offset)),
            _ => None,
        })
        .collect();
    assert_eq!(
        calls,
        [32, 33].map(|column| (
            TeletextObjectType::Passive,
            Some(TeletextObjectOffset::new(0, column).unwrap())
        ))
    );
    actual_editor::operand_metadata(flow, identity, 2, word(0), word(1), "local_drcs_slot")
}
fn assert_pixels(
    service: &ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService,
    edited: bool,
    foreground: [u8; 3],
) {
    let raster = external_editor_probe::preview_page(service, 124);
    assert_eq!((raster.width(), raster.height()), (672, 250));
    // Main remains on original slot0; only the two Local panel consumers
    // select the separately authored mirrored slot1 from the unchanged bank.
    for column in [12, 44, 45] {
        let mask = if edited && column != 12 {
            MIRRORED
        } else {
            PIXELS
        };
        for (row, bits) in mask.into_iter().enumerate() {
            for x in 0..12 {
                let expected = if bits & (1 << (11 - x)) != 0 {
                    foreground
                } else {
                    [0, 0, 0]
                };
                let Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(pixel))) =
                    raster.pixel(column * 12 + x, 10 + row)
                else {
                    panic!(
                        "original Main and both Local DRCS consumers require settled RGB4 pixels"
                    )
                };
                assert_eq!(
                    pixel.get(),
                    expected,
                    "exact original and mirrored native DRCS masks"
                );
            }
        }
    }
}
async fn assert_concept(fixture: &Fixture, flow: &Flow, concept: &SavedEditorialTeletextDesign) {
    let revision = drcs_bitmap::stored(fixture, concept).await;
    let decoded = decode_teletext_design_documents(&revision.document, designs::budget()).unwrap();
    let overlay = decoded.editorial_overlay().unwrap();
    assert_eq!(overlay.schema_version(), 8);
    assert!(overlay.pages().is_empty());
    assert!(overlay.local_program_operands().is_empty());
    assert!(overlay.drcs_bitmap_overlays().is_empty());
    assert_eq!(overlay.local_object_operands(), [change()].as_slice());
    assert_eq!(
        overlay.provenance().original_digest,
        flow.originals[0].digest
    );
    let mut source: Value = serde_json::from_slice(&revision.document).unwrap();
    source.as_object_mut().unwrap().remove("editorial_overlay");
    assert_eq!(
        source,
        serde_json::from_slice::<Value>(&flow.originals[0].document).unwrap()
    );
    let current = drcs_bitmap::restored(fixture, concept).await;
    let original = flow.saved[0].candidate();
    assert_eq!(current.selection(), original.selection());
    assert_eq!(current.metadata(), original.metadata());
    assert_eq!(current.transmissions(), original.transmissions());
    assert_eq!(current.sources().len(), original.sources().len());
    for (source, expected) in current.sources().iter().zip(original.sources()) {
        assert_eq!(source.bytes(), expected.bytes());
        assert_eq!(source.authoring(), expected.authoring());
    }
    assert_eq!(
        current.producers()[0].original(),
        original.producers()[0].original()
    );
    let before = flow.accepted[0].service();
    let after = current.encoded().unwrap();
    let expected = before
        .declared()
        .clone()
        .with_local_object_operand_overlays(&[change()], policy().encoding.validation.service)
        .unwrap();
    assert_eq!(after.declared(), &expected);
    assert_eq!(
        after.declared().semantic().drcs(),
        before.declared().semantic().drcs()
    );
    super::assert_rendered(before, after, change());
    assert_fallback_unchanged(before, after);
    assert_pixels(before, false, [15, 0, 0]);
    assert_pixels(after, true, [15, 0, 0]);
}
fn assert_fallback_unchanged(
    before: &ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService,
    after: &ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService,
) {
    let [mut original, mut edited] = [before, after].map(|service| {
        let display = service
            .pages()
            .iter()
            .find_map(|page| match page {
                EncodedTeletextAuthoredPage::CompiledDisplay(page)
                    if page.page().address() == channel_documents::address(124, 32) =>
                {
                    Some(page.page())
                }
                _ => None,
            })
            .unwrap();
        project_encoded_teletext_display(display, 128).unwrap()
    });
    for projection in [&mut original, &mut edited] {
        assert!(projection
            .records
            .iter()
            .any(|record| matches!(record, TeletextEncodedDisplayRecord::BaseRow { .. })));
        projection.records.retain(|record| {
            !matches!(
                record,
                TeletextEncodedDisplayRecord::Triplets { row: 26, .. }
            )
        });
    }
    assert_eq!(
        original, edited,
        "all clean header controls, fallback and non-X26 records stay exact"
    );
}

async fn run_case(directory: &Path) {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        response,
        Fixture::new_persistent().await,
        drcs_companion::encoding_policy,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    assert_pixels(flow.accepted[0].service(), false, [15, 0, 0]);
    assert_pixels(flow.accepted[1].service(), false, [0, 0, 15]);
    let ready =
        actual_editor::inputs_for_edit(directory, "normal-slot-free", &flow, || metadata(&flow));
    let marker = actual_editor::completion(directory).await;
    let saved =
        live_client::read_bounded(&directory.join("edited.tti"), policy().tti.max_bytes).unwrap();
    let semantic_word = usize::try_from(ready["edit"]["semantic_word"].as_u64().unwrap()).unwrap();
    assert_eq!(actual_editor::word_at(&saved, semantic_word), word(1));
    actual_editor::assert_saved_packet_words(&flow, &saved, word(0), word(1));
    let mut request = actual_editor::actual_request(directory, &flow, &saved);
    "Actual pinned QTM Local Normal DRCS slot concept".clone_into(&mut request.name);
    let concept = import(&fixture, request).await;
    assert_eq!(
        (concept.head().id, concept.head().revision),
        (flow.saved[0].head().id, 2)
    );
    assert_concept(&fixture, &flow, &concept).await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    assert_concept(&fixture, &flow, &concept).await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    let revision = drcs_bitmap::stored(&fixture, &concept).await;
    actual_editor::json_new(
        directory,
        "success.json",
        &json!({"probe_version":1,"case":"normal-slot-free","editor":marker,
        "receipt_id":flow.receipts[0].metadata.id,"receipt_digest":flow.receipts[0].metadata.digest,
        "original_digest":flow.originals[0].digest,"design_id":concept.head().id,"revision":2,"digest":concept.digest(),
        "concept_document_sha256":drcs_bitmap::hash(&revision.document),"saved_file":"edited.tti","saved_sha256":drcs_bitmap::hash(&saved),
        "schema_version":8,"edit":ready["edit"],"native_preview_geometry":[672,250],"exact_native_mirrored_masks_and_both_local_consumers":true,"main_original_slot0_unchanged":true,
        "headers_fallback_non_x26_and_resources_unchanged":true,"reopen_verified":true,
        "original_sources_package_profiles_config_night_unchanged":true,"publication":"unchanged",
        "global_editor_ordinary_stream_client_and_full_acceptance":"not measured by this test"}),
    );
    fixture.state.pool.close().await;
}
#[tokio::test]
#[ignore = "requires fresh pinned headed QTM1.0 PTU Save As via ERSATZRS_TELETEXT_DRCS_SLOT_EDITOR_DIR"]
async fn actual_qtm_local_object_normal_drcs_slot_save_as_import_v8_preview_reopen() {
    let root = PathBuf::from(super::super::super::client_capture::selected(
        "ERSATZRS_TELETEXT_DRCS_SLOT_EDITOR_DIR",
    ));
    std::fs::create_dir(&root).expect("actual DRCS slot proof root must be create-new");
    Box::pin(run_case(&root.join("normal-slot-free"))).await;
}
