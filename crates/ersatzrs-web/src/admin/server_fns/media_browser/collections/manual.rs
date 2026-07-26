#[cfg(feature = "ssr")]
use super::super::{expect_context, CollectionForm, CollectionListRow, ServerFnError};

// ---------------------------------------------------------------------------
// Manual collections
// ---------------------------------------------------------------------------

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn list_collections_impl(
    search: String,
    page: u32,
    page_size: u32,
) -> Result<(Vec<CollectionListRow>, u64), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let (rows, total) = state
        .application
        .list_collections(ersatzrs_app::queries::ListCollections {
            search,
            page,
            page_size,
        })
        .await
        .map_err(ServerFnError::new)?;
    Ok((
        rows.into_iter()
            .map(|row| CollectionListRow {
                id: row.id,
                name: row.name,
                query: None,
            })
            .collect(),
        total,
    ))
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn get_collection_impl(
    id: i64,
) -> Result<Option<CollectionForm>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
        .application
        .get_collection(id)
        .await
        .map_err(ServerFnError::new)?
        .map(|row| CollectionForm {
            id: row.id,
            name: row.name,
            query: String::new(),
        }))
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn save_collection_impl(
    form: CollectionForm,
) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    save_collection_with_state(&state, form).await
}

#[cfg(feature = "ssr")]
pub async fn save_collection_with_state(
    state: &crate::host::state::AppState,
    form: CollectionForm,
) -> Result<i64, ServerFnError> {
    state
        .application
        .save_collection(ersatzrs_app::commands::SaveCollection {
            id: form.id,
            name: form.name,
        })
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn delete_collection_impl(
    id: i64,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    state
        .application
        .delete_collection(ersatzrs_app::commands::DeleteCollection { id })
        .await
        .map_err(ServerFnError::new)
}
