use async_trait::async_trait;
use ersatzrs_core::path_replacement::{PathPlatform, PathReplacement};
use ersatzrs_core::repositories::{
    AdminActorCard, AdminArtistDetail, AdminAudioTrack, AdminCommentarySubtitle,
    AdminFacetedMediaDetail, AdminFacetedMediaKind, AdminMediaCard, AdminMediaInfoChapter,
    AdminMediaInfoKind, AdminMediaInfoStream, AdminMediaItemInfo, AdminMediaRepository,
    AdminMovieDetail, AdminMoviePlaybackSource, AdminMoviePlaybackStream, AdminOtherVideoDetail,
    AdminShowDetail, RepositoryError, Result, TelevisionRepository,
};
use ersatzrs_domain::{ContentKind, EntityId};
use sqlx::{Pool, Row};

use crate::metadata_query::{load_artwork_path, load_strings, split_content_ratings};
use crate::repositories::SqlxResultExt as _;

#[derive(Clone)]
pub struct SqliteAdminMediaRepository(pub Pool<crate::db::Db>);

fn decode_content_kind(value: i32) -> Result<ContentKind> {
    match value {
        0 => Ok(ContentKind::Primary),
        1 => Ok(ContentKind::Extra),
        2 => Ok(ContentKind::AlternateVersion),
        value => Err(RepositoryError::Decode {
            kind: "media content kind",
            value: value.to_string(),
        }),
    }
}

