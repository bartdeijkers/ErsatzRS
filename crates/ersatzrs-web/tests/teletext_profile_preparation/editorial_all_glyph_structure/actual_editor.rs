//! Exact mixed raw editor output to canonical unpublished V12 concepts.

use super::super::super::local_object_glyphs::actual_editor as editor;
use super::*;
use std::path::{Path, PathBuf};

const CASE: &str = "mixed-structure-free";
const ORIGINAL_WORDS: [usize; 13] = [12, 13, 14, 15, 16, 17, 18, 19, 21, 23, 24, 25, 26];

fn replacement() -> TeletextEnhancementProgram {
    let mut commands = all_commands(0);
    assert_eq!(commands.remove(8), column(5, family(5, false)));
    commands.insert(4, column(0, family(1, false)));
    TeletextEnhancementProgram::new(commands).unwrap()
}
fn expected_overlay() -> TeletextLocalObjectGlyphProgramOverlay {
    TeletextLocalObjectGlyphProgramOverlay::new_with_all_glyphs(
        identity(65520),
        all_program(0),
        replacement(),
        vec![
            Some(0),
            Some(1),
            Some(2),
            Some(3),
            None,
            Some(4),
            Some(5),
            Some(6),
            Some(7),
            Some(9),
            Some(10),
            Some(11),
            Some(12),
        ],
    )
    .unwrap()
}
fn raw_body() -> Vec<u32> {
    let mut body = all_words(0);
    assert_eq!(body.len(), 15);
    assert_eq!(body.remove(9), word(5, 13, 64));
    body.insert(4, word(0, 15, 35));
    assert_eq!(
        body[9],
        word(40, 24, 80),
        "actual editor retains the now-orphan Normal selector"
    );
    body
}
fn canonical_body() -> Vec<u32> {
    let mut body = raw_body();
    assert_eq!(body.remove(9), word(40, 24, 80));
    body
}
fn tuples(words: &[u32]) -> Vec<[u32; 3]> {
    words
        .iter()
        .map(|word| [word & 63, (word >> 6) & 31, word >> 11])
        .collect()
}
fn metadata(flow: &Flow) -> Value {
    let source = flow.accepted[0].service();
    assert_eq!(definitions(source), [11, 27]);
    assert_eq!(encoded(source).packet_bodies().len(), 3);
    let definition = source
        .declared()
        .semantic()
        .objects()
        .definition(identity(65520))
        .unwrap();
    assert_eq!(definition.object_type, TeletextObjectType::Passive);
    assert_eq!(definition.program, all_program(0));
    let positions: Vec<_> = (0..13)
        .map(|command| {
            let mut matching = encoded(source)
                .command_positions()
                .iter()
                .filter(|position| {
                    position.source().object == Some(identity(65520))
                        && position.source().command == command
                });
            let position = matching.next().unwrap();
            assert!(matching.next().is_none() && position.original_local_command().is_none());
            if command == 8 {
                assert_eq!(position.first_word(), 20);
            }
            if command == 9 {
                assert_eq!(position.first_word(), 22);
            }
            position.semantic_word()
        })
        .collect();
    assert_eq!(positions, ORIGINAL_WORDS);
    let source_words = service_words(source);
    assert_eq!(&source_words[12..27], all_words(0));
    assert_eq!(source_words[31], word(63, 31, 5));
    let calls: Vec<_> = source.declared().semantic().page_enhancements()
        [&channel_documents::address(124, 32)]
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
            } if *target == identity(65520) => Some((*object_type, *offset)),
            _ => None,
        })
        .collect();
    assert_eq!(
        calls,
        [32, 36].map(|column| (
            TeletextObjectType::Passive,
            Some(TeletextObjectOffset::new(0, column).unwrap())
        ))
    );
    let original_semantic: Vec<_> = ORIGINAL_WORDS.map(|word| source_words[word]).into();
    let replacement_semantic: Vec<_> = original_semantic[..4]
        .iter()
        .copied()
        .chain([word(0, 15, 35)])
        .chain(original_semantic[4..8].iter().copied())
        .chain(original_semantic[9..].iter().copied())
        .collect();
    json!({"kind":"local_mixed_glyph_structure","identity":identity(65520),
        "body_first_word":12,"original_semantic_words":ORIGINAL_WORDS,
        "original_commands":tuples(&original_semantic),"replacement_commands":tuples(&replacement_semantic),
        "expected_retained_commands":expected_overlay().retained_commands(),
        "original_raw_commands":tuples(&all_words(0)),"saved_raw_commands":tuples(&raw_body()),
        "canonical_raw_commands":tuples(&canonical_body()),
        "delete":{"source_command":8,"semantic_word":21,"tuple":[5,13,64],
            "retained_prefix_word":20,"retained_prefix":[40,24,80]},
        "insert_copy":{"source_command":4,"semantic_word":16,"tuple":[1,15,35],
            "inserted_semantic_word":16,"from_column":1,"to_column":0},
        "definition_marker":{"absolute_index":11,"object_number":number(11),"object_type":"passive"},
        "following_definition":{"identity":identity(65521),"original_marker":27,"saved_raw_marker":27,
            "canonical_marker":26,"original_object_number":number(27),"saved_raw_object_number":number(27),
            "canonical_object_number":number(26),"body":[[0,8,0],[0,12,16],[0,9,80]]},
        "invocation_offsets":[32,36],"consumer_visual_columns":[44,45,46,47,48,49,50,51,52,53,54,55],
        "expected_changed_visual_columns":[44,48,53],
        "original_x26_packets":3,"saved_raw_x26_packets":3,"canonical_x26_packets":3,
        "saved_raw_meaningful_words":32,"canonical_meaningful_words":31})
}

