use super::*;
use std::io::Write as _;

fn config(root: &Path) -> NextRuntimeManagerConfig {
    NextRuntimeManagerConfig {
        override_folder: None,
        host_executable: root.join("app/host"),
        home: None,
        managed_root: root.join("state/next"),
        allow_download: true,
    }
}

fn synthetic(rid: &str) -> (Manifest, BTreeMap<String, Vec<u8>>) {
    let mut manifest = Manifest::accepted().unwrap();
    let files: BTreeMap<_, _> = payload_names(rid)
        .into_iter()
        .map(|name| {
            let bytes = format!("bounded synthetic {name}\n").into_bytes();
            (name, bytes)
        })
        .collect();
    let package = manifest.runtime_packages.get_mut(rid).unwrap();
    package.payload_sha256 = files
        .iter()
        .map(|(name, bytes)| (name.clone(), digest(bytes)))
        .collect();
    package.payload_bytes = files
        .iter()
        .map(|(name, bytes)| (name.clone(), u64::try_from(bytes.len()).unwrap()))
        .collect();
    let origin = serde_json::json!({
        "kind": "patched-source-build", "repository": manifest.upstream_repository,
        "source_revision": manifest.upstream_revision,
        "source_archive_sha256": manifest.source_archive_sha256,
        "cargo_lock_sha256": manifest.source_lock_sha256,
        "distribution_version": manifest.distribution_version,
        "rid": rid, "target": package.target,
        "patches": manifest.patches.iter().map(|p| serde_json::json!({"file":p.file,"sha256":p.sha256})).collect::<Vec<_>>(),
        "payload_sha256": package.payload_sha256,
    });
    let mut files = files;
    files.insert(
        "origin.json".to_owned(),
        serde_json::to_vec(&origin).unwrap(),
    );
    (manifest, files)
}

fn tar_bytes(manifest: &mut Manifest, rid: &str, entries: &[(String, Vec<u8>, u8)]) -> Vec<u8> {
    let root = manifest
        .package(rid)
        .unwrap()
        .asset
        .trim_end_matches(".tar.gz")
        .to_owned();
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut tar = tar::Builder::new(encoder);
    for (name, data, kind) in entries {
        let mut header = tar::Header::new_gnu();
        let path = format!("{root}/{name}");
        let path_bytes = path.as_bytes();
        assert!(path_bytes.len() < 100);
        header.as_mut_bytes()[..path_bytes.len()].copy_from_slice(path_bytes);
        header.set_size(u64::try_from(data.len()).unwrap());
        header.set_mode(0o644);
        header.set_entry_type(tar::EntryType::new(*kind));
        if *kind == b'2' {
            header.set_link_name("outside").unwrap();
        }
        header.set_cksum();
        tar.append(&header, data.as_slice()).unwrap();
    }
    let bytes = tar.into_inner().unwrap().finish().unwrap();
    let asset = manifest.package(rid).unwrap().asset.clone();
    manifest.artifact_sha256.insert(asset, digest(&bytes));
    bytes
}

fn good_archive(manifest: &mut Manifest, rid: &str, files: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    if rid != "win-x64" {
        return tar_bytes(
            manifest,
            rid,
            &files
                .iter()
                .map(|(name, bytes)| (name.clone(), bytes.clone(), b'0'))
                .collect::<Vec<_>>(),
        );
    }
    let root = manifest
        .package(rid)
        .unwrap()
        .asset
        .trim_end_matches(".zip")
        .to_owned();
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in files {
        zip.start_file(
            format!("{root}/{name}"),
            zip::write::SimpleFileOptions::default().unix_permissions(0o644),
        )
        .unwrap();
        zip.write_all(bytes).unwrap();
    }
    let bytes = zip.finish().unwrap().into_inner();
    let asset = manifest.package(rid).unwrap().asset.clone();
    manifest.artifact_sha256.insert(asset, digest(&bytes));
    bytes
}

fn write_folder(folder: &Path, files: &BTreeMap<String, Vec<u8>>, split: bool) {
    fs::create_dir_all(folder).unwrap();
    if split {
        fs::create_dir_all(folder.join("next")).unwrap();
    }
    for (name, bytes) in files {
        let parent = if split && (name == "LICENSE" || name == "origin.json") {
            folder.join("next")
        } else {
            folder.to_owned()
        };
        fs::write(parent.join(name), bytes).unwrap();
    }
}

