use std::{sync::Arc, time::Duration};

use mpeg2ts_reader::packet::Packet;

use super::{
    EncodedTeletextAuthoredService, EncodedTeletextPageSet, TeletextBufferError, TeletextCarousel,
    TeletextClockConfig, TeletextClockError, TeletextClockValidator, TeletextMultiplexError,
    TeletextPacketBuffer, TeletextPacketizer, TeletextPmtInstallError, TeletextPmtInstallReport,
    TeletextPmtInstaller, TeletextPsiDiscovery, TeletextPsiError, TeletextPsiMonitor,
    TeletextServiceDescriptor, TeletextTimestamp, TeletextTransportError,
};

const STEP_TICKS: u64 = 1_800;
const PACKET_BITS: u128 = 188 * 8;

#[cfg(test)]
#[path = "output_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "output_tail_tests.rs"]
mod tail_tests;

#[cfg(test)]
#[path = "output_retained_tests.rs"]
mod retained_tests;

#[cfg(test)]
#[path = "authored_service_output_tests.rs"]
mod authored_service_tests;

#[cfg(test)]
#[path = "output_absent_tests.rs"]
mod absent_tests;

#[path = "output_transitions.rs"]
mod transitions;

#[cfg(test)]
#[path = "output_transition_tests.rs"]
pub(in crate::teletext) mod transition_tests;

/// Immutable admission and delivery policy for one host-owned TS188 epoch.
///
/// The selected program/PCR PID and declared muxrate must describe the encoded
/// output, not a reopened source. This policy supplies no production defaults.
#[derive(Debug, Clone, Copy)]
pub struct TeletextOutputConfig {
    /// Explicit PAT program to receive the authored service.
    pub program: u16,
    /// Dedicated authored-data PID, absent from the entire source multiplex.
    pub elementary_pid: u16,
    /// Expected selected-program PCR and explicit measured-rate policy.
    pub clock: TeletextClockConfig,
    /// Total retention capacity, including incomplete transport bytes.
    pub max_buffer_packets: usize,
    /// Maximum first-byte holding age, including a prepared sink window.
    pub max_hold: Duration,
    /// Delay from first source-byte arrival, or an explicit retained delivery
    /// start, to the first output packet's due time.
    pub playout_delay: Duration,
    /// Allowed sink lateness; an overdue window cannot be rescued by catchup.
    pub max_lateness: Duration,
}

