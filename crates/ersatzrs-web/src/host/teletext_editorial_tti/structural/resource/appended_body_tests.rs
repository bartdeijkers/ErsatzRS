//! Exact-prefix trailing rows keep all sealed semantic anchors and ordinals.
use super::first_body_tests::{empty, location, word};
use super::*;
use ersatzrs_domain::TeletextGlyphCode;
use std::time::Duration;

fn populated() -> (TeletextEnhancementObjectDefinition, Vec<Span>, Vec<u32>) {
    let mut definition = empty();
    let words = vec![word(41, 4, 0), word(8, 9, 80)];
    definition.program = parse_first_body(
        &definition,
        &[],
        (&[], &words),
        8,
        location(),
        Instant::now() + Duration::from_secs(5),
    )
    .unwrap()
    .replacement;
    let spans = vec![
        Span {
            ordinal: 0,
            first: 0,
            word: 0,
        },
        Span {
            ordinal: 1,
            first: 1,
            word: 1,
        },
    ];
    (definition, spans, words)
}

#[test]
fn appended_body_preserves_original_positions_and_appends_checked_rows() {
    let (definition, spans, before) = populated();
    let after = [
        before.clone(),
        vec![
            word(42, 4, 0),
            word(4, 9, 81),
            word(40, 4, 39),
            word(39, 9, 82),
        ],
    ]
    .concat();
    let parsed = parse_appended_body(
        &definition,
        &spans,
        (&before, &after),
        8,
        location(),
        Instant::now() + Duration::from_secs(5),
    )
    .unwrap()
    .unwrap();
    assert_eq!(parsed.retained, [Some(0), Some(1), None, None, None, None]);
    assert_eq!(
        &parsed.replacement.commands()[..2],
        definition.program.commands()
    );
    assert_eq!(
        parsed.replacement.commands()[2],
        TeletextEnhancementCommand::SetPosition {
            position: TeletextEnhancementPosition::new(2, 0).unwrap()
        }
    );
    assert_eq!(
        parsed.replacement.commands()[4],
        TeletextEnhancementCommand::SetPosition {
            position: TeletextEnhancementPosition::new(24, 39).unwrap()
        }
    );
}

#[test]
fn appended_body_retains_hidden_positions_and_semantic_budget() {
    let mut definition = empty();
    definition.program = TeletextEnhancementProgram::new(vec![
        TeletextEnhancementCommand::SetPosition {
            position: TeletextEnhancementPosition::new(0, 8).unwrap(),
        },
        TeletextEnhancementCommand::AtColumn {
            column: 8,
            operation: TeletextColumnOperation::G0 {
                code: TeletextGlyphCode::new(80).unwrap(),
            },
        },
    ])
    .unwrap();
    let spans = [Span {
        ordinal: 1,
        first: 0,
        word: 0,
    }];
    let before = [word(8, 9, 80)];
    let after = [before[0], word(41, 4, 0), word(4, 9, 81)];
    let deadline = Instant::now() + Duration::from_secs(5);
    let parsed = parse_appended_body(
        &definition,
        &spans,
        (&before, &after),
        4,
        location(),
        deadline,
    )
    .unwrap()
    .unwrap();
    assert_eq!(parsed.retained, [Some(0), Some(1), None, None]);
    assert_eq!(
        &parsed.replacement.commands()[..2],
        definition.program.commands()
    );
    assert!(parse_appended_body(
        &definition,
        &spans,
        (&before, &after),
        3,
        location(),
        deadline
    )
    .is_err());
}

#[test]
fn appended_body_requires_exact_prefix_span_coverage_and_real_new_position() {
    let (definition, spans, before) = populated();
    let deadline = Instant::now() + Duration::from_secs(5);
    for after in [
        vec![word(41, 4, 1), before[1], word(42, 4, 0), word(4, 9, 81)],
        vec![before[1], word(42, 4, 0), word(4, 9, 81)],
        vec![before[0], before[1], word(9, 9, 81)],
    ] {
        assert!(parse_appended_body(
            &definition,
            &spans,
            (&before, &after),
            8,
            location(),
            deadline
        )
        .unwrap()
        .is_none());
    }
    let after = [before.clone(), vec![word(42, 4, 0), word(4, 9, 81)]].concat();
    assert!(parse_appended_body(
        &definition,
        &spans[..1],
        (&before, &after),
        8,
        location(),
        deadline
    )
    .is_err());
    assert!(parse_appended_body(
        &definition,
        &spans,
        (&before, &after),
        8,
        location(),
        Instant::now()
    )
    .is_err());
    let invalid = [
        before.clone(),
        vec![word(42, 4, 40), word(43, 4, 0), word(4, 9, 81)],
    ]
    .concat();
    assert!(parse_appended_body(
        &definition,
        &spans,
        (&before, &invalid),
        8,
        location(),
        deadline
    )
    .is_err());
    let oversized = [before.clone(), vec![word(42, 4, 0); 506]].concat();
    assert!(parse_appended_body(
        &definition,
        &spans,
        (&before, &oversized),
        1024,
        location(),
        deadline
    )
    .is_err());
}
