//! Four established identities/effects share one bounded generated bank.

use super::*;

fn reowner(
    original: &TeletextEnhancementProgram,
    address: TeletextPageAddress,
) -> TeletextEnhancementProgram {
    let mut commands = original.commands().to_vec();
    for command in &mut commands {
        if let TeletextEnhancementCommand::InvokeObject { target, .. } = command {
            assert_eq!(target.owner(), Some(owner()));
            *target = TeletextObjectIdentity::local(address, target.object_id());
        }
    }
    program(commands)
}

fn consumers(slots: &[u8]) -> TeletextDeclaredService {
    let seeds = slots
        .iter()
        .map(|&slot| {
            let address = TeletextPageAddress {
                page: TeletextPageNumber::new(200 + u16::from(slot)).unwrap(),
                subcode: owner().subcode,
            };
            let seed = if slot < 2 {
                super::super::polish_carrier::enlarged_polish_base(None, slot == 1)
            } else {
                turkish_base(0x16, slot == 3, false)
            };
            (address, seed)
        })
        .collect::<Vec<_>>();
    let mut base = seeds[0].1.base().clone();
    base.initial_page = seeds[0].0.page;
    base.pages = seeds
        .iter()
        .map(|(address, seed)| {
            let mut page = seed.base().pages[0].clone();
            page.address = *address;
            page
        })
        .collect();
    let pages = seeds
        .iter()
        .map(|(address, seed)| {
            let mut page = seed.page_enhancements()[&owner()].clone();
            page.address = *address;
            page.local_program = page
                .local_program
                .as_ref()
                .map(|program| reowner(program, *address));
            page
        })
        .collect();
    let objects = seeds
        .iter()
        .flat_map(|(address, seed)| {
            seed.objects().definitions().iter().map(move |object| {
                let mut object = object.clone();
                object.identity =
                    TeletextObjectIdentity::local(*address, object.identity.object_id());
                object.program = reowner(&object.program, *address);
                object
            })
        })
        .collect();
    let semantic = TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base,
            pages,
            objects,
            magazines: vec![],
            drcs: vec![],
        },
        authored::semantic_limits(),
    )
    .unwrap()
    .with_physical_base_overlays(
        seeds
            .iter()
            .map(|(address, seed)| {
                TeletextPhysicalBaseOverlay::new(
                    *address,
                    seed.physical_base_overlays()[&owner()].rows().to_vec(),
                )
                .unwrap()
            })
            .collect(),
        authored::semantic_limits(),
    )
    .unwrap();
    declared(semantic)
}

fn assert_display(
    display: &crate::teletext::EncodedTeletextCompiledDisplayPage,
    input: &TeletextDeclaredService,
) {
    let address = display.page().address();
    let slot = u8::try_from(address.page.get() - 200).unwrap();
    let original = display.normalized().original();
    let expected_code = if slot < 2 { 0x5b } else { 0x23 };
    let expected_subset = if slot < 2 {
        TeletextNationalSubset::Polish
    } else {
        TeletextNationalSubset::Turkish
    };
    let index = selected(original.display_resolution(), 1, 7);
    assert!(matches!(original.glyph_intent().records()[index].glyph(),
        Some(TeletextGlyphIdentity::G0 {glyph}) if glyph.code()==expected_code
            && glyph.national_subset()==Some(expected_subset)));
    assert_eq!(original.base_replacement(1, 7), None);
    let actual = display.normalized().physical().replay();
    let index = selected(actual.display_resolution(), 1, 7);
    let execution = actual
        .drcs_execution()
        .records()
        .iter()
        .find(|record| record.candidate_index() == index)
        .unwrap();
    assert_eq!(execution.reference().slot.get(), slot);
    assert_eq!(
        execution.bitmap().get(),
        super::variants::expected_mask(slot)
    );
    let budget = service_budget().normalization;
    let allocations = crate::teletext::allocate_teletext_resource_objects(
        input.semantic().objects(),
        &[],
        budget.max_resource_objects,
    )
    .unwrap();
    let composed = crate::teletext::compile_teletext_composed_main_program(
        input.semantic(),
        address,
        &allocations,
        budget.composition,
        budget.packing,
    )
    .unwrap();
    assert_eq!(original, composed.composition());
}

#[test]
fn mixed_polish_turkish_slots_remain_stable_across_consumer_order_and_removal() {
    let mut identity = None;
    for slots in [
        vec![0, 1, 2, 3],
        vec![3, 2, 1, 0],
        vec![3],
        vec![2],
        vec![0, 3],
    ] {
        let input = consumers(&slots);
        assert!(input.resource_inventory().is_empty());
        let encoded = encode(&input);
        assert_eq!(encoded.declared(), &input);
        assert_eq!(encoded.compiler_drcs_resources().len(), 1);
        let resource = &encoded.compiler_drcs_resources()[0];
        assert_eq!(
            *identity.get_or_insert(resource.table().identity()),
            resource.table().identity()
        );
        assert_eq!(
            resource.table().identity().kind(),
            TeletextResourceKind::Drcs
        );
        let mut expected = slots.clone();
        expected.sort_unstable();
        assert_eq!(
            resource
                .table()
                .glyphs()
                .iter()
                .map(|glyph| glyph.slot.get())
                .collect::<Vec<_>>(),
            expected
        );
        for glyph in resource.table().glyphs() {
            assert_eq!(
                glyph.bitmap.get(),
                super::variants::expected_mask(glyph.slot.get())
            );
        }
        for page in encoded.pages() {
            if let EncodedTeletextAuthoredPage::CompiledDisplay(display) = page {
                assert_display(display, &input);
            }
        }
        super::variants::assert_ptu(&encoded);
    }
}

#[path = "mixed/both_scopes.rs"]
mod both_scopes;

#[path = "mixed/many_targets.rs"]
mod many_targets;
