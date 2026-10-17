//! Actual pinned DRCS subtable/scope Save As to unpublished Local schema10 concepts.
//! Four original banks; no ordinary delivery, client or full resource acceptance claim.
use super::*;
use ersatzrs_domain::{
    TeletextDrcsGlyphReference, TeletextDrcsGlyphSlot, TeletextDrcsSubtableIndex,
    TeletextDrcsTableScope, TeletextEnhancementCommand, TeletextObjectId, TeletextObjectIdentity,
    TeletextObjectOffset, TeletextObjectType,
};
use std::path::{Path, PathBuf};

// These masks/resources are the fixed four-bank native fixture, not observed output.
use super::drcs_selections::fixture::{bank_mask, encoding, prepare_banks};
#[derive(Clone, Copy)]
enum Case {
    Subtable,
    Scope,
}
impl Case {
    fn name(self) -> &'static str {
        match self {
            Self::Subtable => "subtable-free",
            Self::Scope => "scope-free",
        }
    }
    fn operation(self, edited: bool, slot: u8) -> TeletextColumnOperation {
        TeletextColumnOperation::Drcs {
            reference: TeletextDrcsGlyphReference {
                table: if edited && matches!(self, Self::Scope) {
                    TeletextDrcsTableScope::Global
                } else {
                    TeletextDrcsTableScope::Normal
                },
                subtable: TeletextDrcsSubtableIndex::new(u8::from(
                    edited && matches!(self, Self::Subtable),
                ))
                .unwrap(),
                slot: TeletextDrcsGlyphSlot::new(slot).unwrap(),
            },
        }
    }
    fn word(self, edited: bool, slot: u8) -> u32 {
        let scope = if edited && matches!(self, Self::Scope) {
            0
        } else {
            0x40
        };
        u32::from(slot) | (13 << 6) | ((scope | u32::from(slot)) << 11)
    }
    fn changes(self) -> Vec<TeletextLocalObjectOperandOverlay> {
        (0..2)
            .map(|slot| {
                TeletextLocalObjectOperandOverlay::new(
                    identity(),
                    usize::from(slot) + 2,
                    slot,
                    self.operation(false, slot),
                    self.operation(true, slot),
                )
                .unwrap()
            })
            .collect()
    }
    fn selector_data(self) -> u32 {
        match self {
            Self::Subtable => 81,
            Self::Scope => 16,
        }
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
    prepare_banks(&mut value);
    for object in value["service"]["objects"].as_array_mut().unwrap() {
        let commands = object["program"]["commands"].as_array_mut().unwrap();
        assert_eq!(commands.len(), 3);
        assert_eq!(
            commands[2]["operation"],
            serde_json::to_value(Family::G0.operation(0)).unwrap()
        );
        commands[2]["operation"] =
            serde_json::to_value(Case::Subtable.operation(false, 0)).unwrap();
        commands.push(
            json!({"kind":"at_column","column":1,"operation":Case::Subtable.operation(false,1)}),
        );
    }
    for page in value["service"]["page_enhancements"]
        .as_array_mut()
        .unwrap()
    {
        let commands = page["local_program"]["commands"].as_array_mut().unwrap();
        assert_eq!(commands.len(), 5);
        assert_eq!(commands[4]["offset"]["column"], 33);
        commands[4]["offset"]["column"] = json!(34);
    }
    for page in value["authoring"]["pages"].as_array_mut().unwrap() {
        let region = page["regions"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|region| region["id"] == "free-right-glyph")
            .unwrap();
        assert_eq!(region["width"], 2);
        region["width"] = json!(4);
    }
    value
}
fn selector_position(flow: &Flow) -> (usize, [usize; 2]) {
    let owner = identity().owner().unwrap();
    let service = flow.accepted[0].service();
    let program = service
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page)
                if page.page().address() == owner =>
            {
                Some(page.normalized().encoded())
            }
            _ => None,
        })
        .unwrap();
    let first = program
        .command_positions()
        .iter()
        .find(|position| {
            position.source().object == Some(identity()) && position.source().command == 2
        })
        .unwrap();
    let second = program
        .command_positions()
        .iter()
        .find(|position| {
            position.source().object == Some(identity()) && position.source().command == 3
        })
        .unwrap();
    assert_eq!(
        first.first_word() + 1,
        first.semantic_word(),
        "the first original DRCS command emits one selector and one glyph"
    );
    assert_eq!(second.first_word(), second.semantic_word());
    let selector = first.first_word();
    let mapping = flow.exported[0]
        .sidecar()
        .project
        .pages
        .iter()
        .find(|page| page.page.page == 124 && page.page.subcode == 32)
        .unwrap();
    let file = flow.exported[0]
        .files()
        .iter()
        .find(|file| file.name == mapping.name)
        .unwrap();
    assert_eq!(actual_editor::word_at(&file.bytes, selector), 0x28628);
    (selector, [first.semantic_word(), second.semantic_word()])
}
fn metadata(flow: &Flow, case: Case) -> Value {
    let owner = identity().owner().unwrap();
    let service = flow.accepted[0].service();
    let semantic = service.declared().semantic();
    let definition = semantic.objects().definition(identity()).unwrap();
    assert_eq!(definition.object_type, TeletextObjectType::Passive);
    assert_eq!(definition.program.commands().len(), 4);
    for slot in 0_u8..2 {
        assert_eq!(
            definition.program.commands()[usize::from(slot) + 2],
            TeletextEnhancementCommand::AtColumn {
                column: slot,
                operation: case.operation(false, slot)
            }
        );
    }
    let calls: Vec<_> = semantic.page_enhancements()[&owner]
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
            } if *target == identity() => Some((*object_type, *offset)),
            _ => None,
        })
        .collect();
    assert_eq!(
        calls,
        [32, 34].map(|column| (
            TeletextObjectType::Passive,
            Some(TeletextObjectOffset::new(0, column).unwrap())
        ))
    );
    let glyph_provenance: Vec<_> = (0..2)
        .map(|slot| {
            actual_editor::operand_metadata(
                flow,
                identity(),
                usize::from(slot) + 2,
                case.word(false, slot),
                case.word(true, slot),
                "local_drcs_selection",
            )
        })
        .collect();
    let (selector, covered_semantic_words) = selector_position(flow);
    let mut targets = vec![
        json!({"role":"selector","triplet_index":selector,"mode":24,"column":40,"from":80,"to":case.selector_data(),"source_command":2}),
    ];
    if matches!(case, Case::Scope) {
        for slot in 0_u8..2 {
            targets.push(json!({"role":"glyph","triplet_index":glyph_provenance[usize::from(slot)]["semantic_word"],"mode":13,"column":slot,"from":64+slot,"to":slot,"source_command":usize::from(slot)+2}));
        }
    }
    let mut edit = glyph_provenance[0].clone();
    edit["selector_word"] = json!(selector);
    edit["targets"] = json!(targets);
    edit["covered_commands"] = json!([2, 3]);
    edit["covered_semantic_words"] = json!(covered_semantic_words);
    edit["consumer_visual_columns"] = json!([44, 45, 46, 47]);
    edit["glyph_provenance"] = json!(glyph_provenance);
    edit
}
fn assert_saved_packets(flow: &Flow, saved: &[u8], edit: &Value) {
    let mapping = flow.exported[0]
        .sidecar()
        .project
        .pages
        .iter()
        .find(|page| page.page.page == 124 && page.page.subcode == 32)
        .unwrap();
    let file = flow.exported[0]
        .files()
        .iter()
        .find(|file| file.name == mapping.name)
        .unwrap();
    let mut records = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    for target in edit["targets"].as_array().unwrap() {
        let index = usize::try_from(target["triplet_index"].as_u64().unwrap()).unwrap();
        let word = |field: &str| {
            u32::try_from(
                target["column"].as_u64().unwrap()
                    | (target["mode"].as_u64().unwrap() << 6)
                    | (target[field].as_u64().unwrap() << 11),
            )
            .unwrap()
        };
        let mut seen = 0;
        for record in &mut records {
            if let TeletextTtiRecord::Packet {
                row: 26,
                designation,
                words,
            } = record
            {
                if usize::from(*designation) == index / 13 {
                    assert_eq!(words[index % 13], word("from"));
                    words[index % 13] = word("to");
                    seen += 1;
                }
            }
        }
        assert_eq!(seen, 1);
        assert_eq!(actual_editor::word_at(saved, index), word("to"));
    }
    let packets = |records: &[TeletextTtiRecord]| {
        let mut packets: Vec<_> = records
            .iter()
            .filter_map(|record| match record {
                TeletextTtiRecord::Packet {
                    row,
                    designation,
                    words,
                } => Some((*row, *designation, *words)),
                _ => None,
            })
            .collect();
        packets.sort_unstable();
        packets
    };
    assert_eq!(
        packets(parse_teletext_tti(saved, policy().tti).unwrap().records()),
        packets(&records),
        "only the existing authenticated shared prefix and selected glyphscope words may differ"
    );
}
fn assert_pixels(
    service: &ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService,
    case: Case,
    edited: bool,
    foreground: [u8; 3],
) {
    let raster = external_editor_probe::preview_page(service, 124);
    assert_eq!((raster.width(), raster.height()), (672, 250));
    for (column, slot) in [(12, 0), (44, 0), (45, 1), (46, 0), (47, 1)] {
        let TeletextColumnOperation::Drcs { reference } =
            case.operation(edited && column != 12, slot)
        else {
            unreachable!()
        };
        let mask = bank_mask(
            reference.table,
            reference.subtable.get(),
            reference.slot.get(),
        );
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
                        "original Main and four shared Local DRCS consumers require settled RGB4"
                    )
                };
                assert_eq!(
                    pixel.get(),
                    expected,
                    "fixed originalbank scope/subtable/slot mask"
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
    assert_eq!(overlay.schema_version(), 10);
    assert!(overlay.pages().is_empty());
    assert!(overlay.local_program_operands().is_empty());
    assert!(overlay.drcs_bitmap_overlays().is_empty());
    assert_eq!(overlay.local_object_operands(), case.changes().as_slice());
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
        .with_local_object_operand_overlays(&case.changes(), policy().encoding.validation.service)
        .unwrap();
    assert_eq!(after.declared(), &expected);
    assert_eq!(
        after.declared().semantic().drcs(),
        before.declared().semantic().drcs()
    );
    assert_rendered(before, after);
    assert_fallback_unchanged(before, after);
    assert_pixels(before, case, false, [15, 0, 0]);
    assert_pixels(after, case, true, [15, 0, 0]);
}
fn assert_rendered(
    before: &ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService,
    after: &ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService,
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
    let original = external_editor_probe::preview_page(before, 124);
    let edited = external_editor_probe::preview_page(after, 124);
    let mut changes = [false; 4];
    for (position, (old, new)) in original.pixels().iter().zip(edited.pixels()).enumerate() {
        if old != new {
            let (row, column) = (position / original.width(), position % original.width());
            assert!(
                (10..20).contains(&row) && (44 * 12..48 * 12).contains(&column),
                "Main and every protected row/panel pixel stay original"
            );
            changes[column / 12 - 44] = true;
        }
    }
    assert_eq!(changes, [true; 4]);
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
        encoding,
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
    assert_saved_packets(&flow, &saved, &ready["edit"]);
    let mut request = actual_editor::actual_request(directory, &flow, &saved);
    "Actual pinned QTM Local DRCS selection concept".clone_into(&mut request.name);
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
        "schema_version":10,"edit":ready["edit"],"native_preview_geometry":[672,250],"exact_native_bank_masks_and_four_local_consumers":true,"main_original_slot0_unchanged":true,
        "headers_fallback_non_x26_and_resources_unchanged":true,"reopen_verified":true,
        "original_sources_package_profiles_config_night_unchanged":true,"publication":"unchanged",
        "ordinary_stream_client_and_full_resource_acceptance":"not measured by this test"}),
    );
    fixture.state.pool.close().await;
}
#[tokio::test]
#[ignore = "requires two fresh pinned headed QTM1.0 DRCS selection Save As processes via ERSATZRS_TELETEXT_DRCS_SELECTION_EDITOR_DIR"]
async fn actual_qtm_local_drcs_subtable_and_scope_save_as_v10_preview_reopen() {
    let root = PathBuf::from(super::super::super::client_capture::selected(
        "ERSATZRS_TELETEXT_DRCS_SELECTION_EDITOR_DIR",
    ));
    std::fs::create_dir(&root).expect("actual DRCS selection proof root must be create-new");
    for case in [Case::Subtable, Case::Scope] {
        Box::pin(run_case(&root.join(case.name()), case)).await;
    }
}
