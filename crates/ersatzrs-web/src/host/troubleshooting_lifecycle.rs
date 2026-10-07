//! Ephemeral preview identity, admission and cleanup-aware state.

use super::{
    map_runtime_error, next, read_completed_preview_log, start_playback_preview,
    troubleshooting_folder, AdminTroubleshootingRuntimeError, Arc, DateTime, Duration, FsPath,
    NativeAdminTroubleshootingRuntime, PlaybackPreviewLog, PlaybackPreviewRequest,
    PreviewAvailability, PreviewOwnership, Utc,
};
use ersatzrs_app::{PlaybackPreviewState as Phase, PlaybackPreviewStatus};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

const HISTORY_LIMIT: usize = 16;
const MAX_HLS_FILE_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Default)]
pub(super) struct Registry {
    jobs: VecDeque<Arc<Job>>,
}

pub(super) struct Job {
    pub(super) cancel: CancellationToken,
    pub(super) spawned: AtomicBool,
    status: watch::Sender<PlaybackPreviewStatus>,
    reservation:
        std::sync::Mutex<Option<ersatzrs_infra::transcode_capacity::ActiveTranscodeReservation>>,
    failure: std::sync::Mutex<Option<(bool, String)>>,
}

impl Job {
    fn new(request: &PlaybackPreviewRequest) -> Self {
        let id = uuid::Uuid::new_v4().to_string();
        let channel = request.channel_id.unwrap_or_default();
        let media = request.media_item_id.unwrap_or_default();
        let playlist = if request.streaming_engine == ersatzrs_domain::StreamingEngine::Next {
            "ffmpeg.m3u8"
        } else {
            "live.m3u8"
        };
        let now = Utc::now().to_rfc3339();
        let status = PlaybackPreviewStatus {
            id: id.clone(),
            state: Phase::Queued,
            ready: false,
            cleanup_confirmed: false,
            playlist_url: format!("/api/troubleshoot/playback/previews/{id}/files/{playlist}"),
            selected_at: None,
            created_at: now.clone(),
            updated_at: now,
            error_code: None,
            return_url: format!(
                "/system/troubleshooting/playback?mediaItem={media}&channel={channel}&previewId={id}"
            ),
            log: None,
        };
        let (status, _) = watch::channel(status);
        Self {
            cancel: CancellationToken::new(),
            spawned: AtomicBool::new(false),
            status,
            reservation: std::sync::Mutex::new(None),
            failure: std::sync::Mutex::new(None),
        }
    }

    pub(super) fn read(&self) -> PlaybackPreviewStatus {
        self.status.borrow().clone()
    }

    pub(super) fn retain_reservation(
        &self,
        reservation: ersatzrs_infra::transcode_capacity::ActiveTranscodeReservation,
    ) {
        *self
            .reservation
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(reservation);
    }

    pub(super) fn take_reservation(
        &self,
    ) -> Option<ersatzrs_infra::transcode_capacity::ActiveTranscodeReservation> {
        self.reservation
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    }

