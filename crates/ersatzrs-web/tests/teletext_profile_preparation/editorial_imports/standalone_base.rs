//! L15-A/B: direct Main capacity and native concept/publication.
//! The ignored case adds genuine pinned editor evidence; no stream/client claim.

use super::local_object_glyphs::actual_editor as qtm;
use super::*;
use serde_json::{json, Value};

fn commands() -> Value {
    let mut commands = Vec::new();
    for row in 1..=5 {
        commands.push(json!({"kind":"set_position","position":{"row":row,"column":0}}));
        for column in 0..40 {
            let operation = match (row, column) {
                (1, 1) => json!({"kind":"g3","code":47,"compatibility":"level15"}),
                (1, 2) => json!({"kind":"g0_diacritic","code":65,"diacritic":2}),
                _ => json!({"kind":"g2","code":35}),
            };
            commands.push(json!({"kind":"at_column","column":column,"operation":operation}));
        }
    }
    commands.push(json!({"kind":"set_position","position":{"row":6,"column":0}}));
    commands.push(json!({"kind":"at_column","column":0,"operation":{"kind":"g2","code":35}}));
    assert_eq!(commands.len(), 207); // The native termination fills the 208th word.
    json!({"commands":commands})
}

fn response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut response = external_editor_probe::editor_response(theme, page, subcode);
    response["service"]["magazines"] = json!([]);
    for enhancement in response["service"]["page_enhancements"]
        .as_array_mut()
        .unwrap()
    {
        enhancement["presentation"] = Value::Null;
        enhancement["compositional_links"] = Value::Null;
        enhancement["local_program"] = if enhancement["address"]["page"] == 124 {
            commands()
        } else {
            Value::Null
        };
    }
    for page in response["authoring"]["pages"].as_array_mut().unwrap() {
        page["regions"] = json!([
            {"id":"free-title","kind":"free","area":"main","row":1,"column":0,"width":40,"height":6},
            {"id":"custom-zone","kind":"zone","area":"main","row":7,"column":0,"width":40,"height":1},
            {"id":"guide-content","kind":"generated","area":"main","row":8,"column":0,"width":40,"height":1}
        ]);
    }
    assert!(serde_json::to_vec(&response).unwrap().len() <= 32_768);
    response
}

fn packet_words(files: &[TeletextExportFile], final_word: u32) -> Vec<u32> {
    let file = files
        .iter()
        .find(|file| file.name == "page-124-0020.tti")
        .unwrap();
    let tti = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
    let mut words = Vec::new();
    for record in tti.records() {
        match record {
            TeletextTtiRecord::Packet {
                row: 26,
                designation,
                words: packet,
            } => {
                assert_eq!(usize::from(*designation), words.len() / 13);
                words.extend(packet);
            }
            TeletextTtiRecord::Packet { row, .. } => {
                panic!("unexpected enhanced-only packet {row}")
            }
            _ => {}
        }
    }
    assert_eq!(words.len(), 208);
    assert_eq!(words[206], final_word); // Main G2, not a Local object body.
    for word in &words {
        let address: u32 = *word & 63_u32;
        let mode: u32 = (*word >> 6_u32) & 31_u32;
        assert!(
            if address < 40 {
                [2_u32, 15, 18].contains(&mode)
            } else {
                [4_u32, 31].contains(&mode)
            },
            "base-only programme must not acquire enhanced attributes or object calls: {word:05x}"
        );
    }
    words
}

fn edited_request(flow: &Flow) -> NativeTeletextEditorialImportRequest {
    let exported = &flow.exported[0];
    let mut files = exported.files().to_vec();
    let file = files
        .iter_mut()
        .find(|file| file.name == "page-124-0020.tti")
        .unwrap();
    let mut records = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    let last = records
        .iter_mut()
        .find_map(|record| match record {
            TeletextTtiRecord::Packet {
                row: 26,
                designation: 15,
                words,
            } => Some(words),
            _ => None,
        })
        .unwrap();
    assert_eq!(last[11], 0x11bc0);
    last[11] = 0x213c0; // Same-family G2 acute, original command206 remains owned.
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    NativeTeletextEditorialImportRequest {
        name: "Standalone Level 1.5 Main concept".into(),
        receipt_id: exported.metadata().id,
        receipt_digest: exported.metadata().digest.clone(),
        expected_design_head: 1,
        sidecar: write_teletext_export_sidecar(exported.sidecar(), policy().project).unwrap(),
        files,
    }
}

