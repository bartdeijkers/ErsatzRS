use anyhow::{bail, Context};
use ersatzrs_scanner::{scan_local_library, LocalScanConfig};
use sqlx::sqlite::SqlitePoolOptions;

#[tokio::main]
#[allow(clippy::disallowed_methods)]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 || args[1] != "local" {
        bail!("usage: ersatzrs-scanner local <sqlite-database-url> <library-id> [ffprobe-path]");
    }

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&args[2])
        .await
        .context("connect to sqlite database")?;
    let library_id = args[3].parse::<i64>().context("parse library id")?;
    let ffprobe_path = args.get(4).cloned().unwrap_or_else(|| "ffprobe".to_owned());

    let report = scan_local_library(
        &pool,
        &LocalScanConfig {
            library_id,
            ffprobe_path,
        },
    )
    .await?;
    println!("scanned {} local media files", report.scanned_files);
    Ok(())
}
