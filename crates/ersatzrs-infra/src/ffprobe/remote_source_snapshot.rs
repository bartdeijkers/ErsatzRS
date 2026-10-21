//! Complete finite HTTP acquisition; no native source or caption authority.

use std::{io::Write as _, sync::Arc};

use crate::provider_source_storage::ProviderSourceLease;
use ersatzrs_core::locking::TeletextWorkHold;
use reqwest::{header, Response, StatusCode};
use sha2::{Digest as _, Sha256};
use tempfile::NamedTempFile;
use tokio::{sync::oneshot, time::Instant};

use super::{
    local_cleanup_scope::{self, AdmissionError, OwnerGuard},
    FfprobeError, Result,
};

const LABEL: &str = "complete finite HTTP source";
type Snapshot = Arc<NamedTempFile>;
type Reply = oneshot::Sender<Result<Captured>>;

struct Captured {
    file: Snapshot,
    storage: Option<Arc<ProviderSourceLease>>,
    length: u64,
    digest: [u8; 32],
}

/// Complete private response bytes; native/source/track readiness is separate.
/// Every actual reader or decoder must retain this owner, not only its path.
pub(crate) struct RetainedProviderSource {
    pub(crate) file: Snapshot,
    pub(crate) storage: Arc<ProviderSourceLease>,
    pub(crate) length: u64,
    pub(crate) digest: [u8; 32],
}

impl RetainedProviderSource {
    pub(crate) fn path(&self) -> &std::path::Path {
        self.file.path()
    }
}

pub(crate) async fn retain_provider_http_source(
    response: Response,
    deadline: Instant,
    maximum_bytes: u64,
    hold: TeletextWorkHold,
    storage: Arc<ProviderSourceLease>,
) -> Result<Arc<RetainedProviderSource>> {
    if maximum_bytes > storage.source_limit() {
        return Err(too_large());
    }
    let captured = retain(response, deadline, maximum_bytes, hold, Some(storage)).await?;
    let storage = captured.storage.ok_or_else(invalid)?;
    Ok(Arc::new(RetainedProviderSource {
        file: captured.file,
        storage,
        length: captured.length,
        digest: captured.digest,
    }))
}

/// Retain one complete, nonempty HTTP response in a private temporary file.
///
/// The caller must supply an active `LocalProbeCleanupScope`, an already
/// admitted work hold and an explicit response byte ceiling. This leaf does not
/// reserve total spool space, acquire a source revision or establish native
/// media readiness.
/// The caller owns total spool reservations and must retain the returned file
/// through subsequent native work and every session that can still read it.
///
/// Cancellation or the one absolute deadline may return before a blocking file
/// operation settles. Its actual owner retains the file and work hold; close
/// and drain the existing cleanup scope before releasing operation resources.
///
/// # Errors
/// Refuses unmanaged/closed scopes, partial or empty responses, exceeded limits,
/// incomplete bodies, cancellation, deadline expiry and unsettled file work.
/// Diagnostics contain no remote URL, credentials or temporary path.
pub async fn retain_complete_http_source(
    response: Response,
    deadline: Instant,
    maximum_bytes: u64,
    hold: TeletextWorkHold,
) -> Result<Snapshot> {
    retain(response, deadline, maximum_bytes, hold, None)
        .await
        .map(|captured| captured.file)
}

async fn retain(
    response: Response,
    deadline: Instant,
    maximum_bytes: u64,
    hold: TeletextWorkHold,
    storage: Option<Arc<ProviderSourceLease>>,
) -> Result<Captured> {
    let owner = local_cleanup_scope::register_current()
        .map_err(|error| match error {
            AdmissionError::Closed => cancelled(),
            AdmissionError::Unconfirmed => uncertain(),
            AdmissionError::Capacity => FfprobeError::Spawn {
                path: LABEL.into(),
                source: std::io::Error::other("source cleanup scope is busy"),
            },
        })?
        .ok_or_else(cancelled)?;
    let (mut reply, receive) = oneshot::channel();
    let worker = tokio::spawn(async move {
        let stop = Stop {
            owner: &owner,
            reply: &mut reply,
            deadline,
        };
        let result = acquire(response, maximum_bytes, &hold, stop, storage).await;
        let confirmed = !matches!(result, Err(FfprobeError::CleanupUnconfirmed { .. }));
        // Failed sends drop their private snapshot before reporting cleanup.
        let _ = reply.send(result);
        drop(hold);
        owner.complete(confirmed);
    });
    // Dropping this join future never aborts the actual file/body owner. Dropping
    // the result receiver requests cancellation through Reply::closed instead.
    tokio::time::timeout_at(deadline, async {
        worker.await.map_err(|_| uncertain())?;
        receive.await.map_err(|_| uncertain())?
    })
    .await
    .map_err(|_| uncertain())?
}

struct Stop<'a> {
    owner: &'a OwnerGuard,
    reply: &'a mut Reply,
    deadline: Instant,
}

