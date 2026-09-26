//! Next lifecycle behind the host's existing HLS session interface.

use super::{
    coordinator::RefreshError,
    process::{NextProcess, ProcessError},
};
use async_trait::async_trait;
use ersatzrs_core::hls::{HlsAdmissionStatus, HlsSessionHandle, HlsStartupReadiness};
use std::{
    future::Future,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicI32, AtomicU8, Ordering},
        Mutex,
    },
    time::Duration,
};
use tokio::{io::AsyncReadExt, sync::Notify, time::Instant};

pub struct NextSession {
    number: String,
    output: PathBuf,
    deadline: Instant,
    last_access: Mutex<Instant>,
    last_playback_touch: Mutex<Option<chrono::DateTime<chrono::Utc>>>,
    state: AtomicU8,
    admission: AtomicI32,
    started: AtomicBool,
    cancel: Notify,
    refresh: Notify,
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
        Self {
            number,
            output,
            deadline,
            last_access: Mutex::new(Instant::now()),
            last_playback_touch: Mutex::new(None),
            state: AtomicU8::new(0),
            admission: AtomicI32::new(0),
            started: AtomicBool::new(false),
            cancel: Notify::new(),
            refresh: Notify::new(),
        }
    }

    /// Supervise startup, demand, heartbeat, rebuild publication and process exit.
    /// The host's shutdown path calls `cancel` and awaits this task; it must not
    /// abort it when normal reaping/registry cleanup is required. Refresh work
    /// runs concurrently with heartbeat and cancellation and must use the shared
    /// channel publisher. Notifications coalesce while one refresh is in flight.
    ///
    /// # Errors
    /// Returns redacted startup, publication, process or cleanup failures.
    pub async fn run<S, F, R>(
        &self,
        startup: S,
        idle_timeout: Option<Duration>,
        mut refresh: F,
    ) -> Result<SessionExit, SessionError>
    where
        S: Future<Output = Result<NextProcess, ProcessError>>,
        F: FnMut() -> R,
        R: Future<Output = Result<(), RefreshError>>,
    {
        if self.started.swap(true, Ordering::AcqRel) {
            return Err(SessionError::AlreadyStarted);
        }
        let _terminal = TerminalState(&self.state);
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
            let refreshes = async {
                loop {
                    self.refresh.notified().await;
                    if let Err(error) = refresh().await {
                        return Err::<SessionExit, SessionError>(error.into());
                    }
                }
            };
            let outcome = tokio::select! {
                result = self.monitor(&mut process, idle_timeout) => result,
                result = refreshes => result,
            };
            // Block playlist access before stopping the process that owns it.
            self.state.store(2, Ordering::Release);
            process.stop().await?;
            outcome
        }.await;
        self.state.store(2, Ordering::Release);
        result
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

struct TerminalState<'a>(&'a AtomicU8);

impl Drop for TerminalState<'_> {
    fn drop(&mut self) {
        self.0.store(2, Ordering::Release);
    }
}

#[async_trait]
impl HlsSessionHandle for NextSession {
    fn channel_number(&self) -> &str {
        &self.number
    }
    fn touch(&self) {
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
        self.refresh.notify_one();
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
