//! Server functions for the media browser (movies/shows/images/artists/
//! songs/other videos/trash) and the
//! manual/smart collection editors. Ports:
//!
//! - `Pages/MovieList.razor` → [`list_movies`]
//! - `Pages/TelevisionShowList.razor` → [`list_shows`]
//! - `Pages/ImageList.razor` → [`list_images`]
//! - `Pages/ArtistList.razor` → [`list_artists`]
//! - `Pages/SongList.razor` → [`list_songs`]
//! - `Pages/OtherVideoList.razor` → [`list_other_videos`]
//! - `Pages/Trash.razor` → [`list_trash_items`], [`delete_media_items`],
//!   [`empty_trash`]
//! - `Pages/ManualCollections.razor` + `CollectionEditor.razor` →
//!   [`list_collections`], [`get_collection`], [`save_collection`],
//!   [`delete_collection`]
//! - `Pages/SmartCollections.razor` + `SmartCollectionEditor.razor` →
//!   [`list_smart_collections`], [`get_smart_collection`],
//!   [`save_smart_collection`], [`delete_smart_collection`]
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// Row for a media-card grid (movies/shows/images share this shape).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MediaCardRow {
    pub id: i64,
    pub title: String,
    pub sort_title: String,
    pub subtitle: Option<String>,
    pub artwork_path: Option<String>,
}

/// Page-map for the alphabetic jump-bar. Mirrors `ErsatzTV`'s
/// `SearchPageMap.PageMap`: for each bucket ('#' or 'a'..='z'), the 1-based
/// page number that contains the first item starting with that letter.
///
/// Stored as a 27-element vector indexed by [`bucket_index`] (0 = '#',
/// 1 = 'a', …, 26 = 'z') so the type is `Serialize`/`Deserialize`-friendly
/// for `server_fn`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LetterPageMap {
    pub pages: Vec<u32>,
}

/// Returns the bucket index for a letter: `'#' => 0`, `'a'..='z' => 1..=26`.
/// Out-of-range characters bucket to '#'.
#[must_use]
pub fn bucket_index(letter: char) -> usize {
    let lc = letter.to_ascii_lowercase();
    if lc.is_ascii_lowercase() {
        (lc as usize - 'a' as usize) + 1
    } else {
        0
    }
}

/// Detailed projection for the movie details page. Mirrors the relevant
/// subset of `ErsatzTV.Application.Movies.MovieViewModel`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MovieDetailRow {
    pub id: i64,
    pub title: String,
    pub year: Option<i64>,
    pub plot: Option<String>,
    pub tagline: Option<String>,
    pub content_rating: Option<String>,
    pub release_date: Option<String>,
    pub poster_url: Option<String>,
    pub fanart_url: Option<String>,
    pub file_path: Option<String>,
    /// `MediaItem.State`: 0 = Normal, 1 = `FileNotFound`, 2 = Unavailable.
    pub media_item_state: i32,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub studios: Vec<String>,
    pub directors: Vec<String>,
    pub writers: Vec<String>,
}

/// One season card on the show details page. Mirrors
/// `ErsatzTV.Application.MediaCards.TelevisionSeasonCardViewModel`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SeasonCardRow {
    pub id: i64,
    pub season_number: i64,
    /// `"Specials"` when `season_number == 0`, else `"Season {n}"`.
    pub title: String,
    pub poster_url: Option<String>,
}

/// One cast member on the show details page. Mirrors
/// `ErsatzTV.Application.MediaCards.ActorCardViewModel`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ActorCardRow {
    pub id: i64,
    pub name: String,
    pub role: Option<String>,
    pub thumb_url: Option<String>,
}

/// Detailed projection for the TV show details page. Mirrors the relevant
/// subset of `ErsatzTV.Application.Television.TelevisionShowViewModel`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ShowDetailRow {
    pub id: i64,
    pub library_id: i64,
    /// `Local` | `Plex` | `Jellyfin` | `Emby` — drives the Scan menu and
    /// matches `Television/Mapper.cs`'s `MediaSourceKind` resolution.
    pub media_source_kind: String,
    pub title: String,
    pub year: Option<i64>,
    pub plot: Option<String>,
    pub content_ratings: Vec<String>,
    pub poster_url: Option<String>,
    pub fanart_url: Option<String>,
    /// `MediaItem.State`: 0 = Normal, 1 = `FileNotFound`, 2 = Unavailable.
    pub media_item_state: i32,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub languages: Vec<String>,
    pub studios: Vec<String>,
    pub networks: Vec<String>,
    pub actors: Vec<ActorCardRow>,
    pub seasons: Vec<SeasonCardRow>,
}

#[cfg(feature = "ssr")]
#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
struct MediaItemInfoJson {
    id: i64,
    title: String,
    kind: String,
    library_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    server_name: Option<String>,
    library_name: String,
    state: String,
    duration: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    sample_aspect_ratio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    display_aspect_ratio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    r_frame_rate: Option<String>,
    video_scan_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    interlaced_ratio: Option<f64>,
    width: i64,
    height: i64,
    streams: Vec<MediaItemInfoStreamJson>,
    chapters: Vec<MediaItemInfoChapterJson>,
}

#[cfg(feature = "ssr")]
#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
struct MediaItemInfoStreamJson {
    #[serde(skip_serializing_if = "Option::is_none")]
    index: Option<i64>,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    codec: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    channels: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    default: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    forced: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    attached_pic: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pixel_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    color_range: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    color_space: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    color_transfer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    color_primaries: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bits_per_raw_sample: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_extracted: Option<bool>,
}

#[cfg(feature = "ssr")]
#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
struct MediaItemInfoChapterJson {
    title: String,
    start_time: String,
    end_time: String,
}

