//! Finite transparent Next graphics streams (ADR 0105).

use std::time::Duration;

use crate::{
    canvas_overlay_filter, hide_banner, loglevel_error, no_standard_input, no_stats, pipe_output,
    time_limit, CanvasRegion, CommandGenerator, ComplexFilter, FfmpegCommand, FilterChain,
    FrameSize, GraphicsFrameRate, GraphicsOverlayFilter, GraphicsPipelineOverlay, InputFile,
    PipelineStep, WatermarkOpacityClock,
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
    build_canvas_output(request, false, None)
}

/// Render only a fixed rectangle while retaining full-output watermark geometry.
/// Mutable authored graphics keep their original coordinate system and are
/// clipped to the published rectangle instead of widening it.
///
/// # Errors
/// Returns an error for invalid timing, empty dimensions or coordinate overflow.
pub fn build_canvas_region_command(
    request: CanvasRenderRequest,
    region: CanvasRegion,
) -> Result<FfmpegCommand, InvalidCanvasRenderRequest> {
    region.validate()?;
    build_canvas_output(request, false, Some(region))
}

/// Render a single PNG through the same native canvas and programme-clock filters.
///
/// # Errors
/// Returns invalid geometry/rate/duration errors.
pub fn build_canvas_preview_command(
    request: CanvasRenderRequest,
) -> Result<FfmpegCommand, InvalidCanvasRenderRequest> {
    build_canvas_output(request, true, None)
}

fn build_canvas_output(
    mut request: CanvasRenderRequest,
    preview: bool,
    region: Option<CanvasRegion>,
) -> Result<FfmpegCommand, InvalidCanvasRenderRequest> {
    validate_request(&request)?;
    // Watermarks use the small base surface. Authored drawvg/drawtext scripts
    // can depend on frame dimensions, so a later selection change retains the
    // full coordinate system and crops only after those scripts have rendered.
    let small_surface = region.filter(|_| {
        request
            .overlays
            .iter()
            .all(|overlay| matches!(overlay, GraphicsPipelineOverlay::NativeImage { .. }))
    });
    let surface = small_surface.map_or(request.resolution, |region| {
        FrameSize::new(region.width, region.height)
    });
    let mut inputs = vec![InputFile::lavfi_video(format!(
        "color=c=black@0.0:s={}x{}:r={},format=bgra",
        surface.width,
        surface.height,
        request.frame_rate.r_frame_rate(),
    ))
    .with_input_options(["-readrate", "1"])];
    let mut chain = canvas_overlays(&mut request, &mut inputs, small_surface);
    if let Some(region) = region.filter(|_| small_surface.is_none()) {
        // A profile may shrink after publication. Keep the signed dimensions
        // even for newly selected authored graphics; missing frame area is
        // transparent, rather than an invalid crop that kills the chunk.
        let width = request.resolution.width.max(region.x + region.width);
        let height = request.resolution.height.max(region.y + region.height);
        if width != request.resolution.width || height != request.resolution.height {
            chain
                .pixel_format
                .push(format!("pad={width}:{height}:0:0:color=black@0"));
        }
        chain.pixel_format.push(format!(
            "crop={}:{}:{}:{}:exact=1",
            region.width, region.height, region.x, region.y,
        ));
    }
    let complex = ComplexFilter::build(&inputs, chain, false);
    let output_size = region.map_or(request.resolution, |region| {
        FrameSize::new(region.width, region.height)
    });
    let slices = if output_size.width < 4 || output_size.height < 4 {
        "1"
    } else {
        "16"
    };
    let steps = [
        no_standard_input(),
        hide_banner(),
        no_stats(),
        loglevel_error(),
        PipelineStep::complex_filter(complex.options),
        time_limit(request.duration),
        canvas_output_step(preview, slices),
        pipe_output(),
    ];
    Ok(CommandGenerator::generate_command(&inputs, &steps, false))
}

fn canvas_overlays(
    request: &mut CanvasRenderRequest,
    inputs: &mut Vec<InputFile>,
    small_surface: Option<CanvasRegion>,
) -> FilterChain {
    let mut chain = FilterChain {
        pixel_format: vec!["format=bgra".to_owned()],
        ..FilterChain::default()
    };
    request
        .overlays
        .sort_by_key(GraphicsPipelineOverlay::z_index);
    let mut image_index = 0;
    for overlay in std::mem::take(&mut request.overlays) {
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
                        filter: match small_surface {
                            Some(region) => crate::canvas_region_overlay_filter(
                                &overlay,
                                request.resolution,
                                request.content_size,
                                region,
                            ),
                            None => canvas_overlay_filter(
                                &overlay,
                                request.resolution,
                                request.content_size,
                            ),
                        },
                    });
                image_index += 1;
            }
        }
    }
    chain
}

fn validate_request(request: &CanvasRenderRequest) -> Result<(), InvalidCanvasRenderRequest> {
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
    Ok(())
}

fn canvas_output_step(preview: bool, slices: &str) -> PipelineStep {
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
                slices,
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
