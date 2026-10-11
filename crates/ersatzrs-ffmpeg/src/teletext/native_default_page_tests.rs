//! Literal packet presence, metadata and fallback checks for native defaults.

use super::native_default_page::*;
use super::*;
use ersatzrs_domain::*;

fn owner() -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(200).unwrap(),
        subcode: TeletextSubcode::new(0x1234).unwrap(),
    }
}
fn row(number: u8, column: u8, text: &str) -> TeletextRow {
    TeletextRow {
        row: number,
        runs: vec![TeletextRun {
            column,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Text { text: text.into() },
        }],
    }
}
fn declaration(code: u8) -> TeletextLevel25PresentationDefaults {
    let mut result =
        super::presentation::western_latin_v1_presentation(TeletextNationalOption::new(0).unwrap())
            .unwrap();
    result.primary = TeletextPageCharsetDesignation::new(code).unwrap();
    result
}
fn limits() -> TeletextServiceBudget {
    TeletextServiceBudget {
        max_records: 32,
        max_objects: 16,
        max_structure_items: 1024,
        max_base_content_bytes: 4096,
        max_commands: 1024,
        max_collected_commands: 2048,
        max_collected_drcs_references: 0,
    }
}
fn draft() -> TeletextResolvedServiceDraft {
    let language = TeletextLanguage::new("nld".into()).unwrap();
    TeletextResolvedServiceDraft {
        base: TeletextPageSetDraft {
            language: language.clone(),
            initial_page: owner().page,
            pages: vec![TeletextPageDraft {
                address: owner(),
                kind: TeletextPageKind::FullPage,
                language,
                national_option: TeletextNationalOption::new(0).unwrap(),
                rows: vec![row(1, 8, "ASCII")],
                navigation: vec![],
            }],
        },
        pages: vec![TeletextResolvedPageEnhancementDraft {
            address: owner(),
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
        magazines: vec![],
        objects: vec![],
        drcs: vec![],
    }
}
fn service(draft: TeletextResolvedServiceDraft) -> TeletextSemanticService {
    TeletextSemanticService::new(draft, limits()).unwrap()
}
fn encode(service: &TeletextSemanticService) -> EncodedTeletextNativeDefaultPage {
    encode_teletext_native_default_page(service, owner(), TeletextEncodingRegion::WesternLatin)
        .unwrap()
}

const HAM: [u8; 16] = [
    0x15, 0x02, 0x49, 0x5e, 0x64, 0x73, 0x38, 0x2f, 0xd0, 0xc7, 0x8c, 0x9b, 0xa1, 0xb6, 0xfd, 0xea,
];
fn nibble(byte: u8) -> u8 {
    u8::try_from(HAM.iter().position(|&value| value == byte).unwrap()).unwrap()
}
fn packet(unit: &[u8; 46]) -> u8 {
    (nibble(unit[4].reverse_bits()) | (nibble(unit[5].reverse_bits()) << 4)) >> 3
}
fn body(unit: &[u8; 46]) -> [u8; 40] {
    std::array::from_fn(|index| unit[index + 6].reverse_bits())
}
fn packets(page: &EncodedTeletextPage) -> Vec<u8> {
    page.data_units().iter().map(packet).collect()
}
// Fixed Hamming positions independently read the body data, not the bit writer.
fn words(body: &[u8; 40]) -> [u32; 13] {
    std::array::from_fn(|index| {
        let start = 1 + 3 * index;
        let raw = u32::from_le_bytes([body[start], body[start + 1], body[start + 2], 0]);
        assert_eq!(raw.count_ones() % 2, 1);
        for parity in [1_u32, 2, 4, 8, 16] {
            let ones: u32 = (1_u32..24)
                .filter(|position| position & parity != 0)
                .map(|position| (raw >> (position - 1)) & 1)
                .sum();
            assert_eq!(ones % 2, 1);
        }
        [
            3, 5, 6, 7, 9, 10, 11, 12, 13, 14, 15, 17, 18, 19, 20, 21, 22, 23,
        ]
        .into_iter()
        .enumerate()
        .fold(0, |value, (bit, position)| {
            value | (((raw >> (position - 1)) & 1) << bit)
        })
    })
}
fn field(words: &[u32; 13], offset: usize, width: usize) -> u32 {
    (0..width).fold(0, |value, bit| {
        value | (((words[(offset + bit) / 18] >> ((offset + bit) % 18)) & 1) << bit)
    })
}

#[test]
fn absent_page_metadata_keeps_header_only_presentation_and_all_fallback_rows() {
    let mut input = draft();
    input.pages.clear();
    let service = service(input);
    let retained = service.clone();
    let encoded = encode(&service);
    assert_eq!(packets(encoded.page()), (0_u8..25).collect::<Vec<_>>());
    assert!(encoded.magazine_units().is_empty());
    assert_eq!(encoded.defaults(), &[None; 4]);
    assert!(
        matches!(service.effective_presentation(owner()),Some(TeletextEffectivePresentation::Header {national_option}) if national_option.get()==0)
    );
    let header = body(&encoded.page().data_units()[0]);
    assert_eq!(
        header[..8].iter().copied().map(nibble).collect::<Vec<_>>(),
        [0, 0, 4, 11, 2, 1, 0, 1]
    );
    let first = body(&encoded.page().data_units()[1]);
    assert_eq!(&first[8..13], &[0xc1, 0xd3, 0x43, 0x49, 0x49]);
    assert!(encoded.page().data_units()[2..]
        .iter()
        .all(|unit| body(unit) == [0x20; 40]));
    assert_eq!(encoded.page().address(), owner());
    assert_eq!(encoded.page().kind(), TeletextPageKind::FullPage);
    assert_eq!(encoded.page().language(), &service.base().language);
    assert_eq!(service, retained);
}

#[test]
fn exact_owner_and_present_empty_local_program_reject_before_layout_output() {
    let original = service(draft());
    let mut other = owner();
    other.subcode = TeletextSubcode::new(0x1235).unwrap();
    assert_eq!(
        encode_teletext_native_default_page(&original, other, TeletextEncodingRegion::WesternLatin)
            .unwrap_err(),
        TeletextNativeDefaultPageEncodingError {
            owner: other,
            reason: TeletextNativeDefaultPageEncodingFailure::MissingPage
        }
    );
    for commands in [
        vec![],
        vec![TeletextEnhancementCommand::SetPosition {
            position: TeletextEnhancementPosition::new(0, 0).unwrap(),
        }],
    ] {
        let mut input = draft();
        input.base.pages[0].rows = vec![row(1, 8, "é")];
        input.pages[0].local_program = Some(TeletextEnhancementProgram::new(commands).unwrap());
        let service = service(input);
        let retained = service.clone();
        assert_eq!(
            encode_teletext_native_default_page(
                &service,
                owner(),
                TeletextEncodingRegion::WesternLatin
            )
            .unwrap_err()
            .reason,
            TeletextNativeDefaultPageEncodingFailure::LocalProgramPresent
        );
        assert_eq!(service, retained);
    }
}

#[test]
fn first_required_derived_glyph_is_located_but_direct_english_pound_needs_no_x26() {
    let mut input = draft();
    input.base.pages[0].national_option = TeletextNationalOption::new(1).unwrap();
    input.base.pages[0].rows = vec![row(3, 8, "é"), row(1, 8, "£")];
    let original = service(input);
    let retained = original.clone();
    assert_eq!(
        encode_teletext_native_default_page(
            &original,
            owner(),
            TeletextEncodingRegion::WesternLatin
        )
        .unwrap_err()
        .reason,
        TeletextNativeDefaultPageEncodingFailure::DerivedGlyphNeedsX26 {
            position: TeletextEnhancementPosition::new(1, 8).unwrap()
        }
    );
    assert_eq!(original, retained);
    let mut input = draft();
    input.base.pages[0].rows = vec![row(1, 8, "£")];
    let encoded = encode(&service(input));
    assert_eq!(body(&encoded.page().data_units()[1])[8], 0x23);
    assert!(!packets(encoded.page()).contains(&26));
}

#[test]
fn legacy_v1_units_are_exact_native_units_plus_original_terminating_x26() {
    for national in 0_u8..7 {
        let mut input = draft();
        input.base.pages[0].national_option = TeletextNationalOption::new(national).unwrap();
        input.pages[0].presentation = Some(declaration(national));
        let service = service(input);
        let legacy = encode_teletext_page(
            &service.base().pages[0],
            TeletextEncodingRegion::WesternLatin,
        )
        .unwrap();
        let native = encode(&service);
        let without_x26: Vec<_> = legacy
            .data_units()
            .iter()
            .filter(|unit| packet(unit) != 26)
            .copied()
            .collect();
        assert_eq!(native.page().data_units(), without_x26);
        assert_eq!(legacy.data_units().len(), 27);
        let x26 = legacy
            .data_units()
            .iter()
            .find(|unit| packet(unit) == 26)
            .unwrap();
        assert_eq!(words(&body(x26)), [63 | (31 << 6); 13]);
        let declaration = body(&legacy.data_units()[1]);
        assert_eq!(declaration[0], 0x15);
        assert_eq!(
            &declaration[1..4],
            &[
                [0x8b, 0x80, 0x78],
                [0x03, 0x88, 0xf8],
                [0x02, 0x90, 0x78],
                [0x8a, 0x98, 0xf8],
                [0x01, 0xa0, 0x78],
                [0x89, 0xa8, 0xf8],
                [0x88, 0xb0, 0x78]
            ][usize::from(national)]
        );
    }
}

#[test]
fn magazine_declaration_is_separate_and_never_becomes_page_override() {
    let mut input = draft();
    let mut magazine = declaration(0x37);
    magazine.screen_colour = TeletextColourReference::new(13).unwrap();
    magazine.row_colour = TeletextColourReference::new(18).unwrap();
    input.magazines.push(TeletextMagazinePresentationDraft {
        magazine: 2,
        presentation: magazine,
    });
    let service = service(input);
    let encoded = encode(&service);
    assert_eq!(packets(encoded.page()), (0_u8..25).collect::<Vec<_>>());
    assert_eq!(encoded.magazine_units().len(), 1);
    assert_eq!(packet(&encoded.magazine_units()[0]), 29);
    assert_eq!(nibble(encoded.magazine_units()[0][4].reverse_bits()) & 7, 2);
    let bits = words(&body(&encoded.magazine_units()[0]));
    assert_eq!(
        (
            field(&bits, 7, 7),
            field(&bits, 220, 5),
            field(&bits, 225, 5)
        ),
        (0x37, 13, 18)
    );
    assert!(matches!(
        service.effective_presentation(owner()),
        Some(TeletextEffectivePresentation::Extension {
            source: TeletextPresentationSource::Magazine,
            ..
        })
    ));
}

#[test]
fn custom_page_and_magazine_palettes_preserve_both_declarations_and_page_priority() {
    let mut input = draft();
    let mut page = declaration(0x24);
    page.secondary = Some(TeletextPageCharsetDesignation::new(0x37).unwrap());
    page.palette.clut2[0] = TeletextRgb::new([1, 2, 3]).unwrap();
    page.palette.clut3[7] = TeletextRgb::new([4, 5, 6]).unwrap();
    page.remapping = TeletextColourRemapping::FgClut2BgClut3;
    page.screen_colour = TeletextColourReference::new(5).unwrap();
    page.row_colour = TeletextColourReference::new(27).unwrap();
    page.black_background_substitution = true;
    page.panels = TeletextSidePanels {
        left: true,
        right: true,
        status: TeletextPanelStatus::Level25And35,
        width: TeletextPanelWidth::new(4).unwrap(),
    };
    let magazine = declaration(0);
    input.pages[0].presentation = Some(page);
    input.magazines.push(TeletextMagazinePresentationDraft {
        magazine: 2,
        presentation: magazine,
    });
    let service = service(input);
    let retained = service.clone();
    let encoded = encode(&service);
    assert_eq!(packet(&encoded.page().data_units()[1]), 28);
    let bits = words(&body(&encoded.page().data_units()[1]));
    assert_eq!(field(&bits, 7, 7), 0x24);
    assert_eq!(field(&bits, 14, 7), 0x37);
    assert_eq!(field(&bits, 21, 7), 39); // L/R/status set, raw width4.
    assert_eq!(field(&bits, 28, 12), 0x321);
    assert_eq!(field(&bits, 208, 12), 0x654);
    assert_eq!(field(&bits, 220, 14), 27 << 5 | 5 | 1 << 10 | 7 << 11);
    let magazine_bits = words(&body(&encoded.magazine_units()[0]));
    assert_eq!(field(&magazine_bits, 7, 7), 0);
    assert!(
        matches!(service.effective_presentation(owner()),Some(TeletextEffectivePresentation::Extension {source:TeletextPresentationSource::Page,defaults}) if defaults==&page)
    );
    assert_eq!(service, retained);
}

fn links(page: TeletextRawDataPageNumber) -> TeletextX27Level25Links {
    let null = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Unspecified {
            magazine: TeletextX27Magazine::new(8).unwrap(),
        },
        TeletextX27LinkValidity::Level35Only,
        TeletextSubpageRequirementMask::new(0xffff),
    )
    .unwrap();
    let pop = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Page { page },
        TeletextX27LinkValidity::Level25And35,
        TeletextSubpageRequirementMask::new(0),
    )
    .unwrap();
    TeletextX27Level25Links::new(null, pop, null, null).unwrap()
}

