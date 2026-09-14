//! Read-only, redacted feedback for the existing durable preparation jobs.

use std::collections::{BTreeMap, BTreeSet};

use ersatzrs_core::repositories::{
    AddonMediaListPreparationFailure, AddonMediaListPreparationFailureReason, Result,
};
use sqlx::Row as _;

use crate::{
    addon_media_list_item_jobs::ItemOperationCompletion, db::DbPool,
    repositories::SqlxResultExt as _,
};

/// Summarize the latest attempt per still-required item/operation. Earlier
/// revisions explain history but never contribute to the current failure count.
pub(crate) async fn preparation_failures(
    pool: &DbPool,
    list_id: i64,
    saved_revision: i32,
    missing: &BTreeSet<(i64, i32)>,
) -> Result<(u64, Vec<AddonMediaListPreparationFailure>)> {
    if missing.is_empty() {
        return Ok((0, Vec::new()));
    }
    let rows = sqlx::query(
        r#"SELECT "job"."AddonMediaListItemId", "job"."Kind",
                  "job"."SavedDraftRevision", "job"."OutcomeJson"
           FROM "AddonMediaListItemOperationJob" AS "job"
           JOIN (
               SELECT MAX("Id") AS "Id" FROM "AddonMediaListItemOperationJob"
               WHERE "AddonMediaListId"=? AND "Kind" IN (1,2,3)
               GROUP BY "AddonMediaListItemId", "Kind"
           ) AS "latest" ON "latest"."Id"="job"."Id"
           WHERE "job"."State"=3"#,
    )
    .bind(list_id)
    .fetch_all(pool)
    .await
    .repository()?;
    let mut grouped = BTreeMap::new();
    let mut current_failed = 0;
    for row in rows {
        let item_id: i64 = row.try_get("AddonMediaListItemId").repository()?;
        let kind: i32 = row.try_get("Kind").repository()?;
        if !missing.contains(&(item_id, kind)) {
            continue;
        }
        let revision: i32 = row.try_get("SavedDraftRevision").repository()?;
        let json: Option<String> = row.try_get("OutcomeJson").repository()?;
        let reason = failure_reason(json.as_deref());
        *grouped.entry((revision, reason)).or_insert(0_u64) += 1;
        current_failed += u64::from(revision == saved_revision);
    }
    Ok((
        current_failed,
        grouped
            .into_iter()
            .rev()
            .map(
                |((saved_revision, reason), count)| AddonMediaListPreparationFailure {
                    saved_revision,
                    reason,
                    count,
                },
            )
            .collect(),
    ))
}

