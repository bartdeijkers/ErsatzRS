//! Bounded, paced forwarding of one host-owned encoded TS188 epoch.

use std::{future::Future, io::ErrorKind, time::Duration};

use ersatzrs_ffmpeg::teletext::{
    EncodedTeletextPageSet, TeletextBufferError, TeletextOutputConfig, TeletextOutputError,
    TeletextOutputGate, TeletextOutputSchedule,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    time::{timeout_at, Instant},
};

use super::{
    teletext_binding::{
        control::PumpControl, TeletextOutputBinding, TeletextOutputContent, TeletextOutputControl,
    },
    teletext_startup::TeletextStartupPrefix,
};

const READ_BYTES: usize = 256 * 188;

// Read the same borrowed stdout while delivery waits, without another task or
// resetting arrival ages. Pending output and raw lookahead share gate capacity.
struct SourcePrefetch<'a, R> {
    source: &'a mut R,
    scratch: Box<[u8]>,
    ended: bool,
    capacity_bytes: usize,
    max_held_bytes: usize,
}

impl<'a, R: AsyncRead + Unpin> SourcePrefetch<'a, R> {
    fn new(source: &'a mut R, config: TeletextOutputConfig) -> Result<Self, TeletextPumpError> {
        let capacity_bytes = config.max_buffer_packets * 188;
        // A fast source must not fill more physical lookahead than can be
        // delivered within its original holding age, including startup delay
        // and the allowed sink lateness. A policy with no spare age disables
        // opportunistic reads; ordinary demand reads retain their old behavior.
        let age = config
            .max_hold
            .checked_sub(config.playout_delay)
            .and_then(|age| age.checked_sub(config.max_lateness))
            .unwrap_or(Duration::ZERO);
        let packets = age
            .as_nanos()
            .checked_mul(u128::from(config.clock.muxrate_bps))
            .ok_or(TeletextPumpError::Configuration)?
            / (188 * 8_000_000_000_u128);
        let max_held_bytes = usize::try_from(packets.min(config.max_buffer_packets as u128))
            .map_err(|_| TeletextPumpError::Configuration)?
            * 188;
        Ok(Self {
            source,
            scratch: vec![0_u8; READ_BYTES].into_boxed_slice(),
            ended: false,
            capacity_bytes,
            max_held_bytes,
        })
    }

    async fn during_delivery<T, F>(
        &mut self,
        gate: &mut TeletextOutputGate,
        epoch: Instant,
        delivery: F,
    ) -> Result<T, TeletextPumpError>
    where
        F: Future<Output = Result<T, TeletextPumpError>>,
    {
        tokio::pin!(delivery);
        loop {
            let free = gate.remaining_capacity()?;
            let held = self.capacity_bytes - free;
            let available = free
                .min(self.max_held_bytes.saturating_sub(held))
                .min(self.scratch.len());
            if self.ended || available == 0 {
                return delivery.await;
            }
            tokio::select! {
                biased;
                result = &mut delivery => return result,
                read = self.source.read(&mut self.scratch[..available]) => {
                    let read = read.map_err(|error| TeletextPumpError::Read(error.kind()))?;
                    if read == 0 {
                        // Seal only after the current immutable window is
                        // flushed/acknowledged, then drain the ordinary tail.
                        self.ended = true;
                    } else {
                        gate.push(&self.scratch[..read], epoch.elapsed())?;
                    }
                }
            }
        }
    }
}

