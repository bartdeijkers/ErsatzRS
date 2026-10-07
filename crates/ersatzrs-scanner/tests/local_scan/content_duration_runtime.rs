//! Opt-in scan-to-playback proof using the published Next and `FFmpeg` packages.
//! Both cases require `ERSATZRS_NEXT_TEST_FFMPEG`, `ERSATZRS_NEXT_TEST_FFPROBE`,
//! and matching normal `FFMPEG_PATH`/`FFPROBE_PATH`. The combined native/Next
//! case additionally requires `ERSATZRS_NEXT_TEST_BIN`; native video-only does not.
//! Optional `ERSATZRS_ROW194_PROOF_DIR` exports synthetic evidence.

use super::*;
use chrono::{Duration, Utc};
use ersatzrs_core::{
    hls::{HlsAdmissionStatus, HlsSessionHandle, HlsSessionRegistry},
    locking::EntityLocker,
    repositories::{ChannelRepository, FfmpegProfileRepository},
    scheduling::{PlayoutBuildMode, PlayoutBuildRequest},
};
use ersatzrs_domain::{
    Channel, FFmpegProfile, FFmpegProfileAudioFormat, FFmpegProfileVideoFormat,
    NextEngineTextSubtitleMode, StreamingMode,
};
use ersatzrs_ffmpeg::OutputFormat;
use ersatzrs_infra::{
    hls::InMemoryHlsSessionRegistry,
    hls_worker::HlsSessionWorker,
    maintenance::run_requested_playout_build,
    next_engine::{
        config, host::NextHost, process::NextProcess, protocol, schedule,
        troubleshooting::TroubleshootingPreview,
    },
    playback::load_playback_profile,
    repositories::make_repositories,
    transcode_capacity::active_transcode_snapshot,
};
use futures_util::FutureExt;
use serde_json::{json, Value};
use std::{panic::AssertUnwindSafe, sync::Arc, time::Duration as StdDuration};
use tokio::{io::AsyncReadExt, process::Command, sync::RwLock, time::timeout};

struct Tools {
    ffmpeg: PathBuf,
    ffprobe: PathBuf,
    next: Option<PathBuf>,
}

#[expect(
    clippy::disallowed_methods,
    reason = "Explicit opt-in test tools and evidence output, never production configuration"
)]
fn required_path(name: &str) -> PathBuf {
    let path = env::var_os(name).map(PathBuf::from).expect(name);
    assert!(path.is_file(), "required runtime input {name} must exist");
    path
}

impl Tools {
    fn load() -> Self {
        let mut tools = Self::native();
        tools.next = Some(required_path("ERSATZRS_NEXT_TEST_BIN"));
        tools
    }

    fn native() -> Self {
        let tools = Self {
            ffmpeg: required_path("ERSATZRS_NEXT_TEST_FFMPEG"),
            ffprobe: required_path("ERSATZRS_NEXT_TEST_FFPROBE"),
            next: None,
        };
        assert_eq!(PathBuf::from(ersatzrs_ffmpeg::ffmpeg_path()), tools.ffmpeg);
        assert_eq!(
            PathBuf::from(ersatzrs_ffmpeg::ffprobe_path()),
            tools.ffprobe
        );
        tools
    }
}

async fn tool(command: &mut Command) -> Vec<u8> {
    command
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = command.spawn().unwrap();
    let mut stdout = child.stdout.take().unwrap().take(4 * 1024 * 1024 + 1);
    let mut stderr = child.stderr.take().unwrap().take(128 * 1024 + 1);
    let stdout = tokio::spawn(async move {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).await.unwrap();
        bytes
    });
    let stderr = tokio::spawn(async move {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).await.unwrap();
        bytes
    });
    let waited = timeout(StdDuration::from_secs(30), child.wait()).await;
    if !matches!(&waited, Ok(Ok(_))) {
        child.start_kill().unwrap();
        timeout(StdDuration::from_secs(5), child.wait())
            .await
            .expect("timed-out fixture tool must be reaped")
            .unwrap();
    }
    let bytes = stdout.await.unwrap();
    let errors = stderr.await.unwrap();
    let status = waited.expect("synthetic fixture tool timed out").unwrap();
    assert!(
        bytes.len() <= 4 * 1024 * 1024 && errors.len() <= 128 * 1024,
        "fixture tool output exceeded its bound"
    );
    assert!(status.success(), "{}", String::from_utf8_lossy(&errors));
    bytes
}

