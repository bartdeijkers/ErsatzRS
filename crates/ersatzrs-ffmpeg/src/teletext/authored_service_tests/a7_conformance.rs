//! Complete-factory A.7 attribute groups, type priority and interleaving.
use super::*;

fn identity(index: usize) -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(
        owner(0),
        TeletextObjectId::new(0xffe0 + u16::try_from(index).unwrap()),
    )
}
fn at(column: u8, operation: TeletextColumnOperation) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn { column, operation }
}
fn glyph(column: u8, code: u8) -> TeletextEnhancementCommand {
    at(
        column,
        TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(code).unwrap(),
        },
    )
}
fn foreground(column: u8, colour: u8) -> TeletextEnhancementCommand {
    at(
        column,
        TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(colour).unwrap(),
        },
    )
}
fn groups(column: u8) -> [TeletextEnhancementCommand; 4] {
    [
        foreground(column, 1),
        at(
            column,
            TeletextColumnOperation::Background {
                colour: TeletextColourReference::new(4).unwrap(),
            },
        ),
        at(
            column,
            TeletextColumnOperation::Flash {
                mode: TeletextFlashMode::Normal,
                rate: TeletextFlashRate::Slow1Hz,
            },
        ),
        at(
            column,
            TeletextColumnOperation::Attributes {
                double_height: false,
                double_width: false,
                boxed: true,
                conceal: false,
                invert: false,
                underline_or_separated_mosaics: false,
            },
        ),
    ]
}
fn source(
    objects: Vec<(TeletextObjectType, Vec<TeletextEnhancementCommand>)>,
) -> TeletextDeclaredService {
    let mut draft = input();
    let mut main = vec![TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(1, 0).unwrap(),
    }];
    for (index, (object_type, body)) in objects.into_iter().enumerate() {
        draft.objects.push(TeletextEnhancementObjectDefinition {
            identity: identity(index),
            object_type,
            program: TeletextEnhancementProgram::new(body).unwrap(),
        });
        main.push(TeletextEnhancementCommand::InvokeObject {
            target: identity(index),
            object_type,
            offset: None,
        });
    }
    draft.pages[0].local_program = Some(TeletextEnhancementProgram::new(main).unwrap());
    declared(draft)
}

struct Expected {
    column: u8,
    code: u8,
    foreground: u8,
    background: u8,
    flashing: bool,
    boxed: bool,
}
fn point(column: u8, code: u8, foreground: u8, background: u8) -> Expected {
    Expected {
        column,
        code,
        foreground,
        background,
        flashing: false,
        boxed: false,
    }
}

#[test]
fn four_disjoint_active_groups_combine_with_latest_invoked_glyph() {
    let objects = groups(8)
        .into_iter()
        .zip(*b"FBTD")
        .map(|(group, code)| (TeletextObjectType::Active, vec![group, glyph(8, code)]))
        .collect();
    check(
        "a7-groups",
        &source(objects),
        &[Expected {
            flashing: true,
            boxed: true,
            ..point(8, b'D', 1, 4)
        }],
    );
}

#[test]
fn all_three_types_keep_passive_priority_in_both_invocation_orders() {
    for reverse in [false, true] {
        let mut objects = vec![
            (
                TeletextObjectType::Active,
                vec![foreground(8, 1), glyph(8, b'A')],
            ),
            (
                TeletextObjectType::Adaptive,
                vec![foreground(8, 2), glyph(8, b'B')],
            ),
            (
                TeletextObjectType::Passive,
                vec![foreground(8, 3), glyph(8, b'P')],
            ),
        ];
        if reverse {
            objects.reverse();
        }
        check(
            if reverse {
                "a7-priority-reverse"
            } else {
                "a7-priority-forward"
            },
            &source(objects),
            &[point(8, b'P', 3, 0)],
        );
    }
}

