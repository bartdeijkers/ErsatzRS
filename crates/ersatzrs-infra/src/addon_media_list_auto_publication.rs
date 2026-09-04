//! ADR 0085: scheduled refreshes may publish only validated additions.
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
             AND "list"."RefreshPublicationPolicy" = 1"#,
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

/// Evaluate at most one completed scheduled import and durably admit its exact
/// revision. Provider work stays in the existing detail/import coordinators.
///
/// # Errors
/// Returns database/validation errors without losing the pending assessment.
#[allow(clippy::too_many_lines)] // Keep the fenced assessment and its transaction exits together.
pub async fn process_one(pool: &DbPool, now: DateTime<Utc>) -> Result<bool> {
    let row = sqlx::query(
        r#"SELECT "AddonMediaListId" FROM "AddonMediaListAutoPublication"
           WHERE "State" IN ('ready', 'preparing')
              OR ("State" = 'queued' AND EXISTS (SELECT 1 FROM "ManagedLibraryOperation"
                  WHERE "Id" = "AddonMediaListAutoPublication"."OperationId" AND "State" = 4))
           ORDER BY COALESCE("LastAssessedAt", "RecordedAt"), "AddonMediaListId" LIMIT 1"#,
    )
    .fetch_optional(pool)
    .await
    .repository()?;
    let Some(row) = row else {
        return Ok(false);
    };
    let list_id: EntityId = row.try_get("AddonMediaListId").repository()?;
    let repository = SqliteAddonMediaListRepository(pool.clone());
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
                   WHERE "Id" = "auto"."OperationId" AND "State" = 4)))"#,
    ).bind(list_id).fetch_optional(&mut *tx).await.repository()?;
    let Some(row) = row else {
        return Ok(false);
    };
    sqlx::query(r#"UPDATE "AddonMediaListAutoPublication" SET "LastAssessedAt" = ? WHERE "AddonMediaListId" = ?"#)
        .bind(encode_datetime(now)).bind(list_id).execute(&mut *tx).await.repository()?;
    if row
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
    let assessment = if active {
        AutoPublicationAssessment::Hold(Reason::ConcurrentChange)
    } else {
        assess_auto_publication_in_tx(&mut tx, list_id).await?
    };
    match assessment {
        AutoPublicationAssessment::NoChanges => {
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
            if !readiness.blockers.is_empty() || readiness.restricted_excluded_count > 0 {
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_core::repositories::AddonMediaListLifecycleRepository as _;

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
            .retain_published_revision(1, 1, 1, Utc::now(), &[], &[])
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
        sqlx::raw_sql(r#"UPDATE "AddonMediaListItem" SET "EffectiveLiveness" = 1 WHERE "Id" = 11;
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
            .retain_published_revision(2, 1, 1, Utc::now(), &[], &[])
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
