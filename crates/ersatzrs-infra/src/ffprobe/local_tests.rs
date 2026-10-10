use std::pin::Pin;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::task::{Context, Poll};

use tokio::io::{AsyncWriteExt as _, DuplexStream, ReadBuf};

use super::*;

struct TrackedReader {
    pipe: DuplexStream,
    dropped: Arc<AtomicBool>,
}

impl AsyncRead for TrackedReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.pipe).poll_read(context, buffer)
    }
}

impl Drop for TrackedReader {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
    }
}

fn tracked_pair() -> (TrackedReader, DuplexStream, Arc<AtomicBool>) {
    let (pipe, writer) = tokio::io::duplex(64);
    let dropped = Arc::new(AtomicBool::new(false));
    (
        TrackedReader {
            pipe,
            dropped: dropped.clone(),
        },
        writer,
        dropped,
    )
}

#[tokio::test]
async fn stdout_accepts_exactly_the_limit_and_rejects_the_next_byte() {
    let bytes = vec![b' '; MAX_LOCAL_STDOUT_BYTES + 1];
    assert_eq!(
        read_stdout(&bytes[..MAX_LOCAL_STDOUT_BYTES])
            .await
            .unwrap()
            .len(),
        MAX_LOCAL_STDOUT_BYTES,
    );
    assert_eq!(
        read_stdout(bytes.as_slice()).await,
        Err(LocalProbeProblem::OutputTooLarge),
    );
}

#[tokio::test]
async fn stdout_overflow_is_detected_without_waiting_for_eof() {
    let (reader, mut writer) = tokio::io::duplex(8192);
    let writer_task = tokio::spawn(async move {
        let chunk = [0; 8192];
        for _ in 0..MAX_LOCAL_STDOUT_BYTES / chunk.len() {
            writer.write_all(&chunk).await.unwrap();
        }
        writer.write_all(&[0]).await.unwrap();
        std::future::pending::<()>().await;
        drop(writer);
    });
    let output = tokio::time::timeout(Duration::from_secs(3), read_stdout(reader))
        .await
        .expect("overflow detection must not await pipe closure");
    writer_task.abort();
    let _ = writer_task.await;
    assert_eq!(output, Err(LocalProbeProblem::OutputTooLarge));
}

#[tokio::test]
async fn stderr_continuously_drains_and_retains_only_the_bounded_tail() {
    let mut bytes = vec![b'a'; MAX_LOCAL_STDERR_TAIL_BYTES * 8];
    bytes.extend_from_slice(b"last diagnostic");
    let result = read_stderr(bytes.as_slice()).await.unwrap();
    assert_eq!(result.len(), MAX_LOCAL_STDERR_TAIL_BYTES);
    assert!(result.ends_with("last diagnostic"));
}

#[tokio::test(start_paused = true)]
async fn a_completed_reader_is_not_polled_twice_when_its_sibling_times_out() {
    let (stdout, stdout_writer, stdout_dropped) = tracked_pair();
    let (stderr, stderr_writer, stderr_dropped) = tracked_pair();
    drop(stdout_writer);
    let mut readers = ProbeReaders::spawn(stdout, stderr);
    assert_eq!(
        readers.collect().await,
        Err(LocalProbeProblem::ReaderShutdown)
    );
    assert!(readers.stdout_task.is_none());
    assert!(readers.stderr_task.is_none());
    assert!(stdout_dropped.load(Ordering::SeqCst));
    assert!(stderr_dropped.load(Ordering::SeqCst));
    drop(stderr_writer);
}

#[tokio::test(start_paused = true)]
async fn reader_shutdown_aborts_and_joins_both_pending_owners() {
    let (stdout, stdout_writer, stdout_dropped) = tracked_pair();
    let (stderr, stderr_writer, stderr_dropped) = tracked_pair();
    let mut readers = ProbeReaders::spawn(stdout, stderr);
    assert_eq!(
        readers.collect().await,
        Err(LocalProbeProblem::ReaderShutdown)
    );
    assert!(stdout_dropped.load(Ordering::SeqCst));
    assert!(stderr_dropped.load(Ordering::SeqCst));
    assert!(readers.stdout_task.is_none());
    assert!(readers.stderr_task.is_none());
    drop((stdout_writer, stderr_writer));
}

