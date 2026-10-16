//! Opt-in pinned editor output to an unpublished concept; no delivery/client claim.
use super::*;
use ersatzrs_domain::{
    TeletextEnhancementCommand, TeletextObjectId, TeletextObjectIdentity, TeletextObjectOffset,
    TeletextObjectType,
};
use std::path::{Path, PathBuf};

const EDITOR_COMMIT: &str = "cac2167bac933a6160cca45a45d5d4ee95355898";
const EDITOR_BINARY: &str = "af7cb11220a411011917db0739f27a35b5c6ab725552b52b8a09bc01e296568a";

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct EditorSaved {
    probe_version: u16,
    editor_name: String,
    editor_version: String,
    editor_source_commit: String,
    editor_binary_sha256: String,
    saved_file: String,
}

fn json_new(directory: &Path, name: &str, value: &impl serde::Serialize) {
    live_client::write_new(
        &directory.join(name),
        &serde_json::to_vec_pretty(value).unwrap(),
    )
    .unwrap();
}

fn word_at(bytes: &[u8], semantic_word: usize) -> u32 {
    let parsed = parse_teletext_tti(bytes, policy().tti).unwrap();
    let mut matching = parsed.records().iter().filter_map(|record| match record {
        TeletextTtiRecord::Packet {
            row: 26,
            designation,
            words,
        } if usize::from(*designation) == semantic_word / 13 => Some(words[semantic_word % 13]),
        _ => None,
    });
    let word = matching.next().unwrap();
    assert!(matching.next().is_none());
    word
}

fn original_identity(flow: &Flow, family: Family) -> TeletextObjectIdentity {
    let owner = channel_documents::address(124, 32);
    let identity = TeletextObjectIdentity::local(owner, TeletextObjectId::new(65520));
    let service = flow.accepted[0].service();
    let definition = service
        .declared()
        .semantic()
        .objects()
        .definition(identity)
        .unwrap();
    assert_eq!(definition.object_type, TeletextObjectType::Passive);
    assert_eq!(
        definition.program.commands()[2],
        ersatzrs_domain::TeletextEnhancementCommand::AtColumn {
            column: 0,
            operation: family.operation(0),
        }
    );
    let calls: Vec<_> = service.declared().semantic().page_enhancements()[&owner]
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
        vec![
            (
                TeletextObjectType::Passive,
                Some(TeletextObjectOffset::new(0, 32).unwrap())
            ),
            (
                TeletextObjectType::Passive,
                Some(TeletextObjectOffset::new(0, 33).unwrap())
            ),
        ]
    );
    identity
}

fn edit_metadata(flow: &Flow, family: Family) -> Value {
    let identity = original_identity(flow, family);
    let owner = identity.owner().unwrap();
    let service = flow.accepted[0].service();
    let program = service
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page)
                if page.page().address() == owner =>
            {
                let geometry = page.normalized().original().display_geometry();
                assert_eq!(
                    (
                        geometry.left_width(),
                        geometry.right_width(),
                        geometry.total_width()
                    ),
                    (4, 12, 56)
                );
                Some(page.normalized().encoded())
            }
            _ => None,
        })
        .unwrap();
    let mut positions = program.command_positions().iter().filter(|position| {
        position.source().object == Some(identity) && position.source().command == 2
    });
    let position = positions.next().unwrap();
    assert!(positions.next().is_none());
    assert!(position.original_local_command().is_none());
    let word = position.semantic_word();
    assert!(position.first_word() <= word && word < 16 * 13);
    let mut markers = program
        .definition_positions()
        .iter()
        .filter(|marker| marker.target() == identity);
    let marker = markers.next().unwrap();
    assert!(markers.next().is_none());
    assert_eq!(marker.object_type(), TeletextObjectType::Passive);
    let marker_index = usize::from(marker.absolute_index());
    assert!(marker_index < position.first_word());
    assert_eq!(
        usize::from(marker.object_number()),
        ((marker_index / 13) << 4) | (marker_index % 13)
    );
    assert!(program
        .definition_positions()
        .iter()
        .all(|other| usize::from(other.absolute_index()) <= marker_index
            || usize::from(other.absolute_index()) > word));
    let exported = &flow.exported[0];
    let mapping = exported
        .sidecar()
        .project
        .pages
        .iter()
        .find(|page| page.page.page == 124 && page.page.subcode == 32)
        .unwrap();
    let file = exported
        .files()
        .iter()
        .find(|file| file.name == mapping.name)
        .unwrap();
    assert_eq!(word_at(&file.bytes, word), family.word(0));
    let number = u32::from(marker.object_number());
    assert_eq!(
        word_at(&file.bytes, marker_index),
        (0x28 | (number >> 7)) | (0x17 << 6) | ((number & 127) << 11)
    );
    json!({"kind":"local_glyph","mode":match family {Family::G0=>9,Family::G1=>1},
        "from":family.data(0),"to":family.data(1),"column":0,"row":1,
        "semantic_word":word,"designation":word/13,"triplet_slot":word%13,
        "identity":identity,"source_command":2,
        "definition_marker":{"absolute_index":marker_index,"object_number":marker.object_number(),"object_type":"passive"},
        "invocation_offsets":[32,33]})
}

