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

#[server]
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

#[server]
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

#[server]
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
/// On update: renames the library and replaces all path rows (delete-all + reinsert).
#[server]
pub async fn save_local_library(form: LocalLibraryForm) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
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
        .await?)
}

#[cfg(all(test, feature = "ssr"))]
pub(crate) async fn save_local_library_with_pool(
    pool: &ersatzrs_infra::db::DbPool,
    form: LocalLibraryForm,
) -> Result<i64, ServerFnError> {
    let normalized_paths = normalize_local_library_paths(&form.paths)?;

    let mut tx = pool.begin().await?;

    if form.id == 0 {
        let ms = sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (NULL)"#)
            .execute(&mut *tx)
            .await?;
        let ms_id = ersatzrs_infra::db::last_insert_id(ms);

        sqlx::query(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (?)"#)
            .bind(ms_id)
            .execute(&mut *tx)
            .await?;

        let lib = sqlx::query(
            r#"INSERT INTO "Library" ("MediaSourceId", "MediaKind", "Name") VALUES (?, ?, ?)"#,
        )
        .bind(ms_id)
        .bind(form.media_kind)
        .bind(&form.name)
        .execute(&mut *tx)
        .await?;
        let lib_id = ersatzrs_infra::db::last_insert_id(lib);

        sqlx::query(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (?)"#)
            .bind(lib_id)
            .execute(&mut *tx)
            .await?;

        for trimmed in &normalized_paths {
            sqlx::query(r#"INSERT INTO "LibraryPath" ("LibraryId", "Path") VALUES (?, ?)"#)
                .bind(lib_id)
                .bind(trimmed)
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;
        Ok(lib_id)
    } else {
        sqlx::query(r#"UPDATE "Library" SET "Name" = ? WHERE "Id" = ?"#)
            .bind(&form.name)
            .bind(form.id)
            .execute(&mut *tx)
            .await?;

        // Replace all paths: delete then reinsert the full deduplicated list.
        sqlx::query(r#"DELETE FROM "LibraryPath" WHERE "LibraryId" = ?"#)
            .bind(form.id)
            .execute(&mut *tx)
            .await?;

        for trimmed in &normalized_paths {
            sqlx::query(r#"INSERT INTO "LibraryPath" ("LibraryId", "Path") VALUES (?, ?)"#)
                .bind(form.id)
                .bind(trimmed)
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;
        Ok(form.id)
    }
}

#[cfg(test)]
fn normalize_local_library_paths(paths: &[LocalLibraryPath]) -> Result<Vec<String>, ServerFnError> {
    let mut seen = std::collections::HashSet::new();
    let mut normalized = Vec::new();

    for path in paths {
        let Some(candidate) = normalize_local_library_path(&path.path) else {
            continue;
        };

        if !std::path::Path::new(&candidate).is_dir() {
            return Err(ServerFnError::new(format!(
                "Path does not exist: {candidate}"
            )));
        }

        if seen.insert(candidate.clone()) {
            normalized.push(candidate);
        }
    }

    Ok(normalized)
}

#[cfg(test)]
fn normalize_local_library_path(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let uri_path = trimmed
        .strip_prefix("file://localhost")
        .or_else(|| trimmed.strip_prefix("file://"))
        .unwrap_or(trimmed);
    let decoded = urlencoding::decode(uri_path)
        .map_or_else(|_| uri_path.to_string(), std::borrow::Cow::into_owned);
    let path = std::path::Path::new(&decoded);
    let normalized = dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let text = normalized.to_string_lossy();
    let text = text.trim_end_matches(['/', '\\']);

    (!text.is_empty()).then(|| text.to_string())
}

#[server]
pub async fn delete_local_library(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state.application.delete_local_library(id).await?)
}

// ─── Jellyfin ─────────────────────────────────────────────────────────────────

#[server]
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
#[server]
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
#[server]
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

#[server]
pub async fn delete_jellyfin_source(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state.application.delete_remote_source(id).await?)
}

#[server]
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

#[server]
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

#[server]
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

#[server]
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

#[server]
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

#[server]
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

#[server]
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

#[server]
pub async fn delete_emby_source(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state.application.delete_remote_source(id).await?)
}

#[server]
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

#[server]
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

#[server]
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

#[server]
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
}

#[server]
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
        })
        .collect())
}

#[server]
pub async fn has_plex_token() -> Result<bool, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state.application.has_plex_token().await?)
}

#[server]
pub async fn sign_out_plex() -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state.application.sign_out_plex().await?)
}

/// Begins a Plex PIN auth flow. Returns `(pin_id, auth_url)`.
///
/// Calls `IPlexTvApiClient.StartPinFlow()` equivalent in infra, stores the
/// pending pin code in `ConfigElement` for later polling.
#[server]
pub async fn start_plex_pin_flow() -> Result<(String, String), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    Ok(state.application.start_plex_pin_flow().await?)
}

/// Polls Plex for PIN completion. Returns `Some(token)` when the user has
/// authorized, `None` if still pending.
///
/// On success, calls `PlexService::get_servers` (mirrors
/// `SynchronizePlexMediaSources`) to discover and upsert PlexMediaSource rows.
#[server]
pub async fn check_plex_pin(pin_id: String) -> Result<Option<String>, ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    Ok(state.application.check_plex_pin(&pin_id).await?)
}

#[server]
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

#[server]
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

#[server]
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

#[server]
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

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;
    use sqlx::Row;

    #[tokio::test]
    async fn local_library_save_normalizes_and_persists_existing_paths() {
        let database_dir = tempfile::tempdir().expect("temp database dir");
        let database_url = format!(
            "sqlite:{}",
            database_dir.path().join("local-library.db").display()
        );
        let pool = ersatzrs_infra::db::init(&database_url)
            .await
            .expect("initialize test database");
        let media_dir = tempfile::tempdir().expect("temp media dir");
        let raw_path = format!("{}/", media_dir.path().display());

        let id = save_local_library_with_pool(
            &pool,
            LocalLibraryForm {
                id: 0,
                name: "Movies".to_string(),
                media_kind: 1,
                paths: vec![
                    LocalLibraryPath {
                        id: 0,
                        path: raw_path,
                    },
                    LocalLibraryPath {
                        id: 0,
                        path: format!("file://{}", media_dir.path().display()),
                    },
                ],
            },
        )
        .await
        .expect("save local library");

        let paths = sqlx::query(r#"SELECT "Path" FROM "LibraryPath" WHERE "LibraryId" = ?"#)
            .bind(id)
            .fetch_all(&pool)
            .await
            .expect("load paths");

        assert_eq!(paths.len(), 1);
        assert_eq!(
            paths[0].try_get::<String, _>("Path").expect("path"),
            dunce::canonicalize(media_dir.path())
                .expect("canonical path")
                .display()
                .to_string()
        );
    }

    #[test]
    fn local_library_path_normalization_accepts_file_uri_input() {
        assert_eq!(
            normalize_local_library_path("file:///library/downloads/movies/"),
            Some("/library/downloads/movies".to_string())
        );
    }
}
