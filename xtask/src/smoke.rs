//! Runtime smoke gate for streaming-path parity.
//!
//! This boots the real web binary against a temporary SQLite database, seeds a
//! minimal local movie + playout, then probes the IPTV MPEG-TS and HLS
//! segmenter endpoints. It is intentionally std-only apart from `rusqlite`
//! because the command is a developer harness, not shipped product code.

use std::{
    env, fs,
    io::{self, Read, Write},
    net::{SocketAddr, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, ExitCode, Stdio},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, bail, Context, Result};
use rusqlite::{params, Connection};

const LISTEN_ADDR: &str = "127.0.0.1:8409";
const DEFAULT_TIMEOUT_SECONDS: u64 = 90;
const DEFAULT_CHANNEL: &str = "1";
const DEFAULT_SMOKE_MEDIA_SECONDS: i64 = 24;
const LIFECYCLE_SMOKE_MEDIA_SECONDS: i64 = 120;
const DEFAULT_SEGMENTER_TIMEOUT_SECONDS: u64 = 30;
const LIFECYCLE_SEGMENTER_TIMEOUT_SECONDS: u64 = 20;
const HLS_IDLE_GRACE_SECONDS: u64 = 8;
const MIN_STREAM_BODY_BYTES: usize = 188;

const FAILURE_SIGNATURES: &[&str] = &[
    "ffmpeg hls segmenter exited unsuccessfully",
    "failed to spawn ffmpeg hls segmenter",
    "hls segmenter did not produce initial playlist segments before timeout",
    "Impossible to convert between the formats",
    "Your platform doesn't support hardware accelerated",
    "Error submitting packet to decoder",
    "Decode error rate",
    "CUDA_ERROR_INVALID_VALUE",
    "no active playout item",
    "channel has no playback ffmpeg profile",
];

