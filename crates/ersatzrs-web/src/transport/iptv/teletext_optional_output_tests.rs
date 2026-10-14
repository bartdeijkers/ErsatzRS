use super::super::{
    retained_stream_buffer::{ReserveError, TERMINAL_SUCCESS},
    teletext_output::tests::{authored_set, fixture, flatten, policy},
};
use super::*;
use std::{sync::atomic::Ordering, time::Duration};

fn binding(config: ersatzrs_ffmpeg::teletext::TeletextOutputConfig) -> TeletextOutputBinding {
    TeletextOutputBinding::new(config, authored_set(), Duration::from_millis(40)).unwrap()
}

fn absent_binding(
    config: ersatzrs_ffmpeg::teletext::TeletextOutputConfig,
) -> TeletextOutputBinding {
    TeletextOutputBinding::new_absent(config, Duration::from_millis(40)).unwrap()
}

#[tokio::test(start_paused = true)]
async fn initially_absent_optional_output_preserves_every_source_byte_including_pmt_reset() {
    let mut packets = fixture(30);
    // Retain the actual startup reset flag and payload, including PMT CRC/version.
    // An active PMT installer would replace these bytes rather than forwarding them.
    packets[1].copy_within(4..186, 6);
    packets[1][3] = 0x30;
    packets[1][4] = 1;
    packets[1][5] = 0x80;
    let original = flatten(&packets);
    let selected = absent_binding(policy());
    let mut source = std::io::Cursor::new(original.clone());
    let mut output = Vec::new();
    let result = forward_optional_teletext_output(&mut source, &mut output, &selected, 1024 * 188)
        .await
        .unwrap();
    assert!(result.rejection.is_none());
    assert_eq!(result.bytes, u64::try_from(original.len()).unwrap());
    assert_eq!(output, original);
    assert_eq!(source.position(), u64::try_from(original.len()).unwrap());
    assert!(output
        .chunks_exact(188)
        .all(|packet| { u16::from_be_bytes([packet[1] & 0x1f, packet[2]]) != 512 }));
}

#[tokio::test(start_paused = true)]
async fn initially_absent_optional_admission_failures_preserve_ordinary_playback() {
    let original = flatten(&fixture(30));
    for mode in 0..3 {
        let mut config = policy();
        match mode {
            0 => config.program = 999,
            1 => config.elementary_pid = 300,
            _ => config.clock.pcr_pid = 301,
        }
        let mut source = std::io::Cursor::new(original.clone());
        let mut output = Vec::new();
        let result = forward_optional_teletext_output(
            &mut source,
            &mut output,
            &absent_binding(config),
            1024 * 188,
        )
        .await
        .unwrap();
        assert!(result.rejection.is_some());
        assert_eq!(output, original);
        assert_eq!(result.bytes, u64::try_from(original.len()).unwrap());
        assert_eq!(source.position(), result.bytes);
    }
}

#[tokio::test(start_paused = true)]
async fn existing_bound_control_activates_initially_absent_output_after_original_pmt() {
    use super::super::teletext_binding::{
        authored_tests::{actual_units, authored_service_with_counter},
        teletext_output_control_channel, TeletextOutputTransitionPhase, TeletextOutputUpdate,
    };
    let original = fixture(50);
    let service = Arc::new(authored_service_with_counter(4));
    let (sender, control, status) = teletext_output_control_channel();
    sender.send_replace(Some(
        TeletextOutputUpdate::replace(1, Arc::clone(&service)).unwrap(),
    ));
    let selected = absent_binding(policy())
        .with_control(control)
        .with_optional_startup(1024 * 188)
        .unwrap();
    let mut source = std::io::Cursor::new(flatten(&original));
    let mut output = Vec::new();
    let count = forward_bound_teletext_output(&mut source, &mut output, &selected)
        .await
        .unwrap();
    assert_eq!(count, u64::try_from(original.len() * 188).unwrap());
    assert_eq!(output.len(), original.len() * 188);
    assert_eq!(status.borrow().sequence(), Some(1));
    assert_eq!(
        status.borrow().phase(),
        TeletextOutputTransitionPhase::Applied
    );
    assert_eq!(&output[..2 * 188], flatten(&original[..2]));
    let packet_pid = |packet: &[u8]| u16::from_be_bytes([packet[1] & 0x1f, packet[2]]);
    let first_pes = output
        .chunks_exact(188)
        .position(|packet| packet_pid(packet) == 512)
        .unwrap();
    let (announcement_index, announcement) = output
        .chunks_exact(188)
        .enumerate()
        .find(|(_, packet)| packet_pid(packet) == 3337 && packet[10] != 0xc1)
        .unwrap();
    assert!(announcement_index < first_pes);
    assert_eq!(announcement[3] & 15, 1);
    assert_eq!(announcement[10], 0xc3);
    assert!(announcement
        .windows(5)
        .any(|bytes| bytes == [6, 0xe2, 0, 0xf0, 7]));
    for (before, after) in original.iter().zip(output.chunks_exact(188)) {
        if !matches!(packet_pid(before), 3337 | 8191) {
            assert_eq!(before.as_slice(), after);
        }
    }
    let units = actual_units(&output);
    for page in service.pages() {
        for unit in page.data_units() {
            assert!(
                units.contains(unit),
                "missing complete authored page/resource unit"
            );
        }
    }
}

