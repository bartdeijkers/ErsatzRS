//! One bounded reserve and physical packet clock for retained authored MPEG-TS.

use std::{io::ErrorKind, mem::size_of, sync::atomic::AtomicU8, time::Duration};

use ersatzrs_ffmpeg::teletext::{
    TeletextOutputConfig, TeletextOutputError, TeletextOutputGate, TeletextOutputSchedule,
};
use futures_util::FutureExt as _;
use tokio::{
    io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _},
    sync::Notify,
    time::Instant,
};

use super::{
    retained_stream_buffer::{ReserveError, ReservePolicy},
    teletext_binding::TeletextOutputBinding,
};

const PACKET_BYTES: usize = 188;
const READ_BYTES: usize = 256 * PACKET_BYTES;
const MAX_PROCESSING_PACKETS: usize = 32_768;
const MAX_ENCODED_BUFFER_BYTES: usize = 32 * 1024 * 1024;
// Requested allocation payload, excluding allocator overhead/RSS. Account for
// packet/arrival vectors, the reader, bounded section/PES scratch, and concurrent
// prepared/release/PMT vectors plus installer index/group temporaries. Authored
// pages and continuing PSI ownership retain their separate component bounds.
const FIXED_BYTES: usize = 49_152 + 65_536;

/// Content-free terminal result of one borrowed retained-output epoch.
#[derive(Debug, thiserror::Error)]
pub(super) enum RetainedTeletextError {
    #[error(transparent)]
    Reserve(#[from] ReserveError),
    #[error(transparent)]
    Gate(#[from] TeletextOutputError),
    #[error("retained teletext source read failed ({0:?})")]
    Read(ErrorKind),
    #[error("retained teletext sink write failed ({0:?})")]
    Write(ErrorKind),
    #[error("retained teletext policies are incompatible")]
    Configuration,
}

struct PendingWindow {
    schedule: TeletextOutputSchedule,
    written: usize,
    final_window: bool,
}

struct ForwardState {
    gate: TeletextOutputGate,
    config: TeletextOutputConfig,
    policy: ReservePolicy,
    epoch: Instant,
    first_byte_deadline: Instant,
    delivery_start: Option<Duration>,
    received: u64,
    emitted: u64,
    eof: bool,
    started: bool,
    finished: bool,
    intervals: usize,
    processing_packets: usize,
    pending: Option<PendingWindow>,
}

impl ForwardState {
    fn new(
        policy: ReservePolicy,
        binding: &TeletextOutputBinding,
    ) -> Result<Self, RetainedTeletextError> {
        let config = binding.config();
        let processing_packets = validate_policy(policy, config)?;
        let epoch = Instant::now();
        Ok(Self {
            gate: binding.new_gate(true)?,
            config,
            policy,
            epoch,
            first_byte_deadline: absolute(epoch, binding.first_byte_timeout())?,
            delivery_start: None,
            received: 0,
            emitted: 0,
            eof: false,
            started: false,
            finished: false,
            intervals: 1,
            processing_packets,
            pending: None,
        })
    }

    fn held_bytes(&self) -> Result<usize, RetainedTeletextError> {
        Ok(self.config.max_buffer_packets * PACKET_BYTES - self.gate.remaining_capacity()?)
    }

    fn free_bytes(&self) -> Result<usize, RetainedTeletextError> {
        self.policy
            .capacity_bytes()
            .checked_sub(self.held_bytes()?)
            .ok_or(RetainedTeletextError::Configuration)
    }

    fn advance(&mut self, startup: Instant) -> Result<(), RetainedTeletextError> {
        if self.finished {
            return Ok(());
        }
        self.gate.check_deadline(self.epoch.elapsed())?;
        if !self.started
            && ((!self.policy.short()
                && self.held_bytes()? >= self.policy.reserve_bytes()
                && self.gate.source_ready()?)
                || self.eof)
        {
            if Instant::now() >= startup {
                return Err(ReserveError::Deadline.into());
            }
            if self.eof && !self.policy.short() && self.held_bytes()? < self.policy.reserve_bytes()
            {
                return Err(ReserveError::InvalidOutput.into());
            }
            let delivery_start = self.epoch.elapsed();
            self.gate.start_delivery(delivery_start)?;
            self.delivery_start = Some(delivery_start);
            self.started = true;
        }
        if !self.started || self.pending.is_some() || self.finished {
            return Ok(());
        }
        loop {
            if let Some(end) = self.gate.window_end(self.intervals)? {
                let count = usize::try_from(
                    end.checked_sub(self.emitted)
                        .ok_or(TeletextOutputError::Boundary)?,
                )
                .map_err(|_| TeletextOutputError::Arithmetic)?;
                if count > self.processing_packets {
                    return Err(ReserveError::InvalidOutput.into());
                }
                if count <= self.gate.retained_packets()? {
                    if self.gate.prepare_window(count, self.epoch.elapsed())? {
                        self.set_pending(false)?;
                        return Ok(());
                    }
                    self.intervals = self
                        .intervals
                        .checked_add(1)
                        .ok_or(TeletextOutputError::Arithmetic)?;
                    continue;
                }
            }
            if self.eof {
                if self.gate.retained_packets()? > self.processing_packets {
                    return Err(ReserveError::InvalidOutput.into());
                }
                if self.gate.prepare_final_window(self.epoch.elapsed())? {
                    self.set_pending(true)?;
                } else {
                    self.gate.finish(self.epoch.elapsed())?;
                    self.finished = true;
                }
            }
            return Ok(());
        }
    }

    fn set_pending(&mut self, final_window: bool) -> Result<(), RetainedTeletextError> {
        self.pending = Some(PendingWindow {
            schedule: self.gate.pending_schedule()?,
            written: 0,
            final_window,
        });
        self.intervals = 1;
        Ok(())
    }

    fn acknowledge(&mut self) -> Result<(), RetainedTeletextError> {
        let pending = self.pending.take().ok_or(TeletextOutputError::Pending)?;
        self.emitted = self
            .emitted
            .checked_add(
                u64::try_from(pending.schedule.packet_count())
                    .map_err(|_| TeletextOutputError::Arithmetic)?,
            )
            .ok_or(TeletextOutputError::Arithmetic)?;
        self.gate
            .acknowledge_window(pending.schedule, self.epoch.elapsed())?;
        if pending.final_window {
            self.gate.finish(self.epoch.elapsed())?;
            self.finished = true;
        }
        Ok(())
    }

    fn received(&mut self, bytes: &[u8], terminal: &AtomicU8) -> Result<(), RetainedTeletextError> {
        if bytes.is_empty() {
            self.policy.validate_eof(self.received, terminal)?;
            self.eof = true;
        } else {
            self.received = self
                .received
                .checked_add(u64::try_from(bytes.len()).map_err(|_| ReserveError::InvalidOutput)?)
                .ok_or(ReserveError::InvalidOutput)?;
            self.gate.push(bytes, self.epoch.elapsed())?;
        }
        Ok(())
    }

    fn wait_deadline(&self, startup: Instant) -> Result<Option<Instant>, RetainedTeletextError> {
        let mut deadline = self
            .gate
            .next_deadline()?
            .map(|elapsed| absolute(self.epoch, elapsed))
            .transpose()?;
        if self.started && self.pending.is_none() {
            let nanos = u128::from(self.emitted) * PACKET_BYTES as u128 * 8_000_000_000
                / u128::from(self.policy.muxrate_bps());
            let expiry = self
                .delivery_start
                .ok_or(RetainedTeletextError::Configuration)?
                .checked_add(self.config.playout_delay)
                .and_then(|due| due.checked_add(Duration::from_nanos(u64::try_from(nanos).ok()?)))
                .and_then(|due| due.checked_add(self.config.max_lateness))
                .and_then(|due| due.checked_add(Duration::from_nanos(1)))
                .ok_or(TeletextOutputError::Arithmetic)?;
            let expiry = absolute(self.epoch, expiry)?;
            deadline = Some(deadline.map_or(expiry, |deadline| deadline.min(expiry)));
        }
        if !self.started {
            deadline = Some(deadline.map_or(startup, |deadline| deadline.min(startup)));
        }
        if self.received == 0 {
            deadline = Some(deadline.map_or(self.first_byte_deadline, |deadline| {
                deadline.min(self.first_byte_deadline)
            }));
        }
        Ok(deadline)
    }
}

pub(super) fn validate_policy(
    policy: ReservePolicy,
    config: TeletextOutputConfig,
) -> Result<usize, RetainedTeletextError> {
    let base = config
        .max_buffer_packets
        .checked_mul(PACKET_BYTES + size_of::<Duration>())
        .and_then(|bytes| bytes.checked_add(FIXED_BYTES))
        .ok_or(RetainedTeletextError::Configuration)?;
    let packet_index = size_of::<(usize, [u8; PACKET_BYTES])>();
    let per_processing_packet = PACKET_BYTES
        .checked_add(17 * size_of::<usize>())
        .and_then(|bytes| bytes.checked_add(2 * packet_index))
        .and_then(|bytes| bytes.checked_add(packet_index.max(3 * size_of::<usize>())))
        .ok_or(RetainedTeletextError::Configuration)?;
    let processing_packets = MAX_ENCODED_BUFFER_BYTES
        .checked_sub(base)
        .ok_or(RetainedTeletextError::Configuration)?
        / per_processing_packet;
    let processing_packets = processing_packets.min(MAX_PROCESSING_PACKETS);
    let span = (policy.capacity_bytes() as u128 * 8_000_000_000)
        .div_ceil(u128::from(policy.muxrate_bps()));
    let minimum_hold = Duration::from_nanos(
        u64::try_from(span).map_err(|_| RetainedTeletextError::Configuration)?,
    )
    .checked_add(config.playout_delay)
    .and_then(|hold| hold.checked_add(config.max_lateness))
    .and_then(|hold| hold.checked_add(Duration::from_millis(20)))
    .ok_or(RetainedTeletextError::Configuration)?;
    if config.clock.muxrate_bps != policy.muxrate_bps()
        || config.max_buffer_packets < policy.capacity_bytes().div_ceil(PACKET_BYTES)
        || processing_packets as u128
            <= u128::from(policy.muxrate_bps()) / (50 * PACKET_BYTES as u128 * 8)
        || config.max_hold <= minimum_hold
        || Instant::now().checked_add(config.max_hold).is_none()
    {
        return Err(RetainedTeletextError::Configuration);
    }
    Ok(processing_packets)
}

/// Forward one reserve through a single gate without reopening its source.
///
/// The source is read between individual physical packet due times. Both raw
/// and prepared packets remain charged to the gate's shared capacity. Actual
/// arrivals retain their ages across startup; EOF is validated once, and all
/// prepared writes and their flush precede the exact window acknowledgement.
/// Cancellation drops pending borrowed I/O; process cleanup stays with caller.
pub(super) async fn forward_retained_ts_with_teletext(
    source: &mut (impl AsyncRead + Unpin),
    writer: &mut (impl AsyncWrite + Unpin),
    policy: ReservePolicy,
    terminal: &AtomicU8,
    startup: Instant,
    cancel: &Notify,
    binding: &TeletextOutputBinding,
) -> Result<(), RetainedTeletextError> {
    let mut state = ForwardState::new(policy, binding)?;
    let mut incoming = vec![0_u8; READ_BYTES].into_boxed_slice();
    loop {
        if cancel.notified().now_or_never().is_some() {
            return Err(ReserveError::Cancelled.into());
        }
        state.advance(startup)?;
        if state.finished {
            return Ok(());
        }
        let deadline = state.wait_deadline(startup)?;
        let free = state.free_bytes()?.min(incoming.len());
        if !state.started && free == 0 {
            return Err(ReserveError::InvalidOutput.into());
        }
        if state
            .pending
            .as_ref()
            .is_some_and(|pending| pending.written == pending.schedule.packet_count())
        {
            flush_window(writer, &mut state, cancel).await?;
            continue;
        }
        let due = state
            .pending
            .as_ref()
            .map(|pending| pending.schedule.packet_due(pending.written))
            .transpose()?
            .map(|elapsed| absolute(state.epoch, elapsed))
            .transpose()?;
        tokio::select! {
            biased;
            () = cancel.notified() => return Err(ReserveError::Cancelled.into()),
            () = wait_until(deadline) => return Err(if state.started {
                TeletextOutputError::Deadline.into()
            } else { ReserveError::Deadline.into() }),
            () = wait_until(due) => write_packet(writer, &mut state, cancel).await?,
            result = source.read(&mut incoming[..free]), if !state.eof && free > 0 => {
                let length = result.map_err(|error| RetainedTeletextError::Read(error.kind()))?;
                if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                    return Err(if state.started {
                        TeletextOutputError::Deadline.into()
                    } else { ReserveError::Deadline.into() });
                }
                state.received(&incoming[..length], terminal)?;
            }
        }
    }
}

async fn wait_until(deadline: Option<Instant>) {
    if let Some(deadline) = deadline {
        tokio::time::sleep_until(deadline).await;
    } else {
        std::future::pending::<()>().await;
    }
}

async fn write_packet(
    writer: &mut (impl AsyncWrite + Unpin),
    state: &mut ForwardState,
    cancel: &Notify,
) -> Result<(), RetainedTeletextError> {
    let pending = state.pending.as_ref().ok_or(TeletextOutputError::Pending)?;
    let due = pending.schedule.packet_due(pending.written)?;
    let deadline = delivery_deadline(state, due)?;
    let packet = state.gate.pending_packets(state.epoch.elapsed())?[pending.written];
    tokio::select! {
        biased;
        () = cancel.notified() => return Err(ReserveError::Cancelled.into()),
        () = tokio::time::sleep_until(deadline) => return Err(TeletextOutputError::Deadline.into()),
        result = writer.write_all(&packet) => result.map_err(|error| RetainedTeletextError::Write(error.kind()))?,
    }
    if Instant::now() >= deadline {
        return Err(TeletextOutputError::Deadline.into());
    }
    state.gate.check_deadline(state.epoch.elapsed())?;
    state
        .pending
        .as_mut()
        .ok_or(TeletextOutputError::Pending)?
        .written += 1;
    Ok(())
}

async fn flush_window(
    writer: &mut (impl AsyncWrite + Unpin),
    state: &mut ForwardState,
    cancel: &Notify,
) -> Result<(), RetainedTeletextError> {
    let due = state
        .pending
        .as_ref()
        .ok_or(TeletextOutputError::Pending)?
        .schedule
        .last_due();
    let deadline = delivery_deadline(state, due)?;
    tokio::select! {
        biased;
        () = cancel.notified() => return Err(ReserveError::Cancelled.into()),
        () = tokio::time::sleep_until(deadline) => return Err(TeletextOutputError::Deadline.into()),
        result = writer.flush() => result.map_err(|error| RetainedTeletextError::Write(error.kind()))?,
    }
    if Instant::now() >= deadline {
        return Err(TeletextOutputError::Deadline.into());
    }
    state.acknowledge()
}

fn delivery_deadline(
    state: &ForwardState,
    due: Duration,
) -> Result<Instant, RetainedTeletextError> {
    let packet = due
        .checked_add(state.config.max_lateness)
        .and_then(|elapsed| elapsed.checked_add(Duration::from_nanos(1)))
        .ok_or(TeletextOutputError::Arithmetic)?;
    let elapsed = state
        .gate
        .next_deadline()?
        .map_or(packet, |holding| holding.min(packet));
    absolute(state.epoch, elapsed)
}

fn absolute(epoch: Instant, elapsed: Duration) -> Result<Instant, RetainedTeletextError> {
    epoch
        .checked_add(elapsed)
        .ok_or(RetainedTeletextError::Configuration)
}

#[cfg(test)]
#[path = "teletext_retained_output_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "teletext_retained_runtime_tests.rs"]
pub(super) mod runtime_tests;
