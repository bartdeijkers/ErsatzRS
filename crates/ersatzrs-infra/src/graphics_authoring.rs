//! Native editor validation and preview; never changes a channel or attachment.
use crate::{
    db::DbPool,
    dynamic_text::{DynamicTextRenderContext, DynamicTextTemplate},
    graphics_template::{graphics_template_variables, GraphicsMedia, GraphicsTemplateContext},
    playback::{self, PlaybackGraphicsRenderContext, ResolvedPlaybackGraphicsElement},
};
use chrono::Timelike as _;
use ersatzrs_core::repositories::{
    validate_authored_graphics, GraphicsRenderPreview, GraphicsRenderPreviewRequest,
    GraphicsTextPreviewRequest, RepositoryError, Result,
};
use ersatzrs_domain::{GraphicsElementConfig as Config, TextGraphicsRenderer};
use ersatzrs_ffmpeg::{
    build_canvas_preview_command, ffmpeg_path, CanvasRenderRequest, FrameSize, GraphicsFrameRate,
    WatermarkOpacityClock,
};
use std::{process::Stdio, time::Duration};
use tokio::{io::AsyncReadExt as _, process::Command};

fn invalid(code: &str) -> RepositoryError {
    RepositoryError::Validation {
        field: "graphics authoring",
        message: code.into(),
    }
}

async fn context(
    pool: &DbPool,
    request: &GraphicsRenderPreviewRequest,
) -> Result<GraphicsTemplateContext> {
    if !request.seek_seconds.is_finite() || !(0.0..=86400.0).contains(&request.seek_seconds) {
        return Err(invalid("graphics-authoring-invalid"));
    }
    let offset = chrono::Duration::from_std(Duration::from_secs_f64(request.seek_seconds))
        .map_err(|_| invalid("graphics-authoring-invalid"))?;
    if !(16..=8192).contains(&request.width) || !(16..=8192).contains(&request.height) {
        return Err(invalid("graphics-authoring-invalid"));
    }
    if let Some(channel_id) = request.channel_id {
        let mut context = crate::graphics_text_preview::load_context(
            pool,
            &GraphicsTextPreviewRequest {
                channel_id,
                element_id: None,
                render_time: request.render_time,
                template: String::new(),
                renderer: TextGraphicsRenderer::Drawvg,
                epg_entries: usize::try_from(request.element.epg_entries)
                    .map_err(|_| invalid("graphics-authoring-invalid"))?,
                width: request.width,
                height: request.height,
            },
        )
        .await?;
        context.render_time += offset;
        context.stream_seek_seconds += request.seek_seconds;
        if context
            .content_finish_time
            .is_some_and(|finish| context.render_time >= finish)
        {
            return Err(invalid("graphics-authoring-invalid"));
        }
        return Ok(context);
    }
    Ok(GraphicsTemplateContext {
        channel_id: 0,
        channel_number: String::new(),
        render_time: request.render_time + offset,
        playout_item_id: None,
        media: GraphicsMedia::Transient(serde_json::Map::default()),
        media_path: String::new(),
        channel_start_time: request.render_time,
        content_start_time: request.render_time,
        content_finish_time: Some(request.render_time + chrono::Duration::days(1)),
        stream_seek_seconds: request.seek_seconds,
        duration_seconds: 86400.0,
        frame_width: request.width,
        frame_height: request.height,
        scaled_frame_width: request.width,
        scaled_frame_height: request.height,
        frame_rate: Some(25.0),
        r_frame_rate: Some("25/1".into()),
    })
}

pub(crate) async fn validate(pool: &DbPool, request: &GraphicsRenderPreviewRequest) -> Result<()> {
    prepare(pool, request).await.map(|_| ())
}

async fn prepare(
    pool: &DbPool,
    request: &GraphicsRenderPreviewRequest,
) -> Result<(GraphicsTemplateContext, Option<String>, String)> {
    validate_authored_graphics(&request.element)?;
    let context = context(pool, request).await?;
    let saved = crate::graphics_text_preview::load_item_variables(
        pool,
        context.playout_item_id,
        Some(request.element.id),
    )
    .await?;
    let overrides = crate::graphics_template::combine_variable_overrides(
        saved.as_deref(),
        request.variables.as_deref(),
    )?;
    let variables = graphics_template_variables(
        pool,
        &context,
        usize::try_from(request.element.epg_entries)
            .map_err(|_| invalid("graphics-authoring-invalid"))?,
        overrides.as_deref(),
    )
    .await?;
    let element = playback::render_graphics_element_templates(&request.element, &variables)
        .map_err(|_| invalid("graphics-authoring-missing-variable"))?;
    match &element.config {
        Config::Image(config) => {
            require_file(config.image.as_deref(), "graphics-authoring-missing-media").await?;
        }
        Config::Motion(config) => {
            require_file(
                config.video_path.as_deref(),
                "graphics-authoring-missing-media",
            )
            .await?;
        }
        Config::Subtitle(config) => {
            require_file(
                config.template.as_deref(),
                "graphics-authoring-missing-media",
            )
            .await?;
            let path = config
                .template
                .as_deref()
                .ok_or_else(|| invalid("graphics-authoring-missing-media"))?;
            crate::graphics_template::render_subtitle_graphics_source(path, &variables).await?;
        }
        Config::Text(config) if config.renderer == TextGraphicsRenderer::Drawtext => {
            if let Some(path) = &config.drawtext.font_file {
                require_file(Some(path), "graphics-authoring-missing-font").await?;
            }
            DynamicTextTemplate::compile(config.text.clone().unwrap_or_default())
                .and_then(|template| {
                    template.render(
                        &variables,
                        DynamicTextRenderContext {
                            now: context.render_time,
                            playback_elapsed_seconds: context.stream_seek_seconds,
                            playback_duration_seconds: context.duration_seconds,
                        },
                    )
                })
                .map_err(|_| invalid("graphics-authoring-missing-variable"))?;
        }
        Config::Script(_) => {
            let size = FrameSize::new(request.width, request.height);
            let variables = crate::graphics_template::scalar_template_variables(&variables);
            tokio::task::spawn_blocking(move || {
                let directory =
                    tempfile::tempdir().map_err(|_| invalid("graphics-authoring-invalid"))?;
                ersatzrs_ffmpeg::GraphicsTemplateRenderer::new(directory.path())
                    .render_graphics_elements(
                        &[element],
                        &variables,
                        &ersatzrs_ffmpeg::GraphicsRenderContext::new(size, size),
                    )
                    .map_err(|_| invalid("graphics-authoring-invalid"))
            })
            .await
            .map_err(|_| invalid("graphics-authoring-invalid"))??;
        }
        Config::Text(_) => {}
    }
    let local_time = variables["Programme_LocalTime"]
        .as_str()
        .ok_or_else(|| invalid("graphics-authoring-invalid"))?
        .to_owned();
    Ok((context, overrides, local_time))
}