pub fn run(args: Vec<String>) -> ExitCode {
    match Opts::parse(args).and_then(run_inner) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("smoke: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run_inner(opts: Opts) -> Result<()> {
    ensure_port_free()?;

    let root = SmokeRoot::new(opts.keep_temp)?;
    let db_path = root.path.join("ErsatzRS-smoke.db");
    let segmenter_timeout_seconds = opts.segmenter_timeout_seconds();
    let media_seconds = opts.media_seconds();
    let media_path = match opts.media_path {
        Some(path) => path,
        None => {
            let path = root.path.join("smoke-source.mp4");
            generate_fixture_media(&path, media_seconds)?;
            path
        }
    };
    if !media_path.is_file() {
        bail!("smoke media does not exist: {}", media_path.display());
    }

    let log_path = root.path.join("ersatzrs-web-smoke.log");
    let database_url = format!("sqlite:{}", db_path.display());
    let mut server = ServerProcess::start(&database_url, &root.path, &log_path)?;

    wait_for_health(&opts.timeout)?;
    seed_streaming_fixture(
        &db_path,
        &media_path,
        segmenter_timeout_seconds,
        media_seconds,
    )?;

    let deadline = Deadline::from_now(opts.timeout);
    for channel in &opts.channels {
        if opts.check_ts && !opts.check_hls_lifecycle {
            let path = format!("/iptv/channel/{channel}.ts?mode=ts");
            let response = http_get_until_body(&path, MIN_STREAM_BODY_BYTES, deadline.remaining()?)
                .with_context(|| format!("probing MPEG-TS endpoint {path}"))?;
            response.ensure_success_with_body(&path)?;
            println!(
                "smoke evidence: {path} -> HTTP {} with {} body bytes",
                response.status,
                response.body.len()
            );
        }

        if opts.check_hls {
            let variant_path = format!("/iptv/channel/{channel}.m3u8?mode=segmenter");
            let variant = http_get_until_body(&variant_path, 1, deadline.remaining()?)
                .with_context(|| format!("probing HLS variant endpoint {variant_path}"))?;
            variant.ensure_success_with_body(&variant_path)?;
            if !String::from_utf8_lossy(&variant.body).contains("#EXTM3U") {
                bail!("{variant_path}: response did not contain an HLS playlist");
            }
            println!(
                "smoke evidence: {variant_path} -> HTTP {} variant playlist",
                variant.status
            );

            let session_path = format!("/iptv/session/{channel}/hls.m3u8");
            let session = http_get_until_body(&session_path, 1, deadline.remaining()?)
                .with_context(|| format!("probing HLS session endpoint {session_path}"))?;
            session.ensure_success_with_body(&session_path)?;
            let session_body = String::from_utf8_lossy(&session.body);
            if !session_body.contains("#EXTINF") {
                bail!("{session_path}: playlist did not contain any initial segments");
            }
            println!(
                "smoke evidence: {session_path} -> HTTP {} playlist with initial segments",
                session.status
            );
            if opts.check_hls_lifecycle {
                fetch_first_hls_segment(channel, &session_body, deadline.remaining()?)?;
            }
        }
    }

    if opts.check_hls_lifecycle {
        for channel in &opts.channels {
            if opts.check_hls {
                wait_for_hls_working_dir_removed(
                    channel,
                    Duration::from_secs(segmenter_timeout_seconds + HLS_IDLE_GRACE_SECONDS),
                    "HLS segmenter",
                )?;
            }

            if opts.check_ts {
                let path = format!("/iptv/channel/{channel}.ts?mode=ts");
                let response =
                    http_get_until_body(&path, MIN_STREAM_BODY_BYTES, deadline.remaining()?)
                        .with_context(|| {
                            format!("probing MPEG-TS wrapper endpoint for lifecycle proof {path}")
                        })?;
                response.ensure_success_with_body(&path)?;
                println!(
                    "smoke evidence: {path} -> HTTP {} wrapper restart with {} body bytes",
                    response.status,
                    response.body.len()
                );
                wait_for_hls_working_dir_removed(
                    channel,
                    Duration::from_secs(segmenter_timeout_seconds + HLS_IDLE_GRACE_SECONDS),
                    "MPEG-TS wrapper",
                )?;
            }
        }
    }

    server.stop();
    fail_on_log_signatures(&log_path)?;
    println!("smoke: passed; log: {}", log_path.display());
    Ok(())
}

#[derive(Debug)]
struct Opts {
    channels: Vec<String>,
    timeout: Duration,
    media_path: Option<PathBuf>,
    keep_temp: bool,
    check_ts: bool,
    check_hls: bool,
    check_hls_lifecycle: bool,
}

impl Opts {
    fn parse(args: Vec<String>) -> Result<Self> {
        let mut channels = Vec::new();
        let mut timeout = Duration::from_secs(DEFAULT_TIMEOUT_SECONDS);
        let mut media_path = None;
        let mut keep_temp = false;
        let mut check_ts = true;
        let mut check_hls = true;
        let mut check_hls_lifecycle = false;

        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "-h" | "--help" => {
                    print_help();
                    std::process::exit(0);
                }
                "--channel" => {
                    i += 1;
                    let Some(channel) = args.get(i) else {
                        bail!("--channel requires a channel number");
                    };
                    channels.push(channel.clone());
                }
                "--timeout" => {
                    i += 1;
                    let Some(value) = args.get(i) else {
                        bail!("--timeout requires seconds");
                    };
                    timeout = Duration::from_secs(
                        value
                            .parse::<u64>()
                            .with_context(|| format!("invalid --timeout `{value}`"))?,
                    );
                }
                "--media" => {
                    i += 1;
                    let Some(value) = args.get(i) else {
                        bail!("--media requires a path");
                    };
                    media_path = Some(PathBuf::from(value));
                }
                "--keep-temp" => keep_temp = true,
                "--hls-lifecycle" => check_hls_lifecycle = true,
                "--skip-ts" => check_ts = false,
                "--skip-hls" => check_hls = false,
                other if other.starts_with('-') => bail!("unknown smoke flag `{other}`"),
                other => channels.push(other.to_owned()),
            }
            i += 1;
        }

        if channels.is_empty() {
            channels.push(DEFAULT_CHANNEL.to_owned());
        }
        if !check_ts && !check_hls {
            bail!("at least one of MPEG-TS or HLS smoke checks must run");
        }

        Ok(Self {
            channels,
            timeout,
            media_path,
            keep_temp,
            check_ts,
            check_hls,
            check_hls_lifecycle,
        })
    }

    const fn segmenter_timeout_seconds(&self) -> u64 {
        if self.check_hls_lifecycle {
            LIFECYCLE_SEGMENTER_TIMEOUT_SECONDS
        } else {
            DEFAULT_SEGMENTER_TIMEOUT_SECONDS
        }
    }

    const fn media_seconds(&self) -> i64 {
        if self.check_hls_lifecycle {
            LIFECYCLE_SMOKE_MEDIA_SECONDS
        } else {
            DEFAULT_SMOKE_MEDIA_SECONDS
        }
    }
}

