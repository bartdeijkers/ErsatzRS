#[cfg(feature = "ssr")]
use super::super::{
    expect_context, list_media_cards_from_application, localize_episode_display_text,
    media_card_from_admin, season_name, short_date, ActorCardRow, AudioTrackRow,
    CommentarySubtitleRow, EpisodeCardRow, MediaCardRow, MediaItemInfoChapterJson,
    MediaItemInfoJson, MediaItemInfoStreamJson, MovieDetailRow, MoviePlaybackSourceRow,
    MoviePlaybackStreamRow, SeasonCardRow, SeasonDetailRow, ServerFnError, ShowDetailRow,
};
#[cfg(all(test, feature = "ssr"))]
use super::super::{
    load_extra_cards, load_movie_alternate_version_cards, load_movie_audio_tracks,
    load_movie_commentary_subtitles, SubtitleInfoRow,
};

// ---------------------------------------------------------------------------
// Movies
// ---------------------------------------------------------------------------

/// Paginated movie list. Results ordered by `SortTitle`.
///
/// Note: `ArtworkKind = 0` is `Poster` in ErsatzTV's enum (see
/// `ErsatzTV.Core/Domain/ArtworkKind.cs`); the previous version of this query
/// used `1` (`Thumbnail`) by mistake and so never returned poster art for
/// movies stored with kind `Poster`.
#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn list_movies_impl(
    search: String,
    page: u32,
    page_size: u32,
    show_variants: bool,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    list_media_cards_from_application(
        &state,
        "movie",
        ersatzrs_core::repositories::AdminSearchKind::Movie,
        &search,
        page,
        page_size,
        show_variants,
    )
    .await
}

/// Movie details for the `/media/movies/:id` page. Ports
/// `ErsatzTV/Pages/Movie.razor` + `Application.Movies/Queries/GetMovieById`.
#[cfg(feature = "ssr")]
#[allow(clippy::too_many_lines)]
pub(in crate::admin::server_fns::media_browser) async fn get_movie_details_impl(
    id: i64,
) -> Result<Option<MovieDetailRow>, ServerFnError> {
    use crate::api::artwork::{artwork_reference_url as artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
        .application
        .get_movie_detail(id)
        .await?
        .map(|movie| MovieDetailRow {
            id: movie.id,
            title: movie.title,
            year: movie.year,
            plot: movie.plot,
            tagline: movie.tagline,
            content_ratings: movie.content_ratings,
            release_date: movie.release_date,
            poster_url: movie
                .poster_path
                .and_then(|path| artwork_url(&path, ArtworkUrlKind::Poster)),
            fanart_url: movie
                .fanart_path
                .and_then(|path| artwork_url(&path, ArtworkUrlKind::FanArt)),
            file_path: movie.file_path,
            local_path: movie.local_path,
            media_item_state: movie.media_item_state,
            genres: movie.genres,
            tags: movie.tags,
            languages: movie.facets.languages,
            studios: movie.studios,
            music_labels: movie.facets.music_labels,
            directors: movie.directors,
            writers: movie.writers,
            actors: movie
                .actors
                .into_iter()
                .map(|actor| ActorCardRow {
                    id: actor.id,
                    name: actor.name,
                    role: actor.role,
                    thumb_url: actor
                        .artwork_path
                        .and_then(|path| artwork_url(&path, ArtworkUrlKind::Thumbnail)),
                })
                .collect(),
            extras: movie
                .extras
                .into_iter()
                .map(media_card_from_admin)
                .collect(),
            alternate_versions: movie
                .alternate_versions
                .into_iter()
                .map(media_card_from_admin)
                .collect(),
            audio_tracks: movie
                .audio_tracks
                .into_iter()
                .map(|track| AudioTrackRow {
                    media_version_id: track.media_version_id,
                    stream_index: track.stream_index,
                    title: track.title,
                    language: track.language,
                    codec: track.codec,
                    channels: track.channels,
                    is_default: track.is_default,
                    is_commentary: track.is_commentary,
                })
                .collect(),
            commentary_subtitles: movie
                .commentary_subtitles
                .into_iter()
                .map(|subtitle| CommentarySubtitleRow {
                    id: subtitle.id,
                    stream_index: subtitle.stream_index,
                    title: subtitle.title,
                    language: subtitle.language,
                    codec: subtitle.codec,
                })
                .collect(),
            playback_sources: movie
                .playback_sources
                .into_iter()
                .map(|source| MoviePlaybackSourceRow {
                    media_version_id: source.media_version_id,
                    name: source.name,
                    width: source.width,
                    height: source.height,
                    streams: source
                        .streams
                        .into_iter()
                        .map(|stream| MoviePlaybackStreamRow {
                            index: stream.index,
                            kind: stream.kind,
                            title: stream.title,
                            language: stream.language,
                            codec: stream.codec,
                            channels: stream.channels,
                            width: stream.width,
                            height: stream.height,
                            is_default: stream.is_default,
                            is_forced: stream.is_forced,
                            is_external: stream.is_external,
                        })
                        .collect(),
                })
                .collect(),
        }))
}

