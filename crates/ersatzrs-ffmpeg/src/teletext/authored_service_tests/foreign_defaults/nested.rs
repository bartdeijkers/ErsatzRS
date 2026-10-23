//! Original sparse MOT roots retain selected nested resource dependencies.
use super::*;

pub(super) fn add_dependency(
    object: &mut TeletextEnhancementObjectDefinition,
    target: TeletextObjectIdentity,
) {
    let mut commands = object.program.commands().to_vec();
    commands.push(TeletextEnhancementCommand::InvokeObject {
        target,
        object_type: TeletextObjectType::Passive,
        offset: Some(TeletextObjectOffset::new(0, 8).unwrap()),
    });
    object.program = TeletextEnhancementProgram::new(commands).unwrap();
}

fn export(encoded: EncodedTeletextAuthoredService, name: &str) {
    let Some(directory) = crate::locator::first_non_empty_env(&[
        "ERSATZRS_TELETEXT_TEST_DEFAULT_DEPENDENCIES_FIXTURE_DIR",
    ]) else {
        return;
    };
    let mut carousel = TeletextCarousel::new_authored(encoded).unwrap();
    let cycles = [
        crate::teletext::carousel::authored_service_carousel_tests::cycle(&mut carousel),
        crate::teletext::carousel::authored_service_carousel_tests::cycle(&mut carousel),
    ];
    let units = cycles
        .iter()
        .flatten()
        .flat_map(TeletextCarouselBatch::data_units)
        .map(|unit| unit.to_vec())
        .collect::<Vec<_>>();
    std::fs::write(
        std::path::Path::new(&directory).join(format!("{name}.json")),
        serde_json::to_vec(&serde_json::json!({"page":512,"subcode":1,"units":units})).unwrap(),
    )
    .unwrap();
}

#[test]
fn sparse_foreign_defaults_retain_selected_nested_pop_and_gpop_dependencies() {
    let plain = sparse_fixture(true, false);
    let source = sparse_fixture(true, true);
    assert_eq!(plain.semantic().objects(), source.semantic().objects());
    assert_eq!(plain.defaults(), source.defaults());
    assert_eq!(plain.resolutions(), source.resolutions());
    assert_eq!(plain.resource_inventory(), source.resource_inventory());
    let native = encode_teletext_authored_service(plain.clone(), &transmissions(&plain), &limits())
        .expect("absent X26 must preserve original native MOT activation and nested links");
    let compiled =
        encode_teletext_authored_service(source.clone(), &transmissions(&source), &limits())
            .expect(
                "compatibility X26 must realize original sparse roots with selected nested calls",
            );
    assert_eq!(native.declared(), &plain);
    assert_eq!(compiled.declared(), &source);
    let EncodedTeletextAuthoredPage::NativeDisplay(native_page) = &native.pages()[1] else {
        panic!("plain Base must retain native default activation");
    };
    assert!(native_page
        .encoded()
        .page()
        .data_units()
        .iter()
        .all(|unit| packet(unit) != 26));
    check_provenance(compiled_owner(&compiled));
    let fallback = encode_teletext_page(
        &source.semantic().base().pages[1],
        TeletextEncodingRegion::WesternLatin,
    )
    .unwrap();
    assert_eq!(
        compiled.pages()[1]
            .data_units()
            .iter()
            .filter(|unit| packet(unit) <= 25)
            .collect::<Vec<_>>(),
        fallback
            .data_units()
            .iter()
            .filter(|unit| packet(unit) <= 25)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        compiled.pages()[1]
            .data_units()
            .iter()
            .filter(|unit| packet(unit) >= 27)
            .collect::<Vec<_>>(),
        native.pages()[1]
            .data_units()
            .iter()
            .filter(|unit| packet(unit) >= 27)
            .collect::<Vec<_>>()
    );
    for resource in native
        .pages()
        .iter()
        .filter(|page| page.resource_header().is_some())
    {
        let actual = compiled
            .pages()
            .iter()
            .find(|page| page.inventory_record() == resource.inventory_record())
            .unwrap();
        assert_eq!(
            actual.data_units(),
            resource.data_units(),
            "original resource body changed"
        );
    }
    assert_eq!(
        compiled.pages()[0].data_units(),
        native.pages()[0].data_units()
    );
    export(native, "native");
    export(compiled, "compatibility");
}

fn check_provenance(page: &EncodedTeletextNormalizedMainProgram) {
    assert_eq!(
        page.original().original_program_source(),
        &TeletextEffectiveProgramSource::Defaults {
            slots: vec![0, 2, 3]
        }
    );
    let calls = page.original().spatial_trace().invocations();
    assert_eq!(
        calls
            .iter()
            .map(|call| (call.target(), call.object_type(), call.root_source()))
            .collect::<Vec<_>>(),
        [
            (global_target(0xb0), TeletextObjectType::Active, 0),
            (selected(), TeletextObjectType::Passive, 0),
            (target(20), TeletextObjectType::Passive, 2),
            (target(21), TeletextObjectType::Adaptive, 3),
            (global_target(0xb1), TeletextObjectType::Passive, 3),
        ]
        .map(|(target, kind, slot)| (
            target,
            kind,
            TeletextSpatialRootSource::DefaultSlot { slot }
        ))
    );
    for (child, parent, source_object) in [(1, 0, global_target(0xb0)), (4, 3, target(21))] {
        assert_eq!(calls[child].parent(), Some(calls[parent].id()));
        assert_eq!(calls[child].source().object, Some(source_object));
        assert_eq!(calls[child].source().command, 2);
        assert_eq!(
            calls[child].offset(),
            Some(TeletextObjectOffset::new(0, 8).unwrap())
        );
    }
    // EN300706 13.7: caller AP + origin modifier + definition coordinates.
    assert_eq!(
        (calls[1].origin().row(), calls[1].origin().column()),
        (1, 20)
    );
    assert_eq!(
        (calls[4].origin().row(), calls[4].origin().column()),
        (1, 18)
    );
    for (column, code) in [(8, b'A'), (10, b'C'), (12, b'G')] {
        assert_latin_cell(page, 1, column, code);
    }
    assert_latin_cell(page, 2, 28, b'Z');
    assert_latin_cell(page, 2, 30, b'Q');
}
