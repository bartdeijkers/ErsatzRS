//! Initially absent authored output retains the same physical source epoch.
use super::*;
use crate::teletext::psi_tests::{packet, pmt};

fn policy() -> TeletextOutputConfig {
    let mut value = tests::config();
    value.max_buffer_packets = 32_768;
    value.clock.max_window_packets = 32_768;
    value.max_hold = Duration::from_secs(10);
    value
}

fn source(frames: u64) -> Vec<[u8; 188]> {
    let mut value = tests::source(frames);
    value[3] = packet(300, false, 0, &[0x31; 184]);
    value
}

fn pid(packet: &[u8; 188]) -> u16 {
    u16::from(Packet::new(packet).pid())
}

fn deliver(
    gate: &mut TeletextOutputGate,
) -> (TeletextOutputAcknowledgement, Vec<[u8; 188]>, Duration) {
    let end = gate.next_window_end().unwrap().unwrap();
    let count = usize::try_from(end - gate.source_start).unwrap();
    let now = gate.due_at(gate.source_start).unwrap();
    assert!(gate.prepare_window(count, now).unwrap());
    let schedule = gate.pending_schedule().unwrap();
    let delivered = gate.pending_packets(schedule.first_due()).unwrap().to_vec();
    let at = schedule.last_due();
    let receipt = gate.acknowledge_window_with_receipt(schedule, at).unwrap();
    (receipt, delivered, at)
}

#[test]
fn initial_absence_forwards_complete_source_byte_exact_and_closes_without_authored_cycle() {
    let mut source = source(4);
    // Preserve the real startup PMT reset too, rather than copying only a
    // canonical unflagged table which an ordinary installer could rebuild.
    assert_eq!(source[1][3] & 0x30, 0x30);
    assert!(source[1][4] > 0);
    source[1][5] |= 0x80;
    let mut gate = TeletextOutputGate::new_absent(policy()).unwrap();
    gate.push(&tests::bytes(&source), Duration::ZERO).unwrap();
    let clock = gate.clock.report().unwrap();
    let mut emitted = Vec::new();
    let mut now = Duration::ZERO;
    for _ in 0..4 {
        let (receipt, packets, at) = deliver(&mut gate);
        assert!(receipt.is_cycle_boundary());
        assert!(!receipt.contains_complete_cycle());
        assert!(!receipt.signalling_announced());
        assert!(packets.iter().all(|packet| pid(packet) != 512));
        emitted.extend(packets);
        now = at;
    }
    assert_eq!(emitted, source);
    assert_eq!(gate.clock.report().unwrap(), clock);
    assert!(gate.service.is_none() && gate.carousel.is_none());
    assert!(!gate.authored_payload_selected());
    assert!(!gate.signalling_transition_pending());
    assert_eq!(
        gate.installer.as_ref().unwrap().next_continuity_counter(),
        Some(1)
    );
    gate.finish(now).unwrap();
    gate.finish(now).unwrap();
    assert_eq!(gate.push(&[], now), Err(TeletextOutputError::Closed));
}

