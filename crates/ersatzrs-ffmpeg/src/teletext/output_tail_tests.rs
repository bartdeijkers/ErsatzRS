use super::tests::{bytes, config, end, expose_and_ack, pcr, set, source};
use super::*;
use crate::teletext::psi_tests::{one, pmt};

fn gate_with(raw: &[[u8; 188]]) -> TeletextOutputGate {
    let mut gate = TeletextOutputGate::new(config(), set()).unwrap();
    gate.push(&bytes(raw), Duration::ZERO).unwrap();
    gate
}

fn fractional_source(complete_intervals: u64, tail_packets: usize) -> Vec<[u8; 188]> {
    let mut raw = source(complete_intervals + 1);
    raw.truncate(end(complete_intervals) + tail_packets);
    raw
}

fn assert_source_preserved(raw: &[[u8; 188]], output: &[[u8; 188]]) {
    assert_eq!(raw.len(), output.len());
    for (before, after) in raw.iter().zip(output) {
        let pid = (u16::from(before[1] & 0x1f) << 8) | u16::from(before[2]);
        if pid != 8191 && pid != 3337 {
            assert_eq!(before, after);
        }
    }
}

#[test]
fn eof_preserves_fractional_transport_tail_without_padding_or_partial_interval_insertion() {
    for tail_packets in [1, 7, 25, end(4) - end(3) - 1] {
        let raw = fractional_source(3, tail_packets);
        let mut gate = gate_with(&raw);
        assert!(gate.prepare_final_window(Duration::ZERO).unwrap());
        let schedule = gate.pending_schedule().unwrap();
        assert_eq!(schedule.first_packet(), 0);
        assert_eq!(schedule.packet_count(), raw.len());
        assert_eq!(schedule.first_due(), config().playout_delay);
        let expected_last = config().playout_delay
            + Duration::from_nanos(
                u64::try_from(raw.len() - 1).unwrap() * 188 * 8 * 1_000_000_000 / 5_000_000,
            );
        assert_eq!(schedule.last_due(), expected_last);
        let output = expose_and_ack(&mut gate);
        assert_source_preserved(&raw, &output);
        assert_eq!(&output[end(3)..], &raw[end(3)..]);
        assert_eq!(gate.finish(expected_last), Ok(()));
        assert_eq!(gate.finish(expected_last), Ok(()));
    }
}

#[test]
fn a_cycle_acknowledged_inside_a_larger_window_allows_later_repetition_to_end_midcycle() {
    for intervals in [4, 5, 7, 8] {
        let raw = source(intervals);
        let mut gate = gate_with(&raw);
        assert!(gate.prepare_window(raw.len(), Duration::ZERO).unwrap());
        let last_due = gate.pending_schedule().unwrap().last_due();
        let output = expose_and_ack(&mut gate);
        assert_source_preserved(&raw, &output);
        assert!(!gate.carousel.as_ref().unwrap().is_cycle_boundary());
        assert_eq!(gate.finish(last_due), Ok(()));
    }
}

#[test]
fn fractional_tail_after_an_acknowledged_cycle_is_delivered_at_its_original_positions() {
    let raw = fractional_source(3, 17);
    let mut gate = gate_with(&raw);
    assert!(gate.prepare_window(end(3), Duration::ZERO).unwrap());
    let first_end = gate.pending_schedule().unwrap().last_due();
    expose_and_ack(&mut gate);
    assert!(gate.prepare_final_window(first_end).unwrap());
    let schedule = gate.pending_schedule().unwrap();
    assert_eq!(schedule.first_packet(), u64::try_from(end(3)).unwrap());
    assert_eq!(schedule.packet_count(), 17);
    let last_due = schedule.last_due();
    let tail = expose_and_ack(&mut gate);
    assert_eq!(tail, raw[end(3)..]);
    assert_eq!(gate.finish(last_due), Ok(()));
}

#[test]
fn initial_incomplete_cycle_is_rejected_before_any_tail_bytes_can_be_exposed() {
    for intervals in [0, 1, 2] {
        let raw = fractional_source(intervals, 20);
        let mut gate = gate_with(&raw);
        assert_eq!(
            gate.prepare_final_window(Duration::ZERO),
            Err(TeletextOutputError::Unterminated)
        );
        assert!(gate.pending_schedule().is_err());
        assert!(gate.pending_packets(Duration::ZERO).is_err());
    }
}

#[test]
fn prepared_final_terminator_still_requires_actual_sink_acknowledgement() {
    let raw = fractional_source(3, 12);
    let mut gate = gate_with(&raw);
    assert!(gate.prepare_final_window(Duration::ZERO).unwrap());
    assert_eq!(
        gate.finish(Duration::ZERO),
        Err(TeletextOutputError::Pending)
    );
    assert!(gate.pending_packets(Duration::ZERO).is_err());
}

