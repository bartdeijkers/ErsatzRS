//! Shared actual attribute-cell updates include original/physical settlement.

use super::planning_quota_tests::calling_displays;
use super::update_quota_tests::{assert_same, single};
use super::*;

fn mixed_displays() -> TeletextDeclaredService {
    let mut draft = input();
    draft.pages[0].local_program = Some(
        TeletextEnhancementProgram::new(vec![TeletextEnhancementCommand::InvokeObject {
            target: target(20),
            object_type: TeletextObjectType::Passive,
            offset: None,
        }])
        .unwrap(),
    );
    declared(draft)
}

fn metered_limits(maximum: usize) -> TeletextAuthoredServiceEncodingBudget {
    let mut budget = limits();
    budget.max_attribute_cell_updates = Some(maximum);
    budget
}

fn encode(
    declared: TeletextDeclaredService,
    budget: &TeletextAuthoredServiceEncodingBudget,
) -> (
    Result<EncodedTeletextAuthoredService, TeletextAuthoredServiceEncodingError>,
    TeletextAuthoredServiceEncodingUsage,
) {
    let tx = transmissions(&declared);
    encode_teletext_authored_service_with_work_usage(declared, &tx, budget, None)
}

fn passes(
    base: &TeletextBaseDisplay,
    trace: &TeletextSpatialTrace,
    layers: &TeletextGlyphLayers,
    colours: &[TeletextColourReference; 25],
    settled: &TeletextAttributeExecution,
    maximum: usize,
) -> [usize; 2] {
    let (unbound, unbound_used) = resolve_teletext_attribute_execution_with_usage(
        base, trace, layers, colours, maximum, None,
    );
    assert!(unbound.unwrap().transmission_evidence().is_none());
    let (result, settled_used) = resolve_teletext_attribute_execution_with_transmission_and_usage(
        base,
        trace,
        layers,
        colours,
        maximum,
        settled
            .transmission_evidence()
            .expect("complete service owns final-emitter evidence"),
        None,
    );
    assert_eq!(&result.unwrap(), settled);
    [unbound_used, settled_used]
}

fn independent_service_updates(encoded: &EncodedTeletextAuthoredService) -> usize {
    let budget = limits().normalization;
    encoded
        .pages()
        .iter()
        .map(|page| match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page) => {
                let original = page.normalized().original();
                let physical = page.normalized().physical().replay();
                let original_passes = passes(
                    original.base_display(),
                    original.spatial_trace(),
                    original.glyph_layers(),
                    original.colour_context().row_colours(),
                    original.attribute_execution(),
                    budget.composition.collection.max_commands,
                );
                let physical_passes = passes(
                    original.base_display(),
                    physical.spatial_trace(),
                    physical.glyph_layers(),
                    original.colour_context().row_colours(),
                    physical.attribute_execution(),
                    budget.replay.max_commands,
                );
                assert!(original_passes.into_iter().all(|used| used > 0));
                assert!(physical_passes.into_iter().all(|used| used > 0));
                original_passes
                    .into_iter()
                    .chain(physical_passes)
                    .sum::<usize>()
            }
            EncodedTeletextAuthoredPage::NativeDisplay(page) => {
                let original = page.composition();
                let usage = passes(
                    original.base_display(),
                    original.spatial_trace(),
                    original.glyph_layers(),
                    original.colour_context().row_colours(),
                    original.attribute_execution(),
                    budget.composition.collection.max_commands,
                );
                assert!(usage.into_iter().all(|used| used > 0));
                usage.into_iter().sum()
            }
            _ => 0,
        })
        .sum()
}

fn compiled_attribute_failure(
    error: &TeletextAuthoredServiceEncodingError,
) -> &TeletextAttributeExecutionError {
    let Failure::Compiled(error) = &error.reason else {
        panic!("expected compiled-page failure: {error:?}");
    };
    let TeletextCompiledDisplayPageEncodingFailure::Normalization(error) = &error.reason else {
        panic!("expected normalization failure: {error:?}");
    };
    let composition = match &error.reason {
        TeletextNormalizedMainProgramFailure::Original(error) => {
            let TeletextComposedMainEncodingError::Composition(error) = error.as_ref() else {
                panic!("expected original composition failure: {error:?}");
            };
            &error.reason
        }
        TeletextNormalizedMainProgramFailure::Compilation(error) => {
            let TeletextAugmentedOriginCompilationFailure::Physical(error) = &error.reason else {
                panic!("expected physical compilation failure: {error:?}");
            };
            let TeletextPhysicalDisplayEncodingError::PhysicalReplay(error) = error.as_ref() else {
                panic!("expected physical replay failure: {error:?}");
            };
            &error.reason
        }
        _ => panic!("expected attribute execution failure: {error:?}"),
    };
    let TeletextMainCompositionFailure::AttributeExecution(error) = composition else {
        panic!("expected attribute execution failure: {composition:?}");
    };
    error
}

#[test]
fn exact_attribute_credit_preserves_models_units_and_all_four_compiled_passes() {
    let declared = calling_displays(2);
    let tx = transmissions(&declared);
    let expected = encode_teletext_authored_service(declared.clone(), &tx, &limits()).unwrap();
    let (full, usage) = encode(declared.clone(), &metered_limits(usize::MAX));
    let full = full.unwrap();
    assert_same(&expected, &full);
    let admitted = usage.attribute_cell_updates.unwrap();
    assert!(admitted > 0);
    assert_eq!(admitted, independent_service_updates(&full));
    let (result, exact_usage) = encode(declared.clone(), &metered_limits(admitted));
    assert_eq!(exact_usage, usage);
    assert_same(&expected, &result.unwrap());
    let (result, short_usage) = encode(declared, &metered_limits(admitted - 1));
    let error = result.unwrap_err();
    assert_eq!(error.display, Some(owner(1)));
    assert_eq!(
        compiled_attribute_failure(&error).reason,
        TeletextAttributeExecutionFailure::WorkBudget
    );
    assert!(short_usage.attribute_cell_updates.unwrap() < admitted);
}

