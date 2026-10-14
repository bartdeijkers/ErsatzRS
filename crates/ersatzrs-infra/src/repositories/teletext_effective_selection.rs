//! One read snapshot of explicit published overrides and unoverridden bases.
//! Eligibility/origin observations grant no invocation or native publication.

use super::{binding_from_row, channel_exists, selection_for_binding, Result, M, R};
use crate::db::Db;
use crate::repositories::{
    common::{decode_datetime, encode_datetime},
    playout::teletext_intervals,
    SqlxResultExt as _,
};
use chrono::{DateTime, Utc};
use ersatzrs_domain::{
    EntityId, TeletextEffectiveBindingSnapshot as Binding,
    TeletextEffectiveChannelSelectionSnapshot, MAX_TELETEXT_OVERRIDE_COMMANDS,
    MAX_TELETEXT_PAGE_RECORDS,
};
use sqlx::{Row as _, Transaction};
use std::collections::BTreeMap;

pub(super) async fn recheck_in_transaction(
    tx: &mut Transaction<'_, Db>,
    expected: &TeletextEffectiveChannelSelectionSnapshot,
    when: DateTime<Utc>,
) -> Result<M<TeletextEffectiveChannelSelectionSnapshot>> {
    if expected
        .valid_until
        .is_some_and(|boundary| when >= boundary)
    {
        return Ok(M::Rejected(R::Conflict));
    }
    match read_in_transaction(tx, expected.channel_id, when).await {
        Ok(M::Saved(current)) if current == *expected => Ok(M::Saved(current)),
        Ok(_) | Err(ersatzrs_core::repositories::RepositoryError::Conflict(_)) => {
            Ok(M::Rejected(R::Conflict))
        }
        Err(error) => Err(error),
    }
}

async fn next_transition(
    tx: &mut Transaction<'_, Db>,
    channel: EntityId,
    when: DateTime<Utc>,
) -> Result<Option<DateTime<Utc>>> {
    // SQL projects bounded date framing before MIN/host allocation. Invalid
    // future metadata cannot silently grant an unlimited validity window.
    let next: Option<String> = sqlx::query_scalar(
        r#"SELECT MIN("Boundary") FROM (
        SELECT CASE WHEN TYPEOF(i."Start") = 'text' AND LENGTH(CAST(i."Start" AS BLOB)) <= 32
            THEN i."Start" ELSE '!invalid' END AS "Boundary"
        FROM "PlayoutTeletextInterval" i JOIN "Playout" p ON p."Id" = i."PlayoutId"
        WHERE p."ChannelId" = ? AND i."Start" > ?
        UNION ALL
        SELECT CASE WHEN TYPEOF(i."Finish") = 'text' AND LENGTH(CAST(i."Finish" AS BLOB)) <= 32
            THEN i."Finish" ELSE '!invalid' END AS "Boundary"
        FROM "PlayoutTeletextInterval" i JOIN "Playout" p ON p."Id" = i."PlayoutId"
        WHERE p."ChannelId" = ? AND i."Finish" > ?)"#,
    )
    .bind(channel)
    .bind(encode_datetime(when))
    .bind(channel)
    .bind(encode_datetime(when))
    .fetch_one(&mut **tx)
    .await
    .repository()?;
    next.map(|date| decode_datetime("teletext effective transition", &date))
        .transpose()
}

pub(super) async fn read_in_transaction(
    tx: &mut Transaction<'_, Db>,
    channel: EntityId,
    when: DateTime<Utc>,
) -> Result<M<TeletextEffectiveChannelSelectionSnapshot>> {
    if !channel_exists(tx, channel).await? {
        return Ok(M::Rejected(R::MissingChannel));
    }
    // Explicit On/Off must win before any overridden base profile is validated.
    let published = teletext_intervals::active_published_in_transaction(tx, channel, when).await?;
    let mut bindings = published
        .into_iter()
        .map(|published| {
            let binding = Binding::Scripted(published);
            (binding.addon_id().to_owned(), binding)
        })
        .collect::<BTreeMap<_, _>>();
    let rows = sqlx::query(
        r#"SELECT "ChannelId", "ProfileId", "Enabled", "Revision",
        CASE WHEN TYPEOF("AddonId") = 'text' AND LENGTH(CAST("AddonId" AS BLOB)) BETWEEN 1 AND 256
            THEN "AddonId" END AS "AddonId"
        FROM "ChannelTeletextBinding" WHERE "ChannelId" = ? AND "Enabled" = 1
        ORDER BY "AddonId" LIMIT ?"#,
    )
    .bind(channel)
    .bind(
        i64::try_from(MAX_TELETEXT_OVERRIDE_COMMANDS + 1)
            .map_err(|_| super::decode_error("effective base membership bound"))?,
    )
    .fetch_all(&mut **tx)
    .await
    .repository()?;
    if rows.len() > MAX_TELETEXT_OVERRIDE_COMMANDS {
        return Ok(M::Rejected(R::BindingLimit));
    }
    let remaining = rows.iter().try_fold(0_usize, |count, row| -> Result<_> {
        let addon: String = row.try_get("AddonId").repository()?;
        Ok(count + usize::from(!bindings.contains_key(&addon)))
    })?;
    let producers = bindings
        .values()
        .filter(|binding| binding.enabled())
        .count();
    if producers.saturating_add(remaining) > MAX_TELETEXT_PAGE_RECORDS {
        return Ok(M::Rejected(R::BindingLimit));
    }
    for row in rows {
        let addon: String = row.try_get("AddonId").repository()?;
        if bindings.contains_key(&addon) {
            continue;
        }
        let base = binding_from_row(&row)?;
        match selection_for_binding(tx, &base).await? {
            M::Saved(selection) => {
                bindings.insert(addon, Binding::Base(selection));
            }
            M::Rejected(reason) => return Ok(M::Rejected(reason)),
        }
    }
    let valid_until = next_transition(tx, channel, when).await?;
    Ok(M::Saved(TeletextEffectiveChannelSelectionSnapshot {
        channel_id: channel,
        bindings: bindings.into_values().collect(),
        valid_until,
    }))
}
