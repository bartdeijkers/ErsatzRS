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
    synthetic_manifest(Manifest::accepted().unwrap(), rid)
}

fn synthetic_manifest(mut manifest: Manifest, rid: &str) -> (Manifest, BTreeMap<String, Vec<u8>>) {
    let files: BTreeMap<_, _> = manifest
        .payload_names(rid)
        .unwrap()
        .into_iter()
        .map(|name| {
            let bytes = format!("bounded synthetic {name}\n").into_bytes();
            (name, bytes)
        })
        .collect();
    if rid == "linux-arm64" {
        for (name, hash) in &mut manifest.linux_arm64_companions_sha256 {
            *hash = digest(&files[name]);
        }
    }
    let identity = manifest.runtime_packages.get(rid).unwrap();
    let (revision, archive, lock) = identity.source_identity(&manifest);
    let (revision, archive, prepared_lock) =
        (revision.to_owned(), archive.to_owned(), lock.to_owned());
    let version = identity.version(&manifest).to_owned();
    let patches = identity.patches(&manifest).to_vec();
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
        "source_revision": revision,
        "source_archive_sha256": archive,
        "cargo_lock_sha256": prepared_lock,
        "distribution_version": version,
        "rid": rid, "target": package.target,
        "patches": patches.iter().map(|p| serde_json::json!({"file":p.file,"sha256":p.sha256})).collect::<Vec<_>>(),
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
        let retirement = patch["retire_when"].as_str().unwrap();
        assert!(retirement.contains("upstream") && retirement.contains("pass"));
        assert!([
            "Remove this patch, not the tests",
            "Preserve the tests",
            "Retain the tests",
            "Preserve the regression",
        ]
        .iter()
        .any(|condition| retirement.contains(condition)));
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

#[tokio::test]
async fn managed_next_deferred_platform_retains_exact_download_cache_origin_and_health_identity() {
    for rid in ["osx-x64", "osx-arm64"] {
        let mut manifest = Manifest::accepted().unwrap();
        let package = manifest.runtime_packages.get_mut(rid).unwrap();
        package.distribution_version = Some("0.2.3-retained".to_owned());
        package.distribution_tag = Some("next-retained".to_owned());
        package.patches = Some(manifest.patches[..1].to_vec());
        package.asset = format!("ersatzrs-next-0.2.3-retained-{rid}.tar.gz");
        manifest
            .artifact_sha256
            .insert(package.asset.clone(), "0".repeat(64));
        let (mut manifest, files) = synthetic_manifest(manifest, rid);
        assert!(manifest
            .download_base(rid)
            .unwrap()
            .ends_with("/next-retained"));
        assert!(manifest
            .download_base("linux-x64")
            .unwrap()
            .ends_with(&manifest.distribution_tag));
        let archive = good_archive(&mut manifest, rid, &files);
        let root = tempfile::tempdir().unwrap();
        let cfg = config(root.path());
        let accepted = install(&cfg.managed_root, &manifest, rid, &archive).unwrap();
        assert_eq!(accepted.version, "0.2.3-retained");
        assert_eq!(
            accepted.folder,
            cfg.managed_root.join("next-retained").join(rid)
        );
        let cached = local_automatic(&cfg, &manifest, rid).unwrap();
        assert_eq!(cached.source, NextRuntimeSource::Managed);
        assert_eq!(cached.version, accepted.version);
        validate_automatic(&cached, manifest.clone(), async {
            Some("0.2.3-retained".to_owned())
        })
        .await
        .unwrap();
        assert_eq!(
            validate_automatic(&cached, manifest.clone(), async {
                Some(manifest.distribution_version.clone())
            })
            .await,
            Err(NextRuntimeError::Execution)
        );

        for field in ["distribution_version", "patches"] {
            let mut changed = files.clone();
            let mut origin: serde_json::Value =
                serde_json::from_slice(&changed["origin.json"]).unwrap();
            origin[field] = if field == "distribution_version" {
                serde_json::json!(manifest.distribution_version)
            } else {
                serde_json::json!(manifest
                    .patches
                    .iter()
                    .map(|p| serde_json::json!({"file":p.file,"sha256":p.sha256}))
                    .collect::<Vec<_>>())
            };
            changed.insert(
                "origin.json".to_owned(),
                serde_json::to_vec(&origin).unwrap(),
            );
            assert_eq!(
                verify_files(&manifest, rid, &changed),
                Err(NextRuntimeError::Identity)
            );
        }
        let mut incomplete = manifest.clone();
        incomplete.runtime_packages.get_mut(rid).unwrap().patches = None;
        assert!(matches!(
            incomplete.package(rid),
            Err(NextRuntimeError::Manifest)
        ));
        for invalid in ["../outside", ".", ".."] {
            let mut escaping = manifest.clone();
            escaping
                .runtime_packages
                .get_mut(rid)
                .unwrap()
                .distribution_tag = Some(invalid.to_owned());
            assert!(matches!(
                escaping.package(rid),
                Err(NextRuntimeError::Manifest)
            ));
        }
        let mut relabelled = manifest.clone();
        relabelled.runtime_packages.get_mut(rid).unwrap().asset = format!(
            "ersatzrs-next-{}-{rid}.tar.gz",
            manifest.distribution_version
        );
        assert!(matches!(
            relabelled.package(rid),
            Err(NextRuntimeError::Manifest)
        ));
    }
}

#[test]
fn retained_source_identity_rejects_a_mixture_with_the_new_global_source() {
    let rid = "linux-x64";
    let mut manifest = Manifest::accepted().unwrap();
    let package = manifest.runtime_packages.get_mut(rid).unwrap();
    package.distribution_version = Some("0.2.3-retained".to_owned());
    package.distribution_tag = Some("next-retained".to_owned());
    package.patches = Some(manifest.patches[..1].to_vec());
    package.asset = format!("ersatzrs-next-0.2.3-retained-{rid}.tar.gz");
    package.source_identity = Some(SourceIdentity {
        upstream_revision: "a".repeat(40),
        source_archive_sha256: "b".repeat(64),
        prepared_lock_sha256: "c".repeat(64),
    });
    manifest
        .artifact_sha256
        .insert(package.asset.clone(), "0".repeat(64));
    let (manifest, files) = synthetic_manifest(manifest, rid);
    verify_files(&manifest, rid, &files).unwrap();
    for (key, replacement) in [
        ("source_revision", manifest.upstream_revision.as_str()),
        (
            "source_archive_sha256",
            manifest.source_archive_sha256.as_str(),
        ),
        ("cargo_lock_sha256", manifest.prepared_lock_sha256()),
    ] {
        let mut changed = files.clone();
        let mut origin: serde_json::Value =
            serde_json::from_slice(&changed["origin.json"]).unwrap();
        origin[key] = replacement.into();
        changed.insert(
            "origin.json".to_owned(),
            serde_json::to_vec(&origin).unwrap(),
        );
        assert!(
            matches!(
                verify_files(&manifest, rid, &changed),
                Err(NextRuntimeError::Identity)
            ),
            "{key}"
        );
    }
}

/// Exercise reviewed real archives without executing another target's binary.
#[test]
#[ignore = "requires NEXT_CANDIDATE_PACKAGES with reviewed manifest.json and its archives"]
#[allow(
    clippy::disallowed_methods,
    reason = "manual package acceptance supplies its reviewed fixture directory explicitly"
)]
fn managed_next_candidate_packages_install_and_relocate_with_exact_identity() {
    let directory = PathBuf::from(std::env::var_os("NEXT_CANDIDATE_PACKAGES").unwrap());
    let manifest: Manifest =
        serde_json::from_slice(&fs::read(directory.join("manifest.json")).unwrap()).unwrap();
    assert!(!manifest.runtime_packages.is_empty());
    let root = tempfile::tempdir().unwrap();
    for rid in manifest.runtime_packages.keys() {
        let package = manifest.package(rid).unwrap();
        let bytes = fs::read(directory.join(&package.asset)).unwrap();
        let installed = install(root.path(), &manifest, rid, &bytes).unwrap();
        validate_folder(&manifest, rid, &installed.folder, false).unwrap();
        let relocated = root.path().join(format!("relocated-{rid}"));
        fs::rename(&installed.folder, &relocated).unwrap();
        validate_folder(&manifest, rid, &relocated, false).unwrap();
        let files = archive_files(&manifest, rid, &bytes).unwrap();
        let bundled = root.path().join(format!("bundle-{rid}"));
        write_folder(&bundled, &files, true);
        validate_folder(&manifest, rid, &bundled, true).unwrap();
        eprintln!("verified real candidate archive, managed relocation and bundle: {rid}");
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

#[test]
fn managed_next_arm64_private_companions_require_complete_pinned_payloads() {
    let rid = "linux-arm64";
    let mut manifest = Manifest::accepted().unwrap();
    manifest
        .patches
        .retain(|patch| patch.file != "0004-private-mpp-driver-readiness.patch");
    manifest.patches.push(Patch {
        file: "0004-private-mpp-driver-readiness.patch".to_owned(),
        sha256: "1".repeat(64),
    });
    manifest.linux_arm64_companions_sha256.clear();
    assert!(matches!(
        manifest.package(rid),
        Err(NextRuntimeError::Manifest)
    ));
    manifest.linux_arm64_companions_sha256 = ARM64_COMPANIONS
        .into_iter()
        .map(|name| (name.to_owned(), String::new()))
        .collect();
    let (mut manifest, files) = synthetic_manifest(manifest, rid);
    let archive = good_archive(&mut manifest, rid, &files);
    assert!(archive_files(&manifest, rid, &archive).is_ok());
    let root = tempfile::tempdir().unwrap();
    write_folder(root.path(), &files, true);
    validate_folder(&manifest, rid, root.path(), true).unwrap();

    for name in ARM64_COMPANIONS {
        for missing in [false, true] {
            let mut changed = files.clone();
            if missing {
                changed.remove(name);
            } else {
                changed.get_mut(name).unwrap()[0] ^= 1;
            }
            // Re-pin the archive, so rejection must inspect its actual members.
            let archive = good_archive(&mut manifest, rid, &changed);
            assert_eq!(
                archive_files(&manifest, rid, &archive).err(),
                Some(NextRuntimeError::Identity),
                "{name}: missing={missing}"
            );
        }
    }
    let mut incomplete = manifest.clone();
    incomplete
        .linux_arm64_companions_sha256
        .remove(ARM64_COMPANIONS[0]);
    assert!(matches!(
        incomplete.package(rid),
        Err(NextRuntimeError::Manifest)
    ));
    let mut extra = manifest.clone();
    extra
        .linux_arm64_companions_sha256
        .insert("unreviewed.so".to_owned(), "0".repeat(64));
    assert!(matches!(
        extra.package(rid),
        Err(NextRuntimeError::Manifest)
    ));
    manifest
        .linux_arm64_companions_sha256
        .insert(ARM64_COMPANIONS[0].to_owned(), "0".repeat(64));
    assert!(matches!(
        manifest.package(rid),
        Err(NextRuntimeError::Manifest)
    ));
}

#[test]
fn managed_next_prepared_lock_pin_rejects_an_unpatched_origin() {
    for rid in [
        "linux-x64",
        "linux-arm64",
        "linux-arm",
        "osx-x64",
        "osx-arm64",
        "win-x64",
    ] {
        let mut manifest = Manifest::accepted().unwrap();
        manifest.patched_lock_sha256 = Some("1".repeat(64));
        let (manifest, mut files) = synthetic_manifest(manifest, rid);
        verify_files(&manifest, rid, &files).unwrap();
        let mut origin: serde_json::Value = serde_json::from_slice(&files["origin.json"]).unwrap();
        origin["cargo_lock_sha256"] = serde_json::json!(manifest.source_lock_sha256);
        files.insert(
            "origin.json".to_owned(),
            serde_json::to_vec(&origin).unwrap(),
        );
        assert_eq!(
            verify_files(&manifest, rid, &files),
            Err(NextRuntimeError::Identity)
        );
        let mut legacy = Manifest::accepted().unwrap();
        legacy.patched_lock_sha256 = None;
        let (legacy, files) = synthetic_manifest(legacy, rid);
        assert!(legacy.patched_lock_sha256.is_none());
        verify_files(&legacy, rid, &files).unwrap();
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
    let cache = cfg
        .managed_root
        .join(manifest.package(rid).unwrap().tag(&manifest))
        .join(rid);
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
        .join(manifest.package(rid).unwrap().tag(&manifest))
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
    let destination = root
        .path()
        .join(manifest.package(rid).unwrap().tag(&manifest))
        .join(rid);
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
        let accepted = automatic_result(&manifest, rid, folder, split, source).unwrap();
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

// The supplied manifest is a test-local qualification pin, never the compiled
// production selection. Exercise real archives without publishing global state.
#[allow(clippy::disallowed_methods)] // Explicit opt-in candidate package paths, never production configuration.
#[tokio::test]
#[ignore = "requires ERSATZRS_NEXT_TEST_PACKAGE_MANIFEST and ERSATZRS_NEXT_TEST_PACKAGE_ARCHIVE for the native RID"]
async fn real_next_candidate_package_acquisition_and_health_validation() {
    let path = |key| PathBuf::from(std::env::var_os(key).expect("set both package test variables"));
    let manifest: Manifest = serde_json::from_slice(
        &bounded_file(&path("ERSATZRS_NEXT_TEST_PACKAGE_MANIFEST"), MAX_ORIGIN).unwrap(),
    )
    .unwrap();
    let archive = bounded_file(&path("ERSATZRS_NEXT_TEST_PACKAGE_ARCHIVE"), MAX_ARCHIVE).unwrap();
    let rid = ersatzrs_ffmpeg::runtime::current_ersatzrs_ffmpeg_rid().unwrap();
    let asset = manifest.package(rid).unwrap().asset.clone();
    let root = tempfile::tempdir().unwrap();
    let cfg = config(root.path());
    let server = httpmock::MockServer::start_async().await;
    let request = server
        .mock_async(|when, then| {
            when.method(httpmock::Method::GET).path(format!("/{asset}"));
            then.status(200).body(archive.clone());
        })
        .await;
    let installed = acquire(cfg.clone(), manifest.clone(), &server.base_url())
        .await
        .unwrap();
    assert_eq!(installed.source, NextRuntimeSource::Downloaded);
    assert!(installed.published_pin_verified);
    // Same identity-before-execution function used by validate_published/Health.
    validate_automatic(
        &installed,
        manifest.clone(),
        super::super::version::probe_binary(&installed.binary),
    )
    .await
    .unwrap();
    request.assert_calls_async(1).await;

    let mut offline = cfg;
    offline.allow_download = false;
    let cached = acquire(offline.clone(), manifest.clone(), "http://127.0.0.1:1")
        .await
        .unwrap();
    assert_eq!(cached.source, NextRuntimeSource::Managed);
    assert_eq!(cached.binary, installed.binary);
    assert_eq!(
        cached.version,
        manifest.package(rid).unwrap().version(&manifest)
    );
    validate_automatic(
        &cached,
        manifest.clone(),
        super::super::version::probe_binary(&cached.binary),
    )
    .await
    .unwrap();
    request.assert_calls_async(1).await;
    assert!(try_install_lock(&offline.managed_root).unwrap().is_some());

    // A same-length replacement must fail before launching untrusted bytes.
    let mut replaced = fs::read(&cached.binary).unwrap();
    replaced[0] ^= 1;
    fs::write(&cached.binary, replaced).unwrap();
    assert_eq!(
        validate_automatic(&cached, manifest.clone(), async {
            panic!("an identity mismatch must never poll the executable probe")
        })
        .await,
        Err(NextRuntimeError::Identity)
    );
    assert!(matches!(
        acquire(offline.clone(), manifest, "http://127.0.0.1:1").await,
        Err(NextRuntimeError::Offline)
    ));
    assert!(try_install_lock(&offline.managed_root).unwrap().is_some());
}

#[tokio::test]
async fn managed_next_indexed_health_requires_a_registered_unchanged_reader() {
    let root = tempfile::tempdir().unwrap();
    let rid = ersatzrs_ffmpeg::runtime::current_ersatzrs_ffmpeg_rid().unwrap();
    let (manifest, files) = synthetic(rid);
    let mut cfg = config(root.path());
    cfg.allow_download = false;
    let folder = cfg
        .managed_root
        .join(manifest.package(rid).unwrap().tag(&manifest))
        .join(rid);
    write_folder(&folder, &files, false);
    let mut accepted = acquire(cfg, manifest, "http://127.0.0.1:1").await.unwrap();
    assert!(accepted.indexed_reader.is_none());
    assert_eq!(validate_indexed(&accepted).await, Ok(()));
    let expected = accepted.clone();
    let mut replaced = accepted.clone();
    replaced.version.push_str("-replacement");
    assert_eq!(
        register_indexed(
            &mut replaced,
            &expected,
            super::super::indexed_runtime::IndexedReaderPermit::fixture()
        ),
        Err(NextRuntimeError::Contract)
    );
    assert!(replaced.indexed_reader.is_none());
    register_indexed(
        &mut accepted,
        &expected,
        super::super::indexed_runtime::IndexedReaderPermit::fixture(),
    )
    .unwrap();
    assert_eq!(
        validate_indexed(&accepted).await,
        Err(NextRuntimeError::Identity)
    );
}

// Shell fixture verifies successful caching without changing global startup
// state or requiring a published indexed runtime; native qualification is open.
#[cfg(unix)]
#[tokio::test]
async fn managed_next_indexed_cache_rechecks_identity_and_does_not_cache_failed_probes() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let binary = root.path().join("next");
    let marker = root.path().join("described");
    let description = crate::next_engine::indexed_runtime::tests::description();
    let body=format!("#!/bin/sh\nif [ \"$1\" = --version ]; then printf 'ersatztv-channel synthetic'; else printf 'call\\n' >> '{}'; printf '%s' '{}'; fi\n",marker.display(),description);
    std::fs::write(&binary, body).unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut selected = NextRuntimeAcquisition {
        binary: binary.clone(),
        source: NextRuntimeSource::Override,
        rid: ersatzrs_ffmpeg::runtime::current_ersatzrs_ffmpeg_rid()
            .unwrap()
            .into(),
        version: "synthetic".into(),
        published_pin_verified: false,
        folder: root.path().into(),
        split_bundle: false,
        indexed_reader: None,
    };
    let reader = qualify_selected_indexed(&selected).await.unwrap();
    let expected = selected.clone();
    register_indexed(&mut selected, &expected, reader).unwrap();
    assert!(qualify_selected_indexed(&selected).await.is_ok());
    assert_eq!(std::fs::read_to_string(&marker).unwrap().lines().count(), 1);
    assert_eq!(validate_selected(&selected).await, Ok(()));
    let changed_marker = root.path().join("changed-must-not-run");
    std::fs::write(&binary,format!("#!/bin/sh\ntouch '{}'\nif [ \"$1\" = --version ]; then printf 'ersatztv-channel synthetic'; else printf '{{}}'; fi\n",changed_marker.display())).unwrap();
    assert_eq!(
        qualify_selected_indexed(&selected).await.unwrap_err(),
        NextRuntimeError::Identity
    );
    assert_eq!(
        validate_selected(&selected).await,
        Err(NextRuntimeError::Identity)
    );
    assert!(!changed_marker.exists());
    selected.indexed_reader = None;
    assert_eq!(
        qualify_selected_indexed(&selected).await.unwrap_err(),
        NextRuntimeError::Contract
    );
    assert!(selected.indexed_reader.is_none());
    // Process failure is not an unsupported legacy contract: startup must fail.
    std::fs::write(&binary, "#!/bin/sh\nif [ \"$1\" = --version ]; then printf 'ersatztv-channel synthetic'; else exit 7; fi\n").unwrap();
    assert_eq!(
        qualify_selected_indexed(&selected).await.unwrap_err(),
        NextRuntimeError::Execution
    );
    // A changed executable during describe cannot select the ordinary fallback.
    std::fs::write(&binary, format!("#!/bin/sh\nif [ \"$1\" = --version ]; then printf 'ersatztv-channel synthetic'; else printf '# changed\\n' >> \"$0\"; printf '%s' '{description}'; fi\n")).unwrap();
    assert_eq!(
        qualify_selected_indexed(&selected).await.unwrap_err(),
        NextRuntimeError::Identity
    );
}
