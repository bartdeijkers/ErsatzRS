use async_trait::async_trait;
use nastv_core::repositories::{MediaSourceRepository, Result};
use nastv_domain::{
    EmbyConnection, EmbyMediaSource, EmbyPathReplacement, EntityId, JellyfinConnection,
    JellyfinMediaSource, JellyfinPathReplacement, MediaSource, MediaSourceKind, PlexConnection,
    PlexMediaSource, PlexPathReplacement,
};
use sqlx::sqlite::SqliteRow;
use sqlx::{Pool, Row, Sqlite};

use super::common::{
    decode_enum, decode_optional_datetime, encode_optional_datetime, not_found,
};

#[derive(Clone)]
pub struct SqliteMediaSourceRepository(pub Pool<Sqlite>);

#[async_trait]
impl MediaSourceRepository for SqliteMediaSourceRepository {
    async fn get_plex(&self, id: EntityId) -> Result<PlexMediaSource> {
        let base = fetch_base_source(&self.0, id, MediaSourceKind::Plex).await?;
        let row = sqlx::query("SELECT server_name, machine_identifier FROM plex_media_sources WHERE id = ?")
            .bind(id)
            .fetch_one(&self.0)
            .await
            .map_err(|error| not_found("plex media source", id, error))?;

        let connection_rows = sqlx::query(
            "SELECT id, is_active, uri, plex_media_source_id
             FROM plex_connections
             WHERE plex_media_source_id = ?
             ORDER BY id",
        )
        .bind(id)
        .fetch_all(&self.0)
        .await?;

        let replacement_rows = sqlx::query(
            "SELECT id, plex_media_source_id, path, replacement_path
             FROM plex_path_replacements
             WHERE plex_media_source_id = ?
             ORDER BY id",
        )
        .bind(id)
        .fetch_all(&self.0)
        .await?;

        Ok(PlexMediaSource {
            media_source: base,
            server_name: row.try_get("server_name")?,
            machine_identifier: row.try_get("machine_identifier")?,
            connections: connection_rows
                .iter()
                .map(|connection| PlexConnection {
                    id: connection.try_get("id").unwrap_or_default(),
                    is_active: connection.try_get("is_active").unwrap_or(false),
                    uri: connection.try_get("uri").unwrap_or_default(),
                    plex_media_source_id: connection
                        .try_get("plex_media_source_id")
                        .unwrap_or_default(),
                })
                .collect(),
            path_replacements: replacement_rows
                .iter()
                .map(|replacement| PlexPathReplacement {
                    id: replacement.try_get("id").unwrap_or_default(),
                    plex_media_source_id: replacement
                        .try_get("plex_media_source_id")
                        .unwrap_or_default(),
                    path: replacement.try_get("path").unwrap_or_default(),
                    replacement_path: replacement
                        .try_get("replacement_path")
                        .unwrap_or_default(),
                })
                .collect(),
        })
    }

    async fn get_jellyfin(&self, id: EntityId) -> Result<JellyfinMediaSource> {
        let base = fetch_base_source(&self.0, id, MediaSourceKind::Jellyfin).await?;
        let row = sqlx::query(
            "SELECT server_name, operating_system, last_collections_scan
             FROM jellyfin_media_sources
             WHERE id = ?",
        )
        .bind(id)
        .fetch_one(&self.0)
        .await
        .map_err(|error| not_found("jellyfin media source", id, error))?;

        let connection_rows = sqlx::query(
            "SELECT id, address, jellyfin_media_source_id
             FROM jellyfin_connections
             WHERE jellyfin_media_source_id = ?
             ORDER BY id",
        )
        .bind(id)
        .fetch_all(&self.0)
        .await?;

        let replacement_rows = sqlx::query(
            "SELECT id, jellyfin_media_source_id, path, replacement_path
             FROM jellyfin_path_replacements
             WHERE jellyfin_media_source_id = ?
             ORDER BY id",
        )
        .bind(id)
        .fetch_all(&self.0)
        .await?;

        Ok(JellyfinMediaSource {
            media_source: base,
            server_name: row.try_get("server_name")?,
            operating_system: row.try_get("operating_system")?,
            connections: connection_rows
                .iter()
                .map(|connection| JellyfinConnection {
                    id: connection.try_get("id").unwrap_or_default(),
                    address: connection.try_get("address").unwrap_or_default(),
                    jellyfin_media_source_id: connection
                        .try_get("jellyfin_media_source_id")
                        .unwrap_or_default(),
                })
                .collect(),
            path_replacements: replacement_rows
                .iter()
                .map(|replacement| JellyfinPathReplacement {
                    id: replacement.try_get("id").unwrap_or_default(),
                    jellyfin_media_source_id: replacement
                        .try_get("jellyfin_media_source_id")
                        .unwrap_or_default(),
                    path: replacement.try_get("path").unwrap_or_default(),
                    replacement_path: replacement
                        .try_get("replacement_path")
                        .unwrap_or_default(),
                })
                .collect(),
            last_collections_scan: decode_optional_datetime(
                "jellyfin media source last_collections_scan",
                row.try_get("last_collections_scan")?,
            )?,
        })
    }

