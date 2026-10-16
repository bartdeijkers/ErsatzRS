//! Checked alternatives preserve immutable services and per-response history.

use super::*;
use crate::teletext::{
    carousel::authored_service_carousel_tests::cycle,
    service_update::tests::{candidate, spec},
    source_header::{observe_dvb_teletext_header, TeletextHeaderObservation},
    TeletextCarousel, TeletextCarouselBatch, TeletextResourceFullUpdate, TeletextTransportError,
};
use std::{sync::Arc, time::Duration};

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(30)
}
fn body(
    counter: u8,
    bits: u16,
    update: TeletextResourceFullUpdate,
) -> EncodedTeletextAuthoredService {
    let mut input = spec(counter, update);
    input.glyph = Some((0, bits));
    candidate(input)
}
fn checked(
    counter: u8,
    bits: u16,
    update: TeletextResourceFullUpdate,
) -> EncodedTeletextAuthoredService {
    body(counter, bits, update)
        .with_resource_lifecycle_variants(deadline())
        .unwrap()
}
fn resource_header(
    batches: &[TeletextCarouselBatch],
) -> super::super::source_header::SuppliedTeletextHeader {
    batches
        .iter()
        .flat_map(TeletextCarouselBatch::data_units)
        .find_map(|unit| match observe_dvb_teletext_header(unit).unwrap() {
            TeletextHeaderObservation::Header(header) if header.wire_page == 0xc0 => Some(header),
            _ => None,
        })
        .unwrap()
}
fn assert_counter(batches: &[TeletextCarouselBatch], counter: u8, changed: bool) {
    let header = resource_header(batches);
    assert_eq!((header.subcode >> 4) & 7, u16::from(counter));
    assert_eq!(header.controls.erase_page(), changed);
    assert_eq!(header.controls.raw() & 16 != 0, changed);
}

#[test]
fn checked_plan_is_complete_native_order_and_changes_only_s2_c4_c8() {
    let service = body(7, 1, TeletextResourceFullUpdate::Changed);
    assert!(service.resource_lifecycle_variants().is_none());
    let units: Vec<_> = service
        .pages()
        .iter()
        .map(|page| page.data_units().to_vec())
        .collect();
    let inventory = service.inventory().to_vec();
    let allocation = service.pages().as_ptr();
    let service = service
        .with_resource_lifecycle_variants(deadline())
        .unwrap();
    assert_eq!(service.pages().as_ptr(), allocation);
    assert_eq!(service.inventory(), inventory);
    let plan = service.resource_lifecycle_variants().unwrap();
    assert_eq!(
        plan.entries().len(),
        service
            .pages()
            .iter()
            .filter(|page| page.resource_header().is_some())
            .count()
    );
    let mut previous = None;
    for entry in plan.entries() {
        assert!(previous.is_none_or(|index| index < entry.physical_page_index()));
        previous = Some(entry.physical_page_index());
        let baseline = &service.pages()[entry.physical_page_index()];
        assert_eq!(
            entry.resource(),
            baseline.resource_header().unwrap().identity()
        );
        for (index, header) in entry.headers().iter().enumerate() {
            let TeletextHeaderObservation::Header(decoded) =
                observe_dvb_teletext_header(header).unwrap()
            else {
                panic!("resource header")
            };
            assert_eq!(
                (decoded.subcode >> 4) & 7,
                u16::try_from(index / 2).unwrap()
            );
            assert_eq!(decoded.controls.erase_page(), index % 2 == 1);
            assert_eq!(decoded.controls.raw() & 16 != 0, index % 2 == 1);
            for (offset, (&actual, &expected)) in
                header.iter().zip(&baseline.data_units()[0]).enumerate()
            {
                if ![9, 12].contains(&offset) {
                    assert_eq!(actual, expected);
                }
            }
        }
    }
    assert_eq!(
        service
            .pages()
            .iter()
            .map(|page| page.data_units().to_vec())
            .collect::<Vec<_>>(),
        units
    );
    let mut strict =
        TeletextCarousel::new_authored(body(7, 1, TeletextResourceFullUpdate::Changed)).unwrap();
    let mut checked = TeletextCarousel::new_authored(service).unwrap();
    assert_eq!(cycle(&mut checked), cycle(&mut strict));
    assert!(matches!(
        body(1, 1, TeletextResourceFullUpdate::Changed)
            .with_resource_lifecycle_variants(Instant::now()),
        Err(TeletextResourceLifecycleError::TimedOut)
    ));
}

