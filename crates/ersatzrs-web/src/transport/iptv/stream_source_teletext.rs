//! Thin ordinary-route adapter for shared native source caption preparation.

use std::{sync::Arc, time::Duration};

use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::{
    ChannelSubtitleMode, HardwareAccelerationKind, SourceTeletextCaptionDelivery, StreamingMode,
};
use ersatzrs_infra::playback::source_teletext::runtime::{self, RuntimeSelectionContext};
use tokio::time::Instant;

pub(super) use ersatzrs_infra::playback::source_teletext::runtime::{
    PlaybackSource, Selection, SourceTeletextConsumerRequest, SourceTeletextPlaybackInterval,
};

use super::ActivePlayoutItem;
use crate::host::state::AppState;

pub(super) struct SelectionContext<'a> {
    pub state: &'a AppState,
    pub output_channel_id: i64,
    pub active: &'a ActivePlayoutItem,
    pub resolved: &'a str,
    pub languages: &'a [String],
    pub subtitle_mode: ChannelSubtitleMode,
    pub streaming_mode: StreamingMode,
    pub playback_interval: Option<SourceTeletextPlaybackInterval>,
    pub deadline: Instant,
}

pub(super) async fn select(context: SelectionContext<'_>) -> Result<Option<Selection>> {
    if context.active.external.is_some()
        || context.active.remote_stream.is_some()
        || context.subtitle_mode == ChannelSubtitleMode::None
        || context.streaming_mode == StreamingMode::HttpLiveStreamingDirect
    {
        return Ok(None);
    }
    let locker = context.state.application.operations().entity_locker();
    let delivery = tokio::time::timeout_at(
        context.deadline,
        ersatzrs_infra::source_teletext_delivery::load_channel_source_teletext_delivery(
            &context.state.pool,
            context.output_channel_id,
        ),
    )
    .await
    .map_err(|_| unavailable())??;
    Box::pin(runtime::select(RuntimeSelectionContext {
        pool: &context.state.pool,
        locker: &locker,
        media_item_id: Some(context.active.media_item_id),
        media_version_id: Some(context.active.media_version_id),
        media_file_id: context.active.media_file_id,
        original_path: &context.active.media_path,
        resolved_path: context.resolved,
        languages: context.languages,
        subtitle_mode: context.subtitle_mode,
        source_teletext_delivery: delivery,
        streaming_mode: context.streaming_mode,
        playback_interval: context.playback_interval,
        deadline: context.deadline,
    }))
    .await
}

/// Bind only the explicitly selected prepared source input, never a fallback.
pub(super) fn consumer_request(
    acceleration: HardwareAccelerationKind,
    request: ersatzrs_ffmpeg::PipelineRequest,
    source: Option<&PlaybackSource>,
) -> SourceTeletextConsumerRequest {
    if let Some(source) =
        source.filter(|source| source.delivery() == SourceTeletextCaptionDelivery::DvbSubtitles)
    {
        let expected = source.input();
        if request.inputs.iter().any(|input| input == &expected) {
            return SourceTeletextConsumerRequest::new_bitmap_carriage(
                acceleration,
                request,
                expected,
            );
        }
    }
    SourceTeletextConsumerRequest::new(acceleration, request)
}

/// Startup/retry context, owned by the existing actual stream supervisor.
#[derive(Clone)]
pub(super) struct Consumer {
    source: Arc<PlaybackSource>,
    plan: Option<ersatzrs_ffmpeg::SourceTeletextInputPlan>,
    request: Option<SourceTeletextConsumerRequest>,
    reservation: Option<ersatzrs_infra::transcode_capacity::ActiveTranscodeReservation>,
    initialized: Option<Arc<runtime::InitializedSourceTeletextBitmap>>,
}

impl Consumer {
    pub(super) fn new(
        source: Arc<PlaybackSource>,
        plan: Option<ersatzrs_ffmpeg::SourceTeletextInputPlan>,
        request: Option<SourceTeletextConsumerRequest>,
    ) -> Self {
        Self {
            source,
            plan,
            request,
            reservation: None,
            initialized: None,
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
    ) -> Result<ersatzrs_ffmpeg::FfmpegCommand> {
        self.check_command(command)?;
        self.reservation = Some(reservation.ok_or_else(unavailable)?.clone());
        let initialized = Box::pin(self.source.initialize_consumer(
            command,
            self.plan.as_ref(),
            self.request.as_ref().ok_or_else(unavailable)?,
            deadline,
        ))
        .await?;
        let (command, owner) = initialized.into_parts();
        self.initialized = owner;
        Ok(command)
    }

    pub(super) async fn before_retry(
        &mut self,
        command: &ersatzrs_ffmpeg::FfmpegCommand,
        plan: Option<ersatzrs_ffmpeg::SourceTeletextInputPlan>,
        request: Option<SourceTeletextConsumerRequest>,
    ) -> Result<ersatzrs_ffmpeg::FfmpegCommand> {
        // A changed hardware topology needs this attempt's own generated plan.
        // Never let an absent or mismatched retry borrow the initial seal.
        self.plan = plan;
        self.request = request;
        self.check_command(command)?;
        let initialized = Box::pin(self.source.initialize_consumer(
            command,
            self.plan.as_ref(),
            self.request.as_ref().ok_or_else(unavailable)?,
            Instant::now() + Duration::from_secs(15),
        ))
        .await?;
        let (command, owner) = initialized.into_parts();
        self.initialized = owner;
        Ok(command)
    }

    fn check_command(&self, command: &ersatzrs_ffmpeg::FfmpegCommand) -> Result<()> {
        self.source
            .check_consumer_command(command, self.plan.as_ref())
    }

    pub(super) fn attach(
        &self,
        child: &mut ersatzrs_infra::remote_stream::RemoteStreamProcess,
    ) -> Result<()> {
        let reservation = self.reservation.as_ref().ok_or_else(unavailable)?.clone();
        if let Some(owner) = &self.initialized {
            child
                .attach_initialized_teletext_bitmap(owner.clone(), reservation)
                .map_err(|_| unavailable())
        } else if self.source.delivery() == SourceTeletextCaptionDelivery::Burn {
            child
                .attach_local_teletext_source(self.source.receipt(), reservation)
                .map_err(|_| unavailable())
        } else {
            Err(unavailable())
        }
    }
}

fn unavailable() -> RepositoryError {
    RepositoryError::unavailable(
        "source teletext captions",
        "source caption selection is unavailable",
    )
}
