//! Own the same admitted preparation, process supervisor and callback finish as the web adapter.
use super::*;
use ersatzrs_infra::transcode_capacity::{
    active_transcode_snapshot, try_reserve_active_transcode, ActiveTranscodeSlot,
};

pub(super) async fn complete(
    fixture: &Fixture,
    query: &PlaybackPreviewQuery,
    scenario: Scenario,
) -> ApiResult<()> {
    let mut paths = fixture.paths();
    paths.executable = runtime_path("ERSATZRS_NEXT_TEST_BIN");
    let mut ownership = Arc::clone(&fixture.state.preview_lock).try_lock_owned()?;
    let reservation = try_reserve_active_transcode(
        2,
        ActiveTranscodeSlot::new_next_hls_with_capacity_key(
            TRANSCODE_TROUBLESHOOTING_CHANNEL,
            "troubleshooting",
        ),
    )
    .ok_or("fixture admission refused")?;
    let mut session = None;
    let result = AssertUnwindSafe(complete_admitted(
        fixture,
        query,
        scenario,
        &paths,
        &mut ownership,
        &mut session,
    ))
    .catch_unwind()
    .await;
    let cleanup = if let Some(instance) = session {
        fixture
            .state
            .next_engine
            .finish_troubleshooting_session(instance)
            .await
    } else {
        Ok(())
    };
    if cleanup.is_err() || matches!(*ownership, PreviewAvailability::CleanupUnconfirmed { .. }) {
        *ownership = PreviewAvailability::CleanupUnconfirmed {
            _reservation: Some(reservation),
        };
        return Err("owned Next cleanup is unconfirmed; preserve fixture and admission".into());
    }
    *ownership = PreviewAvailability::Available;
    drop(ownership);
    drop(reservation);
    assert_eq!(active_transcode_snapshot(2).active, 0);
    match result {
        Ok(result) => result,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

async fn complete_admitted(
    fixture: &Fixture,
    query: &PlaybackPreviewQuery,
    scenario: Scenario,
    paths: &RuntimePaths,
    ownership: &mut PreviewOwnership,
    session: &mut Option<uuid::Uuid>,
) -> ApiResult<()> {
    let deadline = Instant::now() + Duration::from_secs(30);
    let prepared = tokio::time::timeout_at(
        deadline,
        prepare(&fixture.state, query, paths, deadline, session),
    )
    .await;
    let (manifest, configuration, number, diagnostics) =
        prepared.map_err(|_| "actual window preparation timed out")??;
    evidence::configuration(fixture, &configuration, &number, scenario).await;
    let mut entries = tokio::fs::read_dir(manifest.path()).await?;
    let entry = entries
        .next_entry()
        .await?
        .ok_or("missing window manifest")?;
    let bytes = tokio::fs::read(entry.path()).await?;
    let playout: protocol::Playout = serde_json::from_slice(&bytes)?;
    assert!(entries.next_entry().await?.is_none());
    evidence::manifest(
        &playout,
        configuration.playout.virtual_start.unwrap(),
        scenario,
    );
    tokio::fs::write(fixture.root.path().join("actual-window.json"), &bytes).await?;
    // A stale publication-style child must not replace the actual top-level window.
    tokio::fs::create_dir(manifest.path().join("current")).await?;
    tokio::fs::write(
        manifest.path().join("current/decoy.json"),
        br#"{"version":"unsupported-row201-decoy","items":[null]}"#,
    )
    .await?;
    let process = NextProcess::start_troubleshooting_with_session(
        &paths.executable,
        &number,
        &paths.output,
        &configuration,
        paths.overlays.as_deref(),
        *session,
        deadline,
    )
    .await;
    let mut process = match process {
        Ok(process) => process,
        Err(error) => {
            if matches!(
                error,
                ProcessError::CleanupTimeout
                    | ProcessError::Io {
                        operation: "stop",
                        ..
                    }
            ) {
                **ownership = PreviewAvailability::CleanupUnconfirmed { _reservation: None };
                let _ = manifest.keep();
            }
            return Err(error.into());
        }
    };
    let (sender, receiver) = oneshot::channel();
    let mut ready = Some(sender);
    let result = supervise(
        &mut process,
        &paths.output,
        ownership,
        &mut ready,
        &diagnostics,
    )
    .await;
    if matches!(**ownership, PreviewAvailability::CleanupUnconfirmed { .. }) {
        let _ = manifest.keep();
    }
    result?;
    receiver.await??;
    Ok(())
}

pub(super) async fn cancel(fixture: &Fixture, query: &PlaybackPreviewQuery) -> ApiResult<()> {
    let request = PlaybackPreviewRequest {
        streaming_engine: query.streaming_engine,
        channel_id: query.channel,
        ffmpeg_profile_id: query.ffmpeg_profile,
        preferred_audio_title: query.preferred_audio_title.clone(),
        subtitle_id: query.subtitle_id,
        start: query.start.clone(),
        ..Default::default()
    };
    let job = fixture.state.admit_preview(request).await?;
    let ready = timeout(Duration::from_secs(75), job.wait_ready()).await;
    job.cancel();
    let terminal = timeout(Duration::from_secs(20), async {
        loop {
            let status = job.read();
            if status.state.is_terminal() {
                break status;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await;
    // Cleanup is asserted before reporting readiness errors: the browser's Stop owns
    // the worker even when readiness timed out or failed.
    let status = terminal.map_err(|_| "cancelled preview did not finish owned cleanup")?;
    if !status.cleanup_confirmed {
        return Err("cancelled preview cleanup unconfirmed".into());
    }
    assert_eq!(status.state, ersatzrs_app::PlaybackPreviewState::Cancelled);
    assert!(!status.ready);
    assert_eq!(active_transcode_snapshot(2).active, 0);
    assert!(matches!(
        *fixture.state.preview_lock.lock().await,
        PreviewAvailability::Available
    ));
    ready.map_err(|_| "actual preview did not become ready before Stop")??;
    tokio::fs::write(
        fixture.root.path().join("actual-stop.json"),
        serde_json::to_vec_pretty(&status)?,
    )
    .await?;
    assert!(
        status.log.is_some(),
        "owned stopped runtime must retain its diagnostics"
    );
    Ok(())
}
