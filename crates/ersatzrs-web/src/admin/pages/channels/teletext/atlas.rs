//! Atlas channel editor. The same form spans Pages and Sources.
use super::{
    admin_loading_resource_view, admin_result_view, btn_class, hidden, load_channel_teletext, tr,
    ButtonVariant, ChannelTeletextData, TextKey,
};
use crate::admin::server_fns::teletext::{effective_setting, GUIDE_ADDON};
use crate::admin::server_fns::teletext_atlas::{
    assignments, element_pages, load_atlas_edition, AtlasEdition, AtlasPage, AtlasPicture,
};
use crate::admin::server_fns::teletext_drafts::load_atlas_guide_preview;
use crate::admin::ui::primitives::settings_navigation::style as tabs;
use ersatzrs_domain::TeletextPageAddress;
use leptos::prelude::*;
use leptos_router::hooks::use_query_map;
use serde_json::json;

const COLOURS: [(&str, TextKey); 4] = [
    ("red", TextKey::TeletextNavRed),
    ("green", TextKey::TeletextNavGreen),
    ("yellow", TextKey::TeletextNavYellow),
    ("blue", TextKey::AtlasBlue),
];

#[component]
pub(super) fn Atlas(channel_id: i64) -> impl IntoView {
    let query = use_query_map();
    let sources = query.with_untracked(|q| q.get("tab").as_deref() == Some("sources"));
    let page = query.with_untracked(|q| {
        q.get("page")
            .and_then(|s| s.parse::<u16>().ok())
            .and_then(|n| n.try_into().ok())
    });
    let subcode = query.with_untracked(|q| {
        q.get("subcode")
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(0)
    });
    let owner = page
        .zip(ersatzrs_domain::TeletextSubcode::new(subcode).ok())
        .map(|(page, subcode)| TeletextPageAddress { page, subcode });
    let error = query.with_untracked(|q| {
        q.get("atlas_error").unwrap_or_else(|| {
            if q.get("guide").as_deref() == Some("off") {
                "saved-off".into()
            } else {
                String::new()
            }
        })
    });
    let data = Resource::new_blocking(
        move || (channel_id, owner),
        |(id, owner)| async move {
            let data = load_channel_teletext(id)
                .await?
                .ok_or_else(|| ServerFnError::new("not-found"))?;
            let edition = load_atlas_edition(id).await?;
            let initial_owner = owner.or_else(|| {
                let mut pages = assignments(&data, &edition);
                let guide_on = edition.guide_on.unwrap_or_else(|| {
                    data.addons
                        .iter()
                        .any(|a| a.addon_id == GUIDE_ADDON && super::source_on(a))
                });
                if !guide_on {
                    pages.clear();
                }
                pages.extend(element_pages(&data));
                let first = pages.iter().min_by_key(|p| p.page)?;
                let count = first.element.as_ref().and_then(|id| {
                    data.custom_pages
                        .as_ref()?
                        .library
                        .iter()
                        .find(|e| e.id.to_string() == *id)
                        .map(|e| {
                            if first.subpages.is_empty() {
                                usize::from(e.subpages)
                            } else {
                                first.subpages.len()
                            }
                        })
                });
                Some(TeletextPageAddress {
                    page: first.page,
                    subcode: ersatzrs_domain::TeletextSubcode::new(u16::from(
                        count.is_some_and(|n| n > 1),
                    ))
                    .ok()?,
                })
            });
            let image = initial_picture(
                id,
                initial_owner,
                &edition,
                data.current_guide_available == Some(true),
            )
            .await
            .map_err(|e| e.to_string());
            let library = crate::admin::server_fns::teletext_atlas::load_guide_library().await?;
            Ok::<_, ServerFnError>((data, edition, image, library))
        },
    );
    view! { <Suspense fallback=admin_loading_resource_view>{move ||data.get().map(|result|admin_result_view(
        result.map_err(|_|tr(TextKey::TeletextAdminUnavailable).to_owned()),
        |(data,edition,image,library)|workspace(&data,&edition,image,sources,owner,&error,library)
    ))}</Suspense> }
}

