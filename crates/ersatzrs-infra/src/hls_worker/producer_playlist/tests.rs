use super::*;
use ersatzrs_ffmpeg::FfmpegRuntimeReport;
use std::{fmt::Write, path::PathBuf, sync::Arc};
use tokio::sync::RwLock;

fn worker(directory: &Path) -> Arc<HlsSessionWorker> {
    HlsSessionWorker::new(
        "synthetic-channel".into(),
        1,
        directory.into(),
        OutputFormat::Hls,
        Vec::new(),
        Arc::new(RwLock::new(FfmpegRuntimeReport::for_tests())),
    )
}

fn playlist(start: u64, durations: &[u32], target: u32, end: bool) -> String {
    let mut body = format!("#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:{target}\n#EXT-X-MEDIA-SEQUENCE:{start}\n");
    for (offset, duration) in durations.iter().enumerate() {
        write!(
            body,
            "#EXTINF:{duration}.000000,\nlive{:06}.ts\n",
            start + offset as u64
        )
        .unwrap();
    }
    if end {
        body.push_str("#EXT-X-ENDLIST\n");
    }
    body
}

async fn segment(directory: &Path, number: u64) -> PathBuf {
    let path = directory.join(format!("live{number:06}.ts"));
    let mut packet = [0xff; TS_PACKET_SIZE];
    packet[..4].copy_from_slice(&[0x47, 0x01, 0x00, 0x10]);
    tokio::fs::write(&path, packet).await.unwrap();
    path
}

async fn producer(helper: &MpegTsProducerPlaylist, body: &str) {
    tokio::fs::write(helper.directory.path().join("producer.m3u8"), body)
        .await
        .unwrap();
}

async fn public(worker: &HlsSessionWorker) -> String {
    tokio::fs::read_to_string(worker.working_dir.join("live.m3u8"))
        .await
        .unwrap()
}

#[tokio::test]
async fn private_mpegts_playlist_appends_closed_segments_once_with_pdt_and_final_tail() {
    let root = tempfile::tempdir().unwrap();
    let worker = worker(root.path());
    let anchor = Utc::now();
    *worker.playlist_start.lock().await = anchor;
    segment(root.path(), 10).await;
    tokio::fs::write(root.path().join("live.m3u8"), playlist(10, &[5], 5, false))
        .await
        .unwrap();
    let helper = MpegTsProducerPlaylist::prepare(root.path(), anchor)
        .await
        .unwrap();
    assert_eq!(helper.options().unwrap().start_number, 11);
    assert!(helper
        .options()
        .unwrap()
        .producer_playlist
        .starts_with(".mpegts-producer-"));
    assert_eq!(helper.start, anchor + chrono::Duration::seconds(5));
    segment(root.path(), 11).await;
    producer(&helper, &playlist(11, &[4], 4, false)).await;
    let previous = public(&worker).await;
    helper.publish(&worker, false).await.unwrap();
    assert_eq!(
        public(&worker).await,
        previous,
        "listed latest segment is not yet closed"
    );
    segment(root.path(), 12).await;
    helper.publish(&worker, false).await.unwrap();
    let first = public(&worker).await;
    assert_eq!(first.matches("#EXTINF:").count(), 2);
    assert!(first.contains("#EXT-X-TARGETDURATION:5\n"));
    assert!(first.contains("#EXT-X-MEDIA-SEQUENCE:10\n"));
    assert!(first.contains(&super::super::format_program_date_time(
        anchor + chrono::Duration::seconds(5)
    )));
    helper.publish(&worker, false).await.unwrap();
    assert_eq!(
        public(&worker).await,
        first,
        "same native list never duplicates its run"
    );
    producer(&helper, &playlist(11, &[4, 2], 4, true)).await;
    helper.publish(&worker, true).await.unwrap();
    let final_list = public(&worker).await;
    assert_eq!(final_list.matches("#EXTINF:").count(), 3);
    assert!(!final_list.contains("#EXT-X-ENDLIST"));
    assert_eq!(
        tokio::fs::read(root.path().join("live000012.ts"))
            .await
            .unwrap()[3]
            & 0x0f,
        2
    );
}

#[tokio::test]
async fn private_mpegts_playlist_keeps_output_on_partial_stale_and_malformed_updates() {
    let root = tempfile::tempdir().unwrap();
    let worker = worker(root.path());
    let anchor = Utc::now();
    *worker.playlist_start.lock().await = anchor;
    let helper = MpegTsProducerPlaylist::prepare(root.path(), anchor)
        .await
        .unwrap();
    segment(root.path(), 0).await;
    segment(root.path(), 1).await;
    producer(&helper, &playlist(0, &[4], 4, false)).await;
    helper.publish(&worker, false).await.unwrap();
    let original = public(&worker).await;
    for body in [
        "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXTINF:4,\n",
        "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXTINF:4,\nlive000",
    ] {
        producer(&helper, body).await;
        helper.publish(&worker, false).await.unwrap();
        assert_eq!(public(&worker).await, original);
        assert!(helper.publish(&worker, true).await.is_err());
    }
    for body in [
        playlist(0, &[3], 4, false),
        playlist(7, &[4], 4, false),
        playlist(0, &[4], 4, false).replace("live000000.ts", "../live000000.ts"),
        playlist(0, &[4], 4, false).replace("4.000000", "NaN"),
    ] {
        producer(&helper, &body).await;
        assert!(helper.publish(&worker, false).await.is_err());
        assert_eq!(public(&worker).await, original);
    }
    producer(&helper, "#EXTM3U\n#EXT-X-TARGETDURATION:4\n").await;
    helper.publish(&worker, false).await.unwrap();
    assert_eq!(
        public(&worker).await,
        original,
        "stale shorter list cannot roll back published output"
    );
}

