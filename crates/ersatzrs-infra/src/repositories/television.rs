use crate::metadata_query::{load_projected_artwork, projected_artwork};
use crate::repositories::SqlxResultExt as _;
use async_trait::async_trait;
use ersatzrs_core::path_replacement::{replace_path, PathPlatform, PathReplacement};
use ersatzrs_core::repositories::{
    EpisodeCard, EpisodeMediaItemInfo, Result, SeasonDetail, TelevisionRepository,
};
use ersatzrs_domain::EntityId;
use sqlx::{Pool, Row};
use std::collections::HashMap;

#[derive(Clone)]
pub struct SqliteTelevisionRepository(pub Pool<crate::db::Db>);

#[async_trait]
impl TelevisionRepository for SqliteTelevisionRepository {
    async fn get_season(&self, season_id: EntityId) -> Result<Option<SeasonDetail>> {
        let Some(row) = sqlx::query(
            r#"SELECT s."Id" AS "Id",
                      s."ShowId" AS "ShowId",
                      s."SeasonNumber" AS "SeasonNumber",
                      COALESCE(shm."Title", '') AS "ShowTitle",
                      shm."Year" AS "ShowYear",
                      (
                          SELECT sa."Id"
                          FROM "Artwork" sa
                          WHERE sa."SeasonMetadataId" = sm."Id"
                            AND sa."ArtworkKind" = 0
                            AND NULLIF(TRIM(sa."Path"), '') IS NOT NULL
                          ORDER BY sa."Id"
                          LIMIT 1
                      ) AS "SeasonPoster",
                      (
                          SELECT a."Id" FROM "Artwork" a
                          WHERE a."ShowMetadataId" = shm."Id"
                            AND a."ArtworkKind" = 0
                            AND NULLIF(TRIM(a."Path"), '') IS NOT NULL
                          ORDER BY a."Id" LIMIT 1
                      ) AS "ShowPoster",
                      (
                          SELECT fa."Id"
                          FROM "Artwork" fa
                          WHERE fa."ShowMetadataId" = shm."Id"
                            AND fa."ArtworkKind" = 3
                            AND NULLIF(TRIM(fa."Path"), '') IS NOT NULL
                          ORDER BY fa."Id"
                          LIMIT 1
                      ) AS "ShowFanArt"
               FROM "Season" s
               JOIN "Show" sh ON sh."Id" = s."ShowId"
               JOIN "ShowMetadata" shm ON shm."ShowId" = sh."Id"
               LEFT JOIN "SeasonMetadata" sm ON sm."SeasonId" = s."Id"
               WHERE s."Id" = ?
               ORDER BY s."Id"
               LIMIT 1"#,
        )
        .bind(season_id)
        .fetch_optional(&self.0)
        .await
        .repository()?
        else {
            return Ok(None);
        };

        let artwork = load_projected_artwork(
            &self.0,
            std::slice::from_ref(&row),
            &["SeasonPoster", "ShowPoster", "ShowFanArt"],
        )
        .await?;
        Ok(Some(SeasonDetail {
            id: row.try_get("Id").repository()?,
            show_id: row.try_get("ShowId").repository()?,
            show_title: row.try_get("ShowTitle").repository()?,
            show_year: row.try_get("ShowYear").repository()?,
            season_number: row.try_get("SeasonNumber").repository()?,
            season_poster_path: projected_artwork(&row, "SeasonPoster", &artwork)?
                .or(projected_artwork(&row, "ShowPoster", &artwork)?),
            show_fanart_path: projected_artwork(&row, "ShowFanArt", &artwork)?,
        }))
    }

    async fn get_episode_count(&self, season_id: EntityId) -> Result<i64> {
        let count = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Episode" WHERE "SeasonId" = ?"#)
            .bind(season_id)
            .fetch_one(&self.0)
            .await
            .repository()?;
        Ok(count)
    }