#[test]
fn managed_next_manifest_maps_six_exact_packages_and_retains_patch_identity() {
    let manifest = Manifest::accepted().unwrap();
    assert_eq!(manifest.runtime_packages.len(), 6);
    for rid in [
        "linux-x64",
        "linux-arm64",
        "linux-arm",
        "osx-x64",
        "osx-arm64",
        "win-x64",
    ] {
        let package = manifest.package(rid).unwrap();
        assert_eq!(manifest.artifact_sha256[&package.asset].len(), 64);
        assert_eq!(
            package.payload_sha256["LICENSE"],
            manifest.artifact_sha256["LICENSE"]
        );
        assert!(!package.target.is_empty());
    }
    let value: serde_json::Value = serde_json::from_str(MANIFEST).unwrap();
    for patch in value["patches"].as_array().unwrap() {
        assert!(patch["retire_when"]
            .as_str()
            .unwrap()
            .contains("Remove this patch, not the tests"));
    }
}

#[test]
fn managed_next_archive_guards_reject_malformed_members_even_with_matching_archive_hash() {
    let (manifest, files) = synthetic("linux-x64");
    let entries: Vec<_> = files
        .iter()
        .map(|(name, bytes)| (name.clone(), bytes.clone(), b'0'))
        .collect();
    for variant in 0..5 {
        let mut manifest = manifest.clone();
        let mut changed = entries.clone();
        match variant {
            0 => changed.push(changed[0].clone()),
            1 => changed[0].0 = "../outside".to_owned(),
            2 => changed[0].2 = b'2',
            3 => changed[0].1.push(b'x'),
            _ => changed.retain(|(name, _, _)| name != "origin.json"),
        }
        let archive = tar_bytes(&mut manifest, "linux-x64", &changed);
        assert!(matches!(
            archive_files(&manifest, "linux-x64", &archive),
            Err(NextRuntimeError::Identity)
        ));
    }
}

#[test]
fn managed_next_windows_companions_and_origin_are_verified_without_running_executables() {
    let (mut manifest, files) = synthetic("win-x64");
    let archive = good_archive(&mut manifest, "win-x64", &files);
    assert!(archive_files(&manifest, "win-x64", &archive).is_ok());
    for name in ["libvpl.dll", "libvpl-LICENSE.txt", "LICENSE"] {
        let mut missing = files.clone();
        missing.remove(name);
        let archive = good_archive(&mut manifest, "win-x64", &missing);
        assert!(matches!(
            archive_files(&manifest, "win-x64", &archive),
            Err(NextRuntimeError::Identity)
        ));
    }
    for key in [
        "source_revision",
        "source_archive_sha256",
        "cargo_lock_sha256",
        "rid",
        "target",
        "distribution_version",
    ] {
        let mut changed = files.clone();
        let mut origin: serde_json::Value =
            serde_json::from_slice(&changed["origin.json"]).unwrap();
        origin[key] = serde_json::json!("wrong");
        changed.insert(
            "origin.json".to_owned(),
            serde_json::to_vec(&origin).unwrap(),
        );
        assert_eq!(
            verify_files(&manifest, "win-x64", &changed),
            Err(NextRuntimeError::Identity)
        );
    }
}

#[tokio::test]
async fn managed_next_offline_bundle_cache_and_strict_override_resolution() {
    let root = tempfile::tempdir().unwrap();
    let rid = ersatzrs_ffmpeg::runtime::current_ersatzrs_ffmpeg_rid().unwrap();
    let (manifest, files) = synthetic(rid);
    let mut cfg = config(root.path());
    cfg.allow_download = false;
    write_folder(cfg.host_executable.parent().unwrap(), &files, true);
    let bundled = acquire(cfg.clone(), manifest.clone(), "http://127.0.0.1:1")
        .await
        .unwrap();
    assert_eq!(bundled.source, NextRuntimeSource::Bundled);
    cfg.override_folder = Some(root.path().join("absent-override"));
    assert!(matches!(
        acquire(cfg.clone(), manifest.clone(), "http://127.0.0.1:1").await,
        Err(NextRuntimeError::Override)
    ));
    cfg.override_folder = None;
    fs::remove_dir_all(cfg.host_executable.parent().unwrap()).unwrap();
    let cache = cfg.managed_root.join(&manifest.distribution_tag).join(rid);
    write_folder(&cache, &files, false);
    let managed = acquire(cfg.clone(), manifest.clone(), "http://127.0.0.1:1")
        .await
        .unwrap();
    assert_eq!(managed.source, NextRuntimeSource::Managed);
    fs::write(managed.binary, b"corrupt").unwrap();
    assert!(matches!(
        acquire(cfg, manifest, "http://127.0.0.1:1").await,
        Err(NextRuntimeError::Offline)
    ));
}

