use serde_json::json;

use super::*;

fn raw(page: u8) -> TeletextRawDataPageNumber {
    TeletextRawDataPageNumber::new(2, page).unwrap()
}

fn identity(kind: TeletextResourceKind, page: u8, subpage: u8) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        kind,
        raw(page),
        TeletextResourceSubpageIndex::new(subpage).unwrap(),
    )
    .unwrap()
}

fn glyph(slot: u8) -> TeletextDrcsGlyphDefinition {
    TeletextDrcsGlyphDefinition {
        slot: TeletextDrcsGlyphSlot::new(slot).unwrap(),
        bitmap: TeletextDrcsBitmap::new([0x801; 10]).unwrap(),
    }
}

fn table(kind: TeletextResourceKind, page: u8, subpage: u8, slots: &[u8]) -> TeletextDrcsSubtable {
    TeletextDrcsSubtable::new(
        identity(kind, page, subpage),
        slots.iter().copied().map(glyph).collect(),
    )
    .unwrap()
}

fn reference(scope: TeletextDrcsTableScope, subtable: u8, slot: u8) -> TeletextDrcsGlyphReference {
    TeletextDrcsGlyphReference {
        table: scope,
        subtable: TeletextDrcsSubtableIndex::new(subtable).unwrap(),
        slot: TeletextDrcsGlyphSlot::new(slot).unwrap(),
    }
}

fn display(page: u16) -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(page).unwrap(),
        subcode: TeletextSubcode::new(0x1234).unwrap(),
    }
}

#[test]
fn subtable_checks_purpose_count_and_duplicate_slots_without_reordering() {
    let id = identity(TeletextResourceKind::Drcs, 0xa0, 0);
    let value = TeletextDrcsSubtable::new(id, vec![glyph(47), glyph(0), glyph(23)]).unwrap();
    assert_eq!(value.identity(), id);
    assert_eq!(
        value
            .glyphs()
            .iter()
            .map(|g| g.slot.get())
            .collect::<Vec<_>>(),
        [47, 0, 23]
    );
    assert!(TeletextDrcsSubtable::new(id, Vec::new())
        .unwrap()
        .glyphs()
        .is_empty());
    assert!(TeletextDrcsSubtable::new(id, (0..48).map(glyph).collect()).is_ok());
    assert_eq!(
        TeletextDrcsSubtable::new(id, vec![glyph(0); 49]),
        Err(TeletextDrcsResourceError::GlyphCount {
            identity: id,
            count: 49
        })
    );
    assert_eq!(
        TeletextDrcsSubtable::new(id, vec![glyph(2), glyph(2)]),
        Err(TeletextDrcsResourceError::DuplicateGlyph {
            identity: id,
            slot: TeletextDrcsGlyphSlot::new(2).unwrap()
        })
    );
    let object = identity(TeletextResourceKind::Pop, 0xa0, 0);
    assert_eq!(
        TeletextDrcsSubtable::new(object, Vec::new()),
        Err(TeletextDrcsResourceError::Kind { identity: object })
    );
}

#[test]
fn catalog_count_precedes_indexing_and_kind_tags_cannot_hide_logical_collisions() {
    let first = table(TeletextResourceKind::Drcs, 0xa0, 0, &[0]);
    assert_eq!(
        TeletextDrcsCatalog::new(vec![first.clone(); MAX_TELETEXT_PAGE_RECORDS + 1]),
        Err(TeletextDrcsResourceError::ResourceCount {
            count: MAX_TELETEXT_PAGE_RECORDS + 1
        })
    );
    for kind in [TeletextResourceKind::Drcs, TeletextResourceKind::Gdrcs] {
        assert_eq!(
            TeletextDrcsCatalog::new(vec![first.clone(), table(kind, 0xa0, 0, &[1])]),
            Err(TeletextDrcsResourceError::DuplicateResource {
                page: raw(0xa0),
                subpage: TeletextResourceSubpageIndex::new(0).unwrap()
            })
        );
    }
    // No inferred whole-page-function restriction: these logical S1 values differ.
    assert!(TeletextDrcsCatalog::new(vec![
        first,
        table(TeletextResourceKind::Gdrcs, 0xa0, 1, &[0])
    ])
    .is_ok());
    assert!(TeletextDrcsCatalog::new(Vec::new()).is_ok());
}

