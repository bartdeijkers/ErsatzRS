//! Exact assigned Romanian originals need no guessed breve/caron recipe.

use super::*;
use crate::teletext::authored_service_tests as authored;
use crate::teletext::{
    encode_teletext_authored_service, project_encoded_teletext_display,
    EncodedTeletextAuthoredPage, EncodedTeletextAuthoredService,
    TeletextAuthoredServiceEncodingBudget, TeletextEncodedDisplayRecord,
};

// Existing pinned national atlas masks, not Unicode aliases or composed marks.
const CAPITAL: [u16; 10] = [0x0d8, 0x070, 0x1fc, 0x306, 0x306, 0x3fe, 0x306, 0x306, 0, 0];
const SMALL: [u16; 10] = [0x0d8, 0x070, 0, 0x3f8, 0x00c, 0x3fc, 0x60c, 0x3fe, 0, 0];

struct Case {
    code: u8,
    underlined: bool,
    input: TeletextDeclaredService,
    original: TeletextMainComposition,
    fallback: Vec<TeletextEncodedDisplayRecord>,
}

fn expected_mask(code: u8, underlined: bool) -> [u16; 10] {
    let mut mask = match code {
        0x5d => CAPITAL,
        0x7d => SMALL,
        _ => panic!("only the two exact Romanian national positions are assigned here"),
    };
    if underlined {
        mask[9] = 0xfff;
    }
    mask
}

fn service_budget() -> TeletextAuthoredServiceEncodingBudget {
    let mut budget = authored::limits();
    budget.normalization = super::polish_carrier::polish_limits();
    budget
}

fn romanian_base(code: u8, underlined: bool) -> TeletextSemanticService {
    let mut fixture = enlarged_national_base();
    fixture.national = 0;
    fixture.primary = Some(0x1f);
    fixture.rows = vec![text(1, 7, "A"), text(2, 8, "é"), text(3, 1, "B")];
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
        panic!("the exact Adaptive begins with the final repaint attributes");
    };
    *underline_or_separated_mosaics = underlined;
    adaptive.program = program(commands);
    let service = fixture.build();
    let layout =
        crate::teletext::layout::compile_for_composition(&service.base().pages[0]).unwrap();
    let mut cells = layout.rows[1];
    assert_eq!(cells[7], b'A');
    cells[7] = code;
    service
        .with_physical_base_overlays(
            vec![TeletextPhysicalBaseOverlay::new(
                owner(),
                vec![TeletextPhysicalBaseRow::new(1, cells).unwrap()],
            )
            .unwrap()],
            authored::semantic_limits(),
        )
        .unwrap()
}

fn rows(page: &crate::teletext::EncodedTeletextPage) -> Vec<TeletextEncodedDisplayRecord> {
    project_encoded_teletext_display(page, 128)
        .unwrap()
        .records
        .into_iter()
        .filter(|record| matches!(record, TeletextEncodedDisplayRecord::BaseRow { .. }))
        .collect()
}

fn assert_original(original: &TeletextMainComposition, code: u8, underlined: bool) {
    assert_eq!(original.base_display().primary().get(), 0x1f);
    let view = original.display_resolution();
    let origin = view.cell(1, 7).unwrap().origin().unwrap();
    assert_eq!(view.cell(1, 8).unwrap().origin(), Some(origin));
    assert_eq!(
        view.origins()[origin].effective_size(),
        Some(TeletextDisplaySize::DoubleWidth)
    );
    assert!(origin >= original.glyph_layers().candidates().len());
    let index = view.origins()[origin].candidate_index();
    let candidate = original.glyph_layers().candidates()[index];
    assert_eq!(candidate.priority(), TeletextGlyphPriority::Base);
    assert_eq!(candidate.base().unwrap().code(), code);
    assert_eq!(candidate.source(), None);
    assert_eq!(candidate.root_source(), None);
    assert_eq!(candidate.invocation(), None);
    let intent = original.glyph_intent().records()[index];
    assert_eq!(intent.disposition(), TeletextGlyphDisposition::Assigned);
    assert!(
        matches!(intent.glyph(),Some(TeletextGlyphIdentity::G0 {glyph})
        if glyph.repertoire() == TeletextG0Repertoire::Latin && glyph.code() == code
            && glyph.national_subset() == Some(TeletextNationalSubset::Romanian))
    );
    assert_eq!(
        ersatzrs_domain::resolve_teletext_national_conversion(intent.glyph().unwrap()),
        Err(TeletextNationalConversionError::UnprovenRecipe {
            subset: TeletextNationalSubset::Romanian,
            code
        })
    );
    assert_eq!(original.base_replacement(1, 7), None);
    assert!(original.drcs_execution().records().is_empty());
    for column in [7, 8] {
        let cell = view.cell(1, column).unwrap();
        assert_eq!(cell.attributes().spacing_separated().value(), Some(false));
        assert_eq!(
            cell.attributes().underline_or_separated_mosaics().value(),
            Some(underlined)
        );
        let glyph = crate::teletext::project_teletext_comparison_glyph(original, cell).unwrap();
        assert_eq!(glyph.mask.unwrap().rows(), expected_mask(code, underlined));
        assert_eq!(
            glyph.effects,
            crate::teletext::TeletextComparisonGlyphEffects::Alphanumeric { underlined }
        );
    }
}

