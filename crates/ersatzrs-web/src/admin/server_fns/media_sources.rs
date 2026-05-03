use leptos::prelude::*;
use serde::{Deserialize, Serialize};

// ─── Shared types ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LocalLibraryRow {
    pub id: i64,
    pub name: String,
    pub media_kind: i32,
    pub path_count: i64,
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
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let rows = sqlx::query(
        r#"SELECT l."Id", l."Name", l."MediaKind",
                  COUNT(lp."Id") AS "PathCount"
           FROM "Library" l
           JOIN "LocalLibrary" ll ON ll."Id" = l."Id"
           LEFT JOIN "LibraryPath" lp ON lp."LibraryId" = l."Id"
           GROUP BY l."Id", l."Name", l."MediaKind"
           ORDER BY l."Name""#,
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| LocalLibraryRow {
            id: r.try_get("Id").unwrap_or(0),
            name: r.try_get("Name").unwrap_or_default(),
            media_kind: r.try_get("MediaKind").unwrap_or(1),
            path_count: r.try_get("PathCount").unwrap_or(0),
        })
        .collect())
}

#[server]
pub async fn get_local_library_form(id: i64) -> Result<LocalLibraryForm, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let row = sqlx::query(
        r#"SELECT l."Id", l."Name", l."MediaKind"
           FROM "Library" l
           JOIN "LocalLibrary" ll ON ll."Id" = l."Id"
           WHERE l."Id" = ?"#,
    )
    .bind(id)
    .fetch_one(&state.pool)
    .await?;

    let path_rows = sqlx::query(
        r#"SELECT "Id", COALESCE("Path", '') AS "Path"
           FROM "LibraryPath"
           WHERE "LibraryId" = ?
           ORDER BY "Id""#,
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;

    Ok(LocalLibraryForm {
        id: row.try_get("Id").unwrap_or(0),
        name: row.try_get("Name").unwrap_or_default(),
        media_kind: row.try_get("MediaKind").unwrap_or(1),
        paths: path_rows
            .iter()
            .map(|r| LocalLibraryPath {
                id: r.try_get("Id").unwrap_or(0),
                path: r.try_get("Path").unwrap_or_default(),
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
    save_local_library_with_pool(&state.pool, form).await
}

#[cfg(feature = "ssr")]
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
    let normalized = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let text = normalized.to_string_lossy();
    let text = text.trim_end_matches(['/', '\\']);

    (!text.is_empty()).then(|| text.to_string())
}

#[server]
pub async fn delete_local_library(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    // Delete MediaSource; FK ON DELETE CASCADE propagates to Library → LocalLibrary + LibraryPath.
    sqlx::query(
        r#"DELETE FROM "MediaSource"
           WHERE "Id" = (SELECT "MediaSourceId" FROM "Library" WHERE "Id" = ?)"#,
    )
    .bind(id)
    .execute(&state.pool)
    .await?;
    Ok(())
}

// ─── Jellyfin ─────────────────────────────────────────────────────────────────

#[server]
pub async fn list_jellyfin_sources() -> Result<Vec<RemoteSourceRow>, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let rows = sqlx::query(
        r#"SELECT jms."Id",
                  COALESCE(jms."ServerName", 'Jellyfin') AS "Name",
                  COALESCE(
                      (SELECT "Address" FROM "JellyfinConnection"
                       WHERE "JellyfinMediaSourceId" = jms."Id"
                       ORDER BY "Id" LIMIT 1),
                      '') AS "Address"
           FROM "JellyfinMediaSource" jms
           ORDER BY jms."Id""#,
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| RemoteSourceRow {
            id: r.try_get("Id").unwrap_or(0),
            name: r.try_get("Name").unwrap_or_default(),
            address: r.try_get("Address").unwrap_or_default(),
        })
        .collect())
}

/// Loads the current Jellyfin address + API key for the connection editor.
#[server]
pub async fn get_jellyfin_connection() -> Result<ConnectionForm, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();

    let source_row = sqlx::query(
        r#"SELECT jms."Id",
                  COALESCE(
                      (SELECT "Address" FROM "JellyfinConnection"
                       WHERE "JellyfinMediaSourceId" = jms."Id"
                       ORDER BY "Id" LIMIT 1),
                      '') AS "Address"
           FROM "JellyfinMediaSource" jms
           ORDER BY jms."Id"
           LIMIT 1"#,
    )
    .fetch_optional(&state.pool)
    .await?;

    let (source_id, address) = source_row
        .as_ref()
        .map(|r| {
            (
                r.try_get::<i64, _>("Id").unwrap_or(0),
                r.try_get::<String, _>("Address").unwrap_or_default(),
            )
        })
        .unwrap_or_default();

    let api_key: String = sqlx::query_scalar(
        r#"SELECT COALESCE("Value", '') FROM "ConfigElement"
           WHERE "Key" = 'jellyfin.current_server_api_key'
           LIMIT 1"#,
    )
    .fetch_optional(&state.pool)
    .await?
    .unwrap_or_default();

    Ok(ConnectionForm {
        source_id,
        address,
        api_key,
    })
}

