mod common;

use chrono::{Duration, TimeZone, Utc};
use ersatzrs_core::scheduling::{
    BlockSchedulingEngine, PlayoutSchedulingEngine, SchedulingCollectionKey, SchedulingContentGuid,
    SchedulingContentSource, SchedulingRequest,
};
use ersatzrs_domain::{CollectionType, PlaybackOrder, Playout};
use ersatzrs_infra::repositories::make_repositories;
use sqlx::{Executor, SqlitePool};

#[tokio::test]
async fn scheduling_repository_resolves_sql_content_sources() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_scheduling_fixture(&pool).await;

    let collection = repos
        .scheduling
        .media_items_for_source(&SchedulingContentSource::Collection("prime".to_owned()))
        .await
        .unwrap();
    assert_eq!(
        collection
            .iter()
            .map(|item| (item.id, item.title.as_deref(), item.season, item.episode))
            .collect::<Vec<_>>(),
        vec![
            (12, Some("Pilot"), Some(1), Some(1)),
            (13, Some("Finale"), Some(1), Some(2)),
            (20, Some("Feature"), None, None),
        ]
    );

    let multi = repos
        .scheduling
        .media_items_for_source(&SchedulingContentSource::MultiCollection(
            "multi-prime".to_owned(),
        ))
        .await
        .unwrap();
    assert_eq!(
        multi.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![12, 13, 20, 21]
    );

    let smart = repos
        .scheduling
        .media_items_for_source(&SchedulingContentSource::SmartCollection(
            "smart-bonus".to_owned(),
        ))
        .await
        .unwrap();
    assert_eq!(
        smart.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![21]
    );

    let search = repos
        .scheduling
        .media_items_for_source(&SchedulingContentSource::Search(
            "title contains Pilot".to_owned(),
        ))
        .await
        .unwrap();
    assert_eq!(
        search.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![12]
    );

    let show = repos
        .scheduling
        .media_items_for_source(&SchedulingContentSource::Show {
            name: None,
            guids: vec![SchedulingContentGuid {
                source: "tvdb".to_owned(),
                value: "show-guid".to_owned(),
            }],
        })
        .await
        .unwrap();
    assert_eq!(
        show.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![12, 13]
    );
}

#[tokio::test]
async fn scheduling_repository_resolves_playlist_children_and_overlays() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_scheduling_fixture(&pool).await;

    let playlist_items = repos
        .scheduling
        .playlist_items_for_source(&SchedulingContentSource::Playlist {
            group: Some("prime".to_owned()),
            name: "nightly".to_owned(),
        })
        .await
        .unwrap()
        .unwrap();

    assert_eq!(playlist_items.len(), 2);
    assert_eq!(
        playlist_items[0].collection_key.collection_type,
        CollectionType::Collection
    );
    assert_eq!(playlist_items[0].collection_key.collection_id, Some(1));
    assert_eq!(
        playlist_items[0].playback_order,
        PlaybackOrder::SeasonEpisode
    );
    assert!(playlist_items[0].play_all);
    assert_eq!(playlist_items[0].count, Some(2));
    assert_eq!(
        playlist_items[0]
            .media_items
            .iter()
            .map(|item| item.id)
            .collect::<Vec<_>>(),
        vec![12, 13, 20]
    );
    assert_eq!(
        playlist_items[1]
            .media_items
            .iter()
            .map(|item| item.id)
            .collect::<Vec<_>>(),
        vec![21]
    );

    assert_eq!(
        repos.scheduling.watermark_id_by_name("bug").await.unwrap(),
        Some(501)
    );
    assert_eq!(
        repos
            .scheduling
            .graphics_element_id_by_path("lower-third.svg")
            .await
            .unwrap(),
        Some(601)
    );
}

