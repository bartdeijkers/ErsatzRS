use std::time::Duration;

use mpeg2ts_reader::packet::Packet;

use super::*;
use crate::teletext::{
    psi_tests::{one, packet, pat, pmt, revise},
    signalling_tests::{page, pages},
};

pub(super) fn config() -> TeletextOutputConfig {
    TeletextOutputConfig {
        program: 10,
        elementary_pid: 512,
        clock: TeletextClockConfig {
            pcr_pid: 300,
            muxrate_bps: 5_000_000,
            rate_tolerance_ppm: 0,
            pcr_jitter_ticks: 0,
            max_pcr_gap_packets: 100,
            max_pcr_gap_ticks: 1_080_000,
            min_pcr_intervals: 2,
            max_window_packets: 1024,
        },
        max_buffer_packets: 1024,
        max_hold: Duration::from_millis(500),
        playout_delay: Duration::from_millis(100),
        max_lateness: Duration::from_millis(20),
    }
}

pub(super) fn set() -> EncodedTeletextPageSet {
    pages(vec![page(200, 0, "nld", false)])
}

fn gate() -> TeletextOutputGate {
    TeletextOutputGate::new(config(), set()).unwrap()
}

pub(super) fn pcr(ticks: u64) -> [u8; 188] {
    let base = ticks / 300;
    let extension = ticks % 300;
    let mut raw = [0xff; 188];
    raw[..6].copy_from_slice(&[0x47, 1, 44, 0x20, 183, 0x10]);
    raw[6] = u8::try_from(base >> 25).unwrap();
    raw[7] = u8::try_from((base >> 17) & 255).unwrap();
    raw[8] = u8::try_from((base >> 9) & 255).unwrap();
    raw[9] = u8::try_from((base >> 1) & 255).unwrap();
    raw[10] = u8::try_from((base & 1) << 7).unwrap() | 0x7e | u8::try_from(extension >> 8).unwrap();
    raw[11] = u8::try_from(extension & 255).unwrap();
    raw
}

pub(super) fn end(frame: u64) -> usize {
    usize::try_from(3 + frame * 5_000_000 / (50 * 188 * 8)).unwrap()
}

pub(super) fn source(frames: u64) -> Vec<[u8; 188]> {
    let mut raw = vec![packet(8191, false, 0, &[0xff; 184]); end(frames)];
    raw[0] = one(0, 0, &pat(0, 0, 0, &[(10, 3337)]));
    raw[1] = one(3337, 0, &pmt(10, 300, 300, &[]));
    for (sample, position) in (2..raw.len()).step_by(5).enumerate() {
        raw[position] = pcr(900_000 + u64::try_from(sample).unwrap() * 40_608);
    }
    raw
}

pub(super) fn bytes(raw: &[[u8; 188]]) -> Vec<u8> {
    raw.iter().flatten().copied().collect()
}

fn fenced(gate: &mut TeletextOutputGate, error: TeletextOutputError) {
    assert_eq!(gate.push(&[], Duration::ZERO), Err(error.clone()));
    assert_eq!(gate.prepare_window(1, Duration::ZERO), Err(error.clone()));
    assert_eq!(gate.pending_packets(Duration::ZERO), Err(error.clone()));
    assert!(matches!(gate.pending_schedule(), Err(found) if found == error));
    assert_eq!(gate.finish(Duration::ZERO), Err(error));
    assert!(gate.buffer.is_none());
    assert!(gate.pending.is_none());
}

fn prepared(gate: &mut TeletextOutputGate, source: &[[u8; 188]]) {
    gate.push(&bytes(source), Duration::ZERO).unwrap();
    assert!(gate.prepare_window(end(1), Duration::ZERO).unwrap());
}

pub(super) fn expose_and_ack(gate: &mut TeletextOutputGate) -> Vec<[u8; 188]> {
    let schedule = gate.pending_schedule().unwrap();
    let last_due = schedule.last_due();
    let output = gate.pending_packets(schedule.first_due()).unwrap().to_vec();
    gate.acknowledge_window(schedule, last_due).unwrap();
    output
}

