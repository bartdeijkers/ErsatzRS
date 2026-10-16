//! Fixed national identities, scope ownership, fallback and complete previews.

use super::*;
use ersatzrs_ffmpeg::teletext::EncodedTeletextCompiledDisplayPage;

const POLISH: [u16; 10] = [0, 0x3fc, 0x18, 0x30, 0x1f8, 0xc0, 0x180, 0x3fc, 0, 0];
const TURKISH: [u16; 10] = [0, 0, 0x3f0, 0xc0, 0xd8, 0xd8, 0xd8, 0xde, 0, 0];
const LATIN_A: [u16; 10] = [0, 0xf0, 0x198, 0x30c, 0x606, 0x7fe, 0x606, 0x606, 0, 0];

pub(super) fn mask(page: u16, edited: bool) -> [u16; 10] {
    let mut result = if !edited {
        LATIN_A
    } else if slot(page) < 2 {
        POLISH
    } else {
        TURKISH
    };
    if slot(page) % 2 == 1 {
        result[9] = 0xfff;
    }
    result
}

fn display(
    service: &EncodedTeletextAuthoredService,
    page: u16,
) -> &EncodedTeletextCompiledDisplayPage {
    service
        .pages()
        .iter()
        .find_map(|candidate| match candidate {
            EncodedTeletextAuthoredPage::CompiledDisplay(candidate)
                if candidate.page().address() == address(page) =>
            {
                Some(candidate)
            }
            _ => None,
        })
        .unwrap()
}

fn preview(service: &EncodedTeletextAuthoredService, page: u16) -> TeletextRaster {
    let preview = project_teletext_preview(
        service,
        address(page),
        TeletextColourPixelOptions {
            reveal_concealed: false,
            flash_phase: TeletextFlashPhase::Original,
        },
        preview_budget(),
    )
    .unwrap();
    assert!(preview.raster().diagnostics().is_empty());
    assert_eq!(
        preview.navigation(),
        service
            .declared()
            .semantic()
            .base()
            .pages
            .iter()
            .find(|candidate| candidate.address == address(page))
            .unwrap()
            .navigation
    );
    preview.raster().clone()
}

pub(super) fn previews(service: &EncodedTeletextAuthoredService) -> Vec<TeletextRaster> {
    (124..132).map(|page| preview(service, page)).collect()
}

fn assert_original(service: &EncodedTeletextAuthoredService, page: u16, edited: bool) {
    let original = display(service, page).normalized().original();
    assert_eq!(
        original.base_display().primary().get(),
        if slot(page) < 2 { 8 } else { 22 }
    );
    let column = original.display_geometry().left_width() + 7;
    let view = original.display_resolution();
    let origin = view.cell(1, column).unwrap().origin().unwrap();
    assert_eq!(view.cell(1, column + 1).unwrap().origin(), Some(origin));
    assert_eq!(
        view.origins()[origin].effective_size(),
        Some(TeletextDisplaySize::DoubleWidth)
    );
    let index = view.origins()[origin].candidate_index();
    let candidate = original.glyph_layers().candidates()[index];
    assert_eq!(candidate.priority(), TeletextGlyphPriority::Base);
    assert_eq!(
        candidate.base().unwrap().code(),
        if edited { code(page) } else { b'A' }
    );
    assert_eq!(candidate.source(), None);
    assert_eq!(candidate.invocation(), None);
    let intent = original.glyph_intent().records()[index];
    assert_eq!(intent.disposition(), TeletextGlyphDisposition::Assigned);
    assert!(
        matches!(intent.glyph(),Some(TeletextGlyphIdentity::G0 {glyph})
        if glyph.repertoire()==TeletextG0Repertoire::Latin
            && glyph.code()==if edited {code(page)} else {b'A'}
            && glyph.national_subset()==if edited {Some(if slot(page)<2 {TeletextNationalSubset::Polish} else {TeletextNationalSubset::Turkish})} else {None})
    );
    for column in [column, column + 1] {
        let cell = view.cell(1, column).unwrap();
        assert_eq!(
            cell.attributes().underline_or_separated_mosaics().value(),
            Some(slot(page) % 2 == 1)
        );
        assert_eq!(cell.attributes().spacing_separated().value(), Some(false));
        assert_eq!(
            ersatzrs_ffmpeg::teletext::project_teletext_comparison_glyph(original, cell)
                .unwrap()
                .mask
                .unwrap()
                .rows(),
            mask(page, edited)
        );
    }
    let image = preview(service, page);
    for (y, row) in mask(page, edited).into_iter().enumerate() {
        for x in 0..24 {
            let expected = if row & (1 << (11 - x / 2)) == 0 {
                [0, 0, 0]
            } else {
                [15, 15, 15]
            };
            assert!(matches!(image.pixel(usize::from(column)*12+x,10+y),
                Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(value))) if value.get()==expected));
        }
    }
}

