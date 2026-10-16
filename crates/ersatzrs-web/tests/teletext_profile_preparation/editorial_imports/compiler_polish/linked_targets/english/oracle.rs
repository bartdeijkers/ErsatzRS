//! Original identities, fixed native pixels, exact resources and literal fallback.

use super::*;
use std::collections::BTreeSet;

const LATIN_A: [u16; 10] = [0, 0xf0, 0x198, 0x30c, 0x606, 0x7fe, 0x606, 0x606, 0, 0];

fn display(
    service: &EncodedTeletextAuthoredService,
    page: u16,
) -> &ersatzrs_ffmpeg::teletext::EncodedTeletextCompiledDisplayPage {
    service
        .pages()
        .iter()
        .find_map(|candidate| match candidate {
            EncodedTeletextAuthoredPage::CompiledDisplay(display)
                if display.page().address() == address(page) =>
            {
                Some(display)
            }
            _ => None,
        })
        .unwrap()
}

fn pairs() -> BTreeSet<TeletextCharsetPair> {
    BTreeSet::from([
        TeletextCharsetPair::LatinLatin,
        TeletextCharsetPair::GreekGreek,
    ])
}

fn assert_identity(service: &EncodedTeletextAuthoredService, page: u16, code: u8, primary: u8) {
    let original = display(service, page).normalized().original();
    assert!(matches!(primary, 0 | 0x10));
    assert_eq!(original.base_display().primary().get(), primary);
    if primary == 0x10 {
        let metadata = &service.declared().semantic().page_enhancements()[&address(page)];
        assert_eq!(
            metadata.presentation.as_ref().unwrap().primary.get(),
            primary
        );
        assert_eq!(
            display(service, page)
                .normalized()
                .physical()
                .replay()
                .charset_execution()
                .primary()
                .get(),
            primary
        );
        let projection =
            project_encoded_teletext_display(display(service, page).page(), 128).unwrap();
        let words = projection
            .records
            .iter()
            .find_map(|record| match record {
                TeletextEncodedDisplayRecord::Triplets {
                    row: 28,
                    designation: 0,
                    words,
                } => Some(words),
                _ => None,
            })
            .expect("raw alias10 must be emitted in the actual X28/0");
        assert_eq!((words[0] >> 7) & 0x7f, u32::from(primary));
    }
    assert_eq!(original.collected().charset_pairs(), &pairs());
    let view = original.display_resolution();
    let left = original.display_geometry().left_width();
    let origin = view.cell(1, left + 7).unwrap().origin().unwrap();
    assert_eq!(view.cell(1, left + 8).unwrap().origin(), Some(origin));
    assert_eq!(
        view.origins()[origin].effective_size(),
        Some(TeletextDisplaySize::DoubleWidth)
    );
    let index = view.origins()[origin].candidate_index();
    let candidate = original.glyph_layers().candidates()[index];
    assert_eq!(candidate.priority(), TeletextGlyphPriority::Base);
    assert_eq!(candidate.base().unwrap().code(), code);
    assert_eq!(candidate.source(), None);
    assert_eq!(candidate.invocation(), None);
    let intent = original.glyph_intent().records()[index];
    assert_eq!(intent.disposition(), TeletextGlyphDisposition::Assigned);
    assert!(
        matches!(intent.glyph(),Some(TeletextGlyphIdentity::G0 {glyph})
        if glyph.repertoire() == TeletextG0Repertoire::Latin && glyph.code() == code
            && glyph.national_subset() == if code == 0x7c {Some(TeletextNationalSubset::English)} else {None})
    );
    for column in [left + 7, left + 8] {
        let cell = view.cell(1, column).unwrap();
        assert_eq!(
            cell.attributes().underline_or_separated_mosaics().value(),
            Some(false)
        );
        assert_eq!(cell.attributes().spacing_separated().value(), Some(false));
    }
    let greek = view.cell(4, left + 12).unwrap().origin().unwrap();
    let index = view.origins()[greek].candidate_index();
    let glyph = original.glyph_intent().records()[index];
    assert_eq!(glyph.disposition(), TeletextGlyphDisposition::Assigned);
    assert!(
        matches!(glyph.glyph(),Some(TeletextGlyphIdentity::G0 {glyph})
        if glyph.repertoire() == TeletextG0Repertoire::Greek && glyph.code() == b'A'
            && glyph.national_subset().is_none())
    );
}

