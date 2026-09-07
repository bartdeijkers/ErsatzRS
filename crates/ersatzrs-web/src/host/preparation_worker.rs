//! Private preparation process lifetime; the existing coordinator owns all work.

use std::io::Read as _;
use std::path::Path;
use std::process::Stdio;

use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_infra::db::DbPool;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

/// Internal same-executable mode, dispatched before normal web-host startup.
pub const PRIVATE_WORKER_ARGUMENT: &str = "--internal-addon-preparation-worker";

/// Run against the already initialized database without HTTP or startup side effects.
///
/// # Errors
/// Returns a redacted error if configuration, ownership or cleanup cannot be proven.
pub async fn run_private_worker() -> Result<()> {
    let database_url = ersatzrs_infra::db::resolve_database_url().map_err(|_| unavailable())?;
    let pool = ersatzrs_infra::db::lazy_sqlite_pool(&database_url).map_err(|_| unavailable())?;
    let root = ersatzrs_infra::file_system_layout::addon_processed_media_folder();
    let (shutdown, cancellation) = tokio::sync::oneshot::channel();
    // Tokio stdin uses an uncancellable blocking read that can hold up runtime
    // shutdown. This dedicated thread owns only the parent's liveness pipe and
    // never keeps the worker process alive after its async work has drained.
    std::thread::Builder::new()
        .name("preparation-host-liveness".into())
        .spawn(move || {
            let mut byte = [0];
            loop {
                match std::io::stdin().read(&mut byte) {
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                    _ => break,
                }
            }
            // EOF, an unexpected byte or a read failure all request safe shutdown.
            let _ = shutdown.send(());
        })
        .map_err(|_| unavailable())?;
    ersatzrs_infra::addon_preparation_worker::run_preparation_worker(pool, &root, cancellation)
        .await
}

/// Keep the worker independent of host termination and await its normal drain.
pub(crate) async fn supervise(pool: DbPool, shutdown: CancellationToken) {
    if shutdown.is_cancelled() {
        return;
    }
    let Ok(executable) = std::env::current_exe() else {
        tracing::warn!("add-on preparation worker executable is unavailable");
        record_worker_unavailable(&pool).await;
        return;
    };
    let mut command = Command::new(executable);
    command
        .arg(PRIVATE_WORKER_ARGUMENT)
        .env("ErsatzRS_FFMPEG__PATH", ersatzrs_ffmpeg::ffmpeg_path())
        .env("ErsatzRS_FFPROBE__PATH", ersatzrs_ffmpeg::ffprobe_path());
    let root = ersatzrs_infra::file_system_layout::addon_processed_media_folder();
    supervise_command(pool, shutdown, &root, command).await;
}

async fn supervise_command(
    pool: DbPool,
    shutdown: CancellationToken,
    root: &Path,
    mut command: Command,
) {
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // This process owns cleanup after host exit. Do not wrap it in the host's
    // managed add-on group/job or enable kill-on-drop.
    #[cfg(unix)]
    command.process_group(0);
    #[cfg(windows)]
    {
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }
    let Ok(mut child) = command.spawn() else {
        tracing::warn!("add-on preparation worker could not start");
        record_worker_unavailable(&pool).await;
        return;
    };
    // Child::wait closes stdin before observing exit. Keep the host liveness
    // lease separately so polling for an unexpected exit does not stop the worker.
    let host_liveness = child.stdin.take();
    let result = tokio::select! {
        biased;
        () = shutdown.cancelled() => {
            // Closing the only parent write handle also works after SIGKILL or
            // TerminateProcess. Never kill the worker before it records cleanup.
            drop(host_liveness);
            child.wait().await
        },
        result = child.wait() => result,
    };
    if let Ok(status) = result {
        if ersatzrs_infra::addon_preparation_worker::mark_worker_exit_uncertain(&pool, root)
            .await
            .is_err()
        {
            tracing::warn!("add-on preparation worker recovery state could not be recorded");
        }
        if !status.success() || !shutdown.is_cancelled() {
            record_worker_unavailable(&pool).await;
            tracing::warn!(
                "add-on preparation worker stopped; inspect preparation recovery health"
            );
        }
    } else {
        record_worker_unavailable(&pool).await;
        tracing::warn!("add-on preparation worker exit could not be observed");
    }
}

async fn record_worker_unavailable(pool: &DbPool) {
    if ersatzrs_infra::addon_preparation_worker::set_worker_unavailable(pool, true)
        .await
        .is_err()
    {
        tracing::warn!("add-on preparation worker availability could not be recorded");
    }
}

