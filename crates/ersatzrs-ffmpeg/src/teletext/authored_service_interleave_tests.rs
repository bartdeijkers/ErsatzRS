//! Resource refresh preserves whole-display order and the only cycle boundary.

use super::authored_service_carousel_tests::{cycle, many_displays, service_with_resources};
use super::*;

fn resource_headers(batches: &[TeletextCarouselBatch]) -> Vec<&TeletextCarouselBatch> {
    batches
        .iter()
        .filter(|batch| batch.kind() == TeletextCarouselBatchKind::Header)
        // The fixture's resource page is 2C0; its display page is 200.
        .filter(|batch| batch.data_units()[0][7] != packets::hamming_8_4(0).reverse_bits())
        .collect()
}

#[test]
fn long_display_cycle_refreshes_without_replacing_or_reordering_displays() {
    let original = many_displays(499, true);
    let display_headers: Vec<_> = original.pages()[..499]
        .iter()
        .map(|page| page.data_units()[0])
        .collect();
    let magazines = original.magazine_units().to_vec();
    let mut carousel = TeletextCarousel::new_authored(original).unwrap();
    let batches = cycle(&mut carousel);
    assert_eq!(batches.len(), 1_005);
    assert_eq!(carousel.ticks, 1_005 * 1_800);
    let starts: Vec<_> = batches
        .iter()
        .filter(|batch| batch.kind() == TeletextCarouselBatchKind::Magazine)
        .map(TeletextCarouselBatch::presentation_ticks)
        .collect();
    assert_eq!(starts, [0, 997 * 1_800]);
    for batch in batches
        .iter()
        .filter(|batch| batch.kind() == TeletextCarouselBatchKind::Magazine)
    {
        assert_eq!(batch.data_units(), magazines);
    }
    let actual: Vec<_> = batches
        .iter()
        .filter(|batch| batch.kind() == TeletextCarouselBatchKind::Header)
        .filter(|batch| batch.data_units()[0][7] == packets::hamming_8_4(0).reverse_bits())
        .map(|batch| batch.data_units()[0])
        .collect();
    assert_eq!(actual, display_headers);
    assert_eq!(resource_headers(&batches).len(), 2);
    assert_eq!(
        batches
            .iter()
            .filter(|batch| batch.kind() == TeletextCarouselBatchKind::Terminator)
            .count(),
        1
    );
    let second = cycle(&mut carousel);
    let mut starts = starts;
    starts.extend(
        second
            .iter()
            .filter(|batch| batch.kind() == TeletextCarouselBatchKind::Magazine)
            .map(TeletextCarouselBatch::presentation_ticks),
    );
    assert!(starts
        .windows(2)
        .all(|pair| pair[1] - pair[0] <= 999 * 1_800));
    assert!(carousel.is_cycle_boundary());
}

#[test]
fn first_long_cycle_resource_repeat_clears_c4_c8_without_erasing_later_displays() {
    let mut candidates = vec![many_displays(499, true)];
    let declared = candidates[0].declared().clone();
    let mut tx = crate::teletext::authored_service_tests::transmissions(&declared);
    for record in &mut tx {
        record.update = crate::teletext::TeletextResourceFullUpdate::Unchanged;
    }
    let mut limits = crate::teletext::authored_service_tests::limits();
    limits.max_pages = 600;
    limits.max_units = 20_000;
    candidates
        .push(crate::teletext::encode_teletext_authored_service(declared, &tx, &limits).unwrap());
    for candidate in candidates {
        let expected_last_display = candidate.pages()[498].data_units()[0];
        let mut carousel = TeletextCarousel::new_authored(candidate).unwrap();
        let original = carousel.content.page(0).0.to_vec();
        let subcode = carousel.content.page(0).1;
        let resource = carousel.content.page(0).2.unwrap();
        let batches = cycle(&mut carousel);
        let headers = resource_headers(&batches);
        assert_eq!(headers[0].data_units(), &original[..1]);
        let mut repeated = original[0];
        repeated[9] = packets::hamming_8_4(subcode.to_le_bytes()[0] >> 4).reverse_bits();
        if resource.transmission().update_indicator() {
            repeated[12] ^= (packets::hamming_8_4(0) ^ packets::hamming_8_4(2)).reverse_bits();
        }
        assert_eq!(headers[1].data_units(), &[repeated]);
        assert_eq!(carousel.content.page(0).0, original);
        let last_display = batches
            .iter()
            .filter(|batch| batch.kind() == TeletextCarouselBatchKind::Header)
            .rfind(|batch| batch.data_units()[0][7] == packets::hamming_8_4(0).reverse_bits())
            .unwrap();
        assert_eq!(last_display.data_units(), &[expected_last_display]);
        for header in headers {
            let index = batches
                .iter()
                .position(|batch| std::ptr::eq(batch, header))
                .unwrap();
            assert_eq!(batches[index + 1].data_units(), &original[1..]);
        }
    }
}

#[test]
fn magazine_only_refresh_restores_the_exact_display_cursor() {
    let original = service_with_resources(500, 0, true);
    let expected: Vec<_> = original
        .pages()
        .iter()
        .map(|page| page.data_units()[0])
        .collect();
    let mut carousel = TeletextCarousel::new_authored(original).unwrap();
    let batches = cycle(&mut carousel);
    let actual: Vec<_> = batches
        .iter()
        .filter(|batch| batch.kind() == TeletextCarouselBatchKind::Header)
        .map(|batch| batch.data_units()[0])
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(
        batches
            .iter()
            .filter(|batch| batch.kind() == TeletextCarouselBatchKind::Magazine)
            .count(),
        2
    );
    assert_eq!(batches.len(), 1_003);
}

#[test]
fn resource_only_refresh_starts_its_interval_at_the_actual_header() {
    let mut carousel =
        TeletextCarousel::new_authored(service_with_resources(500, 1, false)).unwrap();
    let mut starts = vec![];
    for _ in 0..3 {
        let batches = cycle(&mut carousel);
        assert!(!batches
            .iter()
            .any(|batch| batch.kind() == TeletextCarouselBatchKind::Magazine));
        let headers = resource_headers(&batches);
        assert_eq!(headers.len(), 2);
        starts.extend(
            headers
                .into_iter()
                .map(TeletextCarouselBatch::presentation_ticks),
        );
    }
    assert!(starts
        .windows(2)
        .all(|pair| pair[1] - pair[0] <= 999 * 1_800));
}

#[test]
fn refresh_is_not_a_replacement_boundary_and_overflow_retains_resume_state() {
    let mut carousel = TeletextCarousel::new_authored(many_displays(499, true)).unwrap();
    for _ in 0..997 {
        carousel.next_batch().unwrap();
    }
    assert_eq!(
        carousel.next_batch().unwrap().kind(),
        TeletextCarouselBatchKind::Magazine
    );
    assert_eq!(carousel.resource_resume, Some(498));
    assert!(!carousel.is_cycle_boundary());
    assert_eq!(
        carousel.replace_authored_at_boundary(many_displays(1, true)),
        Err(TeletextTransportError::MidCycleReplacement)
    );
    let state = (
        carousel.page,
        carousel.unit,
        carousel.resource_resume,
        carousel.resource_start_ticks,
    );
    carousel.ticks = u64::MAX - 1_799;
    assert_eq!(
        carousel.next_batch(),
        Err(TeletextTransportError::ClockOverflow)
    );
    assert_eq!(
        (
            carousel.page,
            carousel.unit,
            carousel.resource_resume,
            carousel.resource_start_ticks
        ),
        state
    );
}
