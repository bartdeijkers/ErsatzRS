#[cfg(all(test, feature = "ssr"))]
use super::PlaylistItemForm;
#[cfg(feature = "ssr")]
use super::{
    expect_context, playlist_editor_from_admin, playlist_editor_into_admin,
    playlist_group_from_admin, playlist_options_from_admin, playlist_row_from_admin,
    PlaylistEditorForm, PlaylistEditorOptions, PlaylistGroupOption, PlaylistGroupRow,
    PlaylistOption, PlaylistPageData, PlaylistPreviewRow, PlaylistRow, ServerFnError,
};

#[cfg(test)]
pub(in crate::admin::server_fns::media_browser) const PLAYLIST_COLLECTION_TYPE_COLLECTION: i32 = 0;
#[cfg(test)]
pub(in crate::admin::server_fns::media_browser) const PLAYLIST_COLLECTION_TYPE_TELEVISION_SHOW:
    i32 = 1;
#[cfg(test)]
pub(in crate::admin::server_fns::media_browser) const PLAYLIST_COLLECTION_TYPE_TELEVISION_SEASON:
    i32 = 2;
#[cfg(test)]
pub(in crate::admin::server_fns::media_browser) const PLAYLIST_COLLECTION_TYPE_ARTIST: i32 = 3;
#[cfg(test)]
pub(in crate::admin::server_fns::media_browser) const PLAYLIST_COLLECTION_TYPE_MULTI_COLLECTION:
    i32 = 4;
#[cfg(test)]
pub(in crate::admin::server_fns::media_browser) const PLAYLIST_COLLECTION_TYPE_SMART_COLLECTION:
    i32 = 5;
#[cfg(test)]
pub(in crate::admin::server_fns::media_browser) const PLAYLIST_COLLECTION_TYPE_MOVIE: i32 = 10;
#[cfg(test)]
pub(in crate::admin::server_fns::media_browser) const PLAYLIST_COLLECTION_TYPE_EPISODE: i32 = 20;
#[cfg(test)]
pub(in crate::admin::server_fns::media_browser) const PLAYLIST_COLLECTION_TYPE_MUSIC_VIDEO: i32 =
    30;
#[cfg(test)]
pub(in crate::admin::server_fns::media_browser) const PLAYLIST_COLLECTION_TYPE_OTHER_VIDEO: i32 =
    40;
#[cfg(test)]
pub(in crate::admin::server_fns::media_browser) const PLAYLIST_COLLECTION_TYPE_SONG: i32 = 50;
#[cfg(test)]
pub(in crate::admin::server_fns::media_browser) const PLAYLIST_COLLECTION_TYPE_IMAGE: i32 = 60;
#[cfg(test)]
pub(in crate::admin::server_fns::media_browser) const PLAYLIST_COLLECTION_TYPE_REMOTE_STREAM: i32 =
    70;

#[cfg(test)]
pub(in crate::admin::server_fns::media_browser) fn playlist_collection_type(
    value: i32,
) -> ersatzrs_domain::CollectionType {
    match value {
        PLAYLIST_COLLECTION_TYPE_TELEVISION_SHOW => ersatzrs_domain::CollectionType::TelevisionShow,
        PLAYLIST_COLLECTION_TYPE_TELEVISION_SEASON => {
            ersatzrs_domain::CollectionType::TelevisionSeason
        }
        PLAYLIST_COLLECTION_TYPE_ARTIST => ersatzrs_domain::CollectionType::Artist,
        PLAYLIST_COLLECTION_TYPE_MULTI_COLLECTION => {
            ersatzrs_domain::CollectionType::MultiCollection
        }
        PLAYLIST_COLLECTION_TYPE_SMART_COLLECTION => {
            ersatzrs_domain::CollectionType::SmartCollection
        }
        PLAYLIST_COLLECTION_TYPE_MOVIE => ersatzrs_domain::CollectionType::Movie,
        PLAYLIST_COLLECTION_TYPE_EPISODE => ersatzrs_domain::CollectionType::Episode,
        PLAYLIST_COLLECTION_TYPE_MUSIC_VIDEO => ersatzrs_domain::CollectionType::MusicVideo,
        PLAYLIST_COLLECTION_TYPE_OTHER_VIDEO => ersatzrs_domain::CollectionType::OtherVideo,
        PLAYLIST_COLLECTION_TYPE_SONG => ersatzrs_domain::CollectionType::Song,
        PLAYLIST_COLLECTION_TYPE_IMAGE => ersatzrs_domain::CollectionType::Image,
        PLAYLIST_COLLECTION_TYPE_REMOTE_STREAM => ersatzrs_domain::CollectionType::RemoteStream,
        _ => ersatzrs_domain::CollectionType::Collection,
    }
}

