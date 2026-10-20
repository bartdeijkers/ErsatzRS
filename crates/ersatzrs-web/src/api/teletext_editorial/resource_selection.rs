//! Explicit UI destinations are resolved only inside the authenticated native worker.
use super::{
    fields, resource_controls, respond, AppState, BTreeMap, Failure, Form, Response, Result, State,
    TeletextDesignMutation, MAX_PARTS,
};
use crate::host::teletext_editorial_designs::{
    choose_teletext_editorial_object_pages, NativeTeletextObjectPagesChooseRequest,
    TeletextObjectCallChoice, TeletextObjectPageChoice,
};
use ersatzrs_domain::{
    TeletextObjectId, TeletextObjectIdentity, TeletextRawDataPageNumber, TeletextResourceIdentity,
    TeletextResourceKind, TeletextResourceSubpageIndex, TeletextSubpageRequirementMask,
    TeletextX27LinkRole,
};
use serde::de::DeserializeOwned;

const INVALID: &str = "object-selection-invalid";
type Rows = BTreeMap<usize, BTreeMap<String, String>>;
type Choices = (Vec<TeletextObjectPageChoice>, Vec<TeletextObjectCallChoice>);

pub async fn choose_object_pages(
    State(state): State<AppState>,
    Form(pairs): Form<Vec<(String, String)>>,
) -> Response {
    let channel = resource_controls::channel(&pairs);
    match perform(&state, pairs).await {
        Ok(response) => response,
        Err(error) => respond(channel, &error),
    }
}

