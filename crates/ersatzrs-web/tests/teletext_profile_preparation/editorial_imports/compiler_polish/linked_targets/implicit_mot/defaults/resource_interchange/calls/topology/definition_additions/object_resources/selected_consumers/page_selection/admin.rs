//! The actual POST resolves explicit choices into the already qualified native concept.
use super::*;
use ersatzrs_domain::{
    TeletextEnhancementCommand, TeletextMotResolutionRequirements, TeletextMotRoleResolution,
};
use std::collections::BTreeMap;
#[path = "admin/actual_admin.rs"]
mod actual_admin;
const ROUTE: &str = "/admin-api/teletext/design/choose-object-pages";
type Fields = Vec<(String, String)>;
fn fields(stored: &TeletextDesignRevision, before: &BankExpected, change: bool) -> Fields {
    let mut values = [
        ("channel_id", "1".to_owned()),
        ("design_id", stored.design_id.to_string()),
        ("revision", stored.revision.to_string()),
        ("digest", stored.digest.clone()),
        ("expected_design_head", stored.revision.to_string()),
        ("name", "Explicit selected resource and call".to_owned()),
    ]
    .map(|(k, v)| (k.to_owned(), v))
    .to_vec();
    let mut index = 0;
    for resolution in before.declared.resolutions() {
        for (role, key) in [
            (TeletextX27LinkRole::Pop, "pop"),
            (TeletextX27LinkRole::Gpop, "gpop"),
        ] {
            let TeletextMotRoleResolution::Selected {
                target,
                requirements,
                ..
            } = *resolution.role(role)
            else {
                continue;
            };
            let mask = match requirements {
                TeletextMotResolutionRequirements::MotExtent(extent) => extent.required_mask(),
                TeletextMotResolutionRequirements::X27Mask(mask) => mask,
            };
            let target = if change && role == TeletextX27LinkRole::Gpop {
                resources()[0].page()
            } else {
                target
            };
            values.extend([
                (
                    format!("page_{index}_display"),
                    serde_json::to_string(&resolution.display()).unwrap(),
                ),
                (format!("page_{index}_role"), key.to_owned()),
                (
                    format!("page_{index}_target"),
                    serde_json::to_string(&target).unwrap(),
                ),
                (format!("page_{index}_mask"), format!("{:04X}", mask.get())),
            ]);
            index += 1;
        }
    }
    let mut index = 0;
    for definition in before.declared.semantic().objects().definitions() {
        if definition.identity.resource().is_none() {
            continue;
        }
        for (command_index, command) in definition.program.commands().iter().enumerate() {
            if let TeletextEnhancementCommand::InvokeObject { target, .. } = command {
                let target =
                    if change && definition.identity == seed::target(20) && command_index == 4 {
                        oracle::target()
                    } else {
                        *target
                    };
                values.extend([
                    (
                        format!("call_{index}_caller"),
                        serde_json::to_string(&definition.identity).unwrap(),
                    ),
                    (format!("call_{index}_command"), command_index.to_string()),
                    (
                        format!("call_{index}_target"),
                        serde_json::to_string(&target).unwrap(),
                    ),
                ]);
                index += 1;
            }
        }
    }
    values
}
async fn submit(fixture: &Fixture, values: &Fields) -> BTreeMap<String, String> {
    let response = http_routes::post(
        fixture,
        ROUTE,
        "application/x-www-form-urlencoded",
        serde_urlencoded::to_string(values).unwrap().into_bytes(),
    )
    .await;
    let channel = values
        .iter()
        .find(|(key, _)| key == "channel_id")
        .unwrap()
        .1
        .parse()
        .unwrap();
    http_routes::redirect(&response, channel)
}
async fn unchanged(
    fixture: &mut Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    current: &TeletextDesignRevision,
) {
    assert_head_only(fixture, concept).await;
    assert_eq!(drcs_bitmap::stored(fixture, concept).await, *current);
    assert_original_pointers(fixture, flow).await;
    assert!(fixture.playout.try_recv().is_err());
}
fn invalid_variant(valid: &Fields, unchanged: &Fields, revision: i64, variant: usize) -> Fields {
    let mut input = valid.clone();
    match variant {
        0 => {
            input
                .iter_mut()
                .find(|(key, _)| key == "expected_design_head")
                .unwrap()
                .1 = (revision - 1).to_string();
        }
        1 => input.push(("document_index".into(), "0".into())),
        2 => input.push(("page_0_mask".into(), "0001".into())),
        3 => {
            // A shared call retarget needs the new owner on every consumer.
            for (key, value) in unchanged {
                if key.starts_with("page_") && key.ends_with("_display") {
                    let owner: TeletextPageAddress = serde_json::from_str(value).unwrap();
                    if owner.page.get() != 124 {
                        let target = key.replace("_display", "_target");
                        let old = &unchanged
                            .iter()
                            .find(|(name, _)| name == &target)
                            .unwrap()
                            .1;
                        input
                            .iter_mut()
                            .find(|(name, _)| name == &target)
                            .unwrap()
                            .1
                            .clone_from(old);
                    }
                }
            }
        }
        4 => {
            input
                .iter_mut()
                .find(|(key, _)| key == "channel_id")
                .unwrap()
                .1 = "2".into();
        }
        _ => unreachable!(),
    }
    input
}

