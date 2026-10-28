//! Opt-in pinned-editor boundary for both explicit secondary-default choices.
//! Native validation/publication is mandatory; GUI markers alone prove nothing.
use super::*;
use ersatzrs_web::host::teletext_editorial_designs::NativeTeletextEditorialTtiError;
use serde::Deserialize;
use std::{path::Path, time::Duration};

const DIRECTORY_ENV: &str = "ERSATZRS_QTM_SECONDARY_DEFAULT_DIR";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EditorSaved {
    probe_version: u8,
    editor_name: String,
    editor_version: String,
    editor_source_commit: String,
    editor_binary_sha256: String,
    saved_file: String,
}

fn contribution(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = response(theme, page, subcode);
    for page in value["authoring"]["pages"].as_array_mut().unwrap() {
        // ESC changes the full row tail, so its declared zone must cover it.
        page["regions"][0]["width"] = json!(39);
    }
    value
}

fn row(file: &TeletextExportFile) -> [u8; 40] {
    let parsed = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
    parsed
        .records()
        .iter()
        .find_map(|record| match record {
            TeletextTtiRecord::BaseRow { row: 2, cells } => Some(*cells),
            _ => None,
        })
        .unwrap()
}

fn has_presentation(file: &TeletextExportFile) -> bool {
    parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .iter()
        .any(|record| {
            matches!(
                record,
                TeletextTtiRecord::Packet {
                    row: 28,
                    designation: 0,
                    ..
                }
            )
        })
}

fn without_presentation(file: &mut TeletextExportFile) {
    let parsed = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
    let records = parsed
        .records()
        .iter()
        .filter(|record| {
            !matches!(
                record,
                TeletextTtiRecord::Packet {
                    row: 28,
                    designation: 0,
                    ..
                }
            )
        })
        .cloned()
        .collect();
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
}

#[tokio::test]
async fn qtm_default_packet_omission_requires_retained_no_second_and_preserves_packet_guards() {
    for choice in [agreement(None), agreement(Some(0x10))] {
        let (fixture, flow) = setup_with_response(contribution).await;
        let chosen = choose(&fixture, &flow.receipts[0], Some(choice)).await;
        let mut edited = body_request(&fixture, &chosen).await;
        let name = edited.files[0].name.clone();
        edit_cell(&mut edited.files, &name, 2, 8, b'Q');
        let file = edited
            .files
            .iter_mut()
            .find(|file| file.name == name)
            .unwrap();
        assert!(has_presentation(file));
        without_presentation(file);
        let result = import_teletext_editorial_design(
            &fixture.state.application,
            edited,
            policy(),
            designs::deadline(),
        )
        .await;
        if choice.secondary().is_none() {
            let TeletextDesignMutation::Saved(saved) = result.unwrap() else {
                panic!("only the sealed no-second omission must be recoverable")
            };
            inspect(&fixture, &flow, &saved, Some(choice)).await;
        } else {
            assert!(matches!(
                result,
                Err(NativeTeletextEditorialImportError::Tti(
                    NativeTeletextEditorialTtiError {
                        row: Some(28),
                        designation: Some(0),
                        ..
                    }
                ))
            ));
            assert_original_pointers_with_width(&fixture, &flow, 40 * 12).await;
        }
        fixture.state.pool.close().await;
    }
    let (fixture, flow) = setup_with_response(contribution).await;
    let chosen = choose(&fixture, &flow.receipts[0], Some(agreement(None))).await;
    let mut edited = body_request(&fixture, &chosen).await;
    let file = &mut edited.files[0];
    let parsed = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
    let mut records = parsed.records().to_vec();
    let record = records
        .iter_mut()
        .find(|record| {
            matches!(
                record,
                TeletextTtiRecord::Packet {
                    row: 28,
                    designation: 0,
                    ..
                }
            )
        })
        .unwrap();
    let TeletextTtiRecord::Packet { words, .. } = record else {
        unreachable!()
    };
    words[0] ^= 1 << 7; // Uploaded primary bits never authorize a new context.
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    assert!(matches!(
        import_teletext_editorial_design(
            &fixture.state.application,
            edited,
            policy(),
            designs::deadline(),
        )
        .await,
        Err(NativeTeletextEditorialImportError::Tti(_))
    ));
    assert_original_pointers_with_width(&fixture, &flow, 40 * 12).await;
    fixture.state.pool.close().await;
}

