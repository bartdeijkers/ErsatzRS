//! Exact local-operand transformation, without editor permission or packet proof.
use super::*;

fn foreground(colour: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::Foreground {
        colour: TeletextColourReference::new(colour).unwrap(),
    }
}

fn background(colour: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::Background {
        colour: TeletextColourReference::new(colour).unwrap(),
    }
}

fn attributes(invert: bool) -> TeletextColumnOperation {
    TeletextColumnOperation::Attributes {
        double_height: false,
        double_width: false,
        boxed: false,
        conceal: false,
        invert,
        underline_or_separated_mosaics: false,
    }
}

fn change(
    owner: TeletextPageAddress,
    command: usize,
    original: TeletextColumnOperation,
    replacement: TeletextColumnOperation,
) -> TeletextLocalProgramOperandOverlay {
    TeletextLocalProgramOperandOverlay::new(owner, command, 3, original, replacement).unwrap()
}

fn editable_input() -> TeletextResolvedServiceDraft {
    let mut input = draft(&[address(200, 0), address(200, 1)]);
    input.pages[0].local_program = Some(program(vec![
        TeletextEnhancementCommand::SetPosition {
            position: TeletextEnhancementPosition::new(4, 0).unwrap(),
        },
        TeletextEnhancementCommand::AtColumn {
            column: 3,
            operation: foreground(1),
        },
        TeletextEnhancementCommand::AtColumn {
            column: 3,
            operation: background(0),
        },
        TeletextEnhancementCommand::AtColumn {
            column: 3,
            operation: attributes(false),
        },
        TeletextEnhancementCommand::AtColumn {
            column: 3,
            operation: TeletextColumnOperation::G0 {
                code: TeletextGlyphCode::new(b'A').unwrap(),
            },
        },
    ]));
    input
}

#[test]
fn local_operand_value_rejects_noop_cross_family_glyph_charset_and_invalid_column() {
    let owner = address(200, 0);
    for (expected, replacement) in [
        (foreground(1), background(2)),
        (foreground(1), attributes(true)),
        (
            TeletextColumnOperation::G0 {
                code: TeletextGlyphCode::new(b'A').unwrap(),
            },
            TeletextColumnOperation::G0 {
                code: TeletextGlyphCode::new(b'B').unwrap(),
            },
        ),
        (
            TeletextColumnOperation::DesignateCharset {
                designation: TeletextEnhancementCharsetDesignation::new(0).unwrap(),
            },
            TeletextColumnOperation::DesignateCharset {
                designation: TeletextEnhancementCharsetDesignation::new(1).unwrap(),
            },
        ),
    ] {
        assert_eq!(
            TeletextLocalProgramOperandOverlay::new(owner, 1, 3, expected, replacement),
            Err(TeletextLocalProgramOperandOverlayError::Family)
        );
    }
    assert_eq!(
        TeletextLocalProgramOperandOverlay::new(owner, 1, 3, foreground(1), foreground(1)),
        Err(TeletextLocalProgramOperandOverlayError::Unchanged)
    );
    assert_eq!(
        TeletextLocalProgramOperandOverlay::new(owner, 1, 40, foreground(1), foreground(2)),
        Err(TeletextLocalProgramOperandOverlayError::Column { column: 40 })
    );
    for (double_height, double_width) in [(true, false), (false, true), (true, true)] {
        let replacement = TeletextColumnOperation::Attributes {
            double_height,
            double_width,
            boxed: false,
            conceal: false,
            invert: true,
            underline_or_separated_mosaics: false,
        };
        assert_eq!(
            TeletextLocalProgramOperandOverlay::new(owner, 3, 3, attributes(false), replacement),
            Err(TeletextLocalProgramOperandOverlayError::Geometry)
        );
    }
}

#[test]
fn local_operands_rebuild_exact_candidate_without_changing_other_semantic_fields() {
    let owner = address(200, 0);
    let sibling = address(200, 1);
    let mut input = editable_input();
    input.pages[0].presentation = Some(presentation(0x37, 1));
    input.magazines.push(TeletextMagazinePresentationDraft {
        magazine: 3,
        presentation: presentation(0x20, 2),
    });
    input.pages[0].resources.objects.public = Some(raw(0xa0));
    input.pages[0].resources.drcs = TeletextDrcsPageLinks::new(Some(raw(0xb0)), None).unwrap();
    input.drcs.push(table(0xb0, 0, 0x111));
    input.objects.push(definition(
        public(0xa0, 1, 1),
        TeletextObjectType::Passive,
        vec![],
    ));
    let empty = resource(TeletextResourceKind::Pop, 0xa0, 0);
    let mut original =
        TeletextSemanticService::new_with_object_companions(input.clone(), vec![empty], budget())
            .unwrap();
    original.set_mot_fallback(sibling, presentation(0, 3));
    let physical = TeletextPhysicalBaseOverlay::new(
        owner,
        vec![TeletextPhysicalBaseRow::new(6, [b'P'; 40]).unwrap()],
    )
    .unwrap();
    original = original
        .with_physical_base_overlays(vec![physical.clone()], budget())
        .unwrap();
    let overlays = [
        change(owner, 1, foreground(1), foreground(31)),
        change(owner, 2, background(0), background(16)),
        change(owner, 3, attributes(false), attributes(true)),
    ];
    let changed = original
        .clone()
        .with_local_program_operand_overlays(&overlays, budget())
        .unwrap();
    let mut commands = input.pages[0]
        .local_program
        .as_ref()
        .unwrap()
        .commands()
        .to_vec();
    commands[1] = TeletextEnhancementCommand::AtColumn {
        column: 3,
        operation: foreground(31),
    };
    commands[2] = TeletextEnhancementCommand::AtColumn {
        column: 3,
        operation: background(16),
    };
    commands[3] = TeletextEnhancementCommand::AtColumn {
        column: 3,
        operation: attributes(true),
    };
    input.pages[0].local_program = Some(program(commands));
    let mut expected =
        TeletextSemanticService::new_with_object_companions(input, vec![empty], budget()).unwrap();
    expected.set_mot_fallback(sibling, presentation(0, 3));
    expected = expected
        .with_physical_base_overlays(vec![physical], budget())
        .unwrap();
    assert_eq!(changed, expected);
    assert_ne!(changed, original);
    assert_eq!(
        changed.effective_program(sibling).unwrap(),
        original.effective_program(sibling).unwrap()
    );
    assert_eq!(changed.collected(owner), original.collected(owner));
}

