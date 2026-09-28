//! End-to-end test for the local TV scanner. Builds a minimal show tree with a
//! `tvshow.nfo`, one season folder, and one episode (video + per-episode NFO),
//! then asserts that every metadata table the scanner is supposed to write was
//! populated.
//!
//! Like [`local_scan.rs`] this is `#[ignore]`-gated on `ffprobe` and the Big
//! Buck Bunny fixture; run with `./run.sh test -p ersatzrs-scanner --run-ignored ignored-only`.

use std::env;
use std::path::{Path, PathBuf};

use ersatzrs_scanner::{scan_local_library, LocalScanConfig};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Executor, Row, SqlitePool};

#[tokio::test]
#[ignore = "requires ffprobe and the Big Buck Bunny fixture; use ./run.sh test with --run-ignored ignored-only"]
#[allow(clippy::too_many_lines)]
async fn local_show_scan_writes_full_metadata_and_child_rows() {
    let Some(ffprobe) = ffprobe_binary() else {
        eprintln!("skipping: ffprobe not found and ERSATZRS_FFPROBE is unset");
        return;
    };
    let Some(fixture) = big_buck_bunny_fixture() else {
        eprintln!("skipping: Big Buck Bunny testdata fixture not found");
        return;
    };

    let temp = TempDir::new("ersatzrs-local-television");
    let show_dir = temp.path().join("library").join("The Expanse");
    let season_dir = show_dir.join("Season 01");
    std::fs::create_dir_all(&season_dir).unwrap();

    std::fs::write(
        show_dir.join("tvshow.nfo"),
        r#"<tvshow>
            <title>The Expanse</title>
            <sorttitle>Expanse, The</sorttitle>
            <plot>Belters vs Inners.</plot>
            <year>2015</year>
            <premiered>2015-12-14</premiered>
            <contentrating>TV-14</contentrating>
            <genre>Science Fiction</genre>
            <genre>Drama</genre>
            <tag>space</tag>
            <studio>Syfy</studio>
            <actor><name>Shohreh Aghdashloo</name><role>Avasarala</role><order>1</order></actor>
            <uniqueid type="imdb" default="true">tt3230854</uniqueid>
        </tvshow>"#,
    )
    .unwrap();

    let episode_path = season_dir.join("The Expanse - S01E01 - Dulcinea.mkv");
    std::fs::copy(fixture, &episode_path).unwrap();
    std::fs::write(
        season_dir.join("The Expanse - S01E01 - Dulcinea.nfo"),
        r#"<episodedetails>
            <title>Dulcinea</title>
            <plot>Pilot episode.</plot>
            <season>1</season>
            <episode>1</episode>
            <year>2015</year>
            <aired>2015-12-14</aired>
            <genre>Drama</genre>
            <tag>pilot</tag>
            <director>Terry McDonough</director>
            <credits>Mark Fergus</credits>
            <actor><name>Steven Strait</name><role>Holden</role><order>1</order></actor>
            <uniqueid type="imdb" default="true">tt4574334</uniqueid>
        </episodedetails>"#,
    )
    .unwrap();

    let pool = test_db().await;
    seed_local_library(
        &pool,
        temp.path().join("library").as_path(),
        /* MediaKind = */ 2, // Shows
    )
    .await;

    let report = scan_local_library(
        &pool,
        &LocalScanConfig {
            library_id: 1,
            ffprobe_path: ffprobe,
        },
    )
    .await
    .unwrap();

    assert_eq!(report.scanned_files, 1);
    assert_eq!(count(&pool, "Show").await, 1);
    assert_eq!(count(&pool, "ShowMetadata").await, 1);
    assert_eq!(count(&pool, "Season").await, 1);
    assert_eq!(count(&pool, "SeasonMetadata").await, 1);
    assert_eq!(count(&pool, "Episode").await, 1);
    assert_eq!(count(&pool, "EpisodeMetadata").await, 1);
    assert!(count(&pool, "MediaVersion").await >= 1);
    assert_eq!(count(&pool, "MediaFile").await, 1);

    // Steps 3-5 parity: one show LibraryFolder + one season LibraryFolder,
    // with the season folder's ParentId pointing at the show folder. The
    // library root itself gets no row (ErsatzTV's ScanFolder only adds
    // show/season folders), so the show folder's ParentId stays NULL. The
    // show folder's Etag stays NULL (ErsatzTV only etags season folders,
    // after ScanEpisodes); the season folder's Etag is persisted after the
    // episode scan; the episode's MediaFile links to the season folder.
    assert_eq!(count(&pool, "LibraryFolder").await, 2);
    let show_path = show_dir.to_string_lossy().into_owned();
    let season_path = season_dir.to_string_lossy().into_owned();
    let show_folder =
        sqlx::query(r#"SELECT "Id", "ParentId", "Etag" FROM "LibraryFolder" WHERE "Path" = ?"#)
            .bind(&show_path)
            .fetch_one(&pool)
            .await
            .expect("show LibraryFolder row");
    let show_folder_id: i64 = show_folder.get("Id");
    assert_eq!(show_folder.get::<Option<i64>, _>("ParentId"), None);
    assert_eq!(show_folder.get::<Option<String>, _>("Etag"), None);

    let season_folder =
        sqlx::query(r#"SELECT "Id", "ParentId", "Etag" FROM "LibraryFolder" WHERE "Path" = ?"#)
            .bind(&season_path)
            .fetch_one(&pool)
            .await
            .expect("season LibraryFolder row");
    let season_folder_id: i64 = season_folder.get("Id");
    assert_eq!(
        season_folder.get::<Option<i64>, _>("ParentId"),
        Some(show_folder_id)
    );
    assert!(
        season_folder.get::<Option<String>, _>("Etag").is_some(),
        "season folder etag is persisted after the episode scan"
    );

    // Step 4 parity: the episode MediaFile links to the season LibraryFolder.
    let media_file_folder: Option<i64> =
        sqlx::query(r#"SELECT "LibraryFolderId" FROM "MediaFile" LIMIT 1"#)
            .fetch_one(&pool)
            .await
            .unwrap()
            .get("LibraryFolderId");
    assert_eq!(media_file_folder, Some(season_folder_id));

    // Show-scoped child rows.
    assert!(count_where(&pool, "Genre", r#""ShowMetadataId""#).await >= 2);
    assert!(count_where(&pool, "Tag", r#""ShowMetadataId""#).await >= 1);
    assert!(count_where(&pool, "Studio", r#""ShowMetadataId""#).await >= 1);
    assert!(count_where(&pool, "Actor", r#""ShowMetadataId""#).await >= 1);
    assert!(count_where(&pool, "MetadataGuid", r#""ShowMetadataId""#).await >= 1);

    // Episode-scoped child rows.
    assert!(count_where(&pool, "Genre", r#""EpisodeMetadataId""#).await >= 1);
    assert!(count_where(&pool, "Tag", r#""EpisodeMetadataId""#).await >= 1);
    assert!(count_where(&pool, "Actor", r#""EpisodeMetadataId""#).await >= 1);
    assert!(count_where(&pool, "Director", r#""EpisodeMetadataId""#).await >= 1);
    assert!(count_where(&pool, "Writer", r#""EpisodeMetadataId""#).await >= 1);
    assert!(count_where(&pool, "MetadataGuid", r#""EpisodeMetadataId""#).await >= 1);

    // Sidecar NFO should have flipped MetadataKind to 1 (Sidecar) for both.
    let show_kind: i32 = sqlx::query(r#"SELECT "MetadataKind" FROM "ShowMetadata" LIMIT 1"#)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get("MetadataKind");
    assert_eq!(show_kind, 1);

    let episode_kind: i32 = sqlx::query(r#"SELECT "MetadataKind" FROM "EpisodeMetadata" LIMIT 1"#)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get("MetadataKind");
    assert_eq!(episode_kind, 1);
}

#[tokio::test]
#[ignore = "requires ffprobe and the Big Buck Bunny fixture; use ./run.sh test with --run-ignored ignored-only"]
async fn rescan_repairs_same_title_original_and_reboot_collision() {
    let Some(ffprobe) = ffprobe_binary() else {
        eprintln!("skipping: ffprobe not found and ERSATZRS_FFPROBE is unset");
        return;
    };
    let Some(fixture) = big_buck_bunny_fixture() else {
        eprintln!("skipping: Big Buck Bunny testdata fixture not found");
        return;
    };

    let temp = TempDir::new("ersatzrs-local-television-remakes");
    let library = temp.path().join("library");
    for (folder, year, imdb, episode_name) in [
        ("original", 1988, "tt0096623", "original-s01e01.mkv"),
        ("reboot", 2022, "tt11921588", "reboot-s01e01.mkv"),
    ] {
        let show = library.join(folder);
        let season = show.join("Season 01");
        std::fs::create_dir_all(&season).unwrap();
        std::fs::write(
            show.join("tvshow.nfo"),
            format!(
                r#"<tvshow>
                    <title>The Kids in the Hall</title>
                    <year>{year}</year>
                    <uniqueid type="imdb" default="true">{imdb}</uniqueid>
                </tvshow>"#
            ),
        )
        .unwrap();
        std::fs::copy(&fixture, season.join(episode_name)).unwrap();
    }

    let pool = test_db().await;
    seed_local_library(&pool, &library, 2).await;
    let config = LocalScanConfig {
        library_id: 1,
        ffprobe_path: ffprobe,
    };
    let first = scan_local_library(&pool, &config).await.unwrap();
    assert_eq!(first.scanned_files, 2);

    let shows = sqlx::query(
        r#"SELECT show."Id", metadata."Year"
           FROM "Show" show JOIN "ShowMetadata" metadata ON metadata."ShowId" = show."Id"
           ORDER BY metadata."Year""#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(shows.len(), 2);
    let original_show_id: i64 = shows[0].get("Id");
    let reboot_show_id: i64 = shows[1].get("Id");
    assert_eq!(shows[0].get::<Option<i32>, _>("Year"), Some(1988));
    assert_eq!(shows[1].get::<Option<i32>, _>("Year"), Some(2022));

    let reboot_season_id: i64 = sqlx::query_scalar(
        r#"SELECT "Id" FROM "Season" WHERE "ShowId" = ? AND "SeasonNumber" = 1"#,
    )
    .bind(reboot_show_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query(r#"UPDATE "Season" SET "ShowId" = ? WHERE "Id" = ?"#)
        .bind(original_show_id)
        .bind(reboot_season_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"DELETE FROM "MediaItem" WHERE "Id" = ?"#)
        .bind(reboot_show_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "LibraryFolder" SET "Etag" = 'cached' WHERE "ParentId" IS NOT NULL"#)
        .execute(&pool)
        .await
        .unwrap();

    let repair = scan_local_library(&pool, &config).await.unwrap();
    assert_eq!(
        repair.scanned_files, 2,
        "the split must bypass both cached season folder etags"
    );
    let repaired = sqlx::query(
        r#"SELECT show."Id", metadata."Year", COUNT(DISTINCT season."Id") AS "Seasons",
                  COUNT(DISTINCT episode."Id") AS "Episodes"
           FROM "Show" show
           JOIN "ShowMetadata" metadata ON metadata."ShowId" = show."Id"
           LEFT JOIN "Season" season ON season."ShowId" = show."Id"
           LEFT JOIN "Episode" episode ON episode."SeasonId" = season."Id"
           GROUP BY show."Id", metadata."Year" ORDER BY metadata."Year""#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(repaired.len(), 2);
    assert_eq!(repaired[0].get::<i64, _>("Id"), original_show_id);
    for (row, year) in repaired.iter().zip([1988, 2022]) {
        assert_eq!(row.get::<Option<i32>, _>("Year"), Some(year));
        assert_eq!(row.get::<i64, _>("Seasons"), 1);
        assert_eq!(row.get::<i64, _>("Episodes"), 1);
    }
}

/// Step 5 parity: a second scan of an unchanged season folder hits the etag
/// fast path (no episode probing, `scanned_files == 0`), while touching a
/// file under the season folder changes the etag and forces a rescan.
#[tokio::test]
#[ignore = "requires ffprobe and the Big Buck Bunny fixture; use ./run.sh test with --run-ignored ignored-only"]
async fn second_scan_skips_unchanged_season_via_etag() {
    let Some(ffprobe) = ffprobe_binary() else {
        eprintln!("skipping: ffprobe not found and ERSATZRS_FFPROBE is unset");
        return;
    };
    let Some(fixture) = big_buck_bunny_fixture() else {
        eprintln!("skipping: Big Buck Bunny testdata fixture not found");
        return;
    };

    let temp = TempDir::new("ersatzrs-local-television-etag");
    let season_dir = temp
        .path()
        .join("library")
        .join("The Expanse")
        .join("Season 01");
    std::fs::create_dir_all(&season_dir).unwrap();
    let episode_path = season_dir.join("The Expanse - S01E01 - Dulcinea.mkv");
    std::fs::copy(&fixture, &episode_path).unwrap();

    let pool = test_db().await;
    seed_local_library(&pool, temp.path().join("library").as_path(), 2).await;

    let config = LocalScanConfig {
        library_id: 1,
        ffprobe_path: ffprobe,
    };

    // First scan probes the episode and persists the season folder etag.
    let first = scan_local_library(&pool, &config).await.unwrap();
    assert_eq!(first.scanned_files, 1);
    let etag_after_first: Option<String> =
        sqlx::query(r#"SELECT "Etag" FROM "LibraryFolder" WHERE "Path" = ?"#)
            .bind(season_dir.to_string_lossy().into_owned())
            .fetch_one(&pool)
            .await
            .unwrap()
            .get("Etag");
    assert!(etag_after_first.is_some(), "season etag must be persisted");

    // Second scan with an unchanged folder skips episode work entirely.
    let second = scan_local_library(&pool, &config).await.unwrap();
    assert_eq!(
        second.scanned_files, 0,
        "unchanged season must not be re-probed"
    );
    assert_eq!(count(&pool, "Episode").await, 1);

    // Touching the episode file changes the folder etag and forces a rescan.
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
    std::fs::File::options()
        .write(true)
        .open(&episode_path)
        .unwrap()
        .set_modified(later)
        .unwrap();
    let third = scan_local_library(&pool, &config).await.unwrap();
    assert_eq!(
        third.scanned_files, 1,
        "changed season etag must trigger a rescan"
    );
    let etag_after_third: Option<String> =
        sqlx::query(r#"SELECT "Etag" FROM "LibraryFolder" WHERE "Path" = ?"#)
            .bind(season_dir.to_string_lossy().into_owned())
            .fetch_one(&pool)
            .await
            .unwrap()
            .get("Etag");
    assert_ne!(
        etag_after_third, etag_after_first,
        "etag must change after the file mtime changed"
    );
}

/// Failed episode writes and malformed sidecars must not suppress the next scan.
#[tokio::test]
#[ignore = "requires ffprobe and the Big Buck Bunny fixture"]
async fn failed_episode_scan_does_not_publish_season_etag() {
    let ffprobe = ffprobe_binary().expect("ffprobe is required for acceptance");
    let fixture = big_buck_bunny_fixture().expect("media fixture is required for acceptance");
    for malformed_nfo in [false, true] {
        let temp = TempDir::new("ersatzrs-season-etag-failure");
        let root = temp.path().join("library");
        let season = root.join("Synthetic Show").join("Season 01");
        std::fs::create_dir_all(&season).unwrap();
        for name in ["S01E01.mkv", "S01E02.mkv"] {
            let destination = season.join(name);
            std::fs::hard_link(&fixture, &destination)
                .or_else(|_| std::fs::copy(&fixture, &destination).map(|_| ()))
                .unwrap();
        }
        let sidecar = season.join("S01E02.nfo");
        std::fs::write(
            &sidecar,
            if malformed_nfo {
                "<episodedetails><title>Broken</episodedetails>"
            } else {
                "<episodedetails><title>Recovered</title></episodedetails>"
            },
        )
        .unwrap();
        let original_mtime = std::fs::metadata(&sidecar).unwrap().modified().unwrap();
        let pool = test_db().await;
        seed_local_library(&pool, &root, 2).await;
        if !malformed_nfo {
            pool.execute(
                r#"CREATE TRIGGER fail_second_episode BEFORE INSERT ON "EpisodeMetadata"
                   WHEN NEW."Title" = 'Recovered'
                   BEGIN SELECT RAISE(ABORT, 'synthetic metadata write failure'); END"#,
            )
            .await
            .unwrap();
        }
        let config = LocalScanConfig {
            library_id: 1,
            ffprobe_path: ffprobe.clone(),
        };
        for _ in 0..2 {
            let report = scan_local_library(&pool, &config).await.unwrap();
            assert!(report.scanned_files >= 1, "successful siblings still scan");
            assert_eq!(
                season_etag(&pool, &season).await,
                None,
                "partial processing must remain retryable without touching a file"
            );
        }
        if malformed_nfo {
            std::fs::write(
                &sidecar,
                "<episodedetails><title>Recovered</title></episodedetails>",
            )
            .unwrap();
            std::fs::File::options()
                .write(true)
                .open(&sidecar)
                .unwrap()
                .set_modified(original_mtime)
                .unwrap();
        } else {
            pool.execute("DROP TRIGGER fail_second_episode")
                .await
                .unwrap();
        }
        assert_eq!(
            scan_local_library(&pool, &config)
                .await
                .unwrap()
                .scanned_files,
            2
        );
        assert!(season_etag(&pool, &season).await.is_some());
        let recovered: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "EpisodeMetadata" WHERE "Title" = 'Recovered'"#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(recovered, 1);
        assert_eq!(count(&pool, "Episode").await, 2);
        assert_eq!(
            scan_local_library(&pool, &config)
                .await
                .unwrap()
                .scanned_files,
            0
        );
    }
}

async fn season_etag(pool: &SqlitePool, season: &Path) -> Option<String> {
    sqlx::query_scalar(r#"SELECT "Etag" FROM "LibraryFolder" WHERE "Path" = ?"#)
        .bind(season.to_string_lossy().as_ref())
        .fetch_one(pool)
        .await
        .unwrap()
}

/// The one-time HDR upgrade must repair metadata even when media bytes and
/// the season directory have not changed since the previous successful scan.
#[tokio::test]
#[ignore = "requires verified FFmpeg 9 and ffprobe with libx265"]
#[allow(
    clippy::too_many_lines,
    reason = "One end-to-end upgrade scenario preserves file and subtitle identity."
)]
async fn hdr_metadata_upgrade_reenters_unchanged_local_scan() {
    let ffprobe = ffprobe_binary().expect("ffprobe is required for HDR acceptance");
    let ffmpeg = Path::new(&ffprobe).with_file_name(format!("ffmpeg{}", env::consts::EXE_SUFFIX));
    let temp = TempDir::new("ersatzrs-hdr-metadata-upgrade");
    let season = temp
        .path()
        .join("library")
        .join("Synthetic HDR")
        .join("Season 01");
    std::fs::create_dir_all(&season).unwrap();
    let episode = season.join("Synthetic HDR - S01E01.mkv");
    let output = std::process::Command::new(ffmpeg).args([
        "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "color=blue:s=64x64:r=10",
        "-vf", "setparams=color_primaries=bt2020:color_trc=smpte2084:colorspace=bt2020nc",
        "-t", "1", "-c:v", "libx265", "-pix_fmt", "yuv420p10le", "-color_trc", "smpte2084",
        "-color_primaries", "bt2020", "-colorspace", "bt2020nc", "-x265-params",
        "pools=1:frame-threads=1:log-level=error:colorprim=9:transfer=16:colormatrix=9:master-display=G(13250,34500)B(7500,3000)R(34000,16000)WP(15635,16450)L(10000000,50):max-cll=1000,400",
    ]).arg(&episode).output().unwrap();
    assert!(
        output.status.success(),
        "HDR fixture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::write(
        season.join("Synthetic HDR - S01E01.nl.srt"),
        "1\n00:00:00,000 --> 00:00:01,000\nNederlandse ondertiteling\n",
    )
    .unwrap();
    let original_modified = std::fs::metadata(&episode).unwrap().modified().unwrap();
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let migrations = sqlx::migrate!("../../migrations");
    sqlx::migrate::Migrator::with_migrations(
        migrations
            .iter()
            .filter(|m| m.version < 202_609_282_140)
            .cloned()
            .collect(),
    )
    .run(&pool)
    .await
    .unwrap();
    seed_local_library(&pool, &temp.path().join("library"), 2).await;
    let config = LocalScanConfig {
        library_id: 1,
        ffprobe_path: ffprobe,
    };
    assert_eq!(
        scan_local_library(&pool, &config)
            .await
            .unwrap()
            .scanned_files,
        1
    );
    let before: (i64, i64) = sqlx::query_as(r#"SELECT "Id", "EpisodeId" FROM "MediaVersion""#)
        .fetch_one(&pool)
        .await
        .unwrap();
    let subtitles: Vec<(i64, String)> =
        sqlx::query_as(r#"SELECT "Id", "Language" FROM "Subtitle" ORDER BY "Id""#)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(subtitles.len(), 1);
    assert_eq!(subtitles[0].1, "dut");
    let max_cll: Option<i64> =
        sqlx::query_scalar(r#"SELECT "MaxCll" FROM "MediaStream" WHERE "MediaStreamKind" = 1"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(max_cll, Some(1000));
    pool.execute(
        r#"UPDATE "MediaStream" SET "MasteringDisplay" = NULL, "MaxCll" = NULL, "MaxFall" = NULL"#,
    )
    .await
    .unwrap();
    assert_eq!(
        scan_local_library(&pool, &config)
            .await
            .unwrap()
            .scanned_files,
        0
    );
    let transfer: Option<String> = sqlx::query_scalar(
        r#"SELECT "ColorTransfer" FROM "MediaStream" WHERE "MediaStreamKind" = 1"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        transfer.as_deref(),
        Some("smpte2084"),
        "HDR fixture must carry PQ transfer"
    );
    migrations.run(&pool).await.unwrap();
    let gate: Option<String> = sqlx::query_scalar(r#"SELECT LF."Etag" FROM "LibraryFolder" LF JOIN "MediaFile" MF ON MF."LibraryFolderId" = LF."Id""#)
        .fetch_one(&pool).await.unwrap();
    assert_eq!(
        gate, None,
        "migration must invalidate the real scanner folder gate"
    );
    assert_eq!(
        scan_local_library(&pool, &config)
            .await
            .unwrap()
            .scanned_files,
        1
    );
    let repaired: (Option<String>, Option<i64>, Option<i64>) = sqlx::query_as(
        r#"SELECT "MasteringDisplay", "MaxCll", "MaxFall" FROM "MediaStream" WHERE "MediaStreamKind" = 1"#)
        .fetch_one(&pool).await.unwrap();
    assert!(repaired.0.is_some());
    assert_eq!((repaired.1, repaired.2), (Some(1000), Some(400)));
    let after: (i64, i64) = sqlx::query_as(r#"SELECT "Id", "EpisodeId" FROM "MediaVersion""#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(after, before);
    let after_subtitles: Vec<(i64, String)> =
        sqlx::query_as(r#"SELECT "Id", "Language" FROM "Subtitle" ORDER BY "Id""#)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(after_subtitles, subtitles);
    assert_eq!(
        std::fs::metadata(&episode).unwrap().modified().unwrap(),
        original_modified
    );
    migrations.run(&pool).await.unwrap();
    assert_eq!(
        scan_local_library(&pool, &config)
            .await
            .unwrap()
            .scanned_files,
        0
    );
}

/// A legacy collision survives an unchanged-folder scan, then the upgrade
/// invalidates the folder and the ordinary scanner repairs it without a file touch.
#[tokio::test]
#[ignore = "requires FFmpeg 9, ffprobe and the Big Buck Bunny fixture"]
#[allow(
    clippy::too_many_lines,
    reason = "The end-to-end upgrade regression covers scan invalidation and playback selection."
)]
async fn subtitle_collision_upgrade_reenters_scan_and_restores_language_selection() {
    let ffprobe = ffprobe_binary().expect("ffprobe is required for this acceptance test");
    let fixture = big_buck_bunny_fixture().expect("Big Buck Bunny fixture is required");
    let temp = TempDir::new("ersatzrs-subtitle-collision-upgrade");
    let season = temp
        .path()
        .join("library")
        .join("Synthetic Show")
        .join("Season 01");
    std::fs::create_dir_all(&season).unwrap();
    std::fs::copy(fixture, season.join("Synthetic Show - S01E01.mkv")).unwrap();
    for (language, caption) in [
        ("en", "English caption"),
        ("nl", "Nederlandse ondertiteling"),
    ] {
        std::fs::write(
            season.join(format!("Synthetic Show - S01E01.{language}.srt")),
            format!("1\n00:00:00,000 --> 00:00:02,000\n{caption}\n"),
        )
        .unwrap();
    }
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let migrations = sqlx::migrate!("../../migrations");
    sqlx::migrate::Migrator::with_migrations(
        migrations
            .iter()
            .filter(|m| m.version < 202_609_241_530)
            .cloned()
            .collect(),
    )
    .run(&pool)
    .await
    .unwrap();
    seed_local_library(&pool, &temp.path().join("library"), 2).await;
    let config = LocalScanConfig {
        library_id: 1,
        ffprobe_path: ffprobe,
    };
    assert_eq!(
        scan_local_library(&pool, &config)
            .await
            .unwrap()
            .scanned_files,
        1
    );
    // Reproduce the old blank-path collision without changing the directory etag.
    pool.execute(r#"UPDATE "Subtitle" SET "Path" = NULL WHERE "Language" = 'dut';
        INSERT INTO "Subtitle" ("EpisodeMetadataId","SubtitleKind","StreamIndex","Path","Language","Codec","Default","Forced","DateAdded","DateUpdated")
        SELECT "EpisodeMetadataId","SubtitleKind","StreamIndex","Path","Language","Codec","Default","Forced","DateAdded","DateUpdated"
        FROM "Subtitle" WHERE "Language" = 'eng'"#).await.unwrap();
    assert_eq!(count(&pool, "Subtitle").await, 3);
    assert_eq!(
        scan_local_library(&pool, &config)
            .await
            .unwrap()
            .scanned_files,
        0
    );
    migrations.run(&pool).await.unwrap();
    assert_eq!(
        scan_local_library(&pool, &config)
            .await
            .unwrap()
            .scanned_files,
        1
    );
    assert_eq!(count(&pool, "Subtitle").await, 2);
    let episode: i64 = sqlx::query_scalar(r#"SELECT "Id" FROM "Episode""#)
        .fetch_one(&pool)
        .await
        .unwrap();
    let version: i64 =
        sqlx::query_scalar(r#"SELECT "Id" FROM "MediaVersion" WHERE "EpisodeId" = ?"#)
            .bind(episode)
            .fetch_one(&pool)
            .await
            .unwrap();
    for (language, caption) in [
        ("en", "English caption"),
        ("nl", "Nederlandse ondertiteling"),
    ] {
        let selected = ersatzrs_infra::playback::select_playback_subtitle(
            &pool,
            episode,
            &[language.to_owned()],
            ersatzrs_domain::ChannelSubtitleMode::Any,
            ersatzrs_domain::StreamingMode::HttpLiveStreamingDirect,
        )
        .await
        .unwrap()
        .expect("repaired sidecar must be selectable");
        let next = ersatzrs_infra::playback::select_next_playback_subtitle_for_version(
            &pool,
            episode,
            version,
            &[language.to_owned()],
            ersatzrs_domain::ChannelSubtitleMode::Any,
            ersatzrs_domain::StreamingMode::HttpLiveStreamingDirect,
        )
        .await
        .unwrap()
        .expect("Next must select the repaired sidecar");
        assert_eq!(next.id, selected.id);
        let path = selected.path.expect("selected sidecar must have a path");
        let mut command = tokio::process::Command::new(ersatzrs_ffmpeg::ffmpeg_path());
        command
            .kill_on_drop(true)
            .args(["-hide_banner", "-loglevel", "error", "-i"])
            .arg(&path)
            .args(["-map", "0:s:0", "-c:s", "webvtt", "-f", "webvtt", "pipe:1"]);
        let output = tokio::time::timeout(std::time::Duration::from_secs(30), command.output())
            .await
            .expect("subtitle conversion must finish")
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8(output.stdout).unwrap().contains(caption));
    }
    assert_eq!(
        scan_local_library(&pool, &config)
            .await
            .unwrap()
            .scanned_files,
        0
    );
    assert_eq!(count(&pool, "Subtitle").await, 2);
}

/// Step 6 parity: a previously trashed (`State IN (1,2)`) episode under an
/// otherwise unchanged season disables the etag fast path so the episode is
/// re-probed and restored to `Normal` (State 0).
#[tokio::test]
#[ignore = "requires ffprobe and the Big Buck Bunny fixture; use ./run.sh test with --run-ignored ignored-only"]
async fn trashed_episode_disables_etag_skip_and_restores_normal() {
    let Some(ffprobe) = ffprobe_binary() else {
        eprintln!("skipping: ffprobe not found and ERSATZRS_FFPROBE is unset");
        return;
    };
    let Some(fixture) = big_buck_bunny_fixture() else {
        eprintln!("skipping: Big Buck Bunny testdata fixture not found");
        return;
    };

    let temp = TempDir::new("ersatzrs-local-television-trash");
    let season_dir = temp
        .path()
        .join("library")
        .join("The Expanse")
        .join("Season 01");
    std::fs::create_dir_all(&season_dir).unwrap();
    let episode_path = season_dir.join("The Expanse - S01E01 - Dulcinea.mkv");
    std::fs::copy(&fixture, &episode_path).unwrap();

    let pool = test_db().await;
    seed_local_library(&pool, temp.path().join("library").as_path(), 2).await;
    let config = LocalScanConfig {
        library_id: 1,
        ffprobe_path: ffprobe,
    };

    // First scan probes the episode and persists the season folder etag.
    assert_eq!(
        scan_local_library(&pool, &config)
            .await
            .unwrap()
            .scanned_files,
        1
    );

    // Mark the episode MediaItem as trashed (FileNotFound = 1).
    sqlx::query(
        r#"UPDATE "MediaItem" SET "State" = 1
           WHERE "Id" IN (SELECT "Id" FROM "Episode")"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    // The season folder etag is unchanged, but the restore guard must
    // disable the fast path: the episode is re-probed.
    let rescan = scan_local_library(&pool, &config).await.unwrap();
    assert_eq!(
        rescan.scanned_files, 1,
        "trashed episode must disable the etag skip"
    );

    // And the episode is restored to Normal (State 0).
    let state: i32 = sqlx::query(
        r#"SELECT "State" FROM "MediaItem" WHERE "Id" IN (SELECT "Id" FROM "Episode")"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap()
    .get("State");
    assert_eq!(state, 0, "restored episode must return to Normal");
}

/// Step 7 parity: `clean_library_folders_for_path` runs after a library-path
/// scan. A stale season folder still referenced by an episode `MediaFile`
/// survives, and that surviving child keeps its (also stale) show parent via
/// the `NOT EXISTS child` guard — exactly ErsatzTV's `CleanEtagsForLibraryPath`
/// behavior. The unit test `clean_keeps_referenced_and_removes_stale` covers
/// the fully-unreferenced deletion path.
#[tokio::test]
#[ignore = "requires ffprobe and the Big Buck Bunny fixture; use ./run.sh test with --run-ignored ignored-only"]
async fn stale_library_folders_cleaned_after_scan() {
    let Some(ffprobe) = ffprobe_binary() else {
        eprintln!("skipping: ffprobe not found and ERSATZRS_FFPROBE is unset");
        return;
    };
    let Some(fixture) = big_buck_bunny_fixture() else {
        eprintln!("skipping: Big Buck Bunny testdata fixture not found");
        return;
    };

    let temp = TempDir::new("ersatzrs-local-television-clean");
    let library_dir = temp.path().join("library");
    let show_dir = library_dir.join("The Expanse");
    let season_dir = show_dir.join("Season 01");
    std::fs::create_dir_all(&season_dir).unwrap();
    std::fs::copy(
        &fixture,
        season_dir.join("The Expanse - S01E01 - Dulcinea.mkv"),
    )
    .unwrap();

    let pool = test_db().await;
    seed_local_library(&pool, library_dir.as_path(), 2).await;
    let config = LocalScanConfig {
        library_id: 1,
        ffprobe_path: ffprobe,
    };

    assert_eq!(
        scan_local_library(&pool, &config)
            .await
            .unwrap()
            .scanned_files,
        1
    );
    // First scan created the show + season LibraryFolder rows.
    assert_eq!(count(&pool, "LibraryFolder").await, 2);
    let show_path = show_dir.to_string_lossy().into_owned();
    let season_path = season_dir.to_string_lossy().into_owned();

    // Remove the entire show subtree from disk, then rescan.
    std::fs::remove_dir_all(&show_dir).unwrap();
    scan_local_library(&pool, &config).await.unwrap();

    // The season folder row survives because the episode MediaFile still
    // references it; the surviving season row then keeps its stale show
    // parent through the `NOT EXISTS child` guard — both rows remain, exactly
    // like ErsatzTV's clean-up guards.
    let season_rows: i64 =
        sqlx::query(r#"SELECT COUNT(*) AS "C" FROM "LibraryFolder" WHERE "Path" = ?"#)
            .bind(&season_path)
            .fetch_one(&pool)
            .await
            .unwrap()
            .get("C");
    assert_eq!(
        season_rows, 1,
        "season folder referenced by a MediaFile must survive"
    );

    let show_rows: i64 =
        sqlx::query(r#"SELECT COUNT(*) AS "C" FROM "LibraryFolder" WHERE "Path" = ?"#)
            .bind(&show_path)
            .fetch_one(&pool)
            .await
            .unwrap()
            .get("C");
    assert_eq!(
        show_rows, 1,
        "stale show parent is kept while its referenced child survives"
    );
}

async fn test_db() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    pool.execute("PRAGMA foreign_keys = ON").await.unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    pool
}

async fn seed_local_library(pool: &SqlitePool, path: &Path, media_kind: i32) {
    pool.execute(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
        .await
        .unwrap();
    pool.execute(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (1)"#)
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
           VALUES (1, 'Local', ?, 1)"#,
    )
    .bind(media_kind)
    .execute(pool)
    .await
    .unwrap();
    pool.execute(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (1)"#)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path") VALUES (1, 1, ?)"#)
        .bind(path.display().to_string())
        .execute(pool)
        .await
        .unwrap();
}

async fn count(pool: &SqlitePool, table: &str) -> i64 {
    let sql = format!(r#"SELECT COUNT(*) AS "Count" FROM "{table}""#);
    sqlx::query(ersatzrs_infra::db::assert_sql_safe(&sql))
        .fetch_one(pool)
        .await
        .unwrap()
        .get("Count")
}

async fn count_where(pool: &SqlitePool, table: &str, fk_column_quoted: &str) -> i64 {
    let sql = format!(
        r#"SELECT COUNT(*) AS "Count" FROM "{table}" WHERE {fk_column_quoted} IS NOT NULL"#
    );
    sqlx::query(ersatzrs_infra::db::assert_sql_safe(&sql))
        .fetch_one(pool)
        .await
        .unwrap()
        .get("Count")
}

#[allow(clippy::disallowed_methods)]
fn ffprobe_binary() -> Option<String> {
    env::var("ERSATZRS_FFPROBE")
        .ok()
        .or_else(|| find_on_path("ffprobe").map(|path| path.display().to_string()))
}

#[allow(clippy::disallowed_methods)]
fn find_on_path(binary: &str) -> Option<PathBuf> {
    env::var_os("PATH")?
        .to_string_lossy()
        .split(':')
        .find_map(|dir| {
            let path = Path::new(dir).join(binary);
            path.is_file().then_some(path)
        })
}

fn big_buck_bunny_fixture() -> Option<PathBuf> {
    const NAMES: &[&str] = &[
        "bbb_sunflower_1080p_30fps_normal.mp4",
        "bbb_sunflower_1080p_60fps_normal.mp4",
        "bbb_sunflower_2160p_30fps_normal.mp4",
        "bbb_sunflower_2160p_60fps_normal.mp4",
    ];
    const ROOTS: &[&str] = &["testdata", "../../testdata"];
    ROOTS
        .iter()
        .flat_map(|root| NAMES.iter().map(move |name| PathBuf::from(root).join(name)))
        .find(|path| path.is_file())
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    #[allow(clippy::disallowed_methods)]
    fn new(prefix: &str) -> Self {
        let path = std::env::temp_dir().join(format!("{prefix}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