#[tokio::test(start_paused = true)]
async fn final_output_dispatch_keeps_strict_default_and_requires_explicit_optional_policy() {
    let original = flatten(&fixture(30));
    let mut config = policy();
    config.program = 999;
    assert!(binding(config).optional_startup_bytes().is_none());
    for maximum in [0, config.max_buffer_packets * 188 + 1, 32 * 1024 * 1024 + 1] {
        assert!(matches!(
            binding(config).with_optional_startup(maximum),
            Err(TeletextPumpError::Configuration)
        ));
    }
    for optional in [false, true] {
        let selected = binding(config);
        let selected = if optional {
            selected.with_optional_startup(191).unwrap()
        } else {
            selected
        };
        let mut source = std::io::Cursor::new(original.clone());
        let mut output = Vec::new();
        let result = forward_bound_teletext_output(&mut source, &mut output, &selected).await;
        if optional {
            assert_eq!(result.unwrap(), u64::try_from(original.len()).unwrap());
            assert_eq!(output, original);
        } else {
            assert!(matches!(result, Err(TeletextPumpError::Gate(_))));
            assert!(output.is_empty());
        }
    }
}

#[tokio::test(start_paused = true)]
async fn retained_final_dispatch_selects_optional_startup_without_changing_strict_default() {
    let policy = ReservePolicy::new(5_000_000, Some(Duration::from_secs(1))).unwrap();
    let original = flatten(&fixture(50));
    let terminal = AtomicU8::new(TERMINAL_SUCCESS);
    let cancel = Notify::new();
    for optional in [false, true] {
        let selected = retained_binding(policy, true);
        let selected = if optional {
            selected.with_optional_startup(191).unwrap()
        } else {
            selected
        };
        let mut source = std::io::Cursor::new(original.clone());
        let mut output = Vec::new();
        let result = forward_bound_retained_teletext_output(
            &mut source,
            &mut output,
            policy,
            &terminal,
            Instant::now() + Duration::from_secs(2),
            &cancel,
            &selected,
        )
        .await;
        if optional {
            result.unwrap();
            assert_eq!(output, original);
        } else {
            assert!(matches!(result, Err(RetainedTeletextError::Gate(_))));
            assert!(output.is_empty());
        }
    }
}

#[tokio::test(start_paused = true)]
async fn optional_program_pid_clock_and_null_capacity_failures_replay_exact_source() {
    for mode in 0..4 {
        let mut config = policy();
        let mut packets = fixture(30);
        match mode {
            0 => config.program = 999,
            1 => config.elementary_pid = 300,
            2 => config.clock.pcr_pid = 301,
            _ => {
                for packet in &mut packets {
                    if packet[1] == 0x1f && packet[2] == 0xff {
                        packet[..4].copy_from_slice(&[0x47, 1, 44, 0x10]);
                    }
                }
            }
        }
        let original = flatten(&packets);
        let mut source = std::io::Cursor::new(original.clone());
        let mut output = Vec::new();
        let result = forward_optional_teletext_output(
            &mut source,
            &mut output,
            &binding(config),
            1024 * 188,
        )
        .await
        .unwrap();
        assert!(result.rejection.is_some());
        assert_eq!(result.bytes, u64::try_from(original.len()).unwrap());
        assert_eq!(output, original);
        assert_eq!(source.position(), u64::try_from(original.len()).unwrap());
    }
}

