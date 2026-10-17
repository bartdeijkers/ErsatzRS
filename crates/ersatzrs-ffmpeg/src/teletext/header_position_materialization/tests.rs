//! Real typed setter effects, independent of the materializer's colour choice.

use super::*;
use crate::teletext::{
    allocate_teletext_resource_objects,
    authored_service_tests::{input, limits, semantic_limits},
    compare_teletext_physical_display, compile_teletext_physical_display_program,
    compose_teletext_main_program, TeletextPhysicalComparisonField,
    TeletextPhysicalComparisonFinding,
};
use ersatzrs_domain::{
    TeletextEnhancementObjectDefinition, TeletextEnhancementPosition, TeletextGlyphCode,
    TeletextObjectId, TeletextObjectIdentity, TeletextObjectType, TeletextRow, TeletextRun,
    TeletextRunContent, TeletextStyle,
};

fn colour(value: u8) -> TeletextColourReference {
    TeletextColourReference::new(value).unwrap()
}
fn fg(column: u8, value: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column,
        operation: TeletextColumnOperation::Foreground {
            colour: colour(value),
        },
    }
}
fn fixture(mut prefix: Vec<TeletextEnhancementCommand>, column: u8) -> TeletextSemanticService {
    let mut draft = input();
    let owner = draft.base.pages[0].address;
    let target = TeletextObjectIdentity::local(owner, TeletextObjectId::new(65535));
    draft.base.pages[0].rows.push(TeletextRow {
        row: 0,
        runs: vec![TeletextRun {
            column: 8,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Text { text: "ZZZ".into() },
        }],
    });
    prefix.extend([
        TeletextEnhancementCommand::SetPosition {
            position: TeletextEnhancementPosition::new(0, column).unwrap(),
        },
        TeletextEnhancementCommand::InvokeObject {
            target,
            object_type: TeletextObjectType::Passive,
            offset: None,
        },
    ]);
    draft.pages[0].local_program = Some(TeletextEnhancementProgram::new(prefix).unwrap());
    draft.objects.push(TeletextEnhancementObjectDefinition {
        identity: target,
        object_type: TeletextObjectType::Passive,
        program: TeletextEnhancementProgram::new(vec![
            fg(0, 2),
            TeletextEnhancementCommand::AtColumn {
                column: 0,
                operation: TeletextColumnOperation::G0 {
                    code: TeletextGlyphCode::new(b'A').unwrap(),
                },
            },
        ])
        .unwrap(),
    });
    TeletextSemanticService::new(draft, semantic_limits()).unwrap()
}

#[test]
fn header_materialization_preserves_original_target_and_uses_main_not_passive_colour() {
    let service = fixture(vec![], 8);
    let budget = limits().normalization.composition;
    let original =
        compose_teletext_main_program(&service, service.base().pages[0].address, budget).unwrap();
    let trace = original.spatial_trace().clone();
    let attributes = original.attribute_execution().clone();
    let original_commands = original.program().commands().to_vec();
    let materialized = materialize(
        original,
        &service,
        budget,
        &mut TeletextCompileContext::new(None, None, None),
    )
    .unwrap();
    assert_eq!(materialized.spatial_trace(), &trace);
    assert_eq!(materialized.attribute_execution(), &attributes);
    let generated = materialized
        .provenance()
        .iter()
        .position(|source| *source == TeletextMainCompositionSource::HeaderPosition { command: 0 })
        .unwrap();
    assert_eq!(materialized.program().commands()[generated], fg(8, 7));
    let retained: Vec<_> = materialized
        .program()
        .commands()
        .iter()
        .zip(materialized.provenance())
        .filter_map(|(command, source)| {
            (!matches!(source, TeletextMainCompositionSource::HeaderPosition { .. }))
                .then_some(*command)
        })
        .collect();
    assert_eq!(retained, original_commands);
}

