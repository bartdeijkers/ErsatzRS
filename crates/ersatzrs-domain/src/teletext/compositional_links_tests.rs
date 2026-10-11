//! Authored declaration tests; not resource resolution or publication proof.

use super::*;
use serde_json::{json, Value};

fn null(magazine: u8) -> TeletextX27LinkTarget {
    TeletextX27LinkTarget::Unspecified {
        magazine: TeletextX27Magazine::new(magazine).unwrap(),
    }
}
fn page(magazine: u8, page: u8) -> TeletextX27LinkTarget {
    TeletextX27LinkTarget::Page {
        page: TeletextRawDataPageNumber::new(magazine, page).unwrap(),
    }
}
fn declaration(
    target: TeletextX27LinkTarget,
    validity: TeletextX27LinkValidity,
    mask: u16,
) -> TeletextX27LinkDeclaration {
    TeletextX27LinkDeclaration::new(target, validity, TeletextSubpageRequirementMask::new(mask))
        .unwrap()
}
fn complete() -> TeletextX27Level25Links {
    TeletextX27Level25Links::new(
        declaration(page(8, 0xA0), TeletextX27LinkValidity::Level25Only, 0x8001),
        declaration(page(2, 0xB1), TeletextX27LinkValidity::Level25And35, 0),
        declaration(null(7), TeletextX27LinkValidity::Level35Only, 0xffff),
        declaration(page(3, 0x42), TeletextX27LinkValidity::Level25Only, 0x5555),
    )
    .unwrap()
}

#[test]
fn every_sixteen_bit_mask_preserves_scalar_value_and_all_s1_directions() {
    for mask in 0_u16..=u16::MAX {
        let checked = TeletextSubpageRequirementMask::new(mask);
        assert_eq!(checked.get(), mask);
        assert_eq!(
            serde_json::from_value::<TeletextSubpageRequirementMask>(json!(mask)).unwrap(),
            checked
        );
        assert_eq!(serde_json::to_value(checked).unwrap(), json!(mask));
        for s1 in 0_u8..16 {
            assert_eq!(
                checked.requires(TeletextResourceSubpageIndex::new(s1).unwrap()),
                (mask >> s1) & 1 != 0
            );
        }
    }
    for malformed in [
        json!(-1),
        json!(65536),
        json!(1.5),
        json!("1"),
        json!(null),
        json!([1]),
        json!({"mask":1}),
    ] {
        assert!(serde_json::from_value::<TeletextSubpageRequirementMask>(malformed).is_err());
    }
}

#[test]
fn explicit_null_preserves_all_target_magazines_and_requires_ffff_even_at_level35() {
    for magazine in 1_u8..=8 {
        let target = null(magazine);
        assert_eq!(target.magazine(), magazine);
        assert_eq!(target.wire_magazine(), magazine & 7);
        assert_eq!(target.page_byte(), 0xff);
        for validity in [
            TeletextX27LinkValidity::Level25Only,
            TeletextX27LinkValidity::Level35Only,
            TeletextX27LinkValidity::Level25And35,
        ] {
            let checked = declaration(target, validity, 0xffff);
            assert_eq!(checked.target(), target);
            assert_eq!(checked.validity(), validity);
            assert_eq!(checked.required_subpages().get(), 0xffff);
        }
    }
    for mask in 0_u16..u16::MAX {
        assert_eq!(
            TeletextX27LinkDeclaration::new(
                null(8),
                TeletextX27LinkValidity::Level25Only,
                TeletextSubpageRequirementMask::new(mask)
            ),
            Err(TeletextX27LinkError::UnspecifiedMask { mask })
        );
    }
}

#[test]
fn target_magazine_bounds_are_checked_and_real_ff_is_never_a_resource_page() {
    for magazine in 0_u8..=255 {
        let parsed = serde_json::from_value::<TeletextX27Magazine>(json!(magazine));
        if (1..=8).contains(&magazine) {
            let checked = TeletextX27Magazine::new(magazine).unwrap();
            assert_eq!(parsed.unwrap(), checked);
            assert_eq!(checked.get(), magazine);
            assert_eq!(checked.wire_magazine(), magazine & 7);
        } else {
            assert_eq!(
                TeletextX27Magazine::new(magazine),
                Err(TeletextX27LinkError::Magazine { magazine })
            );
            assert!(parsed.is_err());
        }
    }
    for target in [
        json!({"kind":"page","page":{"magazine":2,"page":255}}),
        json!({"kind":"unspecified","magazine":0}),
        json!({"kind":"unspecified","magazine":9}),
    ] {
        assert!(serde_json::from_value::<TeletextX27LinkTarget>(target).is_err());
    }
}

