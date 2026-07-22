use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(not(target_arch = "wasm32"))]
use crate::host::state::{
    EntityLocker, LibraryLockGuard, LibraryScanJob, LibraryScanQueue, ScanProgressMap,
};

#[cfg(not(target_arch = "wasm32"))]
struct ScanCleanup {
    _lock_guard: LibraryLockGuard,
    progress_map: ScanProgressMap,
    library_id: i64,
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for ScanCleanup {
    fn drop(&mut self) {
        self.progress_map.clear(self.library_id);
    }
}

/// Serializable transport row shared by the SSR server function and WASM
/// island. Native SQL decoding stays in `ersatzrs-infra::admin_libraries`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LibraryRow {
    pub id: i64,
    pub name: String,
    /// "Local", "Plex", "Jellyfin", or "Emby".
    pub kind: String,
    /// Raw database integer matching ErsatzTV's `LibraryMediaKind`.
    pub media_kind: i64,
    pub source_name: Option<String>,
    pub media_source_id: i64,
}

#[cfg(not(target_arch = "wasm32"))]
impl From<ersatzrs_infra::admin_libraries::AdminLibraryRow> for LibraryRow {
    fn from(row: ersatzrs_infra::admin_libraries::AdminLibraryRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            kind: row.kind,
            media_kind: row.media_kind,
            source_name: row.source_name,
            media_source_id: row.media_source_id,
        }
    }
}

/// Serializable external-collection transport row for the hydrated island.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExternalCollectionRow {
    pub media_source_id: i64,
    /// "Plex", "Jellyfin", or "Emby".
    pub kind: String,
    pub server_name: Option<String>,
}

