//! Upstream Prepare/StartTroubleshootingPlayback for a finite Next preview.
//! Preparation and process supervision keep the shared output ownership even
//! when the requesting browser disconnects. No preview playout is persisted.

use super::*;
use chrono::Duration as TimeDelta;
use ersatzrs_core::repositories::ChannelRepository as _;
use ersatzrs_domain::{PlayoutItem, StreamingEngine};
use ersatzrs_infra::next_engine::{
    config,
    diagnostics::SelectionLog,
    host::channel_binary,
    process::{NextProcess, ProcessError},
    protocol, schedule,
    troubleshooting::TroubleshootingPreview,
};
use ersatzrs_infra::transcode_capacity::{
    load_active_transcode_limit, try_reserve_active_transcode, ActiveTranscodeSlot,
    DEFAULT_ACTIVE_TRANSCODE_LIMIT,
};
use tokio::{
    sync::oneshot,
    time::{timeout_at, Instant},
};

type Readiness = Option<oneshot::Sender<Result<(), &'static str>>>;

pub(super) async fn start(
    state: NativeAdminTroubleshootingRuntime,
    query: PlaybackPreviewQuery,
    mut ownership: PreviewOwnership,
) -> Result<(), AdminTroubleshootingRuntimeError> {
    let (sender, receiver) = oneshot::channel();
    tokio::spawn(async move {
        let mut ready = Some(sender);
        let job = lifecycle::job(&ownership);
        let result = Box::pin(run(&state, &query, &mut ownership, &mut ready)).await;
        if let Err(error) = &result {
            tracing::warn!("Next troubleshooting preview failed");
            if let Some(job) = &job {
                if !job.cancel.is_cancelled() {
                    job.fail(lifecycle::startup_code(&error.to_string()));
                }
            }
        }
        lifecycle::complete_ownership(ownership, job, result.is_ok(), &troubleshooting_folder())
            .await;
        if result.is_err() {
            notify(
                &mut ready,
                Err("Next troubleshooting preview could not start"),
            );
        }
    });
    receiver
        .await
        .map_err(|_| AdminTroubleshootingRuntimeError::unavailable("Next preview task stopped"))?
        .map_err(AdminTroubleshootingRuntimeError::unavailable)
}

struct Selection {
    channel: Channel,
    media: schedule::ScheduledMedia,
    info: PreviewMediaItem,
    profile: PlaybackProfile,
    selected_at: Option<DateTime<Utc>>,
    playback_start: DateTime<Utc>,
    subtitle_id: Option<i64>,
}

