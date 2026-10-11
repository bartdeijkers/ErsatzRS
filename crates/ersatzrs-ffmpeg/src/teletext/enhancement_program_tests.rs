//! Independent EN 300 706 tables 28/29 field and Hamming checks.

use ersatzrs_domain::*;

use super::{
    compile_teletext_local_program, TeletextLocalProgramEncodingError,
    TeletextLocalProgramEncodingFailure,
};

fn owner() -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(200).unwrap(),
        subcode: TeletextSubcode::new(0x1234).unwrap(),
    }
}

fn program(commands: Vec<TeletextEnhancementCommand>) -> TeletextEnhancementProgram {
    TeletextEnhancementProgram::new(commands).unwrap()
}

fn position(row: u8, column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(row, column).unwrap(),
    }
}

fn at(column: u8, operation: TeletextColumnOperation) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn { column, operation }
}

fn colour(value: u8) -> TeletextColourReference {
    TeletextColourReference::new(value).unwrap()
}

fn glyph(value: u8) -> TeletextGlyphCode {
    TeletextGlyphCode::new(value).unwrap()
}

// No production triplet builder, encoder, decoder or parity table is reused.
fn words(bodies: &[[u8; 40]]) -> Vec<(u8, u8, u8)> {
    let hamming = [
        0x15, 0x02, 0x49, 0x5e, 0x64, 0x73, 0x38, 0x2f, 0xd0, 0xc7, 0x8c, 0x9b, 0xa1, 0xb6, 0xfd,
        0xea,
    ];
    bodies
        .iter()
        .enumerate()
        .flat_map(|(designation, body)| {
            assert_eq!(body[0], hamming[designation]);
            body[1..].chunks_exact(3).map(|bytes| {
                let raw = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], 0]);
                assert_eq!(raw.count_ones() % 2, 1);
                for parity in [1_u32, 2, 4, 8, 16] {
                    let ones: u32 = (1_u32..24)
                        .filter(|position| position & parity != 0)
                        .map(|position| (raw >> (position - 1)) & 1)
                        .sum();
                    assert_eq!(ones % 2, 1);
                }
                let positions = [
                    3, 5, 6, 7, 9, 10, 11, 12, 13, 14, 15, 17, 18, 19, 20, 21, 22, 23,
                ];
                let data = positions
                    .into_iter()
                    .enumerate()
                    .fold(0_u32, |value, (bit, position)| {
                        value | (((raw >> (position - 1)) & 1) << bit)
                    });
                (
                    u8::try_from(data & 63).unwrap(),
                    u8::try_from((data >> 6) & 31).unwrap(),
                    u8::try_from(data >> 11).unwrap(),
                )
            })
        })
        .collect()
}

fn encoded(commands: Vec<TeletextEnhancementCommand>) -> Vec<(u8, u8, u8)> {
    let input = program(commands);
    let before = input.clone();
    let output = compile_teletext_local_program(owner(), &input, 208).unwrap();
    assert_eq!(input, before);
    assert_eq!(output.owner(), owner());
    words(output.packet_bodies())
}

#[test]
fn every_row_address_and_colour_preserves_exact_field_values() {
    let mut commands = vec![TeletextEnhancementCommand::SetScreenColour { colour: colour(31) }];
    let mut expected = vec![(41, 0, 31)];
    commands.push(TeletextEnhancementCommand::SetRowColour {
        row: 0,
        colour: colour(7),
        following_rows: true,
    });
    expected.push((63, 7, 0x67));
    for row in 1_u8..=24 {
        commands.push(TeletextEnhancementCommand::SetRowColour {
            row,
            colour: colour(row),
            following_rows: row % 2 == 0,
        });
        commands.push(position(row, 39));
        let address = if row == 24 { 40 } else { 40 + row };
        expected.push((address, 1, row | if row % 2 == 0 { 0x60 } else { 0 }));
        expected.push((address, 4, 39));
    }
    let actual = encoded(commands);
    assert_eq!(&actual[..expected.len()], expected);
    assert!(actual[expected.len()..]
        .iter()
        .all(|word| *word == (63, 31, 7)));
}

