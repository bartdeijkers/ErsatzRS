#[cfg(feature = "ssr")]
use super::{expect_context, CollectionMember, ServerFnError};
use super::{Deserialize, Serialize};

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
    /// Surrogate `CollectionItem.Id` — the row identity. ADR 0048 allows the
    /// same media item to appear several times with different playback
    /// overrides, so selection/removal/reordering key on this, not on
    /// `media_item_id`.
    pub collection_item_id: i64,
    pub media_item_id: i64,
    pub kind: String,
    pub title: String,
    pub sort_title: String,
    pub subtitle: Option<String>,
    pub artwork_path: Option<String>,
    pub href: Option<String>,
    pub custom_index: i64,
    pub preferred_audio_language_code: Option<String>,
    pub preferred_audio_title: Option<String>,
    pub preferred_subtitle_language_code: Option<String>,
    pub subtitle_mode: Option<i32>,
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
#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn get_collection_detail_impl(
    id: i64,
) -> Result<Option<CollectionDetail>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .get_collection_detail(id)
        .await
        .map(|detail| detail.map(collection_detail_from_admin))
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) fn collection_detail_from_admin(
    detail: ersatzrs_core::repositories::CollectionDetailAdmin,
) -> CollectionDetail {
    use crate::api::artwork::{artwork_kind_for_media_kind, artwork_url};

    CollectionDetail {
        id: detail.id,
        name: detail.name,
        use_custom_playback_order: detail.use_custom_playback_order,
        items: detail
            .items
            .into_iter()
            .map(|item| {
                let artwork_kind = artwork_kind_for_media_kind(&item.kind);
                let href = match item.kind.as_str() {
                    "movie" => Some(format!("/media/movies/{}", item.media_item_id)),
                    "show" => Some(format!("/media/tv/shows/{}", item.media_item_id)),
                    "other_video" => Some(format!("/media/other/videos/{}", item.media_item_id)),
                    "music_video" => Some(format!("/media/music/videos/{}", item.media_item_id)),
                    "song" => Some(format!("/media/music/songs/{}", item.media_item_id)),
                    "image" => Some(format!("/media/browser/images/{}", item.media_item_id)),
                    _ => None,
                };
                CollectionItemCard {
                    collection_item_id: item.collection_item_id,
                    media_item_id: item.media_item_id,
                    kind: item.kind,
                    title: item.title,
                    sort_title: item.sort_title,
                    subtitle: item.subtitle,
                    artwork_path: item
                        .artwork_path
                        .and_then(|path| artwork_url(&path, artwork_kind)),
                    href,
                    custom_index: item.custom_index,
                    preferred_audio_language_code: item.playback.preferred_audio_language_code,
                    preferred_audio_title: item.playback.preferred_audio_title,
                    preferred_subtitle_language_code: item
                        .playback
                        .preferred_subtitle_language_code,
                    subtitle_mode: item.playback.subtitle_mode,
                }
            })
            .collect(),
    }
}

