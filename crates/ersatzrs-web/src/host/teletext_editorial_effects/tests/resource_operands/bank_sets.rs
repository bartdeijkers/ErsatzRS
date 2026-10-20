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

fn bank_definition(
    bank: &TeletextSavedObjectResourceAddition,
    id: u16,
    code: u8,
) -> TeletextResourceObjectDefinitionAddition {
    let resource = bank.addition().resource();
    let identity = if resource.kind() == TeletextResourceKind::Gpop {
        TeletextObjectIdentity::global(resource, TeletextObjectId::new(id))
    } else {
        TeletextObjectIdentity::public(resource, TeletextObjectId::new(id))
    }
    .unwrap();
    TeletextResourceObjectDefinitionAddition::new(
        TeletextEnhancementObjectDefinition {
            identity,
            object_type: TeletextObjectType::Passive,
            program: TeletextEnhancementProgram::new(vec![at(0, glyph(code))]).unwrap(),
        },
        policy().validation.service,
    )
    .unwrap()
}
fn extend_bank(
    bank: &TeletextSavedObjectResourceAddition,
    additions: &[TeletextResourceObjectDefinitionAddition],
) -> TeletextSavedObjectResourceAddition {
    let definitions = bank
        .addition()
        .definitions()
        .iter()
        .cloned()
        .chain(
            additions
                .iter()
                .map(|addition| addition.definition().clone()),
        )
        .collect();
    TeletextSavedObjectResourceAddition::new(
        bank.document_index(),
        TeletextObjectResourceAddition::new(
            bank.addition().resource(),
            definitions,
            policy().validation.service,
        )
        .unwrap(),
        bank.transmission(),
    )
    .unwrap()
}
fn ordinary_second() -> TeletextResourceObjectDefinitionAddition {
    let mut value = definition().definition().clone();
    value.identity = target(false, 65523);
    TeletextResourceObjectDefinitionAddition::new(value, policy().validation.service).unwrap()
}

#[test]
fn retained_bank_definition_effects_append_empty_and_populated_banks_before_old_original_additions()
{
    let root = compile(Fixture::default());
    let banks = [bank(false, true), bank(true, false)];
    let old_additions = [definition()];
    let before = compile_added(&root, &banks, &old_additions);
    let deltas = [
        bank_definition(&banks[0], 65531, b'D'),
        bank_definition(&banks[1], 65531, b'E'),
        ordinary_second(),
    ];
    let complete = [
        extend_bank(&banks[0], &deltas[..1]),
        extend_bank(&banks[1], &deltas[1..2]),
    ];
    let ordinary = [old_additions[0].clone(), deltas[2].clone()];
    let after = compile_added(&root, &complete, &ordinary);
    qualifies(
        &before,
        &after,
        bank_edits(&[], &deltas),
        bank_edits(&complete, &ordinary),
    )
    .unwrap();
    qualifies(
        &root,
        &after,
        bank_edits(&complete, &ordinary),
        bank_edits(&complete, &ordinary),
    )
    .unwrap();
    qualifies(
        &after,
        &after,
        bank_edits(&[], &[]),
        bank_edits(&complete, &ordinary),
    )
    .unwrap();
    for (old, new) in banks.iter().zip(&complete) {
        assert_eq!(old.document_index(), new.document_index());
        assert_eq!(old.transmission(), new.transmission());
    }
    let reordered_ordinary = [ordinary[1].clone(), ordinary[0].clone()];
    let reordered_service = compile_added(&root, &complete, &reordered_ordinary);
    assert!(
        qualifies(
            &before,
            &reordered_service,
            bank_edits(&[], &deltas),
            bank_edits(&complete, &reordered_ordinary),
        )
        .is_err(),
        "bank insertions do not allow moving an ordinary addition ahead of an older identity"
    );
}

