//! ADR0136: authenticated caption changes leave the exact published AV binding intact.
use super::*;
use crate::next_engine::{dynamic_projection::Identity, protocol, DynamicItemRequest};
use ersatzrs_core::repositories::RepositoryError;

const PREFIX: &str = "caption.";

fn provider_binding(
    subtitle: &PlaybackSubtitle,
    provider: &crate::provider_subtitles::ProviderSubtitleBinding,
) -> Result<String, CaptionError> {
    let endpoint = provider
        .endpoint("http://caption.invalid")
        .map_err(|_| CaptionError::Binding)?;
    binding(
        subtitle,
        &format!(
            "{:?}:{}:{endpoint}",
            provider.kind, provider.media_source_id
        ),
    )
}

impl NextHost {
    pub(crate) async fn publish_provider_caption(
        &self,
        channel: i64,
        media: &schedule::ScheduledMedia,
        projection: &selection::DeferredProjection,
        identity: &Identity,
    ) -> ersatzrs_core::repositories::Result<Option<String>> {
        let Some(authority) = identity.av_descriptor.clone() else {
            return Ok(None);
        };
        let Some(pending) = projection.pending_burn.as_ref().filter(|pending| {
            pending.source_text.is_none()
                && pending.subtitle.id > 0
                && pending.subtitle.subtitle_kind == crate::metadata::subtitle_kind::SIDECAR
                && !pending.subtitle.is_image()
                && !matches!(
                    Format::for_codec(pending.subtitle.codec.as_deref()),
                    Format::Auto
                )
                && media
                    .item
                    .pinned_stream_selection
                    .is_none_or(|pin| pin.subtitle_stream_index.is_none())
        }) else {
            return Ok(None);
        };
        let Some(provider) =
            crate::provider_subtitles::binding(&self.pool, pending.subtitle.id).await?
        else {
            return Ok(None);
        };
        let mut owners: Vec<_> = self
            .dynamic_contexts
            .lock()
            .await
            .iter()
            .filter(|(_, context)| {
                context.request.channel_id == channel && context.troubleshooting.is_none()
            })
            .map(|(id, _)| *id)
            .collect();
        owners.sort_unstable();
        if owners.is_empty() {
            return Ok(None);
        }
        let audio = projection
            .item
            .tracks
            .as_ref()
            .and_then(|tracks| tracks.audio.as_ref())
            .filter(|track| !matches!(track.source, Some(Source::Lavfi { .. })))
            .and_then(|track| track.stream_index);
        let grant = Grant {
            version: 1,
            channel,
            item: media.item.id,
            media: media.item.media_item_id,
            media_version: media.media_version_id,
            file: media.media_file_id,
            subtitle: pending.subtitle.id,
            start: media.item.start,
            finish: media.item.finish,
            expires: media
                .item
                .finish
                .checked_add_signed(chrono::Duration::minutes(1))
                .ok_or_else(unavailable)?
                .timestamp(),
            binding: provider_binding(&pending.subtitle, &provider).map_err(|_| unavailable())?,
            // Retain the existing per-source/text cache key across occurrences.
            // The separately signed AV authority still binds this exact item.
            media_binding: identity.cache_key.clone(),
            format: Format::for_codec(pending.subtitle.codec.as_deref()),
            soft: true,
            owners,
            authority: Some(authority),
            audio,
            prepared: false,
            preview: false,
        };
        let uri = self
            .media
            .encode_caption(&grant)
            .map_err(|_| unavailable())?;
        let mut parts = uri.rsplit('/');
        let signature = parts.next().ok_or_else(unavailable)?;
        let encoded = parts.next().ok_or_else(unavailable)?;
        Ok(Some(format!("{PREFIX}{encoded}.{signature}")))
    }

    #[cfg(test)]
    pub(crate) async fn resolve_caption_publication(
        &self,
        callback: &DynamicItemRequest<'_>,
    ) -> ersatzrs_core::repositories::Result<Option<protocol::PlayoutItem>> {
        self.resolve_caption_publication_with_source_support(
            callback,
            crate::next_engine::SourceVideoCutSupport::Unavailable,
            tokio::time::Instant::now() + Duration::from_secs(30),
        )
        .await
    }