fn failure_reason(json: Option<&str>) -> AddonMediaListPreparationFailureReason {
    match json
        .filter(|value| value.len() <= 65_536)
        .and_then(|value| serde_json::from_str::<ItemOperationCompletion>(value).ok())
    {
        Some(ItemOperationCompletion::Failed(reason)) => reason,
        _ => AddonMediaListPreparationFailureReason::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preparation_failure_projection_preserves_closed_categories_only() {
        for reason in [
            AddonMediaListPreparationFailureReason::InvalidRequest,
            AddonMediaListPreparationFailureReason::InputChanged,
            AddonMediaListPreparationFailureReason::Failed,
            AddonMediaListPreparationFailureReason::StorageFull,
        ] {
            let json = serde_json::to_string(&ItemOperationCompletion::Failed(reason)).unwrap();
            assert_eq!(failure_reason(Some(&json)), reason);
        }
        for json in [
            None,
            Some("not a safe diagnostic"),
            Some(r#"{"Failed":"unknown"}"#),
        ] {
            assert_eq!(
                failure_reason(json),
                AddonMediaListPreparationFailureReason::Failed
            );
        }
        assert_eq!(
            failure_reason(Some(&"x".repeat(65_537))),
            AddonMediaListPreparationFailureReason::Failed
        );
    }
}

/// Project current requirements and existing durable jobs without scheduling work.
pub(crate) async fn preparation_summary(
    pool: &DbPool,
    list_id: i64,
    revision: i32,
    tasks: &BTreeMap<(i64, i32), bool>,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<ersatzrs_domain::AddonMediaListPreparationSummary> {
    use crate::repositories::common::decode_datetime;
    let mut summary = ersatzrs_domain::AddonMediaListPreparationSummary::default();
    if tasks.is_empty() {
        return Ok(summary);
    }
    summary.affected_items = tasks
        .keys()
        .map(|(id, _)| id)
        .collect::<BTreeSet<_>>()
        .len() as u64;
    for ((_, kind), complete) in tasks {
        match kind {
            1 => summary.download_tasks += 1,
            2 => summary.interval_tasks += 1,
            3 => summary.subtitle_tasks += 1,
            _ => continue,
        }
        summary.completed_tasks += u64::from(*complete);
    }
    let rows = sqlx::query(r#"SELECT "job"."AddonMediaListItemId", "job"."Kind", "job"."State",
        "job"."NextEligibleAt", "job"."LeaseExpiresAt", "job"."CreatedAt", "job"."UpdatedAt", "job"."CompletedAt"
        FROM "AddonMediaListItemOperationJob" AS "job"
        JOIN (SELECT MAX("Id") AS "Id" FROM "AddonMediaListItemOperationJob"
              WHERE "AddonMediaListId"=? AND "SavedDraftRevision"=? AND "Kind" IN (1,2,3)
              GROUP BY "AddonMediaListItemId", "Kind") AS "latest" ON "latest"."Id"="job"."Id""#)
        .bind(list_id).bind(revision).fetch_all(pool).await.repository()?;
    let mut admitted = BTreeSet::new();
    let mut started = None;
    let mut activity = None;
    let mut completed = None;
    let mut progress = None;
    let mut retry = None;
    for row in rows {
        let key = (
            row.try_get::<i64, _>("AddonMediaListItemId").repository()?,
            row.try_get::<i32, _>("Kind").repository()?,
        );
        let Some(complete) = tasks.get(&key) else {
            continue;
        };
        let created = decode_datetime(
            "preparation creation",
            &row.try_get::<String, _>("CreatedAt").repository()?,
        )?;
        let updated = decode_datetime(
            "preparation activity",
            &row.try_get::<String, _>("UpdatedAt").repository()?,
        )?;
        started = Some(
            started.map_or(created, |value: chrono::DateTime<chrono::Utc>| {
                value.min(created)
            }),
        );
        activity = Some(
            activity.map_or(updated, |value: chrono::DateTime<chrono::Utc>| {
                value.max(updated)
            }),
        );
        if let Some(value) = row
            .try_get::<Option<String>, _>("CompletedAt")
            .repository()?
        {
            let value = decode_datetime("preparation completion", &value)?;
            if *complete && row.try_get::<i32, _>("State").repository()? == 2 {
                progress = Some(
                    progress.map_or(value, |previous: chrono::DateTime<chrono::Utc>| {
                        previous.max(value)
                    }),
                );
            }
            completed = Some(
                completed.map_or(value, |previous: chrono::DateTime<chrono::Utc>| {
                    previous.max(value)
                }),
            );
        }
        if *complete {
            continue;
        }
        if !count_pending_job(&row, now, &mut summary, &mut retry)? {
            continue;
        }
        admitted.insert(key);
    }
    summary.unqueued_tasks = tasks
        .iter()
        .filter(|(key, complete)| !**complete && !admitted.contains(key))
        .count() as u64;
    summary.started_at = started.map(|value| value.to_rfc3339());
    summary.last_activity_at = activity.map(|value| value.to_rfc3339());
    summary.last_progress_at = progress.map(|value| value.to_rfc3339());
    if summary.completed_tasks == tasks.len() as u64 {
        summary.completed_at = completed.map(|value| value.to_rfc3339());
    }
    summary.retry_at = retry.map(|value| value.to_rfc3339());
    Ok(summary)
}

fn count_pending_job(
    row: &sqlx::sqlite::SqliteRow,
    now: chrono::DateTime<chrono::Utc>,
    summary: &mut ersatzrs_domain::AddonMediaListPreparationSummary,
    retry: &mut Option<chrono::DateTime<chrono::Utc>>,
) -> Result<bool> {
    use crate::repositories::common::decode_datetime;
    let eligible = row
        .try_get::<Option<String>, _>("NextEligibleAt")
        .repository()?
        .map(|value| decode_datetime("preparation eligibility", &value))
        .transpose()?;
    match row.try_get::<i32, _>("State").repository()? {
        0 if eligible.is_some_and(|at| at > now) => {
            summary.waiting_tasks += 1;
            if let Some(at) = eligible {
                *retry = Some(retry.map_or(at, |previous| previous.min(at)));
            }
        }
        0 => summary.queued_tasks += 1,
        1 => {
            let lease = row
                .try_get::<Option<String>, _>("LeaseExpiresAt")
                .repository()?
                .map(|value| decode_datetime("preparation lease", &value))
                .transpose()?;
            if lease.is_some_and(|at| at > now) {
                summary.running_tasks += 1;
            } else {
                summary.waiting_tasks += 1;
            }
        }
        3 => summary.failed_tasks += 1,
        _ => return Ok(false),
    }
    Ok(true)
}

#[cfg(test)]
mod summary_tests {
    use super::*;
    use sqlx::Executor as _;

    #[tokio::test]
    async fn overlapping_preparation_counts_tasks_items_and_current_durable_states() {
        let pool = crate::db::init("sqlite::memory:").await.unwrap();
        pool.execute(r#"INSERT INTO "AddonInstallation" ("AddonId","Version","Enabled","PackageSha256","ContentSha256","ManifestJson","InstallPath","InstalledAt","UpdatedAt")
            VALUES ('org.example.preparation','1',1,'package','content','{}','fixture','2026-09-01 00:00:00','2026-09-01 00:00:00')"#).await.unwrap();
        pool.execute(r#"INSERT INTO "AddonMediaList" ("Id","AddonId","ProviderId","SourceUrl","Name","DraftRevision")
            VALUES (1,'org.example.preparation','fixture','https://example.invalid/list','Synthetic list',7)"#).await.unwrap();
        // A 166-entry list: 165 known finite entries plus one unresolved entry.
        // The unresolved entry has no invented preparation requirement.
        for id in 1..=166 {
            sqlx::query(r#"INSERT INTO "AddonMediaListItem" ("Id","AddonMediaListId","ProviderId","DisplayTitle","Kind","Rank","Title","Availability") VALUES (?,1,?,'Fixture',4,?,'Fixture',?)"#)
                .bind(id).bind(id.to_string()).bind(id).bind(if id == 166 {2} else {0}).execute(&pool).await.unwrap();
        }
        let mut tasks: BTreeMap<_, _> = (1..=165).map(|id| ((id, 2), true)).collect();
        tasks.insert((1, 2), false);
        tasks.insert((1, 3), false);
        tasks.insert((2, 1), false);
        let now = chrono::DateTime::parse_from_rfc3339("2026-09-14T12:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        for (item, kind, state, eligible, lease) in [
            (1, 2, 1, None, Some("2026-09-14 12:05:00")),
            (1, 3, 0, Some("2026-09-14 12:01:00"), None),
            (2, 1, 3, None, None),
        ] {
            sqlx::query(r#"INSERT INTO "AddonMediaListItemOperationJob" ("AddonMediaListId","AddonMediaListItemId","Kind","SavedDraftRevision","InputJson","ContextFingerprint","State","NextEligibleAt","LeaseExpiresAt","CreatedAt","UpdatedAt") VALUES (1,?,?,7,'{}','fixture',?,?,?,'2026-09-14 11:59:00','2026-09-14 12:00:00')"#)
                .bind(item).bind(kind).bind(state).bind(eligible).bind(lease).execute(&pool).await.unwrap();
        }
        let summary = preparation_summary(&pool, 1, 7, &tasks, now).await.unwrap();
        assert_eq!(summary.affected_items, 165);
        assert_eq!(
            (
                summary.interval_tasks,
                summary.download_tasks,
                summary.subtitle_tasks
            ),
            (165, 1, 1)
        );
        assert_eq!(
            (
                summary.completed_tasks,
                summary.running_tasks,
                summary.waiting_tasks,
                summary.failed_tasks
            ),
            (164, 1, 1, 1)
        );
        assert_eq!(summary.unqueued_tasks, 0);
        assert_eq!(
            summary.retry_at.as_deref(),
            Some("2026-09-14T12:01:00+00:00")
        );
        assert!(
            summary.last_progress_at.is_none(),
            "heartbeats are not progress"
        );
        pool.execute(r#"INSERT INTO "AddonMediaListItemOperationJob" ("AddonMediaListId","AddonMediaListItemId","Kind","SavedDraftRevision","InputJson","ContextFingerprint","State","CreatedAt","UpdatedAt","CompletedAt") VALUES (1,3,2,7,'{}','fixture',2,'2026-09-14 11:58:00','2026-09-14 11:59:30','2026-09-14 11:59:30')"#).await.unwrap();
        pool.execute(
            r#"UPDATE "AddonMediaListItemOperationJob" SET "NextEligibleAt"=NULL WHERE "Kind"=3"#,
        )
        .await
        .unwrap();
        let progressed = preparation_summary(&pool, 1, 7, &tasks, now).await.unwrap();
        assert_eq!(progressed.queued_tasks, 1);
        assert_eq!(progressed.waiting_tasks, 0);
        assert_eq!(
            progressed.last_progress_at.as_deref(),
            Some("2026-09-14T11:59:30+00:00")
        );
        // Old jobs never masquerade as active work for a newer revision.
        let newer = preparation_summary(&pool, 1, 8, &tasks, now).await.unwrap();
        assert_eq!(newer.running_tasks, 0);
        assert_eq!(newer.failed_tasks, 0);
        assert_eq!(newer.unqueued_tasks, 3);
    }
}
