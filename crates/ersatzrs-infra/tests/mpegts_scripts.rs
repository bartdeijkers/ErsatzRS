use ersatzrs_infra::mpegts_scripts::discover_in;

#[tokio::test]
async fn mpegts_scripts_reload_edits_remove_missing_and_retry_invalid_definitions() {
    let root = tempfile::tempdir().unwrap();
    let custom = root.path().join("custom");
    let broken = root.path().join("broken");
    std::fs::create_dir_all(&custom).unwrap();
    std::fs::create_dir_all(&broken).unwrap();
    std::fs::write(
        custom.join("mpegts.yml"),
        "name: Original\nlinux_script: old.sh\n",
    )
    .unwrap();
    std::fs::write(broken.join("mpegts.yml"), "name: [invalid").unwrap();
    let initial = discover_in(root.path()).await.unwrap();
    assert_eq!(initial.len(), 1);
    assert_eq!(initial[0].folder, custom);
    std::fs::write(
        custom.join("mpegts.yml"),
        "name: Updated\nlinux_script: new.sh\n",
    )
    .unwrap();
    std::fs::write(
        broken.join("mpegts.yml"),
        "name: A repaired script\nwindows_script: run.bat\n",
    )
    .unwrap();
    let updated = discover_in(root.path()).await.unwrap();
    assert_eq!(updated.len(), 2);
    assert_eq!(updated[0].id, "broken");
    assert_eq!(
        updated[0].definition.windows_script.as_deref(),
        Some("run.bat")
    );
    assert_eq!(
        updated[1].definition.linux_script.as_deref(),
        Some("new.sh")
    );
    assert_eq!(initial[0].definition.name, "Original");
    std::fs::remove_file(custom.join("mpegts.yml")).unwrap();
    std::fs::remove_dir_all(broken).unwrap();
    assert!(discover_in(root.path()).await.unwrap().is_empty());
    std::fs::write(custom.join("mpegts.yml"), "name: Restored\n").unwrap();
    assert_eq!(
        discover_in(root.path()).await.unwrap()[0].definition.name,
        "Restored"
    );
}

#[tokio::test]
async fn mpegts_scripts_concurrent_reads_own_complete_independent_snapshots() {
    let root = tempfile::tempdir().unwrap();
    let folder = root.path().join("custom");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(
        folder.join("mpegts.yml"),
        "name: Custom\nlinux_script: run.sh\n",
    )
    .unwrap();
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..8 {
        let path = root.path().to_owned();
        tasks.spawn(async move { discover_in(&path).await.unwrap() });
    }
    while let Some(result) = tasks.join_next().await {
        let scripts = result.unwrap();
        assert_eq!(scripts.len(), 1);
        assert_eq!(scripts[0].id, "custom");
        assert_eq!(
            scripts[0].definition.linux_script.as_deref(),
            Some("run.sh")
        );
    }
    assert!(discover_in(&root.path().join("absent"))
        .await
        .unwrap()
        .is_empty());
}
