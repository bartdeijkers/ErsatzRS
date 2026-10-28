//! Explicit context consent through the existing current-receipt draft route.
use super::{
    deadline, fields, resource_controls, respond, AppState, Failure, Form, Response, Result, State,
    TeletextDesignMutation,
};
use crate::host::teletext_editorial_designs::{
    select_teletext_editorial_secondary_default, NativeTeletextSecondaryDefaultRequest,
};
use ersatzrs_domain::{
    TeletextPageCharsetDesignation, TeletextSecondaryDefaultAgreement,
    TeletextSecondaryDefaultChoice,
};

const INVALID: &str = "secondary-default-invalid";

pub async fn choose_secondary_default(
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
            "producer",
            "choice",
            "designation",
        ],
    )?;
    let producer = values
        .get("producer")
        .filter(|value| value.len() <= 20)
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or_else(|| Failure::new(INVALID))?;
    let agreement = match values.get("choice").map(String::as_str) {
        Some("original") => None,
        Some("no_second") => Some(TeletextSecondaryDefaultAgreement::new(
            TeletextSecondaryDefaultChoice::NoSecondG0 {},
        )),
        Some("designated") => {
            let designation = values
                .get("designation")
                .filter(|value| value.len() == 2 && value.bytes().all(|b| b.is_ascii_hexdigit()))
                .and_then(|value| u8::from_str_radix(value, 16).ok())
                .and_then(|value| TeletextPageCharsetDesignation::new(value).ok())
                .ok_or_else(|| Failure::new(INVALID))?;
            Some(TeletextSecondaryDefaultAgreement::new(
                TeletextSecondaryDefaultChoice::Designated { designation },
            ))
        }
        _ => return Err(Failure::new(INVALID)),
    };
    let current = resource_controls::prepare(application, &values, end).await?;
    let result = select_teletext_editorial_secondary_default(
        application,
        NativeTeletextSecondaryDefaultRequest {
            name: current.name,
            receipt_id: current.exported.metadata().id,
            receipt_digest: current.exported.metadata().digest.clone(),
            expected_design_head: current.head,
            producer,
            agreement,
        },
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
