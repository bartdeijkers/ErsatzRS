use std::collections::HashMap;
use std::sync::Arc;

use ersatzrs_core::hls::HlsSessionRegistry;
use ersatzrs_ffmpeg::FfmpegRuntimeReport;
use ersatzrs_infra::db::{DatabaseBackend, DatabaseSchemaStatus, DbPool};
use ersatzrs_infra::hls::InMemoryHlsSessionRegistry;
use ersatzrs_infra::repositories::{make_repositories, Repositories};
use jsonwebtoken::{DecodingKey, Validation};
use tokio::sync::RwLock;
use uuid::Uuid;

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
    pub repositories: Arc<Repositories>,
    pub pool: DbPool,
    pub database_backend: DatabaseBackend,
    pub system_startup: Arc<SystemStartup>,
    pub ffmpeg_runtime: Arc<FfmpegRuntimeReport>,
    pub device_uuid: Uuid,
    pub tuner_count: i32,
    pub hls_sessions: Arc<dyn HlsSessionRegistry>,
    pub hls_direct_indices: Arc<RwLock<HashMap<i64, (String, u64)>>>,
    pub iptv_access_token_verifier: IptvAccessTokenVerifier,
}

#[derive(Clone)]
pub struct IptvAccessTokenVerifier {
    pub(crate) validation: Option<Validation>,
    pub(crate) decoding_key: Option<DecodingKey>,
}

impl AppState {
    #[must_use]
    pub fn new(
        pool: DbPool,
        database_backend: DatabaseBackend,
        system_startup: SystemStartup,
        ffmpeg_runtime: FfmpegRuntimeReport,
        device_uuid: Uuid,
        tuner_count: i32,
        iptv_access_token_verifier: IptvAccessTokenVerifier,
    ) -> Self {
        let repositories = Arc::new(make_repositories(pool.clone()));
        let hls_sessions: Arc<dyn HlsSessionRegistry> = Arc::new(InMemoryHlsSessionRegistry::new());
        let hls_direct_indices = Arc::new(RwLock::new(HashMap::new()));
        Self {
            repositories,
            pool,
            database_backend,
            system_startup: Arc::new(system_startup),
            ffmpeg_runtime: Arc::new(ffmpeg_runtime),
            device_uuid,
            tuner_count,
            hls_sessions,
            hls_direct_indices,
            iptv_access_token_verifier,
        }
    }

    #[cfg(test)]
    #[must_use]
    pub fn for_tests(iptv_access_token_verifier: IptvAccessTokenVerifier) -> Self {
        let pool =
            ersatzrs_infra::db::lazy_sqlite_pool("sqlite::memory:").expect("lazy sqlite pool");

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
        )
    }

    #[must_use]
    pub fn health_issue_counts(&self) -> (u32, u32) {
        let (startup_errors, startup_warnings) = self.system_startup.health_issue_counts();
        let (ffmpeg_errors, ffmpeg_warnings) = self.ffmpeg_runtime.issue_counts();
        (
            startup_errors.saturating_add(ffmpeg_errors),
            startup_warnings.saturating_add(ffmpeg_warnings),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::SystemStartup;
    use ersatzrs_infra::db::DatabaseSchemaStatus;

    #[test]
    fn startup_is_ready_when_database_and_schema_are_clean() {
        let startup = SystemStartup {
            database_ready: true,
            schema_status: DatabaseSchemaStatus::default(),
        };

        assert!(startup.is_ready());
        assert_eq!(startup.health_issue_counts(), (0, 0));
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
