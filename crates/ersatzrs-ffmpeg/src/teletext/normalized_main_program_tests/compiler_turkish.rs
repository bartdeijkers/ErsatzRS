//! Exact Table 36 Turkish 2/3 original, not a currency alias or two origins.

use super::*;
use crate::teletext::authored_service_tests as authored;
use crate::teletext::{
    encode_teletext_authored_service, project_encoded_teletext_display,
    EncodedTeletextAuthoredPage, EncodedTeletextAuthoredService,
    TeletextAuthoredServiceEncodingBudget, TeletextAuthoredServiceEncodingFailure,
    TeletextCompiledDisplayPageEncodingFailure, TeletextEncodedDisplayRecord,
};

// Existing pinned font policy for the assigned Table 36 TL identity. The
// standard assigns identity, not these exact twelve-by-ten font-policy pixels.
const TURKISH_23_MASK: [u16; 10] = [0, 0, 0x3f0, 0xc0, 0xd8, 0xd8, 0xd8, 0xde, 0, 0];

fn enlarged_turkish_base() -> TeletextSemanticService {
    turkish_base(0x16, false, false)
}

fn turkish_mask(underlined: bool) -> [u16; 10] {
    let mut mask = TURKISH_23_MASK;
    if underlined {
        mask[9] = 0xfff;
    }
    mask
}

fn turkish_base(primary: u8, underlined: bool, spacing: bool) -> TeletextSemanticService {
    let mut fixture = enlarged_national_base();
    fixture.national = 0;
    fixture.primary = Some(primary);
    // The overlapping row2 glyph is necessary to establish a carrier seed.
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
        panic!("the selected Adaptive starts with the final repaint attributes");
    };
    *underline_or_separated_mosaics = underlined;
    adaptive.program = program(commands);
    let service = fixture.build();
    let layout =
        crate::teletext::layout::compile_for_composition(&service.base().pages[0]).unwrap();
    let mut cells = layout.rows[1];
    assert_eq!(cells[7], b'A');
    if spacing {
        assert_eq!(cells[6], b' ');
        cells[6] = 0x1a;
    }
    cells[7] = 0x23;
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

fn assert_original(original: &TeletextMainComposition) -> usize {
    assert_original_case(original, 0x16, false, false)
}

fn assert_original_case(
    original: &TeletextMainComposition,
    primary: u8,
    underlined: bool,
    spacing: bool,
) -> usize {
    assert_eq!(original.base_display().primary().get(), primary);
    let display = original.display_resolution();
    let origin = display.cell(1, 7).unwrap().origin().unwrap();
    assert_eq!(display.cell(1, 8).unwrap().origin(), Some(origin));
    assert_eq!(
        display.origins()[origin].effective_size(),
        Some(TeletextDisplaySize::DoubleWidth)
    );
    assert!(origin >= original.glyph_layers().candidates().len());
    let index = display.origins()[origin].candidate_index();
    let candidate = original.glyph_layers().candidates()[index];
    assert_eq!(candidate.priority(), TeletextGlyphPriority::Base);
    assert_eq!(candidate.base().unwrap().code(), 0x23);
    assert_eq!(candidate.source(), None);
    assert_eq!(candidate.root_source(), None);
    assert_eq!(candidate.invocation(), None);
    let intent = original.glyph_intent().records()[index];
    assert_eq!(intent.disposition(), TeletextGlyphDisposition::Assigned);
    assert!(
        matches!(intent.glyph(), Some(TeletextGlyphIdentity::G0 {glyph})
        if glyph.repertoire() == TeletextG0Repertoire::Latin && glyph.code() == 0x23
            && glyph.national_subset() == Some(TeletextNationalSubset::Turkish))
    );
    assert_eq!(original.base_replacement(1, 7), None);
    assert!(original.drcs_execution().records().is_empty());
    for column in [7, 8] {
        let cell = display.cell(1, column).unwrap();
        assert_eq!(cell.attributes().spacing_separated().value(), Some(spacing));
        assert_eq!(
            cell.attributes().underline_or_separated_mosaics().value(),
            Some(underlined)
        );
        let glyph = crate::teletext::project_teletext_comparison_glyph(original, cell).unwrap();
        assert_eq!(glyph.mask.unwrap().rows(), turkish_mask(underlined));
        assert_eq!(
            glyph.effects,
            crate::teletext::TeletextComparisonGlyphEffects::Alphanumeric { underlined }
        );
    }
    origin
}

fn service_budget() -> TeletextAuthoredServiceEncodingBudget {
    let mut budget = authored::limits();
    budget.normalization = super::polish_carrier::polish_limits();
    budget
}

