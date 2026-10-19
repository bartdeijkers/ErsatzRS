//! Generated G2 words must not impersonate original page-local commands.
use super::*;
use ersatzrs_web::host::teletext_editorial_designs::{
    NativeTeletextEditorialImportError, NativeTeletextEditorialTtiFailure,
};

fn request(flow: &Flow, context: Context) -> NativeTeletextEditorialImportRequest {
    request_for(flow, context, false)
}

fn request_for(
    flow: &Flow,
    context: Context,
    native: bool,
) -> NativeTeletextEditorialImportRequest {
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    let file = request
        .files
        .iter_mut()
        .find(|file| file.name == "page-124-0020.tti")
        .unwrap();
    let mut records = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    let mut targets = Vec::new();
    for record in &mut records {
        if let TeletextTtiRecord::Packet {
            row: 26,
            designation,
            words,
        } = record
        {
            for (slot, word) in words.iter_mut().enumerate() {
                if (*word >> 6) & 31 == 15 && *word >> 11 == 0x23 {
                    targets.push((usize::from(*designation) * 13 + slot, *word));
                    *word = (*word & 0x7ff)
                        | (if native || context.primary == 0x37 {
                            0x24
                        } else {
                            0x25
                        } << 11);
                }
            }
        }
    }
    assert_eq!(
        targets.len(),
        1,
        "exact original generated £ word: {targets:?}"
    );
    println!(
        "native generated G2 context{:02x} physical target {targets:?}",
        context.primary
    );
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    request
}

async fn assert_unchanged(fixture: &Fixture, flow: &Flow) {
    assert_original_pointers_with_width(fixture, flow, 480).await;
    for original in &flow.saved {
        assert_eq!(
            fixture
                .state
                .application
                .teletext_design_head(original.head().id)
                .await
                .unwrap()
                .unwrap()
                .revision,
            1
        );
    }
    assert!(fixture.playout.is_empty());
}

fn non_text_request(flow: &Flow, context: Context) -> NativeTeletextEditorialImportRequest {
    let mut result = request(flow, context);
    let file = result
        .files
        .iter_mut()
        .find(|file| file.name == "page-124-0020.tti")
        .unwrap();
    let mut records = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    let mut count = 0;
    for record in &mut records {
        if let TeletextTtiRecord::Packet { row: 26, words, .. } = record {
            for word in words {
                if (*word >> 6) & 31 == 15
                    && *word >> 11 == if context.primary == 0x37 { 0x24 } else { 0x25 }
                {
                    // A Base text glyph must not become an authored colour writer.
                    *word = (*word & 63) | (1 << 11);
                    count += 1;
                }
            }
        }
    }
    assert_eq!(count, 1);
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    result
}

// Explicit Base text intent does not authorize invented Local attribute writes.
#[tokio::test]
async fn generated_native_g2_non_text_edit_rejects_without_concept_or_channel_damage() {
    for context in CONTEXTS_G2 {
        let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
            context.response,
            Fixture::new_persistent().await,
            channel_documents::policy,
            authoring_metadata::write_compact,
        ))
        .await;
        let error = import_teletext_editorial_design(
            &fixture.state.application,
            non_text_request(&flow, context),
            policy(),
            designs::deadline(),
        )
        .await
        .unwrap_err();
        let NativeTeletextEditorialImportError::Tti(error) = error else {
            panic!("generated Base G2 must not impersonate authored Local intent: {error:?}");
        };
        assert_eq!(error.failure, NativeTeletextEditorialTtiFailure::Packet);
        assert_eq!(error.page, 124);
        assert_eq!(error.subcode, 32);
        assert_eq!(error.row, Some(26));
        assert_eq!(error.designation, Some(0));
        assert_eq!(error.column, Some(1));
        assert_unchanged(&fixture, &flow).await;
        fixture = fixture.reopen().await;
        assert_unchanged(&fixture, &flow).await;
        fixture.state.pool.close().await;
    }
}

async fn editor_request(
    flow: &Flow,
    context: Context,
    editor: Option<&Path>,
    native: bool,
) -> NativeTeletextEditorialImportRequest {
    if let Some(directory) = editor {
        qtm::inputs_for_layout(
            directory,
            "standalone-generated-base-g2",
            flow,
            [0, 0, 8],
            || {
                json!({
                    "kind":"base_text_g2","primary":context.primary,"row":3,"column":8,
                    "mode":15,"from":35,"to":if native || context.primary == 0x37 {36} else {37},
                    "semantic_word":1,"designation":0,"triplet_slot":1,
                    "consumer_visual_columns":[8],"no_optional_resource_banks":true
                })
            },
        );
        qtm::completion(directory).await;
        let bytes =
            live_client::read_bounded(&directory.join("edited.tti"), policy().tti.max_bytes)
                .unwrap();
        qtm::actual_request(directory, flow, &bytes)
    } else {
        request_for(flow, context, native)
    }
}

fn assert_base_provenance(
    flow: &Flow,
    restored: &ersatzrs_web::host::teletext_designs::RestoredOriginalTeletextDesign,
    context: Context,
    native: bool,
) {
    let original = flow.saved[0].candidate();
    assert_eq!(restored.selection(), original.selection());
    assert_eq!(restored.metadata(), original.metadata());
    assert_eq!(restored.transmissions(), original.transmissions());
    assert_eq!(restored.sources().len(), original.sources().len());
    for (before, after) in original.sources().iter().zip(restored.sources()) {
        assert_eq!(before.bytes(), after.bytes());
        assert_eq!(before.authoring(), after.authoring());
    }
    let encoded = restored.encoded().unwrap();
    assert!(encoded
        .declared()
        .semantic()
        .page_enhancements()
        .values()
        .all(|page| page.local_program.is_none()));
    let page = encoded
        .declared()
        .semantic()
        .base()
        .pages
        .iter()
        .find(|page| page.address.page.get() == 124)
        .unwrap();
    let row = page.rows.iter().find(|row| row.row == 3).unwrap();
    let TeletextRunContent::Text { text } = &row.runs[0].content else {
        panic!("retain Base Text role")
    };
    assert_eq!(
        text.chars().next(),
        Some(if native {
            '$'
        } else if context.primary == 0x37 {
            'e'
        } else {
            '¥'
        })
    );
}