/// Content-free terminal failure of the encoded-output forwarding epoch.
#[derive(Debug, thiserror::Error)]
pub enum TeletextPumpError {
    /// Transport ownership, timing, framing or authored insertion failed.
    #[error(transparent)]
    Gate(#[from] TeletextOutputError),
    /// The existing encoded-output source failed; no path or stderr is retained.
    #[error("teletext output source read failed ({0:?})")]
    Read(ErrorKind),
    /// The existing response sink failed, including a partial packet write.
    #[error("teletext output sink write failed ({0:?})")]
    Write(ErrorKind),
    /// The output owner did not supply its first encoded byte in time.
    #[error("teletext output source did not start before its deadline")]
    FirstByteTimeout,
    /// The producer closed stdout before supplying any encoded output byte.
    #[error("teletext output source ended without encoded output")]
    EmptyOutput,
    /// A caller-supplied asynchronous timing policy cannot be represented.
    #[error("teletext output asynchronous configuration is invalid")]
    Configuration,
}

/// Forward one admitted final MPEG-TS stdout into its existing response sink.
///
/// The caller owns source/process identity, final-output-role admission and
/// cancellation. Dropping this future drops held data and any pending I/O;
/// the caller must terminate/reap the producer and end the sink. A failure
/// cannot be retried within this output epoch or converted to successful EOF.
/// No source is spawned/reopened and no detached forwarding task is created.
///
/// Complete physical windows are preflighted before writes. Each packet is
/// paced against its absolute source-position schedule, with bounded source
/// waiting and sink writes. Flush precedes epoch-bound window acknowledgement.
/// Sink acknowledgement proves acceptance by this writer, not client receipt.
/// At actual EOF, complete transport packets in the fractional final interval
/// retain their original positions and pacing. A complete authored cycle must
/// have been flushed and acknowledged. Producer handover remains separate.
/// # Errors
/// Returns terminal configuration, source/sink, admission or deadline failure.
pub async fn forward_teletext_output<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    source: &mut R,
    sink: &mut W,
    config: TeletextOutputConfig,
    set: EncodedTeletextPageSet,
    first_byte_timeout: Duration,
) -> Result<u64, TeletextPumpError> {
    forward_teletext_output_with_content(
        source,
        sink,
        config,
        TeletextOutputContent::Base(set),
        first_byte_timeout,
    )
    .await
}

/// Forward a complete typed service through the same paced output algorithm.
/// This grants no publication or receiver readiness authority.
/// # Errors
/// Preserves terminal source, sink, clock, capacity and complete-cycle failures.
pub async fn forward_teletext_output_with_content<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    source: &mut R,
    sink: &mut W,
    config: TeletextOutputConfig,
    content: TeletextOutputContent,
    first_byte_timeout: Duration,
) -> Result<u64, TeletextPumpError> {
    forward_teletext_output_with_control(source, sink, config, content, first_byte_timeout, None)
        .await
}

/// Forward with optional latest trusted engine proposals. Normal paths remain
/// unbound; sink receipts establish only this output epoch's accepted writes.
/// # Errors
/// Actual source/sink/transport failures remain terminal. Invalid replacement
/// proposals report bounded status while the existing output continues.
pub async fn forward_teletext_output_with_control<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    source: &mut R,
    sink: &mut W,
    config: TeletextOutputConfig,
    content: TeletextOutputContent,
    first_byte_timeout: Duration,
    control: Option<TeletextOutputControl>,
) -> Result<u64, TeletextPumpError> {
    forward_output(
        source,
        sink,
        ForwardOptions {
            config,
            content,
            first_byte_timeout,
            control,
            seed: None,
        },
    )
    .await
}

/// Continue the same source after bounded discovery without renewing arrival ages.
/// The retained raw prefix starts at physical packet zero and is counted exactly
/// once. Its source epoch and oldest arrival remain authoritative for this pump.
pub(super) async fn forward_teletext_output_seeded<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    source: &mut R,
    sink: &mut W,
    binding: &TeletextOutputBinding,
    seed: TeletextStartupPrefix,
) -> Result<u64, TeletextPumpError> {
    forward_output(
        source,
        sink,
        ForwardOptions {
            config: binding.config(),
            content: binding.content().clone(),
            first_byte_timeout: binding.first_byte_timeout(),
            control: binding.cloned_control(),
            seed: Some(seed),
        },
    )
    .await
}

struct ForwardOptions {
    config: TeletextOutputConfig,
    content: TeletextOutputContent,
    first_byte_timeout: Duration,
    control: Option<TeletextOutputControl>,
    seed: Option<TeletextStartupPrefix>,
}

