//! Bounded occurrence ownership for shared AV/caption retained provider bytes.

use futures_util::FutureExt as _;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{sync::Mutex as Gate, time::Instant};
use uuid::Uuid;

use super::session_transfers::TransferBinding;
use crate::{
    ffprobe::{
        open_retained_source_reader, LocalProbeCleanupScope, RetainedProviderSource,
        RetainedSourceReader,
    },
    provider_source_storage::ProviderSourceCleanup,
    remote_stream::RemoteStreamError,
};

const MAX_ENTRIES: usize = 8;
const MAX_CLEANUPS: usize = 64;
const SHUTDOWN_BUDGET: Duration = Duration::from_secs(8);
type Result<T> = std::result::Result<T, RemoteStreamError>;

#[derive(Default)]
pub(crate) struct SessionSourceFiles {
    scope: LocalProbeCleanupScope,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    closed_at: Option<Instant>,
    uncertain: bool,
    registrations_closed: bool,
    entries: HashMap<Uuid, Entry>,
    cleanups: Vec<ProviderSourceCleanup>,
}

struct Entry {
    binding: TransferBinding,
    expires: Instant,
    gate: Arc<Gate<()>>,
    source: Option<Arc<RetainedProviderSource>>,
}

impl SessionSourceFiles {
    pub(crate) fn gate(
        &self,
        binding: TransferBinding,
        expires: Instant,
    ) -> Result<(Uuid, Arc<Gate<()>>)> {
        let mut retired = Vec::new();
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        prune(&mut state, &mut retired);
        available(&state)?;
        if expires <= Instant::now() || binding.finish <= binding.start {
            return Err(RemoteStreamError::PreparedTimeout);
        }
        if let Some((id, entry)) = state
            .entries
            .iter()
            .find(|(_, entry)| same_binding(&entry.binding, &binding))
        {
            return Ok((*id, entry.gate.clone()));
        }
        if state.entries.len() >= MAX_ENTRIES {
            return Err(RemoteStreamError::PreparedRequestTooLarge);
        }
        let id = Uuid::new_v4();
        let gate = Arc::new(Gate::new(()));
        state.entries.insert(
            id,
            Entry {
                binding,
                expires,
                gate: gate.clone(),
                source: None,
            },
        );
        Ok((id, gate))
    }

    pub(crate) fn ready(&self, id: Uuid) -> Option<Arc<RetainedProviderSource>> {
        let mut retired = Vec::new();
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        prune(&mut state, &mut retired);
        available(&state).ok()?;
        state.entries.get(&id)?.source.clone()
    }

    pub(crate) fn install(&self, id: Uuid, source: Arc<RetainedProviderSource>) -> Result<()> {
        let mut retired = Vec::new();
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        prune(&mut state, &mut retired);
        available(&state)?;
        let entry = state
            .entries
            .get_mut(&id)
            .ok_or(RemoteStreamError::PreparedCancelled)?;
        if entry
            .source
            .as_ref()
            .is_some_and(|installed| !Arc::ptr_eq(installed, &source))
        {
            return Err(RemoteStreamError::PreparedCancelled);
        }
        entry.source = Some(source);
        Ok(())
    }

    pub(crate) fn describe(&self, id: Uuid) -> Result<(u64, TransferBinding)> {
        let mut retired = Vec::new();
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        prune(&mut state, &mut retired);
        available(&state)?;
        let entry = state
            .entries
            .get(&id)
            .ok_or(RemoteStreamError::PreparedCancelled)?;
        let source = entry
            .source
            .as_ref()
            .ok_or(RemoteStreamError::PreparedCancelled)?;
        Ok((source.length, copy_binding(&entry.binding)))
    }

    pub(crate) async fn open(
        &self,
        id: Uuid,
        offset: u64,
        length: u64,
    ) -> Result<RetainedSourceReader> {
        let source = self.ready(id).ok_or(RemoteStreamError::PreparedCancelled)?;
        self.scope
            .scope(open_retained_source_reader(source, offset, length))
            .await
            .map_err(|error| match error {
                crate::ffprobe::FfprobeError::CleanupUnconfirmed { .. } => {
                    RemoteStreamError::PreparedTerminationUnconfirmed
                }
                _ => RemoteStreamError::PreparedProducerFailed,
            })
    }

