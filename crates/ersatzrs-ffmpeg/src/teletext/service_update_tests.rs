//! Actual encoded full-resource changes, pending candidates and retired owners.

use super::*;
use crate::teletext::authored_service_tests as fixtures;
use crate::teletext::{encode_teletext_authored_service, TeletextResourceFullUpdate};
use ersatzrs_domain::*;

#[derive(Clone, Copy)]
pub(in crate::teletext) struct DrcsSpec {
    pub(in crate::teletext) kind: Option<TeletextResourceKind>,
    pub(in crate::teletext) page: u8,
    pub(in crate::teletext) subpage: u8,
    pub(in crate::teletext) counter: u8,
    pub(in crate::teletext) update: TeletextResourceFullUpdate,
    pub(in crate::teletext) glyph: Option<(u8, u16)>,
}

pub(in crate::teletext) fn spec(counter: u8, update: TeletextResourceFullUpdate) -> DrcsSpec {
    DrcsSpec {
        kind: Some(TeletextResourceKind::Drcs),
        page: 0xc0,
        subpage: 0,
        counter,
        update,
        glyph: None,
    }
}

fn identity(kind: TeletextResourceKind, subpage: u8) -> TeletextResourceIdentity {
    identity_at(kind, 2, 0xc0, subpage)
}

fn identity_at(
    kind: TeletextResourceKind,
    magazine: u8,
    page: u8,
    subpage: u8,
) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        kind,
        TeletextRawDataPageNumber::new(magazine, page).unwrap(),
        TeletextResourceSubpageIndex::new(subpage).unwrap(),
    )
    .unwrap()
}

pub(in crate::teletext) fn candidate(value: DrcsSpec) -> EncodedTeletextAuthoredService {
    let mut input = fixtures::input();
    input.drcs = value
        .kind
        .map(|kind| {
            let glyphs = value
                .glyph
                .map(|(slot, bits)| TeletextDrcsGlyphDefinition {
                    slot: TeletextDrcsGlyphSlot::new(slot).unwrap(),
                    bitmap: TeletextDrcsBitmap::new([bits; 10]).unwrap(),
                })
                .into_iter()
                .collect();
            TeletextDrcsSubtable::new(identity_at(kind, 2, value.page, value.subpage), glyphs)
                .unwrap()
        })
        .into_iter()
        .collect();
    let declared = fixtures::declared(input);
    let mut transmissions = fixtures::transmissions(&declared);
    for tx in &mut transmissions {
        tx.update = TeletextResourceFullUpdate::Unchanged;
        if Some(tx.resource.kind()) == value.kind && tx.resource.page().page() == value.page {
            tx.counter = TeletextResourceUpdateCounter::new(value.counter).unwrap();
            tx.update = value.update;
        }
    }
    encode_teletext_authored_service(declared, &transmissions, &fixtures::limits()).unwrap()
}

fn drcs(service: &EncodedTeletextAuthoredService) -> &EncodedTeletextAuthoredPage {
    service
        .pages()
        .iter()
        .find(|page| {
            page.resource_header()
                .is_some_and(|header| header.identity().page().page() == 0xc0)
        })
        .unwrap()
}

fn update_error(
    resource: TeletextResourceIdentity,
    previous: u8,
    requested: u8,
    expected: u8,
    transmission: TeletextResourceTransmission,
    expected_transmission: TeletextResourceTransmission,
) -> TeletextTransportError {
    TeletextTransportError::ResourceUpdate {
        resource,
        previous: TeletextResourceUpdateCounter::new(previous).unwrap(),
        requested: TeletextResourceUpdateCounter::new(requested).unwrap(),
        expected: TeletextResourceUpdateCounter::new(expected).unwrap(),
        transmission,
        expected_transmission,
    }
}

#[test]
fn equal_resources_allow_unchanged_same_counter_and_deliberate_changed_next_counter() {
    let original = candidate(spec(7, TeletextResourceFullUpdate::Changed));
    let unchanged = candidate(spec(7, TeletextResourceFullUpdate::Unchanged));
    let refresh = candidate(spec(0, TeletextResourceFullUpdate::Changed));
    let history = ResourceUpdateHistory::default();
    assert_ne!(
        drcs(&original).data_units()[0],
        drcs(&unchanged).data_units()[0]
    );
    assert!(same_content(drcs(&original), drcs(&unchanged)));
    history.validate(&unchanged, Some(&original)).unwrap();
    history.validate(&refresh, Some(&original)).unwrap();
    assert!(history.resources.is_empty());
}

