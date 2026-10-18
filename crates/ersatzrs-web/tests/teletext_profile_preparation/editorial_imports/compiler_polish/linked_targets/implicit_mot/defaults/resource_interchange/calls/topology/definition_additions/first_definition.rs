//! First genuine empty POP/GPOP definitions through the production Add form.
use super::*;
use ersatzrs_domain::{
    ChannelTeletextBindingRead, TeletextObjectId, TeletextObjectIdentity, TeletextProfileRead,
    TeletextRawDataPageNumber, TeletextResourceIdentity, TeletextResourceKind,
    TeletextResourceSubpageIndex,
};
use ersatzrs_ffmpeg::teletext::EncodedTeletextObjectPage;
use ersatzrs_web::host::{
    teletext_authoring::{preview_original_teletext_design, TeletextAuthoringPreviewRequest},
    teletext_channel_documents::TeletextChannelDocumentEncodingPolicy,
};
use std::collections::BTreeMap;

#[path = "first_definition/actual_admin.rs"]
mod actual_admin;

const ADD: &str = "/admin-api/teletext/design/add-object";
const NEW_IDS: [u16; 4] = [25, 26, 27, 28];
const TYPES: [&str; 4] = ["passive", "active", "adaptive", "passive"];
const RESOURCE_INDICES: [usize; 4] = [0, 0, 1, 1];

