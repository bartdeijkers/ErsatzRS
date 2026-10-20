//! Explicit multi-display dependency intent; a fresh export resolves each source.
use super::{
    fields, resource_controls, respond, AppState, BTreeSet, Failure, Form, Response, Result, State,
    TeletextDesignMutation, MAX_PARTS,
};
use crate::host::teletext_editorial_designs::{
    announce_teletext_editorial_object_subpages, NativeTeletextEditorialImportError,
    NativeTeletextObjectSubpagesAnnounceRequest,
};
use crate::host::{
    teletext_channel_documents::ChannelTeletextDocumentEncodingError,
    teletext_designs::NativeTeletextDesignError,
};
use ersatzrs_app::teletext_design_documents::TeletextSavedObjectSubpageAnnouncement;
use ersatzrs_domain::{
    TeletextObjectSubpageAnnouncement, TeletextObjectSubpageAnnouncementError, TeletextPageAddress,
};

const INVALID: &str = "subpage-announcement-invalid";

pub async fn announce_object_subpages(
    State(state): State<AppState>,
    Form(pairs): Form<Vec<(String, String)>>,
) -> Response {
    let channel = resource_controls::channel(&pairs);
    match perform(&state, pairs).await {
        Ok(response) => response,
        Err(error) => respond(channel, &error),
    }
}

fn input(
    pairs: Vec<(String, String)>,
) -> Result<(super::BTreeMap<String, String>, Vec<TeletextPageAddress>)> {
    let mut common = Vec::new();
    let mut displays = Vec::new();
    let mut seen = BTreeSet::new();
    for (key, value) in pairs {
        if key != "display" {
            common.push((key, value));
            continue;
        }
        if displays.len() >= MAX_PARTS {
            return Err(Failure::new("editorial-limit"));
        }
        let display = resource_controls::display(&value, INVALID)?;
        if !seen.insert(display) {
            return Err(Failure::new(INVALID).position(
                display.page.get(),
                display.subcode.get(),
                None,
                None,
            ));
        }
        displays.push(display);
    }
    if displays.is_empty() {
        return Err(Failure::new(INVALID));
    }
    Ok((
        fields(
            common,
            &[
                "channel_id",
                "design_id",
                "revision",
                "digest",
                "expected_design_head",
                "name",
                "resource",
            ],
        )?,
        displays,
    ))
}

