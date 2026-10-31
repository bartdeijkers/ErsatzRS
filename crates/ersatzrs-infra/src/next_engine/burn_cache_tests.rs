use super::*;

#[tokio::test]
async fn provider_preparation_refreshes_content_and_rejects_stale_publication() {
    use httpmock::prelude::*;
    let server = MockServer::start_async().await;
    let pool = crate::next_engine::selection::tests::fixture_pool().await;
    for sql in [
        r#"INSERT INTO "JellyfinMediaSource" ("Id") VALUES (1)"#,
        r#"INSERT INTO "JellyfinLibrary" ("Id","ShouldSyncItems") VALUES (1,1)"#,
        r#"INSERT INTO "JellyfinMovie" ("Id","ItemId") VALUES (10,'movie')"#,
        r#"UPDATE "Subtitle" SET "Path" = NULL WHERE "Id" = 2"#,
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    sqlx::query(
        r#"INSERT INTO "JellyfinConnection" ("Address","JellyfinMediaSourceId") VALUES (?,1)"#,
    )
    .bind(server.base_url())
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(r#"INSERT INTO "ConfigElement" ("Key","Value") VALUES (?, '<synthetic-key>')"#)
        .bind(format!("jellyfin.api_key.{}", server.base_url()))
        .execute(&pool)
        .await
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    let cache = BurnCache::new(root.path().to_owned());
    let expiry = Utc::now() + Duration::minutes(5);
    let mut previous = None;
    for text in ["First captions", "Changed captions"] {
        let body = format!("1\n00:00:00,000 --> 00:00:03,000\n{text}\n");
        let endpoint = server
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/Videos/movie/movie/Subtitles/9/9/Stream.srt");
                then.status(200).body(&body);
            })
            .await;
        let path = cache
            .prepare_provider(
                &pool,
                (&key(1), 2),
                expiry,
                Instant::now() + std::time::Duration::from_secs(10),
                || async { true },
            )
            .await
            .unwrap();
        assert_eq!(tokio::fs::read_to_string(&path).await.unwrap(), body);
        if let Some(previous) = &previous {
            assert_ne!(
                &path, previous,
                "changed provider bytes need a distinct artifact"
            );
            assert!(tokio::fs::read_to_string(previous)
                .await
                .unwrap()
                .contains("First captions"));
        }
        assert_eq!(
            cache
                .prepare_provider(
                    &pool,
                    (&key(1), 2),
                    expiry,
                    Instant::now() + std::time::Duration::from_secs(10),
                    || async { false },
                )
                .await
                .unwrap_err(),
            CacheError::Stale
        );
        previous = Some(path);
        endpoint.delete_async().await;
    }
}

fn key(value: u8) -> String {
    format!("{value:064x}")
}

fn source() -> Source {
    Source::Local {
        path: "synthetic/source.mkv".into(),
        in_point_ms: None,
        out_point_ms: None,

        probe_hint: None,
    }
}

fn seed(root: &Path, key: &str, bytes: u64, expiry: DateTime<Utc>) -> PathBuf {
    std::fs::create_dir_all(root).unwrap();
    let path = root.join(format!("{key}.srt"));
    std::fs::File::create(&path)
        .unwrap()
        .set_len(bytes)
        .unwrap();
    persist_manifest(
        root,
        &Manifest {
            version: VERSION,
            key: key.to_owned(),
            format: "srt".into(),
            bytes,
            expires: expiry,
        },
    )
    .unwrap();
    path
}

async fn prepare(
    cache: &BurnCache,
    key: &str,
    finish: DateTime<Utc>,
) -> Result<PathBuf, CacheError> {
    cache
        .prepare(
            key,
            &source(),
            4,
            "subrip",
            Path::new("missing-synthetic-ffmpeg"),
            finish,
            Instant::now() + std::time::Duration::from_secs(5),
            || async { true },
        )
        .await
}

