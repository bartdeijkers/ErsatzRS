//! Compatibility intent and budgets; not enhanced compilation or client proof.

use ersatzrs_domain::*;

use super::*;

fn base() -> TeletextPageSetDraft {
    let language = TeletextLanguage::new("nld".into()).unwrap();
    let page = TeletextPageNumber::new(200).unwrap();
    TeletextPageSetDraft {
        language: language.clone(),
        initial_page: page,
        pages: (0..7)
            .map(|option| TeletextPageDraft {
                address: TeletextPageAddress {
                    page,
                    subcode: TeletextSubcode::new(u16::from(option)).unwrap(),
                },
                kind: TeletextPageKind::FullPage,
                language: language.clone(),
                national_option: TeletextNationalOption::new(option).unwrap(),
                rows: Vec::new(),
                navigation: Vec::new(),
            })
            .collect(),
    }
}

fn budget() -> TeletextServiceBudget {
    TeletextServiceBudget {
        max_records: 1024,
        max_objects: 0,
        max_structure_items: 4096,
        max_base_content_bytes: 4096,
        max_commands: 0,
        max_collected_commands: 0,
        max_collected_drcs_references: 0,
    }
}

fn lift(base: TeletextPageSetDraft, budget: TeletextServiceBudget) -> TeletextSemanticService {
    lift_teletext_v1_page_set(base, TeletextEncodingRegion::WesternLatin, budget).unwrap()
}

#[test]
fn every_v1_subpage_has_explicit_presentation_and_empty_local_presence() {
    let base = base();
    let service = lift(base.clone(), budget());
    assert_eq!(service.base(), &base);
    assert_eq!(service.page_enhancements().len(), 7);
    assert!(service.magazine_presentations().is_empty());
    assert!(service.objects().definitions().is_empty());
    for page in &base.pages {
        let metadata = &service.page_enhancements()[&page.address];
        assert_eq!(metadata.compositional_links, None);
        let presentation = metadata.presentation.unwrap();
        assert_eq!(presentation.primary.get(), page.national_option.get());
        assert_eq!(presentation.secondary, None);
        assert_eq!(presentation.screen_colour.get(), 0);
        assert_eq!(presentation.row_colour.get(), 0);
        assert!(!presentation.black_background_substitution);
        assert_eq!(
            presentation.remapping,
            TeletextColourRemapping::FgClut0BgClut0
        );
        assert_eq!(presentation.panels.status, TeletextPanelStatus::Level35Only);
        assert_eq!(presentation.panels.level25_widths(), (0, 0));
        assert!(metadata
            .local_program
            .as_ref()
            .unwrap()
            .commands()
            .is_empty());
        assert_eq!(metadata.resources.objects.public, None);
        assert_eq!(metadata.resources.objects.global, None);
        assert_eq!(
            metadata.resources.drcs.page(TeletextDrcsTableScope::Normal),
            None
        );
        assert_eq!(
            metadata.resources.drcs.page(TeletextDrcsTableScope::Global),
            None
        );
        assert_eq!(metadata.resources.defaults, [None; 4]);
        let (source, references) = service.collected(page.address).unwrap();
        assert_eq!(source, &TeletextEffectiveProgramSource::Local);
        assert_eq!(references.command_count(), 0);
        assert!(references.object_references().is_empty());
        assert!(references.references().is_empty());
    }
}

