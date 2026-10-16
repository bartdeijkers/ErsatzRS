//! Exact English national7C realization must retain an original two-pair page.

use super::*;
use crate::teletext::authored_service_tests as authored;
use crate::teletext::{
    encode_teletext_authored_service, project_encoded_teletext_display,
    EncodedTeletextAuthoredPage, EncodedTeletextAuthoredService,
    TeletextAuthoredServiceEncodingBudget, TeletextEncodedDisplayRecord,
};
use std::collections::BTreeSet;

// Original English Table36/7C and Hebrew Table46/7C share the assigned double
// vertical identity. These are the unchanged original national font-policy
// pixels, not a Latin broken-bar alias or decoder-derived replacement.
const ENGLISH_7C: [u16; 10] = [0, 0x198, 0x198, 0x198, 0x198, 0x198, 0x198, 0x198, 0x198, 0];

struct Case {
    underlined: bool,
    input: TeletextDeclaredService,
    original: TeletextMainComposition,
    fallback: Vec<TeletextEncodedDisplayRecord>,
}

fn mask(underlined: bool) -> [u16; 10] {
    let mut mask = ENGLISH_7C;
    if underlined {
        mask[9] = 0xfff;
    }
    mask
}

fn pairs() -> BTreeSet<TeletextCharsetPair> {
    BTreeSet::from([
        TeletextCharsetPair::LatinLatin,
        TeletextCharsetPair::GreekGreek,
    ])
}

fn service_budget() -> TeletextAuthoredServiceEncodingBudget {
    let mut budget = authored::limits();
    budget.normalization = super::polish_carrier::polish_limits();
    budget
}

fn seed(underlined: bool) -> TeletextSemanticService {
    seed_with_pair(underlined, Some(0x37))
}

fn seed_with_pair(underlined: bool, second: Option<u8>) -> TeletextSemanticService {
    let mut fixture = enlarged_national_base();
    fixture.national = 0;
    fixture.primary = Some(0);
    fixture.rows = vec![text(1, 7, "‖"), text(2, 8, "é"), text(3, 1, "B")];
    let adaptive = fixture
        .objects
        .iter_mut()
        .find(|object| object.identity == local(0xfffe))
        .unwrap();
    let mut commands = adaptive.program.commands().to_vec();
    let TeletextEnhancementCommand::AtColumn {
        operation:
            TeletextColumnOperation::Attributes {
                underline_or_separated_mosaics,
                ..
            },
        ..
    } = &mut commands[0]
    else {
        panic!("the exact Adaptive starts with final repaint attributes")
    };
    *underline_or_separated_mosaics = underlined;
    adaptive.program = program(commands);
    if let Some(second) = second {
        fixture.main.as_mut().unwrap().extend([
            position(4, 12),
            at(
                12,
                TeletextColumnOperation::DesignateCharset {
                    designation: TeletextEnhancementCharsetDesignation::new(second).unwrap(),
                },
            ),
            at(
                12,
                TeletextColumnOperation::G0 {
                    code: code(if second == 0x55 { 0x60 } else { b'A' }),
                },
            ),
            at(
                13,
                TeletextColumnOperation::DesignateCharset {
                    designation: TeletextEnhancementCharsetDesignation::new(0).unwrap(),
                },
            ),
        ]);
    }
    fixture.build()
}

