//! Complete V3 producer oracles, never rebuilt by the action under test.
use super::*;

pub(super) fn resources() -> [TeletextResourceIdentity; 2] {
    [
        (TeletextResourceKind::Pop, 0xe2),
        (TeletextResourceKind::Gpop, 0xe3),
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
pub(super) fn definition(index: usize) -> TeletextEnhancementObjectDefinition {
    let resource = resources()[index];
    let id = TeletextObjectId::new(40 + u16::try_from(index).unwrap());
    let identity = if index == 0 {
        TeletextObjectIdentity::public(resource, id).unwrap()
    } else {
        TeletextObjectIdentity::global(resource, id).unwrap()
    };
    TeletextEnhancementObjectDefinition {
        identity,
        object_type: TeletextObjectType::Passive,
        program: TeletextEnhancementProgram::new(vec![
            TeletextEnhancementCommand::SetPosition {
                position: TeletextEnhancementPosition::new(1, 0).unwrap(),
            },
            TeletextEnhancementCommand::AtColumn {
                column: 0,
                operation: TeletextColumnOperation::G0 {
                    code: TeletextGlyphCode::new(b'P' + u8::try_from(index).unwrap()).unwrap(),
                },
            },
        ])
        .unwrap(),
    }
}
pub(super) fn transmission(
    resource: TeletextResourceIdentity,
) -> TeletextServiceResourceTransmission {
    TeletextServiceResourceTransmission {
        resource,
        counter: TeletextResourceUpdateCounter::new(7).unwrap(),
        update: TeletextResourceFullUpdate::Changed,
        last_subpage: true,
    }
}
pub(super) fn additions() -> Vec<TeletextSavedObjectResourceAddition> {
    resources()
        .into_iter()
        .enumerate()
        .map(|(index, resource)| {
            TeletextSavedObjectResourceAddition::new(
                0,
                TeletextObjectResourceAddition::new(
                    resource,
                    vec![definition(index)],
                    source::encoding().composition.service,
                )
                .unwrap(),
                TeletextSavedResourceTransmission {
                    resource,
                    counter: TeletextResourceUpdateCounter::new(7).unwrap(),
                    update: TeletextSavedResourceUpdate::Changed,
                    last_subpage: true,
                },
            )
            .unwrap()
        })
        .collect()
}
pub(super) fn encoding(
) -> ersatzrs_web::host::teletext_channel_documents::TeletextChannelDocumentEncodingPolicy {
    let mut encoding = source::encoding();
    encoding.transmissions.extend(resources().map(transmission));
    encoding
}
pub(super) fn bank_only(theme: &str, page: u16, subcode: u16) -> Value {
    let mut response = source::absent(theme, page, subcode);
    for index in 0..2 {
        let mut object = serde_json::to_value(definition(index)).unwrap();
        let resource = object["identity"]["resource"].as_object_mut().unwrap();
        let subpage = resource.remove("logical_subpage").unwrap();
        resource.insert("subpage".to_owned(), subpage);
        response["service"]["objects"]
            .as_array_mut()
            .unwrap()
            .push(object);
    }
    response
}
pub(super) fn attached(theme: &str, page: u16, subcode: u16) -> Value {
    let mut response = bank_only(theme, page, subcode);
    let metadata = response["service"]["page_enhancements"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|metadata| metadata["address"] == json!(source::display()))
        .unwrap();
    let unused = json!({"target":{"kind":"unspecified","magazine":2},"validity":"level25_only","required_subpages":65535});
    metadata["compositional_links"] = json!({
        "pop":{"target":{"kind":"page","page":resources()[0].page()},"validity":"level25_only","required_subpages":1},
        "gpop":{"target":{"kind":"page","page":resources()[1].page()},"validity":"level25_only","required_subpages":1},
        "drcs":unused,"gdrcs":unused,
    });
    metadata["resources"]["objects"]["public"] = json!(resources()[0].page());
    metadata["resources"]["objects"]["global"] = json!(resources()[1].page());
    response
}
