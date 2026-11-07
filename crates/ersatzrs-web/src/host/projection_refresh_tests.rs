use super::*;
use ersatzrs_core::hls::HlsSessionHandle;
use ersatzrs_infra::hls::InMemoryHlsSessionRegistry;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Worker {
    number: String,
    updates: AtomicUsize,
}

#[async_trait]
impl HlsSessionHandle for Worker {
    fn channel_number(&self) -> &str {
        &self.number
    }
    fn touch(&self) {
        panic!("refresh must not extend playback lifetime")
    }
    async fn trim_playlist(&self) -> Option<String> {
        panic!("refresh must not read a playlist")
    }
    async fn cancel(&self) {
        panic!("refresh must not stop playback")
    }
    fn playout_updated(&self) {
        self.updates.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn projection_refresh_reaches_normal_and_preview_once_without_touching_others() {
    let previews = Arc::new(InMemoryHlsSessionRegistry::new());
    let normal = Arc::new(InMemoryHlsSessionRegistry::with_playout_updates_to(
        previews.clone(),
    ));
    let mut workers = Vec::new();
    for registry in [&normal, &previews] {
        for number in ["1", "2"] {
            let worker = Arc::new(Worker {
                number: number.into(),
                updates: AtomicUsize::new(0),
            });
            registry.insert(worker.clone()).await;
            workers.push(worker);
        }
    }
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .connect_lazy("sqlite::memory:")
        .unwrap();
    let runtime = NativeChannelRuntime::new(pool, normal, previews);
    runtime
        .refresh_playout_projections(vec!["1".into(), "1".into()])
        .await
        .unwrap();
    assert_eq!(
        workers
            .iter()
            .map(|worker| worker.updates.load(Ordering::SeqCst))
            .collect::<Vec<_>>(),
        [1, 0, 1, 0]
    );
}

async fn application_fixture() -> (crate::host::state::AppState, i64) {
    use ersatzrs_core::repositories::{ChannelRepository, FfmpegProfileRepository};
    use ersatzrs_domain::{Channel, ChannelPlayoutSource, FFmpegProfile, StreamingEngine};
    let state = crate::host::state::AppState::for_tests(
        crate::host::state::IptvAccessTokenVerifier::disabled(),
    );
    sqlx::migrate!("../../migrations")
        .run(&state.pool)
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO "Resolution" ("Id","Name","Width","Height") VALUES (1,'Fixture',320,180)"#,
    )
    .execute(&state.pool)
    .await
    .unwrap();
    let repos = ersatzrs_infra::repositories::make_repositories(state.pool.clone());
    repos
        .ffmpeg_profile
        .upsert(&FFmpegProfile {
            id: 1,
            resolution_id: 1,
            ..Default::default()
        })
        .await
        .unwrap();
    let watermark = state
        .application
        .save_watermark(ersatzrs_app::commands::SaveWatermark {
            form: watermark_form(0),
        })
        .await
        .unwrap();
    for id in 1..=5 {
        let channel = Channel {
            id,
            number: id.to_string(),
            name: format!("Fixture {id}"),
            ffmpeg_profile_id: 1,
            streaming_engine: if id == 2 {
                StreamingEngine::Legacy
            } else {
                StreamingEngine::Next
            },
            watermark_id: matches!(id, 1 | 2).then_some(watermark),
            playout_source: if id >= 4 {
                ChannelPlayoutSource::Mirror
            } else {
                ChannelPlayoutSource::Generated
            },
            mirror_source_channel_id: match id {
                4 => Some(2),
                5 => Some(1),
                _ => None,
            },
            ..Default::default()
        };
        repos.channel.upsert(&channel).await.unwrap();
    }
    (state, watermark)
}

fn watermark_form(id: i64) -> ersatzrs_app::commands::WatermarkAdminForm {
    crate::admin::server_fns::watermarks::watermark_form_into_admin(
        crate::admin::server_fns::watermarks::WatermarkForm {
            id,
            name: "Fixture".into(),
            image_source: 1,
            ..Default::default()
        },
    )
}

#[tokio::test]
async fn application_watermark_save_and_delete_refresh_only_dependents_before_refs_disappear() {
    let (state, watermark) = application_fixture().await;
    let mut workers = Vec::new();
    for registry in [&state.hls_sessions, &state.preview_hls_sessions] {
        for id in 1..=5 {
            let worker = Arc::new(Worker {
                number: id.to_string(),
                updates: AtomicUsize::new(0),
            });
            registry.insert(worker.clone()).await;
            workers.push(worker);
        }
    }
    let mut form = watermark_form(watermark);
    form.opacity = 70;
    state
        .application
        .save_watermark(ersatzrs_app::commands::SaveWatermark { form })
        .await
        .unwrap();
    assert_eq!(
        workers
            .iter()
            .map(|worker| worker.updates.load(Ordering::SeqCst))
            .collect::<Vec<_>>(),
        [1, 0, 0, 1, 1, 1, 0, 0, 1, 1]
    );
    state
        .application
        .delete_watermark(ersatzrs_app::commands::DeleteWatermark { id: watermark })
        .await
        .unwrap();
    assert_eq!(
        workers
            .iter()
            .map(|worker| worker.updates.load(Ordering::SeqCst))
            .collect::<Vec<_>>(),
        [2, 0, 0, 2, 2, 2, 0, 0, 2, 2]
    );
    let references: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Channel" WHERE "WatermarkId"=?"#)
            .bind(watermark)
            .fetch_one(&state.pool)
            .await
            .unwrap();
    assert_eq!(references, 0);
}
