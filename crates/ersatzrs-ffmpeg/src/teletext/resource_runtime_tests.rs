//! Opt-in independent native libzvbi calibration, not client/transport acceptance.
//! The ctypes oracle is calibrated only on Linux x64; other native targets skip
//! this diagnostic. Platform/runtime acceptance remains a separate Phase 9 gate.

use ersatzrs_domain::*;

use super::packets::{data_unit, hamming_24_18, terminator};
use super::*;

fn resource(kind: TeletextResourceKind, s1: u8) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        kind,
        TeletextRawDataPageNumber::new(8, 0xa0).unwrap(),
        TeletextResourceSubpageIndex::new(s1).unwrap(),
    )
    .unwrap()
}

fn unused() -> TeletextX27LinkTarget {
    TeletextX27LinkTarget::Unspecified {
        magazine: TeletextX27Magazine::new(8).unwrap(),
    }
}

fn mot(table: TeletextResourceIdentity, normal: bool) -> TeletextMotLevel25Declaration {
    let object = TeletextMotObjectLink::new(
        unused(),
        TeletextMotSubpageExtent::new(0).unwrap(),
        false,
        TeletextMotFallback::new(0).unwrap(),
        [TeletextMotDefaultObject::NoObject { raw_number: 0 }; 2],
    );
    let inactive =
        TeletextMotDrcsLink::new(unused(), TeletextMotSubpageExtent::new(0).unwrap(), false);
    let selected = TeletextMotDrcsLink::new(
        TeletextX27LinkTarget::Page { page: table.page() },
        TeletextMotSubpageExtent::new(table.logical_subpage().get()).unwrap(),
        false,
    );
    let association = TeletextMotAssociationSelection::new(
        !normal,
        normal.then(|| TeletextMotPublicLinkIndex::new(1).unwrap()),
    );
    let mut public = [inactive; 7];
    if normal {
        public[0] = selected;
    }
    TeletextMotLevel25Declaration::new(
        TeletextResourceIdentity::new(
            TeletextResourceKind::Mot,
            TeletextRawDataPageNumber::new(2, 0xfe).unwrap(),
            TeletextResourceSubpageIndex::ZERO,
        )
        .unwrap(),
        vec![TeletextMotPageAssociation::new(
            0,
            TeletextMotAssociationSelection::new(false, None),
            association,
        )],
        TeletextMotLevel25Links::new(
            object,
            [object; 7],
            if normal { inactive } else { selected },
            public,
            TeletextMotEnhancementCounts::new(1, 0, table.logical_subpage().get() + 1),
        )
        .unwrap(),
    )
    .unwrap()
}

fn x26(owner: TeletextPageAddress, s1: u8, slot: u8, normal: bool) -> [u8; 46] {
    let program = TeletextEnhancementProgram::new(vec![
        TeletextEnhancementCommand::SetPosition {
            position: TeletextEnhancementPosition::new(1, 8).unwrap(),
        },
        TeletextEnhancementCommand::AtColumn {
            column: 8,
            operation: TeletextColumnOperation::Drcs {
                reference: TeletextDrcsGlyphReference {
                    table: if normal {
                        TeletextDrcsTableScope::Normal
                    } else {
                        TeletextDrcsTableScope::Global
                    },
                    subtable: TeletextDrcsSubtableIndex::new(s1).unwrap(),
                    slot: TeletextDrcsGlyphSlot::new(slot).unwrap(),
                },
            },
        },
    ])
    .unwrap();
    let encoded = compile_teletext_local_program(owner, &program, 2).unwrap();
    assert_eq!(encoded.packet_bodies().len(), 1);
    data_unit(
        owner.page.wire_magazine(),
        26,
        &encoded.packet_bodies()[0],
        false,
    )
}

fn display(table: TeletextResourceIdentity, slot: u8, normal: bool) -> Vec<[u8; 46]> {
    let mut draft = super::transport_tests::draft(200, 0, false);
    draft.rows = vec![
        TeletextRow {
            row: 1,
            runs: vec![TeletextRun {
                column: 8,
                style: TeletextStyle::default(),
                content: TeletextRunContent::Text { text: "T".into() },
            }],
        },
        TeletextRow {
            row: 2,
            runs: vec![TeletextRun {
                column: 1,
                style: TeletextStyle::default(),
                content: TeletextRunContent::Text { text: "B".into() },
            }],
        },
    ];
    let encoded = encode_teletext_page(&draft, TeletextEncodingRegion::WesternLatin).unwrap();
    let base = encoded.data_units();
    let mut units = vec![base[0]];
    units.push(base[1]); // Presentation X28 before content.
    units.push(x26(
        draft.address,
        table.logical_subpage().get(),
        slot,
        normal,
    ));
    assert!(
        matches!(source_header::observe_dvb_teletext_header(&base[2]).unwrap(),
        source_header::TeletextHeaderObservation::NotHeader(address) if address.packet_number==26)
    );
    // Replace the base encoder's empty X26/0, never transmit duplicate designation zero.
    units.extend_from_slice(&base[3..]);
    units.push(terminator(2));
    units
}

#[derive(Clone, Copy)]
enum Damage {
    None,
    MissingRow,
    UndefinedHalf,
    UndefinedMode,
    BadParity,
}

