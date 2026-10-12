//! Lossless optional startup admission; injected output epochs remain strict.

use std::{
    io,
    pin::Pin,
    sync::{atomic::AtomicU8, Arc, Mutex},
    task::{Context, Poll},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _, ReadBuf},
    sync::Notify,
    time::Instant,
};

use super::{
    retained_stream_buffer::{forward_retained_ts, ReservePolicy},
    teletext_binding::TeletextOutputBinding,
    teletext_output::forward_teletext_output_seeded,
    teletext_output::{forward_teletext_output_with_control, TeletextPumpError},
    teletext_retained_output::{
        allocation_model, forward_retained_ts_with_startup, forward_retained_ts_with_teletext,
        processing_cap, RetainedTeletextError, RetainedTeletextInput,
    },
    teletext_startup::TeletextStartupPrefix,
};

const PACKET_BYTES: usize = 188;
const READ_BYTES: usize = 256 * PACKET_BYTES;
const MAX_BYTES: usize = 32 * 1024 * 1024;

/// Result of one optional output epoch, with content-free startup rejection.
#[derive(Debug)]
pub struct OptionalTeletextOutput {
    pub bytes: u64,
    pub rejection: Option<TeletextPumpError>,
}

#[derive(Debug)]
pub(super) struct OptionalRetainedTeletextOutput {
    pub bytes: u64,
    pub rejection: Option<RetainedTeletextError>,
}

struct Startup {
    bytes: Vec<u8>,
    maximum: usize,
    committed: bool,
    capacity_reached: bool,
    received: u64,
}
impl Startup {
    fn new(
        maximum: usize,
        binding: &TeletextOutputBinding,
    ) -> Result<Arc<Mutex<Self>>, TeletextPumpError> {
        let capacity = binding
            .config()
            .max_buffer_packets
            .checked_mul(PACKET_BYTES)
            .ok_or(TeletextPumpError::Configuration)?;
        if maximum == 0 || maximum > capacity || maximum > MAX_BYTES {
            return Err(TeletextPumpError::Configuration);
        }
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(maximum)
            .map_err(|_| TeletextPumpError::Configuration)?;
        Ok(Arc::new(Mutex::new(Self {
            bytes,
            maximum,
            committed: false,
            capacity_reached: false,
            received: 0,
        })))
    }
    fn take(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.bytes)
    }

    fn seeded(
        maximum: usize,
        binding: &TeletextOutputBinding,
        prefix: &TeletextStartupPrefix,
    ) -> Result<Arc<Mutex<Self>>, TeletextPumpError> {
        if prefix.bytes().len() > maximum {
            return Err(TeletextPumpError::Configuration);
        }
        let state = Self::new(maximum, binding)?;
        {
            let mut journal = state.lock().unwrap();
            journal.bytes.extend_from_slice(prefix.bytes());
            journal.received = u64::try_from(prefix.bytes().len())
                .map_err(|_| TeletextPumpError::Configuration)?;
        }
        Ok(state)
    }
}

fn read_scratch() -> Result<Vec<u8>, TeletextPumpError> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(READ_BYTES)
        .map_err(|_| TeletextPumpError::Configuration)?;
    bytes.resize(READ_BYTES, 0);
    Ok(bytes)
}

