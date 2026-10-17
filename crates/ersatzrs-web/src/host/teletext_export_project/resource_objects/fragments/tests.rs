//! Reuse the unchanged V6 real-native fixture and literal wrapper oracles.
use super::*;
use ersatzrs_domain::{
    TeletextDrcsGlyphReference, TeletextDrcsGlyphSlot, TeletextDrcsSubtableIndex,
    TeletextDrcsTableScope,
};

fn definition(
    target: TeletextObjectIdentity,
    kind: TeletextObjectType,
    commands: Vec<TeletextEnhancementCommand>,
) -> TeletextEnhancementObjectDefinition {
    TeletextEnhancementObjectDefinition {
        identity: target,
        object_type: kind,
        program: TeletextEnhancementProgram::new(commands).unwrap(),
    }
}

fn reconstruct(
    page: &EncodedTeletextAuthoredPage,
    definition: &TeletextEnhancementObjectDefinition,
) -> Vec<ObjectEditorFragmentPlan> {
    let original = page.data_units().to_vec();
    let plans = plan_fragments(page, definition, 43, deadline()).unwrap();
    assert!(!plans.is_empty());
    let (_, body) = body_range(resource(page), definition.identity, deadline()).unwrap();
    let projection = project_encoded_teletext_object(page, 43).unwrap();
    let expected: Vec<_> = body
        .clone()
        .map(|index| native_word(&projection, index).unwrap())
        .collect();
    let (marker, _) = wrapper(definition.object_type).unwrap();
    let mut actual = Vec::new();
    let mut command_end = 0;
    let mut word_end = u16::try_from(body.start).unwrap();
    for (index, plan) in plans.iter().enumerate() {
        assert_eq!(usize::from(plan.fragment_index), index);
        assert_eq!(usize::from(plan.fragment_count), plans.len());
        assert_eq!(plan.identity, definition.identity);
        assert_eq!(plan.object_type, definition.object_type);
        assert_eq!(plan.command_start, command_end);
        assert_eq!(plan.native_word_start, word_end);
        command_end = plan.command_end;
        word_end = plan.native_word_end;
        let frame = build_fragment(page, definition, plan, 43, budget(), deadline()).unwrap();
        let words = words(&frame);
        assert!(words.len() <= 208 && words.len().is_multiple_of(13));
        let count = usize::from(plan.native_word_end - plan.native_word_start);
        actual.extend_from_slice(&words[marker + 1..marker + 1 + count]);
        assert!(words[marker + 1 + count..]
            .iter()
            .all(|word| *word == 0x2fff));
        assert_eq!(
            frame.document.records()[0],
            TeletextTtiRecord::Description(format!(
                "Object command fragment {}/{}; use native preview for full context.",
                index + 1,
                plans.len()
            ))
        );
        for position in frame.command_positions {
            assert!((plan.command_start..plan.command_end)
                .contains(&u64::try_from(position.command).unwrap()));
            let native = resource(page)
                .command_positions()
                .iter()
                .find(|native| {
                    native.source().object == Some(definition.identity)
                        && native.source().command == position.command
                })
                .unwrap();
            assert_eq!(
                position.editor_word,
                marker + 1 + native.semantic_word() - usize::from(plan.native_word_start)
            );
        }
    }
    assert_eq!(
        command_end,
        u64::try_from(definition.program.commands().len()).unwrap()
    );
    assert_eq!(usize::from(word_end), body.end);
    assert_eq!(actual, expected);
    assert_eq!(page.data_units(), original);
    plans
}

#[test]
fn complete_native_capacity_fragments_reconstruct_all_scopes_types_and_pointer_extents() {
    for scope in [TeletextResourceKind::Pop, TeletextResourceKind::Gpop] {
        for kind in [
            TeletextObjectType::Active,
            TeletextObjectType::Adaptive,
            TeletextObjectType::Passive,
        ] {
            for (objects, capacity) in [(1, 507_usize), (17, 494), (25, 481)] {
                let commands = vec![foreground(); capacity - usize::from(objects) - 1];
                let (page, target) = encoded(scope, kind, objects, &commands);
                assert_eq!(page.data_units().len(), 43);
                let definition = definition(target, kind, commands);
                assert_eq!(definition.identity.object_id().get(), 65535);
                assert_eq!(
                    definition
                        .identity
                        .resource()
                        .unwrap()
                        .logical_subpage()
                        .get(),
                    15
                );
                assert!(requires_fragments(&page, deadline()).unwrap());
                let plans = reconstruct(&page, &definition);
                assert!(plans.len() > 1);
            }
        }
    }
}

