//! Mutation ordering for derived Next files; scheduling state remains untouched.
use std::future::Future;

use ersatzrs_domain::PlayoutProjectionChange;

use crate::{ApplicationError, ChannelRuntime, Result};

/// Successful scans replace metadata without rebuilding the persisted timeline.
/// Resolve committed library dependencies, then use the existing bounded queue.
pub(crate) async fn refresh_scanned_library(
    runtime: &dyn ChannelRuntime,
    library_id: i64,
) -> std::result::Result<(), crate::ScanRuntimeError> {
    let channels = runtime
        .playout_projection_dependents(PlayoutProjectionChange::Library(library_id))
        .await
        .map_err(|_| {
            crate::ScanRuntimeError::new("Scanned library playout dependencies could not be loaded")
        })?;
    if !channels.is_empty() {
        runtime
            .refresh_playout_projections(channels)
            .await
            .map_err(|_| {
                crate::ScanRuntimeError::new(
                    "Scanned library playout projections could not be refreshed",
                )
            })?;
    }
    Ok(())
}

pub(crate) async fn refresh_after<T>(
    runtime: &dyn ChannelRuntime,
    change: PlayoutProjectionChange,
    mutation: impl Future<Output = Result<T>>,
) -> Result<T> {
    let channels = runtime
        .playout_projection_dependents(change)
        .await
        .map_err(|error| ApplicationError::Unavailable {
            operation: "resolve affected playout projections",
            message: error.to_string(),
        })?;
    let result = mutation.await?;
    if !channels.is_empty() {
        runtime
            .refresh_playout_projections(channels)
            .await
            .map_err(|error| ApplicationError::Unavailable {
                operation: "refresh saved playout projections",
                message: error.to_string(),
            })?;
    }
    Ok(result)
}

#[cfg(test)]
#[path = "playout_projection_tests.rs"]
mod tests;
