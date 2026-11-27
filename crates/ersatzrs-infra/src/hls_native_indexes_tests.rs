use super::tests::playback_profile;
use super::*;

/// ADR0137: preserve logical choices until the original-file boundary.
#[tokio::test]
async fn row295_native_hls_physical_indexes_preserve_all_provider_layouts() {
    let mut reference = None;
    for logical in [[2, 3, 4, 5, 6], [0, 1, 2, 3, 4], [0, 2, 3, 5, 6]] {
        let pool = crate::next_engine::selection::tests::fixture_pool().await;
        crate::next_engine::selection::tests::jellyfin_stream_indexes::qualify(&pool, logical)
            .await;
        let map = crate::playback::source_indexes::load(&pool, 100)
            .await
            .unwrap();
        let (acceleration, mut request) = native_request(logical);
        let original_request = request.clone();
        let mut automatic = request.clone();
        automatic.inputs[1].streams[0].index = 0;
        crate::playback::source_indexes::apply_native_request(
            map.as_ref(),
            "synthetic-native.mkv",
            &mut automatic,
        )
        .unwrap();
        assert_eq!(
            automatic.inputs[1].streams[0].index, 1,
            "ordinary automatic fallback keeps the first audio track"
        );
        let mut subtitle_request = request.clone();
        subtitle_request.inputs.push(
            InputFile::image_subtitle(
                "synthetic-native.mkv",
                ersatzrs_ffmpeg::SubtitleMethod::Burn,
            )
            .with_stream_index(usize::try_from(logical[4]).unwrap()),
        );
        subtitle_request.inputs.push(InputFile::text_subtitle(
            "prepared/caption.srt",
            ersatzrs_ffmpeg::SubtitleMethod::Burn,
        ));
        crate::playback::source_indexes::apply_native_request(
            map.as_ref(),
            "synthetic-native.mkv",
            &mut subtitle_request,
        )
        .unwrap();
        assert_eq!(subtitle_request.inputs[2].streams[0].index, 4);
        assert_eq!(subtitle_request.inputs[3].streams[0].index, 0);
        crate::playback::source_indexes::apply_native_request(
            map.as_ref(),
            "synthetic-native.mkv",
            &mut request,
        )
        .unwrap();
        assert_eq!(request.source_video_stream.as_ref().unwrap().index, 0);
        assert_eq!(
            request
                .source_audio_streams
                .iter()
                .map(|s| s.index)
                .collect::<Vec<_>>(),
            [1, 2]
        );
        let command = ersatzrs_ffmpeg::build_pipeline_command_with_source_teletext_plan(
            acceleration,
            request.clone(),
        )
        .unwrap()
        .0;
        if let Some(previous) = &reference {
            assert_eq!(&command.arguments, previous);
        }
        reference = Some(command.arguments);
        // A missing selected physical identity must fail atomically.
        let mut invalid = original_request;
        invalid.source_audio_streams[1].index = 999;
        let previous = invalid.clone();
        assert!(crate::playback::source_indexes::apply_native_request(
            map.as_ref(),
            "synthetic-native.mkv",
            &mut invalid
        )
        .is_err());
        assert_eq!(invalid, previous);
        // A retained producer has already selected physical NUT indexes.
        request.state.retained_stream_timestamps = true;
        let previous = request.clone();
        crate::playback::source_indexes::apply_native_request(
            map.as_ref(),
            "synthetic-native.mkv",
            &mut request,
        )
        .unwrap();
        assert_eq!(request, previous);
    }
    insta::assert_snapshot!(
        "row295_native_hls_physical_tracks",
        reference.unwrap().join("\n")
    );
}

fn native_request(logical: [i32; 5]) -> (HardwareAccelerationKind, PipelineRequest) {
    let mut profile = playback_profile();
    profile.profile.video_format = FFmpegProfileVideoFormat::Copy;
    profile.profile.audio_format = FFmpegProfileAudioFormat::Copy;
    build_hls_request_with_input_explicit_discontinuity(
        InputFile::video("synthetic-native.mkv"),
        OutputFormat::Hls,
        true,
        false,
        true,
        Duration::ZERO,
        Some(Duration::from_secs(44)),
        None,
        &profile,
        None,
        OutputMetadata::default(),
        FfmpegCapabilityReport::default(),
        Some(
            SourceVideoStream::new(SourceVideoCodec::H264)
                .with_index(usize::try_from(logical[0]).unwrap()),
        ),
        vec![
            SourceAudioStream::new(SourceAudioCodec::Aac)
                .with_index(usize::try_from(logical[1]).unwrap()),
            SourceAudioStream::new(SourceAudioCodec::Aac)
                .with_index(usize::try_from(logical[2]).unwrap()),
        ],
        PlaybackAudioSelection::Exact(usize::try_from(logical[2]).unwrap()),
        None,
        Vec::new(),
        Vec::new(),
        None,
        HlsDiscontinuity::Absent,
        None,
        HlsInputTimeline::Original,
        None,
    )
}
