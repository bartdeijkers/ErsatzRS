//! Saved-draft preparation orchestration; publication consumes admitted artifacts only.

use std::path::Path;

use chrono::{Duration, Utc};
use ersatzrs_addon_contract::{CAPABILITY_MEDIA_LIST_PREPARE_V1, CAPABILITY_MEDIA_LIST_PREPARE_V2};
use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::RemoteStreamLiveness;
use sqlx::Row as _;

use crate::{
    addon_media_list_artifacts::{
        AddonMediaListArtifactStore, ArtifactAdmission, ArtifactPromotionError, ArtifactReservation,
    },
    addon_media_list_import::{load_detail_wait_range, sample_provider_delay_seconds},
    addon_media_list_item_coordinator::AddonMediaListItemCoordinator,
    addon_media_list_item_jobs::{
        ClaimedItemOperation, ItemOperationCompletion, ItemOperationFailure, ItemOperationInput,
        ItemOperationKind, ItemOperationStatus,
    },
    addon_media_list_item_runtime::{InstalledItemRuntime, ItemRuntimeError},
    addon_media_list_options::load_draft_item_options,
    addon_media_list_refresh::release_provider_host_lease,
    repositories::{
        common::{decode_enum_column, encode_datetime},
        SqlxResultExt as _,
    },
};

// A reclaimed database lease alone cannot prove an old in-process child has exited.
// Hold this guard until the previous invocation and its managed process owner drop.
static PREPARATION_WORKER: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static PREPARATION_CLEANUP_UNCONFIRMED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// In-process quarantine shared by every provider admission path. This is not
/// restart recovery proof; the host must establish that separately at startup.
pub(crate) fn provider_process_recovery_required() -> bool {
    PREPARATION_CLEANUP_UNCONFIRMED.load(std::sync::atomic::Ordering::Acquire)
}

/// Share the existing admission quarantine when any bounded item process cannot exit.
pub(crate) fn mark_provider_process_recovery_required() {
    PREPARATION_CLEANUP_UNCONFIRMED.store(true, std::sync::atomic::Ordering::Release);
}

enum PreparationAttemptFailure {
    Failed(ItemOperationFailure),
    TerminationUnconfirmed,
}

struct PreparationProcessGuard {
    cleanup_unconfirmed: bool,
}

impl Drop for PreparationProcessGuard {
    fn drop(&mut self) {
        if self.cleanup_unconfirmed {
            PREPARATION_CLEANUP_UNCONFIRMED.store(true, std::sync::atomic::Ordering::Release);
        }
    }
}

impl From<ItemOperationFailure> for PreparationAttemptFailure {
    fn from(failure: ItemOperationFailure) -> Self {
        Self::Failed(failure)
    }
}

impl AddonMediaListItemCoordinator {
    /// Freeze saved finite-item options and the local tool identity, then queue work.
    ///
    /// # Errors
    /// Rejects stale drafts, unsupported preparation, non-finite items and invalid input.
    pub async fn queue_preparation(
        &self,
        list_id: i64,
        item_id: i64,
        revision: i32,
        max_video_height: Option<u32>,
    ) -> Result<i64> {
        self.queue_preparation_impl(list_id, item_id, revision, max_video_height, false)
            .await
    }

    pub(crate) async fn queue_preparation_once(
        &self,
        list_id: i64,
        item_id: i64,
        revision: i32,
        max_video_height: Option<u32>,
    ) -> Result<i64> {
        self.queue_preparation_impl(list_id, item_id, revision, max_video_height, true)
            .await
    }

