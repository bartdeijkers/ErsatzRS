//! Small explicit attachment/restoration forms reuse the current-receipt worker.
use super::{
    fields, invalid, json, page_choices, resource_controls, respond, selection_error, take,
    AppState, BTreeMap, Form, Response, Result, State, TeletextDesignMutation,
    TeletextObjectPageChoice, TeletextX27LinkRole,
};
use crate::host::teletext_editorial_designs::{
    attach_teletext_editorial_object_pages, restore_teletext_editorial_page_attachments,
    NativeTeletextObjectPagesChooseRequest, NativeTeletextPageAttachmentsRestoreRequest,
};
use ersatzrs_domain::TeletextPageAddress;

enum Intent {
    Attach(TeletextObjectPageChoice),
    Restore(TeletextPageAddress, TeletextX27LinkRole),
}
fn input(pairs: Vec<(String, String)>, attach: bool) -> Result<(BTreeMap<String, String>, Intent)> {
    let allowed = if attach {
        &[
            "channel_id",
            "design_id",
            "revision",
            "digest",
            "expected_design_head",
            "name",
            "display",
            "role",
            "target",
            "mask",
        ][..]
    } else {
        &[
            "channel_id",
            "design_id",
            "revision",
            "digest",
            "expected_design_head",
            "name",
            "display",
            "role",
        ][..]
    };
    let mut values = fields(pairs, allowed)?;
    let display = take(&mut values, "display")?;
    let role = take(&mut values, "role")?;
    if attach {
        let row = BTreeMap::from([
            ("display".to_owned(), display),
            ("role".to_owned(), role),
            ("target".to_owned(), take(&mut values, "target")?),
            ("mask".to_owned(), take(&mut values, "mask")?),
        ]);
        let choice = page_choices(BTreeMap::from([(0, row)]))?
            .into_iter()
            .next()
            .ok_or_else(invalid)?;
        Ok((values, Intent::Attach(choice)))
    } else {
        let display = json(&display)?;
        let role = match role.as_str() {
            "pop" => TeletextX27LinkRole::Pop,
            "gpop" => TeletextX27LinkRole::Gpop,
            "drcs" => TeletextX27LinkRole::Drcs,
            "gdrcs" => TeletextX27LinkRole::Gdrcs,
            _ => return Err(invalid()),
        };
        Ok((values, Intent::Restore(display, role)))
    }
}

async fn perform(state: &AppState, pairs: Vec<(String, String)>, attach: bool) -> Result<Response> {
    let end = super::super::deadline()?;
    let (values, intent) = input(pairs, attach)?;
    let context = resource_controls::prepare(&state.application, &values, end).await?;
    let result = match intent {
        Intent::Attach(selection) => {
            attach_teletext_editorial_object_pages(
                &state.application,
                NativeTeletextObjectPagesChooseRequest {
                    name: context.name,
                    receipt_id: context.exported.metadata().id,
                    receipt_digest: context.exported.metadata().digest.clone(),
                    expected_design_head: context.head,
                    selections: vec![selection],
                    calls: vec![],
                },
                context.policy,
                context.end,
            )
            .await
        }
        Intent::Restore(display, role) => {
            restore_teletext_editorial_page_attachments(
                &state.application,
                NativeTeletextPageAttachmentsRestoreRequest {
                    name: context.name,
                    receipt_id: context.exported.metadata().id,
                    receipt_digest: context.exported.metadata().digest.clone(),
                    expected_design_head: context.head,
                    roles: vec![(display, role)],
                },
                context.policy,
                context.end,
            )
            .await
        }
    };
    let saved = match result.map_err(selection_error)? {
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

pub async fn attach_object_page(
    State(state): State<AppState>,
    Form(pairs): Form<Vec<(String, String)>>,
) -> Response {
    let channel = resource_controls::channel(&pairs);
    match perform(&state, pairs, true).await {
        Ok(response) => response,
        Err(error) => respond(channel, &error),
    }
}
pub async fn restore_page_attachment(
    State(state): State<AppState>,
    Form(pairs): Form<Vec<(String, String)>>,
) -> Response {
    let channel = resource_controls::channel(&pairs);
    match perform(&state, pairs, false).await {
        Ok(response) => response,
        Err(error) => respond(channel, &error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pairs(attach: bool) -> Vec<(String, String)> {
        let mut values = [
            ("channel_id", "1".to_owned()),
            ("design_id", "9".to_owned()),
            ("revision", "4".to_owned()),
            ("digest", "a".repeat(64)),
            ("expected_design_head", "4".to_owned()),
            ("name", "Attachment".to_owned()),
        ]
        .map(|(key, value)| (key.to_owned(), value))
        .to_vec();
        values.extend([
            ("display".into(), r#"{"page":124,"subcode":32}"#.into()),
            ("role".into(), "gdrcs".into()),
        ]);
        if attach {
            values.extend([
                ("target".into(), "2E1".into()),
                ("mask".into(), "8001".into()),
            ]);
        }
        values
    }
    #[test]
    fn attachment_and_restore_forms_keep_distinct_minimal_authority() {
        let (_, Intent::Attach(choice)) =
            input(pairs(true), true).unwrap_or_else(|_| panic!("attach"))
        else {
            panic!("attach intent")
        };
        assert_eq!(choice.role, TeletextX27LinkRole::Gdrcs);
        assert_eq!(choice.required_subpages.get(), 0x8001);
        let (_, Intent::Restore(display, role)) =
            input(pairs(false), false).unwrap_or_else(|_| panic!("restore"))
        else {
            panic!("restore intent")
        };
        assert_eq!(display.page.get(), 124);
        assert_eq!(role, TeletextX27LinkRole::Gdrcs);
        assert!(input(pairs(true), false).is_err());
        assert!(input(pairs(false), true).is_err());
        for attach in [false, true] {
            for field in [
                "display",
                "role",
                "expected_role",
                "original_links",
                "document_index",
                "receipt_id",
            ] {
                let mut values = pairs(attach);
                values.push((field.into(), "{}".into()));
                assert!(input(values, attach).is_err(), "{attach}: {field}");
            }
            let mut values = pairs(attach);
            values.retain(|(key, _)| key != "display");
            assert!(input(values, attach).is_err());
        }
    }
}
