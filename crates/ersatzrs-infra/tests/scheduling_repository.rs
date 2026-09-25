mod common;

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Duration, TimeZone, Utc};
use ersatzrs_core::content_variants;
use ersatzrs_core::repositories::{
    AdminMediaRepository, AdminSearchKind, AdminSearchRepository, CollectionRepository,
    FillerRepository, ProgramScheduleRepository,
};
use ersatzrs_core::scheduling::{
    block_item_smart_shuffle_config_key, classic_schedule_item_key,
    classic_schedule_item_policy_key, legacy_playout_cooldown_config_key,
    program_schedule_item_smart_shuffle_config_key, BlockPreviewRepository, BlockSchedulingEngine,
    ClassicSchedulingEngine, EffectiveBlockItem, PlayoutSchedulingEngine, SchedulingCollectionKey,
    SchedulingContentGuid, SchedulingContentSource, SchedulingRequest,
    SchedulingSmartShufflePolicy, SchedulingSmartShufflePreset, SchedulingSmartShuffleRule,
    SchedulingSmartShuffleRuleKind, ScriptedSchedulingEngine,
};
use ersatzrs_domain::{
    ChannelSubtitleMode, CollectionType, FillerKind, PlaybackOrder, Playout, PlayoutMode,
    PlayoutScheduleKind, ProgramSchedule, ProgramScheduleItem,
    CONFIG_KEY_FFMPEG_GLOBAL_FALLBACK_FILLER_ID, CONFIG_KEY_FFMPEG_GLOBAL_WATERMARK_ID,
    CONFIG_KEY_PLAYOUT_SKIP_MISSING_ITEMS,
};
use ersatzrs_infra::repositories::{make_repositories, Repositories};
use sqlx::{Executor, SqlitePool};

fn persisted_smart_shuffle_policy() -> SchedulingSmartShufflePolicy {
    SchedulingSmartShufflePolicy {
        enabled: true,
        preset: SchedulingSmartShufflePreset::Custom,
        after_items: Some(2),
        rules: vec![
            SchedulingSmartShuffleRule {
                id: "show".to_owned(),
                kind: SchedulingSmartShuffleRuleKind::Show,
                search: None,
            },
            SchedulingSmartShuffleRule {
                id: "season".to_owned(),
                kind: SchedulingSmartShuffleRuleKind::Season,
                search: None,
            },
            SchedulingSmartShuffleRule {
                id: "tag".to_owned(),
                kind: SchedulingSmartShuffleRuleKind::Tag,
                search: None,
            },
            SchedulingSmartShuffleRule {
                id: "content_rating".to_owned(),
                kind: SchedulingSmartShuffleRuleKind::ContentRating,
                search: None,
            },
            SchedulingSmartShuffleRule {
                id: "language".to_owned(),
                kind: SchedulingSmartShuffleRuleKind::Language,
                search: None,
            },
            SchedulingSmartShuffleRule {
                id: "music_label".to_owned(),
                kind: SchedulingSmartShuffleRuleKind::MusicLabel,
                search: None,
            },
            SchedulingSmartShuffleRule {
                id: "pilot".to_owned(),
                kind: SchedulingSmartShuffleRuleKind::Filter,
                search: Some("title contains Pilot".to_owned()),
            },
        ],
        ..SchedulingSmartShufflePolicy::default()
    }
}

#[tokio::test]
async fn scheduling_repository_skips_missing_items_when_configured() {
    let pool = common::test_db().await;
    seed_scheduling_fixture(&pool).await;

    // Flag the "prime" collection's movie (id 20) as FileNotFound.
    sqlx::query(r#"UPDATE "MediaItem" SET "State" = 1 WHERE "Id" = 20"#)
        .execute(&pool)
        .await
        .unwrap();

    // Default (no config row): missing items remain in scheduling, matching
    // ErsatzTV's PlayoutSkipMissingItems defaulting to false.
    let included = make_repositories(pool.clone())
        .scheduling
        .media_items_for_source(&SchedulingContentSource::Collection("prime".to_owned()))
        .await
        .unwrap();
    assert!(
        included.iter().any(|item| item.id == 20),
        "missing item is kept when skip-missing is off"
    );

    // Enable skip-missing: a fresh repository re-reads the flag and drops the
    // FileNotFound item while keeping the present ones.
    sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, 'True')"#)
        .bind(CONFIG_KEY_PLAYOUT_SKIP_MISSING_ITEMS)
        .execute(&pool)
        .await
        .unwrap();
    let filtered = make_repositories(pool.clone())
        .scheduling
        .media_items_for_source(&SchedulingContentSource::Collection("prime".to_owned()))
        .await
        .unwrap();
    assert!(
        !filtered.iter().any(|item| item.id == 20),
        "missing item is skipped when skip-missing is on"
    );
    assert!(
        filtered.iter().any(|item| item.id == 12),
        "present items are kept"
    );
}

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
async fn scheduling_repository_loads_global_playout_config() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_scheduling_fixture(&pool).await;

    pool.execute(
        r#"INSERT INTO "FillerPreset" (
               "Id", "Name", "FillerMode", "FillerKind", "CollectionType",
               "CollectionId", "AllowWatermarks", "UseChaptersAsMediaItems"
           ) VALUES (77, 'Global Fallback', 1, 0, 'Collection', 1, 0, 0)"#,
    )
    .await
    .unwrap();
    sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
        .bind(CONFIG_KEY_FFMPEG_GLOBAL_WATERMARK_ID)
        .bind("55")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
        .bind(CONFIG_KEY_FFMPEG_GLOBAL_FALLBACK_FILLER_ID)
        .bind("77")
        .execute(&pool)
        .await
        .unwrap();

    let reference_data = repos.scheduling.configured_playout_options().await.unwrap();

    assert_eq!(reference_data.global_watermark_id, Some(55));
    let fallback = reference_data.global_fallback.unwrap();
    assert_eq!(fallback.id, 77);
    assert!(!fallback.allow_watermarks);
    assert_eq!(
        fallback
            .media_items
            .iter()
            .map(|item| item.id)
            .collect::<Vec<_>>(),
        vec![12, 13, 20]
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
async fn scheduling_repository_applies_collection_and_playlist_playback_overrides() {
    let pool = common::test_db().await;
    seed_scheduling_fixture(&pool).await;

    pool.execute(
        r#"UPDATE "CollectionItem"
              SET "PreferredAudioLanguageCode" = 'eng',
                  "PreferredAudioTitle" = 'Director Commentary',
                  "PreferredSubtitleLanguageCode" = 'eng',
                  "SubtitleMode" = 3
            WHERE "CollectionId" = 1 AND "MediaItemId" = 10"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"UPDATE "PlaylistItem"
              SET "PreferredAudioLanguageCode" = 'jpn',
                  "PreferredAudioTitle" = 'Isolated Score',
                  "PreferredSubtitleLanguageCode" = 'eng',
                  "SubtitleMode" = 2
            WHERE "Id" = 402"#,
    )
    .await
    .unwrap();

    let repos = make_repositories(pool.clone());
    let collection = repos
        .scheduling
        .media_items_for_source(&SchedulingContentSource::Collection("prime".to_owned()))
        .await
        .unwrap();
    let pilot = collection.iter().find(|item| item.id == 12).unwrap();
    let finale = collection.iter().find(|item| item.id == 13).unwrap();
    let feature = collection.iter().find(|item| item.id == 20).unwrap();

    assert_eq!(
        pilot.preferred_audio_title.as_deref(),
        Some("Director Commentary")
    );
    assert_eq!(finale.preferred_audio_language_code.as_deref(), Some("eng"));
    assert_eq!(pilot.subtitle_mode, Some(ChannelSubtitleMode::Any));
    assert_eq!(feature.preferred_audio_title, None);

    let playlist_items = repos
        .scheduling
        .playlist_items_for_source(&SchedulingContentSource::Playlist {
            group: Some("prime".to_owned()),
            name: "nightly".to_owned(),
        })
        .await
        .unwrap()
        .unwrap();
    let playlist_bonus = playlist_items[1]
        .media_items
        .iter()
        .find(|item| item.id == 21)
        .unwrap();

    assert_eq!(
        playlist_bonus.preferred_audio_title.as_deref(),
        Some("Isolated Score")
    );
    assert_eq!(
        playlist_bonus.preferred_audio_language_code.as_deref(),
        Some("jpn")
    );
    assert_eq!(
        playlist_bonus.subtitle_mode,
        Some(ChannelSubtitleMode::Default)
    );
}

#[tokio::test]
async fn scheduling_repository_expands_duplicate_collection_rows_with_distinct_overrides() {
    let pool = common::test_db().await;
    seed_scheduling_fixture(&pool).await;

    // ADR 0048: the same movie appears twice in one collection, once as a
    // default row and once with a commentary-track override. Both rows must
    // schedule as separate items carrying their own playback preferences.
    pool.execute(
        r#"INSERT INTO "CollectionItem" (
              "CollectionId", "MediaItemId", "PreferredAudioLanguageCode",
              "PreferredAudioTitle", "SubtitleMode"
           ) VALUES (1, 20, 'eng', 'Director Commentary', 3)"#,
    )
    .await
    .unwrap();

    let repos = make_repositories(pool.clone());
    let items = repos
        .scheduling
        .media_items_for_source(&SchedulingContentSource::Collection("prime".to_owned()))
        .await
        .unwrap();

    let features: Vec<_> = items.iter().filter(|item| item.id == 20).collect();
    assert_eq!(features.len(), 2, "both duplicate rows must expand");
    assert!(features
        .iter()
        .any(|item| item.preferred_audio_title.is_none()));
    let commentary = features
        .iter()
        .find(|item| item.preferred_audio_title.as_deref() == Some("Director Commentary"))
        .expect("override row expands with its own audio title");
    assert_eq!(
        commentary.preferred_audio_language_code.as_deref(),
        Some("eng")
    );
    assert_eq!(commentary.subtitle_mode, Some(ChannelSubtitleMode::Any));
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
        artist
            .iter()
            .map(|item| (item.id, item.artist_id))
            .collect::<Vec<_>>(),
        vec![(31, Some(30))]
    );

    let stream_fields = repos
        .scheduling
        .media_items_for_source(&SchedulingContentSource::Search(
            "director:\"Director One\" AND writer:\"Writer One\" AND language_tag:eng AND sub_language_tag:nld AND video_codec:h264 AND audio_codec:aac AND subtitle_codec:srt AND width:1920 AND height:1080 AND chapters:2"
                .to_owned(),
        ))
        .await
        .unwrap();
    assert_eq!(
        stream_fields.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![20]
    );

    let fuzzy_boosted = repos
        .scheduling
        .media_items_for_source(&SchedulingContentSource::Search(
            "title:feture~1^3 AND minutes:[59 TO 61]".to_owned(),
        ))
        .await
        .unwrap();
    assert_eq!(
        fuzzy_boosted.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![20]
    );
}

