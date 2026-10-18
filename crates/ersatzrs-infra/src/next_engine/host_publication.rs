//! Full host projection shared by committed refresh and private SQL preparation.

use super::NextHost;
use crate::{
    next_engine::{
        coordinator::{
            AppliedChannelPlayout, ChannelPublisher, PreparedChannelPlayout, RefreshError,
            RefreshOutcome,
        },
        fallback::Gap,
        protocol,
        schedule::ScheduledMedia,
        selection, source,
    },
    playback::load_channel_playback_profile,
    repositories::PendingPlayoutBuild,
};
use chrono::{DateTime, Utc};
use ersatzrs_core::{path_replacement::PathReplacement, repositories::Result};
use ersatzrs_domain::Channel;
use std::{collections::BTreeMap, ops::Range, sync::Arc, time::Instant};
use tokio::sync::Mutex;

pub(super) struct ProjectedSchedule {
    pub items: Vec<protocol::PlayoutItem>,
    pub gaps: Vec<Gap>,
}

/// A full Next projection waiting for coordinated SQL/file publication.
/// Dropping it discards the staged file. This owner does not commit SQL.
#[must_use = "publish the prepared projection or drop it to discard it"]
pub struct PreparedHostPlayout {
    host: Arc<NextHost>,
    channel_id: i64,
    file: PreparedChannelPlayout,
    gaps: Vec<Gap>,
}

pub(crate) struct AppliedHostPlayout {
    file: AppliedChannelPlayout,
    host: Arc<NextHost>,
    channel_id: i64,
    gaps: Vec<Gap>,
}

impl AppliedHostPlayout {
    pub(crate) fn accept(&mut self) {
        self.file.accept();
    }

    pub(crate) fn restore(&mut self) -> std::result::Result<(), RefreshError> {
        self.file.restore()
    }

    pub(crate) async fn register(self) -> i64 {
        self.host
            .register_published_gaps(self.channel_id, &self.gaps)
            .await;
        self.channel_id
    }
}

impl PreparedHostPlayout {
    pub(crate) fn restrict_deadline(&mut self, deadline: std::time::Instant) {
        self.file.restrict_deadline(deadline);
    }

    pub(crate) const fn channel_id(&self) -> i64 {
        self.channel_id
    }

    pub(crate) fn publish_reversible(
        self,
    ) -> std::result::Result<AppliedHostPlayout, RefreshError> {
        Ok(AppliedHostPlayout {
            file: self.file.publish_reversible()?,
            host: self.host,
            channel_id: self.channel_id,
            gaps: self.gaps,
        })
    }

    /// Publish the staged file, then expose its fallback gaps to active sessions.
    ///
    /// # Errors
    /// Returns deadline, cancellation or file-publication failures. The caller
    /// remains responsible for coordinating and recovering SQL publication.
    pub async fn publish(self) -> std::result::Result<RefreshOutcome, RefreshError> {
        let outcome = self.file.publish().await?;
        self.host
            .register_published_gaps(self.channel_id, &self.gaps)
            .await;
        Ok(outcome)
    }
}

impl NextHost {
    /// Stage each active source/mirror publisher once, including preview owners.
    ///
    /// The caller holds the pending build's scheduler lock. New sessions cannot
    /// read that source until the caller releases it. Other-source contention
    /// returns None and discards every file already prepared by this call.
    ///
    /// # Errors
    /// Returns bounded selection/preparation errors, including conflicting path
    /// replacements among owners of one shared channel manifest.
    pub async fn prepare_active_build(
        self: &Arc<Self>,
        pending: &mut PendingPlayoutBuild,
        window: Range<DateTime<Utc>>,
        deadline: Instant,
    ) -> std::result::Result<Option<Vec<PreparedHostPlayout>>, RefreshError> {
        let deadline = pending
            .publication_deadline()
            .map_or(deadline, |limit| limit.min(deadline));
        tokio::time::timeout_at(deadline.into(), async {
            let channels = sqlx::query_scalar::<_, i64>(
                r#"SELECT "Id" FROM "Channel" WHERE "Id" = ?
                   OR ("PlayoutSource" = 1 AND "MirrorSourceChannelId" = ?)"#,
            )
            .bind(pending.channel_id())
            .bind(pending.channel_id())
            .fetch_all(&self.pool)
            .await
            .map_err(|_| RefreshError::Read)?;
            let mut owners = BTreeMap::<i64, Vec<PathReplacement>>::new();
            for context in self.dynamic_contexts.lock().await.values() {
                let request = &context.request;
                if !channels.contains(&request.channel_id) {
                    continue;
                }
                if let Some(existing) = owners.get(&request.channel_id) {
                    if existing != &request.replacements {
                        return Err(RefreshError::Projection);
                    }
                } else {
                    owners.insert(request.channel_id, request.replacements.clone());
                }
            }
            let mut prepared = Vec::with_capacity(owners.len());
            for (channel_id, replacements) in owners {
                let Some(file) = Box::pin(self.prepare_build(
                    pending,
                    channel_id,
                    window.clone(),
                    &replacements,
                    deadline,
                ))
                .await?
                else {
                    return Ok(None);
                };
                prepared.push(file);
            }
            Ok(Some(prepared))
        })
        .await
        .map_err(|_| crate::next_engine::publication::PublicationError::Expired)?
    }

