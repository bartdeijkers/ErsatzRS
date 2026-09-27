use std::collections::BTreeMap;
use std::path::Path;

use async_trait::async_trait;
use ersatzrs_addon_contract::{
    validate_addon_id, AddonCapability, AddonManifest,
    AddonMediaListOptionValue as ContractOptionValue,
};
use ersatzrs_core::path_replacement::{PathPlatform, PathReplacement};
use ersatzrs_core::repositories::{
    AddonMediaListLifecycleRepository, AddonMediaListOptionSchema, AddonMediaListOptionSchemaRef,
    AdminActorCard, AdminArtistDetail, AdminAudioTrack, AdminCommentarySubtitle,
    AdminFacetedMediaDetail, AdminFacetedMediaKind, AdminMediaCard, AdminMediaInfoChapter,
    AdminMediaInfoKind, AdminMediaInfoStream, AdminMediaItemInfo, AdminMediaRepository,
    AdminMovieDetail, AdminMoviePlaybackSource, AdminMoviePlaybackStream, AdminOtherVideoDetail,
    AdminRemoteStreamChannel, AdminRemoteStreamDetail, AdminRemoteStreamOwner,
    AdminRemoteStreamPlaybackHealth, AdminRemoteStreamPlaybackHealthReason, AdminShowDetail,
    RepositoryError, Result, TelevisionRepository,
};
use ersatzrs_domain::{
    AddonMediaListContentKind, AddonMediaListItemAvailability, AddonMediaListOptionValue,
    ContentKind, EntityId, RemoteStreamLiveness,
};
use semver::Version;
use sqlx::{Pool, Row};

