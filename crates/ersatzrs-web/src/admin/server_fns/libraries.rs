use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(not(target_arch = "wasm32"))]
use ersatzrs_app::{
    commands::{
        BeginOperation, RequestLibraryScan, RequestScanOutcome, RequestShowScan, ScanRequest,
    },
    operations::OperationKey,
    queries::ScanOperationState,
};

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

/// Transport-neutral scan lifecycle row for diagnostics and future adapters.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScanOperationRow {
    pub operation_id: u64,
    pub library_id: i64,
    pub state: String,
    pub progress: Option<f32>,
    pub error: Option<String>,
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

/// Admit a background scan through the application-owned FIFO worker.
#[server]
pub async fn scan_library(
    library_id: i64,
    kind: String,
    deep_scan: bool,
) -> Result<bool, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    tracing::info!(library_id, kind = %kind, deep_scan, "scan_library invoked");
    map_scan_outcome(
        state
            .application
            .request_library_scan(RequestLibraryScan::new(library_id, deep_scan))
            .await,
    )
}

/// Fires a background scan for a single show.
///
/// Provider dispatch and Local full-library fallback are resolved from
/// persisted state by the application command. Mirrors
/// `$ERSATZTV_SRC/ErsatzTV.Application/Libraries/Commands/QueueShowScanByLibraryIdHandler.cs`.
#[server]
pub async fn scan_show(
    library_id: i64,
    show_id: i64,
    kind: String,
    deep_scan: bool,
) -> Result<bool, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    tracing::info!(
        library_id,
        show_id,
        kind = %kind,
        deep_scan,
        "scan_show invoked"
    );
    map_scan_outcome(
        state
            .application
            .request_show_scan(RequestShowScan::new(library_id, show_id, deep_scan))
            .await,
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn map_scan_outcome(outcome: RequestScanOutcome) -> Result<bool, ServerFnError> {
    match outcome {
        RequestScanOutcome::Accepted { .. } => Ok(true),
        RequestScanOutcome::Rejected(reason) => {
            tracing::warn!(?reason, "scan command rejected");
            Err(ServerFnError::new(format!(
                "scan request rejected: {reason:?}"
            )))
        }
        RequestScanOutcome::Unavailable { message } => Err(ServerFnError::new(message)),
    }
}

/// Returns the ids of libraries currently being scanned. Polled by the
/// libraries page to drive the per-row "scanning" spinner.
#[server]
#[allow(clippy::unused_async)]
pub async fn list_locked_libraries() -> Result<Vec<i64>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state.application.active_library_scans())
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
    Ok(state.application.scan_progress())
}

/// Returns retained queued, running, completed, and failed scan operations.
#[server]
#[allow(clippy::unused_async)]
pub async fn list_scan_operations() -> Result<Vec<ScanOperationRow>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
        .application
        .scan_operations()
        .into_iter()
        .map(|operation| {
            let (state, error) = match operation.state {
                ScanOperationState::Queued => ("queued", None),
                ScanOperationState::Running => ("running", None),
                ScanOperationState::Completed => ("completed", None),
                ScanOperationState::Failed { message } => ("failed", Some(message)),
            };
            ScanOperationRow {
                operation_id: operation.operation_id.0,
                library_id: match operation.request {
                    ScanRequest::Library { library_id, .. }
                    | ScanRequest::Show { library_id, .. } => library_id,
                },
                state: state.to_owned(),
                progress: operation.progress,
                error,
            }
        })
        .collect())
}

/// One row of the External Collections table. Mirrors ETV's
/// `GetExternalCollectionsHandler` — one row per remote `MediaSource` whose
/// libraries are flagged with `ShouldSyncItems = 1`.
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
/// gated by the shared application operation coordinator.
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
    let operations = state.application.operations().clone();
    let key = OperationKey::ExternalCollections {
        provider: kind.clone(),
        media_source_id,
    };

    tracing::info!(media_source_id, kind = %kind, deep_scan, "scan_external_collections invoked");

    let Some(operation_guard) = operations.begin(BeginOperation::new(key)).into_guard() else {
        tracing::info!(media_source_id, kind = %kind, "scan_external_collections: already locked, no-op");
        return Ok(false);
    };

    tokio::task::spawn(async move {
        let _operation_guard = operation_guard;
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
    Ok(state.application.operations().active_external_collections())
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
    use ersatzrs_app::commands::{RequestScanOutcome, RequestScanRejection, ScanOperationId};
    use leptos::server_fn::{xxhash_rust::const_xxh64::xxh64, ServerFn};

    fn pinned_route(fn_name: &str, module_path: &str) -> String {
        let hash = xxh64(format!("ersatzrs:{module_path}").as_bytes(), 0);
        format!("/api/{fn_name}{hash}")
    }

    #[test]
    fn server_fn_routes_are_build_dir_independent() {
        for (actual, function_name) in [
            (super::ScanLibrary::PATH, "scan_library"),
            (super::ScanShow::PATH, "scan_show"),
            (super::ListLockedLibraries::PATH, "list_locked_libraries"),
            (super::ListScanProgress::PATH, "list_scan_progress"),
            (super::ListScanOperations::PATH, "list_scan_operations"),
        ] {
            assert_eq!(
                actual,
                pinned_route(function_name, "ersatzrs_web::admin::server_fns::libraries"),
                "server_fn routes must hash the pinned SERVER_FN_OVERRIDE_KEY, not \
                 CARGO_MANIFEST_DIR; see .cargo/config.toml",
            );
        }
    }

    #[test]
    fn rejected_scan_outcomes_are_transport_errors() {
        assert!(super::map_scan_outcome(RequestScanOutcome::Accepted {
            operation_id: ScanOperationId(1),
        })
        .expect("accepted scan"));

        let error = super::map_scan_outcome(RequestScanOutcome::Rejected(
            RequestScanRejection::SynchronizationDisabled,
        ))
        .expect_err("rejected scan must not be rendered as queued");
        assert!(error.to_string().contains("SynchronizationDisabled"));
    }
}
