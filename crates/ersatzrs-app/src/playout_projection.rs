//! Mutation ordering for derived Next files; scheduling state remains untouched.
use std::future::Future;

use ersatzrs_domain::PlayoutProjectionChange;

use crate::{ApplicationError, ChannelRuntime, Result};

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
