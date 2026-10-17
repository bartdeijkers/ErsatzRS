//! Full authenticated aliases survive call topology before native-selector erasure.
use super::*;

fn invoked(offset: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::InvokeObject {
        target: target(22),
        object_type: TeletextObjectType::Passive,
        offset: Some(TeletextObjectOffset::new(0, offset).unwrap()),
    }
}
fn root_with(calls: &[u8]) -> TeletextEnhancementProgram {
    program(
        std::iter::once(glyph(0, b'A'))
            .chain(calls.iter().copied().map(invoked))
            .collect(),
    )
}
#[test]
fn context_call_topology_preserves_complete_identity_and_fixed_native_skeleton() {
    for (before, after) in [
        (&[0][..], &[0, 1][..]),
        (&[0][..], &[1, 2][..]),
        (&[0, 1][..], &[0][..]),
        (&[0][..], &[][..]),
        (&[][..], &[1][..]),
    ] {
        // POP22 and unselected POP24 deliberately share the same wire selector.
        let original = native_fixture(root_with(before), true);
        let replacement = root_with(after);
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
        let after = files(&original, &project, &edited.document);
        let changes =
            reconcile_project(&project, &original, &before, &after, limits(), deadline()).unwrap();
        assert!(changes.operands.is_empty());
        assert_eq!(changes.programs.len(), 1);
        let change = &changes.programs[0];
        assert!(change.has_invocation_structure_changes());
        assert_eq!(change.replacement(), &replacement);
        assert_eq!(
            change.expected(),
            &original
                .declared()
                .semantic()
                .objects()
                .definition(target(21))
                .unwrap()
                .program
        );
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
}
fn assert_glyph_topology_roundtrip(
    original_glyph: TeletextEnhancementCommand,
    replacement_glyph: TeletextEnhancementCommand,
    before_calls: &[u8],
    after_calls: &[u8],
) {
    let commands = |glyph, calls: &[u8]| {
        program(
            std::iter::once(glyph)
                .chain(calls.iter().copied().map(invoked))
                .collect(),
        )
    };
    let expected = commands(original_glyph, before_calls);
    let replacement = commands(replacement_glyph, after_calls);
    // The extra unselected POP24 has the same native selector as POP22.
    // Import must retain the sealed POP22 identity, never infer one from words.
    let original = native_fixture(expected.clone(), true);
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
    let after = files(&original, &project, &edited.document);
    for (a, b) in before.iter().zip(&after) {
        assert_eq!(a.name, b.name);
        if a.name != "context.tti" {
            assert_eq!(a.bytes, b.bytes);
        }
    }
    let changes =
        reconcile_project(&project, &original, &before, &after, limits(), deadline()).unwrap();
    assert!(changes.operands.is_empty());
    let [change] = changes.programs.as_slice() else {
        panic!("combined scalar/call edit must remain one original resource program")
    };
    assert_eq!(change.identity(), target(21));
    assert_eq!(change.expected(), &expected);
    assert_eq!(change.replacement(), &replacement);
    assert_eq!(change.original_command(0), Some(0));
    assert!(change.has_invocation_structure_changes());
    for command in &change.replacement().commands()[1..] {
        assert!(matches!(command,
            TeletextEnhancementCommand::InvokeObject { target: selected, object_type: TeletextObjectType::Passive, .. }
                if *selected == target(22)));
    }
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
fn context_g0_scalar_with_call_add_delete_and_multiple_calls_qualifies_native_candidate() {
    for (before, after) in [
        (&[0][..], &[0, 1][..]),
        (&[0, 1][..], &[1][..]),
        (&[0][..], &[][..]),
        (&[][..], &[1, 2][..]),
    ] {
        assert_glyph_topology_roundtrip(glyph(0, b'A'), glyph(0, b'B'), before, after);
    }
}

fn mosaic(bits: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column: 0,
        operation: TeletextColumnOperation::G1 {
            mosaic: TeletextMosaic::new(bits).unwrap(),
        },
    }
}

#[test]
fn context_g1_scalar_with_call_add_and_delete_keeps_native_family_and_original_identity() {
    for (before, after) in [(&[0][..], &[0, 1][..]), (&[0, 1][..], &[][..])] {
        assert_glyph_topology_roundtrip(mosaic(1), mosaic(21), before, after);
    }
}

#[test]
fn context_call_topology_keeps_wrong_family_moved_column_and_fixed_anchors_closed() {
    let foreground = |colour| TeletextEnhancementCommand::AtColumn {
        column: 0,
        operation: TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(colour).unwrap(),
        },
    };
    let position = |row| TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(row, 0).unwrap(),
    };
    let charset = |code| TeletextEnhancementCommand::AtColumn {
        column: 0,
        operation: TeletextColumnOperation::DesignateCharset {
            designation: TeletextEnhancementCharsetDesignation::new(code).unwrap(),
        },
    };
    for (before, after) in [
        (glyph(0, b'A'), mosaic(1)),
        (glyph(0, b'A'), glyph(1, b'B')),
        (foreground(2), foreground(3)),
        (position(1), position(2)),
        (charset(0), charset(1)),
    ] {
        let original = native_fixture(program(vec![before, invoked(0)]), true);
        let replacement = program(vec![after, invoked(0), invoked(1)]);
        let _candidate = native_fixture(replacement.clone(), true);
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
        assert!(
            reconcile_project(
                &project,
                &original,
                &files(&original, &project, &baseline.document),
                &files(&original, &project, &edited.document),
                limits(),
                deadline(),
            )
            .is_err(),
            "unsupported noncall change: {before:?} -> {after:?}"
        );
    }
}

