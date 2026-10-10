//! Unix process fixtures cover the retained response owner without `FFmpeg`.
//! Native Windows process and packaged playback acceptance are separate gates.

use std::sync::atomic::{AtomicBool, AtomicU8};

use futures_util::StreamExt as _;

use super::super::teletext_output::tests::{authored_set, fixture, flatten, policy};
use super::{teletext_tests::output_child, *};

struct Epoch {
    raw_response: Response,
    raw_writer: tokio::io::DuplexStream,
    raw_cancel: Arc<Notify>,
    retained: RetainedTsOutput,
    output: TeletextOutputAdmission,
    public_body: Body,
    public_writer: tokio::io::DuplexStream,
    completion: StreamOutputCompletion,
    cancel: Arc<Notify>,
}

fn epoch() -> Epoch {
    let remaining = Some(Duration::from_millis(60));
    let reserve = ReservePolicy::new(5_000_000, remaining).unwrap();
    let mut config = policy();
    config.max_buffer_packets = reserve.capacity_bytes().div_ceil(188);
    config.max_hold = Duration::from_secs(9);
    let binding =
        TeletextOutputBinding::new(config, authored_set(), Duration::from_millis(40)).unwrap();
    let output = TeletextOutputAdmission::with_binding(
        TeletextOutputRole::FinalWrapperMpegTs,
        Arc::new(binding),
    )
    .unwrap();
    let retained = RetainedTsOutput {
        muxrate_bps: reserve.muxrate_bps(),
        remaining,
        terminal: Arc::new(AtomicU8::new(0)),
        completed: Arc::new(Notify::new()),
    };
    let raw_cancel = Arc::new(Notify::new());
    let (reader, raw_writer) = tokio::io::duplex(4096);
    let (raw_body, completion) = stream_body_for_output(
        reader,
        Arc::clone(&raw_cancel),
        &TeletextOutputAdmission::plain(TeletextOutputRole::InternalItem),
    );
    assert!(completion.is_none(), "the raw child must not author pages");
    let cancel = Arc::new(Notify::new());
    let (reader, public_writer) = tokio::io::duplex(188);
    let (public_body, completion) = stream_body_for_output(reader, Arc::clone(&cancel), &output);
    Epoch {
        raw_response: raw_body.into_response(),
        raw_writer,
        raw_cancel,
        retained,
        output,
        public_body,
        public_writer,
        completion: completion.unwrap(),
        cancel,
    }
}

async fn supervise_raw(
    child: &mut RemoteStreamProcess,
    writer: &mut tokio::io::DuplexStream,
    cancel: &Arc<Notify>,
) -> DirectStreamOutcome {
    supervise_stream_process_with_retry_context(
        child,
        writer,
        cancel,
        "synthetic-retained-teletext",
        None,
        None,
        &TeletextOutputAdmission::plain(TeletextOutputRole::InternalItem),
    )
    .await
}

fn report_producer(retained: &RetainedTsOutput, outcome: &DirectStreamOutcome) {
    retained.terminal.store(
        if matches!(outcome, DirectStreamOutcome::Completed) {
            TERMINAL_SUCCESS
        } else {
            TERMINAL_FAILURE
        },
        Ordering::Release,
    );
    retained.completed.notify_one();
}

fn pid(packet: &[u8]) -> u16 {
    (u16::from(packet[1] & 0x1f) << 8) | u16::from(packet[2])
}

fn authored_pmt_entries(packet: &[u8]) -> usize {
    let section = &packet[5..];
    let end = 3 + ((usize::from(section[1] & 0x0f) << 8) | usize::from(section[2])) - 4;
    let mut offset = 12 + ((usize::from(section[10] & 0x0f) << 8) | usize::from(section[11]));
    let mut entries = 0;
    while offset < end {
        let elementary_pid =
            (u16::from(section[offset + 1] & 0x1f) << 8) | u16::from(section[offset + 2]);
        entries += usize::from(elementary_pid == 512);
        offset +=
            5 + ((usize::from(section[offset + 3] & 0x0f) << 8) | usize::from(section[offset + 4]));
    }
    assert_eq!(offset, end);
    entries
}

