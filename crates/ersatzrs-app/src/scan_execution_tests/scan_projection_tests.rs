//! Row292: projection notifications remain inside the scan completion gate.
use super::*;
use crate::{ChannelRuntime, ChannelRuntimeError};
use ersatzrs_domain::PlayoutProjectionChange;

#[derive(Default)]
pub(crate) struct ProjectionRuntime {
    dependents: Vec<String>,
    lookups: Mutex<Vec<PlayoutProjectionChange>>,
    notifications: Mutex<Vec<Vec<String>>>,
    fail_lookup: bool,
    fail_notify: bool,
    running: Option<(crate::operations::ScanOperationRegistry, i64)>,
}

#[async_trait]
impl ChannelRuntime for ProjectionRuntime {
    async fn active_channel_numbers(&self) -> Result<Vec<String>, ChannelRuntimeError> {
        panic!("no session enumeration")
    }
    async fn session_models(
        &self,
    ) -> Result<Vec<ersatzrs_core::hls::HlsSessionModel>, ChannelRuntimeError> {
        panic!("no session inspection")
    }
    async fn stop_channel_session(&self, _: &str) -> Result<bool, ChannelRuntimeError> {
        panic!("no stopping playback")
    }
    fn request_subtitle_extraction(&self, _: Vec<i64>) {
        panic!("no separate extraction")
    }
    async fn refresh_profile_playouts(&self, _: i64) -> Result<(), ChannelRuntimeError> {
        panic!("no profile refresh")
    }
    async fn playout_projection_dependents(
        &self,
        change: PlayoutProjectionChange,
    ) -> Result<Vec<String>, ChannelRuntimeError> {
        if let Some((registry, id)) = &self.running {
            assert_eq!(
                registry.library_status(GetLibraryScanStatus::new(*id)),
                LibraryScanStatus::Running
            );
        }
        self.lookups.lock().unwrap().push(change);
        if self.fail_lookup {
            return Err(ChannelRuntimeError::unavailable("synthetic lookup failure"));
        }
        Ok(self.dependents.clone())
    }
    async fn refresh_playout_projections(
        &self,
        channels: Vec<String>,
    ) -> Result<(), ChannelRuntimeError> {
        if let Some((registry, id)) = &self.running {
            assert_eq!(
                registry.library_status(GetLibraryScanStatus::new(*id)),
                LibraryScanStatus::Running
            );
        }
        self.notifications.lock().unwrap().push(channels);
        if self.fail_notify {
            return Err(ChannelRuntimeError::unavailable(
                "synthetic notification failure",
            ));
        }
        Ok(())
    }
}

async fn execute(
    runtime: &dyn ScanRuntime,
    projections: &ProjectionRuntime,
    queue: &dyn PlayoutBuildQueue,
    request: ScanRequest,
) -> (ExecuteScanOutcome, OperationCoordinator) {
    let operations = coordinator();
    let work = admitted(&operations, request);
    let result = crate::execute_scan_work_with_runtime_and_claim(
        work,
        runtime,
        queue,
        projections,
        &operations,
        None,
    )
    .await;
    (result, operations)
}

#[tokio::test]
async fn row292_provider_library_and_show_scans_notify_before_completion() {
    for provider in [
        MediaSourceKind::Local,
        MediaSourceKind::Plex,
        MediaSourceKind::Jellyfin,
        MediaSourceKind::Emby,
    ] {
        for deep_scan in [false, true] {
            let operations = coordinator();
            let projections = ProjectionRuntime {
                dependents: vec!["1".into(), "5".into()],
                running: Some((operations.scan_activity(), 71)),
                ..Default::default()
            };
            let runtime = FakeRuntime::default();
            let queue = RecordingPlayoutQueue::default();
            let request = ScanRequest::Library {
                provider,
                library_id: 71,
                deep_scan,
            };
            let work = admitted(&operations, request);
            assert_eq!(
                crate::execute_scan_work_with_runtime_and_claim(
                    work,
                    &runtime,
                    &queue,
                    &projections,
                    &operations,
                    None
                )
                .await,
                ExecuteScanOutcome::Completed
            );
            assert_eq!(
                *projections.lookups.lock().unwrap(),
                [PlayoutProjectionChange::Library(71)]
            );
            assert_eq!(
                *projections.notifications.lock().unwrap(),
                [vec!["1".to_string(), "5".to_string()]]
            );
            assert!(queue.requests.lock().unwrap().is_empty());
            if provider != MediaSourceKind::Local {
                let request = ScanRequest::Show {
                    provider,
                    library_id: 71,
                    show_id: 81,
                    deep_scan,
                };
                let work = admitted(&operations, request);
                assert_eq!(
                    crate::execute_scan_work_with_runtime_and_claim(
                        work,
                        &runtime,
                        &queue,
                        &projections,
                        &operations,
                        None
                    )
                    .await,
                    ExecuteScanOutcome::Completed
                );
                assert_eq!(projections.notifications.lock().unwrap().len(), 2);
            }
        }
    }
}

#[tokio::test]
async fn row292_failed_scan_cleanup_or_scheduler_admission_never_notifies() {
    let projections = ProjectionRuntime::default();
    let request = ScanRequest::Library {
        provider: MediaSourceKind::Local,
        library_id: 71,
        deep_scan: false,
    };
    let queue = RecordingPlayoutQueue::default();
    assert!(matches!(
        execute(&FakeRuntime::failing(), &projections, &queue, request)
            .await
            .0,
        ExecuteScanOutcome::Failed { .. }
    ));
    let unconfirmed = UnconfirmedRuntime {
        inner: FakeRuntime::default(),
        fail_subtitles: false,
    };
    assert_eq!(
        execute(&unconfirmed, &projections, &queue, request).await.0,
        ExecuteScanOutcome::CleanupUnconfirmed
    );
    let runtime = FakeRuntime::reporting(vec![ScanPlayoutRefresh {
        playout_id: 1,
        reason: ScanPlayoutRefreshReason::RemoteStreamMembershipRemoved,
    }]);
    assert!(matches!(
        execute(&runtime, &projections, &UnavailablePlayoutQueue, request)
            .await
            .0,
        ExecuteScanOutcome::Failed { .. }
    ));
    assert!(projections.lookups.lock().unwrap().is_empty());
    assert!(projections.notifications.lock().unwrap().is_empty());
}

#[tokio::test]
async fn row292_projection_failures_remain_observable_and_empty_libraries_complete() {
    for (lookup, notify) in [(true, false), (false, true), (false, false)] {
        let projections = ProjectionRuntime {
            fail_lookup: lookup,
            fail_notify: notify,
            dependents: if lookup || notify {
                vec!["1".into()]
            } else {
                Vec::new()
            },
            ..Default::default()
        };
        let request = ScanRequest::Library {
            provider: MediaSourceKind::Jellyfin,
            library_id: 71,
            deep_scan: false,
        };
        let (result, operations) = execute(
            &FakeRuntime::default(),
            &projections,
            &RecordingPlayoutQueue::default(),
            request,
        )
        .await;
        assert_eq!(
            matches!(result, ExecuteScanOutcome::Failed { .. }),
            lookup || notify
        );
        let status = operations
            .scan_activity()
            .library_status(GetLibraryScanStatus::new(71));
        assert_eq!(
            matches!(status, LibraryScanStatus::Completed),
            !lookup && !notify
        );
        assert_eq!(
            projections.notifications.lock().unwrap().len(),
            usize::from(notify)
        );
    }
}
