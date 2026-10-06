#![cfg_attr(test, allow(dead_code))]

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

// ─── Shared types ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LocalLibraryRow {
    pub id: i64,
    pub name: String,
    pub media_kind: i32,
    pub path_count: i64,
    pub managed_by_addon: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ManagedAddonMediaSourceRow {
    pub addon_id: String,
    pub name: String,
    pub root_path: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LocalLibraryPath {
    pub id: i64,
    pub path: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LocalLibraryForm {
    pub id: i64,
    pub name: String,
    /// DB integer: 1=Movies 2=Shows 3=MusicVideos 4=OtherVideos 5=Songs 6=Images 7=RemoteStreams
    pub media_kind: i32,
    pub paths: Vec<LocalLibraryPath>,
}

impl LocalLibraryForm {
    #[must_use]
    pub fn new_defaults() -> Self {
        Self {
            name: "New Local Library".to_string(),
            media_kind: 1,
            ..Default::default()
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RemoteSourceRow {
    pub id: i64,
    pub name: String,
    pub address: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ConnectionForm {
    /// 0 = create new
    pub source_id: i64,
    pub address: String,
    pub api_key: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LibraryPrefRow {
    pub id: i64,
    pub name: String,
    pub media_kind: i32,
    pub should_sync: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PathReplacementRow {
    pub id: i64,
    pub remote_path: String,
    pub local_path: String,
}

// ─── Local Libraries ──────────────────────────────────────────────────────────

#[server(prefix = "/admin-api")]
pub async fn list_local_libraries() -> Result<Vec<LocalLibraryRow>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
        .application
        .list_local_libraries()
        .await?
        .into_iter()
        .map(|row| LocalLibraryRow {
            id: row.id,
            name: row.name,
            media_kind: row.media_kind,
            path_count: i64::try_from(row.path_count).unwrap_or(i64::MAX),
            managed_by_addon: row.managed_by_addon,
        })
        .collect())
}

#[server(prefix = "/admin-api")]
pub async fn list_managed_addon_media_sources(
) -> Result<Vec<ManagedAddonMediaSourceRow>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
        .application
        .list_managed_addon_media_sources()
        .await?
        .into_iter()
        .map(|row| ManagedAddonMediaSourceRow {
            addon_id: row.addon_id,
            name: row.name,
            root_path: row.root_path,
        })
        .collect())
}

#[server(prefix = "/admin-api")]
pub async fn get_local_library_form(id: i64) -> Result<LocalLibraryForm, ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    let row = state
        .application
        .get_local_library(id)
        .await?
        .ok_or_else(|| ServerFnError::new(format!("Local library {id} was not found")))?;
    Ok(LocalLibraryForm {
        id: row.id,
        name: row.name,
        media_kind: row.media_kind,
        paths: row
            .paths
            .into_iter()
            .map(|path| LocalLibraryPath {
                id: path.id,
                path: path.path,
            })
            .collect(),
    })
}

/// Creates or updates a local library. Returns the library id.
///
/// On create: inserts MediaSource → LocalMediaSource → Library → LocalLibrary → LibraryPath rows.
/// On update: reconciles paths while retaining unchanged path and media identities.
#[server(prefix = "/admin-api")]
pub async fn save_local_library(form: LocalLibraryForm) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let language =
        super::settings::load_ui_language_for_api(state.application.ports().admin_settings())
            .await?;
    state
        .application
        .save_local_library(ersatzrs_core::repositories::LocalLibraryAdmin {
            id: form.id,
            name: form.name,
            media_kind: form.media_kind,
            paths: form
                .paths
                .into_iter()
                .map(|path| ersatzrs_core::repositories::LocalLibraryPathAdmin {
                    id: path.id,
                    path: path.path,
                })
                .collect(),
        })
        .await
        .map_err(|error| ServerFnError::new(local_library_error_text(language, &error)))
}

#[cfg(feature = "ssr")]
pub(crate) fn local_library_error_text(
    language: crate::admin::i18n::UiLanguage,
    error: &ersatzrs_app::ApplicationError,
) -> String {
    use crate::admin::i18n::{text, TextKey};
    if let ersatzrs_app::ApplicationError::Validation { field, message } = error {
        let key = match *field {
            "library_path_absolute" => TextKey::LocalLibraryPathAbsolute,
            "library_path_exists" => TextKey::LocalLibraryPathExists,
            "library_path_conflict" => TextKey::LocalLibraryPathConflict,
            _ => return text(language, TextKey::ErrorOccurred).to_owned(),
        };
        return text(language, key).replace("{path}", message);
    }
    text(language, TextKey::ErrorOccurred).to_owned()
}

#[server(prefix = "/admin-api")]
pub async fn delete_local_library(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state.application.delete_local_library(id).await?)
}

// ─── Jellyfin ─────────────────────────────────────────────────────────────────

#[server(prefix = "/admin-api")]
pub async fn list_jellyfin_sources() -> Result<Vec<RemoteSourceRow>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
        .application
        .list_remote_sources(ersatzrs_core::repositories::RemoteMediaSourceKind::Jellyfin)
        .await?
        .into_iter()
        .map(|row| RemoteSourceRow {
            id: row.id,
            name: row.name,
            address: row.address,
        })
        .collect())
}

