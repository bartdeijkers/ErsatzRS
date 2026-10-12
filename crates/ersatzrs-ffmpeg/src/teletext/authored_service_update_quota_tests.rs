//! Shared admitted-update credit covers every display and discarded earlier pass.

use super::*;

fn single(subcode: u16) -> TeletextDeclaredService {
    let mut draft = input();
    draft
        .base
        .pages
        .retain(|page| page.address == owner(subcode));
    draft.pages.retain(|page| page.address == owner(subcode));
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
        vec![TeletextDisplayMotSelection {
            display: owner(subcode),
            mot: Some(resource(TeletextResourceKind::Mot, 0xfe, 9)),
        }],
        companions,
        declaration_limits(),
    )
    .unwrap()
}

fn metered(
    declared: TeletextDeclaredService,
    maximum: usize,
) -> (
    Result<EncodedTeletextAuthoredService, TeletextAuthoredServiceEncodingError>,
    usize,
) {
    let tx = transmissions(&declared);
    let mut budget = limits();
    budget.max_display_updates = Some(maximum);
    let (result, usage) = encode_teletext_authored_service_with_usage(declared, &tx, &budget, None);
    (result, usage.unwrap())
}

fn assert_same(expected: &EncodedTeletextAuthoredService, actual: &EncodedTeletextAuthoredService) {
    assert_eq!(actual.declared(), expected.declared());
    assert_eq!(actual.inventory(), expected.inventory());
    assert_eq!(actual.unit_count(), expected.unit_count());
    assert_eq!(actual.magazine_units(), expected.magazine_units());
    assert_eq!(actual.pages().len(), expected.pages().len());
    for (left, right) in actual.pages().iter().zip(expected.pages()) {
        assert_eq!(left.data_units(), right.data_units());
        assert_eq!(left.resource_header(), right.resource_header());
        match (left, right) {
            (
                EncodedTeletextAuthoredPage::NativeDisplay(left),
                EncodedTeletextAuthoredPage::NativeDisplay(right),
            ) => {
                assert_eq!(left.composition(), right.composition());
            }
            (
                EncodedTeletextAuthoredPage::CompiledDisplay(left),
                EncodedTeletextAuthoredPage::CompiledDisplay(right),
            ) => {
                assert_eq!(left.normalized().original(), right.normalized().original());
                assert_eq!(left.normalized().physical(), right.normalized().physical());
            }
            (EncodedTeletextAuthoredPage::Object(_), EncodedTeletextAuthoredPage::Object(_))
            | (
                EncodedTeletextAuthoredPage::Resource(_),
                EncodedTeletextAuthoredPage::Resource(_),
            ) => {}
            _ => panic!("quota must not switch the existing encoding path"),
        }
    }
}

#[test]
fn shared_update_quota_preserves_models_wire_and_exact_inclusive_boundary() {
    let original = declared(input());
    let tx = transmissions(&original);
    let expected = encode_teletext_authored_service(original.clone(), &tx, &limits()).unwrap();
    let (actual, used) = metered(original.clone(), usize::MAX);
    let actual = actual.unwrap();
    assert!(used > 0);
    assert_same(&expected, &actual);
    let (exact, exact_used) = metered(original.clone(), used);
    assert_eq!(exact_used, used);
    assert_same(&expected, &exact.unwrap());
    let (denied, denied_used) = metered(original, used - 1);
    let denied = denied.unwrap_err();
    assert_eq!(denied.display, Some(owner(1)));
    assert!(denied_used < used);
}

#[test]
fn shared_credit_is_not_reset_between_compiled_and_native_displays() {
    let (compiled, compiled_used) = metered(single(0), usize::MAX);
    let (native, native_used) = metered(single(1), usize::MAX);
    compiled.unwrap();
    native.unwrap();
    let (complete, combined) = metered(declared(input()), usize::MAX);
    complete.unwrap();
    assert_eq!(combined, compiled_used.checked_add(native_used).unwrap());
    let maximum = compiled_used.max(native_used);
    let (result, admitted) = metered(declared(input()), maximum);
    assert_eq!(result.unwrap_err().display, Some(owner(1)));
    assert!(admitted >= compiled_used);
    assert!(admitted <= maximum);
}

#[test]
fn failure_retains_admitted_work_and_zero_quota_returns_no_partial_service() {
    let original = declared(input());
    let before = original.clone();
    let (result, used) = metered(original.clone(), 0);
    assert_eq!(used, 0);
    assert_eq!(result.unwrap_err().display, Some(owner(0)));
    let (full, full_used) = metered(original.clone(), usize::MAX);
    let mut budget = limits();
    budget.max_display_updates = Some(full_used);
    budget.max_units = full.unwrap().unit_count() - 1;
    let tx = transmissions(&original);
    let (late, late_used) =
        encode_teletext_authored_service_with_usage(original.clone(), &tx, &budget, None);
    assert!(matches!(
        late.unwrap_err().reason,
        Failure::Budget {
            category: "units",
            ..
        }
    ));
    assert_eq!(late_used, Some(full_used));
    let (early, early_used) =
        encode_teletext_authored_service_with_usage(original.clone(), &[], &budget, None);
    assert!(matches!(
        early.unwrap_err().reason,
        Failure::MissingTransmission
    ));
    assert_eq!(early_used, Some(0));
    assert_eq!(original, before);
}

#[test]
fn disabled_aggregate_quota_reports_unknown_usage_and_keeps_existing_units() {
    let original = declared(input());
    let tx = transmissions(&original);
    let expected = encode_teletext_authored_service(original.clone(), &tx, &limits()).unwrap();
    let (actual, usage) =
        encode_teletext_authored_service_with_usage(original, &tx, &limits(), None);
    assert!(usage.is_none());
    assert_same(&expected, &actual.unwrap());
}
