use super::{
    is_anamorphic, nonempty_strings, sort_title, Duration, JellyfinActor, JellyfinChapterResponse,
    JellyfinCollectionItem, JellyfinCollectionMediaItem, JellyfinCollectionMediaKind,
    JellyfinLibraryItemResponse, JellyfinLibraryResponse, JellyfinLibrarySummary,
    JellyfinMediaItem, JellyfinMediaStreamResponse, JellyfinPathInfo, JellyfinPersonResponse,
    JellyfinPlaybackInfoResponse, JellyfinPlaybackMetadata, JellyfinProviderIdsResponse,
    JellyfinSeasonItem, JellyfinShowItem, LibraryMediaKind, MediaChapter, MediaStream,
    MediaStreamKind, Utc,
};

fn guids_from_provider_ids(provider_ids: &JellyfinProviderIdsResponse) -> Vec<String> {
    crate::media_server::jellyfin_emby::guids_from_provider_ids(
        provider_ids.imdb.as_deref(),
        provider_ids.tmdb.as_deref(),
        provider_ids.tvdb.as_deref(),
    )
}

pub(super) fn project_library(response: JellyfinLibraryResponse) -> Option<JellyfinLibrarySummary> {
    let media_kind = match response.collection_type?.to_ascii_lowercase().as_str() {
        "movies" => LibraryMediaKind::Movies,
        "tvshows" => LibraryMediaKind::Shows,
        _ => return None,
    };
    let path_infos = response
        .library_options
        .map(|options| {
            options
                .path_infos
                .into_iter()
                .filter_map(|info| {
                    let network_path = info.network_path?;
                    if network_path.trim().is_empty() {
                        return None;
                    }
                    Some(JellyfinPathInfo {
                        path: info.path.unwrap_or_default(),
                        network_path,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Some(JellyfinLibrarySummary {
        item_id: response.item_id?,
        name: response.name.unwrap_or_default(),
        media_kind,
        path_infos,
    })
}

pub(super) fn project_show(item: JellyfinLibraryItemResponse) -> Option<JellyfinShowItem> {
    let name = item.name.unwrap_or_default();
    let release_date = item.premiere_date;
    let tagline = item
        .taglines
        .iter()
        .find(|tagline| !tagline.trim().is_empty())
        .cloned();
    let item_id = item.id.as_deref().unwrap_or_default();
    let poster_artwork_path = item
        .image_tags
        .primary
        .as_deref()
        .filter(|tag| !tag.trim().is_empty())
        .map(|tag| format!("jellyfin://Items/{item_id}/Images/Primary?tag={tag}"));
    let thumb_artwork_path = item
        .image_tags
        .thumb
        .as_deref()
        .filter(|tag| !tag.trim().is_empty())
        .map(|tag| format!("jellyfin://Items/{item_id}/Images/Thumb?tag={tag}"));
    let fanart_artwork_path = item
        .backdrop_image_tags
        .iter()
        .find(|tag| !tag.trim().is_empty())
        .map(|tag| format!("jellyfin://Items/{item_id}/Images/Backdrop?tag={tag}"));
    let provider_guids = guids_from_provider_ids(&item.provider_ids);
    let genres = nonempty_strings(item.genres);
    let tags = nonempty_strings(item.tags);
    let studios = item
        .studios
        .into_iter()
        .filter_map(|s| s.name)
        .filter(|name| !name.trim().is_empty())
        .collect();
    let actors = project_actors(&item.people);
    Some(JellyfinShowItem {
        item_id: item.id?,
        etag: item.etag,
        sort_title: sort_title(&name),
        plot: item.overview,
        year: item.production_year,
        tagline,
        content_rating: item.official_rating,
        release_date,
        poster_artwork_path,
        thumb_artwork_path,
        fanart_artwork_path,
        provider_guids,
        genres,
        tags,
        studios,
        actors,
        name,
        date_created: item.date_created.unwrap_or_else(Utc::now),
    })
}

pub(super) fn project_actors(people: &[JellyfinPersonResponse]) -> Vec<JellyfinActor> {
    people
        .iter()
        .filter(|person| {
            person
                .person_type
                .as_deref()
                .is_some_and(|person_type| person_type.eq_ignore_ascii_case("actor"))
        })
        .enumerate()
        .filter_map(|(idx, person)| {
            let name = person
                .name
                .as_ref()
                .filter(|name| !name.trim().is_empty())?
                .clone();
            let role = person
                .role
                .as_ref()
                .filter(|role| !role.trim().is_empty())
                .cloned();
            Some(JellyfinActor {
                name,
                role,
                order: Some(i32::try_from(idx).unwrap_or(i32::MAX)),
            })
        })
        .collect()
}

pub(super) fn project_season(item: JellyfinLibraryItemResponse) -> Option<JellyfinSeasonItem> {
    let name = item.name.unwrap_or_default();
    let item_id = item.id.as_deref().unwrap_or_default();
    let poster_artwork_path = item
        .image_tags
        .primary
        .as_deref()
        .filter(|tag| !tag.trim().is_empty())
        .map(|tag| format!("jellyfin://Items/{item_id}/Images/Primary?tag={tag}"));
    let fanart_artwork_path = item
        .backdrop_image_tags
        .iter()
        .find(|tag| !tag.trim().is_empty())
        .map(|tag| format!("jellyfin://Items/{item_id}/Images/Backdrop?tag={tag}"));
    let provider_guids = guids_from_provider_ids(&item.provider_ids);
    Some(JellyfinSeasonItem {
        item_id: item.id?,
        etag: item.etag,
        sort_title: sort_title(&name),
        year: item.production_year,
        release_date: item.premiere_date,
        poster_artwork_path,
        fanart_artwork_path,
        provider_guids,
        date_created: item.date_created.unwrap_or_else(Utc::now),
        name,
        season_number: item.index_number.unwrap_or_default(),
    })
}

pub(super) fn project_collection(
    item: JellyfinLibraryItemResponse,
) -> Option<JellyfinCollectionItem> {
    Some(JellyfinCollectionItem {
        item_id: item.id?,
        etag: item.etag,
        name: item.name.unwrap_or_default(),
    })
}

pub(super) fn project_collection_media_item(
    item: JellyfinLibraryItemResponse,
) -> Option<JellyfinCollectionMediaItem> {
    if item.location_type.as_deref() != Some("FileSystem") {
        return None;
    }
    let kind = match item.item_type.as_deref()? {
        "Movie" => JellyfinCollectionMediaKind::Movie,
        "Series" => JellyfinCollectionMediaKind::Show,
        "Season" => JellyfinCollectionMediaKind::Season,
        "Episode" => JellyfinCollectionMediaKind::Episode,
        _ => return None,
    };
    Some(JellyfinCollectionMediaItem {
        item_id: item.id?,
        kind,
    })
}

pub(super) fn project_movie(item: JellyfinLibraryItemResponse) -> Option<JellyfinMediaItem> {
    if item.location_type.as_deref() != Some("FileSystem") {
        return None;
    }
    let path = item.path?;
    if path.to_ascii_lowercase().ends_with(".strm") {
        return None;
    }
    let duration = Duration::nanoseconds(item.run_time_ticks * 100);
    let name = item.name.unwrap_or_default();
    let release_date = item.premiere_date;
    let tagline = item
        .taglines
        .iter()
        .find(|tagline| !tagline.trim().is_empty())
        .cloned();
    let poster_artwork_path = item
        .image_tags
        .primary
        .filter(|tag| !tag.trim().is_empty())
        .map(|tag| {
            format!(
                "jellyfin://Items/{}/Images/Primary?tag={tag}",
                item.id.as_deref().unwrap_or_default()
            )
        });
    let fanart_artwork_path = item
        .backdrop_image_tags
        .iter()
        .find(|tag| !tag.trim().is_empty())
        .map(|tag| {
            format!(
                "jellyfin://Items/{}/Images/Backdrop?tag={tag}",
                item.id.as_deref().unwrap_or_default()
            )
        });
    let provider_guids = guids_from_provider_ids(&item.provider_ids);
    let genres = nonempty_strings(item.genres);
    let tags = nonempty_strings(item.tags);
    let actors = project_actors(&item.people);
    let directors = project_person_names(&item.people, "Director");
    let writers = project_person_names(&item.people, "Writer");
    Some(JellyfinMediaItem {
        item_id: item.id?,
        etag: item.etag,
        sort_title: sort_title(&name),
        plot: item.overview,
        year: item.production_year,
        tagline,
        content_rating: item.official_rating,
        release_date,
        episode_number: None,
        poster_artwork_path,
        thumb_artwork_path: None,
        fanart_artwork_path,
        provider_guids,
        genres,
        tags,
        actors,
        directors,
        writers,
        name,
        path,
        date_created: item.date_created.unwrap_or_else(Utc::now),
        duration,
        chapters: project_chapters(item.chapters, duration),
    })
}

pub(super) fn project_episode(item: JellyfinLibraryItemResponse) -> Option<JellyfinMediaItem> {
    if item.location_type.as_deref() != Some("FileSystem") {
        return None;
    }
    let path = item.path?;
    if path.to_ascii_lowercase().ends_with(".strm") {
        return None;
    }
    let duration = Duration::nanoseconds(item.run_time_ticks * 100);
    let name = item.name.unwrap_or_default();
    let item_id = item.id.as_deref().unwrap_or_default();
    let thumb_artwork_path = item
        .image_tags
        .primary
        .as_deref()
        .filter(|tag| !tag.trim().is_empty())
        .map(|tag| format!("jellyfin://Items/{item_id}/Images/Primary?tag={tag}"));
    let provider_guids = guids_from_provider_ids(&item.provider_ids);
    let genres = nonempty_strings(item.genres);
    let tags = nonempty_strings(item.tags);
    let actors = project_actors(&item.people);
    let directors = project_person_names(&item.people, "Director");
    let writers = project_person_names(&item.people, "Writer");
    Some(JellyfinMediaItem {
        item_id: item.id?,
        etag: item.etag,
        sort_title: sort_title(&name),
        plot: item.overview,
        year: item.production_year,
        tagline: None,
        content_rating: None,
        release_date: item.premiere_date,
        episode_number: item.index_number,
        poster_artwork_path: None,
        thumb_artwork_path,
        fanart_artwork_path: None,
        provider_guids,
        genres,
        tags,
        actors,
        directors,
        writers,
        name,
        path,
        date_created: item.date_created.unwrap_or_else(Utc::now),
        duration,
        chapters: project_chapters(item.chapters, duration),
    })
}

pub(super) fn project_person_names(
    people: &[JellyfinPersonResponse],
    person_type: &str,
) -> Vec<String> {
    people
        .iter()
        .filter(|person| {
            person
                .person_type
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case(person_type))
        })
        .filter_map(|person| person.name.as_ref())
        .filter(|name| !name.trim().is_empty())
        .cloned()
        .collect()
}

pub(super) fn project_chapters(
    chapters: Vec<JellyfinChapterResponse>,
    duration: Duration,
) -> Vec<MediaChapter> {
    let mut chapters: Vec<MediaChapter> = chapters
        .into_iter()
        .enumerate()
        .map(|(index, chapter)| MediaChapter {
            chapter_id: i64::try_from(index).unwrap_or_default(),
            title: chapter.name.unwrap_or_default(),
            start_time: Duration::nanoseconds(chapter.start_position_ticks * 100),
            ..MediaChapter::default()
        })
        .collect();
    chapters.sort_by_key(|chapter| chapter.start_time);
    for index in 0..chapters.len() {
        chapters[index].chapter_id = i64::try_from(index).unwrap_or_default();
        chapters[index].end_time = chapters
            .get(index + 1)
            .map_or(duration, |next| next.start_time);
    }
    chapters
}

pub(super) fn project_playback(
    response: JellyfinPlaybackInfoResponse,
) -> Option<JellyfinPlaybackMetadata> {
    let media_source = response.media_sources.into_iter().next()?;
    let video = media_source
        .media_streams
        .iter()
        .find(|stream| stream.stream_type.as_deref() == Some("Video"))?;
    let width = video.width.unwrap_or(1);
    let height = video.height.unwrap_or(1);
    let display_aspect_ratio = video.aspect_ratio.clone().filter(|value| !value.is_empty());
    let sample_aspect_ratio = Some(
        if display_aspect_ratio
            .as_deref()
            .is_some_and(|aspect| is_anamorphic(width, height, aspect))
        {
            "0:0".to_owned()
        } else {
            "1:1".to_owned()
        },
    );

    Some(JellyfinPlaybackMetadata {
        duration: Duration::nanoseconds(media_source.run_time_ticks * 100),
        width,
        height,
        sample_aspect_ratio,
        display_aspect_ratio,
        r_frame_rate: video.real_frame_rate.map(|rate| format!("{rate:.2}")),
        streams: media_source
            .media_streams
            .into_iter()
            .filter_map(project_stream)
            .collect(),
    })
}

pub(super) fn project_stream(stream: JellyfinMediaStreamResponse) -> Option<MediaStream> {
    let kind = match stream.stream_type.as_deref()? {
        "Video" => MediaStreamKind::Video,
        "Audio" => MediaStreamKind::Audio,
        "Subtitle" if stream.is_external => MediaStreamKind::ExternalSubtitle,
        "Subtitle" => MediaStreamKind::Subtitle,
        _ => return None,
    };

    let is_video = matches!(kind, MediaStreamKind::Video);
    Some(MediaStream {
        index: stream.index,
        kind,
        codec: stream.codec.map(|codec| codec.to_ascii_lowercase()),
        profile: stream
            .profile
            .as_ref()
            .map(|value| value.to_ascii_lowercase()),
        language: stream.language,
        title: stream.title,
        default: stream.is_default,
        forced: stream.is_forced,
        channels: stream.channels,
        width: stream.width,
        height: stream.height,
        pixel_format: if is_video {
            stream.pixel_format.map(|value| value.to_ascii_lowercase())
        } else {
            None
        },
        color_range: if is_video {
            stream.color_range.map(|value| value.to_ascii_lowercase())
        } else {
            None
        },
        color_space: if is_video {
            stream.color_space.map(|value| value.to_ascii_lowercase())
        } else {
            None
        },
        color_transfer: if is_video {
            stream
                .color_transfer
                .map(|value| value.to_ascii_lowercase())
        } else {
            None
        },
        color_primaries: if is_video {
            stream
                .color_primaries
                .map(|value| value.to_ascii_lowercase())
        } else {
            None
        },
        bits_per_raw_sample: if is_video {
            stream.bit_depth.unwrap_or_default()
        } else {
            0
        },
        ..MediaStream::default()
    })
}