#[cfg(all(test, feature = "ssr"))]
#[allow(clippy::too_many_lines)]
pub async fn get_movie_details_with_pool(
    pool: &ersatzrs_infra::db::DbPool,
    id: i64,
) -> Result<Option<MovieDetailRow>, ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use sqlx::Row;

    let Some(row) = sqlx::query(
        r#"SELECT m."Id" AS "Id",
                  COALESCE(mm."Title", '') AS "Title",
                  mm."Year" AS "Year",
                  mm."Plot" AS "Plot",
                  mm."Tagline" AS "Tagline",
                  mm."ContentRating" AS "ContentRating",
                  mm."ReleaseDate" AS "ReleaseDate",
                  COALESCE(mi."State", 0) AS "State",
                  mi."LibraryPathId" AS "LibraryPathId",
                  mm."Id" AS "MetadataId"
           FROM "Movie" m
           JOIN "MovieMetadata" mm ON mm."MovieId" = m."Id"
           LEFT JOIN "MediaItem" mi ON mi."Id" = m."Id"
           WHERE m."Id" = ?
           LIMIT 1"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };

    let metadata_id: i64 = row.try_get("MetadataId").unwrap_or(0);

    // Poster (kind 0) + FanArt (kind 3).
    let poster_path: Option<String> = sqlx::query_scalar(
        r#"SELECT "Path" FROM "Artwork"
           WHERE "MovieMetadataId" = ? AND "ArtworkKind" = 0 LIMIT 1"#,
    )
    .bind(metadata_id)
    .fetch_optional(pool)
    .await?
    .and_then(|v: Option<String>| v);
    let fanart_path: Option<String> = sqlx::query_scalar(
        r#"SELECT "Path" FROM "Artwork"
           WHERE "MovieMetadataId" = ? AND "ArtworkKind" = 3 LIMIT 1"#,
    )
    .bind(metadata_id)
    .fetch_optional(pool)
    .await?
    .and_then(|v: Option<String>| v);

    // First media file path. Mirrors ETV's `movie.GetHeadVersion().MediaFiles.Head().Path`.
    let file_path: Option<String> = sqlx::query_scalar(
        r#"SELECT mf."Path"
           FROM "MediaFile" mf
           JOIN "MediaVersion" mv ON mv."Id" = mf."MediaVersionId"
           WHERE mv."MovieId" = ?
           ORDER BY mv."Id" LIMIT 1"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .and_then(|v: Option<String>| v);
    let library_path_id: Option<i64> = row.try_get("LibraryPathId").unwrap_or(None);
    let local_path = match (file_path.as_deref(), library_path_id) {
        (Some(path), Some(library_path_id)) => {
            let replacements =
                load_path_replacements_for_library_path(pool, library_path_id).await?;
            Some(resolve_local_path(path, &replacements))
        }
        (Some(path), None) => Some(path.to_owned()),
        (None, _) => None,
    };

    let content_ratings: Vec<String> = row
        .try_get::<Option<String>, _>("ContentRating")
        .unwrap_or(None)
        .map(|cr| {
            cr.split('/')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let genres = load_strings(pool, r#""Genre""#, "MovieMetadataId", metadata_id).await?;
    let tags = load_strings(pool, r#""Tag""#, "MovieMetadataId", metadata_id).await?;
    let studios = load_strings(pool, r#""Studio""#, "MovieMetadataId", metadata_id).await?;
    let music_labels =
        load_strings(pool, r#""MusicLabel""#, "MovieMetadataId", metadata_id).await?;
    let directors = load_strings(pool, r#""Director""#, "MovieMetadataId", metadata_id).await?;
    let writers = load_strings(pool, r#""Writer""#, "MovieMetadataId", metadata_id).await?;
    let languages = load_movie_languages(pool, id).await?;

    // Actors ordered by `Order` then `Id` (see `Movies/Mapper.cs`).
    let actor_rows = sqlx::query(
        r#"SELECT a."Id" AS "Id",
                  COALESCE(a."Name", '') AS "Name",
                  a."Role" AS "Role",
                  art."Path" AS "Thumb"
           FROM "Actor" a
           LEFT JOIN "Artwork" art ON art."Id" = a."ArtworkId"
           WHERE a."MovieMetadataId" = ?
           ORDER BY a."Order", a."Id""#,
    )
    .bind(metadata_id)
    .fetch_all(pool)
    .await?;
    let actors = actor_rows
        .iter()
        .map(|r| ActorCardRow {
            id: r.try_get("Id").unwrap_or(0),
            name: r.try_get("Name").unwrap_or_default(),
            role: r.try_get::<Option<String>, _>("Role").unwrap_or(None),
            thumb_url: r
                .try_get::<Option<String>, _>("Thumb")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Thumbnail)),
        })
        .collect();
    let extras = load_extra_cards(pool, id).await?;
    let alternate_versions = load_movie_alternate_version_cards(pool, id).await?;
    let audio_tracks = load_movie_audio_tracks(pool, id).await?;
    let commentary_subtitles = load_movie_commentary_subtitles(pool, id).await?;

    Ok(Some(MovieDetailRow {
        id: row.try_get("Id").unwrap_or(0),
        title: row.try_get("Title").unwrap_or_default(),
        year: row.try_get::<Option<i64>, _>("Year").unwrap_or(None),
        plot: row.try_get::<Option<String>, _>("Plot").unwrap_or(None),
        tagline: row.try_get::<Option<String>, _>("Tagline").unwrap_or(None),
        content_ratings,
        release_date: row
            .try_get::<Option<String>, _>("ReleaseDate")
            .unwrap_or(None),
        poster_url: poster_path.and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
        fanart_url: fanart_path.and_then(|p| artwork_url(&p, ArtworkUrlKind::FanArt)),
        file_path,
        local_path,
        media_item_state: row.try_get("State").unwrap_or(0),
        genres,
        tags,
        languages,
        studios,
        music_labels,
        directors,
        writers,
        actors,
        extras,
        alternate_versions,
        audio_tracks,
        commentary_subtitles,
        playback_sources: Vec::new(),
    }))
}

/// TV show details for the `/media/tv/shows/:id` page. Ports
/// `ErsatzTV/Pages/TelevisionSeasonList.razor` +
/// `Application.Television/Queries/GetTelevisionShowById` +
/// `Application.MediaCards/Queries/GetTelevisionSeasonCards`.
#[cfg(feature = "ssr")]
#[allow(clippy::too_many_lines)]
pub(in crate::admin::server_fns::media_browser) async fn get_show_details_impl(
    id: i64,
) -> Result<Option<ShowDetailRow>, ServerFnError> {
    use crate::api::artwork::{artwork_reference_url as artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
        .application
        .get_show_detail(id)
        .await?
        .map(|show| ShowDetailRow {
            id: show.id,
            library_id: show.library_id,
            media_source_kind: show.media_source_kind,
            title: show.title,
            year: show.year,
            plot: show.plot,
            content_ratings: show.content_ratings,
            poster_url: show
                .poster_path
                .and_then(|path| artwork_url(&path, ArtworkUrlKind::Poster)),
            fanart_url: show
                .fanart_path
                .and_then(|path| artwork_url(&path, ArtworkUrlKind::FanArt)),
            media_item_state: show.media_item_state,
            genres: show.genres,
            tags: show.tags,
            languages: show.facets.languages,
            studios: show.studios,
            music_labels: show.facets.music_labels,
            networks: show.networks,
            actors: show
                .actors
                .into_iter()
                .map(|actor| ActorCardRow {
                    id: actor.id,
                    name: actor.name,
                    role: actor.role,
                    thumb_url: actor
                        .artwork_path
                        .and_then(|path| artwork_url(&path, ArtworkUrlKind::Thumbnail)),
                })
                .collect(),
            seasons: show
                .seasons
                .into_iter()
                .map(|season| SeasonCardRow {
                    id: season.id,
                    season_number: season.season_number,
                    title: season_name(season.season_number),
                    poster_url: season
                        .poster_path
                        .and_then(|path| artwork_url(&path, ArtworkUrlKind::Poster)),
                })
                .collect(),
            extras: show.extras.into_iter().map(media_card_from_admin).collect(),
        }))
}

#[cfg(all(test, feature = "ssr"))]
#[allow(clippy::too_many_lines)]
pub(in crate::admin::server_fns::media_browser) async fn get_show_details_legacy_sql(
    id: i64,
) -> Result<Option<ShowDetailRow>, ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;
    use sqlx::Row;

    // `Tag.PlexNetworkTypeId` — see `ErsatzTV.Core/Domain/Metadata/Tag.cs`.
    const PLEX_NETWORK_TYPE_ID: &str = "319";

    let state = expect_context::<AppState>();

    let Some(row) = sqlx::query(
        r#"SELECT s."Id" AS "Id",
                  COALESCE(lp."LibraryId", 0) AS "LibraryId",
                  CASE
                      WHEN pl."Id" IS NOT NULL THEN 'Plex'
                      WHEN jl."Id" IS NOT NULL THEN 'Jellyfin'
                      WHEN el."Id" IS NOT NULL THEN 'Emby'
                      ELSE 'Local'
                  END AS "MediaSourceKind",
                  COALESCE(sm."Title", '') AS "Title",
                  sm."Year" AS "Year",
                  sm."Plot" AS "Plot",
                  sm."ContentRating" AS "ContentRating",
                  COALESCE(mi."State", 0) AS "State",
                  sm."Id" AS "MetadataId"
           FROM "Show" s
           JOIN "ShowMetadata" sm ON sm."ShowId" = s."Id"
           LEFT JOIN "MediaItem" mi ON mi."Id" = s."Id"
           LEFT JOIN "LibraryPath" lp ON lp."Id" = mi."LibraryPathId"
           LEFT JOIN "PlexLibrary" pl ON pl."Id" = lp."LibraryId"
           LEFT JOIN "JellyfinLibrary" jl ON jl."Id" = lp."LibraryId"
           LEFT JOIN "EmbyLibrary" el ON el."Id" = lp."LibraryId"
           WHERE s."Id" = ?
           LIMIT 1"#,
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    else {
        return Ok(None);
    };

    let metadata_id: i64 = row.try_get("MetadataId").unwrap_or(0);

    let poster_path: Option<String> = sqlx::query_scalar(
        r#"SELECT "Path" FROM "Artwork"
           WHERE "ShowMetadataId" = ? AND "ArtworkKind" = 0 LIMIT 1"#,
    )
    .bind(metadata_id)
    .fetch_optional(&state.pool)
    .await?
    .and_then(|v: Option<String>| v);
    let fanart_path: Option<String> = sqlx::query_scalar(
        r#"SELECT "Path" FROM "Artwork"
           WHERE "ShowMetadataId" = ? AND "ArtworkKind" = 3 LIMIT 1"#,
    )
    .bind(metadata_id)
    .fetch_optional(&state.pool)
    .await?
    .and_then(|v: Option<String>| v);

    let genres = load_strings(&state.pool, r#""Genre""#, "ShowMetadataId", metadata_id).await?;
    let studios = load_strings(&state.pool, r#""Studio""#, "ShowMetadataId", metadata_id).await?;
    let music_labels = load_strings(
        &state.pool,
        r#""MusicLabel""#,
        "ShowMetadataId",
        metadata_id,
    )
    .await?;
    let languages = load_show_languages(&state.pool, id).await?;

    // Match Television/Mapper.cs and Tag.IsSearchTag: typed labels and
    // collections remain tags; networks and NFO countries have separate fields.
    let tag_rows = sqlx::query(
        r#"SELECT COALESCE("Name", '') AS "Name",
                  COALESCE("ExternalTypeId", '') AS "ExternalTypeId"
           FROM "Tag" WHERE "ShowMetadataId" = ? ORDER BY "Name""#,
    )
    .bind(metadata_id)
    .fetch_all(&state.pool)
    .await?;
    let mut tags: Vec<String> = Vec::new();
    let mut networks: Vec<String> = Vec::new();
    for r in &tag_rows {
        let name: String = r.try_get("Name").unwrap_or_default();
        if name.trim().is_empty() {
            continue;
        }
        let external: String = r.try_get("ExternalTypeId").unwrap_or_default();
        if external == PLEX_NETWORK_TYPE_ID {
            networks.push(name);
        } else if external != "nfo/country" {
            tags.push(name);
        }
    }

    // Content rating is a single "/"-separated field in ErsatzTV.
    let content_ratings: Vec<String> = row
        .try_get::<Option<String>, _>("ContentRating")
        .unwrap_or(None)
        .map(|cr| {
            cr.split('/')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    // Actors ordered by `Order` then `Id` (see `Television/Mapper.cs`).
    let actor_rows = sqlx::query(
        r#"SELECT a."Id" AS "Id",
                  COALESCE(a."Name", '') AS "Name",
                  a."Role" AS "Role",
                  art."Path" AS "Thumb"
           FROM "Actor" a
           LEFT JOIN "Artwork" art ON art."Id" = a."ArtworkId"
           WHERE a."ShowMetadataId" = ?
           ORDER BY a."Order", a."Id""#,
    )
    .bind(metadata_id)
    .fetch_all(&state.pool)
    .await?;
    let actors = actor_rows
        .iter()
        .map(|r| ActorCardRow {
            id: r.try_get("Id").unwrap_or(0),
            name: r.try_get("Name").unwrap_or_default(),
            role: r.try_get::<Option<String>, _>("Role").unwrap_or(None),
            thumb_url: r
                .try_get::<Option<String>, _>("Thumb")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Thumbnail)),
        })
        .collect();

    // Seasons ordered by season number (see `MediaCards/Mapper.cs`).
    let season_rows = sqlx::query(
        r#"SELECT se."Id" AS "Id",
                  se."SeasonNumber" AS "SeasonNumber",
                  a."Path" AS "Poster"
           FROM "Season" se
           JOIN "SeasonMetadata" sem ON sem."SeasonId" = se."Id"
           LEFT JOIN "Artwork" a
               ON a."SeasonMetadataId" = sem."Id" AND a."ArtworkKind" = 0
           WHERE se."ShowId" = ?
           ORDER BY se."SeasonNumber""#,
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;
    let seasons = season_rows
        .iter()
        .map(|r| {
            let season_number: i64 = r.try_get("SeasonNumber").unwrap_or(0);
            SeasonCardRow {
                id: r.try_get("Id").unwrap_or(0),
                season_number,
                title: if season_number == 0 {
                    "Specials".to_string()
                } else {
                    format!("Season {season_number}")
                },
                poster_url: r
                    .try_get::<Option<String>, _>("Poster")
                    .unwrap_or(None)
                    .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
            }
        })
        .collect();
    let extras = load_extra_cards(&state.pool, id).await?;

    Ok(Some(ShowDetailRow {
        id: row.try_get("Id").unwrap_or(0),
        library_id: row.try_get("LibraryId").unwrap_or(0),
        media_source_kind: row.try_get("MediaSourceKind").unwrap_or_default(),
        title: row.try_get("Title").unwrap_or_default(),
        year: row.try_get::<Option<i64>, _>("Year").unwrap_or(None),
        plot: row.try_get::<Option<String>, _>("Plot").unwrap_or(None),
        content_ratings,
        poster_url: poster_path.and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
        fanart_url: fanart_path.and_then(|p| artwork_url(&p, ArtworkUrlKind::FanArt)),
        media_item_state: row.try_get("State").unwrap_or(0),
        genres,
        tags,
        languages,
        studios,
        music_labels,
        networks,
        actors,
        seasons,
        extras,
    }))
}

/// TV season details for the `/media/tv/seasons/:id` page. Ports
/// `ErsatzTV/Pages/TelevisionEpisodeList.razor` +
/// `Application.Television/Queries/GetTelevisionSeasonById` +
/// `Application.MediaCards/Queries/GetTelevisionEpisodeCards`.
#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn get_season_details_impl(
    id: i64,
) -> Result<Option<SeasonDetailRow>, ServerFnError> {
    use crate::api::artwork::{artwork_reference_url as artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let Some(detail) = state.application.get_season_detail(id).await? else {
        return Ok(None);
    };
    let season = detail.season;

    let season_number = i64::from(season.season_number);
    Ok(Some(SeasonDetailRow {
        id: season.id,
        show_id: season.show_id,
        show_title: season.show_title,
        show_year: season.show_year,
        name: season_name(season_number),
        season_number,
        poster_url: season
            .season_poster_path
            .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
        fanart_url: season
            .show_fanart_path
            .and_then(|p| artwork_url(&p, ArtworkUrlKind::FanArt)),
        episode_count: detail.episode_count,
        episodes: detail
            .episodes
            .into_iter()
            .map(|episode| EpisodeCardRow {
                episode_id: episode.episode_id,
                aired: short_date(episode.release_date.as_deref()),
                show_title: episode.show_title,
                show_id: episode.show_id,
                season_id: episode.season_id,
                season_number: i64::from(episode.season_number),
                episode_number: i64::from(episode.episode_number),
                title: episode.title,
                sort_title: episode.sort_title,
                plot: episode.plot,
                thumbnail_url: episode
                    .thumbnail_path
                    .and_then(|p| artwork_url(&p, ArtworkUrlKind::Thumbnail)),
                directors: episode.directors,
                writers: episode.writers,
                media_item_state: episode.state,
                file_path: episode.file_path,
                local_path: episode.local_path,
            })
            .collect(),
        extras: detail
            .extras
            .into_iter()
            .map(media_card_from_admin)
            .collect(),
    }))
}

/// Media-info JSON for `MediaItemInfoDialog`. Ports the movie branch of
/// `ErsatzTV.Application.MediaItems.Queries.GetMediaItemInfoHandler` and the
/// serialization options used by `Shared/MediaItemInfoDialog.razor`
/// (`JsonStringEnumConverter`, null fields omitted, indented JSON).
#[cfg(feature = "ssr")]
#[allow(clippy::too_many_lines)]
pub(in crate::admin::server_fns::media_browser) async fn get_media_item_info_json_impl(
    id: i64,
) -> Result<String, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let Some(info) = state.application.get_media_item_info(id).await? else {
        return Err(ServerFnError::new("Unable to locate media item"));
    };
    let language = crate::admin::server_fns::settings::load_ui_language_for_api(
        state.application.ports().admin_settings(),
    )
    .await?;
    media_item_info_json_from_admin_with_language(info, language)
}

#[cfg(feature = "ssr")]
pub fn media_item_info_json_from_admin(
    info: ersatzrs_core::repositories::AdminMediaItemInfo,
) -> Result<String, ServerFnError> {
    media_item_info_json_from_admin_with_language(info, crate::admin::i18n::use_i18n())
}

#[cfg(feature = "ssr")]
fn media_item_info_json_from_admin_with_language(
    info: ersatzrs_core::repositories::AdminMediaItemInfo,
    language: crate::admin::i18n::UiLanguage,
) -> Result<String, ServerFnError> {
    use ersatzrs_core::repositories::AdminMediaInfoKind;

    let title = if info.kind == AdminMediaInfoKind::Episode {
        localize_episode_display_text(language, &info.title).unwrap_or(info.title)
    } else {
        info.title
    };
    let kind = match info.kind {
        AdminMediaInfoKind::Movie => "Movie",
        AdminMediaInfoKind::OtherVideo => "OtherVideo",
        AdminMediaInfoKind::Episode => "Episode",
        AdminMediaInfoKind::MusicVideo => "MusicVideo",
        AdminMediaInfoKind::Song => "Song",
        AdminMediaInfoKind::Image => "Image",
        AdminMediaInfoKind::RemoteStream => "RemoteStream",
    };
    let json = MediaItemInfoJson {
        id: info.id,
        title,
        kind: kind.to_owned(),
        library_kind: info.library_kind,
        server_name: info.server_name,
        library_name: info.library_name,
        state: media_item_state_name(i64::from(info.state)).to_owned(),
        duration: info.duration,
        sample_aspect_ratio: info.sample_aspect_ratio,
        display_aspect_ratio: info.display_aspect_ratio,
        r_frame_rate: info.r_frame_rate,
        video_scan_kind: video_scan_kind_name(i64::from(info.video_scan_kind)).to_owned(),
        interlaced_ratio: info.interlaced_ratio,
        width: info.width,
        height: info.height,
        streams: info
            .streams
            .into_iter()
            .map(|stream| MediaItemInfoStreamJson {
                index: stream.index,
                kind: if stream.external_subtitle {
                    "ExternalSubtitle".to_owned()
                } else {
                    media_stream_kind_name(i64::from(stream.kind)).to_owned()
                },
                title: stream.title,
                codec: stream.codec,
                profile: stream.profile,
                language: stream.language,
                channels: stream.channels,
                default: stream.is_default,
                forced: stream.is_forced,
                attached_pic: stream.attached_pic,
                pixel_format: stream.pixel_format,
                color_range: stream.color_range,
                color_space: stream.color_space,
                color_transfer: stream.color_transfer,
                color_primaries: stream.color_primaries,
                bits_per_raw_sample: stream.bits_per_raw_sample,
                mime_type: stream.mime_type,
                file_name: stream.file_name,
                is_extracted: stream.is_extracted,
            })
            .collect(),
        chapters: info
            .chapters
            .into_iter()
            .map(|chapter| MediaItemInfoChapterJson {
                title: chapter.title,
                start_time: chapter.start_time,
                end_time: chapter.end_time,
            })
            .collect(),
    };
    serde_json::to_string_pretty(&json).map_err(ServerFnError::new)
}

#[cfg(all(test, feature = "ssr"))]
#[allow(clippy::too_many_lines)]
pub async fn get_media_item_info_json_with_pool(
    pool: &ersatzrs_infra::db::DbPool,
    id: i64,
) -> Result<String, ServerFnError> {
    use ersatzrs_core::repositories::TelevisionRepository;
    use ersatzrs_infra::repositories::SqliteTelevisionRepository;
    use sqlx::Row;

    let movie_row = sqlx::query(
        r#"SELECT m."Id" AS "Id",
                  COALESCE(mm."Title", '') AS "Title",
                  mm."Id" AS "MetadataId",
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
           FROM "Movie" m
           JOIN "MovieMetadata" mm ON mm."MovieId" = m."Id"
           LEFT JOIN "MediaItem" mi ON mi."Id" = m."Id"
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
           LEFT JOIN "MediaVersion" mv ON mv."MovieId" = m."Id"
           WHERE mi."Id" = ?
           ORDER BY mv."Id"
           LIMIT 1"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    if let Some(row) = movie_row {
        let metadata_id: i64 = row.try_get("MetadataId").unwrap_or(0);
        let version_id: Option<i64> = row.try_get("MediaVersionId").unwrap_or(None);
        let is_local_library =
            row.try_get::<String, _>("LibraryKind").unwrap_or_default() == "LocalLibrary";

        let subtitles = load_movie_subtitle_info(pool, metadata_id).await?;
        let streams = if let Some(version_id) = version_id {
            load_media_info_streams(pool, version_id, &subtitles, is_local_library).await?
        } else {
            Vec::new()
        };
        let chapters = if let Some(version_id) = version_id {
            load_media_info_chapters(pool, version_id).await?
        } else {
            Vec::new()
        };

        let info = MediaItemInfoJson {
            id: row.try_get("Id").unwrap_or(0),
            title: row.try_get("Title").unwrap_or_default(),
            kind: "Movie".to_string(),
            library_kind: row.try_get("LibraryKind").unwrap_or_default(),
            server_name: row
                .try_get::<Option<String>, _>("ServerName")
                .unwrap_or(None),
            library_name: row.try_get("LibraryName").unwrap_or_default(),
            state: media_item_state_name(row.try_get("State").unwrap_or(0)).to_string(),
            duration: row
                .try_get::<Option<String>, _>("Duration")
                .unwrap_or(None)
                .unwrap_or_else(|| "00:00:00".to_string()),
            sample_aspect_ratio: row
                .try_get::<Option<String>, _>("SampleAspectRatio")
                .unwrap_or(None),
            display_aspect_ratio: row
                .try_get::<Option<String>, _>("DisplayAspectRatio")
                .unwrap_or(None),
            r_frame_rate: row
                .try_get::<Option<String>, _>("RFrameRate")
                .unwrap_or(None),
            video_scan_kind: video_scan_kind_name(row.try_get("VideoScanKind").unwrap_or(0))
                .to_string(),
            interlaced_ratio: row
                .try_get::<Option<f64>, _>("InterlacedRatio")
                .unwrap_or(None),
            width: row.try_get("Width").unwrap_or(0),
            height: row.try_get("Height").unwrap_or(0),
            streams,
            chapters,
        };

        return serde_json::to_string_pretty(&info).map_err(ServerFnError::new);
    }

    let other_video_row = sqlx::query(
        r#"SELECT ov."Id" AS "Id",
                  COALESCE(ovm."Title", '') AS "Title",
                  ovm."Id" AS "MetadataId",
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
           FROM "OtherVideo" ov
           JOIN "OtherVideoMetadata" ovm ON ovm."OtherVideoId" = ov."Id"
           LEFT JOIN "MediaItem" mi ON mi."Id" = ov."Id"
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
           LEFT JOIN "MediaVersion" mv ON mv."OtherVideoId" = ov."Id"
           WHERE mi."Id" = ?
           ORDER BY mv."Id"
           LIMIT 1"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    if let Some(row) = other_video_row {
        let metadata_id: i64 = row.try_get("MetadataId").unwrap_or(0);
        let version_id: Option<i64> = row.try_get("MediaVersionId").unwrap_or(None);
        let is_local_library =
            row.try_get::<String, _>("LibraryKind").unwrap_or_default() == "LocalLibrary";

        let subtitles = load_other_video_subtitle_info(pool, metadata_id).await?;
        let streams = if let Some(version_id) = version_id {
            load_media_info_streams(pool, version_id, &subtitles, is_local_library).await?
        } else {
            Vec::new()
        };
        let chapters = if let Some(version_id) = version_id {
            load_media_info_chapters(pool, version_id).await?
        } else {
            Vec::new()
        };

        let info = MediaItemInfoJson {
            id: row.try_get("Id").unwrap_or(0),
            title: row.try_get("Title").unwrap_or_default(),
            kind: "OtherVideo".to_string(),
            library_kind: row.try_get("LibraryKind").unwrap_or_default(),
            server_name: row
                .try_get::<Option<String>, _>("ServerName")
                .unwrap_or(None),
            library_name: row.try_get("LibraryName").unwrap_or_default(),
            state: media_item_state_name(row.try_get("State").unwrap_or(0)).to_string(),
            duration: row
                .try_get::<Option<String>, _>("Duration")
                .unwrap_or(None)
                .unwrap_or_else(|| "00:00:00".to_string()),
            sample_aspect_ratio: row
                .try_get::<Option<String>, _>("SampleAspectRatio")
                .unwrap_or(None),
            display_aspect_ratio: row
                .try_get::<Option<String>, _>("DisplayAspectRatio")
                .unwrap_or(None),
            r_frame_rate: row
                .try_get::<Option<String>, _>("RFrameRate")
                .unwrap_or(None),
            video_scan_kind: video_scan_kind_name(row.try_get("VideoScanKind").unwrap_or(0))
                .to_string(),
            interlaced_ratio: row
                .try_get::<Option<f64>, _>("InterlacedRatio")
                .unwrap_or(None),
            width: row.try_get("Width").unwrap_or(0),
            height: row.try_get("Height").unwrap_or(0),
            streams,
            chapters,
        };

        return serde_json::to_string_pretty(&info).map_err(ServerFnError::new);
    }

    let repo = SqliteTelevisionRepository(pool.clone());
    if let Some(episode) = repo.get_episode_media_item_info(id).await? {
        let subtitles = load_episode_subtitle_info(pool, episode.metadata_id).await?;
        let streams = if let Some(version_id) = episode.media_version_id {
            load_media_info_streams(pool, version_id, &subtitles, episode.is_local_library).await?
        } else {
            Vec::new()
        };
        let chapters = if let Some(version_id) = episode.media_version_id {
            load_media_info_chapters(pool, version_id).await?
        } else {
            Vec::new()
        };
        let raw_title = format!(
            "{} - s{:02}e{:02} - {}",
            episode.show_title, episode.season_number, episode.episode_number, episode.title
        );
        let title =
            localize_episode_display_text(crate::admin::i18n::UiLanguage::English, &raw_title)
                .unwrap_or(raw_title);
        let info = MediaItemInfoJson {
            id: episode.id,
            title,
            kind: "Episode".to_string(),
            library_kind: episode.library_kind,
            server_name: episode.server_name,
            library_name: episode.library_name,
            state: media_item_state_name(i64::from(episode.state)).to_string(),
            duration: episode.duration.unwrap_or_else(|| "00:00:00".to_string()),
            sample_aspect_ratio: episode.sample_aspect_ratio,
            display_aspect_ratio: episode.display_aspect_ratio,
            r_frame_rate: episode.r_frame_rate,
            video_scan_kind: video_scan_kind_name(i64::from(episode.video_scan_kind)).to_string(),
            interlaced_ratio: episode.interlaced_ratio,
            width: episode.width,
            height: episode.height,
            streams,
            chapters,
        };

        return serde_json::to_string_pretty(&info).map_err(ServerFnError::new);
    }

    Err(ServerFnError::new("Unable to locate media item"))
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn load_show_languages(
    pool: &ersatzrs_infra::db::DbPool,
    show_id: i64,
) -> Result<Vec<String>, ServerFnError> {
    // Mirrors `ErsatzTV.Infrastructure/Data/Repositories/SearchRepository.cs`
    // `GetLanguagesForShow`; `MediaStreamKind = 2` is the ErsatzTV persisted
    // enum value for audio streams.
    let codes: Vec<Option<String>> = sqlx::query_scalar(
        r#"SELECT DISTINCT ms."Language"
           FROM "MediaStream" ms
           JOIN "MediaVersion" mv ON mv."Id" = ms."MediaVersionId"
           JOIN "Episode" e ON e."Id" = mv."EpisodeId"
           JOIN "Season" s ON s."Id" = e."SeasonId"
           WHERE ms."MediaStreamKind" = 2 AND s."ShowId" = ?"#,
    )
    .bind(show_id)
    .fetch_all(pool)
    .await?;

    let media_codes = codes
        .into_iter()
        .flatten()
        .filter(|code| !code.trim().is_empty());

    let mut languages: Vec<String> = ersatzrs_core::language_codes::all_language_codes(media_codes)
        .into_iter()
        .filter_map(|code| ersatzrs_core::language_codes::english_name(&code))
        .filter(|name| !name.trim().is_empty())
        .collect();
    languages.sort();
    languages.dedup();
    Ok(languages)
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn load_movie_languages(
    pool: &ersatzrs_infra::db::DbPool,
    movie_id: i64,
) -> Result<Vec<String>, ServerFnError> {
    // Mirrors `GetMovieByIdHandler.cs`: inspect the movie head version's
    // audio streams, then expand any ISO-639 aliases to English names.
    let codes: Vec<Option<String>> = sqlx::query_scalar(
        r#"SELECT ms."Language"
           FROM "MediaStream" ms
           JOIN "MediaVersion" mv ON mv."Id" = ms."MediaVersionId"
           WHERE ms."MediaStreamKind" = 2
             AND mv."MovieId" = ?
             AND mv."Id" = (
                 SELECT "Id"
                 FROM "MediaVersion"
                 WHERE "MovieId" = ?
                 ORDER BY "Id"
                 LIMIT 1
             )
           ORDER BY ms."Index", ms."Id""#,
    )
    .bind(movie_id)
    .bind(movie_id)
    .fetch_all(pool)
    .await?;

    let media_codes = codes
        .into_iter()
        .flatten()
        .filter(|code| !code.trim().is_empty());

    let mut languages: Vec<String> = ersatzrs_core::language_codes::all_language_codes(media_codes)
        .into_iter()
        .filter_map(|code| ersatzrs_core::language_codes::english_name(&code))
        .filter(|name| !name.trim().is_empty())
        .collect();
    languages.sort();
    languages.dedup();
    Ok(languages)
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn load_other_video_languages(
    pool: &ersatzrs_infra::db::DbPool,
    other_video_id: i64,
) -> Result<Vec<String>, ServerFnError> {
    let codes: Vec<Option<String>> = sqlx::query_scalar(
        r#"SELECT ms."Language"
           FROM "MediaStream" ms
           JOIN "MediaVersion" mv ON mv."Id" = ms."MediaVersionId"
           WHERE ms."MediaStreamKind" = 2
             AND mv."OtherVideoId" = ?
             AND mv."Id" = (
                 SELECT "Id"
                 FROM "MediaVersion"
                 WHERE "OtherVideoId" = ?
                 ORDER BY "Id"
                 LIMIT 1
             )
           ORDER BY ms."Index", ms."Id""#,
    )
    .bind(other_video_id)
    .bind(other_video_id)
    .fetch_all(pool)
    .await?;

    let media_codes = codes
        .into_iter()
        .flatten()
        .filter(|code| !code.trim().is_empty());

    let mut languages: Vec<String> = ersatzrs_core::language_codes::all_language_codes(media_codes)
        .into_iter()
        .filter_map(|code| ersatzrs_core::language_codes::english_name(&code))
        .filter(|name| !name.trim().is_empty())
        .collect();
    languages.sort();
    languages.dedup();
    Ok(languages)
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn load_artist_languages(
    pool: &ersatzrs_infra::db::DbPool,
    artist_id: i64,
) -> Result<Vec<String>, ServerFnError> {
    // Mirrors `SearchRepository.GetLanguagesForArtist`; `MediaStreamKind = 2`
    // is the ErsatzTV persisted enum value for audio streams.
    let codes: Vec<Option<String>> = sqlx::query_scalar(
        r#"SELECT DISTINCT ms."Language"
           FROM "MediaStream" ms
           JOIN "MediaVersion" mv ON mv."Id" = ms."MediaVersionId"
           JOIN "MusicVideo" mv_item ON mv_item."Id" = mv."MusicVideoId"
           WHERE ms."MediaStreamKind" = 2
             AND mv_item."ArtistId" = ?"#,
    )
    .bind(artist_id)
    .fetch_all(pool)
    .await?;

    let media_codes = codes
        .into_iter()
        .flatten()
        .filter(|code| !code.trim().is_empty());

    let mut languages: Vec<String> = ersatzrs_core::language_codes::all_language_codes(media_codes)
        .into_iter()
        .filter_map(|code| ersatzrs_core::language_codes::english_name(&code))
        .filter(|name| !name.trim().is_empty())
        .collect();
    languages.sort();
    languages.dedup();
    Ok(languages)
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn load_path_replacements_for_library_path(
    pool: &ersatzrs_infra::db::DbPool,
    library_path_id: i64,
) -> Result<Vec<ersatzrs_core::path_replacement::PathReplacement>, ServerFnError> {
    use ersatzrs_core::path_replacement::PathReplacement;
    use sqlx::Row;

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
    .await?;

    let mut replacements = Vec::new();
    for row in rows {
        let source_path: Option<String> = row.try_get("Source")?;
        let replacement_path: Option<String> = row.try_get("Local")?;
        let platform: Option<String> = row.try_get("Platform")?;
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

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) fn resolve_local_path(
    path: &str,
    replacements: &[ersatzrs_core::path_replacement::PathReplacement],
) -> String {
    if replacements.is_empty() {
        path.to_owned()
    } else {
        ersatzrs_core::path_replacement::replace_path(replacements, path, target_platform())
    }
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) fn source_platform(
    source_path: &str,
    platform: Option<&str>,
) -> ersatzrs_core::path_replacement::PathPlatform {
    use ersatzrs_core::path_replacement::PathPlatform;

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

#[cfg(all(test, feature = "ssr", target_os = "windows"))]
pub(in crate::admin::server_fns::media_browser) fn target_platform(
) -> ersatzrs_core::path_replacement::PathPlatform {
    ersatzrs_core::path_replacement::PathPlatform::Windows
}

#[cfg(all(test, feature = "ssr", not(target_os = "windows")))]
pub(in crate::admin::server_fns::media_browser) fn target_platform(
) -> ersatzrs_core::path_replacement::PathPlatform {
    ersatzrs_core::path_replacement::PathPlatform::Unix
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn enqueue_schedule_playout_refresh(
    state: &crate::host::state::AppState,
    schedule_id: i64,
) -> Result<(), ServerFnError> {
    let playout_ids = playout_ids_using_schedule(&state.pool, schedule_id).await?;
    enqueue_playout_refreshes(state, playout_ids);
    Ok(())
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn playout_ids_using_schedule(
    pool: &ersatzrs_infra::db::DbPool,
    schedule_id: i64,
) -> Result<Vec<i64>, sqlx::Error> {
    sqlx::query_scalar(
        r#"SELECT "Id"
           FROM "Playout"
           WHERE "ProgramScheduleId" = ?
           ORDER BY "Id""#,
    )
    .bind(schedule_id)
    .fetch_all(pool)
    .await
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn playout_ids_using_collection(
    pool: &ersatzrs_infra::db::DbPool,
    collection_id: i64,
) -> Result<Vec<i64>, sqlx::Error> {
    // Mirrors `ErsatzTV.Infrastructure/Data/Repositories/
    // MediaCollectionRepository.cs` `PlayoutIdsUsingCollection`.
    sqlx::query_scalar(
        r#"SELECT DISTINCT "PlayoutId"
           FROM "PlayoutProgramScheduleAnchor"
           WHERE "CollectionId" = ?
           ORDER BY "PlayoutId""#,
    )
    .bind(collection_id)
    .fetch_all(pool)
    .await
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) fn enqueue_playout_refreshes(
    state: &crate::host::state::AppState,
    playout_ids: Vec<i64>,
) {
    use ersatzrs_app::commands::{RequestPlayoutBuild, RequestPlayoutBuildOutcome};
    use ersatzrs_core::scheduling::PlayoutBuildMode;

    for playout_id in playout_ids {
        match state
            .application
            .request_playout_build(RequestPlayoutBuild::new(
                playout_id,
                PlayoutBuildMode::Refresh,
            )) {
            RequestPlayoutBuildOutcome::Accepted
            | RequestPlayoutBuildOutcome::AlreadyQueued
            | RequestPlayoutBuildOutcome::AlreadyBuilding => {}
            RequestPlayoutBuildOutcome::Unavailable { message } => {
                tracing::warn!(
                    playout_id,
                    %message,
                    "unable to enqueue requested playout refresh; worker queue is unavailable"
                );
            }
            RequestPlayoutBuildOutcome::Rejected(reason) => {
                tracing::warn!(
                    playout_id,
                    ?reason,
                    "unable to enqueue invalid playout refresh request"
                );
            }
        }
    }
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn load_strings(
    pool: &ersatzrs_infra::db::DbPool,
    table: &str,
    column: &str,
    metadata_id: i64,
) -> Result<Vec<String>, ServerFnError> {
    use sqlx::Row;

    // `table` and `column` are private constants supplied by the call sites
    // in this module; not user input. Identifiers are inlined because the
    // sqlx query cache uses the rendered SQL as the key.
    let tag_filter = if table == r#""Tag""# {
        r#" AND ("ExternalTypeId" IS NULL OR "ExternalTypeId" NOT IN ('319', 'nfo/country'))"#
    } else {
        ""
    };
    let sql =
        format!(r#"SELECT "Name" FROM {table} WHERE "{column}" = ?{tag_filter} ORDER BY "Name""#);
    let rows = sqlx::query(ersatzrs_infra::db::assert_sql_safe(&sql))
        .bind(metadata_id)
        .fetch_all(pool)
        .await?;
    Ok(rows
        .iter()
        .filter_map(|r| r.try_get::<Option<String>, _>("Name").ok().flatten())
        .filter(|name| !name.trim().is_empty())
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn load_movie_subtitle_info(
    pool: &ersatzrs_infra::db::DbPool,
    metadata_id: i64,
) -> Result<Vec<SubtitleInfoRow>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT "StreamIndex",
                  "SubtitleKind",
                  "Title",
                  "Codec",
                  "Language",
                  "Default",
                  "Forced",
                  "Path",
                  "IsExtracted"
           FROM "Subtitle"
           WHERE "MovieMetadataId" = ?
           ORDER BY "StreamIndex", "Id""#,
    )
    .bind(metadata_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| SubtitleInfoRow {
            stream_index: r.try_get("StreamIndex").unwrap_or(0),
            subtitle_kind: r.try_get("SubtitleKind").unwrap_or(0),
            title: r.try_get::<Option<String>, _>("Title").unwrap_or(None),
            codec: r.try_get::<Option<String>, _>("Codec").unwrap_or(None),
            language: r.try_get::<Option<String>, _>("Language").unwrap_or(None),
            default: r.try_get::<i64, _>("Default").unwrap_or(0) != 0,
            forced: r.try_get::<i64, _>("Forced").unwrap_or(0) != 0,
            path: r.try_get::<Option<String>, _>("Path").unwrap_or(None),
            is_extracted: r.try_get::<i64, _>("IsExtracted").unwrap_or(0) != 0,
        })
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn load_other_video_subtitle_info(
    pool: &ersatzrs_infra::db::DbPool,
    metadata_id: i64,
) -> Result<Vec<SubtitleInfoRow>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT "StreamIndex",
                  "SubtitleKind",
                  "Title",
                  "Codec",
                  "Language",
                  "Default",
                  "Forced",
                  "Path",
                  "IsExtracted"
           FROM "Subtitle"
           WHERE "OtherVideoMetadataId" = ?
           ORDER BY "StreamIndex", "Id""#,
    )
    .bind(metadata_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| SubtitleInfoRow {
            stream_index: r.try_get("StreamIndex").unwrap_or(0),
            subtitle_kind: r.try_get("SubtitleKind").unwrap_or(0),
            title: r.try_get::<Option<String>, _>("Title").unwrap_or(None),
            codec: r.try_get::<Option<String>, _>("Codec").unwrap_or(None),
            language: r.try_get::<Option<String>, _>("Language").unwrap_or(None),
            default: r.try_get::<i64, _>("Default").unwrap_or(0) != 0,
            forced: r.try_get::<i64, _>("Forced").unwrap_or(0) != 0,
            path: r.try_get::<Option<String>, _>("Path").unwrap_or(None),
            is_extracted: r.try_get::<i64, _>("IsExtracted").unwrap_or(0) != 0,
        })
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn load_episode_subtitle_info(
    pool: &ersatzrs_infra::db::DbPool,
    metadata_id: i64,
) -> Result<Vec<SubtitleInfoRow>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT "StreamIndex",
                  "SubtitleKind",
                  "Title",
                  "Codec",
                  "Language",
                  "Default",
                  "Forced",
                  "Path",
                  "IsExtracted"
           FROM "Subtitle"
           WHERE "EpisodeMetadataId" = ?
           ORDER BY "StreamIndex", "Id""#,
    )
    .bind(metadata_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| SubtitleInfoRow {
            stream_index: r.try_get("StreamIndex").unwrap_or(0),
            subtitle_kind: r.try_get("SubtitleKind").unwrap_or(0),
            title: r.try_get::<Option<String>, _>("Title").unwrap_or(None),
            codec: r.try_get::<Option<String>, _>("Codec").unwrap_or(None),
            language: r.try_get::<Option<String>, _>("Language").unwrap_or(None),
            default: r.try_get::<i64, _>("Default").unwrap_or(0) != 0,
            forced: r.try_get::<i64, _>("Forced").unwrap_or(0) != 0,
            path: r.try_get::<Option<String>, _>("Path").unwrap_or(None),
            is_extracted: r.try_get::<i64, _>("IsExtracted").unwrap_or(0) != 0,
        })
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
#[allow(clippy::too_many_lines)]
pub(in crate::admin::server_fns::media_browser) async fn load_media_info_streams(
    pool: &ersatzrs_infra::db::DbPool,
    media_version_id: i64,
    subtitles: &[SubtitleInfoRow],
    is_local_library: bool,
) -> Result<Vec<MediaItemInfoStreamJson>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT "Index",
                  "MediaStreamKind",
                  "Title",
                  "Codec",
                  "Profile",
                  "Language",
                  "Channels",
                  "Default",
                  "Forced",
                  "AttachedPic",
                  "PixelFormat",
                  "ColorRange",
                  "ColorSpace",
                  "ColorTransfer",
                  "ColorPrimaries",
                  "BitsPerRawSample",
                  "MimeType",
                  "FileName"
           FROM "MediaStream"
           WHERE "MediaVersionId" = ?
           ORDER BY "Index", "Id""#,
    )
    .bind(media_version_id)
    .fetch_all(pool)
    .await?;

    let mut streams = rows
        .iter()
        .map(|r| {
            let index = r.try_get::<i64, _>("Index").unwrap_or(0);
            let kind_value = r.try_get::<i64, _>("MediaStreamKind").unwrap_or(0);
            let mut stream = MediaItemInfoStreamJson {
                index: Some(index),
                kind: media_stream_kind_name(kind_value).to_string(),
                title: r.try_get::<Option<String>, _>("Title").unwrap_or(None),
                codec: r.try_get::<Option<String>, _>("Codec").unwrap_or(None),
                profile: r.try_get::<Option<String>, _>("Profile").unwrap_or(None),
                language: r.try_get::<Option<String>, _>("Language").unwrap_or(None),
                channels: positive_i64(r.try_get("Channels").unwrap_or(0)),
                default: true_or_none(r.try_get("Default").unwrap_or(0)),
                forced: true_or_none(r.try_get("Forced").unwrap_or(0)),
                attached_pic: true_or_none(r.try_get("AttachedPic").unwrap_or(0)),
                pixel_format: r
                    .try_get::<Option<String>, _>("PixelFormat")
                    .unwrap_or(None),
                color_range: r.try_get::<Option<String>, _>("ColorRange").unwrap_or(None),
                color_space: r.try_get::<Option<String>, _>("ColorSpace").unwrap_or(None),
                color_transfer: r
                    .try_get::<Option<String>, _>("ColorTransfer")
                    .unwrap_or(None),
                color_primaries: r
                    .try_get::<Option<String>, _>("ColorPrimaries")
                    .unwrap_or(None),
                bits_per_raw_sample: positive_i64(r.try_get("BitsPerRawSample").unwrap_or(0)),
                mime_type: r.try_get::<Option<String>, _>("MimeType").unwrap_or(None),
                file_name: r.try_get::<Option<String>, _>("FileName").unwrap_or(None),
                is_extracted: None,
            };

            if stream.kind == "Subtitle" {
                if let Some(subtitle) = subtitles.iter().find(|s| s.stream_index == index) {
                    if !is_image_subtitle(subtitle.codec.as_deref()) {
                        stream.is_extracted = Some(subtitle.is_extracted);
                    }
                    stream.file_name = clean_string(subtitle.path.clone());
                }
            }

            stream
        })
        .collect::<Vec<_>>();

    if is_local_library {
        streams.extend(subtitles.iter().filter(|s| s.subtitle_kind == 1).map(|s| {
            MediaItemInfoStreamJson {
                index: None,
                kind: "ExternalSubtitle".to_string(),
                title: clean_string(s.title.clone()),
                codec: clean_string(s.codec.clone()),
                profile: None,
                language: clean_string(s.language.clone()),
                channels: None,
                default: s.default.then_some(true),
                forced: s.forced.then_some(true),
                attached_pic: None,
                pixel_format: None,
                color_range: None,
                color_space: None,
                color_transfer: None,
                color_primaries: None,
                bits_per_raw_sample: None,
                mime_type: None,
                file_name: s.path.as_ref().and_then(|p| {
                    std::path::Path::new(p)
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                }),
                is_extracted: None,
            }
        }));
    }

    Ok(streams)
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn load_media_info_chapters(
    pool: &ersatzrs_infra::db::DbPool,
    media_version_id: i64,
) -> Result<Vec<MediaItemInfoChapterJson>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT COALESCE("Title", '') AS "Title", "StartTime", "EndTime"
           FROM "MediaChapter"
           WHERE "MediaVersionId" = ?
           ORDER BY "StartTime", "ChapterId""#,
    )
    .bind(media_version_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| MediaItemInfoChapterJson {
            title: r.try_get("Title").unwrap_or_default(),
            start_time: r.try_get("StartTime").unwrap_or_default(),
            end_time: r.try_get("EndTime").unwrap_or_default(),
        })
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) fn clean_string(
    value: Option<String>,
) -> Option<String> {
    value.filter(|s| !s.trim().is_empty())
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) fn true_or_none(value: i64) -> Option<bool> {
    (value != 0).then_some(true)
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) fn positive_i64(value: i64) -> Option<i64> {
    (value > 0).then_some(value)
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) fn media_item_state_name(
    value: i64,
) -> &'static str {
    match value {
        1 => "FileNotFound",
        2 => "Unavailable",
        3 => "RemoteOnly",
        _ => "Normal",
    }
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) fn media_stream_kind_name(
    value: i64,
) -> &'static str {
    match value {
        2 => "Audio",
        3 => "Subtitle",
        4 => "Attachment",
        5 => "ExternalSubtitle",
        // Some current ErsatzRS scanner/provider paths still write zero-based
        // stream kinds. Accept `0` as video when displaying media info, while
        // preserving the ErsatzTV one-based names for canonical data.
        _ => "Video",
    }
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) fn video_scan_kind_name(
    value: i64,
) -> &'static str {
    match value {
        1 => "Progressive",
        2 => "Interlaced",
        _ => "Unknown",
    }
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) fn is_image_subtitle(codec: Option<&str>) -> bool {
    matches!(
        codec,
        Some("hdmv_pgs_subtitle" | "dvd_subtitle" | "dvdsub" | "vobsub" | "pgssub" | "pgs")
    )
}