fn packets(bytes: &[u8]) -> Vec<(u8, u8, [u32; 13])> {
    let mut packets: Vec<_> = parse_teletext_tti(bytes, policy().tti)
        .unwrap()
        .records()
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
}
fn prepared_request(flow: &Flow) -> NativeTeletextEditorialImportRequest {
    all_request_words(
        &flow.receipts[0],
        1,
        flow.accepted[0].service(),
        0,
        raw_body(),
    )
}
fn assert_saved_raw(flow: &Flow, saved: &[u8]) {
    let expected = prepared_request(flow);
    let name = page_name(&expected);
    let expected = &expected
        .files
        .iter()
        .find(|file| file.name == name)
        .unwrap()
        .bytes;
    assert_eq!(
        packets(saved),
        packets(expected),
        "exact genuine raw packets retain the orphan Normal selector"
    );
    let records = parse_teletext_tti(saved, policy().tti).unwrap();
    let words = packet_words(records.records());
    assert_eq!(words.len(), 39);
    assert_eq!(&words[12..27], raw_body());
    assert_eq!(words[27], marker(27));
    assert_eq!(
        words
            .iter()
            .position(|word| *word == super::word(63, 31, 5)),
        Some(31)
    );
}
fn assert_model(before: &EncodedTeletextAuthoredService, after: &EncodedTeletextAuthoredService) {
    assert_eq!(definitions(after), [11, 26]);
    let words = service_words(after);
    assert_eq!(words.len(), 39);
    assert_eq!(&words[12..26], canonical_body());
    assert_eq!(words[26], marker(26));
    assert_eq!(
        words
            .iter()
            .position(|word| *word == super::word(63, 31, 5)),
        Some(30)
    );
    assert_eq!(
        words[..11]
            .iter()
            .filter(|word| **word == invocation(26))
            .count(),
        1
    );
    assert_eq!(non_x26_records(after), non_x26_records(before));
    assert_eq!(after.magazine_units(), before.magazine_units());
    assert_eq!(after.inventory(), before.inventory());
    assert_eq!(after.pages().len(), before.pages().len());
    for (old, new) in before.pages().iter().zip(after.pages()) {
        assert_eq!(old.resource_header(), new.resource_header());
        if !matches!(old,EncodedTeletextAuthoredPage::CompiledDisplay(page) if page.page().address()==channel_documents::address(124,32))
        {
            assert_eq!(old.data_units(), new.data_units());
        }
    }
    assert_eq!(
        after
            .declared()
            .semantic()
            .objects()
            .definition(identity(65521)),
        before
            .declared()
            .semantic()
            .objects()
            .definition(identity(65521))
    );
    assert_native_bindings(after);
    let old = external_editor_probe::preview_page(before, 124);
    let new = external_editor_probe::preview_page(after, 124);
    assert_eq!((new.width(), new.height()), (672, 250));
    let mut exclusive = [false; 2];
    for (position, (a, b)) in old.pixels().iter().zip(new.pixels()).enumerate() {
        if a == b {
            continue;
        }
        let (row, column) = (position / new.width(), position % new.width());
        assert!((10..20).contains(&row) && (44 * 12..56 * 12).contains(&column));
        if (44 * 12..48 * 12).contains(&column) {
            exclusive[0] = true;
        }
        if (52 * 12..56 * 12).contains(&column) {
            exclusive[1] = true;
        }
    }
    assert_eq!(exclusive, [true, true]);
}
fn assert_native_bindings(after: &EncodedTeletextAuthoredService) {
    assert_eq!(selected_operation(after, 48), family(1, false));
    assert_eq!(selected_operation(after, 49), family(1, false));
    assert_eq!(selected_operation(after, 54), family(6, false));
    let composed = composition(after);
    let cell = composed.display_resolution().cell(1, 54).unwrap();
    let candidate =
        composed.display_resolution().origins()[cell.origin().unwrap()].candidate_index();
    let global = composed
        .drcs_execution()
        .records()
        .iter()
        .find(|record| record.candidate_index() == candidate)
        .unwrap();
    assert_eq!(global.reference().table, TeletextDrcsTableScope::Global);
    assert_eq!(
        (
            global.reference().subtable.get(),
            global.reference().slot.get()
        ),
        (0, 0)
    );
    assert_eq!(global.bitmap().get(), [0x111; 10]);
    assert_eq!(
        global.resource(),
        TeletextResourceIdentity::new(
            TeletextResourceKind::Gdrcs,
            TeletextRawDataPageNumber::new(1, 0xc1).unwrap(),
            TeletextResourceSubpageIndex::new(0).unwrap()
        )
        .unwrap()
    );
    assert!(!composed
        .drcs_execution()
        .records()
        .iter()
        .any(
            |record| composed.glyph_layers().candidates()[record.candidate_index()]
                .source()
                .is_some_and(|source| source.object == Some(identity(65520))
                    && record.reference().table == TeletextDrcsTableScope::Normal)
        ));
}
async fn assert_concept(fixture: &Fixture, flow: &Flow, concept: &SavedEditorialTeletextDesign) {
    let stored = drcs_bitmap::stored(fixture, concept).await;
    let restored = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    let overlay = restored.editorial_overlay().unwrap();
    assert_eq!(overlay.schema_version(), 12);
    assert!(
        overlay.pages().is_empty()
            && overlay.local_program_operands().is_empty()
            && overlay.local_object_operands().is_empty()
            && overlay.drcs_bitmap_overlays().is_empty()
    );
    let expected = expected_overlay();
    assert_eq!(
        overlay.local_object_glyph_program_overlays(),
        std::slice::from_ref(&expected)
    );
    assert_eq!(
        overlay.provenance().original_digest,
        flow.originals[0].digest
    );
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
        .with_local_object_glyph_program_overlays(&[expected], policy().encoding.validation.service)
        .unwrap();
    assert_eq!(current.encoded().unwrap().declared(), &expected);
    assert_model(flow.accepted[0].service(), current.encoded().unwrap());
}
async fn setup() -> (Fixture, Flow) {
    Box::pin(setup_with_fixture_policy_and_writer(
        all_free,
        Fixture::new_persistent().await,
        local_object_glyphs::structural_encoding,
        authoring_metadata::write_compact,
    ))
    .await
}
async fn finish(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    retained: &resource_bank_admin::Retained,
) {
    Box::pin(assert_concept(fixture, flow, concept)).await;
    assert_original_pointers(fixture, flow).await;
    retained.assert_unchanged(fixture, flow).await;
    assert_retained_storage(fixture, flow).await;
    generations::assert_same_output(&recover(fixture, 1).await, &flow.accepted[0]);
    generations::assert_same_output(&recover(fixture, 2).await, &flow.accepted[1]);
}

