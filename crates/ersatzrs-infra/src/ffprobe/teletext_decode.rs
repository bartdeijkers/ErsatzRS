//! Source-associated, exact-page native bitmap decoding (ADR 0121).
//!
//! A receipt proves actual decoder bitmap events within one bounded source span. It
//! neither resolves charset provenance nor promotes stored header observations
//! to ready pages. Selection policy, database ownership and the lifetime of a
//! later playback worker remain the caller's responsibilities.

use std::{path::PathBuf, sync::Arc, time::Duration};

use ersatzrs_core::locking::TeletextWorkHold;
#[cfg(test)]
use ersatzrs_domain::MediaSourceKind;
use ersatzrs_domain::{SourceTeletextBindingIdentity, TeletextPageNumber};
use ersatzrs_ffmpeg::teletext::source_observation::SuppliedTeletextHeaderWindow;
use ersatzrs_ffmpeg::InputFile;
use serde::Deserialize;
use tokio::time::Instant;

use super::{
    local, local_teletext, FfprobeCommand, FfprobeError, FfprobeExtractor, LocalProbeCleanupScope,
    LocalTeletextSourceGuard,
};

pub use super::local_teletext::{
    SourceTeletextAvOriginEvidence, SourceTeletextAvOriginWitness,
    SourceTeletextContinuousClockEvidence, SourceTeletextCueEvidence, SourceTeletextCueRequest,
    SourceTeletextDecodeWindowEvidence, SourceTeletextNativeClockEvidence,
    SourceTeletextPacketSpan, SourceTeletextPacketSpanError, SourceTeletextPagePrelude,
};

#[path = "teletext_decode_bitmap_carriage.rs"]
mod bitmap_carriage;
pub use bitmap_carriage::{
    SourceTeletextBitmapArtifact, SourceTeletextBitmapArtifactGroup,
    SourceTeletextBitmapArtifactObservation, SourceTeletextBitmapFragments,
};

pub(crate) use bitmap_carriage::SourceTeletextBitmapPlaylist;

#[path = "teletext_decode_consumer.rs"]
mod consumer;
#[path = "teletext_decode_demux.rs"]
mod demux;
#[path = "teletext_decode_origin.rs"]
mod origin;
#[path = "teletext_decode_prepared.rs"]
mod prepared;
pub use prepared::PreparedLocalTeletextSource;

#[path = "teletext_decode_text.rs"]
mod text;
pub(crate) use text::SourceTeletextTextPreparation;

const MAX_STARTUP: Duration = Duration::from_secs(15);
const MAX_FRAMES: usize = 4096;
const SOURCE_LABEL: &str = "source teletext page";

/// Explicit page request, not proof of current database ownership or readiness.
/// Provider identities remain intact; syntactic acceptance grants no source I/O
/// authority or assurance that a mapped physical source is current.
/// Subpages remain updates of this same selected page, never separate choices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceTeletextPageDecodeRequest {
    pub binding: SourceTeletextBindingIdentity,
    /// Absolute source stream index, not subtitle ordinal or transport PID.
    pub stream_index: i32,
    pub pid: u16,
    pub program_number: u16,
    pub page: TeletextPageNumber,
}

impl SourceTeletextPageDecodeRequest {
    fn validate(&self) -> Result<(), SourceTeletextPageDecodeError> {
        if self.binding.validate().is_err()
            || self.stream_index < 0
            || !(0x20..=0x1ffe).contains(&self.pid)
            || self.program_number == 0
        {
            return Err(SourceTeletextPageDecodeError::InvalidRequest);
        }
        Ok(())
    }
}

/// Static failures never expose source locators, revision strings or diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SourceTeletextPageDecodeError {
    #[error("source teletext page request is invalid")]
    InvalidRequest,
    #[error("source teletext page decoding requires an owned cleanup scope")]
    NoCleanupScope,
    #[error("source teletext page decode deadline expired")]
    Deadline,
    #[error("source teletext page association is unavailable or changed")]
    SourceChanged,
    #[error("source teletext page decoding was cancelled")]
    Cancelled,
    #[error("source teletext page decoder failed")]
    Decoder,
    #[error("source teletext page decoder cleanup could not be confirmed")]
    CleanupUnconfirmed,
    #[error("source teletext page decoder output is invalid")]
    InvalidOutput,
    #[error("source teletext page stream or program differs from the request")]
    StreamChanged,
    #[error("source teletext decode window has invalid or changing transport associations")]
    InvalidTransportWindow,
    #[error("source teletext page header was not observed in the bounded decode window")]
    UnobservedPage,
    #[error("source teletext page produced no bitmap in the bounded decode window")]
    NoBitmap,
}

