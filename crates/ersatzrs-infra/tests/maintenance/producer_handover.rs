//! Opt-in decoded proof using production HLS producers and the scheduler.

use super::*;
use ersatzrs_core::scheduling::PlayoutPreservation;
use ersatzrs_ffmpeg::OutputFormat;
use ersatzrs_infra::{hls::InMemoryHlsSessionRegistry, hls_worker::HlsSessionWorker};
use futures_util::FutureExt;
use std::{collections::BTreeMap, path::Path, process::Output, time::Duration as StdDuration};
use tokio::{process::Command, sync::RwLock};

#[path = "producer_handover_recovery.rs"]
mod recovery;

#[path = "producer_handover_sources.rs"]
mod sources;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires FFmpeg 9; runs the real native producer for up to four minutes"]
async fn native_producer_preserves_prefetch_and_decodes_new_programme_opening() {
    prove_handover(false, Source::Classic).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires FFmpeg 9 and ERSATZRS_NEXT_TEST_BIN; runs a real Next producer"]
async fn next_producer_preserves_prefetch_and_decodes_new_programme_opening() {
    prove_handover(true, Source::Classic).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires FFmpeg 9; runs the real native producer with Scripted Refresh"]
async fn native_scripted_producer_preserves_prefetch_and_decodes_new_programme_opening() {
    prove_handover(false, Source::Scripted).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires FFmpeg 9 and ERSATZRS_NEXT_TEST_BIN; runs Next with Scripted Refresh"]
async fn next_scripted_producer_preserves_prefetch_and_decodes_new_programme_opening() {
    prove_handover(true, Source::Scripted).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires FFmpeg 9; runs native playback through a Block Refresh"]
async fn native_block_producer_preserves_prefetch_and_decodes_new_programme_opening() {
    prove_handover(false, Source::Block).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires FFmpeg 9 and ERSATZRS_NEXT_TEST_BIN; runs Next with Block Refresh"]
async fn next_block_producer_preserves_prefetch_and_decodes_new_programme_opening() {
    prove_handover(true, Source::Block).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires FFmpeg 9; runs native playback through a YAML Refresh"]
async fn native_yaml_producer_preserves_prefetch_and_decodes_new_programme_opening() {
    prove_handover(false, Source::Yaml).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires FFmpeg 9 and ERSATZRS_NEXT_TEST_BIN; runs Next with YAML Refresh"]
async fn next_yaml_producer_preserves_prefetch_and_decodes_new_programme_opening() {
    prove_handover(true, Source::Yaml).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires FFmpeg 9; runs the real native producer with External JSON Refresh"]
async fn native_external_producer_preserves_prefetch_and_decodes_new_programme_opening() {
    prove_handover(false, Source::External).await;
}

#[derive(Clone, Copy)]
enum Source {
    Classic,
    Scripted,
    Block,
    Yaml,
    External,
}

#[tokio::test]
async fn external_source_failure_can_be_cancelled_before_first_segment() {
    let root = tempfile::tempdir().unwrap();
    let blue = root.path().join("blue.mp4");
    let pool = fixture(&blue, &root.path().join("green.mp4"), false, false).await;
    initialize(&pool, &EntityLocker::new(), &blue, Source::External).await;
    // Reproduce the missing-origin failure before any FFmpeg process starts.
    pool.execute(r#"UPDATE "PlexMediaSource" SET "ServerName"='Unavailable' WHERE "Id"=10"#)
        .await
        .unwrap();
    let registry = Arc::new(InMemoryHlsSessionRegistry::new());
    let folder = root.path().join("hls");
    let worker = HlsSessionWorker::new(
        "1".into(),
        1,
        folder.clone(),
        OutputFormat::Hls,
        Vec::new(),
        Arc::new(RwLock::new(
            ersatzrs_ffmpeg::FfmpegRuntimeReport::for_tests(),
        )),
    );
    registry.insert(worker.clone()).await;
    worker.clone().start(pool, registry.clone(), None);
    let _cleanup = CancelWorker(worker.clone());
    tokio::time::sleep(StdDuration::from_secs(1)).await;
    assert!(registry.try_get("1").await.is_some());
    assert!(!folder.join("live.m3u8").exists());
    worker.cancel().await;
    tokio::time::timeout(StdDuration::from_secs(3), async {
        while registry.try_get("1").await.is_some() {
            tokio::time::sleep(StdDuration::from_millis(25)).await;
        }
    })
    .await
    .expect("failed source does not strand its cancelled producer");
}

async fn prove_handover(next: bool, source: Source) {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::WARN)
        .try_init();
    let runtime = ersatzrs_ffmpeg::check_ffmpeg_runtime().await.unwrap();
    let root = tempfile::tempdir().unwrap();
    let blue = root.path().join("blue.mp4");
    let green = root.path().join("green.mp4");
    generate(&blue, false).await;
    generate(&green, true).await;
    let pool = fixture(&blue, &green, next, matches!(source, Source::Scripted)).await;
    let locker = EntityLocker::new();
    let external = initialize(&pool, &locker, &blue, source).await;
    let registry = Arc::new(InMemoryHlsSessionRegistry::new());
    let folder = root.path().join("hls");
    let mut next_host = None;
    let callback_listener = sources::callback_listener(next);
    let (worker, task): (Arc<dyn HlsSessionHandle>, _) = if next {
        use ersatzrs_infra::next_engine::host::{NextHost, NextStart};
        let host = Arc::new(NextHost::new(
            pool.clone(),
            locker.clone(),
            root.path().join("playout"),
        ));
        host.media
            .configure(callback_listener.as_ref().unwrap().local_addr().unwrap())
            .unwrap();
        let task = host
            .start(
                registry.clone(),
                NextStart {
                    channel_id: 1,
                    preview: false,
                    number: "1".into(),
                    executable: next_binary(),
                    output: folder.clone(),
                    ffmpeg: ersatzrs_ffmpeg::ffmpeg_path().into(),
                    ffprobe: ersatzrs_ffmpeg::ffprobe_path().into(),
                    replacements: Vec::new(),
                    idle_timeout: None,
                    deadline: tokio::time::Instant::now() + StdDuration::from_secs(30),
                },
            )
            .await
            .unwrap();
        next_host = Some(host);
        (registry.try_get("1").await.unwrap(), Some(task))
    } else {
        let worker = HlsSessionWorker::new(
            "1".into(),
            1,
            folder.clone(),
            OutputFormat::Hls,
            Vec::new(),
            Arc::new(RwLock::new(runtime)),
        );
        registry.insert(worker.clone()).await;
        worker.clone().start(pool.clone(), registry.clone(), None);
        (worker as Arc<dyn HlsSessionHandle>, None)
    };
    let _cleanup = CancelWorker(worker.clone());
    let captured = std::panic::AssertUnwindSafe(capture_handover(
        &pool,
        &locker,
        registry.as_ref(),
        worker.as_ref(),
        &folder,
        external.as_deref(),
        next_host.as_ref(),
    ))
    .catch_unwind()
    .await;
    worker.cancel().await;
    tokio::time::timeout(StdDuration::from_secs(10), async {
        while registry.try_get("1").await.is_some() {
            tokio::time::sleep(StdDuration::from_millis(50)).await;
        }
    })
    .await
    .expect("worker reaped and registry released");
    if let Some(task) = task {
        task.await.unwrap();
    }
    sources::assert_no_callbacks(callback_listener);
    let (bytes, retained) = captured.unwrap_or_else(|panic| std::panic::resume_unwind(panic));
    verify_capture(root, bytes, retained, next).await;
}

async fn verify_capture(root: tempfile::TempDir, bytes: Vec<u8>, retained: Duration, next: bool) {
    let capture = root.path().join("handover.ts");
    std::fs::write(&capture, bytes).unwrap();
    eprintln!(
        "producer handover engine: {}",
        if next { "Next" } else { "native" }
    );
    let verified = std::panic::AssertUnwindSafe(verify_decoded_transition(&capture, retained))
        .catch_unwind()
        .await;
    if let Err(panic) = verified {
        eprintln!(
            "retained synthetic handover fixture: {}",
            root.keep().display()
        );
        std::panic::resume_unwind(panic);
    }
}

async fn initialize(
    pool: &SqlitePool,
    locker: &EntityLocker,
    blue: &Path,
    source: Source,
) -> Option<std::path::PathBuf> {
    let start = sources::configure(pool, source, Utc::now() - Duration::seconds(5)).await;
    let mut request = PlayoutBuildRequest::new(88, PlayoutBuildMode::Reset);
    let external = if matches!(source, Source::External) {
        // The external wire format identifies the originating Plex server,
        // even when its media file is available locally and no network is used.
        pool.execute(
            r#"
            INSERT INTO "MediaSource" ("Id") VALUES (10);
            INSERT INTO "PlexMediaSource" ("Id", "ServerName", "ClientIdentifier")
                VALUES (10, 'External Fixture', 'external-fixture');
            INSERT INTO "Library" ("Id", "MediaSourceId", "MediaKind") VALUES (10, 10, 1);
            INSERT INTO "PlexLibrary" ("Id", "ShouldSyncItems") VALUES (10, 1);
            INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path") VALUES (10, 10, 'fixture');
        "#,
        )
        .await
        .unwrap();
        let path = blue.with_file_name("schedule.json");
        let document = serde_json::json!({
            "startTime": start.to_rfc3339(),
            "guideMinimumDurationSeconds": 5,
            "programs": (0..12).map(|index| serde_json::json!({
                "title": format!("External programme {index}"),
                "duration": 60_000,
                "file": blue.to_str().unwrap(),
                "serverKey": "External Fixture",
                "type": "movie"
            })).collect::<Vec<_>>()
        });
        std::fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
        sqlx::query(r#"UPDATE "Playout" SET "ScheduleKind"=20, "ProgramScheduleId"=NULL, "ScheduleFile"=? WHERE "Id"=88"#)
            .bind(path.to_str().unwrap()).execute(pool).await.unwrap();
        request = request.with_preservation(PlayoutPreservation::ReplaceCurrent);
        Some(path)
    } else {
        None
    };
    run_requested_playout_build(pool, locker, request, start, None)
        .await
        .unwrap();
    external
}

#[allow(clippy::disallowed_methods)] // Explicit opt-in test binary, never production configuration.
fn next_binary() -> std::path::PathBuf {
    std::env::var_os("ERSATZRS_NEXT_TEST_BIN")
        .expect("set ERSATZRS_NEXT_TEST_BIN")
        .into()
}

async fn capture_handover(
    pool: &SqlitePool,
    locker: &EntityLocker,
    registry: &InMemoryHlsSessionRegistry,
    worker: &dyn HlsSessionHandle,
    folder: &Path,
    external: Option<&Path>,
    next: Option<&Arc<ersatzrs_infra::next_engine::host::NextHost>>,
) -> (Vec<u8>, Duration) {
    let mut segments = BTreeMap::new();
    wait_for_segments(folder, worker, &mut segments, 1).await;
    let identity = worker.session_model().unwrap().started_at;
    // Next constructs the session before projecting its initial manifest.
    // Use the first segment's broadcast clock, excluding that preparation time.
    let playback_start = if worker.session_model().unwrap().state == "next" {
        let playlist = tokio::fs::read_to_string(folder.join("live.m3u8"))
            .await
            .unwrap();
        let value = playlist
            .lines()
            .find_map(|line| line.strip_prefix("#EXT-X-PROGRAM-DATE-TIME:"))
            .expect("Next publishes the first segment's broadcast clock");
        chrono::DateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f%z")
            .unwrap()
            .with_timezone(&Utc)
    } else {
        identity
    };
    replace_source(pool, external).await;
    if let Some(host) = next {
        recovery::cancel_once(pool, locker, registry, host, folder).await;
    }
    ersatzrs_infra::maintenance::run_requested_playout_build_with_execution(
        pool,
        locker,
        PlayoutBuildRequest::new(88, PlayoutBuildMode::Refresh)
            .with_preservation(PlayoutPreservation::KeepCurrent),
        Utc::now(),
        ersatzrs_infra::maintenance::PlayoutExecution {
            scripted_teletext: None,
            hls_sessions: Some(registry),
            next,
            cancelled: None,
        },
    )
    .await
    .unwrap();
    let seam = replacement_boundary(pool, external).await;
    assert!(seam > Utc::now() + Duration::seconds(120));
    let deadline = tokio::time::Instant::now() + StdDuration::from_mins(4);
    loop {
        collect_segments(folder, &mut segments).await;
        worker.touch();
        assert_eq!(worker.session_model().unwrap().started_at, identity);
        if worker
            .session_model()
            .unwrap()
            .transcoded_until
            .unwrap_or_else(Utc::now)
            >= seam + Duration::seconds(10)
        {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "producer did not cross the retained boundary"
        );
        assert!(
            registry.try_get("1").await.is_some(),
            "producer ended during handover"
        );
        tokio::time::sleep(StdDuration::from_millis(250)).await;
    }
    collect_segments(folder, &mut segments).await;
    let bytes: Vec<_> = segments
        .values()
        .flat_map(|bytes| bytes.iter().copied())
        .collect();
    (bytes, seam - playback_start)
}

async fn replace_source(pool: &SqlitePool, external: Option<&Path>) {
    if let Some(path) = external {
        let mut document: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        for programme in document["programs"].as_array_mut().unwrap() {
            programme["file"] = path.with_file_name("green.mp4").to_str().unwrap().into();
        }
        // Change the actual author document. The worker must retain the old
        // accepted prefix without moving any of these absolute programme times.
        std::fs::write(path, serde_json::to_vec(&document).unwrap()).unwrap();
    } else {
        // Keep the old media available for retained rows.
        pool.execute("UPDATE CollectionItem SET MediaItemId=101 WHERE CollectionId=1")
            .await
            .unwrap();
    }
}

async fn replacement_boundary(pool: &SqlitePool, external: Option<&Path>) -> chrono::DateTime<Utc> {
    if let Some(path) = external {
        let accepted = ersatzrs_infra::external_json::snapshot::accepted(pool, 88)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(accepted.revision, 2);
        let green = path.with_file_name("green.mp4");
        let seam = accepted
            .schedule
            .items
            .iter()
            .find(|item| item.program.file.as_deref() == green.to_str())
            .expect("accepted future uses the replacement")
            .start;
        let authored = ersatzrs_infra::external_json::load(path)
            .await
            .unwrap()
            .unwrap();
        for (old, new) in accepted.schedule.items.iter().zip(&authored.items) {
            assert_eq!((old.start, old.finish), (new.start, new.finish));
        }
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                r#"SELECT COUNT(*) FROM "PlayoutItem" WHERE "PlayoutId"=88"#
            )
            .fetch_one(pool)
            .await
            .unwrap(),
            0,
            "external proof must not use materialized rows"
        );
        seam
    } else {
        sqlx::query_scalar(
            "SELECT MIN(Start) FROM PlayoutItem WHERE PlayoutId=88 AND MediaItemId=101",
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }
}

struct CancelWorker(Arc<dyn HlsSessionHandle>);
impl Drop for CancelWorker {
    fn drop(&mut self) {
        let worker = self.0.clone();
        tokio::spawn(async move {
            worker.cancel().await;
        });
    }
}

async fn ffmpeg(command: &mut Command) -> Output {
    let output = tokio::time::timeout(
        StdDuration::from_secs(30),
        command.kill_on_drop(true).output(),
    )
    .await
    .expect("bounded FFmpeg command")
    .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

async fn generate(path: &Path, replacement: bool) {
    let video = if replacement {
        "color=c=lime:s=320x180:r=25,drawbox=color=white:t=fill:enable='lt(t,2)'"
    } else {
        "color=c=blue:s=320x180:r=25"
    };
    ffmpeg(
        Command::new(ersatzrs_ffmpeg::ffmpeg_path())
            .args([
                "-v",
                "error",
                "-nostdin",
                "-y",
                "-f",
                "lavfi",
                "-i",
                video,
                "-f",
                "lavfi",
                "-i",
                if replacement {
                    "sine=frequency=880:sample_rate=48000"
                } else {
                    "sine=frequency=440:sample_rate=48000"
                },
                "-t",
                "60",
                "-c:v",
                "mpeg4",
                "-threads",
                "2",
                "-c:a",
                "aac",
                "-ac",
                "2",
            ])
            .arg(path),
    )
    .await;
}

async fn fixture(blue: &Path, green: &Path, next: bool, scripted: bool) -> SqlitePool {
    // Candidate publication keeps a writer while the producer/projector reads
    // accepted state. Use the production WAL pool, not the single-connection
    // in-memory helper, so the fixture exercises that concurrency contract.
    let url = format!("sqlite://{}", blue.with_file_name("handover.db").display());
    let pool = ersatzrs_infra::db::init(&url).await.unwrap();
    pool.execute("INSERT OR IGNORE INTO Resolution (Id, Name, Width, Height) VALUES (1, '1920x1080', 1920, 1080)")
        .await
        .unwrap();
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 1).await;
    seed_refresh_rebuild_fixture(&pool, &repos).await;
    pool.execute(r"
        UPDATE Resolution SET Width=320, Height=180 WHERE Id=1;
        UPDATE FFmpegProfile SET VideoFormat=1, AudioFormat=1, AudioChannels=2,
            VideoBitrate=1000, VideoBufferSize=2000, HardwareAcceleration=0,
            BitDepth=0, ThreadCount=2 WHERE Id=1;
        UPDATE Channel SET StreamingMode=4, IdleBehavior=0, PlayoutMode=0 WHERE Id=1;
        UPDATE MediaVersion SET Duration='00:01:00', Width=320, Height=180,
            RFrameRate='25/1', VideoScanKind=1 WHERE Id=1000;
        INSERT INTO MediaItem (Id, LibraryPathId, State) VALUES (101, 1, 0);
        INSERT INTO Movie (Id) VALUES (101);
        INSERT INTO MovieMetadata (Id, MovieId, MetadataKind, Title, SortTitle, DateAdded, DateUpdated)
            SELECT 101, 101, MetadataKind, 'Replacement', 'Replacement', DateAdded, DateUpdated
            FROM MovieMetadata WHERE Id=100;
        INSERT INTO MediaVersion (Id, MovieId, Name, Duration, VideoScanKind, DateAdded, DateUpdated, Width, Height, RFrameRate)
            SELECT 1001, 101, Name, Duration, VideoScanKind, DateAdded, DateUpdated, Width, Height, RFrameRate
            FROM MediaVersion WHERE Id=1000;
        INSERT INTO ConfigElement (Key, Value) VALUES ('ffmpeg.segmenter.initial_segment_count', '1');
    ").await.unwrap();
    for (id, path) in [(1000, blue), (1001, green)] {
        sqlx::query(
            "INSERT INTO MediaFile (Id, MediaVersionId, Path, PathHash) VALUES (?, ?, ?, ?)",
        )
        .bind(id)
        .bind(id)
        .bind(path.to_str().unwrap())
        .bind(id.to_string())
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(r#"INSERT INTO MediaStream ("Index", Codec, MediaStreamKind, Channels, "Default", Forced, MediaVersionId, PixelFormat)
            VALUES (0, 'mpeg4', 1, 0, 1, 0, ?, 'yuv420p'), (1, 'aac', 2, 2, 1, 0, ?, NULL)"#)
            .bind(id).bind(id).execute(&pool).await.unwrap();
    }
    if scripted {
        let script = blue.with_file_name("schedule.lua");
        std::fs::write(&script, "add_collection('source', 'Refresh Collection', 'chronological') while not is_done() do add_count('source', 1) end").unwrap();
        sqlx::query(
            "UPDATE Playout SET ScheduleKind=4, ProgramScheduleId=NULL, ScheduleFile=? WHERE Id=88",
        )
        .bind(format!("\"{}\"", script.display()))
        .execute(&pool)
        .await
        .unwrap();
    }
    if next {
        pool.execute("UPDATE Channel SET StreamingEngine=1 WHERE Id=1")
            .await
            .unwrap();
    }
    pool
}

async fn collect_segments(folder: &Path, segments: &mut BTreeMap<String, Vec<u8>>) {
    let Ok(playlist) = tokio::fs::read_to_string(folder.join("live.m3u8")).await else {
        return;
    };
    for name in playlist
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        if !segments.contains_key(name) {
            let bytes = tokio::fs::read(folder.join(name)).await.unwrap();
            segments.insert(name.to_owned(), bytes);
        }
    }
}

async fn wait_for_segments(
    folder: &Path,
    worker: &dyn HlsSessionHandle,
    segments: &mut BTreeMap<String, Vec<u8>>,
    count: usize,
) {
    tokio::time::timeout(StdDuration::from_secs(30), async {
        while segments.len() < count {
            worker.touch();
            assert!(
                !matches!(
                    worker.startup_readiness(),
                    ersatzrs_core::hls::HlsStartupReadiness::Failed
                ),
                "producer failed during startup; inspect captured runtime warnings"
            );
            collect_segments(folder, segments).await;
            tokio::time::sleep(StdDuration::from_millis(100)).await;
        }
    })
    .await
    .expect("real producer startup");
}

async fn verify_decoded_transition(capture: &Path, retained: Duration) {
    let output = ffmpeg(
        Command::new(ersatzrs_ffmpeg::ffmpeg_path())
            .args(["-v", "error", "-nostdin", "-i"])
            .arg(capture)
            .args(["-t", &(retained.num_seconds() + 10).to_string()])
            .args([
                "-map",
                "0:v:0",
                "-vf",
                "fps=10,scale=1:1",
                "-pix_fmt",
                "rgb24",
                "-f",
                "rawvideo",
                "-",
            ]),
    )
    .await;
    assert!(
        output.stderr.is_empty(),
        "decode errors: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let colours: Vec<_> = output
        .stdout
        .chunks_exact(3)
        .map(|p| {
            if p[2] > 200 && p[0] < 30 && p[1] < 30 {
                'B'
            } else if p.iter().all(|v| *v > 200) {
                'W'
            } else if p[1] > 200 && p[0] < 30 && p[2] < 30 {
                'G'
            } else {
                panic!("unexpected slate or frame colour: {p:?}");
            }
        })
        .collect();
    let white = colours
        .iter()
        .position(|c| *c == 'W')
        .expect("new programme opening");
    let green = colours
        .iter()
        .position(|c| *c == 'G')
        .expect("new programme body");
    assert!(
        white > 1200,
        "retained programmes were cut: {white} samples"
    );
    let observed_ms = i64::try_from(white).unwrap() * 100;
    assert!(
        (observed_ms - retained.num_milliseconds()).abs() < 1000,
        "retained programme duration changed: observed {observed_ms}ms, expected {}ms",
        retained.num_milliseconds()
    );
    assert!(
        (18..=22).contains(&(green - white)),
        "two-second opening was lost or repeated"
    );
    assert!(colours[..white].iter().all(|c| *c == 'B'));
    assert!(colours[white..green].iter().all(|c| *c == 'W'));
    assert!(colours[green..].iter().all(|c| *c == 'G'));
    assert!(colours.len() - green >= 50, "replacement body too short");
    eprintln!("producer handover decoded: {} retained samples, {} opening samples, {} new-body samples at 10 fps", white, green - white, colours.len() - green);
    verify_audio(capture).await;
}

async fn verify_audio(capture: &Path) {
    // Decode the actual first/last PCM windows. Input-side MPEG-TS seeking is
    // not an oracle for the last broadcast programme after stream transitions.
    // Give the raw PCM output its sample clock: concatenated HLS segments can
    // reset transport timestamps. This checks decoded content, not TS timing.
    let output = ffmpeg(
        Command::new(ersatzrs_ffmpeg::ffmpeg_path())
            .args(["-v", "error", "-nostdin", "-i"])
            .arg(capture)
            .args([
                "-map",
                "0:a:0",
                "-ac",
                "1",
                "-ar",
                "8000",
                "-af",
                "asetpts=N/SR/TB",
                "-f",
                "s16le",
                "-",
            ]),
    )
    .await;
    assert!(
        output.stderr.is_empty(),
        "audio decode errors: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let samples: Vec<_> = output
        .stdout
        .chunks_exact(2)
        .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]))
        .collect();
    assert!(samples.len() >= 16000, "audio capture is too short");
    for (samples, expected) in [
        (&samples[..8000], 440.0),
        (&samples[samples.len() - 8000..], 880.0),
    ] {
        let crossings = samples
            .windows(2)
            .filter(|pair| pair[0] <= 0 && pair[1] > 0)
            .count();
        let hz = f64::from(u32::try_from(crossings).unwrap()) * 8000.0
            / f64::from(u32::try_from(samples.len()).unwrap());
        assert!(
            (hz - expected).abs() < 40.0,
            "wrong programme audio: {hz}Hz instead of {expected}Hz"
        );
        eprintln!("producer handover decoded audio: {hz:.1}Hz (expected {expected:.0}Hz)");
    }
}
