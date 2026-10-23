//! Genuine native G0 editing is distinct from retaining context during a G2 edit.
use super::*;

#[path = "native_editor/supplementary.rs"]
mod supplementary;

#[path = "native_editor/national.rs"]
mod national;

#[path = "native_editor/arabic_g0.rs"]
mod arabic_g0;

#[derive(Clone, Copy)]
struct Context {
    name: &'static str,
    primary: u8,
    headline: &'static str,
    sentinels: &'static str,
    from: u8,
    to: u8,
    typed: char,
    response: fn(&str, u16, u16) -> Value,
}

const CONTEXTS: [Context; 4] = [
    Context {
        name: "cyrillic1",
        primary: 0x20,
        headline: "Београд 200",
        // Table38 has uppercaseЏ at5F;7F is a rectangle, not lowercaseџ.
        sentinels: "ЉЊЏљњ 200",
        from: 0x42,
        to: 0x58,
        typed: 'Љ',
        response: cyrillic1,
    },
    Context {
        name: "cyrillic2",
        primary: 0x24,
        headline: "Москва 200",
        sentinels: "Ййы 200",
        from: 0x4d,
        to: 0x56,
        typed: 'Ж',
        response: cyrillic2,
    },
    Context {
        name: "cyrillic3",
        primary: 0x25,
        headline: "Київ 200",
        sentinels: "ЙйЇїІі 200",
        from: 0x4b,
        to: 0x59,
        typed: 'І',
        response: cyrillic3,
    },
    Context {
        name: "hebrew",
        primary: 0x55,
        headline: "שלום 200",
        sentinels: "£#₪ 200",
        from: 0x79,
        to: 0x60,
        typed: 'א',
        response: hebrew,
    },
];

fn response(context: Context, theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = super::response(theme, page, subcode);
    let mut presentation = super::super::super::documents::document(
        super::super::super::documents::DocumentFixture::Authored,
        theme,
    )["magazines"][0]["presentation"]
        .clone();
    presentation["primary"] = json!(context.primary);
    presentation["panels"] = json!({"left":false,"right":false,"status":"level25_and35","width":0});
    for enhancement in value["service"]["page_enhancements"]
        .as_array_mut()
        .unwrap()
    {
        enhancement["presentation"] = presentation.clone();
        enhancement["local_program"] = Value::Null;
    }
    for page in value["service"]["pages"].as_array_mut().unwrap() {
        page["national_option"] = json!(context.primary & 7);
        page["rows"][0]["runs"][0]["content"]["text"] = json!(context.headline);
        let style = page["rows"][0]["runs"][0]["style"].clone();
        page["rows"]
            .as_array_mut()
            .unwrap()
            .push(json!({"row":3,"runs":[{
                "column":8,"style":style,
                "content":{"kind":"text","text":context.sentinels}
            }]}));
    }
    value
}

fn cyrillic1(theme: &str, page: u16, subcode: u16) -> Value {
    response(CONTEXTS[0], theme, page, subcode)
}
fn cyrillic2(theme: &str, page: u16, subcode: u16) -> Value {
    response(CONTEXTS[1], theme, page, subcode)
}
fn cyrillic3(theme: &str, page: u16, subcode: u16) -> Value {
    response(CONTEXTS[2], theme, page, subcode)
}
fn hebrew(theme: &str, page: u16, subcode: u16) -> Value {
    response(CONTEXTS[3], theme, page, subcode)
}

fn expected_request(flow: &Flow, context: Context) -> NativeTeletextEditorialImportRequest {
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    let file = request
        .files
        .iter_mut()
        .find(|f| f.name == "page-124-0020.tti")
        .unwrap();
    let mut records = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    let row = records
        .iter_mut()
        .find_map(|record| match record {
            TeletextTtiRecord::BaseRow { row: 1, cells } => Some(cells),
            _ => None,
        })
        .unwrap();
    assert_eq!(row[8], context.from);
    row[8] = context.to;
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    request
}

async fn genuine_edit(
    directory: &Path,
    flow: &Flow,
    context: Context,
) -> NativeTeletextEditorialImportRequest {
    qtm::inputs_for_layout(directory, "standalone-native-g0", flow, [0, 0, 8], || {
        json!({
            "kind":"native_g0","primary":context.primary,"from":context.from,"to":context.to,
            "unicode":u32::from(context.typed),"row":1,"column":8,
            "consumer_visual_columns":[8],"no_optional_resource_banks":true
        })
    });
    qtm::completion(directory).await;
    let bytes =
        live_client::read_bounded(&directory.join("edited.tti"), policy().tti.max_bytes).unwrap();
    qtm::actual_request(directory, flow, &bytes)
}

fn assert_preview(before: &TeletextRaster, after: &TeletextRaster) {
    assert_eq!((after.width(), after.height()), (480, 250));
    assert!(after.diagnostics().is_empty());
    assert_ne!(before, after);
    for y in 0..250 {
        for x in 0..480 {
            if !(10..20).contains(&y) || !(96..108).contains(&x) {
                assert_eq!(before.pixel(x, y), after.pixel(x, y));
            }
        }
    }
}

