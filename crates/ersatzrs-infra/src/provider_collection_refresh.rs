//! Durable handoff from atomic provider collection publication to the existing
//! playout refresh sweep. No provider requests run during dependency resolution.

use std::collections::BTreeSet;

use chrono::Utc;
use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::{EntityId, ScanPlayoutRefreshReason};
use sqlx::{Row, Transaction};

use crate::{
    db::{begin_write, Db, DbPool},
    repositories::{SqliteSchedulingContentRepository, SqlxResultExt as _},
    scan_refresh::persist_unowned_refresh_intents,
};

/// Retain changed media and old dependency matches in the publication transaction.
/// Empty changes create no work. IDs contain no provider credentials or paths.
///
/// # Errors
/// Returns an error if the durable handoff cannot be recorded.
pub async fn persist_resolution(
    tx: &mut Transaction<'_, Db>,
    changed_media: &BTreeSet<EntityId>,
    previous_playouts: &[EntityId],
) -> Result<()> {
    persist_resolution_with_reason(
        tx,
        changed_media,
        previous_playouts,
        ScanPlayoutRefreshReason::ProviderCollectionChanged,
    )
    .await
}

async fn persist_resolution_with_reason(
    tx: &mut Transaction<'_, Db>,
    changed_media: &BTreeSet<EntityId>,
    previous_playouts: &[EntityId],
    reason: ScanPlayoutRefreshReason,
) -> Result<()> {
    if changed_media.is_empty() {
        return Ok(());
    }
    sqlx::query(
        r#"INSERT INTO "ProviderCollectionRefreshResolution"
        ("ChangedMediaIds", "PreviousPlayoutIds", "CreatedAt", "Reason") VALUES (?, ?, ?, ?)"#,
    )
    .bind(serde_json::json!(changed_media).to_string())
    .bind(serde_json::json!(previous_playouts).to_string())
    .bind(Utc::now().to_rfc3339())
    .bind(reason.as_str())
    .execute(&mut **tx)
    .await
    .repository()?;
    Ok(())
}

fn decode_ids(value: &str) -> Result<BTreeSet<EntityId>> {
    serde_json::from_str(value).map_err(|_| RepositoryError::Decode {
        kind: "provider collection refresh",
        value: "invalid persisted dependency identities".to_owned(),
    })
}

