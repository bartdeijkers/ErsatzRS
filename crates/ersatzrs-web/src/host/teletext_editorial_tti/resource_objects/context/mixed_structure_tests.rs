//! Structural root edits retain the sealed resource catalogue and native body.
use super::*;

fn foreground(column: u8, colour: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column,
        operation: TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(colour).unwrap(),
        },
    }
}

#[test]
fn context_mixed_glyph_style_and_call_structure_reconstructs_full_native_program() {
    let expected = root_with(&[0]);
    let replacement = program(vec![
        glyph(1, b'A'),
        glyph(2, b'B'),
        foreground(3, 2),
        invoked(0),
        invoked(1),
    ]);
    // Both complete native services must encode before editor reconciliation.
    // Unselected POP24 has POP22's native selector; the receipt owns identity.
    let original = native_fixture(expected.clone(), true);
    let candidate = native_fixture(replacement.clone(), true);
    let project = receipt(&original);
    let sealed = serde_json::to_vec(&project).unwrap();
    assert_eq!(project.editor_profile, "qteletextmaker-1.0-carrier-v8");
    let baseline =
        build_context(&original, target(21), limits().1, limits().0, deadline()).unwrap();
    let edited = build_context_with_root(
        &original,
        target(21),
        &replacement,
        limits().1,
        limits().0,
        deadline(),
    )
    .unwrap();
    assert_eq!(edited.aliases, baseline.aliases);
    let before = files(&original, &project, &baseline.document);
    let after = files(&original, &project, &edited.document);
    assert_eq!(before.len(), after.len());
    for (original, uploaded) in before.iter().zip(&after) {
        assert_eq!(original.name, uploaded.name);
        if original.name == "context.tti" {
            assert_ne!(original.bytes, uploaded.bytes);
        } else {
            assert_eq!(original.bytes, uploaded.bytes);
        }
    }
    let changes =
        reconcile_project(&project, &original, &before, &after, limits(), deadline()).unwrap();
    assert!(changes.operands.is_empty());
    let [change] = changes.programs.as_slice() else {
        panic!("combined root edits must retain one original resource program");
    };
    assert_eq!(change.identity(), target(21));
    assert_eq!(change.expected(), &expected);
    assert_eq!(change.replacement(), &replacement);
    assert!(change.has_invocation_structure_changes());
    validate_applied_project(
        &project,
        &original,
        &candidate,
        &after,
        limits(),
        deadline(),
    )
    .unwrap();
    assert_eq!(serde_json::to_vec(&project).unwrap(), sealed);
}

fn enriched() -> TeletextEnhancementProgram {
    program(vec![
        glyph(1, b'A'),
        TeletextEnhancementCommand::AtColumn {
            column: 2,
            operation: TeletextColumnOperation::G1 {
                mosaic: TeletextMosaic::new(21).unwrap(),
            },
        },
        foreground(3, 2),
        TeletextEnhancementCommand::AtColumn {
            column: 3,
            operation: TeletextColumnOperation::Background {
                colour: TeletextColourReference::new(1).unwrap(),
            },
        },
        TeletextEnhancementCommand::AtColumn {
            column: 3,
            operation: TeletextColumnOperation::Attributes {
                double_width: false,
                double_height: false,
                boxed: false,
                conceal: false,
                invert: true,
                underline_or_separated_mosaics: false,
            },
        },
        invoked(0),
        invoked(1),
    ])
}
fn assert_mixed_roundtrip(
    expected: &TeletextEnhancementProgram,
    replacement: &TeletextEnhancementProgram,
) {
    let original = native_fixture(expected.clone(), true);
    let candidate = native_fixture(replacement.clone(), true);
    let project = receipt(&original);
    let baseline =
        build_context(&original, target(21), limits().1, limits().0, deadline()).unwrap();
    let edited = build_context_with_root(
        &original,
        target(21),
        replacement,
        limits().1,
        limits().0,
        deadline(),
    )
    .unwrap();
    assert_eq!(edited.aliases, baseline.aliases);
    let before = files(&original, &project, &baseline.document);
    let after = files(&original, &project, &edited.document);
    for (source, uploaded) in before.iter().zip(&after) {
        assert_eq!(source.name, uploaded.name);
        if source.name != "context.tti" {
            assert_eq!(source.bytes, uploaded.bytes);
        }
    }
    let changes =
        reconcile_project(&project, &original, &before, &after, limits(), deadline()).unwrap();
    assert!(changes.operands.is_empty());
    let [change] = changes.programs.as_slice() else {
        panic!("mixed structure must reconstruct one complete resource program");
    };
    assert_eq!(change.identity(), target(21));
    assert_eq!(change.expected(), expected);
    assert_eq!(change.replacement(), replacement);
    assert!(change.has_invocation_structure_changes());
    super::super::super::project::authorize_programs(
        &project,
        &original,
        &[],
        &changes.programs,
        limits(),
        deadline(),
    )
    .unwrap();
    validate_applied_project(
        &project,
        &original,
        &candidate,
        &after,
        limits(),
        deadline(),
    )
    .unwrap();
}

