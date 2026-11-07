//! UCU-15 row279: unnamed persisted codecs must not discard actual streams.
use super::*;

#[tokio::test]
#[ignore = "requires published Next/FFmpeg9; decodes missing-codec video and alternate audio"]
async fn real_next_unnamed_codec_hints_preserve_picture_and_selected_audio() {
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let root = tempfile::tempdir().unwrap();
    let clip = root.path().join("fixture.mkv");
    generate_clip(&ffmpeg, &clip).await;
    let saved = capture(root.path(), &clip, (None, Some("")), false).await;
    let manifest: serde_json::Value = serde_json::from_slice(
        &tokio::fs::read(
            root.path()
                .join("playout/1")
                .join(crate::next_engine::publication::PLAYOUT_FILENAME),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    let item = &manifest["items"][0];
    let hint = &item["tracks"]["video"]["source"]["probe_hint"];
    assert_eq!(hint["format_name"], "matroska,webm");
    assert_eq!(hint["video"][0]["codec"], "unknown");
    assert!(hint["audio"]
        .as_array()
        .unwrap()
        .iter()
        .all(|stream| stream["codec"] == "unknown"));
    assert_eq!(item["tracks"]["video"]["stream_index"], 0);
    assert_eq!(item["tracks"]["audio"]["stream_index"], 2);

    let mut audio = Command::new(&ffmpeg);
    audio
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(&saved)
        .args([
            "-map", "0:a:0", "-t", "1", "-ac", "1", "-ar", "8000", "-f", "s16le", "pipe:1",
        ]);
    let pcm = tool(audio).await;
    let samples: Vec<_> = pcm
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .collect();
    assert!(samples.len() >= 7000);
    let crossings = samples.windows(2).filter(|p| p[0] <= 0 && p[1] > 0).count();
    let frequency = f64::from(u32::try_from(crossings).unwrap()) * 8000.0
        / f64::from(u32::try_from(samples.len()).unwrap());
    assert!(
        (860.0..900.0).contains(&frequency),
        "selected Dutch audio: {frequency} Hz"
    );
    let mut decode = Command::new(&ffmpeg);
    decode
        .args(["-v", "error", "-xerror", "-i"])
        .arg(&saved)
        .args([
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-vf",
            "scale=32:18",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ]);
    let pixels = tool(decode).await;
    assert_eq!(pixels.len(), 32 * 18 * 3);
    assert!(
        pixels
            .chunks_exact(3)
            .all(|pixel| pixel[2] > 200 && pixel[0] < 30 && pixel[1] < 30),
        "recorded video must not become generated black"
    );
    let mut decode = Command::new(ffmpeg);
    decode
        .args(["-v", "error", "-xerror", "-i"])
        .arg(&saved)
        .args(["-f", "null", "-"]);
    tool(decode).await;
}
