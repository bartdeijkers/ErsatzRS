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

// ---------------------------------------------------------------------------
// Movies
// ---------------------------------------------------------------------------

/// Paginated movie list. Results ordered by `SortTitle`.
#[server]
pub async fn list_movies(
    search: String,
    page: u32,
    page_size: u32,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let offset = i64::from(page) * i64::from(page_size);
    let like = format!("%{}%", search.trim());

    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "MovieMetadata" mm
           WHERE (? = '' OR mm."Title" LIKE ? COLLATE NOCASE)"#,
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
               ON a."MovieMetadataId" = mm."Id" AND a."ArtworkKind" = 1
           WHERE (? = '' OR mm."Title" LIKE ? COLLATE NOCASE)
           ORDER BY mm."SortTitle" COLLATE NOCASE, mm."Title" COLLATE NOCASE
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
            artwork_path: r.try_get::<Option<String>, _>("Poster").unwrap_or(None),
        })
        .collect();

    #[allow(clippy::cast_sign_loss)]
    Ok((cards, total.max(0) as u64))
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
    use crate::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let offset = i64::from(page) * i64::from(page_size);
    let like = format!("%{}%", search.trim());

    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "ShowMetadata" sm
           WHERE (? = '' OR sm."Title" LIKE ? COLLATE NOCASE)"#,
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
               ON a."ShowMetadataId" = sm."Id" AND a."ArtworkKind" = 1
           WHERE (? = '' OR sm."Title" LIKE ? COLLATE NOCASE)
           ORDER BY sm."SortTitle" COLLATE NOCASE, sm."Title" COLLATE NOCASE
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
            artwork_path: r.try_get::<Option<String>, _>("Poster").unwrap_or(None),
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
    use crate::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let offset = i64::from(page) * i64::from(page_size);
    let like = format!("%{}%", search.trim());

    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "ImageMetadata" im
           WHERE (? = '' OR im."Title" LIKE ? COLLATE NOCASE)"#,
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
           WHERE (? = '' OR im."Title" LIKE ? COLLATE NOCASE)
           ORDER BY im."SortTitle" COLLATE NOCASE, im."Title" COLLATE NOCASE
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
            artwork_path: r.try_get::<Option<String>, _>("Thumb").unwrap_or(None),
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
    use crate::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let offset = i64::from(page) * i64::from(page_size);
    let like = format!("%{}%", search.trim());

    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "ArtistMetadata" am
           WHERE (? = '' OR am."Title" LIKE ? COLLATE NOCASE)"#,
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
           WHERE (? = '' OR am."Title" LIKE ? COLLATE NOCASE)
           ORDER BY am."SortTitle" COLLATE NOCASE, am."Title" COLLATE NOCASE
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
            artwork_path: r.try_get::<Option<String>, _>("Thumb").unwrap_or(None),
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
    use crate::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let offset = i64::from(page) * i64::from(page_size);
    let like = format!("%{}%", search.trim());

    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "OtherVideoMetadata" ovm
           WHERE (? = '' OR ovm."Title" LIKE ? COLLATE NOCASE)"#,
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
           WHERE (? = '' OR ovm."Title" LIKE ? COLLATE NOCASE)
           ORDER BY ovm."SortTitle" COLLATE NOCASE, ovm."Title" COLLATE NOCASE
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
            artwork_path: r.try_get::<Option<String>, _>("Artwork").unwrap_or(None),
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
    use crate::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let offset = i64::from(page) * i64::from(page_size);
    let like = format!("%{}%", search.trim());

    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "SongMetadata" sm
           WHERE (? = '' OR sm."Title" LIKE ? COLLATE NOCASE)"#,
    )
    .bind(search.trim())
    .bind(&like)
    .fetch_one(&state.pool)
    .await?;

    let rows = sqlx::query(
        r#"SELECT s."Id" AS "Id",
                  COALESCE(sm."Title", '') AS "Title",
                  COALESCE(sm."SortTitle", sm."Title", '') AS "SortTitle",
                  CASE
                      WHEN COALESCE(sm."Artists", '') = '' AND COALESCE(sm."Album", '') = '' THEN NULL
                      WHEN COALESCE(sm."Artists", '') = '' THEN sm."Album"
                      WHEN COALESCE(sm."Album", '') = '' THEN sm."Artists"
                      ELSE sm."Artists" || ' - ' || sm."Album"
                  END AS "Subtitle",
                  art."Path" AS "Thumb"
           FROM "Song" s
           JOIN "SongMetadata" sm ON sm."SongId" = s."Id"
           LEFT JOIN "Artwork" art
               ON art."SongMetadataId" = sm."Id" AND art."ArtworkKind" = 1
           WHERE (? = '' OR sm."Title" LIKE ? COLLATE NOCASE)
           ORDER BY sm."SortTitle" COLLATE NOCASE, sm."Title" COLLATE NOCASE
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
            artwork_path: r.try_get::<Option<String>, _>("Thumb").unwrap_or(None),
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
    use crate::state::AppState;

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
    use crate::state::AppState;
    use sqlx::{QueryBuilder, Sqlite};

    let state = expect_context::<AppState>();
    if ids.is_empty() {
        return Ok(0);
    }

    let mut builder = QueryBuilder::<Sqlite>::new(r#"DELETE FROM "MediaItem" WHERE "Id" IN ("#);
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
    use crate::state::AppState;

    let state = expect_context::<AppState>();
    let result = sqlx::query(r#"DELETE FROM "MediaItem" WHERE "State" = 1"#)
        .execute(&state.pool)
        .await?;
    Ok(result.rows_affected())
}

async fn load_trash_movies(pool: &sqlx::Pool<sqlx::Sqlite>) -> Result<Vec<MediaCardRow>, sqlx::Error> {
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
               ON a."MovieMetadataId" = mm."Id" AND a."ArtworkKind" = 1
           ORDER BY mm."SortTitle" COLLATE NOCASE, mm."Title" COLLATE NOCASE"#,
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
            artwork_path: r.try_get::<Option<String>, _>("Poster").unwrap_or(None),
        })
        .collect())
}

