use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use ersatzrs_ffmpeg::teletext::TeletextBufferError;
use tokio::io::ReadBuf;

use super::super::{
    retained_stream_buffer::TERMINAL_SUCCESS,
    teletext_output::tests::{authored_set, fixture, flatten, policy as ordinary_policy},
};
use super::*;

fn binding(policy: ReservePolicy) -> TeletextOutputBinding {
    let mut config = ordinary_policy();
    config.max_buffer_packets = policy.capacity_bytes().div_ceil(PACKET_BYTES);
    config.max_hold = Duration::from_secs(9);
    TeletextOutputBinding::new(config, authored_set(), Duration::from_millis(40)).unwrap()
}

struct Remainder {
    polls: usize,
    error: Option<io::ErrorKind>,
}

impl AsyncRead for Remainder {
    fn poll_read(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        _: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        this.polls += 1;
        Poll::Ready(this.error.map_or(Ok(()), |error| Err(error.into())))
    }
}

#[derive(Default)]
struct Sink {
    bytes: Vec<u8>,
    times: Vec<Instant>,
    fail: bool,
}

impl AsyncWrite for Sink {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        if this.fail {
            return Poll::Ready(Err(io::ErrorKind::BrokenPipe.into()));
        }
        this.bytes.extend_from_slice(bytes);
        this.times.push(Instant::now());
        Poll::Ready(Ok(bytes.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

fn assert_packet_pacing(sink: &Sink, handoff: Instant, config: TeletextOutputConfig) {
    assert_eq!(sink.times.len(), sink.bytes.len() / PACKET_BYTES);
    let first_due = handoff + config.playout_delay;
    // Independent literal: one 188-byte packet at 5,000,000 bit/s is 300,800 ns.
    // Tokio may wake several sub-millisecond packets at the same timer tick;
    // each write must still fit its own physical due/lateness interval.
    for (index, observed) in sink.times.iter().enumerate() {
        let offset = Duration::from_nanos(u64::try_from(index).unwrap() * 300_800);
        let due = first_due + offset;
        assert!(
            *observed >= due,
            "packet {index} was emitted before its due time"
        );
        assert!(
            *observed <= due + config.max_lateness,
            "packet {index} was emitted too late"
        );
    }
    let first = *sink.times.first().unwrap();
    let last = *sink.times.last().unwrap();
    let extent = Duration::from_nanos(u64::try_from(sink.times.len() - 1).unwrap() * 300_800);
    let observed_extent = last.checked_duration_since(first).unwrap();
    assert!(observed_extent >= extent.saturating_sub(config.max_lateness));
    assert!(observed_extent <= extent.checked_add(config.max_lateness).unwrap());
}

#[tokio::test(start_paused = true)]
async fn expired_original_holding_age_rejects_before_reading_or_writing() {
    let policy = ReservePolicy::new(5_000_000, Some(Duration::from_secs(1))).unwrap();
    let binding = binding(policy);
    let epoch = Instant::now();
    let prefix = TeletextStartupPrefix::new(flatten(&fixture(3)), epoch, epoch).unwrap();
    tokio::time::advance(Duration::from_secs(10)).await;
    let mut source = Remainder {
        polls: 0,
        error: None,
    };
    let mut sink = Sink::default();
    let error = forward_retained_ts_with_startup(
        &mut source,
        &mut sink,
        policy,
        &AtomicU8::new(TERMINAL_SUCCESS),
        Instant::now() + Duration::from_secs(1),
        &Notify::new(),
        RetainedTeletextInput {
            binding: &binding,
            prefix: Some(prefix),
            extra_startup_bytes: 0,
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(
        error,
        RetainedTeletextError::Gate(TeletextOutputError::Buffer(TeletextBufferError::Deadline))
    ));
    assert_eq!(source.polls, 0);
    assert!(sink.bytes.is_empty());
}

#[tokio::test(start_paused = true)]
async fn late_original_first_arrival_is_not_replaced_by_a_fresh_epoch() {
    let policy = ReservePolicy::new(5_000_000, Some(Duration::from_secs(1))).unwrap();
    let binding = binding(policy);
    let epoch = Instant::now();
    tokio::time::advance(binding.first_byte_timeout()).await;
    let prefix = TeletextStartupPrefix::new(flatten(&fixture(3)), epoch, Instant::now()).unwrap();
    let mut source = Remainder {
        polls: 0,
        error: None,
    };
    let mut sink = Sink::default();
    assert!(matches!(
        forward_retained_ts_with_startup(
            &mut source,
            &mut sink,
            policy,
            &AtomicU8::new(TERMINAL_SUCCESS),
            Instant::now() + Duration::from_secs(1),
            &Notify::new(),
            RetainedTeletextInput {
                binding: &binding,
                prefix: Some(prefix),
                extra_startup_bytes: 0,
            },
        )
        .await,
        Err(RetainedTeletextError::Reserve(ReserveError::Deadline))
    ));
    assert_eq!(source.polls, 0);
    assert!(sink.bytes.is_empty());
}

#[tokio::test(start_paused = true)]
async fn prefilled_reserve_preserves_absolute_startup_deadline_and_cancellation() {
    let policy = ReservePolicy::new(5_000_000, None).unwrap();
    let binding = binding(policy);
    for cancelled in [false, true] {
        let epoch = Instant::now();
        let bytes = flatten(&fixture(400));
        assert!(bytes.len() >= policy.reserve_bytes());
        assert!(bytes.len() <= policy.capacity_bytes());
        let prefix = TeletextStartupPrefix::new(bytes, epoch, epoch).unwrap();
        let deadline = epoch + Duration::from_millis(20);
        tokio::time::advance(Duration::from_millis(30)).await;
        let cancel = Notify::new();
        if cancelled {
            cancel.notify_one();
        }
        let mut source = Remainder {
            polls: 0,
            error: None,
        };
        let mut sink = Sink::default();
        let error = forward_retained_ts_with_startup(
            &mut source,
            &mut sink,
            policy,
            &AtomicU8::new(TERMINAL_SUCCESS),
            deadline,
            &cancel,
            RetainedTeletextInput {
                binding: &binding,
                prefix: Some(prefix),
                extra_startup_bytes: 0,
            },
        )
        .await
        .unwrap_err();
        if cancelled {
            assert!(matches!(
                error,
                RetainedTeletextError::Reserve(ReserveError::Cancelled)
            ));
        } else {
            assert!(matches!(
                error,
                RetainedTeletextError::Reserve(ReserveError::Deadline)
            ));
        }
        assert_eq!(source.polls, 0);
        assert!(sink.bytes.is_empty());
    }
}

#[tokio::test(start_paused = true)]
async fn timely_prefix_uses_whole_extent_once_and_keeps_physical_packet_pacing() {
    let policy = ReservePolicy::new(5_000_000, Some(Duration::from_secs(1))).unwrap();
    let binding = binding(policy);
    let packets = fixture(50);
    let original = flatten(&packets);
    let epoch = Instant::now();
    tokio::time::advance(Duration::from_millis(3)).await;
    let prefix = TeletextStartupPrefix::new(original.clone(), epoch, Instant::now()).unwrap();
    tokio::time::advance(Duration::from_millis(80)).await;
    let handoff = Instant::now();
    let mut source = Remainder {
        polls: 0,
        error: None,
    };
    let mut sink = Sink::default();
    forward_retained_ts_with_startup(
        &mut source,
        &mut sink,
        policy,
        &AtomicU8::new(TERMINAL_SUCCESS),
        handoff + Duration::from_secs(2),
        &Notify::new(),
        RetainedTeletextInput {
            binding: &binding,
            prefix: Some(prefix),
            extra_startup_bytes: 0,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        source.polls, 1,
        "only actual EOF is read from the remainder"
    );
    assert_eq!(sink.bytes.len(), original.len());
    assert_ne!(sink.bytes, original);
    assert_packet_pacing(&sink, handoff, binding.config());
    for (before, after) in packets.iter().zip(sink.bytes.chunks_exact(PACKET_BYTES)) {
        if before[1] == 1 && before[2] == 44 {
            assert_eq!(before.as_slice(), after);
        }
    }
}

#[tokio::test(start_paused = true)]
async fn prefix_capacity_is_charged_before_gate_allocation_or_remainder_io() {
    let policy = ReservePolicy::new(5_000_000, Some(Duration::from_secs(1))).unwrap();
    let binding = binding(policy);
    let mut bytes = Vec::with_capacity(MAX_ENCODED_BUFFER_BYTES);
    bytes.extend_from_slice(&flatten(&fixture(3)));
    let now = Instant::now();
    let prefix = TeletextStartupPrefix::new(bytes, now, now).unwrap();
    let mut source = Remainder {
        polls: 0,
        error: None,
    };
    let mut sink = Sink::default();
    assert!(matches!(
        forward_retained_ts_with_startup(
            &mut source,
            &mut sink,
            policy,
            &AtomicU8::new(TERMINAL_SUCCESS),
            now + Duration::from_secs(2),
            &Notify::new(),
            RetainedTeletextInput {
                binding: &binding,
                prefix: Some(prefix),
                extra_startup_bytes: 0,
            },
        )
        .await,
        Err(RetainedTeletextError::Configuration)
    ));
    assert_eq!(source.polls, 0);
    assert!(sink.bytes.is_empty());
}

#[tokio::test(start_paused = true)]
async fn seeded_true_source_and_sink_failures_stay_terminal() {
    let policy = ReservePolicy::new(5_000_000, Some(Duration::from_secs(1))).unwrap();
    let binding = binding(policy);
    for read_failure in [true, false] {
        let now = Instant::now();
        let frames = if read_failure { 3 } else { 50 };
        let prefix = TeletextStartupPrefix::new(flatten(&fixture(frames)), now, now).unwrap();
        let mut source = Remainder {
            polls: 0,
            error: read_failure.then_some(io::ErrorKind::ConnectionReset),
        };
        let mut sink = Sink {
            fail: !read_failure,
            ..Sink::default()
        };
        let error = forward_retained_ts_with_startup(
            &mut source,
            &mut sink,
            policy,
            &AtomicU8::new(TERMINAL_SUCCESS),
            now + Duration::from_secs(2),
            &Notify::new(),
            RetainedTeletextInput {
                binding: &binding,
                prefix: Some(prefix),
                extra_startup_bytes: 0,
            },
        )
        .await
        .unwrap_err();
        if read_failure {
            assert!(matches!(
                error,
                RetainedTeletextError::Read(io::ErrorKind::ConnectionReset)
            ));
        } else {
            assert!(matches!(
                error,
                RetainedTeletextError::Write(io::ErrorKind::BrokenPipe)
            ));
        }
        assert_eq!(source.polls, 1);
        assert!(sink.bytes.is_empty());
    }
}

#[test]
fn processing_cap_reserves_full_eight_second_journal_at_generated_native_rate() {
    let policy = ReservePolicy::new(9_830_400, None).unwrap();
    let mut config = ordinary_policy();
    config.clock.muxrate_bps = 9_830_400;
    config.max_buffer_packets = policy.capacity_bytes().div_ceil(PACKET_BYTES);
    config.max_hold = Duration::from_secs(9);
    let (base, per_packet) = allocation_model(config).unwrap();
    let original = validate_policy(policy, config).unwrap();
    assert_eq!(processing_cap(policy, config, 0).unwrap(), original);
    let extra = policy.capacity_bytes().checked_add(READ_BYTES).unwrap();
    assert!(base + original * per_packet + extra > MAX_ENCODED_BUFFER_BYTES);
    let reduced = processing_cap(policy, config, extra).unwrap();
    assert!(reduced < original);
    assert!(base + reduced * per_packet + extra <= MAX_ENCODED_BUFFER_BYTES);
    assert!(reduced as u128 > u128::from(policy.muxrate_bps()) / (50 * 188 * 8));
    assert!(matches!(
        processing_cap(policy, config, usize::MAX),
        Err(RetainedTeletextError::Configuration)
    ));
    let one_interval = usize::try_from(u128::from(policy.muxrate_bps()) / (50 * 188 * 8)).unwrap();
    let insufficient = MAX_ENCODED_BUFFER_BYTES - base - one_interval * per_packet;
    assert!(matches!(
        processing_cap(policy, config, insufficient),
        Err(RetainedTeletextError::Configuration)
    ));
}

#[tokio::test(start_paused = true)]
async fn reduced_cap_preserves_complete_short_extent_and_exact_packet_pacing() {
    let policy = ReservePolicy::new(5_000_000, Some(Duration::from_secs(1))).unwrap();
    let binding = binding(policy);
    let packets = fixture(50);
    let original = flatten(&packets);
    let now = Instant::now();
    let prefix = TeletextStartupPrefix::new(original.clone(), now, now).unwrap();
    let extra = policy.capacity_bytes().checked_add(READ_BYTES).unwrap();
    let reduced = processing_cap(policy, binding.config(), extra).unwrap();
    assert!(reduced < validate_policy(policy, binding.config()).unwrap());
    let mut source = Remainder {
        polls: 0,
        error: None,
    };
    let mut sink = Sink::default();
    forward_retained_ts_with_startup(
        &mut source,
        &mut sink,
        policy,
        &AtomicU8::new(TERMINAL_SUCCESS),
        now + Duration::from_secs(2),
        &Notify::new(),
        RetainedTeletextInput {
            binding: &binding,
            prefix: Some(prefix),
            extra_startup_bytes: extra,
        },
    )
    .await
    .unwrap();
    assert_eq!(source.polls, 1);
    assert_eq!(sink.bytes.len(), original.len());
    assert_packet_pacing(&sink, now, binding.config());
    for (before, after) in packets.iter().zip(sink.bytes.chunks_exact(PACKET_BYTES)) {
        if before[1] == 1 && before[2] == 44 {
            assert_eq!(before.as_slice(), after);
        }
    }
}
