//! First native attachment of all retained bank roles and exact original absence.
use super::*;
use ersatzrs_domain::TeletextMotRoleResolution;
#[path = "first_attachment/actual_admin.rs"]
mod actual_admin;
#[path = "first_attachment/checks.rs"]
mod checks;
#[path = "first_attachment/created_banks.rs"]
mod created_banks;
#[path = "first_attachment/lifecycle.rs"]
mod lifecycle;
#[path = "first_attachment/source.rs"]
mod source;

fn choices(
    service: &EncodedTeletextAuthoredService,
) -> Vec<TeletextSavedObjectPageSelectionChange> {
    let metadata = &service.declared().semantic().page_enhancements()[&source::display()];
    TeletextX27LinkRole::ALL
        .map(|role| {
            TeletextSavedObjectPageSelectionChange::new(
                0,
                TeletextObjectPageSelectionChange::new_attachment(
                    source::display(),
                    role,
                    metadata.compositional_links,
                    source::declaration(role),
                )
                .unwrap(),
            )
            .unwrap()
        })
        .to_vec()
}

#[tokio::test]
async fn retained_four_role_first_attachment_preserves_root_presence_and_explicit_publication() {
    for (original, selected, explicit) in [
        (
            source::absent as fn(&str, u16, u16) -> Value,
            source::selected_absent as fn(&str, u16, u16) -> Value,
            false,
        ),
        (source::explicit, source::selected_explicit, true),
    ] {
        let (target_fixture, target) = Box::pin(setup_with_fixture_policy_and_writer(
            selected,
            Fixture::new_persistent().await,
            source::encoding,
            authoring_metadata::write_compact,
        ))
        .await;
        let (retargeted_fixture, retargeted) = Box::pin(setup_with_fixture_policy_and_writer(
            source::retargeted,
            Fixture::new_persistent().await,
            source::encoding,
            authoring_metadata::write_compact,
        ))
        .await;
        retargeted_fixture.state.pool.close().await;
        let (partial_fixture, partial) = Box::pin(setup_with_fixture_policy_and_writer(
            source::partial,
            Fixture::new_persistent().await,
            source::encoding,
            authoring_metadata::write_compact,
        ))
        .await;
        partial_fixture.state.pool.close().await;
        let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
            original,
            Fixture::new_persistent().await,
            source::encoding,
            authoring_metadata::write_compact,
        ))
        .await;
        checks::qualified(&flow, &target, explicit);
        target_fixture.state.pool.close().await;
        Box::pin(lifecycle::complete(
            fixture,
            &flow,
            &target,
            &retargeted,
            &partial,
        ))
        .await;
    }
}