async fn select(
    state: &NativeAdminTroubleshootingRuntime,
    query: &PlaybackPreviewQuery,
) -> ApiResult<Selection> {
    let start = DateTime::from_timestamp(Utc::now().timestamp(), 0)
        .ok_or(PreviewNotFound("invalid preview clock"))?;
    let (mut channel, mut media, selected_at) = if let Some(id) = query.channel.filter(|id| *id > 0)
    {
        let channel = state.repositories.channel.get_with_profile(id).await?;
        if !channel.is_enabled {
            return Err(PreviewNotFound("channel is disabled").into());
        }
        let at = requested_preview_start(query.start.as_deref())?;
        let mut media = state
            .next_engine
            .troubleshooting_channel_media(&channel, at)
            .await?
            .ok_or(PreviewNotFound("channel has no active playout item"))?;
        let at = programme_preview_time(query, media.item.start, media.item.finish, at)?;
        join_channel_item(&mut media.item, at, start)?;
        (channel, media, Some(at))
    } else {
        let id = query
            .media_item
            .filter(|id| *id > 0)
            .ok_or(PreviewNotFound(
                "media item troubleshooting preview requires mediaItem",
            ))?;
        let mut media = schedule::bind_media(
            &state.pool,
            PlayoutItem {
                media_item_id: id,
                start,
                subtitle_mode: Some(ChannelSubtitleMode::Any),
                disable_watermarks: query.watermark.is_empty(),
                ..PlayoutItem::default()
            },
        )
        .await?;
        let info = state
            .repositories
            .troubleshooting
            .load_preview_media_admin(id, Some(media.media_version_id))
            .await?
            .ok_or(PreviewNotFound("media item does not exist"))?;
        let seek = preview_media_seek_seconds(state, &info, query.seek_seconds).await?;
        seek_media_item(&mut media.item, Some(seek), info.duration_seconds)?;
        (
            preview_selector_channel(query, StreamingMode::HttpLiveStreamingSegmenter),
            media,
            None,
        )
    };
    channel.streaming_engine = StreamingEngine::Next;
    let mut subtitle_id = query.subtitle_id;
    if selected_at.is_none() {
        if channel.stream_selector.is_some() {
            channel.stream_selector_mode = ersatzrs_domain::ChannelStreamSelectorMode::Custom;
        } else {
            // Upstream media troubleshooting supplies no subtitle candidates
            // unless the operator chooses a track or a custom selector.
            subtitle_id = Some(subtitle_id.unwrap_or(0));
        }
    }
    let profile_id = query
        .ffmpeg_profile
        .filter(|id| *id > 0)
        .or_else(|| selected_at.map(|_| channel.ffmpeg_profile_id))
        .ok_or(PreviewNotFound(
            "playback troubleshooting preview requires ffmpegProfile",
        ))?;
    let profile = load_playback_profile(&state.pool, profile_id)
        .await?
        .ok_or(PreviewNotFound("ffmpeg profile does not exist"))?;
    if let Some(title) = query
        .preferred_audio_title
        .as_ref()
        .filter(|title| !title.trim().is_empty())
    {
        media.item.preferred_audio_title = Some(title.clone());
    }
    let mut info = state
        .repositories
        .troubleshooting
        .load_preview_media_admin(media.item.media_item_id, Some(media.media_version_id))
        .await?
        .ok_or(PreviewNotFound("media item does not exist"))?;
    info.media_version_id = media.media_version_id;
    info.path.clone_from(&media.path);
    channel = credits::channel(query, &info, &channel);
    if credits::enabled(&channel, &info) {
        subtitle_id = Some(0);
    }
    Ok(Selection {
        channel,
        media,
        info,
        profile,
        selected_at,
        playback_start: start,
        subtitle_id,
    })
}

fn seek_media_item(
    item: &mut PlayoutItem,
    seek_seconds: Option<i64>,
    duration_seconds: f64,
) -> ApiResult<()> {
    let seek = preview_seek_seconds(seek_seconds, duration_seconds);
    let duration = TimeDelta::from_std(Duration::from_secs_f64(preview_duration_seconds(
        duration_seconds,
        seek,
    )))?;
    let seek = TimeDelta::try_seconds(seek).ok_or(PreviewNotFound("invalid preview seek"))?;
    // Join a running programme: Next applies elapsed item time to every track
    // and canvas. An additional source in-point would seek the media twice.
    item.finish = item.start + duration;
    item.start = item
        .start
        .checked_sub_signed(seek)
        .ok_or(PreviewNotFound("invalid preview programme start"))?;
    item.in_point = TimeDelta::zero();
    item.out_point = seek
        .checked_add(&duration)
        .ok_or(PreviewNotFound("invalid preview source range"))?;
    Ok(())
}

fn join_channel_item(
    item: &mut PlayoutItem,
    at: DateTime<Utc>,
    start: DateTime<Utc>,
) -> ApiResult<()> {
    let duration = (item.finish - at).min(TimeDelta::seconds(30));
    item.in_point = item
        .in_point
        .checked_add(&(at - item.start))
        .ok_or(PreviewNotFound("invalid preview source range"))?;
    item.out_point = item
        .in_point
        .checked_add(&duration)
        .ok_or(PreviewNotFound("invalid preview source range"))?;
    item.start = start;
    item.finish = start + duration;
    Ok(())
}