/// Actual bitmap events; this contains no guessed charset or forced/default flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceTeletextBitmapEvents {
    pub count: usize,
    /// Decoder timestamps may be absent; absence never becomes a synthetic zero.
    pub first_pts_microseconds: Option<i64>,
    pub last_pts_microseconds: Option<i64>,
}

/// Successful bounded decode retaining the exact held source and requested page.
///
/// The decoder uses automatic region (`-1`); charset provenance remains unknown.
/// The decoder consumes only the same private byte window whose program/PID and
/// stable current PSI were checked. This does not certify the entire page inventory,
/// all later page updates or a source's Level 2.5 semantics. The input getter
/// constructs native bitmap Burn only; it adds no Direct/copy/Next support.
pub struct DecodedLocalTeletextPage {
    request: SourceTeletextPageDecodeRequest,
    original: PathBuf,
    resolved: PathBuf,
    guard: Arc<LocalTeletextSourceGuard>,
    events: SourceTeletextBitmapEvents,
    window: SourceTeletextDecodeWindowEvidence,
    headers: Option<SuppliedTeletextHeaderWindow>,
    input: InputFile,
}

impl std::fmt::Debug for DecodedLocalTeletextPage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DecodedLocalTeletextPage")
            .field("request", &self.request)
            .field("events", &self.events)
            .field("window", &self.window)
            .finish_non_exhaustive()
    }
}

impl DecodedLocalTeletextPage {
    /// Observe every held source packet from zero through EOF under the existing
    /// work owner, retaining only current PSI, raw clock state and a byte hash.
    /// Current programme/PCR association must match this decoded receipt.
    /// The retained decoder bytes are rehashed during that same walk; changed
    /// valid payload cannot silently join an older bitmap receipt.
    ///
    /// This establishes neither a native media origin nor requested cue coverage.
    /// Raw PCR elapsed from its first sample is not media-relative seek time, and
    /// the last PCR is not presentation EOF. Independent bounded decode windows
    /// still need their own real PSI/PES; no source packets are manufactured.
    /// Large/slow sources remain subject to the same absolute owner deadline.
    ///
    /// # Errors
    /// Rejects missing ownership, changed source/PSI, malformed transport or clock
    /// epochs, deadline/cancellation and uncertain worker cleanup.
    pub async fn observe_source_clock(
        &self,
        deadline: Instant,
        hold: TeletextWorkHold,
    ) -> Result<SourceTeletextContinuousClockEvidence, SourceTeletextPageDecodeError> {
        check_deadline(deadline)?;
        if LocalProbeCleanupScope::current().is_none() {
            return Err(SourceTeletextPageDecodeError::NoCleanupScope);
        }
        local_teletext::observe_source_clock_owned(
            self.guard.clone(),
            (self.original.clone(), self.resolved.clone()),
            self.request.pid,
            self.window,
            deadline,
            hold,
        )
        .await
        .map_err(probe_error)
    }

    #[must_use]
    pub fn request(&self) -> &SourceTeletextPageDecodeRequest {
        &self.request
    }

    #[must_use]
    pub const fn events(&self) -> SourceTeletextBitmapEvents {
        self.events
    }

    /// Exact byte-window evidence, not coverage of a later playback seek/open.
    #[must_use]
    pub fn window(&self) -> &SourceTeletextDecodeWindowEvidence {
        &self.window
    }

    /// Actual headers retained by prepared-source decoding from the same bytes
    /// supplied to the bitmap decoder. Completion ordinals are relative to the
    /// decoder snapshot, including its unrelated packets, after prefix omission
    /// and tail trimming. They do not establish playback-time coverage, charset
    /// provenance, complete inventory or readiness of every observed page.
    #[must_use]
    pub fn headers(&self) -> Option<&SuppliedTeletextHeaderWindow> {
        self.headers.as_ref()
    }

    /// Construct the already decoded exact page on its associated physical input.
    /// The consuming worker must retain this receipt through actual cleanup and
    /// call `check_source` immediately before its later source open. This getter
    /// targets the original source, not the temporary decoder snapshot. Later
    /// seek/cue reads need their own transport qualification; this getter alone
    /// neither starts playback nor closes the external-open mutation race.
    #[must_use]
    pub fn native_burn_input(&self) -> InputFile {
        self.input.clone()
    }

