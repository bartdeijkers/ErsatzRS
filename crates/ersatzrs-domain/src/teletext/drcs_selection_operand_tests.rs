//! Exact reference selections; resource/usage qualification stays in service rebuild.
use super::*;
use crate::teletext::TeletextDrcsTableScope::{Global, Normal};

fn reference(table: TeletextDrcsTableScope, subtable: u8, slot: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::Drcs {
        reference: TeletextDrcsGlyphReference {
            table,
            subtable: TeletextDrcsSubtableIndex::new(subtable).unwrap(),
            slot: TeletextDrcsGlyphSlot::new(slot).unwrap(),
        },
    }
}
fn column(column: u8, operation: TeletextColumnOperation) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn { column, operation }
}
fn page_change(after: TeletextColumnOperation) -> TeletextLocalProgramOperandOverlay {
    TeletextLocalProgramOperandOverlay::new(address(200, 0), 5, 10, reference(Normal, 0, 0), after)
        .unwrap()
}
fn object_change(after: TeletextColumnOperation) -> TeletextLocalObjectOperandOverlay {
    change(
        target(address(200, 0)),
        5,
        2,
        reference(Normal, 0, 0),
        after,
    )
}
fn selected(table: TeletextDrcsTableScope, subtable: u8) -> TeletextResourceIdentity {
    resource(
        if table == Normal {
            TeletextResourceKind::Drcs
        } else {
            TeletextResourceKind::Gdrcs
        },
        if table == Normal { 0xb0 } else { 0xb1 },
        subtable,
    )
}
fn bank(
    table: TeletextDrcsTableScope,
    subtable: u8,
    slots: impl IntoIterator<Item = u8>,
) -> TeletextDrcsSubtable {
    TeletextDrcsSubtable::new(
        selected(table, subtable),
        slots
            .into_iter()
            .map(|slot| TeletextDrcsGlyphDefinition {
                slot: TeletextDrcsGlyphSlot::new(slot).unwrap(),
                bitmap: TeletextDrcsBitmap::new([if slot == 0 { 0x111 } else { 0x801 }; 10])
                    .unwrap(),
            })
            .collect(),
    )
    .unwrap()
}
fn selection_input() -> TeletextResolvedServiceDraft {
    let mut input = super::extended_operand_tests::extended_input();
    let mut page = input.pages[0]
        .local_program
        .as_ref()
        .unwrap()
        .commands()
        .to_vec();
    page.push(column(10, reference(Normal, 0, 0)));
    page.push(TeletextEnhancementCommand::InvokeObject {
        target: target(address(200, 0)),
        object_type: TeletextObjectType::Passive,
        offset: Some(TeletextObjectOffset::new(0, 33).unwrap()),
    });
    input.pages[0].local_program = Some(program(page));
    input.pages[0].resources.drcs =
        TeletextDrcsPageLinks::new(Some(raw(0xb0)), Some(raw(0xb1))).unwrap();
    input.drcs = [Normal, Global]
        .into_iter()
        .flat_map(|table| [0, 15].map(|subtable| bank(table, subtable, [0, 47])))
        .collect();
    input
}
#[test]
fn drcs_selection_value_preserves_all_target_subtables_scopes_and_slot_boundaries() {
    for before_table in [Normal, Global] {
        for after_table in [Normal, Global] {
            for before_subtable in [0, 15] {
                for after_subtable in 0..=15 {
                    for before_slot in [0, 47] {
                        for after_slot in [0, 47] {
                            let before = reference(before_table, before_subtable, before_slot);
                            let after = reference(after_table, after_subtable, after_slot);
                            let page = TeletextLocalProgramOperandOverlay::new(
                                address(200, 0),
                                usize::MAX,
                                39,
                                before,
                                after,
                            );
                            let object = TeletextLocalObjectOperandOverlay::new(
                                target(address(200, 0)),
                                usize::MAX,
                                39,
                                before,
                                after,
                            );
                            if before == after {
                                assert_eq!(
                                    page,
                                    Err(TeletextLocalProgramOperandOverlayError::Unchanged)
                                );
                                assert_eq!(
                                    object,
                                    Err(TeletextLocalObjectOperandOverlayError::Operand(
                                        TeletextLocalProgramOperandOverlayError::Unchanged
                                    ))
                                );
                            } else {
                                let page = page.unwrap();
                                let object = object.unwrap();
                                assert_eq!((page.expected(), page.replacement()), (before, after));
                                assert_eq!(
                                    (object.expected(), object.replacement()),
                                    (before, after)
                                );
                                assert_eq!(
                                    (object.owner(), object.command(), object.column()),
                                    (address(200, 0), usize::MAX, 39)
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
#[test]
fn drcs_selection_rebuilds_page_and_repeated_local_uses_without_touching_catalogs() {
    for table in [Normal, Global] {
        for subtable in [0, 15] {
            for slot in [0, 47] {
                let after = reference(table, subtable, slot);
                if after == reference(Normal, 0, 0) {
                    continue;
                }
                let input = selection_input();
                let original = service(input.clone());
                let page = page_change(after);
                let object = object_change(after);
                let edited = original
                    .clone()
                    .with_local_program_operand_overlays(&[page], budget())
                    .unwrap()
                    .with_local_object_operand_overlays(&[object], budget())
                    .unwrap();
                let reverse = original
                    .clone()
                    .with_local_object_operand_overlays(&[object], budget())
                    .unwrap()
                    .with_local_program_operand_overlays(&[page], budget())
                    .unwrap();
                assert_eq!(edited, reverse);
                let mut expected = input;
                let mut commands = expected.pages[0]
                    .local_program
                    .as_ref()
                    .unwrap()
                    .commands()
                    .to_vec();
                commands[5] = column(10, after);
                expected.pages[0].local_program = Some(program(commands));
                let mut commands = expected.objects[0].program.commands().to_vec();
                commands[5] = column(2, after);
                expected.objects[0].program = program(commands);
                assert_eq!(edited, service(expected));
                assert_eq!(edited.drcs(), original.drcs());
                assert_eq!(edited.object_companions(), original.object_companions());
                assert_eq!(
                    edited.effective_program(address(200, 1)).unwrap(),
                    original.effective_program(address(200, 1)).unwrap()
                );
                let (_, references) = edited.collected(address(200, 0)).unwrap();
                let TeletextColumnOperation::Drcs { reference } = after else {
                    unreachable!()
                };
                assert_eq!(references.references(), [reference; 3].as_slice());
            }
        }
    }
}
fn missing_case(case: u8) -> (TeletextResolvedServiceDraft, TeletextColumnOperation) {
    let mut input = selection_input();
    match case {
        0 => {
            input.pages[0].resources.drcs =
                TeletextDrcsPageLinks::new(Some(raw(0xb0)), None).unwrap();
            (input, reference(Global, 15, 47))
        }
        1 => {
            input
                .drcs
                .retain(|bank| bank.identity() != selected(Normal, 15));
            (input, reference(Normal, 15, 47))
        }
        _ => (input, reference(Global, 15, 1)),
    }
}
fn assert_missing(error: TeletextServiceError, case: u8, source: TeletextEnhancementSource) {
    let TeletextServiceError::CollectedDrcs { error, .. } = error else {
        panic!("selected reference must fail at collection")
    };
    assert_eq!(error.source_location, Some(source));
    match (case, error.error) {
        (
            0,
            TeletextDrcsResourceError::MissingLink {
                display,
                table: Global,
                ..
            },
        ) => assert_eq!(display, address(200, 0)),
        (
            1,
            TeletextDrcsResourceError::MissingSubtable {
                display, identity, ..
            },
        ) => {
            assert_eq!(display, address(200, 0));
            assert_eq!(identity, selected(Normal, 15));
        }
        (
            2,
            TeletextDrcsResourceError::MissingGlyph {
                display,
                identity,
                slot,
                ..
            },
        ) => {
            assert_eq!(display, address(200, 0));
            assert_eq!(identity, selected(Global, 15));
            assert_eq!(slot.get(), 1);
        }
        _ => panic!("missing selection must preserve the exact typed resource failure"),
    }
}
#[test]
fn drcs_selection_requires_link_subtable_and_glyph_at_each_transitive_source() {
    for case in 0..3 {
        let (input, after) = missing_case(case);
        let original = service(input);
        let before = original.clone();
        assert_missing(
            original
                .clone()
                .with_local_program_operand_overlays(&[page_change(after)], budget())
                .unwrap_err(),
            case,
            TeletextEnhancementSource {
                object: None,
                command: 5,
            },
        );
        assert_missing(
            original
                .clone()
                .with_local_object_operand_overlays(&[object_change(after)], budget())
                .unwrap_err(),
            case,
            TeletextEnhancementSource {
                object: Some(target(address(200, 0))),
                command: 5,
            },
        );
        assert_eq!(original, before);
    }
}
#[test]
fn drcs_selection_keeps_two_subtable_limit_after_repeated_local_expansion() {
    let mut input = selection_input();
    let mut commands = input.pages[0]
        .local_program
        .as_ref()
        .unwrap()
        .commands()
        .to_vec();
    commands.push(column(11, reference(Global, 0, 0)));
    input.pages[0].local_program = Some(program(commands));
    let original = service(input);
    let before = original.clone();
    assert!(
        matches!(original.clone().with_local_object_operand_overlays(&[object_change(reference(Global,15,47))],budget()),
        Err(TeletextServiceError::CollectedDrcs {error:TeletextCollectedDrcsError {
            source_location:Some(TeletextEnhancementSource {object:None,command:7}),
            error:TeletextDrcsResourceError::DisplaySubtables {display,..}},..}) if display==address(200,0))
    );
    assert_eq!(original, before);
}
#[test]
fn drcs_selection_keeps_twenty_four_distinct_glyph_limit_including_original_uses() {
    let mut input = selection_input();
    input.drcs[0] = bank(Normal, 0, 0..24);
    let mut commands = input.pages[0]
        .local_program
        .as_ref()
        .unwrap()
        .commands()
        .to_vec();
    for slot in 1..24 {
        commands.push(column(10 + slot, reference(Normal, 0, slot)));
    }
    input.pages[0].local_program = Some(program(commands));
    let original = service(input);
    let before = original.clone();
    assert!(
        matches!(original.clone().with_local_object_operand_overlays(&[object_change(reference(Global,15,47))],budget()),
        Err(TeletextServiceError::CollectedDrcs {error:TeletextCollectedDrcsError {
            source_location:Some(TeletextEnhancementSource {object:None,command:29}),
            error:TeletextDrcsResourceError::DisplayGlyphs {display,..}},..}) if display==address(200,0))
    );
    assert_eq!(original, before);
}
