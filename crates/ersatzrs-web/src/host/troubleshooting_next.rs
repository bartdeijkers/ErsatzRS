//! Upstream Prepare/StartTroubleshootingPlayback for a finite Next preview.
//! Preparation and process supervision keep the shared output ownership even
//! when the requesting browser disconnects. No preview playout is persisted.

use super::*;
use chrono::Duration as TimeDelta;
use ersatzrs_core::repositories::ChannelRepository as _;
use ersatzrs_domain::{PlayoutItem, StreamingEngine};
use ersatzrs_infra::next_engine::{
    config,
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
    mut query: PlaybackPreviewQuery,
    mut ownership: PreviewOwnership,
) -> Result<(), AdminTroubleshootingRuntimeError> {
    query.stream_selector = None;
    query.graphics_element.clear();
    query.watermark.truncate(1);
    let (sender, receiver) = oneshot::channel();
    tokio::spawn(async move {
        let mut ready = Some(sender);
        if Box::pin(run(&state, &query, &mut ownership, &mut ready))
            .await
            .is_err()
        {
            // Underlying provider/runtime errors may contain private inputs.
            tracing::warn!("Next troubleshooting preview failed");
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
        let at = parse_preview_start(query.start.as_deref()).unwrap_or_else(Utc::now);
        let mut media = state
            .next_engine
            .troubleshooting_channel_media(&channel, at)
            .await?
            .ok_or(PreviewNotFound("channel has no active playout item"))?;
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
        seek_media_item(&mut media.item, query.seek_seconds, info.duration_seconds)?;
        (
            preview_selector_channel(query, StreamingMode::HttpLiveStreamingSegmenter),
            media,
            None,
        )
    };
    channel.streaming_engine = StreamingEngine::Next;
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
    Ok(Selection {
        channel,
        media,
        info,
        profile,
        selected_at,
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
    item.finish = item.start + duration;
    item.in_point = TimeDelta::try_seconds(seek).ok_or(PreviewNotFound("invalid preview seek"))?;
    item.out_point = item
        .in_point
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
) -> ApiResult<(tempfile::TempDir, protocol::ChannelConfig, String)> {
    Box::pin(prepare_inner(state, query, paths, deadline, session)).await
}

async fn prepare_inner(
    state: &NativeAdminTroubleshootingRuntime,
    query: &PlaybackPreviewQuery,
    paths: &RuntimePaths,
    deadline: Instant,
    session: &mut Option<uuid::Uuid>,
) -> ApiResult<(tempfile::TempDir, protocol::ChannelConfig, String)> {
    let selected = select(state, query).await?;
    let runtime = state.ffmpeg_runtime.read().await.clone();
    let replacements = stream::load_path_replacements(&state.repositories.media_source).await?;
    let request = TroubleshootingPreview {
        media: &selected.media,
        channel: &selected.channel,
        profile: &selected.profile,
        subtitle_id: query.subtitle_id,
        selection_time: selected.selected_at,
        watermark_ids: &query.watermark,
        replacements: &replacements,
        ffmpeg: FsPath::new(&runtime.ffmpeg.path),
        deadline,
    };
    let item = if selected.media.remote_stream_id.is_some() {
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
        item
    } else {
        state.next_engine.prepare_troubleshooting(request).await?
    };
    let manifest = tempfile::tempdir()?;
    let working = &paths.output;
    let mut configuration = config::channel_config(
        &selected.profile,
        selected.channel.next_engine_text_subtitle_mode,
        manifest.path(),
        FsPath::new(&runtime.ffmpeg.path),
        FsPath::new(&runtime.ffprobe.path),
    )?;
    configuration.playout.virtual_start = Some(item.start);
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
    let playout = protocol::Playout {
        version: protocol::PLAYOUT_VERSION.to_owned(),
        items: vec![item],
    };
    tokio::fs::write(manifest.path().join(name), serde_json::to_vec(&playout)?).await?;
    prepare_troubleshooting_folder(working).await?;
    write_preview_artifacts(state, query, &selected.info, &selected.profile, working).await?;
    Ok((manifest, configuration, selected.channel.number))
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
    let (manifest, configuration, number) =
        timeout_at(deadline, prepare(state, query, paths, deadline, session))
            .await
            .map_err(|_| "Next troubleshooting preparation timed out")??;
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
    let result = supervise(&mut process, &paths.output, ownership, ready).await;
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
) -> ApiResult<()> {
    let result = timeout_at(Instant::now() + Duration::from_mins(2), async {
        tokio::select! {
            status = process.wait() => {
                let status = status?;
                if status.success() && output_ready(working).await? {
                    notify(ready, Ok(()));
                }
                Ok::<_, Box<dyn std::error::Error + Send + Sync>>(status)
            }
            result = wait_output(working) => {
                result?;
                notify(ready, Ok(()));
                Ok(process.wait().await?)
            }
        }
    })
    .await;
    let success = matches!(&result, Ok(Ok(status)) if status.success());
    if !success && process.stop().await.is_err() {
        **ownership = PreviewAvailability::CleanupUnconfirmed { _reservation: None };
    }
    let outcome = if success {
        "Next troubleshooting completed"
    } else {
        "Next troubleshooting failed or timed out"
    };
    tokio::fs::write(working.join("logs.txt"), outcome).await?;
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
    Ok(tokio::fs::try_exists(folder.join("ffmpeg.m3u8")).await?
        && count_segments(&folder.to_owned(), "ts").await? > 0)
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
