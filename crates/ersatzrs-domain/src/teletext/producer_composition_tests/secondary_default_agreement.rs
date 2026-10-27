//! Explicit enrollment preserves source uncertainty and every unrelated effect.
use super::*;

fn agreement(secondary: Option<u8>) -> TeletextSecondaryDefaultAgreement {
    TeletextSecondaryDefaultAgreement::new(secondary.map_or(
        TeletextSecondaryDefaultChoice::NoSecondG0 {},
        |code| TeletextSecondaryDefaultChoice::Designated {
            designation: TeletextPageCharsetDesignation::new(code).unwrap(),
        },
    ))
}

#[test]
fn explicit_secondary_default_agreement_derives_only_header_pop_page_context_and_keeps_originals() {
    for use_pop in [false, true] {
        for secondary in [None, Some(1), Some(0x10)] {
            let original = if use_pop {
                pop(page(200, 0), 14)
            } else {
                producer(draft(vec![page(200, 0)]), vec![])
            };
            let before = original.original().clone();
            let snapshot = original.pages()[0];
            let agreed = agreement(secondary);
            let foreign = theme(0x37, [0, 0, 15], 9);
            let result = compose(
                vec![
                    original.with_secondary_default_agreement(Some(agreed)),
                    producer(themed(vec![page(201, 0)], foreign), vec![]),
                ],
                address(200, 0),
            )
            .unwrap();
            let retained = &result.producers()[0];
            assert_eq!(retained.original(), &before);
            assert_eq!(retained.pages(), &[snapshot]);
            assert_eq!(
                snapshot.secondary_selection(),
                TeletextProducerSecondarySelection::DecoderDependent
            );
            assert_eq!(retained.secondary_default_agreement(), Some(agreed));
            assert_eq!(
                result.provenance()[0].derivation(),
                TeletextProducerPageDerivation::SecondaryDefaultAgreement { agreement: agreed }
            );
            let service = result.composed().semantic();
            assert!(service.magazine_presentations().is_empty());
            let mut expected = *snapshot.effective_defaults();
            expected.secondary = agreed.secondary();
            assert_eq!(
                service.page_enhancements()[&address(200, 0)].presentation,
                Some(expected)
            );
            assert_eq!(
                service.page_enhancements()[&address(201, 0)].presentation,
                Some(foreign)
            );
            assert_eq!(service.base().pages[0], before.semantic().base().pages[0]);
            assert_eq!(service.objects(), before.semantic().objects());
            assert_eq!(service.drcs(), before.semantic().drcs());
            assert_eq!(result.composed().mots(), before.mots());
            let old = &before.resolutions()[0];
            let new = &result.composed().resolutions()[0];
            assert_eq!(old.defaults(), new.defaults());
            assert_eq!(
                old.conditional_pop_fallback(),
                new.conditional_pop_fallback()
            );
            for role in TeletextX27LinkRole::ALL {
                assert_eq!(old.role(role), new.role(role));
            }
        }
    }
}

#[test]
fn agreement_never_overrides_declared_context_or_turns_removal_into_an_implicit_default() {
    for secondary in [None, Some(1)] {
        let agreed = agreement(secondary);
        let original = producer(
            themed(vec![page(200, 0)], theme(0x37, [0, 0, 15], 9)),
            vec![],
        );
        let plain = compose(vec![original], address(200, 0)).unwrap();
        let enrolled = producer(
            themed(vec![page(200, 0)], theme(0x37, [0, 0, 15], 9)),
            vec![],
        )
        .with_secondary_default_agreement(Some(agreed));
        let result = compose(vec![enrolled], address(200, 0)).unwrap();
        assert_eq!(result.composed(), plain.composed());
        assert_eq!(
            result.pages()[0].derivation(),
            TeletextProducerPageDerivation::Unchanged
        );
        let cleared = producer(draft(vec![page(200, 0)]), vec![])
            .with_secondary_default_agreement(Some(agreed))
            .with_secondary_default_agreement(None);
        let result = compose(vec![cleared], address(200, 0)).unwrap();
        assert_eq!(
            result.producers()[0].pages()[0].secondary_selection(),
            TeletextProducerSecondarySelection::DecoderDependent
        );
        assert!(!result
            .composed()
            .semantic()
            .page_enhancements()
            .contains_key(&address(200, 0)));
        assert_eq!(
            result.pages()[0].derivation(),
            TeletextProducerPageDerivation::Unchanged
        );
    }
}

#[test]
fn secondary_default_agreement_keeps_owned_parts_and_existing_admission_limits() {
    let agreed = agreement(Some(0x10));
    let original = producer(draft(vec![page(200, 0)]), vec![]);
    let before = original.original().clone();
    let (retained, snapshots, policy) = original
        .with_secondary_default_agreement(Some(agreed))
        .into_parts();
    assert_eq!(retained, before);
    assert_eq!(
        snapshots[0].secondary_selection(),
        TeletextProducerSecondarySelection::DecoderDependent
    );
    assert_eq!(policy, Some(agreed));
    let mut limited = budget();
    limited.max_work = 0;
    let result = TeletextColdProducerComposition::new(
        vec![producer(draft(vec![page(200, 0)]), vec![])
            .with_secondary_default_agreement(Some(agreed))],
        TeletextProducerCompositionMetadata {
            language: language("nld"),
            entry: address(200, 0),
        },
        limited,
    );
    assert!(matches!(
        result.unwrap_err().reason,
        TeletextProducerCompositionFailure::Budget { .. }
    ));
}
