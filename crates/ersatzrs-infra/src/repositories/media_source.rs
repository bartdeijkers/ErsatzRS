use async_trait::async_trait;
use ersatzrs_core::repositories::{MediaSourceRepository, Result};
use ersatzrs_domain::{
    EmbyConnection, EmbyMediaSource, EmbyPathReplacement, EntityId, JellyfinConnection,
    JellyfinMediaSource, JellyfinPathReplacement, MediaSource, MediaSourceKind, PlexConnection,
    PlexMediaSource, PlexPathReplacement,
};
use sqlx::{Pool, Row, Sqlite};

use super::common::{decode_optional_datetime, encode_optional_datetime, not_found};

#[derive(Clone)]
pub struct SqliteMediaSourceRepository(pub Pool<Sqlite>);

#[async_trait]
impl MediaSourceRepository for SqliteMediaSourceRepository {
    async fn get_plex(&self, id: EntityId) -> Result<PlexMediaSource> {
        let base = fetch_base_source(&self.0, id, MediaSourceKind::Plex).await?;
        let row = sqlx::query(
            r#"SELECT "ServerName", "ClientIdentifier" FROM "PlexMediaSource" WHERE "Id" = ?"#,
        )
        .bind(id)
        .fetch_one(&self.0)
        .await
        .map_err(|error| not_found("plex media source", id, error))?;

        let connection_rows = sqlx::query(
            r#"SELECT "Id", "IsActive", "Uri", "PlexMediaSourceId"
             FROM "PlexConnection"
             WHERE "PlexMediaSourceId" = ?
             ORDER BY "Id""#,
        )
        .bind(id)
        .fetch_all(&self.0)
        .await?;

        let replacement_rows = sqlx::query(
            r#"SELECT "Id", "PlexMediaSourceId", "PlexPath", "LocalPath"
             FROM "PlexPathReplacement"
             WHERE "PlexMediaSourceId" = ?
             ORDER BY "Id""#,
        )
        .bind(id)
        .fetch_all(&self.0)
        .await?;

        Ok(PlexMediaSource {
            media_source: base,
            server_name: row.try_get("ServerName")?,
            machine_identifier: row.try_get("ClientIdentifier")?,
            connections: connection_rows
                .iter()
                .map(|connection| PlexConnection {
                    id: connection.try_get("Id").unwrap_or_default(),
                    is_active: connection.try_get("IsActive").unwrap_or(false),
                    uri: connection.try_get("Uri").unwrap_or_default(),
                    plex_media_source_id: connection
                        .try_get("PlexMediaSourceId")
                        .unwrap_or_default(),
                })
                .collect(),
            path_replacements: replacement_rows
                .iter()
                .map(|replacement| PlexPathReplacement {
                    id: replacement.try_get("Id").unwrap_or_default(),
                    plex_media_source_id: replacement
                        .try_get("PlexMediaSourceId")
                        .unwrap_or_default(),
                    path: replacement.try_get("PlexPath").unwrap_or_default(),
                    replacement_path: replacement.try_get("LocalPath").unwrap_or_default(),
                })
                .collect(),
        })
    }

    async fn get_jellyfin(&self, id: EntityId) -> Result<JellyfinMediaSource> {
        let base = fetch_base_source(&self.0, id, MediaSourceKind::Jellyfin).await?;
        let row = sqlx::query(
            r#"SELECT "ServerName", "OperatingSystem", "LastCollectionsScan"
             FROM "JellyfinMediaSource"
             WHERE "Id" = ?"#,
        )
        .bind(id)
        .fetch_one(&self.0)
        .await
        .map_err(|error| not_found("jellyfin media source", id, error))?;

        let connection_rows = sqlx::query(
            r#"SELECT "Id", "Address", "JellyfinMediaSourceId"
             FROM "JellyfinConnection"
             WHERE "JellyfinMediaSourceId" = ?
             ORDER BY "Id""#,
        )
        .bind(id)
        .fetch_all(&self.0)
        .await?;

        let replacement_rows = sqlx::query(
            r#"SELECT "Id", "JellyfinMediaSourceId", "JellyfinPath", "LocalPath"
             FROM "JellyfinPathReplacement"
             WHERE "JellyfinMediaSourceId" = ?
             ORDER BY "Id""#,
        )
        .bind(id)
        .fetch_all(&self.0)
        .await?;

        Ok(JellyfinMediaSource {
            media_source: base,
            server_name: row.try_get("ServerName")?,
            operating_system: row.try_get("OperatingSystem")?,
            connections: connection_rows
                .iter()
                .map(|connection| JellyfinConnection {
                    id: connection.try_get("Id").unwrap_or_default(),
                    address: connection.try_get("Address").unwrap_or_default(),
                    jellyfin_media_source_id: connection
                        .try_get("JellyfinMediaSourceId")
                        .unwrap_or_default(),
                })
                .collect(),
            path_replacements: replacement_rows
                .iter()
                .map(|replacement| JellyfinPathReplacement {
                    id: replacement.try_get("Id").unwrap_or_default(),
                    jellyfin_media_source_id: replacement
                        .try_get("JellyfinMediaSourceId")
                        .unwrap_or_default(),
                    path: replacement.try_get("JellyfinPath").unwrap_or_default(),
                    replacement_path: replacement.try_get("LocalPath").unwrap_or_default(),
                })
                .collect(),
            last_collections_scan: decode_optional_datetime(
                "jellyfin media source last_collections_scan",
                row.try_get("LastCollectionsScan")?,
            )?,
        })
    }

