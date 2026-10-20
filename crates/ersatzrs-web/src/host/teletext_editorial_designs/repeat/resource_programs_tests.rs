//! Resource survivor maps remain root-relative through repeat, scalar edits and undo.
use super::*;
use ersatzrs_domain::{
    TeletextColourReference, TeletextColumnOperation, TeletextEnhancementObjectDefinition,
    TeletextGlyphCode, TeletextObjectCatalogBudget, TeletextObjectId, TeletextObjectType,
    TeletextRawDataPageNumber, TeletextResourceIdentity, TeletextResourceKind,
    TeletextResourceSubpageIndex,
};
use std::time::Duration;

#[path = "resource_programs_tests/original_positions.rs"]
mod original_positions;
fn identity(global: bool) -> TeletextObjectIdentity {
    let resource = TeletextResourceIdentity::new(
        if global {
            TeletextResourceKind::Gpop
        } else {
            TeletextResourceKind::Pop
        },
        TeletextRawDataPageNumber::new(2, 0xa0).unwrap(),
        TeletextResourceSubpageIndex::new(15).unwrap(),
    )
    .unwrap();
    if global {
        TeletextObjectIdentity::global(resource, TeletextObjectId::new(65520))
    } else {
        TeletextObjectIdentity::public(resource, TeletextObjectId::new(65520))
    }
    .unwrap()
}
fn g(code: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::G0 {
        code: TeletextGlyphCode::new(code).unwrap(),
    }
}
fn bg(colour: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::Background {
        colour: TeletextColourReference::new(colour).unwrap(),
    }
}
fn at(column: u8, operation: TeletextColumnOperation) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn { column, operation }
}
fn root() -> TeletextEnhancementProgram {
    TeletextEnhancementProgram::new(vec![at(0, g(b'A')), at(1, bg(0)), at(1, g(b'B'))]).unwrap()
}
fn catalog(global: bool) -> TeletextEnhancementObjectCatalog {
    TeletextEnhancementObjectCatalog::new(
        vec![TeletextEnhancementObjectDefinition {
            identity: identity(global),
            object_type: TeletextObjectType::Passive,
            program: root(),
        }],
        TeletextObjectCatalogBudget {
            max_objects: 1,
            max_commands: 16,
        },
    )
    .unwrap()
}
fn budget() -> TeletextServiceBudget {
    TeletextServiceBudget {
        max_records: 32,
        max_objects: 16,
        max_structure_items: 4096,
        max_base_content_bytes: 1024,
        max_commands: 128,
        max_collected_commands: 128,
        max_collected_drcs_references: 24,
    }
}
fn end() -> Instant {
    Instant::now() + Duration::from_secs(30)
}
fn growth(global: bool) -> Program {
    Program::new(
        identity(global),
        root(),
        TeletextEnhancementProgram::new(vec![
            at(0, g(b'A')),
            at(0, g(b'Q')),
            at(1, bg(0)),
            at(1, g(b'B')),
        ])
        .unwrap(),
        vec![Some(0), None, Some(1), Some(2)],
        budget(),
    )
    .unwrap()
}
#[test]
fn resource_program_repeat_translates_inserted_glyph_and_style_scalars_to_root() {
    for global in [false, true] {
        let previous = TeletextEditorialChanges {
            object_resource_additions: Vec::new(),
            object_subpage_announcements: Vec::new(),
            object_page_selections: Vec::new(),
            main_resource_invocations: Vec::new(),
            base_text_edits: Vec::new(),
            resource_programs: vec![growth(global)],
            ..TeletextEditorialChanges::default()
        };
        let incoming = TeletextEditorialChanges {
            object_resource_additions: Vec::new(),
            object_subpage_announcements: Vec::new(),
            object_page_selections: Vec::new(),
            main_resource_invocations: Vec::new(),
            base_text_edits: Vec::new(),
            resource_operands: vec![
                Operand::new(identity(global), 1, 0, g(b'Q'), g(b'Z')).unwrap(),
                Operand::new(identity(global), 2, 1, bg(0), bg(2)).unwrap(),
            ],
            ..TeletextEditorialChanges::default()
        };
        let (operands, programs) =
            merge(&catalog(global), &previous, &incoming, budget(), end()).unwrap();
        assert_eq!(
            operands,
            vec![Operand::new(identity(global), 1, 1, bg(0), bg(2)).unwrap()]
        );
        assert_eq!(programs.len(), 1);
        assert_eq!(
            programs[0].retained_commands(),
            [Some(0), None, Some(1), Some(2)]
        );
        assert_eq!(programs[0].replacement().commands()[1], at(0, g(b'Z')));
        assert_eq!(programs[0].expected().commands()[1], at(1, bg(2)));
        let undo = Program::new(
            identity(global),
            programs[0].replacement().clone(),
            programs[0].expected().clone(),
            vec![Some(0), Some(2), Some(3)],
            budget(),
        )
        .unwrap();
        let previous = TeletextEditorialChanges {
            object_resource_additions: Vec::new(),
            object_subpage_announcements: Vec::new(),
            object_page_selections: Vec::new(),
            main_resource_invocations: Vec::new(),
            base_text_edits: Vec::new(),
            resource_operands: operands,
            resource_programs: programs,
            ..TeletextEditorialChanges::default()
        };
        let incoming = TeletextEditorialChanges {
            object_resource_additions: Vec::new(),
            object_subpage_announcements: Vec::new(),
            object_page_selections: Vec::new(),
            main_resource_invocations: Vec::new(),
            base_text_edits: Vec::new(),
            resource_programs: vec![undo],
            ..TeletextEditorialChanges::default()
        };
        let (operands, programs) =
            merge(&catalog(global), &previous, &incoming, budget(), end()).unwrap();
        assert!(programs.is_empty());
        assert_eq!(
            operands,
            vec![Operand::new(identity(global), 1, 1, bg(0), bg(2)).unwrap()]
        );
        let previous = TeletextEditorialChanges {
            object_resource_additions: Vec::new(),
            object_subpage_announcements: Vec::new(),
            object_page_selections: Vec::new(),
            main_resource_invocations: Vec::new(),
            base_text_edits: Vec::new(),
            resource_operands: operands,
            ..TeletextEditorialChanges::default()
        };
        let incoming = TeletextEditorialChanges {
            object_resource_additions: Vec::new(),
            object_subpage_announcements: Vec::new(),
            object_page_selections: Vec::new(),
            main_resource_invocations: Vec::new(),
            base_text_edits: Vec::new(),
            resource_operands: vec![Operand::new(identity(global), 1, 1, bg(2), bg(0)).unwrap()],
            ..TeletextEditorialChanges::default()
        };
        let (operands, programs) =
            merge(&catalog(global), &previous, &incoming, budget(), end()).unwrap();
        assert!(operands.is_empty() && programs.is_empty());
    }
}
#[test]
fn resource_program_merge_refuses_duplicates_stale_expectations_limits_and_deadline() {
    let original = catalog(false);
    let previous = TeletextEditorialChanges {
        object_resource_additions: Vec::new(),
        object_subpage_announcements: Vec::new(),
        object_page_selections: Vec::new(),
        main_resource_invocations: Vec::new(),
        base_text_edits: Vec::new(),
        resource_programs: vec![growth(false)],
        ..TeletextEditorialChanges::default()
    };
    let stale = TeletextEditorialChanges {
        object_resource_additions: Vec::new(),
        object_subpage_announcements: Vec::new(),
        object_page_selections: Vec::new(),
        main_resource_invocations: Vec::new(),
        base_text_edits: Vec::new(),
        resource_programs: vec![growth(false)],
        ..TeletextEditorialChanges::default()
    };
    assert!(merge(&original, &previous, &stale, budget(), end()).is_err());
    let duplicate = TeletextEditorialChanges {
        object_resource_additions: Vec::new(),
        object_subpage_announcements: Vec::new(),
        object_page_selections: Vec::new(),
        main_resource_invocations: Vec::new(),
        base_text_edits: Vec::new(),
        resource_programs: vec![growth(false), growth(false)],
        ..TeletextEditorialChanges::default()
    };
    assert!(merge(
        &original,
        &TeletextEditorialChanges::default(),
        &duplicate,
        budget(),
        end()
    )
    .is_err());
    let mut tiny = budget();
    tiny.max_commands = 0;
    assert!(matches!(
        merge(
            &original,
            &TeletextEditorialChanges::default(),
            &previous,
            tiny,
            end()
        ),
        Err(NativeTeletextEditorialImportError::Budget)
    ));
    assert!(matches!(
        merge(
            &original,
            &TeletextEditorialChanges::default(),
            &previous,
            budget(),
            Instant::now()
        ),
        Err(NativeTeletextEditorialImportError::TimedOut)
    ));
}
