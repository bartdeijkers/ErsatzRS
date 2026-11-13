//! Shared checked replacement and withdrawn-clock prerequisites, not delivery.

use super::authored_service_carousel_tests::{cycle, service_with_resources};
use super::*;
use crate::teletext::authored_service_tests as fixtures;
use crate::teletext::service_update::tests::{candidate, spec};
use crate::teletext::TeletextResourceFullUpdate::{Changed, Unchanged};

fn cursor(
    carousel: &TeletextCarousel,
) -> (usize, usize, u64, bool, bool, bool, u64, Option<usize>) {
    (
        carousel.page,
        carousel.unit,
        carousel.ticks,
        carousel.boundary,
        carousel.first_cycle,
        carousel.magazine_pending,
        carousel.resource_start_ticks,
        carousel.resource_resume,
    )
}

fn retained(carousel: &TeletextCarousel) -> &Arc<EncodedTeletextAuthoredService> {
    let CarouselContent::Authored { service, .. } = &carousel.content else {
        panic!("authored content required");
    };
    service
}

#[test]
fn shared_replacement_retains_exact_service_allocation_and_completed_clock() {
    let old = Arc::new(candidate(spec(7, Changed)));
    let mut carousel = TeletextCarousel::new_authored_shared(Arc::clone(&old)).unwrap();
    cycle(&mut carousel);
    let ticks = carousel.ticks;
    let next = Arc::new(candidate(spec(7, Unchanged)));
    let page_pointer = next.pages()[0].data_units().as_ptr();
    let before = cursor(&carousel);
    carousel
        .validate_shared_replacement_at_boundary(&next)
        .unwrap();
    assert_eq!(cursor(&carousel), before);
    assert!(Arc::ptr_eq(retained(&carousel), &old));
    carousel
        .replace_authored_shared_at_boundary(Arc::clone(&next))
        .unwrap();
    assert!(Arc::ptr_eq(retained(&carousel), &next));
    assert_eq!(
        retained(&carousel).pages()[0].data_units().as_ptr(),
        page_pointer
    );
    assert_eq!(Arc::strong_count(&next), 2);
    assert_eq!(Arc::strong_count(&old), 1);
    assert_eq!(carousel.ticks, ticks);
    assert_eq!(carousel.next_batch().unwrap().presentation_ticks(), ticks);
}

#[test]
fn prepared_replacement_can_be_discarded_and_commits_exact_shared_content_infallibly() {
    let old = Arc::new(candidate(spec(7, Changed)));
    let mut carousel = TeletextCarousel::new_authored_shared(Arc::clone(&old)).unwrap();
    cycle(&mut carousel);
    let before = cursor(&carousel);
    let next = Arc::new(candidate(spec(7, Unchanged)));
    let prepared = carousel
        .prepare_authored_shared_replacement_at_boundary(Arc::clone(&next))
        .unwrap();
    assert_eq!(cursor(&carousel), before);
    assert!(Arc::ptr_eq(retained(&carousel), &old));
    assert_eq!(Arc::strong_count(&next), 2);
    drop(prepared);
    assert_eq!(Arc::strong_count(&next), 1);
    assert_eq!(cursor(&carousel), before);
    let prepared = carousel
        .prepare_authored_shared_replacement_at_boundary(Arc::clone(&next))
        .unwrap();
    carousel.commit_prepared_replacement(prepared);
    assert!(Arc::ptr_eq(retained(&carousel), &next));
    assert_eq!(carousel.ticks, before.2);
    let stale = Arc::new(candidate(spec(6, Unchanged)));
    cycle(&mut carousel);
    let installed = cursor(&carousel);
    assert!(matches!(
        carousel.prepare_authored_shared_replacement_at_boundary(stale),
        Err(TeletextTransportError::ResourceUpdate { .. })
    ));
    assert_eq!(cursor(&carousel), installed);
    assert!(Arc::ptr_eq(retained(&carousel), &next));
}

