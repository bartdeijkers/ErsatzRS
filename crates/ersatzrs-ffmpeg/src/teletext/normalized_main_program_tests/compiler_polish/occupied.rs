//! A free logical subpage must remain usable when both raw-page links are owned.

use super::*;

#[test]
fn occupied_roles_use_a_free_linked_subpage_without_changing_authored_banks() {
    let seed = occupied_seed(true);
    let input = declared(seed.clone());
    let fallback = polish_fallback(&seed);
    let normal = seed.page_enhancements()[&owner()]
        .resources
        .drcs
        .page(TeletextDrcsTableScope::Normal)
        .unwrap();
    assert_eq!(seed.drcs().subtables().len(), 2);
    assert!(seed.drcs().subtables().iter().all(|table| {
        table.identity().logical_subpage().get() == 0
            && table
                .glyphs()
                .iter()
                .all(|glyph| glyph.bitmap.get() == [0; 10])
    }));
    assert!(input
        .resource_inventory()
        .iter()
        .all(|resource| { resource.page() != normal || resource.logical_subpage().get() != 1 }));
    let links = seed.page_enhancements()[&owner()]
        .compositional_links
        .unwrap();
    assert_eq!(
        links
            .link(TeletextX27LinkRole::Drcs)
            .required_subpages()
            .get(),
        1
    );
    assert_eq!(
        links
            .link(TeletextX27LinkRole::Gdrcs)
            .required_subpages()
            .get(),
        1
    );

    let encoded = encode(&input);
    assert_eq!(encoded.declared(), &input);
    assert_eq!(encoded.compiler_drcs_resources().len(), 1);
    let generated = &encoded.compiler_drcs_resources()[0];
    let identity = generated.table().identity();
    assert_eq!(identity.kind(), TeletextResourceKind::Drcs);
    assert_eq!(identity.page(), normal);
    assert_eq!(identity.logical_subpage().get(), 1);
    assert_eq!(generated.consumers(), &[owner()]);
    assert_eq!(generated.table().glyphs().len(), 1);
    assert_eq!(generated.table().glyphs()[0].slot.get(), 0);
    assert_eq!(generated.table().glyphs()[0].bitmap.get(), POLISH_5B_MASK);
    let effective = generated.compositional_links(owner()).unwrap();
    for role in TeletextX27LinkRole::ALL {
        let old = links.link(role);
        let new = effective.link(role);
        if role == TeletextX27LinkRole::Drcs {
            assert_eq!(new.target(), old.target());
            assert_eq!(new.validity(), old.validity());
            assert_eq!(new.required_subpages().get(), 3);
        } else {
            assert_eq!(new, old);
        }
    }
    let display = display(&encoded);
    assert_polish_original(display.normalized().original());
    assert_eq!(rows(display.page()), rows(&fallback));
    let actual = display.normalized().physical().replay();
    let candidate = selected(actual.display_resolution(), 1, 7);
    let executed = actual
        .drcs_execution()
        .records()
        .iter()
        .find(|record| record.candidate_index() == candidate)
        .unwrap();
    assert_eq!(executed.resource(), identity);
    assert_eq!(executed.bitmap().get(), POLISH_5B_MASK);
    for table in seed.drcs().subtables() {
        let tx = authored::transmissions(&input)
            .into_iter()
            .find(|tx| tx.resource == table.identity())
            .unwrap();
        let expected = crate::teletext::encode_teletext_drcs_page(
            table,
            tx.counter,
            tx.update,
            crate::teletext::TeletextResourceTransmissionMode::Serial,
        )
        .unwrap();
        let physical = encoded
            .pages()
            .iter()
            .find(|page| {
                page.resource_header()
                    .is_some_and(|header| header.identity() == table.identity())
            })
            .unwrap();
        assert_eq!(physical.data_units(), expected.data_units());
    }
    assert_eq!(encoded.declared().semantic(), &seed);
}
