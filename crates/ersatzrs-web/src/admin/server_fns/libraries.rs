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
    let progress_map = state.scan_progress.clone();

    tracing::info!(library_id, kind = %kind, deep_scan, "scan_library invoked");

    if !locker.lock_library(library_id) {
        tracing::info!(library_id, "scan_library: already locked, no-op");
        return Ok(());
    }

    tokio::task::spawn(async move {
        let started = std::time::Instant::now();
        tracing::info!(library_id, kind = %kind, "library scan started");
        let sink = progress_map.sink_for(library_id);
        let sink_ref: &dyn ersatzrs_domain::ScanProgressSink = &sink;
        macro_rules! log_err {
            ($res:expr) => {
                if let Err(e) = $res {
                    tracing::warn!(library_id, kind, error = %e, "library scan failed");
                }
            };
        }
        match kind.as_str() {
            "Plex" => log_err!(
                ersatzrs_infra::plex::sync_library_by_id_with_progress(
                    &pool,
                    library_id,
                    deep_scan,
                    Some(sink_ref),
                )
                .await
            ),
            "Jellyfin" => {
                log_err!(
                    ersatzrs_infra::jellyfin::sync_library_by_id_with_progress(
                        &pool,
                        library_id,
                        deep_scan,
                        Some(sink_ref),
                    )
                    .await
                );
            }
            "Emby" => {
                log_err!(
                    ersatzrs_infra::emby::sync_library_by_id_with_progress(
                        &pool,
                        library_id,
                        deep_scan,
                        Some(sink_ref),
                    )
                    .await
                );
            }
            "Local" => {
                use ersatzrs_scanner::{scan_local_library_with_progress, LocalScanConfig};
                let config = LocalScanConfig {
                    library_id,
                    ffprobe_path: ersatzrs_ffmpeg::ffprobe_path_from_env(),
                };
                log_err!(scan_local_library_with_progress(&pool, &config, Some(sink_ref)).await);
            }
            other => {
                tracing::warn!(kind = other, "unknown library kind in scan_library");
            }
        }
        locker.unlock_library(library_id);
        progress_map.clear(library_id);
        tracing::info!(
            library_id,
            elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            "library scan finished"
        );
    });

    Ok(())
}

/// Fires a background scan for a single show.
///
/// Plex now uses the targeted ErsatzTV-equivalent path. Jellyfin, Emby, and
/// Local intentionally fall back to full library scans until their provider
/// slices port `ScanSingleShow`. Mirrors
/// `$ERSATZTV_SRC/ErsatzTV.Application/Libraries/Commands/QueueShowScanByLibraryIdHandler.cs`.
#[server]
#[allow(clippy::unused_async)]
pub async fn scan_show(
    library_id: i64,
    show_id: i64,
    kind: String,
    deep_scan: bool,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    if kind != "Plex" {
        return scan_library(library_id, kind, deep_scan).await;
    }

    let state = expect_context::<AppState>();
    let pool = state.pool.clone();
    let locker = state.entity_locker.clone();
    let progress_map = state.scan_progress.clone();

    tracing::info!(
        library_id,
        show_id,
        kind = %kind,
        deep_scan,
        "scan_show invoked"
    );

    if !locker.lock_library(library_id) {
        tracing::info!(library_id, show_id, "scan_show: already locked, no-op");
        return Ok(());
    }

    tokio::task::spawn(async move {
        let started = std::time::Instant::now();
        tracing::info!(library_id, show_id, kind = %kind, "show scan started");
        let sink = progress_map.sink_for(library_id);
        let sink_ref: &dyn ersatzrs_domain::ScanProgressSink = &sink;
        let result = ersatzrs_infra::plex::sync_plex_show_by_id(
            &pool,
            library_id,
            show_id,
            deep_scan,
            Some(sink_ref),
        )
        .await;

        match result {
            Ok(_) => {
                if deep_scan {
                    // ErsatzTV queues `ExtractEmbeddedShowSubtitles` after a
                    // successful deep single-show scan; embedded subtitle
                    // extraction is not ported yet, so keep the deferral
                    // visible at the call site.
                    tracing::debug!(
                        library_id,
                        show_id,
                        "embedded subtitle extraction after deep show scan is deferred"
                    );
                }
            }
            Err(e) => {
                tracing::warn!(library_id, show_id, kind, error = %e, "show scan failed");
            }
        }

        locker.unlock_library(library_id);
        progress_map.clear(library_id);
        tracing::info!(
            library_id,
            show_id,
            elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            "show scan finished"
        );
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

/// Returns the fractional scan progress per library in `[0.0, 1.0]`. Polled
/// by the libraries page alongside `list_locked_libraries` to drive the
/// percentage ring. Libraries that are locked but have not yet emitted any
/// progress are simply absent from the map; the UI falls back to an
/// indeterminate spinner in that case.
#[server]
#[allow(clippy::unused_async)]
pub async fn list_scan_progress() -> Result<Vec<(i64, f32)>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state.scan_progress.snapshot().into_iter().collect())
}

/// One row of the External Collections table. Mirrors ETV's
/// `GetExternalCollectionsHandler` — one row per remote `MediaSource` whose
/// libraries are flagged with `ShouldSyncItems = 1`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ExternalCollectionRow {
    pub media_source_id: i64,
    /// "Plex", "Jellyfin", or "Emby".
    pub kind: String,
    pub server_name: Option<String>,
}

