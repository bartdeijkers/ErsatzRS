//! Separately authored complete source and target graphs, not mutation replay.
use super::*;

fn response(theme: &str, page: u16, subcode: u16, crossed: bool) -> Value {
    let mut value = super::super::source::selected(theme, page, subcode);
    let definitions = value["service"]["objects"].as_array_mut().unwrap();
    let mut passive = definitions
        .iter()
        .find(|definition| definition["identity"] == wire_target(oracle::target()))
        .unwrap()
        .clone();
    passive["identity"] = wire_target(public(61));
    assert_eq!(passive["object_type"], "passive");
    assert_eq!(passive["program"]["commands"][4]["operation"]["code"], 81);
    passive["program"]["commands"][4]["operation"]["code"] = json!(82);
    let mut active_definition = definitions
        .iter()
        .find(|definition| definition["identity"] == wire_target(active()))
        .unwrap()
        .clone();
    active_definition["identity"] = wire_target(public(60));
    assert_eq!(active_definition["object_type"], "active");
    active_definition["program"]["commands"][4]["target"] = wire_target(if crossed {
        oracle::target()
    } else {
        public(61)
    });
    definitions.extend([active_definition, passive]);
    if crossed {
        for page in 124..127 {
            let local = definitions
                .iter_mut()
                .find(|definition| {
                    definition["identity"] == serde_json::to_value(local(page)).unwrap()
                })
                .unwrap();
            local["program"]["commands"][1]["target"] = wire_target(public(61));
        }
        for metadata in value["service"]["page_enhancements"]
            .as_array_mut()
            .unwrap()
        {
            metadata["local_program"]["commands"][0]["target"] = wire_target(public(60));
        }
    }
    serde_json::from_value::<ersatzrs_addon_contract::TeletextPagesV3Response>(value.clone())
        .expect("independent complete cross-role source retains closed producer wire");
    value
}
pub(super) fn original(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, false)
}
pub(super) fn crossed(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, true)
}
pub(super) fn generated(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = original(theme, page, subcode);
    let protected = value["authoring"]["pages"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|page| page["page"]["page"] == 125)
        .unwrap();
    protected["regions"][0]["kind"] = json!("generated");
    value
}
async fn qualify_one(factory: fn(&str, u16, u16) -> Value) -> BankExpected {
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
pub(super) async fn qualify() -> [BankExpected; 2] {
    let original = Box::pin(qualify_one(original)).await;
    let crossed = Box::pin(qualify_one(crossed)).await;
    assert_eq!(
        original.declared.semantic().base(),
        crossed.declared.semantic().base()
    );
    assert_eq!(original.declared.mots(), crossed.declared.mots());
    assert_eq!(
        original.declared.resolutions(),
        crossed.declared.resolutions()
    );
    assert_ne!(original.rasters, crossed.rasters);
    let old_definitions = original.declared.semantic().objects().definitions();
    let new_definitions = crossed.declared.semantic().objects().definitions();
    assert_eq!(old_definitions.len(), new_definitions.len());
    for (before, after) in old_definitions.iter().zip(new_definitions) {
        assert_eq!(before.identity, after.identity);
        assert_eq!(before.object_type, after.object_type);
        assert_eq!(
            before.program.commands().len(),
            after.program.commands().len()
        );
        for (before, after) in before
            .program
            .commands()
            .iter()
            .zip(after.program.commands())
        {
            match (*before, *after) {
                (
                    TeletextEnhancementCommand::InvokeObject {
                        object_type: old_type,
                        offset: old_offset,
                        ..
                    },
                    TeletextEnhancementCommand::InvokeObject {
                        object_type: new_type,
                        offset: new_offset,
                        ..
                    },
                ) => {
                    assert_eq!(old_type, new_type);
                    assert_eq!(old_offset, new_offset);
                }
                _ => assert_eq!(before, after),
            }
        }
    }
    [original, crossed]
}
