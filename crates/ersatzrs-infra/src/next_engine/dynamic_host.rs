//! Resolve an authenticated Next item only after bounded, version-bound Burn preparation.

use chrono::{DateTime, Utc};
use ersatzrs_core::repositories::{RepositoryError, Result};
use tokio::time::{timeout_at, Instant};

use super::{dynamic_item, dynamic_projection, host::NextHost, protocol};

impl NextHost {
    /// Resolve a published item for the currently registered Next session.
    /// Authentication belongs to the internal transport boundary. No caller paths
    /// or executable names are accepted; all bindings are reconstructed from SQL.
    /// # Errors
    /// Returns a redacted failure for expired sessions, stale publication,
    /// unavailable media, or bounded subtitle preparation failure.
    #[allow(clippy::too_many_arguments)]
    pub async fn resolve_dynamic(
        &self,
        channel_id: i64,
        item_id: i64,
        digest: &str,
        audio: Option<u32>,
        number: &str,
        instance: uuid::Uuid,
        now: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> Result<protocol::PlayoutItem> {
        let deadline = Instant::now() + std::time::Duration::from_secs(30);
        timeout_at(
            deadline,
            Box::pin(self.resolve_dynamic_inner(
                channel_id, item_id, digest, audio, number, instance, now, until, deadline,
            )),
        )
        .await
        .map_err(|_| unavailable("resolution deadline expired"))?
    }

    #[allow(clippy::too_many_arguments)]
    async fn resolve_dynamic_inner(
        &self,
        channel_id: i64,
        item_id: i64,
        digest: &str,
        audio: Option<u32>,
        number: &str,
        instance: uuid::Uuid,
        now: DateTime<Utc>,
        until: DateTime<Utc>,
        deadline: Instant,
    ) -> Result<protocol::PlayoutItem> {
        let audio = audio
            .map(usize::try_from)
            .transpose()
            .map_err(|_| unavailable("audio index is invalid"))?;
        let request = self
            .dynamic_contexts
            .lock()
            .await
            .get(&instance)
            .map(|context| context.request.clone())
            .filter(|request| request.channel_id == channel_id && request.number == number)
            .ok_or_else(|| unavailable("session is unavailable"))?;
        let mut before = dynamic_projection::project_snapshot(
            &self.pool,
            &self.locker,
            &self.media,
            channel_id,
            item_id,
            now,
            until,
            &request.replacements,
            audio,
        )
        .await?;
        if before.identity.descriptor != digest {
            return Err(unavailable("publication changed"));
        }
        if super::live_subtitles::prepare(&self.burn_cache, &mut before, now, deadline).await? {
            let current = dynamic_projection::project_snapshot(
                &self.pool,
                &self.locker,
                &self.media,
                channel_id,
                item_id,
                now,
                until,
                &request.replacements,
                audio,
            )
            .await?;
            if current.identity.descriptor != digest
                || !self.dynamic_contexts.lock().await.contains_key(&instance)
            {
                return Err(unavailable(
                    "publication or session changed during caption preparation",
                ));
            }
        }
        if let Some(retained) = before.retained_remote.take() {
            return Box::pin(
                self.resolve_retained(before, retained, &request, instance, audio, now, deadline),
            )
            .await;
        }
        if let Some(producer) = before.producer_remote.take() {
            return Box::pin(
                self.resolve_producer(before, producer, &request, instance, audio, now, deadline),
            )
            .await;
        }
        Box::pin(self.resolve_concrete(
            before, &request, item_id, instance, audio, now, until, deadline,
        ))
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn resolve_concrete(
        &self,
        before: dynamic_projection::Snapshot,
        request: &super::host::NextStart,
        item_id: i64,
        instance: uuid::Uuid,
        audio: Option<usize>,
        now: DateTime<Utc>,
        until: DateTime<Utc>,
        deadline: Instant,
    ) -> Result<protocol::PlayoutItem> {
        let channel_id = request.channel_id;
        let digest = &before.identity.descriptor;
        if before.projection.pending_burn.is_none() {
            return self
                .resolve_ready_item(instance, &before.projection.item, now)
                .await;
        }
        let pending = before
            .projection
            .pending_burn
            .as_ref()
            .ok_or_else(|| unavailable("subtitle preparation changed"))?;
        let stream = u32::try_from(pending.subtitle.stream_index)
            .map_err(|_| unavailable("subtitle stream is invalid"))?;
        let prepared = self
            .burn_cache
            .prepare(
                &before.identity.cache_key,
                &pending.source,
                stream,
                pending.subtitle.codec.as_deref().unwrap_or("subrip"),
                &request.ffmpeg,
                before.projection.item.finish,
                // Reserve five seconds for managed-child cleanup plus one for
                // final identity validation before the outer handler timeout.
                deadline - std::time::Duration::from_secs(6),
                || async {
                    dynamic_projection::project_snapshot(
                        &self.pool,
                        &self.locker,
                        &self.media,
                        channel_id,
                        item_id,
                        now,
                        until,
                        &request.replacements,
                        audio,
                    )
                    .await
                    .is_ok_and(|current| {
                        &current.identity.descriptor == digest
                            && current.identity.cache_key == before.identity.cache_key
                    })
                },
            )
            .await
            .map_err(|_| unavailable("subtitle preparation failed"))?;
        // Metadata and membership may change while FFmpeg is running. Never
        // attach a completed artifact to a different selection or publication.
        let after = dynamic_projection::project_snapshot(
            &self.pool,
            &self.locker,
            &self.media,
            channel_id,
            item_id,
            now,
            until,
            &request.replacements,
            audio,
        )
        .await?;
        if &after.identity.descriptor != digest
            || after.identity.cache_key != before.identity.cache_key
            || !self.dynamic_contexts.lock().await.contains_key(&instance)
        {
            return Err(unavailable("publication changed during preparation"));
        }
        let item = install_prepared_track(after.projection.item, &prepared)?;
        dynamic_item::resolve_at(&item, now)
            .map_err(|_| unavailable("requested item timing is invalid"))
    }
    async fn resolve_ready_item(
        &self,
        instance: uuid::Uuid,
        item: &protocol::PlayoutItem,
        now: DateTime<Utc>,
    ) -> Result<protocol::PlayoutItem> {
        if !self.dynamic_contexts.lock().await.contains_key(&instance) {
            return Err(unavailable("session is unavailable"));
        }
        dynamic_item::resolve_at(item, now)
            .map_err(|_| unavailable("requested item timing is invalid"))
    }
}

pub(super) fn install_prepared_track(
    mut item: protocol::PlayoutItem,
    prepared: &std::path::Path,
) -> Result<protocol::PlayoutItem> {
    let track = item
        .tracks
        .as_mut()
        .and_then(|tracks| tracks.subtitle.as_mut())
        .ok_or_else(|| unavailable("subtitle selection changed"))?;
    let (in_point_ms, out_point_ms) = match track.source.as_ref() {
        Some(
            protocol::Source::Local {
                in_point_ms,
                out_point_ms,
                ..
            }
            | protocol::Source::Http {
                in_point_ms,
                out_point_ms,
                ..
            },
        ) => (*in_point_ms, *out_point_ms),
        _ => return Err(unavailable("subtitle source changed")),
    };
    track.source = Some(protocol::Source::Local {
        path: prepared
            .to_str()
            .ok_or_else(|| unavailable("subtitle cache path is invalid"))?
            .to_owned(),
        in_point_ms,
        out_point_ms,
    });
    track.stream_index = Some(0);
    Ok(item)
}

fn unavailable(reason: &str) -> RepositoryError {
    RepositoryError::unavailable("Next dynamic item", reason)
}