async fn run(context: Context, editor: Option<&Path>) {
    Box::pin(run_with_publication(context, editor, false)).await;
}

fn original_x26_records(flow: &Flow) -> usize {
    parse_teletext_tti(
        &flow.exported[0]
            .files()
            .iter()
            .find(|file| file.name == "page-124-0020.tti")
            .unwrap()
            .bytes,
        policy().tti,
    )
    .unwrap()
    .records()
    .iter()
    .filter(|record| matches!(record, TeletextTtiRecord::Packet { row: 26, .. }))
    .count()
}

async fn run_with_publication(context: Context, editor: Option<&Path>, publish: bool) {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        context.response,
        Fixture::new_persistent().await,
        channel_documents::policy,
        authoring_metadata::write_compact,
    ))
    .await;
    for exported in &flow.exported {
        assert!(exported.sidecar().project.assets.is_empty());
        assert_eq!(exported.files().len(), 2);
    }
    for accepted in &flow.accepted {
        assert!(accepted.service().compiler_drcs_resources().is_empty());
        assert!(accepted
            .service()
            .declared()
            .resource_inventory()
            .is_empty());
        for page in &accepted.service().declared().semantic().base().pages {
            assert_eq!(page.national_option.get(), context.primary & 7);
        }
    }
    let expected = expected_request(&flow, context);
    let request = if let Some(directory) = editor {
        genuine_edit(directory, &flow, context).await
    } else {
        expected_request(&flow, context)
    };
    national_editor::assert_files(&request.files, &expected.files);
    assert_eq!(request.sidecar, expected.sidecar);
    let concept = import(&fixture, request).await;
    let restored = drcs_bitmap::restored(&fixture, &concept).await;
    let original = flow.saved[0].candidate();
    assert_eq!(restored.selection(), original.selection());
    assert_eq!(restored.metadata(), original.metadata());
    assert_eq!(restored.transmissions(), original.transmissions());
    assert_eq!(restored.sources().len(), original.sources().len());
    for (before, after) in original.sources().iter().zip(restored.sources()) {
        assert_eq!(before.bytes(), after.bytes());
        assert_eq!(before.authoring(), after.authoring());
    }
    let preview = external_editor_probe::preview_page(restored.encoded().unwrap(), 124);
    assert_preview(
        &external_editor_probe::preview_page(flow.accepted[0].service(), 124),
        &preview,
    );
    assert_eq!(
        external_editor_probe::preview_page(restored.encoded().unwrap(), 125),
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
    if publish {
        let accepted = publish_and_accept(&fixture, &flow, &concept).await;
        assert_eq!(
            external_editor_probe::preview_page(accepted.service(), 124),
            preview
        );
        assert_retained_storage(&fixture, &flow).await;
        fixture = fixture.reopen().await;
        generations::assert_same_output_width(&recover(&fixture, 1).await, &accepted, 480);
        generations::assert_same_output_width(&recover(&fixture, 2).await, &flow.accepted[1], 480);
        national_editor::assert_files(
            drcs_bitmap::reexport(&fixture, &concept).await.files(),
            &expected.files,
        );
    }
    if let Some(directory) = editor {
        qtm::json_new(
            directory,
            "success.json",
            &json!({
                "scope":"Actual pinned headed Level1.5 native G0 input, concept import/shared preview/reexport/SQLite reopen; explicit publication only when requested, no new stream/client claim",
                "explicit_publication_and_cold_recovery":publish,
                "context":context.name,"primary":context.primary,"header_national_option":context.primary&7,
                "headline":context.headline,"sentinels":context.sentinels,"from":context.from,"to":context.to,"typed":context.typed,
                "row":1,"column":8,"resource_assets":0,"original_x26_records":original_x26_records(&flow),"other_records_preserved":true,
                "originals_metadata_config_receipts_and_channels_preserved_before_publication":true,
                "other_channel_preserved_after_publication":publish,"unchanged_pixels_outside_cell":true,
                "editor":"QTeletextMaker1.0/cac2167/af7cb","design_id":concept.head().id,"revision":concept.head().revision
            }),
        );
    }
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn native_g0_four_contexts_export_import_preview_without_publication() {
    for context in CONTEXTS {
        Box::pin(run(context, None)).await;
    }
}

#[tokio::test]
#[ignore = "requires four actual pinned headed QTM1.0 saves in fresh ERSATZRS_TELETEXT_NATIVE_G0_EDITOR_DIR"]
async fn actual_qtm_native_g0_four_contexts_edit_import_preview() {
    let root = std::path::PathBuf::from(super::super::super::client_capture::selected(
        "ERSATZRS_TELETEXT_NATIVE_G0_EDITOR_DIR",
    ));
    std::fs::create_dir(&root).expect("native editor destination must be new");
    for context in CONTEXTS {
        Box::pin(run(context, Some(&root.join(context.name)))).await;
    }
}