#[test]
fn permitted_interleaving_keeps_each_types_original_attribute_scope() {
    use TeletextObjectType::{Active, Adaptive, Passive};
    for (name, types, first, second, expected) in [
        (
            "a7-active",
            [Active, Active],
            vec![foreground(5, 1), glyph(5, b'A'), glyph(15, b'A')],
            vec![foreground(10, 2), glyph(10, b'B')],
            [
                point(5, b'A', 1, 0),
                point(10, b'B', 2, 0),
                point(15, b'A', 2, 0),
            ],
        ),
        (
            "a7-adaptive",
            [Adaptive, Adaptive],
            vec![glyph(5, b'A'), glyph(15, b'A')],
            vec![glyph(10, b'B')],
            [
                point(5, b'A', 7, 0),
                point(10, b'B', 7, 0),
                point(15, b'A', 7, 0),
            ],
        ),
        (
            "a7-passive",
            [Passive, Passive],
            vec![foreground(5, 1), glyph(5, b'A'), glyph(15, b'A')],
            vec![foreground(10, 2), glyph(10, b'B')],
            [
                point(5, b'A', 1, 0),
                point(10, b'B', 2, 0),
                point(15, b'A', 1, 0),
            ],
        ),
        (
            "a7-mixed",
            [Active, Adaptive],
            vec![foreground(5, 1), glyph(5, b'A'), glyph(15, b'A')],
            vec![groups(10)[1], glyph(10, b'B')],
            [
                point(5, b'A', 1, 0),
                point(10, b'B', 1, 4),
                point(15, b'A', 1, 0),
            ],
        ),
    ] {
        check(
            name,
            &source(vec![(types[0], first), (types[1], second)]),
            &expected,
        );
    }
}

#[test]
fn active_overlap_rejects_each_same_attribute_type_even_with_equal_values() {
    for group in groups(8) {
        let body = vec![group, glyph(8, b'A')];
        let source = source(vec![
            (TeletextObjectType::Active, body.clone()),
            (TeletextObjectType::Active, body),
        ]);
        let error = refusal(&source);
        assert!(
            matches!(error.reason,TeletextDisplayResolutionFailure::ObjectOverlap { first,second }
            if first.get()==0 && second.get()==1)
        );
        assert_original_second(&error);
    }
}

#[test]
fn adaptive_interleaving_rejects_each_redefined_attribute_type() {
    for group in groups(5) {
        let source = source(vec![
            (
                TeletextObjectType::Adaptive,
                vec![group, glyph(5, b'A'), glyph(15, b'A')],
            ),
            (TeletextObjectType::Adaptive, vec![glyph(10, b'B')]),
        ]);
        let error = refusal(&source);
        assert!(
            matches!(error.reason,TeletextDisplayResolutionFailure::AdaptiveInterleaving { first,second }
            if first.get()==0 && second.get()==1)
        );
        assert_original_second(&error);
    }
}

fn refusal(source: &TeletextDeclaredService) -> TeletextDisplayResolutionError {
    let error = encode_teletext_authored_service(source.clone(), &transmissions(source), &limits())
        .unwrap_err();
    let Failure::Compiled(error) = error.reason else {
        panic!("complete original compilation refusal")
    };
    let TeletextCompiledDisplayPageEncodingFailure::Normalization(error) = error.reason else {
        panic!("normalization refusal")
    };
    let TeletextNormalizedMainProgramFailure::Original(error) = error.reason else {
        panic!("original source refusal")
    };
    let TeletextComposedMainEncodingError::Composition(error) = *error else {
        panic!("original composition refusal")
    };
    let TeletextMainCompositionFailure::DisplayResolution(error) = error.reason else {
        panic!("original A7 admission refusal")
    };
    error
}
fn assert_original_second(error: &TeletextDisplayResolutionError) {
    assert_eq!(
        error.location,
        Some(TeletextEnhancementSource {
            object: Some(identity(1)),
            command: 0
        })
    );
    assert_eq!(
        error.root_source,
        Some(TeletextSpatialRootSource::Local { command: 2 })
    );
    assert_eq!(
        error.invocation.map(TeletextSpatialInvocationId::get),
        Some(1)
    );
}

