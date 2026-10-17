//! Extended exact-original operands, not native glyph assignment or packet proof.
use super::*;

fn g2(code: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::G2 {
        code: TeletextGlyphCode::new(code).unwrap(),
    }
}
fn diacritic(code: u8, index: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::G0Diacritic {
        code: TeletextGlyphCode::new(code).unwrap(),
        diacritic: TeletextDiacriticIndex::new(index).unwrap(),
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

fn extended_input() -> TeletextResolvedServiceDraft {
    let mut input = input();
    let original = input.pages[0].local_program.as_ref().unwrap().commands();
    input.pages[0].local_program = Some(program(vec![
        original[0],
        column(
            8,
            TeletextColumnOperation::DesignateCharset {
                designation: TeletextEnhancementCharsetDesignation::new(0x24).unwrap(),
            },
        ),
        original[1],
        column(8, g2(0x27)),
        column(9, diacritic(b'A', 1)),
    ]));
    let commands = input.objects[0].program.commands();
    input.objects[0].program = program(vec![
        commands[0],
        commands[1],
        column(
            0,
            TeletextColumnOperation::DesignateCharset {
                designation: TeletextEnhancementCharsetDesignation::new(0x10).unwrap(),
            },
        ),
        column(0, g2(0x27)),
        column(1, diacritic(b'A', 1)),
        commands[4],
    ]);
    input.base.pages[0].rows.push(TeletextRow {
        row: 5,
        runs: vec![TeletextRun {
            column: 1,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Text {
                text: "Original fallback".to_owned(),
            },
        }],
    });
    input.base.pages[0].navigation.push(TeletextNavigationLink {
        role: TeletextNavigationRole::Red,
        target: TeletextNavigationTarget {
            page: address(200, 1).page,
            subcode: Some(address(200, 1).subcode),
        },
    });
    input
}

#[test]
fn extended_operands_preserve_all_checked_code_and_diacritic_scalar_values() {
    let owner = address(200, 0);
    let identity = target(owner);
    for (expected, replacement) in [
        (g2(0x20), g2(0x7f)),
        (diacritic(0x20, 0), diacritic(0x7f, 15)),
        (diacritic(0x2a, 0), diacritic(0x2b, 0)),
    ] {
        let page = page_change(usize::MAX, 39, expected, replacement);
        let object = change(identity, usize::MAX, 39, expected, replacement);
        assert_eq!(
            (page.owner(), page.command(), page.column()),
            (owner, usize::MAX, 39)
        );
        assert_eq!(
            (
                object.owner(),
                object.identity(),
                object.command(),
                object.column()
            ),
            (owner, identity, usize::MAX, 39)
        );
        assert_eq!(
            (page.expected(), page.replacement()),
            (expected, replacement)
        );
        assert_eq!(
            (object.expected(), object.replacement()),
            (expected, replacement)
        );
    }
    // The entire same variant is editable: code-only, selector-only and both.
    // Indices 0, 9, 12 and 15 remain exact, without Latin normalization.
    for before in 0..=15 {
        for after in 0..=15 {
            let expected = diacritic(b'A', before);
            let replacement = diacritic(b'E', after);
            assert_eq!(
                page_change(4, 9, expected, replacement).replacement(),
                replacement
            );
            assert_eq!(
                change(identity, 4, 1, expected, replacement).replacement(),
                replacement
            );
            if before != after {
                assert!(TeletextLocalProgramOperandOverlay::new(
                    owner,
                    4,
                    9,
                    expected,
                    diacritic(b'A', after)
                )
                .is_ok());
                assert!(TeletextLocalObjectOperandOverlay::new(
                    identity,
                    4,
                    1,
                    expected,
                    diacritic(b'A', after)
                )
                .is_ok());
            }
        }
    }
}

#[test]
fn extended_operands_reject_reserved_scalars_cross_family_noop_and_invalid_columns() {
    let owner = address(200, 0);
    let identity = target(owner);
    for code in [0x1f, 0x80] {
        assert!(TeletextGlyphCode::new(code).is_err());
        for kind in ["g2", "g0_diacritic"] {
            let mut value = serde_json::json!({"kind":kind,"code":code});
            if kind == "g0_diacritic" {
                value["diacritic"] = serde_json::json!(0);
            }
            assert!(serde_json::from_value::<TeletextColumnOperation>(value).is_err());
        }
    }
    for expected in [g2(0x20), g2(0x7f), diacritic(0x20, 0), diacritic(0x7f, 15)] {
        assert_eq!(
            serde_json::from_value::<TeletextColumnOperation>(
                serde_json::to_value(expected).unwrap()
            )
            .unwrap(),
            expected
        );
    }
    assert!(TeletextDiacriticIndex::new(16).is_err());
    assert!(serde_json::from_value::<TeletextColumnOperation>(
        serde_json::json!({"kind":"g0_diacritic","code":65,"diacritic":16})
    )
    .is_err());
    let g3 = |code| TeletextColumnOperation::G3 {
        code: TeletextGlyphCode::new(code).unwrap(),
        compatibility: TeletextG3Compatibility::Level25,
    };
    for (expected, replacement) in [
        (g2(0x27), g0(b'A')),
        (g0(b'A'), g2(0x27)),
        (g2(0x27), diacritic(b'A', 1)),
        (diacritic(b'A', 0), g0(b'A')),
        (diacritic(b'A', 1), g2(0x27)),
        (g3(b'A'), g3(b'B')),
    ] {
        assert_eq!(
            TeletextLocalProgramOperandOverlay::new(owner, 3, 8, expected, replacement),
            Err(TeletextLocalProgramOperandOverlayError::Family)
        );
        assert_eq!(
            TeletextLocalObjectOperandOverlay::new(identity, 3, 0, expected, replacement),
            Err(TeletextLocalObjectOperandOverlayError::Operand(
                TeletextLocalProgramOperandOverlayError::Family
            ))
        );
    }
    for expected in [g2(0x27), diacritic(b'A', 0), diacritic(b'A', 15)] {
        assert_eq!(
            TeletextLocalProgramOperandOverlay::new(owner, 3, 8, expected, expected),
            Err(TeletextLocalProgramOperandOverlayError::Unchanged)
        );
        assert_eq!(
            TeletextLocalObjectOperandOverlay::new(identity, 3, 0, expected, expected),
            Err(TeletextLocalObjectOperandOverlayError::Operand(
                TeletextLocalProgramOperandOverlayError::Unchanged
            ))
        );
        let replacement = match expected {
            TeletextColumnOperation::G2 { .. } => g2(0x29),
            _ => diacritic(b'E', 2),
        };
        assert_eq!(
            TeletextLocalProgramOperandOverlay::new(owner, 3, 40, expected, replacement),
            Err(TeletextLocalProgramOperandOverlayError::Column { column: 40 })
        );
        assert_eq!(
            TeletextLocalObjectOperandOverlay::new(identity, 3, 40, expected, replacement),
            Err(TeletextLocalObjectOperandOverlayError::Operand(
                TeletextLocalProgramOperandOverlayError::Column { column: 40 }
            ))
        );
    }
}

#[test]
fn extended_operands_rebuild_page_and_local_object_with_exact_context_and_all_originals() {
    let input = extended_input();
    let original = service(input.clone());
    let identity = target(address(200, 0));
    let page_changes = [
        page_change(3, 8, g2(0x27), g2(0x29)),
        page_change(4, 9, diacritic(b'A', 1), diacritic(b'E', 2)),
    ];
    let object_changes = [
        change(identity, 3, 0, g2(0x27), g2(0x29)),
        change(identity, 4, 1, diacritic(b'A', 1), diacritic(b'E', 2)),
    ];
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
    page[3] = column(8, g2(0x29));
    page[4] = column(9, diacritic(b'E', 2));
    expected.pages[0].local_program = Some(program(page));
    let mut object = expected.objects[0].program.commands().to_vec();
    object[3] = column(0, g2(0x29));
    object[4] = column(1, diacritic(b'E', 2));
    expected.objects[0].program = program(object);
    // Whole-model equality includes exact raw 0x24/0x10 designation aliases,
    // unchanged default/presentation/resource/physical base/nav/source ownership.
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
fn extended_operands_keep_stale_and_duplicate_rejection_for_page_and_object_targets() {
    let original = service(extended_input());
    let identity = target(address(200, 0));
    for valid in [
        page_change(3, 8, g2(0x27), g2(0x29)),
        page_change(4, 9, diacritic(b'A', 1), diacritic(b'E', 2)),
    ] {
        assert_eq!(
            original
                .clone()
                .with_local_program_operand_overlays(&[valid, valid], budget()),
            Err(TeletextServiceError::LocalProgramOperandOverlay {
                address: valid.owner(),
                command: valid.command(),
                reason: TeletextLocalProgramOperandFailure::Duplicate
            })
        );
        let edited = original
            .clone()
            .with_local_program_operand_overlays(&[valid], budget())
            .unwrap();
        assert_eq!(
            edited.with_local_program_operand_overlays(&[valid], budget()),
            Err(TeletextServiceError::LocalProgramOperandOverlay {
                address: valid.owner(),
                command: valid.command(),
                reason: TeletextLocalProgramOperandFailure::CommandMismatch
            })
        );
    }
    for valid in [
        change(identity, 3, 0, g2(0x27), g2(0x29)),
        change(identity, 4, 1, diacritic(b'A', 1), diacritic(b'E', 2)),
    ] {
        assert_eq!(
            original
                .clone()
                .with_local_object_operand_overlays(&[valid, valid], budget()),
            Err(TeletextServiceError::LocalObjectOperandOverlay {
                identity,
                command: valid.command(),
                reason: TeletextLocalObjectOperandFailure::Duplicate
            })
        );
        let edited = original
            .clone()
            .with_local_object_operand_overlays(&[valid], budget())
            .unwrap();
        assert_eq!(
            edited.with_local_object_operand_overlays(&[valid], budget()),
            Err(TeletextServiceError::LocalObjectOperandOverlay {
                identity,
                command: valid.command(),
                reason: TeletextLocalObjectOperandFailure::CommandMismatch
            })
        );
    }
}

#[test]
fn extended_operands_retain_raw_and_collector_limits_before_reconstruction() {
    let original = service(extended_input());
    let identity = target(address(200, 0));
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
        for (page, object) in [
            (
                page_change(3, 8, g2(0x27), g2(0x29)),
                change(identity, 3, 0, g2(0x27), g2(0x29)),
            ),
            (
                page_change(4, 9, diacritic(b'A', 1), diacritic(b'E', 2)),
                change(identity, 4, 1, diacritic(b'A', 1), diacritic(b'E', 2)),
            ),
        ] {
            assert!(
                matches!(original.clone().with_local_program_operand_overlays(&[page,page],limit),Err(TeletextServiceError::Budget { dimension:actual,.. }) if actual==dimension)
            );
            assert!(
                matches!(original.clone().with_local_object_operand_overlays(&[object,object],limit),Err(TeletextServiceError::Budget { dimension:actual,.. }) if actual==dimension)
            );
        }
    }
}