#[test]
fn impossible_first_window_capacity_or_holding_budget_is_rejected_at_admission() {
    let packets = 5_000_000 / (50 * 188 * 8);
    let minimum_last_due = config()
        .playout_delay
        .checked_add(Duration::from_nanos(
            packets * 188 * 8 * 1_000_000_000 / 5_000_000,
        ))
        .unwrap();
    for kind in 0..5 {
        let mut policy = config();
        match kind {
            0 => policy.max_buffer_packets = usize::try_from(packets).unwrap(),
            1 => policy.max_hold = minimum_last_due,
            2 => policy.clock.muxrate_bps = 50 * 188 * 8 - 1,
            3 => policy.playout_delay = Duration::ZERO,
            _ => {
                policy.playout_delay = Duration::MAX.checked_sub(Duration::from_nanos(1)).unwrap();
                policy.max_hold = Duration::MAX;
            }
        }
        assert!(matches!(
            TeletextOutputGate::new(policy, set()),
            Err(TeletextOutputError::Configuration)
        ));
    }
    let mut policy = config();
    policy.max_buffer_packets = usize::try_from(packets + 1).unwrap();
    policy.max_hold = minimum_last_due
        .checked_add(Duration::from_nanos(1))
        .unwrap();
    assert!(TeletextOutputGate::new(policy, set()).is_ok());
}

#[test]
fn asynchronous_reads_observe_the_same_raw_partial_and_pending_byte_capacity() {
    let mut gate = gate();
    let capacity = config().max_buffer_packets * 188;
    assert_eq!(gate.remaining_capacity(), Ok(capacity));
    assert_eq!(gate.retained_packets(), Ok(0));
    let raw = source(3);
    gate.push(&bytes(&raw), Duration::ZERO).unwrap();
    gate.push(&[0x47], Duration::ZERO).unwrap();
    let remaining = capacity - raw.len() * 188 - 1;
    assert_eq!(gate.remaining_capacity(), Ok(remaining));
    assert_eq!(gate.retained_packets(), Ok(raw.len()));
    gate.prepare_window(end(1), Duration::ZERO).unwrap();
    assert_eq!(gate.remaining_capacity(), Ok(remaining));
    assert_eq!(gate.retained_packets(), Ok(raw.len() - end(1)));
    expose_and_ack(&mut gate);
    assert_eq!(gate.remaining_capacity(), Ok(remaining + end(1) * 188));
}

#[test]
fn asynchronous_deadline_query_and_idle_polling_preserve_inclusive_delivery_limit() {
    let mut gate = gate();
    assert_eq!(gate.next_deadline(), Ok(None));
    gate.push(&[0x47], Duration::ZERO).unwrap();
    let inclusive = Duration::from_millis(120);
    let expiry = inclusive.checked_add(Duration::from_nanos(1)).unwrap();
    assert_eq!(gate.next_deadline(), Ok(Some(expiry)));
    gate.check_deadline(inclusive).unwrap();
    let error = TeletextOutputError::Deadline;
    assert_eq!(gate.check_deadline(expiry), Err(error.clone()));
    fenced(&mut gate, error);

    let mut gate = TeletextOutputGate::new(config(), set()).unwrap();
    prepared(&mut gate, &source(3));
    assert_eq!(gate.next_deadline(), Ok(Some(expiry)));
    let schedule = gate.pending_schedule().unwrap();
    gate.pending_packets(Duration::ZERO).unwrap();
    let final_expiry = schedule
        .last_due()
        .checked_add(config().max_lateness)
        .unwrap()
        .checked_add(Duration::from_nanos(1))
        .unwrap();
    assert_eq!(gate.next_deadline(), Ok(Some(final_expiry)));
}

