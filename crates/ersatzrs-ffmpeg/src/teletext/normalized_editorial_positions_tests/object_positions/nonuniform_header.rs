//! The retained mixed-background source must reach the complete authored factory.
use super::*;

pub(super) fn assert_complete(
    source: &TeletextDeclaredService,
    original: &TeletextMainComposition,
    identities: (TeletextObjectIdentity, TeletextObjectIdentity),
) {
    let unchanged = source.clone();
    let fallback = encode_teletext_page(
        &source.semantic().base().pages[0],
        TeletextEncodingRegion::WesternLatin,
    )
    .unwrap();
    assert_nested_header_trace(
        original,
        identities.0,
        identities.1,
        TeletextObjectType::Active,
        TeletextObjectType::Passive,
    );
    let actual = encode_teletext_authored_service(source.clone(), &transmissions(source), &limits())
        .expect("the exact legal nonuniform-background Active header source must reach the complete factory");
    assert_eq!(actual.declared(), &unchanged);
    assert_eq!(source, &unchanged);
    let EncodedTeletextAuthoredPage::CompiledDisplay(display) = &actual.pages()[0] else {
        panic!("complete shared compiled display required");
    };
    let normalized = display.normalized();
    assert_native_flash(normalized.encoded(), identities);
    assert_nested_header_trace(
        normalized.original(),
        identities.0,
        identities.1,
        TeletextObjectType::Active,
        TeletextObjectType::Passive,
    );
    assert_nested_header_invocations(
        normalized.physical().replay().spatial_trace(),
        identities.0,
        identities.1,
        TeletextObjectType::Active,
        TeletextObjectType::Passive,
    );
    assert_nested_header_geometry(
        normalized.physical().replay().display_geometry(),
        identities.0,
        identities.1,
    );
    assert_tail_context(original, normalized);
    assert_selected_child_a(
        normalized.original().display_resolution(),
        normalized.original().glyph_intent(),
    );
    assert_selected_child_a(
        normalized.physical().replay().display_resolution(),
        normalized.physical().replay().glyph_intent(),
    );
    let emitted = project_encoded_teletext_display(display.page(), 128).unwrap();
    let baseline = project_encoded_teletext_display(&fallback, 128).unwrap();
    assert_eq!(emitted.header, baseline.header);
    let rows = |records: Vec<TeletextEncodedDisplayRecord>| {
        records
            .into_iter()
            .filter(|record| !matches!(record, TeletextEncodedDisplayRecord::Triplets { .. }))
            .collect::<Vec<_>>()
    };
    assert_eq!(rows(emitted.records), rows(baseline.records));
}

fn assert_tail_context(
    original: &TeletextMainComposition,
    normalized: &EncodedTeletextNormalizedMainProgram,
) {
    for column in 8..40 {
        let expected = original
            .attribute_execution()
            .before_adaptive_cell(0, column)
            .unwrap();
        let retained = normalized
            .original()
            .attribute_execution()
            .before_adaptive_cell(0, column)
            .unwrap();
        assert_eq!(retained, expected, "original context at column{column}");
        for model in [
            normalized.original().display_resolution(),
            normalized.physical().replay().display_resolution(),
        ] {
            let expected = original
                .display_resolution()
                .cell(0, column)
                .unwrap()
                .attributes();
            let actual = model.cell(0, column).unwrap().attributes();
            assert_eq!(
                actual.flash().value(),
                expected.flash().value(),
                "flash at column{column}"
            );
            assert_eq!(
                actual.flash().pending().is_some(),
                expected.flash().pending().is_some(),
                "flash uncertainty at column{column}"
            );
            for (before, after) in [
                (expected.foreground(), actual.foreground()),
                (expected.background(), actual.background()),
            ] {
                assert_eq!(after.value(), before.value(), "attribute at column{column}");
                assert_eq!(
                    after.pending().is_some(),
                    before.pending().is_some(),
                    "uncertainty at column{column}"
                );
            }
        }
    }
}

