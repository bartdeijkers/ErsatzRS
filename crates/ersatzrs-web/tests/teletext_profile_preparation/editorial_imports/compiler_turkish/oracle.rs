//! Literal original/fallback and compiler-owned Normal DRCS response.

use super::*;

fn rows(service: &EncodedTeletextAuthoredService) -> Vec<TeletextEncodedDisplayRecord> {
    let page = service
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page)
                if page.page().address() == channel_documents::address(124, 32) =>
            {
                Some(page.page())
            }
            _ => None,
        })
        .unwrap();
    project_encoded_teletext_display(page, 128)
        .unwrap()
        .records
        .into_iter()
        .filter(|record| matches!(record, TeletextEncodedDisplayRecord::BaseRow { .. }))
        .collect()
}

pub(super) fn assert_fallback(flow: &Flow, service: &EncodedTeletextAuthoredService) {
    let mut expected = rows(flow.accepted[0].service());
    let cells = expected
        .iter_mut()
        .find_map(|record| match record {
            TeletextEncodedDisplayRecord::BaseRow { row: 1, cells } => Some(cells),
            _ => None,
        })
        .unwrap();
    assert_eq!(cells[7], b'A');
    cells[7] = 0x23;
    assert_eq!(rows(service), expected);
}

pub(super) fn assert_generated(service: &EncodedTeletextAuthoredService) {
    assert_eq!(service.compiler_drcs_resources().len(), 1);
    assert_eq!(
        service
            .pages()
            .iter()
            .filter(|page| page.resource_header().is_some())
            .count(),
        1
    );
    let resource = &service.compiler_drcs_resources()[0];
    assert_eq!(resource.consumers(), [channel_documents::address(124, 32)]);
    let identity = resource.table().identity();
    assert_eq!(identity.kind(), TeletextResourceKind::Drcs);
    assert_eq!(identity.page().magazine(), 1);
    assert_eq!(identity.page().page(), 0xa0);
    assert_eq!(identity.logical_subpage().get(), 0);
    assert_eq!(resource.table().glyphs().len(), 1);
    assert_eq!(resource.table().glyphs()[0].slot.get(), 2);
    assert_eq!(resource.table().glyphs()[0].bitmap.get(), MASK);
    let page = service
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page)
                if page.page().address() == channel_documents::address(124, 32) =>
            {
                Some(page)
            }
            _ => None,
        })
        .unwrap();
    let original = page.normalized().original();
    assert_base(original, 0x16, 0x23);
    let column = original.display_geometry().left_width() + 7;
    let actual = page.normalized().physical().replay();
    for col in [column, column + 1] {
        let first = original.display_resolution().cell(1, col).unwrap();
        let second = actual.display_resolution().cell(1, col).unwrap();
        assert_eq!(first.fragment(), second.fragment());
        assert_eq!(
            first.attributes().underline_or_separated_mosaics().value(),
            Some(false)
        );
        assert_eq!(first.attributes().spacing_separated().value(), Some(false));
        assert_eq!(
            second.attributes().underline_or_separated_mosaics().value(),
            Some(false)
        );
        assert_eq!(second.attributes().spacing_separated().value(), Some(false));
        let expected =
            ersatzrs_ffmpeg::teletext::project_teletext_comparison_glyph(original, first).unwrap();
        assert_eq!(expected.mask.unwrap().rows(), MASK);
    }
    let origin = actual
        .display_resolution()
        .cell(1, column)
        .unwrap()
        .origin()
        .unwrap();
    let candidate = actual.display_resolution().origins()[origin].candidate_index();
    let record = actual
        .drcs_execution()
        .records()
        .iter()
        .find(|record| record.candidate_index() == candidate)
        .unwrap();
    assert_eq!(record.resource(), identity);
    assert_eq!(record.reference().table, TeletextDrcsTableScope::Normal);
    assert_eq!(record.reference().subtable.get(), 0);
    assert_eq!(record.reference().slot.get(), 2);
    assert_eq!(record.bitmap().get(), MASK);
    assert_pixels(service, usize::from(column));
}

fn assert_pixels(service: &EncodedTeletextAuthoredService, column: usize) {
    let raster = external_editor_probe::preview_page(service, 124);
    for (row, mask) in MASK.into_iter().enumerate() {
        for x in 0..24 {
            let expected = if mask & (1 << (11 - x / 2)) == 0 {
                [0, 0, 0]
            } else {
                [15, 15, 15]
            };
            assert!(
                matches!(raster.pixel(column*12+x,10+row),
                Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(value))) if value.get()==expected),
                "Turkish TL pixel{x},{row}"
            );
        }
    }
}
