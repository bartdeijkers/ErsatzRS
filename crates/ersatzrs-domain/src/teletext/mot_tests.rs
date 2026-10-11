//! Synthetic Level 2.5 declarations, not MOT selection, physical allocation or readiness proof.

use super::*;
use serde_json::{json, Value};

fn target(magazine: u8, byte: u8) -> TeletextX27LinkTarget {
    TeletextX27LinkTarget::Page {
        page: TeletextRawDataPageNumber::new(magazine, byte).unwrap(),
    }
}
fn unused(magazine: u8) -> TeletextX27LinkTarget {
    TeletextX27LinkTarget::Unspecified {
        magazine: TeletextX27Magazine::new(magazine).unwrap(),
    }
}
fn resource(
    kind: TeletextResourceKind,
    magazine: u8,
    byte: u8,
    s1: u8,
) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        kind,
        TeletextRawDataPageNumber::new(magazine, byte).unwrap(),
        TeletextResourceSubpageIndex::new(s1).unwrap(),
    )
    .unwrap()
}
fn object(
    kind: TeletextResourceKind,
    magazine: u8,
    byte: u8,
    s1: u8,
    id: u16,
) -> TeletextObjectIdentity {
    let resource = resource(kind, magazine, byte, s1);
    match kind {
        TeletextResourceKind::Gpop => {
            TeletextObjectIdentity::global(resource, TeletextObjectId::new(id)).unwrap()
        }
        TeletextResourceKind::Pop => {
            TeletextObjectIdentity::public(resource, TeletextObjectId::new(id)).unwrap()
        }
        _ => panic!("object fixture requires object resource"),
    }
}
fn no_object(raw_number: u8) -> TeletextMotDefaultObject {
    TeletextMotDefaultObject::NoObject { raw_number }
}
fn active(
    target: TeletextObjectIdentity,
    object_type: TeletextObjectType,
) -> TeletextMotDefaultObject {
    TeletextMotDefaultObject::Object {
        target,
        object_type,
    }
}
fn object_link(
    target: TeletextX27LinkTarget,
    maximum: u8,
    supersedable: bool,
    raw_fallback: u8,
    defaults: [TeletextMotDefaultObject; 2],
) -> TeletextMotObjectLink {
    TeletextMotObjectLink::new(
        target,
        TeletextMotSubpageExtent::new(maximum).unwrap(),
        supersedable,
        TeletextMotFallback::new(raw_fallback).unwrap(),
        defaults,
    )
}
fn drcs_link(
    target: TeletextX27LinkTarget,
    maximum: u8,
    supersedable: bool,
) -> TeletextMotDrcsLink {
    TeletextMotDrcsLink::new(
        target,
        TeletextMotSubpageExtent::new(maximum).unwrap(),
        supersedable,
    )
}

