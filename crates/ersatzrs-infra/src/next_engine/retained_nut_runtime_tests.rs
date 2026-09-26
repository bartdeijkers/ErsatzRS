//! Transport prerequisite only: one-shot, non-seekable HTTP carrying unchanged NUT.
//! The production retained-interval producer and its lifetime are tested separately.
use super::*;
use crate::next_engine::{config, protocol, publication};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::net::{TcpListener, TcpStream};

async fn serve_nut(mut socket: TcpStream, path: PathBuf, requests: Arc<AtomicUsize>) {
    let request = timeout(Duration::from_secs(3), async {
        let mut header = Vec::with_capacity(1024);
        while header.len() < 8192 {
            header.push(socket.read_u8().await?);
            if header.ends_with(b"\r\n\r\n") {
                return Ok::<_, std::io::Error>(header);
            }
        }
        Err(std::io::Error::from(std::io::ErrorKind::InvalidData))
    })
    .await;
    let Ok(Ok(header)) = request else {
        return;
    };
    if !header.starts_with(b"GET /retained.nut HTTP/") {
        return;
    }
    if requests.fetch_add(1, Ordering::SeqCst) != 0 {
        let _ = socket
            .write_all(b"HTTP/1.1 410 Gone\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await;
        return;
    }
    // Deliberately no Content-Length, Accept-Ranges or Content-Range. A probe
    // would consume the producer: probe_hint must prevent that extra request.
    if socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n").await.is_err() { return; }
    let Ok(mut file) = tokio::fs::File::open(path).await else {
        return;
    };
    // Tokio copy has a bounded buffer; no remuxing or whole-body allocation.
    let _ = tokio::io::copy(&mut file, &mut socket).await;
    let _ = socket.shutdown().await;
}

async fn first_segment(output: &Path) -> std::io::Result<PathBuf> {
    timeout(Duration::from_secs(30), async {
        loop {
            if let Ok(playlist) = tokio::fs::read_to_string(output.join("live.m3u8")).await {
                if let Some(name) = playlist.lines().find(|line| {
                    !line.starts_with('#')
                        && Path::new(line)
                            .extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("ts"))
                }) {
                    let path = output.join(name);
                    if path.is_file() {
                        return path;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .map_err(|_| std::io::ErrorKind::TimedOut.into())
}

#[tokio::test]
#[ignore = "requires real Next and FFmpeg 9; non-seekable retained NUT transport prerequisite"]
async fn real_next_decodes_nonseekable_http_nut_and_selected_pcm_audio() {
    let binary = runtime_path("ERSATZRS_NEXT_TEST_BIN");
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let root = tempfile::tempdir().unwrap();
    let (nut, hint) = generate_nut(&ffmpeg, &ffprobe, root.path()).await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let requests = Arc::new(AtomicUsize::new(0));
    let (cancel, mut server_cancel) = tokio::sync::oneshot::channel::<()>();
    let server_requests = requests.clone();
    let server = tokio::spawn(async move {
        let mut connections = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                _ = &mut server_cancel => break,
                connection = listener.accept(), if connections.len() < 8 => {
                    let Ok((socket, _)) = connection else { break; };
                    connections.spawn(serve_nut(socket, nut.clone(), server_requests.clone()));
                }
                _ = connections.join_next(), if !connections.is_empty() => {}
            }
        }
        connections.abort_all();
        while connections.join_next().await.is_some() {}
    });
    let output = root.path().join("output");
    let config = prepare_config(root.path(), address, hint, &ffmpeg, &ffprobe);
    let started = NextProcess::start(
        &binary,
        "1",
        &output,
        &config,
        Instant::now() + Duration::from_secs(30),
    )
    .await;
    let result = match started {
        Ok(mut process) => {
            let segment = first_segment(&output).await;
            // Copy before stop removes output; all assertions/decoding follow reap.
            let saved = root.path().join("proof.ts");
            let captured = match segment {
                Ok(path) => tokio::fs::copy(path, &saved).await.map(|_| saved),
                Err(error) => Err(error),
            };
            let stopped = process.stop().await;
            (captured, stopped)
        }
        Err(error) => {
            let _ = cancel.send(());
            server.await.unwrap();
            panic!("Next NUT startup failed: {error}");
        }
    };
    let _ = cancel.send(());
    server.await.unwrap();
    result.1.unwrap();
    let segment = result
        .0
        .expect("Next must decode non-seekable NUT into an HLS segment");
    assert_eq!(
        requests.load(Ordering::SeqCst),
        1,
        "probe hints must preserve single-consumption producer ownership"
    );
    assert_selected_audio(&ffmpeg, &segment).await;
    assert_blue_video(&ffmpeg, &segment).await;
}

pub(super) async fn assert_selected_audio(ffmpeg: &Path, segment: &Path) {
    let mut decode = Command::new(ffmpeg);
    decode
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(segment)
        .args([
            "-map", "0:a:0", "-t", "1", "-ac", "1", "-ar", "8000", "-f", "s16le", "pipe:1",
        ]);
    let bytes = tool(decode).await;
    let samples: Vec<_> = bytes
        .chunks_exact(2)
        .map(|p| i16::from_le_bytes([p[0], p[1]]))
        .collect();
    assert!(samples.len() >= 7000);
    let crossings = samples.windows(2).filter(|p| p[0] <= 0 && p[1] > 0).count();
    let hz = f64::from(u32::try_from(crossings).unwrap()) * 8000.0
        / f64::from(u32::try_from(samples.len()).unwrap());
    assert!(
        (820.0..940.0).contains(&hz),
        "selected Dutch audio must remain 880Hz: {hz}"
    );
}

pub(super) async fn generate_nut(
    ffmpeg: &Path,
    ffprobe: &Path,
    folder: &Path,
) -> (PathBuf, protocol::ProbeHint) {
    let nut = folder.join("retained.nut");
    let mut generate = Command::new(ffmpeg);
    generate
        .args([
            "-v",
            "error",
            "-nostdin",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=blue:s=160x90:r=10",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:sample_rate=48000",
            "-map",
            "0:v",
            "-map",
            "1:a",
            "-map",
            "2:a",
            "-t",
            "24",
            "-c:v",
            "rawvideo",
            "-threads",
            "1",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "pcm_f32le",
            "-metadata:s:a:0",
            "language=eng",
            "-metadata:s:a:0",
            "title=English",
            "-metadata:s:a:1",
            "language=nld",
            "-metadata:s:a:1",
            "title=Dutch",
            "-f",
            "nut",
        ])
        .arg(&nut);
    tool(generate).await;
    assert!(tokio::fs::metadata(&nut).await.unwrap().len() < 16 * 1024 * 1024);
    let metadata = crate::ffprobe::FfprobeExtractor::new(ffprobe.to_str().unwrap())
        .extract(&nut)
        .await
        .unwrap();
    let hint = crate::next_engine::probe_hint::retained(&metadata, 24_000).unwrap();
    (nut, hint)
}

fn prepare_config(
    folder: &Path,
    address: std::net::SocketAddr,
    hint: protocol::ProbeHint,
    ffmpeg: &Path,
    ffprobe: &Path,
) -> protocol::ChannelConfig {
    let playout = folder.join("playout");
    let start = chrono::Utc::now();
    let source = protocol::Source::Http {
        uri: format!("http://{address}/retained.nut"),
        probe_hint: Some(hint),
        is_live: Some(false),
        headers: Vec::new(),
        user_agent: None,
        keep_alive: Some(false),
        reconnect: Some(false),
        reconnect_delay_max: None,
        timeout_us: Some(5_000_000),
        in_point_ms: None,
        out_point_ms: None,
    };
    publication::publish_playout(
        &playout,
        &protocol::Playout {
            version: protocol::PLAYOUT_VERSION.into(),
            items: vec![protocol::PlayoutItem {
                id: "retained-transport".into(),
                start,
                finish: start + chrono::Duration::seconds(24),
                source: Some(source),
                watermark: None,
                graphics: Vec::new(),
                tracks: Some(protocol::Tracks {
                    video: Some(protocol::Track {
                        source: None,
                        stream_index: Some(0),
                    }),
                    audio: Some(protocol::Track {
                        source: None,
                        stream_index: Some(2),
                    }),
                    subtitle: None,
                }),
            }],
        },
    )
    .unwrap();
    let profile = PlaybackProfile {
        profile: FFmpegProfile {
            video_format: FFmpegProfileVideoFormat::H264,
            audio_format: FFmpegProfileAudioFormat::Aac,
            video_bitrate: 1000,
            video_buffer_size: 2000,
            audio_sample_rate: 48,
            audio_channels: 1,
            ..Default::default()
        },
        resolution: Resolution {
            width: 320,
            height: 180,
            ..Default::default()
        },
    };
    let mut config = config::channel_config(
        &profile,
        ersatzrs_domain::NextEngineTextSubtitleMode::Burn,
        &playout,
        ffmpeg,
        ffprobe,
    )
    .unwrap();
    config.playout.virtual_start = Some(start);
    config
}

pub(super) async fn assert_blue_video(ffmpeg: &Path, segment: &Path) {
    let mut video = Command::new(ffmpeg);
    video
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(segment)
        .args([
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-vf",
            "scale=1:1",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ]);
    let pixels = tool(video).await;
    assert_eq!(pixels.len(), 3);
    assert!(
        pixels[2] > 180 && pixels[0] < 40 && pixels[1] < 40,
        "decoded retained video must remain blue"
    );
}
