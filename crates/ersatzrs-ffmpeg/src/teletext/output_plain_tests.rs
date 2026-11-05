use std::time::Duration;

use mpeg2ts_reader::packet::Packet;

use super::{tests::*, *};
use crate::teletext::psi_tests::{one, pmt};

const PMT_PID: u16 = 3337;

fn starve(raw: &mut [[u8; 188]], intervals: std::ops::Range<u64>) {
    for slot in &mut raw[end(intervals.start)..end(intervals.end)] {
        if u16::from(Packet::new(slot).pid()) == 8191 {
            *slot = crate::teletext::psi_tests::packet(300, false, 0, &[0x55; 184]);
        }
    }
}

fn pid(raw: &[u8; 188]) -> u16 {
    u16::from(Packet::new(raw).pid())
}

fn pmt_version(raw: &[u8; 188]) -> u8 {
    let payload = Packet::new(raw).payload().unwrap();
    let section = &payload[1 + usize::from(payload[0])..];
    (section[5] >> 1) & 0x1f
}

fn pmt_cc(raw: &[u8; 188]) -> u8 {
    Packet::new(raw).continuity_counter().count()
}

fn packets(bytes: &[u8]) -> Vec<[u8; 188]> {
    bytes
        .chunks_exact(188)
        .map(|chunk| chunk.try_into().unwrap())
        .collect()
}

/// Deliver one-interval windows until the gate fails; return delivered packets.
fn deliver_until_failure(
    gate: &mut TeletextOutputGate,
    raw: &[[u8; 188]],
    frames: u64,
) -> (Vec<[u8; 188]>, TeletextOutputError) {
    gate.push(&bytes(raw), Duration::ZERO).unwrap();
    let (mut delivered, mut start, mut now) = (Vec::new(), 0, Duration::ZERO);
    for frame in 1..=frames {
        match gate.prepare_window(end(frame) - start, now) {
            Ok(prepared) => assert!(prepared),
            Err(error) => return (delivered, error),
        }
        start = end(frame);
        now = gate.pending_schedule().unwrap().last_due();
        delivered.extend(expose_and_ack(gate));
    }
    panic!("the gate did not fail");
}

fn sustained_failure() -> (TeletextOutputGate, Vec<[u8; 188]>, Vec<[u8; 188]>) {
    let mut raw = source(9);
    // The header leaves in the first interval; the page then starves for
    // longer than the B.9 spacing, after which nulls return.
    starve(&mut raw, 1..6);
    let mut gate = TeletextOutputGate::new(config(), set()).unwrap();
    gate.push(&bytes(&raw[..end(1)]), Duration::ZERO).unwrap();
    assert!(gate.prepare_window(end(1), Duration::ZERO).unwrap());
    let mut now = gate.pending_schedule().unwrap().last_due();
    let mut delivered = expose_and_ack(&mut gate);
    gate.push(&bytes(&raw[end(1)..]), now).unwrap();
    for frame in 2..=9 {
        match gate.prepare_window(end(frame) - end(frame - 1), now) {
            Ok(prepared) => assert!(prepared),
            Err(TeletextOutputError::Multiplex(TeletextMultiplexError::SustainedCapacity {
                ..
            })) => return (gate, delivered, raw),
            Err(error) => panic!("unexpected {error}"),
        }
        now = gate.pending_schedule().unwrap().last_due();
        delivered.extend(expose_and_ack(&mut gate));
    }
    panic!("the gate did not fail");
}

