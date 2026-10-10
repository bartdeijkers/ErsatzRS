//! Synthetic resource addresses; identities alone are not encoded resources.

use serde_json::json;

use super::{
    TeletextRawDataPageNumber, TeletextResourceIdentity, TeletextResourceIdentityError,
    TeletextResourceKind, TeletextResourceSubpageIndex,
};

const KINDS: [TeletextResourceKind; 6] = [
    TeletextResourceKind::Pop,
    TeletextResourceKind::Gpop,
    TeletextResourceKind::Drcs,
    TeletextResourceKind::Gdrcs,
    TeletextResourceKind::Mot,
    TeletextResourceKind::Mip,
];

#[test]
fn raw_pages_preserve_all_magazines_and_page_bytes_without_bcd_conversion() {
    for magazine in 1..=8 {
        for page in 0..=0xfe {
            let number = TeletextRawDataPageNumber::new(magazine, page).unwrap();
            assert_eq!(number.magazine(), magazine);
            assert_eq!(number.page(), page);
            assert_eq!(number.wire_magazine(), magazine % 8);
            assert_eq!(
                serde_json::to_value(number).unwrap(),
                json!({"magazine": magazine, "page": page})
            );
        }
        assert_eq!(
            TeletextRawDataPageNumber::new(magazine, 0xff),
            Err(TeletextResourceIdentityError::PageByte {
                magazine,
                page: 0xff
            })
        );
    }
    assert_eq!(
        TeletextRawDataPageNumber::new(1, 0).unwrap().to_string(),
        "100"
    );
    assert_eq!(
        TeletextRawDataPageNumber::new(8, 0xab).unwrap().to_string(),
        "8AB"
    );
    assert_eq!(
        TeletextRawDataPageNumber::new(2, 0xfe).unwrap().to_string(),
        "2FE"
    );
    for magazine in [0, 9, 255] {
        assert_eq!(
            TeletextRawDataPageNumber::new(magazine, 0),
            Err(TeletextResourceIdentityError::Magazine { magazine })
        );
    }
}