fn assert_original(original: &TeletextMainComposition, underlined: bool) {
    assert_eq!(original.base_display().primary().get(), 0);
    assert_eq!(original.collected().charset_pairs(), &pairs());
    assert!(original.drcs_execution().records().is_empty());
    let view = original.display_resolution();
    let origin = view.cell(1, 7).unwrap().origin().unwrap();
    assert_eq!(view.cell(1, 8).unwrap().origin(), Some(origin));
    assert_eq!(
        view.origins()[origin].effective_size(),
        Some(TeletextDisplaySize::DoubleWidth)
    );
    assert!(origin >= original.glyph_layers().candidates().len());
    let candidate = original.glyph_layers().candidates()[view.origins()[origin].candidate_index()];
    assert_eq!(candidate.priority(), TeletextGlyphPriority::Base);
    assert_eq!(candidate.base().unwrap().code(), 0x7c);
    assert_eq!(candidate.source(), None);
    assert_eq!(candidate.root_source(), None);
    assert_eq!(candidate.invocation(), None);
    let intent = original.glyph_intent().records()[view.origins()[origin].candidate_index()];
    assert_eq!(intent.disposition(), TeletextGlyphDisposition::Assigned);
    assert!(
        matches!(intent.glyph(),Some(TeletextGlyphIdentity::G0 {glyph})
        if glyph.repertoire() == TeletextG0Repertoire::Latin && glyph.code() == 0x7c
            && glyph.national_subset() == Some(TeletextNationalSubset::English))
    );
    let direct = resolve_teletext_national_conversion(intent.glyph().unwrap()).unwrap();
    assert_eq!(
        direct.designation().pair(),
        TeletextCharsetPair::HebrewArabic
    );
    assert!(!pairs().contains(&direct.designation().pair()));
    assert_eq!(original.base_replacement(1, 7), None);
    for column in [7, 8] {
        let cell = view.cell(1, column).unwrap();
        assert_eq!(
            cell.attributes().underline_or_separated_mosaics().value(),
            Some(underlined)
        );
        assert_eq!(cell.attributes().spacing_separated().value(), Some(false));
        let glyph = crate::teletext::project_teletext_comparison_glyph(original, cell).unwrap();
        assert_eq!(glyph.mask.unwrap().rows(), mask(underlined));
        assert_eq!(
            glyph.effects,
            crate::teletext::TeletextComparisonGlyphEffects::Alphanumeric { underlined }
        );
    }
    let greek = view.cell(4, 12).unwrap().origin().unwrap();
    let index = view.origins()[greek].candidate_index();
    let glyph = original.glyph_intent().records()[index];
    assert_eq!(glyph.disposition(), TeletextGlyphDisposition::Assigned);
    assert!(
        matches!(glyph.glyph(),Some(TeletextGlyphIdentity::G0 {glyph})
        if glyph.repertoire() == TeletextG0Repertoire::Greek && glyph.code() == b'A'
            && glyph.national_subset().is_none())
    );
}

fn rows(page: &crate::teletext::EncodedTeletextPage) -> Vec<TeletextEncodedDisplayRecord> {
    project_encoded_teletext_display(page, 128)
        .unwrap()
        .records
        .into_iter()
        .filter(|record| matches!(record, TeletextEncodedDisplayRecord::BaseRow { .. }))
        .collect()
}

fn prepare(underlined: bool) -> Case {
    let semantic = seed(underlined);
    assert!(semantic.drcs().subtables().is_empty());
    assert!(semantic.object_companions().is_empty());
    let metadata = &semantic.page_enhancements()[&owner()];
    assert_eq!(metadata.resources.drcs, TeletextDrcsPageLinks::unlinked());
    assert!(metadata.compositional_links.is_none());
    assert_eq!(
        semantic.collected(owner()).unwrap().1.charset_pairs(),
        &pairs()
    );
    let fallback = rows(&super::polish_carrier::polish_fallback(&semantic));
    assert!(fallback.iter().any(|record| matches!(record,
        TeletextEncodedDisplayRecord::BaseRow {row:1,cells} if cells[7] == 0x7c)));
    let budget = service_budget().normalization;
    let allocations = crate::teletext::allocate_teletext_resource_objects(
        semantic.objects(),
        &[],
        budget.max_resource_objects,
    )
    .unwrap();
    let composed = crate::teletext::compile_teletext_composed_main_program(
        &semantic,
        owner(),
        &allocations,
        budget.composition,
        budget.packing,
    )
    .unwrap();
    assert_original(composed.composition(), underlined);
    let plan = crate::teletext::plan_teletext_base_compatibility_realization(
        composed.composition(),
        budget.realization,
    )
    .unwrap();
    assert!(!plan.seeds().is_empty());
    let original = composed.composition().clone();
    let input = TeletextDeclaredService::new(
        semantic,
        vec![],
        vec![TeletextDisplayMotSelection {
            display: owner(),
            mot: None,
        }],
        vec![],
        authored::declaration_limits(),
    )
    .unwrap();
    assert!(input.resource_inventory().is_empty());
    assert!(authored::transmissions(&input).is_empty());
    Case {
        underlined,
        input,
        original,
        fallback,
    }
}

fn assert_ptu(encoded: &EncodedTeletextAuthoredService, slot: u8, expected: [u16; 10]) {
    let resource = &encoded.compiler_drcs_resources()[0];
    let page = encoded
        .pages()
        .iter()
        .find(|page| {
            page.resource_header()
                .is_some_and(|header| header.identity() == resource.table().identity())
        })
        .unwrap();
    let header = page.resource_header().unwrap();
    assert_eq!(header.update_counter().get(), 0);
    assert_eq!(
        header.transmission(),
        TeletextResourceTransmission::FullUnchangedFirst
    );
    let projection = crate::teletext::project_encoded_teletext_drcs(page, 32).unwrap();
    let cells = projection
        .records
        .iter()
        .find_map(|record| match record {
            TeletextEncodedDisplayRecord::BaseRow { row: 5, cells } => Some(cells),
            _ => None,
        })
        .unwrap();
    assert_eq!(slot / 2 + 1, 5);
    let offset = usize::from(slot % 2) * 20;
    let actual = std::array::from_fn(|row| {
        (u16::from(cells[offset + row * 2] & 63) << 6) | u16::from(cells[offset + row * 2 + 1] & 63)
    });
    assert_eq!(actual, expected);
}

