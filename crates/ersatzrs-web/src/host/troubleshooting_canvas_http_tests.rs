//! Unix-only process observation uses a disposable POSIX wrapper around the pinned `FFmpeg`.
//! It records only this fixture's PID/cwd; rendering still runs the published executable.
use super::*;
use axum::http::{HeaderMap, HeaderName, StatusCode};
use ersatzrs_infra::next_engine::protocol::Source;
use std::os::unix::fs::PermissionsExt as _;
use std::process::Stdio;

#[tokio::test]
#[ignore = "requires pinned FFmpeg; real private HTTP canvas lifecycle"]
async fn canvas_http_decodes_alpha_and_reaps_on_disconnect_and_session_stop() {
    let fixture = Fixture::new().await;
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    ersatzrs_ffmpeg::set_resolved_ffmpeg_tools(ffmpeg.to_str().unwrap(), ffprobe.to_str().unwrap());
    let wrapper = wrapper(fixture.root.path(), &ffmpeg).await;
    {
        let mut runtime = fixture.state.ffmpeg_runtime.write().await;
        runtime.ffmpeg.path = wrapper.to_str().unwrap().into();
        runtime.ffprobe.path = ffprobe.to_str().unwrap().into();
    }
    let mut query = Fixture::query();
    query.seek_seconds = Some(0);
    let origin = remote_runtime::Origin::start(&fixture, remote_runtime::Mode::Local).await;
    canvas_runtime::install(&fixture, &mut query).await;
    let paths = fixture.paths();
    let mut session = None;
    let (manifest, _, _, _) = prepare(
        &fixture.state,
        &query,
        &paths,
        Instant::now() + Duration::from_secs(30),
        &mut session,
    )
    .await
    .unwrap();
    let mut files = tokio::fs::read_dir(manifest.path()).await.unwrap();
    let path = files.next_entry().await.unwrap().unwrap().path();
    let playout: protocol::Playout =
        serde_json::from_slice(&tokio::fs::read(path).await.unwrap()).unwrap();
    let source = &playout.items[0].graphics[0].source;
    let client = new_client();
    let response = request(&client, source, "2500", "1000").await;
    let bytes = response.bytes().await.unwrap();
    assert!(bytes.len() < 4 * 1024 * 1024);
    let first = producer(fixture.root.path()).await;
    reaped(&first).await;
    let nut = fixture.root.path().join("canvas.nut");
    tokio::fs::write(&nut, bytes).await.unwrap();
    assert_alpha(&ffmpeg, &nut).await;

    // A 30-second producer cannot finish naturally within the cleanup deadline.
    // Observe disconnect cleanup before revoking the session.
    let mut response = request(&client, source, "0", "30000").await;
    assert!(!response.chunk().await.unwrap().unwrap().is_empty());
    let disconnected = producer(fixture.root.path()).await;
    assert!(disconnected.directory.exists());
    assert!(alive(&disconnected.pid).await);
    drop(response);
    drop(client);
    reaped(&disconnected).await;

    // Keep the HTTP body open while session shutdown owns cancellation/reaping.
    let client = new_client();
    let mut response = request(&client, source, "0", "30000").await;
    assert!(!response.chunk().await.unwrap().unwrap().is_empty());
    let stopped = producer(fixture.root.path()).await;
    assert!(alive(&stopped.pid).await);
    fixture
        .state
        .next_engine
        .finish_troubleshooting_session(session.unwrap())
        .await
        .unwrap();
    reaped(&stopped).await;
    drop(response);
    origin.verify_revoked_and_stop().await;
}

fn new_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap()
}

async fn request(
    client: &reqwest::Client,
    source: &Source,
    offset: &str,
    duration: &str,
) -> reqwest::Response {
    let Source::Http { uri, headers, .. } = source else {
        panic!("canvas HTTP source")
    };
    let mut map = HeaderMap::new();
    for header in headers {
        let (name, value) = header.split_once(": ").unwrap();
        map.insert(name.parse::<HeaderName>().unwrap(), value.parse().unwrap());
    }
    let response = client
        .get(uri)
        .headers(map)
        .header("x-etv-offset-ms", offset)
        .header("x-etv-duration-ms", duration)
        .header("x-etv-frame-rate", "4")
        .header("connection", "close")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    response
}

async fn wrapper(root: &FsPath, ffmpeg: &FsPath) -> PathBuf {
    let quoted = |path: &FsPath| format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"));
    let script = format!(
        "#!/bin/sh\nset -eu\nprintf '%s\\n%s\\n' \"$$\" \"$PWD\" > {}\nexec {} \"$@\"\n",
        quoted(&root.join("canvas-process")),
        quoted(ffmpeg)
    );
    let path = root.join("ffmpeg-fixture.sh");
    tokio::fs::write(&path, script).await.unwrap();
    tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
        .await
        .unwrap();
    path
}

