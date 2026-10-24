use super::*;

#[tokio::test]
async fn strips_only_nul_units_and_preserves_multilingual_encodings() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("caption.ass");
    let text = "Ā café 日本語\0 Ελληνικά\0";
    let expected = text.replace('\0', "");
    let mut encodings = vec![(text.as_bytes().to_vec(), expected.as_bytes().to_vec())];
    for little in [true, false] {
        let encode = |s: &str| {
            let mut bytes = if little {
                vec![0xff, 0xfe]
            } else {
                vec![0xfe, 0xff]
            };
            for unit in s.encode_utf16() {
                bytes.extend(if little {
                    unit.to_le_bytes()
                } else {
                    unit.to_be_bytes()
                });
            }
            bytes
        };
        encodings.push((encode(text), encode(&expected)));
        let encode = |s: &str| {
            let mut bytes = if little {
                vec![0xff, 0xfe, 0, 0]
            } else {
                vec![0, 0, 0xfe, 0xff]
            };
            for character in s.chars() {
                let unit = u32::from(character);
                bytes.extend(if little {
                    unit.to_le_bytes()
                } else {
                    unit.to_be_bytes()
                });
            }
            bytes
        };
        encodings.push((encode(text), encode(&expected)));
    }
    for (original, expected) in encodings {
        fs::write(&path, original).await.unwrap();
        assert!(strip(&path).await.unwrap());
        assert_eq!(fs::read(&path).await.unwrap(), expected);
        let modified = fs::metadata(&path).await.unwrap().modified().unwrap();
        assert!(!strip(&path).await.unwrap());
        assert_eq!(
            fs::metadata(&path).await.unwrap().modified().unwrap(),
            modified
        );
    }
}

#[tokio::test]
async fn chunk_boundaries_empty_and_clean_readonly_files_are_preserved() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("caption.srt");
    let mut text = vec![b'A'; 64 * 1024 - 1];
    text.extend([0, 0, b'B', 0]);
    fs::write(&path, &text).await.unwrap();
    assert!(strip(&path).await.unwrap());
    assert_eq!(
        fs::read(&path).await.unwrap(),
        text.into_iter().filter(|v| *v != 0).collect::<Vec<_>>()
    );
    fs::write(&path, []).await.unwrap();
    assert!(!strip(&path).await.unwrap());
    fs::write(&path, b"clean").await.unwrap();
    let original_permissions = fs::metadata(&path).await.unwrap().permissions();
    let mut readonly = original_permissions.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&path, readonly).await.unwrap();
    let result = strip(&path).await;
    fs::set_permissions(&path, original_permissions)
        .await
        .unwrap();
    assert!(!result.unwrap());
}

#[tokio::test]
async fn failed_and_cancelled_partial_writes_preserve_original_and_remove_temporary() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("caption.ass");
    let original = b"original\0captions";
    fs::write(&path, original).await.unwrap();
    let metadata = fs::metadata(&path).await.unwrap();
    let result = replace_file(&path, &metadata, async |file| {
        file.write_all(b"partial").await?;
        Err(io::Error::other("synthetic write failure"))
    })
    .await;
    assert!(result.is_err());
    assert_eq!(fs::read(&path).await.unwrap(), original);
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    let notified = std::sync::Arc::new(tokio::sync::Notify::new());
    let observed = notified.clone();
    let owned_path = path.clone();
    let task = tokio::spawn(async move {
        replace_file(&owned_path, &metadata, async |file| {
            file.write_all(b"partial").await?;
            notified.notify_one();
            std::future::pending::<io::Result<()>>().await
        })
        .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), observed.notified())
        .await
        .unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert_eq!(fs::read(&path).await.unwrap(), original);
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}

