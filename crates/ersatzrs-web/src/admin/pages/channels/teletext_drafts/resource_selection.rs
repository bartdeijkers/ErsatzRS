//! Explicit raw-page and existing resource-call choices save only a concept.
use ersatzrs_domain::{
    TeletextObjectIdentity, TeletextObjectOffset, TeletextObjectType, TeletextResourceKind,
};
use leptos::prelude::*;

use super::{
    btn_class, editorial_export_fields, hidden, style, tr, ButtonVariant, TeletextDraftPreviewData,
    TeletextDraftPublicationObservation, TextKey,
};

fn kind_label(kind: TeletextResourceKind) -> &'static str {
    match kind {
        TeletextResourceKind::Pop => "POP",
        TeletextResourceKind::Gpop => "GPOP",
        TeletextResourceKind::Drcs => "DRCS",
        TeletextResourceKind::Gdrcs => "GDRCS",
        _ => "—",
    }
}
fn role_name(kind: TeletextResourceKind) -> &'static str {
    match kind {
        TeletextResourceKind::Pop => "pop",
        TeletextResourceKind::Gpop => "gpop",
        TeletextResourceKind::Drcs => "drcs",
        TeletextResourceKind::Gdrcs => "gdrcs",
        _ => "",
    }
}
fn object_label(identity: TeletextObjectIdentity) -> String {
    if let Some(resource) = identity.resource() {
        format!(
            "{} {:X}{:02X} S1={} #{}",
            kind_label(resource.kind()),
            resource.page().magazine(),
            resource.page().page(),
            resource.logical_subpage().get(),
            identity.object_id().get(),
        )
    } else if let Some(owner) = identity.owner() {
        format!(
            "LOCAL {} / {:04X} #{}",
            owner.page.get(),
            owner.subcode.get(),
            identity.object_id().get()
        )
    } else {
        identity.object_id().get().to_string()
    }
}
fn object_type_label(kind: TeletextObjectType) -> &'static str {
    tr(match kind {
        TeletextObjectType::Active => TextKey::TeletextObjectTypeActive,
        TeletextObjectType::Adaptive => TextKey::TeletextObjectTypeAdaptive,
        TeletextObjectType::Passive => TextKey::TeletextObjectTypePassive,
    })
}

fn raw_page_label(page: ersatzrs_domain::TeletextRawDataPageNumber) -> String {
    format!("{:X}{:02X}", page.magazine(), page.page())
}
fn page_list_id(kind: TeletextResourceKind) -> &'static str {
    match kind {
        TeletextResourceKind::Pop => "teletext-object-pages-pop",
        TeletextResourceKind::Gpop => "teletext-object-pages-gpop",
        TeletextResourceKind::Drcs => "teletext-object-pages-drcs",
        TeletextResourceKind::Gdrcs => "teletext-object-pages-gdrcs",
        _ => "",
    }
}

fn target_list_id(kind: TeletextObjectType) -> &'static str {
    match kind {
        TeletextObjectType::Active => "teletext-object-targets-active",
        TeletextObjectType::Adaptive => "teletext-object-targets-adaptive",
        TeletextObjectType::Passive => "teletext-object-targets-passive",
    }
}

fn page_catalogue(data: &TeletextDraftPreviewData) -> AnyView {
    ersatzrs_domain::TeletextX27LinkRole::ALL
        .into_iter()
        .map(ersatzrs_domain::TeletextX27LinkRole::kind)
        .filter(|kind| {
            data.object_choices
                .pages
                .iter()
                .any(|page| page.role == *kind)
        })
        .map(|kind| {
            let mut seen = std::collections::BTreeSet::new();
            let options = data
                .object_resources
                .iter()
                .filter(|resource| resource.kind() == kind)
                .filter(|resource| {
                    seen.insert((resource.page().magazine(), resource.page().page()))
                })
                .map(|resource| view! { <option value=raw_page_label(resource.page())></option> })
                .collect_view();
            view! { <datalist id=page_list_id(kind)>{options}</datalist> }
        })
        .collect_view()
        .into_any()
}

