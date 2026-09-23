use super::*;

async fn fixture() -> Pool<Db> {
    let pool = crate::db::init("sqlite::memory:").await.unwrap();
    for sql in [
        r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#,
        r#"INSERT INTO "Library" ("Id","MediaSourceId","MediaKind") VALUES (1,1,1)"#,
        r#"INSERT INTO "LibraryPath" ("Id","LibraryId","Path") VALUES (1,1,'synthetic')"#,
        r#"INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES (10,1,0)"#,
        r#"INSERT INTO "Movie" ("Id") VALUES (10)"#,
        r#"INSERT INTO "MovieMetadata" ("Id","MovieId","DateAdded","DateUpdated","MetadataKind")
            VALUES (100,10,'2026-09-23 00:00:00','2026-09-23 00:00:00',1)"#,
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    pool
}

async fn seed(
    pool: &Pool<Db>,
    index: i32,
    codec: &str,
    kind: i32,
    extracted: bool,
    path: Option<&str>,
) {
    sqlx::query(
        r#"INSERT INTO "Subtitle" ("MovieMetadataId","SubtitleKind","StreamIndex","Codec",
        "Default","Forced","SDH","IsExtracted","Path","DateAdded","DateUpdated")
        VALUES (100,?,?,?,0,0,0,?,?,'2026-09-23 00:00:00','2026-09-23 00:00:00')"#,
    )
    .bind(kind)
    .bind(index)
    .bind(codec)
    .bind(extracted)
    .bind(path)
    .execute(pool)
    .await
    .unwrap();
}

fn cached(cache: &Path, index: i32) {
    let path = cache.join(relative_output_path(10, index, "subrip").unwrap());
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, b"synthetic subtitle").unwrap();
}

#[tokio::test]
async fn extraction_checks_the_cache_root_and_reextracts_missing_or_blank_state() {
    let pool = fixture().await;
    let root = tempfile::tempdir().unwrap();
    let cache = root.path().join("cache");
    // Cache is outside cwd: existence must be checked after joining this root.
    assert_ne!(cache, std::env::current_dir().unwrap());
    for (index, extracted, path, exists) in [
        (2, true, Some("recorded.srt"), true),
        (3, true, Some("recorded.srt"), false),
        (4, false, Some("recorded.srt"), true),
        (5, true, None, true),
        (6, true, Some(""), true),
        (7, true, Some(" \t\r\n"), true),
    ] {
        seed(
            &pool,
            index,
            "subrip",
            subtitle_kind::EMBEDDED,
            extracted,
            path,
        )
        .await;
        if exists {
            cached(&cache, index);
        }
    }
    let mut selected: Vec<_> = subtitles_to_extract(&pool, 10, &cache)
        .await
        .unwrap()
        .into_iter()
        .map(|subtitle| subtitle.stream_index)
        .collect();
    selected.sort_unstable();
    assert_eq!(selected, [3, 4, 5, 6, 7]);
    // An unrelated empty cache must not reuse artifacts from the first root.
    let other = root.path().join("other-cache");
    assert_eq!(
        subtitles_to_extract(&pool, 10, &other).await.unwrap().len(),
        6
    );
}

#[tokio::test]
async fn pending_subset_excludes_images_and_sidecars_without_renumbering_streams() {
    let pool = fixture().await;
    let cache = tempfile::tempdir().unwrap();
    seed(
        &pool,
        2,
        "subrip",
        subtitle_kind::EMBEDDED,
        true,
        Some("cached.srt"),
    )
    .await;
    cached(cache.path(), 2);
    seed(
        &pool,
        3,
        "hdmv_pgs_subtitle",
        subtitle_kind::EMBEDDED,
        false,
        None,
    )
    .await;
    seed(&pool, 4, "subrip", subtitle_kind::EMBEDDED, false, None).await;
    seed(&pool, 5, "subrip", subtitle_kind::SIDECAR, false, None).await;
    let pending = subtitles_to_extract(&pool, 10, cache.path()).await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].stream_index, 4);
    // Upstream 4ffd775a corrected cd298820's pending-only subtitle ordinal.
    // Absolute stream indices already preserve the full input stream identity.
    let args = build_subtitle_args("synthetic.mkv", &pending);
    let mappings: Vec<_> = args
        .windows(2)
        .filter(|pair| pair[0] == "-map")
        .map(|pair| pair[1].as_str())
        .collect();
    assert_eq!(mappings, ["0:4"]);
}