#[test]
fn all_column_operation_families_preserve_modes_and_reserved_bits() {
    let actual = encoded(vec![
        position(1, 1),
        at(
            1,
            TeletextColumnOperation::Foreground { colour: colour(31) },
        ),
        at(
            1,
            TeletextColumnOperation::Background { colour: colour(16) },
        ),
        at(
            1,
            TeletextColumnOperation::DesignateCharset {
                designation: TeletextEnhancementCharsetDesignation::new(0x57).unwrap(),
            },
        ),
        at(1, TeletextColumnOperation::G0 { code: glyph(0x7f) }),
        at(
            1,
            TeletextColumnOperation::G1 {
                mosaic: TeletextMosaic::new(0x21).unwrap(),
            },
        ),
        at(1, TeletextColumnOperation::G2 { code: glyph(0x60) }),
        at(
            1,
            TeletextColumnOperation::G3 {
                code: glyph(0x2f),
                compatibility: TeletextG3Compatibility::Level15,
            },
        ),
        at(
            1,
            TeletextColumnOperation::G3 {
                code: glyph(0x7e),
                compatibility: TeletextG3Compatibility::Level25,
            },
        ),
        at(
            1,
            TeletextColumnOperation::Attributes {
                double_height: true,
                double_width: true,
                boxed: true,
                conceal: true,
                invert: true,
                underline_or_separated_mosaics: true,
            },
        ),
        at(
            1,
            TeletextColumnOperation::Attributes {
                double_height: false,
                double_width: false,
                boxed: false,
                conceal: false,
                invert: false,
                underline_or_separated_mosaics: false,
            },
        ),
    ]);
    assert_eq!(
        &actual[..11],
        &[
            (41, 4, 1),
            (1, 0, 31),
            (1, 3, 16),
            (1, 8, 0x57),
            (1, 9, 0x7f),
            (1, 1, 0x61),
            (1, 15, 0x60),
            (1, 2, 0x2f),
            (1, 11, 0x7e),
            (1, 12, 0x77),
            (1, 12, 0),
        ]
    );
}

#[test]
fn every_flash_combination_and_diacritic_index_is_preserved() {
    let modes = [
        TeletextFlashMode::Steady,
        TeletextFlashMode::Normal,
        TeletextFlashMode::Inverted,
        TeletextFlashMode::AdjacentClut,
    ];
    let rates = [
        TeletextFlashRate::Slow1Hz,
        TeletextFlashRate::Fast2HzPhase1,
        TeletextFlashRate::Fast2HzPhase2,
        TeletextFlashRate::Fast2HzPhase3,
        TeletextFlashRate::Fast2HzIncremental,
        TeletextFlashRate::Fast2HzDecremental,
    ];
    for (m, mode) in modes.into_iter().enumerate() {
        for (r, rate) in rates.into_iter().enumerate() {
            let actual = encoded(vec![
                position(2, 0),
                at(0, TeletextColumnOperation::Flash { mode, rate }),
            ]);
            assert_eq!(actual[1], (0, 7, u8::try_from(m + 4 * r).unwrap()));
        }
    }
    for index in 0_u8..16 {
        let actual = encoded(vec![
            position(2, 0),
            at(
                0,
                TeletextColumnOperation::G0Diacritic {
                    code: glyph(0x2a),
                    diacritic: TeletextDiacriticIndex::new(index).unwrap(),
                },
            ),
        ]);
        assert_eq!(actual[1], (0, 0x10 + index, 0x2a));
    }
}

#[test]
fn all_mosaics_and_assigned_charset_aliases_preserve_original_codes() {
    for blocks in 0_u8..64 {
        let actual = encoded(vec![
            position(3, 0),
            at(
                0,
                TeletextColumnOperation::G1 {
                    mosaic: TeletextMosaic::new(blocks).unwrap(),
                },
            ),
        ]);
        assert_eq!(actual[1], (0, 1, 0x20 + blocks % 32 + (blocks / 32) * 64));
    }
    for code in (0_u8..=0x26).chain([0x36, 0x37, 0x40, 0x44, 0x47, 0x55, 0x57]) {
        let actual = encoded(vec![
            position(3, 0),
            at(
                0,
                TeletextColumnOperation::DesignateCharset {
                    designation: TeletextEnhancementCharsetDesignation::new(code).unwrap(),
                },
            ),
        ]);
        assert_eq!(actual[1], (0, 8, code));
    }
}

