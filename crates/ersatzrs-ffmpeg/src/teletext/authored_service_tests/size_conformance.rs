//! Whole-factory C.3 reduction and illegal-boundary evidence from original cases.
use super::*;

fn position(row: u8, column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(row, column).unwrap(),
    }
}

fn size(column: u8, height: bool, width: bool) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column,
        operation: TeletextColumnOperation::Attributes {
            double_height: height,
            double_width: width,
            boxed: false,
            conceal: false,
            invert: false,
            underline_or_separated_mosaics: false,
        },
    }
}

fn glyph(column: u8, code: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column,
        operation: TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(code).unwrap(),
        },
    }
}

fn source(commands: Vec<TeletextEnhancementCommand>) -> TeletextDeclaredService {
    let mut draft = input();
    // Match the original domain witness's blank fallback, not another source.
    draft.base.pages[0].rows.clear();
    draft.pages[0].local_program = Some(TeletextEnhancementProgram::new(commands).unwrap());
    declared(draft)
}

#[test]
fn whole_factory_double_size_conflict_preserves_unconflicted_height_and_fallback() {
    let source = source(vec![
        position(1, 9),
        size(9, true, false),
        glyph(9, b'A'),
        size(10, false, false),
        position(2, 8),
        size(8, true, true),
        glyph(8, b'B'),
        size(9, false, false),
    ]);
    let encoded =
        encode_teletext_authored_service(source.clone(), &transmissions(&source), &limits())
            .expect("C3 requires reduction of only the conflicting width");
    assert_eq!(encoded.declared(), &source);
    let EncodedTeletextAuthoredPage::CompiledDisplay(page) = &encoded.pages()[0] else {
        panic!("the complete original size program must be compiled")
    };
    for resolution in [
        page.normalized().original().display_resolution(),
        page.normalized().physical().replay().display_resolution(),
    ] {
        assert!(resolution.pending().is_empty());
        let top = resolution.cell(2, 8).unwrap();
        let bottom = resolution.cell(3, 8).unwrap();
        assert_eq!(top.origin(), bottom.origin());
        assert_eq!(top.fragment(), TeletextDisplayFragment::DoubleHeightTop);
        assert_eq!(
            bottom.fragment(),
            TeletextDisplayFragment::DoubleHeightBottom
        );
        let origin = &resolution.origins()[top.origin().unwrap()];
        assert_eq!(
            origin.requested_size(),
            Some(TeletextDisplaySize::DoubleSize)
        );
        assert_eq!(
            origin.effective_size(),
            Some(TeletextDisplaySize::DoubleHeight)
        );
        assert_eq!(
            resolution.cell(1, 9).unwrap().origin(),
            resolution.cell(2, 9).unwrap().origin()
        );
        assert_ne!(resolution.cell(3, 9).unwrap().origin(), top.origin());
    }
    let original = page.normalized().original();
    let resolution = original.display_resolution();
    let origin = resolution.origins()[resolution.cell(2, 8).unwrap().origin().unwrap()];
    let candidate = original.glyph_layers().candidates()[origin.candidate_index()];
    assert_eq!(
        candidate.source(),
        Some(TeletextEnhancementSource {
            object: None,
            command: 6
        })
    );
    assert_eq!(
        candidate.root_source(),
        Some(TeletextSpatialRootSource::Local { command: 6 })
    );
    assert_eq!(candidate.invocation(), None);
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
    let projection = project_encoded_teletext_display(page.page(), 128).unwrap();
    assert!(
        projection.records.iter().any(|record| matches!(record,
        TeletextEncodedDisplayRecord::Triplets {row:26,words,..}
        if words.contains(&((65 << 11) | (12 << 6) | 8)))),
        "native words must retain requested double-size rather than coercing the source"
    );
    export(page.page());
}

#[test]
fn whole_factory_illegal_size_boundaries_retain_exact_original_refusals() {
    for (row, column, height, width) in [(23, 8, true, false), (1, 39, false, true)] {
        let source = source(vec![
            position(row, column),
            size(column, height, width),
            glyph(column, b'A'),
        ]);
        let result =
            encode_teletext_authored_service(source.clone(), &transmissions(&source), &limits());
        let error = result
            .expect_err("illegal authoring may not silently choose a decoder's partial rendering");
        assert_eq!(error.display, Some(owner(0)));
        let Failure::Compiled(error) = error.reason else {
            panic!("located original compilation refusal")
        };
        let TeletextCompiledDisplayPageEncodingFailure::Normalization(error) = error.reason else {
            panic!("original normalization refusal")
        };
        let TeletextNormalizedMainProgramFailure::DisplayMismatch(error) = error.reason else {
            panic!("original invalid display refusal")
        };
        assert!(matches!(
            error.finding,
            TeletextPhysicalComparisonFinding::Pending {
                side: TeletextPhysicalComparisonSide::Original,
                kind: TeletextPhysicalComparisonPendingKind::Origin,
                index: Some(_),
            }
        ));
        assert_eq!(error.position, Some((row, column)));
        assert_eq!(
            error.original.raw_position,
            Some((u16::from(row), u16::from(column)))
        );
        assert_eq!(
            error.original.location,
            Some(TeletextEnhancementSource {
                object: None,
                command: 2
            })
        );
        assert_eq!(
            error.original.root_source,
            Some(TeletextSpatialRootSource::Local { command: 2 })
        );
        assert_eq!(error.original.invocation, None);
    }
}

fn export(page: &EncodedTeletextPage) {
    let Some(directory) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_SIZE_CONFORMANCE_DIR"])
    else {
        return;
    };
    let cells = [
        (1, 9, 65, 1),
        (2, 9, 65, 2),
        (2, 8, 66, 1),
        (3, 8, 66, 2),
        (3, 9, 32, 0),
    ]
    .map(|(row, column, code, fragment)| {
        serde_json::json!({"row":row,"column":column,"l15":[32,15,0,7,0,0],
            "l25":[code,if code==32 {15} else {0},0,7,0,fragment]})
    });
    let value = serde_json::json!({"expected_version":"0.2.0","cases":[{
        "name":"double-size-conflict-reduction","page":512,"subcode":0,
        "units":page.data_units().iter().map(|u|u.to_vec()).collect::<Vec<_>>(),"cells":cells
    }]});
    std::fs::write(
        std::path::Path::new(&directory).join("native.json"),
        serde_json::to_vec(&value).unwrap(),
    )
    .unwrap();
}
