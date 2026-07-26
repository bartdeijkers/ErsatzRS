#[cfg(all(test, feature = "ssr"))]
use super::super::content_variant_from_row;
#[cfg(feature = "ssr")]
use super::super::{
    expect_context, list_media_cards_from_application, ArtistDetailRow, ArtistMusicVideoRow,
    MediaCardRow, ServerFnError,
};
#[cfg(all(test, feature = "ssr"))]
use super::movies::{load_path_replacements_for_library_path, resolve_local_path};

// ---------------------------------------------------------------------------
// Artists
// ---------------------------------------------------------------------------

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn list_artists_impl(
    search: String,
    page: u32,
    page_size: u32,
    show_variants: bool,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    list_media_cards_from_application(
        &state,
        "artist",
        ersatzrs_core::repositories::AdminSearchKind::Artist,
        &search,
        page,
        page_size,
        show_variants,
    )
    .await
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn list_artists_legacy_sql(
    search: String,
    page: u32,
    page_size: u32,
    show_variants: bool,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let offset = i64::from(page) * i64::from(page_size);
    let like = format!("%{}%", search.trim());

    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "ArtistMetadata" am
           JOIN "MediaItem" mi ON mi."Id" = am."ArtistId"
           WHERE (? = '' OR am."Title" LIKE ?)
             AND (? = 1 OR COALESCE(mi."ContentKind", 0) = 0)"#,
    )
    .bind(search.trim())
    .bind(&like)
    .bind(i64::from(show_variants))
    .fetch_one(&state.pool)
    .await?;

    let rows = sqlx::query(
        r#"SELECT a."Id" AS "Id",
                  COALESCE(am."Title", '') AS "Title",
                  COALESCE(am."SortTitle", am."Title", '') AS "SortTitle",
                  am."Disambiguation" AS "Subtitle",
                  art."Path" AS "Thumb",
                  COALESCE(mi."ContentKind", 0) AS "ContentKind",
                  mi."SubKind" AS "SubKind"
           FROM "Artist" a
           JOIN "ArtistMetadata" am ON am."ArtistId" = a."Id"
           JOIN "MediaItem" mi ON mi."Id" = a."Id"
           LEFT JOIN "Artwork" art
               ON art."ArtistMetadataId" = am."Id" AND art."ArtworkKind" = 1
           WHERE (? = '' OR am."Title" LIKE ?)
             AND (? = 1 OR COALESCE(mi."ContentKind", 0) = 0)
           ORDER BY am."SortTitle", am."Title"
           LIMIT ? OFFSET ?"#,
    )
    .bind(search.trim())
    .bind(&like)
    .bind(i64::from(show_variants))
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
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Thumbnail)),
            href: None,
            season_number: None,
            variant: content_variant_from_row(r),
        })
        .collect();

    #[allow(clippy::cast_sign_loss)]
    Ok((cards, total.max(0) as u64))
}