#[cfg(all(test, feature = "ssr"))]
#[allow(dead_code, clippy::too_many_lines)]
pub(in crate::admin::server_fns::media_browser) async fn get_collection_detail_legacy_sql(
    id: i64,
) -> Result<Option<CollectionDetail>, ServerFnError> {
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
        SELECT ci."Id" AS "CollectionItemId",
               ci."MediaItemId" AS "Id",
               ci."CustomIndex" AS "CustomIndex",
               ci."PreferredAudioLanguageCode" AS "PreferredAudioLanguageCode",
               ci."PreferredAudioTitle" AS "PreferredAudioTitle",
               ci."PreferredSubtitleLanguageCode" AS "PreferredSubtitleLanguageCode",
               ci."SubtitleMode" AS "SubtitleMode",
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

        SELECT ci."Id" AS "CollectionItemId",
               ci."MediaItemId" AS "Id",
               ci."CustomIndex" AS "CustomIndex",
               ci."PreferredAudioLanguageCode" AS "PreferredAudioLanguageCode",
               ci."PreferredAudioTitle" AS "PreferredAudioTitle",
               ci."PreferredSubtitleLanguageCode" AS "PreferredSubtitleLanguageCode",
               ci."SubtitleMode" AS "SubtitleMode",
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

        SELECT ci."Id" AS "CollectionItemId",
               ci."MediaItemId" AS "Id",
               ci."CustomIndex" AS "CustomIndex",
               ci."PreferredAudioLanguageCode" AS "PreferredAudioLanguageCode",
               ci."PreferredAudioTitle" AS "PreferredAudioTitle",
               ci."PreferredSubtitleLanguageCode" AS "PreferredSubtitleLanguageCode",
               ci."SubtitleMode" AS "SubtitleMode",
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

        SELECT ci."Id" AS "CollectionItemId",
               ci."MediaItemId" AS "Id",
               ci."CustomIndex" AS "CustomIndex",
               ci."PreferredAudioLanguageCode" AS "PreferredAudioLanguageCode",
               ci."PreferredAudioTitle" AS "PreferredAudioTitle",
               ci."PreferredSubtitleLanguageCode" AS "PreferredSubtitleLanguageCode",
               ci."SubtitleMode" AS "SubtitleMode",
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

        SELECT ci."Id" AS "CollectionItemId",
               ci."MediaItemId" AS "Id",
               ci."CustomIndex" AS "CustomIndex",
               ci."PreferredAudioLanguageCode" AS "PreferredAudioLanguageCode",
               ci."PreferredAudioTitle" AS "PreferredAudioTitle",
               ci."PreferredSubtitleLanguageCode" AS "PreferredSubtitleLanguageCode",
               ci."SubtitleMode" AS "SubtitleMode",
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

        SELECT ci."Id" AS "CollectionItemId",
               ci."MediaItemId" AS "Id",
               ci."CustomIndex" AS "CustomIndex",
               ci."PreferredAudioLanguageCode" AS "PreferredAudioLanguageCode",
               ci."PreferredAudioTitle" AS "PreferredAudioTitle",
               ci."PreferredSubtitleLanguageCode" AS "PreferredSubtitleLanguageCode",
               ci."SubtitleMode" AS "SubtitleMode",
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

        SELECT ci."Id" AS "CollectionItemId",
               ci."MediaItemId" AS "Id",
               ci."CustomIndex" AS "CustomIndex",
               ci."PreferredAudioLanguageCode" AS "PreferredAudioLanguageCode",
               ci."PreferredAudioTitle" AS "PreferredAudioTitle",
               ci."PreferredSubtitleLanguageCode" AS "PreferredSubtitleLanguageCode",
               ci."SubtitleMode" AS "SubtitleMode",
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

        SELECT ci."Id" AS "CollectionItemId",
               ci."MediaItemId" AS "Id",
               ci."CustomIndex" AS "CustomIndex",
               ci."PreferredAudioLanguageCode" AS "PreferredAudioLanguageCode",
               ci."PreferredAudioTitle" AS "PreferredAudioTitle",
               ci."PreferredSubtitleLanguageCode" AS "PreferredSubtitleLanguageCode",
               ci."SubtitleMode" AS "SubtitleMode",
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

        SELECT ci."Id" AS "CollectionItemId",
               ci."MediaItemId" AS "Id",
               ci."CustomIndex" AS "CustomIndex",
               ci."PreferredAudioLanguageCode" AS "PreferredAudioLanguageCode",
               ci."PreferredAudioTitle" AS "PreferredAudioTitle",
               ci."PreferredSubtitleLanguageCode" AS "PreferredSubtitleLanguageCode",
               ci."SubtitleMode" AS "SubtitleMode",
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

        SELECT ci."Id" AS "CollectionItemId",
               ci."MediaItemId" AS "Id",
               ci."CustomIndex" AS "CustomIndex",
               ci."PreferredAudioLanguageCode" AS "PreferredAudioLanguageCode",
               ci."PreferredAudioTitle" AS "PreferredAudioTitle",
               ci."PreferredSubtitleLanguageCode" AS "PreferredSubtitleLanguageCode",
               ci."SubtitleMode" AS "SubtitleMode",
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

        ORDER BY "Kind", "SortTitle", "Title", "CollectionItemId"
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
            let artwork_kind = if r.try_get::<i64, _>("ArtworkKindHint").unwrap_or(0) == 1 {
                ArtworkUrlKind::Thumbnail
            } else {
                ArtworkUrlKind::Poster
            };
            let artwork_path = r
                .try_get::<Option<String>, _>("Artwork")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, artwork_kind));

            let href = match kind.as_str() {
                "movie" => Some(format!("/media/movies/{item_id}")),
                "show" => Some(format!("/media/tv/shows/{item_id}")),
                "other_video" => Some(format!("/media/other/videos/{item_id}")),
                "music_video" => Some(format!("/media/music/videos/{item_id}")),
                "song" => Some(format!("/media/music/songs/{item_id}")),
                "image" => Some(format!("/media/browser/images/{item_id}")),
                _ => None,
            };

            CollectionItemCard {
                collection_item_id: r.try_get("CollectionItemId").unwrap_or(0),
                media_item_id: item_id,
                kind,
                title: r.try_get("Title").unwrap_or_default(),
                sort_title: r.try_get("SortTitle").unwrap_or_default(),
                subtitle: r.try_get::<Option<String>, _>("Subtitle").unwrap_or(None),
                artwork_path,
                href,
                custom_index: r.try_get("CustomIndex").unwrap_or(0),
                preferred_audio_language_code: r
                    .try_get("PreferredAudioLanguageCode")
                    .unwrap_or_default(),
                preferred_audio_title: r.try_get("PreferredAudioTitle").unwrap_or_default(),
                preferred_subtitle_language_code: r
                    .try_get("PreferredSubtitleLanguageCode")
                    .unwrap_or_default(),
                subtitle_mode: r.try_get("SubtitleMode").unwrap_or_default(),
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
#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn set_collection_use_custom_order_impl(
    id: i64,
    on: bool,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    state
        .application
        .set_collection_custom_order(ersatzrs_app::commands::SetCollectionCustomOrder {
            collection_id: id,
            enabled: on,
        })
        .await
        .map_err(ServerFnError::new)
}