#[tokio::test]
async fn private_mpegts_playlist_fences_foreign_public_updates_and_invalid_tail_files() {
    let root = tempfile::tempdir().unwrap();
    let worker = worker(root.path());
    let anchor = Utc::now();
    *worker.playlist_start.lock().await = anchor;
    let helper = MpegTsProducerPlaylist::prepare(root.path(), anchor)
        .await
        .unwrap();
    segment(root.path(), 0).await;
    producer(&helper, &playlist(0, &[4], 4, true)).await;
    tokio::fs::write(root.path().join("live.m3u8"), playlist(99, &[4], 4, false))
        .await
        .unwrap();
    assert!(helper.publish(&worker, true).await.is_err());
    assert!(public(&worker).await.contains("live000099.ts"));
    tokio::fs::remove_file(root.path().join("live.m3u8"))
        .await
        .unwrap();
    tokio::fs::write(root.path().join("live000000.ts"), [0x47; 187])
        .await
        .unwrap();
    assert!(helper.publish(&worker, true).await.is_err());
    assert!(!root.path().join("live.m3u8").exists());
}

#[tokio::test]
async fn private_mpegts_playlist_restart_preserves_sequence_anchor_and_directory_owner() {
    let root = tempfile::tempdir().unwrap();
    let worker = worker(root.path());
    let anchor = Utc::now();
    *worker.playlist_start.lock().await = anchor;
    segment(root.path(), 20).await;
    tokio::fs::write(root.path().join("live.m3u8"), playlist(20, &[6], 6, false))
        .await
        .unwrap();
    let first = MpegTsProducerPlaylist::prepare(root.path(), anchor)
        .await
        .unwrap();
    segment(root.path(), 21).await;
    producer(&first, &playlist(21, &[4], 4, true)).await;
    first.publish(&worker, true).await.unwrap();
    let owner = first.directory_owner();
    let private = owner.path().to_owned();
    drop(first);
    assert!(private.exists());
    drop(owner);
    assert!(!private.exists());
    // A leftover file absent from the producer playlist is not reused as this child's close witness.
    segment(root.path(), 24).await;
    assert!(
        MpegTsProducerPlaylist::prepare(root.path(), *worker.playlist_start.lock().await)
            .await
            .is_err()
    );
    tokio::fs::remove_file(root.path().join("live000024.ts"))
        .await
        .unwrap();
    let second = MpegTsProducerPlaylist::prepare(root.path(), *worker.playlist_start.lock().await)
        .await
        .unwrap();
    assert_eq!(second.options().unwrap().start_number, 22);
    assert_eq!(second.start, anchor + chrono::Duration::seconds(10));
    segment(root.path(), 22).await;
    producer(&second, &playlist(22, &[3], 3, true)).await;
    second.publish(&worker, true).await.unwrap();
    let list = public(&worker).await;
    assert_eq!(list.matches("#EXTINF:").count(), 3);
    assert!(list.contains("#EXT-X-TARGETDURATION:6\n"));
    assert!(list.contains(&super::super::format_program_date_time(
        anchor + chrono::Duration::seconds(10)
    )));
}

#[tokio::test]
async fn private_mpegts_playlist_rejects_oversize_inputs_and_native_sequence_overflow() {
    let root = tempfile::tempdir().unwrap();
    let body = vec![b'x'; usize::try_from(MAX_PLAYLIST_BYTES).unwrap() + 1];
    tokio::fs::write(root.path().join("live.m3u8"), body)
        .await
        .unwrap();
    assert!(MpegTsProducerPlaylist::prepare(root.path(), Utc::now())
        .await
        .is_err());
    tokio::fs::remove_file(root.path().join("live.m3u8"))
        .await
        .unwrap();
    segment(root.path(), i32::MAX as u64).await;
    assert!(MpegTsProducerPlaylist::prepare(root.path(), Utc::now())
        .await
        .is_err());
}

#[tokio::test]
async fn private_mpegts_playlist_allows_cleanup_of_older_published_run_files() {
    let root = tempfile::tempdir().unwrap();
    let worker = worker(root.path());
    let anchor = Utc::now();
    *worker.playlist_start.lock().await = anchor;
    let helper = MpegTsProducerPlaylist::prepare(root.path(), anchor)
        .await
        .unwrap();
    for number in 0..4 {
        segment(root.path(), number).await;
    }
    producer(&helper, &playlist(0, &[10, 10, 10], 10, false)).await;
    helper.publish_at(&worker, false, anchor).await.unwrap();
    assert_eq!(public(&worker).await.matches("#EXTINF:").count(), 3);
    for number in 0..2 {
        tokio::fs::remove_file(root.path().join(format!("live{number:06}.ts")))
            .await
            .unwrap();
    }
    for number in 4..8 {
        segment(root.path(), number).await;
    }
    producer(&helper, &playlist(0, &[10; 7], 10, false)).await;
    helper
        .publish_at(&worker, false, anchor + chrono::Duration::seconds(50))
        .await
        .unwrap();
    let current = public(&worker).await;
    assert!(current.contains("#EXT-X-MEDIA-SEQUENCE:2\n"));
    assert!(!current.contains("live000000.ts"));
    assert!(!current.contains("live000001.ts"));
    assert_eq!(current.matches("#EXTINF:").count(), 5);
}