#[test]
fn asynchronous_packet_schedule_preserves_absolute_fractional_position_rounding() {
    let mut policy = config();
    policy.clock.muxrate_bps = 5_000_003;
    policy.clock.rate_tolerance_ppm = 1;
    let mut gate = TeletextOutputGate::new(policy, set()).unwrap();
    prepared(&mut gate, &source(3));
    expose_and_ack(&mut gate);
    let now = Duration::from_millis(121);
    gate.prepare_window(end(2) - end(1), now).unwrap();
    let schedule = gate.pending_schedule().unwrap();
    for index in 0..schedule.packet_count() {
        let position = schedule.first_packet() + u64::try_from(index).unwrap();
        let nanos = position * 188 * 8 * 1_000_000_000 / policy.clock.muxrate_bps;
        let expected = policy
            .playout_delay
            .checked_add(Duration::from_nanos(nanos))
            .unwrap();
        assert_eq!(schedule.packet_due(index), Ok(expected));
    }
    assert_eq!(schedule.packet_due(0), Ok(schedule.first_due()));
    assert_eq!(
        schedule.packet_due(schedule.packet_count() - 1),
        Ok(schedule.last_due())
    );
    assert_eq!(
        schedule.packet_due(schedule.packet_count()),
        Err(TeletextOutputError::Boundary)
    );
}

#[test]
fn extended_interval_queries_preserve_first_pcr_physical_origin_and_bounds() {
    let mut gate = gate();
    assert_eq!(gate.window_end(2), Ok(None));
    assert_eq!(gate.window_end(0), Err(TeletextOutputError::Boundary));
    assert_eq!(gate.window_end(32769), Err(TeletextOutputError::Boundary));
    gate.push(&bytes(&source(3)), Duration::ZERO).unwrap();
    assert_eq!(gate.window_end(1), Ok(Some(u64::try_from(end(1)).unwrap())));
    assert_eq!(gate.window_end(2), Ok(Some(u64::try_from(end(2)).unwrap())));
    assert_eq!(gate.window_end(3), Ok(Some(u64::try_from(end(3)).unwrap())));
    prepared_window_interval_origin_after_ack(&mut gate);
}

fn prepared_window_interval_origin_after_ack(gate: &mut TeletextOutputGate) {
    gate.prepare_window(end(1), Duration::ZERO).unwrap();
    expose_and_ack(gate);
    assert_eq!(gate.window_end(1), Ok(Some(u64::try_from(end(2)).unwrap())));
    assert_eq!(gate.window_end(2), Ok(Some(u64::try_from(end(3)).unwrap())));
}

#[test]
fn admission_and_clock_evidence_precede_any_prepared_output() {
    let mut gate = gate();
    assert_eq!(gate.next_window_end(), Ok(None));
    assert!(!gate.prepare_window(1, Duration::ZERO).unwrap());
    let raw = source(3);
    gate.push(&bytes(&raw[..3]), Duration::ZERO).unwrap();
    assert_eq!(
        gate.next_window_end(),
        Ok(Some(u64::try_from(end(1)).unwrap()))
    );
    assert!(!gate.prepare_window(1, Duration::ZERO).unwrap());
    gate.push(&bytes(&raw[3..12]), Duration::ZERO).unwrap();
    assert!(!gate.prepare_window(1, Duration::ZERO).unwrap());
    gate.push(&bytes(&raw[12..]), Duration::ZERO).unwrap();
    assert!(gate.prepare_window(end(1), Duration::ZERO).unwrap());
    let output = gate.pending_packets(Duration::ZERO).unwrap();
    assert!(output
        .iter()
        .any(|raw| u16::from(Packet::new(raw).pid()) == 512));
}

#[test]
fn arbitrary_byte_splits_preserve_raw_accounting_and_first_byte_origin() {
    let raw = source(3);
    let data = bytes(&raw);
    for cut in [1, 3, 4, 187, 188, 189, 377, data.len() - 1] {
        let mut gate = gate();
        gate.push(&data[..cut], Duration::from_millis(5)).unwrap();
        gate.push(&data[cut..], Duration::from_millis(6)).unwrap();
        assert!(gate
            .prepare_window(end(1), Duration::from_millis(6))
            .unwrap());
        let schedule = gate.pending_schedule().unwrap();
        assert_eq!(schedule.first_packet(), 0);
        assert_eq!(schedule.first_due(), Duration::from_millis(105));
        assert_eq!(schedule.packet_count(), end(1));
        assert_eq!(
            gate.clock.report().unwrap().packets_seen,
            u64::try_from(raw.len()).unwrap()
        );
        assert_eq!(gate.observed, raw.len() - end(1));
    }
}

