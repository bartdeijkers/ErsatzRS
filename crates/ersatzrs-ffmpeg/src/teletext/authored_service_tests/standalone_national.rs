//! Table36 identities remain coded glyphs, never inferred Unicode aliases.

use super::*;
use serde_json::{json, Value};

const CODES: [u8; 13] = [
    0x23, 0x24, 0x40, 0x5b, 0x5c, 0x5d, 0x5e, 0x5f, 0x60, 0x7b, 0x7c, 0x7d, 0x7e,
];
const SUBSETS: [(u8, TeletextNationalSubset); 13] = [
    (0, TeletextNationalSubset::English),
    (1, TeletextNationalSubset::German),
    (2, TeletextNationalSubset::SwedishFinnishHungarian),
    (3, TeletextNationalSubset::Italian),
    (4, TeletextNationalSubset::French),
    (5, TeletextNationalSubset::PortugueseSpanish),
    (6, TeletextNationalSubset::CzechSlovak),
    (8, TeletextNationalSubset::Polish),
    (0x16, TeletextNationalSubset::Turkish),
    (0x1d, TeletextNationalSubset::SerbianCroatianSlovenian),
    (0x1f, TeletextNationalSubset::Romanian),
    (0x22, TeletextNationalSubset::Estonian),
    (0x23, TeletextNationalSubset::LatvianLithuanian),
];

fn draft(primary: u8) -> TeletextResolvedServiceDraft {
    let mut draft = input();
    draft.base.pages.truncate(1);
    draft.pages.truncate(1);
    draft.objects.clear();
    draft.drcs.clear();
    draft.magazines.clear();
    draft.base.pages[0].national_option = TeletextNationalOption::new(primary & 7).unwrap();
    draft.base.pages[0].rows = vec![TeletextRow {
        row: 3,
        runs: vec![TeletextRun {
            column: 1,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Text {
                text: "BASE 200".into(),
            },
        }],
    }];
    let mut presentation = super::super::presentation::western_latin_v1_presentation(
        TeletextNationalOption::default(),
    )
    .unwrap();
    presentation.primary = TeletextPageCharsetDesignation::new(primary).unwrap();
    draft.pages[0].presentation = Some(presentation);
    draft.pages[0].local_program = None;
    draft.pages[0].resources = TeletextResolvedPageResourceSelection {
        objects: TeletextResolvedObjectPageLinks {
            public: None,
            global: None,
        },
        drcs: TeletextDrcsPageLinks::unlinked(),
        defaults: [None; 4],
    };
    draft
}

fn declare(draft: TeletextResolvedServiceDraft) -> TeletextDeclaredService {
    let mut limits = semantic_limits();
    limits.max_objects = 0;
    let service = TeletextSemanticService::new(draft, limits).unwrap();
    let mut budget = declaration_limits();
    budget.max_mots = 0;
    budget.max_resources = 0;
    budget.max_objects = 0;
    TeletextDeclaredService::new(
        service,
        vec![],
        vec![TeletextDisplayMotSelection {
            display: owner(0),
            mot: None,
        }],
        vec![],
        budget,
    )
    .unwrap()
}

fn original(primary: u8) -> TeletextDeclaredService {
    let declared = declare(draft(primary));
    let mut cells = [b' '; 40];
    cells[2..15].copy_from_slice(&CODES);
    declared
        .with_physical_base_overlays(
            vec![TeletextPhysicalBaseOverlay::new(
                owner(0),
                vec![TeletextPhysicalBaseRow::new(1, cells).unwrap()],
            )
            .unwrap()],
            semantic_limits(),
        )
        .unwrap()
}

#[test]
fn reserved_header_without_matching_latin_context_stays_rejected() {
    for primary in [None, Some(0), Some(0x37)] {
        let mut draft = draft(primary.unwrap_or(0));
        draft.base.pages[0].national_option = TeletextNationalOption::new(7).unwrap();
        if primary.is_none() {
            draft.pages[0].presentation = None;
            assert!(matches!(
                TeletextSemanticService::new(draft, semantic_limits()),
                Err(TeletextServiceError::Charset {
                    error: TeletextCharsetValueError::PageDesignation { code: 7 },
                    ..
                })
            ));
            continue;
        }
        let service = declare(draft);
        let page = &service.semantic().base().pages[0];
        assert!(matches!(
            layout::compile_for_service_composition(service.semantic(), page),
            Err(TeletextEncodingError::NationalOption { address }) if address == owner(0)
        ));
        assert!(encode_teletext_authored_service(service.clone(), &[], &limits()).is_err());
        assert_eq!(service.semantic().base().pages[0], *page);
    }
}

