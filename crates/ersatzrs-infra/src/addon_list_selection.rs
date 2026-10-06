//! Selection is part of the saved draft; publication and artifact ownership stay unchanged.

use chrono::{DateTime, NaiveDate, Utc};
use ersatzrs_core::{
    addon_list_selection::{
        self, AddonListSelectionItem, AddonListSelectionPolicy, AddonListSelectionPreview,
        AddonListSelectionSettings, AddonListSelectionState,
    },
    repositories::{RepositoryError, Result},
};
use sqlx::{Row as _, Transaction};

use crate::{
    db::{Db, DbPool},
    repositories::{
        common::{decode_datetime, encode_datetime},
        SqlxResultExt as _,
    },
};

fn invalid(message: &str) -> RepositoryError {
    RepositoryError::Conflict(message.to_owned())
}

pub(crate) async fn state_in_tx(
    tx: &mut Transaction<'_, Db>,
    list_id: i64,
) -> Result<AddonListSelectionState> {
    let row = sqlx::query(r#"SELECT "PolicyJson", "RefreshPaused", "ReviewRequired", "EvaluatedAt", "CutoffAt" FROM "AddonMediaListSelection" WHERE "AddonMediaListId"=?"#)
        .bind(list_id).fetch_optional(&mut **tx).await.repository()?;
    let Some(row) = row else {
        return Ok(AddonListSelectionState::default());
    };
    let policy = row
        .try_get::<Option<String>, _>("PolicyJson")
        .repository()?
        .map(|s| {
            serde_json::from_str::<AddonListSelectionPolicy>(&s)
                .map_err(|_| invalid("invalid saved selection policy"))
        })
        .transpose()?;
    if let Some(policy) = &policy {
        policy.validate().map_err(invalid)?;
    }
    Ok(AddonListSelectionState {
        review_required: row.try_get("ReviewRequired").repository()?,
        settings: AddonListSelectionSettings {
            policy,
            refresh_paused: row.try_get("RefreshPaused").repository()?,
        },
        evaluated_at: row
            .try_get::<Option<String>, _>("EvaluatedAt")
            .repository()?
            .map(|s| decode_datetime("selection evaluation", &s))
            .transpose()?,
        cutoff_at: row
            .try_get::<Option<String>, _>("CutoffAt")
            .repository()?
            .map(|s| decode_datetime("selection cutoff", &s))
            .transpose()?,
    })
}

pub(crate) async fn request_review_in_tx(tx: &mut Transaction<'_, Db>, list_id: i64) -> Result<()> {
    let eligible: bool = sqlx::query_scalar(r#"SELECT "PublishedRevision"=0 AND "PublicationJournalJson" IS NULL AND NOT EXISTS (SELECT 1 FROM "AddonMediaListPublicationJob" AS "job" JOIN "ManagedLibraryOperation" AS "operation" ON "operation"."Id"="job"."OperationId" WHERE "job"."AddonMediaListId"="AddonMediaList"."Id" AND "operation"."State" IN (0,1,2)) FROM "AddonMediaList" WHERE "Id"=?"#)
        .bind(list_id).fetch_one(&mut **tx).await.repository()?;
    if !eligible {
        return Err(invalid(
            "initial selection review requires an unpublished list",
        ));
    }
    sqlx::query(r#"INSERT INTO "AddonMediaListSelection" ("AddonMediaListId","ReviewRequired") VALUES (?,1) ON CONFLICT ("AddonMediaListId") DO UPDATE SET "ReviewRequired"=1"#)
        .bind(list_id).execute(&mut **tx).await.repository()?;
    Ok(())
}

pub(crate) async fn load_state(pool: &DbPool, list_id: i64) -> Result<AddonListSelectionState> {
    // A read-only transaction keeps the policy and timestamps in one snapshot.
    let mut tx = pool.begin().await.repository()?;
    let state = state_in_tx(&mut tx, list_id).await?;
    tx.commit().await.repository()?;
    Ok(state)
}

pub(crate) async fn preview_in_tx(
    tx: &mut Transaction<'_, Db>,
    list_id: i64,
    policy: Option<&AddonListSelectionPolicy>,
    now: DateTime<Utc>,
) -> Result<AddonListSelectionPreview> {
    let rows = sqlx::query(r#"SELECT "Id", "Rank", "DisplayTitle", "ProviderMetadataJson", "MetadataOverrideJson", "DurationSeconds", "SelectionOverride", "Included" FROM "AddonMediaListItem" WHERE "AddonMediaListId"=? ORDER BY "Rank", "Id" LIMIT 10001"#)
        .bind(list_id).fetch_all(&mut **tx).await.repository()?;
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let metadata = crate::repositories::addon_media_list::metadata_form_from_json(
            &row.try_get::<String, _>("ProviderMetadataJson")
                .repository()?,
            &row.try_get::<String, _>("MetadataOverrideJson")
                .repository()?,
            None,
        )?
        .metadata;
        items.push(AddonListSelectionItem {
            id: row.try_get("Id").repository()?,
            rank: row.try_get("Rank").repository()?,
            title: if metadata.title.trim().is_empty() {
                row.try_get("DisplayTitle").repository()?
            } else {
                metadata.title
            },
            published_at: metadata.release_date.as_deref().and_then(release_date),
            duration_seconds: row
                .try_get::<Option<i64>, _>("DurationSeconds")
                .repository()?
                .and_then(|n| u64::try_from(n).ok()),
            manual: row.try_get("SelectionOverride").repository()?,
            legacy_included: row.try_get("Included").repository()?,
        });
    }
    addon_list_selection::preview(policy, &items, now).map_err(invalid)
}

fn release_date(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|t| t.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .ok()?
                .and_hms_opt(0, 0, 0)
                .map(|t| t.and_utc())
        })
}

pub(crate) async fn save_in_tx(
    tx: &mut Transaction<'_, Db>,
    list_id: i64,
    settings: &AddonListSelectionSettings,
    now: DateTime<Utc>,
) -> Result<()> {
    let previous = state_in_tx(tx, list_id).await?;
    if let Some(policy) = &settings.policy {
        policy.validate().map_err(invalid)?;
    }
    let json = settings
        .policy
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|_| invalid("cannot encode selection policy"))?;
    sqlx::query(r#"INSERT INTO "AddonMediaListSelection" ("AddonMediaListId", "PolicyJson", "RefreshPaused") VALUES (?, ?, ?) ON CONFLICT ("AddonMediaListId") DO UPDATE SET "PolicyJson"=excluded."PolicyJson", "RefreshPaused"=excluded."RefreshPaused", "ReviewRequired"=0"#)
        .bind(list_id).bind(json).bind(settings.refresh_paused).execute(&mut **tx).await.repository()?;
    if settings.policy.is_some() || previous.settings.policy.is_some() {
        let fallback = AddonListSelectionPolicy::default();
        apply_policy_in_tx(
            tx,
            list_id,
            Some(settings.policy.as_ref().unwrap_or(&fallback)),
            now,
        )
        .await?;
    }
    Ok(())
}

pub(crate) async fn apply_in_tx(
    tx: &mut Transaction<'_, Db>,
    list_id: i64,
    now: DateTime<Utc>,
) -> Result<bool> {
    let state = state_in_tx(tx, list_id).await?;
    if state.settings.policy.is_none() {
        return Ok(false);
    }
    apply_policy_in_tx(tx, list_id, state.settings.policy.as_ref(), now).await
}

async fn apply_policy_in_tx(
    tx: &mut Transaction<'_, Db>,
    list_id: i64,
    policy: Option<&AddonListSelectionPolicy>,
    now: DateTime<Utc>,
) -> Result<bool> {
    let preview = preview_in_tx(tx, list_id, policy, now).await?;
    let previous = sqlx::query_as::<_, (i64, bool, Option<String>)>(r#"SELECT "Id","Included","SelectionReason" FROM "AddonMediaListItem" WHERE "AddonMediaListId"=? LIMIT 10001"#)
        .bind(list_id).fetch_all(&mut **tx).await.repository()?.into_iter()
        .map(|(id, included, reason)|(id,(included,reason))).collect::<std::collections::BTreeMap<_,_>>();
    let mut changed = false;
    for decision in preview.decisions {
        let old = previous
            .get(&decision.item_id)
            .ok_or_else(|| invalid("selection item disappeared"))?;
        let code = decision.reason.code();
        if old.0 == decision.accepted && old.1.as_deref() == Some(code) {
            continue;
        }
        sqlx::query(r#"UPDATE "AddonMediaListItem" SET "Included"=?, "SelectionReason"=? WHERE "Id"=? AND "AddonMediaListId"=?"#)
            .bind(decision.accepted).bind(code).bind(decision.item_id).bind(list_id).execute(&mut **tx).await.repository()?;
        changed |= old.0 != decision.accepted;
    }
    sqlx::query(r#"UPDATE "AddonMediaListSelection" SET "EvaluatedAt"=?, "CutoffAt"=? WHERE "AddonMediaListId"=?"#)
        .bind(encode_datetime(preview.evaluated_at)).bind(preview.cutoff_at.map(encode_datetime)).bind(list_id).execute(&mut **tx).await.repository()?;
    Ok(changed)
}

/// Use the existing single-item detail contract for plausible candidates only.
pub(crate) async fn queue_missing_facts(pool: &DbPool, list_id: i64) -> Result<()> {
    let ids = sqlx::query_scalar::<_,i64>(r#"SELECT "item"."Id" FROM "AddonMediaListItem" AS "item" JOIN "AddonMediaListSelection" AS "selection" ON "selection"."AddonMediaListId"="item"."AddonMediaListId" JOIN "AddonMediaListDetailJob" AS "detail" ON "detail"."AddonMediaListItemId"="item"."Id" WHERE "item"."AddonMediaListId"=? AND "selection"."PolicyJson" IS NOT NULL AND "selection"."ReviewRequired"=0 AND EXISTS (SELECT 1 FROM "AddonMediaList" AS "list" WHERE "list"."Id"="item"."AddonMediaListId" AND "list"."DetailImportPolicy"=0) AND "item"."SelectionReason" IN ('missing_date','missing_duration') AND "detail"."State" IN (0,1,2,4) AND "detail"."ResultRecordJson" IS NULL AND "detail"."OptionPurpose"<>1 ORDER BY "item"."Rank","item"."Id" LIMIT 32"#)
        .bind(list_id)
        .fetch_all(pool).await.repository()?;
    let coordinator =
        crate::addon_media_list_import::AddonMediaListImportCoordinator::new(pool.clone());
    for id in ids {
        coordinator.request_selection_details(list_id, id).await?;
    }
    Ok(())
}

/// Reassess one rolling selection per worker tick, including unchanged sources.
pub(crate) async fn reassess_due(pool: &DbPool, now: DateTime<Utc>) -> Result<()> {
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    let id: Option<i64> = sqlx::query_scalar(r#"SELECT "selection"."AddonMediaListId" FROM "AddonMediaListSelection" AS "selection" JOIN "AddonMediaList" AS "list" ON "list"."Id"="selection"."AddonMediaListId" WHERE "selection"."PolicyJson" IS NOT NULL AND "selection"."RefreshPaused"=0 AND ("selection"."EvaluatedAt" IS NULL OR "selection"."EvaluatedAt"<=?) AND "list"."PublicationJournalJson" IS NULL AND NOT EXISTS (SELECT 1 FROM "AddonMediaListImportJob" AS "import" WHERE "import"."AddonMediaListId"="list"."Id" AND "import"."State" IN (0,1,2,4,5)) AND NOT EXISTS (SELECT 1 FROM "AddonMediaListPublicationJob" AS "job" JOIN "ManagedLibraryOperation" AS "operation" ON "operation"."Id"="job"."OperationId" WHERE "job"."AddonMediaListId"="list"."Id" AND "operation"."State" IN (0,1,2)) ORDER BY "selection"."EvaluatedAt", "selection"."AddonMediaListId" LIMIT 1"#)
        .bind(encode_datetime(now - chrono::Duration::hours(1))).fetch_optional(&mut *tx).await.repository()?;
    if let Some(id) = id {
        if apply_in_tx(&mut tx, id, now).await? {
            crate::repositories::addon_media_list::mark_list_ready_tx(&mut tx, id).await?;
        }
    }
    tx.commit().await.repository()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repositories::SqliteAddonMediaListRepository;
    use ersatzrs_core::repositories::AddonMediaListRepository as _;

    async fn fixture(url: &str) -> DbPool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(url)
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
        sqlx::raw_sql(r#"
            INSERT INTO "AddonInstallation" ("AddonId","Version","Enabled","PackageSha256","ContentSha256","ManifestJson","InstallPath","InstalledAt","UpdatedAt")
            VALUES ('org.example.selection','1.0.0',1,'package','content','{}','addons/selection','2026-10-05T00:00:00Z','2026-10-05T00:00:00Z');
            INSERT INTO "AddonMediaList" ("Id","AddonId","ProviderId","SourceUrl","Name","DraftRevision","PublishedRevision","ReviewState")
            VALUES (1,'org.example.selection','one','https://example.invalid/one','One',7,5,2),(2,'org.example.selection','two','https://example.invalid/two','Two',9,8,2);
            INSERT INTO "AddonMediaListItem" ("Id","AddonMediaListId","ProviderId","Rank","DisplayTitle","Title","ProviderMetadataJson","DurationSeconds","Published","Kind") VALUES
            (10,1,'old',0,'Old','Old','{"title":"Old","release_date":"2025-10-04"}',60,1,4),
            (11,1,'new',1,'New','New','{"title":"New","release_date":"2026-10-04"}',120,1,4),
            (20,2,'other',0,'Other','Other','{"title":"Other","release_date":"2025-10-04"}',60,1,4);
        "#).execute(&pool).await.unwrap();
        pool
    }

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-05T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }
    fn settings() -> AddonListSelectionSettings {
        AddonListSelectionSettings {
            policy: Some(AddonListSelectionPolicy {
                age_days: Some(365),
                newest_count: Some(1),
                ..Default::default()
            }),
            refresh_paused: false,
        }
    }

    #[tokio::test]
    async fn selection_preview_is_read_only_and_failed_settings_preserve_publication() {
        let pool = fixture("sqlite::memory:").await;
        let mut tx = crate::db::begin_write(&pool).await.unwrap();
        let preview = preview_in_tx(&mut tx, 1, settings().policy.as_ref(), now())
            .await
            .unwrap();
        assert_eq!(preview.accepted_count, 1);
        tx.commit().await.unwrap();
        let before: Vec<(i64, bool, bool)> = sqlx::query_as(
            r#"SELECT "Id","Included","Published" FROM "AddonMediaListItem" ORDER BY "Id""#,
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert!(before
            .iter()
            .all(|(_, included, published)| *included && *published));
        let mut tx = crate::db::begin_write(&pool).await.unwrap();
        let invalid = AddonListSelectionSettings {
            policy: Some(AddonListSelectionPolicy {
                newest_count: Some(0),
                ..Default::default()
            }),
            refresh_paused: false,
        };
        assert!(save_in_tx(&mut tx, 1, &invalid, now()).await.is_err());
        tx.rollback().await.unwrap();
        assert_eq!(
            sqlx::query_as::<_, (i64, bool, bool)>(
                r#"SELECT "Id","Included","Published" FROM "AddonMediaListItem" ORDER BY "Id""#
            )
            .fetch_all(&pool)
            .await
            .unwrap(),
            before
        );
        assert_eq!(
            load_state(&pool, 1).await.unwrap(),
            AddonListSelectionState::default()
        );
    }

    #[tokio::test]
    async fn selection_restart_retains_policy_cutoff_archives_and_manual_overrides() {
        let temporary = tempfile::tempdir().unwrap();
        let db_path = temporary.path().join("selection.db");
        let url = format!("sqlite:{}?mode=rwc", db_path.display());
        let pool = fixture(&url).await;
        let mut tx = crate::db::begin_write(&pool).await.unwrap();
        save_in_tx(&mut tx, 1, &settings(), now()).await.unwrap();
        tx.commit().await.unwrap();
        let rows: Vec<(i64, bool, bool)> = sqlx::query_as(
            r#"SELECT "Id","Included","Published" FROM "AddonMediaListItem" ORDER BY "Id""#,
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            rows,
            [(10, false, true), (11, true, true), (20, true, true)]
        );
        let saved = load_state(&pool, 1).await.unwrap();
        assert_eq!(saved.cutoff_at, Some(now() - chrono::Duration::days(365)));
        let repo = SqliteAddonMediaListRepository(pool.clone());
        repo.set_selection_override(1, 10, 7, Some(true))
            .await
            .unwrap();
        assert!(repo
            .set_selection_override(1, 11, 7, Some(false))
            .await
            .is_err());
        repo.set_selection_override(1, 11, 8, Some(false))
            .await
            .unwrap();
        pool.close().await;
        let restarted = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .unwrap();
        sqlx::migrate!("../../migrations")
            .run(&restarted)
            .await
            .unwrap();
        assert_eq!(
            load_state(&restarted, 1).await.unwrap().settings,
            settings()
        );
        let mut tx = crate::db::begin_write(&restarted).await.unwrap();
        assert!(!apply_in_tx(&mut tx, 1, now() + chrono::Duration::days(1))
            .await
            .unwrap());
        tx.commit().await.unwrap();
        let included: Vec<(i64, bool)> =
            sqlx::query_as(r#"SELECT "Id","Included" FROM "AddonMediaListItem" ORDER BY "Id""#)
                .fetch_all(&restarted)
                .await
                .unwrap();
        assert_eq!(included, [(10, true), (11, false), (20, true)]);
    }

    #[tokio::test]
    async fn selection_unchanged_sources_age_out_once_and_pause_retains_cadence() {
        let pool = fixture("sqlite::memory:").await;
        let mut tx = crate::db::begin_write(&pool).await.unwrap();
        let mut s = settings();
        s.refresh_paused = true;
        save_in_tx(&mut tx, 1, &s, now()).await.unwrap();
        tx.commit().await.unwrap();
        reassess_due(&pool, now() + chrono::Duration::days(366))
            .await
            .unwrap();
        assert_eq!(
            load_state(&pool, 1).await.unwrap().evaluated_at,
            Some(now())
        );
        let mut tx = crate::db::begin_write(&pool).await.unwrap();
        s.refresh_paused = false;
        save_in_tx(&mut tx, 1, &s, now()).await.unwrap();
        tx.commit().await.unwrap();
        reassess_due(&pool, now() + chrono::Duration::days(366))
            .await
            .unwrap();
        let revision: i32 =
            sqlx::query_scalar(r#"SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id"=1"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(revision, 8);
        reassess_due(&pool, now() + chrono::Duration::days(366))
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i32>(
                r#"SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id"=1"#
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            revision
        );
    }
}
