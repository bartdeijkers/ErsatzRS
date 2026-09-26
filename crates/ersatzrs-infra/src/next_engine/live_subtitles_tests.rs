use super::super::{dynamic_projection::Identity, protocol, selection::DeferredProjection};
use super::*;

fn snapshot(path: &std::path::Path) -> Snapshot {
    let start = Utc::now();
    Snapshot {
        identity: Identity {
            descriptor: "synthetic-publication".into(),
            cache_key: "unused".into(),
        },
        prepared_remote: None,
        producer_remote: None,
        retained_remote: None,
        projection: DeferredProjection {
            pending_burn: None,
            selected_subtitle: Some(crate::playback::PlaybackSubtitle {
                id: 1,
                subtitle_kind: crate::metadata::subtitle_kind::SIDECAR,
                title: None,
                stream_index: 0,
                codec: Some("subrip".into()),
                default: true,
                forced: false,
                sdh: false,
                language: Some("nld".into()),
                is_extracted: false,
                path: Some(path.to_str().unwrap().into()),
            }),
            item: protocol::PlayoutItem {
                id: "fixture".into(),
                start,
                finish: start + chrono::Duration::seconds(60),
                source: None,
                watermark: None,
                graphics: Vec::new(),
                tracks: Some(protocol::Tracks {
                    video: Some(protocol::Track {
                        source: Some(Source::Rtsp {
                            uri: "rtsp://example.test/live".into(),
                            timeout_us: None,
                            probe_hint: None,
                        }),
                        stream_index: Some(0),
                    }),
                    audio: None,
                    subtitle: Some(protocol::Track {
                        source: Some(Source::Local {
                            path: path.to_str().unwrap().into(),
                            in_point_ms: Some(1000),
                            out_point_ms: None,

                            probe_hint: None,
                        }),
                        stream_index: Some(0),
                    }),
                }),
            },
        },
    }
}

const CUES: &str =
    "1\n00:00:00,000 --> 00:00:01,234\nEXPIRED\n\n2\n00:00:01,000 --> 00:00:02,345\nACTIVE\n";

#[tokio::test]
async fn live_caption_cache_binds_content_and_fractional_join_without_second_seek() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("captions.srt");
    tokio::fs::write(&path, CUES).await.unwrap();
    let cache = BurnCache::new(root.path().join("cache"));
    let original = snapshot(&path);
    let mut first = snapshot(&path);
    first.projection.item.start = original.projection.item.start;
    let now = first.projection.item.start + chrono::Duration::milliseconds(234);
    assert!(prepare(
        &cache,
        &mut first,
        now,
        Instant::now() + std::time::Duration::from_secs(5)
    )
    .await
    .unwrap());
    assert_eq!(first.projection.item.start, now);
    let track = first
        .projection
        .item
        .tracks
        .as_ref()
        .unwrap()
        .subtitle
        .as_ref()
        .unwrap();
    let Some(Source::Local {
        path: artifact,
        in_point_ms: Some(0),
        out_point_ms: None,

        probe_hint: None,
    }) = &track.source
    else {
        panic!("rebased source must start at zero");
    };
    let content = tokio::fs::read_to_string(artifact).await.unwrap();
    assert!(!content.contains("EXPIRED"));
    assert!(content.contains("00:00:00,000 --> 00:00:01,111\nACTIVE"));
    assert_eq!(
        first
            .projection
            .selected_subtitle
            .as_ref()
            .unwrap()
            .path
            .as_ref(),
        Some(artifact)
    );
    let mut changed = original;
    tokio::fs::write(&path, CUES.replace("ACTIVE", "REPLACED"))
        .await
        .unwrap();
    prepare(
        &cache,
        &mut changed,
        now,
        Instant::now() + std::time::Duration::from_secs(5),
    )
    .await
    .unwrap();
    let changed_path = changed.projection.selected_subtitle.unwrap().path.unwrap();
    assert_ne!(&changed_path, artifact);
    assert!(tokio::fs::read_to_string(changed_path)
        .await
        .unwrap()
        .contains("REPLACED"));
}

#[tokio::test]
async fn ended_cues_are_removed_but_invalid_captions_fail_closed() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("captions.srt");
    let cache = BurnCache::new(root.path().join("cache"));
    tokio::fs::write(&path, CUES).await.unwrap();
    let mut expired = snapshot(&path);
    let now = expired.projection.item.start + chrono::Duration::seconds(2);
    prepare(
        &cache,
        &mut expired,
        now,
        Instant::now() + std::time::Duration::from_secs(5),
    )
    .await
    .unwrap();
    assert!(expired.projection.selected_subtitle.is_none());
    assert!(expired.projection.item.tracks.unwrap().subtitle.is_none());
    tokio::fs::write(&path, "invalid cues").await.unwrap();
    let mut invalid = snapshot(&path);
    let now = invalid.projection.item.start;
    assert!(prepare(
        &cache,
        &mut invalid,
        now,
        Instant::now() + std::time::Duration::from_secs(5)
    )
    .await
    .is_err());
    assert!(invalid.projection.selected_subtitle.is_some());
    assert!(!root.path().join("cache").exists());
}

#[tokio::test]
async fn live_caption_reads_reject_oversized_files_and_deadlines() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("captions.srt");
    let file = tokio::fs::File::create(&path).await.unwrap();
    file.set_len(MAX_BYTES as u64 + 1).await.unwrap();
    assert!(read(path.to_str().unwrap()).await.is_err());
    assert!(read(root.path().to_str().unwrap()).await.is_err());
    let cache = BurnCache::new(root.path().join("cache"));
    let mut source = snapshot(&path);
    let now = source.projection.item.start;
    assert!(prepare(&cache, &mut source, now, Instant::now())
        .await
        .is_err());
    assert!(!root.path().join("cache").exists());
}

#[tokio::test]
async fn oversized_or_stalled_http_captions_close_the_connection() {
    use tokio::io::AsyncWriteExt as _;
    for advertised in [MAX_BYTES + 1, 100] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let uri = format!("http://{}/captions.srt", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut header = Vec::new();
            while !header.ends_with(b"\r\n\r\n") {
                assert!(header.len() < 8192);
                header.push(socket.read_u8().await.unwrap());
            }
            socket
                .write_all(
                    format!("HTTP/1.1 200 OK\r\nContent-Length: {advertised}\r\n\r\n").as_bytes(),
                )
                .await
                .unwrap();
            let mut byte = [0];
            let closed =
                tokio::time::timeout(std::time::Duration::from_secs(5), socket.read(&mut byte))
                    .await
                    .unwrap();
            assert!(matches!(closed, Ok(0)) || closed.is_err());
        });
        let result = tokio::time::timeout(std::time::Duration::from_secs(2), read(&uri)).await;
        if advertised > MAX_BYTES {
            assert!(result.unwrap().is_err());
        } else {
            assert!(result.is_err());
        }
        server.await.unwrap();
    }
}