#[test]
fn equal_resources_reject_wrong_counters_without_mutating_history_or_candidates() {
    let original = candidate(spec(7, TeletextResourceFullUpdate::Changed));
    let wrong_unchanged = candidate(spec(0, TeletextResourceFullUpdate::Unchanged));
    let wrong_changed = candidate(spec(7, TeletextResourceFullUpdate::Changed));
    let original_units = drcs(&original).data_units().to_vec();
    let requested_units = drcs(&wrong_changed).data_units().to_vec();
    let history = ResourceUpdateHistory::default();
    assert_eq!(
        history.validate(&wrong_unchanged, Some(&original)),
        Err(update_error(
            identity(TeletextResourceKind::Drcs, 0),
            7,
            0,
            7,
            TeletextResourceTransmission::FullUnchangedFirst,
            TeletextResourceTransmission::FullUnchangedFirst,
        ))
    );
    assert_eq!(
        history.validate(&wrong_changed, Some(&original)),
        Err(update_error(
            identity(TeletextResourceKind::Drcs, 0),
            7,
            7,
            0,
            TeletextResourceTransmission::FullChangedFirst,
            TeletextResourceTransmission::FullChangedFirst,
        ))
    );
    assert_eq!(drcs(&original).data_units(), original_units);
    assert_eq!(drcs(&wrong_changed).data_units(), requested_units);
    assert!(history.resources.is_empty());
}

#[test]
fn changed_drcs_body_requires_changed_next_counter_even_with_identical_nn() {
    let mut old = spec(7, TeletextResourceFullUpdate::Changed);
    old.glyph = Some((0, 1));
    let original = candidate(old);
    let mut changed = spec(7, TeletextResourceFullUpdate::Unchanged);
    changed.glyph = Some((0, 2));
    let rejected = candidate(changed);
    assert_eq!(
        drcs(&original).resource_header().unwrap().last_packet(),
        drcs(&rejected).resource_header().unwrap().last_packet()
    );
    assert!(!same_content(drcs(&original), drcs(&rejected)));
    let history = ResourceUpdateHistory::default();
    assert_eq!(
        history.validate(&rejected, Some(&original)),
        Err(update_error(
            identity(TeletextResourceKind::Drcs, 0),
            7,
            7,
            0,
            TeletextResourceTransmission::FullUnchangedFirst,
            TeletextResourceTransmission::FullChangedFirst,
        ))
    );
    changed.counter = 0;
    changed.update = TeletextResourceFullUpdate::Changed;
    history
        .validate(&candidate(changed), Some(&original))
        .unwrap();
}

#[test]
fn actual_last_packet_and_same_raw_owner_kind_changes_require_new_full_update() {
    let original = candidate(spec(1, TeletextResourceFullUpdate::Changed));
    let mut longer = spec(1, TeletextResourceFullUpdate::Unchanged);
    longer.glyph = Some((47, 1));
    let next = candidate(longer);
    assert_ne!(
        drcs(&original).resource_header().unwrap().last_packet(),
        drcs(&next).resource_header().unwrap().last_packet()
    );
    assert!(matches!(
        ResourceUpdateHistory::default().validate(&next, Some(&original)),
        Err(TeletextTransportError::ResourceUpdate {
            expected_transmission: TeletextResourceTransmission::FullChangedFirst,
            ..
        })
    ));
    let mut different_kind = spec(1, TeletextResourceFullUpdate::Unchanged);
    different_kind.kind = Some(TeletextResourceKind::Gdrcs);
    assert_eq!(
        ResourceUpdateHistory::default().validate(&candidate(different_kind), Some(&original)),
        Err(update_error(
            identity(TeletextResourceKind::Gdrcs, 0),
            1,
            1,
            2,
            TeletextResourceTransmission::FullUnchangedFirst,
            TeletextResourceTransmission::FullChangedFirst,
        ))
    );
    different_kind.counter = 2;
    different_kind.update = TeletextResourceFullUpdate::Changed;
    ResourceUpdateHistory::default()
        .validate(&candidate(different_kind), Some(&original))
        .unwrap();
}

