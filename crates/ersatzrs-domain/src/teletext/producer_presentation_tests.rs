//! Producer-local complete defaults and unchanged original declaration proofs.
use super::*;
use crate::teletext::*;

fn address(subcode: u16) -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(200).unwrap(),
        subcode: TeletextSubcode::new(subcode).unwrap(),
    }
}
fn resource(kind: TeletextResourceKind, byte: u8, s1: u8) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        kind,
        TeletextRawDataPageNumber::new(2, byte).unwrap(),
        TeletextResourceSubpageIndex::new(s1).unwrap(),
    )
    .unwrap()
}
fn base(subcodes: &[u16], national: u8) -> TeletextPageSetDraft {
    let language = TeletextLanguage::new("nld".into()).unwrap();
    TeletextPageSetDraft {
        language: language.clone(),
        initial_page: address(0).page,
        pages: subcodes
            .iter()
            .map(|&subcode| TeletextPageDraft {
                address: address(subcode),
                kind: TeletextPageKind::FullPage,
                language: language.clone(),
                national_option: TeletextNationalOption::new(national).unwrap(),
                rows: vec![],
                navigation: vec![],
            })
            .collect(),
    }
}
fn unlinked() -> TeletextResolvedPageResourceSelection {
    TeletextResolvedPageResourceSelection {
        objects: TeletextResolvedObjectPageLinks {
            public: None,
            global: None,
        },
        drcs: TeletextDrcsPageLinks::unlinked(),
        defaults: [None; 4],
    }
}
fn metadata(subcode: u16) -> TeletextResolvedPageEnhancementDraft {
    TeletextResolvedPageEnhancementDraft {
        address: address(subcode),
        presentation: None,
        compositional_links: None,
        local_program: None,
        resources: unlinked(),
    }
}
fn draft(subcodes: &[u16], national: u8) -> TeletextResolvedServiceDraft {
    TeletextResolvedServiceDraft {
        base: base(subcodes, national),
        pages: vec![],
        magazines: vec![],
        objects: vec![],
        drcs: vec![],
    }
}
fn declared(
    input: TeletextResolvedServiceDraft,
    mots: Vec<TeletextMotLevel25Declaration>,
) -> TeletextDeclaredService {
    let semantic = TeletextSemanticService::new(
        input,
        TeletextServiceBudget {
            max_records: 64,
            max_objects: 64,
            max_structure_items: 4096,
            max_base_content_bytes: 4096,
            max_commands: 4096,
            max_collected_commands: 4096,
            max_collected_drcs_references: 64,
        },
    )
    .unwrap();
    let selections = semantic
        .base()
        .pages
        .iter()
        .map(|page| TeletextDisplayMotSelection {
            display: page.address,
            mot: mots.first().map(TeletextMotLevel25Declaration::identity),
        })
        .collect();
    TeletextDeclaredService::new(
        semantic,
        mots,
        selections,
        vec![],
        TeletextDeclaredServiceBudget {
            max_mots: 8,
            max_displays: 64,
            max_resources: 64,
            max_defaults: 64,
            max_objects: 64,
        },
    )
    .unwrap()
}
fn snapshot(original: TeletextDeclaredService) -> TeletextProducerPresentation {
    TeletextProducerPresentation::new(
        original,
        TeletextProducerPresentationBudget { max_pages: 64 },
    )
    .unwrap()
}
fn theme(
    primary: u8,
    secondary: Option<u8>,
    rgb: [u8; 3],
    width: u8,
) -> TeletextLevel25PresentationDefaults {
    TeletextLevel25PresentationDefaults {
        primary: TeletextPageCharsetDesignation::new(primary).unwrap(),
        secondary: secondary.map(|code| TeletextPageCharsetDesignation::new(code).unwrap()),
        palette: TeletextLevel25Palette {
            clut2: [TeletextRgb::new(rgb).unwrap(); 8],
            clut3: [TeletextRgb::new([3, 5, 7]).unwrap(); 8],
        },
        screen_colour: TeletextColourReference::new(18).unwrap(),
        row_colour: TeletextColourReference::new(27).unwrap(),
        black_background_substitution: true,
        remapping: TeletextColourRemapping::FgClut2BgClut3,
        panels: TeletextSidePanels {
            left: true,
            right: true,
            status: TeletextPanelStatus::Level25And35,
            width: TeletextPanelWidth::new(width).unwrap(),
        },
    }
}

