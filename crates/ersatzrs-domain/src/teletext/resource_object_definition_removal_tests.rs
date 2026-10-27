//! The inverse catalogue operation keeps all consumers and resource identities.
use super::*;

#[test]
fn unused_resource_removal_restores_complete_original_after_closed_subtree_removal() {
    let original = service(input());
    let additions = additions();
    let removals: Vec<_> = additions
        .iter()
        .map(TeletextResourceObjectDefinitionAddition::identity)
        .collect();
    let extended = original
        .clone()
        .with_resource_object_definition_additions(&additions, budget())
        .unwrap();
    let restored = extended
        .without_unused_resource_object_definitions(&removals, budget())
        .unwrap();
    assert_eq!(restored, original);
}

#[test]
fn unused_resource_removal_refuses_surviving_dormant_callers_main_and_suppressed_defaults() {
    let original = service(input());
    let extended = original
        .clone()
        .with_resource_object_definition_additions(&additions(), budget())
        .unwrap();
    // The unused Adaptive definition still references this otherwise unused leaf.
    let leaf = identity(false, 0xa0, 1, 25);
    assert_eq!(
        extended
            .clone()
            .without_unused_resource_object_definitions(&[leaf], budget()),
        Err(TeletextResourceObjectDefinitionRemovalError::Referenced { identity: leaf })
    );
    let main = identity(false, 0xa0, 1, 19);
    assert_eq!(
        original
            .clone()
            .without_unused_resource_object_definitions(&[main], budget()),
        Err(TeletextResourceObjectDefinitionRemovalError::Referenced { identity: main })
    );
    let mut value = input();
    for page in &mut value.pages {
        page.local_program = Some(program(vec![]));
    }
    let suppressed = service(value);
    let default = identity(true, 0xa1, 0, 19);
    assert_eq!(
        suppressed.without_unused_resource_object_definitions(&[default], budget()),
        Err(TeletextResourceObjectDefinitionRemovalError::Referenced { identity: default })
    );
    assert_eq!(original, service(input()));
}

#[test]
fn unused_resource_removal_preserves_surviving_definition_order_and_exact_partial_resource() {
    let original = service(input());
    let extended = original
        .clone()
        .with_resource_object_definition_additions(&additions(), budget())
        .unwrap();
    let leaf = identity(false, 0xa2, 3, 28);
    let changed = extended
        .clone()
        .without_unused_resource_object_definitions(&[leaf], budget())
        .unwrap();
    let expected: Vec<_> = extended
        .objects()
        .definitions()
        .iter()
        .filter(|definition| definition.identity != leaf)
        .cloned()
        .collect();
    assert_eq!(changed.objects().definitions(), expected);
    assert_eq!(
        changed.object_companions(),
        &[resource(TeletextResourceKind::Pop, 0xa2, 3)]
    );
    assert_eq!(changed.base(), original.base());
    assert_eq!(changed.page_enhancements(), original.page_enhancements());
    assert_eq!(
        changed.physical_base_overlays(),
        original.physical_base_overlays()
    );
    assert_eq!(
        changed.base_text_spellings(),
        original.base_text_spellings()
    );
    assert_eq!(changed.drcs(), original.drcs());
}

#[test]
fn unused_resource_removal_rejects_invalid_batches_and_rechecks_existing_budgets() {
    let service = service(input());
    let target = identity(false, 0xa0, 1, 19);
    let missing = identity(false, 0xa0, 1, 99);
    let local = TeletextObjectIdentity::local(address(200, 0), TeletextObjectId::new(99));
    for (batch, expected) in [
        (vec![], TeletextResourceObjectDefinitionRemovalError::Empty),
        (
            vec![local],
            TeletextResourceObjectDefinitionRemovalError::Scope { identity: local },
        ),
        (
            vec![missing],
            TeletextResourceObjectDefinitionRemovalError::Missing { identity: missing },
        ),
        (
            vec![target, target],
            TeletextResourceObjectDefinitionRemovalError::Duplicate { identity: target },
        ),
    ] {
        assert_eq!(
            service
                .clone()
                .without_unused_resource_object_definitions(&batch, budget()),
            Err(expected)
        );
    }
    for limits in [
        TeletextServiceBudget {
            max_objects: 0,
            ..budget()
        },
        TeletextServiceBudget {
            max_structure_items: 0,
            ..budget()
        },
        TeletextServiceBudget {
            max_commands: 0,
            ..budget()
        },
    ] {
        assert!(matches!(
            service
                .clone()
                .without_unused_resource_object_definitions(&[target], limits),
            Err(TeletextResourceObjectDefinitionRemovalError::Service(_))
        ));
    }
}
