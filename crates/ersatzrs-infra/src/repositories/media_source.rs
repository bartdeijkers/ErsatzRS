use crate::repositories::SqlxResultExt as _;
use async_trait::async_trait;
use ersatzrs_core::repositories::{
    LibraryPreferenceAdmin, LocalLibraryAdmin, LocalLibraryAdminRow, LocalLibraryPathAdmin,
    ManagedAddonMediaSourceAdminRow, MediaSourceRepository, PathReplacementAdmin,
    PlexArtworkConnectionAdmin, RemoteConnectionAdmin, RemoteMediaSourceKind, RemoteSourceAdminRow,
    RepositoryError, Result,
};
use ersatzrs_domain::{
    EmbyConnection, EmbyMediaSource, EmbyPathReplacement, EntityId, JellyfinConnection,
    JellyfinMediaSource, JellyfinPathReplacement, MediaSource, MediaSourceKind, PlexConnection,
    PlexMediaSource, PlexPathReplacement,
};
use sqlx::{Pool, Row};

use super::common::upsert_sql;
use super::common::{
    decode_bool_column, decode_optional_datetime, encode_optional_datetime, not_found,
};

#[derive(Clone)]
pub struct SqliteMediaSourceRepository(pub Pool<crate::db::Db>);

#[async_trait]
#[allow(clippy::too_many_lines)]
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
        .await
        .repository()?;

        let replacement_rows = sqlx::query(
            r#"SELECT "Id", "PlexMediaSourceId", "PlexPath", "LocalPath"
             FROM "PlexPathReplacement"
             WHERE "PlexMediaSourceId" = ?
             ORDER BY "Id""#,
        )
        .bind(id)
        .fetch_all(&self.0)
        .await
        .repository()?;

        Ok(PlexMediaSource {
            media_source: base,
            server_name: row.try_get("ServerName").repository()?,
            machine_identifier: row.try_get("ClientIdentifier").repository()?,
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
        .await
        .repository()?;

        let replacement_rows = sqlx::query(
            r#"SELECT "Id", "JellyfinMediaSourceId", "JellyfinPath", "LocalPath"
             FROM "JellyfinPathReplacement"
             WHERE "JellyfinMediaSourceId" = ?
             ORDER BY "Id""#,
        )
        .bind(id)
        .fetch_all(&self.0)
        .await
        .repository()?;

        Ok(JellyfinMediaSource {
            media_source: base,
            server_name: row.try_get("ServerName").repository()?,
            operating_system: row.try_get("OperatingSystem").repository()?,
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
                row.try_get("LastCollectionsScan").repository()?,
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
        .await
        .repository()?;

        let replacement_rows = sqlx::query(
            r#"SELECT "Id", "EmbyMediaSourceId", "EmbyPath", "LocalPath"
             FROM "EmbyPathReplacement"
             WHERE "EmbyMediaSourceId" = ?
             ORDER BY "Id""#,
        )
        .bind(id)
        .fetch_all(&self.0)
        .await
        .repository()?;

        Ok(EmbyMediaSource {
            media_source: base,
            server_name: row.try_get("ServerName").repository()?,
            operating_system: row.try_get("OperatingSystem").repository()?,
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
                row.try_get("LastCollectionsScan").repository()?,
            )?,
        })
    }

    async fn list_all(&self) -> Result<Vec<MediaSource>> {
        // MediaSource table only has "Id". Build a synthetic list from all sub-type tables.
        let plex_ids = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "Id" FROM "PlexMediaSource" ORDER BY "Id""#,
        )
        .fetch_all(&self.0)
        .await
        .repository()?;
        let jellyfin_ids = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "Id" FROM "JellyfinMediaSource" ORDER BY "Id""#,
        )
        .fetch_all(&self.0)
        .await
        .repository()?;
        let emby_ids = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "Id" FROM "EmbyMediaSource" ORDER BY "Id""#,
        )
        .fetch_all(&self.0)
        .await
        .repository()?;
        let local_ids = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "Id" FROM "LocalMediaSource" ORDER BY "Id""#,
        )
        .fetch_all(&self.0)
        .await
        .repository()?;

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

    async fn plex_artwork_connection_admin(
        &self,
        source_id: EntityId,
    ) -> Result<Option<PlexArtworkConnectionAdmin>> {
        let row = sqlx::query(
            r#"SELECT "pc"."Uri", "pms"."ClientIdentifier"
               FROM "PlexConnection" AS "pc"
               JOIN "PlexMediaSource" AS "pms"
                 ON "pms"."Id" = "pc"."PlexMediaSourceId"
               WHERE "pc"."PlexMediaSourceId" = ? AND "pc"."IsActive" = 1
                 AND "pc"."Uri" IS NOT NULL
               ORDER BY "pc"."Id"
               LIMIT 1"#,
        )
        .bind(source_id)
        .fetch_optional(&self.0)
        .await
        .repository()?;
        let Some(row) = row else {
            return Ok(None);
        };
        let address: String = row.try_get("Uri").repository()?;
        let client_identifier: String = row.try_get("ClientIdentifier").repository()?;
        let token = sqlx::query_scalar::<_, String>(
            r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ? AND "Value" <> ''"#,
        )
        .bind(format!("plex.server_auth_token.{client_identifier}"))
        .fetch_optional(&self.0)
        .await
        .repository()?;
        Ok(token.map(|token| PlexArtworkConnectionAdmin { address, token }))
    }

    async fn first_jellyfin_address_admin(&self) -> Result<Option<String>> {
        sqlx::query_scalar::<_, Option<String>>(
            r#"SELECT "Address" FROM "JellyfinConnection" ORDER BY "Id" LIMIT 1"#,
        )
        .fetch_optional(&self.0)
        .await
        .repository()
        .map(Option::flatten)
    }

    async fn first_emby_address_admin(&self) -> Result<Option<String>> {
        sqlx::query_scalar::<_, Option<String>>(
            r#"SELECT "Address" FROM "EmbyConnection" ORDER BY "Id" LIMIT 1"#,
        )
        .fetch_optional(&self.0)
        .await
        .repository()
        .map(Option::flatten)
    }

    async fn list_local_libraries_admin(&self) -> Result<Vec<LocalLibraryAdminRow>> {
        let rows = sqlx::query(
            r#"SELECT l."Id", l."Name", l."MediaKind", COUNT(lp."Id") AS "PathCount",
                      CASE WHEN aml."LibraryId" IS NULL THEN 0 ELSE 1 END AS "ManagedByAddon"
               FROM "Library" l
               JOIN "LocalLibrary" ll ON ll."Id" = l."Id"
               LEFT JOIN "LibraryPath" lp ON lp."LibraryId" = l."Id"
               LEFT JOIN "AddonMediaListLibrary" aml ON aml."LibraryId" = l."Id"
               GROUP BY l."Id", l."Name", l."MediaKind", aml."LibraryId"
               ORDER BY l."Name""#,
        )
        .fetch_all(&self.0)
        .await
        .repository()?;
        rows.iter()
            .map(|row| {
                let path_count: i64 = row.try_get("PathCount").repository()?;
                Ok(LocalLibraryAdminRow {
                    id: row.try_get("Id").repository()?,
                    name: row.try_get("Name").repository()?,
                    media_kind: row.try_get("MediaKind").repository()?,
                    path_count: u64::try_from(path_count).map_err(|error| {
                        RepositoryError::Decode {
                            kind: "local library path count",
                            value: error.to_string(),
                        }
                    })?,
                    managed_by_addon: decode_bool_column(row, "ManagedByAddon")?,
                })
            })
            .collect()
    }

    async fn list_managed_addon_media_sources_admin(
        &self,
    ) -> Result<Vec<ManagedAddonMediaSourceAdminRow>> {
        let rows = sqlx::query(
            r#"SELECT "AddonId", "Name", "RootPath"
               FROM "AddonMediaSource" ORDER BY "Name", "AddonId""#,
        )
        .fetch_all(&self.0)
        .await
        .repository()?;
        rows.iter()
            .map(|row| {
                Ok(ManagedAddonMediaSourceAdminRow {
                    addon_id: row.try_get("AddonId").repository()?,
                    name: row.try_get("Name").repository()?,
                    root_path: row.try_get("RootPath").repository()?,
                })
            })
            .collect()
    }

    async fn get_local_library_admin(&self, id: EntityId) -> Result<Option<LocalLibraryAdmin>> {
        let row = sqlx::query(
            r#"SELECT l."Id", l."Name", l."MediaKind"
               FROM "Library" l
               JOIN "LocalLibrary" ll ON ll."Id" = l."Id"
               WHERE l."Id" = ?"#,
        )
        .bind(id)
        .fetch_optional(&self.0)
        .await
        .repository()?;
        let Some(row) = row else {
            return Ok(None);
        };
        let paths = sqlx::query(
            r#"SELECT "Id", COALESCE("Path", '') AS "Path"
               FROM "LibraryPath" WHERE "LibraryId" = ? ORDER BY "Id""#,
        )
        .bind(id)
        .fetch_all(&self.0)
        .await
        .repository()?
        .iter()
        .map(|path| {
            Ok(LocalLibraryPathAdmin {
                id: path.try_get("Id").repository()?,
                path: path.try_get("Path").repository()?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
        Ok(Some(LocalLibraryAdmin {
            id: row.try_get("Id").repository()?,
            name: row.try_get("Name").repository()?,
            media_kind: row.try_get("MediaKind").repository()?,
            paths,
        }))
    }

    async fn save_local_library_admin(&self, library: &LocalLibraryAdmin) -> Result<EntityId> {
        if library.id != 0 {
            let managed: i64 = sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM "AddonMediaListLibrary" WHERE "LibraryId" = ?"#,
            )
            .bind(library.id)
            .fetch_one(&self.0)
            .await
            .repository()?;
            if managed > 0 {
                return Err(RepositoryError::Conflict(
                    "managed add-on libraries must be changed through Add-on Lists".to_owned(),
                ));
            }
        }
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let id = if library.id == 0 {
            let source = sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (NULL)"#)
                .execute(&mut *tx)
                .await
                .repository()?;
            let source_id = crate::db::last_insert_id(source);
            sqlx::query(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (?)"#)
                .bind(source_id)
                .execute(&mut *tx)
                .await
                .repository()?;
            let inserted = sqlx::query(
                r#"INSERT INTO "Library" ("MediaSourceId", "MediaKind", "Name")
                   VALUES (?, ?, ?)"#,
            )
            .bind(source_id)
            .bind(library.media_kind)
            .bind(&library.name)
            .execute(&mut *tx)
            .await
            .repository()?;
            let id = crate::db::last_insert_id(inserted);
            sqlx::query(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (?)"#)
                .bind(id)
                .execute(&mut *tx)
                .await
                .repository()?;
            id
        } else {
            sqlx::query(r#"UPDATE "Library" SET "Name" = ?, "MediaKind" = ? WHERE "Id" = ?"#)
                .bind(&library.name)
                .bind(library.media_kind)
                .bind(library.id)
                .execute(&mut *tx)
                .await
                .repository()?;
            library.id
        };
        sqlx::query(r#"DELETE FROM "LibraryPath" WHERE "LibraryId" = ?"#)
            .bind(id)
            .execute(&mut *tx)
            .await
            .repository()?;
        for path in &library.paths {
            sqlx::query(r#"INSERT INTO "LibraryPath" ("LibraryId", "Path") VALUES (?, ?)"#)
                .bind(id)
                .bind(&path.path)
                .execute(&mut *tx)
                .await
                .repository()?;
        }
        tx.commit().await.repository()?;
        Ok(id)
    }

    async fn delete_local_library_admin(&self, id: EntityId) -> Result<()> {
        let managed: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "AddonMediaListLibrary" WHERE "LibraryId" = ?"#,
        )
        .bind(id)
        .fetch_one(&self.0)
        .await
        .repository()?;
        if managed > 0 {
            return Err(RepositoryError::Conflict(
                "managed add-on libraries must be deleted through Add-on Lists".to_owned(),
            ));
        }
        sqlx::query(
            r#"DELETE FROM "MediaSource"
               WHERE "Id" = (SELECT "MediaSourceId" FROM "Library" WHERE "Id" = ?)"#,
        )
        .bind(id)
        .execute(&self.0)
        .await
        .repository()?;
        Ok(())
    }

    async fn list_remote_sources_admin(
        &self,
        kind: RemoteMediaSourceKind,
    ) -> Result<Vec<RemoteSourceAdminRow>> {
        let sql = match kind {
            RemoteMediaSourceKind::Jellyfin => {
                r#"SELECT source."Id",
                          COALESCE(source."ServerName", 'Jellyfin') AS "Name",
                          COALESCE((SELECT "Address" FROM "JellyfinConnection"
                            WHERE "JellyfinMediaSourceId" = source."Id"
                            ORDER BY "Id" LIMIT 1), '') AS "Address"
                   FROM "JellyfinMediaSource" source ORDER BY source."Id""#
            }
            RemoteMediaSourceKind::Emby => {
                r#"SELECT source."Id",
                          COALESCE(source."ServerName", 'Emby') AS "Name",
                          COALESCE((SELECT "Address" FROM "EmbyConnection"
                            WHERE "EmbyMediaSourceId" = source."Id"
                            ORDER BY "Id" LIMIT 1), '') AS "Address"
                   FROM "EmbyMediaSource" source ORDER BY source."Id""#
            }
            RemoteMediaSourceKind::Plex => {
                r#"SELECT source."Id",
                          COALESCE(source."ServerName", 'Plex') AS "Name",
                          COALESCE((SELECT "Uri" FROM "PlexConnection"
                            WHERE "PlexMediaSourceId" = source."Id" AND "IsActive" = 1
                            ORDER BY "Id" LIMIT 1), '') AS "Address"
                   FROM "PlexMediaSource" source ORDER BY source."Id""#
            }
        };
        let rows = sqlx::query(sql).fetch_all(&self.0).await.repository()?;
        rows.iter()
            .map(|row| {
                Ok(RemoteSourceAdminRow {
                    id: row.try_get("Id").repository()?,
                    name: row.try_get("Name").repository()?,
                    address: row.try_get("Address").repository()?,
                })
            })
            .collect()
    }

    async fn get_remote_connection_admin(
        &self,
        kind: RemoteMediaSourceKind,
    ) -> Result<Option<RemoteConnectionAdmin>> {
        let sql = match kind {
            RemoteMediaSourceKind::Jellyfin => {
                r#"SELECT source."Id",
                          COALESCE((SELECT "Address" FROM "JellyfinConnection"
                            WHERE "JellyfinMediaSourceId" = source."Id"
                            ORDER BY "Id" LIMIT 1), '') AS "Address"
                   FROM "JellyfinMediaSource" source ORDER BY source."Id" LIMIT 1"#
            }
            RemoteMediaSourceKind::Emby => {
                r#"SELECT source."Id",
                          COALESCE((SELECT "Address" FROM "EmbyConnection"
                            WHERE "EmbyMediaSourceId" = source."Id"
                            ORDER BY "Id" LIMIT 1), '') AS "Address"
                   FROM "EmbyMediaSource" source ORDER BY source."Id" LIMIT 1"#
            }
            RemoteMediaSourceKind::Plex => {
                r#"SELECT source."Id",
                          COALESCE((SELECT "Uri" FROM "PlexConnection"
                            WHERE "PlexMediaSourceId" = source."Id" AND "IsActive" = 1
                            ORDER BY "Id" LIMIT 1), '') AS "Address"
                   FROM "PlexMediaSource" source ORDER BY source."Id" LIMIT 1"#
            }
        };
        sqlx::query(sql)
            .fetch_optional(&self.0)
            .await
            .repository()?
            .map(|row| {
                Ok(RemoteConnectionAdmin {
                    source_id: row.try_get("Id").repository()?,
                    address: row.try_get("Address").repository()?,
                })
            })
            .transpose()
    }

    async fn save_remote_connection_admin(
        &self,
        kind: RemoteMediaSourceKind,
        connection: &RemoteConnectionAdmin,
        server_name: &str,
        operating_system: &str,
    ) -> Result<EntityId> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let id = if connection.source_id == 0 {
            let inserted = sqlx::query(r#"INSERT INTO "MediaSource" DEFAULT VALUES"#)
                .execute(&mut *tx)
                .await
                .repository()?;
            let id = crate::db::last_insert_id(inserted);
            match kind {
                RemoteMediaSourceKind::Jellyfin => {
                    sqlx::query(
                        r#"INSERT INTO "JellyfinMediaSource"
                           ("Id", "ServerName", "OperatingSystem") VALUES (?, ?, ?)"#,
                    )
                    .bind(id)
                    .bind(server_name)
                    .bind(operating_system)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
                    sqlx::query(
                        r#"INSERT INTO "JellyfinConnection"
                           ("Address", "JellyfinMediaSourceId") VALUES (?, ?)"#,
                    )
                    .bind(&connection.address)
                    .bind(id)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
                }
                RemoteMediaSourceKind::Emby => {
                    sqlx::query(
                        r#"INSERT INTO "EmbyMediaSource" ("Id", "ServerName") VALUES (?, ?)"#,
                    )
                    .bind(id)
                    .bind(server_name)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
                    sqlx::query(
                        r#"INSERT INTO "EmbyConnection"
                           ("Address", "EmbyMediaSourceId") VALUES (?, ?)"#,
                    )
                    .bind(&connection.address)
                    .bind(id)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
                }
                RemoteMediaSourceKind::Plex => {
                    return Err(RepositoryError::Conflict(
                        "Plex connections are discovered through PIN authentication".to_owned(),
                    ));
                }
            }
            id
        } else {
            match kind {
                RemoteMediaSourceKind::Jellyfin => {
                    sqlx::query(
                        r#"UPDATE "JellyfinMediaSource"
                           SET "ServerName" = ?, "OperatingSystem" = ? WHERE "Id" = ?"#,
                    )
                    .bind(server_name)
                    .bind(operating_system)
                    .bind(connection.source_id)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
                    sqlx::query(
                        r#"UPDATE "JellyfinConnection" SET "Address" = ?
                           WHERE "JellyfinMediaSourceId" = ?
                           AND "Id" = (SELECT MIN("Id") FROM "JellyfinConnection"
                             WHERE "JellyfinMediaSourceId" = ?)"#,
                    )
                    .bind(&connection.address)
                    .bind(connection.source_id)
                    .bind(connection.source_id)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
                }
                RemoteMediaSourceKind::Emby => {
                    sqlx::query(
                        r#"UPDATE "EmbyConnection" SET "Address" = ?
                           WHERE "EmbyMediaSourceId" = ?
                           AND "Id" = (SELECT MIN("Id") FROM "EmbyConnection"
                             WHERE "EmbyMediaSourceId" = ?)"#,
                    )
                    .bind(&connection.address)
                    .bind(connection.source_id)
                    .bind(connection.source_id)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
                }
                RemoteMediaSourceKind::Plex => {
                    return Err(RepositoryError::Conflict(
                        "Plex connections are discovered through PIN authentication".to_owned(),
                    ));
                }
            }
            connection.source_id
        };
        tx.commit().await.repository()?;
        Ok(id)
    }

    async fn delete_remote_source_admin(&self, id: EntityId) -> Result<()> {
        sqlx::query(r#"DELETE FROM "MediaSource" WHERE "Id" = ?"#)
            .bind(id)
            .execute(&self.0)
            .await
            .repository()?;
        Ok(())
    }

    async fn list_library_preferences_admin(
        &self,
        kind: RemoteMediaSourceKind,
        source_id: EntityId,
    ) -> Result<Vec<LibraryPreferenceAdmin>> {
        let sql = match kind {
            RemoteMediaSourceKind::Jellyfin => {
                r#"SELECT l."Id", COALESCE(l."Name", '') AS "Name", l."MediaKind",
                          provider."ShouldSyncItems"
                   FROM "Library" l JOIN "JellyfinLibrary" provider ON provider."Id" = l."Id"
                   WHERE l."MediaSourceId" = ? ORDER BY l."MediaKind", l."Name""#
            }
            RemoteMediaSourceKind::Emby => {
                r#"SELECT l."Id", COALESCE(l."Name", '') AS "Name", l."MediaKind",
                          provider."ShouldSyncItems"
                   FROM "Library" l JOIN "EmbyLibrary" provider ON provider."Id" = l."Id"
                   WHERE l."MediaSourceId" = ? ORDER BY l."MediaKind", l."Name""#
            }
            RemoteMediaSourceKind::Plex => {
                r#"SELECT l."Id", COALESCE(l."Name", '') AS "Name", l."MediaKind",
                          provider."ShouldSyncItems"
                   FROM "Library" l JOIN "PlexLibrary" provider ON provider."Id" = l."Id"
                   WHERE l."MediaSourceId" = ? ORDER BY l."MediaKind", l."Name""#
            }
        };
        let rows = sqlx::query(sql)
            .bind(source_id)
            .fetch_all(&self.0)
            .await
            .repository()?;
        rows.iter()
            .map(|row| {
                Ok(LibraryPreferenceAdmin {
                    id: row.try_get("Id").repository()?,
                    name: row.try_get("Name").repository()?,
                    media_kind: row.try_get("MediaKind").repository()?,
                    should_sync: row.try_get("ShouldSyncItems").repository()?,
                })
            })
            .collect()
    }

    async fn save_library_preferences_admin(
        &self,
        kind: RemoteMediaSourceKind,
        preferences: &[LibraryPreferenceAdmin],
    ) -> Result<()> {
        let sql = match kind {
            RemoteMediaSourceKind::Jellyfin => {
                r#"UPDATE "JellyfinLibrary" SET "ShouldSyncItems" = ? WHERE "Id" = ?"#
            }
            RemoteMediaSourceKind::Emby => {
                r#"UPDATE "EmbyLibrary" SET "ShouldSyncItems" = ? WHERE "Id" = ?"#
            }
            RemoteMediaSourceKind::Plex => {
                r#"UPDATE "PlexLibrary" SET "ShouldSyncItems" = ? WHERE "Id" = ?"#
            }
        };
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        for preference in preferences {
            sqlx::query(sql)
                .bind(preference.should_sync)
                .bind(preference.id)
                .execute(&mut *tx)
                .await
                .repository()?;
        }
        tx.commit().await.repository()
    }

    async fn list_path_replacements_admin(
        &self,
        kind: RemoteMediaSourceKind,
        source_id: EntityId,
    ) -> Result<Vec<PathReplacementAdmin>> {
        let sql = match kind {
            RemoteMediaSourceKind::Jellyfin => {
                r#"SELECT "Id", COALESCE("JellyfinPath", '') AS "RemotePath",
                          COALESCE("LocalPath", '') AS "LocalPath"
                   FROM "JellyfinPathReplacement"
                   WHERE "JellyfinMediaSourceId" = ? ORDER BY "Id""#
            }
            RemoteMediaSourceKind::Emby => {
                r#"SELECT "Id", COALESCE("EmbyPath", '') AS "RemotePath",
                          COALESCE("LocalPath", '') AS "LocalPath"
                   FROM "EmbyPathReplacement"
                   WHERE "EmbyMediaSourceId" = ? ORDER BY "Id""#
            }
            RemoteMediaSourceKind::Plex => {
                r#"SELECT "Id", COALESCE("PlexPath", '') AS "RemotePath",
                          COALESCE("LocalPath", '') AS "LocalPath"
                   FROM "PlexPathReplacement"
                   WHERE "PlexMediaSourceId" = ? ORDER BY "Id""#
            }
        };
        let rows = sqlx::query(sql)
            .bind(source_id)
            .fetch_all(&self.0)
            .await
            .repository()?;
        rows.iter()
            .map(|row| {
                Ok(PathReplacementAdmin {
                    id: row.try_get("Id").repository()?,
                    remote_path: row.try_get("RemotePath").repository()?,
                    local_path: row.try_get("LocalPath").repository()?,
                })
            })
            .collect()
    }

    async fn save_path_replacements_admin(
        &self,
        kind: RemoteMediaSourceKind,
        source_id: EntityId,
        paths: &[PathReplacementAdmin],
    ) -> Result<()> {
        let (delete_sql, insert_sql) = match kind {
            RemoteMediaSourceKind::Jellyfin => (
                r#"DELETE FROM "JellyfinPathReplacement" WHERE "JellyfinMediaSourceId" = ?"#,
                r#"INSERT INTO "JellyfinPathReplacement"
                   ("JellyfinPath", "LocalPath", "JellyfinMediaSourceId") VALUES (?, ?, ?)"#,
            ),
            RemoteMediaSourceKind::Emby => (
                r#"DELETE FROM "EmbyPathReplacement" WHERE "EmbyMediaSourceId" = ?"#,
                r#"INSERT INTO "EmbyPathReplacement"
                   ("EmbyPath", "LocalPath", "EmbyMediaSourceId") VALUES (?, ?, ?)"#,
            ),
            RemoteMediaSourceKind::Plex => (
                r#"DELETE FROM "PlexPathReplacement" WHERE "PlexMediaSourceId" = ?"#,
                r#"INSERT INTO "PlexPathReplacement"
                   ("PlexPath", "LocalPath", "PlexMediaSourceId") VALUES (?, ?, ?)"#,
            ),
        };
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        sqlx::query(delete_sql)
            .bind(source_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        for path in paths {
            sqlx::query(insert_sql)
                .bind(&path.remote_path)
                .bind(&path.local_path)
                .bind(source_id)
                .execute(&mut *tx)
                .await
                .repository()?;
        }
        tx.commit().await.repository()
    }

    async fn delete_all_plex_sources_admin(&self) -> Result<()> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key" LIKE 'plex.%auth_token%'"#)
            .execute(&mut *tx)
            .await
            .repository()?;
        sqlx::query(
            r#"DELETE FROM "MediaSource" WHERE "Id" IN (SELECT "Id" FROM "PlexMediaSource")"#,
        )
        .execute(&mut *tx)
        .await
        .repository()?;
        tx.commit().await.repository()
    }
}

async fn upsert_source<T>(
    pool: &Pool<crate::db::Db>,
    base: &MediaSource,
    provider: &T,
) -> Result<()>
where
    T: ProviderUpsert + Sync,
{
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    // "MediaSource" only has "Id"
    sqlx::query(r#"INSERT OR IGNORE INTO "MediaSource" ("Id") VALUES (?)"#)
        .bind(base.id)
        .execute(&mut *tx)
        .await
        .repository()?;

    provider.upsert(&mut tx).await?;

    tx.commit().await.repository()?;
    Ok(())
}

trait ProviderUpsert {
    fn upsert<'a>(
        &'a self,
        tx: &'a mut sqlx::Transaction<'_, crate::db::Db>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>>;
}

impl ProviderUpsert for PlexMediaSource {
    fn upsert<'a>(
        &'a self,
        tx: &'a mut sqlx::Transaction<'_, crate::db::Db>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            sqlx::query(upsert_sql!(
                r#"INSERT INTO "PlexMediaSource" ("Id", "ServerName", "ClientIdentifier")
                 VALUES (?, ?, ?)"#,
                &[&["Id"]],
            ))
            .bind(self.media_source.id)
            .bind(&self.server_name)
            .bind(&self.machine_identifier)
            .execute(&mut **tx)
            .await
            .repository()?;

            sqlx::query(r#"DELETE FROM "PlexConnection" WHERE "PlexMediaSourceId" = ?"#)
                .bind(self.media_source.id)
                .execute(&mut **tx)
                .await
                .repository()?;
            for connection in &self.connections {
                sqlx::query(
                    r#"REPLACE INTO "PlexConnection" ("Id", "PlexMediaSourceId", "IsActive", "Uri")
                     VALUES (?, ?, ?, ?)"#,
                )
                .bind(connection.id)
                .bind(self.media_source.id)
                .bind(connection.is_active)
                .bind(&connection.uri)
                .execute(&mut **tx)
                .await
                .repository()?;
            }

            sqlx::query(r#"DELETE FROM "PlexPathReplacement" WHERE "PlexMediaSourceId" = ?"#)
                .bind(self.media_source.id)
                .execute(&mut **tx)
                .await
                .repository()?;
            for replacement in &self.path_replacements {
                sqlx::query(
                    r#"REPLACE INTO "PlexPathReplacement" (
                        "Id", "PlexMediaSourceId", "PlexPath", "LocalPath"
                     ) VALUES (?, ?, ?, ?)"#,
                )
                .bind(replacement.id)
                .bind(self.media_source.id)
                .bind(&replacement.path)
                .bind(&replacement.replacement_path)
                .execute(&mut **tx)
                .await
                .repository()?;
            }

            Ok(())
        })
    }
}

impl ProviderUpsert for JellyfinMediaSource {
    fn upsert<'a>(
        &'a self,
        tx: &'a mut sqlx::Transaction<'_, crate::db::Db>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            sqlx::query(upsert_sql!(
                r#"INSERT INTO "JellyfinMediaSource" (
                    "Id", "ServerName", "OperatingSystem", "LastCollectionsScan"
                 ) VALUES (?, ?, ?, ?)"#,
                &[&["Id"]],
            ))
            .bind(self.media_source.id)
            .bind(&self.server_name)
            .bind(&self.operating_system)
            .bind(encode_optional_datetime(self.last_collections_scan))
            .execute(&mut **tx)
            .await
            .repository()?;

            sqlx::query(r#"DELETE FROM "JellyfinConnection" WHERE "JellyfinMediaSourceId" = ?"#)
                .bind(self.media_source.id)
                .execute(&mut **tx)
                .await
                .repository()?;
            for connection in &self.connections {
                sqlx::query(
                    r#"REPLACE INTO "JellyfinConnection" ("Id", "JellyfinMediaSourceId", "Address")
                     VALUES (?, ?, ?)"#,
                )
                .bind(connection.id)
                .bind(self.media_source.id)
                .bind(&connection.address)
                .execute(&mut **tx)
                .await
                .repository()?;
            }

            sqlx::query(
                r#"DELETE FROM "JellyfinPathReplacement" WHERE "JellyfinMediaSourceId" = ?"#,
            )
            .bind(self.media_source.id)
            .execute(&mut **tx)
            .await
            .repository()?;
            for replacement in &self.path_replacements {
                sqlx::query(
                    r#"REPLACE INTO "JellyfinPathReplacement" (
                        "Id", "JellyfinMediaSourceId", "JellyfinPath", "LocalPath"
                     ) VALUES (?, ?, ?, ?)"#,
                )
                .bind(replacement.id)
                .bind(self.media_source.id)
                .bind(&replacement.path)
                .bind(&replacement.replacement_path)
                .execute(&mut **tx)
                .await
                .repository()?;
            }

            Ok(())
        })
    }
}

impl ProviderUpsert for EmbyMediaSource {
    fn upsert<'a>(
        &'a self,
        tx: &'a mut sqlx::Transaction<'_, crate::db::Db>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            sqlx::query(upsert_sql!(
                r#"INSERT INTO "EmbyMediaSource" (
                    "Id", "ServerName", "OperatingSystem", "LastCollectionsScan"
                 ) VALUES (?, ?, ?, ?)"#,
                &[&["Id"]],
            ))
            .bind(self.media_source.id)
            .bind(&self.server_name)
            .bind(&self.operating_system)
            .bind(encode_optional_datetime(self.last_collections_scan))
            .execute(&mut **tx)
            .await
            .repository()?;

            sqlx::query(r#"DELETE FROM "EmbyConnection" WHERE "EmbyMediaSourceId" = ?"#)
                .bind(self.media_source.id)
                .execute(&mut **tx)
                .await
                .repository()?;
            for connection in &self.connections {
                sqlx::query(
                    r#"REPLACE INTO "EmbyConnection" ("Id", "EmbyMediaSourceId", "Address")
                     VALUES (?, ?, ?)"#,
                )
                .bind(connection.id)
                .bind(self.media_source.id)
                .bind(&connection.address)
                .execute(&mut **tx)
                .await
                .repository()?;
            }

            sqlx::query(r#"DELETE FROM "EmbyPathReplacement" WHERE "EmbyMediaSourceId" = ?"#)
                .bind(self.media_source.id)
                .execute(&mut **tx)
                .await
                .repository()?;
            for replacement in &self.path_replacements {
                sqlx::query(
                    r#"REPLACE INTO "EmbyPathReplacement" (
                        "Id", "EmbyMediaSourceId", "EmbyPath", "LocalPath"
                     ) VALUES (?, ?, ?, ?)"#,
                )
                .bind(replacement.id)
                .bind(self.media_source.id)
                .bind(&replacement.path)
                .bind(&replacement.replacement_path)
                .execute(&mut **tx)
                .await
                .repository()?;
            }

            Ok(())
        })
    }
}

