//! Stage a Next file from an unpublished SQL build without exposing either.

use super::{ChannelPublisher, PublicationAttempt, RefreshError, RefreshOutcome};
use crate::{
    next_engine::{protocol, publication, schedule},
    repositories::{make_repositories, PendingPlayoutBuild},
};
use chrono::{DateTime, Utc};
use ersatzrs_core::repositories::{ChannelRepository, RepositoryError};
use ersatzrs_domain::{Channel, StreamingEngine};
use std::{
    future::Future,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::sync::OwnedMutexGuard;

/// Prepared Next bytes plus exclusive publisher admission and source read locks.
///
/// Drop discards the file. The caller must also retain the pending SQL build and
/// its exclusive scheduler lock until it coordinates final publication. This
/// type does not make SQL commit and filesystem replacement atomic.
#[must_use = "publish the candidate or drop it to discard its temporary file"]
pub struct PreparedChannelPlayout {
    file: publication::PreparedPlayout,
    admission: OwnedMutexGuard<()>,
    schedule: schedule::LockedSchedule,
    attempt: PublicationAttempt,
    count: usize,
}

/// Finalizer-owned replacement. Its file restores before admission is released
/// on unwinding; successful SQL commit disarms that compensation explicitly.
pub(crate) struct AppliedChannelPlayout {
    file: publication::AppliedPlayout,
    _admission: OwnedMutexGuard<()>,
    _schedule: schedule::LockedSchedule,
    _attempt: PublicationAttempt,
}

impl AppliedChannelPlayout {
    pub(crate) fn accept(&mut self) {
        self.file.accept();
    }

    pub(crate) fn restore(&mut self) -> Result<(), RefreshError> {
        self.file.restore().map_err(Into::into)
    }
}

impl PreparedChannelPlayout {
    pub(crate) fn restrict_deadline(&mut self, deadline: std::time::Instant) {
        self.file.restrict_deadline(deadline);
    }

    /// Called only on the owned blocking finalizer, with its source build hold.
    pub(crate) fn publish_reversible(self) -> Result<AppliedChannelPlayout, RefreshError> {
        let Self {
            file,
            admission,
            schedule,
            attempt,
            ..
        } = self;
        let active = attempt.0.clone();
        let file = file.publish_reversible(|| active.load(Ordering::Acquire))?;
        Ok(AppliedChannelPlayout {
            file,
            _admission: admission,
            _schedule: schedule,
            _attempt: attempt,
        })
    }

    /// Replace the file while retaining ownership through blocking cleanup.
    ///
    /// # Errors
    /// Returns cancellation, deadline, replacement or worker failures. The
    /// previous file survives failures before replacement starts.
    pub async fn publish(self) -> Result<RefreshOutcome, RefreshError> {
        let Self {
            file,
            admission,
            schedule,
            attempt,
            count,
        } = self;
        let active = attempt.0.clone();
        tokio::task::spawn_blocking(move || {
            let _admission = admission;
            let _schedule = schedule;
            file.publish(|| active.load(Ordering::Acquire))
        })
        .await
        .map_err(|_| RefreshError::Worker)??;
        Ok(RefreshOutcome::Published { items: count })
    }
}

impl ChannelPublisher {
    /// Project and flush a candidate while SQL still retains its previous state.
    ///
    /// Keep the pending build and its exclusive scheduler lock alive until final
    /// publication. This prepares the complete target channel, including mirrors
    /// and other source playouts. `None` means another source is busy. Preparation
    /// retains this publisher's admission, so do not prepare the same publisher
    /// again until the returned owner is published or discarded.
    ///
    /// # Errors
    /// Returns redacted read, projection, deadline or filesystem failures without
    /// changing the published file or committing the supplied SQL build.
    pub async fn prepare_build<F, Fut>(
        &self,
        pending: &mut PendingPlayoutBuild,
        start: DateTime<Utc>,
        finish: DateTime<Utc>,
        deadline: std::time::Instant,
        project: F,
    ) -> Result<Option<PreparedChannelPlayout>, RefreshError>
    where
        F: FnOnce(Channel, Vec<schedule::ScheduledMedia>) -> Fut,
        Fut: Future<Output = ersatzrs_core::repositories::Result<Vec<protocol::PlayoutItem>>>,
    {
        let deadline = pending
            .publication_deadline()
            .map_or(deadline, |limit| limit.min(deadline));
        tokio::time::timeout_at(
            deadline.into(),
            self.prepare_before(pending, start, finish, deadline, project),
        )
        .await
        .map_err(|_| publication::PublicationError::Expired)?
    }

    async fn prepare_before<F, Fut>(
        &self,
        pending: &mut PendingPlayoutBuild,
        start: DateTime<Utc>,
        finish: DateTime<Utc>,
        deadline: std::time::Instant,
        project: F,
    ) -> Result<Option<PreparedChannelPlayout>, RefreshError>
    where
        F: FnOnce(Channel, Vec<schedule::ScheduledMedia>) -> Fut,
        Fut: Future<Output = ersatzrs_core::repositories::Result<Vec<protocol::PlayoutItem>>>,
    {
        if std::time::Instant::now() >= deadline {
            return Err(publication::PublicationError::Expired.into());
        }
        let attempt = PublicationAttempt(Arc::new(AtomicBool::new(true)));
        let admission = self.gate.clone().lock_owned().await;
        let channel = make_repositories(self.pool.clone())
            .channel
            .get_with_profile(self.channel_id)
            .await
            .map_err(|error| match error {
                RepositoryError::NotFound { .. } => RefreshError::Retired,
                _ => RefreshError::Read,
            })?;
        if channel.streaming_engine != StreamingEngine::Next {
            return Err(RefreshError::Retired);
        }
        let Some(mut locked) =
            schedule::read_pending(&self.pool, &self.locker, &channel, start, finish, pending)
                .await
                .map_err(|_| RefreshError::Read)?
        else {
            return Ok(None);
        };
        let items = project(channel, std::mem::take(&mut locked.items))
            .await
            .map_err(|_| RefreshError::Projection)?;
        let count = items.len();
        let folder = self.folder.clone();
        let active = attempt.0.clone();
        let (file, admission, schedule) = tokio::task::spawn_blocking(move || {
            let file = publication::prepare_playout_checked(
                &folder,
                &protocol::Playout {
                    version: protocol::PLAYOUT_VERSION.to_owned(),
                    items,
                },
                || active.load(Ordering::Acquire),
                deadline,
            )?;
            Ok::<_, publication::PublicationError>((file, admission, locked))
        })
        .await
        .map_err(|_| RefreshError::Worker)??;
        Ok(Some(PreparedChannelPlayout {
            file,
            admission,
            schedule,
            attempt,
            count,
        }))
    }
}
