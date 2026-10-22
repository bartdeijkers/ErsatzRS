use super::*;
use crate::ffprobe::LocalProbeCleanupScope;
use std::io::SeekFrom;

fn source(format: SourceTeletextPacketFormat) -> Vec<u8> {
    let mut bytes = Vec::new();
    for ordinal in 0..4 {
        if format == SourceTeletextPacketFormat::M2ts192 {
            bytes.extend_from_slice(&[0x20, ordinal, 0xa5, 0x5a]);
        }
        let mut packet = [0x55; TS_BYTES];
        packet[..4].copy_from_slice(&[0x47, 0, ordinal, 0x10]);
        bytes.extend_from_slice(&packet);
    }
    bytes
}

async fn inspect<T, F>(bytes: Vec<u8>, work: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce(&File, &headers::ReadStop<'_, T>) -> Result<T> + Send + 'static,
{
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.bin");
    std::fs::write(&path, bytes).unwrap();
    let file = File::open(&path).unwrap();
    let scope = LocalProbeCleanupScope::new();
    let result = scope
        .scope(headers::run_owned(
            &path,
            std::time::Duration::from_secs(5),
            move |stop| work(&file, stop),
        ))
        .await;
    scope.close();
    scope.wait_cleanup().await.unwrap();
    assert_eq!(scope.cleanup_status(), Ok(true));
    result
}

#[tokio::test]
async fn exact_source_parts_and_cursor_preserving_detection() {
    for format in [
        SourceTeletextPacketFormat::Ts188,
        SourceTeletextPacketFormat::M2ts192,
    ] {
        let bytes = source(format);
        let expected = bytes.clone();
        inspect(bytes, move |file, stop| {
            let mut cursor = file;
            cursor.seek(SeekFrom::Start(11)).unwrap();
            assert_eq!(detect(file, expected.len() as u64, stop)?, format);
            assert_eq!(cursor.stream_position().unwrap(), 11);
            for ordinal in 0..4 {
                let frame = read(file, ordinal, format, stop)?;
                let start = usize::try_from(ordinal).unwrap() * format.packet_bytes();
                let original = &expected[start..start + format.packet_bytes()];
                assert_eq!(frame.parts().concat(), original);
                assert_eq!(frame.parts()[0].len(), format.sync_offset());
                assert_eq!(frame.packet, original[format.sync_offset()..]);
            }
            Ok(())
        })
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn unknown_ambiguous_and_shifted_sources_are_not_guessed_or_resynchronized() {
    let mut ambiguous = vec![0x55; MAX_FRAME_BYTES * PROBE_FRAMES];
    for start in [0, 188, 376, 4, 196, 388] {
        ambiguous[start..start + 4].copy_from_slice(&[0x47, 0, 0, 0x10]);
    }
    let shifted = [
        &[0][..],
        source(SourceTeletextPacketFormat::Ts188).as_slice(),
    ]
    .concat();
    for bytes in [vec![], vec![0; 187], vec![0; 576], ambiguous, shifted] {
        let len = bytes.len() as u64;
        inspect(bytes, move |file, stop| {
            let mut cursor = file;
            cursor.seek(SeekFrom::Start(11)).unwrap();
            assert!(detect(file, len, stop).is_err());
            assert_eq!(cursor.stream_position().unwrap(), 11);
            Ok(())
        })
        .await
        .unwrap();
    }
    // A stale length must also restore the cursor when the prefix read fails.
    inspect(vec![0; TS_BYTES], |file, stop| {
        let mut cursor = file;
        cursor.seek(SeekFrom::Start(11)).unwrap();
        assert!(detect(file, (MAX_FRAME_BYTES * PROBE_FRAMES) as u64, stop).is_err());
        assert_eq!(cursor.stream_position().unwrap(), 11);
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn prefix_detection_never_certifies_a_later_bad_or_partial_frame() {
    for format in [
        SourceTeletextPacketFormat::Ts188,
        SourceTeletextPacketFormat::M2ts192,
    ] {
        for truncate in [false, true] {
            let mut bytes = source(format);
            if truncate {
                bytes.pop();
            } else {
                bytes[3 * format.packet_bytes() + format.sync_offset()] = 0;
            }
            let len = bytes.len() as u64;
            inspect(bytes, move |file, stop| {
                assert_eq!(detect(file, len, stop)?, format);
                assert!(read(file, 3, format, stop).is_err());
                Ok(())
            })
            .await
            .unwrap();
        }
    }
}

#[tokio::test]
async fn exact_frame_start_positions_and_read_arithmetic_remain_bounded() {
    for format in [
        SourceTeletextPacketFormat::Ts188,
        SourceTeletextPacketFormat::M2ts192,
    ] {
        let stride = format.packet_bytes() as u64;
        assert_eq!(format.source_packet_at(0), Some(0));
        assert_eq!(format.source_packet_at(17 * stride), Some(17));
        assert_eq!(format.source_packet_at(17 * stride + 1), None);
        assert_eq!(format.source_packet_at(17 * stride + 4), None);
        inspect(source(format), move |file, stop| {
            assert!(read(file, u64::MAX, format, stop).is_err());
            assert!(read_bytes(file, &mut [0; 2], u64::MAX, stop).is_err());
            let other = match format {
                SourceTeletextPacketFormat::Ts188 => SourceTeletextPacketFormat::M2ts192,
                SourceTeletextPacketFormat::M2ts192 => SourceTeletextPacketFormat::Ts188,
            };
            assert!(read(file, 0, other, stop).is_err());
            Ok(())
        })
        .await
        .unwrap();
    }
}
