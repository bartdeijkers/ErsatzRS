//! Bounded encoded MPEG-TS reserve for retained-interval response owners.

use std::{
    collections::VecDeque,
    sync::atomic::{AtomicU8, Ordering},
    time::Duration,
};

use tokio::{
    io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _},
    sync::Notify,
    time::Instant,
};

const PACKET_BYTES: usize = 188;
const MAX_OUTPUT_BATCH_BYTES: usize = 256 * PACKET_BYTES;
const OUTPUT_BATCH_DURATION: Duration = Duration::from_millis(10);
const MAX_PACING_CATCHUP: Duration = Duration::from_millis(20);
const MAX_BUFFER_BYTES: usize = 32 * 1024 * 1024;
const RESERVE_DURATION: Duration = Duration::from_secs(8);
// Encoder/audio packet tails are not exact schedule-duration boundaries.
const EOF_TAIL_TOLERANCE: Duration = Duration::from_millis(250);
pub(super) const TERMINAL_SUCCESS: u8 = 1;
pub(super) const TERMINAL_FAILURE: u8 = 2;

#[derive(Debug, Clone, Copy, thiserror::Error)]
pub(super) enum ReserveError {
    #[error("retained MPEG-TS reserve exceeds the supported memory or rate bound")]
    UnsupportedRate,
    #[error("retained MPEG-TS startup deadline expired")]
    Deadline,
    #[error("retained MPEG-TS response was cancelled")]
    Cancelled,
    #[error("retained MPEG-TS output failed or ended before its expected remainder")]
    InvalidOutput,
    #[error("retained MPEG-TS response transport failed")]
    Transport,
}

#[derive(Clone, Copy)]
pub(super) struct ReservePolicy {
    muxrate_bps: u64,
    reserve_bytes: usize,
    capacity_bytes: usize,
    batch_bytes: usize,
    remaining: Option<Duration>,
}

impl ReservePolicy {
    pub(super) fn new(muxrate_bps: u64, remaining: Option<Duration>) -> Result<Self, ReserveError> {
        if muxrate_bps == 0
            || muxrate_bps > MAX_BUFFER_BYTES as u64
            || remaining.is_some_and(|remaining| remaining.is_zero())
        {
            return Err(ReserveError::UnsupportedRate);
        }
        let bytes = byte_count(muxrate_bps, RESERVE_DURATION)?;
        let packets = bytes.div_ceil(PACKET_BYTES as u64);
        let reserve_bytes = usize::try_from(
            packets
                .checked_mul(PACKET_BYTES as u64)
                .ok_or(ReserveError::UnsupportedRate)?,
        )
        .map_err(|_| ReserveError::UnsupportedRate)?;
        let batch_bytes = usize::try_from(
            byte_count(muxrate_bps, OUTPUT_BATCH_DURATION)?.div_ceil(PACKET_BYTES as u64)
                * PACKET_BYTES as u64,
        )
        .map_err(|_| ReserveError::UnsupportedRate)?
        .clamp(PACKET_BYTES, MAX_OUTPUT_BATCH_BYTES);
        let tail_bytes = if remaining.is_some_and(|remaining| remaining <= RESERVE_DURATION) {
            usize::try_from(byte_count(muxrate_bps, EOF_TAIL_TOLERANCE)?)
                .map_err(|_| ReserveError::UnsupportedRate)?
                + PACKET_BYTES
        } else {
            0
        };
        let capacity_bytes = reserve_bytes
            .checked_add(batch_bytes)
            .and_then(|bytes| bytes.checked_add(tail_bytes))
            .filter(|bytes| bytes.saturating_add(2 * MAX_OUTPUT_BATCH_BYTES) <= MAX_BUFFER_BYTES)
            .ok_or(ReserveError::UnsupportedRate)?;
        Ok(Self {
            muxrate_bps,
            reserve_bytes,
            capacity_bytes,
            batch_bytes,
            remaining,
        })
    }

    fn short(self) -> bool {
        self.remaining
            .is_some_and(|remaining| remaining <= RESERVE_DURATION)
    }

    fn validate_eof(self, received: u64, terminal: &AtomicU8) -> Result<(), ReserveError> {
        if terminal.load(Ordering::Acquire) != TERMINAL_SUCCESS
            || received == 0
            || !received.is_multiple_of(PACKET_BYTES as u64)
        {
            return Err(ReserveError::InvalidOutput);
        }
        if let Some(remaining) = self.remaining {
            let expected = byte_count(self.muxrate_bps, remaining)?;
            let tolerance = byte_count(self.muxrate_bps, EOF_TAIL_TOLERANCE)? + PACKET_BYTES as u64;
            if received.abs_diff(expected) > tolerance {
                return Err(ReserveError::InvalidOutput);
            }
        }
        Ok(())
    }
}

fn byte_count(rate: u64, duration: Duration) -> Result<u64, ReserveError> {
    let bits = u128::from(rate)
        .checked_mul(duration.as_nanos())
        .ok_or(ReserveError::UnsupportedRate)?;
    u64::try_from(bits.div_ceil(8_000_000_000)).map_err(|_| ReserveError::UnsupportedRate)
}