#[test]
fn stale_lifecycle_preflight_and_install_leave_clock_content_and_history_unchanged() {
    let old = Arc::new(candidate(spec(7, Changed)));
    let mut carousel = TeletextCarousel::new_authored_shared(Arc::clone(&old)).unwrap();
    cycle(&mut carousel);
    let stale = Arc::new(candidate(spec(6, Unchanged)));
    let before = cursor(&carousel);
    assert!(matches!(
        carousel.validate_shared_replacement_at_boundary(&stale),
        Err(TeletextTransportError::ResourceUpdate { .. })
    ));
    assert!(matches!(
        carousel.replace_authored_shared_at_boundary(Arc::clone(&stale)),
        Err(TeletextTransportError::ResourceUpdate { .. })
    ));
    assert_eq!(cursor(&carousel), before);
    assert!(Arc::ptr_eq(retained(&carousel), &old));
    assert_eq!(Arc::strong_count(&stale), 1);
    let valid = Arc::new(candidate(spec(0, Changed)));
    carousel
        .validate_shared_replacement_at_boundary(&valid)
        .unwrap();
    carousel.replace_authored_shared_at_boundary(valid).unwrap();
}

#[test]
fn shared_replacement_and_empty_advance_require_complete_cycle_boundary() {
    let old = Arc::new(candidate(spec(7, Changed)));
    let next = Arc::new(candidate(spec(7, Unchanged)));
    let mut carousel = TeletextCarousel::new_authored_shared(Arc::clone(&old)).unwrap();
    carousel.next_batch().unwrap();
    assert!(!carousel.is_cycle_boundary());
    let before = cursor(&carousel);
    assert_eq!(
        carousel.validate_shared_replacement_at_boundary(&next),
        Err(TeletextTransportError::MidCycleReplacement)
    );
    assert_eq!(
        carousel.replace_authored_shared_at_boundary(Arc::clone(&next)),
        Err(TeletextTransportError::MidCycleReplacement)
    );
    assert_eq!(
        carousel.advance_empty_at_boundary(1800),
        Err(TeletextTransportError::MidCycleReplacement)
    );
    assert_eq!(cursor(&carousel), before);
    assert!(Arc::ptr_eq(retained(&carousel), &old));
    assert_eq!(Arc::strong_count(&next), 1);
}

#[test]
fn paused_clock_advances_only_ticks_and_preserves_repeat_controls_and_lifecycle() {
    let old = Arc::new(candidate(spec(7, Changed)));
    let mut carousel = TeletextCarousel::new_authored_shared(Arc::clone(&old)).unwrap();
    cycle(&mut carousel);
    let before = cursor(&carousel);
    carousel.advance_empty_at_boundary(0).unwrap();
    assert_eq!(cursor(&carousel), before);
    let delta = 90_000;
    carousel.advance_empty_at_boundary(delta).unwrap();
    let mut expected = before;
    expected.2 += delta;
    assert_eq!(cursor(&carousel), expected);
    assert!(Arc::ptr_eq(retained(&carousel), &old));
    let stale = Arc::new(candidate(spec(6, Unchanged)));
    assert!(matches!(
        carousel.validate_shared_replacement_at_boundary(&stale),
        Err(TeletextTransportError::ResourceUpdate { .. })
    ));
    let repeated = carousel.next_batch().unwrap();
    assert_eq!(repeated.presentation_ticks(), expected.2);
    // Complete-cycle repeat state remains established, never first-cycle erasure.
    assert!(!carousel.first_cycle);
    cycle_boundary_for_overflow(&mut carousel);
}

fn cycle_boundary_for_overflow(carousel: &mut TeletextCarousel) {
    // Finish the actual repeat cycle before selecting boundary-clock arithmetic.
    cycle(carousel);
    carousel.ticks = u64::MAX - 1;
    let before = cursor(carousel);
    assert_eq!(
        carousel.advance_empty_at_boundary(2),
        Err(TeletextTransportError::ClockOverflow)
    );
    assert_eq!(cursor(carousel), before);
}

#[test]
fn acquisition_preflight_rejection_is_nonmutating_before_resource_order_allocation() {
    let old = Arc::new(candidate(spec(7, Changed)));
    let mut carousel = TeletextCarousel::new_authored_shared(Arc::clone(&old)).unwrap();
    cycle(&mut carousel);
    let excessive = Arc::new(service_with_resources(1, 500, true));
    let before = cursor(&carousel);
    assert!(matches!(
        carousel.validate_shared_replacement_at_boundary(&excessive),
        Err(TeletextTransportError::CycleTiming { .. })
    ));
    assert!(matches!(
        carousel.replace_authored_shared_at_boundary(Arc::clone(&excessive)),
        Err(TeletextTransportError::CycleTiming { .. })
    ));
    assert_eq!(cursor(&carousel), before);
    assert!(Arc::ptr_eq(retained(&carousel), &old));
    assert_eq!(Arc::strong_count(&excessive), 1);
}

