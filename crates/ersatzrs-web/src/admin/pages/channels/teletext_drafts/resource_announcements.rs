//! Explicit selected-owner announcements never grant ordinary TTI authority.

use ersatzrs_domain::TeletextResourceKind;
use leptos::prelude::*;

use super::{
    btn_class, editorial_export_fields, resource_banks, style, tr, ButtonVariant,
    TeletextDraftPreviewData, TeletextDraftPublicationObservation, TextKey,
};

pub(super) fn form(
    data: &TeletextDraftPreviewData,
    observation: &TeletextDraftPublicationObservation,
) -> AnyView {
    if !data.object_resources.iter().any(|resource| {
        matches!(
            resource.kind(),
            TeletextResourceKind::Pop | TeletextResourceKind::Gpop
        )
    }) {
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
        <form method="post" action="/admin-api/teletext/design/announce-object-subpages"
            class=style::preview_form data-testid="teletext-object-subpages-announce-form"
            aria-describedby="teletext-object-subpages-help">
            {editorial_export_fields(data, observation)}
            <fieldset disabled=!current>
                <legend>{tr(TextKey::TeletextSubpageAnnouncementTitle)}</legend>
                <p id="teletext-object-subpages-help">{tr(TextKey::TeletextSubpageAnnouncementHelp)}</p>
                <label for="teletext-object-subpages-name">{tr(TextKey::TeletextDesignName)}</label>
                <input id="teletext-object-subpages-name" name="name" required
                    maxlength=ersatzrs_domain::MAX_TELETEXT_DESIGN_NAME_BYTES/>
                <label for="teletext-object-subpages-resource">{tr(TextKey::TeletextObjectAddResource)}</label>
                <select id="teletext-object-subpages-resource" name="resource" required>
                    <option value="" selected disabled>{tr(TextKey::TeletextObjectAddResource)}</option>
                    {resources}
                </select>
                <label for="teletext-object-subpages-displays">{tr(TextKey::TeletextSubpageAnnouncementDisplays)}</label>
                <select id="teletext-object-subpages-displays" name="display" multiple required size="5"
                    aria-describedby="teletext-object-subpages-help">
                    {resource_banks::display_options(data)}
                </select>
                <button type="submit" class=btn_class(ButtonVariant::Primary)>
                    {tr(TextKey::TeletextSubpageAnnouncementAction)}
                </button>
            </fieldset>
            <p style:display=if current { "none" } else { "" }>
                {tr(TextKey::TeletextResourceEditCurrentHelp)}
            </p>
        </form>
    }.into_any()
}