#[tokio::test]
async fn scheduling_repository_resolves_playlist_children_by_group_before_name() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_scheduling_fixture(&pool).await;

    pool.execute(r#"INSERT INTO "PlaylistGroup" ("Id", "Name") VALUES (42, 'late')"#)
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "Playlist" ("Id", "Name", "PlaylistGroupId", "IsSystem")
         VALUES (5, 'nightly', 42, 0)"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "PlaylistItem" (
            "Id", "Index", "PlaylistId", "CollectionType", "CollectionId",
            "SmartCollectionId", "PlaybackOrder", "Count", "PlayAll"
         ) VALUES (501, 0, 5, 'SmartCollection', NULL, 2, 'Chronological', NULL, 0)"#,
    )
    .await
    .unwrap();

    let prime = repos
        .scheduling
        .playlist_items_for_source(&SchedulingContentSource::Playlist {
            group: Some("prime".to_owned()),
            name: "nightly".to_owned(),
        })
        .await
        .unwrap()
        .unwrap();
    let late = repos
        .scheduling
        .playlist_items_for_source(&SchedulingContentSource::Playlist {
            group: Some("late".to_owned()),
            name: "nightly".to_owned(),
        })
        .await
        .unwrap()
        .unwrap();

    assert_eq!(prime[0].collection_key.collection_id, Some(1));
    assert_eq!(late[0].collection_key.smart_collection_id, Some(2));
    assert_eq!(
        late[0]
            .media_items
            .iter()
            .map(|item| item.id)
            .collect::<Vec<_>>(),
        vec![21]
    );
}

#[tokio::test]
async fn scheduling_repository_searches_metadata_fields_and_expands_containers() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_scheduling_fixture(&pool).await;
    seed_search_enrichment(&pool).await;

    let movie = repos
        .scheduling
        .media_items_for_source(&SchedulingContentSource::Search(
            "genre:Adventure AND tag:Festival AND studio:\"Studio One\" AND year:[2025 TO 2025]"
                .to_owned(),
        ))
        .await
        .unwrap();
    assert_eq!(
        movie
            .iter()
            .map(|item| (item.id, item.title.as_deref()))
            .collect::<Vec<_>>(),
        vec![(20, Some("Feature"))]
    );

    let episode = repos
        .scheduling
        .media_items_for_source(&SchedulingContentSource::Search(
            "show_title:\"The Show\" AND episode_number:2 AND actor:\"Lead Actor\"".to_owned(),
        ))
        .await
        .unwrap();
    assert_eq!(
        episode.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![13]
    );

    let show = repos
        .scheduling
        .media_items_for_source(&SchedulingContentSource::Search(
            "type:show AND tvdb:show-guid".to_owned(),
        ))
        .await
        .unwrap();
    assert_eq!(
        show.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![12, 13]
    );

    let season = repos
        .scheduling
        .media_items_for_source(&SchedulingContentSource::Search(
            "type:season AND season_number:1".to_owned(),
        ))
        .await
        .unwrap();
    assert_eq!(
        season.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![12, 13]
    );

    let artist = repos
        .scheduling
        .media_items_for_source(&SchedulingContentSource::Search(
            "type:artist AND title:\"The Band\"".to_owned(),
        ))
        .await
        .unwrap();
    assert_eq!(
        artist.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![31]
    );

    let stream_fields = repos
        .scheduling
        .media_items_for_source(&SchedulingContentSource::Search(
            "director:\"Director One\" AND writer:\"Writer One\" AND language_tag:eng AND sub_language_tag:nld AND video_codec:h264 AND audio_codec:aac AND subtitle_codec:srt AND width:1920 AND height:1080"
                .to_owned(),
        ))
        .await
        .unwrap();
    assert_eq!(
        stream_fields.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![20]
    );
}

#[tokio::test]
async fn scheduling_repository_smart_collection_uses_fielded_search_query() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_scheduling_fixture(&pool).await;
    seed_search_enrichment(&pool).await;

    pool.execute(
        r#"INSERT INTO "SmartCollection" ("Id", "Name", "Query")
         VALUES (
            5,
            'fielded-smart',
            'show_title:"The Show" AND release_date:[20260101 TO 20260131]'
         )"#,
    )
    .await
    .unwrap();

    let smart = repos
        .scheduling
        .media_items_for_source(&SchedulingContentSource::SmartCollection(
            "fielded-smart".to_owned(),
        ))
        .await
        .unwrap();
    assert_eq!(
        smart.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![13, 12]
    );
}

