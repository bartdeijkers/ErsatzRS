//! Exact bitmap intentions through complete native encoding, without edit rights.

use super::authored_service_tests as fixtures;
use super::*;
use ersatzrs_domain::*;

const BEFORE: [u16; 10] = [
    0x801, 0x800, 0x400, 0x200, 0x100, 0x080, 0x040, 0x020, 0x010, 0x009,
];
const AFTER: [u16; 10] = [
    0x803, 0x800, 0x400, 0x200, 0x100, 0x080, 0x040, 0x020, 0x010, 0x009,
];

fn semantic_budget() -> TeletextServiceBudget {
    TeletextServiceBudget {
        max_collected_drcs_references: 2,
        ..fixtures::semantic_limits()
    }
}

fn encoding_budget() -> TeletextAuthoredServiceEncodingBudget {
    let mut budget = fixtures::limits();
    budget
        .normalization
        .composition
        .collection
        .max_drcs_references = 1;
    budget.normalization.replay.max_drcs_references = 1;
    budget
}

fn original(normal: bool) -> (TeletextDeclaredService, TeletextResourceIdentity) {
    let kind = if normal {
        TeletextResourceKind::Drcs
    } else {
        TeletextResourceKind::Gdrcs
    };
    let identity = TeletextResourceIdentity::new(
        kind,
        TeletextRawDataPageNumber::new(2, 0xc0).unwrap(),
        TeletextResourceSubpageIndex::ZERO,
    )
    .unwrap();
    let mut draft = fixtures::input();
    draft.objects.clear();
    draft.drcs = vec![TeletextDrcsSubtable::new(
        identity,
        vec![
            TeletextDrcsGlyphDefinition {
                slot: TeletextDrcsGlyphSlot::new(0).unwrap(),
                bitmap: TeletextDrcsBitmap::new(BEFORE).unwrap(),
            },
            TeletextDrcsGlyphDefinition {
                slot: TeletextDrcsGlyphSlot::new(1).unwrap(),
                bitmap: TeletextDrcsBitmap::new([0x123; 10]).unwrap(),
            },
        ],
    )
    .unwrap()];
    for page in &mut draft.pages {
        page.resources.objects = TeletextResolvedObjectPageLinks {
            public: None,
            global: None,
        };
        page.resources.defaults = [None; 4];
        page.resources.drcs = TeletextDrcsPageLinks::new(
            normal.then_some(identity.page()),
            (!normal).then_some(identity.page()),
        )
        .unwrap();
        page.local_program = Some(
            TeletextEnhancementProgram::new(vec![
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
                            subtable: TeletextDrcsSubtableIndex::new(0).unwrap(),
                            slot: TeletextDrcsGlyphSlot::new(0).unwrap(),
                        },
                    },
                },
            ])
            .unwrap(),
        );
    }
    let semantic = TeletextSemanticService::new(draft, semantic_budget()).unwrap();
    let mot = super::resource_runtime_tests::mot(identity, normal);
    let selections = semantic
        .base()
        .pages
        .iter()
        .map(|page| TeletextDisplayMotSelection {
            display: page.address,
            mot: Some(mot.identity()),
        })
        .collect();
    let declared = TeletextDeclaredService::new(
        semantic,
        vec![mot],
        selections,
        vec![],
        fixtures::declaration_limits(),
    )
    .unwrap();
    (declared, identity)
}

fn compiled(service: TeletextDeclaredService) -> EncodedTeletextAuthoredService {
    let tx = fixtures::transmissions(&service);
    encode_teletext_authored_service(service, &tx, &encoding_budget()).unwrap()
}

fn packet(unit: &[u8; 46]) -> u8 {
    use super::source_header::{observe_dvb_teletext_header, TeletextHeaderObservation};
    match observe_dvb_teletext_header(unit).unwrap() {
        TeletextHeaderObservation::Header(header) => header.packet_address.packet_number,
        TeletextHeaderObservation::NotHeader(address) => address.packet_number,
        _ => panic!("complete encoder must emit a page packet"),
    }
}

fn composition(page: &EncodedTeletextAuthoredPage) -> &TeletextMainComposition {
    let EncodedTeletextAuthoredPage::CompiledDisplay(display) = page else {
        panic!("explicit DRCS programs must use the complete display compiler");
    };
    display.normalized().original()
}