/// Loads the current Jellyfin address + API key for the connection editor.
#[server(prefix = "/admin-api")]
pub async fn get_jellyfin_connection() -> Result<ConnectionForm, ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    let connection = state
        .application
        .get_remote_connection(ersatzrs_core::repositories::RemoteMediaSourceKind::Jellyfin)
        .await?
        .unwrap_or(ersatzrs_core::repositories::RemoteConnectionAdmin {
            source_id: 0,
            address: String::new(),
        });
    let api_key = state
        .application
        .get_string_setting("jellyfin.current_server_api_key", "")
        .await?;

    Ok(ConnectionForm {
        source_id: connection.source_id,
        address: connection.address,
        api_key,
    })
}

/// Creates or updates the Jellyfin connection and stores the API key.
///
/// Mirrors ErsatzTV's `SaveJellyfinSecretsHandler`: validates the API key by
/// calling `/System/Info` first, persists the connection with the real server
/// name, then triggers a background library list refresh + sync. If the
/// validation call fails the entire save is rejected so the user sees the
/// error in the form instead of silently storing a bad key.
#[server(prefix = "/admin-api")]
pub async fn save_jellyfin_connection(form: ConnectionForm) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    Ok(state
        .application
        .save_jellyfin_connection(
            ersatzrs_core::repositories::RemoteConnectionAdmin {
                source_id: form.source_id,
                address: form.address,
            },
            &form.api_key,
        )
        .await?)
}

#[server(prefix = "/admin-api")]
pub async fn delete_jellyfin_source(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state.application.delete_remote_source(id).await?)
}

#[server(prefix = "/admin-api")]
pub async fn list_jellyfin_libraries(source_id: i64) -> Result<Vec<LibraryPrefRow>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    list_library_preferences_with_application(
        &state,
        ersatzrs_core::repositories::RemoteMediaSourceKind::Jellyfin,
        source_id,
    )
    .await
}

#[server(prefix = "/admin-api")]
pub async fn save_jellyfin_library_prefs(prefs: Vec<LibraryPrefRow>) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    save_library_preferences_with_application(
        &state,
        ersatzrs_core::repositories::RemoteMediaSourceKind::Jellyfin,
        prefs,
    )
    .await
}

#[server(prefix = "/admin-api")]
pub async fn list_jellyfin_paths(source_id: i64) -> Result<Vec<PathReplacementRow>, ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    list_path_replacements_with_application(
        &state,
        ersatzrs_core::repositories::RemoteMediaSourceKind::Jellyfin,
        source_id,
    )
    .await
}

#[server(prefix = "/admin-api")]
pub async fn save_jellyfin_paths(
    source_id: i64,
    paths: Vec<PathReplacementRow>,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    save_path_replacements_with_application(
        &state,
        ersatzrs_core::repositories::RemoteMediaSourceKind::Jellyfin,
        source_id,
        paths,
    )
    .await
}

// ─── Emby ─────────────────────────────────────────────────────────────────────

#[server(prefix = "/admin-api")]
pub async fn list_emby_sources() -> Result<Vec<RemoteSourceRow>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
        .application
        .list_remote_sources(ersatzrs_core::repositories::RemoteMediaSourceKind::Emby)
        .await?
        .into_iter()
        .map(|row| RemoteSourceRow {
            id: row.id,
            name: row.name,
            address: row.address,
        })
        .collect())
}