#[tokio::test]
async fn scheduling_repository_loads_block_reference_data_from_sql() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_scheduling_fixture(&pool).await;
    common::seed_profile_and_channel(&repos, 900).await;
    seed_block_scheduling_fixture(&pool).await;

    let reference_data = repos
        .scheduling
        .block_reference_data(
            900,
            Utc.with_ymd_and_hms(2026, 4, 15, 8, 0, 0).unwrap(),
            1,
            Duration::hours(1),
        )
        .await
        .unwrap();

    assert_eq!(reference_data.effective_blocks.len(), 2);
    let block = &reference_data.effective_blocks[0];
    assert_eq!(block.block_id, 710);
    assert_eq!(block.template_item_id, 811);
    assert_eq!(block.block_key.as_ref().unwrap().block_updated_ticks, 7100);
    assert_eq!(
        block.block_key.as_ref().unwrap().template_updated_ticks,
        8100
    );
    assert_eq!(
        block
            .block_key
            .as_ref()
            .unwrap()
            .playout_template_updated_ticks,
        81000
    );
    assert_eq!(
        block.start,
        Utc.with_ymd_and_hms(2026, 4, 15, 8, 0, 0).unwrap()
    );
    assert_eq!(block.items.len(), 1);
    assert_eq!(block.items[0].content, "block-item:711");
    assert_eq!(block.items[0].watermark_ids, vec![501]);
    assert_eq!(block.items[0].graphics_element_ids, vec![601]);
    assert_eq!(
        reference_data
            .media_by_key
            .get("block-item:711")
            .unwrap()
            .iter()
            .map(|item| item.id)
            .collect::<Vec<_>>(),
        vec![12, 13, 20]
    );

    let deco = block.deco.as_ref().unwrap();
    assert_eq!(deco.id, 750);
    assert_eq!(
        deco.default_filler_content.as_deref(),
        Some("deco-default-filler:750")
    );
    assert_eq!(
        deco.default_filler_collection_key
            .as_ref()
            .and_then(|key| key.collection_id),
        Some(1)
    );
    assert_eq!(deco.break_content.len(), 1);
    assert_eq!(deco.break_content[0].content, "deco-break-content:751");
    assert_eq!(
        reference_data
            .media_by_key
            .get("deco-break-content:751")
            .unwrap()
            .iter()
            .map(|item| item.id)
            .collect::<Vec<_>>(),
        vec![12, 13, 20, 21]
    );
    assert_eq!(
        reference_data
            .media_by_key
            .get("deco-default-filler:750")
            .unwrap()
            .iter()
            .map(|item| item.id)
            .collect::<Vec<_>>(),
        vec![12, 13, 20]
    );

    assert_eq!(reference_data.existing_items.len(), 1);
    assert_eq!(reference_data.existing_items[0].id, 9901);
    assert_eq!(
        reference_data.existing_items[0].block_key.as_deref(),
        Some(sql_block_key().as_str())
    );
    assert_eq!(reference_data.existing_history.len(), 1);
    assert_eq!(
        reference_data.existing_history[0].key,
        "persisted-block-history"
    );
}

#[tokio::test]
async fn sql_block_reference_data_feeds_block_change_detection() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_scheduling_fixture(&pool).await;
    common::seed_profile_and_channel(&repos, 900).await;
    seed_block_scheduling_fixture(&pool).await;

    let start = Utc.with_ymd_and_hms(2026, 4, 15, 8, 0, 0).unwrap();
    let reference_data = repos
        .scheduling
        .block_reference_data(900, start, 1, Duration::hours(1))
        .await
        .unwrap();

    let result = BlockSchedulingEngine
        .schedule(SchedulingRequest {
            playout: Playout {
                id: 900,
                seed: 123,
                ..Playout::default()
            },
            start,
            finish: start + Duration::hours(2),
            mode: ersatzrs_core::scheduling::PlayoutBuildMode::Continue,
            reference_data,
        })
        .unwrap();

    assert_eq!(
        result
            .added_items
            .iter()
            .map(|item| item.media_item_id)
            .collect::<Vec<_>>(),
        vec![12]
    );
    assert_eq!(
        result
            .added_items
            .iter()
            .filter_map(|item| item.collection_etag.as_deref())
            .collect::<Vec<_>>()
            .len(),
        1
    );
}

async fn seed_scheduling_fixture(pool: &SqlitePool) {
    seed_media_fixture(pool).await;
    seed_collection_fixture(pool).await;
    seed_overlay_fixture(pool).await;
}