#[tokio::test]
async fn finite_retained_child_authors_once_and_preserves_source_clock_positions() {
    let directory = tempfile::tempdir().unwrap();
    let original = fixture(3);
    assert!(original.iter().all(|packet| pid(packet) != 512));
    let mut child = output_child(directory.path(), &flatten(&original), "exec cat \"$1\"");
    let Epoch {
        raw_response,
        mut raw_writer,
        raw_cancel,
        retained,
        output,
        public_body,
        public_writer,
        completion,
        cancel,
    } = epoch();
    let produce = async {
        let outcome = supervise_raw(&mut child, &mut raw_writer, &raw_cancel).await;
        assert!(child.try_wait().unwrap().unwrap().success());
        report_producer(&retained, &outcome);
        drop(raw_writer);
        outcome
    };
    let forward = async {
        let result = forward_retained_response(
            raw_response,
            public_writer,
            &retained,
            tokio::time::Instant::now() + Duration::from_secs(1),
            &cancel,
            output.binding(),
        )
        .await;
        completion.send(result).unwrap();
        result
    };
    let receive = async {
        let mut stream = public_body.into_data_stream();
        let mut delivered = Vec::new();
        while let Some(chunk) = stream.next().await {
            delivered.extend_from_slice(&chunk.expect("the verified finite producer must succeed"));
        }
        delivered
    };
    let (outcome, result, delivered) = tokio::time::timeout(
        Duration::from_secs(2),
        Box::pin(async { tokio::join!(produce, forward, receive) }),
    )
    .await
    .unwrap();
    assert!(matches!(outcome, DirectStreamOutcome::Completed));
    assert_eq!(result, Ok(()));
    assert_eq!(delivered.len(), original.len() * 188);
    assert_eq!(authored_pmt_entries(&delivered[188..376]), 1);
    let starts = delivered
        .chunks_exact(188)
        .filter(|packet| pid(packet) == 512 && packet[1] & 0x40 != 0)
        .count();
    assert_eq!(starts, 3, "one header, one row and one cycle terminator");
    for (before, after) in original.iter().zip(delivered.chunks_exact(188)) {
        if pid(before) != 8191 && pid(before) != 3337 {
            assert_eq!(before.as_slice(), after);
        }
    }
}

#[tokio::test]
async fn malformed_transport_and_nonzero_producer_surface_body_errors_after_reaping() {
    for malformed in [true, false] {
        let directory = tempfile::tempdir().unwrap();
        let mut original = fixture(3);
        if malformed {
            original[0][0] = 0;
        }
        let script = if malformed {
            "exec cat \"$1\""
        } else {
            "cat \"$1\"; exit 7"
        };
        let mut child = output_child(directory.path(), &flatten(&original), script);
        let Epoch {
            raw_response,
            mut raw_writer,
            raw_cancel,
            retained,
            output,
            public_body,
            public_writer,
            completion,
            cancel,
        } = epoch();
        let reaped = AtomicBool::new(false);
        let produce = async {
            let outcome = supervise_raw(&mut child, &mut raw_writer, &raw_cancel).await;
            assert!(child.try_wait().unwrap().is_some());
            reaped.store(true, Ordering::Release);
            report_producer(&retained, &outcome);
            drop(raw_writer);
            outcome
        };
        let forward = async {
            let result = forward_retained_response(
                raw_response,
                public_writer,
                &retained,
                tokio::time::Instant::now() + Duration::from_secs(1),
                &cancel,
                output.binding(),
            )
            .await;
            completion.send(result).unwrap();
            result
        };
        let receive = async {
            let mut stream = public_body.into_data_stream();
            let error = stream
                .next()
                .await
                .unwrap()
                .expect_err("no unverified prefix may escape");
            assert!(
                reaped.load(Ordering::Acquire),
                "body failure must follow owned cleanup"
            );
            assert_ne!(retained.terminal.load(Ordering::Acquire), 0);
            assert!(stream.next().await.is_none());
            error
        };
        let (outcome, result, _body_error) = tokio::time::timeout(
            Duration::from_secs(2),
            Box::pin(async { tokio::join!(produce, forward, receive) }),
        )
        .await
        .unwrap();
        assert!(result.is_err());
        if !malformed {
            assert!(matches!(outcome, DirectStreamOutcome::Failed(_)));
            assert_eq!(child.try_wait().unwrap().unwrap().code(), Some(7));
        }
    }
}

