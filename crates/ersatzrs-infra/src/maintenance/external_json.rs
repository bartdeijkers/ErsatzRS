//! Adopt external documents through the ordinary owned playout worker.

use std::{path::Path, time::Instant};

use chrono::{DateTime, Utc};
use ersatzrs_core::scheduling::PlayoutPreservation;
use ersatzrs_domain::Playout;

use super::{
    handover, HlsSessionRegistry, PlayoutBuildCancellation, PlayoutBuildMode, PlayoutBuildRequest,
    PlayoutBuildStatus, PlayoutLockGuard, PlayoutRepository, Result, SchedulingError,
    SqlitePlayoutRepository,
};
use crate::{
    db::DbPool,
    external_json::{adaptation, snapshot},
};

pub(super) struct Attempt {
    pub request: PlayoutBuildRequest,
    pub now: DateTime<Utc>,
    pub started: Instant,
}

/// False retains the upstream no-op for an unadopted, legacy automatic request.
pub(super) async fn build(
    pool: &DbPool,
    playout: &Playout,
    attempt: Attempt,
    sessions: Option<&dyn HlsSessionRegistry>,
    cancelled: Option<&PlayoutBuildCancellation>,
    guard: &mut PlayoutLockGuard,
) -> Result<bool> {
    let (baseline, expected_revision) =
        load_baseline(pool, playout.id, attempt.request.preservation).await?;
    let mut request = attempt.request;
    if request.preservation == PlayoutPreservation::EngineDefault {
        if baseline.is_none() {
            return Ok(false);
        }
        // Once adopted, ordinary Continue remains append-only. A scheduled
        // Reset must not silently discard the accepted on-air programme.
        request.preservation = PlayoutPreservation::KeepCurrent;
    }
    require_native_consumers(pool, playout.channel_id).await?;
    let path = playout.schedule_file.as_deref().unwrap_or_default();
    if request.mode == PlayoutBuildMode::Continue
        && baseline
            .as_ref()
            .is_some_and(|old| old.schedule_file != path)
    {
        return Err(refusal(
            "external JSON Continue cannot change its accepted source",
        ));
    }
    let candidate = crate::external_json::load(Path::new(path))
        .await
        .map_err(refusal)?
        .ok_or_else(|| refusal("external JSON source is missing or contains no document"))?;
    let window = if sessions.is_some() && request.preservation == PlayoutPreservation::KeepCurrent {
        Some(handover::Window::load(pool, playout.channel_id, attempt.now).await?)
    } else {
        None
    };
    let adapted = adaptation::reconcile(
        baseline.as_ref().map(|old| &old.schedule),
        candidate,
        request.mode,
        request.preservation,
        window.map_or(attempt.now, |window| window.not_before),
    )
    .map_err(refusal)?;
    if cancelled.is_some_and(|check| check()) {
        return Err(SchedulingError::Cancelled.into());
    }
    if adapted.changed {
        let clock = match window {
            Some(window) => window.boundary_clock(adapted.effective_from, attempt.now)?,
            None => None,
        };
        let deadline = super::publication_deadline(clock, attempt.now, attempt.started)?;
        snapshot::publish(
            pool,
            snapshot::Publication {
                playout_id: playout.id,
                expected_revision,
                schedule_file: path,
                published_at: attempt.now,
                effective_from: adapted.effective_from,
                deadline,
            },
            &adapted.schedule,
        )
        .await
        .map_err(refusal)?;
        // Native readers consult the accepted document at the next item. A
        // preserving action must not restart their existing producer.
        guard.allow_publication_reads();
    }
    // Repeat an explicit replacement's notification even if its prior attempt
    // committed the document but failed before all native owners were notified.
    if request.preservation == PlayoutPreservation::ReplaceCurrent {
        guard.allow_publication_reads();
        super::notify_playout_channels(
            pool,
            playout.channel_id,
            false,
            sessions,
            Instant::now() + std::time::Duration::from_secs(30),
            &[],
        )
        .await?;
    }
    SqlitePlayoutRepository(pool.clone())
        .upsert_build_status(&PlayoutBuildStatus {
            playout_id: playout.id,
            last_build: attempt.now,
            success: true,
            message: None,
        })
        .await?;
    Ok(true)
}

async fn require_native_consumers(pool: &DbPool, channel: i64) -> Result<()> {
    if !super::load_playout_dependent_channel_numbers(pool, channel, true, &[])
        .await?
        .is_empty()
    {
        return Err(refusal("external JSON adaptation requires native playback on the channel and its mirrors; Next does not consume external documents"));
    }
    Ok(())
}

async fn load_baseline(
    pool: &DbPool,
    playout_id: i64,
    preservation: PlayoutPreservation,
) -> Result<(Option<snapshot::Snapshot>, Option<i64>)> {
    match snapshot::accepted(pool, playout_id).await {
        Ok(baseline) => {
            let revision = baseline.as_ref().map(|old| old.revision);
            Ok((baseline, revision))
        }
        Err(snapshot::SnapshotError::Invalid | snapshot::SnapshotError::TooLarge)
            if preservation == PlayoutPreservation::ReplaceCurrent =>
        {
            // Only explicit replacement may repair unreadable state. Keep
            // the revision fence; readers still fail closed until commit.
            Ok((
                None,
                snapshot::revision(pool, playout_id)
                    .await
                    .map_err(refusal)?,
            ))
        }
        Err(error) => Err(refusal(error)),
    }
}

fn refusal(error: impl std::fmt::Display) -> super::MaintenanceError {
    SchedulingError::InvalidRebuild(error.to_string()).into()
}