    pub(crate) async fn resolve_caption_publication_with_source_support(
        &self,
        callback: &DynamicItemRequest<'_>,
        video_cut: crate::next_engine::SourceVideoCutSupport,
        deadline: tokio::time::Instant,
    ) -> ersatzrs_core::repositories::Result<Option<protocol::PlayoutItem>> {
        let Some(token) = callback.digest.strip_prefix(PREFIX) else {
            return Ok(None);
        };
        let (encoded, signature) = token.split_once('.').ok_or_else(unavailable)?;
        // A plain local caption token cannot authorize a deferred item callback.
        let grant = self
            .media
            .decode_caption(encoded, signature)
            .map_err(|_| unavailable())?;
        if grant.authority.is_none()
            || !grant.soft
            || !grant.owners.contains(&callback.instance)
            || grant.channel != callback.channel_id
            || grant.item != callback.item_id
            || grant.audio != callback.audio
            || callback.now < grant.start
            || callback.now >= grant.finish
        {
            return Err(unavailable());
        }
        let request = self
            .dynamic_contexts
            .lock()
            .await
            .get(&callback.instance)
            .filter(|context| {
                context.request.channel_id == grant.channel
                    && context.request.number == callback.number
                    && context.troubleshooting.is_some() == grant.preview
            })
            .map(|context| context.request.clone())
            .ok_or_else(unavailable)?;
        let mut snapshot = Box::pin(self.caption_snapshot(&request, callback, &grant)).await?;
        if !grant.prepared {
            let provider = crate::next_engine::provider_source_host::ProviderRequest {
                request: &request,
                instance: callback.instance,
                item_id: callback.item_id,
                audio: callback
                    .audio
                    .map(usize::try_from)
                    .transpose()
                    .map_err(|_| unavailable())?,
                now: callback.now,
                until: callback.until,
                deadline,
                video_cut,
            };
            // Keep the existing complete-source qualifier and optional AV fallback.
            // A signed ordinary caption must never bypass a qualified source page.
            match Box::pin(self.resolve_provider_source(&snapshot, &provider)).await {
                Ok(Some(item)) => return Ok(Some(item)),
                Ok(None) => {}
                Err(error) => {
                    let current =
                        Box::pin(self.caption_snapshot(&request, callback, &grant)).await?;
                    if current.identity.descriptor == snapshot.identity.descriptor {
                        return Err(error);
                    }
                    // Only caption metadata changed during optional preparation.
                    // The independent signed AV/intent authority was revalidated.
                    snapshot = current;
                }
            }
        }
        snapshot
            .projection
            .item
            .tracks
            .as_mut()
            .ok_or_else(unavailable)?
            .subtitle = Some(
            self.media
                .provider_caption_track(&grant, &snapshot.projection.original_source)?,
        );
        self.resolve_ready_item(callback.instance, &snapshot.projection.item, callback.now)
            .await
            .map(Some)
    }

    async fn caption_snapshot(
        &self,
        request: &crate::next_engine::host::NextStart,
        callback: &DynamicItemRequest<'_>,
        grant: &Grant,
    ) -> ersatzrs_core::repositories::Result<crate::next_engine::dynamic_projection::Snapshot> {
        let mut snapshot = self
            .project_session_snapshot(
                request,
                callback.instance,
                grant.item,
                callback.now,
                callback.until,
                callback
                    .audio
                    .map(usize::try_from)
                    .transpose()
                    .map_err(|_| unavailable())?,
            )
            .await?;
        if snapshot.identity.av_descriptor != grant.authority
            || snapshot.prepared_remote.is_some()
            || snapshot.retained_remote.is_some()
            || snapshot.producer_remote.is_some()
            || snapshot.projection.item.start != grant.start
            || snapshot.projection.item.finish != grant.finish
        {
            return Err(unavailable());
        }
        snapshot
            .projection
            .publish_graphics(&self.media, Some(callback.instance), callback.now)?;
        Ok(snapshot)
    }

