//! One genuine Save As combines an existing G0 edit and a new resource call.
use super::*;
use ersatzrs_domain::TeletextEnhancementProgram;

const CASE: &str = "existing-gpop-call-insert-glyph";
const B: [u16; 10] = [0, 0x3fc, 0x306, 0x306, 0x3fc, 0x306, 0x306, 0x3fc, 0, 0];
const P: [u16; 10] = [0, 0x3fc, 0x306, 0x306, 0x3fc, 0x300, 0x300, 0x300, 0, 0];

fn isolated(mut value: Value) -> Value {
    let adaptive = value["service"]["objects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|object| object["identity"]["object_id"] == 22)
        .unwrap();
    let commands = adaptive["program"]["commands"].as_array_mut().unwrap();
    assert_eq!(commands.len(), 4);
    assert_eq!(
        commands[1],
        json!({"kind":"at_column","column":7,
        "operation":{"kind":"attributes","double_width":true,"double_height":false,
        "boxed":false,"conceal":false,"invert":false,"underline_or_separated_mosaics":false}})
    );
    // Keep the Base and all remaining catalogue commands. The inherited
    // Adaptive writer otherwise widens Base7 across the Active glyph at8.
    commands.remove(1);
    for page in value["authoring"]["pages"].as_array_mut().unwrap() {
        let regions = page["regions"].as_array_mut().unwrap();
        let inherited = regions
            .iter_mut()
            .find(|region| region["id"] == "free-national-base")
            .unwrap();
        assert_eq!(
            (
                inherited["row"].clone(),
                inherited["column"].clone(),
                inherited["width"].clone(),
                inherited["height"].clone()
            ),
            (json!(1), json!(7), json!(2), json!(1))
        );
        assert_eq!(inherited["kind"], "free");
        assert_eq!(inherited["area"], "main");
        inherited["width"] = json!(1);
        regions.push(json!({"id":"active-glyph-zone","kind":"zone","area":"main",
            "row":1,"column":8,"width":2,"height":1}));
    }
    value
}
fn original(theme: &str, page: u16, subcode: u16) -> Value {
    isolated(original_response(theme, page, subcode))
}
fn edited(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = isolated(extra_response(theme, page, subcode));
    let active = value["service"]["objects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|object| object["identity"]["object_id"] == 20)
        .unwrap();
    let command = &mut active["program"]["commands"][2];
    assert_eq!(
        *command,
        json!({"kind":"at_column","column":8,
        "operation":{"kind":"g0","code":65}})
    );
    command["operation"]["code"] = json!(66);
    value
}
struct Qualified {
    records: Vec<TeletextTtiRecord>,
    program: TeletextEnhancementProgram,
    pixels: Vec<TeletextRaster>,
    units: Vec<Vec<[u8; 46]>>,
}
async fn qualify() -> Qualified {
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        edited,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let service = flow.accepted[0].service();
    let program = service
        .declared()
        .semantic()
        .objects()
        .definition(seed::target(20))
        .unwrap()
        .program
        .clone();
    assert_eq!(program.commands().len(), 6);
    assert_eq!(
        program.commands()[2],
        TeletextEnhancementCommand::AtColumn {
            column: 8,
            operation: TeletextColumnOperation::G0 {
                code: ersatzrs_domain::TeletextGlyphCode::new(66).unwrap(),
            },
        }
    );
    assert_eq!(
        program.commands()[5],
        TeletextEnhancementCommand::InvokeObject {
            target: seed::target(21),
            object_type: TeletextObjectType::Passive,
            offset: Some(ersatzrs_domain::TeletextObjectOffset::new(0, 1).unwrap()),
        }
    );
    let qualified = Qualified {
        records: editor_records(&flow.exported[0]),
        program,
        pixels: preview(service),
        units: service
            .pages()
            .iter()
            .map(|page| page.data_units().to_vec())
            .collect(),
    };
    fixture.state.pool.close().await;
    qualified
}
fn assert_pixels(flow: &Flow, pixels: &[TeletextRaster]) {
    assert_eq!(pixels.len(), 3);
    for (page, raster) in (124..127).zip(pixels) {
        let before = external_editor_probe::preview_page(flow.accepted[0].service(), page);
        assert_eq!((raster.width(), raster.height()), (672, 250));
        for y in 0..250 {
            for x in 0..672 {
                let expected = if (144..156).contains(&x) && (10..30).contains(&y) {
                    Some(if B[(y - 10) / 2] & (1 << (11 - (x - 144) / 2)) != 0 {
                        [15, 15, 15]
                    } else {
                        [0, 0, 0]
                    })
                } else if (192..204).contains(&x) && (50..60).contains(&y) {
                    Some(if P[y - 50] & (1 << (11 - (x - 192))) != 0 {
                        [0, 15, 0]
                    } else {
                        [0, 0, 0]
                    })
                } else {
                    None
                };
                if let Some(expected) = expected {
                    assert!(
                        matches!(raster.pixel(x,y),
                        Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(rgb))) if rgb.get()==expected),
                        "fixed B/P page{page} at{x},{y}"
                    );
                } else if !((156..168).contains(&x) && (10..30).contains(&y)) {
                    assert_eq!(
                        raster.pixel(x, y),
                        before.pixel(x, y),
                        "outside mixed B/P footprint"
                    );
                }
            }
        }
        for start in [10, 20] {
            assert!(
                (start..start + 10)
                    .any(|y| (144..156).any(|x| raster.pixel(x, y) != before.pixel(x, y))),
                "both enlarged halves change A to B"
            );
        }
    }
}
fn meaningful(records: &[TeletextTtiRecord]) -> Vec<[u8; 3]> {
    let mut result = Vec::new();
    let mut next = 0;
    for record in records {
        if let TeletextTtiRecord::Packet {
            row: 26,
            designation,
            words,
        } = record
        {
            assert_eq!(usize::from(*designation), next);
            next += 1;
            for word in words {
                result.push([
                    u8::try_from(word & 63).unwrap(),
                    u8::try_from((word >> 6) & 31).unwrap(),
                    u8::try_from((word >> 11) & 127).unwrap(),
                ]);
            }
        }
    }
    assert!(next > 0 && next <= 16);
    let end = result.iter().position(|word| *word == [63, 31, 5]).unwrap();
    assert!(result[end..].iter().all(|word| *word == [63, 31, 5]));
    result.truncate(end + 1);
    result
}
fn stage(directory: &Path, flow: &Flow, target: &Qualified) -> Value {
    std::fs::create_dir(directory).expect("fresh combined resource editor stage");
    let original = directory.join("original");
    std::fs::create_dir(&original).unwrap();
    let exported = &flow.exported[0];
    let sidecar = write_teletext_export_sidecar(exported.sidecar(), policy().project).unwrap();
    live_client::write_new(&original.join("project.ersatzrs.json"), &sidecar).unwrap();
    for file in exported.files() {
        live_client::write_new(&original.join(&file.name), &file.bytes).unwrap();
    }
    let expected = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(target.records.clone(), policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    live_client::write_new(&directory.join("expected-editor.tti"), &expected).unwrap();
    let before = meaningful(&editor_records(exported));
    let after = meaningful(&target.records);
    assert_eq!((before.len(), after.len()), (20, 22));
    assert_eq!((before[5], after[5]), ([8, 9, 65], [8, 9, 66]));
    assert_eq!(
        (before[7], after[7], after[9]),
        ([40, 16, 0], [40, 16, 0], [40, 16, 1])
    );
    assert_eq!(
        (before[8], after[8], after[10]),
        ([40, 19, 16], [40, 19, 18], [40, 19, 18])
    );
    let name = companion(exported);
    let file = exported
        .files()
        .iter()
        .find(|file| file.name == name)
        .unwrap();
    let edit = json!({"kind":"resource_call_topology_mixed","action":"insert","identity":seed::target(20),
            "target":seed::target(21),"object_type":"passive","source_command":4,"glyph_source_command":2,
            "original_pair_words":[7,8],"added_pair_words":[9,10],"editor_word":7,"address":40,"mode":16,
            "glyph_word":5,"glyph_column":8,"glyph_mode":9,"from_glyph":65,"to_glyph":66,
            "from_calls":1,"to_calls":2,"model_before":before.len(),"model_after":after.len(),
            "original_words":before,"expected_words":after,"protected_context":true,"consumers":[124,125,126]});
    let ready = json!({"probe_version":1,"case":CASE,"editor_profile":exported.sidecar().project.editor_profile,
        "receipt_id":flow.receipts[0].metadata.id,"receipt_digest":flow.receipts[0].metadata.digest,
        "input":format!("original/{name}"),"input_sha256":drcs_bitmap::hash(&file.bytes),
        "sidecar":"original/project.ersatzrs.json","sidecar_sha256":drcs_bitmap::hash(&sidecar),
        "files":exported.files().iter().map(|f|json!({"name":f.name,"bytes":f.bytes.len(),"sha256":drcs_bitmap::hash(&f.bytes)})).collect::<Vec<_>>(),
        "protected_files":exported.files().iter().filter(|f|f.name!=name).map(|f|f.name.as_str()).collect::<Vec<_>>(),
        "expected_file":"expected-editor.tti","expected_sha256":drcs_bitmap::hash(&expected),
        "page":{"page":256,"subcode":0},"editor_geometry":{"left_columns":0,"right_columns":0,"total_columns":40},
        "edit":edit,
        "save_as":"edited.tti","completion":"editor-saved.json","timeout_seconds":600,
        "editor":{"name":"QTeletextMaker","version":"1.0","source_commit":"cac2167bac933a6160cca45a45d5d4ee95355898",
            "binary_sha256":"af7cb11220a411011917db0739f27a35b5c6ab725552b52b8a09bc01e296568a"},
        "publication":"import unpublished concept and reopen before separate explicit publication"});
    editor::json_new(directory, "ready.json", &ready);
    ready
}
async fn run_actual(
    fixture: &Fixture,
    flow: &Flow,
    directory: &Path,
    target: &Qualified,
) -> EditorOutput {
    let ready = stage(directory, flow, target);
    let completion = serde_json::to_value(editor::completion(directory).await).unwrap();
    let saved =
        live_client::read_bounded(&directory.join("edited.tti"), policy().tti.max_bytes).unwrap();
    let concept = import(
        fixture,
        saved_request(
            directory,
            &flow.exported[0],
            &flow.receipts[0],
            1,
            &saved,
            &target.records,
        ),
    )
    .await;
    assert_eq!(concept.head().revision, 2);
    EditorOutput {
        ready,
        completion,
        saved,
        concept,
    }
}
async fn check(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    target: &Qualified,
) {
    let stored = drcs_bitmap::stored(fixture, concept).await;
    let document = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    let overlay = document.editorial_overlay().unwrap();
    assert_eq!(overlay.schema_version(), 17);
    assert!(overlay.pages().is_empty());
    assert!(overlay.local_program_operands().is_empty());
    assert!(overlay.local_object_operands().is_empty());
    assert!(overlay.local_object_glyph_program_overlays().is_empty());
    assert!(overlay.resource_object_operands().is_empty());
    assert!(overlay.drcs_bitmap_overlays().is_empty());
    let changes = overlay.resource_object_program_overlays();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].identity(), seed::target(20));
    let original = flow.accepted[0].service();
    assert_eq!(
        changes[0].expected(),
        &original
            .declared()
            .semantic()
            .objects()
            .definition(seed::target(20))
            .unwrap()
            .program
    );
    assert_eq!(changes[0].replacement(), &target.program);
    assert_eq!(
        changes[0].retained_commands(),
        [Some(0), Some(1), Some(2), Some(3), Some(4), None]
    );
    let restored = drcs_bitmap::restored(fixture, concept).await;
    let root = flow.saved[0].candidate();
    assert_eq!(restored.selection(), root.selection());
    assert_eq!(restored.metadata(), root.metadata());
    assert_eq!(restored.transmissions(), root.transmissions());
    assert_eq!(restored.sources().len(), root.sources().len());
    for (source, before) in restored.sources().iter().zip(root.sources()) {
        assert_eq!(source.bytes(), before.bytes());
        assert_eq!(source.authoring(), before.authoring());
    }
    assert_eq!(
        restored.producers()[0].original(),
        root.producers()[0].original()
    );
    let expected = original
        .declared()
        .clone()
        .with_resource_object_program_overlays(changes, policy().encoding.validation.service)
        .unwrap();
    let service = restored.encoded().unwrap();
    assert_eq!(service.declared(), &expected);
    assert_eq!(service.inventory(), original.inventory());
    assert_eq!(service.magazine_units(), original.magazine_units());
    assert_eq!(preview(service), target.pixels);
    assert_units(service, target);
}
fn assert_units(service: &EncodedTeletextAuthoredService, target: &Qualified) {
    assert_eq!(service.pages().len(), target.units.len());
    for (page, units) in service.pages().iter().zip(&target.units) {
        assert_eq!(
            page.data_units(),
            units.as_slice(),
            "complete independently compiled native page"
        );
    }
}

