use std::sync::{atomic::AtomicUsize, Condvar, Mutex};

use super::*;

fn request_bytes() -> Vec<u8> {
    br#"{"schema_version":1,"generation":"fixture","locale":"nl-NL","timezone":"Europe/Amsterdam","captured_at":"2026-10-25T00:30:00Z","window":{"start":"2026-10-24T22:00:00Z","finish":"2026-10-27T23:00:00Z"},"channels":[]}"#.to_vec()
}

struct Blocker {
    stage: &'static str,
    occurrence: usize,
    count: AtomicUsize,
    entered: AtomicBool,
    finished: AtomicBool,
    ready: tokio::sync::Notify,
    released: Mutex<bool>,
    wake: Condvar,
    stages: Mutex<Vec<&'static str>>,
}

impl Blocker {
    fn new(stage: &'static str, occurrence: usize) -> Arc<Self> {
        Arc::new(Self {
            stage,
            occurrence,
            count: AtomicUsize::new(0),
            entered: AtomicBool::new(false),
            finished: AtomicBool::new(false),
            ready: tokio::sync::Notify::new(),
            released: Mutex::new(false),
            wake: Condvar::new(),
            stages: Mutex::new(Vec::new()),
        })
    }

    fn hook(self: &Arc<Self>) -> Arc<dyn Fn(&'static str) + Send + Sync> {
        let blocker = Arc::clone(self);
        Arc::new(move |stage| {
            blocker.stages.lock().unwrap().push(stage);
            if stage == blocker.stage
                && blocker.count.fetch_add(1, Ordering::AcqRel) + 1 == blocker.occurrence
            {
                blocker.entered.store(true, Ordering::Release);
                blocker.ready.notify_one();
                let mut released = blocker.released.lock().unwrap();
                while !*released {
                    released = blocker.wake.wait(released).unwrap();
                }
                blocker.finished.store(true, Ordering::Release);
            }
        })
    }

    #[cfg(unix)]
    async fn wait_entered(&self) {
        tokio::time::timeout(Duration::from_secs(3), async {
            while !self.entered.load(Ordering::Acquire) {
                self.ready.notified().await;
            }
        })
        .await
        .expect("filesystem job entered its controlled native-call stand-in");
    }

    fn release(&self) {
        *self.released.lock().unwrap() = true;
        self.wake.notify_all();
    }
}

// Always release a deliberately blocked job if an assertion fails, so the
// Tokio runtime can settle its blocking pool instead of hanging test teardown.
struct ReleaseOnDrop(Arc<Blocker>);
impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        self.0.release();
    }
}

#[cfg(unix)]
async fn wait_true(flag: &AtomicBool) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while !flag.load(Ordering::Acquire) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("owned worker settled");
}

#[test]
fn fence_and_static_error_mapping_preserve_cleanup_uncertainty() {
    let fence = ResolutionFence::new(Instant::now() + Duration::from_secs(30));
    assert!(fence.check().is_ok());
    fence.stop();
    assert!(fence.check().is_err());
    assert!(ResolutionFence::new(Instant::now()).check().is_err());
    assert_eq!(
        admission_error(&AddonError::ToolCleanupRequired),
        TeletextInvocationError::CleanupUnconfirmed
    );
    assert_eq!(
        command_error(TeletextCommandError::CleanupUnconfirmed),
        TeletextInvocationError::CleanupUnconfirmed
    );
    let error = admission_error(&AddonError::Validation("private package path".to_owned()));
    assert_eq!(error, TeletextInvocationError::AdmissionRejected);
    assert!(!error.to_string().contains("private package path"));
}

#[tokio::test]
async fn request_preflight_precedes_even_an_unavailable_database_or_filesystem() {
    let pool = DbPool::connect_lazy("sqlite::memory:").unwrap();
    let blocker = Blocker::new("canonicalize", 1);
    let _release = ReleaseOnDrop(Arc::clone(&blocker));
    for input in [
        b"{}".to_vec(),
        vec![0xff],
        vec![b' '; MAX_TELETEXT_PAGES_INPUT_BYTES + 1],
    ] {
        let mut fence = ResolutionFence::new(Instant::now() + Duration::from_secs(30));
        fence.hook = Some(blocker.hook());
        let invocation = Invocation {
            pool: pool.clone(),
            addon_id: "org.example.teletext-fixture".to_owned(),
            runtime: AddonRuntimeContext::default(),
            input,
            cancellation: None,
            selection: None,
        };
        assert_eq!(
            invoke_with_fence(invocation, fence, Duration::from_secs(1)).await,
            Err(TeletextInvocationError::InvalidRequest)
        );
    }
    assert!(!blocker.entered.load(Ordering::Acquire));
    assert!(blocker.stages.lock().unwrap().is_empty());
}