#[test]
fn native_defaults_retain_original_a0_owner_while_x27_selects_a1() {
    let mut input = draft();
    let resource = TeletextResourceIdentity::new(
        TeletextResourceKind::Pop,
        TeletextRawDataPageNumber::new(2, 0xa0).unwrap(),
        TeletextResourceSubpageIndex::new(15).unwrap(),
    )
    .unwrap();
    let target = TeletextObjectIdentity::public(resource, TeletextObjectId::new(65535)).unwrap();
    input.objects.push(TeletextEnhancementObjectDefinition {
        identity: target,
        object_type: TeletextObjectType::Adaptive,
        program: TeletextEnhancementProgram::new(vec![]).unwrap(),
    });
    let selection = TeletextDefaultObjectSelection {
        target,
        object_type: TeletextObjectType::Adaptive,
    };
    input.pages[0].resources.defaults[2] = Some(selection);
    let selected = TeletextRawDataPageNumber::new(2, 0xa1).unwrap();
    // A real selected resource must also exist; its presence never rebinds A0.
    let selected_resource = TeletextResourceIdentity::new(
        TeletextResourceKind::Pop,
        selected,
        TeletextResourceSubpageIndex::new(0).unwrap(),
    )
    .unwrap();
    input.objects.push(TeletextEnhancementObjectDefinition {
        identity: TeletextObjectIdentity::public(selected_resource, TeletextObjectId::new(65535))
            .unwrap(),
        object_type: TeletextObjectType::Adaptive,
        program: TeletextEnhancementProgram::new(vec![]).unwrap(),
    });
    input.pages[0].resources.objects.public = Some(selected);
    input.pages[0].compositional_links = Some(links(selected));
    let service = service(input);
    let retained = service.clone();
    let encoded = encode(&service);
    assert_eq!(encoded.defaults(), &[None, None, Some(selection), None]);
    assert_eq!(
        encoded.defaults()[2].unwrap().target.resource(),
        Some(resource)
    );
    assert_eq!(packet(&encoded.page().data_units()[1]), 27);
    let link_body = body(&encoded.page().data_units()[1]);
    assert_eq!(nibble(link_body[0]), 4);
    let link_words = words(&link_body);
    assert_eq!(link_words[2], 1 | 0x0c | (1 << 6) | (1 << 10) | (10 << 14));
    assert_eq!(link_words[3], 0);
    assert!(!packets(encoded.page()).contains(&26));
    assert_eq!(service, retained);
    drop(service);
    assert_eq!(encoded.defaults()[2], Some(selection));
    assert_eq!(encoded.page().address(), owner());
}

