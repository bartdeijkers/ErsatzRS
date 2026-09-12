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