#[tokio::test(start_paused = true)]
async fn a_failed_reader_does_not_detach_its_pending_sibling() {
    let (stdout, stdout_writer, stdout_dropped) = tracked_pair();
    let (stderr, stderr_writer, stderr_dropped) = tracked_pair();
    drop(stdout_writer);
    let mut readers = ProbeReaders::spawn(stdout, stderr);
    let result = await_reader(&mut readers.stdout_task).await;
    readers.accept_stdout(result).unwrap();
    readers.stdout = Some(Err(LocalProbeProblem::StdoutRead));
    assert_eq!(
        readers.collect().await,
        Err(LocalProbeProblem::ReaderShutdown)
    );
    assert!(stdout_dropped.load(Ordering::SeqCst));
    assert!(stderr_dropped.load(Ordering::SeqCst));
    drop(stderr_writer);
}

// Unix shell/process-group fixtures are intentionally skipped on Windows;
// reader bounds and ownership tests above are portable. Native Windows job
// acceptance remains a separate gate for the shared ManagedProcess adapter.
#[cfg(unix)]
mod unix {
    use super::*;
    use crate::ffprobe::{LocalProbeCleanupError, LocalProbeCleanupScope};

    fn command(script: &str, arguments: &[PathBuf]) -> FfprobeCommand {
        FfprobeCommand {
            program: "/bin/sh".to_owned(),
            args: [
                "-c".to_owned(),
                script.to_owned(),
                "probe-fixture".to_owned(),
            ]
            .into_iter()
            .chain(
                arguments
                    .iter()
                    .map(|path| path.to_string_lossy().into_owned()),
            )
            .collect(),
        }
    }

    async fn wait_for_pid_file(path: &std::path::Path) -> nix::unistd::Pid {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if let Ok(text) = tokio::fs::read_to_string(path).await {
                    if let Ok(pid) = text.trim().parse::<i32>() {
                        return nix::unistd::Pid::from_raw(pid);
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("fixture process must report its identity")
    }

    async fn wait_for_exit(pid: nix::unistd::Pid) {
        tokio::time::timeout(Duration::from_secs(8), async {
            while nix::sys::signal::kill(pid, None) != Err(nix::errno::Errno::ESRCH) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("owned process must be killed and reaped");
    }

    #[tokio::test]
    async fn oversized_output_kills_and_reaps_a_process_that_would_keep_running() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("pid");
        let operation = extract(
            command(
                "printf '%s' \"$$\" >\"$1\"; head -c 8388609 /dev/zero; exec sleep 30",
                std::slice::from_ref(&marker),
            ),
            PathBuf::from("synthetic.ts"),
        );
        let result = tokio::time::timeout(Duration::from_secs(8), operation)
            .await
            .expect("overflow must cancel the active process");
        assert!(matches!(result, Err(FfprobeError::OutputTooLarge { .. })));
        let pid = wait_for_pid_file(&marker).await;
        assert_eq!(
            nix::sys::signal::kill(pid, None),
            Err(nix::errno::Errno::ESRCH)
        );
    }

    #[tokio::test]
    async fn an_abandoned_caller_leaves_cleanup_with_the_owned_worker() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("pid");
        let caller = tokio::spawn(extract(
            command(
                "printf '%s' \"$$\" >\"$1\"; exec sleep 30",
                std::slice::from_ref(&marker),
            ),
            PathBuf::from("synthetic.ts"),
        ));
        let pid = wait_for_pid_file(&marker).await;
        assert!(nix::sys::signal::kill(pid, None).is_ok());
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        wait_for_exit(pid).await;
    }

    #[tokio::test]
    async fn closing_a_scope_cancels_a_live_probe_and_confirms_cleanup_before_response() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("pid");
        let scope = LocalProbeCleanupScope::new();
        let caller_scope = scope.clone();
        let owned_marker = marker.clone();
        let caller = tokio::spawn(async move {
            caller_scope
                .scope(extract(
                    command("printf '%s' \"$$\" >\"$1\"; exec sleep 30", &[owned_marker]),
                    PathBuf::from("synthetic.ts"),
                ))
                .await
        });
        let pid = wait_for_pid_file(&marker).await;
        assert_eq!(scope.cleanup_status(), Ok(false));
        scope.close();
        assert!(matches!(
            caller.await.unwrap(),
            Err(FfprobeError::Cancelled { .. })
        ));
        // Completion acknowledgement precedes the response. The caller cannot
        // observe cancellation while its managed process or readers remain live.
        assert_eq!(scope.cleanup_status(), Ok(true));
        assert_eq!(scope.wait_cleanup().await, Ok(()));
        assert_eq!(
            nix::sys::signal::kill(pid, None),
            Err(nix::errno::Errno::ESRCH)
        );
    }

    #[tokio::test]
    async fn scoped_caller_drop_can_be_awaited_without_an_operation_deadline() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("pid");
        let scope = LocalProbeCleanupScope::new();
        let caller_scope = scope.clone();
        let owned_marker = marker.clone();
        let caller = tokio::spawn(async move {
            caller_scope
                .scope(extract(
                    command("printf '%s' \"$$\" >\"$1\"; exec sleep 30", &[owned_marker]),
                    PathBuf::from("synthetic.ts"),
                ))
                .await
        });
        let pid = wait_for_pid_file(&marker).await;
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        scope.close();
        assert_eq!(scope.wait_cleanup().await, Ok(()));
        assert_eq!(
            nix::sys::signal::kill(pid, None),
            Err(nix::errno::Errno::ESRCH)
        );
    }

