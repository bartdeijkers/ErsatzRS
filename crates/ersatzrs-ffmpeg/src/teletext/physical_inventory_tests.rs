//! Actual synthesized headers and independent exact-address collision fixtures.

use ersatzrs_domain::*;

use super::physical_inventory::{
    validate_teletext_physical_page_inventory, TeletextPhysicalPageInventoryError as Error,
    TeletextPhysicalPageInventoryRecord as Record,
};
use super::source_header::{observe_dvb_teletext_header, TeletextHeaderObservation};
use super::*;

fn identity(
    kind: TeletextResourceKind,
    magazine: u8,
    page: u8,
    s1: u8,
) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        kind,
        TeletextRawDataPageNumber::new(magazine, page).unwrap(),
        TeletextResourceSubpageIndex::new(s1).unwrap(),
    )
    .unwrap()
}

fn display(page: u16, subcode: u16) -> Record {
    Record::Display(TeletextPageAddress {
        page: TeletextPageNumber::new(page).unwrap(),
        subcode: TeletextSubcode::new(subcode).unwrap(),
    })
}

fn check_header(unit: &[u8; 46], header: TeletextResourceHeaderMetadata) {
    let TeletextHeaderObservation::Header(observed) = observe_dvb_teletext_header(unit).unwrap()
    else {
        panic!("expected compiled resource header")
    };
    assert_eq!(
        observed.packet_address.wire_magazine,
        header.identity().page().wire_magazine()
    );
    assert_eq!(observed.wire_page, header.identity().page().page());
    assert_eq!(observed.subcode, header.physical_subcode());
}

fn drcs(
    owner: TeletextResourceIdentity,
    counter: u8,
    slot: Option<u8>,
) -> TeletextResourceHeaderMetadata {
    let glyphs = slot
        .into_iter()
        .map(|slot| TeletextDrcsGlyphDefinition {
            slot: TeletextDrcsGlyphSlot::new(slot).unwrap(),
            bitmap: TeletextDrcsBitmap::new([0; 10]).unwrap(),
        })
        .collect();
    let table = TeletextDrcsSubtable::new(owner, glyphs).unwrap();
    let page = encode_teletext_drcs_page(
        &table,
        TeletextResourceUpdateCounter::new(counter).unwrap(),
        TeletextResourceFullUpdate::Changed,
        TeletextResourceTransmissionMode::Serial,
    )
    .unwrap();
    check_header(&page.data_units()[0], page.header());
    page.header()
}

fn pop(owner: TeletextResourceIdentity) -> TeletextResourceHeaderMetadata {
    let catalog = TeletextEnhancementObjectCatalog::new(
        vec![],
        TeletextObjectCatalogBudget {
            max_objects: 0,
            max_commands: 0,
        },
    )
    .unwrap();
    let allocations = allocate_teletext_resource_objects(&catalog, &[], 0).unwrap();
    let page = encode_teletext_object_page(
        owner,
        &catalog,
        &allocations,
        TeletextObjectPageTransmission {
            counter: TeletextResourceUpdateCounter::new(0).unwrap(),
            update: TeletextResourceFullUpdate::Changed,
            mode: TeletextResourceTransmissionMode::Serial,
            last_subpage: true,
        },
        0,
    )
    .unwrap();
    check_header(&page.data_units()[0], page.header());
    page.header()
}

fn mot(s1: u8, counter: u8) -> TeletextResourceHeaderMetadata {
    let target = TeletextX27LinkTarget::Unspecified {
        magazine: TeletextX27Magazine::new(2).unwrap(),
    };
    let object = TeletextMotObjectLink::new(
        target,
        TeletextMotSubpageExtent::new(0).unwrap(),
        false,
        TeletextMotFallback::new(0).unwrap(),
        [TeletextMotDefaultObject::NoObject { raw_number: 0 }; 2],
    );
    let drcs = TeletextMotDrcsLink::new(target, TeletextMotSubpageExtent::new(0).unwrap(), false);
    let model = TeletextMotLevel25Declaration::new(
        identity(TeletextResourceKind::Mot, 2, 0xfe, s1),
        vec![],
        TeletextMotLevel25Links::new(
            object,
            [object; 7],
            drcs,
            [drcs; 7],
            TeletextMotEnhancementCounts::new(0, 0, 0),
        )
        .unwrap(),
    )
    .unwrap();
    let page = encode_teletext_mot_page(
        &model,
        &[[None; 2]; 8],
        TeletextResourceUpdateCounter::new(counter).unwrap(),
        TeletextResourceFullUpdate::Changed,
        TeletextResourceTransmissionMode::Serial,
    )
    .unwrap();
    check_header(&page.data_units()[0], page.header());
    page.header()
}

