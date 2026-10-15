//! Fixture-only independent caption owner; no runtime seek or cue admission.

use super::*;

struct CapturedCase {
    name: &'static str,
    trace: serde_json::Value,
    masks: Vec<Vec<u8>>,
    audio: Vec<String>,
}

struct Fixture<'a> {
    scope: &'a LocalProbeCleanupScope,
    tools: (&'a str, &'a str),
    source: &'a PreparedLocalTeletextSource,
    work: &'a TeletextWorkGuard,
    evidence: &'a std::path::Path,
}

impl Fixture<'_> {
    async fn record_runtime(&self, original: &[u8]) {
        let mut versions = Vec::new();
        for program in [self.tools.0, self.tools.1] {
            let version = capture_owned(
                self.scope,
                FfprobeCommand {
                    program: program.into(),
                    args: vec!["-version".into()],
                },
                self.source,
                self.work,
            )
            .await;
            versions.push(serde_json::json!({"program":program,"version":String::from_utf8(version).unwrap()}));
        }
        std::fs::write(
            self.evidence.join("runtime.json"),
            serde_json::to_vec_pretty(&serde_json::json!({"versions":versions,
                "source_sha256":hex_text(&Sha256::digest(original)),"source_bytes":original.len()}))
            .unwrap(),
        )
        .unwrap();
    }

    async fn capture(&self, name: &'static str, command: FfmpegCommand) -> CapturedCase {
        let evidence = self.evidence.join(name);
        std::fs::write(
            evidence.with_extension("command.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "executable": command.executable, "arguments": command.arguments,
                "environment_empty": command.environment.is_empty()
            }))
            .unwrap(),
        )
        .unwrap();
        self.scope
            .scope(self.source.check_source(
                Instant::now() + Duration::from_secs(15),
                self.work.hold_work(),
            ))
            .await
            .unwrap();
        let bytes = capture_owned(
            self.scope,
            FfprobeCommand {
                program: command.executable,
                args: command.arguments,
            },
            self.source,
            self.work,
        )
        .await;
        let path = evidence.with_extension("ts");
        std::fs::write(&path, &bytes).unwrap();
        let (trace, gray) =
            frame_evidence(self.scope, self.tools, self.source, &path, self.work).await;
        std::fs::write(
            evidence.with_extension("frames.json"),
            serde_json::to_vec_pretty(&trace).unwrap(),
        )
        .unwrap();
        std::fs::write(evidence.with_extension("gray"), gray.concat()).unwrap();
        let masks: Vec<Vec<u8>> = gray
            .iter()
            .map(|frame| frame.iter().map(|value| u8::from(*value > 100)).collect())
            .collect();
        std::fs::write(evidence.with_extension("masks"), masks.concat()).unwrap();
        let audio = repeat::decoded_frame_hashes(
            self.scope,
            self.tools.0,
            self.source,
            self.work,
            &path,
            false,
        )
        .await;
        self.scope
            .scope(self.source.check_source(
                Instant::now() + Duration::from_secs(15),
                self.work.hold_work(),
            ))
            .await
            .unwrap();
        CapturedCase {
            name,
            trace,
            masks,
            audio,
        }
    }
}

fn independent_command(
    ffmpeg: &str,
    source: &PreparedLocalTeletextSource,
    caption: &DecodedLocalTeletextPage,
) -> FfmpegCommand {
    let mut request = burn_request(
        ffmpeg,
        source,
        Some(caption),
        Some(Duration::from_millis(500)),
        None,
    );
    let input = request.inputs.last_mut().unwrap();
    input.input_instance = 1;
    // Native input options do not carry across -i boundaries. Match the
    // existing origin probe's flags explicitly on this independent owner.
    let format_flags = ersatzrs_ffmpeg::standard_format_flags();
    input
        .input_options
        .extend(format_flags.global_options().iter().cloned());
    // Explicit fixture control only: Kv=1400000, Kc in [860944,860945].
    // This selects the lower endpoint and retains its one-us uncertainty.
    input
        .input_options
        .extend(["-itsoffset".into(), "-0.539056".into()]);
    let expected = input.clone();
    let (command, plan) = build_pipeline_command_with_source_teletext_plan(
        ersatzrs_domain::HardwareAccelerationKind::None,
        request,
    )
    .unwrap();
    let plan = plan.unwrap();
    assert!(plan.matches_command(&command));
    assert_eq!(plan.caption_input(), &expected);
    assert_eq!(plan.caption_physical_input_index(), 1);
    assert_eq!(plan.video_stream(), Some((0, 0)));
    assert_eq!(plan.audio_stream(), Some((0, 1)));
    assert_eq!(plan.enabled_caption_input_streams(), [2]);
    assert_eq!(plan.timing().input_seek(), None);
    assert_eq!(plan.timing().effective_input_seek_microseconds(), None);
    assert!(plan.timing().has_opaque_input_options());
    assert!(plan
        .physical_input_options()
        .starts_with(format_flags.global_options()));
    assert!(plan
        .physical_input_options()
        .windows(2)
        .any(|pair| pair == ["-itsoffset", "-0.539056"]));
    assert!(!plan
        .physical_input_options()
        .iter()
        .any(|option| option == "-ss"));
    assert_eq!(
        command
            .arguments
            .iter()
            .filter(|option| *option == "-i")
            .count(),
        2
    );
    assert!(command.environment.is_empty());
    command
}

