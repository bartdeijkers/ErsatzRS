#[cfg(all(test, feature = "ssr"))]
use super::super::content_variant_from_row;
#[cfg(feature = "ssr")]
use super::super::{
    expect_context, list_media_cards_from_application, ActorCardRow, MediaCardRow,
    OtherVideoDetailRow, ServerFnError,
};

// ---------------------------------------------------------------------------
// Other videos
// ---------------------------------------------------------------------------

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn list_other_videos_impl(
    search: String,
    page: u32,
    page_size: u32,
    show_variants: bool,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    list_media_cards_from_application(
        &state,
        "other_video",
        ersatzrs_core::repositories::AdminSearchKind::OtherVideo,
        &search,
        page,
        page_size,
        show_variants,
    )
    .await
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn list_other_videos_legacy_sql(
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
        r#"SELECT COUNT(*) FROM "OtherVideoMetadata" ovm
           JOIN "MediaItem" mi ON mi."Id" = ovm."OtherVideoId"
           WHERE (? = '' OR ovm."Title" LIKE ?)
             AND (? = 1 OR COALESCE(mi."ContentKind", 0) = 0)"#,
    )
    .bind(search.trim())
    .bind(&like)
    .bind(i64::from(show_variants))
    .fetch_one(&state.pool)
    .await?;

    let rows = sqlx::query(
        r#"SELECT ov."Id" AS "Id",
                  COALESCE(ovm."Title", '') AS "Title",
                  COALESCE(ovm."SortTitle", ovm."Title", '') AS "SortTitle",
                  ovm."OriginalTitle" AS "Subtitle",
                  COALESCE(thumb."Path", poster."Path") AS "Artwork",
                  COALESCE(mi."ContentKind", 0) AS "ContentKind",
                  mi."SubKind" AS "SubKind"
           FROM "OtherVideo" ov
           JOIN "OtherVideoMetadata" ovm ON ovm."OtherVideoId" = ov."Id"
           JOIN "MediaItem" mi ON mi."Id" = ov."Id"
           LEFT JOIN "Artwork" thumb
               ON thumb."OtherVideoMetadataId" = ovm."Id" AND thumb."ArtworkKind" = 1
           LEFT JOIN "Artwork" poster
               ON poster."OtherVideoMetadataId" = ovm."Id" AND poster."ArtworkKind" = 0
           WHERE (? = '' OR ovm."Title" LIKE ?)
             AND (? = 1 OR COALESCE(mi."ContentKind", 0) = 0)
           ORDER BY ovm."SortTitle", ovm."Title"
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
                .try_get::<Option<String>, _>("Artwork")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
            href: None,
            season_number: None,
            variant: content_variant_from_row(r),
        })
        .collect();

    #[allow(clippy::cast_sign_loss)]
    Ok((cards, total.max(0) as u64))
}

/// Other-video details for `/media/other/videos/:id`. ErsatzTV has no
/// separate Razor detail page for this route, so this ports the authoritative
/// other-video data graph and action surface from `OtherVideoList.razor`,
/// `OtherVideoCardViewModel`, and `OtherVideoRepository`.
#[cfg(feature = "ssr")]
#[allow(clippy::too_many_lines)]
pub(in crate::admin::server_fns::media_browser) async fn get_other_video_details_impl(
    id: i64,
) -> Result<Option<OtherVideoDetailRow>, ServerFnError> {
    use crate::api::artwork::{artwork_reference_url as artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let Some(detail) = state.application.get_other_video_detail(id).await? else {
        return Ok(None);
    };

    Ok(Some(OtherVideoDetailRow {
        id: detail.id,
        title: detail.title,
        original_title: detail.original_title,
        year: detail.year,
        plot: detail.plot,
        tagline: detail.tagline,
        content_ratings: detail.content_ratings,
        release_date: detail.release_date,
        artwork_url: detail
            .thumbnail_path
            .and_then(|path| artwork_url(&path, ArtworkUrlKind::Thumbnail))
            .or_else(|| {
                detail
                    .poster_path
                    .and_then(|path| artwork_url(&path, ArtworkUrlKind::Poster))
            }),
        fanart_url: detail
            .fanart_path
            .and_then(|path| artwork_url(&path, ArtworkUrlKind::FanArt)),
        file_path: detail.file_path,
        local_path: detail.local_path,
        media_item_state: detail.media_item_state,
        genres: detail.genres,
        tags: detail.tags,
        languages: detail.facets.languages,
        studios: detail.studios,
        music_labels: detail.facets.music_labels,
        directors: detail.directors,
        writers: detail.writers,
        actors: detail
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
    }))
}
