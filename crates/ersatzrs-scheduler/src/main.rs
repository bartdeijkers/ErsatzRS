use std::ffi::OsString;
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
    /// Override the channel-guide cache folder pruned by the cache loop.
    #[arg(long)]
    channel_guide_dir: Option<PathBuf>,
    /// Override the path to the `ersatzrs-scanner` binary used by the library
    /// refresh dispatcher.
    #[arg(long = "scanner-command")]
    scanner_path: Option<PathBuf>,
    /// Extra arguments forwarded to every scanner invocation.
    #[arg(long = "scanner-arg")]
    scanner_extra: Vec<OsString>,
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
        channel_guide_dir: args.channel_guide_dir,
        scanner_command: args.scanner_path,
        scanner_extra_args: args.scanner_extra,
    })
    .await
    .context("run scheduler")
}
