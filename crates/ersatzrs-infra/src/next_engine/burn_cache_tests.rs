use super::*;

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
async fn completed_artifact_survives_restart_and_lease_only_extends() {
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
