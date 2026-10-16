//! The supported V3 DTO retains eight real page owners and one unrelated bank.

use super::*;

const MAX_RESPONSE_BYTES: usize = 32_768;

fn page_response(theme: &str, page: u16) -> Value {
    let mut value = compiler_polish::response(theme, 124, 32);
    let owner = json!({"page":page,"subcode":32});
    value["service"]["pages"][0]["address"] = owner.clone();
    value["service"]["pages"][0]["navigation"][0]["target"]["page"] =
        json!(if page == 124 { 125 } else { 124 });
    value["service"]["page_enhancements"][0]["address"] = owner.clone();
    value["service"]["page_enhancements"][0]["presentation"]["primary"] =
        json!(if slot(page) < 2 { 8 } else { 22 });
    let commands = value["service"]["page_enhancements"][0]["local_program"]["commands"]
        .as_array_mut()
        .unwrap();
    for command in commands {
        if command["kind"] == "invoke_object" {
            command["target"]["owner"] = owner.clone();
        }
    }
    let objects = value["service"]["objects"].as_array_mut().unwrap();
    assert_eq!(objects.len(), 2);
    for object in objects {
        object["identity"]["owner"] = owner.clone();
        if object["identity"]["object_id"] == 0xfffe {
            assert_eq!(object["object_type"], "adaptive");
            object["program"]["commands"][0]["operation"]["underline_or_separated_mosaics"] =
                json!(slot(page) % 2 == 1);
        }
    }
    value["service"]["mot_selections"][0]["display"] = owner.clone();
    value["authoring"]["pages"][0]["page"] = owner;
    if page >= 128 {
        let unused = json!({"target":{"kind":"unspecified","magazine":1},"validity":"level25_only","required_subpages":65535});
        value["service"]["page_enhancements"][0]["resources"]["drcs"] =
            json!({"normal":{"magazine":2,"page":162},"global":null});
        value["service"]["page_enhancements"][0]["compositional_links"] = json!({"gpop":unused,"pop":unused,"gdrcs":unused,
            "drcs":{"target":{"kind":"page","page":{"magazine":2,"page":162}},"validity":"level25_only","required_subpages":8}});
    }
    value
}

pub(super) fn response(theme: &str, page: u16, subcode: u16) -> Value {
    assert_eq!((page, subcode), (124, 32));
    let mut value = page_response(theme, 124);
    for field in ["pages", "page_enhancements", "mot_selections", "objects"] {
        value["service"][field] = json!([]);
    }
    value["authoring"]["pages"] = json!([]);
    for number in 124..132 {
        let page = page_response(theme, number);
        for field in ["pages", "page_enhancements", "mot_selections"] {
            value["service"][field]
                .as_array_mut()
                .unwrap()
                .push(page["service"][field][0].clone());
        }
        value["service"]["objects"].as_array_mut().unwrap().extend(
            page["service"]["objects"]
                .as_array()
                .unwrap()
                .iter()
                .cloned(),
        );
        value["authoring"]["pages"]
            .as_array_mut()
            .unwrap()
            .push(page["authoring"]["pages"][0].clone());
    }
    value["service"]["drcs"] = json!([{"identity":{"kind":"drcs","page":{"magazine":2,"page":162},"subpage":3},
        "glyphs":[{"slot":5,"bitmap":[0,0,0,0,0,0,0,0,0,0]}]}]);
    let response_bytes = serde_json::to_vec(&value).unwrap().len();
    assert!(response_bytes <= MAX_RESPONSE_BYTES, "{response_bytes}");
    value
}

pub(super) fn encoding_policy(
) -> ersatzrs_web::host::teletext_channel_documents::TeletextChannelDocumentEncodingPolicy {
    let mut policy = channel_documents::policy();
    assert_eq!(policy.input.max_response_bytes, MAX_RESPONSE_BYTES);
    policy.transmissions = vec![TeletextServiceResourceTransmission {
        resource: authored_bank().identity(),
        counter: TeletextResourceUpdateCounter::new(1).unwrap(),
        update: TeletextResourceFullUpdate::Changed,
        last_subpage: true,
    }];
    policy
}
