//! Ephemeral producer ownership for one admitted Next process instance.

use crate::{
    ffprobe::FfprobeMediaVersion,
    remote_stream::{PreparedRemoteStreamInput, RemoteStreamError},
};
use chrono::{DateTime, Utc};
use futures_util::{
    future::{BoxFuture, Shared},
    FutureExt,
};
use std::{
    collections::HashMap,
    future::Future,
    io,
    pin::Pin,
    sync::{Arc, Mutex, Weak},
    task::{Context, Poll},
};
use tokio::{
    io::{AsyncRead, DuplexStream, ReadBuf},
    sync::{oneshot, watch},
    time::Instant,
};
use uuid::Uuid;

const MAX_TRANSFERS: usize = 8;
const BUFFER_BYTES: usize = 64 * 1024;
type Completion = Shared<BoxFuture<'static, Result<(), RemoteStreamError>>>;

/// Immutable callback selection bound to a single-use byte transfer.
/// Deliberately not Debug: publication identity can contain private source data.
pub struct TransferBinding {
    pub media_item_id: i64,
    pub playout_item_id: i64,
    pub media_version_id: i64,
    pub media_file_id: i64,
    pub publication: String,
    pub start: DateTime<Utc>,
    pub finish: DateTime<Utc>,
}

pub struct PreparedTransferMedia {
    pub input: PreparedRemoteStreamInput,
    pub probe: FfprobeMediaVersion,
}

pub struct ReadyTransfer {
    pub id: Uuid,
    pub probe: FfprobeMediaVersion,
}

/// Bounded pending and active transfers belonging exclusively to one session.
#[derive(Default)]
pub struct SessionTransfers {
    state: Arc<Mutex<State>>,
}

#[derive(Default)]
struct State {
    closed: bool,
    entries: HashMap<Uuid, Entry>,
}

struct Entry {
    _binding: TransferBinding,
    deadline: Instant,
    cancel: watch::Sender<bool>,
    claimed: Option<oneshot::Sender<()>>,
    reader: Option<TransferReader>,
    completion: Completion,
}

impl SessionTransfers {
    /// Signal cancellation from a dropped callback; shutdown remains the waiter.
    pub(crate) fn request_cancel(&self, id: Uuid) {
        if let Some(entry) = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entries
            .get(&id)
        {
            entry.cancel.send_replace(true);
        }
    }

    /// Register ownership before invoking preparation. The closure must observe
    /// its cancellation receiver and await cleanup before returning an error.
    /// The request may drop this future; its already-registered task remains
    /// owned and is cancelled, including while its startup prefix is pending.
    ///
    /// # Errors
    /// Refuses closed/full sessions, invalid intervals, expired preparation or
    /// producer failure. Unconfirmed cleanup remains a distinct error.
    pub async fn prepare<F, Fut>(
        &self,
        binding: TransferBinding,
        preparation_deadline: Instant,
        claim_deadline: Instant,
        prepare: F,
    ) -> Result<ReadyTransfer, RemoteStreamError>
    where
        F: FnOnce(oneshot::Receiver<()>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<PreparedTransferMedia, RemoteStreamError>> + Send + 'static,
    {
        if binding.finish <= binding.start
            || preparation_deadline <= Instant::now()
            || claim_deadline <= Instant::now()
        {
            return Err(RemoteStreamError::PreparedTimeout);
        }
        let preparation_deadline = preparation_deadline.min(claim_deadline);
        let id = Uuid::new_v4();
        let (cancel, cancellation) = watch::channel(false);
        let (claim, claimed) = oneshot::channel();
        let (ready, receive_ready) = oneshot::channel();
        let (start, started) = oneshot::channel();
        let weak = Arc::downgrade(&self.state);
        {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            // A completed claimed/expired entry no longer owns playable bytes.
            state
                .entries
                .retain(|_, entry| entry.reader.is_some() || cleanup_unresolved(&entry.completion));
            if state.closed || state.entries.len() >= MAX_TRANSFERS {
                return Err(RemoteStreamError::PreparedCancelled);
            }
            let task = tokio::spawn(async move {
                if started.await.is_err() {
                    return Err(RemoteStreamError::PreparedCancelled);
                }
                run_preparation(
                    weak,
                    id,
                    preparation_deadline,
                    claim_deadline,
                    cancellation,
                    claimed,
                    ready,
                    prepare,
                )
                .await
            });
            let completion = async move {
                task.await
                    .map_err(|_| RemoteStreamError::PreparedTerminationUnconfirmed)?
            }
            .boxed()
            .shared();
            state.entries.insert(
                id,
                Entry {
                    _binding: binding,
                    deadline: claim_deadline,
                    cancel: cancel.clone(),
                    claimed: Some(claim),
                    reader: None,
                    completion,
                },
            );
        }
        let mut guard = CancelOnDrop(Some(cancel));
        let _ = start.send(());
        let result = receive_ready
            .await
            .map_err(|_| RemoteStreamError::PreparedProducerFailed)?;
        if result.is_ok() {
            guard.0.take();
        }
        result
    }

    /// Consume one ready reader. Opening the same token twice is rejected.
    /// # Errors
    /// Refuses closed sessions, unknown, expired, not-ready or consumed tokens.
    pub fn claim(&self, id: Uuid) -> Result<TransferReader, RemoteStreamError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.closed {
            return Err(RemoteStreamError::PreparedCancelled);
        }
        let entry = state
            .entries
            .get_mut(&id)
            .ok_or(RemoteStreamError::PreparedCancelled)?;
        if Instant::now() >= entry.deadline || *entry.cancel.borrow() {
            entry.cancel.send_replace(true);
            return Err(RemoteStreamError::PreparedTimeout);
        }
        let reader = entry
            .reader
            .take()
            .ok_or(RemoteStreamError::PreparedCancelled)?;
        let claim = entry
            .claimed
            .take()
            .ok_or(RemoteStreamError::PreparedCancelled)?;
        claim
            .send(())
            .map_err(|()| RemoteStreamError::PreparedCancelled)?;
        Ok(reader)
    }

