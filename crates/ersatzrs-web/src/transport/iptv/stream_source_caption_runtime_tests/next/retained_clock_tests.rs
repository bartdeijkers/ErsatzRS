//! Scheduling lookahead may differ; delivered timestamps and the next witness may not.

use super::*;

fn frames(count: usize) -> Vec<i64> {
    (0..count)
        .map(|n| 54 + i64::try_from(n).unwrap() * FRAME_TICKS)
        .collect()
}

#[test]
fn retained_clock_join_accepts_only_unrepresented_lookahead_differences() {
    let output = frames(7);
    let represented = 3;
    for raw_count in [4, 7, 9] {
        let raw = frames(raw_count);
        let filtered = (5..5 + i64::try_from(raw_count).unwrap()).collect::<Vec<_>>();
        assert!(filter_prefixes_match(&raw, &output, &filtered, represented));
    }
}

#[test]
fn retained_clock_join_rejects_missing_delivered_or_next_witness_and_clock_changes() {
    let output = frames(7);
    let raw = frames(4);
    let filtered = vec![5, 6, 7, 8];
    for represented in [0, 1, 4, 7] {
        assert!(!filter_prefixes_match(
            &raw,
            &output,
            &filtered,
            represented
        ));
    }
    assert!(!filter_prefixes_match(
        &raw[..2],
        &output,
        &filtered[..2],
        3
    ));
    assert!(!filter_prefixes_match(
        &raw[..3],
        &output,
        &filtered[..3],
        3
    ));
    assert!(!filter_prefixes_match(&raw, &output[..3], &filtered, 3));
    assert!(!filter_prefixes_match(&raw, &output, &filtered[..3], 3));
    assert!(!filter_prefixes_match(&[], &output, &[], 3));
    for index in 0..raw.len() {
        let mut changed = raw.clone();
        changed[index] += 1;
        assert!(!filter_prefixes_match(&changed, &output, &filtered, 3));
    }
    for index in 0..output.len() {
        let mut changed = output.clone();
        changed[index] += 1;
        assert!(!filter_prefixes_match(&raw, &changed, &filtered, 3));
    }
    // Even unrepresented raw suffixes must preserve the exact source cadence.
    let mut long = frames(9);
    long[8] += FRAME_TICKS;
    assert!(!filter_prefixes_match(&long, &output, &[0; 9], 3));
    assert!(!filter_prefixes_match(
        &frames(MAX_FRAMES + 1),
        &output,
        &[0; MAX_FRAMES + 1],
        3
    ));
}
