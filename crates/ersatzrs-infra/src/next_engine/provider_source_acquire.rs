//! One owned admission joins reservation, exact response bytes and native proof.

use ersatzrs_core::locking::TeletextWorkGuard;
use std::sync::Arc;
use tokio::{
    sync::{oneshot, OwnedMutexGuard},
    time::Instant,
};

use super::{
    media::MediaRelay,
    retained_source_origin::{RetainedOrigin, RetainedSourceIntent},
    session_source_files::SessionSourceFiles,
};
use crate::{
    db::DbPool,
    ffprobe::{
        self,
        local_cleanup_scope::{self, OwnerGuard},
        FfprobeError, FfprobeExtractor,
    },
    provider_source_storage::{Policy, ProviderSourceStore},
};

pub(super) struct Acquisition {
    pub pool: DbPool,
    pub relay: Arc<MediaRelay>,
    pub files: Arc<SessionSourceFiles>,
    pub store: ProviderSourceStore,
    pub ffprobe: String,
    pub binding: (i64, i64, i64),
    pub expected: Option<String>,
    pub intent: RetainedSourceIntent,
    pub id: uuid::Uuid,
    pub deadline: Instant,
}

type Reply = oneshot::Sender<ffprobe::Result<Arc<RetainedOrigin>>>;

impl Acquisition {
    pub(super) async fn run(
        self,
        gate: OwnedMutexGuard<()>,
        work: TeletextWorkGuard,
    ) -> ffprobe::Result<Arc<RetainedOrigin>> {
        let scope = self.files.scope();
        let owner = scope
            .scope(async {
                local_cleanup_scope::register_current()
                    .map_err(|_| unavailable())?
                    .ok_or_else(unavailable)
            })
            .await?;
        let (mut reply, receive) = oneshot::channel();
        tokio::spawn(async move {
            let result = scope
                .scope(Box::pin(self.acquire(&mut reply, &owner, &work)))
                .await;
            let confirmed = !matches!(result, Err(FfprobeError::CleanupUnconfirmed { .. }));
            drop(work);
            let _ = reply.send(result);
            drop(gate);
            owner.complete(confirmed);
        });
        receive.await.map_err(|_| uncertain())?
    }

    async fn acquire(
        &self,
        reply: &mut Reply,
        owner: &OwnerGuard,
        work: &TeletextWorkGuard,
    ) -> ffprobe::Result<Arc<RetainedOrigin>> {
        let source = if let Some(source) = self.files.ready(self.id) {
            source
        } else {
            let policy = self
                .stopped(reply, owner, Policy::load(&self.pool))
                .await?
                .map_err(|_| unavailable())?;
            // Never select away the actual reservation. Its receipt must be
            // registered before shutdown can certify that this gate settled.
            let lease = self
                .store
                .reserve(
                    policy,
                    policy.maximum_source_bytes,
                    policy.maximum_source_bytes,
                    work.hold_work(),
                )
                .await
                .map_err(|_| unavailable())?;
            self.files
                .track_cleanup(lease.cleanup())
                .map_err(|_| unavailable())?;
            let response = self
                .stopped(
                    reply,
                    owner,
                    self.relay
                        .open_bound(&self.pool, self.binding, None, self.expected.as_deref()),
                )
                .await?
                .map_err(|_| unavailable())?;
            self.stopped(
                reply,
                owner,
                ffprobe::retain_provider_http_source(
                    response,
                    self.deadline,
                    policy.maximum_source_bytes,
                    work.hold_work(),
                    lease,
                ),
            )
            .await??
        };
        let extractor = FfprobeExtractor::new(self.ffprobe.clone());
        let probe = self
            .stopped(
                reply,
                owner,
                Box::pin(extractor.extract_retained_teletext_report(
                    source.clone(),
                    self.deadline,
                    work.hold_work(),
                )),
            )
            .await??;
        let origin = RetainedOrigin::qualify(self.intent.clone(), source.clone(), probe)
            .map_err(|_| unavailable())?;
        self.files
            .install(self.id, source)
            .map_err(|_| unavailable())?;
        Ok(origin)
    }

    async fn stopped<T>(
        &self,
        reply: &mut Reply,
        owner: &OwnerGuard,
        future: impl std::future::Future<Output = T>,
    ) -> ffprobe::Result<T> {
        tokio::select! {
            biased;
            () = reply.closed() => Err(cancelled()),
            () = owner.cancelled() => Err(cancelled()),
            () = tokio::time::sleep_until(self.deadline) => Err(timed_out()),
            result = future => Ok(result),
        }
    }
}

fn unavailable() -> FfprobeError {
    FfprobeError::InvalidOutput {
        path: "retained provider source preparation".into(),
    }
}
fn cancelled() -> FfprobeError {
    FfprobeError::Cancelled {
        path: "retained provider source preparation".into(),
    }
}
fn timed_out() -> FfprobeError {
    FfprobeError::Timeout {
        path: "retained provider source preparation".into(),
    }
}
fn uncertain() -> FfprobeError {
    FfprobeError::CleanupUnconfirmed {
        path: "retained provider source preparation".into(),
    }
}
