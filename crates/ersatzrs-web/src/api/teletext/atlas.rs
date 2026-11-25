//! The channel save explicitly validates and selects a complete immutable edition.
use super::{AppState, ControlResult};
use crate::admin::server_fns::{
    teletext::{load_channel_teletext_for_application, ChannelTeletextData, GUIDE_ADDON},
    teletext_atlas::{assignments, AtlasEdition, AtlasPage},
};
use crate::host::teletext_authoring::{
    atlas, prepare_teletext_design, publish_teletext_generation, request_teletext_publication,
    TeletextAuthoringPrepare, TeletextAuthoringPublicationTarget, TeletextAuthoringPublishTarget,
    TeletextAuthoringScope,
};
use axum::{
    extract::{Form, State},
    response::{IntoResponse, Redirect, Response},
    Json,
};
use ersatzrs_app::teletext_elements::TeletextElementError;
use ersatzrs_domain::{TeletextPageAddress, TeletextSubcode};
use serde::Deserialize;
use std::{collections::BTreeSet, time::Instant};

#[derive(Deserialize)]
pub struct Submission {
    channel_id: i64,
    layout: String,
    selection: String,
    generation: String,
    revision: i64,
    #[serde(default)]
    guide_on: Option<String>,
    expected_installation: String,
    expected_binding_revision: String,
    expected_profile_revision: String,
    pending: String,
    page: Option<u16>,
    subcode: Option<u16>,
    reveal: Option<bool>,
    #[serde(default)]
    tab: String,
    #[serde(default)]
    current: bool,
}

fn deadline() -> Instant {
    Instant::now() + ersatzrs_app::teletext_channel_documents::PREPARATION_TIMEOUT
}

async fn channel(state: &AppState, form: &Submission) -> ControlResult<ChannelTeletextData> {
    let data = load_channel_teletext_for_application(&state.application, form.channel_id)
        .await
        .map_err(|_| "unavailable")?
        .ok_or("invalid-input")?;
    if data.authoring_error.as_deref() == Some("busy") {
        return Err("busy");
    }
    if data.authoring_selection.as_deref().unwrap_or_default() != form.selection {
        return Err("conflict");
    }
    Ok(data)
}

