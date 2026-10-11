//! Authored metadata and arithmetic, without body/state/encoder acceptance claims.

use super::*;
use serde_json::{json, Value};

fn identity(kind: TeletextResourceKind, s1: u8) -> TeletextResourceIdentity {
    let byte = match kind {
        TeletextResourceKind::Mot => 0xfe,
        TeletextResourceKind::Mip => 0xfd,
        _ => 0xa0,
    };
    TeletextResourceIdentity::new(
        kind,
        TeletextRawDataPageNumber::new(8, byte).unwrap(),
        TeletextResourceSubpageIndex::new(s1).unwrap(),
    )
    .unwrap()
}
fn metadata(
    kind: TeletextResourceKind,
    s1: u8,
    last: u8,
    counter: u8,
    transmission: TeletextResourceTransmission,
) -> TeletextResourceHeaderMetadata {
    TeletextResourceHeaderMetadata::new(
        identity(kind, s1),
        TeletextResourceLastPacket::new(last).unwrap(),
        TeletextResourceUpdateCounter::new(counter).unwrap(),
        transmission,
    )
    .unwrap()
}
const KINDS: [TeletextResourceKind; 6] = [
    TeletextResourceKind::Pop,
    TeletextResourceKind::Gpop,
    TeletextResourceKind::Drcs,
    TeletextResourceKind::Gdrcs,
    TeletextResourceKind::Mot,
    TeletextResourceKind::Mip,
];
const TRANSMISSIONS: [(TeletextResourceTransmission, &str, [bool; 4]); 6] = [
    (
        TeletextResourceTransmission::FullUnchangedFirst,
        "full_unchanged_first",
        [false, false, false, false],
    ),
    (
        TeletextResourceTransmission::FullUnchangedContinuation,
        "full_unchanged_continuation",
        [false, false, true, false],
    ),
    (
        TeletextResourceTransmission::FullChangedFirst,
        "full_changed_first",
        [true, true, false, false],
    ),
    (
        TeletextResourceTransmission::FullChangedContinuation,
        "full_changed_continuation",
        [false, true, true, false],
    ),
    (
        TeletextResourceTransmission::PartialFirst,
        "partial_first",
        [false, true, false, true],
    ),
    (
        TeletextResourceTransmission::PartialContinuation,
        "partial_continuation",
        [false, true, true, true],
    ),
];

#[test]
fn last_packet_and_counter_constructors_and_serde_check_every_byte() {
    for value in 0_u8..=255 {
        let last = TeletextResourceLastPacket::new(value);
        let last_json = serde_json::from_value::<TeletextResourceLastPacket>(json!(value));
        if (1..=41).contains(&value) {
            assert_eq!(last.as_ref().unwrap().get(), value);
            assert_eq!(last_json.unwrap(), last.unwrap());
            assert_eq!(serde_json::to_value(last.unwrap()).unwrap(), json!(value));
        } else {
            assert_eq!(
                last,
                Err(TeletextResourceHeaderError::LastPacket { last: value })
            );
            assert!(last_json.is_err());
        }
        let counter = TeletextResourceUpdateCounter::new(value);
        let counter_json = serde_json::from_value::<TeletextResourceUpdateCounter>(json!(value));
        if value <= 7 {
            assert_eq!(counter.as_ref().unwrap().get(), value);
            assert_eq!(counter_json.unwrap(), counter.unwrap());
            assert_eq!(
                serde_json::to_value(counter.unwrap()).unwrap(),
                json!(value)
            );
        } else {
            assert_eq!(
                counter,
                Err(TeletextResourceHeaderError::UpdateCounter { counter: value })
            );
            assert!(counter_json.is_err());
        }
    }
}

#[test]
fn malformed_scalars_are_rejected_without_coercion_or_wrapping() {
    for value in [
        json!(-1),
        json!(256),
        json!(1.0),
        json!(1.5),
        json!("1"),
        json!(null),
        json!(true),
        json!([1]),
        json!({"value": 1}),
    ] {
        assert!(
            serde_json::from_value::<TeletextResourceLastPacket>(value.clone()).is_err(),
            "{value}"
        );
        assert!(
            serde_json::from_value::<TeletextResourceUpdateCounter>(value.clone()).is_err(),
            "{value}"
        );
    }
}

