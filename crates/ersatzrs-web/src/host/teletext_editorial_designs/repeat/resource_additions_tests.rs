//! Added definitions never acquire original command ancestry during editor repeats.
use super::*;
use ersatzrs_domain::{
    TeletextColumnOperation, TeletextEnhancementCommand, TeletextEnhancementObjectDefinition,
    TeletextEnhancementProgram, TeletextGlyphCode, TeletextObjectCatalogBudget, TeletextObjectId,
    TeletextObjectIdentity, TeletextObjectType, TeletextRawDataPageNumber,
    TeletextResourceIdentity, TeletextResourceKind, TeletextResourceSubpageIndex,
};
use std::time::Duration;
fn budget() -> TeletextServiceBudget {
    TeletextServiceBudget {
        max_records: 16,
        max_objects: 16,
        max_structure_items: 128,
        max_base_content_bytes: 1024,
        max_commands: 32,
        max_collected_commands: 32,
        max_collected_drcs_references: 16,
    }
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(30)
}
fn identity(id: u16) -> TeletextObjectIdentity {
    TeletextObjectIdentity::public(
        TeletextResourceIdentity::new(
            TeletextResourceKind::Pop,
            TeletextRawDataPageNumber::new(2, 0xa0).unwrap(),
            TeletextResourceSubpageIndex::new(15).unwrap(),
        )
        .unwrap(),
        TeletextObjectId::new(id),
    )
    .unwrap()
}
fn glyph(code: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column: 1,
        operation: TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(code).unwrap(),
        },
    }
}
fn program(codes: &[u8]) -> TeletextEnhancementProgram {
    TeletextEnhancementProgram::new(codes.iter().copied().map(glyph).collect()).unwrap()
}
fn definition(id: u16, codes: &[u8]) -> TeletextEnhancementObjectDefinition {
    TeletextEnhancementObjectDefinition {
        identity: identity(id),
        object_type: TeletextObjectType::Passive,
        program: program(codes),
    }
}
fn root() -> TeletextEnhancementObjectCatalog {
    TeletextEnhancementObjectCatalog::new(
        vec![definition(20, b"A")],
        TeletextObjectCatalogBudget {
            max_objects: 16,
            max_commands: 32,
        },
    )
    .unwrap()
}
fn addition(id: u16, codes: &[u8]) -> Addition {
    Addition::new(definition(id, codes), budget()).unwrap()
}
fn changes(additions: Vec<Addition>) -> TeletextEditorialChanges {
    TeletextEditorialChanges {
        base_text_edits: Vec::new(),
        resource_additions: additions,
        ..TeletextEditorialChanges::default()
    }
}
fn scalar(id: u16, ordinal: usize, before: u8, after: u8) -> Operand {
    let TeletextEnhancementCommand::AtColumn {
        operation: before, ..
    } = glyph(before)
    else {
        unreachable!()
    };
    let TeletextEnhancementCommand::AtColumn {
        operation: after, ..
    } = glyph(after)
    else {
        unreachable!()
    };
    Operand::new(identity(id), ordinal, 1, before, after).unwrap()
}
#[test]
fn resource_addition_repeat_folds_scalar_structure_undo_and_repeat_without_root_program_overlay() {
    let previous = changes(vec![addition(21, b"B")]);
    let incoming = TeletextEditorialChanges {
        base_text_edits: Vec::new(),
        resource_operands: vec![scalar(21, 0, b'B', b'C'), scalar(20, 0, b'A', b'Q')],
        ..TeletextEditorialChanges::default()
    };
    let (operands, programs, additions) =
        merge(&root(), &previous, &incoming, budget(), deadline()).unwrap();
    assert_eq!(operands, [scalar(20, 0, b'A', b'Q')]);
    assert!(programs.is_empty());
    assert_eq!(additions, [addition(21, b"C")]);
    let previous = TeletextEditorialChanges {
        base_text_edits: Vec::new(),
        resource_operands: operands,
        resource_programs: programs,
        resource_additions: additions,
        ..TeletextEditorialChanges::default()
    };
    let grown = Program::new(
        identity(21),
        program(b"C"),
        program(b"CD"),
        vec![Some(0), None],
        budget(),
    )
    .unwrap();
    let incoming = TeletextEditorialChanges {
        base_text_edits: Vec::new(),
        resource_programs: vec![grown],
        ..TeletextEditorialChanges::default()
    };
    let (operands, programs, additions) =
        merge(&root(), &previous, &incoming, budget(), deadline()).unwrap();
    assert_eq!(operands, previous.resource_operands);
    assert!(programs.is_empty());
    assert_eq!(additions, [addition(21, b"CD")]);
    let previous = TeletextEditorialChanges {
        base_text_edits: Vec::new(),
        resource_operands: operands,
        resource_additions: additions,
        ..TeletextEditorialChanges::default()
    };
    let incoming = TeletextEditorialChanges {
        base_text_edits: Vec::new(),
        resource_programs: vec![Program::new(
            identity(21),
            program(b"CD"),
            program(b"C"),
            vec![Some(0)],
            budget(),
        )
        .unwrap()],
        ..TeletextEditorialChanges::default()
    };
    let (_, programs, additions) =
        merge(&root(), &previous, &incoming, budget(), deadline()).unwrap();
    assert!(programs.is_empty());
    assert_eq!(additions, [addition(21, b"C")]);
    let previous = changes(additions);
    let incoming = TeletextEditorialChanges {
        base_text_edits: Vec::new(),
        resource_operands: vec![scalar(21, 0, b'C', b'B')],
        ..TeletextEditorialChanges::default()
    };
    let (_, programs, additions) =
        merge(&root(), &previous, &incoming, budget(), deadline()).unwrap();
    assert!(programs.is_empty());
    assert_eq!(
        additions,
        [addition(21, b"B")],
        "body undo preserves the explicit addition, never invents an immutable-root definition"
    );
    let incoming = TeletextEditorialChanges {
        base_text_edits: Vec::new(),
        resource_operands: vec![scalar(21, 0, b'B', b'C')],
        ..TeletextEditorialChanges::default()
    };
    let (_, programs, additions) = merge(
        &root(),
        &changes(additions),
        &incoming,
        budget(),
        deadline(),
    )
    .unwrap();
    assert!(programs.is_empty());
    assert_eq!(additions, [addition(21, b"C")]);
}
#[test]
fn resource_addition_repeat_refuses_untrusted_adds_duplicate_stale_missing_and_shadowed_targets() {
    let previous = changes(vec![addition(21, b"B")]);
    for incoming in [
        changes(vec![addition(22, b"C")]),
        TeletextEditorialChanges {
            base_text_edits: Vec::new(),
            resource_operands: vec![scalar(21, 0, b'Z', b'C')],
            ..TeletextEditorialChanges::default()
        },
        TeletextEditorialChanges {
            base_text_edits: Vec::new(),
            resource_operands: vec![scalar(21, 0, b'B', b'C'), scalar(21, 0, b'B', b'D')],
            ..TeletextEditorialChanges::default()
        },
        TeletextEditorialChanges {
            base_text_edits: Vec::new(),
            resource_operands: vec![scalar(22, 0, b'B', b'C')],
            ..TeletextEditorialChanges::default()
        },
        TeletextEditorialChanges {
            base_text_edits: Vec::new(),
            resource_programs: vec![Program::new(
                identity(21),
                program(b"Z"),
                program(b"ZQ"),
                vec![Some(0), None],
                budget(),
            )
            .unwrap()],
            ..TeletextEditorialChanges::default()
        },
    ] {
        assert!(matches!(
            merge(&root(), &previous, &incoming, budget(), deadline()),
            Err(NativeTeletextEditorialImportError::Association)
        ));
    }
    for previous in [
        changes(vec![addition(21, b"B"), addition(21, b"B")]),
        changes(vec![addition(20, b"B")]),
        TeletextEditorialChanges {
            base_text_edits: Vec::new(),
            resource_additions: vec![addition(21, b"B")],
            resource_operands: vec![scalar(21, 0, b'B', b'C')],
            ..TeletextEditorialChanges::default()
        },
    ] {
        assert!(merge(
            &root(),
            &previous,
            &TeletextEditorialChanges::default(),
            budget(),
            deadline()
        )
        .is_err());
    }
}
#[test]
fn resource_addition_repeat_bounds_aggregate_result_and_keeps_unselected_definitions() {
    let previous = changes(vec![addition(21, b"B"), addition(22, b"C")]);
    let incoming = TeletextEditorialChanges {
        base_text_edits: Vec::new(),
        resource_operands: vec![scalar(21, 0, b'B', b'D')],
        ..TeletextEditorialChanges::default()
    };
    let (_, _, additions) = merge(&root(), &previous, &incoming, budget(), deadline()).unwrap();
    assert_eq!(additions, [addition(21, b"D"), addition(22, b"C")]);
    let grow = |id, before: u8| {
        Program::new(
            identity(id),
            program(&[before]),
            program(&[before, b'Q']),
            vec![Some(0), None],
            budget(),
        )
        .unwrap()
    };
    let incoming = TeletextEditorialChanges {
        base_text_edits: Vec::new(),
        resource_programs: vec![grow(21, b'B'), grow(22, b'C')],
        ..TeletextEditorialChanges::default()
    };
    let tight = TeletextServiceBudget {
        max_commands: 3,
        ..budget()
    };
    assert!(
        matches!(
            merge(&root(), &previous, &incoming, tight, deadline()),
            Err(NativeTeletextEditorialImportError::Budget)
        ),
        "each new definition fits individually, their combined result does not"
    );
    assert!(matches!(
        merge(
            &root(),
            &previous,
            &TeletextEditorialChanges::default(),
            budget(),
            Instant::now()
        ),
        Err(NativeTeletextEditorialImportError::TimedOut)
    ));
}