    pub(super) fn preparation_reservation(
        &self,
    ) -> Option<ersatzrs_infra::transcode_capacity::ActiveTranscodeReservation> {
        self.reservation
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub(super) fn running(&self) {
        self.spawned.store(true, Ordering::Release);
        self.status.send_modify(|status| {
            if !self.cancel.is_cancelled() {
                status.state = Phase::Running;
            }
            status.updated_at = Utc::now().to_rfc3339();
        });
    }

    pub(super) fn ready(&self) {
        self.status.send_modify(|status| {
            if !self.cancel.is_cancelled() {
                status.ready = true;
            }
            status.updated_at = Utc::now().to_rfc3339();
        });
    }

    pub(super) fn selected_at(&self, at: DateTime<Utc>) {
        self.status
            .send_modify(|status| status.selected_at = Some(at.to_rfc3339()));
    }

    pub(super) fn failed_runtime(&self, error: &AdminTroubleshootingRuntimeError) {
        let missing = matches!(error, AdminTroubleshootingRuntimeError::NotFound(_));
        *self
            .failure
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Some((missing, error.to_string()));
        self.fail(startup_code(&error.to_string()));
    }

    pub(super) fn fail(&self, code: &str) {
        self.status.send_modify(|status| {
            status.error_code = Some(code.to_owned());
            status.updated_at = Utc::now().to_rfc3339();
        });
    }

    pub(super) fn cancel(&self) {
        self.status.send_modify(|status| {
            if !status.state.is_terminal() {
                status.state = Phase::Cancelling;
                status.ready = false;
                status.updated_at = Utc::now().to_rfc3339();
                self.cancel.cancel();
            }
        });
    }

    pub(super) fn finish(
        &self,
        success: bool,
        cleanup_confirmed: bool,
        log: Option<PlaybackPreviewLog>,
    ) {
        self.status.send_modify(|status| {
            status.cleanup_confirmed = cleanup_confirmed;
            if !cleanup_confirmed {
                status.error_code = Some("cleanup".into());
            }
            status.state =
                if status.error_code.is_some() || (!success && !self.cancel.is_cancelled()) {
                    if status.error_code.is_none() {
                        status.error_code = Some("startup".into());
                    }
                    Phase::Failed
                } else if self.cancel.is_cancelled() {
                    Phase::Cancelled
                } else {
                    Phase::Completed
                };
            if status.state != Phase::Completed {
                status.ready = false;
            }
            status.log = log;
            status.updated_at = Utc::now().to_rfc3339();
        });
    }

    pub(super) async fn wait_ready(&self) -> Result<(), AdminTroubleshootingRuntimeError> {
        let mut changes = self.status.subscribe();
        loop {
            let status = changes.borrow_and_update().clone();
            if status.ready {
                return Ok(());
            }
            if status.state.is_terminal() {
                if let Some((missing, error)) = self
                    .failure
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .as_ref()
                {
                    return Err(if *missing {
                        AdminTroubleshootingRuntimeError::NotFound(error.clone())
                    } else {
                        AdminTroubleshootingRuntimeError::unavailable(error.clone())
                    });
                }
                return Err(AdminTroubleshootingRuntimeError::unavailable(match status
                    .error_code
                    .as_deref()
                {
                    Some("configuration") => {
                        "failed to decode channel streaming_mode: invalid value"
                    }
                    Some("cleanup") => "troubleshooting process cleanup is unconfirmed",
                    _ => "troubleshooting playback could not start",
                }));
            }
            changes.changed().await.map_err(|_| {
                AdminTroubleshootingRuntimeError::unavailable("preview owner stopped")
            })?;
        }
    }
}

pub(super) fn job(ownership: &PreviewOwnership) -> Option<Arc<Job>> {
    if let PreviewAvailability::Active(job) = &**ownership {
        Some(Arc::clone(job))
    } else {
        None
    }
}

pub(super) async fn complete_ownership(
    mut ownership: PreviewOwnership,
    job: Option<Arc<Job>>,
    success: bool,
    folder: &FsPath,
) {
    let confirmed = !matches!(*ownership, PreviewAvailability::CleanupUnconfirmed { .. });
    let log = if job.is_some() && confirmed {
        read_completed_preview_log(folder).await.ok().flatten()
    } else {
        None
    };
    if confirmed {
        *ownership = PreviewAvailability::Available;
    }
    if let Some(job) = &job {
        let reservation = job.take_reservation();
        if !confirmed {
            // Retain capacity with the poisoned owner, even after history eviction.
            if let PreviewAvailability::CleanupUnconfirmed {
                _reservation: retained,
            } = &mut *ownership
            {
                if retained.is_none() {
                    *retained = reservation;
                }
            }
        }
    }
    drop(ownership);
    if let Some(job) = job {
        job.finish(success, confirmed, log);
    }
}

pub(super) fn startup_code(error: &str) -> &'static str {
    if error.starts_with("failed to decode channel streaming_mode:") {
        "configuration"
    } else if error.contains("capacity") {
        "capacity"
    } else if error.contains("cleanup is unconfirmed") {
        "cleanup"
    } else {
        "startup"
    }
}

