//! Frozen preview media and one session authorize a local caption, never SQL IDs alone.
use super::{binding, media_binding, replace_path, CaptionError, Grant, NextHost};

impl NextHost {
    pub(super) async fn preview_caption_path(
        &self,
        grant: &Grant,
    ) -> Result<Option<String>, CaptionError> {
        if !grant.preview || grant.prepared || grant.authority.is_some() || grant.owners.len() != 1
        {
            return Err(CaptionError::Unauthorized);
        }
        let instance = grant.owners[0];
        let (request, session) = self
            .dynamic_contexts
            .lock()
            .await
            .get(&instance)
            .filter(|context| context.request.channel_id == grant.channel)
            .and_then(|context| {
                context
                    .troubleshooting
                    .clone()
                    .map(|session| (context.request.clone(), session))
            })
            .ok_or(CaptionError::Unauthorized)?;
        let context = session
            .occurrence(grant.item, grant.start)
            .map_err(|_| CaptionError::Binding)?;
        if context.channel.id != grant.channel
            || context.media.item.finish != grant.finish
            || media_binding(&context.media)? != grant.media_binding
        {
            return Err(CaptionError::Binding);
        }
        let current =
            crate::next_engine::schedule::bind_media(&self.pool, context.media.item.clone())
                .await
                .map_err(|_| CaptionError::Binding)?;
        if current.item.media_item_id != grant.media
            || current.media_version_id != grant.media_version
            || current.media_file_id != grant.file
            || media_binding(&current)? != grant.media_binding
        {
            return Err(CaptionError::Binding);
        }
        let subtitles = crate::playback::load_subtitles_for_media_item(&self.pool, grant.media)
            .await
            .map_err(|_| CaptionError::Binding)?;
        let main = replace_path(
            &request.replacements,
            &current.path,
            crate::next_engine::source::platform(),
        );
        let path = subtitles
            .iter()
            .find(|subtitle| subtitle.id == grant.subtitle)
            .and_then(|subtitle| {
                let input = crate::playback::subtitle_input_for_stream(
                    subtitle,
                    &main,
                    ersatzrs_ffmpeg::OutputFormat::MpegTs,
                    ersatzrs_domain::StreamingMode::HttpLiveStreamingSegmenter,
                )?;
                (!crate::playback::subtitle_is_http(&input.path)
                    && binding(subtitle, &input.path).is_ok_and(|value| value == grant.binding))
                .then_some(input.path)
            });
        if !self.dynamic_contexts.lock().await.contains_key(&instance) {
            return Err(CaptionError::Unauthorized);
        }
        Ok(path)
    }
}
