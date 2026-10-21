//! Resource C2 is tied to final words and one exact executing instance.

use super::*;

fn position(row: u8, column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(row, column).unwrap(),
    }
}
fn foreground(value: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column: 5,
        operation: TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(value).unwrap(),
        },
    }
}
fn call(object_type: TeletextObjectType) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::InvokeObject {
        target: target(20),
        object_type,
        offset: None,
    }
}
fn typed_service(object_type: TeletextObjectType) -> TeletextDeclaredService {
    typed_service_with_main(object_type, vec![call(object_type)])
}
fn typed_service_with_main(
    object_type: TeletextObjectType,
    main: Vec<TeletextEnhancementCommand>,
) -> TeletextDeclaredService {
    let mut draft = input();
    for page in &mut draft.base.pages {
        page.rows = vec![TeletextRow {
            row: 2,
            runs: vec![TeletextRun {
                column: 1,
                style: TeletextStyle::default(),
                content: TeletextRunContent::Text { text: "B".into() },
            }],
        }];
    }
    let mut commands = vec![position(1, 0); 11];
    commands.extend([
        foreground(4),
        foreground(13),
        TeletextEnhancementCommand::AtColumn {
            column: 5,
            operation: TeletextColumnOperation::G0 {
                code: TeletextGlyphCode::new(b'A').unwrap(),
            },
        },
    ]);
    draft.objects[1].object_type = object_type;
    draft.objects[1].program = TeletextEnhancementProgram::new(commands).unwrap();
    for page in &mut draft.pages {
        page.resources.defaults[2].as_mut().unwrap().object_type = object_type;
    }
    draft.pages[0].local_program = Some(TeletextEnhancementProgram::new(main).unwrap());
    let companions = vec![resource(TeletextResourceKind::Pop, 0xa0, 1)];
    let semantic = TeletextSemanticService::new_with_object_companions(
        draft,
        companions.clone(),
        semantic_limits(),
    )
    .unwrap();
    TeletextDeclaredService::new(
        semantic,
        vec![mot_with_type(object_type)],
        [0, 1]
            .map(|subcode| TeletextDisplayMotSelection {
                display: owner(subcode),
                mot: Some(resource(TeletextResourceKind::Mot, 0xfe, 9)),
            })
            .to_vec(),
        companions,
        declaration_limits(),
    )
    .unwrap()
}
fn assert_pink(composition: &TeletextMainComposition, row: u8) {
    let attrs = composition.attribute_execution();
    let field = attrs.cell(row, 5).unwrap().foreground();
    assert_eq!(
        field.value(),
        Some(TeletextColourReference::new(13).unwrap())
    );
    assert_eq!(field.pending(), None);
    let source = field.source().unwrap();
    assert!(matches!(source, TeletextAttributeSource::Enhancement {
        location: TeletextEnhancementSource { object: Some(identity), command: 12 },
        invocation: Some(_), ..
    } if identity == target(20)));
    let display = composition.display_resolution();
    assert_eq!(
        display
            .cell(row, 5)
            .unwrap()
            .attributes()
            .foreground()
            .value(),
        field.value()
    );
    assert_eq!(
        display
            .cell(row, 5)
            .unwrap()
            .attributes()
            .foreground()
            .pending(),
        None
    );
    let pixel = composition
        .colour_composition()
        .pixel(
            display,
            row,
            5,
            true,
            TeletextColourPixelOptions {
                reveal_concealed: false,
                flash_phase: TeletextFlashPhase::Original,
            },
        )
        .unwrap();
    assert!(matches!(pixel, TeletextColourPixel::Rgb(rgb) if rgb.get() == [7, 0, 7]));
}
fn complete_case(object_type: TeletextObjectType) {
    let declared = typed_service(object_type);
    let original = declared.clone();
    let budget = limits();
    // The standalone primitive has no emitted resource-body evidence.
    let standalone = compose_teletext_main_program(
        declared.semantic(),
        owner(1),
        budget.normalization.composition,
    )
    .unwrap();
    assert!(standalone
        .attribute_execution()
        .cell(1, 5)
        .unwrap()
        .foreground()
        .pending()
        .is_some());
    let tx = transmissions(&declared);
    let compiled = encode_teletext_authored_service(declared, &tx, &budget).unwrap();
    assert_eq!(compiled.declared(), &original);
    let EncodedTeletextAuthoredPage::CompiledDisplay(page) = &compiled.pages()[0] else {
        panic!("explicit Local call must retain normalization");
    };
    assert_pink(page.normalized().original(), 1);
    let actual = page.normalized().physical().replay();
    assert_eq!(
        actual
            .attribute_execution()
            .cell(1, 5)
            .unwrap()
            .foreground()
            .value(),
        Some(TeletextColourReference::new(13).unwrap())
    );
    assert_eq!(
        actual
            .attribute_execution()
            .cell(1, 5)
            .unwrap()
            .foreground()
            .pending(),
        None
    );
    let EncodedTeletextAuthoredPage::NativeDisplay(page) = &compiled.pages()[1] else {
        panic!("native MOT default must stay absent X26");
    };
    assert_pink(page.composition(), 1);
    assert!(matches!(
        page.composition().spatial_trace().invocations()[0].root_source(),
        TeletextSpatialRootSource::DefaultSlot { slot: 2 }
    ));
    assert!(!page
        .encoded()
        .page()
        .data_units()
        .iter()
        .any(|unit| packet(unit) == 26));
    let object = compiled
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::Object(page)
                if !page.definition_positions().is_empty() =>
            {
                Some(page)
            }
            _ => None,
        })
        .unwrap();
    let blue = object
        .command_positions()
        .iter()
        .find(|position| {
            position.source()
                == (TeletextEnhancementSource {
                    object: Some(target(20)),
                    command: 11,
                })
        })
        .unwrap();
    let pink = object
        .command_positions()
        .iter()
        .find(|position| {
            position.source()
                == (TeletextEnhancementSource {
                    object: Some(target(20)),
                    command: 12,
                })
        })
        .unwrap();
    assert_eq!((blue.semantic_word(), pink.semantic_word()), (12, 13));
}

