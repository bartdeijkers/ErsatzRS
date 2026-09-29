//! Shared native rendering behind the session-owned Next canvas transfer.

use std::{collections::BTreeMap, time::Duration};

use chrono::Timelike as _;
use ersatzrs_domain::{MediaStream, MediaStreamKind, MediaVersion, ScalingBehavior};
use ersatzrs_ffmpeg::{
    build_canvas_command, CanvasRenderRequest, FrameSize, GraphicsPipelineOverlay, InputKind,
    WatermarkOpacityClock,
};
use tokio::{sync::oneshot, time::Instant};

use super::{
    canvas::CanvasRequest, canvas_host::CanvasSession, session_transfers::PreparedTransferMedia,
};
use crate::{
    db::DbPool,
    ffprobe::FfprobeMediaVersion,
    graphics_template::GraphicsMedia,
    playback::{self, PlaybackGraphicsRenderContext},
    remote_stream::{
        PreparedRemoteStreamInput, RemoteStreamCommand, RemoteStreamError, RemoteStreamSpawnContext,
    },
};

pub(super) async fn prepare(
    pool: DbPool,
    session: CanvasSession,
    timing: CanvasRequest,
    deadline: Instant,
    mut cancellation: oneshot::Receiver<()>,
) -> Result<PreparedTransferMedia, RemoteStreamError> {
    let context = tokio::select! {
        biased;
        _ = &mut cancellation => return Err(RemoteStreamError::PreparedCancelled),
        () = tokio::time::sleep_until(deadline) => return Err(RemoteStreamError::PreparedTimeout),
        context = render_context(&pool, &session, &timing) => context?,
    };
    let directory = tempfile::Builder::new()
        .prefix("ersatzrs-canvas-")
        .tempdir()
        .map_err(|_| RemoteStreamError::PreparedProducerFailed)?;
    let (cancel, receiver) = oneshot::channel();
    let (cancelled, watermarks) = stage(
        &mut cancellation,
        cancel,
        playback::watermark_inputs_for_stream_cancellable(
            session.watermarks.clone(),
            Some(timing.offset() + session.timeline.content_remaining),
            deadline,
            receiver,
        ),
    )
    .await;
    if cancelled {
        return Err(RemoteStreamError::PreparedCancelled);
    }
    let watermarks = watermarks.map_err(|_| RemoteStreamError::PreparedProducerFailed)?;
    let (cancel, receiver) = oneshot::channel();
    let (cancelled, artifacts) = stage(
        &mut cancellation,
        cancel,
        playback::render_playback_canvas_overlays_cancellable(
            &pool,
            &session.graphics,
            &context,
            directory.path(),
            deadline,
            receiver,
        ),
    )
    .await;
    if cancelled {
        if let Ok(artifacts) = artifacts {
            artifacts.shutdown().await;
        }
        return Err(RemoteStreamError::PreparedCancelled);
    }
    let mut artifacts = artifacts.map_err(|_| RemoteStreamError::PreparedProducerFailed)?;
    // Stable equal-z ordering matches upstream: watermarks precede graphics.
    let mut overlays: Vec<_> = watermarks
        .into_iter()
        .map(|(mut input, overlay)| {
            input.kind = InputKind::GraphicsImage;
            GraphicsPipelineOverlay::native_image(overlay.z_index, input, overlay)
        })
        .collect();
    overlays.append(&mut artifacts.overlays);
    let command = build_canvas_command(CanvasRenderRequest {
        resolution: context.frame_size,
        content_size: context.square_pixel_frame_size,
        frame_rate: timing.graphics_frame_rate(),
        duration: session.timeline.duration,
        opacity_clock: opacity_clock(&session, &timing),
        overlays,
    });
    let Ok(command) = command else {
        artifacts.shutdown().await;
        return Err(RemoteStreamError::PreparedProducerFailed);
    };
    let Some(program) = session.request.ffmpeg.to_str() else {
        artifacts.shutdown().await;
        return Err(RemoteStreamError::PreparedProducerFailed);
    };
    let command = RemoteStreamCommand {
        program: program.to_owned(),
        args: command.arguments,
        tool_paths: Vec::new(),
        working_directory: directory.path().to_owned(),
        environment: BTreeMap::new(),
    };
    let input = PreparedRemoteStreamInput::start_with_graphics(
        &command,
        RemoteStreamSpawnContext::hls(session.request.number.clone()),
        deadline,
        cancellation,
        directory,
        artifacts.dynamic_text_controllers,
    )
    .await?;
    Ok(PreparedTransferMedia {
        input,
        probe: output_metadata(&context, session.timeline.duration, &timing),
    })
}

