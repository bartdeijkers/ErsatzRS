//! Complete finite provider captions without changing the published selection.

use super::{
    dynamic_projection::Snapshot,
    host::{NextHost, NextStart},
    protocol,
    retained_source_origin::{RetainedOrigin, RetainedSourceIntent},
    session_source_files::SessionSourceFiles,
    session_transfers::TransferBinding,
};
use crate::{
    playback,
    remote_stream::RemoteStreamError,
    repositories::{make_repositories, SqliteConfigRepository},
};
use chrono::{DateTime, Utc};
use ersatzrs_core::{
    locking::TeletextWorkGuard,
    repositories::{ChannelRepository as _, ConfigRepository as _, RepositoryError, Result},
};
use std::sync::Arc;
use tokio::time::Instant;

pub(super) struct ProviderRequest<'a> {
    pub request: &'a NextStart,
    pub instance: uuid::Uuid,
    pub item_id: i64,
    pub audio: Option<usize>,
    pub now: DateTime<Utc>,
    pub until: DateTime<Utc>,
    pub deadline: Instant,
    pub video_cut: super::SourceVideoCutSupport,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Publication {
    descriptor: String,
    audio: Option<usize>,
    intent: serde_json::Value,
}

struct Context {
    channel: ersatzrs_domain::Channel,
    media: super::schedule::ScheduledMedia,
    resolution: ersatzrs_domain::Resolution,
}

impl NextHost {
    pub(super) async fn resolve_provider_source(
        &self,
        before: &Snapshot,
        request: &ProviderRequest<'_>,
    ) -> Result<Option<protocol::PlayoutItem>> {
        if !matches!(
            before.projection.original_source,
            protocol::Source::Http { .. }
        ) {
            return Ok(None);
        }
        // Transient previews own their source/profile without a persisted channel.
        // Provider retention is limited to ordinary sessions, as in provider_files.
        if self.preview_session(request.instance).await.is_some() {
            return Ok(None);
        }
        let context = self.provider_context(request).await?;
        let Some(intent) = RetainedSourceIntent::discover(
            &self.pool,
            &context.channel,
            &context.media,
            &before.projection.original_source,
        )
        .await
        else {
            return Ok(None);
        };
        let files = self.provider_files(request.instance).await?;
        let Ok((id, gate)) = self.provider_gate(before, request, &context, &intent).await else {
            self.validate_provider_publication(before, request).await?;
            tracing::warn!(
                channel_id = request.request.channel_id,
                item_id = request.item_id,
                "Finite provider source admission refused; ordinary AV remains available"
            );
            return Ok(None);
        };
        let Ok(work) = self.provider_work().await else {
            self.validate_provider_publication(before, request).await?;
            tracing::warn!(
                channel_id = request.request.channel_id,
                item_id = request.item_id,
                "Finite provider caption admission refused; ordinary AV remains available"
            );
            return Ok(None);
        };
        let acquired = super::provider_source_acquire::Acquisition {
            pool: self.pool.clone(),
            relay: self.media.clone(),
            files: files.clone(),
            store: self.source_storage.clone(),
            ffprobe: request
                .request
                .ffprobe
                .to_str()
                .ok_or_else(unavailable)?
                .to_owned(),
            binding: (
                context.media.item.media_item_id,
                context.media.media_version_id,
                context.media.media_file_id,
            ),
            expected: source_binding(&before.projection.original_source),
            intent,
            id,
            deadline: request.deadline - std::time::Duration::from_secs(6),
        }
        .run(gate, work)
        .await;
        let Ok(origin) = acquired else {
            self.validate_provider_publication(before, request).await?;
            tracing::warn!(
                channel_id = request.request.channel_id,
                item_id = request.item_id,
                "Finite provider caption preparation refused; ordinary AV remains available"
            );
            return Ok(None);
        };
        if let Ok(item) =
            Box::pin(self.finish_provider_source(before, request, &context, id, origin)).await
        {
            Ok(item)
        } else {
            self.validate_provider_publication(before, request).await?;
            tracing::warn!(
                channel_id = request.request.channel_id,
                item_id = request.item_id,
                "Finite provider caption qualification refused; ordinary AV remains available"
            );
            Ok(None)
        }
    }

    async fn provider_gate(
        &self,
        before: &Snapshot,
        request: &ProviderRequest<'_>,
        context: &Context,
        intent: &RetainedSourceIntent,
    ) -> Result<(uuid::Uuid, tokio::sync::OwnedMutexGuard<()>)> {
        let files = self.provider_files(request.instance).await?;
        let expires =
            u64::try_from((before.projection.item.finish - Utc::now()).num_milliseconds())
                .map_err(|_| unavailable())?;
        let publication = serde_json::to_string(&Publication {
            descriptor: before.identity.descriptor.clone(),
            audio: request.audio,
            intent: intent.identity(),
        })
        .map_err(|_| unavailable())?;
        let binding = TransferBinding {
            media_item_id: context.media.item.media_item_id,
            playout_item_id: context.media.item.id,
            media_version_id: context.media.media_version_id,
            media_file_id: context.media.media_file_id,
            publication,
            start: before.projection.item.start,
            finish: before.projection.item.finish,
        };
        let (id, gate) = files
            .gate(
                binding,
                Instant::now()
                    + std::time::Duration::from_millis(expires)
                    + std::time::Duration::from_secs(10),
            )
            .map_err(|_| unavailable())?;
        let gate = tokio::time::timeout_at(request.deadline, gate.lock_owned())
            .await
            .map_err(|_| unavailable())?;
        Ok((id, gate))
    }

