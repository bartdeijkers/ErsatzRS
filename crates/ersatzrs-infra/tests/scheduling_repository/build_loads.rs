//! Count actual SQL reads during reference preparation, independently of elapsed time.
use super::*;
use std::sync::{Arc, Mutex};
use tracing::{field::Visit, Event, Subscriber};
use tracing_subscriber::{layer::Context, prelude::*, Layer};

#[derive(Clone, Default)]
struct Reads(Arc<Mutex<Vec<String>>>);

struct Statement(String);
impl Visit for Statement {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "db.statement" {
            self.0 = format!("{value:?}");
        }
    }
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "db.statement" {
            value.clone_into(&mut self.0);
        }
    }
}
impl<S: Subscriber> Layer<S> for Reads {
    fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
        if event.metadata().target() == "sqlx::query" {
            let mut statement = Statement(String::new());
            event.record(&mut statement);
            self.0.lock().unwrap().push(statement.0);
        }
    }
}

#[tokio::test]
async fn row294_measure_classic_repeated_smart_collection_reads() {
    let reads = Reads::default();
    // SQLx workers run on other threads; nextest isolates this test process.
    tracing::subscriber::set_global_default(tracing_subscriber::registry().with(reads.clone()))
        .unwrap();
    let pool = common::test_db().await;
    seed_scheduling_fixture(&pool).await;
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 901).await;
    repos
        .program_schedule
        .upsert(&ProgramSchedule {
            id: 301,
            name: "repeated source".into(),
            ..ProgramSchedule::default()
        })
        .await
        .unwrap();
    for id in 401..=403 {
        repos
            .program_schedule
            .upsert_item(&ProgramScheduleItem {
                id,
                program_schedule_id: 301,
                collection_type: CollectionType::SmartCollection,
                smart_collection_id: Some(2),
                playback_order: PlaybackOrder::Chronological,
                playout_mode: PlayoutMode::One,
                sort_order: i32::try_from(id).unwrap(),
                ..ProgramScheduleItem::default()
            })
            .await
            .unwrap();
    }
    pool.execute(r#"INSERT INTO "Playout" ("Id", "ChannelId", "ProgramScheduleId", "ScheduleKind", "Seed") VALUES (901,901,301,'Classic',123)"#).await.unwrap();
    reads.0.lock().unwrap().clear();
    let reference = repos
        .scheduling
        .classic_reference_data(
            901,
            Utc.with_ymd_and_hms(2026, 11, 16, 8, 0, 0).unwrap(),
            Duration::zero(),
        )
        .await
        .unwrap();
    {
        let queries = reads.0.lock().unwrap();
        let smart = queries
            .iter()
            .filter(|s| s.contains("FROM \"SmartCollection\" WHERE \"Id\" = ?"))
            .count();
        let anchors = queries
            .iter()
            .filter(|s| s.contains("FROM \"PlayoutProgramScheduleAnchor\" AS"))
            .count();
        let groups = queries
            .iter()
            .filter(|s| s.contains("FROM \"PlayoutScheduleItemFillGroupIndex\" AS"))
            .count();
        println!(
            "row294 baseline: smart={smart}, anchors={anchors}, groups={groups}, total_sql={}",
            queries.len()
        );
        assert_eq!(reference.media_by_key.len(), 1, "source identity is shared");
        assert!(reference
            .media_by_key
            .values()
            .all(|items| !items.is_empty()));
        assert_eq!((anchors, groups), (1, 1), "state is already bulk loaded");
        assert_eq!(
            smart, 1,
            "one smart query per build, independent of duplicate sources"
        );
    }
    assert_reloads_membership(&repos, &pool, &reads).await;
}

impl Reads {
    fn reset(&self) {
        self.0.lock().unwrap().clear();
    }
    fn count(&self, text: &str) -> usize {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.contains(text))
            .count()
    }
    fn smart(&self) -> usize {
        self.count("FROM \"SmartCollection\" WHERE \"Id\" = ?")
    }
}