fn drcs(table: TeletextDrcsTableScope, subtable: u8, slot: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::Drcs {
        reference: TeletextDrcsGlyphReference {
            table,
            subtable: TeletextDrcsSubtableIndex::new(subtable).unwrap(),
            slot: TeletextDrcsGlyphSlot::new(slot).unwrap(),
        },
    }
}

#[test]
fn separate_drcs_latches_select_zero_explicitly_and_reselect_only_changed_scope() {
    use TeletextDrcsTableScope::{Global, Normal};
    let actual = encoded(vec![
        position(4, 8),
        at(8, drcs(Global, 0, 0)),
        at(8, drcs(Normal, 15, 47)),
        at(8, drcs(Global, 0, 46)),
        at(8, drcs(Normal, 15, 1)),
        at(8, drcs(Global, 15, 1)),
        at(8, drcs(Normal, 0, 0)),
    ]);
    assert_eq!(
        &actual[..11],
        &[
            (44, 4, 8),
            (40, 24, 0x10),
            (8, 13, 0),
            (40, 24, 0x5f),
            (8, 13, 0x6f),
            (8, 13, 46),
            (8, 13, 0x41),
            (40, 24, 0x1f),
            (8, 13, 1),
            (40, 24, 0x50),
            (8, 13, 0x40),
        ]
    );
    for table in [Global, Normal] {
        for subtable in 0_u8..16 {
            for slot in 0_u8..48 {
                let actual = encoded(vec![position(1, 39), at(39, drcs(table, subtable, slot))]);
                let scope = if table == Normal { 0x40 } else { 0 };
                assert_eq!(actual[1], (40, 24, scope | 0x10 | subtable));
                assert_eq!(actual[2], (39, 13, scope | slot));
            }
        }
    }
}

#[test]
fn exact_packet_capacity_includes_terminator_and_repeated_padding() {
    for count in [0, 1, 12, 13, 14, 206, 207] {
        let input = program(vec![
            at(
                8,
                TeletextColumnOperation::Foreground { colour: colour(2) }
            );
            count
        ]);
        let output = compile_teletext_local_program(owner(), &input, 208).unwrap();
        assert_eq!(output.payload_triplet_count(), count);
        assert_eq!(output.packet_bodies().len(), (count + 1).div_ceil(13));
        let decoded = words(output.packet_bodies());
        assert!(decoded[..count].iter().all(|word| *word == (8, 0, 2)));
        assert!(decoded[count..].iter().all(|word| *word == (63, 31, 7)));
    }
    let input = program(vec![
        at(
            8,
            TeletextColumnOperation::Foreground { colour: colour(2) }
        );
        208
    ]);
    let before = input.clone();
    let error = compile_teletext_local_program(owner(), &input, usize::MAX).unwrap_err();
    assert_eq!(input, before);
    assert_eq!(error.owner, owner());
    assert_eq!(error.location.unwrap().command, 207);
    assert_eq!(
        error.reason,
        TeletextLocalProgramEncodingFailure::PayloadCapacity
    );
}

#[test]
fn incomplete_lowering_retains_exact_owner_command_and_object_without_mutation() {
    let target = TeletextObjectIdentity::local(owner(), TeletextObjectId::new(0xabcd));
    for command in [
        position(0, 9),
        TeletextEnhancementCommand::InvokeObject {
            target,
            object_type: TeletextObjectType::Passive,
            offset: Some(TeletextObjectOffset::new(23, 71).unwrap()),
        },
    ] {
        let input = program(vec![position(1, 0), command]);
        let before = input.clone();
        let error = compile_teletext_local_program(owner(), &input, 2).unwrap_err();
        assert_eq!(input, before);
        assert_eq!(error.owner, owner());
        assert_eq!(
            error.location.unwrap(),
            TeletextEnhancementSource {
                object: None,
                command: 1
            }
        );
        let reason = match command {
            TeletextEnhancementCommand::SetPosition { position } => {
                TeletextLocalProgramEncodingFailure::NeedsRowZeroLowering { position }
            }
            TeletextEnhancementCommand::InvokeObject {
                target,
                object_type,
                offset,
            } => TeletextLocalProgramEncodingFailure::NeedsObjectAllocation {
                target,
                object_type,
                offset,
            },
            _ => unreachable!(),
        };
        assert_eq!(error.reason, reason);
    }
}

