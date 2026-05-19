use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration as StdDuration;

use ersatzrs_core::locking::EntityLocker;
use ersatzrs_core::scheduling::PlayoutBuildRequest;
use ersatzrs_infra::cache_cleanup::CacheCleanupConfig;
use ersatzrs_infra::db;
use ersatzrs_infra::maintenance::{
    run_maintenance_once, run_requested_playout_build, LibraryDispatchConfig, MaintenancePassReport,
};
use tokio::sync::mpsc;
use tokio::time::{interval, MissedTickBehavior};
use tracing::{info, warn};

/// Scheduler CLI options after argument parsing.
#[derive(Debug, Default)]
pub struct SchedulerOptions {
    /// Run a single pass and exit.
    pub run_once: bool,
    /// Outer tick for due-loop checks.
    pub tick: StdDuration,
    /// Optional transcode cache folder.
    pub transcode_dir: Option<PathBuf>,
    /// Optional channel-guide cache folder pruned of orphaned XML files.
    pub channel_guide_dir: Option<PathBuf>,
    /// Override path to the `ersatzrs-scanner` binary used by the library
    /// refresh dispatcher.
    pub scanner_command: Option<PathBuf>,
    /// Extra arguments appended to every scanner invocation.
    pub scanner_extra_args: Vec<OsString>,
    /// Requests from the web host for immediate playout rebuilds.
    pub build_requests: Option<mpsc::Receiver<PlayoutBuildRequest>>,
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
        scanner_command,
        scanner_extra_args,
        mut build_requests,
    } = options;

    let startup = db::bootstrap_media_server_database().await?;
    let cache_config = CacheCleanupConfig::from_paths(transcode_dir, channel_guide_dir);
    let library_dispatch = LibraryDispatchConfig {
        scanner_command,
        extra_args: scanner_extra_args,
    };
    // Single in-process `EntityLocker` shared across maintenance passes,
    // matching ErsatzTV's `IEntityLocker` singleton.
    let locker = EntityLocker::new();

    if run_once {
        let report = run_maintenance_once(
            startup.pool,
            &cache_config,
            &library_dispatch,
            &locker,
            startup.schema_status,
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
            _ = tokio::signal::ctrl_c() => {
                info!("scheduler service shutting down");
                return Ok(());
            }
            _ = ticker.tick() => {
                match run_maintenance_once(
                    startup.pool.clone(),
                    &cache_config,
                    &library_dispatch,
                    &locker,
                    startup.schema_status.clone(),
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
                let playout_id = request.playout_id;
                let mode = request.mode;
                match run_requested_playout_build(&startup.pool, &locker, request, chrono::Utc::now()).await {
                    Ok(true) => info!(playout_id, ?mode, "requested playout build complete"),
                    Ok(false) => info!(playout_id, ?mode, "requested playout build skipped because playout is locked"),
                    Err(error) => warn!(playout_id, ?mode, %error, "requested playout build failed"),
                }
            }
        }
    }
}

async fn recv_build_request(
    receiver: &mut Option<mpsc::Receiver<PlayoutBuildRequest>>,
) -> Option<PlayoutBuildRequest> {
    match receiver {
        Some(receiver) => receiver.recv().await,
        None => std::future::pending::<Option<PlayoutBuildRequest>>().await,
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
