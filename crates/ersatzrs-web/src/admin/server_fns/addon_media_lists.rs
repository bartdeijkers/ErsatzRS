use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListRow {
    pub id: i64,
    pub addon_id: String,
    pub provider_id: String,
    pub source_url: String,
    pub name: String,
    pub item_count: i32,
    pub match_count: i32,
    pub auto_refresh: bool,
    pub generate_playlist: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListForm {
    pub id: i64,
    pub addon_id: String,
    pub source_url: String,
    pub auto_refresh: bool,
    pub generate_playlist: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListProviderRow {
    pub addon_id: String,
    pub name: String,
}

#[server]
pub async fn list_addon_media_list_providers(
) -> Result<Vec<AddonMediaListProviderRow>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let overview = state
        .application
        .addon_overview()
        .await
        .map_err(ServerFnError::new)?;
    let mut providers = overview
        .installed
        .into_iter()
        .filter(|item| item.enabled)
        .filter(|item| {
            item.manifest.capabilities.iter().any(|capability| {
                capability.id == ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_LIST_V1
            })
        })
        .map(|item| AddonMediaListProviderRow {
            addon_id: item.addon_id,
            name: item
                .manifest
                .name
                .get("en-US")
                .or_else(|| item.manifest.name.values().next())
                .cloned()
                .unwrap_or_default(),
        })
        .collect::<Vec<_>>();
    providers.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(providers)
}

#[server]
pub async fn list_addon_media_lists(
    page: u32,
    page_size: u32,
) -> Result<(Vec<AddonMediaListRow>, u64), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    list_addon_media_lists_with_state(&state, page, page_size)
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub async fn list_addon_media_lists_with_state(
    state: &crate::host::state::AppState,
    page: u32,
    page_size: u32,
) -> ersatzrs_app::Result<(Vec<AddonMediaListRow>, u64)> {
    let (rows, total) = state
        .application
        .list_addon_media_lists(ersatzrs_app::queries::ListAddonMediaLists { page, page_size })
        .await?;
    Ok((
        rows.into_iter()
            .map(|row| AddonMediaListRow {
                id: row.id,
                addon_id: row.addon_id,
                provider_id: row.provider_id,
                source_url: row.source_url,
                name: row.name,
                item_count: row.item_count,
                match_count: row.match_count,
                auto_refresh: row.auto_refresh,
                generate_playlist: row.generate_playlist,
            })
            .collect(),
        total,
    ))
}

#[server]
pub async fn get_addon_media_lists_page_size() -> Result<u32, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .get_addon_media_lists_page_size()
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn get_addon_media_list_form(
    id: i64,
) -> Result<Option<AddonMediaListForm>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .get_addon_media_list(id)
        .await
        .map_err(ServerFnError::new)
        .map(|form| {
            form.map(|form| AddonMediaListForm {
                id: form.id,
                addon_id: form.addon_id,
                source_url: form.source_url,
                auto_refresh: form.auto_refresh,
                generate_playlist: form.generate_playlist,
            })
        })
}

#[server]
pub async fn add_addon_media_list(
    addon_id: String,
    source_url: String,
    folder_name: String,
) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    add_addon_media_list_with_state(&state, addon_id, source_url, folder_name)
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn match_addon_media_list(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    match_addon_media_list_with_state(&state, id)
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn update_addon_media_list(form: AddonMediaListForm) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    update_addon_media_list_with_state(&state, form)
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn delete_addon_media_list(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    delete_addon_media_list_with_state(&state, id)
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub async fn add_addon_media_list_with_state(
    state: &crate::host::state::AppState,
    addon_id: String,
    source_url: String,
    folder_name: String,
) -> ersatzrs_app::Result<i64> {
    let list_id = state
        .application
        .add_addon_media_list(ersatzrs_app::commands::AddAddonMediaList {
            addon_id,
            source_url,
            folder_name,
        })
        .await?;
    if let Some(library_id) = state
        .application
        .addon_media_list_library_id(list_id)
        .await?
    {
        let outcome = state
            .application
            .request_library_scan(ersatzrs_app::commands::RequestLibraryScan::new(
                library_id, false,
            ))
            .await;
        tracing::info!(
            list_id,
            library_id,
            ?outcome,
            "queued managed add-on list library scan"
        );
    }
    Ok(list_id)
}

#[cfg(feature = "ssr")]
pub async fn match_addon_media_list_with_state(
    state: &crate::host::state::AppState,
    id: i64,
) -> ersatzrs_app::Result<()> {
    state
        .application
        .match_addon_media_list(ersatzrs_app::commands::MatchAddonMediaList { id })
        .await
}

#[cfg(feature = "ssr")]
pub async fn update_addon_media_list_with_state(
    state: &crate::host::state::AppState,
    form: AddonMediaListForm,
) -> ersatzrs_app::Result<()> {
    state
        .application
        .update_addon_media_list(ersatzrs_app::commands::UpdateAddonMediaList {
            id: form.id,
            auto_refresh: form.auto_refresh,
            generate_playlist: form.generate_playlist,
        })
        .await
}

#[cfg(feature = "ssr")]
pub async fn delete_addon_media_list_with_state(
    state: &crate::host::state::AppState,
    id: i64,
) -> ersatzrs_app::Result<()> {
    state
        .application
        .delete_addon_media_list(ersatzrs_app::commands::DeleteAddonMediaList { id })
        .await
}
