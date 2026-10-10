use super::*;

const PID: u16 = 0x100;

fn supplied_pes(packet_count: usize) -> Vec<u8> {
    let size = 184 * packet_count;
    let length = u16::try_from(size - 6).unwrap();
    let mut bytes = vec![0xff; size];
    bytes[..9].copy_from_slice(&[0, 0, 1, 0xbd, 0, 0, 0x84, 0x80, 0x24]);
    bytes[4..6].copy_from_slice(&length.to_be_bytes());
    bytes[9..14].copy_from_slice(&[0x21, 0, 5, 0xbf, 0x21]);
    bytes[45] = 0x10;
    for unit in bytes[46..].chunks_exact_mut(46) {
        unit[..2].copy_from_slice(&[2, 44]);
    }
    bytes
}

fn packet(payload: &[u8], cc: u8, start: bool) -> [u8; 188] {
    assert_eq!(payload.len(), 184);
    let mut raw = [0_u8; 188];
    raw[..4].copy_from_slice(&[0x47, 1 | if start { 0x40 } else { 0 }, 0, 0x10 | cc]);
    raw[4..].copy_from_slice(payload);
    raw
}

fn transport(pes: &[u8], initial_cc: u8) -> Vec<[u8; 188]> {
    pes.chunks_exact(184)
        .enumerate()
        .map(|(index, bytes)| {
            let increment = u8::try_from(index & 15).unwrap();
            packet(bytes, (initial_cc + increment) & 15, index == 0)
        })
        .collect()
}

fn adaptation(cc: u8) -> [u8; 188] {
    let mut raw = [0xff; 188];
    raw[..6].copy_from_slice(&[0x47, 1, 0, 0x20 | cc, 183, 0]);
    raw
}

fn fenced(assembler: &mut SuppliedTeletextAssembler, error: TeletextAssemblyError) {
    assert!(assembler.pending.is_empty());
    assert!(assembler.previous.is_none());
    assert_eq!(
        assembler.push(&transport(&supplied_pes(1), 0)[0]),
        Err(error)
    );
    assert_eq!(assembler.finish(), Err(error));
    let unrelated = [0x47; 188];
    assert_eq!(assembler.push(&unrelated), Err(error));
}

#[test]
fn two_packet_pes_returns_one_checked_borrowed_view_without_a_next_pusi() {
    let pes = supplied_pes(2);
    let packets = transport(&pes, 0);
    let mut assembler = SuppliedTeletextAssembler::new(PID).unwrap();
    assert_eq!(
        assembler.push(&packets[0]),
        Ok(TeletextAssemblyEvent::Pending)
    );
    let TeletextAssemblyEvent::Complete(view) = assembler.push(&packets[1]).unwrap() else {
        panic!("exact final packet must complete the supplied PES");
    };
    assert_eq!(view.bytes(), pes);
    assert_eq!(view.pts().unwrap().ticks(), 90_000);
    assert_eq!(view.units().count(), 7);
    for (index, unit) in view.units().enumerate() {
        assert!(std::ptr::eq(
            unit.bytes().as_ptr(),
            view.bytes()[46 + index * 46..].as_ptr()
        ));
        assert!(unit.teletext_unit().is_some());
    }
    assert!(assembler.expected.is_none());
    assert_eq!(assembler.pending.len(), pes.len());
    assembler.finish().unwrap();
}

#[test]
fn unseen_prefix_is_reported_and_never_attached_to_the_first_observed_pes() {
    let pes = supplied_pes(1);
    let mut prefix = transport(&pes, 4)[0];
    prefix[1] &= 0xbf;
    prefix[4..].fill(0xa5);
    let mut assembler = SuppliedTeletextAssembler::new(PID).unwrap();
    assert_eq!(
        assembler.push(&prefix),
        Ok(TeletextAssemblyEvent::UnobservedPrefix)
    );
    assert!(assembler.pending.is_empty());
    let TeletextAssemblyEvent::Complete(view) = assembler.push(&transport(&pes, 5)[0]).unwrap()
    else {
        panic!("first observed start must begin independent PES bytes");
    };
    assert_eq!(view.bytes(), pes);
}

