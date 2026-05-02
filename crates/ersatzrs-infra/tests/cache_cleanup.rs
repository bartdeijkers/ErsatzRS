#![cfg(not(feature = "mysql"))]

use ersatzrs_infra::cache_cleanup::{clean_caches, CacheCleanupConfig};

#[tokio::test]
async fn cache_cleanup_empties_transcode_folder() {
    let temp = tempfile::tempdir().unwrap();
    let transcode_dir = temp.path().join("transcode");
    tokio::fs::create_dir_all(transcode_dir.join("nested"))
        .await
        .unwrap();
    tokio::fs::write(transcode_dir.join("a.ts"), b"segment")
        .await
        .unwrap();
    tokio::fs::write(transcode_dir.join("nested").join("b.ts"), b"segment")
        .await
        .unwrap();

    let report = clean_caches(&CacheCleanupConfig::from_paths(
        Some(transcode_dir.clone()),
        None,
        None,
    ))
    .await
    .unwrap();

    assert_eq!(report.transcode_files_removed, 2);
    assert!(!transcode_dir.join("a.ts").exists());
    assert!(!transcode_dir.join("nested").join("b.ts").exists());
}