#[test]
fn resource_addition_repeat_shared_overlay_dispatch_preserves_addition_only_and_existing_intents() {
    let provenance = super::super::TeletextSavedEditorialProvenance {
        export_receipt_id: 1,
        export_receipt_digest: "a".repeat(64),
        original_design_id: 2,
        original_revision: 1,
        original_digest: "b".repeat(64),
    };
    let only = changes(vec![addition(21, b"B")]);
    let overlay = super::super::cumulative_overlay(&provenance, &only).unwrap();
    assert_eq!(overlay.schema_version(), 18);
    assert!(overlay.pages().is_empty());
    let restored = super::super::from_overlay(&overlay);
    assert_eq!(restored.resource_additions, only.resource_additions);
    assert!(restored.resource_operands.is_empty() && restored.resource_programs.is_empty());
    let body = TeletextEditorialChanges {
        base_text_edits: Vec::new(),
        resource_operands: vec![scalar(20, 0, b'A', b'Q')],
        ..TeletextEditorialChanges::default()
    };
    let legacy = super::super::cumulative_overlay(&provenance, &body).unwrap();
    assert_eq!(
        legacy.schema_version(),
        14,
        "addition-free private role stays unchanged"
    );
    let mixed = TeletextEditorialChanges {
        base_text_edits: Vec::new(),
        resource_additions: only.resource_additions,
        resource_operands: body.resource_operands,
        ..TeletextEditorialChanges::default()
    };
    let overlay = super::super::cumulative_overlay(&provenance, &mixed).unwrap();
    assert_eq!(overlay.schema_version(), 18);
    let restored = super::super::from_overlay(&overlay);
    assert_eq!(restored.resource_additions, mixed.resource_additions);
    assert_eq!(restored.resource_operands, mixed.resource_operands);
    assert_eq!(overlay.provenance(), &provenance);
}