    async fn get_paged_episodes(
        &self,
        season_id: EntityId,
        page_number: i64,
        page_size: i64,
    ) -> Result<Vec<EpisodeCard>> {
        let page_number = page_number.max(1);
        let page_size = page_size.max(1);
        let offset = (page_number - 1) * page_size;

        let rows = sqlx::query(
            r#"SELECT e."Id" AS "EpisodeId",
                      em."Id" AS "MetadataId",
                      COALESCE(em."EpisodeNumber", 0) AS "EpisodeNumber",
                      COALESCE(em."Title", '') AS "Title",
                      COALESCE(em."SortTitle", em."Title", '') AS "SortTitle",
                      em."Plot" AS "Plot",
                      em."ReleaseDate" AS "ReleaseDate",
                      (
                          SELECT a."Id"
                          FROM "Artwork" a
                          WHERE a."EpisodeMetadataId" = em."Id"
                            AND a."ArtworkKind" = 1
                            AND NULLIF(TRIM(a."Path"), '') IS NOT NULL
                          ORDER BY a."Id"
                          LIMIT 1
                      ) AS "Thumbnail",
                      COALESCE(mi."State", 0) AS "State",
                      mi."LibraryPathId" AS "LibraryPathId",
                      (
                          SELECT mf."Path"
                          FROM "MediaVersion" mv
                          JOIN "MediaFile" mf ON mf."MediaVersionId" = mv."Id"
                          WHERE mv."EpisodeId" = e."Id" AND mf."Path" IS NOT NULL
                          ORDER BY mv."Id", mf."Id"
                          LIMIT 1
                      ) AS "FilePath",
                      COALESCE(shm."Title", '') AS "ShowTitle",
                      sh."Id" AS "ShowId",
                      s."Id" AS "SeasonId",
                      COALESCE(s."SeasonNumber", 0) AS "SeasonNumber"
               FROM "EpisodeMetadata" em
               JOIN "Episode" e ON e."Id" = em."EpisodeId"
               JOIN "Season" s ON s."Id" = e."SeasonId"
               JOIN "Show" sh ON sh."Id" = s."ShowId"
               JOIN "ShowMetadata" shm ON shm."ShowId" = sh."Id"
               LEFT JOIN "MediaItem" mi ON mi."Id" = e."Id"
               WHERE e."SeasonId" = ?
               ORDER BY em."EpisodeNumber", em."Id"
               LIMIT ? OFFSET ?"#,
        )
        .bind(season_id)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.0)
        .await
        .repository()?;

        let artwork = load_projected_artwork(&self.0, &rows, &["Thumbnail"]).await?;
        let mut episodes = Vec::with_capacity(rows.len());
        let mut replacements_by_library_path = HashMap::new();
        for row in rows {
            let metadata_id: EntityId = row.try_get("MetadataId").repository()?;
            let file_path: Option<String> = row.try_get("FilePath").repository()?;
            let library_path_id: Option<EntityId> = row.try_get("LibraryPathId").repository()?;
            let local_path = match (file_path.as_deref(), library_path_id) {
                (Some(path), Some(library_path_id)) => {
                    let replacements = if let Some(replacements) =
                        replacements_by_library_path.get(&library_path_id)
                    {
                        replacements
                    } else {
                        let replacements =
                            load_path_replacements_for_library_path(&self.0, library_path_id)
                                .await?;
                        replacements_by_library_path.insert(library_path_id, replacements);
                        replacements_by_library_path
                            .get(&library_path_id)
                            .expect("inserted path replacements")
                    };
                    Some(resolve_local_path(path, replacements))
                }
                (Some(path), None) => Some(path.to_owned()),
                (None, _) => None,
            };
            episodes.push(EpisodeCard {
                episode_id: row.try_get("EpisodeId").repository()?,
                episode_number: row.try_get("EpisodeNumber").repository()?,
                title: row.try_get("Title").repository()?,
                sort_title: row.try_get("SortTitle").repository()?,
                plot: row.try_get("Plot").repository()?,
                release_date: row.try_get("ReleaseDate").repository()?,
                thumbnail_path: projected_artwork(&row, "Thumbnail", &artwork)?,
                state: row.try_get("State").repository()?,
                local_path,
                file_path,
                directors: load_people(&self.0, "Director", metadata_id).await?,
                writers: load_people(&self.0, "Writer", metadata_id).await?,
                show_title: row.try_get("ShowTitle").repository()?,
                show_id: row.try_get("ShowId").repository()?,
                season_id: row.try_get("SeasonId").repository()?,
                season_number: row.try_get("SeasonNumber").repository()?,
            });
        }

