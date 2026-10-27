//! Server-populated unused-copy removal with the existing native form fallback.
use super::{
    btn_class, editorial_export_fields, style, tr, AnyView, ButtonVariant, IntoAny,
    TeletextDraftPreviewData, TeletextDraftPublicationObservation, TextKey,
};
use leptos::prelude::*;

pub(super) fn form(
    data: &TeletextDraftPreviewData,
    observation: &TeletextDraftPublicationObservation,
) -> AnyView {
    if data.removable_resource_objects.is_empty() {
        return ().into_any();
    }
    let current = data.revision == observation.current_design_head;
    let options = data
        .removable_resource_objects
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
        <form method="post" action="/admin-api/teletext/design/remove-unused-object"
            class=style::preview_form data-testid="teletext-object-remove-form"
            aria-describedby="teletext-object-remove-help">
            {editorial_export_fields(data, observation)}
            <fieldset disabled=!current>
                <legend>{tr(TextKey::TeletextObjectRemoveTitle)}</legend>
                <p id="teletext-object-remove-help">{tr(TextKey::TeletextObjectRemoveHelp)}</p>
                <label for="teletext-object-remove-name">{tr(TextKey::TeletextDesignName)}</label>
                <input id="teletext-object-remove-name" name="name" required
                    maxlength=ersatzrs_domain::MAX_TELETEXT_DESIGN_NAME_BYTES/>
                <label for="teletext-object-remove-target">{tr(TextKey::TeletextObjectRemoveTarget)}</label>
                <select id="teletext-object-remove-target" name="object" required>{options}</select>
                <button type="submit" class=btn_class(ButtonVariant::Danger)>
                    {tr(TextKey::TeletextObjectRemoveAction)}
                </button>
            </fieldset>
            <p style:display=if current { "none" } else { "" }>
                {tr(TextKey::TeletextObjectRemoveCurrentHelp)}
            </p>
        </form>
    }.into_any()
}