impl Stop<'_> {
    fn check(&self) -> Result<()> {
        if self.reply.is_closed() || self.owner.cancellation_requested() {
            Err(cancelled())
        } else if Instant::now() >= self.deadline {
            Err(timed_out())
        } else {
            Ok(())
        }
    }

    async fn chunk(&mut self, response: &mut Response) -> Result<Option<bytes::Bytes>> {
        self.check()?;
        tokio::select! {
            biased;
            () = self.reply.closed() => Err(cancelled()),
            () = self.owner.cancelled() => Err(cancelled()),
            () = tokio::time::sleep_until(self.deadline) => Err(timed_out()),
            result = response.chunk() => result.map_err(|_| invalid()),
        }
    }
}

async fn acquire(
    mut response: Response,
    maximum_bytes: u64,
    hold: &TeletextWorkHold,
    mut stop: Stop<'_>,
    storage: Option<Arc<ProviderSourceLease>>,
) -> Result<Captured> {
    stop.check()?;
    if response.status() != StatusCode::OK
        || response.headers().contains_key(header::CONTENT_RANGE)
        || response
            .headers()
            .get_all(header::CONTENT_ENCODING)
            .iter()
            .any(|value| {
                !value
                    .to_str()
                    .is_ok_and(|encoding| encoding.trim().eq_ignore_ascii_case("identity"))
            })
    {
        return Err(invalid());
    }
    if maximum_bytes == 0 {
        return Err(too_large());
    }
    let declared = response.content_length();
    if declared == Some(0) {
        return Err(invalid());
    }
    if declared.is_some_and(|length| length > maximum_bytes) {
        return Err(too_large());
    }
    let file = create_file(hold.clone(), storage.clone()).await?;
    let mut total = 0_u64;
    let mut digest = Sha256::new();
    while let Some(chunk) = stop.chunk(&mut response).await? {
        total = total
            .checked_add(u64::try_from(chunk.len()).map_err(|_| too_large())?)
            .filter(|total| *total <= maximum_bytes)
            .ok_or_else(too_large)?;
        stop.check()?;
        let hashing = chunk.clone();
        file_operation(file.clone(), hold.clone(), storage.clone(), move |file| {
            let mut writer = file.as_file();
            writer.write_all(&chunk)
        })
        .await?;
        digest.update(&hashing);
        stop.check()?;
    }
    if total == 0 || declared.is_some_and(|length| length != total) {
        return Err(invalid());
    }
    file_operation(file.clone(), hold.clone(), storage.clone(), |file| {
        let mut writer = file.as_file();
        writer.flush()
    })
    .await?;
    stop.check()?;
    Ok(Captured {
        file,
        storage,
        length: total,
        digest: digest.finalize().into(),
    })
}

async fn create_file(
    hold: TeletextWorkHold,
    storage: Option<Arc<ProviderSourceLease>>,
) -> Result<Snapshot> {
    tokio::task::spawn_blocking(move || {
        let _hold = hold;
        let mut builder = tempfile::Builder::new();
        builder.prefix("ersatzrs-finite-source-");
        match &storage {
            Some(storage) => builder.tempfile_in(storage.directory()),
            None => builder.tempfile(),
        }
        .map(Arc::new)
    })
    .await
    .map_err(|_| uncertain())?
    .map_err(|_| invalid())
}

// Field order also protects unwinding: remove/close the actual file owner before
// releasing the work hold, even if its awaiting async future was abandoned.
struct FileOperation {
    file: Snapshot,
    _storage: Option<Arc<ProviderSourceLease>>,
    _hold: TeletextWorkHold,
}

impl FileOperation {
    fn run<T>(
        self,
        operation: impl FnOnce(&NamedTempFile) -> std::io::Result<T>,
    ) -> std::io::Result<T> {
        operation(&self.file)
    }
}

async fn file_operation<T, F>(
    file: Snapshot,
    hold: TeletextWorkHold,
    storage: Option<Arc<ProviderSourceLease>>,
    operation: F,
) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce(&NamedTempFile) -> std::io::Result<T> + Send + 'static,
{
    let retained = FileOperation {
        file,
        _storage: storage,
        _hold: hold,
    };
    tokio::task::spawn_blocking(move || retained.run(operation))
        .await
        .map_err(|_| uncertain())?
        .map_err(|_| invalid())
}

fn invalid() -> FfprobeError {
    FfprobeError::InvalidOutput { path: LABEL.into() }
}
fn cancelled() -> FfprobeError {
    FfprobeError::Cancelled { path: LABEL.into() }
}
fn timed_out() -> FfprobeError {
    FfprobeError::Timeout { path: LABEL.into() }
}
fn too_large() -> FfprobeError {
    FfprobeError::OutputTooLarge { path: LABEL.into() }
}
fn uncertain() -> FfprobeError {
    FfprobeError::CleanupUnconfirmed { path: LABEL.into() }
}

#[cfg(test)]
#[path = "remote_source_snapshot/tests.rs"]
mod tests;