#[test]
fn compiled_and_native_attribute_credit_is_shared_between_pages_and_native_settlement() {
    let (compiled, compiled_usage) = encode(calling_displays(1), &metered_limits(usize::MAX));
    let compiled = compiled.unwrap();
    let compiled_used = compiled_usage.attribute_cell_updates.unwrap();
    assert_eq!(compiled_used, independent_service_updates(&compiled));
    let (native, native_usage) = encode(single(1), &metered_limits(usize::MAX));
    let native = native.unwrap();
    let native_used = native_usage.attribute_cell_updates.unwrap();
    assert_eq!(native_used, independent_service_updates(&native));
    let declared = mixed_displays();
    let (full, usage) = encode(declared.clone(), &metered_limits(usize::MAX));
    assert_eq!(
        usage.attribute_cell_updates,
        Some(compiled_used + native_used)
    );
    assert_eq!(
        usage.attribute_cell_updates.unwrap(),
        independent_service_updates(&full.unwrap())
    );
    let maximum = compiled_used.max(native_used);
    let (result, admitted) = encode(declared, &metered_limits(maximum));
    assert_eq!(result.unwrap_err().display, Some(owner(1)));
    assert!(admitted.attribute_cell_updates.unwrap() >= compiled_used);
    assert!(admitted.attribute_cell_updates.unwrap() <= maximum);
}

#[test]
fn attribute_credit_never_resets_at_original_or_physical_settlement() {
    let declared = calling_displays(1);
    let (full, _) = encode(declared.clone(), &metered_limits(usize::MAX));
    let full = full.unwrap();
    let page = full
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page) => Some(page),
            _ => None,
        })
        .unwrap();
    let original = page.normalized().original();
    let physical = page.normalized().physical().replay();
    let budget = limits().normalization;
    let original_passes = passes(
        original.base_display(),
        original.spatial_trace(),
        original.glyph_layers(),
        original.colour_context().row_colours(),
        original.attribute_execution(),
        budget.composition.collection.max_commands,
    );
    let physical_passes = passes(
        original.base_display(),
        physical.spatial_trace(),
        physical.glyph_layers(),
        original.colour_context().row_colours(),
        physical.attribute_execution(),
        budget.replay.max_commands,
    );
    let mut maximum = 0;
    for (stage, next) in original_passes
        .into_iter()
        .chain(physical_passes)
        .enumerate()
    {
        assert!(next > 0);
        let (result, admitted) = encode(declared.clone(), &metered_limits(maximum));
        let error = result.unwrap_err();
        assert_eq!(error.display, Some(owner(0)));
        assert_eq!(admitted.attribute_cell_updates, Some(maximum));
        let failed = compiled_attribute_failure(&error);
        assert_eq!(failed.reason, TeletextAttributeExecutionFailure::WorkBudget);
        let (trace, layers, commands) = if stage < 2 {
            (
                original.spatial_trace(),
                original.glyph_layers(),
                budget.composition.collection.max_commands,
            )
        } else {
            (
                physical.spatial_trace(),
                physical.glyph_layers(),
                budget.replay.max_commands,
            )
        };
        let (expected, used) = resolve_teletext_attribute_execution_with_usage(
            original.base_display(),
            trace,
            layers,
            original.colour_context().row_colours(),
            commands,
            Some(0),
        );
        assert_eq!(used, 0);
        assert_eq!(failed, &expected.unwrap_err());
        maximum += next;
    }
}

#[test]
fn empty_zero_credit_is_valid_but_real_work_fails_and_late_failure_retains_admission() {
    let empty = single(0);
    let tx = transmissions(&empty);
    let expected = encode_teletext_authored_service(empty.clone(), &tx, &limits()).unwrap();
    let (result, usage) = encode(empty, &metered_limits(0));
    assert_same(&expected, &result.unwrap());
    assert_eq!(usage.attribute_cell_updates, Some(0));
    let declared = mixed_displays();
    let (full, full_usage) = encode(declared.clone(), &metered_limits(usize::MAX));
    let mut late = metered_limits(usize::MAX);
    late.max_units = full.unwrap().unit_count() - 1;
    let (result, admitted) = encode(declared.clone(), &late);
    assert!(matches!(
        result.unwrap_err().reason,
        Failure::Budget {
            category: "units",
            ..
        }
    ));
    assert_eq!(admitted, full_usage);
    let (early, usage) =
        encode_teletext_authored_service_with_work_usage(declared, &[], &late, None);
    assert!(matches!(
        early.unwrap_err().reason,
        Failure::MissingTransmission
    ));
    assert_eq!(usage.attribute_cell_updates, Some(0));
}

#[test]
fn legacy_encoders_honor_attribute_cap_and_disabled_usage_is_unknown() {
    let declared = calling_displays(1);
    let tx = transmissions(&declared);
    let (full, usage) = encode(declared.clone(), &limits());
    full.unwrap();
    assert_eq!(usage.attribute_cell_updates, None);
    let budget = metered_limits(0);
    let error = encode_teletext_authored_service(declared.clone(), &tx, &budget).unwrap_err();
    assert_eq!(error.display, Some(owner(0)));
    assert_eq!(
        compiled_attribute_failure(&error).reason,
        TeletextAttributeExecutionFailure::WorkBudget
    );
    let (result, compatible_usage) =
        encode_teletext_authored_service_with_usage(declared, &tx, &budget, None);
    assert!(result.is_err());
    assert_eq!(compatible_usage, None);
}