#[tokio::test]
async fn scheduling_repository_searches_uniform_facets_for_every_media_kind() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_scheduling_fixture(&pool).await;
    seed_search_enrichment(&pool).await;
    seed_uniform_search_facets(&pool).await;

    for (media_kind, expected_ids) in [
        ("movie", vec![20]),
        ("show", vec![12, 13]),
        ("episode", vec![12]),
        ("music_video", vec![31]),
        ("other_video", vec![50]),
        ("song", vec![51]),
        ("image", vec![52]),
    ] {
        let items = repos
            .scheduling
            .media_items_for_source(&SchedulingContentSource::Search(format!(
                "type:{media_kind} AND content_rating:\"Uniform Rating\" AND \
                 studio:\"Uniform Studio\" AND music_label:\"Uniform Label\" AND \
                 language:Dutch AND language_tag:dut AND \
                 genre:\"Uniform Genre\" AND tag:\"Uniform Tag\""
            )))
            .await
            .unwrap();
        assert_eq!(
            items.iter().map(|item| item.id).collect::<Vec<_>>(),
            expected_ids,
            "uniform facet query failed for {media_kind}"
        );
    }
}

#[tokio::test]
async fn scheduling_repository_searches_documented_lucene_smart_collection_fields() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_scheduling_fixture(&pool).await;
    seed_search_enrichment(&pool).await;

    let documented_fields = repos
        .scheduling
        .media_items_for_source(&SchedulingContentSource::Search(
            "library_name:local AND library_folder_id:70 AND collection:prime AND \
             content_rating:TV-MA AND video_bit_depth:10 AND video_dynamic_range:hdr"
                .to_owned(),
        ))
        .await
        .unwrap();
    assert_eq!(
        documented_fields
            .iter()
            .map(|item| item.id)
            .collect::<Vec<_>>(),
        vec![20]
    );

    let show_documented_fields = repos
        .scheduling
        .media_items_for_source(&SchedulingContentSource::Search(
            "show_network:Network AND show_content_rating:TV-14".to_owned(),
        ))
        .await
        .unwrap();
    assert_eq!(
        show_documented_fields
            .iter()
            .map(|item| item.id)
            .collect::<Vec<_>>(),
        vec![13, 12]
    );

    let music_video = repos
        .scheduling
        .media_items_for_source(&SchedulingContentSource::Search(
            "type:music_video AND album:hits AND artist:\"The Band\"".to_owned(),
        ))
        .await
        .unwrap();
    assert_eq!(
        music_video.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![31]
    );

    let excluded_music_video = repos
        .scheduling
        .media_items_for_source(&SchedulingContentSource::Search(
            "type:music_video AND NOT artist:\"The Band\"".to_owned(),
        ))
        .await
        .unwrap();
    assert!(excluded_music_video.is_empty());

    let artist_style_mood = repos
        .scheduling
        .media_items_for_source(&SchedulingContentSource::Search(
            "type:artist AND style:lush AND mood:melancholy".to_owned(),
        ))
        .await
        .unwrap();
    assert_eq!(
        artist_style_mood
            .iter()
            .map(|item| item.id)
            .collect::<Vec<_>>(),
        vec![31]
    );
}

#[tokio::test]
async fn deleting_manual_collection_removes_search_membership_immediately() {
    let pool = common::test_db().await;
    seed_scheduling_fixture(&pool).await;
    let repos = make_repositories(pool.clone());

    let (before, _, _) = repos
        .scheduling
        .search_documents_page("collection:prime", 0, 0)
        .await
        .unwrap();
    assert!(before.iter().any(|document| document.media_item_id == 20));

    repos.collection.delete_admin(1).await.unwrap();

    let (after, total, _) = repos
        .scheduling
        .search_documents_page("collection:prime", 0, 0)
        .await
        .unwrap();
    assert!(after.is_empty());
    assert_eq!(total, 0);
    let (media, _, _) = repos
        .scheduling
        .search_documents_page("title:Feature", 0, 0)
        .await
        .unwrap();
    assert!(media.iter().any(|document| document.media_item_id == 20));
}

#[tokio::test]
async fn scheduling_repository_searches_content_variants_and_stream_roles() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_scheduling_fixture(&pool).await;
    seed_search_enrichment(&pool).await;

    sqlx::query(
        r#"INSERT INTO "MediaItem" (
            "Id", "LibraryPathId", "State", "ContentKind", "SubKind", "ParentMediaItemId"
         ) VALUES (40, 1, 0, 1, 'Featurettes', 20)"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.execute(r#"INSERT INTO "OtherVideo" ("Id") VALUES (40)"#)
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "OtherVideoMetadata" (
            "Id", "OtherVideoId", "Title", "SortTitle", "DateAdded", "DateUpdated", "MetadataKind"
         ) VALUES (
            400, 40, 'Making The Escape', 'making the escape',
            '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z', 0
         )"#,
    )
    .await
    .unwrap();

    let default_search = repos
        .scheduling
        .media_items_for_source(&SchedulingContentSource::Search("title:Making".to_owned()))
        .await
        .unwrap();
    assert!(default_search.is_empty());

    let explicit_variant = repos
        .scheduling
        .media_items_for_source(&SchedulingContentSource::Search(
            "content_kind:extra AND sub_kind:featurette AND parent_title:Feature".to_owned(),
        ))
        .await
        .unwrap();
    assert_eq!(
        explicit_variant
            .iter()
            .map(|item| item.id)
            .collect::<Vec<_>>(),
        vec![40]
    );

    let commentary = repos
        .scheduling
        .media_items_for_source(&SchedulingContentSource::Search(
            "has_commentary AND audio_title:commentary AND subtitle_title:commentary".to_owned(),
        ))
        .await
        .unwrap();
    assert_eq!(
        commentary.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![20]
    );
}

#[tokio::test]
async fn scheduling_repository_content_variant_examples_resolve_and_drive_featurette_preroll() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_scheduling_fixture(&pool).await;
    seed_search_enrichment(&pool).await;
    seed_content_variant_example_rows(&pool).await;

    insert_content_variant_example_smart_collections(&pool).await;

    assert_smart_collection_source_ids(&repos, content_variants::UNPLUGGED_RENDITIONS.name, &[31])
        .await;
    assert_smart_collection_source_ids(&repos, content_variants::COMMENTARY_MOVIES.name, &[20])
        .await;
    assert_smart_collection_source_ids(&repos, content_variants::MOVIE_PRIMARIES.name, &[21, 20])
        .await;

    let filler = content_variants::featurette_pre_roll_filler(77, 53);
    repos.filler.upsert(&filler).await.unwrap();
    repos
        .program_schedule
        .upsert(&ProgramSchedule {
            id: 302,
            name: "Example - Featurette Before Movie".to_owned(),
            item_ids: vec![402],
            ..ProgramSchedule::default()
        })
        .await
        .unwrap();
    let schedule_item =
        content_variants::movie_schedule_item_with_featurette_pre_roll(402, 302, 54, 77);
    repos
        .program_schedule
        .upsert_item(&schedule_item)
        .await
        .unwrap();
    common::seed_profile_and_channel(&repos, 902).await;
    pool.execute(
        r#"INSERT INTO "Playout" ("Id", "ChannelId", "ProgramScheduleId", "ScheduleKind", "Seed")
           VALUES (902, 902, 302, 'Classic', 123)"#,
    )
    .await
    .unwrap();

    let start = Utc.with_ymd_and_hms(2026, 4, 15, 8, 0, 0).unwrap();
    let reference_data = repos
        .scheduling
        .classic_reference_data(902, start, Duration::hours(1))
        .await
        .unwrap();

    assert_eq!(
        reference_data.classic_schedule_items[0].pre_roll_filler_id,
        Some(77)
    );
    assert_eq!(
        reference_data
            .media_by_key
            .get(&classic_schedule_item_key(
                &reference_data.classic_schedule_items[0]
            ))
            .unwrap()
            .iter()
            .map(|item| item.id)
            .collect::<Vec<_>>(),
        vec![21, 20]
    );

    let result = ClassicSchedulingEngine
        .schedule(SchedulingRequest {
            playout: Playout {
                id: 902,
                schedule_kind: PlayoutScheduleKind::Classic,
                seed: 123,
                ..Playout::default()
            },
            start,
            finish: start + Duration::hours(1),
            mode: ersatzrs_core::scheduling::PlayoutBuildMode::Continue,
            reference_data,
        })
        .unwrap();

    assert_eq!(
        result
            .added_items
            .iter()
            .take(2)
            .map(|item| (item.media_item_id, item.filler_kind))
            .collect::<Vec<_>>(),
        vec![(40, FillerKind::PreRoll), (20, FillerKind::None)]
    );
}

