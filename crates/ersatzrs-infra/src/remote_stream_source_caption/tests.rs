//! Unix process fixtures exercise actual consumer ownership using a genuinely
//! decoded synthetic page. Windows retains the same production counterpart;
//! its native process acceptance remains a separate platform gate.

#[cfg(unix)]
mod unix {
    use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Duration};

    use ersatzrs_core::locking::{EntityLocker, TeletextWorkGuard};
    use ersatzrs_domain::{MediaSourceKind, SourceTeletextBindingIdentity, TeletextPageNumber};
    use ersatzrs_ffmpeg::FfmpegCommand;
    use nix::{
        sys::signal::{kill, Signal},
        unistd::Pid,
    };
    use tokio::time::Instant;

    use super::super::*;
    use crate::{
        ffprobe::{
            teletext_decode::SourceTeletextPageDecodeRequest, FfprobeExtractor,
            LocalProbeCleanupScope,
        },
        remote_stream::{
            spawn_ffmpeg_with_remote_input, RemoteStreamCommand, RemoteStreamSpawnContext,
        },
        transcode_capacity::{
            active_transcode_snapshot, active_transcode_test_guard_for_tests,
            reset_active_transcode_state_for_tests, try_reserve_active_transcode,
            ActiveTranscodeSlot,
        },
    };

    #[allow(
        clippy::disallowed_methods,
        reason = "Explicit opt-in pinned decoder and synthetic source input, never product configuration"
    )]
    fn inputs() -> (String, PathBuf) {
        (
            std::env::var("FFPROBE_PATH").expect("set pinned native FFprobe 9"),
            std::env::var_os("ERSATZRS_TELETEXT_ACQUISITION_FIXTURE")
                .map(PathBuf::from)
                .expect("set redistributable two-audio AV/200/888 fixture"),
        )
    }

    async fn page() -> (
        tempfile::TempDir,
        Arc<DecodedLocalTeletextPage>,
        EntityLocker,
    ) {
        let (probe, source) = inputs();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("synthetic-source.ts");
        std::fs::copy(source, &path).unwrap();
        let extractor = FfprobeExtractor::new(probe);
        let scope = LocalProbeCleanupScope::new();
        let acquired = scope
            .scope(extractor.extract_local_with_teletext(&path))
            .await
            .unwrap();
        let acquisition = acquired.teletext.unwrap();
        let locker = EntityLocker::new();
        let work = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
        // These IDs label a component request only, without claiming database
        // ownership or full source-page readiness.
        let request = SourceTeletextPageDecodeRequest {
            binding: SourceTeletextBindingIdentity::new(
                1,
                MediaSourceKind::Local,
                2,
                3,
                4,
                acquisition.revision().clone(),
            )
            .unwrap(),
            stream_index: 3,
            pid: 0x103,
            program_number: 1,
            page: TeletextPageNumber::new(888).unwrap(),
        };
        let decoded = scope
            .scope(extractor.decode_local_teletext_page(
                request,
                path.clone(),
                path,
                Instant::now() + Duration::from_secs(15),
                work.hold_work(),
            ))
            .await
            .unwrap();
        assert!(decoded.events().count > 0);
        scope.close();
        scope.wait_cleanup().await.unwrap();
        drop(work);
        assert_eq!(locker.active_teletext_work(), 0);
        (directory, Arc::new(decoded), locker)
    }

    fn command(script: &str) -> FfmpegCommand {
        FfmpegCommand::new_with_executable("sh", vec![], vec!["-c".into(), script.into()])
    }

    fn consumer() -> RemoteStreamProcess {
        spawn_ffmpeg_with_remote_input(
            &command("exec sleep 30"),
            None,
            None,
            RemoteStreamSpawnContext::probe(),
        )
        .unwrap()
    }

    fn attach(process: &mut RemoteStreamProcess, page: &Arc<DecodedLocalTeletextPage>) {
        let capacity =
            try_reserve_active_transcode(1, ActiveTranscodeSlot::new_mpeg_ts("source-owner"))
                .unwrap();
        process
            .attach_local_teletext_source(page.clone(), capacity.clone())
            .unwrap();
        drop(capacity);
        assert_eq!(active_transcode_snapshot(1).active, 1);
    }

    async fn wait_confirms_even_unsuccessful_exit(page: &Arc<DecodedLocalTeletextPage>) {
        let baseline = Arc::strong_count(page);
        let mut process = consumer();
        attach(&mut process, page);
        let pid = Pid::from_raw(i32::try_from(process.id().unwrap()).unwrap());
        kill(pid, Signal::SIGTERM).unwrap();
        let status = process.wait().await.unwrap();
        assert!(!status.success());
        assert!(process.source_caption.as_ref().unwrap().reaped);
        assert_eq!(Arc::strong_count(page), baseline + 1);
        drop(process);
        assert_eq!(Arc::strong_count(page), baseline);
        assert_eq!(active_transcode_snapshot(1).active, 0);
    }

    async fn kill_confirms_and_releases_only_after_owner_disposal(
        page: &Arc<DecodedLocalTeletextPage>,
    ) {
        let baseline = Arc::strong_count(page);
        let mut process = consumer();
        attach(&mut process, page);
        process.kill().await.unwrap();
        assert!(process.source_caption.as_ref().unwrap().reaped);
        assert_eq!(active_transcode_snapshot(1).active, 1);
        drop(process);
        assert_eq!(Arc::strong_count(page), baseline);
        assert_eq!(active_transcode_snapshot(1).active, 0);
    }

    async fn cancelled_wait_does_not_confirm_or_release(
        page: &Arc<DecodedLocalTeletextPage>,
        preparation_locker: &EntityLocker,
    ) {
        let mut process = consumer();
        attach(&mut process, page);
        assert!(
            tokio::time::timeout(Duration::from_millis(20), process.wait())
                .await
                .is_err()
        );
        assert!(!process.source_caption.as_ref().unwrap().reaped);
        assert_eq!(active_transcode_snapshot(1).active, 1);
        let other_preparation = TeletextWorkGuard::try_acquire(preparation_locker, 1)
            .expect("actual playback must not retain global teletext preparation admission");
        drop(other_preparation);
        assert!(
            try_reserve_active_transcode(1, ActiveTranscodeSlot::new_mpeg_ts("other")).is_none()
        );
        process.kill().await.unwrap();
        drop(process);
        assert_eq!(active_transcode_snapshot(1).active, 0);
    }

    async fn start_kill_and_unconfirmed_drop_quarantine_the_actual_source(
        page: &Arc<DecodedLocalTeletextPage>,
    ) {
        let baseline = Arc::strong_count(page);
        let mut process = consumer();
        attach(&mut process, page);
        process.start_kill().unwrap();
        assert!(!process.source_caption.as_ref().unwrap().reaped);
        // Reap directly solely to leave no live process in this fixture. The
        // public owner received no successful wait/kill confirmation, so its
        // Drop must still take the conservative uncertainty path.
        process.child.wait().await.unwrap();
        assert!(!process.source_caption.as_ref().unwrap().reaped);
        drop(process);
        assert_eq!(active_transcode_snapshot(1).active, 1);
        assert!(
            try_reserve_active_transcode(1, ActiveTranscodeSlot::new_mpeg_ts("other")).is_none()
        );
        assert_eq!(Arc::strong_count(page), baseline + 1);
        reset_active_transcode_state_for_tests();
    }

    async fn duplicate_and_exited_attachment_refusals_do_not_add_quarantine(
        page: &Arc<DecodedLocalTeletextPage>,
    ) {
        let mut process = consumer();
        let capacity =
            try_reserve_active_transcode(1, ActiveTranscodeSlot::new_mpeg_ts("source-owner"))
                .unwrap();
        process
            .attach_local_teletext_source(page.clone(), capacity.clone())
            .unwrap();
        assert_eq!(
            process.attach_local_teletext_source(page.clone(), capacity.clone()),
            Err(SourceCaptionAttachmentError::AlreadyAttached)
        );
        process.kill().await.unwrap();
        drop(process);
        drop(capacity);
        assert_eq!(active_transcode_snapshot(1).active, 0);

        let mut exited = consumer();
        exited.kill().await.unwrap();
        let capacity =
            try_reserve_active_transcode(1, ActiveTranscodeSlot::new_mpeg_ts("exited")).unwrap();
        assert_eq!(
            exited.attach_local_teletext_source(page.clone(), capacity.clone()),
            Err(SourceCaptionAttachmentError::Exited)
        );
        drop(capacity);
        drop(exited);
        assert_eq!(active_transcode_snapshot(1).active, 0);
    }

    async fn provider_attachment_is_refused_without_changing_producer_cleanup(
        page: &Arc<DecodedLocalTeletextPage>,
    ) {
        let producer = RemoteStreamCommand {
            tool_paths: vec![],
            program: "sh".into(),
            args: vec!["-c".into(), "printf synthetic-provider-bytes".into()],
            working_directory: PathBuf::from("."),
            environment: BTreeMap::new(),
        };
        let mut process = spawn_ffmpeg_with_remote_input(
            &command("cat"),
            None,
            Some(&producer),
            RemoteStreamSpawnContext::probe(),
        )
        .unwrap();
        let capacity =
            try_reserve_active_transcode(1, ActiveTranscodeSlot::new_mpeg_ts("provider")).unwrap();
        assert_eq!(
            process.attach_local_teletext_source(page.clone(), capacity.clone()),
            Err(SourceCaptionAttachmentError::Producer)
        );
        assert!(process.source_caption.is_none());
        drop(capacity);
        assert_eq!(active_transcode_snapshot(1).active, 0);
        assert!(process.wait().await.unwrap().success());
    }

    #[test]
    #[ignore = "requires pinned FFprobe 9, synthetic AV/200/888 input and Unix actual process fixtures"]
    fn native_source_caption_child_ownership_confirms_reaping_and_quarantines_uncertainty() {
        let _guard = active_transcode_test_guard_for_tests();
        reset_active_transcode_state_for_tests();
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let (_directory, page, preparation_locker) = page().await;
                wait_confirms_even_unsuccessful_exit(&page).await;
                kill_confirms_and_releases_only_after_owner_disposal(&page).await;
                cancelled_wait_does_not_confirm_or_release(&page, &preparation_locker).await;
                start_kill_and_unconfirmed_drop_quarantine_the_actual_source(&page).await;
                duplicate_and_exited_attachment_refusals_do_not_add_quarantine(&page).await;
                provider_attachment_is_refused_without_changing_producer_cleanup(&page).await;
                assert_eq!(preparation_locker.active_teletext_work(), 0);
                assert!(TeletextWorkGuard::try_acquire(&preparation_locker, 1).is_some());
            });
        assert_eq!(active_transcode_snapshot(1).active, 0);
        reset_active_transcode_state_for_tests();
    }
}