async fn require_file(value: Option<&str>, code: &str) -> Result<()> {
    let file = tokio::fs::File::open(value.ok_or_else(|| invalid(code))?)
        .await
        .map_err(|_| invalid(code))?;
    let metadata = file.metadata().await.map_err(|_| invalid(code))?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(invalid(code));
    }
    Ok(())
}

pub(crate) async fn render_preview(
    pool: &DbPool,
    request: &GraphicsRenderPreviewRequest,
) -> Result<GraphicsRenderPreview> {
    let (context, overrides, local_time) = prepare(pool, request).await?;
    let directory = tempfile::Builder::new()
        .prefix("ersatzrs-graphics-preview-")
        .tempdir()
        .map_err(|_| invalid("graphics-authoring-preview-failed"))?;
    let size = FrameSize::new(request.width, request.height);
    let render_context = PlaybackGraphicsRenderContext {
        channel_id: context.channel_id,
        channel_number: context.channel_number.clone(),
        render_time: context.render_time,
        playout_item_id: context.playout_item_id,
        media: context.media,
        media_path: context.media_path,
        channel_start_time: context.channel_start_time,
        content_start_time: context.content_start_time,
        content_finish_time: context.content_finish_time,
        stream_seek_seconds: context.stream_seek_seconds,
        duration_seconds: context.duration_seconds,
        frame_size: size,
        square_pixel_frame_size: size,
        frame_rate: Some(
            GraphicsFrameRate::parse("25/1").map_err(|_| invalid("graphics-authoring-invalid"))?,
        ),
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let (_cancel, receiver) = tokio::sync::oneshot::channel();
    let artifacts = playback::render_playback_canvas_overlays_cancellable(
        pool,
        &[ResolvedPlaybackGraphicsElement {
            element: request.element.clone(),
            variables: overrides,
        }],
        &render_context,
        directory.path(),
        deadline,
        receiver,
    )
    .await?;
    let result = async {
        let command = build_canvas_preview_command(CanvasRenderRequest {
            resolution: size,
            content_size: size,
            frame_rate: render_context
                .frame_rate
                .clone()
                .ok_or_else(|| invalid("graphics-authoring-invalid"))?,
            duration: Duration::from_millis(40),
            opacity_clock: WatermarkOpacityClock::new(
                f64::from(context.render_time.num_seconds_from_midnight())
                    + f64::from(context.render_time.nanosecond()) / 1e9,
                context.stream_seek_seconds,
                Some(context.duration_seconds),
                (context.render_time - context.channel_start_time).as_seconds_f64(),
            ),
            overlays: artifacts.overlays.clone(),
        })
        .map_err(|_| invalid("graphics-authoring-preview-failed"))?;
        let png = render_png(command, deadline).await?;
        Ok(GraphicsRenderPreview { png, local_time })
    }
    .await;
    artifacts.shutdown().await;
    result
}

async fn render_png(
    command: ersatzrs_ffmpeg::FfmpegCommand,
    deadline: tokio::time::Instant,
) -> Result<Vec<u8>> {
    let mut child = Command::new(ffmpeg_path())
        .args(command.arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| invalid("graphics-authoring-preview-failed"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| invalid("graphics-authoring-preview-failed"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| invalid("graphics-authoring-preview-failed"))?;
    let mut png = Vec::new();
    let mut errors = Vec::new();
    let mut stdout = stdout.take(64 * 1024 * 1024 + 1);
    let mut stderr = stderr.take(64 * 1024);
    let work = async {
        tokio::join!(
            stdout.read_to_end(&mut png),
            stderr.read_to_end(&mut errors),
            child.wait()
        )
    };
    let outcome = tokio::time::timeout_at(deadline, work).await;
    let Ok((Ok(_), Ok(_), Ok(status))) = outcome else {
        let _ = child.kill().await;
        let _ = child.wait().await;
        return Err(invalid("graphics-authoring-preview-failed"));
    };
    if !status.success() || png.len() > 64 * 1024 * 1024 || !png.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(invalid("graphics-authoring-preview-failed"));
    }
    Ok(png)
}
