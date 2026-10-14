//! Complete candidates reuse the existing output epoch and display signalling.

use super::tests::{bytes, config, end, expose_and_ack, source};
use super::*;
use crate::teletext::authored_service_tests::{candidate, declared, input, limits, transmissions};

#[path = "authored_service_interleave_output_tests.rs"]
mod interleave;

fn cycle_batches() -> u64 {
    let mut carousel = TeletextCarousel::new_authored(candidate()).unwrap();
    let mut count = 0;
    loop {
        count += 1;
        if carousel.next_batch().unwrap().kind()
            == crate::teletext::TeletextCarouselBatchKind::Terminator
        {
            return count;
        }
    }
}

#[test]
fn descriptor_uses_only_original_displays_languages_and_subtitle_roles() {
    let mut draft = input();
    draft.base.pages[1].kind = ersatzrs_domain::TeletextPageKind::Subtitle;
    draft.base.pages[1].language = ersatzrs_domain::TeletextLanguage::new("eng".into()).unwrap();
    for row in &mut draft.base.pages[1].rows {
        for run in &mut row.runs {
            run.style.boxed = true;
        }
    }
    let declared = declared(draft);
    let tx = transmissions(&declared);
    let service =
        crate::teletext::encode_teletext_authored_service(declared, &tx, &limits()).unwrap();
    let descriptor = TeletextServiceDescriptor::from_authored_service(&service);
    assert_eq!(
        descriptor.bytes(),
        [0x56, 10, b'e', b'n', b'g', 18, 0, b'n', b'l', b'd', 10, 0]
    );
    assert_eq!(service.pages().len(), 6);
    assert_eq!(service.magazine_units().len(), 1);
    // MOT/DRCS/POP/empty POP cannot create additional descriptor entries.
    let gate = TeletextOutputGate::new_authored(config(), service).unwrap();
    assert_eq!(gate.service.as_ref(), Some(&descriptor));
}

#[test]
fn complete_authored_output_reuses_pacing_capacity_acknowledgement_and_eof() {
    let frames = cycle_batches();
    let raw = source(frames);
    let mut gate = TeletextOutputGate::new_authored(config(), candidate()).unwrap();
    gate.push(&bytes(&raw), Duration::ZERO).unwrap();
    assert!(gate.prepare_window(end(frames), Duration::ZERO).unwrap());
    let last_due = gate.pending_schedule().unwrap().last_due();
    let output = expose_and_ack(&mut gate);
    assert_eq!(output.len(), raw.len());
    assert!(gate.completed_cycle);
    for (original, actual) in raw.iter().zip(&output) {
        let pid = (u16::from(original[1] & 0x1f) << 8) | u16::from(original[2]);
        if pid == 300 {
            assert_eq!(original, actual, "PCR/source packets were rewritten");
        }
    }
    gate.finish(last_due).unwrap();
}

#[test]
fn authored_retained_output_keeps_holding_age_and_explicit_delivery_start() {
    let frames = cycle_batches();
    let mut gate = TeletextOutputGate::new_authored_retained(config(), candidate()).unwrap();
    gate.push(&bytes(&source(frames)), Duration::ZERO).unwrap();
    assert_eq!(gate.next_deadline().unwrap(), Some(config().max_hold));
    let start = Duration::from_millis(50);
    gate.start_delivery(start).unwrap();
    gate.prepare_window(end(frames), start).unwrap();
    let schedule = gate.pending_schedule().unwrap();
    assert_eq!(schedule.first_due(), start + config().playout_delay);
    assert_eq!(gate.pending.as_ref().unwrap().first_arrival, Duration::ZERO);
    let last = schedule.last_due();
    expose_and_ack(&mut gate);
    gate.finish(last).unwrap();
}
