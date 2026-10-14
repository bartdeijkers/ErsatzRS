//! Atomic Scripted override publication; no execution, packages or output promotion.

use super::{decode_datetime, encode_datetime, EntityId, RepositoryError, Result, Transaction};
use crate::db::Db;
use crate::repositories::{teletext_profiles, SqlxResultExt as _};
use chrono::{DateTime, Duration, Utc};
use ersatzrs_core::repositories::TeletextProfileMutation as M;
use ersatzrs_core::scheduling::{truncate_to_persisted_precision, SchedulingResult};
use ersatzrs_domain::{
    Playout, PlayoutScheduleKind, TeletextOverrideInterval, TeletextOverrideState,
    MAX_TELETEXT_OVERRIDE_COMMANDS, MAX_TELETEXT_OVERRIDE_SNAPSHOT_BYTES,
    MAX_TELETEXT_PAGE_RECORDS,
};
use sqlx::Row as _;
use std::collections::BTreeSet;

#[path = "teletext_interval_codec.rs"]
mod codec;

fn rejected() -> RepositoryError {
    RepositoryError::Conflict("Scripted teletext interval no longer matches its published timeline or candidate configuration".to_owned())
}

async fn recheck(tx: &mut Transaction<'_, Db>, state: &TeletextOverrideState) -> Result<()> {
    match state {
        TeletextOverrideState::Enabled(expected) => {
            if !matches!(teletext_profiles::candidate_in_transaction(
                tx, expected.channel_id, &expected.addon_id, expected.profile_id,
            ).await?, M::Saved(current) if current == *expected)
            {
                return Err(rejected());
            }
        }
        TeletextOverrideState::Off {
            channel_id,
            addon_id,
        } => {
            if !matches!(
                teletext_profiles::scripted_base_in_transaction(tx, *channel_id, addon_id).await?,
                M::Saved(_)
            ) {
                return Err(rejected());
            }
        }
    }
    Ok(())
}

