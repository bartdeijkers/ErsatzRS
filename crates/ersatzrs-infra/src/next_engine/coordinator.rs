//! Serialize a channel's database read, projection and atomic publication.

use std::{
    future::Future,
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::{DateTime, Utc};
use ersatzrs_core::{locking::EntityLocker, repositories::ChannelRepository};
use ersatzrs_domain::Channel;
use tokio::sync::Mutex;

use crate::{db::DbPool, repositories::make_repositories};

use super::{protocol, publication, schedule};

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
    #[error("Next schedule read failed")]
    Read,
    #[error("Next schedule projection failed")]
    Projection,
    #[error(transparent)]
    Publication(#[from] publication::PublicationError),
    #[error("Next publication worker failed")]
    Worker,
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
    /// Cancellation before publication drops the locks and changes no file.
    /// Once the filesystem worker starts it owns both admission and build locks
    /// through replacement, even if its awaiting caller is cancelled. A later
    /// refresh therefore cannot overtake an abandoned blocking write.
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
        self.refresh_with(start, finish, project, publication::publish_playout)
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
        Write: FnOnce(&Path, &protocol::Playout) -> Result<(), publication::PublicationError>
            + Send
            + 'static,
    {
        let admission = self.gate.clone().lock_owned().await;
        let channel = make_repositories(self.pool.clone())
            .channel
            .get_with_profile(self.channel_id)
            .await
            .map_err(|_| RefreshError::Read)?;
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
