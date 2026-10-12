use super::*;

fn request_bytes() -> Vec<u8> {
    br#"{"schema_version":1,"generation":"fixture","locale":"nl-NL","timezone":"Europe/Amsterdam","captured_at":"2026-10-25T00:30:00Z","window":{"start":"2026-10-24T22:00:00Z","finish":"2026-10-27T23:00:00Z"},"channels":[]}"#.to_vec()
}

#[tokio::test]
async fn bounded_reader_accepts_exact_bytes_without_retaining_overflow_sentinel() {
    for length in [0, 5, MAX_STDERR_BYTES] {
        let bytes = vec![b'x'; length];
        assert_eq!(read_bounded(bytes.as_slice(), length).await.unwrap(), bytes);
    }
    let bytes = vec![b'x'; MAX_STDERR_BYTES + 1];
    assert!(matches!(
        read_bounded(bytes.as_slice(), MAX_STDERR_BYTES).await,
        Err(PipeFailure::Overflow)
    ));
}

#[tokio::test]
async fn bounded_reader_maps_io_failure_without_raw_diagnostics() {
    struct BrokenReader;
    impl AsyncRead for BrokenReader {
        fn poll_read(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
            _: &mut tokio::io::ReadBuf<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::task::Poll::Ready(Err(std::io::Error::other("private producer content")))
        }
    }
    assert!(matches!(
        read_bounded(BrokenReader, MAX_STDOUT_BYTES).await,
        Err(PipeFailure::Io)
    ));
    assert!(!TeletextCommandError::IoFailed
        .to_string()
        .contains("private producer content"));
}

#[test]
fn command_owner_unwind_retains_admission_until_restart() {
    use ersatzrs_core::locking::{EntityLocker, TeletextWorkGuard};

    let locker = EntityLocker::new();
    let guard = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let hold = guard.hold_work();
    drop(guard);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _ownership = CommandWorkHold {
            hold: Some(hold),
            confirmed: false,
        };
        panic!("synthetic command owner unwind before exit confirmation");
    }));
    assert!(outcome.is_err());
    assert_eq!(locker.active_teletext_work(), 1);
    assert!(TeletextWorkGuard::try_acquire(&locker, 1).is_none());
}

// Synthetic shell fixtures exercise the managed Unix owner; native Windows
// JobObject acceptance is separate and is not inferred from these tests.
#[cfg(unix)]
mod unix {
    use std::{collections::BTreeMap, path::Path};

    use super::*;

    fn command(root: &Path, script: &str) -> ResolvedAddonCommand {
        ResolvedAddonCommand {
            addon_id: "org.ersatzrs.teletext.engine-fixture".to_owned(),
            program: "/bin/sh".to_owned(),
            args: vec!["-c".to_owned(), script.to_owned(), "fixture".to_owned()],
            working_directory: root.to_path_buf(),
            environment: BTreeMap::new(),
            state_revision: "fixture".to_owned(),
            tool_paths: Vec::new(),
        }
    }