impl ExternalCollectionRow {
    /// Key used by the `EntityLocker` to gate a collections sync for one
    /// media source. Kept in sync with `scan_external_collections`.
    #[must_use]
    pub fn lock_key(&self) -> String {
        external_collections_lock_key(&self.kind, self.media_source_id)
    }
}

fn external_collections_lock_key(kind: &str, media_source_id: i64) -> String {
    format!("collections:{kind}:{media_source_id}")
}

/// Returns one External Collections row per remote MediaSource whose
/// libraries are flagged for collection sync. Mirrors ETV's
/// `GetExternalCollectionsHandler`.
#[server]
pub async fn list_external_collections() -> Result<Vec<ExternalCollectionRow>, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();

    let rows = sqlx::query(
        r#"SELECT DISTINCT
               l."MediaSourceId" AS "MediaSourceId",
               CASE
                   WHEN pl."Id" IS NOT NULL THEN 'Plex'
                   WHEN jl."Id" IS NOT NULL THEN 'Jellyfin'
                   WHEN el."Id" IS NOT NULL THEN 'Emby'
                   ELSE NULL
               END AS "Kind",
               COALESCE(pms."ServerName", jms."ServerName", ems."ServerName") AS "ServerName"
           FROM "Library" l
           LEFT JOIN "PlexLibrary"     pl  ON pl."Id"  = l."Id"
           LEFT JOIN "JellyfinLibrary" jl  ON jl."Id"  = l."Id"
           LEFT JOIN "EmbyLibrary"     el  ON el."Id"  = l."Id"
           LEFT JOIN "PlexMediaSource"     pms ON pms."Id" = l."MediaSourceId"
           LEFT JOIN "JellyfinMediaSource" jms ON jms."Id" = l."MediaSourceId"
           LEFT JOIN "EmbyMediaSource"     ems ON ems."Id" = l."MediaSourceId"
           WHERE (pl."ShouldSyncItems" = 1
               OR jl."ShouldSyncItems" = 1
               OR el."ShouldSyncItems" = 1)
           ORDER BY "Kind", "ServerName", "MediaSourceId""#,
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .iter()
        .filter_map(|r| {
            let kind: Option<String> = r.try_get("Kind").ok().flatten();
            kind.map(|kind| ExternalCollectionRow {
                media_source_id: r.try_get("MediaSourceId").unwrap_or(0),
                kind,
                server_name: r.try_get("ServerName").ok().flatten(),
            })
        })
        .collect())
}

/// Fires a background sync of external collections for a single remote
/// MediaSource. Returns immediately; the sync runs in a spawned task and is
/// gated by the `EntityLocker` under `collections:{kind}:{media_source_id}`.
#[server]
#[allow(clippy::unused_async)]
pub async fn scan_external_collections(
    media_source_id: i64,
    kind: String,
    deep_scan: bool,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let pool = state.pool.clone();
    let locker = state.entity_locker.clone();
    let key = external_collections_lock_key(&kind, media_source_id);

    tracing::info!(media_source_id, kind = %kind, deep_scan, "scan_external_collections invoked");

    if !locker.lock_key(key.clone()) {
        tracing::info!(media_source_id, kind = %kind, "scan_external_collections: already locked, no-op");
        return Ok(());
    }

    tokio::task::spawn(async move {
        let started = std::time::Instant::now();
        tracing::info!(media_source_id, kind = %kind, "external collections sync started");
        let res = match kind.as_str() {
            "Plex" => {
                ersatzrs_infra::plex::sync_collections_for_media_source(
                    &pool,
                    media_source_id,
                    deep_scan,
                )
                .await
            }
            "Jellyfin" => {
                ersatzrs_infra::jellyfin::sync_collections_for_media_source(
                    &pool,
                    media_source_id,
                    deep_scan,
                )
                .await
            }
            "Emby" => {
                ersatzrs_infra::emby::sync_collections_for_media_source(
                    &pool,
                    media_source_id,
                    deep_scan,
                )
                .await
            }
            other => {
                tracing::warn!(kind = other, "unknown kind in scan_external_collections");
                Ok(0)
            }
        };
        if let Err(e) = res {
            tracing::warn!(media_source_id, kind = %kind, error = %e, "external collections sync failed");
        }
        locker.unlock_key(&key);
        tracing::info!(
            media_source_id,
            kind = %kind,
            elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            "external collections sync finished"
        );
    });

    Ok(())
}

/// Returns the `(kind, media_source_id)` pairs whose collections sync is
/// currently locked. Polled by the libraries page alongside
/// `list_locked_libraries`.
#[server]
#[allow(clippy::unused_async)]
pub async fn list_locked_collections() -> Result<Vec<(String, i64)>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
        .entity_locker
        .locked_keys()
        .into_iter()
        .filter_map(|key| {
            let rest = key.strip_prefix("collections:")?;
            let (kind, id) = rest.rsplit_once(':')?;
            id.parse::<i64>().ok().map(|id| (kind.to_owned(), id))
        })
        .collect())
}
