use super::super::LocalFileRevision;
use super::*;
use crate::ffprobe::LocalProbeCleanupScope;
use ersatzrs_core::locking::{EntityLocker, TeletextWorkGuard};
use std::time::Duration;

fn fixture() -> Vec<u8> {
    include_str!("../../../../../ersatzrs-ffmpeg/tests/fixtures/teletext-source-assembly.hex")
        .split_whitespace()
        .map(|byte| u8::from_str_radix(byte, 16).unwrap())
        .collect()
}

#[tokio::test]
async fn continuous_hash_preserves_real_prefix_omissions_and_trimmed_tail_identity() {
    let original = fixture();
    for capped in [false, true] {
        let mut bytes = original.clone();
        if capped {
            let mut null = [0xff; 188];
            null[..4].copy_from_slice(&[0x47, 0x1f, 0xff, 0x10]);
            while bytes.len() / 188 < MAX_SOURCE_OBSERVATION_PACKETS - 1 {
                bytes.extend_from_slice(&null);
            }
            let mut pending = original[15 * 188..].to_vec();
            pending[3] = 0x17;
            pending[8..10].copy_from_slice(&362_u16.to_be_bytes());
            bytes.extend_from_slice(&pending);
            bytes.extend_from_slice(&null);
        }
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("synthetic-source.ts");
        std::fs::write(&path, &bytes).unwrap();
        let revision = LocalFileRevision::open(&path).unwrap().revision().unwrap();
        let guard = Arc::new(LocalTeletextSourceGuard::open(&path, &revision).unwrap());
        let locker = EntityLocker::new();
        let work = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
        let scope = LocalProbeCleanupScope::new();
        let deadline = Instant::now() + Duration::from_secs(5);
        let window = scope
            .scope(source_window::capture_source_window_owned(
                guard.clone(),
                path.clone(),
                path.clone(),
                0x100,
                1,
                deadline,
                work.hold_work(),
            ))
            .await
            .unwrap();
        assert_eq!(window.evidence.skipped_selected_prefix_packets, 1);
        assert_eq!(window.evidence.decoder_packets, 15);
        assert_eq!(
            window.evidence.trimmed_source_tail_packets,
            if capped { 32_752 } else { 0 }
        );
        let observed = scope
            .scope(observe_owned(
                guard,
                (path.clone(), path),
                0x100,
                window.evidence,
                deadline,
                work.hold_work(),
            ))
            .await
            .unwrap();
        let expected: [u8; 32] = Sha256::digest(&bytes).into();
        assert_eq!(observed.source_sha256(), expected);
        assert_eq!(
            observed.clock().packets_seen,
            u64::try_from(bytes.len() / 188).unwrap()
        );
        drop(window);
        scope.close();
        scope.wait_cleanup().await.unwrap();
        drop(work);
        assert_eq!(locker.active_teletext_work(), 0);
    }
}
