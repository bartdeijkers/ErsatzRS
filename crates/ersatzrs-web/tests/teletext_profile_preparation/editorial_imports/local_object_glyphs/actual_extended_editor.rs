//! Actual pinned popup/menu Save As to unpublished Local schema6 concepts.
//! Fixed native masks are checked; actual widget-font equivalence is not asserted.
use super::*;
use ersatzrs_domain::{
    TeletextDiacriticIndex, TeletextEnhancementCommand, TeletextObjectId, TeletextObjectIdentity,
    TeletextObjectOffset, TeletextObjectType,
};
use std::path::{Path, PathBuf};

// Literal fixed-font masks from domain glyph_mask_tests.rs, not captured output.
const POUND: [u16; 10] = [0, 0x0fc, 0x186, 0x0c0, 0x3f0, 0x0c0, 0x3e6, 0x33c, 0, 0];
const ACUTE: [u16; 10] = [0x060, 0x0c0, 0, 0, 0, 0, 0, 0, 0, 0];
const A_MARK9: [u16; 10] = [0, 0x0f0, 0x198, 0x30c, 0x606, 0x7fe, 0x606, 0x606, 0, 0x060];
const A_MARK12: [u16; 10] = [0, 0x0f0, 0x198, 0x30c, 0x606, 0x7fe, 0x606, 0x606, 0, 0x7fe];

#[derive(Clone, Copy)]
enum Case {
    G2,
    Diacritic,
}
impl Case {
    fn name(self) -> &'static str {
        match self {
            Self::G2 => "g2-free",
            Self::Diacritic => "diacritic-zone",
        }
    }
    fn operation(self, edited: bool) -> TeletextColumnOperation {
        match self {
            Self::G2 => TeletextColumnOperation::G2 {
                code: TeletextGlyphCode::new(if edited { 66 } else { 35 }).unwrap(),
            },
            Self::Diacritic => TeletextColumnOperation::G0Diacritic {
                code: TeletextGlyphCode::new(b'A').unwrap(),
                diacritic: TeletextDiacriticIndex::new(if edited { 12 } else { 9 }).unwrap(),
            },
        }
    }
    fn word(self, edited: bool) -> u32 {
        match (self, edited) {
            (Self::G2, false) => 0x11bc0,
            (Self::G2, true) => 0x213c0,
            (Self::Diacritic, false) => 0x20e40,
            (Self::Diacritic, true) => 0x20f00,
        }
    }
    fn mask(self, edited: bool) -> [u16; 10] {
        match (self, edited) {
            (Self::G2, false) => POUND,
            (Self::G2, true) => ACUTE,
            (Self::Diacritic, false) => A_MARK9,
            (Self::Diacritic, true) => A_MARK12,
        }
    }
    fn change(self) -> TeletextLocalObjectOperandOverlay {
        TeletextLocalObjectOperandOverlay::new(
            identity(),
            2,
            0,
            self.operation(false),
            self.operation(true),
        )
        .unwrap()
    }
    fn factory(self) -> fn(&str, u16, u16) -> Value {
        match self {
            Self::G2 => g2_free,
            Self::Diacritic => diacritic_zone,
        }
    }
}
fn identity() -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(
        channel_documents::address(124, 32),
        TeletextObjectId::new(65520),
    )
}
fn response(theme: &str, page: u16, subcode: u16, case: Case) -> Value {
    let permission = match case {
        Case::G2 => Permission::Free,
        Case::Diacritic => Permission::Zone,
    };
    let mut value = super::response(theme, page, subcode, Family::G0, permission);
    for object in value["service"]["objects"].as_array_mut().unwrap() {
        let commands = object["program"]["commands"].as_array_mut().unwrap();
        assert_eq!(commands.len(), 3);
        assert_eq!(
            commands[2]["operation"],
            serde_json::to_value(Family::G0.operation(0)).unwrap()
        );
        commands[2]["operation"] = serde_json::to_value(case.operation(false)).unwrap();
    }
    value
}
fn g2_free(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, Case::G2)
}
fn diacritic_zone(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, Case::Diacritic)
}
fn metadata(flow: &Flow, case: Case) -> Value {
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
            operation: case.operation(false)
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
    actual_editor::operand_metadata(
        flow,
        identity,
        2,
        case.word(false),
        case.word(true),
        "local_extended_glyph",
    )
}
fn assert_pixels(
    service: &ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService,
    case: Case,
    edited: bool,
    foreground: [u8; 3],
) {
    let raster = external_editor_probe::preview_page(service, 124);
    assert_eq!((raster.width(), raster.height()), (672, 250));
    for column in [44, 45] {
        for (row, bits) in case.mask(edited).into_iter().enumerate() {
            for x in 0..12 {
                let expected = if bits & (1 << (11 - x)) != 0 {
                    foreground
                } else {
                    [0, 0, 0]
                };
                let Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(pixel))) =
                    raster.pixel(column * 12 + x, 10 + row)
                else {
                    panic!("both shared extended Local consumers require settled RGB4 pixels")
                };
                assert_eq!(pixel.get(), expected, "literal native extended glyph mask");
            }
        }
    }
}
async fn assert_concept(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    case: Case,
) {
    let revision = drcs_bitmap::stored(fixture, concept).await;
    let decoded = decode_teletext_design_documents(&revision.document, designs::budget()).unwrap();
    let overlay = decoded.editorial_overlay().unwrap();
    assert_eq!(overlay.schema_version(), 6);
    assert!(overlay.pages().is_empty());
    assert!(overlay.local_program_operands().is_empty());
    assert!(overlay.drcs_bitmap_overlays().is_empty());
    assert_eq!(overlay.local_object_operands(), [case.change()].as_slice());
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
        .with_local_object_operand_overlays(&[case.change()], policy().encoding.validation.service)
        .unwrap();
    assert_eq!(after.declared(), &expected);
    super::assert_rendered(before, after, case.change());
    assert_pixels(before, case, false, [15, 0, 0]);
    assert_pixels(after, case, true, [15, 0, 0]);
}
async fn prohibited_g2_positions(fixture: &Fixture, flow: &Flow) {
    use ersatzrs_web::host::{
        teletext_channel_documents::ChannelTeletextDocumentEncodingError,
        teletext_designs::NativeTeletextDesignError,
    };
    // EN 300 706 table37 note5 is assignment policy, not scalar shape.
    for code in [0x59_u32, 0x5a, 0x5b, 0x65] {
        let request = edited_request(
            &flow.receipts[0],
            1,
            Case::G2.word(false),
            (15 << 6) | (code << 11),
        );
        let NativeTeletextEditorialImportError::Design(error) =
            super::reject(fixture, flow, request).await
        else {
            panic!("a seven-bit prohibited G2 position must reach native qualification");
        };
        let NativeTeletextDesignError::Encoding(error) = *error else {
            panic!("prohibited G2 positions require an encoding diagnostic");
        };
        let ChannelTeletextDocumentEncodingError::Encoding(error) = *error else {
            panic!("the full authored encoder must refuse a prohibited G2 position");
        };
        assert_eq!(error.display, Some(channel_documents::address(124, 32)));
        assert_retained_storage(fixture, flow).await;
    }
}