#[tokio::test]
async fn standalone_level15_full_main_exports_without_banks_and_keeps_explicit_publication() {
    Box::pin(run(None)).await;
}

#[tokio::test]
#[ignore = "requires pinned headed QTM1.0 via fresh ERSATZRS_TELETEXT_LEVEL15_EDITOR_DIR"]
async fn actual_qtm_standalone_level15_full_main_edit_preview_publish_reopen() {
    let root = std::path::PathBuf::from(super::super::client_capture::selected(
        "ERSATZRS_TELETEXT_LEVEL15_EDITOR_DIR",
    ));
    Box::pin(run(Some(&root))).await;
}

async fn run(editor: Option<&Path>) {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        response,
        Fixture::new_persistent().await,
        channel_documents::policy,
        authoring_metadata::write_compact,
    ))
    .await;
    for exported in &flow.exported {
        assert!(exported.sidecar().project.assets.is_empty());
        assert_eq!(exported.sidecar().project.pages.len(), 2);
        packet_words(exported.files(), 0x11bc0);
    }
    for accepted in &flow.accepted {
        assert!(accepted
            .service()
            .declared()
            .resource_inventory()
            .is_empty());
        assert!(accepted
            .service()
            .declared()
            .semantic()
            .objects()
            .definitions()
            .is_empty());
    }
    let request = if let Some(directory) = editor {
        genuine_edit(directory, &flow).await
    } else {
        edited_request(&flow)
    };
    let mut expected = packet_words(flow.exported[0].files(), 0x11bc0);
    expected[206] = 0x213c0;
    assert_eq!(packet_words(&request.files, 0x213c0), expected);
    let edited = import(&fixture, request).await;
    let restored = reload_original_teletext_design_revision(
        &fixture.state.application,
        edited.head().id,
        edited.head().revision,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    let preview = external_editor_probe::preview_page(restored.encoded().unwrap(), 124);
    assert_eq!((preview.width(), preview.height()), (480, 250));
    assert_ne!(
        preview,
        external_editor_probe::preview_page(flow.accepted[0].service(), 124)
    );
    assert_original_pointers_with_width(&fixture, &flow, 480).await;
    let accepted = publish_and_accept(&fixture, &flow, &edited).await;
    assert_ne!(
        designs::units(accepted.service()),
        designs::units(flow.accepted[0].service())
    );
    assert!(accepted
        .service()
        .declared()
        .resource_inventory()
        .is_empty());
    fixture = fixture.reopen().await;
    assert_eq!(
        recover(&fixture, 1).await.generation(),
        accepted.generation()
    );
    assert_eq!(
        recover(&fixture, 2).await.generation(),
        flow.accepted[1].generation()
    );
    assert_retained_storage(&fixture, &flow).await;
    if let Some(directory) = editor {
        qtm::json_new(
            directory,
            "success.json",
            &json!({
                "scope":"Pinned actual editor to native concept/preview/explicit publication and durable recovery only; no ordinary stream/client/fullL15 claim",
                "editor":"QTeletextMaker1.0/cac2167/af7cb", "main_words":208,
                "edited_word":206,"before":0x11bc0,"after":0x213c0,"resource_assets":0,
                "design_id":edited.head().id,"concept_revision":edited.head().revision,
                "generation_id":accepted.generation().id,"night_unchanged":true,"originals_unchanged":true
            }),
        );
    }
}

async fn genuine_edit(directory: &Path, flow: &Flow) -> NativeTeletextEditorialImportRequest {
    qtm::inputs_for_layout(
        directory,
        "standalone-level15-full-main",
        flow,
        [0, 0, 0],
        || {
            json!({
                "kind":"main_g2","mode":15,"from":35,"to":66,"column":0,"row":6,
                "semantic_word":206,"designation":15,"triplet_slot":11,"source_command":206,
                "main_words":208,"consumer_visual_columns":[0],"no_optional_resource_banks":true
            })
        },
    );
    qtm::completion(directory).await;
    let bytes =
        live_client::read_bounded(&directory.join("edited.tti"), policy().tti.max_bytes).unwrap();
    qtm::actual_request(directory, flow, &bytes)
}
