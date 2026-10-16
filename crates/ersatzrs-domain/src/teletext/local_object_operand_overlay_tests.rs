//! Retained local definition transformations, not packet/editor or rendering proof.
use super::*;

fn g0(code: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::G0 {
        code: TeletextGlyphCode::new(code).unwrap(),
    }
}

fn g1(bits: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::G1 {
        mosaic: TeletextMosaic::new(bits).unwrap(),
    }
}

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
fn attributes(double_height: bool, double_width: bool, mask: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::Attributes {
        double_height,
        double_width,
        boxed: mask & 1 != 0,
        conceal: mask & 2 != 0,
        invert: mask & 4 != 0,
        underline_or_separated_mosaics: mask & 8 != 0,
    }
}
fn attribute_input(double_height: bool, double_width: bool) -> TeletextResolvedServiceDraft {
    let mut input = input();
    let mut commands = input.objects[0].program.commands().to_vec();
    commands.push(TeletextEnhancementCommand::AtColumn {
        column: 0,
        operation: attributes(double_height, double_width, 0),
    });
    input.objects[0].program = program(commands);
    input
}

fn target(owner: TeletextPageAddress) -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(owner, TeletextObjectId::new(65520))
}

fn change(
    identity: TeletextObjectIdentity,
    command: usize,
    column: u8,
    expected: TeletextColumnOperation,
    replacement: TeletextColumnOperation,
) -> TeletextLocalObjectOperandOverlay {
    TeletextLocalObjectOperandOverlay::new(identity, command, column, expected, replacement)
        .unwrap()
}

fn input() -> TeletextResolvedServiceDraft {
    let owner = address(200, 0);
    let mut input = draft(&[owner, address(200, 1)]);
    let mut defaults = presentation(0, 1);
    defaults.panels = TeletextSidePanels {
        left: true,
        right: true,
        status: TeletextPanelStatus::Level25And35,
        width: TeletextPanelWidth::new(4).unwrap(),
    };
    input.pages[0].presentation = Some(defaults);
    input.pages[0].local_program = Some(program(vec![
        TeletextEnhancementCommand::SetPosition {
            position: TeletextEnhancementPosition::new(1, 8).unwrap(),
        },
        TeletextEnhancementCommand::InvokeObject {
            target: target(owner),
            object_type: TeletextObjectType::Passive,
            offset: Some(TeletextObjectOffset::new(0, 32).unwrap()),
        },
    ]));
    input.objects.push(definition(
        target(owner),
        TeletextObjectType::Passive,
        vec![
            TeletextEnhancementCommand::AtColumn {
                column: 0,
                operation: TeletextColumnOperation::Foreground {
                    colour: TeletextColourReference::new(18).unwrap(),
                },
            },
            TeletextEnhancementCommand::AtColumn {
                column: 0,
                operation: TeletextColumnOperation::Background {
                    colour: TeletextColourReference::new(0).unwrap(),
                },
            },
            TeletextEnhancementCommand::AtColumn {
                column: 0,
                operation: g0(b'A'),
            },
            TeletextEnhancementCommand::AtColumn {
                column: 1,
                operation: g1(0),
            },
            TeletextEnhancementCommand::AtColumn {
                column: 2,
                operation: TeletextColumnOperation::Drcs {
                    reference: glyph_reference(0),
                },
            },
        ],
    ));
    input.magazines.push(TeletextMagazinePresentationDraft {
        magazine: 3,
        presentation: presentation(0x20, 2),
    });
    // Unrelated selected public resource and DRCS table must survive reconstruction.
    input.pages[0].resources.objects.public = Some(raw(0xa0));
    input.objects.push(definition(
        public(0xa0, 1, 1),
        TeletextObjectType::Passive,
        vec![],
    ));
    input.pages[0].resources.drcs = TeletextDrcsPageLinks::new(Some(raw(0xb0)), None).unwrap();
    input.drcs.push(table(0xb0, 0, 0x111));
    input
}

fn service(input: TeletextResolvedServiceDraft) -> TeletextSemanticService {
    let mut service = TeletextSemanticService::new_with_object_companions(
        input,
        vec![resource(TeletextResourceKind::Pop, 0xa0, 0)],
        budget(),
    )
    .unwrap();
    service.set_mot_fallback(address(200, 1), presentation(0, 3));
    service
        .with_physical_base_overlays(
            vec![TeletextPhysicalBaseOverlay::new(
                address(200, 0),
                vec![TeletextPhysicalBaseRow::new(6, [b'P'; 40]).unwrap()],
            )
            .unwrap()],
            budget(),
        )
        .unwrap()
}

