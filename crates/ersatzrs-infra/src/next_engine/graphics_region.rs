//! Upstream `CanvasRegionCalculator.ForWatermarks`, using the existing bounded probe.

use std::time::Duration;

use ersatzrs_domain::ChannelWatermarkMode;
use ersatzrs_ffmpeg::{CanvasRegion, FrameSize, InputFile};
use tokio::{sync::oneshot, time::Instant};

use crate::{
    ffprobe::FfprobeExtractor,
    playback::{self, PlaybackWatermarkImage, ResolvedPlaybackWatermark},
};

pub(super) async fn measure(
    watermarks: &[ResolvedPlaybackWatermark],
    frame: FrameSize,
    content: Option<FrameSize>,
) -> Option<CanvasRegion> {
    let content = content?;
    let deadline = Instant::now() + Duration::from_secs(2);
    let extractor = FfprobeExtractor::new(ersatzrs_ffmpeg::ffprobe_path());
    let mut images = Vec::new();
    for watermark in watermarks
        .iter()
        .filter(|w| w.watermark.mode != ChannelWatermarkMode::None)
    {
        // URL images retain the full frame; do not add network work to publication.
        let PlaybackWatermarkImage::Local(path) = &watermark.image else {
            return None;
        };
        let path = path.to_str()?;
        let (_cancel, receiver) = oneshot::channel();
        let probe = extractor
            .extract_input_cancellable(
                &InputFile::watermark(path)
                    .with_input_options(["-protocol_whitelist", "file,pipe"]),
                deadline.saturating_duration_since(Instant::now()),
                receiver,
            )
            .await
            .ok()?;
        let image = FrameSize::new(
            u32::try_from(probe.version.width).ok()?,
            u32::try_from(probe.version.height).ok()?,
        );
        // Timing is irrelevant to the maximum spatial rectangle. The renderer
        // still receives the original bounded programme-clock enable/fades.
        let overlay = playback::watermark_overlay_for_stream(
            &watermark.watermark,
            Some(Duration::from_secs(1)),
        )?;
        images.push((overlay, image));
    }
    CanvasRegion::for_watermarks(frame, content, &images)
}