#[tokio::test]
#[ignore = "requires pinned headed QTM combined glyph/call driver and fresh explicit proof directory"]
async fn actual_qtm_same_column_g0_and_resource_call_insert_publish_and_reopen() {
    let root = std::path::PathBuf::from(crate::client_capture::selected(
        "ERSATZRS_TELETEXT_RESOURCE_CALL_MIXED_EDITOR_DIR",
    ));
    assert!(root.is_absolute());
    std::fs::create_dir(&root).expect("fresh actual combined resource proof root");
    let target = Box::pin(qualify()).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        original,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    assert_pixels(&flow, &target.pixels);
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let directory = root.join(CASE);
    let output = Box::pin(run_actual(&fixture, &flow, &directory, &target)).await;
    Box::pin(check(&fixture, &flow, &output.concept, &target)).await;
    assert_original_pointers(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    Box::pin(check(&fixture, &flow, &output.concept, &target)).await;
    assert_original_pointers(&fixture, &flow).await;
    let day = publish_and_accept(&fixture, &flow, &output.concept).await;
    assert_eq!(preview(day.service()), target.pixels);
    assert_units(day.service(), &target);
    fixture = fixture.reopen().await;
    Box::pin(check(&fixture, &flow, &output.concept, &target)).await;
    assert_pointers(&fixture, &day, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert_staged(&directory, &flow.exported[0]);
    assert_eq!(
        live_client::read_bounded(&directory.join("edited.tti"), policy().tti.max_bytes).unwrap(),
        output.saved
    );
    assert!(fixture.playout.try_recv().is_err());
    editor::json_new(
        &root,
        "success.json",
        &json!({"probe_version":1,"editor":output.completion,
        "ready_sha256":drcs_bitmap::hash(&serde_json::to_vec_pretty(&output.ready).unwrap()),
        "saved_sha256":drcs_bitmap::hash(&output.saved),"schema_version":17,"concept_revision":2,
        "literal_enlarged_b_and_added_p":true,"all_three_native_previews_exact":true,
        "all_native_page_units_equal_independent_target":true,"unpublished_concept_reopened":true,
        "explicit_publication_accepted_then_cold_reopened":true,
        "original_catalogue_profile_package_other_channel_unchanged":true,
        "ordinary_stream_or_client_acceptance_claim":false}),
    );
    fixture.state.pool.close().await;
}