fn candidate(
    data: &ChannelTeletextData,
    pages: &[AtlasPage],
    all_pages: &[AtlasPage],
    guide_on: bool,
) -> ControlResult<TeletextAuthoringPrepare> {
    let addon = data
        .addons
        .iter()
        .find(|a| guide_on && a.addon_id == GUIDE_ADDON && a.installed);
    let custom = data
        .custom_pages
        .as_ref()
        .and_then(|p| p.links.iter().find(|p| p.enabled));
    let mut input = serde_json::json!({"input_version":1,"channel_id":data.channel_id,"resource_transmissions":[],"v1_region":"western_latin"});
    if let Some(addon) = addon {
        let index = pages
            .iter()
            .find(|p| p.source.is_none())
            .ok_or("invalid-input")?
            .page;
        if pages.iter().filter(|p| p.source.is_none()).count() != 1
            || pages.iter().map(|p| p.page).collect::<BTreeSet<_>>().len() != pages.len()
        {
            return Err("invalid-input");
        }
        let mut ids = BTreeSet::new();
        for page in pages {
            for id in page
                .source
                .iter()
                .chain(page.subpages.iter().filter_map(|s| s.source.as_ref()))
            {
                let id = id.parse::<i64>().map_err(|_| "invalid-input")?;
                if !data.guide_channels.iter().flatten().any(|c| c.id == id) {
                    return Err("invalid-input");
                }
                ids.insert(id);
            }
        }
        if ids.is_empty() {
            ids.insert(data.channel_id);
        }
        input["entry_addon_id"] = GUIDE_ADDON.into();
        input["entry"] = serde_json::json!({"page":index,"subcode":0});
        input["guide_channel_ids"] = serde_json::json!(ids);
        if addon
            .settings
            .iter()
            .any(|s| s.definition.key == "PAGE_MAP")
        {
            input["input_version"] = 3.into();
            let mut layout = serde_json::to_value(pages).map_err(|_| "invalid-input")?;
            let external: Vec<_> = all_pages.iter().filter_map(|p| {
                let element = p.element.as_ref()?;
                let template = data.custom_pages.as_ref()?.library.iter().find(|e| e.id.to_string() == *element)?;
                Some(serde_json::json!({"page":p.page,"title":p.title.as_ref().unwrap_or(&template.name)}))
            }).collect();
            if !external.is_empty() {
                let index_entry = pages
                    .iter()
                    .position(|p| p.page == index)
                    .ok_or("invalid-input")?;
                layout[index_entry]["index_entries"] = external.into();
            }
            input["page_layout"] = layout.to_string().into();
        } else {
            if pages != assignments(data, &AtlasEdition::default()) {
                return Err("wrong-addon");
            }
            input["input_version"] = 2.into();
        }
    } else if let Some(link) = custom {
        input["entry_addon_id"] = ersatzrs_domain::TELETEXT_CUSTOM_PAGES_PRODUCER_ID.into();
        let count = link
            .instance
            .as_ref()
            .map(|i| i.subpages.len())
            .filter(|n| *n > 0)
            .unwrap_or_else(|| {
                data.custom_pages
                    .iter()
                    .flat_map(|p| &p.library)
                    .find(|e| e.id == link.element_id)
                    .map_or(1, |e| usize::from(e.subpages))
            });
        input["entry"] = serde_json::json!({"page":link.page.parse::<u16>().map_err(|_|"invalid-input")?,"subcode":u16::from(count>1)});
    } else {
        return Err("unavailable");
    }
    Ok(TeletextAuthoringPrepare {
        channel_id: data.channel_id,
        scope: TeletextAuthoringScope::Base,
        name: format!("Teletext · {}", data.channel_number),
        selection_digest: match (&data.authoring_selection, data.authoring_error.as_deref()) {
            (Some(digest), _) => digest.clone(),
            (None, Some("off")) => String::new(),
            _ => return Err("unavailable"),
        },
        input_json: input.to_string(),
    })
}

fn pages(form: &Submission) -> ControlResult<Vec<AtlasPage>> {
    if form.layout.len() > 16 * 1024 {
        return Err("limit");
    }
    serde_json::from_str(&form.layout).map_err(|_| "invalid-input")
}

pub async fn picture(State(state): State<AppState>, Form(form): Form<Submission>) -> Response {
    let result = async {
        let until = deadline();
        let data = channel(&state, &form).await?;
        let pages = themed_pages(&state, &form).await?;
        let owner = TeletextPageAddress {
            page: form
                .page
                .ok_or("invalid-input")?
                .try_into()
                .map_err(|_| "invalid-input")?,
            subcode: TeletextSubcode::new(form.subcode.unwrap_or(0))
                .map_err(|_| "invalid-input")?,
        };
        let (guides, elements) = validate_layout(&data, &pages, form.guide_on.as_deref()==Some("1"))?;
        // Only use a sealed live raster for the generation the reader opened.
        // Edits, theme changes and unavailable live data fall back to the draft compiler.
        if let Some(generation) = super::optional_positive(&form.generation)?.filter(|_|form.current && data.current_guide_available==Some(true)) {
            let fingerprint = publication_key(&data,&pages,&elements,form.guide_on.as_deref()==Some("1"))?;
            let stored=state.application.teletext_atlas_state(form.channel_id).await.map_err(|_|"storage")?;
            if stored.is_some_and(|s|s.publication_key.as_ref()==Some(&fingerprint)&&s.generation==Some(generation)) {
                let live=state.preview_current_guide(form.channel_id,Some(owner),form.reveal.unwrap_or(false),move |p|{
                    if p.generation.generation_id!=generation {
                        return Err(crate::host::teletext_authoring::TeletextAuthoringError{status:crate::host::teletext_authoring::TeletextAuthoringStatus::SelectionChanged,location:None});
                    }
                    Ok(crate::admin::server_fns::teletext_atlas::AtlasPicture {hits:p.hits.clone(),image:crate::admin::server_fns::teletext_drafts::raster_image(&p.raster)?,owner,pages:p.pages.clone(),navigation:p.navigation.clone()})
                }).await;
                if let Ok(live)=live {return Ok(live);}
            }
        }
        let layout = serde_json::to_string(&guides).map_err(|_| "invalid-input")?;
        if pages
            .iter()
            .any(|p| p.page == owner.page && p.element.is_some())
        {
            return atlas::element_picture(
                &state,
                form.channel_id,
                &elements,
                Some(&layout),
                owner,
                form.reveal.unwrap_or(false),
                until,
            )
            .await
            .map_err(|e| e.code());
        }
        let request = candidate(
            &data,
            &guides,
            &pages,
            true,
        )?;
        atlas::picture(&state, request, &elements, owner, form.reveal.unwrap_or(false), until)
            .await
            .map_err(|e| e.code())
    }
    .await;
    match result {
        Ok(p) => Json(p).into_response(),
        Err(code) => (axum::http::StatusCode::UNPROCESSABLE_ENTITY, code).into_response(),
    }
}

