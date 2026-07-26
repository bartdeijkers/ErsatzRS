#[cfg(feature = "ssr")]
use super::super::{
    expect_context, MultiCollectionEditorOptions, MultiCollectionForm, MultiCollectionItemForm,
    MultiCollectionListRow, MultiCollectionSelectOption, ServerFnError,
};

// ---------------------------------------------------------------------------
// Multi-collections
// ---------------------------------------------------------------------------

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn get_multi_collections_page_size_impl(
) -> Result<u32, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .get_multi_collections_page_size()
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn list_multi_collections_impl(
    search: String,
    page: u32,
    page_size: u32,
) -> Result<(Vec<MultiCollectionListRow>, u64), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let (rows, total) = state
        .application
        .list_multi_collections(ersatzrs_app::queries::ListCollections {
            search,
            page,
            page_size,
        })
        .await
        .map_err(ServerFnError::new)?;
    Ok((
        rows.into_iter()
            .map(|row| MultiCollectionListRow {
                id: row.id,
                name: row.name,
            })
            .collect(),
        total,
    ))
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn get_multi_collection_impl(
    id: i64,
) -> Result<Option<MultiCollectionForm>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
        .application
        .get_multi_collection(id)
        .await
        .map_err(ServerFnError::new)?
        .map(multi_collection_form))
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn get_multi_collection_editor_options_impl(
) -> Result<MultiCollectionEditorOptions, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let options = state
        .application
        .get_multi_collection_editor_options()
        .await
        .map_err(ServerFnError::new)?;
    Ok(MultiCollectionEditorOptions {
        collections: options
            .collections
            .into_iter()
            .map(multi_collection_select_option)
            .collect(),
        smart_collections: options
            .smart_collections
            .into_iter()
            .map(multi_collection_select_option)
            .collect(),
    })
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) fn multi_collection_form(
    form: ersatzrs_core::repositories::MultiCollectionAdmin,
) -> MultiCollectionForm {
    MultiCollectionForm {
        id: form.id,
        name: form.name,
        items: form
            .items
            .into_iter()
            .map(multi_collection_item_form)
            .collect(),
    }
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) fn multi_collection_item_form(
    item: ersatzrs_core::repositories::MultiCollectionItem,
) -> MultiCollectionItemForm {
    MultiCollectionItemForm {
        collection_id: item.collection_id,
        smart_collection_id: item.smart_collection_id,
        name: item.name,
        collection_type: item.collection_type,
        use_custom_playback_order: item.use_custom_playback_order,
        schedule_as_group: item.schedule_as_group,
        playback_order: item.playback_order,
    }
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) fn multi_collection_item(
    item: MultiCollectionItemForm,
) -> ersatzrs_core::repositories::MultiCollectionItem {
    ersatzrs_core::repositories::MultiCollectionItem {
        collection_id: item.collection_id,
        smart_collection_id: item.smart_collection_id,
        name: item.name,
        collection_type: item.collection_type,
        use_custom_playback_order: item.use_custom_playback_order,
        schedule_as_group: item.schedule_as_group,
        playback_order: item.playback_order,
    }
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) fn multi_collection_select_option(
    option: ersatzrs_core::repositories::MultiCollectionSelectOption,
) -> MultiCollectionSelectOption {
    MultiCollectionSelectOption {
        id: option.id,
        name: option.name,
        collection_type: option.collection_type,
        use_custom_playback_order: option.use_custom_playback_order,
    }
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn save_multi_collection_impl(
    form: MultiCollectionForm,
) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    save_multi_collection_with_state(&state, form).await
}

#[cfg(feature = "ssr")]
pub async fn save_multi_collection_with_state(
    state: &crate::host::state::AppState,
    form: MultiCollectionForm,
) -> Result<i64, ServerFnError> {
    state
        .application
        .save_multi_collection(ersatzrs_app::commands::SaveMultiCollection {
            id: form.id,
            name: form.name,
            items: form.items.into_iter().map(multi_collection_item).collect(),
        })
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn delete_multi_collection_impl(
    id: i64,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .delete_multi_collection(ersatzrs_app::commands::DeleteMultiCollection { id })
        .await
        .map_err(ServerFnError::new)
}