#[test]
fn pure_boundary_prediction_matches_actual_prefix_at_initial_and_partial_cycles() {
    let service = Arc::new(candidate(spec(7, Changed)));
    let mut carousel = TeletextCarousel::new_authored_shared(Arc::clone(&service)).unwrap();
    let before = cursor(&carousel);
    assert_eq!(carousel.batches_until_boundary(0).unwrap(), None);
    let predicted = carousel.batches_until_boundary(100).unwrap().unwrap();
    assert!(predicted > 1);
    assert_eq!(
        carousel.batches_until_boundary(predicted - 1).unwrap(),
        None
    );
    assert_eq!(
        carousel.batches_until_boundary(predicted).unwrap(),
        Some(predicted)
    );
    assert_eq!(cursor(&carousel), before);
    assert!(Arc::ptr_eq(retained(&carousel), &service));
    assert_eq!(Arc::strong_count(&service), 2);
    carousel.next_batch().unwrap();
    assert_eq!(
        carousel.batches_until_boundary(100).unwrap(),
        Some(predicted - 1)
    );
    let remaining = cycle(&mut carousel);
    assert_eq!(remaining.len(), predicted - 1);
    assert_eq!(
        remaining.last().unwrap().kind(),
        TeletextCarouselBatchKind::Terminator
    );
    let repeat = carousel.batches_until_boundary(100).unwrap().unwrap();
    assert_eq!(repeat, cycle(&mut carousel).len());
}

#[test]
fn boundary_prediction_reuses_resource_refresh_resume_for_cycles_larger_than_window() {
    let service = Arc::new(service_with_resources(500, 1, false));
    let mut carousel = TeletextCarousel::new_authored_shared(Arc::clone(&service)).unwrap();
    let before = cursor(&carousel);
    assert_eq!(carousel.batches_until_boundary(1000).unwrap(), None);
    let predicted = carousel.batches_until_boundary(2000).unwrap().unwrap();
    assert!(predicted > 1000);
    assert_eq!(cursor(&carousel), before);
    let actual = cycle(&mut carousel);
    assert_eq!(predicted, actual.len());
    assert_eq!(
        actual
            .iter()
            .filter(|batch| batch.kind() == TeletextCarouselBatchKind::Terminator)
            .count(),
        1
    );
    // Resource refresh headers recur before the sole FF; they never masquerade
    // as a generation boundary in the pure prefix prediction.
    let resource_headers = actual
        .iter()
        .filter(|batch| {
            batch.kind() == TeletextCarouselBatchKind::Header
                && batch.data_units()[0][6..8] == service.pages()[500].data_units()[0][6..8]
        })
        .count();
    assert!(resource_headers > 1);
}

#[test]
fn boundary_prediction_overflow_changes_neither_clock_nor_service() {
    let service = Arc::new(candidate(spec(7, Changed)));
    let mut carousel = TeletextCarousel::new_authored_shared(Arc::clone(&service)).unwrap();
    carousel.ticks = u64::MAX - STEP_TICKS + 1;
    let before = cursor(&carousel);
    assert_eq!(
        carousel.batches_until_boundary(1),
        Err(TeletextTransportError::ClockOverflow)
    );
    assert_eq!(cursor(&carousel), before);
    assert!(Arc::ptr_eq(retained(&carousel), &service));
}

fn display_field_service(value: u8) -> Arc<EncodedTeletextAuthoredService> {
    use ersatzrs_domain::{TeletextPhysicalBaseOverlay, TeletextPhysicalBaseRow};
    let baseline = service_with_resources(3, 0, false);
    let owner = baseline.declared().semantic().base().pages[1].address;
    let mut cells = [b' '; 40];
    cells[0] = value;
    let declared = baseline
        .declared()
        .clone()
        .with_physical_base_overlays(
            vec![TeletextPhysicalBaseOverlay::new(
                owner,
                vec![TeletextPhysicalBaseRow::new(1, cells).unwrap()],
            )
            .unwrap()],
            fixtures::semantic_limits(),
        )
        .unwrap();
    Arc::new(
        crate::teletext::encode_teletext_authored_service(declared, &[], &fixtures::limits())
            .unwrap(),
    )
}