/// Creates or updates the Jellyfin connection and stores the API key.
#[server]
pub async fn save_jellyfin_connection(form: ConnectionForm) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();

    let source_id = if form.source_id == 0 {
        let ms = sqlx::query(r#"INSERT INTO "MediaSource" DEFAULT VALUES"#)
            .execute(&state.pool)
            .await?;
        let ms_id = ersatzrs_infra::db::last_insert_id(ms);

        sqlx::query(
            r#"INSERT INTO "JellyfinMediaSource" ("Id", "ServerName") VALUES (?, 'Jellyfin')"#,
        )
        .bind(ms_id)
        .execute(&state.pool)
        .await?;

        sqlx::query(
            r#"INSERT INTO "JellyfinConnection" ("Address", "JellyfinMediaSourceId") VALUES (?, ?)"#,
        )
        .bind(&form.address)
        .bind(ms_id)
        .execute(&state.pool)
        .await?;

        ms_id
    } else {
        sqlx::query(
            r#"UPDATE "JellyfinConnection" SET "Address" = ?
               WHERE "JellyfinMediaSourceId" = ?
               AND "Id" = (SELECT MIN("Id") FROM "JellyfinConnection"
                           WHERE "JellyfinMediaSourceId" = ?)"#,
        )
        .bind(&form.address)
        .bind(form.source_id)
        .bind(form.source_id)
        .execute(&state.pool)
        .await?;

        form.source_id
    };

    sqlx::query(
        r#"REPLACE INTO "ConfigElement" ("Key", "Value") VALUES ('jellyfin.current_server_api_key', ?)
           "#,
    )
    .bind(&form.api_key)
    .execute(&state.pool)
    .await?;

    Ok(source_id)
}

#[server]
pub async fn delete_jellyfin_source(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    sqlx::query(r#"DELETE FROM "MediaSource" WHERE "Id" = ?"#)
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(())
}

#[server]
pub async fn list_jellyfin_libraries(source_id: i64) -> Result<Vec<LibraryPrefRow>, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let rows = sqlx::query(
        r#"SELECT l."Id", COALESCE(l."Name", '') AS "Name", l."MediaKind", jl."ShouldSyncItems"
           FROM "Library" l
           JOIN "JellyfinLibrary" jl ON jl."Id" = l."Id"
           WHERE l."MediaSourceId" = ?
           ORDER BY l."MediaKind", l."Name""#,
    )
    .bind(source_id)
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| LibraryPrefRow {
            id: r.try_get("Id").unwrap_or(0),
            name: r.try_get("Name").unwrap_or_default(),
            media_kind: r.try_get("MediaKind").unwrap_or(1),
            should_sync: r.try_get::<bool, _>("ShouldSyncItems").unwrap_or(false),
        })
        .collect())
}

#[server]
pub async fn save_jellyfin_library_prefs(prefs: Vec<LibraryPrefRow>) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_infra::jellyfin::sync_library_by_id as jellyfin_sync;

    let state = expect_context::<AppState>();
    for pref in &prefs {
        sqlx::query(r#"UPDATE "JellyfinLibrary" SET "ShouldSyncItems" = ? WHERE "Id" = ?"#)
            .bind(pref.should_sync)
            .bind(pref.id)
            .execute(&state.pool)
            .await?;
    }

    // Mirrors RemoteMediaSourceLibrariesEditor.razor SaveChanges: enqueue sync
    // for each library that has ShouldSyncItems = true.
    let sync_ids: Vec<i64> = prefs
        .iter()
        .filter(|p| p.should_sync)
        .map(|p| p.id)
        .collect();
    if !sync_ids.is_empty() {
        let pool = state.pool.clone();
        tokio::task::spawn(async move {
            for id in sync_ids {
                if let Err(e) = jellyfin_sync(&pool, id).await {
                    tracing::warn!(library_id = id, error = %e, "jellyfin sync failed");
                }
            }
        });
    }
    Ok(())
}