#[test]
fn two_producers_snapshot_distinct_full_magazine_themes_before_composition() {
    let day = theme(0x20, Some(1), [15, 0, 0], 4);
    let night = theme(0x37, None, [0, 0, 15], 9);
    for (defaults, pair, widths) in [
        (day, TeletextCharsetPair::Cyrillic1Cyrillic, (4, 12)),
        (night, TeletextCharsetPair::GreekGreek, (9, 7)),
    ] {
        let mut input = draft(&[0], 1);
        input.magazines.push(TeletextMagazinePresentationDraft {
            magazine: 2,
            presentation: defaults,
        });
        let original = declared(input, vec![]);
        let expected = original.clone();
        let base_storage = original.semantic().base().pages.as_ptr();
        let view = snapshot(original);
        let page = view.page(address(0)).unwrap();
        assert_eq!(
            page.source(),
            TeletextProducerPresentationSource::Magazine { magazine: 2 }
        );
        assert_eq!(page.effective_defaults(), &defaults);
        assert_eq!(page.effective_defaults().primary.pair(), pair);
        assert_eq!(page.effective_defaults().panels.level25_widths(), widths);
        assert_eq!(
            page.secondary_selection(),
            TeletextProducerSecondarySelection::Declared(defaults.secondary)
        );
        assert_eq!(
            page.local_program_presence(),
            TeletextProducerLocalProgramPresence::Absent
        );
        assert_eq!(view.original(), &expected);
        assert_eq!(
            view.original().semantic().base().pages.as_ptr(),
            base_storage
        );
        assert!(view.original().semantic().page_enhancements().is_empty());
        assert_eq!(view.into_original(), expected);
    }
}

#[test]
fn exact_subcodes_keep_page_precedence_and_all_three_local_presence_states() {
    let magazine = theme(0x20, Some(1), [15, 0, 0], 4);
    let page_theme = theme(0x37, None, [0, 0, 15], 9);
    let mut input = draft(&[0, 16, 32], 1);
    input.magazines.push(TeletextMagazinePresentationDraft {
        magazine: 2,
        presentation: magazine,
    });
    let mut page = metadata(16);
    page.presentation = Some(page_theme);
    page.local_program = Some(TeletextEnhancementProgram::new(vec![]).unwrap());
    input.pages.push(page);
    let mut nonempty = metadata(32);
    nonempty.local_program = Some(
        TeletextEnhancementProgram::new(vec![TeletextEnhancementCommand::SetScreenColour {
            colour: TeletextColourReference::BLACK,
        }])
        .unwrap(),
    );
    input.pages.push(nonempty);
    let original = declared(input, vec![]);
    let expected = original.clone();
    let view = snapshot(original);
    assert_eq!(
        view.pages()
            .iter()
            .map(TeletextProducerPagePresentation::address)
            .collect::<Vec<_>>(),
        [address(0), address(16), address(32)]
    );
    assert_eq!(
        view.page(address(0)).unwrap().effective_defaults(),
        &magazine
    );
    let page = view.page(address(16)).unwrap();
    assert_eq!(page.source(), TeletextProducerPresentationSource::Page);
    assert_eq!(page.effective_defaults(), &page_theme);
    assert_eq!(
        page.secondary_selection(),
        TeletextProducerSecondarySelection::Declared(None)
    );
    assert_eq!(
        page.local_program_presence(),
        TeletextProducerLocalProgramPresence::PresentEmpty
    );
    assert_eq!(
        view.page(address(32)).unwrap().local_program_presence(),
        TeletextProducerLocalProgramPresence::PresentNonEmpty
    );
    assert!(view.page(address(1)).is_none());
    assert_eq!(view.original(), &expected);
}

#[test]
fn header_snapshots_preserve_national_defaults_without_guessing_second_g0_or_x28() {
    for national in 0..7 {
        let view = snapshot(declared(draft(&[0], national), vec![]));
        let page = view.page(address(0)).unwrap();
        assert_eq!(
            page.source(),
            TeletextProducerPresentationSource::Header {
                national_option: TeletextNationalOption::new(national).unwrap()
            }
        );
        let defaults = page.effective_defaults();
        assert_eq!(defaults.primary.get(), national);
        assert_eq!(defaults.primary.pair(), TeletextCharsetPair::LatinLatin);
        assert_eq!(defaults.palette, TeletextLevel25Palette::standard());
        assert_eq!(
            (defaults.screen_colour.get(), defaults.row_colour.get()),
            (0, 0)
        );
        assert!(!defaults.black_background_substitution);
        assert_eq!(defaults.remapping, TeletextColourRemapping::FgClut0BgClut0);
        assert_eq!(defaults.panels.level25_widths(), (0, 0));
        assert_eq!(
            page.secondary_selection(),
            TeletextProducerSecondarySelection::DecoderDependent
        );
        assert_eq!(defaults.secondary, None); // Current policy is explicitly not a declaration.
        assert!(view.original().semantic().page_enhancements().is_empty());
        assert!(view
            .original()
            .semantic()
            .magazine_presentations()
            .is_empty());
    }
}