#[test]
fn initial_adaptation_only_retains_a_counter_baseline() {
    let mut assembler = SuppliedTeletextAssembler::new(PID).unwrap();
    assert_eq!(
        assembler.push(&adaptation(15)),
        Ok(TeletextAssemblyEvent::NoPayload)
    );
    assert_eq!(
        assembler.push(&adaptation(15)),
        Ok(TeletextAssemblyEvent::NoPayload)
    );
    assert!(matches!(
        assembler.push(&transport(&supplied_pes(1), 0)[0]),
        Ok(TeletextAssemblyEvent::Complete(_))
    ));
    let mut invalid = SuppliedTeletextAssembler::new(PID).unwrap();
    invalid.push(&adaptation(15)).unwrap();
    assert_eq!(
        invalid.push(&transport(&supplied_pes(1), 15)[0]),
        Err(TeletextAssemblyError::Continuity)
    );
    fenced(&mut invalid, TeletextAssemblyError::Continuity);
}

#[test]
fn adaptation_between_payloads_preserves_bytes_and_duplicate_identity() {
    let packets = transport(&supplied_pes(2), 3);
    let mut assembler = SuppliedTeletextAssembler::new(PID).unwrap();
    assembler.push(&packets[0]).unwrap();
    assert_eq!(
        assembler.push(&adaptation(3)),
        Ok(TeletextAssemblyEvent::NoPayload)
    );
    assert_eq!(
        assembler.push(&packets[0]),
        Ok(TeletextAssemblyEvent::Duplicate)
    );
    assert_eq!(assembler.pending.len(), 184);
    assert!(matches!(
        assembler.push(&packets[1]),
        Ok(TeletextAssemblyEvent::Complete(_))
    ));
}

#[test]
fn exact_duplicates_before_and_after_completion_never_emit_again() {
    let packets = transport(&supplied_pes(2), 0);
    let mut assembler = SuppliedTeletextAssembler::new(PID).unwrap();
    assembler.push(&packets[0]).unwrap();
    for _ in 0..3 {
        assert_eq!(
            assembler.push(&packets[0]),
            Ok(TeletextAssemblyEvent::Duplicate)
        );
    }
    assert!(matches!(
        assembler.push(&packets[1]),
        Ok(TeletextAssemblyEvent::Complete(_))
    ));
    for _ in 0..3 {
        assert_eq!(
            assembler.push(&packets[1]),
            Ok(TeletextAssemblyEvent::Duplicate)
        );
    }
    let one = transport(&supplied_pes(1), 2)[0];
    assert!(matches!(
        assembler.push(&one),
        Ok(TeletextAssemblyEvent::Complete(_))
    ));
    assert_eq!(assembler.push(&one), Ok(TeletextAssemblyEvent::Duplicate));
}

#[test]
fn payload_counters_wrap_from_fifteen_to_zero() {
    let packets = transport(&supplied_pes(2), 15);
    let mut assembler = SuppliedTeletextAssembler::new(PID).unwrap();
    assert_eq!(
        assembler.push(&packets[0]),
        Ok(TeletextAssemblyEvent::Pending)
    );
    assert!(matches!(
        assembler.push(&packets[1]),
        Ok(TeletextAssemblyEvent::Complete(_))
    ));
}

#[test]
fn changed_duplicates_and_gaps_fence_pending_or_completed_state() {
    for count in [1, 2] {
        for gap in [false, true] {
            let packets = transport(&supplied_pes(count), 0);
            let mut assembler = SuppliedTeletextAssembler::new(PID).unwrap();
            assembler.push(&packets[0]).unwrap();
            let mut damaged = packets[0];
            if gap {
                damaged[3] = 0x12;
            } else {
                damaged[100] ^= 1;
            }
            assert_eq!(
                assembler.push(&damaged),
                Err(TeletextAssemblyError::Continuity)
            );
            fenced(&mut assembler, TeletextAssemblyError::Continuity);
        }
    }
}

#[test]
fn an_early_payload_start_cannot_replace_an_incomplete_pes() {
    let packets = transport(&supplied_pes(2), 0);
    let mut assembler = SuppliedTeletextAssembler::new(PID).unwrap();
    assembler.push(&packets[0]).unwrap();
    let restart = transport(&supplied_pes(1), 1)[0];
    assert_eq!(
        assembler.push(&restart),
        Err(TeletextAssemblyError::UnexpectedStart)
    );
    fenced(&mut assembler, TeletextAssemblyError::UnexpectedStart);
}

