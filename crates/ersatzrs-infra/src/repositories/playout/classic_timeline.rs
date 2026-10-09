//! Validate the exact retained/candidate Classic timeline before any mutation.

use super::*;
use crate::db::Db;
use ersatzrs_core::scheduling::{truncate_to_persisted_precision, validate_classic_timeline};

pub(super) async fn validate_publication(
    tx: &mut sqlx::Transaction<'_, Db>,
    playout_id: EntityId,
    result: &SchedulingResult,
    offset: chrono::Duration,
) -> Result<()> {
    if result
        .added_items
        .iter()
        .any(|item| item.playout_id != playout_id)
        || result
            .added_history
            .iter()
            .any(|entry| entry.playout_id != playout_id)
    {
        return Err(RepositoryError::Conflict(
            "Classic timeline candidate has another playout owner".into(),
        ));
    }
    let rows =
        sqlx::query(r#"SELECT "Id", "Start", "Finish" FROM "PlayoutItem" WHERE "PlayoutId"=?"#)
            .bind(playout_id)
            .fetch_all(&mut **tx)
            .await
            .repository()?;
    let cutoff = result
        .remove_before
        .or(result.retain_through.filter(|_| result.clear_items))
        .map(|time| time - offset.max(chrono::Duration::zero()));
    let mut retained = Vec::new();
    for row in rows {
        let id: i64 = row.try_get("Id").repository()?;
        let start = decode_datetime(
            "playout item start",
            &row.try_get::<String, _>("Start").repository()?,
        )?;
        let finish = decode_datetime(
            "playout item finish",
            &row.try_get::<String, _>("Finish").repository()?,
        )?;
        // Expired rows are removed by publication even when a legacy row
        // crosses the restore seam. Only a still-retained row can be cut.
        if cutoff.is_some_and(|cutoff| finish < cutoff) {
            continue;
        }
        if result.clear_items
            && result
                .retain_through
                .is_some_and(|seam| start < seam && finish > seam)
        {
            return Err(RepositoryError::Conflict("Classic timeline retention boundary cuts a published item; choose a complete programme boundary".into()));
        }
        if (result.clear_items && result.retain_through.is_none_or(|seam| finish > seam))
            || (!result.clear_items && result.items_to_remove.contains(&id))
        {
            continue;
        }
        retained.push((start, finish));
    }
    retained.sort_unstable();
    let candidate = result.added_items.iter().map(|item| {
        (
            truncate_to_persisted_precision(item.start),
            truncate_to_persisted_precision(item.finish),
        )
    });
    validate_classic_timeline(
        candidate.clone(),
        retained.last().map(|(_, finish)| *finish),
    )
    .map_err(|error| RepositoryError::Conflict(error.to_string()))?;
    retained.extend(candidate);
    validate_classic_timeline(retained.iter().copied(), None)
        .map_err(|error| RepositoryError::Conflict(error.to_string()))?;
    if let (Some((_, tail)), Some(anchor)) = (retained.last(), &result.anchor) {
        if truncate_to_persisted_precision(anchor.next_start) < *tail {
            return Err(RepositoryError::Conflict("Classic timeline anchor precedes the published finish; restore consistent continuation state".into()));
        }
    }
    Ok(())
}