    async fn get_emby(&self, id: EntityId) -> Result<EmbyMediaSource> {
        let base = fetch_base_source(&self.0, id, MediaSourceKind::Emby).await?;
        let row = sqlx::query(
            r#"SELECT "ServerName", "OperatingSystem", "LastCollectionsScan"
             FROM "EmbyMediaSource"
             WHERE "Id" = ?"#,
        )
        .bind(id)
        .fetch_one(&self.0)
        .await
        .map_err(|error| not_found("emby media source", id, error))?;

        let connection_rows = sqlx::query(
            r#"SELECT "Id", "Address", "EmbyMediaSourceId"
             FROM "EmbyConnection"
             WHERE "EmbyMediaSourceId" = ?
             ORDER BY "Id""#,
        )
        .bind(id)
        .fetch_all(&self.0)
        .await?;

        let replacement_rows = sqlx::query(
            r#"SELECT "Id", "EmbyMediaSourceId", "EmbyPath", "LocalPath"
             FROM "EmbyPathReplacement"
             WHERE "EmbyMediaSourceId" = ?
             ORDER BY "Id""#,
        )
        .bind(id)
        .fetch_all(&self.0)
        .await?;

        Ok(EmbyMediaSource {
            media_source: base,
            server_name: row.try_get("ServerName")?,
            operating_system: row.try_get("OperatingSystem")?,
            connections: connection_rows
                .iter()
                .map(|connection| EmbyConnection {
                    id: connection.try_get("Id").unwrap_or_default(),
                    address: connection.try_get("Address").unwrap_or_default(),
                    emby_media_source_id: connection
                        .try_get("EmbyMediaSourceId")
                        .unwrap_or_default(),
                })
                .collect(),
            path_replacements: replacement_rows
                .iter()
                .map(|replacement| EmbyPathReplacement {
                    id: replacement.try_get("Id").unwrap_or_default(),
                    emby_media_source_id: replacement
                        .try_get("EmbyMediaSourceId")
                        .unwrap_or_default(),
                    path: replacement.try_get("EmbyPath").unwrap_or_default(),
                    replacement_path: replacement.try_get("LocalPath").unwrap_or_default(),
                })
                .collect(),
            last_collections_scan: decode_optional_datetime(
                "emby media source last_collections_scan",
                row.try_get("LastCollectionsScan")?,
            )?,
        })
    }

