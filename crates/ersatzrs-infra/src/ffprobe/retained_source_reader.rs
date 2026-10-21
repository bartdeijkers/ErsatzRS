//! Actual range-reader ownership for an already retained private provider file.

use std::{
    fs::File,
    future::Future as _,
    io::{self, Read as _, Seek as _},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use futures_util::{
    future::{BoxFuture, Shared},
    FutureExt as _,
};
use tokio::{
    io::{AsyncRead, AsyncWriteExt as _, DuplexStream, ReadBuf},
    sync::{oneshot, watch},
};

use super::{
    local_cleanup_scope::{self, AdmissionError, OwnerGuard},
    FfprobeError, Result, RetainedProviderSource,
};

const LABEL: &str = "retained provider source reader";
const BUFFER_BYTES: usize = 64 * 1024;
type Completion = Shared<BoxFuture<'static, std::result::Result<(), Failure>>>;

#[derive(Clone, Copy)]
enum Failure {
    Cancelled,
    Invalid,
    Unconfirmed,
}

impl Failure {
    fn error(self) -> FfprobeError {
        match self {
            Self::Cancelled => FfprobeError::Cancelled { path: LABEL.into() },
            Self::Invalid => FfprobeError::InvalidOutput { path: LABEL.into() },
            Self::Unconfirmed => FfprobeError::CleanupUnconfirmed { path: LABEL.into() },
        }
    }
}

/// A range of exact retained bytes, with actual reader status attached to EOF.
/// Dropping the body cancels its worker; the session scope awaits real cleanup.
pub(crate) struct RetainedSourceReader {
    reader: DuplexStream,
    cancel: watch::Sender<bool>,
    completion: Completion,
}

/// Open and validate before returning the body; every actual file job retains
/// the source and quota lease even if this awaiting request disappears.
pub(crate) async fn open_retained_source_reader(
    source: Arc<RetainedProviderSource>,
    offset: u64,
    length: u64,
) -> Result<RetainedSourceReader> {
    if source.length > source.storage.source_limit()
        || offset
            .checked_add(length)
            .is_none_or(|end| end > source.length)
    {
        return Err(Failure::Invalid.error());
    }
    let owner = local_cleanup_scope::register_current()
        .map_err(|error| match error {
            AdmissionError::Unconfirmed => Failure::Unconfirmed.error(),
            AdmissionError::Closed | AdmissionError::Capacity => Failure::Cancelled.error(),
        })?
        .ok_or_else(|| Failure::Cancelled.error())?;
    let (ready, opened) = oneshot::channel();
    let (cancel, cancellation) = watch::channel(false);
    let worker = tokio::spawn(async move {
        let result = serve(source, offset, length, ready, cancellation, &owner).await;
        owner.complete(!matches!(result, Err(Failure::Unconfirmed)));
        result
    });
    let completion = async move { worker.await.map_err(|_| Failure::Unconfirmed)? }
        .boxed()
        .shared();
    match opened.await {
        Ok(Ok(reader)) => Ok(RetainedSourceReader {
            reader,
            cancel,
            completion,
        }),
        result => {
            completion.await.map_err(Failure::error)?;
            Err(result
                .err()
                .map_or(Failure::Invalid, |_| Failure::Unconfirmed)
                .error())
        }
    }
}

// Field order closes the actual descriptor before the last source/quota owner.
struct Input {
    file: File,
    _source: Arc<RetainedProviderSource>,
    remaining: u64,
}

async fn open(
    source: Arc<RetainedProviderSource>,
    offset: u64,
    length: u64,
) -> std::result::Result<Input, Failure> {
    tokio::task::spawn_blocking(move || {
        let mut file = source.file.reopen().map_err(|_| Failure::Invalid)?;
        let metadata = file.metadata().map_err(|_| Failure::Invalid)?;
        if !metadata.is_file() || metadata.len() != source.length {
            return Err(Failure::Invalid);
        }
        file.seek(io::SeekFrom::Start(offset))
            .map_err(|_| Failure::Invalid)?;
        Ok(Input {
            file,
            _source: source,
            remaining: length,
        })
    })
    .await
    .map_err(|_| Failure::Unconfirmed)?
}

async fn chunk(mut input: Input) -> std::result::Result<(Input, Vec<u8>), Failure> {
    tokio::task::spawn_blocking(move || {
        let length = usize::try_from(input.remaining.min(BUFFER_BYTES as u64))
            .map_err(|_| Failure::Invalid)?;
        let mut bytes = vec![0; length];
        input
            .file
            .read_exact(&mut bytes)
            .map_err(|_| Failure::Invalid)?;
        input.remaining -= u64::try_from(length).map_err(|_| Failure::Invalid)?;
        Ok((input, bytes))
    })
    .await
    .map_err(|_| Failure::Unconfirmed)?
}

async fn close(input: Input) -> std::result::Result<(), Failure> {
    tokio::task::spawn_blocking(move || drop(input))
        .await
        .map_err(|_| Failure::Unconfirmed)
}

async fn cancelled(cancellation: &mut watch::Receiver<bool>) {
    if *cancellation.borrow() {
        return;
    }
    let _ = cancellation.changed().await;
}

async fn serve(
    source: Arc<RetainedProviderSource>,
    offset: u64,
    length: u64,
    ready: oneshot::Sender<std::result::Result<DuplexStream, Failure>>,
    mut cancellation: watch::Receiver<bool>,
    owner: &OwnerGuard,
) -> std::result::Result<(), Failure> {
    if ready.is_closed() || owner.cancellation_requested() {
        return Err(Failure::Cancelled);
    }
    // Never select away an actual blocking open/read/close job.
    let mut input = open(source, offset, length).await?;
    if ready.is_closed() || owner.cancellation_requested() || *cancellation.borrow() {
        close(input).await?;
        return Err(Failure::Cancelled);
    }
    let (reader, mut writer) = tokio::io::duplex(BUFFER_BYTES);
    if ready.send(Ok(reader)).is_err() {
        close(input).await?;
        return Err(Failure::Cancelled);
    }
    let mut result = Ok(());
    while input.remaining != 0 {
        if owner.cancellation_requested() || *cancellation.borrow() {
            result = Err(Failure::Cancelled);
            break;
        }
        let (retained, bytes) = chunk(input).await?;
        input = retained;
        result = tokio::select! {
            biased;
            () = owner.cancelled() => Err(Failure::Cancelled),
            () = cancelled(&mut cancellation) => Err(Failure::Cancelled),
            write = writer.write_all(&bytes) => write.map_err(|_| Failure::Cancelled),
        };
        if result.is_err() {
            break;
        }
    }
    close(input).await?;
    result
}

impl AsyncRead for RetainedSourceReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let before = buf.filled().len();
        match Pin::new(&mut self.reader).poll_read(cx, buf) {
            Poll::Ready(Ok(())) if before == buf.filled().len() && buf.remaining() != 0 => {
                Pin::new(&mut self.completion).poll(cx).map(|result| {
                    result.map_err(|_| io::Error::other("retained source reader failed"))
                })
            }
            result => result,
        }
    }
}

impl Drop for RetainedSourceReader {
    fn drop(&mut self) {
        self.cancel.send_replace(true);
    }
}

#[cfg(test)]
#[path = "retained_source_reader/tests.rs"]
pub(crate) mod tests;