        Ok(episodes)
    }

    async fn get_episode_media_item_info(
        &self,
        episode_id: EntityId,
    ) -> Result<Option<EpisodeMediaItemInfo>> {
        let Some(row) = sqlx::query(
            r#"SELECT e."Id" AS "Id",
                      COALESCE(em."Title", '') AS "Title",
                      em."Id" AS "MetadataId",
                      COALESCE(em."EpisodeNumber", 0) AS "EpisodeNumber",
                      COALESCE(shm."Title", '') AS "ShowTitle",
                      COALESCE(s."SeasonNumber", 0) AS "SeasonNumber",
                      COALESCE(mi."State", 0) AS "State",
                      COALESCE(l."Name", '') AS "LibraryName",
                      CASE
                          WHEN ll."Id" IS NOT NULL THEN 'LocalLibrary'
                          WHEN pl."Id" IS NOT NULL THEN 'PlexLibrary'
                          WHEN jl."Id" IS NOT NULL THEN 'JellyfinLibrary'
                          WHEN el."Id" IS NOT NULL THEN 'EmbyLibrary'
                          ELSE 'Library'
                      END AS "LibraryKind",
                      COALESCE(pms."ServerName", jms."ServerName", ems."ServerName") AS "ServerName",
                      mv."Id" AS "MediaVersionId",
                      mv."Duration" AS "Duration",
                      mv."SampleAspectRatio" AS "SampleAspectRatio",
                      mv."DisplayAspectRatio" AS "DisplayAspectRatio",
                      mv."RFrameRate" AS "RFrameRate",
                      COALESCE(mv."VideoScanKind", 0) AS "VideoScanKind",
                      mv."InterlacedRatio" AS "InterlacedRatio",
                      COALESCE(mv."Width", 0) AS "Width",
                      COALESCE(mv."Height", 0) AS "Height"
               FROM "Episode" e
               JOIN "EpisodeMetadata" em ON em."EpisodeId" = e."Id"
               JOIN "Season" s ON s."Id" = e."SeasonId"
               JOIN "Show" sh ON sh."Id" = s."ShowId"
               JOIN "ShowMetadata" shm ON shm."ShowId" = sh."Id"
               LEFT JOIN "MediaItem" mi ON mi."Id" = e."Id"
               LEFT JOIN "LibraryPath" lp ON lp."Id" = mi."LibraryPathId"
               LEFT JOIN "Library" l ON l."Id" = lp."LibraryId"
               LEFT JOIN "LocalLibrary" ll ON ll."Id" = l."Id"
               LEFT JOIN "PlexLibrary" pl ON pl."Id" = l."Id"
               LEFT JOIN "JellyfinLibrary" jl ON jl."Id" = l."Id"
               LEFT JOIN "EmbyLibrary" el ON el."Id" = l."Id"
               LEFT JOIN "MediaSource" ms ON ms."Id" = l."MediaSourceId"
               LEFT JOIN "PlexMediaSource" pms ON pms."Id" = ms."Id"
               LEFT JOIN "JellyfinMediaSource" jms ON jms."Id" = ms."Id"
               LEFT JOIN "EmbyMediaSource" ems ON ems."Id" = ms."Id"
               LEFT JOIN "MediaVersion" mv ON mv."EpisodeId" = e."Id"
               WHERE e."Id" = ?
               ORDER BY mv."Id"
               LIMIT 1"#,
        )
        .bind(episode_id)
        .fetch_optional(&self.0)
        .await.repository()?
        else {
            return Ok(None);
        };

        let library_kind: String = row.try_get("LibraryKind").repository()?;
        Ok(Some(EpisodeMediaItemInfo {
            id: row.try_get("Id").repository()?,
            title: row.try_get("Title").repository()?,
            show_title: row.try_get("ShowTitle").repository()?,
            season_number: row.try_get("SeasonNumber").repository()?,
            episode_number: row.try_get("EpisodeNumber").repository()?,
            metadata_id: row.try_get("MetadataId").repository()?,
            server_name: row.try_get("ServerName").repository()?,
            library_name: row.try_get("LibraryName").repository()?,
            state: row.try_get("State").repository()?,
            media_version_id: row.try_get("MediaVersionId").repository()?,
            duration: row.try_get("Duration").repository()?,
            sample_aspect_ratio: row.try_get("SampleAspectRatio").repository()?,
            display_aspect_ratio: row.try_get("DisplayAspectRatio").repository()?,
            r_frame_rate: row.try_get("RFrameRate").repository()?,
            video_scan_kind: row.try_get("VideoScanKind").repository()?,
            interlaced_ratio: row.try_get("InterlacedRatio").repository()?,
            width: row.try_get("Width").repository()?,
            height: row.try_get("Height").repository()?,
            is_local_library: library_kind == "LocalLibrary",
            library_kind,
        }))
    }
}

