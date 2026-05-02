use std::path::PathBuf;
use std::time::Duration as StdDuration;

use ersatzrs_core::maintenance::{maintenance_loops, MaintenanceLoop};
use ersatzrs_infra::cache_cleanup::CacheCleanupConfig;
use ersatzrs_infra::db;
use ersatzrs_infra::maintenance::{
    load_library_refresh_interval, load_loop_last_run, run_maintenance_once, MaintenancePassReport,
};
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
    /// Optional image cache folder.
    pub image_cache_dir: Option<PathBuf>,
    /// Optional log folder.
    pub log_dir: Option<PathBuf>,
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
    let cache_config = CacheCleanupConfig::from_paths(
        options.transcode_dir,
        options.image_cache_dir,
        options.log_dir,
    );

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
                let library_refresh_interval = load_library_refresh_interval(&startup.pool).await?;
                let loops = maintenance_loops(library_refresh_interval);
                if any_loop_due(&startup.pool, &loops).await? {
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
}

async fn any_loop_due(
    pool: &ersatzrs_infra::db::DbPool,
    loops: &[MaintenanceLoop],
) -> anyhow::Result<bool> {
    let now = chrono::Utc::now();
    for loop_def in loops {
        let last_run = load_loop_last_run(pool, loop_def.name).await?;
        if loop_def.is_due(last_run, now) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn log_report(report: &MaintenancePassReport) {
    info!(
        playouts_built = report.playouts_built,
        playouts_skipped_locked = report.playouts_skipped_locked,
        playouts_failed = report.playouts_failed,
        libraries_dispatched = report.libraries_dispatched,
        providers_polled = report.providers_polled,
        providers_reachable = report.providers_reachable,
        transcode_files_removed = report.cache_cleanup.transcode_files_removed,
        image_files_removed = report.cache_cleanup.image_files_removed,
        log_files_removed = report.cache_cleanup.log_files_removed,
        search_index_dispatched = report.search_index_dispatched,
        health_warnings = report.health_summary.map(|summary| summary.warnings),
        health_errors = report.health_summary.map(|summary| summary.errors),
        "scheduler pass complete"
    );
}
