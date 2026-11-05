use std::sync::Arc;

use super::teletext_output_observations::OutputObservations;
use async_trait::async_trait;
use ersatzrs_app::{ChannelRuntime, ChannelRuntimeError};
use ersatzrs_core::hls::HlsSessionRegistry;
use ersatzrs_domain::EntityId;
use ersatzrs_infra::db::DbPool;

/// Native channel runtime assembled by the web composition root.
#[derive(Clone)]
pub struct NativeChannelRuntime {
    pool: DbPool,
    hls_sessions: Arc<dyn HlsSessionRegistry>,
    preview_hls_sessions: Arc<dyn HlsSessionRegistry>,
    teletext_outputs: OutputObservations,
}

impl NativeChannelRuntime {
    #[must_use]
    pub fn new(
        pool: DbPool,
        hls_sessions: Arc<dyn HlsSessionRegistry>,
        preview_hls_sessions: Arc<dyn HlsSessionRegistry>,
    ) -> Self {
        Self {
            pool,
            hls_sessions,
            preview_hls_sessions,
            teletext_outputs: OutputObservations::default(),
        }
    }

    /// Share one diagnostics registry with the admin Health runtime.
    #[must_use]
    pub(super) fn with_teletext_outputs(mut self, outputs: OutputObservations) -> Self {
        self.teletext_outputs = outputs;
        self
    }
}

#[async_trait]
impl ChannelRuntime for NativeChannelRuntime {
    fn observe_teletext_output(
        &self,
        channel_id: EntityId,
        initial: ersatzrs_domain::TeletextOutputObservation,
    ) -> Option<Box<dyn ersatzrs_app::TeletextOutputObserver>> {
        self.teletext_outputs.register(channel_id, initial)
    }

    fn teletext_output_observations(
        &self,
        channel_id: EntityId,
    ) -> Option<ersatzrs_domain::TeletextChannelOutputObservations> {
        self.teletext_outputs.snapshot(channel_id)
    }

    async fn refresh_profile_playouts(
        &self,
        profile_id: EntityId,
    ) -> Result<(), ChannelRuntimeError> {
        let numbers = sqlx::query_scalar::<_, String>(
            r#"SELECT "Number" FROM "Channel" WHERE "FFmpegProfileId" = ? AND "StreamingEngine" = 1"#,
        )
        .bind(profile_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|_| ChannelRuntimeError::unavailable("profile channels could not be loaded"))?;
        for number in numbers {
            // The normal registry forwards updates to its preview registry.
            self.hls_sessions.playout_updated(&number).await;
        }
        Ok(())
    }

    async fn active_channel_numbers(&self) -> Result<Vec<String>, ChannelRuntimeError> {
        let mut active = self.hls_sessions.active_channels().await;
        active.extend(self.preview_hls_sessions.active_channels().await);
        active.sort();
        active.dedup();
        Ok(active)
    }

    async fn session_models(
        &self,
    ) -> Result<Vec<ersatzrs_core::hls::HlsSessionModel>, ChannelRuntimeError> {
        Ok(self.hls_sessions.session_models().await)
    }

    async fn preview_segment_output(
        &self,
        number: &str,
        segment: &str,
        started_at_millis: i64,
    ) -> Result<Option<ersatzrs_core::hls::HlsSegmentOutput>, ChannelRuntimeError> {
        let Some(worker) = self.preview_hls_sessions.try_get(number).await else {
            return Ok(None);
        };
        let output = worker.segment_output(segment, started_at_millis).await;
        let current = self.preview_hls_sessions.try_get(number).await;
        Ok(current
            .filter(|current| Arc::ptr_eq(current, &worker))
            .and(output))
    }

    async fn stop_channel_session(&self, number: &str) -> Result<bool, ChannelRuntimeError> {
        let (normal, preview) = tokio::join!(
            self.hls_sessions.stop_channel(number),
            self.preview_hls_sessions.stop_channel(number)
        );
        Ok(normal || preview)
    }