async fn origins(
    fixture: &Fixture<'_>,
    extractor: &FfprobeExtractor,
    caption: &DecodedLocalTeletextPage,
) -> ((i64, i64), (i64, i64)) {
    let mut observations = Vec::new();
    let mut zeros = Vec::new();
    for (enabled, seek) in [(&[0, 1][..], 500_000), (&[2][..], 0)] {
        let origin = fixture
            .scope
            .scope(extractor.observe_local_teletext_consumer_origin(
                caption,
                enabled,
                Instant::now() + Duration::from_secs(15),
                fixture.work.hold_work(),
            ))
            .await
            .unwrap();
        let timing = origin.timing(seek).unwrap();
        zeros.push(timing.presentation_zero_bounds_microseconds());
        observations.push(serde_json::json!({
            "enabled":enabled,"seek_us":seek,"format_start_bounds_us":timing.format_start_bounds_microseconds(),
            "enabled_start_us":timing.enabled_start_microseconds(),"presentation_zero_bounds_us":timing.presentation_zero_bounds_microseconds(),
            "streams":origin.streams().iter().map(|stream| serde_json::json!({"index":stream.index(),"pid":stream.pid(),"role":stream.codec_type(),"start_pts":stream.start_pts(),"time_base":format!("{:?}",stream.time_base())})).collect::<Vec<_>>()
        }));
    }
    std::fs::write(fixture.evidence.join("origins.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "observations":observations,"fixture_caption_itsoffset_us":-539_056,
        "input1_standard_format_flags":ersatzrs_ffmpeg::standard_format_flags().global_options(),
        "residual_caption_alignment_us":[-1,0],
        "note":"Independent fixed-policy source observations, not a probe with itsoffset or proof of general alignment. The explicit fixture offset uses the lower Kc endpoint; its possible one-us residual is retained, never a new production tolerance. Runtime receipt instance fences are unchanged."
    })).unwrap()).unwrap();
    (zeros[0], zeros[1])
}

fn audio_projection(rows: &[String]) -> (Vec<String>, Vec<[i64; 4]>, Vec<[i64; 4]>) {
    let mut content = Vec::new();
    let mut clocks = Vec::new();
    for row in rows {
        let fields: Vec<_> = row.split(',').map(str::trim).collect();
        assert_eq!(fields.len(), 6);
        content.push(fields[5].to_owned());
        clocks.push([
            fields[1].parse().unwrap(),
            fields[2].parse().unwrap(),
            fields[3].parse().unwrap(),
            fields[4].parse().unwrap(),
        ]);
    }
    let first = clocks.first().unwrap();
    let relative = clocks
        .iter()
        .map(|row| [row[0] - first[0], row[1] - first[1], row[2], row[3]])
        .collect();
    (content, clocks, relative)
}

fn observations(cases: &[CapturedCase]) -> serde_json::Value {
    let off = audio_projection(&cases[1].audio);
    serde_json::json!({
        "threshold_luma_exclusive":100,"sample_stride_decoded_frames":SAMPLE_STRIDE,
        "note":"Each case retains its own actual PTS. Sample ordinals do not establish a shared clock. Masks are exact spatial fixture glyph controls, not a Level 2.5 color tolerance. Relative audio clocks do not prove AV synchronization across different mux baselines.",
        "cases":cases.iter().map(|case| {
            let audio = audio_projection(&case.audio);
            serde_json::json!({"name":case.name,"audio_content_equal_sought_off":audio.0==off.0,"audio_absolute_clocks_equal_sought_off":audio.1==off.1,"audio_relative_clocks_equal_sought_off":audio.2==off.2,
                "audio_first_clock":audio.1.first(),"audio_last_clock":audio.1.last(),"audio_frame_count":audio.1.len(),
                "samples":case.masks.iter().enumerate().map(|(sample,mask)|serde_json::json!({
                    "sample":sample,"decoded_frame_ordinal":sample*SAMPLE_STRIDE,"own_frame":case.trace["frames"][sample*SAMPLE_STRIDE],
                    "foreground_pixels":mask.iter().copied().map(usize::from).sum::<usize>(),"spatial_mask_sha256":hex_text(&Sha256::digest(mask))
                })).collect::<Vec<_>>()})
        }).collect::<Vec<_>>()
    })
}

