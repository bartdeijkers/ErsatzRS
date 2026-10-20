//! Complete bank lifecycle, simultaneous original-bank Add and repeat ordering.
use super::*;
use ersatzrs_app::teletext_design_documents::{
    TeletextSavedObjectResourceAddition, TeletextSavedResourceTransmission,
    TeletextSavedResourceUpdate,
};

fn bank(global: bool, empty: bool) -> TeletextSavedObjectResourceAddition {
    let resource = TeletextResourceIdentity::new(
        if global {
            TeletextResourceKind::Gpop
        } else {
            TeletextResourceKind::Pop
        },
        TeletextRawDataPageNumber::new(1, if global { 0xa3 } else { 0xa2 }).unwrap(),
        TeletextResourceSubpageIndex::new(3).unwrap(),
    )
    .unwrap();
    let identity = if global {
        TeletextObjectIdentity::global(resource, TeletextObjectId::new(65530))
    } else {
        TeletextObjectIdentity::public(resource, TeletextObjectId::new(65530))
    }
    .unwrap();
    let definitions = if empty {
        vec![]
    } else {
        vec![TeletextEnhancementObjectDefinition {
            identity,
            object_type: TeletextObjectType::Passive,
            program: TeletextEnhancementProgram::new(vec![at(0, glyph(b'B'))]).unwrap(),
        }]
    };
    TeletextSavedObjectResourceAddition::new(
        0,
        TeletextObjectResourceAddition::new(resource, definitions, policy().validation.service)
            .unwrap(),
        TeletextSavedResourceTransmission {
            resource,
            counter: TeletextResourceUpdateCounter::new(7).unwrap(),
            update: TeletextSavedResourceUpdate::Unchanged,
            last_subpage: false,
        },
    )
    .unwrap()
}
fn definition() -> TeletextResourceObjectDefinitionAddition {
    TeletextResourceObjectDefinitionAddition::new(
        TeletextEnhancementObjectDefinition {
            identity: target(false, 65522),
            object_type: TeletextObjectType::Passive,
            program: TeletextEnhancementProgram::new(vec![at(0, glyph(b'C'))]).unwrap(),
        },
        policy().validation.service,
    )
    .unwrap()
}
fn compile_added(
    root: &EncodedTeletextAuthoredService,
    banks: &[TeletextSavedObjectResourceAddition],
    definitions: &[TeletextResourceObjectDefinitionAddition],
) -> EncodedTeletextAuthoredService {
    let additions: Vec<_> = banks.iter().map(|bank| bank.addition().clone()).collect();
    let declared = root
        .declared()
        .clone()
        .with_object_resource_and_definition_additions(
            &additions,
            definitions,
            policy().validation.service,
            policy().validation.declarations,
        )
        .unwrap();
    let transmissions: Vec<_> = declared
        .resource_inventory()
        .iter()
        .map(|resource| {
            banks
                .iter()
                .find(|bank| bank.addition().resource() == *resource)
                .map_or(
                    TeletextServiceResourceTransmission {
                        resource: *resource,
                        counter: TeletextResourceUpdateCounter::new(1).unwrap(),
                        update: TeletextResourceFullUpdate::Changed,
                        last_subpage: true,
                    },
                    |bank| {
                        let tx = bank.transmission();
                        TeletextServiceResourceTransmission {
                            resource: tx.resource,
                            counter: tx.counter,
                            update: match tx.update {
                                TeletextSavedResourceUpdate::Changed => {
                                    TeletextResourceFullUpdate::Changed
                                }
                                TeletextSavedResourceUpdate::Unchanged => {
                                    TeletextResourceFullUpdate::Unchanged
                                }
                            },
                            last_subpage: tx.last_subpage,
                        }
                    },
                )
        })
        .collect();
    encode_teletext_authored_service_before(
        declared,
        &transmissions,
        &policy().encoding,
        deadline(),
    )
    .unwrap()
}
fn bank_edits<'a>(
    banks: &'a [TeletextSavedObjectResourceAddition],
    additions: &'a [TeletextResourceObjectDefinitionAddition],
) -> TeletextEditorialEdits<'a> {
    let mut value = edits(&[]);
    value.object_resource_additions = banks;
    value.resource_additions = additions;
    value
}
fn qualifies(
    before: &EncodedTeletextAuthoredService,
    after: &EncodedTeletextAuthoredService,
    incoming: TeletextEditorialEdits<'_>,
    cumulative: TeletextEditorialEdits<'_>,
) -> Result<()> {
    validate_teletext_editorial_transition_with_operands(
        before,
        after,
        &permissions(true),
        incoming,
        cumulative,
        &policy().encoding,
        deadline(),
    )
}

