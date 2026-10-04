//! Durable Classic deferred-save receipts, published with the accepted build.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use ersatzrs_core::repositories::{RepositoryError, Result, ScheduleDeferredChange};
use ersatzrs_core::scheduling::SchedulingResult;
use ersatzrs_domain::EntityId;
use serde::{Deserialize, Serialize};
use sqlx::{Executor, Row, Transaction};

use crate::db::{Db, DbPool};
use crate::repositories::{common::decode_datetime, SqlxResultExt as _};

#[derive(Clone, Serialize, Deserialize)]
struct Receipt {
    saved_at: DateTime<Utc>,
    effective_after: Option<DateTime<Utc>>,
    applied_from: Option<DateTime<Utc>>,
}

#[derive(Serialize, Deserialize)]
struct Receipts {
    version: u8,
    schedules: BTreeMap<EntityId, Receipt>,
}

pub(crate) fn key(playout_id: EntityId) -> String {
    format!("playout.classic.deferred.{playout_id}")
}

pub(crate) async fn revision<'e>(
    executor: impl Executor<'e, Database = Db>,
    playout_id: EntityId,
) -> std::result::Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(r#"SELECT COALESCE("Value", '') FROM "ConfigElement" WHERE "Key"=?"#)
        .bind(key(playout_id))
        .fetch_optional(executor)
        .await
}

fn decode(value: Option<&str>) -> Result<Receipts> {
    let Some(value) = value else {
        return Ok(Receipts {
            version: 1,
            schedules: BTreeMap::new(),
        });
    };
    let receipts: Receipts = serde_json::from_str(value).map_err(|_| {
        RepositoryError::Conflict("Classic deferred save receipt is invalid".into())
    })?;
    if receipts.version != 1 {
        return Err(RepositoryError::Conflict(
            "Classic deferred save receipt version is unsupported".into(),
        ));
    }
    Ok(receipts)
}

pub(crate) async fn state_is_valid(
    pool: &DbPool,
    playout_id: EntityId,
) -> std::result::Result<bool, sqlx::Error> {
    Ok(decode(revision(pool, playout_id).await?.as_deref()).is_ok())
}

async fn store(tx: &mut Transaction<'_, Db>, id: EntityId, receipts: &Receipts) -> Result<()> {
    if receipts.schedules.is_empty() {
        sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key"=?"#)
            .bind(key(id))
            .execute(&mut **tx)
            .await
            .repository()?;
        return Ok(());
    }
    let value = serde_json::to_string(receipts).map_err(|error| {
        RepositoryError::Conflict(format!(
            "Classic deferred save receipt cannot be encoded: {error}"
        ))
    })?;
    sqlx::query(
        r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)
        ON CONFLICT("Key") DO UPDATE SET "Value"=excluded."Value""#,
    )
    .bind(key(id))
    .bind(value)
    .execute(&mut **tx)
    .await
    .repository()?;
    Ok(())
}

pub(crate) async fn queue(
    tx: &mut Transaction<'_, Db>,
    playout_id: EntityId,
    schedule_id: EntityId,
    effective_after: Option<DateTime<Utc>>,
) -> Result<()> {
    let value = revision(&mut **tx, playout_id).await.repository()?;
    let mut receipts = decode(value.as_deref())?;
    receipts.schedules.insert(
        schedule_id,
        Receipt {
            saved_at: Utc::now(),
            effective_after,
            applied_from: None,
        },
    );
    store(tx, playout_id, &receipts).await
}

pub(crate) async fn playout_ids<'e>(
    executor: impl Executor<'e, Database = Db>,
    schedule_id: EntityId,
) -> Result<Vec<EntityId>> {
    sqlx::query_scalar(
        r#"SELECT "p"."Id" FROM "Playout" AS "p"
        WHERE "p"."ProgramScheduleId"=? OR EXISTS (
            SELECT 1 FROM "ProgramScheduleAlternate" AS "a"
            WHERE "a"."PlayoutId"="p"."Id" AND "a"."ProgramScheduleId"=?)
        ORDER BY "p"."Id""#,
    )
    .bind(schedule_id)
    .bind(schedule_id)
    .fetch_all(executor)
    .await
    .repository()
}

