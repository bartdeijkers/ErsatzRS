//! Literal pinned masks, original identities, fallback and private slot ownership.

use super::*;
use ersatzrs_web::host::teletext_editorial_designs::{
    NativeTeletextEditorialImportError, NativeTeletextEditorialTtiFailure,
};

const CAPITAL: [u16; 10] = [0x0d8, 0x070, 0x1fc, 0x306, 0x306, 0x3fe, 0x306, 0x306, 0, 0];
const SMALL: [u16; 10] = [0x0d8, 0x070, 0, 0x3f8, 0x00c, 0x3fc, 0x60c, 0x3fe, 0, 0];
const LATIN_A: [u16; 10] = [0, 0xf0, 0x198, 0x30c, 0x606, 0x7fe, 0x606, 0x606, 0, 0];

pub(super) fn mask(page: u16, edited: bool) -> [u16; 10] {
    let mut result = if !edited {
        LATIN_A
    } else if variant(page) < 2 {
        CAPITAL
    } else {
        SMALL
    };
    if variant(page) % 2 == 1 {
        result[9] = 0xfff;
    }
    result
}

fn display(
    service: &EncodedTeletextAuthoredService,
    page: u16,
) -> &ersatzrs_ffmpeg::teletext::EncodedTeletextCompiledDisplayPage {
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

pub(super) fn previews(
    service: &EncodedTeletextAuthoredService,
    edited: bool,
) -> Vec<TeletextRaster> {
    (124..128)
        .map(|page| {
            let original = display(service, page).normalized().original();
            assert_eq!(original.base_display().primary().get(), 0x1f);
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
                matches!(intent.glyph(), Some(TeletextGlyphIdentity::G0 { glyph })
                if glyph.repertoire() == TeletextG0Repertoire::Latin
                    && glyph.code() == if edited { code(page) } else { b'A' }
                    && glyph.national_subset() == if edited {
                        Some(TeletextNationalSubset::Romanian)
                    } else {
                        None
                    })
            );
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
            let declared = service
                .declared()
                .semantic()
                .base()
                .pages
                .iter()
                .find(|candidate| candidate.address == address(page))
                .unwrap();
            assert_eq!(preview.navigation(), declared.navigation);
            for cell_column in [column, column + 1] {
                let cell = view.cell(1, cell_column).unwrap();
                assert_eq!(
                    cell.attributes().underline_or_separated_mosaics().value(),
                    Some(variant(page) % 2 == 1)
                );
                assert_eq!(
                    ersatzrs_ffmpeg::teletext::project_teletext_comparison_glyph(original, cell)
                        .unwrap()
                        .mask
                        .unwrap()
                        .rows(),
                    mask(page, edited)
                );
            }
            for (y, row) in mask(page, edited).into_iter().enumerate() {
                for x in 0..24 {
                    let rgb = if row & (1 << (11 - x / 2)) == 0 {
                        [0, 0, 0]
                    } else {
                        [15, 15, 15]
                    };
                    assert!(matches!(
                        preview.raster().pixel(usize::from(column) * 12 + x, 10 + y),
                        Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(value)))
                            if value.get() == rgb
                    ));
                }
            }
            preview.raster().clone()
        })
        .collect()
}

pub(super) fn assert_originals(flow: &Flow) {
    for accepted in &flow.accepted {
        let service = accepted.service();
        assert_eq!(service.declared().semantic().base().pages.len(), 4);
        compiler_polish::assert_authored(service);
        assert!(service.compiler_drcs_resources().is_empty());
        assert!(service
            .pages()
            .iter()
            .all(|page| page.resource_header().is_none()));
        previews(service, false);
    }
    let day = &flow.accepted[0].selection().bindings[0];
    let night = &flow.accepted[1].selection().bindings[0];
    assert_eq!(day.addon_id, night.addon_id);
    assert_ne!(day.profile_id, night.profile_id);
}

