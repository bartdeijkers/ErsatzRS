//! Native Public/Global call crossing, retaining selected banks and call semantics.
use super::*;
use ersatzrs_web::host::teletext_editorial_designs::{
    choose_teletext_editorial_object_consumers, NativeTeletextObjectConsumersChooseRequest,
    TeletextMainCallChoice, TeletextObjectCallChoice,
};
#[path = "cross_roles/source.rs"]
mod source;
#[path = "cross_roles/workflow.rs"]
mod workflow;

fn public(id: u16) -> TeletextObjectIdentity {
    TeletextObjectIdentity::public(
        seed::target(21).resource().unwrap(),
        TeletextObjectId::new(id),
    )
    .unwrap()
}
fn choices(
    receipt: &TeletextDesignExportReceipt,
    head: i64,
    crossing: bool,
) -> NativeTeletextObjectConsumersChooseRequest {
    let mut calls = (124..127)
        .map(|page| TeletextObjectCallChoice {
            caller: local(page),
            command_index: 1,
            target: if crossing {
                public(61)
            } else {
                oracle::target()
            },
        })
        .collect::<Vec<_>>();
    calls.push(TeletextObjectCallChoice {
        caller: public(60),
        command_index: 4,
        target: if crossing {
            oracle::target()
        } else {
            public(61)
        },
    });
    NativeTeletextObjectConsumersChooseRequest {
        name: "Native Public and Global consumer target crossing".into(),
        receipt_id: receipt.metadata.id,
        receipt_digest: receipt.metadata.digest.clone(),
        expected_design_head: head,
        selections: vec![],
        calls,
        main_calls: (124..127)
            .map(|page| TeletextMainCallChoice {
                owner: channel_documents::address(page, 32),
                command_index: 0,
                target: if crossing { public(60) } else { active() },
            })
            .collect(),
    }
}
async fn choose(
    fixture: &Fixture,
    receipt: &TeletextDesignExportReceipt,
    head: i64,
    crossing: bool,
) -> SavedEditorialTeletextDesign {
    let result = choose_teletext_editorial_object_consumers(
        &fixture.state.application,
        choices(receipt, head, crossing),
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextDesignMutation::Saved(saved) = result else {
        panic!("complete cross-role choices save an unpublished concept")
    };
    saved
}
async fn current(
    fixture: &Fixture,
    concept: &SavedEditorialTeletextDesign,
) -> TeletextDesignExportReceipt {
    let exported = drcs_bitmap::reexport(fixture, concept).await;
    receipt(fixture, &exported).await
}
async fn setup(factory: fn(&str, u16, u16) -> Value) -> (Fixture, Flow) {
    let fixture = Fixture::new_persistent().await;
    http_routes::configure_authoring_policy(&fixture).await;
    Box::pin(setup_with_fixture_policy_and_writer(
        factory,
        fixture,
        encoding,
        authoring_metadata::write_compact,
    ))
    .await
}

#[tokio::test]
async fn public_global_main_local_resource_choices_preserve_tti_undo_publication_and_cold_state() {
    let [original, crossed] = Box::pin(source::qualify()).await;
    let (mut fixture, flow) = Box::pin(setup(source::original)).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    assert_bank_output(flow.accepted[0].service(), &original);
    Box::pin(workflow::target_refusals(&fixture, &flow)).await;
    let concept = Box::pin(choose(&fixture, &flow.receipts[0], 1, true)).await;
    let id = concept.head().id;
    Box::pin(workflow::check(&fixture, &flow, id, 2, &crossed, true)).await;
    assert_original_pointers(&fixture, &flow).await;
    let receipt = Box::pin(current(&fixture, &concept)).await;
    let unchanged = import_teletext_editorial_design(
        &fixture.state.application,
        drcs_bitmap::from_receipt(&receipt, 2),
        policy(),
        designs::deadline(),
    )
    .await;
    assert!(matches!(
        unchanged,
        Err(NativeTeletextEditorialImportError::NoChanges)
    ));
    fixture = fixture.reopen().await;
    Box::pin(workflow::check(&fixture, &flow, id, 2, &crossed, true)).await;
    let undone = Box::pin(choose(&fixture, &receipt, 2, false)).await;
    let root = Box::pin(workflow::check(&fixture, &flow, id, 3, &original, false)).await;
    assert_eq!(root.document, flow.originals[0].document);
    let receipt = Box::pin(current(&fixture, &undone)).await;
    let concept = Box::pin(choose(&fixture, &receipt, 3, true)).await;
    let stored = Box::pin(workflow::check(&fixture, &flow, id, 4, &crossed, true)).await;
    assert_original_pointers(&fixture, &flow).await;
    let accepted = super::super::super::admin::publish(&fixture, &flow, &stored).await;
    assert_bank_output(accepted.service(), &crossed);
    fixture = fixture.reopen().await;
    for (revision, expected, crossing) in [
        (2, &crossed, true),
        (3, &original, false),
        (4, &crossed, true),
    ] {
        Box::pin(workflow::check(
            &fixture, &flow, id, revision, expected, crossing,
        ))
        .await;
    }
    assert_pointers(&fixture, &accepted, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    assert_eq!(concept.head().revision, 4);
    fixture.state.pool.close().await;
    Box::pin(workflow::generated_refusal(&original)).await;
}
