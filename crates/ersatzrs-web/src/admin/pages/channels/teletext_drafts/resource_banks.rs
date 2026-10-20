//! Explicit retained-bank creation remains an unpublished, current-head action.

use leptos::prelude::*;

use super::{
    btn_class, editorial_export_fields, style, tr, ButtonVariant, TeletextDraftPreviewData,
    TeletextDraftPublicationObservation, TextKey,
};

pub(super) fn display_options(data: &TeletextDraftPreviewData) -> impl IntoView {
    data.pages
        .iter()
        .filter_map(|display| {
            let value = serde_json::to_string(display).ok()?;
            let label = format!("{} / {:04X}", display.page.get(), display.subcode.get());
            Some(view! { <option value=value>{label}</option> })
        })
        .collect_view()
}

pub(super) fn form(
    data: &TeletextDraftPreviewData,
    observation: &TeletextDraftPublicationObservation,
) -> AnyView {
    let current = data.revision == observation.current_design_head;
    view! {
        <form method="post" action="/admin-api/teletext/design/add-resource-bank"
            class=style::preview_form data-testid="teletext-resource-bank-add-form"
            aria-describedby="teletext-resource-bank-help">
            {editorial_export_fields(data, observation)}
            <fieldset disabled=!current>
                <legend>{tr(TextKey::TeletextResourceBankTitle)}</legend>
                <p id="teletext-resource-bank-help">{tr(TextKey::TeletextResourceBankHelp)}</p>
                <label for="teletext-resource-bank-name">{tr(TextKey::TeletextDesignName)}</label>
                <input id="teletext-resource-bank-name" name="name" required
                    maxlength=ersatzrs_domain::MAX_TELETEXT_DESIGN_NAME_BYTES/>
                <label for="teletext-resource-bank-display">{tr(TextKey::TeletextResourceDisplay)}</label>
                <select id="teletext-resource-bank-display" name="display" required>
                    <option value="" selected disabled>{tr(TextKey::TeletextResourceDisplay)}</option>
                    {display_options(data)}
                </select>
                <label for="teletext-resource-bank-kind">{tr(TextKey::TeletextResourceBankKind)}</label>
                <select id="teletext-resource-bank-kind" name="resource_kind" required>
                    <option value="" selected disabled>{tr(TextKey::TeletextResourceBankKind)}</option>
                    <option value="pop">"POP"</option><option value="gpop">"GPOP"</option>
                </select>
                <label for="teletext-resource-bank-page">{tr(TextKey::TeletextResourceBankPage)}</label>
                <input id="teletext-resource-bank-page" name="resource_page" required
                    pattern="[1-8][0-9A-Fa-f]{2}" minlength="3" maxlength="3"/>
                <label for="teletext-resource-bank-subpage">{tr(TextKey::TeletextResourceBankSubpage)}</label>
                <input id="teletext-resource-bank-subpage" name="logical_subpage" type="number"
                    min="0" max="15" step="1" required/>
                <label for="teletext-resource-bank-counter">{tr(TextKey::TeletextResourceBankCounter)}</label>
                <input id="teletext-resource-bank-counter" name="counter" type="number"
                    min="0" max="7" step="1" required/>
                <label for="teletext-resource-bank-update">{tr(TextKey::TeletextResourceBankUpdate)}</label>
                <select id="teletext-resource-bank-update" name="update" required>
                    <option value="" selected disabled>{tr(TextKey::TeletextResourceBankUpdate)}</option>
                    <option value="unchanged">{tr(TextKey::TeletextResourceBankUnchanged)}</option>
                    <option value="changed">{tr(TextKey::TeletextResourceBankChanged)}</option>
                </select>
                <label for="teletext-resource-bank-last">{tr(TextKey::TeletextResourceBankLastSubpage)}</label>
                <select id="teletext-resource-bank-last" name="last_subpage" required>
                    <option value="" selected disabled>{tr(TextKey::TeletextResourceBankLastSubpage)}</option>
                    <option value="false">{tr(TextKey::TeletextResourceBankMoreSubpages)}</option>
                    <option value="true">{tr(TextKey::TeletextResourceBankFinalSubpage)}</option>
                </select>
                <button type="submit" class=btn_class(ButtonVariant::Primary)>
                    {tr(TextKey::TeletextResourceBankAction)}
                </button>
            </fieldset>
            <p style:display=if current { "none" } else { "" }>
                {tr(TextKey::TeletextResourceEditCurrentHelp)}
            </p>
        </form>
    }.into_any()
}