async fn assert_smart_collection_source_ids(
    repos: &Repositories,
    name: &str,
    expected_ids: &[i64],
) {
    let items = repos
        .scheduling
        .media_items_for_source(&SchedulingContentSource::SmartCollection(name.to_owned()))
        .await
        .unwrap();
    assert_eq!(
        items.iter().map(|item| item.id).collect::<Vec<_>>(),
        expected_ids
    );
}

#[tokio::test]
async fn scheduling_repository_search_documents_page_returns_page_map() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_scheduling_fixture(&pool).await;
    seed_search_enrichment(&pool).await;

    let (page, total, page_map) = repos
        .scheduling
        .search_documents_page("type:movie OR type:episode", 0, 2)
        .await
        .unwrap();

    assert_eq!(total, 4);
    assert_eq!(
        page.iter().map(|doc| doc.media_item_id).collect::<Vec<_>>(),
        vec![21, 20]
    );
    let page_map = page_map.expect("page map");
    assert_eq!(page_map.page_map[&'#'], 1);
    assert_eq!(page_map.page_map[&'b'], 1);
    assert_eq!(page_map.page_map[&'g'], 2);
}

#[tokio::test]
async fn addon_list_search_indexes_only_published_members() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_scheduling_fixture(&pool).await;

    pool.execute(
        r#"INSERT INTO "AddonInstallation" (
              "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
              "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt"
           ) VALUES (
              'org.example.search-list', '1.0.0', 1, 'package', 'content', '{}',
              'addons/search-list', '2026-09-04T00:00:00Z', '2026-09-04T00:00:00Z'
           )"#,
    )
    .await
    .unwrap();
    for (list_id, name) in [
        (501_i64, "Published list"),
        (502_i64, "Draft list"),
        (503_i64, "Pending exclusion"),
    ] {
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" (
                  "Id", "AddonId", "ProviderId", "SourceUrl", "Name"
               ) VALUES (?, 'org.example.search-list', ?, 'https://example.invalid/list', ?)"#,
        )
        .bind(list_id)
        .bind(format!("list-{list_id}"))
        .bind(name)
        .execute(&pool)
        .await
        .unwrap();
    }
    for (item_id, list_id, published, included) in [
        (601_i64, 501_i64, true, true),
        (602_i64, 502_i64, false, true),
        // Included is mutable draft intent. Until the next Publish, this item
        // remains part of the committed list snapshot and stays searchable.
        (603_i64, 503_i64, true, false),
    ] {
        sqlx::query(
            r#"INSERT INTO "AddonMediaListItem" (
                  "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind",
                  "MediaItemId", "Rank", "Title", "Published", "Included"
               ) VALUES (?, ?, ?, 'Feature', 0, 20, 0, 'Feature', ?, ?)"#,
        )
        .bind(item_id)
        .bind(list_id)
        .bind(format!("item-{item_id}"))
        .bind(published)
        .bind(included)
        .execute(&pool)
        .await
        .unwrap();
    }

    for (list_id, expected_ids) in [
        (501_i64, vec![20_i64]),
        (502_i64, Vec::new()),
        (503_i64, vec![20_i64]),
    ] {
        let (hits, _, _) = repos
            .scheduling
            .search_documents_page(&format!("addon_list:{list_id}"), 0, 0)
            .await
            .unwrap();
        assert_eq!(
            hits.into_iter()
                .map(|document| document.media_item_id)
                .collect::<Vec<_>>(),
            expected_ids
        );
    }
}

#[tokio::test]
async fn admin_show_detail_uses_one_lowest_non_empty_artwork_winner_per_season() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_scheduling_fixture(&pool).await;
    seed_legacy_tv_artwork_variants(&pool).await;

    let detail = repos
        .admin_media
        .get_show_detail_admin(10)
        .await
        .unwrap()
        .expect("show detail");

    assert_eq!(detail.poster_path.as_deref(), Some("show-lowest.jpg"));
    assert_eq!(detail.fanart_path.as_deref(), Some("fanart-lowest.jpg"));
    assert_eq!(detail.seasons.len(), 1);
    assert_eq!(detail.seasons[0].id, 11);
    assert_eq!(
        detail.seasons[0].poster_path.as_deref(),
        Some("season-lowest.jpg")
    );
}

#[tokio::test]
async fn admin_search_uses_stable_lowest_non_empty_tv_artwork_winners() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_scheduling_fixture(&pool).await;
    seed_legacy_tv_artwork_variants(&pool).await;

    let show = repos
        .scheduling
        .enrich_admin_search(AdminSearchKind::Show, &[10])
        .await
        .unwrap();
    assert_eq!(show.poster.len(), 1);
    assert_eq!(
        show.poster.get(&10).map(String::as_str),
        Some("show-lowest.jpg")
    );

    let season = repos
        .scheduling
        .enrich_admin_search(AdminSearchKind::Season, &[11])
        .await
        .unwrap();
    assert_eq!(season.poster.len(), 1);
    assert_eq!(season.season_poster.len(), 1);
    assert_eq!(
        season.poster.get(&11).map(String::as_str),
        Some("season-lowest.jpg")
    );
    assert_eq!(
        season.season_poster.get(&11).map(String::as_str),
        Some("show-lowest.jpg")
    );

    let episode = repos
        .scheduling
        .enrich_admin_search(AdminSearchKind::Episode, &[12, 13])
        .await
        .unwrap();
    assert_eq!(episode.poster.len(), 1);
    assert_eq!(episode.season_poster.len(), 2);
    assert_eq!(episode.show_poster.len(), 2);
    assert_eq!(episode.episode_season_id.len(), 2);
    assert_eq!(
        episode.poster.get(&12).map(String::as_str),
        Some("episode-lowest.jpg")
    );
    assert!(!episode.poster.contains_key(&13));
    for episode_id in [12, 13] {
        assert_eq!(
            episode.season_poster.get(&episode_id).map(String::as_str),
            Some("season-lowest.jpg")
        );
        assert_eq!(
            episode.show_poster.get(&episode_id).map(String::as_str),
            Some("show-lowest.jpg")
        );
    }

    let restarted = make_repositories(pool);
    assert_eq!(
        restarted
            .scheduling
            .enrich_admin_search(AdminSearchKind::Show, &[10])
            .await
            .unwrap(),
        show
    );
    assert_eq!(
        restarted
            .scheduling
            .enrich_admin_search(AdminSearchKind::Season, &[11])
            .await
            .unwrap(),
        season
    );
    assert_eq!(
        restarted
            .scheduling
            .enrich_admin_search(AdminSearchKind::Episode, &[12, 13])
            .await
            .unwrap(),
        episode
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
async fn scheduling_repository_expands_nested_smart_collection_queries_and_breaks_cycles() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_scheduling_fixture(&pool).await;
    seed_search_enrichment(&pool).await;

    pool.execute(
        r#"INSERT INTO "SmartCollection" ("Id", "Name", "Query")
         VALUES
            (6, 'inner-smart', 'type:movie AND title:Feature'),
            (7, 'outer-smart', 'smart_collection:"inner-smart" OR title:Bonus'),
            (8, 'cycle-a', 'smart_collection:"cycle-b"'),
            (9, 'cycle-b', 'smart_collection:"cycle-a"')"#,
    )
    .await
    .unwrap();

    let nested = repos
        .scheduling
        .media_items_for_source(&SchedulingContentSource::SmartCollection(
            "outer-smart".to_owned(),
        ))
        .await
        .unwrap();
    assert_eq!(
        nested.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![21, 20]
    );

    let cycle = repos
        .scheduling
        .media_items_for_source(&SchedulingContentSource::SmartCollection(
            "cycle-a".to_owned(),
        ))
        .await
        .unwrap();
    assert!(cycle.is_empty());
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
    // BlockItem 711 points at Collection 1, whose CollectionItems are Show 10
    // and Movie 20. ErsatzTV's MediaCollectionRepository.GetShowItems expands a
    // show in a collection into its episodes, so the resolved set is the show's
    // episodes (12, 13) plus the movie (20) — matching the identical Collection
    // 1 resolution asserted for "deco-default-filler:750" below.
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

fn block_preview_items() -> Vec<EffectiveBlockItem> {
    let keys = [
        SchedulingCollectionKey {
            collection_type: CollectionType::Collection,
            collection_id: Some(1),
            ..SchedulingCollectionKey::default()
        },
        SchedulingCollectionKey {
            collection_type: CollectionType::TelevisionShow,
            media_item_id: Some(10),
            ..SchedulingCollectionKey::default()
        },
        SchedulingCollectionKey {
            collection_type: CollectionType::TelevisionSeason,
            media_item_id: Some(11),
            ..SchedulingCollectionKey::default()
        },
        SchedulingCollectionKey {
            collection_type: CollectionType::Artist,
            media_item_id: Some(30),
            ..SchedulingCollectionKey::default()
        },
        SchedulingCollectionKey {
            collection_type: CollectionType::MultiCollection,
            multi_collection_id: Some(3),
            ..SchedulingCollectionKey::default()
        },
        SchedulingCollectionKey {
            collection_type: CollectionType::SmartCollection,
            smart_collection_id: Some(2),
            ..SchedulingCollectionKey::default()
        },
        SchedulingCollectionKey {
            collection_type: CollectionType::Playlist,
            playlist_id: Some(4),
            ..SchedulingCollectionKey::default()
        },
        SchedulingCollectionKey {
            collection_type: CollectionType::SearchQuery,
            search_query: Some("title contains Pilot".to_owned()),
            ..SchedulingCollectionKey::default()
        },
        SchedulingCollectionKey {
            collection_type: CollectionType::Movie,
            media_item_id: Some(20),
            ..SchedulingCollectionKey::default()
        },
    ];

    keys.into_iter()
        .enumerate()
        .map(|(index, collection_key)| EffectiveBlockItem {
            id: i64::try_from(index).unwrap_or_default() + 1,
            block_id: 1,
            content: format!("preview:{index}"),
            collection_key,
            playback_order: PlaybackOrder::Chronological,
            include_in_program_guide: true,
            disable_watermarks: false,
            watermark_ids: Vec::new(),
            graphics_element_ids: Vec::new(),
        })
        .collect()
}

#[tokio::test]
async fn block_preview_content_uses_real_sql_resolvers_for_supported_sources() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_scheduling_fixture(&pool).await;
    seed_search_enrichment(&pool).await;

    let reference_data = repos
        .scheduling
        .load_block_preview_content(&block_preview_items(), &BTreeMap::new())
        .await
        .unwrap();

    assert_eq!(
        reference_data
            .media_by_key
            .get("preview:0")
            .unwrap()
            .iter()
            .map(|item| item.id)
            .collect::<Vec<_>>(),
        vec![12, 13, 20]
    );
    assert_eq!(
        reference_data
            .media_by_key
            .get("preview:3")
            .unwrap()
            .iter()
            .map(|item| item.id)
            .collect::<Vec<_>>(),
        vec![31]
    );
    assert_eq!(
        reference_data
            .media_by_key
            .get("preview:6")
            .unwrap()
            .iter()
            .map(|item| item.id)
            .collect::<Vec<_>>(),
        vec![12, 13, 20, 21]
    );
    assert_eq!(
        reference_data
            .media_items_by_id
            .get(&12)
            .unwrap()
            .title
            .as_deref(),
        Some("Pilot")
    );
    assert_eq!(
        reference_data
            .media_items_by_id
            .get(&20)
            .unwrap()
            .title
            .as_deref(),
        Some("Feature")
    );
}

