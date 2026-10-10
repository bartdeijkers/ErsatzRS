use serde_json::json;

use super::*;

fn display(page: u16, subcode: u16) -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(page).unwrap(),
        subcode: TeletextSubcode::new(subcode).unwrap(),
    }
}

fn resource(kind: TeletextResourceKind) -> TeletextResourceIdentity {
    let page = match kind {
        TeletextResourceKind::Mot => 0xfe,
        TeletextResourceKind::Mip => 0xfd,
        _ => 0xa1,
    };
    let subpage = if kind == TeletextResourceKind::Mip {
        0
    } else {
        15
    };
    TeletextResourceIdentity::new(
        kind,
        TeletextRawDataPageNumber::new(8, page).unwrap(),
        TeletextResourceSubpageIndex::new(subpage).unwrap(),
    )
    .unwrap()
}

#[test]
fn object_type_invocation_matrix_has_only_strictly_higher_priority_edges() {
    let kinds = [
        TeletextObjectType::Active,
        TeletextObjectType::Adaptive,
        TeletextObjectType::Passive,
    ];
    let expected = [
        [false, true, true],
        [false, false, true],
        [false, false, false],
    ];
    for (source, row) in kinds.into_iter().zip(expected) {
        for (target, permitted) in kinds.into_iter().zip(row) {
            assert_eq!(
                source.may_invoke(target),
                permitted,
                "{source:?}->{target:?}"
            );
        }
    }
}

#[test]
fn logical_ids_preserve_zero_and_values_outside_physical_pointer_ranges() {
    for id in [0, 1, 511, 512, u16::MAX] {
        let value = TeletextObjectId::new(id);
        assert_eq!(value.get(), id);
        assert_eq!(serde_json::to_value(value).unwrap(), json!(id));
        assert_eq!(
            serde_json::from_value::<TeletextObjectId>(json!(id)).unwrap(),
            value
        );
    }
    for invalid in [json!(-1), json!(65536), json!(1.5), json!("1"), json!(null)] {
        assert!(serde_json::from_value::<TeletextObjectId>(invalid).is_err());
    }
}

#[test]
fn local_owner_retains_raw_subcode_and_isolates_rotating_display_pages() {
    let owner = display(888, 0x3f7f);
    let id = TeletextObjectId::new(0);
    let value = TeletextObjectIdentity::local(owner, id);
    assert_eq!(value.scope(), TeletextObjectScope::Local);
    assert_eq!(value.object_id(), id);
    assert_eq!(value.owner(), Some(owner));
    assert_eq!(value.resource(), None);
    assert_eq!(value.validate_display_owner(owner), Ok(()));
    for other in [display(888, 0x1234), display(889, 0x3f7f)] {
        assert_eq!(
            value.validate_display_owner(other),
            Err(TeletextObjectIdentityError::LocalOwner {
                object_id: id,
                owner,
                display: other,
            })
        );
        assert_ne!(value, TeletextObjectIdentity::local(other, id));
    }
}

#[test]
fn resource_scope_rejects_every_wrong_purpose_and_has_no_display_owner() {
    let id = TeletextObjectId::new(u16::MAX);
    for kind in [
        TeletextResourceKind::Pop,
        TeletextResourceKind::Gpop,
        TeletextResourceKind::Drcs,
        TeletextResourceKind::Gdrcs,
        TeletextResourceKind::Mot,
        TeletextResourceKind::Mip,
    ] {
        let location = resource(kind);
        for (scope, result, accepted) in [
            (
                TeletextObjectScope::Public,
                TeletextObjectIdentity::public(location, id),
                kind == TeletextResourceKind::Pop,
            ),
            (
                TeletextObjectScope::Global,
                TeletextObjectIdentity::global(location, id),
                kind == TeletextResourceKind::Gpop,
            ),
        ] {
            if accepted {
                let value = result.unwrap();
                assert_eq!(value.scope(), scope);
                assert_eq!(value.resource(), Some(location));
                assert_eq!(value.owner(), None);
                assert_eq!(value.object_id(), id);
                assert_eq!(value.validate_display_owner(display(200, 0)), Ok(()));
                assert_eq!(value.validate_display_owner(display(899, 0x1234)), Ok(()));
            } else {
                assert_eq!(
                    result,
                    Err(TeletextObjectIdentityError::WrongResourceKind {
                        scope,
                        resource: location,
                    })
                );
            }
        }
    }
}