#[tokio::test]
async fn unsanitized_cache_is_rebuilt_and_atomic_replacement_is_reusable() {
    let root = tempfile::tempdir().unwrap();
    let identity = key(1);
    let expiry = Utc::now() + Duration::minutes(2);
    let old = seed(root.path(), &identity, 30, expiry);
    let manifest = root.path().join(format!("{identity}.json"));
    let mut record: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();
    record["version"] = 1.into();
    std::fs::write(&manifest, serde_json::to_vec(&record).unwrap()).unwrap();
    assert!(read_manifest(root.path(), &identity).unwrap().is_none());
    let cache = BurnCache::new(root.path().to_owned());
    let result = cache
        .prepare_artifact(
            &identity,
            "srt",
            expiry,
            Instant::now() + std::time::Duration::from_secs(5),
            || async { true },
            |path, file| async move {
                assert!(!old.exists(), "old cache must be retired before building");
                drop(file);
                tokio::fs::write(&path, b"caption\0 caf\xc3\xa9")
                    .await
                    .unwrap();
                assert_eq!(
                    crate::subtitle_cache_nul::strip_or_delete(&path).await,
                    crate::subtitle_cache_nul::StripResult::Stripped
                );
                Ok(())
            },
        )
        .await
        .unwrap();
    assert_eq!(std::fs::read(&result).unwrap(), "caption café".as_bytes());
    assert_eq!(
        prepare(&cache, &identity, Utc::now()).await.unwrap(),
        result
    );
    assert_eq!(
        read_manifest(root.path(), &identity)
            .unwrap()
            .unwrap()
            .version,
        VERSION
    );
}

#[tokio::test(start_paused = true)]
async fn completed_artifact_survives_restart_and_lease_only_extends() {
    // This verifies cache identity and lease persistence, not filesystem speed.
    // Keep paused Tokio time runnable while external filesystem workers finish;
    // otherwise auto-advance can expire the unchanged preparation deadline.
    let clock_guard = tokio::spawn(async {
        loop {
            tokio::task::yield_now().await;
        }
    });
    let started = Instant::now();
    let root = tempfile::tempdir().unwrap();
    let identity = key(1);
    let original_expiry = Utc::now() + Duration::minutes(2);
    let path = seed(root.path(), &identity, 30, original_expiry);
    let first = BurnCache::new(root.path().to_owned());
    assert_eq!(prepare(&first, &identity, Utc::now()).await.unwrap(), path);
    assert_eq!(
        read_manifest(root.path(), &identity)
            .unwrap()
            .unwrap()
            .expires,
        original_expiry
    );
    drop(first);
    let restarted = BurnCache::new(root.path().to_owned());
    let later_finish = Utc::now() + Duration::minutes(10);
    assert_eq!(
        prepare(&restarted, &identity, later_finish).await.unwrap(),
        path
    );
    assert_eq!(
        read_manifest(root.path(), &identity)
            .unwrap()
            .unwrap()
            .expires,
        later_finish + Duration::seconds(GRACE_SECONDS)
    );
    let manifest = std::fs::read_to_string(root.path().join(format!("{identity}.json"))).unwrap();
    assert!(!manifest.contains("synthetic/source"));
    assert!(!manifest.contains("headers"));
    assert_eq!(
        Instant::now(),
        started,
        "filesystem I/O must not advance test time"
    );
    clock_guard.abort();
    assert!(clock_guard.await.unwrap_err().is_cancelled());
}