    /// Recheck both database locator and actual resolved physical source, using
    /// the existing blocking owner and the caller's shared absolute deadline.
    /// # Errors
    /// Returns an opaque changed-source, cancellation, deadline or cleanup error.
    pub async fn check_source(
        &self,
        deadline: Instant,
        hold: TeletextWorkHold,
    ) -> Result<(), SourceTeletextPageDecodeError> {
        check_deadline(deadline)?;
        if LocalProbeCleanupScope::current().is_none() {
            return Err(SourceTeletextPageDecodeError::NoCleanupScope);
        }
        local_teletext::check_source_guard_owned(
            self.guard.clone(),
            self.original.clone(),
            self.resolved.clone(),
            deadline,
            hold,
        )
        .await
        .map_err(probe_error)
    }
}

impl FfprobeExtractor {
    /// Decode one exact local source page with the existing owned probe runner.
    ///
    /// The caller supplies a current durable binding and admitted work. One
    /// absolute deadline (at most fifteen seconds) includes held source opening,
    /// decode and final source checks. No scope means no worker is started.
    /// The decoder consumes a completed private window of at most 32,768 TS188
    /// packets read from the original held file. The same bytes pass stable PSI
    /// and selected-PES checks before decoding; no later source reopen supplies
    /// decoder bytes. Missing/discontinuous clocks imply no temporal coverage.
    /// No bitmap means none decoded in this window, not absence from the source.
    /// Ordinary metadata, publication and playback remain untouched.
    /// The real process owner retains source and admission after caller loss.
    ///
    /// # Errors
    /// Rejects unsafe/stale source associations, changed stream/program identity,
    /// malformed decoder output, no actual bitmap, timeout or uncertain cleanup.
    pub async fn decode_local_teletext_page(
        &self,
        request: SourceTeletextPageDecodeRequest,
        original: PathBuf,
        resolved: PathBuf,
        deadline: Instant,
        hold: TeletextWorkHold,
    ) -> Result<DecodedLocalTeletextPage, SourceTeletextPageDecodeError> {
        self.decode_local_teletext_window(request, original, resolved, None, deadline, hold)
            .await
    }

    /// Decode an exact page from an explicitly positioned bounded source span.
    ///
    /// A locator supplies only untrusted packet positions. The original held
    /// bytes must contain their own stable current PSI and complete selected
    /// PES, under the same source association, deadline and actual work owner
    /// as the prefix decoder. No preceding PSI is invented or copied in.
    ///
    /// The receipt proves decoded content and byte positions, not media-clock
    /// coverage or suitability for a later playback seek. In particular, a
    /// nonzero span reaching EOF does not prove the whole source. Automatic
    /// playback must qualify its actual interval separately before selecting it.
    ///
    /// # Errors
    /// Rejects out-of-source spans, missing/changing transport associations,
    /// absent bitmaps, stale sources and uncertain cleanup as the prefix API does.
    pub async fn decode_local_teletext_page_at_packet_span(
        &self,
        request: SourceTeletextPageDecodeRequest,
        original: PathBuf,
        resolved: PathBuf,
        span: SourceTeletextPacketSpan,
        deadline: Instant,
        hold: TeletextWorkHold,
    ) -> Result<DecodedLocalTeletextPage, SourceTeletextPageDecodeError> {
        self.decode_local_teletext_window(request, original, resolved, Some(span), deadline, hold)
            .await
    }

    /// Decode an explicitly positioned window using the already prepared held
    /// source. This does not reopen a new source association or require a page
    /// in the initial scan prefix. The window must contain its own stable PSI,
    /// complete selected PES and actual requested-page header before the bitmap
    /// process can start. Native observations remain separate from decoder
    /// timestamps; neither this span nor a successful bitmap grants cue coverage.
    /// # Errors
    /// Rejects stale association, missing ownership/header, invalid transport,
    /// absent bitmaps, deadlines and uncertain actual-worker cleanup.
    pub async fn decode_prepared_local_teletext_page(
        &self,
        source: &PreparedLocalTeletextSource,
        span: SourceTeletextPacketSpan,
        deadline: Instant,
        hold: TeletextWorkHold,
    ) -> Result<DecodedLocalTeletextPage, SourceTeletextPageDecodeError> {
        self.decode_held_local_teletext_window(
            source.request.clone(),
            (
                source.original.clone(),
                source.resolved.clone(),
                source.guard.clone(),
            ),
            (Some(span), true),
            deadline,
            hold,
        )
        .await
    }