fn invalid() -> Failure {
    Failure::new(INVALID)
}
fn row<'a>(key: &'a str, prefix: &str) -> Result<Option<(usize, &'a str)>> {
    let Some(suffix) = key.strip_prefix(prefix) else {
        return Ok(None);
    };
    let (index, field) = suffix.split_once('_').ok_or_else(invalid)?;
    let number = index.parse::<usize>().map_err(|_| invalid())?;
    if number >= MAX_PARTS || number.to_string() != index {
        return Err(invalid());
    }
    Ok(Some((number, field)))
}
fn insert(rows: &mut Rows, index: usize, field: &str, value: String) -> Result<()> {
    if rows
        .entry(index)
        .or_default()
        .insert(field.to_owned(), value)
        .is_some()
    {
        return Err(invalid());
    }
    Ok(())
}
fn take(values: &mut BTreeMap<String, String>, field: &str) -> Result<String> {
    values.remove(field).ok_or_else(invalid)
}
fn json<T: DeserializeOwned>(value: &str) -> Result<T> {
    serde_json::from_str(value).map_err(|_| invalid())
}
fn page_choices(rows: Rows) -> Result<Vec<TeletextObjectPageChoice>> {
    rows.into_iter()
        .enumerate()
        .map(|(expected, (index, mut values))| {
            if expected != index || values.len() != 4 {
                return Err(invalid());
            }
            let display = json(&take(&mut values, "display")?)?;
            let role = match take(&mut values, "role")?.as_str() {
                "pop" => TeletextX27LinkRole::Pop,
                "gpop" => TeletextX27LinkRole::Gpop,
                _ => return Err(invalid()),
            };
            let target = raw_page(&take(&mut values, "target")?)?;
            let mask = take(&mut values, "mask")?;
            if mask.is_empty() || mask.len() > 4 || !mask.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(invalid());
            }
            let required_subpages = TeletextSubpageRequirementMask::new(
                u16::from_str_radix(&mask, 16).map_err(|_| invalid())?,
            );
            Ok(TeletextObjectPageChoice {
                display,
                role,
                target,
                required_subpages,
            })
        })
        .collect()
}
fn raw_page(value: &str) -> Result<TeletextRawDataPageNumber> {
    if value.starts_with('{') {
        return json(value);
    }
    if value.len() != 3 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid());
    }
    let raw = u16::from_str_radix(value, 16).map_err(|_| invalid())?;
    TeletextRawDataPageNumber::new(raw.to_be_bytes()[0], raw.to_be_bytes()[1])
        .map_err(|_| invalid())
}
fn object_target(value: &str) -> Result<TeletextObjectIdentity> {
    // Structured agents retain the typed JSON choice; the native datalist uses
    // the same visible resource identity once per type, without repeated lists.
    if value.starts_with('{') {
        return json(value);
    }
    let parts = value.split_whitespace().collect::<Vec<_>>();
    let [kind, raw, subpage, object] = parts.as_slice() else {
        return Err(invalid());
    };
    let kind = match *kind {
        "POP" => TeletextResourceKind::Pop,
        "GPOP" => TeletextResourceKind::Gpop,
        _ => return Err(invalid()),
    };
    if raw.len() != 3 || !raw.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid());
    }
    let page = u16::from_str_radix(raw, 16).map_err(|_| invalid())?;
    let page = TeletextRawDataPageNumber::new(page.to_be_bytes()[0], page.to_be_bytes()[1])
        .map_err(|_| invalid())?;
    let subpage = subpage
        .strip_prefix("S1=")
        .ok_or_else(invalid)?
        .parse::<u8>()
        .map_err(|_| invalid())?;
    let subpage = TeletextResourceSubpageIndex::new(subpage).map_err(|_| invalid())?;
    let object = object
        .strip_prefix('#')
        .ok_or_else(invalid)?
        .parse::<u16>()
        .map_err(|_| invalid())?;
    let resource = TeletextResourceIdentity::new(kind, page, subpage).map_err(|_| invalid())?;
    let id = TeletextObjectId::new(object);
    match kind {
        TeletextResourceKind::Pop => TeletextObjectIdentity::public(resource, id),
        TeletextResourceKind::Gpop => TeletextObjectIdentity::global(resource, id),
        _ => return Err(invalid()),
    }
    .map_err(|_| invalid())
}
fn call_choices(rows: Rows) -> Result<Vec<TeletextObjectCallChoice>> {
    rows.into_iter()
        .enumerate()
        .map(|(expected, (index, mut values))| {
            if expected != index || values.len() != 3 {
                return Err(invalid());
            }
            let caller = json(&take(&mut values, "caller")?)?;
            let command_index = take(&mut values, "command")?
                .parse()
                .map_err(|_| invalid())?;
            let target = object_target(&take(&mut values, "target")?)?;
            Ok(TeletextObjectCallChoice {
                caller,
                command_index,
                target,
            })
        })
        .collect()
}
fn input(pairs: Vec<(String, String)>) -> Result<(BTreeMap<String, String>, Choices)> {
    if pairs.len() > MAX_PARTS {
        return Err(Failure::new("editorial-limit"));
    }
    let mut common = Vec::new();
    let mut pages = Rows::new();
    let mut calls = Rows::new();
    for (key, value) in pairs {
        if key.len() > 64 || value.len() > 512 || value.contains('\0') {
            return Err(invalid());
        }
        if let Some((index, field)) = row(&key, "page_")? {
            if !["display", "role", "target", "mask"].contains(&field) {
                return Err(invalid());
            }
            insert(&mut pages, index, field, value)?;
        } else if let Some((index, field)) = row(&key, "call_")? {
            if !["caller", "command", "target"].contains(&field) {
                return Err(invalid());
            }
            insert(&mut calls, index, field, value)?;
        } else {
            common.push((key, value));
        }
    }
    if pages.is_empty() && calls.is_empty() {
        return Err(invalid());
    }
    let common = fields(
        common,
        &[
            "channel_id",
            "design_id",
            "revision",
            "digest",
            "expected_design_head",
            "name",
        ],
    )?;
    Ok((common, (page_choices(pages)?, call_choices(calls)?)))
}
async fn perform(state: &AppState, pairs: Vec<(String, String)>) -> Result<Response> {
    let end = super::deadline()?;
    let (values, (selections, calls)) = input(pairs)?;
    let context = resource_controls::prepare(&state.application, &values, end).await?;
    let saved = match choose_teletext_editorial_object_pages(
        &state.application,
        NativeTeletextObjectPagesChooseRequest {
            name: context.name,
            receipt_id: context.exported.metadata().id,
            receipt_digest: context.exported.metadata().digest.clone(),
            expected_design_head: context.head,
            selections,
            calls,
        },
        context.policy,
        context.end,
    )
    .await
    .map_err(selection_error)?
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

fn selection_error(
    error: crate::host::teletext_editorial_designs::NativeTeletextEditorialImportError,
) -> Failure {
    use crate::host::{
        teletext_channel_documents::ChannelTeletextDocumentEncodingError,
        teletext_designs::NativeTeletextDesignError,
        teletext_editorial_designs::NativeTeletextEditorialImportError,
    };
    use ersatzrs_domain::TeletextObjectPageSelectionError as DomainError;
    if let NativeTeletextEditorialImportError::Design(design) = &error {
        if let NativeTeletextDesignError::Encoding(encoding) = design.as_ref() {
            if let ChannelTeletextDocumentEncodingError::ObjectPageSelection(selection) =
                encoding.as_ref()
            {
                let display = match selection.as_ref() {
                    DomainError::Duplicate { display, .. }
                    | DomainError::MissingDisplay { display }
                    | DomainError::OriginalMismatch { display, .. }
                    | DomainError::MissingResource { display, .. }
                    | DomainError::UnprovenRoles { display }
                    | DomainError::Unresolved { display, .. } => Some(*display),
                    _ => None,
                };
                if let Some(display) = display {
                    return Failure::new(INVALID).position(
                        display.page.get(),
                        display.subcode.get(),
                        None,
                        None,
                    );
                }
            }
        }
    }
    resource_controls::error(error, INVALID)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_domain::{
        TeletextObjectId, TeletextObjectIdentity, TeletextPageAddress, TeletextPageNumber,
        TeletextRawDataPageNumber, TeletextResourceIdentity, TeletextResourceKind,
        TeletextResourceSubpageIndex, TeletextSubcode,
    };
    fn pairs() -> Vec<(String, String)> {
        let display = TeletextPageAddress {
            page: TeletextPageNumber::new(124).unwrap(),
            subcode: TeletextSubcode::new(32).unwrap(),
        };
        let page = TeletextRawDataPageNumber::new(2, 0xe1).unwrap();
        let resource = TeletextResourceIdentity::new(
            TeletextResourceKind::Gpop,
            page,
            TeletextResourceSubpageIndex::new(0).unwrap(),
        )
        .unwrap();
        let object = TeletextObjectIdentity::global(resource, TeletextObjectId::new(32)).unwrap();
        [
            ("channel_id", "1".into()),
            ("design_id", "9".into()),
            ("revision", "4".into()),
            ("digest", "a".repeat(64)),
            ("expected_design_head", "4".into()),
            ("name", "Explicit selection".into()),
            ("page_0_display", serde_json::to_string(&display).unwrap()),
            ("page_0_role", "gpop".into()),
            ("page_0_target", serde_json::to_string(&page).unwrap()),
            ("page_0_mask", "8001".into()),
            ("call_0_caller", serde_json::to_string(&object).unwrap()),
            ("call_0_command", "4".into()),
            ("call_0_target", serde_json::to_string(&object).unwrap()),
        ]
        .map(|(key, value)| (key.to_owned(), value))
        .to_vec()
    }
    #[test]
    fn exact_page_and_call_choices_preserve_raw_identity_mask_and_ordinal() {
        let (common, (pages, calls)) =
            input(pairs()).unwrap_or_else(|_| panic!("valid explicit chooser form"));
        assert_eq!(common["name"], "Explicit selection");
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].display.page.get(), 124);
        assert_eq!(pages[0].target.page(), 0xe1);
        assert_eq!(pages[0].required_subpages.get(), 0x8001);
        assert_eq!(pages[0].role, TeletextX27LinkRole::Gpop);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].command_index, 4);
        assert_eq!(calls[0].caller, calls[0].target);
    }
    #[test]
    fn shared_page_datalist_values_preserve_literal_raw_address_and_reject_invalid_magazines() {
        assert_eq!(
            raw_page("2e1").unwrap_or_else(|_| panic!("raw target")),
            TeletextRawDataPageNumber::new(2, 0xe1).unwrap()
        );
        for bad in ["", "2E10", "9E1", "0E1", "2G1", "2 E1", "0x2E1"] {
            assert!(raw_page(bad).is_err(), "{bad}");
        }
        let mut values = pairs();
        values
            .iter_mut()
            .find(|(key, _)| key == "page_0_target")
            .unwrap()
            .1 = "2E1".into();
        let (_, (pages, _)) = input(values).unwrap_or_else(|_| panic!("native raw page form"));
        assert_eq!(pages[0].target.page(), 0xe1);
    }
    #[test]
    fn native_selection_refusal_keeps_display_and_subpage_without_untrusted_details() {
        use crate::host::{
            teletext_channel_documents::ChannelTeletextDocumentEncodingError,
            teletext_designs::NativeTeletextDesignError,
            teletext_editorial_designs::NativeTeletextEditorialImportError,
        };
        let display = TeletextPageAddress {
            page: TeletextPageNumber::new(124).unwrap(),
            subcode: TeletextSubcode::new(32).unwrap(),
        };
        let error = NativeTeletextEditorialImportError::Design(Box::new(
            NativeTeletextDesignError::Encoding(Box::new(
                ChannelTeletextDocumentEncodingError::ObjectPageSelection(Box::new(
                    ersatzrs_domain::TeletextObjectPageSelectionError::MissingResource {
                        display,
                        role: TeletextX27LinkRole::Gpop,
                    },
                )),
            )),
        ));
        let failure = selection_error(error);
        assert_eq!(failure.code, INVALID);
        let location: BTreeMap<String, String> =
            serde_urlencoded::from_str(failure.location.trim_start_matches('&')).unwrap();
        assert_eq!(location["error_page"], "124");
        assert_eq!(location["error_subcode"], "0020");
        assert_eq!(location.len(), 2);
    }
    #[test]
    fn shared_datalist_targets_preserve_the_complete_resource_identity() {
        let value = object_target("GPOP 2E1 S1=15 #32")
            .unwrap_or_else(|_| panic!("valid complete target label"));
        let resource = value.resource().unwrap();
        assert_eq!(resource.kind(), TeletextResourceKind::Gpop);
        assert_eq!(
            resource.page(),
            TeletextRawDataPageNumber::new(2, 0xe1).unwrap()
        );
        assert_eq!(resource.logical_subpage().get(), 15);
        assert_eq!(value.object_id().get(), 32);
        for invalid in [
            "LOCAL 2E1 S1=0 #32",
            "POP 9E1 S1=0 #32",
            "POP 2E1 S1=16 #32",
            "POP 2E1 0 #32",
            "POP 2E1 S1=0 32",
            "POP 2E1 S1=0 #65536",
            "POP 2E1 S1=0 #32 extra",
        ] {
            assert!(object_target(invalid).is_err(), "{invalid}");
        }
    }
    #[test]
    fn rows_refuse_unknown_missing_duplicate_sparse_and_forged_authority() {
        for (key, value) in [
            ("page_0_provenance", "{}"),
            ("page_0_mask", "0001"),
            ("page_01_role", "pop"),
            ("page_2_role", "pop"),
            ("page_x_role", "pop"),
            ("call_0_expected", "{}"),
            ("document_index", "0"),
            ("channel_id", "2"),
        ] {
            let mut values = pairs();
            values.push((key.into(), value.into()));
            assert!(input(values).is_err(), "{key}");
        }
        for missing in ["page_0_display", "page_0_mask", "call_0_command"] {
            let mut values = pairs();
            values.retain(|(key, _)| key != missing);
            assert!(input(values).is_err(), "{missing}");
        }
    }
    #[test]
    fn masks_roles_row_indices_and_input_limits_reject_before_native_work() {
        for (key, value) in [
            ("page_0_mask", "10000"),
            ("page_0_mask", "0x1"),
            ("page_0_mask", ""),
            ("page_0_role", "gdrcs"),
            ("call_0_command", "-1"),
            ("call_0_caller", "null"),
        ] {
            let mut values = pairs();
            values.iter_mut().find(|(k, _)| k == key).unwrap().1 = value.into();
            assert!(input(values).is_err(), "{key}={value}");
        }
        assert!(input(vec![]).is_err());
        assert!(input(vec![("name".into(), "x".repeat(513))]).is_err());
        assert!(input(vec![("name".into(), "x".into()); MAX_PARTS + 1]).is_err());
    }
}
