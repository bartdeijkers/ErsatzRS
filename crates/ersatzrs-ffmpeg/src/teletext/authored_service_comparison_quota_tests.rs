//! Separate comparison credit remains cumulative without changing model or wire identity.

use super::update_quota_tests::{assert_same, single};
use super::*;

fn two_compiled() -> TeletextDeclaredService {
    let mut draft = input();
    draft.pages[1].local_program = Some(TeletextEnhancementProgram::new(vec![]).unwrap());
    declared(draft)
}

fn metered_limits() -> TeletextAuthoredServiceEncodingBudget {
    let mut budget = limits();
    budget.max_display_updates = Some(usize::MAX);
    budget.max_glyph_comparison_work = Some(usize::MAX);
    budget.max_display_comparison_updates = Some(usize::MAX);
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

fn comparison_failure(
    error: &TeletextAuthoredServiceEncodingError,
) -> &TeletextNormalizedMainProgramFailure {
    let Failure::Compiled(error) = &error.reason else {
        panic!("expected located compiled-page error: {error:?}");
    };
    let TeletextCompiledDisplayPageEncodingFailure::Normalization(error) = &error.reason else {
        panic!("expected original normalization error: {error:?}");
    };
    &error.reason
}

#[test]
fn exact_separate_comparison_quotas_preserve_models_units_and_compatibility_usage() {
    let declared = two_compiled();
    let tx = transmissions(&declared);
    let expected = encode_teletext_authored_service(declared.clone(), &tx, &limits()).unwrap();
    let (full, usage) = encode(declared.clone(), &metered_limits());
    assert_same(&expected, &full.unwrap());
    assert!(usage.display_updates.unwrap() > 0);
    assert!(usage.glyph_comparison_work.unwrap() > 0);
    assert!(usage.display_comparison_updates.unwrap() > 0);
    let mut exact = limits();
    exact.max_display_updates = usage.display_updates;
    exact.max_glyph_comparison_work = usage.glyph_comparison_work;
    exact.max_display_comparison_updates = usage.display_comparison_updates;
    let (result, admitted) = encode(declared.clone(), &exact);
    assert_eq!(admitted, usage);
    assert_same(&expected, &result.unwrap());
    let (result, compatible) =
        encode_teletext_authored_service_with_usage(declared, &tx, &exact, None);
    assert_eq!(compatible, usage.display_updates);
    assert_same(&expected, &result.unwrap());
}

#[test]
fn comparison_credits_are_independent_and_never_reset_for_the_next_display() {
    let (first, first_usage) = encode(single(0), &metered_limits());
    first.unwrap();
    let declared = two_compiled();
    let (full, full_usage) = encode(declared.clone(), &metered_limits());
    full.unwrap();
    assert_eq!(
        full_usage.glyph_comparison_work.unwrap(),
        first_usage.glyph_comparison_work.unwrap() * 2
    );
    assert_eq!(
        full_usage.display_comparison_updates.unwrap(),
        first_usage.display_comparison_updates.unwrap() * 2
    );
    for glyph in [true, false] {
        let mut budget = metered_limits();
        if glyph {
            budget.max_glyph_comparison_work = first_usage.glyph_comparison_work;
        } else {
            budget.max_display_comparison_updates = first_usage.display_comparison_updates;
        }
        let (result, usage) = encode(declared.clone(), &budget);
        let error = result.unwrap_err();
        assert_eq!(error.display, Some(owner(1)));
        if glyph {
            assert!(matches!(
                comparison_failure(&error),
                TeletextNormalizedMainProgramFailure::GlyphComparison(_)
            ));
            assert_eq!(
                usage.glyph_comparison_work,
                first_usage.glyph_comparison_work
            );
            assert_eq!(
                usage.display_comparison_updates,
                first_usage.display_comparison_updates
            );
        } else {
            assert!(matches!(
                comparison_failure(&error),
                TeletextNormalizedMainProgramFailure::DisplayComparison(_)
            ));
            assert_eq!(
                usage.glyph_comparison_work,
                full_usage.glyph_comparison_work
            );
            assert_eq!(
                usage.display_comparison_updates,
                first_usage.display_comparison_updates
            );
        }
        assert!(usage.display_updates.unwrap() > first_usage.display_updates.unwrap());
    }
}

#[test]
fn one_less_comparison_credit_returns_no_partial_service_and_never_charges_denied_work() {
    let declared = two_compiled();
    let (full, full_usage) = encode(declared.clone(), &metered_limits());
    full.unwrap();
    for glyph in [true, false] {
        let mut budget = metered_limits();
        if glyph {
            budget.max_glyph_comparison_work = Some(full_usage.glyph_comparison_work.unwrap() - 1);
        } else {
            budget.max_display_comparison_updates =
                Some(full_usage.display_comparison_updates.unwrap() - 1);
        }
        let (result, usage) = encode(declared.clone(), &budget);
        let error = result.unwrap_err();
        assert_eq!(error.display, Some(owner(1)));
        if glyph {
            assert!(matches!(
                comparison_failure(&error),
                TeletextNormalizedMainProgramFailure::GlyphComparison(_)
            ));
            assert!(
                usage.glyph_comparison_work.unwrap() < full_usage.glyph_comparison_work.unwrap()
            );
        } else {
            assert!(matches!(
                comparison_failure(&error),
                TeletextNormalizedMainProgramFailure::DisplayComparison(_)
            ));
            assert!(
                usage.display_comparison_updates.unwrap()
                    < full_usage.display_comparison_updates.unwrap()
            );
        }
    }
}

#[test]
fn enabled_buckets_report_zero_for_unreached_comparisons_and_preserve_late_failure_usage() {
    let budget = metered_limits();
    let (native, native_usage) = encode(single(1), &budget);
    native.unwrap();
    assert_eq!(native_usage.glyph_comparison_work, Some(0));
    assert_eq!(native_usage.display_comparison_updates, Some(0));
    assert!(native_usage.display_updates.unwrap() > 0);
    let declared = two_compiled();
    let (full, full_usage) = encode(declared.clone(), &budget);
    let mut late = budget;
    late.max_units = full.unwrap().unit_count() - 1;
    let (result, usage) = encode(declared.clone(), &late);
    assert!(matches!(
        result.unwrap_err().reason,
        Failure::Budget {
            category: "units",
            ..
        }
    ));
    assert_eq!(usage, full_usage);
    let (early, usage) =
        encode_teletext_authored_service_with_work_usage(declared, &[], &budget, None);
    assert!(matches!(
        early.unwrap_err().reason,
        Failure::MissingTransmission
    ));
    assert_eq!(usage.display_updates, Some(0));
    assert_eq!(usage.glyph_comparison_work, Some(0));
    assert_eq!(usage.display_comparison_updates, Some(0));
}

#[test]
fn legacy_encoder_honors_comparison_quotas_while_disabled_usage_remains_none() {
    let declared = two_compiled();
    let tx = transmissions(&declared);
    let (full, usage) = encode(declared.clone(), &limits());
    full.unwrap();
    assert_eq!(
        usage,
        TeletextAuthoredServiceEncodingUsage {
            display_updates: None,
            glyph_comparison_work: None,
            display_comparison_updates: None,
            realization_updates: None,
            placement_work: None,
            assembly_work: None,
            ancestry_work: None,
        }
    );
    let mut budget = limits();
    budget.max_glyph_comparison_work = Some(0);
    let error = encode_teletext_authored_service(declared.clone(), &tx, &budget).unwrap_err();
    assert_eq!(error.display, Some(owner(0)));
    assert!(matches!(
        comparison_failure(&error),
        TeletextNormalizedMainProgramFailure::GlyphComparison(_)
    ));
    let (error, compatible) =
        encode_teletext_authored_service_with_usage(declared, &tx, &budget, None);
    assert!(error.is_err());
    assert_eq!(compatible, None);
}
