use super::*;
use axum::{body::Body, http::Request, response::IntoResponse as _, routing::get, Router};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tower::ServiceExt as _;

struct Server {
    address: std::net::SocketAddr,
    task: tokio::task::JoinHandle<()>,
}

impl Server {
    async fn start(app: Router) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await
            .unwrap();
        });
        Self { address, task }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.address)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn plex(clip: &std::path::Path, reads: Arc<AtomicUsize>) -> Server {
    let file = tower_http::services::ServeFile::new(clip);
    let app = Router::new()
        .route("/library/metadata/42", get(|| async {
            axum::Json(serde_json::json!({"MediaContainer":{"Metadata":[{
                "ratingKey":"42", "title":"Remote fixture", "Media":[{
                    "id":1,"duration":8000,"width":320,"height":180,
                    "Part":[{"id":50,"file":"missing-fixture.mkv",
                        "key":"/library/parts/50/file.mkv","Stream":[
                            {"index":0,"streamType":1,"codec":"ffv1","frameRate":"25","width":320,"height":180},
                            {"index":1,"streamType":2,"codec":"pcm_s16le","channels":1,"languageCode":"nld","default":true},
                            {"index":2,"streamType":3,"codec":"srt","languageCode":"eng"}
                        ]}]
                }]
            }]}}))
        }))
        .route("/library/parts/50/file.mkv", get(move |request: Request<Body>| {
            let file = file.clone();
            let reads = reads.clone();
            async move {
                reads.fetch_add(1, Ordering::SeqCst);
                file.oneshot(request).await.unwrap().into_response()
            }
        }))
        .layer(axum::middleware::from_fn(|request: Request<Body>, next: axum::middleware::Next| async move {
            if request.headers().get("X-Plex-Token").is_none_or(|value| value != "synthetic-http-fixture") {
                return StatusCode::UNAUTHORIZED.into_response();
            }
            next.run(request).await
        }));
    Server::start(app).await
}