fn assert_exact_change(normal: bool) {
    let (original, identity) = original(normal);
    let root = original.clone();
    let intent = TeletextDrcsBitmapOverlay::new(
        identity,
        TeletextDrcsGlyphSlot::new(0).unwrap(),
        TeletextDrcsBitmap::new(BEFORE).unwrap(),
        TeletextDrcsBitmap::new(AFTER).unwrap(),
    )
    .unwrap();
    let edited = original
        .clone()
        .with_drcs_bitmap_overlays(&[intent], semantic_budget())
        .unwrap();
    let before = compiled(original);
    let after = compiled(edited.clone());
    assert_eq!(before.declared(), &root);
    assert_eq!(after.inventory(), before.inventory());
    assert_eq!(after.magazine_units(), before.magazine_units());
    assert_eq!(after.unit_count(), before.unit_count());
    assert_eq!(after.pages().len(), before.pages().len());
    assert_eq!(edited.mots(), root.mots());
    assert_eq!(edited.selections(), root.selections());
    assert_eq!(edited.resolutions(), root.resolutions());
    assert_eq!(edited.defaults(), root.defaults());
    let mut changed_units = 0;
    for (old, new) in before.pages().iter().zip(after.pages()) {
        assert_eq!(old.inventory_record(), new.inventory_record());
        assert_eq!(old.resource_header(), new.resource_header());
        assert_eq!(old.data_units().len(), new.data_units().len());
        for (old, new) in old.data_units().iter().zip(new.data_units()) {
            if old == new {
                continue;
            }
            changed_units += 1;
            assert_eq!(packet(old), 1);
            assert_eq!(packet(new), 1);
            assert_eq!(&old[..7], &new[..7]);
            assert_eq!(&old[8..], &new[8..]);
            // Exact QTM carrier change A->C, independently stripping DVB bit
            // reversal/parity rather than asking the encoder for its own oracle.
            assert_eq!(old[7].reverse_bits() & 0x7f, b'A');
            assert_eq!(new[7].reverse_bits() & 0x7f, b'C');
            for unit in [old, new] {
                assert!(unit[6..].iter().all(|byte| byte.count_ones() % 2 == 1));
            }
        }
    }
    assert_eq!(changed_units, 1);
    // Both display consumers keep *every* original packet, including Level1
    // fallback, X26 references and X27 links, while their resolved glyph changes.
    // Thus packet equality must never bypass shared-resource permission checks.
    for index in 0..2 {
        assert_eq!(
            before.pages()[index].data_units(),
            after.pages()[index].data_units()
        );
        let old = composition(&before.pages()[index]);
        let new = composition(&after.pages()[index]);
        for (composition, bitmap) in [(old, BEFORE), (new, AFTER)] {
            let cell = composition.display_resolution().cell(1, 8).unwrap();
            assert_eq!(
                project_teletext_comparison_glyph(composition, cell)
                    .unwrap()
                    .mask
                    .unwrap()
                    .rows(),
                bitmap
            );
            assert_eq!(composition.base_display().cell(1, 8).unwrap().code(), b'Z');
        }
        let old_cell = old.display_resolution().cell(1, 9).unwrap();
        let new_cell = new.display_resolution().cell(1, 9).unwrap();
        assert_eq!(
            project_teletext_comparison_glyph(old, old_cell).unwrap(),
            project_teletext_comparison_glyph(new, new_cell).unwrap()
        );
    }
    let undo = TeletextDrcsBitmapOverlay::new(
        identity,
        intent.slot(),
        intent.replacement(),
        intent.expected(),
    )
    .unwrap();
    let reverted = edited
        .with_drcs_bitmap_overlays(&[undo], semantic_budget())
        .unwrap();
    assert_eq!(reverted, root);
    let reverted = compiled(reverted);
    for (old, new) in before.pages().iter().zip(reverted.pages()) {
        assert_eq!(old.data_units(), new.data_units());
    }
}

#[test]
fn normal_bitmap_intent_changes_resource_and_both_shared_consumers_only() {
    assert_exact_change(true);
}

#[test]
fn global_bitmap_intent_changes_resource_and_both_shared_consumers_only() {
    assert_exact_change(false);
}
