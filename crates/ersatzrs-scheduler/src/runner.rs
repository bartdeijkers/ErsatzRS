use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration as StdDuration;

use ersatzrs_core::hls::HlsSessionRegistry;
use ersatzrs_core::locking::{EntityLocker, PlayoutBuildActivity};
use ersatzrs_core::scheduling::PlayoutBuildRequest;
use ersatzrs_infra::cache_cleanup::CacheCleanupConfig;
use ersatzrs_infra::db;
use ersatzrs_infra::maintenance::{
    repair_invalid_classic_playouts, run_maintenance_once, run_requested_playout_build,
    MaintenancePassReport,
};
use tokio::sync::mpsc;
use tokio::time::{interval, MissedTickBehavior};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

/// Scheduler CLI options after argument parsing.
#[derive(Default)]
pub struct SchedulerOptions {
    /// Run a single pass and exit.
    pub run_once: bool,
    /// Outer tick for due-loop checks.
    pub tick: StdDuration,
    /// Optional transcode cache folder.
    pub transcode_dir: Option<PathBuf>,
    /// Optional channel-guide cache folder pruned of orphaned XML files.
    pub channel_guide_dir: Option<PathBuf>,
    /// Shared application command boundary for periodic library scans.
    pub application: Option<Arc<ersatzrs_app::ApplicationServices>>,
    /// Requests from the web host for immediate playout rebuilds.
    pub build_requests: Option<mpsc::UnboundedReceiver<PlayoutBuildRequest>>,
    /// Shared activity registry used by the web host to distinguish accepted
    /// queued work from a build that has started.
    pub build_activity: Option<PlayoutBuildActivity>,
    /// Optional live HLS session registry supplied by the web host so playout
    /// rebuilds can notify active segmenter workers.
    pub hls_sessions: Option<Arc<dyn HlsSessionRegistry>>,
    /// Shared playout lock registry. When provided, the scheduler reuses this
    /// `EntityLocker` instead of constructing its own so the web host can poll
    /// playout lock state for the playouts-page build spinner. Mirrors
    /// ErsatzTV's single-process `IEntityLocker` singleton.
    pub locker: Option<EntityLocker>,
    /// Optional external shutdown signal. When supplied (e.g. the web host's
    /// coordinated `CancellationToken`), the loop exits on cancellation rather
    /// than installing its own `ctrl_c` handler. When `None` (standalone use),
    /// it falls back to `tokio::signal::ctrl_c()`.
    pub shutdown: Option<CancellationToken>,
}

/// Run the background scheduler.
///
/// # Errors
///
/// Returns an error when database bootstrap or a run-once maintenance pass
/// fails. In loop mode, individual pass failures are logged and retried on the
/// next tick.
pub async fn run_scheduler(options: SchedulerOptions) -> anyhow::Result<()> {
    let SchedulerOptions {
        run_once,
        tick,
        transcode_dir,
        channel_guide_dir,
        application,
        mut build_requests,
        build_activity,
        hls_sessions,
        locker,
        shutdown,
    } = options;

    let startup = db::bootstrap_media_server_database().await?;
    let cache_config = CacheCleanupConfig::from_paths(transcode_dir, channel_guide_dir);
    // Single in-process `EntityLocker` shared across maintenance passes,
    // matching ErsatzTV's `IEntityLocker` singleton. When the web host supplies
    // one, reuse it so the admin UI can observe playout lock state.
    let locker = locker.unwrap_or_default();

    repair_classic_playouts_on_startup(&startup.pool, &locker, hls_sessions.as_deref()).await?;

    recover_scan_refreshes_on_startup(application.as_ref()).await?;

    if run_once {
        let report = run_maintenance_once(
            startup.pool,
            &cache_config,
            application.as_deref().map(|services| {
                services as &dyn ersatzrs_core::maintenance::ScheduledLibraryScanRequester
            }),
            &locker,
            startup.schema_status,
            hls_sessions.clone(),
        )
        .await?;
        log_report(&report);
        return Ok(());
    }

    info!("scheduler service started");
    let mut ticker = interval(tick);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            () = wait_for_shutdown(shutdown.as_ref()) => {
                info!("scheduler service shutting down");
                return Ok(());
            }
            _ = ticker.tick() => {
                retry_pending_scan_refreshes(application.as_ref()).await;
                match run_maintenance_once(
                    startup.pool.clone(),
                    &cache_config,
                    application.as_deref().map(|services| {
                        services as &dyn ersatzrs_core::maintenance::ScheduledLibraryScanRequester
                    }),
                    &locker,
                    startup.schema_status.clone(),
                    hls_sessions.clone(),
                )
                .await
                {
                    Ok(report) => log_report(&report),
                    Err(error) => warn!(%error, "scheduler pass failed"),
                }
            }
            request = recv_build_request(&mut build_requests) => {
                let Some(request) = request else {
                    build_requests = None;
                    continue;
                };
                let (request, _activity_guard) = if let Some(activity) = &build_activity {
                    let (request, guard) = activity.start_guard(request);
                    (request, Some(guard))
                } else {
                    (request, None)
                };
                let playout_id = request.playout_id;
                let mode = request.mode;
                match run_requested_playout_build(
                    &startup.pool,
                    &locker,
                    request,
                    chrono::Utc::now(),
                    hls_sessions.as_deref(),
                )
                .await
                {
                    Ok(true) => info!(playout_id, ?mode, "requested playout build complete"),
                    Ok(false) => info!(playout_id, ?mode, "requested playout build skipped because playout is locked"),
                    Err(error) => warn!(playout_id, ?mode, %error, "requested playout build failed"),
                }
            }
        }
    }
}

