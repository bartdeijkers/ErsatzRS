#![cfg(all(feature = "mysql", feature = "mysql-integration"))]

use std::collections::BTreeSet;
use std::panic::AssertUnwindSafe;

use chrono::{DateTime, Duration, TimeZone, Utc};
use ersatzrs_core::locking::EntityLocker;
use ersatzrs_core::repositories::{
    ChannelRepository, ConfigRepository, FfmpegProfileRepository, PlayoutRepository,
    ProgramScheduleRepository,
};
use ersatzrs_core::scheduling::{PlayoutBuildMode, PlayoutBuildRequest, SchedulingResult};
use ersatzrs_domain::{
    AddonMediaListDetailPolicy, AddonMediaListImportState, CollectionEnumeratorState,
    CollectionType, EntityId, PlaybackOrder, Playout, PlayoutClassicCheckpoint, PlayoutMode,
    PlayoutProgramScheduleAnchor, PlayoutScheduleItemFillGroupIndex, ProgramSchedule,
    ProgramScheduleItem, ScanPlayoutRefreshReason,
};
use ersatzrs_infra::addon_media_list_import::AddonMediaListImportCoordinator;
use ersatzrs_infra::db;
use ersatzrs_infra::maintenance::run_requested_playout_build;
use ersatzrs_infra::repositories::{make_repositories, Repositories};
use ersatzrs_infra::scan_refresh::{self, DurableScanPlayoutRefresh};
use futures_util::FutureExt as _;
use sqlx::{ConnectOptions as _, Executor as _, Row};
use tokio::sync::Barrier;