#[test]
fn prepared_windows_preserve_non_null_non_pmt_packets_and_complete_two_cycles() {
    let raw = source(6);
    let mut gate = gate();
    gate.push(&bytes(&raw), Duration::ZERO).unwrap();
    let original_clock = gate.clock.report().unwrap();
    let mut output = Vec::new();
    let mut start = 0;
    let mut now = Duration::ZERO;
    for frame in 1..=6 {
        assert!(gate.prepare_window(end(frame) - start, now).unwrap());
        now = gate.pending_schedule().unwrap().last_due();
        let next = expose_and_ack(&mut gate);
        let emitted_pids: Vec<_> = next
            .iter()
            .map(|raw| u16::from(Packet::new(raw).pid()))
            .collect();
        assert!(emitted_pids.contains(&512));
        output.extend(next);
        start = end(frame);
    }
    assert_eq!(output.len(), raw.len());
    for (before, after) in raw.iter().zip(&output) {
        if ![8191, 3337].contains(&u16::from(Packet::new(before).pid())) {
            assert_eq!(before, after);
        }
    }
    assert_eq!(gate.clock.report().unwrap(), original_clock);
    assert!(gate.carousel.is_cycle_boundary());
    let finish_at = Duration::from_millis(250);
    gate.finish(finish_at).unwrap();
    gate.finish(finish_at).unwrap();
    assert_eq!(gate.push(&[], finish_at), Err(TeletextOutputError::Closed));
}

#[test]
fn source_clock_wrap_preserves_carousel_pts_and_physical_boundaries() {
    let mut raw = source(3);
    let wrap = (1_u64 << 33) * 300;
    for (sample, position) in (2..raw.len()).step_by(5).enumerate() {
        raw[position] = pcr((wrap - 1_000 + u64::try_from(sample).unwrap() * 40_608) % wrap);
    }
    let mut gate = gate();
    prepared(&mut gate, &raw);
    let output = gate.pending_packets(Duration::ZERO).unwrap();
    let payload = output
        .iter()
        .find(|raw| u16::from(Packet::new(*raw).pid()) == 512)
        .unwrap();
    let pes = Packet::new(payload).payload().unwrap();
    let pts = ((u64::from(pes[9]) & 0x0e) << 29)
        | (u64::from(pes[10]) << 22)
        | ((u64::from(pes[11]) & 0xfe) << 14)
        | (u64::from(pes[12]) << 7)
        | (u64::from(pes[13]) >> 1);
    assert_eq!(pts, ((wrap - 1_000) / 300 + 1800) & ((1 << 33) - 1));
}

#[test]
fn pending_window_requires_actual_delivery_acknowledgement_before_next_prepare() {
    let mut gate = gate();
    prepared(&mut gate, &source(3));
    let error = TeletextOutputError::Pending;
    assert_eq!(gate.prepare_window(1, Duration::ZERO), Err(error.clone()));
    fenced(&mut gate, error);
}

#[test]
fn foreign_stale_unexposed_and_early_acknowledgements_permanently_fence() {
    for kind in 0..4 {
        let mut gate = gate();
        let raw = source(3);
        prepared(&mut gate, &raw);
        let mut other = TeletextOutputGate::new(config(), set()).unwrap();
        prepared(&mut other, &raw);
        let schedule = if kind == 0 {
            other.pending_schedule().unwrap()
        } else {
            gate.pending_schedule().unwrap()
        };
        let at = schedule.last_due();
        if kind != 2 {
            gate.pending_packets(Duration::ZERO).unwrap();
        }
        if kind == 1 {
            let valid = gate.pending_schedule().unwrap();
            gate.acknowledge_window(valid, at).unwrap();
            assert!(gate.prepare_window(end(2) - end(1), at).unwrap());
            gate.pending_packets(at).unwrap();
        }
        let now = match kind {
            2 => schedule.first_due(),
            3 => Duration::ZERO,
            _ => at,
        };
        let error = TeletextOutputError::Acknowledgement;
        assert_eq!(gate.acknowledge_window(schedule, now), Err(error.clone()));
        fenced(&mut gate, error);
    }
}

