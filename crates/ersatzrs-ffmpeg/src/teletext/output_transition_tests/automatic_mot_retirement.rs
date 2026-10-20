//! Automatic wire retirement is separate from completing the accepted successor.

use super::*;
use crate::teletext::{
    source_assembly::{SuppliedTeletextAssembler, TeletextAssemblyEvent},
    source_header::{observe_dvb_teletext_header, TeletextHeaderObservation},
};

fn units(packets: &[[u8; 188]]) -> Vec<[u8; 46]> {
    let mut assembler = SuppliedTeletextAssembler::new(512).unwrap();
    let mut units = Vec::new();
    for packet in packets {
        if let TeletextAssemblyEvent::Complete(pes) = assembler.push(packet).unwrap() {
            units.extend(pes.units().filter_map(|unit| unit.teletext_unit()).copied());
        }
    }
    assembler.finish().unwrap();
    units
}

fn assert_neutral_wire(units: &[[u8; 46]], subpage: u8, counter: u8) {
    assert_eq!(units.len(), 20, "full MOT plus its separate closing FF");
    let TeletextHeaderObservation::Header(header) = observe_dvb_teletext_header(&units[0]).unwrap()
    else {
        panic!("MOT header required")
    };
    assert_eq!(
        (header.packet_address.wire_magazine, header.wire_page),
        (2, 0xfe)
    );
    assert_eq!(header.subcode & 15, u16::from(subpage));
    assert_eq!((header.subcode >> 4) & 7, u16::from(counter));
    assert_eq!(
        header.subcode >> 8,
        21,
        "last complete MOT body packet is 21"
    );
    assert!(header.controls.erase_page());
    assert_ne!(header.controls.raw() & 16, 0, "full changed update sets C8");
    // Independent literal Hamming nibble checks: all association and count
    // bytes are zero; only unused link page bytes are F and magazine is two.
    let rows = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 19, 20, 21];
    for (unit, row) in units[2..19].iter().zip(rows) {
        let TeletextHeaderObservation::NotHeader(address) =
            observe_dvb_teletext_header(unit).unwrap()
        else {
            panic!("complete MOT body row required")
        };
        assert_eq!((address.wire_magazine, address.packet_number), (2, row));
        let mut expected = [0_u8; 40];
        if matches!(row, 19 | 20) {
            for offset in [0, 10, 20, 30] {
                expected[offset] = 2;
                expected[offset + 1] = 15;
                expected[offset + 2] = 15;
            }
        } else if row == 21 {
            for offset in [0, 4, 8, 12, 16, 20, 24, 28] {
                expected[offset] = 2;
                expected[offset + 1] = 15;
                expected[offset + 2] = 15;
            }
        }
        let codes = [
            0x15_u8, 0x02, 0x49, 0x5e, 0x64, 0x73, 0x38, 0x2f, 0xd0, 0xc7, 0x8c, 0x9b, 0xa1, 0xb6,
            0xfd, 0xea,
        ];
        assert_eq!(
            &unit[6..],
            &expected.map(|nibble| codes[usize::from(nibble)].reverse_bits()),
            "row {row}"
        );
    }
    assert!(matches!(
        observe_dvb_teletext_header(&units[19]).unwrap(),
        TeletextHeaderObservation::Terminator(_)
    ));
}

#[test]
fn populated_mot_is_neutralized_and_acknowledged_before_any_successor_display() {
    let old = populated();
    let baseline: Vec<_> = old
        .pages()
        .iter()
        .map(|page| page.data_units().to_vec())
        .collect();
    let next = simple("nld", None);
    let (mut output, source) = gate(Arc::clone(&old), 200);
    let (receipt, _, now) = cycle(&mut output);
    let identity = old.declared().mots()[0].identity();
    let previous = output
        .carousel
        .as_ref()
        .unwrap()
        .mot_retirement_counter(identity)
        .unwrap();
    output
        .replace_authored_at_acknowledged_boundary(&receipt, &next, now)
        .unwrap();
    let first = usize::try_from(output.source_start).unwrap();
    let end = output.window_end(6).unwrap().unwrap();
    let start = output.due_at(output.source_start).unwrap();
    assert!(output
        .prepare_window(usize::try_from(end - output.source_start).unwrap(), start)
        .unwrap());
    let schedule = output.pending_schedule().unwrap();
    let packets = output
        .pending_packets(schedule.first_due())
        .unwrap()
        .to_vec();
    assert_neutral_wire(&units(&packets), 9, previous.next_modulo_eight().get());
    assert_source_positions(&packets, &source, first);
    assert_eq!(output.acknowledged_nonempty_mots[2], 1 << 9);
    assert!(Arc::ptr_eq(output.warm_service.as_ref().unwrap(), &old));
    let now = schedule.last_due();
    let receipt = output
        .acknowledge_window_with_receipt(schedule, now)
        .unwrap();
    assert!(
        !receipt.contains_complete_cycle(),
        "the neutral prefix is not the successor"
    );
    assert!(!receipt.is_cycle_boundary());
    assert_eq!(output.acknowledged_nonempty_mots, [0; 8]);
    assert!(Arc::ptr_eq(output.warm_service.as_ref().unwrap(), &old));
    let first = usize::try_from(output.source_start).unwrap();
    let (_, successor, _) = cycle(&mut output);
    assert_source_positions(&successor, &source, first);
    assert!(!units(&successor).iter().any(|unit| matches!(observe_dvb_teletext_header(unit).unwrap(), TeletextHeaderObservation::Header(header) if header.wire_page == 0xfe)));
    assert!(Arc::ptr_eq(output.warm_service.as_ref().unwrap(), &next));
    assert_eq!(
        baseline,
        old.pages()
            .iter()
            .map(|page| page.data_units().to_vec())
            .collect::<Vec<_>>()
    );
}

