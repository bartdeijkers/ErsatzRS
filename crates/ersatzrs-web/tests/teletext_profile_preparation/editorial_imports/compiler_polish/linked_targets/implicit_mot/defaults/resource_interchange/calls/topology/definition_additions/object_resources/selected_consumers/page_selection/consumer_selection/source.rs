//! Independently authored complete original and selected producer documents.
use super::*;

fn response(theme: &str, page: u16, subcode: u16, selected: bool) -> Value {
    let mut value = if selected {
        oracle::selected_original_call(theme, page, subcode)
    } else {
        oracle::before(theme, page, subcode)
    };
    let objects = value["service"]["objects"].as_array_mut().unwrap();
    let mut old_passive_definition = objects
        .iter()
        .find(|object| object["identity"] == wire_target(seed::target(21)))
        .unwrap()
        .clone();
    old_passive_definition["identity"] = wire_target(old_passive());
    objects.push(old_passive_definition);
    let mut new_active = objects
        .iter()
        .find(|object| object["identity"] == wire_target(seed::target(20)))
        .unwrap()
        .clone();
    new_active["identity"] = wire_target(active());
    new_active["program"]["commands"][4]["target"] = wire_target(if selected {
        oracle::target()
    } else {
        old_passive()
    });
    objects.push(new_active);
    for display in 124..127 {
        let owner = channel_documents::address(display, 32);
        let identity = json!({"scope":"local","owner":owner,"object_id":65000});
        objects.push(json!({"identity":identity,"object_type":"active","program":{"commands":[
            {"kind":"set_position","position":{"row":9,"column":3}},
            {"kind":"invoke_object","target":wire_target(if selected {oracle::target()} else {old_passive()}),
                "object_type":"passive","offset":{"row":0,"column":0}}
        ]}}));
    }
    for metadata in value["service"]["page_enhancements"]
        .as_array_mut()
        .unwrap()
    {
        let owner = metadata["address"].clone();
        metadata["local_program"] = json!({"commands":[
            {"kind":"invoke_object","target":wire_target(if selected {active()} else {seed::target(20)}),
                "object_type":"active","offset":null},
            {"kind":"invoke_object","target":{"scope":"local","owner":owner,"object_id":65000},
                "object_type":"active","offset":null}
        ]});
        if selected {
            metadata["compositional_links"]["pop"]["required_subpages"] = json!(1);
        }
    }
    // The complete Main-body zone is intentional original authority for newly
    // retargeted writer trees; the separate Generated fixture proves refusal.
    for declaration in value["authoring"]["pages"].as_array_mut().unwrap() {
        declaration["regions"] = json!([{"id":"coordinated-consumers","kind":"zone","area":"main",
            "row":1,"column":0,"width":40,"height":24}]);
        declaration["dynamic_fields"] = json!([]);
    }
    serde_json::from_value::<ersatzrs_addon_contract::TeletextPagesV3Response>(value.clone())
        .expect("the independent complete fixture must match the closed producer V3 wire");
    value
}
pub(super) fn original(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, false)
}
pub(super) fn selected(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, true)
}
pub(super) fn edited(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = selected(theme, page, subcode);
    let definition = value["service"]["objects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|object| object["identity"] == wire_target(oracle::target()))
        .unwrap();
    assert_eq!(
        definition["program"]["commands"][4]["operation"]["code"],
        81
    );
    definition["program"]["commands"][4]["operation"]["code"] = json!(82);
    value
}
pub(super) fn generated(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = original(theme, page, subcode);
    let page = value["authoring"]["pages"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|page| page["page"]["page"] == 125)
        .unwrap();
    page["regions"][0]["kind"] = json!("generated");
    value
}
async fn qualify(factory: fn(&str, u16, u16) -> Value) -> BankExpected {
    let fixture = Fixture::new_persistent().await;
    authoring_metadata::install(&fixture).await;
    authoring_metadata::write_compact(&fixture, "day", &factory("day", 124, 32));
    let profile = fixture.profile("day", "day", "good").await;
    fixture.bind(1, &profile, None).await;
    let candidate = Box::pin(prepare_with_policy(&fixture, 1, encoding())).await;
    let expected = bank_expected(candidate.encoded().unwrap(), &resources());
    fixture.state.pool.close().await;
    expected
}
pub(super) async fn all() -> [BankExpected; 3] {
    let original = Box::pin(qualify(original)).await;
    let selected = Box::pin(qualify(selected)).await;
    assert_eq!(
        original.declared.semantic().base(),
        selected.declared.semantic().base()
    );
    assert_eq!(original.declared.mots(), selected.declared.mots());
    assert_ne!(original.rasters, selected.rasters);
    let edited = Box::pin(qualify(edited)).await;
    assert_ne!(selected.rasters, edited.rasters);
    [original, selected, edited]
}
