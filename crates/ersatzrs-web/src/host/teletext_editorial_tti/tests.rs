//! Reconciliation syntax/identity proof; native effects, permissions, real editor
//! interaction, transport and display acceptance remain separate host gates.
use super::*;
use ersatzrs_addon_contract::{
    TeletextNavigationLinkWire, TeletextNavigationRoleWire, TeletextNavigationTargetWire,
    TeletextPageAddressWire, TeletextPageKindWire,
};
use ersatzrs_app::teletext_tti::{write_teletext_tti, TeletextTtiErrorKind};
use std::time::Duration;

fn budget() -> TeletextTtiBudget {
    TeletextTtiBudget {
        max_bytes: 8192,
        max_records: 64,
    }
}
fn deadline() -> Instant {
    Instant::now().checked_add(Duration::from_secs(30)).unwrap()
}
fn mapping() -> TeletextExportPageMapping {
    TeletextExportPageMapping {
        name: "page-200-0010.tti".into(),
        page: TeletextPageAddressWire {
            page: 200,
            subcode: 16,
        },
        producer_ordinal: 0,
        editor_page: 0x200,
        editor_ordinal: 0,
        language: "nld".into(),
        kind: TeletextPageKindWire::FullPage,
        header_controls: 0x101,
        navigation: vec![TeletextNavigationLinkWire {
            role: TeletextNavigationRoleWire::Red,
            target: TeletextNavigationTargetWire {
                page: 201,
                subcode: Some(17),
            },
        }],
        materialized_magazine: Some(2),
        tti_bytes: 0,
        tti_sha256: "a".repeat(64),
    }
}
fn row(number: u8, text: &[u8]) -> TeletextTtiRecord {
    let mut cells = [b' '; 40];
    let start = if number == 0 { 8 } else { 0 };
    cells[start..start + text.len()].copy_from_slice(text);
    TeletextTtiRecord::BaseRow { row: number, cells }
}
fn records() -> Vec<TeletextTtiRecord> {
    let mut words = [0; 13];
    words[0] = (1 << 7) | (15 << 14);
    words[1] = 7;
    vec![
        TeletextTtiRecord::PageNumber {
            page: 0x200,
            editor_ordinal: 0,
        },
        TeletextTtiRecord::SubpageOrdinal(0),
        TeletextTtiRecord::PageStatus(status(mapping().header_controls)),
        TeletextTtiRecord::Region(0),
        TeletextTtiRecord::Cycle {
            value: 20,
            unit: TeletextTtiCycleUnit::Time,
        },
        row(0, b"ORIGINAL HEADER"),
        row(1, b"CUSTOM ZONE"),
        row(2, b""),
        TeletextTtiRecord::FastLinks([0x201, 0x2ff, 0x2ff, 0x2ff, 0x2ff, 0x2ff]),
        TeletextTtiRecord::Packet {
            row: 26,
            designation: 0,
            words: [0; 13],
        },
        TeletextTtiRecord::Packet {
            row: 28,
            designation: 0,
            words,
        },
    ]
}
fn bytes(records: Vec<TeletextTtiRecord>) -> Vec<u8> {
    write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, budget()).unwrap(),
        budget(),
    )
    .unwrap()
}
fn reconcile(edited: Vec<TeletextTtiRecord>) -> Result<Option<TeletextPhysicalBaseOverlay>> {
    reconcile_teletext_editorial_tti(
        &mapping(),
        &bytes(records()),
        &bytes(edited),
        budget(),
        deadline(),
    )
}

#[test]
fn editorial_tti_exact_and_numeric_line_ending_formatting_preserve_original_identity() {
    let baseline = bytes(records());
    assert!(reconcile_teletext_editorial_tti(
        &mapping(),
        &baseline,
        &baseline,
        budget(),
        deadline()
    )
    .unwrap()
    .is_none());
    let edited = String::from_utf8(baseline.clone())
        .unwrap()
        .replace("PS,C200", "PS,c200")
        .replace("\r\n", "\n");
    assert!(reconcile_teletext_editorial_tti(
        &mapping(),
        &baseline,
        edited.as_bytes(),
        budget(),
        deadline()
    )
    .unwrap()
    .is_none());
}

