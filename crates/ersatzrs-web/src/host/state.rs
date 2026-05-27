use std::collections::{HashMap, HashSet};
use std::sync::{
    atomic::{AtomicI32, Ordering},
    Arc, Mutex,
};

use ersatzrs_core::hls::HlsSessionRegistry;
use ersatzrs_core::scheduling::PlayoutBuildRequest;
use ersatzrs_ffmpeg::FfmpegRuntimeReport;
use ersatzrs_infra::db::{DatabaseBackend, DatabaseSchemaStatus, DbPool};
use ersatzrs_infra::hls::InMemoryHlsSessionRegistry;
use ersatzrs_infra::repositories::{make_repositories, Repositories};
use jsonwebtoken::{DecodingKey, Validation};
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

/// In-process tracker for entities that have an active scan/sync. Mirrors
/// `IEntityLocker` in `ErsatzTV`: the admin UI polls this via a server fn to
/// render an indeterminate spinner while a background task is running.
///
/// Keys are arbitrary strings so the same locker can guard libraries
/// (`library:{id}`), external collection syncs (`collections:{kind}:{id}`),
/// and any future entity type.
#[derive(Clone, Default)]
pub struct EntityLocker {
    keys: Arc<Mutex<HashSet<String>>>,
}

/// In-process map of `library_id -> fractional progress in [0.0, 1.0]`.
/// Mirrors ETV's `IScannerProxyService.GetProgress` + the `LibraryScanProgress`
/// Courier notification, but polled rather than push-based.
///
/// Scanner entry points write through a `ScanProgressSink` adapter that
/// targets this map; the libraries page reads a snapshot via a server fn.
#[derive(Clone, Default)]
pub struct ScanProgressMap {
    inner: Arc<Mutex<HashMap<i64, f32>>>,
}

impl ScanProgressMap {
    pub fn set(&self, library_id: i64, fraction: f32) {
        let clamped = fraction.clamp(0.0, 1.0);
        if let Ok(mut map) = self.inner.lock() {
            map.insert(library_id, clamped);
        }
    }

    pub fn clear(&self, library_id: i64) {
        if let Ok(mut map) = self.inner.lock() {
            map.remove(&library_id);
        }
    }

    pub fn snapshot(&self) -> HashMap<i64, f32> {
        self.inner.lock().map(|map| map.clone()).unwrap_or_default()
    }
}

/// `ScanProgressSink` impl that fans out into a `ScanProgressMap` for one
/// specific `library_id`. Use `ScanProgressMap::sink_for(library_id)` to build
/// one inside the scan server fn before spawning the background task.
pub struct LibraryProgressSink {
    map: ScanProgressMap,
    library_id: i64,
}

impl ScanProgressMap {
    #[must_use]
    pub fn sink_for(&self, library_id: i64) -> LibraryProgressSink {
        LibraryProgressSink {
            map: self.clone(),
            library_id,
        }
    }
}

impl ersatzrs_domain::ScanProgressSink for LibraryProgressSink {
    fn report(&self, fraction: f32) {
        self.map.set(self.library_id, fraction);
    }
}

fn library_key(library_id: i64) -> String {
    format!("library:{library_id}")
}

impl EntityLocker {
    /// Attempts to acquire `key`. Returns true when the lock was acquired
    /// (caller is responsible for `unlock_key` when done). Returns false when
    /// the key is already locked.
    pub fn lock_key(&self, key: impl Into<String>) -> bool {
        self.keys.lock().is_ok_and(|mut set| set.insert(key.into()))
    }

    pub fn unlock_key(&self, key: &str) {
        if let Ok(mut set) = self.keys.lock() {
            set.remove(key);
        }
    }

    pub fn is_locked(&self, key: &str) -> bool {
        self.keys.lock().is_ok_and(|set| set.contains(key))
    }