fn inputs(directory: &Path, name: &str, flow: &Flow, family: Family) -> Value {
    std::fs::create_dir(directory).expect("each editor case directory must be create-new");
    let original = directory.join("original");
    std::fs::create_dir(&original).unwrap();
    let exported = &flow.exported[0];
    let sidecar = write_teletext_export_sidecar(exported.sidecar(), policy().project).unwrap();
    live_client::write_new(&original.join("project.ersatzrs.json"), &sidecar).unwrap();
    for file in exported.files() {
        live_client::write_new(&original.join(&file.name), &file.bytes).unwrap();
    }
    let mapping = exported
        .sidecar()
        .project
        .pages
        .iter()
        .find(|page| page.page.page == 124 && page.page.subcode == 32)
        .unwrap();
    let edit = edit_metadata(flow, family);
    let ready = json!({"probe_version":1,"case":name,"editor_profile":exported.sidecar().project.editor_profile,
        "design_id":flow.saved[0].head().id,"design_revision":1,"design_digest":flow.saved[0].digest(),
        "receipt_id":exported.metadata().id,"receipt_digest":exported.metadata().digest,
        "input":format!("original/{}",mapping.name),"input_sha256":mapping.tti_sha256,
        "sidecar":"original/project.ersatzrs.json","sidecar_sha256":drcs_bitmap::hash(&sidecar),"page":mapping.page,
        "files":exported.files().iter().map(|file|json!({"name":file.name,"path":format!("original/{}",file.name),"bytes":file.bytes.len(),"sha256":drcs_bitmap::hash(&file.bytes)})).collect::<Vec<_>>(),
        "editor_geometry":{"left_columns":4,"right_columns":12,"total_columns":56,"main_origin_column":4,
            "target_visual_column":44,"target_visual_columns":[44,45]},"edit":edit,
        "save_as":"edited.tti","completion":"editor-saved.json","timeout_seconds":600,
        "editor":{"name":"QTeletextMaker","version":"1.0","source_commit":EDITOR_COMMIT,"binary_sha256":EDITOR_BINARY},
        "publication":"concept only; accepted generation remains unchanged"});
    json_new(directory, "ready.json", &ready);
    ready
}

