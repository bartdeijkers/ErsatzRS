//! Storage- and transport-neutral application services.
//!
//! Commands and queries enter through this crate, which coordinates core
//! ports without depending on SQL, HTTP, scheduler implementations, `FFmpeg`,
//! or platform adapters. The native web host constructs [`ApplicationServices`]
//! from concrete infrastructure ports.

pub mod commands;
pub mod error;
pub mod operations;
pub mod ports;
pub mod queries;

pub use error::{ApplicationError, Result};
pub use operations::OperationCoordinator;
pub use ports::{
    ApplicationPorts, PlayoutBuildQueue, PlayoutBuildQueueError, ScanRuntime, ScanRuntimeError,
    ScanWorkQueue, ScanWorkQueueError,
};

/// Native application services assembled by a composition root.
///
/// The port set is intentionally limited to the playout and library
/// repositories required by the immediately following Phase 7B vertical
/// slices. Additional ports belong beside the use case that first needs them.
#[derive(Clone)]
pub struct ApplicationServices {
    ports: ApplicationPorts,
    operations: OperationCoordinator,
}

impl ApplicationServices {
    /// Construct the application container from injected core ports and the
    /// process-wide operation coordinator.
    #[must_use]
    pub fn new(ports: ApplicationPorts, operations: OperationCoordinator) -> Self {
        Self { ports, operations }
    }

    /// Core repository ports used by application use cases.
    #[must_use]
    pub const fn ports(&self) -> &ApplicationPorts {
        &self.ports
    }

    /// Shared process-local coordination for long-running operations.
    #[must_use]
    pub const fn operations(&self) -> &OperationCoordinator {
        &self.operations
    }

    /// Admit one playout build through the single application-owned queue
    /// boundary.
    #[must_use]
    pub fn request_playout_build(
        &self,
        command: commands::RequestPlayoutBuild,
    ) -> commands::RequestPlayoutBuildOutcome {
        commands::request_playout_build(command, self.ports.playout_builds(), &self.operations)
    }

    /// Validate and admit one full library scan.
    pub async fn request_library_scan(
        &self,
        command: commands::RequestLibraryScan,
    ) -> commands::RequestScanOutcome {
        commands::request_library_scan(
            command,
            self.ports.libraries(),
            self.ports.scan_work(),
            &self.operations,
        )
        .await
    }

    /// Validate and admit one targeted show scan.
    pub async fn request_show_scan(
        &self,
        command: commands::RequestShowScan,
    ) -> commands::RequestScanOutcome {
        commands::request_show_scan(
            command,
            self.ports.libraries(),
            self.ports.scan_work(),
            &self.operations,
        )
        .await
    }

    /// Execute one accepted queue token through the injected native runtime.
    pub async fn execute_scan_work(
        &self,
        work: commands::ScanWorkRequest,
    ) -> commands::ExecuteScanOutcome {
        execute_scan_work_with_runtime(work, self.ports.scan_runtime(), &self.operations).await
    }

    /// Mark the native scan worker available for new commands.
    pub fn scan_worker_started(&self) {
        self.operations.scan_activity().worker_started();
    }

    /// Fail outstanding work and reject admission after worker shutdown.
    pub fn scan_worker_stopped(&self, message: impl Into<String>) {
        self.operations.scan_activity().worker_stopped(message);
    }

    /// Query every retained scan operation.
    #[must_use]
    pub fn scan_operations(&self) -> Vec<queries::ScanOperationSnapshot> {
        self.operations.scan_activity().snapshots()
    }

    /// Query distinct libraries with queued or running scan work.
    #[must_use]
    pub fn active_library_scans(&self) -> Vec<ersatzrs_domain::EntityId> {
        self.operations.scan_activity().active_library_ids()
    }

    /// Query active fractional progress by library.
    #[must_use]
    pub fn scan_progress(&self) -> Vec<(ersatzrs_domain::EntityId, f32)> {
        self.operations.scan_activity().progress_by_library()
    }

    /// Query one library's aggregate active or latest terminal state.
    #[must_use]
    pub fn library_scan_status(
        &self,
        query: queries::GetLibraryScanStatus,
    ) -> queries::LibraryScanStatus {
        self.operations.scan_activity().library_status(query)
    }
}

struct ScanProgressReporter {
    registry: operations::ScanOperationRegistry,
    operation_id: commands::ScanOperationId,
}