impl NativeAdminTroubleshootingRuntime {
    pub(super) async fn admit_preview(
        &self,
        request: PlaybackPreviewRequest,
    ) -> Result<Arc<Job>, AdminTroubleshootingRuntimeError> {
        let mut ownership = Arc::clone(&self.preview_lock)
            .try_lock_owned()
            .map_err(|_| {
                AdminTroubleshootingRuntimeError::NotFound(
                    "troubleshooting playback is already running".into(),
                )
            })?;
        if matches!(*ownership, PreviewAvailability::CleanupUnconfirmed { .. }) {
            return Err(AdminTroubleshootingRuntimeError::unavailable(
                "troubleshooting process cleanup is unconfirmed; restart is required",
            ));
        }
        let job = Arc::new(Job::new(&request));
        *ownership = PreviewAvailability::Active(Arc::clone(&job));
        {
            let mut registry = self.previews.write().await;
            for old in &registry.jobs {
                old.status.send_modify(|status| status.log = None);
            }
            registry.jobs.push_back(Arc::clone(&job));
            while registry.jobs.len() > HISTORY_LIMIT {
                registry.jobs.pop_front();
            }
        }
        let state = self.clone();
        let worker = Arc::clone(&job);
        tokio::spawn(async move {
            if worker.cancel.is_cancelled() {
                *ownership = PreviewAvailability::Available;
                drop(ownership);
                worker.finish(false, true, None);
                return;
            }
            let result = if request.streaming_engine == ersatzrs_domain::StreamingEngine::Next {
                next::start(state, request.into(), ownership).await
            } else {
                Box::pin(start_playback_preview(&state, &request.into(), ownership))
                    .await
                    .map_err(map_runtime_error)
            };
            match result {
                Ok(()) => worker.ready(),
                Err(error) => {
                    let prior = worker.read();
                    if prior.state == Phase::Completed && !prior.ready {
                        worker.failed_runtime(&error);
                        worker.finish(false, prior.cleanup_confirmed, prior.log);
                    } else if !prior.state.is_terminal() {
                        if !worker.cancel.is_cancelled() {
                            worker.failed_runtime(&error);
                        }
                        worker.cancel.cancel();
                        // A spawned supervisor retains ownership and publishes its own terminal state.
                        if !worker.spawned.load(Ordering::Acquire) {
                            worker.finish(false, true, None);
                        }
                    }
                }
            }
        });
        Ok(job)
    }

    pub(super) async fn record_preview_selection(&self, at: DateTime<Utc>) {
        // Preparation owns the shared preview lock, so the latest ID is its own.
        if let Some(job) = self.previews.read().await.jobs.back() {
            job.selected_at(at);
        }
    }

    pub(super) async fn find_preview(&self, id: &str) -> Option<Arc<Job>> {
        if uuid::Uuid::parse_str(id).ok()?.to_string() != id {
            return None;
        }
        self.previews
            .read()
            .await
            .jobs
            .iter()
            .find(|job| job.read().id == id)
            .cloned()
    }

