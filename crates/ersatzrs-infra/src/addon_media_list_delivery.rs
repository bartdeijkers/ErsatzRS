//! AMM-M-01B: saved delivery policy and durable first-import classification.
//! Publication owns freezing these choices; saving never changes active playback.

pub mod portable;

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::{AddonMediaListDeliveryMode, EntityId, RemoteStreamLiveness};
use sqlx::Transaction;

use crate::db::{Db, DbPool};
use crate::repositories::common::{decode_datetime, encode_datetime};
use crate::repositories::SqlxResultExt as _;

/// Saved policy, with a display timestamp and an ordering fence independent of clocks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedDeliveryPolicy {
    pub mode: AddonMediaListDeliveryMode,
    pub cutoff_at: Option<String>,
    pub cutoff_identity_id: Option<EntityId>,
}

impl SavedDeliveryPolicy {
    /// Classify a known identity. Only confirmed finite media may download.
    ///
    /// # Errors
    /// Rejects missing identity evidence or a malformed new-videos cutoff.
    pub fn effective_item_delivery(
        &self,
        first_import_identity_id: Option<EntityId>,
        liveness: RemoteStreamLiveness,
    ) -> Result<AddonMediaListDeliveryMode> {
        let identity = first_import_identity_id
            .filter(|id| *id > 0)
            .ok_or_else(|| {
                RepositoryError::Conflict("delivery first-import identity is unavailable".into())
            })?;
        if liveness != RemoteStreamLiveness::Finite {
            return Ok(AddonMediaListDeliveryMode::Stream);
        }
        match self.mode {
            AddonMediaListDeliveryMode::Stream => Ok(AddonMediaListDeliveryMode::Stream),
            AddonMediaListDeliveryMode::DownloadAhead => {
                Ok(AddonMediaListDeliveryMode::DownloadAhead)
            }
            AddonMediaListDeliveryMode::DownloadAheadNewVideos => {
                let cutoff = self
                    .cutoff_identity_id
                    .filter(|id| *id >= 0)
                    .ok_or_else(|| {
                        RepositoryError::Conflict(
                            "delivery first-import cutoff is unavailable".into(),
                        )
                    })?;
                Ok(if identity > cutoff {
                    AddonMediaListDeliveryMode::DownloadAhead
                } else {
                    AddonMediaListDeliveryMode::Stream
                })
            }
        }
    }
}

/// Read saved policy without modifying the publication or draft.
///
/// # Errors
/// Returns database, missing-list, or invalid-policy errors.
pub async fn load(pool: &DbPool, list_id: EntityId) -> Result<SavedDeliveryPolicy> {
    // Read-only transaction: retrieve the saved policy without modifying its fence.
    let mut tx = pool.begin().await.repository()?;
    let policy = load_in_tx(&mut tx, list_id).await?;
    tx.commit().await.repository()?;
    Ok(policy)
}

/// Read policy in the caller's existing transaction.
///
/// # Errors
/// Returns database, missing-list, or invalid-policy errors.
pub async fn load_in_tx(
    tx: &mut Transaction<'_, Db>,
    list_id: EntityId,
) -> Result<SavedDeliveryPolicy> {
    let row = sqlx::query_as::<_, (i32, Option<String>, Option<i64>)>(
        r#"SELECT "DeliveryMode", "DeliveryCutoffAt", "DeliveryCutoffIdentityId"
           FROM "AddonMediaList" WHERE "Id" = ?"#,
    )
    .bind(list_id)
    .fetch_optional(&mut **tx)
    .await
    .repository()?
    .ok_or(RepositoryError::NotFound {
        kind: "add-on media list",
        id: list_id,
    })?;
    let mode = match row.0 {
        0 => AddonMediaListDeliveryMode::Stream,
        1 => AddonMediaListDeliveryMode::DownloadAhead,
        2 => AddonMediaListDeliveryMode::DownloadAheadNewVideos,
        _ => return Err(invalid_policy()),
    };
    if mode == AddonMediaListDeliveryMode::DownloadAheadNewVideos {
        let at = row.1.as_deref().ok_or_else(invalid_policy)?;
        decode_datetime("delivery cutoff", at)?;
        if row.2.is_none_or(|id| id < 0) {
            return Err(invalid_policy());
        }
    } else if row.1.is_some() || row.2.is_some() {
        return Err(invalid_policy());
    }
    Ok(SavedDeliveryPolicy {
        mode,
        cutoff_at: row.1,
        cutoff_identity_id: row.2,
    })
}