async fn seed_media_fixture(pool: &SqlitePool) {
    common::seed_media_path(pool, 1, 1, 1).await;
    for id in [10, 11, 12, 13, 20, 21] {
        common::seed_media_item(pool, id, 1).await;
    }

    // Show 10
    pool.execute(r#"INSERT INTO "Show" ("Id") VALUES (10)"#)
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "ShowMetadata" (
            "Id", "ShowId", "MetadataKind", "Title", "SortTitle",
            "DateAdded", "DateUpdated"
         ) VALUES (100, 10, 0, 'The Show', 'show', '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z')"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "MetadataGuid" ("Id", "ShowMetadataId", "Guid")
         VALUES (1000, 100, 'tvdb://show-guid')"#,
    )
    .await
    .unwrap();

    // Season 11 (in Show 10)
    pool.execute(r#"INSERT INTO "Season" ("Id", "ShowId", "SeasonNumber") VALUES (11, 10, 1)"#)
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "SeasonMetadata" (
            "Id", "SeasonId", "MetadataKind", "Title", "SortTitle",
            "DateAdded", "DateUpdated"
         ) VALUES (110, 11, 0, 'Season 1', 'season 1', '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z')"#,
    )
    .await
    .unwrap();

    insert_episode(pool, 12, 120, "Pilot", 1, "2026-01-01T00:00:00Z").await;
    insert_episode(pool, 13, 130, "Finale", 2, "2026-01-08T00:00:00Z").await;
    insert_movie(pool, 20, 200, "Feature", "2025-12-24T00:00:00Z").await;
    insert_movie(pool, 21, 210, "Bonus", "2026-02-01T00:00:00Z").await;
}

async fn seed_search_enrichment(pool: &SqlitePool) {
    pool.execute(
        r#"UPDATE "MovieMetadata" SET "Year" = 2025, "SortTitle" = 'feature',
           "OriginalTitle" = 'Original Feature' WHERE "Id" = 200"#,
    )
    .await
    .unwrap();
    pool.execute(r#"UPDATE "EpisodeMetadata" SET "SortTitle" = 'finale' WHERE "Id" = 130"#)
        .await
        .unwrap();

    // Genre: FK column per metadata type (no junction table)
    pool.execute(
        r#"INSERT INTO "Genre" ("Id", "Name", "MovieMetadataId") VALUES (1, 'Adventure', 200)"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "Genre" ("Id", "Name", "ShowMetadataId") VALUES (2, 'Drama', 100)"#,
    )
    .await
    .unwrap();

    // Tag: FK column per metadata type
    pool.execute(
        r#"INSERT INTO "Tag" ("Id", "Name", "MovieMetadataId") VALUES (1, 'Festival', 200)"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "Tag" ("Id", "Name", "ShowMetadataId") VALUES (2, 'Network', 100)"#,
    )
    .await
    .unwrap();

    // Studio: FK column per metadata type
    pool.execute(
        r#"INSERT INTO "Studio" ("Id", "Name", "MovieMetadataId") VALUES (1, 'Studio One', 200)"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "Studio" ("Id", "Name", "ShowMetadataId") VALUES (2, 'Show Studio', 100)"#,
    )
    .await
    .unwrap();

    // Actor: FK column per metadata type
    pool.execute(
        r#"INSERT INTO "Actor" ("Id", "Name", "Role", "Order", "EpisodeMetadataId")
         VALUES (1, 'Lead Actor', 'Lead', 0, 130)"#,
    )
    .await
    .unwrap();

    pool.execute(r#"INSERT INTO "Director" ("Id", "Name", "MovieMetadataId") VALUES (1, 'Director One', 200)"#)
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "Writer" ("Id", "Name", "MovieMetadataId") VALUES (1, 'Writer One', 200)"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "LanguageCode" ("Id", "ThreeCode1", "ThreeCode2", "TwoCode", "EnglishName")
         VALUES (1, 'eng', 'eng', 'en', 'English'), (2, 'nld', 'dut', 'nl', 'Dutch')"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "MediaStream" (
            "Id", "Index", "Codec", "MediaStreamKind", "Language", "Channels",
            "Default", "Forced", "MediaVersionId"
         ) VALUES
            (1, 0, 'h264', 1, NULL, 0, 1, 0, 200),
            (2, 1, 'aac', 2, 'eng', 2, 1, 0, 200),
            (3, 2, 'ass', 3, 'nld', 0, 0, 0, 200)"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "Subtitle" (
            "Id", "Codec", "DateAdded", "DateUpdated", "Default", "Forced",
            "IsExtracted", "Language", "MovieMetadataId", "SDH", "StreamIndex",
            "SubtitleKind"
         ) VALUES (
            1, 'srt', '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z', 0, 0,
            0, 'nld', 200, 0, 3, 0
         )"#,
    )
    .await
    .unwrap();

    // MetadataGuid for movie
    pool.execute(
        r#"INSERT INTO "MetadataGuid" ("Id", "MovieMetadataId", "Guid")
         VALUES (2000, 200, 'tmdb://feature-guid')"#,
    )
    .await
    .unwrap();

    // Artist 30 and MusicVideo 31
    common::seed_media_item(pool, 30, 1).await;
    common::seed_media_item(pool, 31, 1).await;
    pool.execute(r#"INSERT INTO "Artist" ("Id") VALUES (30)"#)
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "ArtistMetadata" (
            "Id", "ArtistId", "MetadataKind", "Title", "SortTitle",
            "DateAdded", "DateUpdated"
         ) VALUES (300, 30, 0, 'The Band', 'band', '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z')"#,
    )
    .await
    .unwrap();
    pool.execute(r#"INSERT INTO "MusicVideo" ("Id", "ArtistId") VALUES (31, 30)"#)
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "MusicVideoMetadata" (
            "Id", "MusicVideoId", "MetadataKind", "Title", "SortTitle",
            "DateAdded", "DateUpdated"
         ) VALUES (310, 31, 0, 'The Hit', 'hit', '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z')"#,
    )
    .await
    .unwrap();
    insert_version(pool, 31, 240_000, "MusicVideoId").await;
}