    #[allow(clippy::too_many_lines)] // Validate and freeze one preparation input in its fenced transaction.
    async fn queue_preparation_impl(
        &self,
        list_id: i64,
        item_id: i64,
        revision: i32,
        max_video_height: Option<u32>,
        automatic: bool,
    ) -> Result<i64> {
        let row = sqlx::query(
            r#"SELECT "ProviderId", "SourceUrl",
                      COALESCE("LivenessOverride", "ProviderLiveness") AS "Liveness"
               FROM "AddonMediaListItem" WHERE "Id"=? AND "AddonMediaListId"=?"#,
        )
        .bind(item_id)
        .bind(list_id)
        .fetch_optional(&self.pool)
        .await
        .repository()?
        .ok_or_else(changed)?;
        let liveness: RemoteStreamLiveness =
            decode_enum_column("RemoteStreamLiveness", &row, "Liveness")?;
        let options = load_draft_item_options(&self.pool, list_id, item_id).await?;
        let addon_id: String =
            sqlx::query_scalar(r#"SELECT "AddonId" FROM "AddonMediaList" WHERE "Id"=?"#)
                .bind(list_id)
                .fetch_one(&self.pool)
                .await
                .repository()?;
        // Read-only transaction: classify delivery; the queue owns the later fenced write.
        let mut tx = self.pool.begin().await.repository()?;
        let delivery = crate::addon_media_list_delivery::effective_item_delivery_in_tx(
            &mut tx,
            list_id,
            &row.try_get::<String, _>("ProviderId").repository()?,
            liveness,
        )
        .await?;
        tx.commit().await.repository()?;
        let generic = delivery == ersatzrs_domain::AddonMediaListDeliveryMode::DownloadAhead;
        if liveness != RemoteStreamLiveness::Finite
            || (!generic
                && !options
                    .as_ref()
                    .is_some_and(|options| options.requires_preparation(liveness)))
        {
            return Err(RepositoryError::Conflict(
                "this item does not require finite preparation".into(),
            ));
        }
        let source_url = row
            .try_get::<Option<String>, _>("SourceUrl")
            .repository()?
            .ok_or_else(changed)?;
        let provider_id: String = row.try_get("ProviderId").repository()?;
        let runtime = InstalledItemRuntime::load(&self.pool, &addon_id, None)
            .await
            .map_err(runtime_error)?;
        if let Some(options) = &options {
            runtime.validate_options(options).map_err(runtime_error)?;
        }
        let installed = crate::addons::load_installed_addon(&self.pool, &addon_id)
            .await
            .map_err(|_| changed())?
            .ok_or_else(changed)?;
        if !installed.manifest.has_capability(if generic {
            CAPABILITY_MEDIA_LIST_PREPARE_V2
        } else {
            CAPABILITY_MEDIA_LIST_PREPARE_V1
        }) {
            return Err(RepositoryError::unavailable(
                "item preparation",
                "the item operation is unsupported",
            ));
        }
        let tool_version = runtime
            .runtime_info()
            .await
            .map_err(runtime_error)?
            .tool_version;
        let package_identity = runtime.package_identity().to_owned();
        let mut input = ItemOperationInput {
            source_url,
            provider_id,
            context_fingerprint: String::new(),
            schema: options.as_ref().map(|options| options.schema.clone()),
            definition: options.as_ref().map(|options| options.definition.clone()),
            options: options.as_ref().map(|options| options.values.clone()),
            addon_id: options.is_none().then_some(addon_id),
            preparation_v2: generic.then_some(true),
            package_identity,
            tool_version,
            item_is_finite: true,
            max_video_height,
            requested_timing: None,
        };
        input.context_fingerprint = input.base_fingerprint()?;
        if automatic {
            self.queue
                .enqueue_once(
                    list_id,
                    item_id,
                    revision,
                    ItemOperationKind::Preparation,
                    input,
                    Utc::now(),
                )
                .await
        } else {
            self.queue
                .enqueue(
                    list_id,
                    item_id,
                    revision,
                    ItemOperationKind::Preparation,
                    input,
                    Utc::now(),
                )
                .await
        }
    }

    /// Read the latest preparation for this exact item, hiding changed saved/tool context.
    ///
    /// # Errors
    /// Returns an error for unavailable runtime identity or invalid database state.
    pub async fn preparation_status(
        &self,
        list_id: i64,
        item_id: i64,
    ) -> Result<Option<ItemOperationStatus>> {
        let row = sqlx::query(
            r#"SELECT "Id", "SavedDraftRevision", "InputJson" FROM "AddonMediaListItemOperationJob"
               WHERE "AddonMediaListId"=? AND "AddonMediaListItemId"=? AND "Kind"=1
               ORDER BY "Id" DESC LIMIT 1"#,
        )
        .bind(list_id)
        .bind(item_id)
        .fetch_optional(&self.pool)
        .await
        .repository()?;
        let Some(row) = row else {
            return Ok(None);
        };
        let id: i64 = row.try_get("Id").repository()?;
        let Some(mut status) = self.queue.status(id, Utc::now()).await? else {
            return Ok(None);
        };
        if matches!(status.state, 0..=2) {
            let input: ItemOperationInput =
                serde_json::from_str(&row.try_get::<String, _>("InputJson").repository()?)
                    .map_err(|_| changed())?;
            let saved_revision: i32 = row.try_get("SavedDraftRevision").repository()?;
            let current = self
                .preparation_context_is_current(list_id, item_id, saved_revision, &input)
                .await?;
            if !current {
                status.state = 5;
                status.completion = None;
            }
        }
        Ok(Some(status))
    }

    /// Cancel an active preparation owned by this list item.
    ///
    /// # Errors
    /// Rejects missing, mismatched or inactive jobs and database failures.
    pub async fn cancel_preparation(&self, list_id: i64, item_id: i64, job_id: i64) -> Result<()> {
        let owns: bool = sqlx::query_scalar(
            r#"SELECT EXISTS(SELECT 1 FROM "AddonMediaListItemOperationJob"
               WHERE "Id"=? AND "AddonMediaListId"=? AND "AddonMediaListItemId"=? AND "Kind"=1)"#,
        )
        .bind(job_id)
        .bind(list_id)
        .bind(item_id)
        .fetch_one(&self.pool)
        .await
        .repository()?;
        if !owns {
            return Err(changed());
        }
        self.queue.cancel(job_id, Utc::now()).await
    }

