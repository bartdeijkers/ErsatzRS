//! Explicit unpublished authoring input and exact immutable historical preview.

use ersatzrs_domain::{TeletextNavigationLink, TeletextNavigationRole, TeletextPageAddress};
use leptos::prelude::*;

use crate::admin::i18n::{tr, TextKey};
use crate::admin::server_fns::teletext::ChannelTeletextData;
use crate::admin::server_fns::teletext_drafts::{
    TeletextDraftDiagnosticData, TeletextDraftErrorLocation, TeletextDraftPreviewData,
    TeletextDraftPreviewResult, TeletextDraftPublicationObservation, TeletextDraftPublicationState,
};
use crate::admin::ui::primitives::button::{btn_class, ButtonVariant};
use crate::admin::ui::primitives::resource_state::admin_result_view;

mod editorial;
mod output_status;
mod resource_add;
mod resource_announcements;
mod resource_banks;
mod resource_remove;
mod resource_selection;

stylance::import_crate_style!(style, "src/admin/pages/channels/teletext_drafts.module.css");

pub(super) fn draft_status_text(code: &str) -> &'static str {
    tr(match code {
        "publication-requested" => TextKey::TeletextPublicationRequestSaved,
        "publication-selected" => TextKey::TeletextPublicationSelectedSaved,
        "publication-conflict" => TextKey::TeletextPublicationConflict,
        "draft" => TextKey::TeletextDraftSaved,
        "editorial-limit" => TextKey::TeletextEditorLimit,
        "editorial-mismatch" => TextKey::TeletextEditorMismatch,
        "object-copy-invalid" => TextKey::TeletextObjectCopyInvalid,
        "object-remove-invalid" => TextKey::TeletextObjectRemoveInvalid,
        "object-add-invalid" => TextKey::TeletextObjectAddInvalid,
        "resource-bank-invalid" => TextKey::TeletextResourceBankInvalid,
        "subpage-announcement-invalid" => TextKey::TeletextSubpageAnnouncementInvalid,
        "object-selection-invalid" => TextKey::TeletextObjectSelectionInvalid,
        "editorial-protected" => TextKey::TeletextEditorProtected,
        "editorial-no-changes" => TextKey::TeletextEditorNoChanges,
        "editorial-no-pages" => TextKey::TeletextPreviewAbsent,
        "editorial-syntax" => TextKey::TeletextEditorSyntax,
        "editorial-missing-receipt" => TextKey::TeletextEditorMissingReceipt,
        "busy" => TextKey::TeletextWorkBusy,
        "expired" => TextKey::TeletextWorkExpired,
        "missing-design" | "foreign-channel" => TextKey::TeletextDraftMissing,
        "missing-policy" | "invalid-policy" | "off" | "unavailable" => {
            TextKey::TeletextAuthoringUnavailable
        }
        _ => TextKey::TeletextAuthoringRejected,
    })
}

pub(super) fn authoring_controls(data: &ChannelTeletextData) -> AnyView {
    let eligible = data.addons.iter().filter(|addon| {
        addon.installed
            && addon.availability == "configured"
            && addon
                .binding
                .as_ref()
                .is_some_and(|binding| binding.enabled && binding.availability == "configured")
    });
    let available = data.catalog_available
        && data.authoring_selection.is_some()
        && eligible.clone().next().is_some();
    let options = eligible
        .map(|addon| {
            let name = if addon.name_labels.is_empty() {
                addon.name.clone()
            } else {
                super::teletext::manifest_label(&addon.name_labels)
            };
            view! { <option value=addon.addon_id.clone()>{name}</option> }
        })
        .collect_view();
    view! {
        <section class=style::authoring data-testid="teletext-authoring">
            <h2>{tr(TextKey::TeletextAuthoringTitle)}</h2>
            <p>{tr(TextKey::TeletextAuthoringHelp)}</p>
            <p>{tr(TextKey::TeletextAuthoringStaticHelp)}</p>
            <p role="status" hidden=available>{draft_status_text(data.authoring_error.as_deref().unwrap_or("unavailable"))}</p>
            <form method="post" action="/admin-api/teletext/design/prepare"
                data-testid="teletext-authoring-form">
                {hidden("channel_id", data.channel_id.to_string())}
                {hidden("selection_digest", data.authoring_selection.clone().unwrap_or_default())}
                <fieldset disabled=!available class=style::fields>
                    <legend>{tr(TextKey::TeletextAuthoringTitle)}</legend>
                    <label for="teletext-draft-name">{tr(TextKey::TeletextDesignName)}</label>
                    <input id="teletext-draft-name" name="name" required
                        maxlength=ersatzrs_domain::MAX_TELETEXT_DESIGN_NAME_BYTES/>
                    <label for="teletext-entry-producer">{tr(TextKey::TeletextEntryProducer)}</label>
                    <select id="teletext-entry-producer" name="entry_addon_id" required>
                        <option value="" selected disabled>{tr(TextKey::TeletextEntryProducer)}</option>
                        {options}
                    </select>
                    {entry_fields("teletext")}
                    <label for="teletext-v1-region">{tr(TextKey::TeletextV1Region)}</label>
                    <select id="teletext-v1-region" name="v1_region" required>
                        <option value="" selected disabled>{tr(TextKey::TeletextV1Region)}</option>
                        <option value="western_latin">{tr(TextKey::TeletextV1WesternLatin)}</option>
                    </select>
                    <label for="teletext-resources">{tr(TextKey::TeletextResourceLifecycle)}</label>
                    <p id="teletext-resources-help">{tr(TextKey::TeletextResourceLifecycleHelp)}</p>
                    <textarea id="teletext-resources" name="resource_transmissions" required
                        rows="5" maxlength="65536" aria-describedby="teletext-resources-help"
                        inner_html=textarea_content("")></textarea>
                    <button type="submit" class=btn_class(ButtonVariant::Primary)>
                        {tr(TextKey::TeletextValidateSaveDraft)}
                    </button>
                </fieldset>
            </form>
        </section>
    }.into_any()
}

pub(super) fn effective_authoring_controls(data: &ChannelTeletextData) -> AnyView {
    let observed = data.effective_authoring.as_ref();
    let available = observed.is_some();
    let absent = observed.is_some_and(|scope| scope.is_absent);
    let options = observed
        .into_iter()
        .flat_map(|scope| &scope.producer_addon_ids)
        .map(|id| {
            let name = data
                .addons
                .iter()
                .find(|addon| addon.addon_id == *id)
                .map_or_else(
                    || id.clone(),
                    |addon| {
                        if addon.name_labels.is_empty() {
                            addon.name.clone()
                        } else {
                            super::teletext::manifest_label(&addon.name_labels)
                        }
                    },
                );
            view! { <option value=id.clone()>{name}</option> }
        })
        .collect_view();
    let content = (!absent).then(|| view! {
        <label for="teletext-effective-entry-producer">{tr(TextKey::TeletextEntryProducer)}</label>
        <select id="teletext-effective-entry-producer" name="entry_addon_id" required>
            <option value="" selected disabled>{tr(TextKey::TeletextEntryProducer)}</option>{options}
        </select>
        {entry_fields("teletext-effective")}
        <label for="teletext-effective-v1-region">{tr(TextKey::TeletextV1Region)}</label>
        <select id="teletext-effective-v1-region" name="v1_region" required>
            <option value="" selected disabled>{tr(TextKey::TeletextV1Region)}</option>
            <option value="western_latin">{tr(TextKey::TeletextV1WesternLatin)}</option>
        </select>
        <label for="teletext-effective-resources">{tr(TextKey::TeletextResourceLifecycle)}</label>
        <p id="teletext-effective-resources-help">{tr(TextKey::TeletextResourceLifecycleHelp)}</p>
        <textarea id="teletext-effective-resources" name="resource_transmissions" required rows="5"
            maxlength="65536" aria-describedby="teletext-effective-resources-help"
            inner_html=textarea_content("")></textarea>
    });
    view! {
        <section class=style::authoring data-testid="teletext-effective-authoring"
            data-state=if absent { "absent" } else if available { "available" } else { "unavailable" }>
            <h2>{tr(TextKey::TeletextEffectiveAuthoringTitle)}</h2>
            <p>{tr(TextKey::TeletextEffectiveAuthoringHelp)}</p>
            <p role="status" hidden=available>{tr(TextKey::TeletextEffectiveAuthoringUnavailable)}</p>
            <p role="status" hidden=!absent>{tr(TextKey::TeletextEffectiveAuthoringAbsent)}</p>
            {scope_expiry(observed.and_then(|scope| scope.valid_until.as_deref()))}
            <form method="post" action="/admin-api/teletext/design/prepare"
                data-testid="teletext-effective-authoring-form">
                {hidden("channel_id", data.channel_id.to_string())}
                {hidden("scope", "effective".to_owned())}
                {hidden("selection_digest", observed.map_or_else(String::new, |scope| scope.selection_digest.clone()))}
                <fieldset disabled=!available class=style::fields>
                    <legend>{tr(TextKey::TeletextEffectiveAuthoringTitle)}</legend>
                    <label for="teletext-effective-draft-name">{tr(TextKey::TeletextDesignName)}</label>
                    <input id="teletext-effective-draft-name" name="name" required
                        maxlength=ersatzrs_domain::MAX_TELETEXT_DESIGN_NAME_BYTES/>
                    {content}
                    <button type="submit" class=btn_class(ButtonVariant::Primary)>{tr(TextKey::TeletextValidateSaveDraft)}</button>
                </fieldset>
            </form>
        </section>
    }.into_any()
}

fn scope_expiry(until: Option<&str>) -> AnyView {
    until.map(|until| view! {
        <p data-testid="teletext-scope-expiry">{tr(TextKey::TeletextEffectiveScopeExpiry)}" "
            <time datetime=until.to_owned()>{until.to_owned()}</time>
        </p>
    }).into_any()
}

fn preview_return_fields(owner: Option<TeletextPageAddress>) -> AnyView {
    match owner {
        Some(owner) => view! {
            {hidden("preview_page", owner.page.get().to_string())}
            {hidden("preview_subcode", format!("{:04X}", owner.subcode.get()))}
        }
        .into_any(),
        None => hidden("preview_absence", "1".to_owned()),
    }
}