async fn stage<T>(
    cancellation: &mut oneshot::Receiver<()>,
    cancel: oneshot::Sender<()>,
    work: impl std::future::Future<Output = T>,
) -> (bool, T) {
    tokio::pin!(work);
    tokio::select! {
        biased;
        _ = cancellation => {
            let _ = cancel.send(());
            (true, work.await)
        }
        result = &mut work => (false, result),
    }
}

async fn render_context(
    pool: &DbPool,
    session: &CanvasSession,
    timing: &CanvasRequest,
) -> Result<PlaybackGraphicsRenderContext, RemoteStreamError> {
    let resolution = &session.profile.resolution;
    let dimension = |value| {
        u32::try_from(value)
            .ok()
            .filter(|value| *value > 0)
            .ok_or(RemoteStreamError::PreparedProducerFailed)
    };
    let frame_size = FrameSize::new(dimension(resolution.width)?, dimension(resolution.height)?);
    let source = playback::load_source_video_stream(pool, session.media.media_version_id)
        .await
        .map_err(|_| RemoteStreamError::PreparedProducerFailed)?;
    let square_pixel_frame_size = match session.profile.profile.scaling_behavior {
        ScalingBehavior::Stretch | ScalingBehavior::Crop => frame_size,
        ScalingBehavior::ScaleAndPad => source
            .and_then(|source| source.square_pixel_frame_size(frame_size))
            .unwrap_or(frame_size),
    };
    Ok(PlaybackGraphicsRenderContext {
        channel_id: session.channel.id,
        channel_number: session.request.number.clone(),
        media: GraphicsMedia::Persisted(session.media.item.media_item_id),
        media_path: session.media.path.clone(),
        channel_start_time: session.started_at,
        content_start_time: session.timeline.content_start,
        content_finish_time: Some(session.timeline.content_finish),
        stream_seek_seconds: timing.offset().as_secs_f64(),
        duration_seconds: session.timeline.content_remaining.as_secs_f64(),
        frame_size,
        square_pixel_frame_size,
        frame_rate: Some(timing.graphics_frame_rate()),
    })
}

fn opacity_clock(session: &CanvasSession, timing: &CanvasRequest) -> WatermarkOpacityClock {
    let now = session.timeline.now;
    WatermarkOpacityClock::new(
        f64::from(now.num_seconds_from_midnight()) + f64::from(now.nanosecond()) / 1e9,
        timing.offset().as_secs_f64(),
        Some((timing.offset() + session.timeline.content_remaining).as_secs_f64()),
        (now - session.started_at)
            .to_std()
            .unwrap_or_default()
            .as_secs_f64(),
    )
}

// Known encoder output, not a consuming ffprobe operation on this one-shot stream.
fn output_metadata(
    context: &PlaybackGraphicsRenderContext,
    duration: Duration,
    timing: &CanvasRequest,
) -> FfprobeMediaVersion {
    FfprobeMediaVersion {
        version: MediaVersion {
            duration: chrono::TimeDelta::from_std(duration).unwrap_or_default(),
            width: i32::try_from(context.frame_size.width).unwrap_or_default(),
            height: i32::try_from(context.frame_size.height).unwrap_or_default(),
            r_frame_rate: Some(timing.frame_rate().to_owned()),
            sample_aspect_ratio: Some("1:1".into()),
            ..MediaVersion::default()
        },
        streams: vec![MediaStream {
            index: 0,
            kind: MediaStreamKind::Video,
            codec: Some("ffv1".into()),
            pixel_format: Some("bgra".into()),
            bits_per_raw_sample: 8,
            ..MediaStream::default()
        }],
        field_orders: BTreeMap::new(),
        chapters: Vec::new(),
        format_metadata: BTreeMap::new(),
        format_name: Some("nut".into()),
    }
}