struct SmokeRoot {
    path: PathBuf,
    keep: bool,
}

impl SmokeRoot {
    fn new(keep: bool) -> Result<Self> {
        let path = env::temp_dir().join(format!(
            "ersatzrs-smoke-{}-{}",
            std::process::id(),
            unix_now()?.as_secs()
        ));
        fs::create_dir_all(&path).with_context(|| format!("creating {}", path.display()))?;
        Ok(Self { path, keep })
    }
}

impl Drop for SmokeRoot {
    fn drop(&mut self) {
        if self.keep {
            eprintln!("smoke: kept temp dir {}", self.path.display());
        } else {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

struct ServerProcess {
    child: Option<Child>,
}

impl ServerProcess {
    fn start(database_url: &str, home: &Path, log_path: &Path) -> Result<Self> {
        let binary = build_web_binary()?;
        let log = fs::File::create(log_path)
            .with_context(|| format!("creating smoke log {}", log_path.display()))?;
        let stderr = log
            .try_clone()
            .with_context(|| format!("cloning smoke log {}", log_path.display()))?;

        let mut command = Command::new(&binary);
        command
            .env("DATABASE_URL", database_url)
            .env("HOME", home)
            .env("RUST_LOG", smoke_rust_log())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(stderr));
        let ffmpeg_path = configured_ffmpeg_path().or_else(|| find_on_path("ffmpeg"));
        apply_ffmpeg_library_path(&mut command, ffmpeg_path.as_deref());
        let child = command
            .spawn()
            .with_context(|| format!("starting {}", binary.display()))?;

        Ok(Self { child: Some(child) })
    }

    fn stop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        if child.try_wait().ok().flatten().is_none() {
            let _ = child.kill();
        }
        let _ = child.wait();
    }
}

impl Drop for ServerProcess {
    fn drop(&mut self) {
        self.stop();
    }
}

fn smoke_rust_log() -> String {
    #[allow(clippy::disallowed_methods)]
    env::var("RUST_LOG").unwrap_or_else(|_| {
        "info,ersatzrs_infra::hls_worker=debug,ersatzrs_web::transport::iptv=debug".to_owned()
    })
}

fn build_web_binary() -> Result<PathBuf> {
    let status = Command::new("cargo")
        .args(["build", "--bin", "ersatzrs-web"])
        .status()
        .context("building ersatzrs-web for smoke")?;
    if !status.success() {
        bail!("cargo build --bin ersatzrs-web failed with {status}");
    }

    let exe = if cfg!(windows) {
        "ersatzrs-web.exe"
    } else {
        "ersatzrs-web"
    };
    Ok(resolve_target_dir().join("debug").join(exe))
}

fn resolve_target_dir() -> PathBuf {
    #[allow(clippy::disallowed_methods)]
    if let Some(value) = env::var_os("CARGO_TARGET_DIR") {
        return PathBuf::from(value);
    }
    let config = Path::new(".cargo").join("config.toml");
    let Some(value) = fs::read_to_string(config)
        .ok()
        .and_then(|text| parse_config_target_dir(&text))
    else {
        return PathBuf::from("target");
    };
    PathBuf::from(value)
}

fn parse_config_target_dir(text: &str) -> Option<String> {
    let mut in_build = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_build = trimmed == "[build]";
            continue;
        }
        if !in_build || !trimmed.starts_with("target-dir") {
            continue;
        }
        let (_, value) = trimmed.split_once('=')?;
        return Some(value.trim().trim_matches(['"', '\'']).to_owned());
    }
    None
}

fn ensure_port_free() -> Result<()> {
    let addr: SocketAddr = LISTEN_ADDR.parse()?;
    if TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok() {
        bail!("{LISTEN_ADDR} is already in use; stop the existing server before running smoke");
    }
    Ok(())
}