#[test]
fn tagged_identity_roundtrip_preserves_owner_resource_and_logical_id() {
    let values = [
        TeletextObjectIdentity::local(display(200, 0x1234), TeletextObjectId::new(0)),
        TeletextObjectIdentity::public(
            resource(TeletextResourceKind::Pop),
            TeletextObjectId::new(512),
        )
        .unwrap(),
        TeletextObjectIdentity::global(
            resource(TeletextResourceKind::Gpop),
            TeletextObjectId::new(u16::MAX),
        )
        .unwrap(),
    ];
    assert_eq!(
        serde_json::to_value(values[0]).unwrap(),
        json!({"scope": "local", "owner": {"page": 200, "subcode": 4660}, "object_id": 0})
    );
    for value in values {
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(
            serde_json::from_str::<TeletextObjectIdentity>(&json).unwrap(),
            value
        );
    }
}

#[test]
fn object_scope_and_type_remain_independent_for_all_nine_classes() {
    let identities = [
        TeletextObjectIdentity::local(display(200, 0), TeletextObjectId::new(0)),
        TeletextObjectIdentity::public(
            resource(TeletextResourceKind::Pop),
            TeletextObjectId::new(0),
        )
        .unwrap(),
        TeletextObjectIdentity::global(
            resource(TeletextResourceKind::Gpop),
            TeletextObjectId::new(0),
        )
        .unwrap(),
    ];
    for identity in identities {
        for (kind, serialized) in [
            (TeletextObjectType::Active, "active"),
            (TeletextObjectType::Adaptive, "adaptive"),
            (TeletextObjectType::Passive, "passive"),
        ] {
            let pair = (identity, kind);
            let value = serde_json::to_value(pair).unwrap();
            assert_eq!(value[1], json!(serialized));
            assert_eq!(
                serde_json::from_value::<(TeletextObjectIdentity, TeletextObjectType)>(value)
                    .unwrap(),
                pair
            );
        }
        assert_eq!(
            serde_json::from_value::<TeletextObjectScope>(json!(identity.scope())).unwrap(),
            identity.scope()
        );
    }
    assert!(serde_json::from_str::<TeletextObjectType>("\"Active\"").is_err());
    assert!(serde_json::from_str::<TeletextObjectScope>("\"pop\"").is_err());
}

#[test]
fn tagged_identity_json_rejects_wrong_scope_fields_and_resource_purpose() {
    let local = json!({"scope": "local", "owner": {"page": 200, "subcode": 0}, "object_id": 0});
    let public = serde_json::to_value(
        TeletextObjectIdentity::public(
            resource(TeletextResourceKind::Pop),
            TeletextObjectId::new(1),
        )
        .unwrap(),
    )
    .unwrap();
    for (base, invalid_field, invalid_value) in [
        (&local, "resource", public["resource"].clone()),
        (&public, "owner", local["owner"].clone()),
        (&local, "pointer", json!(0)),
    ] {
        let mut changed = base.clone();
        changed[invalid_field] = invalid_value;
        assert!(serde_json::from_value::<TeletextObjectIdentity>(changed).is_err());
    }
    for scope in ["global", "local", "other"] {
        let mut changed = public.clone();
        changed["scope"] = json!(scope);
        assert!(serde_json::from_value::<TeletextObjectIdentity>(changed).is_err());
    }
    for (document, address_field) in [(local, "owner"), (public, "resource")] {
        for required in ["scope", "object_id", address_field] {
            let mut changed = document.clone();
            changed.as_object_mut().unwrap().remove(required);
            assert!(serde_json::from_value::<TeletextObjectIdentity>(changed).is_err());
        }
    }
}

#[test]
fn nested_identity_json_cannot_bypass_checked_display_and_resource_values() {
    let local = json!({"scope": "local", "owner": {"page": 200, "subcode": 0}, "object_id": 0});
    for (field, invalid) in [
        ("page", json!(900)),
        ("subcode", json!(128)),
        ("extra", json!(0)),
    ] {
        let mut changed = local.clone();
        changed["owner"][field] = invalid;
        assert!(serde_json::from_value::<TeletextObjectIdentity>(changed).is_err());
    }
    let public = serde_json::to_value(
        TeletextObjectIdentity::public(
            resource(TeletextResourceKind::Pop),
            TeletextObjectId::new(0),
        )
        .unwrap(),
    )
    .unwrap();
    for (field, invalid) in [
        ("kind", json!("gdrcs")),
        ("logical_subpage", json!(16)),
        ("update_indicator", json!(1)),
    ] {
        let mut changed = public.clone();
        changed["resource"][field] = invalid;
        assert!(serde_json::from_value::<TeletextObjectIdentity>(changed).is_err());
    }
    let mut changed = public;
    changed["resource"]["page"]["page"] = json!(255);
    assert!(serde_json::from_value::<TeletextObjectIdentity>(changed).is_err());
}
