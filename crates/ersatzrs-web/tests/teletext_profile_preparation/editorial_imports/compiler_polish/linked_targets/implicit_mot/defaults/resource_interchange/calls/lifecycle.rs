//! Existing catalogue calls retain root provenance across retargeting and undo.
use super::*;

fn lifecycle_response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = original_response(theme, page, subcode);
    for page in value["authoring"]["pages"].as_array_mut().unwrap() {
        let region = page["regions"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|r| r["id"] == "free-title")
            .unwrap();
        assert_eq!(
            (region["row"].as_u64(), region["column"].as_u64()),
            (Some(2), Some(1))
        );
        assert_eq!(region["width"], 20);
        // Capture the entire remaining Main row BEFORE export. Adaptive context,
        // not only its visible X, must be covered by original permissions.
        region["width"] = json!(39);
    }
    value
}

fn retargeted_response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = lifecycle_response(theme, page, subcode);
    let objects = value["service"]["objects"].as_array_mut().unwrap();
    let target = objects
        .iter()
        .find(|o| o["identity"]["object_id"] == 22)
        .unwrap()["identity"]
        .clone();
    let active = objects
        .iter_mut()
        .find(|o| o["identity"]["object_id"] == 20)
        .unwrap();
    active["program"]["commands"][4] = json!({"kind":"invoke_object",
        "target":target,"object_type":"adaptive","offset":{"row":0,"column":0}});
    value
}

fn pixels(service: &EncodedTeletextAuthoredService) -> Vec<TeletextRaster> {
    (124..127)
        .map(|p| external_editor_probe::preview_page(service, p))
        .collect()
}

async fn qualified_retarget() -> (Vec<TeletextTtiRecord>, Vec<TeletextRaster>) {
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        retargeted_response,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let definition = flow.accepted[0]
        .service()
        .declared()
        .semantic()
        .objects()
        .definition(seed::target(20))
        .unwrap();
    assert_eq!(
        definition.program.commands()[4],
        call(22, TeletextObjectType::Adaptive, 0)
    );
    let file = flow.exported[0]
        .files()
        .iter()
        .find(|f| f.name == companion(&flow.exported[0]))
        .unwrap();
    let records = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    let result = pixels(flow.accepted[0].service());
    fixture.state.pool.close().await;
    (records, result)
}

fn call(id: u16, object_type: TeletextObjectType, column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::InvokeObject {
        target: seed::target(id),
        object_type,
        offset: Some(ersatzrs_domain::TeletextObjectOffset::new(0, column).unwrap()),
    }
}

