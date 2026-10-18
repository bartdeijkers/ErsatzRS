//! Intent correspondence only; catalogue, call graph and consumer authority remain external.

use super::{
    TeletextColumnOperation, TeletextEnhancementCommand, TeletextEnhancementProgram,
    TeletextLocalObjectGlyphProgramOverlay as Overlay,
    TeletextLocalObjectGlyphProgramOverlayError as Error, TeletextObjectIdentity,
    TeletextPageAddress,
};
use crate::teletext::{
    TeletextColourReference, TeletextEnhancementPosition, TeletextG3Compatibility,
    TeletextGlyphCode, TeletextMosaic, TeletextObjectId, TeletextObjectOffset, TeletextObjectType,
    TeletextPageNumber, TeletextRawDataPageNumber, TeletextResourceIdentity, TeletextResourceKind,
    TeletextResourceSubpageIndex, TeletextSubcode,
};

fn owner() -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(0x124).unwrap(),
        subcode: TeletextSubcode::new(32).unwrap(),
    }
}

fn identity(id: u16) -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(owner(), TeletextObjectId::new(id))
}

fn glyph(column: u8, code: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column,
        operation: TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(code).unwrap(),
        },
    }
}

fn call(
    id: u16,
    object_type: TeletextObjectType,
    offset: Option<TeletextObjectOffset>,
) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::InvokeObject {
        target: identity(id),
        object_type,
        offset,
    }
}

fn commands() -> Vec<TeletextEnhancementCommand> {
    vec![
        TeletextEnhancementCommand::SetPosition {
            position: TeletextEnhancementPosition::new(1, 0).unwrap(),
        },
        glyph(0, b'A'),
        call(1, TeletextObjectType::Passive, None),
        glyph(1, b'B'),
        call(2, TeletextObjectType::Adaptive, None),
    ]
}

fn program(commands: Vec<TeletextEnhancementCommand>) -> TeletextEnhancementProgram {
    TeletextEnhancementProgram::new(commands).unwrap()
}

fn retained(len: usize) -> Vec<Option<usize>> {
    (0..len).map(Some).collect()
}

fn changed() -> Vec<TeletextEnhancementCommand> {
    let mut after = commands();
    after[2] = call(
        1,
        TeletextObjectType::Passive,
        Some(TeletextObjectOffset::new(0, 1).unwrap()),
    );
    after
}

#[test]
fn local_retained_call_offsets_preserve_exact_identity_and_old_constructor_closure() {
    let offsets = [
        None,
        Some(TeletextObjectOffset::new(0, 0).unwrap()),
        Some(TeletextObjectOffset::new(0, 1).unwrap()),
        Some(TeletextObjectOffset::new(1, 0).unwrap()),
    ];
    for before_offset in offsets {
        for after_offset in offsets {
            if before_offset == after_offset {
                continue;
            }
            let mut before = commands();
            before[2] = call(1, TeletextObjectType::Passive, before_offset);
            let mut after = before.clone();
            after[2] = call(1, TeletextObjectType::Passive, after_offset);
            let before = program(before);
            let after = program(after);
            let map = retained(5);
            let overlay = Overlay::new_with_invocation_changes(
                identity(65520),
                before.clone(),
                after.clone(),
                map.clone(),
            )
            .unwrap();
            assert_eq!(overlay.identity(), identity(65520));
            assert_eq!(overlay.owner(), owner());
            assert_eq!(overlay.expected(), &before);
            assert_eq!(overlay.replacement(), &after);
            assert_eq!(overlay.retained_commands(), &map);
            assert_eq!(overlay.original_command(2), Some(2));
            assert_eq!(overlay.replacement_command(2), Some(2));
            assert!(overlay.has_invocation_changes());
            assert!(!overlay.is_style_commands_compatible());
            assert!(!overlay.is_all_glyphs_compatible());
            assert!(!overlay.is_legacy_compatible());
            for constructor in [
                Overlay::new,
                Overlay::new_with_all_glyphs,
                Overlay::new_with_style_commands,
            ] {
                assert_eq!(
                    constructor(identity(65520), before.clone(), after.clone(), map.clone()),
                    Err(Error::ProtectedCommand)
                );
            }
        }
    }
}

#[test]
fn local_retained_call_targets_and_types_are_intent_not_catalogue_authority() {
    for (id, object_type) in [
        (2, TeletextObjectType::Passive),
        (1, TeletextObjectType::Adaptive),
        (2, TeletextObjectType::Active),
    ] {
        let mut after = commands();
        after[2] = call(id, object_type, None);
        let overlay = Overlay::new_with_invocation_changes(
            identity(65520),
            program(commands()),
            program(after.clone()),
            retained(5),
        )
        .unwrap();
        assert_eq!(overlay.replacement().commands()[2], after[2]);
        assert!(overlay.has_invocation_changes());
    }
}

#[test]
fn local_call_changes_can_coexist_with_checked_glyph_and_style_structure() {
    let mut after = changed();
    after.insert(
        3,
        TeletextEnhancementCommand::AtColumn {
            column: 1,
            operation: TeletextColumnOperation::Foreground {
                colour: TeletextColourReference::new(2).unwrap(),
            },
        },
    );
    after[4] = glyph(2, b'C');
    let overlay = Overlay::new_with_invocation_changes(
        identity(65520),
        program(commands()),
        program(after),
        vec![Some(0), Some(1), Some(2), None, Some(3), Some(4)],
    )
    .unwrap();
    assert_eq!(overlay.original_command(3), None);
    assert_eq!(overlay.replacement_command(4), Some(5));
    assert!(overlay.has_invocation_changes());
}