pub(super) fn assert_foreground_derivation(
    original: &TeletextMainComposition,
    catalog: &TeletextEnhancementObjectCatalog,
    derived: &crate::teletext::object_header_positions::DerivedObjectHeaderPositions,
    outer: TeletextObjectIdentity,
) {
    let colour = TeletextColourReference::new(7).unwrap();
    for invocation in original
        .spatial_trace()
        .invocations()
        .iter()
        .filter(|call| call.target() == outer)
    {
        let row = u8::try_from(invocation.origin().row()).unwrap();
        let first = u8::try_from(invocation.origin().column() + 8).unwrap();
        for column in first..original.display_geometry().total_width() {
            let foreground = original
                .attribute_execution()
                .before_adaptive_cell(row, column)
                .unwrap()
                .foreground();
            assert_eq!(foreground.value(), Some(colour));
            assert_eq!(foreground.pending(), None);
            assert!(matches!(
                foreground.source(),
                Some(TeletextAttributeSource::Base { .. })
            ));
        }
    }
    assert_derived_writer(
        catalog,
        derived,
        outer,
        TeletextColumnOperation::Foreground { colour },
    );
}

pub(super) fn assert_flash_derivation(
    original: &TeletextMainComposition,
    catalog: &TeletextEnhancementObjectCatalog,
    derived: &crate::teletext::object_header_positions::DerivedObjectHeaderPositions,
    outer: TeletextObjectIdentity,
) {
    let expected = original
        .attribute_execution()
        .before_adaptive_cell(0, 8)
        .unwrap()
        .flash()
        .value()
        .unwrap();
    assert_eq!(expected.mode(), TeletextFlashMode::Steady);
    assert_eq!(expected.rate(), TeletextFlashRate::Slow1Hz);
    for invocation in original
        .spatial_trace()
        .invocations()
        .iter()
        .filter(|call| call.target() == outer)
    {
        let row = u8::try_from(invocation.origin().row()).unwrap();
        let first = u8::try_from(invocation.origin().column() + 8).unwrap();
        for column in first..original.display_geometry().total_width() {
            let field = original
                .attribute_execution()
                .before_adaptive_cell(row, column)
                .unwrap()
                .flash();
            assert_eq!(field.value(), Some(expected));
            assert_eq!(field.pending(), None);
            assert!(matches!(
                field.source(),
                Some(TeletextAttributeSource::Base { .. })
            ));
        }
    }
    assert_derived_writer(
        catalog,
        derived,
        outer,
        TeletextColumnOperation::Flash {
            mode: expected.mode(),
            rate: expected.rate(),
        },
    );
}

fn assert_derived_writer(
    catalog: &TeletextEnhancementObjectCatalog,
    derived: &crate::teletext::object_header_positions::DerivedObjectHeaderPositions,
    outer: TeletextObjectIdentity,
    operation: TeletextColumnOperation,
) {
    for definition in catalog.definitions() {
        let actual = derived.catalog().definition(definition.identity).unwrap();
        if definition.identity != outer {
            assert_eq!(actual, definition);
            continue;
        }
        let mut expected = definition.program.commands().to_vec();
        expected.insert(
            1,
            TeletextEnhancementCommand::AtColumn {
                column: 8,
                operation,
            },
        );
        assert_eq!(actual.program.commands(), expected);
        let source = |command| TeletextEnhancementSource {
            object: Some(outer),
            command,
        };
        assert_eq!(derived.retained_source(source(1)), None);
        assert_eq!(derived.retained_source(source(0)), Some(source(0)));
        for command in 1..definition.program.commands().len() {
            assert_eq!(
                derived.retained_source(source(command + 1)),
                Some(source(command))
            );
        }
        let (_, sources) = derived.clone().into_parts();
        assert_eq!(sources.materialized_position(source(1)), Some(0));
    }
}