#[test]
fn base_content_styles_navigation_and_encoder_character_triplets_are_preserved() {
    let mut base = base();
    base.pages[0].rows.push(TeletextRow {
        row: 1,
        runs: vec![TeletextRun {
            column: 1,
            style: TeletextStyle {
                foreground: TeletextColour::Cyan,
                ..TeletextStyle::default()
            },
            content: TeletextRunContent::Text {
                text: "ASCII @ é ©".into(),
            },
        }],
    });
    let target_subcode = base.pages[1].address.subcode;
    base.pages[0].navigation.push(TeletextNavigationLink {
        role: TeletextNavigationRole::Red,
        target: TeletextNavigationTarget {
            page: base.initial_page,
            subcode: Some(target_subcode),
        },
    });
    let encoded = encode_teletext_page_set(&base, TeletextEncodingRegion::WesternLatin).unwrap();
    let service = lift(base.clone(), budget());
    assert_eq!(service.base(), &base);
    let after =
        encode_teletext_page_set(service.base(), TeletextEncodingRegion::WesternLatin).unwrap();
    assert_eq!(after, encoded);
    let first = &after.pages()[0];
    let packet_address = first.address().page.wire_magazine() | (26 << 3);
    let x26 = first
        .data_units()
        .iter()
        .find(|unit| {
            unit[4] == packets::hamming_8_4(packet_address & 15).reverse_bits()
                && unit[5] == packets::hamming_8_4(packet_address >> 4).reverse_bits()
        })
        .unwrap();
    let first_triplet = [x26[7], x26[8], x26[9]].map(u8::reverse_bits);
    assert_ne!(
        first_triplet,
        packets::hamming_24_18(layout::triplet(63, 31, 0))
    );
}

#[test]
fn exact_budget_preflight_matches_full_service_and_rejects_each_one_short() {
    let mut base = base();
    base.pages[0].rows.push(TeletextRow {
        row: 1,
        runs: vec![TeletextRun {
            column: 0,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Text { text: "é".into() },
        }],
    });
    base.pages[1].rows.push(TeletextRow {
        row: 2,
        runs: vec![TeletextRun {
            column: 0,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Mosaics {
                cells: vec![TeletextMosaic::new(1).unwrap()],
                separated: true,
            },
        }],
    });
    base.pages[0].navigation.push(TeletextNavigationLink {
        role: TeletextNavigationRole::Index,
        target: TeletextNavigationTarget {
            page: base.initial_page,
            subcode: None,
        },
    });
    let exact = TeletextServiceBudget {
        max_records: 7,
        max_structure_items: 19, // Seven pages, seven metadata, two rows/runs, one link.
        max_base_content_bytes: 3, // UTF-8 bytes plus one mosaic cell.
        ..budget()
    };
    assert_eq!(exact.validate_base_with_page_metadata(&base), Ok(()));
    assert_eq!(lift(base.clone(), exact).base(), &base);
    for (dimension, maximum, short) in [
        (
            "record",
            6,
            TeletextServiceBudget {
                max_records: 6,
                ..exact
            },
        ),
        (
            "structure item",
            18,
            TeletextServiceBudget {
                max_structure_items: 18,
                ..exact
            },
        ),
        (
            "base content byte",
            2,
            TeletextServiceBudget {
                max_base_content_bytes: 2,
                ..exact
            },
        ),
    ] {
        let expected = TeletextServiceError::Budget { dimension, maximum };
        assert_eq!(
            short.validate_base_with_page_metadata(&base),
            Err(expected.clone())
        );
        assert_eq!(
            lift_teletext_v1_page_set(base.clone(), TeletextEncodingRegion::WesternLatin, short),
            Err(expected)
        );
    }
}

#[test]
fn budgets_precede_structure_and_structure_precedes_reserved_national_option() {
    let mut base = base();
    base.pages[0].national_option = TeletextNationalOption::new(7).unwrap();
    base.pages[0].rows.push(TeletextRow {
        row: 25,
        runs: vec![],
    });
    let address = base.pages[0].address;
    assert_eq!(
        lift_teletext_v1_page_set(
            base.clone(),
            TeletextEncodingRegion::WesternLatin,
            TeletextServiceBudget {
                max_records: 0,
                ..budget()
            }
        ),
        Err(TeletextServiceError::Budget {
            dimension: "record",
            maximum: 0
        })
    );
    assert_eq!(
        lift_teletext_v1_page_set(base.clone(), TeletextEncodingRegion::WesternLatin, budget()),
        Err(TeletextServiceError::Base(TeletextStructureError::Row {
            address,
            row: 25
        }))
    );
    base.pages[0].rows.clear();
    assert_eq!(
        lift_teletext_v1_page_set(base, TeletextEncodingRegion::WesternLatin, budget()),
        Err(TeletextServiceError::Charset {
            display: address,
            error: TeletextCharsetValueError::PageDesignation { code: 7 }
        })
    );
}