async fn prepare(
    state: &NativeAdminTroubleshootingRuntime,
    query: &PlaybackPreviewQuery,
    paths: &RuntimePaths,
    deadline: Instant,
    session: &mut Option<uuid::Uuid>,
) -> ApiResult<(
    tempfile::TempDir,
    protocol::ChannelConfig,
    String,
    SelectionLog,
)> {
    Box::pin(prepare_inner(state, query, paths, deadline, session)).await
}

async fn prepare_inner(
    state: &NativeAdminTroubleshootingRuntime,
    query: &PlaybackPreviewQuery,
    paths: &RuntimePaths,
    deadline: Instant,
    session: &mut Option<uuid::Uuid>,
) -> ApiResult<(
    tempfile::TempDir,
    protocol::ChannelConfig,
    String,
    SelectionLog,
)> {
    let selected = select(state, query).await?;
    state
        .record_preview_selection(selected.selected_at.unwrap_or(selected.playback_start))
        .await;
    let runtime = state.ffmpeg_runtime.read().await.clone();
    let replacements = stream::load_path_replacements(&state.repositories.media_source).await?;
    let diagnostics = SelectionLog::default();
    let request = TroubleshootingPreview {
        media: &selected.media,
        channel: &selected.channel,
        profile: &selected.profile,
        subtitle_id: selected.subtitle_id,
        selection_time: selected.selected_at,
        watermark_ids: &query.watermark,
        graphics_ids: &query.graphics_element,
        replacements: &replacements,
        ffmpeg: FsPath::new(&runtime.ffmpeg.path),
        deadline,
        diagnostics: Some(diagnostics.clone()),
    };
    let start = ersatzrs_infra::next_engine::host::NextStart {
        channel_id: selected.channel.id,
        preview: true,
        number: selected.channel.number.clone(),
        executable: paths.executable.clone(),
        output: paths.output.clone(),
        ffmpeg: runtime.ffmpeg.path.clone().into(),
        ffprobe: runtime.ffprobe.path.clone().into(),
        replacements: replacements.clone(),
        idle_timeout: None,
        deadline,
    };
    let (instance, item) = state
        .next_engine
        .prepare_troubleshooting_session(request, start)
        .await?;
    *session = Some(instance);
    let manifest = tempfile::tempdir()?;
    let working = &paths.output;
    let mut configuration = config::channel_config(
        &selected.profile,
        selected.channel.next_engine_text_subtitle_mode,
        manifest.path(),
        FsPath::new(&runtime.ffmpeg.path),
        FsPath::new(&runtime.ffprobe.path),
    )?;
    configuration.playout.virtual_start = Some(selected.playback_start);
    configuration.ffmpeg.reports_folder = Some(
        working
            .to_str()
            .ok_or("invalid output path encoding")?
            .to_owned(),
    );
    configuration.normalization.subtitle.fonts_folder = Some(
        paths
            .fonts
            .to_str()
            .ok_or("invalid fonts path encoding")?
            .to_owned(),
    );
    let name = format!(
        "{}_{}.json",
        item.start.timestamp_millis(),
        item.finish.timestamp_millis()
    );
    let mut playout = protocol::Playout {
        version: protocol::PLAYOUT_VERSION.to_owned(),
        items: vec![item],
    };
    prepare_troubleshooting_folder(working).await?;
    write_preview_artifacts(state, query, &selected.info, &selected.profile, working).await?;
    attach_preview_credits(&state.pool, &selected, working, &mut playout).await?;
    tokio::fs::write(manifest.path().join(&name), serde_json::to_vec(&playout)?).await?;
    Ok((
        manifest,
        configuration,
        selected.channel.number,
        diagnostics,
    ))
}