    async fn get_emby(&self, id: EntityId) -> Result<EmbyMediaSource> {
        let base = fetch_base_source(&self.0, id, MediaSourceKind::Emby).await?;
        let row = sqlx::query(
            "SELECT server_name, operating_system, last_collections_scan
             FROM emby_media_sources
             WHERE id = ?",
        )
        .bind(id)
        .fetch_one(&self.0)
        .await
        .map_err(|error| not_found("emby media source", id, error))?;

        let connection_rows = sqlx::query(
            "SELECT id, address, emby_media_source_id
             FROM emby_connections
             WHERE emby_media_source_id = ?
             ORDER BY id",
        )
        .bind(id)
        .fetch_all(&self.0)
        .await?;

        let replacement_rows = sqlx::query(
            "SELECT id, emby_media_source_id, path, replacement_path
             FROM emby_path_replacements
             WHERE emby_media_source_id = ?
             ORDER BY id",
        )
        .bind(id)
        .fetch_all(&self.0)
        .await?;

        Ok(EmbyMediaSource {
            media_source: base,
            server_name: row.try_get("server_name")?,
            operating_system: row.try_get("operating_system")?,
            connections: connection_rows
                .iter()
                .map(|connection| EmbyConnection {
                    id: connection.try_get("id").unwrap_or_default(),
                    address: connection.try_get("address").unwrap_or_default(),
                    emby_media_source_id: connection
                        .try_get("emby_media_source_id")
                        .unwrap_or_default(),
                })
                .collect(),
            path_replacements: replacement_rows
                .iter()
                .map(|replacement| EmbyPathReplacement {
                    id: replacement.try_get("id").unwrap_or_default(),
                    emby_media_source_id: replacement
                        .try_get("emby_media_source_id")
                        .unwrap_or_default(),
                    path: replacement.try_get("path").unwrap_or_default(),
                    replacement_path: replacement
                        .try_get("replacement_path")
                        .unwrap_or_default(),
                })
                .collect(),
            last_collections_scan: decode_optional_datetime(
                "emby media source last_collections_scan",
                row.try_get("last_collections_scan")?,
            )?,
        })
    }

    async fn list_all(&self) -> Result<Vec<MediaSource>> {
        let rows = sqlx::query("SELECT * FROM media_sources ORDER BY name, id")
        .fetch_all(&self.0)
        .await?;
        rows.iter().map(map_media_source).collect()
    }

    async fn upsert_plex(&self, source: &PlexMediaSource) -> Result<()> {
        upsert_source(&self.0, &source.media_source, MediaSourceKind::Plex, source).await
    }

    async fn upsert_jellyfin(&self, source: &JellyfinMediaSource) -> Result<()> {
        upsert_source(&self.0, &source.media_source, MediaSourceKind::Jellyfin, source).await
    }

    async fn upsert_emby(&self, source: &EmbyMediaSource) -> Result<()> {
        upsert_source(&self.0, &source.media_source, MediaSourceKind::Emby, source).await
    }
}

