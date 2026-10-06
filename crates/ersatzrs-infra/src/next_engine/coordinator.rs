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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshOutcome {
    Published {
        items: usize,
    },
    /// A scheduler build owns a source lock; keep the old file and retry later.
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
        }
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
        self.refresh_with(start, finish, project, |folder, playout, active| {
            publication::publish_playout_checked(
                folder,
                playout,
                || active.load(Ordering::Acquire),
                None,
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
        if std::time::Instant::now() >= deadline {
            return Err(publication::PublicationError::Expired.into());
        }
        self.refresh_with(start, finish, project, move |folder, playout, active| {
            publication::publish_playout_checked(
                folder,
                playout,
                || active.load(Ordering::Acquire),
                Some(deadline),
            )
        })
        .await
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
        tokio::task::spawn_blocking(move || {
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
        .map_err(|_| RefreshError::Worker)??;
        Ok(RefreshOutcome::Published { items: count })
    }
}

#[cfg(test)]
#[path = "coordinator_tests.rs"]
mod tests;