#[test]
fn local_call_constructor_requires_genuinely_changed_retained_invocation() {
    let before = program(commands());
    let mut after = commands();
    after[1] = glyph(0, b'Z');
    let after = program(after);
    let legacy = Overlay::new(identity(65520), before.clone(), after.clone(), retained(5)).unwrap();
    assert!(!legacy.has_invocation_changes());
    assert!(legacy.is_style_commands_compatible());
    assert_eq!(
        Overlay::new_with_invocation_changes(identity(65520), before.clone(), after, retained(5)),
        Err(Error::MissingInvocationChange)
    );
    assert_eq!(
        Overlay::new_with_invocation_changes(identity(65520), before.clone(), before, retained(5)),
        Err(Error::Unchanged)
    );
    assert_eq!(
        Overlay::new_with_invocation_changes(
            identity(65520),
            program(vec![glyph(0, b'A')]),
            program(vec![glyph(0, b'B')]),
            retained(1)
        ),
        Err(Error::MissingInvocationChange)
    );
}

#[test]
fn local_retained_call_policy_rejects_call_insertion_deletion_and_false_maps() {
    let mut inserted = changed();
    inserted.insert(3, call(1, TeletextObjectType::Passive, None));
    assert_eq!(
        Overlay::new_with_invocation_changes(
            identity(65520),
            program(commands()),
            program(inserted),
            vec![Some(0), Some(1), Some(2), None, Some(3), Some(4)]
        ),
        Err(Error::ProtectedCommand)
    );

    let mut deleted = changed();
    deleted.remove(4);
    assert_eq!(
        Overlay::new_with_invocation_changes(
            identity(65520),
            program(commands()),
            program(deleted),
            retained(4)
        ),
        Err(Error::ProtectedCommand)
    );

    for map in [
        vec![Some(0), Some(1), Some(2), Some(3)],
        vec![Some(0), Some(1), Some(2), Some(2), Some(4)],
        vec![Some(0), Some(1), Some(2), Some(3), Some(5)],
    ] {
        assert_eq!(
            Overlay::new_with_invocation_changes(
                identity(65520),
                program(commands()),
                program(changed()),
                map
            ),
            Err(Error::Correspondence)
        );
    }
    let mut false_call = changed();
    false_call[1] = call(1, TeletextObjectType::Passive, None);
    assert_eq!(
        Overlay::new_with_invocation_changes(
            identity(65520),
            program(commands()),
            program(false_call),
            retained(5)
        ),
        Err(Error::ProtectedCommand)
    );
}

#[test]
fn local_retained_call_policy_preserves_protected_anchors_and_glyph_families() {
    let mut anchor = changed();
    anchor[0] = TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(2, 0).unwrap(),
    };
    let mut cross_family = changed();
    cross_family[1] = TeletextEnhancementCommand::AtColumn {
        column: 0,
        operation: TeletextColumnOperation::G1 {
            mosaic: TeletextMosaic::new(1).unwrap(),
        },
    };
    for after in [anchor, cross_family] {
        assert_eq!(
            Overlay::new_with_invocation_changes(
                identity(65520),
                program(commands()),
                program(after),
                retained(5)
            ),
            Err(Error::ProtectedCommand)
        );
    }
    let g3 = |compatibility| TeletextEnhancementCommand::AtColumn {
        column: 0,
        operation: TeletextColumnOperation::G3 {
            code: TeletextGlyphCode::new(0x21).unwrap(),
            compatibility,
        },
    };
    let mut before = commands();
    before[1] = g3(TeletextG3Compatibility::Level15);
    let mut after = changed();
    after[1] = g3(TeletextG3Compatibility::Level25);
    assert_eq!(
        Overlay::new_with_invocation_changes(
            identity(65520),
            program(before),
            program(after),
            retained(5)
        ),
        Err(Error::ProtectedCommand)
    );
}

#[test]
fn local_retained_call_policy_preserves_exact_208_capacity_and_local_scope() {
    let mut before = vec![glyph(0, b'A'); 207];
    before.push(call(1, TeletextObjectType::Passive, None));
    let mut after = before.clone();
    after[207] = call(
        1,
        TeletextObjectType::Passive,
        Some(TeletextObjectOffset::new(0, 1).unwrap()),
    );
    let overlay = Overlay::new_with_invocation_changes(
        identity(65520),
        program(before.clone()),
        program(after.clone()),
        retained(208),
    )
    .unwrap();
    assert_eq!(overlay.expected().commands().len(), 208);
    assert_eq!(overlay.original_command(207), Some(207));
    for (original, replacement, map) in [
        (
            {
                let mut value = before.clone();
                value.push(glyph(0, b'A'));
                value
            },
            after.clone(),
            retained(208),
        ),
        (
            before.clone(),
            {
                let mut value = after.clone();
                value.push(glyph(0, b'A'));
                value
            },
            retained(209),
        ),
        (before.clone(), after.clone(), retained(209)),
    ] {
        assert_eq!(
            Overlay::new_with_invocation_changes(
                identity(65520),
                program(original),
                program(replacement),
                map
            ),
            Err(Error::Capacity)
        );
    }
    let resource = TeletextResourceIdentity::new(
        TeletextResourceKind::Pop,
        TeletextRawDataPageNumber::new(1, 0xa0).unwrap(),
        TeletextResourceSubpageIndex::new(1).unwrap(),
    )
    .unwrap();
    let public = TeletextObjectIdentity::public(resource, TeletextObjectId::new(65520)).unwrap();
    assert_eq!(
        Overlay::new_with_invocation_changes(
            public,
            program(before),
            program(after),
            retained(208)
        ),
        Err(Error::Scope)
    );
}
