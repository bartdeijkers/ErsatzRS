#[cfg(all(test, feature = "ssr"))]
use super::super::content_variant_from_row;
#[cfg(feature = "ssr")]
use super::super::{
    expect_context, list_media_cards_from_application, MediaCardRow, ServerFnError,
};

// ---------------------------------------------------------------------------
// Episodes — "See All" target for the search page's Episodes section.
//
// Subtitle mirrors `search.rs`: "{ShowTitle} - S{season} E{episode}".
// Artwork falls back episode → season → show poster.
// ---------------------------------------------------------------------------

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn list_episodes_impl(
    search: String,
    page: u32,
    page_size: u32,
    show_variants: bool,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    list_media_cards_from_application(
        &state,
        "episode",
        ersatzrs_core::repositories::AdminSearchKind::Episode,
        &search,
        page,
        page_size,
        show_variants,
    )
    .await
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn list_episodes_legacy_sql(
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
        r#"SELECT COUNT(*) FROM "EpisodeMetadata" em
           JOIN "MediaItem" mi ON mi."Id" = em."EpisodeId"
           WHERE (? = '' OR em."Title" LIKE ?)
             AND (? = 1 OR COALESCE(mi."ContentKind", 0) = 0)"#,
    )
    .bind(search.trim())
    .bind(&like)
    .bind(i64::from(show_variants))
    .fetch_one(&state.pool)
    .await?;

    let rows = sqlx::query(
        r#"SELECT e."Id" AS "Id",
                  COALESCE(em."Title", '') AS "Title",
                  COALESCE(em."SortTitle", em."Title", '') AS "SortTitle",
                  em."EpisodeNumber" AS "EpisodeNumber",
                  s."SeasonNumber" AS "SeasonNumber",
                  COALESCE(shm."Title", '') AS "ShowTitle",
                  COALESCE(ea."Path", sa."Path", sha."Path") AS "Poster",
                  COALESCE(mi."ContentKind", 0) AS "ContentKind",
                  mi."SubKind" AS "SubKind"
           FROM "Episode" e
           JOIN "EpisodeMetadata" em ON em."EpisodeId" = e."Id"
           JOIN "MediaItem" mi ON mi."Id" = e."Id"
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
             AND (? = 1 OR COALESCE(mi."ContentKind", 0) = 0)
           ORDER BY em."SortTitle", em."Title"
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
            let show_title: String = r.try_get("ShowTitle").unwrap_or_default();
            let season_number: i64 = r.try_get("SeasonNumber").unwrap_or(0);
            let episode_number: i64 = r.try_get("EpisodeNumber").unwrap_or(0);
            let subtitle = crate::admin::i18n::episode_display_subtitle_text(
                crate::admin::i18n::UiLanguage::English,
                (!show_title.is_empty()).then_some(show_title.as_str()),
                Some(season_number),
                Some(episode_number),
            );
            MediaCardRow {
                id: r.try_get("Id").unwrap_or(0),
                title: r.try_get("Title").unwrap_or_default(),
                sort_title: r.try_get("SortTitle").unwrap_or_default(),
                subtitle,
                artwork_path: r
                    .try_get::<Option<String>, _>("Poster")
                    .unwrap_or(None)
                    .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
                href: None,
                season_number: None,
                variant: content_variant_from_row(r),
            }
        })
        .collect();

    #[allow(clippy::cast_sign_loss)]
    Ok((cards, total.max(0) as u64))
}
