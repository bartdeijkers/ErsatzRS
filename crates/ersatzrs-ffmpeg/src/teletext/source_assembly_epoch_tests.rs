//! Source-origin allowance never repairs selected transport or PES bytes.

use super::*;

fn initial_packet(cc: u8) -> [u8; 188] {
    let mut raw = adaptation(cc);
    raw[5] |= 0x80;
    raw
}

#[test]
fn explicit_assembly_epoch_keeps_exact_pes_bytes_and_pts_absence() {
    for with_pts in [false, true] {
        let mut pes = supplied_pes(2);
        if !with_pts {
            pes[7] = 0;
            pes[9..14].fill(0xff);
        }
        let mut assembler = SuppliedTeletextAssembler::new_source_epoch(PID).unwrap();
        let mut unrelated = initial_packet(15);
        unrelated[1] = 2;
        assert_eq!(
            assembler.push(&unrelated),
            Ok(TeletextAssemblyEvent::IgnoredPid)
        );
        assert_eq!(
            assembler.push(&initial_packet(15)),
            Ok(TeletextAssemblyEvent::NoPayload)
        );
        let packets = transport(&pes, 0);
        assert_eq!(
            assembler.push(&packets[0]),
            Ok(TeletextAssemblyEvent::Pending)
        );
        let TeletextAssemblyEvent::Complete(view) = assembler.push(&packets[1]).unwrap() else {
            panic!("initial flag must not change exact PES assembly");
        };
        assert_eq!(view.bytes(), pes);
        assert_eq!(
            view.pts().map(crate::teletext::TeletextTimestamp::ticks),
            with_pts.then_some(90_000)
        );
        assert_eq!(view.units().count(), 7);
        assembler.finish().unwrap();
        assert_eq!(
            assembler.push(&packets[0]),
            Err(TeletextAssemblyError::Closed)
        );
    }
}

#[test]
fn strict_assembly_stays_strict_and_flagged_duplicates_never_reopen_an_epoch() {
    let mut strict = SuppliedTeletextAssembler::new(PID).unwrap();
    assert_eq!(
        strict.push(&initial_packet(0)),
        Err(TeletextAssemblyError::Discontinuity)
    );
    fenced(&mut strict, TeletextAssemblyError::Discontinuity);
    for first in [
        initial_packet(0),
        adaptation(0),
        transport(&supplied_pes(1), 0)[0],
    ] {
        let mut assembler = SuppliedTeletextAssembler::new_source_epoch(PID).unwrap();
        assembler.push(&first).unwrap();
        assert_eq!(
            assembler.push(&initial_packet(0)),
            Err(TeletextAssemblyError::Discontinuity)
        );
        fenced(&mut assembler, TeletextAssemblyError::Discontinuity);
    }
}

#[test]
fn initial_assembly_exception_preserves_framing_and_transport_profile() {
    let original = initial_packet(0);
    for (index, value, error) in [
        (0, 0, TeletextAssemblyError::Sync),
        (1, 0x81, TeletextAssemblyError::Framing),
        (1, 0x41, TeletextAssemblyError::UnexpectedStart),
        (3, 0x60, TeletextAssemblyError::Framing),
        (4, 182, TeletextAssemblyError::Framing),
    ] {
        let mut raw = original;
        raw[index] = value;
        let mut assembler = SuppliedTeletextAssembler::new_source_epoch(PID).unwrap();
        assert_eq!(assembler.push(&raw), Err(error));
        fenced(&mut assembler, error);
    }
    let mut combined = transport(&supplied_pes(1), 0)[0];
    combined[3] = 0x30;
    combined[4] = 0;
    let mut assembler = SuppliedTeletextAssembler::new_source_epoch(PID).unwrap();
    assert_eq!(
        assembler.push(&combined),
        Err(TeletextAssemblyError::UnsupportedTransport)
    );
    fenced(&mut assembler, TeletextAssemblyError::UnsupportedTransport);
}

#[test]
fn initial_assembly_exception_preserves_counter_gaps_and_unseen_prefix_facts() {
    let pes = supplied_pes(1);
    let mut wrong = SuppliedTeletextAssembler::new_source_epoch(PID).unwrap();
    wrong.push(&initial_packet(15)).unwrap();
    assert_eq!(
        wrong.push(&transport(&pes, 15)[0]),
        Err(TeletextAssemblyError::Continuity)
    );
    fenced(&mut wrong, TeletextAssemblyError::Continuity);
    let mut wrong = SuppliedTeletextAssembler::new_source_epoch(PID).unwrap();
    wrong.push(&initial_packet(0)).unwrap();
    assert_eq!(
        wrong.push(&adaptation(1)),
        Err(TeletextAssemblyError::Continuity)
    );
    fenced(&mut wrong, TeletextAssemblyError::Continuity);
    let mut assembler = SuppliedTeletextAssembler::new_source_epoch(PID).unwrap();
    let mut prefix = transport(&pes, 0)[0];
    prefix[1] &= !0x40;
    assembler.push(&initial_packet(15)).unwrap();
    assert_eq!(
        assembler.push(&prefix),
        Ok(TeletextAssemblyEvent::UnobservedPrefix)
    );
    let TeletextAssemblyEvent::Complete(view) = assembler.push(&transport(&pes, 1)[0]).unwrap()
    else {
        panic!("unseen prefix must not become part of the observed PES");
    };
    assert_eq!(view.bytes(), pes);
    assembler.finish().unwrap();
}

#[test]
fn initial_assembly_exception_never_repairs_interrupted_or_incomplete_pes() {
    let packets = transport(&supplied_pes(2), 0);
    for interruption in [false, true] {
        let mut assembler = SuppliedTeletextAssembler::new_source_epoch(PID).unwrap();
        assembler.push(&initial_packet(15)).unwrap();
        assembler.push(&packets[0]).unwrap();
        let error = if interruption {
            assert_eq!(
                assembler.push(&transport(&supplied_pes(1), 1)[0]),
                Err(TeletextAssemblyError::UnexpectedStart)
            );
            TeletextAssemblyError::UnexpectedStart
        } else {
            assert_eq!(assembler.finish(), Err(TeletextAssemblyError::Incomplete));
            TeletextAssemblyError::Incomplete
        };
        fenced(&mut assembler, error);
    }
    for pid in [0, 0x1f, 0x1fff, 0x2000] {
        assert_eq!(
            SuppliedTeletextAssembler::new_source_epoch(pid).unwrap_err(),
            TeletextAssemblyError::Pid
        );
    }
}
