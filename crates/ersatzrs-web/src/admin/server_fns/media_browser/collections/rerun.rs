#[cfg(feature = "ssr")]
use super::super::{
    expect_context, RerunCollectionEditorOptions, RerunCollectionForm, RerunCollectionListRow,
    RerunCollectionSelectOption, ServerFnError,
};

// ---------------------------------------------------------------------------
// Rerun collections
// ---------------------------------------------------------------------------

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn get_rerun_collections_page_size_impl(
) -> Result<u32, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .get_rerun_collections_page_size()
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn list_rerun_collections_impl(
    search: String,
    page: u32,
    page_size: u32,
) -> Result<(Vec<RerunCollectionListRow>, u64), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let (rows, total) = state
        .application
        .list_rerun_collections(ersatzrs_app::queries::ListCollections {
            search,
            page,
            page_size,
        })
        .await
        .map_err(ServerFnError::new)?;
    Ok((
        rows.into_iter()
            .map(|row| RerunCollectionListRow {
                id: row.id,
                name: row.name,
            })
            .collect(),
        total,
    ))
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn get_rerun_collection_form_impl(
    id: i64,
) -> Result<Option<RerunCollectionForm>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
        .application
        .get_rerun_collection(id)
        .await
        .map_err(ServerFnError::new)?
        .map(rerun_collection_form))
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn get_rerun_collection_editor_options_impl(
) -> Result<RerunCollectionEditorOptions, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let options = state
        .application
        .get_rerun_collection_editor_options()
        .await
        .map_err(ServerFnError::new)?;
    Ok(rerun_collection_editor_options(options))
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) fn rerun_collection_form(
    form: ersatzrs_core::repositories::RerunCollectionAdmin,
) -> RerunCollectionForm {
    RerunCollectionForm {
        id: form.id,
        name: form.name,
        collection_type: form.collection_type,
        collection_id: form.collection_id,
        media_item_id: form.media_item_id,
        multi_collection_id: form.multi_collection_id,
        smart_collection_id: form.smart_collection_id,
        first_run_playback_order: form.first_run_playback_order,
        rerun_playback_order: form.rerun_playback_order,
    }
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) fn rerun_collection_admin(
    form: RerunCollectionForm,
) -> ersatzrs_core::repositories::RerunCollectionAdmin {
    ersatzrs_core::repositories::RerunCollectionAdmin {
        id: form.id,
        name: form.name,
        collection_type: form.collection_type,
        collection_id: form.collection_id,
        media_item_id: form.media_item_id,
        multi_collection_id: form.multi_collection_id,
        smart_collection_id: form.smart_collection_id,
        first_run_playback_order: form.first_run_playback_order,
        rerun_playback_order: form.rerun_playback_order,
    }
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) fn rerun_collection_editor_options(
    options: ersatzrs_core::repositories::RerunCollectionEditorOptions,
) -> RerunCollectionEditorOptions {
    fn map(
        rows: Vec<ersatzrs_core::repositories::RerunCollectionSelectOption>,
    ) -> Vec<RerunCollectionSelectOption> {
        rows.into_iter()
            .map(|row| RerunCollectionSelectOption {
                id: row.id,
                name: row.name,
            })
            .collect()
    }

    RerunCollectionEditorOptions {
        collections: map(options.collections),
        multi_collections: map(options.multi_collections),
        smart_collections: map(options.smart_collections),
        shows: map(options.shows),
        seasons: map(options.seasons),
        artists: map(options.artists),
    }
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn save_rerun_collection_impl(
    form: RerunCollectionForm,
) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    save_rerun_collection_with_state(&state, form).await
}

#[cfg(feature = "ssr")]
pub async fn save_rerun_collection_with_state(
    state: &crate::host::state::AppState,
    form: RerunCollectionForm,
) -> Result<i64, ServerFnError> {
    state
        .application
        .save_rerun_collection(ersatzrs_app::commands::SaveRerunCollection {
            form: rerun_collection_admin(form),
        })
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn delete_rerun_collection_impl(
    id: i64,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .delete_rerun_collection(ersatzrs_app::commands::DeleteRerunCollection { id })
        .await
        .map_err(ServerFnError::new)
}
