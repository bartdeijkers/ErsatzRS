use super::*;

#[tokio::test]
async fn legacy_subtitle_preparation_handles_zero_seek_restart_and_source_replacement() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("synthetic.srt");
    let cache = root.path().join("cache");
    // Synthetic reproducer: a legacy accented byte appears at exactly 1284.
    let mut bytes = b"1\n00:00:42,000 --> 00:00:46,000\n".to_vec();
    bytes.resize(1284, b'a');
    bytes.extend_from_slice(
        b"\xe8 caf\xe9 caf\xe9 caf\xe9\n\n2\n00:00:46,000 --> 00:00:48,000\nNext\n",
    );
    assert_eq!(
        String::from_utf8(bytes.clone())
            .unwrap_err()
            .utf8_error()
            .valid_up_to(),
        1284
    );
    tokio::fs::write(&source, &bytes).await.unwrap();
    for seek in [0, 44, 45] {
        let path = prepare_text_subtitle_in(source.to_str().unwrap(), None, seek, &cache)
            .await
            .unwrap()
            .unwrap();
        let text = tokio::fs::read_to_string(&path).await.unwrap();
        assert!(text.contains("è café café café"));
        if seek > 0 {
            assert!(text.contains(&format!("00:00:00,000 --> 00:00:0{},000", 46 - seek)));
        }
        tokio::fs::write(&path, "partial cache").await.unwrap();
        let repaired = prepare_text_subtitle_in(source.to_str().unwrap(), None, seek, &cache)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(tokio::fs::read_to_string(repaired).await.unwrap(), text);
    }
    let original = prepare_text_subtitle_in(source.to_str().unwrap(), None, 44, &cache)
        .await
        .unwrap()
        .unwrap();
    tokio::fs::write(&source, "1\n00:00:42,000 --> 00:00:46,000\nReplacement\n")
        .await
        .unwrap();
    let replaced = prepare_text_subtitle_in(source.to_str().unwrap(), None, 44, &cache)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(original, replaced);
    assert!(tokio::fs::read_to_string(replaced)
        .await
        .unwrap()
        .contains("Replacement"));
    assert!(
        prepare_text_subtitle_in(source.to_str().unwrap(), None, 60, &cache)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn burned_text_at_zero_is_normalized_and_uncertain_sources_fail_explicitly() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("synthetic.srt");
    tokio::fs::write(&source, b"1\n00:00:00,000 --> 00:00:03,000\ncaf\xe9\n")
        .await
        .unwrap();
    let input = InputFile::text_subtitle(source.to_string_lossy(), SubtitleMethod::Burn);
    assert!(rebase_burned_text_subtitle_input(Some(input.clone()), 0)
        .await
        .is_err());
    tokio::fs::write(root.path().join("synthetic.srt.charset"), "windows-1252")
        .await
        .unwrap();
    let prepared = rebase_burned_text_subtitle_input(Some(input), 0)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(prepared.path, source.to_string_lossy());
    assert!(tokio::fs::read_to_string(prepared.path)
        .await
        .unwrap()
        .contains("café"));
    assert_eq!(
        tokio::fs::read(&source).await.unwrap(),
        b"1\n00:00:00,000 --> 00:00:03,000\ncaf\xe9\n"
    );
}

#[tokio::test]
async fn legacy_ass_and_webvtt_keep_payload_and_straddling_cues() {
    let root = tempfile::tempdir().unwrap();
    for (extension, text, timestamp) in [
        ("ass", "[Events]\nFormat: Start, End, Text\nDialogue: 0:00:42.00,0:00:46.00,{\\i1}Café élève déjà très éclairé\n", "0:00:00.00,0:00:02.00"),
        ("vtt", "WEBVTT\n\ncue\n00:42.000 --> 00:46.000 align:start\nCafé élève déjà très éclairé\n", "00:00:00.000 --> 00:00:02.000 align:start"),
    ] {
        let source = root.path().join(format!("synthetic.{extension}"));
        let (bytes,_,errors)=encoding_rs::WINDOWS_1252.encode(text);
        assert!(!errors);
        tokio::fs::write(&source,&bytes).await.unwrap();
        let path=prepare_text_subtitle_in(source.to_str().unwrap(),None,44,&root.path().join("cache")).await.unwrap().unwrap();
        let prepared=tokio::fs::read_to_string(path).await.unwrap();
        assert!(prepared.contains("Café élève déjà très éclairé"));
        assert!(prepared.contains(timestamp),"{prepared}");
        if extension=="ass" { assert!(prepared.contains("{\\i1}")); }
    }
    let broken = root.path().join("broken.srt");
    tokio::fs::write(&broken, "not a subtitle").await.unwrap();
    assert!(prepare_text_subtitle_in(
        broken.to_str().unwrap(),
        None,
        0,
        &root.path().join("cache")
    )
    .await
    .is_err());
}