async fn seed_collection_fixture(pool: &SqlitePool) {
    pool.execute(
        r#"INSERT INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder")
         VALUES (1, 'prime', 0)"#,
    )
    .await
    .unwrap();
    // CollectionItem has composite PK (CollectionId, MediaItemId) — no id or sort_order
    pool.execute(
        r#"INSERT INTO "CollectionItem" ("CollectionId", "MediaItemId")
         VALUES (1, 10), (1, 20)"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "SmartCollection" ("Id", "Name", "Query")
         VALUES (2, 'smart-bonus', 'title = Bonus')"#,
    )
    .await
    .unwrap();
    pool.execute(r#"INSERT INTO "MultiCollection" ("Id", "Name") VALUES (3, 'multi-prime')"#)
        .await
        .unwrap();
    // MultiCollectionItem has composite PK (MultiCollectionId, CollectionId)
    pool.execute(
        r#"INSERT INTO "MultiCollectionItem" (
            "MultiCollectionId", "CollectionId", "ScheduleAsGroup", "PlaybackOrder"
         ) VALUES (3, 1, 0, 'None')"#,
    )
    .await
    .unwrap();
    // MultiCollectionSmartItem has composite PK (MultiCollectionId, SmartCollectionId)
    pool.execute(
        r#"INSERT INTO "MultiCollectionSmartItem" (
            "MultiCollectionId", "SmartCollectionId", "ScheduleAsGroup", "PlaybackOrder"
         ) VALUES (3, 2, 0, 'None')"#,
    )
    .await
    .unwrap();

    // PlaylistGroup is the top-level parent; Playlist has PlaylistGroupId FK
    pool.execute(r#"INSERT INTO "PlaylistGroup" ("Id", "Name") VALUES (41, 'prime')"#)
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "Playlist" ("Id", "Name", "PlaylistGroupId", "IsSystem")
         VALUES (4, 'nightly', 41, 0)"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "PlaylistItem" (
            "Id", "Index", "PlaylistId", "CollectionType", "CollectionId",
            "SmartCollectionId", "PlaybackOrder", "Count", "PlayAll"
         ) VALUES
            (401, 0, 4, 'Collection', 1, NULL, 'SeasonEpisode', 2, 1),
            (402, 1, 4, 'SmartCollection', NULL, 2, 'Chronological', NULL, 0)"#,
    )
    .await
    .unwrap();
}

