//! Explicit author consent, populated from the exact historical preview.
use super::{
    btn_class, editorial_export_fields, hidden, style, tr, AnyView, ButtonVariant, IntoAny,
    TeletextDraftPreviewData, TeletextDraftPublicationObservation, TextKey,
};
use leptos::prelude::*;

pub(super) fn forms(
    data: &TeletextDraftPreviewData,
    observation: &TeletextDraftPublicationObservation,
) -> AnyView {
    if !data
        .authoring
        .iter()
        .any(|source| source.secondary_default_context)
    {
        return ().into_any();
    }
    let current = data.revision == observation.current_design_head;
    data.authoring.iter().filter(|source| source.secondary_default_context).map(|source| {
        let prefix = format!("teletext-secondary-default-{}", source.producer_index);
        let (choice, designation) = match source.secondary_default_agreement {
            None => ("original", String::new()),
            Some(agreement) => agreement.secondary().map_or(
                ("no_second", String::new()),
                |code| ("designated", format!("{:02X}", code.get())),
            ),
        };
        let name_id = format!("{prefix}-name");
        let choice_id = format!("{prefix}-choice");
        let designation_id = format!("{prefix}-designation");
        let help_id = format!("{prefix}-help");
        view! {
            <form method="post" action="/admin-api/teletext/design/choose-secondary-default"
                class=style::preview_form data-testid="teletext-secondary-default-form"
                aria-describedby=help_id.clone()>
                {editorial_export_fields(data, observation)}
                {hidden("producer", source.producer_index.to_string())}
                <fieldset disabled=!current>
                    <legend>{tr(TextKey::TeletextSecondaryDefaultTitle)}" · "{source.addon_id.clone()}</legend>
                    <p id=help_id.clone()>{tr(TextKey::TeletextSecondaryDefaultHelp)}</p>
                    <label for=name_id.clone()>{tr(TextKey::TeletextDesignName)}</label>
                    <input id=name_id name="name" required
                        maxlength=ersatzrs_domain::MAX_TELETEXT_DESIGN_NAME_BYTES/>
                    <label for=choice_id.clone()>{tr(TextKey::TeletextSecondaryDefaultChoice)}</label>
                    <select id=choice_id name="choice" required>
                        <option value="original" selected=choice=="original">{tr(TextKey::TeletextSecondaryDefaultOriginal)}</option>
                        <option value="no_second" selected=choice=="no_second">{tr(TextKey::TeletextSecondaryDefaultNone)}</option>
                        <option value="designated" selected=choice=="designated">{tr(TextKey::TeletextSecondaryDefaultDesignated)}</option>
                    </select>
                    <label for=designation_id.clone()>{tr(TextKey::TeletextSecondaryDefaultCode)}</label>
                    <input id=designation_id name="designation" value=designation
                        maxlength="2" pattern="[0-7][0-9A-Fa-f]" placeholder="00"/>
                    <button type="submit" class=btn_class(ButtonVariant::Primary)>
                        {tr(TextKey::TeletextSecondaryDefaultSave)}
                    </button>
                </fieldset>
                <p hidden=current>{tr(TextKey::TeletextSecondaryDefaultCurrentHelp)}</p>
            </form>
        }
    }).collect_view().into_any()
}
