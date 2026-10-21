//! Newly created banks compose with first attachment and exact role-only undo.
use super::*;
use ersatzrs_app::teletext_design_documents::{
    TeletextSavedObjectResourceAddition, TeletextSavedResourceTransmission,
    TeletextSavedResourceUpdate,
};
use ersatzrs_domain::{
    TeletextEnhancementObjectDefinition, TeletextEnhancementPosition, TeletextEnhancementProgram,
    TeletextObjectId, TeletextObjectIdentity, TeletextObjectResourceAddition, TeletextObjectType,
};
use ersatzrs_web::host::{
    teletext_authoring::{preview_original_teletext_design, TeletextAuthoringPreviewRequest},
    teletext_editorial_designs::{
        add_teletext_editorial_object_resources, attach_teletext_editorial_object_pages,
        NativeTeletextObjectPagesChooseRequest, NativeTeletextObjectResourcesAddRequest,
        TeletextObjectPageChoice,
    },
};
#[path = "created_banks/factories.rs"]
mod factories;
#[path = "created_banks/oracles.rs"]
mod oracles;
#[path = "created_banks/workflow.rs"]
mod workflow;

#[tokio::test]
async fn newly_created_pop_gpop_first_attachment_restores_only_roles() {
    let (bank_fixture, banks) = Box::pin(setup_with_fixture_policy_and_writer(
        factories::bank_only,
        Fixture::new_persistent().await,
        factories::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let (target_fixture, target) = Box::pin(setup_with_fixture_policy_and_writer(
        factories::attached,
        Fixture::new_persistent().await,
        factories::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let fixture = Fixture::new_persistent().await;
    http_routes::configure_authoring_policy(&fixture).await;
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        source::absent,
        fixture,
        source::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    oracles::qualify(&flow, &banks, &target);
    bank_fixture.state.pool.close().await;
    target_fixture.state.pool.close().await;
    Box::pin(workflow::complete(fixture, &flow, &banks, &target)).await;
}
