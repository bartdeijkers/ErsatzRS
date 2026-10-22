//! SI3: legal optional PES fields preserve the real scan's source observations.

use super::missing_source::source_ids;
use super::*;

fn with_optional_headers(original: &[u8]) -> Vec<u8> {
    let mut changed = original.to_vec();
    let mut headers = 0;
    for (before, after) in original
        .chunks_exact(188)
        .zip(changed.chunks_exact_mut(188))
    {
        let pid = (u16::from(before[1] & 31) << 8) | u16::from(before[2]);
        if pid != 0x103 || before[1] & 0x40 == 0 {
            continue;
        }
        assert_eq!(before[3] & 0x30, 0x10);
        let pes = &mut after[4..];
        assert_eq!(&pes[..4], &[0, 0, 1, 0xbd]);
        assert_eq!(&pes[7..9], &[0x80, 36]);
        let pts = pes[9..14].to_vec();
        // Preserve the source clock. Copy/private data need no invented clock,
        // previous-packet CRC, sequence counter or decoder buffer assertion.
        let mut fields = pts;
        fields.extend([0x80, 0x8e]); // Copy info and private-data extension.
        fields.extend([0x55; 16]);
        pes[7] = 0x85;
        pes[9..45].fill(0xff);
        pes[9..9 + fields.len()].copy_from_slice(&fields);
        // Entire PSI, AV, teletext units and physical packet positions stay exact.
        assert_eq!(&after[..11], &before[..11]);
        assert_eq!(&after[49..], &before[49..]);
        headers += 1;
    }
    assert_eq!(headers, 14);
    changed
}

pub(super) async fn observations(pool: &SqlitePool) -> SourceTeletextObservedPages {
    let (_, version, file) = source_ids(pool).await;
    let stream: i64 = sqlx::query_scalar(
        r#"SELECT "Id" FROM "MediaStream" WHERE "MediaVersionId"=? AND "Codec"='dvb_teletext'"#,
    )
    .bind(version)
    .fetch_one(pool)
    .await
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    teletext_source::load(&mut tx, stream, file)
        .await
        .unwrap()
        .unwrap()
        .observed_pages
}

#[tokio::test]
#[ignore = "requires pinned FFprobe 9 and synthetic two-audio AV/200/888 input"]
async fn native_optional_pes_fields_preserve_pages_clocks_tracks_and_rescan_identity() {
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
    let before_observed = observations(&pool).await;
    let before_ids = source_ids(&pool).await;
    let original = std::fs::read(&path).unwrap();
    std::fs::write(&path, with_optional_headers(&original)).unwrap();
    scan(&pool, &config).await;
    let after_reports = reports(&pool, 1).await;
    assert_eq!(after_reports[0].0, before_reports[0].0);
    assert_ne!(after_reports[0].1, before_reports[0].1);
    assert_eq!(observations(&pool).await, before_observed);
    assert_eq!(tracks(&pool, 1).await, before_tracks);
    assert_eq!(subtitles(&pool, 1).await, before_subtitles);
    assert_eq!(source_ids(&pool).await, before_ids);
    pool.close().await;
}
