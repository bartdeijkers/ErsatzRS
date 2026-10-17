//! Independent triplet-field vectors for opt-in Level 1.5 row restoration.

use ersatzrs_domain::*;

use super::enhancement_program::{LoweringState, ResourceContext};
use super::{allocate_teletext_resource_objects, TeletextLocalProgramEncodingFailure as Failure};

fn colour(value: u8) -> TeletextColourReference {
    TeletextColourReference::new(value).unwrap()
}
fn position(row: u8, column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(row, column).unwrap(),
    }
}
fn at(column: u8, operation: TeletextColumnOperation) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn { column, operation }
}
fn code(value: u8) -> TeletextGlyphCode {
    TeletextGlyphCode::new(value).unwrap()
}
fn g2(column: u8) -> TeletextEnhancementCommand {
    at(column, TeletextColumnOperation::G2 { code: code(0x23) })
}
fn accent(column: u8) -> TeletextEnhancementCommand {
    at(
        column,
        TeletextColumnOperation::G0Diacritic {
            code: code(b'e'),
            diacritic: TeletextDiacriticIndex::new(2).unwrap(),
        },
    )
}
fn g3(column: u8, compatibility: TeletextG3Compatibility) -> TeletextEnhancementCommand {
    at(
        column,
        TeletextColumnOperation::G3 {
            code: code(0x21),
            compatibility,
        },
    )
}
fn screen() -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetScreenColour { colour: colour(5) }
}
fn drcs(column: u8, table: TeletextDrcsTableScope, subtable: u8) -> TeletextEnhancementCommand {
    at(
        column,
        TeletextColumnOperation::Drcs {
            reference: TeletextDrcsGlyphReference {
                table,
                subtable: TeletextDrcsSubtableIndex::new(subtable).unwrap(),
                slot: TeletextDrcsGlyphSlot::new(2).unwrap(),
            },
        },
    )
}
fn fields(words: &[u32]) -> Vec<(u8, u8, u8)> {
    words
        .iter()
        .map(|&word| {
            (
                u8::try_from(word & 63).unwrap(),
                u8::try_from((word >> 6) & 31).unwrap(),
                u8::try_from(word >> 11).unwrap(),
            )
        })
        .collect()
}
fn lower(state: &mut LoweringState, command: TeletextEnhancementCommand) -> Vec<(u8, u8, u8)> {
    fields(state.lower(command, None).unwrap().words())
}

#[test]
fn screen_address_disturbs_compatibility_but_not_enhanced_ap() {
    let mut state = LoweringState::new_with_compatibility(colour(3));
    assert_eq!(lower(&mut state, position(2, 1)), [(42, 4, 1)]);
    assert_eq!(lower(&mut state, screen()), [(41, 0, 5)]);
    // Literal mode4 at the exact next glyph column restores row two.
    let words = state.lower(g2(3), None).unwrap();
    assert_eq!(words.words(), &[0x192a, 0x11bc3]);
    assert_eq!(lower(&mut state, accent(4)), [(4, 18, b'e')]);
}

#[test]
fn initial_header_selector_preserves_nonblack_colour_and_column() {
    let mut state = LoweringState::new_with_compatibility(colour(11));
    assert_eq!(lower(&mut state, g2(8)), [(63, 7, 11), (8, 15, 0x23)]);
    assert_eq!(lower(&mut state, accent(9)), [(9, 18, b'e')]);
    assert_eq!(
        lower(&mut state, g3(10, TeletextG3Compatibility::Level15)),
        [(10, 2, 0x21)]
    );
}

#[test]
fn first_header_attribute_or_enhanced_glyph_gets_only_one_safe_initial_selector() {
    for operation in [
        TeletextColumnOperation::Foreground { colour: colour(2) },
        TeletextColumnOperation::G0 { code: code(b'A') },
        TeletextColumnOperation::G1 {
            mosaic: TeletextMosaic::new(1).unwrap(),
        },
    ] {
        let mut state = LoweringState::new_with_compatibility(colour(3));
        let words = lower(&mut state, at(8, operation));
        assert_eq!(words.len(), 2);
        assert_eq!(words[0], (63, 7, 3));
        assert_eq!(words[1].0, 8);
        assert_eq!(lower(&mut state, accent(9)), [(9, 18, b'e')]);
    }
}

