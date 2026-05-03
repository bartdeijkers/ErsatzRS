use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LibraryRow {
    pub id: i64,
    pub name: String,
    /// "Local", "Plex", "Jellyfin", or "Emby"
    pub kind: String,
    /// Raw DB integer matching `ErsatzTV`'s `LibraryMediaKind`:
    /// 1=Movies 2=Shows 3=MusicVideos 4=OtherVideos 5=Songs 6=Images 7=RemoteStreams
    pub media_kind: i64,
    /// Populated for remote providers; None for Local.
    pub source_name: Option<String>,
    pub media_source_id: i64,
}

impl LibraryRow {
    pub fn media_kind_label(&self) -> &'static str {
        match self.media_kind {
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
}

/// Returns every configured library with its provider kind and media kind.
#[server]
pub async fn list_libraries() -> Result<Vec<LibraryRow>, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();

    let rows = sqlx::query(
        r#"SELECT
               l."Id",
               COALESCE(l."Name", '') AS "Name",
               l."MediaKind",
               l."MediaSourceId",
               CASE
                   WHEN ll."Id" IS NOT NULL THEN 'Local'
                   WHEN pl."Id" IS NOT NULL THEN 'Plex'
                   WHEN jl."Id" IS NOT NULL THEN 'Jellyfin'
                   WHEN el."Id" IS NOT NULL THEN 'Emby'
                   ELSE 'Unknown'
               END AS "Kind",
               CASE
                   WHEN pl."Id" IS NOT NULL THEN pms."ServerName"
                   WHEN jl."Id" IS NOT NULL THEN jms."ServerName"
                   WHEN el."Id" IS NOT NULL THEN ems."ServerName"
                   ELSE NULL
               END AS "SourceName"
           FROM "Library" l
           LEFT JOIN "LocalLibrary"    ll  ON ll."Id"  = l."Id"
           LEFT JOIN "PlexLibrary"     pl  ON pl."Id"  = l."Id"
           LEFT JOIN "JellyfinLibrary" jl  ON jl."Id"  = l."Id"
           LEFT JOIN "EmbyLibrary"     el  ON el."Id"  = l."Id"
           LEFT JOIN "PlexMediaSource"     pms ON pms."Id" = l."MediaSourceId"
           LEFT JOIN "JellyfinMediaSource" jms ON jms."Id" = l."MediaSourceId"
           LEFT JOIN "EmbyMediaSource"     ems ON ems."Id" = l."MediaSourceId"
           ORDER BY "Kind", "SourceName", "Name""#,
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| LibraryRow {
            id: r.try_get("Id").unwrap_or(0),
            name: r.try_get("Name").unwrap_or_default(),
            media_kind: r.try_get("MediaKind").unwrap_or(0),
            media_source_id: r.try_get("MediaSourceId").unwrap_or(0),
            kind: r.try_get("Kind").unwrap_or_default(),
            source_name: r.try_get("SourceName").ok().flatten(),
        })
        .collect())
}

/// Fires a background scan for a single library. Returns immediately.
/// The actual sync work runs in a spawned tokio task. The library is marked
/// as locked in `AppState::entity_locker` for the duration of the scan so the
/// UI can render an indeterminate spinner while it runs.
#[server]
#[allow(clippy::unused_async)]
pub async fn scan_library(
    library_id: i64,
    kind: String,
    deep_scan: bool,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let pool = state.pool.clone();
    let locker = state.entity_locker.clone();

    if !locker.lock_library(library_id) {
        // Already scanning — mirror ETV's no-op when the library is locked.
        return Ok(());
    }

    tokio::task::spawn(async move {
        macro_rules! log_err {
            ($res:expr) => {
                if let Err(e) = $res {
                    tracing::warn!(library_id, kind, error = %e, "library scan failed");
                }
            };
        }
        match kind.as_str() {
            "Plex" => log_err!(
                ersatzrs_infra::plex::sync_library_by_id_with_options(&pool, library_id, deep_scan)
                    .await
            ),
            "Jellyfin" => {
                log_err!(
                    ersatzrs_infra::jellyfin::sync_library_by_id_with_options(
                        &pool, library_id, deep_scan
                    )
                    .await
                );
            }
            "Emby" => {
                log_err!(
                    ersatzrs_infra::emby::sync_library_by_id_with_options(
                        &pool, library_id, deep_scan
                    )
                    .await
                );
            }
            "Local" => {
                use ersatzrs_scanner::{scan_local_library, LocalScanConfig};
                let config = LocalScanConfig {
                    library_id,
                    ffprobe_path: ersatzrs_ffmpeg::ffprobe_path_from_env(),
                };
                log_err!(scan_local_library(&pool, &config).await);
            }
            other => {
                tracing::warn!(kind = other, "unknown library kind in scan_library");
            }
        }
        locker.unlock_library(library_id);
    });

    Ok(())
}

/// Returns the ids of libraries currently being scanned. Polled by the
/// libraries page to drive the per-row "scanning" spinner.
#[server]
#[allow(clippy::unused_async)]
pub async fn list_locked_libraries() -> Result<Vec<i64>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state.entity_locker.locked_libraries())
}
