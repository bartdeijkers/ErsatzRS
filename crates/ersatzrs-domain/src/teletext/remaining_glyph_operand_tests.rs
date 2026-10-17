//! Remaining scalar/semantic editor intents, not native glyph or client readiness.
use super::*;
use crate::teletext::TeletextG3Compatibility::{Level15, Level25};

fn alpha(code: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::G1Alpha {
        code: TeletextG1AlphaCode::new(code).unwrap(),
    }
}
fn g3(code: u8, compatibility: TeletextG3Compatibility) -> TeletextColumnOperation {
    TeletextColumnOperation::G3 {
        code: TeletextGlyphCode::new(code).unwrap(),
        compatibility,
    }
}
fn column(column: u8, operation: TeletextColumnOperation) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn { column, operation }
}
fn page_change(
    command: usize,
    column: u8,
    expected: TeletextColumnOperation,
    replacement: TeletextColumnOperation,
) -> TeletextLocalProgramOperandOverlay {
    TeletextLocalProgramOperandOverlay::new(address(200, 0), command, column, expected, replacement)
        .unwrap()
}
fn pairs() -> [(
    TeletextLocalProgramOperandOverlay,
    TeletextLocalObjectOperandOverlay,
); 3] {
    let identity = target(address(200, 0));
    [
        (
            page_change(4, 8, alpha(b'A'), alpha(b'B')),
            change(identity, 4, 0, alpha(b'A'), alpha(b'B')),
        ),
        (
            page_change(5, 9, g3(0x20, Level15), g3(0x21, Level15)),
            change(identity, 5, 1, g3(0x20, Level15), g3(0x21, Level15)),
        ),
        (
            page_change(6, 10, g3(0x21, Level25), g3(0x22, Level25)),
            change(identity, 7, 3, g3(0x21, Level25), g3(0x22, Level25)),
        ),
    ]
}
fn remaining_input() -> TeletextResolvedServiceDraft {
    let mut input = super::extended_operand_tests::extended_input();
    let mut page = input.pages[0]
        .local_program
        .as_ref()
        .unwrap()
        .commands()
        .to_vec();
    page[3] = column(8, alpha(b'A'));
    page[4] = column(9, g3(0x20, Level15));
    page.insert(3, column(8, attributes(false, false, 8)));
    page.push(column(10, g3(0x21, Level25)));
    input.pages[0].local_program = Some(program(page));
    let mut object = input.objects[0].program.commands().to_vec();
    object[3] = column(0, alpha(b'A'));
    object[4] = column(1, g3(0x20, Level15));
    object.insert(3, column(0, attributes(false, false, 8)));
    object.push(column(3, g3(0x21, Level25)));
    input.objects[0].program = program(object);
    input
}

#[test]
fn remaining_glyph_operands_preserve_checked_ranges_and_original_compatibility() {
    let owner = address(200, 0);
    let identity = target(owner);
    // G3's scalar boundary is not Table48 assignment or transmission readiness:
    // existing native validation still prohibits 6e/6f/7e/7f in either mode.
    for (expected, replacement) in [
        (alpha(0x40), alpha(0x5f)),
        (g3(0x20, Level15), g3(0x7f, Level15)),
        (g3(0x20, Level25), g3(0x7f, Level25)),
    ] {
        let page = page_change(usize::MAX, 39, expected, replacement);
        let object = change(identity, usize::MAX, 39, expected, replacement);
        assert_eq!(
            (page.owner(), page.command(), page.column()),
            (owner, usize::MAX, 39)
        );
        assert_eq!(
            (page.expected(), page.replacement()),
            (expected, replacement)
        );
        assert_eq!(
            (object.identity(), object.command(), object.column()),
            (identity, usize::MAX, 39)
        );
        assert_eq!(
            (object.expected(), object.replacement()),
            (expected, replacement)
        );
        assert_eq!(
            serde_json::from_value::<TeletextColumnOperation>(
                serde_json::to_value(replacement).unwrap()
            )
            .unwrap(),
            replacement
        );
    }
}

#[test]
fn remaining_glyph_operands_reject_out_of_range_codes_and_unknown_compatibility() {
    for code in [0x3f, 0x60] {
        assert!(TeletextG1AlphaCode::new(code).is_err());
        assert!(serde_json::from_value::<TeletextColumnOperation>(
            serde_json::json!({"kind":"g1_alpha","code":code})
        )
        .is_err());
    }
    for code in [0x1f, 0x80] {
        assert!(TeletextGlyphCode::new(code).is_err());
        for compatibility in ["level15", "level25"] {
            assert!(serde_json::from_value::<TeletextColumnOperation>(
                serde_json::json!({"kind":"g3","code":code,"compatibility":compatibility})
            )
            .is_err());
        }
    }
    assert!(serde_json::from_value::<TeletextColumnOperation>(
        serde_json::json!({"kind":"g3","code":32,"compatibility":"level35"})
    )
    .is_err());
}

fn reject_family(expected: TeletextColumnOperation, replacement: TeletextColumnOperation) {
    let owner = address(200, 0);
    assert_eq!(
        TeletextLocalProgramOperandOverlay::new(owner, 4, 8, expected, replacement),
        Err(TeletextLocalProgramOperandOverlayError::Family)
    );
    assert_eq!(
        TeletextLocalObjectOperandOverlay::new(target(owner), 4, 0, expected, replacement),
        Err(TeletextLocalObjectOperandOverlayError::Operand(
            TeletextLocalProgramOperandOverlayError::Family
        ))
    );
}

