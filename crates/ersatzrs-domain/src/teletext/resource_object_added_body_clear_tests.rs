//! Emptying a checked addition grants no original-position or saved-program role.
use super::*;

fn addition(
    global: bool,
    object_type: TeletextObjectType,
) -> TeletextResourceObjectDefinitionAddition {
    TeletextResourceObjectDefinitionAddition::new(
        definition(
            target(global, 15),
            object_type,
            vec![
                TeletextEnhancementCommand::SetPosition {
                    position: TeletextEnhancementPosition::new(1, 0).unwrap(),
                },
                charset(0),
                at(8, glyph(b'Q')),
                invoke(target(!global, 1), TeletextObjectType::Passive),
            ],
        ),
        budget(),
    )
    .unwrap()
}

#[test]
fn clear_added_body_preserves_pop_gpop_identity_all_types_and_ephemeral_only_intent() {
    for global in [false, true] {
        for object_type in [
            TeletextObjectType::Active,
            TeletextObjectType::Adaptive,
            TeletextObjectType::Passive,
        ] {
            let added = addition(global, object_type);
            let unchanged = added.clone();
            let clear = TeletextResourceObjectProgramOverlay::clear_added_body(
                &added,
                program(vec![]),
                budget(),
            )
            .unwrap();
            assert_eq!(added, unchanged);
            assert_eq!(added.definition().object_type, object_type);
            assert_eq!(clear.identity(), added.identity());
            assert_eq!(clear.expected(), &added.definition().program);
            assert!(clear.replacement().commands().is_empty());
            assert!(clear.retained_commands().is_empty());
            assert!(clear.is_position_restoration());
            assert!(!clear.is_original_position_intent());
            assert!(!clear.has_inserted_positions());
            for ordinal in 0..clear.expected().commands().len() {
                assert_eq!(clear.replacement_command(ordinal), None);
            }
            assert_eq!(clear.original_command(0), None);
            assert_eq!(
                TeletextResourceObjectProgramOverlay::new(
                    added.identity(),
                    added.definition().program.clone(),
                    program(vec![]),
                    vec![],
                    budget(),
                ),
                Err(TeletextResourceObjectProgramOverlayError::Correspondence(
                    TeletextLocalObjectGlyphProgramOverlayError::ProtectedCommand
                ))
            );
            assert_eq!(
                TeletextResourceObjectProgramOverlay::restore_original_positions(&clear, budget()),
                Err(TeletextResourceObjectProgramOverlayError::OriginalPositionIntent)
            );
        }
    }
}

#[test]
fn clear_added_body_requires_exact_fresh_bounds_and_nonempty_to_empty_edge() {
    let added = addition(false, TeletextObjectType::Passive);
    let exact = TeletextServiceBudget {
        max_objects: 1,
        max_structure_items: 1,
        max_commands: added.definition().program.commands().len(),
        ..budget()
    };
    assert!(
        TeletextResourceObjectProgramOverlay::clear_added_body(&added, program(vec![]), exact,)
            .is_ok()
    );
    for bounded in [
        TeletextServiceBudget {
            max_objects: 0,
            ..exact
        },
        TeletextServiceBudget {
            max_structure_items: 0,
            ..exact
        },
        TeletextServiceBudget {
            max_commands: exact.max_commands - 1,
            ..exact
        },
    ] {
        assert_eq!(
            TeletextResourceObjectProgramOverlay::clear_added_body(
                &added,
                program(vec![]),
                bounded
            ),
            Err(TeletextResourceObjectProgramOverlayError::Budget)
        );
    }
    assert_eq!(
        TeletextResourceObjectProgramOverlay::clear_added_body(
            &added,
            program(vec![at(8, glyph(b'R'))]),
            exact,
        ),
        Err(TeletextResourceObjectProgramOverlayError::NonemptyAddedBody)
    );
    let mut empty_definition = added.definition().clone();
    empty_definition.program = program(vec![]);
    let empty = TeletextResourceObjectDefinitionAddition::new(empty_definition, exact).unwrap();
    assert_eq!(
        TeletextResourceObjectProgramOverlay::clear_added_body(&empty, program(vec![]), exact),
        Err(TeletextResourceObjectProgramOverlayError::Unchanged)
    );
}