use crate::metadata_query::{
    load_artwork_path, load_projected_artwork, load_strings, projected_artwork,
    split_content_ratings,
};
use crate::remote_stream::{
    decode_persisted_liveness, resolve_remote_stream, RemoteStreamDefinition,
};
use crate::repositories::common::{decode_bool_column, decode_enum_column};
use crate::repositories::{SqliteAddonMediaListRepository, SqlxResultExt as _};

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
                  metadata."Plot", metadata."Album", artwork."Id" AS "Thumbnail",
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

    let artwork = load_projected_artwork(pool, &rows, &["Thumbnail"]).await?;
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
            thumbnail_path: projected_artwork(&row, "Thumbnail", &artwork)?,
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
                  actor."Role", artwork."Id" AS "Artwork"
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
    let artwork = load_projected_artwork(pool, &rows, &["Artwork"]).await?;
    rows.iter()
        .map(|row| {
            Ok(AdminActorCard {
                id: row.try_get("Id").repository()?,
                name: row.try_get("Name").repository()?,
                role: row.try_get("Role").repository()?,
                artwork_path: projected_artwork(row, "Artwork", &artwork)?,
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

fn decode_remote_stream_string_list(
    field: &'static str,
    value: Option<String>,
) -> Result<Vec<String>> {
    let Some(value) = value.filter(|value| !value.trim().is_empty()) else {
        return Ok(Vec::new());
    };
    let values =
        serde_json::from_str::<Vec<String>>(&value).map_err(|error| RepositoryError::Decode {
            kind: field,
            value: error.to_string(),
        })?;
    Ok(values
        .into_iter()
        .filter(|value| !value.trim().is_empty())
        .collect())
}

fn remote_stream_option_defaults(
    schema: &AddonMediaListOptionSchema,
) -> Result<BTreeMap<String, AddonMediaListOptionValue>> {
    let capability =
        serde_json::from_str::<AddonCapability>(&schema.definition_json).map_err(|error| {
            RepositoryError::Decode {
                kind: "add-on media-list option schema",
                value: error.to_string(),
            }
        })?;
    if capability.id != schema.schema.capability_id {
        return Err(RepositoryError::Conflict(
            "published option schema identity does not match its definition".into(),
        ));
    }
    Ok(capability
        .options
        .into_iter()
        .map(|definition| {
            let value = match definition.default {
                ContractOptionValue::Boolean(value) => AddonMediaListOptionValue::Boolean(value),
                ContractOptionValue::Enum(value) => AddonMediaListOptionValue::Enum(value),
            };
            (definition.key, value)
        })
        .collect())
}

fn decode_remote_stream_option_values(
    kind: &'static str,
    json: Option<String>,
) -> Result<BTreeMap<String, AddonMediaListOptionValue>> {
    json.map_or_else(
        || Ok(BTreeMap::new()),
        |json| {
            serde_json::from_str(&json).map_err(|error| RepositoryError::Decode {
                kind,
                value: error.to_string(),
            })
        },
    )
}

async fn load_remote_stream_owners(
    pool: &Pool<crate::db::Db>,
    remote_stream_id: EntityId,
) -> Result<(Vec<AdminRemoteStreamOwner>, bool)> {
    let rows = sqlx::query(
        r#"SELECT list."Id" AS "ListId",
                  list."Name" AS "ListName", list."AddonId", list."PublishedRevision",
                  revision."OptionValuesJson" AS "ListOptionValuesJson",
                  item_option."OptionOverridesJson" AS "ItemOptionValuesJson",
                  schema."AddonId" AS "SchemaAddonId",
                  schema."CapabilityId" AS "SchemaCapabilityId",
                  schema."SchemaDigest" AS "SchemaDigest"
           FROM "AddonMediaListItem" AS item
           JOIN "AddonMediaList" AS list ON list."Id" = item."AddonMediaListId"
           LEFT JOIN "AddonMediaListRevision" AS revision
             ON revision."AddonMediaListId" = list."Id"
            AND revision."Revision" = list."PublishedRevision"
           LEFT JOIN "AddonMediaListRevisionItemOption" AS item_option
             ON item_option."AddonMediaListRevisionId" = revision."Id"
            AND item_option."ProviderId" = item."ProviderId"
           LEFT JOIN "AddonMediaListOptionSchema" AS schema
             ON schema."Id" = revision."OptionSchemaId"
           WHERE item."MediaItemId" = ? AND item."Published" = 1
           ORDER BY list."Name", list."Id", item."Rank", item."Id""#,
    )
    .bind(remote_stream_id)
    .fetch_all(pool)
    .await
    .repository()?;
    let lifecycle = SqliteAddonMediaListRepository(pool.clone());
    let mut owners = Vec::with_capacity(rows.len());
    let mut missing_option_schema = false;
    let mut schema_cache: BTreeMap<(String, String, String), Option<AddonMediaListOptionSchema>> =
        BTreeMap::new();
    for row in rows {
        let list_id = row.try_get("ListId").repository()?;
        let published_revision = row.try_get("PublishedRevision").repository()?;
        let list_options = decode_remote_stream_option_values(
            "add-on media-list published option values",
            row.try_get("ListOptionValuesJson").repository()?,
        )?;
        let item_options = decode_remote_stream_option_values(
            "add-on media-list published item option values",
            row.try_get("ItemOptionValuesJson").repository()?,
        )?;
        let schema_ref = match (
            row.try_get::<Option<String>, _>("SchemaAddonId")
                .repository()?,
            row.try_get::<Option<String>, _>("SchemaCapabilityId")
                .repository()?,
            row.try_get::<Option<String>, _>("SchemaDigest")
                .repository()?,
        ) {
            (Some(addon_id), Some(capability_id), Some(schema_digest)) => {
                Some(AddonMediaListOptionSchemaRef {
                    addon_id,
                    capability_id,
                    schema_digest,
                })
            }
            (None, None, None) => None,
            _ => {
                return Err(RepositoryError::Conflict(
                    "published option schema identity is incomplete".into(),
                ));
            }
        };
        let mut option_schema = None;
        if let Some(schema_ref) = schema_ref {
            let key = (
                schema_ref.addon_id.clone(),
                schema_ref.capability_id.clone(),
                schema_ref.schema_digest.clone(),
            );
            if let Some(cached) = schema_cache.get(&key) {
                option_schema = cached.clone();
            } else {
                option_schema = lifecycle.option_schema(&schema_ref).await?;
                schema_cache.insert(key, option_schema.clone());
            }
            missing_option_schema |= option_schema.is_none();
        }
        let mut effective_options = option_schema
            .as_ref()
            .map(remote_stream_option_defaults)
            .transpose()?
            .unwrap_or_default();
        effective_options.extend(list_options.clone());
        effective_options.extend(item_options.clone());
        if option_schema.is_none() && (!list_options.is_empty() || !item_options.is_empty()) {
            missing_option_schema = true;
        }
        owners.push(AdminRemoteStreamOwner {
            list_id,
            list_name: row.try_get("ListName").repository()?,
            addon_id: row.try_get("AddonId").repository()?,
            published_revision,
            published_effective_options: effective_options,
            option_schema,
        });
    }
    Ok((owners, missing_option_schema))
}

async fn load_remote_stream_channels(
    pool: &Pool<crate::db::Db>,
    remote_stream_id: EntityId,
) -> Result<Vec<AdminRemoteStreamChannel>> {
    let rows = sqlx::query(
        r#"SELECT DISTINCT channel."Id" AS "ChannelId",
                  COALESCE(channel."Name", '') AS "ChannelName",
                  COALESCE(channel."Number", '') AS "ChannelNumber",
                  channel."IsEnabled"
           FROM "PlayoutItem" AS item
           JOIN "Playout" AS playout ON playout."Id" = item."PlayoutId"
           JOIN "Channel" AS channel ON channel."Id" = playout."ChannelId"
           WHERE item."MediaItemId" = ?
              OR EXISTS (
                  SELECT 1
                  FROM "AddonMediaListItem" AS owner
                  JOIN "AddonMediaListItemReplacement" AS replacement
                    ON replacement."AddonMediaListItemId" = owner."Id"
                  WHERE owner."MediaItemId" = ? AND owner."Published" = 1
                    AND item."MediaItemId" = replacement."MediaItemId"
                    AND (item."MediaVersionId" IS NULL
                         OR item."MediaVersionId" = replacement."MediaVersionId")
              )
           ORDER BY channel."SortNumber", channel."Id""#,
    )
    .bind(remote_stream_id)
    .bind(remote_stream_id)
    .fetch_all(pool)
    .await
    .repository()?;
    rows.iter()
        .map(|row| {
            Ok(AdminRemoteStreamChannel {
                channel_id: row.try_get("ChannelId").repository()?,
                name: row.try_get("ChannelName").repository()?,
                number: row.try_get("ChannelNumber").repository()?,
                is_enabled: decode_bool_column(row, "IsEnabled")?,
            })
        })
        .collect()
}

fn has_usable_remote_stream_duration(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        let value = value.trim();
        if value.is_empty() || value == "00:00:00" || value.starts_with("00:00:00.") {
            return false;
        }
        resolve_remote_stream(
            &RemoteStreamDefinition {
                url: Some("duration-validation".to_owned()),
                duration: Some(value.to_owned()),
                liveness: Some(RemoteStreamLiveness::Live),
                is_live: true,
                ..RemoteStreamDefinition::default()
            },
            Path::new("remote-stream.yml"),
        )
        .is_ok()
    })
}

fn remote_stream_addon_is_compatible(addon_id: &str, manifest_json: Option<&str>) -> bool {
    let Some(host_version) = Version::parse(env!("CARGO_PKG_VERSION")).ok() else {
        return false;
    };
    validate_addon_id(addon_id).is_ok()
        && manifest_json
            .and_then(|json| serde_json::from_str::<AddonManifest>(json).ok())
            .is_some_and(|manifest| {
                manifest.validate().is_ok()
                    && manifest.id == addon_id
                    && manifest.has_remote_stream_play_capability()
                    && manifest.host_version.matches(&host_version)
            })
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
                  metadata."Year", artwork."Id" AS "Artwork",
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
                  metadata."Year", CASE WHEN thumb."Path" IS NOT NULL THEN thumb."Id" ELSE poster."Id" END AS "Artwork",
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
    let artwork = load_projected_artwork(pool, &rows, &["Artwork"]).await?;
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
                artwork_path: projected_artwork(row, "Artwork", &artwork)?,
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
            if external == "319" {
                networks.push(name);
            } else if external != "nfo/country" {
                tags.push(name);
            }
        }
        let season_rows = sqlx::query(
            r#"SELECT season."Id", season."SeasonNumber",
                      (
                          SELECT artwork."Id"
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
        let artwork = load_projected_artwork(&self.0, &season_rows, &["Poster"]).await?;
        let seasons = season_rows
            .iter()
            .map(|season| {
                Ok(ersatzrs_core::repositories::AdminSeasonCard {
                    id: season.try_get("Id").repository()?,
                    season_number: season.try_get("SeasonNumber").repository()?,
                    poster_path: projected_artwork(season, "Poster", &artwork)?,
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
                      thumbnail."Id" AS "Thumbnail", fanart."Id" AS "FanArt"
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
        let artwork = load_projected_artwork(
            &self.0,
            std::slice::from_ref(&row),
            &["Thumbnail", "FanArt"],
        )
        .await?;
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
            thumbnail_path: projected_artwork(&row, "Thumbnail", &artwork)?,
            fanart_path: projected_artwork(&row, "FanArt", &artwork)?,
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

    #[allow(clippy::too_many_lines)]
    async fn get_remote_stream_detail_admin(
        &self,
        id: EntityId,
    ) -> Result<Option<AdminRemoteStreamDetail>> {
        let Some(row) = sqlx::query(
            r#"SELECT stream."Id", stream."Url", stream."Script", stream."AddonId",
                      stream."Duration" AS "ConfiguredDuration", stream."FallbackQuery",
                      stream."Liveness", stream."IsLive",
                      stream."Availability", stream."AvailabilityReason", stream."ContentKind",
                      metadata."Id" AS "MetadataId",
                      COALESCE(metadata."Title", '') AS "Title",
                      COALESCE(metadata."SortTitle", metadata."Title", '') AS "SortTitle",
                      metadata."OriginalTitle", metadata."Year", metadata."Plot",
                      metadata."Outline", metadata."Tagline", metadata."ShowTitle",
                      metadata."SeasonNumber", metadata."EpisodeNumber", metadata."ReleaseDate",
                      metadata."CollectionName", metadata."OriginalBroadcasters",
                      metadata."Broadcasters", metadata."ContentRating",
                      COALESCE(media."State", 0) AS "State",
                      version."Duration" AS "ProbedDuration",
                      installation."Enabled" AS "AddonEnabled",
                      installation."ManifestJson" AS "AddonManifestJson",
                      -- An unavailable published item can play from its assigned Local
                      -- replacement. Keep this scalar so multiple owners cannot multiply
                      -- the detail row.
                      (SELECT replacement_version."Duration"
                         FROM "AddonMediaListItem" AS owner
                         JOIN "AddonMediaListItemReplacement" AS replacement
                           ON replacement."AddonMediaListItemId" = owner."Id"
                         JOIN "MediaItem" AS replacement_item
                           ON replacement_item."Id" = replacement."MediaItemId"
                          AND replacement_item."State" = 0
                         JOIN "LibraryPath" AS replacement_path
                           ON replacement_path."Id" = replacement_item."LibraryPathId"
                         JOIN "Library" AS replacement_library
                           ON replacement_library."Id" = replacement_path."LibraryId"
                         JOIN "LocalMediaSource" AS replacement_local
                           ON replacement_local."Id" = replacement_library."MediaSourceId"
                         JOIN "MediaVersion" AS replacement_version
                           ON replacement_version."Id" = replacement."MediaVersionId"
                          AND (replacement_version."MovieId" = replacement_item."Id"
                               OR replacement_version."EpisodeId" = replacement_item."Id"
                               OR replacement_version."MusicVideoId" = replacement_item."Id"
                               OR replacement_version."OtherVideoId" = replacement_item."Id")
                         JOIN "MediaFile" AS replacement_file
                           ON replacement_file."MediaVersionId" = replacement_version."Id"
                        WHERE owner."MediaItemId" = stream."Id" AND owner."Published" = 1
                          AND EXISTS (
                              SELECT 1 FROM "MediaStream" AS replacement_stream
                              WHERE replacement_stream."MediaVersionId" = replacement_version."Id"
                                AND replacement_stream."MediaStreamKind" = 1
                                AND replacement_stream."AttachedPic" = 0
                          )
                        ORDER BY replacement_version."Duration" DESC
                        LIMIT 1) AS "ReplacementDuration"
               FROM "RemoteStream" AS stream
               JOIN "MediaItem" AS media ON media."Id" = stream."Id"
               JOIN "RemoteStreamMetadata" AS metadata
                 ON metadata."RemoteStreamId" = stream."Id"
               LEFT JOIN "MediaVersion" AS version
                 ON version."RemoteStreamId" = stream."Id"
                AND version."Id" = (
                    SELECT candidate."Id" FROM "MediaVersion" AS candidate
                    WHERE candidate."RemoteStreamId" = stream."Id"
                    ORDER BY candidate."Id" LIMIT 1
                )
               LEFT JOIN "AddonInstallation" AS installation
                 ON installation."AddonId" = stream."AddonId"
               WHERE stream."Id" = ?
               ORDER BY metadata."Id" LIMIT 1"#,
        )
        .bind(id)
        .fetch_optional(&self.0)
        .await
        .repository()?
        else {
            return Ok(None);
        };

        let metadata_id: EntityId = row.try_get("MetadataId").repository()?;
        let content_rating: Option<String> = row.try_get("ContentRating").repository()?;
        let configured_duration: Option<String> = row.try_get("ConfiguredDuration").repository()?;
        let probed_duration: Option<String> = row.try_get("ProbedDuration").repository()?;
        let replacement_duration: Option<String> =
            row.try_get("ReplacementDuration").repository()?;
        let stored_liveness: i64 = row.try_get("Liveness").repository()?;
        let legacy_is_live = decode_bool_column(&row, "IsLive")?;
        let (liveness, persisted_liveness_valid) =
            match decode_persisted_liveness(stored_liveness, legacy_is_live) {
                Ok(liveness) => (liveness, true),
                Err(_) => (
                    decode_enum_column::<RemoteStreamLiveness>(
                        "RemoteStreamLiveness",
                        &row,
                        "Liveness",
                    )?,
                    false,
                ),
            };
        let availability = decode_enum_column::<AddonMediaListItemAvailability>(
            "AddonMediaListItemAvailability",
            &row,
            "Availability",
        )?;
        let media_item_state: i32 = row.try_get("State").repository()?;
        let url: Option<String> = row.try_get("Url").repository()?;
        let script: Option<String> = row.try_get("Script").repository()?;
        let raw_addon_id: Option<String> = row.try_get("AddonId").repository()?;
        let source_addon_id = clean_string(raw_addon_id.clone());
        let has_url = url.as_deref().is_some_and(|url| !url.trim().is_empty());
        let fallback_query: Option<String> = row.try_get("FallbackQuery").repository()?;
        let mut direct_definition = RemoteStreamDefinition {
            url: url.clone(),
            duration: configured_duration.clone(),
            fallback_query,
            liveness: Some(liveness),
            is_live: legacy_is_live,
            ..RemoteStreamDefinition::default()
        };
        // Source validation is separate from the explicit unknown-liveness reason.
        // This mirrors the existing health check without making a network request.
        if direct_definition.liveness == Some(RemoteStreamLiveness::Unknown) {
            direct_definition.liveness = Some(RemoteStreamLiveness::Finite);
            direct_definition.is_live = false;
        }
        let direct_source_valid = raw_addon_id.is_some()
            || resolve_remote_stream(&direct_definition, Path::new("remote-stream.yml")).is_ok();
        let invalid_source_configuration = !persisted_liveness_valid
            || (script.is_none() && has_url && raw_addon_id.is_none() && !direct_source_valid);
        let mut health_reasons = Vec::new();
        if media_item_state != 0 && replacement_duration.is_none() {
            health_reasons.push(AdminRemoteStreamPlaybackHealthReason::MediaUnavailable);
        }
        match availability {
            AddonMediaListItemAvailability::Unavailable if replacement_duration.is_none() => {
                health_reasons.push(AdminRemoteStreamPlaybackHealthReason::AvailabilityUnavailable);
            }
            AddonMediaListItemAvailability::Available
            | AddonMediaListItemAvailability::Unavailable => {}
            AddonMediaListItemAvailability::Unknown => {
                health_reasons.push(AdminRemoteStreamPlaybackHealthReason::AvailabilityUnknown);
            }
        }
        if script.is_some() {
            health_reasons.push(AdminRemoteStreamPlaybackHealthReason::LegacyScriptUnsupported);
        }
        if !has_url {
            health_reasons.push(AdminRemoteStreamPlaybackHealthReason::SourceMissing);
        }
        if invalid_source_configuration {
            health_reasons.push(AdminRemoteStreamPlaybackHealthReason::SourceInvalid);
        }
        if let Some(addon_id) = raw_addon_id.as_deref() {
            let addon_enabled = row
                .try_get::<Option<i64>, _>("AddonEnabled")
                .repository()?
                .map(|value| value != 0);
            let manifest_json: Option<String> = row.try_get("AddonManifestJson").repository()?;
            if validate_addon_id(addon_id).is_err() {
                health_reasons.push(AdminRemoteStreamPlaybackHealthReason::AddonIncompatible);
            } else if addon_enabled.is_none() {
                health_reasons.push(AdminRemoteStreamPlaybackHealthReason::AddonMissing);
            } else if addon_enabled == Some(false) {
                health_reasons.push(AdminRemoteStreamPlaybackHealthReason::AddonDisabled);
            } else if !remote_stream_addon_is_compatible(addon_id, manifest_json.as_deref()) {
                health_reasons.push(AdminRemoteStreamPlaybackHealthReason::AddonIncompatible);
            }
        }
        match liveness {
            RemoteStreamLiveness::Unknown => {
                health_reasons.push(AdminRemoteStreamPlaybackHealthReason::LivenessUnknown);
            }
            RemoteStreamLiveness::Finite
                if !has_usable_remote_stream_duration(configured_duration.as_deref())
                    && !has_usable_remote_stream_duration(probed_duration.as_deref())
                    && !has_usable_remote_stream_duration(replacement_duration.as_deref()) =>
            {
                health_reasons.push(AdminRemoteStreamPlaybackHealthReason::FiniteDurationMissing);
            }
            RemoteStreamLiveness::Finite | RemoteStreamLiveness::Live => {}
        }

        let (owners, missing_option_schema) = load_remote_stream_owners(&self.0, id).await?;
        if missing_option_schema {
            health_reasons
                .push(AdminRemoteStreamPlaybackHealthReason::PublishedOptionSchemaMissing);
        }
        let channels = load_remote_stream_channels(&self.0, id).await?;
        let media_info =
            self.get_media_item_info_admin(id)
                .await?
                .ok_or(RepositoryError::NotFound {
                    kind: "Remote Stream media info",
                    id,
                })?;
        let stream_languages = load_audio_languages(&self.0, "RemoteStreamId", id).await?;
        let effective_duration = if availability == AddonMediaListItemAvailability::Unavailable {
            replacement_duration
                .as_deref()
                .filter(|duration| has_usable_remote_stream_duration(Some(duration)))
        } else {
            None
        }
        .or_else(|| {
            probed_duration
                .as_deref()
                .filter(|duration| has_usable_remote_stream_duration(Some(duration)))
        })
        .or_else(|| {
            configured_duration
                .as_deref()
                .filter(|duration| has_usable_remote_stream_duration(Some(duration)))
        })
        .map(ToOwned::to_owned);

        Ok(Some(AdminRemoteStreamDetail {
            id,
            title: row.try_get("Title").repository()?,
            original_title: row.try_get("OriginalTitle").repository()?,
            sort_title: row.try_get("SortTitle").repository()?,
            year: row.try_get("Year").repository()?,
            plot: row.try_get("Plot").repository()?,
            outline: row.try_get("Outline").repository()?,
            tagline: row.try_get("Tagline").repository()?,
            show_title: row.try_get("ShowTitle").repository()?,
            season_number: row.try_get("SeasonNumber").repository()?,
            episode_number: row.try_get("EpisodeNumber").repository()?,
            release_date: row.try_get("ReleaseDate").repository()?,
            collection_name: row.try_get("CollectionName").repository()?,
            original_broadcasters: decode_remote_stream_string_list(
                "RemoteStreamMetadata.OriginalBroadcasters",
                row.try_get("OriginalBroadcasters").repository()?,
            )?,
            broadcasters: decode_remote_stream_string_list(
                "RemoteStreamMetadata.Broadcasters",
                row.try_get("Broadcasters").repository()?,
            )?,
            thumbnail_path: load_artwork_path(&self.0, "RemoteStreamMetadataId", metadata_id, 1)
                .await?,
            poster_path: load_artwork_path(&self.0, "RemoteStreamMetadataId", metadata_id, 0)
                .await?,
            fanart_path: load_artwork_path(&self.0, "RemoteStreamMetadataId", metadata_id, 3)
                .await?,
            media_item_state,
            content_kind: decode_enum_column::<AddonMediaListContentKind>(
                "AddonMediaListContentKind",
                &row,
                "ContentKind",
            )?,
            liveness,
            configured_duration,
            probed_duration,
            effective_duration,
            availability,
            availability_reason: clean_string(row.try_get("AvailabilityReason").repository()?),
            source_addon_id,
            facets: load_media_facets(
                &self.0,
                "RemoteStreamMetadataId",
                metadata_id,
                content_rating,
                stream_languages,
            )
            .await?,
            media_info,
            owners,
            channels,
            playback_health: AdminRemoteStreamPlaybackHealth::from_reasons(health_reasons),
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

        let remote_stream = sqlx::query(
            r#"SELECT stream."Id", COALESCE(metadata."Title", '') AS "Title",
                      metadata."Id" AS "MetadataId", COALESCE(media."State", 0) AS "State",
                      COALESCE(library."Name", '') AS "LibraryName",
                      CASE WHEN local."Id" IS NOT NULL THEN 'LocalLibrary'
                           ELSE 'Library' END AS "LibraryKind",
                      version."Id" AS "MediaVersionId", version."Duration",
                      version."SampleAspectRatio", version."DisplayAspectRatio",
                      version."RFrameRate", COALESCE(version."VideoScanKind", 0) AS "VideoScanKind",
                      version."InterlacedRatio", COALESCE(version."Width", 0) AS "Width",
                      COALESCE(version."Height", 0) AS "Height"
               FROM "RemoteStream" AS stream
               JOIN "RemoteStreamMetadata" AS metadata
                 ON metadata."RemoteStreamId" = stream."Id"
               JOIN "MediaItem" AS media ON media."Id" = stream."Id"
               LEFT JOIN "LibraryPath" AS path ON path."Id" = media."LibraryPathId"
               LEFT JOIN "Library" AS library ON library."Id" = path."LibraryId"
               LEFT JOIN "LocalLibrary" AS local ON local."Id" = library."Id"
               LEFT JOIN "MediaVersion" AS version ON version."RemoteStreamId" = stream."Id"
               WHERE media."Id" = ?
               ORDER BY metadata."Id", version."Id" LIMIT 1"#,
        )
        .bind(id)
        .fetch_optional(&self.0)
        .await
        .repository()?;
        if let Some(row) = remote_stream {
            let metadata_id = row.try_get("MetadataId").repository()?;
            let media_version_id = row.try_get("MediaVersionId").repository()?;
            let library_kind: String = row.try_get("LibraryKind").repository()?;
            let info = AdminMediaItemInfo {
                id: row.try_get("Id").repository()?,
                title: row.try_get("Title").repository()?,
                kind: AdminMediaInfoKind::RemoteStream,
                library_kind: library_kind.clone(),
                server_name: None,
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
                "RemoteStreamMetadataId",
                metadata_id,
                media_version_id,
                library_kind == "LocalLibrary",
            )
            .await
            .map(Some);
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

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest as _, Sha256};

    const OPTION_SCHEMA_JSON: &str = r#"{"id":"media-list.options.v1","options":[{"key":"quality","label":{"en-US":"Quality"},"description":{"en-US":"Playback quality"},"scope":"both","operation_targets":["playback"],"applicability":"all","kind":"enum","default":"medium","choices":[{"value":"medium","label":{"en-US":"Medium"}},{"value":"high","label":{"en-US":"High"}},{"value":"low","label":{"en-US":"Low"}}]},{"key":"remove_ads","label":{"en-US":"Remove ads"},"description":{"en-US":"Remove marked breaks"},"scope":"both","operation_targets":["playback"],"applicability":"all","kind":"boolean","default":true}]}"#;

    async fn test_pool() -> Pool<crate::db::Db> {
        crate::db::init("sqlite::memory:")
            .await
            .expect("initialize database")
    }

    #[allow(clippy::too_many_lines)]
    async fn seed_remote_stream_detail(pool: &Pool<crate::db::Db>) {
        let now = "2026-09-04T12:00:00Z";
        for statement in [
            r#"INSERT INTO "Resolution" ("Id", "Name", "Width", "Height")
               VALUES (1, '1920x1080', 1920, 1080)"#,
            r#"INSERT INTO "FFmpegProfile" (
                   "Id", "AudioBitrate", "AudioBufferSize", "AudioChannels",
                   "AudioFormat", "AudioSampleRate", "BitDepth", "HardwareAcceleration",
                   "Name", "NormalizeLoudnessMode", "ResolutionId", "ScalingBehavior",
                   "ThreadCount", "VaapiDriver", "VideoBitrate", "VideoBufferSize",
                   "VideoFormat"
               ) VALUES (1, 192, 0, 2, 0, 48000, 8, 0, 'default', 0, 1, 0, 0, 0,
                         4000, 0, 0)"#,
            r#"INSERT INTO "Channel" (
                   "Id", "UniqueId", "Number", "SortNumber", "Name", "Group",
                   "FFmpegProfileId", "StreamingMode", "StreamSelectorMode", "SubtitleMode",
                   "MusicVideoCreditsMode", "SongVideoMode", "PlayoutSource", "PlayoutMode",
                   "TranscodeMode", "IdleBehavior", "IsEnabled", "ShowInEpg"
               ) VALUES (39, '00000000-0000-0000-0000-000000000039', '42', 42.0,
                         'Track channel', 'ErsatzRS', 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1)"#,
            r#"INSERT INTO "MediaSource" ("Id") VALUES (10)"#,
            r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (10)"#,
            r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
               VALUES (11, 'Managed streams', 7, 10)"#,
            r#"INSERT INTO "LocalLibrary" ("Id") VALUES (11)"#,
            r#"INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId")
               VALUES (12, '/managed/streams', 11)"#,
            r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (20, 12, 0)"#,
            r#"INSERT INTO "RemoteStream" (
                   "Id", "Url", "Duration", "IsLive", "Liveness", "Availability",
                   "AvailabilityReason", "ContentKind"
               ) VALUES (20, 'https://example.test/watch', NULL, 1, 2, 0, NULL, 1)"#,
            r#"INSERT INTO "RemoteStreamMetadata" (
                   "Id", "RemoteStreamId", "MetadataKind", "Title", "OriginalTitle",
                   "SortTitle", "Year", "ReleaseDate", "DateAdded", "DateUpdated",
                   "ContentRating", "Plot", "Outline", "Tagline", "ShowTitle",
                   "SeasonNumber", "EpisodeNumber", "CollectionName",
                   "OriginalBroadcasters", "Broadcasters"
               ) VALUES (
                   21, 20, 1, 'Night race', 'Original race', 'night race', 2026,
                   '2026-09-04', '2026-09-04T12:00:00Z', '2026-09-04T12:00:00Z',
                   'nl:AL/us:TV-G', 'Race coverage', 'Race outline', 'Live from the circuit',
                   'Racing archive', 0, 0, 'Motorsport', '["Original Sports"]',
                   '["Current Sports"]'
               )"#,
            r#"INSERT INTO "MediaVersion" (
                   "Id", "DateAdded", "DateUpdated", "Duration", "Height", "Name",
                   "RemoteStreamId", "VideoScanKind", "Width"
               ) VALUES (22, '2026-09-04T12:00:00Z', '2026-09-04T12:00:00Z',
                         '00:00:00', 1080, 'Main', 20, 0, 1920)"#,
            r#"INSERT INTO "MediaStream" (
                   "Index", "Codec", "MediaStreamKind", "Language", "Channels", "Title",
                   "Default", "Forced", "MediaVersionId"
               ) VALUES
                   (0, 'h264', 1, NULL, 0, 'Video', 1, 0, 22),
                   (1, 'aac', 2, 'eng', 2, 'English', 1, 0, 22)"#,
            r#"INSERT INTO "Subtitle" (
                   "Codec", "DateAdded", "DateUpdated", "Default", "Forced", "IsExtracted",
                   "Language", "Path", "RemoteStreamMetadataId", "StreamIndex",
                   "SubtitleKind", "Title"
               ) VALUES ('webvtt', '2026-09-04T12:00:00Z', '2026-09-04T12:00:00Z',
                         0, 0, 0, 'nld', 'night-race.nl.vtt', 21, 2, 1, 'Dutch')"#,
            r#"INSERT INTO "MediaChapter" (
                   "MediaVersionId", "ChapterId", "StartTime", "EndTime", "Title"
               ) VALUES (22, 0, '00:00:00', '00:10:00', 'Opening')"#,
            r#"INSERT INTO "Genre" ("Name", "RemoteStreamMetadataId")
               VALUES ('Sports', 21)"#,
            r#"INSERT INTO "Tag" ("Name", "RemoteStreamMetadataId")
               VALUES ('Night', 21)"#,
            r#"INSERT INTO "Studio" ("Name", "RemoteStreamMetadataId")
               VALUES ('Track TV', 21)"#,
            r#"INSERT INTO "MusicLabel" ("Name", "RemoteStreamMetadataId")
               VALUES ('Race Audio', 21)"#,
            r#"INSERT INTO "MetadataLanguage" ("Code", "RemoteStreamMetadataId")
               VALUES ('nld', 21)"#,
            r#"INSERT INTO "Artwork" (
                   "ArtworkKind", "DateAdded", "DateUpdated", "Path", "RemoteStreamMetadataId"
               ) VALUES
                   (0, '2026-09-04T12:00:00Z', '2026-09-04T12:00:00Z', 'race-poster.jpg', 21),
                   (1, '2026-09-04T12:00:00Z', '2026-09-04T12:00:00Z', 'race-thumb.jpg', 21),
                   (3, '2026-09-04T12:00:00Z', '2026-09-04T12:00:00Z', 'race-fanart.jpg', 21)"#,
        ] {
            sqlx::query(statement)
                .execute(pool)
                .await
                .expect("seed Remote Stream detail");
        }

        sqlx::query(
            r#"INSERT INTO "AddonInstallation" (
                   "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
                   "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt"
               ) VALUES (?, '1.0.0', 1, 'package', 'content', '{}', 'addons/example', ?, ?)"#,
        )
        .bind("org.example.lists")
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .expect("seed add-on installation");

        let schema_digest = schema_digest_for_test();
        sqlx::query(
            r#"INSERT INTO "AddonMediaListOptionSchema" (
                   "Id", "AddonId", "CapabilityId", "SchemaDigest", "DefinitionJson", "CreatedAt"
               ) VALUES (30, ?, 'media-list.options.v1', ?, ?, ?)"#,
        )
        .bind("org.example.lists")
        .bind(&schema_digest)
        .bind(OPTION_SCHEMA_JSON)
        .bind(now)
        .execute(pool)
        .await
        .expect("seed frozen option schema");
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" (
                   "Id", "AddonId", "ProviderId", "SourceUrl", "Name", "PublishedRevision"
               ) VALUES (31, ?, 'list', 'https://example.test/list', 'Race list', 3)"#,
        )
        .bind("org.example.lists")
        .execute(pool)
        .await
        .expect("seed add-on media list");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListRevision" (
                   "Id", "AddonMediaListId", "Revision", "SourceDraftRevision",
                   "OptionSchemaId", "OptionValuesJson", "CompletedAt"
               ) VALUES (32, 31, 3, 4, 30, '{"quality":"high"}', ?)"#,
        )
        .bind(now)
        .execute(pool)
        .await
        .expect("seed published option revision");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListItem" (
                   "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind",
                   "MediaItemId", "Rank", "Title", "Included", "Published"
               ) VALUES
                   (33, 31, 'race', 'Night race', 0, 20, 0, 'Night race', 1, 1),
                   (34, 31, 'excluded-race', 'Excluded race', 0, 20, 1,
                    'Excluded race', 0, 1)"#,
        )
        .execute(pool)
        .await
        .expect("seed owner mappings");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListRevisionItemOption" (
                   "AddonMediaListRevisionId", "ProviderId", "OptionOverridesJson"
               ) VALUES (32, 'race', '{"quality":"low"}')"#,
        )
        .execute(pool)
        .await
        .expect("seed published item options");

        sqlx::query(
            r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed")
               VALUES (40, 39, 0, 1)"#,
        )
        .execute(pool)
        .await
        .expect("seed playout");
        for item_id in [41_i64, 42] {
            sqlx::query(
                r#"INSERT INTO "PlayoutItem" (
                       "Id", "DisableWatermarks", "FillerKind", "Finish", "GuideGroup",
                       "InPoint", "MediaItemId", "OutPoint", "PlayoutId", "Start"
                   ) VALUES (?, 0, 0, '2026-09-04T13:00:00Z', 0, '00:00:00', 20,
                             '00:10:00', 40, '2026-09-04T12:00:00Z')"#,
            )
            .bind(item_id)
            .execute(pool)
            .await
            .expect("seed playout item");
        }
    }

    #[tokio::test]
    async fn remote_stream_detail_projects_metadata_media_info_owners_and_channels() {
        let pool = test_pool().await;
        seed_remote_stream_detail(&pool).await;
        let repository = SqliteAdminMediaRepository(pool);

        let detail = repository
            .get_remote_stream_detail_admin(20)
            .await
            .expect("load Remote Stream detail")
            .expect("Remote Stream detail exists");

        assert_eq!(detail.title, "Night race");
        assert_eq!(detail.original_title.as_deref(), Some("Original race"));
        assert_eq!(detail.show_title.as_deref(), Some("Racing archive"));
        assert_eq!(detail.collection_name.as_deref(), Some("Motorsport"));
        assert_eq!(detail.original_broadcasters, ["Original Sports"]);
        assert_eq!(detail.broadcasters, ["Current Sports"]);
        assert_eq!(
            detail.thumbnail_path.as_ref().map(|art| art.path.as_str()),
            Some("race-thumb.jpg")
        );
        assert_eq!(
            detail.poster_path.as_ref().map(|art| art.path.as_str()),
            Some("race-poster.jpg")
        );
        assert_eq!(
            detail.fanart_path.as_ref().map(|art| art.path.as_str()),
            Some("race-fanart.jpg")
        );
        assert_eq!(detail.facets.genres, ["Sports"]);
        assert_eq!(detail.facets.tags, ["Night"]);
        assert_eq!(detail.facets.studios, ["Track TV"]);
        assert_eq!(detail.facets.music_labels, ["Race Audio"]);
        assert_eq!(detail.media_info.kind, AdminMediaInfoKind::RemoteStream);
        assert_eq!(detail.media_info.streams.len(), 3);
        assert_eq!(detail.media_info.chapters[0].title, "Opening");
        assert_eq!(detail.configured_duration, None);
        assert_eq!(detail.probed_duration.as_deref(), Some("00:00:00"));
        assert_eq!(detail.effective_duration, None);
        assert_eq!(
            detail.playback_health.status,
            ersatzrs_core::repositories::AdminRemoteStreamPlaybackHealthStatus::Ready
        );

        assert_eq!(detail.owners.len(), 2);
        let owner = &detail.owners[0];
        assert_eq!(owner.list_id, 31);
        assert_eq!(owner.published_revision, 3);
        assert_eq!(
            owner.published_effective_options.get("quality"),
            Some(&AddonMediaListOptionValue::Enum("low".to_owned()))
        );
        assert_eq!(
            owner.published_effective_options.get("remove_ads"),
            Some(&AddonMediaListOptionValue::Boolean(true))
        );
        assert_eq!(
            owner
                .option_schema
                .as_ref()
                .map(|schema| schema.schema.schema_digest.as_str()),
            Some(schema_digest_for_test().as_str())
        );
        assert_eq!(
            detail.owners[1].published_effective_options.get("quality"),
            Some(&AddonMediaListOptionValue::Enum("high".to_owned()))
        );

        assert_eq!(detail.channels.len(), 1);
        assert_eq!(detail.channels[0].name, "Track channel");
        assert_eq!(detail.channels[0].number, "42");
    }

    fn schema_digest_for_test() -> String {
        use std::fmt::Write as _;

        let digest = Sha256::digest(OPTION_SCHEMA_JSON.as_bytes());
        let mut encoded = String::with_capacity(digest.len() * 2);
        for byte in digest {
            let _ = write!(encoded, "{byte:02x}");
        }
        encoded
    }

    #[tokio::test]
    async fn remote_stream_detail_reports_not_found() {
        let repository = SqliteAdminMediaRepository(test_pool().await);
        assert!(repository
            .get_remote_stream_detail_admin(9_999)
            .await
            .expect("query absent Remote Stream")
            .is_none());
    }

    #[tokio::test]
    async fn remote_stream_playback_health_respects_liveness_and_persisted_duration() {
        let pool = test_pool().await;
        seed_remote_stream_detail(&pool).await;
        let repository = SqliteAdminMediaRepository(pool.clone());

        let live = repository
            .get_remote_stream_detail_admin(20)
            .await
            .expect("load live detail")
            .expect("live detail");
        assert_eq!(
            live.playback_health.status,
            ersatzrs_core::repositories::AdminRemoteStreamPlaybackHealthStatus::Ready
        );

        sqlx::query(r#"UPDATE "RemoteStream" SET "Liveness" = 1, "IsLive" = 0 WHERE "Id" = 20"#)
            .execute(&pool)
            .await
            .expect("make stream finite");
        let finite_without_duration = repository
            .get_remote_stream_detail_admin(20)
            .await
            .expect("load finite detail")
            .expect("finite detail");
        assert_eq!(
            finite_without_duration.playback_health.status,
            ersatzrs_core::repositories::AdminRemoteStreamPlaybackHealthStatus::NeedsReview
        );
        assert!(finite_without_duration
            .playback_health
            .reasons
            .contains(&AdminRemoteStreamPlaybackHealthReason::FiniteDurationMissing));

        sqlx::query(r#"UPDATE "RemoteStream" SET "Duration" = '00:17:00' WHERE "Id" = 20"#)
            .execute(&pool)
            .await
            .expect("configure finite duration");
        let finite_with_configured_duration = repository
            .get_remote_stream_detail_admin(20)
            .await
            .expect("load finite detail with configured duration")
            .expect("finite detail with configured duration");
        assert_eq!(
            finite_with_configured_duration.playback_health.status,
            ersatzrs_core::repositories::AdminRemoteStreamPlaybackHealthStatus::Ready
        );
        assert_eq!(
            finite_with_configured_duration
                .effective_duration
                .as_deref(),
            Some("00:17:00")
        );

        sqlx::query(r#"UPDATE "MediaVersion" SET "Duration" = '00:42:00' WHERE "Id" = 22"#)
            .execute(&pool)
            .await
            .expect("persist probed duration");
        let finite_with_duration = repository
            .get_remote_stream_detail_admin(20)
            .await
            .expect("load finite detail with duration")
            .expect("finite detail with duration");
        assert_eq!(
            finite_with_duration.playback_health.status,
            ersatzrs_core::repositories::AdminRemoteStreamPlaybackHealthStatus::Ready
        );
        assert_eq!(
            finite_with_duration.probed_duration.as_deref(),
            Some("00:42:00")
        );
        assert_eq!(
            finite_with_duration.effective_duration.as_deref(),
            Some("00:42:00")
        );

        sqlx::query(r#"UPDATE "RemoteStream" SET "Liveness" = 0 WHERE "Id" = 20"#)
            .execute(&pool)
            .await
            .expect("mark liveness unknown");
        let unknown = repository
            .get_remote_stream_detail_admin(20)
            .await
            .expect("load unknown detail")
            .expect("unknown detail");
        assert_eq!(
            unknown.playback_health.status,
            ersatzrs_core::repositories::AdminRemoteStreamPlaybackHealthStatus::NeedsReview
        );
        assert_eq!(
            unknown.playback_health.reasons,
            [AdminRemoteStreamPlaybackHealthReason::LivenessUnknown]
        );
    }

    #[tokio::test]
    async fn remote_stream_health_rejects_liveness_mirror_drift_and_malformed_duration() {
        let pool = test_pool().await;
        seed_remote_stream_detail(&pool).await;
        let repository = SqliteAdminMediaRepository(pool.clone());

        sqlx::query(r#"UPDATE "RemoteStream" SET "IsLive" = 0 WHERE "Id" = 20"#)
            .execute(&pool)
            .await
            .expect("drift legacy liveness mirror");
        let drifted = repository
            .get_remote_stream_detail_admin(20)
            .await
            .expect("load mirror-drift detail")
            .expect("mirror-drift detail");
        assert!(drifted
            .playback_health
            .reasons
            .contains(&AdminRemoteStreamPlaybackHealthReason::SourceInvalid));

        sqlx::query(
            r#"UPDATE "RemoteStream"
               SET "IsLive" = 1, "Duration" = 'not-a-duration'
               WHERE "Id" = 20"#,
        )
        .execute(&pool)
        .await
        .expect("malform configured duration");
        let malformed = repository
            .get_remote_stream_detail_admin(20)
            .await
            .expect("load malformed-duration detail")
            .expect("malformed-duration detail");
        assert!(malformed
            .playback_health
            .reasons
            .contains(&AdminRemoteStreamPlaybackHealthReason::SourceInvalid));
        assert_eq!(malformed.effective_duration, None);
    }

    #[tokio::test]
    async fn remote_stream_replacement_supplies_health_duration_and_channel_membership() {
        let pool = test_pool().await;
        seed_remote_stream_detail(&pool).await;
        for statement in [
            r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State")
               VALUES (50, 12, 0)"#,
            r#"INSERT INTO "Movie" ("Id") VALUES (50)"#,
            r#"INSERT INTO "MediaVersion" (
                   "Id", "DateAdded", "DateUpdated", "Duration", "Height", "MovieId",
                   "Name", "VideoScanKind", "Width"
               ) VALUES (51, '2026-09-04T12:00:00Z', '2026-09-04T12:00:00Z',
                         '00:25:00', 1080, 50, 'Replacement', 0, 1920)"#,
            r#"INSERT INTO "MediaFile" ("MediaVersionId", "Path", "PathHash")
               VALUES (51, '/managed/replacement.mkv', 'replacement-hash')"#,
            r#"INSERT INTO "MediaStream" (
                   "Index", "Codec", "MediaStreamKind", "Language", "Channels", "Title",
                   "Default", "Forced", "MediaVersionId", "AttachedPic"
               ) VALUES (0, 'h264', 1, NULL, 0, 'Video', 1, 0, 51, 0)"#,
            r#"INSERT INTO "AddonMediaListItemReplacement" (
                   "AddonMediaListItemId", "MediaItemId", "MediaVersionId", "CreatedAt",
                   "UpdatedAt"
               ) VALUES (33, 50, 51, '2026-09-04T12:00:00Z',
                         '2026-09-04T12:00:00Z')"#,
            r#"UPDATE "RemoteStream"
               SET "Availability" = 1, "Liveness" = 1, "IsLive" = 0
               WHERE "Id" = 20"#,
            r#"UPDATE "MediaItem" SET "State" = 2 WHERE "Id" = 20"#,
            r#"UPDATE "PlayoutItem"
               SET "MediaItemId" = 50, "MediaVersionId" = 999
               WHERE "PlayoutId" = 40"#,
        ] {
            sqlx::query(statement)
                .execute(&pool)
                .await
                .expect("seed Local replacement");
        }
        let repository = SqliteAdminMediaRepository(pool.clone());

        let wrong_pin = repository
            .get_remote_stream_detail_admin(20)
            .await
            .expect("load replacement detail")
            .expect("replacement detail");
        assert_eq!(
            wrong_pin.playback_health.status,
            ersatzrs_core::repositories::AdminRemoteStreamPlaybackHealthStatus::Ready
        );
        assert_eq!(wrong_pin.effective_duration.as_deref(), Some("00:25:00"));
        assert!(wrong_pin.channels.is_empty());

        sqlx::query(
            r#"UPDATE "PlayoutItem" SET "MediaVersionId" = 51
               WHERE "Id" = 41"#,
        )
        .execute(&pool)
        .await
        .expect("pin assigned replacement version");
        let pinned = repository
            .get_remote_stream_detail_admin(20)
            .await
            .expect("load pinned replacement detail")
            .expect("pinned replacement detail");
        assert_eq!(pinned.channels.len(), 1);
        assert_eq!(pinned.channels[0].channel_id, 39);
    }
}
