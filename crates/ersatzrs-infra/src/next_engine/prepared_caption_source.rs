//! Mutable caption delivery keeps immutable prepared AV publication checks intact.
use super::{
    binding, schedule, unavailable, CaptionError, Format, Grant, NextHost, PlaybackSubtitle, Source,
};
use crate::next_engine::dynamic_projection::Snapshot;

fn caption_binding(subtitle: &PlaybackSubtitle) -> Result<String, CaptionError> {
    binding(subtitle, subtitle.path.as_deref().unwrap_or_default())
}

impl NextHost {
    pub(crate) async fn publish_prepared_caption(
        &self,
        channel: &ersatzrs_domain::Channel,
        media: &schedule::ScheduledMedia,
        snapshot: &Snapshot,
        owner: Option<uuid::Uuid>,
    ) -> ersatzrs_core::repositories::Result<Option<String>> {
        if snapshot.prepared_remote.is_none()
            || channel.stream_selector_mode == ersatzrs_domain::ChannelStreamSelectorMode::Custom
            || media.item.pinned_stream_selection.is_some()
        {
            return Ok(None);
        }
        let Some(authority) = snapshot.identity.av_descriptor.clone() else {
            return Ok(None);
        };
        let Some(subtitle) = snapshot
            .projection
            .selected_subtitle
            .as_ref()
            .filter(|subtitle| {
                subtitle.id > 0
                    && subtitle.subtitle_kind == crate::metadata::subtitle_kind::SIDECAR
                    && !subtitle.is_image()
                    && !matches!(Format::for_codec(subtitle.codec.as_deref()), Format::Auto)
                    && subtitle.path.as_deref().is_some_and(|path| {
                        !path.trim().is_empty() && !crate::playback::subtitle_is_http(path)
                    })
            })
        else {
            return Ok(None);
        };
        let owners = self.caption_owners(channel.id, owner).await;
        if owners.is_empty() {
            return Ok(None);
        }
        let audio = snapshot
            .projection
            .item
            .tracks
            .as_ref()
            .and_then(|tracks| tracks.audio.as_ref())
            .filter(|track| !matches!(track.source, Some(Source::Lavfi { .. })))
            .and_then(|track| track.stream_index);
        let grant = Grant {
            version: 1,
            channel: channel.id,
            item: media.item.id,
            media: media.item.media_item_id,
            media_version: media.media_version_id,
            file: media.media_file_id,
            subtitle: subtitle.id,
            start: media.item.start,
            finish: media.item.finish,
            expires: media
                .item
                .finish
                .checked_add_signed(chrono::Duration::minutes(1))
                .ok_or_else(unavailable)?
                .timestamp(),
            binding: caption_binding(subtitle).map_err(|_| unavailable())?,
            media_binding: snapshot.identity.cache_key.clone(),
            format: Format::for_codec(subtitle.codec.as_deref()),
            soft: true,
            owners,
            authority: Some(authority),
            audio,
            prepared: true,
            preview: owner.is_some(),
        };
        let uri = self
            .media
            .encode_caption(&grant)
            .map_err(|_| unavailable())?;
        let mut parts = uri.rsplit('/');
        let signature = parts.next().ok_or_else(unavailable)?;
        let encoded = parts.next().ok_or_else(unavailable)?;
        Ok(Some(format!("caption.{encoded}.{signature}")))
    }

    pub(super) async fn prepared_caption_path(
        &self,
        grant: &Grant,
    ) -> Result<Option<String>, CaptionError> {
        if !grant.prepared || !grant.soft || grant.authority.is_none() {
            return Err(CaptionError::Unauthorized);
        }
        let (instance, request) = self
            .dynamic_contexts
            .lock()
            .await
            .iter()
            .find(|(id, context)| {
                grant.owners.contains(id)
                    && context.request.channel_id == grant.channel
                    && context.troubleshooting.is_some() == grant.preview
            })
            .map(|(id, context)| (*id, context.request.clone()))
            .ok_or(CaptionError::Unauthorized)?;
        // Re-resolve the frozen publication and confined prepared AV artifact.
        // Removing a caption never substitutes a provider URL or another file.
        let snapshot = self
            .project_session_snapshot(
                &request,
                instance,
                grant.item,
                grant.start,
                grant.finish,
                grant
                    .audio
                    .map(usize::try_from)
                    .transpose()
                    .map_err(|_| CaptionError::Binding)?,
            )
            .await
            .map_err(|_| CaptionError::Binding)?;
        if snapshot.identity.av_descriptor != grant.authority
            || snapshot.projection.item.start != grant.start
            || snapshot.projection.item.finish != grant.finish
            || snapshot.producer_remote.is_some()
            || snapshot.retained_remote.is_some()
        {
            return Err(CaptionError::Binding);
        }
        let Source::Local { path: main, .. } = &snapshot.projection.original_source else {
            return Err(CaptionError::Binding);
        };
        let subtitles = crate::playback::load_subtitles_for_media_item(&self.pool, grant.media)
            .await
            .map_err(|_| CaptionError::Binding)?;
        let path = subtitles
            .iter()
            .find(|subtitle| subtitle.id == grant.subtitle)
            .filter(|subtitle| caption_binding(subtitle).is_ok_and(|value| value == grant.binding))
            .and_then(|subtitle| {
                crate::playback::subtitle_input_for_stream(
                    subtitle,
                    main,
                    ersatzrs_ffmpeg::OutputFormat::MpegTs,
                    ersatzrs_domain::StreamingMode::HttpLiveStreamingSegmenter,
                )
            })
            .filter(|input| !crate::playback::subtitle_is_http(&input.path))
            .map(|input| input.path);
        if !self.dynamic_contexts.lock().await.contains_key(&instance) {
            return Err(CaptionError::Unauthorized);
        }
        Ok(path)
    }
}
