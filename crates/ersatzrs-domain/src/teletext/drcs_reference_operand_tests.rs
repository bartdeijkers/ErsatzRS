//! Exact original DRCS slot intentions, not packet or external-editor evidence.
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
fn page_change(
    table: TeletextDrcsTableScope,
    replacement: u8,
) -> TeletextLocalProgramOperandOverlay {
    TeletextLocalProgramOperandOverlay::new(
        address(200, 0),
        5,
        10,
        reference(table, 15, 0),
        reference(table, 15, replacement),
    )
    .unwrap()
}
fn object_change(
    table: TeletextDrcsTableScope,
    replacement: u8,
) -> TeletextLocalObjectOperandOverlay {
    change(
        target(address(200, 0)),
        5,
        2,
        reference(table, 15, 0),
        reference(table, 15, replacement),
    )
}
fn selected_resource(table: TeletextDrcsTableScope) -> TeletextResourceIdentity {
    match table {
        Normal => resource(TeletextResourceKind::Drcs, 0xb0, 15),
        Global => resource(TeletextResourceKind::Gdrcs, 0xb1, 15),
    }
}
fn slot_input(table: TeletextDrcsTableScope) -> TeletextResolvedServiceDraft {
    let mut input = super::extended_operand_tests::extended_input();
    let mut page = input.pages[0]
        .local_program
        .as_ref()
        .unwrap()
        .commands()
        .to_vec();
    page.push(column(10, reference(table, 15, 0)));
    page.push(TeletextEnhancementCommand::InvokeObject {
        target: target(address(200, 0)),
        object_type: TeletextObjectType::Passive,
        offset: Some(TeletextObjectOffset::new(0, 33).unwrap()),
    });
    input.pages[0].local_program = Some(program(page));
    let mut object = input.objects[0].program.commands().to_vec();
    object[5] = column(2, reference(table, 15, 0));
    input.objects[0].program = program(object);
    input.pages[0].resources.drcs = TeletextDrcsPageLinks::new(
        Some(raw(0xb0)),
        if table == Global {
            Some(raw(0xb1))
        } else {
            None
        },
    )
    .unwrap();
    input.drcs.push(
        TeletextDrcsSubtable::new(
            selected_resource(table),
            [(0, 0x111), (47, 0x801)]
                .map(|(slot, pixels)| TeletextDrcsGlyphDefinition {
                    slot: TeletextDrcsGlyphSlot::new(slot).unwrap(),
                    bitmap: TeletextDrcsBitmap::new([pixels; 10]).unwrap(),
                })
                .to_vec(),
        )
        .unwrap(),
    );
    input
}

#[test]
fn drcs_slot_operands_preserve_typed_scope_subtable_and_boundary_slots() {
    for table in [Normal, Global] {
        for subtable in [0, 15] {
            let before = reference(table, subtable, 0);
            let after = reference(table, subtable, 47);
            let page = TeletextLocalProgramOperandOverlay::new(
                address(200, 0),
                usize::MAX,
                39,
                before,
                after,
            )
            .unwrap();
            let object = change(target(address(200, 0)), usize::MAX, 39, before, after);
            assert_eq!((page.expected(), page.replacement()), (before, after));
            assert_eq!((object.expected(), object.replacement()), (before, after));
            assert_eq!(
                (object.owner(), object.command(), object.column()),
                (address(200, 0), usize::MAX, 39)
            );
            assert_eq!(
                serde_json::from_value::<TeletextColumnOperation>(
                    serde_json::to_value(after).unwrap()
                )
                .unwrap(),
                after
            );
        }
    }
    for slot in [48, 255] {
        assert!(TeletextDrcsGlyphSlot::new(slot).is_err());
        assert!(
            serde_json::from_value::<TeletextColumnOperation>(serde_json::json!({
                "kind":"drcs", "reference":{"table":"normal","subtable":0,"slot":slot}
            }))
            .is_err()
        );
    }
    assert!(TeletextDrcsSubtableIndex::new(16).is_err());
}