#[tokio::test(start_paused = true)]
async fn partial_journal_cap_preserves_current_chunk_and_does_not_consume_overflow() {
    let original = flatten(&fixture(30));
    for maximum in [1, 187, 188, 191, 1024 * 188] {
        let mut source = std::io::Cursor::new(original.clone());
        let mut output = Vec::new();
        let mut config = policy();
        config.program = 999;
        let result =
            forward_optional_teletext_output(&mut source, &mut output, &binding(config), maximum)
                .await
                .unwrap();
        assert!(result.rejection.is_some());
        assert_eq!(output, original);
        assert_eq!(result.bytes, u64::try_from(original.len()).unwrap());
    }
    let selected = binding(policy());
    for maximum in [0, 1024 * 188 + 1] {
        let mut source = std::io::Cursor::new(original.clone());
        let mut output = Vec::new();
        assert!(matches!(
            forward_optional_teletext_output(&mut source, &mut output, &selected, maximum).await,
            Err(TeletextPumpError::Configuration)
        ));
        assert_eq!(source.position(), 0);
        assert!(output.is_empty());
    }
}

#[tokio::test(start_paused = true)]
async fn complete_strict_output_never_falls_back_and_preserves_av_extent() {
    let packets = fixture(30);
    let original = flatten(&packets);
    let mut source = std::io::Cursor::new(original.clone());
    let mut output = Vec::new();
    let result =
        forward_optional_teletext_output(&mut source, &mut output, &binding(policy()), 1024 * 188)
            .await
            .unwrap();
    assert!(result.rejection.is_none());
    assert_eq!(result.bytes, u64::try_from(original.len()).unwrap());
    assert_eq!(output.len(), original.len());
    assert_ne!(output, original);
    for (before, after) in packets.iter().zip(output.chunks_exact(188)) {
        if before[1] == 1 && before[2] == 44 {
            assert_eq!(before.as_slice(), after);
        }
    }
}

struct Sink {
    mode: u8,
    calls: usize,
    bytes: Vec<u8>,
}
impl AsyncWrite for Sink {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        this.calls += 1;
        match this.mode {
            0 => Poll::Pending,
            1 if this.calls == 1 => {
                this.bytes.push(bytes[0]);
                Poll::Ready(Ok(1))
            }
            1 => Poll::Ready(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "synthetic sink",
            ))),
            _ => {
                this.bytes.extend_from_slice(bytes);
                Poll::Ready(Ok(bytes.len()))
            }
        }
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Err(io::Error::other("synthetic flush")))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[tokio::test(start_paused = true)]
async fn first_write_pending_partial_failure_and_flush_failure_never_replay_raw_bytes() {
    for mode in 0..3 {
        let mut source = std::io::Cursor::new(flatten(&fixture(30)));
        let mut sink = Sink {
            mode,
            calls: 0,
            bytes: Vec::new(),
        };
        let error = forward_optional_teletext_output(
            &mut source,
            &mut sink,
            &binding(policy()),
            1024 * 188,
        )
        .await
        .unwrap_err();
        match mode {
            0 => {
                assert!(matches!(error, TeletextPumpError::Gate(_)));
                assert!(sink.bytes.is_empty());
            }
            1 => {
                assert!(matches!(
                    error,
                    TeletextPumpError::Write(io::ErrorKind::BrokenPipe)
                ));
                assert_eq!(sink.bytes.len(), 1);
                assert_eq!(sink.calls, 2);
            }
            _ => {
                assert!(matches!(
                    error,
                    TeletextPumpError::Write(io::ErrorKind::Other)
                ));
                assert!(!sink.bytes.is_empty());
            }
        }
        assert!(sink.calls > 0);
    }
}

#[tokio::test(start_paused = true)]
async fn initially_absent_first_write_failure_stays_terminal_without_raw_replay() {
    let original = flatten(&fixture(30));
    let mut source = std::io::Cursor::new(original.clone());
    let mut sink = Sink {
        mode: 1,
        calls: 0,
        bytes: Vec::new(),
    };
    let error = forward_optional_teletext_output(
        &mut source,
        &mut sink,
        &absent_binding(policy()),
        1024 * 188,
    )
    .await
    .unwrap_err();
    assert!(matches!(
        error,
        TeletextPumpError::Write(io::ErrorKind::BrokenPipe)
    ));
    assert_eq!(sink.calls, 2);
    assert_eq!(sink.bytes, original[..1]);
}

struct FaultSource {
    first: bool,
}
impl AsyncRead for FaultSource {
    fn poll_read(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        output: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if this.first {
            this.first = false;
            output.put_slice(&[0x47]);
            Poll::Ready(Ok(()))
        } else {
            Poll::Ready(Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "synthetic source",
            )))
        }
    }
}

