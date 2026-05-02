use std::path::PathBuf;
use std::time::Duration as StdDuration;

use ersatzrs_infra::cache_cleanup::CacheCleanupConfig;
use ersatzrs_infra::db;
use ersatzrs_infra::maintenance::{run_maintenance_once, MaintenancePassReport};
use tokio::time::{interval, MissedTickBehavior};
use tracing::{info, warn};

/// Scheduler CLI options after argument parsing.
#[derive(Debug, Clone)]
pub struct SchedulerOptions {
    /// Run a single pass and exit.
    pub run_once: bool,
    /// Outer tick for due-loop checks.
    pub tick: StdDuration,
    /// Optional transcode cache folder.
    pub transcode_dir: Option<PathBuf>,
    /// Optional channel-guide cache folder pruned of orphaned XML files.
    pub channel_guide_dir: Option<PathBuf>,
}

/// Run the background scheduler.
///
/// # Errors
///
/// Returns an error when database bootstrap or a run-once maintenance pass
/// fails. In loop mode, individual pass failures are logged and retried on the
/// next tick.
pub async fn run_scheduler(options: SchedulerOptions) -> anyhow::Result<()> {
    let startup = db::bootstrap_media_server_database().await?;
    let cache_config =
        CacheCleanupConfig::from_paths(options.transcode_dir, options.channel_guide_dir);

    if options.run_once {
        let report =
            run_maintenance_once(startup.pool, &cache_config, startup.schema_status).await?;
        log_report(&report);
        return Ok(());
    }

    info!("scheduler service started");
    let mut ticker = interval(options.tick);
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
                    startup.schema_status.clone(),
                )
                .await
                {
                    Ok(report) => log_report(&report),
                    Err(error) => warn!(%error, "scheduler pass failed"),
                }
            }
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
