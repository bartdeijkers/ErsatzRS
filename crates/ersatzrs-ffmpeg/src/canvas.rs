//! Finite transparent Next graphics streams (ADR 0105).

use std::time::Duration;

use crate::{
    canvas_overlay_filter, hide_banner, loglevel_error, no_standard_input, no_stats, pipe_output,
    time_limit, CommandGenerator, ComplexFilter, FfmpegCommand, FilterChain, FrameRate, FrameSize,
    GraphicsOverlayFilter, GraphicsPipelineOverlay, InputFile, PipelineStep, WatermarkOpacityClock,
};

/// Render geometry and timing for one canvas chunk. Overlay filters already
/// contain the programme seek and dynamic-template context; the canvas clock
/// itself starts at zero for each request.
pub struct CanvasRenderRequest {
    pub resolution: FrameSize,
    pub content_size: FrameSize,
    pub frame_rate: FrameRate,
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
    mut request: CanvasRenderRequest,
) -> Result<FfmpegCommand, InvalidCanvasRenderRequest> {
    if request.resolution.width == 0
        || request.resolution.height == 0
        || request.content_size.width == 0
        || request.content_size.height == 0
        || request.frame_rate.numerator == 0
        || request.frame_rate.denominator == 0
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
            GraphicsPipelineOverlay::Drawvg { filter, .. } => chain
                .graphics_overlay
                .push(GraphicsOverlayFilter::Drawvg(filter)),
            GraphicsPipelineOverlay::Drawtext { filter, .. } => chain
                .graphics_overlay
                .push(GraphicsOverlayFilter::Drawtext(filter)),
            GraphicsPipelineOverlay::NativeImage {
                mut input, overlay, ..
            } => {
                crate::pipeline::prepare_graphics_image_filters(
                    &mut input,
                    &overlay,
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
        ),
        pipe_output(),
    ];
    Ok(CommandGenerator::generate_command(&inputs, &steps, false))
}