fn replace_call(
    exported: &ExportedOriginalTeletextDesign,
    receipt: &TeletextDesignExportReceipt,
    head: i64,
    target: &[TeletextTtiRecord],
) -> NativeTeletextEditorialImportRequest {
    let mut request = drcs_bitmap::from_receipt(receipt, head);
    let name = companion(exported);
    let originals = request.files.clone();
    let sidecar = request.sidecar.clone();
    let file = request.files.iter_mut().find(|f| f.name == name).unwrap();
    let baseline = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
    let mut changed = baseline.records().to_vec();
    let source = changed
        .iter_mut()
        .find_map(|r| match r {
            TeletextTtiRecord::Packet {
                row: 26,
                designation: 0,
                words,
            } => Some(words),
            _ => None,
        })
        .unwrap();
    let destination = target
        .iter()
        .find_map(|r| match r {
            TeletextTtiRecord::Packet {
                row: 26,
                designation: 0,
                words,
            } => Some(words),
            _ => None,
        })
        .unwrap();
    // Only the existing call's origin modifier and allocated invocation change.
    source[7] = destination[7];
    source[8] = destination[8];
    assert_eq!(
        changed, target,
        "every other wrapper/body word and TTI record is immutable"
    );
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(changed, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    assert_eq!(request.sidecar, sidecar);
    for (old, new) in originals.iter().zip(&request.files) {
        assert_eq!(old.name, new.name);
        if old.name != name {
            assert_eq!(old.bytes, new.bytes);
        }
    }
    request
}

async fn original_changes(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    expected_call: TeletextEnhancementCommand,
) -> Vec<ersatzrs_domain::TeletextResourceObjectProgramOverlay> {
    let stored = drcs_bitmap::stored(fixture, concept).await;
    let document = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    let root = flow.accepted[0].service();
    let definition = root
        .declared()
        .semantic()
        .objects()
        .definition(seed::target(20))
        .unwrap();
    if expected_call == definition.program.commands()[4] {
        assert!(document.editorial_overlay().is_none());
        assert_eq!(stored.document, flow.originals[0].document);
        vec![]
    } else {
        let overlay = document.editorial_overlay().unwrap();
        assert_eq!(overlay.schema_version(), 16);
        assert!(overlay.pages().is_empty());
        assert!(overlay.local_program_operands().is_empty());
        assert!(overlay.local_object_operands().is_empty());
        assert!(overlay.local_object_glyph_program_overlays().is_empty());
        assert!(overlay.resource_object_operands().is_empty());
        assert!(overlay.drcs_bitmap_overlays().is_empty());
        let changes = overlay.resource_object_program_overlays();
        assert_eq!(changes.len(), 1);
        let change = &changes[0];
        assert_eq!(change.identity(), seed::target(20));
        assert_eq!(change.expected(), &definition.program);
        let mut replacement = definition.program.commands().to_vec();
        replacement[4] = expected_call;
        assert_eq!(change.replacement().commands(), replacement);
        assert_eq!(
            change.retained_commands(),
            &[Some(0), Some(1), Some(2), Some(3), Some(4)]
        );
        changes.to_vec()
    }
}

async fn assert_concept(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    expected_call: TeletextEnhancementCommand,
    expected_pixels: &[TeletextRaster],
) {
    let changes = original_changes(fixture, flow, concept, expected_call).await;
    let restored = drcs_bitmap::restored(fixture, concept).await;
    let original = flow.saved[0].candidate();
    assert_eq!(restored.selection(), original.selection());
    assert_eq!(restored.metadata(), original.metadata());
    assert_eq!(restored.transmissions(), original.transmissions());
    assert_eq!(restored.sources().len(), original.sources().len());
    for (source, before) in restored.sources().iter().zip(original.sources()) {
        assert_eq!(source.bytes(), before.bytes());
        assert_eq!(source.authoring(), before.authoring());
    }
    assert_eq!(
        restored.producers()[0].original(),
        original.producers()[0].original()
    );
    let expected = if changes.is_empty() {
        flow.accepted[0].service().declared().clone()
    } else {
        flow.accepted[0]
            .service()
            .declared()
            .clone()
            .with_resource_object_program_overlays(&changes, policy().encoding.validation.service)
            .unwrap()
    };
    let service = restored.encoded().unwrap();
    let root = flow.accepted[0].service();
    assert_eq!(service.declared(), &expected);
    assert_eq!(service.magazine_units(), root.magazine_units());
    assert_eq!(service.inventory(), root.inventory());
    assert_eq!(pixels(service).as_slice(), expected_pixels);
    for (before, after) in root.pages().iter().zip(service.pages()) {
        if before
            .resource_header()
            .is_none_or(|h| Some(h.identity()) != seed::target(20).resource())
        {
            assert_eq!(
                before.data_units(),
                after.data_units(),
                "other native pages/resources stay exact"
            );
        }
    }
}

async fn undo_and_repeat(
    fixture: Fixture,
    flow: &Flow,
    retarget: &SavedEditorialTeletextDesign,
    retargeted: &[TeletextTtiRecord],
    expected_pixels: &[TeletextRaster],
    root_preview: &[TeletextRaster],
) -> Fixture {
    let original_file = flow.exported[0]
        .files()
        .iter()
        .find(|f| f.name == companion(&flow.exported[0]))
        .unwrap();
    let original_records = parse_teletext_tti(&original_file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    let exported = drcs_bitmap::reexport(&fixture, retarget).await;
    let latest = receipt(&fixture, &exported).await;
    let undone = import(
        &fixture,
        replace_call(&exported, &latest, 3, &original_records),
    )
    .await;
    assert_eq!(undone.head().revision, 4);
    Box::pin(assert_concept(
        &fixture,
        flow,
        &undone,
        call(21, TeletextObjectType::Passive, 0),
        root_preview,
    ))
    .await;
    let exported = drcs_bitmap::reexport(&fixture, &undone).await;
    let latest = receipt(&fixture, &exported).await;
    let repeated = import(&fixture, replace_call(&exported, &latest, 4, retargeted)).await;
    assert_eq!(repeated.head().revision, 5);
    let fixture = fixture.reopen().await;
    Box::pin(assert_concept(
        &fixture,
        flow,
        &repeated,
        call(22, TeletextObjectType::Adaptive, 0),
        expected_pixels,
    ))
    .await;
    fixture
}

#[tokio::test]
async fn existing_resource_calls_retarget_publish_reopen_repeat_and_undo_root_intent() {
    let (translated, translated_preview) = Box::pin(qualified_target()).await;
    let (retargeted, retargeted_preview) = Box::pin(qualified_retarget()).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        lifecycle_response,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let root_preview = translated_pixels(flow.accepted[0].service(), 0);
    assert_ne!(retargeted_preview, root_preview);
    assert_ne!(retargeted_preview, translated_preview);
    let offset = import(&fixture, request(&flow, &translated)).await;
    assert_eq!(offset.head().revision, 2);
    Box::pin(assert_concept(
        &fixture,
        &flow,
        &offset,
        call(21, TeletextObjectType::Passive, 1),
        &translated_preview,
    ))
    .await;
    assert_original_pointers(&fixture, &flow).await;
    let day = publish_and_accept(&fixture, &flow, &offset).await;
    assert_eq!(pixels(day.service()), translated_preview);
    let exported = drcs_bitmap::reexport(&fixture, &offset).await;
    let latest = receipt(&fixture, &exported).await;
    let retarget = import(&fixture, replace_call(&exported, &latest, 2, &retargeted)).await;
    assert_eq!(retarget.head().revision, 3);
    Box::pin(assert_concept(
        &fixture,
        &flow,
        &retarget,
        call(22, TeletextObjectType::Adaptive, 0),
        &retargeted_preview,
    ))
    .await;
    fixture = fixture.reopen().await;
    Box::pin(assert_concept(
        &fixture,
        &flow,
        &retarget,
        call(22, TeletextObjectType::Adaptive, 0),
        &retargeted_preview,
    ))
    .await;
    assert_pointers(&fixture, &day, &flow.accepted[1]).await;
    fixture = Box::pin(undo_and_repeat(
        fixture,
        &flow,
        &retarget,
        &retargeted,
        &retargeted_preview,
        &root_preview,
    ))
    .await;
    assert_pointers(&fixture, &day, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