#[test]
fn retained_bank_definition_effects_reject_unaccounted_reordered_stale_or_occupied_deltas() {
    let root = compile(Fixture::default());
    let banks = [bank(false, true), bank(true, false)];
    let ordinary = [definition()];
    let before = compile_added(&root, &banks, &ordinary);
    let delta = [bank_definition(&banks[1], 65531, b'D')];
    let complete = [banks[0].clone(), extend_bank(&banks[1], &delta)];
    let after = compile_added(&root, &complete, &ordinary);
    let duplicate = [delta[0].clone(), delta[0].clone()];
    let wrong = [bank_definition(&banks[1], 65531, b'E')];
    let occupied = [bank_definition(&banks[1], 65530, b'B')];
    for (incoming, cumulative) in [
        (bank_edits(&[], &[]), bank_edits(&complete, &ordinary)),
        (
            bank_edits(&[], &duplicate),
            bank_edits(&complete, &ordinary),
        ),
        (bank_edits(&[], &wrong), bank_edits(&complete, &ordinary)),
        (bank_edits(&[], &occupied), bank_edits(&complete, &ordinary)),
        (bank_edits(&[], &delta), bank_edits(&banks, &ordinary)),
    ] {
        assert!(qualifies(&before, &after, incoming, cumulative).is_err());
    }
    let mut reversed = complete[1].addition().definitions().to_vec();
    reversed.reverse();
    let reordered = [
        complete[0].clone(),
        TeletextSavedObjectResourceAddition::new(
            complete[1].document_index(),
            TeletextObjectResourceAddition::new(
                complete[1].addition().resource(),
                reversed,
                policy().validation.service,
            )
            .unwrap(),
            complete[1].transmission(),
        )
        .unwrap(),
    ];
    let after_reordered = compile_added(&root, &reordered, &ordinary);
    assert!(qualifies(
        &before,
        &after_reordered,
        bank_edits(&[], &delta),
        bank_edits(&reordered, &ordinary)
    )
    .is_err());
    let mut lifecycle = complete[1].transmission();
    lifecycle.last_subpage = !lifecycle.last_subpage;
    let changed_lifecycle = [
        complete[0].clone(),
        TeletextSavedObjectResourceAddition::new(
            complete[1].document_index(),
            complete[1].addition().clone(),
            lifecycle,
        )
        .unwrap(),
    ];
    let after_lifecycle = compile_added(&root, &changed_lifecycle, &ordinary);
    assert!(qualifies(
        &before,
        &after_lifecycle,
        bank_edits(&[], &delta),
        bank_edits(&changed_lifecycle, &ordinary)
    )
    .is_err());
}

#[test]
fn retained_bank_definition_effects_reject_double_authority_in_cumulative_complete_bank() {
    let root = compile(Fixture::default());
    let banks = [bank(false, true)];
    let before = compile_added(&root, &banks, &[]);
    let delta = [bank_definition(&banks[0], 65531, b'D')];
    let complete = [extend_bank(&banks[0], &delta)];
    let after = compile_added(&root, &complete, &[]);
    let incoming = bank_edits(&[], &delta);
    let canonical = bank_edits(&complete, &[]);
    qualifies(&before, &after, incoming, canonical).unwrap();
    // These bodies match the final value but invent original-root ancestry for
    // an identity owned solely by the complete retained bank. Native replay
    // rejects this independently; the effects seam must retain that role split.
    let operand = [TeletextResourceObjectOperandOverlay::new(
        delta[0].identity(),
        0,
        0,
        glyph(b'X'),
        glyph(b'D'),
    )
    .unwrap()];
    let program = [TeletextResourceObjectProgramOverlay::new(
        delta[0].identity(),
        TeletextEnhancementProgram::new(vec![at(0, glyph(b'X'))]).unwrap(),
        delta[0].definition().program.clone(),
        vec![Some(0)],
        policy().validation.service,
    )
    .unwrap()];
    for conflicting in [
        TeletextEditorialEdits {
            resource_operands: &operand,
            ..canonical
        },
        TeletextEditorialEdits {
            resource_programs: &program,
            ..canonical
        },
    ] {
        assert_eq!(
            qualifies(&before, &after, incoming, conflicting)
                .unwrap_err()
                .reason,
            TeletextEditorialEffectsFailure::Association
        );
    }
}
