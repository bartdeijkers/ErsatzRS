//! Complete resource-backed services share separate planning/ancestry quotas.

use super::update_quota_tests::{assert_same, single};
use super::*;

#[derive(Clone, Copy)]
enum Bucket {
    Realization,
    Placement,
    Assembly,
    Ancestry,
}

const BUCKETS: [Bucket; 4] = [
    Bucket::Realization,
    Bucket::Placement,
    Bucket::Assembly,
    Bucket::Ancestry,
];

impl Bucket {
    fn set(self, budget: &mut TeletextAuthoredServiceEncodingBudget, maximum: Option<usize>) {
        match self {
            Self::Realization => budget.max_realization_updates = maximum,
            Self::Placement => budget.max_placement_work = maximum,
            Self::Assembly => budget.max_assembly_work = maximum,
            Self::Ancestry => budget.max_ancestry_work = maximum,
        }
    }

    fn usage(self, usage: TeletextAuthoredServiceEncodingUsage) -> Option<usize> {
        match self {
            Self::Realization => usage.realization_updates,
            Self::Placement => usage.placement_work,
            Self::Assembly => usage.assembly_work,
            Self::Ancestry => usage.ancestry_work,
        }
    }

    fn assert_failure(self, error: &TeletextAuthoredServiceEncodingError) {
        let Failure::Compiled(error) = &error.reason else {
            panic!("expected compiled-page quota rejection: {error:?}");
        };
        let TeletextCompiledDisplayPageEncodingFailure::Normalization(error) = &error.reason else {
            panic!("expected normalization quota rejection: {error:?}");
        };
        match self {
            Self::Realization => assert!(matches!(
                error.reason,
                TeletextNormalizedMainProgramFailure::Realization(_)
            )),
            Self::Placement => assert!(matches!(
                error.reason,
                TeletextNormalizedMainProgramFailure::Placement(_)
            )),
            Self::Assembly => assert!(matches!(
                error.reason,
                TeletextNormalizedMainProgramFailure::Assembly(_)
            )),
            Self::Ancestry => {
                let TeletextNormalizedMainProgramFailure::Compilation(error) = &error.reason else {
                    panic!("expected ancestry quota rejection: {error:?}");
                };
                assert!(matches!(
                    error.reason,
                    TeletextAugmentedOriginCompilationFailure::WorkBudget { .. }
                ));
            }
        }
    }
}

pub(super) fn calling_displays(count: u16) -> TeletextDeclaredService {
    let mut draft = input();
    draft
        .base
        .pages
        .retain(|page| page.address.subcode.get() < count);
    draft
        .pages
        .retain(|page| page.address.subcode.get() < count);
    for page in &mut draft.pages {
        page.local_program = Some(
            TeletextEnhancementProgram::new(vec![TeletextEnhancementCommand::InvokeObject {
                target: target(20),
                object_type: TeletextObjectType::Passive,
                offset: None,
            }])
            .unwrap(),
        );
    }
    let companions = vec![resource(TeletextResourceKind::Pop, 0xa0, 1)];
    let semantic = TeletextSemanticService::new_with_object_companions(
        draft,
        companions.clone(),
        semantic_limits(),
    )
    .unwrap();
    TeletextDeclaredService::new(
        semantic,
        vec![mot()],
        (0..count)
            .map(|subcode| TeletextDisplayMotSelection {
                display: owner(subcode),
                mot: Some(resource(TeletextResourceKind::Mot, 0xfe, 9)),
            })
            .collect(),
        companions,
        declaration_limits(),
    )
    .unwrap()
}

