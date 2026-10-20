//! Explicit raw-page and existing resource-call choices save only a concept.
use ersatzrs_domain::{TeletextObjectIdentity, TeletextObjectType, TeletextResourceKind};
use leptos::prelude::*;

use super::{
    btn_class, editorial_export_fields, hidden, style, tr, ButtonVariant, TeletextDraftPreviewData,
    TeletextDraftPublicationObservation, TextKey,
};

fn kind_label(kind: TeletextResourceKind) -> &'static str {
    match kind {
        TeletextResourceKind::Pop => "POP",
        TeletextResourceKind::Gpop => "GPOP",
        _ => "—",
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
    [TeletextResourceKind::Pop, TeletextResourceKind::Gpop]
        .into_iter()
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

pub(super) fn form(
    data: &TeletextDraftPreviewData,
    observation: &TeletextDraftPublicationObservation,
) -> AnyView {
    if data.object_choices.pages.is_empty() && data.object_choices.calls.is_empty() {
        return ().into_any();
    }
    let current = data.revision == observation.current_design_head;
    let page_lists = page_catalogue(data);
    let pages = data.object_choices.pages.iter().enumerate().map(|(index, page)| {
        let prefix = format!("teletext-object-page-{index}");
        let target_id = format!("{prefix}-target");
        let mask_id = format!("{prefix}-mask");
        let role = if page.role == TeletextResourceKind::Pop { "pop" } else { "gpop" };
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
    let calls = data.object_choices.calls.iter().enumerate().map(|(index,call)| {
        let target_id = format!("teletext-object-call-{index}-target");
        let caller = serde_json::to_string(&call.caller).unwrap_or_default();
        let offset = call.offset.map_or_else(|| tr(TextKey::TeletextObjectSelectionNoOffset).to_owned(),|offset| format!("{}, {}",offset.row(),offset.column()));
        view! {
            <fieldset style:min-width="0">
                <legend>{tr(TextKey::TeletextObjectSelectionCalls)}" "{index+1}</legend>
                {hidden(&format!("call_{index}_caller"),caller)}
                {hidden(&format!("call_{index}_command"),call.command_index.to_string())}
                <dl class=style::location>
                    <dt>{tr(TextKey::TeletextObjectSelectionCaller)}</dt><dd>{object_label(call.caller)}</dd>
                    <dt>{tr(TextKey::TeletextObjectSelectionCommand)}</dt><dd>{call.command_index}</dd>
                    <dt>{tr(TextKey::TeletextObjectSelectionCurrentTarget)}</dt><dd>{object_label(call.target)}</dd>
                    <dt>{tr(TextKey::TeletextObjectAddType)}</dt><dd>{object_type_label(call.object_type)}</dd>
                    <dt>{tr(TextKey::TeletextObjectSelectionOffset)}</dt><dd>{offset}</dd>
                </dl>
                <label for=target_id.clone()>{tr(TextKey::TeletextObjectSelectionCallTarget)}</label>
                <input id=target_id name=format!("call_{index}_target") required
                    value=object_label(call.target) list=target_list_id(call.object_type)
                    maxlength="64"/>
            </fieldset>
        }
    }).collect_view();
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
                <button type="submit" class=btn_class(ButtonVariant::Primary)>{tr(TextKey::TeletextObjectSelectionAction)}</button>
            </fieldset>
            <p style:display=if current { "none" } else { "" }>{tr(TextKey::TeletextResourceEditCurrentHelp)}</p>
        </form>
    }.into_any()
}