pub async fn save(State(state): State<AppState>, Form(form): Form<Submission>) -> Response {
    let tab = if form.tab == "sources" {
        "sources"
    } else {
        "pages"
    };
    let result = publish(&state, &form).await;
    let feedback = match result {
        Ok(0) => "guide=off".to_owned(),
        Ok(id) => format!("edition={id}"),
        Err(code) => format!("atlas_error={code}"),
    };
    Redirect::to(&format!(
        "/channels/{}/teletext?tab={tab}&{feedback}",
        form.channel_id
    ))
    .into_response()
}

async fn guide_binding_change(
    state: &AppState,
    data: &ChannelTeletextData,
    form: &Submission,
    guide_on: bool,
) -> ControlResult<Option<ersatzrs_core::repositories::ChannelTeletextBindingSave>> {
    let observed = data.addons.iter().find(|a| a.addon_id == GUIDE_ADDON);
    let was_on = observed.is_some_and(|a| {
        a.binding
            .as_ref()
            .is_some_and(|b| b.enabled && b.availability == "configured")
    });
    if guide_on == was_on {
        return Ok(None);
    }
    let addon = observed.ok_or("unavailable")?;
    let install = serde_json::from_str::<
        crate::admin::server_fns::teletext::TeletextInstallationData,
    >(&form.expected_installation)
    .map_err(|_| "conflict")?;
    let binding = addon.binding.as_ref();
    let profile = binding.and_then(|b| addon.profiles.iter().find(|p| p.id == b.profile_id));
    let binding_revision = super::optional_positive(&form.expected_binding_revision)?;
    if addon.installation.as_ref() != Some(&install)
        || addon.availability != "configured"
        || binding.map(|b| b.revision) != binding_revision
        || profile.map(|p| p.revision) != super::optional_positive(&form.expected_profile_revision)?
    {
        return Err("conflict");
    }
    let install = install.into();
    let (profile_id, expected_profile_revision) = super::guide::profile_to_bind(
        &state.application,
        form.channel_id,
        addon,
        profile,
        &install,
    )
    .await?;
    Ok(Some(
        ersatzrs_core::repositories::ChannelTeletextBindingSave {
            channel_id: form.channel_id,
            addon_id: GUIDE_ADDON.into(),
            profile_id,
            enabled: guide_on,
            expected_installation: install,
            expected_profile_revision,
            expected_binding_revision: binding_revision,
        },
    ))
}