    async fn preparation_context_is_current(
        &self,
        list_id: i64,
        item_id: i64,
        saved_revision: i32,
        input: &ItemOperationInput,
    ) -> Result<bool> {
        let row = sqlx::query(r#"SELECT "list"."DraftRevision", "item"."SourceUrl", "item"."ProviderId",
            COALESCE("item"."LivenessOverride", "item"."ProviderLiveness") AS "Liveness"
            FROM "AddonMediaList" AS "list" JOIN "AddonMediaListItem" AS "item" ON "item"."AddonMediaListId"="list"."Id"
            WHERE "list"."Id"=? AND "item"."Id"=?"#)
            .bind(list_id).bind(item_id).fetch_optional(&self.pool).await.repository()?;
        let Some(row) = row else {
            return Ok(false);
        };
        let liveness: RemoteStreamLiveness =
            decode_enum_column("RemoteStreamLiveness", &row, "Liveness")?;
        if row.try_get::<i32, _>("DraftRevision").repository()? != saved_revision
            || row
                .try_get::<Option<String>, _>("SourceUrl")
                .repository()?
                .as_deref()
                != Some(input.source_url.as_str())
            || row.try_get::<String, _>("ProviderId").repository()? != input.provider_id
            || liveness != RemoteStreamLiveness::Finite
        {
            return Ok(false);
        }
        let options = load_draft_item_options(&self.pool, list_id, item_id).await?;
        if options != input.resolved_options()? {
            return Ok(false);
        }
        let Ok(runtime) = InstalledItemRuntime::load(
            &self.pool,
            input.addon_id()?,
            Some(&input.package_identity),
        )
        .await
        else {
            return Ok(false);
        };
        Ok(runtime
            .runtime_info()
            .await
            .map_err(runtime_error)?
            .tool_version
            == input.tool_version)
    }

    /// Execute one bounded preparation, outside the publication lease.
    ///
    /// Do not run two host processes against the same database. Unfinished
    /// predecessor reservations block preparation before any provider invocation;
    /// lease expiry and application restart are not proof of process termination.
    /// Actual restart recovery must independently confirm descendant termination
    /// before reclaiming those files and reservations. This worker does not do so.
    ///
    /// # Errors
    /// Returns database/ownership/cleanup failures; provider failures become typed job outcomes.
    pub async fn process_one_preparation(&self, root: &Path) -> Result<bool> {
        let (_sender, cancellation) = tokio::sync::oneshot::channel();
        self.process_one_preparation_cancellable(root, cancellation)
            .await
    }

    /// Execute one preparation while observing host shutdown.
    ///
    /// Sending or dropping the cancellation sender requests shutdown. The caller
    /// must await this future to retain ownership until managed cleanup finishes.
    /// Unknown predecessor reservations remain blocked; this is not restart recovery.
    ///
    /// # Errors
    /// Returns database/ownership/cleanup failures without releasing unconfirmed work.
    #[allow(clippy::too_many_lines)] // Keep claim, process ownership and admission release in one scope.
    pub async fn process_one_preparation_cancellable(
        &self,
        root: &Path,
        cancellation: tokio::sync::oneshot::Receiver<()>,
    ) -> Result<bool> {
        self.process_one_preparation_inner(root, cancellation, None)
            .await
    }

    pub(crate) async fn process_one_preparation_supervised(
        &self,
        root: &Path,
        cancellation: tokio::sync::oneshot::Receiver<()>,
        session: &crate::addon_preparation_worker::WorkerSession,
    ) -> Result<bool> {
        self.process_one_preparation_inner(root, cancellation, Some(session))
            .await
    }

    async fn process_one_preparation_inner(
        &self,
        root: &Path,
        mut cancellation: tokio::sync::oneshot::Receiver<()>,
        session: Option<&crate::addon_preparation_worker::WorkerSession>,
    ) -> Result<bool> {
        let _worker = tokio::select! {
            biased;
            _ = &mut cancellation => return Ok(false),
            worker = PREPARATION_WORKER.lock() => worker,
        };
        if PREPARATION_CLEANUP_UNCONFIRMED.load(std::sync::atomic::Ordering::Acquire) {
            return Err(RepositoryError::unavailable(
                "item preparation",
                "provider process cleanup requires host recovery",
            ));
        }
        let Some(job) = self
            .queue
            .claim_kind(Utc::now(), 90, ItemOperationKind::Preparation)
            .await?
        else {
            return Ok(false);
        };
        if let Some(session) = session {
            session.begin(&job).await?;
        }
        let result = self
            .process_claimed_preparation(root, &job, &mut cancellation)
            .await;
        if let Some(session) = session {
            session
                .finish(root, &job, !provider_process_recovery_required())
                .await?;
        }
        result
    }

    #[allow(clippy::too_many_lines)] // One claimed attempt owns all provider/probe cleanup.
    async fn process_claimed_preparation(
        &self,
        root: &Path,
        job: &ClaimedItemOperation,
        cancellation: &mut tokio::sync::oneshot::Receiver<()>,
    ) -> Result<bool> {
        // Preparation is globally serialized. A reservation left by any earlier
        // attempt can still belong to a live descendant, including after restart.
        // Keep its quota charged and expose recovery as an explicit terminal
        // outcome instead of spending retries or treating it as ordinary storage pressure.
        let recovery_required: bool = sqlx::query_scalar(
            r#"SELECT EXISTS(SELECT 1 FROM "AddonMediaListArtifact"
               WHERE "Kind"=1 AND "State" IN (0,1))"#,
        )
        .fetch_one(&self.pool)
        .await
        .repository()?;
        if recovery_required {
            return self
                .fail_preparation(job, ItemOperationFailure::ProcessRecoveryRequired)
                .await;
        }
        let Ok(runtime) = InstalledItemRuntime::load(
            &self.pool,
            job.input.addon_id()?,
            Some(&job.input.package_identity),
        )
        .await
        else {
            return self
                .fail_preparation(job, ItemOperationFailure::InputChanged)
                .await;
        };
        let (cancel_probe, probe_cancellation) = tokio::sync::oneshot::channel();
        let probe = runtime.runtime_info_cancellable(probe_cancellation);
        tokio::pin!(probe);
        let mut probe_guard = PreparationProcessGuard {
            cleanup_unconfirmed: true,
        };
        let info = tokio::select! {
            biased;
            _ = &mut *cancellation => {
                let _ = cancel_probe.send(());
                probe_guard.cleanup_unconfirmed = matches!(probe.await, Err(ItemRuntimeError::TerminationUnconfirmed));
                if probe_guard.cleanup_unconfirmed {
                    return Err(runtime_error(ItemRuntimeError::TerminationUnconfirmed));
                }
                self.queue.recover_interrupted_after_cleanup(&job.into(), Utc::now()).await?;
                return Ok(true);
            },
            result = &mut probe => result,
        };
        probe_guard.cleanup_unconfirmed =
            matches!(info, Err(ItemRuntimeError::TerminationUnconfirmed));
        if probe_guard.cleanup_unconfirmed {
            return Err(runtime_error(ItemRuntimeError::TerminationUnconfirmed));
        }
        let Ok(info) = info else {
            let now = Utc::now();
            self.queue
                .defer(job, now, now + Duration::seconds(30))
                .await?;
            return Ok(true);
        };
        let version = info.tool_version;
        if !matches!(
            cancellation.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty)
        ) {
            self.queue
                .recover_interrupted_after_cleanup(&job.into(), Utc::now())
                .await?;
            return Ok(true);
        }
        if version != job.input.tool_version {
            return self
                .fail_preparation(job, ItemOperationFailure::InputChanged)
                .await;
        }
        let store = AddonMediaListArtifactStore::new(self.pool.clone());
        let host = url::Url::parse(&job.input.source_url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
            .ok_or_else(changed)?;
        let delay = sample_provider_delay_seconds(
            uuid::Uuid::new_v4().as_bytes(),
            load_detail_wait_range(&self.pool).await?,
        );
        if let Some(next) = self.admit_provider(job, &host, delay).await? {
            let now = Utc::now();
            self.queue
                .defer_unstarted(job, now, next.max(now + Duration::seconds(1)))
                .await?;
            return Ok(true);
        }
        // Release admission only after process cleanup is confirmed. An unknown
        // process retains both its artifact reservation and provider ownership.
        let outcome = self
            .run_preparation(job, &runtime, &store, root, &host, cancellation)
            .await;
        if PREPARATION_CLEANUP_UNCONFIRMED.load(std::sync::atomic::Ordering::Acquire) {
            return outcome.map(|()| true);
        }
        let now = Utc::now();
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        release_provider_host_lease(
            &mut tx,
            &host,
            &job.lease_owner,
            &encode_datetime(now),
            &encode_datetime(now + Duration::seconds(i64::from(delay))),
            delay,
        )
        .await?;
        tx.commit().await.repository()?;
        outcome?;
        Ok(true)
    }

