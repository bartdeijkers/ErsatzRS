//! Regression proof for cancellation and absolute wrapper startup deadlines.
//! The held connection stays held until cleanup is observed, so releasing the
//! database cannot accidentally make an abandoned response appear cooperative.

use super::*;
use ersatzrs_infra::transcode_capacity::{
    active_transcode_snapshot, active_transcode_test_guard_for_tests,
    reset_active_transcode_state_for_tests,
};

async fn held_capacity_pool() -> DbPool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query(
        r#"CREATE TABLE "ConfigElement" (
            "Key" TEXT PRIMARY KEY,
            "Value" TEXT NOT NULL
        )"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    pool
}

fn unstarted_command() -> FfmpegCommand {
    FfmpegCommand {
        executable: "wrapper-fixture-must-not-start".to_owned(),
        arguments: Vec::new(),
        environment: Vec::new(),
        mpegts_output: None,
    }
}

async fn wait_for_registration(registry: &Arc<dyn HlsSessionRegistry>, number: &str) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while !registry.is_active(number).await {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("wait actor must register before the response is dropped");
}

async fn wait_for_cleanup(
    registry: &Arc<dyn HlsSessionRegistry>,
    number: &str,
    artifact: &std::path::Path,
) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while registry.is_active(number).await || artifact.exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("body drop must unregister and clean up without a SQLite connection");
}

fn with_capacity_state(future: impl std::future::Future<Output = ()>) {
    let _guard = active_transcode_test_guard_for_tests();
    reset_active_transcode_state_for_tests();
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future);
    assert_eq!(active_transcode_snapshot(2).active, 0);
}

#[test]
fn generic_capacity_wait_drop_cleans_up_while_sqlite_is_unavailable() {
    with_capacity_state(generic_wait_drop());
}

async fn generic_wait_drop() {
    let pool = held_capacity_pool().await;
    let held = pool.acquire().await.unwrap();
    let occupied =
        try_reserve_active_transcode(2, ActiveTranscodeSlot::new_mpeg_ts("other")).unwrap();
    let root = tempfile::tempdir().unwrap();
    let artifact = root.path().join("pending-custom-wrapper");
    tokio::fs::write(&artifact, b"prepared command")
        .await
        .unwrap();
    let registry: Arc<dyn HlsSessionRegistry> =
        Arc::new(ersatzrs_infra::hls::InMemoryHlsSessionRegistry::new());
    let body = spawn_stream_body_waiting_for_mpegts_capacity(
        unstarted_command(),
        unstarted_command(),
        vec![artifact.clone()],
        Vec::new(),
        "blocked-generic".to_owned(),
        registry.clone(),
        pool.clone(),
        None,
        None,
        None,
        TeletextOutputAdmission::plain(TeletextOutputRole::FinalWrapperMpegTs),
        None,
    );
    wait_for_registration(&registry, "blocked-generic").await;
    assert_eq!(pool.num_idle(), 0);
    drop(body);
    wait_for_cleanup(&registry, "blocked-generic", &artifact).await;
    assert_eq!(
        pool.num_idle(),
        0,
        "SQLite remains unavailable during cleanup"
    );
    assert_eq!(active_transcode_snapshot(2).active, 1);
    drop(held);
    drop(occupied);
    assert_eq!(active_transcode_snapshot(2).active, 0);
    pool.close().await;
}

fn state_for_pool(pool: DbPool) -> AppState {
    let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel();
    AppState::new(
        pool,
        ersatzrs_infra::db::DatabaseBackend::Sqlite,
        crate::host::state::SystemStartup {
            database_ready: true,
            schema_status: ersatzrs_infra::db::DatabaseSchemaStatus::default(),
        },
        ersatzrs_ffmpeg::FfmpegRuntimeReport::for_tests(),
        uuid::Uuid::nil(),
        2,
        crate::host::state::IptvAccessTokenVerifier::disabled(),
        sender,
        ersatzrs_core::locking::EntityLocker::default(),
    )
}