#[test]
fn normalization_ignores_only_s2_c4_and_c8_preserving_other_header_controls() {
    let old = candidate(spec(7, TeletextResourceFullUpdate::Changed));
    let next = candidate(spec(0, TeletextResourceFullUpdate::Changed));
    let previous = drcs(&old);
    let following = drcs(&next);
    let old_header = previous.resource_header().unwrap();
    let next_header = following.resource_header().unwrap();
    assert!(same_content(previous, following));
    // Independent Hamming8/4 literals. Flipping data bit0 in the C7/C8/C9/C10
    // nibble is a legal C7 change, not the ignored C8 bit1.
    let mut controls = following.data_units().to_vec();
    controls[0][12] ^= (0x15_u8 ^ 0x02).reverse_bits();
    assert!(!same_units(
        old_header,
        previous.data_units(),
        next_header,
        &controls
    ));
    controls = following.data_units().to_vec();
    controls[0][13] ^= (0x15_u8 ^ 0x02).reverse_bits();
    assert!(!same_units(
        old_header,
        previous.data_units(),
        next_header,
        &controls
    ));
    let mut row = following.data_units().to_vec();
    row[1][9] ^= 1;
    assert!(!same_units(
        old_header,
        previous.data_units(),
        next_header,
        &row
    ));
}

#[test]
fn pending_replacements_validate_against_last_completed_body_without_advancing_it() {
    let original = candidate(spec(1, TeletextResourceFullUpdate::Changed));
    let resource_pointer = drcs(&original).data_units().as_ptr();
    let mut history = ResourceUpdateHistory::default();
    history.record_completed(original);
    let retained = &history.resources[&owner(identity(TeletextResourceKind::Drcs, 0))];
    assert_eq!(
        retained.page.as_ref().unwrap().data_units().as_ptr(),
        resource_pointer
    );
    assert_eq!(history.resources.len(), 4);
    let pending = candidate(spec(2, TeletextResourceFullUpdate::Changed));
    history.validate(&pending, None).unwrap();
    history.validate(&pending, None).unwrap();
    assert_eq!(
        history.resources[&owner(identity(TeletextResourceKind::Drcs, 0))]
            .counter
            .get(),
        1
    );
    let skipped = candidate(spec(3, TeletextResourceFullUpdate::Changed));
    assert_eq!(
        history.validate(&skipped, None),
        Err(update_error(
            identity(TeletextResourceKind::Drcs, 0),
            1,
            3,
            2,
            TeletextResourceTransmission::FullChangedFirst,
            TeletextResourceTransmission::FullChangedFirst,
        ))
    );
    // Once the actual newer generation completes, it overrides older snapshots.
    history.validate(&skipped, Some(&pending)).unwrap();
}

#[test]
fn removed_owners_keep_counter_tombstones_and_readdition_requires_next_changed_state() {
    let mut history = ResourceUpdateHistory::default();
    history.record_completed(candidate(spec(7, TeletextResourceFullUpdate::Changed)));
    let mut removed = spec(0, TeletextResourceFullUpdate::Unchanged);
    removed.kind = None;
    let removal = candidate(removed);
    let unchanged = candidate(spec(7, TeletextResourceFullUpdate::Unchanged));
    let expected = Err(update_error(
        identity(TeletextResourceKind::Drcs, 0),
        7,
        7,
        0,
        TeletextResourceTransmission::FullUnchangedFirst,
        TeletextResourceTransmission::FullChangedFirst,
    ));
    assert_eq!(history.validate(&unchanged, Some(&removal)), expected);
    history.record_completed(removal);
    assert_eq!(history.resources.len(), 4);
    let tombstone = &history.resources[&owner(identity(TeletextResourceKind::Drcs, 0))];
    assert!(tombstone.page.is_none());
    assert_eq!(tombstone.counter.get(), 7);
    assert_eq!(
        history
            .resources
            .values()
            .filter(|entry| entry.page.is_some())
            .count(),
        3
    );
    assert_eq!(history.validate(&unchanged, None), expected);
    history
        .validate(
            &candidate(spec(0, TeletextResourceFullUpdate::Changed)),
            None,
        )
        .unwrap();
}

