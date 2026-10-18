//! Explicit finite bitmap carriage preparation; ordinary Burn remains unchanged.
use super::{
    begin, unavailable, ClosingScope, FfprobeExtractor, LocalProbeCleanupScope, PlaybackSource,
    Result, StreamingMode,
};
use std::sync::Arc;
use tokio::time::Instant;

impl PlaybackSource {
    pub(super) async fn prepare_bitmap_carriage(
        &mut self,
        mode: StreamingMode,
        deadline: Instant,
    ) -> Result<()> {
        if !matches!(
            mode,
            StreamingMode::TransportStream | StreamingMode::TransportStreamHybrid
        ) {
            return Err(unavailable());
        }
        self.recheck_candidate(deadline).await?;
        let work = begin(&self.pool, &self.locker, deadline).await?;
        let scope = ClosingScope(LocalProbeCleanupScope::new());
        let extractor = FfprobeExtractor::new(ersatzrs_ffmpeg::ffprobe_path());
        let operation = scope
            .0
            .scope(async {
                let artifact = extractor
                    .prepare_local_teletext_bitmap_artifact(
                        self.receipt.clone(),
                        deadline,
                        work.hold_work(),
                    )
                    .await
                    .map_err(|_| unavailable())?;
                let input = artifact.carriage_input().map_err(|_| unavailable())?;
                self.recheck_candidate(deadline).await?;
                self.receipt
                    .check_source(deadline, work.hold_work())
                    .await
                    .map_err(|_| unavailable())?;
                Ok::<_, ersatzrs_core::repositories::RepositoryError>((Arc::new(artifact), input))
            })
            .await;
        scope.0.close();
        scope.0.wait_cleanup().await.map_err(|_| unavailable())?;
        let (artifact, input) = operation?;
        self.artifact = Some(artifact);
        self.caption_input = input;
        Ok(())
    }
}
