use super::*;

fn literal(key: &str, value: String) -> TeletextProfileSetting {
    TeletextProfileSetting {
        key: key.to_owned(),
        value: TeletextProfileValue::Literal(value),
    }
}

#[test]
fn profile_debug_never_contains_literal_or_reference_content() {
    for value in [
        TeletextProfileValue::Literal("private fixture text".to_owned()),
        TeletextProfileValue::SecretReference {
            kind: TeletextSecretReferenceKind::File,
            reference: "private fixture location".to_owned(),
        },
    ] {
        let setting = TeletextProfileSetting {
            key: "SETTING".to_owned(),
            value,
        };
        assert!(!format!("{setting:?}").contains("private fixture"));
        assert!(format!("{setting:?}").contains("SETTING"));
    }
}

#[test]
fn settings_bounds_count_utf8_bytes_and_reject_duplicate_keys() {
    let maximum = vec![literal(
        "KEY",
        "é".repeat(MAX_TELETEXT_PROFILE_VALUE_BYTES / 2),
    )];
    assert!(validate_teletext_profile_values("Profile", &maximum).is_ok());
    assert_eq!(
        validate_teletext_profile_values(
            "Profile",
            &[literal(
                "KEY",
                "é".repeat(MAX_TELETEXT_PROFILE_VALUE_BYTES / 2 + 1)
            )]
        ),
        Err(TeletextProfileValueError::SettingsBound)
    );
    assert_eq!(
        validate_teletext_profile_values(
            "Profile",
            &[literal("KEY", String::new()), literal("KEY", String::new())]
        ),
        Err(TeletextProfileValueError::SettingKey)
    );
    assert!(validate_teletext_profile_values(
        "Profile",
        &[literal("KEY", "zero\0byte".to_owned())]
    )
    .is_err());
}

#[test]
fn aggregate_settings_and_row_count_have_exact_resource_boundaries() {
    let mut bag = (0..4)
        .map(|i| {
            literal(
                &format!("K{i}"),
                "x".repeat(MAX_TELETEXT_PROFILE_VALUE_BYTES - 2),
            )
        })
        .collect::<Vec<_>>();
    assert!(validate_teletext_profile_values("Profile", &bag).is_ok());
    if let TeletextProfileValue::Literal(value) = &mut bag[0].value {
        value.push('x');
    }
    assert_eq!(
        validate_teletext_profile_values("Profile", &bag),
        Err(TeletextProfileValueError::SettingsBound)
    );
    let mut rows = (0..MAX_TELETEXT_PROFILE_SETTINGS)
        .map(|i| literal(&format!("K{i}"), String::new()))
        .collect::<Vec<_>>();
    assert!(validate_teletext_profile_values("Profile", &rows).is_ok());
    rows.push(literal("EXTRA", String::new()));
    assert_eq!(
        validate_teletext_profile_values("Profile", &rows),
        Err(TeletextProfileValueError::SettingsBound)
    );
}

#[test]
fn names_require_nonblank_bounded_text_without_controls() {
    assert!(validate_teletext_profile_values(
        &"é".repeat(MAX_TELETEXT_PROFILE_NAME_BYTES / 2),
        &[]
    )
    .is_ok());
    for name in [
        String::new(),
        "   ".to_owned(),
        "name\n".to_owned(),
        "é".repeat(MAX_TELETEXT_PROFILE_NAME_BYTES / 2 + 1),
    ] {
        assert_eq!(
            validate_teletext_profile_values(&name, &[]),
            Err(TeletextProfileValueError::Name)
        );
    }
}