#[test]
fn romanian_unicode_replacements_keep_original_intent_and_overlay_guards() {
    let mut draft = draft(0x1f);
    draft.base.pages[0].rows = rows("A@£éă");
    let declared = declare(draft);
    let service = declared.semantic();
    let page = &service.base().pages[0];
    // This exact page remains invalid at the legacy region-zero entry point.
    assert!(matches!(
        layout::compile_for_composition(page),
        Err(TeletextEncodingError::NationalOption { .. })
    ));
    let layout = layout::compile_for_service_composition(service, page).unwrap();
    assert_eq!(&layout.rows[1][8..13], b"A  ea");
    let replacements: Vec<_> = layout
        .base_glyphs()
        .iter()
        .map(TeletextColumnOperation::from)
        .collect();
    assert_eq!(
        replacements,
        [
            TeletextColumnOperation::G0Diacritic {
                code: TeletextGlyphCode::new(b'*').unwrap(),
                diacritic: TeletextDiacriticIndex::new(0).unwrap()
            },
            TeletextColumnOperation::G2 {
                code: TeletextGlyphCode::new(0x23).unwrap()
            },
            TeletextColumnOperation::G0Diacritic {
                code: TeletextGlyphCode::new(b'e').unwrap(),
                diacritic: TeletextDiacriticIndex::new(2).unwrap()
            },
            TeletextColumnOperation::G0Diacritic {
                code: TeletextGlyphCode::new(b'a').unwrap(),
                diacritic: TeletextDiacriticIndex::new(6).unwrap()
            },
        ]
    );
    let original = declared.clone();
    let encoded = encode_teletext_authored_service(declared.clone(), &[], &limits()).unwrap();
    assert_eq!(encoded.declared(), &original);
    assert!(matches!(
        &encoded.pages()[0],
        EncodedTeletextAuthoredPage::CompiledDisplay(_)
    ));
    if let Some(path) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_NATIONAL_UNICODE_FIXTURE"])
    {
        let mut units = encoded.pages()[0].data_units().to_vec();
        units.push(super::super::packets::terminator(2));
        let request = json!({"primary":0x1f,"page":0x200,"subcode":0,
            "units":units.iter().map(|unit|unit.to_vec()).collect::<Vec<_>>()});
        std::fs::write(path, serde_json::to_vec(&request).unwrap()).unwrap();
    }
    // A raw edit may not erase the original generated replacement under its
    // fallback. This rejection is identical to the established legacy guard.
    let mut cells = layout.rows[1];
    cells[11] = b'X';
    let edited = service
        .clone()
        .with_physical_base_overlays(
            vec![TeletextPhysicalBaseOverlay::new(
                owner(0),
                vec![TeletextPhysicalBaseRow::new(1, cells).unwrap()],
            )
            .unwrap()],
            semantic_limits(),
        )
        .unwrap();
    assert!(matches!(
        layout::compile_for_service_composition(&edited, &edited.base().pages[0]),
        Err(TeletextEncodingError::PhysicalBaseGeneratedGlyph {
            row: 1,
            column: 11,
            ..
        })
    ));
}

fn check_intent(original: &TeletextDeclaredService, primary: u8, subset: TeletextNationalSubset) {
    let composition = compose_teletext_main_program(
        original.semantic(),
        owner(0),
        TeletextMainCompositionBudget {
            max_input_commands: 64,
            max_output_commands: 64,
            collection: TeletextEnhancementCollectionBudget {
                max_commands: 128,
                max_drcs_references: 0,
            },
        },
    )
    .unwrap();
    assert_eq!(composition.base_display().primary().get(), primary);
    for (offset, code) in CODES.into_iter().enumerate() {
        let column = u8::try_from(offset + 2).unwrap();
        let display = composition.display_resolution();
        let origin = display.cell(1, column).unwrap().origin().unwrap();
        let index = display.origins()[origin].candidate_index();
        let candidate = composition.glyph_layers().candidates()[index];
        assert_eq!(candidate.priority(), TeletextGlyphPriority::Base);
        assert_eq!(candidate.base().unwrap().code(), code);
        let intent = composition.glyph_intent().records()[index];
        assert_eq!(intent.disposition(), TeletextGlyphDisposition::Assigned);
        assert!(
            matches!(intent.glyph(),Some(TeletextGlyphIdentity::G0 {glyph})
            if glyph.repertoire() == TeletextG0Repertoire::Latin && glyph.code() == code
                && glyph.national_subset() == Some(subset)),
            "{subset:?}/{code:02x}"
        );
    }
}

