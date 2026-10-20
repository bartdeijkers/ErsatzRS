//! Complete source factories independently specify final declarations and calls.
use super::*;

pub(super) fn target() -> TeletextObjectIdentity {
    TeletextObjectIdentity::global(resources()[0], TeletextObjectId::new(32)).unwrap()
}
fn allow_selection(value: &mut Value) {
    // Only this new immutable fixture permits replacement. Old fixtures remain
    // non-supersedable and their rejection contract is unchanged.
    assert_eq!(
        value["service"]["mots"][0]["links"]["global_object"]["supersedable"],
        false
    );
    value["service"]["mots"][0]["links"]["global_object"]["supersedable"] = json!(true);
}
pub(super) fn original(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = super::super::super::super::super::response(theme, page, subcode);
    allow_selection(&mut value);
    value
}
pub(super) fn generated_original(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = original(theme, page, subcode);
    let owner = value["authoring"]["pages"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entry| entry["page"]["page"] == 125)
        .unwrap();
    let region = owner["regions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|region| region["id"] == "nested-call-zone")
        .unwrap();
    assert_eq!(region["kind"], "zone");
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
fn authored(theme: &str, page: u16, subcode: u16, announced: bool, selected: bool) -> Value {
    let mut value = dormant(theme, page, subcode);
    allow_selection(&mut value);
    let objects = value["service"]["objects"].as_array_mut().unwrap();
    let mut passive = objects
        .iter()
        .find(|object| object["identity"]["object_id"] == 21)
        .unwrap()
        .clone();
    assert_eq!(passive["object_type"], "passive");
    passive["identity"] = wire_target(target());
    passive["program"]["commands"][4]["operation"]["code"] = json!(81);
    let index = objects
        .iter()
        .position(|object| object["identity"] == wire_target(new_target(0)))
        .unwrap();
    objects.insert(index + 1, passive);
    if selected {
        let active = objects
            .iter_mut()
            .find(|object| object["identity"] == wire_target(seed::target(20)))
            .unwrap();
        active["program"]["commands"][4]["target"] = wire_target(target());
    }
    if announced {
        for metadata in value["service"]["page_enhancements"]
            .as_array_mut()
            .unwrap()
        {
            assert!(metadata["compositional_links"].is_null());
            let link = |page: &Value, mask: u16| json!({"target":{"kind":"page","page":page},"validity":"level25_only","required_subpages":mask});
            let global = if selected {
                serde_json::to_value(resources()[0].page()).unwrap()
            } else {
                metadata["resources"]["objects"]["global"].clone()
            };
            metadata["compositional_links"] = json!({
                "gpop":link(&global,1),
                "pop":link(&metadata["resources"]["objects"]["public"],0x8001),
                "gdrcs":link(&metadata["resources"]["drcs"]["global"],1),
                "drcs":link(&metadata["resources"]["drcs"]["normal"],1),
            });
            if selected {
                metadata["resources"]["objects"]["global"] = global;
            }
        }
    }
    value
}
pub(super) fn before(theme: &str, page: u16, subcode: u16) -> Value {
    authored(theme, page, subcode, false, false)
}
fn announced(theme: &str, page: u16, subcode: u16) -> Value {
    authored(theme, page, subcode, true, false)
}
fn selected(theme: &str, page: u16, subcode: u16) -> Value {
    authored(theme, page, subcode, true, true)
}
fn edited(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = selected(theme, page, subcode);
    let definition = value["service"]["objects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|object| object["identity"] == wire_target(target()))
        .unwrap();
    assert_eq!(
        definition["program"]["commands"][4]["operation"]["code"],
        81
    );
    definition["program"]["commands"][4]["operation"]["code"] = json!(82);
    value
}
async fn qualify(factory: fn(&str, u16, u16) -> Value) -> BankExpected {
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        factory,
        Fixture::new_persistent().await,
        encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let expected = bank_expected(flow.accepted[0].service(), &resources());
    fixture.state.pool.close().await;
    expected
}
pub(super) async fn all() -> [BankExpected; 4] {
    let before = Box::pin(qualify(before)).await;
    let announced = Box::pin(qualify(announced)).await;
    let selected = Box::pin(qualify(selected)).await;
    assert_eq!(before.rasters, announced.rasters);
    assert_ne!(
        announced.rasters, selected.rasters,
        "new GPOP Q must actually be consumed"
    );
    assert_eq!(
        before.declared.semantic().base(),
        selected.declared.semantic().base(),
        "exact logical Level 1/1.5 fallback"
    );
    assert_eq!(before.declared.mots(), selected.declared.mots());
    assert_eq!(before.declared.selections(), selected.declared.selections());
    let edited = Box::pin(qualify(edited)).await;
    assert_ne!(
        selected.rasters, edited.rasters,
        "fresh ordinary TTI must edit the actually used Q into R"
    );
    assert_eq!(
        selected.declared.semantic().base(),
        edited.declared.semantic().base()
    );
    [before, announced, selected, edited]
}

pub(super) fn selected_original_call(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = selected(theme, page, subcode);
    let active = value["service"]["objects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|object| object["identity"] == wire_target(seed::target(20)))
        .unwrap();
    assert_eq!(
        active["program"]["commands"][4]["target"],
        wire_target(target())
    );
    active["program"]["commands"][4]["target"] = wire_target(seed::target(21));
    value
}
pub(super) async fn qualify_selected_original_call() -> BankExpected {
    qualify(selected_original_call).await
}
