//! Direct Main AP0,8 survives Local-call growth, deletion and exact root undo.

use super::{lifecycle::*, *};

#[path = "deferred_ap/checks.rs"]
mod checks;
#[path = "deferred_ap/guards.rs"]
mod guards;
#[path = "deferred_ap/roundtrip.rs"]
mod roundtrip;

fn main_program(stage: u8) -> TeletextEnhancementProgram {
    let mut commands = vec![anchor()];
    for column in match stage {
        0 => &[24][..],
        1 => &[24, 25][..],
        2 => &[][..],
        _ => unreachable!(),
    } {
        commands.push(TeletextEnhancementCommand::InvokeObject {
            target: identity(65520),
            object_type: TeletextObjectType::Passive,
            offset: Some(TeletextObjectOffset::new(1, *column).unwrap()),
        });
    }
    TeletextEnhancementProgram::new(commands).unwrap()
}

fn direct_response<const STAGE: u8>(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = response(theme, page, subcode);
    if theme == "day" && page == 124 {
        let display = value["service"]["page_enhancements"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|page| page["address"]["page"] == 124)
            .unwrap();
        display["local_program"] = serde_json::to_value(main_program(STAGE)).unwrap();
    }
    value
}

fn target_request(
    receipt: &TeletextDesignExportReceipt,
    head: i64,
    target: &Flow,
) -> NativeTeletextEditorialImportRequest {
    let mut request = drcs_bitmap::from_receipt(receipt, head);
    "Edit direct Main calls after retained AP0,8".clone_into(&mut request.name);
    replace_packets(&mut request, native_words(target.accepted[0].service()));
    assert_complete_tti(&request, target);
    request
}

#[tokio::test]
async fn direct_main_deferred_ap_call_growth_empty_root_undo_publication_and_recovery() {
    // Every complete original/target factory is validated before interchange.
    // A failure here is factory correctness, rather than an import refusal.
    let (fixture, original) = target(direct_response::<0>).await;
    let (grown_fixture, grown) = target(direct_response::<1>).await;
    let (empty_fixture, empty) = target(direct_response::<2>).await;
    for (stage, expected) in [(0, &original), (1, &grown), (2, &empty)] {
        assert_target_preserves_catalogue(&original, expected);
        checks::main_provenance(expected.accepted[0].service(), stage);
    }
    Box::pin(roundtrip::run(fixture, original, &grown, &empty)).await;
    grown_fixture.state.pool.close().await;
    empty_fixture.state.pool.close().await;
}