async fn flag(pool: &DbPool, key: &str) -> bool {
    sqlx::query_scalar::<_, String>(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key"=?"#)
        .bind(key)
        .fetch_one(pool)
        .await
        .unwrap()
        == "true"
}

#[tokio::test]
async fn cache_repair_is_scoped_persistent_and_one_time() {
    let directory = tempfile::tempdir().unwrap();
    let cache = directory.path().join("cache");
    fs::create_dir_all(cache.join("nested")).await.unwrap();
    for name in ["a.ass", "b.srt", "nested/c.vtt"] {
        fs::write(cache.join(name), b"captions\0retained")
            .await
            .unwrap();
    }
    fs::write(cache.join("image.sup"), b"binary\0data")
        .await
        .unwrap();
    fs::write(directory.path().join("sidecar.ass"), b"external\0data")
        .await
        .unwrap();
    let database = format!("sqlite:{}", directory.path().join("state.db").display());
    let pool = crate::db::init(&database).await.unwrap();
    let report = repair_once(&pool, &cache).await.unwrap();
    assert_eq!(
        report,
        RepairReport {
            repaired: 3,
            complete: true,
            ..Default::default()
        }
    );
    for name in ["a.ass", "b.srt", "nested/c.vtt"] {
        assert_eq!(
            fs::read(cache.join(name)).await.unwrap(),
            b"captionsretained"
        );
    }
    assert_eq!(
        fs::read(cache.join("image.sup")).await.unwrap(),
        b"binary\0data"
    );
    assert_eq!(
        fs::read(directory.path().join("sidecar.ass"))
            .await
            .unwrap(),
        b"external\0data"
    );
    assert!(flag(&pool, REPAIR_DONE_KEY).await);
    assert!(!flag(&pool, REPAIR_PENDING_KEY).await);
    pool.close().await;
    let pool = crate::db::init(&database).await.unwrap();
    assert_eq!(
        repair_once(&pool, &cache).await.unwrap(),
        RepairReport {
            complete: true,
            ..Default::default()
        }
    );
    // An old deferred attempt cannot undo a completed repair's marker/health state.
    record_state(&pool, false).await.unwrap();
    assert!(flag(&pool, REPAIR_DONE_KEY).await);
    assert!(!flag(&pool, REPAIR_PENDING_KEY).await);
}

#[tokio::test]
async fn busy_extraction_and_readonly_failures_remain_retryable() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("caption.ass");
    fs::write(&path, b"captions\0retained").await.unwrap();
    let pool = crate::db::init("sqlite::memory:").await.unwrap();
    let lease = cache_lease(directory.path(), false).unwrap().unwrap();
    assert!(!repair_once(&pool, directory.path()).await.unwrap().complete);
    assert!(flag(&pool, REPAIR_PENDING_KEY).await);
    assert_eq!(fs::read(&path).await.unwrap(), b"captions\0retained");
    drop(lease);
    let permissions = fs::metadata(&path).await.unwrap().permissions();
    let mut readonly = permissions.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&path, readonly).await.unwrap();
    let report = repair_once(&pool, directory.path()).await.unwrap();
    fs::set_permissions(&path, permissions).await.unwrap();
    assert_eq!(report.failed, 1);
    assert!(!report.complete);
    assert!(!flag(&pool, REPAIR_DONE_KEY).await);
    assert_eq!(fs::read(&path).await.unwrap(), b"captions\0retained");
    assert!(repair_once(&pool, directory.path()).await.unwrap().complete);
    assert!(!flag(&pool, REPAIR_PENDING_KEY).await);
}

// Unix-only link fixture; Windows link creation requires an external privilege.
// Production uses the same portable symlink_metadata/file_type guard on all RIDs.
#[cfg(unix)]
#[tokio::test]
async fn cache_repair_never_follows_file_directory_or_root_links() {
    let directory = tempfile::tempdir().unwrap();
    let cache = directory.path().join("cache");
    let outside = directory.path().join("outside");
    fs::create_dir_all(&cache).await.unwrap();
    fs::create_dir_all(&outside).await.unwrap();
    let target = outside.join("external.ass");
    fs::write(&target, b"outside\0data").await.unwrap();
    std::os::unix::fs::symlink(&target, cache.join("linked.ass")).unwrap();
    std::os::unix::fs::symlink(&outside, cache.join("directory")).unwrap();
    let pool = crate::db::init("sqlite::memory:").await.unwrap();
    assert!(repair_once(&pool, &cache).await.unwrap().complete);
    assert_eq!(fs::read(&target).await.unwrap(), b"outside\0data");
    let linked_root = directory.path().join("root-link");
    std::os::unix::fs::symlink(&outside, &linked_root).unwrap();
    let pool = crate::db::init("sqlite::memory:").await.unwrap();
    assert!(!repair_once(&pool, &linked_root).await.unwrap().complete);
    assert_eq!(fs::read(&target).await.unwrap(), b"outside\0data");
}