async fn initial_picture(
    id: i64,
    owner: Option<TeletextPageAddress>,
    edition: &AtlasEdition,
    live: bool,
) -> Result<Option<AtlasPicture>, ServerFnError> {
    let Some((design, revision)) = edition.design else {
        return Ok(None);
    };
    if edition.working.is_some() && edition.publication_key.is_none() {
        return Ok(None);
    }
    if live {
        match load_atlas_guide_preview(id, owner, false).await {
            Ok(p) => {
                return Ok(Some(AtlasPicture {
                    owner: p.owner,
                    hits: p.hits,
                    image: p.image_url,
                    pages: p.pages,
                    navigation: p.navigation,
                }))
            }
            Err(error) => {
                tracing::debug!(channel_id = id, %error, "using the stored edition for the Atlas preview");
            }
        }
    }
    let result = crate::admin::server_fns::teletext_drafts::load_teletext_draft_preview(
        id, design, revision, owner,
    )
    .await?;
    if let Some(error) = result.error {
        return Err(ServerFnError::new(error));
    }
    Ok(result.preview.and_then(|p| {
        Some(AtlasPicture {
            owner: p.owner?,
            hits: p.hits,
            image: p.image_url?,
            pages: p.pages,
            navigation: p.navigation,
        })
    }))
}

fn reader_labels() -> serde_json::Value {
    json!({
        "from":tr(TextKey::TeletextElementPageNumber),
        "index":tr(TextKey::AtlasGuideIndex),
        "guide":tr(TextKey::AtlasGuidePage),
        "edit":tr(TextKey::Edit),
        "delete":tr(TextKey::Delete),
        "confirm":tr(TextKey::ConfirmDelete),
        "add":tr(TextKey::Add),
        "sheet":tr(TextKey::TeletextPreviewSubpage),
        "blank":tr(TextKey::AtlasNone),
        "invalid":tr(TextKey::TeletextElementErrorPageNumber),
        "occupied":tr(TextKey::TeletextElementChannelErrorPageInUse),
        "auto":tr(TextKey::AtlasAuto),
        "none":tr(TextKey::AtlasNoLink),
        "inherit":tr(TextKey::Inherit),
        "unsaved":tr(TextKey::AtlasUnsaved),
        "failed":tr(TextKey::TeletextAuthoringRejected),
        "hide":tr(TextKey::AtlasHide),
        "show":tr(TextKey::AtlasReveal),
        "play":tr(TextKey::AtlasPlay),
        "pause":tr(TextKey::AtlasPause),
        "preview":tr(TextKey::AtlasNoPreview),
        "off":format!("{} · {}",tr(TextKey::ProgrammeGuide),tr(TextKey::Off)),
        "loading":tr(TextKey::Loading),
        "source":tr(TextKey::AtlasChannelSource)})
}

fn edition_status(edition: &AtlasEdition, error: &str) -> String {
    if error == "conflict" || error == "stale" {
        tr(TextKey::TeletextPublicationConflict).to_owned()
    } else if error == "saved-off" {
        tr(TextKey::Off).to_owned()
    } else if error.is_empty() {
        edition
            .accepted
            .map(|id| format!("{} · {id}", tr(TextKey::AtlasPublished)))
            .unwrap_or_default()
    } else {
        super::super::teletext_drafts::draft_status_text(error).to_owned()
    }
}

fn receiver_config(
    data: &ChannelTeletextData,
    pages: &[AtlasPage],
    image: Option<&AtlasPicture>,
    capable: bool,
    addon: Option<&super::TeletextAddonData>,
) -> serde_json::Value {
    let channels = data
        .guide_channels
        .iter()
        .flatten()
        .map(|c| json!({"id": c.id.to_string(), "name": format!("{} · {}", c.number, c.name)}))
        .collect::<Vec<_>>();
    let first = addon
        .and_then(|a| effective_setting(a, &["PAGE_START"]))
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(100);
    json!({
        "pages": pages,
        "hits": image.map(|p| &p.hits),
        "channels": channels,
        "labels": reader_labels(),
        "inventory": image.map_or(&[][..], |p| p.pages.as_slice()),
        "navigation": image.map_or(&[][..], |p| p.navigation.as_slice()),
        "capable": capable,
        "custom": data.custom_pages,
        "first": first,
    })
}

