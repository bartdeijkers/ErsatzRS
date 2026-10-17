use super::*;
use ersatzrs_app::teletext_tti::{parse_teletext_tti, write_teletext_tti};
use ersatzrs_domain::{
    TeletextColourReference, TeletextColumnOperation, TeletextEnhancementPosition,
    TeletextGlyphCode, TeletextObjectId, TeletextRawDataPageNumber, TeletextResourceIdentity,
    TeletextResourceKind, TeletextResourceSubpageIndex, TeletextResourceUpdateCounter,
};
use ersatzrs_ffmpeg::teletext::{
    encode_teletext_object_page, TeletextObjectPageTransmission, TeletextResourceFullUpdate,
    TeletextResourceTransmissionMode,
};

fn budget() -> TeletextTtiBudget {
    TeletextTtiBudget {
        max_bytes: 65536,
        max_records: 64,
    }
}
fn words(frame: &ObjectEditorFrame) -> Vec<u32> {
    frame
        .document
        .records()
        .iter()
        .filter_map(|record| match record {
            TeletextTtiRecord::Packet { row: 26, words, .. } => Some(words.as_slice()),
            _ => None,
        })
        .flatten()
        .copied()
        .collect()
}
fn resource(page: &EncodedTeletextAuthoredPage) -> &EncodedTeletextObjectPage {
    let EncodedTeletextAuthoredPage::Object(resource) = page else {
        panic!("object fixture")
    };
    resource
}
fn deadline() -> Instant {
    Instant::now() + std::time::Duration::from_secs(30)
}
fn foreground() -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column: 8,
        operation: TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(2).unwrap(),
        },
    }
}
fn encoded(
    kind: TeletextResourceKind,
    object_type: TeletextObjectType,
    count: u16,
    commands: &[TeletextEnhancementCommand],
) -> (EncodedTeletextAuthoredPage, TeletextObjectIdentity) {
    let resource = TeletextResourceIdentity::new(
        kind,
        TeletextRawDataPageNumber::new(8, 0xa0).unwrap(),
        TeletextResourceSubpageIndex::new(15).unwrap(),
    )
    .unwrap();
    let identity = |id| {
        if kind == TeletextResourceKind::Pop {
            TeletextObjectIdentity::public(resource, TeletextObjectId::new(id)).unwrap()
        } else {
            TeletextObjectIdentity::global(resource, TeletextObjectId::new(id)).unwrap()
        }
    };
    let definitions = (0..count)
        .map(|index| TeletextEnhancementObjectDefinition {
            identity: identity(65535 - index),
            object_type,
            program: TeletextEnhancementProgram::new(if index == 0 {
                commands.to_vec()
            } else {
                vec![]
            })
            .unwrap(),
        })
        .collect();
    let limits = TeletextObjectCatalogBudget {
        max_objects: 32,
        max_commands: 2000,
    };
    let catalog = TeletextEnhancementObjectCatalog::new(definitions, limits).unwrap();
    let allocations = allocate_teletext_resource_objects(&catalog, &[], 32).unwrap();
    let page = encode_teletext_object_page(
        resource,
        &catalog,
        &allocations,
        TeletextObjectPageTransmission {
            counter: TeletextResourceUpdateCounter::new(7).unwrap(),
            update: TeletextResourceFullUpdate::Changed,
            mode: TeletextResourceTransmissionMode::Serial,
            last_subpage: true,
        },
        2000,
    )
    .unwrap();
    (EncodedTeletextAuthoredPage::Object(page), identity(65535))
}

#[test]
fn genuine_wrapper_invokes_exact_resource_body_for_both_scopes_and_all_types() {
    for kind in [TeletextResourceKind::Pop, TeletextResourceKind::Gpop] {
        for object_type in [
            TeletextObjectType::Active,
            TeletextObjectType::Adaptive,
            TeletextObjectType::Passive,
        ] {
            let commands = vec![
                TeletextEnhancementCommand::SetPosition {
                    position: TeletextEnhancementPosition::new(4, 8).unwrap(),
                },
                foreground(),
                TeletextEnhancementCommand::AtColumn {
                    column: 8,
                    operation: TeletextColumnOperation::G0 {
                        code: TeletextGlyphCode::new(b'P').unwrap(),
                    },
                },
            ];
            let (page, target) = encoded(kind, object_type, 1, &commands);
            let original = page.data_units().to_vec();
            let frame = build_frame(&page, target, 43, budget(), deadline()).unwrap();
            assert_eq!(frame.identity, target);
            assert_eq!(
                frame.identity.resource().unwrap().logical_subpage().get(),
                15
            );
            assert_eq!(frame.object_type, object_type);
            let words = words(&frame);
            assert_eq!(words.len(), 13);
            assert_eq!(
                resource(&page).definition_positions()[0].absolute_index(),
                0
            );
            let mode = match object_type {
                TeletextObjectType::Active => 17,
                TeletextObjectType::Adaptive => 18,
                TeletextObjectType::Passive => 19,
            };
            assert_eq!(words[0], 0x28 | (mode << 6) | (2 << 11));
            assert_eq!(words[1], 0x37ff);
            assert_eq!(words[2], 0x28 | ((mode | 4) << 6) | (2 << 11));
            assert_eq!(
                words[3..6],
                [
                    0x2c | (4 << 6) | (8 << 11),
                    8 | (2 << 11),
                    8 | (9 << 6) | (80 << 11)
                ]
            );
            assert_eq!(words[6..], [0x2fff; 7]);
            for (ordinal, position) in frame.command_positions.iter().enumerate() {
                assert_eq!(position.command, ordinal);
                let native = &resource(&page).command_positions()[ordinal];
                assert_eq!(native.source().object, Some(target));
                assert_eq!(native.source().command, position.command);
                assert_eq!(native.first_word(), ordinal + 1);
                assert_eq!(native.semantic_word(), ordinal + 1);
                assert_eq!(position.editor_word, ordinal + 3);
            }
            assert_eq!(frame.command_positions.len(), 3);
            let bytes = write_teletext_tti(&frame.document, budget()).unwrap();
            let restored = parse_teletext_tti(&bytes, budget()).unwrap();
            assert_eq!(restored.records(), frame.document.records());
            assert!(matches!(
                restored.records()[0],
                TeletextTtiRecord::PageNumber {
                    page: 0x100,
                    editor_ordinal: 0
                }
            ));
            assert_eq!(page.data_units(), original);
        }
    }
}

