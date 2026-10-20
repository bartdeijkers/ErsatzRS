//! Independently authored complete sources, not replay of the actions under test.
use super::*;
pub(super) fn resource(global: bool) -> TeletextResourceIdentity {
    let original = seed::target(if global { 20 } else { 21 })
        .resource()
        .unwrap();
    TeletextResourceIdentity::new(
        original.kind(),
        original.page(),
        TeletextResourceSubpageIndex::new(14).unwrap(),
    )
    .unwrap()
}
fn add_bank(value: &mut Value, global: bool, filled: bool) {
    if !filled {
        value["service"]["empty_object_pages"]
            .as_array_mut()
            .unwrap()
            .push(wire_resource(resource(global)));
        return;
    }
    let objects = value["service"]["objects"].as_array_mut().unwrap();
    let mut definition = objects
        .iter()
        .find(|object| object["identity"]["object_id"] == 20)
        .unwrap()
        .clone();
    let identity = if global {
        TeletextObjectIdentity::global(resource(true), TeletextObjectId::new(32))
    } else {
        TeletextObjectIdentity::public(resource(false), TeletextObjectId::new(32))
    }
    .unwrap();
    definition["identity"] = wire_target(identity);
    definition["program"]["commands"] = json!([]);
    let index = objects
        .iter()
        .position(|object| object["identity"]["object_id"] == 25)
        .unwrap();
    objects.insert(index, definition);
}
fn authored(theme: &str, page: u16, subcode: u16, stage: u8) -> Value {
    let mut value = dormant(theme, page, subcode);
    add_bank(&mut value, false, stage >= 1);
    if stage >= 3 {
        add_bank(&mut value, true, stage >= 4);
    }
    if stage >= 2 {
        for metadata in value["service"]["page_enhancements"]
            .as_array_mut()
            .unwrap()
        {
            assert!(metadata["compositional_links"].is_null());
            let link = |page: &Value, mask: u16| json!({"target":{"kind":"page","page":page},"validity":"level25_only","required_subpages":mask});
            metadata["compositional_links"] = json!({
                "gpop":link(&metadata["resources"]["objects"]["global"],if stage>=5 {0x4001}else{1}),
                "pop":link(&metadata["resources"]["objects"]["public"],0x4001),
                "gdrcs":link(&metadata["resources"]["drcs"]["global"],1),
                "drcs":link(&metadata["resources"]["drcs"]["normal"],1),
            });
        }
    }
    value
}
fn pop_empty(theme: &str, page: u16, subcode: u16) -> Value {
    authored(theme, page, subcode, 0)
}
fn pop_filled(theme: &str, page: u16, subcode: u16) -> Value {
    authored(theme, page, subcode, 1)
}
fn pop_announced(theme: &str, page: u16, subcode: u16) -> Value {
    authored(theme, page, subcode, 2)
}
fn gpop_empty(theme: &str, page: u16, subcode: u16) -> Value {
    authored(theme, page, subcode, 3)
}
fn gpop_filled(theme: &str, page: u16, subcode: u16) -> Value {
    authored(theme, page, subcode, 4)
}
fn both_announced(theme: &str, page: u16, subcode: u16) -> Value {
    authored(theme, page, subcode, 5)
}
fn encoding(global: bool) -> TeletextChannelDocumentEncodingPolicy {
    let mut policy = super::super::encoding();
    policy.transmissions.push(lifecycle(resource(false)));
    if global {
        policy.transmissions.push(lifecycle(resource(true)));
    }
    policy
}
fn pop_encoding() -> TeletextChannelDocumentEncodingPolicy {
    encoding(false)
}
fn both_encoding() -> TeletextChannelDocumentEncodingPolicy {
    encoding(true)
}
async fn qualify(factory: fn(&str, u16, u16) -> Value, global: bool) -> BankExpected {
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        factory,
        Fixture::new_persistent().await,
        if global { both_encoding } else { pop_encoding },
        authoring_metadata::write_compact,
    ))
    .await;
    let mut resources = super::super::resources().to_vec();
    resources.push(resource(false));
    if global {
        resources.push(resource(true));
    }
    let expected = bank_expected(flow.accepted[0].service(), &resources);
    fixture.state.pool.close().await;
    expected
}
pub(super) async fn qualify_all() -> Vec<BankExpected> {
    let mut expected = Vec::new();
    for (index, factory) in [
        pop_empty,
        pop_filled,
        pop_announced,
        gpop_empty,
        gpop_filled,
        both_announced,
    ]
    .into_iter()
    .enumerate()
    {
        expected.push(Box::pin(qualify(factory, index >= 3)).await);
    }
    for pair in expected.windows(2) {
        assert_eq!(pair[0].rasters, pair[1].rasters);
        assert_ne!(pair[0].units, pair[1].units);
    }
    expected
}