#[test]
fn drcs_operands_reject_cross_family_noop_and_bad_column_changes() {
    for (before, after) in [
        (reference(Normal, 0, 0), g0(b'A')),
        (g0(b'A'), reference(Normal, 0, 47)),
    ] {
        assert_eq!(
            TeletextLocalProgramOperandOverlay::new(address(200, 0), 5, 10, before, after),
            Err(TeletextLocalProgramOperandOverlayError::Family)
        );
        assert_eq!(
            TeletextLocalObjectOperandOverlay::new(target(address(200, 0)), 5, 2, before, after),
            Err(TeletextLocalObjectOperandOverlayError::Operand(
                TeletextLocalProgramOperandOverlayError::Family
            ))
        );
    }
    for table in [Normal, Global] {
        let before = reference(table, 15, 0);
        let after = reference(table, 15, 47);
        assert_eq!(
            TeletextLocalProgramOperandOverlay::new(address(200, 0), 5, 10, before, before),
            Err(TeletextLocalProgramOperandOverlayError::Unchanged)
        );
        assert_eq!(
            TeletextLocalObjectOperandOverlay::new(target(address(200, 0)), 5, 2, before, before),
            Err(TeletextLocalObjectOperandOverlayError::Operand(
                TeletextLocalProgramOperandOverlayError::Unchanged
            ))
        );
        assert_eq!(
            TeletextLocalProgramOperandOverlay::new(address(200, 0), 5, 40, before, after),
            Err(TeletextLocalProgramOperandOverlayError::Column { column: 40 })
        );
        assert_eq!(
            TeletextLocalObjectOperandOverlay::new(target(address(200, 0)), 5, 40, before, after),
            Err(TeletextLocalObjectOperandOverlayError::Operand(
                TeletextLocalProgramOperandOverlayError::Column { column: 40 }
            ))
        );
        for identity in [public(0xa0, 0, 65520), global(0xa1, 65520)] {
            assert_eq!(
                TeletextLocalObjectOperandOverlay::new(identity, 5, 2, before, after),
                Err(TeletextLocalObjectOperandOverlayError::Scope)
            );
        }
    }
}

#[test]
fn drcs_slot_operands_rebuild_page_and_repeated_local_uses_without_resource_mutation() {
    for table in [Normal, Global] {
        let input = slot_input(table);
        let original = service(input.clone());
        let page = page_change(table, 47);
        let object = object_change(table, 47);
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
        commands[5] = column(10, reference(table, 15, 47));
        expected.pages[0].local_program = Some(program(commands));
        let mut commands = expected.objects[0].program.commands().to_vec();
        commands[5] = column(2, reference(table, 15, 47));
        expected.objects[0].program = program(commands);
        // Full equality retains original resource identities/order/bitmaps, charset,
        // base/nav/physical rows, panels, MOT fallback and unrelated POP sources.
        assert_eq!(edited, service(expected));
        assert_eq!(edited.drcs(), original.drcs());
        assert_eq!(edited.object_companions(), original.object_companions());
        assert_eq!(
            edited.effective_program(address(200, 1)).unwrap(),
            original.effective_program(address(200, 1)).unwrap()
        );
        let (_, before) = original.collected(address(200, 0)).unwrap();
        let (_, after) = edited.collected(address(200, 0)).unwrap();
        assert_eq!(after.sources(), before.sources());
        assert_eq!(after.object_references(), before.object_references());
        assert_eq!(after.references().len(), 3);
        for value in after.references() {
            assert_eq!(
                *value,
                TeletextDrcsGlyphReference {
                    table,
                    subtable: TeletextDrcsSubtableIndex::new(15).unwrap(),
                    slot: TeletextDrcsGlyphSlot::new(47).unwrap(),
                }
            );
        }
    }
}