#[derive(Clone, Copy)]
struct BankFixture {
    global_object: TeletextMotObjectLink,
    public_objects: [TeletextMotObjectLink; 7],
    global_drcs: TeletextMotDrcsLink,
    normal_drcs: [TeletextMotDrcsLink; 7],
    counts: TeletextMotEnhancementCounts,
}
impl BankFixture {
    fn bank(self) -> Result<TeletextMotLevel25Links, TeletextMotError> {
        TeletextMotLevel25Links::new(
            self.global_object,
            self.public_objects,
            self.global_drcs,
            self.normal_drcs,
            self.counts,
        )
    }
}
fn fixture() -> BankFixture {
    let mut public_objects = std::array::from_fn(|index| {
        object_link(
            unused(2),
            u8::try_from(index).unwrap(),
            index % 2 != 0,
            u8::try_from(index).unwrap(),
            [no_object(u8::try_from(index).unwrap()), no_object(255)],
        )
    });
    public_objects[2] = object_link(
        target(2, 0xB0),
        4,
        true,
        15,
        [
            active(
                object(TeletextResourceKind::Pop, 2, 0xB0, 0, 0),
                TeletextObjectType::Active,
            ),
            active(
                object(TeletextResourceKind::Pop, 2, 0xB0, 4, u16::MAX),
                TeletextObjectType::Passive,
            ),
        ],
    );
    public_objects[6] = object_link(
        unused(8),
        15,
        true,
        14,
        [
            TeletextMotDefaultObject::Dormant {
                object_type: TeletextObjectType::Adaptive,
                raw_number: 255,
            },
            no_object(173),
        ],
    );
    BankFixture {
        global_object: object_link(
            target(8, 0xA0),
            3,
            false,
            13,
            [
                active(
                    object(TeletextResourceKind::Gpop, 8, 0xA0, 3, 60000),
                    TeletextObjectType::Adaptive,
                ),
                no_object(99),
            ],
        ),
        public_objects,
        global_drcs: drcs_link(target(7, 0xA1), 15, true),
        normal_drcs: std::array::from_fn(|index| {
            drcs_link(
                target(3, 0xC0 + u8::try_from(index).unwrap()),
                u8::try_from(index).unwrap(),
                index % 2 == 0,
            )
        }),
        counts: TeletextMotEnhancementCounts::new(0x81, 255, 0),
    }
}
fn association(page: u8) -> TeletextMotPageAssociation {
    TeletextMotPageAssociation::new(
        page,
        TeletextMotAssociationSelection::new(
            true,
            Some(TeletextMotPublicLinkIndex::new(3).unwrap()),
        ),
        TeletextMotAssociationSelection::new(false, None),
    )
}
fn declaration() -> TeletextMotLevel25Declaration {
    TeletextMotLevel25Declaration::new(
        resource(TeletextResourceKind::Mot, 4, 0xfe, 9),
        vec![association(0xAB), association(0xff)],
        fixture().bank().unwrap(),
    )
    .unwrap()
}

#[test]
fn every_extent_is_contiguous_from_zero_without_aliasing_an_arbitrary_mask() {
    for maximum in 0_u8..=255 {
        let constructed = TeletextMotSubpageExtent::new(maximum);
        let parsed = serde_json::from_value::<TeletextMotSubpageExtent>(json!(maximum));
        if maximum < 16 {
            let extent = constructed.unwrap();
            assert_eq!(extent.maximum().get(), maximum);
            assert_eq!(parsed.unwrap(), extent);
            assert_eq!(serde_json::to_value(extent).unwrap(), json!(maximum));
            let expected = u16::try_from((1_u32 << (maximum + 1)) - 1).unwrap();
            assert_eq!(extent.required_mask().get(), expected);
            for subpage in TeletextResourceSubpageIndex::ALL {
                assert_eq!(extent.requires(subpage), subpage.get() <= maximum);
                assert_eq!(
                    extent.required_mask().requires(subpage),
                    extent.requires(subpage)
                );
            }
        } else {
            assert_eq!(
                constructed,
                Err(TeletextResourceIdentityError::Subpage { subpage: maximum })
            );
            assert!(parsed.is_err());
        }
    }
    for malformed in [
        json!(-1),
        json!(256),
        json!(1.5),
        json!("1"),
        json!(null),
        json!([0]),
    ] {
        assert!(serde_json::from_value::<TeletextMotSubpageExtent>(malformed).is_err());
    }
}

#[test]
fn selectors_preserve_all_association_nibbles_and_require_explicit_null_zero() {
    for index in 0_u8..=255 {
        let constructed = TeletextMotPublicLinkIndex::new(index);
        let parsed = serde_json::from_value::<TeletextMotPublicLinkIndex>(json!(index));
        if (1..=7).contains(&index) {
            assert_eq!(parsed.unwrap(), constructed.unwrap());
        } else {
            assert_eq!(constructed, Err(TeletextMotError::PublicIndex { index }));
            assert!(parsed.is_err());
        }
    }
    assert_eq!(
        TeletextMotPublicLinkIndex::ALL.map(TeletextMotPublicLinkIndex::get),
        [1, 2, 3, 4, 5, 6, 7]
    );
    for global in [false, true] {
        for index in 0_u8..8 {
            let public = if index == 0 {
                None
            } else {
                Some(TeletextMotPublicLinkIndex::new(index).unwrap())
            };
            let selection = TeletextMotAssociationSelection::new(global, public);
            assert_eq!(selection.global(), global);
            assert_eq!(selection.public(), public);
            let serialized = serde_json::to_value(selection).unwrap();
            assert_eq!(
                serialized,
                json!({"global":global,"public":public.map(TeletextMotPublicLinkIndex::get)})
            );
            assert_eq!(
                serde_json::from_value::<TeletextMotAssociationSelection>(serialized).unwrap(),
                selection
            );
        }
    }
    for malformed in [
        json!({"global":true}),
        json!({"public":null}),
        json!({"global":true,"public":0}),
        json!([true, null]),
    ] {
        assert!(serde_json::from_value::<TeletextMotAssociationSelection>(malformed).is_err());
    }
}