async fn upsert_source<T>(
    pool: &Pool<Sqlite>,
    base: &MediaSource,
    kind: MediaSourceKind,
    provider: &T,
) -> Result<()>
where
    T: ProviderUpsert + Sync,
{
    let mut base = base.clone();
    base.kind = kind;
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT OR REPLACE INTO media_sources (id, name, kind, last_scan)
         VALUES (?, ?, ?, ?)",
    )
    .bind(base.id)
    .bind(&base.name)
    .bind(format!("{kind:?}"))
    .bind(encode_optional_datetime(base.last_scan))
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
                "INSERT OR REPLACE INTO plex_media_sources (id, server_name, machine_identifier)
                 VALUES (?, ?, ?)",
            )
            .bind(self.media_source.id)
            .bind(&self.server_name)
            .bind(&self.machine_identifier)
            .execute(&mut **tx)
            .await?;

            sqlx::query("DELETE FROM plex_connections WHERE plex_media_source_id = ?")
                .bind(self.media_source.id)
                .execute(&mut **tx)
                .await?;
            for connection in &self.connections {
                sqlx::query(
                    "INSERT OR REPLACE INTO plex_connections (id, plex_media_source_id, is_active, uri)
                     VALUES (?, ?, ?, ?)",
                )
                .bind(connection.id)
                .bind(self.media_source.id)
                .bind(connection.is_active)
                .bind(&connection.uri)
                .execute(&mut **tx)
                .await?;
            }

            sqlx::query("DELETE FROM plex_path_replacements WHERE plex_media_source_id = ?")
                .bind(self.media_source.id)
                .execute(&mut **tx)
                .await?;
            for replacement in &self.path_replacements {
                sqlx::query(
                    "INSERT OR REPLACE INTO plex_path_replacements (
                        id, plex_media_source_id, path, replacement_path
                     ) VALUES (?, ?, ?, ?)",
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
                "INSERT OR REPLACE INTO jellyfin_media_sources (
                    id, server_name, operating_system, last_collections_scan
                 ) VALUES (?, ?, ?, ?)",
            )
            .bind(self.media_source.id)
            .bind(&self.server_name)
            .bind(&self.operating_system)
            .bind(encode_optional_datetime(self.last_collections_scan))
            .execute(&mut **tx)
            .await?;

            sqlx::query("DELETE FROM jellyfin_connections WHERE jellyfin_media_source_id = ?")
                .bind(self.media_source.id)
                .execute(&mut **tx)
                .await?;
            for connection in &self.connections {
                sqlx::query(
                    "INSERT OR REPLACE INTO jellyfin_connections (id, jellyfin_media_source_id, address)
                     VALUES (?, ?, ?)",
                )
                .bind(connection.id)
                .bind(self.media_source.id)
                .bind(&connection.address)
                .execute(&mut **tx)
                .await?;
            }

            sqlx::query("DELETE FROM jellyfin_path_replacements WHERE jellyfin_media_source_id = ?")
                .bind(self.media_source.id)
                .execute(&mut **tx)
                .await?;
            for replacement in &self.path_replacements {
                sqlx::query(
                    "INSERT OR REPLACE INTO jellyfin_path_replacements (
                        id, jellyfin_media_source_id, path, replacement_path
                     ) VALUES (?, ?, ?, ?)",
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
                "INSERT OR REPLACE INTO emby_media_sources (
                    id, server_name, operating_system, last_collections_scan
                 ) VALUES (?, ?, ?, ?)",
            )
            .bind(self.media_source.id)
            .bind(&self.server_name)
            .bind(&self.operating_system)
            .bind(encode_optional_datetime(self.last_collections_scan))
            .execute(&mut **tx)
            .await?;

            sqlx::query("DELETE FROM emby_connections WHERE emby_media_source_id = ?")
                .bind(self.media_source.id)
                .execute(&mut **tx)
                .await?;
            for connection in &self.connections {
                sqlx::query(
                    "INSERT OR REPLACE INTO emby_connections (id, emby_media_source_id, address)
                     VALUES (?, ?, ?)",
                )
                .bind(connection.id)
                .bind(self.media_source.id)
                .bind(&connection.address)
                .execute(&mut **tx)
                .await?;
            }

            sqlx::query("DELETE FROM emby_path_replacements WHERE emby_media_source_id = ?")
                .bind(self.media_source.id)
                .execute(&mut **tx)
                .await?;
            for replacement in &self.path_replacements {
                sqlx::query(
                    "INSERT OR REPLACE INTO emby_path_replacements (
                        id, emby_media_source_id, path, replacement_path
                     ) VALUES (?, ?, ?, ?)",
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
    let row = sqlx::query("SELECT * FROM media_sources WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await
        .map_err(|error| not_found("media source", id, error))?;
    let source = map_media_source(&row)?;
    debug_assert_eq!(source.kind, expected_kind);
    Ok(source)
}

fn map_media_source(row: &SqliteRow) -> Result<MediaSource> {
    Ok(MediaSource {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        kind: decode_enum("media source kind", &row.try_get::<String, _>("kind")?)?,
        last_scan: decode_optional_datetime("media source last_scan", row.try_get("last_scan")?)?,
    })
}
