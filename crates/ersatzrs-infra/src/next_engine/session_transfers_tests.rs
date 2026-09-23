use super::*;
#[cfg(unix)]
use std::collections::BTreeMap;
use std::time::Duration;
#[cfg(unix)]
use tokio::io::AsyncReadExt as _;

fn binding() -> TransferBinding {
    TransferBinding {
        media_item_id: 1,
        playout_item_id: 2,
        media_version_id: 3,
        media_file_id: 4,
        publication: "synthetic-publication".into(),
        start: Utc::now(),
        finish: Utc::now() + chrono::Duration::seconds(30),
    }
}

#[cfg(unix)]
fn probe() -> FfprobeMediaVersion {
    FfprobeMediaVersion {
        version: ersatzrs_domain::MediaVersion::default(),
        streams: Vec::new(),
        field_orders: BTreeMap::new(),
        chapters: Vec::new(),
        format_name: None,
        format_metadata: BTreeMap::new(),
    }
}

#[tokio::test]
async fn shutdown_awaits_pending_preparation_cleanup_and_closes_admission() {
    let transfers = Arc::new(SessionTransfers::default());
    let (started, started_rx) = oneshot::channel();
    let (cleaned, cleaned_rx) = oneshot::channel();
    let (release, release_rx) = oneshot::channel();
    let preparing = transfers.clone();
    let prepare_task = tokio::spawn(async move {
        preparing
            .prepare(
                binding(),
                Instant::now() + Duration::from_secs(15),
                Instant::now() + Duration::from_secs(20),
                |cancellation| async move {
                    started.send(()).unwrap();
                    let _ = cancellation.await;
                    cleaned.send(()).unwrap();
                    release_rx.await.unwrap();
                    Err(RemoteStreamError::PreparedCancelled)
                },
            )
            .await
    });
    started_rx.await.unwrap();
    let stopping = transfers.clone();
    let shutdown = tokio::spawn(async move { stopping.shutdown().await });
    cleaned_rx.await.unwrap();
    assert!(
        !shutdown.is_finished(),
        "shutdown must await cleanup acknowledgment"
    );
    assert!(transfers
        .prepare(
            binding(),
            Instant::now() + Duration::from_secs(15),
            Instant::now() + Duration::from_secs(20),
            |_| async { panic!("closed admission invoked preparation") }
        )
        .await
        .is_err());
    // Cancelling an observer must not detach registered cleanup ownership.
    shutdown.abort();
    assert!(shutdown.await.unwrap_err().is_cancelled());
    release.send(()).unwrap();
    transfers.shutdown().await.unwrap();
    assert!(prepare_task.await.unwrap().is_err());
}

#[tokio::test]
async fn capacity_is_bounded_before_preparation_is_invoked() {
    let transfers = Arc::new(SessionTransfers::default());
    let mut tasks = Vec::new();
    for _ in 0..MAX_TRANSFERS {
        let (started, started_rx) = oneshot::channel();
        let registry = transfers.clone();
        tasks.push(tokio::spawn(async move {
            registry
                .prepare(
                    binding(),
                    Instant::now() + Duration::from_secs(15),
                    Instant::now() + Duration::from_secs(20),
                    |cancellation| async move {
                        started.send(()).unwrap();
                        let _ = cancellation.await;
                        Err(RemoteStreamError::PreparedCancelled)
                    },
                )
                .await
        }));
        started_rx.await.unwrap();
    }
    assert!(transfers
        .prepare(
            binding(),
            Instant::now() + Duration::from_secs(15),
            Instant::now() + Duration::from_secs(20),
            |_| async { panic!("full registry invoked preparation") }
        )
        .await
        .is_err());
    transfers.shutdown().await.unwrap();
    for task in tasks {
        assert!(task.await.unwrap().is_err());
    }
}

// Process fixtures require the Unix shell; ownership/admission tests above are portable.
#[cfg(unix)]
async fn prepare_shell(
    transfers: &SessionTransfers,
    script: &str,
    expiry: Instant,
) -> ReadyTransfer {
    let command = crate::remote_stream::RemoteStreamCommand {
        tool_paths: Vec::new(),
        program: "sh".into(),
        args: vec!["-c".into(), script.into()],
        working_directory: ".".into(),
        environment: BTreeMap::new(),
    };
    transfers
        .prepare(
            binding(),
            Instant::now() + Duration::from_secs(15),
            expiry,
            move |cancellation| async move {
                let input = PreparedRemoteStreamInput::start_without_request(
                    &command,
                    crate::remote_stream::RemoteStreamSpawnContext::probe(),
                    Instant::now() + Duration::from_secs(15),
                    cancellation,
                )
                .await?;
                Ok(PreparedTransferMedia {
                    input,
                    probe: probe(),
                })
            },
        )
        .await
        .unwrap()
}

