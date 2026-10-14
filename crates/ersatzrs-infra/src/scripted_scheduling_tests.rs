use super::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use async_trait::async_trait;
use ersatzrs_core::repositories::TeletextProfileRejection;

struct Controls {
    calls: AtomicUsize,
    cancellation: Option<Arc<AtomicBool>>,
}
#[async_trait]
impl ScriptedTeletextControl for Controls {
    async fn teletext_base(
        &self,
        channel_id: EntityId,
        addon_id: &str,
    ) -> Result<TeletextProfileMutation<TeletextScriptedBaseSelection>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(cancellation) = &self.cancellation {
            cancellation.store(true, Ordering::SeqCst);
        }
        Ok(TeletextProfileMutation::Saved(
            TeletextScriptedBaseSelection {
                channel_id,
                addon_id: addon_id.to_owned(),
                profile_id: 7,
                enabled: false,
            },
        ))
    }
    async fn teletext_candidate(
        &self,
        _: EntityId,
        _: &str,
        _: EntityId,
    ) -> Result<TeletextProfileMutation<TeletextProfileCandidateSnapshot>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(TeletextProfileMutation::Rejected(
            TeletextProfileRejection::WrongAddon,
        ))
    }
}

fn sources(controls: Arc<Controls>) -> SqlSources {
    SqlSources {
        repository: SqliteSchedulingContentRepository::new(
            sqlx::sqlite::SqlitePoolOptions::new()
                .connect_lazy("sqlite::memory:")
                .unwrap(),
        ),
        runtime: tokio::runtime::Handle::current(),
        cancelled: None,
        dependencies: Mutex::default(),
        teletext: Some(controls),
        teletext_deadline: Instant::now() + Duration::from_secs(30),
    }
}

#[tokio::test]
async fn owned_control_bridge_retains_one_deadline_and_rejects_expired_work_before_poll() {
    let controls = Arc::new(Controls {
        calls: AtomicUsize::new(0),
        cancellation: None,
    });
    let mut sources = sources(controls.clone());
    tokio::task::spawn_blocking(move || {
        let deadline = sources.teletext_deadline;
        let base = sources.teletext_base(3, "org.example.control").unwrap();
        assert!(matches!(base, TeletextProfileMutation::Saved(value)
            if value.channel_id == 3 && value.profile_id == 7 && !value.enabled));
        assert_eq!(sources.teletext_deadline, deadline);
        assert_eq!(
            sources
                .teletext_candidate(3, "org.example.control", 9)
                .unwrap(),
            TeletextProfileMutation::Rejected(TeletextProfileRejection::WrongAddon)
        );
        assert_eq!(sources.teletext_deadline, deadline);
        sources.teletext_deadline = Instant::now();
        assert!(sources.teletext_base(3, "org.example.control").is_err());
        assert_eq!(controls.calls.load(Ordering::SeqCst), 2);
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn owned_control_bridge_checks_cancellation_before_and_after_application_work() {
    let cancellation = Arc::new(AtomicBool::new(true));
    let controls = Arc::new(Controls {
        calls: AtomicUsize::new(0),
        cancellation: Some(cancellation.clone()),
    });
    let mut sources = sources(controls.clone());
    sources.cancelled = Some(Arc::new({
        let cancellation = cancellation.clone();
        move || cancellation.load(Ordering::SeqCst)
    }));
    tokio::task::spawn_blocking(move || {
        assert!(sources.teletext_base(3, "org.example.control").is_err());
        assert_eq!(controls.calls.load(Ordering::SeqCst), 0);
        cancellation.store(false, Ordering::SeqCst);
        assert!(sources.teletext_base(3, "org.example.control").is_err());
        assert_eq!(controls.calls.load(Ordering::SeqCst), 1);
        assert!(cancellation.load(Ordering::SeqCst));
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn owned_control_bridge_preserves_repository_conflict_and_missing_port_is_explicit() {
    let mut sources = sources(Arc::new(Controls {
        calls: AtomicUsize::new(0),
        cancellation: None,
    }));
    sources.teletext = None;
    tokio::task::spawn_blocking(move || {
        assert!(sources.teletext_base(3, "org.example.control").is_err());
        let result: Result<()> = sources.lookup_control(async {
            Err(RepositoryError::Conflict(
                "synthetic control changed".to_owned(),
            ))
        });
        assert!(matches!(result, Err(RepositoryError::Conflict(message))
            if message == "synthetic control changed"));
    })
    .await
    .unwrap();
}