#[tokio::test]
async fn managed_next_cold_http_install_is_serialized_and_exact_cache_stays_offline() {
    let root = tempfile::tempdir().unwrap();
    let rid = ersatzrs_ffmpeg::runtime::current_ersatzrs_ffmpeg_rid().unwrap();
    let (mut manifest, files) = synthetic(rid);
    let archive = good_archive(&mut manifest, rid, &files);
    let server = httpmock::MockServer::start_async().await;
    let asset = manifest.package(rid).unwrap().asset.clone();
    let request = server
        .mock_async(|when, then| {
            when.method(httpmock::Method::GET).path(format!("/{asset}"));
            then.status(200)
                .body(archive.clone())
                .delay(Duration::from_millis(75));
        })
        .await;
    let cfg = config(root.path());
    let base = server.base_url();
    let (first, second) = tokio::join!(
        acquire(cfg.clone(), manifest.clone(), &base),
        acquire(cfg.clone(), manifest.clone(), &base)
    );
    let first = first.unwrap();
    let second = second.unwrap();
    assert!(matches!(
        (first.source, second.source),
        (NextRuntimeSource::Downloaded, NextRuntimeSource::Managed)
            | (NextRuntimeSource::Managed, NextRuntimeSource::Downloaded)
    ));
    assert_eq!(first.binary, second.binary);
    request.assert_calls_async(1).await;
    validate_folder(&manifest, rid, first.binary.parent().unwrap(), false).unwrap();
    let mut cfg = cfg;
    cfg.allow_download = false;
    assert_eq!(
        acquire(cfg, manifest, "http://127.0.0.1:1")
            .await
            .unwrap()
            .source,
        NextRuntimeSource::Managed
    );
}

