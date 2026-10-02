use super::{encode_datetime, EntityId, RepositoryError, Result, Transaction};
use crate::repositories::SqlxResultExt as _;
use chrono::{DateTime, Utc};
use ersatzrs_domain::PlayoutScriptedCheckpoint;
use std::collections::BTreeSet;

pub(super) fn validate(
    playout_id: EntityId,
    checkpoints: &[PlayoutScriptedCheckpoint],
    published: &BTreeSet<DateTime<Utc>>,
    mut added: impl Iterator<Item = DateTime<Utc>>,
) -> Result<()> {
    let mut dates = BTreeSet::new();
    for checkpoint in checkpoints {
        ersatzrs_core::scheduling::validate_scripted_checkpoint(checkpoint, playout_id)
            .map_err(|error| RepositoryError::Conflict(error.to_string()))?;
        let date = checkpoint.checkpoint_date;
        if !published.contains(&date) || !dates.insert(date) {
            return Err(RepositoryError::Conflict(
                "Scripted checkpoint does not identify a unique published boundary".to_owned(),
            ));
        }
    }
    if !added.all(|date| dates.contains(&date)) {
        return Err(RepositoryError::Conflict(
            "Scripted checkpoint set does not cover every new programme boundary".to_owned(),
        ));
    }
    Ok(())
}

pub(super) async fn replace(
    tx: &mut Transaction<'_, crate::db::Db>,
    playout_id: EntityId,
    checkpoints: &[PlayoutScriptedCheckpoint],
) -> Result<()> {
    sqlx::query(r#"DELETE FROM "PlayoutScriptedCheckpoint" WHERE "PlayoutId" = ?"#)
        .bind(playout_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    for checkpoint in checkpoints {
        sqlx::query(r#"INSERT INTO "PlayoutScriptedCheckpoint" ("PlayoutId", "CheckpointDate", "Context") VALUES (?, ?, ?)"#)
            .bind(playout_id).bind(encode_datetime(checkpoint.checkpoint_date)).bind(&checkpoint.context)
            .execute(&mut **tx).await.repository()?;
    }
    Ok(())
}
