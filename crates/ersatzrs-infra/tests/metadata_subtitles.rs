mod common;

use ersatzrs_domain::{MediaStream, MediaStreamKind};
use ersatzrs_infra::metadata::{subtitle_kind, update_local_file_subtitles, SubtitleMetadataOwner};
use sqlx::{Executor, Row};
use tempfile::tempdir;

async fn seed_local_movie(pool: &sqlx::SqlitePool) {
    pool.execute(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
        .await
        .unwrap();
    pool.execute(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (1)"#)
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
           VALUES (1, 'Local Movies', 1, 1)"#,
    )
    .await
    .unwrap();
    pool.execute(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (1)"#)
        .await
        .unwrap();
    pool.execute(r#"INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path") VALUES (1, 1, '/tmp')"#)
        .await
        .unwrap();
    pool.execute(r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (1, 1, 0)"#)
        .await
        .unwrap();
    pool.execute(r#"INSERT INTO "Movie" ("Id") VALUES (1)"#)
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "MovieMetadata" (
               "Id", "MovieId", "DateAdded", "DateUpdated", "MetadataKind", "Title"
           ) VALUES (10, 1, '2026-05-31T00:00:00Z', '2026-05-31T00:00:00Z', 1, 'Movie')"#,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn local_movie_subtitles_include_embedded_streams_and_sidecars() {
    let pool = common::test_db().await;
    seed_local_movie(&pool).await;

    let temp = tempdir().unwrap();
    let movie_path = temp.path().join("movie.mkv");
    let sidecar_path = temp.path().join("movie.en.forced.cc.srt");
    std::fs::write(&movie_path, b"").unwrap();
    std::fs::write(&sidecar_path, b"1\n00:00:00,000 --> 00:00:01,000\nHi\n").unwrap();

    let streams = vec![MediaStream {
        index: 2,
        kind: MediaStreamKind::Subtitle,
        codec: Some("webvtt".to_owned()),
        language: Some("spa".to_owned()),
        forced: false,
        default: true,
        ..MediaStream::default()
    }];

    let mut tx = pool.begin().await.unwrap();
    update_local_file_subtitles(
        &mut tx,
        SubtitleMetadataOwner::Movie(10),
        &streams,
        &movie_path,
        true,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let rows = sqlx::query(
        r#"SELECT "StreamIndex", "Codec", "SubtitleKind", "Language", "Forced", "SDH", "Path"
           FROM "Subtitle"
           WHERE "MovieMetadataId" = 10
           ORDER BY "SubtitleKind", "StreamIndex", "Path""#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].get::<i64, _>("StreamIndex"), 2);
    assert_eq!(
        rows[0].get::<Option<String>, _>("Codec").as_deref(),
        Some("webvtt")
    );
    assert_eq!(
        rows[0].get::<i64, _>("SubtitleKind"),
        i64::from(subtitle_kind::EMBEDDED)
    );
    assert_eq!(
        rows[0].get::<Option<String>, _>("Language").as_deref(),
        Some("spa")
    );

    assert_eq!(rows[1].get::<i64, _>("StreamIndex"), 0);
    assert_eq!(
        rows[1].get::<Option<String>, _>("Codec").as_deref(),
        Some("subrip")
    );
    assert_eq!(
        rows[1].get::<i64, _>("SubtitleKind"),
        i64::from(subtitle_kind::SIDECAR)
    );
    assert_eq!(
        rows[1].get::<Option<String>, _>("Language").as_deref(),
        Some("eng")
    );
    assert_eq!(rows[1].get::<i64, _>("Forced"), 1);
    assert_eq!(rows[1].get::<i64, _>("SDH"), 1);
    let expected_path = sidecar_path.display().to_string();
    assert_eq!(
        rows[1].get::<Option<String>, _>("Path").as_deref(),
        Some(expected_path.as_str())
    );
}

#[tokio::test]
async fn local_sidecars_require_exact_basename_and_use_und_without_language_suffix() {
    let pool = common::test_db().await;
    seed_local_movie(&pool).await;

    let temp = tempdir().unwrap();
    let movie_path = temp.path().join("movie.mkv");
    let bare_sidecar = temp.path().join("movie.srt");
    let prefix_collision = temp.path().join("movie2.en.srt");
    std::fs::write(&movie_path, b"").unwrap();
    std::fs::write(&bare_sidecar, b"1\n00:00:00,000 --> 00:00:01,000\nHi\n").unwrap();
    std::fs::write(
        &prefix_collision,
        b"1\n00:00:00,000 --> 00:00:01,000\nWrong\n",
    )
    .unwrap();

    let mut tx = pool.begin().await.unwrap();
    update_local_file_subtitles(
        &mut tx,
        SubtitleMetadataOwner::Movie(10),
        &[],
        &movie_path,
        true,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let rows = sqlx::query(
        r#"SELECT "Language", "Path"
           FROM "Subtitle"
           WHERE "MovieMetadataId" = 10
           ORDER BY "Path""#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get::<Option<String>, _>("Language").as_deref(),
        Some("und")
    );
    let expected_path = bare_sidecar.display().to_string();
    assert_eq!(
        rows[0].get::<Option<String>, _>("Path").as_deref(),
        Some(expected_path.as_str())
    );
}

#[tokio::test]
async fn local_sidecars_use_path_identity_for_multiple_sidecars_and_rescans() {
    let pool = common::test_db().await;
    seed_local_movie(&pool).await;

    let temp = tempdir().unwrap();
    let movie_path = temp.path().join("movie.mkv");
    let english = temp.path().join("movie.en.srt");
    let spanish = temp.path().join("movie.es.srt");
    std::fs::write(&movie_path, b"").unwrap();
    std::fs::write(&english, b"1\n00:00:00,000 --> 00:00:01,000\nHi\n").unwrap();
    std::fs::write(&spanish, b"1\n00:00:00,000 --> 00:00:01,000\nHola\n").unwrap();

    let mut tx = pool.begin().await.unwrap();
    update_local_file_subtitles(
        &mut tx,
        SubtitleMetadataOwner::Movie(10),
        &[],
        &movie_path,
        true,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let rows = sidecar_rows(&pool).await;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0, 0);
    assert_eq!(rows[0].1.as_deref(), Some("eng"));
    assert_eq!(rows[1].0, 1);
    assert_eq!(rows[1].1.as_deref(), Some("spa"));

    std::fs::remove_file(&english).unwrap();
    let mut tx = pool.begin().await.unwrap();
    update_local_file_subtitles(
        &mut tx,
        SubtitleMetadataOwner::Movie(10),
        &[],
        &movie_path,
        true,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let rows = sidecar_rows(&pool).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, 0);
    assert_eq!(rows[0].1.as_deref(), Some("spa"));
    let expected_path = spanish.display().to_string();
    assert_eq!(rows[0].2.as_deref(), Some(expected_path.as_str()));
}

async fn sidecar_rows(pool: &sqlx::SqlitePool) -> Vec<(i64, Option<String>, Option<String>)> {
    sqlx::query(
        r#"SELECT "StreamIndex", "Language", "Path"
           FROM "Subtitle"
           WHERE "MovieMetadataId" = 10 AND "SubtitleKind" = 1
           ORDER BY "StreamIndex", "Path""#,
    )
    .fetch_all(pool)
    .await
    .unwrap()
    .into_iter()
    .map(|row| {
        (
            row.get::<i64, _>("StreamIndex"),
            row.get::<Option<String>, _>("Language"),
            row.get::<Option<String>, _>("Path"),
        )
    })
    .collect()
}

#[tokio::test]
async fn provider_sidecars_with_equal_filenames_keep_distinct_url_identities() {
    let pool = common::test_db().await;
    seed_local_movie(&pool).await;
    let streams: Vec<_> = [("one", "eng"), ("two", "spa")]
        .into_iter()
        .map(|(directory, language)| MediaStream {
            index: 0,
            kind: MediaStreamKind::ExternalSubtitle,
            codec: Some("subrip".into()),
            language: Some(language.into()),
            file_name: Some(format!("https://example.test/{directory}/captions.srt")),
            ..MediaStream::default()
        })
        .collect();
    for _ in 0..2 {
        let mut tx = pool.begin().await.unwrap();
        ersatzrs_infra::metadata::update_metadata_subtitles(
            &mut tx,
            SubtitleMetadataOwner::Movie(10),
            &streams,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let rows = sidecar_rows(&pool).await;
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].1.as_deref(), Some("eng"));
        assert_eq!(rows[1].1.as_deref(), Some("spa"));
        assert_ne!(rows[0].2, rows[1].2);
    }
}

#[tokio::test]
async fn local_rescan_repairs_duplicates_blank_sidecars_and_preserves_embedded_zero_cache() {
    let pool = common::test_db().await;
    seed_local_movie(&pool).await;
    let temp = tempdir().unwrap();
    let movie = temp.path().join("movie.mkv");
    for name in ["movie.en.srt", "movie.nl.srt"] {
        std::fs::write(temp.path().join(name), b"captions").unwrap();
    }
    for (id, kind, language, path, extracted) in [
        (1, 0, "eng", "cached.srt", 1),
        (2, 0, "eng", "", 0),
        (3, 1, "wrong", "", 0),
        (4, 1, "eng", "old/MOVIE.EN.SRT", 0),
        (5, 1, "eng", "old/movie.en.srt", 0),
        (6, 99, "eng", "generated.ass", 0),
    ] {
        sqlx::query(r#"INSERT INTO "Subtitle" ("Id","MovieMetadataId","SubtitleKind","StreamIndex","Codec","Default","Forced","SDH","Language","IsExtracted","Path","DateAdded","DateUpdated") VALUES (?,10,?,0,'subrip',0,0,0,?,?,?,'2026-09-24','2026-09-24')"#)
            .bind(id).bind(kind).bind(language).bind(extracted).bind(path)
            .execute(&pool).await.unwrap();
    }
    let stream = MediaStream {
        index: 0,
        kind: MediaStreamKind::Subtitle,
        codec: Some("subrip".into()),
        language: Some("eng".into()),
        ..MediaStream::default()
    };
    let mut previous = None;
    for _ in 0..2 {
        let mut tx = pool.begin().await.unwrap();
        update_local_file_subtitles(
            &mut tx,
            SubtitleMetadataOwner::Movie(10),
            std::slice::from_ref(&stream),
            &movie,
            true,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let rows: Vec<(i64, i32, String, Option<String>, bool)> = sqlx::query_as(
            r#"SELECT "Id","SubtitleKind","Language","Path","IsExtracted" FROM "Subtitle" ORDER BY "Id""#,
        ).fetch_all(&pool).await.unwrap();
        assert_eq!(rows.len(), 4);
        assert_eq!(
            rows[0],
            (1, 0, "eng".into(), Some("cached.srt".into()), true)
        );
        assert_eq!(rows[1].0, 4);
        assert_eq!(
            rows[1].3.as_deref(),
            temp.path().join("movie.en.srt").to_str()
        );
        assert_eq!(rows[2].1, 99);
        assert_eq!(rows[3].2, "dut");
        if let Some(previous) = previous.as_ref() {
            assert_eq!(&rows, previous);
        }
        previous = Some(rows);
    }
}

#[tokio::test]
async fn local_sidecar_flags_and_undefined_language_come_only_from_suffix() {
    let cases = [
        (".srt", "und", false, false, "subrip"),
        (".forced.srt", "und", true, false, "subrip"),
        (".sdh.srt", "und", false, true, "subrip"),
        (".cc.vtt", "und", false, true, "webvtt"),
        (".en.srt", "eng", false, false, "subrip"),
        (".en.forced.ass", "eng", true, false, "ass"),
        (".FORCED.EN.SDH.SRT", "eng", true, true, "subrip"),
    ];
    for stem in ["Movie", "Synthetic.Forced.SDH.CC.Title"] {
        let pool = common::test_db().await;
        seed_local_movie(&pool).await;
        let temp = tempdir().unwrap();
        let media = temp.path().join(format!("{stem}.mkv"));
        std::fs::write(&media, b"").unwrap();
        for (suffix, ..) in cases {
            std::fs::write(temp.path().join(format!("{stem}{suffix}")), b"caption").unwrap();
        }
        let mut first_ids = Vec::new();
        for full_path in [false, true, true] {
            let mut tx = pool.begin().await.unwrap();
            update_local_file_subtitles(
                &mut tx,
                SubtitleMetadataOwner::Movie(10),
                &[],
                &media,
                full_path,
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();
            let rows = sqlx::query(
                r#"SELECT "Id", "Language", "Forced", "SDH", "Codec", "Path"
                FROM "Subtitle" WHERE "MovieMetadataId" = 10 ORDER BY "Id""#,
            )
            .fetch_all(&pool)
            .await
            .unwrap();
            assert_eq!(rows.len(), cases.len());
            let ids: Vec<i64> = rows.iter().map(|row| row.get("Id")).collect();
            if first_ids.is_empty() {
                first_ids = ids;
            } else {
                assert_eq!(ids, first_ids);
            }
            for (suffix, language, forced, sdh, codec) in cases {
                let filename = format!("{stem}{suffix}");
                let expected_path = if full_path {
                    temp.path().join(&filename).display().to_string()
                } else {
                    filename
                };
                let row = rows
                    .iter()
                    .find(|row| row.get::<String, _>("Path") == expected_path)
                    .unwrap();
                assert_eq!(row.get::<String, _>("Language"), language);
                assert_eq!(row.get::<bool, _>("Forced"), forced);
                assert_eq!(row.get::<bool, _>("SDH"), sdh);
                assert_eq!(row.get::<String, _>("Codec"), codec);
            }
        }
    }
}
