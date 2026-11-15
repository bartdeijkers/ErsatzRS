use super::{encode_teletext_authored_service, EncodedTeletextAuthoredService};
use crate::teletext::authored_service_tests as fixtures;
use crate::teletext::carousel::authored_service_carousel_tests::{cycle as sweep, many_displays};
use crate::teletext::{TeletextCarousel, TeletextCarouselBatchKind};
use ersatzrs_domain::*;

fn cycle(page: u16, seconds: u16) -> TeletextSubpageCycle {
    TeletextSubpageCycle::new(TeletextPageNumber::new(page).unwrap(), seconds).unwrap()
}

#[test]
fn published_subpage_cycle_metadata_never_changes_encoded_page_bytes() {
    let service = many_displays(2, false);
    let page = service.declared().semantic().base().pages[0]
        .address
        .page
        .get();
    let before: Vec<_> = service
        .pages()
        .iter()
        .flat_map(super::EncodedTeletextAuthoredPage::data_units)
        .copied()
        .collect();
    let service = service.with_subpage_cycles(vec![cycle(page, 2)]).unwrap();
    assert_eq!(service.subpage_cycles(), [cycle(page, 2)]);
    let after: Vec<_> = service
        .pages()
        .iter()
        .flat_map(super::EncodedTeletextAuthoredPage::data_units)
        .copied()
        .collect();
    assert_eq!(before, after);
    let service = service
        .with_resource_lifecycle_variants(
            std::time::Instant::now() + std::time::Duration::from_secs(30),
        )
        .unwrap();
    assert_eq!(service.subpage_cycles(), [cycle(page, 2)]);
}

#[test]
fn published_subpage_cycle_rejects_duplicate_unknown_and_excess_owners() {
    let service = many_displays(2, false);
    let page = service.declared().semantic().base().pages[0]
        .address
        .page
        .get();
    assert_eq!(
        service
            .with_subpage_cycles(vec![cycle(page, 2), cycle(page, 2)])
            .unwrap_err(),
        TeletextSubpageCycleError::Duplicate { page }
    );
    assert_eq!(
        many_displays(2, false)
            .with_subpage_cycles(vec![cycle(899, 2)])
            .unwrap_err(),
        TeletextSubpageCycleError::MissingPage { page: 899 }
    );
    assert_eq!(
        many_displays(2, false)
            .with_subpage_cycles(vec![cycle(page, 2); 3])
            .unwrap_err(),
        TeletextSubpageCycleError::Count
    );
}

#[test]
fn published_subpage_cycle_holds_the_first_subpage_for_its_full_second() {
    let service = many_displays(2, false);
    assert_eq!(service.pages().len(), 2);
    let page = service.declared().semantic().base().pages[0]
        .address
        .page
        .get();
    // These two fixture subcodes differ in the low nibble, independently of C4.
    let headers = [
        service.pages()[0].data_units()[0][8],
        service.pages()[1].data_units()[0][8],
    ];
    assert_ne!(headers[0], headers[1]);
    let mut carousel =
        TeletextCarousel::new_authored(service.with_subpage_cycles(vec![cycle(page, 1)]).unwrap())
            .unwrap();
    let mut first_next = None;
    for _ in 0..120 {
        let batch = carousel.next_batch().unwrap();
        if batch.kind() != TeletextCarouselBatchKind::Header {
            continue;
        }
        let subcode = batch.data_units()[0][8];
        if batch.presentation_ticks() < 90_000 {
            assert_eq!(
                subcode, headers[0],
                "A subpage advanced before its published dwell time"
            );
        } else if subcode == headers[1] {
            first_next.get_or_insert(batch.presentation_ticks());
        }
    }
    let first_next = first_next.expect("The next subpage must become reachable");
    // The small resource-free fixture finishes its entire sweep in <=100 ms.
    assert!(
        (90_000..=99_000).contains(&first_next),
        "first advance at {first_next} ticks"
    );
}

fn service(addresses: &[(u16, u16)]) -> EncodedTeletextAuthoredService {
    let mut input = fixtures::input();
    let base = input.base.pages[0].clone();
    let enhancement = input.pages[1].clone();
    input.base.pages = addresses
        .iter()
        .map(|(page, subcode)| {
            let mut value = base.clone();
            value.address = TeletextPageAddress {
                page: TeletextPageNumber::new(*page).unwrap(),
                subcode: TeletextSubcode::new(*subcode).unwrap(),
            };
            value
        })
        .collect();
    input.base.initial_page = input.base.pages[0].address.page;
    input.pages = input
        .base
        .pages
        .iter()
        .map(|base| {
            let mut value = enhancement.clone();
            value.address = base.address;
            value.local_program = None;
            value.resources.objects = TeletextResolvedObjectPageLinks {
                public: None,
                global: None,
            };
            value.resources.defaults = [None; 4];
            value
        })
        .collect();
    input.objects.clear();
    input.drcs.clear();
    input.magazines.clear();
    let semantic = TeletextSemanticService::new(input, fixtures::semantic_limits()).unwrap();
    let selections = semantic
        .base()
        .pages
        .iter()
        .map(|page| TeletextDisplayMotSelection {
            display: page.address,
            mot: None,
        })
        .collect();
    let mut declarations = fixtures::declaration_limits();
    declarations.max_displays = 64;
    let declared =
        TeletextDeclaredService::new(semantic, Vec::new(), selections, Vec::new(), declarations)
            .unwrap();
    let mut budget = fixtures::limits();
    budget.max_pages = 64;
    budget.max_units = 2_048;
    encode_teletext_authored_service(declared, &[], &budget).unwrap()
}

