//! Optional source captions can fail closed without interrupting ordinary AV.

use std::sync::Arc;

use ersatzrs_core::repositories::RepositoryError;
use ersatzrs_ffmpeg::{FfmpegCommand, InputFile, OutputFormat, OutputMetadata};
use ersatzrs_infra::{
    playback::subtitle_input_for_stream, remote_stream::RemoteStreamCommand,
    subtitles::rebase_burned_text_subtitle_input, transcode_capacity::ActiveTranscodeReservation,
};
use tokio::sync::Notify;

use super::{
    cancel_stream_retry_attempt, source_teletext, trimmed_string, LegacyStreamLifetime,
    LegacyWrapperItemCancellation, StreamDenialRetry, StreamRetryAttempt, StreamSpawnError,
};

#[derive(Clone)]
pub(super) struct OrdinaryCaption {
    pub(super) input: Option<InputFile>,
    pub(super) metadata: OutputMetadata,
}

pub(super) async fn prepare(
    context: source_teletext::SelectionContext<'_>,
    output_format: OutputFormat,
    start_seconds: i64,
    mut metadata: OutputMetadata,
) -> ersatzrs_core::repositories::Result<OrdinaryCaption> {
    let selected = tokio::time::timeout_at(
        context.deadline,
        ersatzrs_infra::playback::source_teletext::select_native_playback_subtitle_for_version(
            &context.state.pool,
            context.active.media_item_id,
            context.active.media_version_id,
            context.languages,
            context.subtitle_mode,
            context.streaming_mode,
            &[],
        ),
    )
    .await
    .map_err(|_| {
        RepositoryError::unavailable(
            "source teletext captions",
            "ordinary caption selection timed out",
        )
    })??;
    let subtitle = selected.map(|selected| selected.into_parts().0);
    metadata.subtitle_language = subtitle
        .as_ref()
        .and_then(|subtitle| subtitle.language.as_deref())
        .and_then(trimmed_string);
    metadata.subtitle_title = subtitle
        .as_ref()
        .and_then(|subtitle| subtitle.title.as_deref())
        .and_then(trimmed_string);
    let input = subtitle.as_ref().and_then(|subtitle| {
        subtitle_input_for_stream(
            subtitle,
            context.resolved,
            output_format,
            context.streaming_mode,
        )
    });
    let input = rebase_burned_text_subtitle_input(input, start_seconds).await;
    Ok(OrdinaryCaption { input, metadata })
}

/// Returns the initialized source-caption command or its ordinary replacement.
/// The caller seals output/graph identity after choosing this actual command.
pub(super) async fn before_open(
    command: &FfmpegCommand,
    context: &mut StreamDenialRetry,
    reservation: Option<&ActiveTranscodeReservation>,
    deadline: tokio::time::Instant,
    cancel: Option<&Arc<Notify>>,
    lifetime: Option<&LegacyStreamLifetime>,
) -> Result<Option<FfmpegCommand>, StreamSpawnError> {
    let Some(source) = context.source_caption.as_mut() else {
        return Ok(None);
    };
    let checked = tokio::select! {
        biased;
        () = cancelled_open(cancel, lifetime) => return Err(cancelled()),
        checked = source.before_open(command, reservation, deadline) => checked,
    };
    if lifetime.is_some_and(LegacyStreamLifetime::item_cancelled) {
        return Err(cancelled());
    }
    if let Ok(command) = checked {
        update_command(context, &command);
        return Ok(Some(command));
    }
    let command = ordinary_command(context.ordinary_caption_fallback.take())?;
    context.source_caption = None;
    update_command(context, &command);
    tracing::warn!("source caption rejected before open; retaining ordinary playback");
    Ok(Some(command))
}

pub(super) async fn before_retry(
    mut attempt: StreamRetryAttempt,
    context: &mut StreamDenialRetry,
    producer: Option<&RemoteStreamCommand>,
    cancel: &Arc<Notify>,
    item: Option<&LegacyWrapperItemCancellation>,
) -> Result<StreamRetryAttempt, StreamSpawnError> {
    if item.is_some_and(LegacyWrapperItemCancellation::is_cancelled) {
        cancel_stream_retry_attempt(attempt).await?;
        return Err(cancelled());
    }
    let source_unavailable = if let Some(source) = context.source_caption.as_mut() {
        if producer.is_some() || attempt.prepared.is_some() {
            cancel_stream_retry_attempt(attempt).await?;
            return Err(StreamSpawnError::Spawn(
                "source teletext retry cannot use a remote producer".into(),
            ));
        }
        let checked = tokio::select! {
            biased;
            () = cancelled_retry(cancel, item) => {
                cancel_stream_retry_attempt(attempt).await?;
                return Err(cancelled());
            },
            checked = source.before_retry(
                &attempt.command,
                attempt.source_caption_plan.take(),
                attempt.source_caption_request.take(),
            ) => checked,
        };
        if item.is_some_and(LegacyWrapperItemCancellation::is_cancelled) {
            cancel_stream_retry_attempt(attempt).await?;
            return Err(cancelled());
        }
        match checked {
            Ok(command) => {
                attempt.command = command;
                false
            }
            Err(_) => true,
        }
    } else {
        // A previous open may already have rejected source captions. The new
        // fallback was rebuilt with this attempt's learned capabilities.
        attempt.ordinary_caption_fallback.is_some()
    };
    if source_unavailable {
        attempt.command = ordinary_command(attempt.ordinary_caption_fallback.take())?;
        attempt.source_caption_plan = None;
        attempt.source_caption_request = None;
        context.source_caption = None;
        tracing::warn!("source caption unavailable before retry; retaining ordinary playback");
    }
    update_command(context, &attempt.command);
    Ok(attempt)
}

