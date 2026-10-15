//! Separate original bounded-rate controls from a generated no-buffer policy.

use super::*;

#[derive(Clone, Copy)]
struct RepeatPolicy {
    seek: Option<Duration>,
    video_buffer_size: Option<u32>,
}

async fn encoder_configuration(
    scope: &LocalProbeCleanupScope,
    ffmpeg: &str,
    source: &PreparedLocalTeletextSource,
    work: &TeletextWorkGuard,
    path: &std::path::Path,
) -> String {
    let mut args: Vec<String> = ["-hide_banner", "-loglevel", "error", "-nostdin", "-i"]
        .map(str::to_owned)
        .into();
    args.push(path.to_str().unwrap().into());
    args.extend(
        [
            "-map", "0:v:0", "-an", "-c:v", "copy", "-f", "h264", "pipe:1",
        ]
        .map(str::to_owned),
    );
    let bytes = capture_owned(
        scope,
        FfprobeCommand {
            program: ffmpeg.into(),
            args,
        },
        source,
        work,
    )
    .await;
    let text = String::from_utf8_lossy(&bytes);
    let start = text.find("x264 - core").unwrap();
    text[start..].split('\0').next().unwrap().to_owned()
}

pub(super) async fn decoded_frame_hashes(
    scope: &LocalProbeCleanupScope,
    ffmpeg: &str,
    source: &PreparedLocalTeletextSource,
    work: &TeletextWorkGuard,
    path: &std::path::Path,
    video: bool,
) -> Vec<String> {
    let mut args: Vec<String> = [
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-copyts",
        "-i",
    ]
    .map(str::to_owned)
    .into();
    args.push(path.to_str().unwrap().into());
    args.extend(
        if video {
            [
                "-map",
                "0:v:0",
                "-an",
                "-fps_mode",
                "passthrough",
                "-c:v",
                "rawvideo",
            ]
            .as_slice()
        } else {
            ["-map", "0:a:0", "-vn", "-c:a", "pcm_f32le"].as_slice()
        }
        .iter()
        .map(|arg| (*arg).to_owned()),
    );
    args.extend(
        [
            "-enc_time_base",
            "demux",
            "-f",
            "framehash",
            "-hash",
            "sha256",
            "pipe:1",
        ]
        .map(str::to_owned),
    );
    let output = capture_owned(
        scope,
        FfprobeCommand {
            program: ffmpeg.into(),
            args,
        },
        source,
        work,
    )
    .await;
    let output = String::from_utf8(output).unwrap();
    std::fs::write(
        path.with_extension(if video {
            "video.framehash"
        } else {
            "audio.framehash"
        }),
        &output,
    )
    .unwrap();
    assert!(output.lines().any(|line| line == "#tb 0: 1/90000"));
    let rows: Vec<_> = output
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .map(str::to_owned)
        .collect();
    assert!(!rows.is_empty());
    rows
}