fn unavailable() -> RepositoryError {
    RepositoryError::unavailable(
        "item preparation",
        "the private preparation worker is unavailable",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn assert_unavailable(pool: &DbPool) {
        let value: String = sqlx::query_scalar(
            r#"SELECT "Value" FROM "ConfigElement" WHERE "Key"='addon.preparation.worker-unavailable'"#,
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(value, "true");
        let checkpoint: Option<String> = sqlx::query_scalar(
            r#"SELECT "Value" FROM "ConfigElement" WHERE "Key"='addon.preparation.worker'"#,
        )
        .fetch_optional(pool)
        .await
        .unwrap();
        assert!(
            checkpoint.is_none(),
            "unavailability must not invent cleanup evidence"
        );
    }

    #[tokio::test]
    async fn worker_spawn_failure_is_visible_without_an_active_attempt() {
        let temporary = tempfile::tempdir().unwrap();
        let pool = ersatzrs_infra::db::init("sqlite::memory:").await.unwrap();
        supervise_command(
            pool.clone(),
            CancellationToken::new(),
            temporary.path(),
            Command::new(temporary.path().join("missing-worker")),
        )
        .await;
        assert_unavailable(&pool).await;
    }

    #[tokio::test]
    async fn worker_early_idle_exit_is_visible_without_an_active_attempt() {
        let temporary = tempfile::tempdir().unwrap();
        let pool = ersatzrs_infra::db::init("sqlite::memory:").await.unwrap();
        #[cfg(unix)]
        let mut command = Command::new("/bin/sh");
        #[cfg(unix)]
        command.args(["-c", "exit 0"]);
        #[cfg(windows)]
        let mut command = Command::new("cmd.exe");
        #[cfg(windows)]
        command.args(["/D", "/C", "exit 0"]);
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            supervise_command(
                pool.clone(),
                CancellationToken::new(),
                temporary.path(),
                command,
            ),
        )
        .await
        .unwrap();
        assert_unavailable(&pool).await;
    }

    #[tokio::test]
    async fn worker_stdin_liveness_survives_exit_polling_until_shutdown() {
        let temporary = tempfile::tempdir().unwrap();
        let ready = temporary.path().join("ready");
        let drained = temporary.path().join("drained");
        let pool = ersatzrs_infra::db::init("sqlite::memory:").await.unwrap();
        #[cfg(unix)]
        let mut command = Command::new("/bin/sh");
        #[cfg(unix)]
        command.args([
            "-c",
            "printf ready > \"$ERSATZRS_TEST_READY\"; read -r parent_lease; printf drained > \"$ERSATZRS_TEST_DRAINED\"; exit 0",
        ]);
        #[cfg(windows)]
        let mut command = Command::new("cmd.exe");
        #[cfg(windows)]
        command.args([
            "/D",
            "/C",
            "echo ready>\"%ERSATZRS_TEST_READY%\" & set /p parent_lease= & echo drained>\"%ERSATZRS_TEST_DRAINED%\" & exit /b 0",
        ]);
        command
            .env("ERSATZRS_TEST_READY", &ready)
            .env("ERSATZRS_TEST_DRAINED", &drained);
        let shutdown = CancellationToken::new();
        let supervision =
            supervise_command(pool.clone(), shutdown.clone(), temporary.path(), command);
        tokio::pin!(supervision);
        let observation = async {
            let started = tokio::time::timeout(std::time::Duration::from_secs(5), async {
                while !ready.exists() {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            })
            .await
            .is_ok();
            // A real child needs a scheduling opportunity after its ready marker;
            // virtual time alone cannot prove that its stdin remains open.
            if started {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            started && !drained.exists()
        };
        let (finished, retained_lease) = tokio::select! {
            () = &mut supervision => (true, false),
            retained = observation => (false, retained),
        };
        // Always release the lease and await cleanup before any assertion.
        shutdown.cancel();
        if !finished {
            tokio::time::timeout(std::time::Duration::from_secs(5), &mut supervision)
                .await
                .unwrap();
        }
        assert!(
            retained_lease,
            "exit polling must not close the stdin lease"
        );
        assert!(
            drained.exists(),
            "shutdown must deliver EOF and await drain"
        );
        let unavailable: Option<String> = sqlx::query_scalar(
            r#"SELECT "Value" FROM "ConfigElement" WHERE "Key"='addon.preparation.worker-unavailable'"#,
        )
        .fetch_optional(&pool)
        .await
        .unwrap();
        assert_ne!(unavailable.as_deref(), Some("true"));
    }
}