fn target_catalogue(data: &TeletextDraftPreviewData) -> AnyView {
    [
        TeletextObjectType::Active,
        TeletextObjectType::Adaptive,
        TeletextObjectType::Passive,
    ]
    .into_iter()
    .map(|kind| {
        let options = data
            .object_choices
            .definitions
            .iter()
            .filter(|definition| {
                definition.object_type == kind && definition.identity.resource().is_some()
            })
            .map(|definition| view! { <option value=object_label(definition.identity)></option> })
            .collect_view();
        view! { <datalist id=target_list_id(kind)>{options}</datalist> }
    })
    .collect_view()
    .into_any()
}

fn call_metadata(
    command_index: usize,
    target: TeletextObjectIdentity,
    object_type: TeletextObjectType,
    offset: Option<TeletextObjectOffset>,
) -> AnyView {
    let offset = offset.map_or_else(
        || tr(TextKey::TeletextObjectSelectionNoOffset).to_owned(),
        |offset| format!("{}, {}", offset.row(), offset.column()),
    );
    view! {
        <dt>{tr(TextKey::TeletextObjectSelectionCommand)}</dt><dd>{command_index}</dd>
        <dt>{tr(TextKey::TeletextObjectSelectionCurrentTarget)}</dt><dd>{object_label(target)}</dd>
        <dt>{tr(TextKey::TeletextObjectAddType)}</dt><dd>{object_type_label(object_type)}</dd>
        <dt>{tr(TextKey::TeletextObjectSelectionOffset)}</dt><dd>{offset}</dd>
    }
    .into_any()
}
fn call_target(
    prefix: &str,
    index: usize,
    target: TeletextObjectIdentity,
    object_type: TeletextObjectType,
) -> AnyView {
    let id = format!("teletext-object-{prefix}-{index}-target");
    view! {
        <label for=id.clone()>{tr(TextKey::TeletextObjectSelectionCallTarget)}</label>
        <input id=id name=format!("{prefix}_{index}_target") required
            value=object_label(target) list=target_list_id(object_type) maxlength="64"/>
    }
    .into_any()
}
fn object_calls(data: &TeletextDraftPreviewData) -> AnyView {
    data.object_choices.calls.iter().enumerate().map(|(index,call)| {
        let caller = serde_json::to_string(&call.caller).unwrap_or_default();
        view! {
            <fieldset style:min-width="0">
                <legend>{tr(TextKey::TeletextObjectSelectionCalls)}" "{index+1}</legend>
                {hidden(&format!("call_{index}_caller"),caller)}
                {hidden(&format!("call_{index}_command"),call.command_index.to_string())}
                <dl class=style::location>
                    <dt>{tr(TextKey::TeletextObjectSelectionCaller)}</dt><dd>{object_label(call.caller)}</dd>
                    {call_metadata(call.command_index,call.target,call.object_type,call.offset)}
                </dl>
                {call_target("call",index,call.target,call.object_type)}
            </fieldset>
        }
    }).collect_view().into_any()
}
fn main_calls(data: &TeletextDraftPreviewData) -> AnyView {
    data.object_choices
        .main_calls
        .iter()
        .enumerate()
        .map(|(index, call)| {
            let owner = serde_json::to_string(&call.owner).unwrap_or_default();
            let label = format!(
                "{} / {:04X}",
                call.owner.page.get(),
                call.owner.subcode.get()
            );
            view! {
                <fieldset style:min-width="0">
                    <legend>{tr(TextKey::TeletextObjectSelectionMainCalls)}" "{index+1}</legend>
                    {hidden(&format!("main_{index}_owner"),owner)}
                    {hidden(&format!("main_{index}_command"),call.command_index.to_string())}
                    <dl class=style::location>
                        <dt>{tr(TextKey::TeletextResourceDisplay)}</dt><dd>{label}</dd>
                        {call_metadata(call.command_index,call.target,call.object_type,call.offset)}
                    </dl>
                    {call_target("main",index,call.target,call.object_type)}
                </fieldset>
            }
        })
        .collect_view()
        .into_any()
}

pub(super) fn form(
    data: &TeletextDraftPreviewData,
    observation: &TeletextDraftPublicationObservation,
) -> AnyView {
    (
        selection_form(data, observation),
        attachment_forms(data, observation),
        restoration_forms(data, observation),
    )
        .into_any()
}