#[cfg(unix)]
#[tokio::test]
async fn confirmed_failures_do_not_exhaust_later_programme_admission() {
    let transfers = SessionTransfers::default();
    for _ in 0..MAX_TRANSFERS + 2 {
        let result = transfers
            .prepare(
                binding(),
                Instant::now() + Duration::from_secs(15),
                Instant::now() + Duration::from_secs(20),
                |_| async { Err(RemoteStreamError::PreparedProducerFailed) },
            )
            .await;
        assert!(matches!(
            result,
            Err(RemoteStreamError::PreparedProducerFailed)
        ));
        // Await the registered owner, not just delivery of its error response.
        let completions: Vec<_> = transfers
            .state
            .lock()
            .unwrap()
            .entries
            .values()
            .map(|entry| entry.completion.clone())
            .collect();
        for completion in completions {
            assert_eq!(
                completion.await,
                Err(RemoteStreamError::PreparedProducerFailed)
            );
        }
    }
    let ready = prepare_shell(
        &transfers,
        "printf recovered",
        Instant::now() + Duration::from_secs(20),
    )
    .await;
    let mut bytes = Vec::new();
    transfers
        .claim(ready.id)
        .unwrap()
        .read_to_end(&mut bytes)
        .await
        .unwrap();
    assert_eq!(bytes, b"recovered");
    transfers.shutdown().await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn reader_preserves_original_bytes_and_rejects_second_claim() {
    let transfers = SessionTransfers::default();
    let ready = prepare_shell(
        &transfers,
        "cat; head -c 70000 /dev/zero; printf tail",
        Instant::now() + Duration::from_secs(20),
    )
    .await;
    let mut reader = transfers.claim(ready.id).unwrap();
    assert!(transfers.claim(ready.id).is_err());
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).await.unwrap();
    assert_eq!(bytes.len(), 70004);
    assert!(bytes[..70000].iter().all(|byte| *byte == 0));
    assert_eq!(&bytes[70000..], b"tail");
    transfers.shutdown().await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn terminal_producer_failure_is_an_http_reader_error_not_clean_eof() {
    let transfers = SessionTransfers::default();
    let ready = prepare_shell(
        &transfers,
        "head -c 70000 /dev/zero; exit 7",
        Instant::now() + Duration::from_secs(20),
    )
    .await;
    let mut reader = transfers.claim(ready.id).unwrap();
    let mut bytes = Vec::new();
    assert!(reader.read_to_end(&mut bytes).await.is_err());
    assert_eq!(bytes.len(), 70000);
    assert!(transfers.shutdown().await.is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn unopened_expiry_body_drop_and_explicit_cancel_await_cleanup() {
    for mode in ["expire", "body", "cancel", "callback", "shutdown"] {
        let transfers = SessionTransfers::default();
        let expiry = Instant::now()
            + if mode == "expire" {
                Duration::from_millis(500)
            } else {
                Duration::from_secs(20)
            };
        let ready = prepare_shell(
            &transfers,
            "head -c 70000 /dev/zero; while :; do :; done",
            expiry,
        )
        .await;
        let completion = transfers.state.lock().unwrap().entries[&ready.id]
            .completion
            .clone();
        match mode {
            "expire" => tokio::time::sleep_until(expiry).await,
            "body" => {
                let mut reader = transfers.claim(ready.id).unwrap();
                // Observe bytes first, proving cancellation covers an active
                // transfer rather than only an unclaimed prepared producer.
                reader.read_exact(&mut [0_u8; 1]).await.unwrap();
                drop(reader);
            }
            "cancel" => {
                transfers.cancel(ready.id).await.unwrap();
            }
            "callback" => transfers.request_cancel(ready.id),
            _ => transfers.shutdown().await.unwrap(),
        }
        tokio::time::timeout(Duration::from_secs(3), completion)
            .await
            .unwrap()
            .unwrap();
        assert!(transfers.claim(ready.id).is_err());
        transfers.shutdown().await.unwrap();
    }
}

#[cfg(unix)]
#[tokio::test]
async fn shutdown_reaps_a_real_producer_still_preparing_its_prefix() {
    let root = tempfile::tempdir().unwrap();
    let pid_file = root.path().join("producer.pid");
    let registry = Arc::new(SessionTransfers::default());
    let command = crate::remote_stream::RemoteStreamCommand {
        tool_paths: Vec::new(),
        program: "sh".into(),
        args: vec![
            "-c".into(),
            "printf '%s' $$ > \"$FIXTURE_PID_PATH\"; printf short; while :; do :; done".into(),
        ],
        working_directory: ".".into(),
        environment: BTreeMap::from([(
            "FIXTURE_PID_PATH".into(),
            pid_file.to_string_lossy().into_owned(),
        )]),
    };
    let preparing = registry.clone();
    let task = tokio::spawn(async move {
        preparing
            .prepare(
                binding(),
                Instant::now() + Duration::from_secs(15),
                Instant::now() + Duration::from_secs(20),
                move |cancellation| async move {
                    let input = PreparedRemoteStreamInput::start_without_request(
                        &command,
                        crate::remote_stream::RemoteStreamSpawnContext::probe(),
                        Instant::now() + Duration::from_secs(15),
                        cancellation,
                    )
                    .await?;
                    Ok(PreparedTransferMedia {
                        input,
                        probe: probe(),
                    })
                },
            )
            .await
    });
    tokio::time::timeout(Duration::from_secs(3), async {
        while !pid_file.exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let pid: i32 = std::fs::read_to_string(&pid_file).unwrap().parse().unwrap();
    tokio::time::timeout(Duration::from_secs(3), registry.shutdown())
        .await
        .unwrap()
        .unwrap();
    assert!(task.await.unwrap().is_err());
    assert_eq!(
        nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None),
        Err(nix::errno::Errno::ESRCH)
    );
}

#[tokio::test]
async fn abandoned_callback_cancels_registered_preparation() {
    let transfers = Arc::new(SessionTransfers::default());
    let (started, started_rx) = oneshot::channel();
    let (cancelled_tx, cancelled_rx) = oneshot::channel();
    let registry = transfers.clone();
    let callback = tokio::spawn(async move {
        registry
            .prepare(
                binding(),
                Instant::now() + Duration::from_secs(15),
                Instant::now() + Duration::from_secs(20),
                |cancellation| async move {
                    started.send(()).unwrap();
                    let _ = cancellation.await;
                    cancelled_tx.send(()).unwrap();
                    Err(RemoteStreamError::PreparedCancelled)
                },
            )
            .await
    });
    started_rx.await.unwrap();
    callback.abort();
    assert!(matches!(callback.await, Err(error) if error.is_cancelled()));
    tokio::time::timeout(Duration::from_secs(2), cancelled_rx)
        .await
        .unwrap()
        .unwrap();
    transfers.shutdown().await.unwrap();
}

#[tokio::test]
async fn unconfirmed_cleanup_survives_cancel_pruning_and_repeated_shutdown() {
    let transfers = SessionTransfers::default();
    let mut ids = Vec::new();
    for error in [
        RemoteStreamError::PreparedProducerFailed,
        RemoteStreamError::PreparedTerminationUnconfirmed,
    ] {
        let id = Uuid::new_v4();
        let (cancel, _) = watch::channel(false);
        transfers.state.lock().unwrap().entries.insert(
            id,
            Entry {
                _binding: binding(),
                deadline: Instant::now() + Duration::from_secs(15),
                cancel,
                claimed: None,
                reader: None,
                completion: async move { Err(error) }.boxed().shared(),
            },
        );
        ids.push(id);
    }
    assert_eq!(
        transfers.cancel(ids[1]).await,
        Err(RemoteStreamError::PreparedTerminationUnconfirmed)
    );
    assert!(transfers
        .state
        .lock()
        .unwrap()
        .entries
        .contains_key(&ids[1]));
    // A new admission executes pruning; the failed cleanup record must survive.
    assert!(transfers
        .prepare(
            binding(),
            Instant::now() + Duration::from_secs(15),
            Instant::now() + Duration::from_secs(20),
            |_| async { Err(RemoteStreamError::PreparedProducerFailed) }
        )
        .await
        .is_err());
    for _ in 0..2 {
        assert_eq!(
            transfers.shutdown().await,
            Err(RemoteStreamError::PreparedTerminationUnconfirmed)
        );
    }
    assert!(transfers
        .state
        .lock()
        .unwrap()
        .entries
        .contains_key(&ids[1]));
}