async fn publish(state: &AppState, form: &Submission) -> ControlResult<i64> {
    let until = deadline();

    let mut data = channel(state, form).await?;
    let pages = themed_pages(state, form).await?;
    let guide_on = form.guide_on.as_deref() == Some("1");
    let (guides, elements) = validate_layout(&data, &pages, guide_on)?;
    let layout = serde_json::to_string(&guides).map_err(|_| "invalid-input")?;
    let generation = super::optional_positive(&form.generation)?;
    let pending = super::optional_positive(&form.pending)?;
    let current = atlas::edition(&state.application, form.channel_id)
        .await
        .map_err(|e| e.code())?;
    if current.revision != form.revision
        || current.accepted != generation
        || current.pending != pending
    {
        return Err("conflict");
    }
    let key = publication_key(&data, &pages, &elements, guide_on)?;
    if let Some(id) = generation {
        if current.selection.as_deref() == Some(&form.selection)
            && current.publication_key.as_ref() == Some(&key)
        {
            return Ok(id);
        }
    }
    let change = guide_binding_change(state, &data, form, guide_on).await?;
    let revision = state
        .application
        .save_teletext_atlas(
            form.channel_id,
            form.revision,
            &layout,
            &elements,
            change.as_ref(),
        )
        .await
        .map_err(TeletextElementError::code)?;
    data = load_channel_teletext_for_application(&state.application, form.channel_id)
        .await
        .map_err(|_| "unavailable")?
        .ok_or("invalid-input")?;
    if !guide_on && elements.is_empty() {
        if state
            .application
            .finish_teletext_atlas_off(form.channel_id, revision, generation, pending)
            .await
            .map_err(TeletextElementError::code)?
        {
            return Ok(0);
        }
        return Err("conflict");
    }
    let request = candidate(&data, &guides, &pages, guide_on)?;
    let saved = prepare_teletext_design(state, request, until)
        .await
        .map_err(|e| e.code())?;
    let preview = saved.preview.map_err(|e| e.code())?;
    let obs = preview.publication().ok_or("unavailable")?;
    if pending != obs.current_pending_id || generation != obs.current_accepted_id {
        return Err("conflict");
    }
    if !state
        .application
        .register_teletext_atlas_candidate(
            form.channel_id,
            revision,
            &key,
            saved.identity.design_id,
            saved.identity.revision,
        )
        .await
        .map_err(TeletextElementError::code)?
    {
        return Err("conflict");
    }
    let mut target = TeletextAuthoringPublicationTarget {
        identity: saved.identity,
        digest: obs.digest.clone(),
        expected_design_head: obs.current_design_head,
        expected_request_id: pending,
    };
    drop(preview);
    let intent = request_teletext_publication(state, target.clone(), until)
        .await
        .map_err(|e| e.code())?;
    target.expected_request_id = Some(intent.request_id);
    let selected = publish_teletext_generation(
        state,
        TeletextAuthoringPublishTarget {
            publication: target,
            expected_generation_id: generation,
        },
        until,
    )
    .await
    .map_err(|e| e.code())?;
    Ok(selected.generation_id)
}

fn publication_key(
    data: &ChannelTeletextData,
    pages: &[AtlasPage],
    elements: &[ersatzrs_domain::TeletextAtlasElement],
    guide_on: bool,
) -> ControlResult<String> {
    let versions = elements
        .iter()
        .filter_map(|p| {
            data.custom_pages
                .as_ref()?
                .library
                .iter()
                .find(|e| e.id == p.element_id)
                .map(|e| (e.id, e.revision))
        })
        .collect::<Vec<_>>();
    let signature = if guide_on && !elements.is_empty() {
        // Earlier mixed editions omitted library destinations from the broadcast index.
        serde_json::to_vec(&("atlas-index-v2", guide_on, pages, versions))
    } else {
        serde_json::to_vec(&(guide_on, pages, versions))
    }
    .map_err(|_| "invalid-input")?;
    Ok(atlas::fingerprint(&signature))
}

async fn themed_pages(state: &AppState, form: &Submission) -> ControlResult<Vec<AtlasPage>> {
    let mut pages = pages(form)?;
    let theme = crate::admin::server_fns::teletext_atlas::read_theme(&state.application).await?;
    for page in &mut pages {
        page.theme = if page.source.is_none() && page.element.is_none() {
            theme.clone()
        } else {
            None
        };
    }
    Ok(pages)
}

