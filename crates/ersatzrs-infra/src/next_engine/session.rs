//! Next lifecycle behind the host's existing HLS session interface.

use super::{
    coordinator::RefreshError,
    process::{NextProcess, ProcessError},
};
use async_trait::async_trait;
use ersatzrs_core::hls::{
    HlsAdmissionStatus, HlsPublicationError, HlsSessionHandle, HlsStartupReadiness,
};
use futures_util::FutureExt;
use std::{
    future::Future,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicI32, AtomicI64, AtomicU8, Ordering},
        Mutex,
    },
    time::Duration,
};
use tokio::{
    io::AsyncReadExt,
    sync::{watch, Notify},
    time::Instant,
};

const REFRESH_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_REFRESH_BACKOFF: Duration = Duration::from_secs(30);

pub struct NextSession {
    pub(super) started_at: chrono::DateTime<chrono::Utc>,
    number: String,
    output: PathBuf,
    deadline: Instant,
    last_access: Mutex<Instant>,
    last_access_utc_ms: AtomicI64,
    last_playback_touch: Mutex<Option<chrono::DateTime<chrono::Utc>>>,
    state: AtomicU8,
    admission: AtomicI32,
    started: AtomicBool,
    cancel: Notify,
    refresh: Notify,
    refresh_request: Mutex<PendingRefresh>,
    publication: watch::Sender<PublicationProgress>,
}

#[derive(Clone, Copy, Default)]
struct RefreshRequest {
    generation: u64,
    deadline: Option<std::time::Instant>,
}

#[derive(Default)]
struct PendingRefresh {
    request: RefreshRequest,
    pending: bool,
}

