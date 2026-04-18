use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LibraryRow {
    pub id: i64,
    pub name: String,
    /// "Local", "Plex", "Jellyfin", or "Emby"
    pub kind: String,
    /// Raw DB integer: 0=Movies 1=Shows 2=MusicVideos 3=OtherVideos 4=Songs 5=Images 6=RemoteStreams
    pub media_kind: i64,
    /// Populated for remote providers; None for Local.
    pub source_name: Option<String>,
    pub media_source_id: i64,
}

impl LibraryRow {
    pub fn media_kind_label(&self) -> &'static str {
        match self.media_kind {
            0 => "Movies",
            1 => "Shows",
            2 => "Music Videos",
            3 => "Other Videos",
            4 => "Songs",
            5 => "Images",
            6 => "Remote Streams",
            _ => "Unknown",
        }
    }
}

/// Returns every configured library with its provider kind and media kind.
#[server]
pub async fn list_libraries() -> Result<Vec<LibraryRow>, ServerFnError> {
    use crate::state::AppState;
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
/// The actual sync work runs in a spawned tokio task.
#[server]
#[allow(clippy::unused_async)]
pub async fn scan_library(library_id: i64, kind: String) -> Result<(), ServerFnError> {
    use crate::state::AppState;

    let state = expect_context::<AppState>();
    let pool = state.pool.clone();

    tokio::task::spawn(async move {
        macro_rules! log_err {
            ($res:expr) => {
                if let Err(e) = $res {
                    tracing::warn!(library_id, kind, error = %e, "library scan failed");
                }
            };
        }
        match kind.as_str() {
            "Plex" => log_err!(ersatzrs_infra::plex::sync_library_by_id(&pool, library_id).await),
            "Jellyfin" => {
                log_err!(ersatzrs_infra::jellyfin::sync_library_by_id(&pool, library_id).await);
            }
            "Emby" => {
                log_err!(ersatzrs_infra::emby::sync_library_by_id(&pool, library_id).await);
            }
            "Local" => {
                use ersatzrs_scanner::{scan_local_library, LocalScanConfig};
                #[allow(clippy::disallowed_methods)]
                let ffprobe_path =
                    std::env::var("FFPROBE_PATH").unwrap_or_else(|_| "ffprobe".to_string());
                let config = LocalScanConfig {
                    library_id,
                    ffprobe_path,
                };
                log_err!(scan_local_library(&pool, &config).await);
            }
            other => {
                tracing::warn!(kind = other, "unknown library kind in scan_library");
            }
        }
    });

    Ok(())
}