fn assert_pixels(
    raster: &TeletextRaster,
    left: usize,
    row: usize,
    column: usize,
    width: usize,
    mask: [u16; 10],
) {
    for (y, bits) in mask.into_iter().enumerate() {
        for x in 0..width {
            let expected = if bits & (1 << (11 - x * 12 / width)) == 0 {
                [0, 0, 0]
            } else {
                [15, 15, 15]
            };
            assert!(matches!(raster.pixel((left+column)*12+x,row*10+y),
                Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(value)))
                    if value.get() == expected));
        }
    }
}

pub(super) fn previews(service: &EncodedTeletextAuthoredService, code: u8) -> Vec<TeletextRaster> {
    previews_with_primary(service, code, 0)
}

pub(super) fn previews_with_primary(
    service: &EncodedTeletextAuthoredService,
    code: u8,
    primary: u8,
) -> Vec<TeletextRaster> {
    (124..127)
        .map(|page| {
            assert_identity(service, page, code, primary);
            let raster = external_editor_probe::preview_page(service, page);
            let left = usize::from(
                display(service, page)
                    .normalized()
                    .original()
                    .display_geometry()
                    .left_width(),
            );
            assert_pixels(
                &raster,
                left,
                1,
                7,
                24,
                if code == 0x7c { ENGLISH_7C } else { LATIN_A },
            );
            assert_pixels(&raster, left, 4, 12, 12, LATIN_A);
            raster
        })
        .collect()
}

pub(super) fn assert_originals(flow: &Flow) {
    assert_originals_with_primary(flow, 0);
}

pub(super) fn assert_originals_with_primary(flow: &Flow, primary: u8) {
    for accepted in &flow.accepted {
        let service = accepted.service();
        let declared = service.declared();
        assert_eq!(declared.semantic().base().pages.len(), 3);
        assert_eq!(declared.resource_inventory().len(), 4);
        assert!(service.compiler_drcs_resources().is_empty());
        for identity in authored_identities() {
            let table = declared
                .semantic()
                .drcs()
                .subtables()
                .iter()
                .find(|table| table.identity() == identity)
                .unwrap();
            assert_eq!(table.glyphs().len(), 1);
            assert_eq!(table.glyphs()[0].slot.get(), 0);
            assert_eq!(table.glyphs()[0].bitmap.get(), [0; 10]);
        }
        for page in 124..127 {
            let metadata = &declared.semantic().page_enhancements()[&address(page)];
            assert_eq!(
                metadata.resources.drcs.page(TeletextDrcsTableScope::Normal),
                Some(normal_identity(page, 0).page())
            );
            assert_eq!(
                metadata.resources.drcs.page(TeletextDrcsTableScope::Global),
                Some(authored_identities()[3].page())
            );
            for role in [TeletextX27LinkRole::Drcs, TeletextX27LinkRole::Gdrcs] {
                assert_eq!(
                    metadata
                        .compositional_links
                        .unwrap()
                        .link(role)
                        .required_subpages()
                        .get(),
                    1
                );
            }
        }
        previews_with_primary(service, b'A', primary);
    }
    let day = &flow.accepted[0].selection().bindings[0];
    let night = &flow.accepted[1].selection().bindings[0];
    assert_eq!(day.addon_id, night.addon_id);
    assert_ne!(day.profile_id, night.profile_id);
}

fn assert_original_resources(flow: &Flow, service: &EncodedTeletextAuthoredService) {
    for old in flow.accepted[0]
        .service()
        .pages()
        .iter()
        .filter(|page| page.resource_header().is_some())
    {
        let identity = old.resource_header().unwrap().identity();
        let current = service
            .pages()
            .iter()
            .find(|page| {
                page.resource_header()
                    .is_some_and(|header| header.identity() == identity)
            })
            .unwrap();
        assert_eq!(current.data_units(), old.data_units());
    }
}

fn base_rows(
    service: &EncodedTeletextAuthoredService,
    page: u16,
) -> Vec<TeletextEncodedDisplayRecord> {
    project_encoded_teletext_display(display(service, page).page(), 128)
        .unwrap()
        .records
        .into_iter()
        .filter(|record| matches!(record, TeletextEncodedDisplayRecord::BaseRow { .. }))
        .collect()
}