#[test]
fn audiovisual_output_continues_exactly_after_a_sustained_capacity_failure() {
    let (mut gate, delivered, raw) = sustained_failure();
    let mut plain = gate.take_plain_continuation().unwrap();
    assert!(gate.take_plain_continuation().is_none());
    let continued = packets(&plain.push(&[]).unwrap());
    assert_eq!(plain.held_bytes(), 0);
    let mut output = delivered.clone();
    output.extend(&continued);
    // Every source packet keeps its physical position; nothing is dropped.
    assert_eq!(output.len(), raw.len());
    for (before, after) in raw.iter().zip(&output) {
        if ![8191, PMT_PID].contains(&pid(before)) {
            assert_eq!(before, after);
        }
    }
    // The authored stream stops at the failure and its PMT entry is withdrawn
    // with a newer version and continuing output continuity.
    assert!(continued.iter().all(|raw| pid(raw) != 512));
    let announced = delivered.iter().rfind(|raw| pid(raw) == PMT_PID).unwrap();
    let withdrawn = continued.iter().find(|raw| pid(raw) == PMT_PID).unwrap();
    assert_eq!(pmt_version(announced), 1);
    assert_eq!(pmt_version(withdrawn), 2);
    assert_eq!(pmt_cc(withdrawn), (pmt_cc(announced) + 1) & 15);
    assert!(plain.finish().unwrap().is_empty());
}

#[test]
fn later_source_pmt_repetitions_stay_withdrawn_and_split_reads_are_preserved() {
    let (mut gate, _, _) = sustained_failure();
    let mut plain = gate.take_plain_continuation().unwrap();
    let first = packets(&plain.push(&[]).unwrap());
    let last = first.iter().rfind(|raw| pid(raw) == PMT_PID).unwrap();
    let mut next = vec![crate::teletext::psi_tests::packet(8191, false, 0, &[0xff; 184]); 4];
    next[1] = one(PMT_PID, 1, &pmt(10, 300, 300, &[]));
    next[2] = crate::teletext::psi_tests::packet(300, false, 0, &[0x33; 184]);
    let raw = bytes(&next);
    let mut output = Vec::new();
    for chunk in raw.chunks(77) {
        output.extend(plain.push(chunk).unwrap());
    }
    output.extend(plain.finish().unwrap());
    let output = packets(&output);
    assert_eq!(output.len(), 4);
    assert_eq!(output[2], next[2]);
    assert_eq!(pmt_version(&output[1]), 2);
    assert_eq!(pmt_cc(&output[1]), (pmt_cc(last) + 1) & 15);
}

#[test]
fn a_withdrawal_without_room_waits_for_input_and_eof_keeps_the_tail() {
    let frames = u64::from(super::MAX_PAGE_START_DEFERRED_INTERVALS) + 2;
    let mut raw = source(frames);
    starve(&mut raw, 1..frames);
    let mut config = config();
    config.max_buffer_packets = 20_000;
    config.max_hold = Duration::from_secs(10);
    let mut gate = TeletextOutputGate::new(config, set()).unwrap();
    let (delivered, error) = deliver_until_failure(&mut gate, &raw, frames);
    assert!(matches!(
        error,
        TeletextOutputError::Multiplex(TeletextMultiplexError::SustainedCapacity { .. })
    ));
    let mut plain = gate.take_plain_continuation().unwrap();
    // No null packet and no source PMT: nothing can carry the withdrawal yet.
    assert!(plain.push(&[]).unwrap().is_empty());
    let tail = packets(&plain.finish().unwrap());
    assert_eq!(delivered.len() + tail.len(), raw.len());
    assert_eq!(tail, raw[delivered.len()..]);
}

#[test]
fn retention_failures_and_exposed_windows_have_no_continuation() {
    // A holding deadline discards the retained bytes themselves.
    let mut gate = TeletextOutputGate::new(config(), set()).unwrap();
    gate.push(&bytes(&source(2)), Duration::ZERO).unwrap();
    assert!(gate.check_deadline(Duration::from_secs(5)).is_err());
    assert!(gate.take_plain_continuation().is_none());

    // A failure while a window is exposed cannot know what the sink received.
    let mut gate = TeletextOutputGate::new(config(), set()).unwrap();
    gate.push(&bytes(&source(2)), Duration::ZERO).unwrap();
    assert!(gate.prepare_window(end(1), Duration::ZERO).unwrap());
    let schedule = gate.pending_schedule().unwrap();
    gate.pending_packets(schedule.first_due()).unwrap();
    assert!(gate.check_deadline(Duration::from_secs(5)).is_err());
    assert!(gate.take_plain_continuation().is_none());
}