    fn request_subtitle_extraction(&self, playout_ids: Vec<EntityId>) {
        let pool = self.pool.clone();
        tokio::spawn(async move {
            for playout_id in playout_ids {
                if let Err(error) =
                    ersatzrs_infra::subtitles::extract_embedded_playout_subtitles(&pool, playout_id)
                        .await
                {
                    tracing::warn!(
                        playout_id,
                        %error,
                        "embedded subtitle extraction failed after channel save"
                    );
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_core::hls::{HlsSegmentOutput, HlsSessionHandle};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::Notify;

    struct PreviewWorker {
        touches: AtomicUsize,
        entered: Notify,
        release: Option<Arc<Notify>>,
    }

    #[async_trait]
    impl HlsSessionHandle for PreviewWorker {
        fn channel_number(&self) -> &'static str {
            "synthetic"
        }
        fn touch(&self) {
            self.touches.fetch_add(1, Ordering::Relaxed);
        }
        async fn trim_playlist(&self) -> Option<String> {
            None
        }
        async fn cancel(&self) {
            panic!("status reads must not cancel")
        }
        fn playout_updated(&self) {
            panic!("status reads must not rebuild")
        }
        async fn segment_output(
            &self,
            segment: &str,
            _started_at_millis: i64,
        ) -> Option<HlsSegmentOutput> {
            self.entered.notify_one();
            if let Some(release) = &self.release {
                release.notified().await;
            }
            (segment == "slate.ts").then(|| HlsSegmentOutput {
                error: Some("synthetic drawvg failure".into()),
            })
        }
    }

    fn worker(release: Option<Arc<Notify>>) -> Arc<PreviewWorker> {
        Arc::new(PreviewWorker {
            touches: AtomicUsize::new(0),
            entered: Notify::new(),
            release,
        })
    }

    #[tokio::test]
    async fn preview_segment_status_uses_only_preview_registry_without_touching() {
        let state = crate::host::state::AppState::for_tests(
            crate::host::state::IptvAccessTokenVerifier::from_optional_secret(None),
        );
        let runtime = NativeChannelRuntime::new(
            state.pool,
            state.hls_sessions.clone(),
            state.preview_hls_sessions.clone(),
        );
        let normal = worker(None);
        state.hls_sessions.insert(normal.clone()).await;
        assert!(runtime
            .preview_segment_output("synthetic", "slate.ts", 0)
            .await
            .unwrap()
            .is_none());
        let preview = worker(None);
        state.preview_hls_sessions.insert(preview.clone()).await;
        assert!(runtime
            .preview_segment_output("synthetic", "slate.ts", 0)
            .await
            .unwrap()
            .unwrap()
            .error
            .is_some());
        assert!(runtime
            .preview_segment_output("synthetic", "unknown.ts", 0)
            .await
            .unwrap()
            .is_none());
        assert_eq!(normal.touches.load(Ordering::Relaxed), 0);
        assert_eq!(preview.touches.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn replaced_preview_worker_cannot_return_its_stale_error() {
        let state = crate::host::state::AppState::for_tests(
            crate::host::state::IptvAccessTokenVerifier::from_optional_secret(None),
        );
        let runtime = NativeChannelRuntime::new(
            state.pool,
            state.hls_sessions,
            state.preview_hls_sessions.clone(),
        );
        let release = Arc::new(Notify::new());
        let old = worker(Some(release.clone()));
        state.preview_hls_sessions.insert(old.clone()).await;
        let read = tokio::spawn(async move {
            runtime
                .preview_segment_output("synthetic", "slate.ts", 0)
                .await
        });
        tokio::time::timeout(std::time::Duration::from_secs(5), old.entered.notified())
            .await
            .unwrap();
        state.preview_hls_sessions.remove("synthetic").await;
        state.preview_hls_sessions.insert(worker(None)).await;
        release.notify_one();
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(5), read)
                .await
                .unwrap()
                .unwrap()
                .unwrap()
                .is_none()
        );
        assert_eq!(old.touches.load(Ordering::Relaxed), 0);
    }
}