pub(super) fn assert_originals(flow: &Flow) {
    for accepted in &flow.accepted {
        let service = accepted.service();
        assert_eq!(service.declared().semantic().base().pages.len(), 8);
        assert_eq!(
            service.declared().resource_inventory(),
            &[authored_bank().identity()]
        );
        assert_eq!(
            service.declared().semantic().drcs().subtables(),
            &[authored_bank()]
        );
        assert!(service.compiler_drcs_resources().is_empty());
        for page in 124..132 {
            assert_original(service, page, false);
            let metadata = &service.declared().semantic().page_enhancements()[&address(page)];
            assert_eq!(
                metadata.resources.drcs.page(TeletextDrcsTableScope::Global),
                None
            );
            assert_eq!(
                metadata.resources.drcs.page(TeletextDrcsTableScope::Normal),
                if page < 128 {
                    None
                } else {
                    Some(authored_bank().identity().page())
                }
            );
        }
    }
    let day = &flow.accepted[0].selection().bindings[0];
    let night = &flow.accepted[1].selection().bindings[0];
    assert_eq!(day.addon_id, night.addon_id);
    assert_ne!(day.profile_id, night.profile_id);
    assert_eq!(flow.exported[0].sidecar().project.pages.len(), 8);
    assert_eq!(flow.exported[0].sidecar().project.producers.len(), 1);
    let authoring = flow.exported[0].sidecar().project.producers[0]
        .authoring
        .as_ref()
        .unwrap();
    assert_eq!(authoring.pages.len(), 8);
    for page in &authoring.pages {
        assert!(page
            .regions
            .iter()
            .any(|region| region.id == "free-national-base"
                && region.row == 1
                && region.column == 7
                && region.width == 2
                && region.height == 1));
    }
}

fn assert_fallback(flow: &Flow, service: &EncodedTeletextAuthoredService, page: u16) {
    let before =
        project_encoded_teletext_display(display(flow.accepted[0].service(), page).page(), 128)
            .unwrap();
    let after = project_encoded_teletext_display(display(service, page).page(), 128).unwrap();
    assert_eq!(before.header.controls.raw(), after.header.controls.raw());
    let retain = |records: Vec<TeletextEncodedDisplayRecord>| {
        records
            .into_iter()
            .filter(|record| {
                matches!(
                    record,
                    TeletextEncodedDisplayRecord::BaseRow { .. }
                        | TeletextEncodedDisplayRecord::HeaderText(_)
                )
            })
            .collect::<Vec<_>>()
    };
    let mut expected = retain(before.records);
    let cells = expected
        .iter_mut()
        .find_map(|record| match record {
            TeletextEncodedDisplayRecord::BaseRow { row: 1, cells } => Some(cells),
            _ => None,
        })
        .unwrap();
    assert_eq!(cells[7], b'A');
    cells[7] = code(page);
    assert_eq!(retain(after.records), expected);
}

