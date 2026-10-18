//! A real shared invocation must not bypass one consumer's generated region.
use super::support;
use super::*;
use std::collections::BTreeSet;

fn called(theme: &str, page: u16, subcode: u16, insert: bool) -> Value {
    let mut value = if insert {
        original_inserted_row(theme, page, subcode)
    } else {
        original_outer_rows(theme, page, subcode)
    };
    let other = if insert {
        original_inserted_row(theme, 125, subcode)
    } else {
        original_outer_rows(theme, 125, subcode)
    };
    for key in ["pages", "page_enhancements", "mot_selections"] {
        value["service"][key]
            .as_array_mut()
            .unwrap()
            .push(other["service"][key][0].clone());
    }
    let mut metadata = other["authoring"]["pages"][0].clone();
    metadata["regions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|region| region["id"] == "custom-zone")
        .unwrap()["kind"] = json!("generated");
    value["authoring"]["pages"]
        .as_array_mut()
        .unwrap()
        .push(metadata);
    for enhancement in value["service"]["page_enhancements"]
        .as_array_mut()
        .unwrap()
    {
        enhancement["local_program"] = json!({"commands":[
            {"kind":"set_position","position":{"row":1,"column":0}},
            {"kind":"invoke_object","target":wire_identity(0),"object_type":"passive",
                "offset":{"row":0,"column":0}}
        ]});
    }
    validate_response(&value);
    value
}
fn before(theme: &str, page: u16, subcode: u16) -> Value {
    called(theme, page, subcode, false)
}
fn after(theme: &str, page: u16, subcode: u16) -> Value {
    called(theme, page, subcode, true)
}

fn raster(service: &EncodedTeletextAuthoredService, page: u16) -> TeletextRaster {
    let preview = project_teletext_preview(
        service,
        channel_documents::address(page, 32),
        ersatzrs_domain::TeletextColourPixelOptions {
            reveal_concealed: false,
            flash_phase: ersatzrs_domain::TeletextFlashPhase::Original,
        },
        preview_budget(),
    )
    .unwrap();
    assert!(preview.raster().diagnostics().is_empty());
    preview.into_parts().0
}

async fn qualified_called_target() -> (BodyExpected, Vec<TeletextRaster>) {
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        after,
        Fixture::new_persistent().await,
        encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let expected = support::expected(&flow, identity(0));
    let rasters = (124..=125)
        .map(|page| raster(flow.accepted[0].service(), page))
        .collect();
    fixture.state.pool.close().await;
    (expected, rasters)
}
fn assert_executed_consumers(service: &EncodedTeletextAuthoredService, changed: &[TeletextRaster]) {
    for (page, after) in (124..=125).zip(changed) {
        let before = raster(service, page);
        assert_eq!((before.width(), before.height()), (480, 250));
        assert_eq!((after.width(), after.height()), (480, 250));
        let mut cells = BTreeSet::new();
        for row in 0..250 {
            for column in 0..480 {
                if before.pixel(column, row) != after.pixel(column, row) {
                    cells.insert((row / 10, column / 12));
                }
            }
        }
        assert_eq!(
            cells,
            BTreeSet::from([(3, 6)]),
            "both real consumers execute inserted row2 R at inherited origin row1"
        );
    }
}

#[tokio::test]
async fn original_resource_positions_reject_executed_generated_shared_consumer_atomically() {
    let (expected, changed) = Box::pin(qualified_called_target()).await;
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        before,
        Fixture::new_persistent().await,
        encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    assert_executed_consumers(flow.accepted[0].service(), &changed);
    let retained_state = retained(&fixture).await;
    let result = import_teletext_editorial_design(
        &fixture.state.application,
        support::request(
            &flow.exported[0],
            &flow.receipts[0],
            identity(0),
            &expected.records,
        ),
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap_err();
    let NativeTeletextEditorialImportError::Effects(error) = result else {
        panic!("valid original row insertion must reach executed-consumer effects: {result:?}");
    };
    assert_eq!(
        error.reason,
        TeletextEditorialEffectsFailure::ProtectedDisplay
    );
    assert_eq!(error.page, Some(channel_documents::address(125, 32)));
    assert_eq!(error.position, Some((3, 6)));
    assert_eq!(
        fixture
            .state
            .application
            .teletext_design_head(flow.saved[0].head().id)
            .await
            .unwrap(),
        Some(flow.saved[0].head().clone())
    );
    assert!(fixture
        .state
        .application
        .teletext_design_revision(
            flow.saved[0].head().id,
            2,
            designs::budget().max_serialized_bytes
        )
        .await
        .unwrap()
        .is_none());
    unchanged(&fixture, &flow, &retained_state).await;
    assert_retained_storage(&fixture, &flow).await;
    assert!(fixture.playout.is_empty());
    fixture.state.pool.close().await;
}