#[test]
fn editorial_tti_editor_save_order_blank_omission_and_zone_change_return_exact_rows() {
    let original = records();
    // QTeletextMaker writes header, X27, X28, X26, base rows, then FL. Its
    // LevelOnePage removes an all-space packet after clearing the last cell.
    let mut saved = original[..6].to_vec();
    saved.extend([
        original[10].clone(),
        original[9].clone(),
        row(1, b"EDITED ZONE"),
        original[8].clone(),
    ]);
    let overlay = reconcile(saved).unwrap().unwrap();
    assert_eq!(overlay.owner().page.get(), 200);
    assert_eq!(overlay.owner().subcode.get(), 16);
    assert_eq!(overlay.rows().len(), 1);
    assert_eq!(overlay.rows()[0].row(), 1);
    assert_eq!(&overlay.rows()[0].cells()[..11], b"EDITED ZONE");
}

#[test]
fn editorial_tti_missing_base_rows_are_spaces_and_clear_content_without_dropping_the_change() {
    let mut saved = records();
    saved.retain(|record| !matches!(record, TeletextTtiRecord::BaseRow { row: 1 | 2, .. }));
    let cleared = reconcile(saved).unwrap().unwrap();
    assert_eq!(cleared.rows().len(), 1);
    assert_eq!(cleared.rows()[0].row(), 1);
    assert_eq!(cleared.rows()[0].cells(), &[b' '; 40]);
    let mut added = records();
    added.push(row(24, b"NEW ZONE"));
    let value = reconcile(added).unwrap().unwrap();
    assert_eq!(value.rows()[0].row(), 24);
}