#[server]
pub async fn list_jellyfin_paths(source_id: i64) -> Result<Vec<PathReplacementRow>, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let rows = sqlx::query(
        r#"SELECT "Id",
                  COALESCE("JellyfinPath", '') AS "JellyfinPath",
                  COALESCE("LocalPath", '') AS "LocalPath"
           FROM "JellyfinPathReplacement"
           WHERE "JellyfinMediaSourceId" = ?
           ORDER BY "Id""#,
    )
    .bind(source_id)
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| PathReplacementRow {
            id: r.try_get("Id").unwrap_or(0),
            remote_path: r.try_get("JellyfinPath").unwrap_or_default(),
            local_path: r.try_get("LocalPath").unwrap_or_default(),
        })
        .collect())
}

#[server]
pub async fn save_jellyfin_paths(
    source_id: i64,
    paths: Vec<PathReplacementRow>,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    sqlx::query(r#"DELETE FROM "JellyfinPathReplacement" WHERE "JellyfinMediaSourceId" = ?"#)
        .bind(source_id)
        .execute(&state.pool)
        .await?;

    for p in &paths {
        sqlx::query(
            r#"INSERT INTO "JellyfinPathReplacement"
               ("JellyfinPath", "LocalPath", "JellyfinMediaSourceId")
               VALUES (?, ?, ?)"#,
        )
        .bind(&p.remote_path)
        .bind(&p.local_path)
        .bind(source_id)
        .execute(&state.pool)
        .await?;
    }
    Ok(())
}

// ─── Emby ─────────────────────────────────────────────────────────────────────

#[server]
pub async fn list_emby_sources() -> Result<Vec<RemoteSourceRow>, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let rows = sqlx::query(
        r#"SELECT ems."Id",
                  COALESCE(ems."ServerName", 'Emby') AS "Name",
                  COALESCE(
                      (SELECT "Address" FROM "EmbyConnection"
                       WHERE "EmbyMediaSourceId" = ems."Id"
                       ORDER BY "Id" LIMIT 1),
                      '') AS "Address"
           FROM "EmbyMediaSource" ems
           ORDER BY ems."Id""#,
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| RemoteSourceRow {
            id: r.try_get("Id").unwrap_or(0),
            name: r.try_get("Name").unwrap_or_default(),
            address: r.try_get("Address").unwrap_or_default(),
        })
        .collect())
}

#[server]
pub async fn get_emby_connection() -> Result<ConnectionForm, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();

    let source_row = sqlx::query(
        r#"SELECT ems."Id",
                  COALESCE(
                      (SELECT "Address" FROM "EmbyConnection"
                       WHERE "EmbyMediaSourceId" = ems."Id"
                       ORDER BY "Id" LIMIT 1),
                      '') AS "Address"
           FROM "EmbyMediaSource" ems
           ORDER BY ems."Id"
           LIMIT 1"#,
    )
    .fetch_optional(&state.pool)
    .await?;

    let (source_id, address) = source_row
        .as_ref()
        .map(|r| {
            (
                r.try_get::<i64, _>("Id").unwrap_or(0),
                r.try_get::<String, _>("Address").unwrap_or_default(),
            )
        })
        .unwrap_or_default();

    let api_key: String = sqlx::query_scalar(
        r#"SELECT COALESCE("Value", '') FROM "ConfigElement"
           WHERE "Key" = 'emby.current_server_api_key'
           LIMIT 1"#,
    )
    .fetch_optional(&state.pool)
    .await?
    .unwrap_or_default();

    Ok(ConnectionForm {
        source_id,
        address,
        api_key,
    })
}