#[tokio::test]
async fn block_preview_content_propagates_closed_pool_failure() {
    let pool = common::test_db().await;
    let repository = make_repositories(pool.clone()).scheduling;
    pool.close().await;
    let item = EffectiveBlockItem {
        id: 1,
        block_id: 1,
        content: "preview:closed".to_owned(),
        collection_key: SchedulingCollectionKey {
            collection_type: CollectionType::Collection,
            collection_id: Some(1),
            ..SchedulingCollectionKey::default()
        },
        playback_order: PlaybackOrder::Chronological,
        include_in_program_guide: true,
        disable_watermarks: false,
        watermark_ids: Vec::new(),
        graphics_element_ids: Vec::new(),
    };

    let error = repository
        .load_block_preview_content(&[item], &BTreeMap::new())
        .await
        .expect_err("closed pool must fail");

    assert!(matches!(
        error,
        ersatzrs_core::repositories::RepositoryError::Unavailable { .. }
    ));
}

#[tokio::test]
async fn disabled_block_item_policy_overrides_legacy_playout_policy() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_scheduling_fixture(&pool).await;
    common::seed_profile_and_channel(&repos, 900).await;
    seed_block_scheduling_fixture(&pool).await;

    let legacy_policy = persisted_smart_shuffle_policy();
    sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
        .bind(legacy_playout_cooldown_config_key(900))
        .bind(serde_json::to_string(&legacy_policy).unwrap())
        .execute(&pool)
        .await
        .unwrap();

    let inherited = repos
        .scheduling
        .block_reference_data(
            900,
            Utc.with_ymd_and_hms(2026, 4, 15, 8, 0, 0).unwrap(),
            1,
            Duration::hours(1),
        )
        .await
        .unwrap();
    assert_eq!(
        inherited.smart_shuffle_policy_for_unit("711"),
        &legacy_policy
    );

    let disabled = SchedulingSmartShufflePolicy::default();
    sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
        .bind(block_item_smart_shuffle_config_key(711))
        .bind(serde_json::to_string(&disabled).unwrap())
        .execute(&pool)
        .await
        .unwrap();

    let overridden = repos
        .scheduling
        .block_reference_data(
            900,
            Utc.with_ymd_and_hms(2026, 4, 15, 8, 0, 0).unwrap(),
            1,
            Duration::hours(1),
        )
        .await
        .unwrap();
    assert_eq!(overridden.smart_shuffle_policy_for_unit("711"), &disabled);
}

