//! Cold producer composition through the complete encoder and shared preview.
//! Warm replacement and independent-client acceptance remain separate gates.

use super::authored_service_tests::{declaration_limits, limits, semantic_limits};
use super::*;
use ersatzrs_domain::*;

fn address(number: u16, subcode: u16) -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(number).unwrap(),
        subcode: TeletextSubcode::new(subcode).unwrap(),
    }
}

fn language() -> TeletextLanguage {
    TeletextLanguage::new("nld".into()).unwrap()
}

fn original(owner: TeletextPageAddress, rgb: [u8; 3], panel_width: u8) -> TeletextDeclaredService {
    let mut presentation =
        super::presentation::western_latin_v1_presentation(TeletextNationalOption::new(1).unwrap())
            .unwrap();
    presentation.palette.clut2[0] = TeletextRgb::new(rgb).unwrap();
    presentation.screen_colour = TeletextColourReference::new(16).unwrap();
    presentation.row_colour = TeletextColourReference::new(16).unwrap();
    presentation.black_background_substitution = true;
    presentation.panels = TeletextSidePanels {
        left: true,
        right: true,
        status: TeletextPanelStatus::Level25And35,
        width: TeletextPanelWidth::new(panel_width).unwrap(),
    };
    let input = TeletextResolvedServiceDraft {
        base: TeletextPageSetDraft {
            language: language(),
            initial_page: owner.page,
            pages: vec![TeletextPageDraft {
                address: owner,
                kind: TeletextPageKind::FullPage,
                language: language(),
                national_option: TeletextNationalOption::new(1).unwrap(),
                rows: vec![TeletextRow {
                    row: 2,
                    runs: vec![TeletextRun {
                        column: 8,
                        style: TeletextStyle::default(),
                        content: TeletextRunContent::Text { text: "B".into() },
                    }],
                }],
                navigation: vec![],
            }],
        },
        pages: vec![TeletextResolvedPageEnhancementDraft {
            address: owner,
            presentation: None,
            compositional_links: None,
            local_program: None,
            resources: TeletextResolvedPageResourceSelection {
                objects: TeletextResolvedObjectPageLinks {
                    public: None,
                    global: None,
                },
                drcs: TeletextDrcsPageLinks::unlinked(),
                defaults: [None; 4],
            },
        }],
        magazines: vec![TeletextMagazinePresentationDraft {
            magazine: 2,
            presentation,
        }],
        objects: vec![],
        drcs: vec![],
    };
    let semantic = TeletextSemanticService::new(input, semantic_limits()).unwrap();
    TeletextDeclaredService::new(
        semantic,
        vec![],
        vec![TeletextDisplayMotSelection {
            display: owner,
            mot: None,
        }],
        vec![],
        declaration_limits(),
    )
    .unwrap()
}

fn producer(declared: TeletextDeclaredService) -> TeletextProducerPresentation {
    TeletextProducerPresentation::new(
        declared,
        TeletextProducerPresentationBudget { max_pages: 8 },
    )
    .unwrap()
}

fn composition_limits() -> TeletextProducerCompositionBudget {
    TeletextProducerCompositionBudget {
        max_producers: 4,
        max_input_items: 4096,
        max_input_content_bytes: 8192,
        max_input_commands: 8192,
        max_storage_bytes: 8 * 1024 * 1024,
        max_work: 1_000_000,
        service: semantic_limits(),
        declarations: declaration_limits(),
    }
}

fn compose(producers: Vec<TeletextProducerPresentation>) -> TeletextColdProducerComposition {
    let entry = producers[0].pages()[0].address();
    TeletextColdProducerComposition::new(
        producers,
        TeletextProducerCompositionMetadata {
            language: language(),
            entry,
        },
        composition_limits(),
    )
    .unwrap()
}

fn raster(composition: &TeletextMainComposition) -> TeletextRaster {
    render_teletext_composition(
        composition,
        TeletextColourPixelOptions {
            reveal_concealed: false,
            flash_phase: TeletextFlashPhase::Original,
        },
        TeletextRasterBudget {
            max_pixels: 216_000,
            max_diagnostics: 0,
        },
    )
    .unwrap()
}

fn packet_numbers(page: &EncodedTeletextAuthoredPage) -> Vec<u8> {
    page.data_units()
        .iter()
        .map(
            |unit| match source_header::observe_dvb_teletext_header(unit).unwrap() {
                source_header::TeletextHeaderObservation::NotHeader(packet) => packet.packet_number,
                _ => 0,
            },
        )
        .collect()
}