static MYSQL_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
async fn mysql_artwork_duplicate_repair_preserves_variants_and_actor_references() {
    #[allow(clippy::disallowed_methods)]
    let Ok(database_url) = std::env::var("ERSATZRS_MYSQL_TEST_URL") else {
        eprintln!("skipping MySQL artwork repair test: ERSATZRS_MYSQL_TEST_URL is not set");
        return;
    };
    let _test_guard = MYSQL_TEST_LOCK.lock().await;
    let pool = db::init(&database_url)
        .await
        .expect("initialize MySQL artwork repair database");

    const BASE: i64 = 980_000;
    sqlx::query(r#"DELETE FROM `Actor` WHERE `Id` = ?"#)
        .bind(BASE)
        .execute(&pool)
        .await
        .expect("clear prior actor fixture");
    sqlx::query(r#"DELETE FROM `AddonMediaList` WHERE `Id` = ?"#)
        .bind(BASE)
        .execute(&pool)
        .await
        .expect("clear prior list fixture");
    sqlx::query(r#"DELETE FROM `Show` WHERE `Id` = ?"#)
        .bind(BASE)
        .execute(&pool)
        .await
        .expect("clear prior show fixture");
    sqlx::query(r#"DELETE FROM `MediaSource` WHERE `Id` = ?"#)
        .bind(BASE)
        .execute(&pool)
        .await
        .expect("clear prior media source fixture");
    sqlx::query(
        r#"DELETE FROM `AddonInstallation` WHERE `AddonId` = 'org.example.artwork-repair'"#,
    )
    .execute(&pool)
    .await
    .expect("clear prior add-on fixture");

    sqlx::query(
        r#"INSERT INTO `AddonInstallation` (
               `AddonId`, `Version`, `Enabled`, `PackageSha256`, `ContentSha256`,
               `ManifestJson`, `InstallPath`, `InstalledAt`, `UpdatedAt`
           ) VALUES (
               'org.example.artwork-repair', '1.0.0', 1, 'package', 'content',
               '{}', 'addons/artwork-repair', '2026-08-24T00:00:00Z', '2026-08-24T00:00:00Z'
           )"#,
    )
    .execute(&pool)
    .await
    .expect("seed repair add-on");
    sqlx::query(
        r#"INSERT INTO `AddonMediaList` (
               `Id`, `AddonId`, `ProviderId`, `SourceUrl`, `Name`, `ItemCount`,
               `ProviderMetadataJson`, `MetadataOverrideJson`
           ) VALUES (?, 'org.example.artwork-repair', 'fixture',
                     'https://example.invalid/fixture', 'Artwork repair', 0, '{}', '{}')"#,
    )
    .bind(BASE)
    .execute(&pool)
    .await
    .expect("seed repair list");
    sqlx::query(r#"INSERT INTO `MediaSource` (`Id`) VALUES (?)"#)
        .bind(BASE)
        .execute(&pool)
        .await
        .expect("seed repair media source");
    sqlx::query(r#"INSERT INTO `LocalMediaSource` (`Id`) VALUES (?)"#)
        .bind(BASE)
        .execute(&pool)
        .await
        .expect("seed repair local media source");
    sqlx::query(
        r#"INSERT INTO `Library` (`Id`, `Name`, `MediaKind`, `MediaSourceId`)
           VALUES (?, 'Artwork repair', 2, ?)"#,
    )
    .bind(BASE)
    .bind(BASE)
    .execute(&pool)
    .await
    .expect("seed repair library");
    sqlx::query(r#"INSERT INTO `LocalLibrary` (`Id`) VALUES (?)"#)
        .bind(BASE)
        .execute(&pool)
        .await
        .expect("seed repair local library");
    sqlx::query(
        r#"INSERT INTO `LibraryPath` (`Id`, `LibraryId`, `Path`)
           VALUES (?, ?, '/tmp/artwork-repair')"#,
    )
    .bind(BASE)
    .bind(BASE)
    .execute(&pool)
    .await
    .expect("seed repair library path");
    sqlx::query(r#"INSERT INTO `MediaItem` (`Id`, `LibraryPathId`, `State`) VALUES (?, ?, 0)"#)
        .bind(BASE)
        .bind(BASE)
        .execute(&pool)
        .await
        .expect("seed repair media item");
    sqlx::query(r#"INSERT INTO `Show` (`Id`) VALUES (?)"#)
        .bind(BASE)
        .execute(&pool)
        .await
        .expect("seed repair show");
    sqlx::query(
        r#"INSERT INTO `ShowMetadata` (
               `Id`, `DateAdded`, `DateUpdated`, `MetadataKind`, `ShowId`, `Title`
           ) VALUES (?, 'old', 'old', 0, ?, 'Artwork repair')"#,
    )
    .bind(BASE)
    .bind(BASE)
    .execute(&pool)
    .await
    .expect("seed repair show metadata");
    sqlx::query(
        r#"INSERT INTO `AddonMediaListTelevisionShow` (`AddonMediaListId`, `ShowId`)
           VALUES (?, ?)"#,
    )
    .bind(BASE)
    .bind(BASE)
    .execute(&pool)
    .await
    .expect("seed repair projection");

    for (offset, path) in [
        (1_i64, "/media/thumb.jpg"),
        (2, "/media/thumb.jpg"),
        (3, "/media/Thumb.jpg"),
        (4, "/media/actor.jpg"),
        (5, "/media/actor.jpg"),
    ] {
        sqlx::query(
            r#"INSERT INTO `Artwork` (
                   `Id`, `ArtworkKind`, `DateAdded`, `DateUpdated`, `ShowMetadataId`, `Path`
               ) VALUES (?, 0, 'old', 'new', ?, ?)"#,
        )
        .bind(BASE + offset)
        .bind(BASE)
        .bind(path)
        .execute(&pool)
        .await
        .expect("seed repair artwork");
    }
    sqlx::query(r#"INSERT INTO `Actor` (`Id`, `ArtworkId`, `Name`) VALUES (?, ?, 'Keeper')"#)
        .bind(BASE)
        .bind(BASE + 5)
        .execute(&pool)
        .await
        .expect("seed actor-referenced artwork");

    sqlx::raw_sql(include_str!(
        "../../../migrations/mysql/202608241400_artwork_cardinality_repair.sql"
    ))
    .execute(&pool)
    .await
    .expect("apply MySQL artwork repair");

    let ids = sqlx::query_scalar::<_, i64>(
        r#"SELECT `Id` FROM `Artwork`
           WHERE `ShowMetadataId` = ? ORDER BY `Id`"#,
    )
    .bind(BASE)
    .fetch_all(&pool)
    .await
    .expect("load repaired MySQL artwork");
    assert_eq!(ids, [BASE + 1, BASE + 3, BASE + 4, BASE + 5]);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(r#"SELECT `ArtworkId` FROM `Actor` WHERE `Id` = ?"#)
            .bind(BASE)
            .fetch_one(&pool)
            .await
            .expect("load retained MySQL actor reference"),
        BASE + 5
    );

    sqlx::query(r#"DELETE FROM `Actor` WHERE `Id` = ?"#)
        .bind(BASE)
        .execute(&pool)
        .await
        .expect("remove repair actor fixture");
    sqlx::query(r#"DELETE FROM `AddonMediaList` WHERE `Id` = ?"#)
        .bind(BASE)
        .execute(&pool)
        .await
        .expect("remove repair list fixture");
    sqlx::query(r#"DELETE FROM `Show` WHERE `Id` = ?"#)
        .bind(BASE)
        .execute(&pool)
        .await
        .expect("remove repair show fixture");
    sqlx::query(r#"DELETE FROM `MediaSource` WHERE `Id` = ?"#)
        .bind(BASE)
        .execute(&pool)
        .await
        .expect("remove repair media source fixture");
    sqlx::query(
        r#"DELETE FROM `AddonInstallation` WHERE `AddonId` = 'org.example.artwork-repair'"#,
    )
    .execute(&pool)
    .await
    .expect("remove repair add-on fixture");
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn mysql_backend_runs_migrations_and_representative_queries() {
    #[allow(clippy::disallowed_methods)]
    let Ok(database_url) = std::env::var("ERSATZRS_MYSQL_TEST_URL") else {
        eprintln!("skipping MySQL integration test: ERSATZRS_MYSQL_TEST_URL is not set");
        return;
    };
    let _test_guard = MYSQL_TEST_LOCK.lock().await;

    let pool = db::init(&database_url)
        .await
        .expect("initialize MySQL database");
    let repos = make_repositories(pool.clone());

    let video_tune_count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM INFORMATION_SCHEMA.COLUMNS
           WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = 'FFmpegProfile'
             AND COLUMN_NAME = 'VideoTune'"#,
    )
    .fetch_one(&pool)
    .await
    .expect("query VideoTune column");
    assert_eq!(video_tune_count, 1, "missing FFmpegProfile.VideoTune");

    for table in [
        "EpisodeMetadata",
        "MusicVideoMetadata",
        "SongMetadata",
        "ImageMetadata",
    ] {
        let count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM INFORMATION_SCHEMA.COLUMNS
               WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ?
                 AND COLUMN_NAME = 'ContentRating'"#,
        )
        .bind(table)
        .fetch_one(&pool)
        .await
        .expect("query uniform content-rating column");
        assert_eq!(count, 1, "missing ContentRating on {table}");
    }
    for table in ["MusicLabel", "MetadataLanguage"] {
        let table_count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM INFORMATION_SCHEMA.TABLES
               WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ?"#,
        )
        .bind(table)
        .fetch_one(&pool)
        .await
        .expect("query uniform facet table");
        assert_eq!(table_count, 1, "missing {table}");

        let index_count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(DISTINCT INDEX_NAME) FROM INFORMATION_SCHEMA.STATISTICS
               WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ?
                 AND INDEX_NAME LIKE 'IX_%'"#,
        )
        .bind(table)
        .fetch_one(&pool)
        .await
        .expect("query uniform facet indexes");
        assert_eq!(index_count, 10, "unexpected owner-index count for {table}");
    }

    let key = format!("mysql.integration.{}", uuid::Uuid::new_v4());
    repos
        .config
        .set(&key, "ok")
        .await
        .expect("write config value");
    assert_eq!(
        repos.config.get(&key).await.expect("read config value"),
        Some("ok".to_string())
    );

    sqlx::query(
        r#"REPLACE INTO "Resolution" ("Id", "Name", "Width", "Height")
           VALUES (7001, 'mysql-test', 1920, 1080)"#,
    )
    .execute(&pool)
    .await
    .expect("seed resolution");

    sqlx::query(
        r#"REPLACE INTO "FFmpegProfile" (
               "Id", "Name", "ThreadCount", "TranscodeVideo", "TranscodeAudio",
               "ResolutionId", "ScalingBehavior", "VideoFormat", "AudioFormat",
               "VideoBitrate", "VideoBufferSize", "AudioBitrate", "AudioBufferSize",
               "NormalizeLoudnessMode", "AudioChannels", "AudioSampleRate",
               "HardwareAcceleration", "QsvExtraHardwareFrames", "VaapiDriver",
               "TonemapAlgorithm", "AllowBFrames", "BitDepth", "NormalizeFramerate",
               "NormalizeColors", "DeinterlaceVideo", "PadMode", "VideoPreset",
               "VideoTune"
           ) VALUES (
               7001, 'mysql-test', 0, 1, 1,
               7001, 0, 1, 1,
               4000, 8000, 192, 384,
               0, 2, 48000,
               0, 0, 0,
               0, 0, 0, 0,
               1, 0, 0, 'p7', 'll'
           )"#,
    )
    .execute(&pool)
    .await
    .expect("seed ffmpeg profile");

    sqlx::query(
        r#"REPLACE INTO "Channel" (
               "Id", "UniqueId", "Number", "SortNumber", "Name", "Group",
               "FFmpegProfileId", "StreamingMode", "StreamSelectorMode",
               "SubtitleMode", "MusicVideoCreditsMode", "SongVideoMode",
               "PlayoutSource", "PlayoutMode", "TranscodeMode", "IdleBehavior",
               "IsEnabled", "ShowInEpg"
           ) VALUES (
               7001, '00000000-0000-0000-0000-000000007001', '7001', 7001.0,
               'mysql-test', 'ErsatzTV', 7001, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1
           )"#,
    )
    .execute(&pool)
    .await
    .expect("seed channel");

    let channel = repos
        .channel
        .get_by_number("7001")
        .await
        .expect("read channel by number");
    assert_eq!(channel.name, "mysql-test");

    let profile = repos
        .ffmpeg_profile
        .get(7001)
        .await
        .expect("read ffmpeg profile");
    assert_eq!(profile.name, "mysql-test");
    assert_eq!(profile.video_preset.as_deref(), Some("p7"));
    assert_eq!(profile.video_tune.as_deref(), Some("ll"));

    sqlx::query(r#"REPLACE INTO "MediaSource" ("Id") VALUES (7001)"#)
        .execute(&pool)
        .await
        .expect("seed media source");
    sqlx::query(r#"REPLACE INTO "LocalMediaSource" ("Id") VALUES (7001)"#)
        .execute(&pool)
        .await
        .expect("seed local media source");
    sqlx::query(
        r#"REPLACE INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
           VALUES (7001, 'mysql-test', 0, 7001)"#,
    )
    .execute(&pool)
    .await
    .expect("seed library");
    sqlx::query(r#"REPLACE INTO "LocalLibrary" ("Id") VALUES (7001)"#)
        .execute(&pool)
        .await
        .expect("seed local library");
    sqlx::query(
        r#"REPLACE INTO "LibraryPath" ("Id", "LibraryId", "Path")
           VALUES (7001, 7001, '/tmp')"#,
    )
    .execute(&pool)
    .await
    .expect("seed library path");

    let inserted =
        sqlx::query(r#"INSERT INTO "MediaItem" ("LibraryPathId", "State") VALUES (7001, 0)"#)
            .execute(&pool)
            .await
            .expect("insert media item");
    assert!(db::last_insert_id(inserted) > 0);

    let playout_count: i64 =
        sqlx::query(r#"SELECT COUNT(*) AS "Count" FROM "Channel" WHERE "Number" = ?"#)
            .bind("7001")
            .fetch_one(&pool)
            .await
            .expect("run bound MySQL query")
            .try_get("Count")
            .expect("decode count");
    assert_eq!(playout_count, 1);

    sqlx::query(
        r#"REPLACE INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder")
           VALUES (7101, 'mysql-runtime-state', 0)"#,
    )
    .execute(&pool)
    .await
    .expect("seed runtime-state collection");
    for (schedule_id, item_id) in [(7101, 7101), (7102, 7102)] {
        repos
            .program_schedule
            .upsert(&ProgramSchedule {
                id: schedule_id,
                name: format!("mysql-runtime-state-{schedule_id}"),
                item_ids: vec![item_id],
                ..ProgramSchedule::default()
            })
            .await
            .expect("seed runtime-state schedule");
    }
    for (playout_id, schedule_id) in [(7101, 7101), (7102, 7102)] {
        sqlx::query(
            r#"REPLACE INTO "Playout" (
                   "Id", "ChannelId", "ProgramScheduleId", "ScheduleKind", "Seed"
               ) VALUES (?, 7001, ?, 1, 0)"#,
        )
        .bind(playout_id)
        .bind(schedule_id)
        .execute(&pool)
        .await
        .expect("seed runtime-state playout");
        repos
            .playout
            .replace_runtime_state(
                &Playout {
                    id: playout_id,
                    channel_id: 7001,
                    program_schedule_id: Some(schedule_id),
                    ..Playout::default()
                },
                &[],
                &[],
                &[PlayoutProgramScheduleAnchor {
                    id: 0,
                    playout_id,
                    collection_type: CollectionType::Collection,
                    collection_id: Some(7101),
                    enumerator_state: CollectionEnumeratorState { seed: 1, index: 2 },
                    ..PlayoutProgramScheduleAnchor::default()
                }],
                &[PlayoutScheduleItemFillGroupIndex {
                    id: 0,
                    playout_id,
                    program_schedule_item_id: schedule_id,
                    enumerator_state: CollectionEnumeratorState { seed: 3, index: 4 },
                }],
            )
            .await
            .expect("persist database-assigned runtime-state identities");
    }

    let anchor_ids = sqlx::query(
        r#"SELECT "Id" FROM "PlayoutProgramScheduleAnchor"
           WHERE "PlayoutId" IN (7101, 7102) ORDER BY "PlayoutId""#,
    )
    .fetch_all(&pool)
    .await
    .expect("load generated anchor identities")
    .into_iter()
    .map(|row| row.try_get::<i64, _>("Id").expect("decode anchor identity"))
    .collect::<Vec<_>>();
    let fill_group_ids = sqlx::query(
        r#"SELECT "Id" FROM "PlayoutScheduleItemFillGroupIndex"
           WHERE "PlayoutId" IN (7101, 7102) ORDER BY "PlayoutId""#,
    )
    .fetch_all(&pool)
    .await
    .expect("load generated fill-group identities")
    .into_iter()
    .map(|row| {
        row.try_get::<i64, _>("Id")
            .expect("decode fill-group identity")
    })
    .collect::<Vec<_>>();
    assert_eq!(anchor_ids.len(), 2);
    assert_ne!(anchor_ids[0], anchor_ids[1]);
    assert_eq!(fill_group_ids.len(), 2);
    assert_ne!(fill_group_ids[0], fill_group_ids[1]);

    let refresh_reason = ScanPlayoutRefreshReason::RemoteStreamMembershipAdded;
    let resolution_generation =
        ersatzrs_infra::scan_refresh::begin_resolution(&pool, 7001, refresh_reason)
            .await
            .expect("begin MySQL scan-impact resolution");
    let mut refresh_tx = db::begin_write(&pool)
        .await
        .expect("begin MySQL refresh-intent transaction");
    ersatzrs_infra::scan_refresh::persist_refresh_intents(
        &mut refresh_tx,
        7001,
        refresh_reason,
        &[7101],
    )
    .await
    .expect("persist MySQL refresh intent");
    ersatzrs_infra::scan_refresh::complete_resolution_in_tx(
        &mut refresh_tx,
        7001,
        resolution_generation,
        refresh_reason,
    )
    .await
    .expect("complete MySQL scan-impact resolution");
    refresh_tx
        .commit()
        .await
        .expect("commit MySQL refresh-intent transaction");
    assert_eq!(
        ersatzrs_infra::scan_refresh::recoverable_refreshes(&pool)
            .await
            .expect("recover MySQL refresh intent")
            .iter()
            .filter(|refresh| refresh.playout_id == 7101)
            .count(),
        1
    );
    let refresh_generation = ersatzrs_infra::scan_refresh::mark_refresh_building(&pool, 7101)
        .await
        .expect("mark MySQL refresh building")
        .expect("MySQL refresh generation");
    ersatzrs_infra::scan_refresh::complete_refresh(&pool, 7101, refresh_generation)
        .await
        .expect("complete MySQL refresh intent");

    let checkpoint_date = chrono::Utc::now();
    let checkpoint_playout = Playout {
        id: 7101,
        channel_id: 7001,
        program_schedule_id: Some(7101),
        ..Playout::default()
    };
    for context in ["{\"Version\":1}", "{\"Version\":1,\"Retry\":true}"] {
        repos
            .playout
            .apply_build_result(
                &SchedulingResult {
                    classic_checkpoints: vec![PlayoutClassicCheckpoint {
                        playout_id: 7101,
                        checkpoint_date,
                        context: context.to_owned(),
                    }],
                    ..SchedulingResult::default()
                },
                &checkpoint_playout,
                chrono::Duration::zero(),
            )
            .await
            .expect("replace MySQL Classic checkpoint state");
    }
    let checkpoint_rows =
        sqlx::query(r#"SELECT "Context" FROM "PlayoutClassicCheckpoint" WHERE "PlayoutId" = 7101"#)
            .fetch_all(&pool)
            .await
            .expect("load MySQL Classic checkpoints");
    assert_eq!(checkpoint_rows.len(), 1);
    assert_eq!(
        checkpoint_rows[0]
            .try_get::<String, _>("Context")
            .expect("decode checkpoint context"),
        "{\"Version\":1,\"Retry\":true}"
    );
}

type MysqlIntegrationResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync + 'static>>;

#[derive(Clone, Copy)]
#[allow(clippy::struct_field_names)]
struct MysqlClassicRefreshFixture {
    library_id: EntityId,
    added_media_id: EntityId,
    related_playlist_id: EntityId,
    target_playout_id: EntityId,
    oracle_playout_id: EntityId,
    unrelated_playout_id: EntityId,
}

impl MysqlClassicRefreshFixture {
    fn playout_ids(self) -> [EntityId; 3] {
        [
            self.target_playout_id,
            self.oracle_playout_id,
            self.unrelated_playout_id,
        ]
    }

    fn contains_playout(self, playout_id: EntityId) -> bool {
        self.playout_ids().contains(&playout_id)
    }
}

/// `crf-02`: two independent `MySQL` connections serialize scan-refresh
/// generations and merge concurrent intent writes without losing state.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mysql_scan_refresh_concurrency_preserves_generations_and_newer_resolution() {
    #[allow(clippy::disallowed_methods)]
    let Ok(database_url) = std::env::var("ERSATZRS_MYSQL_TEST_URL") else {
        eprintln!(
            "skipping MySQL scan-refresh concurrency test: ERSATZRS_MYSQL_TEST_URL is not set"
        );
        return;
    };
    let _test_guard = MYSQL_TEST_LOCK.lock().await;
    let random = uuid::Uuid::new_v4().simple().to_string();
    let tag = format!("crf02-concurrency-{}", &random[..16]);

    let outcome = AssertUnwindSafe(run_mysql_scan_refresh_concurrency(&database_url, &tag))
        .catch_unwind()
        .await;
    let cleanup = cleanup_mysql_classic_refresh_fixture(&database_url, &tag).await;

    match (outcome, cleanup) {
        (Ok(Ok(())), Ok(())) => {}
        (Ok(Err(test_error)), Ok(())) => {
            panic!("MySQL scan-refresh concurrency test failed: {test_error}")
        }
        (Err(payload), Ok(())) => std::panic::resume_unwind(payload),
        (Ok(Ok(())), Err(cleanup_error)) => {
            panic!("MySQL scan-refresh concurrency fixture cleanup failed: {cleanup_error}")
        }
        (Ok(Err(test_error)), Err(cleanup_error)) => panic!(
            "MySQL scan-refresh concurrency test failed: {test_error}; cleanup also failed: {cleanup_error}"
        ),
        (Err(_), Err(cleanup_error)) => panic!(
            "MySQL scan-refresh concurrency test panicked; cleanup also failed: {cleanup_error}"
        ),
    }
}

#[allow(clippy::too_many_lines)]
async fn run_mysql_scan_refresh_concurrency(
    database_url: &str,
    tag: &str,
) -> MysqlIntegrationResult {
    let setup_pool = db::init(database_url).await?;
    let fixture = seed_mysql_classic_refresh_fixture(&setup_pool, tag).await?;
    setup_pool.close().await;

    let first_pool = mysql_single_connection_pool(database_url).await?;
    let second_pool = mysql_single_connection_pool(database_url).await?;
    let first_connection_id: i64 = sqlx::query_scalar("SELECT CAST(CONNECTION_ID() AS SIGNED)")
        .fetch_one(&first_pool)
        .await?;
    let second_connection_id: i64 = sqlx::query_scalar("SELECT CAST(CONNECTION_ID() AS SIGNED)")
        .fetch_one(&second_pool)
        .await?;
    assert_ne!(
        first_connection_id, second_connection_id,
        "the race must use two independent InnoDB connections"
    );

    let resolution_barrier = Barrier::new(2);
    let first_resolution = async {
        resolution_barrier.wait().await;
        scan_refresh::begin_resolution(
            &first_pool,
            fixture.library_id,
            ScanPlayoutRefreshReason::RemoteStreamMembershipAdded,
        )
        .await
    };
    let second_resolution = async {
        resolution_barrier.wait().await;
        scan_refresh::begin_resolution(
            &second_pool,
            fixture.library_id,
            ScanPlayoutRefreshReason::RemoteStreamMembershipReordered,
        )
        .await
    };
    let (first_generation, second_generation) = tokio::join!(first_resolution, second_resolution);
    let mut generations = vec![first_generation?, second_generation?];
    generations.sort_unstable();
    assert_eq!(
        generations,
        vec![1, 2],
        "concurrent resolution starts must allocate unique monotonic generations"
    );
    let persisted_resolution_generation: i64 = sqlx::query_scalar(
        r#"SELECT "Generation" FROM "ScanRefreshResolution" WHERE "LibraryId" = ?"#,
    )
    .bind(fixture.library_id)
    .fetch_one(&first_pool)
    .await?;
    assert_eq!(persisted_resolution_generation, 2);

    let intent_barrier = Barrier::new(2);
    let owned_intent = async {
        let mut tx = db::begin_write(&first_pool).await?;
        intent_barrier.wait().await;
        scan_refresh::persist_refresh_intents(
            &mut tx,
            fixture.library_id,
            ScanPlayoutRefreshReason::RemoteStreamMembershipAdded,
            &[fixture.target_playout_id],
        )
        .await?;
        tx.commit().await
    };
    let unowned_intent = async {
        let mut tx = db::begin_write(&second_pool).await?;
        intent_barrier.wait().await;
        scan_refresh::persist_unowned_refresh_intents(
            &mut tx,
            ScanPlayoutRefreshReason::RemoteStreamMembershipReordered,
            &[fixture.target_playout_id],
        )
        .await?;
        tx.commit().await
    };
    let (owned_result, unowned_result) = tokio::join!(owned_intent, unowned_intent);
    owned_result?;
    unowned_result?;

    let intent: (i64, Option<i64>, i64, String, Option<String>) = sqlx::query_as(
        r#"SELECT "Generation", "LibraryId", "State", "Reason", "Diagnostic"
           FROM "ScanPlayoutRefreshIntent" WHERE "PlayoutId" = ?"#,
    )
    .bind(fixture.target_playout_id)
    .fetch_one(&first_pool)
    .await?;
    assert_eq!(
        intent,
        (
            2,
            None,
            0,
            ScanPlayoutRefreshReason::RemoteStreamMembershipMixed
                .as_str()
                .to_owned(),
            None,
        ),
        "concurrent intent writes must retain both generations and conservatively merge owner and reason"
    );

    let interrupted_generation = generations[0];
    let newer_generation = generations[1];
    let mut completion_tx = db::begin_write(&second_pool).await?;
    scan_refresh::complete_resolution_in_tx(
        &mut completion_tx,
        fixture.library_id,
        newer_generation,
        ScanPlayoutRefreshReason::RemoteStreamMembershipReplaced,
    )
    .await?;
    completion_tx.commit().await?;

    let resolved_before: (i64, i64, String, Option<String>, String, String) = sqlx::query_as(
        r#"SELECT "Generation", "State", "Reason", "Diagnostic", "StartedAt", "UpdatedAt"
           FROM "ScanRefreshResolution" WHERE "LibraryId" = ?"#,
    )
    .bind(fixture.library_id)
    .fetch_one(&first_pool)
    .await?;
    assert_eq!(resolved_before.0, newer_generation);
    assert_eq!(resolved_before.1, 1);
    assert_eq!(
        resolved_before.2,
        ScanPlayoutRefreshReason::RemoteStreamMembershipReplaced.as_str()
    );
    assert_eq!(resolved_before.3, None);

    scan_refresh::fail_interrupted_replay(&first_pool, fixture.library_id, interrupted_generation)
        .await?;
    let resolved_after: (i64, i64, String, Option<String>, String, String) = sqlx::query_as(
        r#"SELECT "Generation", "State", "Reason", "Diagnostic", "StartedAt", "UpdatedAt"
           FROM "ScanRefreshResolution" WHERE "LibraryId" = ?"#,
    )
    .bind(fixture.library_id)
    .fetch_one(&second_pool)
    .await?;
    assert_eq!(
        resolved_after, resolved_before,
        "a stale replay failure must not alter newer resolved work"
    );

    first_pool.close().await;
    second_pool.close().await;
    Ok(())
}

async fn mysql_single_connection_pool(database_url: &str) -> Result<db::DbPool, sqlx::Error> {
    sqlx::any::install_default_drivers();
    let options = database_url
        .parse::<sqlx::any::AnyConnectOptions>()?
        .disable_statement_logging();
    sqlx::any::AnyPoolOptions::new()
        .max_connections(1)
        .after_connect(|connection, _metadata| {
            Box::pin(async move {
                connection
                    .execute("SET SESSION sql_mode = CONCAT_WS(',', @@sql_mode, 'ANSI_QUOTES')")
                    .await?;
                Ok(())
            })
        })
        .connect_with(options)
        .await
}

/// `crf-02`: `MySQL` preserves one exact managed-playlist Refresh across a host restart.
///
/// The target and oracle are equivalent consumers of the related playlist; the
/// third Classic playout consumes a disjoint playlist and is the isolation
/// control. The oracle processes the committed intent without interruption,
/// while the target pool is closed and reopened before startup recovery.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn mysql_recovers_scoped_classic_refresh_without_touching_unrelated_playout() {
    #[allow(clippy::disallowed_methods)]
    let Ok(database_url) = std::env::var("ERSATZRS_MYSQL_TEST_URL") else {
        eprintln!(
            "skipping MySQL Classic refresh continuity test: ERSATZRS_MYSQL_TEST_URL is not set"
        );
        return;
    };
    let _test_guard = MYSQL_TEST_LOCK.lock().await;
    let random = uuid::Uuid::new_v4().simple().to_string();
    let tag = format!("crf02-{}", &random[..16]);

    let outcome = AssertUnwindSafe(run_mysql_classic_refresh_continuity(&database_url, &tag))
        .catch_unwind()
        .await;
    let cleanup = cleanup_mysql_classic_refresh_fixture(&database_url, &tag).await;

    match (outcome, cleanup) {
        (Ok(Ok(())), Ok(())) => {}
        (Ok(Err(test_error)), Ok(())) => {
            panic!("MySQL Classic refresh continuity test failed: {test_error}")
        }
        (Err(payload), Ok(())) => std::panic::resume_unwind(payload),
        (Ok(Ok(())), Err(cleanup_error)) => {
            panic!("MySQL Classic refresh fixture cleanup failed: {cleanup_error}")
        }
        (Ok(Err(test_error)), Err(cleanup_error)) => panic!(
            "MySQL Classic refresh continuity test failed: {test_error}; cleanup also failed: {cleanup_error}"
        ),
        (Err(_), Err(cleanup_error)) => panic!(
            "MySQL Classic refresh continuity test panicked; cleanup also failed: {cleanup_error}"
        ),
    }
}

#[allow(clippy::too_many_lines)]
async fn run_mysql_classic_refresh_continuity(
    database_url: &str,
    tag: &str,
) -> MysqlIntegrationResult {
    let pool = db::init(database_url).await?;
    let fixture = seed_mysql_classic_refresh_fixture(&pool, tag).await?;
    let build_at = Utc
        .with_ymd_and_hms(2030, 1, 15, 10, 0, 0)
        .single()
        .expect("valid fixed MySQL build time");
    let refresh_at = build_at + Duration::minutes(1);
    let uninterrupted_locker = EntityLocker::new();

    for playout_id in fixture.playout_ids() {
        run_mysql_classic_build(
            &pool,
            &uninterrupted_locker,
            playout_id,
            PlayoutBuildMode::Continue,
            build_at,
        )
        .await?;
    }

    let unrelated_before =
        mysql_persisted_classic_rows(&pool, fixture.unrelated_playout_id, false).await?;
    let target_before =
        mysql_persisted_classic_rows(&pool, fixture.target_playout_id, true).await?;
    let oracle_before =
        mysql_persisted_classic_rows(&pool, fixture.oracle_playout_id, true).await?;
    assert!(!unrelated_before.playout_items.is_empty());
    assert!(!unrelated_before.playout_anchor.is_empty());
    assert!(!unrelated_before.classic_checkpoints.is_empty());
    assert_eq!(
        target_before, oracle_before,
        "the restart target and uninterrupted oracle must begin from equivalent fresh Classic state"
    );
    assert_eq!(
        mysql_playout_media_count(&pool, fixture.target_playout_id, fixture.added_media_id).await?,
        0
    );

    let affected = commit_mysql_scan_membership_change(&pool, fixture).await?;
    let mut expected_affected = vec![fixture.target_playout_id, fixture.oracle_playout_id];
    expected_affected.sort_unstable();
    assert_eq!(
        affected, expected_affected,
        "exact playlist dependency resolution must exclude the unrelated Classic playout"
    );
    assert_eq!(
        mysql_fixture_intended_playout_ids(&pool, fixture).await?,
        expected_affected,
        "the membership transaction must publish one intent per related playout"
    );
    assert_eq!(
        mysql_playout_intent_count(&pool, fixture.unrelated_playout_id).await?,
        0,
        "the unrelated Classic playout must never receive an intent"
    );

    run_mysql_classic_build(
        &pool,
        &uninterrupted_locker,
        fixture.oracle_playout_id,
        PlayoutBuildMode::Refresh,
        refresh_at,
    )
    .await?;
    let oracle_after = mysql_persisted_classic_rows(&pool, fixture.oracle_playout_id, true).await?;
    assert!(
        mysql_playout_media_count(&pool, fixture.oracle_playout_id, fixture.added_media_id).await?
            > 0,
        "the uninterrupted oracle must materialize the added playlist member"
    );
    assert_eq!(
        mysql_fixture_intended_playout_ids(&pool, fixture).await?,
        [fixture.target_playout_id],
        "the uninterrupted oracle must consume only its own intent"
    );
    assert_eq!(
        unrelated_before,
        mysql_persisted_classic_rows(&pool, fixture.unrelated_playout_id, false).await?,
        "the unrelated Classic state changed before the simulated restart"
    );

    pool.close().await;
    let reopened = db::init(database_url).await?;
    let recovered = scan_refresh::recoverable_refreshes(&reopened)
        .await?
        .into_iter()
        .filter(|refresh| fixture.contains_playout(refresh.playout_id))
        .collect::<Vec<_>>();
    assert_eq!(
        recovered,
        [DurableScanPlayoutRefresh {
            playout_id: fixture.target_playout_id,
            reason: ScanPlayoutRefreshReason::RemoteStreamMembershipAdded,
        }],
        "startup recovery must admit only the still-pending related target"
    );
    assert_eq!(
        mysql_scan_resolution_state(&reopened, fixture.library_id).await?,
        1
    );
    assert_eq!(
        mysql_playlist_membership_count(
            &reopened,
            fixture.related_playlist_id,
            fixture.added_media_id,
        )
        .await?,
        1,
        "the playlist membership and durable intent must survive the same commit boundary"
    );

    let restarted_locker = EntityLocker::new();
    for refresh in recovered {
        run_mysql_classic_build(
            &reopened,
            &restarted_locker,
            refresh.playout_id,
            PlayoutBuildMode::Refresh,
            refresh_at,
        )
        .await?;
    }

    let restarted_after =
        mysql_persisted_classic_rows(&reopened, fixture.target_playout_id, true).await?;
    assert_ne!(
        restarted_after, target_before,
        "the recovered Refresh must replace the target's fresh-build state"
    );
    assert_eq!(
        restarted_after, oracle_after,
        "the restarted target must match uninterrupted scheduling, anchors, runtime rows, and checkpoints"
    );
    assert!(
        mysql_playout_media_count(&reopened, fixture.target_playout_id, fixture.added_media_id,)
            .await?
            > 0,
        "the recovered Refresh must materialize the added playlist member"
    );
    assert_eq!(
        unrelated_before,
        mysql_persisted_classic_rows(&reopened, fixture.unrelated_playout_id, false).await?,
        "unrelated PlayoutItem, anchor, checkpoint, and runtime rows changed"
    );
    assert!(
        mysql_fixture_intended_playout_ids(&reopened, fixture)
            .await?
            .is_empty(),
        "both related intents must be consumed after successful Refresh builds"
    );
    assert_eq!(
        mysql_playout_intent_count(&reopened, fixture.unrelated_playout_id).await?,
        0,
        "the unrelated Classic playout must still have no intent"
    );
    reopened.close().await;
    Ok(())
}

#[allow(clippy::too_many_lines)]
async fn seed_mysql_classic_refresh_fixture(
    pool: &db::DbPool,
    tag: &str,
) -> MysqlIntegrationResult<MysqlClassicRefreshFixture> {
    let resolution = sqlx::query(
        r#"INSERT INTO "Resolution" ("Name", "IsCustom", "Width", "Height")
           VALUES (?, 0, 1920, 1080)"#,
    )
    .bind(format!("{tag}-resolution"))
    .execute(pool)
    .await?;
    let resolution_id = mysql_inserted_entity_id(resolution);

    let profile = sqlx::query(
        r#"INSERT INTO "FFmpegProfile" (
               "Name", "ThreadCount", "TranscodeVideo", "TranscodeAudio",
               "ResolutionId", "ScalingBehavior", "VideoFormat", "AudioFormat",
               "VideoBitrate", "VideoBufferSize", "AudioBitrate", "AudioBufferSize",
               "NormalizeLoudnessMode", "AudioChannels", "AudioSampleRate",
               "HardwareAcceleration", "QsvExtraHardwareFrames", "VaapiDriver",
               "TonemapAlgorithm", "AllowBFrames", "BitDepth", "NormalizeFramerate",
               "NormalizeColors", "DeinterlaceVideo", "PadMode", "VideoPreset",
               "VideoTune"
           ) VALUES (
               ?, 0, 1, 1, ?, 0, 1, 1, 4000, 8000, 192, 384, 0, 2, 48000,
               0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 'p7', 'll'
           )"#,
    )
    .bind(format!("{tag}-profile"))
    .bind(resolution_id)
    .execute(pool)
    .await?;
    let profile_id = mysql_inserted_entity_id(profile);

    let target_channel_id =
        insert_mysql_fixture_channel(pool, profile_id, tag, "target", 1.0).await?;
    let oracle_channel_id =
        insert_mysql_fixture_channel(pool, profile_id, tag, "oracle", 2.0).await?;
    let unrelated_channel_id =
        insert_mysql_fixture_channel(pool, profile_id, tag, "unrelated", 3.0).await?;

    let mut media_tx = db::begin_write(pool).await?;
    let media_source = sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (NULL)"#)
        .execute(&mut *media_tx)
        .await?;
    let media_source_id = mysql_inserted_entity_id(media_source);
    sqlx::query(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (?)"#)
        .bind(media_source_id)
        .execute(&mut *media_tx)
        .await?;
    let library = sqlx::query(
        r#"INSERT INTO "Library" ("Name", "MediaKind", "MediaSourceId")
           VALUES (?, 1, ?)"#,
    )
    .bind(format!("{tag}-library"))
    .bind(media_source_id)
    .execute(&mut *media_tx)
    .await?;
    let library_id = mysql_inserted_entity_id(library);
    sqlx::query(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (?)"#)
        .bind(library_id)
        .execute(&mut *media_tx)
        .await?;
    let library_path =
        sqlx::query(r#"INSERT INTO "LibraryPath" ("LibraryId", "Path") VALUES (?, ?)"#)
            .bind(library_id)
            .bind(format!("fixtures/{tag}"))
            .execute(&mut *media_tx)
            .await?;
    let library_path_id = mysql_inserted_entity_id(library_path);
    media_tx.commit().await?;

    let related_media_id =
        insert_mysql_fixture_movie(pool, library_path_id, &format!("{tag} Related One")).await?;
    let added_media_id =
        insert_mysql_fixture_movie(pool, library_path_id, &format!("{tag} Related Two")).await?;
    let unrelated_media_id =
        insert_mysql_fixture_movie(pool, library_path_id, &format!("{tag} Unrelated")).await?;

    let playlist_group =
        sqlx::query(r#"INSERT INTO "PlaylistGroup" ("Name", "IsSystem") VALUES (?, 1)"#)
            .bind(format!("{tag}-group"))
            .execute(pool)
            .await?;
    let playlist_group_id = mysql_inserted_entity_id(playlist_group);
    let related_playlist_id =
        insert_mysql_fixture_playlist(pool, playlist_group_id, "related", tag).await?;
    let unrelated_playlist_id =
        insert_mysql_fixture_playlist(pool, playlist_group_id, "unrelated", tag).await?;
    insert_mysql_playlist_item(pool, related_playlist_id, related_media_id, 0).await?;
    insert_mysql_playlist_item(pool, unrelated_playlist_id, unrelated_media_id, 0).await?;

    let repositories = make_repositories(pool.clone());
    let related_schedule_id = seed_mysql_classic_schedule(
        pool,
        &repositories,
        &format!("{tag}-schedule-related"),
        related_playlist_id,
    )
    .await?;
    let unrelated_schedule_id = seed_mysql_classic_schedule(
        pool,
        &repositories,
        &format!("{tag}-schedule-unrelated"),
        unrelated_playlist_id,
    )
    .await?;

    let target_playout_id =
        insert_mysql_classic_playout(pool, target_channel_id, related_schedule_id).await?;
    let oracle_playout_id =
        insert_mysql_classic_playout(pool, oracle_channel_id, related_schedule_id).await?;
    let unrelated_playout_id =
        insert_mysql_classic_playout(pool, unrelated_channel_id, unrelated_schedule_id).await?;

    Ok(MysqlClassicRefreshFixture {
        library_id,
        added_media_id,
        related_playlist_id,
        target_playout_id,
        oracle_playout_id,
        unrelated_playout_id,
    })
}

async fn insert_mysql_fixture_channel(
    pool: &db::DbPool,
    profile_id: EntityId,
    tag: &str,
    role: &str,
    sort_number: f64,
) -> MysqlIntegrationResult<EntityId> {
    let channel = sqlx::query(
        r#"INSERT INTO "Channel" (
               "UniqueId", "Number", "SortNumber", "Name", "Group",
               "FFmpegProfileId", "StreamingMode", "StreamSelectorMode",
               "SubtitleMode", "MusicVideoCreditsMode", "SongVideoMode",
               "PlayoutSource", "PlayoutMode", "TranscodeMode", "IdleBehavior",
               "IsEnabled", "ShowInEpg"
           ) VALUES (?, ?, ?, ?, 'ErsatzTV', ?, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1)"#,
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(format!("{tag}-{role}"))
    .bind(sort_number)
    .bind(format!("{tag}-channel-{role}"))
    .bind(profile_id)
    .execute(pool)
    .await?;
    Ok(mysql_inserted_entity_id(channel))
}

async fn insert_mysql_fixture_movie(
    pool: &db::DbPool,
    library_path_id: EntityId,
    title: &str,
) -> MysqlIntegrationResult<EntityId> {
    let media_item =
        sqlx::query(r#"INSERT INTO "MediaItem" ("LibraryPathId", "State") VALUES (?, 0)"#)
            .bind(library_path_id)
            .execute(pool)
            .await?;
    let media_id = mysql_inserted_entity_id(media_item);
    sqlx::query(r#"INSERT INTO "Movie" ("Id") VALUES (?)"#)
        .bind(media_id)
        .execute(pool)
        .await?;
    sqlx::query(
        r#"INSERT INTO "MovieMetadata" (
               "MovieId", "MetadataKind", "Title", "SortTitle", "DateAdded", "DateUpdated"
           ) VALUES (?, 0, ?, ?, '2030-01-15T00:00:00Z', '2030-01-15T00:00:00Z')"#,
    )
    .bind(media_id)
    .bind(title)
    .bind(title)
    .execute(pool)
    .await?;
    sqlx::query(
        r#"INSERT INTO "MediaVersion" (
               "MovieId", "Name", "Duration", "VideoScanKind", "DateAdded",
               "DateUpdated", "Width", "Height"
           ) VALUES (
               ?, 'main', '00:10:00', 1, '2030-01-15T00:00:00Z',
               '2030-01-15T00:00:00Z', 1920, 1080
           )"#,
    )
    .bind(media_id)
    .execute(pool)
    .await?;
    Ok(media_id)
}

async fn insert_mysql_fixture_playlist(
    pool: &db::DbPool,
    playlist_group_id: EntityId,
    name: &str,
    tag: &str,
) -> MysqlIntegrationResult<EntityId> {
    let playlist = sqlx::query(
        r#"INSERT INTO "Playlist" (
               "Name", "PlaylistGroupId", "IsSystem", "ManagedSource"
           ) VALUES (?, ?, 1, ?)"#,
    )
    .bind(name)
    .bind(playlist_group_id)
    .bind(format!("scan:v1:{tag}:{name}"))
    .execute(pool)
    .await?;
    Ok(mysql_inserted_entity_id(playlist))
}

async fn insert_mysql_playlist_item(
    pool: &db::DbPool,
    playlist_id: EntityId,
    media_id: EntityId,
    index: i32,
) -> MysqlIntegrationResult {
    sqlx::query(
        r#"INSERT INTO "PlaylistItem" (
               "Index", "PlaylistId", "CollectionType", "MediaItemId",
               "IncludeInProgramGuide", "PlaybackOrder", "PlayAll"
           ) VALUES (?, ?, 10, ?, 1, 1, 1)"#,
    )
    .bind(index)
    .bind(playlist_id)
    .bind(media_id)
    .execute(pool)
    .await?;
    Ok(())
}

async fn seed_mysql_classic_schedule(
    pool: &db::DbPool,
    repositories: &Repositories,
    name: &str,
    playlist_id: EntityId,
) -> MysqlIntegrationResult<EntityId> {
    let schedule = sqlx::query(
        r#"INSERT INTO "ProgramSchedule" (
               "FixedStartTimeBehavior", "KeepMultiPartEpisodesTogether", "Name",
               "RandomStartPoint", "ShuffleScheduleItems", "TreatCollectionsAsShows"
           ) VALUES (0, 1, ?, 0, 0, 0)"#,
    )
    .bind(name)
    .execute(pool)
    .await?;
    let schedule_id = mysql_inserted_entity_id(schedule);
    let item = sqlx::query(
        r#"INSERT INTO "ProgramScheduleItem" (
               "ProgramScheduleId", "CollectionType", "FillWithGroupMode", "GuideMode",
               "Index", "MarathonGroupBy", "MarathonShuffleGroups", "MarathonShuffleItems",
               "PlaybackOrder", "PlaylistId"
           ) VALUES (?, 6, 0, 0, 0, 0, 0, 0, 1, ?)"#,
    )
    .bind(schedule_id)
    .bind(playlist_id)
    .execute(pool)
    .await?;
    let schedule_item_id = mysql_inserted_entity_id(item);

    repositories
        .program_schedule
        .upsert_item(&ProgramScheduleItem {
            id: schedule_item_id,
            program_schedule_id: schedule_id,
            collection_type: CollectionType::Playlist,
            playlist_id: Some(playlist_id),
            playout_mode: PlayoutMode::One,
            playback_order: PlaybackOrder::Chronological,
            sort_order: 0,
            ..ProgramScheduleItem::default()
        })
        .await?;
    repositories
        .program_schedule
        .upsert(&ProgramSchedule {
            id: schedule_id,
            name: name.to_owned(),
            item_ids: vec![schedule_item_id],
            ..ProgramSchedule::default()
        })
        .await?;
    Ok(schedule_id)
}

async fn insert_mysql_classic_playout(
    pool: &db::DbPool,
    channel_id: EntityId,
    schedule_id: EntityId,
) -> MysqlIntegrationResult<EntityId> {
    let playout = sqlx::query(
        r#"INSERT INTO "Playout" (
               "ChannelId", "ProgramScheduleId", "ScheduleKind", "Seed"
           ) VALUES (?, ?, 1, 123)"#,
    )
    .bind(channel_id)
    .bind(schedule_id)
    .execute(pool)
    .await?;
    Ok(mysql_inserted_entity_id(playout))
}

async fn commit_mysql_scan_membership_change(
    pool: &db::DbPool,
    fixture: MysqlClassicRefreshFixture,
) -> MysqlIntegrationResult<Vec<EntityId>> {
    let reason = ScanPlayoutRefreshReason::RemoteStreamMembershipAdded;
    let generation = scan_refresh::begin_resolution(pool, fixture.library_id, reason).await?;
    let repositories = make_repositories(pool.clone());
    let affected = repositories
        .scheduling
        .classic_playouts_affected_by_scan(
            &BTreeSet::from([fixture.added_media_id]),
            Some(fixture.related_playlist_id),
        )
        .await?;
    let mut expected = vec![fixture.target_playout_id, fixture.oracle_playout_id];
    expected.sort_unstable();
    assert_eq!(
        affected, expected,
        "dependency resolution must be exact before the fixture mutates membership"
    );

    let mut tx = db::begin_write(pool).await?;
    sqlx::query(
        r#"INSERT INTO "PlaylistItem" (
               "Index", "PlaylistId", "CollectionType", "MediaItemId",
               "IncludeInProgramGuide", "PlaybackOrder", "PlayAll"
           ) VALUES (1, ?, 10, ?, 1, 1, 1)"#,
    )
    .bind(fixture.related_playlist_id)
    .bind(fixture.added_media_id)
    .execute(&mut *tx)
    .await?;
    scan_refresh::persist_refresh_intents(&mut tx, fixture.library_id, reason, &affected).await?;
    scan_refresh::complete_resolution_in_tx(&mut tx, fixture.library_id, generation, reason)
        .await?;
    tx.commit().await?;
    Ok(affected)
}

async fn run_mysql_classic_build(
    pool: &db::DbPool,
    locker: &EntityLocker,
    playout_id: EntityId,
    mode: PlayoutBuildMode,
    now: DateTime<Utc>,
) -> MysqlIntegrationResult {
    assert!(
        run_requested_playout_build(
            pool,
            locker,
            PlayoutBuildRequest { playout_id, mode },
            now,
            None,
        )
        .await?,
        "the MySQL acceptance fixture must acquire the scheduler playout lock"
    );
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
struct MysqlPersistedClassicRows {
    playout: Vec<String>,
    playout_items: Vec<String>,
    playout_gaps: Vec<String>,
    playout_history: Vec<String>,
    playout_anchor: Vec<String>,
    schedule_item_enumerator: Vec<String>,
    program_schedule_anchors: Vec<String>,
    collection_enumerators: Vec<String>,
    fill_group_indices: Vec<String>,
    fill_group_enumerators: Vec<String>,
    classic_checkpoints: Vec<String>,
    build_status: Vec<String>,
}

#[allow(clippy::too_many_lines)]
async fn mysql_persisted_classic_rows(
    pool: &db::DbPool,
    playout_id: EntityId,
    equivalent_playout: bool,
) -> MysqlIntegrationResult<MysqlPersistedClassicRows> {
    let playout_ignored: &[&str] = if equivalent_playout {
        &["Id", "ChannelId"]
    } else {
        &[]
    };
    let generated_ignored: &[&str] = if equivalent_playout {
        &["Id", "PlayoutId"]
    } else {
        &[]
    };
    let playout_id_ignored: &[&str] = if equivalent_playout {
        &["PlayoutId"]
    } else {
        &[]
    };
    let anchor_ignored: &[&str] = if equivalent_playout {
        &["Id", "PlayoutId"]
    } else {
        &[]
    };
    let schedule_state_ignored: &[&str] = if equivalent_playout {
        &["PlayoutAnchorPlayoutId"]
    } else {
        &[]
    };
    let collection_state_ignored: &[&str] = if equivalent_playout {
        &["PlayoutProgramScheduleAnchorId"]
    } else {
        &[]
    };
    let fill_state_ignored: &[&str] = if equivalent_playout {
        &["PlayoutScheduleItemFillGroupIndexId"]
    } else {
        &[]
    };

    Ok(MysqlPersistedClassicRows {
        playout: mysql_direct_rows(pool, "Playout", "Id", playout_id, playout_ignored).await?,
        playout_items: mysql_direct_rows(
            pool,
            "PlayoutItem",
            "PlayoutId",
            playout_id,
            generated_ignored,
        )
        .await?,
        playout_gaps: mysql_direct_rows(
            pool,
            "PlayoutGap",
            "PlayoutId",
            playout_id,
            generated_ignored,
        )
        .await?,
        playout_history: mysql_direct_rows(
            pool,
            "PlayoutHistory",
            "PlayoutId",
            playout_id,
            generated_ignored,
        )
        .await?,
        playout_anchor: mysql_direct_rows(
            pool,
            "PlayoutAnchor",
            "PlayoutId",
            playout_id,
            playout_id_ignored,
        )
        .await?,
        schedule_item_enumerator: mysql_direct_rows(
            pool,
            "ScheduleItemsEnumeratorState",
            "PlayoutAnchorPlayoutId",
            playout_id,
            schedule_state_ignored,
        )
        .await?,
        program_schedule_anchors: mysql_direct_rows(
            pool,
            "PlayoutProgramScheduleAnchor",
            "PlayoutId",
            playout_id,
            anchor_ignored,
        )
        .await?,
        collection_enumerators: mysql_child_rows(
            pool,
            "CollectionEnumeratorState",
            "PlayoutProgramScheduleAnchorId",
            "PlayoutProgramScheduleAnchor",
            "Id",
            playout_id,
            collection_state_ignored,
        )
        .await?,
        fill_group_indices: mysql_direct_rows(
            pool,
            "PlayoutScheduleItemFillGroupIndex",
            "PlayoutId",
            playout_id,
            generated_ignored,
        )
        .await?,
        fill_group_enumerators: mysql_child_rows(
            pool,
            "FillGroupEnumeratorState",
            "PlayoutScheduleItemFillGroupIndexId",
            "PlayoutScheduleItemFillGroupIndex",
            "Id",
            playout_id,
            fill_state_ignored,
        )
        .await?,
        classic_checkpoints: mysql_direct_rows(
            pool,
            "PlayoutClassicCheckpoint",
            "PlayoutId",
            playout_id,
            playout_id_ignored,
        )
        .await?,
        build_status: mysql_direct_rows(
            pool,
            "PlayoutBuildStatus",
            "PlayoutId",
            playout_id,
            playout_id_ignored,
        )
        .await?,
    })
}

async fn mysql_direct_rows(
    pool: &db::DbPool,
    table: &str,
    playout_column: &str,
    playout_id: EntityId,
    ignored_columns: &[&str],
) -> MysqlIntegrationResult<Vec<String>> {
    let columns = mysql_table_columns(pool, table, ignored_columns).await?;
    let projection = mysql_row_projection("fixture_row", &columns);
    let sql = format!(
        "SELECT {projection} FROM {} AS {} WHERE {}.{} = ?",
        mysql_quote_identifier(table),
        mysql_quote_identifier("fixture_row"),
        mysql_quote_identifier("fixture_row"),
        mysql_quote_identifier(playout_column),
    );
    let mut rows = sqlx::query_scalar::<_, String>(db::assert_sql_safe(sql))
        .bind(playout_id)
        .fetch_all(pool)
        .await?;
    rows.sort();
    Ok(rows)
}

#[allow(clippy::too_many_arguments)]
async fn mysql_child_rows(
    pool: &db::DbPool,
    child_table: &str,
    child_foreign_key: &str,
    parent_table: &str,
    parent_primary_key: &str,
    playout_id: EntityId,
    ignored_columns: &[&str],
) -> MysqlIntegrationResult<Vec<String>> {
    let columns = mysql_table_columns(pool, child_table, ignored_columns).await?;
    let projection = mysql_row_projection("child", &columns);
    let sql = format!(
        "SELECT {projection} FROM {} AS {} JOIN {} AS {} ON {}.{} = {}.{} \
         WHERE {}.{} = ?",
        mysql_quote_identifier(child_table),
        mysql_quote_identifier("child"),
        mysql_quote_identifier(parent_table),
        mysql_quote_identifier("parent"),
        mysql_quote_identifier("parent"),
        mysql_quote_identifier(parent_primary_key),
        mysql_quote_identifier("child"),
        mysql_quote_identifier(child_foreign_key),
        mysql_quote_identifier("parent"),
        mysql_quote_identifier("PlayoutId"),
    );
    let mut rows = sqlx::query_scalar::<_, String>(db::assert_sql_safe(sql))
        .bind(playout_id)
        .fetch_all(pool)
        .await?;
    rows.sort();
    Ok(rows)
}

async fn mysql_table_columns(
    pool: &db::DbPool,
    table: &str,
    ignored_columns: &[&str],
) -> MysqlIntegrationResult<Vec<String>> {
    let columns = sqlx::query_scalar::<_, String>(
        r#"SELECT "COLUMN_NAME"
           FROM "INFORMATION_SCHEMA"."COLUMNS"
           WHERE "TABLE_SCHEMA" = DATABASE() AND "TABLE_NAME" = ?
           ORDER BY "ORDINAL_POSITION""#,
    )
    .bind(table)
    .fetch_all(pool)
    .await?
    .into_iter()
    .filter(|column| !ignored_columns.contains(&column.as_str()))
    .collect::<Vec<_>>();
    assert!(!columns.is_empty(), "MySQL table {table} must exist");
    Ok(columns)
}

fn mysql_row_projection(alias: &str, columns: &[String]) -> String {
    let values = columns
        .iter()
        .map(|column| {
            format!(
                "COALESCE(HEX(CAST({}.{} AS BINARY)), '<NULL>')",
                mysql_quote_identifier(alias),
                mysql_quote_identifier(column),
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("CONCAT_WS(CHAR(31), {values})")
}

fn mysql_quote_identifier(identifier: &str) -> String {
    format!("`{}`", identifier.replace('`', "``"))
}

async fn mysql_fixture_intended_playout_ids(
    pool: &db::DbPool,
    fixture: MysqlClassicRefreshFixture,
) -> MysqlIntegrationResult<Vec<EntityId>> {
    Ok(sqlx::query_scalar(
        r#"SELECT "PlayoutId" FROM "ScanPlayoutRefreshIntent"
           WHERE "PlayoutId" IN (?, ?, ?) ORDER BY "PlayoutId""#,
    )
    .bind(fixture.target_playout_id)
    .bind(fixture.oracle_playout_id)
    .bind(fixture.unrelated_playout_id)
    .fetch_all(pool)
    .await?)
}

async fn mysql_playout_intent_count(
    pool: &db::DbPool,
    playout_id: EntityId,
) -> MysqlIntegrationResult<i64> {
    Ok(sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "ScanPlayoutRefreshIntent" WHERE "PlayoutId" = ?"#,
    )
    .bind(playout_id)
    .fetch_one(pool)
    .await?)
}

async fn mysql_playout_media_count(
    pool: &db::DbPool,
    playout_id: EntityId,
    media_id: EntityId,
) -> MysqlIntegrationResult<i64> {
    Ok(sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "PlayoutItem"
           WHERE "PlayoutId" = ? AND "MediaItemId" = ?"#,
    )
    .bind(playout_id)
    .bind(media_id)
    .fetch_one(pool)
    .await?)
}

async fn mysql_playlist_membership_count(
    pool: &db::DbPool,
    playlist_id: EntityId,
    media_id: EntityId,
) -> MysqlIntegrationResult<i64> {
    Ok(sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "PlaylistItem"
           WHERE "PlaylistId" = ? AND "MediaItemId" = ?"#,
    )
    .bind(playlist_id)
    .bind(media_id)
    .fetch_one(pool)
    .await?)
}

async fn mysql_scan_resolution_state(
    pool: &db::DbPool,
    library_id: EntityId,
) -> MysqlIntegrationResult<i64> {
    Ok(
        sqlx::query_scalar(r#"SELECT "State" FROM "ScanRefreshResolution" WHERE "LibraryId" = ?"#)
            .bind(library_id)
            .fetch_one(pool)
            .await?,
    )
}

fn mysql_inserted_entity_id(result: db::DbQueryResult) -> EntityId {
    db::last_insert_id(result)
}

async fn cleanup_mysql_classic_refresh_fixture(
    database_url: &str,
    tag: &str,
) -> MysqlIntegrationResult {
    let pool = db::init(database_url).await?;
    let channel_pattern = format!("{tag}-channel-%");
    let schedule_pattern = format!("{tag}-schedule-%");
    let library_name = format!("{tag}-library");
    let media_source_ids = sqlx::query_scalar::<_, EntityId>(
        r#"SELECT "MediaSourceId" FROM "Library" WHERE "Name" = ?"#,
    )
    .bind(&library_name)
    .fetch_all(&pool)
    .await?;

    let mut tx = db::begin_write(&pool).await?;
    sqlx::query(
        r#"DELETE FROM "Playout"
           WHERE "ChannelId" IN (SELECT "Id" FROM "Channel" WHERE "Name" LIKE ?)
              OR "ProgramScheduleId" IN (
                     SELECT "Id" FROM "ProgramSchedule" WHERE "Name" LIKE ?
                 )"#,
    )
    .bind(&channel_pattern)
    .bind(&schedule_pattern)
    .execute(&mut *tx)
    .await?;
    sqlx::query(r#"DELETE FROM "ProgramSchedule" WHERE "Name" LIKE ?"#)
        .bind(&schedule_pattern)
        .execute(&mut *tx)
        .await?;
    sqlx::query(r#"DELETE FROM "Channel" WHERE "Name" LIKE ?"#)
        .bind(&channel_pattern)
        .execute(&mut *tx)
        .await?;
    sqlx::query(r#"DELETE FROM "FFmpegProfile" WHERE "Name" = ?"#)
        .bind(format!("{tag}-profile"))
        .execute(&mut *tx)
        .await?;
    sqlx::query(r#"DELETE FROM "PlaylistGroup" WHERE "Name" = ?"#)
        .bind(format!("{tag}-group"))
        .execute(&mut *tx)
        .await?;
    sqlx::query(r#"DELETE FROM "Library" WHERE "Name" = ?"#)
        .bind(&library_name)
        .execute(&mut *tx)
        .await?;
    for media_source_id in media_source_ids {
        sqlx::query(r#"DELETE FROM "MediaSource" WHERE "Id" = ?"#)
            .bind(media_source_id)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query(r#"DELETE FROM "Resolution" WHERE "Name" = ?"#)
        .bind(format!("{tag}-resolution"))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn mysql_persists_resumable_addon_import_progress_and_policy() {
    #[allow(clippy::disallowed_methods)]
    let Ok(database_url) = std::env::var("ERSATZRS_MYSQL_TEST_URL") else {
        eprintln!("skipping MySQL add-on import test: ERSATZRS_MYSQL_TEST_URL is not set");
        return;
    };
    let _test_guard = MYSQL_TEST_LOCK.lock().await;

    let pool = db::init(&database_url)
        .await
        .expect("initialize MySQL test database");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let addon_id = format!("org.ersatzrs.mysql.{suffix}");
    let source_url = format!("https://mysql.example.test/{suffix}");
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        r#"INSERT INTO "AddonInstallation" (
               "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
               "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt"
           ) VALUES (?, '1.0.0', 1, ?, ?, '{}', ?, ?, ?)"#,
    )
    .bind(&addon_id)
    .bind("a".repeat(64))
    .bind("b".repeat(64))
    .bind(format!("addons/{suffix}"))
    .bind(&now)
    .bind(&now)
    .execute(&pool)
    .await
    .expect("seed MySQL add-on installation");
    let inserted = sqlx::query(
        r#"INSERT INTO "AddonMediaList" (
               "AddonId", "ProviderId", "SourceUrl", "Name", "ItemCount",
               "ProviderMetadataJson", "MetadataOverrideJson", "DetailImportPolicy"
           ) VALUES (?, ?, ?, 'MySQL resumable import', 37, '{}', '{}', 0)"#,
    )
    .bind(&addon_id)
    .bind(format!("fixture/{suffix}"))
    .bind(&source_url)
    .execute(&pool)
    .await
    .expect("seed MySQL add-on list");
    let list_id = i64::try_from(db::last_insert_id(inserted)).expect("MySQL list identity");
    sqlx::query(
        r#"INSERT INTO "AddonMediaListImportJob" (
               "AddonMediaListId", "Generation", "State", "RecordCapability",
               "DiscoveredCount", "OverviewComplete", "CreatedAt", "UpdatedAt",
               "LastProgressAt", "CompletedAt"
           ) VALUES (?, 1, 3, 'media-list.list.v5', 37, 1, ?, ?, ?, ?)"#,
    )
    .bind(list_id)
    .bind(&now)
    .bind(&now)
    .bind(&now)
    .bind(&now)
    .execute(&pool)
    .await
    .expect("persist MySQL import checkpoint");

    let coordinator = AddonMediaListImportCoordinator::new(pool.clone());
    let progress = coordinator
        .progress(list_id)
        .await
        .expect("read MySQL import progress")
        .expect("progress row");
    assert_eq!(progress.state, AddonMediaListImportState::Ready);
    assert_eq!(progress.discovered_count, 37);
    coordinator
        .set_detail_policy(list_id, AddonMediaListDetailPolicy::ManualOnly)
        .await
        .expect("persist MySQL manual-only policy");
    let policy: i32 =
        sqlx::query_scalar(r#"SELECT "DetailImportPolicy" FROM "AddonMediaList" WHERE "Id" = ?"#)
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .expect("read MySQL detail policy");
    assert_eq!(policy, 1);

    sqlx::query(r#"REPLACE INTO "ConfigElement" ("Key", "Value") VALUES (?, ?), (?, ?)"#)
        .bind(ersatzrs_domain::CONFIG_KEY_ADDON_DETAIL_WAIT_MINIMUM_SECONDS)
        .bind("7")
        .bind(ersatzrs_domain::CONFIG_KEY_ADDON_DETAIL_WAIT_MAXIMUM_SECONDS)
        .bind("11")
        .execute(&pool)
        .await
        .expect("persist MySQL add-on detail wait settings");
    sqlx::query(
        r#"INSERT INTO "AddonProviderHostCooldown" (
               "ProviderHost", "NextEligibleAt", "SampledDelaySeconds",
               "LastAttemptAt", "UpdatedAt"
           ) VALUES (?, ?, 9, ?, ?)"#,
    )
    .bind(format!("{suffix}.example.test"))
    .bind(&now)
    .bind(&now)
    .bind(&now)
    .execute(&pool)
    .await
    .expect("persist MySQL provider cooldown");
    let sampled_delay: i32 = sqlx::query_scalar(
        r#"SELECT "SampledDelaySeconds" FROM "AddonProviderHostCooldown"
           WHERE "ProviderHost" = ?"#,
    )
    .bind(format!("{suffix}.example.test"))
    .fetch_one(&pool)
    .await
    .expect("read MySQL provider cooldown");
    assert_eq!(sampled_delay, 9);

    sqlx::query(r#"DELETE FROM "AddonMediaList" WHERE "Id" = ?"#)
        .bind(list_id)
        .execute(&pool)
        .await
        .expect("remove MySQL list fixture");
    sqlx::query(r#"DELETE FROM "AddonInstallation" WHERE "AddonId" = ?"#)
        .bind(&addon_id)
        .execute(&pool)
        .await
        .expect("remove MySQL add-on fixture");
    sqlx::query(r#"DELETE FROM "AddonProviderHostCooldown" WHERE "ProviderHost" = ?"#)
        .bind(format!("{suffix}.example.test"))
        .execute(&pool)
        .await
        .expect("remove MySQL provider cooldown fixture");
    sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key" IN (?, ?)"#)
        .bind(ersatzrs_domain::CONFIG_KEY_ADDON_DETAIL_WAIT_MINIMUM_SECONDS)
        .bind(ersatzrs_domain::CONFIG_KEY_ADDON_DETAIL_WAIT_MAXIMUM_SECONDS)
        .execute(&pool)
        .await
        .expect("remove MySQL add-on detail wait settings");
}

/// The graph-state row's persist/overwrite/clear/reload lifecycle on `MySQL`.
///
/// This row is written at startup and on every `FFmpeg` settings save, so a
/// `SQLite`-only upsert (`ON CONFLICT ... excluded`) breaks both paths on `MySQL`.
/// The `SQLite` half of this lifecycle lives in the `hardware_graph` unit tests;
/// this is the backend that actually rejects the wrong statement.
#[tokio::test]
async fn mysql_persists_and_clears_hardware_graph_state() {
    #[allow(clippy::disallowed_methods)]
    let Ok(database_url) = std::env::var("ERSATZRS_MYSQL_TEST_URL") else {
        eprintln!("skipping MySQL graph-state test: ERSATZRS_MYSQL_TEST_URL is not set");
        return;
    };
    let _test_guard = MYSQL_TEST_LOCK.lock().await;

    let pool = db::init(&database_url)
        .await
        .expect("initialize MySQL test database");

    let denial = ersatzrs_ffmpeg::HardwareGraphDenial::new(
        ersatzrs_ffmpeg::HardwareGraphOperation::NvencHdrVulkanLibplacebo,
        "NVIDIA GeForce RTX 3080 Ti",
        ersatzrs_ffmpeg::GraphFailureKind::CudaInterop,
    );

    ersatzrs_infra::hardware_graph::clear_hardware_graph_denials(&pool)
        .await
        .expect("clear any prior state");

    let mut report = ersatzrs_ffmpeg::FfmpegCapabilityReport::default();
    report.deny_hardware_graph(denial.clone());
    ersatzrs_infra::hardware_graph::persist_hardware_graph_denials(&pool, "/opt/ffmpeg", &report)
        .await
        .expect("persist graph denial on MySQL");

    assert_eq!(
        ersatzrs_infra::hardware_graph::load_hardware_graph_denials(&pool)
            .await
            .expect("reload"),
        vec![denial]
    );

    // Overwriting the same key must replace, which is what the upsert statement
    // has to get right on both backends.
    let mut second = ersatzrs_ffmpeg::FfmpegCapabilityReport::default();
    second.deny_hardware_graph(ersatzrs_ffmpeg::HardwareGraphDenial::new(
        ersatzrs_ffmpeg::HardwareGraphOperation::NvencCudaScale,
        "NVIDIA GeForce RTX 3080 Ti",
        ersatzrs_ffmpeg::GraphFailureKind::EncoderInit,
    ));
    ersatzrs_infra::hardware_graph::persist_hardware_graph_denials(&pool, "/opt/ffmpeg", &second)
        .await
        .expect("overwrite graph denial on MySQL");
    let reloaded = ersatzrs_infra::hardware_graph::load_hardware_graph_denials(&pool)
        .await
        .expect("reload after overwrite");
    assert_eq!(reloaded.len(), 1, "overwrite must replace, not append");

    // Startup reload, then the settings-save clear.
    let mut startup = ersatzrs_ffmpeg::FfmpegCapabilityReport::default();
    ersatzrs_infra::hardware_graph::apply_hardware_graph_denials(
        &pool,
        "/opt/ffmpeg",
        &mut startup,
    )
    .await
    .expect("apply at startup");
    assert_eq!(startup.hardware_graph_denials.len(), 1);

    ersatzrs_infra::hardware_graph::clear_hardware_graph_denials(&pool)
        .await
        .expect("clear on capability refresh");
    assert!(
        ersatzrs_infra::hardware_graph::load_hardware_graph_denials(&pool)
            .await
            .expect("reload after clear")
            .is_empty()
    );
}

/// The Output Dynamic Range health check on `MySQL`.
///
/// `MySQL` rejects an `ORDER BY` column absent from a `SELECT DISTINCT`
/// projection (error 3065), so this check used to fail on every health run
/// against `MySQL` while passing on `SQLite`. Duplicate profile names are seeded
/// deliberately: they are what makes the `DISTINCT` meaningful and what the
/// dropped `Id` tie-breaker was there for.
#[tokio::test]
async fn mysql_output_dynamic_range_health_check_classifies_every_row_shape() {
    #[allow(clippy::disallowed_methods)]
    let Ok(database_url) = std::env::var("ERSATZRS_MYSQL_TEST_URL") else {
        eprintln!("skipping MySQL health-check test: ERSATZRS_MYSQL_TEST_URL is not set");
        return;
    };
    let _test_guard = MYSQL_TEST_LOCK.lock().await;

    let pool = db::init(&database_url)
        .await
        .expect("initialize MySQL test database");

    // No HDR10 profiles at all.
    let result = ersatzrs_infra::health::run_output_dynamic_range_check(&pool)
        .await
        .expect("check with no HDR10 profiles");
    assert_eq!(
        result.status,
        ersatzrs_core::health::HealthCheckStatus::NotApplicable
    );

    // Two channels sharing one profile name, so the DISTINCT projection has
    // duplicates to collapse.
    seed_hdr10_channel(&pool, 900, "shared name", 1, 1, 1, 0).await;
    seed_hdr10_channel(&pool, 901, "shared name", 1, 1, 1, 0).await;
    let result = ersatzrs_infra::health::run_output_dynamic_range_check(&pool)
        .await
        .expect("check with an h264 HDR10 profile");
    assert_eq!(
        result.status,
        ersatzrs_core::health::HealthCheckStatus::Fail
    );

    // Hardware without a proven HDR10 graph demotes to software.
    reset_hdr10_channels(&pool).await;
    seed_hdr10_channel(&pool, 902, "vaapi hdr10", 2, 1, 1, 3).await;
    let result = ersatzrs_infra::health::run_output_dynamic_range_check(&pool)
        .await
        .expect("check with a VAAPI HDR10 profile");
    assert_eq!(
        result.status,
        ersatzrs_core::health::HealthCheckStatus::Warning
    );

    // Video copy makes the selection inert rather than wrong.
    reset_hdr10_channels(&pool).await;
    seed_hdr10_channel(&pool, 903, "copy hdr10", 99, 0, 0, 0).await;
    let result = ersatzrs_infra::health::run_output_dynamic_range_check(&pool)
        .await
        .expect("check with a copy HDR10 profile");
    assert_eq!(
        result.status,
        ersatzrs_core::health::HealthCheckStatus::Info
    );

    reset_hdr10_channels(&pool).await;
}

async fn reset_hdr10_channels(pool: &db::DbPool) {
    // Child rows first: the orientation seed hangs media and playout rows off
    // the same ids.
    for table in [
        "PlayoutItem",
        "Playout",
        "MediaStream",
        "MediaFile",
        "MediaVersion",
        "Movie",
        "MediaItem",
        "Channel",
        "FFmpegProfile",
    ] {
        let sql = format!(r#"DELETE FROM `{table}` WHERE `Id` >= 900"#);
        sqlx::query(ersatzrs_infra::db::assert_sql_safe(sql))
            .execute(pool)
            .await
            .expect("reset seeded rows");
    }
}

/// ADR 0003 / `p8c-02`: the Source Orientation query joins `MediaVersion`'s
/// per-type id columns through `COALESCE` under `SELECT DISTINCT`, which is the
/// shape `MySQL` and `SQLite` have disagreed about before. Assert the same
/// classification on both.
#[tokio::test]
async fn mysql_source_orientation_health_check_reports_only_copied_rotated_items() {
    #[allow(clippy::disallowed_methods)]
    let Ok(database_url) = std::env::var("ERSATZRS_MYSQL_TEST_URL") else {
        eprintln!("skipping MySQL orientation test: ERSATZRS_MYSQL_TEST_URL is not set");
        return;
    };
    let _test_guard = MYSQL_TEST_LOCK.lock().await;

    let pool = db::init(&database_url)
        .await
        .expect("initialize MySQL test database");
    reset_hdr10_channels(&pool).await;

    // Rotated, but transcoded: autorotation bakes it into the pixels.
    seed_rotated_item_on_channel(&pool, 910, 1, 1, 90).await;
    let result = ersatzrs_infra::health::run_source_orientation_check(&pool)
        .await
        .expect("check with a transcoded rotated item");
    assert_eq!(
        result.status,
        ersatzrs_core::health::HealthCheckStatus::NotApplicable
    );

    // Copied and rotated: MPEG-TS drops the display matrix.
    reset_hdr10_channels(&pool).await;
    seed_rotated_item_on_channel(&pool, 911, 99, 0, 270).await;
    let result = ersatzrs_infra::health::run_source_orientation_check(&pool)
        .await
        .expect("check with a copied rotated item");
    assert_eq!(
        result.status,
        ersatzrs_core::health::HealthCheckStatus::Warning
    );
    assert!(
        result.message.contains("portrait-911.mp4"),
        "{}",
        result.message
    );

    reset_hdr10_channels(&pool).await;
}

/// Seed a channel whose playout holds one rotated video item.
async fn seed_rotated_item_on_channel(
    pool: &db::DbPool,
    id: i64,
    video_format: i32,
    transcode_video: i32,
    rotation: i32,
) {
    seed_hdr10_channel(pool, id, "orientation", video_format, 0, transcode_video, 0).await;
    sqlx::query(ersatzrs_infra::db::assert_sql_safe(
        r#"INSERT IGNORE INTO `Library` (`Id`, `Name`, `MediaKind`, `MediaSourceId`)
           SELECT 900, 'orientation', 1, MIN(`Id`) FROM `LocalMediaSource`"#
            .to_owned(),
    ))
    .execute(pool)
    .await
    .ok();

    for statement in [
        format!(r#"INSERT INTO `MediaItem` (`Id`, `State`) VALUES ({id}, 0)"#),
        format!(r#"INSERT INTO `Movie` (`Id`) VALUES ({id})"#),
        format!(
            r#"INSERT INTO `MediaVersion` (
                   `Id`, `DateAdded`, `DateUpdated`, `Duration`, `Height`,
                   `MovieId`, `VideoScanKind`, `Width`
               ) VALUES (
                   {id}, '2026-08-08T00:00:00Z', '2026-08-08T00:00:00Z',
                   '01:00:00', 1920, {id}, 1, 1080
               )"#
        ),
        format!(
            r#"INSERT INTO `MediaStream` (
                   `Id`, `Index`, `Codec`, `MediaStreamKind`, `Channels`,
                   `Default`, `Forced`, `MediaVersionId`, `Rotation`
               ) VALUES ({id}, 0, 'h264', 2, 0, 1, 0, {id}, {rotation})"#
        ),
        format!(
            r#"INSERT INTO `MediaFile` (`MediaVersionId`, `Path`, `PathHash`)
               VALUES ({id}, '/media/portrait-{id}.mp4', 'hash-{id}')"#
        ),
        format!(
            r#"INSERT INTO `Playout` (`Id`, `ChannelId`, `ScheduleKind`, `Seed`)
               VALUES ({id}, {id}, 1, 0)"#
        ),
        format!(
            r#"INSERT INTO `PlayoutItem` (
                   `Id`, `DisableWatermarks`, `FillerKind`, `Finish`, `GuideGroup`,
                   `InPoint`, `MediaItemId`, `OutPoint`, `PlayoutId`, `Start`
               ) VALUES (
                   {id}, 0, 0, '2026-08-08T01:00:00Z', 0, '00:00:00', {id},
                   '01:00:00', {id}, '2026-08-08T00:00:00Z'
               )"#
        ),
    ] {
        sqlx::query(ersatzrs_infra::db::assert_sql_safe(statement))
            .execute(pool)
            .await
            .expect("seed rotated playout item");
    }
}

#[allow(clippy::too_many_arguments)]
async fn seed_hdr10_channel(
    pool: &db::DbPool,
    id: i64,
    name: &str,
    video_format: i32,
    bit_depth: i32,
    transcode_video: i32,
    acceleration: i32,
) {
    sqlx::query(ersatzrs_infra::db::assert_sql_safe(
        r#"INSERT IGNORE INTO `Resolution` (`Id`, `Name`, `IsCustom`, `Width`, `Height`)
           VALUES (900, '1920x1080', 0, 1920, 1080)"#
            .to_owned(),
    ))
    .execute(pool)
    .await
    .expect("seed resolution");

    let profile = format!(
        r#"INSERT INTO `FFmpegProfile` (
            `Id`, `AudioBitrate`, `AudioBufferSize`, `AudioChannels`, `AudioFormat`,
            `AudioSampleRate`, `BitDepth`, `HardwareAcceleration`, `Name`,
            `NormalizeFramerate`, `NormalizeLoudnessMode`, `ResolutionId`,
            `ScalingBehavior`, `ThreadCount`, `VaapiDriver`, `VideoBitrate`,
            `VideoBufferSize`, `VideoFormat`, `TranscodeVideo`, `OutputDynamicRange`
           ) VALUES ({id}, 192, 384, 2, 1, 48, {bit_depth}, {acceleration}, '{name}',
            0, 0, 900, 0, 0, 0, 2000, 4000, {video_format}, {transcode_video}, 1)"#
    );
    sqlx::query(ersatzrs_infra::db::assert_sql_safe(profile))
        .execute(pool)
        .await
        .expect("seed hdr10 profile");

    let channel = format!(
        r#"INSERT INTO `Channel` (
            `Id`, `UniqueId`, `Number`, `SortNumber`, `Name`, `FFmpegProfileId`,
            `IdleBehavior`, `MusicVideoCreditsMode`, `PlayoutMode`, `PlayoutSource`,
            `SongVideoMode`, `StreamingMode`, `StreamSelectorMode`, `SubtitleMode`,
            `TranscodeMode`
           ) VALUES ({id}, UUID(), '{id}', {id}.0, '{name}', {id}, 0, 0, 1, 0, 0, 1, 0, 0, 0)"#
    );
    sqlx::query(ersatzrs_infra::db::assert_sql_safe(channel))
        .execute(pool)
        .await
        .expect("seed channel");
}