fn physical(first: usize, second: usize, magazine: u8, page: u8, subcode: u16) -> Error {
    Error::PhysicalCollision {
        first,
        second,
        magazine,
        page,
        subcode,
    }
}

#[test]
fn actual_empty_drcs_header_collides_with_display_200_raw_subcode_0100() {
    let header = drcs(identity(TeletextResourceKind::Drcs, 2, 0, 0), 0, None);
    assert_eq!(header.physical_subcode(), 0x0100);
    for records in [
        [display(200, 0x0100), Record::Resource(header)],
        [Record::Resource(header), display(200, 0x0100)],
    ] {
        let before = records;
        assert_eq!(
            validate_teletext_physical_page_inventory(&records, 2),
            Err(physical(0, 1, 2, 0, 0x0100))
        );
        assert_eq!(records, before);
    }
}

#[test]
fn actual_empty_pop_and_gpop_collide_with_display_200_raw_subcode_0300() {
    for kind in [TeletextResourceKind::Pop, TeletextResourceKind::Gpop] {
        let header = pop(identity(kind, 2, 0, 0));
        assert_eq!(header.last_packet().get(), 3);
        assert_eq!(header.physical_subcode(), 0x0300);
        assert_eq!(
            validate_teletext_physical_page_inventory(
                &[display(200, 0x0300), Record::Resource(header)],
                2
            ),
            Err(physical(0, 1, 2, 0, 0x0300))
        );
    }
}

#[test]
fn actual_last_row_and_counter_bits_require_exact_display_subcode_comparison() {
    for (counter, slot, expected) in [(0, None, 0x0100), (7, None, 0x0170), (3, Some(46), 0x1830)] {
        let header = drcs(identity(TeletextResourceKind::Drcs, 2, 0, 0), counter, slot);
        assert_eq!(header.physical_subcode(), expected);
        assert_eq!(
            validate_teletext_physical_page_inventory(
                &[display(200, expected), Record::Resource(header)],
                2
            ),
            Err(physical(0, 1, 2, 0, expected))
        );
        // Same low S1 nibble alone is not a physical display collision.
        assert!(validate_teletext_physical_page_inventory(
            &[display(200, 0), Record::Resource(header)],
            2
        )
        .is_ok());
    }
}

#[test]
fn cross_kind_same_logical_owner_is_rejected_even_when_actual_headers_differ() {
    let object = pop(identity(TeletextResourceKind::Pop, 2, 0xa0, 3));
    let glyphs = drcs(identity(TeletextResourceKind::Gdrcs, 2, 0xa0, 3), 7, None);
    assert_eq!(object.physical_subcode(), 0x0303);
    assert_eq!(glyphs.physical_subcode(), 0x0173);
    assert_eq!(
        validate_teletext_physical_page_inventory(
            &[Record::Resource(object), Record::Resource(glyphs)],
            2
        ),
        Err(Error::ResourceOwnerCollision {
            first: 0,
            second: 1,
            page: object.identity().page(),
            subpage: TeletextResourceSubpageIndex::new(3).unwrap(),
        })
    );
}

#[test]
fn different_resource_kinds_at_different_s1_remain_legal() {
    let records = [
        Record::Resource(pop(identity(TeletextResourceKind::Pop, 2, 0xa0, 0))),
        Record::Resource(drcs(
            identity(TeletextResourceKind::Drcs, 2, 0xa0, 1),
            0,
            None,
        )),
        Record::Resource(pop(identity(TeletextResourceKind::Gpop, 2, 0xa0, 2))),
        Record::Resource(drcs(
            identity(TeletextResourceKind::Gdrcs, 2, 0xa0, 15),
            0,
            None,
        )),
    ];
    let before = records;
    assert!(validate_teletext_physical_page_inventory(&records, 4).is_ok());
    assert_eq!(records, before);
}

#[test]
fn resource_counter_and_last_row_changes_cannot_hide_repeated_logical_owner() {
    let owner = identity(TeletextResourceKind::Drcs, 8, 0xa0, 15);
    let first = drcs(owner, 0, None);
    for second in [drcs(owner, 7, None), drcs(owner, 0, Some(46))] {
        assert_ne!(first.physical_subcode(), second.physical_subcode());
        assert_eq!(
            validate_teletext_physical_page_inventory(
                &[Record::Resource(first), Record::Resource(second)],
                2
            ),
            Err(Error::ResourceOwnerCollision {
                first: 0,
                second: 1,
                page: owner.page(),
                subpage: owner.logical_subpage(),
            })
        );
    }
}