    #[tokio::test]
    async fn closed_or_uncertain_scope_does_not_start_another_process() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("must-not-exist");
        let closed = LocalProbeCleanupScope::new();
        closed.close();
        let result = closed
            .scope(extract(
                command("printf spawned >\"$1\"", std::slice::from_ref(&marker)),
                PathBuf::from("synthetic.ts"),
            ))
            .await;
        assert!(matches!(result, Err(FfprobeError::Cancelled { .. })));
        let uncertain = LocalProbeCleanupScope::new();
        uncertain
            .scope(async {
                drop(local_cleanup_scope::register_current().unwrap().unwrap());
            })
            .await;
        assert_eq!(
            uncertain.cleanup_status(),
            Err(LocalProbeCleanupError::CleanupUnconfirmed)
        );
        let result = uncertain
            .scope(extract(
                command("printf spawned >\"$1\"", std::slice::from_ref(&marker)),
                PathBuf::from("synthetic.ts"),
            ))
            .await;
        assert!(matches!(
            result,
            Err(FfprobeError::CleanupUnconfirmed { .. })
        ));
        assert!(!marker.exists());
    }

    #[tokio::test]
    async fn confirmed_probe_errors_allow_later_child_scans_in_the_same_scope() {
        let scope = LocalProbeCleanupScope::new();
        let result = scope
            .scope(extract(
                command("exit 7", &[]),
                PathBuf::from("synthetic.ts"),
            ))
            .await;
        assert!(matches!(result, Err(FfprobeError::Process { .. })));
        assert_eq!(scope.wait_cleanup().await, Ok(()));
        let result = scope
            .scope(extract(
                command("printf '{'", &[]),
                PathBuf::from("synthetic.ts"),
            ))
            .await;
        assert!(matches!(result, Err(FfprobeError::Json { .. })));
        assert_eq!(scope.wait_cleanup().await, Ok(()));
        let result = scope
            .scope(extract(
                command(
                    "printf '{\"streams\":[],\"format\":{\"duration\":\"1.0\"}}'",
                    &[],
                ),
                PathBuf::from("synthetic.ts"),
            ))
            .await;
        assert!(result.is_ok());
        assert_eq!(scope.cleanup_status(), Ok(true));
    }

    #[tokio::test]
    async fn a_closed_response_receiver_prevents_process_spawn() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("must-not-exist");
        let (mut response, receiver) = oneshot::channel();
        drop(receiver);
        let result = run_owned(
            command("printf spawned >\"$1\"", std::slice::from_ref(&marker)),
            "synthetic.ts",
            &mut response,
        )
        .await;
        assert!(matches!(result, Err(FfprobeError::Cancelled { .. })));
        assert!(!marker.exists());
    }

    #[tokio::test]
    async fn nonzero_exit_retains_only_the_stderr_tail_and_malformed_json_stays_a_json_error() {
        let result = extract(
            command(
                "head -c 131072 /dev/zero | tr '\\000' x >&2; printf tail >&2; exit 7",
                &[],
            ),
            PathBuf::from("synthetic.ts"),
        )
        .await;
        let Err(FfprobeError::Process { status, stderr, .. }) = result else {
            panic!("nonzero probe must remain a process error");
        };
        assert_eq!(status.code(), Some(7));
        assert_eq!(stderr.len(), MAX_LOCAL_STDERR_TAIL_BYTES);
        assert!(stderr.ends_with("tail"));
        assert!(matches!(
            extract(command("printf '{'", &[]), PathBuf::from("synthetic.ts")).await,
            Err(FfprobeError::Json { .. }),
        ));
    }

    #[tokio::test]
    async fn valid_metadata_succeeds_with_noisy_stderr_and_early_stdout_eof() {
        let result = tokio::time::timeout(
            Duration::from_secs(8),
            extract(
                command("head -c 131072 /dev/zero >&2; printf '{\"streams\":[],\"format\":{\"duration\":\"1.0\"}}'; exec 1>&-; sleep 0.1", &[]),
                PathBuf::from("synthetic.ts"),
            ),
        )
        .await
        .expect("concurrent pipe draining must not deadlock")
        .unwrap();
        assert_eq!(result.version.duration, chrono::Duration::seconds(1));
    }

    // Linux subreaper state is process-global, so this regression runs in a
    // separate test process. Other Unix platforms lack this Linux-only API.
    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[expect(
        clippy::disallowed_methods,
        reason = "Test-only environment flag isolates Linux subreaper state from concurrent tests"
    )]
    async fn abandoned_local_probe_confirms_descendant_cleanup_in_an_isolated_subreaper() {
        const HELPER: &str = "ERSATZRS_LOCAL_PROBE_SUBREAPER_TEST";
        if std::env::var_os(HELPER).is_none() {
            let mut helper = Command::new(std::env::current_exe().unwrap());
            helper
                .args([
                    "--exact",
                    "ffprobe::local::tests::unix::abandoned_local_probe_confirms_descendant_cleanup_in_an_isolated_subreaper",
                    "--nocapture",
                ])
                .env(HELPER, "1")
                .kill_on_drop(true);
            let output = tokio::time::timeout(Duration::from_secs(15), helper.output())
                .await
                .expect("isolated cleanup regression timed out")
                .unwrap();
            assert!(
                output.status.success(),
                "isolated descendant cleanup regression failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        nix::sys::prctl::set_child_subreaper(true).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let leader_marker = directory.path().join("leader-pid");
        let descendant_marker = directory.path().join("descendant-pid");
        let arguments = vec![leader_marker.clone(), descendant_marker.clone()];
        let scope = LocalProbeCleanupScope::new();
        let caller_scope = scope.clone();
        let caller =
            tokio::spawn(async move {
                caller_scope.scope(extract(
                command(
                    "sleep 30 & printf '%s' \"$!\" >\"$2\"; printf '%s' \"$$\" >\"$1\"; wait",
                    &arguments,
                ),
                PathBuf::from("synthetic.ts"),
            )).await
            });
        let leader = wait_for_pid_file(&leader_marker).await;
        let descendant = wait_for_pid_file(&descendant_marker).await;
        assert!(nix::sys::signal::kill(descendant, None).is_ok());
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        scope.close();
        assert_eq!(scope.wait_cleanup().await, Ok(()));
        wait_for_exit(leader).await;
        wait_for_exit(descendant).await;
        assert_eq!(
            nix::sys::signal::killpg(leader, None),
            Err(nix::errno::Errno::ESRCH)
        );
    }
}