#[tokio::test]
async fn retained_failure_waits_for_the_raw_producer_completion_notification() {
    let directory = tempfile::tempdir().unwrap();
    let mut bytes = flatten(&fixture(3));
    bytes[0] = 0;
    let mut child = output_child(directory.path(), &bytes, "exec cat \"$1\"");
    let Epoch {
        raw_response,
        mut raw_writer,
        raw_cancel,
        retained,
        output,
        public_body,
        public_writer,
        completion,
        cancel,
    } = epoch();
    let (reaped_sender, reaped_receiver) = tokio::sync::oneshot::channel();
    let (release_sender, release_receiver) = tokio::sync::oneshot::channel();
    let produce = async {
        let outcome = supervise_raw(&mut child, &mut raw_writer, &raw_cancel).await;
        assert!(child.try_wait().unwrap().is_some());
        reaped_sender.send(()).unwrap();
        release_receiver.await.unwrap();
        report_producer(&retained, &outcome);
        drop(raw_writer);
    };
    let operation = forward_retained_response(
        raw_response,
        public_writer,
        &retained,
        tokio::time::Instant::now() + Duration::from_secs(1),
        &cancel,
        output.binding(),
    );
    tokio::pin!(operation);
    let forward = async {
        // Keep driving the reader while the supervisor drains stdout. Polling
        // just once and then waiting for reaping can deadlock its bounded pipe.
        tokio::select! {
            result = &mut operation => panic!("transport completed before producer notification: {result:?}"),
            result = reaped_receiver => result.unwrap(),
        }
        assert_eq!(retained.terminal.load(Ordering::Acquire), 0);
        assert!(
            futures_util::poll!(&mut operation).is_pending(),
            "transport rejection must still await the producer's owned completion"
        );
        release_sender.send(()).unwrap();
        let result = operation.await;
        completion.send(result).unwrap();
        result
    };
    let receive = async {
        let mut stream = public_body.into_data_stream();
        assert!(stream.next().await.unwrap().is_err());
        assert!(stream.next().await.is_none());
    };
    let ((), result, ()) = tokio::time::timeout(Duration::from_secs(2), async {
        tokio::join!(produce, forward, receive)
    })
    .await
    .unwrap();
    assert!(result.is_err());
}

#[tokio::test]
async fn public_body_disposal_and_explicit_cancellation_reap_the_raw_child() {
    for dispose_body in [true, false] {
        let directory = tempfile::tempdir().unwrap();
        let mut child = output_child(directory.path(), &[], "exec sleep 30");
        let Epoch {
            raw_response,
            mut raw_writer,
            raw_cancel,
            retained,
            output,
            public_body,
            public_writer,
            completion,
            cancel,
        } = epoch();
        let produce = async {
            let outcome = supervise_raw(&mut child, &mut raw_writer, &raw_cancel).await;
            assert!(child.try_wait().unwrap().is_some());
            report_producer(&retained, &outcome);
            drop(raw_writer);
            outcome
        };
        let operation = forward_retained_response(
            raw_response,
            public_writer,
            &retained,
            tokio::time::Instant::now() + Duration::from_secs(1),
            &cancel,
            output.binding(),
        );
        tokio::pin!(operation);
        let forward = async {
            assert!(futures_util::poll!(&mut operation).is_pending());
            let mut body = Some(public_body);
            if dispose_body {
                drop(body.take());
            } else {
                cancel.notify_one();
            }
            let result = operation.await;
            let _ = completion.send(result);
            if let Some(body) = body {
                let mut stream = body.into_data_stream();
                assert!(stream.next().await.unwrap().is_err());
                assert!(stream.next().await.is_none());
            }
            result
        };
        let (outcome, result) = tokio::time::timeout(Duration::from_secs(2), async {
            tokio::join!(produce, forward)
        })
        .await
        .unwrap();
        assert!(matches!(outcome, DirectStreamOutcome::Cancelled));
        assert!(result.is_err());
        assert_eq!(retained.terminal.load(Ordering::Acquire), TERMINAL_FAILURE);
    }
}