const HAM: [u8; 16] = [
    0x15, 0x02, 0x49, 0x5e, 0x64, 0x73, 0x38, 0x2f, 0xd0, 0xc7, 0x8c, 0x9b, 0xa1, 0xb6, 0xfd, 0xea,
];
fn erase(header: &[u8; 46]) -> bool {
    HAM.iter()
        .position(|value| *value == header[9].reverse_bits())
        .unwrap()
        & 8
        != 0
}
fn comparable(mut header: [u8; 46]) -> [u8; 46] {
    let nibble = HAM
        .iter()
        .position(|value| *value == header[9].reverse_bits())
        .unwrap();
    header[9] = HAM[nibble & 7].reverse_bits();
    header
}
fn references(service: &EncodedTeletextAuthoredService) -> Vec<(TeletextPageAddress, [u8; 46])> {
    service
        .pages()
        .iter()
        .filter_map(|page| match page.inventory_record() {
            crate::teletext::TeletextPhysicalPageInventoryRecord::Display(address) => {
                Some((address, comparable(page.data_units()[0])))
            }
            crate::teletext::TeletextPhysicalPageInventoryRecord::Resource(_) => None,
        })
        .collect()
}
fn address(
    batch: &crate::teletext::TeletextCarouselBatch,
    references: &[(TeletextPageAddress, [u8; 46])],
) -> Option<TeletextPageAddress> {
    if batch.kind() != TeletextCarouselBatchKind::Header {
        return None;
    }
    let header = comparable(batch.data_units()[0]);
    references
        .iter()
        .find_map(|(address, reference)| (*reference == header).then_some(*address))
}

#[test]
fn published_subpage_cycles_rotate_independent_interleaved_pages_without_skips() {
    let source = service(&[(200, 0), (201, 0), (200, 1), (201, 1)]);
    let references = references(&source);
    let mut carousel = TeletextCarousel::new_authored(
        source
            .with_subpage_cycles(vec![cycle(200, 1), cycle(201, 2)])
            .unwrap(),
    )
    .unwrap();
    let mut current = std::collections::BTreeMap::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut transitions = std::collections::BTreeMap::<u16, usize>::new();
    for _ in 0..600 {
        let batch = carousel.next_batch().unwrap();
        let Some(address) = address(&batch, &references) else {
            continue;
        };
        assert_eq!(erase(&batch.data_units()[0]), seen.insert(address));
        let page = address.page.get();
        let subcode = address.subcode.get();
        let entry = current
            .entry(page)
            .or_insert((0, batch.presentation_ticks()));
        if entry.0 != subcode {
            assert_eq!(subcode, (entry.0 + 1) % 2);
            let dwell = if page == 200 { 90_000 } else { 180_000 };
            assert!((dwell..=dwell + 9_000).contains(&(batch.presentation_ticks() - entry.1)));
            *entry = (subcode, batch.presentation_ticks());
            *transitions.entry(page).or_default() += 1;
        }
    }
    assert!(transitions[&200] >= 8);
    assert!(transitions[&201] >= 4);
    assert_eq!(seen.len(), 4);
}

#[test]
fn published_subpage_cycle_prediction_matches_actual_timed_sweeps() {
    let mut carousel = TeletextCarousel::new_authored(
        service(&[(200, 0), (201, 0), (200, 1), (201, 1)])
            .with_subpage_cycles(vec![cycle(200, 1), cycle(201, 2)])
            .unwrap(),
    )
    .unwrap();
    for _ in 0..100 {
        let count = carousel.batches_until_boundary(20).unwrap().unwrap();
        for index in 0..count {
            let batch = carousel.next_batch().unwrap();
            assert_eq!(
                batch.kind() == TeletextCarouselBatchKind::Terminator,
                index + 1 == count
            );
        }
        // Query both initial boundaries and an already started display.
        carousel.next_batch().unwrap();
    }
}

