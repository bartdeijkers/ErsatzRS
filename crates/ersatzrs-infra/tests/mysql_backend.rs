#![cfg(all(feature = "mysql", feature = "mysql-integration"))]

use ersatzrs_core::repositories::{
    ChannelRepository, ConfigRepository, FfmpegProfileRepository, PlayoutRepository,
    ProgramScheduleRepository,
};
use ersatzrs_domain::{
    AddonMediaListDetailPolicy, AddonMediaListImportState, CollectionEnumeratorState,
    CollectionType, Playout, PlayoutProgramScheduleAnchor, PlayoutScheduleItemFillGroupIndex,
    ProgramSchedule,
};
use ersatzrs_infra::addon_media_list_import::AddonMediaListImportCoordinator;
use ersatzrs_infra::db;
use ersatzrs_infra::repositories::make_repositories;
use sqlx::Row;

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn mysql_backend_runs_migrations_and_representative_queries() {
    #[allow(clippy::disallowed_methods)]
    let Ok(database_url) = std::env::var("ERSATZRS_MYSQL_TEST_URL") else {
        eprintln!("skipping MySQL integration test: ERSATZRS_MYSQL_TEST_URL is not set");
        return;
    };

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
}

#[tokio::test]
async fn mysql_persists_resumable_addon_import_progress_and_policy() {
    #[allow(clippy::disallowed_methods)]
    let Ok(database_url) = std::env::var("ERSATZRS_MYSQL_TEST_URL") else {
        eprintln!("skipping MySQL add-on import test: ERSATZRS_MYSQL_TEST_URL is not set");
        return;
    };

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