#[test]
fn editorial_tti_national_characters_mosaic_controls_and_raw_nos_flags_are_preserved() {
    let raw = [1, b'[', 0x11, 0x21, 0x1a, b']', 0x0e, 0x7f];
    let mut saved = records();
    saved[6] = row(1, &raw);
    let original_mapping = mapping();
    let overlay = reconcile_teletext_editorial_tti(
        &original_mapping,
        &bytes(records()),
        &bytes(saved),
        budget(),
        deadline(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(&overlay.rows()[0].cells()[..raw.len()], &raw);
    assert_eq!(original_mapping.header_controls, 0x101);
    assert_eq!(original_mapping.navigation[0].target.subcode, Some(17));
}

#[test]
fn editorial_tti_header_identity_is_reserved_but_header_text_can_change() {
    let mut saved = records();
    saved[5] = row(0, b"EDITED HEADER");
    let overlay = reconcile(saved.clone()).unwrap().unwrap();
    assert_eq!(overlay.rows()[0].row(), 0);
    if let TeletextTtiRecord::BaseRow { cells, .. } = &mut saved[5] {
        cells[7] = b'X';
    }
    let error = reconcile(saved).unwrap_err();
    assert_eq!(
        error.failure,
        NativeTeletextEditorialTtiFailure::HeaderIdentity
    );
    assert_eq!(error.row, Some(0));
    assert_eq!(error.column, Some(7));
    assert_eq!(error.page, 200);
    assert_eq!(error.subcode, 16);
    assert_eq!(error.resource, None);
}

#[test]
fn editorial_tti_page_ordinal_subpage_and_mapping_cannot_be_rebound() {
    for changed in [
        TeletextTtiRecord::PageNumber {
            page: 0x201,
            editor_ordinal: 0,
        },
        TeletextTtiRecord::PageNumber {
            page: 0x200,
            editor_ordinal: 1,
        },
    ] {
        let mut saved = records();
        saved[0] = changed;
        assert_eq!(
            reconcile(saved).unwrap_err().failure,
            NativeTeletextEditorialTtiFailure::Identity
        );
    }
    let mut saved = records();
    saved[1] = TeletextTtiRecord::SubpageOrdinal(16);
    assert_eq!(
        reconcile(saved).unwrap_err().failure,
        NativeTeletextEditorialTtiFailure::Identity
    );
    let mut multiple = records();
    multiple.extend([
        TeletextTtiRecord::PageNumber {
            page: 0x200,
            editor_ordinal: 1,
        },
        TeletextTtiRecord::SubpageOrdinal(1),
    ]);
    assert_eq!(
        reconcile(multiple).unwrap_err().failure,
        NativeTeletextEditorialTtiFailure::Identity
    );
    for field in [
        "page",
        "subcode",
        "editor_page",
        "editor_ordinal",
        "controls",
    ] {
        let mut map = mapping();
        match field {
            "page" => map.page.page = 99,
            "subcode" => map.page.subcode = 0x80,
            "editor_page" => map.editor_page = 200,
            "editor_ordinal" => map.editor_ordinal = 1,
            _ => map.header_controls = 0x800,
        }
        assert_eq!(
            reconcile_teletext_editorial_tti(
                &map,
                &bytes(records()),
                &bytes(records()),
                budget(),
                deadline()
            )
            .unwrap_err()
            .failure,
            NativeTeletextEditorialTtiFailure::Mapping
        );
    }
}

#[test]
fn editorial_tti_status_region_links_description_and_cycle_changes_are_explicitly_refused() {
    for (index, value, failure) in [
        (
            2,
            TeletextTtiRecord::PageStatus(0xC280),
            NativeTeletextEditorialTtiFailure::Status,
        ),
        (
            3,
            TeletextTtiRecord::Region(1),
            NativeTeletextEditorialTtiFailure::Region,
        ),
        (
            8,
            TeletextTtiRecord::FastLinks([0x202, 0x2ff, 0x2ff, 0x2ff, 0x2ff, 0x2ff]),
            NativeTeletextEditorialTtiFailure::Navigation,
        ),
        (
            4,
            TeletextTtiRecord::Cycle {
                value: 21,
                unit: TeletextTtiCycleUnit::Time,
            },
            NativeTeletextEditorialTtiFailure::Cycle,
        ),
    ] {
        let mut saved = records();
        saved[index] = value;
        assert_eq!(reconcile(saved).unwrap_err().failure, failure);
        let mut missing = records();
        missing.remove(index);
        assert_eq!(reconcile(missing).unwrap_err().failure, failure);
    }
    let mut description = records();
    description.insert(
        0,
        TeletextTtiRecord::Description("edited description sentinel".into()),
    );
    let error = reconcile(description).unwrap_err();
    assert_eq!(
        error.failure,
        NativeTeletextEditorialTtiFailure::Description
    );
    assert!(!format!("{error:?}").contains("sentinel"));
}

#[test]
fn editorial_tti_original_description_and_protected_row25_are_retained_exactly() {
    let mut original = records();
    original.insert(
        0,
        TeletextTtiRecord::Description("original description sentinel".into()),
    );
    original.push(row(25, b"PROTECTED ROW"));
    let mut saved = original.clone();
    saved[7] = row(1, b"EDITED ZONE");
    let changed = reconcile_teletext_editorial_tti(
        &mapping(),
        &bytes(original.clone()),
        &bytes(saved.clone()),
        budget(),
        deadline(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(changed.rows().len(), 1);
    assert_eq!(changed.rows()[0].row(), 1);
    saved[0] = TeletextTtiRecord::Description("changed description sentinel".into());
    assert_eq!(
        reconcile_teletext_editorial_tti(
            &mapping(),
            &bytes(original.clone()),
            &bytes(saved),
            budget(),
            deadline()
        )
        .unwrap_err()
        .failure,
        NativeTeletextEditorialTtiFailure::Description
    );
    let mut lost_row = original.clone();
    lost_row.pop();
    let error = reconcile_teletext_editorial_tti(
        &mapping(),
        &bytes(original),
        &bytes(lost_row),
        budget(),
        deadline(),
    )
    .unwrap_err();
    assert_eq!(
        error.failure,
        NativeTeletextEditorialTtiFailure::ProtectedRow
    );
    assert_eq!(error.row, Some(25));
}

#[test]
fn editorial_tti_enhancement_keys_survive_reordering_but_changes_deletions_and_insertions_fail() {
    for index in [9, 10] {
        let mut changed = records();
        if let TeletextTtiRecord::Packet { words, .. } = &mut changed[index] {
            words[12] ^= 1;
        }
        let error = reconcile(changed).unwrap_err();
        assert_eq!(error.failure, NativeTeletextEditorialTtiFailure::Packet);
        assert_eq!(error.row, Some(if index == 9 { 26 } else { 28 }));
        assert_eq!(error.designation, Some(0));
        let mut missing = records();
        missing.remove(index);
        assert_eq!(
            reconcile(missing).unwrap_err().failure,
            NativeTeletextEditorialTtiFailure::Packet
        );
    }
    let mut added = records();
    added.push(TeletextTtiRecord::Packet {
        row: 27,
        designation: 4,
        words: [0; 13],
    });
    assert_eq!(
        reconcile(added).unwrap_err().failure,
        NativeTeletextEditorialTtiFailure::Packet
    );
    let mut extra_row = records();
    extra_row.push(row(25, b"UNSUPPORTED EDIT"));
    assert_eq!(
        reconcile(extra_row).unwrap_err().failure,
        NativeTeletextEditorialTtiFailure::ProtectedRow
    );
}

#[test]
fn editorial_tti_nos_is_exact_even_with_unchanged_materialized_x28() {
    // These are not accepted as an approximate NOS mask. A real editor proof
    // would be required before allowing a specific normalization derivation.
    for bit in [0x80, 0x100, 0x200, 0x4000, 0x8000] {
        let mut saved = records();
        saved[2] = TeletextTtiRecord::PageStatus(status(mapping().header_controls) ^ bit);
        assert_eq!(
            reconcile(saved).unwrap_err().failure,
            NativeTeletextEditorialTtiFailure::Status
        );
    }
    let mut invalid_baseline = records();
    invalid_baseline[2] = TeletextTtiRecord::PageStatus(0x8000);
    assert_eq!(
        reconcile_teletext_editorial_tti(
            &mapping(),
            &bytes(invalid_baseline.clone()),
            &bytes(invalid_baseline),
            budget(),
            deadline()
        )
        .unwrap_err()
        .failure,
        NativeTeletextEditorialTtiFailure::Mapping
    );
}

#[test]
fn editorial_tti_assets_unknown_commands_and_parse_locations_are_never_treated_as_editable_rows() {
    let baseline = bytes(records());
    for edited in [
        b"PN,20000\nPF,3,2\n".as_slice(),
        b"PN,2ff00\nPS,8000\n".as_slice(),
        &[0x10, 0x02, 0xe4, 0xff],
    ] {
        let error =
            reconcile_teletext_editorial_tti(&mapping(), &baseline, edited, budget(), deadline())
                .unwrap_err();
        assert!(matches!(
            error.failure,
            NativeTeletextEditorialTtiFailure::Syntax | NativeTeletextEditorialTtiFailure::Identity
        ));
    }
    let mut malformed = baseline;
    malformed.extend_from_slice(b"OL,3,short\r\n");
    let error = reconcile_teletext_editorial_tti(
        &mapping(),
        &bytes(records()),
        &malformed,
        budget(),
        deadline(),
    )
    .unwrap_err();
    assert_eq!(error.failure, NativeTeletextEditorialTtiFailure::Syntax);
    assert_eq!(error.row, Some(3));
    assert_eq!(
        error.cause.unwrap().kind,
        TeletextTtiErrorKind::InvalidRowWidth
    );
}

#[test]
fn editorial_tti_byte_record_budgets_and_expired_deadline_reject_without_partial_overlay() {
    let baseline = bytes(records());
    for b in [
        TeletextTtiBudget {
            max_bytes: baseline.len() - 1,
            max_records: 64,
        },
        TeletextTtiBudget {
            max_bytes: 8192,
            max_records: 1,
        },
        TeletextTtiBudget {
            max_bytes: 0,
            max_records: 64,
        },
    ] {
        let error =
            reconcile_teletext_editorial_tti(&mapping(), &baseline, &baseline, b, deadline())
                .unwrap_err();
        assert_eq!(error.failure, NativeTeletextEditorialTtiFailure::Syntax);
        assert!(error.cause.is_some());
    }
    let expired = Instant::now().checked_sub(Duration::from_secs(1)).unwrap();
    let error =
        reconcile_teletext_editorial_tti(&mapping(), &baseline, b"malformed", budget(), expired)
            .unwrap_err();
    assert_eq!(error.failure, NativeTeletextEditorialTtiFailure::TimedOut);
    assert!(error.cause.is_none());
}

#[test]
fn editorial_tti_v2_carrier_status_preserves_independent_original_header_nos() {
    let mut map = mapping();
    // Original header NOS 5 remains distinct from unchanged X28 primary NOS 1.
    // The trusted V2 carrier status projects primary 1, not header controls 5.
    map.header_controls = 0x501;
    assert_eq!(status(map.header_controls), 0xC280);
    let baseline = bytes(records());
    let mut saved = records();
    saved[6] = row(1, b"EDITED ZONE");
    let edited = bytes(saved);
    let overlay = reconcile_teletext_editorial_tti_with_status(
        &map,
        &baseline,
        &edited,
        budget(),
        deadline(),
        0xC200,
    )
    .unwrap()
    .unwrap();
    assert_eq!(overlay.owner().subcode.get(), 16);
    assert_eq!(overlay.rows().len(), 1);
    assert_eq!(&overlay.rows()[0].cells()[..11], b"EDITED ZONE");
    assert_eq!(map.header_controls, 0x501);
    // The legacy entry still enforces the original header-derived status.
    assert_eq!(
        reconcile_teletext_editorial_tti(&map, &baseline, &edited, budget(), deadline())
            .unwrap_err()
            .failure,
        NativeTeletextEditorialTtiFailure::Mapping
    );
}

#[test]
fn editorial_tti_trusted_status_rejects_baseline_mismatch_and_every_edited_flag_change() {
    let baseline = bytes(records());
    assert_eq!(
        reconcile_teletext_editorial_tti_with_status(
            &mapping(),
            &baseline,
            &baseline,
            budget(),
            deadline(),
            0xC080,
        )
        .unwrap_err()
        .failure,
        NativeTeletextEditorialTtiFailure::Mapping
    );
    for bit in [0x80, 0x100, 0x200, 0x4000, 0x8000] {
        let mut saved = records();
        saved[2] = TeletextTtiRecord::PageStatus(0xC200 ^ bit);
        assert_eq!(
            reconcile_teletext_editorial_tti_with_status(
                &mapping(),
                &baseline,
                &bytes(saved),
                budget(),
                deadline(),
                0xC200,
            )
            .unwrap_err()
            .failure,
            NativeTeletextEditorialTtiFailure::Status
        );
    }
}

#[test]
fn editorial_tti_v2_primary_four_does_not_relax_enhancement_or_region_protection() {
    let mut original = records();
    original[2] = TeletextTtiRecord::PageStatus(0xC080);
    if let TeletextTtiRecord::Packet { words, .. } = &mut original[10] {
        words[0] = (4 << 7) | (15 << 14);
    }
    let baseline = bytes(original.clone());
    let mut saved = original.clone();
    saved[6] = row(1, b"ZONE FOUR");
    assert!(reconcile_teletext_editorial_tti_with_status(
        &mapping(),
        &baseline,
        &bytes(saved),
        budget(),
        deadline(),
        0xC080,
    )
    .unwrap()
    .is_some());
    for (index, record, failure) in [
        (
            10,
            records()[10].clone(),
            NativeTeletextEditorialTtiFailure::Packet,
        ),
        (
            3,
            TeletextTtiRecord::Region(1),
            NativeTeletextEditorialTtiFailure::Region,
        ),
    ] {
        let mut changed = original.clone();
        changed[index] = record;
        let error = reconcile_teletext_editorial_tti_with_status(
            &mapping(),
            &baseline,
            &bytes(changed),
            budget(),
            deadline(),
            0xC080,
        )
        .unwrap_err();
        assert_eq!(error.failure, failure);
        if failure == NativeTeletextEditorialTtiFailure::Packet {
            assert_eq!(error.row, Some(28));
            assert_eq!(error.designation, Some(0));
        }
    }
}

#[test]
fn editorial_tti_explicit_legacy_status_is_identical_to_public_legacy_reconciliation() {
    let baseline = bytes(records());
    let mut saved = records();
    saved[6] = row(1, b"LEGACY ZONE");
    let edited = bytes(saved);
    let legacy =
        reconcile_teletext_editorial_tti(&mapping(), &baseline, &edited, budget(), deadline())
            .unwrap();
    let explicit = reconcile_teletext_editorial_tti_with_status(
        &mapping(),
        &baseline,
        &edited,
        budget(),
        deadline(),
        0xC200,
    )
    .unwrap();
    assert_eq!(legacy, explicit);
}