#[cfg(feature = "ssr")]
struct SubtitleInfoRow {
    stream_index: i64,
    subtitle_kind: i64,
    title: Option<String>,
    codec: Option<String>,
    language: Option<String>,
    default: bool,
    forced: bool,
    path: Option<String>,
    is_extracted: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CollectionListRow {
    pub id: i64,
    pub name: String,
    /// Only populated for smart collections.
    pub query: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CollectionForm {
    pub id: i64,
    pub name: String,
    /// Only used by smart collections.
    pub query: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TrashPageData {
    pub movies: Vec<MediaCardRow>,
    pub shows: Vec<MediaCardRow>,
    pub artists: Vec<MediaCardRow>,
    pub other_videos: Vec<MediaCardRow>,
    pub songs: Vec<MediaCardRow>,
    pub images: Vec<MediaCardRow>,
}

fn song_subtitle(artists: Option<String>, album: Option<String>) -> Option<String> {
    let artists = artists.unwrap_or_default();
    let album = album.unwrap_or_default();
    match (artists.trim(), album.trim()) {
        ("", "") => None,
        ("", album) => Some(album.to_string()),
        (artists, "") => Some(artists.to_string()),
        (artists, album) => Some(format!("{artists} - {album}")),
    }
}

// ---------------------------------------------------------------------------
// Movies
// ---------------------------------------------------------------------------

/// Paginated movie list. Results ordered by `SortTitle`.
///
/// Note: `ArtworkKind = 0` is `Poster` in ErsatzTV's enum (see
/// `ErsatzTV.Core/Domain/ArtworkKind.cs`); the previous version of this query
/// used `1` (`Thumbnail`) by mistake and so never returned poster art for
/// movies stored with kind `Poster`.
#[server]
pub async fn list_movies(
    search: String,
    page: u32,
    page_size: u32,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let offset = i64::from(page) * i64::from(page_size);
    let like = format!("%{}%", search.trim());

    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "MovieMetadata" mm
           WHERE (? = '' OR mm."Title" LIKE ?)"#,
    )
    .bind(search.trim())
    .bind(&like)
    .fetch_one(&state.pool)
    .await?;

    let rows = sqlx::query(
        r#"SELECT m."Id" AS "Id",
                  COALESCE(mm."Title", '') AS "Title",
                  COALESCE(mm."SortTitle", mm."Title", '') AS "SortTitle",
                  mm."Year" AS "Year",
                  a."Path" AS "Poster"
           FROM "Movie" m
           JOIN "MovieMetadata" mm ON mm."MovieId" = m."Id"
           LEFT JOIN "Artwork" a
               ON a."MovieMetadataId" = mm."Id" AND a."ArtworkKind" = 0
           WHERE (? = '' OR mm."Title" LIKE ?)
           ORDER BY mm."SortTitle", mm."Title"
           LIMIT ? OFFSET ?"#,
    )
    .bind(search.trim())
    .bind(&like)
    .bind(i64::from(page_size))
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    let cards = rows
        .iter()
        .map(|r| MediaCardRow {
            id: r.try_get("Id").unwrap_or(0),
            title: r.try_get("Title").unwrap_or_default(),
            sort_title: r.try_get("SortTitle").unwrap_or_default(),
            subtitle: r
                .try_get::<Option<i64>, _>("Year")
                .unwrap_or(None)
                .map(|y| y.to_string()),
            artwork_path: r
                .try_get::<Option<String>, _>("Poster")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
        })
        .collect();

    #[allow(clippy::cast_sign_loss)]
    Ok((cards, total.max(0) as u64))
}

/// Movie details for the `/media/movies/:id` page. Ports
/// `ErsatzTV/Pages/Movie.razor` + `Application.Movies/Queries/GetMovieById`.
#[server]
pub async fn get_movie_details(id: i64) -> Result<Option<MovieDetailRow>, ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();

    let Some(row) = sqlx::query(
        r#"SELECT m."Id" AS "Id",
                  COALESCE(mm."Title", '') AS "Title",
                  mm."Year" AS "Year",
                  mm."Plot" AS "Plot",
                  mm."Tagline" AS "Tagline",
                  mm."ContentRating" AS "ContentRating",
                  mm."ReleaseDate" AS "ReleaseDate",
                  COALESCE(mi."State", 0) AS "State",
                  mm."Id" AS "MetadataId"
           FROM "Movie" m
           JOIN "MovieMetadata" mm ON mm."MovieId" = m."Id"
           LEFT JOIN "MediaItem" mi ON mi."Id" = m."Id"
           WHERE m."Id" = ?
           LIMIT 1"#,
    )
    .bind(id)
    .fetch_optional(&state.pool)
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
    .fetch_optional(&state.pool)
    .await?
    .and_then(|v: Option<String>| v);
    let fanart_path: Option<String> = sqlx::query_scalar(
        r#"SELECT "Path" FROM "Artwork"
           WHERE "MovieMetadataId" = ? AND "ArtworkKind" = 3 LIMIT 1"#,
    )
    .bind(metadata_id)
    .fetch_optional(&state.pool)
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
    .fetch_optional(&state.pool)
    .await?
    .and_then(|v: Option<String>| v);

    let genres = load_strings(&state.pool, r#""Genre""#, "MovieMetadataId", metadata_id).await?;
    let tags = load_strings(&state.pool, r#""Tag""#, "MovieMetadataId", metadata_id).await?;
    let studios = load_strings(&state.pool, r#""Studio""#, "MovieMetadataId", metadata_id).await?;
    let directors =
        load_strings(&state.pool, r#""Director""#, "MovieMetadataId", metadata_id).await?;
    let writers = load_strings(&state.pool, r#""Writer""#, "MovieMetadataId", metadata_id).await?;

    Ok(Some(MovieDetailRow {
        id: row.try_get("Id").unwrap_or(0),
        title: row.try_get("Title").unwrap_or_default(),
        year: row.try_get::<Option<i64>, _>("Year").unwrap_or(None),
        plot: row.try_get::<Option<String>, _>("Plot").unwrap_or(None),
        tagline: row.try_get::<Option<String>, _>("Tagline").unwrap_or(None),
        content_rating: row
            .try_get::<Option<String>, _>("ContentRating")
            .unwrap_or(None),
        release_date: row
            .try_get::<Option<String>, _>("ReleaseDate")
            .unwrap_or(None),
        poster_url: poster_path.and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
        fanart_url: fanart_path.and_then(|p| artwork_url(&p, ArtworkUrlKind::FanArt)),
        file_path,
        media_item_state: row.try_get("State").unwrap_or(0),
        genres,
        tags,
        studios,
        directors,
        writers,
    }))
}

/// TV show details for the `/media/tv/shows/:id` page. Ports
/// `ErsatzTV/Pages/TelevisionSeasonList.razor` +
/// `Application.Television/Queries/GetTelevisionShowById` +
/// `Application.MediaCards/Queries/GetTelevisionSeasonCards`.
#[server]
#[allow(clippy::too_many_lines)]
pub async fn get_show_details(id: i64) -> Result<Option<ShowDetailRow>, ServerFnError> {
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
    let languages = load_show_languages(&state.pool, id).await?;

    // Tags split into plain tags vs Plex "network" tags, mirroring
    // `Television/Mapper.cs` (`ExternalTypeId` is null/blank for tags,
    // `Tag.PlexNetworkTypeId` for networks).
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
        if external.trim().is_empty() {
            tags.push(name);
        } else if external == PLEX_NETWORK_TYPE_ID {
            networks.push(name);
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
        networks,
        actors,
        seasons,
    }))
}

/// Media-info JSON for `MediaItemInfoDialog`. Ports the movie branch of
/// `ErsatzTV.Application.MediaItems.Queries.GetMediaItemInfoHandler` and the
/// serialization options used by `Shared/MediaItemInfoDialog.razor`
/// (`JsonStringEnumConverter`, null fields omitted, indented JSON).
#[server]
#[allow(clippy::too_many_lines)]
pub async fn get_media_item_info_json(id: i64) -> Result<String, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();

    let Some(row) = sqlx::query(
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
    .fetch_optional(&state.pool)
    .await?
    else {
        return Err(ServerFnError::new("Unable to locate media item"));
    };

    let metadata_id: i64 = row.try_get("MetadataId").unwrap_or(0);
    let version_id: Option<i64> = row.try_get("MediaVersionId").unwrap_or(None);
    let is_local_library =
        row.try_get::<String, _>("LibraryKind").unwrap_or_default() == "LocalLibrary";

    let subtitles = load_movie_subtitle_info(&state.pool, metadata_id).await?;
    let streams = if let Some(version_id) = version_id {
        load_media_info_streams(&state.pool, version_id, &subtitles, is_local_library).await?
    } else {
        Vec::new()
    };
    let chapters = if let Some(version_id) = version_id {
        load_media_info_chapters(&state.pool, version_id).await?
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

    serde_json::to_string_pretty(&info).map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
async fn load_show_languages(
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

#[cfg(feature = "ssr")]
async fn load_strings(
    pool: &ersatzrs_infra::db::DbPool,
    table: &str,
    column: &str,
    metadata_id: i64,
) -> Result<Vec<String>, ServerFnError> {
    use sqlx::Row;

    // `table` and `column` are private constants supplied by the call sites
    // in this module; not user input. Identifiers are inlined because the
    // sqlx query cache uses the rendered SQL as the key.
    let sql = format!(r#"SELECT "Name" FROM {table} WHERE "{column}" = ? ORDER BY "Name""#);
    let rows = sqlx::query(&sql).bind(metadata_id).fetch_all(pool).await?;
    Ok(rows
        .iter()
        .filter_map(|r| r.try_get::<Option<String>, _>("Name").ok().flatten())
        .filter(|name| !name.trim().is_empty())
        .collect())
}

#[cfg(feature = "ssr")]
async fn load_movie_subtitle_info(
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

#[cfg(feature = "ssr")]
#[allow(clippy::too_many_lines)]
async fn load_media_info_streams(
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

#[cfg(feature = "ssr")]
async fn load_media_info_chapters(
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

#[cfg(feature = "ssr")]
fn clean_string(value: Option<String>) -> Option<String> {
    value.filter(|s| !s.trim().is_empty())
}

#[cfg(feature = "ssr")]
fn true_or_none(value: i64) -> Option<bool> {
    (value != 0).then_some(true)
}

#[cfg(feature = "ssr")]
fn positive_i64(value: i64) -> Option<i64> {
    (value > 0).then_some(value)
}

#[cfg(feature = "ssr")]
fn media_item_state_name(value: i64) -> &'static str {
    match value {
        1 => "FileNotFound",
        2 => "Unavailable",
        3 => "RemoteOnly",
        _ => "Normal",
    }
}

#[cfg(feature = "ssr")]
fn media_stream_kind_name(value: i64) -> &'static str {
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
fn video_scan_kind_name(value: i64) -> &'static str {
    match value {
        1 => "Progressive",
        2 => "Interlaced",
        _ => "Unknown",
    }
}

#[cfg(feature = "ssr")]
fn is_image_subtitle(codec: Option<&str>) -> bool {
    matches!(
        codec,
        Some("hdmv_pgs_subtitle" | "dvd_subtitle" | "dvdsub" | "vobsub" | "pgssub" | "pgs")
    )
}

// ---------------------------------------------------------------------------
// Shows
// ---------------------------------------------------------------------------

#[server]
pub async fn list_shows(
    search: String,
    page: u32,
    page_size: u32,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let offset = i64::from(page) * i64::from(page_size);
    let like = format!("%{}%", search.trim());

    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "ShowMetadata" sm
           WHERE (? = '' OR sm."Title" LIKE ?)"#,
    )
    .bind(search.trim())
    .bind(&like)
    .fetch_one(&state.pool)
    .await?;

    let rows = sqlx::query(
        r#"SELECT s."Id" AS "Id",
                  COALESCE(sm."Title", '') AS "Title",
                  COALESCE(sm."SortTitle", sm."Title", '') AS "SortTitle",
                  sm."Year" AS "Year",
                  a."Path" AS "Poster"
           FROM "Show" s
           JOIN "ShowMetadata" sm ON sm."ShowId" = s."Id"
           LEFT JOIN "Artwork" a
               ON a."ShowMetadataId" = sm."Id" AND a."ArtworkKind" = 0
           WHERE (? = '' OR sm."Title" LIKE ?)
           ORDER BY sm."SortTitle", sm."Title"
           LIMIT ? OFFSET ?"#,
    )
    .bind(search.trim())
    .bind(&like)
    .bind(i64::from(page_size))
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    let cards = rows
        .iter()
        .map(|r| MediaCardRow {
            id: r.try_get("Id").unwrap_or(0),
            title: r.try_get("Title").unwrap_or_default(),
            sort_title: r.try_get("SortTitle").unwrap_or_default(),
            subtitle: r
                .try_get::<Option<i64>, _>("Year")
                .unwrap_or(None)
                .map(|y| y.to_string()),
            artwork_path: r
                .try_get::<Option<String>, _>("Poster")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
        })
        .collect();

    #[allow(clippy::cast_sign_loss)]
    Ok((cards, total.max(0) as u64))
}

// ---------------------------------------------------------------------------
// Images
// ---------------------------------------------------------------------------

#[server]
pub async fn list_images(
    search: String,
    page: u32,
    page_size: u32,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let offset = i64::from(page) * i64::from(page_size);
    let like = format!("%{}%", search.trim());

    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "ImageMetadata" im
           WHERE (? = '' OR im."Title" LIKE ?)"#,
    )
    .bind(search.trim())
    .bind(&like)
    .fetch_one(&state.pool)
    .await?;

    let rows = sqlx::query(
        r#"SELECT i."Id" AS "Id",
                  COALESCE(im."Title", '') AS "Title",
                  COALESCE(im."SortTitle", im."Title", '') AS "SortTitle",
                  im."ReleaseDate" AS "ReleaseDate",
                  a."Path" AS "Thumb"
           FROM "Image" i
           JOIN "ImageMetadata" im ON im."ImageId" = i."Id"
           LEFT JOIN "Artwork" a
               ON a."ImageMetadataId" = im."Id"
           WHERE (? = '' OR im."Title" LIKE ?)
           ORDER BY im."SortTitle", im."Title"
           LIMIT ? OFFSET ?"#,
    )
    .bind(search.trim())
    .bind(&like)
    .bind(i64::from(page_size))
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    let cards = rows
        .iter()
        .map(|r| MediaCardRow {
            id: r.try_get("Id").unwrap_or(0),
            title: r.try_get("Title").unwrap_or_default(),
            sort_title: r.try_get("SortTitle").unwrap_or_default(),
            subtitle: r
                .try_get::<Option<String>, _>("ReleaseDate")
                .unwrap_or(None),
            artwork_path: r
                .try_get::<Option<String>, _>("Thumb")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
        })
        .collect();

    #[allow(clippy::cast_sign_loss)]
    Ok((cards, total.max(0) as u64))
}

// ---------------------------------------------------------------------------
// Artists
// ---------------------------------------------------------------------------

#[server]
pub async fn list_artists(
    search: String,
    page: u32,
    page_size: u32,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let offset = i64::from(page) * i64::from(page_size);
    let like = format!("%{}%", search.trim());

    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "ArtistMetadata" am
           WHERE (? = '' OR am."Title" LIKE ?)"#,
    )
    .bind(search.trim())
    .bind(&like)
    .fetch_one(&state.pool)
    .await?;

    let rows = sqlx::query(
        r#"SELECT a."Id" AS "Id",
                  COALESCE(am."Title", '') AS "Title",
                  COALESCE(am."SortTitle", am."Title", '') AS "SortTitle",
                  am."Disambiguation" AS "Subtitle",
                  art."Path" AS "Thumb"
           FROM "Artist" a
           JOIN "ArtistMetadata" am ON am."ArtistId" = a."Id"
           LEFT JOIN "Artwork" art
               ON art."ArtistMetadataId" = am."Id" AND art."ArtworkKind" = 1
           WHERE (? = '' OR am."Title" LIKE ?)
           ORDER BY am."SortTitle", am."Title"
           LIMIT ? OFFSET ?"#,
    )
    .bind(search.trim())
    .bind(&like)
    .bind(i64::from(page_size))
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    let cards = rows
        .iter()
        .map(|r| MediaCardRow {
            id: r.try_get("Id").unwrap_or(0),
            title: r.try_get("Title").unwrap_or_default(),
            sort_title: r.try_get("SortTitle").unwrap_or_default(),
            subtitle: r.try_get::<Option<String>, _>("Subtitle").unwrap_or(None),
            artwork_path: r
                .try_get::<Option<String>, _>("Thumb")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
        })
        .collect();

    #[allow(clippy::cast_sign_loss)]
    Ok((cards, total.max(0) as u64))
}

// ---------------------------------------------------------------------------
// Other videos
// ---------------------------------------------------------------------------

#[server]
pub async fn list_other_videos(
    search: String,
    page: u32,
    page_size: u32,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let offset = i64::from(page) * i64::from(page_size);
    let like = format!("%{}%", search.trim());

    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "OtherVideoMetadata" ovm
           WHERE (? = '' OR ovm."Title" LIKE ?)"#,
    )
    .bind(search.trim())
    .bind(&like)
    .fetch_one(&state.pool)
    .await?;

    let rows = sqlx::query(
        r#"SELECT ov."Id" AS "Id",
                  COALESCE(ovm."Title", '') AS "Title",
                  COALESCE(ovm."SortTitle", ovm."Title", '') AS "SortTitle",
                  ovm."OriginalTitle" AS "Subtitle",
                  COALESCE(thumb."Path", poster."Path") AS "Artwork"
           FROM "OtherVideo" ov
           JOIN "OtherVideoMetadata" ovm ON ovm."OtherVideoId" = ov."Id"
           LEFT JOIN "Artwork" thumb
               ON thumb."OtherVideoMetadataId" = ovm."Id" AND thumb."ArtworkKind" = 1
           LEFT JOIN "Artwork" poster
               ON poster."OtherVideoMetadataId" = ovm."Id" AND poster."ArtworkKind" = 0
           WHERE (? = '' OR ovm."Title" LIKE ?)
           ORDER BY ovm."SortTitle", ovm."Title"
           LIMIT ? OFFSET ?"#,
    )
    .bind(search.trim())
    .bind(&like)
    .bind(i64::from(page_size))
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    let cards = rows
        .iter()
        .map(|r| MediaCardRow {
            id: r.try_get("Id").unwrap_or(0),
            title: r.try_get("Title").unwrap_or_default(),
            sort_title: r.try_get("SortTitle").unwrap_or_default(),
            subtitle: r.try_get::<Option<String>, _>("Subtitle").unwrap_or(None),
            artwork_path: r
                .try_get::<Option<String>, _>("Artwork")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
        })
        .collect();

    #[allow(clippy::cast_sign_loss)]
    Ok((cards, total.max(0) as u64))
}

// ---------------------------------------------------------------------------
// Seasons — "See All" target for the search page's Seasons section.
//
// Mirrors the season projection in `search.rs`: the card title is
// "Specials" for season 0 else "Season {n}"; subtitle is the show title;
// artwork falls back from the season poster to the show poster.
// ---------------------------------------------------------------------------

#[server]
pub async fn list_seasons(
    search: String,
    page: u32,
    page_size: u32,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let offset = i64::from(page) * i64::from(page_size);
    let like = format!("%{}%", search.trim());

    // Seasons are searched by their show's title (seasons have no title of
    // their own); ordering matches `media_page_map`'s SortTitle ordering.
    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "Season" s
           JOIN "Show" sh ON sh."Id" = s."ShowId"
           JOIN "ShowMetadata" shm ON shm."ShowId" = sh."Id"
           WHERE (? = '' OR shm."Title" LIKE ?)"#,
    )
    .bind(search.trim())
    .bind(&like)
    .fetch_one(&state.pool)
    .await?;

    let rows = sqlx::query(
        r#"SELECT s."Id" AS "Id",
                  s."SeasonNumber" AS "SeasonNumber",
                  COALESCE(shm."Title", '') AS "ShowTitle",
                  COALESCE(shm."SortTitle", shm."Title", '') AS "ShowSortTitle",
                  COALESCE(sa."Path", sha."Path") AS "Poster"
           FROM "Season" s
           JOIN "Show" sh ON sh."Id" = s."ShowId"
           JOIN "ShowMetadata" shm ON shm."ShowId" = sh."Id"
           LEFT JOIN "SeasonMetadata" sm ON sm."SeasonId" = s."Id"
           LEFT JOIN "Artwork" sa
               ON sa."SeasonMetadataId" = sm."Id" AND sa."ArtworkKind" = 0
           LEFT JOIN "Artwork" sha
               ON sha."ShowMetadataId" = shm."Id" AND sha."ArtworkKind" = 0
           WHERE (? = '' OR shm."Title" LIKE ?)
           ORDER BY shm."SortTitle", shm."Title", s."SeasonNumber"
           LIMIT ? OFFSET ?"#,
    )
    .bind(search.trim())
    .bind(&like)
    .bind(i64::from(page_size))
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    let cards = rows
        .iter()
        .map(|r| {
            let number: i64 = r.try_get("SeasonNumber").unwrap_or(0);
            let show_title: String = r.try_get("ShowTitle").unwrap_or_default();
            let show_sort: String = r.try_get("ShowSortTitle").unwrap_or_default();
            let title = if number == 0 {
                "Specials".to_string()
            } else {
                format!("Season {number}")
            };
            MediaCardRow {
                id: r.try_get("Id").unwrap_or(0),
                title,
                // Sort within a show by season number, scoped under the
                // show's sort title so the letter-bar groups by show.
                sort_title: format!("{show_sort} {number:04}"),
                subtitle: (!show_title.is_empty()).then_some(show_title),
                artwork_path: r
                    .try_get::<Option<String>, _>("Poster")
                    .unwrap_or(None)
                    .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
            }
        })
        .collect();

    #[allow(clippy::cast_sign_loss)]
    Ok((cards, total.max(0) as u64))
}

// ---------------------------------------------------------------------------
// Episodes — "See All" target for the search page's Episodes section.
//
// Subtitle mirrors `search.rs`: "{ShowTitle} - S{season} E{episode}".
// Artwork falls back episode → season → show poster.
// ---------------------------------------------------------------------------

#[server]
pub async fn list_episodes(
    search: String,
    page: u32,
    page_size: u32,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let offset = i64::from(page) * i64::from(page_size);
    let like = format!("%{}%", search.trim());

    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "EpisodeMetadata" em
           WHERE (? = '' OR em."Title" LIKE ?)"#,
    )
    .bind(search.trim())
    .bind(&like)
    .fetch_one(&state.pool)
    .await?;

    let rows = sqlx::query(
        r#"SELECT e."Id" AS "Id",
                  COALESCE(em."Title", '') AS "Title",
                  COALESCE(em."SortTitle", em."Title", '') AS "SortTitle",
                  em."EpisodeNumber" AS "EpisodeNumber",
                  s."SeasonNumber" AS "SeasonNumber",
                  COALESCE(shm."Title", '') AS "ShowTitle",
                  COALESCE(ea."Path", sa."Path", sha."Path") AS "Poster"
           FROM "Episode" e
           JOIN "EpisodeMetadata" em ON em."EpisodeId" = e."Id"
           JOIN "Season" s ON s."Id" = e."SeasonId"
           JOIN "Show" sh ON sh."Id" = s."ShowId"
           JOIN "ShowMetadata" shm ON shm."ShowId" = sh."Id"
           LEFT JOIN "SeasonMetadata" sm ON sm."SeasonId" = s."Id"
           LEFT JOIN "Artwork" ea
               ON ea."EpisodeMetadataId" = em."Id" AND ea."ArtworkKind" = 0
           LEFT JOIN "Artwork" sa
               ON sa."SeasonMetadataId" = sm."Id" AND sa."ArtworkKind" = 0
           LEFT JOIN "Artwork" sha
               ON sha."ShowMetadataId" = shm."Id" AND sha."ArtworkKind" = 0
           WHERE (? = '' OR em."Title" LIKE ?)
           ORDER BY em."SortTitle", em."Title"
           LIMIT ? OFFSET ?"#,
    )
    .bind(search.trim())
    .bind(&like)
    .bind(i64::from(page_size))
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    let cards = rows
        .iter()
        .map(|r| {
            let show_title: String = r.try_get("ShowTitle").unwrap_or_default();
            let season_number: i64 = r.try_get("SeasonNumber").unwrap_or(0);
            let episode_number: i64 = r.try_get("EpisodeNumber").unwrap_or(0);
            let subtitle = (!show_title.is_empty())
                .then(|| format!("{show_title} - S{season_number} E{episode_number}"));
            MediaCardRow {
                id: r.try_get("Id").unwrap_or(0),
                title: r.try_get("Title").unwrap_or_default(),
                sort_title: r.try_get("SortTitle").unwrap_or_default(),
                subtitle,
                artwork_path: r
                    .try_get::<Option<String>, _>("Poster")
                    .unwrap_or(None)
                    .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
            }
        })
        .collect();

    #[allow(clippy::cast_sign_loss)]
    Ok((cards, total.max(0) as u64))
}

// ---------------------------------------------------------------------------
// Music videos — "See All" target for the search page's Music Videos section.
//
// Subtitle is the owning artist's name; artwork is the thumbnail (kind 1).
// ---------------------------------------------------------------------------

#[server]
pub async fn list_music_videos(
    search: String,
    page: u32,
    page_size: u32,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let offset = i64::from(page) * i64::from(page_size);
    let like = format!("%{}%", search.trim());

    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "MusicVideoMetadata" mvm
           WHERE (? = '' OR mvm."Title" LIKE ?)"#,
    )
    .bind(search.trim())
    .bind(&like)
    .fetch_one(&state.pool)
    .await?;

    let rows = sqlx::query(
        r#"SELECT mv."Id" AS "Id",
                  COALESCE(mvm."Title", '') AS "Title",
                  COALESCE(mvm."SortTitle", mvm."Title", '') AS "SortTitle",
                  am."Title" AS "ArtistName",
                  art."Path" AS "Thumb"
           FROM "MusicVideo" mv
           JOIN "MusicVideoMetadata" mvm ON mvm."MusicVideoId" = mv."Id"
           LEFT JOIN "Artist" a ON a."Id" = mv."ArtistId"
           LEFT JOIN "ArtistMetadata" am ON am."ArtistId" = a."Id"
           LEFT JOIN "Artwork" art
               ON art."MusicVideoMetadataId" = mvm."Id" AND art."ArtworkKind" = 1
           WHERE (? = '' OR mvm."Title" LIKE ?)
           ORDER BY mvm."SortTitle", mvm."Title"
           LIMIT ? OFFSET ?"#,
    )
    .bind(search.trim())
    .bind(&like)
    .bind(i64::from(page_size))
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    let cards = rows
        .iter()
        .map(|r| MediaCardRow {
            id: r.try_get("Id").unwrap_or(0),
            title: r.try_get("Title").unwrap_or_default(),
            sort_title: r.try_get("SortTitle").unwrap_or_default(),
            subtitle: r
                .try_get::<Option<String>, _>("ArtistName")
                .unwrap_or(None)
                .filter(|s| !s.trim().is_empty()),
            artwork_path: r
                .try_get::<Option<String>, _>("Thumb")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
        })
        .collect();

    #[allow(clippy::cast_sign_loss)]
    Ok((cards, total.max(0) as u64))
}

// ---------------------------------------------------------------------------
// Remote streams — "See All" target for the search page's Remote Streams
// section. Subtitle is the original title; artwork is the thumbnail.
// ---------------------------------------------------------------------------

#[server]
pub async fn list_remote_streams(
    search: String,
    page: u32,
    page_size: u32,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let offset = i64::from(page) * i64::from(page_size);
    let like = format!("%{}%", search.trim());

    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "RemoteStreamMetadata" rsm
           WHERE (? = '' OR rsm."Title" LIKE ?)"#,
    )
    .bind(search.trim())
    .bind(&like)
    .fetch_one(&state.pool)
    .await?;

    let rows = sqlx::query(
        r#"SELECT rs."Id" AS "Id",
                  COALESCE(rsm."Title", '') AS "Title",
                  COALESCE(rsm."SortTitle", rsm."Title", '') AS "SortTitle",
                  rsm."OriginalTitle" AS "Subtitle",
                  art."Path" AS "Thumb"
           FROM "RemoteStream" rs
           JOIN "RemoteStreamMetadata" rsm ON rsm."RemoteStreamId" = rs."Id"
           LEFT JOIN "Artwork" art
               ON art."RemoteStreamMetadataId" = rsm."Id" AND art."ArtworkKind" = 1
           WHERE (? = '' OR rsm."Title" LIKE ?)
           ORDER BY rsm."SortTitle", rsm."Title"
           LIMIT ? OFFSET ?"#,
    )
    .bind(search.trim())
    .bind(&like)
    .bind(i64::from(page_size))
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    let cards = rows
        .iter()
        .map(|r| MediaCardRow {
            id: r.try_get("Id").unwrap_or(0),
            title: r.try_get("Title").unwrap_or_default(),
            sort_title: r.try_get("SortTitle").unwrap_or_default(),
            subtitle: r
                .try_get::<Option<String>, _>("Subtitle")
                .unwrap_or(None)
                .filter(|s| !s.trim().is_empty()),
            artwork_path: r
                .try_get::<Option<String>, _>("Thumb")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
        })
        .collect();

    #[allow(clippy::cast_sign_loss)]
    Ok((cards, total.max(0) as u64))
}

// ---------------------------------------------------------------------------
// Songs
// ---------------------------------------------------------------------------

#[server]
pub async fn list_songs(
    search: String,
    page: u32,
    page_size: u32,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let offset = i64::from(page) * i64::from(page_size);
    let like = format!("%{}%", search.trim());

    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "SongMetadata" sm
           WHERE (? = '' OR sm."Title" LIKE ?)"#,
    )
    .bind(search.trim())
    .bind(&like)
    .fetch_one(&state.pool)
    .await?;