fn matrix() -> Value {
    let mut cases = Vec::new();
    for (primary, subset) in SUBSETS {
        let original = original(primary);
        assert!(original.resource_inventory().is_empty());
        check_intent(&original, primary, subset);
        let encoded = encode_teletext_authored_service(original.clone(), &[], &limits()).unwrap();
        assert_eq!(encoded.declared(), &original);
        assert!(encoded.compiler_drcs_resources().is_empty());
        assert!(encoded.magazine_units().is_empty());
        assert_eq!(encoded.pages().len(), 1);
        let page = &encoded.pages()[0];
        let EncodedTeletextAuthoredPage::NativeDisplay(_) = page else {
            panic!("a base-only national page must not acquire optional objects")
        };
        let row = page
            .data_units()
            .iter()
            .find(|unit| packet(unit) == 1)
            .unwrap();
        assert_eq!(body(row).map(|code| code & 0x7f)[2..15], CODES);
        let source_header::TeletextHeaderObservation::Header(header) =
            source_header::observe_dvb_teletext_header(&page.data_units()[0]).unwrap()
        else {
            panic!("actual page must start with a checked header")
        };
        assert_eq!(header.controls.c12_c14_bits(), primary & 7);
        let mut units = page.data_units().to_vec();
        units.push(super::super::packets::terminator(2));
        cases.push(
            json!({"name":format!("national_{primary:02x}"),"primary":primary,
            "page":0x200,"subcode":0,"codes":CODES,
            "units":units.iter().map(|unit|unit.to_vec()).collect::<Vec<_>>()}),
        );
    }
    json!({"cases":cases})
}

#[test]
fn all_169_national_base_glyphs_keep_header_context_and_need_no_object_banks() {
    let request = matrix();
    if let Some(path) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_NATIONAL_BASE_FIXTURE"])
    {
        std::fs::write(path, serde_json::to_vec(&request).unwrap()).unwrap();
    }
    println!("13 full-factory native contexts,169 literal Table36 identities and code cells; no object/MOT/DRCS banks or inferred Unicode aliases");
}

#[test]
fn regional_unicode_text_remains_exact_at_the_shared_layout_boundary() {
    for (primary, _) in SUBSETS {
        let mut draft = draft(primary);
        draft.base.pages[0].rows = rows("£");
        let declared = declare(draft);
        let service = declared.semantic();
        let layout =
            layout::compile_for_service_composition(service, &service.base().pages[0]).unwrap();
        if matches!(primary, 0 | 3) {
            assert_eq!(layout.rows[1][8], 0x23, "{primary:02x}");
            assert!(layout.base_glyphs().is_empty());
        } else {
            assert_eq!(
                layout.rows[1][8], b' ',
                "{primary:02x}: £ must not become another national glyph"
            );
            assert_eq!(layout.base_glyphs().len(), 1);
            assert_eq!(
                TeletextColumnOperation::from(&layout.base_glyphs()[0]),
                TeletextColumnOperation::G2 {
                    code: TeletextGlyphCode::new(0x23).unwrap()
                }
            );
        }
    }
}

