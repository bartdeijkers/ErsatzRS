//! The supported DTO uses three cloned Polish owners and four authored banks.

use super::*;

fn page_response(theme: &str, page: u16) -> Value {
    let mut value = super::super::response(theme, 124, 32);
    let owner = json!({"page":page,"subcode":32});
    value["service"]["pages"][0]["address"] = owner.clone();
    value["service"]["pages"][0]["navigation"][0]["target"]["page"] =
        json!(if page == 124 { 125 } else { 124 });
    let enhancement = &mut value["service"]["page_enhancements"][0];
    enhancement["address"] = owner.clone();
    for command in enhancement["local_program"]["commands"]
        .as_array_mut()
        .unwrap()
    {
        if command["kind"] == "invoke_object" {
            command["target"]["owner"] = owner.clone();
        }
    }
    let unused = json!({"target":{"kind":"unspecified","magazine":1},"validity":"level25_only","required_subpages":65535});
    let linked = |identity: TeletextResourceIdentity| json!({"target":{"kind":"page","page":identity.page()},"validity":"level25_only","required_subpages":1});
    let global = *authored_identities().last().unwrap();
    enhancement["resources"]["drcs"] =
        json!({"normal":normal_identity(page,0).page(),"global":global.page()});
    enhancement["compositional_links"] = json!({"pop":unused,"gpop":unused,"drcs":linked(normal_identity(page,0)),"gdrcs":linked(global)});
    for object in value["service"]["objects"].as_array_mut().unwrap() {
        object["identity"]["owner"] = owner.clone();
    }
    value["service"]["mot_selections"][0]["display"] = owner.clone();
    value["authoring"]["pages"][0]["page"] = owner;
    value
}

pub(super) fn response(theme: &str, page: u16, subcode: u16) -> Value {
    assert_eq!((page, subcode), (124, 32));
    let mut value = page_response(theme, 124);
    for field in ["pages", "page_enhancements", "mot_selections", "objects"] {
        value["service"][field] = json!([]);
    }
    value["authoring"]["pages"] = json!([]);
    for number in 124..127 {
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
    value["service"]["drcs"] = json!(authored_identities().iter().map(|identity| json!({"identity":{"kind":identity.kind(),"page":identity.page(),"subpage":0},"glyphs":[{"slot":0,"bitmap":[0,0,0,0,0,0,0,0,0,0]}]})).collect::<Vec<_>>());
    let bytes = serde_json::to_vec(&value).unwrap();
    assert!(
        bytes.len() <= encoding().input.max_response_bytes,
        "{}",
        bytes.len()
    );
    value
}

pub(super) fn encoding(
) -> ersatzrs_web::host::teletext_channel_documents::TeletextChannelDocumentEncodingPolicy {
    let mut policy = channel_documents::policy();
    policy.transmissions = authored_identities()
        .into_iter()
        .map(|resource| TeletextServiceResourceTransmission {
            resource,
            counter: TeletextResourceUpdateCounter::new(1).unwrap(),
            update: TeletextResourceFullUpdate::Changed,
            last_subpage: true,
        })
        .collect();
    policy
}