#[test]
fn continuation_after_exact_completion_is_not_silently_discarded() {
    let packets = transport(&supplied_pes(1), 0);
    let mut assembler = SuppliedTeletextAssembler::new(PID).unwrap();
    assembler.push(&packets[0]).unwrap();
    let extra = packet(&[0xff; 184], 1, false);
    assert_eq!(
        assembler.push(&extra),
        Err(TeletextAssemblyError::UnexpectedPayload)
    );
    fenced(&mut assembler, TeletextAssemblyError::UnexpectedPayload);
}

#[test]
fn finite_eof_fences_a_partial_pes_but_no_start_does_not_mean_empty_inventory() {
    let mut incomplete = SuppliedTeletextAssembler::new(PID).unwrap();
    incomplete.push(&transport(&supplied_pes(2), 0)[0]).unwrap();
    assert_eq!(incomplete.finish(), Err(TeletextAssemblyError::Incomplete));
    fenced(&mut incomplete, TeletextAssemblyError::Incomplete);
    for prefix in [false, true] {
        let mut closed = SuppliedTeletextAssembler::new(PID).unwrap();
        if prefix {
            closed.push(&packet(&[0xff; 184], 0, false)).unwrap();
        }
        closed.finish().unwrap();
        closed.finish().unwrap();
        assert_eq!(
            closed.push(&transport(&supplied_pes(1), 1)[0]),
            Err(TeletextAssemblyError::Closed)
        );
    }
    let mut complete = SuppliedTeletextAssembler::new(PID).unwrap();
    complete.push(&transport(&supplied_pes(1), 0)[0]).unwrap();
    complete.finish().unwrap();
    complete.finish().unwrap();
    assert_eq!(
        complete.push(&adaptation(0)),
        Err(TeletextAssemblyError::Closed)
    );
}

#[test]
fn pid_profile_and_global_sync_do_not_claim_unrelated_packet_validation() {
    for pid in [0, 0x1f, 0x1fff, 0x2000, u16::MAX] {
        assert_eq!(
            SuppliedTeletextAssembler::new(pid).unwrap_err(),
            TeletextAssemblyError::Pid
        );
    }
    for pid in [0x20, 0x1ffe] {
        assert!(SuppliedTeletextAssembler::new(pid).is_ok());
    }
    let mut unrelated = [0xff; 188];
    unrelated[..4].copy_from_slice(&[0x47, 0x82, 0, 0xf7]);
    let mut assembler = SuppliedTeletextAssembler::new(PID).unwrap();
    assert_eq!(
        assembler.push(&unrelated),
        Ok(TeletextAssemblyEvent::IgnoredPid)
    );
    assert!(assembler.counter.is_none());
    assert!(assembler.previous.is_none());
    assert!(matches!(
        assembler.push(&transport(&supplied_pes(1), 9)[0]),
        Ok(TeletextAssemblyEvent::Complete(_))
    ));
    unrelated[0] = 0;
    assert_eq!(assembler.push(&unrelated), Err(TeletextAssemblyError::Sync));
    fenced(&mut assembler, TeletextAssemblyError::Sync);
}

#[test]
fn selected_framing_rejects_tei_scrambling_and_combined_adaptation_payload() {
    let original = transport(&supplied_pes(1), 0)[0];
    for (index, value, error) in [
        (0, 0, TeletextAssemblyError::Sync),
        (1, 0xc1, TeletextAssemblyError::Framing),
        (3, 0x50, TeletextAssemblyError::Framing),
        (3, 0x90, TeletextAssemblyError::Framing),
        (3, 0xd0, TeletextAssemblyError::Framing),
        (3, 0, TeletextAssemblyError::Framing),
        (3, 0x30, TeletextAssemblyError::UnsupportedTransport),
    ] {
        let mut raw = original;
        raw[index] = value;
        let mut assembler = SuppliedTeletextAssembler::new(PID).unwrap();
        assert_eq!(assembler.push(&raw), Err(error));
        fenced(&mut assembler, error);
    }
}