fn invalid_policy() -> RepositoryError {
    RepositoryError::Conflict("stored delivery policy is invalid".into())
}

/// Save policy inside the existing fenced list-save transaction.
///
/// The caller must first validate and acquire its draft/publication write fence
/// in this same transaction, and owns revision bumps and commit. This helper is
/// not an independent save endpoint. The single UPDATE serializes the cohort
/// boundary with item insertion; wall-clock ordering never decides newness.
///
/// # Errors
/// Returns database, missing-list, or invalid-policy errors.
pub async fn save_in_tx(
    tx: &mut Transaction<'_, Db>,
    list_id: EntityId,
    mode: AddonMediaListDeliveryMode,
    now: DateTime<Utc>,
) -> Result<SavedDeliveryPolicy> {
    let value = match mode {
        AddonMediaListDeliveryMode::Stream => 0,
        AddonMediaListDeliveryMode::DownloadAhead => 1,
        AddonMediaListDeliveryMode::DownloadAheadNewVideos => 2,
    };
    sqlx::query(
        r#"UPDATE "AddonMediaList" SET
            "DeliveryCutoffAt" = CASE
                WHEN ? != 2 THEN NULL
                WHEN "DeliveryMode" = 2 THEN "DeliveryCutoffAt"
                ELSE ? END,
            "DeliveryCutoffIdentityId" = CASE
                WHEN ? != 2 THEN NULL
                WHEN "DeliveryMode" = 2 THEN "DeliveryCutoffIdentityId"
                ELSE (SELECT COALESCE(MAX("Id"), 0) FROM "AddonMediaListKnownIdentity"
                      WHERE "AddonMediaListId" = ?) END,
            "DeliveryMode" = ?
           WHERE "Id" = ?"#,
    )
    .bind(value)
    .bind(encode_datetime(now))
    .bind(value)
    .bind(list_id)
    .bind(value)
    .bind(list_id)
    .execute(&mut **tx)
    .await
    .repository()?;
    load_in_tx(tx, list_id).await
}