async fn perform(state: &AppState, pairs: Vec<(String, String)>) -> Result<Response> {
    let end = super::deadline()?;
    let (values, displays) = input(pairs)?;
    let resource = resource_controls::resource(
        values
            .get("resource")
            .ok_or_else(|| Failure::new(INVALID))?,
        INVALID,
    )?;
    let context = resource_controls::prepare(&state.application, &values, end).await?;
    let announcements = displays
        .into_iter()
        .map(|display| {
            let document_index = resource_controls::source_ordinal(
                &context.exported.sidecar().project,
                display,
                INVALID,
            )?;
            let announcement = TeletextObjectSubpageAnnouncement::new(display, resource)
                .map_err(|_| Failure::new(INVALID).resource_position(resource, None, None))?;
            TeletextSavedObjectSubpageAnnouncement::new(document_index, announcement)
                .map_err(|_| Failure::new(INVALID))
        })
        .collect::<Result<Vec<_>>>()?;
    let request = NativeTeletextObjectSubpagesAnnounceRequest {
        name: context.name,
        receipt_id: context.exported.metadata().id,
        receipt_digest: context.exported.metadata().digest.clone(),
        expected_design_head: context.head,
        announcements,
    };
    let saved = match announce_teletext_editorial_object_subpages(
        &state.application,
        request,
        context.policy,
        context.end,
    )
    .await
    .map_err(announcement_error)?
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

fn announcement_error(error: NativeTeletextEditorialImportError) -> Failure {
    if let NativeTeletextEditorialImportError::Design(design) = &error {
        if let NativeTeletextDesignError::Encoding(encoding) = design.as_ref() {
            if let ChannelTeletextDocumentEncodingError::ObjectAnnouncement(announcement) =
                encoding.as_ref()
            {
                if let Some(failure) = domain_failure(announcement) {
                    return failure;
                }
            }
        }
    }
    resource_controls::error(error, INVALID)
}

fn domain_failure(error: &TeletextObjectSubpageAnnouncementError) -> Option<Failure> {
    use TeletextObjectSubpageAnnouncementError as Error;
    let (display, resource) = match *error {
        Error::Duplicate { display, resource }
        | Error::Owner { display, resource }
        | Error::AlreadyAnnounced { display, resource } => (Some(display), Some(resource)),
        Error::MissingDisplay { display }
        | Error::UnprovenRoles { display }
        | Error::Level35Only { display } => (Some(display), None),
        Error::Kind { resource } | Error::MissingResource { resource } => (None, Some(resource)),
        _ => return None,
    };
    let mut failure = Failure::new(INVALID);
    if let Some(display) = display {
        failure = failure.position(display.page.get(), display.subcode.get(), None, None);
    }
    if let Some(resource) = resource {
        failure.location.push_str(
            &Failure::new(INVALID)
                .resource_position(resource, None, None)
                .location,
        );
    }
    Some(failure)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(key: &str, value: &str) -> (String, String) {
        (key.to_owned(), value.to_owned())
    }

    #[test]
    fn multiple_displays_are_explicit_distinct_and_do_not_widen_other_form_fields() {
        let page = r#"{"page":124,"subcode":32}"#;
        let other = r#"{"page":125,"subcode":32}"#;
        let (values, pages) = input(vec![
            pair("display", page),
            pair("display", other),
            pair("name", "Draft"),
        ])
        .map_err(|error| error.code)
        .unwrap();
        assert_eq!(values["name"], "Draft");
        assert_eq!(
            pages.iter().map(|page| page.page.get()).collect::<Vec<_>>(),
            [124, 125]
        );
        assert!(pages.iter().all(|page| page.subcode.get() == 32));
        for pairs in [
            vec![],
            vec![pair("display", page), pair("display", page)],
            vec![pair("display", "{}")],
            vec![pair("display", page), pair("document_index", "0")],
            vec![
                pair("display", page),
                pair("resource", "x"),
                pair("resource", "x"),
            ],
        ] {
            assert!(input(pairs).is_err());
        }
    }

    #[test]
    fn native_owner_refusal_keeps_display_subcode_and_resource_without_untrusted_details() {
        let resource = ersatzrs_domain::TeletextResourceIdentity::new(
            ersatzrs_domain::TeletextResourceKind::Pop,
            ersatzrs_domain::TeletextRawDataPageNumber::new(2, 0xc0).unwrap(),
            ersatzrs_domain::TeletextResourceSubpageIndex::new(14).unwrap(),
        )
        .unwrap();
        let display = resource_controls::display(r#"{"page":124,"subcode":32}"#, INVALID)
            .map_err(|error| error.code)
            .unwrap();
        let error = NativeTeletextEditorialImportError::Design(Box::new(
            NativeTeletextDesignError::Encoding(Box::new(
                ChannelTeletextDocumentEncodingError::ObjectAnnouncement(Box::new(
                    TeletextObjectSubpageAnnouncementError::Owner { display, resource },
                )),
            )),
        ));
        let failure = announcement_error(error);
        assert_eq!(failure.code, INVALID);
        let values: super::super::BTreeMap<String, String> =
            serde_urlencoded::from_str(failure.location.trim_start_matches('&')).unwrap();
        assert_eq!(values["error_page"], "124");
        assert_eq!(values["error_subcode"], "0020");
        assert_eq!(
            serde_json::from_str::<ersatzrs_domain::TeletextResourceIdentity>(
                &values["error_resource"]
            )
            .unwrap(),
            resource
        );
    }
}
