//! L15-A/B: direct Main capacity and native concept/publication.
//! The ignored case adds genuine pinned editor evidence; no stream/client claim.

use super::local_object_glyphs::actual_editor as qtm;
use super::*;
use serde_json::{json, Value};

#[path = "standalone_base/national_editor.rs"]
mod national_editor;

#[path = "standalone_base/native_editor.rs"]
mod native_editor;

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

fn invalid_enrichments(original: &Value) -> [(&'static str, Value); 3] {
    let mut palette = original.clone();
    let mut presentation = super::super::documents::document(
        super::super::documents::DocumentFixture::Authored,
        "day",
    )["magazines"][0]["presentation"]
        .clone();
    presentation["palette"]["clut2"][0][0] = json!(16); // RGB4 is 0..15.
    palette["service"]["page_enhancements"][0]["presentation"] = presentation;

    let mut drcs = original.clone();
    // Structurally known enhanced operation, but no declared DRCS source/body.
    drcs["service"]["page_enhancements"][0]["local_program"] = json!({"commands":[
        {"kind":"set_position","position":{"row":1,"column":0}},
        {"kind":"at_column","column":0,"operation":{"kind":"drcs",
            "reference":{"table":"normal","subtable":0,"slot":0}}}
    ]});

    let mut pop = original.clone();
    let mut companion = super::super::documents::document(
        super::super::documents::DocumentFixture::Companion,
        "day",
    )["page_enhancements"][0]
        .clone();
    companion["address"] = json!({"page":124,"subcode":32});
    // Keep the required X27/public page, omit the explicit empty POP body.
    pop["service"]["page_enhancements"][0] = companion;
    [
        ("RGB4 out of range", palette),
        ("missing DRCS", drcs),
        ("missing POP body", pop),
    ]
}

async fn assert_accepted_base_unchanged(fixture: &Fixture, flow: &Flow) {
    for (index, channel) in [1, 2].into_iter().enumerate() {
        assert_eq!(
            fixture
                .state
                .application
                .teletext_publication_request(channel)
                .await
                .unwrap(),
            Some(flow.pending[index].clone())
        );
        assert_eq!(
            fixture
                .state
                .application
                .accepted_teletext_generation(channel)
                .await
                .unwrap(),
            Some(flow.accepted[index].generation().clone())
        );
        generations::assert_same_output_width(
            &recover(fixture, channel).await,
            &flow.accepted[index],
            480,
        );
        assert_eq!(
            original(fixture, &flow.saved[index]).await,
            flow.originals[index]
        );
        assert_eq!(
            receipt(fixture, &flow.exported[index]).await,
            flow.receipts[index]
        );
        assert_eq!(
            fixture
                .state
                .application
                .teletext_design_head(flow.saved[index].head().id)
                .await
                .unwrap()
                .unwrap()
                .revision,
            1
        );
    }
    assert_eq!(designs::config(fixture).await, flow.config);
    assert!(fixture.playout.is_empty());
}

fn assert_enrichment_error(
    index: usize,
    error: ersatzrs_web::host::teletext_channel_documents::ChannelTeletextDocumentEncodingError,
) {
    use ersatzrs_app::{
        teletext_channel_documents::ChannelTeletextDocumentError,
        teletext_pages_v2::{TeletextAuthoredPagesError, TeletextAuthoredValueError},
        teletext_pages_v3::TeletextAuthoredPagesV3Error,
        teletext_profile_documents::BoundTeletextDocumentError,
    };
    use ersatzrs_domain::teletext::{
        TeletextDrcsResourceError, TeletextEnhancementValueError, TeletextServiceError,
        TeletextX27LinkRole,
    };
    use ersatzrs_web::host::teletext_channel_documents::ChannelTeletextDocumentEncodingError as Error;
    let Error::Preparation(ChannelTeletextDocumentError::Document { binding: 0, error }) = error
    else {
        panic!("complete original must reject during semantic response validation")
    };
    let BoundTeletextDocumentError::AuthoredMetadataResponse(error) = *error else {
        panic!("must retain the selected V3 contract")
    };
    let TeletextAuthoredPagesV3Error::Authored(error) = *error else {
        panic!("input framing must be valid; the enrichment itself is invalid")
    };
    match (index, error) {
        (
            0,
            TeletextAuthoredPagesError::Conversion {
                location,
                reason:
                    TeletextAuthoredValueError::Enhancement(
                        TeletextEnhancementValueError::RgbComponent { component: 0 },
                    ),
            },
        ) => {
            assert_eq!(location.record, 0);
        }
        (1, TeletextAuthoredPagesError::Service(error)) => {
            let TeletextServiceError::CollectedDrcs { error, .. } = *error else {
                panic!("the referenced DRCS must remain required")
            };
            assert_eq!(error.source_location.unwrap().command, 1);
            let TeletextDrcsResourceError::MissingLink {
                display,
                reference: 0,
                ..
            } = error.error
            else {
                panic!("missing normal DRCS link must not become a base glyph")
            };
            assert_eq!(display, channel_documents::address(124, 32));
        }
        (2, TeletextAuthoredPagesError::Service(error)) => {
            let TeletextServiceError::CompositionalResource {
                display,
                role: TeletextX27LinkRole::Pop,
                page,
                subpage,
            } = *error
            else {
                panic!("the announced POP body must remain required")
            };
            assert_eq!(display, channel_documents::address(124, 32));
            assert_eq!((page.magazine(), page.page(), subpage.get()), (1, 0xa0, 0));
        }
        other => panic!("wrong semantic rejection for enrichment {index}: {other:?}"),
    }
}

#[tokio::test]
async fn valid_standalone_base_never_silently_drops_invalid_optional_enrichments() {
    use ersatzrs_web::host::teletext_channel_documents::prepare_channel_teletext_document_candidate;
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        response,
        Fixture::new_persistent().await,
        channel_documents::policy,
        authoring_metadata::write_compact,
    ))
    .await;
    let original = response("day", 124, 32);
    for (index, (name, invalid)) in invalid_enrichments(&original).into_iter().enumerate() {
        authoring_metadata::write_compact(&fixture, "day", &invalid);
        let result = prepare_channel_teletext_document_candidate(
            &fixture.state.application,
            1,
            Some(ADDON),
            &guide(&[]),
            channel_documents::policy(),
        )
        .await;
        let Err(error) = result else {
            panic!("{name}: malformed enrichment must reject the complete candidate")
        };
        println!("standalone-base rejection: {name}: {error:?}");
        assert_enrichment_error(index, error);
        assert_accepted_base_unchanged(&fixture, &flow).await;
        assert_eq!(
            std::fs::read_to_string(fixture.root.path().join("authoring-calls")).unwrap(),
            format!(
                "{}{}",
                std::str::from_utf8(&flow.calls).unwrap(),
                "day:teletext.pages.v3\n".repeat(index + 1)
            )
        );
    }
    // A genuine valid base remains independently preparable afterwards; no
    // resource installation, fallback stripping or configuration change repairs it.
    authoring_metadata::write_compact(&fixture, "day", &original);
    let valid = prepare_with_policy(&fixture, 1, channel_documents::policy()).await;
    assert!(valid
        .encoded()
        .unwrap()
        .declared()
        .resource_inventory()
        .is_empty());
    assert_eq!(
        designs::units(valid.encoded().unwrap()),
        designs::units(flow.accepted[0].service())
    );
    assert_accepted_base_unchanged(&fixture, &flow).await;
    fixture.state.pool.close().await;
}
