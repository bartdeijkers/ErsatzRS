//! Actual scan of two program definitions carried on one physical PMT PID.

use super::missing_source::source_ids;
use super::*;
use ersatzrs_domain::{SourceTeletextObservation, SourceTeletextPrograms};

fn shared_source(original: &[u8]) -> Vec<u8> {
    // Independently fixed MPEG-CRC vectors: PAT1/2->1000, PMT1/v0 and PMT2/v3.
    // The first PMT is exact for the existing synthetic two-audio source.
    let sections =
        include_str!("../../../ersatzrs-ffmpeg/tests/fixtures/teletext-shared-pmt-psi.hex")
            .lines()
            .map(|line| {
                line.split_whitespace()
                    .map(|byte| u8::from_str_radix(byte, 16).unwrap())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
    assert_eq!(sections.iter().map(Vec::len).collect::<Vec<_>>(), [20, 120]);
    let mut bytes = original.to_vec();
    let mut changed = [0; 2];
    for packet in bytes.chunks_exact_mut(188) {
        let pid = (u16::from(packet[1] & 31) << 8) | u16::from(packet[2]);
        let index = match pid {
            0 => 0,
            0x1000 => 1,
            _ => continue, // AV, captions, clocks and physical positions stay exact.
        };
        assert_eq!(
            (packet[1] & 0x40, packet[3] & 0x30, packet[4]),
            (0x40, 0x10, 0)
        );
        if index == 1 {
            assert_eq!(&packet[5..65], &sections[1][..60]);
        }
        packet[5..].fill(0xff);
        packet[5..5 + sections[index].len()].copy_from_slice(&sections[index]);
        changed[index] += 1;
    }
    assert_eq!(changed, [16, 16]);
    bytes
}

async fn report(pool: &SqlitePool) -> SourceTeletextObservation {
    let (_, version, file) = source_ids(pool).await;
    let stream: i64 = sqlx::query_scalar(
        r#"SELECT "Id" FROM "MediaStream" WHERE "MediaVersionId"=? AND "Codec"='dvb_teletext'"#,
    )
    .bind(version)
    .fetch_one(pool)
    .await
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    let report = teletext_source::load(&mut tx, stream, file)
        .await
        .unwrap()
        .unwrap();
    let SourceTeletextBinding::Bound(binding) = &report.binding else {
        panic!("managed native report must bind to the current stream row")
    };
    assert_eq!(binding.media_stream_id, stream);
    report
}

fn assert_programs(observation: &SourceTeletextObservation) {
    let SourceTeletextPrograms::Reported(programs) = &observation.programs else {
        panic!("pinned native probe must report both program memberships")
    };
    assert_eq!(
        programs
            .iter()
            .map(|p| (p.program_number, p.pmt_pid, p.pcr_pid))
            .collect::<Vec<_>>(),
        [(1, 0x1000, 0x100), (2, 0x1000, 0x100)]
    );
    let headers = observation
        .observed_pages
        .headers()
        .expect("complete shared-PMT source read");
    assert_eq!(
        headers
            .programs
            .iter()
            .map(|p| (p.program_number, p.pmt_pid, p.pmt_version))
            .collect::<Vec<_>>(),
        [(1, 0x1000, 0), (2, 0x1000, 3)]
    );
    assert_eq!(
        (
            headers.packet_count,
            headers.complete_pes,
            headers.headers.len()
        ),
        (303, 14, 8)
    );
    assert!(headers.ended_at_eof);
    assert_page_capture(&observation.observed_pages);
}

#[tokio::test]
#[ignore = "requires pinned FFprobe 9 and synthetic two-audio AV/200/888 input"]
async fn native_shared_pmt_scan_preserves_both_programs_pages_and_ordinary_tracks() {
    let (probe, fixture) = inputs();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("Synthetic.ts");
    std::fs::write(&path, shared_source(&std::fs::read(fixture).unwrap())).unwrap();
    std::fs::write(
        path.with_extension("nld.srt"),
        "1\n00:00:00,000 --> 00:00:01,000\nOrdinary caption\n",
    )
    .unwrap();
    let pool = database(root.path(), 4).await;
    let config = config(&pool, &probe).await;
    scan(&pool, &config).await;
    let mut before = report(&pool).await;
    assert_programs(&before);
    let before_tracks = tracks(&pool, 1).await;
    let before_subtitles = subtitles(&pool, 1).await;
    let before_ids = source_ids(&pool).await;
    scan(&pool, &config).await;
    let after = report(&pool).await;
    // replace_streams deliberately replaces SQL rows on every deep scan.
    // Verify the fresh binding above, then compare every remaining field.
    let (SourceTeletextBinding::Bound(previous), SourceTeletextBinding::Bound(current)) =
        (&mut before.binding, &after.binding)
    else {
        panic!("managed source bindings must remain explicit")
    };
    assert_ne!(previous.media_stream_id, current.media_stream_id);
    previous.media_stream_id = current.media_stream_id;
    assert_eq!(after, before);
    assert_eq!(tracks(&pool, 1).await, before_tracks);
    assert_eq!(subtitles(&pool, 1).await, before_subtitles);
    assert_eq!(source_ids(&pool).await, before_ids);
    pool.close().await;
}
