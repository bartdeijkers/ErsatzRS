//! Existing Jellyfin negative audio -> ordinary rescan -> actual Next playback.
use super::*;
use ersatzrs_core::repositories::PlayoutRepository as _;
use httpmock::prelude::*;
use sqlx::SqlitePool;

#[tokio::test]
#[ignore = "requires exact Next/FFmpeg9; repaired Jellyfin metadata and decoded playback"]
async fn real_next_repaired_jellyfin_audio_preserves_selected_tracks() {
    repaired_provider_playback(false, [0, 1, 2, 3], &[], false).await;
}

#[tokio::test]
#[ignore = "requires exact Next/FFmpeg9; unknown provider HDR and real frame metadata"]
async fn real_next_unknown_provider_hdr_keeps_frame_probe_and_selected_tracks() {
    repaired_provider_playback(true, [0, 1, 2, 3], &[], false).await;
}

#[tokio::test]
#[ignore = "requires exact Next/FFmpeg9; row295 physical mapping and decoded AV/captions"]
async fn row295_real_next_maps_external_first_last_and_interleaved_tracks() {
    for (logical, external) in [
        ([2, 3, 4, 5], [0, 1]),
        ([0, 1, 2, 3], [4, 5]),
        ([0, 2, 3, 5], [1, 4]),
    ] {
        repaired_provider_playback(false, logical, &external, false).await;
    }
}

#[tokio::test]
#[ignore = "requires exact Next/FFmpeg9; row295 Convert after persisted SQLite restart"]
async fn row295_real_next_convert_preserves_mapped_tracks_after_restart() {
    for (logical, external) in [
        ([2, 3, 4, 5], [0, 1]),
        ([0, 1, 2, 3], [4, 5]),
        ([0, 2, 3, 5], [1, 4]),
    ] {
        repaired_provider_playback(false, logical, &external, true).await;
    }
}

