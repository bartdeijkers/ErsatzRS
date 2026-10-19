use super::*;
use ersatzrs_domain::{
    TeletextColourReference, TeletextColumnOperation, TeletextEnhancementObjectDefinition,
    TeletextGlyphCode, TeletextObjectCatalogBudget, TeletextObjectId, TeletextObjectType,
    TeletextPageAddress, TeletextPageNumber, TeletextSubcode,
};
use std::time::Duration;
#[path = "object_programs_tests/retained_calls.rs"]
mod retained_calls;
fn identity() -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(
        TeletextPageAddress {
            page: TeletextPageNumber::new(124).unwrap(),
            subcode: TeletextSubcode::new(32).unwrap(),
        },
        TeletextObjectId::new(65520),
    )
}
fn at(column: u8, operation: TeletextColumnOperation) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn { column, operation }
}
fn g(code: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::G0 {
        code: TeletextGlyphCode::new(code).unwrap(),
    }
}
fn bg(code: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::Background {
        colour: TeletextColourReference::new(code).unwrap(),
    }
}
fn root() -> TeletextEnhancementProgram {
    TeletextEnhancementProgram::new(vec![at(0, g(b'A')), at(1, bg(0)), at(1, g(b'B'))]).unwrap()
}
fn catalog() -> TeletextEnhancementObjectCatalog {
    TeletextEnhancementObjectCatalog::new(
        vec![TeletextEnhancementObjectDefinition {
            identity: identity(),
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
fn growth() -> TeletextLocalObjectGlyphProgramOverlay {
    let mut commands = root().commands().to_vec();
    commands.insert(1, at(0, g(b'Q')));
    TeletextLocalObjectGlyphProgramOverlay::new(
        identity(),
        root(),
        TeletextEnhancementProgram::new(commands).unwrap(),
        vec![Some(0), None, Some(1), Some(2)],
    )
    .unwrap()
}
fn scalar(
    command: usize,
    column: u8,
    expected: TeletextColumnOperation,
    replacement: TeletextColumnOperation,
) -> TeletextLocalObjectOperandOverlay {
    TeletextLocalObjectOperandOverlay::new(identity(), command, column, expected, replacement)
        .unwrap()
}
#[test]
fn inserted_glyph_edits_stay_inside_the_root_relative_program() {
    let previous = TeletextEditorialChanges {
        base_text_edits: Vec::new(),
        resource_operands: Vec::new(),
        object_programs: vec![growth()],
        ..TeletextEditorialChanges::default()
    };
    let incoming = TeletextEditorialChanges {
        base_text_edits: Vec::new(),
        resource_operands: Vec::new(),
        object_operands: vec![scalar(1, 0, g(b'Q'), g(b'Z'))],
        ..TeletextEditorialChanges::default()
    };
    let (operands, programs) = merge(&catalog(), &previous, &incoming, budget(), end()).unwrap();
    assert!(operands.is_empty());
    assert_eq!(programs.len(), 1);
    assert_eq!(
        programs[0].retained_commands(),
        [Some(0), None, Some(1), Some(2)]
    );
    assert_eq!(programs[0].replacement().commands()[1], at(0, g(b'Z')));
}
#[test]
fn nonglyph_scalar_ordinals_translate_to_root_and_patch_both_skeletons() {
    let previous = TeletextEditorialChanges {
        base_text_edits: Vec::new(),
        resource_operands: Vec::new(),
        object_programs: vec![growth()],
        ..TeletextEditorialChanges::default()
    };
    let incoming = TeletextEditorialChanges {
        base_text_edits: Vec::new(),
        resource_operands: Vec::new(),
        object_operands: vec![scalar(2, 1, bg(0), bg(2))],
        ..TeletextEditorialChanges::default()
    };
    let (operands, programs) = merge(&catalog(), &previous, &incoming, budget(), end()).unwrap();
    assert_eq!(operands, [scalar(1, 1, bg(0), bg(2))]);
    assert_eq!(programs[0].expected().commands()[1], at(1, bg(2)));
    assert_eq!(programs[0].replacement().commands()[2], at(1, bg(2)));
}
#[test]
fn structural_undo_removes_intent_and_prior_glyph_scalars_fold_when_structure_starts() {
    let growth = growth();
    let previous = TeletextEditorialChanges {
        base_text_edits: Vec::new(),
        resource_operands: Vec::new(),
        object_programs: vec![growth.clone()],
        ..TeletextEditorialChanges::default()
    };
    let undo = TeletextLocalObjectGlyphProgramOverlay::new(
        identity(),
        growth.replacement().clone(),
        root(),
        vec![Some(0), Some(2), Some(3)],
    )
    .unwrap();
    let incoming = TeletextEditorialChanges {
        base_text_edits: Vec::new(),
        resource_operands: Vec::new(),
        object_programs: vec![undo],
        ..TeletextEditorialChanges::default()
    };
    let (operands, programs) = merge(&catalog(), &previous, &incoming, budget(), end()).unwrap();
    assert!(operands.is_empty() && programs.is_empty());
    let previous = TeletextEditorialChanges {
        base_text_edits: Vec::new(),
        resource_operands: Vec::new(),
        object_operands: vec![scalar(0, 0, g(b'A'), g(b'C'))],
        ..TeletextEditorialChanges::default()
    };
    let mut expected = root().commands().to_vec();
    expected[0] = at(0, g(b'C'));
    let mut replacement = expected.clone();
    replacement.insert(1, at(0, g(b'Q')));
    let program = TeletextLocalObjectGlyphProgramOverlay::new(
        identity(),
        TeletextEnhancementProgram::new(expected).unwrap(),
        TeletextEnhancementProgram::new(replacement).unwrap(),
        vec![Some(0), None, Some(1), Some(2)],
    )
    .unwrap();
    let incoming = TeletextEditorialChanges {
        base_text_edits: Vec::new(),
        resource_operands: Vec::new(),
        object_programs: vec![program],
        ..TeletextEditorialChanges::default()
    };
    let (operands, programs) = merge(&catalog(), &previous, &incoming, budget(), end()).unwrap();
    assert!(operands.is_empty());
    assert_eq!(programs[0].expected(), &root());
    assert_eq!(programs[0].replacement().commands()[0], at(0, g(b'C')));
}
#[test]
fn stale_duplicate_deadline_and_caller_budgets_remain_closed() {
    let previous = TeletextEditorialChanges {
        base_text_edits: Vec::new(),
        resource_operands: Vec::new(),
        object_programs: vec![growth()],
        ..TeletextEditorialChanges::default()
    };
    let stale = TeletextEditorialChanges {
        base_text_edits: Vec::new(),
        resource_operands: Vec::new(),
        object_operands: vec![scalar(1, 0, g(b'A'), g(b'Z'))],
        ..TeletextEditorialChanges::default()
    };
    assert!(merge(&catalog(), &previous, &stale, budget(), end()).is_err());
    let duplicate = TeletextEditorialChanges {
        base_text_edits: Vec::new(),
        resource_operands: Vec::new(),
        object_programs: vec![growth(), growth()],
        ..TeletextEditorialChanges::default()
    };
    assert!(merge(
        &catalog(),
        &duplicate,
        &TeletextEditorialChanges::default(),
        budget(),
        end()
    )
    .is_err());
    assert!(merge(
        &catalog(),
        &previous,
        &TeletextEditorialChanges::default(),
        budget(),
        Instant::now()
    )
    .is_err());
    let mut zero = budget();
    zero.max_commands = 0;
    assert!(merge(
        &catalog(),
        &previous,
        &TeletextEditorialChanges::default(),
        zero,
        end()
    )
    .is_err());
}

fn g2(code: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::G2 {
        code: TeletextGlyphCode::new(code).unwrap(),
    }
}
#[test]
fn wide_inserted_glyph_edits_fold_into_v12_and_undo_downgrades_to_v11() {
    let mut commands = growth().replacement().commands().to_vec();
    commands.insert(2, at(0, g2(0x23)));
    let wide = TeletextLocalObjectGlyphProgramOverlay::new_with_all_glyphs(
        identity(),
        root(),
        TeletextEnhancementProgram::new(commands).unwrap(),
        vec![Some(0), None, None, Some(1), Some(2)],
    )
    .unwrap();
    assert!(!wide.is_legacy_compatible());
    let previous = TeletextEditorialChanges {
        base_text_edits: Vec::new(),
        resource_operands: Vec::new(),
        object_programs: vec![wide.clone()],
        ..TeletextEditorialChanges::default()
    };
    let incoming = TeletextEditorialChanges {
        base_text_edits: Vec::new(),
        resource_operands: Vec::new(),
        object_operands: vec![scalar(2, 0, g2(0x23), g2(0x27))],
        ..TeletextEditorialChanges::default()
    };
    let (operands, programs) = merge(&catalog(), &previous, &incoming, budget(), end()).unwrap();
    assert!(operands.is_empty());
    assert!(!programs[0].is_legacy_compatible());
    assert_eq!(programs[0].replacement().commands()[2], at(0, g2(0x27)));
    let revert = TeletextLocalObjectGlyphProgramOverlay::new_with_all_glyphs(
        identity(),
        wide.replacement().clone(),
        growth().replacement().clone(),
        vec![Some(0), Some(1), Some(3), Some(4)],
    )
    .unwrap();
    let incoming = TeletextEditorialChanges {
        base_text_edits: Vec::new(),
        resource_operands: Vec::new(),
        object_programs: vec![revert],
        ..TeletextEditorialChanges::default()
    };
    let (operands, programs) = merge(&catalog(), &previous, &incoming, budget(), end()).unwrap();
    assert!(operands.is_empty());
    assert_eq!(programs, [growth()]);
    assert!(programs[0].is_legacy_compatible());
}
#[test]
fn wide_same_column_scalar_can_stay_a_legacy_anchor_beside_g0_structure() {
    let mut original = root().commands().to_vec();
    original[2] = at(1, g2(0x23));
    let original = TeletextEnhancementProgram::new(original).unwrap();
    let mut changed = original.commands().to_vec();
    changed.insert(1, at(0, g(b'Q')));
    changed[3] = at(1, g2(0x27));
    let replacement = TeletextEnhancementProgram::new(changed).unwrap();
    let mut operands = Vec::new();
    let mut programs = Vec::new();
    collect(
        identity(),
        &original,
        &replacement,
        &[Some(0), None, Some(1), Some(2)],
        &mut operands,
        &mut programs,
    )
    .unwrap();
    assert_eq!(operands, [scalar(2, 1, g2(0x23), g2(0x27))]);
    assert!(programs[0].is_legacy_compatible());
    assert_eq!(programs[0].expected().commands()[2], at(1, g2(0x27)));
}

#[test]
fn style_structure_composes_inserted_and_original_scalars_then_downgrades_on_deletion() {
    let foreground = |value| TeletextColumnOperation::Foreground {
        colour: TeletextColourReference::new(value).unwrap(),
    };
    let mut commands = root().commands().to_vec();
    commands.insert(1, at(0, foreground(18)));
    let program = TeletextLocalObjectGlyphProgramOverlay::new_with_style_commands(
        identity(),
        root(),
        TeletextEnhancementProgram::new(commands).unwrap(),
        vec![Some(0), None, Some(1), Some(2)],
    )
    .unwrap();
    let previous = TeletextEditorialChanges {
        base_text_edits: Vec::new(),
        resource_operands: Vec::new(),
        object_programs: vec![program],
        ..TeletextEditorialChanges::default()
    };
    let incoming = TeletextEditorialChanges {
        base_text_edits: Vec::new(),
        resource_operands: Vec::new(),
        object_operands: vec![
            scalar(1, 0, foreground(18), foreground(19)),
            scalar(2, 1, bg(0), bg(1)),
        ],
        ..TeletextEditorialChanges::default()
    };
    let (operands, programs) = merge(&catalog(), &previous, &incoming, budget(), end()).unwrap();
    assert_eq!(operands, [scalar(1, 1, bg(0), bg(1))]);
    assert_eq!(programs.len(), 1);
    assert!(!programs[0].is_all_glyphs_compatible());
    assert_eq!(
        programs[0].replacement().commands()[1],
        at(0, foreground(19))
    );
    assert_eq!(programs[0].expected().commands()[1], at(1, bg(1)));
    let mut reverted = root().commands().to_vec();
    reverted[1] = at(1, bg(1));
    let incoming = TeletextEditorialChanges {
        base_text_edits: Vec::new(),
        resource_operands: Vec::new(),
        object_programs: vec![
            TeletextLocalObjectGlyphProgramOverlay::new_with_style_commands(
                identity(),
                programs[0].replacement().clone(),
                TeletextEnhancementProgram::new(reverted).unwrap(),
                vec![Some(0), Some(2), Some(3)],
            )
            .unwrap(),
        ],
        ..TeletextEditorialChanges::default()
    };
    let previous = TeletextEditorialChanges {
        base_text_edits: Vec::new(),
        resource_operands: Vec::new(),
        object_operands: operands.clone(),
        object_programs: programs,
        ..TeletextEditorialChanges::default()
    };
    let (remaining, programs) = merge(&catalog(), &previous, &incoming, budget(), end()).unwrap();
    assert_eq!(remaining, operands);
    assert!(programs.is_empty());
}