fn check(name: &str, source: &TeletextDeclaredService, expected: &[Expected]) {
    let encoded =
        encode_teletext_authored_service(source.clone(), &transmissions(source), &limits())
            .unwrap();
    assert_eq!(encoded.declared(), source);
    let EncodedTeletextAuthoredPage::CompiledDisplay(page) = &encoded.pages()[0] else {
        panic!("full compiled display")
    };
    for resolution in [
        page.normalized().original().display_resolution(),
        page.normalized().physical().replay().display_resolution(),
    ] {
        assert!(resolution.pending().is_empty());
        for value in expected {
            let cell = resolution.cell(1, value.column).unwrap();
            let attrs = cell.attributes();
            assert_eq!(cell.fragment(), TeletextDisplayFragment::Normal, "{name}");
            assert_eq!(
                attrs.foreground().value().unwrap().get(),
                value.foreground,
                "{name}"
            );
            assert_eq!(
                attrs.background().value().unwrap().get(),
                value.background,
                "{name}"
            );
            assert_eq!(attrs.boxed().value(), Some(value.boxed), "{name}");
            assert_eq!(
                attrs.flash().value().unwrap().mode(),
                if value.flashing {
                    TeletextFlashMode::Normal
                } else {
                    TeletextFlashMode::Steady
                },
                "{name}"
            );
        }
    }
    let original = page.normalized().original();
    for value in expected {
        let resolution = original.display_resolution();
        let origin =
            resolution.origins()[resolution.cell(1, value.column).unwrap().origin().unwrap()];
        assert!(
            matches!(original.glyph_layers().candidates()[origin.candidate_index()].operation(),
            Some(TeletextColumnOperation::G0 {code}) if code.get()==value.code),
            "{name}"
        );
    }
    for (index, call) in original.spatial_trace().invocations().iter().enumerate() {
        assert_eq!(call.target(), identity(index));
        assert_eq!(call.parent(), None);
        assert_eq!(
            call.root_source(),
            TeletextSpatialRootSource::Local { command: index + 1 }
        );
    }
    native_invocations(page.page(), original.spatial_trace());
    let base = encode_teletext_page(
        &source.semantic().base().pages[0],
        TeletextEncodingRegion::WesternLatin,
    )
    .unwrap();
    assert_eq!(
        page.page()
            .data_units()
            .iter()
            .filter(|u| packet(u) <= 25)
            .collect::<Vec<_>>(),
        base.data_units()
            .iter()
            .filter(|u| packet(u) <= 25)
            .collect::<Vec<_>>()
    );
    export(name, page.page(), expected);
}

fn native_invocations(page: &EncodedTeletextPage, trace: &TeletextSpatialTrace) {
    let projection = project_encoded_teletext_display(page, 128).unwrap();
    let modes = projection
        .records
        .iter()
        .filter_map(|record| match record {
            TeletextEncodedDisplayRecord::Triplets { row: 26, words, .. } => Some(words),
            _ => None,
        })
        .flatten()
        .filter(|word| **word & 0x3f >= 40)
        .map(|word| (word >> 6) & 0x1f)
        .collect::<Vec<_>>();
    let expected = trace
        .invocations()
        .iter()
        .map(|call| match call.object_type() {
            TeletextObjectType::Active => 0x11,
            TeletextObjectType::Adaptive => 0x12,
            TeletextObjectType::Passive => 0x13,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        modes
            .iter()
            .copied()
            .filter(|mode| (0x11..=0x13).contains(mode))
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(
        modes
            .iter()
            .copied()
            .filter(|mode| (0x15..=0x17).contains(mode))
            .count(),
        trace.invocations().len()
    );
}

fn export(name: &str, page: &EncodedTeletextPage, expected: &[Expected]) {
    let Some(directory) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_A7_CONFORMANCE_DIR"])
    else {
        return;
    };
    let cells=expected.iter().map(|value|{
        let base=[if value.column==8 || value.column==9 {90} else {32},15,0,7,0,0];
        let attrs=[serde_json::json!(u8::from(value.flashing)),serde_json::json!(0),serde_json::json!(false),
            serde_json::json!(value.boxed),serde_json::json!(false),serde_json::json!(false)];
        serde_json::json!({"row":1,"column":value.column,"l1":base,"l15":base,
            "l25":[value.code,0,0,value.foreground,value.background,0],
            "attributes":{"l1":[0,0,false,false,false,false],"l15":[0,0,false,false,false,false],"l25":attrs}})
    }).collect::<Vec<_>>();
    let value = serde_json::json!({"name":name,"page":512,"subcode":0,
        "units":page.data_units().iter().map(|u|u.to_vec()).collect::<Vec<_>>(),"cells":cells});
    std::fs::write(
        std::path::Path::new(&directory).join(format!("{name}.json")),
        serde_json::to_vec(&value).unwrap(),
    )
    .unwrap();
}