#[test]
fn local_object_glyphs_preserve_exact_definition_identity_type_order_and_service_inputs() {
    let input = input();
    let original = service(input.clone());
    let identity = target(address(200, 0));
    let changes = [
        change(identity, 2, 0, g0(b'A'), g0(b'B')),
        change(identity, 3, 1, g1(0), g1(63)),
    ];
    assert_eq!(changes[0].owner(), address(200, 0));
    let edited = original
        .clone()
        .with_local_object_operand_overlays(&changes, budget())
        .unwrap();
    let mut expected = input;
    let mut commands = expected.objects[0].program.commands().to_vec();
    commands[2] = TeletextEnhancementCommand::AtColumn {
        column: 0,
        operation: g0(b'B'),
    };
    commands[3] = TeletextEnhancementCommand::AtColumn {
        column: 1,
        operation: g1(63),
    };
    expected.objects[0].program = program(commands);
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
    assert!(matches!(
        edited.with_local_object_operand_overlays(&changes, budget()),
        Err(TeletextServiceError::LocalObjectOperandOverlay {
            reason: TeletextLocalObjectOperandFailure::CommandMismatch,
            ..
        })
    ));
}

#[test]
fn local_object_intents_reject_external_scope_cross_family_noop_and_invalid_column() {
    assert!(TeletextGlyphCode::new(0x1f).is_err());
    assert!(TeletextGlyphCode::new(0x80).is_err());
    assert!(TeletextMosaic::new(64).is_err());
    let identity = target(address(200, 0));
    for external in [public(0xa0, 0, 65520), global(0xa1, 65520)] {
        assert_eq!(
            TeletextLocalObjectOperandOverlay::new(external, 2, 0, g0(b'A'), g0(b'B')),
            Err(TeletextLocalObjectOperandOverlayError::Scope)
        );
    }
    for (expected, replacement) in [
        (g0(b'A'), g1(1)),
        (g1(0), g0(b'A')),
        (
            TeletextColumnOperation::Foreground {
                colour: TeletextColourReference::new(1).unwrap(),
            },
            TeletextColumnOperation::Background {
                colour: TeletextColourReference::new(2).unwrap(),
            },
        ),
        (
            TeletextColumnOperation::G1Alpha {
                code: TeletextG1AlphaCode::new(b'A').unwrap(),
            },
            TeletextColumnOperation::G1Alpha {
                code: TeletextG1AlphaCode::new(b'B').unwrap(),
            },
        ),
    ] {
        assert_eq!(
            TeletextLocalObjectOperandOverlay::new(identity, 2, 0, expected, replacement),
            Err(TeletextLocalObjectOperandOverlayError::Operand(
                TeletextLocalProgramOperandOverlayError::Family
            ))
        );
    }
    assert_eq!(
        TeletextLocalObjectOperandOverlay::new(identity, 2, 0, g0(b'A'), g0(b'A')),
        Err(TeletextLocalObjectOperandOverlayError::Operand(
            TeletextLocalProgramOperandOverlayError::Unchanged
        ))
    );
    assert_eq!(
        TeletextLocalObjectOperandOverlay::new(identity, 2, 40, g0(b'A'), g0(b'B')),
        Err(TeletextLocalObjectOperandOverlayError::Operand(
            TeletextLocalProgramOperandOverlayError::Column { column: 40 }
        ))
    );
    assert!(
        TeletextLocalObjectOperandOverlay::new(identity, usize::MAX, 39, g1(0), g1(63)).is_ok()
    );
}

