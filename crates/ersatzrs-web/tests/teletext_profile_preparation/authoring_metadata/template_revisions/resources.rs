//! Complete new resources arrive only from the explicitly installed r2 producer.
use super::*;
use ersatzrs_domain::{
    TeletextEnhancementCommand, TeletextObjectId, TeletextObjectIdentity, TeletextObjectType,
    TeletextRawDataPageNumber, TeletextResourceIdentity, TeletextResourceKind,
    TeletextResourceSubpageIndex,
};

pub(super) fn inventory() -> [TeletextResourceIdentity; 2] {
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
pub(super) fn target(global: bool) -> TeletextObjectIdentity {
    let resource = inventory()[usize::from(global)];
    let id = TeletextObjectId::new(if global { 31 } else { 32 });
    if global {
        TeletextObjectIdentity::global(resource, id).unwrap()
    } else {
        TeletextObjectIdentity::public(resource, id).unwrap()
    }
}
fn wire_target(global: bool) -> Value {
    let mut value = serde_json::to_value(target(global)).unwrap();
    let resource = value["resource"].as_object_mut().unwrap();
    let subpage = resource.remove("logical_subpage").unwrap();
    resource.insert("subpage".to_owned(), subpage);
    value
}
pub(super) fn introduce(value: &mut Value) {
    let companion = channel_documents::response(DocumentFixture::Companion, "day", 123, 16, "nld");
    let enhancement = &mut value["service"]["page_enhancements"][0];
    enhancement["compositional_links"] =
        companion["page_enhancements"][0]["compositional_links"].clone();
    enhancement["compositional_links"]["gpop"] = json!({"target":{"kind":"page","page":{"magazine":1,"page":161}},"validity":"level25_only","required_subpages":1});
    enhancement["resources"]["objects"] =
        json!({"public":{"magazine":1,"page":160},"global":{"magazine":1,"page":161}});
    enhancement["local_program"]["commands"].as_array_mut().unwrap().extend([
        json!({"kind":"set_position","position":{"row":3,"column":0}}),
        json!({"kind":"invoke_object","target":wire_target(true),"object_type":"active","offset":null}),
    ]);
    value["service"]["objects"] = json!([
        {"identity":wire_target(false),"object_type":"passive","program":{"commands":[
            {"kind":"at_column","column":6,"operation":{"kind":"foreground","colour":18}},
            {"kind":"at_column","column":6,"operation":{"kind":"g0","code":80}}
        ]}},
        {"identity":wire_target(true),"object_type":"active","program":{"commands":[
            {"kind":"invoke_object","target":wire_target(false),"object_type":"passive","offset":null}
        ]}}
    ]);
}
pub(super) fn policy(
    revision: u8,
) -> ersatzrs_web::host::teletext_channel_documents::TeletextChannelDocumentEncodingPolicy {
    let mut policy = channel_documents::policy();
    if revision == 2 {
        policy.transmissions = inventory()
            .map(|resource| {
                let mut transmission = crate::documents::companion_transmission();
                transmission.resource = resource;
                transmission
            })
            .to_vec();
    }
    policy
}
pub(super) fn check(
    candidate: &EncodedChannelTeletextDocumentCandidate,
    revision: u8,
    theme: &str,
) {
    let service = candidate.encoded().unwrap();
    let semantic = service.declared().semantic();
    if revision == 1 {
        assert!(service.declared().resource_inventory().is_empty());
        for global in [false, true] {
            assert!(semantic.objects().definition(target(global)).is_none());
        }
    } else {
        assert_eq!(service.declared().resource_inventory(), inventory());
        let pop = semantic.objects().definition(target(false)).unwrap();
        assert_eq!(pop.object_type, TeletextObjectType::Passive);
        let expected = package::response(2, theme)["service"]["objects"][0]["program"].clone();
        assert_eq!(serde_json::to_value(&pop.program).unwrap(), expected);
        let gpop = semantic.objects().definition(target(true)).unwrap();
        assert_eq!(gpop.object_type, TeletextObjectType::Active);
        assert_eq!(
            gpop.program.commands(),
            [TeletextEnhancementCommand::InvokeObject {
                target: target(false),
                object_type: TeletextObjectType::Passive,
                offset: None,
            }]
        );
        let owner = channel_documents::address(123, 16);
        let links = &semantic.page_enhancements()[&owner].resources.objects;
        assert_eq!(links.public, Some(inventory()[0].page()));
        assert_eq!(links.global, Some(inventory()[1].page()));
        assert_eq!(
            semantic
                .collected(owner)
                .unwrap()
                .1
                .object_references()
                .iter()
                .map(|call| call.target)
                .collect::<Vec<_>>(),
            [target(true), target(false)]
        );
    }
    let preview = ersatzrs_web::host::teletext_preview::project_teletext_preview(
        service,
        channel_documents::address(123, 16),
        ersatzrs_domain::TeletextColourPixelOptions {
            reveal_concealed: false,
            flash_phase: ersatzrs_domain::TeletextFlashPhase::Original,
        },
        ersatzrs_web::host::teletext_preview::TeletextPreviewBudget {
            max_pages: 16,
            max_model_records: 32_768,
            max_pixels: 168_000,
            max_diagnostics: 1400,
            max_work: 50_000_000,
        },
    )
    .unwrap();
    assert!(preview.raster().diagnostics().is_empty());
    let mask = [0, 0x3fc, 0x306, 0x306, 0x3fc, 0x300, 0x300, 0x300, 0, 0];
    for (row, bits) in mask.into_iter().enumerate() {
        for column in 0..12 {
            let rgb = if revision == 2 && bits & (1 << (11 - column)) != 0 {
                if theme == "day" {
                    [15, 0, 0]
                } else {
                    [0, 0, 15]
                }
            } else {
                [0, 0, 0]
            };
            assert!(
                matches!(preview.raster().pixel(120+column,30+row),
                Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(value))) if value.get()==rgb),
                "r1 blank -> r2 literal nested resource P at Main3/6, including left panel4"
            );
        }
    }
}