async fn repaired_provider_playback(hdr: bool, logical: [i32; 4], external: &[i32], convert: bool) {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_test_writer()
        .try_init()
        .ok();
    let _owned_data = runtime_path(crate::file_system_layout::APP_DATA_DIR_ENV_VAR);
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let root = tempfile::tempdir().unwrap().keep();
    println!("Jellyfin repair/Next proof retained at {}", root.display());
    let clip = text_subtitle_runtime_tests::generate_text_clip_with_cues(
        &ffmpeg,
        &root,
        if convert {
            "1\n00:00:05,000 --> 00:00:55,000\nNext text conversion proof\n"
        } else {
            "1\n00:00:00,000 --> 00:01:00,000\nNext subtitle proof\n"
        },
    )
    .await;
    let clip = if hdr {
        hdr_fixture(&ffmpeg, &ffprobe, &clip, &root).await
    } else {
        clip
    };
    let profile = FFmpegProfile {
        video_format: FFmpegProfileVideoFormat::H264,
        audio_format: FFmpegProfileAudioFormat::Aac,
        video_bitrate: 1000,
        video_buffer_size: 2000,
        audio_sample_rate: 48,
        audio_channels: 1,
        ..Default::default()
    };
    let database = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(root.join("provider.sqlite"))
        .create_if_missing(true);
    let (pool, locker) = if convert {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(database.clone())
            .await
            .unwrap();
        let pool = selection::tests::seed_fixture_pool(pool).await;
        prepare_schedule_with_pool(pool, &clip, false, &profile).await
    } else {
        prepare_schedule(&clip, false, &profile).await
    };
    sqlx::raw_sql(r#"
        DELETE FROM "Subtitle";
        DELETE FROM "MediaFile" WHERE "Id"=2;
        DELETE FROM "LocalLibrary" WHERE "Id"=1;
        INSERT INTO "JellyfinMediaSource" ("Id") VALUES (1);
        INSERT INTO "JellyfinLibrary" ("Id","ItemId","ShouldSyncItems") VALUES (1,'library-1',1);
        INSERT INTO "JellyfinMovie" ("Id","ItemId","Etag") VALUES (10,'movie-1','same');
        INSERT INTO "MediaStream" ("MediaVersionId","Index","Codec","MediaStreamKind","Channels","Default","Forced") VALUES (100,-1,'aac',2,2,0,0);
    "#).execute(&pool).await.unwrap();
    let server = MockServer::start_async().await;
    repair_with_ordinary_scans(&pool, &server, &clip, hdr, logical, external).await;
    let version: i64 = sqlx::query_scalar(r#"SELECT "Id" FROM "MediaVersion" WHERE "MovieId"=10"#)
        .fetch_one(&pool)
        .await
        .unwrap();
    pin_provider_choice(&pool, version, hdr, logical, convert).await;
    let (pool, locker) = if convert {
        let pool = reopen_provider_pool(pool, version, database).await;
        drop(locker);
        (pool, ersatzrs_core::locking::EntityLocker::default())
    } else {
        (pool, locker)
    };
    let host = std::sync::Arc::new(host::NextHost::new(pool, locker, root.join("playout")));
    let mut callback = text_subtitle_runtime_tests::callback_listener(host.clone()).await;
    assert_next_session_with_text_mode(
        &host,
        host::NextStart {
            channel_id: 1,
            preview: false,
            number: "1".into(),
            executable: runtime_path("ERSATZRS_NEXT_TEST_BIN"),
            output: root.join("output"),
            ffmpeg,
            ffprobe,
            replacements: Vec::new(),
            idle_timeout: None,
            deadline: Instant::now() + Duration::from_secs(30),
        },
        false,
        convert,
    )
    .await;
    callback.stop().await;
}

async fn reopen_provider_pool(
    pool: SqlitePool,
    version: i64,
    database: sqlx::sqlite::SqliteConnectOptions,
) -> SqlitePool {
    let before = crate::playback::source_indexes::load(&pool, version)
        .await
        .unwrap();
    assert!(before.is_some());
    pool.close().await;
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(database.create_if_missing(false))
        .await
        .unwrap();
    let after = crate::playback::source_indexes::load(&pool, version)
        .await
        .unwrap();
    assert!(
        before == after,
        "source identity map must survive SQLite reopen"
    );
    pool
}

async fn pin_provider_choice(
    pool: &SqlitePool,
    version: i64,
    hdr: bool,
    logical: [i32; 4],
    convert: bool,
) {
    let hint = crate::next_engine::probe_hint::persisted(pool, version)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(hint.video[0].has_hdr10_metadata, None);
    assert_eq!(
        hint.video[0].color_transfer.as_deref(),
        hdr.then_some("smpte2084")
    );
    let mut item = selection::tests::item();
    item.id = 1;
    item.playout_id = 1;
    item.in_point = ersatzrs_time::Duration::zero();
    item.out_point = ersatzrs_time::Duration::seconds(if convert { 55 } else { 60 });
    if convert {
        item.in_point = ersatzrs_time::Duration::seconds(5);
        sqlx::query(r#"UPDATE "Channel" SET "NextEngineTextSubtitleMode"=1 WHERE "Id"=1"#)
            .execute(pool)
            .await
            .unwrap();
    }
    item.pinned_stream_selection = Some(ersatzrs_domain::PinnedStreamSelection {
        media_version_id: version,
        video_stream_index: logical[0],
        audio_stream_index: Some(logical[2]),
        subtitle_stream_index: Some(logical[3]),
    });
    crate::repositories::make_repositories(pool.clone())
        .playout
        .upsert_item(&item)
        .await
        .unwrap();
}

async fn repair_with_ordinary_scans(
    pool: &SqlitePool,
    server: &MockServer,
    clip: &std::path::Path,
    hdr: bool,
    logical: [i32; 4],
    external: &[i32],
) {
    if hdr {
        sqlx::query(r#"UPDATE "MediaStream" SET "Codec"='hevc',"ColorTransfer"='smpte2084' WHERE "MediaVersionId"=100 AND "MediaStreamKind"=1"#)
            .execute(pool).await.unwrap();
    }
    let _items=server.mock_async(|when,then| {
        when.method(GET).path("/Items").query_param("includeItemTypes","Movie");
        then.status(200).json_body(serde_json::json!({"TotalRecordCount":1,"Items":[{
            "Id":"movie-1","Name":"Synthetic","Type":"Movie","Etag":"same","Path":clip,"RunTimeTicks":600_000_000,"LocationType":"FileSystem"
        }]}));
    }).await;
    let mut streams = vec![
        serde_json::json!({"Type":"Video","Index":logical[0],"Codec":if hdr {"hevc"} else {"h264"},"Width":320,"Height":180,
            "PixelFormat":if hdr {"yuv420p10le"} else {"yuv420p"},"RealFrameRate":25.0,
            "ColorTransfer":hdr.then_some("smpte2084"),"ColorSpace":hdr.then_some("bt2020nc"),
            "ColorPrimaries":hdr.then_some("bt2020"),"BitDepth":if hdr {10} else {8}}),
        serde_json::json!({"Type":"Audio","Index":logical[1],"Codec":"aac","Language":"eng","IsDefault":true,"Channels":1}),
        serde_json::json!({"Type":"Audio","Index":logical[2],"Codec":"aac","Language":"nld","Channels":1}),
        serde_json::json!({"Type":"Subtitle","Index":logical[3],"Codec":"subrip","Language":"nld","IsDefault":true}),
        serde_json::json!({"Type":"Audio","Index":-1,"Codec":"aac","Language":"nld","IsExternal":true}),
    ];
    streams.extend(external.iter().map(
        |index| serde_json::json!({"Type":"Audio","Index":index,"Codec":"aac","IsExternal":true}),
    ));
    let playback = server
        .mock_async(|when, then| {
            when.method(GET).path("/Items/movie-1/PlaybackInfo");
            then.status(200).json_body(serde_json::json!({"MediaSources":[{
            "Id":"selected-source","Path":clip,"RunTimeTicks":600_000_000,"MediaStreams":streams
        }]}));
        })
        .await;
    let service = crate::jellyfin::JellyfinService::new();
    let config = ersatzrs_core::repositories::JellyfinConnectionConfig {
        address: server.base_url(),
        authorization_header: "MediaBrowser Token=\"test\"".into(),
    };
    let scanned = crate::jellyfin::sync_jellyfin_movie_library(
        pool,
        &service,
        &config,
        1,
        1,
        "library-1",
        false,
    )
    .await
    .unwrap();
    assert_eq!(
        scanned, 1,
        "the unchanged item is present in the ordinary scan"
    );
    assert_eq!(
        playback.calls_async().await,
        1,
        "row295 admission repairs legacy maps despite equal ETags"
    );
    // Reinject the legacy defect to preserve the earlier migration recovery proof.
    if hdr {
        sqlx::query(r#"UPDATE "MediaStream" SET "Codec"='hevc',"ColorTransfer"='smpte2084' WHERE "MediaVersionId"=100 AND "MediaStreamKind"=1"#).execute(pool).await.unwrap();
    } else {
        sqlx::query(r#"INSERT INTO "MediaStream" ("MediaVersionId","Index","Codec","MediaStreamKind","Channels","Default","Forced") VALUES (100,-1,'aac',2,2,0,0)"#).execute(pool).await.unwrap();
    }
    let repair = if hdr {
        include_str!("../../../../migrations/202610240415_provider_hdr_metadata_rescan.sql")
    } else {
        include_str!("../../../../migrations/202610240341_jellyfin_external_audio_rescan.sql")
    };
    sqlx::raw_sql(repair).execute(pool).await.unwrap();
    let scanned = crate::jellyfin::sync_jellyfin_movie_library(
        pool,
        &service,
        &config,
        1,
        1,
        "library-1",
        false,
    )
    .await
    .unwrap();
    assert_eq!(
        scanned, 1,
        "the repaired item is present in the ordinary scan"
    );
    assert_eq!(playback.calls_async().await, 2);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM "MediaStream" WHERE "Index"<0"#)
            .fetch_one(pool)
            .await
            .unwrap(),
        0
    );
}

async fn hdr_fixture(ffmpeg: &Path, ffprobe: &Path, source: &Path, root: &Path) -> PathBuf {
    let clip = root.join("hdr10.mkv");
    let mut command = Command::new(ffmpeg);
    command.args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(source)
        .args(["-map", "0", "-c", "copy", "-c:v", "libx265", "-preset", "ultrafast",
            "-vf", "format=yuv420p10le,setparams=color_primaries=bt2020:color_trc=smpte2084:colorspace=bt2020nc:range=tv",
            "-pix_fmt", "yuv420p10le", "-color_primaries", "bt2020", "-color_trc", "smpte2084",
            "-colorspace", "bt2020nc", "-color_range", "tv", "-x265-params",
            "pools=2:frame-threads=2:master-display=G(13250,34500)B(7500,3000)R(34000,16000)WP(15635,16450)L(10000000,50):max-cll=1000,400:hdr-opt=1:repeat-headers=1:log-level=error"])
        .arg(&clip);
    tool(command).await;
    let probed = crate::ffprobe::FfprobeExtractor::new(ffprobe.to_str().unwrap())
        .extract(&clip)
        .await
        .unwrap();
    let video = probed
        .streams
        .iter()
        .find(|stream| stream.kind == ersatzrs_domain::MediaStreamKind::Video)
        .unwrap();
    assert!(
        video.mastering_display.is_some(),
        "real frame inspection must find mastering metadata"
    );
    assert_eq!(video.max_cll, Some(1000));
    assert_eq!(video.max_fall, Some(400));
    assert_eq!(video.color_transfer.as_deref(), Some("smpte2084"));
    assert_eq!(video.color_primaries.as_deref(), Some("bt2020"));
    clip
}
