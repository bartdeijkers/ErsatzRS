//! Strict native resource inverse; editor/client acceptance remains separate.
use super::*;
use crate::teletext::{encode_teletext_drcs_page, TeletextResourceFullUpdate};
use ersatzrs_domain::{
    TeletextDrcsBitmap, TeletextDrcsGlyphDefinition, TeletextDrcsGlyphSlot, TeletextDrcsSubtable,
    TeletextRawDataPageNumber, TeletextResourceIdentity, TeletextResourceSubpageIndex,
    TeletextResourceUpdateCounter,
};

fn resource(kind: TeletextResourceKind, slot: u8) -> EncodedTeletextResourcePage {
    // Same asymmetric scanlines as the independent DRCS body fixture.
    let bitmap = [0, 0x800, 1, 0xaaa, 0x555, 0x3f, 0xfc0, 0xfff, 0x801, 0x123];
    let table = TeletextDrcsSubtable::new(
        TeletextResourceIdentity::new(
            kind,
            TeletextRawDataPageNumber::new(8, 0xa0).unwrap(),
            TeletextResourceSubpageIndex::new(15).unwrap(),
        )
        .unwrap(),
        vec![TeletextDrcsGlyphDefinition {
            slot: TeletextDrcsGlyphSlot::new(slot).unwrap(),
            bitmap: TeletextDrcsBitmap::new(bitmap).unwrap(),
        }],
    )
    .unwrap();
    encode_teletext_drcs_page(
        &table,
        TeletextResourceUpdateCounter::new(7).unwrap(),
        TeletextResourceFullUpdate::Changed,
        TeletextResourceTransmissionMode::Serial,
    )
    .unwrap()
}

fn project(
    page: &EncodedTeletextResourcePage,
    units: &[[u8; 46]],
) -> Result<TeletextEncodedDrcsProjection, TeletextEncodedDisplayProjectionError> {
    project_drcs_units(page, units, 26)
}

#[test]
fn drcs_inverse_keeps_exact_resource_header_sparse_modes_and_asymmetric_rows() {
    for kind in [TeletextResourceKind::Drcs, TeletextResourceKind::Gdrcs] {
        let page = resource(kind, 0);
        let result =
            project_encoded_teletext_drcs(&EncodedTeletextAuthoredPage::Resource(page.clone()), 3)
                .unwrap();
        assert_eq!(result.resource_header, page.header());
        assert_eq!(result.header.wire_page, 0xa0);
        assert_eq!(result.header.packet_address.wire_magazine, 0);
        assert_eq!(result.header.subcode, 0x017f);
        assert_eq!(
            result.resource_header.identity().logical_subpage().get(),
            15
        );
        assert_eq!(result.resource_header.update_counter().get(), 7);
        assert_eq!(
            result.transmission_mode,
            TeletextResourceTransmissionMode::Serial
        );
        assert!(result.header.controls.erase_page());
        assert_eq!(
            result.records[0],
            TeletextEncodedDisplayRecord::HeaderText([b' '; 32])
        );
        let TeletextEncodedDisplayRecord::Triplets {
            row,
            designation,
            words,
        } = result.records[1]
        else {
            panic!("expected exact modes record");
        };
        assert_eq!((row, designation), (28, 3));
        assert_eq!(words[0], u32::from(kind.function_code()));
        assert_eq!(words[1], 0x3_fff0);
        assert_eq!(&words[2..11], &[0x3_ffff; 9]);
        assert_eq!(&words[11..], &[0xfff, 0]);
        let mut expected = [b' '; 40];
        expected[..20].copy_from_slice(&[
            0x40, 0x40, 0x60, 0x40, 0x40, 0x41, 0x6a, 0x6a, 0x55, 0x55, 0x40, 0x7f, 0x7f, 0x40,
            0x7f, 0x7f, 0x60, 0x41, 0x44, 0x63,
        ]);
        assert_eq!(
            result.records[2],
            TeletextEncodedDisplayRecord::BaseRow {
                row: 1,
                cells: expected
            }
        );
        assert_eq!(result.records.len(), 3);
        assert!(!format!("{result:?}").contains("words:"));
        assert!(!format!("{result:?}").contains("cells:"));
    }
}

#[test]
fn drcs_inverse_keeps_high_slot_and_native_last_packet() {
    let page = resource(TeletextResourceKind::Drcs, 47);
    let result = project(&page, page.data_units()).unwrap();
    assert_eq!(result.header.subcode, 0x187f);
    assert_eq!(result.resource_header.last_packet().get(), 24);
    let TeletextEncodedDisplayRecord::BaseRow { row, cells } = result.records[2] else {
        panic!("expected high PTU row");
    };
    assert_eq!(row, 24);
    assert_eq!(&cells[..20], &[b' '; 20]);
    assert_eq!(&cells[20..24], &[0x40, 0x40, 0x60, 0x40]);
}

#[test]
fn drcs_inverse_rejects_invalid_bounds_and_absent_units_before_projection() {
    let page = resource(TeletextResourceKind::Drcs, 0);
    for (units, maximum) in [(page.data_units(), 0), (page.data_units(), 2), (&[][..], 3)] {
        assert_eq!(
            project_drcs_units(&page, units, maximum)
                .unwrap_err()
                .failure,
            TeletextEncodedDisplayProjectionFailure::Budget
        );
    }
}