#[test]
fn original_pointer_extensions_and_hidden_ordinals_keep_first_x3_correspondence() {
    for (count, origin) in [(1, 0), (17, 13), (25, 26)] {
        let (page, target) = encoded(
            TeletextResourceKind::Pop,
            TeletextObjectType::Passive,
            count,
            &[
                TeletextEnhancementCommand::SetPosition {
                    position: TeletextEnhancementPosition::new(0, 0).unwrap(),
                },
                foreground(),
            ],
        );
        let frame = build_frame(&page, target, 43, budget(), deadline()).unwrap();
        let words = words(&frame);
        let [native] = resource(&page).command_positions() else {
            panic!("one emitting original command")
        };
        assert_eq!(native.source().object, Some(target));
        assert_eq!(native.source().command, 1);
        let [position] = frame.command_positions.as_slice() else {
            panic!("one emitting original command")
        };
        assert_eq!(position.command, 1);
        assert_eq!(
            (native.first_word(), native.semantic_word()),
            (origin + 1, origin + 1)
        );
        assert_eq!(position.editor_word, 3);
        assert_eq!(words[3], 8 | (2 << 11));
    }
}

#[test]
fn entire_frame208_boundary_refuses209_without_truncating_large_native_objects() {
    for count in [204, 205, 505] {
        let (page, target) = encoded(
            TeletextResourceKind::Gpop,
            TeletextObjectType::Active,
            1,
            &vec![foreground(); count],
        );
        let original = page.data_units().to_vec();
        let result = build_frame(&page, target, 43, budget(), deadline());
        if count == 204 {
            let frame = result.unwrap();
            let words = words(&frame);
            assert_eq!(words.len(), 208);
            assert_eq!(words[207], 0x2fff);
            assert_eq!(frame.command_positions.len(), 204);
        } else {
            assert!(matches!(result, Err(Error::Budget)));
        }
        if count == 505 {
            assert_eq!(page.data_units().len(), 43);
        }
        assert_eq!(page.data_units(), original);
    }
    // Full native X3/X4 pointer extensions keep their own494/481 capacities;
    // refusing the editor frame must not replace, cut or renumber those pages.
    for (count, capacity) in [(17_u16, 494_usize), (25, 481)] {
        let (page, target) = encoded(
            TeletextResourceKind::Pop,
            TeletextObjectType::Active,
            count,
            &vec![foreground(); capacity - usize::from(count) - 1],
        );
        let original = page.data_units().to_vec();
        assert_eq!(original.len(), 43);
        assert!(matches!(
            build_frame(&page, target, 43, budget(), deadline()),
            Err(Error::Budget)
        ));
        assert_eq!(page.data_units(), original);
    }
}

#[test]
fn frame_requires_exact_target_positive_bounds_and_original_deadline() {
    let (page, target) = encoded(
        TeletextResourceKind::Pop,
        TeletextObjectType::Passive,
        1,
        &[],
    );
    let empty = build_frame(&page, target, 43, budget(), deadline()).unwrap();
    assert!(empty.command_positions.is_empty());
    assert!(resource(&page).command_positions().is_empty());
    let words = words(&empty);
    assert_eq!(
        words[..3],
        [
            0x28 | (19 << 6) | (2 << 11),
            0x37ff,
            0x28 | (23 << 6) | (2 << 11)
        ]
    );
    assert_eq!(words[3..], [0x2fff; 10]);
    let wrong =
        TeletextObjectIdentity::public(target.resource().unwrap(), TeletextObjectId::new(65534))
            .unwrap();
    assert!(matches!(
        build_frame(&page, wrong, 43, budget(), deadline()),
        Err(Error::Association)
    ));
    assert!(matches!(
        build_frame(&page, target, 0, budget(), deadline()),
        Err(Error::Budget)
    ));
    assert!(matches!(
        build_frame(&page, target, 4, budget(), deadline()),
        Err(Error::Budget)
    ));
    assert!(matches!(
        build_frame(&page, target, 43, budget(), Instant::now()),
        Err(Error::TimedOut)
    ));
    let tiny = TeletextTtiBudget {
        max_records: 5,
        ..budget()
    };
    assert!(matches!(
        build_frame(&page, target, 43, tiny, deadline()),
        Err(Error::Budget)
    ));
}