#[test]
fn all_assigned_latin_aliases_encode_unicode_without_optional_resource_banks() {
    const A: [u16; 10] = [0, 0x0f0, 0x198, 0x30c, 0x606, 0x7fe, 0x606, 0x606, 0, 0];
    let mut cases = Vec::new();
    for primary in (0..=0x7f).filter(|&code| {
        TeletextPageCharsetDesignation::new(code)
            .is_ok_and(|primary| primary.pair() == TeletextCharsetPair::LatinLatin)
    }) {
        let mut draft = draft(primary);
        draft.base.pages[0].rows = rows("A£@#éÄçí");
        let declared = declare(draft);
        let encoded = encode_teletext_authored_service(declared.clone(), &[], &limits()).unwrap();
        assert_eq!(encoded.declared(), &declared);
        assert!(encoded.compiler_drcs_resources().is_empty());
        assert!(encoded.magazine_units().is_empty());
        assert_eq!(encoded.pages().len(), 1);
        let EncodedTeletextAuthoredPage::CompiledDisplay(display) = &encoded.pages()[0] else {
            panic!("the Unicode replacement fixture requires X26")
        };
        let raster = render_teletext_composition(
            display.normalized().original(),
            TeletextColourPixelOptions {
                reveal_concealed: false,
                flash_phase: TeletextFlashPhase::Original,
            },
            TeletextRasterBudget {
                max_pixels: 120_000,
                max_diagnostics: 1400,
            },
        )
        .unwrap();
        assert!(raster.diagnostics().is_empty());
        // A fixed invariant glyph has an absolute atlas oracle. National atlas
        // letters and composed diacritics may differ in vertical fitting (the
        // glyph-mask contract); cross-font pixel equality is not an oracle.
        for (y, mask) in A.into_iter().enumerate() {
            for x in 0..12 {
                let expected = if mask & (1 << (11 - x)) != 0 {
                    [15, 15, 15]
                } else {
                    [0, 0, 0]
                };
                assert!(
                    matches!(raster.pixel(8*12+x,10+y), Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(rgb))) if rgb.get()==expected)
                );
            }
        }
        let mut units = encoded.pages()[0].data_units().to_vec();
        units.push(super::super::packets::terminator(2));
        cases.push(json!({"name":format!("unicode_{primary:02x}"),"primary":primary,"page":0x200,"subcode":0,
            "units":units.iter().map(|unit|unit.to_vec()).collect::<Vec<_>>()}));
    }
    assert_eq!(cases.len(), 27);
    if let Some(path) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_REGIONAL_UNICODE_FIXTURE"])
    {
        std::fs::write(path, serde_json::to_vec(&json!({"cases":cases})).unwrap()).unwrap();
    }
}

#[test]
fn qualified_national_unicode_keeps_its_native_level1_cells() {
    // Literal Table36 scalar inputs. Blanks leave unresolved Unicode aliases
    // unclaimed; all thirteen original coded cells have separate raw fixtures.
    const TEXTS: [&str; 13] = [
        "£$@←½→↑#—¼‖¾÷",
        "#$§ÄÖÜ^_°äöüß",
        "#¤ÉÄÖÅÜ_éäöåü",
        "£$é°ç→↑#ùàòèì",
        "éïàëêùî#èâôûç",
        "ç$¡áéíóú¿üñèà",
        "#ůčťžýířéáěúš",
        "#ńą ŚŁćóężśłź",
        " ğİŞÖÇÜĞışöçü",
        "#ËČĆŽ Šëčćž š",
        "# ŢÂŞ  ıţâş î",
        "#õŠÄÖŽÜÕšäöžü",
        "#$Šė Žčūšąųžį",
    ];
    let mut cases = Vec::new();
    for ((primary, _), text) in SUBSETS.into_iter().zip(TEXTS) {
        assert_eq!(text.chars().count(), 13);
        let mut draft = draft(primary);
        draft.base.pages[0].rows = vec![TeletextRow {
            row: 1,
            runs: vec![TeletextRun {
                column: 2,
                style: TeletextStyle::default(),
                content: TeletextRunContent::Text { text: text.into() },
            }],
        }];
        let declared = declare(draft);
        let encoded = encode_teletext_authored_service(declared.clone(), &[], &limits()).unwrap();
        assert_eq!(encoded.declared(), &declared);
        assert!(encoded.compiler_drcs_resources().is_empty());
        assert!(matches!(
            &encoded.pages()[0],
            EncodedTeletextAuthoredPage::NativeDisplay(_)
        ));
        let expected: Vec<_> = text
            .chars()
            .zip(CODES)
            .map(|(ch, code)| if ch == ' ' { b' ' } else { code })
            .collect();
        let actual = encoded.pages()[0]
            .data_units()
            .iter()
            .find(|u| packet(u) == 1)
            .unwrap();
        assert_eq!(&body(actual).map(|c| c & 0x7f)[2..15], expected);
        let mut units = encoded.pages()[0].data_units().to_vec();
        units.push(super::super::packets::terminator(2));
        cases.push(json!({"name":format!("native_unicode_{primary:02x}"),"primary":primary,"page":0x200,"subcode":0,"text":text,
            "units":units.iter().map(|unit|unit.to_vec()).collect::<Vec<_>>()}));
    }
    if let Some(path) = crate::locator::first_non_empty_env(&[
        "ERSATZRS_TELETEXT_TEST_NATIVE_NATIONAL_UNICODE_FIXTURE",
    ]) {
        std::fs::write(path, serde_json::to_vec(&json!({"cases":cases})).unwrap()).unwrap();
    }
}
