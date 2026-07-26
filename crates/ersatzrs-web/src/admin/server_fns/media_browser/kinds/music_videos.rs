#[cfg(all(test, feature = "ssr"))]
use super::super::content_variant_from_row;
#[cfg(feature = "ssr")]
use super::super::{
    expect_context, list_media_cards_from_application, MediaCardRow, ServerFnError,
};

// ---------------------------------------------------------------------------
// Music videos — "See All" target for the search page's Music Videos section.
//
// Subtitle is the owning artist's name; artwork is the thumbnail (kind 1).
// ---------------------------------------------------------------------------

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn list_music_videos_impl(
    search: String,
    page: u32,
    page_size: u32,
    show_variants: bool,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    list_media_cards_from_application(
        &state,
        "music_video",
        ersatzrs_core::repositories::AdminSearchKind::MusicVideo,
        &search,
        page,
        page_size,
        show_variants,
    )
    .await
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn list_music_videos_legacy_sql(
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
        r#"SELECT COUNT(*) FROM "MusicVideoMetadata" mvm
           JOIN "MediaItem" mi ON mi."Id" = mvm."MusicVideoId"
           WHERE (? = '' OR mvm."Title" LIKE ?)
             AND (? = 1 OR COALESCE(mi."ContentKind", 0) = 0)"#,
    )
    .bind(search.trim())
    .bind(&like)
    .bind(i64::from(show_variants))
    .fetch_one(&state.pool)
    .await?;

    let rows = sqlx::query(
        r#"SELECT mv."Id" AS "Id",
                  COALESCE(mvm."Title", '') AS "Title",
                  COALESCE(mvm."SortTitle", mvm."Title", '') AS "SortTitle",
                  am."Title" AS "ArtistName",
                  art."Path" AS "Thumb",
                  COALESCE(mi."ContentKind", 0) AS "ContentKind",
                  mi."SubKind" AS "SubKind"
           FROM "MusicVideo" mv
           JOIN "MusicVideoMetadata" mvm ON mvm."MusicVideoId" = mv."Id"
           JOIN "MediaItem" mi ON mi."Id" = mv."Id"
           LEFT JOIN "Artist" a ON a."Id" = mv."ArtistId"
           LEFT JOIN "ArtistMetadata" am ON am."ArtistId" = a."Id"
           LEFT JOIN "Artwork" art
               ON art."MusicVideoMetadataId" = mvm."Id" AND art."ArtworkKind" = 1
           WHERE (? = '' OR mvm."Title" LIKE ?)
             AND (? = 1 OR COALESCE(mi."ContentKind", 0) = 0)
           ORDER BY mvm."SortTitle", mvm."Title"
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
            subtitle: r
                .try_get::<Option<String>, _>("ArtistName")
                .unwrap_or(None)
                .filter(|s| !s.trim().is_empty()),
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
