//! ADR 0094: publish saved changes through the existing fenced coordinator.
//! The existing import worker advances one bounded list-owned assessment;
//! actual publication continues through the managed-library coordinator.

use chrono::{DateTime, Utc};
use ersatzrs_core::repositories::{
    AddonMediaListAutoPublicationHold, AddonMediaListAutoPublicationHoldReason as Reason,
    AddonMediaListPublicationBlockerKind, AddonMediaListRepository, RepositoryError, Result,
};
use ersatzrs_domain::{AddonMediaListPublicationEnqueueTrigger, EntityId};
use sqlx::{Row, Transaction};

use crate::db::{Db, DbPool};
use crate::repositories::addon_media_list_auto_eligibility::{
    assess_auto_publication_in_tx, AutoPublicationAssessment,
};
use crate::repositories::common::{decode_datetime, encode_datetime};
use crate::repositories::managed_library_operation::enqueue_publication_in_tx;
use crate::repositories::{SqliteAddonMediaListRepository, SqlxResultExt as _};

/// Persist authorization only at scheduled import admission, never for manual work.
pub(crate) async fn admit_in_tx(
    tx: &mut Transaction<'_, Db>,
    list_id: EntityId,
    import_id: Option<EntityId>,
) -> Result<()> {
    let enabled: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM "AddonMediaList" WHERE "Id" = ? AND "RefreshPublicationPolicy" = 1)"#,
    ).bind(list_id).fetch_one(&mut **tx).await.repository()?;
    if !enabled {
        return Ok(());
    }
    let initial_hold = initial_hold_in_tx(tx, list_id).await?;
    sqlx::query(
        r#"INSERT INTO "AddonMediaListAutoPublication" (
               "AddonMediaListId", "ImportJobId", "DraftRevision", "PublishedRevision",
               "State", "RecordedAt", "InitialHoldReason"
           ) SELECT "Id", ?, "DraftRevision", "PublishedRevision", 'importing', ?, ?
             FROM "AddonMediaList" WHERE "Id" = ? AND "RefreshPublicationPolicy" = 1
           ON CONFLICT ("AddonMediaListId") DO UPDATE SET
               "ImportJobId" = excluded."ImportJobId",
               "DraftRevision" = excluded."DraftRevision",
               "PublishedRevision" = excluded."PublishedRevision",
               "State" = 'importing', "OperationId" = NULL,
               "InitialHoldReason" = excluded."InitialHoldReason", "LastAssessedAt" = NULL"#,
    )
    .bind(import_id)
    .bind(encode_datetime(Utc::now()))
    .bind(initial_hold)
    .bind(list_id)
    .execute(&mut **tx)
    .await
    .repository()?;
    Ok(())
}

async fn initial_hold_in_tx(
    tx: &mut Transaction<'_, Db>,
    list_id: EntityId,
) -> Result<Option<String>> {
    let reason = match assess_auto_publication_in_tx(tx, list_id).await? {
        AutoPublicationAssessment::NoChanges => None,
        AutoPublicationAssessment::Eligible => Some(Reason::ExistingChanges),
        AutoPublicationAssessment::Hold(reason) => Some(reason),
    };
    reason
        .map(|reason| {
            serde_json::to_string(&reason).map_err(|error| RepositoryError::Decode {
                kind: "automatic publication initial hold",
                value: error.to_string(),
            })
        })
        .transpose()
}

/// Manual refresh takes ownership even when it coalesces into an existing import.
pub(crate) async fn manual_refresh_in_tx(
    tx: &mut Transaction<'_, Db>,
    list_id: EntityId,
) -> Result<()> {
    let pending: bool = sqlx::query_scalar(
        r#"SELECT EXISTS (SELECT 1 FROM "AddonMediaListAutoPublication"
        WHERE "AddonMediaListId" = ? AND "State" IN ('importing', 'ready', 'preparing'))"#,
    )
    .bind(list_id)
    .fetch_one(&mut **tx)
    .await
    .repository()?;
    if pending {
        hold_in_tx(tx, list_id, Reason::ConcurrentChange).await?;
    }
    // Only a manual refresh rechecks subtitles; earlier outcomes stop being reusable.
    sqlx::query(r#"UPDATE "AddonMediaList" SET "SubtitleRecheckRequestedAt" = ? WHERE "Id" = ?"#)
        .bind(encode_datetime(Utc::now()))
        .bind(list_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    Ok(())
}

pub(crate) async fn manual_refresh(pool: &DbPool, addon_id: &str, source_url: &str) -> Result<()> {
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    let list_ids: Vec<EntityId> = sqlx::query_scalar(
        r#"SELECT "Id" FROM "AddonMediaList" WHERE "AddonId" = ? AND "SourceUrl" = ?"#,
    )
    .bind(addon_id)
    .bind(source_url)
    .fetch_all(&mut *tx)
    .await
    .repository()?;
    for list_id in list_ids {
        manual_refresh_in_tx(&mut tx, list_id).await?;
    }
    tx.commit().await.repository()?;
    Ok(())
}

/// A scheduled legacy invocation can advance exactly one saved revision.
pub(crate) struct LegacyAutoPublicationBaseline {
    list_id: EntityId,
    draft_revision: i32,
    published_revision: i32,
}

pub(crate) async fn legacy_baseline(
    pool: &DbPool,
    addon_id: &str,
    source_url: &str,
) -> Result<Option<LegacyAutoPublicationBaseline>> {
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    let row = sqlx::query(
        r#"SELECT "Id", "DraftRevision", "PublishedRevision" FROM "AddonMediaList"
        WHERE "AddonId" = ? AND "SourceUrl" = ? AND "RefreshPublicationPolicy" = 1"#,
    )
    .bind(addon_id)
    .bind(source_url)
    .fetch_optional(&mut *tx)
    .await
    .repository()?;
    let Some(row) = row else {
        return Ok(None);
    };
    let baseline = LegacyAutoPublicationBaseline {
        list_id: row.try_get("Id").repository()?,
        draft_revision: row.try_get("DraftRevision").repository()?,
        published_revision: row.try_get("PublishedRevision").repository()?,
    };
    admit_in_tx(&mut tx, baseline.list_id, None).await?;
    tx.commit().await.repository()?;
    Ok(Some(baseline))
}

pub(crate) async fn legacy_ready(
    pool: &DbPool,
    list_id: EntityId,
    baseline: LegacyAutoPublicationBaseline,
) -> Result<()> {
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    let updated = sqlx::query(
        r#"UPDATE "AddonMediaListAutoPublication"
        SET "State" = 'ready', "DraftRevision" = "DraftRevision" + 1
        WHERE "AddonMediaListId" = ? AND "AddonMediaListId" = ?
          AND "State" = 'importing' AND "ImportJobId" IS NULL
          AND "DraftRevision" = ? AND "PublishedRevision" = ?
          AND EXISTS (SELECT 1 FROM "AddonMediaList" WHERE "Id" = ?
            AND "DraftRevision" = ? AND "PublishedRevision" = ?)"#,
    )
    .bind(list_id)
    .bind(baseline.list_id)
    .bind(baseline.draft_revision)
    .bind(baseline.published_revision)
    .bind(list_id)
    .bind(baseline.draft_revision.checked_add(1))
    .bind(baseline.published_revision)
    .execute(&mut *tx)
    .await
    .repository()?;
    if updated.rows_affected() == 0 {
        let still_owned: bool = sqlx::query_scalar(
            r#"SELECT EXISTS (SELECT 1 FROM "AddonMediaListAutoPublication"
            WHERE "AddonMediaListId" = ? AND "ImportJobId" IS NULL AND "State" = 'importing'
              AND "DraftRevision" = ? AND "PublishedRevision" = ?)"#,
        )
        .bind(baseline.list_id)
        .bind(baseline.draft_revision)
        .bind(baseline.published_revision)
        .fetch_one(&mut *tx)
        .await
        .repository()?;
        if still_owned {
            hold_in_tx(&mut tx, baseline.list_id, Reason::ConcurrentChange).await?;
        }
    }
    tx.commit().await.repository()?;
    Ok(())
}