#[test]
fn cold_or_distinct_s1_owners_remain_explicit_host_authority_and_m29_is_not_a_resource() {
    let history = ResourceUpdateHistory::default();
    let original = candidate(spec(7, TeletextResourceFullUpdate::Changed));
    history.validate(&original, None).unwrap();
    let mut distinct = spec(0, TeletextResourceFullUpdate::Unchanged);
    distinct.subpage = 1;
    history
        .validate(&candidate(distinct), Some(&original))
        .unwrap();
    let mut changed_magazine = fixtures::input();
    changed_magazine.magazines[0].presentation.palette.clut2[0] =
        TeletextRgb::new([7, 6, 5]).unwrap();
    let declared = fixtures::declared(changed_magazine);
    let mut tx = fixtures::transmissions(&declared);
    for record in &mut tx {
        record.update = TeletextResourceFullUpdate::Unchanged;
        if record.resource.kind() == TeletextResourceKind::Drcs {
            record.counter = TeletextResourceUpdateCounter::new(7).unwrap();
        }
    }
    let next = encode_teletext_authored_service(declared, &tx, &fixtures::limits()).unwrap();
    assert_ne!(original.magazine_units(), next.magazine_units());
    history.validate(&next, Some(&original)).unwrap();
}

#[test]
fn completed_legacy_display_retires_all_matching_s1_bodies_but_preserves_other_owners() {
    let affected = [
        identity_at(TeletextResourceKind::Drcs, 2, 0x20, 0),
        identity_at(TeletextResourceKind::Drcs, 2, 0x20, 1),
    ];
    let unrelated = [
        identity_at(TeletextResourceKind::Drcs, 2, 0xc0, 0),
        identity_at(TeletextResourceKind::Drcs, 3, 0x20, 0),
    ];
    let mut input = fixtures::input();
    input.drcs = affected
        .into_iter()
        .chain(unrelated)
        .map(|resource| TeletextDrcsSubtable::new(resource, vec![]).unwrap())
        .collect();
    let declared = fixtures::declared(input);
    let mut tx = fixtures::transmissions(&declared);
    for record in &mut tx {
        record.update = TeletextResourceFullUpdate::Unchanged;
        if record.resource.kind() == TeletextResourceKind::Drcs {
            record.counter = TeletextResourceUpdateCounter::new(7).unwrap();
        }
    }
    let completed = encode_teletext_authored_service(declared, &tx, &fixtures::limits()).unwrap();
    let mut history = ResourceUpdateHistory::default();
    history.record_completed(completed);
    let unchanged = DrcsSpec {
        page: 0x20,
        ..spec(7, TeletextResourceFullUpdate::Unchanged)
    };
    history.validate(&candidate(unchanged), None).unwrap();
    let legacy =
        crate::teletext::transport_tests::set(vec![crate::teletext::transport_tests::draft(
            220, 7, false,
        )]);
    history.retire_display_owners(&legacy);
    assert_eq!(history.resources.len(), 7);
    for resource in affected {
        let previous = &history.resources[&owner(resource)];
        assert!(previous.page.is_none());
        assert_eq!(previous.counter.get(), 7);
    }
    for resource in unrelated {
        let previous = &history.resources[&owner(resource)];
        assert!(previous.page.is_some());
        assert_eq!(previous.counter.get(), 7);
    }
    assert_eq!(
        history.validate(&candidate(unchanged), None),
        Err(update_error(
            affected[0],
            7,
            7,
            0,
            TeletextResourceTransmission::FullUnchangedFirst,
            TeletextResourceTransmission::FullChangedFirst,
        ))
    );
    let changed = DrcsSpec {
        page: 0x20,
        ..spec(0, TeletextResourceFullUpdate::Changed)
    };
    history.validate(&candidate(changed), None).unwrap();
    history
        .validate(
            &candidate(spec(7, TeletextResourceFullUpdate::Unchanged)),
            None,
        )
        .unwrap();
}
