#[cfg(all(test, feature = "ssr"))]
use super::super::content_variant_from_row;
#[cfg(feature = "ssr")]
use super::super::{
    expect_context, list_media_cards_from_application, MediaCardRow, ServerFnError,
};

// ---------------------------------------------------------------------------
// Seasons — "See All" target for the search page's Seasons section.
//
// Mirrors the season projection in `search.rs`: the card title is
// "Specials" for season 0 else "Season {n}"; subtitle is the show title;
// artwork falls back from the season poster to the show poster.
// ---------------------------------------------------------------------------

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn list_seasons_impl(
    search: String,
    page: u32,
    page_size: u32,
    show_variants: bool,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    list_media_cards_from_application(
        &state,
        "season",
        ersatzrs_core::repositories::AdminSearchKind::Season,
        &search,
        page,
        page_size,
        show_variants,
    )
    .await
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn list_seasons_legacy_sql(
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

    // Seasons are searched by their show's title (seasons have no title of
    // their own); ordering matches `media_page_map`'s SortTitle ordering.
    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "Season" s
           JOIN "Show" sh ON sh."Id" = s."ShowId"
           JOIN "ShowMetadata" shm ON shm."ShowId" = sh."Id"
           JOIN "MediaItem" mi ON mi."Id" = s."Id"
           WHERE (? = '' OR shm."Title" LIKE ?)
             AND (? = 1 OR COALESCE(mi."ContentKind", 0) = 0)"#,
    )
    .bind(search.trim())
    .bind(&like)
    .bind(i64::from(show_variants))
    .fetch_one(&state.pool)
    .await?;

    let rows = sqlx::query(
        r#"SELECT s."Id" AS "Id",
                  s."SeasonNumber" AS "SeasonNumber",
                  COALESCE(shm."Title", '') AS "ShowTitle",
                  COALESCE(shm."SortTitle", shm."Title", '') AS "ShowSortTitle",
                  COALESCE(sa."Path", sha."Path") AS "Poster",
                  COALESCE(mi."ContentKind", 0) AS "ContentKind",
                  mi."SubKind" AS "SubKind"
           FROM "Season" s
           JOIN "Show" sh ON sh."Id" = s."ShowId"
           JOIN "ShowMetadata" shm ON shm."ShowId" = sh."Id"
           JOIN "MediaItem" mi ON mi."Id" = s."Id"
           LEFT JOIN "SeasonMetadata" sm ON sm."SeasonId" = s."Id"
           LEFT JOIN "Artwork" sa
               ON sa."SeasonMetadataId" = sm."Id" AND sa."ArtworkKind" = 0
           LEFT JOIN "Artwork" sha
               ON sha."ShowMetadataId" = shm."Id" AND sha."ArtworkKind" = 0
           WHERE (? = '' OR shm."Title" LIKE ?)
             AND (? = 1 OR COALESCE(mi."ContentKind", 0) = 0)
           ORDER BY shm."SortTitle", shm."Title", s."SeasonNumber"
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
                href: None,
                season_number: Some(number),
                variant: content_variant_from_row(r),
            }
        })
        .collect();

    #[allow(clippy::cast_sign_loss)]
    Ok((cards, total.max(0) as u64))
}