#[tokio::test]
async fn scheduling_repository_loads_classic_reference_data_from_sql() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_scheduling_fixture(&pool).await;
    common::seed_profile_and_channel(&repos, 901).await;

    repos
        .program_schedule
        .upsert(&ProgramSchedule {
            id: 301,
            name: "Classic Prime".to_owned(),
            item_ids: vec![401],
            ..ProgramSchedule::default()
        })
        .await
        .unwrap();
    let item = ProgramScheduleItem {
        id: 401,
        program_schedule_id: 301,
        collection_type: CollectionType::Collection,
        collection_id: Some(1),
        playout_mode: PlayoutMode::One,
        playback_order: PlaybackOrder::SeasonEpisode,
        sort_order: 1,
        ..ProgramScheduleItem::default()
    };
    repos.program_schedule.upsert_item(&item).await.unwrap();
    pool.execute(
        r#"INSERT INTO "Playout" ("Id", "ChannelId", "ProgramScheduleId", "ScheduleKind", "Seed")
           VALUES (901, 901, 301, 'Classic', 123)"#,
    )
    .await
    .unwrap();

    let reference_data = repos
        .scheduling
        .classic_reference_data(
            901,
            Utc.with_ymd_and_hms(2026, 4, 15, 8, 0, 0).unwrap(),
            Duration::hours(1),
        )
        .await
        .unwrap();

    assert_eq!(reference_data.classic_schedule_id, Some(301));
    assert_eq!(reference_data.classic_schedule_items.len(), 1);
    assert_eq!(
        reference_data
            .media_by_key
            .get(&classic_schedule_item_key(
                &reference_data.classic_schedule_items[0]
            ))
            .unwrap()
            .iter()
            .map(|item| item.id)
            .collect::<Vec<_>>(),
        vec![12, 13, 20]
    );

    let start = Utc.with_ymd_and_hms(2026, 4, 15, 8, 0, 0).unwrap();
    let result = ClassicSchedulingEngine
        .schedule(SchedulingRequest {
            playout: Playout {
                id: 901,
                schedule_kind: PlayoutScheduleKind::Classic,
                seed: 123,
                ..Playout::default()
            },
            start,
            finish: start + Duration::hours(1),
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
        vec![20]
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn classic_scan_impact_resolver_uses_current_dependency_graph_not_runtime_anchors() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_scheduling_fixture(&pool).await;

    common::seed_media_path(&pool, 2, 2, 2).await;
    common::seed_media_item(&pool, 50, 2).await;
    insert_movie(&pool, 50, 500, "Second Library", "2026-03-01T00:00:00Z").await;
    pool.execute(
        r#"INSERT INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder")
           VALUES (5, 'second-library', 0);
           INSERT INTO "CollectionItem" ("CollectionId", "MediaItemId") VALUES (5, 50);
           INSERT INTO "PlaylistGroup" ("Id", "Name") VALUES (42, 'second');
           INSERT INTO "Playlist" ("Id", "Name", "PlaylistGroupId", "IsSystem")
           VALUES (40, 'nightly', 42, 0);
           INSERT INTO "PlaylistItem" (
               "Id", "Index", "PlaylistId", "CollectionType", "MediaItemId",
               "PlaybackOrder", "PlayAll"
           ) VALUES (403, 0, 40, 'Episode', 50, 'Chronological', 1);
           INSERT INTO "FillerPreset" (
               "Id", "Name", "FillerMode", "FillerKind", "CollectionType",
               "PlaylistId", "AllowWatermarks", "UseChaptersAsMediaItems"
           ) VALUES (60, 'managed playlist filler', 'Count', 'PreRoll', 'Playlist', 4, 0, 0);
           INSERT INTO "RerunCollection" (
               "Id", "Name", "CollectionType", "SmartCollectionId",
               "FirstRunPlaybackOrder", "RerunPlaybackOrder"
           ) VALUES (6, 'bonus-rerun', 'SmartCollection', 2, 'Chronological', 'Chronological')"#,
    )
    .await
    .unwrap();

    let cases = [
        (
            1_101,
            1_201,
            ProgramScheduleItem {
                collection_type: CollectionType::Collection,
                collection_id: Some(1),
                ..ProgramScheduleItem::default()
            },
        ),
        (
            1_102,
            1_202,
            ProgramScheduleItem {
                collection_type: CollectionType::SmartCollection,
                smart_collection_id: Some(2),
                ..ProgramScheduleItem::default()
            },
        ),
        (
            1_103,
            1_203,
            ProgramScheduleItem {
                collection_type: CollectionType::MultiCollection,
                multi_collection_id: Some(3),
                ..ProgramScheduleItem::default()
            },
        ),
        (
            1_104,
            1_204,
            ProgramScheduleItem {
                collection_type: CollectionType::Playlist,
                playlist_id: Some(4),
                ..ProgramScheduleItem::default()
            },
        ),
        (
            1_105,
            1_205,
            ProgramScheduleItem {
                collection_type: CollectionType::Episode,
                media_item_id: Some(13),
                ..ProgramScheduleItem::default()
            },
        ),
        (
            1_106,
            1_206,
            ProgramScheduleItem {
                collection_type: CollectionType::Collection,
                collection_id: Some(5),
                ..ProgramScheduleItem::default()
            },
        ),
        (
            1_107,
            1_207,
            ProgramScheduleItem {
                collection_type: CollectionType::RerunFirstRun,
                rerun_collection_id: Some(6),
                ..ProgramScheduleItem::default()
            },
        ),
        (
            1_108,
            1_208,
            ProgramScheduleItem {
                collection_type: CollectionType::Playlist,
                playlist_id: Some(40),
                ..ProgramScheduleItem::default()
            },
        ),
        (
            1_109,
            1_209,
            ProgramScheduleItem {
                collection_type: CollectionType::Episode,
                media_item_id: Some(13),
                pre_roll_filler_id: Some(60),
                ..ProgramScheduleItem::default()
            },
        ),
        (
            1_110,
            1_210,
            ProgramScheduleItem {
                collection_type: CollectionType::Collection,
                collection_id: Some(5),
                playlist_id: Some(4),
                ..ProgramScheduleItem::default()
            },
        ),
    ];
    for (playout_id, schedule_id, item) in cases {
        seed_classic_impact_playout(&repos, &pool, playout_id, schedule_id, item).await;
    }

    repos
        .program_schedule
        .upsert(&ProgramSchedule {
            id: 1_305,
            name: "Alternate smart bonus".to_owned(),
            ..ProgramSchedule::default()
        })
        .await
        .unwrap();
    repos
        .program_schedule
        .upsert_item(&ProgramScheduleItem {
            id: 2_305,
            program_schedule_id: 1_305,
            collection_type: CollectionType::SmartCollection,
            smart_collection_id: Some(2),
            ..ProgramScheduleItem::default()
        })
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "ProgramScheduleAlternate" (
               "PlayoutId", "ProgramScheduleId", "Index", "LimitToDateRange",
               "StartMonth", "StartDay", "EndMonth", "EndDay"
           ) VALUES (1105, 1305, 0, 0, 1, 1, 12, 31)"#,
    )
    .await
    .unwrap();

    assert_eq!(
        repos
            .scheduling
            .classic_playouts_affected_by_scan(&BTreeSet::from([20]), None)
            .await
            .unwrap(),
        [1_101, 1_103, 1_104, 1_109]
    );
    assert_eq!(
        repos
            .scheduling
            .classic_playouts_affected_by_scan(&BTreeSet::from([21]), None)
            .await
            .unwrap(),
        [1_102, 1_103, 1_104, 1_105, 1_107, 1_109]
    );
    assert_eq!(
        repos
            .scheduling
            .classic_playouts_affected_by_scan(&BTreeSet::new(), Some(4))
            .await
            .unwrap(),
        [1_104, 1_109]
    );
    assert_eq!(
        repos
            .scheduling
            .classic_playouts_affected_by_scan(&BTreeSet::from([50]), None)
            .await
            .unwrap(),
        [1_106, 1_108, 1_110]
    );
    assert!(repos
        .scheduling
        .classic_playouts_affected_by_scan(&BTreeSet::from([999]), None)
        .await
        .unwrap()
        .is_empty());
}

async fn seed_classic_impact_playout(
    repos: &Repositories,
    pool: &SqlitePool,
    playout_id: i64,
    schedule_id: i64,
    mut item: ProgramScheduleItem,
) {
    common::seed_profile_and_channel(repos, playout_id).await;
    repos
        .program_schedule
        .upsert(&ProgramSchedule {
            id: schedule_id,
            name: format!("Impact {playout_id}"),
            ..ProgramSchedule::default()
        })
        .await
        .unwrap();
    item.id = schedule_id + 1_000;
    item.program_schedule_id = schedule_id;
    repos.program_schedule.upsert_item(&item).await.unwrap();
    sqlx::query(
        r#"INSERT INTO "Playout" (
               "Id", "ChannelId", "ProgramScheduleId", "ScheduleKind", "Seed"
           ) VALUES (?, ?, ?, 'Classic', 123)"#,
    )
    .bind(playout_id)
    .bind(playout_id)
    .bind(schedule_id)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn scheduling_repository_hydrates_persisted_smart_shuffle_policy_and_similarity_tags() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_scheduling_fixture(&pool).await;
    seed_search_enrichment(&pool).await;
    pool.execute(
        r#"INSERT INTO "Tag" ("Id", "Name", "EpisodeMetadataId")
           VALUES (99, 'Network', 120)"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "MusicLabel" ("Id", "Name", "EpisodeMetadataId")
           VALUES (99, 'Public Media', 120);
           INSERT INTO "MetadataLanguage" ("Id", "Code", "EpisodeMetadataId")
           VALUES (99, 'nld', 120)"#,
    )
    .await
    .unwrap();
    common::seed_profile_and_channel(&repos, 903).await;

    repos
        .program_schedule
        .upsert(&ProgramSchedule {
            id: 303,
            name: "Cooldown Prime".to_owned(),
            item_ids: vec![403],
            ..ProgramSchedule::default()
        })
        .await
        .unwrap();
    repos
        .program_schedule
        .upsert_item(&ProgramScheduleItem {
            id: 403,
            program_schedule_id: 303,
            collection_type: CollectionType::Collection,
            collection_id: Some(1),
            playout_mode: PlayoutMode::One,
            playback_order: PlaybackOrder::Shuffle,
            sort_order: 1,
            ..ProgramScheduleItem::default()
        })
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "Playout" ("Id", "ChannelId", "ProgramScheduleId", "ScheduleKind", "Seed")
           VALUES (903, 903, 303, 'Classic', 123)"#,
    )
    .await
    .unwrap();

    let policy = persisted_smart_shuffle_policy();
    sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
        .bind(program_schedule_item_smart_shuffle_config_key(403))
        .bind(serde_json::to_string(&policy).unwrap())
        .execute(&pool)
        .await
        .unwrap();

    let reference_data = repos
        .scheduling
        .classic_reference_data(
            903,
            Utc.with_ymd_and_hms(2026, 4, 15, 8, 0, 0).unwrap(),
            Duration::hours(1),
        )
        .await
        .unwrap();

    let unit_key = classic_schedule_item_policy_key(&ProgramScheduleItem {
        id: 403,
        program_schedule_id: 303,
        collection_type: CollectionType::Collection,
        collection_id: Some(1),
        playout_mode: PlayoutMode::One,
        playback_order: PlaybackOrder::Shuffle,
        sort_order: 1,
        ..ProgramScheduleItem::default()
    });
    assert_eq!(
        reference_data.smart_shuffle_policy_for_unit(&unit_key),
        &policy
    );
    let pilot = reference_data.smart_shuffle_tags_for_media(&unit_key, 12);
    assert!(pilot.contains("show:the show"));
    assert!(pilot.contains("season:the show:1"));
    assert!(pilot.contains("tag:network"));
    assert!(pilot.contains("content_rating:tv-14"));
    assert!(pilot.contains("language:dut"));
    assert!(pilot.contains("music_label:public media"));
    assert!(pilot.contains(&format!("filter:{unit_key}:pilot")));
    let finale = reference_data.smart_shuffle_tags_for_media(&unit_key, 13);
    assert!(finale.contains("show:the show"));
    assert!(!finale.contains(&format!("filter:{unit_key}:pilot")));
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn classic_duplicate_sources_share_media_but_isolate_item_policies_and_tags() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_scheduling_fixture(&pool).await;
    seed_search_enrichment(&pool).await;
    common::seed_profile_and_channel(&repos, 904).await;
    repos
        .program_schedule
        .upsert(&ProgramSchedule {
            id: 304,
            name: "Duplicate Sources".to_owned(),
            item_ids: vec![404, 405],
            ..ProgramSchedule::default()
        })
        .await
        .unwrap();
    let item = |id, sort_order| ProgramScheduleItem {
        id,
        program_schedule_id: 304,
        collection_type: CollectionType::Collection,
        collection_id: Some(1),
        playout_mode: PlayoutMode::One,
        playback_order: PlaybackOrder::Shuffle,
        sort_order,
        ..ProgramScheduleItem::default()
    };
    repos
        .program_schedule
        .upsert_item(&item(404, 0))
        .await
        .unwrap();
    repos
        .program_schedule
        .upsert_item(&item(405, 1))
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "Playout" ("Id", "ChannelId", "ProgramScheduleId", "ScheduleKind", "Seed")
           VALUES (904, 904, 304, 'Classic', 123)"#,
    )
    .await
    .unwrap();
    let filter_policy = |search: &str| SchedulingSmartShufflePolicy {
        enabled: true,
        preset: SchedulingSmartShufflePreset::Custom,
        after_items: Some(2),
        rules: vec![SchedulingSmartShuffleRule {
            id: "segment".to_owned(),
            kind: SchedulingSmartShuffleRuleKind::Filter,
            search: Some(search.to_owned()),
        }],
        ..SchedulingSmartShufflePolicy::default()
    };
    let pilot_policy = filter_policy("title contains Pilot");
    let finale_policy = filter_policy("title contains Finale");
    for (id, policy) in [(404_i64, &pilot_policy), (405_i64, &finale_policy)] {
        sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
            .bind(program_schedule_item_smart_shuffle_config_key(id))
            .bind(serde_json::to_string(policy).unwrap())
            .execute(&pool)
            .await
            .unwrap();
    }

    let reference_data = repos
        .scheduling
        .classic_reference_data(
            904,
            Utc.with_ymd_and_hms(2026, 4, 15, 8, 0, 0).unwrap(),
            Duration::hours(1),
        )
        .await
        .unwrap();
    let first = &reference_data.classic_schedule_items[0];
    let second = &reference_data.classic_schedule_items[1];
    let content_key = classic_schedule_item_key(first);
    let first_policy_key = classic_schedule_item_policy_key(first);
    let second_policy_key = classic_schedule_item_policy_key(second);

    assert_eq!(content_key, classic_schedule_item_key(second));
    assert_eq!(reference_data.media_by_key.len(), 1);
    assert_eq!(
        reference_data
            .media_by_key
            .get(&content_key)
            .unwrap()
            .iter()
            .map(|media| media.id)
            .collect::<Vec<_>>(),
        vec![12, 13, 20]
    );
    assert_eq!(
        reference_data.smart_shuffle_policy_for_unit(&first_policy_key),
        &pilot_policy
    );
    assert_eq!(
        reference_data.smart_shuffle_policy_for_unit(&second_policy_key),
        &finale_policy
    );
    let first_filter_tag = format!("filter:{first_policy_key}:segment");
    let second_filter_tag = format!("filter:{second_policy_key}:segment");
    assert_ne!(first_filter_tag, second_filter_tag);
    assert!(reference_data
        .smart_shuffle_tags_for_media(&first_policy_key, 12)
        .contains(&first_filter_tag));
    assert!(!reference_data
        .smart_shuffle_tags_for_media(&first_policy_key, 12)
        .contains(&second_filter_tag));
    assert!(reference_data
        .smart_shuffle_tags_for_media(&first_policy_key, 13)
        .is_empty());
    assert!(reference_data
        .smart_shuffle_tags_for_media(&second_policy_key, 12)
        .is_empty());
    assert!(reference_data
        .smart_shuffle_tags_for_media(&second_policy_key, 13)
        .contains(&second_filter_tag));
    assert!(!reference_data
        .smart_shuffle_tags_for_media(&second_policy_key, 13)
        .contains(&first_filter_tag));
}

