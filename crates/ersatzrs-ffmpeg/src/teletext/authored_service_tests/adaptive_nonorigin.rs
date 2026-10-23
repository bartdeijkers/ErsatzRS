//! C.3 inherited width keeps the earlier lower origin, not an unknown overlay.
use super::*;

fn at(column: u8, operation: TeletextColumnOperation) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn { column, operation }
}

fn position(column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(1, column).unwrap(),
    }
}

fn foreground(column: u8, colour: u8) -> TeletextEnhancementCommand {
    at(
        column,
        TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(colour).unwrap(),
        },
    )
}

fn glyph(column: u8, code: u8) -> TeletextEnhancementCommand {
    at(
        column,
        TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(code).unwrap(),
        },
    )
}

fn fixture() -> (TeletextDeclaredService, TeletextObjectIdentity) {
    let identity = TeletextObjectIdentity::local(owner(0), TeletextObjectId::new(65535));
    let mut draft = input();
    draft.objects.push(TeletextEnhancementObjectDefinition {
        identity,
        object_type: TeletextObjectType::Adaptive,
        program: TeletextEnhancementProgram::new(vec![foreground(0, 2), glyph(0, b'A')]).unwrap(),
    });
    draft.pages[0].local_program = Some(
        TeletextEnhancementProgram::new(vec![
            position(8),
            foreground(8, 1),
            at(
                8,
                TeletextColumnOperation::Attributes {
                    double_height: false,
                    double_width: true,
                    boxed: false,
                    conceal: false,
                    invert: false,
                    underline_or_separated_mosaics: false,
                },
            ),
            glyph(8, b'L'),
            position(9),
            TeletextEnhancementCommand::InvokeObject {
                target: identity,
                object_type: TeletextObjectType::Adaptive,
                offset: None,
            },
        ])
        .unwrap(),
    );
    (declared(draft), identity)
}

#[test]
fn whole_factory_preserves_inherited_width_nonorigin_and_exact_fallback() {
    let (source, identity) = fixture();
    let encoded =
        encode_teletext_authored_service(source.clone(), &transmissions(&source), &limits())
            .expect("C3 inherited width has a definite earlier lower origin");
    assert_eq!(encoded.declared(), &source);
    let EncodedTeletextAuthoredPage::CompiledDisplay(page) = &encoded.pages()[0] else {
        panic!("the complete original Main and object must be compiled")
    };
    for resolution in [
        page.normalized().original().display_resolution(),
        page.normalized().physical().replay().display_resolution(),
    ] {
        let left = resolution.cell(1, 8).unwrap();
        let right = resolution.cell(1, 9).unwrap();
        assert_eq!(left.origin(), right.origin());
        assert_eq!(left.fragment(), TeletextDisplayFragment::DoubleWidthLeft);
        assert_eq!(right.fragment(), TeletextDisplayFragment::DoubleWidthRight);
        for cell in [left, right] {
            assert_eq!(cell.attributes().foreground().value().unwrap().get(), 1);
            assert_eq!(cell.attributes().double_width().value(), Some(true));
        }
    }
    let original = page.normalized().original();
    let call = original.spatial_trace().invocations()[0];
    assert_eq!(call.target(), identity);
    assert_eq!(call.parent(), None);
    assert_eq!(
        call.root_source(),
        TeletextSpatialRootSource::Local { command: 5 }
    );
    let resolution = original.display_resolution();
    let hidden = resolution
        .origins()
        .iter()
        .find(|origin| {
            original.glyph_layers().candidates()[origin.candidate_index()].source()
                == Some(TeletextEnhancementSource {
                    object: Some(identity),
                    command: 1,
                })
        })
        .unwrap();
    assert_eq!(hidden.pending(), None);
    assert_eq!(
        hidden.obscured_by(),
        resolution.cell(1, 9).unwrap().origin()
    );
    let fallback = encode_teletext_page(
        &source.semantic().base().pages[0],
        TeletextEncodingRegion::WesternLatin,
    )
    .unwrap();
    assert_eq!(
        page.page()
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
    native_calls_and_body(page.page());
    export(page.page());
}

fn native_calls_and_body(page: &EncodedTeletextPage) {
    let projection = project_encoded_teletext_display(page, 128).unwrap();
    let words = projection
        .records
        .iter()
        .filter_map(|record| match record {
            TeletextEncodedDisplayRecord::Triplets { row: 26, words, .. } => Some(words),
            _ => None,
        })
        .flatten()
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(
        words
            .iter()
            .filter(|&&word| word & 0x3f >= 40 && (word >> 6) & 0x1f == 0x12)
            .count(),
        1
    );
    let definition = words
        .iter()
        .position(|&word| word & 0x3f >= 40 && (word >> 6) & 0x1f == 0x16)
        .unwrap();
    assert_eq!(
        &words[definition + 1..definition + 3],
        &[2 << 11, (u32::from(b'A') << 11) | (9 << 6),],
        "the complete native Adaptive body must survive even though its glyph is obscured"
    );
}

fn export(page: &EncodedTeletextPage) {
    let Some(directory) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_ADAPTIVE_NONORIGIN_DIR"])
    else {
        return;
    };
    let units = page
        .data_units()
        .iter()
        .map(|unit| unit.to_vec())
        .collect::<Vec<_>>();
    let value = serde_json::json!({"expected_version":"0.2.0", "cases":[{
    "name":"adaptive-inherited-width", "page":0x200, "subcode":0,
    "units":units, "cells":[
        {"row":1,"column":8,"l15":[90,15,0,7,0,0],"l25":[76,0,0,1,0,3]},
        {"row":1,"column":9,"l15":[90,15,0,7,0,0],"l25":[76,0,0,1,0,4]}
    ]}]});
    std::fs::write(
        std::path::Path::new(&directory).join("native.json"),
        serde_json::to_vec(&value).unwrap(),
    )
    .unwrap();
}
