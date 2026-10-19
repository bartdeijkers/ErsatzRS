use super::*;

fn deadline() -> Instant {
    Instant::now() + std::time::Duration::from_secs(5)
}

fn frame(pts: i64) -> Frame {
    Frame {
        media_type: "subtitle".into(),
        pts: Some(pts),
        format: 1,
        num_rects: 1,
        start_display_time: 0,
        end_display_time: u32::MAX,
    }
}

#[test]
fn final_native_text_recovery_requires_exact_prefix_clocks_text_and_cardinality() {
    let frames = [frame(1_000_000), frame(2_000_000)];
    let fixed = "1\n00:00:01,000 --> 00:00:02,000\nEARLY\n\n";
    let raw = "1\n00:00:01,000 --> 1193:02:48,295\nEARLY\n\n2\n00:00:02,000 --> 1193:02:49,295\nFINAL\n\n";
    let result = recover_events(&frames, (0, 0), fixed, raw, 4000, deadline()).unwrap();
    assert_eq!(
        result,
        "1\n00:00:01,000 --> 00:00:02,000\nEARLY\n\n2\n00:00:02,000 --> 00:00:04,000\nFINAL\n\n"
    );
    for wrong in [
        raw.replace("EARLY", "OTHER"),
        raw.replace("00:00:02,000", "00:00:02,001"),
        raw.replace("49,295", "49,294"),
        raw.split("\n\n").next().unwrap().to_owned(),
    ] {
        assert!(recover_events(&frames, (0, 0), fixed, &wrong, 4000, deadline()).is_err());
    }
    assert!(recover_events(
        &frames,
        (0, 0),
        &fixed.replace("02,000", "02,001"),
        raw,
        4000,
        deadline()
    )
    .is_err());
    assert!(recover_events(&frames, (0, 0), fixed, raw, 2000, deadline()).is_err());
    assert!(recover_events(&frames, (0, 0), fixed, raw, 1999, deadline()).is_err());
    assert!(recover_events(&frames, (0, 0), fixed, raw, 4000, Instant::now()).is_err());
}

#[test]
fn a_single_unclosed_cue_needs_no_manufactured_prior_cue() {
    let raw = "1\n00:00:01,000 --> 1193:02:48,295\nFINAL\n\n";
    assert_eq!(
        recover_events(&[frame(1_000_000)], (0, 0), "", raw, 2000, deadline()).unwrap(),
        "1\n00:00:01,000 --> 00:00:02,000\nFINAL\n\n"
    );
    let mut finite = frame(1_000_000);
    finite.end_display_time = 500;
    assert!(recover_events(&[finite], (0, 0), "", raw, 2000, deadline()).is_err());
}

#[test]
fn recovery_compares_actual_normalized_srt_bodies_including_the_last_fixed_block() {
    let normalize = super::super::super::normalize;
    let fixed = normalize(b"1\n00:00:01,000 --> 00:00:02,000\nEARLY\n\n".to_vec()).unwrap();
    let raw = normalize(b"1\n00:00:01,000 --> 1193:02:48,295\nEARLY\n\n2\n00:00:02,000 --> 1193:02:49,295\nFINAL\n\n".to_vec()).unwrap();
    assert!(recover_events(
        &[frame(1_000_000), frame(2_000_000)],
        (0, 0),
        &fixed,
        &raw,
        4000,
        deadline()
    )
    .is_ok());
}

fn video() -> VideoOutput {
    serde_json::from_value(serde_json::json!({
        "streams":[{"index":3,"codec_type":"video","id":"0x104","time_base":"1/90000"}],
        "programs":[{"program_num":7,"streams":[{"index":3,"codec_type":"video","id":"0x104","time_base":"1/90000"}]}],
        "frames":[{"media_type":"video","stream_index":3,"pts":90000,"duration":3600},
                  {"media_type":"video","stream_index":3,"pts":93600,"duration":3600}]
    })).unwrap()
}

#[test]
fn selected_video_end_uses_decoded_pts_plus_duration_in_the_exact_source_clock() {
    let output = video();
    assert_eq!(
        end(&output, 3, 260, 7, (999_999, 1_000_001), deadline()).unwrap(),
        80
    );
    assert!(end(&output, 0, 260, 7, (0, 0), deadline()).is_err());
    assert!(end(&output, 3, 256, 7, (0, 0), deadline()).is_err());
    assert!(end(&output, 3, 260, 1, (0, 0), deadline()).is_err());
    assert!(end(&output, 3, 260, 7, (999_499, 999_501), deadline()).is_err());
    assert!(end(&output, 3, 260, 7, (0, 0), Instant::now()).is_err());
    for scenario in 0..6 {
        let mut output = video();
        match scenario {
            0 => output.frames[1].duration = None,
            1 => output.frames[1].duration = Some(0),
            2 => output.frames[1].pts = None,
            3 => output.frames[1].pts = Some(90000),
            4 => output.frames[1].decode_error_flags = 1,
            _ => output.frames[1].stream_index = 0,
        }
        assert!(end(&output, 3, 260, 7, (0, 0), deadline()).is_err());
    }
}

#[test]
fn eof_video_probe_snapshots_selected_stream_and_full_decoder_presentation() {
    let command = video_command("ffprobe", 3, Path::new("held-source.ts")).unwrap();
    insta::assert_snapshot!(command.args.join("\n"), @r"
-v
error
-select_streams
3
-show_frames
-show_streams
-show_programs
-show_entries
stream=index,codec_type,id,time_base:program=program_num:program_stream=index,codec_type,id,time_base:frame=media_type,stream_index,pts,duration,decode_error_flags
-of
json
-f
mpegts
held-source.ts
");
}
