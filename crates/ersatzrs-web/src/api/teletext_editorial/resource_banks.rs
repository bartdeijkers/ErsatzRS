//! Explicit empty-bank creation preserves original source and lifecycle authority.
use super::{
    fields, resource_controls, respond, AppState, BTreeMap, Failure, Form, Response, Result, State,
    TeletextDesignMutation,
};
use crate::host::teletext_editorial_designs::{
    add_teletext_editorial_object_resources, NativeTeletextObjectResourcesAddRequest,
};
use ersatzrs_app::teletext_design_documents::{
    TeletextSavedObjectResourceAddition, TeletextSavedResourceTransmission,
    TeletextSavedResourceUpdate,
};
use ersatzrs_domain::{
    TeletextObjectResourceAddition, TeletextRawDataPageNumber, TeletextResourceIdentity,
    TeletextResourceKind, TeletextResourceSubpageIndex, TeletextResourceUpdateCounter,
};

const INVALID: &str = "resource-bank-invalid";

pub async fn add_resource_bank(
    State(state): State<AppState>,
    Form(pairs): Form<Vec<(String, String)>>,
) -> Response {
    let channel = resource_controls::channel(&pairs);
    match perform(&state, pairs).await {
        Ok(response) => response,
        Err(error) => respond(channel, &error),
    }
}

fn number(values: &BTreeMap<String, String>, field: &str) -> Result<u8> {
    values
        .get(field)
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| Failure::new(INVALID))
}

fn transmission(values: &BTreeMap<String, String>) -> Result<TeletextSavedResourceTransmission> {
    let invalid = || Failure::new(INVALID);
    let kind = match values.get("resource_kind").map(String::as_str) {
        Some("pop") => TeletextResourceKind::Pop,
        Some("gpop") => TeletextResourceKind::Gpop,
        _ => return Err(invalid()),
    };
    let page = values
        .get("resource_page")
        .filter(|page| page.len() == 3 && page.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(invalid)?;
    let raw = u16::from_str_radix(page, 16).map_err(|_| invalid())?;
    let magazine = u8::try_from(raw >> 8).map_err(|_| invalid())?;
    let page =
        TeletextRawDataPageNumber::new(magazine, raw.to_be_bytes()[1]).map_err(|_| invalid())?;
    let resource = TeletextResourceIdentity::new(
        kind,
        page,
        TeletextResourceSubpageIndex::new(number(values, "logical_subpage")?)
            .map_err(|_| invalid())?,
    )
    .map_err(|_| invalid())?;
    let counter =
        TeletextResourceUpdateCounter::new(number(values, "counter")?).map_err(|_| invalid())?;
    let update = match values.get("update").map(String::as_str) {
        Some("unchanged") => TeletextSavedResourceUpdate::Unchanged,
        Some("changed") => TeletextSavedResourceUpdate::Changed,
        _ => return Err(invalid()),
    };
    let last_subpage = match values.get("last_subpage").map(String::as_str) {
        Some("true") => true,
        Some("false") => false,
        _ => return Err(invalid()),
    };
    Ok(TeletextSavedResourceTransmission {
        resource,
        counter,
        update,
        last_subpage,
    })
}

async fn perform(state: &AppState, pairs: Vec<(String, String)>) -> Result<Response> {
    let end = super::deadline()?;
    let values = fields(
        pairs,
        &[
            "channel_id",
            "design_id",
            "revision",
            "digest",
            "expected_design_head",
            "name",
            "display",
            "resource_kind",
            "resource_page",
            "logical_subpage",
            "counter",
            "update",
            "last_subpage",
        ],
    )?;
    let display = resource_controls::display(
        values.get("display").ok_or_else(|| Failure::new(INVALID))?,
        INVALID,
    )?;
    let transmission = transmission(&values)?;
    let context = resource_controls::prepare(&state.application, &values, end).await?;
    let document_index =
        resource_controls::source_ordinal(&context.exported.sidecar().project, display, INVALID)?;
    let bank = TeletextObjectResourceAddition::new(
        transmission.resource,
        Vec::new(),
        context.policy.encoding.composition.service,
    )
    .map_err(|_| Failure::new(INVALID).resource_position(transmission.resource, None, None))?;
    let bank = TeletextSavedObjectResourceAddition::new(document_index, bank, transmission)
        .map_err(|_| Failure::new(INVALID))?;
    let request = NativeTeletextObjectResourcesAddRequest {
        name: context.name,
        receipt_id: context.exported.metadata().id,
        receipt_digest: context.exported.metadata().digest.clone(),
        expected_design_head: context.head,
        banks: vec![bank],
        definitions: Vec::new(),
    };
    let saved = match add_teletext_editorial_object_resources(
        &state.application,
        request,
        context.policy,
        context.end,
    )
    .await
    .map_err(|error| resource_controls::error(error, INVALID))?
    {
        TeletextDesignMutation::Saved(saved) => saved,
        TeletextDesignMutation::Rejected(reason) => {
            return Err(resource_controls::rejection(reason))
        }
    };
    Ok(resource_controls::saved(
        context.channel,
        &context.exported,
        &saved,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values() -> BTreeMap<String, String> {
        [
            ("resource_kind", "pop"),
            ("resource_page", "2c0"),
            ("logical_subpage", "14"),
            ("counter", "7"),
            ("update", "changed"),
            ("last_subpage", "true"),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect()
    }

    #[test]
    fn explicit_bank_identity_and_lifecycle_preserve_values_and_reject_missing_or_invalid_fields() {
        let values = values();
        let bank = transmission(&values).map_err(|error| error.code).unwrap();
        assert_eq!(bank.resource.kind(), TeletextResourceKind::Pop);
        assert_eq!(
            bank.resource.page(),
            TeletextRawDataPageNumber::new(2, 0xc0).unwrap()
        );
        assert_eq!(bank.resource.logical_subpage().get(), 14);
        assert_eq!(bank.counter.get(), 7);
        assert_eq!(bank.update, TeletextSavedResourceUpdate::Changed);
        assert!(bank.last_subpage);
        for field in values.keys() {
            let mut missing = values.clone();
            missing.remove(field);
            assert!(transmission(&missing).is_err(), "{field}");
        }
        for (field, invalid) in [
            ("resource_kind", "drcs"),
            ("resource_page", "2FF"),
            ("resource_page", "0C0"),
            ("resource_page", "20"),
            ("logical_subpage", "16"),
            ("counter", "8"),
            ("update", "auto"),
            ("last_subpage", "14"),
        ] {
            let mut changed = values.clone();
            changed.insert(field.to_owned(), invalid.to_owned());
            assert!(transmission(&changed).is_err(), "{field}={invalid}");
        }
    }
}