/// Terminal output-gate failure, without retaining source or authored content.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TeletextOutputError {
    /// Output policy cannot describe a bounded carousel or retention budget.
    #[error("teletext output configuration is invalid")]
    Configuration,
    /// Retained bytes or wall-clock policy failed.
    #[error(transparent)]
    Buffer(#[from] TeletextBufferError),
    /// The selected source clock or constant muxrate failed.
    #[error(transparent)]
    Clock(#[from] TeletextClockError),
    /// Discovery or continuing source ownership failed.
    #[error(transparent)]
    Psi(#[from] TeletextPsiError),
    /// Admitted PMT installation failed.
    #[error(transparent)]
    Pmt(#[from] TeletextPmtInstallError),
    /// Carousel/null-slot admission failed.
    #[error(transparent)]
    Multiplex(#[from] TeletextMultiplexError),
    /// Authored transport timing failed.
    #[error(transparent)]
    Transport(#[from] TeletextTransportError),
    /// A requested output prefix cuts a carousel interval or exceeds retained data.
    #[error("teletext output prefix is not a complete carousel boundary")]
    Boundary,
    /// The owner attempted to replace an undelivered prepared window.
    #[error("teletext output still awaits sink acknowledgement")]
    Pending,
    /// A sink acknowledgement is absent, early, foreign or otherwise invalid.
    #[error("teletext output acknowledgement is invalid")]
    Acknowledgement,
    /// The output window exceeded its delivery/holding budget.
    #[error("teletext output exceeded its delivery deadline")]
    Deadline,
    /// A physical position or time cannot be represented without overflow.
    #[error("teletext output timing exceeds its arithmetic bound")]
    Arithmetic,
    /// EOF arrived before a complete authored cycle was acknowledged.
    #[error("teletext output ended before a complete authored cycle")]
    Unterminated,
    /// Retained output has not received its one-shot delivery start.
    #[error("teletext output delivery has not started")]
    DeliveryNotStarted,
    /// Warm magazine state cannot yet be retired without explicit reconciliation.
    #[error("teletext output requires warm magazine-cache reconciliation")]
    CacheReconciliationRequired,
    /// This ended source epoch cannot be reopened.
    #[error("teletext output generation has ended")]
    Closed,
}

/// Sink schedule for a fully validated, immutable output window.
///
/// The host must pace physical packets between these times, await the actual
/// sink writes and then acknowledge this exact window. This is not client proof.
pub struct TeletextOutputSchedule {
    epoch: Arc<()>,
    first_packet: u64,
    packet_count: usize,
    first_due: Duration,
    last_due: Duration,
    muxrate_bps: u64,
}

/// Exact sink-accepted window, without receiver or durable-publication authority.
/// The private stamp prevents reuse after a newer acknowledgement or transition.
/// A contained terminator does not imply the window ends at a cycle boundary.
pub struct TeletextOutputAcknowledgement {
    epoch: Arc<()>,
    stamp: Arc<()>,
    end_packet: u64,
    batch_ticks: u64,
    cycle_boundary: bool,
    completes_cycle: bool,
    signalling_announced: bool,
}

impl TeletextOutputAcknowledgement {
    #[must_use]
    pub const fn is_cycle_boundary(&self) -> bool {
        self.cycle_boundary
    }
    #[must_use]
    pub const fn contains_complete_cycle(&self) -> bool {
        self.completes_cycle
    }
    #[must_use]
    pub const fn signalling_announced(&self) -> bool {
        self.signalling_announced
    }
    #[must_use]
    pub const fn end_packet(&self) -> u64 {
        self.end_packet
    }
}

impl TeletextOutputSchedule {
    /// Original physical source position of the first packet.
    #[must_use]
    pub const fn first_packet(&self) -> u64 {
        self.first_packet
    }

    /// Number of packets; output extent exactly equals its source prefix.
    #[must_use]
    pub const fn packet_count(&self) -> usize {
        self.packet_count
    }

    /// Due time for the first packet on the owner's monotonic elapsed clock.
    #[must_use]
    pub const fn first_due(&self) -> Duration {
        self.first_due
    }

    /// Due time for the final packet, before the configured lateness allowance.
    #[must_use]
    pub const fn last_due(&self) -> Duration {
        self.last_due
    }

    /// Exact due time of one physical packet in this prepared window.
    /// # Errors
    /// Rejects out-of-window indices or unrepresentable elapsed times.
    pub fn packet_due(&self, index: usize) -> Result<Duration, TeletextOutputError> {
        if index >= self.packet_count {
            return Err(TeletextOutputError::Boundary);
        }
        let packet = self
            .first_packet
            .checked_add(u64::try_from(index).map_err(|_| TeletextOutputError::Arithmetic)?)
            .ok_or(TeletextOutputError::Arithmetic)?;
        let nanos = |position: u64| {
            u128::from(position) * PACKET_BITS * 1_000_000_000 / u128::from(self.muxrate_bps)
        };
        let offset = u64::try_from(nanos(packet) - nanos(self.first_packet))
            .map_err(|_| TeletextOutputError::Arithmetic)?;
        self.first_due
            .checked_add(Duration::from_nanos(offset))
            .ok_or(TeletextOutputError::Arithmetic)
    }
}

#[derive(Clone, Copy)]
enum DeliveryStart {
    OnArrival,
    Deferred,
    Started(Duration),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SourceState {
    Open,
    Ended,
    Closed,
}

/// Construction metadata; only sink acknowledgement turns it into a receipt.
struct PendingCycleEvidence {
    completes_cycle: bool,
    cycle_boundary: bool,
    signalling_announced: bool,
}

/// Payload selection and its independently acknowledged signalling transition.
struct PayloadSelection {
    active: bool,
    signalling_ready: bool,
}

struct PendingOutput {
    packets: Vec<[u8; 188]>,
    first_packet: u64,
    first_arrival: Duration,
    exposed: bool,
    cycle: PendingCycleEvidence,
    completed_authored: Option<Arc<EncodedTeletextAuthoredService>>,
}

/// Compose bounded retention, source PSI/PID, PCR/CBR and atomic page insertion.
///
/// Feed each new raw output byte once. Discovery and clock readiness precede
/// any prepared output. This instance owns a fresh source epoch: only the first
/// physical packet per PSI/PCR PID may carry a startup discontinuity indicator.
/// Later packets go through the sealed PSI monitor; a
/// failed generation discards all held/prepared bytes and cannot be retried.
/// Regular windows cover complete 20 ms intervals; actual EOF can drain a
/// fractional interval without adding an authored batch. PMT growth is
/// preflighted before all carousel batches are inserted into their own null
/// slots; insufficient interval capacity never silently drops or delays pages.
///
/// A prepared window is immutable and must be acknowledged after real sink
/// delivery before another can be prepared. The epoch token prevents using a
/// different output owner's acknowledgement. The host still owns asynchronous
/// reads, idle deadline polling, packet pacing, blocked-write deadlines,
/// cancellation, error reporting and output-role admission. This object neither
/// runs a task nor consumes/reopens media. Page refresh/publication is separate.
pub struct TeletextOutputGate {
    config: TeletextOutputConfig,
    service: Option<TeletextServiceDescriptor>,
    buffer: Option<TeletextPacketBuffer>,
    discovery: Option<TeletextPsiDiscovery>,
    monitor: Option<TeletextPsiMonitor>,
    clock: TeletextClockValidator,
    installer: Option<TeletextPmtInstaller>,
    packetizer: TeletextPacketizer,
    carousel: Option<TeletextCarousel>,
    anchor: Option<(u64, TeletextTimestamp)>,
    origin: Option<Duration>,
    delivery_start: DeliveryStart,
    observed: usize,
    source_start: u64,
    batch_ticks: u64,
    psi_ready: bool,
    pending: Option<PendingOutput>,
    epoch: Arc<()>,
    failure: Option<TeletextOutputError>,
    source_state: SourceState,
    completed_cycle: bool,
    payload: PayloadSelection,
    last_ack: Option<Arc<()>>,
    warm_service: Option<Arc<EncodedTeletextAuthoredService>>,
    // One bit per wire magazine (zero denotes magazine eight). Omission and
    // withdrawal cannot prove that an attached decoder forgot earlier M29.
    acknowledged_m29_magazines: u8,
    // One bit per checked logical S1 in each wire magazine. Only a completed
    // neutral bank with that exact owner clears previously delivered MOT state.
    acknowledged_nonempty_mots: [u16; 8],
}

impl TeletextOutputGate {
    /// Bind an immutable authored set and explicit policy to a new output epoch.
    /// # Errors
    /// Rejects invalid source identities, clocks, retention and timing budgets.
    pub fn new(
        config: TeletextOutputConfig,
        set: EncodedTeletextPageSet,
    ) -> Result<Self, TeletextOutputError> {
        let descriptor = TeletextServiceDescriptor::from_page_set(&set);
        Self::new_inner(
            config,
            Some(TeletextCarousel::new(set)),
            Some(descriptor),
            DeliveryStart::OnArrival,
        )
    }

    /// Retain and validate an encoded reserve before the owner starts delivery.
    ///
    /// Arrival/holding clocks start with the real source bytes. No physical
    /// delivery deadline runs before `start_delivery`; this does not reset or
    /// forgive holding age. Capacity and source PSI/PCR checks remain active.
    /// # Errors
    /// Rejects the same invalid source, retention and timing policy as `new`.
    pub fn new_retained(
        config: TeletextOutputConfig,
        set: EncodedTeletextPageSet,
    ) -> Result<Self, TeletextOutputError> {
        let descriptor = TeletextServiceDescriptor::from_page_set(&set);
        Self::new_inner(
            config,
            Some(TeletextCarousel::new(set)),
            Some(descriptor),
            DeliveryStart::Deferred,
        )
    }

    /// Plan complete authored packets through the existing bounded output epoch.
    /// This does not supply host publication or Level 2.5 admission authority.
    /// # Errors
    /// Rejects carousel repetition bounds and the existing source/timing policy.
    pub fn new_authored(
        config: TeletextOutputConfig,
        service: EncodedTeletextAuthoredService,
    ) -> Result<Self, TeletextOutputError> {
        Self::new_authored_shared(config, Arc::new(service))
    }

    /// Retain a complete authored candidate until its owner starts delivery.
    /// Source acquisition/holding and sink acknowledgement keep their epoch fences.
    /// # Errors
    /// Rejects the same carousel and output-policy bounds as `new_authored`.
    pub fn new_authored_retained(
        config: TeletextOutputConfig,
        service: EncodedTeletextAuthoredService,
    ) -> Result<Self, TeletextOutputError> {
        Self::new_authored_retained_shared(config, Arc::new(service))
    }

    /// Fresh paced epoch borrowing the same immutable complete authored service.
    /// # Errors
    /// Enforces the existing carousel, source identity and timing bounds.
    pub fn new_authored_shared(
        config: TeletextOutputConfig,
        service: Arc<EncodedTeletextAuthoredService>,
    ) -> Result<Self, TeletextOutputError> {
        let descriptor = TeletextServiceDescriptor::from_authored_service(&service);
        let carousel = TeletextCarousel::new_authored_shared(service)?;
        Self::new_inner(
            config,
            Some(carousel),
            Some(descriptor),
            DeliveryStart::OnArrival,
        )
    }

    /// Fresh retained epoch sharing content while preserving separate holding age.
    /// # Errors
    /// Enforces the same bounds as the owned retained constructor.
    pub fn new_authored_retained_shared(
        config: TeletextOutputConfig,
        service: Arc<EncodedTeletextAuthoredService>,
    ) -> Result<Self, TeletextOutputError> {
        let descriptor = TeletextServiceDescriptor::from_authored_service(&service);
        let carousel = TeletextCarousel::new_authored_shared(service)?;
        Self::new_inner(
            config,
            Some(carousel),
            Some(descriptor),
            DeliveryStart::Deferred,
        )
    }

    /// Start a bounded source epoch with no authored content or signalling.
    /// Source AV/PSI bytes remain exact; an acknowledged boundary may later
    /// accept a complete authored service through the existing replacement API.
    /// This reserves/checks the configured PID without inventing empty pages.
    /// # Errors
    /// Enforces the same explicit source, clock, PID and retention policy.
    pub fn new_absent(config: TeletextOutputConfig) -> Result<Self, TeletextOutputError> {
        Self::new_inner(config, None, None, DeliveryStart::OnArrival)
    }

    /// Retain an initially absent source until its owner starts delivery.
    /// Holding age is preserved and ordinary source packets are not rewritten.
    /// # Errors
    /// Enforces the same output-policy bounds as `new_absent`.
    pub fn new_absent_retained(config: TeletextOutputConfig) -> Result<Self, TeletextOutputError> {
        Self::new_inner(config, None, None, DeliveryStart::Deferred)
    }

    /// Establish the one final packet schedule after reserve readiness.
    ///
    /// Packet zero becomes due at `now + playout_delay`. Original byte arrivals
    /// remain unchanged. The owner must independently prove reserve readiness
    /// and compose its memory/startup policy before calling this operation.
    /// # Errors
    /// Expired retention, missing source readiness, repeated starts or prepared
    /// output permanently fence the epoch; normal streaming cannot be rebased.
    pub fn start_delivery(&mut self, now: Duration) -> Result<(), TeletextOutputError> {
        let result = self.start_delivery_inner(now);
        self.fence(result)
    }

    fn new_inner(
        config: TeletextOutputConfig,
        carousel: Option<TeletextCarousel>,
        descriptor: Option<TeletextServiceDescriptor>,
        delivery_start: DeliveryStart,
    ) -> Result<Self, TeletextOutputError> {
        let interval_packets = u128::from(config.clock.muxrate_bps) / (50 * PACKET_BITS);
        if config.playout_delay.is_zero()
            || config.max_hold <= config.playout_delay
            || config.max_lateness >= config.max_hold
            || interval_packets == 0
            || interval_packets + 1 > config.max_buffer_packets as u128
        {
            return Err(TeletextOutputError::Configuration);
        }
        let minimum_span = Duration::from_nanos(
            u64::try_from(
                interval_packets * PACKET_BITS * 1_000_000_000
                    / u128::from(config.clock.muxrate_bps),
            )
            .map_err(|_| TeletextOutputError::Configuration)?,
        );
        if config.max_hold
            <= config
                .playout_delay
                .checked_add(minimum_span)
                .ok_or(TeletextOutputError::Configuration)?
        {
            return Err(TeletextOutputError::Configuration);
        }
        let active = carousel.is_some();
        Ok(Self {
            config,
            service: descriptor,
            buffer: Some(TeletextPacketBuffer::new(
                config.max_buffer_packets,
                config.max_hold,
            )?),
            discovery: Some(TeletextPsiDiscovery::new_source_epoch(
                config.program,
                config.elementary_pid,
            )?),
            monitor: None,
            clock: TeletextClockValidator::new_source_epoch(config.clock)?,
            installer: None,
            packetizer: TeletextPacketizer::new(config.elementary_pid)?,
            carousel,
            anchor: None,
            origin: None,
            delivery_start,
            observed: 0,
            source_start: 0,
            batch_ticks: 0,
            psi_ready: false,
            pending: None,
            epoch: Arc::new(()),
            failure: None,
            source_state: SourceState::Open,
            completed_cycle: false,
            payload: PayloadSelection {
                active,
                signalling_ready: true,
            },
            last_ack: None,
            warm_service: None,
            acknowledged_m29_magazines: 0,
            acknowledged_nonempty_mots: [0; 8],
        })
    }

    /// Retain and validate new raw bytes, including partial TS188 reads.
    /// # Errors
    /// Every input, ownership, clock, capacity or deadline error is terminal.
    pub fn push(&mut self, bytes: &[u8], now: Duration) -> Result<(), TeletextOutputError> {
        let result = self.push_inner(bytes, now);
        self.fence(result)
    }

    /// Earliest complete carousel interval's global exclusive packet boundary.
    ///
    /// Before the first PCR this is absent. This is framing information only;
    /// preparation still requires complete source admission and clock evidence.
    /// # Errors
    /// Returns a retained failure or arithmetic overflow.
    pub fn next_window_end(&self) -> Result<Option<u64>, TeletextOutputError> {
        self.window_end(1)
    }

    /// Exclusive boundary after the specified number of pending 20 ms intervals.
    ///
    /// The owner can retain a larger prefix when a source PMT or local capacity
    /// crosses the earliest boundary. This never bypasses preparation/deadlines.
    /// # Errors
    /// Rejects zero/oversized interval counts, failed epochs or timing overflow.
    pub fn window_end(&self, intervals: usize) -> Result<Option<u64>, TeletextOutputError> {
        self.ensure_open()?;
        if !(1..=super::signalling::MAX_WINDOW_PACKETS).contains(&intervals) {
            return Err(TeletextOutputError::Boundary);
        }
        let next = self
            .batch_ticks
            .checked_add(
                u64::try_from(intervals)
                    .map_err(|_| TeletextOutputError::Arithmetic)?
                    .checked_mul(STEP_TICKS)
                    .ok_or(TeletextOutputError::Arithmetic)?,
            )
            .ok_or(TeletextOutputError::Arithmetic)?;
        self.anchor.map(|_| self.frame_position(next)).transpose()
    }

    /// Whether complete source PSI and the declared clock have been admitted.
    ///
    /// This does not establish reserve readiness, start delivery or release bytes.
    /// A later split source table may temporarily require additional retained input.
    /// # Errors
    /// Rejects failed/closed epochs or a failed source clock.
    pub fn source_ready(&self) -> Result<bool, TeletextOutputError> {
        self.ensure_open()?;
        Ok(self.psi_ready && self.clock.report()?.ready)
    }

    /// Complete retained raw packets available for preparation.
    /// # Errors
    /// Rejects failed or closed generations.
    pub fn retained_packets(&self) -> Result<usize, TeletextOutputError> {
        self.ensure_open()?;
        Ok(self.buffer_ref()?.window()?.len())
    }

    /// Remaining shared raw/prepared byte capacity, including partial input.
    /// # Errors
    /// Rejects failed or closed generations.
    pub fn remaining_capacity(&self) -> Result<usize, TeletextOutputError> {
        self.ensure_open()?;
        let pending = self
            .pending
            .as_ref()
            .map_or(0, |pending| pending.packets.len() * 188);
        Ok(self.config.max_buffer_packets * 188 - self.buffer_ref()?.held_bytes() - pending)
    }

    /// Earliest holding or delivery expiry for an asynchronous wait.
    ///
    /// Delivery expiry is one nanosecond beyond the inclusive lateness limit.
    /// Before any source arrival or when fully drained, this is absent; the
    /// asynchronous owner separately bounds waiting for new source bytes.
    /// # Errors
    /// Rejects failed/closed epochs or unrepresentable deadlines.
    pub fn next_deadline(&self) -> Result<Option<Duration>, TeletextOutputError> {
        self.ensure_open()?;
        let mut deadline = self
            .buffer_ref()?
            .first_arrival()?
            .map(|arrival| {
                arrival
                    .checked_add(self.config.max_hold)
                    .ok_or(TeletextOutputError::Arithmetic)
            })
            .transpose()?;
        if let Some(pending) = &self.pending {
            let hold = pending
                .first_arrival
                .checked_add(self.config.max_hold)
                .ok_or(TeletextOutputError::Arithmetic)?;
            deadline = Some(deadline.map_or(hold, |previous| previous.min(hold)));
            let packet = if pending.exposed {
                pending
                    .first_packet
                    .checked_add(
                        u64::try_from(pending.packets.len() - 1)
                            .map_err(|_| TeletextOutputError::Arithmetic)?,
                    )
                    .ok_or(TeletextOutputError::Arithmetic)?
            } else {
                pending.first_packet
            };
            let delivery = self.delivery_expiry(packet)?;
            deadline = Some(deadline.map_or(delivery, |previous| previous.min(delivery)));
        } else if self.buffer_ref()?.held_bytes() != 0 && self.delivery_started() {
            let delivery = self.delivery_expiry(self.source_start)?;
            deadline = Some(deadline.map_or(delivery, |previous| previous.min(delivery)));
        }
        Ok(deadline)
    }

    /// Prepare a complete retained prefix through a physical carousel boundary.
    ///
    /// False retains everything while PSI/clock evidence, source fragments or
    /// local PMT capacity require more bytes. The owner can extend the requested
    /// prefix to the next complete interval. Successful preparation inserts all
    /// corresponding batches before exposing any bytes. Page/null capacity or
    /// source failures are terminal; they cannot be rescued by overdue bursts.
    /// # Errors
    /// Invalid prefixes, pending delivery, source faults and overdue output fail.
    pub fn prepare_window(
        &mut self,
        packet_count: usize,
        now: Duration,
    ) -> Result<bool, TeletextOutputError> {
        let result = self.prepare_inner(packet_count, now);
        self.fence(result)
    }

    /// Seal actual source EOF and prepare all remaining complete transport packets.
    ///
    /// Only complete 20 ms intervals receive batches. A fractional final interval
    /// preserves its physical packets without padding or starting another batch.
    /// At least one complete cycle must have been acknowledged or be included in
    /// this final window; preparation itself does not acknowledge delivery.
    /// False means a successfully sealed source has no remaining packets.
    /// # Errors
    /// Incomplete TS/PSI, unavailable clock/capacity, pending output or an initial
    /// incomplete carousel are terminal: EOF cannot wait for additional input.
    pub fn prepare_final_window(&mut self, now: Duration) -> Result<bool, TeletextOutputError> {
        let result = self.prepare_final_inner(now);
        self.fence(result)
    }

    /// Borrow the immutable prepared packet window before pacing its sink writes.
    /// # Errors
    /// No prepared window or any retained/delivery failure prevents access.
    pub fn pending_packets(&mut self, now: Duration) -> Result<&[[u8; 188]], TeletextOutputError> {
        self.check_deadline(now)?;
        if self.pending.is_none() {
            return self.fence(Err(TeletextOutputError::Pending));
        }
        let Some(pending) = self.pending.as_mut() else {
            return Err(TeletextOutputError::Pending);
        };
        pending.exposed = true;
        Ok(&pending.packets)
    }

    /// Obtain an epoch-bound acknowledgement token and physical sink schedule.
    /// # Errors
    /// Requires an open, prepared window and representable due times.
    pub fn pending_schedule(&self) -> Result<TeletextOutputSchedule, TeletextOutputError> {
        self.ensure_open()?;
        let pending = self.pending.as_ref().ok_or(TeletextOutputError::Pending)?;
        let last = pending.first_packet
            + u64::try_from(pending.packets.len() - 1)
                .map_err(|_| TeletextOutputError::Arithmetic)?;
        Ok(TeletextOutputSchedule {
            epoch: Arc::clone(&self.epoch),
            first_packet: pending.first_packet,
            packet_count: pending.packets.len(),
            first_due: self.due_at(pending.first_packet)?,
            last_due: self.due_at(last)?,
            muxrate_bps: self.config.clock.muxrate_bps,
        })
    }

    /// Acknowledge this exact prepared window after all its paced sink writes.
    /// # Errors
    /// Foreign/stale/unexposed acknowledgements, early/late delivery or holding
    /// failure permanently fence the epoch. No page boundary is released early.
    pub fn acknowledge_window(
        &mut self,
        schedule: TeletextOutputSchedule,
        now: Duration,
    ) -> Result<(), TeletextOutputError> {
        self.acknowledge_window_with_receipt(schedule, now)
            .map(|_| ())
    }

    /// Return end-of-window evidence only after exact exposed, paced sink acceptance.
    /// # Errors
    /// Preserves the same terminal acknowledgement/deadline checks as the wrapper.
    pub fn acknowledge_window_with_receipt(
        &mut self,
        schedule: TeletextOutputSchedule,
        now: Duration,
    ) -> Result<TeletextOutputAcknowledgement, TeletextOutputError> {
        let result = self.acknowledge_inner(&schedule, now);
        drop(schedule);
        self.fence(result)
    }

    /// Poll holding and prepared-output deadlines while reads or writes are idle.
    /// # Errors
    /// A failed or overdue generation cannot continue or expose held output.
    pub fn check_deadline(&mut self, now: Duration) -> Result<(), TeletextOutputError> {
        let result = self.check_inner(now);
        self.fence(result)
    }

    /// Close after all source bytes and, when selected, a complete authored cycle.
    ///
    /// This does not invent tail packets or silently discard a partial interval.
    /// The owner drains any fractional interval with `prepare_final_window`.
    /// Later repetitions may end after a previously acknowledged complete cycle;
    /// this proves historical delivery, not the final decoder cache or handover.
    /// Repeated successful closure is idempotent.
    /// # Errors
    /// Retained/incomplete bytes, undelivered windows or unterminated cycles fail.
    pub fn finish(&mut self, now: Duration) -> Result<(), TeletextOutputError> {
        if self.source_state == SourceState::Closed && self.failure.is_none() {
            let result = self.buffer_mut()?.finish(now).map_err(Into::into);
            return self.fence(result);
        }
        let result = self.finish_inner(now);
        self.fence(result)
    }

    fn push_inner(&mut self, bytes: &[u8], now: Duration) -> Result<(), TeletextOutputError> {
        self.check_inner(now)?;
        self.ensure_source_open()?;
        let held = self.buffer_ref()?.held_bytes()
            + self
                .pending
                .as_ref()
                .map_or(0, |pending| pending.packets.len() * 188);
        if bytes.len() > self.config.max_buffer_packets * 188 - held {
            return Err(TeletextBufferError::Capacity.into());
        }
        if !bytes.is_empty() && self.origin.is_none() {
            self.origin = Some(now);
        }
        self.buffer_mut()?.push(bytes, now)?;
        while self.observed < self.buffer_ref()?.window()?.len() {
            let raw = self.buffer_ref()?.window()?[self.observed];
            let position = self
                .source_start
                .checked_add(
                    u64::try_from(self.observed).map_err(|_| TeletextOutputError::Arithmetic)?,
                )
                .ok_or(TeletextOutputError::Arithmetic)?;
            self.clock.observe(&[raw])?;
            if self.anchor.is_none() {
                let packet = Packet::new(&raw);
                if u16::from(packet.pid()) == self.config.clock.pcr_pid {
                    if let Some(clock) =
                        packet.adaptation_field().and_then(|field| field.pcr().ok())
                    {
                        self.anchor = Some((position, TeletextTimestamp::new(clock.base())?));
                    }
                }
            }
            if let Some(discovery) = self.discovery.as_mut() {
                discovery.push(&raw)?;
                match discovery.plan_optional(self.service.as_ref()) {
                    Ok(plan) => {
                        if plan.pcr_pid() != self.config.clock.pcr_pid {
                            return Err(TeletextMultiplexError::Clock.into());
                        }
                        self.installer = Some(TeletextPmtInstaller::new_source_epoch(plan));
                        self.monitor = Some(
                            self.discovery
                                .take()
                                .ok_or(TeletextOutputError::Configuration)?
                                .into_monitor_optional(self.service.as_ref())?,
                        );
                        self.psi_ready = true;
                    }
                    Err(TeletextPsiError::Incomplete) => (),
                    Err(error) => return Err(error.into()),
                }
            } else {
                self.psi_ready = self
                    .monitor
                    .as_mut()
                    .ok_or(TeletextOutputError::Configuration)?
                    .inspect_window(&[raw])?;
            }
            self.observed += 1;
        }
        Ok(())
    }

    fn prepare_inner(&mut self, count: usize, now: Duration) -> Result<bool, TeletextOutputError> {
        self.check_inner(now)?;
        self.ensure_source_open()?;
        if !self.delivery_started() {
            return Err(TeletextOutputError::DeliveryNotStarted);
        }
        if self.pending.is_some() {
            return Err(TeletextOutputError::Pending);
        }
        if !self.psi_ready || !self.clock.report()?.ready {
            return Ok(false);
        }
        let length = self.buffer_ref()?.window()?.len();
        if count == 0 || count > length {
            return Err(TeletextOutputError::Boundary);
        }
        let end = self
            .source_start
            .checked_add(u64::try_from(count).map_err(|_| TeletextOutputError::Arithmetic)?)
            .ok_or(TeletextOutputError::Arithmetic)?;
        let end_ticks = self.boundary_ticks(end)?;
        if end_ticks <= self.batch_ticks {
            return Err(TeletextOutputError::Boundary);
        }
        self.prepare_packets(count, end, end_ticks, false, now)
    }

    fn prepare_final_inner(&mut self, now: Duration) -> Result<bool, TeletextOutputError> {
        self.check_inner(now)?;
        self.ensure_source_open()?;
        if !self.delivery_started() {
            return Err(TeletextOutputError::DeliveryNotStarted);
        }
        if self.pending.is_some() {
            return Err(TeletextOutputError::Pending);
        }
        let count = self.buffer_ref()?.window()?.len();
        if self.buffer_ref()?.held_bytes() != count * 188 {
            return Err(TeletextBufferError::PartialPacket.into());
        }
        self.monitor
            .as_mut()
            .ok_or(TeletextPsiError::Incomplete)?
            .finish()?;
        if !self.clock.report()?.ready {
            return Err(TeletextMultiplexError::Clock.into());
        }
        self.source_state = SourceState::Ended;
        if count == 0 {
            if !self.payload.signalling_ready || (self.payload.active && !self.completed_cycle) {
                return Err(TeletextOutputError::Unterminated);
            }
            return Ok(false);
        }
        let end = self
            .source_start
            .checked_add(u64::try_from(count).map_err(|_| TeletextOutputError::Arithmetic)?)
            .ok_or(TeletextOutputError::Arithmetic)?;
        // Include a batch only when its entire original physical interval fits.
        let mut end_ticks = self.batch_ticks;
        loop {
            let next = end_ticks
                .checked_add(STEP_TICKS)
                .ok_or(TeletextOutputError::Arithmetic)?;
            if self.frame_position(next)? > end {
                break;
            }
            end_ticks = next;
        }
        self.prepare_packets(count, end, end_ticks, true, now)
    }

    fn install_source_window(
        &mut self,
        packets: &mut [[u8; 188]],
    ) -> Result<TeletextPmtInstallReport, TeletextOutputError> {
        let installer = self
            .installer
            .as_mut()
            .ok_or(TeletextOutputError::Configuration)?;
        Ok(if self.carousel.is_none() {
            installer.forward_unchanged(packets)
        } else {
            installer.install(packets)
        }?)
    }

    fn prepare_packets(
        &mut self,
        count: usize,
        end: u64,
        end_ticks: u64,
        final_window: bool,
        now: Duration,
    ) -> Result<bool, TeletextOutputError> {
        let first_due = self.due_at(self.source_start)?;
        if now
            > first_due
                .checked_add(self.config.max_lateness)
                .ok_or(TeletextOutputError::Arithmetic)?
        {
            return Err(TeletextOutputError::Deadline);
        }
        let arrival = self
            .buffer_ref()?
            .first_arrival()?
            .ok_or(TeletextOutputError::Boundary)?;
        let mut packets = self.buffer_ref()?.window()?[..count].to_vec();
        let installation = match self.install_source_window(&mut packets) {
            Ok(report) => report,
            Err(TeletextOutputError::Pmt(
                TeletextPmtInstallError::Incomplete
                | TeletextPmtInstallError::Multiplex(TeletextMultiplexError::Capacity { .. }),
            )) if !final_window => return Ok(false),
            Err(error) => return Err(error),
        };
        let mut completes_cycle = false;
        while self.batch_ticks < end_ticks {
            let start = self.frame_position(self.batch_ticks)?;
            let stop = self.frame_position(
                self.batch_ticks
                    .checked_add(STEP_TICKS)
                    .ok_or(TeletextOutputError::Arithmetic)?,
            )?;
            let range = usize::try_from(
                start
                    .checked_sub(self.source_start)
                    .ok_or(TeletextOutputError::Boundary)?,
            )
            .map_err(|_| TeletextOutputError::Arithmetic)?
                ..usize::try_from(
                    stop.checked_sub(self.source_start)
                        .ok_or(TeletextOutputError::Boundary)?,
                )
                .map_err(|_| TeletextOutputError::Arithmetic)?;
            let batch = if self.payload.active && self.payload.signalling_ready {
                let batch = self
                    .carousel
                    .as_mut()
                    .ok_or(TeletextOutputError::Configuration)?
                    .next_batch()?;
                completes_cycle |= batch.kind() == super::TeletextCarouselBatchKind::Terminator;
                Some(batch)
            } else {
                if let Some(carousel) = self.carousel.as_mut() {
                    carousel.advance_empty_at_boundary(STEP_TICKS)?;
                }
                None
            };
            let next = self
                .batch_ticks
                .checked_add(STEP_TICKS)
                .ok_or(TeletextOutputError::Arithmetic)?;
            let pts = self
                .anchor
                .ok_or(TeletextOutputError::Boundary)?
                .1
                .wrapping_add(next);
            if let Some(batch) = batch {
                self.monitor
                    .as_ref()
                    .ok_or(TeletextOutputError::Configuration)?
                    .plan()
                    .replace_null_packets(&mut packets[range], &batch, pts, &mut self.packetizer)?;
            }
            self.batch_ticks = next;
        }
        if final_window && self.payload.active && !self.completed_cycle && !completes_cycle {
            return Err(TeletextOutputError::Unterminated);
        }
        self.buffer_mut()?.release(count, now)?;
        self.observed -= count;
        let completed_authored = self
            .carousel
            .as_ref()
            .filter(|_| completes_cycle)
            .and_then(TeletextCarousel::authored_service_shared)
            .cloned();
        self.pending = Some(PendingOutput {
            packets,
            first_packet: self.source_start,
            first_arrival: arrival,
            exposed: false,
            cycle: PendingCycleEvidence {
                completes_cycle,
                cycle_boundary: self
                    .carousel
                    .as_ref()
                    .is_none_or(TeletextCarousel::is_cycle_boundary),
                signalling_announced: installation.announced,
            },
            completed_authored,
        });
        self.source_start = end;
        Ok(true)
    }

    fn acknowledge_inner(
        &mut self,
        schedule: &TeletextOutputSchedule,
        now: Duration,
    ) -> Result<TeletextOutputAcknowledgement, TeletextOutputError> {
        self.check_inner(now)?;
        let pending = self
            .pending
            .as_ref()
            .ok_or(TeletextOutputError::Acknowledgement)?;
        if !pending.exposed
            || !Arc::ptr_eq(&self.epoch, &schedule.epoch)
            || pending.first_packet != schedule.first_packet
            || pending.packets.len() != schedule.packet_count
        {
            return Err(TeletextOutputError::Acknowledgement);
        }
        if now < schedule.last_due {
            return Err(TeletextOutputError::Acknowledgement);
        }
        if now
            > schedule
                .last_due
                .checked_add(self.config.max_lateness)
                .ok_or(TeletextOutputError::Arithmetic)?
        {
            return Err(TeletextOutputError::Deadline);
        }
        // A completed acquisition cycle ties these actual M29 units to this
        // exposed, timely sink ACK. Candidate metadata alone grants no history.
        let mut acknowledged_magazines = 0_u8;
        if let Some(service) = &pending.completed_authored {
            for unit in service.magazine_units() {
                let super::source_header::TeletextHeaderObservation::NotHeader(address) =
                    super::source_header::observe_dvb_teletext_header(unit)
                        .map_err(|_| TeletextOutputError::Configuration)?
                else {
                    return Err(TeletextOutputError::Configuration);
                };
                if address.packet_number != 29 {
                    return Err(TeletextOutputError::Configuration);
                }
                acknowledged_magazines |= 1 << address.wire_magazine;
            }
        }
        self.acknowledged_m29_magazines |= acknowledged_magazines;
        self.completed_cycle |= pending.cycle.completes_cycle;
        let stamp = Arc::new(());
        let receipt = TeletextOutputAcknowledgement {
            epoch: Arc::clone(&self.epoch),
            stamp: Arc::clone(&stamp),
            end_packet: self.source_start,
            batch_ticks: self.batch_ticks,
            cycle_boundary: pending.cycle.cycle_boundary,
            completes_cycle: pending.cycle.completes_cycle,
            signalling_announced: pending.cycle.signalling_announced,
        };
        if pending.cycle.signalling_announced {
            self.payload.signalling_ready = true;
        }
        if let Some(service) = &pending.completed_authored {
            for mot in service.declared().mots() {
                let magazine = usize::from(mot.identity().page().magazine() & 7);
                let owner = 1_u16 << mot.identity().logical_subpage().get();
                if transitions::empty_mot(mot) {
                    self.acknowledged_nonempty_mots[magazine] &= !owner;
                } else {
                    self.acknowledged_nonempty_mots[magazine] |= owner;
                }
            }
            self.warm_service = Some(Arc::clone(service));
        }
        self.last_ack = Some(stamp);
        self.pending = None;
        Ok(receipt)
    }

    fn check_inner(&mut self, now: Duration) -> Result<(), TeletextOutputError> {
        self.ensure_open()?;
        self.buffer_mut()?.check_deadline(now)?;
        if self.pending.is_none()
            && self.buffer_ref()?.held_bytes() != 0
            && self.delivery_started()
            && now >= self.delivery_expiry(self.source_start)?
        {
            return Err(TeletextOutputError::Deadline);
        }
        if let Some(pending) = &self.pending {
            if now
                .checked_sub(pending.first_arrival)
                .ok_or(TeletextOutputError::Buffer(TeletextBufferError::TimeOrder))?
                >= self.config.max_hold
            {
                return Err(TeletextOutputError::Deadline);
            }
            let deadline_packet = if pending.exposed {
                pending
                    .first_packet
                    .checked_add(
                        u64::try_from(pending.packets.len() - 1)
                            .map_err(|_| TeletextOutputError::Arithmetic)?,
                    )
                    .ok_or(TeletextOutputError::Arithmetic)?
            } else {
                pending.first_packet
            };
            if now
                > self
                    .due_at(deadline_packet)?
                    .checked_add(self.config.max_lateness)
                    .ok_or(TeletextOutputError::Arithmetic)?
            {
                return Err(TeletextOutputError::Deadline);
            }
        }
        Ok(())
    }

    fn finish_inner(&mut self, now: Duration) -> Result<(), TeletextOutputError> {
        self.check_inner(now)?;
        if self.pending.is_some() {
            return Err(TeletextOutputError::Pending);
        }
        self.buffer_mut()?.finish(now)?;
        self.monitor
            .as_mut()
            .ok_or(TeletextOutputError::Unterminated)?
            .finish()?;
        if !self.payload.signalling_ready || (self.payload.active && !self.completed_cycle) {
            return Err(TeletextOutputError::Unterminated);
        }
        self.source_state = SourceState::Closed;
        Ok(())
    }

    fn frame_position(&self, ticks: u64) -> Result<u64, TeletextOutputError> {
        let anchor = self.anchor.ok_or(TeletextOutputError::Boundary)?.0;
        let offset =
            u128::from(ticks) * u128::from(self.config.clock.muxrate_bps) / (90_000 * PACKET_BITS);
        anchor
            .checked_add(1)
            .and_then(|start| start.checked_add(u64::try_from(offset).ok()?))
            .ok_or(TeletextOutputError::Arithmetic)
    }

    fn boundary_ticks(&self, end: u64) -> Result<u64, TeletextOutputError> {
        let first = self.frame_position(0)?;
        let distance = end
            .checked_sub(first)
            .ok_or(TeletextOutputError::Boundary)?;
        let frame = (u128::from(distance) * 50 * PACKET_BITS)
            .div_ceil(u128::from(self.config.clock.muxrate_bps));
        let ticks = u64::try_from(frame * u128::from(STEP_TICKS))
            .map_err(|_| TeletextOutputError::Arithmetic)?;
        if self.frame_position(ticks)? != end {
            return Err(TeletextOutputError::Boundary);
        }
        Ok(ticks)
    }

    fn due_at(&self, packet: u64) -> Result<Duration, TeletextOutputError> {
        let nanos = u128::from(packet) * PACKET_BITS * 1_000_000_000
            / u128::from(self.config.clock.muxrate_bps);
        let origin = match self.delivery_start {
            DeliveryStart::OnArrival => self.origin.ok_or(TeletextOutputError::Boundary)?,
            DeliveryStart::Started(origin) => origin,
            DeliveryStart::Deferred => return Err(TeletextOutputError::DeliveryNotStarted),
        };
        origin
            .checked_add(self.config.playout_delay)
            .and_then(|origin| origin.checked_add(Duration::from_nanos(u64::try_from(nanos).ok()?)))
            .ok_or(TeletextOutputError::Arithmetic)
    }

    fn delivery_expiry(&self, packet: u64) -> Result<Duration, TeletextOutputError> {
        self.due_at(packet)?
            .checked_add(self.config.max_lateness)
            .and_then(|limit| limit.checked_add(Duration::from_nanos(1)))
            .ok_or(TeletextOutputError::Arithmetic)
    }

    fn ensure_open(&self) -> Result<(), TeletextOutputError> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        if self.source_state == SourceState::Closed {
            return Err(TeletextOutputError::Closed);
        }
        Ok(())
    }

    fn start_delivery_inner(&mut self, now: Duration) -> Result<(), TeletextOutputError> {
        self.check_inner(now)?;
        self.ensure_source_open()?;
        if !matches!(self.delivery_start, DeliveryStart::Deferred)
            || self.pending.is_some()
            || self.origin.is_none()
            || !self.psi_ready
            || !self.clock.report()?.ready
        {
            return Err(TeletextOutputError::Configuration);
        }
        self.delivery_start = DeliveryStart::Started(now);
        // Establishing a start does not allow unrepresentable due times.
        self.due_at(self.source_start)?;
        Ok(())
    }

    fn delivery_started(&self) -> bool {
        !matches!(self.delivery_start, DeliveryStart::Deferred)
    }

    fn ensure_source_open(&self) -> Result<(), TeletextOutputError> {
        if self.source_state != SourceState::Open {
            return Err(TeletextOutputError::Closed);
        }
        Ok(())
    }

    fn buffer_ref(&self) -> Result<&TeletextPacketBuffer, TeletextOutputError> {
        self.buffer.as_ref().ok_or(TeletextOutputError::Closed)
    }

    fn buffer_mut(&mut self) -> Result<&mut TeletextPacketBuffer, TeletextOutputError> {
        self.buffer.as_mut().ok_or(TeletextOutputError::Closed)
    }

    fn fence<T>(
        &mut self,
        result: Result<T, TeletextOutputError>,
    ) -> Result<T, TeletextOutputError> {
        if let Err(error) = &result {
            if self.failure.is_none() {
                self.failure = Some(error.clone());
                self.buffer = None;
                self.pending = None;
                self.discovery = None;
                self.monitor = None;
                self.installer = None;
            }
        }
        result
    }
}
