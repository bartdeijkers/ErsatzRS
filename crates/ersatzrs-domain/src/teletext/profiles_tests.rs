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
fn candidate_debug_redacts_package_identity_addon_and_effective_settings() {
    let candidate = TeletextProfileCandidateSnapshot {
        contract: TeletextPageContract::V3,
        channel_id: 2,
        addon_id: "private fixture addon".to_owned(),
        profile_id: 17,
        profile_revision: 9,
        installation: TeletextInstallationSnapshot {
            addon_id: "private fixture addon".to_owned(),
            state_revision: "private fixture state".to_owned(),
            version: "private fixture version".to_owned(),
            package_sha256: "private fixture package".to_owned(),
            content_sha256: "private fixture content".to_owned(),
            manifest_sha256: "private fixture manifest".to_owned(),
        },
        effective_settings: vec![literal(
            "private fixture key",
            "private fixture value".to_owned(),
        )],
    };
    let debug = format!("{candidate:?}");
    assert!(!debug.contains("private fixture"));
    assert!(debug.contains("channel_id: 2"));
    assert!(debug.contains("profile_id: 17"));
    assert!(debug.contains("settings_count: 1"));
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