#[test]
fn conflicting_magazine_themes_compile_to_isolated_pages_with_unchanged_preview() {
    let inputs = [
        original(address(200, 0x1234), [1, 2, 3], 4),
        original(address(201, 0x0021), [9, 8, 7], 7),
    ];
    let candidate = compose(inputs.iter().cloned().map(producer).collect());
    assert!(candidate
        .composed()
        .semantic()
        .magazine_presentations()
        .is_empty());
    let encoded =
        encode_teletext_authored_service(candidate.composed().clone(), &[], &limits()).unwrap();
    assert!(encoded.magazine_units().is_empty());
    assert_eq!(encoded.pages().len(), 2);
    for (index, (original, page)) in inputs.iter().zip(encoded.pages()).enumerate() {
        assert_eq!(candidate.producers()[index].original(), original);
        let EncodedTeletextAuthoredPage::NativeDisplay(native) = page else {
            panic!("ordinary absent-X26 source must retain the native path");
        };
        let owner = original.semantic().base().pages[0].address;
        assert_eq!(native.encoded().page().address(), owner);
        let numbers = packet_numbers(page);
        assert!(numbers.contains(&28));
        assert!(!numbers.contains(&26));
        assert!(!numbers.contains(&29));
        let original_composition = compose_teletext_main_program(
            original.semantic(),
            owner,
            limits().normalization.composition,
        )
        .unwrap();
        let before = raster(&original_composition);
        let after = raster(native.composition());
        assert_eq!(
            after, before,
            "every original glyph/palette/panel pixel survives"
        );
        assert_eq!((after.width(), after.height()), (672, 250));
    }
    let EncodedTeletextAuthoredPage::NativeDisplay(day) = &encoded.pages()[0] else {
        unreachable!();
    };
    let EncodedTeletextAuthoredPage::NativeDisplay(night) = &encoded.pages()[1] else {
        unreachable!();
    };
    assert_ne!(
        raster(day.composition()).pixels(),
        raster(night.composition()).pixels()
    );
}

#[test]
fn actual_resource_header_collision_is_rejected_after_logical_composition() {
    let display = original(address(200, 0x0320), [1, 2, 3], 4);
    let mut draft = super::authored_service_tests::input();
    draft.base.pages.truncate(1);
    draft.pages.truncate(1);
    let owner = address(201, 0);
    draft.base.initial_page = owner.page;
    draft.base.pages[0].address = owner;
    draft.pages[0].address = owner;
    let resource = TeletextResourceIdentity::new(
        TeletextResourceKind::Pop,
        TeletextRawDataPageNumber::new(2, 0).unwrap(),
        TeletextResourceSubpageIndex::new(0).unwrap(),
    )
    .unwrap();
    draft.objects.truncate(1);
    draft.objects[0].identity =
        TeletextObjectIdentity::public(resource, TeletextObjectId::new(10)).unwrap();
    draft.drcs.clear();
    draft.pages[0].resources.defaults = [None; 4];
    draft.pages[0].resources.objects.public = Some(resource.page());
    let unused = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Unspecified {
            magazine: TeletextX27Magazine::new(2).unwrap(),
        },
        TeletextX27LinkValidity::Level35Only,
        TeletextSubpageRequirementMask::new(0xffff),
    )
    .unwrap();
    let pop = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Page {
            page: resource.page(),
        },
        TeletextX27LinkValidity::Level25Only,
        TeletextSubpageRequirementMask::new(1),
    )
    .unwrap();
    draft.pages[0].compositional_links =
        Some(TeletextX27Level25Links::new(unused, pop, unused, unused).unwrap());
    let semantic = TeletextSemanticService::new(draft, semantic_limits()).unwrap();
    let resources = TeletextDeclaredService::new(
        semantic,
        vec![],
        vec![TeletextDisplayMotSelection {
            display: owner,
            mot: None,
        }],
        vec![],
        declaration_limits(),
    )
    .unwrap();
    let candidate = compose(vec![producer(display), producer(resources)]);
    assert_eq!(candidate.composed().resource_inventory(), &[resource]);
    let tx = [TeletextServiceResourceTransmission {
        resource,
        counter: TeletextResourceUpdateCounter::new(2).unwrap(),
        update: TeletextResourceFullUpdate::Changed,
        last_subpage: true,
    }];
    let error =
        encode_teletext_authored_service(candidate.composed().clone(), &tx, &limits()).unwrap_err();
    assert!(matches!(error.reason,
        TeletextAuthoredServiceEncodingFailure::Inventory(error)
        if *error == TeletextPhysicalPageInventoryError::PhysicalCollision {
            first: 0, second: 2, magazine: 2, page: 0, subcode: 0x0320,
        }
    ));
}