#[tokio::test]
async fn scheduling_repository_hydrates_multiline_scripted_smart_shuffle_declaration() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_scheduling_fixture(&pool).await;
    seed_search_enrichment(&pool).await;
    let script = r#"
set_smart_shuffle_policy(
    "main",
    [[
        {
            "version": 1,
            "enabled": true,
            "preset": "custom",
            "scope": "playout",
            "mode": "soft",
            "after_items": 2,
            "include_filler": false,
            "rules": [{"id": "show", "kind": "show"}]
        }
    ]]
)
"#;
    let (policies, legacy) =
        ScriptedSchedulingEngine::smart_shuffle_policies_from_script(script).unwrap();

    let reference_data = repos
        .scheduling
        .scripted_reference_data(903, policies, legacy)
        .await
        .unwrap();

    assert!(reference_data
        .smart_shuffle_policy_for_unit("main")
        .is_active());
    assert!(reference_data
        .smart_shuffle_tags_for_media("main", 12)
        .contains("show:the show"));
}

#[tokio::test]
async fn scheduling_repository_loads_imported_classic_item_filler_from_sql() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_scheduling_fixture(&pool).await;
    common::seed_profile_and_channel(&repos, 902).await;

    pool.execute(
        r#"INSERT INTO "FillerPreset" (
               "Id", "Name", "FillerMode", "FillerKind", "CollectionType",
               "CollectionId", "Count", "AllowWatermarks", "UseChaptersAsMediaItems"
           ) VALUES (77, 'Imported Pre Roll', 'Count', 'PreRoll', 'Collection', 1, 1, 0, 0)"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "ProgramSchedule" (
               "Id", "Name", "KeepMultiPartEpisodesTogether", "TreatCollectionsAsShows",
               "ShuffleScheduleItems", "RandomStartPoint", "FixedStartTimeBehavior"
           ) VALUES (302, 'Imported Classic', 1, 0, 0, 0, 'Strict')"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "ProgramScheduleItem" (
               "Id", "ProgramScheduleId", "CollectionType", "CollectionId",
               "PlaybackOrder", "FillWithGroupMode", "GuideMode", "MarathonGroupBy",
               "MarathonShuffleGroups", "MarathonShuffleItems", "Index", "PreRollFillerId"
           ) VALUES (
               402, 302, 'Collection', 1, 'Chronological', 'None', 'Normal',
               'None', 0, 0, 0, 77
           )"#,
    )
    .await
    .unwrap();
    pool.execute(r#"INSERT INTO "ProgramScheduleOneItem" ("Id") VALUES (402)"#)
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "Playout" ("Id", "ChannelId", "ProgramScheduleId", "ScheduleKind", "Seed")
           VALUES (902, 902, 302, 'Classic', 123)"#,
    )
    .await
    .unwrap();

    let start = Utc.with_ymd_and_hms(2026, 4, 15, 8, 0, 0).unwrap();
    let reference_data = repos
        .scheduling
        .classic_reference_data(902, start, Duration::hours(1))
        .await
        .unwrap();

    let item = reference_data
        .classic_schedule_items
        .first()
        .expect("classic schedule item");
    assert_eq!(item.pre_roll_filler_id, Some(77));
    let filler = reference_data
        .classic_filler_presets
        .get(&77)
        .expect("filler preset");
    assert_eq!(filler.count, Some(1));
    assert!(!filler.allow_watermarks);

    let result = ClassicSchedulingEngine
        .schedule(SchedulingRequest {
            playout: Playout {
                id: 902,
                schedule_kind: PlayoutScheduleKind::Classic,
                seed: 123,
                ..Playout::default()
            },
            start,
            finish: start + Duration::hours(1),
            mode: ersatzrs_core::scheduling::PlayoutBuildMode::Continue,
            reference_data,
        })
        .unwrap();

    assert_eq!(
        result
            .added_items
            .iter()
            .take(2)
            .map(|item| (
                item.media_item_id,
                item.filler_kind,
                item.disable_watermarks
            ))
            .collect::<Vec<_>>(),
        vec![
            (12, FillerKind::PreRoll, true),
            (20, FillerKind::None, false)
        ]
    );

    assert_classic_chapter_filler(&pool).await;
}