    async fn decode_local_teletext_window(
        &self,
        request: SourceTeletextPageDecodeRequest,
        original: PathBuf,
        resolved: PathBuf,
        span: Option<SourceTeletextPacketSpan>,
        deadline: Instant,
        hold: TeletextWorkHold,
    ) -> Result<DecodedLocalTeletextPage, SourceTeletextPageDecodeError> {
        request.validate()?;
        check_deadline(deadline)?;
        if LocalProbeCleanupScope::current().is_none() {
            return Err(SourceTeletextPageDecodeError::NoCleanupScope);
        }
        let guard = local_teletext::open_source_guard_owned(
            original.clone(),
            resolved.clone(),
            request.binding.source_revision.clone(),
            deadline,
            hold.clone(),
        )
        .await
        .map_err(probe_error)?;
        self.decode_held_local_teletext_window(
            request,
            (original, resolved, guard),
            (span, false),
            deadline,
            hold,
        )
        .await
    }

    async fn decode_held_local_teletext_window(
        &self,
        request: SourceTeletextPageDecodeRequest,
        source: (PathBuf, PathBuf, Arc<LocalTeletextSourceGuard>),
        window: (Option<SourceTeletextPacketSpan>, bool),
        deadline: Instant,
        hold: TeletextWorkHold,
    ) -> Result<DecodedLocalTeletextPage, SourceTeletextPageDecodeError> {
        let (original, resolved, guard) = source;
        let (span, observe_headers) = window;
        check_deadline(deadline)?;
        if LocalProbeCleanupScope::current().is_none() {
            return Err(SourceTeletextPageDecodeError::NoCleanupScope);
        }
        let captured = if let Some(span) = span {
            if observe_headers {
                local_teletext::capture_observed_source_packet_span_owned(
                    guard.clone(),
                    (original.clone(), resolved.clone()),
                    request.pid,
                    request.program_number,
                    span,
                    deadline,
                    hold.clone(),
                )
                .await
            } else {
                local_teletext::capture_source_packet_span_owned(
                    guard.clone(),
                    (original.clone(), resolved.clone()),
                    request.pid,
                    request.program_number,
                    span,
                    deadline,
                    hold.clone(),
                )
                .await
            }
        } else {
            local_teletext::capture_source_window_owned(
                guard.clone(),
                original.clone(),
                resolved.clone(),
                request.pid,
                request.program_number,
                deadline,
                hold.clone(),
            )
            .await
        };
        let local_teletext::LocalTeletextSourceWindow {
            snapshot,
            evidence,
            headers,
        } = captured.map_err(probe_error)?;
        if observe_headers
            && !headers.as_ref().is_some_and(|window| {
                window
                    .headers()
                    .iter()
                    .any(|occurrence| occurrence.header.display_page() == Some(request.page))
            })
        {
            return Err(SourceTeletextPageDecodeError::UnobservedPage);
        }
        check_deadline(deadline)?;
        let command = self.teletext_page_decode_command(&request, snapshot.path())?;
        let output = local::capture_retained(
            command,
            PathBuf::from(SOURCE_LABEL),
            Some(deadline),
            Some(local::ProbeRetention::new(hold.clone(), guard.clone()).with_snapshot(snapshot)),
        )
        .await
        .map_err(probe_error)?;
        let events = parse_bitmap_events(&request, &output)?;
        let index = usize::try_from(request.stream_index)
            .map_err(|_| SourceTeletextPageDecodeError::InvalidRequest)?;
        let path = resolved
            .to_str()
            .ok_or(SourceTeletextPageDecodeError::SourceChanged)?;
        let input = InputFile::teletext_subtitle(path, index, request.page);
        let decoded = DecodedLocalTeletextPage {
            request,
            original,
            resolved,
            guard,
            events,
            window: evidence,
            headers,
            input,
        };
        decoded.check_source(deadline, hold).await?;
        Ok(decoded)
    }

    fn teletext_page_decode_command(
        &self,
        request: &SourceTeletextPageDecodeRequest,
        resolved: &std::path::Path,
    ) -> Result<FfprobeCommand, SourceTeletextPageDecodeError> {
        let path = resolved
            .to_str()
            .ok_or(SourceTeletextPageDecodeError::SourceChanged)?;
        Ok(FfprobeCommand { program: self.ffprobe_path.clone(), args: vec![
            "-v".into(), "error".into(),
            format!("-txt_page:{}", request.stream_index), request.page.get().to_string(),
            format!("-txt_format:{}", request.stream_index), "bitmap".into(),
            format!("-txt_default_region:{}", request.stream_index), "-1".into(),
            "-select_streams".into(), request.stream_index.to_string(),
            "-show_frames".into(), "-show_streams".into(), "-show_programs".into(),
            "-show_entries".into(),
            "stream=index,codec_name,codec_type,id:program=program_id,program_num,pmt_pid,pcr_pid:program_stream=index,codec_name,codec_type,id:frame=media_type,pts,format,num_rects".into(),
            "-of".into(), "json".into(), "-f".into(), "mpegts".into(), path.into(),
        ] })
    }
}

