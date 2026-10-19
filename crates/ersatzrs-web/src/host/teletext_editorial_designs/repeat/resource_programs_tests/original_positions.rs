//! Original position authority survives root composition without borrowing Add intent.
use super::*;
use ersatzrs_domain::TeletextEnhancementPosition;

fn position(row: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(row, 0).unwrap(),
    }
}
fn before() -> TeletextEnhancementProgram {
    TeletextEnhancementProgram::new(vec![
        position(1),
        at(8, g(b'P')),
        position(3),
        at(4, g(b'Q')),
    ])
    .unwrap()
}
fn inserted() -> TeletextEnhancementProgram {
    TeletextEnhancementProgram::new(vec![
        position(1),
        at(8, g(b'P')),
        position(2),
        at(6, g(b'R')),
        position(3),
        at(4, g(b'Q')),
    ])
    .unwrap()
}
fn survivor_map() -> Vec<Option<usize>> {
    vec![Some(0), Some(1), None, None, Some(2), Some(3)]
}
fn original_catalog(global: bool) -> TeletextEnhancementObjectCatalog {
    TeletextEnhancementObjectCatalog::new(
        vec![TeletextEnhancementObjectDefinition {
            identity: identity(global),
            object_type: TeletextObjectType::Passive,
            program: before(),
        }],
        TeletextObjectCatalogBudget {
            max_objects: 1,
            max_commands: 16,
        },
    )
    .unwrap()
}

#[test]
fn original_position_merge_retains_root_anchors_and_inserted_scalar_lineage() {
    for global in [false, true] {
        let intent = Program::new_with_original_positions(
            identity(global),
            before(),
            inserted(),
            survivor_map(),
            budget(),
        )
        .unwrap();
        let incoming = TeletextEditorialChanges {
            base_text_edits: Vec::new(),
            resource_programs: vec![intent.clone()],
            ..TeletextEditorialChanges::default()
        };
        let (operands, programs) = merge(
            &original_catalog(global),
            &TeletextEditorialChanges::default(),
            &incoming,
            budget(),
            end(),
        )
        .unwrap();
        assert!(operands.is_empty());
        assert_eq!(programs, [intent]);
        let previous = TeletextEditorialChanges {
            base_text_edits: Vec::new(),
            resource_programs: programs,
            ..TeletextEditorialChanges::default()
        };
        let incoming = TeletextEditorialChanges {
            base_text_edits: Vec::new(),
            resource_operands: vec![Operand::new(identity(global), 3, 6, g(b'R'), g(b'S')).unwrap()],
            ..TeletextEditorialChanges::default()
        };
        let (operands, programs) = merge(
            &original_catalog(global),
            &previous,
            &incoming,
            budget(),
            end(),
        )
        .unwrap();
        assert!(operands.is_empty());
        assert_eq!(programs.len(), 1);
        assert!(programs[0].is_original_position_intent());
        assert_eq!(programs[0].expected(), &before());
        assert_eq!(programs[0].retained_commands(), survivor_map());
        assert_eq!(programs[0].replacement().commands()[3], at(6, g(b'S')));
        for (after, source) in programs[0]
            .replacement()
            .commands()
            .iter()
            .zip(survivor_map())
        {
            if let Some(source) = source {
                assert_eq!(*after, before().commands()[source]);
            }
        }
    }
}

#[test]
fn original_position_merge_refuses_transient_add_intent_in_either_generation() {
    for global in [false, true] {
        let transient = Program::new_with_inserted_positions(
            identity(global),
            before(),
            inserted(),
            survivor_map(),
            budget(),
        )
        .unwrap();
        let changes = TeletextEditorialChanges {
            base_text_edits: Vec::new(),
            resource_programs: vec![transient],
            ..TeletextEditorialChanges::default()
        };
        for (previous, incoming) in [
            (&changes, &TeletextEditorialChanges::default()),
            (&TeletextEditorialChanges::default(), &changes),
        ] {
            assert!(merge(
                &original_catalog(global),
                previous,
                incoming,
                budget(),
                end()
            )
            .is_err());
        }
    }
}

#[test]
fn original_position_restoration_folds_exact_history_without_saving_inverse_authority() {
    for global in [false, true] {
        let previous = Program::new_with_original_positions(
            identity(global),
            before(),
            inserted(),
            survivor_map(),
            budget(),
        )
        .unwrap();
        let inverse = Program::restore_original_positions(&previous, budget()).unwrap();
        let prior = TeletextEditorialChanges {
            base_text_edits: Vec::new(),
            resource_programs: vec![previous],
            ..TeletextEditorialChanges::default()
        };
        let incoming = TeletextEditorialChanges {
            base_text_edits: Vec::new(),
            resource_programs: vec![inverse],
            ..TeletextEditorialChanges::default()
        };
        let (operands, programs) = merge(
            &original_catalog(global),
            &prior,
            &incoming,
            budget(),
            end(),
        )
        .unwrap();
        assert!(operands.is_empty());
        assert!(
            programs.is_empty(),
            "exact root restoration stores no empty or inverse overlay"
        );
        assert!(merge(
            &original_catalog(global),
            &TeletextEditorialChanges::default(),
            &incoming,
            budget(),
            end()
        )
        .is_err());
        assert!(merge(
            &original_catalog(global),
            &incoming,
            &TeletextEditorialChanges::default(),
            budget(),
            end()
        )
        .is_err());
    }
}