fn entry_fields(prefix: &str) -> AnyView {
    let page_id = format!("{prefix}-entry-page");
    let subcode_id = format!("{prefix}-entry-subcode");
    view! {
        <div class=style::address>
            <div><label for=page_id.clone()>{tr(TextKey::TeletextEntryPage)}</label>
                <input id=page_id name="entry_page" type="number"
                    min="100" max="899" step="1" required/></div>
            <div><label for=subcode_id.clone()>{tr(TextKey::TeletextEntrySubpage)}</label>
                <input id=subcode_id name="entry_subcode" pattern="[0-9a-fA-F]{4}"
                    minlength="4" maxlength="4" required/></div>
        </div>
    }
    .into_any()
}

fn hidden(name: &str, value: String) -> AnyView {
    view! { <input type="hidden" name=name.to_owned() value=value/> }.into_any()
}

fn textarea_content(value: &str) -> String {
    // Tachys emits raw textarea children. Escape explicitly and preserve the
    // initial newline ignored by HTML parsing, including normalized CR/CRLF.
    let escaped = value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    if value.starts_with(['\n', '\r']) {
        format!("\n{escaped}")
    } else {
        escaped
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct PreviewQuery {
    pub(super) design_id: i64,
    pub(super) revision: i64,
    pub(super) owner: Option<TeletextPageAddress>,
}

pub(super) fn preview_query(values: &leptos_router::params::ParamsMap) -> Option<PreviewQuery> {
    let owner = match values.get("absence").as_deref() {
        Some("1") if values.get("page").is_none() && values.get("subcode").is_none() => None,
        Some(_) => return None,
        None => {
            let subcode = values.get("subcode")?;
            if subcode.len() != 4 || !subcode.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return None;
            }
            Some(TeletextPageAddress {
                page: ersatzrs_domain::TeletextPageNumber::new(values.get("page")?.parse().ok()?)
                    .ok()?,
                subcode: ersatzrs_domain::TeletextSubcode::new(
                    u16::from_str_radix(&subcode, 16).ok()?,
                )
                .ok()?,
            })
        }
    };
    let query = PreviewQuery {
        design_id: values.get("design_id")?.parse().ok()?,
        revision: values.get("revision")?.parse().ok()?,
        owner,
    };
    (query.design_id > 0 && query.revision > 0).then_some(query)
}

#[component]
pub fn TeletextDraftPreview(
    preview: Option<Result<TeletextDraftPreviewResult, ServerFnError>>,
    requested: bool,
    redirected: Option<TeletextDraftErrorLocation>,
) -> impl IntoView {
    let content = match preview {
        Some(result) => admin_result_view(
            result.map_err(|_| draft_status_text("unavailable").to_owned()),
            preview_result,
        ),
        None if requested => {
            view! { <p role="alert">{draft_status_text("invalid-input")}</p> }.into_any()
        }
        None => ().into_any(),
    };
    view! {
        <section class=style::preview data-testid="teletext-draft-preview">
            <h2>{tr(TextKey::TeletextPreviewTitle)}</h2>
            <p>{tr(TextKey::TeletextPreviewHelp)}</p>
            {redirected.map(|location| error_location(&location))}
            {content}
        </section>
    }
}

fn preview_result(result: TeletextDraftPreviewResult) -> AnyView {
    match result.preview {
        Some(preview) => preview_view(&preview),
        None => view! {
            <div role="alert" data-testid="teletext-preview-error">
                <p>{draft_status_text(result.error.as_deref().unwrap_or("unavailable"))}</p>
                {result.error_location.as_ref().map(error_location)}
            </div>
        }
        .into_any(),
    }
}

fn address_text(owner: TeletextPageAddress) -> String {
    format!("{} / {:04X}", owner.page.get(), owner.subcode.get())
}

fn preview_url(data: &TeletextDraftPreviewData, owner: TeletextPageAddress) -> String {
    format!(
        "/channels/{}/teletext?design_id={}&revision={}&page={}&subcode={:04X}",
        data.channel_id,
        data.design_id,
        data.revision,
        owner.page.get(),
        owner.subcode.get()
    )
}

fn page_link(data: &TeletextDraftPreviewData, owner: TeletextPageAddress) -> AnyView {
    view! {
        <a href=preview_url(data, owner) aria-current=(Some(owner) == data.owner).then_some("page")>
            {address_text(owner)}
        </a>
    }
    .into_any()
}

fn preview_view(data: &TeletextDraftPreviewData) -> AnyView {
    let description = data.owner.map_or_else(
        || tr(TextKey::TeletextPreviewAbsent).to_owned(),
        |owner| {
            format!(
                "{} · {}",
                tr(TextKey::TeletextPreviewTitle),
                address_text(owner)
            )
        },
    );
    let image = data.image_url.as_ref().map(|url| {
        view! {
            <img class=style::raster src=url.clone() alt=description.clone()/>
        }
    });
    let inventory = data
        .pages
        .iter()
        .map(|owner| page_link(data, *owner))
        .collect_view();
    let navigation = data
        .navigation
        .iter()
        .map(|link| navigation_link(data, link))
        .collect_view();
    let diagnostics = diagnostic_list(&data.diagnostics);
    view! {
        <div data-testid="teletext-preview-model" data-scope=if data.effective { "effective" } else { "base" }
            data-page=data.owner.map(|owner| owner.page.get())
            data-subcode=data.owner.map(|owner| format!("{:04X}", owner.subcode.get()))>
            {preview_controls(data)}
            <p data-testid="teletext-preview-absence" role="status" hidden=data.owner.is_some()>{tr(TextKey::TeletextPreviewAbsent)}</p>
            {data.owner.map(|owner| view! { <p>{address_text(owner)}</p> })}
            {scope_expiry(data.valid_until.as_deref())}
            {image}
            {data.owner.map(|_| preview_legend())}
            <nav class=style::inventory aria-label=tr(TextKey::TeletextPreviewPage)>{inventory}</nav>
            <ul class=style::navigation>{navigation}</ul>
            {diagnostics}
            {editorial::boundaries(data)}
            {editorial_controls(data)}
            {publication_controls(data)}
            {output_status::view(data)}
        </div>
    }.into_any()
}

fn editorial_controls(data: &TeletextDraftPreviewData) -> AnyView {
    if data.effective && data.owner.is_none() {
        return view! {
            <section data-testid="teletext-editor-controls" data-state="unavailable">
                <h3>{tr(TextKey::TeletextEditorTitle)}</h3>
                <p role="status">{tr(TextKey::TeletextPreviewAbsent)}</p>
            </section>
        }
        .into_any();
    }
    let Some(owner) = data.owner else {
        return ().into_any();
    };
    let Some(observation) = data.publication.as_ref() else {
        return view! {
            <section data-testid="teletext-editor-controls" data-state="unavailable">
                <h3>{tr(TextKey::TeletextEditorTitle)}</h3>
                <p role="status">{tr(TextKey::TeletextEditorUnavailable)}</p>
            </section>
        }
        .into_any();
    };
    view! {
        <section data-testid="teletext-editor-controls" data-state="available"
            aria-labelledby="teletext-editor-title">
            <h3 id="teletext-editor-title">{tr(TextKey::TeletextEditorTitle)}</h3>
            <p>{tr(TextKey::TeletextEditorHelp)}</p>
            <p>{tr(TextKey::TeletextEditorOwnershipHelp)}</p>
            <p>{tr(TextKey::TeletextEditorTemplateHelp)}</p>
            <div class=style::preview_form>
                <form method="post" action="/admin-api/teletext/design/export"
                    data-testid="teletext-editor-export-all-form">
                    {editorial_export_fields(data, observation)}
                    <button type="submit" class=btn_class(ButtonVariant::Primary)>
                        {tr(TextKey::TeletextEditorExportSet)}
                    </button>
                </form>
                <form method="post" action="/admin-api/teletext/design/export"
                    data-testid="teletext-editor-export-page-form">
                    {editorial_export_fields(data, observation)}
                    {hidden("export_page", owner.page.get().to_string())}
                    {hidden("export_subcode", format!("{:04X}", owner.subcode.get()))}
                    <button type="submit" class=btn_class(ButtonVariant::Primary)>
                        {tr(TextKey::TeletextEditorExportPage)}
                    </button>
                </form>
            </div>
            {resource_banks::form(data, observation)}
            {resource_announcements::form(data, observation)}
            {resource_selection::form(data, observation)}
            {resource_add::form(data, observation)}
            {object_copy_form(data, observation)}
            {resource_remove::form(data, observation)}
            {editorial_import_form(data.channel_id, observation.current_design_head)}
            <p>{tr(TextKey::TeletextEditorNoPublicationHelp)}</p>
        </section>
    }
    .into_any()
}

fn editorial_export_fields(
    data: &TeletextDraftPreviewData,
    observation: &TeletextDraftPublicationObservation,
) -> AnyView {
    view! {
        {hidden("channel_id", data.channel_id.to_string())}
        {hidden("design_id", data.design_id.to_string())}
        {hidden("revision", data.revision.to_string())}
        {hidden("digest", observation.digest.clone())}
        {hidden("expected_design_head", observation.current_design_head.to_string())}
    }
    .into_any()
}

fn object_copy_form(
    data: &TeletextDraftPreviewData,
    observation: &TeletextDraftPublicationObservation,
) -> AnyView {
    if data.resource_objects.is_empty() {
        return ().into_any();
    }
    let current = data.revision == observation.current_design_head;
    let options = data
        .resource_objects
        .iter()
        .filter_map(|identity| {
            let resource = identity.resource()?;
            let value = serde_json::to_string(identity).ok()?;
            let label = format!(
                "{:?} {:X}{:02X} S1={} · {}",
                resource.kind(),
                resource.page().magazine(),
                resource.page().page(),
                resource.logical_subpage().get(),
                identity.object_id().get(),
            );
            Some(view! { <option value=value>{label}</option> })
        })
        .collect_view();
    view! {
        <form method="post" action="/admin-api/teletext/design/copy-object"
            class=style::preview_form data-testid="teletext-object-copy-form"
            aria-describedby="teletext-object-copy-help">
            {editorial_export_fields(data, observation)}
            <fieldset disabled=!current>
                <legend>{tr(TextKey::TeletextObjectCopyTitle)}</legend>
                <p id="teletext-object-copy-help">{tr(TextKey::TeletextObjectCopyHelp)}</p>
                <label for="teletext-object-copy-name">{tr(TextKey::TeletextDesignName)}</label>
                <input id="teletext-object-copy-name" name="name" required
                    maxlength=ersatzrs_domain::MAX_TELETEXT_DESIGN_NAME_BYTES/>
                <label for="teletext-object-copy-source">{tr(TextKey::TeletextObjectCopySource)}</label>
                <select id="teletext-object-copy-source" name="source_object" required>{options}</select>
                <label for="teletext-object-copy-number">{tr(TextKey::TeletextObjectCopyNumber)}</label>
                <input id="teletext-object-copy-number" name="new_object_id" type="number"
                    min="0" max="65535" step="1" required/>
                <button type="submit" class=btn_class(ButtonVariant::Primary)>
                    {tr(TextKey::TeletextObjectCopyAction)}
                </button>
            </fieldset>
            <p style:display=if current { "none" } else { "" }>
                {tr(TextKey::TeletextObjectCopyCurrentHelp)}
            </p>
        </form>
    }.into_any()
}

fn editorial_import_form(channel_id: i64, current_design_head: i64) -> AnyView {
    view! {
        <p id="teletext-editor-import-help">{tr(TextKey::TeletextEditorImportHelp)}</p>
        <form method="post" action=format!("/admin-api/teletext/design/import?channel_id={channel_id}")
            enctype="multipart/form-data" class=style::preview_form
            aria-describedby="teletext-editor-import-help"
            data-testid="teletext-editor-import-form">
            {hidden("channel_id", channel_id.to_string())}
            {hidden("expected_design_head", current_design_head.to_string())}
            <div>
                <label for="teletext-editor-name">{tr(TextKey::TeletextDesignName)}</label>
                <input id="teletext-editor-name" name="name" required
                    maxlength=ersatzrs_domain::MAX_TELETEXT_DESIGN_NAME_BYTES/>
            </div>
            <div>
                <label for="teletext-editor-sidecar">{tr(TextKey::TeletextEditorSidecar)}</label>
                <input id="teletext-editor-sidecar" name="sidecar" type="file"
                    accept=".json" required/>
            </div>
            <div>
                <label for="teletext-editor-files">{tr(TextKey::TeletextEditorFiles)}</label>
                <input id="teletext-editor-files" name="files" type="file"
                    accept=".tti,.dvb" multiple required/>
            </div>
            <button type="submit" class=btn_class(ButtonVariant::Primary)>
                {tr(TextKey::TeletextEditorImport)}
            </button>
        </form>
    }
    .into_any()
}

fn publication_controls(data: &TeletextDraftPreviewData) -> AnyView {
    let Some(observation) = data.publication.as_ref() else {
        return view! {
            <section data-testid="teletext-publication" data-state="unavailable">
                <h3>{tr(TextKey::TeletextPublicationTitle)}</h3>
                <p role="status">{tr(TextKey::TeletextPublicationUnavailable)}</p>
            </section>
        }
        .into_any();
    };
    let (state, message) = match observation.state {
        TeletextDraftPublicationState::Unrequested => {
            ("unrequested", TextKey::TeletextPublicationUnrequested)
        }
        TeletextDraftPublicationState::Pending => ("pending", TextKey::TeletextPublicationPending),
        TeletextDraftPublicationState::Selected => {
            ("selected", TextKey::TeletextPublicationSelected)
        }
        TeletextDraftPublicationState::Unavailable => {
            ("unavailable", TextKey::TeletextPublicationUnavailable)
        }
    };
    view! {
        <section data-testid="teletext-publication" data-state=state aria-labelledby="teletext-publication-title">
            <h3 id="teletext-publication-title">{tr(TextKey::TeletextPublicationTitle)}</h3>
            <p>{tr(TextKey::TeletextPublicationHelp)}</p>
            <p role="status" data-testid="teletext-publication-status">{tr(message)}</p>
            <p hidden=!observation.replaces_pending data-testid="teletext-publication-replaces-pending">
                {tr(TextKey::TeletextPublicationReplacePending)}
            </p>
            <div class=style::preview_form>
                <form method="post" action="/admin-api/teletext/design/request-publication"
                    data-testid="teletext-publication-request-form">
                    {publication_fields(data, observation)}
                    <button type="submit" class=btn_class(ButtonVariant::Primary) disabled=!observation.can_request>
                        {tr(TextKey::TeletextPublicationRequest)}
                    </button>
                </form>
                <form method="post" action="/admin-api/teletext/design/publish"
                    data-testid="teletext-publication-publish-form">
                    {publication_fields(data, observation)}
                    {hidden("expected_generation_id", observation.current_accepted_id.map_or_else(String::new, |id| id.to_string()))}
                    <button type="submit" class=btn_class(ButtonVariant::Primary) disabled=!observation.can_publish>
                        {tr(TextKey::TeletextPublicationPublish)}
                    </button>
                </form>
            </div>
        </section>
    }.into_any()
}

fn publication_fields(
    data: &TeletextDraftPreviewData,
    observation: &TeletextDraftPublicationObservation,
) -> AnyView {
    view! {
        {hidden("channel_id", data.channel_id.to_string())}
        {hidden("design_id", data.design_id.to_string())}
        {hidden("revision", data.revision.to_string())}
        {hidden("digest", observation.digest.clone())}
        {hidden("expected_design_head", observation.current_design_head.to_string())}
        {hidden("expected_request_id", observation.current_pending_id.map_or_else(String::new, |id| id.to_string()))}
        // Preview navigation is a return destination, never the opening page.
        {preview_return_fields(data.owner)}
    }.into_any()
}

fn preview_controls(data: &TeletextDraftPreviewData) -> AnyView {
    let Some(owner) = data.owner else {
        return view! { <a href=format!("/channels/{}/teletext?design_id={}&revision={}&absence=1", data.channel_id, data.design_id, data.revision)>{tr(TextKey::TeletextPreviewReload)}</a> }.into_any();
    };
    view! {
        <form method="get" action=format!("/channels/{}/teletext", data.channel_id)
            class=style::preview_form data-testid="teletext-preview-form">
            {hidden("design_id", data.design_id.to_string())}
            {hidden("revision", data.revision.to_string())}
            <div><label for="teletext-preview-page">{tr(TextKey::TeletextPreviewPage)}</label>
                <input id="teletext-preview-page" name="page" type="number" min="100" max="899"
                    step="1" required value=owner.page.get()/></div>
            <div><label for="teletext-preview-subcode">{tr(TextKey::TeletextPreviewSubpage)}</label>
                <input id="teletext-preview-subcode" name="subcode" pattern="[0-9a-fA-F]{4}"
                    minlength="4" maxlength="4" required value=format!("{:04X}", owner.subcode.get())/></div>
            <button type="submit" class=btn_class(ButtonVariant::Primary)>{tr(TextKey::TeletextPreviewLoad)}</button>
            <a href=preview_url(data, owner)>{tr(TextKey::TeletextPreviewReload)}</a>
        </form>
    }.into_any()
}

fn navigation_role(role: TeletextNavigationRole) -> &'static str {
    tr(match role {
        TeletextNavigationRole::Index => TextKey::TeletextNavIndex,
        TeletextNavigationRole::Red => TextKey::TeletextNavRed,
        TeletextNavigationRole::Green => TextKey::TeletextNavGreen,
        TeletextNavigationRole::Yellow => TextKey::TeletextNavYellow,
        TeletextNavigationRole::Cyan => TextKey::TeletextNavCyan,
    })
}