    pub(super) async fn read_provider_caption(
        &self,
        grant: &Grant,
    ) -> Result<CaptionDocument, CaptionError> {
        if !self.provider_caption_valid(grant).await? {
            return absent(grant);
        }
        let prepared = Box::pin(self.burn_cache.prepare_provider(
            &self.pool,
            (&grant.media_binding, grant.subtitle),
            grant.finish.max(Utc::now()),
            tokio::time::Instant::now() + Duration::from_secs(24),
            || async { self.provider_caption_valid(grant).await.unwrap_or(false) },
        ))
        .await;
        // Missing captions alone can become empty; changed AV/storage stays an error.
        if !self.provider_caption_valid(grant).await? {
            return absent(grant);
        }
        let path = prepared.map_err(|_| CaptionError::Source)?;
        let bytes = crate::subtitle_text::read_bounded(&path, 32 * 1024 * 1024)
            .await
            .map_err(|_| CaptionError::Source)?;
        let text = String::from_utf8(bytes).map_err(|_| CaptionError::Source)?;
        if text.contains('\0') {
            return Err(CaptionError::Source);
        }
        if !self.provider_caption_valid(grant).await? {
            return absent(grant);
        }
        Ok(CaptionDocument {
            text,
            content_type: grant.format.content_type(),
        })
    }

    async fn provider_caption_valid(&self, grant: &Grant) -> Result<bool, CaptionError> {
        if grant.authority.is_none() || !grant.soft || grant.prepared || grant.preview {
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
                    && context.troubleshooting.is_none()
            })
            .map(|(id, context)| (*id, context.request.clone()))
            .ok_or(CaptionError::Unauthorized)?;
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
        {
            return Err(CaptionError::Binding);
        }
        let valid = self.provider_caption_selection(grant).await?;
        if !self.dynamic_contexts.lock().await.contains_key(&instance) {
            return Err(CaptionError::Unauthorized);
        }
        Ok(valid)
    }

    async fn provider_caption_selection(&self, grant: &Grant) -> Result<bool, CaptionError> {
        let subtitles = crate::playback::load_subtitles_for_media_item(&self.pool, grant.media)
            .await
            .map_err(|_| CaptionError::Binding)?;
        let Some(subtitle) = subtitles.iter().find(|subtitle| {
            subtitle.id == grant.subtitle
                && subtitle.subtitle_kind == crate::metadata::subtitle_kind::SIDECAR
        }) else {
            return Ok(false);
        };
        let provider = match crate::provider_subtitles::binding(&self.pool, grant.subtitle).await {
            Ok(provider) => provider,
            Err(RepositoryError::Decode {
                kind: "provider subtitle",
                ..
            }) => return Ok(false),
            Err(_) => return Err(CaptionError::Binding),
        };
        let Some(provider) = provider else {
            return Ok(false);
        };
        Ok(provider_binding(subtitle, &provider).is_ok_and(|value| value == grant.binding))
    }
}

impl MediaRelay {
    fn provider_caption_track(
        &self,
        grant: &Grant,
        original: &Source,
    ) -> ersatzrs_core::repositories::Result<protocol::Track> {
        let (in_point_ms, out_point_ms) = match original {
            Source::Local {
                in_point_ms,
                out_point_ms,
                ..
            }
            | Source::Http {
                in_point_ms,
                out_point_ms,
                ..
            } => (*in_point_ms, *out_point_ms),
            _ => return Err(unavailable()),
        };
        let uri = self.encode_caption(grant).map_err(|_| unavailable())?;
        let probe_hint = match grant.format {
            Format::Srt => crate::next_engine::probe_hint::text_subtitle(Some("srt")),
            Format::Ass => crate::next_engine::probe_hint::text_subtitle(Some("ass")),
            Format::Webvtt => crate::next_engine::probe_hint::text_subtitle(Some("webvtt")),
            Format::Auto => return Err(unavailable()),
        };
        Ok(protocol::Track {
            stream_index: Some(0),
            source: Some(Source::Http {
                uri,
                probe_hint,
                is_live: Some(false),
                headers: Vec::new(),
                user_agent: None,
                keep_alive: Some(false),
                reconnect: Some(false),
                reconnect_delay_max: None,
                timeout_us: Some(35_000_000),
                in_point_ms,
                out_point_ms,
            }),
        })
    }
}