#[derive(Clone, Copy, Default)]
struct PublicationProgress {
    published: u64,
    failed: u64,
    ended: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionExit {
    Cancelled,
    Idle,
    Exited,
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("Next session has already started")]
    AlreadyStarted,
    #[error(transparent)]
    Process(#[from] ProcessError),
    #[error(transparent)]
    Refresh(#[from] RefreshError),
    #[error("Next session exited unsuccessfully (exit code {0:?})")]
    Exit(Option<i32>),
}

impl NextSession {
    pub(crate) fn set_admission(&self, status: HlsAdmissionStatus) {
        self.admission.store(status.as_i32(), Ordering::Release);
    }
    /// Reserve this handle in the host registry before running its startup.
    /// The host retains capacity admission and removes the handle after `run`
    /// completes, including failures. The output directory must be exclusive.
    #[must_use]
    pub fn new(number: String, output: PathBuf, deadline: Instant) -> Self {
        let started_at = chrono::Utc::now();
        Self {
            started_at,
            number,
            output,
            deadline,
            last_access: Mutex::new(Instant::now()),
            last_access_utc_ms: AtomicI64::new(started_at.timestamp_millis()),
            last_playback_touch: Mutex::new(None),
            state: AtomicU8::new(0),
            admission: AtomicI32::new(0),
            started: AtomicBool::new(false),
            cancel: Notify::new(),
            refresh: Notify::new(),
            refresh_request: Mutex::new(PendingRefresh::default()),
            publication: watch::channel(PublicationProgress::default()).0,
        }
    }

    /// Supervise startup, demand, heartbeat, rebuild publication and process exit.
    /// The host's shutdown path calls `cancel` and awaits this task; it must not
    /// abort it when normal reaping/registry cleanup is required. Refresh work
    /// runs concurrently with heartbeat and cancellation and must use the shared
    /// channel publisher. Notifications coalesce while one refresh is in flight.
    /// Failed refreshes retain playback and retry with capped backoff; each
    /// attempt receives its publication deadline independently of startup.
    /// Coalesced scheduler requests keep the earliest safe deadline; expiration
    /// requires a fresh request rather than extending the old handover window.
    ///
    /// # Errors
    /// Returns redacted startup, retired-channel, process or cleanup failures.
    pub async fn run<S, F, R>(
        &self,
        startup: S,
        idle_timeout: Option<Duration>,
        mut refresh: F,
    ) -> Result<SessionExit, SessionError>
    where
        S: Future<Output = Result<NextProcess, ProcessError>>,
        F: FnMut(std::time::Instant) -> R,
        R: Future<Output = Result<(), RefreshError>>,
    {
        if self.started.swap(true, Ordering::AcqRel) {
            return Err(SessionError::AlreadyStarted);
        }
        let _terminal = TerminalState(self);
        let result = async {
            let mut process = tokio::select! {
                biased;
                () = self.cancel.notified() => return Ok(SessionExit::Cancelled),
                result = tokio::time::timeout_at(self.deadline, startup) => result.map_err(|_| ProcessError::StartupTimeout)??,
            };
            if Instant::now() >= self.deadline {
                process.stop().await?;
                return Err(ProcessError::StartupTimeout.into());
            }
            self.state.store(1, Ordering::Release);
            self.touch();
            let outcome = tokio::select! {
                result = self.monitor(&mut process, idle_timeout) => result,
                result = self.run_refreshes(&mut refresh) => result,
            };
            // Block playlist access before stopping the process that owns it.
            self.state.store(2, Ordering::Release);
            process.stop().await?;
            outcome
        }.await;
        self.state.store(2, Ordering::Release);
        result
    }

    async fn run_refreshes<F, R>(&self, refresh: &mut F) -> Result<SessionExit, SessionError>
    where
        F: FnMut(std::time::Instant) -> R,
        R: Future<Output = Result<(), RefreshError>>,
    {
        loop {
            self.refresh.notified().await;
            let Some(mut request) = self.take_refresh() else {
                continue;
            };
            let mut backoff = Duration::from_secs(1);
            loop {
                let generation = request.generation;
                let deadline = request
                    .deadline
                    .map_or(Instant::now() + REFRESH_TIMEOUT, |limit| {
                        Instant::from_std(limit).min(Instant::now() + REFRESH_TIMEOUT)
                    });
                if Instant::now() >= deadline {
                    self.publication
                        .send_modify(|state| state.failed = generation);
                    break;
                }
                match tokio::time::timeout_at(deadline, refresh(deadline.into_std())).await {
                    Ok(Ok(())) => {
                        self.publication
                            .send_modify(|state| state.published = generation);
                        break;
                    }
                    Ok(Err(error @ RefreshError::Retired)) => return Err(error.into()),
                    Ok(Err(error)) => tracing::warn!(
                        channel_number = %self.number,
                        %error,
                        retry_seconds = backoff.as_secs(),
                        "Next refresh failed; retaining playback and retrying publication"
                    ),
                    Err(_) => tracing::warn!(
                        channel_number = %self.number,
                        retry_seconds = backoff.as_secs(),
                        "Next refresh timed out; retaining playback and retrying publication"
                    ),
                }
                self.publication
                    .send_modify(|state| state.failed = generation);
                // Retrying cannot extend a scheduler's safe handover window.
                // Wait for a new request after it expires, retaining playback.
                if request
                    .deadline
                    .is_some_and(|limit| Instant::now() >= limit.into())
                {
                    break;
                }
                // The monitor still owns cancellation, idle expiry and heartbeat.
                // An already-started blocking publication retains its publisher
                // gate, so a timeout cannot let this retry overtake that writer.
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_REFRESH_BACKOFF);
                // A new request after failure may carry a newly prepared
                // boundary. It cannot bypass the failed attempt's cooldown.
                let _ = self.refresh.notified().now_or_never();
                if let Some(pending) = self.take_refresh() {
                    request = pending;
                }
            }
        }
    }

    fn request_refresh(&self, deadline: Option<std::time::Instant>) -> u64 {
        let mut pending = self
            .refresh_request
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        pending.request.generation += 1;
        pending.request.deadline = if pending.pending {
            // Coalescing an unconstrained notification cannot erase a queued
            // scheduler deadline; multiple waiters keep the earliest one.
            pending.request.deadline.into_iter().chain(deadline).min()
        } else {
            deadline
        };
        pending.pending = true;
        let generation = pending.request.generation;
        self.refresh.notify_one();
        generation
    }

    fn take_refresh(&self) -> Option<RefreshRequest> {
        let mut pending = self
            .refresh_request
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !std::mem::take(&mut pending.pending) {
            return None;
        }
        Some(pending.request)
    }

    async fn await_publication(
        &self,
        deadline: std::time::Instant,
    ) -> Result<(), HlsPublicationError> {
        let mut progress = self.publication.subscribe();
        let required = self.request_refresh(Some(deadline));
        tokio::time::timeout_at(deadline.into(), async {
            loop {
                let state = *progress.borrow_and_update();
                if state.ended {
                    return Err(HlsPublicationError::SessionEnded);
                }
                if state.published >= required {
                    return Ok(());
                }
                if Instant::now() >= deadline.into() {
                    return Err(HlsPublicationError::TimedOut);
                }
                if state.failed >= required {
                    return Err(HlsPublicationError::Failed);
                }
                progress
                    .changed()
                    .await
                    .map_err(|_| HlsPublicationError::SessionEnded)?;
            }
        })
        .await
        .map_err(|_| HlsPublicationError::TimedOut)?
    }

    async fn monitor(
        &self,
        process: &mut NextProcess,
        idle: Option<Duration>,
    ) -> Result<SessionExit, SessionError> {
        let mut heartbeat = tokio::time::interval(Duration::from_secs(5));
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            let last_access = *self
                .last_access
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let idle_wait = async {
                if let Some(timeout) = idle {
                    tokio::time::sleep_until(last_access + timeout).await;
                } else {
                    std::future::pending::<()>().await;
                }
            };
            tokio::select! {
                biased;
                () = self.cancel.notified() => return Ok(SessionExit::Cancelled),
                status = process.wait() => {
                    let status = status?;
                    return if status.success() { Ok(SessionExit::Exited) } else { Err(SessionError::Exit(status.code())) };
                }
                () = idle_wait => {
                    let last_access = *self.last_access.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                    if idle.is_some_and(|timeout| last_access.elapsed() >= timeout) { return Ok(SessionExit::Idle); }
                }
                _ = heartbeat.tick() => process.touch().await?,
            }
        }
    }