fn navigation_targets(
    data: &TeletextDraftPreviewData,
    link: &TeletextNavigationLink,
) -> Vec<TeletextPageAddress> {
    data.pages
        .iter()
        .filter(|owner| {
            owner.page == link.target.page
                && link
                    .target
                    .subcode
                    .is_none_or(|subcode| owner.subcode == subcode)
        })
        .copied()
        .collect()
}

fn navigation_link(data: &TeletextDraftPreviewData, link: &TeletextNavigationLink) -> AnyView {
    let targets = navigation_targets(data, link)
        .into_iter()
        .map(|owner| page_link(data, owner))
        .collect_view();
    let target = link.target.subcode.map_or_else(
        || link.target.page.get().to_string(),
        |subcode| format!("{} / {:04X}", link.target.page.get(), subcode.get()),
    );
    view! { <li><span>{navigation_role(link.role)}" · "{target}</span><div>{targets}</div></li> }
        .into_any()
}

fn preview_legend() -> AnyView {
    view! {
        <ul class=style::legend aria-label=tr(TextKey::TeletextPreviewDiagnostics)>
            <li><span class=style::header_pattern aria-hidden="true"></span>{tr(TextKey::TeletextPreviewReceiverHeader)}</li>
            <li><span class=style::video_pattern aria-hidden="true"></span>{tr(TextKey::TeletextPreviewVideo)}</li>
            <li><span class=style::unresolved_pattern aria-hidden="true"></span>{tr(TextKey::TeletextPreviewUnresolved)}</li>
        </ul>
    }.into_any()
}

fn diagnostic_list(diagnostics: &[TeletextDraftDiagnosticData]) -> AnyView {
    let entries = diagnostics.iter().map(|diagnostic| {
        let original = diagnostic.original_position.map(|(row, column)| format!("{row}, {column}"));
        view! {
            <li>{address_text(diagnostic.owner)}" · "
                {tr(TextKey::TeletextPreviewPosition)}" "{diagnostic.visual_row}", "{diagnostic.visual_column}
                <span>{tr(TextKey::TeletextPreviewSource)}" "{diagnostic.category.clone()}</span>
                {original.map(|position| view! { <span>{tr(TextKey::TeletextPreviewSource)}" · "{tr(TextKey::TeletextPreviewPosition)}" "{position}</span> })}
                {diagnostic_provenance(diagnostic)}
            </li>
        }
    }).collect_view();
    view! {
        <section class=style::diagnostics data-testid="teletext-preview-diagnostics">
            <h3>{tr(TextKey::TeletextPreviewDiagnostics)}</h3>
            <p hidden=!diagnostics.is_empty()>{tr(TextKey::TeletextPreviewNoDiagnostics)}</p>
            <ul>{entries}</ul>
        </section>
    }
    .into_any()
}

