use std::path::PathBuf;
use std::time::Duration as StdDuration;

use anyhow::Context;
use clap::Parser;
use ersatzrs_scheduler::runner::{run_scheduler, SchedulerOptions};

#[derive(Debug, Parser)]
#[command(about = "Run ErsatzRS background maintenance loops")]
struct Args {
    /// Run one maintenance pass and exit.
    #[arg(long)]
    run_once: bool,
    /// Override the transcode cache folder cleaned by the cache loop.
    #[arg(long)]
    transcode_dir: Option<PathBuf>,
    /// Override the image cache folder inspected by the cache loop.
    #[arg(long)]
    image_cache_dir: Option<PathBuf>,
    /// Override the log folder used by the log rotation loop.
    #[arg(long)]
    log_dir: Option<PathBuf>,
    /// Outer scheduler tick in seconds.
    #[arg(long, default_value_t = 60)]
    tick_seconds: u64,
}

#[tokio::main]
#[allow(clippy::disallowed_methods)]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();
    run_scheduler(SchedulerOptions {
        run_once: args.run_once,
        tick: StdDuration::from_secs(args.tick_seconds),
        transcode_dir: args.transcode_dir,
        image_cache_dir: args.image_cache_dir,
        log_dir: args.log_dir,
    })
    .await
    .context("run scheduler")
}