#[test]
fn legacy_capacity_wait_drop_cleans_up_while_sqlite_is_unavailable() {
    with_capacity_state(async {
        let pool = held_capacity_pool().await;
        let held = pool.acquire().await.unwrap();
        let state = state_for_pool(pool.clone());
        let channel = Channel {
            id: 314,
            number: "blocked-legacy".to_owned(),
            ..Channel::default()
        };
        let occupied =
            try_reserve_active_transcode(2, ActiveTranscodeSlot::new_mpeg_ts("other")).unwrap();
        let root = tempfile::tempdir().unwrap();
        let artifact = root.path().join("pending-capacity-slate");
        tokio::fs::create_dir(&artifact).await.unwrap();
        tokio::fs::write(artifact.join("slate.vgs"), b"prepared slate")
            .await
            .unwrap();
        let body = legacy_wrapper_capacity_wait_body(
            &state,
            &channel,
            "127.0.0.1:1".parse().unwrap(),
            unstarted_command(),
            artifact.clone(),
            OutputMetadata::default(),
            5_000_000,
        );
        wait_for_registration(&state.hls_sessions, &channel.number).await;
        assert_eq!(pool.num_idle(), 0);
        drop(body);
        wait_for_cleanup(&state.hls_sessions, &channel.number, &artifact).await;
        assert_eq!(
            pool.num_idle(),
            0,
            "cleanup must not depend on releasing SQLite"
        );
        assert_eq!(active_transcode_snapshot(2).active, 1);
        drop(held);
        drop(occupied);
        pool.close().await;
    });
}

#[tokio::test]
async fn hybrid_command_preparation_obeys_original_deadline_with_held_config() {
    let pool = held_capacity_pool().await;
    let held = pool.acquire().await.unwrap();
    let state = state_for_pool(pool.clone());
    let channel = Channel::default();
    tokio::time::pause();
    let deadline = tokio::time::Instant::now() + Duration::from_millis(100);
    let result = tokio::time::timeout(
        Duration::from_secs(1),
        segmenter_wrapper_command(
            &state,
            &channel,
            "http://127.0.0.1:1/iptv/session/1/index.m3u8".to_owned(),
            &OutputMetadata::default(),
            false,
            5_000_000,
            deadline,
        ),
    )
    .await
    .expect("script-config read must stop at the caller's deadline");
    assert_eq!(
        result
            .expect_err("held config must refuse startup")
            .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    // Tokio rounds a deadline up by one timer-wheel millisecond. This bounds
    // scheduler wakeup without renewing the original startup deadline.
    let returned_at = tokio::time::Instant::now();
    assert!(returned_at >= deadline);
    assert!(returned_at <= deadline + Duration::from_millis(1));
    assert_eq!(pool.num_idle(), 0, "config remained blocked until refusal");
    tokio::time::resume();
    drop(held);
    pool.close().await;
}

#[test]
fn hybrid_capacity_deadline_cleans_prepared_script_before_sqlite_is_released() {
    with_capacity_state(async {
        let pool = held_capacity_pool().await;
        let held = pool.acquire().await.unwrap();
        let state = state_for_pool(pool.clone());
        let channel = Channel::default();
        let root = tempfile::tempdir().unwrap();
        let artifact = root.path().join("prepared-custom-wrapper");
        tokio::fs::write(&artifact, b"prepared script")
            .await
            .unwrap();
        let deadline = tokio::time::Instant::now() + Duration::from_millis(100);
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            reserve_wrapper_capacity_before(
                &state,
                &channel,
                std::slice::from_ref(&artifact),
                deadline,
                None,
            ),
        )
        .await
        .expect("capacity admission must retain the original startup deadline");
        assert_eq!(
            result
                .err()
                .expect("held capacity config must refuse startup")
                .status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert!(tokio::time::Instant::now() >= deadline);
        assert!(
            !artifact.exists(),
            "expired prepared script must be removed"
        );
        assert_eq!(pool.num_idle(), 0);
        assert_eq!(active_transcode_snapshot(2).active, 0);
        assert!(state.hls_sessions.active_channels().await.is_empty());
        drop(held);
        pool.close().await;
    });
}

#[tokio::test]
async fn hybrid_post_recovery_refuses_original_expired_deadline_and_cleans_artifacts() {
    let root = tempfile::tempdir().unwrap();
    let artifact = root.path().join("late-custom-wrapper");
    tokio::fs::write(&artifact, b"prepared script")
        .await
        .unwrap();
    tokio::time::pause();
    let deadline = tokio::time::Instant::now() + Duration::from_millis(100);
    assert!(
        reject_expired_wrapper_start(deadline, std::slice::from_ref(&artifact))
            .await
            .is_none()
    );
    assert!(
        artifact.exists(),
        "an on-time command remains owned for startup"
    );
    tokio::time::advance(Duration::from_millis(100)).await;
    tokio::time::resume();
    let response = reject_expired_wrapper_start(deadline, std::slice::from_ref(&artifact))
        .await
        .expect("post-recovery spawn must refuse the original expired deadline");
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        !artifact.exists(),
        "late command artifacts must not survive refusal"
    );
}

