//! Exact shared Active/Adaptive/Passive MOT defaults through the supported DTO.

use super::*;

fn object_resource(global: bool) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        if global {
            TeletextResourceKind::Gpop
        } else {
            TeletextResourceKind::Pop
        },
        TeletextRawDataPageNumber::new(2, if global { 0xd0 } else { 0xc0 }).unwrap(),
        TeletextResourceSubpageIndex::ZERO,
    )
    .unwrap()
}

pub(super) fn target(id: u16) -> TeletextObjectIdentity {
    let resource = object_resource(id == 20);
    if id == 20 {
        TeletextObjectIdentity::global(resource, TeletextObjectId::new(id)).unwrap()
    } else {
        TeletextObjectIdentity::public(resource, TeletextObjectId::new(id)).unwrap()
    }
}

fn wire_target(id: u16) -> Value {
    let resource = object_resource(id == 20);
    json!({"scope":if id == 20 {"global"} else {"public"},
        "resource":{"kind":resource.kind(),"page":resource.page(),"subpage":0},"object_id":id})
}

pub(super) fn selections() -> [TeletextDefaultObjectSelection; 3] {
    [
        (20, TeletextObjectType::Active),
        (22, TeletextObjectType::Adaptive),
        (21, TeletextObjectType::Passive),
    ]
    .map(|(id, object_type)| TeletextDefaultObjectSelection {
        target: target(id),
        object_type,
    })
}

pub(super) fn defaults() -> [Option<TeletextDefaultObjectSelection>; 4] {
    let [active, adaptive, passive] = selections();
    [Some(active), None, Some(adaptive), Some(passive)]
}

pub(super) fn mot() -> TeletextMotLevel25Declaration {
    let original = super::super::mot();
    let mut public = [original.links().object_link(None); 7];
    let [active, adaptive, passive] = selections();
    let link = |global: bool, defaults| {
        TeletextMotObjectLink::new(
            TeletextX27LinkTarget::Page {
                page: object_resource(global).page(),
            },
            TeletextMotSubpageExtent::new(0).unwrap(),
            false,
            TeletextMotFallback::new(0).unwrap(),
            defaults,
        )
    };
    let object = |selection: TeletextDefaultObjectSelection| TeletextMotDefaultObject::Object {
        target: selection.target,
        object_type: selection.object_type,
    };
    let global = link(
        true,
        [
            object(active),
            TeletextMotDefaultObject::NoObject { raw_number: 77 },
        ],
    );
    public[0] = link(false, [object(adaptive), object(passive)]);
    let associations = original
        .associations()
        .iter()
        .map(|association| {
            TeletextMotPageAssociation::new(
                association.page(),
                TeletextMotAssociationSelection::new(
                    true,
                    Some(TeletextMotPublicLinkIndex::new(1).unwrap()),
                ),
                association.drcs(),
            )
        })
        .collect();
    TeletextMotLevel25Declaration::new(
        original.identity(),
        associations,
        TeletextMotLevel25Links::new(
            global,
            public,
            original.links().drcs_link(None),
            TeletextMotPublicLinkIndex::ALL.map(|index| original.links().drcs_link(Some(index))),
            original.links().counts(),
        )
        .unwrap(),
    )
    .unwrap()
}

fn attributes(column: u8, wide: bool, high: bool) -> Value {
    json!({"kind":"at_column","column":column,"operation":{"kind":"attributes",
        "double_height":high,"double_width":wide,"boxed":false,"conceal":false,
        "invert":false,"underline_or_separated_mosaics":false}})
}

fn position(row: u8, column: u8) -> Value {
    json!({"kind":"set_position","position":{"row":row,"column":column}})
}

fn glyph(column: u8, code: u8) -> Value {
    json!({"kind":"at_column","column":column,"operation":{"kind":"g0","code":code}})
}

pub(super) fn objects() -> Value {
    let object = |id: u16, kind: &str, commands: Vec<Value>| {
        json!({
        "identity":wire_target(id),"object_type":kind,"program":{"commands":commands}})
    };
    json!([
        object(
            20,
            "active",
            vec![
                position(1, 8),
                attributes(8, true, true),
                glyph(8, b'A'),
                attributes(10, false, false)
            ]
        ),
        object(
            22,
            "adaptive",
            vec![
                position(1, 7),
                attributes(7, true, false),
                attributes(9, false, false),
                glyph(9, b'X')
            ]
        ),
        object(
            21,
            "passive",
            vec![
                position(4, 1),
                attributes(1, false, false),
                json!({"kind":"at_column","column":1,"operation":{"kind":"foreground","colour":2}}),
                json!({"kind":"at_column","column":1,"operation":{"kind":"background","colour":0}}),
                glyph(1, b'P')
            ]
        )
    ])
}

pub(super) fn response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = super::super::response(theme, page, subcode);
    value["service"]["objects"] = objects();
    let selection = |id: u16, kind: &str| json!({"target":wire_target(id),"object_type":kind});
    for metadata in value["service"]["page_enhancements"]
        .as_array_mut()
        .unwrap()
    {
        metadata["local_program"] = Value::Null;
        metadata["resources"]["objects"] = json!({"public":object_resource(false).page(),
            "global":object_resource(true).page()});
        metadata["resources"]["defaults"] = json!([
            selection(20, "active"),
            null,
            selection(22, "adaptive"),
            selection(21, "passive")
        ]);
    }
    let mut declaration = serde_json::to_value(mot()).unwrap();
    declaration["identity"] = json!({"kind":"mot","page":mot_identity().page(),"subpage":9});
    for (bank, slot, id) in [
        ("global_object", 0, 20),
        ("public_objects", 0, 22),
        ("public_objects", 1, 21),
    ] {
        let defaults = if bank == "global_object" {
            &mut declaration["links"][bank]["defaults"]
        } else {
            &mut declaration["links"][bank][0]["defaults"]
        };
        defaults[slot]["target"] = wire_target(id);
    }
    value["service"]["mots"] = json!([declaration]);
    assert!(serde_json::to_vec(&value).unwrap().len() <= encoding().input.max_response_bytes);
    value
}

pub(super) fn encoding(
) -> ersatzrs_web::host::teletext_channel_documents::TeletextChannelDocumentEncodingPolicy {
    let mut policy = super::super::encoding();
    for resource in [object_resource(true), object_resource(false)] {
        policy
            .transmissions
            .push(TeletextServiceResourceTransmission {
                resource,
                counter: TeletextResourceUpdateCounter::new(1).unwrap(),
                update: TeletextResourceFullUpdate::Changed,
                last_subpage: true,
            });
    }
    assert_eq!(policy.transmissions.len(), 7);
    policy
}