/// Change only the requested mode bits. Row/SPACE probes retain mode zero;
/// a separate undefined-mode probe retains valid bitmap bytes.
fn override_mode(unit: &mut [u8; 46], slot: u8, mode: u8) {
    const DATA: [u32; 18] = [
        3, 5, 6, 7, 9, 10, 11, 12, 13, 14, 15, 17, 18, 19, 20, 21, 22, 23,
    ];
    for bit in 0..4 {
        let offset = 18 + usize::from(slot) * 4 + bit;
        let start = 7 + (offset / 18) * 3;
        let protected = u32::from_le_bytes([
            unit[start].reverse_bits(),
            unit[start + 1].reverse_bits(),
            unit[start + 2].reverse_bits(),
            0,
        ]);
        let mut word = DATA
            .into_iter()
            .enumerate()
            .fold(0, |value, (bit, position)| {
                value | (((protected >> (position - 1)) & 1) << bit)
            });
        let shift = offset % 18;
        word = (word & !(1 << shift)) | (u32::from((mode >> bit) & 1) << shift);
        for (target, value) in unit[start..start + 3].iter_mut().zip(hamming_24_18(word)) {
            *target = value.reverse_bits();
        }
    }
}

fn case(
    name: &str,
    slot: u8,
    s1: u8,
    pixels: [u16; 10],
    normal: bool,
    damage: Damage,
) -> serde_json::Value {
    let owner = resource(
        if normal {
            TeletextResourceKind::Drcs
        } else {
            TeletextResourceKind::Gdrcs
        },
        s1,
    );
    let defined_slot = match damage {
        Damage::MissingRow => 0,
        Damage::UndefinedHalf => slot ^ 1,
        Damage::None | Damage::UndefinedMode | Damage::BadParity => slot,
    };
    let table = TeletextDrcsSubtable::new(
        owner,
        vec![TeletextDrcsGlyphDefinition {
            slot: TeletextDrcsGlyphSlot::new(defined_slot).unwrap(),
            bitmap: TeletextDrcsBitmap::new(pixels).unwrap(),
        }],
    )
    .unwrap();
    let counter = TeletextResourceUpdateCounter::new(0).unwrap();
    let mut units = Vec::new();
    units.extend_from_slice(
        encode_teletext_mot_page(
            &mot(owner, normal),
            &[[None; 2]; 8],
            counter,
            TeletextResourceFullUpdate::Changed,
            TeletextResourceTransmissionMode::Serial,
        )
        .unwrap()
        .data_units(),
    );
    units.push(terminator(2));
    // Supply the complete contiguous MOT extent, including empty subtables.
    for logical in 0..=s1 {
        let current = if logical == s1 {
            table.clone()
        } else {
            TeletextDrcsSubtable::new(resource(owner.kind(), logical), vec![]).unwrap()
        };
        let encoded = encode_teletext_drcs_page(
            &current,
            counter,
            TeletextResourceFullUpdate::Changed,
            TeletextResourceTransmissionMode::Serial,
        )
        .unwrap();
        let mut resource_units = encoded.data_units().to_vec();
        if logical == s1 {
            match damage {
                Damage::MissingRow | Damage::UndefinedHalf => {
                    override_mode(&mut resource_units[1], slot, 0);
                }
                Damage::UndefinedMode => override_mode(&mut resource_units[1], slot, 15),
                Damage::None | Damage::BadParity => {}
            }
        }
        if logical == s1 && matches!(damage, Damage::BadParity) {
            // Only the first D-byte parity changes, after DVB bit reversal.
            resource_units[2][6 + 20 * usize::from(slot % 2)] ^= 1;
        }
        units.extend(resource_units);
        // libzvbi 0.2.42 needs own-magazine closure after C4-set resources.
        // The resource lifecycle/bytes stay unchanged; FF:0 is a valid terminator.
        units.push(terminator(0));
    }
    units.extend(display(owner, slot, normal));
    serde_json::json!({"name":name,"units":units.iter().map(|unit|unit.to_vec()).collect::<Vec<_>>(),
        "page":0x200,"subcode":0,"row":1,"column":8,"slot":slot,"s1":s1,
        "normal":normal,"pixels":pixels,"expect_glyph":matches!(damage,Damage::None)})
}

#[tokio::test]
#[ignore = "Requires Python 3 and the explicitly calibrated native libzvbi decoder"]
async fn authored_resource_pages_decode_with_pinned_independent_libzvbi() {
    let cases = vec![
        case("mot_corner_slot0", 0, 0, [0x801; 10], false, Damage::None),
        case(
            "mot_asymmetric_slot1",
            1,
            15,
            [0, 0x800, 1, 0xaaa, 0x555, 0x3f, 0xfc0, 0xfff, 0x801, 0x123],
            false,
            Damage::None,
        ),
        case("mot_global_slot46", 46, 0, [0x800; 10], false, Damage::None),
        case("mot_normal_slot47", 47, 0, [1; 10], true, Damage::None),
        case("defined_blank", 0, 0, [0; 10], false, Damage::None),
        case("missing_row", 47, 0, [0x801; 10], false, Damage::MissingRow),
        case(
            "undefined_half",
            1,
            0,
            [0x801; 10],
            false,
            Damage::UndefinedHalf,
        ),
        case("bad_parity", 0, 0, [0x801; 10], false, Damage::BadParity),
        case(
            "undefined_mode",
            0,
            0,
            [0x801; 10],
            false,
            Damage::UndefinedMode,
        ),
    ];
    let request =
        serde_json::to_vec(&serde_json::json!({"expected_version":"0.2.42","cases":cases}))
            .unwrap();
    if let Some(path) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_RESOURCE_FIXTURE"])
    {
        std::fs::write(path, &request).unwrap();
    }
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/e2e/teletext-resource-decoder.py");
    let python = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_PYTHON"])
        .unwrap_or_else(|| "python3".into());
    let report =
        crate::teletext_probe::run_probe(&python, &[script.to_str().unwrap()], &request, 16 * 1024)
            .await
            .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&report).unwrap();
    assert_eq!(report["decoder_version"], "0.2.42");
    assert_eq!(report["cases_passed"], 9);
    println!("Independent resource calibration: {report}");
}