pub(crate) async fn clear_schedule(
    tx: &mut Transaction<'_, Db>,
    schedule_id: EntityId,
) -> Result<()> {
    for id in playout_ids(&mut **tx, schedule_id).await? {
        let value = revision(&mut **tx, id).await.repository()?;
        let mut receipts = decode(value.as_deref())?;
        if receipts.schedules.remove(&schedule_id).is_some() {
            store(tx, id, &receipts).await?;
        }
    }
    Ok(())
}

pub(crate) async fn mark_applied(
    tx: &mut Transaction<'_, Db>,
    playout_id: EntityId,
    result: &SchedulingResult,
) -> Result<()> {
    let value = revision(&mut **tx, playout_id).await.repository()?;
    let mut receipts = match decode(value.as_deref()) {
        Ok(receipts) => receipts,
        Err(_) if result.clear_items && result.retain_through.is_none() => {
            // An explicit full replacement supersedes corrupt bookkeeping.
            // Preserving builds must not guess which edit/boundary was saved.
            sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key"=?"#)
                .bind(key(playout_id))
                .execute(&mut **tx)
                .await
                .repository()?;
            return Ok(());
        }
        Err(error) => return Err(error),
    };
    let mut changed = false;
    if result.publication_guard.is_none()
        && receipts
            .schedules
            .values()
            .any(|receipt| receipt.applied_from.is_none())
    {
        return Err(RepositoryError::Conflict(
            "Classic deferred save requires a revision-fenced build; retry continuation".into(),
        ));
    }
    for item in &result.added_items {
        let Some(id) = item
            .scheduling_context
            .as_deref()
            .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
            .and_then(|value| value.get("ScheduleId").and_then(serde_json::Value::as_i64))
        else {
            continue;
        };
        if let Some(receipt) = receipts
            .schedules
            .get_mut(&id)
            .filter(|receipt| receipt.applied_from.is_none())
        {
            receipt.applied_from = Some(item.start);
            changed = true;
        }
    }
    if changed {
        store(tx, playout_id, &receipts).await?;
    }
    Ok(())
}

pub(crate) async fn list(
    pool: &DbPool,
    schedule_id: EntityId,
) -> Result<Vec<ScheduleDeferredChange>> {
    let mut result = Vec::new();
    for id in playout_ids(pool, schedule_id).await? {
        let value = revision(pool, id).await.repository()?;
        let receipts = decode(value.as_deref())?;
        let Some(receipt) = receipts.schedules.get(&schedule_id) else {
            continue;
        };
        let row = sqlx::query(
            r#"SELECT "c"."Name", "s"."LastBuild", "s"."Success", "s"."Message"
            FROM "Playout" AS "p" JOIN "Channel" AS "c" ON "c"."Id"="p"."ChannelId"
            LEFT JOIN "PlayoutBuildStatus" AS "s" ON "s"."PlayoutId"="p"."Id" WHERE "p"."Id"=?"#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await
        .repository()?;
        let Some(row) = row else {
            continue;
        };
        result.push(ScheduleDeferredChange {
            playout_id: id,
            channel_name: row.try_get("Name").repository()?,
            saved_at: receipt.saved_at,
            effective_after: receipt.effective_after,
            applied_from: receipt.applied_from,
            last_build: row
                .try_get::<Option<String>, _>("LastBuild")
                .repository()?
                .as_deref()
                .map(|value| decode_datetime("LastBuild", value))
                .transpose()?,
            build_succeeded: row
                .try_get::<Option<i64>, _>("Success")
                .repository()?
                .map(|value| value != 0),
            build_message: row.try_get("Message").repository()?,
        });
    }
    Ok(result)
}