#[tokio::test]
async fn managed_next_failed_or_cancelled_http_never_publishes_partial_install_or_holds_lock() {
    let root = tempfile::tempdir().unwrap();
    let rid = ersatzrs_ffmpeg::runtime::current_ersatzrs_ffmpeg_rid().unwrap();
    let (manifest, _) = synthetic(rid);
    let cfg = config(root.path());
    let server = httpmock::MockServer::start_async().await;
    let asset = manifest.package(rid).unwrap().asset.clone();
    let request = server
        .mock_async(|when, then| {
            when.method(httpmock::Method::GET).path(format!("/{asset}"));
            then.status(200).body("checksum mismatch");
        })
        .await;
    assert!(matches!(
        acquire(cfg.clone(), manifest.clone(), &server.base_url()).await,
        Err(NextRuntimeError::Identity)
    ));
    request.assert_calls_async(1).await;
    assert!(!cfg
        .managed_root
        .join(&manifest.distribution_tag)
        .join(rid)
        .exists());
    assert!(try_install_lock(&cfg.managed_root).unwrap().is_some());
    request.delete_async().await;
    let delayed = server
        .mock_async(|when, then| {
            when.method(httpmock::Method::GET).path(format!("/{asset}"));
            then.status(200)
                .body("not completed")
                .delay(Duration::from_secs(10));
        })
        .await;
    let config_task = cfg.clone();
    let base = server.base_url();
    let task = tokio::spawn(async move { acquire(config_task, manifest, &base).await });
    tokio::time::timeout(Duration::from_secs(3), async {
        while delayed.calls_async().await == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    task.abort();
    match task.await {
        Err(error) => assert!(error.is_cancelled()),
        Ok(_) => panic!("cancelled acquisition unexpectedly completed"),
    }
    assert!(try_install_lock(&cfg.managed_root).unwrap().is_some());
}

#[test]
fn managed_next_bad_repair_preserves_existing_accepted_copy_and_receipt() {
    let root = tempfile::tempdir().unwrap();
    let rid = ersatzrs_ffmpeg::runtime::current_ersatzrs_ffmpeg_rid().unwrap();
    let (manifest, files) = synthetic(rid);
    let destination = root.path().join(&manifest.distribution_tag).join(rid);
    write_folder(&destination, &files, false);
    assert!(matches!(
        install(root.path(), &manifest, rid, b"not the pinned archive"),
        Err(NextRuntimeError::Identity)
    ));
    validate_folder(&manifest, rid, &destination, false).unwrap();
    for (name, expected) in files {
        assert_eq!(fs::read(destination.join(name)).unwrap(), expected);
    }
}

#[test]
fn managed_next_zip_symlink_rejected_even_when_target_bytes_match_payload_pin() {
    let rid = "win-x64";
    let (mut manifest, mut files) = synthetic(rid);
    let target = b"../outside".to_vec();
    files.insert("libvpl.dll".to_owned(), target.clone());
    let package = manifest.runtime_packages.get_mut(rid).unwrap();
    package
        .payload_sha256
        .insert("libvpl.dll".to_owned(), digest(&target));
    package.payload_bytes.insert(
        "libvpl.dll".to_owned(),
        u64::try_from(target.len()).unwrap(),
    );
    let mut origin: serde_json::Value = serde_json::from_slice(&files["origin.json"]).unwrap();
    origin["payload_sha256"] = serde_json::to_value(&package.payload_sha256).unwrap();
    files.insert(
        "origin.json".to_owned(),
        serde_json::to_vec(&origin).unwrap(),
    );
    verify_files(&manifest, rid, &files).unwrap();
    let root = manifest
        .package(rid)
        .unwrap()
        .asset
        .trim_end_matches(".zip");
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in files {
        if name == "libvpl.dll" {
            zip.add_symlink(
                format!("{root}/{name}"),
                "../outside",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        } else {
            zip.start_file(
                format!("{root}/{name}"),
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
            zip.write_all(&bytes).unwrap();
        }
    }
    let archive = zip.finish().unwrap().into_inner();
    let asset = manifest.package(rid).unwrap().asset.clone();
    manifest.artifact_sha256.insert(asset, digest(&archive));
    assert!(matches!(
        archive_files(&manifest, rid, &archive),
        Err(NextRuntimeError::Identity)
    ));
}

#[tokio::test]
async fn managed_next_http_declared_size_limit_fails_without_installing() {
    let server = httpmock::MockServer::start_async().await;
    let request = server
        .mock_async(|when, then| {
            when.path("/oversized");
            then.status(200).body("123456789");
        })
        .await;
    assert_eq!(
        download_with_limit(&format!("{}/oversized", server.base_url()), 8).await,
        Err(NextRuntimeError::Download)
    );
    request.assert_calls_async(1).await;
}

#[tokio::test]
async fn managed_next_automatic_identity_rejection_precedes_every_version_probe() {
    let root = tempfile::tempdir().unwrap();
    let rid = ersatzrs_ffmpeg::runtime::current_ersatzrs_ffmpeg_rid().unwrap();
    let (manifest, files) = synthetic(rid);
    for (source, split) in [
        (NextRuntimeSource::Bundled, true),
        (NextRuntimeSource::Managed, false),
    ] {
        let folder = root.path().join(format!("{source:?}"));
        write_folder(&folder, &files, split);
        let accepted = automatic_result(&manifest, rid, folder, split, source);
        let probed = std::cell::Cell::new(false);
        let valid = validate_automatic(&accepted, manifest.clone(), async {
            probed.set(true);
            Some(accepted.version.clone())
        })
        .await;
        assert_eq!(valid, Ok(()));
        assert!(
            probed.get(),
            "a trusted package must reach its version probe"
        );
        // Same-sized replacement avoids relying only on the metadata-size guard.
        let binary = fs::read(&accepted.binary).unwrap();
        let mut replaced = binary;
        replaced[0] ^= 1;
        fs::write(&accepted.binary, replaced).unwrap();
        probed.set(false);
        let invalid = validate_automatic(&accepted, manifest.clone(), async {
            probed.set(true);
            Some(accepted.version.clone())
        })
        .await;
        assert_eq!(invalid, Err(NextRuntimeError::Identity));
        assert!(
            !probed.get(),
            "an automatic identity mismatch must reject before attempting a launch"
        );
    }
}
