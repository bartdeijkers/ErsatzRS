//! Shared guide template catalogue, derived from the enabled installation.
use crate::admin::{
    i18n::{tr, TextKey},
    server_fns::teletext_atlas::{load_guide_library, GuideLibrary, COLOURS},
    ui::primitives::button::{btn_class, ButtonVariant},
};
use leptos::prelude::*;

#[component]
pub(crate) fn GuideTheme(#[prop(optional)] feedback: String) -> impl IntoView {
    let library = Resource::new_blocking(|| (), |()| load_guide_library());
    view! {<Suspense>{move ||library.get().map(|result| match result{
        Ok(Some(lib))=>editor(lib,&feedback),
        Ok(None)=>().into_any(),
        Err(_)=>view!{<p role="alert">{tr(TextKey::TeletextAdminUnavailable)}</p>}.into_any()
    })}</Suspense>}
}
pub(crate) fn editor(library: GuideLibrary, feedback: &str) -> AnyView {
    let theme = library.saved.unwrap_or(library.defaults);
    let elements = library.elements;
    let swatches = |selected: String| {
        COLOURS
            .into_iter()
            .map(|value| {
                let key = match value {
                    "red" => TextKey::TeletextNavRed,
                    "green" => TextKey::TeletextNavGreen,
                    "yellow" => TextKey::TeletextNavYellow,
                    "blue" => TextKey::AtlasBlue,
                    "cyan" => TextKey::TeletextNavCyan,
                    "magenta" => TextKey::AtlasMagenta,
                    _ => TextKey::AtlasWhite,
                };
                view! {<option value=value selected=value==selected>{tr(key)}</option>}
            })
            .collect_view()
    };
    view!{
    <section class="atlas-theme" id="guide-theme">
        <h2>{tr(TextKey::AtlasTheme)}</h2>
        <form action="/admin-api/teletext/atlas/theme" method="post" data-guide-theme>
            <div><label>{tr(TextKey::AtlasBanner)}<input name="banner" value=theme.banner.clone() maxlength="36" required/></label>
            <label>{tr(TextKey::AtlasBannerColour)}<select name="accent">{swatches(theme.accent.clone())}</select></label>
            <label>{tr(TextKey::AtlasHeadingColour)}<select name="heading">{swatches(theme.heading.clone())}</select></label></div>
            <figure aria-label=tr(TextKey::Preview)><strong style:color=theme.accent data-banner>{theme.banner}</strong>
            <span style:color=theme.heading data-heading>{tr(TextKey::AtlasGuideIndex)}</span></figure>
            <footer><button class=btn_class(ButtonVariant::Primary)>{tr(TextKey::Save)}</button>
            <span role="status" data-saved=tr(TextKey::TeletextAdminSaved) data-error=tr(TextKey::TeletextAuthoringRejected)>{match feedback {"saved"=>tr(TextKey::TeletextAdminSaved),"error"=>tr(TextKey::TeletextAuthoringRejected),_=>""}}</span></footer>
        </form>
        <details><summary>{tr(TextKey::TeletextElements)}</summary>
            <div class="atlas-catalog"><section><h3>{tr(TextKey::ProgrammeGuide)}</h3><ul>
                <li>{tr(TextKey::AtlasGuideIndex)}</li><li>{tr(TextKey::AtlasGuidePage)}</li>
            </ul></section><section><h3>{tr(TextKey::TeletextCustomPagesTitle)}</h3>
            {if elements.is_empty(){view!{<a href="/teletext-elements/add">{tr(TextKey::UploadTeletextPage)}</a>}.into_any()}
                else{view!{<ul>{elements.into_iter().map(|(id,name)|view!{<li><a href=format!("/teletext-elements/{id}")>{name}</a></li>}).collect_view()}</ul>}.into_any()}}
            </section></div></details>
        <link rel="stylesheet" href="/public/teletext-atlas.css"/>

    </section>}.into_any()
}