#[tokio::test(start_paused = true)]
async fn real_read_empty_and_first_byte_timeout_remain_terminal_without_output() {
    let selected = binding(policy());
    let mut fault = FaultSource { first: true };
    let mut output = Vec::new();
    assert!(matches!(
        forward_optional_teletext_output(&mut fault, &mut output, &selected, 1024 * 188).await,
        Err(TeletextPumpError::Read(io::ErrorKind::UnexpectedEof))
    ));
    assert!(output.is_empty());
    let mut empty = std::io::Cursor::new(Vec::<u8>::new());
    assert!(matches!(
        forward_optional_teletext_output(&mut empty, &mut output, &selected, 1024 * 188).await,
        Err(TeletextPumpError::EmptyOutput)
    ));
    let (mut waiting, _writer) = tokio::io::duplex(188);
    assert!(matches!(
        forward_optional_teletext_output(&mut waiting, &mut output, &selected, 1024 * 188).await,
        Err(TeletextPumpError::FirstByteTimeout)
    ));
    assert!(output.is_empty());
}

fn retained_binding(policy: ReservePolicy, invalid: bool) -> TeletextOutputBinding {
    let mut config = super::super::teletext_output::tests::policy();
    config.max_buffer_packets = policy.capacity_bytes().div_ceil(188);
    config.max_hold = Duration::from_secs(9);
    if invalid {
        config.program = 999;
    }
    binding(config)
}

#[tokio::test(start_paused = true)]
async fn retained_startup_fallback_preserves_full_remainder_and_original_cancellation() {
    let policy = ReservePolicy::new(5_000_000, Some(Duration::from_secs(1))).unwrap();
    let selected = retained_binding(policy, true);
    let original = flatten(&fixture(50));
    let mut source = std::io::Cursor::new(original.clone());
    let mut output = Vec::new();
    let terminal = AtomicU8::new(TERMINAL_SUCCESS);
    let cancel = Notify::new();
    let result = forward_optional_retained_teletext_output(
        &mut source,
        &mut output,
        policy,
        &terminal,
        Instant::now() + Duration::from_secs(2),
        &cancel,
        OptionalTeletextStartup {
            binding: &selected,
            maximum_bytes: 1024 * 188,
        },
    )
    .await
    .unwrap();
    assert!(result.rejection.is_some());
    assert_eq!(result.bytes, u64::try_from(original.len()).unwrap());
    assert_eq!(output, original);
    assert_eq!(terminal.load(Ordering::Acquire), TERMINAL_SUCCESS);
    let mut source = std::io::Cursor::new(original);
    let mut output = Vec::new();
    cancel.notify_one();
    let result = forward_optional_retained_teletext_output(
        &mut source,
        &mut output,
        policy,
        &terminal,
        Instant::now() + Duration::from_secs(2),
        &cancel,
        OptionalTeletextStartup {
            binding: &selected,
            maximum_bytes: 1024 * 188,
        },
    )
    .await;
    assert!(matches!(
        result,
        Err(RetainedTeletextError::Reserve(ReserveError::Cancelled))
    ));
    assert!(output.is_empty());
}

#[test]
fn retained_allocation_model_charges_journal_and_processing_before_admission() {
    let policy = ReservePolicy::new(5_000_000, Some(Duration::from_secs(1))).unwrap();
    let selected = retained_binding(policy, false);
    assert!(retained_startup_budget(policy, &selected, 1024 * 188).is_ok());
    // The processing cap now makes room for an entire pre-write reserve journal.
    assert!(retained_startup_budget(policy, &selected, policy.capacity_bytes()).is_ok());
    assert!(matches!(
        retained_startup_budget(policy, &selected, MAX_BYTES),
        Err(RetainedTeletextError::Configuration)
    ));
}

#[tokio::test]
async fn capped_reader_stops_before_consuming_the_next_partial_packet_byte() {
    let original = flatten(&fixture(3));
    let mut source = std::io::Cursor::new(original.clone());
    let selected = binding(policy());
    let state = Startup::new(191, &selected).unwrap();
    let mut reader = TeeReader::new(&mut source, &state).unwrap();
    let mut bytes = [0_u8; 400];
    assert_eq!(reader.read(&mut bytes).await.unwrap(), 191);
    assert_eq!(&bytes[..191], &original[..191]);
    assert_eq!(
        reader.read(&mut bytes).await.unwrap_err().kind(),
        io::ErrorKind::Other
    );
    drop(reader);
    assert_eq!(source.position(), 191);
    let state = state.lock().unwrap();
    assert!(state.capacity_reached);
    assert_eq!(state.bytes, original[..191]);
    assert!(!state.committed);
}