#[test]
fn late_final_window_acknowledgement_fences_retained_output() {
    let raw = fractional_source(3, 12);
    let mut gate = gate_with(&raw);
    assert!(gate.prepare_final_window(Duration::ZERO).unwrap());
    let schedule = gate.pending_schedule().unwrap();
    let first_due = schedule.first_due();
    let expired = schedule.last_due() + config().max_lateness + Duration::from_nanos(1);
    gate.pending_packets(first_due).unwrap();
    assert_eq!(
        gate.acknowledge_window(schedule, expired),
        Err(TeletextOutputError::Deadline)
    );
    assert_eq!(gate.finish(expired), Err(TeletextOutputError::Deadline));
    assert!(gate.pending.is_none());
}

#[test]
fn eof_seal_refuses_new_bytes_and_regular_preparation_without_reopening_output() {
    for action in 0..2 {
        let raw = fractional_source(3, 12);
        let mut gate = gate_with(&raw);
        assert!(gate.prepare_final_window(Duration::ZERO).unwrap());
        let failure = if action == 0 {
            gate.push(&raw[0], Duration::ZERO).unwrap_err()
        } else {
            gate.prepare_window(1, Duration::ZERO).unwrap_err()
        };
        assert_eq!(gate.finish(Duration::ZERO), Err(failure));
        assert!(gate.pending.is_none());
    }
}

#[test]
fn empty_eof_after_cycle_ack_seals_without_manufacturing_an_output_window() {
    for attempt_new_input in [false, true] {
        let raw = source(3);
        let mut gate = gate_with(&raw);
        gate.prepare_window(raw.len(), Duration::ZERO).unwrap();
        let last_due = gate.pending_schedule().unwrap().last_due();
        expose_and_ack(&mut gate);
        assert!(!gate.prepare_final_window(last_due).unwrap());
        assert!(gate.pending.is_none());
        if attempt_new_input {
            assert!(gate.push(&[], last_due).is_err());
        } else {
            assert_eq!(gate.finish(last_due), Ok(()));
            assert_eq!(gate.finish(last_due), Ok(()));
        }
    }
}

#[test]
fn eof_rejects_partial_transport_packet_instead_of_truncating_it() {
    let raw = source(3);
    let mut gate = gate_with(&raw);
    gate.push(&[0x47, 0x1f], Duration::ZERO).unwrap();
    assert_eq!(
        gate.prepare_final_window(Duration::ZERO),
        Err(TeletextOutputError::Buffer(
            TeletextBufferError::PartialPacket
        ))
    );
    assert!(gate.pending.is_none());
}

#[test]
fn eof_rejects_an_incomplete_selected_psi_section_before_tail_exposure() {
    let mut raw = fractional_source(3, 5);
    let partial = &pmt(10, 300, 300, &[])[..9];
    *raw.last_mut().unwrap() = one(3337, 1, partial);
    let mut gate = gate_with(&raw);
    assert!(matches!(
        gate.prepare_final_window(Duration::ZERO),
        Err(TeletextOutputError::Psi(TeletextPsiError::Incomplete))
    ));
    assert!(gate.pending.is_none());
}

#[test]
fn eof_rejects_missing_or_insufficient_clock_evidence_without_releasing_source() {
    for samples in [0, 1, 2] {
        let mut raw = source(3);
        for position in (2..raw.len()).step_by(5).skip(samples) {
            raw[position] = raw[3];
        }
        let mut policy = config();
        policy.clock.max_pcr_gap_packets = 1024;
        let mut gate = TeletextOutputGate::new(policy, set()).unwrap();
        gate.push(&bytes(&raw), Duration::ZERO).unwrap();
        assert!(gate.prepare_final_window(Duration::ZERO).is_err());
        assert!(gate.pending.is_none());
    }
}

#[test]
fn final_pmt_growth_cannot_borrow_a_nonexistent_null_slot_or_pad_the_source() {
    let mut descriptors = vec![0x80, 158];
    descriptors.extend_from_slice(&[0; 158]);
    let section = pmt(10, 300, 300, &descriptors);
    assert_eq!(section.len(), 181);
    let mut raw = fractional_source(3, 1);
    raw[1] = one(3337, 0, &section);
    *raw.last_mut().unwrap() = one(3337, 1, &section);
    let mut gate = gate_with(&raw);
    assert!(gate.prepare_window(end(3), Duration::ZERO).unwrap());
    let now = gate.pending_schedule().unwrap().last_due();
    expose_and_ack(&mut gate);
    assert!(matches!(
        gate.prepare_final_window(now),
        Err(TeletextOutputError::Pmt(
            TeletextPmtInstallError::Multiplex(TeletextMultiplexError::Capacity { .. })
        ))
    ));
    assert!(gate.pending.is_none());
}

#[test]
fn final_fractional_tail_does_not_forge_a_pcr_or_change_the_original_clock() {
    let raw = fractional_source(3, 16);
    let mut gate = gate_with(&raw);
    let before = gate.clock.report().unwrap();
    assert!(gate.prepare_final_window(Duration::ZERO).unwrap());
    let output = expose_and_ack(&mut gate);
    assert_eq!(gate.clock.report().unwrap(), before);
    let last_sample = (2..raw.len()).step_by(5).next_back().unwrap();
    let sample = u64::try_from((last_sample - 2) / 5).unwrap();
    assert_eq!(output[last_sample], pcr(900_000 + sample * 40_608));
}