#[test]
fn local_operands_reject_duplicate_absent_foreign_stale_and_nonlocal_commands() {
    let owner = address(200, 0);
    let original = TeletextSemanticService::new(editable_input(), budget()).unwrap();
    let valid = change(owner, 1, foreground(1), foreground(2));
    assert_eq!(
        original
            .clone()
            .with_local_program_operand_overlays(&[], budget()),
        Err(TeletextServiceError::EmptyLocalProgramOperandOverlays)
    );
    for (overlays, address, command, reason) in [
        (
            vec![valid, valid],
            owner,
            1,
            TeletextLocalProgramOperandFailure::Duplicate,
        ),
        (
            vec![change(address(200, 2), 1, foreground(1), foreground(2))],
            address(200, 2),
            1,
            TeletextLocalProgramOperandFailure::MissingLocalProgram,
        ),
        (
            vec![change(address(200, 1), 1, foreground(1), foreground(2))],
            address(200, 1),
            1,
            TeletextLocalProgramOperandFailure::MissingLocalProgram,
        ),
        (
            vec![change(owner, 0, foreground(1), foreground(2))],
            owner,
            0,
            TeletextLocalProgramOperandFailure::CommandMismatch,
        ),
        (
            vec![change(owner, usize::MAX, foreground(1), foreground(2))],
            owner,
            usize::MAX,
            TeletextLocalProgramOperandFailure::CommandMismatch,
        ),
        (
            vec![change(owner, 1, foreground(3), foreground(2))],
            owner,
            1,
            TeletextLocalProgramOperandFailure::CommandMismatch,
        ),
        (
            vec![TeletextLocalProgramOperandOverlay::new(
                owner,
                1,
                4,
                foreground(1),
                foreground(2),
            )
            .unwrap()],
            owner,
            1,
            TeletextLocalProgramOperandFailure::CommandMismatch,
        ),
    ] {
        assert_eq!(
            original
                .clone()
                .with_local_program_operand_overlays(&overlays, budget()),
            Err(TeletextServiceError::LocalProgramOperandOverlay {
                address,
                command,
                reason
            })
        );
    }
    let changed = original
        .with_local_program_operand_overlays(&[valid], budget())
        .unwrap();
    assert!(matches!(
        changed.with_local_program_operand_overlays(&[valid], budget()),
        Err(TeletextServiceError::LocalProgramOperandOverlay {
            reason: TeletextLocalProgramOperandFailure::CommandMismatch,
            ..
        })
    ));
}

#[test]
fn local_operand_preflight_retains_resource_structure_physical_content_and_collector_budgets() {
    let owner = address(200, 0);
    let mut input = editable_input();
    input.objects.push(definition(
        public(0xa0, 0, 1),
        TeletextObjectType::Passive,
        vec![],
    ));
    input.pages[0].resources.objects.public = Some(raw(0xa0));
    let original = TeletextSemanticService::new(input, budget())
        .unwrap()
        .with_physical_base_overlays(
            vec![TeletextPhysicalBaseOverlay::new(
                owner,
                vec![TeletextPhysicalBaseRow::new(6, [b'P'; 40]).unwrap()],
            )
            .unwrap()],
            budget(),
        )
        .unwrap();
    let valid = change(owner, 1, foreground(1), foreground(2));
    for dimension in [
        "record",
        "object",
        "structure item",
        "base content byte",
        "authored command",
        "collected command",
    ] {
        let mut limit = budget();
        match dimension {
            "record" => limit.max_records = 2, // Two display records fit; POP does not.
            "object" => limit.max_objects = 0,
            "structure item" => limit.max_structure_items = 5, // Four owners/metadata + two overlay items.
            "base content byte" => limit.max_base_content_bytes = 39,
            "authored command" => limit.max_commands = 4,
            _ => limit.max_collected_commands = 4,
        }
        assert!(
            matches!(original.clone().with_local_program_operand_overlays(&[valid], limit),
            Err(TeletextServiceError::Budget { dimension: actual, .. }) if actual == dimension),
            "{dimension}"
        );
    }
    let mut exact = budget();
    exact.max_records = 3;
    exact.max_objects = 1;
    exact.max_structure_items = 6;
    exact.max_base_content_bytes = 40;
    exact.max_commands = 5;
    exact.max_collected_commands = 5;
    exact.max_collected_drcs_references = 0;
    assert!(original
        .with_local_program_operand_overlays(&[valid], exact)
        .is_ok());
}