#[test]
fn command_budget_is_checked_before_any_lowering_or_payload_allocation() {
    let input = program(vec![position(0, 0); 1000]);
    let error: TeletextLocalProgramEncodingError =
        compile_teletext_local_program(owner(), &input, 999).unwrap_err();
    assert_eq!(error.owner, owner());
    assert!(error.location.is_none());
    assert_eq!(
        error.reason,
        TeletextLocalProgramEncodingFailure::CommandBudget { maximum: 999 }
    );
}

#[test]
fn each_attribute_bit_is_independent_and_reserved_bit_stays_zero() {
    for flags in 0_u8..64 {
        let actual = encoded(vec![
            position(1, 0),
            at(
                0,
                TeletextColumnOperation::Attributes {
                    double_height: flags & 1 != 0,
                    boxed: flags & 2 != 0,
                    conceal: flags & 4 != 0,
                    invert: flags & 8 != 0,
                    underline_or_separated_mosaics: flags & 16 != 0,
                    double_width: flags & 32 != 0,
                },
            ),
        ]);
        assert_eq!(actual[1], (0, 12, (flags & 7) | ((flags & 0x38) << 1)));
        assert_eq!(actual[1].2 & 8, 0);
    }
}

#[test]
fn backward_active_positions_are_located_and_never_reordered() {
    for commands in [
        vec![position(2, 9), position(1, 39)],
        vec![
            position(2, 9),
            at(8, TeletextColumnOperation::G0 { code: glyph(0x41) }),
        ],
        vec![
            position(2, 9),
            TeletextEnhancementCommand::SetRowColour {
                row: 2,
                colour: colour(7),
                following_rows: false,
            },
        ],
        vec![
            position(2, 9),
            TeletextEnhancementCommand::SetRowColour {
                row: 0,
                colour: colour(7),
                following_rows: false,
            },
        ],
    ] {
        let input = program(commands);
        let before = input.clone();
        let error = compile_teletext_local_program(owner(), &input, 2).unwrap_err();
        assert_eq!(input, before);
        assert_eq!(error.owner, owner());
        assert_eq!(error.location.unwrap().command, 1);
    }
}

#[test]
fn compiler_inserted_drcs_selectors_consume_capacity_at_exact_authored_command() {
    let mut commands = vec![position(1, 8)];
    for index in 0_u8..103 {
        commands.push(at(8, drcs(TeletextDrcsTableScope::Global, index % 16, 0)));
    }
    let input = program(commands.clone());
    let output = compile_teletext_local_program(owner(), &input, 105).unwrap();
    assert_eq!(output.payload_triplet_count(), 207);
    assert_eq!(output.packet_bodies().len(), 16);
    assert_eq!(words(output.packet_bodies())[207], (63, 31, 7));
    // Same latched S1 adds one invocation and crosses the boundary at command104.
    commands.push(at(8, drcs(TeletextDrcsTableScope::Global, 6, 1)));
    let input = program(commands.clone());
    let before = input.clone();
    let error = compile_teletext_local_program(owner(), &input, 105).unwrap_err();
    assert_eq!(input, before);
    assert_eq!(error.location.unwrap().command, 104);
    // A different S1 needs two words and fails at the same authored command.
    commands[104] = at(8, drcs(TeletextDrcsTableScope::Global, 7, 1));
    let input = program(commands);
    let error = compile_teletext_local_program(owner(), &input, 105).unwrap_err();
    assert_eq!(error.location.unwrap().command, 104);
}

#[test]
fn drcs_selector_and_invocation_preserve_order_across_packet_boundary() {
    let mut commands = vec![position(1, 8)];
    for index in 0_u8..5 {
        commands.push(at(8, drcs(TeletextDrcsTableScope::Global, index, 0)));
    }
    commands.push(at(
        8,
        TeletextColumnOperation::Foreground { colour: colour(2) },
    ));
    commands.push(at(8, drcs(TeletextDrcsTableScope::Global, 15, 47)));
    let actual = encoded(commands);
    assert_eq!(actual[11], (8, 0, 2));
    assert_eq!(actual[12], (40, 24, 0x1f));
    assert_eq!(actual[13], (8, 13, 47));
    assert!(actual[14..].iter().all(|word| *word == (63, 31, 7)));
}