#[test]
fn magazine_eight_wire_zero_is_compared_as_logical_eight_and_bcd_is_preserved() {
    let header = drcs(identity(TeletextResourceKind::Drcs, 8, 0x99, 0), 0, None);
    assert_eq!(header.identity().page().wire_magazine(), 0);
    assert_eq!(
        validate_teletext_physical_page_inventory(
            &[display(899, 0x0100), Record::Resource(header)],
            2
        ),
        Err(physical(0, 1, 8, 0x99, 0x0100))
    );
    assert!(validate_teletext_physical_page_inventory(
        &[display(199, 0x0100), Record::Resource(header)],
        2
    )
    .is_ok());
    // Decimal 234 has BCD page byte34, not binary22.
    let bcd = drcs(identity(TeletextResourceKind::Drcs, 2, 0x34, 0), 0, None);
    assert_eq!(
        validate_teletext_physical_page_inventory(
            &[display(234, 0x0100), Record::Resource(bcd)],
            2
        ),
        Err(physical(0, 1, 2, 0x34, 0x0100))
    );
}

#[test]
fn actual_mot_physical_headers_preserve_nn_counter_and_nonzero_logical_s1() {
    let zero = mot(0, 0);
    let nonzero = mot(7, 5);
    assert_eq!(zero.physical_subcode(), 0x1500);
    assert_eq!(nonzero.physical_subcode(), 0x1557);
    let records = [
        Record::Resource(zero),
        Record::Resource(nonzero),
        display(200, 0x1500),
    ];
    assert!(validate_teletext_physical_page_inventory(&records, 3).is_ok());
    let changed_counter = mot(7, 6);
    assert_eq!(
        validate_teletext_physical_page_inventory(
            &[Record::Resource(nonzero), Record::Resource(changed_counter)],
            2
        ),
        Err(Error::ResourceOwnerCollision {
            first: 0,
            second: 1,
            page: nonzero.identity().page(),
            subpage: TeletextResourceSubpageIndex::new(7).unwrap(),
        })
    );
}

#[test]
fn exact_display_raw_subcodes_and_first_record_indices_are_retained() {
    let mut records = vec![
        display(200, 0),
        display(200, 0x007f),
        display(200, 0x0100),
        display(200, 0x3f7f),
    ];
    assert!(validate_teletext_physical_page_inventory(&records, 4).is_ok());
    records.push(display(200, 0x007f));
    assert_eq!(
        validate_teletext_physical_page_inventory(&records, 5),
        Err(physical(1, 4, 2, 0, 0x007f))
    );
}

#[test]
fn lifecycle_and_kind_do_not_distinguish_equal_physical_header_identity() {
    let first = drcs(identity(TeletextResourceKind::Drcs, 2, 0xa0, 0), 0, None);
    let second = TeletextResourceHeaderMetadata::new(
        identity(TeletextResourceKind::Gdrcs, 2, 0xa0, 0),
        first.last_packet(),
        first.update_counter(),
        TeletextResourceTransmission::FullUnchangedContinuation,
    )
    .unwrap();
    assert_eq!(first.physical_subcode(), second.physical_subcode());
    assert_eq!(
        validate_teletext_physical_page_inventory(
            &[Record::Resource(first), Record::Resource(second)],
            2
        ),
        Err(physical(0, 1, 2, 0xa0, 0x0100))
    );
}

#[test]
fn raw_count_budget_precedes_collision_indexing_and_zero_budget_allows_empty_input() {
    let records = [display(200, 0), display(200, 0)];
    let before = records;
    assert_eq!(
        validate_teletext_physical_page_inventory(&records, 1),
        Err(Error::Budget {
            count: 2,
            maximum: 1
        })
    );
    assert_eq!(records, before);
    assert_eq!(
        validate_teletext_physical_page_inventory(&records, 2),
        Err(physical(0, 1, 2, 0, 0))
    );
    assert!(validate_teletext_physical_page_inventory(&[], 0).is_ok());
    assert_eq!(
        validate_teletext_physical_page_inventory(&records[..1], 0),
        Err(Error::Budget {
            count: 1,
            maximum: 0
        })
    );
}
