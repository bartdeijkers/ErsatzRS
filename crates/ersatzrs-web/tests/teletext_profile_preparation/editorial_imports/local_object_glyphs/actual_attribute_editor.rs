//! Actual pinned foreground Save As to an unpublished Local schema5 concept.
//! Native raster pixels are checked; no actual widget-font equivalence is claimed.
use super::*;
use ersatzrs_domain::{TeletextColourReference, TeletextObjectId, TeletextObjectIdentity};
use std::path::{Path, PathBuf};

const FROM: u8 = 18;
const TO: u8 = 2;
const LATIN_A: [u16; 10] = [0, 0x0f0, 0x198, 0x30c, 0x606, 0x7fe, 0x606, 0x606, 0, 0];

fn foreground(colour: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::Foreground {
        colour: TeletextColourReference::new(colour).unwrap(),
    }
}
fn change() -> TeletextLocalObjectOperandOverlay {
    TeletextLocalObjectOperandOverlay::new(
        TeletextObjectIdentity::local(
            channel_documents::address(124, 32),
            TeletextObjectId::new(65520),
        ),
        0,
        0,
        foreground(FROM),
        foreground(TO),
    )
    .unwrap()
}

fn metadata(flow: &Flow) -> Value {
    let identity = actual_editor::original_identity(flow, Family::G0);
    let definition = flow.accepted[0]
        .service()
        .declared()
        .semantic()
        .objects()
        .definition(identity)
        .unwrap();
    assert_eq!(
        definition.program.commands()[0],
        ersatzrs_domain::TeletextEnhancementCommand::AtColumn {
            column: 0,
            operation: foreground(FROM)
        }
    );
    actual_editor::operand_metadata(
        flow,
        identity,
        0,
        u32::from(FROM) << 11,
        u32::from(TO) << 11,
        "local_foreground",
    )
}

fn assert_pixels(
    service: &ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService,
    colour: [u8; 3],
) {
    let raster = external_editor_probe::preview_page(service, 124);
    assert_eq!((raster.width(), raster.height()), (672, 250));
    // Fixed original Latin A stays unchanged. Fg18 uses the Day CLUT2 red;
    // Fg2 uses standard CLUT0 green under unchanged fg_clut0_bg_clut0 remapping.
    for column in [44, 45] {
        for (row, bits) in LATIN_A.iter().enumerate() {
            for x in 0..12 {
                let expected = if bits & (1 << (11 - x)) != 0 {
                    colour
                } else {
                    [0, 0, 0]
                };
                let Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(pixel))) =
                    raster.pixel(column * 12 + x, 10 + row)
                else {
                    panic!("both shared Local consumers require settled RGB4 pixels")
                };
                assert_eq!(
                    pixel.get(),
                    expected,
                    "exact original A mask, foreground and black background"
                );
            }
        }
    }
}

async fn assert_concept(fixture: &Fixture, flow: &Flow, concept: &SavedEditorialTeletextDesign) {
    let revision = drcs_bitmap::stored(fixture, concept).await;
    let decoded = decode_teletext_design_documents(&revision.document, designs::budget()).unwrap();
    let overlay = decoded.editorial_overlay().unwrap();
    assert_eq!(overlay.schema_version(), 5);
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
    super::assert_rendered(before, after, change());
    assert_pixels(before, [15, 0, 0]);
    assert_pixels(after, [0, 15, 0]);
}

async fn run_case(directory: &Path) {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        free,
        Fixture::new_persistent().await,
        drcs_companion::encoding_policy,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    assert_pixels(flow.accepted[0].service(), [15, 0, 0]);
    let ready =
        actual_editor::inputs_for_edit(directory, "foreground-free", &flow, || metadata(&flow));
    let marker = actual_editor::completion(directory).await;
    let saved =
        live_client::read_bounded(&directory.join("edited.tti"), policy().tti.max_bytes).unwrap();
    let word = usize::try_from(ready["edit"]["semantic_word"].as_u64().unwrap()).unwrap();
    assert_eq!(actual_editor::word_at(&saved, word), u32::from(TO) << 11);
    actual_editor::assert_saved_packet_words(
        &flow,
        &saved,
        u32::from(FROM) << 11,
        u32::from(TO) << 11,
    );
    let mut request = actual_editor::actual_request(directory, &flow, &saved);
    "Actual pinned QTM Local foreground concept".clone_into(&mut request.name);
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
        &json!({"probe_version":1,"case":"foreground-free","editor":marker,
        "receipt_id":flow.receipts[0].metadata.id,"receipt_digest":flow.receipts[0].metadata.digest,
        "original_digest":flow.originals[0].digest,"design_id":concept.head().id,"revision":2,"digest":concept.digest(),
        "concept_document_sha256":drcs_bitmap::hash(&revision.document),"saved_file":"edited.tti","saved_sha256":drcs_bitmap::hash(&saved),
        "schema_version":5,"edit":ready["edit"],"native_preview_geometry":[672,250],"exact_native_A_masks_and_two_red_to_green_consumers":true,
        "actual_widget_font_equivalence":"not asserted","reopen_verified":true,
        "original_sources_package_profiles_config_night_unchanged":true,"publication":"unchanged",
        "ordinary_stream_client_and_full_acceptance":"not measured by this test"}),
    );
    fixture.state.pool.close().await;
}

#[tokio::test]
#[ignore = "requires fresh pinned headed QTM1.0 foreground Save As via ERSATZRS_TELETEXT_OBJECT_ATTRIBUTE_EDITOR_DIR"]
async fn actual_qtm_local_object_foreground_save_as_import_v5_preview_reopen() {
    let root = PathBuf::from(super::super::super::client_capture::selected(
        "ERSATZRS_TELETEXT_OBJECT_ATTRIBUTE_EDITOR_DIR",
    ));
    std::fs::create_dir(&root).expect("actual foreground proof root must be create-new");
    Box::pin(run_case(&root.join("foreground-free"))).await;
}