fn prepare(code: u8, underlined: bool) -> Case {
    let seed = romanian_base(code, underlined);
    assert!(seed.drcs().subtables().is_empty());
    assert_eq!(
        seed.page_enhancements()[&owner()].resources.drcs,
        TeletextDrcsPageLinks::unlinked()
    );
    assert!(seed.page_enhancements()[&owner()]
        .compositional_links
        .is_none());
    let fallback = rows(&super::polish_carrier::polish_fallback(&seed));
    assert!(fallback.iter().any(|record| matches!(record,TeletextEncodedDisplayRecord::BaseRow {row:1,cells} if cells[7] == code)));
    let budget = service_budget().normalization;
    let allocations = crate::teletext::allocate_teletext_resource_objects(
        seed.objects(),
        &[],
        budget.max_resource_objects,
    )
    .unwrap();
    let composed = crate::teletext::compile_teletext_composed_main_program(
        &seed,
        owner(),
        &allocations,
        budget.composition,
        budget.packing,
    )
    .unwrap();
    assert_original(composed.composition(), code, underlined);
    let plan = crate::teletext::plan_teletext_base_compatibility_realization(
        composed.composition(),
        budget.realization,
    )
    .unwrap();
    assert!(!plan.seeds().is_empty());
    let original = composed.composition().clone();
    let input = TeletextDeclaredService::new(
        seed,
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
    Case {
        code,
        underlined,
        input,
        original,
        fallback,
    }
}

fn assert_raster(display: &crate::teletext::EncodedTeletextCompiledDisplayPage) {
    let budget = crate::teletext::TeletextRasterBudget {
        max_pixels: 120_000,
        max_diagnostics: 4096,
    };
    let options = TeletextColourPixelOptions {
        reveal_concealed: false,
        flash_phase: TeletextFlashPhase::Original,
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

fn assert_output(case: &Case, encoded: &EncodedTeletextAuthoredService) {
    assert_eq!(encoded.declared(), &case.input);
    assert_eq!(encoded.compiler_drcs_resources().len(), 1);
    let resource = &encoded.compiler_drcs_resources()[0];
    assert_eq!(resource.consumers(), [owner()]);
    assert_eq!(
        resource.table().identity().kind(),
        TeletextResourceKind::Drcs
    );
    assert_eq!(resource.table().identity().logical_subpage().get(), 0);
    assert_eq!(resource.table().glyphs().len(), 1);
    let slot = if case.code == 0x5d { 4 } else { 6 } + u8::from(case.underlined);
    let mask = expected_mask(case.code, case.underlined);
    assert_eq!(resource.table().glyphs()[0].slot.get(), slot);
    assert_eq!(resource.table().glyphs()[0].bitmap.get(), mask);
    let EncodedTeletextAuthoredPage::CompiledDisplay(display) = &encoded.pages()[0] else {
        panic!("the enlarged original requires the complete normalized path");
    };
    assert_eq!(display.normalized().original(), &case.original);
    assert_original(display.normalized().original(), case.code, case.underlined);
    assert_eq!(rows(display.page()), case.fallback);
    let actual = display.normalized().physical().replay();
    for column in [7, 8] {
        let cell = actual.display_resolution().cell(1, column).unwrap();
        let index = actual.display_resolution().origins()[cell.origin().unwrap()].candidate_index();
        let executed = actual
            .drcs_execution()
            .records()
            .iter()
            .find(|record| record.candidate_index() == index)
            .unwrap();
        assert_eq!(executed.reference().table, TeletextDrcsTableScope::Normal);
        assert_eq!(executed.reference().subtable.get(), 0);
        assert_eq!(executed.reference().slot.get(), slot);
        assert_eq!(executed.resource(), resource.table().identity());
        assert_eq!(executed.bitmap().get(), mask);
        assert_eq!(
            cell.attributes().underline_or_separated_mosaics().value(),
            Some(false)
        );
        assert_eq!(cell.attributes().spacing_separated().value(), Some(false));
    }
    assert_raster(display);
}

#[test]
fn compiler_owned_romanian_originals_preserve_exact_masks_effects_and_fallback() {
    // Settle all four independent originals before the first whole-service
    // success expectation, so RED cannot conceal a malformed later fixture.
    let cases = [(0x5d, false), (0x5d, true), (0x7d, false), (0x7d, true)]
        .map(|(code, underlined)| prepare(code, underlined));
    for case in cases {
        let original = case.input.clone();
        let encoded = encode_teletext_authored_service(
            case.input.clone(),
            &authored::transmissions(&case.input),
            &service_budget(),
        );
        assert_eq!(case.input, original);
        let encoded = encoded.expect("assigned Romanian original requires exact resource-backed realization, not a guessed national recipe");
        assert_output(&case, &encoded);
        assert_eq!(case.input, original);
    }
}

#[path = "compiler_romanian/mixed.rs"]
mod mixed;