#[test]
fn validity_accepts_only_assigned_strings_and_preserves_level35_metadata() {
    for (validity, name, code, applies) in [
        (
            TeletextX27LinkValidity::Level25Only,
            "level25_only",
            1,
            true,
        ),
        (
            TeletextX27LinkValidity::Level35Only,
            "level35_only",
            2,
            false,
        ),
        (
            TeletextX27LinkValidity::Level25And35,
            "level25_and35",
            3,
            true,
        ),
    ] {
        assert_eq!(
            serde_json::from_value::<TeletextX27LinkValidity>(json!(name)).unwrap(),
            validity
        );
        assert_eq!(serde_json::to_value(validity).unwrap(), json!(name));
        assert_eq!(validity.wire_code(), code);
        assert_eq!(validity.applies_at_level25(), applies);
    }
    for malformed in [
        json!("reserved"),
        json!("Level25Only"),
        json!(0),
        json!(1),
        json!(null),
        json!(["level25_only"]),
        json!({"level25_only":null}),
    ] {
        assert!(serde_json::from_value::<TeletextX27LinkValidity>(malformed).is_err());
    }
    assert_eq!(
        complete().link(TeletextX27LinkRole::Gdrcs).validity(),
        TeletextX27LinkValidity::Level35Only
    );
}

#[test]
fn fixed_roles_keep_direction_order_masks_and_raw_non_bcd_addresses() {
    let links = complete();
    let expected = [
        (TeletextResourceKind::Gpop, 0, 8, 0xA0, 0x8001),
        (TeletextResourceKind::Pop, 1, 2, 0xB1, 0),
        (TeletextResourceKind::Gdrcs, 2, 7, 0xff, 0xffff),
        (TeletextResourceKind::Drcs, 3, 3, 0x42, 0x5555),
    ];
    for (role, (kind, code, magazine, byte, mask)) in
        TeletextX27LinkRole::ALL.into_iter().zip(expected)
    {
        let link = links.link(role);
        assert_eq!(role.kind(), kind);
        assert_eq!(role.wire_code(), code);
        assert_eq!(link.target().magazine(), magazine);
        assert_eq!(link.target().page_byte(), byte);
        assert_eq!(link.required_subpages().get(), mask);
    }
    // Allocation collisions are separate from a declared link address.
    let same = declaration(page(2, 0xA0), TeletextX27LinkValidity::Level35Only, 0);
    assert!(TeletextX27Level25Links::new(same, same, same, same).is_ok());
}

#[test]
fn every_real_target_is_purpose_checked_at_its_named_role_even_when_mask_zero() {
    let harmless = declaration(null(2), TeletextX27LinkValidity::Level25Only, 0xffff);
    for role in TeletextX27LinkRole::ALL {
        for (magazine, byte) in [(2, 0xfd), (2, 0xfe), (1, 0xbe), (1, 0xf0)] {
            let target = TeletextRawDataPageNumber::new(magazine, byte).unwrap();
            let bad = declaration(
                TeletextX27LinkTarget::Page { page: target },
                TeletextX27LinkValidity::Level35Only,
                0,
            );
            let mut values = [harmless; 4];
            values[usize::from(role.wire_code())] = bad;
            assert_eq!(
                TeletextX27Level25Links::new(values[0], values[1], values[2], values[3]),
                Err(TeletextX27LinkError::TargetPurpose {
                    role,
                    error: TeletextResourceIdentityError::ReservedPurpose {
                        kind: role.kind(),
                        page: target
                    }
                })
            );
        }
    }
    for byte in [0xbe, 0xf0] {
        let allowed = declaration(page(2, byte), TeletextX27LinkValidity::Level25Only, 0);
        assert!(TeletextX27Level25Links::new(allowed, allowed, allowed, allowed).is_ok());
    }
}