struct TeeReader<'a, R> {
    source: &'a mut R,
    state: Arc<Mutex<Startup>>,
    scratch: Option<Vec<u8>>,
}
impl<'a, R> TeeReader<'a, R> {
    fn new(source: &'a mut R, state: &Arc<Mutex<Startup>>) -> Result<Self, TeletextPumpError> {
        let scratch = read_scratch()?;
        Ok(Self {
            source,
            state: Arc::clone(state),
            scratch: Some(scratch),
        })
    }
}
impl<R: AsyncRead + Unpin> AsyncRead for TeeReader<'_, R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        output: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if output.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        let (committed, available) = {
            let state = this.state.lock().unwrap();
            (state.committed, state.maximum - state.bytes.len())
        };
        if committed {
            this.scratch = None;
            let before = output.filled().len();
            let result = Pin::new(&mut *this.source).poll_read(cx, output);
            if matches!(result, Poll::Ready(Ok(()))) {
                let mut state = this.state.lock().unwrap();
                let Some(count) = state
                    .received
                    .checked_add(u64::try_from(output.filled().len() - before).unwrap())
                else {
                    return Poll::Ready(Err(io::Error::other("teletext byte count overflow")));
                };
                state.received = count;
            }
            return result;
        }
        if available == 0 {
            this.state.lock().unwrap().capacity_reached = true;
            return Poll::Ready(Err(io::Error::other("teletext startup journal capacity")));
        }
        let scratch = this.scratch.as_mut().unwrap();
        let length = available.min(output.remaining()).min(scratch.len());
        let mut input = ReadBuf::new(&mut scratch[..length]);
        match Pin::new(&mut *this.source).poll_read(cx, &mut input) {
            Poll::Ready(Ok(())) => {
                let bytes = input.filled();
                let mut state = this.state.lock().unwrap();
                let Some(count) = state
                    .received
                    .checked_add(u64::try_from(bytes.len()).unwrap())
                else {
                    return Poll::Ready(Err(io::Error::other("teletext byte count overflow")));
                };
                state.received = count;
                state.bytes.extend_from_slice(bytes);
                drop(state);
                output.put_slice(bytes);
                Poll::Ready(Ok(()))
            }
            other => other,
        }
    }
}

struct CommitWriter<'a, W> {
    sink: &'a mut W,
    state: Arc<Mutex<Startup>>,
}
impl<W: AsyncWrite + Unpin> AsyncWrite for CommitWriter<'_, W> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        if !bytes.is_empty() {
            let mut state = this.state.lock().unwrap();
            state.committed = true;
            drop(state.take());
        }
        Pin::new(&mut *this.sink).poll_write(cx, bytes)
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.get_mut().sink).poll_flush(cx)
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.get_mut().sink).poll_shutdown(cx)
    }
}

fn can_fallback(state: &Startup, error: &TeletextPumpError) -> bool {
    !state.committed
        && (matches!(
            error,
            TeletextPumpError::Gate(_) | TeletextPumpError::Configuration
        ) || (state.capacity_reached && matches!(error, TeletextPumpError::Read(_))))
}

pub(super) async fn forward_bound_teletext_output<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    source: &mut R,
    sink: &mut W,
    binding: &TeletextOutputBinding,
) -> Result<u64, TeletextPumpError> {
    if binding.source_discovery().is_some() {
        return super::teletext_discovery_output::forward_discovered_direct(source, sink, binding)
            .await;
    }
    if let Some(maximum) = binding.optional_startup_bytes() {
        let result = forward_optional_teletext_output(source, sink, binding, maximum).await?;
        if let Some(rejection) = result.rejection {
            tracing::debug!(%rejection, "optional teletext startup rejected; preserving raw playback");
        }
        Ok(result.bytes)
    } else {
        forward_teletext_output_with_control(
            source,
            sink,
            binding.config(),
            binding.content().clone(),
            binding.first_byte_timeout(),
            binding.cloned_control(),
        )
        .await
    }
}

pub(super) async fn forward_bound_retained_teletext_output(
    source: &mut (impl AsyncRead + Unpin),
    sink: &mut (impl AsyncWrite + Unpin),
    policy: ReservePolicy,
    terminal: &AtomicU8,
    startup: Instant,
    cancel: &Notify,
    binding: &TeletextOutputBinding,
) -> Result<(), RetainedTeletextError> {
    if binding.source_discovery().is_some() {
        return super::teletext_discovery_output::forward_discovered_retained(
            source, sink, policy, terminal, startup, cancel, binding,
        )
        .await;
    }
    if let Some(maximum_bytes) = binding.optional_startup_bytes() {
        let result = forward_optional_retained_teletext_output(
            source,
            sink,
            policy,
            terminal,
            startup,
            cancel,
            OptionalTeletextStartup {
                binding,
                maximum_bytes,
            },
        )
        .await?;
        if let Some(rejection) = result.rejection {
            tracing::debug!(%rejection, forwarded_bytes = result.bytes, "optional retained teletext startup rejected; preserving raw playback");
        }
        Ok(())
    } else {
        forward_retained_ts_with_teletext(source, sink, policy, terminal, startup, cancel, binding)
            .await
    }
}