async fn run(context: Context, editor: Option<&Path>, native: bool) {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        context.response,
        Fixture::new_persistent().await,
        channel_documents::policy,
        authoring_metadata::write_compact,
    ))
    .await;
    let expected = request_for(&flow, context, native);
    let actual = editor_request(&flow, context, editor, native).await;
    national_editor::assert_files(&actual.files, &expected.files);
    let concept = import(&fixture, actual).await;
    assert_eq!(concept.head().revision, 2);
    let restored = drcs_bitmap::restored(&fixture, &concept).await;
    assert_base_provenance(&flow, &restored, context, native);
    let encoded = restored.encoded().unwrap();
    let before = external_editor_probe::preview_page(flow.accepted[0].service(), 124);
    let preview = external_editor_probe::preview_page(encoded, 124);
    assert_ne!(before, preview);
    assert!(preview.diagnostics().is_empty());
    for y in 0..250 {
        for x in 0..480 {
            if !(30..40).contains(&y) || !(96..108).contains(&x) {
                assert_eq!(before.pixel(x, y), preview.pixel(x, y));
            }
        }
    }
    assert_eq!(
        external_editor_probe::preview_page(encoded, 125),
        external_editor_probe::preview_page(flow.accepted[0].service(), 125)
    );
    assert_original_pointers_with_width(&fixture, &flow, 480).await;
    let reexport = drcs_bitmap::reexport(&fixture, &concept).await;
    national_editor::assert_files(reexport.files(), &expected.files);
    fixture = fixture.reopen().await;
    let recovered = drcs_bitmap::restored(&fixture, &concept).await;
    assert_eq!(
        external_editor_probe::preview_page(recovered.encoded().unwrap(), 124),
        preview
    );
    assert_original_pointers_with_width(&fixture, &flow, 480).await;
    assert!(fixture.playout.try_recv().is_err());
    let accepted = publish_and_accept(&fixture, &flow, &concept).await;
    assert_eq!(
        external_editor_probe::preview_page(accepted.service(), 124),
        preview
    );
    fixture = fixture.reopen().await;
    generations::assert_same_output_width(&recover(&fixture, 1).await, &accepted, 480);
    generations::assert_same_output_width(&recover(&fixture, 2).await, &flow.accepted[1], 480);
    assert_retained_storage(&fixture, &flow).await;
    if let Some(directory) = editor {
        for (name, service) in [
            ("before", flow.accepted[0].service()),
            ("after", accepted.service()),
        ] {
            let units: Vec<_> = (0..2)
                .flat_map(|_| {
                    service
                        .pages()
                        .iter()
                        .flat_map(|page| page.data_units().iter().map(|unit| unit.to_vec()))
                })
                .collect();
            qtm::json_new(directory, &format!("{name}.units.json"), &units);
        }
        qtm::json_new(
            directory,
            "success.json",
            &json!({
                "scope":"Actual pinned headed QTM save, explicit Base-text concept/shared preview/reexport/reopen, separate publication and accepted recovery/isolation; no ordinary stream/client/fullL15 acceptance",
                "primary":context.primary,"row":3,"column":8,"word":1,"schema":if native {24} else {23},
                "expected":'£',"replacement":if native {'$'} else if context.primary==0x37 {'e'} else {'¥'},
                "no_optional_resource_banks":true,"original_sources_metadata_receipts_other_page_and_night_unchanged":true,
                "concept_revision":concept.head().revision,"published_generation":accepted.generation().id
            }),
        );
    }
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn generated_native_g2_text_edit_import_preview_reexport_reopen_keeps_base_provenance() {
    for context in CONTEXTS_G2 {
        Box::pin(run(context, None, false)).await;
    }
}

#[tokio::test]
#[ignore = "requires four pinned headed QTM1.0 saves and fresh ERSATZRS_TELETEXT_BASE_G2_EDITOR_DIR"]
async fn actual_qtm_generated_native_g2_edit_import_preview_publish_reopen() {
    let root =
        std::path::PathBuf::from(super::super::super::super::super::client_capture::selected(
            "ERSATZRS_TELETEXT_BASE_G2_EDITOR_DIR",
        ));
    std::fs::create_dir(&root).expect("editor destination must be new");
    for context in CONTEXTS_G2 {
        Box::pin(run(context, Some(&root.join(context.name)), false)).await;
    }
}

#[tokio::test]
async fn lossless_generated_g2_native_spelling_import_preview_publish_and_reopen() {
    Box::pin(run(CONTEXTS_G2[0], None, true)).await;
}

#[tokio::test]
#[ignore = "requires one pinned headed QTM1.0 save and fresh ERSATZRS_TELETEXT_LOSSLESS_G2_EDITOR_DIR"]
async fn actual_qtm_lossless_g2_spelling_import_preview_publish_reopen() {
    let root =
        std::path::PathBuf::from(super::super::super::super::super::client_capture::selected(
            "ERSATZRS_TELETEXT_LOSSLESS_G2_EDITOR_DIR",
        ));
    std::fs::create_dir(&root).expect("editor destination must be new");
    Box::pin(run(
        CONTEXTS_G2[0],
        Some(&root.join(CONTEXTS_G2[0].name)),
        true,
    ))
    .await;
}
