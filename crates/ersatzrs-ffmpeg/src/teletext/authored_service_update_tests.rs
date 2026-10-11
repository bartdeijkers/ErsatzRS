//! Consecutive generation replacement uses completed, rather than pending, resources.

use super::authored_service_carousel_tests::cycle;
use super::*;
use crate::teletext::service_update::tests::{candidate, spec};
use crate::teletext::transport_tests::{draft, set};
use crate::teletext::TeletextResourceFullUpdate::{Changed, Unchanged};

fn changed_body(counter: u8) -> EncodedTeletextAuthoredService {
    let mut value = spec(counter, Changed);
    value.glyph = Some((0, 2));
    candidate(value)
}

fn resource_units(carousel: &TeletextCarousel) -> Vec<[u8; 46]> {
    let CarouselContent::Authored { service, .. } = &carousel.content else {
        panic!("authored service required");
    };
    service
        .pages()
        .iter()
        .find(|page| {
            page.resource_header()
                .is_some_and(|header| header.identity().page().page() == 0xc0)
        })
        .unwrap()
        .data_units()
        .to_vec()
}

#[test]
fn changed_body_with_stale_lifecycle_is_rejected_atomically_and_old_bytes_repeat() {
    let mut old = spec(7, Changed);
    old.glyph = Some((0, 1));
    let mut carousel = TeletextCarousel::new_authored(candidate(old)).unwrap();
    let expected = cycle(&mut carousel);
    let cursor_snapshot = (
        carousel.page,
        carousel.unit,
        carousel.ticks,
        carousel.boundary,
        carousel.first_cycle,
        carousel.magazine_pending,
        carousel.resource_start_ticks,
        carousel.resource_resume,
    );
    let old_units = resource_units(&carousel);
    let mut stale = spec(7, Unchanged);
    stale.glyph = Some((0, 2));
    assert!(matches!(
        carousel.replace_authored_at_boundary(candidate(stale)),
        Err(TeletextTransportError::ResourceUpdate { .. })
    ));
    assert_eq!(
        (
            carousel.page,
            carousel.unit,
            carousel.ticks,
            carousel.boundary,
            carousel.first_cycle,
            carousel.magazine_pending,
            carousel.resource_start_ticks,
            carousel.resource_resume,
        ),
        cursor_snapshot
    );
    assert_eq!(resource_units(&carousel), old_units);
    let repeated = cycle(&mut carousel);
    assert_eq!(expected.len(), repeated.len());
    // Headers deliberately clear first-transmission flags; every resource and
    // display body remains the old immutable encoded content.
    for (old, new) in expected.iter().zip(&repeated) {
        assert_eq!(old.kind(), new.kind());
        if old.kind() == TeletextCarouselBatchKind::Page {
            assert_eq!(old.data_units(), new.data_units());
        }
    }
    carousel
        .replace_authored_at_boundary(changed_body(0))
        .unwrap();
}

#[test]
fn never_emitted_pending_candidate_does_not_advance_resource_history() {
    let mut carousel = TeletextCarousel::new_authored(candidate(spec(7, Changed))).unwrap();
    cycle(&mut carousel);
    let ticks = carousel.ticks;
    carousel
        .replace_authored_at_boundary(changed_body(0))
        .unwrap();
    assert_eq!(carousel.ticks, ticks);
    let pending_bytes = resource_units(&carousel);
    assert!(matches!(
        carousel.replace_authored_at_boundary(changed_body(1)),
        Err(TeletextTransportError::ResourceUpdate { .. })
    ));
    assert_eq!(resource_units(&carousel), pending_bytes);
    assert_eq!(carousel.ticks, ticks);
    // A remains the receiver baseline: C may reuse A's unchanged counter/body.
    carousel
        .replace_authored_at_boundary(candidate(spec(7, Unchanged)))
        .unwrap();
    cycle(&mut carousel);
    carousel
        .replace_authored_at_boundary(changed_body(0))
        .unwrap();
    cycle(&mut carousel);
    // B has now actually completed, so its next update uses counter 1.
    carousel
        .replace_authored_at_boundary(changed_body(1))
        .unwrap();
}