/// Transfer a bounded batch of publications to the existing durable refresh queue.
///
/// A cancellation or resolution failure leaves the publication record intact.
/// Completion and intent creation commit together. Concurrent sweeps cannot
/// transfer the same record twice; a new publication has its own stable ID.
///
/// # Errors
/// Returns an error if dependencies cannot be resolved or the transfer fails.
pub async fn resolve_pending(pool: &DbPool) -> Result<usize> {
    let rows = sqlx::query(
        r#"SELECT "Id", "ChangedMediaIds", "PreviousPlayoutIds", "Reason"
           FROM "ProviderCollectionRefreshResolution" ORDER BY "Id" LIMIT 32"#,
    )
    .fetch_all(pool)
    .await
    .repository()?;
    let repository = SqliteSchedulingContentRepository::new(pool.clone());
    let mut completed = 0;
    for row in rows {
        let id: EntityId = row.try_get("Id").repository()?;
        let reason = ScanPlayoutRefreshReason::from_token(row.try_get("Reason").repository()?)
            .ok_or_else(|| RepositoryError::Decode {
                kind: "provider refresh reason",
                value: "invalid persisted reason".to_owned(),
            })?;
        let changed = decode_ids(row.try_get("ChangedMediaIds").repository()?)?;
        let mut affected = decode_ids(row.try_get("PreviousPlayoutIds").repository()?)?;
        affected.extend(
            repository
                .classic_playouts_affected_by_scan(&changed, None)
                .await?,
        );
        let mut tx = begin_write(pool).await.repository()?;
        let removed =
            sqlx::query(r#"DELETE FROM "ProviderCollectionRefreshResolution" WHERE "Id" = ?"#)
                .bind(id)
                .execute(&mut *tx)
                .await
                .repository()?;
        if removed.rows_affected() == 0 {
            tx.rollback().await.repository()?;
            continue;
        }
        // A playout may have been deleted since publication; never resurrect it
        // or let its missing foreign key strand refreshes for surviving playouts.
        let existing: Vec<EntityId> = sqlx::query_scalar(
            r#"SELECT "Id" FROM "Playout" WHERE "Id" IN (SELECT value FROM json_each(?))
               AND "ScheduleKind" IN (?, ?) ORDER BY "Id""#,
        )
        .bind(serde_json::json!(affected).to_string())
        .bind(ersatzrs_domain::PlayoutScheduleKind::Classic as i32)
        .bind("Classic")
        .fetch_all(&mut *tx)
        .await
        .repository()?;
        persist_unowned_refresh_intents(&mut tx, reason, &existing)
            .await
            .repository()?;
        tx.commit().await.repository()?;
        completed += 1;
    }
    Ok(completed)
}

/// Semantic membership/tag projection, excluding volatile row IDs and HTTP validators.
/// Comparing inside the publication transaction suppresses unchanged scans.
pub(crate) struct PublicationState(std::collections::BTreeMap<EntityId, Vec<String>>);

impl PublicationState {
    pub(crate) async fn capture(
        tx: &mut Transaction<'_, Db>,
        provider: crate::collection_tags::Provider,
        source: EntityId,
    ) -> Result<Self> {
        let (table, tag_type) = match provider {
            crate::collection_tags::Provider::Plex => ("PlexCollection", "plex/collection"),
            crate::collection_tags::Provider::Jellyfin => {
                ("JellyfinCollection", "jellyfin/collection")
            }
            crate::collection_tags::Provider::Emby => ("EmbyCollection", "emby/collection"),
        };
        let mut media = std::collections::BTreeMap::<EntityId, Vec<String>>::new();
        let rows: Vec<(EntityId, String)> = sqlx::query_as(crate::db::assert_sql_safe(format!(
            r#"SELECT i."MediaItemId", json_array('member', p."Id", c."Name", i."CustomIndex")
            FROM "{table}" p JOIN "Collection" c ON c."Id" = p."Id"
            JOIN "CollectionItem" i ON i."CollectionId" = p."Id"
            WHERE p."MediaSourceId" = ?"#
        )))
        .bind(source)
        .fetch_all(&mut **tx)
        .await
        .repository()?;
        for (id, value) in rows {
            media.entry(id).or_default().push(value);
        }
        for kind in ["Movie", "Show", "Season", "Episode", "OtherVideo"] {
            let rows: Vec<(EntityId, String)> =
                sqlx::query_as(crate::db::assert_sql_safe(format!(
                    r#"SELECT m."{kind}Id", json_array('tag', t."Name", t."ExternalCollectionId")
                FROM "Tag" t JOIN "{kind}Metadata" m ON m."Id" = t."{kind}MetadataId"
                JOIN "MediaItem" mi ON mi."Id" = m."{kind}Id"
                JOIN "LibraryPath" path ON path."Id" = mi."LibraryPathId"
                JOIN "Library" library ON library."Id" = path."LibraryId"
                WHERE library."MediaSourceId" = ? AND t."ExternalTypeId" = ?"#
                )))
                .bind(source)
                .bind(tag_type)
                .fetch_all(&mut **tx)
                .await
                .repository()?;
            for (id, value) in rows {
                media.entry(id).or_default().push(value);
            }
        }
        for values in media.values_mut() {
            values.sort();
        }
        Ok(Self(media))
    }

    pub(crate) async fn persist_changes(
        self,
        tx: &mut Transaction<'_, Db>,
        after: Self,
        previous_dependencies: &crate::repositories::ClassicMediaDependencies,
    ) -> Result<()> {
        let changed: BTreeSet<EntityId> = self
            .0
            .keys()
            .chain(after.0.keys())
            .filter(|id| self.0.get(id) != after.0.get(id))
            .copied()
            .collect();
        persist_changed_media(
            tx,
            changed,
            previous_dependencies,
            ScanPlayoutRefreshReason::ProviderCollectionChanged,
        )
        .await
    }
}

pub(crate) async fn persist_changed_media(
    tx: &mut Transaction<'_, Db>,
    mut changed: BTreeSet<EntityId>,
    previous_dependencies: &crate::repositories::ClassicMediaDependencies,
    reason: ScanPlayoutRefreshReason,
) -> Result<()> {
    if changed.is_empty() {
        return Ok(());
    }
    // Provider collections may contain shows or seasons; scheduling consumes
    // their episodes, including search matches inherited from show tags.
    let descendants: Vec<EntityId> = sqlx::query_scalar(
        r#"SELECT e."Id" FROM "Episode" e JOIN "Season" s ON s."Id" = e."SeasonId"
            WHERE s."Id" IN (SELECT value FROM json_each(?))
               OR s."ShowId" IN (SELECT value FROM json_each(?))"#,
    )
    .bind(serde_json::json!(changed).to_string())
    .bind(serde_json::json!(changed).to_string())
    .fetch_all(&mut **tx)
    .await
    .repository()?;
    changed.extend(descendants);
    let previous = previous_dependencies.affected_playouts(&changed);
    persist_resolution_with_reason(tx, &changed, &previous, reason).await
}