fn assert_native_flash(
    packed: &EncodedTeletextLocalObjectProgram,
    identities: (TeletextObjectIdentity, TeletextObjectIdentity),
) {
    assert_original_object_sources(packed, identities.0, identities.1);
    let producers = packed
        .command_positions()
        .iter()
        .filter(|position| {
            position.source().object == Some(identities.0)
                && position.original_object_source()
                    == Some(TeletextMainCompositionSource::HeaderPosition { command: 0 })
        })
        .collect::<Vec<_>>();
    assert_eq!(producers.len(), 1);
    let source = producers[0];
    assert_eq!(source.source().command, 1);
    assert_eq!(source.first_word(), source.semantic_word());
    let index = source.semantic_word();
    let offset = 1 + (index % 13) * 3;
    assert_eq!(
        &packed.packet_bodies()[index / 13][offset..offset + 3],
        &crate::teletext::packets::hamming_24_18(0x08 | (7 << 6))
    );
}

#[test]
fn object_header_derivation_refuses_nonsteady_and_phase_sensitive_flash_fallback() {
    for (mode, rate) in [
        (TeletextFlashMode::Normal, TeletextFlashRate::Slow1Hz),
        (
            TeletextFlashMode::Steady,
            TeletextFlashRate::Fast2HzIncremental,
        ),
    ] {
        assert_flash_not_admitted(mode, rate);
    }
}

fn assert_flash_not_admitted(mode: TeletextFlashMode, rate: TeletextFlashRate) {
    let (mut draft, outer, _) =
        nested_header_draft(TeletextObjectType::Active, TeletextObjectType::Passive);
    draft.base.pages[0]
        .rows
        .iter_mut()
        .find(|row| row.row == 0)
        .unwrap()
        .runs
        .push(TeletextRun {
            column: 20,
            style: TeletextStyle {
                background: TeletextColour::Blue,
                ..TeletextStyle::default()
            },
            content: TeletextRunContent::Text {
                text: "Z".repeat(20),
            },
        });
    let page = &mut draft.pages[0];
    let mut commands = page.local_program.as_ref().unwrap().commands().to_vec();
    commands.insert(
        0,
        TeletextEnhancementCommand::AtColumn {
            column: 8,
            operation: TeletextColumnOperation::Flash { mode, rate },
        },
    );
    page.local_program = Some(TeletextEnhancementProgram::new(commands).unwrap());
    let source = declared(draft);
    let unchanged = source.clone();
    let service = source.semantic();
    let budget = limits().normalization;
    let original =
        compose_teletext_main_program(service, service.base().pages[0].address, budget.composition)
            .unwrap();
    let mut backgrounds = std::collections::BTreeSet::new();
    let mut foregrounds = std::collections::BTreeSet::new();
    for call in original
        .spatial_trace()
        .invocations()
        .iter()
        .filter(|call| call.target() == outer)
    {
        let row = u8::try_from(call.origin().row()).unwrap();
        let first = u8::try_from(call.origin().column() + 8).unwrap();
        for column in first..original.display_geometry().total_width() {
            let attributes = original
                .attribute_execution()
                .before_adaptive_cell(row, column)
                .unwrap();
            let flash = attributes.flash();
            assert_eq!(flash.pending(), None);
            let flash = flash.value().unwrap();
            assert_eq!((flash.mode(), flash.rate()), (mode, rate));
            backgrounds.insert(attributes.background().value().unwrap().get());
            foregrounds.insert(attributes.foreground().value().unwrap().get());
        }
    }
    assert!(
        backgrounds.len() > 1 && foregrounds.len() > 1,
        "this control must reach Flash after both colour candidates fail"
    );
    let (derived, _) = crate::teletext::object_header_positions::derive(
        &original,
        service.objects(),
        budget.packing,
        usize::MAX,
    );
    assert_eq!(derived.unwrap().catalog(), service.objects());
    assert_eq!(source, unchanged);
}