    async fn provider_work(&self) -> Result<TeletextWorkGuard> {
        let configured = SqliteConfigRepository(self.pool.clone())
            .get("teletext.work.max_concurrent")
            .await?;
        let maximum = configured.map_or(Ok(1), |value| {
            value
                .parse::<std::num::NonZeroUsize>()
                .map(std::num::NonZeroUsize::get)
                .map_err(|_| unavailable())
        })?;
        TeletextWorkGuard::try_acquire(&self.locker, maximum).ok_or_else(unavailable)
    }

    async fn provider_context(&self, request: &ProviderRequest<'_>) -> Result<Context> {
        let channel = make_repositories(self.pool.clone())
            .channel
            .get_with_profile(request.request.channel_id)
            .await?;
        let schedule = super::schedule::read_locked(
            &self.pool,
            &self.locker,
            &channel,
            request.now,
            request.until,
        )
        .await?
        .ok_or_else(unavailable)?;
        let mut matches = schedule
            .items
            .iter()
            .filter(|media| media.item.id == request.item_id);
        let media = matches.next().ok_or_else(unavailable)?.clone();
        if matches.next().is_some() {
            return Err(unavailable());
        }
        drop(schedule);
        let profile = playback::load_channel_playback_profile(&self.pool, channel.id)
            .await?
            .ok_or_else(unavailable)?;
        Ok(Context {
            channel,
            media,
            resolution: profile.resolution,
        })
    }

    async fn provider_files(&self, instance: uuid::Uuid) -> Result<Arc<SessionSourceFiles>> {
        self.dynamic_contexts
            .lock()
            .await
            .get(&instance)
            .filter(|context| context.troubleshooting.is_none())
            .map(|context| context.source_files.clone())
            .ok_or_else(unavailable)
    }

    async fn finish_provider_source(
        &self,
        before: &Snapshot,
        request: &ProviderRequest<'_>,
        context: &Context,
        id: uuid::Uuid,
        origin: Arc<RetainedOrigin>,
    ) -> Result<Option<protocol::PlayoutItem>> {
        self.validate_provider_origin(before, request, &origin)
            .await?;
        let mut hint = super::probe_hint::producer(origin.media()).map_err(|_| unavailable())?;
        hint.duration_ms = u64::try_from(origin.media().version.duration.num_milliseconds()).ok();
        let source = self
            .media
            .retained_source(request.instance, id, Some(hint))?;
        let mut projected = super::selection::project_retained_scheduled_item(
            &self.pool,
            &context.channel,
            &context.media,
            source,
            &context.resolution,
            super::selection::AudioDirective::Published(request.audio),
            &origin,
        )
        .await?;
        // Keep the already authenticated graphics, picture and credits policy.
        projected.item.watermark = before.projection.item.watermark.clone();
        projected.item.graphics = before.projection.item.graphics.clone();
        let Some(pending) = projected.pending_burn.as_ref() else {
            return Ok(None);
        };
        let Some(text) = pending.source_text.as_deref() else {
            return Ok(None);
        };
        let key = super::dynamic_projection::digest(&serde_json::json!({
            "base": before.identity.cache_key, "source": text.identity(),
            "video": projected.selected_video().map(|track| track.stream_index),
        }))?;
        let valid = || async {
            self.validate_provider_publication(before, request)
                .await
                .is_ok()
        };
        let prepared = Box::pin(super::source_teletext::prepare(
            &self.pool,
            &self.locker,
            &self.burn_cache,
            super::source_teletext::Preparation {
                pending: text,
                source: &pending.source,
                video: projected.selected_video(),
                key: &key,
                expires: projected.item.finish.max(Utc::now()),
                ffmpeg: &request.request.ffmpeg,
                ffprobe: &request.request.ffprobe,
                deadline: request.deadline - std::time::Duration::from_secs(6),
                video_cut: request.video_cut,
            },
            valid,
        ))
        .await?;
        self.validate_provider_origin(before, request, &origin)
            .await?;
        let item = match prepared {
            super::source_teletext::PreparedText::Ready(caption) => {
                if let Some(cut) = &caption.video_cut {
                    cut.check_source().await?;
                    self.validate_provider_origin(before, request, &origin)
                        .await?;
                    cut.install(&mut projected.item)?;
                }
                super::dynamic_host::install_prepared_track(projected.item, &caption.path)?
            }
            super::source_teletext::PreparedText::Absent(absent) => {
                absent.check_source(request.deadline).await?;
                self.validate_provider_origin(before, request, &origin)
                    .await?;
                tracing::warn!(channel_id = request.request.channel_id, item_id = request.item_id,
                    page = absent.page(), "Complete provider source lacks selected teletext page; using ordinary selection");
                return Ok(None);
            }
        };
        self.provider_files(request.instance)
            .await?
            .describe(id)
            .map_err(|_| unavailable())?;
        super::dynamic_item::resolve_at(&item, request.now)
            .map(Some)
            .map_err(|_| unavailable())
    }

