use super::{encode_datetime, EntityId, RepositoryError, Result, Transaction};
use crate::repositories::SqlxResultExt as _;
use ersatzrs_domain::PlayoutYamlCheckpoint;
use std::collections::BTreeSet;

pub(super) async fn replace(
    tx: &mut Transaction<'_, crate::db::Db>,
    playout_id: EntityId,
    checkpoints: &[PlayoutYamlCheckpoint],
) -> Result<()> {
    let boundaries = sqlx::query_scalar::<_, String>(
        r#"SELECT "Start" FROM "PlayoutItem" WHERE "PlayoutId" = ?
           UNION SELECT "Finish" FROM "PlayoutItem" WHERE "PlayoutId" = ?
           UNION SELECT "Start" FROM "PlayoutGap" WHERE "PlayoutId" = ?
           UNION SELECT "Finish" FROM "PlayoutGap" WHERE "PlayoutId" = ?"#,
    )
    .bind(playout_id)
    .bind(playout_id)
    .bind(playout_id)
    .bind(playout_id)
    .fetch_all(&mut **tx)
    .await
    .repository()?
    .into_iter()
    .collect::<BTreeSet<_>>();
    let mut dates = BTreeSet::new();
    for checkpoint in checkpoints {
        ersatzrs_core::scheduling::validate_yaml_checkpoint(checkpoint, playout_id)
            .map_err(|error| RepositoryError::Conflict(error.to_string()))?;
        let date = encode_datetime(checkpoint.checkpoint_date);
        if !boundaries.contains(&date) || !dates.insert(date) {
            return Err(RepositoryError::Conflict(
                "YAML checkpoint does not identify a unique published boundary".into(),
            ));
        }
    }
    sqlx::query(r#"DELETE FROM "PlayoutYamlCheckpoint" WHERE "PlayoutId" = ?"#)
        .bind(playout_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    for checkpoint in checkpoints {
        sqlx::query(r#"INSERT INTO "PlayoutYamlCheckpoint" ("PlayoutId", "CheckpointDate", "Context") VALUES (?, ?, ?)"#)
            .bind(playout_id).bind(encode_datetime(checkpoint.checkpoint_date)).bind(&checkpoint.context)
            .execute(&mut **tx).await.repository()?;
    }
    Ok(())
}