#[test]
fn catalog_roundtrip_preserves_sparse_slots_raw_identity_and_bitmaps() {
    let catalog =
        TeletextDrcsCatalog::new(vec![table(TeletextResourceKind::Gdrcs, 0xa1, 15, &[47, 3])])
            .unwrap();
    let document = serde_json::to_value(&catalog).unwrap();
    assert_eq!(
        document["subtables"][0]["identity"]["page"],
        json!({"magazine": 2, "page": 161})
    );
    assert_eq!(
        document["subtables"][0]["identity"]["logical_subpage"],
        json!(15)
    );
    assert_eq!(
        document["subtables"][0]["glyphs"][0]["bitmap"],
        json!(vec![2049; 10])
    );
    assert_eq!(
        serde_json::from_value::<TeletextDrcsCatalog>(document).unwrap(),
        catalog
    );
    assert_eq!(catalog.subtables()[0].glyphs()[1].slot.get(), 3);
}

#[test]
fn nested_json_cannot_bypass_resource_and_glyph_checks() {
    let fixture = serde_json::to_value(
        TeletextDrcsCatalog::new(vec![table(TeletextResourceKind::Drcs, 0xa0, 0, &[0])]).unwrap(),
    )
    .unwrap();
    for (path, invalid) in [("kind", json!("pop")), ("logical_subpage", json!(16))] {
        let mut changed = fixture.clone();
        changed["subtables"][0]["identity"][path] = invalid;
        assert!(serde_json::from_value::<TeletextDrcsCatalog>(changed).is_err());
    }
    let mut changed = fixture.clone();
    changed["subtables"][0]["glyphs"][0]["slot"] = json!(48);
    assert!(serde_json::from_value::<TeletextDrcsCatalog>(changed).is_err());
    let mut changed = fixture.clone();
    changed["subtables"][0]["glyphs"][0]["bitmap"][9] = json!(4096);
    assert!(serde_json::from_value::<TeletextDrcsCatalog>(changed).is_err());
    let mut changed = fixture.clone();
    changed["subtables"][0]["glyphs"][0]["colour_planes"] = json!(2);
    assert!(serde_json::from_value::<TeletextDrcsCatalog>(changed).is_err());
    let mut changed = fixture.clone();
    changed["subtables"][0]["glyphs"] = json!([
        fixture["subtables"][0]["glyphs"][0],
        fixture["subtables"][0]["glyphs"][0]
    ]);
    assert!(serde_json::from_value::<TeletextDrcsCatalog>(changed).is_err());
    let mut changed = fixture;
    changed["ready"] = json!(true);
    assert!(serde_json::from_value::<TeletextDrcsCatalog>(changed).is_err());
}

#[test]
fn links_reject_reserved_addresses_and_scope_does_not_alias_normal_and_global() {
    for page in [0xfd, 0xfe] {
        assert!(TeletextDrcsPageLinks::new(Some(raw(page)), None).is_err());
        assert!(serde_json::from_value::<TeletextDrcsPageLinks>(
            json!({"normal": {"magazine": 2, "page": page}, "global": null})
        )
        .is_err());
    }
    let links = TeletextDrcsPageLinks::new(Some(raw(0xa0)), Some(raw(0xa1))).unwrap();
    assert_eq!(links.page(TeletextDrcsTableScope::Normal), Some(raw(0xa0)));
    assert_eq!(links.page(TeletextDrcsTableScope::Global), Some(raw(0xa1)));
    assert_eq!(
        serde_json::from_value::<TeletextDrcsPageLinks>(serde_json::to_value(links).unwrap())
            .unwrap(),
        links
    );
    let catalog =
        TeletextDrcsCatalog::new(vec![table(TeletextResourceKind::Drcs, 0xa0, 0, &[0])]).unwrap();
    assert!(matches!(
        catalog.validate_display_usage(
            display(200),
            TeletextDrcsPageLinks::new(None, Some(raw(0xa0))).unwrap(),
            &[]
        ),
        Err(TeletextDrcsResourceError::MissingPage {
            table: TeletextDrcsTableScope::Global,
            ..
        })
    ));
}

