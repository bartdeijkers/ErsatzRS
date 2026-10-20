//! Retained native banks stay complete while repeat edits fold into their bodies.
use super::*;
use ersatzrs_app::teletext_design_documents::{
    TeletextSavedResourceTransmission, TeletextSavedResourceUpdate,
};
use ersatzrs_domain::{
    TeletextColourReference, TeletextColumnOperation, TeletextEnhancementCommand,
    TeletextEnhancementObjectDefinition, TeletextEnhancementProgram, TeletextGlyphCode,
    TeletextObjectCatalogBudget, TeletextObjectId, TeletextObjectIdentity, TeletextObjectType,
    TeletextRawDataPageNumber, TeletextResourceIdentity, TeletextResourceKind,
    TeletextResourceSubpageIndex, TeletextResourceUpdateCounter,
};
use std::time::Duration;

fn budget() -> TeletextServiceBudget {
    TeletextServiceBudget {
        max_records: 4,
        max_objects: 16,
        max_structure_items: 128,
        max_base_content_bytes: 1024,
        max_commands: 64,
        max_collected_commands: 64,
        max_collected_drcs_references: 16,
    }
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(30)
}
fn resource(index: u8) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        if index == 1 {
            TeletextResourceKind::Gpop
        } else {
            TeletextResourceKind::Pop
        },
        TeletextRawDataPageNumber::new(2 + index, 0xa0 + index).unwrap(),
        TeletextResourceSubpageIndex::new(15 - index).unwrap(),
    )
    .unwrap()
}
fn identity(index: u8, id: u16) -> TeletextObjectIdentity {
    if index == 1 {
        TeletextObjectIdentity::global(resource(index), TeletextObjectId::new(id))
    } else {
        TeletextObjectIdentity::public(resource(index), TeletextObjectId::new(id))
    }
    .unwrap()
}
fn glyph(code: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::G0 {
        code: TeletextGlyphCode::new(code).unwrap(),
    }
}
fn colour(value: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::Foreground {
        colour: TeletextColourReference::new(value).unwrap(),
    }
}
fn at(operation: TeletextColumnOperation) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column: 1,
        operation,
    }
}
fn program(code: u8, style: u8) -> TeletextEnhancementProgram {
    TeletextEnhancementProgram::new(vec![at(glyph(code)), at(colour(style))]).unwrap()
}
fn definition(index: u8, id: u16, code: u8) -> TeletextEnhancementObjectDefinition {
    TeletextEnhancementObjectDefinition {
        identity: identity(index, id),
        object_type: TeletextObjectType::Passive,
        program: program(code, 1),
    }
}
fn root() -> TeletextEnhancementObjectCatalog {
    TeletextEnhancementObjectCatalog::new(
        vec![definition(0, 20, b'A'), definition(0, 19, b'Z')],
        TeletextObjectCatalogBudget {
            max_objects: 16,
            max_commands: 64,
        },
    )
    .unwrap()
}
fn bank(index: u8, count: u16) -> TeletextSavedObjectResourceAddition {
    TeletextSavedObjectResourceAddition::new(
        usize::from(index) + 2,
        TeletextObjectResourceAddition::new(
            resource(index),
            (100..100 + count)
                .map(|id| definition(index, id, b'B'))
                .collect(),
            budget(),
        )
        .unwrap(),
        TeletextSavedResourceTransmission {
            resource: resource(index),
            counter: TeletextResourceUpdateCounter::new(7 - index).unwrap(),
            update: if index == 1 {
                TeletextSavedResourceUpdate::Unchanged
            } else {
                TeletextSavedResourceUpdate::Changed
            },
            last_subpage: index != 1,
        },
    )
    .unwrap()
}
fn scalar(
    index: u8,
    id: u16,
    ordinal: usize,
    before: TeletextColumnOperation,
    after: TeletextColumnOperation,
) -> Operand {
    Operand::new(identity(index, id), ordinal, 1, before, after).unwrap()
}
fn previous() -> TeletextEditorialChanges {
    TeletextEditorialChanges {
        object_resource_additions: vec![bank(1, 8), bank(2, 1)],
        object_subpage_announcements: Vec::new(),
        resource_additions: vec![Addition::new(definition(0, 21, b'C'), budget()).unwrap()],
        resource_operands: vec![scalar(0, 20, 0, glyph(b'A'), glyph(b'Q'))],
        ..Default::default()
    }
}
fn apply(
    previous: &TeletextEditorialChanges,
    incoming: &TeletextEditorialChanges,
) -> TeletextEditorialChanges {
    let (resource_operands, resource_programs, resource_additions, object_resource_additions) =
        merge(&root(), previous, incoming, budget(), deadline()).unwrap();
    TeletextEditorialChanges {
        object_resource_additions,
        resource_operands,
        resource_programs,
        resource_additions,
        ..Default::default()
    }
}
fn assert_owner_and_siblings(before: &TeletextEditorialChanges, after: &TeletextEditorialChanges) {
    assert_eq!(after.object_resource_additions.len(), 2);
    let (old, new) = (
        &before.object_resource_additions[0],
        &after.object_resource_additions[0],
    );
    assert_eq!(new.document_index(), old.document_index());
    assert_eq!(new.transmission(), old.transmission());
    assert_eq!(new.addition().resource(), old.addition().resource());
    assert_eq!(new.addition().definitions().len(), 8);
    assert_eq!(
        &new.addition().definitions()[1..],
        &old.addition().definitions()[1..]
    );
    assert_eq!(
        new.addition().definitions()[0].identity,
        old.addition().definitions()[0].identity
    );
    assert_eq!(
        new.addition().definitions()[0].object_type,
        old.addition().definitions()[0].object_type
    );
    assert_eq!(
        after.object_resource_additions[1],
        before.object_resource_additions[1]
    );
    assert!(
        after.resource_programs.is_empty(),
        "bank bodies never become root-relative programs"
    );
}