fn assert_execution(service: &EncodedTeletextAuthoredService, page: u16) {
    let (scope, kind) = if page < 128 {
        (TeletextDrcsTableScope::Normal, TeletextResourceKind::Drcs)
    } else {
        (TeletextDrcsTableScope::Global, TeletextResourceKind::Gdrcs)
    };
    let resource = service
        .compiler_drcs_resources()
        .iter()
        .find(|resource| resource.table().identity().kind() == kind)
        .unwrap();
    assert!(resource.consumers().contains(&address(page)));
    let actual = display(service, page).normalized().physical().replay();
    let left = display(service, page)
        .normalized()
        .original()
        .display_geometry()
        .left_width();
    for column in [left + 7, left + 8] {
        let cell = actual.display_resolution().cell(1, column).unwrap();
        assert_eq!(
            cell.attributes().underline_or_separated_mosaics().value(),
            Some(false)
        );
        let index = actual.display_resolution().origins()[cell.origin().unwrap()].candidate_index();
        let record = actual
            .drcs_execution()
            .records()
            .iter()
            .find(|record| record.candidate_index() == index)
            .unwrap();
        assert_eq!(record.reference().table, scope);
        assert_eq!(record.reference().subtable.get(), 0);
        assert_eq!(record.reference().slot.get(), slot(page));
        assert_eq!(record.resource(), resource.table().identity());
        assert_eq!(record.bitmap().get(), mask(page, true));
    }
    let links = resource.compositional_links(address(page)).unwrap();
    if page >= 128 {
        let original = service.declared().semantic().page_enhancements()[&address(page)]
            .compositional_links
            .unwrap();
        for role in [
            TeletextX27LinkRole::Gpop,
            TeletextX27LinkRole::Pop,
            TeletextX27LinkRole::Drcs,
        ] {
            assert_eq!(links.link(role), original.link(role));
        }
    }
}

fn assert_ptus(service: &EncodedTeletextAuthoredService) {
    assert_eq!(service.compiler_drcs_resources().len(), 2);
    for resource in service.compiler_drcs_resources() {
        let id = resource.table().identity();
        assert_eq!(id.page().magazine(), 1);
        assert_eq!(
            id.page().page(),
            if id.kind() == TeletextResourceKind::Drcs {
                0xa0
            } else {
                0xa1
            }
        );
        assert_eq!(id.logical_subpage().get(), 0);
        assert_eq!(
            resource
                .table()
                .glyphs()
                .iter()
                .map(|glyph| glyph.slot.get())
                .collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
        let page = service
            .pages()
            .iter()
            .find(|page| {
                page.resource_header()
                    .is_some_and(|header| header.identity() == id)
            })
            .unwrap();
        let header = page.resource_header().unwrap();
        assert_eq!(header.update_counter().get(), 0);
        assert_eq!(
            header.transmission(),
            TeletextResourceTransmission::FullUnchangedFirst
        );
        let projection =
            ersatzrs_ffmpeg::teletext::project_encoded_teletext_drcs(page, 32).unwrap();
        for slot in 0_u8..4 {
            let cells = projection
                .records
                .iter()
                .find_map(|record| match record {
                    TeletextEncodedDisplayRecord::BaseRow { row, cells }
                        if *row == slot / 2 + 1 =>
                    {
                        Some(cells)
                    }
                    _ => None,
                })
                .unwrap();
            let offset = usize::from(slot % 2) * 20;
            let actual = std::array::from_fn(|row| {
                (u16::from(cells[offset + row * 2] & 0x3f) << 6)
                    | u16::from(cells[offset + row * 2 + 1] & 0x3f)
            });
            assert_eq!(actual, mask(124 + u16::from(slot), true));
        }
    }
}

pub(super) fn assert_generated(flow: &Flow, service: &EncodedTeletextAuthoredService) {
    assert_eq!(
        service.declared().semantic().drcs().subtables(),
        &[authored_bank()]
    );
    let original = flow.accepted[0]
        .service()
        .pages()
        .iter()
        .find(|page| {
            page.resource_header()
                .is_some_and(|header| header.identity() == authored_bank().identity())
        })
        .unwrap();
    let actual = service
        .pages()
        .iter()
        .find(|page| {
            page.resource_header()
                .is_some_and(|header| header.identity() == authored_bank().identity())
        })
        .unwrap();
    assert_eq!(original.data_units(), actual.data_units());
    assert_ptus(service);
    for page in 124..132 {
        assert_original(service, page, true);
        assert_fallback(flow, service, page);
        assert_execution(service, page);
        let before = preview(flow.accepted[0].service(), page);
        let after = preview(service, page);
        assert_ne!(before, after);
        assert_eq!(before.width(), after.width());
        assert_eq!(before.pixels().len(), after.pixels().len());
        let left = usize::from(
            display(service, page)
                .normalized()
                .original()
                .display_geometry()
                .left_width(),
        );
        for (offset, (old, new)) in before.pixels().iter().zip(after.pixels()).enumerate() {
            if old != new {
                let (x, y) = (offset % after.width(), offset / after.width());
                assert!(((left + 7) * 12..(left + 9) * 12).contains(&x) && (10..20).contains(&y));
            }
        }
    }
}