fn declared(seed: TeletextSemanticService) -> TeletextDeclaredService {
    let selections = seed
        .base()
        .pages
        .iter()
        .map(|page| TeletextDisplayMotSelection {
            display: page.address,
            mot: None,
        })
        .collect();
    TeletextDeclaredService::new(
        seed,
        vec![],
        selections,
        vec![],
        authored::declaration_limits(),
    )
    .unwrap()
}

fn encode(input: &TeletextDeclaredService) -> EncodedTeletextAuthoredService {
    encode_teletext_authored_service(
        input.clone(),
        &authored::transmissions(input),
        &service_budget(),
    )
    .unwrap()
}

fn display(
    encoded: &EncodedTeletextAuthoredService,
) -> &crate::teletext::EncodedTeletextCompiledDisplayPage {
    let EncodedTeletextAuthoredPage::CompiledDisplay(display) = &encoded.pages()[0] else {
        panic!("the first authored display must retain its normalized model");
    };
    display
}

fn rows(page: &crate::teletext::EncodedTeletextPage) -> Vec<TeletextEncodedDisplayRecord> {
    project_encoded_teletext_display(page, 128)
        .unwrap()
        .records
        .into_iter()
        .filter(|record| matches!(record, TeletextEncodedDisplayRecord::BaseRow { .. }))
        .collect()
}

fn assert_pending(error: &crate::teletext::TeletextAuthoredServiceEncodingError, origin: usize) {
    assert_eq!(error.display, Some(owner()));
    assert_eq!(error.resource, None);
    let TeletextAuthoredServiceEncodingFailure::Compiled(compiled) = &error.reason else {
        panic!("expected normalized display rejection: {error:?}");
    };
    assert_eq!(compiled.owner, owner());
    let TeletextCompiledDisplayPageEncodingFailure::Normalization(normalization) = &compiled.reason
    else {
        panic!("expected normalization rejection: {compiled:?}");
    };
    assert_eq!(normalization.owner, owner());
    let Failure::Assembly(assembly) = &normalization.reason else {
        panic!("expected located carrier assembly rejection: {normalization:?}");
    };
    assert_eq!(assembly.owner, owner());
    assert_eq!(assembly.origin, Some(origin));
    let crate::teletext::TeletextAugmentedOriginProgramFailure::Carrier(carrier) = &assembly.reason
    else {
        panic!("expected origin carrier rejection: {assembly:?}");
    };
    assert_eq!(carrier.origin, origin);
    assert_eq!(
        carrier.reason,
        crate::teletext::TeletextOriginCarrierFailure::NationalConversion
    );
}

#[test]
fn compiler_owned_turkish_23_preserves_settled_original_and_fallback() {
    let seed = enlarged_turkish_base();
    let before = seed.clone();
    let fallback = super::polish_carrier::polish_fallback(&seed);
    let fallback_rows = rows(&fallback);
    assert!(fallback_rows.iter().any(|record| matches!(record,
        TeletextEncodedDisplayRecord::BaseRow {row:1,cells} if cells[7] == 0x23)));
    let mut budget = authored::limits();
    budget.normalization = super::polish_carrier::polish_limits();
    let allocations = crate::teletext::allocate_teletext_resource_objects(
        seed.objects(),
        &[],
        budget.normalization.max_resource_objects,
    )
    .unwrap();
    let composed = crate::teletext::compile_teletext_composed_main_program(
        &seed,
        owner(),
        &allocations,
        budget.normalization.composition,
        budget.normalization.packing,
    )
    .unwrap();
    let origin = assert_original(composed.composition());
    let plan = crate::teletext::plan_teletext_base_compatibility_realization(
        composed.composition(),
        budget.normalization.realization,
    )
    .unwrap();
    assert!(!plan.seeds().is_empty());
    let input = TeletextDeclaredService::new(
        seed.clone(),
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
    assert!(seed.drcs().subtables().is_empty());
    assert!(seed.page_enhancements()[&owner()]
        .compositional_links
        .is_none());
    let result =
        encode_teletext_authored_service(input.clone(), &authored::transmissions(&input), &budget);
    assert_eq!(seed, before);
    assert_eq!(
        rows(&super::polish_carrier::polish_fallback(&seed)),
        fallback_rows
    );
    let encoded = result.unwrap_or_else(|error| {
        assert_pending(&error, origin);
        panic!("assigned Turkish Base23 still lacks an exact physical realization: {error}");
    });
    assert_eq!(encoded.declared(), &input);
    let EncodedTeletextAuthoredPage::CompiledDisplay(display) = &encoded.pages()[0] else {
        panic!("enlarged Turkish Base must retain the full normalized path");
    };
    assert_eq!(display.normalized().original(), composed.composition());
    assert_original(display.normalized().original());
    assert_eq!(rows(display.page()), fallback_rows);
}

#[path = "compiler_turkish/binding.rs"]
mod binding;
#[path = "compiler_turkish/mixed.rs"]
mod mixed;
#[path = "compiler_turkish/variants.rs"]
mod variants;