async fn seed_overlay_fixture(pool: &SqlitePool) {
    pool.execute(
        r#"INSERT INTO "ChannelWatermark" (
            "Id", "Name", "Mode", "ImageSource", "Location", "Size", "WidthPercent",
            "HorizontalMarginPercent", "VerticalMarginPercent", "FrequencyMinutes",
            "DurationSeconds", "Opacity", "PlaceWithinSourceContent", "ZIndex"
         ) VALUES (
            501, 'corner bug', 'Permanent', 'Local', 'TopRight', 'Scaled', 10.0,
            1.0, 1.0, 10, 30, 100, 0, 1
         )"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "GraphicsElement" ("Id", "Name", "Kind", "Path")
         VALUES (
            601, 'lower third', 'Image',
            '/var/lib/ErsatzRS/templates/graphics-elements/lower-third.svg'
         )"#,
    )
    .await
    .unwrap();
}

async fn seed_block_scheduling_fixture(pool: &SqlitePool) {
    seed_block_deco_fixture(pool).await;
    seed_block_structure_fixture(pool).await;
    seed_block_playout_fixture(pool).await;
}

async fn seed_block_deco_fixture(pool: &SqlitePool) {
    // DecoGroup is required (DecoGroupId NOT NULL FK)
    pool.execute(r#"INSERT INTO "DecoGroup" ("Id", "Name") VALUES (70, 'block-decos')"#)
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "Deco" (
            "Id", "DecoGroupId", "Name",
            "BreakContentMode", "WatermarkMode", "GraphicsElementsMode",
            "DefaultFillerMode", "DefaultFillerCollectionType", "DefaultFillerCollectionId",
            "DefaultFillerTrimToFit",
            "DeadAirFallbackMode", "DeadAirFallbackCollectionType",
            "UseWatermarkDuringFiller", "UseGraphicsElementsDuringFiller"
         ) VALUES (
            750, 70, 'default deco',
            'Inherit', 'Inherit', 'Inherit',
            'Inherit', 'Collection', 1,
            0,
            'Inherit', 'Collection',
            0, 0
         )"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "Deco" (
            "Id", "DecoGroupId", "Name",
            "BreakContentMode", "WatermarkMode", "GraphicsElementsMode",
            "DefaultFillerMode", "DefaultFillerCollectionType", "DefaultFillerCollectionId",
            "DefaultFillerTrimToFit",
            "DeadAirFallbackMode", "DeadAirFallbackCollectionType",
            "UseWatermarkDuringFiller", "UseGraphicsElementsDuringFiller"
         ) VALUES (
            760, 70, 'morning deco',
            'Inherit', 'Inherit', 'Inherit',
            'Override', 'Collection', 1,
            1,
            'Inherit', 'Collection',
            0, 0
         )"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "DecoBreakContent" (
            "Id", "DecoId", "Placement", "CollectionType", "PlaylistId"
         ) VALUES (751, 750, 'BlockStart', 'Playlist', 4)"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "DecoBreakContent" (
            "Id", "DecoId", "Placement", "CollectionType", "PlaylistId"
         ) VALUES (761, 760, 'BlockStart', 'Playlist', 4)"#,
    )
    .await
    .unwrap();
    // DecoTemplateGroup required (DecoTemplateGroupId NOT NULL FK)
    pool.execute(
        r#"INSERT INTO "DecoTemplateGroup" ("Id", "Name") VALUES (77, 'block-deco-groups')"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "DecoTemplate" ("Id", "DecoTemplateGroupId", "Name", "DateUpdated")
         VALUES (770, 77, 'morning windows', '0')"#,
    )
    .await
    .unwrap();
    // DecoTemplateItem uses StartTime/EndTime as HH:MM:SS text (28800000ms = 08:00:00, 36000000ms = 10:00:00)
    pool.execute(
        r#"INSERT INTO "DecoTemplateItem" (
            "Id", "DecoTemplateId", "DecoId", "StartTime", "EndTime"
         ) VALUES (771, 770, 760, '08:00:00', '10:00:00')"#,
    )
    .await
    .unwrap();
}