#[test]
fn context_mixed_g0_g1_and_all_style_insert_delete_move_with_calls_qualify_native_candidate() {
    assert_mixed_roundtrip(&root_with(&[0]), &enriched());
    assert_mixed_roundtrip(&enriched(), &program(vec![glyph(2, b'B'), invoked(0)]));
    // The old same-family moved-column rejection is now an explicit positive.
    assert_mixed_roundtrip(
        &root_with(&[0]),
        &program(vec![glyph(1, b'B'), invoked(0), invoked(1)]),
    );
}

#[test]
fn context_mixed_structure_retains_readonly_callees_and_indivisible_origin_invocations() {
    let original = native_fixture(root_with(&[0]), true);
    let replacement = enriched();
    let candidate = native_fixture(replacement.clone(), true);
    let project = receipt(&original);
    let baseline =
        build_context(&original, target(21), limits().1, limits().0, deadline()).unwrap();
    let edited = build_context_with_root(
        &original,
        target(21),
        &replacement,
        limits().1,
        limits().0,
        deadline(),
    )
    .unwrap();
    let before = files(&original, &project, &baseline.document);
    let valid = files(&original, &project, &edited.document);
    validate_applied_project(
        &project,
        &original,
        &candidate,
        &valid,
        limits(),
        deadline(),
    )
    .unwrap();
    let positions = edited.encoded.definition_positions();
    assert_eq!(edited.aliases[1], target(22));
    let position = |object, command| {
        edited
            .encoded
            .command_positions()
            .iter()
            .find(|position| {
                position.source().object == Some(object) && position.source().command == command
            })
            .unwrap()
    };
    let callee = position(positions[1].target(), 0).semantic_word();
    let call = position(positions[0].target(), replacement.commands().len() - 1);
    assert_eq!(call.first_word() + 1, call.semantic_word());
    let word = |index| {
        edited
            .document
            .records()
            .iter()
            .find_map(|record| match record {
                TeletextTtiRecord::Packet {
                    row: 26,
                    designation,
                    words,
                } if usize::from(*designation) == index / 13 => Some(words[index % 13]),
                _ => None,
            })
            .unwrap()
    };
    assert_eq!((word(callee) >> 6) & 31, 9);
    assert_eq!(word(callee) >> 11, u32::from(b'P'));
    assert_eq!((word(call.first_word()) >> 6) & 31, 16);
    for (index, value) in [
        (callee, word(callee) ^ (1 << 11)),
        (
            call.first_word(),
            (word(call.first_word()) & 0x7ff) | (72 << 11),
        ),
    ] {
        let mut after = valid.clone();
        after
            .iter_mut()
            .find(|file| file.name == "context.tti")
            .unwrap()
            .bytes = mutate(&edited.document, index, value);
        let error = reconcile_project(&project, &original, &before, &after, limits(), deadline())
            .err()
            .unwrap();
        assert_eq!(error.failure, Failure::Structure);
        for (source, uploaded) in before.iter().zip(&after) {
            if source.name != "context.tti" {
                assert_eq!(source, uploaded);
            }
        }
    }
}