#[test]
fn fragment_boundaries_never_split_compound_selectors_and_keep_hidden_ordinals() {
    let hidden = TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(0, 0).unwrap(),
    };
    let mut commands = vec![hidden];
    commands.extend(vec![foreground(); 203]);
    // Forward row-zero is also invisible; it belongs to the previous member.
    commands.push(TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(0, 9).unwrap(),
    });
    commands.push(TeletextEnhancementCommand::AtColumn {
        column: 9,
        operation: TeletextColumnOperation::Drcs {
            reference: TeletextDrcsGlyphReference {
                table: TeletextDrcsTableScope::Normal,
                subtable: TeletextDrcsSubtableIndex::new(1).unwrap(),
                slot: TeletextDrcsGlyphSlot::new(0).unwrap(),
            },
        },
    });
    commands.push(TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(0, 10).unwrap(),
    });
    let (page, target) = encoded(
        TeletextResourceKind::Pop,
        TeletextObjectType::Passive,
        1,
        &commands,
    );
    let definition = definition(target, TeletextObjectType::Passive, commands);
    let plans = reconstruct(&page, &definition);
    assert_eq!(plans.len(), 2);
    assert_eq!((plans[0].command_start, plans[0].command_end), (0, 205));
    assert_eq!((plans[1].command_start, plans[1].command_end), (205, 207));
    assert_eq!(plans[0].native_word_end - plans[0].native_word_start, 203);
    assert_eq!(plans[1].native_word_end - plans[1].native_word_start, 2);
    let source = resource(&page)
        .command_positions()
        .iter()
        .find(|position| position.source().command == 205)
        .unwrap();
    assert_eq!(source.first_word() + 1, source.semantic_word());
    assert_eq!(usize::from(plans[1].native_word_start), source.first_word());
}

#[test]
fn empty_hidden_and_208_word_members_preserve_v6_bytes_and_require_exact_plans() {
    for count in [0, 204, 205] {
        let commands = vec![foreground(); count];
        let (page, target) = encoded(
            TeletextResourceKind::Gpop,
            TeletextObjectType::Active,
            1,
            &commands,
        );
        let definition = definition(target, TeletextObjectType::Active, commands);
        let plans = reconstruct(&page, &definition);
        assert_eq!(requires_fragments(&page, deadline()).unwrap(), count > 204);
        if count <= 204 {
            let before = build_frame(&page, target, 43, budget(), deadline()).unwrap();
            let after =
                build_fragment(&page, &definition, &plans[0], 43, budget(), deadline()).unwrap();
            assert_eq!(words(&before), words(&after));
            assert_eq!(&after.document.records()[1..], before.document.records());
        }
        let mut forged = plans[0];
        forged.command_end += 1;
        assert!(matches!(
            build_fragment(&page, &definition, &forged, 43, budget(), deadline()),
            Err(Error::Association)
        ));
        let mut forged = plans[0];
        forged.native_word_start += 1;
        assert!(matches!(
            build_fragment(&page, &definition, &forged, 43, budget(), deadline()),
            Err(Error::Association)
        ));
        assert!(matches!(
            plan_fragments(&page, &definition, 0, deadline()),
            Err(Error::Budget)
        ));
        assert!(matches!(
            plan_fragments(&page, &definition, 43, Instant::now()),
            Err(Error::TimedOut)
        ));
        let tiny = TeletextTtiBudget {
            max_records: 7,
            ..budget()
        };
        assert!(matches!(
            build_fragment(&page, &definition, &plans[0], 43, tiny, deadline()),
            Err(Error::Budget)
        ));
    }
    let commands = vec![
        TeletextEnhancementCommand::SetPosition {
            position: TeletextEnhancementPosition::new(0, 0).unwrap(),
        };
        3
    ];
    let (page, target) = encoded(
        TeletextResourceKind::Pop,
        TeletextObjectType::Passive,
        1,
        &commands,
    );
    let mut definition = definition(target, TeletextObjectType::Passive, commands);
    let plans = reconstruct(&page, &definition);
    assert_eq!((plans[0].command_start, plans[0].command_end), (0, 3));
    assert_eq!(plans[0].native_word_start, plans[0].native_word_end);
    definition.program = TeletextEnhancementProgram::new(vec![foreground()]).unwrap();
    assert!(matches!(
        plan_fragments(&page, &definition, 43, deadline()),
        Err(Error::Association)
    ));
}