fn selection_form(
    data: &TeletextDraftPreviewData,
    observation: &TeletextDraftPublicationObservation,
) -> AnyView {
    if data.object_choices.pages.is_empty()
        && data.object_choices.calls.is_empty()
        && data.object_choices.main_calls.is_empty()
    {
        return ().into_any();
    }
    let current = data.revision == observation.current_design_head;
    let page_lists = page_catalogue(data);
    let pages = data.object_choices.pages.iter().enumerate().map(|(index, page)| {
        let prefix = format!("teletext-object-page-{index}");
        let target_id = format!("{prefix}-target");
        let mask_id = format!("{prefix}-mask");
        let role = role_name(page.role);
        let display = serde_json::to_string(&page.display).unwrap_or_default();
        let legend = format!("{} {} / {:04X} · {}",tr(TextKey::TeletextResourceDisplay),page.display.page.get(),page.display.subcode.get(),kind_label(page.role));
        view! {
            <fieldset style:min-width="0">
                <legend>{legend}</legend>
                {hidden(&format!("page_{index}_display"),display)}
                {hidden(&format!("page_{index}_role"),role.to_owned())}
                <label for=target_id.clone()>{tr(TextKey::TeletextObjectSelectionPageTarget)}</label>
                <input id=target_id name=format!("page_{index}_target") required maxlength="3"
                    pattern="[1-8][0-9A-Fa-f]{2}" value=raw_page_label(page.target)
                    list=page_list_id(page.role)/>
                <label for=mask_id.clone()>{tr(TextKey::TeletextObjectSelectionMask)}</label>
                <input id=mask_id name=format!("page_{index}_mask") required pattern="[0-9A-Fa-f]{1,4}"
                    minlength="1" maxlength="4" value=format!("{:04X}",page.required_subpages.get())
                    aria-describedby="teletext-object-pages-mask-help"/>
            </fieldset>
        }
    }).collect_view();
    let target_lists = target_catalogue(data);
    let calls = object_calls(data);
    let main_calls = main_calls(data);
    view! {
        <form method="post" action="/admin-api/teletext/design/choose-object-pages"
            class=style::preview_form data-testid="teletext-object-pages-choose-form"
            aria-describedby="teletext-object-pages-help">
            {editorial_export_fields(data,observation)}
            <fieldset disabled=!current style:min-width="0">
                <legend>{tr(TextKey::TeletextObjectSelectionTitle)}</legend>
                <p id="teletext-object-pages-help">{tr(TextKey::TeletextObjectSelectionHelp)}</p>
                <label for="teletext-object-pages-name">{tr(TextKey::TeletextDesignName)}</label>
                <input id="teletext-object-pages-name" name="name" required maxlength=ersatzrs_domain::MAX_TELETEXT_DESIGN_NAME_BYTES/>
                <p id="teletext-object-pages-mask-help">{tr(TextKey::TeletextObjectSelectionMaskHelp)}</p>
                {page_lists}
                {pages}
                {target_lists}
                {calls}
                {main_calls}
                <button type="submit" class=btn_class(ButtonVariant::Primary)>{tr(TextKey::TeletextObjectSelectionAction)}</button>
            </fieldset>
            <p style:display=if current { "none" } else { "" }>{tr(TextKey::TeletextResourceEditCurrentHelp)}</p>
        </form>
    }.into_any()
}