#[test]
fn prepared_and_source_bytes_share_one_total_retention_bound_including_tail() {
    let raw = source(3);
    let mut policy = config();
    policy.max_buffer_packets = raw.len();
    let mut gate = TeletextOutputGate::new(policy, set()).unwrap();
    prepared(&mut gate, &raw);
    let error = TeletextOutputError::Buffer(TeletextBufferError::Capacity);
    assert_eq!(gate.push(&[0x47], Duration::ZERO), Err(error.clone()));
    fenced(&mut gate, error);
}

#[test]
fn exposed_stalled_write_expires_at_final_due_time_in_idle_polling() {
    let mut gate = gate();
    prepared(&mut gate, &source(3));
    let last = gate.pending_schedule().unwrap().last_due();
    gate.pending_packets(Duration::ZERO).unwrap();
    let deadline = last.checked_add(config().max_lateness).unwrap();
    gate.check_deadline(deadline).unwrap();
    let late = deadline.checked_add(Duration::from_nanos(1)).unwrap();
    let error = TeletextOutputError::Deadline;
    assert_eq!(gate.check_deadline(late), Err(error.clone()));
    fenced(&mut gate, error);
}

#[test]
fn unexposed_and_unprepared_output_cannot_catch_up_after_delivery_deadline() {
    for pending in [false, true] {
        let mut gate = gate();
        gate.push(&bytes(&source(3)), Duration::ZERO).unwrap();
        if pending {
            gate.prepare_window(end(1), Duration::ZERO).unwrap();
        }
        let deadline = config()
            .playout_delay
            .checked_add(config().max_lateness)
            .unwrap();
        let late = deadline.checked_add(Duration::from_nanos(1)).unwrap();
        let error = TeletextOutputError::Deadline;
        let result = if pending {
            gate.check_deadline(late)
        } else {
            gate.prepare_window(end(1), late).map(|_| ())
        };
        assert_eq!(result, Err(error.clone()));
        fenced(&mut gate, error);
    }
}

#[test]
fn retained_first_byte_age_is_not_reset_when_preparing_a_sink_window() {
    let mut gate = gate();
    gate.push(&bytes(&source(1)), Duration::ZERO).unwrap();
    gate.prepare_window(end(1), Duration::from_millis(50))
        .unwrap();
    assert_eq!(gate.pending.as_ref().unwrap().first_arrival, Duration::ZERO);
    gate.pending_packets(Duration::from_millis(50)).unwrap();
    let error = TeletextOutputError::Deadline;
    assert_eq!(gate.check_deadline(config().max_hold), Err(error.clone()));
    fenced(&mut gate, error);
}

#[test]
fn partial_packet_idle_deadline_and_time_order_are_terminal() {
    for (now, error) in [
        (Duration::from_millis(510), TeletextBufferError::Deadline),
        (Duration::from_millis(9), TeletextBufferError::TimeOrder),
    ] {
        let mut gate = gate();
        gate.push(&[0x47], Duration::from_millis(10)).unwrap();
        let error = TeletextOutputError::Buffer(error);
        assert_eq!(gate.check_deadline(now), Err(error.clone()));
        fenced(&mut gate, error);
    }
}

#[test]
fn changed_source_or_observed_owned_pid_discards_prepared_unreleased_output() {
    for (pid, raw) in [
        (
            3337,
            one(3337, 1, &revise(pmt(10, 300, 300, &[]), |s| s[5] += 2)),
        ),
        (512, packet(512, false, 0, &[0xff; 184])),
    ] {
        let mut gate = gate();
        prepared(&mut gate, &source(3));
        let error = if pid == 3337 {
            TeletextOutputError::Psi(TeletextPsiError::SourceChanged { pid })
        } else {
            TeletextOutputError::Psi(TeletextMultiplexError::PidCollision { pid }.into())
        };
        assert_eq!(gate.push(&raw, Duration::ZERO), Err(error.clone()));
        fenced(&mut gate, error);
    }
}