#[test]
fn retained_bank_repeat_preserves_eight_definitions_under_four_record_limit_and_unrelated_edits() {
    let before = previous();
    assert!(
        before.object_resource_additions[0]
            .addition()
            .definitions()
            .len()
            > budget().max_records
    );
    let unchanged = apply(&before, &TeletextEditorialChanges::default());
    assert_eq!(
        unchanged.object_resource_additions,
        before.object_resource_additions
    );
    assert_eq!(unchanged.resource_additions, before.resource_additions);
    assert_eq!(unchanged.resource_operands, before.resource_operands);
    let incoming = TeletextEditorialChanges {
        resource_operands: vec![
            scalar(0, 20, 0, glyph(b'Q'), glyph(b'R')),
            scalar(0, 21, 0, glyph(b'C'), glyph(b'E')),
        ],
        ..Default::default()
    };
    let after = apply(&before, &incoming);
    assert_eq!(
        after.object_resource_additions,
        before.object_resource_additions
    );
    assert_eq!(
        after.resource_operands,
        [scalar(0, 20, 0, glyph(b'A'), glyph(b'R'))]
    );
    assert_eq!(
        after.resource_additions[0].definition(),
        &definition(0, 21, b'E')
    );
    assert_eq!(
        root().definition(identity(0, 20)),
        Some(&definition(0, 20, b'A'))
    );
    assert_eq!(
        root().definition(identity(0, 19)),
        Some(&definition(0, 19, b'Z'))
    );
}