    pub(crate) fn scope(&self) -> LocalProbeCleanupScope {
        self.scope.clone()
    }

    pub(crate) fn track_cleanup(&self, cleanup: ProviderSourceCleanup) -> Result<()> {
        let mut retired = Vec::new();
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        prune(&mut state, &mut retired);
        if state.cleanups.len() >= MAX_CLEANUPS {
            state.uncertain = true;
            return Err(RemoteStreamError::PreparedTerminationUnconfirmed);
        }
        state.cleanups.push(cleanup);
        if state.uncertain || state.registrations_closed {
            state.uncertain = true;
            return Err(RemoteStreamError::PreparedTerminationUnconfirmed);
        }
        if state.closed_at.is_some() {
            // An admitted preparation still holds its gate. Shutdown will join
            // that owner before snapshotting all retained cleanup witnesses.
            return Err(RemoteStreamError::PreparedCancelled);
        }
        Ok(())
    }

    pub(crate) async fn shutdown(&self) -> Result<()> {
        let (deadline, gates, entries) = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let closed_at = *state.closed_at.get_or_insert_with(Instant::now);
            self.scope.close();
            let entries = std::mem::take(&mut state.entries);
            let gates = entries
                .values()
                .map(|entry| entry.gate.clone())
                .collect::<Vec<_>>();
            (closed_at + SHUTDOWN_BUDGET, gates, entries)
        };
        drop(entries);
        let result = tokio::time::timeout_at(deadline, async {
            for gate in gates {
                drop(gate.lock().await);
            }
            let cleanups = {
                let mut state = self
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                state.registrations_closed = true;
                state.cleanups.clone()
            };
            self.scope
                .wait_cleanup()
                .await
                .map_err(|_| RemoteStreamError::PreparedTerminationUnconfirmed)?;
            for cleanup in cleanups {
                cleanup
                    .wait()
                    .await
                    .map_err(|_| RemoteStreamError::PreparedTerminationUnconfirmed)?;
            }
            Ok(())
        })
        .await
        .map_err(|_| RemoteStreamError::PreparedTerminationUnconfirmed)
        .and_then(|result| result);
        let mut retired = Vec::new();
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        prune(&mut state, &mut retired);
        state.uncertain |= result.is_err();
        if state.uncertain {
            Err(RemoteStreamError::PreparedTerminationUnconfirmed)
        } else {
            result
        }
    }
}

fn available(state: &State) -> Result<()> {
    if state.uncertain {
        Err(RemoteStreamError::PreparedTerminationUnconfirmed)
    } else if state.closed_at.is_some() {
        Err(RemoteStreamError::PreparedCancelled)
    } else {
        Ok(())
    }
}

// Callers declare `retired` before their mutex guard, so the last source/file
// drops happen outside the arena lock, including early-return paths.
fn prune(state: &mut State, retired: &mut Vec<Entry>) {
    let now = Instant::now();
    let ids = state
        .entries
        .iter()
        .filter_map(|(id, entry)| (entry.expires <= now).then_some(*id))
        .collect::<Vec<_>>();
    retired.extend(ids.into_iter().filter_map(|id| state.entries.remove(&id)));
    let mut failed = false;
    state
        .cleanups
        .retain(|cleanup| match cleanup.wait().now_or_never() {
            None => true,
            Some(Ok(())) => false,
            Some(Err(_)) => {
                failed = true;
                false
            }
        });
    state.uncertain |= failed;
}

fn same_binding(a: &TransferBinding, b: &TransferBinding) -> bool {
    a.media_item_id == b.media_item_id
        && a.playout_item_id == b.playout_item_id
        && a.media_version_id == b.media_version_id
        && a.media_file_id == b.media_file_id
        && a.publication == b.publication
        && a.start == b.start
        && a.finish == b.finish
}

fn copy_binding(binding: &TransferBinding) -> TransferBinding {
    TransferBinding {
        media_item_id: binding.media_item_id,
        playout_item_id: binding.playout_item_id,
        media_version_id: binding.media_version_id,
        media_file_id: binding.media_file_id,
        publication: binding.publication.clone(),
        start: binding.start,
        finish: binding.finish,
    }
}

#[cfg(test)]
#[path = "session_source_files/tests.rs"]
mod tests;