#[test]
fn drcs_atomic_pair_and_independent_latches_survive_row_restoration() {
    let mut state = LoweringState::new_with_compatibility(colour(0));
    lower(&mut state, position(1, 8));
    assert_eq!(
        lower(&mut state, drcs(8, TeletextDrcsTableScope::Normal, 0)),
        [(40, 24, 0x50), (8, 13, 0x42)]
    );
    assert_eq!(
        lower(&mut state, g3(9, TeletextG3Compatibility::Level15)),
        [(41, 4, 9), (9, 2, 0x21)]
    );
    assert_eq!(
        lower(&mut state, drcs(10, TeletextDrcsTableScope::Normal, 0)),
        [(10, 13, 0x42)]
    );
    assert_eq!(lower(&mut state, g2(11)), [(11, 15, 0x23)]);
    assert_eq!(
        lower(&mut state, drcs(12, TeletextDrcsTableScope::Global, 7)),
        [(40, 24, 0x17), (12, 13, 2)]
    );
    assert_eq!(lower(&mut state, accent(13)), [(41, 4, 13), (13, 18, b'e')]);
    assert_eq!(
        lower(&mut state, drcs(14, TeletextDrcsTableScope::Normal, 0)),
        [(14, 13, 0x42)]
    );
}

#[test]
fn initial_header_drcs_stays_atomic_and_later_glyph_requires_normalization() {
    let mut state = LoweringState::new_with_compatibility(colour(7));
    assert_eq!(
        lower(&mut state, drcs(8, TeletextDrcsTableScope::Normal, 0)),
        [(40, 24, 0x50), (8, 13, 0x42)]
    );
    assert_eq!(
        state.lower(g2(9), None).err().unwrap(),
        Failure::NeedsCompatibilityRowLowering { position: (0, 9) }
    );
}

#[test]
fn row_24_address_40_and_row_zero_address_63_have_distinct_meanings() {
    let mut state = LoweringState::new_with_compatibility(colour(4));
    assert_eq!(
        lower(
            &mut state,
            TeletextEnhancementCommand::SetRowColour {
                row: 0,
                colour: colour(9),
                following_rows: false
            }
        ),
        [(63, 7, 9)]
    );
    assert_eq!(lower(&mut state, accent(8)), [(8, 18, b'e')]);
    assert_eq!(lower(&mut state, position(24, 1)), [(40, 4, 1)]);
    assert_eq!(
        lower(&mut state, drcs(2, TeletextDrcsTableScope::Normal, 0)),
        [(40, 24, 0x50), (2, 13, 0x42)]
    );
    assert_eq!(lower(&mut state, g2(3)), [(3, 15, 0x23)]);
    lower(&mut state, screen());
    assert_eq!(lower(&mut state, accent(4)), [(40, 4, 4), (4, 18, b'e')]);
}

#[test]
fn enhanced_only_glyphs_do_not_restore_nonheader_compatibility_row() {
    let mut state = LoweringState::new_with_compatibility(colour(0));
    lower(&mut state, position(2, 0));
    lower(&mut state, screen());
    assert_eq!(
        lower(
            &mut state,
            at(1, TeletextColumnOperation::G0 { code: code(b'A') })
        ),
        [(1, 9, b'A')]
    );
    assert_eq!(
        lower(
            &mut state,
            at(
                2,
                TeletextColumnOperation::G1 {
                    mosaic: TeletextMosaic::new(1).unwrap()
                }
            )
        ),
        [(2, 1, 0x21)]
    );
    assert_eq!(
        lower(&mut state, g3(3, TeletextG3Compatibility::Level25)),
        [(3, 11, 0x21)]
    );
    assert_eq!(lower(&mut state, g2(4)), [(42, 4, 4), (4, 15, 0x23)]);
}

#[test]
fn late_header_error_is_exact_and_failed_command_does_not_advance_state() {
    let mut state = LoweringState::new_with_compatibility(colour(3));
    lower(&mut state, g2(8));
    lower(&mut state, screen());
    assert_eq!(
        state.lower(accent(9), None).err().unwrap(),
        Failure::NeedsCompatibilityRowLowering { position: (0, 9) }
    );
    // Failed column nine did not commit AP; original column eight still works.
    assert_eq!(
        lower(&mut state, g3(8, TeletextG3Compatibility::Level25)),
        [(8, 11, 0x21)]
    );
    let position = TeletextEnhancementPosition::new(0, 7).unwrap();
    assert_eq!(
        state
            .lower(TeletextEnhancementCommand::SetPosition { position }, None)
            .err()
            .unwrap(),
        Failure::ActivePositionOrder {
            previous: (0, 8),
            requested: (0, 7),
        }
    );
}