    async fn fail_preparation(
        &self,
        job: &ClaimedItemOperation,
        failure: ItemOperationFailure,
    ) -> Result<bool> {
        self.queue
            .complete(job, ItemOperationCompletion::Failed(failure), Utc::now())
            .await?;
        Ok(true)
    }

    async fn run_preparation(
        &self,
        job: &ClaimedItemOperation,
        runtime: &InstalledItemRuntime,
        store: &AddonMediaListArtifactStore,
        root: &Path,
        host: &str,
        cancellation: &mut tokio::sync::oneshot::Receiver<()>,
    ) -> Result<()> {
        if !matches!(
            cancellation.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty)
        ) {
            return self
                .queue
                .recover_interrupted_after_cleanup(&job.into(), Utc::now())
                .await;
        }
        let reservation = match store.reserve(job, root, Utc::now()).await {
            Ok(ArtifactAdmission::Ready(artifact)) => {
                return self
                    .queue
                    .complete(
                        job,
                        ItemOperationCompletion::Prepared {
                            artifact_id: artifact.id,
                        },
                        Utc::now(),
                    )
                    .await;
            }
            Ok(ArtifactAdmission::Reserved(reservation)) => reservation,
            Err(RepositoryError::Conflict(message))
                if message == "processed artifact capacity is unavailable" =>
            {
                self.fail_preparation(job, ItemOperationFailure::StorageFull)
                    .await?;
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        let result = self
            .prepare_reserved(job, runtime, store, &reservation, host, cancellation)
            .await;
        match result {
            Ok(Some(artifact_id)) => {
                self.queue
                    .complete(
                        job,
                        ItemOperationCompletion::Prepared { artifact_id },
                        Utc::now(),
                    )
                    .await
            }
            Ok(None) => {
                store.release_after_cleanup(job, &reservation).await?;
                // Cancellation/changed context already fenced the result. No stale success.
                Ok(())
            }
            Err(PreparationAttemptFailure::TerminationUnconfirmed) => {
                // Keep the reservation charged until host recovery proves process cleanup.
                PREPARATION_CLEANUP_UNCONFIRMED.store(true, std::sync::atomic::Ordering::Release);
                Err(RepositoryError::unavailable(
                    "item preparation",
                    "provider process termination could not be confirmed",
                ))
            }
            Err(PreparationAttemptFailure::Failed(failure)) => {
                store.release_after_cleanup(job, &reservation).await?;
                if failure == ItemOperationFailure::Cancelled {
                    self.queue
                        .recover_interrupted_after_cleanup(&job.into(), Utc::now())
                        .await
                } else if failure == ItemOperationFailure::Failed {
                    let now = Utc::now();
                    self.queue
                        .defer(job, now, now + Duration::seconds(30))
                        .await
                } else {
                    self.fail_preparation(job, failure).await.map(|_| ())
                }
            }
        }
    }

    #[allow(clippy::too_many_lines)] // Provider and probe cancellation retain the same reservation owner.
    async fn prepare_reserved(
        &self,
        job: &ClaimedItemOperation,
        runtime: &InstalledItemRuntime,
        store: &AddonMediaListArtifactStore,
        reservation: &ArtifactReservation,
        host: &str,
        cancellation: &mut tokio::sync::oneshot::Receiver<()>,
    ) -> std::result::Result<Option<i64>, PreparationAttemptFailure> {
        let request = reservation
            .prepare_request(job)
            .map_err(|_| ItemOperationFailure::InputChanged)?;
        let result = {
            // An externally dropped worker cannot asynchronously reap its child. Keep
            // subsequent preparation disabled. A restart still requires independent
            // termination proof before its retained reservation can be reclaimed.
            let mut process_guard = PreparationProcessGuard {
                cleanup_unconfirmed: true,
            };
            let (cancel, provider_cancellation) = tokio::sync::oneshot::channel();
            let operation = async {
                if job.input.preparation_v2 == Some(true) {
                    runtime
                        .prepare_generic_cancellable(&request, provider_cancellation)
                        .await
                } else {
                    runtime
                        .prepare_cancellable(&request, provider_cancellation)
                        .await
                }
            };
            tokio::pin!(operation);
            let monitor = async {
                let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(1));
                loop {
                    heartbeat.tick().await;
                    let lost_claim = self.heartbeat_provider(job, host).await.is_err();
                    let over_budget =
                        !lost_claim && store.check_staging(reservation).await.is_err();
                    if lost_claim || over_budget {
                        return lost_claim;
                    }
                }
            };
            // A contended database or slow staging walk must not suspend the
            // provider owner or prevent shutdown from requesting process cleanup.
            tokio::select! {
                    biased;
                    _ = &mut *cancellation => {
                        let _ = cancel.send(());
                        if matches!(operation.await, Err(ItemRuntimeError::TerminationUnconfirmed)) {
                            return Err(PreparationAttemptFailure::TerminationUnconfirmed);
                        }
                        process_guard.cleanup_unconfirmed = false;
                        return Err(ItemOperationFailure::Cancelled.into());
                    },
                    result = &mut operation => {
                        process_guard.cleanup_unconfirmed = matches!(result, Err(ItemRuntimeError::TerminationUnconfirmed));
                        result
                    },
                    lost_claim = monitor => {
                            let _ = cancel.send(());
                            if matches!(operation.await, Err(ItemRuntimeError::TerminationUnconfirmed)) {
                                return Err(PreparationAttemptFailure::TerminationUnconfirmed);
                            }
                            process_guard.cleanup_unconfirmed = false;
                            return if lost_claim { Ok(None) } else { Err(ItemOperationFailure::StorageFull.into()) };
                    }
            }
        }; // Drop the managed provider owner before touching its staging directory.
        let result = result.map_err(|error| match error {
            ItemRuntimeError::TerminationUnconfirmed => {
                PreparationAttemptFailure::TerminationUnconfirmed
            }
            ItemRuntimeError::PackageChanged | ItemRuntimeError::InvalidRequest => {
                ItemOperationFailure::InputChanged.into()
            }
            ItemRuntimeError::Unsupported => ItemOperationFailure::Unsupported.into(),
            _ => ItemOperationFailure::Failed.into(),
        })?;
        self.promote_reserved(
            job,
            store,
            reservation,
            &result,
            host,
            cancellation,
            &ersatzrs_ffmpeg::ffprobe_path(),
        )
        .await
    }