    /// Cancel a token rejected by post-preparation publication revalidation.
    /// # Errors
    /// Reports producer failure or unconfirmed cleanup after awaiting the owner.
    pub async fn cancel(&self, id: Uuid) -> Result<(), RemoteStreamError> {
        let completion = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let Some(entry) = state.entries.get_mut(&id) else {
                return Ok(());
            };
            entry.cancel.send_replace(true);
            entry.reader.take();
            entry.completion.clone()
        };
        let result = completion.await;
        if !matches!(
            result,
            Err(RemoteStreamError::PreparedTerminationUnconfirmed)
        ) {
            self.state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .entries
                .remove(&id);
        }
        result
    }

    /// Close admission, cancel every producer first, then await all cleanup.
    /// Concurrent token cancellation awaits the same completion owner.
    /// # Errors
    /// Reports failure after all tasks have been awaited. Unconfirmed descendant
    /// termination takes priority over ordinary producer failures.
    pub async fn shutdown(&self) -> Result<(), RemoteStreamError> {
        let entries = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.closed = true;
            for entry in state.entries.values() {
                entry.cancel.send_replace(true);
            }
            state
                .entries
                .values_mut()
                .map(|entry| {
                    entry.reader.take();
                    entry.completion.clone()
                })
                .collect::<Vec<_>>()
        };
        let mut outcome = Ok(());
        for completion in entries {
            if let Err(error) = completion.await {
                if outcome.is_ok()
                    || matches!(error, RemoteStreamError::PreparedTerminationUnconfirmed)
                {
                    outcome = Err(error);
                }
            }
        }
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entries
            .retain(|_, entry| cleanup_unresolved(&entry.completion));
        outcome
    }
}

fn cleanup_unresolved(completion: &Completion) -> bool {
    matches!(
        completion.clone().now_or_never(),
        None | Some(Err(RemoteStreamError::PreparedTerminationUnconfirmed))
    )
}

impl Drop for SessionTransfers {
    fn drop(&mut self) {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for entry in state.entries.values() {
            entry.cancel.send_replace(true);
        }
    }
}

struct CancelOnDrop(Option<watch::Sender<bool>>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if let Some(cancel) = &self.0 {
            cancel.send_replace(true);
        }
    }
}

async fn cancelled(cancellation: &mut watch::Receiver<bool>) {
    if *cancellation.borrow() {
        return;
    }
    let _ = cancellation.changed().await;
}

