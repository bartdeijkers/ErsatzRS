//! Unix shell fixtures exercise owned process supervision without `FFmpeg`.
//! Windows native process and packaged playback acceptance remain separate gates.

use std::{path::Path, sync::atomic::AtomicBool};

use futures_util::StreamExt as _;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

use super::super::{
    teletext_binding::{TeletextOutputAdmission, TeletextOutputBinding, TeletextOutputRole},
    teletext_output::{
        tests::{authored_set, fixture, flatten, policy},
        TeletextPumpError,
    },
};
use super::*;

fn admission() -> TeletextOutputAdmission {
    let binding =
        TeletextOutputBinding::new(policy(), authored_set(), Duration::from_millis(40)).unwrap();
    TeletextOutputAdmission::with_binding(TeletextOutputRole::FinalDirectMpegTs, Arc::new(binding))
        .unwrap()
}

pub(super) fn output_child(directory: &Path, bytes: &[u8], script: &str) -> RemoteStreamProcess {
    let payload = directory.join("synthetic-output.ts");
    std::fs::write(&payload, bytes).unwrap();
    let marker = directory.join("source-drained");
    let command = FfmpegCommand::new_with_executable(
        "sh",
        Vec::new(),
        vec![
            "-c".into(),
            script.into(),
            "synthetic-output".into(),
            payload.to_string_lossy().into_owned(),
            marker.to_string_lossy().into_owned(),
        ],
    );
    spawn_stream_process(&command, None, "synthetic-teletext").unwrap()
}