#[test]
fn normalized_subtitle_compatibility_argv_snapshot() {
    let args = build_seek_subtitle_args(
        "cache/text-source.srt",
        Some(SubtitleTextFormat::Srt),
        44,
        Path::new("cache/rebased.srt"),
    );
    insta::assert_snapshot!(args.join("\n"), @"
    -nostdin
    -hide_banner
    -nostats
    -loglevel
    error
    -i
    cache/text-source.srt
    -ss
    00:00:44
    -c:s
    copy
    -f
    srt
    cache/rebased.srt
    ");
}

async fn decoded_subtitle_frame(ffmpeg: &str, path: &str) -> Vec<u8> {
    let filter = ersatzrs_ffmpeg::subtitles_filter(path, None);
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        Command::new(ffmpeg)
            .args([
                "-nostdin",
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=black:s=320x180:r=1",
                "-vf",
                &filter,
                "-frames:v",
                "1",
                "-pix_fmt",
                "rgb24",
                "-f",
                "rawvideo",
                "pipe:1",
            ])
            .kill_on_drop(true)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout.len(), 320 * 180 * 3);
    assert!(
        output.stdout.iter().any(|byte| *byte > 100),
        "caption must actually be visible"
    );
    output.stdout
}

#[tokio::test]
#[ignore = "Requires FFmpeg 9 with libass; decoded subtitle acceptance"]
async fn legacy_subtitle_decoded_frames_match_unicode_at_hls_restart_boundaries() {
    let ffmpeg = ffmpeg_path();
    let version = Command::new(&ffmpeg)
        .arg("-version")
        .output()
        .await
        .unwrap();
    assert!(version.status.success());
    assert_eq!(
        ersatzrs_ffmpeg::version::parse_version(
            String::from_utf8_lossy(&version.stdout)
                .lines()
                .next()
                .unwrap()
        )
        .unwrap()
        .major,
        9
    );
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("legacy.srt");
    let reference = root.path().join("reference.srt");
    let text = "1\n00:00:00,000 --> 00:00:46,000\nCafé élève déjà très éclairé\n";
    let (legacy, _, error) = encoding_rs::WINDOWS_1252.encode(text);
    assert!(!error);
    tokio::fs::write(&source, &legacy).await.unwrap();
    tokio::fs::write(&reference, text).await.unwrap();
    let raw_filter = ersatzrs_ffmpeg::subtitles_filter(source.to_str().unwrap(), None);
    let raw = Command::new(&ffmpeg)
        .args([
            "-nostdin",
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=320x180:r=1",
            "-vf",
            &raw_filter,
            "-frames:v",
            "1",
            "-f",
            "null",
            "-",
        ])
        .output()
        .await
        .unwrap();
    assert!(
        !raw.status.success(),
        "raw legacy fixture must reproduce the decoding failure"
    );
    assert!(String::from_utf8_lossy(&raw.stderr).contains("Invalid UTF-8"));

    for seek in [0, 44, 45] {
        let actual = prepare_text_subtitle_in(
            source.to_str().unwrap(),
            None,
            seek,
            &root.path().join("cache"),
        )
        .await
        .unwrap()
        .unwrap();
        let expected = prepare_text_subtitle_in(
            reference.to_str().unwrap(),
            None,
            seek,
            &root.path().join("reference-cache"),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            decoded_subtitle_frame(&ffmpeg, &actual).await,
            decoded_subtitle_frame(&ffmpeg, &expected).await,
            "seek {seek}"
        );
    }
}
