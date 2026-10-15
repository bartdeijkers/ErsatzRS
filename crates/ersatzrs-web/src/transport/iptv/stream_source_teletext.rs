//! Thin ordinary-route adapter for shared native source caption preparation.

use std::{sync::Arc, time::Duration};

use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::{ChannelSubtitleMode, StreamingMode};
use ersatzrs_infra::playback::source_teletext::runtime::{self, RuntimeSelectionContext};
use tokio::time::Instant;

pub(super) use ersatzrs_infra::playback::source_teletext::runtime::{
    PlaybackSource, Selection, SourceTeletextPlaybackInterval,
};

use super::ActivePlayoutItem;
use crate::host::state::AppState;

pub(super) struct SelectionContext<'a> {
    pub state: &'a AppState,
    pub active: &'a ActivePlayoutItem,
    pub resolved: &'a str,
    pub languages: &'a [String],
    pub subtitle_mode: ChannelSubtitleMode,
    pub streaming_mode: StreamingMode,
    pub playback_interval: Option<SourceTeletextPlaybackInterval>,
    pub deadline: Instant,
}

pub(super) async fn select(context: SelectionContext<'_>) -> Result<Option<Selection>> {
    if context.active.external.is_some() || context.active.remote_stream.is_some() {
        return Ok(None);
    }
    let locker = context.state.application.operations().entity_locker();
    runtime::select(RuntimeSelectionContext {
        pool: &context.state.pool,
        locker: &locker,
        media_item_id: Some(context.active.media_item_id),
        media_version_id: Some(context.active.media_version_id),
        media_file_id: context.active.media_file_id,
        original_path: &context.active.media_path,
        resolved_path: context.resolved,
        languages: context.languages,
        subtitle_mode: context.subtitle_mode,
        streaming_mode: context.streaming_mode,
        playback_interval: context.playback_interval,
        deadline: context.deadline,
    })
    .await
}

/// Startup/retry context, owned by the existing actual stream supervisor.
#[derive(Clone)]
pub(super) struct Consumer {
    source: Arc<PlaybackSource>,
    plan: Option<ersatzrs_ffmpeg::SourceTeletextInputPlan>,
    reservation: Option<ersatzrs_infra::transcode_capacity::ActiveTranscodeReservation>,
}

impl Consumer {
    pub(super) fn new(
        source: Arc<PlaybackSource>,
        plan: Option<ersatzrs_ffmpeg::SourceTeletextInputPlan>,
    ) -> Self {
        Self {
            source,
            plan,
            reservation: None,
        }
    }

    pub(super) fn initial_deadline(&self) -> Instant {
        self.source.initial_deadline()
    }

    pub(super) async fn before_open(
        &mut self,
        command: &ersatzrs_ffmpeg::FfmpegCommand,
        reservation: Option<&ersatzrs_infra::transcode_capacity::ActiveTranscodeReservation>,
        deadline: Instant,
    ) -> Result<()> {
        self.check_command(command)?;
        self.reservation = Some(reservation.ok_or_else(unavailable)?.clone());
        self.source
            .observe_consumer_origin(command, self.plan.as_ref(), deadline)
            .await
            .map(|_| ())
    }

    pub(super) async fn before_retry(
        &mut self,
        command: &ersatzrs_ffmpeg::FfmpegCommand,
        plan: Option<ersatzrs_ffmpeg::SourceTeletextInputPlan>,
    ) -> Result<()> {
        // A changed hardware topology needs this attempt's own generated plan.
        // Never let an absent or mismatched retry borrow the initial seal.
        self.plan = plan;
        self.check_command(command)?;
        self.source
            .observe_consumer_origin(
                command,
                self.plan.as_ref(),
                Instant::now() + Duration::from_secs(15),
            )
            .await
            .map(|_| ())
    }

    fn check_command(&self, command: &ersatzrs_ffmpeg::FfmpegCommand) -> Result<()> {
        self.source
            .check_consumer_command(command, self.plan.as_ref())
    }

    pub(super) fn attach(
        &self,
        child: &mut ersatzrs_infra::remote_stream::RemoteStreamProcess,
    ) -> Result<()> {
        child
            .attach_local_teletext_source(
                self.source.receipt(),
                self.reservation.as_ref().ok_or_else(unavailable)?.clone(),
            )
            .map_err(|_| unavailable())
    }
}

fn unavailable() -> RepositoryError {
    RepositoryError::unavailable(
        "source teletext captions",
        "source caption selection is unavailable",
    )
}