fn workspace(
    data: &ChannelTeletextData,
    edition: &AtlasEdition,
    picture: Result<Option<AtlasPicture>, String>,
    sources: bool,
    owner: Option<TeletextPageAddress>,
    error: &str,
    theme: Option<crate::admin::server_fns::teletext_atlas::GuideLibrary>,
) -> AnyView {
    let (image, empty_text, empty_role) = match picture {
        Ok(image) => (image, TextKey::AtlasNoPreview, "status"),
        Err(_) => (None, TextKey::PreviewUnavailable, "alert"),
    };
    let base = format!("/channels/{}/teletext", data.channel_id);
    let addon = data.addons.iter().find(|a| a.addon_id == GUIDE_ADDON);
    let enabled = edition
        .guide_on
        .unwrap_or_else(|| addon.is_some_and(super::source_on));
    let capable = addon.is_some_and(|a| {
        super::source_usable(a) && a.settings.iter().any(|s| s.definition.key == "PAGE_MAP")
    });
    let mut pages = assignments(data, edition);
    pages.extend(element_pages(data));
    pages.sort_by_key(|p| p.page);
    let library_count = data.custom_pages.as_ref().map_or(0, |c| c.library.len());
    let install = addon
        .and_then(|a| a.installation.as_ref())
        .and_then(|i| serde_json::to_string(i).ok())
        .unwrap_or_default();
    let binding = addon.and_then(|a| a.binding.as_ref());
    let profile = binding.and_then(|b| addon?.profiles.iter().find(|p| p.id == b.profile_id));
    let active = image.as_ref().map(|v| v.owner).or(owner);
    let number = active
        .map(|p| p.page.get())
        .or_else(|| pages.first().map(|p| p.page.get()))
        .unwrap_or(100);
    let cfg = receiver_config(data, &pages, image.as_ref(), capable, addon);
    let tab = if sources { "sources" } else { "pages" };
    let advanced = format!("{base}?advanced=1");
    let capacity = match data.authoring_error.as_deref() {
        Some("missing-policy") => Some(TextKey::TeletextCapacityAbsent),
        Some("invalid-policy") => Some(TextKey::TeletextCapacityInvalid),
        _ => None,
    };
    let fallback=pages.iter().map(|p|{
        let title=p.title.clone().or_else(||p.element.as_ref().and_then(|id| data.custom_pages.as_ref()?.library.iter().find(|e|e.id.to_string()==*id).map(|e|e.name.clone()))).or_else(||p.source.as_ref().and_then(|id|data.guide_channels.iter().flatten().find(|c|c.id.to_string()==*id).map(|c|c.name.clone()))).unwrap_or_else(||tr(TextKey::AtlasGuideIndex).to_owned());
        view!{<li><a href=format!("{base}?page={}",p.page)>{format!("{}  {title}",p.page)}</a></li>}
    }).collect_view();
    let status = edition_status(edition, error);
    let library = super::super::teletext_custom_pages::custom_pages_island(data);
    let producers = data
        .addons
        .iter()
        .filter(|addon| addon.addon_id != GUIDE_ADDON)
        .map(|addon| super::addon_section(data, addon))
        .collect_view();
    view!{
        <div data-atlas data-config=cfg.to_string() class=tabs::layout data-testid="channel-teletext-page">
            <link rel="stylesheet" href="/public/teletext-atlas.css"/>
            <header class="atlas-heading"><h1>{tr(TextKey::AtlasSection)}</h1><a href=format!("/channels/{}",data.channel_id)>{format!("{} · {}",data.channel_number,data.channel_name)}</a></header>
            <nav class=tabs::tabs aria-label=tr(TextKey::AtlasSection)>
                {[(false,"pages",TextKey::TeletextPageSetPages),(true,"sources",TextKey::AddonSources)].into_iter().map(|(on,id,key)|view!{
                    <a class=if sources==on {tabs::tab_active}else{tabs::tab} data-tab=id data-active-class=tabs::tab_active data-idle-class=tabs::tab href=format!("{base}?tab={id}") aria-current=(sources==on).then_some("page")>{tr(key)}</a>
                }).collect_view()}
            </nav>
            {capacity.map(|key|view!{<p role="status">{tr(key)}" "<a href=advanced.clone()>{tr(TextKey::TeletextReadinessCapacity)}</a></p>})}
            <form action="/admin-api/teletext/atlas/save" method="post" data-edition>
                {hidden("revision",edition.revision.to_string())}{hidden("expected_installation",install)}
                {hidden("expected_binding_revision",binding.map(|b|b.revision.to_string()).unwrap_or_default())}
                {hidden("expected_profile_revision",profile.map(|p|p.revision.to_string()).unwrap_or_default())}
                {hidden("channel_id",data.channel_id.to_string())}{hidden("selection",data.authoring_selection.clone().unwrap_or_default())}
                {hidden("generation",edition.accepted.map(|v|v.to_string()).unwrap_or_default())}{hidden("pending",edition.pending.map(|v|v.to_string()).unwrap_or_default())}
                {hidden("layout",serde_json::to_string(&pages).unwrap_or_default())}{hidden("tab",tab.to_owned())}
                <section class="atlas-reader" data-panel="pages" hidden=sources aria-label=tr(TextKey::TeletextPageSetPages)>
                    <div class="atlas-index"><h2>{tr(TextKey::AtlasIndex)}</h2><ul data-index>{fallback}</ul>
                        <button class=btn_class(ButtonVariant::Text) type="button" data-do="add" disabled=!capable && library_count==0>"＋ "{tr(TextKey::AtlasAddPages)}</button>
                    </div>
                    <figure><div class="atlas-picture">
                        <img data-image hidden=image.is_none() src=image.as_ref().map(|p|p.image.clone()) alt=tr(TextKey::TeletextPreviewPage)/>
                        <span data-empty role=empty_role hidden=image.is_some()>{tr(empty_text)}</span>
                    <div data-hotspots></div></div><div class="atlas-track"><i></i></div>
                    <figcaption><button type="button" class=btn_class(ButtonVariant::Text) data-do="cycle" disabled>{tr(TextKey::AtlasPlay)}</button><span data-guide-off hidden>{format!("{} · {}",tr(TextKey::ProgrammeGuide),tr(TextKey::Off))}</span><span data-counter></span></figcaption></figure>
                    <aside class="atlas-remote"><label for="atlas-number">{tr(TextKey::TeletextElementPageNumber)}</label>
                        <div class="atlas-jump"><button type="button" data-do="home" aria-label=tr(TextKey::GoHome)>"⌂"</button><input id="atlas-number" type="number" min="100" max="899" value=number/><button type="button" data-do="go">"OK"</button></div>
                        <div class="atlas-skip">{[("prev","«",TextKey::PreviousPage),("back","‹",TextKey::AtlasSubpages),("forward","›",TextKey::AtlasSubpages),("next","»",TextKey::NextPage)].into_iter().map(|(id,g,k)|view!{<button type="button" data-do=id aria-label=tr(k)>{g}</button>}).collect_view()}</div>
                        <div class="atlas-digits">{(1..=9).map(|n|view!{<button type="button" data-digit=n>{n}</button>}).collect_view()}
                            <button type="button" data-do="reveal">{tr(TextKey::AtlasReveal)}</button><button type="button" data-digit="0">"0"</button><button type="button" data-do="erase" aria-label=tr(TextKey::AtlasClear)>"⌫"</button>
                        </div><div class="atlas-keys">{COLOURS.into_iter().map(|(c,k)|view!{<button type="button" data-colour=c aria-label=tr(k)>{tr(k)}</button>}).collect_view()}</div>
                    </aside>
                </section>
                <section class="atlas-sources" data-panel="sources" hidden=!sources><h2>{tr(TextKey::AtlasChannelSource)}</h2>
                    <label><input type="checkbox" name="guide_on" value="1" checked=enabled disabled=!addon.is_some_and(super::source_usable)/>{tr(TextKey::ProgrammeGuide)}</label><div data-mapping></div><a href="/teletext-elements#guide-theme">{tr(TextKey::AtlasTheme)}</a></section>
                <footer><button type="submit" class=btn_class(ButtonVariant::Primary) disabled=!data.catalog_available || (addon.is_none() && library_count==0)>{tr(TextKey::AtlasSave)}</button><span role=if error.is_empty() || error=="saved-off" {"status"}else{"alert"} data-status>{status}</span></footer>
            </form>
            <details data-tools hidden=!sources><summary>{tr(TextKey::AtlasAdvanced)}</summary>{producers}{library}<a href=advanced>{tr(TextKey::AtlasAdvanced)}</a></details>
            <noscript><a href=format!("{base}?advanced=1")>{tr(TextKey::AtlasAdvanced)}</a></noscript>
            {page_dialog()}
            <dialog data-theme-dialog><button class=btn_class(ButtonVariant::Text) type="button" data-do="close-theme" aria-label=tr(TextKey::Cancel)>"×"</button>
            {theme.map(|lib|crate::admin::pages::teletext_elements::guide_theme::editor(lib,""))}</dialog>

        </div>
    }.into_any()
}