async fn load_path_replacements_for_library_path(
    pool: &Pool<crate::db::Db>,
    library_path_id: EntityId,
) -> Result<Vec<PathReplacement>> {
    let rows = sqlx::query(
        r#"SELECT ppr."PlexPath" AS "Source",
                  ppr."LocalPath" AS "Local",
                  pms."Platform" AS "Platform",
                  ppr."Id" AS "SortId"
           FROM "LibraryPath" lp
           JOIN "Library" l ON l."Id" = lp."LibraryId"
           JOIN "PlexMediaSource" pms ON pms."Id" = l."MediaSourceId"
           JOIN "PlexPathReplacement" ppr ON ppr."PlexMediaSourceId" = pms."Id"
           WHERE lp."Id" = ?
           UNION ALL
           SELECT jpr."JellyfinPath" AS "Source",
                  jpr."LocalPath" AS "Local",
                  jms."OperatingSystem" AS "Platform",
                  jpr."Id" AS "SortId"
           FROM "LibraryPath" lp
           JOIN "Library" l ON l."Id" = lp."LibraryId"
           JOIN "JellyfinMediaSource" jms ON jms."Id" = l."MediaSourceId"
           JOIN "JellyfinPathReplacement" jpr ON jpr."JellyfinMediaSourceId" = jms."Id"
           WHERE lp."Id" = ?
           UNION ALL
           SELECT epr."EmbyPath" AS "Source",
                  epr."LocalPath" AS "Local",
                  ems."OperatingSystem" AS "Platform",
                  epr."Id" AS "SortId"
           FROM "LibraryPath" lp
           JOIN "Library" l ON l."Id" = lp."LibraryId"
           JOIN "EmbyMediaSource" ems ON ems."Id" = l."MediaSourceId"
           JOIN "EmbyPathReplacement" epr ON epr."EmbyMediaSourceId" = ems."Id"
           WHERE lp."Id" = ?
           ORDER BY "SortId""#,
    )
    .bind(library_path_id)
    .bind(library_path_id)
    .bind(library_path_id)
    .fetch_all(pool)
    .await
    .repository()?;

    let mut replacements = Vec::new();
    for row in rows {
        let source_path: Option<String> = row.try_get("Source").repository()?;
        let replacement_path: Option<String> = row.try_get("Local").repository()?;
        let platform: Option<String> = row.try_get("Platform").repository()?;
        if let (Some(source_path), Some(replacement_path)) = (source_path, replacement_path) {
            replacements.push(PathReplacement {
                source_platform: source_platform(&source_path, platform.as_deref()),
                source_path,
                replacement_path,
            });
        }
    }

    Ok(replacements)
}

fn resolve_local_path(path: &str, replacements: &[PathReplacement]) -> String {
    if replacements.is_empty() {
        path.to_owned()
    } else {
        replace_path(replacements, path, target_platform())
    }
}

fn source_platform(source_path: &str, platform: Option<&str>) -> PathPlatform {
    let source_looks_windows = source_path.contains('\\') || source_path.contains(':');
    let source_platform_is_windows = platform.is_some_and(|platform| {
        platform
            .trim_start()
            .to_ascii_lowercase()
            .starts_with("windows")
    });
    if source_looks_windows || source_platform_is_windows {
        PathPlatform::Windows
    } else {
        PathPlatform::Unix
    }
}

#[cfg(target_os = "windows")]
fn target_platform() -> PathPlatform {
    PathPlatform::Windows
}

#[cfg(not(target_os = "windows"))]
fn target_platform() -> PathPlatform {
    PathPlatform::Unix
}

async fn load_people(
    pool: &Pool<crate::db::Db>,
    table: &str,
    metadata_id: EntityId,
) -> Result<Vec<String>> {
    let sql = format!(
        r#"SELECT "Name"
           FROM "{table}"
           WHERE "EpisodeMetadataId" = ?
           ORDER BY "Id""#
    );
    let rows = sqlx::query(crate::db::assert_sql_safe(&sql))
        .bind(metadata_id)
        .fetch_all(pool)
        .await
        .repository()?;
    Ok(rows
        .iter()
        .filter_map(|row| row.try_get::<Option<String>, _>("Name").ok().flatten())
        .filter(|name| !name.trim().is_empty())
        .collect())
}