#[test]
fn complete_named_declaration_roundtrips_without_implicit_packet_or_mask_defaults() {
    let original = complete();
    let wire = serde_json::to_value(original).unwrap();
    assert_eq!(
        wire,
        json!({
            "gpop":{"target":{"kind":"page","page":{"magazine":8,"page":160}},"validity":"level25_only","required_subpages":32769},
            "pop":{"target":{"kind":"page","page":{"magazine":2,"page":177}},"validity":"level25_and35","required_subpages":0},
            "gdrcs":{"target":{"kind":"unspecified","magazine":7},"validity":"level35_only","required_subpages":65535},
            "drcs":{"target":{"kind":"page","page":{"magazine":3,"page":66}},"validity":"level25_only","required_subpages":21845}
        })
    );
    assert_eq!(
        serde_json::from_value::<TeletextX27Level25Links>(wire.clone()).unwrap(),
        original
    );
    assert_eq!(
        serde_json::from_value::<Option<TeletextX27Level25Links>>(Value::Null).unwrap(),
        None
    );
    for name in ["gpop", "pop", "gdrcs", "drcs"] {
        let mut missing = wire.clone();
        missing.as_object_mut().unwrap().remove(name);
        assert!(serde_json::from_value::<TeletextX27Level25Links>(missing).is_err());
    }
    for field in ["target", "validity", "required_subpages"] {
        let mut missing = wire.clone();
        missing["gpop"].as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<TeletextX27Level25Links>(missing).is_err());
    }
}

#[test]
fn closed_map_json_rejects_unknown_fields_duplicates_arrays_and_constructor_bypasses() {
    let wire = serde_json::to_value(complete()).unwrap();
    let mut cases = vec![
        json!([]),
        json!([
            wire["gpop"].clone(),
            wire["pop"].clone(),
            wire["gdrcs"].clone(),
            wire["drcs"].clone()
        ]),
    ];
    for path in [
        vec![],
        vec!["gpop"],
        vec!["gpop", "target"],
        vec!["gpop", "target", "page"],
    ] {
        let mut changed = wire.clone();
        let mut node = &mut changed;
        for key in path {
            node = &mut node[key];
        }
        node.as_object_mut()
            .unwrap()
            .insert("unexpected".to_owned(), json!(1));
        cases.push(changed);
    }
    let mut positional = wire.clone();
    positional["gpop"] = json!([wire["gpop"]["target"], "level25_only", 32769]);
    cases.push(positional);
    let mut bad_null = wire.clone();
    bad_null["gdrcs"]["required_subpages"] = json!(0);
    cases.push(bad_null);
    let mut reserved = wire.clone();
    reserved["pop"]["target"]["page"]["page"] = json!(254);
    cases.push(reserved);
    for malformed in cases {
        assert!(serde_json::from_value::<TeletextX27Level25Links>(malformed).is_err());
    }
    for malformed in [
        json!([]),
        json!(["unspecified", 2]),
        json!({"kind":"unspecified","magazine":2,"page":{"magazine":2,"page":160}}),
        json!({"kind":"page","page":[2,160]}),
    ] {
        assert!(serde_json::from_value::<TeletextX27LinkTarget>(malformed).is_err());
    }
    let serialized = serde_json::to_string(&wire).unwrap();
    let duplicated_root = serialized.replacen('{', &format!("{{\"gpop\":{},", wire["gpop"]), 1);
    assert!(serde_json::from_str::<TeletextX27Level25Links>(&duplicated_root).is_err());
    assert!(serde_json::from_str::<TeletextX27LinkDeclaration>(r#"{"target":{"kind":"unspecified","magazine":2},"validity":"level25_only","required_subpages":65535,"required_subpages":65535}"#).is_err());
    for duplicate in [
        r#"{"kind":"unspecified","kind":"unspecified","magazine":2}"#,
        r#"{"kind":"unspecified","magazine":2,"magazine":2}"#,
    ] {
        assert!(serde_json::from_str::<TeletextX27LinkTarget>(duplicate).is_err());
    }
}