fn attachment_forms(
    data: &TeletextDraftPreviewData,
    observation: &TeletextDraftPublicationObservation,
) -> AnyView {
    let current = data.revision == observation.current_design_head;
    data.object_choices.unselected_roles.iter().enumerate().map(|(index, role)| {
        let prefix = format!("teletext-object-page-attachment-{index}");
        let list_id = format!("{prefix}-pages");
        let help_id = format!("{prefix}-help");
        let mask_help_id = format!("{prefix}-mask-help");
        let display = serde_json::to_string(&role.display).unwrap_or_default();
        let first = data.object_resources.iter().find(|resource| resource.kind() == role.role);
        let target = first.map_or_else(String::new, |resource| raw_page_label(resource.page()));
        let mask = first.map_or(0, |first| data.object_resources.iter()
            .filter(|resource| resource.kind() == role.role && resource.page() == first.page())
            .fold(0u16, |mask, resource| mask | (1u16 << resource.logical_subpage().get())));
        let mut seen = std::collections::BTreeSet::new();
        let options = data.object_resources.iter().filter(|resource| resource.kind() == role.role)
            .filter(|resource| seen.insert((resource.page().magazine(), resource.page().page())))
            .map(|resource| view! { <option value=raw_page_label(resource.page())></option> }).collect_view();
        let legend = format!("{} {} / {:04X} · {}", tr(TextKey::TeletextResourceDisplay),
            role.display.page.get(), role.display.subcode.get(), kind_label(role.role));
        view! {
            <form method="post" action="/admin-api/teletext/design/attach-object-page"
                class=style::preview_form data-testid="teletext-object-page-attach-form" aria-describedby=help_id.clone()>
                {editorial_export_fields(data,observation)}
                {hidden("display",display)}
                {hidden("role",role_name(role.role).to_owned())}
                <fieldset disabled=!current style:min-width="0">
                    <legend>{tr(TextKey::TeletextObjectAttachmentTitle)}" · "{legend}</legend>
                    <p id=help_id.clone()>{tr(TextKey::TeletextObjectAttachmentHelp)}</p>
                    <label for=format!("{prefix}-name")>{tr(TextKey::TeletextDesignName)}</label>
                    <input id=format!("{prefix}-name") name="name" required maxlength=ersatzrs_domain::MAX_TELETEXT_DESIGN_NAME_BYTES/>
                    <datalist id=list_id.clone()>{options}</datalist>
                    <label for=format!("{prefix}-target")>{tr(TextKey::TeletextObjectSelectionPageTarget)}</label>
                    <input id=format!("{prefix}-target") name="target" required maxlength="3"
                        pattern="[1-8][0-9A-Fa-f]{2}" value=target list=list_id/>
                    <label for=format!("{prefix}-mask")>{tr(TextKey::TeletextObjectSelectionMask)}</label>
                    <input id=format!("{prefix}-mask") name="mask" required pattern="[0-9A-Fa-f]{1,4}"
                        minlength="1" maxlength="4" value=format!("{mask:04X}") aria-describedby=mask_help_id.clone()/>
                    <p id=mask_help_id>{tr(TextKey::TeletextObjectSelectionMaskHelp)}</p>
                    <button type="submit" class=btn_class(ButtonVariant::Primary)>{tr(TextKey::TeletextObjectAttachmentAction)}</button>
                </fieldset>
                <p style:display=if current { "none" } else { "" }>{tr(TextKey::TeletextResourceEditCurrentHelp)}</p>
            </form>
        }
    }).collect_view().into_any()
}