/// Start clean first-import evidence for an explicit list reset.
///
/// Call only after removing the list's item rows in the same fenced write
/// transaction. Ordinary refresh, item removal and revision restore must never
/// call this helper. The selected mode is retained; new-videos-only starts a
/// fresh empty cohort before replacement items can be inserted.
///
/// # Errors
/// Rejects remaining item rows, missing lists and database failures.
pub async fn reset_identity_in_tx(
    tx: &mut Transaction<'_, Db>,
    list_id: EntityId,
    now: DateTime<Utc>,
) -> Result<SavedDeliveryPolicy> {
    let has_items: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ?)"#,
    )
    .bind(list_id)
    .fetch_one(&mut **tx)
    .await
    .repository()?;
    if has_items {
        return Err(RepositoryError::Conflict(
            "remove list items before resetting delivery identities".into(),
        ));
    }
    sqlx::query(r#"DELETE FROM "AddonMediaListKnownIdentity" WHERE "AddonMediaListId" = ?"#)
        .bind(list_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    sqlx::query(
        r#"UPDATE "AddonMediaList" SET
               "DeliveryCutoffAt" = CASE WHEN "DeliveryMode" = 2 THEN ? ELSE NULL END,
               "DeliveryCutoffIdentityId" = CASE WHEN "DeliveryMode" = 2 THEN 0 ELSE NULL END
           WHERE "Id" = ?"#,
    )
    .bind(encode_datetime(now))
    .bind(list_id)
    .execute(&mut **tx)
    .await
    .repository()?;
    load_in_tx(tx, list_id).await
}

/// Classify one item using caller-resolved effective liveness.
///
/// # Errors
/// Fails closed when policy or first-import identity evidence is unavailable.
pub async fn effective_item_delivery_in_tx(
    tx: &mut Transaction<'_, Db>,
    list_id: EntityId,
    provider_id: &str,
    liveness: RemoteStreamLiveness,
) -> Result<AddonMediaListDeliveryMode> {
    let policy = load_in_tx(tx, list_id).await?;
    let identity = sqlx::query_scalar::<_, i64>(
        r#"SELECT "Id" FROM "AddonMediaListKnownIdentity"
           WHERE "AddonMediaListId" = ? AND "ProviderId" = ?"#,
    )
    .bind(list_id)
    .bind(provider_id)
    .fetch_optional(&mut **tx)
    .await
    .repository()?;
    policy.effective_item_delivery(identity, liveness)
}

/// Batch classify current items from operator/provider liveness, without N+1 reads.
/// Inclusion, availability, video kind and provider capability checks remain with
/// the existing admission/publication caller.
///
/// # Errors
/// Rejects invalid liveness, policy or missing identity evidence.
pub async fn load_item_deliveries_in_tx(
    tx: &mut Transaction<'_, Db>,
    list_id: EntityId,
) -> Result<BTreeMap<String, AddonMediaListDeliveryMode>> {
    let policy = load_in_tx(tx, list_id).await?;
    let rows = sqlx::query_as::<_, (String, i32, Option<i64>)>(
        r#"SELECT "item"."ProviderId",
                  COALESCE("item"."LivenessOverride", "item"."ProviderLiveness"), "known"."Id"
           FROM "AddonMediaListItem" AS "item"
           LEFT JOIN "AddonMediaListKnownIdentity" AS "known"
             ON "known"."AddonMediaListId" = "item"."AddonMediaListId"
            AND "known"."ProviderId" = "item"."ProviderId"
           WHERE "item"."AddonMediaListId" = ?"#,
    )
    .bind(list_id)
    .fetch_all(&mut **tx)
    .await
    .repository()?;
    rows.into_iter()
        .map(|(provider_id, liveness, identity)| {
            let liveness = match liveness {
                0 => RemoteStreamLiveness::Unknown,
                1 => RemoteStreamLiveness::Finite,
                2 => RemoteStreamLiveness::Live,
                _ => {
                    return Err(RepositoryError::Conflict(
                        "stored delivery liveness is invalid".into(),
                    ))
                }
            };
            Ok((
                provider_id,
                policy.effective_item_delivery(identity, liveness)?,
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::{sqlite::SqlitePoolOptions, Executor as _};

    const DELIVERY_MIGRATION: i64 = 202_609_071_800;

    #[path = "portable.rs"]
    mod portable_tests;

    async fn database(include_delivery: bool) -> DbPool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        pool.execute("PRAGMA foreign_keys = ON").await.unwrap();
        let migrations = sqlx::migrate!("../../migrations");
        let selected = sqlx::migrate::Migrator::with_migrations(
            migrations
                .iter()
                .filter(|migration| include_delivery || migration.version < DELIVERY_MIGRATION)
                .cloned()
                .collect(),
        );
        selected.run(&pool).await.unwrap();
        seed_lists(&pool).await;
        pool
    }

    async fn seed_lists(pool: &DbPool) {
        sqlx::raw_sql(
            r#"
            INSERT INTO "AddonInstallation" (
                "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
                "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt"
            ) VALUES ('org.example.delivery', '1.0.0', 1, 'package', 'content', '{}',
                'addons/delivery', '2026-09-07T00:00:00Z', '2026-09-07T00:00:00Z');
            INSERT INTO "AddonMediaList" ("Id", "AddonId", "ProviderId", "SourceUrl", "Name")
            VALUES (1, 'org.example.delivery', 'one', 'https://example.invalid/one', 'One'),
                   (2, 'org.example.delivery', 'two', 'https://example.invalid/two', 'Two');
        "#,
        )
        .execute(pool)
        .await
        .unwrap();
    }

    async fn insert_item(pool: &DbPool, list_id: EntityId, provider_id: &str) {
        sqlx::query(
            r#"INSERT INTO "AddonMediaListItem" (
            "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind", "Rank", "Title",
            "ProviderLiveness", "EffectiveLiveness"
        ) VALUES (?, ?, 'Fixture', 4, 0, 'Fixture', 1, 1)"#,
        )
        .bind(list_id)
        .bind(provider_id)
        .execute(pool)
        .await
        .unwrap();
    }

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-09-07T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    async fn save(
        pool: &DbPool,
        mode: AddonMediaListDeliveryMode,
        at: DateTime<Utc>,
    ) -> SavedDeliveryPolicy {
        let mut tx = pool.begin().await.unwrap();
        let policy = save_in_tx(&mut tx, 1, mode, at).await.unwrap();
        tx.commit().await.unwrap();
        policy
    }

    async fn deliveries(pool: &DbPool) -> BTreeMap<String, AddonMediaListDeliveryMode> {
        let mut tx = pool.begin().await.unwrap();
        let result = load_item_deliveries_in_tx(&mut tx, 1).await.unwrap();
        tx.commit().await.unwrap();
        result
    }

    #[tokio::test]
    async fn delivery_explicit_reset_requires_empty_items_and_retains_mode() {
        let pool = database(true).await;
        for mode in [
            AddonMediaListDeliveryMode::Stream,
            AddonMediaListDeliveryMode::DownloadAhead,
            AddonMediaListDeliveryMode::DownloadAheadNewVideos,
        ] {
            insert_item(&pool, 1, "old").await;
            let previous = save(&pool, mode, now()).await;
            let mut tx = pool.begin().await.unwrap();
            assert!(reset_identity_in_tx(&mut tx, 1, now()).await.is_err());
            tx.rollback().await.unwrap();
            assert_eq!(load(&pool, 1).await.unwrap(), previous);
            let mut tx = pool.begin().await.unwrap();
            sqlx::query(r#"DELETE FROM "AddonMediaListItem" WHERE "AddonMediaListId" = 1"#)
                .execute(&mut *tx)
                .await
                .unwrap();
            let reset_at = now() + chrono::Duration::hours(1);
            let reset = reset_identity_in_tx(&mut tx, 1, reset_at).await.unwrap();
            assert_eq!(reset.mode, mode);
            if mode == AddonMediaListDeliveryMode::DownloadAheadNewVideos {
                assert_eq!(reset.cutoff_identity_id, Some(0));
                assert_eq!(reset.cutoff_at, Some(encode_datetime(reset_at)));
            } else {
                assert_eq!(reset.cutoff_identity_id, None);
                assert_eq!(reset.cutoff_at, None);
            }
            let count: i64 = sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM "AddonMediaListKnownIdentity"
                                                 WHERE "AddonMediaListId" = 1"#,
            )
            .fetch_one(&mut *tx)
            .await
            .unwrap();
            assert_eq!(count, 0);
            tx.commit().await.unwrap();
            insert_item(&pool, 1, "old").await;
            let expected = if mode == AddonMediaListDeliveryMode::Stream {
                AddonMediaListDeliveryMode::Stream
            } else {
                AddonMediaListDeliveryMode::DownloadAhead
            };
            assert_eq!(deliveries(&pool).await["old"], expected);
            sqlx::query(r#"DELETE FROM "AddonMediaListItem" WHERE "AddonMediaListId" = 1"#)
                .execute(&pool)
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn delivery_policy_and_known_identities_survive_database_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(directory.path().join("delivery.sqlite"))
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options.clone())
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
        seed_lists(&pool).await;
        insert_item(&pool, 1, "old").await;
        let before = save(
            &pool,
            AddonMediaListDeliveryMode::DownloadAheadNewVideos,
            now(),
        )
        .await;
        sqlx::query(r#"DELETE FROM "AddonMediaListItem" WHERE "AddonMediaListId" = 1"#)
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        let reopened = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        assert_eq!(load(&reopened, 1).await.unwrap(), before);
        insert_item(&reopened, 1, "old").await;
        insert_item(&reopened, 1, "new").await;
        assert_eq!(
            deliveries(&reopened).await,
            BTreeMap::from([
                ("old".into(), AddonMediaListDeliveryMode::Stream),
                ("new".into(), AddonMediaListDeliveryMode::DownloadAhead),
            ])
        );
        reopened.close().await;
    }

    #[tokio::test]
    async fn delivery_cutoff_preserves_same_mode_and_resets_on_reenable() {
        use AddonMediaListDeliveryMode::{DownloadAhead, DownloadAheadNewVideos, Stream};
        let pool = database(true).await;
        assert_eq!(load(&pool, 1).await.unwrap().mode, Stream);
        insert_item(&pool, 1, "old").await;
        let first = save(&pool, DownloadAheadNewVideos, now()).await;
        assert!(first.cutoff_at.is_some());
        assert!(first.cutoff_identity_id.is_some_and(|id| id > 0));
        // Identical wall time still has an unambiguous database ordering boundary.
        insert_item(&pool, 1, "new").await;
        assert_eq!(
            deliveries(&pool).await,
            BTreeMap::from([("new".into(), DownloadAhead), ("old".into(), Stream),])
        );
        assert_eq!(
            save(
                &pool,
                DownloadAheadNewVideos,
                now() + chrono::Duration::hours(1)
            )
            .await,
            first
        );
        assert_eq!(load(&pool, 1).await.unwrap(), first);
        let disabled = save(&pool, Stream, now()).await;
        assert_eq!(disabled.cutoff_at, None);
        assert_eq!(disabled.cutoff_identity_id, None);
        let again = save(
            &pool,
            DownloadAheadNewVideos,
            now() + chrono::Duration::hours(2),
        )
        .await;
        assert!(again.cutoff_identity_id > first.cutoff_identity_id);
        assert_ne!(again.cutoff_at, first.cutoff_at);
        assert!(deliveries(&pool).await.values().all(|mode| *mode == Stream));
        insert_item(&pool, 1, "new-again").await;
        assert_eq!(deliveries(&pool).await["new-again"], DownloadAhead);
        save(&pool, DownloadAhead, now()).await;
        assert!(deliveries(&pool)
            .await
            .values()
            .all(|mode| *mode == DownloadAhead));
    }

    #[tokio::test]
    async fn delivery_identity_survives_item_recreation_and_transaction_rollback() {
        use AddonMediaListDeliveryMode::{DownloadAhead, DownloadAheadNewVideos, Stream};
        let pool = database(true).await;
        insert_item(&pool, 1, "old").await;
        save(&pool, DownloadAheadNewVideos, now()).await;
        sqlx::query(r#"DELETE FROM "AddonMediaListItem" WHERE "AddonMediaListId" = 1"#)
            .execute(&pool)
            .await
            .unwrap();
        insert_item(&pool, 1, "old").await;
        insert_item(&pool, 1, "new").await;
        assert_eq!(deliveries(&pool).await["old"], Stream);
        assert_eq!(deliveries(&pool).await["new"], DownloadAhead);
        // Identity-changing updates keep the old identity and record the new one.
        sqlx::query(
            r#"UPDATE "AddonMediaListItem" SET "ProviderId" = 'renamed'
                       WHERE "AddonMediaListId" = 1 AND "ProviderId" = 'new'"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        insert_item(&pool, 1, "new").await;
        let count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "AddonMediaListKnownIdentity"
                                              WHERE "AddonMediaListId" = 1"#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 3);
        let before = load(&pool, 1).await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        save_in_tx(&mut tx, 1, Stream, now()).await.unwrap();
        tx.rollback().await.unwrap();
        assert_eq!(load(&pool, 1).await.unwrap(), before);
        // List identity is part of the key: the same provider ID is new elsewhere.
        let mut tx = pool.begin().await.unwrap();
        let empty = save_in_tx(&mut tx, 2, DownloadAheadNewVideos, now())
            .await
            .unwrap();
        assert_eq!(empty.cutoff_identity_id, Some(0));
        tx.commit().await.unwrap();
        insert_item(&pool, 2, "old").await;
        let mut tx = pool.begin().await.unwrap();
        assert_eq!(
            effective_item_delivery_in_tx(&mut tx, 2, "old", RemoteStreamLiveness::Finite)
                .await
                .unwrap(),
            DownloadAhead
        );
        tx.commit().await.unwrap();
    }

    #[tokio::test]
    async fn delivery_classification_is_finite_only_and_fails_closed_without_identity() {
        let pool = database(true).await;
        insert_item(&pool, 1, "finite").await;
        insert_item(&pool, 1, "live").await;
        insert_item(&pool, 1, "unknown").await;
        insert_item(&pool, 1, "overridden").await;
        sqlx::query(
            r#"UPDATE "AddonMediaListItem" SET "ProviderLiveness" = CASE
            WHEN "ProviderId" = 'live' THEN 2 WHEN "ProviderId" = 'unknown' THEN 0 ELSE 1 END"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"UPDATE "AddonMediaListItem" SET "LivenessOverride" = 2
                       WHERE "ProviderId" = 'overridden'"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        save(&pool, AddonMediaListDeliveryMode::DownloadAhead, now()).await;
        assert_eq!(
            deliveries(&pool).await,
            BTreeMap::from([
                ("finite".into(), AddonMediaListDeliveryMode::DownloadAhead),
                ("live".into(), AddonMediaListDeliveryMode::Stream),
                ("unknown".into(), AddonMediaListDeliveryMode::Stream),
                ("overridden".into(), AddonMediaListDeliveryMode::Stream),
            ])
        );
        let mut tx = pool.begin().await.unwrap();
        assert!(
            effective_item_delivery_in_tx(&mut tx, 1, "missing", RemoteStreamLiveness::Finite)
                .await
                .is_err()
        );
        tx.rollback().await.unwrap();
        assert!(
            sqlx::query(r#"UPDATE "AddonMediaList" SET "DeliveryMode" = 2 WHERE "Id" = 1"#)
                .execute(&pool)
                .await
                .is_err()
        );
        assert!(
            sqlx::query(r#"UPDATE "AddonMediaList" SET "DeliveryMode" = 3 WHERE "Id" = 1"#)
                .execute(&pool)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn delivery_migration_backfills_current_and_revision_only_identities() {
        let pool = database(false).await;
        insert_item(&pool, 1, "current").await;
        sqlx::raw_sql(r#"
            INSERT INTO "AddonMediaListRevision" ("Id", "AddonMediaListId", "Revision")
            VALUES (1, 1, 1), (2, 1, 2);
            INSERT INTO "AddonMediaListRevisionItem" (
                "AddonMediaListRevisionId", "ProviderId", "Rank", "Kind", "ContentKind",
                "Included", "Published", "DisplayTitle", "Title", "ChapterInput", "ProviderChapterInput",
                "CollectionRole", "Availability", "ProviderLiveness", "EffectiveLiveness",
                "MetadataJson", "MetadataDigest"
            ) VALUES
                (1, 'historical', 0, 4, 4, 1, 1, 'Fixture', 'Fixture', '', '', 0, 0, 1, 1, '{}', 'fixture'),
                (2, 'historical', 0, 4, 4, 1, 1, 'Fixture', 'Fixture', '', '', 0, 0, 1, 1, '{}', 'fixture'),
                (2, 'current', 1, 4, 4, 1, 1, 'Fixture', 'Fixture', '', '', 0, 0, 1, 1, '{}', 'fixture');
        "#).execute(&pool).await.unwrap();
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
        assert_eq!(
            load(&pool, 1).await.unwrap().mode,
            AddonMediaListDeliveryMode::Stream
        );
        let count: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaListKnownIdentity""#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 2);
        save(
            &pool,
            AddonMediaListDeliveryMode::DownloadAheadNewVideos,
            now(),
        )
        .await;
        // Restore recreates the row; migration evidence keeps its original cohort.
        insert_item(&pool, 1, "historical").await;
        assert_eq!(
            deliveries(&pool).await["historical"],
            AddonMediaListDeliveryMode::Stream
        );
        sqlx::query(r#"DELETE FROM "AddonMediaList" WHERE "Id" = 1"#)
            .execute(&pool)
            .await
            .unwrap();
        let count: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaListKnownIdentity""#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 0);
    }
}