impl ersatzrs_domain::ScanProgressSink for ScanProgressReporter {
    fn report(&self, fraction: f32) {
        self.registry.report_progress(self.operation_id, fraction);
    }
}

#[async_trait::async_trait]
impl ersatzrs_core::maintenance::ScheduledLibraryScanRequester for ApplicationServices {
    async fn request_scheduled_library_scan(
        &self,
        library_id: ersatzrs_domain::EntityId,
    ) -> ersatzrs_core::maintenance::ScheduledScanRequestOutcome {
        use ersatzrs_core::maintenance::ScheduledScanRequestOutcome;

        match commands::request_scheduled_library_scan(
            commands::RequestScheduledLibraryScan::new(library_id),
            self.ports.libraries(),
            self.ports.scan_work(),
            &self.operations,
        )
        .await
        {
            commands::RequestScheduledScanOutcome::Accepted { .. } => {
                ScheduledScanRequestOutcome::Accepted
            }
            commands::RequestScheduledScanOutcome::AlreadyActive => {
                ScheduledScanRequestOutcome::AlreadyActive
            }
            commands::RequestScheduledScanOutcome::Rejected(reason) => {
                ScheduledScanRequestOutcome::Rejected {
                    message: format!("{reason:?}"),
                }
            }
            commands::RequestScheduledScanOutcome::Unavailable { message } => {
                ScheduledScanRequestOutcome::Unavailable { message }
            }
        }
    }
}

async fn execute_scan_work_with_runtime(
    work: commands::ScanWorkRequest,
    runtime: &dyn ScanRuntime,
    operations: &OperationCoordinator,
) -> commands::ExecuteScanOutcome {
    let registry = operations.scan_activity();
    let Some(guard) = registry.start_guard(work.operation_id) else {
        return commands::ExecuteScanOutcome::Missing;
    };
    let progress = ScanProgressReporter {
        registry,
        operation_id: work.operation_id,
    };
    let result = match work.request {
        commands::ScanRequest::Library {
            provider,
            library_id,
            deep_scan,
        } => {
            runtime
                .scan_library(provider, library_id, deep_scan, &progress)
                .await
        }
        commands::ScanRequest::Show {
            provider,
            library_id,
            show_id,
            deep_scan,
        } => {
            let result = runtime
                .scan_show(provider, library_id, show_id, deep_scan, &progress)
                .await;
            if result.is_ok() && deep_scan {
                runtime.extract_embedded_show_subtitles(show_id).await
            } else {
                result
            }
        }
    };

    match result {
        Ok(()) => {
            guard.complete();
            commands::ExecuteScanOutcome::Completed
        }
        Err(error) => {
            let message = error.to_string();
            guard.fail(message.clone());
            commands::ExecuteScanOutcome::Failed { message }
        }
    }
}

impl std::fmt::Debug for ApplicationServices {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ApplicationServices")
            .field("ports", &self.ports)
            .field("operations", &self.operations)
            .finish()
    }
}