    let rows = sqlx::query(
        r#"SELECT s."Id" AS "Id",
                  COALESCE(sm."Title", '') AS "Title",
                  COALESCE(sm."SortTitle", sm."Title", '') AS "SortTitle",
                  sm."Artists",
                  sm."Album",
                  art."Path" AS "Thumb"
           FROM "Song" s
           JOIN "SongMetadata" sm ON sm."SongId" = s."Id"
           LEFT JOIN "Artwork" art
               ON art."SongMetadataId" = sm."Id" AND art."ArtworkKind" = 1
           WHERE (? = '' OR sm."Title" LIKE ?)
           ORDER BY sm."SortTitle", sm."Title"
           LIMIT ? OFFSET ?"#,
    )
    .bind(search.trim())
    .bind(&like)
    .bind(i64::from(page_size))
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    let cards = rows
        .iter()
        .map(|r| MediaCardRow {
            id: r.try_get("Id").unwrap_or(0),
            title: r.try_get("Title").unwrap_or_default(),
            sort_title: r.try_get("SortTitle").unwrap_or_default(),
            subtitle: song_subtitle(
                r.try_get::<Option<String>, _>("Artists").unwrap_or(None),
                r.try_get::<Option<String>, _>("Album").unwrap_or(None),
            ),
            artwork_path: r
                .try_get::<Option<String>, _>("Thumb")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
        })
        .collect();

    #[allow(clippy::cast_sign_loss)]
    Ok((cards, total.max(0) as u64))
}