async fn forward_output<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    source: &mut R,
    sink: &mut W,
    options: ForwardOptions,
) -> Result<u64, TeletextPumpError> {
    let ForwardOptions {
        config,
        content,
        first_byte_timeout,
        control,
        seed,
    } = options;
    let mut progress = ForwardProgress {
        emitted: 0,
        intervals: 1,
        control: control.map(PumpControl::new),
    };
    let epoch = seed
        .as_ref()
        .map_or_else(Instant::now, TeletextStartupPrefix::epoch);
    if first_byte_timeout.is_zero() {
        return Err(TeletextPumpError::Configuration);
    }
    let first_deadline = absolute(epoch, first_byte_timeout)?;
    absolute(epoch, config.max_hold)?;
    let mut gate = content.new_gate(config, false)?;
    let mut started = seed_gate(&mut gate, seed, first_byte_timeout, epoch)?;
    let mut prefetch = SourcePrefetch::new(source, config)?;
    loop {
        gate.check_deadline(epoch.elapsed())?;
        if progress
            .forward_ready(sink, &mut gate, config, epoch, &mut prefetch)
            .await?
        {
            continue;
        }
        let available = gate.remaining_capacity()?.min(prefetch.scratch.len());
        if available == 0 {
            return Err(TeletextOutputError::Buffer(TeletextBufferError::Capacity).into());
        }
        let idle_deadline = if started {
            absolute(Instant::now(), config.max_hold)?
        } else {
            first_deadline
        };
        let deadline = gate
            .next_deadline()?
            .map(|elapsed| absolute(epoch, elapsed))
            .transpose()?
            .map_or(idle_deadline, |holding| holding.min(idle_deadline));
        let read = if prefetch.ended {
            0
        } else {
            timeout_at(
                deadline,
                prefetch.source.read(&mut prefetch.scratch[..available]),
            )
            .await
            .map_err(|_| source_deadline_error(started))?
            .map_err(|error| TeletextPumpError::Read(error.kind()))?
        };
        if Instant::now() >= deadline {
            return Err(source_deadline_error(started));
        }
        if read == 0 {
            prefetch.ended = true;
            if let Some(control) = &mut progress.control {
                control.source_ended();
            }
            if !started {
                return Err(TeletextPumpError::EmptyOutput);
            }
            progress
                .forward_final(sink, &mut gate, config, epoch, &mut prefetch)
                .await?;
            gate.finish(epoch.elapsed())?;
            return progress
                .emitted
                .checked_mul(188)
                .ok_or_else(|| TeletextOutputError::Arithmetic.into());
        }
        started = true;
        gate.push(&prefetch.scratch[..read], epoch.elapsed())?;
    }
}

fn seed_gate(
    gate: &mut TeletextOutputGate,
    seed: Option<TeletextStartupPrefix>,
    first_byte_timeout: Duration,
    epoch: Instant,
) -> Result<bool, TeletextPumpError> {
    let Some(seed) = seed else {
        return Ok(false);
    };
    if seed.bytes().is_empty() {
        return Err(TeletextPumpError::Configuration);
    }
    if seed.first_arrival() >= first_byte_timeout {
        return Err(TeletextPumpError::FirstByteTimeout);
    }
    gate.push(seed.bytes(), seed.first_arrival())?;
    drop(seed);
    gate.check_deadline(epoch.elapsed())?;
    Ok(true)
}

struct ForwardProgress {
    emitted: u64,
    intervals: usize,
    control: Option<PumpControl>,
}
impl ForwardProgress {
    async fn forward_final<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
        &mut self,
        sink: &mut W,
        gate: &mut TeletextOutputGate,
        config: TeletextOutputConfig,
        epoch: Instant,
        prefetch: &mut SourcePrefetch<'_, R>,
    ) -> Result<(), TeletextPumpError> {
        if gate.prepare_final_window(epoch.elapsed())? {
            let schedule = gate.pending_schedule()?;
            let count = u64::try_from(schedule.packet_count())
                .map_err(|_| TeletextOutputError::Arithmetic)?;
            if let Some(control) = &self.control {
                control.trace_window_prepared(gate, &schedule, epoch.elapsed());
            }
            write_window(
                sink,
                gate,
                &schedule,
                config,
                epoch,
                prefetch,
                self.control.as_ref(),
            )
            .await?;
            let first_packet = schedule.first_packet();
            let receipt = gate.acknowledge_window_with_receipt(schedule, epoch.elapsed())?;
            if let Some(control) = &mut self.control {
                control.trace_window(gate, &receipt, first_packet, epoch.elapsed());
                control.acknowledge(gate, &receipt, epoch.elapsed(), false)?;
            }
            self.emitted = self
                .emitted
                .checked_add(count)
                .ok_or(TeletextOutputError::Arithmetic)?;
        }
        Ok(())
    }
    async fn forward_ready<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
        &mut self,
        sink: &mut W,
        gate: &mut TeletextOutputGate,
        config: TeletextOutputConfig,
        epoch: Instant,
        prefetch: &mut SourcePrefetch<'_, R>,
    ) -> Result<bool, TeletextPumpError> {
        let end = match &mut self.control {
            Some(control) => control.window_end(gate, self.intervals)?,
            None => gate.window_end(self.intervals)?,
        };
        if let Some(end) = end {
            let count = usize::try_from(
                end.checked_sub(self.emitted)
                    .ok_or(TeletextOutputError::Boundary)?,
            )
            .map_err(|_| TeletextOutputError::Arithmetic)?;
            if count <= gate.retained_packets()? {
                if gate.prepare_window(count, epoch.elapsed())? {
                    let schedule = gate.pending_schedule()?;
                    if let Some(control) = &self.control {
                        control.trace_window_prepared(gate, &schedule, epoch.elapsed());
                    }
                    write_window(
                        sink,
                        gate,
                        &schedule,
                        config,
                        epoch,
                        prefetch,
                        self.control.as_ref(),
                    )
                    .await?;
                    let first_packet = schedule.first_packet();
                    let receipt =
                        gate.acknowledge_window_with_receipt(schedule, epoch.elapsed())?;
                    if let Some(control) = &mut self.control {
                        control.trace_window(gate, &receipt, first_packet, epoch.elapsed());
                        // Prefetch can discover EOF during this exact write.
                        // Never install a new service with no future full cycle.
                        if prefetch.ended {
                            control.source_ended();
                        }
                        control.acknowledge(gate, &receipt, epoch.elapsed(), true)?;
                    }
                    self.emitted = end;
                    self.intervals = 1;
                    return Ok(true);
                }
                if let Some(control) = &mut self.control {
                    if control.preparation_failed(gate, self.intervals, end)? {
                        return Ok(true);
                    }
                }
                self.intervals = self
                    .intervals
                    .checked_add(1)
                    .ok_or(TeletextOutputError::Arithmetic)?;
                return Ok(true);
            }
        }
        Ok(false)
    }
}