fn check_deadline(deadline: Instant) -> Result<(), SourceTeletextPageDecodeError> {
    let now = Instant::now();
    if deadline <= now {
        return Err(SourceTeletextPageDecodeError::Deadline);
    }
    if deadline.duration_since(now) > MAX_STARTUP {
        return Err(SourceTeletextPageDecodeError::InvalidRequest);
    }
    Ok(())
}

fn probe_error(error: FfprobeError) -> SourceTeletextPageDecodeError {
    match error {
        FfprobeError::Cancelled { .. } => SourceTeletextPageDecodeError::Cancelled,
        FfprobeError::CleanupUnconfirmed { .. } => {
            SourceTeletextPageDecodeError::CleanupUnconfirmed
        }
        FfprobeError::Timeout { .. } => SourceTeletextPageDecodeError::Deadline,
        FfprobeError::InvalidOutput { path } if path == "local teletext source association" => {
            SourceTeletextPageDecodeError::SourceChanged
        }
        FfprobeError::InvalidOutput { path } if path == "source teletext decode window" => {
            SourceTeletextPageDecodeError::InvalidTransportWindow
        }
        _ => SourceTeletextPageDecodeError::Decoder,
    }
}

#[derive(Deserialize)]
struct Output {
    streams: Vec<Stream>,
    programs: Vec<Program>,
    frames: Vec<Frame>,
}
#[derive(Deserialize)]
struct Stream {
    index: i32,
    codec_name: String,
    codec_type: String,
    id: String,
}
#[derive(Deserialize)]
struct Program {
    program_num: u16,
    streams: Vec<Stream>,
}
#[derive(Deserialize)]
struct Frame {
    media_type: String,
    pts: Option<i64>,
    format: u8,
    num_rects: u32,
}

fn parse_bitmap_events(
    request: &SourceTeletextPageDecodeRequest,
    bytes: &[u8],
) -> Result<SourceTeletextBitmapEvents, SourceTeletextPageDecodeError> {
    use SourceTeletextPageDecodeError::{InvalidOutput, NoBitmap, StreamChanged};
    let output: Output = serde_json::from_slice(bytes).map_err(|_| InvalidOutput)?;
    if output.frames.len() > MAX_FRAMES
        || output.programs.len() > 128
        || output.streams.len() != 1
        || output.programs.iter().any(|p| p.streams.len() > 1)
    {
        return Err(InvalidOutput);
    }
    let matches = |s: &Stream| {
        s.index == request.stream_index
            && s.codec_name == "dvb_teletext"
            && s.codec_type == "subtitle"
            && s.id
                .strip_prefix("0x")
                .filter(|id| {
                    !id.is_empty() && id.len() <= 4 && id.bytes().all(|b| b.is_ascii_hexdigit())
                })
                .and_then(|id| u16::from_str_radix(id, 16).ok())
                == Some(request.pid)
    };
    let mut memberships = output
        .programs
        .iter()
        .filter(|p| p.program_num == request.program_number);
    let selected = memberships.next().ok_or(StreamChanged)?;
    if !matches(&output.streams[0])
        || memberships.next().is_some()
        || selected.streams.len() != 1
        || !matches(&selected.streams[0])
    {
        return Err(StreamChanged);
    }
    let mut events = SourceTeletextBitmapEvents {
        count: 0,
        first_pts_microseconds: None,
        last_pts_microseconds: None,
    };
    for frame in output.frames {
        if frame.media_type != "subtitle" || frame.format != 0 || frame.num_rects > 4096 {
            return Err(InvalidOutput);
        }
        // A clear/empty subtitle update does not prove any bitmap body decoded.
        if frame.num_rects == 0 {
            continue;
        }
        if events.count == 0 {
            events.first_pts_microseconds = frame.pts;
        }
        events.count += 1;
        events.last_pts_microseconds = frame.pts;
    }
    if events.count == 0 {
        return Err(NoBitmap);
    }
    Ok(events)
}

#[cfg(test)]
#[path = "teletext_decode_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "teletext_decode_seek_tests.rs"]
mod seek_tests;

#[cfg(test)]
#[path = "teletext_decode_prepared_tests.rs"]
mod prepared_tests;