#[test]
fn fallback_preserves_all_raw_nibbles_even_when_protocol_suppresses_other_bits() {
    for flags in 0_u8..=255 {
        let constructed = TeletextMotFallback::new(flags);
        let parsed = serde_json::from_value::<TeletextMotFallback>(json!(flags));
        if flags < 16 {
            let fallback = constructed.unwrap();
            assert_eq!(parsed.unwrap(), fallback);
            assert_eq!(serde_json::to_value(fallback).unwrap(), json!(flags));
            assert_eq!(fallback.get(), flags);
            assert_eq!(fallback.protocol_no_panels(), flags & 1 != 0);
            assert_eq!(fallback.panel_code(), (flags >> 1) & 3);
            assert_eq!(fallback.black_background_substitution(), flags & 8 != 0);
        } else {
            assert_eq!(constructed, Err(TeletextMotError::Fallback { flags }));
            assert!(parsed.is_err());
        }
    }
    let bank = fixture().bank().unwrap();
    assert_eq!(bank.object_link(None).raw_fallback().get(), 13); // GPOP reserved bits retained.
    let pop3 = TeletextMotPublicLinkIndex::new(3).unwrap();
    assert_eq!(bank.public_fallback(pop3).get(), 15);
    assert_eq!(bank.public_fallback(pop3).panel_code(), 3); // Stored even with protocol=1.
}

#[test]
fn raw_coverage_counts_preserve_all_magazine_bits_and_full_byte_totals() {
    for mask in 0_u8..=255 {
        let counts = TeletextMotEnhancementCounts::new(mask, 255, 0);
        assert_eq!(counts.magazine_mask(), mask);
        assert_eq!(counts.object_pages(), 255);
        assert_eq!(counts.drcs_pages(), 0);
        for magazine in 1_u8..=8 {
            assert_eq!(
                counts.includes_magazine(TeletextX27Magazine::new(magazine).unwrap()),
                (mask >> (magazine & 7)) & 1 != 0
            );
        }
        assert_eq!(
            serde_json::from_value::<TeletextMotEnhancementCounts>(
                serde_json::to_value(counts).unwrap()
            )
            .unwrap(),
            counts
        );
    }
    assert_eq!(
        TeletextMotEnhancementCounts::new(1, 0, 255).drcs_pages(),
        255
    );
}

#[test]
fn selected_mot_retains_nonzero_s1_and_all256_raw_associations_in_original_order() {
    let identity = resource(TeletextResourceKind::Mot, 4, 0xfe, 15);
    let associations: Vec<_> = (0_u8..=255).rev().map(association).collect();
    let declaration = TeletextMotLevel25Declaration::new(
        identity,
        associations.clone(),
        fixture().bank().unwrap(),
    )
    .unwrap();
    assert_eq!(declaration.identity(), identity);
    assert_eq!(declaration.associations(), associations);
    assert_eq!(declaration.association(0xff), None);
    for byte in 0_u8..=0xfe {
        assert_eq!(declaration.association(byte), Some(association(byte)));
    }
    assert_eq!(declaration.associations()[0].page(), 0xff);
    assert_eq!(
        serde_json::from_value::<TeletextMotLevel25Declaration>(
            serde_json::to_value(&declaration).unwrap()
        )
        .unwrap(),
        declaration
    );
}

#[test]
fn association_count_duplicates_and_wrong_selected_resource_are_located() {
    let identity = resource(TeletextResourceKind::Mot, 4, 0xfe, 3);
    let bank = fixture().bank().unwrap();
    let repeated = vec![association(0xAB); 2];
    assert_eq!(
        TeletextMotLevel25Declaration::new(identity, repeated, bank),
        Err(TeletextMotError::DuplicateAssociation { page: 0xAB })
    );
    let over = vec![association(0); 257];
    assert_eq!(
        TeletextMotLevel25Declaration::new(identity, over, bank),
        Err(TeletextMotError::AssociationCount { count: 257 })
    );
    let wrong = resource(TeletextResourceKind::Mip, 4, 0xfd, 0);
    assert_eq!(
        TeletextMotLevel25Declaration::new(wrong, vec![], bank),
        Err(TeletextMotError::IdentityKind { identity: wrong })
    );
}

