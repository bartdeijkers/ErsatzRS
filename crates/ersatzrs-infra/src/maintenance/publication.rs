//! Prepare active Next projections while scheduling writes remain private.

use super::{PlayoutExecution, Result, PLAYOUT_PUBLICATION_OPERATION};
use crate::repositories::SqlitePlayoutRepository;
use chrono::{DateTime, Duration, Utc};
use ersatzrs_core::{
    repositories::RepositoryError,
    scheduling::{SchedulingError, SchedulingResult},
};
use ersatzrs_domain::Playout;

#[path = "finalization.rs"]
mod finalization;

pub(super) struct Notifications<'a> {
    pub sessions: Option<&'a dyn ersatzrs_core::hls::HlsSessionRegistry>,
    pub published_next: &'a [i64],
}

pub(super) async fn publish_build(
    repository: &SqlitePlayoutRepository,
    result: &SchedulingResult,
    playout: &Playout,
    max_playout_offset: Duration,
    execution: PlayoutExecution<'_>,
    now: DateTime<Utc>,
    hold: ersatzrs_core::locking::PlayoutBuildHold,
) -> Result<Vec<i64>> {
    super::log_build_warnings(playout.id, result);
    let mut pending = repository
        .prepare_build_result(result, playout, max_playout_offset)
        .await?;
    let prepared = if let Some(host) = execution.next {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let deadline = result
            .publication_deadline
            .map_or(deadline, |limit| limit.min(deadline));
        let finish = DateTime::from_timestamp(253_402_300_799, 0)
            .ok_or_else(|| publication_error("invalid publication horizon"))?;
        host.prepare_active_build(&mut pending, now..finish, deadline)
            .await
            .map_err(|error| publication_error(error.to_string()))?
            .ok_or_else(|| publication_error("another source is busy; retry the build"))?
    } else {
        Vec::new()
    };
    if execution.cancelled.is_some_and(|cancelled| cancelled()) {
        return Err(SchedulingError::Cancelled.into());
    }
    let published =
        finalization::settle(pending, prepared, execution.cancelled.cloned(), hold).await?;
    if let Err(error) =
        crate::subtitles::extract_embedded_playout_subtitles(&repository.0, playout.id).await
    {
        tracing::warn!(playout_id = playout.id, %error, "embedded subtitle extraction failed after playout build");
    }
    Ok(published)
}

fn publication_error(message: impl Into<String>) -> RepositoryError {
    RepositoryError::unavailable(PLAYOUT_PUBLICATION_OPERATION, message.into())
}
