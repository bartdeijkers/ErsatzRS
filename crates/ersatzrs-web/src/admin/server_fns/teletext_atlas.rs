//! Channel-level page assignments. Add-on guide data is captured by the host.
use ersatzrs_domain::{TeletextNavigationLink, TeletextPageAddress, TeletextPageNumber};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::teletext::{effective_setting, ChannelTeletextData, GUIDE_ADDON};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AtlasPage {
    pub page: TeletextPageNumber,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub element: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<GuideRange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub links: BTreeMap<String, Option<TeletextPageNumber>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subpages: Vec<AtlasSheet>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<AtlasTheme>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AtlasSheet {
    pub part: u16,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<GuideRange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GuideRange {
    All,
    Evening,
    Early,
    Late,
}

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AtlasEdition {
    pub design: Option<(i64, i64)>,
    pub guide_on: Option<bool>,
    pub accepted: Option<i64>,
    pub selection: Option<String>,
    pub revision: i64,
    pub publication_key: Option<String>,
    pub working: Option<Vec<AtlasPage>>,
    pub pending: Option<i64>,
    pub layout: Option<Vec<AtlasPage>>,
}

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AtlasHitMap {
    pub columns: u8,
    pub shortcuts: Vec<(u8, u8, ersatzrs_domain::TeletextNavigationRole)>,
    pub concealed: Vec<(u8, u8, u8)>,
    pub entries: Vec<(u8, u8, u16)>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AtlasPicture {
    pub hits: AtlasHitMap,
    pub image: String,
    pub owner: TeletextPageAddress,
    pub pages: Vec<TeletextPageAddress>,
    pub navigation: Vec<TeletextNavigationLink>,
}

#[server(prefix = "/admin-api")]
pub async fn load_atlas_edition(channel_id: i64) -> Result<AtlasEdition, ServerFnError> {
    let state = expect_context::<crate::host::state::AppState>();
    crate::host::teletext_authoring::atlas::edition(&state.application, channel_id)
        .await
        .map_err(|e| ServerFnError::new(e.code()))
}

pub(crate) fn assignments(data: &ChannelTeletextData, edition: &AtlasEdition) -> Vec<AtlasPage> {
    if let Some(pages) = edition.working.as_ref().or(edition.layout.as_ref()) {
        return pages.clone();
    }
    let Some(addon) = data
        .addons
        .iter()
        .find(|a| a.addon_id == GUIDE_ADDON && a.installed)
    else {
        return vec![];
    };
    let setting = |key| effective_setting(addon, &[key]).and_then(|s| s.parse::<u16>().ok());
    let Some((first, last)) = setting("PAGE_START").zip(setting("PAGE_END")) else {
        return vec![];
    };
    let index = setting("INDEX_PAGE").unwrap_or(first);
    let page = |n, source| {
        TeletextPageNumber::new(n).ok().map(|page| AtlasPage {
            page,
            element: None,
            fields: BTreeMap::new(),
            source,
            range: None,
            title: None,
            links: BTreeMap::new(),
            subpages: vec![],
            interval: None,
            theme: None,
        })
    };
    let ids = data
        .guide_selection
        .clone()
        .filter(|ids| !ids.is_empty())
        .unwrap_or_else(|| vec![data.channel_id]);
    let mut pages: Vec<_> = page(index, None)
        .into_iter()
        .chain(
            (first..=last)
                .filter(|n| *n != index)
                .zip(ids)
                .filter_map(|(n, id)| page(n, Some(id.to_string()))),
        )
        .collect();
    pages.sort_by_key(|p| p.page);
    pages
}

pub const THEME_KEY: &str = "teletext.atlas.guide.theme";
pub const COLOURS: [&str; 7] = ["white", "yellow", "green", "cyan", "red", "magenta", "blue"];

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AtlasTheme {
    pub banner: String,
    pub accent: String,
    pub heading: String,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuideLibrary {
    pub elements: Vec<(i64, String)>,
    pub defaults: AtlasTheme,
    pub saved: Option<AtlasTheme>,
}

#[server(prefix = "/admin-api")]
pub async fn load_guide_library() -> Result<Option<GuideLibrary>, ServerFnError> {
    use super::addons::load_addon_detail;
    let Some(installed) = load_addon_detail(GUIDE_ADDON.to_owned())
        .await?
        .and_then(|d| d.installed)
        .filter(|i| i.enabled && i.manifest.settings.iter().any(|s| s.key == "PAGE_MAP"))
    else {
        return Ok(None);
    };
    let value = |key: &str| {
        installed
            .settings
            .iter()
            .find(|s| s.key == key)
            .and_then(|s| s.value.clone())
            .or_else(|| {
                installed
                    .manifest
                    .settings
                    .iter()
                    .find(|s| s.key == key)
                    .and_then(|s| s.default.clone())
            })
    };
    let accent = value("ACCENT")
        .filter(|s| COLOURS.contains(&s.as_str()))
        .unwrap_or_else(|| {
            match value("THEME").as_deref() {
                Some("night") => "cyan",
                Some("contrast") => "white",
                _ => "yellow",
            }
            .to_owned()
        });
    let defaults = AtlasTheme {
        banner: value("HEADER").unwrap_or_else(|| "TV GUIDE".to_owned()),
        heading: accent.clone(),
        accent,
    };
    let state = expect_context::<crate::host::state::AppState>();
    let saved = read_theme(&state.application)
        .await
        .map_err(ServerFnError::new)?;
    let elements = state
        .application
        .list_teletext_elements()
        .await
        .map_err(|_| ServerFnError::new("storage"))?
        .into_iter()
        .map(|e| (e.id, e.name))
        .collect();
    Ok(Some(GuideLibrary {
        elements,
        defaults,
        saved,
    }))
}

#[cfg(feature = "ssr")]
pub(crate) async fn read_theme(
    app: &ersatzrs_app::ApplicationServices,
) -> Result<Option<AtlasTheme>, &'static str> {
    app.get_optional_setting(THEME_KEY)
        .await
        .map_err(|_| "storage")?
        .map(|raw| serde_json::from_str(&raw))
        .transpose()
        .map_err(|_| "invalid-input")
}

pub(crate) fn element_pages(data: &ChannelTeletextData) -> Vec<AtlasPage> {
    data.custom_pages
        .iter()
        .flat_map(|p| &p.links)
        .filter(|l| l.enabled)
        .filter_map(|link| {
            let opts = link.instance.clone().unwrap_or_default();
            Some(AtlasPage {
                page: TeletextPageNumber::new(link.page.parse().ok()?).ok()?,
                source: None,
                range: None,
                element: Some(link.element_id.to_string()),
                fields: opts.fields,
                title: opts.title,
                interval: opts.interval,
                links: opts.links,
                theme: None,
                subpages: opts
                    .subpages
                    .into_iter()
                    .map(|s| AtlasSheet {
                        part: s.part,
                        source: None,
                        range: None,
                        title: s.title,
                        fields: s.fields,
                    })
                    .collect(),
            })
        })
        .collect()
}