async fn seed_block_structure_fixture(pool: &SqlitePool) {
    pool.execute(
        r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed", "DecoId")
         VALUES (900, 900, 'Block', 123, 750)"#,
    )
    .await
    .unwrap();
    // BlockGroup required (BlockGroupId NOT NULL FK)
    pool.execute(r#"INSERT INTO "BlockGroup" ("Id", "Name") VALUES (71, 'morning-blocks')"#)
        .await
        .unwrap();
    // Block stores updated_ticks as string in DateUpdated, duration as integer minutes
    pool.execute(
        r#"INSERT INTO "Block" ("Id", "BlockGroupId", "Name", "DateUpdated", "Minutes", "StopScheduling")
         VALUES
            (700, 71, 'Morning', '7000', 60, 'BeforeDurationEnd'),
            (710, 71, 'Wednesday Morning', '7100', 60, 'BeforeDurationEnd')"#,
    )
    .await
    .unwrap();
    // BlockItem uses Index instead of sort_order
    pool.execute(
        r#"INSERT INTO "BlockItem" (
            "Id", "Index", "BlockId", "CollectionType", "CollectionId", "PlaybackOrder"
         ) VALUES
            (701, 0, 700, 'Collection', 1, 'Chronological'),
            (711, 0, 710, 'Collection', 1, 'Chronological')"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "BlockItemWatermark" ("BlockItemId", "WatermarkId") VALUES (701, 501)"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "BlockItemWatermark" ("BlockItemId", "WatermarkId") VALUES (711, 501)"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "BlockItemGraphicsElement" ("BlockItemId", "GraphicsElementId")
         VALUES (701, 601)"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "BlockItemGraphicsElement" ("BlockItemId", "GraphicsElementId")
         VALUES (711, 601)"#,
    )
    .await
    .unwrap();
    // TemplateGroup required (TemplateGroupId NOT NULL FK)
    pool.execute(r#"INSERT INTO "TemplateGroup" ("Id", "Name") VALUES (72, 'weekly-templates')"#)
        .await
        .unwrap();
    // Template stores updated_ticks as string in DateUpdated
    pool.execute(
        r#"INSERT INTO "Template" ("Id", "TemplateGroupId", "Name", "DateUpdated")
         VALUES (800, 72, 'weekday', '8000'), (810, 72, 'wednesday', '8100')"#,
    )
    .await
    .unwrap();
    // TemplateItem uses StartTime as HH:MM:SS text (28800000ms = 08:00:00)
    pool.execute(
        r#"INSERT INTO "TemplateItem" ("Id", "TemplateId", "BlockId", "StartTime")
         VALUES (801, 800, 700, '08:00:00'), (811, 810, 710, '08:00:00')"#,
    )
    .await
    .unwrap();
    // PlayoutTemplate uses DateUpdated for ticks, Index for ordering, and JSON arrays for day filters
    pool.execute(
        r#"INSERT INTO "PlayoutTemplate" (
            "PlayoutId", "TemplateId", "DateUpdated", "DecoTemplateId", "Index",
            "DaysOfWeek", "DaysOfMonth", "MonthsOfYear",
            "LimitToDateRange", "StartMonth", "StartDay", "EndMonth", "EndDay"
         ) VALUES
            (900, 800, '80000', NULL, 1,
             '[1,2,3,4,5,6,0]', '[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31]', '[1,2,3,4,5,6,7,8,9,10,11,12]',
             0, 1, 1, 12, 31),
            (900, 810, '81000', 770, 0,
             '[3]', '[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31]', '[4]',
             0, 1, 1, 12, 31)"#,
    )
    .await
    .unwrap();
}

