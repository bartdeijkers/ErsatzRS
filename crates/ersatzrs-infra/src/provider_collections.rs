//! Source-owned provider identities (ADR 0104). Display names remain unique in
//! the shared collection namespace; provider names used by tags stay unchanged.
use ersatzrs_core::repositories::{RepositoryError, Result};
use sqlx::{Row, Transaction};

use crate::{
    collection_tags::Provider,
    db::{assert_sql_safe, Db},
    repositories::SqlxResultExt as _,
};

pub(crate) async fn upsert_tx(
    tx: &mut Transaction<'_, Db>,
    provider: Provider,
    source_id: i64,
    external_id: &str,
    etag: Option<&str>,
    name: &str,
) -> Result<i64> {
    let provider = match provider {
        Provider::Plex => "Plex",
        Provider::Jellyfin => "Jellyfin",
        Provider::Emby => "Emby",
    };
    let key = if provider == "Plex" { "Key" } else { "ItemId" };
    if external_id.trim().is_empty() {
        return Err(RepositoryError::Decode {
            kind: "provider collection",
            value: "external identity is empty".to_owned(),
        });
    }
    sqlx::query(r#"INSERT OR IGNORE INTO "MediaSource" ("Id") VALUES (?)"#)
        .bind(source_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    // Identifiers come only from the closed provider enum above.
    sqlx::query(assert_sql_safe(format!(
        r#"INSERT OR IGNORE INTO "{provider}MediaSource" ("Id") VALUES (?)"#
    )))
    .bind(source_id)
    .execute(&mut **tx)
    .await
    .repository()?;
    let existing = sqlx::query(assert_sql_safe(format!(
        r#"SELECT "Id", "Name" FROM "{provider}Collection" WHERE "MediaSourceId" = ? AND "{key}" = ?"#
    )))
    .bind(source_id)
    .bind(external_id)
    .fetch_optional(&mut **tx)
    .await
    .repository()?;
    let (id, renamed) = if let Some(row) = existing {
        let id: i64 = row.try_get("Id").repository()?;
        let previous_name: Option<String> = row.try_get("Name").repository()?;
        sqlx::query(assert_sql_safe(format!(
            r#"UPDATE "{provider}Collection" SET "Name" = ?, "Etag" = ? WHERE "Id" = ?"#
        )))
        .bind(name)
        .bind(etag)
        .bind(id)
        .execute(&mut **tx)
        .await
        .repository()?;
        (id, previous_name.as_deref() != Some(name))
    } else {
        // NULL is temporary inside this transaction, allowing identity allocation
        // before resolving a collision with a manually named collection.
        let result = sqlx::query(
            r#"INSERT INTO "Collection" ("Name", "UseCustomPlaybackOrder") VALUES (NULL, 0)"#,
        )
        .execute(&mut **tx)
        .await
        .repository()?;
        let id = crate::db::last_insert_id(result);
        sqlx::query(assert_sql_safe(format!(
            r#"INSERT INTO "{provider}Collection" ("Id", "MediaSourceId", "{key}", "Etag", "Name") VALUES (?, ?, ?, ?, ?)"#
        )))
        .bind(id)
        .bind(source_id)
        .bind(external_id)
        .bind(etag)
        .bind(name)
        .execute(&mut **tx)
        .await
        .repository()?;
        (id, true)
    };
    if renamed {
        let display_name = display_name(tx, id, provider, source_id, name).await?;
        sqlx::query(r#"UPDATE "Collection" SET "Name" = ? WHERE "Id" = ?"#)
            .bind(display_name)
            .bind(id)
            .execute(&mut **tx)
            .await
            .repository()?;
    }
    Ok(id)
}

async fn display_name(
    tx: &mut Transaction<'_, Db>,
    id: i64,
    provider: &str,
    source_id: i64,
    name: &str,
) -> Result<String> {
    let mut candidate = name.to_owned();
    let mut suffix = 0;
    loop {
        let taken: bool = sqlx::query_scalar(
            r#"SELECT EXISTS(SELECT 1 FROM "Collection" WHERE "Name" = ? AND "Id" <> ?)"#,
        )
        .bind(&candidate)
        .bind(id)
        .fetch_one(&mut **tx)
        .await
        .repository()?;
        if !taken {
            return Ok(candidate);
        }
        candidate = match suffix {
            0 => format!("{name} ({provider} {source_id})"),
            1 => format!("{name} ({provider} {source_id}, {id})"),
            _ => format!("{name} ({provider} {source_id}, {id}, {suffix})"),
        };
        suffix += 1;
    }
}

pub(crate) fn invalid_discovery() -> RepositoryError {
    RepositoryError::Decode {
        kind: "provider collection discovery",
        value: "incomplete, inconsistent or invalid collection response".to_owned(),
    }
}

/// Collection HTTP work is bounded and finishes before the publication transaction.
pub(crate) async fn request<T>(future: impl std::future::Future<Output = Result<T>>) -> Result<T> {
    tokio::time::timeout(std::time::Duration::from_secs(30), future)
        .await
        .map_err(|_| RepositoryError::unavailable("provider collections", "request timed out"))?
}

pub(crate) struct CompletePages {
    start: i32,
    size: i32,
    total: Option<i32>,
}

impl CompletePages {
    pub(crate) fn new(size: i32) -> Self {
        Self {
            start: 0,
            size,
            total: None,
        }
    }

    pub(crate) fn start(&self) -> i32 {
        self.start
    }

    /// Validate raw cardinality before projection; return whether another page is due.
    pub(crate) fn advance(&mut self, total: i32, received: usize) -> Result<bool> {
        if total < self.start || self.total.is_some_and(|previous| previous != total) {
            return Err(invalid_discovery());
        }
        let expected = (total - self.start).min(self.size);
        if usize::try_from(expected).ok() != Some(received) {
            return Err(invalid_discovery());
        }
        self.total = Some(total);
        self.start += expected;
        Ok(self.start < total)
    }
}

pub(crate) async fn reconcile_root(
    tx: &mut Transaction<'_, Db>,
    provider: Provider,
    source: i64,
    root: &str,
    discovered: &[i64],
) -> Result<()> {
    if root.trim().is_empty() {
        return Err(invalid_discovery());
    }
    let name = match provider {
        Provider::Plex => "Plex",
        Provider::Jellyfin => "Jellyfin",
        Provider::Emby => "Emby",
    };
    let previous: Vec<i64> = sqlx::query_scalar(
        r#"SELECT "CollectionId" FROM "ProviderCollectionDiscovery"
        WHERE "Provider" = ? AND "MediaSourceId" = ? AND "RootId" = ?"#,
    )
    .bind(name)
    .bind(source)
    .bind(root)
    .fetch_all(&mut **tx)
    .await
    .repository()?;
    sqlx::query(r#"DELETE FROM "ProviderCollectionDiscovery" WHERE "Provider" = ? AND "MediaSourceId" = ? AND "RootId" = ?"#)
        .bind(name).bind(source).bind(root).execute(&mut **tx).await.repository()?;
    for id in discovered {
        // The provider is a closed enum; identity must still belong to this source.
        let inserted = sqlx::query(assert_sql_safe(format!(
            r#"INSERT INTO "ProviderCollectionDiscovery" ("Provider", "MediaSourceId", "RootId", "CollectionId")
            SELECT ?, ?, ?, "Id" FROM "{name}Collection" WHERE "Id" = ? AND "MediaSourceId" = ?"#
        )))
        .bind(name).bind(source).bind(root).bind(id).bind(source)
        .execute(&mut **tx).await.repository()?;
        if inserted.rows_affected() != 1 {
            return Err(invalid_discovery());
        }
    }
    for id in previous {
        let retained: bool = sqlx::query_scalar(
            r#"SELECT EXISTS(SELECT 1 FROM "ProviderCollectionDiscovery" WHERE "CollectionId" = ?)"#,
        ).bind(id).fetch_one(&mut **tx).await.repository()?;
        if retained {
            continue;
        }
        let owned: bool = sqlx::query_scalar(assert_sql_safe(format!(
            r#"SELECT EXISTS(SELECT 1 FROM "{name}Collection" WHERE "Id" = ? AND "MediaSourceId" = ?)"#
        ))).bind(id).bind(source).fetch_one(&mut **tx).await.repository()?;
        if !owned {
            return Err(invalid_discovery());
        }
        let members = crate::collection_tags::members(tx, id).await?;
        sqlx::query(r#"DELETE FROM "CollectionItem" WHERE "CollectionId" = ?"#)
            .bind(id)
            .execute(&mut **tx)
            .await
            .repository()?;
        crate::collection_tags::reconcile(tx, provider, id, &members).await?;
    }
    Ok(())
}

/// Preserve retained row IDs and user playback overrides while applying provider order.
pub(crate) async fn replace_members(
    tx: &mut Transaction<'_, Db>,
    collection: i64,
    incoming: &[i64],
) -> Result<()> {
    let rows: Vec<(i64, i64)> = sqlx::query_as(
        r#"SELECT "Id", "MediaItemId" FROM "CollectionItem" WHERE "CollectionId" = ? ORDER BY "CustomIndex", "Id""#,
    ).bind(collection).fetch_all(&mut **tx).await.repository()?;
    let mut retained = std::collections::BTreeMap::<i64, std::collections::VecDeque<i64>>::new();
    for (id, media) in rows {
        retained.entry(media).or_default().push_back(id);
    }
    for (position, media) in incoming.iter().enumerate() {
        let position = i32::try_from(position).map_err(|_| invalid_discovery())?;
        if let Some(id) = retained
            .get_mut(media)
            .and_then(std::collections::VecDeque::pop_front)
        {
            sqlx::query(r#"UPDATE "CollectionItem" SET "CustomIndex" = ? WHERE "Id" = ? AND "CustomIndex" IS NOT ?"#)
                .bind(position).bind(id).bind(position).execute(&mut **tx).await.repository()?;
        } else {
            sqlx::query(r#"INSERT INTO "CollectionItem" ("CollectionId", "MediaItemId", "CustomIndex") VALUES (?, ?, ?)"#)
                .bind(collection).bind(media).bind(position).execute(&mut **tx).await.repository()?;
        }
    }
    for id in retained.into_values().flatten() {
        sqlx::query(r#"DELETE FROM "CollectionItem" WHERE "Id" = ?"#)
            .bind(id)
            .execute(&mut **tx)
            .await
            .repository()?;
    }
    Ok(())
}

/// Explicit admin deletion removes only confidently source-owned projections.
/// Unclaimed legacy identities remain conservative: no tag owner is guessed.
pub(crate) async fn delete_admin(pool: &crate::db::DbPool, id: i64) -> Result<()> {
    use crate::provider_collection_refresh::PublicationState;
    let mut owners = Vec::new();
    for (provider, table) in [
        (Provider::Plex, "PlexCollection"),
        (Provider::Jellyfin, "JellyfinCollection"),
        (Provider::Emby, "EmbyCollection"),
    ] {
        let source: Option<Option<i64>> = sqlx::query_scalar(assert_sql_safe(format!(
            r#"SELECT "MediaSourceId" FROM "{table}" WHERE "Id" = ?"#
        )))
        .bind(id)
        .fetch_optional(pool)
        .await
        .repository()?;
        owners.push((provider, table, source.flatten()));
    }
    let dependencies = if owners.iter().all(|(_, _, source)| source.is_none()) {
        None
    } else {
        Some(
            crate::repositories::SqliteSchedulingContentRepository::new(pool.clone())
                .classic_media_dependencies()
                .await?,
        )
    };
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    let mut states = Vec::new();
    for (provider, table, source) in &owners {
        let current: Option<Option<i64>> = sqlx::query_scalar(assert_sql_safe(format!(
            r#"SELECT "MediaSourceId" FROM "{table}" WHERE "Id" = ?"#
        )))
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .repository()?;
        if current.flatten() != *source {
            return Err(invalid_discovery());
        }
        if let Some(source) = source {
            states.push((
                *provider,
                *source,
                PublicationState::capture(&mut tx, *provider, *source).await?,
            ));
        }
    }
    if !states.is_empty() {
        let previous = crate::collection_tags::members(&mut tx, id).await?;
        sqlx::query(r#"DELETE FROM "CollectionItem" WHERE "CollectionId" = ?"#)
            .bind(id)
            .execute(&mut *tx)
            .await
            .repository()?;
        for (provider, _, _) in &states {
            crate::collection_tags::reconcile(&mut tx, *provider, id, &previous).await?;
        }
    }
    sqlx::query(r#"DELETE FROM "Collection" WHERE "Id" = ?"#)
        .bind(id)
        .execute(&mut *tx)
        .await
        .repository()?;
    if let Some(dependencies) = dependencies {
        for (provider, source, before) in states {
            let after = PublicationState::capture(&mut tx, provider, source).await?;
            before
                .persist_changes(&mut tx, after, &dependencies)
                .await?;
        }
    }
    tx.commit().await.repository()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::DbPool;

    async fn upsert(
        pool: &DbPool,
        provider: Provider,
        source: i64,
        key: &str,
        etag: Option<&str>,
        name: &str,
    ) -> Result<i64> {
        let mut tx = crate::db::begin_write(pool).await.repository()?;
        let id = upsert_tx(&mut tx, provider, source, key, etag, name).await?;
        tx.commit().await.repository()?;
        Ok(id)
    }
    use sqlx::{sqlite::SqlitePoolOptions, Executor};

    async fn database() -> DbPool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
        pool
    }

    async fn display(pool: &DbPool, id: i64) -> String {
        sqlx::query_scalar(r#"SELECT "Name" FROM "Collection" WHERE "Id" = ?"#)
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn source_owned_upsert_isolates_collisions_and_preserves_legacy_identity() {
        for (provider, table, key) in [
            (Provider::Plex, "PlexCollection", "Key"),
            (Provider::Jellyfin, "JellyfinCollection", "ItemId"),
            (Provider::Emby, "EmbyCollection", "ItemId"),
        ] {
            let pool = database().await;
            pool.execute(r#"INSERT INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder") VALUES (1, 'Favorites', 1)"#).await.unwrap();
            sqlx::query(assert_sql_safe(format!(r#"INSERT INTO "{table}" ("Id", "{key}", "Name") VALUES (1, 'same-key', 'Favorites')"#)))
                .execute(&pool).await.unwrap();
            let first = upsert(&pool, provider, 11, "same-key", None, "Favorites")
                .await
                .unwrap();
            let second = upsert(&pool, provider, 22, "same-key", Some("v1"), "Favorites")
                .await
                .unwrap();
            assert_ne!(first, 1);
            assert_ne!(second, first);
            let first_name = display(&pool, first).await;
            let second_name = display(&pool, second).await;
            assert_ne!(first_name, second_name);
            assert!(first_name.starts_with("Favorites ("));
            assert_eq!(display(&pool, 1).await, "Favorites");
            assert_eq!(
                upsert(&pool, provider, 11, "same-key", Some("v2"), "Favorites")
                    .await
                    .unwrap(),
                first
            );
            assert_eq!(display(&pool, first).await, first_name);
            assert_eq!(
                upsert(&pool, provider, 22, "same-key", Some("v3"), "Renamed")
                    .await
                    .unwrap(),
                second
            );
            assert_eq!(display(&pool, first).await, first_name);
            assert_eq!(display(&pool, second).await, "Renamed");
            let rows: Vec<(i64, Option<i64>, String)> = sqlx::query_as(assert_sql_safe(format!(
                r#"SELECT "Id", "MediaSourceId", "Name" FROM "{table}" ORDER BY "Id""#
            )))
            .fetch_all(&pool)
            .await
            .unwrap();
            assert_eq!(
                rows,
                vec![
                    (1, None, "Favorites".into()),
                    (first, Some(11), "Favorites".into()),
                    (second, Some(22), "Renamed".into())
                ]
            );
        }
    }

    #[tokio::test]
    async fn source_owned_upsert_rolls_back_identity_and_name_when_write_fails() {
        let pool = database().await;
        pool.execute(r#"CREATE TRIGGER "FailCollectionName" BEFORE UPDATE ON "Collection" BEGIN SELECT RAISE(ABORT, 'synthetic write failure'); END"#).await.unwrap();
        assert!(upsert(&pool, Provider::Plex, 1, "key", None, "Favorites")
            .await
            .is_err());
        let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Collection""#)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
        let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "PlexCollection""#)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
        pool.execute(r#"DROP TRIGGER "FailCollectionName""#)
            .await
            .unwrap();
        assert!(upsert(&pool, Provider::Plex, 1, "  ", None, "Invalid")
            .await
            .is_err());
        let id = upsert(&pool, Provider::Plex, 1, "key", None, "Favorites")
            .await
            .unwrap();
        assert_eq!(display(&pool, id).await, "Favorites");
    }
}
