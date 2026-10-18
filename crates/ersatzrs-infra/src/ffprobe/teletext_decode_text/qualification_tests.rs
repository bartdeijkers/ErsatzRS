use super::*;

fn frame(pts: i64, nonempty: bool) -> Frame {
    Frame {
        media_type: "subtitle".into(),
        pts: Some(pts),
        format: 1,
        num_rects: u32::from(nonempty),
        start_display_time: 0,
        end_display_time: u32::MAX,
    }
}

fn deadline() -> Instant {
    Instant::now() + std::time::Duration::from_secs(5)
}

#[test]
fn source_origin_zero_cue_keeps_submillisecond_bounds_but_not_earlier_or_ambiguous_cues() {
    let frames = [frame(1_000_000, true), frame(2_000_000, false)];
    assert_eq!(
        expected(&frames, (999_999, 1_000_001), deadline()).unwrap(),
        [(0, 1000)]
    );
    assert!(expected(&frames, (1_001_000, 1_001_000), deadline()).is_err());
    assert!(expected(&frames, (1_000_499, 1_000_501), deadline()).is_err());
}

#[test]
fn ordered_text_cues_require_every_nonempty_event_and_exact_clear_boundary() {
    let frames = [
        frame(2_000_000, true),
        frame(3_000_000, true),
        frame(4_000_000, false),
    ];
    let expected = expected(&frames, (999_999, 1_000_001), deadline()).unwrap();
    assert_eq!(expected, [(1000, 2000), (2000, 3000)]);
    let text = "1\n00:00:01,000 --> 00:00:02,000\nA\n\n2\n00:00:02,000 --> 00:00:03,000\nB\n\n";
    assert_eq!(cues(text).unwrap(), expected);
    assert_ne!(
        cues("1\n00:00:01,000 --> 00:00:02,000\nA\n\n").unwrap(),
        expected
    );
    assert_ne!(
        cues(&text.replace("00:00:03,000", "00:00:03,001")).unwrap(),
        expected
    );
    assert_ne!(
        cues(&text.replace("00:00:01,000", "00:00:00,000")).unwrap(),
        expected
    );
}

#[test]
fn eof_unknown_last_cue_reordering_missing_pts_and_ambiguous_rounding_refuse() {
    assert!(expected(&[frame(2_000_000, true)], (999_999, 1_000_001), deadline()).is_err());
    assert!(expected(
        &[frame(3_000_000, true), frame(2_000_000, false)],
        (0, 0),
        deadline()
    )
    .is_err());
    let mut missing = frame(2_000_000, true);
    missing.pts = None;
    assert!(expected(&[missing, frame(3_000_000, false)], (0, 0), deadline()).is_err());
    assert!(expected(
        &[frame(2_000_500, true), frame(3_000_500, false)],
        (-1, 1),
        deadline()
    )
    .is_err());
    assert!(expected(
        &[frame(2_000_000, true), frame(3_000_000, false)],
        (0, 0),
        Instant::now()
    )
    .is_err());
    let frames = (0..=super::super::super::MAX_FRAMES)
        .map(|i| frame(i64::try_from(i).unwrap() * 1000, true))
        .collect::<Vec<_>>();
    assert!(expected(&frames, (0, 0), deadline()).is_err());
}

#[test]
fn clear_and_finite_final_event_never_authorize_a_wrong_format_or_malformed_srt() {
    let mut finite = frame(2_000_000, true);
    finite.end_display_time = 500;
    assert_eq!(
        expected(&[finite], (1_000_000, 1_000_000), deadline()).unwrap(),
        [(1000, 1500)]
    );
    let mut bitmap = frame(2_000_000, true);
    bitmap.format = 0;
    assert!(expected(&[bitmap, frame(3_000_000, false)], (0, 0), deadline()).is_err());
    for text in [
        "not subtitle cues",
        "1\n00:00:01,000 --> 00:60:01,000\nA\n\n",
        "1\n00:00:01,000 --> 00:00:02,000\n",
    ] {
        assert!(cues(text).is_err());
    }
}