async fn write_window<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    sink: &mut W,
    gate: &mut TeletextOutputGate,
    schedule: &TeletextOutputSchedule,
    config: TeletextOutputConfig,
    epoch: Instant,
    prefetch: &mut SourcePrefetch<'_, R>,
    control: Option<&PumpControl>,
) -> Result<(), TeletextPumpError> {
    for index in 0..schedule.packet_count() {
        let packet = gate.pending_packets(epoch.elapsed())?[index];
        let due = schedule.packet_due(index)?;
        let deadline = write_deadline(gate, due, config.max_lateness, epoch)?;
        let due = absolute(epoch, due)?;
        prefetch
            .during_delivery(gate, epoch, async {
                timeout_at(deadline, tokio::time::sleep_until(due))
                    .await
                    .map_err(|_| TeletextOutputError::Deadline)?;
                if Instant::now() >= deadline {
                    return Err(TeletextOutputError::Deadline.into());
                }
                timeout_at(deadline, sink.write_all(&packet))
                    .await
                    .map_err(|_| TeletextOutputError::Deadline)?
                    .map_err(|error| TeletextPumpError::Write(error.kind()))
            })
            .await?;
        if index == 0 {
            if let Some(control) = control {
                control.trace_first_packet_written(
                    schedule.first_packet(),
                    &packet,
                    epoch.elapsed(),
                );
            }
        }
        if Instant::now() >= deadline {
            return Err(TeletextOutputError::Deadline.into());
        }
        gate.check_deadline(epoch.elapsed())?;
    }
    let deadline = write_deadline(gate, schedule.last_due(), config.max_lateness, epoch)?;
    prefetch
        .during_delivery(gate, epoch, async {
            timeout_at(deadline, sink.flush())
                .await
                .map_err(|_| TeletextOutputError::Deadline)?
                .map_err(|error| TeletextPumpError::Write(error.kind()))
        })
        .await?;
    if Instant::now() >= deadline {
        return Err(TeletextOutputError::Deadline.into());
    }
    gate.check_deadline(epoch.elapsed())?;
    Ok(())
}

fn write_deadline(
    gate: &TeletextOutputGate,
    due: Duration,
    lateness: Duration,
    epoch: Instant,
) -> Result<Instant, TeletextPumpError> {
    let packet = due
        .checked_add(lateness)
        .and_then(|limit| limit.checked_add(Duration::from_nanos(1)))
        .ok_or(TeletextOutputError::Arithmetic)?;
    let elapsed = gate
        .next_deadline()?
        .map_or(packet, |holding| holding.min(packet));
    absolute(epoch, elapsed)
}

fn absolute(epoch: Instant, elapsed: Duration) -> Result<Instant, TeletextPumpError> {
    epoch
        .checked_add(elapsed)
        .ok_or(TeletextPumpError::Configuration)
}

fn source_deadline_error(started: bool) -> TeletextPumpError {
    if started {
        TeletextOutputError::Deadline.into()
    } else {
        TeletextPumpError::FirstByteTimeout
    }
}

#[cfg(test)]
#[path = "teletext_output_tests.rs"]
pub(crate) mod tests;

#[cfg(test)]
#[path = "teletext_output_startup_tests.rs"]
mod startup_tests;
