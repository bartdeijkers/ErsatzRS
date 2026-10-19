//! Genuine unknown-final-EOF input with an independent literal fixture oracle.

use super::*;

#[tokio::test]
#[ignore = "requires pinned Next0.2.2, FFmpeg9, Python3 and fresh ERSATZRS_NEXT_TELETEXT_PROOF_DIR"]
async fn real_next_unclosed_final_caption_uses_video_end_and_earlier_programme_cut() {
    let root = fixture::proof_root();
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let clip = fixture::source_inner(&ffmpeg, &root, None, "unclosed-caption").await;
    independent_eof_oracle(&ffmpeg, &ffprobe, &clip, &root).await;

    let profile = FFmpegProfile {
        video_format: FFmpegProfileVideoFormat::Copy,
        audio_format: FFmpegProfileAudioFormat::Copy,
        audio_sample_rate: 48,
        audio_channels: 1,
        ..text_profile()
    };
    let (pool, locker) = prepare_schedule(&clip, false, &profile).await;
    fixture::acquire_and_store(&pool, &ffprobe, &clip, &root).await;
    let prepared = cache::prepare_complete_text(&pool, &locker, &ffmpeg, &ffprobe, &clip)
        .await
        .expect("actual unknown-EOF preparation must succeed before publication");
    assert_eq!(
        clock::cue_blocks(&prepared),
        clock::cue_blocks(
            &tokio::fs::read_to_string(root.join("independent-889.srt"))
                .await
                .unwrap()
        )
    );
    sqlx::query(
        r#"UPDATE "Channel" SET "SubtitleMode"=3,"NextEngineTextSubtitleMode"=1 WHERE "Id"=1"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    let mut paths = Vec::new();
    for (name, end_ms) in [("earlier-programme-cut", 9200), ("actual-video-end", 20559)] {
        let host = std::sync::Arc::new(host::NextHost::new(
            pool.clone(),
            locker.clone(),
            root.join("playout"),
        ));
        paths.push(
            run_case(
                host,
                &pool,
                (&ffmpeg, &ffprobe),
                &root,
                &clip,
                Case {
                    name,
                    start_ms: 2000,
                    end_ms,
                    captions: true,
                    callback_required: true,
                    caption_track_required: true,
                },
            )
            .await,
        );
        assert_eq!(locker.active_teletext_work(), 0);
    }
    assert_eq!(
        paths[0], paths[1],
        "full-source EOF artifact still ignores occurrence cuts"
    );
}

async fn independent_eof_oracle(ffmpeg: &Path, ffprobe: &Path, clip: &Path, root: &Path) {
    fixture::independent_pages(ffmpeg, clip, root).await;
    let fixed = tokio::fs::read_to_string(root.join("independent-889.srt"))
        .await
        .unwrap();
    assert!(
        !fixed.contains("889 FINAL"),
        "retain the actual native fix_sub_duration EOF negative"
    );
    assert_eq!(fixed.matches(" --> ").count(), 7);
    tokio::fs::write(root.join("independent-889-fixed-negative.srt"), &fixed)
        .await
        .unwrap();

    // Check the source's decoded presentation independently of the production
    // qualifier: exactly 500 frames, final PTS1924290 + duration3600 at90kHz;
    // source format origin77544 ticks gives normalized20.5594s, not duration20s.
    // SRT's nearest millisecond is20.559s; no timing tolerance is introduced.
    let mut probe = Command::new(ffprobe);
    probe
        .args([
            "-v",
            "error",
            "-select_streams",
            "0",
            "-show_frames",
            "-show_format",
            "-show_entries",
            "frame=pts,duration:format=start_time",
            "-of",
            "json",
        ])
        .arg(clip);
    let decoded: serde_json::Value = serde_json::from_slice(&tool(probe).await).unwrap();
    let frames = decoded["frames"].as_array().unwrap();
    assert_eq!(frames.len(), 500);
    assert_eq!(frames.last().unwrap()["pts"], 1_924_290);
    assert_eq!(frames.last().unwrap()["duration"], 3600);
    assert_eq!(decoded["format"]["start_time"], "0.861600");
    let last = frames.last().unwrap();
    let end_ticks = last["pts"].as_i64().unwrap() + last["duration"].as_i64().unwrap();
    assert_eq!((end_ticks - 77_544 + 45) / 90, 20559);
    let expected = format!(
        "{}\n\n8\n00:00:07,800 --> 00:00:20,559\nNEDERLANDSE PAGINA 889 FINAL\n\n",
        fixed.trim_end_matches('\n')
    );
    tokio::fs::write(root.join("independent-889.srt"), expected)
        .await
        .unwrap();
}