#[test]
fn ordered_defaults_preserve_semantic_ids_types_and_exact_original_resource_subpages() {
    let bank = fixture().bank().unwrap();
    assert_eq!(
        bank.object_link(None).defaults(),
        [
            active(
                object(TeletextResourceKind::Gpop, 8, 0xA0, 3, 60000),
                TeletextObjectType::Adaptive
            ),
            no_object(99)
        ]
    );
    let pop3 = TeletextMotPublicLinkIndex::new(3).unwrap();
    assert_eq!(
        bank.object_link(Some(pop3)).defaults(),
        [
            active(
                object(TeletextResourceKind::Pop, 2, 0xB0, 0, 0),
                TeletextObjectType::Active
            ),
            active(
                object(TeletextResourceKind::Pop, 2, 0xB0, 4, u16::MAX),
                TeletextObjectType::Passive
            )
        ]
    );
    assert!(bank.object_link(Some(pop3)).supersedable());
    assert!(!bank.object_link(None).supersedable());
    for index in TeletextMotPublicLinkIndex::ALL {
        assert_eq!(
            bank.drcs_link(Some(index)).target().page_byte(),
            0xBF + index.get()
        );
        assert_eq!(
            bank.drcs_link(Some(index)).extent().maximum().get(),
            index.get() - 1
        );
    }
    assert_eq!(bank.drcs_link(None).extent().maximum().get(), 15);
}

#[test]
fn active_default_owner_scope_magazine_page_and_extent_are_checked_per_slot() {
    let index = TeletextMotPublicLinkIndex::new(3).unwrap();
    let role = TeletextMotLinkRole::Pop { index };
    let owner = TeletextRawDataPageNumber::new(2, 0xB0).unwrap();
    let local = TeletextObjectIdentity::local(
        TeletextPageAddress {
            page: TeletextPageNumber::new(200).unwrap(),
            subcode: TeletextSubcode::new(0).unwrap(),
        },
        TeletextObjectId::new(1),
    );
    for wrong in [
        local,
        object(TeletextResourceKind::Gpop, 2, 0xB0, 0, 1),
        object(TeletextResourceKind::Pop, 3, 0xB0, 0, 1),
        object(TeletextResourceKind::Pop, 2, 0xB1, 0, 1),
    ] {
        let mut fixture = fixture();
        fixture.public_objects[2] = object_link(
            target(2, 0xB0),
            4,
            true,
            0,
            [no_object(0), active(wrong, TeletextObjectType::Passive)],
        );
        assert_eq!(
            fixture.bank(),
            Err(TeletextMotError::DefaultOwner {
                role,
                slot: 1,
                target: wrong,
                page: owner
            })
        );
    }
    let beyond = object(TeletextResourceKind::Pop, 2, 0xB0, 5, 1);
    let mut fixture = fixture();
    fixture.public_objects[2] = object_link(
        target(2, 0xB0),
        4,
        true,
        0,
        [active(beyond, TeletextObjectType::Active), no_object(0)],
    );
    assert_eq!(
        fixture.bank(),
        Err(TeletextMotError::DefaultExtent {
            role,
            slot: 0,
            target: beyond,
            maximum: TeletextResourceSubpageIndex::new(4).unwrap()
        })
    );
}