fn metered_limits() -> TeletextAuthoredServiceEncodingBudget {
    let mut budget = limits();
    for bucket in BUCKETS {
        bucket.set(&mut budget, Some(usize::MAX));
    }
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

#[test]
fn exact_planning_and_ancestry_quotas_preserve_original_physical_models_and_units() {
    let declared = calling_displays(2);
    let tx = transmissions(&declared);
    let expected = encode_teletext_authored_service(declared.clone(), &tx, &limits()).unwrap();
    let (full, usage) = encode(declared.clone(), &metered_limits());
    assert_same(&expected, &full.unwrap());
    let mut exact = limits();
    for bucket in BUCKETS {
        assert!(bucket.usage(usage).unwrap() > 0);
        bucket.set(&mut exact, bucket.usage(usage));
    }
    let (actual, exact_usage) = encode(declared.clone(), &exact);
    assert_eq!(exact_usage, usage);
    assert_same(&expected, &actual.unwrap());
    assert_same(
        &expected,
        &encode_teletext_authored_service(declared, &tx, &exact).unwrap(),
    );
}

#[test]
fn planning_credits_are_not_reset_between_two_resource_backed_displays() {
    let (first, first_usage) = encode(calling_displays(1), &metered_limits());
    first.unwrap();
    let declared = calling_displays(2);
    let (full, full_usage) = encode(declared.clone(), &metered_limits());
    full.unwrap();
    for bucket in BUCKETS {
        let first = bucket.usage(first_usage).unwrap();
        assert!(first > 0);
        assert_eq!(bucket.usage(full_usage).unwrap(), first * 2);
        let mut budget = metered_limits();
        bucket.set(&mut budget, Some(first));
        let (result, admitted) = encode(declared.clone(), &budget);
        let error = result.unwrap_err();
        assert_eq!(error.display, Some(owner(1)));
        bucket.assert_failure(&error);
        assert_eq!(bucket.usage(admitted), Some(first));
    }
}

#[test]
fn one_less_planning_credit_denies_final_display_without_charging_denied_work() {
    let declared = calling_displays(2);
    let (full, usage) = encode(declared.clone(), &metered_limits());
    full.unwrap();
    for bucket in BUCKETS {
        let maximum = bucket.usage(usage).unwrap() - 1;
        let mut budget = metered_limits();
        bucket.set(&mut budget, Some(maximum));
        let (result, admitted) = encode(declared.clone(), &budget);
        let error = result.unwrap_err();
        assert_eq!(error.display, Some(owner(1)));
        bucket.assert_failure(&error);
        assert!(bucket.usage(admitted).unwrap() <= maximum);
    }
}

#[test]
fn failure_debits_are_independent_and_later_planning_stages_remain_unreached() {
    let (first, first_usage) = encode(calling_displays(1), &metered_limits());
    first.unwrap();
    for (stage, bucket) in BUCKETS.into_iter().enumerate() {
        let mut budget = metered_limits();
        bucket.set(&mut budget, Some(0));
        let (result, admitted) = encode(calling_displays(2), &budget);
        let error = result.unwrap_err();
        assert_eq!(error.display, Some(owner(0)));
        bucket.assert_failure(&error);
        for (index, other) in BUCKETS.into_iter().enumerate() {
            if index < stage {
                assert_eq!(other.usage(admitted), other.usage(first_usage));
            } else {
                assert_eq!(other.usage(admitted), Some(0));
            }
        }
    }
}

#[test]
fn native_and_early_failures_report_zero_while_late_failure_retains_all_planning_charges() {
    let budget = metered_limits();
    let (native, native_usage) = encode(single(1), &budget);
    native.unwrap();
    for bucket in BUCKETS {
        assert_eq!(bucket.usage(native_usage), Some(0));
    }
    let declared = calling_displays(2);
    let (full, full_usage) = encode(declared.clone(), &budget);
    let mut late = budget;
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
    let (early, early_usage) =
        encode_teletext_authored_service_with_work_usage(declared, &[], &budget, None);
    assert!(matches!(
        early.unwrap_err().reason,
        Failure::MissingTransmission
    ));
    for bucket in BUCKETS {
        assert_eq!(bucket.usage(early_usage), Some(0));
    }
}

#[test]
fn legacy_encoders_honor_each_new_quota_and_disabled_buckets_remain_unknown() {
    let declared = calling_displays(2);
    let tx = transmissions(&declared);
    let (full, usage) = encode(declared.clone(), &limits());
    full.unwrap();
    for bucket in BUCKETS {
        assert_eq!(bucket.usage(usage), None);
        let mut budget = limits();
        bucket.set(&mut budget, Some(0));
        let error = encode_teletext_authored_service(declared.clone(), &tx, &budget).unwrap_err();
        assert_eq!(error.display, Some(owner(0)));
        bucket.assert_failure(&error);
        let (result, compatible_usage) =
            encode_teletext_authored_service_with_usage(declared.clone(), &tx, &budget, None);
        assert!(result.is_err());
        assert_eq!(compatible_usage, None);
    }
}