#[test]
fn explicit_counter_increment_wraps_modulo_eight_without_mutating_the_original() {
    for value in 0_u8..8 {
        let counter = TeletextResourceUpdateCounter::new(value).unwrap();
        assert_eq!(
            counter.next_modulo_eight().get(),
            if value == 7 { 0 } else { value + 1 }
        );
        let mut next = counter;
        for _ in 0..8 {
            next = next.next_modulo_eight();
        }
        assert_eq!(next, counter);
        assert_eq!(counter.get(), value);
    }
}

#[test]
fn six_lifecycle_strings_have_exact_independent_table_b1_control_tuples() {
    for (transmission, name, tuple) in TRANSMISSIONS {
        assert_eq!(
            [
                transmission.erase_page(),
                transmission.update_indicator(),
                transmission.continuation(),
                transmission.partial()
            ],
            tuple
        );
        assert_eq!(serde_json::to_value(transmission).unwrap(), json!(name));
        assert_eq!(
            serde_json::from_value::<TeletextResourceTransmission>(json!(name)).unwrap(),
            transmission
        );
    }
    for value in [
        json!("full"),
        json!("FULL_CHANGED_FIRST"),
        json!(""),
        json!(null),
        json!(true),
        json!(0),
        json!([]),
        json!(["partial_first"]),
        json!({"partial_first": null}),
    ] {
        assert!(
            serde_json::from_value::<TeletextResourceTransmission>(value.clone()).is_err(),
            "{value}"
        );
    }
}

#[test]
fn kind_specific_last_bounds_are_checked_in_constructor_and_nested_json() {
    for kind in KINDS {
        let resource = identity(
            kind,
            if kind == TeletextResourceKind::Mip {
                0
            } else {
                15
            },
        );
        let maximum = if matches!(kind, TeletextResourceKind::Pop | TeletextResourceKind::Gpop) {
            41
        } else {
            24
        };
        for last in 1_u8..=41 {
            let result = TeletextResourceHeaderMetadata::new(
                resource,
                TeletextResourceLastPacket::new(last).unwrap(),
                TeletextResourceUpdateCounter::new(7).unwrap(),
                TeletextResourceTransmission::FullChangedFirst,
            );
            let value = json!({"identity": resource, "last_packet": last, "update_counter": 7, "transmission": "full_changed_first"});
            let decoded = serde_json::from_value::<TeletextResourceHeaderMetadata>(value);
            if last <= maximum {
                let expected = result.unwrap();
                assert_eq!(decoded.unwrap(), expected);
                assert_eq!(expected.identity(), resource);
                assert_eq!(expected.last_packet().get(), last);
                assert_eq!(expected.update_counter().get(), 7);
                assert_eq!(
                    expected.transmission(),
                    TeletextResourceTransmission::FullChangedFirst
                );
            } else {
                assert_eq!(
                    result,
                    Err(TeletextResourceHeaderError::KindLastPacket {
                        identity: resource,
                        last,
                        maximum
                    })
                );
                assert!(decoded.is_err());
            }
        }
    }
}

#[test]
fn physical_nibble_geometry_is_exact_for_every_object_last_counter_and_logical_subpage() {
    for kind in [TeletextResourceKind::Pop, TeletextResourceKind::Gpop] {
        for last in 1_u8..=41 {
            for counter in 0_u8..8 {
                for s1 in 0_u8..16 {
                    let header = metadata(
                        kind,
                        s1,
                        last,
                        counter,
                        TeletextResourceTransmission::FullUnchangedFirst,
                    );
                    let subcode = header.physical_subcode();
                    assert_eq!((subcode >> 12) & 3, u16::from(last >> 4));
                    assert_eq!((subcode >> 8) & 15, u16::from(last & 15));
                    assert_eq!((subcode >> 4) & 7, u16::from(counter));
                    assert_eq!(subcode & 15, u16::from(s1));
                    assert_eq!(
                        subcode & 0xc080,
                        0,
                        "reserved header-control locations remain separate"
                    );
                    assert_eq!(TeletextSubcode::new(subcode).unwrap().get(), subcode);
                    assert_eq!(header.identity().logical_subpage().get(), s1);
                }
            }
        }
    }
    assert_eq!(
        metadata(
            TeletextResourceKind::Pop,
            15,
            25,
            7,
            TeletextResourceTransmission::FullChangedFirst
        )
        .physical_subcode(),
        0x197f
    );
    assert_eq!(
        metadata(
            TeletextResourceKind::Pop,
            15,
            26,
            7,
            TeletextResourceTransmission::FullChangedFirst
        )
        .physical_subcode(),
        0x1a7f
    );
    assert_eq!(
        metadata(
            TeletextResourceKind::Gpop,
            15,
            41,
            7,
            TeletextResourceTransmission::FullChangedFirst
        )
        .physical_subcode(),
        0x297f
    );
    assert_eq!(
        metadata(
            TeletextResourceKind::Drcs,
            15,
            24,
            0,
            TeletextResourceTransmission::FullChangedFirst
        )
        .physical_subcode(),
        0x180f
    );
}