#[tokio::test]
async fn row294_measure_block_and_playlist_smart_loads() {
    let reads = Reads::default();
    tracing::subscriber::set_global_default(tracing_subscriber::registry().with(reads.clone()))
        .unwrap();
    let pool = common::test_db().await;
    seed_scheduling_fixture(&pool).await;
    let repos = make_repositories(pool.clone());
    let template = block_preview_items()
        .into_iter()
        .find(|i| i.collection_key.smart_collection_id == Some(2))
        .unwrap();
    let items = (1..=3)
        .map(|id| EffectiveBlockItem {
            id,
            content: format!("same:{id}"),
            ..template.clone()
        })
        .collect::<Vec<_>>();
    reads.reset();
    let preview = repos
        .scheduling
        .load_block_preview_content(&items, &BTreeMap::new())
        .await
        .unwrap();
    println!(
        "row294 preview smart={}, keys={}",
        reads.smart(),
        preview.media_by_key.len()
    );
    assert_eq!(reads.smart(), 1);
    assert_eq!(preview.media_by_key.len(), 3);
    common::seed_profile_and_channel(&repos, 900).await;
    seed_block_scheduling_fixture(&pool).await;
    pool.execute(r#"UPDATE "BlockItem" SET "CollectionType"='SmartCollection', "CollectionId"=NULL, "SmartCollectionId"=2"#).await.unwrap();
    reads.reset();
    let block = repos
        .scheduling
        .block_reference_data(
            900,
            Utc.with_ymd_and_hms(2026, 4, 15, 8, 0, 0).unwrap(),
            1,
            Duration::zero(),
        )
        .await
        .unwrap();
    println!(
        "row294 block smart={}, keys={}",
        reads.smart(),
        block.media_by_key.len()
    );
    assert_eq!(reads.smart(), 1);
    reads.reset();
    let source = SchedulingContentSource::Playlist {
        group: Some("prime".into()),
        name: "nightly".into(),
    };
    let flat = repos
        .scheduling
        .media_items_for_source(&source)
        .await
        .unwrap();
    let children = repos
        .scheduling
        .playlist_items_for_source(&source)
        .await
        .unwrap()
        .unwrap();
    println!(
        "row294 playlist smart={}, flat={:?}, children={:?}",
        reads.smart(),
        flat.iter().map(|i| i.id).collect::<Vec<_>>(),
        children
            .iter()
            .map(|c| c.media_items.iter().map(|m| m.id).collect::<Vec<_>>())
            .collect::<Vec<_>>()
    );
    assert_eq!(reads.smart(), 2);
    assert_eq!(children.len(), 2);
}

#[tokio::test]
async fn row294_measure_artist_expansion_order_and_queries() {
    let reads = Reads::default();
    tracing::subscriber::set_global_default(tracing_subscriber::registry().with(reads.clone()))
        .unwrap();
    let pool = common::test_db().await;
    seed_scheduling_fixture(&pool).await;
    seed_search_enrichment(&pool).await;
    common::seed_media_item(&pool, 32, 1).await;
    pool.execute(r#"INSERT INTO "Artist" ("Id") VALUES (32);
        INSERT INTO "ArtistMetadata" ("Id","ArtistId","MetadataKind","Title","SortTitle","DateAdded","DateUpdated") VALUES (320,32,0,'Other Artist','other','2026-04-15','2026-04-15');"#).await.unwrap();
    common::seed_media_item(&pool, 33, 1).await;
    pool.execute(r#"INSERT INTO "MusicVideo" ("Id","ArtistId") VALUES (33,32);
        INSERT INTO "MusicVideoMetadata" ("Id","MusicVideoId","MetadataKind","Title","SortTitle","DateAdded","DateUpdated") VALUES (330,33,0,'A song','aaa','2026-04-15','2026-04-15')"#).await.unwrap();
    insert_version(&pool, 33, 600_000, "MusicVideoId").await;
    let repos = make_repositories(pool);
    reads.reset();
    let items = repos
        .scheduling
        .media_items_for_source(&SchedulingContentSource::Search(
            "type:artist OR type:music_video".into(),
        ))
        .await
        .unwrap();
    let count = reads.count("WHERE \"mv\".\"ArtistId\" = ?");
    println!(
        "row294 artist membership queries={count}, IDs={:?}",
        items.iter().map(|i| i.id).collect::<Vec<_>>()
    );
    assert_eq!(count, 0);
    assert_eq!(
        reads.count("WHERE \"ArtistId\" IN (SELECT \"value\" FROM json_each(?))"),
        1
    );
    assert_eq!(items.iter().map(|i| i.id).collect::<Vec<_>>(), vec![33, 31]);
}

async fn assert_reloads_membership(repos: &Repositories, pool: &SqlitePool, reads: &Reads) {
    pool.execute(r#"UPDATE "SmartCollection" SET "Query"='title = Feature' WHERE "Id"=2"#)
        .await
        .unwrap();
    reads.reset();
    let next = repos
        .scheduling
        .classic_reference_data(
            901,
            Utc.with_ymd_and_hms(2026, 11, 16, 8, 0, 0).unwrap(),
            Duration::zero(),
        )
        .await
        .unwrap();
    assert_eq!(
        reads.smart(),
        1,
        "the same long-lived repository starts a fresh build cache"
    );
    assert_eq!(
        next.media_by_key
            .values()
            .next()
            .unwrap()
            .iter()
            .map(|m| m.id)
            .collect::<Vec<_>>(),
        vec![20]
    );
    pool.execute(r#"UPDATE "ProgramScheduleItem" SET "CollectionType"='Playlist', "SmartCollectionId"=NULL, "PlaylistId"=4"#).await.unwrap();
    reads.reset();
    let playlist = repos
        .scheduling
        .classic_reference_data(
            901,
            Utc.with_ymd_and_hms(2026, 11, 16, 8, 0, 0).unwrap(),
            Duration::zero(),
        )
        .await
        .unwrap();
    assert_eq!(
        reads.smart(),
        1,
        "flat and structured playlist children reuse source material within one build"
    );
    assert_eq!(
        playlist
            .playlist_items_by_key
            .values()
            .next()
            .unwrap()
            .iter()
            .map(|c| c.media_items.iter().map(|m| m.id).collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        vec![vec![12, 13, 20], vec![20]]
    );
}

#[tokio::test]
async fn row294_shared_smart_material_keeps_playlist_choices_independent() {
    let pool = common::test_db().await;
    seed_scheduling_fixture(&pool).await;
    pool.execute(r#"UPDATE "PlaylistItem" SET "PreferredAudioLanguageCode"='jpn', "PreferredSubtitleLanguageCode"='eng', "SubtitleMode"=2 WHERE "Id"=402;
        INSERT INTO "PlaylistItem" ("Id","Index","PlaylistId","CollectionType","SmartCollectionId","PlaybackOrder","PlayAll","PreferredAudioLanguageCode","PreferredSubtitleLanguageCode","SubtitleMode") VALUES (403,2,4,'SmartCollection',2,'Chronological',1,'nld','nld',3)"#).await.unwrap();
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 901).await;
    repos
        .program_schedule
        .upsert(&ProgramSchedule {
            id: 301,
            name: "choices".into(),
            ..ProgramSchedule::default()
        })
        .await
        .unwrap();
    repos
        .program_schedule
        .upsert_item(&ProgramScheduleItem {
            id: 401,
            program_schedule_id: 301,
            collection_type: CollectionType::Playlist,
            playlist_id: Some(4),
            ..ProgramScheduleItem::default()
        })
        .await
        .unwrap();
    pool.execute(r#"INSERT INTO "Playout" ("Id","ChannelId","ProgramScheduleId","ScheduleKind","Seed") VALUES (901,901,301,'Classic',123)"#).await.unwrap();
    let data = repos
        .scheduling
        .classic_reference_data(
            901,
            Utc.with_ymd_and_hms(2026, 11, 16, 8, 0, 0).unwrap(),
            Duration::zero(),
        )
        .await
        .unwrap();
    let children = data.playlist_items_by_key.values().next().unwrap();
    let first = &children[1].media_items[0];
    let second = &children[2].media_items[0];
    assert_eq!((first.id, second.id), (21, 21));
    assert_eq!(first.preferred_audio_language_code.as_deref(), Some("jpn"));
    assert_eq!(second.preferred_audio_language_code.as_deref(), Some("nld"));
    assert_ne!(first.subtitle_mode, second.subtitle_mode);
}
