use std::{
    io,
    pin::Pin,
    sync::{atomic::Ordering, Arc, Mutex},
    task::{Context, Poll},
};

use tokio::io::{AsyncWriteExt as _, ReadBuf};

use super::super::{
    retained_stream_buffer::{TERMINAL_FAILURE, TERMINAL_SUCCESS},
    teletext_output::tests::{
        authored_set, fixture, flatten, policy as ordinary_policy, starve_after_first_interval,
    },
};
use super::*;

fn config(policy: ReservePolicy) -> TeletextOutputConfig {
    let mut config = ordinary_policy();
    config.clock.muxrate_bps = policy.muxrate_bps();
    config.max_buffer_packets = policy.capacity_bytes().div_ceil(PACKET_BYTES);
    config.max_hold = Duration::from_secs(9);
    config
}

fn binding(policy: ReservePolicy) -> TeletextOutputBinding {
    TeletextOutputBinding::new(config(policy), authored_set(), Duration::from_millis(40)).unwrap()
}

#[derive(Default)]
struct SourceProbe {
    received: usize,
    eof_polls: usize,
}

struct Source {
    bytes: Vec<u8>,
    chunk: usize,
    eof: bool,
    probe: Arc<Mutex<SourceProbe>>,
}

impl Source {
    fn new(bytes: Vec<u8>, chunk: usize, eof: bool) -> Self {
        Self {
            bytes,
            chunk,
            eof,
            probe: Arc::default(),
        }
    }
}

impl AsyncRead for Source {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        output: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let mut probe = self.probe.lock().unwrap();
        if probe.received == self.bytes.len() {
            if self.eof {
                probe.eof_polls += 1;
                return Poll::Ready(Ok(()));
            }
            return Poll::Pending;
        }
        let count = self
            .chunk
            .min(output.remaining())
            .min(self.bytes.len() - probe.received);
        output.put_slice(&self.bytes[probe.received..probe.received + count]);
        probe.received += count;
        Poll::Ready(Ok(()))
    }
}

#[derive(Clone, Copy)]
enum SinkMode {
    Ready,
    Short,
    WriteFault,
    FlushFault,
    WriteStall,
    FlushStall,
}

#[derive(Default)]
struct Writes {
    bytes: Vec<u8>,
    times: Vec<Instant>,
    received: Vec<usize>,
}

struct Sink {
    mode: SinkMode,
    after_bytes: usize,
    writes: Arc<Mutex<Writes>>,
    source: Arc<Mutex<SourceProbe>>,
    entered: Arc<Notify>,
    cancel_after: Option<(Arc<Notify>, usize)>,
}

impl Sink {
    fn new(source: &Source, mode: SinkMode, after_bytes: usize) -> Self {
        Self {
            mode,
            after_bytes,
            writes: Arc::default(),
            source: Arc::clone(&source.probe),
            entered: Arc::default(),
            cancel_after: None,
        }
    }
}