fn display_erase_flags(carousel: &mut TeletextCarousel) -> Vec<bool> {
    cycle(carousel)
        .iter()
        .filter(|batch| batch.kind() == TeletextCarouselBatchKind::Header)
        .map(|batch| match batch.data_units()[0][9].reverse_bits() {
            0x15 => false, // Hamming(0): repeat, no C4.
            0xd0 => true,  // Hamming(8): C4 erase, subcode high nibble zero.
            _ => panic!("fixture must use subcodes below16"),
        })
        .collect()
}

#[test]
fn display_refresh_erases_only_the_changed_physical_page_and_preserves_clock() {
    let mut carousel = TeletextCarousel::new_authored_shared(display_field_service(b'A')).unwrap();
    assert_eq!(display_erase_flags(&mut carousel), [true, true, true]);
    let ticks = carousel.ticks;
    let next = display_field_service(b'B');
    let prepared = carousel
        .prepare_authored_display_refresh_at_boundary(&next)
        .unwrap();
    assert_eq!(
        prepared
            .repeat_display_headers
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        [0, 2]
    );
    carousel.commit_prepared_replacement(prepared);
    assert_eq!(carousel.ticks, ticks);
    assert_eq!(display_erase_flags(&mut carousel), [false, true, false]);
    assert_eq!(display_erase_flags(&mut carousel), [false, false, false]);
}

#[test]
fn full_publication_still_acquires_every_display() {
    let mut carousel = TeletextCarousel::new_authored_shared(display_field_service(b'A')).unwrap();
    cycle(&mut carousel);
    carousel
        .replace_authored_shared_at_boundary(display_field_service(b'B'))
        .unwrap();
    assert_eq!(display_erase_flags(&mut carousel), [true, true, true]);
}

#[test]
fn display_refresh_never_treats_an_unchanged_resource_header_as_a_display_repeat() {
    let service = Arc::new(
        service_with_resources(3, 1, false)
            .with_resource_lifecycle_variants(
                std::time::Instant::now() + std::time::Duration::from_secs(30),
            )
            .unwrap(),
    );
    let mut carousel = TeletextCarousel::new_authored_shared(Arc::clone(&service)).unwrap();
    cycle(&mut carousel);
    let prepared = carousel
        .prepare_authored_display_refresh_at_boundary(&service)
        .unwrap();
    assert_eq!(prepared.repeat_display_headers.len(), 3);
    for logical in &prepared.repeat_display_headers {
        assert!(prepared.content.page(*logical).2.is_none());
    }
}

#[test]
fn changed_resource_banks_or_magazines_keep_full_acquisition() {
    for next in [
        service_with_resources(3, 1, false),
        service_with_resources(3, 0, true),
    ] {
        let mut carousel =
            TeletextCarousel::new_authored_shared(Arc::new(service_with_resources(3, 0, false)))
                .unwrap();
        cycle(&mut carousel);
        let prepared = carousel
            .prepare_authored_display_refresh_at_boundary(&Arc::new(next))
            .unwrap();
        assert!(prepared.repeat_display_headers.is_empty());
    }
}

#[test]
fn discarded_and_midcycle_display_refresh_cannot_change_the_current_carousel() {
    let mut carousel = TeletextCarousel::new_authored_shared(display_field_service(b'A')).unwrap();
    cycle(&mut carousel);
    let before = cursor(&carousel);
    drop(
        carousel
            .prepare_authored_display_refresh_at_boundary(&display_field_service(b'B'))
            .unwrap(),
    );
    assert_eq!(cursor(&carousel), before);
    assert!(carousel.repeat_display_headers.is_empty());
    carousel.next_batch().unwrap();
    let before = cursor(&carousel);
    assert!(matches!(
        carousel.prepare_authored_display_refresh_at_boundary(&display_field_service(b'B')),
        Err(TeletextTransportError::MidCycleReplacement)
    ));
    assert_eq!(cursor(&carousel), before);
}