    pub(super) fn checkpoint_time(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        *self
            .last_playback_touch
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

struct TerminalState<'a>(&'a NextSession);

impl Drop for TerminalState<'_> {
    fn drop(&mut self) {
        self.0.state.store(2, Ordering::Release);
        self.0.publication.send_modify(|state| state.ended = true);
    }
}

#[async_trait]
impl HlsSessionHandle for NextSession {
    fn channel_number(&self) -> &str {
        &self.number
    }
    fn session_model(&self) -> Option<ersatzrs_core::hls::HlsSessionModel> {
        Some(ersatzrs_core::hls::HlsSessionModel {
            started_at: self.started_at,
            channel_number: self.number.clone(),
            state: "next".to_owned(),
            transcoded_until: None,
            last_access: chrono::DateTime::from_timestamp_millis(
                self.last_access_utc_ms.load(Ordering::Relaxed),
            )?,
        })
    }
    fn touch(&self) {
        self.last_access_utc_ms
            .store(chrono::Utc::now().timestamp_millis(), Ordering::Relaxed);
        *self
            .last_access
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Instant::now();
        if self.state.load(Ordering::Acquire) == 1 {
            *self
                .last_playback_touch
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(chrono::Utc::now());
        }
    }
    fn startup_readiness(&self) -> HlsStartupReadiness {
        match self.state.load(Ordering::Acquire) {
            1 => HlsStartupReadiness::Ready,
            0 if Instant::now() < self.deadline => HlsStartupReadiness::Pending {
                deadline: self.deadline.into_std(),
            },
            _ => HlsStartupReadiness::Failed,
        }
    }
    fn admission_status(&self) -> HlsAdmissionStatus {
        if self.state.load(Ordering::Acquire) == 1 {
            HlsAdmissionStatus::Transcoding
        } else {
            HlsAdmissionStatus::from_i32(self.admission.load(Ordering::Acquire))
        }
    }
    async fn trim_playlist(&self) -> Option<String> {
        const LIMIT: u64 = 256 * 1024;
        if !matches!(self.startup_readiness(), HlsStartupReadiness::Ready) {
            return None;
        }
        let mut file = tokio::fs::File::open(self.output.join("live.m3u8"))
            .await
            .ok()?
            .take(LIMIT + 1);
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).await.ok()?;
        if bytes.len() > usize::try_from(LIMIT).ok()? {
            return None;
        }
        let text = String::from_utf8(bytes).ok()?;
        if !text.starts_with("#EXTM3U")
            || !matches!(self.startup_readiness(), HlsStartupReadiness::Ready)
        {
            return None;
        }
        Some(text)
    }
    async fn cancel(&self) {
        self.cancel.notify_one();
    }
    fn playout_updated(&self) {
        self.request_refresh(None);
    }

    async fn publish_playout_update(
        &self,
        deadline: std::time::Instant,
    ) -> Result<(), HlsPublicationError> {
        self.await_publication(deadline).await
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "session_publication_tests.rs"]
mod publication_tests;