#[test]
fn first_activation_uses_same_acknowledged_epoch_and_pmt_counter_before_pes() {
    let source = source(12);
    let mut gate = TeletextOutputGate::new_absent(policy()).unwrap();
    gate.push(&tests::bytes(&source), Duration::ZERO).unwrap();
    let (receipt, ordinary, now) = deliver(&mut gate);
    assert_eq!(ordinary, source[..ordinary.len()]);
    let service = transition_tests::simple("nld", None);
    let epoch = Arc::clone(&gate.epoch);
    let clock = gate.clock.report().unwrap();
    let anchor = gate.anchor;
    let tick = gate.batch_ticks;
    gate.replace_authored_at_acknowledged_boundary(&receipt, &service, now)
        .unwrap();
    assert!(Arc::ptr_eq(&gate.epoch, &epoch));
    assert_eq!(gate.clock.report().unwrap(), clock);
    assert_eq!(gate.anchor, anchor);
    assert_eq!(gate.batch_ticks, tick);
    assert!(gate.signalling_transition_pending());
    let (announcement, packets, _) = deliver(&mut gate);
    assert!(announcement.signalling_announced());
    assert!(packets.iter().all(|packet| pid(packet) != 512));
    let pmt = packets.iter().find(|packet| pid(packet) == 3337).unwrap();
    assert_eq!(pmt[3] & 15, 1);
    let section = Packet::new(pmt).payload().unwrap();
    assert_eq!((section[6] >> 1) & 31, 1);
    assert!(section
        .windows(5)
        .any(|bytes| bytes == [6, 0xe2, 0, 0xf0, 7]));
    let (_, payload, _) = deliver(&mut gate);
    assert!(payload.iter().any(|packet| pid(packet) == 512));
    assert_eq!(gate.clock.report().unwrap(), clock);
    for (offset, output) in packets.iter().chain(&payload).enumerate() {
        let original = &source[ordinary.len() + offset];
        if !matches!(pid(original), 3337 | 8191) {
            assert_eq!(output, original);
        }
    }
    assert!(Arc::ptr_eq(
        gate.carousel
            .as_ref()
            .unwrap()
            .authored_service_shared()
            .unwrap(),
        &service
    ));
}

fn full_pmt_source() -> Vec<[u8; 188]> {
    let mut source = source(5);
    let mut descriptors = Vec::new();
    for _ in 0..4 {
        descriptors.extend_from_slice(&[0x80, 247]);
        descriptors.extend([0; 247]);
    }
    let section = pmt(10, 300, 300, &descriptors);
    assert_eq!(section.len(), 1017);
    let mut payload = vec![0];
    payload.extend(section);
    for ((counter, bytes), position) in payload.chunks(184).enumerate().zip([1, 3, 4, 5, 6, 8]) {
        source[position] = packet(3337, counter == 0, u8::try_from(counter).unwrap(), bytes);
    }
    source
}

#[test]
fn rejected_first_activation_preserves_initial_off_source_and_acknowledgement() {
    let source = full_pmt_source();
    let mut gate = TeletextOutputGate::new_absent(policy()).unwrap();
    gate.push(&tests::bytes(&source), Duration::ZERO).unwrap();
    let (receipt, first, now) = deliver(&mut gate);
    let cc = gate.installer.as_ref().unwrap().next_continuity_counter();
    let service = transition_tests::simple("nld", None);
    assert!(matches!(
        gate.replace_authored_at_acknowledged_boundary(&receipt, &service, now),
        Err(TeletextOutputError::Pmt(
            TeletextPmtInstallError::Multiplex(TeletextMultiplexError::SectionCapacity)
        ))
    ));
    assert!(gate.failure.is_none());
    assert!(gate.carousel.is_none() && gate.service.is_none());
    assert!(!gate.authored_payload_selected());
    assert!(!gate.signalling_transition_pending());
    assert_eq!(
        gate.installer.as_ref().unwrap().next_continuity_counter(),
        cc
    );
    let (_, next, _) = deliver(&mut gate);
    assert_eq!(first, source[..first.len()]);
    assert_eq!(next, source[first.len()..first.len() + next.len()]);
}

#[test]
fn initially_absent_retained_source_keeps_holding_and_explicit_delivery_start() {
    let source = source(2);
    let mut gate = TeletextOutputGate::new_absent_retained(policy()).unwrap();
    gate.push(&tests::bytes(&source), Duration::ZERO).unwrap();
    gate.start_delivery(Duration::from_secs(1)).unwrap();
    let (_, output, _) = deliver(&mut gate);
    assert_eq!(output, source[..output.len()]);
    assert!(gate.carousel.is_none());
    let mut bounded = TeletextOutputGate::new_absent(tests::config()).unwrap();
    assert!(bounded
        .push(&vec![0; 1024 * 188 + 1], Duration::ZERO)
        .is_err());
    assert!(bounded.failure.is_some());
    let mut expired = TeletextOutputGate::new_absent_retained(tests::config()).unwrap();
    expired
        .push(&tests::bytes(&source), Duration::ZERO)
        .unwrap();
    assert!(expired.check_deadline(Duration::from_millis(500)).is_err());
    assert!(expired.failure.is_some());
}