#[test]
fn adaptation_only_requires_full_extent_no_start_no_discontinuity_and_unchanged_cc() {
    for length in [0, 1, 182, 184, 255] {
        let mut raw = adaptation(0);
        raw[4] = length;
        let mut assembler = SuppliedTeletextAssembler::new(PID).unwrap();
        assert_eq!(assembler.push(&raw), Err(TeletextAssemblyError::Framing));
    }
    for (index, value, error) in [
        (1, 0x41, TeletextAssemblyError::UnexpectedStart),
        (5, 0x80, TeletextAssemblyError::Discontinuity),
        (3, 0x21, TeletextAssemblyError::Continuity),
    ] {
        let mut assembler = SuppliedTeletextAssembler::new(PID).unwrap();
        assembler.push(&adaptation(0)).unwrap();
        let mut raw = adaptation(0);
        raw[index] = value;
        assert_eq!(assembler.push(&raw), Err(error));
        fenced(&mut assembler, error);
    }
}

#[test]
fn invalid_start_headers_and_lengths_fail_before_reserving_pes_storage() {
    for (index, value, error) in [
        (4, 1, TeletextAssemblyError::PesHeader),
        (7, 0xbe, TeletextAssemblyError::PesHeader),
        (8, 0, TeletextAssemblyError::PacketLength),
    ] {
        let mut raw = transport(&supplied_pes(1), 0)[0];
        raw[index] = value;
        if index == 8 {
            raw[9] = 0;
        }
        let mut assembler = SuppliedTeletextAssembler::new(PID).unwrap();
        assert_eq!(assembler.push(&raw), Err(error));
        assert_eq!(assembler.pending.capacity(), 0);
        fenced(&mut assembler, error);
    }
    for length in [1_u16, 177, 179, u16::MAX] {
        let mut raw = transport(&supplied_pes(1), 0)[0];
        raw[8..10].copy_from_slice(&length.to_be_bytes());
        let mut assembler = SuppliedTeletextAssembler::new(PID).unwrap();
        assert_eq!(
            assembler.push(&raw),
            Err(TeletextAssemblyError::PacketLength)
        );
        assert_eq!(assembler.pending.capacity(), 0);
    }
}

#[test]
fn malformed_final_unit_fails_existing_pes_validation_before_any_emission() {
    let mut bytes = supplied_pes(2);
    let last = bytes.len() - 46;
    bytes[last + 1] = 43;
    let packets = transport(&bytes, 0);
    let mut assembler = SuppliedTeletextAssembler::new(PID).unwrap();
    assert_eq!(
        assembler.push(&packets[0]),
        Ok(TeletextAssemblyEvent::Pending)
    );
    let error = TeletextAssemblyError::Pes(TeletextPesError::TeletextUnitLength);
    assert_eq!(assembler.push(&packets[1]), Err(error));
    fenced(&mut assembler, error);
}

#[test]
fn maximum_pes_bound_is_retained_once_and_reused_without_an_output_queue() {
    let pes = supplied_pes(356);
    assert_eq!(pes.len(), MAX_PES_BYTES);
    let packets = transport(&pes, 0);
    let mut assembler = SuppliedTeletextAssembler::new(PID).unwrap();
    for raw in &packets[..355] {
        assert_eq!(assembler.push(raw), Ok(TeletextAssemblyEvent::Pending));
        assert!(assembler.pending.len() <= MAX_PES_BYTES);
        assert!(assembler.pending.capacity() <= MAX_PES_BYTES);
    }
    let TeletextAssemblyEvent::Complete(view) = assembler.push(&packets[355]).unwrap() else {
        panic!("maximum declared packet must complete exactly");
    };
    assert_eq!(view.bytes(), pes);
    assert_eq!(view.units().count(), 1_423);
    let retained_capacity = assembler.pending.capacity();
    let next = transport(&supplied_pes(1), 4)[0];
    assert!(matches!(
        assembler.push(&next),
        Ok(TeletextAssemblyEvent::Complete(_))
    ));
    assert_eq!(assembler.pending.len(), 184);
    assert_eq!(assembler.pending.capacity(), retained_capacity);
    assert_eq!(assembler.previous, Some(next));
}