fn validate_layout(
    data: &ChannelTeletextData,
    pages: &[AtlasPage],
    guide_on: bool,
) -> ControlResult<(Vec<AtlasPage>, Vec<ersatzrs_domain::TeletextAtlasElement>)> {
    use ersatzrs_domain::{TeletextAtlasElement, TeletextElementInstance, TeletextInstanceSheet};
    let addresses = pages.iter().map(|p| p.page).collect::<BTreeSet<_>>();
    if pages.is_empty() || addresses.len() != pages.len() {
        return Err("invalid-input");
    }
    let inactive = pages
        .iter()
        .filter(|p| !guide_on && p.element.is_none())
        .map(|p| p.page)
        .collect::<BTreeSet<_>>();
    let mut elements = Vec::new();
    let mut guides = Vec::new();
    for p in pages {
        if (p.source.is_none()
            && (p.range.is_some() || p.subpages.iter().any(|s| s.range.is_some())))
            || p.interval.is_some_and(|s| !(1..=600).contains(&s))
            || p.subpages.len() > 64
            || p.links.iter().any(|(k, v)| {
                !["red", "green", "yellow", "blue"].contains(&k.as_str())
                    || v.is_some_and(|n| !addresses.contains(&n))
            })
        {
            return Err("invalid-input");
        }
        for title in p
            .title
            .iter()
            .chain(p.subpages.iter().filter_map(|s| s.title.as_ref()))
        {
            if title.is_empty() || title.chars().count() > 34 || title.chars().any(char::is_control)
            {
                return Err("invalid-input");
            }
        }
        if let Some(id) = p.element.as_deref() {
            let id = id.parse::<i64>().map_err(|_| "invalid-input")?;
            let e = data
                .custom_pages
                .as_ref()
                .and_then(|c| c.library.iter().find(|e| e.id == id))
                .ok_or("invalid-input")?;
            if p.source.is_some()
                || p.subpages
                    .iter()
                    .any(|s| s.source.is_some() || s.part == 0 || s.part > e.subpages)
                || p.fields
                    .keys()
                    .chain(p.subpages.iter().flat_map(|s| s.fields.keys()))
                    .any(|k| !e.fields.contains(k))
            {
                return Err("invalid-input");
            }
            elements.push(TeletextAtlasElement {
                element_id: id,
                page: ersatzrs_domain::parse_teletext_element_page_number(&p.page.to_string())
                    .ok_or("invalid-input")?,
                instance: TeletextElementInstance {
                    title: p.title.clone(),
                    interval: p.interval,
                    fields: p.fields.clone(),
                    links: p.links.clone(),
                    inactive_keys: p
                        .links
                        .iter()
                        .filter(|(_, v)| v.is_some_and(|n| inactive.contains(&n)))
                        .map(|(key, _)| key.clone())
                        .collect(),
                    subpages: p
                        .subpages
                        .iter()
                        .map(|s| TeletextInstanceSheet {
                            title: s.title.clone(),
                            fields: s.fields.clone(),
                            part: s.part,
                        })
                        .collect(),
                },
            });
        } else {
            if !p.fields.is_empty()
                || p.subpages
                    .iter()
                    .any(|s| s.part == 0 || !s.fields.is_empty())
            {
                return Err("invalid-input");
            }
            guides.push(p.clone());
        }
    }
    if guide_on && guides.iter().filter(|p| p.source.is_none()).count() != 1 {
        return Err("invalid-input");
    }
    Ok((guides, elements))
}

#[derive(Deserialize)]
pub struct ThemeForm {
    banner: String,
    accent: String,
    heading: String,
}
pub async fn save_theme(State(state): State<AppState>, Form(form): Form<ThemeForm>) -> Response {
    use crate::admin::server_fns::teletext_atlas::{AtlasTheme, COLOURS, THEME_KEY};
    let result = async {
        if form.banner.trim().is_empty()
            || form.banner.chars().count() > 36
            || form.banner.chars().any(char::is_control)
            || ![&form.accent, &form.heading]
                .iter()
                .all(|v| COLOURS.contains(&v.as_str()))
        {
            return Err("invalid-input");
        }
        let theme = AtlasTheme {
            banner: form.banner,
            accent: form.accent,
            heading: form.heading,
        };
        state
            .application
            .set_admin_setting(ersatzrs_app::commands::SetAdminSetting {
                key: THEME_KEY,
                value: serde_json::to_string(&theme).map_err(|_| "invalid-input")?,
            })
            .await
            .map_err(|_| "storage")
    }
    .await;
    Redirect::to(if result.is_ok() {
        "/teletext-elements?theme=saved#guide-theme"
    } else {
        "/teletext-elements?theme=error#guide-theme"
    })
    .into_response()
}
