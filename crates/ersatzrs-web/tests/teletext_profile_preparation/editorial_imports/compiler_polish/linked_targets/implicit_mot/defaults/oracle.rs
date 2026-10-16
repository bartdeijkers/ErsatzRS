//! Actual Defaults[0,2,3], selected roles and fixed visible Passive P.

use super::*;
use ersatzrs_ffmpeg::teletext::EncodedTeletextCompiledDisplayPage;

const PASSIVE_P_MASK: [u16; 10] = [0, 0x3fc, 0x306, 0x306, 0x3fc, 0x300, 0x300, 0x300, 0, 0];

fn display(
    service: &EncodedTeletextAuthoredService,
    page: u16,
) -> &EncodedTeletextCompiledDisplayPage {
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

fn assert_default_display(service: &EncodedTeletextAuthoredService, page: u16) {
    let original = display(service, page).normalized().original();
    assert_eq!(
        original.original_program_source(),
        &TeletextEffectiveProgramSource::Defaults {
            slots: vec![0, 2, 3]
        }
    );
    let view = original.display_resolution();
    let left = original.display_geometry().left_width();
    let passive = view.cell(4, left + 1).unwrap().origin().unwrap();
    let index = view.origins()[passive].candidate_index();
    let candidate = original.glyph_layers().candidates()[index];
    assert_eq!(candidate.priority(), TeletextGlyphPriority::Passive);
    assert_eq!(
        candidate.root_source(),
        Some(TeletextSpatialRootSource::DefaultSlot { slot: 3 })
    );
    assert!(matches!(candidate.source(),Some(source) if source.object == Some(seed::target(21))));
    assert!(candidate.invocation().is_some());
    let intent = original.glyph_intent().records()[index];
    assert_eq!(intent.disposition(), TeletextGlyphDisposition::Assigned);
    assert!(
        matches!(intent.glyph(),Some(TeletextGlyphIdentity::G0 {glyph})
        if glyph.repertoire() == TeletextG0Repertoire::Latin && glyph.code() == b'P'
            && glyph.national_subset().is_none())
    );
    assert_eq!(
        view.cell(4, left + 1)
            .unwrap()
            .attributes()
            .foreground()
            .value(),
        Some(TeletextColourReference::new(2).unwrap())
    );
    let raster = external_editor_probe::preview_page(service, page);
    for (y, row) in PASSIVE_P_MASK.into_iter().enumerate() {
        for x in 0..12 {
            let expected = if row & (1 << (11 - x)) == 0 {
                [0, 0, 0]
            } else {
                [0, 15, 0]
            };
            assert!(matches!(raster.pixel(usize::from(left+1)*12+x,40+y),
                Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(value)))
                if value.get() == expected));
        }
    }
}

fn assert_defaults(service: &EncodedTeletextAuthoredService) {
    let declared = service.declared();
    assert_eq!(declared.resource_inventory().len(), 7);
    assert_eq!(declared.mots(), &[seed::mot()]);
    assert_eq!(
        declared
            .defaults()
            .iter()
            .map(TeletextDeclaredDefaultReservation::selection)
            .collect::<Vec<_>>(),
        seed::selections()
    );
    assert_eq!(declared.semantic().objects().definitions().len(), 3);
    for wire in seed::objects().as_array().unwrap() {
        let id = u16::try_from(wire["identity"]["object_id"].as_u64().unwrap()).unwrap();
        let object = declared
            .semantic()
            .objects()
            .definitions()
            .iter()
            .find(|object| object.identity == seed::target(id))
            .unwrap();
        assert_eq!(
            serde_json::to_value(object.object_type).unwrap(),
            wire["object_type"]
        );
        let program: TeletextEnhancementProgram =
            serde_json::from_value(wire["program"].clone()).unwrap();
        assert_eq!(object.program, program);
    }
    for page in 124..127 {
        let metadata = &declared.semantic().page_enhancements()[&address(page)];
        assert!(metadata.local_program.is_none());
        assert!(metadata.compositional_links.is_none());
        assert_eq!(metadata.resources.defaults, seed::defaults());
        assert_eq!(
            metadata.resources.objects.public,
            Some(seed::target(21).resource().unwrap().page())
        );
        assert_eq!(
            metadata.resources.objects.global,
            Some(seed::target(20).resource().unwrap().page())
        );
        let resolved = declared
            .resolutions()
            .iter()
            .find(|resolved| resolved.display() == address(page))
            .unwrap();
        assert_eq!(resolved.defaults(), &seed::defaults());
        assert_eq!(resolved.mot().unwrap().selected_mot(), mot_identity());
        assert_eq!(resolved.mot().unwrap().mot_links(), seed::mot().links());
        for (role, target) in [
            (
                TeletextX27LinkRole::Gpop,
                seed::target(20).resource().unwrap(),
            ),
            (
                TeletextX27LinkRole::Pop,
                seed::target(21).resource().unwrap(),
            ),
            (TeletextX27LinkRole::Drcs, normal_identity(page, 0)),
            (TeletextX27LinkRole::Gdrcs, authored_identities()[3]),
        ] {
            let TeletextMotRoleResolution::Selected {
                target: actual,
                requirements,
                original_mot,
                ..
            } = *resolved.role(role)
            else {
                panic!("all four original roles must come from MOT")
            };
            assert_eq!(actual, target.page());
            assert_eq!(
                requirements,
                TeletextMotResolutionRequirements::MotExtent(
                    TeletextMotSubpageExtent::new(0).unwrap()
                )
            );
            let candidate = original_mot.unwrap();
            assert_eq!(candidate.target, target.page());
            assert!(!candidate.supersedable);
            assert!(
                matches!(candidate.provenance,TeletextMotLinkProvenance::Mot {identity,..}
                if identity == mot_identity())
            );
        }
        assert_default_display(service, page);
    }
}

