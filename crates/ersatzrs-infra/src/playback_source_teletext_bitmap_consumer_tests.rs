//! Seal and ownership tests do not claim native playback or client qualification.
use super::super::tests::fixture;
use super::*;
use ersatzrs_domain::{FFmpegProfileVideoFormat, HardwareAccelerationKind};
use ersatzrs_ffmpeg::{build_pipeline_command_with_source_bitmap_carriage_plan, SubtitleMethod};

fn inputs() -> (InputFile, InputFile) {
    let mut original = InputFile::image_subtitle("private-full-artifact.mks", SubtitleMethod::Copy);
    original.allow_readrate = false;
    let mut playlist =
        InputFile::image_subtitle("private-initialized.ffconcat", SubtitleMethod::Copy);
    playlist.allow_readrate = false;
    playlist.input_options = vec![
        "-f".into(),
        "concat".into(),
        "-safe".into(),
        "0".into(),
        "-protocol_whitelist".into(),
        "file".into(),
    ];
    (original, playlist)
}
#[test]
fn initialized_bitmap_command_preserves_sealed_av_timing_encoder_and_explicit_file_only_input() {
    let (original, playlist) = inputs();
    for acceleration in [
        HardwareAccelerationKind::None,
        HardwareAccelerationKind::Qsv,
    ] {
        for format in [
            FFmpegProfileVideoFormat::Copy,
            FFmpegProfileVideoFormat::H264,
        ] {
            let mut pipeline = fixture(Some(3), Some(800));
            pipeline.desired.video_format = format;
            pipeline.inputs.pop();
            pipeline.inputs.push(original.clone());
            let request = SourceTeletextConsumerRequest::new_bitmap_carriage(
                acceleration,
                pipeline.clone(),
                original.clone(),
            );
            let (initial, plan) = request.build().unwrap();
            let plan = plan.unwrap();
            assert!(plan.is_bitmap_carriage());
            let final_command = request
                .initialized_bitmap_command(&plan, &playlist)
                .unwrap();
            let mut independently_expected = playlist.clone();
            independently_expected.input_instance = pipeline
                .inputs
                .iter()
                .map(|input| input.input_instance)
                .max()
                .unwrap()
                + 1;
            *pipeline.inputs.last_mut().unwrap() = independently_expected.clone();
            let (expected, final_plan) = build_pipeline_command_with_source_bitmap_carriage_plan(
                acceleration,
                pipeline,
                &independently_expected,
            )
            .unwrap();
            let final_plan = final_plan.unwrap();
            assert_eq!(final_command, expected);
            assert_eq!(final_plan.first_input_timing(), plan.first_input_timing());
            assert_eq!(final_plan.first_input_options(), plan.first_input_options());
            assert_eq!(final_plan.video_stream(), plan.video_stream());
            assert_eq!(final_plan.audio_stream(), plan.audio_stream());
            assert_eq!(final_plan.caption_input(), &independently_expected);
            assert_eq!(final_plan.enabled_caption_input_streams(), [0]);
            assert!(final_command
                .arguments
                .windows(2)
                .any(|pair| pair == ["-protocol_whitelist", "file"]));
            assert!(final_command
                .arguments
                .windows(2)
                .any(|pair| pair == ["-codec:s", "copy"]));
            assert!(!final_command.arguments.contains(&original.path));
            assert!(!plan.matches_command(&final_command));
            assert!(plan.matches_command(&initial));
        }
    }
}
#[test]
fn bitmap_request_never_accepts_a_different_caption_or_a_burn_plan() {
    let (original, playlist) = inputs();
    let burn_pipeline = fixture(Some(3), None);
    let burn =
        SourceTeletextConsumerRequest::new(HardwareAccelerationKind::None, burn_pipeline.clone());
    let (_, burn_plan) = burn.build().unwrap();
    assert!(burn
        .initialized_bitmap_command(&burn_plan.unwrap(), &playlist)
        .is_err());
    let mut pipeline = burn_pipeline;
    pipeline.inputs.pop();
    pipeline.inputs.push(original.clone());
    let request = SourceTeletextConsumerRequest::new_bitmap_carriage(
        HardwareAccelerationKind::None,
        pipeline.clone(),
        original.clone(),
    );
    let (_, plan) = request.build().unwrap();
    let mut wrong = original;
    wrong.path = "another-artifact.mks".into();
    let wrong = SourceTeletextConsumerRequest::new_bitmap_carriage(
        HardwareAccelerationKind::None,
        pipeline,
        wrong,
    );
    assert!(wrong.build().unwrap().1.is_none());
    assert!(wrong
        .initialized_bitmap_command(&plan.unwrap(), &playlist)
        .is_err());
}