fn assert_fixture_control(cases: &[CapturedCase]) {
    let reference = &cases[0].masks[0];
    assert!(reference.iter().copied().map(usize::from).sum::<usize>() > 500);
    assert!(
        cases[1].masks.iter().flatten().all(|value| *value == 0),
        "every sought Off sample stays blank"
    );
    assert!(
        cases[2].masks[0].iter().all(|value| *value == 0),
        "sought shared caption starts blank"
    );
    assert_eq!(
        cases[2].masks.last().unwrap(),
        reference,
        "shared negative eventually displays the reference glyph"
    );
    assert_eq!(cases[3].masks.len(), 24);
    for mask in &cases[3].masks {
        assert_eq!(
            mask, reference,
            "every independent sample has the exact reference glyph mask"
        );
    }
    let off = audio_projection(&cases[1].audio);
    let shared = audio_projection(&cases[2].audio);
    assert_ne!(
        shared.0, off.0,
        "known shared-owner negative changes audio content"
    );
    assert_ne!(
        shared.1, off.1,
        "known shared-owner negative changes absolute audio clocks"
    );
    assert_ne!(
        shared.2, off.2,
        "known shared-owner negative changes relative audio clocks"
    );
    let independent = audio_projection(&cases[3].audio);
    assert_eq!(
        independent.0, off.0,
        "independent decoded audio content equals Off"
    );
    assert_eq!(
        independent.1, off.1,
        "independent absolute audio clocks equal Off"
    );
    assert_eq!(
        independent.2, off.2,
        "independent relative audio clocks equal Off"
    );
}

#[tokio::test]
#[ignore = "requires explicit native FFmpeg/FFprobe for independent caption owner control"]
#[allow(
    clippy::disallowed_methods,
    reason = "Explicit opt-in synthetic evidence destination"
)]
async fn independent_caption_owner_preserves_initial_fixture_glyph_after_seek() {
    let scope = LocalProbeCleanupScope::new();
    let ffmpeg = tool(&scope, "ERSATZRS_TELETEXT_TEST_FFMPEG", "ffmpeg").await;
    let ffprobe = tool(&scope, "FFPROBE_PATH", "ffprobe").await;
    // Retain every generated output even when the diagnostic hypothesis is red.
    let directory = tempfile::tempdir().unwrap().keep();
    let evidence = std::env::var_os("ERSATZRS_TELETEXT_BURN_EVIDENCE")
        .map_or_else(|| directory.join("independent-evidence"), PathBuf::from);
    std::fs::create_dir_all(&evidence).unwrap();
    let path = staggered_source(&scope, &ffmpeg, &directory).await;
    let original = std::fs::read(&path).unwrap();
    std::fs::write(evidence.join("source.ts"), &original).unwrap();
    let locker = EntityLocker::new();
    let work = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let extractor = FfprobeExtractor::new(ffprobe.clone());
    let source = prepare(&scope, &extractor, &path, &work).await;
    let caption = decode(&scope, &extractor, &source, &original, &work).await;
    let fixture = Fixture {
        scope: &scope,
        tools: (&ffmpeg, &ffprobe),
        source: &source,
        work: &work,
        evidence: &evidence,
    };
    fixture.record_runtime(&original).await;
    let zeros = origins(&fixture, &extractor, &caption).await;
    let seek = Some(Duration::from_millis(500));
    let mut cases = Vec::new();
    for (name, command) in [
        (
            "reference-unseeked-888",
            burn_command_with_video_buffer(&ffmpeg, &source, Some(&caption), None, None),
        ),
        (
            "seek-half-off",
            burn_command_with_video_buffer(&ffmpeg, &source, None, seek, None),
        ),
        (
            "seek-half-shared-888",
            burn_command_with_video_buffer(&ffmpeg, &source, Some(&caption), seek, None),
        ),
        (
            "seek-half-independent-888",
            independent_command(&ffmpeg, &source, &caption),
        ),
    ] {
        cases.push(fixture.capture(name, command).await);
    }
    std::fs::write(
        evidence.join("comparison.json"),
        serde_json::to_vec_pretty(&observations(&cases)).unwrap(),
    )
    .unwrap();
    let source_unchanged = std::fs::read(&path).unwrap() == original;
    scope.close();
    assert_eq!(scope.wait_cleanup().await, Ok(()));
    drop(work);
    assert_eq!(locker.active_teletext_work(), 0);
    assert!(source_unchanged);
    assert_eq!(zeros, ((1_400_000, 1_400_000), (860_944, 860_945)));
    assert_fixture_control(&cases);
}
