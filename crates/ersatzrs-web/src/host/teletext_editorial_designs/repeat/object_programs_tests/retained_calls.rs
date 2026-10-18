//! Retained Local calls compose against immutable roots without losing siblings.
use super::*;
use ersatzrs_domain::TeletextObjectOffset;

fn wrapper() -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(identity().owner().unwrap(), TeletextObjectId::new(65522))
}
fn call(column: u8) -> TeletextEnhancementProgram {
    TeletextEnhancementProgram::new(vec![TeletextEnhancementCommand::InvokeObject {
        target: identity(),
        object_type: TeletextObjectType::Passive,
        offset: Some(TeletextObjectOffset::new(0, column).unwrap()),
    }])
    .unwrap()
}
fn change(before: u8, after: u8) -> TeletextLocalObjectGlyphProgramOverlay {
    TeletextLocalObjectGlyphProgramOverlay::new_with_invocation_changes(
        wrapper(),
        call(before),
        call(after),
        vec![Some(0)],
    )
    .unwrap()
}
fn originals() -> TeletextEnhancementObjectCatalog {
    TeletextEnhancementObjectCatalog::new(
        vec![
            catalog().definitions()[0].clone(),
            TeletextEnhancementObjectDefinition {
                identity: wrapper(),
                object_type: TeletextObjectType::Adaptive,
                program: call(0),
            },
        ],
        TeletextObjectCatalogBudget {
            max_objects: 2,
            max_commands: 32,
        },
    )
    .unwrap()
}

#[test]
fn repeat_then_root_undo_keeps_other_local_scalar_and_exact_call_source() {
    let scalar = scalar(0, 0, g(b'A'), g(b'C'));
    let previous = TeletextEditorialChanges {
        object_programs: vec![change(0, 1)],
        object_operands: vec![scalar],
        ..TeletextEditorialChanges::default()
    };
    let incoming = TeletextEditorialChanges {
        object_programs: vec![change(1, 2)],
        ..TeletextEditorialChanges::default()
    };
    let (operands, programs) = merge(&originals(), &previous, &incoming, budget(), end()).unwrap();
    assert_eq!(operands, [scalar]);
    assert_eq!(programs, [change(0, 2)]);
    let previous = TeletextEditorialChanges {
        object_programs: programs,
        object_operands: operands,
        ..TeletextEditorialChanges::default()
    };
    let undo = TeletextEditorialChanges {
        object_programs: vec![change(2, 0)],
        ..TeletextEditorialChanges::default()
    };
    let (operands, programs) = merge(&originals(), &previous, &undo, budget(), end()).unwrap();
    assert_eq!(operands, [scalar]);
    assert!(programs.is_empty());
}

#[test]
fn changed_call_intent_does_not_bypass_stale_expected_or_aggregate_budget() {
    let previous = TeletextEditorialChanges {
        object_programs: vec![change(0, 1)],
        ..TeletextEditorialChanges::default()
    };
    let stale = TeletextEditorialChanges {
        object_programs: vec![change(2, 3)],
        ..TeletextEditorialChanges::default()
    };
    assert!(merge(&originals(), &previous, &stale, budget(), end()).is_err());
    let mut maximum = budget();
    maximum.max_commands = 0;
    assert!(merge(
        &originals(),
        &previous,
        &TeletextEditorialChanges::default(),
        maximum,
        end()
    )
    .is_err());
}