/// Preserve exact consumed raw bytes if optional authored startup admission fails.
/// Existing strict forwarding and every post-write failure remain unchanged.
/// # Errors
/// Invalid journal policy, actual source/sink failures and committed epoch failures.
pub async fn forward_optional_teletext_output<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    source: &mut R,
    sink: &mut W,
    binding: &TeletextOutputBinding,
    startup_bytes: usize,
) -> Result<OptionalTeletextOutput, TeletextPumpError> {
    let state = Startup::new(startup_bytes, binding)?;
    let result = {
        let mut reader = TeeReader::new(source, &state)?;
        let mut writer = CommitWriter {
            sink,
            state: Arc::clone(&state),
        };
        forward_teletext_output_with_control(
            &mut reader,
            &mut writer,
            binding.config(),
            binding.content().clone(),
            binding.first_byte_timeout(),
            binding.cloned_control(),
        )
        .await
    };
    finish_direct(source, sink, &state, result).await
}

async fn finish_direct<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    source: &mut R,
    sink: &mut W,
    state: &Arc<Mutex<Startup>>,
    result: Result<u64, TeletextPumpError>,
) -> Result<OptionalTeletextOutput, TeletextPumpError> {
    match result {
        Ok(bytes) => Ok(OptionalTeletextOutput {
            bytes,
            rejection: None,
        }),
        Err(error) => {
            let prefix = {
                let mut state = state.lock().unwrap();
                if !can_fallback(&state, &error) {
                    return Err(error);
                }
                state.take()
            };
            Ok(OptionalTeletextOutput {
                bytes: replay_direct_prefix(source, sink, prefix).await?,
                rejection: Some(error),
            })
        }
    }
}

pub(super) async fn replay_direct_prefix(
    source: &mut (impl AsyncRead + Unpin),
    sink: &mut (impl AsyncWrite + Unpin),
    prefix: Vec<u8>,
) -> Result<u64, TeletextPumpError> {
    let prefix_bytes = u64::try_from(prefix.len()).map_err(|_| TeletextPumpError::Configuration)?;
    sink.write_all(&prefix)
        .await
        .map_err(|error| TeletextPumpError::Write(error.kind()))?;
    drop(prefix);
    let remaining = copy_plain(source, sink).await?;
    sink.flush()
        .await
        .map_err(|error| TeletextPumpError::Write(error.kind()))?;
    prefix_bytes
        .checked_add(remaining)
        .ok_or(TeletextPumpError::Configuration)
}

pub(super) async fn copy_plain<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    source: &mut R,
    sink: &mut W,
) -> Result<u64, TeletextPumpError> {
    let mut bytes = read_scratch()?;
    let mut count = 0_u64;
    loop {
        let length = source
            .read(&mut bytes)
            .await
            .map_err(|error| TeletextPumpError::Read(error.kind()))?;
        if length == 0 {
            return Ok(count);
        }
        sink.write_all(&bytes[..length])
            .await
            .map_err(|error| TeletextPumpError::Write(error.kind()))?;
        count = count
            .checked_add(u64::try_from(length).map_err(|_| TeletextPumpError::Configuration)?)
            .ok_or(TeletextPumpError::Configuration)?;
    }
}

// Before commitment the journal bounds all consumed/processable packet slots.
// Afterwards the journal is released and the reader drops its scratch before
// more reads; the unchanged strict reserve model supplies its ordinary bound.
pub(super) fn retained_startup_budget(
    policy: ReservePolicy,
    binding: &TeletextOutputBinding,
    journal: usize,
) -> Result<(), RetainedTeletextError> {
    let extra = journal
        .checked_add(READ_BYTES)
        .ok_or(RetainedTeletextError::Configuration)?;
    processing_cap(policy, binding.config(), extra)?;
    // Fallback releases the gate before creating the ordinary reserve, but keeps
    // the original journal as its exact first input; charge their overlap too.
    if policy
        .capacity_bytes()
        .checked_add(journal)
        .and_then(|bytes| bytes.checked_add(2 * READ_BYTES))
        .is_none_or(|bytes| bytes > MAX_BYTES)
    {
        return Err(RetainedTeletextError::Configuration);
    }
    Ok(())
}