fn generate_fixture_media(path: &Path, media_seconds: i64) -> Result<()> {
    let configured = configured_ffmpeg_path().or_else(|| find_on_path("ffmpeg"));
    let ffmpeg = configured.as_deref().unwrap_or("ffmpeg");
    let seconds = media_seconds.to_string();
    let mut command = Command::new(ffmpeg);
    apply_ffmpeg_library_path(&mut command, configured.as_deref());
    let status = command
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x180:rate=30",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=1000:sample_rate=48000",
            "-t",
            &seconds,
            "-c:v",
            "mpeg4",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-shortest",
        ])
        .arg(path)
        .status()
        .context("generating smoke fixture media with ffmpeg")?;
    if !status.success() {
        bail!("ffmpeg failed to generate smoke fixture media with {status}");
    }
    Ok(())
}

fn configured_ffmpeg_path() -> Option<String> {
    #[allow(clippy::disallowed_methods)]
    env::var("ErsatzRS_FFMPEG__PATH")
        .or_else(|_| env::var("ERSATZRS_FFMPEG__PATH"))
        .or_else(|_| env::var("FFMPEG_PATH"))
        .ok()
}

fn apply_ffmpeg_library_path(command: &mut Command, ffmpeg_path: Option<&str>) {
    let Some(ffmpeg_path) = ffmpeg_path else {
        return;
    };
    let Some(bin_dir) = Path::new(ffmpeg_path).parent() else {
        return;
    };
    let Some(root_dir) = bin_dir.parent() else {
        return;
    };
    let lib_dir = root_dir.join("lib");
    if !lib_dir.is_dir() {
        return;
    }

    #[cfg(target_os = "linux")]
    append_env_path(command, "LD_LIBRARY_PATH", &lib_dir);
    #[cfg(target_os = "macos")]
    append_env_path(command, "DYLD_LIBRARY_PATH", &lib_dir);
    #[cfg(windows)]
    {
        append_env_path(command, "PATH", bin_dir);
        append_env_path(command, "PATH", &lib_dir);
    }
}

fn append_env_path(command: &mut Command, key: &str, path: &Path) {
    let mut paths = vec![path.to_path_buf()];
    #[allow(clippy::disallowed_methods)]
    if let Some(existing) = env::var_os(key) {
        paths.extend(env::split_paths(&existing));
    }
    if let Ok(value) = env::join_paths(paths) {
        command.env(key, value);
    }
}

fn find_on_path(exe: &str) -> Option<String> {
    #[allow(clippy::disallowed_methods)]
    let path = env::var_os("PATH")?;
    for dir in env::split_paths(&path) {
        let candidate = dir.join(exe);
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().into_owned());
        }
        #[cfg(windows)]
        {
            let candidate = dir.join(format!("{exe}.exe"));
            if candidate.is_file() {
                return Some(candidate.to_string_lossy().into_owned());
            }
        }
    }
    None
}

fn wait_for_health(timeout: &Duration) -> Result<()> {
    let deadline = Deadline::from_now(*timeout);
    loop {
        match http_get_until_body("/healthz", 0, Duration::from_secs(2)) {
            Ok(response) if response.status == 200 => return Ok(()),
            Ok(response) => {
                if deadline.expired() {
                    bail!(
                        "/healthz did not become healthy; last status {}",
                        response.status
                    );
                }
            }
            Err(error) => {
                if deadline.expired() {
                    return Err(error).context("/healthz did not become reachable");
                }
            }
        }
        thread::sleep(Duration::from_millis(500));
    }
}

