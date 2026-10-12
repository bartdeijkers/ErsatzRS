//! Native encoding of complete channel candidates, without service publication.
//!
//! The retained immutable draft is the browser-facing page source; transport
//! bytes are encoded from exactly that draft. Service admission, trusted
//! generation ownership and durable promotion remain separate obligations.

use std::{fmt, time::Instant};

use ersatzrs_app::{
    teletext_channels::{ChannelTeletextPagesError, PreparedChannelTeletextPages},
    teletext_guide::SuppliedTeletextGuideContext,
    ApplicationServices,
};
use ersatzrs_domain::{EntityId, TeletextChannelSelectionSnapshot, TeletextPageSetDraft};
use ersatzrs_ffmpeg::teletext::{
    encode_teletext_page_set, EncodedTeletextPageSet, TeletextEncodingError,
    TeletextEncodingRegion, TeletextServiceDescriptor,
};

/// One complete encoded candidate or an explicit disabled candidate.
///
/// Fields are private: the retained draft, encoded pages and descriptor cannot
/// be replaced independently. This grants no publication or playback readiness.
pub struct EncodedChannelTeletextCandidate {
    prepared: PreparedChannelTeletextPages,
    encoded: Option<EncodedTeletextPageSet>,
    descriptor: Option<TeletextServiceDescriptor>,
}

impl EncodedChannelTeletextCandidate {
    /// Exact output binding membership and configuration observed at preparation.
    #[must_use]
    pub fn selection(&self) -> &TeletextChannelSelectionSnapshot {
        self.prepared.selection()
    }

    /// Opaque request correlation, without trusted generation ownership.
    #[must_use]
    pub fn generation(&self) -> &str {
        self.prepared.generation()
    }

    /// Explicit entry binding; disabled candidates have no entry.
    #[must_use]
    pub fn entry_addon_id(&self) -> Option<&str> {
        self.prepared.entry_addon_id()
    }

    /// The exact immutable logical pages used by the native encoder.
    #[must_use]
    pub fn draft(&self) -> Option<&TeletextPageSetDraft> {
        self.prepared.draft()
    }

    /// Complete native pages; disabled candidates carry no encoded pages.
    #[must_use]
    pub fn encoded(&self) -> Option<&EncodedTeletextPageSet> {
        self.encoded.as_ref()
    }

    /// Complete host-derived signalling, without truncating subtitle entries.
    #[must_use]
    pub fn descriptor(&self) -> Option<&TeletextServiceDescriptor> {
        self.descriptor.as_ref()
    }
}

impl fmt::Debug for EncodedChannelTeletextCandidate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EncodedChannelTeletextCandidate")
            .field("selection", self.selection())
            .field("enabled", &self.encoded.is_some())
            .finish_non_exhaustive()
    }
}

/// Static preparation failures; no raw programme/configuration values are held.
#[derive(Debug, thiserror::Error)]
pub enum ChannelTeletextEncodingError {
    #[error(transparent)]
    Work(#[from] super::teletext_work::TeletextWorkError),
    #[error(transparent)]
    Preparation(#[from] ChannelTeletextPagesError),
    #[error(transparent)]
    Encoding(#[from] TeletextEncodingError),
    #[error("teletext channel preparation exceeded its deadline")]
    TimedOut,
    #[error("teletext channel encoder worker failed")]
    WorkerFailed,
}

/// Prepare every enabled base binding and encode its complete aggregate.
///
/// The explicit entry binding determines the initial display page and service
/// language. Encoding runs off async workers and is joined as a whole; no partial
/// result escapes a later charset/control failure or expired original budget.
/// Dropping the caller preserves existing native producer cleanup ownership.
/// This function does not run as an HTTP action or admit/publish a live service.
///
/// # Errors
/// Returns exact preparation/encoding failures or a static worker/deadline error.
pub async fn prepare_channel_teletext_candidate(
    application: &ApplicationServices,
    channel_id: EntityId,
    entry_addon_id: Option<&str>,
    context: &SuppliedTeletextGuideContext<'_>,
    region: TeletextEncodingRegion,
) -> Result<EncodedChannelTeletextCandidate, ChannelTeletextEncodingError> {
    let deadline = Instant::now()
        .checked_add(ersatzrs_app::teletext_channels::PREPARATION_TIMEOUT)
        .ok_or(ChannelTeletextEncodingError::TimedOut)?;
    let guard = super::teletext_work::begin(application, Some(deadline)).await?;
    let hold = guard.hold_work();
    let prepared = application
        .prepare_channel_teletext_pages_with_hold(
            channel_id,
            entry_addon_id,
            context,
            deadline,
            hold.clone(),
        )
        .await?;
    super::teletext_work::spawn_blocking(hold, move || encode_prepared(prepared, region))
        .await
        .map_err(|_| ChannelTeletextEncodingError::WorkerFailed)?
}

fn encode_prepared(
    prepared: PreparedChannelTeletextPages,
    region: TeletextEncodingRegion,
) -> Result<EncodedChannelTeletextCandidate, ChannelTeletextEncodingError> {
    check_deadline(prepared.deadline())?;
    let encoded = prepared
        .draft()
        .map(|draft| encode_teletext_page_set(draft, region))
        .transpose()?;
    let descriptor = encoded
        .as_ref()
        .map(TeletextServiceDescriptor::from_page_set);
    check_deadline(prepared.deadline())?;
    Ok(EncodedChannelTeletextCandidate {
        prepared,
        encoded,
        descriptor,
    })
}

fn check_deadline(deadline: Instant) -> Result<(), ChannelTeletextEncodingError> {
    if Instant::now() >= deadline {
        Err(ChannelTeletextEncodingError::TimedOut)
    } else {
        Ok(())
    }
}