async fn attach_preview_credits(
    pool: &DbPool,
    selected: &Selection,
    working: &FsPath,
    playout: &mut protocol::Playout,
) -> ApiResult<()> {
    if let Some(subtitle) = credits::prepare(
        pool,
        &selected.info,
        &selected.channel,
        &selected.profile,
        selected.media.item.in_point.to_std()?,
        working,
    )
    .await?
    {
        let tracks = playout.items[0]
            .tracks
            .as_mut()
            .ok_or("preview tracks are unavailable")?;
        tracks.subtitle = Some(protocol::Track {
            stream_index: Some(0),
            source: Some(protocol::Source::Local {
                path: subtitle.path.ok_or("preview credits path is unavailable")?,
                probe_hint: None,
                in_point_ms: Some(0),
                out_point_ms: None,
            }),
        });
    }
    Ok(())
}

struct RuntimePaths {
    executable: PathBuf,
    output: PathBuf,
    overlays: Option<PathBuf>,
    fonts: PathBuf,
}

async fn run(
    state: &NativeAdminTroubleshootingRuntime,
    query: &PlaybackPreviewQuery,
    ownership: &mut PreviewOwnership,
    ready: &mut Readiness,
) -> ApiResult<()> {
    let paths = RuntimePaths {
        executable: channel_binary()?,
        output: troubleshooting_folder(),
        overlays: Some(ersatzrs_infra::file_system_layout::next_channel_config_overlays_folder()),
        fonts: ersatzrs_infra::file_system_layout::fonts_cache_folder(),
    };
    run_with_paths(state, query, ownership, ready, &paths).await
}

async fn run_with_paths(
    state: &NativeAdminTroubleshootingRuntime,
    query: &PlaybackPreviewQuery,
    ownership: &mut PreviewOwnership,
    ready: &mut Readiness,
    paths: &RuntimePaths,
) -> ApiResult<()> {
    let limit = load_active_transcode_limit(&state.pool)
        .await?
        .unwrap_or(DEFAULT_ACTIVE_TRANSCODE_LIMIT);
    let reservation = try_reserve_active_transcode(
        limit,
        ActiveTranscodeSlot::new_next_hls_with_capacity_key(
            TRANSCODE_TROUBLESHOOTING_CHANNEL,
            "troubleshooting",
        ),
    )
    .ok_or("Next troubleshooting capacity is unavailable")?;
    let mut session = None;
    let result = run_admitted(state, query, ownership, ready, paths, &mut session).await;
    let cleanup = if let Some(instance) = session {
        state
            .next_engine
            .finish_troubleshooting_session(instance)
            .await
    } else {
        Ok(())
    };
    if matches!(
        cleanup,
        Err(ersatzrs_infra::remote_stream::RemoteStreamError::PreparedTerminationUnconfirmed)
    ) || matches!(**ownership, PreviewAvailability::CleanupUnconfirmed { .. })
    {
        **ownership = PreviewAvailability::CleanupUnconfirmed {
            _reservation: Some(reservation),
        };
    }
    cleanup.map_err(|_| "Next troubleshooting producer cleanup failed")?;
    result
}