pub(super) fn assert_originals(flow: &Flow) {
    for accepted in &flow.accepted {
        let service = accepted.service();
        assert_defaults(service);
        assert!(service.compiler_drcs_resources().is_empty());
        for identity in authored_identities() {
            let table = service
                .declared()
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
        oracle::previews(service, b'A');
    }
}

fn base_rows(
    service: &EncodedTeletextAuthoredService,
    page: u16,
) -> Vec<TeletextEncodedDisplayRecord> {
    ersatzrs_ffmpeg::teletext::project_encoded_teletext_display(display(service, page).page(), 128)
        .unwrap()
        .records
        .into_iter()
        .filter(|record| matches!(record, TeletextEncodedDisplayRecord::BaseRow { .. }))
        .collect()
}

pub(super) fn assert_effective(flow: &Flow, service: &EncodedTeletextAuthoredService) {
    oracle::assert_service(flow, service);
    assert_defaults(service);
    for page in 124..127 {
        let mut fallback = base_rows(flow.accepted[0].service(), page);
        for row in &mut fallback {
            if let TeletextEncodedDisplayRecord::BaseRow { row: 1, cells } = row {
                assert_eq!(cells[7], b'A');
                cells[7] = 0x5b;
            }
        }
        assert_eq!(base_rows(service, page), fallback);
        let resource = service
            .compiler_drcs_resources()
            .iter()
            .find(|resource| resource.table().identity() == normal_identity(page, 1))
            .unwrap();
        let links = *resource.compositional_links(address(page)).unwrap();
        let actual =
            TeletextMotLevel25Resolution::resolve(&seed::mot(), address(page), Some(links))
                .unwrap();
        let original = flow.accepted[0]
            .service()
            .declared()
            .resolutions()
            .iter()
            .find(|resolution| resolution.display() == address(page))
            .unwrap()
            .mot()
            .unwrap();
        assert_eq!(actual.selected_mot(), original.selected_mot());
        assert_eq!(actual.association(), original.association());
        assert_eq!(actual.mot_links(), original.mot_links());
        assert_eq!(actual.default_sources(), original.default_sources());
        assert_eq!(actual.default_conflicts(), original.default_conflicts());
        for role in TeletextX27LinkRole::ALL {
            let TeletextMotRoleResolution::Selected {
                target,
                original_mot,
                ..
            } = *original.role(role)
            else {
                panic!("all original MOT roles must remain selected")
            };
            let TeletextMotRoleResolution::Selected {
                target: current,
                original_mot: retained,
                requirements,
                ..
            } = *actual.role(role)
            else {
                panic!("effective X27 must retain all selected roles")
            };
            assert_eq!(current, target);
            assert_eq!(retained, original_mot);
            assert_eq!(
                links.link(role).validity(),
                TeletextX27LinkValidity::Level25Only
            );
            let mask = if role == TeletextX27LinkRole::Drcs {
                3
            } else {
                1
            };
            assert_eq!(links.link(role).required_subpages().get(), mask);
            assert_eq!(
                requirements,
                TeletextMotResolutionRequirements::X27Mask(TeletextSubpageRequirementMask::new(
                    mask
                ))
            );
        }
    }
}