async fn completion(directory: &Path) -> EditorSaved {
    tokio::time::timeout(Duration::from_mins(10), async {
        loop {
            if tokio::fs::try_exists(directory.join("editor-saved.json"))
                .await
                .unwrap()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("pinned editor must complete within600seconds");
    let marker: EditorSaved = serde_json::from_slice(
        &live_client::read_bounded(&directory.join("editor-saved.json"), 4096).unwrap(),
    )
    .unwrap();
    assert_eq!(marker.probe_version, 1);
    assert_eq!(marker.editor_name, "QTeletextMaker");
    assert_eq!(marker.editor_version, "1.0");
    assert_eq!(marker.editor_source_commit, EDITOR_COMMIT);
    assert_eq!(marker.editor_binary_sha256, EDITOR_BINARY);
    assert_eq!(marker.saved_file, "edited.tti");
    marker
}

fn actual_request(
    directory: &Path,
    flow: &Flow,
    bytes: &[u8],
) -> NativeTeletextEditorialImportRequest {
    let exported = &flow.exported[0];
    let original = directory.join("original");
    let sidecar = write_teletext_export_sidecar(exported.sidecar(), policy().project).unwrap();
    assert_eq!(
        live_client::read_bounded(
            &original.join("project.ersatzrs.json"),
            policy().project.max_bytes
        )
        .unwrap(),
        sidecar
    );
    for file in exported.files() {
        assert_eq!(
            live_client::read_bounded(&original.join(&file.name), policy().project.max_file_bytes)
                .unwrap(),
            file.bytes
        );
    }
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    "Actual pinned QTM local glyph concept".clone_into(&mut request.name);
    let name = &exported
        .sidecar()
        .project
        .pages
        .iter()
        .find(|page| page.page.page == 124 && page.page.subcode == 32)
        .unwrap()
        .name;
    request
        .files
        .iter_mut()
        .find(|file| &file.name == name)
        .unwrap()
        .bytes = bytes.to_vec();
    request
}

fn assert_saved_packets(flow: &Flow, saved: &[u8], family: Family) {
    // Compare fixed packet content independently of record order, hex spelling
    // and permitted editor metadata. Main rows are checked by the exact empty
    // row-overlay and full protected native-model assertions after import.
    let expected = edited_request(&flow.receipts[0], 1, family.word(0), family.word(1));
    let mapping = flow.exported[0]
        .sidecar()
        .project
        .pages
        .iter()
        .find(|page| page.page.page == 124 && page.page.subcode == 32)
        .unwrap();
    let bytes = &expected
        .files
        .iter()
        .find(|file| file.name == mapping.name)
        .unwrap()
        .bytes;
    let packets = |bytes: &[u8]| {
        let mut result: Vec<_> = parse_teletext_tti(bytes, policy().tti)
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
        result.sort_unstable();
        result
    };
    assert_eq!(
        packets(saved),
        packets(bytes),
        "only the exact retained glyph body word may change"
    );
}

fn altered_word(bytes: &[u8], semantic_word: usize, expected: u32, replacement: u32) -> Vec<u8> {
    assert_eq!(word_at(bytes, semantic_word), expected);
    let mut records = parse_teletext_tti(bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    for record in &mut records {
        if let TeletextTtiRecord::Packet {
            row: 26,
            designation,
            words,
        } = record
        {
            if usize::from(*designation) == semantic_word / 13 {
                words[semantic_word % 13] = replacement;
            }
        }
    }
    write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap()
}

async fn run_case(
    directory: &Path,
    name: &str,
    factory: fn(&str, u16, u16) -> Value,
    family: Family,
) {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        factory,
        Fixture::new_persistent().await,
        drcs_companion::encoding_policy,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let ready = inputs(directory, name, &flow, family);
    let marker = completion(directory).await;
    let saved =
        live_client::read_bounded(&directory.join("edited.tti"), policy().tti.max_bytes).unwrap();
    let word = usize::try_from(ready["edit"]["semantic_word"].as_u64().unwrap()).unwrap();
    assert_eq!(word_at(&saved, word), family.word(1));
    assert_saved_packets(&flow, &saved, family);
    for replacement in [family.word(1) | 1, family.word(1) ^ (1 << 6)] {
        let altered = altered_word(&saved, word, family.word(1), replacement);
        assert!(matches!(
            reject(&fixture, &flow, actual_request(directory, &flow, &altered)).await,
            NativeTeletextEditorialImportError::Tti(_)
        ));
    }
    let concept = import(&fixture, actual_request(directory, &flow, &saved)).await;
    assert_eq!(
        (concept.head().id, concept.head().revision),
        (flow.saved[0].head().id, 2)
    );
    // The native importer checks all packets and permitted editor metadata; this
    // exact single-intent assertion excludes any additional freely editable rows.
    assert_concept(&fixture, &flow, &concept, family, 1).await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    assert_concept(&fixture, &flow, &concept, family, 1).await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    let revision = drcs_bitmap::stored(&fixture, &concept).await;
    json_new(
        directory,
        "success.json",
        &json!({"probe_version":1,"case":name,"editor":marker,
        "receipt_id":flow.receipts[0].metadata.id,"receipt_digest":flow.receipts[0].metadata.digest,
        "original_digest":flow.originals[0].digest,"design_id":concept.head().id,"revision":2,"digest":concept.digest(),
        "concept_document_sha256":drcs_bitmap::hash(&revision.document),"saved_file":"edited.tti","saved_sha256":drcs_bitmap::hash(&saved),
        "schema_version":4,"edit":ready["edit"],"native_preview_geometry":[672,250],"both_shared_consumers_changed":true,
        "reopen_verified":true,"original_sources_package_profiles_config_night_unchanged":true,
        "publication":"unchanged","ordinary_stream_client_and_full_acceptance":"not measured by this test"}),
    );
    fixture.state.pool.close().await;
}

#[tokio::test]
#[ignore = "requires two fresh pinned headed QTM1.0 Save As processes via ERSATZRS_TELETEXT_OBJECT_EDITOR_DIR"]
async fn actual_qtm_local_object_g0_free_and_g1_zone_save_as_import_preview_reopen() {
    let root = PathBuf::from(super::super::super::client_capture::selected(
        "ERSATZRS_TELETEXT_OBJECT_EDITOR_DIR",
    ));
    std::fs::create_dir(&root).expect("actual editor proof root must be create-new");
    for (name, factory, family) in [
        ("g0-free", free as fn(&str, u16, u16) -> Value, Family::G0),
        ("g1-zone", zone, Family::G1),
    ] {
        Box::pin(run_case(&root.join(name), name, factory, family)).await;
    }
}