#[test]
fn local_object_application_rejects_duplicate_missing_foreign_and_stale_commands() {
    let original = service(input());
    let identity = target(address(200, 0));
    let valid = change(identity, 2, 0, g0(b'A'), g0(b'B'));
    assert_eq!(
        original
            .clone()
            .with_local_object_operand_overlays(&[], budget()),
        Err(TeletextServiceError::EmptyLocalObjectOperandOverlays)
    );
    for (overlays, identity, command, reason) in [
        (
            vec![valid, valid],
            identity,
            2,
            TeletextLocalObjectOperandFailure::Duplicate,
        ),
        (
            vec![change(target(address(200, 1)), 2, 0, g0(b'A'), g0(b'B'))],
            target(address(200, 1)),
            2,
            TeletextLocalObjectOperandFailure::MissingObject,
        ),
        (
            vec![change(
                TeletextObjectIdentity::local(address(200, 0), TeletextObjectId::new(65519)),
                2,
                0,
                g0(b'A'),
                g0(b'B'),
            )],
            TeletextObjectIdentity::local(address(200, 0), TeletextObjectId::new(65519)),
            2,
            TeletextLocalObjectOperandFailure::MissingObject,
        ),
        (
            vec![change(identity, 0, 0, g0(b'A'), g0(b'B'))],
            identity,
            0,
            TeletextLocalObjectOperandFailure::CommandMismatch,
        ),
        (
            vec![change(identity, usize::MAX, 0, g0(b'A'), g0(b'B'))],
            identity,
            usize::MAX,
            TeletextLocalObjectOperandFailure::CommandMismatch,
        ),
        (
            vec![change(identity, 2, 1, g0(b'A'), g0(b'B'))],
            identity,
            2,
            TeletextLocalObjectOperandFailure::CommandMismatch,
        ),
        (
            vec![change(identity, 2, 0, g0(b'C'), g0(b'B'))],
            identity,
            2,
            TeletextLocalObjectOperandFailure::CommandMismatch,
        ),
        (
            vec![change(identity, 2, 0, g1(0), g1(63))],
            identity,
            2,
            TeletextLocalObjectOperandFailure::CommandMismatch,
        ),
    ] {
        assert_eq!(
            original
                .clone()
                .with_local_object_operand_overlays(&overlays, budget()),
            Err(TeletextServiceError::LocalObjectOperandOverlay {
                identity,
                command,
                reason
            })
        );
    }
}

#[test]
fn local_object_application_reuses_raw_work_and_complete_service_budget_preflight() {
    let original = service(attribute_input(false, false));
    let identity = target(address(200, 0));
    for valid in [
        change(identity, 2, 0, g0(b'A'), g0(b'B')),
        change(identity, 0, 0, foreground(18), foreground(31)),
        change(identity, 1, 0, background(0), background(16)),
        change(
            identity,
            5,
            0,
            attributes(false, false, 0),
            attributes(false, false, 15),
        ),
    ] {
        // Raw change count wins before constructing a duplicate index.
        let mut raw = budget();
        raw.max_commands = 1;
        assert_eq!(
            original
                .clone()
                .with_local_object_operand_overlays(&[valid, valid], raw),
            Err(TeletextServiceError::Budget {
                dimension: "authored command",
                maximum: 1
            })
        );
        for dimension in [
            "record",
            "object",
            "structure item",
            "base content byte",
            "authored command",
            "collected command",
            "collected DRCS reference",
        ] {
            let mut limit = budget();
            match dimension {
                "record" => limit.max_records = 0,
                "object" => limit.max_objects = 0,
                "structure item" => limit.max_structure_items = 0,
                "base content byte" => limit.max_base_content_bytes = 0,
                "authored command" => limit.max_commands = 0,
                "collected command" => limit.max_collected_commands = 0,
                _ => limit.max_collected_drcs_references = 0,
            }
            assert!(
                matches!(original.clone().with_local_object_operand_overlays(&[valid], limit),
            Err(TeletextServiceError::Budget { dimension: actual, .. }) if actual == dimension),
                "{dimension}"
            );
        }
    }
}

#[test]
fn page_local_g0_and_g1_use_same_exact_original_family_and_service_validation() {
    let owner = address(200, 0);
    let mut input = draft(&[owner]);
    input.pages[0].local_program = Some(program(vec![
        TeletextEnhancementCommand::SetPosition {
            position: TeletextEnhancementPosition::new(2, 0).unwrap(),
        },
        TeletextEnhancementCommand::AtColumn {
            column: 0,
            operation: g0(b'A'),
        },
        TeletextEnhancementCommand::AtColumn {
            column: 39,
            operation: g1(0),
        },
    ]));
    let original = TeletextSemanticService::new(input.clone(), budget()).unwrap();
    let changes = [
        TeletextLocalProgramOperandOverlay::new(owner, 1, 0, g0(b'A'), g0(0x7f)).unwrap(),
        TeletextLocalProgramOperandOverlay::new(owner, 2, 39, g1(0), g1(63)).unwrap(),
    ];
    let edited = original
        .with_local_program_operand_overlays(&changes, budget())
        .unwrap();
    input.pages[0].local_program = Some(program(vec![
        TeletextEnhancementCommand::SetPosition {
            position: TeletextEnhancementPosition::new(2, 0).unwrap(),
        },
        TeletextEnhancementCommand::AtColumn {
            column: 0,
            operation: g0(0x7f),
        },
        TeletextEnhancementCommand::AtColumn {
            column: 39,
            operation: g1(63),
        },
    ]));
    assert_eq!(
        edited,
        TeletextSemanticService::new(input, budget()).unwrap()
    );
}

#[path = "local_object_operand_attribute_tests.rs"]
mod attribute_tests;