    async fn list_all(&self) -> Result<Vec<MediaSource>> {
        // MediaSource table only has "Id". Build a synthetic list from all sub-type tables.
        let plex_ids = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "Id" FROM "PlexMediaSource" ORDER BY "Id""#,
        )
        .fetch_all(&self.0)
        .await?;
        let jellyfin_ids = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "Id" FROM "JellyfinMediaSource" ORDER BY "Id""#,
        )
        .fetch_all(&self.0)
        .await?;
        let emby_ids = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "Id" FROM "EmbyMediaSource" ORDER BY "Id""#,
        )
        .fetch_all(&self.0)
        .await?;
        let local_ids = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "Id" FROM "LocalMediaSource" ORDER BY "Id""#,
        )
        .fetch_all(&self.0)
        .await?;

        let mut sources = Vec::new();
        for id in plex_ids {
            sources.push(MediaSource {
                id,
                name: String::new(),
                kind: MediaSourceKind::Plex,
                last_scan: None,
            });
        }
        for id in jellyfin_ids {
            sources.push(MediaSource {
                id,
                name: String::new(),
                kind: MediaSourceKind::Jellyfin,
                last_scan: None,
            });
        }
        for id in emby_ids {
            sources.push(MediaSource {
                id,
                name: String::new(),
                kind: MediaSourceKind::Emby,
                last_scan: None,
            });
        }
        for id in local_ids {
            sources.push(MediaSource {
                id,
                name: String::new(),
                kind: MediaSourceKind::Local,
                last_scan: None,
            });
        }
        sources.sort_by_key(|s| s.id);
        Ok(sources)
    }

    async fn upsert_plex(&self, source: &PlexMediaSource) -> Result<()> {
        upsert_source(&self.0, &source.media_source, source).await
    }

    async fn upsert_jellyfin(&self, source: &JellyfinMediaSource) -> Result<()> {
        upsert_source(&self.0, &source.media_source, source).await
    }

    async fn upsert_emby(&self, source: &EmbyMediaSource) -> Result<()> {
        upsert_source(&self.0, &source.media_source, source).await
    }
}

async fn upsert_source<T>(pool: &Pool<Sqlite>, base: &MediaSource, provider: &T) -> Result<()>
where
    T: ProviderUpsert + Sync,
{
    let mut tx = pool.begin().await?;
    // "MediaSource" only has "Id"
    sqlx::query(r#"INSERT OR IGNORE INTO "MediaSource" ("Id") VALUES (?)"#)
        .bind(base.id)
        .execute(&mut *tx)
        .await?;

    provider.upsert(&mut tx).await?;

    tx.commit().await?;
    Ok(())
}

trait ProviderUpsert {
    fn upsert<'a>(
        &'a self,
        tx: &'a mut sqlx::Transaction<'_, Sqlite>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>>;
}

impl ProviderUpsert for PlexMediaSource {
    fn upsert<'a>(
        &'a self,
        tx: &'a mut sqlx::Transaction<'_, Sqlite>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            sqlx::query(
                r#"INSERT OR REPLACE INTO "PlexMediaSource" ("Id", "ServerName", "ClientIdentifier")
                 VALUES (?, ?, ?)"#,
            )
            .bind(self.media_source.id)
            .bind(&self.server_name)
            .bind(&self.machine_identifier)
            .execute(&mut **tx)
            .await?;

            sqlx::query(r#"DELETE FROM "PlexConnection" WHERE "PlexMediaSourceId" = ?"#)
                .bind(self.media_source.id)
                .execute(&mut **tx)
                .await?;
            for connection in &self.connections {
                sqlx::query(
                    r#"INSERT OR REPLACE INTO "PlexConnection" ("Id", "PlexMediaSourceId", "IsActive", "Uri")
                     VALUES (?, ?, ?, ?)"#,
                )
                .bind(connection.id)
                .bind(self.media_source.id)
                .bind(connection.is_active)
                .bind(&connection.uri)
                .execute(&mut **tx)
                .await?;
            }

            sqlx::query(r#"DELETE FROM "PlexPathReplacement" WHERE "PlexMediaSourceId" = ?"#)
                .bind(self.media_source.id)
                .execute(&mut **tx)
                .await?;
            for replacement in &self.path_replacements {
                sqlx::query(
                    r#"INSERT OR REPLACE INTO "PlexPathReplacement" (
                        "Id", "PlexMediaSourceId", "PlexPath", "LocalPath"
                     ) VALUES (?, ?, ?, ?)"#,
                )
                .bind(replacement.id)
                .bind(self.media_source.id)
                .bind(&replacement.path)
                .bind(&replacement.replacement_path)
                .execute(&mut **tx)
                .await?;
            }

            Ok(())
        })
    }
}