    #[allow(clippy::too_many_arguments)] // Keep the fenced reservation and explicit probe executable together.
    async fn promote_reserved(
        &self,
        job: &ClaimedItemOperation,
        store: &AddonMediaListArtifactStore,
        reservation: &ArtifactReservation,
        result: &ersatzrs_addon_contract::MediaListPrepareResult,
        host: &str,
        cancellation: &mut tokio::sync::oneshot::Receiver<()>,
        ffprobe_path: &str,
    ) -> std::result::Result<Option<i64>, PreparationAttemptFailure> {
        // Probe/promotion may outlast a heartbeat interval; keep the claim live without
        // cancelling an in-progress atomic file/database promotion on an unrelated tick.
        let (cancel_probe, probe_cancellation) = tokio::sync::oneshot::channel();
        let promotion = store.promote_cancellable(
            job,
            reservation,
            result,
            ffprobe_path,
            Utc::now(),
            probe_cancellation,
        );
        tokio::pin!(promotion);
        let mut process_guard = PreparationProcessGuard {
            cleanup_unconfirmed: true,
        };
        let heartbeat = async {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
            loop {
                interval.tick().await;
                if self.heartbeat_provider(job, host).await.is_err() {
                    return;
                }
            }
        };
        // Promotion may hold the SQLite write lock while awaiting filesystem work.
        // Poll both futures concurrently so a waiting heartbeat never suspends the
        // promotion which must finish before that heartbeat can acquire its lock.
        let result = tokio::select! {
            biased;
            _ = &mut *cancellation => {
                let _ = cancel_probe.send(());
                match promotion.await {
                    Ok(artifact) => Ok(Some(artifact.id)),
                    Err(ArtifactPromotionError::TerminationUnconfirmed) => Err(PreparationAttemptFailure::TerminationUnconfirmed),
                    Err(ArtifactPromotionError::Repository(_)) => Err(ItemOperationFailure::Cancelled.into()),
                }
            },
            result = &mut promotion => result.map(|artifact| Some(artifact.id)).map_err(|error| match error {
                ArtifactPromotionError::TerminationUnconfirmed => PreparationAttemptFailure::TerminationUnconfirmed,
                ArtifactPromotionError::Repository(_) => ItemOperationFailure::Failed.into(),
            }),
            () = heartbeat => {
                let _ = cancel_probe.send(());
                // A lost claim fences publication, but never permits dropping a
                // running probe or an atomic file/database promotion.
                match promotion.await {
                    Ok(artifact) => Ok(Some(artifact.id)),
                    Err(ArtifactPromotionError::TerminationUnconfirmed) => Err(PreparationAttemptFailure::TerminationUnconfirmed),
                    Err(ArtifactPromotionError::Repository(_)) => Ok(None),
                }
            },
        };
        process_guard.cleanup_unconfirmed = matches!(
            result,
            Err(PreparationAttemptFailure::TerminationUnconfirmed)
        );
        result
    }
}

fn changed() -> RepositoryError {
    RepositoryError::Conflict("the saved item preparation context changed".into())
}

fn runtime_error(_: ItemRuntimeError) -> RepositoryError {
    RepositoryError::unavailable("item preparation", "the item operation is unavailable")
}

#[cfg(all(test, unix))]
pub(crate) mod tests {
    use std::fmt::Write as _;

    use super::*;
    use ersatzrs_core::repositories::{
        AddonMediaListLifecycleRepository as _, AddonMediaListOptionSchema,
        AddonMediaListOptionSchemaRef,
    };
    use sha2::{Digest as _, Sha256};

    pub(crate) async fn fixture(root: &Path) -> sqlx::SqlitePool {
        fixture_with_script(root, "#!/bin/sh\nset -eu\nprintf '%s\\n' \"$1\" >> ../preparation-invocations\nrequest=$(cat)\ncase \"$1\" in\n runtime-info) printf '%s\\n' '{\"tool_version\":\"1.0\"}' ;;\n *) exit 1 ;;\nesac\n").await
    }

