//! Explicit native bank concepts; no external-editor or receiver qualification.
use super::*;
use ersatzrs_app::teletext_design_documents::{
    TeletextSavedObjectResourceAddition, TeletextSavedResourceTransmission,
    TeletextSavedResourceUpdate,
};
use ersatzrs_domain::{
    TeletextDeclaredService, TeletextObjectId, TeletextObjectIdentity,
    TeletextObjectResourceAddition, TeletextRawDataPageNumber, TeletextResourceIdentity,
    TeletextResourceKind, TeletextResourceSubpageIndex, TeletextResourceUpdateCounter,
};
use ersatzrs_ffmpeg::teletext::{TeletextResourceFullUpdate, TeletextServiceResourceTransmission};
use ersatzrs_web::host::{
    teletext_authoring::{preview_original_teletext_design, TeletextAuthoringPreviewRequest},
    teletext_channel_documents::TeletextChannelDocumentEncodingPolicy,
    teletext_editorial_designs::{
        add_teletext_editorial_object_resources, NativeTeletextObjectResourcesAddRequest,
    },
};

#[path = "object_resources/retained_definitions.rs"]
mod retained_definitions;

#[path = "object_resources/selected_consumers.rs"]
mod selected_consumers;

fn bank_resources() -> [TeletextResourceIdentity; 4] {
    [
        (TeletextResourceKind::Gpop, 0xe1, 0),
        (TeletextResourceKind::Pop, 0xe0, 15),
        (TeletextResourceKind::Pop, 0xe0, 0),
        (TeletextResourceKind::Pop, 0xe2, 2),
    ]
    .map(|(kind, page, subpage)| {
        TeletextResourceIdentity::new(
            kind,
            TeletextRawDataPageNumber::new(2, page).unwrap(),
            TeletextResourceSubpageIndex::new(subpage).unwrap(),
        )
        .unwrap()
    })
}
fn new_target(index: usize) -> TeletextObjectIdentity {
    let resource = bank_resources()[index];
    if resource.kind() == TeletextResourceKind::Gpop {
        TeletextObjectIdentity::global(resource, TeletextObjectId::new(31)).unwrap()
    } else {
        TeletextObjectIdentity::public(resource, TeletextObjectId::new(30)).unwrap()
    }
}
fn wire_resource(resource: TeletextResourceIdentity) -> Value {
    let mut value = serde_json::to_value(resource).unwrap();
    let fields = value.as_object_mut().unwrap();
    let subpage = fields.remove("logical_subpage").unwrap();
    fields.insert("subpage".to_owned(), subpage);
    value
}
fn wire_target(target: TeletextObjectIdentity) -> Value {
    let mut value = serde_json::to_value(target).unwrap();
    value["resource"] = wire_resource(target.resource().unwrap());
    value
}
fn full_response(theme: &str, page: u16, subcode: u16, second: bool, edited: bool) -> Value {
    let mut value = super::super::response(theme, page, subcode);
    let objects = value["service"]["objects"].as_array_mut().unwrap();
    let original = |id| {
        objects
            .iter()
            .find(|object| object["identity"]["object_id"] == id)
            .unwrap()
            .clone()
    };
    let mut dormant25 = original(21);
    dormant25["identity"]["object_id"] = json!(25);
    if edited {
        dormant25["program"]["commands"][4]["operation"]["code"] = json!(81);
    }
    let mut adaptive26 = original(22);
    adaptive26["identity"]["object_id"] = json!(26);
    assert_eq!(
        adaptive26["program"]["commands"].as_array().unwrap().len(),
        5
    );
    adaptive26["program"]["commands"][4]["target"] = wire_target(new_target(1));
    let mut public30 = original(21);
    public30["identity"] = wire_target(new_target(1));
    if edited {
        public30["program"]["commands"][4]["operation"]["code"] = json!(81);
    }
    let mut global31 = original(20);
    global31["identity"] = wire_target(new_target(0));
    global31["program"]["commands"][4]["target"] = wire_target(seed::target(26));
    global31["program"]["commands"][4]["object_type"] = json!("adaptive");
    // Native cumulative replay retains the immutable prefix, then ordered new
    // banks, then original-bank definition additions. Match that explicit
    // catalogue order in this separately authored complete-source oracle.
    objects.extend([global31, public30, dormant25, adaptive26]);
    let empty = value["service"]["empty_object_pages"]
        .as_array_mut()
        .unwrap();
    empty.push(wire_resource(bank_resources()[2]));
    if second {
        empty.push(wire_resource(bank_resources()[3]));
    }
    value
}
fn complete_response(theme: &str, page: u16, subcode: u16) -> Value {
    full_response(theme, page, subcode, false, false)
}
fn second_response(theme: &str, page: u16, subcode: u16) -> Value {
    full_response(theme, page, subcode, true, false)
}
fn edited_response(theme: &str, page: u16, subcode: u16) -> Value {
    full_response(theme, page, subcode, true, true)
}
fn lifecycle(resource: TeletextResourceIdentity) -> TeletextServiceResourceTransmission {
    TeletextServiceResourceTransmission {
        resource,
        counter: TeletextResourceUpdateCounter::new(7).unwrap(),
        update: TeletextResourceFullUpdate::Changed,
        last_subpage: resource != bank_resources()[2],
    }
}
fn complete_encoding() -> TeletextChannelDocumentEncodingPolicy {
    let mut encoding = seed::encoding();
    encoding
        .transmissions
        .extend(bank_resources()[..3].iter().copied().map(lifecycle));
    encoding
}
fn second_encoding() -> TeletextChannelDocumentEncodingPolicy {
    let mut encoding = complete_encoding();
    encoding.transmissions.push(lifecycle(bank_resources()[3]));
    encoding
}
struct BankExpected {
    declared: TeletextDeclaredService,
    magazine: Vec<[u8; 46]>,
    units: Vec<Vec<[u8; 46]>>,
    rasters: Vec<TeletextRaster>,
    banks: Vec<TeletextSavedObjectResourceAddition>,
    definition25: TeletextEnhancementObjectDefinition,
    definition26: TeletextEnhancementObjectDefinition,
}
async fn qualify_banks(second: bool, edited: bool) -> BankExpected {
    let response = match (second, edited) {
        (_, true) => edited_response("day", 124, 32),
        (true, false) => second_response("day", 124, 32),
        (false, false) => complete_response("day", 124, 32),
    };
    Box::pin(qualify_response(response, second)).await
}
async fn qualify_response(response: Value, second: bool) -> BankExpected {
    // A complete original V3 producer, independently of the new-bank action,
    // enters the pre-existing native factory and physical encoder directly.
    let fixture = Fixture::new_persistent().await;
    authoring_metadata::install(&fixture).await;
    authoring_metadata::write_compact(&fixture, "day", &response);
    let profile = fixture.profile("day", "day", "good").await;
    fixture.bind(1, &profile, None).await;
    let encoding = if second {
        second_encoding()
    } else {
        complete_encoding()
    };
    let candidate = Box::pin(prepare_with_policy(&fixture, 1, encoding)).await;
    let resources = bank_resources();
    let expected = bank_expected(
        candidate.encoded().unwrap(),
        &resources[..if second { 4 } else { 3 }],
    );
    // No new bank is implicitly selected by a pre-existing display or default.
    assert!(expected
        .declared
        .semantic()
        .base()
        .pages
        .iter()
        .all(|page| {
            !expected
                .declared
                .semantic()
                .effective_program(page.address)
                .unwrap()
                .1
                .commands()
                .iter()
                .any(|command| {
                    matches!(command, TeletextEnhancementCommand::InvokeObject {target,..}
                if [new_target(0),new_target(1),seed::target(26)].contains(target))
                })
        }));
    fixture.state.pool.close().await;
    expected
}
fn bank_expected(
    service: &EncodedTeletextAuthoredService,
    resources: &[TeletextResourceIdentity],
) -> BankExpected {
    let semantic = service.declared().semantic();
    let banks = resources
        .iter()
        .map(|resource| {
            let definitions = semantic
                .objects()
                .definitions()
                .iter()
                .filter(|definition| definition.identity.resource() == Some(*resource))
                .cloned()
                .collect();
            let bank = TeletextObjectResourceAddition::new(
                *resource,
                definitions,
                seed::encoding().composition.service,
            )
            .unwrap();
            let transmission = lifecycle(*resource);
            TeletextSavedObjectResourceAddition::new(
                0,
                bank,
                TeletextSavedResourceTransmission {
                    resource: *resource,
                    counter: transmission.counter,
                    update: TeletextSavedResourceUpdate::Changed,
                    last_subpage: transmission.last_subpage,
                },
            )
            .unwrap()
        })
        .collect();
    BankExpected {
        declared: service.declared().clone(),
        magazine: service.magazine_units().to_vec(),
        units: service
            .pages()
            .iter()
            .map(|page| page.data_units().to_vec())
            .collect(),
        rasters: rasters(service),
        banks,
        definition25: semantic
            .objects()
            .definition(seed::target(25))
            .unwrap()
            .clone(),
        definition26: semantic
            .objects()
            .definition(seed::target(26))
            .unwrap()
            .clone(),
    }
}
fn bank_request(
    receipt: &TeletextDesignExportReceipt,
    head: i64,
    banks: Vec<TeletextSavedObjectResourceAddition>,
    definitions: Vec<TeletextEnhancementObjectDefinition>,
) -> NativeTeletextObjectResourcesAddRequest {
    NativeTeletextObjectResourcesAddRequest {
        name: "Explicit complete native banks".into(),
        receipt_id: receipt.metadata.id,
        receipt_digest: receipt.metadata.digest.clone(),
        expected_design_head: head,
        banks,
        definitions,
    }
}
async fn add_banks(
    fixture: &Fixture,
    request: NativeTeletextObjectResourcesAddRequest,
) -> SavedEditorialTeletextDesign {
    let result = add_teletext_editorial_object_resources(
        &fixture.state.application,
        request,
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextDesignMutation::Saved(saved) = result else {
        panic!("complete explicit native graph must save an unpublished concept")
    };
    saved
}
fn assert_bank_output(service: &EncodedTeletextAuthoredService, expected: &BankExpected) {
    assert_eq!(
        service.declared(),
        &expected.declared,
        "complete independently authored native model"
    );
    assert_eq!(service.magazine_units(), expected.magazine);
    assert_eq!(
        service
            .pages()
            .iter()
            .map(|page| page.data_units().to_vec())
            .collect::<Vec<_>>(),
        expected.units,
        "all encoded display, MOT, DRCS and object resource units"
    );
    assert_eq!(rasters(service), expected.rasters);
}
async fn assert_bank_concept(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    expected: &BankExpected,
) {
    let stored = drcs_bitmap::stored(fixture, concept).await;
    Box::pin(assert_bank_revision(fixture, flow, &stored, expected)).await;
}
async fn assert_bank_revision(
    fixture: &Fixture,
    flow: &Flow,
    stored: &TeletextDesignRevision,
    expected: &BankExpected,
) {
    let saved = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    let overlay = saved.editorial_overlay().unwrap();
    assert_eq!(overlay.schema_version(), 25);
    assert_eq!(overlay.object_resource_additions(), expected.banks);
    assert_eq!(overlay.resource_object_definition_additions().len(), 2);
    let restored = reload_original_teletext_design_revision(
        &fixture.state.application,
        stored.design_id,
        stored.revision,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    let original = flow.saved[0].candidate();
    assert_eq!(restored.selection(), original.selection());
    assert_eq!(restored.metadata(), original.metadata());
    assert_eq!(restored.sources().len(), original.sources().len());
    for (after, before) in restored.sources().iter().zip(original.sources()) {
        assert_eq!(after.bytes(), before.bytes());
        assert_eq!(after.authoring(), before.authoring());
    }
    for (after, before) in restored.producers().iter().zip(original.producers()) {
        assert_eq!(after.original(), before.original());
    }
    for before in original.transmissions() {
        assert!(
            restored.transmissions().contains(before),
            "original lifecycle remains explicit and exact"
        );
    }
    assert_eq!(
        restored.transmissions().len(),
        original.transmissions().len() + expected.banks.len()
    );
    for bank in &expected.banks {
        assert!(restored
            .transmissions()
            .contains(&lifecycle(bank.addition().resource())));
    }
    assert_bank_output(restored.encoded().unwrap(), expected);
    let preview = preview_original_teletext_design(
        &fixture.state.application,
        TeletextAuthoringPreviewRequest {
            channel_id: 1,
            design_id: stored.design_id,
            revision: stored.revision,
            owner: Some(channel_documents::address(124, 32)),
        },
        designs::deadline(),
    )
    .await
    .unwrap();
    assert_eq!(preview.raster(), Some(&expected.rasters[0]));
    for bank in &expected.banks {
        assert!(preview
            .object_resources()
            .contains(&bank.addition().resource()));
    }
    drop(preview);
}
fn edit_bodies(
    exported: &ExportedOriginalTeletextDesign,
    receipt: &TeletextDesignExportReceipt,
    head: i64,
) -> NativeTeletextEditorialImportRequest {
    let resource = exported
        .sidecar()
        .project
        .assets
        .iter()
        .find_map(|asset| match &asset.scope {
            TeletextExportAssetScope::Resource { identity, .. }
                if identity.kind == TeletextResourceKindWire::Pop
                    && identity.page.magazine == 2
                    && identity.page.page == 0xe0
                    && identity.subpage == 15 =>
            {
                Some(asset.name.as_str())
            }
            _ => None,
        })
        .expect("new populated POP e0/S1=15 must have its own resource asset");
    let context = exported
        .sidecar()
        .project
        .assets
        .iter()
        .find_map(|asset| match &asset.scope {
            TeletextExportAssetScope::ObjectContextTtiV1 {
                resource_name,
                object_id: 30,
                editor_page: 256,
                editor_ordinal: 0,
            } if resource_name == resource => Some(asset.name.as_str()),
            _ => None,
        })
        .expect("new-bank definition30 must expose an actual supported editor context");
    let names = [passive_name(exported), context];
    assert_ne!(names[0], names[1]);
    let mut request = drcs_bitmap::from_receipt(receipt, head);
    let original_files = request.files.clone();
    for name in names {
        let file = request
            .files
            .iter_mut()
            .find(|file| file.name == name)
            .unwrap();
        let mut records = parse_teletext_tti(&file.bytes, policy().tti)
            .unwrap()
            .records()
            .to_vec();
        // The concrete carrier must independently confirm the inherited
        // Passive context structure before using this established word index.
        assert_eq!(
            word(&records, 7),
            1 | (9 << 6) | (80 << 11),
            "actual context {name} must contain Passive P at word7"
        );
        for record in &mut records {
            if let TeletextTtiRecord::Packet {
                row: 26,
                designation: 0,
                words,
            } = record
            {
                words[7] = 1 | (9 << 6) | (81 << 11);
            }
        }
        file.bytes = write_teletext_tti(
            &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
            policy().tti,
        )
        .unwrap();
    }
    for (before, after) in original_files.iter().zip(&request.files) {
        assert_eq!(before.name, after.name);
        if !names.contains(&before.name.as_str()) {
            assert_eq!(
                before.bytes, after.bytes,
                "all protected native resources and other editor files remain exact"
            );
        }
    }
    request
}
async fn assert_head_only(fixture: &Fixture, concept: &SavedEditorialTeletextDesign) {
    assert_eq!(
        fixture
            .state
            .application
            .teletext_design_head(concept.head().id)
            .await
            .unwrap()
            .unwrap(),
        *concept.head()
    );
    assert!(fixture
        .state
        .application
        .teletext_design_revision(
            concept.head().id,
            concept.head().revision + 1,
            designs::budget().max_serialized_bytes
        )
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn explicit_object_bank_concept_cumulative_reimport_preview_publication_and_cold_recovery() {
    let first = Box::pin(qualify_banks(false, false)).await;
    let second = Box::pin(qualify_banks(true, false)).await;
    let edited = Box::pin(qualify_banks(true, true)).await;
    let fixture = Fixture::new_persistent().await;
    http_routes::configure_authoring_policy(&fixture).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        super::super::response,
        fixture,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    assert_eq!(
        rasters(flow.accepted[0].service()),
        first.rasters,
        "unused new banks and definitions cannot change original displays"
    );
    let initial_add = add(&fixture, &flow.receipts[0], first.definition25.clone()).await;
    let exported = drcs_bitmap::reexport(&fixture, &initial_add).await;
    let current = receipt(&fixture, &exported).await;
    let concept = add_banks(
        &fixture,
        bank_request(
            &current,
            2,
            first.banks.clone(),
            vec![first.definition26.clone()],
        ),
    )
    .await;
    assert_eq!(concept.head().revision, 3);
    Box::pin(assert_bank_concept(&fixture, &flow, &concept, &first)).await;
    assert_original_pointers(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    Box::pin(assert_bank_concept(&fixture, &flow, &concept, &first)).await;
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let current = receipt(&fixture, &exported).await;
    let historical = add_teletext_editorial_object_resources(
        &fixture.state.application,
        bank_request(&flow.receipts[0], 3, vec![second.banks[3].clone()], vec![]),
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    assert!(matches!(
        historical,
        TeletextDesignMutation::Rejected(
            ersatzrs_core::repositories::TeletextDesignRejection::Conflict
        )
    ));
    assert_head_only(&fixture, &concept).await;
    for malformed in [false, true] {
        let mut request = drcs_bitmap::from_receipt(&current, 3);
        let name = protected_file(&exported, TeletextResourceKindWire::Gpop, 0xe1);
        let file = request
            .files
            .iter_mut()
            .find(|file| file.name == name)
            .unwrap();
        if malformed {
            file.bytes = b"not a protected native bank".to_vec();
        } else {
            file.bytes[0] ^= 1;
        }
        let result = import_teletext_editorial_design(
            &fixture.state.application,
            request,
            policy(),
            designs::deadline(),
        )
        .await;
        assert!(!matches!(result, Ok(TeletextDesignMutation::Saved(_))));
        assert_head_only(&fixture, &concept).await;
        assert_original_pointers(&fixture, &flow).await;
    }
    let concept = add_banks(
        &fixture,
        bank_request(&current, 3, vec![second.banks[3].clone()], vec![]),
    )
    .await;
    assert_eq!(concept.head().revision, 4);
    Box::pin(assert_bank_concept(&fixture, &flow, &concept, &second)).await;
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let current = receipt(&fixture, &exported).await;
    let concept = import(&fixture, edit_bodies(&exported, &current, 4)).await;
    assert_eq!(concept.head().revision, 5);
    fixture = fixture.reopen().await;
    Box::pin(assert_bank_concept(&fixture, &flow, &concept, &edited)).await;
    assert_original_pointers(&fixture, &flow).await;
    let accepted = publish_and_accept(&fixture, &flow, &concept).await;
    assert_bank_output(accepted.service(), &edited);
    fixture = fixture.reopen().await;
    Box::pin(assert_bank_concept(&fixture, &flow, &concept, &edited)).await;
    assert_pointers(&fixture, &accepted, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

fn empty_control_bank(resource: TeletextResourceIdentity) -> TeletextSavedObjectResourceAddition {
    TeletextSavedObjectResourceAddition::new(
        0,
        TeletextObjectResourceAddition::new(resource, vec![], seed::encoding().composition.service)
            .unwrap(),
        TeletextSavedResourceTransmission {
            resource,
            counter: TeletextResourceUpdateCounter::new(7).unwrap(),
            update: TeletextSavedResourceUpdate::Changed,
            last_subpage: true,
        },
    )
    .unwrap()
}

#[tokio::test]
async fn explicit_object_bank_controls_reject_without_revision_publication_or_source_damage() {
    let expected = Box::pin(qualify_banks(false, false)).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        super::super::response,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    for variant in 0..8 {
        let mut banks = expected.banks.clone();
        let mut head = 1;
        let mut bounds = policy();
        let mut definitions = vec![expected.definition26.clone()];
        match variant {
            0 => head = 2,
            1 => {
                let original = banks.remove(0);
                banks.insert(
                    0,
                    TeletextSavedObjectResourceAddition::new(
                        1,
                        original.addition().clone(),
                        original.transmission(),
                    )
                    .unwrap(),
                );
            }
            2 => banks.push(banks[0].clone()),
            3 => {
                let occupied = seed::target(21).resource().unwrap();
                banks = vec![empty_control_bank(occupied)];
            }
            4 => {
                let old = seed::target(21).resource().unwrap();
                let cross = TeletextResourceIdentity::new(
                    TeletextResourceKind::Gpop,
                    old.page(),
                    old.logical_subpage(),
                )
                .unwrap();
                banks = vec![empty_control_bank(cross)];
            }
            5 => bounds.document.max_resource_transmissions = 7,
            6 => bounds.encoding.composition.max_storage_bytes = 1,
            7 => {
                definitions[0].identity = seed::target(21);
            }
            _ => unreachable!(),
        }
        let result = add_teletext_editorial_object_resources(
            &fixture.state.application,
            bank_request(&flow.receipts[0], head, banks, definitions),
            bounds,
            designs::deadline(),
        )
        .await;
        assert!(
            !matches!(result, Ok(TeletextDesignMutation::Saved(_))),
            "variant{variant}"
        );
        assert_unchanged(&fixture, &flow).await;
    }
    let error = add_teletext_editorial_object_resources(
        &fixture.state.application,
        bank_request(
            &flow.receipts[0],
            1,
            expected.banks.clone(),
            vec![expected.definition26.clone()],
        ),
        policy(),
        std::time::Instant::now(),
    )
    .await
    .unwrap_err();
    assert!(matches!(
        error,
        NativeTeletextEditorialImportError::TimedOut
    ));
    let mismatched = TeletextSavedObjectResourceAddition::new(
        0,
        expected.banks[0].addition().clone(),
        expected.banks[1].transmission(),
    );
    assert!(
        mismatched.is_err(),
        "safe API cannot fabricate a lifecycle for another bank"
    );
    assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    assert_unchanged(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
