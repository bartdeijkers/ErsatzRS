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

/// Adds movies to a collection. Ports the movie branch of
/// `AddItemsToCollectionHandler`. The C# handler appends to
/// `Collection.MediaItems`; the joined table is `CollectionItem` and
/// duplicates are skipped via `INSERT OR IGNORE` (PK is composite).
/// Returns the number of rows actually inserted (i.e. ignoring duplicates).
#[server]
pub async fn add_movies_to_collection(
    collection_id: i64,
    movie_ids: Vec<i64>,
) -> Result<u64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    if movie_ids.is_empty() {
        return Ok(0);
    }

    // Validate that the collection exists — mirrors `CollectionMustExist`.
    let exists: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Collection" WHERE "Id" = ?"#)
        .bind(collection_id)
        .fetch_one(&state.pool)
        .await?;
    if exists == 0 {
        return Err(ServerFnError::new("Collection does not exist."));
    }

    let mut added: u64 = 0;
    for id in movie_ids {
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

/// Adds movies to a playlist as individual `PlaylistItem` rows of
/// `CollectionType = 10` (Movie). Ports the movie branch of
/// `AddItemsToPlaylistHandler`: each item gets `Index = max(Index) + 1`,
/// `PlaybackOrder = Shuffle (1)`, `IncludeInProgramGuide = 1`.
#[server]
pub async fn add_movies_to_playlist(
    playlist_id: i64,
    movie_ids: Vec<i64>,
) -> Result<u64, ServerFnError> {
    // CollectionType.Movie = 10 (see `ErsatzTV.Core/Domain/CollectionType.cs`).
    const COLLECTION_TYPE_MOVIE: i32 = 10;
    // PlaybackOrder.Shuffle = 1 (see `ErsatzTV.Core/Domain/PlaybackOrder.cs`).
    const PLAYBACK_ORDER_SHUFFLE: i32 = 1;

    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    if movie_ids.is_empty() {
        return Ok(0);
    }

    // Validate that the playlist exists — mirrors `PlaylistMustExist`.
    let exists: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Playlist" WHERE "Id" = ?"#)
        .bind(playlist_id)
        .fetch_one(&state.pool)
        .await?;
    if exists == 0 {
        return Err(ServerFnError::new("Playlist does not exist."));
    }

    // Next index = max(Index) + 1 over the existing items, or 0 if empty.
    let max_index: Option<i64> =
        sqlx::query_scalar(r#"SELECT MAX("Index") FROM "PlaylistItem" WHERE "PlaylistId" = ?"#)
            .bind(playlist_id)
            .fetch_one(&state.pool)
            .await?;
    let start_index = max_index.map_or(0, |i| i + 1);

    let mut added: u64 = 0;
    for (next_index, id) in (start_index..).zip(movie_ids) {
        let result = sqlx::query(
            r#"INSERT INTO "PlaylistItem"
                   ("Index", "PlaylistId", "CollectionType", "MediaItemId",
                    "IncludeInProgramGuide", "PlaybackOrder", "PlayAll")
               VALUES (?, ?, ?, ?, 1, ?, 0)"#,
        )
        .bind(next_index)
        .bind(playlist_id)
        .bind(COLLECTION_TYPE_MOVIE)
        .bind(id)
        .bind(PLAYBACK_ORDER_SHUFFLE)
        .execute(&state.pool)
        .await?;
        added += result.rows_affected();
    }
    Ok(added)
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
