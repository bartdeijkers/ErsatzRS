use super::*;

#[test]
fn canvas_container_parser_keeps_muxers_demuxers_aliases_and_devices_distinct() {
    let (muxers, demuxers) = parse_container_formats(
        "Formats:\n D.. = Demuxing supported\n .E. = Muxing supported\n ..d = Is a device\n ---\n DE nut NUT\n D mov,mp4 QuickTime\n E matroska Matroska\n DEd alsa audio\n INVALID fake invalid\n",
    );
    assert_eq!(
        muxers,
        BTreeSet::from(["nut".into(), "matroska".into(), "alsa".into()])
    );
    assert_eq!(
        demuxers,
        BTreeSet::from(["nut".into(), "mov".into(), "mp4".into(), "alsa".into()])
    );
    assert!(!parse_container_formats("D nut NUT input only")
        .0
        .contains("nut"));
    assert!(!parse_container_formats("E nut NUT output only")
        .1
        .contains("nut"));
}

#[test]
fn canvas_runtime_contract_rejects_each_missing_required_capability() {
    let config = FfmpegRuntimeCheckConfig::default();
    let supported = FfmpegRuntimeReport::for_tests().capabilities;
    assert!(collect_runtime_issues(&config, &supported).is_empty());
    for (kind, name) in [
        ("encoder", "ffv1"),
        ("decoder", "ffv1"),
        ("muxer", "nut"),
        ("demuxer", "nut"),
        ("filter", "drawvg"),
        ("filter", "drawtext"),
        ("filter", "subtitles"),
        ("filter", "settb"),
        ("filter", "reverse"),
        ("filter", "trim"),
    ] {
        let mut capabilities = supported.clone();
        let (names, expected) = match kind {
            "encoder" => (
                &mut capabilities.encoders,
                FfmpegRuntimeIssue::RequiredEncoder { name: name.into() },
            ),
            "decoder" => (
                &mut capabilities.decoders,
                FfmpegRuntimeIssue::RequiredDecoder { name: name.into() },
            ),
            "muxer" => (
                &mut capabilities.muxers,
                FfmpegRuntimeIssue::RequiredMuxer { name: name.into() },
            ),
            "demuxer" => (
                &mut capabilities.demuxers,
                FfmpegRuntimeIssue::RequiredDemuxer { name: name.into() },
            ),
            _ => (
                &mut capabilities.filters,
                FfmpegRuntimeIssue::RequiredFilter { name: name.into() },
            ),
        };
        assert!(names.remove(name));
        let issues = collect_runtime_issues(&config, &capabilities);
        assert_eq!(issues, vec![expected], "{kind} {name}");
        assert!(issues[0].is_error());
    }
}

#[tokio::test]
#[ignore = "requires the checksum-verified packaged FFmpeg/ffprobe runtime"]
#[allow(
    clippy::disallowed_methods,
    reason = "manual package gate supplies the verified executable path"
)]
async fn canvas_packaged_runtime_passes_required_capabilities_and_startup_smoke() {
    let ffmpeg = std::env::var_os("ERSATZRS_FFMPEG").expect("verified packaged FFmpeg path");
    let ffmpeg = std::path::PathBuf::from(ffmpeg);
    let ffprobe = ffmpeg.with_file_name(format!("ffprobe{}", std::env::consts::EXE_SUFFIX));
    let config =
        FfmpegRuntimeCheckConfig::with_paths(ffmpeg.to_str().unwrap(), ffprobe.to_str().unwrap());
    let report = check_ffmpeg_runtime_with_config(&config).await.unwrap();
    assert!(!report.has_errors());
    assert!(report.capabilities.muxers.contains("nut"));
    assert!(report.capabilities.demuxers.contains("nut"));
    assert!(report.capabilities.encoders.contains("ffv1"));
    assert!(report.capabilities.decoders.contains("ffv1"));
}