#[test]
fn object_bank_effects_authenticate_complete_pop_gpop_empty_and_nonempty_headers_and_repeat_order()
{
    let root = compile(Fixture::default());
    let definitions = [definition()];
    for empty in [false, true] {
        let first = [bank(false, empty)];
        let latest = compile_added(&root, &first, &definitions);
        qualifies(
            &root,
            &latest,
            bank_edits(&first, &definitions),
            bank_edits(&first, &definitions),
        )
        .unwrap();
        let all = [first[0].clone(), bank(true, !empty)];
        let final_service = compile_added(&root, &all, &definitions);
        qualifies(
            &root,
            &final_service,
            bank_edits(&all, &definitions),
            bank_edits(&all, &definitions),
        )
        .unwrap();
        qualifies(
            &latest,
            &final_service,
            bank_edits(&all[1..], &[]),
            bank_edits(&all, &definitions),
        )
        .unwrap();
        qualifies(
            &final_service,
            &final_service,
            bank_edits(&[], &[]),
            bank_edits(&all, &definitions),
        )
        .unwrap();
    }
}

#[test]
fn object_bank_effects_refuse_missing_extra_wrong_definition_and_lifecycle_authority() {
    let root = compile(Fixture::default());
    let banks = [bank(false, false), bank(true, true)];
    let definitions = [definition()];
    let after = compile_added(&root, &banks, &definitions);
    for (incoming, cumulative) in [
        (
            bank_edits(&[], &definitions),
            bank_edits(&banks, &definitions),
        ),
        (
            bank_edits(&banks[..1], &definitions),
            bank_edits(&banks, &definitions),
        ),
        (
            bank_edits(&banks, &definitions),
            bank_edits(&banks[..1], &definitions),
        ),
        (bank_edits(&banks, &[]), bank_edits(&banks, &[])),
    ] {
        assert!(qualifies(&root, &after, incoming, cumulative).is_err());
    }
    let mut altered = banks[0].addition().definitions().to_vec();
    altered[0].program = TeletextEnhancementProgram::new(vec![at(0, glyph(b'D'))]).unwrap();
    let changed_program = [
        TeletextSavedObjectResourceAddition::new(
            0,
            TeletextObjectResourceAddition::new(
                banks[0].addition().resource(),
                altered,
                policy().validation.service,
            )
            .unwrap(),
            banks[0].transmission(),
        )
        .unwrap(),
        banks[1].clone(),
    ];
    assert!(qualifies(
        &root,
        &after,
        bank_edits(&changed_program, &definitions),
        bank_edits(&changed_program, &definitions)
    )
    .is_err());
    let lifecycle = banks[0].transmission();
    for forged_lifecycle in [
        TeletextSavedResourceTransmission {
            update: TeletextSavedResourceUpdate::Changed,
            ..lifecycle
        },
        TeletextSavedResourceTransmission {
            counter: TeletextResourceUpdateCounter::new(6).unwrap(),
            ..lifecycle
        },
        TeletextSavedResourceTransmission {
            last_subpage: true,
            ..lifecycle
        },
    ] {
        let forged = [
            TeletextSavedObjectResourceAddition::new(
                0,
                banks[0].addition().clone(),
                forged_lifecycle,
            )
            .unwrap(),
            banks[1].clone(),
        ];
        assert!(qualifies(
            &root,
            &after,
            bank_edits(&forged, &definitions),
            bank_edits(&forged, &definitions)
        )
        .is_err());
    }
    let original = bank_edits(&banks, &definitions);
    let mutated_root = compile(Fixture {
        other_code: b'Q',
        ..Fixture::default()
    });
    let unrelated = compile_added(&mutated_root, &banks, &definitions);
    assert!(qualifies(&root, &unrelated, original, original).is_err());
    let changed_header_root = compile(Fixture {
        counter: 2,
        ..Fixture::default()
    });
    assert!(qualifies(&changed_header_root, &after, original, original).is_err());
}

#[test]
fn object_bank_effects_bound_raw_commands_and_reject_original_bank_add_into_new_bank() {
    let root = compile(Fixture::default());
    let banks = [bank(false, false)];
    let after = compile_added(&root, &banks, &[]);
    let edits = bank_edits(&banks, &[]);
    let mut budget = policy().encoding;
    budget.max_commands = 0;
    assert_eq!(
        validate_teletext_editorial_effects_with_operands(
            &root,
            &after,
            &permissions(true),
            edits,
            &budget,
            deadline()
        )
        .unwrap_err()
        .reason,
        TeletextEditorialEffectsFailure::Budget
    );
    let false_add = [TeletextResourceObjectDefinitionAddition::new(
        banks[0].addition().definitions()[0].clone(),
        policy().validation.service,
    )
    .unwrap()];
    assert!(qualifies(
        &root,
        &after,
        bank_edits(&banks, &false_add),
        bank_edits(&banks, &false_add)
    )
    .is_err());
    let duplicate = [banks[0].clone(), banks[0].clone()];
    assert!(qualifies(
        &root,
        &after,
        bank_edits(&duplicate, &[]),
        bank_edits(&duplicate, &[])
    )
    .is_err());
}