#[test]
fn header_materialization_counts_generated_output_before_building_a_candidate() {
    let service = fixture(vec![], 8);
    let mut budget = limits().normalization.composition;
    let original =
        compose_teletext_main_program(&service, service.base().pages[0].address, budget).unwrap();
    budget.max_output_commands = original.program().commands().len();
    let error = materialize(
        original,
        &service,
        budget,
        &mut TeletextCompileContext::new(None, None, None),
    )
    .unwrap_err();
    assert_eq!(
        error.location,
        Some(TeletextMainCompositionSource::HeaderPosition { command: 0 })
    );
    assert_eq!(
        error.reason,
        TeletextMainCompositionFailure::OutputBudget {
            maximum: budget.max_output_commands
        }
    );
}

#[test]
fn header_materialization_keeps_real_triplets_within_208_word_capacity() {
    let service = |count| {
        fixture(
            vec![TeletextEnhancementCommand::SetScreenColour { colour: colour(0) }; count],
            8,
        )
    };
    let exact = service(200);
    let normalized = crate::teletext::compile_teletext_normalized_main_program(
        &exact,
        exact.base().pages[0].address,
        limits().normalization,
    )
    .unwrap();
    assert_eq!(normalized.encoded().packet_bodies().len(), 16);
    assert_eq!(normalized.encoded().main_triplet_count(), 203);
    assert_eq!(
        normalized.encoded().definition_positions()[0].absolute_index(),
        204
    );
    let overflow = service(201);
    let saved = overflow.clone();
    let error = crate::teletext::compile_teletext_normalized_main_program(
        &overflow,
        overflow.base().pages[0].address,
        limits().normalization,
    )
    .unwrap_err();
    let crate::teletext::TeletextNormalizedMainProgramFailure::Original(error) = error.reason
    else {
        panic!("overflow must reject during original physical packing")
    };
    let crate::teletext::TeletextComposedMainEncodingError::Packing(error) = *error else {
        panic!("overflow must retain the exact packing failure")
    };
    assert_eq!(
        error.reason,
        crate::teletext::TeletextLocalObjectProgramEncodingFailure::Capacity {
            count: 209,
            maximum: 208
        }
    );
    assert_eq!(overflow, saved);
}

#[test]
fn header_materialization_does_not_guess_unsettled_foreground() {
    let service = fixture(vec![fg(8, 1), fg(8, 2)], 9);
    let budget = limits().normalization.composition;
    let original =
        compose_teletext_main_program(&service, service.base().pages[0].address, budget).unwrap();
    let foreground = original
        .attribute_execution()
        .before_adaptive_cell(0, 9)
        .unwrap()
        .foreground();
    assert_eq!(foreground.value(), None);
    assert!(foreground.pending().is_some());
    let saved = original.clone();
    let actual = materialize(
        original,
        &service,
        budget,
        &mut TeletextCompileContext::new(None, None, None),
    )
    .unwrap();
    assert_eq!(actual, saved);
}

#[test]
fn header_materialization_full_replay_detects_copied_passive_colour_on_base_tail() {
    let service = fixture(vec![], 8);
    let budget = limits().normalization;
    let original = compose_teletext_main_program(
        &service,
        service.base().pages[0].address,
        budget.composition,
    )
    .unwrap();
    let actual = materialize(
        original,
        &service,
        budget.composition,
        &mut TeletextCompileContext::new(None, None, None),
    )
    .unwrap();
    let generated = actual
        .provenance()
        .iter()
        .position(|source| matches!(source, TeletextMainCompositionSource::HeaderPosition { .. }))
        .unwrap();
    let mut wrong = actual.program().commands().to_vec();
    wrong[generated] = fg(8, 2); // Called Passive colour is not neutral Main colour.
    let wrong = TeletextEnhancementProgram::new(wrong).unwrap();
    let allocations =
        allocate_teletext_resource_objects(service.objects(), &[], budget.max_resource_objects)
            .unwrap();
    let physical = compile_teletext_physical_display_program(
        &service,
        &actual,
        &wrong,
        service.objects(),
        &allocations,
        budget.replay,
        budget.packing,
    )
    .unwrap();
    let comparison =
        compare_teletext_physical_display(&actual, physical.replay(), budget.display_comparison)
            .unwrap();
    assert!(comparison
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.position == Some((0, 9))
            && diagnostic.finding
                == TeletextPhysicalComparisonFinding::Difference(
                    TeletextPhysicalComparisonField::ForegroundReference
                )));
}