impl ProviderUpsert for JellyfinMediaSource {
    fn upsert<'a>(
        &'a self,
        tx: &'a mut sqlx::Transaction<'_, Sqlite>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            sqlx::query(
                r#"INSERT OR REPLACE INTO "JellyfinMediaSource" (
                    "Id", "ServerName", "OperatingSystem", "LastCollectionsScan"
                 ) VALUES (?, ?, ?, ?)"#,
            )
            .bind(self.media_source.id)
            .bind(&self.server_name)
            .bind(&self.operating_system)
            .bind(encode_optional_datetime(self.last_collections_scan))
            .execute(&mut **tx)
            .await?;

            sqlx::query(r#"DELETE FROM "JellyfinConnection" WHERE "JellyfinMediaSourceId" = ?"#)
                .bind(self.media_source.id)
                .execute(&mut **tx)
                .await?;
            for connection in &self.connections {
                sqlx::query(
                    r#"INSERT OR REPLACE INTO "JellyfinConnection" ("Id", "JellyfinMediaSourceId", "Address")
                     VALUES (?, ?, ?)"#,
                )
                .bind(connection.id)
                .bind(self.media_source.id)
                .bind(&connection.address)
                .execute(&mut **tx)
                .await?;
            }

            sqlx::query(
                r#"DELETE FROM "JellyfinPathReplacement" WHERE "JellyfinMediaSourceId" = ?"#,
            )
            .bind(self.media_source.id)
            .execute(&mut **tx)
            .await?;
            for replacement in &self.path_replacements {
                sqlx::query(
                    r#"INSERT OR REPLACE INTO "JellyfinPathReplacement" (
                        "Id", "JellyfinMediaSourceId", "JellyfinPath", "LocalPath"
                     ) VALUES (?, ?, ?, ?)"#,
                )
                .bind(replacement.id)
                .bind(self.media_source.id)
                .bind(&replacement.path)
                .bind(&replacement.replacement_path)
                .execute(&mut **tx)
                .await?;
            }

            Ok(())
        })
    }
}

impl ProviderUpsert for EmbyMediaSource {
    fn upsert<'a>(
        &'a self,
        tx: &'a mut sqlx::Transaction<'_, Sqlite>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            sqlx::query(
                r#"INSERT OR REPLACE INTO "EmbyMediaSource" (
                    "Id", "ServerName", "OperatingSystem", "LastCollectionsScan"
                 ) VALUES (?, ?, ?, ?)"#,
            )
            .bind(self.media_source.id)
            .bind(&self.server_name)
            .bind(&self.operating_system)
            .bind(encode_optional_datetime(self.last_collections_scan))
            .execute(&mut **tx)
            .await?;

            sqlx::query(r#"DELETE FROM "EmbyConnection" WHERE "EmbyMediaSourceId" = ?"#)
                .bind(self.media_source.id)
                .execute(&mut **tx)
                .await?;
            for connection in &self.connections {
                sqlx::query(
                    r#"INSERT OR REPLACE INTO "EmbyConnection" ("Id", "EmbyMediaSourceId", "Address")
                     VALUES (?, ?, ?)"#,
                )
                .bind(connection.id)
                .bind(self.media_source.id)
                .bind(&connection.address)
                .execute(&mut **tx)
                .await?;
            }

            sqlx::query(r#"DELETE FROM "EmbyPathReplacement" WHERE "EmbyMediaSourceId" = ?"#)
                .bind(self.media_source.id)
                .execute(&mut **tx)
                .await?;
            for replacement in &self.path_replacements {
                sqlx::query(
                    r#"INSERT OR REPLACE INTO "EmbyPathReplacement" (
                        "Id", "EmbyMediaSourceId", "EmbyPath", "LocalPath"
                     ) VALUES (?, ?, ?, ?)"#,
                )
                .bind(replacement.id)
                .bind(self.media_source.id)
                .bind(&replacement.path)
                .bind(&replacement.replacement_path)
                .execute(&mut **tx)
                .await?;
            }

            Ok(())
        })
    }
}

async fn fetch_base_source(
    pool: &Pool<Sqlite>,
    id: EntityId,
    expected_kind: MediaSourceKind,
) -> Result<MediaSource> {
    // Verify the MediaSource row exists
    let exists: bool = sqlx::query_scalar::<_, bool>(
        r#"SELECT EXISTS(SELECT 1 FROM "MediaSource" WHERE "Id" = ?)"#,
    )
    .bind(id)
    .fetch_one(pool)
    .await?;

    if !exists {
        return Err(super::common::not_found(
            "media source",
            id,
            sqlx::Error::RowNotFound,
        ));
    }

    Ok(MediaSource {
        id,
        name: String::new(),
        kind: expected_kind,
        last_scan: None,
    })
}