fn diagnostic_provenance(diagnostic: &TeletextDraftDiagnosticData) -> AnyView {
    let indices = diagnostic
        .candidate
        .map(|index| format!("candidate {index}"))
        .into_iter()
        .chain(
            diagnostic
                .invocation
                .map(|index| format!("invocation {index}")),
        )
        .collect::<Vec<_>>()
        .join(" / ");
    let indices_empty = indices.is_empty();
    view! {
        <span hidden=diagnostic.source.is_none()>{tr(TextKey::TeletextPreviewSource)}" "{diagnostic.source.clone()}</span>
        <span hidden=indices_empty>{tr(TextKey::TeletextPreviewSource)}" "{indices}</span>
    }.into_any()
}

pub(super) fn redirected_error_location(
    values: &leptos_router::params::ParamsMap,
) -> Option<TeletextDraftErrorLocation> {
    use ersatzrs_domain::{TeletextPageNumber, TeletextSubcode};
    let owner = values
        .get("error_page")
        .and_then(|page| page.parse().ok())
        .and_then(|page| TeletextPageNumber::new(page).ok())
        .zip(
            values
                .get("error_subcode")
                .and_then(|subcode| u16::from_str_radix(&subcode, 16).ok())
                .and_then(|subcode| TeletextSubcode::new(subcode).ok()),
        )
        .map(|(page, subcode)| TeletextPageAddress { page, subcode });
    let position = values
        .get("error_row")
        .and_then(|row| row.parse::<u16>().ok())
        .zip(
            values
                .get("error_column")
                .and_then(|column| column.parse::<u16>().ok()),
        );
    let resource = values
        .get("error_resource")
        .filter(|raw| raw.len() <= 2048)
        .and_then(|raw| {
            serde_json::from_str::<ersatzrs_domain::TeletextResourceIdentity>(&raw).ok()
        });
    (owner.is_some() || position.is_some() || resource.is_some()).then_some(
        TeletextDraftErrorLocation {
            owner,
            resource,
            position,
            source: None,
        },
    )
}