fn page_dialog() -> AnyView {
    view!{<dialog data-editor aria-labelledby="atlas-editor-title"><form method="dialog"><h2 id="atlas-editor-title">{tr(TextKey::AtlasSettings)}</h2>
        <label>{tr(TextKey::AtlasTemplate)}<select name="template"><optgroup label=tr(TextKey::ProgrammeGuide)><option value="guide">{tr(TextKey::AtlasGuidePage)}</option><option value="index">{tr(TextKey::AtlasGuideIndex)}</option></optgroup><optgroup data-element-options label=tr(TextKey::TeletextCustomPagesTitle)></optgroup></select></label>
        <details data-page-settings open><summary>{tr(TextKey::AtlasSettings)}</summary>
        <label data-source-label>{tr(TextKey::AtlasChannelSource)}<select name="source"></select></label>
        <label data-range-label>{tr(TextKey::AtlasPeriod)}<select name="range">
            <option value="">{tr(TextKey::Inherit)}</option><option value="all">{tr(TextKey::All)}</option>
            <option value="evening">"18:00 – 24:00"</option><option value="early">"18:00 – 21:00"</option>
            <option value="late">"21:00 – 24:00"</option></select></label>
        <div data-template-fields></div><label data-part-label>{tr(TextKey::TeletextPreviewSubpage)}<input type="number" name="part" min="1" max="64" value="1"/></label>
        <label data-interval-label>{tr(TextKey::AtlasCycle)}<input type="number" name="interval" min="1" max="600" value="10"/></label>
        <details data-link-fields><summary>{tr(TextKey::AtlasColours)}</summary>{COLOURS.into_iter().map(|(c,k)|view!{<label>{tr(k)}<select name=c></select></label>}).collect_view()}</details>
        <a href="/teletext-elements#guide-theme" target="_blank" rel="noopener">{tr(TextKey::AtlasTheme)}</a></details>
        <label data-page-label>{tr(TextKey::TeletextElementPageNumber)}<input name="page" type="number" min="100" max="899" required/></label>
        <label>{tr(TextKey::Title)}<input name="title" maxlength="34"/></label>
        <footer><button type="submit" class=btn_class(ButtonVariant::Primary)>{tr(TextKey::Apply)}</button><button type="button" data-do="another" class=btn_class(ButtonVariant::Text)>{tr(TextKey::AtlasAddPages)}</button><button type="button" data-do="close" class=btn_class(ButtonVariant::Text)>{tr(TextKey::Cancel)}</button></footer>
    </form></dialog>}.into_any()
}