#[test]
fn published_subpage_cycle_refresh_keeps_phase_and_erases_unseen_subpages() {
    let source = service(&[(200, 0), (200, 1)])
        .with_subpage_cycles(vec![cycle(200, 1)])
        .unwrap();
    let references = references(&source);
    let mut carousel = TeletextCarousel::new_authored(source).unwrap();
    for _ in 0..5 {
        sweep(&mut carousel);
    }
    let next = std::sync::Arc::new(
        service(&[(200, 0), (200, 1)])
            .with_subpage_cycles(vec![cycle(200, 1)])
            .unwrap(),
    );
    let prepared = carousel
        .prepare_authored_display_refresh_at_boundary(&next)
        .unwrap();
    carousel.commit_prepared_replacement(prepared);
    let header = carousel.next_batch().unwrap();
    assert_eq!(address(&header, &references).unwrap().subcode.get(), 0);
    assert!(!erase(&header.data_units()[0]));
    let mut first_next = None;
    for _ in 0..100 {
        let batch = carousel.next_batch().unwrap();
        if address(&batch, &references).is_some_and(|address| address.subcode.get() == 1) {
            assert!(erase(&batch.data_units()[0]));
            first_next = Some(batch.presentation_ticks());
            break;
        }
    }
    assert!((90_000..=99_000).contains(&first_next.unwrap()));
}

#[test]
fn published_subpage_cycle_pending_first_header_waits_but_repeats_do_not_renew_dwell() {
    for first in [true, false] {
        let source = service(&[(200, 0), (200, 1)])
            .with_subpage_cycles(vec![cycle(200, 1)])
            .unwrap();
        let references = references(&source);
        let mut carousel = TeletextCarousel::new_authored(source).unwrap();
        let first_header = carousel.next_batch().unwrap();
        assert_eq!(first_header.kind(), TeletextCarouselBatchKind::Header);
        if !first {
            loop {
                if carousel.next_batch().unwrap().kind() == TeletextCarouselBatchKind::Header {
                    break;
                }
            }
        }
        for _ in 0..10 {
            carousel.defer_pending_batch().unwrap();
        }
        let mut next_tick = None;
        for _ in 0..150 {
            let batch = carousel.next_batch().unwrap();
            if address(&batch, &references).is_some_and(|address| address.subcode.get() == 1) {
                next_tick = Some(batch.presentation_ticks());
                break;
            }
        }
        let lower = if first { 108_000 } else { 90_000 };
        assert!((lower..=lower + 9_000).contains(&next_tick.unwrap()));
    }
}

#[test]
fn published_subpage_cycle_does_not_pause_resource_acquisition_during_long_dwell() {
    let source = many_displays(4, true);
    let page = source.declared().semantic().base().pages[0]
        .address
        .page
        .get();
    let references = references(&source);
    let mut carousel =
        TeletextCarousel::new_authored(source.with_subpage_cycles(vec![cycle(page, 600)]).unwrap())
            .unwrap();
    let mut last = None;
    let mut acquisitions = 0;
    for _ in 0..3_000 {
        let batch = carousel.next_batch().unwrap();
        if batch.kind() == TeletextCarouselBatchKind::Magazine {
            if let Some(last) = last {
                assert!(batch.presentation_ticks() - last <= 1_800_000);
            }
            last = Some(batch.presentation_ticks());
            acquisitions += 1;
        }
        if let Some(address) = address(&batch, &references) {
            assert_eq!(address.subcode.get(), 0);
        }
    }
    assert!(acquisitions > 10);
}

#[test]
fn published_subpage_cycle_capacity_reserves_room_for_acquisition_and_sweeps() {
    let pages: Vec<_> = (200..225).flat_map(|page| [(page, 0), (page, 1)]).collect();
    let timing: Vec<_> = (200..225).map(|page| cycle(page, 1)).collect();
    let source = service(&pages).with_subpage_cycles(timing).unwrap();
    assert!(matches!(
        TeletextCarousel::validate_authored_timing(&source),
        Err(crate::teletext::TeletextTransportError::SubpageCapacity)
    ));
    assert!(matches!(
        TeletextCarousel::new_authored(source),
        Err(crate::teletext::TeletextTransportError::SubpageCapacity)
    ));
    let pages: Vec<_> = (200..224).flat_map(|page| [(page, 0), (page, 1)]).collect();
    let timing: Vec<_> = (200..224).map(|page| cycle(page, 1)).collect();
    let source = service(&pages).with_subpage_cycles(timing).unwrap();
    TeletextCarousel::validate_authored_timing(&source).unwrap();
    TeletextCarousel::new_authored(source).unwrap();
}

#[test]
fn published_subpage_cycle_does_not_reduce_existing_long_sweep_capacity() {
    let mut pages = vec![(200, 0), (200, 1)];
    pages.extend((201..231).map(|page| (page, 0)));
    TeletextCarousel::new_authored(service(&pages)).unwrap();
    let source = service(&pages)
        .with_subpage_cycles(vec![cycle(200, 1)])
        .unwrap();
    let references = references(&source);
    let mut carousel = TeletextCarousel::new_authored(source).unwrap();
    let mut current = (0, 0);
    let mut changes = 0;
    for _ in 0..200 {
        let batch = carousel.next_batch().unwrap();
        let Some(address) = address(&batch, &references) else {
            continue;
        };
        if address.page.get() != 200 || address.subcode.get() == current.0 {
            continue;
        }
        assert!((90_000..=99_000).contains(&(batch.presentation_ticks() - current.1)));
        current = (address.subcode.get(), batch.presentation_ticks());
        changes += 1;
    }
    assert!(changes >= 3);
}
