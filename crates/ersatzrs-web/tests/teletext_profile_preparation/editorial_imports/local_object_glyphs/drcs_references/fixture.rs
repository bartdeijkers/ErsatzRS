//! Fixed explicit original Normal/Global banks and required original magazine MOT.
use super::*;

// Existing drcs_companion.rs PIXELS/MIRRORED_PIXELS, not captured output.
const PIXELS: [u16; 10] = [
    0x801, 0x800, 0x400, 0x200, 0x100, 0x080, 0x040, 0x020, 0x010, 0x009,
];
const MIRRORED: [u16; 10] = [
    0x801, 0x001, 0x002, 0x004, 0x008, 0x010, 0x020, 0x040, 0x080, 0x900,
];
pub(super) const COLUMNS: [u8; 4] = [8, 9, 0, 1];
pub(super) const COMMANDS: [usize; 4] = [2, 5, 2, 3];
pub(super) const VISUAL: [usize; 6] = [12, 13, 44, 45, 46, 47];

#[derive(Clone, Copy)]
enum Case {
    Free,
    Zone,
    NarrowMain,
    NarrowRight,
    GeneratedRight,
}

pub(super) fn scope(target: usize) -> TeletextDrcsTableScope {
    if matches!(target, 0 | 2) {
        TeletextDrcsTableScope::Normal
    } else {
        TeletextDrcsTableScope::Global
    }
}

pub(super) fn bank(scope: TeletextDrcsTableScope) -> TeletextResourceIdentity {
    let (kind, page) = match scope {
        TeletextDrcsTableScope::Normal => (TeletextResourceKind::Drcs, 0xc0),
        TeletextDrcsTableScope::Global => (TeletextResourceKind::Gdrcs, 0xc1),
    };
    TeletextResourceIdentity::new(
        kind,
        TeletextRawDataPageNumber::new(1, page).unwrap(),
        TeletextResourceSubpageIndex::new(0).unwrap(),
    )
    .unwrap()
}

pub(super) fn bank_mask(scope: TeletextDrcsTableScope, slot: u8) -> [u16; 10] {
    match (scope, slot) {
        (TeletextDrcsTableScope::Normal, 0) | (TeletextDrcsTableScope::Global, 1) => PIXELS,
        (TeletextDrcsTableScope::Normal, 1) | (TeletextDrcsTableScope::Global, 0) => MIRRORED,
        _ => unreachable!(),
    }
}

fn table(scope: TeletextDrcsTableScope) -> Value {
    let identity = bank(scope);
    let table = TeletextDrcsSubtable::new(
        identity,
        (0..2)
            .map(|slot| TeletextDrcsGlyphDefinition {
                slot: TeletextDrcsGlyphSlot::new(slot).unwrap(),
                bitmap: TeletextDrcsBitmap::new(bank_mask(scope, slot)).unwrap(),
            })
            .collect(),
    )
    .unwrap();
    let mut value = serde_json::to_value(table).unwrap();
    value["identity"] = json!({"kind":identity.kind(),"page":identity.page(),"subpage":0});
    value
}

fn prepare_banks(value: &mut Value) {
    value["service"]["drcs"] = json!([
        table(TeletextDrcsTableScope::Normal),
        table(TeletextDrcsTableScope::Global)
    ]);
    // Preserve the magazine's mandatory original MOT selection and body.
    // Matching Normal X27 supplies its mask; Global X27 selects the new original bank.
    let unused = json!({"target":{"kind":"unspecified","magazine":1},"validity":"level25_only","required_subpages":65535});
    let linked = |scope| json!({"target":{"kind":"page","page":bank(scope).page()},"validity":"level25_only","required_subpages":1});
    for metadata in value["service"]["page_enhancements"]
        .as_array_mut()
        .unwrap()
    {
        metadata["resources"]["drcs"] = json!({"normal":bank(TeletextDrcsTableScope::Normal).page(),"global":bank(TeletextDrcsTableScope::Global).page()});
        metadata["compositional_links"] = json!({"pop":unused,"gpop":unused,"drcs":linked(TeletextDrcsTableScope::Normal),"gdrcs":linked(TeletextDrcsTableScope::Global)});
    }
}

