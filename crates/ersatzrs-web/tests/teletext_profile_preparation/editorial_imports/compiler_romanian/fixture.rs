//! Four V3 owners retain template/theme/edit metadata and Romanian primary1F.

use super::*;

fn page_response(theme: &str, page: u16) -> Value {
    let mut value = compiler_polish::response(theme, 124, 32);
    let owner = json!({"page":page,"subcode":32});
    value["service"]["pages"][0]["address"] = owner.clone();
    value["service"]["pages"][0]["navigation"][0]["target"]["page"] =
        json!(if page == 127 { 124 } else { page + 1 });
    let enhancement = &mut value["service"]["page_enhancements"][0];
    enhancement["address"] = owner.clone();
    enhancement["presentation"]["primary"] = json!(0x1f);
    for command in enhancement["local_program"]["commands"]
        .as_array_mut()
        .unwrap()
    {
        if command["kind"] == "invoke_object" {
            command["target"]["owner"] = owner.clone();
        }
    }
    for object in value["service"]["objects"].as_array_mut().unwrap() {
        object["identity"]["owner"] = owner.clone();
        if object["identity"]["object_id"] == 0xfffe {
            assert_eq!(object["object_type"], "adaptive");
            let attributes = &mut object["program"]["commands"][0]["operation"];
            assert_eq!(attributes["kind"], "attributes");
            assert_eq!(attributes["double_width"], true);
            assert_eq!(attributes["double_height"], false);
            attributes["underline_or_separated_mosaics"] = json!(variant(page) % 2 == 1);
        }
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
    for page in 124..128 {
        let source = page_response(theme, page);
        for field in ["pages", "page_enhancements", "mot_selections"] {
            value["service"][field]
                .as_array_mut()
                .unwrap()
                .push(source["service"][field][0].clone());
        }
        value["service"]["objects"].as_array_mut().unwrap().extend(
            source["service"]["objects"]
                .as_array()
                .unwrap()
                .iter()
                .cloned(),
        );
        value["authoring"]["pages"]
            .as_array_mut()
            .unwrap()
            .push(source["authoring"]["pages"][0].clone());
    }
    assert!(
        serde_json::to_vec(&value).unwrap().len()
            <= channel_documents::policy().input.max_response_bytes
    );
    value
}