async fn generate(tools: &Tools, library: &Path) {
    tokio::fs::create_dir_all(library).await.unwrap();
    let subtitle = library.with_file_name("duration.nl.srt");
    tokio::fs::write(
        &subtitle,
        "1\n00:00:00,000 --> 00:00:20,000\nDutch duration proof\n",
    )
    .await
    .unwrap();
    tool(
        Command::new(&tools.ffmpeg)
            .args([
                "-v",
                "error",
                "-nostdin",
                "-f",
                "lavfi",
                "-i",
                "color=blue:s=64x64:r=2:d=1",
                "-f",
                "lavfi",
                "-i",
                "anullsrc=r=48000:cl=stereo:d=1",
                "-i",
            ])
            .arg(&subtitle)
            .args([
                "-map",
                "0:v",
                "-map",
                "1:a",
                "-map",
                "2:s",
                "-c:v",
                "libx264",
                "-threads",
                "1",
                "-c:a",
                "pcm_s16le",
                "-c:s",
                "srt",
                "-metadata:s:s:0",
                "language=nld",
                "-metadata:s:s:0",
                "title=Dutch",
            ])
            .arg(library.join("blue.mkv")),
    )
    .await;
    tool(
        Command::new(&tools.ffmpeg)
            .args([
                "-v",
                "error",
                "-nostdin",
                "-f",
                "lavfi",
                "-i",
                "color=red:s=64x64:r=25:d=5",
                "-f",
                "lavfi",
                "-i",
                "anullsrc=r=48000:cl=stereo:d=5",
                "-map",
                "0:v",
                "-map",
                "1:a",
                "-c:v",
                "libx264",
                "-threads",
                "1",
                "-c:a",
                "pcm_s16le",
            ])
            .arg(library.join("red.mkv")),
    )
    .await;
}

async fn generate_video_only(tools: &Tools, root: &Path) {
    let library = root.join("library");
    tokio::fs::create_dir(&library).await.unwrap();
    let subtitle = root.join("duration.nl.srt");
    tokio::fs::write(
        &subtitle,
        "1\n00:00:00,000 --> 00:00:20,000\nDutch video-only proof\n",
    )
    .await
    .unwrap();
    tool(
        Command::new(&tools.ffmpeg)
            .args([
                "-v",
                "error",
                "-nostdin",
                "-f",
                "lavfi",
                "-i",
                "color=blue:s=64x64:r=2:d=1",
                "-i",
            ])
            .arg(&subtitle)
            .args([
                "-map",
                "0:v",
                "-map",
                "1:s",
                "-c:v",
                "libx264",
                "-threads",
                "1",
                "-c:s",
                "srt",
                "-metadata:s:s:0",
                "language=nld",
                "-metadata:s:s:0",
                "title=Dutch",
            ])
            .arg(library.join("blue.mkv")),
    )
    .await;
    tool(
        Command::new(&tools.ffmpeg)
            .args([
                "-v",
                "error",
                "-nostdin",
                "-f",
                "lavfi",
                "-i",
                "color=red:s=64x64:r=25:d=5",
                "-map",
                "0:v",
                "-c:v",
                "libx264",
                "-threads",
                "1",
            ])
            .arg(library.join("red.mkv")),
    )
    .await;
    let red = tool(
        Command::new(&tools.ffprobe)
            .args(["-v", "error", "-show_streams", "-of", "json"])
            .arg(library.join("red.mkv")),
    )
    .await;
    tokio::fs::write(root.join("red-input-probe.json"), &red)
        .await
        .unwrap();
    let red: Value = serde_json::from_slice(&red).unwrap();
    let streams = red["streams"].as_array().unwrap();
    assert_eq!(
        streams.len(),
        1,
        "second programme must be genuine video-only media"
    );
    assert_eq!(streams[0]["codec_type"], "video");
}