fn restoration_forms(
    data: &TeletextDraftPreviewData,
    observation: &TeletextDraftPublicationObservation,
) -> AnyView {
    let current = data.revision == observation.current_design_head;
    data.object_choices.restorable_attachments.iter().enumerate().map(|(index, role)| {
        let prefix = format!("teletext-object-page-restore-{index}");
        let help_id = format!("{prefix}-help");
        let display = serde_json::to_string(&role.display).unwrap_or_default();
        let legend = format!("{} {} / {:04X} · {}",tr(TextKey::TeletextResourceDisplay),
            role.display.page.get(),role.display.subcode.get(),kind_label(role.role));
        view! {
            <form method="post" action="/admin-api/teletext/design/restore-page-attachment"
                class=style::preview_form data-testid="teletext-object-page-restore-form" aria-describedby=help_id.clone()>
                {editorial_export_fields(data,observation)}
                {hidden("display",display)}
                {hidden("role",role_name(role.role).to_owned())}
                <fieldset disabled=!current style:min-width="0">
                    <legend>{tr(TextKey::TeletextObjectAttachmentRestoreTitle)}" · "{legend}</legend>
                    <p id=help_id.clone()>{tr(TextKey::TeletextObjectAttachmentRestoreHelp)}</p>
                    <label for=format!("{prefix}-name")>{tr(TextKey::TeletextDesignName)}</label>
                    <input id=format!("{prefix}-name") name="name" required maxlength=ersatzrs_domain::MAX_TELETEXT_DESIGN_NAME_BYTES/>
                    <button type="submit" class=btn_class(ButtonVariant::Primary)>{tr(TextKey::TeletextObjectAttachmentRestoreAction)}</button>
                </fieldset>
                <p style:display=if current { "none" } else { "" }>{tr(TextKey::TeletextResourceEditCurrentHelp)}</p>
            </form>
        }
    }).collect_view().into_any()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::server_fns::teletext_drafts::{
        TeletextDraftPublicationState, TeletextObjectChoicesObservation,
    };

    fn preview() -> TeletextDraftPreviewData {
        serde_json::from_value(serde_json::json!({
            "channel_id": 1, "design_id": 2, "revision": 3,
            "owner": {"page": 201, "subcode": 32}, "image_url": null,
            "effective": false, "valid_until": null, "pages": [],
            "resource_objects": [],
            "object_resources": [
                {"kind": "drcs", "page": {"magazine": 1, "page": 192}, "logical_subpage": 0},
                {"kind": "drcs", "page": {"magazine": 1, "page": 192}, "logical_subpage": 3},
                {"kind": "gdrcs", "page": {"magazine": 1, "page": 193}, "logical_subpage": 0}
            ],
            "object_choices": {
                "pages": [], "calls": [], "main_calls": [], "definitions": [],
                "unselected_roles": [{"display": {"page": 201, "subcode": 32}, "role": "drcs"}],
                "restorable_attachments": [{"display": {"page": 200, "subcode": 32}, "role": "gpop"}]
            },
            "navigation": [], "diagnostics": [], "authoring": [], "publication": null,
            "outputs": null
        })).unwrap()
    }

    fn observation() -> TeletextDraftPublicationObservation {
        TeletextDraftPublicationObservation {
            digest: "a".repeat(64),
            current_design_head: 3,
            current_pending_id: None,
            current_accepted_id: None,
            state: TeletextDraftPublicationState::Unrequested,
            can_request: true,
            can_publish: false,
            replaces_pending: false,
        }
    }

    #[test]
    fn attachment_forms_are_localized_named_native_concepts_with_exact_fences_and_history() {
        for language in [
            "en-us", "nl", "de-de", "fr-fr", "es-es", "es-419", "it-it", "pt-br", "pl",
        ] {
            Owner::new().with(|| {
                provide_context(crate::admin::i18n::UiLanguage::from_config(language));
                let mut data = preview();
                let mut observed = observation();
                let html = form(&data, &observed).to_html();
                for key in [
                    TextKey::TeletextObjectAttachmentTitle,
                    TextKey::TeletextObjectAttachmentHelp,
                    TextKey::TeletextObjectAttachmentAction,
                    TextKey::TeletextObjectAttachmentRestoreTitle,
                    TextKey::TeletextObjectAttachmentRestoreHelp,
                    TextKey::TeletextObjectAttachmentRestoreAction,
                    TextKey::TeletextObjectSelectionMaskHelp,
                ] {
                    assert!(html.contains(tr(key)), "{language}: {key:?}");
                }
                for action in ["attach-object-page", "restore-page-attachment"] {
                    assert!(html.contains(&format!(
                        "method=\"post\" action=\"/admin-api/teletext/design/{action}\""
                    )));
                }
                for field in [
                    "channel_id",
                    "design_id",
                    "revision",
                    "digest",
                    "expected_design_head",
                    "name",
                    "display",
                    "role",
                ] {
                    assert_eq!(
                        html.matches(&format!("name=\"{field}\"")).count(),
                        2,
                        "{language}: {field}"
                    );
                }
                assert!(html.contains("value=\"1C0\""));
                assert!(html.contains("value=\"0009\""));
                assert!(
                    !html.contains("value=\"1C1\""),
                    "another kind is never an attachment target"
                );
                assert_eq!(
                    html.matches("<option ").count(),
                    1,
                    "deduplicate retained S1 owners"
                );
                assert!(!html.contains("teletext-object-pages-choose-form"));
                assert!(!html.contains("<fieldset disabled"));
                observed.current_design_head += 1;
                let historical = form(&data, &observed).to_html();
                assert_eq!(historical.matches("<fieldset disabled").count(), 2);
                assert!(historical.contains(tr(TextKey::TeletextResourceEditCurrentHelp)));
                data.object_choices.unselected_roles.clear();
                data.object_choices.restorable_attachments.clear();
                assert!(!form(&data, &observed).to_html().contains("<form"));
            });
        }
    }

    #[test]
    fn historical_choice_observations_default_new_attachment_lists_without_inventing_targets() {
        let observed: TeletextObjectChoicesObservation =
            serde_json::from_value(serde_json::json!({
                "pages": [], "calls": [], "main_calls": [], "definitions": []
            }))
            .unwrap();
        assert!(observed.unselected_roles.is_empty());
        assert!(observed.restorable_attachments.is_empty());
        assert!(observed.pages.is_empty());
    }
}