    pub(crate) async fn fixture_with_script(root: &Path, script: &str) -> sqlx::SqlitePool {
        let pool = crate::addon_media_list::tests::fixture_media_list_addon_with_script(
            root,
            script,
            &["example.test"],
        )
        .await;
        let original: String =
            sqlx::query_scalar(r#"SELECT "ManifestJson" FROM "AddonInstallation" LIMIT 1"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        let mut manifest: serde_json::Value = serde_json::from_str(&original).unwrap();
        manifest["manifest_version"] = serde_json::json!(5);
        let declaration = serde_json::json!({"id":"media-list.options.v1","options":[{
            "key":"enabled", "label":{"en-US":"Enabled"}, "description":{"en-US":"Fixture"},
            "scope":"both", "operation_targets":["preparation"], "applicability":"finite_only",
            "kind":"boolean", "default":true, "requires_preparation":true
        }]});
        manifest["capabilities"].as_array_mut().unwrap().extend([
            serde_json::json!({"id":"media-list.item-runtime.v1"}),
            serde_json::json!({"id":"media-list.prepare.v1"}),
            declaration.clone(),
        ]);
        sqlx::query(r#"UPDATE "AddonInstallation" SET "ManifestJson"=?"#)
            .bind(manifest.to_string())
            .execute(&pool)
            .await
            .unwrap();
        let declaration: ersatzrs_addon_contract::AddonCapability =
            serde_json::from_value(declaration).unwrap();
        let definition_json = serde_json::to_string(&declaration).unwrap();
        let mut schema_digest = String::with_capacity(64);
        for byte in Sha256::digest(definition_json.as_bytes()) {
            write!(schema_digest, "{byte:02x}").unwrap();
        }
        let schema = AddonMediaListOptionSchemaRef {
            addon_id: "org.ersatzrs.addon.fixture".into(),
            capability_id: declaration.id.clone(),
            schema_digest,
        };
        crate::repositories::SqliteAddonMediaListRepository(pool.clone())
            .register_option_schema(&AddonMediaListOptionSchema {
                schema: schema.clone(),
                definition_json,
                created_at: Utc::now(),
            })
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO "AddonMediaList" ("Id","AddonId","ProviderId","SourceUrl","Name","DraftOptionSchemaId")
            VALUES (1,'org.ersatzrs.addon.fixture','list','https://example.test/list','Fixture',
            (SELECT "Id" FROM "AddonMediaListOptionSchema" WHERE "SchemaDigest"=?))"#)
            .bind(&schema.schema_digest).execute(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO "AddonMediaListItem" ("Id","AddonMediaListId","ProviderId","DisplayTitle","Kind","Rank","Title","SourceUrl","ProviderLiveness")
            VALUES (10,1,'item','Fixture',4,0,'Fixture','https://example.test/item',1)"#)
            .execute(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn generic_download_without_options_requires_v2_and_queues_owned_work() {
        let temporary = tempfile::tempdir().unwrap();
        let pool = fixture(&temporary.path().join("package")).await;
        sqlx::query(r#"UPDATE "AddonMediaList" SET "DraftOptionSchemaId"=NULL,"DeliveryMode"=1 WHERE "Id"=1"#)
            .execute(&pool).await.unwrap();
        let original: String =
            sqlx::query_scalar(r#"SELECT "ManifestJson" FROM "AddonInstallation" LIMIT 1"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        let mut manifest: serde_json::Value = serde_json::from_str(&original).unwrap();
        manifest["capabilities"]
            .as_array_mut()
            .unwrap()
            .retain(|capability| capability["id"] != "media-list.options.v1");
        sqlx::query(r#"UPDATE "AddonInstallation" SET "ManifestJson"=?"#)
            .bind(manifest.to_string())
            .execute(&pool)
            .await
            .unwrap();
        let coordinator = AddonMediaListItemCoordinator::new(pool.clone());
        assert!(
            coordinator.queue_preparation(1, 10, 0, None).await.is_err(),
            "legacy prepare must not promise generic downloads"
        );
        manifest["capabilities"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"id":"media-list.prepare.v2"}));
        sqlx::query(r#"UPDATE "AddonInstallation" SET "ManifestJson"=?"#)
            .bind(manifest.to_string())
            .execute(&pool)
            .await
            .unwrap();
        let id = coordinator.queue_preparation(1, 10, 0, None).await.unwrap();
        let job = coordinator
            .queue
            .claim_kind(Utc::now(), 90, ItemOperationKind::Preparation)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(job.id, id);
        assert_eq!(job.input.preparation_v2, Some(true));
        assert!(job.input.resolved_options().unwrap().is_none());
        assert_eq!(job.input.addon_id().unwrap(), "org.ersatzrs.addon.fixture");
        coordinator
            .queue
            .complete(
                &job,
                ItemOperationCompletion::Failed(ItemOperationFailure::Failed),
                Utc::now(),
            )
            .await
            .unwrap();
        assert_eq!(
            coordinator
                .queue_preparation_once(1, 10, 0, None)
                .await
                .unwrap(),
            id
        );
        assert_eq!(
            coordinator
                .queue
                .status(id, Utc::now())
                .await
                .unwrap()
                .unwrap()
                .state,
            3
        );
    }

    #[tokio::test]
    async fn preparation_shutdown_before_claim_leaves_job_queued() {
        let temporary = tempfile::tempdir().unwrap();
        let pool = fixture(&temporary.path().join("package")).await;
        let coordinator = AddonMediaListItemCoordinator::new(pool);
        let id = coordinator.queue_preparation(1, 10, 0, None).await.unwrap();
        let (cancel, cancellation) = tokio::sync::oneshot::channel();
        cancel.send(()).unwrap();
        assert!(!coordinator
            .process_one_preparation_cancellable(&temporary.path().join("artifacts"), cancellation,)
            .await
            .unwrap());
        assert_eq!(
            coordinator
                .queue
                .status(id, Utc::now())
                .await
                .unwrap()
                .unwrap()
                .state,
            0
        );
    }

    #[tokio::test]
    async fn preparation_shutdown_awaits_provider_exit_before_releasing_reservation() {
        Box::pin(shutdown_awaits_process_exit(false, false)).await;
    }

    #[tokio::test]
    async fn preparation_shutdown_awaits_runtime_probe_exit() {
        Box::pin(shutdown_awaits_process_exit(true, false)).await;
    }

    #[tokio::test]
    async fn preparation_shutdown_kills_provider_while_heartbeat_database_is_locked() {
        Box::pin(shutdown_awaits_process_exit(false, true)).await;
    }

    #[allow(clippy::too_many_lines)] // Exercise cancellation, process exit and retained quota in one fixture.
    async fn shutdown_awaits_process_exit(runtime_probe: bool, lock_database: bool) {
        let temporary = tempfile::tempdir().unwrap();
        let pool = fixture_with_script(&temporary.path().join("package"),
            "#!/bin/sh\nset -eu\nrequest=$(cat)\ncase \"$1\" in\n runtime-info) if [ -f ../block-runtime ]; then echo $$ > ../provider-pid; exec sleep 30; fi; printf '%s\\n' '{\"tool_version\":\"1.0\"}' ;;\n prepare) echo $$ > ../provider-pid; exec sleep 30 ;;\n *) exit 1 ;;\nesac\n",
        ).await;
        let coordinator = AddonMediaListItemCoordinator::new(pool.clone());
        let id = coordinator.queue_preparation(1, 10, 0, None).await.unwrap();
        if runtime_probe {
            tokio::fs::write(temporary.path().join("block-runtime"), b"block")
                .await
                .unwrap();
        }
        let root = temporary.path().join("artifacts");
        let (cancel, cancellation) = tokio::sync::oneshot::channel();
        let operation = coordinator.process_one_preparation_cancellable(&root, cancellation);
        tokio::pin!(operation);
        let pid_path = temporary.path().join("provider-pid");
        let started = async {
            loop {
                if let Ok(pid) = tokio::fs::read_to_string(&pid_path).await {
                    break pid;
                }
                tokio::task::yield_now().await;
            }
        };
        let pid = tokio::select! {
            result = &mut operation => panic!("provider exited before cancellation: {result:?}"),
            pid = tokio::time::timeout(std::time::Duration::from_secs(10), started) => pid.unwrap(),
        };
        let database_lock = if lock_database {
            let lock = tokio::select! {
                result = &mut operation => panic!("provider exited before lock: {result:?}"),
                lock = crate::db::begin_write(&pool) => lock.unwrap(),
            };
            // Poll the worker through its next heartbeat while SQLite's writer
            // remains held. Cancellation must still reach the provider process.
            tokio::select! {
                result = &mut operation => panic!("provider exited while locked: {result:?}"),
                () = tokio::time::sleep(std::time::Duration::from_millis(1100)) => {},
            }
            Some(lock)
        } else {
            None
        };
        cancel.send(()).unwrap();
        if let Some(lock) = database_lock {
            let exited = async {
                while process_is_alive(pid.trim()).await {
                    tokio::task::yield_now().await;
                }
            };
            tokio::select! {
                result = &mut operation => panic!("cleanup released a locked reservation: {result:?}"),
                result = tokio::time::timeout(std::time::Duration::from_secs(8), exited) => result.unwrap(),
            }
            lock.rollback().await.unwrap();
        }
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(8), operation)
                .await
                .unwrap()
                .unwrap()
        );
        assert!(
            !process_is_alive(pid.trim()).await,
            "provider must have exited before worker returns"
        );
        assert_eq!(
            coordinator
                .queue
                .status(id, Utc::now())
                .await
                .unwrap()
                .unwrap()
                .state,
            0
        );
        let reserved: i64 = sqlx::query_scalar(
            r#"SELECT COALESCE(SUM("ReservedBytes"),0) FROM "AddonMediaListArtifact""#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(reserved, 0);
        assert!(!provider_process_recovery_required());
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)] // Keep the synthetic probe, ownership loss and cleanup assertions together.
    async fn preparation_lost_ownership_cancels_final_probe_before_storage_release() {
        use std::os::unix::fs::PermissionsExt as _;
        let temporary = tempfile::tempdir().unwrap();
        let pool = fixture(&temporary.path().join("package")).await;
        let coordinator = AddonMediaListItemCoordinator::new(pool.clone());
        let id = coordinator.queue_preparation(1, 10, 0, None).await.unwrap();
        let job = coordinator
            .queue
            .claim_kind(Utc::now(), 90, ItemOperationKind::Preparation)
            .await
            .unwrap()
            .unwrap();
        assert!(coordinator
            .admit_provider(&job, "example.test", 0)
            .await
            .unwrap()
            .is_none());
        let store = AddonMediaListArtifactStore::new(pool.clone());
        let ArtifactAdmission::Reserved(reservation) = store
            .reserve(&job, &temporary.path().join("artifacts"), Utc::now())
            .await
            .unwrap()
        else {
            panic!("expected reservation")
        };
        let media = reservation.staging_directory.join("fixture.media");
        tokio::fs::write(&media, b"fixture").await.unwrap();
        let mut digest = String::with_capacity(64);
        for byte in Sha256::digest(b"fixture") {
            write!(digest, "{byte:02x}").unwrap();
        }
        let result = ersatzrs_addon_contract::MediaListPrepareResult {
            provider_id: job.input.provider_id.clone(),
            context_fingerprint: job.input.context_fingerprint.clone(),
            tool_version: job.input.tool_version.clone(),
            selected_format: "fixture".into(),
            relative_path: "fixture.media".into(),
            byte_length: 7,
            sha256: digest,
            duration_milliseconds: 1000,
        };
        let probe = temporary.path().join("probe.sh");
        tokio::fs::write(&probe, "#!/bin/sh\necho $$ > probe-pid\nexec sleep 30\n")
            .await
            .unwrap();
        tokio::fs::set_permissions(&probe, std::fs::Permissions::from_mode(0o755))
            .await
            .unwrap();
        let (_cancel, mut cancellation) = tokio::sync::oneshot::channel();
        let operation = coordinator.promote_reserved(
            &job,
            &store,
            &reservation,
            &result,
            "example.test",
            &mut cancellation,
            probe.to_str().unwrap(),
        );
        tokio::pin!(operation);
        let started = async {
            loop {
                if let Ok(pid) =
                    tokio::fs::read_to_string(reservation.staging_directory.join("probe-pid")).await
                {
                    break pid;
                }
                tokio::task::yield_now().await;
            }
        };
        let pid = tokio::select! {
            _ = &mut operation => panic!("probe finished before ownership loss"),
            pid = tokio::time::timeout(std::time::Duration::from_secs(10), started) => pid.unwrap(),
        };
        tokio::select! {
            _ = &mut operation => panic!("probe finished before cancellation"),
            cancelled = coordinator.queue.cancel(id, Utc::now()) => cancelled.unwrap(),
        }
        let outcome = tokio::time::timeout(std::time::Duration::from_secs(8), operation)
            .await
            .unwrap();
        assert!(matches!(outcome, Ok(None)));
        assert!(!process_is_alive(pid.trim()).await);
        assert!(tokio::fs::try_exists(&media).await.unwrap());
        store
            .release_after_cleanup(&job, &reservation)
            .await
            .unwrap();
        assert!(!tokio::fs::try_exists(&media).await.unwrap());
    }

    async fn process_is_alive(pid: &str) -> bool {
        tokio::process::Command::new("kill")
            .args(["-0", pid])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .unwrap()
            .success()
    }

    #[tokio::test]
    async fn expired_preparation_reservation_survives_reclaimed_job_without_provider_call() {
        unfinished_reservation_blocks_preparation(false).await;
    }

    #[tokio::test]
    async fn unfinished_other_item_reservation_also_blocks_preparation() {
        unfinished_reservation_blocks_preparation(true).await;
    }

    #[allow(clippy::too_many_lines)] // Keep shared admission/recovery fixture and exact refusal assertions together.
    async fn unfinished_reservation_blocks_preparation(other_item: bool) {
        let temporary = tempfile::tempdir().unwrap();
        let pool = fixture(&temporary.path().join("package")).await;
        let coordinator = AddonMediaListItemCoordinator::new(pool.clone());
        let job_id = coordinator.queue_preparation(1, 10, 0, None).await.unwrap();
        let predecessor = coordinator
            .queue
            .claim_kind(Utc::now(), 90, ItemOperationKind::Preparation)
            .await
            .unwrap()
            .unwrap();
        let root = temporary.path().join("artifacts");
        let stage = root.join("staging").join(format!(
            "{}-{}-{}",
            predecessor.id, predecessor.generation, predecessor.lease_owner
        ));
        tokio::fs::create_dir_all(&stage).await.unwrap();
        let partial = stage.join("download.part");
        tokio::fs::write(&partial, b"unfinished predecessor bytes")
            .await
            .unwrap();
        let now = encode_datetime(Utc::now());
        // Seed an in-flight reservation directly: no provider or disk-capacity
        // assumptions are needed to model a worker lost across host restart.
        sqlx::query(
            r#"INSERT INTO "AddonMediaListArtifact" (
            "AddonMediaListId","AddonMediaListItemId","CacheKey","Kind","State",
            "ReservedBytes","PreparationJobId","PreparationGeneration",
            "PreparationLeaseOwner","CreatedAt","UpdatedAt") VALUES (1,10,?,1,1,4096,?,?,?,?,?)"#,
        )
        .bind(&predecessor.input.context_fingerprint)
        .bind(predecessor.id)
        .bind(predecessor.generation)
        .bind(&predecessor.lease_owner)
        .bind(&now)
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"UPDATE "AddonMediaListItemOperationJob" SET "LeaseExpiresAt"=? WHERE "Id"=?"#,
        )
        .bind(encode_datetime(Utc::now() - Duration::minutes(1)))
        .bind(job_id)
        .execute(&pool)
        .await
        .unwrap();
        let claimed_id = if other_item {
            coordinator.queue.cancel(job_id, Utc::now()).await.unwrap();
            sqlx::query(r#"INSERT INTO "AddonMediaListItem" (
                "Id","AddonMediaListId","ProviderId","DisplayTitle","Kind","Rank","Title","SourceUrl","ProviderLiveness")
                VALUES (11,1,'other','Other',4,1,'Other','https://example.test/other',1)"#)
                .execute(&pool).await.unwrap();
            coordinator.queue_preparation(1, 11, 0, None).await.unwrap()
        } else {
            job_id
        };
        let invocation_log = temporary.path().join("preparation-invocations");
        let invocations_before = tokio::fs::read(&invocation_log).await.unwrap();

        assert!(coordinator.process_one_preparation(&root).await.unwrap());

        let status = coordinator
            .queue
            .status(claimed_id, Utc::now())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(status.state, 3);
        assert_eq!(
            status.completion,
            Some(ItemOperationCompletion::Failed(
                ItemOperationFailure::ProcessRecoveryRequired
            ))
        );
        assert_eq!(
            tokio::fs::read(&invocation_log).await.unwrap(),
            invocations_before
        );
        assert_eq!(
            tokio::fs::read(&partial).await.unwrap(),
            b"unfinished predecessor bytes"
        );
        let retained: (i32, i64, i64, String) = sqlx::query_as(
            r#"SELECT "State","ReservedBytes","PreparationGeneration","PreparationLeaseOwner"
               FROM "AddonMediaListArtifact" WHERE "PreparationJobId"=?"#,
        )
        .bind(job_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            retained,
            (1, 4096, predecessor.generation, predecessor.lease_owner)
        );
        if !other_item {
            let generation: i64 = sqlx::query_scalar(
                r#"SELECT "Generation" FROM "AddonMediaListItemOperationJob" WHERE "Id"=?"#,
            )
            .bind(job_id)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(generation, predecessor.generation + 1);
        }
    }

    #[tokio::test]
    async fn preparation_is_owned_by_saved_finite_context() {
        let temporary = tempfile::tempdir().unwrap();
        let pool = fixture(&temporary.path().join("package")).await;
        let coordinator = AddonMediaListItemCoordinator::new(pool.clone());
        assert!(coordinator.queue_preparation(1, 10, 1, None).await.is_err());
        let id = coordinator
            .queue_preparation(1, 10, 0, Some(720))
            .await
            .unwrap();
        assert_eq!(
            coordinator
                .preparation_status(1, 10)
                .await
                .unwrap()
                .unwrap()
                .state,
            0
        );
        assert!(coordinator
            .preparation_status(2, 10)
            .await
            .unwrap()
            .is_none());
        assert!(coordinator.cancel_preparation(2, 10, id).await.is_err());
        sqlx::query(r#"UPDATE "AddonMediaListItem" SET "ProviderLiveness"=2 WHERE "Id"=10"#)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            coordinator
                .preparation_status(1, 10)
                .await
                .unwrap()
                .unwrap()
                .state,
            5
        );
        assert!(coordinator.queue_preparation(1, 10, 0, None).await.is_err());
        coordinator.cancel_preparation(1, 10, id).await.unwrap();
    }

    #[tokio::test]
    async fn preparation_cooldown_does_not_consume_attempt_or_reserve_storage() {
        let temporary = tempfile::tempdir().unwrap();
        let pool = fixture(&temporary.path().join("package")).await;
        let coordinator = AddonMediaListItemCoordinator::new(pool.clone());
        coordinator.queue_preparation(1, 10, 0, None).await.unwrap();
        let blocked_until = Utc::now() + Duration::minutes(5);
        let now = encode_datetime(Utc::now());
        sqlx::query(r#"INSERT INTO "AddonProviderHostCooldown" ("ProviderHost","NextEligibleAt","SampledDelaySeconds","LastAttemptAt","UpdatedAt")
            VALUES ('example.test',?,5,?,?)"#)
            .bind(encode_datetime(blocked_until)).bind(&now).bind(&now)
            .execute(&pool).await.unwrap();
        assert!(coordinator
            .process_one_preparation(&temporary.path().join("artifacts"))
            .await
            .unwrap());
        let status = coordinator
            .preparation_status(1, 10)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(status.attempt_count, 0);
        assert_eq!(
            status.next_eligible_at.map(encode_datetime),
            Some(encode_datetime(blocked_until))
        );
        let artifacts: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaListArtifact""#)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(artifacts, 0);
    }
}