fn assert_missing(
    error: &TeletextServiceError,
    table: TeletextDrcsTableScope,
    source: TeletextEnhancementSource,
) {
    assert!(matches!(*error, TeletextServiceError::CollectedDrcs {
        program_source:TeletextEffectiveProgramSource::Local,
        error:TeletextCollectedDrcsError {source_location:Some(actual),error:TeletextDrcsResourceError::MissingGlyph {display, identity, slot,..}}
    } if actual==source && display==address(200,0) && identity==selected_resource(table) && slot.get()==1));
}

#[test]
fn drcs_slot_operands_require_real_target_glyph_with_exact_collected_source() {
    for table in [Normal, Global] {
        let original = service(slot_input(table));
        assert_missing(
            &original
                .clone()
                .with_local_program_operand_overlays(&[page_change(table, 1)], budget())
                .unwrap_err(),
            table,
            TeletextEnhancementSource {
                object: None,
                command: 5,
            },
        );
        assert_missing(
            &original
                .clone()
                .with_local_object_operand_overlays(&[object_change(table, 1)], budget())
                .unwrap_err(),
            table,
            TeletextEnhancementSource {
                object: Some(target(address(200, 0))),
                command: 5,
            },
        );
        // Neither error alters the retained original; its valid existing-slot edit
        // still resolves without inventing a bank or changing a bitmap.
        assert!(original
            .with_local_object_operand_overlays(&[object_change(table, 47)], budget())
            .is_ok());
    }
}

#[test]
fn drcs_slot_operands_keep_exact_original_stale_and_duplicate_guards() {
    for table in [Normal, Global] {
        let original = service(slot_input(table));
        let page = page_change(table, 47);
        let object = object_change(table, 47);
        assert!(matches!(
            original
                .clone()
                .with_local_program_operand_overlays(&[page, page], budget()),
            Err(TeletextServiceError::LocalProgramOperandOverlay {
                reason: TeletextLocalProgramOperandFailure::Duplicate,
                ..
            })
        ));
        assert!(matches!(
            original
                .clone()
                .with_local_object_operand_overlays(&[object, object], budget()),
            Err(TeletextServiceError::LocalObjectOperandOverlay {
                reason: TeletextLocalObjectOperandFailure::Duplicate,
                ..
            })
        ));
        assert!(matches!(
            original
                .clone()
                .with_local_program_operand_overlays(&[page], budget())
                .unwrap()
                .with_local_program_operand_overlays(&[page], budget()),
            Err(TeletextServiceError::LocalProgramOperandOverlay {
                reason: TeletextLocalProgramOperandFailure::CommandMismatch,
                ..
            })
        ));
        assert!(matches!(
            original
                .with_local_object_operand_overlays(&[object], budget())
                .unwrap()
                .with_local_object_operand_overlays(&[object], budget()),
            Err(TeletextServiceError::LocalObjectOperandOverlay {
                reason: TeletextLocalObjectOperandFailure::CommandMismatch,
                ..
            })
        ));
    }
}

#[test]
fn drcs_slot_operands_reuse_original_raw_and_collected_admission_budgets() {
    for table in [Normal, Global] {
        let original = service(slot_input(table));
        let page = page_change(table, 47);
        let object = object_change(table, 47);
        for dimension in [
            "authored command",
            "collected command",
            "collected DRCS reference",
        ] {
            let mut limit = budget();
            match dimension {
                "authored command" => limit.max_commands = 1,
                "collected command" => limit.max_collected_commands = 0,
                _ => limit.max_collected_drcs_references = 0,
            }
            assert!(
                matches!(original.clone().with_local_program_operand_overlays(&[page,page],limit), Err(TeletextServiceError::Budget {dimension:actual,..}) if actual==dimension)
            );
            assert!(
                matches!(original.clone().with_local_object_operand_overlays(&[object,object],limit), Err(TeletextServiceError::Budget {dimension:actual,..}) if actual==dimension)
            );
        }
    }
}