pub(super) fn assert_service(flow: &Flow, service: &EncodedTeletextAuthoredService) {
    let original = flow.accepted[0].service();
    let expected = original
        .declared()
        .clone()
        .with_physical_base_overlays(
            expected_overlays(flow),
            channel_documents::policy().validation.service,
        )
        .unwrap();
    assert_eq!(service.declared(), &expected);
    compiler_polish::assert_authored(service);
    assert_eq!(service.magazine_units(), original.magazine_units());
    assert_eq!(service.compiler_drcs_resources().len(), 1);
    let resource = &service.compiler_drcs_resources()[0];
    assert_eq!(resource.table().identity(), resource_identity());
    assert_eq!(
        resource.consumers(),
        (124..128).map(address).collect::<Vec<_>>()
    );
    assert_eq!(resource.table().glyphs().len(), 4);
    for (page, glyph) in (124..128).zip(resource.table().glyphs()) {
        assert_eq!(glyph.slot.get(), slot(page));
        assert_eq!(glyph.bitmap.get(), mask(page, true));
        let display = display(service, page);
        let original = display.normalized().original();
        let column = original.display_geometry().left_width() + 7;
        let actual = display.normalized().physical().replay();
        let index = actual
            .display_resolution()
            .cell(1, column)
            .unwrap()
            .origin()
            .unwrap();
        let candidate = actual.display_resolution().origins()[index].candidate_index();
        let execution = actual
            .drcs_execution()
            .records()
            .iter()
            .find(|record| record.candidate_index() == candidate)
            .unwrap();
        assert_eq!(execution.resource(), resource_identity());
        assert_eq!(execution.reference().table, TeletextDrcsTableScope::Normal);
        assert_eq!(execution.reference().subtable.get(), 0);
        assert_eq!(execution.reference().slot.get(), slot(page));
        assert_eq!(execution.bitmap().get(), mask(page, true));
        for cell_column in [column, column + 1] {
            assert_eq!(
                actual
                    .display_resolution()
                    .cell(1, cell_column)
                    .unwrap()
                    .attributes()
                    .underline_or_separated_mosaics()
                    .value(),
                Some(false)
            );
        }
        let rows = |display: &ersatzrs_ffmpeg::teletext::EncodedTeletextCompiledDisplayPage| {
            ersatzrs_ffmpeg::teletext::project_encoded_teletext_display(display.page(), 128)
                .unwrap()
                .records
                .into_iter()
                .filter(|record| matches!(record, TeletextEncodedDisplayRecord::BaseRow { .. }))
                .collect::<Vec<_>>()
        };
        let mut fallback = rows(self::display(flow.accepted[0].service(), page));
        let cells = fallback
            .iter_mut()
            .find_map(|record| match record {
                TeletextEncodedDisplayRecord::BaseRow { row: 1, cells } => Some(cells),
                _ => None,
            })
            .unwrap();
        assert_eq!(cells[7], b'A');
        cells[7] = code(page);
        assert_eq!(rows(display), fallback);
    }
    assert_eq!(
        service
            .pages()
            .iter()
            .filter(|page| page.resource_header().is_some())
            .count(),
        1
    );
}

pub(super) fn assert_accepted(flow: &Flow, accepted: &[AcceptedTeletextOutputCandidate; 2]) {
    assert_service(flow, accepted[0].service());
    assert_eq!(accepted[0].selection(), flow.accepted[0].selection());
    generations::assert_same_output(&accepted[1], &flow.accepted[1]);
    assert!(accepted[1].service().compiler_drcs_resources().is_empty());
    assert_eq!(
        previews(accepted[1].service(), false),
        previews(flow.accepted[1].service(), false)
    );
}