#[test]
fn wrong_selected_pcr_identity_never_enables_authored_service() {
    let mut raw = source(3);
    raw[1] = one(3337, 0, &pmt(10, 301, 300, &[]));
    let mut gate = gate();
    let error = TeletextOutputError::Multiplex(TeletextMultiplexError::Clock);
    assert_eq!(gate.push(&bytes(&raw), Duration::ZERO), Err(error.clone()));
    fenced(&mut gate, error);
}

#[test]
fn malformed_or_wrong_rate_input_never_yields_pending_packets() {
    for clock_failure in [false, true] {
        let mut raw = source(3);
        let error = if clock_failure {
            raw[7] = pcr(900_000 + 40_609);
            TeletextOutputError::Clock(TeletextClockError::Muxrate)
        } else {
            raw.last_mut().unwrap()[0] = 0;
            TeletextOutputError::Clock(TeletextClockError::Framing)
        };
        let mut gate = gate();
        assert_eq!(gate.push(&bytes(&raw), Duration::ZERO), Err(error.clone()));
        fenced(&mut gate, error);
    }
}

#[test]
fn interval_null_capacity_failure_never_exposes_partial_rewriting() {
    let mut raw = source(3);
    for slot in &mut raw[3..end(1)] {
        if u16::from(Packet::new(slot).pid()) == 8191 {
            *slot = packet(300, false, 0, &[0x55; 184]);
        }
    }
    let mut gate = gate();
    gate.push(&bytes(&raw), Duration::ZERO).unwrap();
    let error = TeletextOutputError::Multiplex(TeletextMultiplexError::Capacity {
        needed: 1,
        available: 0,
    });
    assert_eq!(
        gate.prepare_window(end(1), Duration::ZERO),
        Err(error.clone())
    );
    fenced(&mut gate, error);
}

#[test]
fn non_boundary_and_out_of_range_prefixes_cannot_shift_or_drop_intervals() {
    for count in [0, end(1) - 1, end(3) + 1] {
        let mut gate = gate();
        gate.push(&bytes(&source(3)), Duration::ZERO).unwrap();
        let error = TeletextOutputError::Boundary;
        assert_eq!(
            gate.prepare_window(count, Duration::ZERO),
            Err(error.clone())
        );
        fenced(&mut gate, error);
    }
}

#[test]
fn eof_requires_drained_complete_bytes_and_at_least_one_acknowledged_complete_cycle() {
    for kind in 0..4 {
        let mut gate = gate();
        let error = match kind {
            0 => {
                gate.push(&[0x47], Duration::ZERO).unwrap();
                TeletextOutputError::Buffer(TeletextBufferError::PartialPacket)
            }
            1 => {
                gate.push(&bytes(&source(3)), Duration::ZERO).unwrap();
                TeletextOutputError::Buffer(TeletextBufferError::RetainedBytes)
            }
            2 => {
                prepared(&mut gate, &source(3));
                TeletextOutputError::Pending
            }
            _ => {
                gate.push(&bytes(&source(1)), Duration::ZERO).unwrap();
                gate.prepare_window(end(1), Duration::ZERO).unwrap();
                expose_and_ack(&mut gate);
                TeletextOutputError::Unterminated
            }
        };
        let now = if kind == 3 {
            Duration::from_millis(150)
        } else {
            Duration::ZERO
        };
        assert_eq!(gate.finish(now), Err(error.clone()));
        fenced(&mut gate, error);
    }
}

#[test]
fn successful_eof_preserves_monotonic_time_on_repeated_close() {
    let mut gate = gate();
    gate.push(&bytes(&source(3)), Duration::ZERO).unwrap();
    gate.prepare_window(end(3), Duration::ZERO).unwrap();
    expose_and_ack(&mut gate);
    gate.finish(Duration::from_millis(200)).unwrap();
    let error = TeletextOutputError::Buffer(TeletextBufferError::TimeOrder);
    assert_eq!(gate.finish(Duration::from_millis(199)), Err(error.clone()));
    fenced(&mut gate, error);
}
