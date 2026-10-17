//! Explicit resource Add, fresh receipt and edited concept reuse one native flow.
use super::*;
use ersatzrs_domain::TeletextEnhancementObjectDefinition;
use ersatzrs_web::host::teletext_editorial_designs::{
    add_teletext_editorial_resource_definitions, NativeTeletextResourceDefinitionAddRequest,
};

#[path = "definition_additions/actual_editor.rs"]
mod actual_editor;

#[path = "definition_additions/admin_copy.rs"]
mod admin_copy;

#[path = "definition_additions/copy_controls.rs"]
mod copy_controls;

#[derive(Clone, Copy)]
enum State {
    Dormant,
    Called,
    Edited,
}
fn response(theme: &str, page: u16, subcode: u16, state: State) -> Value {
    let mut value = super::response(theme, page, subcode);
    let objects = value["service"]["objects"].as_array_mut().unwrap();
    let mut added = objects
        .iter()
        .find(|object| object["identity"]["object_id"] == 21)
        .unwrap()
        .clone();
    added["identity"]["object_id"] = json!(25);
    assert_eq!(added["object_type"], "passive");
    assert_eq!(added["program"]["commands"][4]["operation"]["code"], 80);
    if matches!(state, State::Edited) {
        added["program"]["commands"][4]["operation"]["code"] = json!(81);
    }
    if !matches!(state, State::Dormant) {
        let active = objects
            .iter_mut()
            .find(|object| object["identity"]["object_id"] == 20)
            .unwrap();
        active["program"]["commands"][4]["target"] = added["identity"].clone();
    }
    objects.push(added);
    value
}
fn dormant(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, State::Dormant)
}
fn called(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, State::Called)
}
fn edited(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, State::Edited)
}
fn rasters(service: &EncodedTeletextAuthoredService) -> Vec<TeletextRaster> {
    (124..127)
        .map(|page| external_editor_probe::preview_page(service, page))
        .collect()
}
fn passive_name(exported: &ExportedOriginalTeletextDesign) -> &str {
    let resource = protected_file(exported, TeletextResourceKindWire::Pop, 0xc0);
    let asset=exported.sidecar().project.assets.iter().find(|asset| matches!(&asset.scope,
        TeletextExportAssetScope::ObjectContextTtiV1 {resource_name,object_id:25,editor_page:256,editor_ordinal:0} if resource_name==resource)).unwrap();
    &asset.name
}
fn file_records(exported: &ExportedOriginalTeletextDesign, name: &str) -> Vec<TeletextTtiRecord> {
    let bytes = &exported
        .files()
        .iter()
        .find(|file| file.name == name)
        .unwrap()
        .bytes;
    parse_teletext_tti(bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec()
}
fn word(records: &[TeletextTtiRecord], index: usize) -> u32 {
    records
        .iter()
        .find_map(|record| match record {
            TeletextTtiRecord::Packet {
                row: 26,
                designation,
                words,
            } if usize::from(*designation) == index / 13 => Some(words[index % 13]),
            _ => None,
        })
        .unwrap()
}
struct Expected {
    state: State,
    definition: TeletextEnhancementObjectDefinition,
    active: TeletextEnhancementProgram,
    call_word: u32,
    glyph_word: u32,
    rasters: Vec<TeletextRaster>,
    units: Vec<Vec<[u8; 46]>>,
}
async fn qualify(state: State) -> Expected {
    let factory: fn(&str, u16, u16) -> Value = match state {
        State::Dormant => dormant,
        State::Called => called,
        State::Edited => edited,
    };
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        factory,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    assert_eq!(
        flow.exported[0].sidecar().project.editor_profile,
        "qteletextmaker-1.0-carrier-v8"
    );
    let service = flow.accepted[0].service();
    let definition = service
        .declared()
        .semantic()
        .objects()
        .definition(seed::target(25))
        .unwrap()
        .clone();
    let active = program(service).clone();
    assert_eq!(
        active.commands()[4],
        TeletextEnhancementCommand::InvokeObject {
            target: seed::target(if matches!(state, State::Dormant) {
                21
            } else {
                25
            }),
            object_type: TeletextObjectType::Passive,
            offset: Some(ersatzrs_domain::TeletextObjectOffset::new(0, 0).unwrap()),
        }
    );
    let result = Expected {
        state,
        definition,
        active,
        call_word: word(
            &file_records(&flow.exported[0], companion(&flow.exported[0])),
            8,
        ),
        glyph_word: word(
            &file_records(&flow.exported[0], passive_name(&flow.exported[0])),
            7,
        ),
        rasters: rasters(service),
        units: service
            .pages()
            .iter()
            .map(|page| page.data_units().to_vec())
            .collect(),
    };
    assert_eq!(
        result.glyph_word,
        1 | (9 << 6)
            | (if matches!(state, State::Edited) {
                81
            } else {
                80
            } << 11)
    );
    assert_literal(&result.rasters, state);
    fixture.state.pool.close().await;
    result
}
fn add_request(
    receipt: &TeletextDesignExportReceipt,
    head: i64,
    definition: TeletextEnhancementObjectDefinition,
) -> NativeTeletextResourceDefinitionAddRequest {
    NativeTeletextResourceDefinitionAddRequest {
        name: "Explicit resource definition concept".to_owned(),
        receipt_id: receipt.metadata.id,
        receipt_digest: receipt.metadata.digest.clone(),
        expected_design_head: head,
        definitions: vec![definition],
    }
}
async fn add(
    fixture: &Fixture,
    receipt: &TeletextDesignExportReceipt,
    definition: TeletextEnhancementObjectDefinition,
) -> SavedEditorialTeletextDesign {
    let result = add_teletext_editorial_resource_definitions(
        &fixture.state.application,
        add_request(receipt, 1, definition),
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextDesignMutation::Saved(saved) = result else {
        panic!("explicit checked Add must save an unpublished concept")
    };
    assert_eq!(saved.head().revision, 2);
    saved
}
fn upload(
    exported: &ExportedOriginalTeletextDesign,
    receipt: &TeletextDesignExportReceipt,
    head: i64,
    target: &Expected,
) -> NativeTeletextEditorialImportRequest {
    let mut request = drcs_bitmap::from_receipt(receipt, head);
    let originals = request.files.clone();
    let sidecar = request.sidecar.clone();
    let active = companion(exported);
    let passive = passive_name(exported);
    for (name, index, replacement) in [
        (active, 8, target.call_word),
        (passive, 7, target.glyph_word),
    ] {
        let file = request
            .files
            .iter_mut()
            .find(|file| file.name == name)
            .unwrap();
        let mut records = parse_teletext_tti(&file.bytes, policy().tti)
            .unwrap()
            .records()
            .to_vec();
        let old = word(&records, index);
        assert_eq!((old >> 6) & 31, if index == 8 { 19 } else { 9 });
        for record in &mut records {
            if let TeletextTtiRecord::Packet {
                row: 26,
                designation,
                words,
            } = record
            {
                if usize::from(*designation) == index / 13 {
                    words[index % 13] = replacement;
                }
            }
        }
        file.bytes = write_teletext_tti(
            &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
            policy().tti,
        )
        .unwrap();
    }
    assert_eq!(request.sidecar, sidecar);
    for (before, after) in originals.iter().zip(&request.files) {
        assert_eq!(before.name, after.name);
        if before.name != active && before.name != passive {
            assert_eq!(
                before.bytes, after.bytes,
                "all original displays, sibling snapshots and native resource uploads remain exact"
            );
        }
    }
    request
}
fn assert_output(service: &EncodedTeletextAuthoredService, target: &Expected) {
    assert_literal(&rasters(service), target.state);
    assert_eq!(
        rasters(service),
        target.rasters,
        "all three full native previews"
    );
    assert_eq!(service.pages().len(), target.units.len());
    for (page, expected) in service.pages().iter().zip(&target.units) {
        assert_eq!(
            page.data_units(),
            expected,
            "complete independently encoded display/resource output"
        );
    }
}
async fn assert_concept(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    target: &Expected,
) {
    let stored = drcs_bitmap::stored(fixture, concept).await;
    let documents = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    let overlay = documents.editorial_overlay().unwrap();
    assert_eq!(overlay.schema_version(), 18);
    let [addition] = overlay.resource_object_definition_additions() else {
        panic!("exactly one explicit addition")
    };
    assert_eq!(addition.definition(), &target.definition);
    assert!(overlay
        .resource_object_operands()
        .iter()
        .all(|edit| edit.identity() != seed::target(25)));
    assert!(
        overlay
            .resource_object_program_overlays()
            .iter()
            .all(|edit| edit.identity() != seed::target(25)),
        "new body edits fold into complete addition, never absent-root program ancestry"
    );
    assert!(
        overlay.pages().is_empty()
            && overlay.local_program_operands().is_empty()
            && overlay.local_object_operands().is_empty()
            && overlay.local_object_glyph_program_overlays().is_empty()
            && overlay.drcs_bitmap_overlays().is_empty()
    );
    if matches!(target.state, State::Dormant) {
        assert!(overlay.resource_object_program_overlays().is_empty());
    } else {
        let [change] = overlay.resource_object_program_overlays() else {
            panic!("one existing Active call change")
        };
        assert_eq!(change.identity(), seed::target(20));
        assert_eq!(change.expected(), program(flow.accepted[0].service()));
        assert_eq!(change.replacement(), &target.active);
        assert_eq!(
            change.retained_commands(),
            [Some(0), Some(1), Some(2), Some(3), Some(4)]
        );
        assert!(change.has_invocation_changes());
    }
    let restored = drcs_bitmap::restored(fixture, concept).await;
    let original = flow.saved[0].candidate();
    assert_eq!(restored.selection(), original.selection());
    assert_eq!(restored.metadata(), original.metadata());
    assert_eq!(restored.transmissions(), original.transmissions());
    assert_eq!(restored.sources().len(), original.sources().len());
    for (after, before) in restored.sources().iter().zip(original.sources()) {
        assert_eq!(after.bytes(), before.bytes());
        assert_eq!(after.authoring(), before.authoring());
    }
    assert_eq!(
        restored.producers()[0].original(),
        original.producers()[0].original()
    );
    assert!(program(flow.accepted[0].service()).commands().iter().all(|command|!matches!(command,TeletextEnhancementCommand::InvokeObject {target,..} if *target==seed::target(25))));
    assert!(flow.accepted[0]
        .service()
        .declared()
        .semantic()
        .objects()
        .definition(seed::target(25))
        .is_none());
    assert_output(restored.encoded().unwrap(), target);
}
fn assert_literal(rasters: &[TeletextRaster], state: State) {
    // Pinned Latin table0 ten-row masks; the new body is not its own pixel oracle.
    let mask = if matches!(state, State::Edited) {
        [
            0, 0x1fc, 0x306, 0x306, 0x306, 0x306, 0x336, 0x1fc, 0x18, 0x0e,
        ]
    } else {
        [0, 0x3fc, 0x306, 0x306, 0x3fc, 0x300, 0x300, 0x300, 0, 0]
    };
    for raster in rasters {
        for (row, bits) in mask.into_iter().enumerate() {
            for x in 0..12 {
                let expected = if bits & (1 << (11 - x)) != 0 {
                    [0, 15, 0]
                } else {
                    [0, 0, 0]
                };
                assert!(matches!(raster.pixel(180+x,50+row),Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(rgb))) if rgb.get()==expected),"literal green P/Q at nested Main(5,11), independently of new-definition native bytes");
            }
        }
    }
}
async fn undo_repeat(
    mut fixture: Fixture,
    flow: &Flow,
    inserted: &SavedEditorialTeletextDesign,
    dormant: &Expected,
    edited: &Expected,
) -> Fixture {
    let exported = drcs_bitmap::reexport(&fixture, inserted).await;
    let latest = receipt(&fixture, &exported).await;
    let undo = import(&fixture, upload(&exported, &latest, 3, dormant)).await;
    assert_eq!(undo.head().revision, 4);
    Box::pin(assert_concept(&fixture, flow, &undo, dormant)).await;
    fixture = fixture.reopen().await;
    Box::pin(assert_concept(&fixture, flow, &undo, dormant)).await;
    let exported = drcs_bitmap::reexport(&fixture, &undo).await;
    let latest = receipt(&fixture, &exported).await;
    let repeated = import(&fixture, upload(&exported, &latest, 4, edited)).await;
    assert_eq!(repeated.head().revision, 5);
    fixture = fixture.reopen().await;
    Box::pin(assert_concept(&fixture, flow, &repeated, edited)).await;
    fixture
}
#[tokio::test]
async fn explicit_definition_add_fresh_receipt_body_fold_publish_reopen_undo_and_repeat() {
    let dormant = Box::pin(qualify(State::Dormant)).await;
    let edited = Box::pin(qualify(State::Edited)).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        super::response,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    assert_eq!(
        rasters(flow.accepted[0].service()),
        dormant.rasters,
        "adding an unused definition alone changes no display"
    );
    assert_ne!(dormant.rasters, edited.rasters);
    let added = add(&fixture, &flow.receipts[0], dormant.definition.clone()).await;
    Box::pin(assert_concept(&fixture, &flow, &added, &dormant)).await;
    assert_original_pointers(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    Box::pin(assert_concept(&fixture, &flow, &added, &dormant)).await;
    assert_original_pointers(&fixture, &flow).await;
    let exported = drcs_bitmap::reexport(&fixture, &added).await;
    assert_eq!(
        exported.sidecar().project.editor_profile,
        "qteletextmaker-1.0-carrier-v8"
    );
    assert!(
        exported
            .files()
            .iter()
            .any(|file| file.name == passive_name(&exported)),
        "fresh receipt exposes new definition for supported editor body changes"
    );
    let latest = receipt(&fixture, &exported).await;
    assert_eq!(latest.metadata.revision, 2);
    let changed = import(&fixture, upload(&exported, &latest, 2, &edited)).await;
    assert_eq!(changed.head().revision, 3);
    Box::pin(assert_concept(&fixture, &flow, &changed, &edited)).await;
    assert_original_pointers(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    Box::pin(assert_concept(&fixture, &flow, &changed, &edited)).await;
    let day = publish_and_accept(&fixture, &flow, &changed).await;
    assert_output(day.service(), &edited);
    fixture = fixture.reopen().await;
    Box::pin(assert_concept(&fixture, &flow, &changed, &edited)).await;
    assert_pointers(&fixture, &day, &flow.accepted[1]).await;
    fixture = Box::pin(undo_repeat(fixture, &flow, &changed, &dormant, &edited)).await;
    assert_pointers(&fixture, &day, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
fn generated(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = super::response(theme, page, subcode);
    let consumer = value["authoring"]["pages"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|page| page["page"]["page"] == 125)
        .unwrap();
    let region = consumer["regions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|region| region["id"] == "nested-call-zone")
        .unwrap();
    assert_eq!(
        (
            region["row"].as_u64(),
            region["column"].as_u64(),
            region["width"].as_u64()
        ),
        (Some(5), Some(11), Some(2))
    );
    region["kind"] = json!("generated");
    value
}
async fn refuse_add_controls(
    fixture: &Fixture,
    flow: &Flow,
    definition: &TeletextEnhancementObjectDefinition,
) {
    let mut absent = definition.clone();
    let mut identity = serde_json::to_value(absent.identity).unwrap();
    identity["resource"]["page"]["page"] = json!(193);
    absent.identity = serde_json::from_value(identity).unwrap();
    let occupied = flow.accepted[0]
        .service()
        .declared()
        .semantic()
        .objects()
        .definition(seed::target(21))
        .unwrap()
        .clone();
    for definition in [absent, occupied] {
        let result = add_teletext_editorial_resource_definitions(
            &fixture.state.application,
            add_request(&flow.receipts[0], 1, definition),
            policy(),
            designs::deadline(),
        )
        .await;
        assert!(
            !matches!(result, Ok(TeletextDesignMutation::Saved(_))),
            "missing resource or occupied logical identity must not save"
        );
        assert_unchanged(fixture, flow).await;
    }
    let stale = add_teletext_editorial_resource_definitions(
        &fixture.state.application,
        add_request(&flow.receipts[0], 2, definition.clone()),
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    assert!(matches!(
        stale,
        TeletextDesignMutation::Rejected(
            ersatzrs_core::repositories::TeletextDesignRejection::Conflict
        )
    ));
    assert_unchanged(fixture, flow).await;
    let expired = add_teletext_editorial_resource_definitions(
        &fixture.state.application,
        add_request(&flow.receipts[0], 1, definition.clone()),
        policy(),
        std::time::Instant::now(),
    )
    .await;
    assert!(matches!(
        expired,
        Err(NativeTeletextEditorialImportError::TimedOut)
    ));
    assert_unchanged(fixture, flow).await;
}
async fn refuse_historical_add(
    fixture: &Fixture,
    flow: &Flow,
    added: &SavedEditorialTeletextDesign,
    dormant: &Expected,
) {
    let mut definition = dormant.definition.clone();
    definition.identity = seed::target(26);
    let result = add_teletext_editorial_resource_definitions(
        &fixture.state.application,
        add_request(&flow.receipts[0], added.head().revision, definition),
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    assert!(matches!(
        result,
        TeletextDesignMutation::Rejected(
            ersatzrs_core::repositories::TeletextDesignRejection::Conflict
        )
    ));
    assert_eq!(
        fixture
            .state
            .application
            .teletext_design_head(added.head().id)
            .await
            .unwrap()
            .unwrap(),
        *added.head()
    );
    assert!(fixture
        .state
        .application
        .teletext_design_revision(added.head().id, 3, designs::budget().max_serialized_bytes)
        .await
        .unwrap()
        .is_none());
    Box::pin(assert_concept(fixture, flow, added, dormant)).await;
    assert_original_pointers(fixture, flow).await;
}
#[tokio::test]
async fn explicit_definition_add_controls_and_inserted_shared_lineage_refuse_without_publication_damage(
) {
    let dormant = Box::pin(qualify(State::Dormant)).await;
    let called = Box::pin(qualify(State::Called)).await;
    let edited = Box::pin(qualify(State::Edited)).await;
    assert_eq!(
        called.rasters, dormant.rasters,
        "retargeting to a new identical definition changes no pixels"
    );
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        generated,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    Box::pin(refuse_add_controls(&fixture, &flow, &dormant.definition)).await;
    let added = add(&fixture, &flow.receipts[0], dormant.definition.clone()).await;
    Box::pin(assert_concept(&fixture, &flow, &added, &dormant)).await;
    Box::pin(refuse_historical_add(&fixture, &flow, &added, &dormant)).await;
    retained.assert_unchanged(&fixture, &flow).await;
    let exported = drcs_bitmap::reexport(&fixture, &added).await;
    let latest = receipt(&fixture, &exported).await;
    for target in [&called, &edited] {
        let error = import_teletext_editorial_design(
            &fixture.state.application,
            upload(&exported, &latest, 2, target),
            policy(),
            designs::deadline(),
        )
        .await
        .unwrap_err();
        let NativeTeletextEditorialImportError::Effects(error) = error else {
            panic!("new-definition use must reach inserted shared-consumer effects: {error:?}")
        };
        assert_eq!(
            error.reason,
            TeletextEditorialEffectsFailure::ProtectedDisplay
        );
        assert_eq!(error.page, Some(channel_documents::address(125, 32)));
        let (row, column) = error.position.unwrap();
        assert_eq!(row, 5);
        assert!((15..=16).contains(&column));
        assert_eq!(
            fixture
                .state
                .application
                .teletext_design_head(added.head().id)
                .await
                .unwrap()
                .unwrap(),
            *added.head()
        );
        assert!(fixture
            .state
            .application
            .teletext_design_revision(added.head().id, 3, designs::budget().max_serialized_bytes)
            .await
            .unwrap()
            .is_none());
        assert_original_pointers(&fixture, &flow).await;
        retained.assert_unchanged(&fixture, &flow).await;
    }
    fixture = fixture.reopen().await;
    Box::pin(assert_concept(&fixture, &flow, &added, &dormant)).await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