/// Removes one or many rows from a collection. Mirrors
/// `RemoveItemsFromCollectionHandler`, but keys on the surrogate
/// `CollectionItem.Id` so an ADR 0048 duplicate row (same media item,
/// different playback override) can be removed without touching its
/// siblings.
#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn remove_items_from_collection_impl(
    collection_id: i64,
    collection_item_ids: Vec<i64>,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    state
        .application
        .remove_collection_rows(ersatzrs_app::commands::RemoveCollectionRows {
            collection_id,
            row_ids: collection_item_ids,
        })
        .await
        .map_err(ServerFnError::new)
}

/// Persists the manual playback order. Mirrors
/// `UpdateCollectionCustomOrderHandler` — writes `CustomIndex` based on
/// position in `ordered_collection_item_ids` (0-based, matching the C#
/// handler's input), keyed on the surrogate `CollectionItem.Id` so ADR 0048
/// duplicate rows order independently.
#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn update_collection_custom_order_impl(
    collection_id: i64,
    ordered_collection_item_ids: Vec<i64>,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    state
        .application
        .update_collection_custom_order(ersatzrs_app::commands::UpdateCollectionCustomOrder {
            collection_id,
            ordered_row_ids: ordered_collection_item_ids,
        })
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn list_collection_members_impl(
    collection_id: i64,
) -> Result<Vec<CollectionMember>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
        .application
        .list_collection_members(collection_id)
        .await
        .map_err(ServerFnError::new)?
        .into_iter()
        .map(|row| CollectionMember {
            media_item_id: row.media_item_id,
            title: row.title,
            kind: row.kind,
        })
        .collect())
}
