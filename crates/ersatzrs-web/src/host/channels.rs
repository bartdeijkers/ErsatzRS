use std::sync::Arc;

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
        }
    }
}

#[async_trait]
impl ChannelRuntime for NativeChannelRuntime {
    async fn active_channel_numbers(&self) -> Result<Vec<String>, ChannelRuntimeError> {
        let mut active = self.hls_sessions.active_channels().await;
        active.extend(self.preview_hls_sessions.active_channels().await);
        active.sort();
        active.dedup();
        Ok(active)
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
