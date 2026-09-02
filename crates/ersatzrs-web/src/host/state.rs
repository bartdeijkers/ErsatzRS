use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicI32, Ordering},
    Arc,
};

use ersatzrs_app::{
    ApplicationPorts, ApplicationServices, OperationCoordinator, PlayoutBuildQueue,
    PlayoutBuildQueueError,
};
use ersatzrs_core::hls::HlsSessionRegistry;
use ersatzrs_core::scheduling::PlayoutBuildRequest;
use ersatzrs_ffmpeg::FfmpegRuntimeReport;
use ersatzrs_infra::db::{DatabaseBackend, DatabaseSchemaStatus, DbPool};
use ersatzrs_infra::hls::InMemoryHlsSessionRegistry;
use ersatzrs_infra::repositories::{make_repositories, Repositories};
use jsonwebtoken::{DecodingKey, EncodingKey, Validation};
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

use super::addons::NativeAddonRuntime;
use super::channels::NativeChannelRuntime;
use super::health::NativeAdminHealthRuntime;
use super::media_sources::NativeMediaSourceRuntime;
use super::remote_stream_playlists::NativeRemoteStreamPlaylistRuntime;
use super::scanning::{NativeScanRuntime, TokioScanWorkQueue};
use super::settings::NativeSettingsRuntime;
use super::troubleshooting::NativeAdminTroubleshootingRuntime;

#[derive(Clone)]
struct TokioPlayoutBuildQueue {
    sender: mpsc::UnboundedSender<PlayoutBuildRequest>,
}

impl PlayoutBuildQueue for TokioPlayoutBuildQueue {
    fn enqueue(&self, request: PlayoutBuildRequest) -> Result<(), PlayoutBuildQueueError> {
        self.sender
            .send(request)
            .map_err(|error| PlayoutBuildQueueError::unavailable(error.to_string()))
    }
}

#[derive(Clone, Debug)]
pub struct SystemStartup {
    pub database_ready: bool,
    pub schema_status: DatabaseSchemaStatus,
}

impl SystemStartup {
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.database_ready && self.schema_status.is_ready()
    }

    #[must_use]
    pub fn health_issue_counts(&self) -> (u32, u32) {
        let errors = u32::from(!self.database_ready) + self.schema_status.failure_count();
        (errors, 0)
    }
}

#[derive(Clone)]
pub struct AppState {
    /// Storage-neutral application services built from native infrastructure
    /// ports by this composition root.
    pub application: Arc<ApplicationServices>,
    pub repositories: Arc<Repositories>,
    pub pool: DbPool,
    pub database_backend: DatabaseBackend,
    pub system_startup: Arc<SystemStartup>,
    pub ffmpeg_runtime: Arc<RwLock<FfmpegRuntimeReport>>,
    pub device_uuid: Uuid,
    pub tuner_count: Arc<AtomicI32>,
    pub hls_sessions: Arc<dyn HlsSessionRegistry>,
    pub preview_hls_sessions: Arc<dyn HlsSessionRegistry>,
    pub hls_direct_indices: Arc<RwLock<HashMap<i64, (String, u64)>>>,
    pub iptv_access_token_verifier: IptvAccessTokenVerifier,
}

#[derive(Clone)]
pub struct IptvAccessTokenVerifier {
    pub(crate) validation: Option<Validation>,
    pub(crate) decoding_key: Option<DecodingKey>,
    pub(crate) encoding_key: Option<EncodingKey>,
}