fn ordinary_command(command: Option<FfmpegCommand>) -> Result<FfmpegCommand, StreamSpawnError> {
    command.ok_or_else(|| {
        StreamSpawnError::Spawn("ordinary source-caption fallback was not prepared".into())
    })
}

fn update_command(context: &mut StreamDenialRetry, command: &FfmpegCommand) {
    context.graph_operation = ersatzrs_ffmpeg::selected_hardware_graph(command);
    context.ffmpeg_path.clone_from(&command.executable);
}

fn cancelled() -> StreamSpawnError {
    StreamSpawnError::Spawn("legacy source caption startup cancelled".into())
}

async fn cancelled_open(cancel: Option<&Arc<Notify>>, lifetime: Option<&LegacyStreamLifetime>) {
    tokio::select! {
        biased;
        () = async {
            if let Some(cancel) = cancel { cancel.notified().await; }
            else { std::future::pending::<()>().await; }
        } => {},
        () = async {
            if let Some(lifetime) = lifetime { lifetime.cancellation().await; }
            else { std::future::pending::<()>().await; }
        } => {},
    }
}

async fn cancelled_retry(cancel: &Arc<Notify>, item: Option<&LegacyWrapperItemCancellation>) {
    tokio::select! {
        biased;
        () = cancel.notified() => {},
        () = async {
            if let Some(item) = item { item.cancelled().await; }
            else { std::future::pending::<()>().await; }
        } => {},
    }
}

#[cfg(test)]
mod tests {
    use ersatzrs_ffmpeg::{FfmpegCapabilityReport, FfmpegRuntimeReport};
    use ersatzrs_infra::db::DbPool;
    use tokio::sync::RwLock;

    use super::*;

    fn command(executable: &str) -> FfmpegCommand {
        FfmpegCommand::new_with_executable(executable, Vec::new(), vec!["-an".into()])
    }

    fn context() -> StreamDenialRetry {
        StreamDenialRetry {
            source_caption: None,
            ordinary_caption_fallback: Some(command("obsolete-initial-capabilities")),
            song: None,
            pool: DbPool::connect_lazy("sqlite::memory:").expect("lazy pool"),
            shared_ffmpeg_runtime: Arc::new(RwLock::new(FfmpegRuntimeReport::for_tests())),
            runtime_capabilities: FfmpegCapabilityReport::default(),
            acceleration: ersatzrs_domain::HardwareAccelerationKind::None,
            source_stream: None,
            graph_operation: None,
            ffmpeg_path: "source-primary".into(),
            rebuild: None,
        }
    }

    #[tokio::test]
    async fn rejected_source_hardware_retry_uses_fresh_ordinary_command_without_sql() {
        let mut context = context();
        context.pool.close().await;
        let attempt = StreamRetryAttempt {
            source_caption_plan: None,
            source_caption_request: None,
            command: command("fresh-source-primary"),
            ordinary_caption_fallback: Some(command("fresh-ordinary-hardware-fallback")),
            source_stream: None,
            prepared: None,
        };
        let selected = before_retry(attempt, &mut context, None, &Arc::new(Notify::new()), None)
            .await
            .expect("ordinary retry");
        assert_eq!(
            selected.command.executable,
            "fresh-ordinary-hardware-fallback"
        );
        assert_eq!(context.ffmpeg_path, selected.command.executable);
        assert_eq!(selected.command.arguments, ["-an"]);
        assert!(context.source_caption.is_none());
    }

    #[tokio::test]
    async fn ordinary_hardware_retry_retains_primary_command_without_sql() {
        let mut context = context();
        context.pool.close().await;
        let attempt = StreamRetryAttempt {
            source_caption_plan: None,
            source_caption_request: None,
            command: command("ordinary-primary"),
            ordinary_caption_fallback: None,
            source_stream: None,
            prepared: None,
        };
        let selected = before_retry(attempt, &mut context, None, &Arc::new(Notify::new()), None)
            .await
            .expect("ordinary retry");
        assert_eq!(selected.command.executable, "ordinary-primary");
        assert_eq!(context.ffmpeg_path, selected.command.executable);
    }

    #[tokio::test]
    async fn source_open_cancellation_observes_stored_notification() {
        let cancel = Arc::new(Notify::new());
        cancel.notify_one();
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            cancelled_open(Some(&cancel), None),
        )
        .await
        .expect("stored response cancellation must be observed before source open");
    }
}