struct Producer {
    pid: String,
    directory: PathBuf,
}

async fn producer(root: &FsPath) -> Producer {
    let log = tokio::fs::read_to_string(root.join("canvas-process"))
        .await
        .unwrap();
    let mut lines = log.lines();
    let pid = lines.next().unwrap().to_owned();
    assert!(pid.parse::<u32>().unwrap() > 0);
    let directory = PathBuf::from(lines.next().unwrap());
    assert!(directory
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("ersatzrs-canvas-"));
    Producer { pid, directory }
}

async fn alive(pid: &str) -> bool {
    Command::new("kill")
        .args(["-0", pid])
        .stderr(Stdio::null())
        .status()
        .await
        .unwrap()
        .success()
}

async fn reaped(producer: &Producer) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while producer.directory.exists() || alive(&producer.pid).await {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("canvas process and generated artifacts must be removed");
}

async fn assert_alpha(ffmpeg: &FsPath, nut: &FsPath) {
    let mut command = Command::new(ffmpeg);
    command
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(nut)
        .args(["-pix_fmt", "bgra", "-f", "rawvideo", "pipe:1"]);
    let decoded = output(command).await;
    assert_eq!(decoded.len(), 320 * 180 * 4 * 4);
    for frame in decoded.chunks_exact(320 * 180 * 4) {
        let pixel = |x: usize, y: usize| &frame[(y * 320 + x) * 4..(y * 320 + x) * 4 + 4];
        assert_eq!(pixel(80, 24)[3], 0, "background must be transparent");
        assert_eq!(pixel(16, 16), [0, 0, 255, 255], "script alpha");
        assert_eq!(pixel(32, 32), [0, 255, 0, 255], "image alpha and order");
        let translucent = pixel(56, 32);
        assert!(
            translucent[1] > 240 && (126..=129).contains(&translucent[3]),
            "straight image alpha: {translucent:?}"
        );
        assert!(pixel(136, 8)[3] > 240, "motion alpha");
        assert!(pixel(228, 16)[3] > 240, "ASS alpha");
        for (left, right) in [(8, 50), (128, 210)] {
            let visible = (60..90)
                .flat_map(|y| (left..right).map(move |x| pixel(x, y)))
                .filter(|pixel| pixel[3] > 200)
                .count();
            assert!(visible > 10, "text backend alpha");
        }
    }
}