#[test]
fn bitmap_interval_fences_actual_av_seek_even_with_an_independent_caption_owner() {
    let (original, _) = inputs();
    let mut pipeline = fixture(Some(3), None);
    pipeline.inputs.pop();
    pipeline.inputs.push(original.clone());
    let (_, plan) = SourceTeletextConsumerRequest::new_bitmap_carriage(
        HardwareAccelerationKind::None,
        pipeline,
        original,
    )
    .build()
    .unwrap();
    let plan = plan.unwrap();
    assert_ne!(plan.caption_physical_input_index(), 0);
    let interval = |start, duration| {
        super::super::super::SourceTeletextPlaybackInterval::from_start_and_duration(
            std::time::Duration::from_millis(start),
            std::time::Duration::from_secs(duration),
        )
        .unwrap()
    };
    assert!(!interval(500, 4).has_known_consumer_mismatch(&plan));
    assert!(interval(600, 4).has_known_consumer_mismatch(&plan));
    assert!(interval(500, 3).has_known_consumer_mismatch(&plan));
}

// Unix-only actual process fixture, matching remote_stream_source_caption/tests.rs.
// Production ownership is shared on all platforms; Windows native proof is separate.
#[cfg(unix)]
mod native_ownership {
    use std::{path::PathBuf, sync::Weak, time::Duration};

    use ersatzrs_core::locking::{EntityLocker, TeletextWorkGuard};
    use ersatzrs_domain::{MediaSourceKind, SourceTeletextBindingIdentity, TeletextPageNumber};

    use super::*;
    use crate::{
        ffprobe::teletext_decode::{
            SourceTeletextBitmapFragments, SourceTeletextPageDecodeRequest,
        },
        remote_stream::{
            spawn_ffmpeg_with_remote_input, RemoteStreamSpawnContext, SourceCaptionAttachmentError,
        },
        transcode_capacity::{
            active_transcode_snapshot, active_transcode_test_guard_for_tests,
            reset_active_transcode_state_for_tests, try_reserve_active_transcode,
            ActiveTranscodeSlot,
        },
    };

    struct OwnedFixture {
        bitmap: Arc<InitializedSourceTeletextBitmap>,
        paths: [PathBuf; 2],
        fragments: Weak<SourceTeletextBitmapFragments>,
    }