async fn run_admitted(
    state: &NativeAdminTroubleshootingRuntime,
    query: &PlaybackPreviewQuery,
    ownership: &mut PreviewOwnership,
    ready: &mut Readiness,
    paths: &RuntimePaths,
    session: &mut Option<uuid::Uuid>,
) -> ApiResult<()> {
    let deadline = Instant::now() + Duration::from_secs(30);
    let cancellation = lifecycle::job(ownership)
        .map(|job| job.cancel.clone())
        .unwrap_or_default();
    let (manifest, configuration, number, diagnostics) = tokio::select! {
        prepared = timeout_at(deadline, prepare(state, query, paths, deadline, session)) => {
            prepared.map_err(|_| "Next troubleshooting preparation timed out")??
        }
        () = cancellation.cancelled() => return Err("Next troubleshooting cancelled".into()),
    };
    let mut process = match NextProcess::start_troubleshooting_with_session(
        &paths.executable,
        &number,
        &paths.output,
        &configuration,
        paths.overlays.as_deref(),
        *session,
        deadline,
    )
    .await
    {
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
    if let Some(job) = lifecycle::job(ownership) {
        job.running();
    }
    let result = supervise(&mut process, &paths.output, ownership, ready, &diagnostics).await;
    if matches!(**ownership, PreviewAvailability::CleanupUnconfirmed { .. }) {
        let _ = manifest.keep();
    }
    result
}

async fn supervise(
    process: &mut NextProcess,
    working: &FsPath,
    ownership: &mut PreviewOwnership,
    ready: &mut Readiness,
    diagnostics: &SelectionLog,
) -> ApiResult<()> {
    let cancellation = lifecycle::job(ownership)
        .map(|job| job.cancel.clone())
        .unwrap_or_default();
    let result = tokio::select! {
        result = timeout_at(Instant::now() + Duration::from_mins(2), async {
        tokio::select! {
            status = process.wait() => {
                let status = status?;
                // A short successful preview can finish before Next publishes
                // its four-segment readiness marker. Only reaped success may
                // use the final generated playlist in that case.
                if !cancellation.is_cancelled()
                    && completed_output_ready(working, status.success()).await?
                    && !cancellation.is_cancelled()
                {
                    if let Some(job) = lifecycle::job(ownership) { job.ready(); }
                    notify(ready, Ok(()));
                }
                Ok::<_, Box<dyn std::error::Error + Send + Sync>>(status)
            }
            result = wait_output(working) => {
                result?;
                if !cancellation.is_cancelled() {
                    if let Some(job) = lifecycle::job(ownership) { job.ready(); }
                    notify(ready, Ok(()));
                }
                Ok(process.wait().await?)
            }
        }
    })
        => result,
        () = cancellation.cancelled() => Ok(Err("Next troubleshooting cancelled".into())),
    };
    let success = matches!(&result, Ok(Ok(status)) if status.success());
    if !success && process.stop().await.is_err() {
        **ownership = PreviewAvailability::CleanupUnconfirmed { _reservation: None };
    }
    let outcome = if cancellation.is_cancelled() {
        "Next troubleshooting cancelled"
    } else if success {
        "Next troubleshooting completed"
    } else {
        "Next troubleshooting failed or timed out"
    };
    let mut log = diagnostics.text();
    log.push_str(&process.diagnostics());
    log.push_str(outcome);
    if let Some(speed) = progress::completed_speed(working, "").await {
        log.push_str(&progress::speed_diagnostic(speed));
    }
    tokio::fs::write(working.join("logs.txt"), log).await?;
    if !success {
        return Err("Next troubleshooting failed or timed out".into());
    }
    notify(
        ready,
        Err("Next troubleshooting did not produce playable output"),
    );
    Ok(())
}

fn notify(ready: &mut Readiness, value: Result<(), &'static str>) {
    if let Some(sender) = ready.take() {
        let _ = sender.send(value);
    }
}

async fn output_ready(folder: &FsPath) -> ApiResult<bool> {
    Ok(tokio::fs::try_exists(folder.join("live.m3u8")).await?
        && tokio::fs::try_exists(folder.join(".ready")).await?)
}

async fn completed_output_ready(folder: &FsPath, succeeded: bool) -> ApiResult<bool> {
    if !succeeded {
        return Ok(false);
    }
    let playlist = match tokio::fs::read_to_string(folder.join("live.m3u8")).await {
        Ok(playlist) => playlist,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    if !playlist.lines().any(|line| line.trim() == "#EXT-X-ENDLIST") {
        return Ok(false);
    }
    for segment in playlist.lines().map(str::trim) {
        // The generated playlist names local TS members; never infer playable
        // output from an unrelated segment or an out-of-folder reference.
        if !segment.starts_with('#')
            && !segment.contains(['/', '\\'])
            && !segment.contains("..")
            && matches!(
                FsPath::new(segment)
                    .extension()
                    .and_then(|ext| ext.to_str()),
                Some("ts")
            )
            && tokio::fs::try_exists(folder.join(segment)).await?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn wait_output(folder: &FsPath) -> ApiResult<()> {
    loop {
        if output_ready(folder).await? {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[cfg(test)]
#[path = "troubleshooting_next_tests.rs"]
mod tests;
