//! Finite transparent Next graphics streams (ADR 0105).

use std::time::Duration;

use crate::{
    canvas_overlay_filter, hide_banner, loglevel_error, no_standard_input, no_stats, pipe_output,
    time_limit, CommandGenerator, ComplexFilter, FfmpegCommand, FilterChain, FrameSize,
    GraphicsFrameRate, GraphicsOverlayFilter, GraphicsPipelineOverlay, InputFile, PipelineStep,
    WatermarkOpacityClock,
};

/// Render geometry and timing for one canvas chunk. Source inputs carry their
/// seek; filter evaluation uses the programme clock while output starts at zero.
pub struct CanvasRenderRequest {
    pub resolution: FrameSize,
    pub content_size: FrameSize,
    pub frame_rate: GraphicsFrameRate,
    pub duration: Duration,
    pub opacity_clock: WatermarkOpacityClock,
    pub overlays: Vec<GraphicsPipelineOverlay>,
}

#[derive(Debug, thiserror::Error)]
#[error("canvas requires positive dimensions, frame rate and duration")]
pub struct InvalidCanvasRenderRequest;

/// Build a finite BGRA FFV1/NUT stdout producer using shared graphics filters.
/// An empty overlay list deliberately emits transparent frames.
///
/// # Errors
/// Returns an error for zero dimensions, rate or duration.
pub fn build_canvas_command(
    request: CanvasRenderRequest,
) -> Result<FfmpegCommand, InvalidCanvasRenderRequest> {
    build_canvas_output(request, false)
}

/// Render a single PNG through the same native canvas and programme-clock filters.
///
/// # Errors
/// Returns invalid geometry/rate/duration errors.
pub fn build_canvas_preview_command(
    request: CanvasRenderRequest,
) -> Result<FfmpegCommand, InvalidCanvasRenderRequest> {
    build_canvas_output(request, true)
}

fn build_canvas_output(
    mut request: CanvasRenderRequest,
    preview: bool,
) -> Result<FfmpegCommand, InvalidCanvasRenderRequest> {
    if request.resolution.width == 0
        || request.resolution.height == 0
        || request.content_size.width == 0
        || request.content_size.height == 0
        || !request.frame_rate.parsed().is_finite()
        || request.frame_rate.parsed() <= 0.0
        || request.duration.is_zero()
    {
        return Err(InvalidCanvasRenderRequest);
    }
    let mut inputs = vec![InputFile::lavfi_video(format!(
        "color=c=black@0.0:s={}x{}:r={},format=bgra",
        request.resolution.width,
        request.resolution.height,
        request.frame_rate.r_frame_rate(),
    ))
    .with_input_options(["-readrate", "1"])];
    let mut chain = FilterChain {
        pixel_format: vec!["format=bgra".to_owned()],
        ..FilterChain::default()
    };
    request
        .overlays
        .sort_by_key(GraphicsPipelineOverlay::z_index);
    let mut image_index = 0;
    for overlay in request.overlays {
        match overlay {
            GraphicsPipelineOverlay::Drawvg { filter, .. } => {
                chain
                    .graphics_overlay
                    .push(GraphicsOverlayFilter::Drawvg(programme_filter(
                        filter,
                        &request.opacity_clock,
                    )));
            }
            GraphicsPipelineOverlay::Drawtext { filter, .. } => {
                chain
                    .graphics_overlay
                    .push(GraphicsOverlayFilter::Drawtext(programme_filter(
                        filter,
                        &request.opacity_clock,
                    )));
            }
            GraphicsPipelineOverlay::NativeImage {
                mut input,
                mut overlay,
                ..
            } => {
                input.input_instance = image_index + 1;
                prepare_image_filters(
                    &mut input,
                    &mut overlay,
                    request.resolution,
                    &request.opacity_clock,
                );
                inputs.push(input);
                chain
                    .graphics_overlay
                    .push(GraphicsOverlayFilter::NativeImage {
                        input_index: image_index,
                        filter: canvas_overlay_filter(
                            &overlay,
                            request.resolution,
                            request.content_size,
                        ),
                    });
                image_index += 1;
            }
        }
    }
    let complex = ComplexFilter::build(&inputs, chain, false);
    let steps = [
        no_standard_input(),
        hide_banner(),
        no_stats(),
        loglevel_error(),
        PipelineStep::complex_filter(complex.options),
        time_limit(request.duration),
        canvas_output_step(preview),
        pipe_output(),
    ];
    Ok(CommandGenerator::generate_command(&inputs, &steps, false))
}

fn canvas_output_step(preview: bool) -> PipelineStep {
    if preview {
        PipelineStep::generic(
            "canvas_preview",
            [
                "-an",
                "-frames:v",
                "1",
                "-c:v",
                "png",
                "-pix_fmt",
                "rgba",
                "-f",
                "image2pipe",
            ],
        )
    } else {
        PipelineStep::generic(
            "canvas_output",
            [
                "-an",
                "-c:v",
                "ffv1",
                "-level",
                "3",
                "-slices",
                "16",
                "-slicecrc",
                "0",
                "-pix_fmt",
                "bgra",
                "-f",
                "nut",
            ],
        )
    }
}

fn programme_filter(filter: String, clock: &WatermarkOpacityClock) -> String {
    let offset = clock.content_seconds_at_spawn;
    if offset <= 0.0 {
        return filter;
    }
    format!("settb=AVTB,setpts=PTS+{offset}/TB,{filter},setpts=PTS-{offset}/TB")
}

fn prepare_image_filters(
    input: &mut InputFile,
    overlay: &mut crate::WatermarkOverlay,
    resolution: FrameSize,
    clock: &WatermarkOpacityClock,
) {
    let fades = std::mem::take(&mut overlay.fade_points);
    crate::pipeline::prepare_graphics_image_filters(input, overlay, resolution, clock);
    if !fades.is_empty() {
        let filter = fades
            .into_iter()
            .map(crate::watermark_fade_filter)
            .collect::<Vec<_>>()
            .join(",");
        input.filter_steps.push(programme_filter(filter, clock));
    }
}
