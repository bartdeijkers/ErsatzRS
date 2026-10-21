//! Independent complete factories retain all original and dormant raw banks.
use super::*;

pub(super) fn alternate(scope: TeletextDrcsTableScope, subpage: u8) -> TeletextResourceIdentity {
    let old = bank(scope, subpage);
    TeletextResourceIdentity::new(
        old.kind(),
        TeletextRawDataPageNumber::new(1, old.page().page() + 2).unwrap(),
        old.logical_subpage(),
    )
    .unwrap()
}
pub(super) fn mask(scope: TeletextDrcsTableScope, subpage: u8, slot: u8) -> [u16; 10] {
    match (scope, subpage, slot) {
        (TeletextDrcsTableScope::Normal, 0, 0) => [0xf00; 10],
        (TeletextDrcsTableScope::Normal, 0, 1) => [0x0f0; 10],
        (TeletextDrcsTableScope::Normal, 1, 0) => [0xf0f; 10],
        (TeletextDrcsTableScope::Normal, 1, 1) => [0x0ff; 10],
        (TeletextDrcsTableScope::Global, 0, 0) => [0x00f; 10],
        (TeletextDrcsTableScope::Global, 0, 1) => [0xa05; 10],
        (TeletextDrcsTableScope::Global, 1, 0) => [0x505; 10],
        (TeletextDrcsTableScope::Global, 1, 1) => [0xa0a; 10],
        _ => unreachable!(),
    }
}
pub(super) fn original(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = free(theme, page, subcode);
    let supersedable = &mut value["service"]["mots"][0]["links"]["normal_drcs"][0]["supersedable"];
    assert_eq!(*supersedable, false);
    *supersedable = json!(true);
    for scope in [
        TeletextDrcsTableScope::Normal,
        TeletextDrcsTableScope::Global,
    ] {
        for subpage in 0..2 {
            let identity = alternate(scope, subpage);
            let table = TeletextDrcsSubtable::new(
                identity,
                (0..2)
                    .map(|slot| TeletextDrcsGlyphDefinition {
                        slot: TeletextDrcsGlyphSlot::new(slot).unwrap(),
                        bitmap: TeletextDrcsBitmap::new(mask(scope, subpage, slot)).unwrap(),
                    })
                    .collect(),
            )
            .unwrap();
            let mut table = serde_json::to_value(table).unwrap();
            table["identity"] =
                json!({"kind":identity.kind(),"page":identity.page(),"subpage":subpage});
            value["service"]["drcs"].as_array_mut().unwrap().push(table);
        }
    }
    value
}
pub(super) fn selected(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = original(theme, page, subcode);
    let metadata = value["service"]["page_enhancements"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|metadata| metadata["address"]["page"] == 124)
        .unwrap();
    for (scope, key, role) in [
        (TeletextDrcsTableScope::Normal, "normal", "drcs"),
        (TeletextDrcsTableScope::Global, "global", "gdrcs"),
    ] {
        assert_eq!(
            metadata["resources"]["drcs"][key],
            json!(bank(scope, 0).page())
        );
        assert_eq!(
            metadata["compositional_links"][role]["target"]["page"],
            json!(bank(scope, 0).page())
        );
        assert_eq!(
            metadata["compositional_links"][role]["required_subpages"],
            3
        );
        metadata["resources"]["drcs"][key] = json!(alternate(scope, 0).page());
        metadata["compositional_links"][role]["target"]["page"] = json!(alternate(scope, 0).page());
    }
    value
}
pub(super) fn protected(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = original(theme, page, subcode);
    for page in value["authoring"]["pages"].as_array_mut().unwrap() {
        let regions = page["regions"].as_array_mut().unwrap();
        let region = regions
            .iter_mut()
            .find(|region| region["id"] == "free-right-glyph")
            .unwrap();
        assert_eq!(region["width"], 4);
        region["width"] = json!(2);
        regions.push(json!({"id":"generated-third","kind":"generated","area":"right_panel","row":1,"column":2,"width":1,"height":1}));
    }
    value
}
pub(super) fn equal_pixel_protected(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = protected(theme, page, subcode);
    let tables = value["service"]["drcs"].as_array_mut().unwrap();
    for scope in [
        TeletextDrcsTableScope::Normal,
        TeletextDrcsTableScope::Global,
    ] {
        for subpage in 0..2 {
            let table = tables
                .iter_mut()
                .find(|table| {
                    table["identity"]["page"] == json!(alternate(scope, subpage).page())
                        && table["identity"]["subpage"] == subpage
                })
                .unwrap();
            for slot in 0..2 {
                table["glyphs"][usize::from(slot)]["bitmap"] = json!(TeletextDrcsBitmap::new(
                    fixture::bank_mask(scope, subpage, slot)
                )
                .unwrap());
            }
        }
    }
    value
}
pub(super) fn encoding(
) -> ersatzrs_web::host::teletext_channel_documents::TeletextChannelDocumentEncodingPolicy {
    let mut result = super::encoding();
    for scope in [
        TeletextDrcsTableScope::Normal,
        TeletextDrcsTableScope::Global,
    ] {
        for subpage in 0..2 {
            result
                .transmissions
                .push(TeletextServiceResourceTransmission {
                    resource: alternate(scope, subpage),
                    counter: TeletextResourceUpdateCounter::new(1).unwrap(),
                    update: TeletextResourceFullUpdate::Changed,
                    last_subpage: subpage == 1,
                });
        }
    }
    result
}
