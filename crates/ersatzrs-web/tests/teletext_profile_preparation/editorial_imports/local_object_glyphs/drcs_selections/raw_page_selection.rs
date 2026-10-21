//! Retained DRCS raw-page choices require explicit native selection authority.
use super::*;
use ersatzrs_app::teletext_design_documents::TeletextSavedObjectPageSelectionChange;
use ersatzrs_domain::{
    TeletextObjectPageSelectionChange, TeletextSubpageRequirementMask, TeletextX27LinkDeclaration,
    TeletextX27LinkRole, TeletextX27LinkTarget, TeletextX27LinkValidity,
};
use ersatzrs_web::host::teletext_editorial_designs::{
    select_teletext_editorial_object_pages, NativeTeletextObjectPagesSelectRequest,
};
#[path = "raw_page_selection/actual_admin.rs"]
mod actual_admin;
#[path = "raw_page_selection/checks.rs"]
mod checks;
#[path = "raw_page_selection/lifecycle.rs"]
mod lifecycle;
#[path = "raw_page_selection/negatives.rs"]
mod negatives;
#[path = "raw_page_selection/source.rs"]
mod source;

fn choices(
    service: &EncodedTeletextAuthoredService,
    selected: bool,
) -> Vec<TeletextSavedObjectPageSelectionChange> {
    let display = channel_documents::address(124, 32);
    let metadata = &service.declared().semantic().page_enhancements()[&display];
    let resolution = service
        .declared()
        .resolutions()
        .iter()
        .find(|resolution| resolution.display() == display)
        .unwrap();
    [TeletextX27LinkRole::Drcs, TeletextX27LinkRole::Gdrcs]
        .map(|role| {
            let scope = match role {
                TeletextX27LinkRole::Drcs => TeletextDrcsTableScope::Normal,
                TeletextX27LinkRole::Gdrcs => TeletextDrcsTableScope::Global,
                _ => unreachable!(),
            };
            let replacement = TeletextX27LinkDeclaration::new(
                TeletextX27LinkTarget::Page {
                    page: if selected {
                        source::alternate(scope, 0).page()
                    } else {
                        bank(scope, 0).page()
                    },
                },
                TeletextX27LinkValidity::Level25Only,
                TeletextSubpageRequirementMask::new(3),
            )
            .unwrap();
            let change = TeletextObjectPageSelectionChange::new(
                display,
                role,
                metadata.compositional_links,
                *resolution.role(role),
                replacement,
            )
            .expect(
                "qualified retained DRCS/GDRCS raw-page choice needs native selection authority",
            );
            TeletextSavedObjectPageSelectionChange::new(0, change).unwrap()
        })
        .to_vec()
}

#[tokio::test]
async fn retained_normal_and_global_raw_page_choices_use_explicit_native_selection() {
    let (target_fixture, target) = Box::pin(setup_with_fixture_policy(
        source::selected,
        Fixture::new_persistent().await,
        source::encoding,
    ))
    .await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        source::original,
        Fixture::new_persistent().await,
        source::encoding,
    ))
    .await;
    checks::qualified(&flow, &target);
    target_fixture.state.pool.close().await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let saved = lifecycle::save(
        &fixture,
        &flow.receipts[0],
        flow.accepted[0].service(),
        true,
    )
    .await;
    assert_eq!(saved.head().revision, 2);
    Box::pin(lifecycle::concept(
        &fixture,
        &flow,
        &saved,
        target.accepted[0].service(),
        true,
    ))
    .await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    Box::pin(lifecycle::concept(
        &fixture,
        &flow,
        &saved,
        target.accepted[0].service(),
        true,
    ))
    .await;
    let accepted = lifecycle::publish(&fixture, &saved, &flow.accepted[0]).await;
    lifecycle::same(accepted.service(), target.accepted[0].service());
    Box::pin(negatives::fresh_tti(&fixture, &flow, &saved)).await;
    Box::pin(lifecycle::undo(
        fixture, &flow, &saved, &accepted, &retained,
    ))
    .await;
}