fn pop_original(flags: u8) -> TeletextDeclaredService {
    let pop = resource(TeletextResourceKind::Pop, 0xa0, 0);
    let unused = TeletextX27LinkTarget::Unspecified {
        magazine: TeletextX27Magazine::new(2).unwrap(),
    };
    let empty = TeletextMotObjectLink::new(
        unused,
        TeletextMotSubpageExtent::new(0).unwrap(),
        true,
        TeletextMotFallback::new(0).unwrap(),
        [TeletextMotDefaultObject::NoObject { raw_number: 213 }; 2],
    );
    let mut public = [empty; 7];
    public[0] = TeletextMotObjectLink::new(
        TeletextX27LinkTarget::Page { page: pop.page() },
        TeletextMotSubpageExtent::new(0).unwrap(),
        true,
        TeletextMotFallback::new(flags).unwrap(),
        [TeletextMotDefaultObject::NoObject { raw_number: 77 }; 2],
    );
    let drcs_link =
        TeletextMotDrcsLink::new(unused, TeletextMotSubpageExtent::new(0).unwrap(), true);
    let mot = TeletextMotLevel25Declaration::new(
        resource(TeletextResourceKind::Mot, 0xfe, 9),
        vec![TeletextMotPageAssociation::new(
            0,
            TeletextMotAssociationSelection::new(
                false,
                Some(TeletextMotPublicLinkIndex::new(1).unwrap()),
            ),
            TeletextMotAssociationSelection::new(false, None),
        )],
        TeletextMotLevel25Links::new(
            empty,
            public,
            drcs_link,
            [drcs_link; 7],
            TeletextMotEnhancementCounts::new(0, 0, 0),
        )
        .unwrap(),
    )
    .unwrap();
    let mut input = draft(&[0], 1);
    let mut page = metadata(0);
    page.resources.objects.public = Some(pop.page());
    input.pages.push(page);
    input.objects.push(TeletextEnhancementObjectDefinition {
        identity: TeletextObjectIdentity::public(pop, TeletextObjectId::new(0xffff)).unwrap(),
        object_type: TeletextObjectType::Passive,
        program: TeletextEnhancementProgram::new(vec![]).unwrap(),
    });
    input.drcs.push(
        TeletextDrcsSubtable::new(
            resource(TeletextResourceKind::Drcs, 0xc0, 3),
            vec![TeletextDrcsGlyphDefinition {
                slot: TeletextDrcsGlyphSlot::new(7).unwrap(),
                bitmap: TeletextDrcsBitmap::new([0xaaa; 10]).unwrap(),
            }],
        )
        .unwrap(),
    );
    declared(input, vec![mot])
}

#[test]
fn original_pop_flags_objects_drcs_and_nonzero_mot_bank_are_preserved() {
    for (flags, widths, bbs) in [(1, (0, 0), false), (14, (8, 8), true)] {
        let original = pop_original(flags);
        let expected = original.clone();
        let view = snapshot(original);
        let page = view.page(address(0)).unwrap();
        let TeletextProducerPresentationSource::MotFallback {
            national_option,
            original_pop,
        } = page.source()
        else {
            panic!("expected original POP fallback")
        };
        assert_eq!(national_option.get(), 1);
        assert_eq!(original_pop.flags.get(), flags);
        assert_eq!(
            original_pop,
            expected.resolutions()[0]
                .conditional_pop_fallback()
                .unwrap()
        );
        assert_eq!(page.effective_defaults().panels.level25_widths(), widths);
        assert_eq!(page.effective_defaults().black_background_substitution, bbs);
        assert_eq!(page.effective_defaults().primary.get(), 1);
        assert_eq!(
            page.effective_defaults().palette,
            TeletextLevel25Palette::standard()
        );
        assert_eq!(
            page.secondary_selection(),
            TeletextProducerSecondarySelection::DecoderDependent
        );
        assert_eq!(
            view.original().mots()[0].identity().logical_subpage().get(),
            9
        );
        assert_eq!(view.original().semantic().objects().definitions().len(), 1);
        assert_eq!(
            view.original().semantic().drcs().subtables()[0].glyphs()[0]
                .bitmap
                .get(),
            [0xaaa; 10]
        );
        assert_eq!(view.original(), &expected);
        assert!(view.original().semantic().page_enhancements()[&address(0)]
            .presentation
            .is_none());
    }
}

#[test]
fn snapshot_budget_zero_cutoff_and_exact_capacity_are_checked() {
    for maximum in [0, 1] {
        assert!(
            matches!(TeletextProducerPresentation::new(declared(draft(&[0, 16], 1), vec![]), TeletextProducerPresentationBudget { max_pages: maximum }),
            Err(TeletextProducerPresentationError::PageBudget { count: 2, maximum: actual }) if actual == maximum)
        );
    }
    let view = TeletextProducerPresentation::new(
        declared(draft(&[0, 16], 1), vec![]),
        TeletextProducerPresentationBudget { max_pages: 2 },
    )
    .unwrap();
    assert_eq!(view.pages().len(), 2);
}

#[test]
fn presentation_view_debug_does_not_expose_original_document() {
    let view = snapshot(pop_original(14));
    let debug = format!("{view:?}");
    assert!(debug.contains("pages: 1"));
    assert!(!debug.contains("raw_number"));
    assert!(!debug.contains("2730"));
}