fn seed_streaming_fixture(
    db_path: &Path,
    media_path: &Path,
    segmenter_timeout_seconds: u64,
    media_seconds: i64,
) -> Result<()> {
    let conn =
        Connection::open(db_path).with_context(|| format!("opening {}", db_path.display()))?;
    let channel_id: i64 = conn.query_row(
        r#"SELECT "Id" FROM "Channel" WHERE "Number" = ? ORDER BY "Id" LIMIT 1"#,
        [DEFAULT_CHANNEL],
        |row| row.get(0),
    )?;
    let media_path = media_path
        .canonicalize()
        .unwrap_or_else(|_| media_path.to_path_buf())
        .to_string_lossy()
        .into_owned();
    let now = unix_now()?.as_secs() as i64;
    let date_added = format_timestamp(now);
    let playout_start = format_timestamp(now - 2);
    let playout_finish = format_timestamp(now + media_seconds - 2);

    conn.execute_batch("BEGIN IMMEDIATE")?;
    let result = seed_streaming_fixture_inner(SeedFixture {
        conn: &conn,
        channel_id,
        media_path: &media_path,
        date_added: &date_added,
        playout_start: &playout_start,
        playout_finish: &playout_finish,
        segmenter_timeout_seconds,
        media_seconds,
    });
    match result {
        Ok(()) => {
            conn.execute_batch("COMMIT")?;
            Ok(())
        }
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

struct SeedFixture<'a> {
    conn: &'a Connection,
    channel_id: i64,
    media_path: &'a str,
    date_added: &'a str,
    playout_start: &'a str,
    playout_finish: &'a str,
    segmenter_timeout_seconds: u64,
    media_seconds: i64,
}

fn seed_streaming_fixture_inner(input: SeedFixture<'_>) -> Result<()> {
    let SeedFixture {
        conn,
        channel_id,
        media_path,
        date_added,
        playout_start,
        playout_finish,
        segmenter_timeout_seconds,
        media_seconds,
    } = input;
    conn.execute(r#"DELETE FROM "PlayoutItem""#, [])?;
    conn.execute(r#"DELETE FROM "Playout""#, [])?;
    conn.execute(r#"DELETE FROM "MediaStream""#, [])?;
    conn.execute(r#"DELETE FROM "MediaFile""#, [])?;
    conn.execute(r#"DELETE FROM "MediaVersion""#, [])?;
    conn.execute(r#"DELETE FROM "MovieMetadata""#, [])?;
    conn.execute(r#"DELETE FROM "Movie""#, [])?;
    conn.execute(r#"DELETE FROM "MediaItem""#, [])?;
    conn.execute(r#"DELETE FROM "LibraryPath""#, [])?;
    conn.execute(r#"DELETE FROM "LocalLibrary""#, [])?;
    conn.execute(r#"DELETE FROM "Library""#, [])?;
    conn.execute(r#"DELETE FROM "LocalMediaSource""#, [])?;
    conn.execute(r#"DELETE FROM "MediaSource""#, [])?;

    conn.execute(
        r#"DELETE FROM "ConfigElement"
           WHERE "Key" IN ('ffmpeg.segmenter.timeout_seconds', 'ffmpeg.segmenter.initial_segment_count')"#,
        [],
    )?;
    conn.execute(
        r#"INSERT INTO "ConfigElement" ("Key", "Value")
           VALUES ('ffmpeg.segmenter.timeout_seconds', ?)"#,
        [segmenter_timeout_seconds.to_string()],
    )?;
    conn.execute(
        r#"INSERT INTO "ConfigElement" ("Key", "Value")
           VALUES ('ffmpeg.segmenter.initial_segment_count', '1')"#,
        [],
    )?;

    conn.execute(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#, [])?;
    conn.execute(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (1)"#, [])?;
    conn.execute(
        r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
           VALUES (1, 'Smoke Movies', 1, 1)"#,
        [],
    )?;
    conn.execute(
        r#"INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId", "LastScan")
           VALUES (1, ?, 1, ?)"#,
        params![
            Path::new(media_path).parent().and_then(Path::to_str),
            date_added
        ],
    )?;
    conn.execute(
        r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (1, 1, 0)"#,
        [],
    )?;
    conn.execute(r#"INSERT INTO "Movie" ("Id") VALUES (1)"#, [])?;
    conn.execute(
        r#"INSERT INTO "MovieMetadata" (
               "DateAdded", "DateUpdated", "MetadataKind", "MovieId", "Title"
           ) VALUES (?, ?, 0, 1, 'Smoke Movie')"#,
        params![date_added, date_added],
    )?;
    conn.execute(
        r#"INSERT INTO "MediaVersion" (
               "Id", "DateAdded", "DateUpdated", "Duration", "Height", "MovieId",
               "Name", "RFrameRate", "VideoScanKind", "Width"
           ) VALUES (1, ?, ?, ?, 180, 1, 'Smoke', '30/1', 1, 320)"#,
        params![date_added, date_added, format_duration_hms(media_seconds)],
    )?;
    conn.execute(
        r#"INSERT INTO "MediaFile" ("Id", "MediaVersionId", "Path", "PathHash")
           VALUES (1, 1, ?, 'smoke-path-hash')"#,
        [media_path],
    )?;
    conn.execute(
        r#"INSERT INTO "MediaStream" (
               "Index", "Codec", "MediaStreamKind", "Channels", "Default",
               "Forced", "MediaVersionId", "PixelFormat"
           ) VALUES (0, 'mpeg4', 1, 0, 1, 0, 1, 'yuv420p')"#,
        [],
    )?;
    conn.execute(
        r#"INSERT INTO "MediaStream" (
               "Index", "Codec", "MediaStreamKind", "Language", "Channels",
               "Default", "Forced", "MediaVersionId"
           ) VALUES (1, 'aac', 2, 'eng', 2, 1, 0, 1)"#,
        [],
    )?;
    conn.execute(
        r#"INSERT INTO "Playout" (
               "Id", "ChannelId", "ScheduleKind", "Seed", "OnDemandCheckpoint"
           ) VALUES (1, ?, 0, 1, ?)"#,
        params![channel_id, playout_start],
    )?;
    conn.execute(
        r#"INSERT INTO "PlayoutItem" (
               "Id", "DisableWatermarks", "FillerKind", "Finish", "GuideGroup",
               "InPoint", "MediaItemId", "OutPoint", "PlayoutId", "Start"
           ) VALUES (1, 0, 0, ?, 0, '00:00:00', 1, ?, 1, ?)"#,
        params![
            playout_finish,
            format_duration_hms(media_seconds),
            playout_start
        ],
    )?;
    conn.execute(
        r#"UPDATE "Channel"
           SET "StreamingMode" = 5,
               "IdleBehavior" = 0,
               "PlayoutMode" = 0,
               "IsEnabled" = 1
           WHERE "Id" = ?"#,
        [channel_id],
    )?;
    Ok(())
}

fn fetch_first_hls_segment(channel: &str, playlist: &str, timeout: Duration) -> Result<()> {
    let uri = playlist
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .ok_or_else(|| anyhow!("/iptv/session/{channel}/hls.m3u8: no segment URI found"))?;
    let path = hls_segment_uri_to_path(channel, uri)?;
    let response = http_get_until_body(&path, 1, timeout)
        .with_context(|| format!("probing HLS segment endpoint {path}"))?;
    response.ensure_success_with_body(&path)?;
    println!(
        "smoke evidence: {path} -> HTTP {} segment with {} body bytes",
        response.status,
        response.body.len()
    );
    Ok(())
}

fn hls_segment_uri_to_path(channel: &str, uri: &str) -> Result<String> {
    if uri.starts_with('/') {
        return Ok(uri.to_owned());
    }

    if let Some(path_start) = uri.find("/iptv/") {
        return Ok(uri[path_start..].to_owned());
    }

    if uri.contains("://") {
        bail!("unsupported absolute HLS segment URI: {uri}");
    }

    Ok(format!("/iptv/session/{channel}/{uri}"))
}

fn wait_for_hls_working_dir_removed(channel: &str, timeout: Duration, label: &str) -> Result<()> {
    let path = env::temp_dir().join("ersatzrs-hls").join(channel);
    let deadline = Deadline::from_now(timeout);
    let mut saw_session_dir = path.exists();
    loop {
        if path.exists() {
            saw_session_dir = true;
        } else if saw_session_dir {
            println!(
                "smoke evidence: {label} session {} removed after idle timeout",
                path.display()
            );
            return Ok(());
        } else {
            println!(
                "smoke evidence: {label} session {} was already removed after disconnect",
                path.display()
            );
            return Ok(());
        }

        if deadline.expired() {
            bail!(
                "{label} session {} remained active after {timeout:?}",
                path.display()
            );
        }

        thread::sleep(Duration::from_millis(250));
    }
}

struct HttpResponse {
    status: u16,
    body: Vec<u8>,
}

impl HttpResponse {
    fn ensure_success_with_body(&self, path: &str) -> Result<()> {
        if !(200..300).contains(&self.status) {
            bail!("{path}: HTTP {}", self.status);
        }
        if self.body.is_empty() {
            bail!("{path}: response contained no body");
        }
        Ok(())
    }
}

fn http_get_until_body(
    path: &str,
    min_body_bytes: usize,
    timeout: Duration,
) -> Result<HttpResponse> {
    let addr: SocketAddr = LISTEN_ADDR.parse()?;
    let mut stream = TcpStream::connect_timeout(&addr, timeout)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {LISTEN_ADDR}\r\nConnection: close\r\n\r\n"
    )?;
    stream.flush()?;

    let mut bytes = Vec::new();
    let mut buf = [0_u8; 8192];
    let mut header_end = None;
    loop {
        let n = match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(error)
                if error.kind() == io::ErrorKind::WouldBlock
                    || error.kind() == io::ErrorKind::TimedOut =>
            {
                if header_end.is_some()
                    && bytes
                        .len()
                        .saturating_sub(header_end.unwrap_or(bytes.len()))
                        >= min_body_bytes
                {
                    break;
                }
                return Err(error.into());
            }
            Err(error) => return Err(error.into()),
        };
        bytes.extend_from_slice(&buf[..n]);
        if header_end.is_none() {
            header_end = find_header_end(&bytes);
        }
        if let Some(end) = header_end {
            if bytes.len().saturating_sub(end) >= min_body_bytes {
                break;
            }
        }
    }

    let Some(end) = header_end else {
        bail!("{path}: response did not contain HTTP headers");
    };
    let headers = String::from_utf8_lossy(&bytes[..end]);
    let status = headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| anyhow!("{path}: could not parse HTTP status"))?;
    Ok(HttpResponse {
        status,
        body: bytes[end..].to_vec(),
    })
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
}

fn fail_on_log_signatures(log_path: &Path) -> Result<()> {
    let text = fs::read_to_string(log_path).unwrap_or_default();
    let mut matched = Vec::new();
    for signature in FAILURE_SIGNATURES {
        if text.contains(signature) {
            matched.push(*signature);
        }
    }
    if matched.is_empty() {
        Ok(())
    } else {
        bail!(
            "smoke log contains failure signature(s): {} (log: {})",
            matched.join(", "),
            log_path.display()
        );
    }
}

#[derive(Clone, Copy)]
struct Deadline {
    expires_at: SystemTime,
}

impl Deadline {
    fn from_now(duration: Duration) -> Self {
        Self {
            expires_at: SystemTime::now() + duration,
        }
    }

    fn expired(self) -> bool {
        SystemTime::now() >= self.expires_at
    }

    fn remaining(self) -> Result<Duration> {
        self.expires_at
            .duration_since(SystemTime::now())
            .map(|duration| duration.max(Duration::from_secs(1)))
            .map_err(|_| anyhow!("smoke timeout expired"))
    }
}

fn unix_now() -> Result<Duration> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock before Unix epoch")
}

fn format_timestamp(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let seconds_of_day = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3600;
    let minute = (seconds_of_day % 3600) / 60;
    let second = seconds_of_day % 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}")
}

fn format_duration_hms(seconds: i64) -> String {
    let seconds = seconds.max(0);
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let seconds = seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn print_help() {
    eprintln!("smoke — boot ersatzrs-web and probe runtime IPTV/HLS playback");
    eprintln!();
    eprintln!("USAGE:");
    eprintln!("    cargo xtask smoke [FLAGS] [--channel N]...");
    eprintln!();
    eprintln!("FLAGS:");
    eprintln!("    --channel N       Channel number to probe (default: 1)");
    eprintln!("    --timeout S       Overall startup/probe timeout seconds (default: 90)");
    eprintln!("    --media PATH      Use an existing media file instead of generating one");
    eprintln!("    --hls-lifecycle  Also prove StopOnDisconnect HLS and mode=ts sessions drain");
    eprintln!("    --skip-ts         Skip /iptv/channel/<n>.ts?mode=ts probe");
    eprintln!("    --skip-hls        Skip HLS segmenter playlist probes");
    eprintln!("    --keep-temp       Keep temporary DB/media/log directory for debugging");
}