    pub(super) async fn publisher(&self, channel_id: i64) -> ChannelPublisher {
        self.publishers
            .lock()
            .await
            .entry(channel_id)
            .or_insert_with(|| {
                ChannelPublisher::new(
                    self.pool.clone(),
                    self.locker.clone(),
                    &self.root,
                    channel_id,
                )
            })
            .clone()
    }

    /// Prepare the full channel projection from a pending SQL build.
    ///
    /// Retain the SQL owner and its scheduler lock until publication finishes.
    /// Main and preview sessions share a publisher: prepare each channel only
    /// once while its prepared owner is held. Other-source contention returns None.
    ///
    /// # Errors
    /// Returns projection, deadline or staging failures without committing SQL
    /// or replacing the channel's current file.
    pub async fn prepare_build(
        self: &Arc<Self>,
        pending: &mut PendingPlayoutBuild,
        channel_id: i64,
        window: Range<DateTime<Utc>>,
        replacements: &[PathReplacement],
        deadline: Instant,
    ) -> std::result::Result<Option<PreparedHostPlayout>, RefreshError> {
        let publisher = self.publisher(channel_id).await;
        let gaps = Mutex::new(Vec::new());
        let file = publisher
            .prepare_build(
                pending,
                window.start,
                window.end,
                deadline,
                |channel, media| async {
                    let projected = self
                        .project_schedule(channel, media, replacements, window.start)
                        .await?;
                    *gaps.lock().await = projected.gaps;
                    Ok(projected.items)
                },
            )
            .await?;
        Ok(file.map(|file| PreparedHostPlayout {
            host: self.clone(),
            channel_id,
            file,
            gaps: gaps.into_inner(),
        }))
    }

    pub(super) async fn project_schedule(
        &self,
        channel: Channel,
        media: Vec<ScheduledMedia>,
        replacements: &[PathReplacement],
        start: DateTime<Utc>,
    ) -> Result<ProjectedSchedule> {
        let channel_id = channel.id;
        let profile = load_channel_playback_profile(&self.pool, channel_id)
            .await?
            .ok_or_else(|| super::unavailable("profile unavailable"))?;
        let mut items = Vec::with_capacity(media.len());
        for media in media {
            if media.remote_stream_id.is_some() {
                let mut snapshot = super::super::remote_projection::project_publishable(
                    &self.pool,
                    &channel,
                    &media,
                    &profile.resolution,
                    selection::AudioDirective::Select,
                )
                .await?;
                snapshot
                    .projection
                    .publish_graphics(&self.media, None, media.item.start)?;
                items.push(self.defer_item(
                    channel_id,
                    media.item.id,
                    &snapshot.identity.descriptor,
                    snapshot.projection.item,
                )?);
                continue;
            }
            let source =
                source::resolve_published(&self.pool, &media, replacements, Some(&self.media))
                    .await?;
            let (watermarks, graphics) = if super::super::graphics::enabled(&profile) {
                media.overlays(&self.pool, &channel).await?
            } else {
                (Vec::new(), Vec::new())
            };
            let mut projection = Box::pin(selection::project_scheduled_item(
                &self.pool,
                &channel,
                &media,
                source,
                &profile.resolution,
                selection::AudioDirective::Select,
            ))
            .await?;
            super::super::credits::attach(
                &self.pool,
                &self.media,
                &channel,
                &media.item,
                &mut projection,
            )
            .await?;
            projection.canvas = super::super::graphics::project(
                &self.pool,
                &channel,
                &media,
                &profile,
                &mut projection.item,
                &watermarks,
                &graphics,
            )
            .await?;
            if projection.pending_burn.is_some() {
                let identity = super::super::dynamic_projection::fingerprint(
                    &self.pool,
                    &media,
                    &projection,
                    &channel,
                )
                .await?;
                projection.item = self.defer_item(
                    channel_id,
                    media.item.id,
                    &identity.descriptor,
                    projection.item,
                )?;
            }
            projection.publish_graphics(&self.media, None, media.item.start)?;
            items.push(projection.item);
        }
        let (items, gaps) =
            super::super::fallback::insert_gaps(&self.media, channel_id, start, items)?;
        Ok(ProjectedSchedule { items, gaps })
    }
}

#[cfg(test)]
#[path = "host_publication_tests.rs"]
pub(super) mod tests;