impl AppState {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        pool: DbPool,
        database_backend: DatabaseBackend,
        system_startup: SystemStartup,
        ffmpeg_runtime: FfmpegRuntimeReport,
        device_uuid: Uuid,
        tuner_count: i32,
        iptv_access_token_verifier: IptvAccessTokenVerifier,
        playout_build_tx: mpsc::UnboundedSender<PlayoutBuildRequest>,
        playout_locker: ersatzrs_core::locking::EntityLocker,
    ) -> Self {
        let (scan_work_tx, _scan_work_rx) = mpsc::unbounded_channel();
        Self::new_with_scan_queue(
            pool,
            database_backend,
            system_startup,
            ffmpeg_runtime,
            device_uuid,
            tuner_count,
            iptv_access_token_verifier,
            playout_build_tx,
            scan_work_tx,
            playout_locker,
        )
    }

    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new_with_scan_queue(
        pool: DbPool,
        database_backend: DatabaseBackend,
        system_startup: SystemStartup,
        ffmpeg_runtime: FfmpegRuntimeReport,
        device_uuid: Uuid,
        tuner_count: i32,
        iptv_access_token_verifier: IptvAccessTokenVerifier,
        playout_build_tx: mpsc::UnboundedSender<PlayoutBuildRequest>,
        scan_work_tx: mpsc::UnboundedSender<ersatzrs_app::commands::ScanWorkRequest>,
        playout_locker: ersatzrs_core::locking::EntityLocker,
    ) -> Self {
        let repositories = Arc::new(make_repositories(pool.clone()));
        let hls_sessions: Arc<dyn HlsSessionRegistry> = Arc::new(InMemoryHlsSessionRegistry::new());
        let preview_hls_sessions: Arc<dyn HlsSessionRegistry> =
            Arc::new(InMemoryHlsSessionRegistry::new());
        let ffmpeg_runtime = Arc::new(RwLock::new(ffmpeg_runtime));
        let tuner_count = Arc::new(AtomicI32::new(tuner_count));
        let (startup_errors, startup_warnings) = system_startup.health_issue_counts();
        let application = Arc::new(ApplicationServices::new(
            ApplicationPorts::new(
                Arc::new(repositories.channel.clone()),
                Arc::new(NativeChannelRuntime::new(
                    pool.clone(),
                    Arc::clone(&hls_sessions),
                    Arc::clone(&preview_hls_sessions),
                )),
                Arc::new(repositories.addon_media_lists.clone()),
                Arc::new(repositories.admin_settings.clone()),
                Arc::new(NativeSettingsRuntime::new(
                    pool.clone(),
                    Arc::clone(&ffmpeg_runtime),
                    Arc::clone(&tuner_count),
                )),
                Arc::new(repositories.collection.clone()),
                Arc::new(repositories.saved_search.clone()),
                Arc::new(repositories.ffmpeg_profile.clone()),
                Arc::new(repositories.filler.clone()),
                Arc::new(repositories.graphics.clone()),
                Arc::new(repositories.watermark.clone()),
                Arc::new(repositories.template.clone()),
                Arc::new(repositories.block.clone()),
                Arc::new(repositories.deco.clone()),
                Arc::new(repositories.program_schedule.clone()),
                Arc::new(repositories.playout.clone()),
                Arc::new(repositories.library.clone()),
                Arc::new(repositories.managed_library_operations.clone()),
                Arc::new(repositories.media_source.clone()),
                Arc::new(repositories.media_item.clone()),
                Arc::new(repositories.admin_media.clone()),
                Arc::new(repositories.television.clone()),
                Arc::new(NativeAdminHealthRuntime::new(
                    pool.clone(),
                    Arc::clone(&ffmpeg_runtime),
                    system_startup.schema_status.clone(),
                    startup_errors,
                    startup_warnings,
                )),
                Arc::new(repositories.troubleshooting.clone()),
                Arc::new(NativeAdminTroubleshootingRuntime::new(
                    pool.clone(),
                    Arc::clone(&repositories),
                    Arc::clone(&ffmpeg_runtime),
                )),
                Arc::new(NativeRemoteStreamPlaylistRuntime::new(pool.clone())),
                Arc::new(NativeAddonRuntime::new(pool.clone())),
                Arc::new(NativeMediaSourceRuntime::new(pool.clone())),
                Arc::new(repositories.scheduling.clone()),
                Arc::new(repositories.scheduling.clone()),
                Arc::new(ersatzrs_core::playout_builder::PlayoutBuilder::default()),
                Arc::new(TokioPlayoutBuildQueue {
                    sender: playout_build_tx,
                }),
                Arc::new(TokioScanWorkQueue::new(scan_work_tx)),
                Arc::new(NativeScanRuntime::new(pool.clone())),
            ),
            OperationCoordinator::new(
                playout_locker,
                ersatzrs_core::locking::PlayoutBuildActivity::default(),
            ),
        ));
        let hls_direct_indices = Arc::new(RwLock::new(HashMap::new()));
        Self {
            application,
            repositories,
            pool,
            database_backend,
            system_startup: Arc::new(system_startup),
            ffmpeg_runtime,
            device_uuid,
            tuner_count,
            hls_sessions,
            preview_hls_sessions,
            hls_direct_indices,
            iptv_access_token_verifier,
        }
    }

    #[cfg(test)]
    #[must_use]
    pub fn for_tests(iptv_access_token_verifier: IptvAccessTokenVerifier) -> Self {
        let pool =
            ersatzrs_infra::db::lazy_sqlite_pool("sqlite::memory:").expect("lazy sqlite pool");
        let (playout_build_tx, _playout_build_rx) = mpsc::unbounded_channel();

        Self::new(
            pool,
            DatabaseBackend::Sqlite,
            SystemStartup {
                database_ready: true,
                schema_status: DatabaseSchemaStatus::default(),
            },
            FfmpegRuntimeReport::for_tests(),
            Uuid::nil(),
            2,
            iptv_access_token_verifier,
            playout_build_tx,
            ersatzrs_core::locking::EntityLocker::default(),
        )
    }

    #[must_use]
    pub async fn health_issue_counts(&self) -> (u32, u32) {
        let (startup_errors, startup_warnings) = self.system_startup.health_issue_counts();
        let (ffmpeg_errors, ffmpeg_warnings) = self.ffmpeg_runtime.read().await.issue_counts();
        (
            startup_errors.saturating_add(ffmpeg_errors),
            startup_warnings.saturating_add(ffmpeg_warnings),
        )
    }

    #[must_use]
    pub fn tuner_count(&self) -> i32 {
        self.tuner_count.load(Ordering::Relaxed)
    }

    pub fn set_tuner_count(&self, tuner_count: i32) {
        self.tuner_count.store(tuner_count, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::{AppState, IptvAccessTokenVerifier, SystemStartup};
    use ersatzrs_infra::db::DatabaseSchemaStatus;
    use std::sync::Arc;

    #[test]
    fn startup_is_ready_when_database_and_schema_are_clean() {
        let startup = SystemStartup {
            database_ready: true,
            schema_status: DatabaseSchemaStatus::default(),
        };

        assert!(startup.is_ready());
        assert_eq!(startup.health_issue_counts(), (0, 0));
    }

    #[tokio::test]
    async fn app_state_tuner_count_updates_in_place() {
        let state = AppState::for_tests(IptvAccessTokenVerifier::disabled());
        assert_eq!(state.tuner_count(), 2);

        state.set_tuner_count(4);

        assert_eq!(state.tuner_count(), 4);
    }

    #[tokio::test]
    async fn app_state_constructs_concrete_application_ports_and_shared_operations() {
        let state = AppState::for_tests(IptvAccessTokenVerifier::disabled());

        assert!(matches!(
            state
                .application
                .ports()
                .libraries()
                .get_with_paths(1)
                .await,
            Err(ersatzrs_core::repositories::RepositoryError::Unavailable { .. })
        ));

        let locker = state.application.operations().entity_locker();
        assert!(locker.lock_playout(12));
        assert_eq!(state.application.operations().active_playout_builds(), [12]);
        assert!(!Arc::ptr_eq(
            &state.hls_sessions,
            &state.preview_hls_sessions
        ));
    }

    #[test]
    fn startup_health_counts_schema_issue_categories() {
        let startup = SystemStartup {
            database_ready: true,
            schema_status: DatabaseSchemaStatus {
                dirty_version: Some(42),
                unknown_migrations: vec![43, 44],
                mismatched_migrations: Vec::new(),
            },
        };

        assert!(!startup.is_ready());
        assert_eq!(startup.health_issue_counts(), (2, 0));
    }
}
