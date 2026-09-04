#[cfg(feature = "ssr")]
use super::{
    expect_context, list_media_cards_from_application_filtered, ServerFnError, TrashPageData,
};
#[cfg(all(test, feature = "ssr"))]
use super::{season_name, song_subtitle, ContentVariantInfo, MediaCardRow};

// ---------------------------------------------------------------------------
// Trash
// ---------------------------------------------------------------------------

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn list_trash_items_impl(
) -> Result<TrashPageData, ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_core::repositories::AdminSearchKind as K;
    use ersatzrs_domain::MediaItemState;

    let state = expect_context::<AppState>();
    let load = |media_kind, kind| {
        list_media_cards_from_application_filtered(
            &state,
            media_kind,
            kind,
            "",
            0,
            u32::MAX,
            true,
            Some(MediaItemState::FileNotFound),
        )
    };
    let (movies, shows, seasons, episodes, artists) = tokio::try_join!(
        load("movie", K::Movie),
        load("show", K::Show),
        load("season", K::Season),
        load("episode", K::Episode),
        load("artist", K::Artist),
    )?;
    let (music_videos, other_videos, songs, images, remote_streams) = tokio::try_join!(
        load("music_video", K::MusicVideo),
        load("other_video", K::OtherVideo),
        load("song", K::Song),
        load("image", K::Image),
        load("remote_stream", K::RemoteStream),
    )?;

    Ok(TrashPageData {
        movies: movies.0,
        shows: shows.0,
        seasons: seasons.0,
        episodes: episodes.0,
        artists: artists.0,
        music_videos: music_videos.0,
        other_videos: other_videos.0,
        songs: songs.0,
        images: images.0,
        remote_streams: remote_streams.0,
    })
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn delete_media_items_impl(
    ids: Vec<i64>,
) -> Result<u64, ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    Ok(state.application.delete_media_items(&ids).await?)
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn empty_trash_impl(
) -> Result<u64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state.application.empty_media_trash().await?)
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn load_trash_movies(
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
            href: None,
            season_number: None,
            variant: ContentVariantInfo::default(),
        })
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn load_trash_shows(
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
            href: None,
            season_number: None,
            variant: ContentVariantInfo::default(),
        })
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn load_trash_seasons(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<Vec<MediaCardRow>, sqlx::Error> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT s."Id" AS "Id",
                  s."SeasonNumber" AS "SeasonNumber",
                  COALESCE(shm."Title", '') AS "ShowTitle",
                  COALESCE(shm."SortTitle", shm."Title", '') AS "ShowSortTitle",
                  COALESCE(sa."Path", sha."Path") AS "Poster"
           FROM "Season" s
           JOIN "MediaItem" mi ON mi."Id" = s."Id" AND mi."State" = 1
           JOIN "Show" sh ON sh."Id" = s."ShowId"
           JOIN "ShowMetadata" shm ON shm."ShowId" = sh."Id"
           LEFT JOIN "SeasonMetadata" sm ON sm."SeasonId" = s."Id"
           LEFT JOIN "Artwork" sa
               ON sa."SeasonMetadataId" = sm."Id" AND sa."ArtworkKind" = 0
           LEFT JOIN "Artwork" sha
               ON sha."ShowMetadataId" = shm."Id" AND sha."ArtworkKind" = 0
           ORDER BY shm."SortTitle", shm."Title", s."SeasonNumber""#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| {
            let number: i64 = r.try_get("SeasonNumber").unwrap_or(0);
            let show_title: String = r.try_get("ShowTitle").unwrap_or_default();
            let show_sort: String = r.try_get("ShowSortTitle").unwrap_or_default();
            MediaCardRow {
                id: r.try_get("Id").unwrap_or(0),
                title: season_name(number),
                sort_title: format!("{show_sort} {number:04}"),
                subtitle: (!show_title.is_empty()).then_some(show_title),
                artwork_path: r
                    .try_get::<Option<String>, _>("Poster")
                    .unwrap_or(None)
                    .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
                href: None,
                season_number: Some(number),
                variant: ContentVariantInfo::default(),
            }
        })
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn load_trash_episodes(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<Vec<MediaCardRow>, sqlx::Error> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT e."Id" AS "Id",
                  COALESCE(em."Title", '') AS "Title",
                  COALESCE(em."SortTitle", em."Title", '') AS "SortTitle",
                  em."EpisodeNumber" AS "EpisodeNumber",
                  s."SeasonNumber" AS "SeasonNumber",
                  COALESCE(shm."Title", '') AS "ShowTitle",
                  COALESCE(ea."Path", sa."Path", sha."Path") AS "Poster"
           FROM "Episode" e
           JOIN "MediaItem" mi ON mi."Id" = e."Id" AND mi."State" = 1
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
           ORDER BY shm."SortTitle", s."SeasonNumber", em."EpisodeNumber", em."SortTitle", em."Title""#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
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
                variant: ContentVariantInfo::default(),
            }
        })
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn load_trash_artists(
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
            href: None,
            season_number: None,
            variant: ContentVariantInfo::default(),
        })
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn load_trash_music_videos(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<Vec<MediaCardRow>, sqlx::Error> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT mv."Id" AS "Id",
                  COALESCE(mvm."Title", '') AS "Title",
                  COALESCE(mvm."SortTitle", mvm."Title", '') AS "SortTitle",
                  am."Title" AS "ArtistName",
                  art."Path" AS "Thumb"
           FROM "MusicVideo" mv
           JOIN "MediaItem" mi ON mi."Id" = mv."Id" AND mi."State" = 1
           JOIN "MusicVideoMetadata" mvm ON mvm."MusicVideoId" = mv."Id"
           LEFT JOIN "Artist" a ON a."Id" = mv."ArtistId"
           LEFT JOIN "ArtistMetadata" am ON am."ArtistId" = a."Id"
           LEFT JOIN "Artwork" art
               ON art."MusicVideoMetadataId" = mvm."Id" AND art."ArtworkKind" = 1
           ORDER BY mvm."SortTitle", mvm."Title""#,
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
                .try_get::<Option<String>, _>("ArtistName")
                .unwrap_or(None)
                .filter(|s| !s.trim().is_empty()),
            artwork_path: r
                .try_get::<Option<String>, _>("Thumb")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
            href: None,
            season_number: None,
            variant: ContentVariantInfo::default(),
        })
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn load_trash_other_videos(
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
            href: None,
            season_number: None,
            variant: ContentVariantInfo::default(),
        })
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn load_trash_songs(
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
            href: None,
            season_number: None,
            variant: ContentVariantInfo::default(),
        })
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn load_trash_images(
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
            href: None,
            season_number: None,
            variant: ContentVariantInfo::default(),
        })
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn load_trash_remote_streams(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<Vec<MediaCardRow>, sqlx::Error> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT rs."Id" AS "Id",
                  COALESCE(rsm."Title", '') AS "Title",
                  COALESCE(rsm."SortTitle", rsm."Title", '') AS "SortTitle",
                  rsm."OriginalTitle" AS "Subtitle",
                  art."Path" AS "Thumb"
           FROM "RemoteStream" rs
           JOIN "MediaItem" mi ON mi."Id" = rs."Id" AND mi."State" = 1
           JOIN "RemoteStreamMetadata" rsm ON rsm."RemoteStreamId" = rs."Id"
           LEFT JOIN "Artwork" art
               ON art."RemoteStreamMetadataId" = rsm."Id" AND art."ArtworkKind" = 1
           ORDER BY rsm."SortTitle", rsm."Title""#,
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
            href: None,
            season_number: None,
            variant: ContentVariantInfo::default(),
        })
        .collect())
}