#[cfg(not(target_arch = "wasm32"))]
impl From<ersatzrs_infra::admin_libraries::AdminExternalCollectionRow> for ExternalCollectionRow {
    fn from(row: ersatzrs_infra::admin_libraries::AdminExternalCollectionRow) -> Self {
        Self {
            media_source_id: row.media_source_id,
            kind: row.kind,
            server_name: row.server_name,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LibrariesPageData {
    pub libraries: Vec<LibraryRow>,
    pub external_collections: Vec<ExternalCollectionRow>,
}

/// Returns every configured library with its provider kind and media kind.
#[server]
pub async fn list_libraries() -> Result<Vec<LibraryRow>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(ersatzrs_infra::admin_libraries::load_libraries(&state.pool)
        .await?
        .into_iter()
        .map(Into::into)
        .collect())
}

#[server]
pub async fn load_libraries_page() -> Result<LibrariesPageData, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(load_libraries_page_with_pool(&state.pool).await?)
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn load_libraries_page_with_pool(
    pool: &ersatzrs_infra::db::DbPool,
) -> ersatzrs_core::repositories::Result<LibrariesPageData> {
    let (libraries, external_collections) = tokio::try_join!(
        ersatzrs_infra::admin_libraries::load_libraries(pool),
        ersatzrs_infra::admin_libraries::load_external_collections(pool),
    )?;
    Ok(LibrariesPageData {
        libraries: libraries.into_iter().map(Into::into).collect(),
        external_collections: external_collections.into_iter().map(Into::into).collect(),
    })
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
) -> Result<bool, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let pool = state.pool.clone();
    let locker = state.entity_locker.clone();
    let progress_map = state.scan_progress.clone();
    let queue = state.library_scan_queue.clone();

    tracing::info!(library_id, kind = %kind, deep_scan, "scan_library invoked");

    let should_spawn = queue.enqueue(LibraryScanJob {
        library_id,
        kind,
        deep_scan,
    });
    if should_spawn {
        tokio::task::spawn(run_library_scan_queue(pool, locker, progress_map, queue));
    }

    Ok(true)
}

#[cfg(not(target_arch = "wasm32"))]
async fn run_library_scan_queue(
    pool: ersatzrs_infra::db::DbPool,
    locker: EntityLocker,
    progress_map: ScanProgressMap,
    queue: LibraryScanQueue,
) {
    while let Some(job) = queue.pop_next_or_stop() {
        run_library_scan_job(&pool, &locker, &progress_map, job).await;
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn run_library_scan_job(
    pool: &ersatzrs_infra::db::DbPool,
    locker: &EntityLocker,
    progress_map: &ScanProgressMap,
    job: LibraryScanJob,
) {
    let LibraryScanJob {
        library_id,
        kind,
        deep_scan,
    } = job;
    let Some(lock_guard) = locker.try_lock_library(library_id) else {
        tracing::info!(
            library_id,
            "library scan queue: already locked, skipping job"
        );
        return;
    };
    let cleanup = ScanCleanup {
        _lock_guard: lock_guard,
        progress_map: progress_map.clone(),
        library_id,
    };
    let started = std::time::Instant::now();
    tracing::info!(library_id, kind = %kind, "library scan started");
    let sink = progress_map.sink_for(library_id);
    let sink_ref: &dyn ersatzrs_domain::ScanProgressSink = &sink;
    macro_rules! log_err {
        ($res:expr) => {
            if let Err(e) = $res {
                tracing::warn!(library_id, kind = %kind, error = %e, "library scan failed");
            }
        };
    }
    match kind.as_str() {
        "Plex" => log_err!(
            ersatzrs_infra::plex::sync_library_by_id_with_progress(
                pool,
                library_id,
                deep_scan,
                Some(sink_ref),
            )
            .await
        ),
        "Jellyfin" => {
            log_err!(
                ersatzrs_infra::jellyfin::sync_library_by_id_with_progress(
                    pool,
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
                    pool,
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
                ffprobe_path: ersatzrs_ffmpeg::ffprobe_path(),
            };
            log_err!(scan_local_library_with_progress(pool, &config, Some(sink_ref)).await);
        }
        other => {
            tracing::warn!(kind = other, "unknown library kind in scan_library");
        }
    }
    drop(cleanup);
    tracing::info!(
        library_id,
        elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        "library scan finished"
    );
}

/// Fires a background scan for a single show.
///
/// Plex, Jellyfin, and Emby use the targeted ErsatzTV-equivalent path. Local
/// intentionally falls back to a full library scan (matches ErsatzTV's
/// behaviour). Mirrors
/// `$ERSATZTV_SRC/ErsatzTV.Application/Libraries/Commands/QueueShowScanByLibraryIdHandler.cs`.
#[server]
#[allow(clippy::unused_async)]
pub async fn scan_show(
    library_id: i64,
    show_id: i64,
    kind: String,
    deep_scan: bool,
) -> Result<bool, ServerFnError> {
    use crate::host::state::AppState;

    if kind != "Plex" && kind != "Jellyfin" && kind != "Emby" {
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

    let Some(lock_guard) = locker.try_lock_library(library_id) else {
        tracing::info!(library_id, show_id, "scan_show: already locked, no-op");
        return Ok(false);
    };

    tokio::task::spawn(async move {
        let cleanup = ScanCleanup {
            _lock_guard: lock_guard,
            progress_map: progress_map.clone(),
            library_id,
        };
        let started = std::time::Instant::now();
        tracing::info!(library_id, show_id, kind = %kind, "show scan started");
        let sink = progress_map.sink_for(library_id);
        let sink_ref: &dyn ersatzrs_domain::ScanProgressSink = &sink;
        let result = match kind.as_str() {
            "Plex" => {
                ersatzrs_infra::plex::sync_plex_show_by_id(
                    &pool,
                    library_id,
                    show_id,
                    deep_scan,
                    Some(sink_ref),
                )
                .await
            }
            "Jellyfin" => {
                ersatzrs_infra::jellyfin::sync_jellyfin_show_by_id(
                    &pool,
                    library_id,
                    show_id,
                    deep_scan,
                    Some(sink_ref),
                )
                .await
            }
            "Emby" => {
                ersatzrs_infra::emby::sync_emby_show_by_id(
                    &pool,
                    library_id,
                    show_id,
                    deep_scan,
                    Some(sink_ref),
                )
                .await
            }
            // Other kinds were filtered out above; defensively no-op.
            _ => Ok(0),
        };

        match result {
            Ok(_) => {
                if deep_scan {
                    // Mirror `QueueShowScanByLibraryIdHandler` (success && deepScan):
                    // extract embedded text subtitles + font attachments. The
                    // function self-gates on the `ffmpeg.use_embedded_subtitles` /
                    // `ffmpeg.extract_embedded_subtitles` config keys, so it is a
                    // no-op under the default (extract = false) configuration.
                    if let Err(e) =
                        ersatzrs_infra::subtitles::extract_embedded_show_subtitles(&pool, show_id)
                            .await
                    {
                        tracing::warn!(
                            library_id,
                            show_id,
                            error = %e,
                            "embedded subtitle extraction failed"
                        );
                    }
                }
            }
            Err(e) => {
                tracing::warn!(library_id, show_id, kind, error = %e, "show scan failed");
            }
        }

        drop(cleanup);
        tracing::info!(
            library_id,
            show_id,
            elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            "show scan finished"
        );
    });

    Ok(true)
}

/// Returns the ids of libraries currently being scanned. Polled by the
/// libraries page to drive the per-row "scanning" spinner.
#[server]
#[allow(clippy::unused_async)]
pub async fn list_locked_libraries() -> Result<Vec<i64>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let mut library_ids = state.entity_locker.locked_libraries();
    library_ids.extend(state.library_scan_queue.queued_libraries());
    library_ids.sort_unstable();
    library_ids.dedup();
    Ok(library_ids)
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
fn external_collections_lock_key(kind: &str, media_source_id: i64) -> String {
    format!("collections:{kind}:{media_source_id}")
}

/// Returns one External Collections row per remote MediaSource whose
/// libraries are flagged for collection sync. Mirrors ETV's
/// `GetExternalCollectionsHandler`.
#[server]
pub async fn list_external_collections() -> Result<Vec<ExternalCollectionRow>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(
        ersatzrs_infra::admin_libraries::load_external_collections(&state.pool)
            .await?
            .into_iter()
            .map(Into::into)
            .collect(),
    )
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
) -> Result<bool, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let pool = state.pool.clone();
    let locker = state.entity_locker.clone();
    let key = external_collections_lock_key(&kind, media_source_id);

    tracing::info!(media_source_id, kind = %kind, deep_scan, "scan_external_collections invoked");

    if !locker.lock_key(key.clone()) {
        tracing::info!(media_source_id, kind = %kind, "scan_external_collections: already locked, no-op");
        return Ok(false);
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

    Ok(true)
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

#[cfg(test)]
mod server_fn_path_stability {
    //! Regression guard for the `SERVER_FN_OVERRIDE_KEY` pin in
    //! `.cargo/config.toml`. `server_fn` derives each `/api/<fn><hash>` route from
    //! `xxh64(CARGO_MANIFEST_DIR + ":" + module_path)`. Without the override key
    //! the absolute build directory leaks into the URL, so the hydration WASM and
    //! the server binary — built on different release runners — disagree on every
    //! route and all server-function calls 404 with the empty-body error
    //! `Invalid format: missing delimiter in ""`. Re-deriving the hash with the
    //! pinned key fails fast if the pin is dropped, changed, or the module moves.
    use leptos::server_fn::{xxhash_rust::const_xxh64::xxh64, ServerFn};

    fn pinned_route(fn_name: &str, module_path: &str) -> String {
        let hash = xxh64(format!("ersatzrs:{module_path}").as_bytes(), 0);
        format!("/api/{fn_name}{hash}")
    }

    #[test]
    fn server_fn_routes_are_build_dir_independent() {
        assert_eq!(
            super::ScanLibrary::PATH,
            pinned_route("scan_library", "ersatzrs_web::admin::server_fns::libraries"),
            "server_fn routes must hash the pinned SERVER_FN_OVERRIDE_KEY, not \
             CARGO_MANIFEST_DIR; see .cargo/config.toml",
        );
    }
}