fn resources() -> [TeletextResourceIdentity; 2] {
    [
        (TeletextResourceKind::Pop, 0xa0),
        (TeletextResourceKind::Gpop, 0xa1),
    ]
    .map(|(kind, page)| {
        TeletextResourceIdentity::new(
            kind,
            TeletextRawDataPageNumber::new(1, page).unwrap(),
            TeletextResourceSubpageIndex::new(0).unwrap(),
        )
        .unwrap()
    })
}
fn identity(index: usize) -> TeletextObjectIdentity {
    let resource = resources()[RESOURCE_INDICES[index]];
    let id = TeletextObjectId::new(NEW_IDS[index]);
    if resource.kind() == TeletextResourceKind::Pop {
        TeletextObjectIdentity::public(resource, id).unwrap()
    } else {
        TeletextObjectIdentity::global(resource, id).unwrap()
    }
}
fn wire_identity(index: usize) -> Value {
    let mut value = serde_json::to_value(identity(index)).unwrap();
    let resource = value["resource"].as_object_mut().unwrap();
    let subpage = resource.remove("logical_subpage").unwrap();
    resource.insert("subpage".to_owned(), subpage);
    value
}
fn validate_response(value: &Value) {
    let budget = crate::documents::validation_budget();
    let (service, _) = ersatzrs_app::teletext_pages_v3::parse_authored_service_with_metadata(
        &serde_json::to_vec(value).unwrap(),
        crate::GENERATION,
        budget.service,
        budget.declarations,
    )
    .expect("synthetic empty-resource response must satisfy the actual V3 wire/domain parser");
    assert_eq!(service.resource_inventory(), resources());
}
fn empty_response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = authoring_metadata::response(theme, page, subcode);
    value["service"] = channel_documents::response(
        crate::documents::DocumentFixture::Companion,
        theme,
        page,
        subcode,
        "nld",
    );
    let service = &mut value["service"];
    service["empty_object_pages"]
        .as_array_mut()
        .unwrap()
        .push(json!({
        "kind":"gpop","page":{"magazine":1,"page":161},"subpage":0}));
    service["page_enhancements"][0]["resources"]["objects"]["global"] =
        json!({"magazine":1,"page":161});
    service["page_enhancements"][0]["compositional_links"]["gpop"]["target"] =
        json!({"kind":"page","page":{"magazine":1,"page":161}});
    service["page_enhancements"][0]["compositional_links"]["gpop"]["required_subpages"] = json!(1);
    validate_response(&value);
    value
}
fn with_definitions(theme: &str, page: u16, subcode: u16, count: usize) -> Value {
    let mut value = empty_response(theme, page, subcode);
    value["service"]["empty_object_pages"]
        .as_array_mut()
        .unwrap()
        .retain(|page| page["kind"] != "pop" && !(count > 2 && page["kind"] == "gpop"));
    value["service"]["objects"] = json!((0..count)
        .map(|index| json!({
            "identity":wire_identity(index),"object_type":TYPES[index],"program":{"commands":[]}
        }))
        .collect::<Vec<_>>());
    validate_response(&value);
    value
}
fn first_response(theme: &str, page: u16, subcode: u16) -> Value {
    with_definitions(theme, page, subcode, 1)
}
fn four_response(theme: &str, page: u16, subcode: u16) -> Value {
    with_definitions(theme, page, subcode, 4)
}
fn encoding() -> TeletextChannelDocumentEncodingPolicy {
    let mut policy = channel_documents::policy();
    let pop = crate::documents::companion_transmission();
    let mut gpop = pop;
    gpop.resource = resources()[1];
    policy.transmissions = vec![pop, gpop];
    policy
}
async fn setup_empty(fixture: Fixture) -> (Fixture, Flow) {
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        empty_response,
        fixture,
        encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    assert_eq!(
        flow.accepted[0].service().declared().resource_inventory(),
        resources()
    );
    (fixture, flow)
}
struct EmptyExpected {
    definitions: Vec<TeletextEnhancementObjectDefinition>,
    units: Vec<Vec<[u8; 46]>>,
    raster: TeletextRaster,
}
async fn qualify_empty(count: usize) -> EmptyExpected {
    let response = if count == 1 {
        first_response
    } else {
        four_response
    };
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        response,
        Fixture::new_persistent().await,
        encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let service = flow.accepted[0].service();
    let expected = EmptyExpected {
        definitions: (0..count)
            .map(|index| {
                service
                    .declared()
                    .semantic()
                    .objects()
                    .definition(identity(index))
                    .unwrap()
                    .clone()
            })
            .collect(),
        units: service
            .pages()
            .iter()
            .map(|page| page.data_units().to_vec())
            .collect(),
        raster: generations::preview(service).raster().clone(),
    };
    fixture.state.pool.close().await;
    expected
}
type Retained = (
    Vec<TeletextProfileRead>,
    [Vec<ChannelTeletextBindingRead>; 2],
    String,
);
async fn retained(fixture: &Fixture) -> Retained {
    let application = &fixture.state.application;
    (
        application.list_teletext_profiles(ADDON).await.unwrap(),
        [
            application.channel_teletext_bindings(1).await.unwrap(),
            application.channel_teletext_bindings(2).await.unwrap(),
        ],
        calculate_addon_content_sha256(&fixture.root.path().join("package"))
            .await
            .unwrap(),
    )
}
async fn unchanged(fixture: &Fixture, flow: &Flow, before: &Retained) {
    assert_eq!(&retained(fixture).await, before);
    assert_original_pointers_with_width(fixture, flow, 40 * 12).await;
    assert!(fixture.playout.is_empty());
}
fn fields(stored: &TeletextDesignRevision, index: usize) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("channel_id".into(), "1".into()),
        ("design_id".into(), stored.design_id.to_string()),
        ("revision".into(), stored.revision.to_string()),
        ("digest".into(), stored.digest.clone()),
        ("expected_design_head".into(), stored.revision.to_string()),
        (
            "name".into(),
            "First empty native resource definition".into(),
        ),
        (
            "resource".into(),
            serde_json::to_string(&resources()[RESOURCE_INDICES[index]]).unwrap(),
        ),
        ("object_type".into(), TYPES[index].into()),
        ("new_object_id".into(), NEW_IDS[index].to_string()),
    ])
}
async fn submit(fixture: &Fixture, fields: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let response = http_routes::post(
        fixture,
        ADD,
        "application/x-www-form-urlencoded",
        serde_urlencoded::to_string(fields).unwrap().into_bytes(),
    )
    .await;
    http_routes::redirect(&response, fields["channel_id"].parse().unwrap())
}
async fn stored(fixture: &Fixture, flow: &Flow, count: usize) -> TeletextDesignRevision {
    let id = flow.saved[0].head().id;
    let revision = i64::try_from(count + 1).unwrap();
    let head = fixture
        .state
        .application
        .teletext_design_head(id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(head.revision, revision);
    assert!(!head.deleted);
    assert!(fixture
        .state
        .application
        .teletext_design_revision(id, revision + 1, designs::budget().max_serialized_bytes)
        .await
        .unwrap()
        .is_none());
    fixture
        .state
        .application
        .teletext_design_revision(id, revision, designs::budget().max_serialized_bytes)
        .await
        .unwrap()
        .unwrap()
}

fn nibble(byte: u8) -> u8 {
    [
        0x15, 0x02, 0x49, 0x5e, 0x64, 0x73, 0x38, 0x2f, 0xd0, 0xc7, 0x8c, 0x9b, 0xa1, 0xb6, 0xfd,
        0xea,
    ]
    .iter()
    .position(|value| *value == byte.reverse_bits())
    .unwrap()
    .try_into()
    .unwrap()
}
fn row(page: &EncodedTeletextObjectPage, number: u8) -> &[u8; 46] {
    page.data_units()
        .iter()
        .find(|unit| (nibble(unit[4]) | (nibble(unit[5]) << 4)) >> 3 == number)
        .unwrap()
}
fn native_words(unit: &[u8; 46]) -> [u32; 13] {
    // Independent extraction from actual Hamming-protected wire triplets, also
    // used by the established object-page golden tests; never re-encode here.
    std::array::from_fn(|index| {
        let raw = u32::from_le_bytes([
            unit[7 + 3 * index].reverse_bits(),
            unit[8 + 3 * index].reverse_bits(),
            unit[9 + 3 * index].reverse_bits(),
            0,
        ]);
        assert_eq!(raw.count_ones() % 2, 1);
        for parity in [1_u32, 2, 4, 8, 16] {
            let ones: u32 = (1_u32..24)
                .filter(|position| position & parity != 0)
                .map(|position| (raw >> (position - 1)) & 1)
                .sum();
            assert_eq!(ones % 2, 1);
        }
        [
            3, 5, 6, 7, 9, 10, 11, 12, 13, 14, 15, 17, 18, 19, 20, 21, 22, 23,
        ]
        .into_iter()
        .enumerate()
        .fold(0, |data, (bit, position)| {
            data | (((raw >> (position - 1)) & 1) << bit)
        })
    })
}
fn assert_native_empty_definitions(service: &EncodedTeletextAuthoredService) {
    for page in service.pages() {
        let EncodedTeletextAuthoredPage::Object(page) = page else {
            continue;
        };
        let body = native_words(row(page, 3));
        let pointers = native_words(row(page, 1));
        let mut slots = [0; 4];
        assert!(
            page.command_positions().is_empty(),
            "no invented NoOp command"
        );
        for (ordinal, definition) in page.definition_positions().iter().enumerate() {
            assert_eq!(usize::from(definition.absolute_index()), ordinal);
            let code = match definition.object_type() {
                TeletextObjectType::Active => 1,
                TeletextObjectType::Adaptive => 2,
                TeletextObjectType::Passive => 3,
            };
            let slot = slots[code];
            slots[code] += 1;
            let pointer = (pointers[code] >> (9 * (slot % 2))) & 511;
            assert_eq!(usize::try_from(pointer).unwrap(), ordinal);
            let word = body[ordinal];
            assert_eq!(word & 63, 0x28, "complete Level2.5 definition marker");
            assert_eq!((word >> 6) & 31, u32::try_from(20 + code).unwrap());
            assert_eq!(word >> 11, u32::try_from(16 * slot).unwrap());
        }
        assert!(body[page.definition_positions().len()..]
            .iter()
            .all(|word| (word & 63, (word >> 6) & 31, word >> 11) == (63, 31, 3)));
    }
}

async fn assert_concept(
    fixture: &Fixture,
    flow: &Flow,
    expected: &EmptyExpected,
) -> TeletextDesignRevision {
    let count = expected.definitions.len();
    let stored = stored(fixture, flow, count).await;
    let document = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    let overlay = document.editorial_overlay().unwrap();
    assert_eq!(overlay.schema_version(), 18);
    let additions = overlay.resource_object_definition_additions();
    assert_eq!(additions.len(), count);
    for (addition, definition) in additions.iter().zip(&expected.definitions) {
        assert_eq!(addition.definition(), definition);
        assert!(addition.definition().program.commands().is_empty());
    }
    assert!(
        overlay.pages().is_empty()
            && overlay.local_program_operands().is_empty()
            && overlay.local_object_operands().is_empty()
            && overlay.local_object_glyph_program_overlays().is_empty()
            && overlay.resource_object_operands().is_empty()
            && overlay.resource_object_program_overlays().is_empty()
            && overlay.drcs_bitmap_overlays().is_empty()
    );
    let restored = reload_original_teletext_design_revision(
        &fixture.state.application,
        stored.design_id,
        stored.revision,
        designs::budget(),
        encoding(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
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
    let service = restored.encoded().unwrap();
    assert_eq!(service.declared().resource_inventory(), resources());
    assert_eq!(generations::preview(service).raster(), &expected.raster);
    assert_eq!(
        generations::preview(service).raster(),
        generations::preview(flow.accepted[0].service()).raster()
    );
    assert_eq!(
        service
            .pages()
            .iter()
            .map(|page| page.data_units().to_vec())
            .collect::<Vec<_>>(),
        expected.units
    );
    assert_native_empty_definitions(service);
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
    assert_eq!(preview.object_resources(), resources());
    drop(preview);
    stored
}

async fn assert_exported(
    fixture: &Fixture,
    stored: &TeletextDesignRevision,
    expected: &EmptyExpected,
) {
    let result = export_original_teletext_design(
        &fixture.state.application,
        NativeTeletextExportRequest {
            design_id: stored.design_id,
            revision: stored.revision,
            design_digest: stored.digest.clone(),
            expected_design_head: stored.revision,
            pages: None,
        },
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextDesignMutation::Saved(exported) = result else {
        panic!("new empty definitions must re-export")
    };
    let contexts = exported
        .sidecar()
        .project
        .assets
        .iter()
        .filter(|asset| {
            matches!(
                asset.scope,
                TeletextExportAssetScope::ObjectContextTtiV1 { .. }
                    | TeletextExportAssetScope::ObjectTtiV1 { .. }
            )
        })
        .count();
    assert_eq!(contexts, expected.definitions.len());
    for definition in &expected.definitions {
        assert!(exported.sidecar().project.assets.iter().any(|asset| matches!(asset.scope,
            TeletextExportAssetScope::ObjectContextTtiV1 {object_id,..} | TeletextExportAssetScope::ObjectTtiV1 {object_id,..}
                if object_id == definition.identity.object_id().get())));
    }
}

async fn reject_original_controls(fixture: &Fixture, flow: &Flow, before: &Retained) {
    let original = fields(&flow.originals[0], 0);
    let mut foreign_page = serde_json::to_value(resources()[0]).unwrap();
    foreign_page["page"]["page"] = json!(162);
    let mut foreign_s1 = serde_json::to_value(resources()[0]).unwrap();
    foreign_s1["logical_subpage"] = json!(1);
    let mut wrong_kind = serde_json::to_value(resources()[0]).unwrap();
    wrong_kind["kind"] = json!("drcs");
    for (key, value, error) in [
        ("object_type", "unknown".to_owned(), "object-add-invalid"),
        ("new_object_id", "65536".to_owned(), "object-add-invalid"),
        (
            "resource",
            serde_json::to_string(&foreign_page).unwrap(),
            "object-add-invalid",
        ),
        (
            "resource",
            serde_json::to_string(&foreign_s1).unwrap(),
            "object-add-invalid",
        ),
        (
            "resource",
            serde_json::to_string(&wrong_kind).unwrap(),
            "object-add-invalid",
        ),
        ("digest", "0".repeat(64), "publication-conflict"),
        (
            "expected_design_head",
            "2".to_owned(),
            "publication-conflict",
        ),
        ("channel_id", "2".to_owned(), "foreign-channel"),
    ] {
        let mut invalid = original.clone();
        invalid.insert(key.to_owned(), value);
        assert_eq!(submit(fixture, &invalid).await["error"], error);
        stored(fixture, flow, 0).await;
        unchanged(fixture, flow, before).await;
    }
}

#[tokio::test]
async fn http_first_empty_resource_definition_add_preserves_native_concept_and_controls() {
    let first = Box::pin(qualify_empty(1)).await;
    let four = Box::pin(qualify_empty(4)).await;
    let fixture = Fixture::new_persistent().await;
    http_routes::configure_authoring_policy(&fixture).await;
    let (mut fixture, flow) = Box::pin(setup_empty(fixture)).await;
    let before = retained(&fixture).await;
    assert_native_empty_definitions(flow.accepted[0].service());
    for page in flow.accepted[0].service().pages() {
        if let EncodedTeletextAuthoredPage::Object(page) = page {
            assert!(page.definition_positions().is_empty());
        }
    }
    Box::pin(reject_original_controls(&fixture, &flow, &before)).await;
    let submitted = fields(&flow.originals[0], 0);
    let query = submit(&fixture, &submitted).await;
    assert_eq!(query["saved"], "draft");
    assert_eq!(query["revision"], "2");
    assert_eq!(query["page"], "124");
    assert_eq!(query["subcode"], "0020");
    let current = Box::pin(assert_concept(&fixture, &flow, &first)).await;
    Box::pin(assert_exported(&fixture, &current, &first)).await;
    assert_eq!(
        submit(&fixture, &fields(&current, 0)).await["error"],
        "object-add-invalid"
    );
    assert_eq!(
        submit(&fixture, &submitted).await["error"],
        "publication-conflict"
    );
    for index in 1..4 {
        let current = stored(&fixture, &flow, index).await;
        let query = submit(&fixture, &fields(&current, index)).await;
        assert_eq!(query["saved"], "draft");
        assert_eq!(query["revision"], (index + 2).to_string());
    }
    let current = Box::pin(assert_concept(&fixture, &flow, &four)).await;
    Box::pin(assert_exported(&fixture, &current, &four)).await;
    unchanged(&fixture, &flow, &before).await;
    fixture = fixture.reopen().await;
    let current = Box::pin(assert_concept(&fixture, &flow, &four)).await;
    Box::pin(assert_exported(&fixture, &current, &four)).await;
    unchanged(&fixture, &flow, &before).await;
    fixture.state.pool.close().await;
}