#[tokio::test]
#[ignore = "requires pinned FFmpeg; decoded channel/mirror preview canvas geometry"]
async fn canvas_http_channel_and_mirror_geometry_uses_the_pinned_version() {
    use ersatzrs_domain::{ChannelPlayoutSource, ScalingBehavior};
    for mirrored in [false, true] {
        let fixture = Fixture::new().await;
        let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
        let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
        {
            let mut runtime = fixture.state.ffmpeg_runtime.write().await;
            runtime.ffmpeg.path = ffmpeg.to_str().unwrap().into();
            runtime.ffprobe.path = ffprobe.to_str().unwrap().into();
        }
        sqlx::raw_sql(r#"
            UPDATE "Resolution" SET "Width"=64,"Height"=64 WHERE "Id"=1;
            UPDATE "MediaVersion" SET "Width"=64,"Height"=64 WHERE "Id"=100;
            UPDATE "MediaVersion" SET "Width"=64,"Height"=32,"SampleAspectRatio"='1:1',"RFrameRate"='4/1' WHERE "Id"=101;
        "#).execute(&fixture.state.pool).await.unwrap();
        geometry_graphic(&fixture).await;
        let mut query = Fixture::query();
        query.channel = Some(1);
        let mut at = fixture.at;
        if mirrored {
            let mut channel = fixture
                .state
                .repositories
                .channel
                .get_with_profile(1)
                .await
                .unwrap();
            channel.id = 2;
            channel.number = "2".into();
            channel.playout_source = ChannelPlayoutSource::Mirror;
            channel.mirror_source_channel_id = Some(1);
            channel.playout_offset = Some(TimeDelta::hours(1));
            fixture
                .state
                .repositories
                .channel
                .upsert(&channel)
                .await
                .unwrap();
            query.channel = Some(2);
            at += TimeDelta::hours(1);
        }
        query.start = Some(at.to_rfc3339());
        let before: Vec<(i64, String, String)> =
            sqlx::query_as(r#"SELECT "Id","Start","Finish" FROM "PlayoutItem""#)
                .fetch_all(&fixture.state.pool)
                .await
                .unwrap();
        let origin = remote_runtime::Origin::start(&fixture, remote_runtime::Mode::Local).await;
        for scaling in [
            ScalingBehavior::ScaleAndPad,
            ScalingBehavior::Stretch,
            ScalingBehavior::Crop,
        ] {
            fixture
                .state
                .repositories
                .ffmpeg_profile
                .upsert(&FFmpegProfile {
                    id: 2,
                    resolution_id: 1,
                    scaling_behavior: scaling,
                    audio_sample_rate: 48,
                    video_format: ersatzrs_domain::FFmpegProfileVideoFormat::H264,
                    audio_format: ersatzrs_domain::FFmpegProfileAudioFormat::Aac,
                    ..Default::default()
                })
                .await
                .unwrap();
            geometry_chunk(
                &fixture,
                &query,
                &ffmpeg,
                scaling == ScalingBehavior::ScaleAndPad,
            )
            .await;
        }
        origin.verify_revoked_and_stop().await;
        let after: Vec<(i64, String, String)> =
            sqlx::query_as(r#"SELECT "Id","Start","Finish" FROM "PlayoutItem""#)
                .fetch_all(&fixture.state.pool)
                .await
                .unwrap();
        assert_eq!(
            before, after,
            "canvas preview must not change the persisted source timeline"
        );
    }
}

async fn geometry_chunk(
    fixture: &Fixture,
    query: &PlaybackPreviewQuery,
    ffmpeg: &FsPath,
    padded: bool,
) {
    let mut session = None;
    let (manifest, _, _, _) = prepare(
        &fixture.state,
        query,
        &fixture.paths(),
        Instant::now() + Duration::from_secs(30),
        &mut session,
    )
    .await
    .unwrap();
    let mut files = tokio::fs::read_dir(manifest.path()).await.unwrap();
    let path = files.next_entry().await.unwrap().unwrap().path();
    let playout: protocol::Playout =
        serde_json::from_slice(&tokio::fs::read(path).await.unwrap()).unwrap();
    let source = &playout.items[0].graphics[0].source;
    let Source::Http {
        uri,
        probe_hint: Some(hint),
        ..
    } = source
    else {
        panic!("hinted canvas")
    };
    assert_eq!((hint.video[0].width, hint.video[0].height), (64, 64));
    let url = reqwest::Url::parse(uri).unwrap();
    let binding: ersatzrs_infra::next_engine::canvas::CanvasBinding = serde_json::from_str(
        &url.query_pairs()
            .find(|(key, _)| key == "binding")
            .unwrap()
            .1,
    )
    .unwrap();
    assert_eq!(
        binding.version, 101,
        "pinned version supplies content geometry"
    );
    assert_eq!(binding.video_stream_index, Some(0));
    assert_eq!(binding.channel, query.channel.unwrap());
    assert_eq!(
        binding.in_point_ms, 30_000,
        "preview seek includes the historical join offset"
    );
    let response = request(&new_client(), source, "0", "1000").await;
    let nut = fixture.root.path().join("geometry.nut");
    tokio::fs::write(&nut, response.bytes().await.unwrap())
        .await
        .unwrap();
    let mut command = Command::new(ffmpeg);
    command
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(&nut)
        .args(["-pix_fmt", "bgra", "-f", "rawvideo", "pipe:1"]);
    let decoded = output(command).await;
    assert_eq!(decoded.len(), 64 * 64 * 4 * 4);
    for frame in decoded.chunks_exact(64 * 64 * 4) {
        let pixel = |x: usize, y: usize| &frame[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
        let mark = pixel(4, if padded { 20 } else { 4 });
        assert!(
            mark[2] > 240 && (126..=129).contains(&mark[3]),
            "position and straight alpha: {mark:?}"
        );
        assert_eq!(pixel(4, if padded { 4 } else { 20 })[3], 0);
        assert_eq!(pixel(60, 60)[3], 0);
    }
    fixture
        .state
        .next_engine
        .finish_troubleshooting_session(session.unwrap())
        .await
        .unwrap();
}

async fn geometry_graphic(fixture: &Fixture) {
    use ersatzrs_core::repositories::GraphicsRepository as _;
    use ersatzrs_domain::{
        GraphicsElement, GraphicsElementConfig, ImageGraphicsConfig, WatermarkLocation,
    };
    let image = fixture.root.path().join("geometry.ppm");
    let mut bytes = b"P6\n8 8\n255\n".to_vec();
    bytes.extend([255, 0, 0].repeat(64));
    tokio::fs::write(&image, bytes).await.unwrap();
    let config = GraphicsElementConfig::Image(ImageGraphicsConfig {
        image: Some(image.to_str().unwrap().into()),
    });
    fixture
        .state
        .repositories
        .graphics
        .upsert_element(&GraphicsElement {
            id: 901,
            name: "Content box".into(),
            kind: config.kind(),
            config,
            location: WatermarkLocation::TopLeft,
            place_within_source_content: true,
            opacity_percent: Some(50),
            ..Default::default()
        })
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "PlayoutItemGraphicsElement" ("PlayoutItemId","GraphicsElementId") VALUES (1,901)"#)
        .execute(&fixture.state.pool).await.unwrap();
}