#[test]
fn dormant_defaults_and_ignored_noobject_numbers_survive_unused_links_without_invention() {
    let mut fixture = fixture();
    for raw_number in 0_u8..=255 {
        fixture.public_objects[0] = object_link(
            unused(8),
            15,
            true,
            15,
            [
                TeletextMotDefaultObject::Dormant {
                    object_type: TeletextObjectType::Passive,
                    raw_number,
                },
                no_object(raw_number),
            ],
        );
        let bank = fixture.bank().unwrap();
        assert_eq!(
            bank.object_link(Some(TeletextMotPublicLinkIndex::new(1).unwrap()))
                .defaults(),
            fixture.public_objects[0].defaults()
        );
        assert_eq!(
            serde_json::from_value::<TeletextMotLevel25Links>(serde_json::to_value(bank).unwrap())
                .unwrap(),
            bank
        );
    }
    let role = TeletextMotLinkRole::Pop {
        index: TeletextMotPublicLinkIndex::new(1).unwrap(),
    };
    fixture.public_objects[0] = object_link(
        target(2, 0xA0),
        15,
        true,
        0,
        [
            TeletextMotDefaultObject::Dormant {
                object_type: TeletextObjectType::Active,
                raw_number: 1,
            },
            no_object(0),
        ],
    );
    assert_eq!(
        fixture.bank(),
        Err(TeletextMotError::DefaultForm {
            role,
            slot: 0,
            target_used: true
        })
    );
    fixture.public_objects[0] = object_link(
        unused(2),
        15,
        false,
        0,
        [
            no_object(0),
            active(
                object(TeletextResourceKind::Pop, 2, 0xA0, 0, 99),
                TeletextObjectType::Adaptive,
            ),
        ],
    );
    assert_eq!(
        fixture.bank(),
        Err(TeletextMotError::DefaultForm {
            role,
            slot: 1,
            target_used: false
        })
    );
}

#[test]
fn all_sixteen_fixed_resource_slots_validate_reserved_purposes_even_when_unselected() {
    for (magazine, byte) in [(2, 0xfd), (2, 0xfe), (1, 0xbe), (1, 0xf0)] {
        let raw = TeletextRawDataPageNumber::new(magazine, byte).unwrap();
        let mut fixture = fixture();
        fixture.global_object =
            object_link(target(magazine, byte), 0, false, 15, [no_object(0); 2]);
        assert_eq!(
            fixture.bank(),
            Err(TeletextMotError::LinkTarget {
                role: TeletextMotLinkRole::Gpop,
                error: TeletextResourceIdentityError::ReservedPurpose {
                    kind: TeletextResourceKind::Gpop,
                    page: raw
                }
            })
        );
        let mut fixture = self::fixture();
        fixture.global_drcs = drcs_link(target(magazine, byte), 0, false);
        assert_eq!(
            fixture.bank(),
            Err(TeletextMotError::LinkTarget {
                role: TeletextMotLinkRole::Gdrcs,
                error: TeletextResourceIdentityError::ReservedPurpose {
                    kind: TeletextResourceKind::Gdrcs,
                    page: raw
                }
            })
        );
        for index in TeletextMotPublicLinkIndex::ALL {
            let mut fixture = self::fixture();
            fixture.public_objects[usize::from(index.get() - 1)] =
                object_link(target(magazine, byte), 0, false, 0, [no_object(0); 2]);
            assert_eq!(
                fixture.bank(),
                Err(TeletextMotError::LinkTarget {
                    role: TeletextMotLinkRole::Pop { index },
                    error: TeletextResourceIdentityError::ReservedPurpose {
                        kind: TeletextResourceKind::Pop,
                        page: raw
                    }
                })
            );
            let mut fixture = self::fixture();
            fixture.normal_drcs[usize::from(index.get() - 1)] =
                drcs_link(target(magazine, byte), 0, false);
            assert_eq!(
                fixture.bank(),
                Err(TeletextMotError::LinkTarget {
                    role: TeletextMotLinkRole::Drcs { index },
                    error: TeletextResourceIdentityError::ReservedPurpose {
                        kind: TeletextResourceKind::Drcs,
                        page: raw
                    }
                })
            );
        }
    }
}