#[test]
fn pending_removal_can_be_cancelled_but_completed_removal_requires_changed_reintroduction() {
    let mut carousel = TeletextCarousel::new_authored(candidate(spec(7, Changed))).unwrap();
    cycle(&mut carousel);
    let mut absent = spec(0, Unchanged);
    absent.kind = None;
    carousel
        .replace_authored_at_boundary(candidate(absent))
        .unwrap();
    carousel
        .replace_authored_at_boundary(candidate(spec(7, Unchanged)))
        .unwrap();
    cycle(&mut carousel);
    carousel
        .replace_authored_at_boundary(candidate(absent))
        .unwrap();
    cycle(&mut carousel);
    assert!(matches!(
        carousel.replace_authored_at_boundary(candidate(spec(7, Unchanged))),
        Err(TeletextTransportError::ResourceUpdate { .. })
    ));
    carousel
        .replace_authored_at_boundary(candidate(spec(0, Changed)))
        .unwrap();
}

#[test]
fn legacy_transition_preserves_known_counters_and_retired_resource_history() {
    let mut carousel = TeletextCarousel::new_authored(candidate(spec(7, Changed))).unwrap();
    cycle(&mut carousel);
    carousel
        .replace_at_boundary(set(vec![draft(200, 0, false)]))
        .unwrap();
    cycle(&mut carousel);
    carousel
        .replace_authored_at_boundary(changed_body(0))
        .unwrap();
    cycle(&mut carousel);
    let mut absent = spec(0, Unchanged);
    absent.kind = None;
    carousel
        .replace_authored_at_boundary(candidate(absent))
        .unwrap();
    cycle(&mut carousel);
    carousel
        .replace_at_boundary(set(vec![draft(200, 0, false)]))
        .unwrap();
    cycle(&mut carousel);
    let mut stale = spec(0, Unchanged);
    stale.glyph = Some((0, 2));
    assert!(matches!(
        carousel.replace_authored_at_boundary(candidate(stale)),
        Err(TeletextTransportError::ResourceUpdate { .. })
    ));
    carousel
        .replace_authored_at_boundary(changed_body(1))
        .unwrap();
}

#[test]
fn cold_pending_services_have_no_invented_previous_receiver_counter() {
    let mut carousel = TeletextCarousel::new_authored(candidate(spec(7, Changed))).unwrap();
    carousel
        .replace_authored_at_boundary(changed_body(3))
        .unwrap();
    assert_eq!(carousel.ticks, 0);
    cycle(&mut carousel);
    assert!(matches!(
        carousel.replace_authored_at_boundary(changed_body(3)),
        Err(TeletextTransportError::ResourceUpdate { .. })
    ));
    carousel
        .replace_authored_at_boundary(changed_body(4))
        .unwrap();
}

#[test]
fn completed_legacy_page_reuse_retires_all_resource_s1_but_pending_legacy_does_not() {
    let mut value = spec(7, Changed);
    value.page = 0x20;
    let mut carousel = TeletextCarousel::new_authored(candidate(value)).unwrap();
    cycle(&mut carousel);
    carousel
        .replace_at_boundary(set(vec![draft(220, 7, false)]))
        .unwrap();
    value.update = Unchanged;
    // This legacy candidate has not transmitted any header: receiver A survives.
    carousel
        .replace_authored_at_boundary(candidate(value))
        .unwrap();
    cycle(&mut carousel);
    carousel
        .replace_at_boundary(set(vec![draft(220, 7, false)]))
        .unwrap();
    cycle(&mut carousel);
    let ticks = carousel.ticks;
    assert!(matches!(
        carousel.replace_authored_at_boundary(candidate(value)),
        Err(TeletextTransportError::ResourceUpdate { .. })
    ));
    assert_eq!(carousel.ticks, ticks);
    assert!(matches!(carousel.content, CarouselContent::Legacy(_)));
    assert!(carousel.is_cycle_boundary());
    value.counter = 0;
    value.update = Changed;
    carousel
        .replace_authored_at_boundary(candidate(value))
        .unwrap();
}
