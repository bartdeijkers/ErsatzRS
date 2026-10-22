use super::*;

fn adaptation(pid: u16, counter: u8, flag: bool) -> [u8; 188] {
    let [hi, lo] = pid.to_be_bytes();
    let mut raw = [0xff; 188];
    raw[..6].copy_from_slice(&[0x47, hi, lo, 0x20 | counter, 183, u8::from(flag) << 7]);
    raw
}

async fn read(bytes: &[u8]) -> LocalTeletextAcquisition {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.ts");
    std::fs::write(&path, bytes).unwrap();
    let scope = LocalProbeCleanupScope::new();
    let before = acquisition(&path);
    let revision = before.revision().clone();
    let snapshot = before.snapshot().clone();
    let actual = scope.scope(acquire(before, &path)).await.unwrap();
    assert_eq!(actual.revision(), &revision);
    assert_eq!(actual.snapshot(), &snapshot);
    assert!(actual.is_current(&path));
    assert_eq!(scope.cleanup_status(), Ok(true));
    actual
}

#[tokio::test]
async fn fresh_source_initial_flags_preserve_exact_pages_pts_and_psi() {
    let original = fixture();
    let baseline = read(&original).await;
    let mut bytes = Vec::new();
    let mut seen = BTreeSet::new();
    for raw in original.chunks_exact(188) {
        let pid = (u16::from(raw[1] & 31) << 8) | u16::from(raw[2]);
        if seen.insert(pid) {
            // No payload means no counter increment, including an existing
            // adaptation-only first packet in the original fixture.
            let counter = (raw[3] - u8::from(raw[3] & 0x10 != 0)) & 15;
            bytes.extend_from_slice(&adaptation(pid, counter, true));
        }
        bytes.extend_from_slice(raw);
    }
    assert_eq!(seen, BTreeSet::from([0, 0x1000, 0x100]));
    let actual = read(&bytes).await;
    let mut expected = baseline.headers().for_stream(2).unwrap().clone();
    expected.packet_count += 3;
    for header in &mut expected.headers {
        header.completion_packet += 3;
    }
    assert_eq!(actual.headers().for_stream(2), Some(&expected));
    let mut pages = baseline.page_window(2).unwrap().clone();
    for unit in &mut pages.units {
        unit.completion_packet += 3;
    }
    assert_eq!(actual.page_window(2), Some(&pages));
}

#[tokio::test]
async fn selected_packet_before_psi_consumes_initial_allowance_with_or_without_flag() {
    for first_flag in [false, true] {
        let original = fixture();
        let mut bytes = adaptation(0x100, 15, first_flag).to_vec();
        bytes.extend_from_slice(&original[..2 * 188]);
        bytes.extend_from_slice(&adaptation(0x100, 15, true));
        bytes.extend_from_slice(&original[2 * 188..]);
        let actual = read(&bytes).await;
        assert!(matches!(
            actual.headers(),
            LocalTeletextHeaders::NotAcquired(Problem::InvalidTransport)
        ));
        assert!(actual.page_window(2).is_none());
    }
}

#[tokio::test]
async fn later_flag_discards_all_captured_pages_without_losing_probe_metadata() {
    let original = fixture();
    for pid in [0, 0x1000, 0x100] {
        let last = original
            .chunks_exact(188)
            .rev()
            .find(|raw| (u16::from(raw[1] & 31) << 8) | u16::from(raw[2]) == pid)
            .unwrap();
        let mut bytes = original.clone();
        bytes.extend_from_slice(&adaptation(pid, last[3] & 15, true));
        let actual = read(&bytes).await;
        assert!(matches!(
            actual.headers(),
            LocalTeletextHeaders::NotAcquired(Problem::InvalidTransport)
        ));
        assert!(actual.page_window(2).is_none());
    }
}

#[tokio::test]
async fn consumed_held_source_handle_cannot_be_reinterpreted_as_a_new_epoch() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.ts");
    std::fs::write(&path, fixture()).unwrap();
    let scope = LocalProbeCleanupScope::new();
    let first = scope
        .scope(acquire(acquisition(&path), &path))
        .await
        .unwrap();
    assert!(first.headers().for_stream(2).is_some());
    let snapshot = first.snapshot().clone();
    let second = scope.scope(acquire(first, &path)).await.unwrap();
    assert!(matches!(
        second.headers(),
        LocalTeletextHeaders::NotAcquired(Problem::InvalidTransport)
    ));
    assert_eq!(second.snapshot(), &snapshot);
    assert!(second.is_current(&path));
    assert_eq!(scope.cleanup_status(), Ok(true));
}