#[server]
pub async fn save_emby_connection(form: ConnectionForm) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();

    let source_id = if form.source_id == 0 {
        let ms = sqlx::query(r#"INSERT INTO "MediaSource" DEFAULT VALUES"#)
            .execute(&state.pool)
            .await?;
        let ms_id = ersatzrs_infra::db::last_insert_id(ms);

        sqlx::query(r#"INSERT INTO "EmbyMediaSource" ("Id", "ServerName") VALUES (?, 'Emby')"#)
            .bind(ms_id)
            .execute(&state.pool)
            .await?;

        sqlx::query(
            r#"INSERT INTO "EmbyConnection" ("Address", "EmbyMediaSourceId") VALUES (?, ?)"#,
        )
        .bind(&form.address)
        .bind(ms_id)
        .execute(&state.pool)
        .await?;

        ms_id
    } else {
        sqlx::query(
            r#"UPDATE "EmbyConnection" SET "Address" = ?
               WHERE "EmbyMediaSourceId" = ?
               AND "Id" = (SELECT MIN("Id") FROM "EmbyConnection"
                           WHERE "EmbyMediaSourceId" = ?)"#,
        )
        .bind(&form.address)
        .bind(form.source_id)
        .bind(form.source_id)
        .execute(&state.pool)
        .await?;

        form.source_id
    };

    sqlx::query(
        r#"REPLACE INTO "ConfigElement" ("Key", "Value") VALUES ('emby.current_server_api_key', ?)
           "#,
    )
    .bind(&form.api_key)
    .execute(&state.pool)
    .await?;

    Ok(source_id)
}

#[server]
pub async fn delete_emby_source(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    sqlx::query(r#"DELETE FROM "MediaSource" WHERE "Id" = ?"#)
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(())
}

#[server]
pub async fn list_emby_libraries(source_id: i64) -> Result<Vec<LibraryPrefRow>, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let rows = sqlx::query(
        r#"SELECT l."Id", COALESCE(l."Name", '') AS "Name", l."MediaKind", el."ShouldSyncItems"
           FROM "Library" l
           JOIN "EmbyLibrary" el ON el."Id" = l."Id"
           WHERE l."MediaSourceId" = ?
           ORDER BY l."MediaKind", l."Name""#,
    )
    .bind(source_id)
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| LibraryPrefRow {
            id: r.try_get("Id").unwrap_or(0),
            name: r.try_get("Name").unwrap_or_default(),
            media_kind: r.try_get("MediaKind").unwrap_or(1),
            should_sync: r.try_get::<bool, _>("ShouldSyncItems").unwrap_or(false),
        })
        .collect())
}

#[server]
pub async fn save_emby_library_prefs(prefs: Vec<LibraryPrefRow>) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_infra::emby::sync_library_by_id as emby_sync;

    let state = expect_context::<AppState>();
    for pref in &prefs {
        sqlx::query(r#"UPDATE "EmbyLibrary" SET "ShouldSyncItems" = ? WHERE "Id" = ?"#)
            .bind(pref.should_sync)
            .bind(pref.id)
            .execute(&state.pool)
            .await?;
    }

    // Mirrors RemoteMediaSourceLibrariesEditor.razor SaveChanges: enqueue sync
    // for each library that has ShouldSyncItems = true.
    let sync_ids: Vec<i64> = prefs
        .iter()
        .filter(|p| p.should_sync)
        .map(|p| p.id)
        .collect();
    if !sync_ids.is_empty() {
        let pool = state.pool.clone();
        tokio::task::spawn(async move {
            for id in sync_ids {
                if let Err(e) = emby_sync(&pool, id).await {
                    tracing::warn!(library_id = id, error = %e, "emby sync failed");
                }
            }
        });
    }
    Ok(())
}

#[server]
pub async fn list_emby_paths(source_id: i64) -> Result<Vec<PathReplacementRow>, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let rows = sqlx::query(
        r#"SELECT "Id",
                  COALESCE("EmbyPath", '') AS "EmbyPath",
                  COALESCE("LocalPath", '') AS "LocalPath"
           FROM "EmbyPathReplacement"
           WHERE "EmbyMediaSourceId" = ?
           ORDER BY "Id""#,
    )
    .bind(source_id)
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| PathReplacementRow {
            id: r.try_get("Id").unwrap_or(0),
            remote_path: r.try_get("EmbyPath").unwrap_or_default(),
            local_path: r.try_get("LocalPath").unwrap_or_default(),
        })
        .collect())
}