#[tokio::test]
#[ignore = "requires verified FFmpeg/ffprobe, loopback sockets and isolated application data"]
async fn external_native_remote_http_preserves_subtitles_without_library_rows() {
    let root = tempfile::tempdir().unwrap();
    let (state, _) = fixture(root.path()).await;
    let clip = generate_clip(root.path()).await;
    let reads = Arc::new(AtomicUsize::new(0));
    let plex = plex(&clip, reads.clone()).await;
    sqlx::query(r#"INSERT INTO "PlexConnection" ("Id", "PlexMediaSourceId", "IsActive", "Uri") VALUES (1, 10, 1, ?)"#)
        .bind(plex.url("")).execute(&state.pool).await.unwrap();
    state.pool.execute(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES ('plex.server_auth_token.fixture', 'synthetic-http-fixture')"#)
        .await.unwrap();
    let internal = Server::start(crate::host::router::build_internal_app(state.clone())).await;
    state.next_engine.media.configure(internal.address).unwrap();
    state.internal_http_addr.set(internal.address).unwrap();
    let public = Server::start(crate::host::router::build_streaming_app(state.clone())).await;
    schedule(
        &state.pool,
        root.path(),
        Utc::now(),
        serde_json::json!([{
            "title":"Remote fixture", "serverKey":"Fixture", "duration":8000,
            "plexFile":"/library/parts/50/file.mkv", "ratingKey":"42"
        }]),
    )
    .await;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(25))
        .build()
        .unwrap();
    assert_eq!(
        client
            .get(public.url("/ffmpeg/stream/1"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    let response = client
        .get(internal.url("/ffmpeg/stream/1"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.bytes().await.unwrap();
    assert!(bytes.len() >= 188 * 10);
    let capture = root.path().join("remote.ts");
    std::fs::write(&capture, bytes).unwrap();
    assert!(
        assert_capture(&capture).await > 4,
        "remote subtitles were not burned"
    );
    assert!(
        reads.load(Ordering::SeqCst) >= 2,
        "both subtitle preparation and playback must read through the relay"
    );
    assert_public_transport(&state, &public, root.path(), &client).await;
    assert_hls_transport(&state, &public, root.path(), &client).await;
    let counts: (i64, i64, i64) = sqlx::query_as(
        r#"SELECT
        (SELECT COUNT(*) FROM "MediaItem"),
        (SELECT COUNT(*) FROM "MediaVersion"),
        (SELECT COUNT(*) FROM "PlayoutItem")"#,
    )
    .fetch_one(&state.pool)
    .await
    .unwrap();
    assert_eq!(counts, (0, 0, 0));
}

async fn assert_public_transport(
    state: &AppState,
    public: &Server,
    root: &std::path::Path,
    client: &reqwest::Client,
) {
    schedule(
        &state.pool,
        root,
        Utc::now(),
        serde_json::json!([{
            "title":"Remote fixture", "serverKey":"Fixture", "duration":8000,
            "plexFile":"/library/parts/50/file.mkv", "ratingKey":"42"
        }]),
    )
    .await;
    let guide = client
        .get(public.url("/iptv/xmltv.xml"))
        .send()
        .await
        .unwrap();
    assert_eq!(guide.status(), StatusCode::OK);
    let xml = guide.text().await.unwrap();
    assert!(xml.contains("<programme ") && xml.contains("Remote fixture"));
    assert!(!xml.contains("synthetic-http-fixture") && !xml.contains("/library/parts/"));
    let mut response = client
        .get(public.url("/iptv/channel/1.ts"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[reqwest::header::CONTENT_TYPE],
        "video/mp2t"
    );
    // The public concat transport is continuous. Capture a bounded prefix and
    // disconnect; the finite private programme was checked separately above.
    let mut bytes = Vec::new();
    while bytes.len() < 1024 * 1024 {
        let Some(chunk) = response.chunk().await.unwrap() else {
            break;
        };
        bytes.extend_from_slice(&chunk);
    }
    drop(response);
    assert!(
        bytes.len() >= 188 * 10,
        "public stream produced no programme"
    );
    let capture = root.join("public.ts");
    std::fs::write(&capture, bytes).unwrap();
    assert!(
        assert_capture(&capture).await > 4,
        "public stream lost subtitles"
    );
}

async fn assert_hls_transport(
    state: &AppState,
    public: &Server,
    root: &std::path::Path,
    client: &reqwest::Client,
) {
    schedule(
        &state.pool,
        root,
        Utc::now(),
        serde_json::Value::Array(
            (0..3)
                .map(|_| {
                    serde_json::json!({
                        "title":"Remote fixture", "serverKey":"Fixture", "duration":8000,
                        "plexFile":"/library/parts/50/file.mkv", "ratingKey":"42"
                    })
                })
                .collect(),
        ),
    )
    .await;
    let url = public.url("/iptv/channel/1.m3u8?mode=segmenter");
    let playlist = client.get(&url).send().await.unwrap();
    assert_eq!(playlist.status(), StatusCode::OK);
    let master = playlist.text().await.unwrap();
    assert!(master.contains("#EXT-X-STREAM-INF:"));
    let variant = master
        .lines()
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .unwrap();
    let base = reqwest::Url::parse(&url).unwrap().join(variant).unwrap();
    let playlist = client.get(base.clone()).send().await.unwrap();
    assert_eq!(playlist.status(), StatusCode::OK);
    let text = playlist.text().await.unwrap();
    assert!(
        text.contains("#EXTINF:"),
        "missing HLS media segments: {text}"
    );
    // MPEG-TS HLS normalizes timestamps/counters and intentionally suppresses
    // discontinuity tags. More than eight seconds requires another occurrence.
    let seconds: f64 = text
        .lines()
        .filter_map(|line| line.strip_prefix("#EXTINF:"))
        .map(|value| value.trim_end_matches(',').parse::<f64>().unwrap())
        .sum();
    assert!(
        seconds > 8.0,
        "HLS did not cross an external programme boundary"
    );
    assert!(!text.contains("#EXT-X-DISCONTINUITY\n"));
    let mut bytes = Vec::new();
    for line in text
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let segment = client.get(base.join(line).unwrap()).send().await.unwrap();
        assert_eq!(segment.status(), StatusCode::OK);
        bytes.extend_from_slice(&segment.bytes().await.unwrap());
    }
    state.hls_sessions.stop_channel("1").await;
    let capture = root.join("hls.ts");
    std::fs::write(&capture, bytes).unwrap();
    assert!(
        assert_capture(&capture).await > 4,
        "HLS lost selected subtitles"
    );
}

#[tokio::test]
#[ignore = "requires verified FFmpeg/ffprobe, loopback sockets and isolated application data"]
async fn external_remote_fmp4_hls_preserves_programmes_and_subtitles() {
    let root = tempfile::tempdir().unwrap();
    let (state, _) = fixture(root.path()).await;
    let clip = generate_clip(root.path()).await;
    let plex = plex(&clip, Arc::new(AtomicUsize::new(0))).await;
    sqlx::query(r#"INSERT INTO "PlexConnection" ("Id", "PlexMediaSourceId", "IsActive", "Uri") VALUES (1, 10, 1, ?)"#)
        .bind(plex.url("")).execute(&state.pool).await.unwrap();
    state.pool.execute(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES ('plex.server_auth_token.fixture', 'synthetic-http-fixture')"#)
        .await.unwrap();
    let internal = Server::start(crate::host::router::build_internal_app(state.clone())).await;
    state.next_engine.media.configure(internal.address).unwrap();
    let public = Server::start(crate::host::router::build_streaming_app(state.clone())).await;
    schedule(
        &state.pool,
        root.path(),
        Utc::now(),
        serde_json::Value::Array(
            (0..3)
                .map(|_| {
                    serde_json::json!({
                        "title":"Remote fixture", "serverKey":"Fixture", "duration":8000,
                        "plexFile":"/library/parts/50/file.mkv", "ratingKey":"42"
                    })
                })
                .collect(),
        ),
    )
    .await;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(25))
        .build()
        .unwrap();
    let master_url = public.url("/iptv/channel/1.m3u8?mode=segmenter-fmp4");
    let response = client.get(&master_url).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let master = response.text().await.unwrap();
    let variant = master
        .lines()
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .unwrap();
    let url = reqwest::Url::parse(&master_url)
        .unwrap()
        .join(variant)
        .unwrap();
    let response = client.get(url.clone()).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let playlist = response.text().await.unwrap();
    assert!(playlist.contains("#EXT-X-MAP:"));
    assert!(
        playlist.contains("#EXT-X-DISCONTINUITY\n"),
        "fMP4 programme boundary missing: {playlist}"
    );
    let capture = root.path().join("fmp4.mkv");
    let output = capture_fmp4_prefix(&url, &capture).await;
    state.hls_sessions.stop_channel("1").await;
    assert!(
        output.status.success(),
        "fMP4 HLS client failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        assert_capture(&capture).await > 4,
        "fMP4 lost selected subtitles"
    );
    let probe = tokio::process::Command::new(ersatzrs_ffmpeg::ffprobe_path())
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "csv=p=0",
        ])
        .arg(&capture)
        .kill_on_drop(true)
        .output()
        .await
        .unwrap();
    assert!(probe.status.success());
    let seconds: f64 = String::from_utf8(probe.stdout)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!(
        seconds > 8.0,
        "fMP4 capture did not cross a programme boundary"
    );
}

async fn capture_fmp4_prefix(
    url: &reqwest::Url,
    capture: &std::path::Path,
) -> std::process::Output {
    tokio::time::timeout(
        Duration::from_secs(20),
        tokio::process::Command::new(ersatzrs_ffmpeg::ffmpeg_path())
            .args([
                "-nostdin",
                "-v",
                "error",
                "-live_start_index",
                "0",
                "-i",
                url.as_str(),
                "-t",
                "12",
                "-map",
                "0:v:0",
                "-map",
                "0:a:0",
                "-c",
                "copy",
            ])
            .arg(capture)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .unwrap()
    .unwrap()
}