// ---------------------------------------------------------------------------
// Trash
// ---------------------------------------------------------------------------

#[server]
pub async fn list_trash_items() -> Result<TrashPageData, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let (movies, shows, artists, other_videos, songs, images) = tokio::join!(
        load_trash_movies(&state.pool),
        load_trash_shows(&state.pool),
        load_trash_artists(&state.pool),
        load_trash_other_videos(&state.pool),
        load_trash_songs(&state.pool),
        load_trash_images(&state.pool),
    );

    Ok(TrashPageData {
        movies: movies?,
        shows: shows?,
        artists: artists?,
        other_videos: other_videos?,
        songs: songs?,
        images: images?,
    })
}

#[server]
pub async fn delete_media_items(ids: Vec<i64>) -> Result<u64, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::QueryBuilder;

    let state = expect_context::<AppState>();
    if ids.is_empty() {
        return Ok(0);
    }

    let mut builder =
        QueryBuilder::<ersatzrs_infra::db::Db>::new(r#"DELETE FROM "MediaItem" WHERE "Id" IN ("#);
    let mut separated = builder.separated(", ");
    for id in ids {
        separated.push_bind(id);
    }
    separated.push_unseparated(")");

    let result = builder.build().execute(&state.pool).await?;
    Ok(result.rows_affected())
}

#[server]
pub async fn empty_trash() -> Result<u64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let result = sqlx::query(r#"DELETE FROM "MediaItem" WHERE "State" = 1"#)
        .execute(&state.pool)
        .await?;
    Ok(result.rows_affected())
}

#[cfg(feature = "ssr")]
async fn load_trash_movies(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<Vec<MediaCardRow>, sqlx::Error> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT m."Id" AS "Id",
                  COALESCE(mm."Title", '') AS "Title",
                  COALESCE(mm."SortTitle", mm."Title", '') AS "SortTitle",
                  mm."Year" AS "Year",
                  a."Path" AS "Poster"
           FROM "Movie" m
           JOIN "MediaItem" mi ON mi."Id" = m."Id" AND mi."State" = 1
           JOIN "MovieMetadata" mm ON mm."MovieId" = m."Id"
           LEFT JOIN "Artwork" a
               ON a."MovieMetadataId" = mm."Id" AND a."ArtworkKind" = 0
           ORDER BY mm."SortTitle", mm."Title""#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| MediaCardRow {
            id: r.try_get("Id").unwrap_or(0),
            title: r.try_get("Title").unwrap_or_default(),
            sort_title: r.try_get("SortTitle").unwrap_or_default(),
            subtitle: r
                .try_get::<Option<i64>, _>("Year")
                .unwrap_or(None)
                .map(|year| year.to_string()),
            artwork_path: r
                .try_get::<Option<String>, _>("Poster")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
        })
        .collect())
}

#[cfg(feature = "ssr")]
async fn load_trash_shows(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<Vec<MediaCardRow>, sqlx::Error> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT s."Id" AS "Id",
                  COALESCE(sm."Title", '') AS "Title",
                  COALESCE(sm."SortTitle", sm."Title", '') AS "SortTitle",
                  sm."Year" AS "Year",
                  a."Path" AS "Poster"
           FROM "Show" s
           JOIN "MediaItem" mi ON mi."Id" = s."Id" AND mi."State" = 1
           JOIN "ShowMetadata" sm ON sm."ShowId" = s."Id"
           LEFT JOIN "Artwork" a
               ON a."ShowMetadataId" = sm."Id" AND a."ArtworkKind" = 0
           ORDER BY sm."SortTitle", sm."Title""#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| MediaCardRow {
            id: r.try_get("Id").unwrap_or(0),
            title: r.try_get("Title").unwrap_or_default(),
            sort_title: r.try_get("SortTitle").unwrap_or_default(),
            subtitle: r
                .try_get::<Option<i64>, _>("Year")
                .unwrap_or(None)
                .map(|year| year.to_string()),
            artwork_path: r
                .try_get::<Option<String>, _>("Poster")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
        })
        .collect())
}

#[cfg(feature = "ssr")]
async fn load_trash_artists(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<Vec<MediaCardRow>, sqlx::Error> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT a."Id" AS "Id",
                  COALESCE(am."Title", '') AS "Title",
                  COALESCE(am."SortTitle", am."Title", '') AS "SortTitle",
                  am."Disambiguation" AS "Subtitle",
                  art."Path" AS "Thumb"
           FROM "Artist" a
           JOIN "MediaItem" mi ON mi."Id" = a."Id" AND mi."State" = 1
           JOIN "ArtistMetadata" am ON am."ArtistId" = a."Id"
           LEFT JOIN "Artwork" art
               ON art."ArtistMetadataId" = am."Id" AND art."ArtworkKind" = 1
           ORDER BY am."SortTitle", am."Title""#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| MediaCardRow {
            id: r.try_get("Id").unwrap_or(0),
            title: r.try_get("Title").unwrap_or_default(),
            sort_title: r.try_get("SortTitle").unwrap_or_default(),
            subtitle: r.try_get::<Option<String>, _>("Subtitle").unwrap_or(None),
            artwork_path: r
                .try_get::<Option<String>, _>("Thumb")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
        })
        .collect())
}

#[cfg(feature = "ssr")]
async fn load_trash_other_videos(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<Vec<MediaCardRow>, sqlx::Error> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT ov."Id" AS "Id",
                  COALESCE(ovm."Title", '') AS "Title",
                  COALESCE(ovm."SortTitle", ovm."Title", '') AS "SortTitle",
                  ovm."OriginalTitle" AS "Subtitle",
                  COALESCE(thumb."Path", poster."Path") AS "Artwork"
           FROM "OtherVideo" ov
           JOIN "MediaItem" mi ON mi."Id" = ov."Id" AND mi."State" = 1
           JOIN "OtherVideoMetadata" ovm ON ovm."OtherVideoId" = ov."Id"
           LEFT JOIN "Artwork" thumb
               ON thumb."OtherVideoMetadataId" = ovm."Id" AND thumb."ArtworkKind" = 1
           LEFT JOIN "Artwork" poster
               ON poster."OtherVideoMetadataId" = ovm."Id" AND poster."ArtworkKind" = 0
           ORDER BY ovm."SortTitle", ovm."Title""#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| MediaCardRow {
            id: r.try_get("Id").unwrap_or(0),
            title: r.try_get("Title").unwrap_or_default(),
            sort_title: r.try_get("SortTitle").unwrap_or_default(),
            subtitle: r.try_get::<Option<String>, _>("Subtitle").unwrap_or(None),
            artwork_path: r
                .try_get::<Option<String>, _>("Artwork")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
        })
        .collect())
}

#[cfg(feature = "ssr")]
async fn load_trash_songs(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<Vec<MediaCardRow>, sqlx::Error> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT s."Id" AS "Id",
                  COALESCE(sm."Title", '') AS "Title",
                  COALESCE(sm."SortTitle", sm."Title", '') AS "SortTitle",
                  sm."Artists",
                  sm."Album",
                  art."Path" AS "Thumb"
           FROM "Song" s
           JOIN "MediaItem" mi ON mi."Id" = s."Id" AND mi."State" = 1
           JOIN "SongMetadata" sm ON sm."SongId" = s."Id"
           LEFT JOIN "Artwork" art
               ON art."SongMetadataId" = sm."Id" AND art."ArtworkKind" = 1
           ORDER BY sm."SortTitle", sm."Title""#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| MediaCardRow {
            id: r.try_get("Id").unwrap_or(0),
            title: r.try_get("Title").unwrap_or_default(),
            sort_title: r.try_get("SortTitle").unwrap_or_default(),
            subtitle: song_subtitle(
                r.try_get::<Option<String>, _>("Artists").unwrap_or(None),
                r.try_get::<Option<String>, _>("Album").unwrap_or(None),
            ),
            artwork_path: r
                .try_get::<Option<String>, _>("Thumb")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
        })
        .collect())
}

#[cfg(feature = "ssr")]
async fn load_trash_images(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<Vec<MediaCardRow>, sqlx::Error> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT i."Id" AS "Id",
                  COALESCE(im."Title", '') AS "Title",
                  COALESCE(im."SortTitle", im."Title", '') AS "SortTitle",
                  im."ReleaseDate" AS "ReleaseDate",
                  a."Path" AS "Thumb"
           FROM "Image" i
           JOIN "MediaItem" mi ON mi."Id" = i."Id" AND mi."State" = 1
           JOIN "ImageMetadata" im ON im."ImageId" = i."Id"
           LEFT JOIN "Artwork" a ON a."ImageMetadataId" = im."Id"
           ORDER BY im."SortTitle", im."Title""#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| MediaCardRow {
            id: r.try_get("Id").unwrap_or(0),
            title: r.try_get("Title").unwrap_or_default(),
            sort_title: r.try_get("SortTitle").unwrap_or_default(),
            subtitle: r
                .try_get::<Option<String>, _>("ReleaseDate")
                .unwrap_or(None),
            artwork_path: r
                .try_get::<Option<String>, _>("Thumb")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Manual collections
// ---------------------------------------------------------------------------

#[server]
pub async fn list_collections(
    search: String,
    page: u32,
    page_size: u32,
) -> Result<(Vec<CollectionListRow>, u64), ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let offset = i64::from(page) * i64::from(page_size);
    let like = format!("%{}%", search.trim());

    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "Collection" c
           WHERE (? = '' OR c."Name" LIKE ?)"#,
    )
    .bind(search.trim())
    .bind(&like)
    .fetch_one(&state.pool)
    .await?;

    let rows = sqlx::query(
        r#"SELECT c."Id", COALESCE(c."Name", '') AS "Name"
           FROM "Collection" c
           WHERE (? = '' OR c."Name" LIKE ?)
           ORDER BY c."Name"
           LIMIT ? OFFSET ?"#,
    )
    .bind(search.trim())
    .bind(&like)
    .bind(i64::from(page_size))
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    let list = rows
        .iter()
        .map(|r| CollectionListRow {
            id: r.try_get("Id").unwrap_or(0),
            name: r.try_get("Name").unwrap_or_default(),
            query: None,
        })
        .collect();

    #[allow(clippy::cast_sign_loss)]
    Ok((list, total.max(0) as u64))
}

#[server]
pub async fn get_collection(id: i64) -> Result<Option<CollectionForm>, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let row = sqlx::query(
        r#"SELECT "Id", COALESCE("Name", '') AS "Name" FROM "Collection" WHERE "Id" = ?"#,
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;

    Ok(row.map(|r| CollectionForm {
        id: r.try_get("Id").unwrap_or(0),
        name: r.try_get("Name").unwrap_or_default(),
        query: String::new(),
    }))
}

#[server]
pub async fn save_collection(form: CollectionForm) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let name = form.name.trim();
    if name.is_empty() {
        return Err(ServerFnError::new("Collection name is required"));
    }

    if form.id == 0 {
        let result = sqlx::query(
            r#"INSERT INTO "Collection" ("Name", "UseCustomPlaybackOrder") VALUES (?, 0)"#,
        )
        .bind(name)
        .execute(&state.pool)
        .await?;
        let id = ersatzrs_infra::db::last_insert_id(result);
        Ok(id)
    } else {
        sqlx::query(r#"UPDATE "Collection" SET "Name" = ? WHERE "Id" = ?"#)
            .bind(name)
            .bind(form.id)
            .execute(&state.pool)
            .await?;
        Ok(form.id)
    }
}

#[server]
pub async fn delete_collection(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    sqlx::query(r#"DELETE FROM "Collection" WHERE "Id" = ?"#)
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Smart collections
// ---------------------------------------------------------------------------

#[server]
pub async fn list_smart_collections(
    search: String,
    page: u32,
    page_size: u32,
) -> Result<(Vec<CollectionListRow>, u64), ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let offset = i64::from(page) * i64::from(page_size);
    let like = format!("%{}%", search.trim());

    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "SmartCollection" sc
           WHERE (? = '' OR sc."Name" LIKE ?)"#,
    )
    .bind(search.trim())
    .bind(&like)
    .fetch_one(&state.pool)
    .await?;

    let rows = sqlx::query(
        r#"SELECT sc."Id",
                  COALESCE(sc."Name", '') AS "Name",
                  COALESCE(sc."Query", '') AS "Query"
           FROM "SmartCollection" sc
           WHERE (? = '' OR sc."Name" LIKE ?)
           ORDER BY sc."Name"
           LIMIT ? OFFSET ?"#,
    )
    .bind(search.trim())
    .bind(&like)
    .bind(i64::from(page_size))
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    let list = rows
        .iter()
        .map(|r| CollectionListRow {
            id: r.try_get("Id").unwrap_or(0),
            name: r.try_get("Name").unwrap_or_default(),
            query: Some(r.try_get("Query").unwrap_or_default()),
        })
        .collect();

    #[allow(clippy::cast_sign_loss)]
    Ok((list, total.max(0) as u64))
}