#[server]
pub async fn save_emby_paths(
    source_id: i64,
    paths: Vec<PathReplacementRow>,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    sqlx::query(r#"DELETE FROM "EmbyPathReplacement" WHERE "EmbyMediaSourceId" = ?"#)
        .bind(source_id)
        .execute(&state.pool)
        .await?;

    for p in &paths {
        sqlx::query(
            r#"INSERT INTO "EmbyPathReplacement"
               ("EmbyPath", "LocalPath", "EmbyMediaSourceId")
               VALUES (?, ?, ?)"#,
        )
        .bind(&p.remote_path)
        .bind(&p.local_path)
        .bind(source_id)
        .execute(&state.pool)
        .await?;
    }
    Ok(())
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
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let rows = sqlx::query(
        r#"SELECT pms."Id",
                  COALESCE(pms."ServerName", 'Plex') AS "Name",
                  COALESCE(
                      (SELECT "Uri" FROM "PlexConnection"
                       WHERE "PlexMediaSourceId" = pms."Id" AND "IsActive" = 1
                       ORDER BY "Id" LIMIT 1),
                      '') AS "Address"
           FROM "PlexMediaSource" pms
           ORDER BY pms."Id""#,
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| PlexSourceRow {
            id: r.try_get("Id").unwrap_or(0),
            name: r.try_get("Name").unwrap_or_default(),
            address: r.try_get("Address").unwrap_or_default(),
        })
        .collect())
}

#[server]
pub async fn has_plex_token() -> Result<bool, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let val: Option<String> = sqlx::query_scalar(
        r#"SELECT "Value" FROM "ConfigElement"
           WHERE "Key" = 'plex.current_server_auth_token'
           LIMIT 1"#,
    )
    .fetch_optional(&state.pool)
    .await?;

    Ok(val.is_some_and(|v| !v.is_empty()))
}

#[server]
pub async fn sign_out_plex() -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    // Remove all Plex auth tokens and wipe media source rows.
    sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key" LIKE 'plex.%auth_token%'"#)
        .execute(&state.pool)
        .await?;
    sqlx::query(r#"DELETE FROM "MediaSource" WHERE "Id" IN (SELECT "Id" FROM "PlexMediaSource")"#)
        .execute(&state.pool)
        .await?;
    Ok(())
}

/// Begins a Plex PIN auth flow. Returns `(pin_id, auth_url)`.
///
/// Calls `IPlexTvApiClient.StartPinFlow()` equivalent in infra, stores the
/// pending pin code in `ConfigElement` for later polling.
#[server]
pub async fn start_plex_pin_flow() -> Result<(String, String), ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_infra::plex::PlexService;

    let state = expect_context::<AppState>();
    let client_id = PlexService::get_or_create_client_identifier(&state.pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    let service = PlexService::new();
    let pin = service
        .start_pin_flow(&client_id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    sqlx::query(
        r#"REPLACE INTO "ConfigElement" ("Key", "Value")
           VALUES ('plex.pending_pin_code', ?)
           "#,
    )
    .bind(&pin.code)
    .execute(&state.pool)
    .await?;

    Ok((pin.id, pin.auth_url))
}

/// Polls Plex for PIN completion. Returns `Some(token)` when the user has
/// authorized, `None` if still pending.
///
/// On success, calls `PlexService::get_servers` (mirrors
/// `SynchronizePlexMediaSources`) to discover and upsert PlexMediaSource rows.
#[server]
pub async fn check_plex_pin(pin_id: String) -> Result<Option<String>, ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_infra::plex::PlexService;

    let state = expect_context::<AppState>();

    let client_id = PlexService::get_or_create_client_identifier(&state.pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    let pin_code: String = sqlx::query_scalar(
        r#"SELECT COALESCE("Value", '') FROM "ConfigElement"
           WHERE "Key" = 'plex.pending_pin_code' LIMIT 1"#,
    )
    .fetch_optional(&state.pool)
    .await?
    .unwrap_or_default();

    let service = PlexService::new();
    let token = service
        .complete_pin_flow(&pin_id, &pin_code, &client_id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    if let Some(ref user_token) = token {
        sqlx::query(
            r#"REPLACE INTO "ConfigElement" ("Key", "Value")
               VALUES ('plex.user_auth_token', ?)
               "#,
        )
        .bind(user_token)
        .execute(&state.pool)
        .await?;

        let servers = service
            .get_servers(user_token, &client_id)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;

        PlexService::upsert_sources(&state.pool, &servers)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;
    }

    Ok(token)
}

#[server]
pub async fn list_plex_libraries(source_id: i64) -> Result<Vec<LibraryPrefRow>, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let rows = sqlx::query(
        r#"SELECT l."Id", COALESCE(l."Name", '') AS "Name", l."MediaKind", pl."ShouldSyncItems"
           FROM "Library" l
           JOIN "PlexLibrary" pl ON pl."Id" = l."Id"
           WHERE l."MediaSourceId" = ?
           ORDER BY l."MediaKind", l."Name""#,
    )
    .bind(source_id)
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| LibraryPrefRow {
            id: r.try_get("Id").unwrap_or(0),
            name: r.try_get("Name").unwrap_or_default(),
            media_kind: r.try_get("MediaKind").unwrap_or(1),
            should_sync: r.try_get::<bool, _>("ShouldSyncItems").unwrap_or(false),
        })
        .collect())
}