async fn run_case(directory: &Path, case: Case) {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        case.factory(),
        Fixture::new_persistent().await,
        drcs_companion::encoding_policy,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    assert_pixels(flow.accepted[0].service(), case, false, [15, 0, 0]);
    assert_pixels(flow.accepted[1].service(), case, false, [0, 0, 15]);
    let ready =
        actual_editor::inputs_for_edit(directory, case.name(), &flow, || metadata(&flow, case));
    let marker = actual_editor::completion(directory).await;
    let saved =
        live_client::read_bounded(&directory.join("edited.tti"), policy().tti.max_bytes).unwrap();
    let word = usize::try_from(ready["edit"]["semantic_word"].as_u64().unwrap()).unwrap();
    assert_eq!(actual_editor::word_at(&saved, word), case.word(true));
    actual_editor::assert_saved_packet_words(&flow, &saved, case.word(false), case.word(true));
    if matches!(case, Case::G2) {
        prohibited_g2_positions(&fixture, &flow).await;
    }
    let mut request = actual_editor::actual_request(directory, &flow, &saved);
    "Actual pinned QTM Local extended glyph concept".clone_into(&mut request.name);
    let concept = import(&fixture, request).await;
    assert_eq!(
        (concept.head().id, concept.head().revision),
        (flow.saved[0].head().id, 2)
    );
    assert_concept(&fixture, &flow, &concept, case).await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    assert_concept(&fixture, &flow, &concept, case).await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    let revision = drcs_bitmap::stored(&fixture, &concept).await;
    actual_editor::json_new(
        directory,
        "success.json",
        &json!({"probe_version":1,"case":case.name(),"editor":marker,
        "receipt_id":flow.receipts[0].metadata.id,"receipt_digest":flow.receipts[0].metadata.digest,
        "original_digest":flow.originals[0].digest,"design_id":concept.head().id,"revision":2,"digest":concept.digest(),
        "concept_document_sha256":drcs_bitmap::hash(&revision.document),"saved_file":"edited.tti","saved_sha256":drcs_bitmap::hash(&saved),
        "schema_version":6,"edit":ready["edit"],"native_preview_geometry":[672,250],"exact_native_extended_masks_and_both_consumers":true,
        "actual_widget_font_equivalence":"not asserted","reopen_verified":true,
        "original_sources_package_profiles_config_night_unchanged":true,"publication":"unchanged",
        "ordinary_stream_client_and_full_acceptance":"not measured by this test"}),
    );
    fixture.state.pool.close().await;
}
#[tokio::test]
#[ignore = "requires fresh pinned headed QTM1.0 extended glyph Save As via ERSATZRS_TELETEXT_EXTENDED_GLYPH_EDITOR_DIR"]
async fn actual_qtm_local_extended_glyph_popup_and_diacritic_menu_save_as_v6_preview_reopen() {
    let root = PathBuf::from(super::super::super::client_capture::selected(
        "ERSATZRS_TELETEXT_EXTENDED_GLYPH_EDITOR_DIR",
    ));
    std::fs::create_dir(&root).expect("actual extended glyph proof root must be create-new");
    for case in [Case::G2, Case::Diacritic] {
        Box::pin(run_case(&root.join(case.name()), case)).await;
    }
}