#[server]
pub async fn get_smart_collection(id: i64) -> Result<Option<CollectionForm>, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let row = sqlx::query(
        r#"SELECT "Id", COALESCE("Name", '') AS "Name", COALESCE("Query", '') AS "Query"
           FROM "SmartCollection" WHERE "Id" = ?"#,
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;

    Ok(row.map(|r| CollectionForm {
        id: r.try_get("Id").unwrap_or(0),
        name: r.try_get("Name").unwrap_or_default(),
        query: r.try_get("Query").unwrap_or_default(),
    }))
}

#[server]
pub async fn save_smart_collection(form: CollectionForm) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let name = form.name.trim();
    if name.is_empty() {
        return Err(ServerFnError::new("Smart collection name is required"));
    }
    let query = form.query.trim();
    if query.is_empty() {
        return Err(ServerFnError::new("Smart collection query is required"));
    }

    if form.id == 0 {
        let result =
            sqlx::query(r#"INSERT INTO "SmartCollection" ("Name", "Query") VALUES (?, ?)"#)
                .bind(name)
                .bind(query)
                .execute(&state.pool)
                .await?;
        let id = ersatzrs_infra::db::last_insert_id(result);
        Ok(id)
    } else {
        sqlx::query(r#"UPDATE "SmartCollection" SET "Name" = ?, "Query" = ? WHERE "Id" = ?"#)
            .bind(name)
            .bind(query)
            .bind(form.id)
            .execute(&state.pool)
            .await?;
        Ok(form.id)
    }
}

#[server]
pub async fn delete_smart_collection(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    sqlx::query(r#"DELETE FROM "SmartCollection" WHERE "Id" = ?"#)
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Autocomplete for collection editors
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MediaItemSuggestion {
    pub id: i64,
    pub title: String,
    /// "movie" | "show"
    pub kind: String,
}

/// Debounced autocomplete. Returns up to 20 movies+shows matching the prefix.
#[server]
pub async fn suggest_media_items(
    prefix: String,
) -> Result<Vec<MediaItemSuggestion>, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let prefix = prefix.trim();
    if prefix.is_empty() {
        return Ok(vec![]);
    }
    let like = format!("{prefix}%");

    let rows = sqlx::query(
        r#"SELECT "Id", "Title", "Kind" FROM (
               SELECT m."Id" AS "Id",
                      COALESCE(mm."Title", '') AS "Title",
                      'movie' AS "Kind",
                      mm."SortTitle" AS "SortTitle"
               FROM "Movie" m
               JOIN "MovieMetadata" mm ON mm."MovieId" = m."Id"
               WHERE mm."Title" LIKE ?
               UNION ALL
               SELECT s."Id" AS "Id",
                      COALESCE(sm."Title", '') AS "Title",
                      'show' AS "Kind",
                      sm."SortTitle" AS "SortTitle"
               FROM "Show" s
               JOIN "ShowMetadata" sm ON sm."ShowId" = s."Id"
               WHERE sm."Title" LIKE ?
           )
           ORDER BY "SortTitle", "Title"
           LIMIT 20"#,
    )
    .bind(&like)
    .bind(&like)
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| MediaItemSuggestion {
            id: r.try_get("Id").unwrap_or(0),
            title: r.try_get("Title").unwrap_or_default(),
            kind: r.try_get("Kind").unwrap_or_default(),
        })
        .collect())
}

#[server]
pub async fn add_item_to_collection(
    collection_id: i64,
    media_item_id: i64,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    sqlx::query(
        r#"INSERT OR IGNORE INTO "CollectionItem" ("CollectionId", "MediaItemId") VALUES (?, ?)"#,
    )
    .bind(collection_id)
    .bind(media_item_id)
    .execute(&state.pool)
    .await?;
    Ok(())
}

#[server]
pub async fn remove_item_from_collection(
    collection_id: i64,
    media_item_id: i64,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    sqlx::query(r#"DELETE FROM "CollectionItem" WHERE "CollectionId" = ? AND "MediaItemId" = ?"#)
        .bind(collection_id)
        .bind(media_item_id)
        .execute(&state.pool)
        .await?;
    Ok(())
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CollectionMember {
    pub media_item_id: i64,
    pub title: String,
    pub kind: String,
}

// ---------------------------------------------------------------------------
// Multi-select: add selected media items to a collection or playlist.
//
// Ports `ErsatzTV.Application.MediaCollections.AddItemsToCollectionHandler`
// (collection branch) and `AddItemsToPlaylistHandler` (playlist branch).
// The MovieList page only ever passes movie ids, but the underlying tables
// are media-item agnostic; we accept a typed kind so future ports of
// `ShowList`, `OtherVideoList`, etc. can reuse these endpoints unchanged.
// ---------------------------------------------------------------------------

/// Row for the schedule dropdown in the show "Add To Schedule" dialog.
/// Mirrors the id + name subset of
/// `ErsatzTV.Application.ProgramSchedules.ProgramScheduleViewModel`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ScheduleOption {
    pub id: i64,
    pub name: String,
}

/// All program schedules, ordered by name. Mirrors
/// `GetAllProgramSchedulesHandler` (every row from `ProgramSchedule`).
#[server]
pub async fn list_program_schedules() -> Result<Vec<ScheduleOption>, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let rows = sqlx::query(
        r#"SELECT "Id" AS "Id", COALESCE("Name", '') AS "Name"
           FROM "ProgramSchedule" ORDER BY "Name""#,
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(rows
        .iter()
        .map(|r| ScheduleOption {
            id: r.try_get("Id").unwrap_or(0),
            name: r.try_get("Name").unwrap_or_default(),
        })
        .collect())
}

/// Adds a show to a program schedule as a single `PlayoutMode.One` item.
/// Ports `AddProgramScheduleItem.ForMediaItem(scheduleId,
/// CollectionType.TelevisionShow, showId)` +
/// `AddProgramScheduleItemHandler` / `ProgramScheduleItemCommandBase`.
///
/// Parity note: ErsatzTV also refreshes any playouts that use the schedule
/// (a `BuildPlayout` background request). That refresh is not wired here;
/// the next scheduled playout build picks the new item up.
#[server]
pub async fn add_show_to_schedule(schedule_id: i64, show_id: i64) -> Result<(), ServerFnError> {
    // Enum values from `ErsatzTV.Core.Domain`:
    // CollectionType.TelevisionShow = 1, PlaybackOrder.Shuffle = 3,
    // FillWithGroupMode.None = 0, GuideMode.Normal = 0,
    // MarathonGroupBy.None = 0.
    const COLLECTION_TYPE_TELEVISION_SHOW: i64 = 1;
    const PLAYBACK_ORDER_SHUFFLE: i64 = 3;
    const FILL_WITH_GROUP_MODE_NONE: i64 = 0;
    const GUIDE_MODE_NORMAL: i64 = 0;
    const MARATHON_GROUP_BY_NONE: i64 = 0;

    use crate::host::state::AppState;

    let state = expect_context::<AppState>();

    let schedule_exists: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "ProgramSchedule" WHERE "Id" = ?"#)
            .bind(schedule_id)
            .fetch_one(&state.pool)
            .await?;
    if schedule_exists == 0 {
        return Err(ServerFnError::new("[ProgramScheduleId] does not exist."));
    }
    let show_exists: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Show" WHERE "Id" = ?"#)
        .bind(show_id)
        .fetch_one(&state.pool)
        .await?;
    if show_exists == 0 {
        return Err(ServerFnError::new("[MediaItem] does not exist."));
    }

    let mut tx = state.pool.begin().await?;

    // nextIndex = max(Index) + 1 over the schedule's existing items.
    let next_index: i64 = sqlx::query_scalar(
        r#"SELECT COALESCE(MAX("Index"), -1) + 1
           FROM "ProgramScheduleItem" WHERE "ProgramScheduleId" = ?"#,
    )
    .bind(schedule_id)
    .fetch_one(&mut *tx)
    .await?;

    let result = sqlx::query(
        r#"INSERT INTO "ProgramScheduleItem" (
               "ProgramScheduleId", "CollectionType", "MediaItemId",
               "PlaybackOrder", "FillWithGroupMode", "GuideMode",
               "MarathonGroupBy", "MarathonShuffleGroups", "MarathonShuffleItems",
               "Index"
           ) VALUES (?, ?, ?, ?, ?, ?, ?, 0, 0, ?)"#,
    )
    .bind(schedule_id)
    .bind(COLLECTION_TYPE_TELEVISION_SHOW)
    .bind(show_id)
    .bind(PLAYBACK_ORDER_SHUFFLE)
    .bind(FILL_WITH_GROUP_MODE_NONE)
    .bind(GUIDE_MODE_NORMAL)
    .bind(MARATHON_GROUP_BY_NONE)
    .bind(next_index)
    .execute(&mut *tx)
    .await?;

    // PlayoutMode.One → a `ProgramScheduleOneItem` subtype row (TPT).
    let item_id = ersatzrs_infra::db::last_insert_id(result);
    sqlx::query(r#"INSERT INTO "ProgramScheduleOneItem" ("Id") VALUES (?)"#)
        .bind(item_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}

/// Row for the "(New Collection)" / existing-collection dropdown in
/// `AddToCollectionDialog`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CollectionOption {
    pub id: i64,
    pub name: String,
}

/// Row for the "Playlist Group" dropdown in `AddToPlaylistDialog`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PlaylistGroupOption {
    pub id: i64,
    pub name: String,
}

/// Row for the "Playlist" dropdown in `AddToPlaylistDialog`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PlaylistOption {
    pub id: i64,
    pub name: String,
    pub playlist_group_id: i64,
}

/// All collections, ordered by name. Mirrors `GetAllCollectionsHandler`,
/// which loads every row from `Collection`.
#[server]
pub async fn list_all_collections() -> Result<Vec<CollectionOption>, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let rows = sqlx::query(
        r#"SELECT "Id", COALESCE("Name", '') AS "Name"
           FROM "Collection"
           ORDER BY "Name" COLLATE NOCASE"#,
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| CollectionOption {
            id: r.try_get("Id").unwrap_or(0),
            name: r.try_get("Name").unwrap_or_default(),
        })
        .collect())
}

/// All smart collections, ordered by name. Used by the search page's
/// Save-As-Smart-Collection dialog.
#[server]
pub async fn list_smart_collections_for_select() -> Result<Vec<CollectionOption>, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let rows = sqlx::query(
        r#"SELECT "Id", COALESCE("Name", '') AS "Name"
           FROM "SmartCollection"
           ORDER BY "Name" COLLATE NOCASE"#,
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| CollectionOption {
            id: r.try_get("Id").unwrap_or(0),
            name: r.try_get("Name").unwrap_or_default(),
        })
        .collect())
}

/// Creates a new collection with the given name and returns it. Mirrors
/// `CreateCollectionHandler`.
#[server]
pub async fn create_collection(name: String) -> Result<CollectionOption, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(ServerFnError::new("Collection name is required"));
    }
    if trimmed.chars().count() > 50 {
        return Err(ServerFnError::new(
            "Collection name must be 50 characters or less",
        ));
    }

    let duplicate: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Collection" WHERE "Name" = ? COLLATE NOCASE"#)
            .bind(trimmed)
            .fetch_one(&state.pool)
            .await?;
    if duplicate > 0 {
        return Err(ServerFnError::new("Collection name must be unique"));
    }

    let result =
        sqlx::query(r#"INSERT INTO "Collection" ("Name", "UseCustomPlaybackOrder") VALUES (?, 0)"#)
            .bind(trimmed)
            .execute(&state.pool)
            .await?;
    let id = ersatzrs_infra::db::last_insert_id(result);
    Ok(CollectionOption {
        id,
        name: trimmed.to_string(),
    })
}

