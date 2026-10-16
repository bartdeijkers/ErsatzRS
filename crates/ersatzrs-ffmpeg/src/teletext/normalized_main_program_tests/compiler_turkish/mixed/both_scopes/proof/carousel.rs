//! All physical resources must be acquired before the eight original displays.

use super::*;
use crate::teletext::{
    TeletextCarousel, TeletextCarouselBatchKind, TeletextPhysicalPageInventoryRecord,
};
use ersatzrs_domain::{TeletextResourceHeaderMetadata, TeletextResourceTransmission};
use std::sync::Arc;

fn batch(
    carousel: &mut TeletextCarousel,
    kind: TeletextCarouselBatchKind,
    units: &[[u8; 46]],
    ordinal: &mut u64,
) {
    let actual = carousel.next_batch().unwrap();
    assert_eq!(actual.kind(), kind);
    assert_eq!(actual.data_units(), units);
    assert_eq!(actual.presentation_ticks(), *ordinal * 1_800);
    assert!(actual.data_units().len() <= 31);
    *ordinal += 1;
}

fn repeat_header(page: &EncodedTeletextAuthoredPage) -> [u8; 46] {
    if let Some(header) = page.resource_header() {
        let repeated = TeletextResourceHeaderMetadata::new(
            header.identity(),
            header.last_packet(),
            header.update_counter(),
            TeletextResourceTransmission::FullUnchangedFirst,
        )
        .unwrap();
        crate::teletext::encode_teletext_resource_header(
            repeated,
            crate::teletext::TeletextResourceTransmissionMode::Serial,
        )
    } else {
        let mut header = page.data_units()[0];
        let TeletextPhysicalPageInventoryRecord::Display(address) = page.inventory_record() else {
            panic!("non-resource physical pages must be displays");
        };
        header[9] =
            crate::teletext::packets::hamming_8_4(address.subcode.get().to_le_bytes()[0] >> 4)
                .reverse_bits();
        header
    }
}

fn complete_cycle(
    carousel: &mut TeletextCarousel,
    service: &EncodedTeletextAuthoredService,
    repeat: bool,
    ordinal: &mut u64,
) {
    assert!(service.magazine_units().is_empty());
    // All three banks have the same acquisition rank; sealed physical order
    // keeps the authored Normal bank, generated Normal, then generated Global.
    for index in (8..11).chain(0..8) {
        let page = &service.pages()[index];
        let units = page.data_units();
        let header = if repeat {
            repeat_header(page)
        } else {
            units[0]
        };
        batch(
            carousel,
            TeletextCarouselBatchKind::Header,
            &[header],
            ordinal,
        );
        for body in units[1..].chunks(31) {
            batch(carousel, TeletextCarouselBatchKind::Page, body, ordinal);
        }
    }
    let terminator = crate::teletext::packets::terminator(
        service
            .declared()
            .semantic()
            .base()
            .initial_page
            .wire_magazine(),
    );
    batch(
        carousel,
        TeletextCarouselBatchKind::Terminator,
        &[terminator],
        ordinal,
    );
    assert!(carousel.is_cycle_boundary());
}

#[test]
fn both_scope_national_variants_carousel_acquires_every_physical_bank_and_display() {
    let input = both_scopes();
    assert_eq!(input.resource_inventory(), &[authored_bank().identity()]);
    let fallback = assert_originals(&input);
    let encoded = encode_teletext_authored_service(
        input.clone(),
        &authored::transmissions(&input),
        &service_budget(),
    )
    .unwrap();
    assert_output(&input, &encoded, &fallback);
    assert_eq!(encoded.pages().len(), 11);
    let resources: Vec<_> = encoded.pages()[8..]
        .iter()
        .map(|page| page.resource_header().unwrap())
        .collect();
    assert_eq!(
        resources
            .iter()
            .map(|header| header.identity().kind())
            .collect::<Vec<_>>(),
        vec![
            TeletextResourceKind::Drcs,
            TeletextResourceKind::Drcs,
            TeletextResourceKind::Gdrcs
        ]
    );
    assert_eq!(resources[0].update_counter().get(), 1);
    assert_eq!(
        resources[0].transmission(),
        TeletextResourceTransmission::FullChangedFirst
    );
    for header in &resources[1..] {
        assert_eq!(header.update_counter().get(), 0);
        assert_eq!(
            header.transmission(),
            TeletextResourceTransmission::FullUnchangedFirst
        );
    }
    let original_units = units(&encoded);
    let service = Arc::new(encoded);
    let mut carousel = TeletextCarousel::new_authored_shared(Arc::clone(&service)).unwrap();
    let mut ordinal = 0;
    complete_cycle(&mut carousel, &service, false, &mut ordinal);
    complete_cycle(&mut carousel, &service, true, &mut ordinal);
    assert_eq!(units(&service), original_units);
    assert_eq!(service.declared(), &input);
}