impl AsyncWrite for Sink {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let mut writes = self.writes.lock().unwrap();
        if writes.bytes.len() >= self.after_bytes {
            match self.mode {
                SinkMode::WriteFault => return Poll::Ready(Err(io::ErrorKind::BrokenPipe.into())),
                SinkMode::WriteStall => {
                    self.entered.notify_one();
                    return Poll::Pending;
                }
                SinkMode::Ready | SinkMode::Short | SinkMode::FlushFault | SinkMode::FlushStall => {
                }
            }
        }
        let count = if matches!(self.mode, SinkMode::Short) {
            bytes.len().min(7)
        } else {
            bytes.len()
        };
        writes.bytes.extend_from_slice(&bytes[..count]);
        writes.times.push(Instant::now());
        writes.received.push(self.source.lock().unwrap().received);
        if let Some((cancel, after)) = &self.cancel_after {
            if writes.bytes.len() >= *after {
                cancel.notify_one();
            }
        }
        Poll::Ready(Ok(count))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if self.writes.lock().unwrap().bytes.len() > self.after_bytes {
            match self.mode {
                SinkMode::FlushFault => {
                    return Poll::Ready(Err(io::ErrorKind::ConnectionReset.into()));
                }
                SinkMode::FlushStall => {
                    self.entered.notify_one();
                    return Poll::Pending;
                }
                SinkMode::Ready | SinkMode::Short | SinkMode::WriteFault | SinkMode::WriteStall => {
                }
            }
        }
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[test]
fn allocation_policy_derives_processing_space_and_rejects_incompatible_bindings() {
    for rate in [5_000_000, 8_000_000] {
        let policy = ReservePolicy::new(rate, None).unwrap();
        let config = config(policy);
        let window = validate_policy(policy, config).unwrap();
        assert!(window > 20_000);
        assert!(window <= MAX_PROCESSING_PACKETS);
        assert!(config.max_buffer_packets >= policy.capacity_bytes().div_ceil(PACKET_BYTES));
        let mut invalid = config;
        invalid.max_buffer_packets -= 1;
        assert!(matches!(
            validate_policy(policy, invalid),
            Err(RetainedTeletextError::Configuration)
        ));
        invalid = config;
        invalid.max_hold = Duration::from_secs(8);
        assert!(validate_policy(policy, invalid).is_err());
        invalid = config;
        invalid.clock.muxrate_bps += 1;
        assert!(validate_policy(policy, invalid).is_err());
        invalid = config;
        invalid.max_buffer_packets = MAX_ENCODED_BUFFER_BYTES / PACKET_BYTES;
        assert!(validate_policy(policy, invalid).is_err());
    }
}

#[tokio::test]
async fn partial_reserve_never_exposes_output_and_cancellation_keeps_priority() {
    let policy = ReservePolicy::new(5_000_000, None).unwrap();
    let binding = binding(policy);
    let mut source = Source::new(flatten(&fixture(3)), usize::MAX, false);
    let mut sink = Sink::new(&source, SinkMode::Ready, 0);
    let terminal = AtomicU8::new(0);
    let cancel = Notify::new();
    {
        let operation = forward_retained_ts_with_teletext(
            &mut source,
            &mut sink,
            policy,
            &terminal,
            Instant::now() + Duration::from_secs(1),
            &cancel,
            &binding,
        );
        tokio::pin!(operation);
        assert!(futures_util::poll!(&mut operation).is_pending());
        cancel.notify_one();
        assert!(matches!(
            operation.await,
            Err(RetainedTeletextError::Reserve(ReserveError::Cancelled))
        ));
    }
    assert!(sink.writes.lock().unwrap().bytes.is_empty());
    cancel.notify_one();
    assert!(matches!(
        forward_retained_ts_with_teletext(
            &mut source,
            &mut sink,
            policy,
            &terminal,
            Instant::now(),
            &cancel,
            &binding,
        )
        .await,
        Err(RetainedTeletextError::Reserve(ReserveError::Cancelled))
    ));
}

#[tokio::test]
async fn live_reserve_refills_between_packet_writes_without_draining_eight_seconds() {
    let policy = ReservePolicy::new(5_000_000, None).unwrap();
    let binding = binding(policy);
    let mut source = Source::new(flatten(&fixture(410)), usize::MAX, false);
    let mut sink = Sink::new(&source, SinkMode::Ready, 0);
    let cancel = Arc::new(Notify::new());
    sink.cancel_after = Some((Arc::clone(&cancel), 150 * PACKET_BYTES));
    let started = Instant::now();
    let result = tokio::time::timeout(
        Duration::from_secs(2),
        forward_retained_ts_with_teletext(
            &mut source,
            &mut sink,
            policy,
            &AtomicU8::new(0),
            started + Duration::from_secs(1),
            &cancel,
            &binding,
        ),
    )
    .await
    .unwrap();
    assert!(matches!(
        result,
        Err(RetainedTeletextError::Reserve(ReserveError::Cancelled))
    ));
    let writes = sink.writes.lock().unwrap();
    assert_eq!(writes.bytes.len(), 150 * PACKET_BYTES);
    assert!(writes.received[0] >= policy.reserve_bytes());
    assert!(writes.received.last().unwrap() > &writes.received[0]);
    assert!(source.probe.lock().unwrap().received < source.bytes.len());
    assert!(writes.times[0].duration_since(started) >= binding.config().playout_delay);
    assert!(writes.times[149].duration_since(writes.times[0]) >= Duration::from_millis(40));
}

#[tokio::test]
async fn short_successful_eof_delivers_a_complete_cycle_and_fractional_tail_once() {
    for mode in [SinkMode::Ready, SinkMode::Short] {
        let policy = ReservePolicy::new(5_000_000, Some(Duration::from_millis(70))).unwrap();
        let binding = binding(policy);
        let original = fixture(4)[..fixture(3).len() + 17].to_vec();
        let mut source = Source::new(flatten(&original), 7, true);
        let mut sink = Sink::new(&source, mode, 0);
        let started = Instant::now();
        forward_retained_ts_with_teletext(
            &mut source,
            &mut sink,
            policy,
            &AtomicU8::new(TERMINAL_SUCCESS),
            started + Duration::from_secs(1),
            &Notify::new(),
            &binding,
        )
        .await
        .unwrap();
        assert!(started.elapsed() < Duration::from_secs(1));
        assert_eq!(source.probe.lock().unwrap().eof_polls, 1);
        let writes = sink.writes.lock().unwrap();
        assert_eq!(writes.bytes.len(), original.len() * PACKET_BYTES);
        assert!(writes
            .received
            .iter()
            .all(|received| *received == source.bytes.len()));
        assert_eq!(
            &writes.bytes[fixture(3).len() * PACKET_BYTES..],
            flatten(&original[fixture(3).len()..])
        );
        for (before, after) in original.iter().zip(writes.bytes.chunks_exact(PACKET_BYTES)) {
            if before[1] & 0x1f == 1 && before[2] == 44 {
                assert_eq!(before.as_slice(), after);
            }
        }
    }
}

#[tokio::test]
async fn unknown_failed_premature_or_wrong_remainder_eof_never_exposes_a_prefix() {
    for (remaining, terminal) in [
        (Some(Duration::from_millis(60)), 0),
        (Some(Duration::from_millis(60)), TERMINAL_FAILURE),
        (Some(Duration::from_secs(5)), TERMINAL_SUCCESS),
        (None, TERMINAL_SUCCESS),
    ] {
        let policy = ReservePolicy::new(5_000_000, remaining).unwrap();
        let binding = binding(policy);
        let mut source = Source::new(flatten(&fixture(3)), usize::MAX, true);
        let mut sink = Sink::new(&source, SinkMode::Ready, 0);
        assert!(matches!(
            forward_retained_ts_with_teletext(
                &mut source,
                &mut sink,
                policy,
                &AtomicU8::new(terminal),
                Instant::now() + Duration::from_secs(1),
                &Notify::new(),
                &binding,
            )
            .await,
            Err(RetainedTeletextError::Reserve(ReserveError::InvalidOutput))
        ));
        assert!(sink.writes.lock().unwrap().bytes.is_empty());
    }
}

#[tokio::test]
async fn startup_deadline_bounds_a_source_that_stalls_before_reserve_readiness() {
    let policy = ReservePolicy::new(5_000_000, None).unwrap();
    let binding = binding(policy);
    let mut source = Source::new(flatten(&fixture(3)), usize::MAX, false);
    let mut sink = Sink::new(&source, SinkMode::Ready, 0);
    assert!(matches!(
        forward_retained_ts_with_teletext(
            &mut source,
            &mut sink,
            policy,
            &AtomicU8::new(0),
            Instant::now() + Duration::from_millis(30),
            &Notify::new(),
            &binding,
        )
        .await,
        Err(RetainedTeletextError::Reserve(ReserveError::Deadline))
    ));
    assert!(sink.writes.lock().unwrap().bytes.is_empty());
}

#[tokio::test]
async fn first_byte_deadline_is_independent_of_the_longer_reserve_startup_deadline() {
    let policy = ReservePolicy::new(5_000_000, None).unwrap();
    let binding = binding(policy);
    let mut source = Source::new(Vec::new(), usize::MAX, false);
    let mut sink = Sink::new(&source, SinkMode::Ready, 0);
    let started = Instant::now();
    assert!(matches!(
        forward_retained_ts_with_teletext(
            &mut source,
            &mut sink,
            policy,
            &AtomicU8::new(0),
            started + Duration::from_secs(1),
            &Notify::new(),
            &binding,
        )
        .await,
        Err(RetainedTeletextError::Reserve(ReserveError::Deadline))
    ));
    assert!(started.elapsed() >= binding.first_byte_timeout());
    assert!(started.elapsed() < Duration::from_millis(500));
    assert!(sink.writes.lock().unwrap().bytes.is_empty());
}

#[tokio::test]
async fn a_ready_first_read_cannot_erase_the_first_byte_deadline_after_poll_overrun() {
    struct DelayedRead {
        inner: Source,
        delay: Option<Duration>,
    }

    impl AsyncRead for DelayedRead {
        fn poll_read(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            output: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            if let Some(delay) = self.delay.take() {
                // Intentionally overrun inside one Ready poll: a timer alone
                // cannot fence bytes if its competing future was polled first.
                std::thread::sleep(delay);
            }
            Pin::new(&mut self.inner).poll_read(cx, output)
        }
    }

    let policy = ReservePolicy::new(5_000_000, None).unwrap();
    let binding = binding(policy);
    let inner = Source::new(flatten(&fixture(3)), 7, true);
    let mut sink = Sink::new(&inner, SinkMode::Ready, 0);
    let mut source = DelayedRead {
        inner,
        delay: Some(Duration::from_millis(80)),
    };
    assert!(matches!(
        forward_retained_ts_with_teletext(
            &mut source,
            &mut sink,
            policy,
            &AtomicU8::new(TERMINAL_SUCCESS),
            Instant::now() + Duration::from_secs(1),
            &Notify::new(),
            &binding,
        )
        .await,
        Err(RetainedTeletextError::Reserve(ReserveError::Deadline))
    ));
    assert_eq!(source.inner.probe.lock().unwrap().received, 7);
    assert!(sink.writes.lock().unwrap().bytes.is_empty());
}

#[tokio::test(start_paused = true)]
async fn an_empty_drained_reserve_still_bounds_waiting_for_the_next_physical_packet() {
    let policy = ReservePolicy::new(5_000_000, None).unwrap();
    let binding = binding(policy);
    let original = fixture(400);
    let input_bytes = original.len() * PACKET_BYTES;
    let mut source = Source::new(flatten(&original), usize::MAX, false);
    let mut sink = Sink::new(&source, SinkMode::Ready, 0);
    let started = Instant::now();
    let result = tokio::time::timeout(
        Duration::from_secs(11),
        forward_retained_ts_with_teletext(
            &mut source,
            &mut sink,
            policy,
            &AtomicU8::new(0),
            started + Duration::from_secs(1),
            &Notify::new(),
            &binding,
        ),
    )
    .await
    .expect("a live source stall after reserve drainage must have its own deadline");
    assert!(matches!(
        result,
        Err(RetainedTeletextError::Gate(TeletextOutputError::Deadline))
    ));
    assert_eq!(sink.writes.lock().unwrap().bytes.len(), input_bytes);
    assert_eq!(source.probe.lock().unwrap().received, input_bytes);
    assert!(started.elapsed() < Duration::from_secs(10));
    assert_eq!(source.probe.lock().unwrap().eof_polls, 0);
}

#[tokio::test]
async fn final_tail_write_and_flush_faults_cannot_be_successful_eof() {
    let policy = ReservePolicy::new(5_000_000, Some(Duration::from_millis(70))).unwrap();
    let binding = binding(policy);
    let original = fixture(4)[..fixture(3).len() + 17].to_vec();
    for (mode, kind, accepted) in [
        (
            SinkMode::WriteFault,
            ErrorKind::BrokenPipe,
            fixture(3).len() * PACKET_BYTES,
        ),
        (
            SinkMode::FlushFault,
            ErrorKind::ConnectionReset,
            original.len() * PACKET_BYTES,
        ),
    ] {
        let mut source = Source::new(flatten(&original), usize::MAX, true);
        let mut sink = Sink::new(&source, mode, fixture(3).len() * PACKET_BYTES);
        assert!(matches!(
            forward_retained_ts_with_teletext(
                &mut source,
                &mut sink,
                policy,
                &AtomicU8::new(TERMINAL_SUCCESS),
                Instant::now() + Duration::from_secs(1),
                &Notify::new(),
                &binding,
            )
            .await,
            Err(RetainedTeletextError::Write(actual)) if actual == kind
        ));
        assert_eq!(sink.writes.lock().unwrap().bytes.len(), accepted);
    }
}

#[tokio::test]
async fn blocked_final_tail_write_and_flush_expire_or_cancel_and_never_resume() {
    for cancel_operation in [false, true] {
        for mode in [SinkMode::WriteStall, SinkMode::FlushStall] {
            let policy = ReservePolicy::new(5_000_000, Some(Duration::from_millis(70))).unwrap();
            let binding = binding(policy);
            let original = fixture(4)[..fixture(3).len() + 17].to_vec();
            let mut source = Source::new(flatten(&original), usize::MAX, true);
            let mut sink = Sink::new(&source, mode, fixture(3).len() * PACKET_BYTES);
            let entered = Arc::clone(&sink.entered);
            let writes = Arc::clone(&sink.writes);
            let cancel = Notify::new();
            let terminal = AtomicU8::new(TERMINAL_SUCCESS);
            let result = {
                let operation = forward_retained_ts_with_teletext(
                    &mut source,
                    &mut sink,
                    policy,
                    &terminal,
                    Instant::now() + Duration::from_secs(1),
                    &cancel,
                    &binding,
                );
                tokio::pin!(operation);
                tokio::select! {
                    () = entered.notified() => (),
                    result = &mut operation => panic!("final tail must be pending: {result:?}"),
                }
                if cancel_operation {
                    cancel.notify_one();
                }
                tokio::time::timeout(Duration::from_secs(1), operation)
                    .await
                    .unwrap()
            };
            if cancel_operation {
                assert!(matches!(
                    result,
                    Err(RetainedTeletextError::Reserve(ReserveError::Cancelled))
                ));
            } else {
                assert!(matches!(result, Err(RetainedTeletextError::Gate(_))));
            }
            let accepted = writes.lock().unwrap().bytes.len();
            sink.mode = SinkMode::Ready;
            tokio::task::yield_now().await;
            assert_eq!(writes.lock().unwrap().bytes.len(), accepted);
            sink.write_all(b"caller-owned write").await.unwrap();
            assert_eq!(
                &writes.lock().unwrap().bytes[accepted..],
                b"caller-owned write"
            );
            assert_eq!(source.probe.lock().unwrap().eof_polls, 1);
            assert_eq!(terminal.load(Ordering::Acquire), TERMINAL_SUCCESS);
        }
    }
}

#[tokio::test]
async fn malformed_clock_partial_transport_and_partial_psi_fail_before_short_output() {
    let policy = ReservePolicy::new(5_000_000, Some(Duration::from_millis(70))).unwrap();
    let binding = binding(policy);
    for failure in 0..4 {
        let mut packets = fixture(3);
        let mut input = match failure {
            0 => {
                packets[0][0] = 0;
                flatten(&packets)
            }
            1 => {
                packets[12][11] ^= 1;
                flatten(&packets)
            }
            _ => flatten(&packets),
        };
        if failure == 2 {
            input.push(0x47);
        } else if failure == 3 {
            let mut pmt = packets[1];
            pmt[3] = 0x11;
            pmt[6] = 0xb1;
            pmt[7] = 44;
            input.extend_from_slice(&pmt);
        }
        let mut source = Source::new(input, 7, true);
        let mut sink = Sink::new(&source, SinkMode::Ready, 0);
        assert!(forward_retained_ts_with_teletext(
            &mut source,
            &mut sink,
            policy,
            &AtomicU8::new(TERMINAL_SUCCESS),
            Instant::now() + Duration::from_secs(1),
            &Notify::new(),
            &binding,
        )
        .await
        .is_err());
        assert!(sink.writes.lock().unwrap().bytes.is_empty());
    }
}

#[tokio::test(start_paused = true)]
async fn a_retained_teletext_failure_after_commitment_keeps_audio_and_video_flowing() {
    let policy = ReservePolicy::new(5_000_000, None).unwrap();
    let binding = binding(policy);
    let mut original = fixture(700);
    starve_after_first_interval(&mut original);
    let mut source = Source::new(flatten(&original), usize::MAX, true);
    let mut sink = Sink::new(&source, SinkMode::Ready, 0);
    let started = Instant::now();
    let result = forward_retained_ts_with_teletext(
        &mut source,
        &mut sink,
        policy,
        &AtomicU8::new(0),
        started + Duration::from_secs(1),
        &Notify::new(),
        &binding,
    )
    .await;
    assert!(result.is_ok(), "{result:?}");
    let writes = sink.writes.lock().unwrap();
    assert_eq!(writes.bytes.len(), original.len() * PACKET_BYTES);
    let pid = |packet: &[u8]| u16::from(packet[1] & 0x1f) << 8 | u16::from(packet[2]);
    let output: Vec<&[u8]> = writes.bytes.chunks_exact(PACKET_BYTES).collect();
    for (before, after) in original.iter().zip(&output) {
        if ![0x1fff, 3337].contains(&pid(before)) {
            assert_eq!(before.as_slice(), *after);
        }
    }
    // The full reserve already shows the multiplex is full, so the first page
    // never starts. The announced stream is withdrawn and AV keeps flowing.
    let version = |packet: &[u8]| (packet[10] >> 1) & 0x1f;
    let last_pmt = output.iter().rfind(|packet| pid(packet) == 3337).unwrap();
    let first_pmt = output.iter().find(|packet| pid(packet) == 3337).unwrap();
    assert_eq!(version(last_pmt), version(first_pmt) + 1);
}