async fn load_trash_shows(pool: &sqlx::Pool<sqlx::Sqlite>) -> Result<Vec<MediaCardRow>, sqlx::Error> {
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
               ON a."ShowMetadataId" = sm."Id" AND a."ArtworkKind" = 1
           ORDER BY sm."SortTitle" COLLATE NOCASE, sm."Title" COLLATE NOCASE"#,
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
            artwork_path: r.try_get::<Option<String>, _>("Poster").unwrap_or(None),
        })
        .collect())
}

async fn load_trash_artists(pool: &sqlx::Pool<sqlx::Sqlite>) -> Result<Vec<MediaCardRow>, sqlx::Error> {
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
           ORDER BY am."SortTitle" COLLATE NOCASE, am."Title" COLLATE NOCASE"#,
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
            artwork_path: r.try_get::<Option<String>, _>("Thumb").unwrap_or(None),
        })
        .collect())
}

async fn load_trash_other_videos(
    pool: &sqlx::Pool<sqlx::Sqlite>,
) -> Result<Vec<MediaCardRow>, sqlx::Error> {
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
           ORDER BY ovm."SortTitle" COLLATE NOCASE, ovm."Title" COLLATE NOCASE"#,
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
            artwork_path: r.try_get::<Option<String>, _>("Artwork").unwrap_or(None),
        })
        .collect())
}

async fn load_trash_songs(pool: &sqlx::Pool<sqlx::Sqlite>) -> Result<Vec<MediaCardRow>, sqlx::Error> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT s."Id" AS "Id",
                  COALESCE(sm."Title", '') AS "Title",
                  COALESCE(sm."SortTitle", sm."Title", '') AS "SortTitle",
                  CASE
                      WHEN COALESCE(sm."Artists", '') = '' AND COALESCE(sm."Album", '') = '' THEN NULL
                      WHEN COALESCE(sm."Artists", '') = '' THEN sm."Album"
                      WHEN COALESCE(sm."Album", '') = '' THEN sm."Artists"
                      ELSE sm."Artists" || ' - ' || sm."Album"
                  END AS "Subtitle",
                  art."Path" AS "Thumb"
           FROM "Song" s
           JOIN "MediaItem" mi ON mi."Id" = s."Id" AND mi."State" = 1
           JOIN "SongMetadata" sm ON sm."SongId" = s."Id"
           LEFT JOIN "Artwork" art
               ON art."SongMetadataId" = sm."Id" AND art."ArtworkKind" = 1
           ORDER BY sm."SortTitle" COLLATE NOCASE, sm."Title" COLLATE NOCASE"#,
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
            artwork_path: r.try_get::<Option<String>, _>("Thumb").unwrap_or(None),
        })
        .collect())
}

