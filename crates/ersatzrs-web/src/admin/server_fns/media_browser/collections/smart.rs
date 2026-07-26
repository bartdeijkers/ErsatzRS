#[cfg(feature = "ssr")]
use super::super::{expect_context, CollectionForm, CollectionListRow, ServerFnError};

// ---------------------------------------------------------------------------
// Smart collections
// ---------------------------------------------------------------------------

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn list_smart_collections_impl(
    search: String,
    page: u32,
    page_size: u32,
) -> Result<(Vec<CollectionListRow>, u64), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let (rows, total) = state
        .application
        .list_smart_collections(ersatzrs_app::queries::ListCollections {
            search,
            page,
            page_size,
        })
        .await
        .map_err(ServerFnError::new)?;
    Ok((smart_collection_list_rows(rows), total))
}

#[cfg(all(test, feature = "ssr"))]
pub async fn list_smart_collections_with_pool(
    pool: &ersatzrs_infra::db::DbPool,
    search: String,
    page: u32,
    page_size: u32,
) -> Result<(Vec<CollectionListRow>, u64), ServerFnError> {
    let repository = ersatzrs_infra::repositories::SqliteCollectionRepository(pool.clone());
    let (rows, total) = ersatzrs_app::queries::list_smart_collections(
        ersatzrs_app::queries::ListCollections {
            search,
            page,
            page_size,
        },
        &repository,
    )
    .await
    .map_err(ServerFnError::new)?;
    Ok((smart_collection_list_rows(rows), total))
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) fn smart_collection_list_rows(
    rows: Vec<ersatzrs_core::repositories::SmartCollectionAdminRow>,
) -> Vec<CollectionListRow> {
    rows.into_iter()
        .map(|row| CollectionListRow {
            id: row.id,
            name: row.name,
            query: Some(row.query),
        })
        .collect()
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn get_smart_collection_impl(
    id: i64,
) -> Result<Option<CollectionForm>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
        .application
        .get_smart_collection(id)
        .await
        .map_err(ServerFnError::new)?
        .map(|row| CollectionForm {
            id: row.id,
            name: row.name,
            query: row.query,
        }))
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn save_smart_collection_impl(
    form: CollectionForm,
) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    save_smart_collection_with_state(&state, form).await
}

#[cfg(feature = "ssr")]
pub async fn save_smart_collection_with_state(
    state: &crate::host::state::AppState,
    form: CollectionForm,
) -> Result<i64, ServerFnError> {
    state
        .application
        .save_smart_collection(ersatzrs_app::commands::SaveSmartCollection {
            id: form.id,
            name: form.name,
            query: form.query,
        })
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn delete_smart_collection_impl(
    id: i64,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    state
        .application
        .delete_smart_collection(ersatzrs_app::commands::DeleteSmartCollection { id })
        .await
        .map_err(ServerFnError::new)
}