async fn assert_classic_chapter_filler(pool: &SqlitePool) {
    let repos = make_repositories(pool.clone());
    let start = Utc.with_ymd_and_hms(2026, 4, 15, 8, 0, 0).unwrap();
    pool.execute(
        r#"UPDATE "FillerPreset" SET "UseChaptersAsMediaItems" = 1, "Count" = 2 WHERE "Id" = 77"#,
    )
    .await
    .unwrap();
    pool.execute(r#"INSERT INTO "MediaChapter" ("Id", "MediaVersionId", "ChapterId", "StartTime", "EndTime", "Title") VALUES
        (9001, 120, 0, '00:00:00', '00:02:00', 'Intro'),
        (9002, 120, 1, '00:10:00', '00:13:00', 'Second chapter')"#).await.unwrap();
    let reference_data = repos
        .scheduling
        .classic_reference_data(902, start, Duration::hours(1))
        .await
        .unwrap();
    let filler = &reference_data.classic_filler_presets[&77];
    assert_eq!(filler.media_items[0].duration, Duration::minutes(2));
    assert_eq!(filler.media_items[1].duration, Duration::minutes(3));
    let result = ClassicSchedulingEngine
        .schedule(SchedulingRequest {
            playout: Playout {
                id: 902,
                schedule_kind: PlayoutScheduleKind::Classic,
                ..Playout::default()
            },
            start,
            finish: start + Duration::hours(1),
            mode: ersatzrs_core::scheduling::PlayoutBuildMode::Continue,
            reference_data,
        })
        .unwrap();
    let chapter = &result.added_items[1];
    assert_eq!(chapter.media_item_id, 12);
    assert_eq!(chapter.in_point, Duration::minutes(10));
    assert_eq!(chapter.out_point, Duration::minutes(13));
    assert_eq!(chapter.chapter_title.as_deref(), Some("Second chapter"));
    assert_eq!(chapter.finish - chapter.start, Duration::minutes(3));
    pool.execute(r#"UPDATE "ProgramScheduleItem" SET "MidRollFillerId" = 77 WHERE "Id" = 402"#)
        .await
        .unwrap();
    let reference_data = repos
        .scheduling
        .classic_reference_data(902, start, Duration::hours(1))
        .await
        .unwrap();
    let boundaries = &reference_data.classic_chapters_by_media_id[&(12, None)];
    assert_eq!(boundaries.len(), 2);
    assert_eq!(boundaries[1].start_time, Duration::minutes(10));
    assert_eq!(boundaries[1].end_time, Duration::minutes(13));
    assert_eq!(boundaries[1].title, "Second chapter");
    assert_classic_pinned_chapter_versions(pool).await;
}

async fn assert_classic_pinned_chapter_versions(pool: &SqlitePool) {
    pool.execute(r#"INSERT INTO "MediaVersion" ("Id", "EpisodeId", "Name", "Duration", "VideoScanKind", "DateAdded", "DateUpdated", "Width", "Height")
        VALUES (121, 12, 'alternate', '00:20:00', 'Progressive', '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z', 1920, 1080)"#).await.unwrap();
    pool.execute(r#"INSERT INTO "MediaChapter" ("Id", "MediaVersionId", "ChapterId", "StartTime", "EndTime", "Title") VALUES
        (9003, 121, 0, '00:00:00', '00:04:00', 'Alternate intro'),
        (9004, 121, 1, '00:04:00', '00:20:00', 'Alternate end')"#).await.unwrap();
    pool.execute(r#"UPDATE "PlaylistItem" SET "MediaVersionId" = 121, "VideoStreamIndex" = 0 WHERE "Id" = 401"#).await.unwrap();
    pool.execute(r#"UPDATE "ProgramScheduleItem" SET "CollectionType" = 'Playlist', "CollectionId" = NULL, "PlaylistId" = 4 WHERE "Id" = 402"#).await.unwrap();
    pool.execute(r#"UPDATE "FillerPreset" SET "CollectionType" = 'Playlist', "CollectionId" = NULL, "PlaylistId" = 4 WHERE "Id" = 77"#).await.unwrap();
    let repos = make_repositories(pool.clone());
    let start = Utc.with_ymd_and_hms(2026, 4, 15, 8, 0, 0).unwrap();
    let data = repos
        .scheduling
        .classic_reference_data(902, start, Duration::hours(1))
        .await
        .unwrap();
    assert_eq!(
        data.classic_chapters_by_media_id[&(12, Some(121))][1].start_time,
        Duration::minutes(4)
    );
    let filler = &data.classic_filler_presets[&77].media_items;
    let alternate = filler
        .iter()
        .find(|item| {
            item.id == 12
                && item
                    .chapter
                    .as_ref()
                    .is_some_and(|chapter| chapter.title.as_deref() == Some("Alternate end"))
        })
        .unwrap();
    assert_eq!(
        alternate.chapter.as_ref().unwrap().start,
        Duration::minutes(4)
    );
    assert_eq!(
        alternate.pinned_stream_selection.unwrap().media_version_id,
        121
    );
    // The schedule's exact pin overrides a playlist-row pin.
    pool.execute(r#"UPDATE "ProgramScheduleItem" SET "MediaVersionId" = 120, "VideoStreamIndex" = 0 WHERE "Id" = 402"#).await.unwrap();
    let data = repos
        .scheduling
        .classic_reference_data(902, start, Duration::hours(1))
        .await
        .unwrap();
    assert_eq!(
        data.classic_chapters_by_media_id[&(12, Some(120))][1].start_time,
        Duration::minutes(10)
    );
    // A version belonging to another item cannot supply chapter seeks.
    pool.execute(r#"UPDATE "ProgramScheduleItem" SET "MediaVersionId" = 200 WHERE "Id" = 402"#)
        .await
        .unwrap();
    let data = repos
        .scheduling
        .classic_reference_data(902, start, Duration::hours(1))
        .await
        .unwrap();
    assert!(data.classic_chapters_by_media_id[&(12, Some(200))].is_empty());
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

async fn seed_legacy_tv_artwork_variants(pool: &SqlitePool) {
    pool.execute(
        r#"INSERT INTO "Artwork" (
               "Id", "ArtworkKind", "Path", "ShowMetadataId", "SeasonMetadataId",
               "EpisodeMetadataId", "DateAdded", "DateUpdated"
           ) VALUES
               (9000, 0, NULL, NULL, 110, NULL, '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z'),
               (9001, 0, '   ', NULL, 110, NULL, '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z'),
               (9002, 0, 'season-lowest.jpg', NULL, 110, NULL, '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z'),
               (9003, 0, 'season-lowest.jpg', NULL, 110, NULL, '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z'),
               (9004, 0, 'season-later.jpg', NULL, 110, NULL, '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z'),
               (9010, 0, NULL, 100, NULL, NULL, '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z'),
               (9011, 0, '   ', 100, NULL, NULL, '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z'),
               (9012, 0, 'show-lowest.jpg', 100, NULL, NULL, '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z'),
               (9013, 0, 'show-lowest.jpg', 100, NULL, NULL, '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z'),
               (9014, 0, 'show-later.jpg', 100, NULL, NULL, '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z'),
               (9020, 3, '   ', 100, NULL, NULL, '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z'),
               (9021, 3, 'fanart-lowest.jpg', 100, NULL, NULL, '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z'),
               (9022, 3, 'fanart-lowest.jpg', 100, NULL, NULL, '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z'),
               (9023, 3, 'fanart-later.jpg', 100, NULL, NULL, '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z'),
               (9030, 0, NULL, NULL, NULL, 120, '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z'),
               (9031, 0, '   ', NULL, NULL, 120, '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z'),
               (9032, 0, 'episode-lowest.jpg', NULL, NULL, 120, '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z'),
               (9033, 0, 'episode-lowest.jpg', NULL, NULL, 120, '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z'),
               (9034, 0, 'episode-later.jpg', NULL, NULL, 120, '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z')"#,
    )
    .await
    .unwrap();
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

#[allow(clippy::too_many_lines)]
async fn seed_search_enrichment(pool: &SqlitePool) {
    pool.execute(
        r#"UPDATE "MovieMetadata" SET "Year" = 2025, "SortTitle" = 'feature',
           "OriginalTitle" = 'Original Feature', "ContentRating" = 'TV-MA' WHERE "Id" = 200"#,
    )
    .await
    .unwrap();
    pool.execute(r#"UPDATE "ShowMetadata" SET "ContentRating" = 'TV-14' WHERE "Id" = 100"#)
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
        r#"INSERT INTO "Tag" ("Id", "Name", "ShowMetadataId", "ExternalTypeId")
         VALUES (2, 'Network', 100, '319')"#,
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
            "Default", "Forced", "MediaVersionId", "PixelFormat", "BitsPerRawSample",
            "ColorTransfer", "Title"
         ) VALUES
            (1, 0, 'h264', 1, NULL, 0, 1, 0, 200, 'yuv420p10le', 0, 'smpte2084', NULL),
            (2, 1, 'aac', 2, 'eng', 2, 1, 0, 200, NULL, 0, NULL, 'Director Commentary'),
            (3, 2, 'ass', 3, 'nld', 0, 0, 0, 200, NULL, 0, NULL, 'Commentary Subtitles')"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "LibraryFolder" ("Id", "LibraryPathId", "Path")
         VALUES (70, 1, '/media/movies')"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "MediaFile" ("Id", "LibraryFolderId", "MediaVersionId", "Path", "PathHash")
         VALUES (70, 70, 200, '/media/movies/feature.mkv', 'feature-hash')"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "MediaChapter" (
            "Id", "MediaVersionId", "ChapterId", "StartTime", "EndTime", "Title"
         ) VALUES
            (1, 200, 0, '00:00:00', '00:30:00', 'First Half'),
            (2, 200, 1, '00:30:00', '01:00:00', 'Second Half')"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "Subtitle" (
            "Id", "Codec", "DateAdded", "DateUpdated", "Default", "Forced",
            "IsExtracted", "Language", "MovieMetadataId", "SDH", "StreamIndex",
            "SubtitleKind", "Title"
         ) VALUES (
            1, 'srt', '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z', 0, 0,
            0, 'nld', 200, 0, 3, 0, 'Commentary Sidecar'
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
    pool.execute(
        r#"INSERT INTO "Style" ("Id", "Name", "ArtistMetadataId") VALUES (1, 'Lush', 300)"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "Mood" ("Id", "Name", "ArtistMetadataId") VALUES (1, 'Melancholy', 300)"#,
    )
    .await
    .unwrap();
    pool.execute(r#"INSERT INTO "MusicVideo" ("Id", "ArtistId") VALUES (31, 30)"#)
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "MusicVideoMetadata" (
            "Id", "MusicVideoId", "MetadataKind", "Title", "SortTitle",
            "DateAdded", "DateUpdated", "Album"
         ) VALUES (310, 31, 0, 'The Hit', 'hit', '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z', 'Hits')"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "MusicVideoArtist" ("Id", "Name", "MusicVideoMetadataId")
         VALUES (1, 'The Band', 310)"#,
    )
    .await
    .unwrap();
    insert_version(pool, 31, 240_000, "MusicVideoId").await;
}

#[allow(clippy::too_many_lines)]
async fn seed_uniform_search_facets(pool: &SqlitePool) {
    pool.execute(
        r#"UPDATE "MovieMetadata" SET "ContentRating" = 'Uniform Rating' WHERE "Id" = 200;
           UPDATE "ShowMetadata" SET "ContentRating" = 'Uniform Rating' WHERE "Id" = 100;
           UPDATE "MusicVideoMetadata" SET "ContentRating" = 'Uniform Rating' WHERE "Id" = 310"#,
    )
    .await
    .unwrap();

    for id in [50, 51, 52] {
        common::seed_media_item(pool, id, 1).await;
    }
    pool.execute(
        r#"INSERT INTO "OtherVideo" ("Id") VALUES (50);
           INSERT INTO "OtherVideoMetadata" (
               "Id", "OtherVideoId", "MetadataKind", "Title", "SortTitle",
               "DateAdded", "DateUpdated", "ContentRating"
           ) VALUES (
               500, 50, 0, 'Uniform Other Video', 'uniform other video',
               '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z', 'Uniform Rating'
           );
           INSERT INTO "Song" ("Id") VALUES (51);
           INSERT INTO "SongMetadata" (
               "Id", "SongId", "MetadataKind", "Title", "SortTitle",
               "DateAdded", "DateUpdated", "ContentRating"
           ) VALUES (
               510, 51, 0, 'Uniform Song', 'uniform song',
               '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z', 'Uniform Rating'
           );
           INSERT INTO "Image" ("Id") VALUES (52);
           INSERT INTO "ImageMetadata" (
               "Id", "ImageId", "MetadataKind", "Title", "SortTitle",
               "DateAdded", "DateUpdated", "ContentRating"
           ) VALUES (
               520, 52, 0, 'Uniform Image', 'uniform image',
               '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z', 'Uniform Rating'
           )"#,
    )
    .await
    .unwrap();
    insert_version(pool, 50, 600_000, "OtherVideoId").await;
    insert_version(pool, 51, 180_000, "SongId").await;
    insert_version(pool, 52, 10_000, "ImageId").await;

    pool.execute(
        r#"INSERT INTO "Genre" ("Id", "Name", "MovieMetadataId") VALUES (600, 'Uniform Genre', 200);
           INSERT INTO "Genre" ("Id", "Name", "ShowMetadataId") VALUES (601, 'Uniform Genre', 100);
           INSERT INTO "Genre" ("Id", "Name", "EpisodeMetadataId") VALUES (602, 'Uniform Genre', 120);
           INSERT INTO "Genre" ("Id", "Name", "MusicVideoMetadataId") VALUES (603, 'Uniform Genre', 310);
           INSERT INTO "Genre" ("Id", "Name", "OtherVideoMetadataId") VALUES (604, 'Uniform Genre', 500);
           INSERT INTO "Genre" ("Id", "Name", "SongMetadataId") VALUES (605, 'Uniform Genre', 510);
           INSERT INTO "Genre" ("Id", "Name", "ImageMetadataId") VALUES (606, 'Uniform Genre', 520);
           INSERT INTO "Tag" ("Id", "Name", "MovieMetadataId") VALUES (600, 'Uniform Tag', 200);
           INSERT INTO "Tag" ("Id", "Name", "ShowMetadataId") VALUES (601, 'Uniform Tag', 100);
           INSERT INTO "Tag" ("Id", "Name", "EpisodeMetadataId") VALUES (602, 'Uniform Tag', 120);
           INSERT INTO "Tag" ("Id", "Name", "MusicVideoMetadataId") VALUES (603, 'Uniform Tag', 310);
           INSERT INTO "Tag" ("Id", "Name", "OtherVideoMetadataId") VALUES (604, 'Uniform Tag', 500);
           INSERT INTO "Tag" ("Id", "Name", "SongMetadataId") VALUES (605, 'Uniform Tag', 510);
           INSERT INTO "Tag" ("Id", "Name", "ImageMetadataId") VALUES (606, 'Uniform Tag', 520);
           INSERT INTO "Studio" ("Id", "Name", "MovieMetadataId") VALUES (600, 'Uniform Studio', 200);
           INSERT INTO "Studio" ("Id", "Name", "ShowMetadataId") VALUES (601, 'Uniform Studio', 100);
           INSERT INTO "Studio" ("Id", "Name", "EpisodeMetadataId") VALUES (602, 'Uniform Studio', 120);
           INSERT INTO "Studio" ("Id", "Name", "MusicVideoMetadataId") VALUES (603, 'Uniform Studio', 310);
           INSERT INTO "Studio" ("Id", "Name", "OtherVideoMetadataId") VALUES (604, 'Uniform Studio', 500);
           INSERT INTO "Studio" ("Id", "Name", "SongMetadataId") VALUES (605, 'Uniform Studio', 510);
           INSERT INTO "Studio" ("Id", "Name", "ImageMetadataId") VALUES (606, 'Uniform Studio', 520);
           INSERT INTO "MusicLabel" ("Id", "Name", "MovieMetadataId") VALUES (600, 'Uniform Label', 200);
           INSERT INTO "MusicLabel" ("Id", "Name", "ShowMetadataId") VALUES (601, 'Uniform Label', 100);
           INSERT INTO "MusicLabel" ("Id", "Name", "EpisodeMetadataId") VALUES (602, 'Uniform Label', 120);
           INSERT INTO "MusicLabel" ("Id", "Name", "MusicVideoMetadataId") VALUES (603, 'Uniform Label', 310);
           INSERT INTO "MusicLabel" ("Id", "Name", "OtherVideoMetadataId") VALUES (604, 'Uniform Label', 500);
           INSERT INTO "MusicLabel" ("Id", "Name", "SongMetadataId") VALUES (605, 'Uniform Label', 510);
           INSERT INTO "MusicLabel" ("Id", "Name", "ImageMetadataId") VALUES (606, 'Uniform Label', 520);
           INSERT INTO "MetadataLanguage" ("Id", "Code", "MovieMetadataId") VALUES (600, 'nl', 200);
           INSERT INTO "MetadataLanguage" ("Id", "Code", "ShowMetadataId") VALUES (601, 'nl', 100);
           INSERT INTO "MetadataLanguage" ("Id", "Code", "EpisodeMetadataId") VALUES (602, 'nl', 120);
           INSERT INTO "MetadataLanguage" ("Id", "Code", "MusicVideoMetadataId") VALUES (603, 'nl', 310);
           INSERT INTO "MetadataLanguage" ("Id", "Code", "OtherVideoMetadataId") VALUES (604, 'nl', 500);
           INSERT INTO "MetadataLanguage" ("Id", "Code", "SongMetadataId") VALUES (605, 'nl', 510);
           INSERT INTO "MetadataLanguage" ("Id", "Code", "ImageMetadataId") VALUES (606, 'nl', 520)"#,
    )
    .await
    .unwrap();
}

async fn seed_content_variant_example_rows(pool: &SqlitePool) {
    common::seed_media_item(pool, 32, 1).await;
    pool.execute(r#"INSERT INTO "MusicVideo" ("Id", "ArtistId") VALUES (32, 30)"#)
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "MusicVideoMetadata" (
            "Id", "MusicVideoId", "MetadataKind", "Title", "SortTitle",
            "DateAdded", "DateUpdated", "Album"
         ) VALUES (
            320, 32, 0, 'The Hit', 'hit',
            '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z', 'Hits'
         )"#,
    )
    .await
    .unwrap();
    insert_version(pool, 32, 240_000, "MusicVideoId").await;
    pool.execute(
        r#"UPDATE "MediaItem"
           SET "ContentKind" = 2, "SubKind" = 'Unplugged', "ParentMediaItemId" = 32
           WHERE "Id" = 31"#,
    )
    .await
    .unwrap();

    sqlx::query(
        r#"INSERT INTO "MediaItem" (
            "Id", "LibraryPathId", "State", "ContentKind", "SubKind", "ParentMediaItemId"
         ) VALUES (40, 1, 0, 1, 'Featurettes', 20)"#,
    )
    .execute(pool)
    .await
    .unwrap();
    pool.execute(r#"INSERT INTO "OtherVideo" ("Id") VALUES (40)"#)
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "OtherVideoMetadata" (
            "Id", "OtherVideoId", "Title", "SortTitle", "DateAdded", "DateUpdated", "MetadataKind"
         ) VALUES (
            400, 40, 'Making The Escape', 'making the escape',
            '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z', 0
         )"#,
    )
    .await
    .unwrap();
    insert_version(pool, 40, 600_000, "OtherVideoId").await;
}

async fn insert_content_variant_example_smart_collections(pool: &SqlitePool) {
    for (id, example) in [
        (51, content_variants::UNPLUGGED_RENDITIONS),
        (52, content_variants::COMMENTARY_MOVIES),
        (53, content_variants::FEATURETTE_PRE_ROLL),
        (54, content_variants::MOVIE_PRIMARIES),
    ] {
        sqlx::query(r#"INSERT INTO "SmartCollection" ("Id", "Name", "Query") VALUES (?, ?, ?)"#)
            .bind(id)
            .bind(example.name)
            .bind(example.query)
            .execute(pool)
            .await
            .unwrap();
    }
}

async fn seed_collection_fixture(pool: &SqlitePool) {
    pool.execute(
        r#"INSERT INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder")
         VALUES (1, 'prime', 0)"#,
    )
    .await
    .unwrap();
    // CollectionItem is keyed by a surrogate Id (ADR 0048) so the same media
    // item may appear more than once with distinct playback overrides
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
    sqlx::query(ersatzrs_infra::db::assert_sql_safe(&sql))
        .bind(media_item_id * 10)
        .bind(media_item_id)
        .bind(duration_text)
        .execute(pool)
        .await
        .unwrap();
}