#[test]
fn lifting_intent_does_not_claim_character_or_physical_encoding_acceptance() {
    let mut base = base();
    base.pages[0].rows.push(TeletextRow {
        row: 1,
        runs: vec![TeletextRun {
            column: 0,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Text {
                text: "🦀".into()
            },
        }],
    });
    let service = lift(base, budget());
    assert!(matches!(
        encode_teletext_page_set(service.base(), TeletextEncodingRegion::WesternLatin),
        Err(TeletextEncodingError::Character {
            row: 1,
            column: 0,
            codepoint: 0x1f980,
            ..
        })
    ));
}

#[test]
fn conflicting_magazine_presentation_and_defaults_cannot_restyle_lifted_pages() {
    let lifted = lift(base(), budget());
    let saved = lifted.page_enhancements().clone();
    let mut base = lifted.base().clone();
    let mut peer = base.pages[0].clone();
    peer.address.page = TeletextPageNumber::new(201).unwrap();
    base.pages.push(peer.clone());
    let resource = TeletextResourceIdentity::new(
        TeletextResourceKind::Gpop,
        TeletextRawDataPageNumber::new(2, 0xa0).unwrap(),
        TeletextResourceSubpageIndex::new(0).unwrap(),
    )
    .unwrap();
    let target = TeletextObjectIdentity::global(resource, TeletextObjectId::new(900)).unwrap();
    let mut pages: Vec<_> = saved.values().cloned().collect();
    let mut peer_metadata = pages[0].clone();
    peer_metadata.address = peer.address;
    peer_metadata.presentation = None;
    peer_metadata.local_program = None;
    pages.push(peer_metadata);
    for metadata in &mut pages {
        metadata.resources.objects.global = Some(resource.page());
        metadata.resources.defaults[0] = Some(TeletextDefaultObjectSelection {
            target,
            object_type: TeletextObjectType::Active,
        });
    }
    let mut conflicting = saved.values().next().unwrap().presentation.unwrap();
    conflicting.primary = TeletextPageCharsetDesignation::new(0x37).unwrap();
    conflicting.screen_colour = TeletextColourReference::new(31).unwrap();
    conflicting.palette.clut2 = [TeletextRgb::new([0, 15, 0]).unwrap(); 8];
    let service = TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base,
            pages,
            magazines: vec![TeletextMagazinePresentationDraft {
                magazine: 2,
                presentation: conflicting,
            }],
            objects: vec![TeletextEnhancementObjectDefinition {
                identity: target,
                object_type: TeletextObjectType::Active,
                program: TeletextEnhancementProgram::new(vec![
                    TeletextEnhancementCommand::SetScreenColour {
                        colour: TeletextColourReference::new(31).unwrap(),
                    },
                ])
                .unwrap(),
            }],
            drcs: vec![],
        },
        TeletextServiceBudget {
            max_objects: 1,
            max_commands: 9,
            max_collected_commands: 2,
            ..budget()
        },
    )
    .unwrap();
    for (address, metadata) in saved {
        assert_eq!(
            service.effective_presentation(address),
            Some(TeletextEffectivePresentation::Extension {
                source: TeletextPresentationSource::Page,
                defaults: metadata.presentation.as_ref().unwrap(),
            })
        );
        let (source, refs) = service.collected(address).unwrap();
        assert_eq!(source, &TeletextEffectiveProgramSource::Local);
        assert_eq!(refs.command_count(), 0);
    }
    assert_eq!(
        service.effective_presentation(peer.address),
        Some(TeletextEffectivePresentation::Extension {
            source: TeletextPresentationSource::Magazine,
            defaults: &conflicting,
        })
    );
    let (source, refs) = service.collected(peer.address).unwrap();
    assert_eq!(
        source,
        &TeletextEffectiveProgramSource::Defaults { slots: vec![0] }
    );
    assert_eq!(refs.command_count(), 2);
}
