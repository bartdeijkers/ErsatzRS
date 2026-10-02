use chrono::Duration;
use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_core::scheduling::{
    truncate_to_persisted_precision, validate_scripted_timeline, SchedulingError, SchedulingResult,
};
use ersatzrs_domain::EntityId;
use sqlx::{Row, Transaction};

use crate::db::Db;
use crate::repositories::common::decode_datetime;
use crate::repositories::SqlxResultExt as _;

fn rejected(message: impl Into<String>) -> RepositoryError {
    RepositoryError::Conflict(SchedulingError::InvalidScriptedTimeline(message.into()).to_string())
}

pub(super) async fn validate_publication(
    tx: &mut Transaction<'_, Db>,
    playout_id: EntityId,
    result: &SchedulingResult,
    max_playout_offset: Duration,
) -> Result<()> {
    if result
        .added_items
        .iter()
        .any(|item| item.playout_id != playout_id)
        || result
            .added_history
            .iter()
            .any(|entry| entry.playout_id != playout_id)
        || !result.items_to_remove.is_empty()
        || (!result.clear_items && !result.history_to_remove.is_empty())
    {
        return Err(rejected(
            "candidate does not preserve Scripted playout ownership",
        ));
    }
    let rows = sqlx::query(r#"SELECT "Start", "Finish" FROM "PlayoutItem" WHERE "PlayoutId" = ?"#)
        .bind(playout_id)
        .fetch_all(&mut **tx)
        .await
        .repository()?;
    let cutoff = result
        .remove_before
        .or(result.retain_through.filter(|_| result.clear_items))
        .map(|time| time - max_playout_offset.max(Duration::zero()));
    let mut retained = Vec::new();
    for row in rows {
        let start = decode_datetime(
            "playout item start",
            &row.try_get::<String, _>("Start").repository()?,
        )?;
        let finish = decode_datetime(
            "playout item finish",
            &row.try_get::<String, _>("Finish").repository()?,
        )?;
        if cutoff.is_some_and(|cutoff| finish < cutoff)
            || (result.clear_items && result.retain_through.is_none_or(|seam| finish > seam))
        {
            continue;
        }
        retained.push((start, finish));
    }
    retained.sort_unstable();
    let appended = result.added_items.iter().map(|item| {
        (
            truncate_to_persisted_precision(item.start),
            truncate_to_persisted_precision(item.finish),
        )
    });
    let earliest_start = retained.last().map(|(_, finish)| *finish);
    validate_scripted_timeline(appended.clone(), earliest_start)
        .map_err(|error| RepositoryError::Conflict(error.to_string()))?;
    retained.extend(appended);
    validate_scripted_timeline(retained.iter().copied(), None)
        .map_err(|error| RepositoryError::Conflict(error.to_string()))?;
    if let Some((_, tail)) = retained.last() {
        if result
            .anchor
            .as_ref()
            .is_none_or(|anchor| truncate_to_persisted_precision(anchor.next_start) != *tail)
        {
            return Err(rejected(
                "candidate anchor does not match the published tail",
            ));
        }
    }
    Ok(())
}