#[test]
fn remaining_glyph_operands_cannot_change_g3_mode_or_g1_alphabetic_family() {
    for (before, after) in [(Level15, Level25), (Level25, Level15)] {
        reject_family(g3(0x20, before), g3(0x20, after));
        reject_family(g3(0x20, before), g3(0x21, after));
    }
    for (expected, replacement) in [
        (alpha(b'A'), g1(1)),
        (g1(1), alpha(b'B')),
        (alpha(b'A'), g0(b'B')),
        (g0(b'A'), alpha(b'B')),
        (g3(0x20, Level15), foreground(2)),
    ] {
        reject_family(expected, replacement);
    }
    for (page, object) in pairs() {
        assert_eq!(
            TeletextLocalProgramOperandOverlay::new(
                page.owner(),
                page.command(),
                page.column(),
                page.expected(),
                page.expected()
            ),
            Err(TeletextLocalProgramOperandOverlayError::Unchanged)
        );
        assert_eq!(
            TeletextLocalObjectOperandOverlay::new(
                object.identity(),
                object.command(),
                object.column(),
                object.expected(),
                object.expected()
            ),
            Err(TeletextLocalObjectOperandOverlayError::Operand(
                TeletextLocalProgramOperandOverlayError::Unchanged
            ))
        );
        assert_eq!(
            TeletextLocalProgramOperandOverlay::new(
                page.owner(),
                page.command(),
                40,
                page.expected(),
                page.replacement()
            ),
            Err(TeletextLocalProgramOperandOverlayError::Column { column: 40 })
        );
        assert_eq!(
            TeletextLocalObjectOperandOverlay::new(
                object.identity(),
                object.command(),
                40,
                object.expected(),
                object.replacement()
            ),
            Err(TeletextLocalObjectOperandOverlayError::Operand(
                TeletextLocalProgramOperandOverlayError::Column { column: 40 }
            ))
        );
    }
}

#[test]
fn remaining_glyph_operands_rebuild_both_g3_modes_and_alpha_with_all_original_context() {
    let input = remaining_input();
    let original = service(input.clone());
    let page_changes = pairs().map(|(page, _)| page);
    let object_changes = pairs().map(|(_, object)| object);
    let edited = original
        .clone()
        .with_local_program_operand_overlays(&page_changes, budget())
        .unwrap()
        .with_local_object_operand_overlays(&object_changes, budget())
        .unwrap();
    let reverse = original
        .clone()
        .with_local_object_operand_overlays(&object_changes, budget())
        .unwrap()
        .with_local_program_operand_overlays(&page_changes, budget())
        .unwrap();
    assert_eq!(edited, reverse);
    let mut expected = input;
    let mut page = expected.pages[0]
        .local_program
        .as_ref()
        .unwrap()
        .commands()
        .to_vec();
    page[4] = column(8, alpha(b'B'));
    page[5] = column(9, g3(0x21, Level15));
    page[6] = column(10, g3(0x22, Level25));
    expected.pages[0].local_program = Some(program(page));
    let mut object = expected.objects[0].program.commands().to_vec();
    object[4] = column(0, alpha(b'B'));
    object[5] = column(1, g3(0x21, Level15));
    object[7] = column(3, g3(0x22, Level25));
    expected.objects[0].program = program(object);
    // Whole-model equality retains current G0/D5, literal charset aliases,
    // original MOT fallback, Base/nav/physical rows, palettes and POP/DRCS sources.
    assert_eq!(edited, service(expected));
    assert_ne!(edited, original);
    assert_eq!(
        edited.collected(address(200, 0)),
        original.collected(address(200, 0))
    );
    assert_eq!(
        edited.effective_program(address(200, 1)).unwrap(),
        original.effective_program(address(200, 1)).unwrap()
    );
}

#[test]
fn remaining_glyph_operands_retain_stale_duplicate_and_raw_collector_admission_gates() {
    let original = service(remaining_input());
    for (page, object) in pairs() {
        assert_eq!(
            original
                .clone()
                .with_local_program_operand_overlays(&[page, page], budget()),
            Err(TeletextServiceError::LocalProgramOperandOverlay {
                address: page.owner(),
                command: page.command(),
                reason: TeletextLocalProgramOperandFailure::Duplicate
            })
        );
        assert_eq!(
            original
                .clone()
                .with_local_object_operand_overlays(&[object, object], budget()),
            Err(TeletextServiceError::LocalObjectOperandOverlay {
                identity: object.identity(),
                command: object.command(),
                reason: TeletextLocalObjectOperandFailure::Duplicate
            })
        );
        let page_edited = original
            .clone()
            .with_local_program_operand_overlays(&[page], budget())
            .unwrap();
        let object_edited = original
            .clone()
            .with_local_object_operand_overlays(&[object], budget())
            .unwrap();
        assert_eq!(
            page_edited.with_local_program_operand_overlays(&[page], budget()),
            Err(TeletextServiceError::LocalProgramOperandOverlay {
                address: page.owner(),
                command: page.command(),
                reason: TeletextLocalProgramOperandFailure::CommandMismatch
            })
        );
        assert_eq!(
            object_edited.with_local_object_operand_overlays(&[object], budget()),
            Err(TeletextServiceError::LocalObjectOperandOverlay {
                identity: object.identity(),
                command: object.command(),
                reason: TeletextLocalObjectOperandFailure::CommandMismatch
            })
        );
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
                matches!(original.clone().with_local_program_operand_overlays(&[page,page],limit),Err(TeletextServiceError::Budget {dimension:actual,..}) if actual==dimension)
            );
            assert!(
                matches!(original.clone().with_local_object_operand_overlays(&[object,object],limit),Err(TeletextServiceError::Budget {dimension:actual,..}) if actual==dimension)
            );
        }
    }
}
