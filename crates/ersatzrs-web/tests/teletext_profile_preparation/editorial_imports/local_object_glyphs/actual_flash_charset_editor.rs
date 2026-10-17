//! Actual pinned Flash/charset Save As to unpublished Local schema9 concepts.
//! Explicit native phases are checked; no timer/client or widget-font equivalence claim.
use super::*;
use ersatzrs_domain::{
    TeletextEnhancementCharsetDesignation, TeletextEnhancementCommand, TeletextFlashMode,
    TeletextFlashRate, TeletextObjectId, TeletextObjectIdentity, TeletextObjectOffset,
    TeletextObjectType,
};
use std::path::{Path, PathBuf};

// Source-fixed masks.bin tables0/4 code43, not received/editor pixels.
const LATIN_C: [u16; 10] = [0, 0x0fc, 0x186, 0x300, 0x300, 0x300, 0x186, 0x0fc, 0, 0];
const GREEK_GAMMA: [u16; 10] = [0, 0x7fe, 0x302, 0x300, 0x300, 0x300, 0x300, 0x780, 0, 0];
#[derive(Clone, Copy)]
enum Case {
    Flash,
    Charset,
}
impl Case {
    fn name(self) -> &'static str {
        match self {
            Self::Flash => "flash-free",
            Self::Charset => "charset-free",
        }
    }
    fn kind(self) -> &'static str {
        match self {
            Self::Flash => "local_flash",
            Self::Charset => "local_charset",
        }
    }
    fn command(self) -> usize {
        match self {
            Self::Flash => 3,
            Self::Charset => 2,
        }
    }
    fn operation(self, edited: bool) -> TeletextColumnOperation {
        match self {
            Self::Flash => TeletextColumnOperation::Flash {
                mode: if edited {
                    TeletextFlashMode::AdjacentClut
                } else {
                    TeletextFlashMode::Steady
                },
                rate: if edited {
                    TeletextFlashRate::Fast2HzPhase2
                } else {
                    TeletextFlashRate::Slow1Hz
                },
            },
            Self::Charset => TeletextColumnOperation::DesignateCharset {
                designation: TeletextEnhancementCharsetDesignation::new(if edited {
                    0x37
                } else {
                    0
                })
                .unwrap(),
            },
        }
    }
    fn word(self, edited: bool) -> u32 {
        let (mode, data) = match self {
            Self::Flash => (7, if edited { 11 } else { 0 }),
            Self::Charset => (8, if edited { 55 } else { 0 }),
        };
        (mode << 6) | (data << 11)
    }
    fn change(self) -> TeletextLocalObjectOperandOverlay {
        TeletextLocalObjectOperandOverlay::new(
            identity(),
            self.command(),
            0,
            self.operation(false),
            self.operation(true),
        )
        .unwrap()
    }
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
        commands.truncate(2);
        commands.extend([
            json!({"kind":"at_column","column":0,"operation":Case::Charset.operation(false)}),
            json!({"kind":"at_column","column":0,"operation":Case::Flash.operation(false)}),
            json!({"kind":"at_column","column":0,"operation":{"kind":"g0","code":67}}),
            json!({"kind":"at_column","column":1,"operation":Case::Charset.operation(false)}),
            json!({"kind":"at_column","column":1,"operation":Case::Flash.operation(false)}),
        ]);
    }
    value
}
fn metadata(flow: &Flow, case: Case) -> Value {
    let owner = channel_documents::address(124, 32);
    let identity = identity();
    let service = flow.accepted[0].service().declared().semantic();
    let definition = service.objects().definition(identity).unwrap();
    assert_eq!(definition.object_type, TeletextObjectType::Passive);
    assert_eq!(definition.program.commands().len(), 7);
    assert_eq!(
        definition.program.commands()[case.command()],
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
        case.command(),
        case.word(false),
        case.word(true),
        case.kind(),
    )
}
fn assert_pixels(
    service: &ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService,
    case: Case,
    edited: bool,
    foreground: [u8; 3],
) {
    for phase in [TeletextFlashPhase::Original, TeletextFlashPhase::Alternate] {
        let preview = project_teletext_preview(
            service,
            channel_documents::address(124, 32),
            TeletextColourPixelOptions {
                reveal_concealed: false,
                flash_phase: phase,
            },
            super::super::preview_budget(),
        )
        .unwrap();
        assert!(preview.raster().diagnostics().is_empty());
        let raster = preview.raster();
        assert_eq!((raster.width(), raster.height()), (672, 250));
        let mask = if matches!(case, Case::Charset) && edited {
            GREEK_GAMMA
        } else {
            LATIN_C
        };
        let colour =
            if matches!(case, Case::Flash) && edited && phase == TeletextFlashPhase::Alternate {
                [3, 3, 3] // Original CLUT3[2], paired with unchanged Fg18 by xor8.
            } else {
                foreground
            };
        for column in [44, 45] {
            for (row, bits) in mask.into_iter().enumerate() {
                for x in 0..12 {
                    let expected = if bits & (1 << (11 - x)) != 0 {
                        colour
                    } else {
                        [0, 0, 0]
                    };
                    let Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(pixel))) =
                        raster.pixel(column * 12 + x, 10 + row)
                    else {
                        panic!("both Flash/charset consumers require settled explicit-phase RGB4 pixels")
                    };
                    assert_eq!(pixel.get(),expected,"fixed LatinC/GreekGamma and adjacent-clut phases, without wall-clock inference");
                }
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
    assert_eq!(overlay.schema_version(), 9);
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
    assert_rendered(before, after, case);
    assert_fallback_unchanged(before, after);
    assert_pixels(before, case, false, [15, 0, 0]);
    assert_pixels(after, case, true, [15, 0, 0]);
}
fn assert_rendered(
    before: &ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService,
    after: &ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService,
    case: Case,
) {
    assert_eq!(before.pages().len(), after.pages().len());
    assert_eq!(before.magazine_units(), after.magazine_units());
    assert_eq!(before.inventory(), after.inventory());
    for (old, new) in before.pages().iter().zip(after.pages()) {
        assert_eq!(old.resource_header(), new.resource_header());
        let owner = matches!(old,EncodedTeletextAuthoredPage::CompiledDisplay(page) if page.page().address()==identity().owner().unwrap());
        if owner {
            assert_ne!(old.data_units(), new.data_units());
        } else {
            assert_eq!(old.data_units(), new.data_units());
        }
    }
    for phase in [TeletextFlashPhase::Original, TeletextFlashPhase::Alternate] {
        let raster = |service, page| {
            project_teletext_preview(
                service,
                channel_documents::address(page, 32),
                TeletextColourPixelOptions {
                    reveal_concealed: false,
                    flash_phase: phase,
                },
                super::super::preview_budget(),
            )
            .unwrap()
            .raster()
            .clone()
        };
        assert_eq!(raster(before, 125), raster(after, 125));
        let original = raster(before, 124);
        let edited = raster(after, 124);
        let mut changes = [false; 2];
        for (position, (a, b)) in original.pixels().iter().zip(edited.pixels()).enumerate() {
            if a == b {
                continue;
            }
            let (row, column) = (position / original.width(), position % original.width());
            assert!(
                (10..20).contains(&row) && (44 * 12..46 * 12).contains(&column),
                "Main and every protected panel/row stay exact in both phases"
            );
            changes[column / 12 - 44] = true;
        }
        assert_eq!(
            changes,
            if matches!(case, Case::Flash) && phase == TeletextFlashPhase::Original {
                [false; 2]
            } else {
                [true; 2]
            }
        );
    }
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
        response,
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
    "Actual pinned QTM Local Flash/charset concept".clone_into(&mut request.name);
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
        "schema_version":9,"edit":ready["edit"],"native_preview_geometry":[672,250],"exact_native_masks_and_both_explicit_phases":true,
        "headers_fallback_and_non_x26_records_unchanged":true,"actual_widget_font_equivalence":"not asserted","wall_clock_flash_timing":"not measured","reopen_verified":true,
        "original_sources_package_profiles_config_night_unchanged":true,"publication":"unchanged",
        "ordinary_stream_client_and_full_acceptance":"not measured by this test"}),
    );
    fixture.state.pool.close().await;
}
#[tokio::test]
#[ignore = "requires two fresh pinned headed QTM1.0 Flash/charset Save As via ERSATZRS_TELETEXT_FLASH_CHARSET_EDITOR_DIR"]
async fn actual_qtm_local_flash_and_charset_save_as_v9_preview_reopen() {
    let root = PathBuf::from(super::super::super::client_capture::selected(
        "ERSATZRS_TELETEXT_FLASH_CHARSET_EDITOR_DIR",
    ));
    std::fs::create_dir(&root).expect("actual Flash/charset proof root must be create-new");
    for case in [Case::Flash, Case::Charset] {
        Box::pin(run_case(&root.join(case.name()), case)).await;
    }
}
