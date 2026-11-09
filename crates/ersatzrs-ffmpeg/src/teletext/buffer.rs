//! Bounded byte retention for one host-owned final TS188 output generation.

use std::time::Duration;

const PACKET_BYTES: usize = 188;
pub(super) const MAX_RETAINED_PACKETS: usize = 32 * 1024 * 1024 / PACKET_BYTES;

/// Retention failure, without retaining transport bytes in the error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextBufferError {
    /// The retention policy has a zero or unsupported bound.
    #[error("teletext buffer configuration has invalid {field}")]
    Configuration {
        /// Invalid policy parameter's name.
        field: &'static str,
    },
    /// Complete packets and partial bytes would exceed capacity.
    #[error("teletext output exceeds its retained byte bound")]
    Capacity,
    /// A retained packet's first byte reached the maximum holding age.
    #[error("teletext retained output reached its holding deadline")]
    Deadline,
    /// The caller's monotonic elapsed time moved backwards.
    #[error("teletext buffer elapsed time moved backwards")]
    TimeOrder,
    /// The permitted prefix exceeds the complete packet window.
    #[error("teletext buffer release exceeds its complete packet prefix")]
    ReleasePrefix,
    /// The source ended inside a TS188 packet.
    #[error("teletext output ended with a partial TS188 packet")]
    PartialPacket,
    /// The source ended before retained complete packets were drained.
    #[error("teletext output ended with unreleased retained packets")]
    RetainedBytes,
    /// An operation attempted to reuse a closed output generation.
    #[error("teletext output buffer is closed")]
    Closed,
}

/// Complete packets and one partial tail retained until the owner permits release.
///
/// Capacity includes partial bytes. Each packet's age starts at its first byte,
/// including packets spanning source reads. The caller supplies elapsed time
/// from one monotonic origin and must call `check_deadline` while input is idle.
/// Every error permanently fences this generation and discards held bytes.
/// Retained transport bytes have a 32 MiB ceiling. Per-packet arrival records
/// are separately bounded by the same packet count; vector overhead, the fixed
/// partial-packet storage and released output are outside that byte ceiling.
///
/// This buffer only groups bytes into TS188-sized packets. It does not validate
/// sync, PSI, clock/rate evidence, rewrite permissions or output pacing. The host
/// must compose those gates before permitting a complete prefix to leave.
pub struct TeletextPacketBuffer {
    max_buffer_packets: usize,
    max_hold: Duration,
    packets: Vec<[u8; PACKET_BYTES]>,
    arrivals: Vec<Duration>,
    partial: [u8; PACKET_BYTES],
    partial_len: usize,
    partial_arrival: Duration,
    last_now: Option<Duration>,
    failure: Option<TeletextBufferError>,
    closed: bool,
}

impl TeletextPacketBuffer {
    /// Start an empty generation with an explicit byte and wall-clock policy.
    /// # Errors
    /// Packet capacity must fit the 32 MiB retained-transport-byte ceiling and
    /// be nonzero; holding duration must also be nonzero. Processing windows
    /// and signalling retain their independent, smaller limits.
    pub fn new(max_buffer_packets: usize, max_hold: Duration) -> Result<Self, TeletextBufferError> {
        if !(1..=MAX_RETAINED_PACKETS).contains(&max_buffer_packets) {
            return Err(TeletextBufferError::Configuration {
                field: "max_buffer_packets",
            });
        }
        if max_hold.is_zero() {
            return Err(TeletextBufferError::Configuration { field: "max_hold" });
        }
        Ok(Self {
            max_buffer_packets,
            max_hold,
            packets: Vec::with_capacity(max_buffer_packets),
            arrivals: Vec::with_capacity(max_buffer_packets),
            partial: [0; PACKET_BYTES],
            partial_len: 0,
            partial_arrival: Duration::ZERO,
            last_now: None,
            failure: None,
            closed: false,
        })
    }

    /// Retain arbitrary source fragments without admitting their transport data.
    ///
    /// The whole append is checked against capacity before copying any bytes.
    /// Empty input still checks monotonic time and the current holding deadline.
    /// # Errors
    /// Returns a sticky capacity, deadline, time-order or closure failure.
    pub fn push(&mut self, mut bytes: &[u8], now: Duration) -> Result<(), TeletextBufferError> {
        self.check_deadline(now)?;
        let available = self.max_buffer_packets * PACKET_BYTES - self.held_bytes();
        if bytes.len() > available {
            return self.fail(TeletextBufferError::Capacity);
        }
        while !bytes.is_empty() {
            if self.partial_len == 0 {
                self.partial_arrival = now;
            }
            let count = bytes.len().min(PACKET_BYTES - self.partial_len);
            self.partial[self.partial_len..self.partial_len + count]
                .copy_from_slice(&bytes[..count]);
            self.partial_len += count;
            bytes = &bytes[count..];
            if self.partial_len == PACKET_BYTES {
                self.packets.push(self.partial);
                self.arrivals.push(self.partial_arrival);
                self.partial.fill(0);
                self.partial_len = 0;
            }
        }
        Ok(())
    }