#[allow(
    clippy::too_many_arguments,
    reason = "Preparation retains ownership, cancellation, and both deadlines explicitly."
)]
async fn run_preparation<F, Fut>(
    state: Weak<Mutex<State>>,
    id: Uuid,
    preparation_deadline: Instant,
    claim_deadline: Instant,
    mut cancellation: watch::Receiver<bool>,
    claimed: oneshot::Receiver<()>,
    ready: oneshot::Sender<Result<ReadyTransfer, RemoteStreamError>>,
    prepare: F,
) -> Result<(), RemoteStreamError>
where
    F: FnOnce(oneshot::Receiver<()>) -> Fut,
    Fut: Future<Output = Result<PreparedTransferMedia, RemoteStreamError>>,
{
    let (cancel_prepare, preparation_cancelled) = oneshot::channel();
    let preparation = prepare(preparation_cancelled);
    tokio::pin!(preparation);
    let prepared = tokio::select! {
        biased;
        () = cancelled(&mut cancellation) => None,
        () = tokio::time::sleep_until(preparation_deadline) => None,
        result = &mut preparation => Some(result),
    };
    let Some(prepared) = prepared else {
        let _ = cancel_prepare.send(());
        let cleanup = match preparation.await {
            Ok(prepared) => prepared.input.cancel().await,
            Err(RemoteStreamError::PreparedTerminationUnconfirmed) => {
                Err(RemoteStreamError::PreparedTerminationUnconfirmed)
            }
            Err(_) => Ok(()),
        };
        let _ = ready.send(Err(cleanup
            .clone()
            .err()
            .unwrap_or(RemoteStreamError::PreparedCancelled)));
        return cleanup;
    };
    let prepared = match prepared {
        Ok(prepared) => prepared,
        Err(error) => {
            let _ = ready.send(Err(error.clone()));
            return Err(error);
        }
    };
    let (writer, reader) = tokio::io::duplex(BUFFER_BYTES);
    let installed = state.upgrade().is_some_and(|state| {
        let mut state = state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.closed {
            return false;
        }
        let Some(entry) = state.entries.get_mut(&id) else {
            return false;
        };
        if *entry.cancel.borrow() {
            return false;
        }
        entry.reader = Some(TransferReader {
            reader,
            cancel: entry.cancel.clone(),
            completion: entry.completion.clone(),
        });
        true
    });
    if !installed {
        let cleanup = prepared.input.cancel().await;
        let _ = ready.send(Err(cleanup
            .clone()
            .err()
            .unwrap_or(RemoteStreamError::PreparedCancelled)));
        return cleanup;
    }
    if ready
        .send(Ok(ReadyTransfer {
            id,
            probe: prepared.probe,
        }))
        .is_err()
    {
        clear_reader(&state, id);
        return prepared.input.cancel().await;
    }
    let opened = tokio::select! {
        biased;
        () = cancelled(&mut cancellation) => false,
        // Claim admission checked the deadline under the registry lock. Honor
        // an accepted claim even if this task is scheduled after that deadline.
        result = claimed => result.is_ok(),
        () = tokio::time::sleep_until(claim_deadline) => false,
    };
    if !opened {
        clear_reader(&state, id);
        return prepared.input.cancel().await;
    }
    let mut transfer = prepared.input.transfer_to(writer).await?;
    tokio::select! {
        biased;
        () = cancelled(&mut cancellation) => transfer.shutdown().await,
        result = transfer.wait() => result,
    }
}

fn clear_reader(state: &Weak<Mutex<State>>, id: Uuid) {
    if let Some(state) = state.upgrade() {
        if let Some(entry) = state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entries
            .get_mut(&id)
        {
            entry.reader.take();
        }
    }
}

/// Original media bytes with terminal producer status attached to EOF.
/// Dropping the HTTP body cancels its producer; session shutdown awaits cleanup.
pub struct TransferReader {
    reader: DuplexStream,
    cancel: watch::Sender<bool>,
    completion: Completion,
}
impl AsyncRead for TransferReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let before = buf.filled().len();
        match Pin::new(&mut self.reader).poll_read(cx, buf) {
            Poll::Ready(Ok(())) if buf.filled().len() == before && buf.remaining() != 0 => {
                Pin::new(&mut self.completion).poll(cx).map(|result| {
                    result.map_err(|_| io::Error::other("Next media producer failed"))
                })
            }
            result => result,
        }
    }
}
impl Drop for TransferReader {
    fn drop(&mut self) {
        self.cancel.send_replace(true);
    }
}

#[cfg(test)]
#[path = "session_transfers_tests.rs"]
mod tests;