#[server(prefix = "/admin-api")]
pub async fn get_emby_connection() -> Result<ConnectionForm, ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    let connection = state
        .application
        .get_remote_connection(ersatzrs_core::repositories::RemoteMediaSourceKind::Emby)
        .await?
        .unwrap_or(ersatzrs_core::repositories::RemoteConnectionAdmin {
            source_id: 0,
            address: String::new(),
        });
    let api_key = state
        .application
        .get_string_setting("emby.current_server_api_key", "")
        .await?;

    Ok(ConnectionForm {
        source_id: connection.source_id,
        address: connection.address,
        api_key,
    })
}

#[server(prefix = "/admin-api")]
pub async fn save_emby_connection(form: ConnectionForm) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();

    Ok(state
        .application
        .save_emby_connection(
            ersatzrs_core::repositories::RemoteConnectionAdmin {
                source_id: form.source_id,
                address: form.address,
            },
            &form.api_key,
        )
        .await?)
}

#[server(prefix = "/admin-api")]
pub async fn delete_emby_source(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state.application.delete_remote_source(id).await?)
}

#[server(prefix = "/admin-api")]
pub async fn list_emby_libraries(source_id: i64) -> Result<Vec<LibraryPrefRow>, ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    list_library_preferences_with_application(
        &state,
        ersatzrs_core::repositories::RemoteMediaSourceKind::Emby,
        source_id,
    )
    .await
}

#[server(prefix = "/admin-api")]
pub async fn save_emby_library_prefs(prefs: Vec<LibraryPrefRow>) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    save_library_preferences_with_application(
        &state,
        ersatzrs_core::repositories::RemoteMediaSourceKind::Emby,
        prefs,
    )
    .await
}

#[server(prefix = "/admin-api")]
pub async fn list_emby_paths(source_id: i64) -> Result<Vec<PathReplacementRow>, ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    list_path_replacements_with_application(
        &state,
        ersatzrs_core::repositories::RemoteMediaSourceKind::Emby,
        source_id,
    )
    .await
}

#[server(prefix = "/admin-api")]
pub async fn save_emby_paths(
    source_id: i64,
    paths: Vec<PathReplacementRow>,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    save_path_replacements_with_application(
        &state,
        ersatzrs_core::repositories::RemoteMediaSourceKind::Emby,
        source_id,
        paths,
    )
    .await
}

// ─── Plex ─────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PlexSourceRow {
    pub id: i64,
    pub name: String,
    pub address: String,
    pub missing_since: Option<String>,
}

#[server(prefix = "/admin-api")]
pub async fn list_plex_sources() -> Result<Vec<PlexSourceRow>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
        .application
        .list_remote_sources(ersatzrs_core::repositories::RemoteMediaSourceKind::Plex)
        .await?
        .into_iter()
        .map(|row| PlexSourceRow {
            id: row.id,
            name: row.name,
            address: row.address,
            missing_since: row.missing_since.map(|timestamp| timestamp.to_rfc3339()),
        })
        .collect())
}

#[server(prefix = "/admin-api")]
pub async fn has_plex_token() -> Result<bool, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state.application.has_plex_token().await?)
}

#[server(prefix = "/admin-api")]
pub async fn delete_missing_plex_source(source_id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    Ok(state
        .application
        .delete_missing_plex_source(source_id)
        .await?)
}

#[server(prefix = "/admin-api")]
pub async fn refresh_plex_libraries(source_id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    Ok(state.application.refresh_plex_libraries(source_id).await?)
}

#[server(prefix = "/admin-api")]
pub async fn sign_out_plex() -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state.application.sign_out_plex().await?)
}

/// Begins a Plex PIN auth flow. Returns `(pin_id, auth_url)`.
///
/// Stores a bounded, client-identity-bound pending flow without replacing
/// existing credentials until authorization succeeds.
#[server(prefix = "/admin-api")]
pub async fn start_plex_pin_flow(
    force_new_credentials: bool,
) -> Result<(String, String), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    Ok(state
        .application
        .start_plex_pin_flow(force_new_credentials)
        .await?)
}

/// Polls Plex for PIN completion. Returns only authorization status;
/// account tokens remain on the server.
///
/// On success, calls `PlexService::get_servers` (mirrors
/// `SynchronizePlexMediaSources`) to discover and upsert PlexMediaSource rows.
#[server(prefix = "/admin-api")]
pub async fn check_plex_pin(pin_id: String) -> Result<bool, ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    Ok(state.application.check_plex_pin(&pin_id).await?)
}