    /// Returns every currently locked key.
    pub fn locked_keys(&self) -> Vec<String> {
        self.keys
            .lock()
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn lock_library(&self, library_id: i64) -> bool {
        self.lock_key(library_key(library_id))
    }

    pub fn unlock_library(&self, library_id: i64) {
        self.unlock_key(&library_key(library_id));
    }

    /// Returns the ids of currently locked libraries (those locked via
    /// `lock_library`, identified by the `library:` key prefix).
    pub fn locked_libraries(&self) -> Vec<i64> {
        self.keys
            .lock()
            .map(|set| {
                set.iter()
                    .filter_map(|k| {
                        k.strip_prefix("library:")
                            .and_then(|n| n.parse::<i64>().ok())
                    })
                    .collect()
            })
            .unwrap_or_default()
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
    pub repositories: Arc<Repositories>,
    pub pool: DbPool,
    pub database_backend: DatabaseBackend,
    pub system_startup: Arc<SystemStartup>,
    pub ffmpeg_runtime: Arc<RwLock<FfmpegRuntimeReport>>,
    pub device_uuid: Uuid,
    pub tuner_count: Arc<AtomicI32>,
    pub hls_sessions: Arc<dyn HlsSessionRegistry>,
    pub hls_direct_indices: Arc<RwLock<HashMap<i64, (String, u64)>>>,
    pub iptv_access_token_verifier: IptvAccessTokenVerifier,
    pub entity_locker: EntityLocker,
    pub scan_progress: ScanProgressMap,
    pub playout_build_tx: mpsc::Sender<PlayoutBuildRequest>,
}

#[derive(Clone)]
pub struct IptvAccessTokenVerifier {
    pub(crate) validation: Option<Validation>,
    pub(crate) decoding_key: Option<DecodingKey>,
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
        playout_build_tx: mpsc::Sender<PlayoutBuildRequest>,
    ) -> Self {
        let repositories = Arc::new(make_repositories(pool.clone()));
        let hls_sessions: Arc<dyn HlsSessionRegistry> = Arc::new(InMemoryHlsSessionRegistry::new());
        let hls_direct_indices = Arc::new(RwLock::new(HashMap::new()));
        Self {
            repositories,
            pool,
            database_backend,
            system_startup: Arc::new(system_startup),
            ffmpeg_runtime: Arc::new(RwLock::new(ffmpeg_runtime)),
            device_uuid,
            tuner_count: Arc::new(AtomicI32::new(tuner_count)),
            hls_sessions,
            hls_direct_indices,
            iptv_access_token_verifier,
            entity_locker: EntityLocker::default(),
            scan_progress: ScanProgressMap::default(),
            playout_build_tx,
        }
    }

    #[cfg(test)]
    #[must_use]
    pub fn for_tests(iptv_access_token_verifier: IptvAccessTokenVerifier) -> Self {
        let pool =
            ersatzrs_infra::db::lazy_sqlite_pool("sqlite::memory:").expect("lazy sqlite pool");
        let (playout_build_tx, _playout_build_rx) = mpsc::channel(1);

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
    use super::{AppState, IptvAccessTokenVerifier, ScanProgressMap, SystemStartup};
    use ersatzrs_domain::ScanProgressSink;
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
    fn scan_progress_map_set_clear_snapshot_round_trip() {
        let map = ScanProgressMap::default();
        assert!(map.snapshot().is_empty());

        map.set(7, 0.25);
        map.set(8, 1.5); // should clamp to 1.0
        map.set(9, -0.5); // should clamp to 0.0

        let snap = map.snapshot();
        assert_eq!(snap.get(&7).copied(), Some(0.25));
        assert_eq!(snap.get(&8).copied(), Some(1.0));
        assert_eq!(snap.get(&9).copied(), Some(0.0));

        map.clear(7);
        let snap = map.snapshot();
        assert!(!snap.contains_key(&7));
        assert_eq!(snap.len(), 2);
    }

    #[test]
    fn library_progress_sink_writes_into_owning_map() {
        let map = ScanProgressMap::default();
        let sink = map.sink_for(42);
        sink.report(0.0);
        sink.report(0.5);
        sink.report(0.9);
        assert_eq!(map.snapshot().get(&42).copied(), Some(0.9));
    }

    #[tokio::test]
    async fn app_state_tuner_count_updates_in_place() {
        let state = AppState::for_tests(IptvAccessTokenVerifier::disabled());
        assert_eq!(state.tuner_count(), 2);

        state.set_tuner_count(4);

        assert_eq!(state.tuner_count(), 4);
    }

    #[test]
    fn scan_progress_map_handles_concurrent_writes() {
        use std::thread;

        let map = ScanProgressMap::default();
        let mut handles = Vec::new();
        for library_id in 0..16i64 {
            let map = map.clone();
            handles.push(thread::spawn(move || {
                for step in 0..100i32 {
                    #[allow(clippy::cast_precision_loss)]
                    let fraction = step as f32 / 100.0;
                    map.set(library_id, fraction);
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        let snap = map.snapshot();
        assert_eq!(snap.len(), 16);
        for library_id in 0..16i64 {
            let v = snap.get(&library_id).copied().unwrap();
            assert!((0.0..=1.0).contains(&v));
        }
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
