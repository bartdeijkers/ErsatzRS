use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraktListRow {
    pub id: i64,
    pub trakt_id: i32,
    pub slug: String,
    pub name: String,
    pub item_count: i32,
    pub match_count: i32,
    pub auto_refresh: bool,
    pub generate_playlist: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraktListForm {
    pub id: i64,
    pub slug: String,
    pub auto_refresh: bool,
    pub generate_playlist: bool,
}

#[server]
pub async fn list_trakt_lists(
    page: u32,
    page_size: u32,
) -> Result<(Vec<TraktListRow>, u64), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    list_trakt_lists_with_state(&state, page, page_size)
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub async fn list_trakt_lists_with_state(
    state: &crate::host::state::AppState,
    page: u32,
    page_size: u32,
) -> ersatzrs_app::Result<(Vec<TraktListRow>, u64)> {
    let (rows, total) = state
        .application
        .list_trakt_lists(ersatzrs_app::queries::ListTraktLists { page, page_size })
        .await?;
    Ok((
        rows.into_iter()
            .map(|row| TraktListRow {
                id: row.id,
                trakt_id: row.trakt_id,
                slug: format!("{}/{}", row.user, row.list),
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
pub async fn get_trakt_lists_page_size() -> Result<u32, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .get_trakt_page_size()
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn get_trakt_list_form(id: i64) -> Result<Option<TraktListForm>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .get_trakt_list(id)
        .await
        .map_err(ServerFnError::new)
        .map(|form| {
            form.map(|form| TraktListForm {
                id: form.id,
                slug: format!("{}/{}", form.user, form.list),
                auto_refresh: form.auto_refresh,
                generate_playlist: form.generate_playlist,
            })
        })
}

#[server]
pub async fn add_trakt_list(input: String) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    add_trakt_list_with_state(&state, input)
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn match_trakt_list(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    match_trakt_list_with_state(&state, id)
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn update_trakt_list(form: TraktListForm) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    update_trakt_list_with_state(&state, form)
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn delete_trakt_list(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    delete_trakt_list_with_state(&state, id)
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub async fn add_trakt_list_with_state(
    state: &crate::host::state::AppState,
    input: String,
) -> ersatzrs_app::Result<i64> {
    state
        .application
        .add_trakt_list(ersatzrs_app::commands::AddTraktList { input })
        .await
}

#[cfg(feature = "ssr")]
pub async fn match_trakt_list_with_state(
    state: &crate::host::state::AppState,
    id: i64,
) -> ersatzrs_app::Result<()> {
    state
        .application
        .match_trakt_list(ersatzrs_app::commands::MatchTraktList { id })
        .await
}

#[cfg(feature = "ssr")]
pub async fn update_trakt_list_with_state(
    state: &crate::host::state::AppState,
    form: TraktListForm,
) -> ersatzrs_app::Result<()> {
    state
        .application
        .update_trakt_list(ersatzrs_app::commands::UpdateTraktList {
            id: form.id,
            auto_refresh: form.auto_refresh,
            generate_playlist: form.generate_playlist,
        })
        .await
}

#[cfg(feature = "ssr")]
pub async fn delete_trakt_list_with_state(
    state: &crate::host::state::AppState,
    id: i64,
) -> ersatzrs_app::Result<()> {
    state
        .application
        .delete_trakt_list(ersatzrs_app::commands::DeleteTraktList { id })
        .await
}