async fn retain(
    tx: &mut Transaction<'_, Db>,
    playout: EntityId,
    result: &SchedulingResult,
    offset: Duration,
) -> Result<()> {
    if result.clear_items {
        if let Some(seam) = result.retain_through {
            sqlx::query(
                r#"DELETE FROM "PlayoutTeletextInterval" WHERE "PlayoutId" = ? AND "Start" >= ?"#,
            )
            .bind(playout)
            .bind(encode_datetime(seam))
            .execute(&mut **tx)
            .await
            .repository()?;
            // A retained interval crosses the seam independently of item rows.
            // Keep its original captured target only through the retained prefix.
            sqlx::query(r#"UPDATE "PlayoutTeletextInterval" SET "Finish" = ? WHERE "PlayoutId" = ? AND "Finish" > ?"#)
                .bind(encode_datetime(seam)).bind(playout).bind(encode_datetime(seam))
                .execute(&mut **tx).await.repository()?;
        } else {
            sqlx::query(r#"DELETE FROM "PlayoutTeletextInterval" WHERE "PlayoutId" = ?"#)
                .bind(playout)
                .execute(&mut **tx)
                .await
                .repository()?;
        }
    }
    if let Some(cutoff) = result
        .remove_before
        .or(result.retain_through.filter(|_| result.clear_items))
    {
        expire(tx, playout, cutoff - offset.max(Duration::zero())).await?;
    }
    Ok(())
}

pub(super) async fn expire(
    tx: &mut Transaction<'_, Db>,
    playout: EntityId,
    cutoff: DateTime<Utc>,
) -> Result<()> {
    sqlx::query(r#"DELETE FROM "PlayoutTeletextInterval" WHERE "PlayoutId" = ? AND "Finish" < ?"#)
        .bind(playout)
        .bind(encode_datetime(cutoff))
        .execute(&mut **tx)
        .await
        .repository()?;
    Ok(())
}

pub(super) async fn publish(
    tx: &mut Transaction<'_, Db>,
    playout: &Playout,
    result: &SchedulingResult,
    offset: Duration,
) -> Result<()> {
    let owner = sqlx::query(r#"SELECT "ChannelId", "ScheduleKind" FROM "Playout" WHERE "Id" = ?"#)
        .bind(playout.id)
        .fetch_one(&mut **tx)
        .await
        .repository()?;
    let channel: EntityId = owner.try_get("ChannelId").repository()?;
    if channel != playout.channel_id
        || result.teletext_intervals.len() > MAX_TELETEXT_OVERRIDE_COMMANDS
    {
        return Err(rejected());
    }
    if !result.teletext_intervals.is_empty()
        && (playout.schedule_kind != PlayoutScheduleKind::Scripted
            || owner.try_get::<i32, _>("ScheduleKind").repository()?
                != super::encode_enum(PlayoutScheduleKind::Scripted))
    {
        return Err(rejected());
    }
    let bytes = result
        .teletext_intervals
        .iter()
        .fold(0_usize, |size, interval| {
            size.saturating_add(interval.state.snapshot_bytes())
        });
    if bytes > MAX_TELETEXT_OVERRIDE_SNAPSHOT_BYTES {
        return Err(rejected());
    }
    // A candidate with no newly accepted items creates no interval; an old
    // retained tail cannot authorize commands from this scheduling result.
    let finish = result
        .added_items
        .last()
        .map(|item| truncate_to_persisted_precision(item.finish));
    retain(tx, playout.id, result, offset).await?;
    for interval in &result.teletext_intervals {
        let start = truncate_to_persisted_precision(interval.start);
        let end = truncate_to_persisted_precision(interval.finish);
        if start >= end
            || interval.state.channel_id() != channel
            || interval.state.addon_id().is_empty()
            || interval.state.addon_id().len() > 256
            || finish.is_none_or(|finish| end > finish)
            || result
                .retain_through
                .is_some_and(|seam| start < truncate_to_persisted_precision(seam))
        {
            return Err(rejected());
        }
        recheck(tx, &interval.state).await?;
        let overlap: bool = sqlx::query_scalar(
            r#"SELECT EXISTS(SELECT 1 FROM "PlayoutTeletextInterval"
            WHERE "PlayoutId" = ? AND "AddonId" = ? AND "Start" < ? AND "Finish" > ?)"#,
        )
        .bind(playout.id)
        .bind(interval.state.addon_id())
        .bind(encode_datetime(end))
        .bind(encode_datetime(start))
        .fetch_one(&mut **tx)
        .await
        .repository()?;
        if overlap {
            return Err(rejected());
        }
        let (state, snapshot) = match &interval.state {
            TeletextOverrideState::Enabled(candidate) => (1_i32, Some(codec::encode(candidate)?)),
            TeletextOverrideState::Off { .. } => (0_i32, None),
        };
        sqlx::query(r#"INSERT INTO "PlayoutTeletextInterval" ("PlayoutId", "AddonId", "Start", "Finish", "State", "CandidateSnapshot") VALUES (?, ?, ?, ?, ?, ?)"#)
            .bind(playout.id).bind(interval.state.addon_id()).bind(encode_datetime(start)).bind(encode_datetime(end))
            .bind(state).bind(snapshot).execute(&mut **tx).await.repository()?;
    }
    Ok(())
}

/// Validate every active override in the caller's same read transaction.
/// Errors preserve the distinction between unavailable and expired/absent.
pub(in crate::repositories) async fn active_in_transaction(
    tx: &mut Transaction<'_, Db>,
    channel_id: EntityId,
    when: DateTime<Utc>,
) -> Result<Vec<TeletextOverrideInterval>> {
    // Admit aggregate serialized material before fetching any snapshot body.
    let budget = sqlx::query(
        r#"SELECT COUNT(*) AS "Records",
        COALESCE(SUM(LENGTH(CAST(i."CandidateSnapshot" AS BLOB))), 0) AS "Bytes"
        FROM "PlayoutTeletextInterval" i JOIN "Playout" p ON p."Id" = i."PlayoutId"
        WHERE p."ChannelId" = ? AND i."Start" <= ? AND i."Finish" > ?"#,
    )
    .bind(channel_id)
    .bind(encode_datetime(when))
    .bind(encode_datetime(when))
    .fetch_one(&mut **tx)
    .await
    .repository()?;
    let records = budget.try_get::<i64, _>("Records").repository()?;
    let bytes = budget.try_get::<i64, _>("Bytes").repository()?;
    if !usize::try_from(records).is_ok_and(|size| size <= MAX_TELETEXT_PAGE_RECORDS)
        || !usize::try_from(bytes).is_ok_and(|size| size <= codec::MAX_BYTES)
    {
        return Err(rejected());
    }
    let rows = sqlx::query(
        r#"SELECT
        CASE WHEN TYPEOF(i."AddonId") = 'text' AND
            LENGTH(CAST(i."AddonId" AS BLOB)) BETWEEN 1 AND 256 THEN i."AddonId" END AS "AddonId",
        CASE WHEN TYPEOF(i."Start") = 'text' AND
            LENGTH(CAST(i."Start" AS BLOB)) <= 32 THEN i."Start" END AS "Start",
        CASE WHEN TYPEOF(i."Finish") = 'text' AND
            LENGTH(CAST(i."Finish" AS BLOB)) <= 32 THEN i."Finish" END AS "Finish",
        i."State", i."CandidateSnapshot" FROM "PlayoutTeletextInterval" i
        JOIN "Playout" p ON p."Id" = i."PlayoutId"
        WHERE p."ChannelId" = ? AND i."Start" <= ? AND i."Finish" > ?
        ORDER BY i."AddonId", i."Start" LIMIT ?"#,
    )
    .bind(channel_id)
    .bind(encode_datetime(when))
    .bind(encode_datetime(when))
    .bind(i64::try_from(MAX_TELETEXT_PAGE_RECORDS + 1).map_err(|_| rejected())?)
    .fetch_all(&mut **tx)
    .await
    .repository()?;
    if rows.len() > MAX_TELETEXT_PAGE_RECORDS {
        return Err(rejected());
    }
    let mut addons = BTreeSet::new();
    let mut result = Vec::with_capacity(rows.len());
    let mut snapshot_bytes = 0_usize;
    for row in rows {
        let addon: String = row.try_get("AddonId").repository()?;
        if !addons.insert(addon.clone()) {
            return Err(rejected());
        }
        let start = decode_datetime(
            "published teletext start",
            &row.try_get::<String, _>("Start").repository()?,
        )?;
        let finish = decode_datetime(
            "published teletext finish",
            &row.try_get::<String, _>("Finish").repository()?,
        )?;
        if start > when || finish <= when || start >= finish {
            return Err(rejected());
        }
        let snapshot: Option<String> = row.try_get("CandidateSnapshot").repository()?;
        let state = match (row.try_get::<i32, _>("State").repository()?, snapshot) {
            (0, None) => TeletextOverrideState::Off {
                channel_id,
                addon_id: addon,
            },
            (1, Some(snapshot)) => {
                let candidate = codec::decode(&snapshot)?;
                if candidate.channel_id != channel_id || candidate.addon_id != addon {
                    return Err(rejected());
                }
                TeletextOverrideState::Enabled(candidate)
            }
            _ => return Err(rejected()),
        };
        snapshot_bytes = snapshot_bytes.saturating_add(state.snapshot_bytes());
        if snapshot_bytes > MAX_TELETEXT_OVERRIDE_SNAPSHOT_BYTES {
            return Err(rejected());
        }
        recheck(tx, &state).await?;
        result.push(TeletextOverrideInterval {
            start,
            finish,
            state,
        });
    }
    Ok(result)
}