    pub(super) async fn read_owned_file(
        &self,
        id: &str,
        filename: &str,
    ) -> Result<Option<Vec<u8>>, AdminTroubleshootingRuntimeError> {
        use tokio::io::AsyncReadExt as _;
        if filename.is_empty()
            || filename.contains(['/', '\\'])
            || filename.contains("..")
            || !matches!(
                FsPath::new(filename)
                    .extension()
                    .and_then(|ext| ext.to_str()),
                Some("m3u8" | "ts" | "m4s" | "mp4")
            )
        {
            return Ok(None);
        }
        // Admission holds the write guard before clearing shared output. A stale
        // ID cannot cross that boundary and read a replacement's HLS member.
        let registry = self.previews.read().await;
        let Some(latest) = registry.jobs.back() else {
            return Ok(None);
        };
        let status = latest.read();
        if status.id != id || !status.ready {
            return Ok(None);
        }
        tokio::time::timeout(Duration::from_secs(2), async {
            let file = match tokio::fs::File::open(troubleshooting_folder().join(filename)).await {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(_) => {
                    return Err(AdminTroubleshootingRuntimeError::unavailable(
                        "preview output cannot be read",
                    ));
                }
            };
            if file
                .metadata()
                .await
                .map_err(|_| {
                    AdminTroubleshootingRuntimeError::unavailable(
                        "preview output cannot be inspected",
                    )
                })?
                .len()
                > MAX_HLS_FILE_BYTES
            {
                return Ok(None);
            }
            let mut bytes = Vec::new();
            file.take(MAX_HLS_FILE_BYTES + 1)
                .read_to_end(&mut bytes)
                .await
                .map_err(|_| {
                    AdminTroubleshootingRuntimeError::unavailable("preview output cannot be read")
                })?;
            if bytes.len() as u64 > MAX_HLS_FILE_BYTES {
                return Ok(None);
            }
            Ok(Some(bytes))
        })
        .await
        .map_err(|_| {
            AdminTroubleshootingRuntimeError::unavailable("preview output read timed out")
        })?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use crate::host::troubleshooting::{
        write_preview_logs, Command, DynamicTextController, ManagedProcess, Mutex, PathBuf, Stdio,
    };
    use crate::host::troubleshooting::{FfmpegRuntimeReport, RwLock};

    fn runtime() -> NativeAdminTroubleshootingRuntime {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect_lazy("sqlite::memory:")
            .unwrap();
        NativeAdminTroubleshootingRuntime::new(
            pool.clone(),
            Arc::new(ersatzrs_infra::repositories::make_repositories(
                pool.clone(),
            )),
            Arc::new(RwLock::new(FfmpegRuntimeReport::for_tests())),
            Arc::default(),
            Arc::new(ersatzrs_infra::next_engine::host::NextHost::new(
                pool,
                ersatzrs_core::locking::EntityLocker::default(),
                std::env::temp_dir().join("preview-lifecycle-unused"),
            )),
        )
    }

    #[tokio::test]
    async fn queued_stop_releases_ownership_without_preparing_or_spawning() {
        let runtime = runtime();
        let job = runtime
            .admit_preview(PlaybackPreviewRequest::default())
            .await
            .unwrap();
        assert_eq!(job.read().state, Phase::Queued);
        job.cancel();
        assert_eq!(job.read().state, Phase::Cancelling);
        assert!(runtime
            .admit_preview(PlaybackPreviewRequest::default())
            .await
            .is_err());
        let mut changes = job.status.subscribe();
        tokio::time::timeout(Duration::from_secs(2), async {
            while !changes.borrow_and_update().state.is_terminal() {
                changes.changed().await.unwrap();
            }
        })
        .await
        .unwrap();
        let status = job.read();
        assert_eq!(status.state, Phase::Cancelled);
        assert!(status.cleanup_confirmed);
        assert!(!job.spawned.load(Ordering::Acquire));
        assert!(runtime.preview_lock.try_lock().is_ok());
        assert!(!status.ready);
        job.cancel();
        assert_eq!(job.read(), status);
    }

    #[tokio::test]
    async fn stale_preview_ids_and_cancelled_jobs_cannot_read_replacement_files() {
        let runtime = runtime();
        let old = Arc::new(Job::new(&PlaybackPreviewRequest::default()));
        old.ready();
        old.finish(true, true, None);
        let replacement = Arc::new(Job::new(&PlaybackPreviewRequest::default()));
        runtime
            .previews
            .write()
            .await
            .jobs
            .extend([Arc::clone(&old), Arc::clone(&replacement)]);
        assert!(runtime.find_preview(&old.read().id).await.is_some());
        assert!(runtime.find_preview("not-an-owned-id").await.is_none());
        assert!(runtime
            .read_owned_file(&old.read().id, "live.m3u8")
            .await
            .unwrap()
            .is_none());
        replacement.ready();
        replacement.cancel();
        assert!(runtime
            .read_owned_file(&replacement.read().id, "live.m3u8")
            .await
            .unwrap()
            .is_none());
        for path in ["../logs.txt", "a/b.ts", "a\\b.ts", "logs.txt", ""] {
            assert!(runtime
                .read_owned_file(&replacement.read().id, path)
                .await
                .unwrap()
                .is_none());
        }
    }

    #[tokio::test]
    async fn failed_cleanup_quarantines_admission_and_retains_capacity() {
        use ersatzrs_infra::transcode_capacity::{
            try_reserve_active_transcode, ActiveTranscodeSlot,
        };
        let runtime = runtime();
        let job = Arc::new(Job::new(&PlaybackPreviewRequest::default()));
        let key = format!("preview-quarantine-{}", job.read().id);
        job.retain_reservation(
            try_reserve_active_transcode(
                i32::MAX,
                ActiveTranscodeSlot::new_next_hls_with_capacity_key(&key, &key),
            )
            .unwrap(),
        );
        let mut ownership = Arc::clone(&runtime.preview_lock).lock_owned().await;
        *ownership = PreviewAvailability::CleanupUnconfirmed { _reservation: None };
        let root = tempfile::tempdir().unwrap();
        complete_ownership(ownership, Some(Arc::clone(&job)), false, root.path()).await;
        assert_eq!(job.read().state, Phase::Failed);
        assert_eq!(job.read().error_code.as_deref(), Some("cleanup"));
        assert!(!job.read().cleanup_confirmed);
        assert!(matches!(
            &*runtime.preview_lock.lock().await,
            PreviewAvailability::CleanupUnconfirmed {
                _reservation: Some(_)
            }
        ));
        let error = runtime
            .admit_preview(PlaybackPreviewRequest::default())
            .await
            .err()
            .unwrap();
        assert!(error.to_string().contains("restart is required"));
    }

    #[test]
    fn late_readiness_cannot_revive_a_cancelled_preview() {
        let job = Job::new(&PlaybackPreviewRequest::default());
        job.running();
        job.cancel();
        job.ready();
        job.finish(true, true, None);
        assert_eq!(job.read().state, Phase::Cancelled);
        assert!(!job.read().ready);
        assert!(job.read().cleanup_confirmed);
    }

    #[cfg(unix)]
    async fn refreshing_controller(text_file: PathBuf) -> DynamicTextController {
        DynamicTextController::start(ersatzrs_infra::dynamic_text::DynamicTextControllerRequest {
            broadcast_start_time: None,
            metadata_refresh: None,
            element_id: 1,
            template: "{{ Playback.ElapsedMilliseconds }}".into(),
            variables: serde_json::json!({}),
            text_file: text_file.clone(),
            refresh_interval_milliseconds: 100,
            playback_offset_seconds: 0.0,
            playback_duration_seconds: 30.0,
        })
        .await
        .unwrap()
    }

    // The shell descendant fixture is Unix-only. Production uses the existing
    // managed-process adapter, with Windows JobObject ownership as its counterpart.
    #[cfg(unix)]
    #[tokio::test]
    async fn running_stop_reaps_descendants_before_releasing_ownership_and_capacity() {
        use ersatzrs_infra::transcode_capacity::{
            try_reserve_active_transcode, ActiveTranscodeSlot,
        };
        let root = tempfile::tempdir().unwrap();
        let job = Arc::new(Job::new(&PlaybackPreviewRequest::default()));
        let key = format!("preview-stop-{}", job.read().id);
        job.retain_reservation(
            try_reserve_active_transcode(
                i32::MAX,
                ActiveTranscodeSlot::new_next_hls_with_capacity_key(&key, &key),
            )
            .unwrap(),
        );
        let lock = Arc::new(Mutex::new(PreviewAvailability::Active(Arc::clone(&job))));
        let ownership = Arc::clone(&lock).lock_owned().await;
        let mut command = Command::new("sh");
        command
            .args(["-c", "sleep 30 & echo $! > descendant.pid; wait"])
            .current_dir(root.path())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = ManagedProcess::spawn(command).unwrap();
        job.running();
        let stderr = child.take_stderr();
        let stdout = child.take_stdout();
        let text_file = root.path().join("controller.txt");
        let controller = refreshing_controller(text_file.clone()).await;
        let log = root.path().join("logs.txt");
        let completed = tokio::spawn(write_preview_logs(
            log.clone(),
            stderr,
            stdout,
            child,
            vec![controller],
            ownership,
            tokio::time::Instant::now() + Duration::from_mins(2),
            None,
        ));
        tokio::time::timeout(Duration::from_secs(2), async {
            while !root.path().join("descendant.pid").exists() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let pid = tokio::fs::read_to_string(root.path().join("descendant.pid"))
            .await
            .unwrap();
        assert!(lock.try_lock().is_err());
        job.cancel();
        assert!(!tokio::time::timeout(Duration::from_secs(5), completed)
            .await
            .unwrap()
            .unwrap());
        assert_eq!(job.read().state, Phase::Cancelled);
        assert!(job.read().cleanup_confirmed);
        assert!(job.take_reservation().is_none());
        assert!(matches!(
            *lock.try_lock().unwrap(),
            PreviewAvailability::Available
        ));
        assert!(tokio::fs::read_to_string(log)
            .await
            .unwrap()
            .contains("cancelled"));
        let after_stop = tokio::fs::read(&text_file).await.unwrap();
        tokio::time::sleep(Duration::from_millis(350)).await;
        assert_eq!(tokio::fs::read(text_file).await.unwrap(), after_stop);
        // A killed orphan can briefly remain a zombie awaiting the OS parent;
        // verify it cannot execute, rather than requiring /proc removal.
        let result = Command::new("ps")
            .args(["-o", "stat=", "-p", pid.trim()])
            .output()
            .await
            .unwrap();
        let state = String::from_utf8_lossy(&result.stdout);
        assert!(
            state.trim().is_empty() || state.trim().starts_with('Z'),
            "descendant still executing: {state}"
        );
    }
}
