use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteStreamPlaylistEntryRow {
    pub id: String,
    pub title: String,
    pub duration_seconds: Option<u64>,
    pub state: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteStreamPlaylistRow {
    pub library_id: i64,
    pub root_index: usize,
    pub relative_path: String,
    pub name: String,
    pub url: String,
    pub extractor: String,
    pub folder: String,
    pub interval: String,
    pub max_items: usize,
    pub order: String,
    pub on_removed: String,
    pub create_playlist: bool,
    pub tags: Vec<String>,
    pub state: String,
    pub last_sync: Option<String>,
    pub item_count: usize,
    pub truncated: bool,
    pub managed_by_ersatzrs: bool,
    pub entries: Vec<RemoteStreamPlaylistEntryRow>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteStreamPlaylistEditor {
    pub library_id: i64,
    pub root_index: usize,
    pub relative_path: Option<String>,
    pub name: String,
    pub url: String,
    pub extractor: String,
    pub folder: String,
    pub interval: String,
    pub max_items: usize,
    pub order: String,
    pub on_removed: String,
    pub create_playlist: bool,
    pub tags: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaylistTroubleshootingResult {
    pub entry_count: usize,
    pub diagnostic_tail: String,
}

#[server]
pub async fn list_remote_stream_playlists(
    library_id: i64,
) -> Result<Vec<RemoteStreamPlaylistRow>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let rows = state
        .application
        .list_remote_stream_playlists(library_id)
        .await
        .map_err(ServerFnError::new)?;
    Ok(rows
        .into_iter()
        .map(RemoteStreamPlaylistRow::from)
        .collect())
}

#[server]
pub async fn list_remote_stream_playlist_extractors() -> Result<Vec<String>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .list_remote_stream_playlist_extractors()
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn save_remote_stream_playlist(
    editor: RemoteStreamPlaylistEditor,
) -> Result<String, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let editor = ersatzrs_app::RemoteStreamPlaylistEdit {
        library_id: editor.library_id,
        root_index: editor.root_index,
        relative_path: editor.relative_path,
        name: editor.name,
        url: editor.url,
        extractor: editor.extractor,
        folder: editor.folder,
        interval: editor.interval,
        max_items: editor.max_items,
        order: editor.order,
        on_removed: editor.on_removed,
        create_playlist: editor.create_playlist,
        tags: editor.tags,
    };
    state
        .application
        .save_remote_stream_playlist(editor)
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn delete_remote_stream_playlist(
    library_id: i64,
    root_index: usize,
    relative_path: String,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .delete_remote_stream_playlist(library_id, root_index, relative_path)
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn exclude_remote_stream_playlist_entry(
    library_id: i64,
    root_index: usize,
    relative_path: String,
    entry_id: String,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .exclude_remote_stream_playlist_entry(library_id, root_index, relative_path, entry_id)
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn troubleshoot_remote_stream_playlist(
    library_id: i64,
    root_index: usize,
    relative_path: String,
) -> Result<PlaylistTroubleshootingResult, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let result = state
        .application
        .troubleshoot_remote_stream_playlist(library_id, root_index, relative_path)
        .await
        .map_err(ServerFnError::new)?;
    Ok(PlaylistTroubleshootingResult {
        entry_count: result.entry_count,
        diagnostic_tail: result.diagnostic_tail,
    })
}

#[cfg(not(target_arch = "wasm32"))]
impl From<ersatzrs_app::RemoteStreamPlaylistEntryRead> for RemoteStreamPlaylistEntryRow {
    fn from(value: ersatzrs_app::RemoteStreamPlaylistEntryRead) -> Self {
        Self {
            id: value.id,
            title: value.title,
            duration_seconds: value.duration_seconds,
            state: value.state,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<ersatzrs_app::RemoteStreamPlaylistRead> for RemoteStreamPlaylistRow {
    fn from(value: ersatzrs_app::RemoteStreamPlaylistRead) -> Self {
        Self {
            library_id: value.library_id,
            root_index: value.root_index,
            relative_path: value.relative_path,
            name: value.name,
            url: value.url,
            extractor: value.extractor,
            folder: value.folder,
            interval: value.interval,
            max_items: value.max_items,
            order: value.order,
            on_removed: value.on_removed,
            create_playlist: value.create_playlist,
            tags: value.tags,
            state: value.state,
            last_sync: value.last_sync,
            item_count: value.item_count,
            truncated: value.truncated,
            managed_by_ersatzrs: value.managed_by_ersatzrs,
            entries: value.entries.into_iter().map(Into::into).collect(),
        }
    }
}