#[test]
fn active_resource_packet_boundary_settles_native_and_both_normalized_models() {
    complete_case(TeletextObjectType::Active);
}
#[test]
fn adaptive_resource_packet_boundary_settles_native_and_both_normalized_models() {
    complete_case(TeletextObjectType::Adaptive);
}

#[test]
fn repeated_resource_instances_reuse_body_positions_without_merging_identity() {
    let object_type = TeletextObjectType::Adaptive;
    let declared = typed_service_with_main(
        object_type,
        vec![
            call(object_type),
            TeletextEnhancementCommand::InvokeObject {
                target: target(20),
                object_type,
                offset: Some(TeletextObjectOffset::new(2, 0).unwrap()),
            },
        ],
    );
    let tx = transmissions(&declared);
    let candidate = encode_teletext_authored_service(declared, &tx, &limits()).unwrap();
    let EncodedTeletextAuthoredPage::CompiledDisplay(page) = &candidate.pages()[0] else {
        panic!("repeated Local calls must retain normalization");
    };
    let original = page.normalized().original();
    assert_pink(original, 1);
    assert_pink(original, 3);
    assert_ne!(
        original.spatial_trace().invocations()[0].id(),
        original.spatial_trace().invocations()[1].id()
    );
    let actual = page.normalized().physical().replay();
    for row in [1, 3] {
        assert_eq!(
            actual
                .attribute_execution()
                .cell(row, 5)
                .unwrap()
                .foreground()
                .value(),
            Some(TeletextColourReference::new(13).unwrap())
        );
        assert_eq!(
            actual
                .attribute_execution()
                .cell(row, 5)
                .unwrap()
                .foreground()
                .pending(),
            None
        );
    }
}

// Intentional opt-in Linux x64 decoder-ABI calibration, not platform acceptance.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[tokio::test]
#[ignore = "Requires explicitly calibrated Linux x64 libzvbi 0.2.42"]
async fn complete_carousel_active_and_adaptive_resource_c2_decode() {
    let cases: Vec<_> = [TeletextObjectType::Active, TeletextObjectType::Adaptive]
        .into_iter()
        .map(|object_type| {
            let declared = typed_service(object_type);
            let tx = transmissions(&declared);
            let service = encode_teletext_authored_service(declared, &tx, &limits()).unwrap();
            let mut carousel = TeletextCarousel::new_authored(service).unwrap();
            let mut units = Vec::new();
            let mut cycles = 0;
            while cycles < 2 {
                let batch = carousel.next_batch().unwrap();
                units.extend_from_slice(batch.data_units());
                if batch.kind() == TeletextCarouselBatchKind::Terminator {
                    cycles += 1;
                }
            }
            serde_json::json!({
                "name": match object_type {
                    TeletextObjectType::Active => "resource_c2_active_native_default",
                    TeletextObjectType::Adaptive => "resource_c2_adaptive_native_default",
                    TeletextObjectType::Passive => unreachable!(),
                },
                "units": units.iter().map(|unit| unit.to_vec()).collect::<Vec<_>>(),
                "page": 0x200,
                "subcode": 1,
                "cells": [{"row":1,"column":5,"unicode":65,"foreground":13,"background":0}],
            })
        })
        .collect();
    let request = serde_json::to_vec(&serde_json::json!({
        "expected_version": "0.2.42", "cases": cases,
    }))
    .unwrap();
    if let Some(path) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_RESOURCE_C2_FIXTURE"])
    {
        std::fs::write(path, &request).unwrap();
    }
    let python = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_PYTHON"])
        .unwrap_or_else(|| "python3".into());
    let script = test_manifest_dir!().join("../../scripts/e2e/teletext-object-decoder.py");
    let report =
        crate::teletext_probe::run_probe(&python, &[script.to_str().unwrap()], &request, 16 * 1024)
            .await
            .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&report).unwrap();
    assert_eq!(
        report,
        serde_json::json!({"decoder_version":"0.2.42","cases_passed":2})
    );
    println!("Independent resource C2 decoding: {report}");
}