    async fn wait_ready(path: &Path) {
        tokio::time::timeout(Duration::from_secs(3), async {
            while !tokio::fs::read_to_string(path)
                .await
                .is_ok_and(|ready| ready.ends_with('\n'))
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("synthetic command became ready");
    }

    #[tokio::test]
    async fn held_command_requester_abort_releases_only_after_confirmed_process_cleanup() {
        use ersatzrs_core::locking::{EntityLocker, TeletextWorkGuard};

        let temp = tempfile::tempdir().unwrap();
        let ready = temp.path().join("ready");
        let supplied = command(temp.path(), "printf '%s\\n' \"$$\" > ready; exec sleep 30");
        let locker = EntityLocker::new();
        let guard = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
        let hold = guard.hold_work();
        drop(guard);
        let requester = tokio::spawn(run_with_contract_deadline_and_hold(
            supplied,
            request_bytes(),
            None,
            Instant::now() + EXECUTION_TIMEOUT,
            TeletextPageContract::V1,
            Some(hold),
        ));
        wait_ready(&ready).await;
        let pid: i32 = tokio::fs::read_to_string(&ready)
            .await
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert!(TeletextWorkGuard::try_acquire(&locker, 1).is_none());
        requester.abort();
        assert!(requester.await.unwrap_err().is_cancelled());
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Some(guard) = TeletextWorkGuard::try_acquire(&locker, 1) {
                    drop(guard);
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            nix::sys::signal::killpg(nix::unistd::Pid::from_raw(pid), None),
            Err(nix::errno::Errno::ESRCH),
        );
        assert_eq!(locker.active_teletext_work(), 0);
    }

    #[tokio::test]
    async fn finite_request_and_raw_output_complete_without_capability_activation() {
        let temp = tempfile::tempdir().unwrap();
        let input = request_bytes();
        let mut supplied = command(
            temp.path(),
            "test \"$ERSATZRS_ADDON_CAPABILITY\" = teletext.pages.v1 || exit 9; exec cat",
        );
        supplied
            .environment
            .insert("ERSATZRS_ADDON_CAPABILITY".to_owned(), "wrong".to_owned());
        assert_eq!(run(supplied, input.clone(), None).await.unwrap(), input);
    }

    #[tokio::test]
    async fn selected_v2_command_enforces_schema_and_replaces_supplied_capability() {
        let temp = tempfile::tempdir().unwrap();
        let mut request = TeletextPagesRequest::from_json(&request_bytes()).unwrap();
        request.schema_version = 2;
        let input = serde_json::to_vec(&request).unwrap();
        let mut supplied = command(
            temp.path(),
            "test \"$ERSATZRS_ADDON_CAPABILITY\" = teletext.pages.v2 || exit 9; exec cat",
        );
        supplied.environment.insert(
            "ERSATZRS_ADDON_CAPABILITY".to_owned(),
            "teletext.pages.v1".to_owned(),
        );
        assert_eq!(
            run_with_contract_deadline(
                supplied,
                input.clone(),
                None,
                Instant::now() + EXECUTION_TIMEOUT,
                TeletextPageContract::V2,
            )
            .await
            .unwrap(),
            input
        );
    }

    #[tokio::test]
    async fn selected_schema_mismatch_rejects_before_spawn_in_both_directions() {
        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join("not-started");
        let mut supplied = command(temp.path(), "printf started > \"$1\"");
        supplied.args.push(marker.to_string_lossy().into_owned());
        for contract in [TeletextPageContract::V1, TeletextPageContract::V2] {
            let mut request = TeletextPagesRequest::from_json(&request_bytes()).unwrap();
            request.schema_version = if contract == TeletextPageContract::V1 {
                2
            } else {
                1
            };
            assert_eq!(
                run_with_contract_deadline(
                    supplied.clone(),
                    serde_json::to_vec(&request).unwrap(),
                    None,
                    Instant::now() + EXECUTION_TIMEOUT,
                    contract,
                )
                .await,
                Err(TeletextCommandError::InvalidRequest)
            );
            assert!(!marker.exists());
        }
    }

    #[tokio::test]
    async fn invalid_or_oversize_input_is_rejected_before_spawn() {
        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join("not-started");
        let mut supplied = command(temp.path(), "printf started > \"$1\"");
        supplied.args.push(marker.to_string_lossy().into_owned());
        for invalid in [
            vec![b' '; MAX_TELETEXT_PAGES_INPUT_BYTES + 1],
            b"{}".to_vec(),
            vec![0xff],
        ] {
            assert_eq!(
                run(supplied.clone(), invalid, None).await,
                Err(TeletextCommandError::InvalidRequest)
            );
            assert!(!marker.exists());
        }
    }

    #[tokio::test]
    async fn preexisting_deadline_or_cancellation_prevents_spawn() {
        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join("not-started");
        let mut supplied = command(temp.path(), "printf started > \"$1\"");
        supplied.args.push(marker.to_string_lossy().into_owned());
        assert_eq!(
            run_with_deadline(supplied.clone(), request_bytes(), None, Instant::now()).await,
            Err(TeletextCommandError::TimedOut)
        );
        let (cancel, cancelled) = oneshot::channel();
        drop(cancel);
        assert_eq!(
            run(supplied, request_bytes(), Some(cancelled)).await,
            Err(TeletextCommandError::Cancelled)
        );
        assert!(!marker.exists());
    }

    #[tokio::test]
    async fn stdout_and_stderr_exact_limits_are_inclusive() {
        let temp = tempfile::tempdir().unwrap();
        let output = run(
            command(temp.path(), "exec head -c 8388608 /dev/zero"),
            request_bytes(),
            None,
        )
        .await
        .unwrap();
        assert_eq!(output, vec![0; MAX_STDOUT_BYTES]);
        let input = request_bytes();
        let output = run(
            command(temp.path(), "printf '%16384s' x >&2; exec cat"),
            input.clone(),
            None,
        )
        .await
        .unwrap();
        assert_eq!(output, input);
    }

    #[tokio::test]
    async fn overflow_stops_non_exiting_stdout_or_stderr_producer_early() {
        let temp = tempfile::tempdir().unwrap();
        for (script, expected) in [
            (
                "while :; do printf '%8192s' x; done",
                TeletextCommandError::OutputTooLarge,
            ),
            (
                "while :; do printf '%8192s' x >&2; done",
                TeletextCommandError::StderrTooLarge,
            ),
        ] {
            let started = Instant::now();
            let result = run_with_deadline(
                command(temp.path(), script),
                request_bytes(),
                None,
                started + Duration::from_secs(10),
            )
            .await;
            assert_eq!(result, Err(expected));
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "overflow waited for the execution deadline"
            );
        }
    }

    #[tokio::test]
    async fn stdin_failure_stops_live_process_and_returns_static_error() {
        let temp = tempfile::tempdir().unwrap();
        let mut input = request_bytes();
        input.resize(1024 * 1024, b' ');
        let started = Instant::now();
        assert_eq!(
            run_with_deadline(
                command(temp.path(), "exec 0<&-; exec sleep 30"),
                input,
                None,
                started + Duration::from_secs(10)
            )
            .await,
            Err(TeletextCommandError::IoFailed)
        );
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn unsuccessful_exit_never_returns_raw_stderr() {
        let temp = tempfile::tempdir().unwrap();
        let result = run(
            command(
                temp.path(),
                "cat >/dev/null; printf 'private producer path and guide' >&2; exit 7",
            ),
            request_bytes(),
            None,
        )
        .await;
        assert_eq!(result, Err(TeletextCommandError::Failed));
        let error = result.unwrap_err();
        assert!(!error.to_string().contains("private producer"));
        assert!(!format!("{error:?}").contains("private producer"));
    }

    #[tokio::test]
    async fn cancellation_and_timeout_reap_leader_before_returning() {
        let temp = tempfile::tempdir().unwrap();
        for timeout in [false, true] {
            let ready = temp.path().join(if timeout {
                "timeout-ready"
            } else {
                "cancel-ready"
            });
            let mut supplied =
                command(temp.path(), "printf '%s\\n' \"$$\" > \"$1\"; exec sleep 30");
            supplied.args.push(ready.to_string_lossy().into_owned());
            let (cancel, cancelled) = oneshot::channel();
            let deadline = Instant::now()
                + if timeout {
                    Duration::from_millis(300)
                } else {
                    Duration::from_secs(10)
                };
            let invocation = tokio::spawn(run_with_deadline(
                supplied,
                request_bytes(),
                Some(cancelled),
                deadline,
            ));
            wait_ready(&ready).await;
            let pid: i32 = tokio::fs::read_to_string(&ready)
                .await
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            if !timeout {
                cancel.send(()).unwrap();
            }
            let result = invocation.await.unwrap();
            assert_eq!(
                result,
                Err(if timeout {
                    TeletextCommandError::TimedOut
                } else {
                    TeletextCommandError::Cancelled
                })
            );
            assert_eq!(
                nix::sys::signal::killpg(nix::unistd::Pid::from_raw(pid), None),
                Err(nix::errno::Errno::ESRCH)
            );
        }
    }

    #[tokio::test]
    async fn cleanup_aborts_and_joins_every_pending_pipe_handle() {
        let temp = tempfile::tempdir().unwrap();
        let supplied = command(temp.path(), "exec sleep 30");
        let mut child = tool_execution::spawn_bounded_addon_process(
            &supplied,
            true,
            crate::addon_tool_gate::ToolLeaseMode::Use,
        )
        .unwrap();
        let pending = || {
            Some(tokio::spawn(std::future::pending::<
                Result<Vec<u8>, PipeFailure>,
            >()))
        };
        let mut tasks = PipeTasks {
            stdin: pending(),
            stdout: pending(),
            stderr: pending(),
        };
        assert_eq!(
            cleaned_result(
                &mut child,
                &mut tasks,
                false,
                Err(TeletextCommandError::Cancelled)
            )
            .await,
            Err(TeletextCommandError::Cancelled)
        );
        assert!(tasks.stdin.is_none() && tasks.stdout.is_none() && tasks.stderr.is_none());
    }

    // Isolate Linux's process-wide subreaper setting from other parallel tests.
    // Other Unix platforms keep the simpler leader/pipe tests above; adopted
    // descendant and native Windows acceptance remain distinct platform proof.
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn inherited_pipes_cancelled_and_abandoned_callers_confirm_descendant_cleanup() {
        const HELPER: &str = "ERSATZRS_TEST_TELETEXT_COMMAND_SUBREAPER";
        if crate::startup_config::environment_variable(HELPER).is_none() {
            let test_name = concat!(
                module_path!(),
                "::inherited_pipes_cancelled_and_abandoned_callers_confirm_descendant_cleanup"
            );
            let name = test_name.strip_prefix("ersatzrs_infra::").unwrap();
            let mut process = tokio::process::Command::new(std::env::current_exe().unwrap());
            process
                .args(["--exact", name, "--nocapture"])
                .env(HELPER, "1")
                .kill_on_drop(true);
            let output = tokio::time::timeout(Duration::from_secs(20), process.output())
                .await
                .unwrap()
                .unwrap();
            assert!(
                output.status.success(),
                "isolated cleanup fixture failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        nix::sys::prctl::set_child_subreaper(true).unwrap();
        for mode in ["normal", "cancel", "sender-drop", "timeout", "abandon"] {
            descendant_scenario(mode).await;
        }
    }

    #[cfg(target_os = "linux")]
    async fn descendant_scenario(mode: &str) {
        let temp = tempfile::tempdir().unwrap();
        let ready = temp.path().join("ready");
        let marker = temp.path().join("descendant-marker");
        let script = if mode == "normal" {
            "cat >/dev/null; (sleep 30; printf orphan > \"$2\") & child=$!; printf '%s %s\\n' \"$$\" \"$child\" > \"$1\"; exit 0"
        } else {
            "(sleep 30; printf orphan > \"$2\") & child=$!; printf '%s %s\\n' \"$$\" \"$child\" > \"$1\"; wait"
        };
        let mut supplied = command(temp.path(), script);
        supplied.args.extend([
            ready.to_string_lossy().into_owned(),
            marker.to_string_lossy().into_owned(),
        ]);
        let (cancel, cancelled) = oneshot::channel();
        let invocation = tokio::spawn(run_with_deadline(
            supplied,
            request_bytes(),
            Some(cancelled),
            Instant::now()
                + if mode == "timeout" {
                    Duration::from_secs(1)
                } else {
                    Duration::from_secs(10)
                },
        ));
        wait_ready(&ready).await;
        let ready_pids = tokio::fs::read_to_string(&ready).await.unwrap();
        let pids: Vec<i32> = ready_pids
            .split_whitespace()
            .map(|pid| pid.parse().unwrap())
            .collect();
        assert_eq!(pids.len(), 2);
        let (pid, descendant) = (pids[0], pids[1]);
        assert!(descendant > 0 && descendant != pid);
        if mode != "normal" {
            assert!(
                nix::sys::signal::kill(nix::unistd::Pid::from_raw(descendant), None).is_ok(),
                "descendant exists before cancellation"
            );
        }
        match mode {
            "normal" => {
                assert_eq!(invocation.await.unwrap(), Ok(Vec::new()));
            }
            "cancel" => {
                cancel.send(()).unwrap();
                assert_eq!(
                    invocation.await.unwrap(),
                    Err(TeletextCommandError::Cancelled)
                );
            }
            "sender-drop" => {
                drop(cancel);
                assert_eq!(
                    invocation.await.unwrap(),
                    Err(TeletextCommandError::Cancelled)
                );
            }
            "timeout" => {
                assert_eq!(
                    invocation.await.unwrap(),
                    Err(TeletextCommandError::TimedOut)
                );
            }
            _ => {
                invocation.abort();
                assert!(invocation.await.unwrap_err().is_cancelled());
            }
        }
        tokio::time::timeout(Duration::from_secs(8), async {
            while nix::sys::signal::killpg(nix::unistd::Pid::from_raw(pid), None)
                != Err(nix::errno::Errno::ESRCH)
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("owned worker confirmed group disappearance");
        assert!(!marker.exists());
    }
}