pub(super) struct OptionalTeletextStartup<'a> {
    pub binding: &'a TeletextOutputBinding,
    pub maximum_bytes: usize,
}

pub(super) async fn forward_optional_retained_teletext_output(
    source: &mut (impl AsyncRead + Unpin),
    sink: &mut (impl AsyncWrite + Unpin),
    policy: ReservePolicy,
    terminal: &AtomicU8,
    startup: Instant,
    cancel: &Notify,
    options: OptionalTeletextStartup<'_>,
) -> Result<OptionalRetainedTeletextOutput, RetainedTeletextError> {
    let OptionalTeletextStartup {
        binding,
        maximum_bytes: startup_bytes,
    } = options;
    retained_startup_budget(policy, binding, startup_bytes)?;
    let state =
        Startup::new(startup_bytes, binding).map_err(|_| RetainedTeletextError::Configuration)?;
    let result = {
        let mut reader =
            TeeReader::new(source, &state).map_err(|_| RetainedTeletextError::Configuration)?;
        let mut writer = CommitWriter {
            sink,
            state: Arc::clone(&state),
        };
        forward_retained_ts_with_startup(
            &mut reader,
            &mut writer,
            policy,
            terminal,
            startup,
            cancel,
            RetainedTeletextInput {
                binding,
                prefix: None,
                extra_startup_bytes: startup_bytes + READ_BYTES,
            },
        )
        .await
    };
    finish_retained(
        source,
        sink,
        policy,
        terminal,
        startup,
        cancel,
        (&state, result),
    )
    .await
}

async fn finish_retained(
    source: &mut (impl AsyncRead + Unpin),
    sink: &mut (impl AsyncWrite + Unpin),
    policy: ReservePolicy,
    terminal: &AtomicU8,
    startup: Instant,
    cancel: &Notify,
    outcome: (&Arc<Mutex<Startup>>, Result<(), RetainedTeletextError>),
) -> Result<OptionalRetainedTeletextOutput, RetainedTeletextError> {
    let (state, result) = outcome;
    match result {
        Ok(()) => Ok(OptionalRetainedTeletextOutput {
            bytes: state.lock().unwrap().received,
            rejection: None,
        }),
        Err(error) => {
            let prefix = {
                let mut state = state.lock().unwrap();
                let allowed = matches!(
                    error,
                    RetainedTeletextError::Gate(_) | RetainedTeletextError::Configuration
                ) || (state.capacity_reached
                    && matches!(error, RetainedTeletextError::Read(_)));
                if state.committed || !allowed {
                    return Err(error);
                }
                state.take()
            };
            Ok(OptionalRetainedTeletextOutput {
                bytes: replay_retained_prefix(
                    source, sink, policy, terminal, startup, cancel, prefix,
                )
                .await?,
                rejection: Some(error),
            })
        }
    }
}

pub(super) async fn replay_retained_prefix(
    source: &mut (impl AsyncRead + Unpin),
    sink: &mut (impl AsyncWrite + Unpin),
    policy: ReservePolicy,
    terminal: &AtomicU8,
    startup: Instant,
    cancel: &Notify,
    prefix: Vec<u8>,
) -> Result<u64, RetainedTeletextError> {
    if policy
        .capacity_bytes()
        .checked_add(prefix.capacity())
        .and_then(|bytes| bytes.checked_add(2 * READ_BYTES))
        .is_none_or(|bytes| bytes > MAX_BYTES)
    {
        return Err(RetainedTeletextError::Configuration);
    }
    let mut continuation = std::io::Cursor::new(prefix).chain(source);
    let mut counted = CountingReader {
        source: &mut continuation,
        count: 0,
    };
    forward_retained_ts(&mut counted, sink, policy, terminal, startup, cancel).await?;
    Ok(counted.count)
}