async fn load_movie_playback_sources(
    pool: &Pool<crate::db::Db>,
    movie_id: EntityId,
    metadata_id: EntityId,
) -> Result<Vec<AdminMoviePlaybackSource>> {
    let version_rows = sqlx::query(
        r#"SELECT "Id", COALESCE("Name", '') AS "Name",
                  "Width", "Height"
           FROM "MediaVersion"
           WHERE "MovieId" = ?
           ORDER BY "Id""#,
    )
    .bind(movie_id)
    .fetch_all(pool)
    .await
    .repository()?;
    let subtitle_rows = sqlx::query(
        r#"SELECT "StreamIndex", "Title", "Language", "Codec", "Default", "Forced"
           FROM "Subtitle"
           WHERE "MovieMetadataId" = ?
           ORDER BY "StreamIndex", "Id""#,
    )
    .bind(metadata_id)
    .fetch_all(pool)
    .await
    .repository()?;

    let mut sources = Vec::with_capacity(version_rows.len());
    for version in version_rows {
        let media_version_id = version.try_get("Id").repository()?;
        let stream_rows = sqlx::query(
            r#"SELECT "Index", "MediaStreamKind", "Title", "Language", "Codec",
                      "Channels", "Default", "Forced"
               FROM "MediaStream"
               WHERE "MediaVersionId" = ?
                 AND "MediaStreamKind" IN (1, 2, 3)
               ORDER BY "MediaStreamKind", "Index", "Id""#,
        )
        .bind(media_version_id)
        .fetch_all(pool)
        .await
        .repository()?;
        let mut streams = stream_rows
            .iter()
            .map(|stream| {
                let kind: i32 = stream.try_get("MediaStreamKind").repository()?;
                Ok(AdminMoviePlaybackStream {
                    index: stream.try_get("Index").repository()?,
                    kind,
                    title: stream.try_get("Title").repository()?,
                    language: stream.try_get("Language").repository()?,
                    codec: stream.try_get("Codec").repository()?,
                    channels: stream
                        .try_get::<i64, _>("Channels")
                        .repository()
                        .map(|value| (value > 0).then_some(value))?,
                    width: (kind == 1)
                        .then(|| version.try_get("Width").repository())
                        .transpose()?,
                    height: (kind == 1)
                        .then(|| version.try_get("Height").repository())
                        .transpose()?,
                    is_default: stream.try_get::<i64, _>("Default").repository()? != 0,
                    is_forced: stream.try_get::<i64, _>("Forced").repository()? != 0,
                    is_external: false,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        for subtitle in &subtitle_rows {
            let index = subtitle.try_get("StreamIndex").repository()?;
            if streams
                .iter()
                .any(|stream| stream.kind == 3 && stream.index == index)
            {
                continue;
            }
            streams.push(AdminMoviePlaybackStream {
                index,
                kind: 3,
                title: subtitle.try_get("Title").repository()?,
                language: subtitle.try_get("Language").repository()?,
                codec: subtitle.try_get("Codec").repository()?,
                channels: None,
                width: None,
                height: None,
                is_default: subtitle.try_get::<i64, _>("Default").repository()? != 0,
                is_forced: subtitle.try_get::<i64, _>("Forced").repository()? != 0,
                is_external: true,
            });
        }
        streams.sort_by_key(|stream| (stream.kind, stream.index, stream.is_external));
        sources.push(AdminMoviePlaybackSource {
            media_version_id,
            name: version.try_get("Name").repository()?,
            width: version.try_get("Width").repository()?,
            height: version.try_get("Height").repository()?,
            streams,
        });
    }
    Ok(sources)
}

async fn load_audio_languages(
    pool: &Pool<crate::db::Db>,
    owner_column: &str,
    item_id: EntityId,
) -> Result<Vec<String>> {
    let sql = format!(
        r#"SELECT DISTINCT ms."Language"
           FROM "MediaVersion" mv
           JOIN "MediaStream" ms ON ms."MediaVersionId" = mv."Id"
           WHERE mv."{owner_column}" = ? AND ms."MediaStreamKind" = 2
           ORDER BY ms."Language""#
    );
    let values = sqlx::query_scalar::<_, Option<String>>(crate::db::assert_sql_safe(&sql))
        .bind(item_id)
        .fetch_all(pool)
        .await
        .repository()?;
    let codes = values
        .into_iter()
        .flatten()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>();
    let mut languages = ersatzrs_core::language_codes::all_language_codes(codes)
        .into_iter()
        .filter_map(|code| ersatzrs_core::language_codes::english_name(&code))
        .filter(|name| !name.trim().is_empty())
        .collect::<Vec<_>>();
    languages.sort();
    languages.dedup();
    Ok(languages)
}

async fn load_metadata_language_names(
    pool: &Pool<crate::db::Db>,
    metadata_column: &str,
    metadata_id: EntityId,
    stream_languages: Vec<String>,
) -> Result<Vec<String>> {
    let sql = format!(
        r#"SELECT "Code" FROM "MetadataLanguage" WHERE "{metadata_column}" = ? ORDER BY "Id""#
    );
    let codes = sqlx::query_scalar::<_, String>(crate::db::assert_sql_safe(&sql))
        .bind(metadata_id)
        .fetch_all(pool)
        .await
        .repository()?;
    let explicit_names = ersatzrs_core::metadata_facets::normalize_language_codes(codes)
        .into_iter()
        .map(|code| ersatzrs_core::language_codes::english_name(&code).unwrap_or(code))
        .collect::<Vec<_>>();
    Ok(ersatzrs_core::metadata_facets::union_facet_values(
        &explicit_names,
        &stream_languages,
    ))
}

async fn load_media_facets(
    pool: &Pool<crate::db::Db>,
    metadata_column: &str,
    metadata_id: EntityId,
    content_rating: Option<String>,
    stream_languages: Vec<String>,
) -> Result<ersatzrs_core::metadata_facets::MediaFacetSet> {
    Ok(ersatzrs_core::metadata_facets::MediaFacetSet {
        content_ratings: split_content_ratings(content_rating),
        studios: load_strings(pool, r#""Studio""#, metadata_column, metadata_id).await?,
        music_labels: load_strings(pool, r#""MusicLabel""#, metadata_column, metadata_id).await?,
        languages: load_metadata_language_names(
            pool,
            metadata_column,
            metadata_id,
            stream_languages,
        )
        .await?,
        genres: load_strings(pool, r#""Genre""#, metadata_column, metadata_id).await?,
        tags: load_strings(pool, r#""Tag""#, metadata_column, metadata_id).await?,
    })
}

async fn load_show_languages(pool: &Pool<crate::db::Db>, show_id: EntityId) -> Result<Vec<String>> {
    let values = sqlx::query_scalar::<_, Option<String>>(
        r#"SELECT DISTINCT stream."Language"
           FROM "MediaStream" stream
           JOIN "MediaVersion" version ON version."Id" = stream."MediaVersionId"
           JOIN "Episode" episode ON episode."Id" = version."EpisodeId"
           JOIN "Season" season ON season."Id" = episode."SeasonId"
           WHERE stream."MediaStreamKind" = 2 AND season."ShowId" = ?"#,
    )
    .bind(show_id)
    .fetch_all(pool)
    .await
    .repository()?;
    let mut languages = ersatzrs_core::language_codes::all_language_codes(
        values
            .into_iter()
            .flatten()
            .filter(|value| !value.trim().is_empty()),
    )
    .into_iter()
    .filter_map(|code| ersatzrs_core::language_codes::english_name(&code))
    .filter(|name| !name.trim().is_empty())
    .collect::<Vec<_>>();
    languages.sort();
    languages.dedup();
    Ok(languages)
}

async fn load_artist_languages(
    pool: &Pool<crate::db::Db>,
    artist_id: EntityId,
) -> Result<Vec<String>> {
    let values = sqlx::query_scalar::<_, Option<String>>(
        r#"SELECT DISTINCT stream."Language"
           FROM "MediaStream" stream
           JOIN "MediaVersion" version ON version."Id" = stream."MediaVersionId"
           JOIN "MusicVideo" video ON video."Id" = version."MusicVideoId"
           WHERE stream."MediaStreamKind" = 2 AND video."ArtistId" = ?"#,
    )
    .bind(artist_id)
    .fetch_all(pool)
    .await
    .repository()?;
    let mut languages = ersatzrs_core::language_codes::all_language_codes(
        values
            .into_iter()
            .flatten()
            .filter(|value| !value.trim().is_empty()),
    )
    .into_iter()
    .filter_map(|code| ersatzrs_core::language_codes::english_name(&code))
    .filter(|name| !name.trim().is_empty())
    .collect::<Vec<_>>();
    languages.sort();
    languages.dedup();
    Ok(languages)
}

async fn load_artist_music_videos(
    pool: &Pool<crate::db::Db>,
    artist_id: EntityId,
    content_kind: i32,
) -> Result<Vec<ersatzrs_core::repositories::AdminArtistMusicVideo>> {
    let rows = sqlx::query(
        r#"SELECT video."Id", COALESCE(metadata."Title", '') AS "Title",
                  COALESCE(metadata."SortTitle", metadata."Title", '') AS "SortTitle",
                  metadata."Plot", metadata."Album", artwork."Path" AS "Thumbnail",
                  COALESCE(media."State", 0) AS "State", media."LibraryPathId",
                  file."Path"
           FROM "MusicVideo" video
           JOIN "MusicVideoMetadata" metadata ON metadata."MusicVideoId" = video."Id"
           JOIN "MediaItem" media ON media."Id" = video."Id"
           LEFT JOIN "Artwork" artwork
             ON artwork."MusicVideoMetadataId" = metadata."Id"
            AND artwork."ArtworkKind" = 1
           LEFT JOIN "MediaVersion" version
             ON version."MusicVideoId" = video."Id"
            AND version."Id" = (
                SELECT "Id" FROM "MediaVersion"
                WHERE "MusicVideoId" = video."Id" ORDER BY "Id" LIMIT 1
            )
           LEFT JOIN "MediaFile" file
             ON file."MediaVersionId" = version."Id"
            AND file."Id" = (
                SELECT "Id" FROM "MediaFile"
                WHERE "MediaVersionId" = version."Id" ORDER BY "Id" LIMIT 1
            )
           WHERE video."ArtistId" = ? AND COALESCE(media."ContentKind", 0) = ?
           ORDER BY metadata."SortTitle", metadata."Title" LIMIT 100"#,
    )
    .bind(artist_id)
    .bind(content_kind)
    .fetch_all(pool)
    .await
    .repository()?;

    let mut videos = Vec::with_capacity(rows.len());
    for row in rows {
        let file_path: Option<String> = row.try_get("Path").repository()?;
        let library_path_id: Option<EntityId> = row.try_get("LibraryPathId").repository()?;
        let local_path = match (file_path.as_deref(), library_path_id) {
            (Some(path), Some(library_path_id)) => Some(resolve_local_path(
                path,
                &load_path_replacements(pool, library_path_id).await?,
            )),
            (Some(path), None) => Some(path.to_owned()),
            (None, _) => None,
        };
        videos.push(ersatzrs_core::repositories::AdminArtistMusicVideo {
            id: row.try_get("Id").repository()?,
            title: row.try_get("Title").repository()?,
            sort_title: row.try_get("SortTitle").repository()?,
            plot: row.try_get("Plot").repository()?,
            album: row.try_get("Album").repository()?,
            thumbnail_path: row.try_get("Thumbnail").repository()?,
            media_item_state: row.try_get("State").repository()?,
            file_path,
            local_path,
        });
    }
    Ok(videos)
}

async fn load_actors(
    pool: &Pool<crate::db::Db>,
    metadata_column: &str,
    metadata_id: EntityId,
) -> Result<Vec<AdminActorCard>> {
    let sql = format!(
        r#"SELECT actor."Id", COALESCE(actor."Name", '') AS "Name",
                  actor."Role", artwork."Path" AS "Artwork"
           FROM "Actor" actor
           LEFT JOIN "Artwork" artwork ON artwork."Id" = actor."ArtworkId"
           WHERE actor."{metadata_column}" = ?
           ORDER BY actor."Order", actor."Id""#
    );
    let rows = sqlx::query(crate::db::assert_sql_safe(&sql))
        .bind(metadata_id)
        .fetch_all(pool)
        .await
        .repository()?;
    rows.iter()
        .map(|row| {
            Ok(AdminActorCard {
                id: row.try_get("Id").repository()?,
                name: row.try_get("Name").repository()?,
                role: row.try_get("Role").repository()?,
                artwork_path: row.try_get("Artwork").repository()?,
            })
        })
        .collect()
}

async fn load_path_replacements(
    pool: &Pool<crate::db::Db>,
    library_path_id: EntityId,
) -> Result<Vec<PathReplacement>> {
    let rows = sqlx::query(
        r#"SELECT ppr."PlexPath" AS "Source", ppr."LocalPath" AS "Local",
                  pms."Platform" AS "Platform", ppr."Id" AS "SortId"
           FROM "LibraryPath" lp
           JOIN "Library" l ON l."Id" = lp."LibraryId"
           JOIN "PlexMediaSource" pms ON pms."Id" = l."MediaSourceId"
           JOIN "PlexPathReplacement" ppr ON ppr."PlexMediaSourceId" = pms."Id"
           WHERE lp."Id" = ?
           UNION ALL
           SELECT jpr."JellyfinPath", jpr."LocalPath", jms."OperatingSystem", jpr."Id"
           FROM "LibraryPath" lp
           JOIN "Library" l ON l."Id" = lp."LibraryId"
           JOIN "JellyfinMediaSource" jms ON jms."Id" = l."MediaSourceId"
           JOIN "JellyfinPathReplacement" jpr ON jpr."JellyfinMediaSourceId" = jms."Id"
           WHERE lp."Id" = ?
           UNION ALL
           SELECT epr."EmbyPath", epr."LocalPath", ems."OperatingSystem", epr."Id"
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
            let windows = source_path.contains('\\')
                || source_path.contains(':')
                || platform.is_some_and(|value| {
                    value
                        .trim_start()
                        .to_ascii_lowercase()
                        .starts_with("windows")
                });
            replacements.push(PathReplacement {
                source_platform: if windows {
                    PathPlatform::Windows
                } else {
                    PathPlatform::Unix
                },
                source_path,
                replacement_path,
            });
        }
    }
    Ok(replacements)
}

fn resolve_local_path(path: &str, replacements: &[PathReplacement]) -> String {
    if replacements.is_empty() {
        return path.to_owned();
    }
    #[cfg(target_os = "windows")]
    let target = PathPlatform::Windows;
    #[cfg(not(target_os = "windows"))]
    let target = PathPlatform::Unix;
    ersatzrs_core::path_replacement::replace_path(replacements, path, target)
}

#[derive(Clone, Debug)]
struct AdminSubtitleInfo {
    stream_index: i64,
    subtitle_kind: i32,
    title: Option<String>,
    codec: Option<String>,
    language: Option<String>,
    is_default: bool,
    is_forced: bool,
    path: Option<String>,
    is_extracted: bool,
}

async fn load_admin_subtitles(
    pool: &Pool<crate::db::Db>,
    owner_column: &str,
    metadata_id: EntityId,
) -> Result<Vec<AdminSubtitleInfo>> {
    let sql = format!(
        r#"SELECT "StreamIndex", "SubtitleKind", "Title", "Codec", "Language",
                  "Default", "Forced", "Path", "IsExtracted"
           FROM "Subtitle" WHERE "{owner_column}" = ? ORDER BY "StreamIndex", "Id""#
    );
    let rows = sqlx::query(crate::db::assert_sql_safe(&sql))
        .bind(metadata_id)
        .fetch_all(pool)
        .await
        .repository()?;
    rows.iter()
        .map(|row| {
            Ok(AdminSubtitleInfo {
                stream_index: row.try_get("StreamIndex").repository()?,
                subtitle_kind: row.try_get("SubtitleKind").repository()?,
                title: row.try_get("Title").repository()?,
                codec: row.try_get("Codec").repository()?,
                language: row.try_get("Language").repository()?,
                is_default: row.try_get::<i64, _>("Default").repository()? != 0,
                is_forced: row.try_get::<i64, _>("Forced").repository()? != 0,
                path: row.try_get("Path").repository()?,
                is_extracted: row.try_get::<i64, _>("IsExtracted").repository()? != 0,
            })
        })
        .collect()
}

fn clean_string(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

fn is_image_subtitle(codec: Option<&str>) -> bool {
    matches!(
        codec,
        Some("hdmv_pgs_subtitle" | "dvd_subtitle" | "dvdsub" | "vobsub" | "pgssub" | "pgs")
    )
}

async fn load_admin_media_streams(
    pool: &Pool<crate::db::Db>,
    media_version_id: EntityId,
    subtitles: &[AdminSubtitleInfo],
    is_local_library: bool,
) -> Result<Vec<AdminMediaInfoStream>> {
    let rows = sqlx::query(
        r#"SELECT "Index", "MediaStreamKind", "Title", "Codec", "Profile", "Language",
                  "Channels", "Default", "Forced", "AttachedPic", "PixelFormat",
                  "ColorRange", "ColorSpace", "ColorTransfer", "ColorPrimaries",
                  "BitsPerRawSample", "MimeType", "FileName"
           FROM "MediaStream" WHERE "MediaVersionId" = ? ORDER BY "Index", "Id""#,
    )
    .bind(media_version_id)
    .fetch_all(pool)
    .await
    .repository()?;
    let mut streams = Vec::with_capacity(rows.len() + subtitles.len());
    for row in rows {
        let index = row.try_get("Index").repository()?;
        let kind = row.try_get("MediaStreamKind").repository()?;
        let channels: i64 = row.try_get("Channels").repository()?;
        let bits_per_raw_sample: i64 = row.try_get("BitsPerRawSample").repository()?;
        let mut stream = AdminMediaInfoStream {
            index: Some(index),
            kind,
            external_subtitle: false,
            title: row.try_get("Title").repository()?,
            codec: row.try_get("Codec").repository()?,
            profile: row.try_get("Profile").repository()?,
            language: row.try_get("Language").repository()?,
            channels: (channels > 0).then_some(channels),
            is_default: (row.try_get::<i64, _>("Default").repository()? != 0).then_some(true),
            is_forced: (row.try_get::<i64, _>("Forced").repository()? != 0).then_some(true),
            attached_pic: (row.try_get::<i64, _>("AttachedPic").repository()? != 0).then_some(true),
            pixel_format: row.try_get("PixelFormat").repository()?,
            color_range: row.try_get("ColorRange").repository()?,
            color_space: row.try_get("ColorSpace").repository()?,
            color_transfer: row.try_get("ColorTransfer").repository()?,
            color_primaries: row.try_get("ColorPrimaries").repository()?,
            bits_per_raw_sample: (bits_per_raw_sample > 0).then_some(bits_per_raw_sample),
            mime_type: row.try_get("MimeType").repository()?,
            file_name: row.try_get("FileName").repository()?,
            is_extracted: None,
        };
        if kind == 3 {
            if let Some(subtitle) = subtitles
                .iter()
                .find(|subtitle| subtitle.stream_index == index)
            {
                if !is_image_subtitle(subtitle.codec.as_deref()) {
                    stream.is_extracted = Some(subtitle.is_extracted);
                }
                stream.file_name = clean_string(subtitle.path.clone());
            }
        }
        streams.push(stream);
    }
    if is_local_library {
        streams.extend(
            subtitles
                .iter()
                .filter(|subtitle| subtitle.subtitle_kind == 1)
                .map(|subtitle| AdminMediaInfoStream {
                    index: None,
                    kind: 5,
                    external_subtitle: true,
                    title: clean_string(subtitle.title.clone()),
                    codec: clean_string(subtitle.codec.clone()),
                    profile: None,
                    language: clean_string(subtitle.language.clone()),
                    channels: None,
                    is_default: subtitle.is_default.then_some(true),
                    is_forced: subtitle.is_forced.then_some(true),
                    attached_pic: None,
                    pixel_format: None,
                    color_range: None,
                    color_space: None,
                    color_transfer: None,
                    color_primaries: None,
                    bits_per_raw_sample: None,
                    mime_type: None,
                    file_name: subtitle.path.as_ref().and_then(|path| {
                        std::path::Path::new(path)
                            .file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                    }),
                    is_extracted: None,
                }),
        );
    }
    Ok(streams)
}

async fn load_admin_media_chapters(
    pool: &Pool<crate::db::Db>,
    media_version_id: EntityId,
) -> Result<Vec<AdminMediaInfoChapter>> {
    let rows = sqlx::query(
        r#"SELECT COALESCE("Title", '') AS "Title", "StartTime", "EndTime"
           FROM "MediaChapter" WHERE "MediaVersionId" = ?
           ORDER BY "StartTime", "ChapterId""#,
    )
    .bind(media_version_id)
    .fetch_all(pool)
    .await
    .repository()?;
    rows.iter()
        .map(|row| {
            Ok(AdminMediaInfoChapter {
                title: row.try_get("Title").repository()?,
                start_time: row.try_get("StartTime").repository()?,
                end_time: row.try_get("EndTime").repository()?,
            })
        })
        .collect()
}

async fn enrich_admin_media_info(
    pool: &Pool<crate::db::Db>,
    mut info: AdminMediaItemInfo,
    metadata_column: &str,
    metadata_id: EntityId,
    media_version_id: Option<EntityId>,
    is_local_library: bool,
) -> Result<AdminMediaItemInfo> {
    let subtitles = load_admin_subtitles(pool, metadata_column, metadata_id).await?;
    if let Some(media_version_id) = media_version_id {
        info.streams =
            load_admin_media_streams(pool, media_version_id, &subtitles, is_local_library).await?;
        info.chapters = load_admin_media_chapters(pool, media_version_id).await?;
    }
    Ok(info)
}

async fn load_extras(
    pool: &Pool<crate::db::Db>,
    parent_id: EntityId,
    content_kind: i32,
    movie: bool,
) -> Result<Vec<AdminMediaCard>> {
    let sql = if movie {
        r#"SELECT item."Id", COALESCE(metadata."Title", '') AS "Title",
                  COALESCE(metadata."SortTitle", metadata."Title", '') AS "SortTitle",
                  metadata."Year", artwork."Path" AS "Artwork",
                  COALESCE(media."ContentKind", 0) AS "ContentKind",
                  media."SubKind", parent."Title" AS "ParentTitle"
           FROM "Movie" item
           JOIN "MovieMetadata" metadata ON metadata."MovieId" = item."Id"
           JOIN "MediaItem" media ON media."Id" = item."Id"
           LEFT JOIN "MovieMetadata" parent ON parent."MovieId" = media."ParentMediaItemId"
           LEFT JOIN "Artwork" artwork
             ON artwork."MovieMetadataId" = metadata."Id" AND artwork."ArtworkKind" = 0
           WHERE media."ParentMediaItemId" = ? AND COALESCE(media."ContentKind", 0) = ?
           ORDER BY COALESCE(media."SubKind", ''), metadata."SortTitle", metadata."Title""#
    } else {
        r#"SELECT item."Id", COALESCE(metadata."Title", '') AS "Title",
                  COALESCE(metadata."SortTitle", metadata."Title", '') AS "SortTitle",
                  metadata."Year", COALESCE(thumb."Path", poster."Path") AS "Artwork",
                  COALESCE(media."ContentKind", 0) AS "ContentKind",
                  media."SubKind", NULL AS "ParentTitle"
           FROM "OtherVideo" item
           JOIN "OtherVideoMetadata" metadata ON metadata."OtherVideoId" = item."Id"
           JOIN "MediaItem" media ON media."Id" = item."Id"
           LEFT JOIN "Artwork" thumb
             ON thumb."OtherVideoMetadataId" = metadata."Id" AND thumb."ArtworkKind" = 1
           LEFT JOIN "Artwork" poster
             ON poster."OtherVideoMetadataId" = metadata."Id" AND poster."ArtworkKind" = 0
           WHERE media."ParentMediaItemId" = ? AND COALESCE(media."ContentKind", 0) = ?
           ORDER BY COALESCE(media."SubKind", ''), metadata."SortTitle", metadata."Title""#
    };
    let rows = sqlx::query(sql)
        .bind(parent_id)
        .bind(content_kind)
        .fetch_all(pool)
        .await
        .repository()?;
    rows.iter()
        .map(|row| {
            Ok(AdminMediaCard {
                id: row.try_get("Id").repository()?,
                title: row.try_get("Title").repository()?,
                sort_title: row.try_get("SortTitle").repository()?,
                subtitle: row
                    .try_get::<Option<i64>, _>("Year")
                    .repository()?
                    .map(|year| year.to_string()),
                artwork_path: row.try_get("Artwork").repository()?,
                season_number: None,
                content_kind: decode_content_kind(row.try_get("ContentKind").repository()?)?,
                sub_kind: row.try_get("SubKind").repository()?,
                parent_title: row.try_get("ParentTitle").repository()?,
            })
        })
        .collect()
}

#[allow(clippy::too_many_lines)]
#[async_trait]
impl AdminMediaRepository for SqliteAdminMediaRepository {
    #[allow(clippy::too_many_lines)]
    async fn get_movie_detail_admin(&self, id: EntityId) -> Result<Option<AdminMovieDetail>> {
        let Some(row) = sqlx::query(
            r#"SELECT movie."Id", COALESCE(metadata."Title", '') AS "Title",
                      metadata."Year", metadata."Plot", metadata."Tagline",
                      metadata."ContentRating", metadata."ReleaseDate",
                      COALESCE(media."State", 0) AS "State",
                      media."LibraryPathId", metadata."Id" AS "MetadataId"
               FROM "Movie" movie
               JOIN "MovieMetadata" metadata ON metadata."MovieId" = movie."Id"
               LEFT JOIN "MediaItem" media ON media."Id" = movie."Id"
               WHERE movie."Id" = ? LIMIT 1"#,
        )
        .bind(id)
        .fetch_optional(&self.0)
        .await
        .repository()?
        else {
            return Ok(None);
        };
        let metadata_id = row.try_get("MetadataId").repository()?;
        let file_path = sqlx::query_scalar::<_, Option<String>>(
            r#"SELECT file."Path"
               FROM "MediaFile" file
               JOIN "MediaVersion" version ON version."Id" = file."MediaVersionId"
               WHERE version."MovieId" = ? ORDER BY version."Id" LIMIT 1"#,
        )
        .bind(id)
        .fetch_optional(&self.0)
        .await
        .repository()?
        .flatten();
        let library_path_id = row.try_get("LibraryPathId").repository()?;
        let local_path = match (file_path.as_deref(), library_path_id) {
            (Some(path), Some(library_path_id)) => Some(resolve_local_path(
                path,
                &load_path_replacements(&self.0, library_path_id).await?,
            )),
            (Some(path), None) => Some(path.to_owned()),
            (None, _) => None,
        };
        let audio_rows = sqlx::query(
            r#"SELECT version."Id" AS "MediaVersionId", stream."Index" AS "StreamIndex",
                      COALESCE(NULLIF(stream."Title", ''), 'Default Audio') AS "Title",
                      stream."Language", stream."Codec", stream."Channels", stream."Default"
               FROM "MediaVersion" version
               JOIN "MediaStream" stream ON stream."MediaVersionId" = version."Id"
               WHERE version."MovieId" = ? AND stream."MediaStreamKind" = 2
               ORDER BY version."Id", stream."Index""#,
        )
        .bind(id)
        .fetch_all(&self.0)
        .await
        .repository()?;
        let audio_tracks = audio_rows
            .iter()
            .map(|audio| {
                let title: String = audio.try_get("Title").repository()?;
                Ok(AdminAudioTrack {
                    media_version_id: audio.try_get("MediaVersionId").repository()?,
                    stream_index: audio.try_get("StreamIndex").repository()?,
                    is_commentary: title.to_ascii_lowercase().contains("commentary"),
                    title,
                    language: audio.try_get("Language").repository()?,
                    codec: audio.try_get("Codec").repository()?,
                    channels: audio.try_get("Channels").repository()?,
                    is_default: audio.try_get::<i64, _>("Default").repository()? != 0,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let subtitle_rows = sqlx::query(
            r#"SELECT DISTINCT subtitle."Id", subtitle."StreamIndex",
                      COALESCE(NULLIF(subtitle."Title", ''), NULLIF(stream."Title", ''),
                               'Commentary') AS "Title",
                      subtitle."Language", subtitle."Codec"
               FROM "Subtitle" subtitle
               JOIN "MovieMetadata" metadata ON metadata."Id" = subtitle."MovieMetadataId"
               LEFT JOIN "MediaVersion" version ON version."MovieId" = metadata."MovieId"
               LEFT JOIN "MediaStream" stream
                 ON stream."MediaVersionId" = version."Id"
                AND stream."Index" = subtitle."StreamIndex"
                AND stream."MediaStreamKind" = 3
               WHERE metadata."MovieId" = ?
                 AND NOT (subtitle."SubtitleKind" = 0 AND subtitle."IsExtracted" = 0
                   AND COALESCE(subtitle."Codec", '') NOT IN
                     ('dvd_subtitle', 'hdmv_pgs_subtitle', 'xsub'))
                 AND (LOWER(COALESCE(subtitle."Title", '')) LIKE '%commentary%'
                   OR LOWER(COALESCE(stream."Title", '')) LIKE '%commentary%')
               ORDER BY subtitle."StreamIndex", subtitle."Id""#,
        )
        .bind(id)
        .fetch_all(&self.0)
        .await
        .repository()?;
        let commentary_subtitles = subtitle_rows
            .iter()
            .map(|subtitle| {
                Ok(AdminCommentarySubtitle {
                    id: subtitle.try_get("Id").repository()?,
                    stream_index: subtitle.try_get("StreamIndex").repository()?,
                    title: subtitle.try_get("Title").repository()?,
                    language: subtitle.try_get("Language").repository()?,
                    codec: subtitle.try_get("Codec").repository()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let playback_sources = load_movie_playback_sources(&self.0, id, metadata_id).await?;
        Ok(Some(AdminMovieDetail {
            id: row.try_get("Id").repository()?,
            title: row.try_get("Title").repository()?,
            year: row.try_get("Year").repository()?,
            plot: row.try_get("Plot").repository()?,
            tagline: row.try_get("Tagline").repository()?,
            content_ratings: split_content_ratings(row.try_get("ContentRating").repository()?),
            release_date: row.try_get("ReleaseDate").repository()?,
            poster_path: load_artwork_path(&self.0, "MovieMetadataId", metadata_id, 0).await?,
            fanart_path: load_artwork_path(&self.0, "MovieMetadataId", metadata_id, 3).await?,
            file_path,
            local_path,
            media_item_state: row.try_get("State").repository()?,
            genres: load_strings(&self.0, r#""Genre""#, "MovieMetadataId", metadata_id).await?,
            tags: load_strings(&self.0, r#""Tag""#, "MovieMetadataId", metadata_id).await?,
            languages: load_audio_languages(&self.0, "MovieId", id).await?,
            studios: load_strings(&self.0, r#""Studio""#, "MovieMetadataId", metadata_id).await?,
            facets: load_media_facets(
                &self.0,
                "MovieMetadataId",
                metadata_id,
                row.try_get("ContentRating").repository()?,
                load_audio_languages(&self.0, "MovieId", id).await?,
            )
            .await?,
            directors: load_strings(&self.0, r#""Director""#, "MovieMetadataId", metadata_id)
                .await?,
            writers: load_strings(&self.0, r#""Writer""#, "MovieMetadataId", metadata_id).await?,
            actors: load_actors(&self.0, "MovieMetadataId", metadata_id).await?,
            extras: load_extras(&self.0, id, 1, false).await?,
            alternate_versions: load_extras(&self.0, id, 2, true).await?,
            audio_tracks,
            commentary_subtitles,
            playback_sources,
        }))
    }

    async fn get_show_detail_admin(&self, id: EntityId) -> Result<Option<AdminShowDetail>> {
        let Some(row) = sqlx::query(
            r#"SELECT show."Id", COALESCE(path."LibraryId", 0) AS "LibraryId",
                      CASE
                        WHEN plex."Id" IS NOT NULL THEN 'Plex'
                        WHEN jellyfin."Id" IS NOT NULL THEN 'Jellyfin'
                        WHEN emby."Id" IS NOT NULL THEN 'Emby'
                        ELSE 'Local'
                      END AS "MediaSourceKind",
                      COALESCE(metadata."Title", '') AS "Title", metadata."Year",
                      metadata."Plot", metadata."ContentRating",
                      COALESCE(media."State", 0) AS "State",
                      metadata."Id" AS "MetadataId"
               FROM "Show" show
               JOIN "ShowMetadata" metadata ON metadata."ShowId" = show."Id"
               LEFT JOIN "MediaItem" media ON media."Id" = show."Id"
               LEFT JOIN "LibraryPath" path ON path."Id" = media."LibraryPathId"
               LEFT JOIN "PlexLibrary" plex ON plex."Id" = path."LibraryId"
               LEFT JOIN "JellyfinLibrary" jellyfin ON jellyfin."Id" = path."LibraryId"
               LEFT JOIN "EmbyLibrary" emby ON emby."Id" = path."LibraryId"
               WHERE show."Id" = ? LIMIT 1"#,
        )
        .bind(id)
        .fetch_optional(&self.0)
        .await
        .repository()?
        else {
            return Ok(None);
        };
        let metadata_id = row.try_get("MetadataId").repository()?;
        let tag_rows = sqlx::query(
            r#"SELECT COALESCE("Name", '') AS "Name",
                      COALESCE("ExternalTypeId", '') AS "ExternalTypeId"
               FROM "Tag" WHERE "ShowMetadataId" = ? ORDER BY "Name""#,
        )
        .bind(metadata_id)
        .fetch_all(&self.0)
        .await
        .repository()?;
        let mut tags = Vec::new();
        let mut networks = Vec::new();
        for tag in tag_rows {
            let name: String = tag.try_get("Name").repository()?;
            let external: String = tag.try_get("ExternalTypeId").repository()?;
            if name.trim().is_empty() {
                continue;
            }
            if external.trim().is_empty() {
                tags.push(name);
            } else if external == "319" {
                networks.push(name);
            }
        }
        let season_rows = sqlx::query(
            r#"SELECT season."Id", season."SeasonNumber",
                      (
                          SELECT artwork."Path"
                          FROM "Artwork" artwork
                          JOIN "SeasonMetadata" artwork_metadata
                            ON artwork_metadata."Id" = artwork."SeasonMetadataId"
                          WHERE artwork_metadata."SeasonId" = season."Id"
                            AND artwork."ArtworkKind" = 0
                            AND NULLIF(TRIM(artwork."Path"), '') IS NOT NULL
                          ORDER BY artwork."Id"
                          LIMIT 1
                      ) AS "Poster"
               FROM "Season" season
               WHERE season."ShowId" = ?
               ORDER BY season."SeasonNumber", season."Id""#,
        )
        .bind(id)
        .fetch_all(&self.0)
        .await
        .repository()?;
        let seasons = season_rows
            .iter()
            .map(|season| {
                Ok(ersatzrs_core::repositories::AdminSeasonCard {
                    id: season.try_get("Id").repository()?,
                    season_number: season.try_get("SeasonNumber").repository()?,
                    poster_path: season.try_get("Poster").repository()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Some(AdminShowDetail {
            id: row.try_get("Id").repository()?,
            library_id: row.try_get("LibraryId").repository()?,
            media_source_kind: row.try_get("MediaSourceKind").repository()?,
            title: row.try_get("Title").repository()?,
            year: row.try_get("Year").repository()?,
            plot: row.try_get("Plot").repository()?,
            content_ratings: split_content_ratings(row.try_get("ContentRating").repository()?),
            poster_path: load_artwork_path(&self.0, "ShowMetadataId", metadata_id, 0).await?,
            fanart_path: load_artwork_path(&self.0, "ShowMetadataId", metadata_id, 3).await?,
            media_item_state: row.try_get("State").repository()?,
            genres: load_strings(&self.0, r#""Genre""#, "ShowMetadataId", metadata_id).await?,
            tags,
            languages: load_show_languages(&self.0, id).await?,
            studios: load_strings(&self.0, r#""Studio""#, "ShowMetadataId", metadata_id).await?,
            facets: load_media_facets(
                &self.0,
                "ShowMetadataId",
                metadata_id,
                row.try_get("ContentRating").repository()?,
                load_show_languages(&self.0, id).await?,
            )
            .await?,
            networks,
            actors: load_actors(&self.0, "ShowMetadataId", metadata_id).await?,
            seasons,
            extras: load_extras(&self.0, id, 1, false).await?,
        }))
    }

    async fn get_artist_detail_admin(&self, id: EntityId) -> Result<Option<AdminArtistDetail>> {
        let Some(row) = sqlx::query(
            r#"SELECT artist."Id", metadata."Id" AS "MetadataId",
                      COALESCE(metadata."Title", '') AS "Title",
                      metadata."Disambiguation", metadata."Biography",
                      thumbnail."Path" AS "Thumbnail", fanart."Path" AS "FanArt"
               FROM "Artist" artist
               JOIN "ArtistMetadata" metadata ON metadata."ArtistId" = artist."Id"
               LEFT JOIN "Artwork" thumbnail
                 ON thumbnail."ArtistMetadataId" = metadata."Id"
                AND thumbnail."ArtworkKind" = 1
               LEFT JOIN "Artwork" fanart
                 ON fanart."ArtistMetadataId" = metadata."Id"
                AND fanart."ArtworkKind" = 3
               WHERE artist."Id" = ? ORDER BY metadata."Id" LIMIT 1"#,
        )
        .bind(id)
        .fetch_optional(&self.0)
        .await
        .repository()?
        else {
            return Ok(None);
        };
        let metadata_id = row.try_get("MetadataId").repository()?;
        Ok(Some(AdminArtistDetail {
            id: row.try_get("Id").repository()?,
            title: row.try_get("Title").repository()?,
            disambiguation: row
                .try_get::<Option<String>, _>("Disambiguation")
                .repository()?
                .filter(|value| !value.trim().is_empty()),
            biography: row
                .try_get::<Option<String>, _>("Biography")
                .repository()?
                .filter(|value| !value.trim().is_empty()),
            thumbnail_path: row.try_get("Thumbnail").repository()?,
            fanart_path: row.try_get("FanArt").repository()?,
            genres: load_strings(&self.0, r#""Genre""#, "ArtistMetadataId", metadata_id).await?,
            styles: load_strings(&self.0, r#""Style""#, "ArtistMetadataId", metadata_id).await?,
            moods: load_strings(&self.0, r#""Mood""#, "ArtistMetadataId", metadata_id).await?,
            languages: load_artist_languages(&self.0, id).await?,
            music_videos: load_artist_music_videos(&self.0, id, 0).await?,
            versions: load_artist_music_videos(&self.0, id, 2).await?,
        }))
    }

    async fn get_other_video_detail_admin(
        &self,
        id: EntityId,
    ) -> Result<Option<AdminOtherVideoDetail>> {
        let Some(row) = sqlx::query(
            r#"SELECT video."Id", COALESCE(metadata."Title", '') AS "Title",
                      metadata."OriginalTitle", metadata."Year", metadata."Plot",
                      metadata."Tagline", metadata."ContentRating", metadata."ReleaseDate",
                      COALESCE(media."State", 0) AS "State", media."LibraryPathId",
                      metadata."Id" AS "MetadataId"
               FROM "OtherVideo" video
               JOIN "OtherVideoMetadata" metadata ON metadata."OtherVideoId" = video."Id"
               LEFT JOIN "MediaItem" media ON media."Id" = video."Id"
               WHERE video."Id" = ? LIMIT 1"#,
        )
        .bind(id)
        .fetch_optional(&self.0)
        .await
        .repository()?
        else {
            return Ok(None);
        };
        let metadata_id = row.try_get("MetadataId").repository()?;
        let file_path = sqlx::query_scalar::<_, Option<String>>(
            r#"SELECT file."Path"
               FROM "MediaFile" file
               JOIN "MediaVersion" version ON version."Id" = file."MediaVersionId"
               WHERE version."OtherVideoId" = ? ORDER BY version."Id" LIMIT 1"#,
        )
        .bind(id)
        .fetch_optional(&self.0)
        .await
        .repository()?
        .flatten();
        let library_path_id = row.try_get("LibraryPathId").repository()?;
        let local_path = match (file_path.as_deref(), library_path_id) {
            (Some(path), Some(library_path_id)) => Some(resolve_local_path(
                path,
                &load_path_replacements(&self.0, library_path_id).await?,
            )),
            (Some(path), None) => Some(path.to_owned()),
            (None, _) => None,
        };
        Ok(Some(AdminOtherVideoDetail {
            id: row.try_get("Id").repository()?,
            title: row.try_get("Title").repository()?,
            original_title: row.try_get("OriginalTitle").repository()?,
            year: row.try_get("Year").repository()?,
            plot: row.try_get("Plot").repository()?,
            tagline: row.try_get("Tagline").repository()?,
            content_ratings: split_content_ratings(row.try_get("ContentRating").repository()?),
            release_date: row.try_get("ReleaseDate").repository()?,
            thumbnail_path: load_artwork_path(&self.0, "OtherVideoMetadataId", metadata_id, 1)
                .await?,
            poster_path: load_artwork_path(&self.0, "OtherVideoMetadataId", metadata_id, 0).await?,
            fanart_path: load_artwork_path(&self.0, "OtherVideoMetadataId", metadata_id, 3).await?,
            file_path,
            local_path,
            media_item_state: row.try_get("State").repository()?,
            genres: load_strings(&self.0, r#""Genre""#, "OtherVideoMetadataId", metadata_id)
                .await?,
            tags: load_strings(&self.0, r#""Tag""#, "OtherVideoMetadataId", metadata_id).await?,
            languages: load_audio_languages(&self.0, "OtherVideoId", id).await?,
            studios: load_strings(&self.0, r#""Studio""#, "OtherVideoMetadataId", metadata_id)
                .await?,
            facets: load_media_facets(
                &self.0,
                "OtherVideoMetadataId",
                metadata_id,
                row.try_get("ContentRating").repository()?,
                load_audio_languages(&self.0, "OtherVideoId", id).await?,
            )
            .await?,
            directors: load_strings(
                &self.0,
                r#""Director""#,
                "OtherVideoMetadataId",
                metadata_id,
            )
            .await?,
            writers: load_strings(&self.0, r#""Writer""#, "OtherVideoMetadataId", metadata_id)
                .await?,
            actors: load_actors(&self.0, "OtherVideoMetadataId", metadata_id).await?,
        }))
    }

    #[allow(clippy::too_many_lines)]
    async fn get_faceted_media_detail_admin(
        &self,
        id: EntityId,
        kind: AdminFacetedMediaKind,
    ) -> Result<Option<AdminFacetedMediaDetail>> {
        let (item_table, metadata_table, item_fk, version_fk, metadata_column, plot_expression) =
            match kind {
                AdminFacetedMediaKind::MusicVideo => (
                    "MusicVideo",
                    "MusicVideoMetadata",
                    "MusicVideoId",
                    "MusicVideoId",
                    "MusicVideoMetadataId",
                    r#"metadata."Plot""#,
                ),
                AdminFacetedMediaKind::Song => (
                    "Song",
                    "SongMetadata",
                    "SongId",
                    "SongId",
                    "SongMetadataId",
                    "NULL",
                ),
                AdminFacetedMediaKind::Image => (
                    "Image",
                    "ImageMetadata",
                    "ImageId",
                    "ImageId",
                    "ImageMetadataId",
                    "NULL",
                ),
            };
        let sql = format!(
            r#"SELECT item."Id", COALESCE(metadata."Title", '') AS "Title",
                      metadata."OriginalTitle", metadata."Year", {plot_expression} AS "Plot",
                      metadata."ReleaseDate", metadata."ContentRating",
                      COALESCE(media."State", 0) AS "State", media."LibraryPathId",
                      metadata."Id" AS "MetadataId"
               FROM "{item_table}" item
               JOIN "{metadata_table}" metadata ON metadata."{item_fk}" = item."Id"
               LEFT JOIN "MediaItem" media ON media."Id" = item."Id"
               WHERE item."Id" = ? ORDER BY metadata."Id" LIMIT 1"#
        );
        let Some(row) = sqlx::query(crate::db::assert_sql_safe(&sql))
            .bind(id)
            .fetch_optional(&self.0)
            .await
            .repository()?
        else {
            return Ok(None);
        };
        let metadata_id: EntityId = row.try_get("MetadataId").repository()?;
        let file_sql = format!(
            r#"SELECT file."Path"
               FROM "MediaFile" file
               JOIN "MediaVersion" version ON version."Id" = file."MediaVersionId"
               WHERE version."{version_fk}" = ? ORDER BY version."Id", file."Id" LIMIT 1"#
        );
        let file_path =
            sqlx::query_scalar::<_, Option<String>>(crate::db::assert_sql_safe(&file_sql))
                .bind(id)
                .fetch_optional(&self.0)
                .await
                .repository()?
                .flatten();
        let library_path_id = row.try_get("LibraryPathId").repository()?;
        let local_path = match (file_path.as_deref(), library_path_id) {
            (Some(path), Some(library_path_id)) => Some(resolve_local_path(
                path,
                &load_path_replacements(&self.0, library_path_id).await?,
            )),
            (Some(path), None) => Some(path.to_owned()),
            (None, _) => None,
        };
        let stream_languages = match kind {
            AdminFacetedMediaKind::MusicVideo => {
                load_audio_languages(&self.0, "MusicVideoId", id).await?
            }
            AdminFacetedMediaKind::Song => load_audio_languages(&self.0, "SongId", id).await?,
            AdminFacetedMediaKind::Image => Vec::new(),
        };
        let artwork_path = load_artwork_path(&self.0, metadata_column, metadata_id, 1)
            .await?
            .or(load_artwork_path(&self.0, metadata_column, metadata_id, 0).await?);
        Ok(Some(AdminFacetedMediaDetail {
            id: row.try_get("Id").repository()?,
            title: row.try_get("Title").repository()?,
            original_title: row.try_get("OriginalTitle").repository()?,
            year: row.try_get("Year").repository()?,
            plot: row.try_get("Plot").repository()?,
            release_date: row.try_get("ReleaseDate").repository()?,
            artwork_path,
            fanart_path: load_artwork_path(&self.0, metadata_column, metadata_id, 3).await?,
            file_path,
            local_path,
            media_item_state: row.try_get("State").repository()?,
            facets: load_media_facets(
                &self.0,
                metadata_column,
                metadata_id,
                row.try_get("ContentRating").repository()?,
                stream_languages,
            )
            .await?,
        }))
    }

    async fn list_extras_admin(&self, parent_id: EntityId) -> Result<Vec<AdminMediaCard>> {
        load_extras(&self.0, parent_id, 1, false).await
    }

    #[allow(clippy::too_many_lines)]
    async fn get_media_item_info_admin(&self, id: EntityId) -> Result<Option<AdminMediaItemInfo>> {
        let movie = sqlx::query(
            r#"SELECT movie."Id", COALESCE(metadata."Title", '') AS "Title",
                      metadata."Id" AS "MetadataId", COALESCE(media."State", 0) AS "State",
                      COALESCE(library."Name", '') AS "LibraryName",
                      CASE
                        WHEN local."Id" IS NOT NULL THEN 'LocalLibrary'
                        WHEN plex."Id" IS NOT NULL THEN 'PlexLibrary'
                        WHEN jellyfin."Id" IS NOT NULL THEN 'JellyfinLibrary'
                        WHEN emby."Id" IS NOT NULL THEN 'EmbyLibrary'
                        ELSE 'Library'
                      END AS "LibraryKind",
                      COALESCE(plex_source."ServerName", jellyfin_source."ServerName",
                               emby_source."ServerName") AS "ServerName",
                      version."Id" AS "MediaVersionId", version."Duration",
                      version."SampleAspectRatio", version."DisplayAspectRatio",
                      version."RFrameRate", COALESCE(version."VideoScanKind", 0) AS "VideoScanKind",
                      version."InterlacedRatio", COALESCE(version."Width", 0) AS "Width",
                      COALESCE(version."Height", 0) AS "Height"
               FROM "Movie" movie
               JOIN "MovieMetadata" metadata ON metadata."MovieId" = movie."Id"
               LEFT JOIN "MediaItem" media ON media."Id" = movie."Id"
               LEFT JOIN "LibraryPath" path ON path."Id" = media."LibraryPathId"
               LEFT JOIN "Library" library ON library."Id" = path."LibraryId"
               LEFT JOIN "LocalLibrary" local ON local."Id" = library."Id"
               LEFT JOIN "PlexLibrary" plex ON plex."Id" = library."Id"
               LEFT JOIN "JellyfinLibrary" jellyfin ON jellyfin."Id" = library."Id"
               LEFT JOIN "EmbyLibrary" emby ON emby."Id" = library."Id"
               LEFT JOIN "MediaSource" source ON source."Id" = library."MediaSourceId"
               LEFT JOIN "PlexMediaSource" plex_source ON plex_source."Id" = source."Id"
               LEFT JOIN "JellyfinMediaSource" jellyfin_source
                 ON jellyfin_source."Id" = source."Id"
               LEFT JOIN "EmbyMediaSource" emby_source ON emby_source."Id" = source."Id"
               LEFT JOIN "MediaVersion" version ON version."MovieId" = movie."Id"
               WHERE media."Id" = ? ORDER BY version."Id" LIMIT 1"#,
        )
        .bind(id)
        .fetch_optional(&self.0)
        .await
        .repository()?;
        if let Some(row) = movie {
            let metadata_id = row.try_get("MetadataId").repository()?;
            let media_version_id = row.try_get("MediaVersionId").repository()?;
            let library_kind: String = row.try_get("LibraryKind").repository()?;
            let info = AdminMediaItemInfo {
                id: row.try_get("Id").repository()?,
                title: row.try_get("Title").repository()?,
                kind: AdminMediaInfoKind::Movie,
                library_kind: library_kind.clone(),
                server_name: row.try_get("ServerName").repository()?,
                library_name: row.try_get("LibraryName").repository()?,
                state: row.try_get("State").repository()?,
                duration: row
                    .try_get::<Option<String>, _>("Duration")
                    .repository()?
                    .unwrap_or_else(|| "00:00:00".to_owned()),
                sample_aspect_ratio: row.try_get("SampleAspectRatio").repository()?,
                display_aspect_ratio: row.try_get("DisplayAspectRatio").repository()?,
                r_frame_rate: row.try_get("RFrameRate").repository()?,
                video_scan_kind: row.try_get("VideoScanKind").repository()?,
                interlaced_ratio: row.try_get("InterlacedRatio").repository()?,
                width: row.try_get("Width").repository()?,
                height: row.try_get("Height").repository()?,
                streams: Vec::new(),
                chapters: Vec::new(),
            };
            return enrich_admin_media_info(
                &self.0,
                info,
                "MovieMetadataId",
                metadata_id,
                media_version_id,
                library_kind == "LocalLibrary",
            )
            .await
            .map(Some);
        }

        let other_video = sqlx::query(
            r#"SELECT video."Id", COALESCE(metadata."Title", '') AS "Title",
                      metadata."Id" AS "MetadataId", COALESCE(media."State", 0) AS "State",
                      COALESCE(library."Name", '') AS "LibraryName",
                      CASE
                        WHEN local."Id" IS NOT NULL THEN 'LocalLibrary'
                        WHEN plex."Id" IS NOT NULL THEN 'PlexLibrary'
                        WHEN jellyfin."Id" IS NOT NULL THEN 'JellyfinLibrary'
                        WHEN emby."Id" IS NOT NULL THEN 'EmbyLibrary'
                        ELSE 'Library'
                      END AS "LibraryKind",
                      COALESCE(plex_source."ServerName", jellyfin_source."ServerName",
                               emby_source."ServerName") AS "ServerName",
                      version."Id" AS "MediaVersionId", version."Duration",
                      version."SampleAspectRatio", version."DisplayAspectRatio",
                      version."RFrameRate", COALESCE(version."VideoScanKind", 0) AS "VideoScanKind",
                      version."InterlacedRatio", COALESCE(version."Width", 0) AS "Width",
                      COALESCE(version."Height", 0) AS "Height"
               FROM "OtherVideo" video
               JOIN "OtherVideoMetadata" metadata ON metadata."OtherVideoId" = video."Id"
               LEFT JOIN "MediaItem" media ON media."Id" = video."Id"
               LEFT JOIN "LibraryPath" path ON path."Id" = media."LibraryPathId"
               LEFT JOIN "Library" library ON library."Id" = path."LibraryId"
               LEFT JOIN "LocalLibrary" local ON local."Id" = library."Id"
               LEFT JOIN "PlexLibrary" plex ON plex."Id" = library."Id"
               LEFT JOIN "JellyfinLibrary" jellyfin ON jellyfin."Id" = library."Id"
               LEFT JOIN "EmbyLibrary" emby ON emby."Id" = library."Id"
               LEFT JOIN "MediaSource" source ON source."Id" = library."MediaSourceId"
               LEFT JOIN "PlexMediaSource" plex_source ON plex_source."Id" = source."Id"
               LEFT JOIN "JellyfinMediaSource" jellyfin_source
                 ON jellyfin_source."Id" = source."Id"
               LEFT JOIN "EmbyMediaSource" emby_source ON emby_source."Id" = source."Id"
               LEFT JOIN "MediaVersion" version ON version."OtherVideoId" = video."Id"
               WHERE media."Id" = ? ORDER BY version."Id" LIMIT 1"#,
        )
        .bind(id)
        .fetch_optional(&self.0)
        .await
        .repository()?;
        if let Some(row) = other_video {
            let metadata_id = row.try_get("MetadataId").repository()?;
            let media_version_id = row.try_get("MediaVersionId").repository()?;
            let library_kind: String = row.try_get("LibraryKind").repository()?;
            let info = AdminMediaItemInfo {
                id: row.try_get("Id").repository()?,
                title: row.try_get("Title").repository()?,
                kind: AdminMediaInfoKind::OtherVideo,
                library_kind: library_kind.clone(),
                server_name: row.try_get("ServerName").repository()?,
                library_name: row.try_get("LibraryName").repository()?,
                state: row.try_get("State").repository()?,
                duration: row
                    .try_get::<Option<String>, _>("Duration")
                    .repository()?
                    .unwrap_or_else(|| "00:00:00".to_owned()),
                sample_aspect_ratio: row.try_get("SampleAspectRatio").repository()?,
                display_aspect_ratio: row.try_get("DisplayAspectRatio").repository()?,
                r_frame_rate: row.try_get("RFrameRate").repository()?,
                video_scan_kind: row.try_get("VideoScanKind").repository()?,
                interlaced_ratio: row.try_get("InterlacedRatio").repository()?,
                width: row.try_get("Width").repository()?,
                height: row.try_get("Height").repository()?,
                streams: Vec::new(),
                chapters: Vec::new(),
            };
            return enrich_admin_media_info(
                &self.0,
                info,
                "OtherVideoMetadataId",
                metadata_id,
                media_version_id,
                library_kind == "LocalLibrary",
            )
            .await
            .map(Some);
        }

        for (base_table, metadata_table, item_fk, version_fk, metadata_fk, kind) in [
            (
                "MusicVideo",
                "MusicVideoMetadata",
                "MusicVideoId",
                "MusicVideoId",
                "MusicVideoMetadataId",
                AdminMediaInfoKind::MusicVideo,
            ),
            (
                "Song",
                "SongMetadata",
                "SongId",
                "SongId",
                "SongMetadataId",
                AdminMediaInfoKind::Song,
            ),
            (
                "Image",
                "ImageMetadata",
                "ImageId",
                "ImageId",
                "ImageMetadataId",
                AdminMediaInfoKind::Image,
            ),
        ] {
            let sql = format!(
                r#"SELECT item."Id", COALESCE(metadata."Title", '') AS "Title",
                          metadata."Id" AS "MetadataId", COALESCE(media."State", 0) AS "State",
                          COALESCE(library."Name", '') AS "LibraryName",
                          CASE
                            WHEN local."Id" IS NOT NULL THEN 'LocalLibrary'
                            WHEN plex."Id" IS NOT NULL THEN 'PlexLibrary'
                            ELSE 'Library'
                          END AS "LibraryKind",
                          plex_source."ServerName" AS "ServerName",
                          version."Id" AS "MediaVersionId", version."Duration",
                          version."SampleAspectRatio", version."DisplayAspectRatio",
                          version."RFrameRate", COALESCE(version."VideoScanKind", 0) AS "VideoScanKind",
                          version."InterlacedRatio", COALESCE(version."Width", 0) AS "Width",
                          COALESCE(version."Height", 0) AS "Height"
                   FROM "{base_table}" item
                   JOIN "{metadata_table}" metadata ON metadata."{item_fk}" = item."Id"
                   LEFT JOIN "MediaItem" media ON media."Id" = item."Id"
                   LEFT JOIN "LibraryPath" path ON path."Id" = media."LibraryPathId"
                   LEFT JOIN "Library" library ON library."Id" = path."LibraryId"
                   LEFT JOIN "LocalLibrary" local ON local."Id" = library."Id"
                   LEFT JOIN "PlexLibrary" plex ON plex."Id" = library."Id"
                   LEFT JOIN "MediaSource" source ON source."Id" = library."MediaSourceId"
                   LEFT JOIN "PlexMediaSource" plex_source ON plex_source."Id" = source."Id"
                   LEFT JOIN "MediaVersion" version ON version."{version_fk}" = item."Id"
                   WHERE media."Id" = ? ORDER BY version."Id" LIMIT 1"#
            );
            let row = sqlx::query(crate::db::assert_sql_safe(&sql))
                .bind(id)
                .fetch_optional(&self.0)
                .await
                .repository()?;
            if let Some(row) = row {
                let metadata_id = row.try_get("MetadataId").repository()?;
                let media_version_id = row.try_get("MediaVersionId").repository()?;
                let library_kind: String = row.try_get("LibraryKind").repository()?;
                let info = AdminMediaItemInfo {
                    id: row.try_get("Id").repository()?,
                    title: row.try_get("Title").repository()?,
                    kind,
                    library_kind: library_kind.clone(),
                    server_name: row.try_get("ServerName").repository()?,
                    library_name: row.try_get("LibraryName").repository()?,
                    state: row.try_get("State").repository()?,
                    duration: row
                        .try_get::<Option<String>, _>("Duration")
                        .repository()?
                        .unwrap_or_else(|| "00:00:00".to_owned()),
                    sample_aspect_ratio: row.try_get("SampleAspectRatio").repository()?,
                    display_aspect_ratio: row.try_get("DisplayAspectRatio").repository()?,
                    r_frame_rate: row.try_get("RFrameRate").repository()?,
                    video_scan_kind: row.try_get("VideoScanKind").repository()?,
                    interlaced_ratio: row.try_get("InterlacedRatio").repository()?,
                    width: row.try_get("Width").repository()?,
                    height: row.try_get("Height").repository()?,
                    streams: Vec::new(),
                    chapters: Vec::new(),
                };
                return enrich_admin_media_info(
                    &self.0,
                    info,
                    metadata_fk,
                    metadata_id,
                    media_version_id,
                    library_kind == "LocalLibrary",
                )
                .await
                .map(Some);
            }
        }

        let television = super::television::SqliteTelevisionRepository(self.0.clone());
        let Some(episode) = television.get_episode_media_item_info(id).await? else {
            return Ok(None);
        };
        let media_version_id = episode.media_version_id;
        let info = AdminMediaItemInfo {
            id: episode.id,
            title: format!(
                "{} - s{:02}e{:02} - {}",
                episode.show_title, episode.season_number, episode.episode_number, episode.title
            ),
            kind: AdminMediaInfoKind::Episode,
            library_kind: episode.library_kind,
            server_name: episode.server_name,
            library_name: episode.library_name,
            state: episode.state,
            duration: episode.duration.unwrap_or_else(|| "00:00:00".to_owned()),
            sample_aspect_ratio: episode.sample_aspect_ratio,
            display_aspect_ratio: episode.display_aspect_ratio,
            r_frame_rate: episode.r_frame_rate,
            video_scan_kind: episode.video_scan_kind,
            interlaced_ratio: episode.interlaced_ratio,
            width: episode.width,
            height: episode.height,
            streams: Vec::new(),
            chapters: Vec::new(),
        };
        enrich_admin_media_info(
            &self.0,
            info,
            "EpisodeMetadataId",
            episode.metadata_id,
            media_version_id,
            episode.is_local_library,
        )
        .await
        .map(Some)
    }
}