#[tokio::test(start_paused = true)]
async fn waiting_cache_admission_expires_at_the_original_deadline() {
    let root = tempfile::tempdir().unwrap();
    let cache = Arc::new(BurnCache::new(root.path().to_owned()));
    let _held = cache.gate.lock().await;
    let waiting = Arc::clone(&cache);
    let started = Instant::now();
    let preparation = tokio::spawn(async move { prepare(&waiting, &key(1), Utc::now()).await });
    tokio::task::yield_now().await;
    assert!(!preparation.is_finished());
    tokio::time::advance(std::time::Duration::from_secs(4)).await;
    assert!(!preparation.is_finished(), "deadline must not expire early");
    tokio::time::advance(std::time::Duration::from_secs(1)).await;
    assert_eq!(preparation.await.unwrap(), Err(CacheError::Deadline));
    assert_eq!(Instant::now() - started, std::time::Duration::from_secs(5));
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn incomplete_and_expired_files_are_removed_but_live_leases_remain() {
    let root = tempfile::tempdir().unwrap();
    let live_key = key(1);
    let live = seed(root.path(), &live_key, 30, Utc::now() + Duration::hours(1));
    let expired = seed(root.path(), &key(2), 40, Utc::now() - Duration::seconds(1));
    let orphan = root.path().join(format!("{}.ass", key(3)));
    let partial = root.path().join(".burn-interrupted");
    std::fs::write(&orphan, b"uncommitted").unwrap();
    std::fs::write(&partial, b"partial").unwrap();
    let cache = BurnCache::new(root.path().to_owned());
    assert_eq!(prepare(&cache, &live_key, Utc::now()).await.unwrap(), live);
    assert!(live.exists());
    assert!(!expired.exists());
    assert!(!orphan.exists());
    assert!(!partial.exists());
    assert!(!root.path().join(format!("{}.json", key(2))).exists());
}

#[tokio::test]
async fn failed_extraction_never_commits_or_leaves_a_temporary_file() {
    let root = tempfile::tempdir().unwrap();
    let cache = BurnCache::new(root.path().to_owned());
    assert!(matches!(
        prepare(&cache, &key(1), Utc::now()).await,
        Err(CacheError::Extraction(burn_subtitles::BurnError::Start))
    ));
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn full_cache_does_not_evict_unexpired_artifacts() {
    let root = tempfile::tempdir().unwrap();
    let expiry = Utc::now() + Duration::hours(1);
    let mut paths = Vec::new();
    for index in 1..=7 {
        paths.push(seed(root.path(), &key(index), MAX_ARTIFACT_BYTES, expiry));
    }
    let cache = BurnCache::new(root.path().to_owned());
    assert_eq!(
        prepare(&cache, &key(8), Utc::now()).await,
        Err(CacheError::Full)
    );
    assert!(paths.iter().all(|path| path.exists()));
    assert_eq!(
        prepare(&cache, &key(1), Utc::now()).await.unwrap(),
        paths[0]
    );
}

#[tokio::test]
async fn admission_deadline_and_key_validation_precede_extraction() {
    let root = tempfile::tempdir().unwrap();
    let cache = BurnCache::new(root.path().to_owned());
    for invalid in [
        "../source".to_owned(),
        "A".repeat(64),
        "f".repeat(63),
        "g".repeat(64),
    ] {
        assert_eq!(
            prepare(&cache, &invalid, Utc::now()).await,
            Err(CacheError::Identity)
        );
    }
    let _held = cache.gate.lock().await;
    assert_eq!(
        cache
            .prepare(
                &key(1),
                &source(),
                0,
                "srt",
                Path::new("missing"),
                Utc::now(),
                Instant::now(),
                || async { true },
            )
            .await,
        Err(CacheError::Deadline)
    );
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn malformed_or_mismatched_completion_records_are_never_cache_hits() {
    let root = tempfile::tempdir().unwrap();
    let identity = key(1);
    let path = seed(root.path(), &identity, 30, Utc::now() + Duration::hours(1));
    std::fs::write(&path, b"truncated").unwrap();
    assert!(read_manifest(root.path(), &identity).unwrap().is_none());
    assert_eq!(cleanup_root(root.path()).unwrap(), (0, 0));
    assert!(!path.exists());
    let manifest = root.path().join(format!("{identity}.json"));
    std::fs::write(&manifest, vec![b'x'; 4096]).unwrap();
    assert!(read_manifest(root.path(), &identity).unwrap().is_none());
    assert_eq!(cleanup_root(root.path()).unwrap(), (0, 0));
}

#[tokio::test]
async fn cache_hit_revalidates_binding_before_extending_lease() {
    let root = tempfile::tempdir().unwrap();
    let identity = key(1);
    let expiry = Utc::now() + Duration::minutes(2);
    let path = seed(root.path(), &identity, 30, expiry);
    let cache = BurnCache::new(root.path().to_owned());
    let result = cache
        .prepare(
            &identity,
            &source(),
            4,
            "subrip",
            Path::new("missing"),
            Utc::now() + Duration::hours(1),
            Instant::now() + std::time::Duration::from_secs(5),
            || async { false },
        )
        .await;
    assert_eq!(result, Err(CacheError::Stale));
    assert!(path.exists());
    assert_eq!(
        read_manifest(root.path(), &identity)
            .unwrap()
            .unwrap()
            .expires,
        expiry
    );
}

#[test]
fn full_entry_limit_recovers_interrupted_lease_manifest_temporary_files() {
    let root = tempfile::tempdir().unwrap();
    let expiry = Utc::now() + Duration::hours(1);
    let first = seed(root.path(), &key(1), 30, expiry);
    let second = seed(root.path(), &key(2), 40, expiry);
    let pending = root.path().join(".burn-interrupted-lease");
    std::fs::write(&pending, b"uncommitted renewal").unwrap();
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 5);
    let (_, retained) = cleanup_with_limit(root.path(), 4).unwrap();
    assert_eq!(retained, 4);
    assert!(!pending.exists());
    assert!(first.exists() && second.exists());
    assert_eq!(cleanup_with_limit(root.path(), 3), Err(CacheError::Full));
    assert!(first.exists() && second.exists());
}

#[tokio::test]
async fn rebased_formats_share_leases_and_revalidate_hits() {
    let root = tempfile::tempdir().unwrap();
    let cache = BurnCache::new(root.path().into());
    let finish = Utc::now() + Duration::minutes(1);
    for (index, format) in [(1, "srt"), (2, "ass"), (3, "vtt")] {
        let identity = key(index);
        let path = cache
            .prepare_text(
                &identity,
                format,
                "complete cues",
                finish,
                Instant::now() + std::time::Duration::from_secs(5),
                || async { true },
            )
            .await
            .unwrap();
        assert_eq!(path.extension().unwrap(), format);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "complete cues");
        assert_eq!(
            read_manifest(root.path(), &identity)
                .unwrap()
                .unwrap()
                .expires,
            finish + Duration::seconds(GRACE_SECONDS)
        );
        assert_eq!(
            cache
                .prepare_text(
                    &identity,
                    format,
                    "complete cues",
                    finish,
                    Instant::now() + std::time::Duration::from_secs(5),
                    || async { false }
                )
                .await,
            Err(CacheError::Stale)
        );
    }
    assert_eq!(cleanup_root(root.path()).unwrap().1, 6);
}

#[tokio::test]
async fn rebased_artifact_failure_leaves_no_admitted_or_partial_entry() {
    let root = tempfile::tempdir().unwrap();
    let cache = BurnCache::new(root.path().into());
    let finish = Utc::now() + Duration::minutes(1);
    assert_eq!(
        cache
            .prepare_text(
                &key(1),
                "vtt",
                "complete cues",
                finish,
                Instant::now() + std::time::Duration::from_secs(5),
                || async { false }
            )
            .await,
        Err(CacheError::Stale)
    );
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    assert_eq!(
        cache
            .prepare_text(
                &key(1),
                "vtt",
                "",
                finish,
                Instant::now() + std::time::Duration::from_secs(5),
                || async { true }
            )
            .await,
        Err(CacheError::Storage)
    );
    assert_eq!(
        cache
            .prepare_text(
                &key(1),
                "vtt",
                "complete cues",
                finish,
                Instant::now(),
                || async { true }
            )
            .await,
        Err(CacheError::Deadline)
    );
}