async fn seed_block_playout_fixture(pool: &SqlitePool) {
    // PlayoutItem uses InPoint/OutPoint as HH:MM:SS text (0ms = 00:00:00, 1800000ms = 00:30:00)
    sqlx::query(
        r#"INSERT INTO "PlayoutItem" (
            "Id", "PlayoutId", "MediaItemId", "Start", "Finish", "GuideStart", "GuideFinish",
            "GuideGroup", "FillerKind", "InPoint", "OutPoint", "DisableWatermarks",
            "BlockKey", "CollectionKey", "CollectionEtag"
         ) VALUES (
            9901, 900, 12, ?, ?, ?, ?, 801, 'None', '00:00:00', '00:30:00', 0, ?, ?, 'stale'
         )"#,
    )
    .bind("2026-04-15T08:00:00+00:00")
    .bind("2026-04-15T08:30:00+00:00")
    .bind("2026-04-15T08:00:00+00:00")
    .bind("2026-04-15T09:00:00+00:00")
    .bind(sql_block_key())
    .bind(sql_collection_key())
    .execute(pool)
    .await
    .unwrap();
    // PlayoutHistory renamed columns: Key, When, Index, PlaybackOrder, IsCurrentChild
    pool.execute(
        r#"INSERT INTO "PlayoutHistory" (
            "Id", "PlayoutId", "Key", "When", "Finish", "Index", "PlaybackOrder",
            "Details", "IsCurrentChild"
         ) VALUES (
            9801, 900, 'persisted-block-history', '2026-04-15T08:00:00+00:00',
            '2026-04-15T08:30:00+00:00', 0, 0, '{}', 0
         )"#,
    )
    .await
    .unwrap();
}

fn sql_block_key() -> String {
    serde_json::json!({
        "b": 710,
        "bt": 7100,
        "t": 810,
        "tt": 8100,
        "pt": 810,
        "ptt": 81000
    })
    .to_string()
}

fn sql_collection_key() -> String {
    serde_json::to_string(&SchedulingCollectionKey {
        collection_type: CollectionType::Collection,
        collection_id: Some(1),
        ..SchedulingCollectionKey::default()
    })
    .unwrap()
}

async fn insert_episode(
    pool: &SqlitePool,
    id: i64,
    metadata_id: i64,
    title: &str,
    episode_number: i32,
    release_date: &str,
) {
    // Episode has only SeasonId FK (no show_id or season_number columns)
    sqlx::query(r#"INSERT INTO "Episode" ("Id", "SeasonId") VALUES (?, 11)"#)
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO "EpisodeMetadata" (
            "Id", "EpisodeId", "MetadataKind", "Title", "ReleaseDate",
            "DateAdded", "DateUpdated", "EpisodeNumber"
         ) VALUES (?, ?, 0, ?, ?, '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z', ?)"#,
    )
    .bind(metadata_id)
    .bind(id)
    .bind(title)
    .bind(release_date)
    .bind(episode_number)
    .execute(pool)
    .await
    .unwrap();
    insert_version(pool, id, 1_800_000, "EpisodeId").await;
}

async fn insert_movie(
    pool: &SqlitePool,
    id: i64,
    metadata_id: i64,
    title: &str,
    release_date: &str,
) {
    // Movie has no additional columns beyond the PK shared with MediaItem
    sqlx::query(r#"INSERT INTO "Movie" ("Id") VALUES (?)"#)
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO "MovieMetadata" (
            "Id", "MovieId", "MetadataKind", "Title", "ReleaseDate",
            "DateAdded", "DateUpdated"
         ) VALUES (?, ?, 0, ?, ?, '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z')"#,
    )
    .bind(metadata_id)
    .bind(id)
    .bind(title)
    .bind(release_date)
    .execute(pool)
    .await
    .unwrap();
    insert_version(pool, id, 3_600_000, "MovieId").await;
}

/// Insert a `MediaVersion` row. `fk_column` is the nullable FK column for the media type,
/// e.g. `"EpisodeId"`, `"MovieId"`, or `"MusicVideoId"`. Duration is derived from `duration_ms`.
async fn insert_version(pool: &SqlitePool, media_item_id: i64, duration_ms: i64, fk_column: &str) {
    let total_seconds = duration_ms / 1000;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    let duration_text = format!("{hours:02}:{minutes:02}:{seconds:02}");
    let sql = format!(
        r#"INSERT INTO "MediaVersion" (
            "Id", "{fk_column}", "Name", "Duration", "VideoScanKind",
            "DateAdded", "DateUpdated", "Width", "Height"
         ) VALUES (?, ?, 'main', ?, 'Progressive', '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z', 1920, 1080)"#
    );
    sqlx::query(&sql)
        .bind(media_item_id * 10)
        .bind(media_item_id)
        .bind(duration_text)
        .execute(pool)
        .await
        .unwrap();
}