#[test]
fn display_validation_reports_exact_missing_link_subtable_and_slot() {
    let catalog =
        TeletextDrcsCatalog::new(vec![table(TeletextResourceKind::Drcs, 0xa0, 0, &[0])]).unwrap();
    let refs = [reference(TeletextDrcsTableScope::Normal, 0, 0)];
    assert_eq!(
        catalog.validate_display_usage(
            display(200),
            TeletextDrcsPageLinks::new(None, None).unwrap(),
            &refs
        ),
        Err(TeletextDrcsResourceError::MissingLink {
            display: display(200),
            reference: 0,
            table: TeletextDrcsTableScope::Normal
        })
    );
    let links = TeletextDrcsPageLinks::new(Some(raw(0xa0)), None).unwrap();
    let refs = [
        reference(TeletextDrcsTableScope::Normal, 0, 0),
        reference(TeletextDrcsTableScope::Normal, 1, 0),
    ];
    assert_eq!(
        catalog.validate_display_usage(display(200), links, &refs),
        Err(TeletextDrcsResourceError::MissingSubtable {
            display: display(200),
            reference: 1,
            identity: identity(TeletextResourceKind::Drcs, 0xa0, 1)
        })
    );
    let refs = [
        reference(TeletextDrcsTableScope::Normal, 0, 0),
        reference(TeletextDrcsTableScope::Normal, 0, 47),
    ];
    assert_eq!(
        catalog.validate_display_usage(display(200), links, &refs),
        Err(TeletextDrcsResourceError::MissingGlyph {
            display: display(200),
            reference: 1,
            identity: identity(TeletextResourceKind::Drcs, 0xa0, 0),
            slot: TeletextDrcsGlyphSlot::new(47).unwrap()
        })
    );
}

#[test]
fn display_glyph_limit_counts_distinct_global_and_normal_glyphs_not_repetitions() {
    let slots: Vec<_> = (0..48).collect();
    let catalog = TeletextDrcsCatalog::new(vec![
        table(TeletextResourceKind::Drcs, 0xa0, 0, &slots),
        table(TeletextResourceKind::Gdrcs, 0xa1, 0, &slots),
    ])
    .unwrap();
    let links = TeletextDrcsPageLinks::new(Some(raw(0xa0)), Some(raw(0xa1))).unwrap();
    let mut refs: Vec<_> = (0..12)
        .map(|slot| reference(TeletextDrcsTableScope::Normal, 0, slot))
        .chain((0..12).map(|slot| reference(TeletextDrcsTableScope::Global, 0, slot)))
        .collect();
    refs.extend(vec![refs[0]; 200]);
    assert!(catalog
        .validate_display_usage(display(200), links, &refs)
        .is_ok());
    refs.push(reference(TeletextDrcsTableScope::Global, 0, 47));
    assert_eq!(
        catalog.validate_display_usage(display(200), links, &refs),
        Err(TeletextDrcsResourceError::DisplayGlyphs {
            display: display(200),
            reference: 224
        })
    );
    assert!(catalog
        .validate_display_usage(display(201), links, &refs[..24])
        .is_ok());
}

#[test]
fn display_subtable_limit_counts_logical_indices_and_restores_between_pages() {
    let catalog = TeletextDrcsCatalog::new(
        (0..3)
            .map(|s| table(TeletextResourceKind::Drcs, 0xa0, s, &[0]))
            .collect(),
    )
    .unwrap();
    let links = TeletextDrcsPageLinks::new(Some(raw(0xa0)), None).unwrap();
    let refs: Vec<_> = (0..3)
        .map(|s| reference(TeletextDrcsTableScope::Normal, s, 0))
        .collect();
    assert!(catalog
        .validate_display_usage(display(200), links, &refs[..2])
        .is_ok());
    assert_eq!(
        catalog.validate_display_usage(display(200), links, &refs),
        Err(TeletextDrcsResourceError::DisplaySubtables {
            display: display(200),
            reference: 2
        })
    );
    assert!(catalog
        .validate_display_usage(display(201), links, &refs[1..])
        .is_ok());
}
