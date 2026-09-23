//! Preserve source timing when Next resolves a demand-time item callback.

use chrono::{DateTime, Utc};

use super::protocol::{PlayoutItem, Source};

/// Stable callback diagnostics contain no persisted IDs, paths or credentials.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum DynamicItemError {
    #[error("Next dynamic item request is outside its published interval")]
    OutsideInterval,
    #[error("Next dynamic item source timing is invalid")]
    SourceTiming,
    #[error("Next dynamic item cannot resolve to another dynamic source")]
    RecursiveSource,
}

/// Resolve a concrete published item at the time requested by Next.
///
/// Next's `ChannelSession::resolve_dynamic_item` resets the returned start to its
/// requested time. Therefore all seekable inputs must advance by that same delta;
/// returning the original item would restart media on every work-ahead callback.
/// Out-points and the original finish remain unchanged. Generated lavfi inputs
/// have no source seeks and use the shortened item's duration in Next's
/// `input_timing`; preserve their finite probe hints and explicit silence.
///
/// # Errors
/// Rejects requests outside the original half-open interval, overflowing or
/// exhausted source timing, and Dynamic sources anywhere in the concrete result.
pub(crate) fn resolve_at(
    item: &PlayoutItem,
    requested_start: DateTime<Utc>,
) -> Result<PlayoutItem, DynamicItemError> {
    if requested_start < item.start || requested_start >= item.finish {
        return Err(DynamicItemError::OutsideInterval);
    }
    // The Next wire format and input_timing both represent seeks in milliseconds.
    let offset_ms = u64::try_from((requested_start - item.start).num_milliseconds())
        .map_err(|_| DynamicItemError::SourceTiming)?;
    let mut resolved = item.clone();
    if let Some(source) = &mut resolved.source {
        advance(source, offset_ms)?;
    }
    if let Some(tracks) = &mut resolved.tracks {
        for track in [&mut tracks.audio, &mut tracks.video, &mut tracks.subtitle]
            .into_iter()
            .flatten()
        {
            if let Some(source) = &mut track.source {
                advance(source, offset_ms)?;
            }
        }
    }
    resolved.start = requested_start;
    Ok(resolved)
}

fn advance(source: &mut Source, offset_ms: u64) -> Result<(), DynamicItemError> {
    match source {
        Source::Local {
            in_point_ms,
            out_point_ms,
            ..
        }
        | Source::Http {
            in_point_ms,
            out_point_ms,
            ..
        } => {
            let seek = in_point_ms
                .unwrap_or(0)
                .checked_add(offset_ms)
                .ok_or(DynamicItemError::SourceTiming)?;
            if out_point_ms.is_some_and(|end| seek >= end) {
                return Err(DynamicItemError::SourceTiming);
            }
            if offset_ms != 0 {
                *in_point_ms = Some(seek);
            }
        }
        Source::Lavfi { .. } => {}
        Source::Dynamic { .. } => return Err(DynamicItemError::RecursiveSource),
    }
    Ok(())
}

#[cfg(test)]
#[path = "dynamic_item_tests.rs"]
mod tests;