async fn fetch_base_source(
    pool: &Pool<crate::db::Db>,
    id: EntityId,
    expected_kind: MediaSourceKind,
) -> Result<MediaSource> {
    // Verify the MediaSource row exists
    let exists: bool = sqlx::query_scalar::<_, bool>(
        r#"SELECT EXISTS(SELECT 1 FROM "MediaSource" WHERE "Id" = ?)"#,
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .repository()?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn plex_artwork_connection_uses_the_owning_sources_uri_and_token() {
        let pool = crate::db::init("sqlite::memory:")
            .await
            .expect("initialize database");
        for source_id in [41_i64, 42] {
            sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (?)"#)
                .bind(source_id)
                .execute(&pool)
                .await
                .expect("seed media source");
        }
        sqlx::query(
            r#"INSERT INTO "PlexMediaSource" ("Id", "ClientIdentifier", "ServerName")
               VALUES (41, 'alpha', 'Alpha'), (42, 'beta', 'Beta')"#,
        )
        .execute(&pool)
        .await
        .expect("seed Plex sources");
        sqlx::query(
            r#"INSERT INTO "PlexConnection" ("PlexMediaSourceId", "IsActive", "Uri")
               VALUES (41, 1, 'http://alpha.invalid'),
                      (42, 1, 'http://beta.invalid')"#,
        )
        .execute(&pool)
        .await
        .expect("seed Plex connections");
        sqlx::query(
            r#"INSERT INTO "ConfigElement" ("Key", "Value")
               VALUES ('plex.server_auth_token.alpha', 'alpha-token'),
                      ('plex.server_auth_token.beta', 'beta-token'),
                      ('plex.current_server_auth_token', 'wrong-global-token')"#,
        )
        .execute(&pool)
        .await
        .expect("seed Plex tokens");

        let repository = SqliteMediaSourceRepository(pool);
        assert_eq!(
            repository
                .plex_artwork_connection_admin(42)
                .await
                .expect("resolve artwork connection"),
            Some(PlexArtworkConnectionAdmin {
                address: "http://beta.invalid".to_owned(),
                token: "beta-token".to_owned(),
            })
        );
    }
}