pub(in crate::admin::server_fns::media_browser) fn playlist_playback_order(
    value: i32,
) -> ersatzrs_domain::PlaybackOrder {
    match value {
        1 => ersatzrs_domain::PlaybackOrder::Chronological,
        2 => ersatzrs_domain::PlaybackOrder::Random,
        3 => ersatzrs_domain::PlaybackOrder::Shuffle,
        4 => ersatzrs_domain::PlaybackOrder::ShuffleInOrder,
        5 => ersatzrs_domain::PlaybackOrder::MultiEpisodeShuffle,
        6 => ersatzrs_domain::PlaybackOrder::SeasonEpisode,
        7 => ersatzrs_domain::PlaybackOrder::RandomRotation,
        8 => ersatzrs_domain::PlaybackOrder::Marathon,
        _ => ersatzrs_domain::PlaybackOrder::None,
    }
}

pub(in crate::admin::server_fns::media_browser) fn channel_subtitle_mode_from_i32(
    value: i32,
) -> Option<ersatzrs_domain::ChannelSubtitleMode> {
    match value {
        1 => Some(ersatzrs_domain::ChannelSubtitleMode::Forced),
        2 => Some(ersatzrs_domain::ChannelSubtitleMode::Default),
        3 => Some(ersatzrs_domain::ChannelSubtitleMode::Any),
        _ => None,
    }
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn get_playlist_page_impl(
) -> Result<PlaylistPageData, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let groups = state
        .application
        .list_playlist_groups()
        .await
        .map_err(ServerFnError::new)?
        .into_iter()
        .map(playlist_group_from_admin)
        .collect();
    Ok(PlaylistPageData { groups })
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn create_playlist_group_impl(
    name: String,
) -> Result<PlaylistGroupRow, ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_app::commands::CreatePlaylistGroup;

    let state = expect_context::<AppState>();
    state
        .application
        .create_playlist_group(CreatePlaylistGroup { name })
        .await
        .map(playlist_group_from_admin)
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn create_playlist_impl(
    playlist_group_id: i64,
    name: String,
) -> Result<PlaylistRow, ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_app::commands::CreatePlaylist;

    let state = expect_context::<AppState>();
    state
        .application
        .create_playlist(CreatePlaylist {
            playlist_group_id,
            name,
        })
        .await
        .map(playlist_row_from_admin)
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn delete_playlist_group_impl(
    playlist_group_id: i64,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_app::commands::DeletePlaylistGroup;

    let state = expect_context::<AppState>();
    state
        .application
        .delete_playlist_group(DeletePlaylistGroup {
            id: playlist_group_id,
        })
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn delete_playlist_impl(
    playlist_id: i64,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_app::commands::DeletePlaylist;

    let state = expect_context::<AppState>();
    state
        .application
        .delete_playlist(DeletePlaylist { id: playlist_id })
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn get_playlist_editor_impl(
    playlist_id: i64,
) -> Result<Option<PlaylistEditorForm>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .get_playlist_editor(playlist_id)
        .await
        .map(|form| form.map(playlist_editor_from_admin))
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn get_playlist_editor_options_impl(
) -> Result<PlaylistEditorOptions, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .get_playlist_editor_options()
        .await
        .map(playlist_options_from_admin)
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn save_playlist_impl(
    form: PlaylistEditorForm,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_app::commands::SavePlaylist;

    let state = expect_context::<AppState>();
    state
        .application
        .save_playlist(SavePlaylist {
            form: playlist_editor_into_admin(form),
        })
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn preview_playlist_playout_impl(
    form: PlaylistEditorForm,
) -> Result<Vec<PlaylistPreviewRow>, ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_app::queries::{PreviewPlaylistItem, PreviewPlaylistPlayout};

    let state = expect_context::<AppState>();
    state
        .application
        .preview_playlist_playout(PreviewPlaylistPlayout {
            playlist_id: form.id,
            items: form
                .items
                .into_iter()
                .map(|item| PreviewPlaylistItem {
                    collection_type: item.collection_type,
                    collection_id: item.collection_id,
                    multi_collection_id: item.multi_collection_id,
                    smart_collection_id: item.smart_collection_id,
                    media_item_id: item.media_item_id,
                    playback_order: playlist_playback_order(item.playback_order),
                    play_all: item.play_all,
                    count: item
                        .count
                        .and_then(|count| usize::try_from(count.max(1)).ok()),
                    include_in_program_guide: item.include_in_program_guide,
                    preferred_audio_language_code: item.preferred_audio_language_code,
                    preferred_audio_title: item.preferred_audio_title,
                    preferred_subtitle_language_code: item.preferred_subtitle_language_code,
                    subtitle_mode: item.subtitle_mode.and_then(channel_subtitle_mode_from_i32),
                })
                .collect(),
        })
        .await
        .map(|rows| {
            rows.into_iter()
                .map(|row| PlaylistPreviewRow {
                    start: format_duration(row.start),
                    finish: format_duration(row.finish),
                    media_item: row.title,
                    duration: format_duration(row.duration),
                })
                .collect()
        })
        .map_err(ServerFnError::new)
}

#[cfg(all(test, feature = "ssr"))]
#[allow(clippy::too_many_lines)]
pub(in crate::admin::server_fns::media_browser) async fn playlist_preview_reference_data(
    pool: &ersatzrs_infra::db::DbPool,
    form: &PlaylistEditorForm,
) -> Result<
    (
        ersatzrs_core::scheduling::SchedulingReferenceData,
        chrono::Duration,
        usize,
    ),
    ServerFnError,
> {
    use chrono::Duration;
    use ersatzrs_core::scheduling::{
        classic_schedule_item_key, SchedulingCollectionKey, SchedulingPlaylistItem,
        SchedulingReferenceData,
    };
    use ersatzrs_domain::{
        CollectionType, FixedStartTimeBehavior, PlaybackOrder, PlayoutMode, ProgramScheduleItem,
        StartType,
    };
    use std::collections::BTreeMap;

    let mut playlist_items = Vec::new();
    let mut one_pass_duration = Duration::zero();
    let mut max_items = 0usize;

    for item in form
        .items
        .iter()
        .filter(|item| item.include_in_program_guide)
    {
        let media_items = preview_media_items_for_playlist_item(pool, item).await?;
        let take = if item.play_all {
            media_items.len()
        } else {
            usize::try_from(item.count.unwrap_or(1).max(1)).unwrap_or(1)
        };
        max_items = max_items.saturating_add(take);
        one_pass_duration += media_items
            .iter()
            .cycle()
            .take(take)
            .fold(Duration::zero(), |duration, media| {
                duration + media.duration
            });
        let preferred_audio_language_code = item.preferred_audio_language_code.clone();
        let preferred_audio_title = item.preferred_audio_title.clone();
        let preferred_subtitle_language_code = item.preferred_subtitle_language_code.clone();
        let subtitle_mode = item.subtitle_mode.and_then(channel_subtitle_mode_from_i32);
        playlist_items.push(SchedulingPlaylistItem {
            collection_key: SchedulingCollectionKey {
                collection_type: playlist_collection_type(item.collection_type),
                collection_id: item.collection_id,
                multi_collection_id: item.multi_collection_id,
                smart_collection_id: item.smart_collection_id,
                media_item_id: item.media_item_id,
                ..SchedulingCollectionKey::default()
            },
            playback_order: playlist_playback_order(item.playback_order),
            play_all: item.play_all,
            count: item
                .count
                .and_then(|count| usize::try_from(count.max(1)).ok()),
            media_items: media_items
                .into_iter()
                .map(|media| ersatzrs_core::scheduling::SchedulingMediaItem {
                    id: media.id,
                    duration: media.duration,
                    liveness: None,
                    title: Some(media.title),
                    show_id: None,
                    season_id: None,
                    artist_id: None,
                    season: None,
                    episode: None,
                    release_date: None,
                    preferred_audio_language_code: preferred_audio_language_code.clone(),
                    preferred_audio_title: preferred_audio_title.clone(),
                    preferred_subtitle_language_code: preferred_subtitle_language_code.clone(),
                    subtitle_mode,
                    pinned_stream_selection: item.pinned_stream_selection.map(|selection| {
                        ersatzrs_domain::PinnedStreamSelection {
                            media_version_id: selection.media_version_id,
                            video_stream_index: selection.video_stream_index,
                            audio_stream_index: selection.audio_stream_index,
                            subtitle_stream_index: selection.subtitle_stream_index,
                        }
                    }),
                })
                .collect(),
        });
    }

    if max_items == 0 || one_pass_duration <= Duration::zero() {
        return Err(ServerFnError::new("Playlist playout preview has no items"));
    }

    let schedule_item = ProgramScheduleItem {
        id: 1,
        program_schedule_id: 1,
        collection_type: CollectionType::Playlist,
        playlist_id: Some(form.id),
        playout_mode: PlayoutMode::Flood,
        playback_order: PlaybackOrder::Shuffle,
        start_type: StartType::Dynamic,
        fixed_start_time_behavior: Some(FixedStartTimeBehavior::Strict),
        ..ProgramScheduleItem::default()
    };
    let schedule_key = classic_schedule_item_key(&schedule_item);
    Ok((
        SchedulingReferenceData {
            classic_schedule_items: vec![schedule_item],
            playlist_items_by_key: BTreeMap::from([(schedule_key, playlist_items)]),
            classic_fixed_start_time_behavior: FixedStartTimeBehavior::Strict,
            ..SchedulingReferenceData::default()
        },
        one_pass_duration,
        max_items,
    ))
}

#[cfg(test)]
pub(in crate::admin::server_fns::media_browser) fn validate_playlist_items(
    items: &[PlaylistItemForm],
) -> Result<(), ServerFnError> {
    for item in items {
        match item.collection_type {
            PLAYLIST_COLLECTION_TYPE_COLLECTION if item.collection_id.is_none() => {
                return Err(ServerFnError::new(
                    "[Collection] is required for collection type 'Collection'",
                ));
            }
            PLAYLIST_COLLECTION_TYPE_TELEVISION_SHOW
            | PLAYLIST_COLLECTION_TYPE_TELEVISION_SEASON
            | PLAYLIST_COLLECTION_TYPE_ARTIST
            | PLAYLIST_COLLECTION_TYPE_MOVIE
            | PLAYLIST_COLLECTION_TYPE_EPISODE
            | PLAYLIST_COLLECTION_TYPE_MUSIC_VIDEO
            | PLAYLIST_COLLECTION_TYPE_OTHER_VIDEO
            | PLAYLIST_COLLECTION_TYPE_SONG
            | PLAYLIST_COLLECTION_TYPE_IMAGE
            | PLAYLIST_COLLECTION_TYPE_REMOTE_STREAM
                if item.media_item_id.is_none() =>
            {
                return Err(ServerFnError::new(format!(
                    "[MediaItem] is required for type '{}'",
                    collection_type_name(item.collection_type)
                )));
            }
            PLAYLIST_COLLECTION_TYPE_MULTI_COLLECTION if item.multi_collection_id.is_none() => {
                return Err(ServerFnError::new(
                    "[MultiCollection] is required for collection type 'MultiCollection'",
                ));
            }
            PLAYLIST_COLLECTION_TYPE_SMART_COLLECTION if item.smart_collection_id.is_none() => {
                return Err(ServerFnError::new(
                    "[SmartCollection] is required for collection type 'SmartCollection'",
                ));
            }
            PLAYLIST_COLLECTION_TYPE_COLLECTION
            | PLAYLIST_COLLECTION_TYPE_TELEVISION_SHOW
            | PLAYLIST_COLLECTION_TYPE_TELEVISION_SEASON
            | PLAYLIST_COLLECTION_TYPE_ARTIST
            | PLAYLIST_COLLECTION_TYPE_MULTI_COLLECTION
            | PLAYLIST_COLLECTION_TYPE_SMART_COLLECTION
            | PLAYLIST_COLLECTION_TYPE_MOVIE
            | PLAYLIST_COLLECTION_TYPE_EPISODE
            | PLAYLIST_COLLECTION_TYPE_MUSIC_VIDEO
            | PLAYLIST_COLLECTION_TYPE_OTHER_VIDEO
            | PLAYLIST_COLLECTION_TYPE_SONG
            | PLAYLIST_COLLECTION_TYPE_IMAGE
            | PLAYLIST_COLLECTION_TYPE_REMOTE_STREAM => {}
            _ => return Err(ServerFnError::new("[CollectionType] is invalid")),
        }
    }
    Ok(())
}

#[cfg(test)]
pub(in crate::admin::server_fns::media_browser) fn collection_type_name(
    collection_type: i32,
) -> &'static str {
    match collection_type {
        PLAYLIST_COLLECTION_TYPE_COLLECTION => "Collection",
        PLAYLIST_COLLECTION_TYPE_TELEVISION_SHOW => "TelevisionShow",
        PLAYLIST_COLLECTION_TYPE_TELEVISION_SEASON => "TelevisionSeason",
        PLAYLIST_COLLECTION_TYPE_ARTIST => "Artist",
        PLAYLIST_COLLECTION_TYPE_MULTI_COLLECTION => "MultiCollection",
        PLAYLIST_COLLECTION_TYPE_SMART_COLLECTION => "SmartCollection",
        PLAYLIST_COLLECTION_TYPE_MOVIE => "Movie",
        PLAYLIST_COLLECTION_TYPE_EPISODE => "Episode",
        PLAYLIST_COLLECTION_TYPE_MUSIC_VIDEO => "MusicVideo",
        PLAYLIST_COLLECTION_TYPE_OTHER_VIDEO => "OtherVideo",
        PLAYLIST_COLLECTION_TYPE_SONG => "Song",
        PLAYLIST_COLLECTION_TYPE_IMAGE => "Image",
        PLAYLIST_COLLECTION_TYPE_REMOTE_STREAM => "RemoteStream",
        _ => "Unknown",
    }
}

#[derive(Debug)]
#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) struct PreviewMediaItem {
    id: i64,
    title: String,
    duration: chrono::Duration,
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn preview_media_items_for_playlist_item(
    pool: &ersatzrs_infra::db::DbPool,
    item: &PlaylistItemForm,
) -> Result<Vec<PreviewMediaItem>, ServerFnError> {
    let media_item_ids = match item.collection_type {
        PLAYLIST_COLLECTION_TYPE_COLLECTION => {
            media_ids_for_collection(pool, item.collection_id).await?
        }
        PLAYLIST_COLLECTION_TYPE_MULTI_COLLECTION => {
            media_ids_for_multi_collection(pool, item.multi_collection_id).await?
        }
        PLAYLIST_COLLECTION_TYPE_SMART_COLLECTION => {
            media_ids_for_smart_collection(pool, item.smart_collection_id).await?
        }
        PLAYLIST_COLLECTION_TYPE_TELEVISION_SHOW => {
            media_ids_for_show(pool, item.media_item_id).await?
        }
        PLAYLIST_COLLECTION_TYPE_TELEVISION_SEASON => {
            media_ids_for_season(pool, item.media_item_id).await?
        }
        PLAYLIST_COLLECTION_TYPE_ARTIST
        | PLAYLIST_COLLECTION_TYPE_MOVIE
        | PLAYLIST_COLLECTION_TYPE_EPISODE
        | PLAYLIST_COLLECTION_TYPE_MUSIC_VIDEO
        | PLAYLIST_COLLECTION_TYPE_OTHER_VIDEO
        | PLAYLIST_COLLECTION_TYPE_SONG
        | PLAYLIST_COLLECTION_TYPE_IMAGE
        | PLAYLIST_COLLECTION_TYPE_REMOTE_STREAM => item.media_item_id.into_iter().collect(),
        _ => Vec::new(),
    };
    load_preview_media_items(pool, media_item_ids).await
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn media_ids_for_smart_collection(
    pool: &ersatzrs_infra::db::DbPool,
    smart_collection_id: Option<i64>,
) -> Result<Vec<i64>, ServerFnError> {
    use std::collections::BTreeSet;

    use ersatzrs_infra::repositories::SqliteSchedulingContentRepository;

    let Some(smart_collection_id) = smart_collection_id else {
        return Ok(Vec::new());
    };
    let Some(query) =
        sqlx::query_scalar::<_, String>(r#"SELECT "Query" FROM "SmartCollection" WHERE "Id" = ?"#)
            .bind(smart_collection_id)
            .fetch_optional(pool)
            .await?
    else {
        return Ok(Vec::new());
    };

    let repository = SqliteSchedulingContentRepository::new(pool.clone());
    let (hits, _, _) = repository.search_documents_page(&query, 0, 0).await?;
    let mut ids = Vec::new();
    let mut seen = BTreeSet::new();
    for hit in hits {
        let hit_ids = match hit.media_kind.as_str() {
            "show" => media_ids_for_show(pool, Some(hit.media_item_id)).await?,
            "season" => media_ids_for_season(pool, Some(hit.media_item_id)).await?,
            "artist" => media_ids_for_artist(pool, Some(hit.media_item_id)).await?,
            _ => vec![hit.media_item_id],
        };
        for id in hit_ids {
            if seen.insert(id) {
                ids.push(id);
            }
        }
    }

    Ok(ids)
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn media_ids_for_collection(
    pool: &ersatzrs_infra::db::DbPool,
    collection_id: Option<i64>,
) -> Result<Vec<i64>, ServerFnError> {
    let Some(collection_id) = collection_id else {
        return Ok(Vec::new());
    };
    Ok(sqlx::query_scalar(
        r#"SELECT "MediaItemId" FROM "CollectionItem" WHERE "CollectionId" = ? ORDER BY "MediaItemId", "Id""#,
    )
    .bind(collection_id)
    .fetch_all(pool)
    .await?)
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn media_ids_for_multi_collection(
    pool: &ersatzrs_infra::db::DbPool,
    multi_collection_id: Option<i64>,
) -> Result<Vec<i64>, ServerFnError> {
    let Some(multi_collection_id) = multi_collection_id else {
        return Ok(Vec::new());
    };
    Ok(sqlx::query_scalar(
        r#"SELECT ci."MediaItemId"
           FROM "MultiCollectionItem" mci
           JOIN "CollectionItem" ci ON ci."CollectionId" = mci."CollectionId"
           WHERE mci."MultiCollectionId" = ?
           ORDER BY mci."CollectionId", ci."MediaItemId""#,
    )
    .bind(multi_collection_id)
    .fetch_all(pool)
    .await?)
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn media_ids_for_artist(
    pool: &ersatzrs_infra::db::DbPool,
    artist_id: Option<i64>,
) -> Result<Vec<i64>, ServerFnError> {
    let Some(artist_id) = artist_id else {
        return Ok(Vec::new());
    };
    Ok(
        sqlx::query_scalar(r#"SELECT "Id" FROM "MusicVideo" WHERE "ArtistId" = ? ORDER BY "Id""#)
            .bind(artist_id)
            .fetch_all(pool)
            .await?,
    )
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn media_ids_for_show(
    pool: &ersatzrs_infra::db::DbPool,
    show_id: Option<i64>,
) -> Result<Vec<i64>, ServerFnError> {
    let Some(show_id) = show_id else {
        return Ok(Vec::new());
    };
    Ok(sqlx::query_scalar(
        r#"SELECT ep."Id"
           FROM "Episode" ep
           JOIN "Season" se ON se."Id" = ep."SeasonId"
           WHERE se."ShowId" = ?
           ORDER BY se."SeasonNumber", ep."Id""#,
    )
    .bind(show_id)
    .fetch_all(pool)
    .await?)
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn media_ids_for_season(
    pool: &ersatzrs_infra::db::DbPool,
    season_id: Option<i64>,
) -> Result<Vec<i64>, ServerFnError> {
    let Some(season_id) = season_id else {
        return Ok(Vec::new());
    };
    Ok(
        sqlx::query_scalar(r#"SELECT "Id" FROM "Episode" WHERE "SeasonId" = ? ORDER BY "Id""#)
            .bind(season_id)
            .fetch_all(pool)
            .await?,
    )
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn load_preview_media_items(
    pool: &ersatzrs_infra::db::DbPool,
    ids: Vec<i64>,
) -> Result<Vec<PreviewMediaItem>, ServerFnError> {
    use chrono::Duration;
    use sqlx::Row;

    let mut result = Vec::new();
    for id in ids {
        let Some(row) = sqlx::query(
            r#"SELECT COALESCE(mm."Title", sm."Title", sem."Title", am."Title",
                              em."Title", mvm."Title", ovm."Title", songm."Title",
                              im."Title", 'Media Item ' || CAST(mi."Id" AS TEXT)) AS "Title",
                      COALESCE(mv."Duration", ev."Duration", muv."Duration",
                               ovv."Duration", sv."Duration", iv."Duration", '00:30:00') AS "Duration"
               FROM "MediaItem" mi
               LEFT JOIN "Movie" movie_item ON movie_item."Id" = mi."Id"
               LEFT JOIN "MovieMetadata" mm ON mm."MovieId" = movie_item."Id"
               LEFT JOIN "MediaVersion" mv ON mv."MovieId" = movie_item."Id"
               LEFT JOIN "Show" show_item ON show_item."Id" = mi."Id"
               LEFT JOIN "ShowMetadata" sm ON sm."ShowId" = show_item."Id"
               LEFT JOIN "Season" season_item ON season_item."Id" = mi."Id"
               LEFT JOIN "SeasonMetadata" sem ON sem."SeasonId" = season_item."Id"
               LEFT JOIN "Artist" artist_item ON artist_item."Id" = mi."Id"
               LEFT JOIN "ArtistMetadata" am ON am."ArtistId" = artist_item."Id"
               LEFT JOIN "Episode" episode_item ON episode_item."Id" = mi."Id"
               LEFT JOIN "EpisodeMetadata" em ON em."EpisodeId" = episode_item."Id"
               LEFT JOIN "MediaVersion" ev ON ev."EpisodeId" = episode_item."Id"
               LEFT JOIN "MusicVideo" music_video_item ON music_video_item."Id" = mi."Id"
               LEFT JOIN "MusicVideoMetadata" mvm ON mvm."MusicVideoId" = music_video_item."Id"
               LEFT JOIN "MediaVersion" muv ON muv."MusicVideoId" = music_video_item."Id"
               LEFT JOIN "OtherVideo" other_video_item ON other_video_item."Id" = mi."Id"
               LEFT JOIN "OtherVideoMetadata" ovm ON ovm."OtherVideoId" = other_video_item."Id"
               LEFT JOIN "MediaVersion" ovv ON ovv."OtherVideoId" = other_video_item."Id"
               LEFT JOIN "Song" song_item ON song_item."Id" = mi."Id"
               LEFT JOIN "SongMetadata" songm ON songm."SongId" = song_item."Id"
               LEFT JOIN "MediaVersion" sv ON sv."SongId" = song_item."Id"
               LEFT JOIN "Image" image_item ON image_item."Id" = mi."Id"
               LEFT JOIN "ImageMetadata" im ON im."ImageId" = image_item."Id"
               LEFT JOIN "MediaVersion" iv ON iv."ImageId" = image_item."Id"
               WHERE mi."Id" = ?
               LIMIT 1"#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await?
        else {
            continue;
        };
        let duration_text: String = row
            .try_get::<Option<String>, _>("Duration")
            .unwrap_or_default()
            .unwrap_or_else(|| "00:30:00".to_string());
        result.push(PreviewMediaItem {
            id,
            title: row
                .try_get("Title")
                .unwrap_or_else(|_| format!("Media Item {id}")),
            duration: parse_hhmmss_duration(&duration_text)
                .unwrap_or_else(|| Duration::minutes(30)),
        });
    }
    Ok(result)
}

#[cfg(test)]
pub(in crate::admin::server_fns::media_browser) fn parse_hhmmss_duration(
    value: &str,
) -> Option<chrono::Duration> {
    let mut parts = value.split(':');
    let hours = parts.next()?.parse::<i64>().ok()?;
    let minutes = parts.next()?.parse::<i64>().ok()?;
    let seconds = parts
        .next()
        .and_then(|part| part.split('.').next())
        .and_then(|part| part.parse::<i64>().ok())?;
    Some(chrono::Duration::seconds(
        hours * 3600 + minutes * 60 + seconds,
    ))
}

pub(in crate::admin::server_fns::media_browser) fn format_duration(
    duration: chrono::Duration,
) -> String {
    let seconds = duration.num_seconds().max(0);
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        (seconds % 3600) / 60,
        seconds % 60
    )
}

/// All playlist groups, ordered system-first then by name. Mirrors
/// `GetAllPlaylistGroupsHandler` (`OrderByDescending(pg => pg.IsSystem)`,
/// `ThenBy(pg => pg.Name)`).
#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn list_all_playlist_groups_impl(
) -> Result<Vec<PlaylistGroupOption>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
        .application
        .list_playlist_groups()
        .await
        .map_err(ServerFnError::new)?
        .into_iter()
        .map(|row| PlaylistGroupOption {
            id: row.id,
            name: row.name,
        })
        .collect())
}

/// Playlists in a given group, ordered by name. Mirrors
/// `GetPlaylistsByPlaylistGroupIdHandler`.
#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn list_playlists_by_group_impl(
    playlist_group_id: i64,
) -> Result<Vec<PlaylistOption>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let groups = state
        .application
        .list_playlist_groups()
        .await
        .map_err(ServerFnError::new)?;
    Ok(groups
        .into_iter()
        .find(|group| group.id == playlist_group_id)
        .map(|group| {
            group
                .playlists
                .into_iter()
                .map(|row| PlaylistOption {
                    id: row.id,
                    name: row.name,
                    playlist_group_id: row.playlist_group_id,
                })
                .collect()
        })
        .unwrap_or_default())
}
