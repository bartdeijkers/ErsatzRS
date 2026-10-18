use super::super::first_body_tests::{empty, location, word};
use super::super::{parse_first_body, Span};
use super::{parse_inserted_rows, Program, Result, TeletextEnhancementCommand};
use ersatzrs_domain::{
    TeletextEnhancementObjectDefinition, TeletextEnhancementPosition, TeletextEnhancementProgram,
};
use std::time::{Duration, Instant};

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
}
fn fixture() -> (TeletextEnhancementObjectDefinition, Vec<Span>, Vec<u32>) {
    let words = vec![
        word(41, 4, 0),
        word(8, 9, 80),
        word(43, 4, 0),
        word(4, 9, 81),
        word(46, 4, 0),
        word(2, 9, 82),
    ];
    let mut definition = empty();
    definition.program =
        parse_first_body(&definition, &[], (&[], &words), 16, location(), deadline())
            .unwrap()
            .replacement;
    let spans = (0..words.len())
        .map(|ordinal| Span {
            ordinal,
            first: ordinal,
            word: ordinal,
        })
        .collect();
    (definition, spans, words)
}
fn parse(
    definition: &TeletextEnhancementObjectDefinition,
    spans: &[Span],
    before: &[u32],
    after: &[u32],
    maximum: usize,
) -> Result<Option<Program>> {
    parse_inserted_rows(
        definition,
        spans,
        (before, after),
        maximum,
        location(),
        deadline(),
    )
}
fn inserted(before: &[u32]) -> Vec<u32> {
    [
        before[..2].to_vec(),
        vec![word(42, 4, 0), word(39, 9, 88)],
        before[2..4].to_vec(),
        vec![
            word(44, 4, 0),
            word(0, 0, 2),
            word(0, 9, 89),
            word(45, 4, 39),
            word(39, 9, 90),
        ],
        before[4..].to_vec(),
    ]
    .concat()
}

#[test]
fn inserted_rows_multiple_gaps_keep_every_exact_semantic_survivor() {
    let (definition, spans, before) = fixture();
    let parsed = parse(&definition, &spans, &before, &inserted(&before), 13)
        .unwrap()
        .unwrap();
    assert_eq!(
        parsed.retained,
        [
            Some(0),
            Some(1),
            None,
            None,
            Some(2),
            Some(3),
            None,
            None,
            None,
            None,
            None,
            Some(4),
            Some(5)
        ]
    );
    for (command, retained) in parsed.replacement.commands().iter().zip(&parsed.retained) {
        if let Some(ordinal) = retained {
            assert_eq!(*command, definition.program.commands()[*ordinal]);
        }
    }
    assert_eq!(
        parsed.replacement.commands()[2],
        TeletextEnhancementCommand::SetPosition {
            position: TeletextEnhancementPosition::new(2, 0).unwrap()
        }
    );
    assert_eq!(
        parsed.replacement.commands()[9],
        TeletextEnhancementCommand::SetPosition {
            position: TeletextEnhancementPosition::new(5, 39).unwrap()
        }
    );
}

#[test]
fn inserted_rows_preserve_hidden_zero_anchors_and_row24_column39() {
    let (mut definition, mut spans, before) = fixture();
    let mut commands = definition.program.commands().to_vec();
    commands.insert(
        0,
        TeletextEnhancementCommand::SetPosition {
            position: TeletextEnhancementPosition::new(0, 8).unwrap(),
        },
    );
    definition.program = TeletextEnhancementProgram::new(commands).unwrap();
    for span in &mut spans {
        span.ordinal += 1;
    }
    let parsed = parse(&definition, &spans, &before, &inserted(&before), 14)
        .unwrap()
        .unwrap();
    assert_eq!(parsed.retained[0], Some(0));
    assert_eq!(
        parsed.replacement.commands()[0],
        definition.program.commands()[0]
    );
    let before = [
        word(41, 4, 0),
        word(8, 9, 80),
        word(40, 4, 39),
        word(39, 9, 81),
    ];
    let mut definition = empty();
    definition.program =
        parse_first_body(&definition, &[], (&[], &before), 8, location(), deadline())
            .unwrap()
            .replacement;
    let spans = (0..4)
        .map(|ordinal| Span {
            ordinal,
            first: ordinal,
            word: ordinal,
        })
        .collect::<Vec<_>>();
    let after = [
        before[..2].to_vec(),
        vec![word(63, 4, 39), word(39, 9, 82)],
        before[2..].to_vec(),
    ]
    .concat();
    let parsed = parse(&definition, &spans, &before, &after, 6)
        .unwrap()
        .unwrap();
    assert_eq!(
        parsed.retained,
        [Some(0), Some(1), None, None, Some(2), Some(3)]
    );
    assert_eq!(
        parsed.replacement.commands()[4],
        definition.program.commands()[2]
    );
}

#[test]
fn inserted_rows_leave_scalar_first_and_append_routes_unchanged() {
    let (definition, spans, before) = fixture();
    for after in [
        before.clone(),
        {
            let mut v = before.clone();
            v[1] = word(8, 9, 81);
            v
        },
        [before.clone(), vec![word(47, 4, 0), word(4, 9, 81)]].concat(),
    ] {
        assert!(parse(&definition, &spans, &before, &after, 16)
            .unwrap()
            .is_none());
    }
    assert!(
        parse(&empty(), &[], &[], &[word(41, 4, 0), word(8, 9, 80)], 16)
            .unwrap()
            .is_none()
    );
}