#[tokio::test]
async fn prepared_mixed_structural_raw_orphan_selector_imports_canonical_v12_preview_reopen() {
    let (mut fixture, flow) = Box::pin(setup()).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    metadata(&flow);
    let request = prepared_request(&flow);
    let name = page_name(&request);
    assert_saved_raw(
        &flow,
        &request
            .files
            .iter()
            .find(|file| file.name == name)
            .unwrap()
            .bytes,
    );
    let concept = import(&fixture, request).await;
    assert_eq!(
        (concept.head().id, concept.head().revision),
        (flow.saved[0].head().id, 2)
    );
    Box::pin(finish(&fixture, &flow, &concept, &retained)).await;
    fixture = fixture.reopen().await;
    Box::pin(finish(&fixture, &flow, &concept, &retained)).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

async fn run_actual(directory: &Path) {
    let (mut fixture, flow) = Box::pin(setup()).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let ready = editor::inputs_for_edit(directory, CASE, &flow, || metadata(&flow));
    let marker = editor::completion(directory).await;
    let saved =
        live_client::read_bounded(&directory.join("edited.tti"), policy().tti.max_bytes).unwrap();
    assert_saved_raw(&flow, &saved);
    let mut request = editor::actual_request(directory, &flow, &saved);
    "Actual pinned QTM mixed Local structural concept".clone_into(&mut request.name);
    let concept = import(&fixture, request).await;
    assert_eq!(
        (concept.head().id, concept.head().revision),
        (flow.saved[0].head().id, 2)
    );
    Box::pin(finish(&fixture, &flow, &concept, &retained)).await;
    fixture = fixture.reopen().await;
    Box::pin(finish(&fixture, &flow, &concept, &retained)).await;
    assert!(fixture.playout.try_recv().is_err());
    let stored = drcs_bitmap::stored(&fixture, &concept).await;
    editor::json_new(
        directory,
        "success.json",
        &json!({"probe_version":1,"case":CASE,"editor":marker,
        "receipt_id":flow.receipts[0].metadata.id,"receipt_digest":flow.receipts[0].metadata.digest,
        "original_digest":flow.originals[0].digest,"design_id":concept.head().id,"revision":2,
        "digest":concept.digest(),"schema_version":12,"concept_document_sha256":drcs_bitmap::hash(&stored.document),
        "saved_file":"edited.tti","saved_sha256":drcs_bitmap::hash(&saved),"edit":ready["edit"],
        "saved_raw_orphan_normal_selector_verified":true,"native_canonical_orphan_omission_verified":true,
        "native_preview_geometry":[672,250],"both_shared_consumers_changed":true,"exact_native_structural_intent":true,
        "original_resource_packets_protected_definition_and_non_x26_unchanged":true,
        "reopen_verified":true,"original_sources_package_profiles_config_night_unchanged":true,
        "publication":"unchanged","actual_widget_font_equivalence":"not asserted",
        "ordinary_stream_client_and_full_acceptance":"not measured by this test"}),
    );
    fixture.state.pool.close().await;
}

#[tokio::test]
#[ignore = "requires fresh pinned headed QTM1.0 mixed structural Save As via ERSATZRS_TELETEXT_MIXED_STRUCTURE_EDITOR_DIR"]
async fn actual_qtm_mixed_structure_save_as_v12_concept_preview_reopen() {
    let root = PathBuf::from(super::super::super::super::client_capture::selected(
        "ERSATZRS_TELETEXT_MIXED_STRUCTURE_EDITOR_DIR",
    ));
    assert!(
        root.is_absolute(),
        "editor proof root must be an explicit absolute input"
    );
    std::fs::create_dir(&root).expect("actual editor proof root must be create-new");
    Box::pin(run_actual(&root.join(CASE))).await;
}