/// Advance only revisions written by this import, preserving evidence of an
/// intervening operator edit instead of adopting its newer draft.
pub(crate) async fn advance_import_revision_in_tx(
    tx: &mut Transaction<'_, Db>,
    list_id: EntityId,
    import_id: EntityId,
    increment: i32,
) -> Result<()> {
    sqlx::query(
        r#"UPDATE "AddonMediaListAutoPublication" SET "DraftRevision" = "DraftRevision" + ?
           WHERE "AddonMediaListId" = ? AND "ImportJobId" = ? AND "State" = 'importing'
             AND "DraftRevision" + ? = (
                 SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id" = ?
             )"#,
    )
    .bind(increment)
    .bind(list_id)
    .bind(import_id)
    .bind(increment)
    .bind(list_id)
    .execute(&mut **tx)
    .await
    .repository()?;
    Ok(())
}

pub(crate) async fn ready_in_tx(tx: &mut Transaction<'_, Db>, import_id: EntityId) -> Result<()> {
    sqlx::query(
        r#"UPDATE "AddonMediaListAutoPublication" SET "State" = 'ready'
           WHERE "ImportJobId" = ? AND "State" = 'importing'"#,
    )
    .bind(import_id)
    .execute(&mut **tx)
    .await
    .repository()?;
    Ok(())
}

