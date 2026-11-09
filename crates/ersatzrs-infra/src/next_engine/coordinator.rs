//! Serialize a channel's database read, projection and atomic publication.

use std::{
    future::Future,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use chrono::{DateTime, Utc};
use ersatzrs_core::{
    locking::EntityLocker,
    repositories::{ChannelRepository, RepositoryError},
};
use ersatzrs_domain::{Channel, StreamingEngine};
use tokio::sync::Mutex;

use crate::{db::DbPool, repositories::make_repositories};

use super::{protocol, publication, schedule};

#[path = "candidate_publication.rs"]
mod candidate_publication;
pub(crate) use candidate_publication::AppliedChannelPlayout;
pub use candidate_publication::PreparedChannelPlayout;

/// One publisher per channel, owned by the host and shared with refresh workers.
/// Clone this owner for concurrent callers; do not construct competing owners
/// for the same folder. A channel-number change must retain this owner because
/// its folder and identity use the stable database ID.
#[derive(Clone)]
pub struct ChannelPublisher {
    pool: DbPool,
    locker: EntityLocker,
    channel_id: i64,
    folder: PathBuf,
    gate: Arc<Mutex<()>>,
    layout: publication::PlayoutLayout,
    indexed_ready: Arc<AtomicBool>,
    indexed_reader: Option<super::indexed_runtime::IndexedReaderPermit>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshOutcome {
    Published {
        items: usize,
    },
    /// A source build or indexed reader is busy; retain publication and retry.
    Busy,
}

/// Stage labels deliberately omit database values, provider URLs and paths.
#[derive(Debug, thiserror::Error)]
pub enum RefreshError {
    #[error("Next channel was removed or switched to another engine")]
    Retired,
    #[error("Next schedule read failed")]
    Read,
    #[error("Next schedule projection failed")]
    Projection,
    #[error(transparent)]
    Publication(#[from] publication::PublicationError),
    #[error("Next publication worker failed")]
    Worker,
}

// The async attempt revokes its permission when dropped, including timeouts.
// The blocking worker still owns admission until it has cleaned up its file.
struct PublicationAttempt(Arc<AtomicBool>);

impl Drop for PublicationAttempt {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl ChannelPublisher {
    /// Create the host's publisher using its existing shared scheduler locker.
    #[must_use]
    pub fn new(pool: DbPool, locker: EntityLocker, root: &Path, channel_id: i64) -> Self {
        Self {
            pool,
            locker,
            channel_id,
            folder: root.join(channel_id.to_string()),
            gate: Arc::default(),
            layout: publication::PlayoutLayout::Complete,
            indexed_ready: Arc::new(AtomicBool::new(true)),
            indexed_reader: None,
        }
    }

    /// Construct an explicitly indexed publisher after qualifying ADR 0131's
    /// reader capability. Ordinary sessions retain [`Self::new`]'s format.
    /// The first committed refresh regenerates derived state from accepted SQL
    /// before private preparation or session admission. Use one owner per
    /// channel and clone it; never mix layouts for an owner.
    #[must_use]
    pub fn new_indexed(
        pool: DbPool,
        locker: EntityLocker,
        root: &Path,
        channel_id: i64,
        clock: ersatzrs_core::programme::ProgrammeClock,
        reader: super::indexed_runtime::IndexedReaderPermit,
    ) -> Self {
        let mut publisher = Self::new(pool, locker, root, channel_id);
        publisher.layout = publication::PlayoutLayout::Indexed(Arc::new(clock));
        publisher.indexed_ready = Arc::new(AtomicBool::new(false));
        publisher.indexed_reader = Some(reader);
        publisher
    }

    pub(super) async fn authorize_reader(
        &self,
        binary: &Path,
    ) -> Result<(), super::indexed_runtime::IndexedRuntimeError> {
        if let Some(reader) = &self.indexed_reader {
            reader.authorize(binary).await?;
        }
        Ok(())
    }

    /// Read current channel settings and schedule only after acquiring admission.
    /// Project the complete schedule, then publish it on a blocking worker. The
    /// projector must preserve source/track/graphics semantics and bound any
    /// external work; a projection failure retains the previous publication.
    ///
    /// Cancellation during projection drops the locks and changes no file.
    /// Once the filesystem worker starts it owns both admission and build locks
    /// through cleanup, even if its awaiting caller is cancelled. Cancellation
    /// revokes replacement permission, checked after the blocking file flush.
    /// A later refresh cannot overtake that worker. A replacement already started
    /// cannot be undone by cancellation.
    ///
    /// # Errors
    /// Returns a redacted stage failure; no partial projection is published.
    pub async fn refresh<F, Fut>(
        &self,
        start: DateTime<Utc>,
        finish: DateTime<Utc>,
        project: F,
    ) -> Result<RefreshOutcome, RefreshError>
    where
        F: FnOnce(Channel, Vec<schedule::ScheduledMedia>) -> Fut,
        Fut: Future<Output = ersatzrs_core::repositories::Result<Vec<protocol::PlayoutItem>>>,
    {
        let layout = self.layout.clone();
        let ready = self.indexed_ready.clone();
        self.refresh_with(start, finish, project, move |folder, playout, active| {
            publication::publish_layout(
                folder,
                playout,
                &layout,
                &ready,
                || active.load(Ordering::Acquire),
                None,
                || {},
            )
        })
        .await
    }

    /// Publish only before the caller's monotonic safe handover deadline.
    /// The same deadline is checked in the blocking worker after file flush;
    /// delayed async timeout polling cannot authorize a late replacement.
    ///
    /// # Errors
    /// Returns the ordinary refresh failures or an expired-publication error.
    pub async fn refresh_before<F, Fut>(
        &self,
        start: DateTime<Utc>,
        finish: DateTime<Utc>,
        deadline: std::time::Instant,
        project: F,
    ) -> Result<RefreshOutcome, RefreshError>
    where
        F: FnOnce(Channel, Vec<schedule::ScheduledMedia>) -> Fut,
        Fut: Future<Output = ersatzrs_core::repositories::Result<Vec<protocol::PlayoutItem>>>,
    {
        self.refresh_before_committed(start, finish, deadline, project, || {})
            .await
    }

    /// Keep visible-file effects inside the owned blocking publication, before
    /// recovery cleanup and admission release. The hook also runs for a no-op
    /// index, admitting a new reader to the already-visible accepted schedule.
    pub(super) async fn refresh_before_committed<F, Fut>(
        &self,
        start: DateTime<Utc>,
        finish: DateTime<Utc>,
        deadline: std::time::Instant,
        project: F,
        committed: impl FnOnce() + Send + 'static,
    ) -> Result<RefreshOutcome, RefreshError>
    where
        F: FnOnce(Channel, Vec<schedule::ScheduledMedia>) -> Fut,
        Fut: Future<Output = ersatzrs_core::repositories::Result<Vec<protocol::PlayoutItem>>>,
    {
        if std::time::Instant::now() >= deadline {
            return Err(publication::PublicationError::Expired.into());
        }
        let layout = self.layout.clone();
        let ready = self.indexed_ready.clone();
        self.refresh_with(start, finish, project, move |folder, playout, active| {
            publication::publish_layout(
                folder,
                playout,
                &layout,
                &ready,
                || active.load(Ordering::Acquire),
                Some(deadline),
                committed,
            )
        })
        .await
    }

    pub(super) async fn callback_admission(&self) -> tokio::sync::OwnedMutexGuard<()> {
        self.gate.clone().lock_owned().await
    }

    async fn refresh_with<F, Fut, Write>(
        &self,
        start: DateTime<Utc>,
        finish: DateTime<Utc>,
        project: F,
        write: Write,
    ) -> Result<RefreshOutcome, RefreshError>
    where
        F: FnOnce(Channel, Vec<schedule::ScheduledMedia>) -> Fut,
        Fut: Future<Output = ersatzrs_core::repositories::Result<Vec<protocol::PlayoutItem>>>,
        Write: FnOnce(
                &Path,
                &protocol::Playout,
                &AtomicBool,
            ) -> Result<(), publication::PublicationError>
            + Send
            + 'static,
    {
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
            schedule::read_locked(&self.pool, &self.locker, &channel, start, finish)
                .await
                .map_err(|_| RefreshError::Read)?
        else {
            return Ok(RefreshOutcome::Busy);
        };
        let items = project(channel, std::mem::take(&mut locked.items))
            .await
            .map_err(|_| RefreshError::Projection)?;
        let count = items.len();
        let folder = self.folder.clone();
        let active = attempt.0.clone();
        let result = tokio::task::spawn_blocking(move || {
            // Keep ownership in this non-cancellable worker, not its awaiter.
            let _admission = admission;
            let _schedule = locked;
            write(
                &folder,
                &protocol::Playout {
                    version: protocol::PLAYOUT_VERSION.to_owned(),
                    items,
                },
                &active,
            )
        })
        .await
        .map_err(|_| RefreshError::Worker)?;
        if matches!(result, Err(publication::PublicationError::ReadersBusy)) {
            return Ok(RefreshOutcome::Busy);
        }
        result?;
        Ok(RefreshOutcome::Published { items: count })
    }
}

#[cfg(test)]
#[path = "coordinator_tests.rs"]
mod tests;