#[test]
fn lifecycle_does_not_implicitly_advance_counter_or_change_fragment_extent() {
    for (transmission, _, _) in TRANSMISSIONS {
        let header = metadata(TeletextResourceKind::Mot, 15, 21, 7, transmission);
        assert_eq!(header.physical_subcode(), 0x157f);
        assert_eq!(
            header.identity().logical_subpage().get(),
            15,
            "MOT zero is advisory, not an identity rewrite"
        );
        assert_eq!(header.last_packet().get(), 21);
        assert_eq!(header.update_counter().get(), 7);
        assert_eq!(header.transmission(), transmission);
    }
    let mip = metadata(
        TeletextResourceKind::Mip,
        0,
        24,
        7,
        TeletextResourceTransmission::FullUnchangedFirst,
    );
    assert_eq!(mip.physical_subcode(), 0x1870);
    let mut mip_json = serde_json::to_value(mip).unwrap();
    mip_json["identity"]["logical_subpage"] = json!(1);
    assert!(serde_json::from_value::<TeletextResourceHeaderMetadata>(mip_json).is_err());
}

#[test]
fn required_map_fields_and_nested_checked_identity_have_no_positional_or_unknown_bypass() {
    let header = metadata(
        TeletextResourceKind::Mot,
        9,
        21,
        3,
        TeletextResourceTransmission::PartialContinuation,
    );
    let canonical = serde_json::to_value(header).unwrap();
    assert_eq!(
        serde_json::from_value::<TeletextResourceHeaderMetadata>(canonical.clone()).unwrap(),
        header
    );
    for name in ["identity", "last_packet", "update_counter", "transmission"] {
        let mut missing = canonical.clone();
        missing.as_object_mut().unwrap().remove(name);
        assert!(
            serde_json::from_value::<TeletextResourceHeaderMetadata>(missing).is_err(),
            "missing {name}"
        );
        let mut null = canonical.clone();
        null[name] = Value::Null;
        assert!(
            serde_json::from_value::<TeletextResourceHeaderMetadata>(null).is_err(),
            "null {name}"
        );
    }
    let mut extra = canonical.clone();
    extra["c14"] = json!(true);
    assert!(serde_json::from_value::<TeletextResourceHeaderMetadata>(extra).is_err());
    let array = json!([canonical["identity"], 21, 3, "partial_continuation"]);
    assert!(serde_json::from_value::<TeletextResourceHeaderMetadata>(array).is_err());
    let mut nested = canonical.clone();
    nested["identity"] = json!(["mot", {"magazine": 8, "page": 254}, 9]);
    assert!(serde_json::from_value::<TeletextResourceHeaderMetadata>(nested).is_err());
    let mut bad_purpose = canonical.clone();
    bad_purpose["identity"]["page"]["page"] = json!(253);
    assert!(serde_json::from_value::<TeletextResourceHeaderMetadata>(bad_purpose).is_err());
    let mut bad_s1 = canonical.clone();
    bad_s1["identity"]["logical_subpage"] = json!(16);
    assert!(serde_json::from_value::<TeletextResourceHeaderMetadata>(bad_s1).is_err());
    let duplicate = format!("{{\"identity\":{},\"last_packet\":21,\"last_packet\":21,\"update_counter\":3,\"transmission\":\"partial_continuation\"}}", canonical["identity"]);
    assert!(serde_json::from_str::<TeletextResourceHeaderMetadata>(&duplicate).is_err());
}