#[test]
fn aborted_wrapper_request_removes_prepared_script_while_sqlite_is_held() {
    with_capacity_state(async {
        let pool = held_capacity_pool().await;
        let held = pool.acquire().await.unwrap();
        let state = state_for_pool(pool.clone());
        let root = tempfile::tempdir().unwrap();
        let script = root.path().join("abandoned-prepared-wrapper.sh");
        tokio::fs::write(&script, b"prepared command")
            .await
            .unwrap();
        let occupied =
            try_reserve_active_transcode(2, ActiveTranscodeSlot::new_mpeg_ts("other")).unwrap();
        let task_script = script.clone();
        let (ready, started) = tokio::sync::oneshot::channel();
        let handler = tokio::spawn(async move {
            let paths = vec![task_script];
            let guards = guard_prepared_mpegts_scripts(&paths).unwrap();
            ready.send(()).unwrap();
            let result = reserve_wrapper_capacity_before(
                &state,
                &Channel::default(),
                &paths,
                tokio::time::Instant::now() + Duration::from_secs(20),
                None,
            )
            .await;
            drop(guards);
            result
        });
        started.await.unwrap();
        assert!(
            script.exists(),
            "request owns the prepared script before admission"
        );
        assert_eq!(pool.num_idle(), 0);
        assert!(
            !handler.is_finished(),
            "capacity read must still be blocked"
        );
        handler.abort();
        let Err(aborted) = handler.await else {
            panic!("request task was not aborted");
        };
        assert!(aborted.is_cancelled());
        assert!(
            !script.exists(),
            "aborted request must release its staged script"
        );
        assert_eq!(
            pool.num_idle(),
            0,
            "cleanup precedes releasing the database"
        );
        assert_eq!(active_transcode_snapshot(2).active, 1);
        drop(held);
        drop(occupied);
        pool.close().await;
    });
}

#[test]
fn handed_off_prepared_script_survives_request_guard_until_wait_body_drop() {
    with_capacity_state(async {
        let pool = held_capacity_pool().await;
        let held = pool.acquire().await.unwrap();
        let root = tempfile::tempdir().unwrap();
        let script = root.path().join("handed-off-wrapper.sh");
        tokio::fs::write(&script, b"prepared command")
            .await
            .unwrap();
        let mut guards = guard_prepared_mpegts_scripts(std::slice::from_ref(&script)).unwrap();
        let occupied =
            try_reserve_active_transcode(2, ActiveTranscodeSlot::new_mpeg_ts("other")).unwrap();
        let registry: Arc<dyn HlsSessionRegistry> =
            Arc::new(ersatzrs_infra::hls::InMemoryHlsSessionRegistry::new());
        let body = spawn_stream_body_waiting_for_mpegts_capacity(
            unstarted_command(),
            unstarted_command(),
            vec![script.clone()],
            Vec::new(),
            "handed-off-wrapper".to_owned(),
            registry.clone(),
            pool.clone(),
            None,
            None,
            None,
            TeletextOutputAdmission::plain(TeletextOutputRole::FinalWrapperMpegTs),
            None,
        );
        disarm_prepared_mpegts_scripts(&mut guards);
        drop(guards);
        wait_for_registration(&registry, "handed-off-wrapper").await;
        assert!(
            script.exists(),
            "body ownership must survive request guard disposal"
        );
        drop(body);
        wait_for_cleanup(&registry, "handed-off-wrapper", &script).await;
        assert_eq!(pool.num_idle(), 0);
        assert_eq!(active_transcode_snapshot(2).active, 1);
        drop(held);
        drop(occupied);
        pool.close().await;
    });
}