#[test]
fn retirement_uses_effective_modulo_counter_and_reactivation_keeps_its_tombstone() {
    let old = with_mot_counter(nonempty_unselected(1), 7);
    let next = simple("nld", None);
    let (mut output, source) = gate(Arc::clone(&old), 400);
    let (receipt, _, now) = cycle(&mut output);
    output
        .replace_authored_at_acknowledged_boundary(&receipt, &old, now)
        .unwrap();
    let (receipt, _, now) = cycle(&mut output);
    let identity = old.declared().mots()[0].identity();
    assert_eq!(
        output
            .carousel
            .as_ref()
            .unwrap()
            .mot_retirement_counter(identity)
            .unwrap()
            .get(),
        0
    );
    output
        .replace_authored_at_acknowledged_boundary(&receipt, &next, now)
        .unwrap();
    let (receipt, _, now) = cycle(&mut output);
    assert_eq!(
        output
            .carousel
            .as_ref()
            .unwrap()
            .mot_retirement_counter(identity)
            .unwrap()
            .get(),
        1
    );
    output
        .replace_authored_at_acknowledged_boundary(&receipt, &old, now)
        .unwrap();
    let (receipt, _, now) = cycle(&mut output);
    assert_eq!(
        output
            .carousel
            .as_ref()
            .unwrap()
            .mot_retirement_counter(identity)
            .unwrap()
            .get(),
        2
    );
    output
        .replace_authored_at_acknowledged_boundary(&receipt, &next, now)
        .unwrap();
    let first = usize::try_from(output.source_start).unwrap();
    let (_, packets, _) = deliver(&mut output, 3);
    assert_neutral_wire(&units(&packets), 0, 3);
    assert_source_positions(&packets, &source, first);
}

fn nonempty_unselected(count: u8) -> TeletextMotLevel25Declaration {
    let neutral = neutral_mot();
    let links = neutral.links();
    let object = links.object_link(None);
    let drcs = links.drcs_link(None);
    TeletextMotLevel25Declaration::new(
        neutral.identity(),
        vec![],
        TeletextMotLevel25Links::new(
            object,
            [object; 7],
            drcs,
            [drcs; 7],
            TeletextMotEnhancementCounts::new(0, count, 0),
        )
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn eof_with_only_a_neutral_prefix_cannot_acknowledge_the_successor() {
    let old = populated();
    let mut cursor = TeletextCarousel::new_authored_shared(Arc::clone(&old)).unwrap();
    let mut batches = 0;
    loop {
        batches += 1;
        if cursor.next_batch().unwrap().kind()
            == crate::teletext::TeletextCarouselBatchKind::Terminator
        {
            break;
        }
    }
    let (mut output, _) = gate(Arc::clone(&old), batches + 3);
    let (receipt, _, now) = cycle(&mut output);
    output
        .replace_authored_at_acknowledged_boundary(&receipt, &simple("nld", None), now)
        .unwrap();
    assert_eq!(
        output.prepare_final_window(now),
        Err(TeletextOutputError::Unterminated)
    );
    assert!(Arc::ptr_eq(output.warm_service.as_ref().unwrap(), &old));
    assert_eq!(output.acknowledged_nonempty_mots[2], 1 << 9);
    assert!(output.failure.is_some());
}

#[test]
fn retirement_cannot_extend_the_existing_twenty_second_acquisition_bound() {
    let next = simple("nld", None);
    // Two display intervals plus FF and the physical placement margin leave
    // exactly 996 of the existing 1000 intervals for a retirement prefix.
    assert!(TeletextCarousel::validate_retirement_prefix(&next, 996).is_ok());
    assert_eq!(
        TeletextCarousel::validate_retirement_prefix(&next, 997),
        Err(TeletextTransportError::CycleTiming {
            batches: 1001,
            maximum: 1000
        })
    );
}
