#![cfg(not(feature = "mysql"))]
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
async fn local_sidecars_require_exact_basename_and_strip_extension_for_language() {
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
    assert_eq!(rows[0].get::<Option<String>, _>("Language"), None);
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
