//! A child cannot reset the Main prefix before its own explicit invocation.
use super::*;

fn fixture(empty_count: usize) -> TeletextDeclaredService {
    let mut draft = input();
    let empty = TeletextObjectIdentity::local(owner(0), TeletextObjectId::new(65534));
    let writer = TeletextObjectIdentity::local(owner(0), TeletextObjectId::new(65535));
    draft.objects.extend([
        TeletextEnhancementObjectDefinition {
            identity: empty,
            object_type: TeletextObjectType::Passive,
            program: TeletextEnhancementProgram::new(vec![]).unwrap(),
        },
        TeletextEnhancementObjectDefinition {
            identity: writer,
            object_type: TeletextObjectType::Active,
            program: TeletextEnhancementProgram::new(vec![TeletextEnhancementCommand::AtColumn {
                column: 0,
                operation: TeletextColumnOperation::G0 {
                    code: TeletextGlyphCode::new(b'A').unwrap(),
                },
            }])
            .unwrap(),
        },
    ]);
    let mut commands = vec![TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(1, 8).unwrap(),
    }];
    commands.extend(vec![
        TeletextEnhancementCommand::InvokeObject {
            target: empty,
            object_type: TeletextObjectType::Passive,
            offset: None,
        };
        empty_count
    ]);
    commands.push(TeletextEnhancementCommand::InvokeObject {
        target: writer,
        object_type: TeletextObjectType::Active,
        offset: None,
    });
    draft.pages[0].local_program = Some(TeletextEnhancementProgram::new(commands).unwrap());
    declared(draft)
}

#[test]
fn whole_factory_child_write_preserves_fifteen_but_cannot_reset_its_sixteenth_main_open() {
    let source = fixture(14);
    let encoded =
        encode_teletext_authored_service(source.clone(), &transmissions(&source), &limits())
            .unwrap();
    assert_eq!(encoded.declared(), &source);
    let EncodedTeletextAuthoredPage::CompiledDisplay(page) = &encoded.pages()[0] else {
        panic!("explicit Main retains complete X26")
    };
    let normalized = page.normalized();
    for trace in [
        normalized.original().spatial_trace(),
        normalized.physical().replay().spatial_trace(),
    ] {
        assert_eq!(trace.invocations().len(), 15);
    }
    let original = normalized.original().spatial_trace();
    for (index, call) in original.invocations().iter().enumerate() {
        assert_eq!(
            call.root_source(),
            TeletextSpatialRootSource::Local { command: index + 1 }
        );
        assert_eq!(call.parent(), None);
    }
    let fallback = encode_teletext_page(
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
        fallback
            .data_units()
            .iter()
            .filter(|u| packet(u) <= 25)
            .collect::<Vec<_>>()
    );
    let projection = project_encoded_teletext_display(page.page(), 128).unwrap();
    let words = projection
        .records
        .iter()
        .filter_map(|r| match r {
            TeletextEncodedDisplayRecord::Triplets { row: 26, words, .. } => Some(words),
            _ => None,
        })
        .flatten()
        .copied()
        .collect::<Vec<_>>();
    let calls = words
        .iter()
        .enumerate()
        .filter(|(_, w)| **w & 0x3f >= 40 && [0x11, 0x13].contains(&((**w >> 6) & 0x1f)))
        .map(|(i, w)| (i, (w >> 6) & 0x1f))
        .collect::<Vec<_>>();
    assert_eq!(
        calls.iter().map(|(_, m)| *m).collect::<Vec<_>>(),
        [vec![0x13; 14], vec![0x11]].concat()
    );
    let expected_glyph = (u32::from(b'A') << 11) | (9 << 6);
    let glyph = words
        .iter()
        .position(|&word| word == expected_glyph)
        .unwrap();
    assert!(
        glyph > calls[14].0,
        "the actual child glyph word follows its invoking Main word"
    );
    export(page.page());
    let invalid = fixture(15);
    let error = super::empty_admission::assert_successive_refusal(&invalid, 16);
    assert_eq!(error.position, Some((1, 8)));
}

fn export(page: &EncodedTeletextPage) {
    let Some(directory) = crate::locator::first_non_empty_env(&[
        "ERSATZRS_TELETEXT_TEST_SUCCESSIVE_PREFIX_FIXTURE_DIR",
    ]) else {
        return;
    };
    let mut units = page.data_units().to_vec();
    units.push(super::super::packets::terminator(2));
    std::fs::write(std::path::Path::new(&directory).join("fifteen.json"),serde_json::to_vec(&serde_json::json!({
        "expected_version":"0.2.0","cases":[{"name":"successive_main_child_fifteen","page":512,"subcode":0,
            "units":units.iter().map(|u|u.to_vec()).collect::<Vec<_>>(),"cells":[
                {"row":1,"column":8,"l15":[90,15,0,7,0],"l25":[65,0,0,7,0]},
                {"row":1,"column":9,"l15":[90,15,0,7,0],"l25":[90,15,0,7,0]}
            ]}]
    })).unwrap()).unwrap();
}

fn deferred_object_fixture(tail: bool) -> (TeletextDeclaredService, TeletextObjectIdentity) {
    let mut draft = input();
    let parent = TeletextObjectIdentity::local(owner(0), TeletextObjectId::new(65534));
    let child = TeletextObjectIdentity::local(owner(0), TeletextObjectId::new(65535));
    let call = TeletextEnhancementCommand::InvokeObject {
        target: child,
        object_type: TeletextObjectType::Passive,
        offset: None,
    };
    let mut body = vec![call; 15];
    if tail {
        body.extend([
            TeletextEnhancementCommand::SetPosition {
                position: TeletextEnhancementPosition::new(24, 0).unwrap(),
            },
            TeletextEnhancementCommand::AtColumn {
                column: 0,
                operation: TeletextColumnOperation::G0 {
                    code: TeletextGlyphCode::new(b'A').unwrap(),
                },
            },
        ]);
        body.extend([call; 15]);
    }
    draft.objects.extend([
        TeletextEnhancementObjectDefinition {
            identity: parent,
            object_type: TeletextObjectType::Active,
            program: TeletextEnhancementProgram::new(body).unwrap(),
        },
        TeletextEnhancementObjectDefinition {
            identity: child,
            object_type: TeletextObjectType::Passive,
            program: TeletextEnhancementProgram::new(vec![]).unwrap(),
        },
    ]);
    draft.pages[0].local_program = Some(
        TeletextEnhancementProgram::new(vec![TeletextEnhancementCommand::InvokeObject {
            target: parent,
            object_type: TeletextObjectType::Active,
            offset: Some(TeletextObjectOffset::new(1, 0).unwrap()),
        }])
        .unwrap(),
    );
    (declared(draft), parent)
}

#[test]
fn whole_factory_later_unplaced_write_cannot_erase_ancestor_prefix_refusal() {
    for tail in [false, true] {
        let (source, parent) = deferred_object_fixture(tail);
        let error = super::empty_admission::original_successive_refusal(&source);
        assert_eq!(
            error.location,
            Some(TeletextEnhancementSource {
                object: Some(parent),
                command: 14
            })
        );
        assert_eq!(
            error.root_source,
            Some(TeletextSpatialRootSource::Local { command: 0 })
        );
        assert_eq!(error.invocation.unwrap().get(), 15);
        assert_eq!(error.position, Some((1, 0)));
    }
}