pub(super) fn encoding(
) -> ersatzrs_web::host::teletext_channel_documents::TeletextChannelDocumentEncodingPolicy {
    let mut policy = drcs_companion::encoding_policy();
    assert_eq!(policy.transmissions.len(), 2);
    policy
        .transmissions
        .push(TeletextServiceResourceTransmission {
            resource: bank(TeletextDrcsTableScope::Global),
            counter: TeletextResourceUpdateCounter::new(1).unwrap(),
            update: TeletextResourceFullUpdate::Changed,
            last_subpage: true,
        });
    policy
}

pub(super) fn operation(index: u8, target: usize) -> TeletextColumnOperation {
    assert!(index <= 2);
    let slot = u8::from(!(index == 0 || (index == 2 && matches!(target, 0 | 2))));
    TeletextColumnOperation::Drcs {
        reference: TeletextDrcsGlyphReference {
            table: scope(target),
            subtable: TeletextDrcsSubtableIndex::new(0).unwrap(),
            slot: TeletextDrcsGlyphSlot::new(slot).unwrap(),
        },
    }
}

pub(super) fn word(index: u8, target: usize) -> u32 {
    let TeletextColumnOperation::Drcs { reference } = operation(index, target) else {
        unreachable!()
    };
    let scope = if reference.table == TeletextDrcsTableScope::Normal {
        0x40
    } else {
        0
    };
    u32::from(COLUMNS[target]) | (13 << 6) | ((scope | u32::from(reference.slot.get())) << 11)
}

pub(super) fn identity(page: u16) -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(
        channel_documents::address(page, 32),
        TeletextObjectId::new(65520),
    )
}

fn response(theme: &str, page: u16, subcode: u16, case: Case) -> Value {
    let permission = if matches!(case, Case::Zone) {
        Permission::Zone
    } else {
        Permission::Free
    };
    let mut value = super::super::response(theme, page, subcode, Family::G0, permission);
    prepare_banks(&mut value);
    assert_eq!(
        value["service"]["magazines"][0]["presentation"]["primary"],
        0
    );
    for object in value["service"]["objects"].as_array_mut().unwrap() {
        let commands = object["program"]["commands"].as_array_mut().unwrap();
        assert_eq!(commands.len(), 3);
        assert_eq!(
            commands[0]["operation"],
            json!({"kind":"foreground","colour":18})
        );
        assert_eq!(
            commands[1]["operation"],
            json!({"kind":"background","colour":0})
        );
        commands[2]["operation"] = serde_json::to_value(operation(0, 2)).unwrap();
        commands.push(json!({"kind":"at_column","column":1,"operation":operation(0,3)}));
    }
    for metadata in value["service"]["page_enhancements"]
        .as_array_mut()
        .unwrap()
    {
        let commands = metadata["local_program"]["commands"]
            .as_array_mut()
            .unwrap();
        assert_eq!(commands.len(), 5);
        commands[2]["operation"] = serde_json::to_value(operation(0, 0)).unwrap();
        assert_eq!(commands[3]["offset"]["column"], 32);
        commands[4]["offset"]["column"] = json!(34);
        // AtColumn advances AP: keep both calls at8 before the final column9.
        commands.push(json!({"kind":"at_column","column":9,"operation":operation(0,1)}));
    }
    for page in value["authoring"]["pages"].as_array_mut().unwrap() {
        let regions = page["regions"].as_array_mut().unwrap();
        for region in regions.iter_mut() {
            if region["id"] == "free-glyph" {
                region["width"] = json!(if matches!(case, Case::NarrowMain) {
                    1
                } else {
                    2
                });
            }
            if region["id"] == "free-right-glyph" {
                region["width"] = json!(match case {
                    Case::NarrowRight => 3,
                    Case::GeneratedRight => 2,
                    _ => 4,
                });
            }
        }
        if matches!(case, Case::GeneratedRight) {
            regions.push(json!({"id":"generated-third","kind":"generated","area":"right_panel","row":1,"column":2,"width":1,"height":1}));
        }
    }
    value
}

pub(super) fn free(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, Case::Free)
}

pub(super) fn zone(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, Case::Zone)
}

pub(super) fn narrow_main(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, Case::NarrowMain)
}

pub(super) fn narrow_right(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, Case::NarrowRight)
}

pub(super) fn generated_right(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, Case::GeneratedRight)
}