    #[allow(
        clippy::disallowed_methods,
        reason = "Explicit opt-in qualified synthetic source, never product configuration"
    )]
    fn source_path() -> PathBuf {
        let path = PathBuf::from(
            std::env::var_os("ERSATZRS_TELETEXT_BITMAP_ARTIFACT_SOURCE")
                .expect("set the qualified finite-caption lifecycle source"),
        );
        assert!(path.is_absolute());
        path
    }

    async fn prepare_owned_fixture() -> OwnedFixture {
        let path = source_path();
        let extractor = FfprobeExtractor::new(ersatzrs_ffmpeg::ffprobe_path());
        let scope = LocalProbeCleanupScope::new();
        let locker = EntityLocker::new();
        let work = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
        let result = scope
            .scope(async {
                let deadline = Instant::now() + Duration::from_secs(15);
                let acquired = extractor
                    .extract_local_with_teletext(&path)
                    .await
                    .map_err(|error| error.to_string())?;
                let request = SourceTeletextPageDecodeRequest {
                    binding: SourceTeletextBindingIdentity::new(
                        1,
                        MediaSourceKind::Local,
                        2,
                        3,
                        4,
                        acquired.teletext.unwrap().revision().clone(),
                    )
                    .unwrap(),
                    stream_index: 2,
                    pid: 258,
                    program_number: 1,
                    page: TeletextPageNumber::new(888).unwrap(),
                };
                let source = extractor
                    .decode_local_teletext_page(
                        request,
                        path.clone(),
                        path.clone(),
                        deadline,
                        work.hold_work(),
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                let artifact = extractor
                    .prepare_local_teletext_bitmap_artifact(
                        Arc::new(source),
                        deadline,
                        work.hold_work(),
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                let original = artifact
                    .carriage_input()
                    .map_err(|error| error.to_string())?;
                let fragments = Arc::new(
                    extractor
                        .isolate_local_teletext_bitmap_artifact_for_consumer(
                            Arc::new(artifact),
                            &[0, 3],
                            17_520_000,
                            deadline,
                            work.hold_work(),
                        )
                        .await
                        .map_err(|error| error.to_string())?,
                );
                let playlist = extractor
                    .prepare_local_teletext_bitmap_playlist(
                        fragments.clone(),
                        deadline,
                        work.hold_work(),
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                Ok::<_, String>((original, playlist, Arc::downgrade(&fragments)))
            })
            .await;
        scope.close();
        assert_eq!(scope.wait_cleanup().await, Ok(()));
        drop(work);
        assert_eq!(locker.active_teletext_work(), 0);
        let (original, playlist, fragments) = result.unwrap();
        seal_owned_fixture(original, playlist, fragments)
    }

    fn seal_owned_fixture(
        original: InputFile,
        playlist: SourceTeletextBitmapPlaylist,
        fragments: Weak<SourceTeletextBitmapFragments>,
    ) -> OwnedFixture {
        let paths = [
            PathBuf::from(&original.path),
            PathBuf::from(&playlist.input().path),
        ];
        // This generated-command fixture owns real prepared files, but supplies
        // no database selection or permission to play its synthetic AV locator.
        let mut pipeline = fixture(Some(3), None);
        pipeline.inputs.pop();
        pipeline.inputs.push(original.clone());
        let request = SourceTeletextConsumerRequest::new_bitmap_carriage(
            HardwareAccelerationKind::None,
            pipeline,
            original,
        );
        let (_, plan) = request.build().unwrap();
        let command = request
            .initialized_bitmap_command(&plan.unwrap(), playlist.input())
            .unwrap();
        OwnedFixture {
            bitmap: Arc::new(InitializedSourceTeletextBitmap {
                playlist,
                command,
                _hls_producer_directory: None,
            }),
            paths,
            fragments,
        }
    }

    async fn mismatch_retains_until_reaped() {
        let OwnedFixture {
            bitmap,
            paths,
            fragments,
        } = Box::pin(prepare_owned_fixture()).await;
        let weak = Arc::downgrade(&bitmap);
        let capacity = try_reserve_active_transcode(
            1,
            ActiveTranscodeSlot::new_mpeg_ts("bitmap-mismatch-owner"),
        )
        .unwrap();
        let mut child = spawn_ffmpeg_with_remote_input(
            &FfmpegCommand::new_with_executable(
                "sh",
                vec![],
                vec!["-c".into(), "exec sleep 30".into()],
            ),
            None,
            None,
            RemoteStreamSpawnContext::probe(),
        )
        .unwrap();
        let attached = child.attach_initialized_teletext_bitmap(bitmap, capacity);
        let retained = weak.upgrade().is_some()
            && fragments.upgrade().is_some()
            && paths.iter().all(|path| path.is_file());
        let occupied = active_transcode_snapshot(1).active;
        // Capture observations before assertions so a regression still reaps
        // the actual child, rather than intentionally leaking an owner/slot.
        let killed = tokio::time::timeout(Duration::from_secs(5), child.kill()).await;
        let retained_after_reap =
            weak.upgrade().is_some() && paths.iter().all(|path| path.is_file());
        drop(child);
        assert_eq!(attached, Err(SourceCaptionAttachmentError::Command));
        assert!(retained);
        assert_eq!(occupied, 1);
        assert!(killed.unwrap().is_ok());
        assert!(retained_after_reap);
        assert!(weak.upgrade().is_none());
        assert!(fragments.upgrade().is_none());
        assert!(paths.iter().all(|path| !path.exists()));
        assert_eq!(active_transcode_snapshot(1).active, 0);
    }

    #[test]
    #[ignore = "requires pinned FFmpeg/ffprobe9, finite-caption lifecycle source and Unix child"]
    fn native_bitmap_command_mismatch_retains_files_and_capacity_until_confirmed_reap() {
        let _guard = active_transcode_test_guard_for_tests();
        reset_active_transcode_state_for_tests();
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap()
            .block_on(mismatch_retains_until_reaped());
        reset_active_transcode_state_for_tests();
    }
}