/// All playlist groups, ordered system-first then by name. Mirrors
/// `GetAllPlaylistGroupsHandler` (`OrderByDescending(pg => pg.IsSystem)`,
/// `ThenBy(pg => pg.Name)`).
#[server]
pub async fn list_all_playlist_groups() -> Result<Vec<PlaylistGroupOption>, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let rows = sqlx::query(
        r#"SELECT "Id", COALESCE("Name", '') AS "Name"
           FROM "PlaylistGroup"
           ORDER BY COALESCE("IsSystem", 0) DESC, "Name" COLLATE NOCASE"#,
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| PlaylistGroupOption {
            id: r.try_get("Id").unwrap_or(0),
            name: r.try_get("Name").unwrap_or_default(),
        })
        .collect())
}

/// Playlists in a given group, ordered by name. Mirrors
/// `GetPlaylistsByPlaylistGroupIdHandler`.
#[server]
pub async fn list_playlists_by_group(
    playlist_group_id: i64,
) -> Result<Vec<PlaylistOption>, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let rows = sqlx::query(
        r#"SELECT "Id", COALESCE("Name", '') AS "Name", "PlaylistGroupId"
           FROM "Playlist"
           WHERE "PlaylistGroupId" = ?
           ORDER BY "Name" COLLATE NOCASE"#,
    )
    .bind(playlist_group_id)
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| PlaylistOption {
            id: r.try_get("Id").unwrap_or(0),
            name: r.try_get("Name").unwrap_or_default(),
            playlist_group_id: r.try_get("PlaylistGroupId").unwrap_or(0),
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Page-map + add-to-collection/playlist used by `/media/movies`,
// `/media/tv/shows`, `/media/music/artists`, and `/media/other/videos`.
//
// These functions accept a `kind` discriminant so the same browser shell can
// drive every multi-select media kind with identical SQL shapes (one table
// per kind, joined to its metadata).
// ---------------------------------------------------------------------------

/// Discriminant for the media-browser server functions below. Values match the
/// kind discriminants used by `CollectionItemCard.kind`.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum MediaBrowserKind {
    Movie,
    Show,
    Artist,
    OtherVideo,
    Season,
    Episode,
    MusicVideo,
    RemoteStream,
}

#[cfg(feature = "ssr")]
impl MediaBrowserKind {
    /// SQL table + metadata table + foreign-key column for the page-map query.
    /// Returns `(item_table, metadata_table, fk_column)`.
    const fn tables(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Movie => ("Movie", "MovieMetadata", "MovieId"),
            Self::Show => ("Show", "ShowMetadata", "ShowId"),
            Self::Artist => ("Artist", "ArtistMetadata", "ArtistId"),
            Self::OtherVideo => ("OtherVideo", "OtherVideoMetadata", "OtherVideoId"),
            Self::Season => ("Season", "SeasonMetadata", "SeasonId"),
            Self::Episode => ("Episode", "EpisodeMetadata", "EpisodeId"),
            Self::MusicVideo => ("MusicVideo", "MusicVideoMetadata", "MusicVideoId"),
            Self::RemoteStream => ("RemoteStream", "RemoteStreamMetadata", "RemoteStreamId"),
        }
    }

    /// `CollectionType` value used when inserting `PlaylistItem` rows.
    /// Matches `ErsatzTV.Core/Domain/CollectionType.cs`.
    const fn collection_type(self) -> i32 {
        match self {
            Self::Movie => 10,
            Self::Show => 1,
            Self::Artist => 3,
            Self::OtherVideo => 40,
            Self::Season => 2,
            Self::Episode => 20,
            Self::MusicVideo => 30,
            Self::RemoteStream => 70,
        }
    }
}

/// Page map for any browser kind. Ports the
/// `LuceneSearchIndex.GetSearchPageMap` algorithm:
///
/// Walk the filtered, sort-title-ordered item list in `page_size` chunks. For
/// every page, look at the bucket of the last item on the page; every bucket
/// up to and including that letter that has not yet been assigned maps to
/// this page (1-based). Unmatched trailing buckets get the final page number
/// so the rightmost letters still scroll to the last page.
#[server]
pub async fn media_page_map(
    kind: MediaBrowserKind,
    search: String,
    page_size: u32,
) -> Result<LetterPageMap, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    if page_size == 0 {
        return Ok(LetterPageMap { pages: vec![0; 27] });
    }

    let state = expect_context::<AppState>();
    let like = format!("%{}%", search.trim());

    let (item_table, metadata_table, fk_column) = kind.tables();
    // `item_table`, `metadata_table`, and `fk_column` are private constants
    // returned by `MediaBrowserKind::tables` — not user input.
    let sql = format!(
        r#"SELECT COALESCE(m."SortTitle", m."Title", '') AS "SortTitle"
           FROM "{item_table}" i
           JOIN "{metadata_table}" m ON m."{fk_column}" = i."Id"
           WHERE (? = '' OR m."Title" LIKE ?)
           ORDER BY m."SortTitle", m."Title""#
    );

    let rows = sqlx::query(&sql)
        .bind(search.trim())
        .bind(&like)
        .fetch_all(&state.pool)
        .await?;

    let buckets: Vec<usize> = rows
        .iter()
        .map(|r| {
            let sort_title: String = r.try_get("SortTitle").unwrap_or_default();
            let letter = sort_title
                .trim()
                .chars()
                .next()
                .map(|c| c.to_ascii_lowercase())
                .filter(char::is_ascii_alphabetic)
                .unwrap_or('#');
            bucket_index(letter)
        })
        .collect();

    let mut map = vec![0_u32; 27];
    let total = buckets.len();
    if total == 0 {
        return Ok(LetterPageMap { pages: map });
    }

    let mut current = 0_usize;
    let mut page: u32 = 0;
    let page_size_usize = page_size as usize;
    while current < total {
        page += 1;
        current += page_size_usize;
        if current > total {
            current = total;
        }
        let jump = buckets[current - 1];
        for (i, slot) in map.iter_mut().enumerate().take(jump + 1) {
            if *slot == 0 {
                *slot = page;
            }
            let _ = i;
        }
    }

    let max = map.iter().copied().max().unwrap_or(0);
    for slot in &mut map {
        if *slot == 0 {
            *slot = max;
        }
    }

    Ok(LetterPageMap { pages: map })
}

/// Add any media items to a collection. Ports the
/// `AddItemsToCollectionHandler` branches: `CollectionItem` only references a
/// `MediaItemId`, so this works uniformly for every kind. Duplicates are
/// skipped via `INSERT OR IGNORE` (PK is composite). Returns the number of
/// rows actually inserted.
#[server]
pub async fn add_media_items_to_collection(
    collection_id: i64,
    media_item_ids: Vec<i64>,
) -> Result<u64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    if media_item_ids.is_empty() {
        return Ok(0);
    }

    let exists: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Collection" WHERE "Id" = ?"#)
        .bind(collection_id)
        .fetch_one(&state.pool)
        .await?;
    if exists == 0 {
        return Err(ServerFnError::new("Collection does not exist."));
    }

    let mut added: u64 = 0;
    for id in media_item_ids {
        let item_exists: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "MediaItem" WHERE "Id" = ?"#)
                .bind(id)
                .fetch_one(&state.pool)
                .await?;
        if item_exists == 0 {
            return Err(ServerFnError::new(format!(
                "Media item {id} does not exist."
            )));
        }

        let result = sqlx::query(
            r#"INSERT OR IGNORE INTO "CollectionItem" ("CollectionId", "MediaItemId") VALUES (?, ?)"#,
        )
        .bind(collection_id)
        .bind(id)
        .execute(&state.pool)
        .await?;
        added += result.rows_affected();
    }
    Ok(added)
}

/// Add media items to a playlist as individual `PlaylistItem` rows. The
/// `CollectionType` is taken from `kind` (see
/// `ErsatzTV.Core/Domain/CollectionType.cs`). Per item:
/// `Index = max(Index) + 1`, `PlaybackOrder = Shuffle (1)`,
/// `IncludeInProgramGuide = 1`, `PlayAll = 0`. Mirrors the per-kind branches
/// of `AddItemsToPlaylistHandler`.
#[server]
pub async fn add_media_items_to_playlist(
    kind: MediaBrowserKind,
    playlist_id: i64,
    media_item_ids: Vec<i64>,
) -> Result<u64, ServerFnError> {
    // PlaybackOrder.Shuffle = 1 (see `ErsatzTV.Core/Domain/PlaybackOrder.cs`).
    const PLAYBACK_ORDER_SHUFFLE: i32 = 1;

    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    if media_item_ids.is_empty() {
        return Ok(0);
    }

    let exists: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Playlist" WHERE "Id" = ?"#)
        .bind(playlist_id)
        .fetch_one(&state.pool)
        .await?;
    if exists == 0 {
        return Err(ServerFnError::new("Playlist does not exist."));
    }

    let max_index: Option<i64> =
        sqlx::query_scalar(r#"SELECT MAX("Index") FROM "PlaylistItem" WHERE "PlaylistId" = ?"#)
            .bind(playlist_id)
            .fetch_one(&state.pool)
            .await?;
    let start_index = max_index.map_or(0, |i| i + 1);

    let collection_type = kind.collection_type();
    let mut added: u64 = 0;
    for (next_index, id) in (start_index..).zip(media_item_ids) {
        let result = sqlx::query(
            r#"INSERT INTO "PlaylistItem"
                   ("Index", "PlaylistId", "CollectionType", "MediaItemId",
                    "IncludeInProgramGuide", "PlaybackOrder", "PlayAll")
               VALUES (?, ?, ?, ?, 1, ?, 0)"#,
        )
        .bind(next_index)
        .bind(playlist_id)
        .bind(collection_type)
        .bind(id)
        .bind(PLAYBACK_ORDER_SHUFFLE)
        .execute(&state.pool)
        .await?;
        added += result.rows_affected();
    }
    Ok(added)
}

/// One row to be inserted into `PlaylistItem` by
/// `add_media_items_to_playlist_mixed`. Computed by
/// [`plan_mixed_playlist_inserts`] so we can unit-test the per-kind branching
/// and `Index` assignment without touching a database.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MixedPlaylistInsert {
    pub index: i64,
    pub collection_type: i32,
    pub media_item_id: i64,
}

/// Expand a `SearchAllIds` payload into the per-row `PlaylistItem` plan,
/// stamping the matching `CollectionType` per kind. Order matches the C#
/// `AddItemsToPlaylistHandler.ApplyAddItemsRequest` iteration:
/// `Movie` → `TelevisionShow` → `TelevisionSeason` → `Episode` → `Artist` →
/// `MusicVideo` → `OtherVideo` → `Song` → `Image` → `RemoteStream`. `Index`
/// is contiguous starting at `start_index` and crosses kind boundaries
/// (matches the C# `index++`).
///
/// `CollectionType` values come from
/// `$ERSATZTV_SRC/ErsatzTV.Core/Domain/CollectionType.cs`.
#[must_use]
pub fn plan_mixed_playlist_inserts(
    start_index: i64,
    ids: &crate::admin::server_fns::search::SearchAllIds,
) -> Vec<MixedPlaylistInsert> {
    // CollectionType values per ErsatzTV.Core/Domain/CollectionType.cs.
    const MOVIE: i32 = 10;
    const TELEVISION_SHOW: i32 = 1;
    const TELEVISION_SEASON: i32 = 2;
    const EPISODE: i32 = 20;
    const ARTIST: i32 = 3;
    const MUSIC_VIDEO: i32 = 30;
    const OTHER_VIDEO: i32 = 40;
    const SONG: i32 = 50;
    const IMAGE: i32 = 60;
    const REMOTE_STREAM: i32 = 70;

    let groups: [(i32, &[i64]); 10] = [
        (MOVIE, &ids.movie_ids),
        (TELEVISION_SHOW, &ids.show_ids),
        (TELEVISION_SEASON, &ids.season_ids),
        (EPISODE, &ids.episode_ids),
        (ARTIST, &ids.artist_ids),
        (MUSIC_VIDEO, &ids.music_video_ids),
        (OTHER_VIDEO, &ids.other_video_ids),
        (SONG, &ids.song_ids),
        (IMAGE, &ids.image_ids),
        (REMOTE_STREAM, &ids.remote_stream_ids),
    ];

    let mut next_index = start_index;
    let mut out = Vec::new();
    for (collection_type, group) in groups {
        for &media_item_id in group {
            out.push(MixedPlaylistInsert {
                index: next_index,
                collection_type,
                media_item_id,
            });
            next_index += 1;
        }
    }
    out
}

