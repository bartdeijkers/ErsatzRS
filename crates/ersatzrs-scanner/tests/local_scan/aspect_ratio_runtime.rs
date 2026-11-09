//! Reuse the bounded real-runtime fixture for initial and corrective local scans.
use super::*;

async fn generate_anamorphic_fixture(tools: &Tools, root: &Path) -> PathBuf {
    let library = root.join("library");
    tokio::fs::create_dir(&library).await.unwrap();
    let subtitle = root.join("subtitles.srt");
    tokio::fs::write(
        &subtitle,
        "1\n00:00:00,000 --> 00:00:02,000\nDutch anamorphic proof\n",
    )
    .await
    .unwrap();
    let clip = library.join("anamorphic.mkv");
    tool(
        Command::new(&tools.ffmpeg)
            .args([
                "-v",
                "error",
                "-nostdin",
                "-f",
                "lavfi",
                "-i",
                "color=c=blue:s=720x480:r=25",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:sample_rate=48000",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=880:sample_rate=48000",
                "-i",
            ])
            .arg(&subtitle)
            .args([
                "-map",
                "0:v",
                "-map",
                "1:a",
                "-map",
                "2:a",
                "-map",
                "3:s",
                "-t",
                "3",
                "-vf",
                "setsar=32/27",
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-threads",
                "2",
                "-c:a",
                "aac",
                "-c:s",
                "srt",
                "-metadata:s:a:0",
                "language=eng",
                "-metadata:s:a:1",
                "language=nld",
                "-metadata:s:s:0",
                "language=nld",
            ])
            .arg(&clip),
    )
    .await;
    library
}

#[tokio::test]
#[ignore = "requires pinned FFmpeg9/ffprobe; verifies anamorphic scan and deep-rescan persistence"]
async fn anamorphic_scan_and_deep_rescan_preserve_ratios_tracks_and_identity() {
    let tools = Tools::native();
    let root = tempfile::tempdir().unwrap();
    let library = generate_anamorphic_fixture(&tools, root.path()).await;
    let pool = test_db().await;
    seed_local_library(&pool, &library, 4).await;
    let mut identity = None;
    for deep_scan in [false, true] {
        let report = scan_local_library(
            &pool,
            &LocalScanConfig {
                managed_claim: None,
                deep_scan,
                library_id: 1,
                ffprobe_path: tools.ffprobe.to_str().unwrap().to_owned(),
            },
        )
        .await
        .unwrap();
        assert_eq!(report.scanned_files, 1);
        let version: (i64, String, String) = sqlx::query_as(
            r#"SELECT "Id", "SampleAspectRatio", "DisplayAspectRatio" FROM "MediaVersion""#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!((&*version.1, &*version.2), ("32:27", "16:9"));
        if let Some(first) = identity {
            assert_eq!(
                version.0, first,
                "deep scan must update the existing version"
            );
        }
        identity = Some(version.0);
        let tracks: Vec<(i32, Option<String>)> = sqlx::query_as(
            r#"SELECT "MediaStreamKind", "Language" FROM "MediaStream"
               WHERE "MediaVersionId" = ? ORDER BY "Index""#,
        )
        .bind(version.0)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            tracks,
            [
                (1, None),
                (2, Some("eng".into())),
                (2, Some("nld".into())),
                (3, Some("nld".into())),
            ]
        );
        if !deep_scan {
            sqlx::query(
                r#"UPDATE "MediaVersion" SET "SampleAspectRatio"='1:1', "DisplayAspectRatio"='4:3'"#,
            )
            .execute(&pool)
            .await
            .unwrap();
        }
    }
    assert_eq!(count(&pool, "MediaVersion").await, 1);
    println!("Verified initial/deep scan: SAR32:27, DAR16:9, same version, English/Dutch audio and Dutch captions");
}