#[test]
fn context_combined_scalar_call_change_respects_original_deadline() {
    let original = native_fixture(root_with(&[0]), true);
    let replacement = program(vec![glyph(0, b'B'), invoked(0), invoked(1)]);
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
    let error = reconcile_project(
        &project,
        &original,
        &files(&original, &project, &baseline.document),
        &files(&original, &project, &edited.document),
        limits(),
        Instant::now()
            .checked_sub(std::time::Duration::from_secs(1))
            .unwrap(),
    )
    .err()
    .unwrap();
    assert_eq!(error.failure, Failure::TimedOut);
}

#[test]
fn context_combined_scalar_call_change_rejects_raw_reserved_origin_prefix() {
    let original = native_fixture(root_with(&[0]), true);
    let replacement = program(vec![glyph(0, b'B'), invoked(0), invoked(1)]);
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
    let root = edited.encoded.definition_positions()[0].target();
    let span = edited
        .encoded
        .command_positions()
        .iter()
        .find(|position| position.source().object == Some(root) && position.source().command == 1)
        .unwrap();
    assert_eq!(span.first_word() + 1, span.semantic_word());
    let index = span.first_word();
    let prefix = edited
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
        .unwrap();
    assert_eq!(prefix, 0x28 | (16 << 6));
    let before = files(&original, &project, &baseline.document);
    let mut after = files(&original, &project, &edited.document);
    validate_applied_project(
        &project,
        &original,
        &candidate,
        &after,
        limits(),
        deadline(),
    )
    .unwrap();
    // Keep the actual compound invocation intact except its checked offset:
    // column72 is reserved, not a new glyph, position or resource selector.
    let damaged = mutate(&edited.document, index, prefix | (72 << 11));
    after
        .iter_mut()
        .find(|file| file.name == "context.tti")
        .unwrap()
        .bytes = damaged;
    for (a, b) in before.iter().zip(&after) {
        assert_eq!(a.name, b.name);
        if a.name != "context.tti" {
            assert_eq!(a.bytes, b.bytes);
        }
    }
    let error = reconcile_project(&project, &original, &before, &after, limits(), deadline())
        .err()
        .unwrap();
    assert_eq!(error.failure, Failure::Structure);
}
