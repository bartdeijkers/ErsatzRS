//! Explicit first-definition intent uses the existing native receipt/CAS flow.

use ersatzrs_domain::TeletextResourceKind;
use leptos::prelude::*;

use super::{
    btn_class, editorial_export_fields, style, tr, ButtonVariant, TeletextDraftPreviewData,
    TeletextDraftPublicationObservation, TextKey,
};

pub(super) fn form(
    data: &TeletextDraftPreviewData,
    observation: &TeletextDraftPublicationObservation,
) -> AnyView {
    if data.object_resources.is_empty() {
        return ().into_any();
    }
    let current = data.revision == observation.current_design_head;
    let resources = data
        .object_resources
        .iter()
        .filter_map(|resource| {
            let kind = match resource.kind() {
                TeletextResourceKind::Pop => "POP",
                TeletextResourceKind::Gpop => "GPOP",
                _ => return None,
            };
            let value = serde_json::to_string(resource).ok()?;
            let label = format!(
                "{kind} {:X}{:02X} S1={}",
                resource.page().magazine(),
                resource.page().page(),
                resource.logical_subpage().get(),
            );
            Some(view! { <option value=value>{label}</option> })
        })
        .collect_view();
    view! {
        <form method="post" action="/admin-api/teletext/design/add-object"
            class=style::preview_form data-testid="teletext-object-add-form"
            aria-describedby="teletext-object-add-help">
            {editorial_export_fields(data, observation)}
            <fieldset disabled=!current>
                <legend>{tr(TextKey::TeletextObjectAddTitle)}</legend>
                <p id="teletext-object-add-help">{tr(TextKey::TeletextObjectAddHelp)}</p>
                <label for="teletext-object-add-name">{tr(TextKey::TeletextDesignName)}</label>
                <input id="teletext-object-add-name" name="name" required
                    maxlength=ersatzrs_domain::MAX_TELETEXT_DESIGN_NAME_BYTES/>
                <label for="teletext-object-add-resource">{tr(TextKey::TeletextObjectAddResource)}</label>
                <select id="teletext-object-add-resource" name="resource" required>
                    <option value="" selected disabled>{tr(TextKey::TeletextObjectAddResource)}</option>
                    {resources}
                </select>
                <label for="teletext-object-add-type">{tr(TextKey::TeletextObjectAddType)}</label>
                <select id="teletext-object-add-type" name="object_type" required>
                    <option value="" selected disabled>{tr(TextKey::TeletextObjectAddType)}</option>
                    <option value="active">{tr(TextKey::TeletextObjectTypeActive)}</option>
                    <option value="adaptive">{tr(TextKey::TeletextObjectTypeAdaptive)}</option>
                    <option value="passive">{tr(TextKey::TeletextObjectTypePassive)}</option>
                </select>
                <label for="teletext-object-add-number">{tr(TextKey::TeletextObjectCopyNumber)}</label>
                <input id="teletext-object-add-number" name="new_object_id" type="number"
                    min="0" max="65535" step="1" required/>
                <button type="submit" class=btn_class(ButtonVariant::Primary)>
                    {tr(TextKey::TeletextObjectAddAction)}
                </button>
            </fieldset>
            <p style:display=if current { "none" } else { "" }>
                {tr(TextKey::TeletextObjectAddCurrentHelp)}
            </p>
        </form>
    }.into_any()
}