pub(super) async fn forward_optional_seeded_teletext_output<
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
>(
    source: &mut R,
    sink: &mut W,
    binding: &TeletextOutputBinding,
    prefix: TeletextStartupPrefix,
    maximum_bytes: usize,
) -> Result<OptionalTeletextOutput, TeletextPumpError> {
    let state = match Startup::seeded(maximum_bytes, binding, &prefix) {
        Ok(state) => state,
        Err(error) => {
            return Ok(OptionalTeletextOutput {
                bytes: replay_direct_prefix(source, sink, prefix.into_bytes()).await?,
                rejection: Some(error),
            })
        }
    };
    let mut reader = match TeeReader::new(source, &state) {
        Ok(reader) => reader,
        Err(error) => {
            drop(prefix);
            return finish_direct(source, sink, &state, Err(error)).await;
        }
    };
    let result = {
        let mut writer = CommitWriter {
            sink,
            state: Arc::clone(&state),
        };
        forward_teletext_output_seeded(&mut reader, &mut writer, binding, prefix).await
    };
    drop(reader);
    finish_direct(source, sink, &state, result).await
}

pub(super) struct SeededOptionalTeletextStartup<'a> {
    pub binding: &'a TeletextOutputBinding,
    pub maximum_bytes: usize,
    pub prefix: TeletextStartupPrefix,
}

pub(super) async fn forward_optional_seeded_retained_output(
    source: &mut (impl AsyncRead + Unpin),
    sink: &mut (impl AsyncWrite + Unpin),
    policy: ReservePolicy,
    terminal: &AtomicU8,
    startup: Instant,
    cancel: &Notify,
    options: SeededOptionalTeletextStartup<'_>,
) -> Result<OptionalRetainedTeletextOutput, RetainedTeletextError> {
    let SeededOptionalTeletextStartup {
        binding,
        maximum_bytes,
        prefix,
    } = options;
    let extra_base = allocation_model(binding.config()).map(|(base, _)| base);
    let base = extra_base.as_ref().copied().unwrap_or(MAX_BYTES);
    if retained_startup_budget(policy, binding, maximum_bytes).is_err()
        || extra_base.is_err()
        || base
            .checked_add(maximum_bytes)
            .and_then(|bytes| bytes.checked_add(prefix.retained_bytes()))
            .and_then(|bytes| bytes.checked_add(READ_BYTES))
            .is_none_or(|bytes| bytes > MAX_BYTES)
    {
        return Ok(OptionalRetainedTeletextOutput {
            bytes: replay_retained_prefix(
                source,
                sink,
                policy,
                terminal,
                startup,
                cancel,
                prefix.into_bytes(),
            )
            .await?,
            rejection: Some(RetainedTeletextError::Configuration),
        });
    }
    let Ok(state) = Startup::seeded(maximum_bytes, binding, &prefix) else {
        return Ok(OptionalRetainedTeletextOutput {
            bytes: replay_retained_prefix(
                source,
                sink,
                policy,
                terminal,
                startup,
                cancel,
                prefix.into_bytes(),
            )
            .await?,
            rejection: Some(RetainedTeletextError::Configuration),
        });
    };
    let Ok(mut reader) = TeeReader::new(source, &state) else {
        drop(prefix);
        return finish_retained(
            source,
            sink,
            policy,
            terminal,
            startup,
            cancel,
            (&state, Err(RetainedTeletextError::Configuration)),
        )
        .await;
    };
    let result = {
        let mut writer = CommitWriter {
            sink,
            state: Arc::clone(&state),
        };
        forward_retained_ts_with_startup(
            &mut reader,
            &mut writer,
            policy,
            terminal,
            startup,
            cancel,
            RetainedTeletextInput {
                binding,
                prefix: Some(prefix),
                extra_startup_bytes: maximum_bytes + READ_BYTES,
            },
        )
        .await
    };
    drop(reader);
    finish_retained(
        source,
        sink,
        policy,
        terminal,
        startup,
        cancel,
        (&state, result),
    )
    .await
}

struct CountingReader<'a, R> {
    source: &'a mut R,
    count: u64,
}
impl<R: AsyncRead + Unpin> AsyncRead for CountingReader<'_, R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        output: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let before = output.filled().len();
        let result = Pin::new(&mut *this.source).poll_read(cx, output);
        if matches!(result, Poll::Ready(Ok(()))) {
            let Some(count) = this
                .count
                .checked_add(u64::try_from(output.filled().len() - before).unwrap())
            else {
                return Poll::Ready(Err(io::Error::other("teletext byte count overflow")));
            };
            this.count = count;
        }
        result
    }
}

#[cfg(test)]
#[path = "teletext_optional_output_tests.rs"]
mod tests;
