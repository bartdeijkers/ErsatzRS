//! ErsatzTV `MediaItemExtensions.GetDurationForPlayout` for persisted media.

use chrono::Duration;
use ersatzrs_core::repositories::{RepositoryError, Result};
use sqlx::Row;

use crate::{db::DbRow, repositories::SqlxResultExt as _};

/// Queries select one version and the first image metadata row by identity.
/// Keep this decoding shared by scheduling, preview and persisted Next hints.
pub(crate) fn from_row(row: &DbRow) -> Result<Duration> {
    if row
        .try_get::<Option<i64>, _>("ImageId")
        .repository()?
        .is_some()
    {
        let seconds = row
            .try_get::<Option<f64>, _>("ImageDurationSeconds")
            .repository()?
            .unwrap_or(15.0);
        let duration = std::time::Duration::try_from_secs_f64(seconds).map_err(|_| invalid())?;
        return Duration::from_std(duration).map_err(|_| invalid());
    }
    let version: Option<String> = row.try_get("Duration").repository()?;
    let duration = crate::repositories::common::try_decode_duration_text(
        version.as_deref().unwrap_or("00:00:00"),
    )?;
    if duration.is_zero() {
        if let Some(remote) = row
            .try_get::<Option<String>, _>("RemoteDuration")
            .repository()?
        {
            return crate::repositories::common::try_decode_duration_text(&remote);
        }
    }
    Ok(duration)
}

fn invalid() -> RepositoryError {
    RepositoryError::unavailable("playout duration", "invalid image duration")
}

#[cfg(test)]
#[path = "playout_duration_tests.rs"]
pub(crate) mod tests;