#[test]
fn map_only_nested_schemas_require_fields_and_null_selectors_without_defaults() {
    let original = declaration();
    let wire = serde_json::to_value(&original).unwrap();
    assert_eq!(wire["identity"]["logical_subpage"], json!(9));
    assert_eq!(wire["links"]["global_object"]["raw_fallback"], json!(13));
    assert_eq!(
        serde_json::from_value::<TeletextMotLevel25Declaration>(wire.clone()).unwrap(),
        original
    );
    for path in [
        "",
        "/links",
        "/links/global_object",
        "/links/global_drcs",
        "/links/counts",
        "/associations/0",
        "/associations/0/objects",
        "/links/global_object/defaults/0",
    ] {
        let node = wire.pointer(path).unwrap();
        let names: Vec<_> = node.as_object().unwrap().keys().cloned().collect();
        for name in names {
            let mut missing = wire.clone();
            missing
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(&name);
            assert!(
                serde_json::from_value::<TeletextMotLevel25Declaration>(missing).is_err(),
                "missing {path}/{name}"
            );
        }
        let mut unknown = wire.clone();
        unknown
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unexpected".to_owned(), json!(0));
        assert!(serde_json::from_value::<TeletextMotLevel25Declaration>(unknown).is_err());
        let mut positional = wire.clone();
        let fields: Vec<_> = positional
            .pointer(path)
            .unwrap()
            .as_object()
            .unwrap()
            .values()
            .cloned()
            .collect();
        *positional.pointer_mut(path).unwrap() = Value::Array(fields);
        assert!(
            serde_json::from_value::<TeletextMotLevel25Declaration>(positional).is_err(),
            "array {path}"
        );
    }
    let mut explicit = wire.clone();
    explicit["associations"][0]["objects"]["public"] = Value::Null;
    assert!(serde_json::from_value::<TeletextMotLevel25Declaration>(explicit).is_ok());
}

#[test]
fn nested_json_cannot_bypass_bounds_ownership_slot_forms_or_fixed_array_counts() {
    let wire = serde_json::to_value(declaration()).unwrap();
    for (path, malformed) in [
        ("/identity/kind", json!("mip")),
        ("/identity/logical_subpage", json!(16)),
        ("/associations/0/objects/public", json!(0)),
        ("/associations/0/drcs/public", json!(8)),
        ("/links/global_object/extent", json!(16)),
        ("/links/global_object/raw_fallback", json!(16)),
        (
            "/links/global_object/defaults/0/target/resource/page/page",
            json!(161),
        ),
        (
            "/links/global_object/defaults/0/target/resource/logical_subpage",
            json!(4),
        ),
        (
            "/links/global_object/defaults/0/target/object_id",
            json!(65536),
        ),
        (
            "/links/global_object/defaults/0/object_type",
            json!("unknown"),
        ),
        (
            "/links/global_object/defaults/0/object_type",
            json!({"adaptive":null}),
        ),
        ("/links/counts/object_pages", json!(256)),
        ("/links/counts/drcs_pages", json!(-1)),
    ] {
        let mut changed = wire.clone();
        *changed.pointer_mut(path).unwrap() = malformed;
        assert!(
            serde_json::from_value::<TeletextMotLevel25Declaration>(changed).is_err(),
            "{path}"
        );
    }
    for path in [
        "/links/public_objects",
        "/links/normal_drcs",
        "/links/global_object/defaults",
    ] {
        let mut short = wire.clone();
        short
            .pointer_mut(path)
            .unwrap()
            .as_array_mut()
            .unwrap()
            .pop();
        assert!(serde_json::from_value::<TeletextMotLevel25Declaration>(short).is_err());
        let mut long = wire.clone();
        let fields = long.pointer_mut(path).unwrap().as_array_mut().unwrap();
        fields.push(fields[0].clone());
        assert!(serde_json::from_value::<TeletextMotLevel25Declaration>(long).is_err());
    }
    let mut dormant = wire.clone();
    dormant["links"]["global_object"]["defaults"][0] =
        json!({"kind":"dormant","object_type":"active","raw_number":255});
    assert!(serde_json::from_value::<TeletextMotLevel25Declaration>(dormant).is_err());
    let mut duplicate = wire.clone();
    duplicate["associations"] = json!([wire["associations"][0], wire["associations"][0]]);
    assert!(serde_json::from_value::<TeletextMotLevel25Declaration>(duplicate).is_err());
    for malformed in [
        r#"{"global":true,"global":false,"public":null}"#,
        r#"{"global":false,"public":null,"public":null}"#,
    ] {
        assert!(serde_json::from_str::<TeletextMotAssociationSelection>(malformed).is_err());
    }
    for malformed in [
        r#"{"kind":"no_object","kind":"no_object","raw_number":0}"#,
        r#"{"kind":"no_object","raw_number":0,"raw_number":1}"#,
    ] {
        assert!(serde_json::from_str::<TeletextMotDefaultObject>(malformed).is_err());
    }
}