fn assert_raster(display: &crate::teletext::EncodedTeletextCompiledDisplayPage) {
    let budget = crate::teletext::TeletextRasterBudget {
        max_pixels: 120_000,
        max_diagnostics: 4096,
    };
    for flash_phase in [TeletextFlashPhase::Original, TeletextFlashPhase::Alternate] {
        let options = TeletextColourPixelOptions {
            reveal_concealed: false,
            flash_phase,
        };
        let original = crate::teletext::render_teletext_composition(
            display.normalized().original(),
            options,
            budget,
        )
        .unwrap();
        let physical = crate::teletext::render_teletext_physical_display(
            display.normalized().physical().replay(),
            options,
            budget,
        )
        .unwrap();
        assert!(original.diagnostics().is_empty());
        assert!(physical.diagnostics().is_empty());
        assert_eq!(
            (original.width(), original.height()),
            (physical.width(), physical.height())
        );
        assert_eq!(original.pixels(), physical.pixels());
    }
}

fn assert_output(case: &Case, encoded: &EncodedTeletextAuthoredService) {
    assert_eq!(encoded.declared(), &case.input);
    assert!(encoded.declared().semantic().drcs().subtables().is_empty());
    assert_eq!(encoded.compiler_drcs_resources().len(), 1);
    let resource = &encoded.compiler_drcs_resources()[0];
    let identity = TeletextResourceIdentity::new(
        TeletextResourceKind::Drcs,
        TeletextRawDataPageNumber::new(1, 0xa0).unwrap(),
        TeletextResourceSubpageIndex::ZERO,
    )
    .unwrap();
    assert_eq!(resource.table().identity(), identity);
    assert_eq!(resource.consumers(), [owner()]);
    assert_eq!(resource.table().glyphs().len(), 1);
    let slot = 8 + u8::from(case.underlined);
    assert_eq!(resource.table().glyphs()[0].slot.get(), slot);
    assert_eq!(
        resource.table().glyphs()[0].bitmap.get(),
        mask(case.underlined)
    );
    let EncodedTeletextAuthoredPage::CompiledDisplay(display) = &encoded.pages()[0] else {
        panic!("the enlarged assigned original needs the normalized path")
    };
    assert_eq!(display.normalized().original(), &case.original);
    assert_original(display.normalized().original(), case.underlined);
    assert_eq!(rows(display.page()), case.fallback);
    let physical = display.normalized().physical().replay();
    let charset = physical.charset_execution();
    let physical_pairs = std::iter::once(charset.primary().pair())
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
    assert_eq!(physical_pairs, pairs());
    for column in [7, 8] {
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
        assert_eq!(record.reference().subtable.get(), 0);
        assert_eq!(record.reference().slot.get(), slot);
        assert_eq!(record.bitmap().get(), mask(case.underlined));
        assert_eq!(
            cell.attributes().underline_or_separated_mosaics().value(),
            Some(false)
        );
        assert_eq!(cell.attributes().spacing_separated().value(), Some(false));
    }
    assert_ptu(encoded, slot, mask(case.underlined));
    assert_raster(display);
}

#[test]
fn english_double_vertical_with_two_original_pairs_uses_exact_resource_without_third_pair() {
    // Settle BOTH originals before the whole-factory success expectation. The
    // direct Hebrew recipe remains correct, and the two-pair limit stays exact.
    let cases = [false, true].map(prepare);
    for case in cases {
        let before = case.input.clone();
        let encoded = encode_teletext_authored_service(
            case.input.clone(),
            &authored::transmissions(&case.input),
            &service_budget(),
        );
        assert_eq!(case.input, before);
        let encoded = encoded.expect("valid English national7C plus unrelated Greek must realize exactly through an owned resource, preserving the original two-pair limit");
        assert_output(&case, &encoded);
        assert_eq!(case.input, before);
    }
}