struct Pacer {
    next: Instant,
    rate: u64,
}

impl Pacer {
    fn after_write(&mut self, now: Instant, bytes: usize) {
        let nanos = ((bytes as u128) * 8_000_000_000).div_ceil(u128::from(self.rate));
        let duration = Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX));
        let scheduled = self.next + duration;
        // Ordinary wakeup jitter must not permanently reduce the mux rate.
        // A real stall may catch up at most 20 ms, never drain the reserve.
        self.next = if now.saturating_duration_since(scheduled) > MAX_PACING_CATCHUP {
            now + duration
        } else {
            scheduled
        };
    }
}

/// The caller owns the input body and drops it on error, cancelling its encoder.
/// This function owns only bounded encoded bytes, not provider/source media.
#[allow(clippy::too_many_lines)]
pub(super) async fn forward_retained_ts(
    source: &mut (impl AsyncRead + Unpin),
    writer: &mut (impl AsyncWrite + Unpin),
    policy: ReservePolicy,
    terminal: &AtomicU8,
    deadline: Instant,
    cancel: &Notify,
) -> Result<(), ReserveError> {
    let mut buffer = VecDeque::with_capacity(policy.capacity_bytes);
    let mut incoming = vec![0_u8; MAX_OUTPUT_BATCH_BYTES].into_boxed_slice();
    let mut outgoing = vec![0_u8; MAX_OUTPUT_BATCH_BYTES].into_boxed_slice();
    let mut received = 0_u64;
    let mut eof = false;
    let mut ready = false;
    let mut pacer = Pacer {
        next: Instant::now(),
        rate: policy.muxrate_bps,
    };
    loop {
        if !ready && ((!policy.short() && buffer.len() >= policy.reserve_bytes) || eof) {
            if Instant::now() >= deadline {
                return Err(ReserveError::Deadline);
            }
            if eof && !policy.short() && buffer.len() < policy.reserve_bytes {
                return Err(ReserveError::InvalidOutput);
            }
            ready = true;
            pacer.next = Instant::now();
        }
        if eof && buffer.is_empty() {
            return Ok(());
        }
        let free = policy.capacity_bytes - buffer.len();
        // A declared short remainder must reach successful EOF within this bound.
        if !ready && free == 0 {
            return Err(ReserveError::InvalidOutput);
        }
        tokio::select! {
            biased;
            () = cancel.notified() => return Err(ReserveError::Cancelled),
            () = tokio::time::sleep_until(deadline), if !ready => return Err(ReserveError::Deadline),
            () = tokio::time::sleep_until(pacer.next), if ready && buffer.len() >= PACKET_BYTES => {
                let length = buffer.len().min(policy.batch_bytes) / PACKET_BYTES * PACKET_BYTES;
                for (index, byte) in buffer.drain(..length).enumerate() { outgoing[index] = byte; }
                if outgoing[..length].chunks_exact(PACKET_BYTES).any(|packet| packet[0] != 0x47) {
                    return Err(ReserveError::InvalidOutput);
                }
                tokio::select! {
                    biased;
                    () = cancel.notified() => return Err(ReserveError::Cancelled),
                    result = writer.write_all(&outgoing[..length]) => result.map_err(|_| ReserveError::Transport)?,
                }
                pacer.after_write(Instant::now(), length);
            }
            result = source.read(&mut incoming[..free.min(MAX_OUTPUT_BATCH_BYTES)]), if !eof && free > 0 => {
                let length = result.map_err(|_| ReserveError::Transport)?;
                if length == 0 {
                    policy.validate_eof(received, terminal)?;
                    eof = true;
                } else {
                    received = received.checked_add(length as u64).ok_or(ReserveError::InvalidOutput)?;
                    buffer.extend(&incoming[..length]);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::task::Poll;

    fn packets(count: usize) -> Vec<u8> {
        let mut bytes = vec![0; count * PACKET_BYTES];
        for packet in bytes.chunks_exact_mut(PACKET_BYTES) {
            packet[0] = 0x47;
        }
        bytes
    }

    #[test]
    fn reserve_policy_has_fixed_memory_ceiling_and_never_reduces_eight_seconds() {
        let policy = ReservePolicy::new(8_000_000, None).unwrap();
        assert!(policy.reserve_bytes >= 8_000_000);
        assert!(policy.capacity_bytes <= MAX_BUFFER_BYTES);
        assert!(ReservePolicy::new((MAX_BUFFER_BYTES as u64) * 8, None).is_err());
        assert!(ReservePolicy::new(
            (MAX_BUFFER_BYTES as u64) * 8,
            Some(Duration::from_millis(1))
        )
        .is_err());
        assert!(ReservePolicy::new(0, None).is_err());
    }

    #[test]
    fn pacer_does_not_accumulate_a_catch_up_burst_after_client_stalls() {
        let start = Instant::now();
        let mut pacer = Pacer {
            next: start,
            rate: (PACKET_BYTES * 8) as u64,
        };
        pacer.after_write(start, PACKET_BYTES);
        assert_eq!(pacer.next, start + Duration::from_secs(1));
        let resumed = start + Duration::from_secs(20);
        pacer.after_write(resumed, PACKET_BYTES);
        assert_eq!(pacer.next, resumed + Duration::from_secs(1));
        pacer.after_write(resumed, PACKET_BYTES);
        assert_eq!(pacer.next, resumed + Duration::from_secs(2));
    }

    #[test]
    fn pacing_preserves_mux_rate_despite_repeated_timer_jitter() {
        let start = Instant::now();
        let mut pacer = Pacer {
            next: start,
            rate: (PACKET_BYTES * 8 * 100) as u64,
        };
        for batch in 0..100 {
            pacer.after_write(start + Duration::from_millis(batch * 10 + 1), PACKET_BYTES);
            assert_eq!(pacer.next, start + Duration::from_millis((batch + 1) * 10));
        }
    }

    #[test]
    fn near_eight_second_remainder_has_room_for_legal_codec_tail() {
        let policy = ReservePolicy::new(8_000_000, Some(Duration::from_secs(8))).unwrap();
        let received = byte_count(policy.muxrate_bps, Duration::from_millis(8_200))
            .unwrap()
            .div_ceil(PACKET_BYTES as u64)
            * PACKET_BYTES as u64;
        assert!(usize::try_from(received).unwrap() <= policy.capacity_bytes);
        policy
            .validate_eof(received, &AtomicU8::new(TERMINAL_SUCCESS))
            .unwrap();
    }

    #[tokio::test]
    async fn reserve_withholds_partial_output_and_cancellation_ends_the_wait() {
        let (mut producer, mut source) = tokio::io::duplex(4096);
        producer.write_all(&packets(7)).await.unwrap();
        let (mut writer, mut reader) = tokio::io::duplex(4096);
        let policy = ReservePolicy::new((PACKET_BYTES * 8) as u64, None).unwrap();
        let terminal = AtomicU8::new(0);
        let cancel = Notify::new();
        let operation = forward_retained_ts(
            &mut source,
            &mut writer,
            policy,
            &terminal,
            Instant::now() + Duration::from_secs(15),
            &cancel,
        );
        tokio::pin!(operation);
        assert!(matches!(futures_util::poll!(&mut operation), Poll::Pending));
        let mut byte = [0];
        let read = reader.read(&mut byte);
        tokio::pin!(read);
        assert!(matches!(futures_util::poll!(&mut read), Poll::Pending));
        cancel.notify_one();
        assert!(matches!(operation.await, Err(ReserveError::Cancelled)));
    }

    #[tokio::test]
    async fn short_successful_remainder_releases_at_eof_without_waiting_eight_seconds() {
        let bytes = packets(1);
        let mut source = bytes.as_slice();
        let (mut writer, mut reader) = tokio::io::duplex(4096);
        let policy = ReservePolicy::new(
            (PACKET_BYTES * 8_000) as u64,
            Some(Duration::from_millis(1)),
        )
        .unwrap();
        forward_retained_ts(
            &mut source,
            &mut writer,
            policy,
            &AtomicU8::new(TERMINAL_SUCCESS),
            Instant::now() + Duration::from_secs(15),
            &Notify::new(),
        )
        .await
        .unwrap();
        drop(writer);
        let mut received = Vec::new();
        reader.read_to_end(&mut received).await.unwrap();
        assert_eq!(received, bytes);
    }

    #[tokio::test]
    async fn reserve_rejects_failed_unknown_or_premature_eof_and_expired_startup() {
        for terminal in [0, TERMINAL_FAILURE, TERMINAL_SUCCESS] {
            let bytes = packets(1);
            let mut source = bytes.as_slice();
            let (mut writer, mut reader) = tokio::io::duplex(4096);
            let policy =
                ReservePolicy::new((PACKET_BYTES * 8_000) as u64, Some(Duration::from_secs(5)))
                    .unwrap();
            assert!(matches!(
                forward_retained_ts(
                    &mut source,
                    &mut writer,
                    policy,
                    &AtomicU8::new(terminal),
                    Instant::now() + Duration::from_secs(15),
                    &Notify::new()
                )
                .await,
                Err(ReserveError::InvalidOutput)
            ));
            drop(writer);
            let mut received = Vec::new();
            reader.read_to_end(&mut received).await.unwrap();
            assert!(received.is_empty());
        }
        let (_producer, mut source) = tokio::io::duplex(4096);
        let (mut writer, _reader) = tokio::io::duplex(4096);
        assert!(matches!(
            forward_retained_ts(
                &mut source,
                &mut writer,
                ReservePolicy::new(8_000_000, None).unwrap(),
                &AtomicU8::new(0),
                Instant::now(),
                &Notify::new()
            )
            .await,
            Err(ReserveError::Deadline)
        ));
    }
}