#[test]
fn initially_absent_tail_and_partial_packet_retain_existing_eof_fences() {
    let source = source(2);
    let mut gate = TeletextOutputGate::new_absent(policy()).unwrap();
    gate.push(&tests::bytes(&source[..source.len() - 1]), Duration::ZERO)
        .unwrap();
    let (_, mut emitted, _) = deliver(&mut gate);
    let now = gate.due_at(gate.source_start).unwrap();
    assert!(gate.prepare_final_window(now).unwrap());
    let schedule = gate.pending_schedule().unwrap();
    emitted.extend(gate.pending_packets(schedule.first_due()).unwrap());
    let at = schedule.last_due();
    gate.acknowledge_window_with_receipt(schedule, at).unwrap();
    gate.finish(at).unwrap();
    assert_eq!(emitted, source[..source.len() - 1]);
    let mut partial = TeletextOutputGate::new_absent(policy()).unwrap();
    partial
        .push(
            &tests::bytes(&source)[..source.len() * 188 - 1],
            Duration::ZERO,
        )
        .unwrap();
    assert!(matches!(
        partial.prepare_final_window(Duration::ZERO),
        Err(TeletextOutputError::Buffer(
            TeletextBufferError::PartialPacket
        ))
    ));
}

#[test]
fn initial_absence_keeps_reserved_pid_and_source_clock_fences() {
    let mut collision = source(2);
    collision[8] = packet(512, false, 0, &[0xff; 184]);
    let mut gate = TeletextOutputGate::new_absent(policy()).unwrap();
    assert!(gate
        .push(&tests::bytes(&collision), Duration::ZERO)
        .is_err());
    assert!(gate.failure.is_some());
    assert!(gate.pending.is_none());
    let mut clock = policy();
    clock.clock.pcr_pid = 301;
    let mut gate = TeletextOutputGate::new_absent(clock).unwrap();
    assert_eq!(
        gate.push(&tests::bytes(&source(2)), Duration::ZERO),
        Err(TeletextOutputError::Multiplex(
            TeletextMultiplexError::Clock
        ))
    );
    assert!(gate.failure.is_some());
}

#[test]
fn first_activation_rejects_foreign_and_unacknowledged_windows_without_losing_off() {
    let source = source(4);
    let mut gate = TeletextOutputGate::new_absent(policy()).unwrap();
    let mut other = TeletextOutputGate::new_absent(policy()).unwrap();
    gate.push(&tests::bytes(&source), Duration::ZERO).unwrap();
    other.push(&tests::bytes(&source), Duration::ZERO).unwrap();
    let (receipt, _, now) = deliver(&mut gate);
    let (foreign, _, _) = deliver(&mut other);
    let service = transition_tests::simple("nld", None);
    assert_eq!(
        gate.replace_authored_at_acknowledged_boundary(&foreign, &service, now),
        Err(TeletextOutputError::Acknowledgement)
    );
    assert!(gate.failure.is_none() && gate.carousel.is_none());
    let end = gate.next_window_end().unwrap().unwrap();
    let count = usize::try_from(end - gate.source_start).unwrap();
    let at = gate.due_at(gate.source_start).unwrap();
    assert!(gate.prepare_window(count, at).unwrap());
    assert_eq!(
        gate.replace_authored_at_acknowledged_boundary(&receipt, &service, at),
        Err(TeletextOutputError::Pending)
    );
    assert!(gate.failure.is_none() && gate.carousel.is_none());
    let schedule = gate.pending_schedule().unwrap();
    let first = usize::try_from(schedule.first_packet()).unwrap();
    assert_eq!(
        gate.pending_packets(schedule.first_due()).unwrap(),
        &source[first..first + count]
    );
}