fn assert_direct_identity(
    display: &crate::teletext::EncodedTeletextCompiledDisplayPage,
    underlined: bool,
) {
    let original = display.normalized().original();
    assert_eq!(original.base_display().primary().get(), 0);
    let view = original.display_resolution();
    let origin = view.cell(1, 7).unwrap().origin().unwrap();
    assert_eq!(view.cell(1, 8).unwrap().origin(), Some(origin));
    let candidate = original.glyph_layers().candidates()[view.origins()[origin].candidate_index()];
    assert_eq!(candidate.priority(), TeletextGlyphPriority::Base);
    assert_eq!(candidate.base().unwrap().code(), 0x7c);
    let intent = original.glyph_intent().records()[view.origins()[origin].candidate_index()];
    assert!(
        matches!(intent.glyph(),Some(TeletextGlyphIdentity::G0 {glyph})
        if glyph.repertoire() == TeletextG0Repertoire::Latin && glyph.code() == 0x7c
            && glyph.national_subset() == Some(TeletextNationalSubset::English))
    );
    let actual = display.normalized().physical().replay();
    assert!(actual.drcs_execution().records().is_empty());
    for column in [7, 8] {
        let cell = actual.display_resolution().cell(1, column).unwrap();
        let index = actual.display_resolution().origins()[cell.origin().unwrap()].candidate_index();
        let glyph = actual.glyph_intent().records()[index];
        assert_eq!(glyph.disposition(), TeletextGlyphDisposition::Assigned);
        assert!(
            matches!(glyph.glyph(),Some(TeletextGlyphIdentity::G0 {glyph})
            if glyph.repertoire() == TeletextG0Repertoire::Hebrew && glyph.code() == 0x7c
                && glyph.national_subset().is_none())
        );
        assert_eq!(
            cell.attributes().underline_or_separated_mosaics().value(),
            Some(underlined)
        );
        let glyph = crate::teletext::raster::comparison_physical_glyph(actual, cell).unwrap();
        assert_eq!(glyph.mask.unwrap().rows(), mask(underlined));
    }
    assert_raster(display);
}

#[test]
fn english_free_pair_or_existing_hebrew_retains_direct_primitive_packets_without_resources() {
    for second in [None, Some(0x55)] {
        for underlined in [false, true] {
            let semantic = seed_with_pair(underlined, second);
            let before = semantic.clone();
            let expected_pairs = if second.is_some() {
                BTreeSet::from([
                    TeletextCharsetPair::LatinLatin,
                    TeletextCharsetPair::HebrewArabic,
                ])
            } else {
                BTreeSet::from([TeletextCharsetPair::LatinLatin])
            };
            assert_eq!(
                semantic.collected(owner()).unwrap().1.charset_pairs(),
                &expected_pairs
            );
            let primitive = crate::teletext::encode_teletext_normalized_display_page(
                &semantic,
                owner(),
                TeletextEncodingRegion::WesternLatin,
                service_budget().normalization,
            )
            .unwrap();
            assert_eq!(
                primitive
                    .normalized()
                    .original()
                    .collected()
                    .charset_pairs(),
                &expected_pairs
            );
            assert_direct_identity(&primitive, underlined);
            if second.is_some() {
                let original = primitive.normalized().original();
                let view = original.display_resolution();
                let origin = view.cell(4, 12).unwrap().origin().unwrap();
                let index = view.origins()[origin].candidate_index();
                let glyph = original.glyph_intent().records()[index];
                assert_eq!(glyph.disposition(), TeletextGlyphDisposition::Assigned);
                assert!(
                    matches!(glyph.glyph(),Some(TeletextGlyphIdentity::G0 {glyph})
                    if glyph.repertoire() == TeletextG0Repertoire::Hebrew && glyph.code() == 0x60
                        && glyph.national_subset().is_none())
                );
            }
            let fallback = rows(&super::polish_carrier::polish_fallback(&semantic));
            let input = TeletextDeclaredService::new(
                semantic.clone(),
                vec![],
                vec![TeletextDisplayMotSelection {
                    display: owner(),
                    mot: None,
                }],
                vec![],
                authored::declaration_limits(),
            )
            .unwrap();
            let unchanged = input.clone();
            assert!(input.resource_inventory().is_empty());
            let encoded = encode_teletext_authored_service(
                input.clone(),
                &authored::transmissions(&input),
                &service_budget(),
            )
            .unwrap();
            assert_eq!(input, unchanged);
            assert_eq!(semantic, before);
            assert_eq!(encoded.declared(), &input);
            assert!(encoded.compiler_drcs_resources().is_empty());
            assert_eq!(encoded.pages().len(), 1);
            let EncodedTeletextAuthoredPage::CompiledDisplay(display) = &encoded.pages()[0] else {
                panic!("the direct original must retain the normalized display path")
            };
            assert_eq!(
                display.normalized().original(),
                primitive.normalized().original()
            );
            assert_eq!(display.page().data_units(), primitive.page().data_units());
            assert_eq!(encoded.magazine_units(), primitive.magazine_units());
            assert_eq!(rows(display.page()), fallback);
            assert_direct_identity(display, underlined);
        }
    }
}

#[path = "compiler_english/independent.rs"]
mod independent;