/// Add a mixed-kind id payload (typically the output of `search_all_ids`) to a
/// playlist. Ports `AddItemsToPlaylistHandler` directly: one `PlaylistItem`
/// row per id, `CollectionType` stamped per kind, `Index` continuous across
/// the whole insertion starting at `MAX(Index) + 1`. Per row:
/// `PlaybackOrder = Shuffle (1)`, `IncludeInProgramGuide = 1`, `PlayAll = 0`.
///
/// Sibling to the single-kind [`add_media_items_to_playlist`], which the
/// browser pages still use for their per-kind grids.
#[server]
pub async fn add_media_items_to_playlist_mixed(
    playlist_id: i64,
    ids: crate::admin::server_fns::search::SearchAllIds,
) -> Result<u64, ServerFnError> {
    // PlaybackOrder.Shuffle = 1 (see `ErsatzTV.Core/Domain/PlaybackOrder.cs`).
    const PLAYBACK_ORDER_SHUFFLE: i32 = 1;

    use crate::host::state::AppState;

    let state = expect_context::<AppState>();

    let exists: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Playlist" WHERE "Id" = ?"#)
        .bind(playlist_id)
        .fetch_one(&state.pool)
        .await?;
    if exists == 0 {
        return Err(ServerFnError::new("Playlist does not exist."));
    }

    let max_index: Option<i64> =
        sqlx::query_scalar(r#"SELECT MAX("Index") FROM "PlaylistItem" WHERE "PlaylistId" = ?"#)
            .bind(playlist_id)
            .fetch_one(&state.pool)
            .await?;
    let start_index = max_index.map_or(0, |i| i + 1);

    let plan = plan_mixed_playlist_inserts(start_index, &ids);
    if plan.is_empty() {
        return Ok(0);
    }

    let mut added: u64 = 0;
    for row in plan {
        let result = sqlx::query(
            r#"INSERT INTO "PlaylistItem"
                   ("Index", "PlaylistId", "CollectionType", "MediaItemId",
                    "IncludeInProgramGuide", "PlaybackOrder", "PlayAll")
               VALUES (?, ?, ?, ?, 1, ?, 0)"#,
        )
        .bind(row.index)
        .bind(playlist_id)
        .bind(row.collection_type)
        .bind(row.media_item_id)
        .bind(PLAYBACK_ORDER_SHUFFLE)
        .execute(&state.pool)
        .await?;
        added += result.rows_affected();
    }
    Ok(added)
}

// ---------------------------------------------------------------------------
// Collection detail page (`/media/collections/:id`).
//
// Ports `ErsatzTV/Pages/CollectionItems.razor` +
// `ErsatzTV.Application.MediaCards.GetCollectionCardsHandler`. The C# handler
// projects to a `CollectionCardResultsViewModel` with one list per kind. We
// flatten the projection into a single `Vec<CollectionItemCard>` carrying a
// `kind` discriminant — the UI groups by kind for rendering but operates on a
// uniform `(media_item_id, custom_index)` everywhere else.
// ---------------------------------------------------------------------------

/// One card in the collection detail grid. `kind` is the discriminant used by
/// the UI to group cards into sections and to build `href`s.
///
/// `kind` values match those used by `suggest_media_items` and
/// `list_collection_members`: `"movie" | "show" | "season" | "episode" |
/// "artist" | "music_video" | "other_video" | "song" | "image" |
/// "remote_stream"`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CollectionItemCard {
    pub media_item_id: i64,
    pub kind: String,
    pub title: String,
    pub sort_title: String,
    pub subtitle: Option<String>,
    pub artwork_path: Option<String>,
    pub href: Option<String>,
    pub custom_index: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CollectionDetail {
    pub id: i64,
    pub name: String,
    pub use_custom_playback_order: bool,
    pub items: Vec<CollectionItemCard>,
}

/// Loads the full collection detail (header + all card rows). Mirrors the
/// projection performed by `GetCollectionCardsHandler` over the joined
/// `Collection` + `CollectionItem` + per-kind metadata graph.
///
/// Artwork URLs are resolved server-side via `artwork_url(...)` so the
/// returned `artwork_path` is ready to drop into `<img src=...>`.
#[allow(clippy::too_many_lines)]
#[server]
pub async fn get_collection_detail(id: i64) -> Result<Option<CollectionDetail>, ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();

    let Some(collection_row) = sqlx::query(
        r#"SELECT "Id",
                  COALESCE("Name", '') AS "Name",
                  COALESCE("UseCustomPlaybackOrder", 0) AS "UseCustomPlaybackOrder"
           FROM "Collection" WHERE "Id" = ? LIMIT 1"#,
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    else {
        return Ok(None);
    };

    let name: String = collection_row.try_get("Name").unwrap_or_default();
    let use_custom: i64 = collection_row
        .try_get("UseCustomPlaybackOrder")
        .unwrap_or(0);

    // Pull every CollectionItem joined with each possible per-kind metadata.
    // We do this as a single UNION ALL to keep the round-trip count to two.
    // `ArtworkKind = 0` is Poster; `1` is Thumbnail (used for artists, music
    // videos, songs, other videos, images, remote streams in ErsatzTV's
    // `MediaCard` defaults).
    let rows = sqlx::query(
        r#"
        SELECT ci."MediaItemId" AS "Id",
               ci."CustomIndex" AS "CustomIndex",
               'movie' AS "Kind",
               COALESCE(mm."Title", '') AS "Title",
               COALESCE(mm."SortTitle", mm."Title", '') AS "SortTitle",
               CAST(mm."Year" AS TEXT) AS "Subtitle",
               (SELECT a."Path" FROM "Artwork" a
                WHERE a."MovieMetadataId" = mm."Id" AND a."ArtworkKind" = 0 LIMIT 1) AS "Artwork",
               0 AS "ArtworkKindHint"
          FROM "CollectionItem" ci
          JOIN "Movie" m ON m."Id" = ci."MediaItemId"
          JOIN "MovieMetadata" mm ON mm."MovieId" = m."Id"
         WHERE ci."CollectionId" = ?

        UNION ALL

        SELECT ci."MediaItemId" AS "Id",
               ci."CustomIndex" AS "CustomIndex",
               'show' AS "Kind",
               COALESCE(sm."Title", '') AS "Title",
               COALESCE(sm."SortTitle", sm."Title", '') AS "SortTitle",
               CAST(sm."Year" AS TEXT) AS "Subtitle",
               (SELECT a."Path" FROM "Artwork" a
                WHERE a."ShowMetadataId" = sm."Id" AND a."ArtworkKind" = 0 LIMIT 1) AS "Artwork",
               0 AS "ArtworkKindHint"
          FROM "CollectionItem" ci
          JOIN "Show" s ON s."Id" = ci."MediaItemId"
          JOIN "ShowMetadata" sm ON sm."ShowId" = s."Id"
         WHERE ci."CollectionId" = ?

        UNION ALL

        SELECT ci."MediaItemId" AS "Id",
               ci."CustomIndex" AS "CustomIndex",
               'season' AS "Kind",
               COALESCE(sh_meta."Title", '') AS "Title",
               COALESCE(sh_meta."SortTitle", sh_meta."Title", '') AS "SortTitle",
               COALESCE(sea_meta."Title", 'Season ' || CAST(se."SeasonNumber" AS TEXT)) AS "Subtitle",
               (SELECT a."Path" FROM "Artwork" a
                WHERE a."SeasonMetadataId" = sea_meta."Id" AND a."ArtworkKind" = 0 LIMIT 1) AS "Artwork",
               0 AS "ArtworkKindHint"
          FROM "CollectionItem" ci
          JOIN "Season" se ON se."Id" = ci."MediaItemId"
          LEFT JOIN "SeasonMetadata" sea_meta ON sea_meta."SeasonId" = se."Id"
          JOIN "Show" sh ON sh."Id" = se."ShowId"
          JOIN "ShowMetadata" sh_meta ON sh_meta."ShowId" = sh."Id"
         WHERE ci."CollectionId" = ?

        UNION ALL

        SELECT ci."MediaItemId" AS "Id",
               ci."CustomIndex" AS "CustomIndex",
               'episode' AS "Kind",
               COALESCE(em."Title", '') AS "Title",
               COALESCE(em."SortTitle", em."Title", '') AS "SortTitle",
               sh_meta."Title" || ' - S' || CAST(COALESCE(se."SeasonNumber", 0) AS TEXT)
                                || ' E' || CAST(COALESCE(em."EpisodeNumber", 0) AS TEXT) AS "Subtitle",
               (SELECT a."Path" FROM "Artwork" a
                WHERE a."EpisodeMetadataId" = em."Id" AND a."ArtworkKind" = 0 LIMIT 1) AS "Artwork",
               0 AS "ArtworkKindHint"
          FROM "CollectionItem" ci
          JOIN "Episode" ep ON ep."Id" = ci."MediaItemId"
          JOIN "EpisodeMetadata" em ON em."EpisodeId" = ep."Id"
          JOIN "Season" se ON se."Id" = ep."SeasonId"
          JOIN "Show" sh ON sh."Id" = se."ShowId"
          JOIN "ShowMetadata" sh_meta ON sh_meta."ShowId" = sh."Id"
         WHERE ci."CollectionId" = ?

        UNION ALL

        SELECT ci."MediaItemId" AS "Id",
               ci."CustomIndex" AS "CustomIndex",
               'artist' AS "Kind",
               COALESCE(am."Title", '') AS "Title",
               COALESCE(am."SortTitle", am."Title", '') AS "SortTitle",
               am."Disambiguation" AS "Subtitle",
               (SELECT a."Path" FROM "Artwork" a
                WHERE a."ArtistMetadataId" = am."Id" AND a."ArtworkKind" = 1 LIMIT 1) AS "Artwork",
               1 AS "ArtworkKindHint"
          FROM "CollectionItem" ci
          JOIN "Artist" ar ON ar."Id" = ci."MediaItemId"
          JOIN "ArtistMetadata" am ON am."ArtistId" = ar."Id"
         WHERE ci."CollectionId" = ?

        UNION ALL

        SELECT ci."MediaItemId" AS "Id",
               ci."CustomIndex" AS "CustomIndex",
               'music_video' AS "Kind",
               COALESCE(mvm."Title", '') AS "Title",
               COALESCE(mvm."SortTitle", mvm."Title", '') AS "SortTitle",
               mvm."Album" AS "Subtitle",
               (SELECT a."Path" FROM "Artwork" a
                WHERE a."MusicVideoMetadataId" = mvm."Id" AND a."ArtworkKind" = 1 LIMIT 1) AS "Artwork",
               1 AS "ArtworkKindHint"
          FROM "CollectionItem" ci
          JOIN "MusicVideo" mv ON mv."Id" = ci."MediaItemId"
          JOIN "MusicVideoMetadata" mvm ON mvm."MusicVideoId" = mv."Id"
         WHERE ci."CollectionId" = ?

        UNION ALL

        SELECT ci."MediaItemId" AS "Id",
               ci."CustomIndex" AS "CustomIndex",
               'other_video' AS "Kind",
               COALESCE(ovm."Title", '') AS "Title",
               COALESCE(ovm."SortTitle", ovm."Title", '') AS "SortTitle",
               NULL AS "Subtitle",
               (SELECT a."Path" FROM "Artwork" a
                WHERE a."OtherVideoMetadataId" = ovm."Id" AND a."ArtworkKind" = 1 LIMIT 1) AS "Artwork",
               1 AS "ArtworkKindHint"
          FROM "CollectionItem" ci
          JOIN "OtherVideo" ov ON ov."Id" = ci."MediaItemId"
          JOIN "OtherVideoMetadata" ovm ON ovm."OtherVideoId" = ov."Id"
         WHERE ci."CollectionId" = ?

        UNION ALL

        SELECT ci."MediaItemId" AS "Id",
               ci."CustomIndex" AS "CustomIndex",
               'song' AS "Kind",
               COALESCE(sgm."Title", '') AS "Title",
               COALESCE(sgm."SortTitle", sgm."Title", '') AS "SortTitle",
               sgm."Album" AS "Subtitle",
               (SELECT a."Path" FROM "Artwork" a
                WHERE a."SongMetadataId" = sgm."Id" AND a."ArtworkKind" = 1 LIMIT 1) AS "Artwork",
               1 AS "ArtworkKindHint"
          FROM "CollectionItem" ci
          JOIN "Song" sg ON sg."Id" = ci."MediaItemId"
          JOIN "SongMetadata" sgm ON sgm."SongId" = sg."Id"
         WHERE ci."CollectionId" = ?

        UNION ALL

        SELECT ci."MediaItemId" AS "Id",
               ci."CustomIndex" AS "CustomIndex",
               'image' AS "Kind",
               COALESCE(im."Title", '') AS "Title",
               COALESCE(im."SortTitle", im."Title", '') AS "SortTitle",
               NULL AS "Subtitle",
               (SELECT a."Path" FROM "Artwork" a
                WHERE a."ImageMetadataId" = im."Id" AND a."ArtworkKind" = 1 LIMIT 1) AS "Artwork",
               1 AS "ArtworkKindHint"
          FROM "CollectionItem" ci
          JOIN "Image" img ON img."Id" = ci."MediaItemId"
          JOIN "ImageMetadata" im ON im."ImageId" = img."Id"
         WHERE ci."CollectionId" = ?

        UNION ALL

        SELECT ci."MediaItemId" AS "Id",
               ci."CustomIndex" AS "CustomIndex",
               'remote_stream' AS "Kind",
               COALESCE(rsm."Title", '') AS "Title",
               COALESCE(rsm."SortTitle", rsm."Title", '') AS "SortTitle",
               NULL AS "Subtitle",
               (SELECT a."Path" FROM "Artwork" a
                WHERE a."RemoteStreamMetadataId" = rsm."Id" AND a."ArtworkKind" = 1 LIMIT 1) AS "Artwork",
               1 AS "ArtworkKindHint"
          FROM "CollectionItem" ci
          JOIN "RemoteStream" rs ON rs."Id" = ci."MediaItemId"
          JOIN "RemoteStreamMetadata" rsm ON rsm."RemoteStreamId" = rs."Id"
         WHERE ci."CollectionId" = ?

        ORDER BY "Kind", "SortTitle", "Title"
        "#,
    )
    .bind(id)
    .bind(id)
    .bind(id)
    .bind(id)
    .bind(id)
    .bind(id)
    .bind(id)
    .bind(id)
    .bind(id)
    .bind(id)
    .fetch_all(&state.pool)
    .await?;

    let items: Vec<CollectionItemCard> = rows
        .iter()
        .map(|r| {
            let kind: String = r.try_get("Kind").unwrap_or_default();
            let item_id: i64 = r.try_get("Id").unwrap_or(0);
            // ErsatzTV's `MediaCard` defaults to poster artwork; for artists,
            // music videos, etc. the card explicitly opts into thumbnail
            // artwork. We don't yet have a `Thumbnail` variant in
            // `ArtworkUrlKind` (it maps to the same posters cache anyway),
            // so reuse `Poster` for both — the underlying `ArtworkController`
            // routes by stored hash, not by the requested kind.
            let _ = r.try_get::<i64, _>("ArtworkKindHint").unwrap_or(0);
            let artwork_path = r
                .try_get::<Option<String>, _>("Artwork")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster));

            let href = match kind.as_str() {
                "movie" => Some(format!("/media/movies/{item_id}")),
                "show" => Some(format!("/media/tv/shows/{item_id}")),
                _ => None,
            };

            CollectionItemCard {
                media_item_id: item_id,
                kind,
                title: r.try_get("Title").unwrap_or_default(),
                sort_title: r.try_get("SortTitle").unwrap_or_default(),
                subtitle: r.try_get::<Option<String>, _>("Subtitle").unwrap_or(None),
                artwork_path,
                href,
                custom_index: r.try_get("CustomIndex").unwrap_or(0),
            }
        })
        .collect();

    Ok(Some(CollectionDetail {
        id,
        name,
        use_custom_playback_order: use_custom != 0,
        items,
    }))
}

