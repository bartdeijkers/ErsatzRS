//! Explicit unused-copy removal through the shared current-draft owner fences.
use super::{
    deadline, fields, resource_controls, respond, AppState, Failure, Form, Response, Result, State,
    TeletextDesignMutation,
};
use crate::host::teletext_editorial_designs::{
    remove_unused_teletext_editorial_resource_definition,
    NativeTeletextResourceDefinitionRemoveRequest,
};
use ersatzrs_domain::TeletextObjectIdentity;

const INVALID: &str = "object-remove-invalid";

pub async fn remove_unused_object(
    State(state): State<AppState>,
    Form(pairs): Form<Vec<(String, String)>>,
) -> Response {
    let channel = resource_controls::channel(&pairs);
    match perform(&state.application, pairs).await {
        Ok(response) => response,
        Err(error) => respond(channel, &error),
    }
}

async fn perform(
    application: &super::ApplicationServices,
    pairs: Vec<(String, String)>,
) -> Result<Response> {
    let end = deadline()?;
    let values = fields(
        pairs,
        &[
            "channel_id",
            "design_id",
            "revision",
            "digest",
            "expected_design_head",
            "name",
            "object",
        ],
    )?;
    let identity = values
        .get("object")
        .filter(|value| value.len() <= 512)
        .and_then(|value| serde_json::from_str::<TeletextObjectIdentity>(value).ok())
        .filter(|identity| identity.resource().is_some())
        .ok_or_else(|| Failure::new(INVALID))?;
    let current = resource_controls::prepare(application, &values, end).await?;
    let request = NativeTeletextResourceDefinitionRemoveRequest {
        name: current.name,
        receipt_id: current.exported.metadata().id,
        receipt_digest: current.exported.metadata().digest.clone(),
        expected_design_head: current.head,
        identity,
    };
    let result = remove_unused_teletext_editorial_resource_definition(
        application,
        request,
        current.policy,
        current.end,
    )
    .await
    .map_err(|error| resource_controls::error(error, INVALID))?;
    match result {
        TeletextDesignMutation::Saved(saved) => Ok(resource_controls::saved(
            current.channel,
            &current.exported,
            &saved,
        )),
        TeletextDesignMutation::Rejected(reason) => Err(resource_controls::rejection(reason)),
    }
}