// Installed synthetic shell fixtures intentionally run on Unix only. The
// production resolver/owner has Windows counterparts; native Windows package
// and JobObject acceptance is a separate gate, not inferred from these fixtures.
#[cfg(unix)]
mod unix {
    use std::{collections::BTreeMap, path::PathBuf};

    use ersatzrs_addon_contract::{
        AddonCapability, AddonDependency, AddonEntrypoint, AddonEntrypointKind, AddonManifest,
        AddonPermissions, AddonSettingDefinition, AddonSettingKind, ADDON_MANIFEST_VERSION,
    };
    use ersatzrs_core::repositories::{
        ChannelRepository as _, ChannelTeletextBindingSave, FfmpegProfileRepository as _,
        TeletextProfileSave,
    };
    use ersatzrs_domain::{Channel, FFmpegProfile, TeletextProfileSetting, TeletextProfileValue};
    use semver::{Version, VersionReq};

    use super::*;

    struct Fixture {
        temp: tempfile::TempDir,
        pool: DbPool,
        package: PathBuf,
        marker: PathBuf,
        manifest: AddonManifest,
    }

    impl Fixture {
        async fn new(script: &str) -> Self {
            let temp = tempfile::tempdir().unwrap();
            let pool = crate::db::init(&format!(
                "sqlite:{}",
                temp.path().join("fixture.db").display()
            ))
            .await
            .unwrap();
            let package = temp.path().join("package");
            std::fs::create_dir(&package).unwrap();
            let manifest = AddonManifest {
                manifest_version: ADDON_MANIFEST_VERSION,
                id: "org.example.teletext-fixture".to_owned(),
                version: Version::new(1, 0, 0),
                name: BTreeMap::from([("en-US".to_owned(), "Engine fixture".to_owned())]),
                summary: BTreeMap::from([(
                    "en-US".to_owned(),
                    "Synthetic teletext fixture".to_owned(),
                )]),
                icon: None,
                license: "GPL-3.0-or-later".to_owned(),
                source_url: "https://example.test/source".to_owned(),
                host_version: VersionReq::STAR,
                capabilities: vec![
                    AddonCapability {
                        id: "addon.check.v1".to_owned(),
                        options: Vec::new(),
                    },
                    AddonCapability {
                        id: "teletext.pages.v1".to_owned(),
                        options: Vec::new(),
                    },
                ],
                dependencies: Vec::new(),
                entrypoints: BTreeMap::from([(
                    "any".to_owned(),
                    AddonEntrypoint {
                        path: "addon.sh".to_owned(),
                        kind: AddonEntrypointKind::PosixShell,
                    },
                )]),
                permissions: AddonPermissions::default(),
                settings: Vec::new(),
                media_list_storage: None,
                default_source_hosts: Vec::new(),
                panels: Vec::new(),
            };
            manifest.validate().unwrap();
            std::fs::write(
                package.join("addon.toml"),
                toml::to_string(&manifest).unwrap(),
            )
            .unwrap();
            std::fs::write(package.join("addon.sh"), format!("test \"$1\" = pages || exit 9\ntest \"$ERSATZRS_ADDON_CAPABILITY\" = teletext.pages.v1 || exit 8\nprintf started > ../started\n{script}\n")).unwrap();
            let digest = super::super::super::calculate_addon_content_sha256(&package)
                .await
                .unwrap();
            sqlx::query(r#"INSERT INTO "AddonInstallation" ("AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256", "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt", "StateRevision") VALUES (?, '1.0.0', 1, 'fixture-package', ?, ?, ?, 'now', 'now', 'fixture-revision')"#)
                .bind(&manifest.id).bind(digest).bind(serde_json::to_string(&manifest).unwrap()).bind(package.to_string_lossy().into_owned()).execute(&pool).await.unwrap();
            let marker = temp.path().join("started");
            Self {
                temp,
                pool,
                package,
                marker,
                manifest,
            }
        }

        fn invocation(&self, cancellation: Option<oneshot::Receiver<()>>) -> Invocation {
            Invocation {
                pool: self.pool.clone(),
                addon_id: self.manifest.id.clone(),
                runtime: AddonRuntimeContext::default(),
                input: request_bytes(),
                cancellation,
                selection: None,
            }
        }

        async fn store_manifest(&self, manifest: &AddonManifest) {
            sqlx::query(r#"UPDATE "AddonInstallation" SET "ManifestJson" = ? WHERE "AddonId" = ?"#)
                .bind(serde_json::to_string(manifest).unwrap())
                .bind(&manifest.id)
                .execute(&self.pool)
                .await
                .unwrap();
        }
    }

    impl Fixture {
        async fn configure_profile_settings(
            &self,
            with_tools: bool,
        ) -> [Vec<TeletextProfileSetting>; 2] {
            let mut manifest = self.manifest.clone();
            manifest.settings = vec![setting(AddonSettingKind::String, Some("global".to_owned()))];
            let mut inherited = setting(AddonSettingKind::String, Some("inherited".to_owned()));
            inherited.key = "OTHER".to_owned();
            inherited.required = false;
            manifest.settings.push(inherited);
            let mut settings = [
                vec![profile_literal("VALUE", "left")],
                vec![profile_literal("VALUE", "right")],
            ];
            if with_tools {
                let mut definition = setting(AddonSettingKind::Executable, None);
                definition.key = "TOOL".to_owned();
                manifest.settings.push(definition);
                for (index, settings) in settings.iter_mut().enumerate() {
                    let path = self.temp.path().join(format!("tool-{index}"));
                    std::fs::write(&path, b"synthetic tool").unwrap();
                    settings.push(profile_literal("TOOL", &path.to_string_lossy()));
                }
            }
            std::fs::write(
                self.package.join("addon.toml"),
                toml::to_string(&manifest).unwrap(),
            )
            .unwrap();
            self.store_manifest(&manifest).await;
            let digest = super::super::super::calculate_addon_content_sha256(&self.package)
                .await
                .unwrap();
            sqlx::query(
                r#"UPDATE "AddonInstallation" SET "ContentSha256" = ? WHERE "AddonId" = ?"#,
            )
            .bind(digest)
            .bind(&manifest.id)
            .execute(&self.pool)
            .await
            .unwrap();
            settings
        }

        async fn profile_selections(&self, with_tools: bool) -> [TeletextSelectionSnapshot; 2] {
            let settings = self.configure_profile_settings(with_tools).await;
            sqlx::query(r#"INSERT INTO "Resolution" ("Id", "Name", "Width", "Height") VALUES (1, 'fixture', 1920, 1080)"#).execute(&self.pool).await.unwrap();
            let repos = crate::repositories::make_repositories(self.pool.clone());
            repos
                .ffmpeg_profile
                .upsert(&FFmpegProfile {
                    id: 1,
                    name: "Fixture".to_owned(),
                    resolution_id: 1,
                    ..FFmpegProfile::default()
                })
                .await
                .unwrap();
            let repo = crate::repositories::SqliteTeletextProfileRepository(self.pool.clone());
            let installation = repo
                .installation_snapshot(&self.manifest.id)
                .await
                .unwrap()
                .snapshot
                .unwrap();
            let mut selections = Vec::new();
            for (index, settings) in settings.into_iter().enumerate() {
                let channel_id = i64::try_from(index + 1).unwrap();
                repos
                    .channel
                    .upsert(&Channel {
                        id: channel_id,
                        unique_id: uuid::Uuid::new_v4(),
                        number: channel_id.to_string(),
                        name: format!("Channel {channel_id}"),
                        ffmpeg_profile_id: 1,
                        ..Channel::default()
                    })
                    .await
                    .unwrap();
                let profile = saved(
                    repo.save_profile(&TeletextProfileSave {
                        addon_id: self.manifest.id.clone(),
                        name: format!("Profile {channel_id}"),
                        settings,
                        expected_installation: installation.clone(),
                        existing: None,
                    })
                    .await
                    .unwrap(),
                )
                .profile;
                let binding = saved(
                    repo.save_binding(&ChannelTeletextBindingSave {
                        channel_id,
                        addon_id: self.manifest.id.clone(),
                        profile_id: profile.id,
                        enabled: true,
                        expected_installation: installation.clone(),
                        expected_profile_revision: profile.revision,
                        expected_binding_revision: None,
                    })
                    .await
                    .unwrap(),
                );
                selections.push(saved(
                    repo.invocation_snapshot(channel_id, &self.manifest.id, binding.revision)
                        .await
                        .unwrap(),
                ));
            }
            selections.try_into().unwrap()
        }
    }

    fn profile_literal(key: &str, value: &str) -> TeletextProfileSetting {
        TeletextProfileSetting {
            key: key.to_owned(),
            value: TeletextProfileValue::Literal(value.to_owned()),
        }
    }

    fn saved<T: std::fmt::Debug>(mutation: TeletextProfileMutation<T>) -> T {
        match mutation {
            TeletextProfileMutation::Saved(value) => value,
            TeletextProfileMutation::Rejected(reason) => panic!("unexpected rejection {reason:?}"),
        }
    }

    fn setting(kind: AddonSettingKind, default: Option<String>) -> AddonSettingDefinition {
        AddonSettingDefinition {
            key: "VALUE".to_owned(),
            label: BTreeMap::from([("en-US".to_owned(), "Fixture value".to_owned())]),
            kind,
            required: true,
            default,
            options: Vec::new(),
            suggested_reference: None,
        }
    }

    fn fenced(blocker: &Arc<Blocker>, deadline: Instant) -> ResolutionFence {
        let mut fence = ResolutionFence::new(deadline);
        fence.hook = Some(blocker.hook());
        fence
    }

    #[tokio::test]
    async fn enabled_installed_package_returns_raw_bytes_and_exact_revision() {
        let fixture = Fixture::new("exec cat").await;
        let receipt = invoke(
            fixture.pool.clone(),
            fixture.manifest.id.clone(),
            AddonRuntimeContext::default(),
            request_bytes(),
            None,
        )
        .await
        .unwrap();
        assert_eq!(receipt.stdout, request_bytes());
        assert_eq!(receipt.state_revision, "fixture-revision");
        assert!(fixture.marker.exists());
    }

    #[tokio::test]
    async fn generic_dispatch_remains_rejected_even_for_enabled_declared_package() {
        let fixture = Fixture::new("exec cat").await;
        let result = super::super::super::resolve_enabled_addon_command(
            &fixture.pool,
            &fixture.manifest.id,
            "teletext.pages.v1",
            "pages",
            &AddonRuntimeContext::default(),
        )
        .await;
        assert!(matches!(result, Err(AddonError::Validation(_))));
        assert!(!fixture.marker.exists());
    }

    #[tokio::test]
    async fn rejected_packages_never_spawn() {
        let fixture = Fixture::new("exec cat").await;
        for rejection in [
            "disabled",
            "capability",
            "host-version",
            "dependency",
            "setting",
            "entrypoint",
            "content-hash",
            "missing",
        ] {
            let mut manifest = fixture.manifest.clone();
            sqlx::query(r#"UPDATE "AddonInstallation" SET "Enabled" = 1 WHERE "AddonId" = ?"#)
                .bind(&manifest.id)
                .execute(&fixture.pool)
                .await
                .unwrap();
            match rejection {
                "disabled" => {
                    sqlx::query(
                        r#"UPDATE "AddonInstallation" SET "Enabled" = 0 WHERE "AddonId" = ?"#,
                    )
                    .bind(&manifest.id)
                    .execute(&fixture.pool)
                    .await
                    .unwrap();
                }
                "capability" => manifest
                    .capabilities
                    .retain(|capability| capability.id != "teletext.pages.v1"),
                "host-version" => manifest.host_version = VersionReq::parse("<0.0.1").unwrap(),
                "dependency" => manifest.dependencies.push(AddonDependency {
                    id: "org.example.missing".to_owned(),
                    version: VersionReq::STAR,
                    optional: false,
                }),
                "setting" => manifest
                    .settings
                    .push(setting(AddonSettingKind::String, None)),
                "entrypoint" => {
                    let entrypoint = manifest.entrypoints.remove("any").unwrap();
                    manifest
                        .entrypoints
                        .insert("win-x64".to_owned(), entrypoint);
                }
                "content-hash" => {
                    std::fs::write(fixture.package.join("tampered"), b"tampered").unwrap();
                }
                "missing" => {
                    sqlx::query(r#"DELETE FROM "AddonInstallation" WHERE "AddonId" = ?"#)
                        .bind(&manifest.id)
                        .execute(&fixture.pool)
                        .await
                        .unwrap();
                }
                _ => unreachable!(),
            }
            fixture.store_manifest(&manifest).await;
            assert_eq!(
                invoke(
                    fixture.pool.clone(),
                    manifest.id,
                    AddonRuntimeContext::default(),
                    request_bytes(),
                    None
                )
                .await,
                Err(TeletextInvocationError::AdmissionRejected),
                "{rejection}"
            );
            assert!(!fixture.marker.exists(), "{rejection}");
        }
    }

    #[tokio::test]
    async fn cancelled_resolution_joins_started_filesystem_job_before_returning() {
        let fixture = Fixture::new("exec cat").await;
        let blocker = Blocker::new("hash", 1);
        let _release = ReleaseOnDrop(Arc::clone(&blocker));
        let fence = fenced(&blocker, Instant::now() + Duration::from_secs(30));
        let stopped = Arc::clone(&fence.stopped);
        let completed = Arc::clone(&fence.worker_completed);
        let (cancel, cancelled) = oneshot::channel();
        let invocation = tokio::spawn(invoke_with_fence(
            fixture.invocation(Some(cancelled)),
            fence,
            Duration::from_secs(5),
        ));
        blocker.wait_entered().await;
        cancel.send(()).unwrap();
        wait_true(&stopped).await;
        assert!(!invocation.is_finished());
        assert!(!completed.load(Ordering::Acquire));
        let stages = blocker.stages.lock().unwrap().len();
        blocker.release();
        assert_eq!(
            invocation.await.unwrap(),
            Err(TeletextInvocationError::Cancelled)
        );
        assert!(blocker.finished.load(Ordering::Acquire));
        assert!(completed.load(Ordering::Acquire));
        assert_eq!(blocker.stages.lock().unwrap().len(), stages);
        assert!(!fixture.marker.exists());
    }

    #[tokio::test]
    async fn abandoned_caller_retains_resolution_until_join_without_later_stages() {
        let fixture = Fixture::new("exec cat").await;
        let blocker = Blocker::new("hash", 1);
        let _release = ReleaseOnDrop(Arc::clone(&blocker));
        let fence = fenced(&blocker, Instant::now() + Duration::from_secs(30));
        let stopped = Arc::clone(&fence.stopped);
        let completed = Arc::clone(&fence.worker_completed);
        let invocation = tokio::spawn(invoke_with_fence(
            fixture.invocation(None),
            fence,
            Duration::from_secs(5),
        ));
        blocker.wait_entered().await;
        invocation.abort();
        assert!(invocation.await.unwrap_err().is_cancelled());
        wait_true(&stopped).await;
        assert!(!completed.load(Ordering::Acquire));
        let stages = blocker.stages.lock().unwrap().len();
        blocker.release();
        wait_true(&completed).await;
        assert!(blocker.finished.load(Ordering::Acquire));
        assert_eq!(blocker.stages.lock().unwrap().len(), stages);
        assert!(!fixture.marker.exists());
    }

    #[tokio::test]
    async fn settlement_uncertainty_returns_but_keeps_pending_job_owned_until_join() {
        let fixture = Fixture::new("exec cat").await;
        let blocker = Blocker::new("hash", 1);
        let _release = ReleaseOnDrop(Arc::clone(&blocker));
        let fence = fenced(&blocker, Instant::now() + Duration::from_secs(30));
        let completed = Arc::clone(&fence.worker_completed);
        let (cancel, cancelled) = oneshot::channel();
        let invocation = tokio::spawn(invoke_with_fence(
            fixture.invocation(Some(cancelled)),
            fence,
            Duration::from_millis(20),
        ));
        blocker.wait_entered().await;
        cancel.send(()).unwrap();
        let result = tokio::time::timeout(Duration::from_secs(1), invocation)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(result, Err(TeletextInvocationError::CleanupUnconfirmed));
        assert!(!blocker.finished.load(Ordering::Acquire));
        assert!(!completed.load(Ordering::Acquire));
        let stages = blocker.stages.lock().unwrap().len();
        blocker.release();
        wait_true(&completed).await;
        assert!(blocker.finished.load(Ordering::Acquire));
        assert_eq!(blocker.stages.lock().unwrap().len(), stages);
        assert!(!fixture.marker.exists());
    }

    #[tokio::test]
    async fn revision_or_enabled_change_during_resolution_rejects_prelaunch() {
        let fixture = Fixture::new("exec cat").await;
        for change in ["revision", "disabled"] {
            sqlx::query(r#"UPDATE "AddonInstallation" SET "Enabled" = 1, "StateRevision" = 'fixture-revision' WHERE "AddonId" = ?"#).bind(&fixture.manifest.id).execute(&fixture.pool).await.unwrap();
            let blocker = Blocker::new("hash", 1);
            let _release = ReleaseOnDrop(Arc::clone(&blocker));
            let fence = fenced(&blocker, Instant::now() + Duration::from_secs(30));
            let invocation = tokio::spawn(invoke_with_fence(
                fixture.invocation(None),
                fence,
                Duration::from_secs(5),
            ));
            blocker.wait_entered().await;
            let sql = if change == "revision" {
                r#"UPDATE "AddonInstallation" SET "StateRevision" = 'changed' WHERE "AddonId" = ?"#
            } else {
                r#"UPDATE "AddonInstallation" SET "Enabled" = 0 WHERE "AddonId" = ?"#
            };
            sqlx::query(sql)
                .bind(&fixture.manifest.id)
                .execute(&fixture.pool)
                .await
                .unwrap();
            blocker.release();
            assert_eq!(
                invocation.await.unwrap(),
                Err(TeletextInvocationError::AdmissionRejected)
            );
            assert!(!fixture.marker.exists());
        }
    }

    #[tokio::test]
    async fn cancelled_tool_canonicalization_is_not_mislabeled_cleanup_uncertainty() {
        let fixture = Fixture::new("exec cat").await;
        let tool = fixture.package.join("tool");
        std::fs::write(&tool, b"synthetic tool").unwrap();
        let digest = super::super::super::calculate_addon_content_sha256(&fixture.package)
            .await
            .unwrap();
        sqlx::query(r#"UPDATE "AddonInstallation" SET "ContentSha256" = ? WHERE "AddonId" = ?"#)
            .bind(digest)
            .bind(&fixture.manifest.id)
            .execute(&fixture.pool)
            .await
            .unwrap();
        let mut manifest = fixture.manifest.clone();
        manifest.settings.push(setting(
            AddonSettingKind::Executable,
            Some(tool.to_string_lossy().into_owned()),
        ));
        fixture.store_manifest(&manifest).await;
        // Root verification, entrypoint, then configured executable canonicalization.
        let blocker = Blocker::new("canonicalize", 3);
        let _release = ReleaseOnDrop(Arc::clone(&blocker));
        let fence = fenced(&blocker, Instant::now() + Duration::from_secs(30));
        let stopped = Arc::clone(&fence.stopped);
        let (cancel, cancelled) = oneshot::channel();
        let invocation = tokio::spawn(invoke_with_fence(
            fixture.invocation(Some(cancelled)),
            fence,
            Duration::from_secs(5),
        ));
        blocker.wait_entered().await;
        cancel.send(()).unwrap();
        wait_true(&stopped).await;
        blocker.release();
        assert_eq!(
            invocation.await.unwrap(),
            Err(TeletextInvocationError::Cancelled)
        );
        assert!(!fixture.marker.exists());
    }

    #[tokio::test]
    async fn resolution_consumes_command_deadline_instead_of_resetting_it() {
        let fixture = Fixture::new("cat >/dev/null; exec sleep 30").await;
        let blocker = Blocker::new("hash", 1);
        let _release = ReleaseOnDrop(Arc::clone(&blocker));
        let started = Instant::now();
        let fence = fenced(&blocker, started + Duration::from_secs(2));
        let invocation = tokio::spawn(invoke_with_fence(
            fixture.invocation(None),
            fence,
            Duration::from_secs(5),
        ));
        blocker.wait_entered().await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        blocker.release();
        let result = tokio::time::timeout(Duration::from_secs(3), invocation)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(result, Err(TeletextInvocationError::TimedOut));
        assert!(
            fixture.marker.exists(),
            "command began with only the remaining budget"
        );
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[tokio::test]
    async fn execution_cancellation_awaits_managed_leader_cleanup() {
        let fixture =
            Fixture::new("cat >/dev/null; printf '%s\\n' \"$$\" > ../ready; exec sleep 30").await;
        let ready = fixture.package.parent().unwrap().join("ready");
        let (cancel, cancelled) = oneshot::channel();
        let invocation = tokio::spawn(invoke(
            fixture.pool.clone(),
            fixture.manifest.id.clone(),
            AddonRuntimeContext::default(),
            request_bytes(),
            Some(cancelled),
        ));
        tokio::time::timeout(Duration::from_secs(3), async {
            while !tokio::fs::read_to_string(&ready)
                .await
                .is_ok_and(|text| text.ends_with('\n'))
            {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let pid: i32 = tokio::fs::read_to_string(&ready)
            .await
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        cancel.send(()).unwrap();
        assert_eq!(
            invocation.await.unwrap(),
            Err(TeletextInvocationError::Cancelled)
        );
        assert_eq!(
            nix::sys::signal::killpg(nix::unistd::Pid::from_raw(pid), None),
            Err(nix::errno::Errno::ESRCH)
        );
    }
    // Isolate the durable tool receipt store in a subprocess. Mutating process
    // environment in this test process could affect concurrently running tests.
    async fn isolate_profile_tool_receipts() -> bool {
        const HELPER: &str = "ERSATZRS_TEST_TELETEXT_PROFILE_TOOL_RECEIPTS";
        if crate::startup_config::environment_variable(HELPER).is_some() {
            return false;
        }
        let temp = tempfile::tempdir().unwrap();
        let name = concat!(
            module_path!(),
            "::two_channels_one_package_use_distinct_effective_profiles_and_exact_receipts"
        );
        let mut process = tokio::process::Command::new(std::env::current_exe().unwrap());
        process
            .args([
                "--exact",
                name.strip_prefix("ersatzrs_infra::").unwrap(),
                "--nocapture",
            ])
            .env(HELPER, "1")
            .env(
                crate::file_system_layout::APP_STATE_DIR_ENV_VAR,
                temp.path(),
            )
            .env(crate::file_system_layout::APP_DATA_DIR_ENV_VAR, temp.path())
            .kill_on_drop(true);
        let output = tokio::time::timeout(Duration::from_secs(90), process.output())
            .await
            .unwrap()
            .unwrap();
        assert!(
            output.status.success(),
            "isolated profile tool fixture failed: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        true
    }

    #[tokio::test]
    async fn two_channels_one_package_use_distinct_effective_profiles_and_exact_receipts() {
        if isolate_profile_tool_receipts().await {
            return;
        }
        let fixture =
            Fixture::new("cat >/dev/null; printf '%s' \"$ERSATZRS_ADDON_SETTING_VALUE\"").await;
        let selections = fixture.profile_selections(true).await;
        for (selection, expected) in selections.iter().zip(["left", "right"]) {
            let command = resolve_profile_for_launch(
                &fixture.pool,
                selection,
                &AddonRuntimeContext::default(),
                &ResolutionFence::new(Instant::now() + EXECUTION_TIMEOUT),
            )
            .await
            .unwrap();
            let expected_tool = selection
                .effective_settings
                .iter()
                .find(|value| value.key == "TOOL")
                .unwrap();
            let TeletextProfileValue::Literal(path) = &expected_tool.value else {
                panic!("expected literal")
            };
            assert_eq!(command.tool_paths, vec![dunce::canonicalize(path).unwrap()]);
            let receipt = invoke_profile(
                fixture.pool.clone(),
                selection.clone(),
                AddonRuntimeContext::default(),
                request_bytes(),
                None,
            )
            .await
            .unwrap();
            assert_eq!(receipt.stdout, expected.as_bytes());
            assert_eq!(receipt.selection, *selection);
            assert_eq!(
                receipt.generation,
                TeletextPagesRequest::from_json(&request_bytes())
                    .unwrap()
                    .generation
            );
        }
        let settings: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonSetting" WHERE "AddonId" = ?"#)
                .bind(&fixture.manifest.id)
                .fetch_one(&fixture.pool)
                .await
                .unwrap();
        assert_eq!(settings, 0);
    }

    #[tokio::test]
    async fn forged_profile_snapshot_is_rejected_before_filesystem_or_spawn() {
        let fixture = Fixture::new("exec cat").await;
        let [selection, _] = fixture.profile_selections(false).await;
        for field in 0..12 {
            let mut changed = selection.clone();
            match field {
                0 => changed.channel_id = 2,
                1 => changed.addon_id.push_str(".changed"),
                2 => changed.binding_revision += 1,
                3 => changed.profile_id += 1,
                4 => changed.profile_revision += 1,
                5 => changed.installation.addon_id.push_str(".changed"),
                6 => changed.installation.state_revision.push_str("changed"),
                7 => changed.installation.version.push_str("changed"),
                8 => changed.installation.package_sha256.push_str("changed"),
                9 => changed.installation.content_sha256.push_str("changed"),
                10 => changed.installation.manifest_sha256.push_str("changed"),
                _ => changed.effective_settings[0] = profile_literal("VALUE", "changed"),
            }
            let blocker = Blocker::new("hash", 1);
            let _release = ReleaseOnDrop(Arc::clone(&blocker));
            let mut invocation = fixture.invocation(None);
            invocation.selection = Some(changed);
            assert_eq!(
                invoke_with_fence(
                    invocation,
                    fenced(&blocker, Instant::now() + EXECUTION_TIMEOUT),
                    Duration::from_secs(1)
                )
                .await,
                Err(TeletextInvocationError::AdmissionRejected)
            );
            assert!(blocker.stages.lock().unwrap().is_empty());
            assert!(!fixture.marker.exists());
        }
    }

    #[tokio::test]
    async fn profile_prelaunch_fence_rejects_changes_after_resolution_started() {
        for change in ["profile", "binding", "global", "manifest"] {
            let fixture = Fixture::new("exec cat").await;
            let [selection, _] = fixture.profile_selections(false).await;
            let blocker = Blocker::new("hash", 1);
            let _release = ReleaseOnDrop(Arc::clone(&blocker));
            let mut invocation = fixture.invocation(None);
            invocation.selection = Some(selection.clone());
            let task = tokio::spawn(invoke_with_fence(
                invocation,
                fenced(&blocker, Instant::now() + EXECUTION_TIMEOUT),
                Duration::from_secs(5),
            ));
            blocker.wait_entered().await;
            match change {
                "profile" => {
                    sqlx::query(r#"UPDATE "TeletextProfileSetting" SET "Value" = 'changed' WHERE "ProfileId" = ? AND "Key" = 'VALUE'"#).bind(selection.profile_id).execute(&fixture.pool).await.unwrap();
                }
                "binding" => {
                    sqlx::query(r#"UPDATE "ChannelTeletextBinding" SET "Enabled" = 0 WHERE "ChannelId" = 1"#).execute(&fixture.pool).await.unwrap();
                }
                "global" => {
                    // Effective inherited values are fenced even when the
                    // installation revision and manifest remain unchanged.
                    sqlx::query(r#"INSERT INTO "AddonSetting" ("AddonId", "Key", "Value") VALUES (?, 'OTHER', 'changed')"#).bind(&fixture.manifest.id).execute(&fixture.pool).await.unwrap();
                }
                _ => {
                    sqlx::query(r#"UPDATE "AddonInstallation" SET "ManifestJson" = "ManifestJson" || ' ' WHERE "AddonId" = ?"#).bind(&fixture.manifest.id).execute(&fixture.pool).await.unwrap();
                }
            }
            blocker.release();
            assert_eq!(
                task.await.unwrap(),
                Err(TeletextInvocationError::AdmissionRejected)
            );
            assert!(!fixture.marker.exists());
        }
    }

    #[tokio::test]
    async fn profile_cancel_or_caller_drop_retains_blocking_resolution_until_joined() {
        for abandon in [false, true] {
            let fixture = Fixture::new("exec cat").await;
            let [selection, _] = fixture.profile_selections(false).await;
            let blocker = Blocker::new("hash", 1);
            let _release = ReleaseOnDrop(Arc::clone(&blocker));
            let fence = fenced(&blocker, Instant::now() + EXECUTION_TIMEOUT);
            let stopped = Arc::clone(&fence.stopped);
            let completed = Arc::clone(&fence.worker_completed);
            let (cancel, cancelled) = oneshot::channel();
            let mut invocation = fixture.invocation(Some(cancelled));
            invocation.selection = Some(selection);
            let task = tokio::spawn(invoke_with_fence(invocation, fence, Duration::from_secs(5)));
            blocker.wait_entered().await;
            if abandon {
                task.abort();
            } else {
                cancel.send(()).unwrap();
            }
            wait_true(&stopped).await;
            assert!(!completed.load(Ordering::Acquire));
            blocker.release();
            wait_true(&completed).await;
            if !abandon {
                assert_eq!(task.await.unwrap(), Err(TeletextInvocationError::Cancelled));
            }
            assert!(blocker.finished.load(Ordering::Acquire));
            assert!(!fixture.marker.exists());
        }
    }
    #[tokio::test]
    async fn expired_shared_profile_deadline_prevents_native_work() {
        let fixture = Fixture::new("exec cat").await;
        let [selection, _] = fixture.profile_selections(false).await;
        assert_eq!(
            invoke_profile_with_deadline(
                fixture.pool.clone(),
                selection.clone(),
                AddonRuntimeContext::default(),
                request_bytes(),
                std::time::Instant::now()
                    .checked_sub(Duration::from_secs(1))
                    .unwrap(),
                None,
            )
            .await,
            Err(TeletextInvocationError::TimedOut)
        );
        let blocker = Blocker::new("hash", 1);
        let _release = ReleaseOnDrop(Arc::clone(&blocker));
        let mut invocation = fixture.invocation(None);
        invocation.selection = Some(selection);
        assert_eq!(
            invoke_with_fence(
                invocation,
                fenced(&blocker, Instant::now()),
                Duration::from_secs(1)
            )
            .await,
            Err(TeletextInvocationError::TimedOut),
        );
        assert!(blocker.stages.lock().unwrap().is_empty());
        assert!(!fixture.marker.exists());
    }

    #[tokio::test]
    async fn shared_profile_deadline_is_carried_into_owned_command() {
        let fixture = Fixture::new("cat >/dev/null; exec sleep 30").await;
        let [selection, _] = fixture.profile_selections(false).await;
        let started = std::time::Instant::now();
        let task = tokio::spawn(invoke_profile_with_deadline(
            fixture.pool.clone(),
            selection,
            AddonRuntimeContext::default(),
            request_bytes(),
            started + Duration::from_secs(3),
            None,
        ));
        tokio::time::timeout(Duration::from_secs(2), async {
            while !fixture.marker.exists() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(8), task)
                .await
                .unwrap()
                .unwrap(),
            Err(TeletextInvocationError::TimedOut)
        );
        assert!(started.elapsed() < Duration::from_secs(8));
    }
}
