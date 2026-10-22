//! SI3: explicit initial discontinuities preserve managed source observations.

use super::{missing_source::source_ids, optional_pes::observations, *};
use std::collections::BTreeSet;

fn initial_flags(original: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(original.len() + 3 * 188);
    let mut seen = BTreeSet::new();
    for raw in original.chunks_exact(188) {
        let pid = (u16::from(raw[1] & 31) << 8) | u16::from(raw[2]);
        if [0, 0x1000, 0x103].contains(&pid) && seen.insert(pid) {
            let counter = (raw[3] - u8::from(raw[3] & 0x10 != 0)) & 15;
            let mut initial = [0xff; 188];
            initial[..6].copy_from_slice(&[0x47, raw[1] & 31, raw[2], 0x20 | counter, 183, 0x80]);
            bytes.extend_from_slice(&initial);
        }
        // Every original packet, including all AV/caption clocks, is unchanged.
        bytes.extend_from_slice(raw);
    }
    assert_eq!(seen, BTreeSet::from([0, 0x1000, 0x103]));
    bytes
}

#[tokio::test]
#[ignore = "requires pinned FFprobe 9 and synthetic two-audio AV/200/888 input"]
async fn native_initial_source_flags_preserve_pages_clocks_tracks_and_identity() {
    let (probe, fixture) = inputs();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("Synthetic.ts");
    std::fs::copy(&fixture, &path).unwrap();
    std::fs::write(
        path.with_extension("nld.srt"),
        "1\n00:00:00,000 --> 00:00:01,000\nSynthetic ordinary caption\n",
    )
    .unwrap();
    let pool = database(root.path(), 4).await;
    let config = config(&pool, &probe).await;
    scan(&pool, &config).await;
    let before_tracks = tracks(&pool, 1).await;
    let before_subtitles = subtitles(&pool, 1).await;
    let before_reports = reports(&pool, 1).await;
    let before = observations(&pool).await;
    let before_ids = source_ids(&pool).await;
    let mut expected_headers = before.headers().unwrap().clone();
    expected_headers.packet_count += 3;
    for header in &mut expected_headers.headers {
        header.completion_packet += 3;
    }
    let mut expected_pages = before.page_window().unwrap().clone();
    for unit in &mut expected_pages.units {
        unit.completion_packet += 3;
    }
    std::fs::write(&path, initial_flags(&std::fs::read(&path).unwrap())).unwrap();
    scan(&pool, &config).await;
    let after = observations(&pool).await;
    assert_page_capture(&after);
    assert_eq!(after.headers(), Some(&expected_headers));
    assert_eq!(after.page_window(), Some(&expected_pages));
    let after_reports = reports_with_packet_count(&pool, 1, 306).await;
    assert_eq!(after_reports[0].0, before_reports[0].0);
    assert_ne!(after_reports[0].1, before_reports[0].1);
    assert_eq!(tracks(&pool, 1).await, before_tracks);
    assert_eq!(subtitles(&pool, 1).await, before_subtitles);
    assert_eq!(source_ids(&pool).await, before_ids);
    pool.close().await;
}