#[test]
fn retained_bank_repeat_folds_scalar_style_structure_and_exact_undo_without_losing_ownership() {
    let before = previous();
    let incoming = TeletextEditorialChanges {
        resource_operands: vec![
            scalar(1, 100, 0, glyph(b'B'), glyph(b'D')),
            scalar(1, 100, 1, colour(1), colour(4)),
        ],
        ..Default::default()
    };
    let styled = apply(&before, &incoming);
    assert_owner_and_siblings(&before, &styled);
    assert_eq!(styled.resource_additions, before.resource_additions);
    assert_eq!(styled.resource_operands, before.resource_operands);
    assert_eq!(
        styled.object_resource_additions[0].addition().definitions()[0].program,
        program(b'D', 4)
    );
    let mut commands = program(b'D', 4).commands().to_vec();
    commands.push(at(glyph(b'E')));
    let grown = TeletextEnhancementProgram::new(commands).unwrap();
    let incoming = TeletextEditorialChanges {
        resource_programs: vec![Program::new(
            identity(1, 100),
            program(b'D', 4),
            grown.clone(),
            vec![Some(0), Some(1), None],
            budget(),
        )
        .unwrap()],
        ..Default::default()
    };
    let expanded = apply(&styled, &incoming);
    assert_owner_and_siblings(&before, &expanded);
    assert_eq!(
        expanded.object_resource_additions[0]
            .addition()
            .definitions()[0]
            .program,
        grown
    );
    let incoming = TeletextEditorialChanges {
        resource_programs: vec![Program::new(
            identity(1, 100),
            grown,
            program(b'D', 4),
            vec![Some(0), Some(1)],
            budget(),
        )
        .unwrap()],
        ..Default::default()
    };
    let shrunk = apply(&expanded, &incoming);
    let incoming = TeletextEditorialChanges {
        resource_operands: vec![
            scalar(1, 100, 0, glyph(b'D'), glyph(b'B')),
            scalar(1, 100, 1, colour(4), colour(1)),
        ],
        ..Default::default()
    };
    let undone = apply(&shrunk, &incoming);
    assert_eq!(
        undone.object_resource_additions,
        before.object_resource_additions
    );
    assert_eq!(undone.resource_additions, before.resource_additions);
    assert_eq!(undone.resource_operands, before.resource_operands);
    assert!(undone.resource_programs.is_empty());
}

#[test]
fn retained_bank_repeat_rejects_creation_stale_missing_targets_duplicates_limits_and_expired_work()
{
    let before = previous();
    let cases = [
        TeletextEditorialChanges {
            object_resource_additions: vec![bank(2, 1)],
            object_subpage_announcements: Vec::new(),
            ..Default::default()
        },
        TeletextEditorialChanges {
            resource_additions: vec![Addition::new(definition(0, 22, b'F'), budget()).unwrap()],
            ..Default::default()
        },
        TeletextEditorialChanges {
            resource_operands: vec![scalar(1, 100, 0, glyph(b'X'), glyph(b'Y'))],
            ..Default::default()
        },
        TeletextEditorialChanges {
            resource_operands: vec![scalar(1, 108, 0, glyph(b'B'), glyph(b'C'))],
            ..Default::default()
        },
        TeletextEditorialChanges {
            resource_operands: vec![scalar(1, 100, 0, glyph(b'B'), glyph(b'C')); 2],
            ..Default::default()
        },
        TeletextEditorialChanges {
            resource_programs: vec![Program::new(
                identity(1, 100),
                program(b'X', 1),
                program(b'Y', 1),
                vec![Some(0), Some(1)],
                budget(),
            )
            .unwrap()],
            ..Default::default()
        },
    ];
    for incoming in cases {
        assert!(merge(&root(), &before, &incoming, budget(), deadline()).is_err());
    }
    for reduced in [
        TeletextServiceBudget {
            max_objects: 9,
            ..budget()
        },
        TeletextServiceBudget {
            max_records: 1,
            ..budget()
        },
        TeletextServiceBudget {
            max_commands: 19,
            ..budget()
        },
    ] {
        assert!(merge(
            &root(),
            &before,
            &TeletextEditorialChanges::default(),
            reduced,
            deadline()
        )
        .is_err());
    }
    assert!(merge(
        &root(),
        &before,
        &TeletextEditorialChanges::default(),
        budget(),
        Instant::now()
    )
    .is_err());
    assert_eq!(
        before.object_resource_additions,
        previous().object_resource_additions,
        "failed folds leave retained complete banks intact"
    );
}
