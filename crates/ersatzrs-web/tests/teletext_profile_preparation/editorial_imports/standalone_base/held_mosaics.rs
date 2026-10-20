//! Supported Base spacing through the same receipt/concept/preview workflow.
//! Genuine pinned widget/menu editing is opt-in and distinct from native checks.

use super::*;

pub(super) fn response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = super::response(theme, page, subcode);
    for enhancement in value["service"]["page_enhancements"]
        .as_array_mut()
        .unwrap()
    {
        enhancement["local_program"] = Value::Null;
    }
    for page in value["service"]["pages"].as_array_mut().unwrap() {
        let mut title = page["rows"][0].clone();
        title["row"] = json!(3);
        let mut first = title["runs"][0]["style"].clone();
        first["foreground"] = json!(if theme == "day" { "cyan" } else { "green" });
        let mut second = first.clone();
        second["foreground"] = json!(if theme == "day" { "white" } else { "yellow" });
        page["rows"] = json!([
            {"row":1,"runs":[
                {"column":8,"style":first,"content":{"kind":"mosaics","cells":[38,0,25],"separated":false}},
                {"column":13,"style":second,"content":{"kind":"mosaics","cells":[38],"separated":true}}
            ]},title
        ]);
    }
    value
}

fn expected_request(flow: &Flow) -> NativeTeletextEditorialImportRequest {
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
    assert!(!records
        .iter()
        .any(|record| matches!(record, TeletextTtiRecord::Packet { row: 26, .. })));
    let cells = records
        .iter_mut()
        .find_map(|record| match record {
            TeletextTtiRecord::BaseRow { row: 1, cells } => Some(cells),
            _ => None,
        })
        .unwrap();
    assert_eq!(&cells[8..15], &[0x66, 0x20, 0x39, 0x17, 0x1a, 0x66, 0x20]);
    cells[9] = 0x1e;
    cells[14] = 0x1f;
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    request
}

async fn genuine_edit(directory: &Path, flow: &Flow) -> NativeTeletextEditorialImportRequest {
    qtm::inputs_for_layout(
        directory,
        "standalone-base-spacing",
        flow,
        [0, 0, 9],
        || {
            json!({
                "kind":"base_spacing","row":1,"hold_column":9,"release_column":14,
                "from":32,"hold":30,"release":31,"consumer_visual_columns":[9,11,12,14],
                "no_optional_resource_banks":true
            })
        },
    );
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
            if !(10..20).contains(&y) || ![9, 11, 12, 14].contains(&(x / 12)) {
                assert_eq!(
                    before.pixel(x, y),
                    after.pixel(x, y),
                    "unclaimed pixel{x}/{y}"
                );
            }
        }
    }
    let contiguous66 = [
        0x03f, 0x03f, 0x03f, 0xfc0, 0xfc0, 0xfc0, 0xfc0, 0x03f, 0x03f, 0x03f,
    ];
    let contiguous39 = [
        0xfc0, 0xfc0, 0xfc0, 0x03f, 0x03f, 0x03f, 0x03f, 0xfc0, 0xfc0, 0xfc0,
    ];
    let separated66 = [0, 0x01e, 0x01e, 0, 0x780, 0x780, 0x780, 0, 0x01e, 0x01e];
    for (column, mask, foreground) in [
        (9, contiguous66, [0, 15, 15]),
        (11, contiguous39, [0, 15, 15]),
        (12, contiguous39, [15, 15, 15]),
        (14, separated66, [15, 15, 15]),
    ] {
        for (y, row) in mask.into_iter().enumerate() {
            for x in 0..12 {
                let expected = if row & (1 << (11 - x)) != 0 {
                    foreground
                } else {
                    [0, 0, 0]
                };
                let Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(actual))) =
                    after.pixel(column * 12 + x, 10 + y)
                else {
                    panic!("unresolved held pixel")
                };
                assert_eq!(actual.get(), expected, "held cell{column} pixel{x}/{y}");
            }
        }
    }
}

fn assert_originals(
    flow: &Flow,
    restored: &ersatzrs_web::host::teletext_designs::RestoredOriginalTeletextDesign,
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
        assert_eq!(exported.files().len(), 2);
    }
    for accepted in &flow.accepted {
        assert!(accepted
            .service()
            .declared()
            .resource_inventory()
            .is_empty());
        assert!(accepted.service().compiler_drcs_resources().is_empty());
    }
    let expected = expected_request(&flow);
    let request = if let Some(directory) = editor {
        genuine_edit(directory, &flow).await
    } else {
        expected_request(&flow)
    };
    national_editor::assert_files(&request.files, &expected.files);
    assert_eq!(request.sidecar, expected.sidecar);
    let concept = import(&fixture, request).await;
    let restored = drcs_bitmap::restored(&fixture, &concept).await;
    assert_originals(&flow, &restored);
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
    let reopened = drcs_bitmap::restored(&fixture, &concept).await;
    assert_originals(&flow, &reopened);
    assert_eq!(
        external_editor_probe::preview_page(reopened.encoded().unwrap(), 124),
        preview
    );
    assert_original_pointers_with_width(&fixture, &flow, 480).await;
    assert!(fixture.playout.try_recv().is_err());
    if let Some(directory) = editor {
        qtm::json_new(
            directory,
            "success.json",
            &json!({
                "scope":"Actual pinned headed Base control menus -> SaveAs -> native concept/shared preview/reexport/SQLite reopen; accepted channels unchanged. Reuse explicit publication/ordinary stream gates; not client or fullbase/9A acceptance.",
                "editor":"QTeletextMaker1.0/cac2167/af7cb","row":1,"hold_column":9,"release_column":14,
                "expected_changes":[9,11,12,14],"originals_metadata_receipts_config_other_page_and_night_preserved":true,
                "resource_assets":0,"main_words":0,"design_id":concept.head().id,"revision":concept.head().revision
            }),
        );
    }
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn hold_release_controls_import_preview_reexport_without_publication() {
    Box::pin(run(None)).await;
}

#[tokio::test]
#[ignore = "requires actual pinned headed QTM1.0 save in fresh ERSATZRS_TELETEXT_HELD_EDITOR_DIR"]
async fn actual_qtm_hold_release_menus_import_preview_reexport_reopen() {
    let root = std::path::PathBuf::from(super::super::super::client_capture::selected(
        "ERSATZRS_TELETEXT_HELD_EDITOR_DIR",
    ));
    Box::pin(run(Some(&root))).await;
}