#[test]
fn drcs_inverse_rejects_correctable_address_header_designation_and_triplet_damage() {
    let page = resource(TeletextResourceKind::Drcs, 0);
    for (unit, byte) in [(0, 4), (0, 6), (1, 4), (1, 6), (1, 7)] {
        let mut units = page.data_units().to_vec();
        units[unit][byte] ^= 1;
        let failure = project(&page, &units).unwrap_err();
        assert_eq!(
            failure.failure,
            TeletextEncodedDisplayProjectionFailure::Protection,
            "unit {unit}, byte {byte}"
        );
        assert_eq!(failure.unit, Some(unit));
    }
}

#[test]
fn drcs_inverse_rejects_parity_damage_with_exact_position() {
    let page = resource(TeletextResourceKind::Drcs, 0);
    for (unit, byte, row, column) in [(0, 14, 0, 8), (2, 6, 1, 0), (2, 45, 1, 39)] {
        let mut units = page.data_units().to_vec();
        units[unit][byte] ^= 1;
        let failure = project(&page, &units).unwrap_err();
        assert_eq!(
            failure.failure,
            TeletextEncodedDisplayProjectionFailure::Parity
        );
        assert_eq!(
            (failure.unit, failure.row, failure.column),
            (Some(unit), Some(row), Some(column))
        );
    }
}

#[test]
fn drcs_inverse_rejects_clean_identity_changes_and_noncanonical_order() {
    use crate::teletext::packets::data_unit;
    let page = resource(TeletextResourceKind::Drcs, 0);
    let mut units = page.data_units().to_vec();
    units[0][6] = hamming_8_4(1).reverse_bits();
    assert_eq!(
        project(&page, &units).unwrap_err().failure,
        TeletextEncodedDisplayProjectionFailure::Identity
    );
    let mut units = page.data_units().to_vec();
    units[2][0] = 3;
    assert_eq!(
        project(&page, &units).unwrap_err().failure,
        TeletextEncodedDisplayProjectionFailure::Identity
    );
    let mut units = page.data_units().to_vec();
    units[2][4] = hamming_8_4(1 | (1 << 3)).reverse_bits();
    assert_eq!(
        project(&page, &units).unwrap_err().failure,
        TeletextEncodedDisplayProjectionFailure::Identity
    );
    let mut units = page.data_units().to_vec();
    units.swap(1, 2);
    assert_eq!(
        project(&page, &units).unwrap_err().failure,
        TeletextEncodedDisplayProjectionFailure::UnsupportedPacket
    );
    let mut units = page.data_units().to_vec();
    units[2] = units[0];
    assert_eq!(
        project(&page, &units).unwrap_err().failure,
        TeletextEncodedDisplayProjectionFailure::HeaderOrder
    );
    let body = std::array::from_fn(|column| page.data_units()[2][column + 6].reverse_bits());
    for row in [25, 26, 27, 29] {
        let mut units = page.data_units().to_vec();
        units[2] = data_unit(0, row, &body, false);
        assert_eq!(
            project(&page, &units).unwrap_err().failure,
            TeletextEncodedDisplayProjectionFailure::UnsupportedPacket
        );
    }
    let mut units = page.data_units().to_vec();
    units.push(units[2]);
    assert_eq!(
        project(&page, &units).unwrap_err().failure,
        TeletextEncodedDisplayProjectionFailure::UnsupportedPacket
    );
}

#[test]
fn drcs_inverse_rejects_clean_modes_header_and_final_row_mismatches() {
    use crate::teletext::packets::data_unit;
    let page = resource(TeletextResourceKind::Drcs, 0);
    for (byte, nibble) in [(8, 0), (9, 8), (12, 1), (13, 0)] {
        let mut units = page.data_units().to_vec();
        units[0][byte] = hamming_8_4(nibble).reverse_bits();
        assert_eq!(
            project(&page, &units).unwrap_err().failure,
            TeletextEncodedDisplayProjectionFailure::Identity
        );
    }
    let mut units = page.data_units().to_vec();
    units[1][6] = hamming_8_4(0).reverse_bits();
    assert_eq!(
        project(&page, &units).unwrap_err().failure,
        TeletextEncodedDisplayProjectionFailure::UnsupportedPacket
    );
    let mut units = page.data_units().to_vec();
    for (byte, value) in units[1][7..10].iter_mut().zip(hamming_24_18(4)) {
        *byte = value.reverse_bits();
    }
    assert_eq!(
        project(&page, &units).unwrap_err().failure,
        TeletextEncodedDisplayProjectionFailure::Identity
    );
    let body = std::array::from_fn(|column| page.data_units()[2][column + 6].reverse_bits());
    let mut units = page.data_units().to_vec();
    units[2] = data_unit(0, 2, &body, false);
    assert_eq!(
        project(&page, &units).unwrap_err().failure,
        TeletextEncodedDisplayProjectionFailure::Identity
    );
    let mut units = page.data_units().to_vec();
    units[2][2] = 0xe7;
    assert_eq!(
        project(&page, &units).unwrap_err().failure,
        TeletextEncodedDisplayProjectionFailure::Envelope
    );
}