#[server]
pub async fn save_plex_library_prefs(prefs: Vec<LibraryPrefRow>) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_infra::plex::sync_library_by_id as plex_sync;

    let state = expect_context::<AppState>();
    for pref in &prefs {
        sqlx::query(r#"UPDATE "PlexLibrary" SET "ShouldSyncItems" = ? WHERE "Id" = ?"#)
            .bind(pref.should_sync)
            .bind(pref.id)
            .execute(&state.pool)
            .await?;
    }

    // Mirrors RemoteMediaSourceLibrariesEditor.razor SaveChanges: enqueue sync
    // for each library that has ShouldSyncItems = true.
    let sync_ids: Vec<i64> = prefs
        .iter()
        .filter(|p| p.should_sync)
        .map(|p| p.id)
        .collect();
    if !sync_ids.is_empty() {
        let pool = state.pool.clone();
        tokio::task::spawn(async move {
            for id in sync_ids {
                if let Err(e) = plex_sync(&pool, id).await {
                    tracing::warn!(library_id = id, error = %e, "plex sync failed");
                }
            }
        });
    }
    Ok(())
}

#[server]
pub async fn list_plex_paths(source_id: i64) -> Result<Vec<PathReplacementRow>, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let rows = sqlx::query(
        r#"SELECT "Id",
                  COALESCE("PlexPath", '') AS "PlexPath",
                  COALESCE("LocalPath", '') AS "LocalPath"
           FROM "PlexPathReplacement"
           WHERE "PlexMediaSourceId" = ?
           ORDER BY "Id""#,
    )
    .bind(source_id)
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| PathReplacementRow {
            id: r.try_get("Id").unwrap_or(0),
            remote_path: r.try_get("PlexPath").unwrap_or_default(),
            local_path: r.try_get("LocalPath").unwrap_or_default(),
        })
        .collect())
}

#[server]
pub async fn save_plex_paths(
    source_id: i64,
    paths: Vec<PathReplacementRow>,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    sqlx::query(r#"DELETE FROM "PlexPathReplacement" WHERE "PlexMediaSourceId" = ?"#)
        .bind(source_id)
        .execute(&state.pool)
        .await?;

    for p in &paths {
        sqlx::query(
            r#"INSERT INTO "PlexPathReplacement"
               ("PlexPath", "LocalPath", "PlexMediaSourceId")
               VALUES (?, ?, ?)"#,
        )
        .bind(&p.remote_path)
        .bind(&p.local_path)
        .bind(source_id)
        .execute(&state.pool)
        .await?;
    }
    Ok(())
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

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

#[cfg(all(test, feature = "ssr", not(feature = "mysql")))]
mod tests {
    use super::*;
    use sqlx::Row;

    #[tokio::test]
    async fn local_library_save_normalizes_and_persists_existing_paths() {
        let database_dir = tempfile::tempdir().expect("temp database dir");
        let database_url = format!(
            "sqlite://{}",
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
            media_dir
                .path()
                .canonicalize()
                .expect("canonical path")
                .display()
                .to_string()
        );
    }

    #[test]
    fn local_library_path_normalization_accepts_file_uri_input() {
        assert_eq!(
            normalize_local_library_path("file:///mnt/d/downloads/movies/"),
            Some("/mnt/d/downloads/movies".to_string())
        );
    }
}