#[test]
fn raw_page_json_is_closed_and_checked_without_wire_magazine_aliases() {
    for raw in [
        r#"{"magazine":0,"page":0}"#,
        r#"{"magazine":9,"page":0}"#,
        r#"{"magazine":8,"page":255}"#,
        r#"{"magazine":1,"page":256}"#,
        r#"{"magazine":-1,"page":0}"#,
        r#"{"magazine":1.5,"page":0}"#,
        r#"{"magazine":1,"page":-1}"#,
        r#"{"magazine":1,"page":"A0"}"#,
        r#"{"magazine":1,"page":null}"#,
        r#"{"magazine":1}"#,
        r#"{"page":0}"#,
        r#"{"magazine":1,"page":0,"subcode":0}"#,
        r#"{"magazine":1,"magazine":2,"page":0}"#,
        "null",
        "100",
        "[1,0]",
    ] {
        assert!(
            serde_json::from_str::<TeletextRawDataPageNumber>(raw).is_err(),
            "{raw}"
        );
    }
    let number: TeletextRawDataPageNumber =
        serde_json::from_str(r#"{"magazine":8,"page":254}"#).unwrap();
    assert_eq!(number.to_string(), "8FE");
}

#[test]
fn logical_subpage_does_not_mask_physical_header_metadata() {
    for value in 0..=15 {
        let index = TeletextResourceSubpageIndex::new(value).unwrap();
        assert_eq!(index.get(), value);
        assert_eq!(u8::from(index), value);
        assert_eq!(serde_json::to_value(index).unwrap(), json!(value));
        assert_eq!(
            serde_json::from_value::<TeletextResourceSubpageIndex>(json!(value)).unwrap(),
            index
        );
    }
    for value in 16..=255 {
        assert_eq!(
            TeletextResourceSubpageIndex::new(value),
            Err(TeletextResourceIdentityError::Subpage { subpage: value })
        );
        assert!(serde_json::from_value::<TeletextResourceSubpageIndex>(json!(value)).is_err());
    }
    for raw in ["4097", "16255", "-1", "1.5", "null", "\"0\"", "{}", "[]"] {
        assert!(serde_json::from_str::<TeletextResourceSubpageIndex>(raw).is_err());
    }
}

#[test]
fn resource_kinds_preserve_standard_names_and_derived_function_coding() {
    let fixtures = [
        (TeletextResourceKind::Pop, "pop", 3, 2),
        (TeletextResourceKind::Gpop, "gpop", 2, 2),
        (TeletextResourceKind::Drcs, "drcs", 5, 0),
        (TeletextResourceKind::Gdrcs, "gdrcs", 4, 0),
        (TeletextResourceKind::Mot, "mot", 6, 3),
        (TeletextResourceKind::Mip, "mip", 7, 3),
    ];
    for (kind, name, function, coding) in fixtures {
        assert_eq!(kind.function_code(), function);
        assert_eq!(kind.coding_code(), coding);
        assert_eq!(serde_json::to_value(kind).unwrap(), json!(name));
        assert_eq!(
            serde_json::from_value::<TeletextResourceKind>(json!(name)).unwrap(),
            kind
        );
    }
    for raw in [
        "\"POP\"",
        "\"object\"",
        "\"btt\"",
        "\"aci\"",
        "0",
        "null",
        "{}",
        r#"{"pop":null}"#,
    ] {
        assert!(serde_json::from_str::<TeletextResourceKind>(raw).is_err());
    }
}

#[test]
fn complete_kind_address_matrix_enforces_only_exact_reserved_purposes() {
    let zero = TeletextResourceSubpageIndex::new(0).unwrap();
    for magazine in 1..=8 {
        for page in 0..=0xfe {
            let number = TeletextRawDataPageNumber::new(magazine, page).unwrap();
            for kind in KINDS {
                let expected = match kind {
                    TeletextResourceKind::Mot => page == 0xfe,
                    TeletextResourceKind::Mip => page == 0xfd,
                    _ => {
                        page != 0xfd
                            && page != 0xfe
                            && !(magazine == 1 && (page == 0xbe || page == 0xf0))
                    }
                };
                assert_eq!(
                    TeletextResourceIdentity::new(kind, number, zero).is_ok(),
                    expected,
                    "{kind:?} {number}"
                );
            }
        }
    }
}

#[test]
fn mip_requires_zero_while_mot_accepts_recommended_and_nonzero_subpages() {
    for magazine in 1..=8 {
        let mot_page = TeletextRawDataPageNumber::new(magazine, 0xfe).unwrap();
        let mip_page = TeletextRawDataPageNumber::new(magazine, 0xfd).unwrap();
        for value in 0..=15 {
            let subpage = TeletextResourceSubpageIndex::new(value).unwrap();
            let mot = TeletextResourceIdentity::new(TeletextResourceKind::Mot, mot_page, subpage)
                .unwrap();
            assert_eq!(mot.logical_subpage(), subpage);
            assert_eq!(
                serde_json::from_value::<TeletextResourceIdentity>(json!({
                    "kind": "mot", "page": {"magazine": magazine, "page": 254},
                    "logical_subpage": value
                }))
                .unwrap(),
                mot
            );
            let mip = TeletextResourceIdentity::new(TeletextResourceKind::Mip, mip_page, subpage);
            if value == 0 {
                assert!(mip.is_ok());
            } else {
                assert_eq!(
                    mip,
                    Err(TeletextResourceIdentityError::MipSubpage {
                        page: mip_page,
                        subpage: value
                    })
                );
            }
        }
    }
}

#[test]
fn decimal_looking_resources_and_other_magazines_reserve_exceptions_are_accepted() {
    for kind in [
        TeletextResourceKind::Pop,
        TeletextResourceKind::Gpop,
        TeletextResourceKind::Drcs,
        TeletextResourceKind::Gdrcs,
    ] {
        for (magazine, page) in [(1, 0), (2, 0x20), (8, 0x99), (2, 0xbe), (2, 0xf0)] {
            for value in 0..=15 {
                let number = TeletextRawDataPageNumber::new(magazine, page).unwrap();
                let subpage = TeletextResourceSubpageIndex::new(value).unwrap();
                let identity = TeletextResourceIdentity::new(kind, number, subpage).unwrap();
                assert_eq!(identity.kind(), kind);
                assert_eq!(identity.page(), number);
                assert_eq!(identity.logical_subpage(), subpage);
                let encoded = serde_json::to_value(identity).unwrap();
                assert_eq!(encoded["page"], json!({"magazine": magazine, "page": page}));
                assert_eq!(
                    serde_json::from_value::<TeletextResourceIdentity>(encoded).unwrap(),
                    identity
                );
            }
        }
    }
}

#[test]
fn identity_json_rechecks_nested_addresses_purpose_and_subpage_constraints() {
    for raw in [
        r#"{"kind":"pop","page":{"magazine":1,"page":190},"logical_subpage":0}"#,
        r#"{"kind":"drcs","page":{"magazine":1,"page":240},"logical_subpage":0}"#,
        r#"{"kind":"gpop","page":{"magazine":2,"page":253},"logical_subpage":0}"#,
        r#"{"kind":"gdrcs","page":{"magazine":2,"page":254},"logical_subpage":0}"#,
        r#"{"kind":"mot","page":{"magazine":2,"page":253},"logical_subpage":0}"#,
        r#"{"kind":"mip","page":{"magazine":2,"page":254},"logical_subpage":0}"#,
        r#"{"kind":"mip","page":{"magazine":2,"page":253},"logical_subpage":1}"#,
        r#"{"kind":"pop","page":{"magazine":0,"page":160},"logical_subpage":0}"#,
        r#"{"kind":"pop","page":{"magazine":1,"page":255},"logical_subpage":0}"#,
        r#"{"kind":"pop","page":{"magazine":1,"page":160},"logical_subpage":16}"#,
        r#"{"kind":"pop","page":{"magazine":1,"page":160},"logical_subpage":4097}"#,
        r#"{"kind":"pop","page":{"magazine":1,"page":160}}"#,
        r#"{"kind":"pop","logical_subpage":0}"#,
        r#"{"page":{"magazine":1,"page":160},"logical_subpage":0}"#,
        r#"{"kind":"pop","page":{"magazine":1,"page":160,"subcode":0},"logical_subpage":0}"#,
        r#"{"kind":"pop","page":{"magazine":1,"page":160},"logical_subpage":0,"function_code":2}"#,
        r#"{"kind":"pop","page":{"magazine":1,"page":160},"logical_subpage":null}"#,
        r#"{"kind":"pop","page":[1,160],"logical_subpage":0}"#,
        r#"["pop",{"magazine":1,"page":160},0]"#,
        r#"{"kind":"pop","kind":"drcs","page":{"magazine":1,"page":160},"logical_subpage":0}"#,
        "null",
        "[]",
    ] {
        assert!(
            serde_json::from_str::<TeletextResourceIdentity>(raw).is_err(),
            "{raw}"
        );
    }
}

#[test]
fn resource_errors_identify_the_exact_purpose_and_address() {
    let zero = TeletextResourceSubpageIndex::new(0).unwrap();
    let reserved = TeletextRawDataPageNumber::new(1, 0xbe).unwrap();
    assert_eq!(
        TeletextResourceIdentity::new(TeletextResourceKind::Pop, reserved, zero)
            .unwrap_err()
            .to_string(),
        "teletext resource Pop cannot use reserved-purpose page 1BE"
    );
    assert_eq!(
        TeletextResourceIdentity::new(TeletextResourceKind::Mot, reserved, zero),
        Err(TeletextResourceIdentityError::RequiredPage {
            kind: TeletextResourceKind::Mot,
            page: reserved,
            required_page: 0xfe
        })
    );
    let mip = TeletextRawDataPageNumber::new(8, 0xfd).unwrap();
    assert_eq!(
        TeletextResourceIdentity::new(
            TeletextResourceKind::Mip,
            mip,
            TeletextResourceSubpageIndex::new(15).unwrap()
        )
        .unwrap_err()
        .to_string(),
        "teletext MIP resource 8FD requires logical subpage zero, received 15"
    );
}
