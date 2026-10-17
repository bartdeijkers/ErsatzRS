//! Actual pinned remaining-glyph popup Save As to unpublished Local schema7 concepts.
//! Fixed native masks are checked; actual widget-font equivalence is not asserted.
use super::*;
use ersatzrs_domain::{
    TeletextEnhancementCommand, TeletextG1AlphaCode, TeletextG3Compatibility, TeletextObjectId,
    TeletextObjectIdentity, TeletextObjectOffset, TeletextObjectType,
};
use std::path::{Path, PathBuf};

// Literal fixed-font masks from domain glyph_mask_tests.rs, not captured output.
const LATIN_A: [u16; 10] = [0, 0x0f0, 0x198, 0x30c, 0x606, 0x7fe, 0x606, 0x606, 0, 0];
const LATIN_AT: [u16; 10] = [0, 0x1fc, 0x306, 0x63e, 0x666, 0x666, 0x63e, 0x300, 0x1fc, 0];
const G3_CHECKERBOARD: [u16; 10] = [
    0xaaa, 0x555, 0xaaa, 0x555, 0xaaa, 0x555, 0xaaa, 0x555, 0xaaa, 0x555,
];
const G3_BLANK: [u16; 10] = [0; 10];

#[derive(Clone, Copy)]
enum Case {
    G1Alpha,
    G3Level25,
}
impl Case {
    fn name(self) -> &'static str {
        match self {
            Self::G1Alpha => "g1-alpha-free",
            Self::G3Level25 => "g3-level25-zone",
        }
    }
    fn operation(self, edited: bool) -> TeletextColumnOperation {
        match self {
            Self::G1Alpha => TeletextColumnOperation::G1Alpha {
                code: TeletextG1AlphaCode::new(if edited { 64 } else { 65 }).unwrap(),
            },
            Self::G3Level25 => TeletextColumnOperation::G3 {
                code: TeletextGlyphCode::new(if edited { 95 } else { 47 }).unwrap(),
                compatibility: TeletextG3Compatibility::Level25,
            },
        }
    }
    fn word(self, edited: bool) -> u32 {
        match (self, edited) {
            (Self::G1Alpha, false) => 0x20840,
            (Self::G1Alpha, true) => 0x20040,
            (Self::G3Level25, false) => 0x17ac0,
            (Self::G3Level25, true) => 0x2fac0,
        }
    }
    fn mask(self, edited: bool) -> [u16; 10] {
        match (self, edited) {
            (Self::G1Alpha, false) => LATIN_A,
            (Self::G1Alpha, true) => LATIN_AT,
            (Self::G3Level25, false) => G3_CHECKERBOARD,
            (Self::G3Level25, true) => G3_BLANK,
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
            Self::G1Alpha => g1_alpha_free,
            Self::G3Level25 => g3_level25_zone,
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
        Case::G1Alpha => Permission::Free,
        Case::G3Level25 => Permission::Zone,
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
fn g1_alpha_free(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, Case::G1Alpha)
}
fn g3_level25_zone(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, Case::G3Level25)
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
        "local_remaining_glyph",
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
                    panic!(
                        "both shared remaining-family Local consumers require settled RGB4 pixels"
                    )
                };
                assert_eq!(
                    pixel.get(),
                    expected,
                    "literal native remaining-family glyph mask"
                );
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
    assert_eq!(overlay.schema_version(), 7);
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
    assert_fallback_unchanged(before, after);
    assert_pixels(before, case, false, [15, 0, 0]);
    assert_pixels(after, case, true, [15, 0, 0]);
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
    let mut request = actual_editor::actual_request(directory, &flow, &saved);
    "Actual pinned QTM Local remaining-family glyph concept".clone_into(&mut request.name);
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
        "schema_version":7,"edit":ready["edit"],"native_preview_geometry":[672,250],"exact_native_remaining_masks_and_both_consumers":true,
        "headers_fallback_and_non_x26_records_unchanged":true,"actual_widget_font_equivalence":"not asserted","reopen_verified":true,
        "original_sources_package_profiles_config_night_unchanged":true,"publication":"unchanged",
        "ordinary_stream_client_and_full_acceptance":"not measured by this test"}),
    );
    fixture.state.pool.close().await;
}
#[tokio::test]
#[ignore = "requires fresh pinned headed QTM1.0 remaining glyph Save As via ERSATZRS_TELETEXT_REMAINING_GLYPH_EDITOR_DIR"]
async fn actual_qtm_local_remaining_glyph_popups_save_as_v7_preview_reopen() {
    let root = PathBuf::from(super::super::super::client_capture::selected(
        "ERSATZRS_TELETEXT_REMAINING_GLYPH_EDITOR_DIR",
    ));
    std::fs::create_dir(&root).expect("actual remaining glyph proof root must be create-new");
    for case in [Case::G1Alpha, Case::G3Level25] {
        Box::pin(run_case(&root.join(case.name()), case)).await;
    }
}
