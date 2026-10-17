//! Checked scalar style changes never inherit the original writer's authority.
use super::*;

fn styles(changed: u8) -> [TeletextEnhancementCommand; 3] {
    let colour = |value| TeletextColourReference::new(value).unwrap();
    [
        TeletextColumnOperation::Foreground {
            colour: colour(if changed & 1 == 0 { 2 } else { 3 }),
        },
        TeletextColumnOperation::Background {
            colour: colour(if changed & 2 == 0 { 1 } else { 4 }),
        },
        TeletextColumnOperation::Attributes {
            double_height: false,
            double_width: false,
            boxed: changed & 4 != 0,
            conceal: false,
            invert: false,
            underline_or_separated_mosaics: false,
        },
    ]
    .map(|operation| TeletextEnhancementCommand::AtColumn {
        column: 0,
        operation,
    })
}

fn commands(changed: u8, calls: &[u8]) -> TeletextEnhancementProgram {
    program(
        styles(changed)
            .into_iter()
            .chain(std::iter::once(glyph(1, b'A')))
            .chain(calls.iter().copied().map(invoked))
            .collect(),
    )
}

fn assert_roundtrip(fields: u8, before_calls: &[u8], after_calls: &[u8]) {
    let expected = commands(0, before_calls);
    let replacement = commands(fields, after_calls);
    // Both complete native programs encode before uploaded words are examined.
    // Unselected POP24 deliberately collides with the selected POP22 selector.
    let original = native_fixture(expected.clone(), true);
    let candidate = native_fixture(replacement.clone(), true);
    let project = receipt(&original);
    let sealed = serde_json::to_vec(&project).unwrap();
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
    assert_eq!(baseline.aliases, edited.aliases);
    let before = files(&original, &project, &baseline.document);
    let after = files(&original, &project, &edited.document);
    for (original, uploaded) in before.iter().zip(&after) {
        assert_eq!(original.name, uploaded.name);
        if original.name != "context.tti" {
            assert_eq!(original.bytes, uploaded.bytes);
        }
    }
    let changes =
        reconcile_project(&project, &original, &before, &after, limits(), deadline()).unwrap();
    assert!(changes.operands.is_empty());
    let [change] = changes.programs.as_slice() else {
        panic!("style values and call topology must remain one original resource program")
    };
    assert_eq!(change.identity(), target(21));
    assert_eq!(change.expected(), &expected);
    assert_eq!(change.replacement(), &replacement);
    assert!(change.has_invocation_structure_changes());
    let mut retained = (0..3)
        .map(|index| (fields & (1 << index) == 0).then_some(index))
        .collect::<Vec<_>>();
    retained.push(Some(3));
    retained.extend(after_calls.iter().map(|offset| {
        before_calls
            .iter()
            .position(|original| original == offset)
            .map(|index| index + 4)
    }));
    assert_eq!(change.retained_commands(), retained);
    for index in 0..3 {
        assert_eq!(
            change.original_command(index).is_none(),
            fields & (1 << index) != 0,
            "changed style writer must be new; unchanged style retains its exact source"
        );
    }
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
    assert_eq!(serde_json::to_vec(&project).unwrap(), sealed);
}

#[test]
fn context_same_column_style_values_with_call_add_delete_are_new_writers() {
    // Each family and the combined edit share the same complete native boundary.
    for changed in [1, 2, 4, 7] {
        for (before, after) in [(&[0][..], &[0, 1][..]), (&[0, 1][..], &[][..])] {
            assert_roundtrip(changed, before, after);
        }
    }
}

#[test]
fn context_style_values_with_call_topology_refuse_reserved_data_and_row_command_replacement() {
    let original = native_fixture(commands(0, &[0]), true);
    let replacement = commands(7, &[0, 1]);
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
    let root = edited.encoded.definition_positions()[0].target();
    for (command, word) in [
        (0, 32 << 11),              // Foreground has only 32 palette entries.
        (1, (3 << 6) | (32 << 11)), // Background has the same checked palette.
        (2, (12 << 6) | (8 << 11)), // Attribute bit3 is reserved.
        (0, 0x28 | (3 << 11)),      // Address40 is a row command, not this style.
    ] {
        let index = edited
            .encoded
            .command_positions()
            .iter()
            .find(|position| {
                position.source().object == Some(root) && position.source().command == command
            })
            .unwrap()
            .semantic_word();
        let mut after = valid.clone();
        after
            .iter_mut()
            .find(|file| file.name == "context.tti")
            .unwrap()
            .bytes = mutate(&edited.document, index, word);
        assert!(
            reconcile_project(&project, &original, &before, &after, limits(), deadline()).is_err(),
            "reserved data or a changed row/column command cannot be a style scalar"
        );
    }
}
