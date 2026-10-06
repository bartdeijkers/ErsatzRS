//! Settle the prepared file group and SQL decision under one owned build hold.

use super::{publication_error, Result};
use crate::{
    maintenance::PlayoutBuildCancellation,
    next_engine::host::{AppliedHostPlayout, PreparedHostPlayout},
    repositories::PendingPlayoutBuild,
};
use ersatzrs_core::{locking::PlayoutBuildHold, scheduling::SchedulingError};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

struct Attempt(Arc<AtomicBool>);

impl Drop for Attempt {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

pub(super) async fn settle(
    pending: PendingPlayoutBuild,
    prepared: Vec<PreparedHostPlayout>,
    cancelled: Option<PlayoutBuildCancellation>,
    hold: PlayoutBuildHold,
) -> Result<Vec<i64>> {
    let attempt = Attempt(Arc::new(AtomicBool::new(true)));
    let active = attempt.0.clone();
    let runtime = tokio::runtime::Handle::current();
    // One blocking owner retains SQL, publisher/source admission, rollback
    // files and the originating build hold through the complete decision.
    // Dropping the async awaiter revokes commit admission, not cleanup ownership.
    tokio::task::spawn_blocking(move || {
        let _hold = hold;
        settle_blocking(pending, prepared, &runtime, || {
            !cancelled.as_ref().is_some_and(|check| check()) && active.load(Ordering::Acquire)
        })
    })
    .await
    .map_err(|_| publication_error("publication finalizer failed"))?
}

fn settle_blocking(
    mut pending: PendingPlayoutBuild,
    mut prepared: Vec<PreparedHostPlayout>,
    runtime: &tokio::runtime::Handle,
    is_current: impl Fn() -> bool,
) -> Result<Vec<i64>> {
    if pending.publication_deadline().is_none() {
        // Explicit replacement and append-only builds have no retained-prefix
        // admission window. Do not expose their immediately selectable item IDs
        // before SQL can serve them; preserve their commit-then-publish contract.
        if !is_current() {
            return Err(SchedulingError::Cancelled.into());
        }
        runtime.block_on(pending.commit())?;
        let mut published = Vec::with_capacity(prepared.len());
        for owner in prepared {
            let channel_id = owner.channel_id();
            runtime
                .block_on(owner.publish())
                .map_err(|error| publication_error(error.to_string()))?;
            published.push(channel_id);
        }
        return Ok(published);
    }
    let decision_deadline = decision_deadline(pending.publication_deadline(), prepared.len())?;
    if let Some(deadline) = decision_deadline {
        if std::time::Instant::now() >= deadline {
            return Err(
                publication_error("publication deadline leaves no file recovery window").into(),
            );
        }
        pending.restrict_deadline(deadline);
        for owner in &mut prepared {
            owner.restrict_deadline(deadline);
        }
    }
    let mut applied = Vec::<AppliedHostPlayout>::with_capacity(prepared.len());
    let decision = (|| -> Result<()> {
        for owner in prepared {
            if !is_current() {
                return Err(SchedulingError::Cancelled.into());
            }
            applied.push(
                owner
                    .publish_reversible()
                    .map_err(|error| publication_error(error.to_string()))?,
            );
        }
        if !is_current() {
            return Err(SchedulingError::Cancelled.into());
        }
        runtime.block_on(pending.commit())?;
        // Disarm the entire group before any asynchronous gap registration:
        // committed SQL must never be paired with an implicit file rollback.
        for owner in &mut applied {
            owner.accept();
        }
        Ok(())
    })();
    if let Err(error) = decision {
        let mut restoration_failed = false;
        for owner in applied.iter_mut().rev() {
            if let Err(error) = owner.restore() {
                tracing::error!(%error, "Next publication compensation failed");
                restoration_failed = true;
            }
        }
        if restoration_failed {
            return Err(publication_error(
                "The scheduling transaction was not committed, but Next file restoration failed. Accepted SQL must be republished after storage recovery.",
            ).into());
        }
        return Err(error);
    }
    Ok(applied
        .into_iter()
        .map(|owner| runtime.block_on(owner.register()))
        .collect())
}

fn decision_deadline(
    deadline: Option<std::time::Instant>,
    owners: usize,
) -> Result<Option<std::time::Instant>> {
    let Some(deadline) = deadline else {
        return Ok(None);
    };
    if owners == 0 {
        return Ok(Some(deadline));
    }
    let owners =
        u32::try_from(owners).map_err(|_| publication_error("too many publication owners"))?;
    let seconds = owners
        .checked_mul(crate::maintenance::handover::ROLLBACK_SECONDS_PER_OWNER)
        .and_then(|seconds| {
            seconds.checked_add(crate::maintenance::handover::SQL_DECISION_MARGIN_SECONDS)
        })
        .ok_or_else(|| publication_error("publication recovery budget overflow"))?;
    deadline
        .checked_sub(std::time::Duration::from_secs(u64::from(seconds)))
        .map(Some)
        .ok_or_else(|| publication_error("invalid publication recovery deadline").into())
}

#[cfg(test)]
#[path = "finalization_tests.rs"]
mod tests;