#[test]
fn origin_modifier_and_local_invocation_are_observed_sequentially_without_rewriting() {
    let owner = TeletextPageAddress {
        page: TeletextPageNumber::new(200).unwrap(),
        subcode: TeletextSubcode::new(1).unwrap(),
    };
    let target = TeletextObjectIdentity::local(owner, TeletextObjectId::new(0xffff));
    let catalog = TeletextEnhancementObjectCatalog::new(
        vec![],
        TeletextObjectCatalogBudget {
            max_objects: 0,
            max_commands: 0,
        },
    )
    .unwrap();
    let allocations = allocate_teletext_resource_objects(&catalog, &[], 0).unwrap();
    let numbers = [(target, TeletextObjectType::Passive, 0x80)];
    let context = ResourceContext::local_page(
        &allocations,
        TeletextResolvedObjectPageLinks {
            public: None,
            global: None,
        },
        &numbers,
    );
    let mut state = LoweringState::new_with_compatibility(colour(3));
    lower(&mut state, position(2, 8));
    let call = TeletextEnhancementCommand::InvokeObject {
        target,
        object_type: TeletextObjectType::Passive,
        offset: Some(TeletextObjectOffset::new(5, 2).unwrap()),
    };
    assert_eq!(
        fields(state.lower(call, Some(context)).unwrap().words()),
        [(45, 16, 2), (41, 19, 0)]
    );
    // Local allocation can change its final raw row address. Neither that
    // address nor its modifier grants a stable compatibility-row assumption.
    // Enhanced AP remains row two/column eight; restore precisely that row.
    assert_eq!(lower(&mut state, accent(9)), [(42, 4, 9), (9, 18, b'e')]);
    assert_eq!(
        fields(state.lower(call, Some(context)).unwrap().words()),
        [(45, 16, 2), (41, 19, 0)]
    );
}

#[test]
fn original_factory_retains_exact_legacy_words_without_policy() {
    let commands = [
        position(2, 1),
        screen(),
        g2(3),
        accent(4),
        drcs(5, TeletextDrcsTableScope::Normal, 0),
        g3(6, TeletextG3Compatibility::Level15),
    ];
    let mut old = LoweringState::new();
    let words: Vec<_> = commands
        .into_iter()
        .flat_map(|command| lower(&mut old, command))
        .collect();
    assert_eq!(
        words,
        [
            (42, 4, 1),
            (41, 0, 5),
            (3, 15, 0x23),
            (4, 18, b'e'),
            (40, 24, 0x50),
            (5, 13, 0x42),
            (6, 2, 0x21)
        ]
    );
    let mut old = LoweringState::new();
    assert_eq!(lower(&mut old, accent(8)), [(8, 18, b'e')]);
}

#[test]
fn local_placeholder_and_high_marker_have_identical_restoration_size() {
    let owner = TeletextPageAddress {
        page: TeletextPageNumber::new(200).unwrap(),
        subcode: TeletextSubcode::new(1).unwrap(),
    };
    let target = TeletextObjectIdentity::local(owner, TeletextObjectId::new(0xffff));
    let catalog = TeletextEnhancementObjectCatalog::new(
        vec![],
        TeletextObjectCatalogBudget {
            max_objects: 0,
            max_commands: 0,
        },
    )
    .unwrap();
    let allocations = allocate_teletext_resource_objects(&catalog, &[], 0).unwrap();
    let links = TeletextResolvedObjectPageLinks {
        public: None,
        global: None,
    };
    for row in [1, 24] {
        for number in [0, 0x80] {
            let numbers = [(target, TeletextObjectType::Passive, number)];
            let context = ResourceContext::local_page(&allocations, links, &numbers);
            let call = TeletextEnhancementCommand::InvokeObject {
                target,
                object_type: TeletextObjectType::Passive,
                offset: None,
            };
            let mut policy = LoweringState::new_with_compatibility(colour(3));
            lower(&mut policy, position(row, 8));
            let words = fields(policy.lower(call, Some(context)).unwrap().words());
            assert_eq!(words, [(if number == 0 { 40 } else { 41 }, 19, 0)]);
            assert_eq!(
                lower(&mut policy, accent(9)),
                [(if row == 24 { 40 } else { 41 }, 4, 9), (9, 18, b'e')]
            );
            assert_eq!(lower(&mut policy, g2(10)), [(10, 15, 0x23)]);
            let mut old = LoweringState::new();
            lower(&mut old, position(row, 8));
            assert_eq!(
                fields(old.lower(call, Some(context)).unwrap().words()),
                words
            );
            assert_eq!(lower(&mut old, accent(9)), [(9, 18, b'e')]);
        }
    }
}