/// Toggles `Collection.UseCustomPlaybackOrder`. Mirrors the
/// `UseCustomPlaybackOrder` branch of `UpdateCollectionHandler`.
#[server]
pub async fn set_collection_use_custom_order(id: i64, on: bool) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    sqlx::query(r#"UPDATE "Collection" SET "UseCustomPlaybackOrder" = ? WHERE "Id" = ?"#)
        .bind(i64::from(on))
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(())
}

/// Removes one or many media items from a collection. Mirrors
/// `RemoveItemsFromCollectionHandler` — note that ErsatzTV also re-queues
/// playout rebuilds for every playout that uses this collection; the playout
/// scheduler in ErsatzRS does not yet have an equivalent enqueue point, so
/// we just persist the removal for now (TODO: wire playout refresh once the
/// scheduling channel exists).
#[server]
pub async fn remove_items_from_collection(
    collection_id: i64,
    media_item_ids: Vec<i64>,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    if media_item_ids.is_empty() {
        return Ok(());
    }
    let mut tx = state.pool.begin().await?;
    for mid in &media_item_ids {
        sqlx::query(
            r#"DELETE FROM "CollectionItem"
               WHERE "CollectionId" = ? AND "MediaItemId" = ?"#,
        )
        .bind(collection_id)
        .bind(mid)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Persists the manual playback order. Mirrors
/// `UpdateCollectionCustomOrderHandler` — writes `CustomIndex` per
/// `(CollectionId, MediaItemId)` based on position in `ordered_item_ids`
/// (0-based, matching the C# handler's input).
#[server]
pub async fn update_collection_custom_order(
    collection_id: i64,
    ordered_item_ids: Vec<i64>,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    if ordered_item_ids.is_empty() {
        return Ok(());
    }
    let mut tx = state.pool.begin().await?;
    for (idx, mid) in ordered_item_ids.iter().enumerate() {
        sqlx::query(
            r#"UPDATE "CollectionItem"
                  SET "CustomIndex" = ?
                WHERE "CollectionId" = ? AND "MediaItemId" = ?"#,
        )
        .bind(i64::try_from(idx).unwrap_or(0))
        .bind(collection_id)
        .bind(mid)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

#[server]
pub async fn list_collection_members(
    collection_id: i64,
) -> Result<Vec<CollectionMember>, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let rows = sqlx::query(
        r#"SELECT ci."MediaItemId" AS "MediaItemId", "Title", "Kind" FROM "CollectionItem" ci
           JOIN (
               SELECT m."Id" AS "Id", COALESCE(mm."Title", '') AS "Title", 'movie' AS "Kind"
               FROM "Movie" m JOIN "MovieMetadata" mm ON mm."MovieId" = m."Id"
               UNION ALL
               SELECT s."Id" AS "Id", COALESCE(sm."Title", '') AS "Title", 'show' AS "Kind"
               FROM "Show" s JOIN "ShowMetadata" sm ON sm."ShowId" = s."Id"
           ) items ON items."Id" = ci."MediaItemId"
           WHERE ci."CollectionId" = ?
           ORDER BY "Title""#,
    )
    .bind(collection_id)
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| CollectionMember {
            media_item_id: r.try_get("MediaItemId").unwrap_or(0),
            title: r.try_get("Title").unwrap_or_default(),
            kind: r.try_get("Kind").unwrap_or_default(),
        })
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;
    use crate::admin::server_fns::search::SearchAllIds;

    #[tokio::test]
    async fn load_show_languages_resolves_episode_audio_stream_codes() {
        let database_dir = tempfile::tempdir().expect("temp database dir");
        let database_url = format!(
            "sqlite://{}",
            database_dir.path().join("show-languages.db").display()
        );
        let pool = ersatzrs_infra::db::init(&database_url)
            .await
            .expect("initialize test database");

        seed_show_language_fixture(&pool).await;

        let languages = load_show_languages(&pool, 10)
            .await
            .expect("load show languages");

        assert_eq!(languages, vec!["Dutch".to_string(), "English".to_string()]);
    }

    #[test]
    fn plan_mixed_playlist_inserts_is_empty_for_empty_payload() {
        let ids = SearchAllIds::default();
        assert!(plan_mixed_playlist_inserts(0, &ids).is_empty());
    }

    #[test]
    fn plan_mixed_playlist_inserts_stamps_collection_type_per_kind() {
        let ids = SearchAllIds {
            movie_ids: vec![101],
            show_ids: vec![201],
            season_ids: vec![301],
            episode_ids: vec![401],
            artist_ids: vec![501],
            music_video_ids: vec![601],
            other_video_ids: vec![701],
            song_ids: vec![801],
            image_ids: vec![901],
            remote_stream_ids: vec![1001],
        };

        let plan = plan_mixed_playlist_inserts(0, &ids);

        // (collection_type, media_item_id) in ErsatzTV iteration order.
        let observed: Vec<(i32, i64)> = plan
            .iter()
            .map(|row| (row.collection_type, row.media_item_id))
            .collect();
        assert_eq!(
            observed,
            vec![
                (10, 101),  // Movie
                (1, 201),   // TelevisionShow
                (2, 301),   // TelevisionSeason
                (20, 401),  // Episode
                (3, 501),   // Artist
                (30, 601),  // MusicVideo
                (40, 701),  // OtherVideo
                (50, 801),  // Song
                (60, 901),  // Image
                (70, 1001), // RemoteStream
            ]
        );
    }

    #[test]
    fn plan_mixed_playlist_inserts_indexes_are_contiguous_across_kinds() {
        let ids = SearchAllIds {
            movie_ids: vec![1, 2],
            show_ids: vec![3],
            episode_ids: vec![4, 5, 6],
            song_ids: vec![7],
            ..SearchAllIds::default()
        };

        let plan = plan_mixed_playlist_inserts(42, &ids);

        let indexes: Vec<i64> = plan.iter().map(|row| row.index).collect();
        assert_eq!(indexes, vec![42, 43, 44, 45, 46, 47, 48]);
        assert_eq!(plan.len(), 7);
        // Empty kinds (seasons/artists/etc.) must not consume an index slot.
        let kinds: Vec<i32> = plan.iter().map(|row| row.collection_type).collect();
        assert_eq!(kinds, vec![10, 10, 1, 20, 20, 20, 50]);
    }

    #[test]
    fn plan_mixed_playlist_inserts_respects_start_index() {
        let ids = SearchAllIds {
            movie_ids: vec![99],
            ..SearchAllIds::default()
        };

        let plan = plan_mixed_playlist_inserts(1000, &ids);
        assert_eq!(
            plan,
            vec![MixedPlaylistInsert {
                index: 1000,
                collection_type: 10,
                media_item_id: 99,
            }]
        );
    }

    async fn seed_show_language_fixture(pool: &ersatzrs_infra::db::DbPool) {
        sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
            .execute(pool)
            .await
            .expect("seed media source");
        sqlx::query(
            r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
               VALUES (1, 'Shows', 2, 1)"#,
        )
        .execute(pool)
        .await
        .expect("seed library");
        sqlx::query(
            r#"INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId") VALUES (1, '/tv', 1)"#,
        )
        .execute(pool)
        .await
        .expect("seed library path");

        for id in [10_i64, 11, 12] {
            sqlx::query(
                r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (?, 1, 0)"#,
            )
            .bind(id)
            .execute(pool)
            .await
            .expect("seed media item");
        }

        sqlx::query(r#"INSERT INTO "Show" ("Id") VALUES (10)"#)
            .execute(pool)
            .await
            .expect("seed show");
        sqlx::query(r#"INSERT INTO "Season" ("Id", "ShowId", "SeasonNumber") VALUES (11, 10, 1)"#)
            .execute(pool)
            .await
            .expect("seed season");
        sqlx::query(r#"INSERT INTO "Episode" ("Id", "SeasonId") VALUES (12, 11)"#)
            .execute(pool)
            .await
            .expect("seed episode");
        sqlx::query(
            r#"INSERT INTO "MediaVersion" (
                   "Id", "DateAdded", "DateUpdated", "Duration", "EpisodeId",
                   "Height", "VideoScanKind", "Width"
               ) VALUES (
                   20, '2026-05-18T00:00:00Z', '2026-05-18T00:00:00Z',
                   '00:22:00', 12, 1080, 0, 1920
               )"#,
        )
        .execute(pool)
        .await
        .expect("seed media version");
        sqlx::query(
            r#"INSERT INTO "MediaStream" (
                   "Id", "Index", "Codec", "MediaStreamKind", "Language", "Channels",
                   "Default", "Forced", "MediaVersionId"
               ) VALUES
                   (30, 0, 'aac', 2, 'eng', 2, 1, 0, 20),
                   (31, 1, 'aac', 2, 'en', 2, 0, 0, 20),
                   (32, 2, 'aac', 2, 'nld', 2, 0, 0, 20),
                   (33, 3, 'ass', 3, 'eng', 0, 0, 0, 20),
                   (34, 4, 'aac', 2, NULL, 2, 0, 0, 20)"#,
        )
        .execute(pool)
        .await
        .expect("seed media streams");
    }
}