#[test]
fn header_editorial_links_and_full_fallback_rows_keep_kind_and_language() {
    for kind in [
        TeletextPageKind::FullPage,
        TeletextPageKind::Newsflash,
        TeletextPageKind::Subtitle,
    ] {
        let mut input = draft();
        input.base.pages[0].kind = kind;
        input.base.pages[0].rows[0].runs[0].style.boxed = kind != TeletextPageKind::FullPage;
        input.base.pages[0].navigation.push(TeletextNavigationLink {
            role: TeletextNavigationRole::Red,
            target: TeletextNavigationTarget {
                page: owner().page,
                subcode: Some(owner().subcode),
            },
        });
        let service = service(input);
        let encoded = encode(&service);
        let legacy = encode_teletext_page(
            &service.base().pages[0],
            TeletextEncodingRegion::WesternLatin,
        )
        .unwrap();
        let core: Vec<_> = legacy
            .data_units()
            .iter()
            .filter(|unit| !matches!(packet(unit), 26 | 28))
            .copied()
            .collect();
        assert_eq!(encoded.page().data_units(), core);
        assert_eq!(&packets(encoded.page())[..3], &[0, 27, 1]);
        assert_eq!(encoded.page().data_units().len(), 26);
        assert!(encoded.page().data_units().iter().all(|unit| unit[0]
            == if kind == TeletextPageKind::Subtitle {
                3
            } else {
                2
            }));
        let header = body(&encoded.page().data_units()[0]);
        assert_eq!(
            nibble(header[5]),
            1 | match kind {
                TeletextPageKind::FullPage => 0,
                TeletextPageKind::Newsflash => 4,
                TeletextPageKind::Subtitle => 8,
            }
        );
        assert_eq!(
            nibble(header[6]),
            u8::from(kind != TeletextPageKind::FullPage)
        );
        let navigation = body(&encoded.page().data_units()[1]);
        assert_eq!(nibble(navigation[0]), 0);
        assert_eq!(nibble(navigation[37]), 15);
        assert_eq!(encoded.page().kind(), kind);
    }
}
