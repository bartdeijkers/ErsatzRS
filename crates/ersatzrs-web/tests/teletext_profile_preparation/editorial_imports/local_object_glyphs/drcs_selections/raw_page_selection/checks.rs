//! Qualify exact original/final sources before asserting native selection authority.
use super::*;

fn pixels(service: &EncodedTeletextAuthoredService, selected: bool, night: bool) {
    let composition = service
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page)
                if page.page().address() == channel_documents::address(124, 32) =>
            {
                Some(page.normalized().original())
            }
            _ => None,
        })
        .unwrap();
    let raster = external_editor_probe::preview_page(service, 124);
    assert_eq!((raster.width(), raster.height()), (672, 250));
    for (visual, target) in VISUAL.into_iter().zip([0, 1, 2, 3, 2, 3]) {
        let TeletextColumnOperation::Drcs { reference } = operation(0, target) else {
            unreachable!()
        };
        let expected_resource = if selected {
            source::alternate(reference.table, reference.subtable.get())
        } else {
            bank(reference.table, reference.subtable.get())
        };
        let expected_mask = if selected {
            source::mask(
                reference.table,
                reference.subtable.get(),
                reference.slot.get(),
            )
        } else {
            fixture::bank_mask(
                reference.table,
                reference.subtable.get(),
                reference.slot.get(),
            )
        };
        let origin = composition
            .display_resolution()
            .cell(1, u8::try_from(visual).unwrap())
            .unwrap()
            .origin()
            .unwrap();
        let candidate = composition.display_resolution().origins()[origin].candidate_index();
        let executed = composition
            .drcs_execution()
            .records()
            .iter()
            .find(|record| record.candidate_index() == candidate)
            .unwrap();
        assert_eq!(executed.reference(), reference);
        assert_eq!(executed.resource(), expected_resource);
        assert_eq!(executed.bitmap().get(), expected_mask);
        for (row, bits) in expected_mask.into_iter().enumerate() {
            for x in 0..12 {
                let expected = if bits & (1 << (11 - x)) == 0 {
                    [0, 0, 0]
                } else if night {
                    [0, 0, 15]
                } else {
                    [15, 0, 0]
                };
                let Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(pixel))) =
                    raster.pixel(visual * 12 + x, 10 + row)
                else {
                    panic!("DRCS cell needs a settled RGB4 pixel")
                };
                assert_eq!(pixel.get(), expected);
            }
        }
    }
}
fn immutable(before: &EncodedTeletextAuthoredService, after: &EncodedTeletextAuthoredService) {
    assert_eq!(
        before.declared().semantic().base(),
        after.declared().semantic().base()
    );
    assert_eq!(
        before.declared().semantic().objects(),
        after.declared().semantic().objects()
    );
    assert_eq!(
        before.declared().semantic().drcs(),
        after.declared().semantic().drcs()
    );
    assert_eq!(before.declared().mots(), after.declared().mots());
    assert_eq!(before.declared().defaults(), after.declared().defaults());
    assert_eq!(
        before.declared().selections(),
        after.declared().selections()
    );
    assert_eq!(
        before.declared().resource_inventory(),
        after.declared().resource_inventory()
    );
    assert_eq!(before.magazine_units(), after.magazine_units());
    assert_eq!(before.pages().len(), after.pages().len());
    assert_eq!(oracle::packet_words(before), oracle::packet_words(after));
    assert_eq!(
        external_editor_probe::preview_page(before, 125),
        external_editor_probe::preview_page(after, 125)
    );
    for (old, new) in before.pages().iter().zip(after.pages()) {
        assert_eq!(old.resource_header(), new.resource_header());
        if matches!(old, EncodedTeletextAuthoredPage::CompiledDisplay(page) if page.page().address() == channel_documents::address(124, 32))
        {
            assert_ne!(
                old.data_units(),
                new.data_units(),
                "only selected page X27 targets change"
            );
        } else {
            assert_eq!(
                old.data_units(),
                new.data_units(),
                "all dormant/old banks and sibling display remain exact"
            );
        }
    }
}
pub(super) fn qualified(before: &Flow, after: &Flow) {
    for index in 0..2 {
        let old = before.accepted[index].service();
        let new = after.accepted[index].service();
        assert_eq!(old.declared().semantic().drcs().subtables().len(), 8);
        assert!(old.compiler_drcs_resources().is_empty());
        assert!(new.compiler_drcs_resources().is_empty());
        immutable(old, new);
        pixels(old, false, index == 1);
        pixels(new, true, index == 1);
        let display = channel_documents::address(124, 32);
        let mut metadata = new.declared().semantic().page_enhancements()[&display].clone();
        let original = &old.declared().semantic().page_enhancements()[&display];
        metadata.resources.drcs = original.resources.drcs;
        metadata.compositional_links = original.compositional_links;
        assert_eq!(
            &metadata, original,
            "programs, local/default provenance and presentation remain exact"
        );
    }
}