async fn repair_classic_playouts_on_startup(
    pool: &ersatzrs_infra::db::DbPool,
    locker: &EntityLocker,
    hls_sessions: Option<&dyn HlsSessionRegistry>,
) -> anyhow::Result<()> {
    let repaired =
        repair_invalid_classic_playouts(pool, locker, chrono::Utc::now(), hls_sessions).await?;
    if repaired > 0 {
        info!(
            repaired,
            "repaired inconsistent Classic playout runtime state"
        );
    }
    Ok(())
}

async fn recover_scan_refreshes_on_startup(
    application: Option<&Arc<ersatzrs_app::ApplicationServices>>,
) -> anyhow::Result<()> {
    let Some(application) = application else {
        return Ok(());
    };
    let recovered = application.recover_scan_playout_refreshes().await?;
    if recovered > 0 {
        info!(
            recovered,
            "re-admitted durable scan-triggered playout refreshes"
        );
    }
    Ok(())
}

async fn retry_pending_scan_refreshes(
    application: Option<&Arc<ersatzrs_app::ApplicationServices>>,
) {
    let Some(application) = application else {
        return;
    };
    match application.retry_pending_scan_playout_refreshes().await {
        Ok(retried) if retried > 0 => info!(
            retried,
            "re-admitted pending scan-triggered playout refreshes"
        ),
        Ok(_) => {}
        Err(error) => warn!(%error, "scan-triggered playout refresh retry failed"),
    }
}

async fn recv_build_request(
    receiver: &mut Option<mpsc::UnboundedReceiver<PlayoutBuildRequest>>,
) -> Option<PlayoutBuildRequest> {
    match receiver {
        Some(receiver) => receiver.recv().await,
        None => std::future::pending::<Option<PlayoutBuildRequest>>().await,
    }
}

/// Resolve when the scheduler should shut down. Prefers an externally supplied
/// `CancellationToken` (the web host's coordinated shutdown); otherwise installs
/// a standalone `ctrl_c` handler so direct callers still stop cleanly.
async fn wait_for_shutdown(token: Option<&CancellationToken>) {
    match token {
        Some(token) => token.cancelled().await,
        None => {
            let _ = tokio::signal::ctrl_c().await;
        }
    }
}

fn log_report(report: &MaintenancePassReport) {
    info!(
        playout_loop_ran = report.loops_ran.playout(),
        playouts_built = report.playouts_built,
        playouts_skipped_locked = report.playouts_skipped_locked,
        playouts_failed = report.playouts_failed,
        playouts_reset = report.playouts_reset,
        library_loop_ran = report.loops_ran.library_refresh(),
        libraries_dispatched = report.libraries_dispatched,
        libraries_failed = report.libraries_failed,
        provider_loop_ran = report.loops_ran.provider_poll(),
        providers_polled = report.providers_polled,
        providers_reachable = report.providers_reachable,
        cache_loop_ran = report.loops_ran.cache_cleanup(),
        transcode_files_removed = report.cache_cleanup.transcode_files_removed,
        channel_guide_files_removed = report.cache_cleanup.channel_guide_files_removed,
        search_loop_ran = report.loops_ran.search_index(),
        search_index_dispatched = report.search_index_dispatched,
        health_warnings = report.health_summary.map(|summary| summary.warnings),
        health_errors = report.health_summary.map(|summary| summary.errors),
        "scheduler pass complete"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// A supplied token must resolve `wait_for_shutdown` as soon as it is
    /// cancelled, which is how the web host drains the scheduler loop.
    #[tokio::test]
    async fn wait_for_shutdown_resolves_when_token_cancelled() {
        let token = CancellationToken::new();
        let waiter = token.clone();
        let handle = tokio::spawn(async move { wait_for_shutdown(Some(&waiter)).await });

        token.cancel();

        let result = tokio::time::timeout(Duration::from_secs(1), handle).await;
        assert!(
            matches!(result, Ok(Ok(()))),
            "wait_for_shutdown did not resolve after the token was cancelled"
        );
    }

    /// While the token is live, `wait_for_shutdown` must keep pending so the
    /// scheduler stays in its loop.
    #[tokio::test]
    async fn wait_for_shutdown_pending_while_token_live() {
        let token = CancellationToken::new();
        let result =
            tokio::time::timeout(Duration::from_millis(50), wait_for_shutdown(Some(&token))).await;
        assert!(
            result.is_err(),
            "wait_for_shutdown resolved while the token was still live"
        );
    }
}