    async fn validate_provider_publication(
        &self,
        before: &Snapshot,
        request: &ProviderRequest<'_>,
    ) -> Result<()> {
        let current = self
            .project_session_snapshot(
                request.request,
                request.instance,
                request.item_id,
                request.now,
                request.until,
                request.audio,
            )
            .await?;
        if current.identity.descriptor != before.identity.descriptor
            || current.identity.cache_key != before.identity.cache_key
        {
            return Err(unavailable());
        }
        self.provider_files(request.instance).await?;
        Ok(())
    }

    async fn validate_provider_origin(
        &self,
        before: &Snapshot,
        request: &ProviderRequest<'_>,
        origin: &RetainedOrigin,
    ) -> Result<()> {
        self.validate_provider_publication(before, request).await?;
        let context = self.provider_context(request).await?;
        let current = RetainedSourceIntent::load(
            &self.pool,
            &context.channel,
            &context.media,
            &before.projection.original_source,
        )
        .await?
        .ok_or_else(unavailable)?;
        if current.identity() != origin.identity()["intent"] {
            return Err(unavailable());
        }
        Ok(())
    }

    async fn validate_source_grant(
        &self,
        instance: uuid::Uuid,
        binding: &TransferBinding,
    ) -> Result<()> {
        let publication: Publication =
            serde_json::from_str(&binding.publication).map_err(|_| unavailable())?;
        let request = self
            .dynamic_contexts
            .lock()
            .await
            .get(&instance)
            .map(|context| context.request.clone())
            .ok_or_else(unavailable)?;
        let context = self
            .provider_context(&ProviderRequest {
                request: &request,
                instance,
                item_id: binding.playout_item_id,
                audio: publication.audio,
                video_cut: super::SourceVideoCutSupport::Unavailable,
                now: binding.start,
                until: binding.finish,
                deadline: Instant::now() + std::time::Duration::from_secs(10),
            })
            .await?;
        let source = super::source::resolve_published(
            &self.pool,
            &context.media,
            &request.replacements,
            Some(&self.media),
        )
        .await?;
        let intent = Box::pin(RetainedSourceIntent::for_binding(
            &self.pool,
            &context.media,
            &source,
        ))
        .await?
        .ok_or_else(unavailable)?;
        if intent.identity() != publication.intent {
            return Err(unavailable());
        }
        Ok(())
    }

    /// Inspect an authenticated active occurrence without opening its bytes.
    /// # Errors
    /// Refuses expired sessions and changed source bindings, without revoking
    /// an immutable file grant for presentation-only channel changes.
    pub async fn retained_source_length(
        &self,
        instance: uuid::Uuid,
        id: uuid::Uuid,
    ) -> std::result::Result<u64, RemoteStreamError> {
        let files = self.provider_files(instance).await.map_err(|_| gone())?;
        let (length, binding) = files.describe(id)?;
        self.validate_source_grant(instance, &binding)
            .await
            .map_err(|_| gone())?;
        Ok(length)
    }

    /// Open an exact immutable range only while its original grant is current.
    /// # Errors
    /// Refuses stale sessions, publications, ranges or unavailable readers.
    pub async fn open_retained_source(
        &self,
        instance: uuid::Uuid,
        id: uuid::Uuid,
        offset: u64,
        length: u64,
    ) -> std::result::Result<impl tokio::io::AsyncRead + Unpin + Send + 'static, RemoteStreamError>
    {
        let files = self.provider_files(instance).await.map_err(|_| gone())?;
        let (_, binding) = files.describe(id)?;
        self.validate_source_grant(instance, &binding)
            .await
            .map_err(|_| gone())?;
        files.open(id, offset, length).await
    }
}

fn source_binding(source: &protocol::Source) -> Option<String> {
    let protocol::Source::Http { headers, .. } = source else {
        return None;
    };
    headers.iter().find_map(|header| {
        header.split_once(':').and_then(|(key, value)| {
            key.eq_ignore_ascii_case(super::media::SOURCE_BINDING_HEADER)
                .then(|| value.trim().to_owned())
        })
    })
}
fn unavailable() -> RepositoryError {
    RepositoryError::unavailable(
        "Next finite provider captions",
        "source or publication unavailable",
    )
}
fn gone() -> RemoteStreamError {
    RemoteStreamError::PreparedCancelled
}
