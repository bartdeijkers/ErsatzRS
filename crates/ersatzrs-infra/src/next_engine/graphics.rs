//! Shared image templates and ordered Next layers (upstream `SelectGraphics`).

use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::{
    Channel, ChannelWatermarkMode, GraphicsElement, GraphicsElementConfig, Resolution,
};
use ersatzrs_ffmpeg::FrameSize;

use crate::{
    db::DbPool,
    graphics_template::{graphics_template_variables, GraphicsTemplateContext},
    playback::{
        self, PlaybackWatermarkImage, ResolvedPlaybackGraphicsElement, ResolvedPlaybackWatermark,
    },
};

use super::{protocol, schedule::ScheduledMedia, watermark};

pub(super) async fn project(
    pool: &DbPool,
    channel: &Channel,
    media: &ScheduledMedia,
    resolution: &Resolution,
    projected: &protocol::PlayoutItem,
    watermarks: &[ResolvedPlaybackWatermark],
    graphics: &[ResolvedPlaybackGraphicsElement],
) -> Result<Vec<protocol::Watermark>> {
    if graphics.is_empty() {
        return watermark::project(watermarks);
    }
    let frame = FrameSize::new(
        u32::try_from(resolution.width).map_err(|_| unavailable())?,
        u32::try_from(resolution.height).map_err(|_| unavailable())?,
    );
    let index = projected
        .tracks
        .as_ref()
        .and_then(|tracks| tracks.video.as_ref())
        .and_then(|video| video.stream_index)
        .map(i32::try_from)
        .transpose()
        .map_err(|_| unavailable())?;
    let video =
        playback::load_source_video_stream_by_index(pool, media.media_version_id, index).await?;
    let square = video
        .as_ref()
        .and_then(|video| video.square_pixel_frame_size(frame))
        .unwrap_or(frame);
    let rate = video.as_ref().and_then(|video| video.frame_rate);
    let context = GraphicsTemplateContext {
        channel_id: channel.id,
        channel_number: channel.number.clone(),
        media: crate::graphics_template::GraphicsMedia::Persisted(media.item.media_item_id),
        media_path: media.path.clone(),
        channel_start_time: media.item.start,
        content_start_time: media.item.start,
        content_finish_time: Some(media.item.finish),
        stream_seek_seconds: 0.0,
        duration_seconds: (media.item.out_point - media.item.in_point)
            .to_std()
            .map_err(|_| unavailable())?
            .as_secs_f64(),
        frame_width: frame.width,
        frame_height: frame.height,
        scaled_frame_width: square.width,
        scaled_frame_height: square.height,
        r_frame_rate: rate.map(ersatzrs_ffmpeg::FrameRate::r_frame_rate),
        frame_rate: rate.map(ersatzrs_ffmpeg::FrameRate::parsed),
    };
    let mut layers = Vec::with_capacity(watermarks.len() + graphics.len());
    for selected in watermarks
        .iter()
        .filter(|selected| selected.watermark.mode != ChannelWatermarkMode::None)
    {
        layers.push((
            selected.watermark.z_index,
            watermark::project_layer(selected)?,
        ));
    }
    for selected in graphics {
        let variables = graphics_template_variables(
            pool,
            &context,
            usize::try_from(selected.element.epg_entries.max(0)).unwrap_or_default(),
            selected.variables.as_deref(),
        )
        .await?;
        let element = playback::render_graphics_element_templates(&selected.element, &variables)?;
        layers.push((element.z_index, image_layer(&element)?));
    }
    // Stable ties retain watermarks before graphics, then selector order.
    layers.sort_by_key(|(z_index, _)| *z_index);
    Ok(layers.into_iter().map(|(_, layer)| layer).collect())
}

fn image_layer(element: &GraphicsElement) -> Result<protocol::Watermark> {
    let GraphicsElementConfig::Image(config) = &element.config else {
        return Err(unavailable());
    };
    if element.kind != element.config.kind()
        || element
            .opacity_expression
            .as_deref()
            .is_some_and(|expression| !expression.trim().is_empty())
        || element.location_x.is_some()
        || element.location_y.is_some()
    {
        return Err(unavailable());
    }
    let path = config
        .image
        .as_deref()
        .filter(|path| !path.trim().is_empty())
        .ok_or_else(unavailable)?;
    let image = if url::Url::parse(path).is_ok_and(|url| matches!(url.scheme(), "http" | "https")) {
        PlaybackWatermarkImage::Http(path.to_owned())
    } else {
        PlaybackWatermarkImage::Local(path.into())
    };
    Ok(protocol::Watermark {
        kind: None,
        source: watermark::image_source(&image)?,
        location: watermark::location(element.location),
        stream_index: Some(0),
        timing: None,
        within_source_content: element.place_within_source_content.then_some(true),
        width_percent: element
            .scale
            .then(|| watermark::percent(element.scale_width_percent.unwrap_or(100.0)))
            .transpose()?,
        horizontal_margin_percent: element
            .horizontal_margin_percent
            .map(watermark::percent)
            .transpose()?,
        vertical_margin_percent: element
            .vertical_margin_percent
            .map(watermark::percent)
            .transpose()?,
        opacity_percent: element
            .opacity_percent
            .map(f64::from)
            .map(watermark::percent)
            .transpose()?,
    })
}

fn unavailable() -> RepositoryError {
    RepositoryError::unavailable(
        "Next graphics projection",
        "selection is unsupported or invalid",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_domain::ImageGraphicsConfig;

    #[test]
    fn image_sources_and_geometry_preserve_configured_values() {
        let mut element = GraphicsElement {
            config: GraphicsElementConfig::Image(ImageGraphicsConfig {
                image: Some("http-prefix.png".into()),
            }),
            scale: true,
            place_within_source_content: true,
            horizontal_margin_percent: Some(12.5),
            vertical_margin_percent: Some(7.5),
            opacity_percent: Some(60),
            ..GraphicsElement::default()
        };
        let layer = image_layer(&element).unwrap();
        assert!(
            matches!(layer.source, protocol::Source::Local { ref path, .. } if path == "http-prefix.png")
        );
        assert_eq!(layer.width_percent, Some(100.0));
        assert_eq!(layer.horizontal_margin_percent, Some(12.5));
        assert_eq!(layer.vertical_margin_percent, Some(7.5));
        assert_eq!(layer.opacity_percent, Some(60.0));
        assert_eq!(layer.within_source_content, Some(true));
        element.config = GraphicsElementConfig::Image(ImageGraphicsConfig {
            image: Some("https://example.test/image.png".into()),
        });
        element.scale = false;
        let layer = image_layer(&element).unwrap();
        assert!(matches!(layer.source, protocol::Source::Http { .. }));
        assert_eq!(layer.width_percent, None);
    }

    #[test]
    fn unsupported_or_invalid_image_settings_fail_explicitly() {
        let image = GraphicsElement {
            config: GraphicsElementConfig::Image(ImageGraphicsConfig {
                image: Some("image.png".into()),
            }),
            ..GraphicsElement::default()
        };
        let mut invalid = image.clone();
        invalid.opacity_expression = Some("t * 10".into());
        assert!(image_layer(&invalid).is_err());
        invalid = image.clone();
        invalid.location_x = Some(100.0);
        assert!(image_layer(&invalid).is_err());
        invalid = image.clone();
        invalid.horizontal_margin_percent = Some(f64::NAN);
        assert!(image_layer(&invalid).is_err());
        invalid = image;
        invalid.config =
            GraphicsElementConfig::Text(ersatzrs_domain::TextGraphicsConfig::default());
        assert!(image_layer(&invalid).is_err());
    }
}