async fn actual_request(
    fixture: &Fixture,
    concept: &SavedEditorialTeletextDesign,
    directory: &Path,
) -> NativeTeletextEditorialImportRequest {
    let exported = drcs_bitmap::reexport(fixture, concept).await;
    let name = exported.sidecar().project.pages[0].name.clone();
    let current = receipt(fixture, &exported).await;
    let mut request = drcs_bitmap::from_receipt(&current, concept.head().revision);
    let original = request.files.iter().find(|f| f.name == name).unwrap();
    let mut expected = row(original);
    assert_eq!(&expected[1..3], &[0x1b, 0x23]);
    assert_eq!(expected[8], b' ');
    assert!(has_presentation(original));
    expected[8] = b'Q';
    let files: Vec<_> = request
        .files
        .iter()
        .map(|file| {
            live_client::write_new(&directory.join(&file.name), &file.bytes).unwrap();
            json!({"name":file.name,"bytes":file.bytes.len(),"sha256":live_client::hash(&file.bytes)})
        })
        .collect();
    live_client::write_new(&directory.join("project.json"), &request.sidecar).unwrap();
    polish_delivery::write_json(
        directory,
        "ready.json",
        &json!({"probe_version":1,"editor_input":name,"sidecar":"project.json","files":files,
        "edit":{"row":2,"column":8,"code":81},"editor_geometry":{"left_columns":0,"right_columns":0}}),
    );
    let saved: EditorSaved = tokio::time::timeout(
        Duration::from_mins(3),
        resource_bank_admin::marker(directory, "editor-saved.json"),
    )
    .await
    .expect("bounded existing editor trial deadline")
    .unwrap();
    assert_eq!(saved.probe_version, 1);
    assert_eq!(saved.editor_name, "QTeletextMaker");
    assert_eq!(saved.editor_version, "1.0");
    assert_eq!(
        saved.editor_source_commit,
        "cac2167bac933a6160cca45a45d5d4ee95355898"
    );
    assert_eq!(
        saved.editor_binary_sha256,
        "af7cb11220a411011917db0739f27a35b5c6ab725552b52b8a09bc01e296568a"
    );
    assert_eq!(saved.saved_file, "edited.tti");
    assert_eq!(
        live_client::read_bounded(&directory.join("project.json"), policy().project.max_bytes)
            .unwrap(),
        request.sidecar
    );
    for file in &request.files {
        assert_eq!(
            live_client::read_bounded(&directory.join(&file.name), policy().project.max_bytes)
                .unwrap(),
            file.bytes
        );
    }
    let file = request.files.iter_mut().find(|f| f.name == name).unwrap();
    file.bytes = live_client::read_bounded(
        &directory.join(saved.saved_file),
        policy().project.max_bytes,
    )
    .unwrap();
    assert_eq!(
        row(file),
        expected,
        "only the actual Q key may change the base row"
    );
    polish_delivery::write_json(
        directory,
        "saved-presentation.json",
        &json!({"input_x28_0":true,"saved_x28_0":has_presentation(file)}),
    );
    request
}

fn preview(service: &ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService) -> TeletextRaster {
    let preview = project_teletext_preview(
        service,
        channel_documents::address(124, 32),
        TeletextColourPixelOptions {
            reveal_concealed: false,
            flash_phase: TeletextFlashPhase::Original,
        },
        preview_budget(),
    )
    .unwrap();
    assert!(preview.raster().diagnostics().is_empty());
    assert_eq!(preview.navigation()[0].target.page.get(), 124);
    preview.raster().clone()
}

async fn run_case(directory: &Path, choice: TeletextSecondaryDefaultAgreement) {
    std::fs::create_dir(directory).unwrap();
    let (mut fixture, flow) = setup_with_response(contribution).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let chosen = choose(&fixture, &flow.receipts[0], Some(choice)).await;
    let mut input = body_request(&fixture, &chosen).await;
    let name = input.files[0].name.clone();
    edit_cell(&mut input.files, &name, 2, 1, 0x1b);
    edit_cell(&mut input.files, &name, 2, 2, 0x23);
    let prepared = import(&fixture, input).await;
    assert_eq!(prepared.head().revision, 3);
    inspect(&fixture, &flow, &prepared, Some(choice)).await;
    let request = actual_request(&fixture, &prepared, directory).await;
    let expected = row(&request.files[0]);
    let concept = import(&fixture, request).await;
    assert_eq!(concept.head().revision, 4);
    inspect(&fixture, &flow, &concept, Some(choice)).await;
    let reexported = drcs_bitmap::reexport(&fixture, &concept).await;
    assert_eq!(row(&reexported.files()[0]), expected);
    assert!(has_presentation(&reexported.files()[0]));
    retained
        .assert_unchanged_width(&fixture, &flow, 40 * 12)
        .await;
    fixture = fixture.reopen().await;
    inspect(&fixture, &flow, &concept, Some(choice)).await;
    let restored = reload_original_teletext_design_revision(
        &fixture.state.application,
        concept.head().id,
        concept.head().revision,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    let raster = preview(restored.encoded().unwrap());
    let accepted = publish_and_accept(&fixture, &flow, &concept).await;
    assert_eq!(preview(accepted.service()), raster);
    fixture = fixture.reopen().await;
    generations::assert_same_output_width(&recover(&fixture, 1).await, &accepted, 40 * 12);
    retained
        .assert_unchanged_width(&fixture, &flow, 40 * 12)
        .await;
    polish_delivery::write_json(
        directory,
        "native-complete.json",
        &json!({
            "probe_version":1,"agreement":choice,"design_id":concept.head().id,"revision":concept.head().revision,
            "digest":concept.digest(),"generation_id":accepted.generation().id,"request_id":accepted.generation().request.id,
            "original_sources_and_unknown_provenance_retained":true,"preview_equals_publication":true,
            "separate_publication":true,"database_reopened":true,"other_channel_and_configuration_unchanged":true,
            "ordinary_stream_retested":false,"receiver_qualified":false
        }),
    );
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
#[ignore = "requires pinned actual QTM edit/Save As through existing GUI driver; no stream/client claim"]
async fn actual_qtm_secondary_default_choices_preserve_companion_preview_and_publication() {
    let directory = std::path::PathBuf::from(client_capture::selected(DIRECTORY_ENV));
    for (label, choice) in [
        ("no-second", agreement(None)),
        ("alias-10", agreement(Some(0x10))),
    ] {
        Box::pin(run_case(&directory.join(label), choice)).await;
    }
}