#[test]
fn inserted_rows_reject_changed_dropped_reordered_survivors_after_new_row() {
    let (definition, spans, before) = fixture();
    let good = inserted(&before);
    for after in [
        {
            let mut v = good.clone();
            v[4] = word(43, 4, 1);
            v
        },
        {
            let mut v = good.clone();
            v.remove(5);
            v
        },
        {
            let mut v = good.clone();
            v.swap(4, 5);
            v
        },
        {
            let mut v = good.clone();
            v[12] = word(2, 9, 83);
            v
        },
    ] {
        assert!(parse(&definition, &spans, &before, &after, 20).is_err());
    }
    assert!(parse(&definition, &spans[..5], &before, &good, 20).is_err());
    let mut sparse = spans;
    sparse[2].first = 1;
    assert!(parse(&definition, &sparse, &before, &good, 20).is_err());
}

#[test]
fn inserted_rows_reject_invalid_ap_empty_rows_and_unsupported_context() {
    let (definition, spans, before) = fixture();
    for block in [
        vec![word(42, 4, 40), word(8, 9, 80)],
        vec![word(42, 4, 0)],
        vec![word(42, 4, 0), word(42, 4, 1), word(8, 9, 80)],
        vec![word(42, 4, 0), word(41, 4, 0), word(8, 9, 80)],
        vec![word(42, 4, 0), word(39, 4, 0), word(8, 9, 80)],
        vec![word(42, 4, 0), word(63, 7, 0), word(8, 9, 80)],
        vec![word(42, 4, 0), word(40, 16, 0), word(40, 19, 20)],
        vec![word(42, 4, 0), word(40, 24, 0x50), word(8, 9, 80)],
    ] {
        let after = [before[..2].to_vec(), block, before[2..].to_vec()].concat();
        assert!(parse(&definition, &spans, &before, &after, 32).is_err());
    }
}

#[test]
fn inserted_rows_bound_complete_semantics_physical_words_and_deadline() {
    let (definition, spans, before) = fixture();
    let after = inserted(&before);
    assert!(parse(&definition, &spans, &before, &after, 12).is_err());
    assert!(parse(&definition, &spans, &before, &after, usize::MAX).is_err());
    assert!(parse_inserted_rows(
        &definition,
        &spans,
        (&before, &after),
        16,
        location(),
        Instant::now()
    )
    .is_err());
    let after = [
        before[..2].to_vec(),
        vec![word(42, 4, 0); 502],
        before[2..].to_vec(),
    ]
    .concat();
    assert!(parse(&definition, &spans, &before, &after, 1024).is_err());
}

#[test]
fn inserted_rows_reject_selector_latch_interference_with_exact_old_raw_words() {
    let selector = |subtable: u32| word(40, 24, 0x50 | subtable);
    let drcs = word(8, 13, 0x40);
    let before = [word(41, 4, 0), selector(0), drcs, word(43, 4, 0), drcs];
    let mut definition = empty();
    definition.program =
        parse_first_body(&definition, &[], (&[], &before), 8, location(), deadline())
            .unwrap()
            .replacement;
    let spans = [
        Span {
            ordinal: 0,
            first: 0,
            word: 0,
        },
        Span {
            ordinal: 1,
            first: 1,
            word: 2,
        },
        Span {
            ordinal: 2,
            first: 3,
            word: 3,
        },
        Span {
            ordinal: 3,
            first: 4,
            word: 4,
        },
    ];
    let build = |subtable| {
        [
            before[..3].to_vec(),
            vec![word(42, 4, 0), selector(subtable), drcs],
            before[3..].to_vec(),
        ]
        .concat()
    };
    assert!(parse(&definition, &spans, &before, &build(0), 8)
        .unwrap()
        .is_some());
    assert!(parse(&definition, &spans, &before, &build(1), 8).is_err());
}

#[test]
fn inserted_rows_hidden_column_anchor_does_not_reset_the_last_physical_row() {
    let (mut definition, mut spans, before) = fixture();
    let mut commands = definition.program.commands().to_vec();
    commands.insert(
        4,
        TeletextEnhancementCommand::SetPosition {
            position: TeletextEnhancementPosition::new(0, 4).unwrap(),
        },
    );
    definition.program = TeletextEnhancementProgram::new(commands).unwrap();
    for span in &mut spans {
        if span.ordinal >= 4 {
            span.ordinal += 1;
        }
    }
    let build = |row: u32| {
        [
            before[..4].to_vec(),
            vec![word(40 + row, 4, 0), word(4, 9, 88)],
            before[4..].to_vec(),
        ]
        .concat()
    };
    assert!(parse(&definition, &spans, &before, &build(2), 12).is_err());
    let parsed = parse(&definition, &spans, &before, &build(4), 12)
        .unwrap()
        .unwrap();
    assert_eq!(
        parsed.retained,
        [
            Some(0),
            Some(1),
            Some(2),
            Some(3),
            Some(4),
            None,
            None,
            Some(5),
            Some(6)
        ]
    );
    assert_eq!(
        parsed.replacement.commands()[4],
        definition.program.commands()[4]
    );
}
