#[cfg(feature = "ssr")]
use super::{expect_context, ServerFnError};
use super::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Autocomplete for collection editors
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MediaItemSuggestion {
    pub id: i64,
    pub title: String,
    /// "movie" | "show"
    pub kind: String,
}

/// Debounced autocomplete. Returns up to 20 movies+shows matching the prefix.
#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn suggest_media_items_impl(
    prefix: String,
) -> Result<Vec<MediaItemSuggestion>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
        .application
        .suggest_media_items(&prefix)
        .await
        .map_err(ServerFnError::new)?
        .into_iter()
        .map(|row| MediaItemSuggestion {
            id: row.id,
            title: row.title,
            kind: row.kind,
        })
        .collect())
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn add_item_to_collection_impl(
    collection_id: i64,
    media_item_id: i64,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    state
        .application
        .add_default_collection_item(ersatzrs_app::commands::ChangeCollectionItem {
            collection_id,
            media_item_id,
        })
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn remove_item_from_collection_impl(
    collection_id: i64,
    media_item_id: i64,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    state
        .application
        .remove_collection_item(ersatzrs_app::commands::ChangeCollectionItem {
            collection_id,
            media_item_id,
        })
        .await
        .map_err(ServerFnError::new)
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CollectionMember {
    pub media_item_id: i64,
    pub title: String,
    pub kind: String,
}

// ---------------------------------------------------------------------------
// Multi-select: add selected media items to a collection or playlist.
//
// Ports `ErsatzTV.Application.MediaCollections.AddItemsToCollectionHandler`
// (collection branch) and `AddItemsToPlaylistHandler` (playlist branch).
// The MovieList page only ever passes movie ids, but the underlying tables
// are media-item agnostic; we accept a typed kind so future ports of
// `ShowList`, `OtherVideoList`, etc. can reuse these endpoints unchanged.
// ---------------------------------------------------------------------------

/// Row for the schedule dropdown in the show "Add To Schedule" dialog.
/// Mirrors the id + name subset of
/// `ErsatzTV.Application.ProgramSchedules.ProgramScheduleViewModel`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ScheduleOption {
    pub id: i64,
    pub name: String,
}

/// All program schedules, ordered by name. Mirrors
/// `GetAllProgramSchedulesHandler` (every row from `ProgramSchedule`).
#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn list_program_schedules_impl(
) -> Result<Vec<ScheduleOption>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
        .application
        .list_all_schedules()
        .await?
        .into_iter()
        .map(|row| ScheduleOption {
            id: row.id,
            name: row.name,
        })
        .collect())
}

/// Adds a show to a program schedule as a single `PlayoutMode.One` item.
/// Ports `AddProgramScheduleItem.ForMediaItem(scheduleId,
/// CollectionType.TelevisionShow, showId)` +
/// `AddProgramScheduleItemHandler` / `ProgramScheduleItemCommandBase`.
///
#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn add_show_to_schedule_impl(
    schedule_id: i64,
    show_id: i64,
) -> Result<(), ServerFnError> {
    const COLLECTION_TYPE_TELEVISION_SHOW: i64 = 1;

    append_media_item_to_schedule(schedule_id, COLLECTION_TYPE_TELEVISION_SHOW, show_id).await
}

/// Adds any media item to a program schedule as a single `PlayoutMode.One`
/// item. Ports `AddProgramScheduleItem.ForMediaItem`.
#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn add_media_item_to_schedule_impl(
    schedule_id: i64,
    collection_type: i64,
    media_item_id: i64,
) -> Result<(), ServerFnError> {
    append_media_item_to_schedule(schedule_id, collection_type, media_item_id).await
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn append_media_item_to_schedule(
    schedule_id: i64,
    collection_type: i64,
    media_item_id: i64,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
        .application
        .append_schedule_media_item(ersatzrs_app::commands::AppendScheduleMediaItem {
            schedule_id,
            collection_type: i32::try_from(collection_type)
                .map_err(|error| ServerFnError::new(error.to_string()))?,
            media_item_id,
        })
        .await?)
}

/// Row for the "(New Collection)" / existing-collection dropdown in
/// `AddToCollectionDialog`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CollectionOption {
    pub id: i64,
    pub name: String,
}

/// Row for the "Playlist Group" dropdown in `AddToPlaylistDialog`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PlaylistGroupOption {
    pub id: i64,
    pub name: String,
}

/// Row for the "Playlist" dropdown in `AddToPlaylistDialog`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PlaylistOption {
    pub id: i64,
    pub name: String,
    pub playlist_group_id: i64,
}

/// All collections, ordered by name. Mirrors `GetAllCollectionsHandler`,
/// which loads every row from `Collection`.
#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn list_all_collections_impl(
) -> Result<Vec<CollectionOption>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
        .application
        .list_all_collections()
        .await
        .map_err(ServerFnError::new)?
        .into_iter()
        .map(|row| CollectionOption {
            id: row.id,
            name: row.name,
        })
        .collect())
}

/// All smart collections, ordered by name. Used by the search page's
/// Save-As-Smart-Collection dialog.
#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn list_smart_collections_for_select_impl(
) -> Result<Vec<CollectionOption>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let (rows, _) = state
        .application
        .list_smart_collections(ersatzrs_app::queries::ListCollections {
            search: String::new(),
            page: 0,
            page_size: u32::MAX,
        })
        .await
        .map_err(ServerFnError::new)?;
    Ok(rows
        .into_iter()
        .map(|row| CollectionOption {
            id: row.id,
            name: row.name,
        })
        .collect())
}

/// Creates a new collection with the given name and returns it. Mirrors
/// `CreateCollectionHandler`.
#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn create_collection_impl(
    name: String,
) -> Result<CollectionOption, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let trimmed = name.trim().to_owned();
    let id = state
        .application
        .save_collection(ersatzrs_app::commands::SaveCollection {
            id: 0,
            name: trimmed.clone(),
        })
        .await
        .map_err(ServerFnError::new)?;
    Ok(CollectionOption { id, name: trimmed })
}