async fn wait_for_drain_marker(directory: &Path) {
    let marker = directory.join("source-drained");
    tokio::time::timeout(Duration::from_millis(500), async {
        while !tokio::fs::try_exists(&marker).await.unwrap() {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("the supervised stdout reader must drain the synthetic child output");
}

fn hardware_retry(rebuild: StreamRetryBuilder) -> StreamDenialRetry {
    StreamDenialRetry {
        source_caption: None,
        ordinary_caption_fallback: None,
        song: None,
        pool: sqlx::sqlite::SqlitePoolOptions::new()
            .connect_lazy("sqlite::memory:")
            .unwrap(),
        shared_ffmpeg_runtime: Arc::new(RwLock::new(FfmpegRuntimeReport::for_tests())),
        runtime_capabilities: FfmpegCapabilityReport::default(),
        acceleration: ersatzrs_domain::HardwareAccelerationKind::Nvenc,
        source_stream: None,
        graph_operation: None,
        ffmpeg_path: "sh".into(),
        rebuild: Some(rebuild),
    }
}

#[tokio::test]
async fn owned_child_delivers_a_complete_paced_cycle_without_changing_source_clock_packets() {
    let directory = tempfile::tempdir().unwrap();
    let original = fixture(3);
    let mut child = output_child(directory.path(), &flatten(&original), "exec cat \"$1\"");
    let output = admission();
    let cancel = Arc::new(Notify::new());
    let (receiver, mut writer) = tokio::io::duplex(188);
    let (body, completion) = stream_body_for_output(receiver, Arc::clone(&cancel), &output);
    let completion = completion.expect("authored output must report its terminal result");
    let mut body = body.into_data_stream();
    let forward = async {
        let outcome = supervise_stream_process_with_retry_context(
            &mut child,
            &mut writer,
            &cancel,
            "synthetic-teletext",
            None,
            None,
            &output,
        )
        .await;
        completion.send(stream_output_result(&outcome)).unwrap();
        drop(writer);
        outcome
    };
    let receive = async {
        let mut bytes = Vec::new();
        while let Some(chunk) = body.next().await {
            bytes.extend_from_slice(&chunk.expect("complete authored output must end cleanly"));
        }
        bytes
    };
    let (outcome, delivered) = Box::pin(tokio::time::timeout(Duration::from_secs(2), async {
        tokio::join!(forward, receive)
    }))
    .await
    .unwrap();
    assert!(matches!(outcome, DirectStreamOutcome::Completed));
    assert!(child.try_wait().unwrap().unwrap().success());
    assert_eq!(delivered.len(), original.len() * 188);
    assert_eq!(&delivered[..188], original[0]);
    assert!(delivered
        .chunks_exact(188)
        .any(|packet| packet[1] & 0x1f == 2 && packet[2] == 0));
    for (before, after) in original.iter().zip(delivered.chunks_exact(188)) {
        if before[1] & 0x1f == 1 && before[2] == 44 {
            assert_eq!(before.as_slice(), after);
        }
    }
}

#[tokio::test]
async fn zero_exit_cannot_hide_malformed_transport_wrong_clock_or_partial_eof() {
    for failure in 0..3 {
        let directory = tempfile::tempdir().unwrap();
        let mut raw = fixture(3);
        let bytes = match failure {
            0 => {
                raw[0][0] = 0;
                flatten(&raw[..13])
            }
            1 => {
                raw[12][11] ^= 1;
                flatten(&raw[..13])
            }
            _ => vec![0x47],
        };
        let mut child = output_child(directory.path(), &bytes, "exec cat \"$1\"");
        // The bounded prefix fits a pipe: establish a real zero exit before
        // supervision, rather than relying on a race with failure cleanup.
        assert!(tokio::time::timeout(Duration::from_secs(2), child.wait())
            .await
            .unwrap()
            .unwrap()
            .success());
        let output = admission();
        let cancel = Arc::new(Notify::new());
        let (mut receiver, mut writer) = tokio::io::duplex(4096);
        let outcome = supervise_stream_process_with_retry_context(
            &mut child,
            &mut writer,
            &cancel,
            "synthetic-teletext",
            None,
            None,
            &output,
        )
        .await;
        assert!(matches!(
            outcome,
            DirectStreamOutcome::TransportFailed(TeletextPumpError::Gate(_))
        ));
        drop(writer);
        let mut delivered = Vec::new();
        receiver.read_to_end(&mut delivered).await.unwrap();
        assert!(delivered.is_empty());
        assert!(child.try_wait().unwrap().unwrap().success());
    }
}

#[tokio::test]
async fn internal_item_stdout_is_forwarded_unchanged_without_transport_admission() {
    let directory = tempfile::tempdir().unwrap();
    let bytes = b"synthetic internal item bytes without TS framing";
    let mut child = output_child(directory.path(), bytes, "exec cat \"$1\"");
    let output = TeletextOutputAdmission::plain(TeletextOutputRole::InternalItem);
    let cancel = Arc::new(Notify::new());
    let (receiver, mut writer) = tokio::io::duplex(4096);
    let (body, completion) = stream_body_for_output(receiver, Arc::clone(&cancel), &output);
    assert!(completion.is_none());
    let mut body = body.into_data_stream();
    let outcome = supervise_stream_process_with_retry_context(
        &mut child,
        &mut writer,
        &cancel,
        "synthetic-teletext",
        None,
        None,
        &output,
    )
    .await;
    assert!(matches!(outcome, DirectStreamOutcome::Completed));
    drop(writer);
    let mut delivered = Vec::new();
    while let Some(chunk) = body.next().await {
        delivered.extend_from_slice(&chunk.expect("internal output must retain clean EOF"));
    }
    assert_eq!(delivered, bytes);
    assert!(child.try_wait().unwrap().unwrap().success());
}

#[tokio::test]
async fn cancellation_reaps_a_child_blocked_before_its_first_output_byte() {
    let directory = tempfile::tempdir().unwrap();
    let mut child = output_child(directory.path(), &[], "exec sleep 30");
    let output = admission();
    let cancel = Arc::new(Notify::new());
    let (_receiver, mut writer) = tokio::io::duplex(188);
    let outcome = {
        let operation = supervise_stream_process_with_retry_context(
            &mut child,
            &mut writer,
            &cancel,
            "synthetic-teletext",
            None,
            None,
            &output,
        );
        tokio::pin!(operation);
        assert!(futures_util::poll!(&mut operation).is_pending());
        cancel.notify_one();
        tokio::time::timeout(Duration::from_secs(2), operation)
            .await
            .unwrap()
    };
    assert!(matches!(outcome, DirectStreamOutcome::Cancelled));
    assert!(child.try_wait().unwrap().is_some());
}

#[tokio::test]
async fn cancellation_reaps_a_child_after_source_drain_while_output_waits_for_pacing() {
    let directory = tempfile::tempdir().unwrap();
    let original = fixture(1);
    let mut child = output_child(
        directory.path(),
        &flatten(&original),
        "cat \"$1\"; printf drained > \"$2\"; exec sleep 30",
    );
    let mut config = policy();
    config.playout_delay = Duration::from_secs(1);
    config.max_hold = Duration::from_secs(3);
    let output = TeletextOutputAdmission::with_binding(
        TeletextOutputRole::FinalDirectMpegTs,
        Arc::new(
            TeletextOutputBinding::new(config, authored_set(), Duration::from_millis(40)).unwrap(),
        ),
    )
    .unwrap();
    let cancel = Arc::new(Notify::new());
    let (mut receiver, mut writer) = tokio::io::duplex(188);
    let outcome = {
        let operation = supervise_stream_process_with_retry_context(
            &mut child,
            &mut writer,
            &cancel,
            "synthetic-teletext",
            None,
            None,
            &output,
        );
        tokio::pin!(operation);
        tokio::select! {
            () = wait_for_drain_marker(directory.path()) => (),
            _ = &mut operation => panic!("the child must remain supervised during pacing"),
        }
        cancel.notify_one();
        tokio::time::timeout(Duration::from_secs(2), operation)
            .await
            .unwrap()
    };
    assert!(matches!(outcome, DirectStreamOutcome::Cancelled));
    assert!(child.try_wait().unwrap().is_some());
    drop(writer);
    let mut delivered = Vec::new();
    receiver.read_to_end(&mut delivered).await.unwrap();
    assert!(delivered.is_empty());
}

#[tokio::test]
async fn response_body_drop_reaps_the_child_while_a_paced_sink_write_is_blocked() {
    let directory = tempfile::tempdir().unwrap();
    let original = fixture(1);
    let mut child = output_child(
        directory.path(),
        &flatten(&original),
        "cat \"$1\"; exec sleep 30",
    );
    let output = admission();
    let cancel = Arc::new(Notify::new());
    let (receiver, mut writer) = tokio::io::duplex(188);
    let mut body = guarded_stream_body(receiver, Arc::clone(&cancel)).into_data_stream();
    let outcome = {
        let operation = supervise_stream_process_with_retry_context(
            &mut child,
            &mut writer,
            &cancel,
            "synthetic-teletext",
            None,
            None,
            &output,
        );
        tokio::pin!(operation);
        let first = tokio::time::timeout(Duration::from_secs(2), async {
            tokio::select! {
                first = body.next() => first.unwrap().unwrap(),
                _ = &mut operation => panic!("the response sink must receive its first packet"),
            }
        })
        .await
        .unwrap();
        assert_eq!(first.as_ref(), original[0]);
        // Stop draining the one-packet duplex. Subsequent writes cannot fit;
        // allow multiple physical packet times while keeping supervision polled.
        assert!(
            tokio::time::timeout(Duration::from_millis(10), &mut operation)
                .await
                .is_err()
        );
        drop(body);
        tokio::time::timeout(Duration::from_secs(2), operation)
            .await
            .unwrap()
    };
    assert!(matches!(outcome, DirectStreamOutcome::Cancelled));
    assert!(child.try_wait().unwrap().is_some());
}

#[tokio::test]
async fn transport_failure_with_hardware_stderr_never_invokes_or_spends_retry() {
    for failure in 0..3 {
        let directory = tempfile::tempdir().unwrap();
        let mut raw = fixture(3);
        let bytes = match failure {
            0 => {
                raw[0][0] = 0;
                flatten(&raw[..13])
            }
            1 => {
                raw[12][11] ^= 1;
                flatten(&raw[..13])
            }
            _ => vec![0x47],
        };
        let mut child = output_child(
            directory.path(),
            &bytes,
            "printf 'Error while opening encoder' >&2; cat \"$1\"; exit 1",
        );
        assert!(!child.wait().await.unwrap().success());
        let rebuilt = Arc::new(AtomicBool::new(false));
        let observer = Arc::clone(&rebuilt);
        let mut retry = Some(hardware_retry(immediate_stream_retry(Box::new(
            move |_| {
                observer.store(true, Ordering::SeqCst);
                None
            },
        ))));
        let output = admission();
        let cancel = Arc::new(Notify::new());
        let (_receiver, mut writer) = tokio::io::duplex(4096);
        let outcome = supervise_stream_command_with_retry(
            child,
            &mut writer,
            &cancel,
            "synthetic-teletext",
            &mut retry,
            None,
            &output,
        )
        .await;
        assert!(matches!(
            outcome,
            DirectStreamOutcome::TransportFailed(TeletextPumpError::Gate(_))
        ));
        assert!(!rebuilt.load(Ordering::SeqCst));
        assert!(retry.as_ref().is_some_and(|retry| retry.rebuild.is_some()));
    }
}

#[tokio::test]
async fn empty_output_preserves_only_a_nonzero_classified_hardware_startup_failure() {
    for (script, hardware_failure) in [
        ("exit 0", false),
        ("printf 'Error while opening encoder' >&2; exit 1", true),
        ("printf 'synthetic unclassified failure' >&2; exit 1", false),
        ("printf 'Error while opening encoder' >&2; exit 0", false),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let mut child = output_child(directory.path(), &[], script);
        tokio::time::timeout(Duration::from_secs(2), child.wait())
            .await
            .unwrap()
            .unwrap();
        let output = admission();
        let cancel = Arc::new(Notify::new());
        let (_receiver, mut writer) = tokio::io::duplex(188);
        let outcome = supervise_stream_process_with_retry_context(
            &mut child,
            &mut writer,
            &cancel,
            "synthetic-teletext",
            None,
            Some(StreamFailureContext {
                song: None,
                graph: None,
                acceleration: ersatzrs_domain::HardwareAccelerationKind::Nvenc,
                source: None,
            }),
            &output,
        )
        .await;
        if hardware_failure {
            let DirectStreamOutcome::Failed(failure) = outcome else {
                panic!("empty output from a hardware startup rejection must retain fallback");
            };
            assert!(failure.encoder_open_failure);
            assert!(!failure.wrote_media);
        } else {
            assert!(matches!(
                outcome,
                DirectStreamOutcome::TransportFailed(TeletextPumpError::EmptyOutput)
            ));
        }
        assert!(child.try_wait().unwrap().is_some());
    }
}

#[tokio::test]
async fn hardware_rejection_without_raw_bytes_respawns_with_a_fresh_transport_gate() {
    let directory = tempfile::tempdir().unwrap();
    let original = fixture(3);
    let replacement = directory.path().join("synthetic-replacement.ts");
    std::fs::write(&replacement, flatten(&original)).unwrap();
    let mut child = output_child(
        directory.path(),
        &[],
        "printf 'Error while opening encoder' >&2; exit 1",
    );
    assert!(!child.wait().await.unwrap().success());
    let rebuilt = Arc::new(AtomicBool::new(false));
    let observer = Arc::clone(&rebuilt);
    let mut retry = Some(hardware_retry(immediate_stream_retry(Box::new(
        move |mode| {
            assert!(matches!(mode, StreamRetryMode::HardwareEncodeDenied(_)));
            observer.store(true, Ordering::SeqCst);
            Some(FfmpegCommand::new_with_executable(
                "sh",
                Vec::new(),
                vec![
                    "-c".into(),
                    "exec cat \"$1\"".into(),
                    "synthetic-replacement".into(),
                    replacement.to_string_lossy().into_owned(),
                ],
            ))
        },
    ))));
    let output = admission();
    let cancel = Arc::new(Notify::new());
    let (mut receiver, mut writer) = tokio::io::duplex(188);
    let forward = async {
        let outcome = supervise_stream_command_with_retry(
            child,
            &mut writer,
            &cancel,
            "synthetic-teletext",
            &mut retry,
            None,
            &output,
        )
        .await;
        drop(writer);
        outcome
    };
    let receive = async {
        let mut bytes = Vec::new();
        receiver.read_to_end(&mut bytes).await.unwrap();
        bytes
    };
    let (outcome, delivered) = Box::pin(tokio::time::timeout(Duration::from_secs(2), async {
        tokio::join!(forward, receive)
    }))
    .await
    .unwrap();
    assert!(matches!(outcome, DirectStreamOutcome::Completed));
    assert!(rebuilt.load(Ordering::SeqCst));
    assert_eq!(delivered.len(), original.len() * 188);
    assert_eq!(&delivered[..188], original[0]);
    assert!(delivered
        .chunks_exact(188)
        .any(|packet| packet[1] & 0x1f == 2 && packet[2] == 0));
}

#[tokio::test]
async fn final_transport_failure_becomes_a_body_error_after_accepted_bytes_and_writer_eof() {
    let output = admission();
    let cancel = Arc::new(Notify::new());
    let (receiver, mut writer) = tokio::io::duplex(4096);
    let (body, completion) = stream_body_for_output(receiver, cancel, &output);
    let mut body = body.into_data_stream();
    let prefix = b"synthetic accepted output prefix";
    writer.write_all(prefix).await.unwrap();
    let outcome = DirectStreamOutcome::TransportFailed(TeletextPumpError::Read(
        std::io::ErrorKind::ConnectionReset,
    ));
    completion
        .unwrap()
        .send(stream_output_result(&outcome))
        .unwrap();
    assert_eq!(body.next().await.unwrap().unwrap().as_ref(), prefix);
    {
        let next = body.next();
        tokio::pin!(next);
        assert!(futures_util::poll!(&mut next).is_pending());
    }
    drop(writer);
    let error = body.next().await.unwrap().unwrap_err();
    assert_eq!(error.to_string(), "teletext transport failed");
}

#[tokio::test]
async fn final_body_reports_lost_supervision_instead_of_clean_eof() {
    let output = admission();
    let (receiver, writer) = tokio::io::duplex(188);
    let (body, completion) = stream_body_for_output(receiver, Arc::new(Notify::new()), &output);
    drop(completion.expect("authored output must await supervision"));
    drop(writer);
    let mut body = body.into_data_stream();
    let error = body.next().await.unwrap().unwrap_err();
    assert_eq!(error.to_string(), "encoded output supervisor ended");
}

#[tokio::test]
async fn final_body_producer_error_never_exposes_the_private_stderr_diagnostic() {
    let output = admission();
    let (receiver, writer) = tokio::io::duplex(188);
    let (body, completion) = stream_body_for_output(receiver, Arc::new(Notify::new()), &output);
    let diagnostic = "synthetic private producer diagnostic";
    let outcome = DirectStreamOutcome::Failed(diagnostic.to_owned().into());
    completion
        .unwrap()
        .send(stream_output_result(&outcome))
        .unwrap();
    drop(writer);
    let mut body = body.into_data_stream();
    let error = body.next().await.unwrap().unwrap_err().to_string();
    assert_eq!(error, "encoded output producer failed");
    assert!(!error.contains(diagnostic));
}

#[test]
fn failed_spawn_mapping_preserves_cleanup_uncertainty_and_redacts_body_messages() {
    let diagnostic = "synthetic private spawn diagnostic";
    for (error, cleanup_unconfirmed, public_message) in [
        (
            StreamSpawnError::Spawn(diagnostic.to_owned()),
            false,
            "encoded output producer failed",
        ),
        (
            StreamSpawnError::MissingStdout,
            false,
            "encoded output producer failed",
        ),
        (
            StreamSpawnError::CleanupUnconfirmed(diagnostic.to_owned()),
            true,
            "encoded output cleanup is unconfirmed",
        ),
    ] {
        let outcome = failed_stream_spawn_outcome(&error);
        if cleanup_unconfirmed {
            assert!(matches!(outcome, DirectStreamOutcome::CleanupUnconfirmed));
        } else {
            assert!(matches!(outcome, DirectStreamOutcome::Failed(_)));
        }
        let message = stream_output_result(&outcome).unwrap_err();
        assert_eq!(message, public_message);
        assert!(!message.contains(diagnostic));
    }
}