fn assert_generated(service: &EncodedTeletextAuthoredService, page: u16) {
    let identity = normal_identity(page, 1);
    let resource = service
        .compiler_drcs_resources()
        .iter()
        .find(|resource| resource.table().identity() == identity)
        .unwrap();
    assert_eq!(resource.consumers(), [address(page)]);
    assert_eq!(resource.table().glyphs().len(), 1);
    assert_eq!(resource.table().glyphs()[0].slot.get(), 8);
    assert_eq!(resource.table().glyphs()[0].bitmap.get(), ENGLISH_7C);
    let links = resource.compositional_links(address(page)).unwrap();
    assert_eq!(
        links.link(TeletextX27LinkRole::Drcs).target(),
        TeletextX27LinkTarget::Page {
            page: identity.page()
        }
    );
    assert_eq!(
        links
            .link(TeletextX27LinkRole::Drcs)
            .required_subpages()
            .get(),
        3
    );
    assert_eq!(
        links
            .link(TeletextX27LinkRole::Gdrcs)
            .required_subpages()
            .get(),
        1
    );
    let resource_page = service
        .pages()
        .iter()
        .find(|page| {
            page.resource_header()
                .is_some_and(|header| header.identity() == identity)
        })
        .unwrap();
    let projected =
        ersatzrs_ffmpeg::teletext::project_encoded_teletext_drcs(resource_page, 32).unwrap();
    let cells = projected
        .records
        .iter()
        .find_map(|record| match record {
            TeletextEncodedDisplayRecord::BaseRow { row: 5, cells } => Some(cells),
            _ => None,
        })
        .unwrap();
    let mask: [u16; 10] = std::array::from_fn(|row| {
        (u16::from(cells[row * 2] & 63) << 6) | u16::from(cells[row * 2 + 1] & 63)
    });
    assert_eq!(mask, ENGLISH_7C);
    assert_eq!(&cells[20..], &[b' '; 20]);
    let physical = display(service, page).normalized().physical().replay();
    let charset = physical.charset_execution();
    let actual_pairs = std::iter::once(charset.primary().pair())
        .chain(
            charset
                .secondary()
                .map(TeletextPageCharsetDesignation::pair),
        )
        .chain(
            charset
                .declarations()
                .iter()
                .map(|value| value.designation().pair()),
        )
        .collect::<BTreeSet<_>>();
    assert_eq!(actual_pairs, pairs());
    let left = display(service, page)
        .normalized()
        .original()
        .display_geometry()
        .left_width();
    for column in [left + 7, left + 8] {
        let cell = physical.display_resolution().cell(1, column).unwrap();
        let index =
            physical.display_resolution().origins()[cell.origin().unwrap()].candidate_index();
        let record = physical
            .drcs_execution()
            .records()
            .iter()
            .find(|record| record.candidate_index() == index)
            .unwrap();
        assert_eq!(record.resource(), identity);
        assert_eq!(record.reference().table, TeletextDrcsTableScope::Normal);
        assert_eq!(record.reference().subtable.get(), 1);
        assert_eq!(record.reference().slot.get(), 8);
        assert_eq!(record.bitmap().get(), ENGLISH_7C);
        assert_eq!(
            cell.attributes().underline_or_separated_mosaics().value(),
            Some(false)
        );
    }
}

pub(super) fn assert_service(flow: &Flow, service: &EncodedTeletextAuthoredService) {
    assert_service_with_primary(flow, service, 0);
}

pub(super) fn assert_service_with_primary(
    flow: &Flow,
    service: &EncodedTeletextAuthoredService,
    primary: u8,
) {
    let expected = flow.accepted[0]
        .service()
        .declared()
        .clone()
        .with_physical_base_overlays(
            expected_overlays(flow),
            input::encoding().validation.service,
        )
        .unwrap();
    assert_eq!(service.declared(), &expected);
    assert_eq!(
        service.magazine_units(),
        flow.accepted[0].service().magazine_units()
    );
    assert_eq!(service.compiler_drcs_resources().len(), 3);
    for page in 124..127 {
        assert_generated(service, page);
        let mut expected = base_rows(flow.accepted[0].service(), page);
        for row in &mut expected {
            if let TeletextEncodedDisplayRecord::BaseRow { row: 1, cells } = row {
                assert_eq!(cells[7], b'A');
                cells[7] = 0x7c;
            }
        }
        assert_eq!(base_rows(service, page), expected);
    }
    assert_original_resources(flow, service);
    previews_with_primary(service, 0x7c, primary);
}