async fn hold_in_tx(tx: &mut Transaction<'_, Db>, list_id: EntityId, reason: Reason) -> Result<()> {
    let reason = serde_json::to_string(&reason).map_err(|error| RepositoryError::Decode {
        kind: "automatic publication reason",
        value: error.to_string(),
    })?;
    let automatic: bool = sqlx::query_scalar(
        r#"SELECT "RefreshPublicationPolicy"=2 FROM "AddonMediaList" WHERE "Id"=?"#,
    )
    .bind(list_id)
    .fetch_one(&mut **tx)
    .await
    .repository()?;
    if automatic {
        let attempts: i32 = sqlx::query_scalar(r#"SELECT "AttemptCount" FROM "AddonMediaListAutoPublication" WHERE "AddonMediaListId"=?"#)
            .bind(list_id).fetch_one(&mut **tx).await.repository()?;
        let retry = attempts < 4;
        let delay = [5, 30, 120, 600][usize::try_from(attempts.clamp(0, 3)).unwrap_or(3)];
        sqlx::query(
            r#"UPDATE "AddonMediaListAutoPublication" SET "State"=?,"Reason"=?,
            "AttemptCount"="AttemptCount"+1,"NextEligibleAt"=?,"OperationId"=NULL
            WHERE "AddonMediaListId"=?"#,
        )
        .bind(if retry { "ready" } else { "held" })
        .bind(&reason)
        .bind(retry.then(|| encode_datetime(Utc::now() + chrono::Duration::seconds(delay))))
        .bind(list_id)
        .execute(&mut **tx)
        .await
        .repository()?;
        return Ok(());
    }
    sqlx::query(
        r#"UPDATE "AddonMediaListAutoPublication"
           SET "State" = 'held', "RecordedAt" = CASE
                   WHEN "Reason" = ? THEN "RecordedAt" ELSE ? END,
               "Reason" = ? WHERE "AddonMediaListId" = ?"#,
    )
    .bind(&reason)
    .bind(encode_datetime(Utc::now()))
    .bind(&reason)
    .bind(list_id)
    .execute(&mut **tx)
    .await
    .repository()?;
    Ok(())
}

/// Current unresolved review notice; publication resolves matching prior holds.
pub(crate) async fn load_hold(
    pool: &DbPool,
    list_id: EntityId,
) -> Result<Option<AddonMediaListAutoPublicationHold>> {
    let row = sqlx::query(
        r#"SELECT "auto"."Reason", "auto"."DraftRevision", "auto"."PublishedRevision", "auto"."RecordedAt"
           FROM "AddonMediaListAutoPublication" AS "auto"
           JOIN "AddonMediaList" AS "list" ON "list"."Id" = "auto"."AddonMediaListId"
           WHERE "auto"."AddonMediaListId" = ? AND "auto"."State" = 'held'
             AND "list"."RefreshPublicationPolicy" IN (1, 2)"#,
    ).bind(list_id).fetch_optional(pool).await.repository()?;
    row.map(|row| {
        let reason: String = row.try_get("Reason").repository()?;
        Ok(AddonMediaListAutoPublicationHold {
            reason: serde_json::from_str(&reason).map_err(|error| RepositoryError::Decode {
                kind: "automatic publication reason",
                value: error.to_string(),
            })?,
            draft_revision: row.try_get("DraftRevision").repository()?,
            published_revision: row.try_get("PublishedRevision").repository()?,
            recorded_at: decode_datetime(
                "automatic publication notice",
                &row.try_get::<String, _>("RecordedAt").repository()?,
            )?,
        })
    })
    .transpose()
}

/// Discover saved changes independently of refresh admission. This also repairs
/// pre-upgrade drafts and survives a crash between a save and worker admission.
async fn admit_saved_changes(pool: &DbPool, now: DateTime<Utc>) -> Result<()> {
    let now = encode_datetime(now);
    let admitted: Option<i64> = sqlx::query_scalar(r#"INSERT INTO "AddonMediaListAutoPublication" (
        "AddonMediaListId", "DraftRevision", "PublishedRevision", "State",
        "RecordedAt", "FirstPendingAt")
        SELECT "list"."Id", "list"."DraftRevision", "list"."PublishedRevision",
               'ready', ?, ?
        FROM "AddonMediaList" AS "list"
        JOIN "AddonMediaListLibrary" AS "library" ON "library"."AddonMediaListId" = "list"."Id"
        LEFT JOIN "AddonMediaListAutoPublication" AS "pending" ON "pending"."AddonMediaListId" = "list"."Id"
        WHERE "list"."RefreshPublicationPolicy" = 2 AND "list"."ReviewState" <> 3
          AND NOT EXISTS (SELECT 1 FROM "AddonMediaListSelection" AS "selection" WHERE "selection"."AddonMediaListId"="list"."Id" AND "selection"."ReviewRequired"=1)
          AND ("list"."PublicationJournalJson" IS NULL OR TRIM("list"."PublicationJournalJson") = '')
          AND ("pending"."AddonMediaListId" IS NULL OR "pending"."DraftRevision" <> "list"."DraftRevision"
               OR "pending"."PublishedRevision" <> "list"."PublishedRevision")
          AND NOT EXISTS (SELECT 1 FROM "AddonMediaListImportJob" AS "import"
              WHERE "import"."AddonMediaListId" = "list"."Id" AND "import"."State" IN (0, 1, 2, 4, 5))
          AND NOT EXISTS (SELECT 1 FROM "AddonMediaListPublicationJob" AS "job"
              JOIN "ManagedLibraryOperation" AS "operation" ON "operation"."Id" = "job"."OperationId"
              WHERE "job"."AddonMediaListId" = "list"."Id" AND "operation"."State" IN (0, 1, 2))
        ORDER BY "list"."Id" LIMIT 1
        ON CONFLICT ("AddonMediaListId") DO UPDATE SET
          "DraftRevision" = excluded."DraftRevision", "PublishedRevision" = excluded."PublishedRevision",
          "State" = 'ready', "Reason" = NULL, "InitialHoldReason" = NULL,
          "OperationId" = NULL, "NextEligibleAt" = NULL, "AttemptCount" = 0,
          "FirstPendingAt" = CASE WHEN "AddonMediaListAutoPublication"."State" IN ('ready', 'preparing')
            THEN COALESCE("AddonMediaListAutoPublication"."FirstPendingAt", excluded."FirstPendingAt")
            ELSE excluded."FirstPendingAt" END
        RETURNING "AddonMediaListId""#)
        .bind(&now).bind(&now).fetch_optional(pool).await.repository()?;
    if let Some(list_id) = admitted {
        crate::addon_list_selection::queue_missing_facts(pool, list_id).await?;
        crate::addon_media_list_import::AddonMediaListImportCoordinator::new(pool.clone())
            .authorize_after_publish(list_id)
            .await?;
    }
    Ok(())
}

/// Evaluate at most one completed scheduled import and durably admit its exact
/// revision. Provider work stays in the existing detail/import coordinators.
///
/// # Errors
/// Returns database/validation errors without losing the pending assessment.
#[allow(clippy::too_many_lines)] // Keep the fenced assessment and its transaction exits together.
pub async fn process_one(pool: &DbPool, now: DateTime<Utc>) -> Result<bool> {
    crate::addon_list_selection::reassess_due(pool, now).await?;
    admit_saved_changes(pool, now).await?;
    let row = sqlx::query(
        r#"SELECT "AddonMediaListId" FROM "AddonMediaListAutoPublication"
           WHERE NOT EXISTS (SELECT 1 FROM "AddonMediaListSelection" AS "selection" WHERE "selection"."AddonMediaListId"="AddonMediaListAutoPublication"."AddonMediaListId" AND "selection"."ReviewRequired"=1) AND ("State" IN ('ready', 'preparing')
              OR ("State" = 'queued' AND EXISTS (SELECT 1 FROM "ManagedLibraryOperation"
                  WHERE "Id" = "AddonMediaListAutoPublication"."OperationId" AND ("State" = 4 OR
                      ("State" = 3 AND EXISTS (SELECT 1 FROM "AddonMediaListPublicationJob" AS "job"
                          WHERE "job"."OperationId"="ManagedLibraryOperation"."Id" AND "job"."Outcome"=1))))))
             AND ("NextEligibleAt" IS NULL OR "NextEligibleAt" <= ?)
             AND ("FirstPendingAt" IS NULL OR "FirstPendingAt" <= ? OR NOT EXISTS (
                 SELECT 1 FROM "AddonMediaList" AS "list" WHERE "list"."Id" = "AddonMediaListId"
                   AND "list"."DetailProjectionPendingAt" > ?))
           ORDER BY COALESCE("LastAssessedAt", "RecordedAt"), "AddonMediaListId" LIMIT 1"#,
    )
    .bind(encode_datetime(now))
    .bind(encode_datetime(now - chrono::Duration::seconds(120)))
    .bind(encode_datetime(now - chrono::Duration::seconds(30)))
    .fetch_optional(pool)
    .await
    .repository()?;
    let Some(row) = row else {
        return Ok(false);
    };
    let list_id: EntityId = row.try_get("AddonMediaListId").repository()?;
    let repository = SqliteAddonMediaListRepository(pool.clone());
    crate::addon_list_selection::queue_missing_facts(pool, list_id).await?;
    let readiness = repository.publication_readiness(list_id).await;
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    let row = sqlx::query(
        r#"SELECT "auto"."DraftRevision", "auto"."PublishedRevision", "auto"."State", "auto"."InitialHoldReason",
                  "list"."DraftRevision" AS "CurrentDraft", "list"."PublishedRevision" AS "CurrentPublished",
                  "list"."RefreshPublicationPolicy", "managed"."LibraryId"
           FROM "AddonMediaListAutoPublication" AS "auto"
           JOIN "AddonMediaList" AS "list" ON "list"."Id" = "auto"."AddonMediaListId"
           LEFT JOIN "AddonMediaListLibrary" AS "managed" ON "managed"."AddonMediaListId" = "list"."Id"
           WHERE "auto"."AddonMediaListId" = ? AND ("auto"."State" IN ('ready', 'preparing')
               OR ("auto"."State" = 'queued' AND EXISTS (SELECT 1 FROM "ManagedLibraryOperation"
                   WHERE "Id" = "auto"."OperationId" AND ("State" = 4 OR
                     ("State" = 3 AND EXISTS (SELECT 1 FROM "AddonMediaListPublicationJob" AS "job"
                        WHERE "job"."OperationId"="ManagedLibraryOperation"."Id" AND "job"."Outcome"=1))))))"#,
    ).bind(list_id).fetch_optional(&mut *tx).await.repository()?;
    let Some(row) = row else {
        return Ok(false);
    };
    sqlx::query(r#"UPDATE "AddonMediaListAutoPublication" SET "LastAssessedAt" = ? WHERE "AddonMediaListId" = ?"#)
        .bind(encode_datetime(now)).bind(list_id).execute(&mut *tx).await.repository()?;
    let automatic = row
        .try_get::<i32, _>("RefreshPublicationPolicy")
        .repository()?
        == 2;
    if !automatic
        && row
            .try_get::<i32, _>("RefreshPublicationPolicy")
            .repository()?
            != 1
    {
        sqlx::query(r#"UPDATE "AddonMediaListAutoPublication" SET "State" = 'done', "Reason" = NULL WHERE "AddonMediaListId" = ?"#)
            .bind(list_id).execute(&mut *tx).await.repository()?;
        tx.commit().await.repository()?;
        return Ok(true);
    }
    let draft: i32 = row.try_get("DraftRevision").repository()?;
    let published: i32 = row.try_get("PublishedRevision").repository()?;
    let current_draft: i32 = row.try_get("CurrentDraft").repository()?;
    let current_published: i32 = row.try_get("CurrentPublished").repository()?;
    if draft != current_draft || published != current_published {
        if automatic {
            tx.commit().await.repository()?;
            return Ok(true); // The next admission captures the latest saved input.
        }
        hold_in_tx(&mut tx, list_id, Reason::ConcurrentChange).await?;
        tx.commit().await.repository()?;
        return Ok(true);
    }
    if row.try_get::<String, _>("State").repository()? == "queued" {
        hold_in_tx(&mut tx, list_id, Reason::PreparationFailed).await?;
        tx.commit().await.repository()?;
        return Ok(true);
    }
    if let Some(reason) = row
        .try_get::<Option<String>, _>("InitialHoldReason")
        .repository()?
    {
        let reason = serde_json::from_str(&reason).map_err(|error| RepositoryError::Decode {
            kind: "automatic publication initial hold",
            value: error.to_string(),
        })?;
        hold_in_tx(&mut tx, list_id, reason).await?;
        tx.commit().await.repository()?;
        return Ok(true);
    }
    // Another operator publication is never adopted as this automatic attempt.
    let active: bool = sqlx::query_scalar(
        r#"SELECT EXISTS (SELECT 1 FROM "ManagedLibraryOperation" AS "operation"
           JOIN "AddonMediaListPublicationJob" AS "job" ON "job"."OperationId" = "operation"."Id"
           WHERE "job"."AddonMediaListId" = ? AND "operation"."State" IN (0, 1, 2))"#,
    )
    .bind(list_id)
    .fetch_one(&mut *tx)
    .await
    .repository()?;
    if active && automatic {
        tx.commit().await.repository()?;
        return Ok(false);
    }
    let assessment = if active {
        AutoPublicationAssessment::Hold(Reason::ConcurrentChange)
    } else {
        assess_auto_publication_in_tx(&mut tx, list_id).await?
    };
    match assessment {
        AutoPublicationAssessment::NoChanges => {
            if automatic {
                sqlx::query(
                    r#"UPDATE "AddonMediaList" SET "ReviewState"=2,"ReviewError"=NULL,
                    "DetailProjectionPendingAt"=NULL WHERE "Id"=? AND "DraftRevision"=?
                    AND "PublishedRevision"=? AND "ReviewState"<>3"#,
                )
                .bind(list_id)
                .bind(draft)
                .bind(published)
                .execute(&mut *tx)
                .await
                .repository()?;
            }
            sqlx::query(r#"UPDATE "AddonMediaListAutoPublication" SET "State" = 'done', "Reason" = NULL WHERE "AddonMediaListId" = ?"#)
                .bind(list_id).execute(&mut *tx).await.repository()?;
        }
        AutoPublicationAssessment::Hold(reason) => hold_in_tx(&mut tx, list_id, reason).await?,
        AutoPublicationAssessment::Eligible => {
            let Ok(readiness) = readiness else {
                hold_in_tx(&mut tx, list_id, Reason::PreparationFailed).await?;
                tx.commit().await.repository()?;
                return Ok(true);
            };
            if readiness.saved_revision != draft {
                hold_in_tx(&mut tx, list_id, Reason::ConcurrentChange).await?;
                tx.commit().await.repository()?;
                return Ok(true);
            }
            if !readiness.blockers.is_empty()
                || (!automatic && readiness.restricted_excluded_count > 0)
            {
                let reason = if readiness.blockers.iter().any(|blocker| {
                    matches!(
                        blocker.kind,
                        AddonMediaListPublicationBlockerKind::PlaybackDetailsFailed
                            | AddonMediaListPublicationBlockerKind::PlaybackDetailsUnavailable
                    )
                }) {
                    Reason::PreparationFailed
                } else {
                    Reason::Unavailable
                };
                hold_in_tx(&mut tx, list_id, reason).await?;
            } else if readiness.required_prepared_media_count > 0 {
                sqlx::query(r#"UPDATE "AddonMediaListAutoPublication" SET "State" = 'preparing' WHERE "AddonMediaListId" = ?"#)
                    .bind(list_id).execute(&mut *tx).await.repository()?;
                tx.commit().await.repository()?;
                // Admission is exact-input idempotent even across a crash after
                // 'preparing' is saved. Pending cooldowns and failed attempts are
                // never reset by this automatic reassessment.
                if crate::addon_media_list_publication_preparation::queue_required_automatic_media(
                    pool, list_id, draft,
                )
                .await
                .is_err()
                {
                    let mut tx = crate::db::begin_write(pool).await.repository()?;
                    let still_preparing: bool = sqlx::query_scalar(r#"SELECT EXISTS(SELECT 1 FROM "AddonMediaListAutoPublication"
                        WHERE "AddonMediaListId"=? AND "State"='preparing' AND "DraftRevision"=? AND "PublishedRevision"=?)"#)
                        .bind(list_id).bind(draft).bind(published).fetch_one(&mut *tx).await.repository()?;
                    if still_preparing {
                        hold_in_tx(&mut tx, list_id, Reason::PreparationFailed).await?;
                    }
                    tx.commit().await.repository()?;
                }
                return Ok(true);
            } else if readiness.required_playback_detail_count > 0 {
                if row.try_get::<String, _>("State").repository()? == "preparing" {
                    // Mandatory work already belongs to the detail coordinator.
                    // Its atomic ManualPriority marker also closes the crash
                    // window between persisting 'preparing' and admitting work.
                    let unrequested: bool = sqlx::query_scalar(r#"SELECT EXISTS (
                        SELECT 1 FROM "AddonMediaListDetailJob" AS "detail"
                        JOIN "AddonMediaListItem" AS "item" ON "item"."Id" = "detail"."AddonMediaListItemId"
                        WHERE "item"."AddonMediaListId" = ? AND "item"."Included" = 1
                          AND "item"."Kind" = 4 AND "item"."EffectiveLiveness" = 1
                          AND ("item"."DurationSeconds" IS NULL OR "item"."DurationSeconds" <= 0)
                          AND "detail"."ManualPriority" = 0)"#)
                        .bind(list_id).fetch_one(&mut *tx).await.repository()?;
                    if !unrequested {
                        if readiness.required_playback_detail_pending
                            < readiness.required_playback_detail_count
                        {
                            hold_in_tx(&mut tx, list_id, Reason::PreparationFailed).await?;
                        }
                        // Re-admission would erase persisted retry cooldowns.
                        tx.commit().await.repository()?;
                        return Ok(true);
                    }
                }
                sqlx::query(r#"UPDATE "AddonMediaListAutoPublication" SET "State" = 'preparing' WHERE "AddonMediaListId" = ?"#)
                    .bind(list_id).execute(&mut *tx).await.repository()?;
                tx.commit().await.repository()?;
                // This is the same idempotent mandatory-detail admission as
                // manual Publish. The next iteration reassesses its result.
                if repository
                    .prepare_automatic_publication_details(list_id)
                    .await
                    .is_err()
                {
                    let mut tx = crate::db::begin_write(pool).await.repository()?;
                    // Keep only a typed, bounded notice, never provider error text.
                    let still_preparing: bool = sqlx::query_scalar(r#"SELECT EXISTS(SELECT 1 FROM "AddonMediaListAutoPublication"
                        WHERE "AddonMediaListId" = ? AND "State" = 'preparing' AND "DraftRevision" = ? AND "PublishedRevision" = ?)"#)
                        .bind(list_id).bind(draft).bind(published).fetch_one(&mut *tx).await.repository()?;
                    if still_preparing {
                        hold_in_tx(&mut tx, list_id, Reason::PreparationFailed).await?;
                    }
                    tx.commit().await.repository()?;
                }
                return Ok(true);
            } else if let Some(library_id) = row
                .try_get::<Option<EntityId>, _>("LibraryId")
                .repository()?
            {
                let queued = enqueue_publication_in_tx(
                    &mut tx,
                    list_id,
                    library_id,
                    draft,
                    published.saturating_add(1),
                    AddonMediaListPublicationEnqueueTrigger::AutomaticRefresh,
                )
                .await?;
                sqlx::query(r#"UPDATE "AddonMediaListAutoPublication" SET "State" = 'queued', "Reason" = NULL, "OperationId" = ? WHERE "AddonMediaListId" = ?"#)
                    .bind(queued.operation.id).bind(list_id).execute(&mut *tx).await.repository()?;
            } else {
                hold_in_tx(&mut tx, list_id, Reason::PreparationFailed).await?;
            }
        }
    }
    tx.commit().await.repository()?;
    Ok(true)
}

/// Resolve the bounded notice only after an explicit or automatic publication
/// has successfully committed a revision based on this assessment's baseline.
pub(crate) async fn published_in_tx(tx: &mut Transaction<'_, Db>, list_id: EntityId) -> Result<()> {
    sqlx::query(
        r#"UPDATE "AddonMediaListAutoPublication" SET "State" = 'done', "Reason" = NULL
           WHERE "AddonMediaListId" = ? AND "PublishedRevision" < (
               SELECT "PublishedRevision" FROM "AddonMediaList" WHERE "Id" = ?
           )"#,
    )
    .bind(list_id)
    .bind(list_id)
    .execute(&mut **tx)
    .await
    .repository()?;
    sqlx::query(r#"UPDATE "AddonMediaList" SET "DetailProjectionPendingAt"=NULL
        WHERE "Id"=? AND "RefreshPublicationPolicy"=2 AND "DraftRevision"=(
            SELECT "DraftRevision" FROM "AddonMediaListAutoPublication" WHERE "AddonMediaListId"=?)"#)
        .bind(list_id).bind(list_id).execute(&mut **tx).await.repository()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_core::repositories::AddonMediaListLifecycleRepository as _;

    #[tokio::test]
    async fn automatic_migration_adopts_saved_drafts_without_breaking_active_journals() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let migrations = sqlx::migrate!("../../migrations");
        sqlx::migrate::Migrator::with_migrations(
            migrations
                .iter()
                .filter(|migration| migration.version < 202_609_161_200)
                .cloned()
                .collect(),
        )
        .run(&pool)
        .await
        .unwrap();
        sqlx::raw_sql(r#"
            INSERT INTO "AddonInstallation" ("AddonId","Version","Enabled","PackageSha256","ContentSha256",
                "ManifestJson","InstallPath","InstalledAt","UpdatedAt")
            VALUES ('org.example.upgrade','1.0.0',1,'package','content','{}','addons/upgrade',
                '2026-09-16T00:00:00Z','2026-09-16T00:00:00Z');
            INSERT INTO "AddonMediaList" ("Id","AddonId","ProviderId","SourceUrl","Name",
                "RefreshPublicationPolicy","DetailImportPolicy","DraftRevision","PublishedRevision",
                "ReviewState","PublicationJournalJson") VALUES
                (1,'org.example.upgrade','one','https://example.invalid/one','One',0,1,19,1,1,NULL),
                (2,'org.example.upgrade','two','https://example.invalid/two','Two',1,0,8,4,3,'{"fence":"kept"}');
            INSERT INTO "AddonMediaListItem" ("Id","AddonMediaListId","ProviderId","DisplayTitle","Title",
                "Kind","Rank","Published","Included","MetadataOverrideJson")
                VALUES (10,1,'video','Video','Video',4,0,1,1,'{"plot":null}');
        "#).execute(&pool).await.unwrap();
        migrations.run(&pool).await.unwrap();
        let rows = sqlx::query_as::<_, (i64,i32,i32,i32,Option<String>)>(r#"
            SELECT "Id","RefreshPublicationPolicy","DetailImportPolicy","DraftRevision","PublicationJournalJson"
            FROM "AddonMediaList" ORDER BY "Id""#).fetch_all(&pool).await.unwrap();
        assert_eq!(
            rows,
            vec![
                (1, 2, 0, 20, None),
                (2, 2, 0, 8, Some("{\"fence\":\"kept\"}".into()))
            ]
        );
        let item: (i32, i32, String) = sqlx::query_as(
            r#"SELECT "Published","Included","MetadataOverrideJson"
            FROM "AddonMediaListItem" WHERE "Id"=10"#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(item, (1, 1, "{\"plot\":null}".into()));
        migrations.run(&pool).await.unwrap();
        let revision: i32 =
            sqlx::query_scalar(r#"SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id"=1"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            revision, 20,
            "restarting does not repeatedly advance the draft"
        );
    }

    async fn fixture() -> DbPool {
        fixture_at("sqlite::memory:").await
    }

    async fn fixture_at(url: &str) -> DbPool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(url)
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
        sqlx::raw_sql(r#"
            INSERT INTO "AddonInstallation" ("AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
                "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt")
            VALUES ('org.example.auto', '1.0.0', 1, 'package', 'content', '{}', 'addons/auto',
                '2026-09-04T00:00:00Z', '2026-09-04T00:00:00Z');
            INSERT INTO "MediaSource" ("Id") VALUES (1);
            INSERT INTO "Library" ("Id", "MediaKind", "MediaSourceId", "Name") VALUES (1, 4, 1, 'Fixture');
            INSERT INTO "AddonMediaList" ("Id", "AddonId", "ProviderId", "SourceUrl", "Name",
                "DraftRevision", "PublishedRevision", "ReviewState", "RefreshPublicationPolicy",
                "PresentationKind", "RequestedPresentationKind")
            VALUES (1, 'org.example.auto', 'list', 'https://example.invalid/list', 'List', 1, 1, 2, 1, 1, 1);
            INSERT INTO "AddonMediaListLibrary" ("AddonMediaListId", "LibraryId", "FolderName", "ManifestPath")
            VALUES (1, 1, 'fixture', 'fixture/list.yml');
            INSERT INTO "AddonMediaListItem" ("Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Title",
                "Kind", "Rank", "Published", "EffectiveLiveness", "SourceUrl")
            VALUES (10, 1, 'old', 'Old', 'Old', 4, 0, 1, 2, 'https://example.invalid/old');
            UPDATE "AddonMediaList" SET "ProviderMetadataJson" = '{"title":"List"}';
            UPDATE "AddonMediaListItem" SET "ProviderMetadataJson" = '{"title":"Old"}';
        "#).execute(&pool).await.unwrap();
        crate::repositories::make_repositories(pool.clone())
            .addon_media_lists
            .retain_published_revision(
                1,
                1,
                1,
                Utc::now(),
                &[],
                &ersatzrs_core::repositories::AddonMediaListRevisionArtifacts::default(),
            )
            .await
            .unwrap();
        pool
    }

    async fn add_item(pool: &DbPool) {
        sqlx::query(
            r#"INSERT INTO "AddonMediaListItem" ("Id", "AddonMediaListId", "ProviderId",
            "DisplayTitle", "Title", "Kind", "Rank", "EffectiveLiveness", "SourceUrl", "ProviderMetadataJson")
            VALUES (11, 1, 'new', 'New', 'New', 4, 1, 2, 'https://example.invalid/new', '{"title":"New"}')"#,
        )
        .execute(pool)
        .await
        .unwrap();
    }

    async fn finish_legacy(pool: &DbPool, baseline: LegacyAutoPublicationBaseline) {
        sqlx::query(
            r#"UPDATE "AddonMediaList" SET "DraftRevision" = "DraftRevision" + 1 WHERE "Id" = 1"#,
        )
        .execute(pool)
        .await
        .unwrap();
        legacy_ready(pool, 1, baseline).await.unwrap();
    }

    async fn baseline(pool: &DbPool) -> LegacyAutoPublicationBaseline {
        legacy_baseline(pool, "org.example.auto", "https://example.invalid/list")
            .await
            .unwrap()
            .unwrap()
    }

    async fn state(pool: &DbPool) -> String {
        sqlx::query_scalar(
            r#"SELECT "State" FROM "AddonMediaListAutoPublication" WHERE "AddonMediaListId" = 1"#,
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)] // Keep the original 13-item defect and its one-publication assertion together.
    async fn thirteen_enriched_items_with_availability_and_duration_changes_publish_automatically()
    {
        use ersatzrs_core::addon_media_list::EnumeratedAddonMediaListItem;
        use ersatzrs_core::repositories::AddonMediaListNfoMetadata;
        use ersatzrs_domain::{
            AddonMediaListContentKind, AddonMediaListItemAvailability, AddonMediaListItemKind,
            AddonMediaListItemRole, RemoteStreamLiveness,
        };
        let pool = fixture().await;
        sqlx::raw_sql(
            r#"DELETE FROM "AddonMediaListItem";
            UPDATE "AddonMediaList" SET "RefreshPublicationPolicy"=2,"DraftRevision"=2;"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        for index in 0..13 {
            let title = format!("Episode {index}");
            let provider_id = format!("episode-{index}");
            let source = format!("https://example.invalid/{provider_id}");
            sqlx::query(
                r#"INSERT INTO "AddonMediaListItem" (
                "AddonMediaListId","ProviderId","DisplayTitle","Title","Kind","Rank",
                "SourceUrl","Published","Availability","DurationSeconds","ProviderLiveness",
                "EffectiveLiveness","ProviderMetadataJson") VALUES (1,?,?,?,4,?,?,1,2,601,1,1,?)"#,
            )
            .bind(&provider_id)
            .bind(&title)
            .bind(&title)
            .bind(index)
            .bind(&source)
            .bind(serde_json::json!({"title":title}).to_string())
            .execute(&pool)
            .await
            .unwrap();
            let detail = EnumeratedAddonMediaListItem {
                provider_id,
                rank: index,
                display_title: title.clone(),
                title: title.clone(),
                year: Some(2026),
                season: Some(1),
                episode: Some(index + 1),
                kind: AddonMediaListItemKind::RemoteStream,
                guids: vec![],
                source_url: Some(source),
                availability: AddonMediaListItemAvailability::Available,
                availability_reason: None,
                availability_reason_code: None,
                content_kind: AddonMediaListContentKind::TelevisionEpisode,
                collection_role: AddonMediaListItemRole::Auto,
                duration_seconds: Some(if index < 8 { 600 } else { 601 }),
                liveness: RemoteStreamLiveness::Finite,
                parent_provider_id: None,
                fragment_start_seconds: None,
                fragment_end_seconds: None,
                additional_image_urls: vec![],
                metadata: AddonMediaListNfoMetadata {
                    title,
                    plot: Some(format!("Full description for episode {index}")),
                    release_date: Some("2026-05-05".into()),
                    genres: vec!["Entertainment".into()],
                    tags: vec!["Archive".into()],
                    languages: vec!["en".into()],
                    studios: vec!["Fixture Channel".into()],
                    content_ratings: vec!["age:0".into()],
                    ..Default::default()
                },
            };
            crate::addon_media_list_sync::AddonMediaListSynchronizer::new(pool.clone())
                .merge_detail_item(1, &detail)
                .await
                .unwrap();
        }
        let count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "AddonMediaListItem"
            WHERE "Availability"=0 AND json_extract("ProviderMetadataJson",'$.plot') IS NOT NULL
            AND json_extract("ProviderMetadataJson",'$.release_date')='2026-05-05'"#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 13);
        let revised: i32 =
            sqlx::query_scalar(r#"SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id"=1"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(revised, 15);
        let now = Utc::now();
        assert!(
            !process_one(&pool, now).await.unwrap(),
            "wait for a quiet detail batch"
        );
        assert!(process_one(&pool, now + chrono::Duration::seconds(31))
            .await
            .unwrap());
        assert_eq!(state(&pool).await, "queued");
        assert!(!process_one(&pool, now + chrono::Duration::seconds(32))
            .await
            .unwrap());
        let jobs: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaListPublicationJob""#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(jobs, 1, "one revision owns all thirteen results");
    }

    #[tokio::test]
    async fn automatic_saved_changes_admit_without_scheduled_refresh_or_review() {
        let pool = fixture().await;
        sqlx::raw_sql(
            r#"UPDATE "AddonMediaList" SET "RefreshPublicationPolicy"=2,
            "DraftRevision"=2,"ReviewState"=1;
            UPDATE "AddonMediaListItem" SET "Availability"=2,
            "MetadataOverrideJson"='{"tags":["operator"]}',
            "MetadataConflictJson"='{"tags":{"provider_before":[],"provider_after":["provider"],"operator_value":["operator"]}}';"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(process_one(&pool, Utc::now()).await.unwrap());
        assert_eq!(state(&pool).await, "queued");
        assert!(load_hold(&pool, 1).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn automatic_first_publication_does_not_require_a_baseline() {
        let pool = fixture().await;
        sqlx::raw_sql(
            r#"UPDATE "AddonMediaList" SET "RefreshPublicationPolicy"=2,
            "PublishedRevision"=0,"ReviewState"=1;
            UPDATE "AddonMediaListItem" SET "Published"=0;"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(process_one(&pool, Utc::now()).await.unwrap());
        assert_eq!(state(&pool).await, "queued");
    }

    #[tokio::test]
    async fn initial_selection_review_holds_first_publication_and_acquisition_until_saved() {
        use ersatzrs_core::addon_list_selection::{
            AddonListSelectionPolicy, AddonListSelectionSettings,
        };
        use ersatzrs_core::repositories::AddonMediaListRepository as _;
        let pool = fixture().await;
        sqlx::raw_sql(r#"UPDATE "AddonMediaList" SET "RefreshPublicationPolicy"=2,"PublishedRevision"=0,"ReviewState"=1;
            UPDATE "AddonMediaListItem" SET "Published"=0;"#).execute(&pool).await.unwrap();
        let mut tx = crate::db::begin_write(&pool).await.unwrap();
        crate::addon_list_selection::request_review_in_tx(&mut tx, 1)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        assert!(!process_one(&pool, Utc::now()).await.unwrap());
        let repo = SqliteAddonMediaListRepository(pool.clone());
        let readiness = repo.publication_readiness(1).await.unwrap();
        assert!(readiness.blockers.iter().any(|b| b.kind==ersatzrs_core::repositories::AddonMediaListPublicationBlockerKind::SelectionReviewRequired));
        assert!(repo.prepare_publication_media(1, 1).await.is_err());
        assert_eq!(
            sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM "AddonMediaListPublicationJob""#)
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
        let mut tx = crate::db::begin_write(&pool).await.unwrap();
        crate::addon_list_selection::save_in_tx(
            &mut tx,
            1,
            &AddonListSelectionSettings {
                policy: Some(AddonListSelectionPolicy::default()),
                refresh_paused: false,
            },
            Utc::now(),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        assert!(process_one(&pool, Utc::now()).await.unwrap());
        assert_eq!(state(&pool).await, "queued");
    }

    #[tokio::test]
    async fn automatic_noop_save_does_not_queue_publication() {
        let pool = fixture().await;
        sqlx::query(
            r#"UPDATE "AddonMediaList" SET "RefreshPublicationPolicy"=2,"DraftRevision"=2"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(process_one(&pool, Utc::now()).await.unwrap());
        assert_eq!(state(&pool).await, "done");
        assert!(!process_one(&pool, Utc::now()).await.unwrap());
        let operations: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaListPublicationJob""#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(operations, 0);
    }

    #[tokio::test]
    async fn automatic_detail_batch_waits_for_quiet_but_cannot_starve() {
        let pool = fixture().await;
        let now = Utc::now();
        sqlx::query(
            r#"UPDATE "AddonMediaList" SET "RefreshPublicationPolicy"=2,
            "DraftRevision"=2,"DetailProjectionPendingAt"=?,"Name"='Changed'"#,
        )
        .bind(encode_datetime(now))
        .execute(&pool)
        .await
        .unwrap();
        assert!(!process_one(&pool, now).await.unwrap());
        assert_eq!(state(&pool).await, "ready");
        assert!(!process_one(&pool, now + chrono::Duration::seconds(29))
            .await
            .unwrap());
        // A stream of newer details extends quiet time, but not first-pending time.
        sqlx::query(
            r#"UPDATE "AddonMediaList" SET "DraftRevision"=3,"DetailProjectionPendingAt"=?"#,
        )
        .bind(encode_datetime(now + chrono::Duration::seconds(115)))
        .execute(&pool)
        .await
        .unwrap();
        assert!(process_one(&pool, now + chrono::Duration::seconds(120))
            .await
            .unwrap());
        assert_eq!(state(&pool).await, "queued");
        let revision: i32 =
            sqlx::query_scalar(r#"SELECT "SavedRevision" FROM "AddonMediaListPublicationJob""#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(revision, 3);
    }

    // The installed-package fixture uses POSIX; native Windows execution remains separate.
    #[cfg(unix)]
    async fn streaming_addition() -> (tempfile::TempDir, DbPool) {
        let temporary = tempfile::tempdir().unwrap();
        let pool = crate::addon_media_list_publication_preparation::tests::streaming_fixture(
            &temporary.path().join("package"),
        )
        .await;
        sqlx::raw_sql(r#"INSERT INTO "MediaSource" ("Id") VALUES (1);
            INSERT INTO "Library" ("Id","MediaKind","MediaSourceId","Name") VALUES (1,4,1,'Fixture');
            INSERT INTO "AddonMediaListLibrary" ("AddonMediaListId","LibraryId","FolderName","ManifestPath")
                VALUES (1,1,'fixture','fixture/list.yml');
            UPDATE "AddonMediaList" SET "DraftRevision"=1,"PublishedRevision"=1,"ReviewState"=2,
                "RefreshPublicationPolicy"=1,"PresentationKind"=1,"RequestedPresentationKind"=1,
                "ProviderMetadataJson"='{"title":"Fixture"}';
            UPDATE "AddonMediaListItem" SET "Published"=1,"EffectiveLiveness"=2,"ProviderLiveness"=2,
                "ProviderMetadataJson"='{"title":"Fixture"}';"#)
            .execute(&pool).await.unwrap();
        SqliteAddonMediaListRepository(pool.clone())
            .retain_published_revision(
                1,
                1,
                1,
                Utc::now(),
                &[],
                &ersatzrs_core::repositories::AddonMediaListRevisionArtifacts::default(),
            )
            .await
            .unwrap();
        let before = legacy_baseline(
            &pool,
            "org.ersatzrs.addon.fixture",
            "https://example.test/list",
        )
        .await
        .unwrap()
        .unwrap();
        sqlx::query(r#"INSERT INTO "AddonMediaListItem" ("Id","AddonMediaListId","ProviderId",
            "DisplayTitle","Title","Kind","Rank","EffectiveLiveness","ProviderLiveness","SourceUrl","ProviderMetadataJson")
            VALUES (11,1,'new','New','New',4,1,1,1,'https://example.test/new','{"title":"New"}')"#)
            .execute(&pool).await.unwrap();
        finish_legacy(&pool, before).await;
        (temporary, pool)
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn automatic_streaming_waits_for_metadata_and_preserves_retry_cooldown() {
        use crate::addon_media_list_item_jobs::{
            AddonMediaListItemJobQueue, ItemOperationCompletion, ItemOperationKind,
        };
        use ersatzrs_core::addon_media_list_playback::AddonMediaListPlaybackTimelinePlan;

        let (temporary, pool) = streaming_addition().await;
        // Crash after persisting 'preparing', before admitting any item job.
        sqlx::query(r#"UPDATE "AddonMediaListAutoPublication" SET "State"='preparing'"#)
            .execute(&pool)
            .await
            .unwrap();
        assert!(process_one(&pool, Utc::now()).await.unwrap());
        assert_eq!(state(&pool).await, "preparing");
        let operations: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "ManagedLibraryOperation""#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(operations, 0);
        sqlx::query(r#"UPDATE "AddonMediaListItemOperationJob" SET "NextEligibleAt"='2099-01-01T00:00:00Z'"#)
            .execute(&pool).await.unwrap();
        let before: (i64, i64, i32, String) = sqlx::query_as(r#"SELECT "Id","Generation","Kind","NextEligibleAt" FROM "AddonMediaListItemOperationJob""#)
            .fetch_one(&pool).await.unwrap();
        assert_eq!(before.2, 2);
        assert!(process_one(&pool, Utc::now()).await.unwrap());
        let after: (i64, i64, i32, String) = sqlx::query_as(r#"SELECT "Id","Generation","Kind","NextEligibleAt" FROM "AddonMediaListItemOperationJob""#)
            .fetch_one(&pool).await.unwrap();
        assert_eq!(after, before);
        assert_eq!(state(&pool).await, "preparing");
        assert_eq!(
            std::fs::read_to_string(temporary.path().join("preparation-invocations")).unwrap(),
            "runtime-info\n"
        );
        sqlx::query(r#"UPDATE "AddonMediaListItemOperationJob" SET "NextEligibleAt"=NULL"#)
            .execute(&pool)
            .await
            .unwrap();
        let queue = AddonMediaListItemJobQueue::new(pool.clone());
        let job = queue
            .claim_kind(Utc::now(), 90, ItemOperationKind::IntervalMetadata)
            .await
            .unwrap()
            .unwrap();
        let plan = AddonMediaListPlaybackTimelinePlan::new(
            20_000,
            0..20_000,
            &[5_000..7_125, 18_500..20_000],
            job.input.context_fingerprint.clone(),
        )
        .unwrap();
        queue
            .complete(
                &job,
                ItemOperationCompletion::IntervalPlan { plan },
                Utc::now(),
            )
            .await
            .unwrap();
        assert!(process_one(&pool, Utc::now()).await.unwrap());
        assert_eq!(state(&pool).await, "queued");
        assert!(!process_one(&pool, Utc::now()).await.unwrap());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn automatic_streaming_holds_failed_metadata_without_readmitting_it() {
        use crate::addon_media_list_item_jobs::{
            AddonMediaListItemJobQueue, ItemOperationCompletion, ItemOperationFailure,
            ItemOperationKind,
        };
        let (_temporary, pool) = streaming_addition().await;
        assert!(process_one(&pool, Utc::now()).await.unwrap());
        let queue = AddonMediaListItemJobQueue::new(pool.clone());
        let job = queue
            .claim_kind(Utc::now(), 90, ItemOperationKind::IntervalMetadata)
            .await
            .unwrap()
            .unwrap();
        queue
            .complete(
                &job,
                ItemOperationCompletion::Failed(ItemOperationFailure::Unsupported),
                Utc::now(),
            )
            .await
            .unwrap();
        assert!(process_one(&pool, Utc::now()).await.unwrap());
        assert_eq!(state(&pool).await, "held");
        assert_eq!(
            load_hold(&pool, 1).await.unwrap().unwrap().reason,
            Reason::PreparationFailed
        );
        assert!(!process_one(&pool, Utc::now()).await.unwrap());
        let attempts: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaListItemOperationJob""#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(attempts, 1);
        let operations: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "ManagedLibraryOperation""#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(operations, 0);
        // A retry is an explicit manual action, not an automatic new attempt.
        assert_eq!(
            SqliteAddonMediaListRepository(pool.clone())
                .prepare_publication_media(1, 2)
                .await
                .unwrap(),
            1
        );
        let attempts: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaListItemOperationJob""#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(attempts, 2);
    }

    #[tokio::test]
    async fn auto_publication_restart_preserves_ready_candidate() {
        let directory = tempfile::tempdir().unwrap();
        let url = format!(
            "sqlite://{}?mode=rwc",
            directory.path().join("auto.sqlite").display()
        );
        let pool = fixture_at(&url).await;
        let before = baseline(&pool).await;
        add_item(&pool).await;
        finish_legacy(&pool, before).await;
        pool.close().await;
        let reopened = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .unwrap();
        assert!(process_one(&reopened, Utc::now()).await.unwrap());
        assert_eq!(state(&reopened).await, "queued");
        assert!(!process_one(&reopened, Utc::now()).await.unwrap());
        reopened.close().await;
    }

    #[tokio::test]
    async fn auto_publication_repeated_hold_retains_notice_timestamp() {
        let pool = fixture().await;
        add_item(&pool).await;
        let first = baseline(&pool).await;
        finish_legacy(&pool, first).await;
        assert!(process_one(&pool, Utc::now()).await.unwrap());
        let recorded_at = load_hold(&pool, 1).await.unwrap().unwrap().recorded_at;
        let next = baseline(&pool).await;
        finish_legacy(&pool, next).await;
        assert!(process_one(&pool, Utc::now()).await.unwrap());
        assert_eq!(
            load_hold(&pool, 1).await.unwrap().unwrap().recorded_at,
            recorded_at
        );
    }

    #[tokio::test]
    async fn auto_publication_requires_scheduler_admission_and_opt_in() {
        let pool = fixture().await;
        add_item(&pool).await;
        assert!(!process_one(&pool, Utc::now()).await.unwrap());
        sqlx::query(r#"UPDATE "AddonMediaList" SET "RefreshPublicationPolicy" = 0"#)
            .execute(&pool)
            .await
            .unwrap();
        assert!(
            legacy_baseline(&pool, "org.example.auto", "https://example.invalid/list")
                .await
                .unwrap()
                .is_none()
        );
        let mut tx = pool.begin().await.unwrap();
        admit_in_tx(&mut tx, 1, None).await.unwrap();
        tx.commit().await.unwrap();
        assert!(!process_one(&pool, Utc::now()).await.unwrap());
    }

    #[tokio::test]
    async fn auto_publication_holds_preexisting_pending_additions() {
        let pool = fixture().await;
        add_item(&pool).await;
        let before = baseline(&pool).await;
        finish_legacy(&pool, before).await;
        assert!(process_one(&pool, Utc::now()).await.unwrap());
        assert_eq!(
            load_hold(&pool, 1).await.unwrap().unwrap().reason,
            Reason::ExistingChanges
        );
        assert!(!process_one(&pool, Utc::now()).await.unwrap());
    }

    #[tokio::test]
    async fn auto_publication_queues_additions_with_causal_trigger_once() {
        let pool = fixture().await;
        let before = baseline(&pool).await;
        add_item(&pool).await;
        finish_legacy(&pool, before).await;
        assert!(process_one(&pool, Utc::now()).await.unwrap());
        assert_eq!(
            state(&pool).await,
            "queued",
            "hold: {:?}; readiness: {:?}",
            load_hold(&pool, 1).await.unwrap(),
            SqliteAddonMediaListRepository(pool.clone())
                .publication_readiness(1)
                .await
                .unwrap()
        );
        let trigger: i32 = sqlx::query_scalar(r#"SELECT "EnqueueTrigger" FROM "AddonMediaListPublicationEvent" WHERE "DedupeKey" = 'enqueued'"#)
            .fetch_one(&pool).await.unwrap();
        assert_eq!(
            trigger,
            crate::repositories::common::encode_enum(
                AddonMediaListPublicationEnqueueTrigger::AutomaticRefresh
            )
        );
        assert!(!process_one(&pool, Utc::now()).await.unwrap());
        sqlx::query(r#"UPDATE "ManagedLibraryOperation" SET "State" = 4"#)
            .execute(&pool)
            .await
            .unwrap();
        assert!(process_one(&pool, Utc::now()).await.unwrap());
        assert_eq!(
            load_hold(&pool, 1).await.unwrap().unwrap().reason,
            Reason::PreparationFailed
        );
        assert!(!process_one(&pool, Utc::now()).await.unwrap());
    }

    #[tokio::test]
    async fn auto_publication_manual_refresh_revokes_only_pending_assessments() {
        let pool = fixture().await;
        let _before = baseline(&pool).await;
        for pending in ["importing", "ready", "preparing"] {
            sqlx::query(
                r#"UPDATE "AddonMediaListAutoPublication" SET "State" = ?, "Reason" = NULL"#,
            )
            .bind(pending)
            .execute(&pool)
            .await
            .unwrap();
            manual_refresh(&pool, "org.example.auto", "https://example.invalid/list")
                .await
                .unwrap();
            assert_eq!(
                load_hold(&pool, 1).await.unwrap().unwrap().reason,
                Reason::ConcurrentChange
            );
            assert!(!process_one(&pool, Utc::now()).await.unwrap());
        }
        sqlx::query(r#"UPDATE "AddonMediaListAutoPublication" SET "State" = 'queued'"#)
            .execute(&pool)
            .await
            .unwrap();
        manual_refresh(&pool, "org.example.auto", "https://example.invalid/list")
            .await
            .unwrap();
        assert_eq!(state(&pool).await, "queued");
    }

    #[tokio::test]
    async fn auto_publication_noop_is_quiet_and_revision_mismatch_is_held() {
        let pool = fixture().await;
        let before = baseline(&pool).await;
        finish_legacy(&pool, before).await;
        assert!(process_one(&pool, Utc::now()).await.unwrap());
        assert_eq!(state(&pool).await, "done");
        assert!(load_hold(&pool, 1).await.unwrap().is_none());

        let before = baseline(&pool).await;
        add_item(&pool).await;
        finish_legacy(&pool, before).await;
        sqlx::query(r#"UPDATE "AddonMediaList" SET "DraftRevision" = "DraftRevision" + 1"#)
            .execute(&pool)
            .await
            .unwrap();
        assert!(process_one(&pool, Utc::now()).await.unwrap());
        assert_eq!(
            load_hold(&pool, 1).await.unwrap().unwrap().reason,
            Reason::ConcurrentChange
        );
    }

    #[tokio::test]
    async fn auto_publication_legacy_requires_exact_single_revision_advance() {
        let pool = fixture().await;
        let before = baseline(&pool).await;
        sqlx::query(r#"UPDATE "AddonMediaList" SET "DraftRevision" = "DraftRevision" + 2"#)
            .execute(&pool)
            .await
            .unwrap();
        legacy_ready(&pool, 1, before).await.unwrap();
        assert_eq!(
            load_hold(&pool, 1).await.unwrap().unwrap().reason,
            Reason::ConcurrentChange
        );
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)] // One persisted preparation lifecycle, including a competing list.
    async fn auto_publication_preparing_preserves_retry_and_failed_work_is_held() {
        let pool = fixture().await;
        let before = baseline(&pool).await;
        add_item(&pool).await;
        sqlx::raw_sql(r#"UPDATE "AddonMediaListItem" SET "EffectiveLiveness" = 1, "ProviderLiveness" = 1 WHERE "Id" = 11;
            INSERT INTO "AddonMediaListDetailJob" ("AddonMediaListItemId", "AddonMediaListId", "ProviderHost", "State",
                "OverviewFingerprint", "OverviewRecordJson", "UpdatedAt")
            VALUES (11, 1, 'example.invalid', 4, 'fingerprint', '{}', '2026-09-04T00:00:00Z');
            UPDATE "AddonMediaListDetailJob" SET "NextEligibleAt" = '2099-01-01T00:00:00Z'"#)
            .execute(&pool).await.unwrap();
        finish_legacy(&pool, before).await;
        assert!(process_one(&pool, Utc::now()).await.unwrap());
        assert_eq!(
            state(&pool).await,
            "preparing",
            "hold: {:?}; readiness: {:?}",
            load_hold(&pool, 1).await.unwrap(),
            SqliteAddonMediaListRepository(pool.clone())
                .publication_readiness(1)
                .await
                .unwrap()
        );
        let first_cooldown: String =
            sqlx::query_scalar(r#"SELECT "NextEligibleAt" FROM "AddonMediaListDetailJob""#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(first_cooldown, "2099-01-01T00:00:00Z");
        assert!(process_one(&pool, Utc::now()).await.unwrap());
        let cooldown: String =
            sqlx::query_scalar(r#"SELECT "NextEligibleAt" FROM "AddonMediaListDetailJob""#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(cooldown, "2099-01-01T00:00:00Z");
        // An older preparing row cannot monopolize the bounded coordinator.
        sqlx::query(r#"INSERT INTO "AddonMediaList" ("Id", "AddonId", "ProviderId", "SourceUrl", "Name",
            "DraftRevision", "PublishedRevision", "ReviewState", "RefreshPublicationPolicy")
            VALUES (2, 'org.example.auto', 'other', 'https://example.invalid/other', 'Other', 1, 1, 2, 1)"#)
            .execute(&pool).await.unwrap();
        crate::repositories::make_repositories(pool.clone())
            .addon_media_lists
            .retain_published_revision(
                2,
                1,
                1,
                Utc::now(),
                &[],
                &ersatzrs_core::repositories::AddonMediaListRevisionArtifacts::default(),
            )
            .await
            .unwrap();
        let mut tx = pool.begin().await.unwrap();
        admit_in_tx(&mut tx, 2, None).await.unwrap();
        sqlx::raw_sql(r#"UPDATE "AddonMediaListAutoPublication" SET "LastAssessedAt" = NULL,
                "RecordedAt" = CASE "AddonMediaListId" WHEN 1 THEN '2000-01-01T00:00:00Z' ELSE '2001-01-01T00:00:00Z' END;
            UPDATE "AddonMediaListAutoPublication" SET "State" = 'ready' WHERE "AddonMediaListId" = 2"#)
            .execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        assert!(process_one(&pool, Utc::now()).await.unwrap());
        assert!(process_one(&pool, Utc::now()).await.unwrap());
        let second_state: String = sqlx::query_scalar(
            r#"SELECT "State" FROM "AddonMediaListAutoPublication" WHERE "AddonMediaListId" = 2"#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(second_state, "done");
        sqlx::query(r#"UPDATE "AddonMediaListDetailJob" SET "State" = 7"#)
            .execute(&pool)
            .await
            .unwrap();
        assert!(process_one(&pool, Utc::now()).await.unwrap());
        assert_eq!(
            load_hold(&pool, 1).await.unwrap().unwrap().reason,
            Reason::PreparationFailed
        );
        assert!(!process_one(&pool, Utc::now()).await.unwrap());
    }
}