#[server(prefix = "/admin-api")]
pub async fn list_plex_libraries(source_id: i64) -> Result<Vec<LibraryPrefRow>, ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    list_library_preferences_with_application(
        &state,
        ersatzrs_core::repositories::RemoteMediaSourceKind::Plex,
        source_id,
    )
    .await
}

#[server(prefix = "/admin-api")]
pub async fn save_plex_library_prefs(prefs: Vec<LibraryPrefRow>) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    save_library_preferences_with_application(
        &state,
        ersatzrs_core::repositories::RemoteMediaSourceKind::Plex,
        prefs,
    )
    .await
}

#[server(prefix = "/admin-api")]
pub async fn list_plex_paths(source_id: i64) -> Result<Vec<PathReplacementRow>, ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    list_path_replacements_with_application(
        &state,
        ersatzrs_core::repositories::RemoteMediaSourceKind::Plex,
        source_id,
    )
    .await
}

#[server(prefix = "/admin-api")]
pub async fn save_plex_paths(
    source_id: i64,
    paths: Vec<PathReplacementRow>,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    save_path_replacements_with_application(
        &state,
        ersatzrs_core::repositories::RemoteMediaSourceKind::Plex,
        source_id,
        paths,
    )
    .await
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

#[cfg(feature = "ssr")]
async fn list_library_preferences_with_application(
    state: &crate::host::state::AppState,
    kind: ersatzrs_core::repositories::RemoteMediaSourceKind,
    source_id: i64,
) -> Result<Vec<LibraryPrefRow>, ServerFnError> {
    Ok(state
        .application
        .list_library_preferences(kind, source_id)
        .await?
        .into_iter()
        .map(|preference| LibraryPrefRow {
            id: preference.id,
            name: preference.name,
            media_kind: preference.media_kind,
            should_sync: preference.should_sync,
        })
        .collect())
}

#[cfg(feature = "ssr")]
async fn save_library_preferences_with_application(
    state: &crate::host::state::AppState,
    kind: ersatzrs_core::repositories::RemoteMediaSourceKind,
    preferences: Vec<LibraryPrefRow>,
) -> Result<(), ServerFnError> {
    let preferences = preferences
        .into_iter()
        .map(
            |preference| ersatzrs_core::repositories::LibraryPreferenceAdmin {
                id: preference.id,
                name: preference.name,
                media_kind: preference.media_kind,
                should_sync: preference.should_sync,
            },
        )
        .collect::<Vec<_>>();
    Ok(state
        .application
        .save_library_preferences_and_sync(kind, &preferences)
        .await?)
}

#[cfg(feature = "ssr")]
async fn list_path_replacements_with_application(
    state: &crate::host::state::AppState,
    kind: ersatzrs_core::repositories::RemoteMediaSourceKind,
    source_id: i64,
) -> Result<Vec<PathReplacementRow>, ServerFnError> {
    Ok(state
        .application
        .list_path_replacements(kind, source_id)
        .await?
        .into_iter()
        .map(|path| PathReplacementRow {
            id: path.id,
            remote_path: path.remote_path,
            local_path: path.local_path,
        })
        .collect())
}

#[cfg(feature = "ssr")]
async fn save_path_replacements_with_application(
    state: &crate::host::state::AppState,
    kind: ersatzrs_core::repositories::RemoteMediaSourceKind,
    source_id: i64,
    paths: Vec<PathReplacementRow>,
) -> Result<(), ServerFnError> {
    let paths = paths
        .into_iter()
        .map(|path| ersatzrs_core::repositories::PathReplacementAdmin {
            id: path.id,
            remote_path: path.remote_path,
            local_path: path.local_path,
        })
        .collect::<Vec<_>>();
    Ok(state
        .application
        .save_path_replacements(kind, source_id, &paths)
        .await?)
}

pub fn media_kind_label(kind: i32) -> &'static str {
    match kind {
        1 => "Movies",
        2 => "Shows",
        3 => "Music Videos",
        4 => "Other Videos",
        5 => "Songs",
        6 => "Images",
        7 => "Remote Streams",
        _ => "Unknown",
    }
}