pub(super) fn assert_mapping(exported: &ExportedOriginalTeletextDesign) {
    let sets = exported
        .sidecar()
        .project
        .assets
        .iter()
        .filter(|asset| matches!(asset.scope, TeletextExportAssetScope::DrcsTtiSet { .. }))
        .collect::<Vec<_>>();
    assert_eq!(sets.len(), 1);
    let asset = sets[0];
    assert_eq!(asset.name, "drcs-set-normal-1a0.tti");
    let TeletextExportAssetScope::DrcsTtiSet { resource_names } = &asset.scope else {
        unreachable!()
    };
    assert_eq!(resource_names.len(), 1);
    let raw = exported
        .sidecar()
        .project
        .assets
        .iter()
        .find(|candidate| Some(&candidate.name) == resource_names[0].as_ref())
        .unwrap();
    let scope = serde_json::to_value(&raw.scope).unwrap();
    assert_eq!(scope["identity"]["kind"], "drcs");
    assert_eq!(scope["identity"]["page"], json!({"magazine":1,"page":160}));
    assert_eq!(scope["identity"]["subpage"], 0);
    assert!(exported.files().iter().any(|file| file.name == raw.name));
    let file = exported
        .files()
        .iter()
        .find(|file| file.name == asset.name)
        .unwrap();
    let tti = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
    assert_eq!(
        tti.records()
            .iter()
            .filter_map(|record| match record {
                TeletextTtiRecord::SubpageOrdinal(ordinal) => Some(*ordinal),
                _ => None,
            })
            .collect::<Vec<_>>(),
        vec![0]
    );
}

pub(super) async fn reject_generated_slots(
    published: &Published,
    flow: &Flow,
    retained: &resource_bank_admin::Retained,
) {
    for page in 124..128 {
        let mut request = drcs_bitmap::from_receipt(&published.receipt, 2);
        let before = request.files.clone();
        let file = request
            .files
            .iter_mut()
            .find(|file| file.name == "drcs-set-normal-1a0.tti")
            .unwrap();
        let mut records = parse_teletext_tti(&file.bytes, policy().tti)
            .unwrap()
            .records()
            .to_vec();
        let row = slot(page) / 2 + 1;
        let offset = usize::from(slot(page) % 2) * 20;
        let cells = records
            .iter_mut()
            .find_map(|record| match record {
                TeletextTtiRecord::BaseRow { row: actual, cells } if *actual == row => Some(cells),
                _ => None,
            })
            .unwrap();
        let old: [u16; 10] = std::array::from_fn(|line| {
            (u16::from(cells[offset + line * 2] & 63) << 6)
                | u16::from(cells[offset + line * 2 + 1] & 63)
        });
        assert_eq!(old, mask(page, true));
        assert!(cells[offset..offset + 20]
            .iter()
            .all(|byte| byte & 0x40 != 0));
        cells[offset + 1] ^= 1;
        let mut changed = old;
        changed[0] ^= 1;
        TeletextDrcsBitmap::new(changed).unwrap();
        file.bytes = write_teletext_tti(
            &TeletextTtiDocument::checked_from_records(records.clone(), policy().tti).unwrap(),
            policy().tti,
        )
        .unwrap();
        assert_eq!(
            parse_teletext_tti(&file.bytes, policy().tti)
                .unwrap()
                .records(),
            records
        );
        assert_ne!(request.files, before);
        for (old, new) in before.iter().zip(&request.files) {
            if old.name != "drcs-set-normal-1a0.tti" {
                assert_eq!(old, new);
            }
        }
        let error = import_teletext_editorial_design(
            &published.fixture.state.application,
            request,
            policy(),
            designs::deadline(),
        )
        .await
        .unwrap_err();
        let NativeTeletextEditorialImportError::Tti(error) = error else {
            panic!("generated Romanian slot must reject at reconciliation: {error:?}")
        };
        assert_eq!(error.failure, NativeTeletextEditorialTtiFailure::Structure);
        assert_eq!(error.resource, Some(resource_identity()));
        assert_eq!((error.page, error.subcode), (0x1a0, 0));
        assert_eq!(error.row, Some(row));
        assert_eq!(error.column, Some(offset));
        Box::pin(assert_retained(published, flow, retained)).await;
    }
}