#[cfg(test)]
mod scan_execution_tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use ersatzrs_core::locking::{EntityLocker, PlayoutBuildActivity};
    use ersatzrs_domain::{EntityId, MediaSourceKind, ScanProgressSink};

    use super::{
        execute_scan_work_with_runtime, OperationCoordinator, ScanRuntime, ScanRuntimeError,
    };
    use crate::commands::{ExecuteScanOutcome, ScanRequest, ScanWorkRequest};
    use crate::queries::{GetLibraryScanStatus, LibraryScanStatus, ScanOperationState};

    #[derive(Default)]
    struct FakeRuntime {
        calls: Mutex<Vec<String>>,
        fail_scan: bool,
    }

    impl FakeRuntime {
        fn failing() -> Self {
            Self {
                calls: Mutex::default(),
                fail_scan: true,
            }
        }
    }

    #[async_trait]
    impl ScanRuntime for FakeRuntime {
        async fn scan_library(
            &self,
            provider: MediaSourceKind,
            library_id: EntityId,
            deep_scan: bool,
            progress: &dyn ScanProgressSink,
        ) -> Result<(), ScanRuntimeError> {
            self.calls
                .lock()
                .expect("calls")
                .push(format!("library:{provider:?}:{library_id}:{deep_scan}"));
            progress.report(-1.0);
            progress.report(0.5);
            progress.report(2.0);
            if self.fail_scan {
                Err(ScanRuntimeError::new("library failed"))
            } else {
                Ok(())
            }
        }

        async fn scan_show(
            &self,
            provider: MediaSourceKind,
            library_id: EntityId,
            show_id: EntityId,
            deep_scan: bool,
            progress: &dyn ScanProgressSink,
        ) -> Result<(), ScanRuntimeError> {
            self.calls.lock().expect("calls").push(format!(
                "show:{provider:?}:{library_id}:{show_id}:{deep_scan}"
            ));
            progress.report(0.75);
            if self.fail_scan {
                Err(ScanRuntimeError::new("show failed"))
            } else {
                Ok(())
            }
        }

        async fn extract_embedded_show_subtitles(
            &self,
            show_id: EntityId,
        ) -> Result<(), ScanRuntimeError> {
            self.calls
                .lock()
                .expect("calls")
                .push(format!("subtitles:{show_id}"));
            Ok(())
        }
    }

    fn coordinator() -> OperationCoordinator {
        OperationCoordinator::new(EntityLocker::default(), PlayoutBuildActivity::default())
    }

    fn admitted(operations: &OperationCoordinator, request: ScanRequest) -> ScanWorkRequest {
        let mut work = None;
        operations
            .scan_activity()
            .admit(request, |token| {
                work = Some(token);
                Ok::<(), ()>(())
            })
            .expect("accepted");
        work.expect("queue token")
    }

    #[tokio::test]
    async fn worker_execution_records_running_progress_and_success_cleanup() {
        let operations = coordinator();
        let runtime = FakeRuntime::default();
        let work = admitted(
            &operations,
            ScanRequest::Library {
                provider: MediaSourceKind::Plex,
                library_id: 20,
                deep_scan: true,
            },
        );

        assert_eq!(
            execute_scan_work_with_runtime(work, &runtime, &operations).await,
            ExecuteScanOutcome::Completed
        );
        assert_eq!(
            operations
                .scan_activity()
                .library_status(GetLibraryScanStatus::new(20)),
            LibraryScanStatus::Completed
        );
        let snapshot = operations.scan_activity().snapshots().remove(0);
        assert!(matches!(snapshot.state, ScanOperationState::Completed));
        assert_eq!(snapshot.progress, None);
        assert_eq!(
            runtime.calls.lock().expect("calls").as_slice(),
            ["library:Plex:20:true"]
        );
    }

    #[tokio::test]
    async fn failed_execution_remains_observable_without_active_progress() {
        let operations = coordinator();
        let runtime = FakeRuntime::failing();
        let work = admitted(
            &operations,
            ScanRequest::Library {
                provider: MediaSourceKind::Emby,
                library_id: 21,
                deep_scan: false,
            },
        );

        assert_eq!(
            execute_scan_work_with_runtime(work, &runtime, &operations).await,
            ExecuteScanOutcome::Failed {
                message: "library failed".to_owned(),
            }
        );
        assert!(operations.scan_activity().progress_by_library().is_empty());
        assert!(matches!(
            operations
                .scan_activity()
                .library_status(GetLibraryScanStatus::new(21)),
            LibraryScanStatus::Failed { ref message } if message == "library failed"
        ));
    }

    #[tokio::test]
    async fn deep_show_extracts_subtitles_only_after_success() {
        let operations = coordinator();
        let runtime = FakeRuntime::default();
        let successful = admitted(
            &operations,
            ScanRequest::Show {
                provider: MediaSourceKind::Jellyfin,
                library_id: 22,
                show_id: 220,
                deep_scan: true,
            },
        );
        assert_eq!(
            execute_scan_work_with_runtime(successful, &runtime, &operations).await,
            ExecuteScanOutcome::Completed
        );
        assert_eq!(
            runtime.calls.lock().expect("calls").as_slice(),
            ["show:Jellyfin:22:220:true", "subtitles:220"]
        );

        let failing_runtime = FakeRuntime::failing();
        let failed = admitted(
            &operations,
            ScanRequest::Show {
                provider: MediaSourceKind::Plex,
                library_id: 23,
                show_id: 230,
                deep_scan: true,
            },
        );
        assert!(matches!(
            execute_scan_work_with_runtime(failed, &failing_runtime, &operations).await,
            ExecuteScanOutcome::Failed { .. }
        ));
        assert_eq!(
            failing_runtime.calls.lock().expect("calls").as_slice(),
            ["show:Plex:23:230:true"]
        );
    }
}