#[test]
fn checked_service_selects_different_counters_per_response_and_preserves_repeats_and_baseline() {
    let mut first =
        TeletextCarousel::new_authored(body(1, 1, TeletextResourceFullUpdate::Changed)).unwrap();
    let mut second =
        TeletextCarousel::new_authored(body(6, 1, TeletextResourceFullUpdate::Changed)).unwrap();
    cycle(&mut first);
    cycle(&mut second);
    let next = Arc::new(checked(1, 2, TeletextResourceFullUpdate::Changed));
    let original = next
        .pages()
        .iter()
        .map(|page| page.data_units().to_vec())
        .collect::<Vec<_>>();
    first
        .replace_authored_shared_at_boundary(Arc::clone(&next))
        .unwrap();
    second
        .replace_authored_shared_at_boundary(Arc::clone(&next))
        .unwrap();
    assert_counter(&cycle(&mut first), 2, true);
    assert_counter(&cycle(&mut second), 7, true);
    assert_counter(&cycle(&mut first), 2, false);
    assert_counter(&cycle(&mut second), 7, false);
    assert_eq!(
        next.pages()
            .iter()
            .map(|page| page.data_units().to_vec())
            .collect::<Vec<_>>(),
        original
    );
}

#[test]
fn skipped_and_untransmitted_candidates_do_not_advance_completed_counters() {
    let mut carousel =
        TeletextCarousel::new_authored(body(7, 1, TeletextResourceFullUpdate::Changed)).unwrap();
    cycle(&mut carousel);
    carousel
        .replace_authored_at_boundary(checked(3, 2, TeletextResourceFullUpdate::Changed))
        .unwrap();
    // Candidate B was selected but never emitted. C is based on completed A,
    // regardless of B/C's authored counters or skipped accepted generations.
    carousel
        .replace_authored_at_boundary(checked(6, 3, TeletextResourceFullUpdate::Changed))
        .unwrap();
    assert_counter(&cycle(&mut carousel), 0, true);
    carousel
        .replace_authored_at_boundary(checked(2, 4, TeletextResourceFullUpdate::Changed))
        .unwrap();
    assert_counter(&cycle(&mut carousel), 1, true);
}

#[test]
fn changed_intent_with_equal_bytes_advances_but_equal_unchanged_retains_actual_counter() {
    let mut carousel =
        TeletextCarousel::new_authored(body(1, 1, TeletextResourceFullUpdate::Changed)).unwrap();
    cycle(&mut carousel);
    carousel
        .replace_authored_at_boundary(checked(1, 1, TeletextResourceFullUpdate::Changed))
        .unwrap();
    assert_counter(&cycle(&mut carousel), 2, true);
    carousel
        .replace_authored_at_boundary(checked(6, 1, TeletextResourceFullUpdate::Unchanged))
        .unwrap();
    assert_counter(&cycle(&mut carousel), 2, false);
    carousel
        .replace_authored_at_boundary(checked(6, 2, TeletextResourceFullUpdate::Unchanged))
        .unwrap();
    assert_counter(&cycle(&mut carousel), 3, true);
}

#[test]
fn completed_removal_keeps_effective_tombstone_and_pending_removal_does_not_retire() {
    let mut carousel =
        TeletextCarousel::new_authored(body(6, 1, TeletextResourceFullUpdate::Changed)).unwrap();
    cycle(&mut carousel);
    carousel
        .replace_authored_at_boundary(checked(1, 2, TeletextResourceFullUpdate::Changed))
        .unwrap();
    assert_counter(&cycle(&mut carousel), 7, true);
    let absent = || {
        let mut input = spec(1, TeletextResourceFullUpdate::Unchanged);
        input.kind = None;
        candidate(input)
            .with_resource_lifecycle_variants(deadline())
            .unwrap()
    };
    carousel.replace_authored_at_boundary(absent()).unwrap();
    carousel
        .replace_authored_at_boundary(checked(1, 2, TeletextResourceFullUpdate::Unchanged))
        .unwrap();
    assert_counter(&cycle(&mut carousel), 7, false);
    carousel.replace_authored_at_boundary(absent()).unwrap();
    cycle(&mut carousel);
    carousel
        .replace_authored_at_boundary(checked(1, 2, TeletextResourceFullUpdate::Unchanged))
        .unwrap();
    assert_counter(&cycle(&mut carousel), 0, true);
}