async fn prove_conflict(tools: &Tools, root: &Path, has_audio: bool) {
    let bytes = tool(
        Command::new(&tools.ffprobe)
            .args([
                "-v",
                "error",
                "-show_streams",
                "-show_format",
                "-show_packets",
                "-of",
                "json",
            ])
            .arg(root.join("library/blue.mkv")),
    )
    .await;
    let probe: Value = serde_json::from_slice(&bytes).unwrap();
    tokio::fs::write(root.join("input-probe.json"), &bytes)
        .await
        .unwrap();
    let video = probe["streams"]
        .as_array()
        .unwrap()
        .iter()
        .find(|stream| stream["codec_type"] == "video")
        .unwrap();
    eprintln!(
        "input conflict: container={}, video numeric={}, video tag={}",
        probe["format"]["duration"], video["duration"], video["tags"]["DURATION"]
    );
    assert!(
        (probe["format"]["duration"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap()
            - 20.0)
            .abs()
            < 1e-9
    );
    assert!((video["duration"].as_str().unwrap().parse::<f64>().unwrap() - 20.0).abs() < 1e-9);
    assert_eq!(video["tags"]["DURATION"], "00:00:01.000000000");
    let packets: Vec<_> = probe["packets"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|packet| packet["codec_type"] == "video")
        .collect();
    assert_eq!(packets.len(), 2);
    let seconds: f64 = packets
        .iter()
        .map(|packet| {
            packet["duration_time"]
                .as_str()
                .unwrap()
                .parse::<f64>()
                .unwrap()
        })
        .sum();
    assert!(
        (seconds - 1.0).abs() < 1e-9,
        "real video packets end at one second"
    );
    for (packet, expected) in packets.iter().zip([0.0, 0.5]) {
        let pts = packet["pts_time"].as_str().unwrap().parse::<f64>().unwrap();
        assert!(
            (pts - expected).abs() < 1e-9,
            "input packet clock must start at0 and end at1s"
        );
    }
    assert!(probe["streams"]
        .as_array()
        .unwrap()
        .iter()
        .any(|stream| stream["codec_type"] == "subtitle" && stream["tags"]["language"] == "nld"));
    let streams = probe["streams"].as_array().unwrap();
    if has_audio {
        assert!(
            streams
                .iter()
                .any(|stream| stream["codec_type"] == "audio" && stream["channels"] == 2),
            "duration fixture must retain normal stereo audio"
        );
    } else {
        assert!(
            !streams.iter().any(|stream| stream["codec_type"] == "audio"),
            "video-only fixture must have no manufactured source audio"
        );
        assert!(
            streams
                .iter()
                .any(|stream| stream["codec_type"] == "subtitle" && stream["index"] == 1),
            "subtitle index1 must never be mistaken for source audio"
        );
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Scanned {
    item: i64,
    version: i64,
    file: i64,
    duration: String,
}

async fn scanned(pool: &SqlitePool, path: &Path) -> Scanned {
    let row = sqlx::query(
        r#"SELECT v."OtherVideoId", v."Id" AS "VersionId", f."Id" AS "FileId", v."Duration"
        FROM "MediaVersion" v JOIN "MediaFile" f ON f."MediaVersionId"=v."Id" WHERE f."Path"=?"#,
    )
    .bind(path.to_str().unwrap())
    .fetch_one(pool)
    .await
    .unwrap();
    Scanned {
        item: row.get("OtherVideoId"),
        version: row.get("VersionId"),
        file: row.get("FileId"),
        duration: row.get("Duration"),
    }
}

async fn scan_and_rescan(tools: &Tools, root: &Path, pool: &SqlitePool) -> [Scanned; 2] {
    let library = root.join("library");
    seed_local_library(pool, &library, 4).await;
    let config = LocalScanConfig {
        library_id: 1,
        ffprobe_path: tools.ffprobe.to_str().unwrap().to_owned(),
    };
    assert_eq!(
        scan_local_library(pool, &config)
            .await
            .unwrap()
            .scanned_files,
        2
    );
    let first = [
        scanned(pool, &library.join("blue.mkv")).await,
        scanned(pool, &library.join("red.mkv")).await,
    ];
    eprintln!("first persisted scan: {first:?}");
    assert_eq!(first[0].duration, "00:00:01");
    assert_eq!(first[1].duration, "00:00:05");
    assert_eq!(
        scan_local_library(pool, &config)
            .await
            .unwrap()
            .scanned_files,
        2
    );
    let again = [
        scanned(pool, &library.join("blue.mkv")).await,
        scanned(pool, &library.join("red.mkv")).await,
    ];
    eprintln!("rescanned identities: {again:?}");
    assert_eq!(
        again, first,
        "rescan must preserve scanned IDs and content duration"
    );
    let languages: Vec<(i32, Option<String>)> = sqlx::query_as(
        r#"SELECT "MediaStreamKind", "Language" FROM "MediaStream" WHERE "MediaVersionId"=?"#,
    )
    .bind(first[0].version)
    .fetch_all(pool)
    .await
    .unwrap();
    assert!(languages
        .iter()
        .any(|(kind, language)| *kind == 3 && language.as_deref() == Some("nld")));
    tokio::fs::write(root.join("scanned-identities.json"), serde_json::to_vec_pretty(&json!({"blue":{"item":first[0].item,"version":first[0].version,"file":first[0].file,"duration":first[0].duration},"red":{"item":first[1].item,"version":first[1].version,"file":first[1].file,"duration":first[1].duration},"subtitle_language":"nld","rescan_preserved":true})).unwrap()).await.unwrap();
    first
}

async fn seed_schedule(pool: &SqlitePool, root: &Path, scanned: &[Scanned; 2]) {
    let repos = make_repositories(pool.clone());
    pool.execute(r#"INSERT INTO "Resolution" ("Id","Name","Width","Height") VALUES (1,'duration proof',64,64)"#).await.unwrap();
    repos
        .ffmpeg_profile
        .upsert(&FFmpegProfile {
            id: 1,
            name: "duration proof".into(),
            resolution_id: 1,
            video_format: FFmpegProfileVideoFormat::H264,
            audio_format: FFmpegProfileAudioFormat::Aac,
            transcode_video: true,
            transcode_audio: true,
            video_preset: Some("ultrafast".into()),
            thread_count: 1,
            audio_channels: 2,
            audio_sample_rate: 48,
            ..Default::default()
        })
        .await
        .unwrap();
    repos
        .channel
        .upsert(&Channel {
            id: 10,
            number: "10".into(),
            name: "duration proof".into(),
            ffmpeg_profile_id: 1,
            streaming_mode: StreamingMode::HttpLiveStreamingSegmenter,
            ..Default::default()
        })
        .await
        .unwrap();
    for (index, item) in scanned.iter().enumerate() {
        let id = i64::try_from(index + 1).unwrap();
        sqlx::query(
            r#"INSERT INTO "Collection" ("Id","Name","UseCustomPlaybackOrder") VALUES (?,?,0)"#,
        )
        .bind(id)
        .bind(if index == 0 { "Blue" } else { "Red" })
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(r#"INSERT INTO "CollectionItem" ("CollectionId","MediaItemId") VALUES (?,?)"#)
            .bind(id)
            .bind(item.item)
            .execute(pool)
            .await
            .unwrap();
    }
    let script = root.join("schedule.lua");
    // Bounded repetition supplies native workahead; all durations come from the scan.
    tokio::fs::write(&script, "add_collection('blue', 'Blue')\nadd_collection('red', 'Red')\nfor n=1,12 do add_count('blue',1) add_count('red',1) end\n").await.unwrap();
    sqlx::query(r#"INSERT INTO "Playout" ("Id","ChannelId","ScheduleKind","Seed","ScheduleFile") VALUES (10,10,4,42,?)"#).bind(script.to_str().unwrap()).execute(pool).await.unwrap();
}

async fn build(
    pool: &SqlitePool,
    locker: &EntityLocker,
    scanned: &[Scanned; 2],
) -> Vec<schedule::ScheduledMedia> {
    let start = Utc::now();
    run_requested_playout_build(
        pool,
        locker,
        PlayoutBuildRequest::new(10, PlayoutBuildMode::Reset),
        start,
        None,
    )
    .await
    .unwrap();
    let channel = make_repositories(pool.clone())
        .channel
        .get_with_profile(10)
        .await
        .unwrap();
    let timeline =
        schedule::read_locked(pool, locker, &channel, start, start + Duration::minutes(2))
            .await
            .unwrap()
            .unwrap();
    assert!(timeline.items.len() >= 2);
    let first = &timeline.items[0].item;
    let second = &timeline.items[1].item;
    eprintln!(
        "normal scheduled blue={}–{}, in={:?}, out={:?}; red={}–{}",
        first.start, first.finish, first.in_point, first.out_point, second.start, second.finish
    );
    assert_eq!(first.media_item_id, scanned[0].item);
    assert_eq!(first.in_point, Duration::zero());
    assert_eq!(first.out_point, Duration::seconds(1));
    assert_eq!(first.finish - first.start, Duration::seconds(1));
    assert_eq!(second.media_item_id, scanned[1].item);
    assert_eq!(second.start, first.finish);
    assert_eq!(second.finish - second.start, Duration::seconds(5));
    timeline.items.clone()
}

fn playlist_seconds(body: &str) -> f64 {
    body.lines()
        .filter_map(|line| line.strip_prefix("#EXTINF:"))
        .map(|line| line.split(',').next().unwrap().parse::<f64>().unwrap())
        .sum()
}

async fn snapshot_native(folder: &Path, capture: &Path, worker: &HlsSessionWorker) {
    tokio::fs::create_dir(capture).await.unwrap();
    timeout(StdDuration::from_secs(40), async {
        loop {
            worker.touch();
            if let Some(body) = worker.trim_playlist().await {
                if playlist_seconds(&body) >= 2.0 {
                    // Preserve the producer's evidence even when its error slate
                    // correctly fails the decoded handoff acceptance below.
                    tokio::fs::write(capture.join("live.m3u8"), &body)
                        .await
                        .unwrap();
                    let mut complete = true;
                    let mut at = None;
                    for name in body.lines() {
                        if let Some(value) = name.strip_prefix("#EXT-X-PROGRAM-DATE-TIME:") {
                            at = Some(
                                chrono::DateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f%z")
                                    .unwrap()
                                    .timestamp_millis(),
                            );
                        }
                        if name.starts_with('#') || name.is_empty() {
                            continue;
                        }
                        assert!(!name.contains(['/', '\\']));
                        let Some(output) = worker
                            .segment_output(
                                name,
                                at.expect("owned generated segment programme clock"),
                            )
                            .await
                        else {
                            complete = false;
                            break;
                        };
                        if tokio::fs::copy(folder.join(name), capture.join(name))
                            .await
                            .is_err()
                        {
                            complete = false;
                            break;
                        }
                        assert!(
                            output.error.is_none(),
                            "error slate cannot count as handoff: {:?}",
                            output.error
                        );
                    }
                    if complete {
                        tokio::fs::write(capture.join("live.m3u8"), body)
                            .await
                            .unwrap();
                        break;
                    }
                }
            }
            tokio::time::sleep(StdDuration::from_millis(25)).await;
        }
    })
    .await
    .expect("native generated playlist must contain both programmes");
}

async fn native(
    pool: &SqlitePool,
    locker: &EntityLocker,
    tools: &Tools,
    root: &Path,
    scanned: &[Scanned; 2],
) {
    let runtime = ersatzrs_ffmpeg::check_ffmpeg_runtime().await.unwrap();
    let registry = Arc::new(InMemoryHlsSessionRegistry::new());
    let folder = root.join("native-output");
    let worker = HlsSessionWorker::new(
        "10".into(),
        10,
        folder.clone(),
        OutputFormat::Hls,
        Vec::new(),
        Arc::new(RwLock::new(runtime)),
    );
    let capacity_before = active_transcode_snapshot(1).active;
    let timeline = build(pool, locker, scanned).await;
    registry.insert(worker.clone()).await;
    worker.clone().start(pool.clone(), registry.clone(), None);
    let proof = AssertUnwindSafe(snapshot_native(
        &folder,
        &root.join("native-capture"),
        &worker,
    ))
    .catch_unwind()
    .await;
    let started = worker.session_model().map(|model| model.started_at);
    worker.cancel().await;
    timeout(StdDuration::from_secs(10), async {
        while registry.try_get("10").await.is_some() {
            tokio::time::sleep(StdDuration::from_millis(25)).await;
        }
    })
    .await
    .expect("native owner must reap and release its registry entry");
    assert_eq!(worker.admission_status(), HlsAdmissionStatus::Waiting);
    assert_eq!(active_transcode_snapshot(1).active, capacity_before);
    assert!(
        !folder.exists(),
        "native owner must remove its output assets"
    );
    if let Err(panic) = proof {
        std::panic::resume_unwind(panic);
    }
    let started = started.expect("native capture must have an installed session model");
    let join_ms = (started - timeline[0].item.start).num_milliseconds();
    let captured = tokio::fs::read_to_string(root.join("native-capture/live.m3u8"))
        .await
        .unwrap();
    let first_clock = captured
        .lines()
        .find_map(|line| line.strip_prefix("#EXT-X-PROGRAM-DATE-TIME:"))
        .expect("captured first programme clock");
    let first_clock = chrono::DateTime::parse_from_str(first_clock, "%Y-%m-%dT%H:%M:%S%.f%z")
        .unwrap()
        .timestamp_millis();
    tokio::fs::write(root.join("native-schedule.json"),serde_json::to_vec_pretty(&json!({"blue_start":timeline[0].item.start,"blue_finish":timeline[0].item.finish,"red_start":timeline[1].item.start,"join_ms":join_ms,"captured_first_clock_ms":first_clock,"producer_started_ms":started.timestamp_millis(),"registry_released":true,"capacity_restored":true,"output_removed":true})).unwrap()).await.unwrap();
    eprintln!("native owned cleanup complete; join={join_ms}ms, captured first clock={first_clock}, producer clock={}",started.timestamp_millis());
    assert_eq!(
        first_clock,
        started.timestamp_millis(),
        "capture must begin at the first programme, not a later repeated pair"
    );
    assert!(
        (0..200).contains(&join_ms),
        "fixture must join the first programme before either decoded sample: {join_ms}ms"
    );
    decoded(
        tools,
        &root.join("native-capture/live.m3u8"),
        root,
        "native",
    )
    .await;
}

async fn next(
    pool: &SqlitePool,
    locker: &EntityLocker,
    tools: &Tools,
    root: &Path,
    scanned: &[Scanned; 2],
) {
    let timeline = build(pool, locker, scanned).await;
    let channel = make_repositories(pool.clone())
        .channel
        .get_with_profile(10)
        .await
        .unwrap();
    let profile = load_playback_profile(pool, 1).await.unwrap().unwrap();
    let folder = root.join("next-playout");
    tokio::fs::create_dir(&folder).await.unwrap();
    let host = NextHost::new(pool.clone(), locker.clone(), root.join("next-host"));
    let mut items = Vec::new();
    for media in &timeline[..2] {
        items.push(
            host.prepare_troubleshooting(TroubleshootingPreview {
                media,
                channel: &channel,
                profile: &profile,
                subtitle_id: Some(0),
                selection_time: None,
                watermark_ids: &[],
                graphics_ids: &[],
                replacements: &[],
                ffmpeg: &tools.ffmpeg,
                deadline: tokio::time::Instant::now() + StdDuration::from_secs(30),
                diagnostics: None,
            })
            .await
            .unwrap(),
        );
    }
    let start = items[0].start;
    let finish = items[1].finish;
    let wire = protocol::Playout {
        version: protocol::PLAYOUT_VERSION.into(),
        items,
    };
    tokio::fs::write(
        folder.join(format!(
            "{}_{}.json",
            start.timestamp_millis(),
            finish.timestamp_millis()
        )),
        serde_json::to_vec_pretty(&wire).unwrap(),
    )
    .await
    .unwrap();
    let mut configuration = config::channel_config(
        &profile,
        NextEngineTextSubtitleMode::Burn,
        &folder,
        &tools.ffmpeg,
        &tools.ffprobe,
    )
    .unwrap();
    configuration.playout.virtual_start = Some(start);
    let output = root.join("next-output");
    let mut process = NextProcess::start_troubleshooting(
        tools
            .next
            .as_ref()
            .expect("finite Next acceptance requires its published executable"),
        "10",
        &output,
        &configuration,
        None,
        tokio::time::Instant::now() + StdDuration::from_secs(30),
    )
    .await
    .unwrap();
    let waited = timeout(StdDuration::from_mins(1), process.wait()).await;
    if !matches!(&waited,Ok(Ok(status)) if status.success()) {
        process
            .stop()
            .await
            .expect("failed finite Next producer must be reaped");
        panic!("finite Next handoff failed: {}", process.diagnostics());
    }
    let playlist = output.join("live.m3u8");
    let body = tokio::fs::read_to_string(&playlist).await.unwrap();
    assert!(body.lines().any(|line| line == "#EXT-X-ENDLIST"));
    let seconds = playlist_seconds(&body);
    eprintln!("finite Next generated playlist duration: {seconds}s, ENDLIST present");
    tokio::fs::write(root.join("next-duration.json"),serde_json::to_vec_pretty(&json!({"playlist_seconds":seconds,"scheduled_milliseconds":(finish-start).num_milliseconds(),"successful_reaped":true})).unwrap()).await.unwrap();
    assert!(
        (5.8..6.3).contains(&seconds),
        "finite scanned pair must be six seconds, not twenty-five: {seconds}"
    );
    decoded(tools, &playlist, root, "next").await;
}

async fn decoded(tools: &Tools, playlist: &Path, root: &Path, engine: &str) {
    let bytes = tool(
        Command::new(&tools.ffmpeg)
            .args(["-v", "error", "-nostdin", "-i"])
            .arg(playlist)
            .args([
                "-map",
                "0:v:0",
                "-vf",
                "fps=4,scale=1:1:flags=area",
                "-t",
                "2",
                "-pix_fmt",
                "rgb24",
                "-f",
                "rawvideo",
                "pipe:1",
            ]),
    )
    .await;
    tokio::fs::write(root.join(format!("{engine}-decoded.rgb")), &bytes)
        .await
        .unwrap();
    assert!(
        bytes.len() >= 6 * 3,
        "decoded frames must extend beyond the programme handoff"
    );
    let blue = &bytes[3..6];
    let red = &bytes[15..18];
    eprintln!("{engine} decoded content handoff: blue@0.25={blue:?}, red@1.25={red:?}");
    assert!(
        blue[2] > 150 && blue[0] < 70 && blue[1] < 70,
        "{engine} frame at0.25s must be blue: {blue:?}"
    );
    assert!(
        red[0] > 150 && red[1] < 70 && red[2] < 70,
        "{engine} frame at1.25s must be red: {red:?}"
    );
    let packets = tool(
        Command::new(&tools.ffprobe)
            .args([
                "-v",
                "error",
                "-select_streams",
                "v:0",
                "-read_intervals",
                "%+#50",
                "-show_packets",
                "-show_entries",
                "packet=pts_time,duration_time",
                "-of",
                "json",
            ])
            .arg(playlist),
    )
    .await;
    let probe: Value = serde_json::from_slice(&packets).unwrap();
    tokio::fs::write(root.join(format!("{engine}-packets.json")), &packets)
        .await
        .unwrap();
    let pts: Vec<f64> = probe["packets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|packet| packet["pts_time"].as_str().unwrap().parse().unwrap())
        .collect();
    assert!(pts.len() > 10);
    // HLS permits a discontinuity at the producer handoff; prohibit a twenty-second gap.
    assert!(
        pts.windows(2).all(|pair| pair[1] - pair[0] < 1.0),
        "{engine} content PTS must have no long subtitle-induced gap"
    );
}

async fn assert_synthetic_silence(tools: &Tools, root: &Path) {
    let playlist = root.join("native-capture/live.m3u8");
    let bytes = tool(
        Command::new(&tools.ffprobe)
            .args([
                "-v",
                "error",
                "-select_streams",
                "a:0",
                "-read_intervals",
                "%+#1",
                "-show_packets",
                "-show_streams",
                "-of",
                "json",
            ])
            .arg(&playlist),
    )
    .await;
    tokio::fs::write(root.join("native-audio-probe.json"), &bytes)
        .await
        .unwrap();
    let probe: Value = serde_json::from_slice(&bytes).unwrap();
    let streams = probe["streams"].as_array().unwrap();
    eprintln!("video-only native generated audio streams: {streams:?}");
    assert_eq!(
        streams.len(),
        1,
        "native output must contain the synthetic audio track"
    );
    assert_eq!(streams[0]["codec_name"], "aac");
    assert_eq!(streams[0]["channels"], 2);
    assert_eq!(streams[0]["sample_rate"], "48000");
    assert!(
        !probe["packets"].as_array().unwrap().is_empty(),
        "generated audio must contain actual packets"
    );
    // Decode both programmes on the audio sample clock. Do not resample or
    // remix: the preceding probe proves the producer's actual stereo/48kHz.
    let pcm = tool(
        Command::new(&tools.ffmpeg)
            .args(["-v", "error", "-nostdin", "-i"])
            .arg(&playlist)
            .args([
                "-map",
                "0:a:0",
                "-af",
                "asetpts=N/SR/TB",
                "-t",
                "2",
                "-c:a",
                "pcm_s16le",
                "-f",
                "s16le",
                "pipe:1",
            ]),
    )
    .await;
    tokio::fs::write(root.join("native-audio.s16le"), &pcm)
        .await
        .unwrap();
    assert_eq!(pcm.len() % 4, 0, "complete interleaved stereo PCM frames");
    let max_amplitude = pcm
        .chunks_exact(2)
        .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]).unsigned_abs())
        .max()
        .unwrap_or_default();
    eprintln!(
        "video-only native decoded silence: {} bytes, max amplitude={max_amplitude}",
        pcm.len()
    );
    tokio::fs::write(root.join("native-silence.json"),serde_json::to_vec_pretty(&json!({"codec":"aac","channels":2,"sample_rate_hz":48000,"pcm_bytes":pcm.len(),"max_amplitude":max_amplitude})).unwrap()).await.unwrap();
    assert!(
        pcm.len() >= 2 * 48000 * 2 * 2,
        "decoded silence must span blue and red programmes"
    );
    assert!(
        max_amplitude <= 1,
        "generated audio must decode to silence across the programme handoff"
    );
}

async fn assert_scanned_video_only(pool: &SqlitePool, scanned: &[Scanned; 2]) {
    for media in scanned {
        let audio:i64=sqlx::query_scalar(r#"SELECT COUNT(*) FROM "MediaStream" WHERE "MediaVersionId"=? AND "MediaStreamKind"=2"#).bind(media.version).fetch_one(pool).await.unwrap();
        assert_eq!(audio, 0, "persisted probe must confirm no source audio");
    }
    let subtitle:i32=sqlx::query_scalar(r#"SELECT "Index" FROM "MediaStream" WHERE "MediaVersionId"=? AND "MediaStreamKind"=3 AND "Language"='nld'"#).bind(scanned[0].version).fetch_one(pool).await.unwrap();
    assert_eq!(
        subtitle, 1,
        "preserve Dutch subtitle index1 without promoting it to audio"
    );
}

#[expect(
    clippy::disallowed_methods,
    reason = "Explicit opt-in native video-only synthetic proof export, never production configuration"
)]
async fn export_video_only(root: &Path) {
    if let Some(destination) = env::var_os("ERSATZRS_ROW194_PROOF_DIR") {
        let destination = PathBuf::from(destination).join("native-video-only");
        tokio::fs::create_dir_all(&destination).await.unwrap();
        for name in [
            "input-probe.json",
            "red-input-probe.json",
            "scanned-identities.json",
            "native-schedule.json",
            "native-decoded.rgb",
            "native-packets.json",
            "native-audio-probe.json",
            "native-audio.s16le",
            "native-silence.json",
        ] {
            tokio::fs::copy(root.join(name), destination.join(name))
                .await
                .unwrap();
        }
        let target = destination.join("native-capture");
        tokio::fs::create_dir(&target).await.unwrap();
        let mut entries = tokio::fs::read_dir(root.join("native-capture"))
            .await
            .unwrap();
        while let Some(entry) = entries.next_entry().await.unwrap() {
            if matches!(
                entry
                    .path()
                    .extension()
                    .and_then(|extension| extension.to_str()),
                Some("ts" | "m3u8")
            ) {
                tokio::fs::copy(entry.path(), target.join(entry.file_name()))
                    .await
                    .unwrap();
            }
        }
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "Optional explicit opt-in synthetic evidence export, never production configuration"
)]
async fn export(root: &Path) {
    if let Some(destination) = env::var_os("ERSATZRS_ROW194_PROOF_DIR") {
        let destination = PathBuf::from(destination);
        tokio::fs::create_dir_all(&destination).await.unwrap();
        for name in [
            "input-probe.json",
            "scanned-identities.json",
            "native-schedule.json",
            "native-decoded.rgb",
            "native-packets.json",
            "next-decoded.rgb",
            "next-packets.json",
            "next-duration.json",
        ] {
            tokio::fs::copy(root.join(name), destination.join(name))
                .await
                .unwrap();
        }
        tokio::fs::copy(
            root.join("next-output/live.m3u8"),
            destination.join("next-live.m3u8"),
        )
        .await
        .unwrap();
        tokio::fs::copy(
            root.join("native-capture/live.m3u8"),
            destination.join("native-live.m3u8"),
        )
        .await
        .unwrap();
        for source in ["native-capture", "next-output"] {
            let target = destination.join(source);
            tokio::fs::create_dir_all(&target).await.unwrap();
            let mut entries = tokio::fs::read_dir(root.join(source)).await.unwrap();
            while let Some(entry) = entries.next_entry().await.unwrap() {
                if matches!(
                    entry
                        .path()
                        .extension()
                        .and_then(|extension| extension.to_str()),
                    Some("ts" | "m3u8")
                ) {
                    tokio::fs::copy(entry.path(), target.join(entry.file_name()))
                        .await
                        .unwrap();
                }
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires exact published Next036 and FFmpeg v2.0.6; isolated scan/schedule/decoded HLS handoff"]
async fn local_scanned_content_duration_bounds_native_and_next_handoff() {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::WARN)
        .try_init();
    let tools = Tools::load();
    let root = tempfile::tempdir().unwrap();
    let pool = test_db().await;
    let proof = AssertUnwindSafe(async {
        generate(&tools, &root.path().join("library")).await;
        prove_conflict(&tools, root.path(), true).await;
        let scanned = scan_and_rescan(&tools, root.path(), &pool).await;
        seed_schedule(&pool, root.path(), &scanned).await;
        let locker = EntityLocker::new();
        native(&pool, &locker, &tools, root.path(), &scanned).await;
        next(&pool, &locker, &tools, root.path(), &scanned).await;
        export(root.path()).await;
    })
    .catch_unwind()
    .await;
    pool.close().await;
    if let Err(panic) = proof {
        eprintln!(
            "retained synthetic content-duration fixture: {}",
            root.keep().display()
        );
        std::panic::resume_unwind(panic);
    }
    let path = root.path().to_path_buf();
    root.close().unwrap();
    assert!(
        !path.exists(),
        "successful fixture must remove its owned temporary assets"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires published FFmpeg v2.0.6; genuine scanned video-only native HLS/silent-audio handoff"]
async fn native_video_only_scanned_duration_synthesizes_silent_audio_and_handoff() {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::WARN)
        .try_init();
    let tools = Tools::native();
    let root = tempfile::tempdir().unwrap();
    let pool = test_db().await;
    let proof = AssertUnwindSafe(async {
        generate_video_only(&tools, root.path()).await;
        prove_conflict(&tools, root.path(), false).await;
        let scanned = scan_and_rescan(&tools, root.path(), &pool).await;
        assert_scanned_video_only(&pool, &scanned).await;
        seed_schedule(&pool, root.path(), &scanned).await;
        native(&pool, &EntityLocker::new(), &tools, root.path(), &scanned).await;
        assert_synthetic_silence(&tools, root.path()).await;
        export_video_only(root.path()).await;
    })
    .catch_unwind()
    .await;
    pool.close().await;
    if let Err(panic) = proof {
        eprintln!(
            "retained synthetic native video-only fixture: {}",
            root.keep().display()
        );
        std::panic::resume_unwind(panic);
    }
    let path = root.path().to_path_buf();
    root.close().unwrap();
    assert!(
        !path.exists(),
        "successful video-only fixture must remove its owned assets"
    );
}
