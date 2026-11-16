//! ADR0136: caption-only grace never changes programme/source authority.
use super::{MediaRelay, Source};
use crate::{
    next_engine::{host::NextHost, schedule, selection},
    playback::PlaybackSubtitle,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as BASE64, Engine as _};
use chrono::{DateTime, Utc};
use ersatzrs_core::{path_replacement::replace_path, repositories::ChannelRepository as _};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use tokio::time::{timeout, Duration};
use uuid::Uuid;

const DOMAIN: &[u8] = b"ersatzrs-next-caption-v1";
const MAX_GRANT: usize = 4096;

/// Opaque failures never carry caption text, filesystem paths or grant bytes.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CaptionError {
    #[error("caption grant is unavailable or invalid")]
    Unauthorized,
    #[error("caption binding is no longer available")]
    Binding,
    #[error("caption source could not be prepared")]
    Source,
    #[error("caption request exceeded its deadline")]
    Deadline,
}

/// Bounded UTF-8 content; deliberately no Debug implementation.
pub struct CaptionDocument {
    pub text: String,
    pub content_type: &'static str,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Format {
    Srt,
    Ass,
    Webvtt,
    Auto,
}

impl Format {
    fn for_codec(codec: Option<&str>) -> Self {
        match codec.unwrap_or_default().to_ascii_lowercase().as_str() {
            "srt" | "subrip" | "mov_text" => Self::Srt,
            "ass" | "ssa" => Self::Ass,
            "webvtt" | "vtt" => Self::Webvtt,
            _ => Self::Auto,
        }
    }
    fn content_type(self) -> &'static str {
        match self {
            Self::Srt => "application/x-subrip; charset=utf-8",
            Self::Ass => "text/x-ssa; charset=utf-8",
            Self::Webvtt => "text/vtt; charset=utf-8",
            Self::Auto => "text/plain; charset=utf-8",
        }
    }
    fn empty(self) -> CaptionDocument {
        let (text,content_type)=match self {
            Self::Srt=>("1\n00:00:00,000 --> 00:00:00,001\n \n\n",Self::Srt.content_type()),
            Self::Webvtt=>("WEBVTT\n\n",Self::Webvtt.content_type()),
            Self::Ass|Self::Auto=>("[Script Info]\nScriptType: v4.00+\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize\nStyle: Default,Arial,20\n\n[Events]\nFormat: Layer, Start, End, Style, Text\n",Self::Ass.content_type()),
        };
        CaptionDocument {
            text: text.into(),
            content_type,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Grant {
    version: u8,
    channel: i64,
    item: i64,
    media: i64,
    media_version: i64,
    file: i64,
    subtitle: i64,
    start: DateTime<Utc>,
    finish: DateTime<Utc>,
    expires: i64,
    binding: String,
    /// Local occurrence hash, or the existing provider artifact/cache binding.
    /// Provider occurrence/AV identity is independently bound by authority.
    media_binding: String,
    format: Format,
    soft: bool,
    owners: Vec<Uuid>,
    /// Some only for an automatic provider/prepared caption's deferred AV binding.
    authority: Option<String>,
    audio: Option<u32>,
    /// Caption belongs to an immutable published AV artifact, not a provider URL.
    #[serde(default)]
    prepared: bool,
    /// One private preview owns frozen media without persisted channel/item rows.
    #[serde(default)]
    preview: bool,
}

fn binding(subtitle: &PlaybackSubtitle, path: &str) -> Result<String, CaptionError> {
    let bytes = serde_json::to_vec(&(
        subtitle.id,
        subtitle.subtitle_kind,
        subtitle.stream_index,
        &subtitle.codec,
        &subtitle.path,
        &subtitle.language,
        subtitle.is_extracted,
        subtitle.default,
        subtitle.forced,
        subtitle.sdh,
        path,
    ))
    .map_err(|_| CaptionError::Binding)?;
    Ok(BASE64.encode(Sha256::digest(bytes)))
}

fn media_binding(media: &schedule::ScheduledMedia) -> Result<String, CaptionError> {
    let bytes = serde_json::to_vec(&(
        &media.item,
        media.media_version_id,
        media.media_file_id,
        &media.path,
    ))
    .map_err(|_| CaptionError::Binding)?;
    Ok(BASE64.encode(Sha256::digest(bytes)))
}

impl MediaRelay {
    fn caption_mac(&self, encoded: &str) -> Result<Hmac<Sha256>, CaptionError> {
        if self.caption_key.get().is_none() {
            let mut key = [0u8; 32];
            getrandom::fill(&mut key).map_err(|_| CaptionError::Unauthorized)?;
            // Concurrent first users all use the winning process-local key.
            let _ = self.caption_key.set(key);
        }
        let key = self.caption_key.get().ok_or(CaptionError::Unauthorized)?;
        let mut mac =
            Hmac::<Sha256>::new_from_slice(key).map_err(|_| CaptionError::Unauthorized)?;
        mac.update(DOMAIN);
        mac.update(encoded.as_bytes());
        Ok(mac)
    }
    fn encode_caption(&self, grant: &Grant) -> Result<String, CaptionError> {
        let address = self.address.get().ok_or(CaptionError::Binding)?;
        let encoded = BASE64.encode(serde_json::to_vec(grant).map_err(|_| CaptionError::Binding)?);
        if encoded.len() > MAX_GRANT {
            return Err(CaptionError::Binding);
        }
        let signature = BASE64.encode(self.caption_mac(&encoded)?.finalize().into_bytes());
        Ok(format!(
            "http://{address}/ffmpeg/next-caption/{encoded}/{signature}"
        ))
    }
    fn decode_caption(&self, encoded: &str, signature: &str) -> Result<Grant, CaptionError> {
        if encoded.len() > MAX_GRANT || signature.len() != 43 {
            return Err(CaptionError::Unauthorized);
        }
        let signature = BASE64
            .decode(signature)
            .map_err(|_| CaptionError::Unauthorized)?;
        self.caption_mac(encoded)?
            .verify_slice(&signature)
            .map_err(|_| CaptionError::Unauthorized)?;
        let grant: Grant = serde_json::from_slice(
            &BASE64
                .decode(encoded)
                .map_err(|_| CaptionError::Unauthorized)?,
        )
        .map_err(|_| CaptionError::Unauthorized)?;
        if grant.version != 1
            || (grant.prepared && grant.authority.is_none())
            || (grant.preview
                && (grant.owners.len() != 1 || (grant.authority.is_some() && !grant.prepared)))
            || [grant.channel, grant.item].iter().any(|id| {
                if grant.preview {
                    *id < 0
                } else {
                    *id <= 0
                }
            })
            || grant.authority.as_ref().is_some_and(|authority| {
                !grant.soft
                    || authority.len() != 64
                    || !authority.bytes().all(|byte| byte.is_ascii_hexdigit())
                    || grant.media_binding.len() != 64
                    || !grant
                        .media_binding
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit())
                    || matches!(grant.format, Format::Auto)
            })
            || grant.expires < Utc::now().timestamp()
            || grant.owners.is_empty()
            || grant.owners.len() > 64
            || grant.start >= grant.finish
            || [grant.media, grant.media_version, grant.file, grant.subtitle]
                .iter()
                .any(|id| *id <= 0)
        {
            return Err(CaptionError::Unauthorized);
        }
        Ok(grant)
    }
}

impl NextHost {
    async fn caption_owners(&self, channel: i64, owner: Option<Uuid>) -> Vec<Uuid> {
        if let Some(owner) = owner {
            return vec![owner];
        }
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
        owners
    }

    pub(crate) async fn publish_local_caption(
        &self,
        channel: &ersatzrs_domain::Channel,
        media: &schedule::ScheduledMedia,
        projection: &mut selection::DeferredProjection,
        owner: Option<Uuid>,
    ) -> ersatzrs_core::repositories::Result<()> {
        let Some(selected) = projection
            .selected_subtitle
            .as_ref()
            .filter(|s| s.id > 0 && s.subtitle_kind == crate::metadata::subtitle_kind::SIDECAR)
        else {
            return Ok(());
        };
        let Some(track) = projection
            .item
            .tracks
            .as_mut()
            .and_then(|tracks| tracks.subtitle.as_mut())
        else {
            return Ok(());
        };
        let Some(Source::Local {
            path,
            in_point_ms,
            out_point_ms,
            ..
        }) = track.source.as_ref()
        else {
            return Ok(());
        };
        let local:bool=sqlx::query_scalar(r#"SELECT EXISTS(SELECT 1 FROM "MediaItem" m JOIN "LibraryPath" p ON p."Id"=m."LibraryPathId" JOIN "LocalLibrary" l ON l."Id"=p."LibraryId" WHERE m."Id"=?)"#)
            .bind(media.item.media_item_id).fetch_one(&self.pool).await.map_err(|_|unavailable())?;
        if !local {
            return Ok(());
        }
        let owners = self.caption_owners(channel.id, owner).await;
        if owners.is_empty() {
            return Ok(());
        }
        let grant = Grant {
            version: 1,
            channel: channel.id,
            item: media.item.id,
            media: media.item.media_item_id,
            media_version: media.media_version_id,
            file: media.media_file_id,
            subtitle: selected.id,
            start: media.item.start,
            finish: media.item.finish,
            expires: media
                .item
                .finish
                .checked_add_signed(chrono::Duration::minutes(1))
                .ok_or_else(unavailable)?
                .timestamp(),
            binding: binding(selected, path).map_err(|_| unavailable())?,
            media_binding: media_binding(media).map_err(|_| unavailable())?,
            format: Format::for_codec(selected.codec.as_deref()),
            soft: channel.stream_selector_mode
                != ersatzrs_domain::ChannelStreamSelectorMode::Custom
                && media
                    .item
                    .pinned_stream_selection
                    .is_none_or(|pin| pin.subtitle_stream_index.is_none()),
            owners,
            authority: None,
            audio: None,
            prepared: false,
            preview: owner.is_some(),
        };
        let uri = self
            .media
            .encode_caption(&grant)
            .map_err(|_| unavailable())?;
        track.source = Some(Source::Http {
            uri,
            probe_hint: crate::next_engine::probe_hint::text_subtitle(selected.codec.as_deref()),
            is_live: Some(false),
            headers: Vec::new(),
            user_agent: None,
            keep_alive: Some(false),
            reconnect: Some(false),
            reconnect_delay_max: None,
            timeout_us: Some(5_000_000),
            in_point_ms: *in_point_ms,
            out_point_ms: *out_point_ms,
        });
        Ok(())
    }

    /// Serve only captions admitted by an active published occurrence.
    /// # Errors
    /// Refuses invalid grants, revoked owners/media, unsafe text and timeouts.
    pub async fn read_caption(
        &self,
        encoded: &str,
        signature: &str,
    ) -> Result<CaptionDocument, CaptionError> {
        let grant = self.media.decode_caption(encoded, signature)?;
        let deadline = Duration::from_secs(if grant.authority.is_some() && !grant.prepared {
            30
        } else {
            5
        });
        timeout(deadline, self.read_caption_inner(&grant))
            .await
            .map_err(|_| CaptionError::Deadline)?
    }

    async fn read_caption_inner(&self, grant: &Grant) -> Result<CaptionDocument, CaptionError> {
        if grant.authority.is_some() && !grant.prepared {
            return Box::pin(self.read_provider_caption(grant)).await;
        }
        let Some(path) = self.caption_path(grant).await? else {
            return absent(grant);
        };
        let text = match crate::subtitle_text::read_source(&path).await {
            Ok(text) => text.text,
            Err(_)
                if tokio::fs::metadata(&path)
                    .await
                    .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
            {
                self.caption_path(grant).await?;
                return absent(grant);
            }
            Err(_) => return Err(CaptionError::Source),
        };
        if self.caption_path(grant).await?.is_none() {
            return absent(grant);
        }
        Ok(CaptionDocument {
            text,
            content_type: grant.format.content_type(),
        })
    }

    async fn caption_path(&self, grant: &Grant) -> Result<Option<String>, CaptionError> {
        if grant.prepared {
            return Box::pin(self.prepared_caption_path(grant)).await;
        }
        if grant.preview {
            return self.preview_caption_path(grant).await;
        }
        let request = self
            .dynamic_contexts
            .lock()
            .await
            .iter()
            .find(|(id, context)| {
                grant.owners.contains(id) && context.request.channel_id == grant.channel
            })
            .map(|(_, context)| context.request.clone())
            .ok_or(CaptionError::Unauthorized)?;
        let channel = crate::repositories::make_repositories(self.pool.clone())
            .channel
            .get_with_profile(grant.channel)
            .await
            .map_err(|_| CaptionError::Binding)?;
        if channel.streaming_engine != ersatzrs_domain::StreamingEngine::Next {
            return Err(CaptionError::Binding);
        }
        let snapshot = schedule::read_locked(
            &self.pool,
            &self.locker,
            &channel,
            grant.start,
            grant.finish,
        )
        .await
        .map_err(|_| CaptionError::Binding)?
        .ok_or(CaptionError::Binding)?;
        let media = snapshot
            .items
            .iter()
            .find(|media| {
                media.item.id == grant.item
                    && media.item.media_item_id == grant.media
                    && media.media_version_id == grant.media_version
                    && media.media_file_id == grant.file
                    && media.item.start == grant.start
                    && media.item.finish == grant.finish
            })
            .ok_or(CaptionError::Binding)?;
        if media_binding(media)? != grant.media_binding {
            return Err(CaptionError::Binding);
        }
        let subtitles = crate::playback::load_subtitles_for_media_item(&self.pool, grant.media)
            .await
            .map_err(|_| CaptionError::Binding)?;
        let path = if let Some(subtitle) = subtitles
            .iter()
            .find(|subtitle| subtitle.id == grant.subtitle)
        {
            let main = replace_path(
                &request.replacements,
                &media.path,
                crate::next_engine::source::platform(),
            );
            let input = crate::playback::subtitle_input_for_stream(
                subtitle,
                &main,
                ersatzrs_ffmpeg::OutputFormat::MpegTs,
                ersatzrs_domain::StreamingMode::HttpLiveStreamingSegmenter,
            );
            input
                .filter(|input| {
                    !crate::playback::subtitle_is_http(&input.path)
                        && binding(subtitle, &input.path).is_ok_and(|value| value == grant.binding)
                })
                .map(|input| input.path)
        } else {
            None
        };
        drop(snapshot);
        if !self
            .dynamic_contexts
            .lock()
            .await
            .iter()
            .any(|(id, context)| {
                grant.owners.contains(id) && context.request.channel_id == grant.channel
            })
        {
            return Err(CaptionError::Unauthorized);
        }
        Ok(path)
    }
}

fn absent(grant: &Grant) -> Result<CaptionDocument, CaptionError> {
    if grant.soft {
        Ok(grant.format.empty())
    } else {
        Err(CaptionError::Source)
    }
}

fn unavailable() -> ersatzrs_core::repositories::RepositoryError {
    ersatzrs_core::repositories::RepositoryError::unavailable(
        "Next caption projection",
        "caption grant could not be prepared",
    )
}

#[cfg(test)]
#[path = "caption_source_tests.rs"]
mod tests;

#[path = "provider_caption_source.rs"]
mod provider;

#[path = "prepared_caption_source.rs"]
mod prepared;

#[path = "preview_caption_source.rs"]
mod preview;
