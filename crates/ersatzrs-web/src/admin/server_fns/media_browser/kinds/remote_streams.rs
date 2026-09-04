#[cfg(all(test, feature = "ssr"))]
use super::super::ContentVariantInfo;
#[cfg(feature = "ssr")]
use super::super::{
    bucket_index, expect_context, list_media_cards_from_application, LetterPageMap, MediaCardRow,
    MediaFacetSetRow, RemoteStreamChannelRow, RemoteStreamChapterRow, RemoteStreamDetailRow,
    RemoteStreamOptionSchemaRow, RemoteStreamOwnerRow, RemoteStreamPlaybackHealthRow,
    RemoteStreamPlaybackHealthStatusRow, RemoteStreamTrackRow, ServerFnError,
};

// ---------------------------------------------------------------------------
// Remote streams — ports `Pages/RemoteStreamList.razor`.
//
// ErsatzTV queries the search index with `type:remote_stream` plus the user's
// query text, so this path must support field queries such as `library_id:7`
// and not just title LIKE filtering.
// ---------------------------------------------------------------------------

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) fn remote_stream_search_query(
    search: &str,
) -> String {
    let trimmed = search.trim();
    if trimmed.is_empty() {
        "type:remote_stream".to_owned()
    } else {
        format!("type:remote_stream AND ({trimmed})")
    }
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn remote_stream_page_map_impl(
    search: String,
    page_size: u32,
) -> Result<LetterPageMap, ServerFnError> {
    use crate::host::state::AppState;

    if page_size == 0 {
        return Ok(LetterPageMap { pages: vec![0; 27] });
    }

    let state = expect_context::<AppState>();
    let mut pages = vec![0_u32; 27];
    let mut documents = state
        .application
        .admin_search_documents(&remote_stream_search_query(&search))
        .await?;
    documents.sort_by_key(|document| {
        document
            .sort_title
            .as_deref()
            .or(document.title.as_deref())
            .unwrap_or_default()
            .to_ascii_lowercase()
    });
    for (index, document) in documents.iter().enumerate() {
        let first = document
            .sort_title
            .as_deref()
            .or(document.title.as_deref())
            .and_then(|title| title.chars().next())
            .unwrap_or('#');
        let bucket = bucket_index(first);
        if pages[bucket] == 0 {
            let page = index / usize::try_from(page_size).unwrap_or(usize::MAX) + 1;
            pages[bucket] = u32::try_from(page).unwrap_or(u32::MAX);
        }
    }

    Ok(LetterPageMap { pages })
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn list_remote_streams_impl(
    search: String,
    page: u32,
    page_size: u32,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    list_media_cards_from_application(
        &state,
        "remote_stream",
        ersatzrs_core::repositories::AdminSearchKind::RemoteStream,
        &search,
        page,
        page_size,
        true,
    )
    .await
}

#[cfg(feature = "ssr")]
pub(in crate::admin::server_fns::media_browser) async fn get_remote_stream_details_impl(
    id: i64,
) -> Result<Option<RemoteStreamDetailRow>, ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let Some(detail) = state.application.get_remote_stream_detail(id).await? else {
        return Ok(None);
    };
    let owners = detail
        .owners
        .into_iter()
        .map(|owner| {
            let option_schema = owner
                .option_schema
                .map(|schema| {
                    let capability =
                        serde_json::from_str::<ersatzrs_addon_contract::AddonCapability>(
                            &schema.definition_json,
                        )
                        .map_err(ServerFnError::new)?;
                    if capability.id != schema.schema.capability_id {
                        return Err(ServerFnError::new(
                            "published option schema identity does not match its definition",
                        ));
                    }
                    Ok(RemoteStreamOptionSchemaRow {
                        definitions: capability.options,
                    })
                })
                .transpose()?;
            Ok(RemoteStreamOwnerRow {
                list_id: owner.list_id,
                list_name: owner.list_name,
                addon_id: owner.addon_id,
                published_revision: owner.published_revision,
                published_effective_options: owner.published_effective_options,
                option_schema,
            })
        })
        .collect::<Result<Vec<_>, ServerFnError>>()?;
    let tracks = detail
        .media_info
        .streams
        .into_iter()
        .map(|stream| RemoteStreamTrackRow {
            index: stream.index,
            kind: stream.kind,
            title: stream.title,
            codec: stream.codec,
            profile: stream.profile,
            language: stream.language,
            channels: stream.channels,
            is_default: stream.is_default,
            is_forced: stream.is_forced,
        })
        .collect();
    let chapters = detail
        .media_info
        .chapters
        .into_iter()
        .map(|chapter| RemoteStreamChapterRow {
            title: chapter.title,
            start_time: chapter.start_time,
            end_time: chapter.end_time,
        })
        .collect();
    let channels = detail
        .channels
        .into_iter()
        .map(|channel| RemoteStreamChannelRow {
            channel_id: channel.channel_id,
            name: channel.name,
            number: channel.number,
            is_enabled: channel.is_enabled,
        })
        .collect();
    let playback_health = RemoteStreamPlaybackHealthRow {
        status: match detail.playback_health.status {
            ersatzrs_core::repositories::AdminRemoteStreamPlaybackHealthStatus::Ready => {
                RemoteStreamPlaybackHealthStatusRow::Ready
            }
            ersatzrs_core::repositories::AdminRemoteStreamPlaybackHealthStatus::NeedsReview => {
                RemoteStreamPlaybackHealthStatusRow::NeedsReview
            }
            ersatzrs_core::repositories::AdminRemoteStreamPlaybackHealthStatus::Blocked => {
                RemoteStreamPlaybackHealthStatusRow::Blocked
            }
        },
    };

    Ok(Some(RemoteStreamDetailRow {
        id: detail.id,
        title: detail.title,
        original_title: detail.original_title,
        sort_title: detail.sort_title,
        year: detail.year,
        plot: detail.plot,
        outline: detail.outline,
        tagline: detail.tagline,
        show_title: detail.show_title,
        season_number: detail.season_number,
        episode_number: detail.episode_number,
        release_date: detail.release_date,
        collection_name: detail.collection_name,
        original_broadcasters: detail.original_broadcasters,
        broadcasters: detail.broadcasters,
        thumbnail_url: detail
            .thumbnail_path
            .and_then(|path| artwork_url(&path, ArtworkUrlKind::Thumbnail)),
        poster_url: detail
            .poster_path
            .and_then(|path| artwork_url(&path, ArtworkUrlKind::Poster)),
        fanart_url: detail
            .fanart_path
            .and_then(|path| artwork_url(&path, ArtworkUrlKind::FanArt)),
        content_kind: detail.content_kind,
        liveness: detail.liveness,
        effective_duration: detail.effective_duration,
        availability: detail.availability,
        availability_reason: detail.availability_reason,
        source_addon_id: detail.source_addon_id,
        facets: MediaFacetSetRow::from(detail.facets),
        tracks,
        chapters,
        owners,
        channels,
        playback_health,
    }))
}

#[cfg(all(test, feature = "ssr"))]
pub(in crate::admin::server_fns::media_browser) async fn list_remote_streams_legacy_sql(
    search: String,
    page: u32,
    page_size: u32,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;
    use sqlx::Row;
    use std::collections::HashMap;

    let state = expect_context::<AppState>();
    let skip = page as usize * page_size as usize;
    let (hits, total, _) = state
        .repositories
        .scheduling
        .search_documents_page(
            &remote_stream_search_query(&search),
            skip,
            page_size as usize,
        )
        .await?;

    let mut thumbs = HashMap::new();
    let ids: Vec<i64> = hits.iter().map(|hit| hit.media_item_id).collect();
    if !ids.is_empty() {
        let mut query = sqlx::QueryBuilder::<ersatzrs_infra::db::Db>::new(
            r#"SELECT rsm."RemoteStreamId" AS "ItemId", art."Path" AS "Thumb"
               FROM "RemoteStreamMetadata" rsm
               LEFT JOIN "Artwork" art
                   ON art."RemoteStreamMetadataId" = rsm."Id" AND art."ArtworkKind" = 1
               WHERE rsm."RemoteStreamId" IN ("#,
        );
        let mut separated = query.separated(", ");
        for id in &ids {
            separated.push_bind(*id);
        }
        separated.push_unseparated(")");

        for row in query.build().fetch_all(&state.pool).await? {
            let id: i64 = row.try_get("ItemId").unwrap_or_default();
            if let Ok(Some(path)) = row.try_get::<Option<String>, _>("Thumb") {
                thumbs.insert(id, path);
            }
        }
    }

    let cards = hits
        .into_iter()
        .map(|hit| {
            let title = hit.title.unwrap_or_default();
            let sort_title = hit.sort_title.unwrap_or_else(|| title.clone());
            MediaCardRow {
                id: hit.media_item_id,
                title,
                sort_title,
                subtitle: hit.original_title.filter(|s| !s.trim().is_empty()),
                artwork_path: thumbs
                    .get(&hit.media_item_id)
                    .and_then(|p| artwork_url(p, ArtworkUrlKind::Poster)),
                href: None,
                season_number: None,
                variant: ContentVariantInfo {
                    content_kind: hit.content_kind,
                    sub_kind: hit.sub_kind,
                    parent_title: hit.parent_title,
                },
            }
        })
        .collect();

    Ok((cards, total as u64))
}