fn error_location(location: &TeletextDraftErrorLocation) -> AnyView {
    let owner = location.owner.map(address_text);
    let position = location
        .position
        .map(|(row, column)| format!("{row}, {column}"));
    let resource = location.resource.map(|identity| {
        use ersatzrs_domain::TeletextResourceKind;
        let kind = match identity.kind() {
            TeletextResourceKind::Pop => "pop",
            TeletextResourceKind::Gpop => "gpop",
            TeletextResourceKind::Drcs => "drcs",
            TeletextResourceKind::Gdrcs => "gdrcs",
            TeletextResourceKind::Mot => "mot",
            TeletextResourceKind::Mip => "mip",
        };
        format!(
            "{kind} {}{:02X} / {:X}",
            identity.page().magazine(),
            identity.page().page(),
            identity.logical_subpage().get()
        )
    });
    view! {
        <dl class=style::location data-testid="teletext-error-location">
            {owner.map(|owner| view! { <dt>{tr(TextKey::TeletextPreviewPage)}</dt><dd>{owner}</dd> })}
            {resource.map(|resource| view! { <dt>{tr(TextKey::TeletextResourceLifecycle)}</dt><dd>{resource}</dd> })}
            {position.map(|position| view! { <dt>{tr(TextKey::TeletextPreviewPosition)}</dt><dd>{position}</dd> })}
            {location.source.clone().map(|source| view! { <dt>{tr(TextKey::TeletextPreviewSource)}</dt><dd>{source}</dd> })}
        </dl>
    }.into_any()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::server_fns::teletext::{TeletextAddonData, TeletextBindingData};
    use ersatzrs_domain::{TeletextNavigationTarget, TeletextPageNumber, TeletextSubcode};

    fn owner(page: u16, subcode: u16) -> TeletextPageAddress {
        TeletextPageAddress {
            page: TeletextPageNumber::new(page).unwrap(),
            subcode: TeletextSubcode::new(subcode).unwrap(),
        }
    }

    fn channel() -> ChannelTeletextData {
        let addon = |addon_id: &str, enabled: bool| TeletextAddonData {
            addon_id: addon_id.to_owned(),
            name: addon_id.to_owned(),
            name_labels: std::collections::BTreeMap::default(),
            installed: true,
            availability: "configured".to_owned(),
            installation: None,
            settings: Vec::new(),
            profiles: Vec::new(),
            binding: Some(TeletextBindingData {
                profile_id: 1,
                enabled,
                revision: 2,
                availability: "configured".to_owned(),
            }),
        };
        ChannelTeletextData {
            channel_id: 44,
            channel_number: "1".to_owned(),
            channel_name: "Synthetic".to_owned(),
            catalog_available: true,
            authoring_selection: Some("observed-membership".to_owned()),
            authoring_error: None,
            effective_authoring: None,
            effective_authoring_error: None,
            addons: vec![
                addon("org.example.on", true),
                addon("org.example.off", false),
            ],
        }
    }

    fn preview() -> TeletextDraftPreviewData {
        TeletextDraftPreviewData {
            channel_id: 44,
            design_id: 8,
            revision: 3,
            owner: Some(owner(200, 0x10)),
            image_url: Some("data:image/svg+xml;base64,PHN2Zy8+".to_owned()),
            effective: false,
            valid_until: None,
            pages: vec![owner(200, 0x10), owner(201, 0x20), owner(201, 0x30)],
            navigation: vec![TeletextNavigationLink {
                role: TeletextNavigationRole::Red,
                target: TeletextNavigationTarget {
                    page: TeletextPageNumber::new(201).unwrap(),
                    subcode: None,
                },
            }],
            diagnostics: Vec::new(),
            resource_objects: Vec::new(),
            removable_resource_objects: Vec::new(),
            object_resources: Vec::new(),
            object_choices:
                crate::admin::server_fns::teletext_drafts::TeletextObjectChoicesObservation::default(
                ),
            authoring: Vec::new(),
            publication: None,
            outputs: None,
        }
    }

    #[test]
    fn output_status_is_localized_read_only_and_preserves_historical_receipts() {
        use ersatzrs_domain::{
            TeletextChannelOutputObservations, TeletextOutputGenerationObservation,
            TeletextOutputObservation, TeletextOutputObservationPhase as Phase,
            TeletextOutputSelectionObservation as Selection,
        };
        for language in [
            "en-us", "nl", "pl", "pt-br", "fr-fr", "de-de", "es-es", "es-419", "it-it",
        ] {
            let owner = Owner::new();
            owner.with(|| {
                provide_context(crate::admin::i18n::UiLanguage::from_config(language));
                let mut data = preview();
                let unavailable = output_status::view(&data).to_html();
                assert!(unavailable.contains(tr(TextKey::TeletextDeliveryUnavailable)));
                data.outputs = Some(TeletextChannelOutputObservations {
                    outputs: vec![],
                    incomplete: false,
                });
                assert!(output_status::view(&data)
                    .to_html()
                    .contains(tr(TextKey::TeletextDeliveryEmpty)));
                let selection = |id, sequence| Selection {
                    sequence,
                    generation: Some(TeletextOutputGenerationObservation {
                        generation_id: id,
                        request_id: 8,
                        design_id: 8,
                        revision: 3,
                        effective: false,
                    }),
                    authored: true,
                };
                data.outputs = Some(TeletextChannelOutputObservations {
                    outputs: vec![TeletextOutputObservation {
                        epoch: uuid::Uuid::new_v4(),
                        requested: selection(20, 2),
                        applied: Some(selection(19, 1)),
                        phase: Phase::Waiting,
                    }],
                    incomplete: true,
                });
                let html = output_status::view(&data).to_html();
                for text in [
                    TextKey::TeletextDeliveryWaiting,
                    TextKey::TeletextDeliveryHelp,
                    TextKey::TeletextDeliveryConfirmed,
                    TextKey::TeletextDeliveryIncomplete,
                ] {
                    assert!(html.contains(tr(text)), "{language}");
                }
                assert!(html.contains("data-requested-generation=\"20\""));
                assert!(html.contains("data-applied-generation=\"19\""));
                assert!(!html.contains("data-state=\"applied\""));
                assert!(!html.contains("<form"));
                let observation = &mut data.outputs.as_mut().unwrap().outputs[0];
                observation.phase = Phase::Rejected;
                assert!(output_status::view(&data)
                    .to_html()
                    .contains(tr(TextKey::TeletextDeliveryRejected)));
                let observation = &mut data.outputs.as_mut().unwrap().outputs[0];
                observation.phase = Phase::Applied;
                observation.requested.authored = false;
                observation.applied = Some(observation.requested.clone());
                let html = output_status::view(&data).to_html();
                assert!(html.contains(tr(TextKey::TeletextDeliveryAbsent)));
                assert!(html.contains(tr(TextKey::TeletextDeliveryApplied)));
            });
        }
    }

    #[test]
    fn authoring_form_requires_explicit_input_and_preserves_membership() {
        let owner = Owner::new();
        let mut data = channel();
        let html = owner.with(|| authoring_controls(&data).to_html());
        assert!(html.contains("name=\"selection_digest\" value=\"observed-membership\""));
        assert!(html.contains("value=\"org.example.on\""));
        assert!(!html.contains("value=\"org.example.off\""));
        for name in ["name", "entry_page", "entry_subcode"] {
            let input = html
                .split(&format!("name=\"{name}\""))
                .nth(1)
                .unwrap()
                .split('>')
                .next()
                .unwrap();
            assert!(!input.contains("value="));
            assert!(input.contains("required"));
        }
        let textarea = html.split("<textarea").nth(1).unwrap();
        let content = textarea
            .split_once('>')
            .unwrap()
            .1
            .split_once("</textarea>")
            .unwrap()
            .0;
        assert_eq!(content, "");
        assert!(!html.contains("<fieldset disabled"));
        data.catalog_available = false;
        let html = owner.with(|| authoring_controls(&data).to_html());
        assert!(html.contains("<fieldset disabled"));
        assert!(html.contains("name=\"selection_digest\" value=\"observed-membership\""));
        data.catalog_available = true;
        let addons = std::mem::take(&mut data.addons);
        let html = owner.with(|| authoring_controls(&data).to_html());
        assert!(html.contains("<fieldset disabled"));
        assert!(html.contains("name=\"selection_digest\" value=\"observed-membership\""));
        data.addons = addons;
        data.authoring_selection = None;
        assert!(owner
            .with(|| authoring_controls(&data).to_html())
            .contains("<fieldset disabled"));
        assert_eq!(textarea_content("</textarea>&"), "&lt;/textarea&gt;&amp;");
        assert_eq!(textarea_content("\r\n[]"), "\n\r\n[]");
    }

    #[test]
    fn preview_navigation_retains_all_actual_subpages_without_inventing_zero() {
        let data = preview();
        assert_eq!(
            navigation_targets(&data, &data.navigation[0]),
            vec![owner(201, 0x20), owner(201, 0x30)]
        );
        let mut exact = data.navigation[0].clone();
        exact.target.subcode = Some(TeletextSubcode::new(0x30).unwrap());
        assert_eq!(navigation_targets(&data, &exact), vec![owner(201, 0x30)]);
        exact.target.subcode = Some(TeletextSubcode::new(0x40).unwrap());
        assert!(navigation_targets(&data, &exact).is_empty());
        let owner = Owner::new();
        let html = owner.with(|| preview_view(&data).to_html());
        for subcode in ["0010", "0020", "0030"] {
            assert!(html.contains(&format!("subcode={subcode}")));
        }
        assert!(!html.contains("subcode=0000"));
        assert!(html.contains("design_id=8"));
        assert!(html.contains("revision=3"));
        assert!(html.contains(tr(TextKey::TeletextNavRed)));
        assert!(html.contains("data:image/svg+xml;base64,PHN2Zy8+"));
        for key in [
            TextKey::TeletextPreviewReceiverHeader,
            TextKey::TeletextPreviewVideo,
            TextKey::TeletextPreviewUnresolved,
        ] {
            assert!(html.contains(tr(key)));
        }
    }

    #[test]
    fn preview_locations_and_diagnostics_are_checked_and_escaped() {
        let owner_context = Owner::new();
        let diagnostic = TeletextDraftDiagnosticData {
            owner: owner(200, 0x10),
            visual_row: 7,
            visual_column: 8,
            original_position: Some((3, 4)),
            category: "glyph".to_owned(),
            candidate: Some(5),
            source: Some("X26".to_owned()),
            invocation: Some(6),
        };
        let html = owner_context.with(|| diagnostic_list(&[diagnostic]).to_html());
        for value in ["200 / 0010", "3, 4", "X26", "candidate 5", "invocation 6"] {
            assert!(html.contains(value));
        }
        let html = owner_context.with(|| {
            preview_result(TeletextDraftPreviewResult {
                preview: None,
                error: Some("<script>private()</script>".to_owned()),
                error_location: Some(TeletextDraftErrorLocation {
                    owner: Some(owner(200, 0x10)),
                    resource: None,
                    position: Some((3, 4)),
                    source: Some("<synthetic>".to_owned()),
                }),
            })
            .to_html()
        });
        assert!(html.contains(tr(TextKey::TeletextAuthoringRejected)));
        assert!(html.contains("200 / 0010"));
        assert!(html.contains("&lt;synthetic&gt;"));
        assert!(!html.contains("private()"));
    }

    #[test]
    fn preloaded_preview_renders_without_router_or_async_resource_context() {
        let owner_context = Owner::new();
        let html = owner_context.with(|| {
            view! {
                <TeletextDraftPreview preview=Some(Ok(TeletextDraftPreviewResult {
                    preview: Some(preview()), error: None, error_location: None,
                })) requested=true redirected=None/>
            }
            .to_html()
        });
        assert!(html.contains("design_id=8"));
        assert!(html.contains("revision=3"));
        assert!(html.contains("data:image/svg+xml;base64,PHN2Zy8+"));
        for requested in [false, true] {
            let html = owner_context.with(|| {
                view! {
                    <TeletextDraftPreview preview=None requested=requested redirected=None/>
                }
                .to_html()
            });
            assert_eq!(html.contains(draft_status_text("invalid-input")), requested);
        }
        let html = owner_context.with(|| view! {
            <TeletextDraftPreview preview=Some(Err(ServerFnError::new("synthetic-private-transport")))
                requested=true redirected=Some(TeletextDraftErrorLocation {
                    owner: Some(owner(200, 0x10)), resource: None, position: Some((7, 8)), source: None,
                })/>
        }.to_html());
        assert!(html.contains(draft_status_text("unavailable")));
        assert!(html.contains("200 / 0010"));
        assert!(html.contains("7, 8"));
        assert!(!html.contains("synthetic-private-transport"));
    }

    #[test]
    fn publication_controls_preserve_observed_fences_and_do_not_invent_absence() {
        let owner_context = Owner::new();
        let mut data = preview();
        let html = owner_context.with(|| publication_controls(&data).to_html());
        assert!(html.contains("data-state=\"unavailable\""));
        assert!(!html.contains("<form"));
        data.publication = Some(TeletextDraftPublicationObservation {
            digest: "a".repeat(64),
            current_design_head: 7,
            current_pending_id: Some(19),
            current_accepted_id: Some(23),
            state: TeletextDraftPublicationState::Pending,
            can_request: false,
            can_publish: true,
            replaces_pending: false,
        });
        let html = owner_context.with(|| publication_controls(&data).to_html());
        for (name, value) in [
            ("channel_id", "44"),
            ("design_id", "8"),
            ("revision", "3"),
            ("expected_design_head", "7"),
            ("expected_request_id", "19"),
            ("preview_page", "200"),
            ("preview_subcode", "0010"),
        ] {
            assert_eq!(
                html.matches(&format!("name=\"{name}\" value=\"{value}\""))
                    .count(),
                2
            );
        }
        assert_eq!(
            html.matches(&format!("name=\"digest\" value=\"{}\"", "a".repeat(64)))
                .count(),
            2
        );
        assert!(!html.contains("entry_page"));
        assert!(!html.contains("entry_subcode"));
        assert!(html.contains(tr(TextKey::TeletextPublicationPending)));
        assert_eq!(html.matches("<form").count(), 2);
        let request = html
            .split("data-testid=\"teletext-publication-request-form\"")
            .nth(1)
            .unwrap()
            .split("</form>")
            .next()
            .unwrap();
        assert!(!request.contains("expected_generation_id"));
        assert!(request
            .split("<button")
            .nth(1)
            .unwrap()
            .split('>')
            .next()
            .unwrap()
            .contains("disabled"));
        let publish = html
            .split("data-testid=\"teletext-publication-publish-form\"")
            .nth(1)
            .unwrap()
            .split("</form>")
            .next()
            .unwrap();
        assert!(publish.contains("name=\"expected_generation_id\" value=\"23\""));
        assert!(!publish
            .split("<button")
            .nth(1)
            .unwrap()
            .split('>')
            .next()
            .unwrap()
            .contains("disabled"));
        data.publication.as_mut().unwrap().replaces_pending = true;
        data.publication.as_mut().unwrap().state = TeletextDraftPublicationState::Selected;
        data.publication.as_mut().unwrap().can_request = true;
        data.publication.as_mut().unwrap().can_publish = false;
        let html = owner_context.with(|| publication_controls(&data).to_html());
        assert!(html.contains(tr(TextKey::TeletextPublicationSelected)));
        let notice = html
            .split("data-testid=\"teletext-publication-replaces-pending\"")
            .nth(1)
            .unwrap();
        assert!(!notice.split('>').next().unwrap().contains("hidden"));
        assert!(html.contains(tr(TextKey::TeletextPublicationReplacePending)));
    }

    #[test]
    fn effective_forms_use_observed_producers_over_base_off_and_omit_absence_content() {
        let context = Owner::new();
        let mut data = channel();
        data.effective_authoring = Some(
            crate::admin::server_fns::teletext::TeletextEffectiveAuthoringData {
                selection_digest: "a".repeat(64),
                producer_addon_ids: vec!["org.example.off".to_owned()],
                valid_until: Some("2026-10-14T12:00:00+00:00".to_owned()),
                is_absent: false,
            },
        );
        let html = context.with(|| effective_authoring_controls(&data).to_html());
        assert!(html.contains("value=\"org.example.off\""));
        assert!(!html.contains("value=\"org.example.on\""));
        assert!(html.contains("name=\"scope\" value=\"effective\""));
        assert!(html.contains("datetime=\"2026-10-14T12:00:00+00:00\""));
        assert!(!html.contains("<fieldset disabled"));
        let observed = data.effective_authoring.as_mut().unwrap();
        observed.is_absent = true;
        observed.producer_addon_ids.clear();
        let html = context.with(|| effective_authoring_controls(&data).to_html());
        assert!(html.contains(tr(TextKey::TeletextEffectiveAuthoringAbsent)));
        for forbidden in [
            "entry_addon_id",
            "entry_page",
            "entry_subcode",
            "v1_region",
            "resource_transmissions",
        ] {
            assert!(!html.contains(&format!("name=\"{forbidden}\"")));
        }
        assert!(html.contains("name=\"name\""));
        assert!(!html.contains("<fieldset disabled"));
    }

    #[test]
    fn effective_historical_preview_retains_native_editor_forms_without_role_authority() {
        let context = Owner::new();
        let mut data = preview();
        data.effective = true;
        data.valid_until = Some("2020-01-01T00:00:00Z".to_owned());
        data.publication = Some(TeletextDraftPublicationObservation {
            digest: "b".repeat(64),
            current_design_head: 3,
            current_pending_id: None,
            current_accepted_id: None,
            state: TeletextDraftPublicationState::Unrequested,
            can_request: false,
            can_publish: false,
            replaces_pending: false,
        });
        let html = context.with(|| editorial_controls(&data).to_html());
        assert!(html.contains("data-state=\"available\""));
        for form in [
            "teletext-editor-export-all-form",
            "teletext-editor-export-page-form",
            "teletext-editor-import-form",
        ] {
            assert!(html.contains(form));
        }
        assert!(html.contains("name=\"expected_design_head\" value=\"3\""));
        assert!(html.contains("name=\"channel_id\" value=\"44\""));
        assert!(html.contains(tr(TextKey::TeletextEditorNoPublicationHelp)));
        assert!(!html.contains("<fieldset disabled"));
        for role_field in ["document_version", "scope", "effective_selection"] {
            assert!(!html.contains(&format!("name=\"{role_field}\"")));
        }
    }

    #[test]
    fn effective_absence_preview_has_no_fake_page_or_editor_authority() {
        let context = Owner::new();
        let mut data = preview();
        data.effective = true;
        data.owner = None;
        data.image_url = None;
        data.pages.clear();
        data.navigation.clear();
        data.publication = Some(TeletextDraftPublicationObservation {
            digest: "b".repeat(64),
            current_design_head: 3,
            current_pending_id: None,
            current_accepted_id: None,
            state: TeletextDraftPublicationState::Unrequested,
            can_request: true,
            can_publish: false,
            replaces_pending: false,
        });
        let html = context.with(|| preview_view(&data).to_html());
        assert!(html.contains(tr(TextKey::TeletextPreviewAbsent)));
        assert!(html.contains("name=\"preview_absence\" value=\"1\""));
        assert!(html.contains("absence=1"));
        assert!(!html.contains(tr(TextKey::TeletextEffectiveEditorUnavailable)));
        for forbidden in [
            "<img",
            "name=\"preview_page\"",
            "name=\"preview_subcode\"",
            "teletext-editor-export-all-form",
            "teletext-editor-import-form",
        ] {
            assert!(!html.contains(forbidden));
        }
        let mut values = leptos_router::params::ParamsMap::new();
        for (key, value) in [("design_id", "8"), ("revision", "3"), ("absence", "1")] {
            values.insert(key, value.to_owned());
        }
        assert!(preview_query(&values).unwrap().owner.is_none());
        values.insert("page", "200".to_owned());
        assert!(preview_query(&values).is_none());
    }

    #[test]
    fn preview_query_never_supplies_missing_design_or_address_defaults() {
        let mut values = leptos_router::params::ParamsMap::new();
        assert!(preview_query(&values).is_none());
        for (key, value) in [("design_id", "8"), ("revision", "3"), ("page", "200")] {
            values.insert(key, value.to_owned());
        }
        assert!(preview_query(&values).is_none());
        values.insert("subcode", "0010".to_owned());
        assert_eq!(
            preview_query(&values).unwrap().owner.unwrap().subcode.get(),
            0x10
        );
        values.insert("subcode", "10".to_owned());
        assert!(preview_query(&values).is_none());
        values.insert("error_row", "7".to_owned());
        values.insert("error_column", "8".to_owned());
        let location = redirected_error_location(&values).unwrap();
        assert_eq!(location.position, Some((7, 8)));
        assert_eq!(location.owner, None);
        let mut resource_values = leptos_router::params::ParamsMap::new();
        resource_values.insert(
            "error_resource",
            r#"{"kind":"pop","page":{"magazine":2,"page":160},"logical_subpage":3}"#.to_owned(),
        );
        let resource = redirected_error_location(&resource_values).unwrap();
        assert_eq!(resource.resource.unwrap().logical_subpage().get(), 3);
        assert_eq!(resource.owner, None);
        assert_eq!(resource.position, None);
        resource_values.insert("error_resource", r#"{"kind":"pop","page":{"magazine":2,"page":160},"logical_subpage":3,"unknown":"<script>"}"#.to_owned());
        assert!(redirected_error_location(&resource_values).is_none());
    }

    #[test]
    fn object_copy_form_is_server_populated_and_historical_revisions_cannot_submit() {
        use ersatzrs_domain::{
            TeletextObjectId, TeletextObjectIdentity, TeletextRawDataPageNumber,
            TeletextResourceIdentity, TeletextResourceKind, TeletextResourceSubpageIndex,
        };
        let context = Owner::new();
        let mut data = preview();
        let resource = TeletextResourceIdentity::new(
            TeletextResourceKind::Pop,
            TeletextRawDataPageNumber::new(2, 0xc0).unwrap(),
            TeletextResourceSubpageIndex::new(15).unwrap(),
        )
        .unwrap();
        data.resource_objects
            .push(TeletextObjectIdentity::public(resource, TeletextObjectId::new(21)).unwrap());
        let observation = TeletextDraftPublicationObservation {
            digest: "b".repeat(64),
            current_design_head: data.revision,
            current_pending_id: None,
            current_accepted_id: None,
            state: TeletextDraftPublicationState::Unrequested,
            can_request: false,
            can_publish: false,
            replaces_pending: false,
        };
        let html = context.with(|| object_copy_form(&data, &observation).to_html());
        assert!(html.contains("/admin-api/teletext/design/copy-object"));
        assert!(html.contains("Pop 2C0 S1=15 · 21"));
        assert!(html.contains("name=\"source_object\""));
        assert!(html.contains("name=\"new_object_id\""));
        assert!(html.contains(tr(TextKey::TeletextObjectCopyHelp)));
        assert!(!html.contains("<fieldset disabled"));
        data.revision -= 1;
        let html = context.with(|| object_copy_form(&data, &observation).to_html());
        assert!(html.contains("<fieldset disabled"));
        assert!(html.contains(tr(TextKey::TeletextObjectCopyCurrentHelp)));
        data.resource_objects.clear();
        let html = context.with(|| object_copy_form(&data, &observation).to_html());
        assert!(!html.contains("teletext-object-copy-form"));
    }

    #[test]
    fn object_remove_form_uses_only_observed_choices_and_disables_historical_submission() {
        use ersatzrs_domain::{
            TeletextObjectId, TeletextObjectIdentity, TeletextRawDataPageNumber,
            TeletextResourceIdentity, TeletextResourceKind, TeletextResourceSubpageIndex,
        };
        let context = Owner::new();
        let mut data = preview();
        let resource = TeletextResourceIdentity::new(
            TeletextResourceKind::Pop,
            TeletextRawDataPageNumber::new(2, 0xe0).unwrap(),
            TeletextResourceSubpageIndex::new(15).unwrap(),
        )
        .unwrap();
        let eligible = TeletextObjectIdentity::public(resource, TeletextObjectId::new(33)).unwrap();
        data.resource_objects
            .push(TeletextObjectIdentity::public(resource, TeletextObjectId::new(20)).unwrap());
        data.removable_resource_objects.push(eligible);
        let observation = TeletextDraftPublicationObservation {
            digest: "b".repeat(64),
            current_design_head: data.revision,
            current_pending_id: None,
            current_accepted_id: None,
            state: TeletextDraftPublicationState::Unrequested,
            can_request: false,
            can_publish: false,
            replaces_pending: false,
        };
        let html = context.with(|| resource_remove::form(&data, &observation).to_html());
        assert!(html.contains("/admin-api/teletext/design/remove-unused-object"));
        assert!(html.contains("Pop 2E0 S1=15 · 33"));
        assert!(!html.contains("Pop 2E0 S1=15 · 20"));
        assert!(html.contains("name=\"object\""));
        assert!(html.contains(tr(TextKey::TeletextObjectRemoveHelp)));
        assert!(!html.contains("<fieldset disabled"));
        data.revision -= 1;
        let html = context.with(|| resource_remove::form(&data, &observation).to_html());
        assert!(html.contains("<fieldset disabled"));
        assert!(html.contains(tr(TextKey::TeletextObjectRemoveCurrentHelp)));
        data.removable_resource_objects.clear();
        let html = context.with(|| resource_remove::form(&data, &observation).to_html());
        assert!(!html.contains("teletext-object-remove-form"));
    }

    #[test]
    fn first_object_form_lists_empty_resources_and_never_submits_a_program() {
        use ersatzrs_domain::{
            TeletextRawDataPageNumber, TeletextResourceIdentity, TeletextResourceKind,
            TeletextResourceSubpageIndex,
        };
        let context = Owner::new();
        let mut data = preview();
        data.object_resources = [TeletextResourceKind::Pop, TeletextResourceKind::Gpop]
            .into_iter()
            .map(|kind| {
                TeletextResourceIdentity::new(
                    kind,
                    TeletextRawDataPageNumber::new(2, 0xc0).unwrap(),
                    TeletextResourceSubpageIndex::new(15).unwrap(),
                )
                .unwrap()
            })
            .collect();
        assert!(data.resource_objects.is_empty());
        let observation = TeletextDraftPublicationObservation {
            digest: "b".repeat(64),
            current_design_head: data.revision,
            current_pending_id: None,
            current_accepted_id: None,
            state: TeletextDraftPublicationState::Unrequested,
            can_request: false,
            can_publish: false,
            replaces_pending: false,
        };
        let html = context.with(|| resource_add::form(&data, &observation).to_html());
        for expected in [
            "/admin-api/teletext/design/add-object",
            "POP 2C0 S1=15",
            "GPOP 2C0 S1=15",
            "name=\"resource\"",
            "name=\"object_type\"",
            "name=\"new_object_id\"",
            "value=\"active\"",
            "value=\"adaptive\"",
            "value=\"passive\"",
        ] {
            assert!(html.contains(expected), "missing {expected}: {html}");
        }
        for forbidden in [
            "source_object",
            "name=\"program\"",
            "name=\"sidecar\"",
            "<textarea",
        ] {
            assert!(
                !html.contains(forbidden),
                "unexpected authority: {forbidden}"
            );
        }
        assert!(!html.contains("<fieldset disabled"));
        assert!(!context
            .with(|| object_copy_form(&data, &observation).to_html())
            .contains("<form"));
        data.revision -= 1;
        let html = context.with(|| resource_add::form(&data, &observation).to_html());
        assert!(html.contains("<fieldset disabled"));
        assert!(html.contains(tr(TextKey::TeletextObjectAddCurrentHelp)));
        data.object_resources.clear();
        assert!(!context
            .with(|| resource_add::form(&data, &observation).to_html())
            .contains("<form"));
        assert_eq!(
            draft_status_text("object-add-invalid"),
            tr(TextKey::TeletextObjectAddInvalid)
        );
    }
    fn resource_edit_observation(
        data: &TeletextDraftPreviewData,
    ) -> TeletextDraftPublicationObservation {
        TeletextDraftPublicationObservation {
            digest: "b".repeat(64),
            current_design_head: data.revision,
            current_pending_id: None,
            current_accepted_id: None,
            state: TeletextDraftPublicationState::Unrequested,
            can_request: false,
            can_publish: false,
            replaces_pending: false,
        }
    }

    fn assert_resource_form_fences(html: &str) {
        for expected in [
            "method=\"post\"",
            "name=\"channel_id\" value=\"44\"",
            "name=\"design_id\" value=\"8\"",
            "name=\"revision\" value=\"3\"",
            "name=\"expected_design_head\" value=\"3\"",
            "name=\"digest\"",
            "name=\"name\" required",
            "200 / 0010",
            "201 / 0020",
            "201 / 0030",
        ] {
            assert!(html.contains(expected), "missing {expected}: {html}");
        }
        for forbidden in [
            "name=\"document_index\"",
            "name=\"program\"",
            "name=\"sidecar\"",
            "name=\"authoring\"",
            "name=\"publication\"",
            "<textarea",
            "<script",
        ] {
            assert!(!html.contains(forbidden), "unexpected {forbidden}: {html}");
        }
    }

    #[test]
    fn resource_bank_form_is_localized_explicit_and_available_without_existing_banks_or_metadata() {
        for language in [
            "en-us", "nl", "pl", "pt-br", "fr-fr", "de-de", "es-es", "es-419", "it-it",
        ] {
            let context = Owner::new();
            context.with(|| {
                provide_context(crate::admin::i18n::UiLanguage::from_config(language));
                let mut data = preview();
                assert!(data.object_resources.is_empty() && data.authoring.is_empty());
                let observation = resource_edit_observation(&data);
                let html = resource_banks::form(&data, &observation).to_html();
                assert_resource_form_fences(&html);
                for expected in [
                    "teletext-resource-bank-add-form",
                    "/admin-api/teletext/design/add-resource-bank",
                    "name=\"display\" required",
                    "name=\"resource_kind\" required",
                    "value=\"pop\"",
                    "value=\"gpop\"",
                    "name=\"resource_page\" required",
                    "pattern=\"[1-8][0-9A-Fa-f]{2}\"",
                    "name=\"logical_subpage\" type=\"number\"",
                    "name=\"counter\" type=\"number\"",
                    "name=\"update\" required",
                    "value=\"unchanged\"",
                    "value=\"changed\"",
                    "name=\"last_subpage\" required",
                    "value=\"false\"",
                    "value=\"true\"",
                ] {
                    assert!(html.contains(expected), "missing {expected}: {html}");
                }
                for key in [
                    TextKey::TeletextResourceBankTitle,
                    TextKey::TeletextResourceBankHelp,
                    TextKey::TeletextResourceDisplay,
                    TextKey::TeletextResourceBankKind,
                    TextKey::TeletextResourceBankPage,
                    TextKey::TeletextResourceBankSubpage,
                    TextKey::TeletextResourceBankCounter,
                    TextKey::TeletextResourceBankUpdate,
                    TextKey::TeletextResourceBankUnchanged,
                    TextKey::TeletextResourceBankChanged,
                    TextKey::TeletextResourceBankLastSubpage,
                    TextKey::TeletextResourceBankMoreSubpages,
                    TextKey::TeletextResourceBankFinalSubpage,
                    TextKey::TeletextResourceBankAction,
                ] {
                    assert!(html.contains(tr(key)), "missing localized {key:?}: {html}");
                }
                assert!(!html.contains("<fieldset disabled"));
                assert!(!html.contains("type=\"checkbox\""));
                data.revision -= 1;
                let historical = resource_banks::form(&data, &observation).to_html();
                assert!(historical.contains("<fieldset disabled"));
                assert!(historical.contains(tr(TextKey::TeletextResourceEditCurrentHelp)));
                assert_eq!(
                    draft_status_text("resource-bank-invalid"),
                    tr(TextKey::TeletextResourceBankInvalid)
                );
            });
        }
    }

    #[test]
    fn resource_announcement_form_localizes_multiple_display_selection_and_retains_exact_resources()
    {
        use ersatzrs_domain::{
            TeletextRawDataPageNumber, TeletextResourceIdentity, TeletextResourceKind,
            TeletextResourceSubpageIndex,
        };
        for language in [
            "en-us", "nl", "pl", "pt-br", "fr-fr", "de-de", "es-es", "es-419", "it-it",
        ] {
            let context = Owner::new();
            context.with(|| {
                provide_context(crate::admin::i18n::UiLanguage::from_config(language));
                let mut data = preview();
                let observation = resource_edit_observation(&data);
                assert!(!resource_announcements::form(&data, &observation)
                    .to_html()
                    .contains("<form"));
                data.object_resources = [TeletextResourceKind::Pop, TeletextResourceKind::Gpop]
                    .into_iter()
                    .map(|kind| {
                        TeletextResourceIdentity::new(
                            kind,
                            TeletextRawDataPageNumber::new(2, 0xc0).unwrap(),
                            TeletextResourceSubpageIndex::new(15).unwrap(),
                        )
                        .unwrap()
                    })
                    .collect();
                let html = resource_announcements::form(&data, &observation).to_html();
                assert_resource_form_fences(&html);
                for expected in [
                    "teletext-object-subpages-announce-form",
                    "/admin-api/teletext/design/announce-object-subpages",
                    "name=\"resource\" required",
                    "name=\"display\" multiple required",
                    "size=\"5\"",
                    "aria-describedby=\"teletext-object-subpages-help\"",
                    "POP 2C0 S1=15",
                    "GPOP 2C0 S1=15",
                ] {
                    assert!(html.contains(expected), "missing {expected}: {html}");
                }
                for key in [
                    TextKey::TeletextSubpageAnnouncementTitle,
                    TextKey::TeletextSubpageAnnouncementHelp,
                    TextKey::TeletextSubpageAnnouncementDisplays,
                    TextKey::TeletextSubpageAnnouncementAction,
                ] {
                    assert!(html.contains(tr(key)), "missing localized {key:?}: {html}");
                }
                for display in &data.pages {
                    let value = serde_json::to_string(display)
                        .unwrap()
                        .replace('"', "&quot;");
                    assert!(
                        html.contains(&format!("value=\"{value}\"")),
                        "lost display {display:?}: {html}"
                    );
                }
                assert!(!html.contains("<fieldset disabled"));
                data.revision -= 1;
                let historical = resource_announcements::form(&data, &observation).to_html();
                assert!(historical.contains("<fieldset disabled"));
                assert!(historical.contains(tr(TextKey::TeletextResourceEditCurrentHelp)));
                assert_eq!(
                    draft_status_text("subpage-announcement-invalid"),
                    tr(TextKey::TeletextSubpageAnnouncementInvalid)
                );
            });
        }
    }
    fn resource_selection_fixture() -> TeletextDraftPreviewData {
        use crate::admin::server_fns::teletext_drafts::{
            TeletextObjectCallChoiceObservation, TeletextObjectDefinitionChoiceObservation,
            TeletextObjectPageChoiceObservation,
        };
        use ersatzrs_domain::{
            TeletextObjectId, TeletextObjectIdentity, TeletextObjectOffset, TeletextObjectType,
            TeletextRawDataPageNumber, TeletextResourceIdentity, TeletextResourceKind,
            TeletextResourceSubpageIndex, TeletextSubpageRequirementMask,
        };
        let resource = |kind, raw, subpage| {
            TeletextResourceIdentity::new(
                kind,
                TeletextRawDataPageNumber::new(2, raw).unwrap(),
                TeletextResourceSubpageIndex::new(subpage).unwrap(),
            )
            .unwrap()
        };
        let pop = resource(TeletextResourceKind::Pop, 0xc0, 0);
        let gpop = resource(TeletextResourceKind::Gpop, 0xd0, 0);
        let alternate = resource(TeletextResourceKind::Gpop, 0xe1, 0);
        let caller = TeletextObjectIdentity::global(gpop, TeletextObjectId::new(20)).unwrap();
        let passive = TeletextObjectIdentity::public(pop, TeletextObjectId::new(21)).unwrap();
        let other = TeletextObjectIdentity::global(alternate, TeletextObjectId::new(32)).unwrap();
        let local = TeletextObjectIdentity::local(owner(200, 0x10), TeletextObjectId::new(99));
        let mut data = preview();
        data.object_resources = vec![
            pop,
            resource(TeletextResourceKind::Pop, 0xc0, 15),
            gpop,
            alternate,
        ];
        data.object_choices.pages = vec![
            TeletextObjectPageChoiceObservation {
                display: owner(200, 0x10),
                role: TeletextResourceKind::Pop,
                target: pop.page(),
                required_subpages: TeletextSubpageRequirementMask::new(0x8001),
            },
            TeletextObjectPageChoiceObservation {
                display: owner(201, 0x20),
                role: TeletextResourceKind::Gpop,
                target: gpop.page(),
                required_subpages: TeletextSubpageRequirementMask::new(1),
            },
        ];
        data.object_choices.calls = vec![
            TeletextObjectCallChoiceObservation {
                caller,
                command_index: 4,
                target: passive,
                object_type: TeletextObjectType::Passive,
                offset: Some(TeletextObjectOffset::new(0, 1).unwrap()),
            },
            TeletextObjectCallChoiceObservation {
                caller,
                command_index: 6,
                target: other,
                object_type: TeletextObjectType::Passive,
                offset: None,
            },
        ];
        data.object_choices.definitions = vec![
            TeletextObjectDefinitionChoiceObservation {
                identity: passive,
                object_type: TeletextObjectType::Passive,
            },
            TeletextObjectDefinitionChoiceObservation {
                identity: other,
                object_type: TeletextObjectType::Passive,
            },
            TeletextObjectDefinitionChoiceObservation {
                identity: caller,
                object_type: TeletextObjectType::Active,
            },
            TeletextObjectDefinitionChoiceObservation {
                identity: local,
                object_type: TeletextObjectType::Passive,
            },
        ];
        add_resource_selection_direct_consumers(&mut data);
        data
    }

    fn add_resource_selection_direct_consumers(data: &mut TeletextDraftPreviewData) {
        use crate::admin::server_fns::teletext_drafts::{
            TeletextMainCallChoiceObservation, TeletextObjectCallChoiceObservation,
        };
        use ersatzrs_domain::{TeletextObjectOffset, TeletextObjectType};
        let local = data.object_choices.definitions[3].identity;
        let passive = data.object_choices.calls[0].target;
        data.object_choices
            .calls
            .push(TeletextObjectCallChoiceObservation {
                caller: local,
                command_index: 7,
                target: passive,
                object_type: TeletextObjectType::Passive,
                offset: Some(TeletextObjectOffset::new(1, 2).unwrap()),
            });
        data.object_choices.main_calls = vec![TeletextMainCallChoiceObservation {
            owner: owner(200, 0x10),
            command_index: 3,
            target: passive,
            object_type: TeletextObjectType::Passive,
            offset: None,
        }];
    }

    fn assert_resource_selection_fields(html: &str) {
        for expected in [
            "teletext-object-pages-choose-form",
            "/admin-api/teletext/design/choose-object-pages",
            "method=\"post\"",
            "name=\"channel_id\" value=\"44\"",
            "name=\"expected_design_head\" value=\"3\"",
            "name=\"page_0_display\"",
            "name=\"page_0_role\" value=\"pop\"",
            "name=\"page_0_target\" required",
            "name=\"page_0_mask\" required",
            "value=\"8001\"",
            "value=\"0001\"",
            "pattern=\"[0-9A-Fa-f]{1,4}\"",
            "name=\"page_1_role\" value=\"gpop\"",
            "name=\"call_0_caller\"",
            "name=\"call_0_command\" value=\"4\"",
            "name=\"call_0_target\" required",
            "name=\"call_1_command\" value=\"6\"",
            "name=\"call_2_command\" value=\"7\"",
            "LOCAL 200 / 0010 #99",
            "name=\"main_0_owner\"",
            "name=\"main_0_command\" value=\"3\"",
            "name=\"main_0_target\" required",
            "id=\"teletext-object-main-0-target\"",
            "200 / 0010",
            "201 / 0020",
            "0, 1",
        ] {
            assert!(html.contains(expected), "missing {expected}: {html}");
        }
        for forbidden in [
            "name=\"document_index\"",
            "name=\"program\"",
            "name=\"authoring\"",
            "name=\"sidecar\"",
            "<textarea",
            "<script",
        ] {
            assert!(!html.contains(forbidden), "unexpected {forbidden}");
        }
        let value = serde_json::to_string(&owner(200, 0x10))
            .unwrap()
            .replace('"', "&quot;");
        assert!(html.contains(&format!("name=\"main_0_owner\" value=\"{value}\"")));
    }

    fn assert_resource_selection_labels(html: &str) {
        for key in [
            TextKey::TeletextObjectSelectionTitle,
            TextKey::TeletextObjectSelectionHelp,
            TextKey::TeletextObjectSelectionPageTarget,
            TextKey::TeletextObjectSelectionMask,
            TextKey::TeletextObjectSelectionMaskHelp,
            TextKey::TeletextObjectSelectionCalls,
            TextKey::TeletextObjectSelectionMainCalls,
            TextKey::TeletextObjectSelectionCaller,
            TextKey::TeletextObjectSelectionCommand,
            TextKey::TeletextObjectSelectionCurrentTarget,
            TextKey::TeletextObjectSelectionOffset,
            TextKey::TeletextObjectSelectionNoOffset,
            TextKey::TeletextObjectSelectionCallTarget,
            TextKey::TeletextObjectSelectionAction,
            TextKey::TeletextObjectTypePassive,
        ] {
            assert!(html.contains(tr(key)), "missing localized {key:?}: {html}");
        }
    }

    fn assert_resource_selection_catalogue(html: &str) {
        let page_lists: [(&str, &[&str]); 2] = [("pop", &["2C0"]), ("gpop", &["2D0", "2E1"])];
        for (kind, values) in page_lists {
            let id = format!("<datalist id=\"teletext-object-pages-{kind}\">");
            assert_eq!(html.matches(&id).count(), 1);
            let list = html
                .split_once(&id)
                .unwrap()
                .1
                .split_once("</datalist>")
                .unwrap()
                .0;
            assert_eq!(list.matches("<option ").count(), values.len());
            for value in values {
                let option = format!("<option value=\"{value}\"");
                assert_eq!(
                    html.matches(&option).count(),
                    1,
                    "raw page catalogue deduplicates S1 owners"
                );
                assert!(list.contains(&option));
            }
        }
        for (page, kind) in [("2C0", "pop"), ("2D0", "gpop")] {
            assert!(
                html.contains(&format!(
                    "value=\"{page}\" list=\"teletext-object-pages-{kind}\""
                )),
                "current page retains its matching shared list"
            );
        }
        assert!(
            !html.contains("<select id=\"teletext-object-page-"),
            "role inputs reuse shared raw page lists"
        );
        assert_eq!(html.matches("<datalist ").count(), 5);
        let list = |kind: &str| {
            let id = format!("<datalist id=\"teletext-object-targets-{kind}\">");
            html.split_once(&id)
                .unwrap()
                .1
                .split_once("</datalist>")
                .unwrap()
                .0
        };
        for kind in ["active", "adaptive", "passive"] {
            assert!(
                !list(kind).contains("#99"),
                "Local identities are callers, never resource targets"
            );
        }
        assert!(list("active").contains("value=\"GPOP 2D0 S1=0 #20\""));
        assert!(
            !list("passive").contains("#20"),
            "Active is not a Passive target"
        );
        assert!(!list("adaptive").contains("<option"));
        for label in ["POP 2C0 S1=0 #21", "GPOP 2E1 S1=0 #32"] {
            let value = format!("<option value=\"{label}\"");
            assert_eq!(
                html.matches(&value).count(),
                1,
                "shared catalogue contains each target once"
            );
            assert!(list("passive").contains(&value));
            assert!(
                html.contains(&format!(
                    "value=\"{label}\" list=\"teletext-object-targets-passive\""
                )),
                "current input retains human target and matching list"
            );
        }
        assert!(
            !html.contains("<select id=\"teletext-object-call-"),
            "call inputs reuse shared lists"
        );
    }

    fn assert_resource_selection_history(
        mut data: TeletextDraftPreviewData,
        observation: &TeletextDraftPublicationObservation,
    ) {
        data.object_choices.pages.clear();
        let call_only = resource_selection::form(&data, observation).to_html();
        assert!(call_only.contains("teletext-object-pages-choose-form"));
        assert!(call_only.contains("name=\"call_0_target\""));
        assert!(!call_only.contains("name=\"page_0_target\""));
        data.object_choices.calls.clear();
        let main_only = resource_selection::form(&data, observation).to_html();
        assert!(main_only.contains("teletext-object-pages-choose-form"));
        assert!(main_only.contains("name=\"main_0_target\""));
        assert!(!main_only.contains("name=\"call_0_target\""));
        data.revision -= 1;
        let historical = resource_selection::form(&data, observation).to_html();
        assert!(historical.contains("<fieldset disabled"));
        assert!(historical.contains(tr(TextKey::TeletextResourceEditCurrentHelp)));
        assert_eq!(
            draft_status_text("object-selection-invalid"),
            tr(TextKey::TeletextObjectSelectionInvalid)
        );
    }

    fn drcs_resource_selection_fixture() -> TeletextDraftPreviewData {
        use crate::admin::server_fns::teletext_drafts::TeletextObjectPageChoiceObservation;
        use ersatzrs_domain::{
            TeletextRawDataPageNumber, TeletextResourceIdentity, TeletextResourceKind,
            TeletextResourceSubpageIndex, TeletextSubpageRequirementMask,
        };
        let resource = |kind, raw| {
            TeletextResourceIdentity::new(
                kind,
                TeletextRawDataPageNumber::new(2, raw).unwrap(),
                TeletextResourceSubpageIndex::new(0).unwrap(),
            )
            .unwrap()
        };
        let mut data = preview();
        for (role, old, new) in [
            (TeletextResourceKind::Drcs, 0xc0, 0xc2),
            (TeletextResourceKind::Gdrcs, 0xd0, 0xd2),
        ] {
            let original = resource(role, old);
            data.object_resources
                .extend([original, resource(role, new)]);
            data.object_choices
                .pages
                .push(TeletextObjectPageChoiceObservation {
                    display: owner(200, 0x10),
                    role,
                    target: original.page(),
                    required_subpages: TeletextSubpageRequirementMask::new(1),
                });
        }
        data
    }

    #[test]
    fn resource_selection_drcs_roles_localize_and_keep_object_actions_separate() {
        for language in [
            "en-us", "nl", "pl", "pt-br", "fr-fr", "de-de", "es-es", "es-419", "it-it",
        ] {
            let context = Owner::new();
            context.with(|| {
                provide_context(crate::admin::i18n::UiLanguage::from_config(language));
                let mut data = drcs_resource_selection_fixture();
                let observation = resource_edit_observation(&data);
                let html = resource_selection::form(&data, &observation).to_html();
                assert!(html.contains(tr(TextKey::TeletextObjectSelectionTitle)));
                assert!(html.contains("DRCS/GDRCS"));
                for (index, kind, current, alternate) in
                    [(0, "drcs", "2C0", "2C2"), (1, "gdrcs", "2D0", "2D2")]
                {
                    assert!(html.contains(&format!("name=\"page_{index}_role\" value=\"{kind}\"")));
                    assert!(html.contains(&format!(
                        "value=\"{current}\" list=\"teletext-object-pages-{kind}\""
                    )));
                    let list = html
                        .split_once(&format!("<datalist id=\"teletext-object-pages-{kind}\">"))
                        .unwrap()
                        .1
                        .split_once("</datalist>")
                        .unwrap()
                        .0;
                    assert_eq!(list.matches("<option ").count(), 2);
                    assert!(list.contains(&format!("value=\"{alternate}\"")));
                }
                for forbidden in [
                    "page_0_program",
                    "page_0_expected",
                    "name=\"document_index\"",
                    "name=\"sidecar\"",
                    "teletext-object-pages-pop",
                    "teletext-object-pages-gpop",
                ] {
                    assert!(!html.contains(forbidden), "{forbidden}");
                }
                assert!(!resource_add::form(&data, &observation)
                    .to_html()
                    .contains("<form"));
                assert!(!resource_announcements::form(&data, &observation)
                    .to_html()
                    .contains("<form"));
                assert!(!html.contains("<fieldset disabled"));
                data.revision -= 1;
                let history = resource_selection::form(&data, &observation).to_html();
                assert!(history.contains("<fieldset disabled"));
                assert!(history.contains(tr(TextKey::TeletextResourceEditCurrentHelp)));
            });
        }
    }

    #[test]
    fn resource_selection_form_localizes_exact_pages_masks_and_explicit_typed_calls() {
        for language in [
            "en-us", "nl", "pl", "pt-br", "fr-fr", "de-de", "es-es", "es-419", "it-it",
        ] {
            let context = Owner::new();
            context.with(|| {
                provide_context(crate::admin::i18n::UiLanguage::from_config(language));
                let data = preview();
                let observation = resource_edit_observation(&data);
                assert!(!resource_selection::form(&data, &observation)
                    .to_html()
                    .contains("<form"));
                let data = resource_selection_fixture();
                let html = resource_selection::form(&data, &observation).to_html();
                assert_resource_selection_fields(&html);
                assert_resource_selection_labels(&html);
                assert_resource_selection_catalogue(&html);
                assert!(!html.contains("<fieldset disabled"));
                assert_resource_selection_history(data, &observation);
            });
        }
    }
}
