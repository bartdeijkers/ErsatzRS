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
    let args = build_subtitle_args("synthetic.mkv", &pending, cache.path());
    let mappings: Vec<_> = args
        .windows(2)
        .filter(|pair| pair[0] == "-map")
        .map(|pair| pair[1].as_str())
        .collect();
    assert_eq!(mappings, ["0:4"]);
}

async fn jellyfin_fixture(logical: [i32; 3]) -> Pool<Db> {
    let pool = fixture().await;
    sqlx::raw_sql(r#"INSERT INTO "JellyfinMediaSource" ("Id") VALUES (1);
        INSERT INTO "JellyfinLibrary" ("Id","ShouldSyncItems") VALUES (1,1);
        INSERT INTO "JellyfinMovie" ("Id","ItemId") VALUES (10,'synthetic');
        INSERT INTO "MediaVersion" ("Id","MovieId","DateAdded","DateUpdated","Duration","Height","Width","VideoScanKind","ProviderStreamIndexVersion")
            VALUES (200,10,'2026-11-17','2026-11-17','00:00:02',180,320,0,1);
        INSERT INTO "MediaFile" ("Id","MediaVersionId","Path","PathHash") VALUES (300,200,'synthetic/clip.mkv','synthetic');"#).execute(&pool).await.unwrap();
    for (physical, logical) in logical.into_iter().enumerate() {
        sqlx::query(r#"INSERT INTO "MediaStream" ("MediaVersionId","Index","MediaStreamKind","Codec","Channels","Default","Forced") VALUES (200,?,?,?,?,0,0)"#)
            .bind(logical).bind([1,2,3][physical]).bind(["h264","aac","subrip"][physical]).bind(i32::from(physical == 1)).execute(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO "ProviderStreamIndex" VALUES (200,?,?)"#)
            .bind(logical)
            .bind(i32::try_from(physical).unwrap())
            .execute(&pool)
            .await
            .unwrap();
    }
    seed(
        &pool,
        logical[2],
        "subrip",
        subtitle_kind::EMBEDDED,
        false,
        None,
    )
    .await;
    pool
}

#[tokio::test]
async fn row295_extraction_maps_the_selected_version_without_rekeying_cache() {
    let cache = tempfile::tempdir().unwrap();
    let mut snapshots = Vec::new();
    for logical in [[2, 3, 4], [0, 1, 2], [0, 2, 3]] {
        let pool = jellyfin_fixture(logical).await;
        let input = resolve_input_path(&pool, 10, &[]).await.unwrap().unwrap();
        let mut pending = subtitles_to_extract(&pool, 10, cache.path()).await.unwrap();
        input.map_candidates(&pool, &mut pending).await.unwrap();
        assert_eq!(pending[0].stream_index, logical[2]);
        assert_eq!(pending[0].physical_index, 2);
        assert_eq!(
            pending[0].relative_output,
            relative_output_path(10, logical[2], "subrip").unwrap()
        );
        snapshots.push(
            build_subtitle_args("synthetic.mkv", &pending, Path::new("cache"))
                .join("\n")
                .replace('\\', "/"),
        );
    }
    insta::assert_snapshot!(
        "row295_extraction_physical_map_logical_cache",
        snapshots.join("\n\n")
    );
}

#[tokio::test]
async fn row295_extraction_publishes_only_current_map_file_and_subtitle_state() {
    for change in [
        "none", "map", "file", "subtitle", "missing", "pathmap", "owner", "sidecar",
    ] {
        let pool = jellyfin_fixture([2, 3, 4]).await;
        let cache = tempfile::tempdir().unwrap();
        let staged = tempfile::tempdir().unwrap();
        let input = resolve_input_path(&pool, 10, &[]).await.unwrap().unwrap();
        let mut pending = subtitles_to_extract(&pool, 10, cache.path()).await.unwrap();
        input.map_candidates(&pool, &mut pending).await.unwrap();
        let path = &pending[0].relative_output;
        for (root, bytes) in [
            (cache.path(), b"old caption".as_slice()),
            (staged.path(), b"new caption".as_slice()),
        ] {
            let full = root.join(path);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, bytes).unwrap();
        }
        let sql = match change {
            "map" => Some(r#"UPDATE "MediaVersion" SET "DateUpdated"='changed' WHERE "Id"=200"#),
            "file" => Some(r#"UPDATE "MediaFile" SET "Path"='other.mkv' WHERE "Id"=300"#),
            "subtitle" => Some(r#"UPDATE "Subtitle" SET "DateUpdated"='changed'"#),
            "owner" => Some(r#"UPDATE "Subtitle" SET "MovieMetadataId"=NULL"#),
            "sidecar" => Some(r#"UPDATE "Subtitle" SET "SubtitleKind"=1"#),
            "pathmap" => Some(
                r#"INSERT INTO "JellyfinPathReplacement" ("JellyfinMediaSourceId","JellyfinPath","LocalPath") VALUES (1,'synthetic','changed')"#,
            ),
            "missing" => Some(r#"DELETE FROM "ProviderStreamIndex" WHERE "StreamIndex"=4"#),
            _ => None,
        };
        if let Some(sql) = sql {
            sqlx::query(sql).execute(&pool).await.unwrap();
        }
        let result = publish_extracted(&pool, &input, cache.path(), staged.path(), &pending).await;
        assert_eq!(
            result.is_err(),
            matches!(change, "map" | "file" | "missing" | "pathmap")
        );
        assert_eq!(
            std::fs::read(cache.path().join(path)).unwrap(),
            if change == "none" {
                b"new caption"
            } else {
                b"old caption"
            }
        );
        let (index, extracted, path): (i32, bool, Option<String>) =
            sqlx::query_as(r#"SELECT "StreamIndex","IsExtracted","Path" FROM "Subtitle""#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(index, 4);
        assert_eq!(extracted, change == "none");
        assert_eq!(path.is_some(), change == "none");
    }
}

#[tokio::test]
async fn row295_extraction_uses_only_the_owning_servers_mapping_and_matching_version() {
    let pool = jellyfin_fixture([2, 3, 4]).await;
    sqlx::raw_sql(r#"INSERT INTO "MediaSource" ("Id") VALUES (2);
        INSERT INTO "JellyfinMediaSource" ("Id") VALUES (2);
        INSERT INTO "JellyfinPathReplacement" ("JellyfinMediaSourceId","JellyfinPath","LocalPath") VALUES (2,'synthetic','foreign');"#).execute(&pool).await.unwrap();
    let global = load_path_replacements(&pool).await.unwrap();
    assert_eq!(
        replace_path(
            &global,
            "synthetic/clip.mkv",
            crate::media_server::HOST_PLATFORM
        ),
        Path::new("foreign").join("clip.mkv").to_str().unwrap()
    );
    let input = resolve_input_path(&pool, 10, &global)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(input.path, "synthetic/clip.mkv");
    sqlx::query(r#"UPDATE "Subtitle" SET "Codec"='ass'"#)
        .execute(&pool)
        .await
        .unwrap();
    let cache = tempfile::tempdir().unwrap();
    let mut candidates = subtitles_to_extract(&pool, 10, cache.path()).await.unwrap();
    assert!(
        input.map_candidates(&pool, &mut candidates).await.is_err(),
        "metadata from a different version cannot be projected onto the head file"
    );
}

#[tokio::test]
#[ignore = "requires hash-qualified FFmpeg9 and an isolated extraction cache"]
async fn row295_real_extraction_preserves_logical_identity_for_all_layouts() {
    let ffmpeg = extraction_runtime_path("ERSATZRS_EXTRACTION_TEST_FFMPEG")
        .to_str()
        .unwrap()
        .to_owned();
    let _owned_data = extraction_runtime_path(crate::file_system_layout::APP_DATA_DIR_ENV_VAR);
    let root = tempfile::tempdir().unwrap();
    let cues = root.path().join("caption.srt");
    let clip = root.path().join("clip.mkv");
    std::fs::write(
        &cues,
        "1\n00:00:00,000 --> 00:00:01,900\nrow295 selected caption\n",
    )
    .unwrap();
    let output = Command::new(&ffmpeg)
        .args([
            "-nostdin",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=blue:s=320x180:r=1",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000",
            "-i",
        ])
        .arg(&cues)
        .args([
            "-map", "0:v:0", "-map", "1:a:0", "-map", "2:s:0", "-t", "2", "-c:v", "libx264",
            "-threads", "1", "-c:a", "aac", "-c:s", "srt",
        ])
        .arg(&clip)
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cache = subtitle_cache_folder();
    for logical in [[2, 3, 4], [0, 1, 2], [0, 2, 3]] {
        let pool = jellyfin_fixture(logical).await;
        sqlx::query(r#"UPDATE "MediaFile" SET "Path"=? WHERE "Id"=300"#)
            .bind(clip.to_str().unwrap())
            .execute(&pool)
            .await
            .unwrap();
        let input = resolve_input_path(&pool, 10, &[]).await.unwrap().unwrap();
        let relative = relative_output_path(10, logical[2], "subrip").unwrap();
        let full = cache.join(&relative);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(&full, b"prior cached caption").unwrap();
        extract_subtitles(&pool, 10, &input, &ffmpeg).await.unwrap();
        assert!(std::fs::read_to_string(&full)
            .unwrap()
            .contains("row295 selected caption"));
        let (index, extracted, path): (i32, bool, String) =
            sqlx::query_as(r#"SELECT "StreamIndex","IsExtracted","Path" FROM "Subtitle""#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(index, logical[2]);
        assert!(extracted);
        assert_eq!(Path::new(&path), relative);
    }
    assert_batch_continues(&clip).await;
    // A failed real input open preserves the prior file and pending metadata.
    let pool = jellyfin_fixture([2, 3, 4]).await;
    let input = resolve_input_path(&pool, 10, &[]).await.unwrap().unwrap();
    let prior = cache.join(relative_output_path(10, 4, "subrip").unwrap());
    std::fs::write(&prior, b"must survive ffmpeg failure").unwrap();
    extract_subtitles(&pool, 10, &input, &ffmpeg).await.unwrap();
    assert_eq!(
        std::fs::read(&prior).unwrap(),
        b"must survive ffmpeg failure"
    );
    let done: bool = sqlx::query_scalar(r#"SELECT "IsExtracted" FROM "Subtitle""#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(!done);
}

async fn assert_batch_continues(clip: &Path) {
    // A repair-pending item must not prevent the next valid batch item.
    let pool = jellyfin_fixture([2, 3, 4]).await;
    sqlx::query(r#"UPDATE "MediaFile" SET "Path"=? WHERE "Id"=300"#)
        .bind(clip.to_str().unwrap())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::raw_sql(r#"INSERT INTO "MediaItem" ("Id","LibraryPathId") VALUES (9,1);
        INSERT INTO "Movie" ("Id") VALUES (9); INSERT INTO "JellyfinMovie" ("Id","ItemId") VALUES (9,'pending');
        INSERT INTO "MovieMetadata" ("Id","MovieId","DateAdded","DateUpdated","MetadataKind") VALUES (99,9,'2026-11-17','2026-11-17',1);
        INSERT INTO "MediaVersion" ("Id","MovieId","DateAdded","DateUpdated","Duration","Height","Width","VideoScanKind") VALUES (199,9,'2026-11-17','2026-11-17','00:00:02',180,320,0);
        INSERT INTO "MediaFile" ("Id","MediaVersionId","Path","PathHash") VALUES (299,199,'pending.mkv','pending');
        INSERT INTO "Subtitle" ("MovieMetadataId","SubtitleKind","StreamIndex","Codec","Default","Forced","SDH","IsExtracted","DateAdded","DateUpdated") VALUES (99,0,4,'subrip',0,0,0,0,'2026-11-17','2026-11-17');"#).execute(&pool).await.unwrap();
    for key in [
        crate::ffmpeg_config::CONFIG_KEY_FFMPEG_USE_EMBEDDED_SUBTITLES,
        crate::ffmpeg_config::CONFIG_KEY_FFMPEG_EXTRACT_EMBEDDED_SUBTITLES,
    ] {
        sqlx::query(r#"INSERT INTO "ConfigElement" ("Key","Value") VALUES (?,'True')"#)
            .bind(key)
            .execute(&pool)
            .await
            .unwrap();
    }
    extract_embedded_media_item_subtitles(&pool, vec![9, 10])
        .await
        .unwrap();
    let states: Vec<(i64, bool)> = sqlx::query_as(
        r#"SELECT "MovieMetadataId","IsExtracted" FROM "Subtitle" ORDER BY "MovieMetadataId""#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(states, [(99, false), (100, true)]);
}

#[allow(
    clippy::disallowed_methods,
    reason = "Explicit opt-in proof locations, never production configuration."
)]
fn extraction_runtime_path(name: &str) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .expect("set the documented isolated extraction proof paths")
}
