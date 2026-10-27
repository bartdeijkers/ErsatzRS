use super::*;
use serde_json::json;

#[test]
fn secondary_default_agreement_requires_explicit_version_and_choice_without_alias_rewriting() {
    let no_second =
        TeletextSecondaryDefaultAgreement::new(TeletextSecondaryDefaultChoice::NoSecondG0 {});
    let value = json!({"policy":"ersatzrs-secondary-default-v1","choice":{"kind":"no_second_g0"}});
    assert_eq!(serde_json::to_value(no_second).unwrap(), value);
    assert_eq!(
        serde_json::from_value::<TeletextSecondaryDefaultAgreement>(value).unwrap(),
        no_second
    );
    assert_eq!(
        no_second.policy_id(),
        TELETEXT_SECONDARY_DEFAULT_AGREEMENT_V1
    );
    assert_eq!(no_second.secondary(), None);
    for code in 0..=u8::MAX {
        let value = json!({"policy":"ersatzrs-secondary-default-v1", "choice":{"kind":"designated","designation":code}});
        let actual = serde_json::from_value::<TeletextSecondaryDefaultAgreement>(value.clone());
        match TeletextPageCharsetDesignation::new(code) {
            Ok(designation) => {
                let agreement = actual.unwrap();
                assert_eq!(agreement.secondary(), Some(designation));
                assert_eq!(serde_json::to_value(agreement).unwrap(), value);
            }
            Err(_) => assert!(actual.is_err(), "reserved designation {code}"),
        }
    }
}

#[test]
fn secondary_default_agreement_rejects_missing_null_unknown_and_duplicate_author_intent() {
    for text in [
        "{}",
        r#"{"choice":{"kind":"no_second_g0"}}"#,
        r#"{"policy":"ersatzrs-secondary-default-v1"}"#,
        r#"{"policy":"ersatzrs-secondary-default-v1","choice":null}"#,
        r#"{"policy":"ersatzrs-secondary-default-v2","choice":{"kind":"no_second_g0"}}"#,
        r#"{"policy":"ersatzrs-secondary-default-v1","choice":{"kind":"no_second_g0","designation":1}}"#,
        r#"{"policy":"ersatzrs-secondary-default-v1","choice":{"kind":"designated"}}"#,
        r#"{"policy":"ersatzrs-secondary-default-v1","choice":{"kind":"designated","designation":127}}"#,
        r#"{"policy":"ersatzrs-secondary-default-v1","choice":{"kind":"no_second_g0"},"scope":"other-producer"}"#,
        r#"{"policy":"ersatzrs-secondary-default-v1","policy":"ersatzrs-secondary-default-v1","choice":{"kind":"no_second_g0"}}"#,
        r#"{"policy":"ersatzrs-secondary-default-v1","choice":{"kind":"no_second_g0"},"choice":{"kind":"designated","designation":1}}"#,
    ] {
        assert!(
            serde_json::from_str::<TeletextSecondaryDefaultAgreement>(text).is_err(),
            "unexpected accepted policy: {text}"
        );
    }
}