async fn load_trash_images(pool: &sqlx::Pool<sqlx::Sqlite>) -> Result<Vec<MediaCardRow>, sqlx::Error> {
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
           ORDER BY im."SortTitle" COLLATE NOCASE, im."Title" COLLATE NOCASE"#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| MediaCardRow {
            id: r.try_get("Id").unwrap_or(0),
            title: r.try_get("Title").unwrap_or_default(),
            sort_title: r.try_get("SortTitle").unwrap_or_default(),
            subtitle: r.try_get::<Option<String>, _>("ReleaseDate").unwrap_or(None),
            artwork_path: r.try_get::<Option<String>, _>("Thumb").unwrap_or(None),
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
    use crate::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let offset = i64::from(page) * i64::from(page_size);
    let like = format!("%{}%", search.trim());

    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "Collection" c
           WHERE (? = '' OR c."Name" LIKE ? COLLATE NOCASE)"#,
    )
    .bind(search.trim())
    .bind(&like)
    .fetch_one(&state.pool)
    .await?;

    let rows = sqlx::query(
        r#"SELECT c."Id", COALESCE(c."Name", '') AS "Name"
           FROM "Collection" c
           WHERE (? = '' OR c."Name" LIKE ? COLLATE NOCASE)
           ORDER BY c."Name" COLLATE NOCASE
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
    use crate::state::AppState;
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
    use crate::state::AppState;

    let state = expect_context::<AppState>();
    let name = form.name.trim();
    if name.is_empty() {
        return Err(ServerFnError::new("Collection name is required"));
    }

    if form.id == 0 {
        let id = sqlx::query(
            r#"INSERT INTO "Collection" ("Name", "UseCustomPlaybackOrder") VALUES (?, 0)"#,
        )
        .bind(name)
        .execute(&state.pool)
        .await?
        .last_insert_rowid();
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
    use crate::state::AppState;
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
    use crate::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let offset = i64::from(page) * i64::from(page_size);
    let like = format!("%{}%", search.trim());

    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "SmartCollection" sc
           WHERE (? = '' OR sc."Name" LIKE ? COLLATE NOCASE)"#,
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
           WHERE (? = '' OR sc."Name" LIKE ? COLLATE NOCASE)
           ORDER BY sc."Name" COLLATE NOCASE
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
    use crate::state::AppState;
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
    use crate::state::AppState;

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
        let id = sqlx::query(r#"INSERT INTO "SmartCollection" ("Name", "Query") VALUES (?, ?)"#)
            .bind(name)
            .bind(query)
            .execute(&state.pool)
            .await?
            .last_insert_rowid();
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
    use crate::state::AppState;
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
    use crate::state::AppState;
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
               WHERE mm."Title" LIKE ? COLLATE NOCASE
               UNION ALL
               SELECT s."Id" AS "Id",
                      COALESCE(sm."Title", '') AS "Title",
                      'show' AS "Kind",
                      sm."SortTitle" AS "SortTitle"
               FROM "Show" s
               JOIN "ShowMetadata" sm ON sm."ShowId" = s."Id"
               WHERE sm."Title" LIKE ? COLLATE NOCASE
           )
           ORDER BY "SortTitle" COLLATE NOCASE, "Title" COLLATE NOCASE
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
    use crate::state::AppState;
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
    use crate::state::AppState;
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

#[server]
pub async fn list_collection_members(
    collection_id: i64,
) -> Result<Vec<CollectionMember>, ServerFnError> {
    use crate::state::AppState;
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
           ORDER BY "Title" COLLATE NOCASE"#,
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