#[test]
fn strict_services_after_checked_completion_use_actual_counter_and_reject_stale_metadata() {
    let mut carousel =
        TeletextCarousel::new_authored(body(1, 1, TeletextResourceFullUpdate::Changed)).unwrap();
    cycle(&mut carousel);
    carousel
        .replace_authored_at_boundary(checked(1, 2, TeletextResourceFullUpdate::Changed))
        .unwrap();
    assert_counter(&cycle(&mut carousel), 2, true);
    assert!(matches!(
        carousel.replace_authored_at_boundary(body(1, 2, TeletextResourceFullUpdate::Unchanged)),
        Err(TeletextTransportError::ResourceUpdate { .. })
    ));
    carousel
        .replace_authored_at_boundary(body(2, 2, TeletextResourceFullUpdate::Unchanged))
        .unwrap();
    assert_counter(&cycle(&mut carousel), 2, false);
}

#[test]
fn discarded_preparation_and_midcycle_rejection_preserve_selected_state_and_shared_content() {
    let old = Arc::new(body(1, 1, TeletextResourceFullUpdate::Changed));
    let mut carousel = TeletextCarousel::new_authored_shared(Arc::clone(&old)).unwrap();
    cycle(&mut carousel);
    let next = Arc::new(checked(1, 2, TeletextResourceFullUpdate::Changed));
    let prepared = carousel
        .prepare_authored_shared_replacement_at_boundary(Arc::clone(&next))
        .unwrap();
    assert!(Arc::ptr_eq(
        carousel.authored_service_shared().unwrap(),
        &old
    ));
    drop(prepared);
    assert_counter(&cycle(&mut carousel), 1, false);
    carousel.next_batch().unwrap();
    assert!(matches!(
        carousel.prepare_authored_shared_replacement_at_boundary(Arc::clone(&next)),
        Err(TeletextTransportError::MidCycleReplacement)
    ));
    assert!(Arc::ptr_eq(
        carousel.authored_service_shared().unwrap(),
        &old
    ));
    cycle(&mut carousel);
    carousel
        .replace_authored_shared_at_boundary(Arc::clone(&next))
        .unwrap();
    assert_counter(&cycle(&mut carousel), 2, true);
    // Retain another Arc so the following replacement records resource-only
    // shared history with effective counter2, not authored baseline counter1.
    carousel
        .replace_authored_at_boundary(checked(1, 3, TeletextResourceFullUpdate::Changed))
        .unwrap();
    assert_counter(&cycle(&mut carousel), 3, true);
    assert_eq!(
        next.resource_lifecycle_variants().unwrap().entries().len(),
        next.pages()
            .iter()
            .filter(|page| page.resource_header().is_some())
            .count()
    );
}

#[test]
fn completed_legacy_owner_reuse_retains_selected_counter_tombstone() {
    let make = |bits, checked_plan| {
        let mut value = spec(1, TeletextResourceFullUpdate::Unchanged);
        value.page = 0x20;
        value.glyph = Some((0, bits));
        let service = candidate(value);
        if checked_plan {
            service
                .with_resource_lifecycle_variants(deadline())
                .unwrap()
        } else {
            service
        }
    };
    let mut carousel = TeletextCarousel::new_authored(make(1, false)).unwrap();
    cycle(&mut carousel);
    carousel
        .replace_authored_at_boundary(make(2, true))
        .unwrap();
    cycle(&mut carousel);
    carousel
        .replace_at_boundary(crate::teletext::transport_tests::set(vec![
            crate::teletext::transport_tests::draft(220, 7, false),
        ]))
        .unwrap();
    cycle(&mut carousel);
    carousel
        .replace_authored_at_boundary(make(2, true))
        .unwrap();
    let batches = cycle(&mut carousel);
    let header = batches
        .iter()
        .flat_map(TeletextCarouselBatch::data_units)
        .find_map(|unit| match observe_dvb_teletext_header(unit).unwrap() {
            TeletextHeaderObservation::Header(header) if header.wire_page == 0x20 => Some(header),
            _ => None,
        })
        .unwrap();
    assert_eq!((header.subcode >> 4) & 7, 3);
    assert!(header.controls.erase_page());
}
