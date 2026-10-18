//! Source-only native caption ownership on the actual audiovisual consumer.

use std::sync::Arc;

use crate::{
    ffprobe::teletext_decode::DecodedLocalTeletextPage,
    playback::source_teletext::runtime::InitializedSourceTeletextBitmap,
    transcode_capacity::ActiveTranscodeReservation,
};

use super::RemoteStreamProcess;

/// Static attachment failures never include media paths or process diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SourceCaptionAttachmentError {
    #[error("local source captions cannot attach to a remote producer")]
    Producer,
    #[error("a local source caption is already attached to this consumer")]
    AlreadyAttached,
    #[error("local source captions require a consumer that has not exited")]
    Exited,
    #[error("local source caption consumer state is unavailable")]
    ProcessState,
    #[error("initialized source captions belong to a different consumer command")]
    Command,
}

/// The logical command before the existing filter-graph transport rewrite.
pub(super) struct ConsumerCommandSeal(pub(super) ersatzrs_ffmpeg::FfmpegCommand);
impl std::fmt::Debug for ConsumerCommandSeal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ConsumerCommandSeal(<redacted>)")
    }
}

/// Hold source before capacity in normal field destruction. Preparation work
/// admission is deliberately absent: playback uses its existing transcode slot.
pub(super) struct SourceCaptionRetention {
    source: Option<Arc<DecodedLocalTeletextPage>>,
    bitmap: Option<Arc<InitializedSourceTeletextBitmap>>,
    capacity: ActiveTranscodeReservation,
    reaped: bool,
}

impl std::fmt::Debug for SourceCaptionRetention {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceCaptionRetention")
            .field("reaped", &self.reaped)
            .field("initialized_bitmap", &self.bitmap.is_some())
            .finish_non_exhaustive()
    }
}

impl SourceCaptionRetention {
    pub(super) fn confirm_reaped(&mut self) {
        self.reaped = true;
    }
}

impl Drop for SourceCaptionRetention {
    fn drop(&mut self) {
        if !self.reaped {
            self.capacity.retain_until_restart();
            if let Some(source) = self.source.take() {
                // Cleanup is unconfirmed, so the consuming worker may still
                // need this association. Its already quarantined transcode
                // slot bounds retained owners until the host restarts.
                std::mem::forget(source);
            }
            if let Some(bitmap) = self.bitmap.take() {
                std::mem::forget(bitmap);
            }
        }
    }
}

impl RemoteStreamProcess {
    /// Attach a validated local caption receipt immediately after consumer spawn.
    ///
    /// This retains the actual playback reservation, never the short-lived
    /// teletext preparation admission. Successful `wait` or `kill` confirms
    /// consumer reaping; sending a kill signal, cancelling a wait, panic or Drop
    /// does not. Unconfirmed disposal quarantines capacity and retains the source
    /// association until restart. Every retry child needs its own attachment.
    ///
    /// The caller must validate the actual command input, current database/file
    /// binding and playback seek immediately before spawn. A bounded preparation
    /// receipt does not establish later seek coverage, readiness or source-byte
    /// immutability. This method neither rewrites input nor blesses provider media.
    ///
    /// # Errors
    /// Rejects remote producers, duplicate attachment and an exited consumer
    /// without installing an owner. An unavailable process state installs an
    /// unconfirmed owner before returning `ProcessState`: the caller must await
    /// consumer cleanup, and abandonment retains the source and quarantines its
    /// reservation. A later successful wait/kill still confirms normal disposal.
    pub fn attach_local_teletext_source(
        &mut self,
        source: Arc<DecodedLocalTeletextPage>,
        capacity: ActiveTranscodeReservation,
    ) -> Result<(), SourceCaptionAttachmentError> {
        self.attach_caption_owner(source, None, capacity)
    }

    /// Retain an initialized bitmap owner while checking its generated consumer.
    ///
    /// The owner includes all private concat and fragment files. Source checks
    /// must immediately precede spawn; this synchronous transfer adds no await.
    /// Cleanup and quarantine semantics are identical to the existing Burn owner.
    /// # Errors
    /// Rejects a different command, producer, duplicate or exited child. On the
    /// first live attachment, both a command mismatch and uncertain process state
    /// retain the owner before returning an error. The caller must reap the child;
    /// abandonment quarantines its files and capacity, granting no playback authority.
    pub fn attach_initialized_teletext_bitmap(
        &mut self,
        bitmap: Arc<InitializedSourceTeletextBitmap>,
        capacity: ActiveTranscodeReservation,
    ) -> Result<(), SourceCaptionAttachmentError> {
        let matches = bitmap.matches_command(&self.consumer_command.0);
        self.attach_caption_owner(bitmap.receipt(), Some(bitmap), capacity)?;
        if !matches {
            return Err(SourceCaptionAttachmentError::Command);
        }
        Ok(())
    }

    fn attach_caption_owner(
        &mut self,
        source: Arc<DecodedLocalTeletextPage>,
        bitmap: Option<Arc<InitializedSourceTeletextBitmap>>,
        capacity: ActiveTranscodeReservation,
    ) -> Result<(), SourceCaptionAttachmentError> {
        if self.producer_pipe.is_some() {
            return Err(SourceCaptionAttachmentError::Producer);
        }
        if self.source_caption.is_some() {
            return Err(SourceCaptionAttachmentError::AlreadyAttached);
        }
        let state = self.child.try_wait();
        if state.as_ref().is_ok_and(Option::is_some) {
            return Err(SourceCaptionAttachmentError::Exited);
        }
        self.source_caption = Some(SourceCaptionRetention {
            source: Some(source),
            bitmap,
            capacity,
            reaped: false,
        });
        state
            .map(|_| ())
            .map_err(|_| SourceCaptionAttachmentError::ProcessState)
    }
}

#[cfg(test)]
#[path = "remote_stream_source_caption/tests.rs"]
mod tests;

#[cfg(test)]
mod command_seal_tests {
    use super::ConsumerCommandSeal;

    #[test]
    fn spawned_consumer_seal_does_not_disclose_arguments_or_environment() {
        let command = ersatzrs_ffmpeg::FfmpegCommand::new_with_executable(
            "synthetic-native-executable",
            vec![],
            vec!["-i".into(), "synthetic-private-caption.ffconcat".into()],
        );
        let seal = ConsumerCommandSeal(command);
        let debug = format!("{seal:?}");
        assert!(!debug.contains("synthetic-native-executable"));
        assert!(!debug.contains("synthetic-private-caption"));
        assert_eq!(debug, "ConsumerCommandSeal(<redacted>)");
    }
}