async fn uncertain_cleanup_retry(branch: u8, invoked: Arc<AtomicBool>) -> StreamDenialRetry {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query(r#"CREATE TABLE "ConfigElement" ("Key" TEXT PRIMARY KEY, "Value" TEXT NULL)"#)
        .execute(&pool)
        .await
        .unwrap();
    StreamDenialRetry {
        song: None,
        pool,
        shared_ffmpeg_runtime: Arc::new(RwLock::new(FfmpegRuntimeReport::for_tests())),
        runtime_capabilities: FfmpegCapabilityReport::default(),
        acceleration: ersatzrs_domain::HardwareAccelerationKind::Nvenc,
        source_stream: (branch == 1).then(|| SourceVideoStream::new(SourceVideoCodec::Av1)),
        graph_operation: (branch == 0)
            .then_some(ersatzrs_ffmpeg::HardwareGraphOperation::NvencHdrVulkanLibplacebo),
        ffmpeg_path: "sh".into(),
        rebuild: Some(StreamRetryBuilder(Box::new(move |mode, _cancel| {
            Box::pin(async move {
                assert!(matches!(
                    (branch, mode),
                    (0, StreamRetryMode::HardwareGraphDenied(_))
                        | (1, StreamRetryMode::HardwareDecodeDenied(_))
                        | (2, StreamRetryMode::HardwareEncodeDenied(_))
                ));
                assert!(!invoked.swap(true, Ordering::AcqRel));
                Err(StreamSpawnError::CleanupUnconfirmed(
                    "synthetic retained retry cleanup is unconfirmed".into(),
                ))
            })
        }))),
    }
}

#[tokio::test]
async fn hardware_retry_cleanup_uncertainty_reaches_the_body_for_all_denial_branches() {
    for (branch, script) in [
        (
            0,
            "printf '[libplacebo] Failed to initialize renderer' >&2; exit 1",
        ),
        (
            1,
            "printf \"Your platform doesn't support hardware accelerated AV1 decoding.\" >&2; exit 1",
        ),
        (2, "printf 'Error while opening encoder' >&2; exit 1"),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let mut child = output_child(directory.path(), &[], script);
        // Establish the real child's nonzero exit and reap it before handing
        // ownership to the retry supervisor; stdout/stderr remain available.
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), child.wait())
                .await
                .unwrap()
                .unwrap()
                .code(),
            Some(1)
        );
        assert!(child.try_wait().unwrap().is_some());
        let invoked = Arc::new(AtomicBool::new(false));
        let mut retry = Some(uncertain_cleanup_retry(branch, Arc::clone(&invoked)).await);
        let binding =
            TeletextOutputBinding::new(policy(), authored_set(), Duration::from_millis(40))
                .unwrap();
        let output = TeletextOutputAdmission::with_binding(
            TeletextOutputRole::FinalDirectMpegTs,
            Arc::new(binding),
        )
        .unwrap();
        let cancel = Arc::new(Notify::new());
        let (receiver, mut writer) = tokio::io::duplex(188);
        let (body, completion) = stream_body_for_output(receiver, Arc::clone(&cancel), &output);
        let produce = async {
            let outcome = supervise_stream_command_with_retry(
                child,
                &mut writer,
                &cancel,
                "synthetic-retained-retry",
                &mut retry,
                None,
                &output,
            )
            .await;
            completion
                .unwrap()
                .send(stream_output_result(&outcome))
                .unwrap();
            drop(writer);
            outcome
        };
        let receive = async {
            let mut stream = body.into_data_stream();
            let error = stream
                .next()
                .await
                .unwrap()
                .expect_err("unconfirmed retry cleanup must fail the public body");
            assert!(error.to_string().contains("cleanup is unconfirmed"));
            assert!(stream.next().await.is_none());
        };
        let (outcome, ()) = tokio::time::timeout(Duration::from_secs(2), async {
            tokio::join!(produce, receive)
        })
        .await
        .unwrap();
        assert!(matches!(outcome, DirectStreamOutcome::CleanupUnconfirmed));
        assert!(invoked.load(Ordering::Acquire));
        assert!(retry.is_none(), "the one retry must remain spent");
    }
}

#[test]
fn retained_retry_preparation_preserves_unconfirmed_termination_as_a_cleanup_error() {
    use ersatzrs_infra::addon_media_list_item_runtime::ItemRuntimeError;

    let result = retained_retry_input(Err(ItemRuntimeError::TerminationUnconfirmed));
    let Err(error @ StreamSpawnError::CleanupUnconfirmed(_)) = result else {
        panic!("unconfirmed producer termination cannot become an ordinary refused retry");
    };
    assert!(!error.cleanup_confirmed());
    assert!(matches!(
        failed_stream_spawn_outcome(&error),
        DirectStreamOutcome::CleanupUnconfirmed
    ));
    assert!(matches!(retained_retry_input(Ok(None)), Ok(None)));
    for error in [
        ItemRuntimeError::Unavailable,
        ItemRuntimeError::PackageChanged,
        ItemRuntimeError::Unsupported,
        ItemRuntimeError::InvalidRequest,
        ItemRuntimeError::Failed,
        ItemRuntimeError::InvalidResult,
    ] {
        assert!(matches!(retained_retry_input(Err(error)), Ok(None)));
    }
}