async fn repeat_unlogged(
    scope: &LocalProbeCleanupScope,
    ffmpeg: &str,
    source: &PreparedLocalTeletextSource,
    caption: &DecodedLocalTeletextPage,
    work: &TeletextWorkGuard,
    policy: RepeatPolicy,
    evidence: &std::path::Path,
) -> bool {
    let command = burn_command_with_video_buffer(
        ffmpeg,
        source,
        Some(caption),
        policy.seek,
        policy.video_buffer_size,
    );
    if policy.video_buffer_size.is_none() {
        let mut bounded = burn_command(ffmpeg, source, Some(caption), policy.seek).arguments;
        let index = bounded.iter().position(|arg| arg == "-bufsize:v").unwrap();
        assert_eq!(bounded[index + 1], "800k");
        bounded.drain(index..index + 2);
        assert_eq!(
            command.arguments, bounded,
            "only the generated video buffer option changes"
        );
    }
    std::fs::write(
        evidence.with_extension("command.json"),
        serde_json::to_vec_pretty(&command.arguments).unwrap(),
    )
    .unwrap();
    let mut runs = Vec::new();
    let mut configuration = None;
    for ordinal in 0..3 {
        let output = capture_owned(
            scope,
            FfprobeCommand {
                program: command.executable.clone(),
                args: command.arguments.clone(),
            },
            source,
            work,
        )
        .await;
        let path = evidence.with_file_name(format!(
            "{}-{ordinal}.ts",
            evidence.file_name().unwrap().to_str().unwrap()
        ));
        std::fs::write(&path, &output).unwrap();
        if ordinal == 0 {
            let actual = encoder_configuration(scope, ffmpeg, source, work, &path).await;
            if policy.video_buffer_size.is_some() {
                assert!(actual.contains("rc=cbr"));
                assert!(actual.contains("vbv_bufsize=800"));
            } else {
                assert!(actual.contains("rc=abr"));
                assert!(!actual.contains("vbv_bufsize="));
            }
            configuration = Some(actual);
        }
        let video = decoded_frame_hashes(scope, ffmpeg, source, work, &path, true).await;
        let audio = decoded_frame_hashes(scope, ffmpeg, source, work, &path, false).await;
        runs.push(serde_json::json!({"ordinal":ordinal,"transport_sha256":hex_text(&Sha256::digest(&output)),"video":video,"audio":audio}));
    }
    let video_equal = runs.iter().all(|run| run["video"] == runs[0]["video"]);
    let audio_equal = runs.iter().all(|run| run["audio"] == runs[0]["audio"]);
    let clocks = |run: &serde_json::Value| {
        run["video"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row.as_str().unwrap().rsplit_once(',').unwrap().0.to_owned())
            .collect::<Vec<_>>()
    };
    let video_clocks = clocks(&runs[0]);
    assert_eq!(video_clocks.len(), 120);
    let clocks_equal = runs.iter().all(|run| clocks(run) == video_clocks);
    std::fs::write(evidence.with_extension("repeats.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "sealed_command_reused":true,"video_buffer_size":policy.video_buffer_size,
        "encoder_configuration":configuration,"video_equal":video_equal,"video_clocks_equal":clocks_equal,"audio_equal":audio_equal,"runs":runs,
        "note":"Unlogged controls only. Bounded-rate pixel variability is reported, never accepted as a caption tolerance. The separate no-buffer control must repeat exactly; neither policy grants original-production cue authority."
    })).unwrap()).unwrap();
    eprintln!("unlogged generated seek={:?}, buffer={:?}: complete decoded video equal={video_equal}, clocks equal={clocks_equal}, audio equal={audio_equal}",policy.seek,policy.video_buffer_size);
    audio_equal && clocks_equal && (policy.video_buffer_size.is_some() || video_equal)
}

#[tokio::test]
#[ignore = "requires explicit native FFmpeg/FFprobe for unlogged caption repeatability controls"]
#[allow(
    clippy::disallowed_methods,
    reason = "Explicit opt-in synthetic evidence destination"
)]
async fn unlogged_burn_compares_bounded_and_no_buffer_controls() {
    let scope = LocalProbeCleanupScope::new();
    let ffmpeg = tool(&scope, "ERSATZRS_TELETEXT_TEST_FFMPEG", "ffmpeg").await;
    let ffprobe = tool(&scope, "FFPROBE_PATH", "ffprobe").await;
    let directory = tempfile::tempdir().unwrap();
    let evidence = std::env::var_os("ERSATZRS_TELETEXT_BURN_EVIDENCE")
        .map_or_else(|| directory.path().join("evidence"), PathBuf::from);
    std::fs::create_dir_all(&evidence).unwrap();
    let path = staggered_source(&scope, &ffmpeg, directory.path()).await;
    let original = std::fs::read(&path).unwrap();
    std::fs::write(evidence.join("source.ts"), &original).unwrap();
    let locker = EntityLocker::new();
    let work = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let extractor = FfprobeExtractor::new(ffprobe);
    let source = prepare(&scope, &extractor, &path, &work).await;
    let caption = decode(&scope, &extractor, &source, &original, &work).await;
    let mut equal = true;
    for (name, seek, video_buffer_size) in [
        ("bounded-no-seek", None, Some(800)),
        (
            "bounded-seek-half",
            Some(Duration::from_millis(500)),
            Some(800),
        ),
        ("no-buffer-no-seek", None, None),
        (
            "no-buffer-seek-half",
            Some(Duration::from_millis(500)),
            None,
        ),
    ] {
        scope
            .scope(source.check_source(Instant::now() + Duration::from_secs(15), work.hold_work()))
            .await
            .unwrap();
        equal &= repeat_unlogged(
            &scope,
            &ffmpeg,
            &source,
            &caption,
            &work,
            RepeatPolicy {
                seek,
                video_buffer_size,
            },
            &evidence.join(name),
        )
        .await;
    }
    assert_eq!(std::fs::read(path).unwrap(), original);
    scope.close();
    assert_eq!(scope.wait_cleanup().await, Ok(()));
    drop(work);
    assert_eq!(locker.active_teletext_work(), 0);
    assert!(
        equal,
        "audio/clocks or the no-buffer pixel equality control failed; original bounded pixels remain unqualified"
    );
}