    /// Borrow complete retained packets in physical source order.
    ///
    /// This is an inspection window, not admission or release permission. It
    /// excludes the partial tail and does not advance the caller's clock. After
    /// successful EOF the window is empty. Check deadlines before processing.
    /// # Errors
    /// Returns the first retained generation failure.
    pub fn window(&self) -> Result<&[[u8; PACKET_BYTES]], TeletextBufferError> {
        self.failure.map_or(Ok(self.packets.as_slice()), Err)
    }

    /// Number of logically retained bytes, including the partial packet tail.
    ///
    /// Zero after failure or successful closure; this does not imply admission.
    #[must_use]
    pub fn held_bytes(&self) -> usize {
        self.packets.len() * PACKET_BYTES + self.partial_len
    }

    /// Query the oldest retained packet or partial tail's first-byte arrival.
    ///
    /// The output owner can carry this age across a prefix release and sink
    /// delivery. This query does not advance time, expire bytes or admit output.
    /// # Errors
    /// Returns the first retained generation failure.
    pub(super) fn first_arrival(&self) -> Result<Option<Duration>, TeletextBufferError> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        Ok(self
            .arrivals
            .first()
            .copied()
            .or_else(|| (self.partial_len != 0).then_some(self.partial_arrival)))
    }

    /// Move only the explicitly permitted complete prefix out of retention.
    ///
    /// Deadline and prefix bounds are checked before releasing any packet. Ages
    /// of remaining packets and the partial tail retain their first timestamps.
    /// The owner remains responsible for validation, rewriting and sink delivery.
    /// # Errors
    /// Returns the sticky generation failure, including an invalid prefix length.
    pub fn release(
        &mut self,
        packet_count: usize,
        now: Duration,
    ) -> Result<Vec<[u8; PACKET_BYTES]>, TeletextBufferError> {
        self.check_deadline(now)?;
        if packet_count > self.packets.len() {
            return self.fail(TeletextBufferError::ReleasePrefix);
        }
        self.arrivals.drain(..packet_count);
        Ok(self.packets.drain(..packet_count).collect())
    }

    /// Check retained output age even when the source supplies no new bytes.
    ///
    /// Age equal to the configured maximum expires. Every timed operation uses
    /// this check, so a late release cannot rescue expired bytes.
    /// # Errors
    /// Returns a sticky deadline, backwards-time or closure failure.
    pub fn check_deadline(&mut self, now: Duration) -> Result<(), TeletextBufferError> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        if self.closed {
            return self.fail(TeletextBufferError::Closed);
        }
        if self.last_now.is_some_and(|previous| now < previous) {
            return self.fail(TeletextBufferError::TimeOrder);
        }
        self.last_now = Some(now);
        let first_arrival = self.first_arrival()?;
        if let Some(arrival) = first_arrival {
            let Some(age) = now.checked_sub(arrival) else {
                return self.fail(TeletextBufferError::TimeOrder);
            };
            if age >= self.max_hold {
                return self.fail(TeletextBufferError::Deadline);
            }
        }
        Ok(())
    }

    /// Close at source EOF only after the owner drained all complete packets.
    ///
    /// This operation releases nothing. Successful closure is idempotent; no
    /// subsequent input or release can reopen the generation.
    /// # Errors
    /// Returns a sticky deadline, partial-packet or undrained-output failure.
    pub fn finish(&mut self, now: Duration) -> Result<(), TeletextBufferError> {
        if self.closed && self.failure.is_none() {
            if self.last_now.is_some_and(|previous| now < previous) {
                return self.fail(TeletextBufferError::TimeOrder);
            }
            self.last_now = Some(now);
            return Ok(());
        }
        self.check_deadline(now)?;
        self.closed = true;
        if self.partial_len != 0 {
            return self.fail(TeletextBufferError::PartialPacket);
        }
        if !self.packets.is_empty() {
            return self.fail(TeletextBufferError::RetainedBytes);
        }
        Ok(())
    }

    /// Move out every retained complete packet and the partial tail.
    ///
    /// None after a retention failure or closure, whose bytes are discarded.
    pub(super) fn take_retained(&mut self) -> Option<(Vec<[u8; PACKET_BYTES]>, Vec<u8>)> {
        if self.failure.is_some() || self.closed {
            return None;
        }
        self.arrivals.clear();
        let partial = self.partial[..self.partial_len].to_vec();
        self.partial.fill(0);
        self.partial_len = 0;
        Some((std::mem::take(&mut self.packets), partial))
    }

    fn fail<T>(&mut self, error: TeletextBufferError) -> Result<T, TeletextBufferError> {
        self.failure = Some(error);
        self.packets.clear();
        self.arrivals.clear();
        self.partial.fill(0);
        self.partial_len = 0;
        self.partial_arrival = Duration::ZERO;
        Err(error)
    }
}
