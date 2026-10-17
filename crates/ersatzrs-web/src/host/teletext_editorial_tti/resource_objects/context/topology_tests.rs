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
#[test]
fn context_call_topology_keeps_unowned_noncall_changes_closed() {
    let original = native_fixture(root_with(&[0]), true);
    let project = receipt(&original);
    let baseline =
        build_context(&original, target(21), limits().1, limits().0, deadline()).unwrap();
    let mut commands = root_with(&[0, 1]).commands().to_vec();
    commands[0] = glyph(0, b'B');
    let edited = build_context_with_root(
        &original,
        target(21),
        &program(commands),
        limits().1,
        limits().0,
        deadline(),
    )
    .unwrap();
    assert!(reconcile_project(
        &project,
        &original,
        &files(&original, &project, &baseline.document),
        &files(&original, &project, &edited.document),
        limits(),
        deadline()
    )
    .is_err());
}