async fn save(
    fixture: &Fixture,
    current: &TeletextDesignRevision,
    input: &Fields,
) -> TeletextDesignRevision {
    let response = Box::pin(submit(fixture, input)).await;
    assert_eq!(
        response.get("saved").map(String::as_str),
        Some("draft"),
        "{response:?}"
    );
    assert!(!response.contains_key("error"), "{response:?}");
    let head = fixture
        .state
        .application
        .teletext_design_head(current.design_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(head.revision, current.revision + 1);
    fixture
        .state
        .application
        .teletext_design_revision(
            current.design_id,
            head.revision,
            designs::budget().max_serialized_bytes,
        )
        .await
        .unwrap()
        .unwrap()
}

fn retarget_original_call(input: &mut Fields, target: TeletextObjectIdentity) {
    let prefixes = input
        .iter()
        .filter_map(|(key, value)| {
            let prefix = key.strip_suffix("_caller")?;
            let caller: TeletextObjectIdentity = serde_json::from_str(value).unwrap();
            let command = &input
                .iter()
                .find(|(key, _)| key == &format!("{prefix}_command"))
                .unwrap()
                .1;
            (caller == seed::target(20) && command == "4").then(|| prefix.to_owned())
        })
        .collect::<Vec<_>>();
    assert_eq!(
        prefixes.len(),
        1,
        "exactly one original Active20 command4 row"
    );
    let field = format!("{}_target", prefixes[0]);
    input.iter_mut().find(|(key, _)| key == &field).unwrap().1 =
        serde_json::to_string(&target).unwrap();
}

async fn call_only_roundtrip(
    fixture: &mut Fixture,
    flow: &Flow,
    current: &TeletextDesignRevision,
    selected: &BankExpected,
) -> TeletextDesignRevision {
    let original_call = Box::pin(oracle::qualify_selected_original_call()).await;
    let mut choices = fields(current, selected, false);
    retarget_original_call(&mut choices, seed::target(21));
    let restored_call = Box::pin(save(fixture, current, &choices)).await;
    Box::pin(assert_bank_revision_with_schema(
        fixture,
        flow,
        &restored_call,
        &original_call,
        27,
    ))
    .await;
    assert_original_pointers(fixture, flow).await;
    assert!(fixture.playout.try_recv().is_err());
    let mut choices = fields(&restored_call, &original_call, false);
    retarget_original_call(&mut choices, oracle::target());
    let selected_call = Box::pin(save(fixture, &restored_call, &choices)).await;
    Box::pin(assert_bank_revision_with_schema(
        fixture,
        flow,
        &selected_call,
        selected,
        27,
    ))
    .await;
    assert_original_pointers(fixture, flow).await;
    assert!(fixture.playout.try_recv().is_err());
    assert_eq!(selected_call.revision, 7);
    selected_call
}

#[tokio::test]
async fn explicit_selection_admin_post_preserves_exact_model_and_refuses_uncoordinated_or_stale_choices(
) {
    let [before, announced, selected, _] = Box::pin(oracle::all()).await;
    let (mut fixture, flow, concept) = Box::pin(setup(&before, oracle::original)).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let concept = Box::pin(announce(&fixture, &concept)).await;
    let current = drcs_bitmap::stored(&fixture, &concept).await;
    let unchanged_fields = fields(&current, &announced, false);
    let response = Box::pin(submit(&fixture, &unchanged_fields)).await;
    assert_eq!(
        response.get("error").map(String::as_str),
        Some("editorial-no-changes")
    );
    Box::pin(unchanged(&mut fixture, &flow, &concept, &current)).await;
    let valid = fields(&current, &announced, true);
    for variant in 0..5 {
        let input = invalid_variant(&valid, &unchanged_fields, current.revision, variant);
        let response = Box::pin(submit(&fixture, &input)).await;
        assert!(
            response.contains_key("error"),
            "variant{variant}: {response:?}"
        );
        Box::pin(unchanged(&mut fixture, &flow, &concept, &current)).await;
    }
    let stored = Box::pin(save(&fixture, &current, &valid)).await;
    Box::pin(assert_bank_revision_with_schema(
        &fixture, &flow, &stored, &selected, 27,
    ))
    .await;
    assert_original_pointers(&fixture, &flow).await;
    let stored = Box::pin(call_only_roundtrip(&mut fixture, &flow, &stored, &selected)).await;
    fixture = fixture.reopen().await;
    Box::pin(assert_bank_revision_with_schema(
        &fixture, &flow, &stored, &selected, 27,
    ))
    .await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