/// Artist details for `/media/music/artists/:id`. Ports
/// `Pages/Artist.razor`, `GetArtistByIdHandler`, and
/// `GetMusicVideoCardsHandler` for the currently implemented DB shape:
/// artist metadata/artwork/facts plus the first 100 music videos sorted by
/// `SortTitle`.
#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn get_artist_details_impl(
    id: i64,
) -> Result<Option<ArtistDetailRow>, ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let Some(detail) = state.application.get_artist_detail(id).await? else {
        return Ok(None);
    };
    let map_music_video =
        |video: ersatzrs_core::repositories::AdminArtistMusicVideo| ArtistMusicVideoRow {
            id: video.id,
            title: video.title,
            sort_title: video.sort_title,
            plot: video.plot,
            album: video.album,
            thumbnail_url: video
                .thumbnail_path
                .and_then(|path| artwork_url(&path, ArtworkUrlKind::Thumbnail)),
            media_item_state: video.media_item_state,
            file_path: video.file_path,
            local_path: video.local_path,
        };

    Ok(Some(ArtistDetailRow {
        id: detail.id,
        title: detail.title,
        disambiguation: detail.disambiguation,
        biography: detail.biography,
        thumbnail_url: detail
            .thumbnail_path
            .and_then(|path| artwork_url(&path, ArtworkUrlKind::Thumbnail)),
        fanart_url: detail
            .fanart_path
            .and_then(|path| artwork_url(&path, ArtworkUrlKind::FanArt)),
        genres: detail.genres,
        styles: detail.styles,
        moods: detail.moods,
        languages: detail.languages,
        music_videos: detail
            .music_videos
            .into_iter()
            .map(map_music_video)
            .collect(),
        versions: detail.versions.into_iter().map(map_music_video).collect(),
    }))
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn load_artist_music_videos(
    pool: &ersatzrs_infra::db::DbPool,
    artist_id: i64,
    versions: bool,
) -> Result<Vec<ArtistMusicVideoRow>, ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT mv."Id" AS "Id",
                  COALESCE(mvm."Title", '') AS "Title",
                  COALESCE(mvm."SortTitle", mvm."Title", '') AS "SortTitle",
                  mvm."Plot" AS "Plot",
                  mvm."Album" AS "Album",
                  art."Path" AS "Thumbnail",
                  mi."State" AS "State",
                  mi."LibraryPathId" AS "LibraryPathId",
                  mf."Path" AS "Path"
           FROM "MusicVideo" mv
           JOIN "MusicVideoMetadata" mvm ON mvm."MusicVideoId" = mv."Id"
           JOIN "MediaItem" mi ON mi."Id" = mv."Id"
           LEFT JOIN "Artwork" art
               ON art."MusicVideoMetadataId" = mvm."Id" AND art."ArtworkKind" = 1
           LEFT JOIN "MediaVersion" ver
               ON ver."MusicVideoId" = mv."Id"
              AND ver."Id" = (
                  SELECT "Id" FROM "MediaVersion"
                  WHERE "MusicVideoId" = mv."Id"
                  ORDER BY "Id"
                  LIMIT 1
              )
           LEFT JOIN "MediaFile" mf
               ON mf."MediaVersionId" = ver."Id"
              AND mf."Id" = (
                  SELECT "Id" FROM "MediaFile"
                  WHERE "MediaVersionId" = ver."Id"
                  ORDER BY "Id"
                  LIMIT 1
              )
           WHERE mv."ArtistId" = ?
             AND COALESCE(mi."ContentKind", 0) = ?
           ORDER BY mvm."SortTitle", mvm."Title"
           LIMIT 100"#,
    )
    .bind(artist_id)
    .bind(if versions { 2_i64 } else { 0_i64 })
    .fetch_all(pool)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let file_path: Option<String> = r.try_get::<Option<String>, _>("Path").unwrap_or(None);
        let library_path_id: Option<i64> = r.try_get("LibraryPathId").unwrap_or(None);
        let local_path = match (file_path.as_deref(), library_path_id) {
            (Some(path), Some(library_path_id)) => {
                let replacements =
                    load_path_replacements_for_library_path(pool, library_path_id).await?;
                Some(resolve_local_path(path, &replacements))
            }
            (Some(path), None) => Some(path.to_owned()),
            (None, _) => None,
        };

        out.push(ArtistMusicVideoRow {
            id: r.try_get("Id").unwrap_or(0),
            title: r.try_get("Title").unwrap_or_default(),
            sort_title: r.try_get("SortTitle").unwrap_or_default(),
            plot: r.try_get::<Option<String>, _>("Plot").unwrap_or(None),
            album: r.try_get::<Option<String>, _>("Album").unwrap_or(None),
            thumbnail_url: r
                .try_get::<Option<String>, _>("Thumbnail")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Thumbnail)),
            media_item_state: r.try_get("State").unwrap_or(0),
            file_path,
            local_path,
        });
    }

    Ok(out)
}
