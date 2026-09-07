//! Metadata-only interval work on the ordinary host coordinator, without file admission.

use chrono::{Duration, Utc};
use ersatzrs_addon_contract::{
    AddonMediaListOptionOperation, MediaListIntervalMetadataOutcome,
    MediaListIntervalMetadataRequest,
};
use ersatzrs_core::{
    addon_media_list_playback::AddonMediaListPlaybackTimelinePlan,
    repositories::{RepositoryError, Result},
};
use ersatzrs_domain::RemoteStreamLiveness;
use tokio::sync::oneshot;

use crate::{
    addon_media_list_import::{load_detail_wait_range, sample_provider_delay_seconds},
    addon_media_list_item_coordinator::AddonMediaListItemCoordinator,
    addon_media_list_item_jobs::{
        ClaimedItemOperation, ItemOperationCompletion, ItemOperationFailure, ItemOperationKind,
    },
    addon_media_list_item_runtime::{InstalledItemRuntime, ItemRuntimeError},
    addon_media_list_refresh::release_provider_host_lease,
    repositories::{common::encode_datetime, SqlxResultExt as _},
};

const LEASE_SECONDS: u32 = 90;
pub(crate) const PROCESS_RECOVERY_KEY: &str = "addon.interval-process-recovery-required";

impl AddonMediaListItemCoordinator {
    /// Run one metadata-only attempt. Signal shutdown and await this future;
    /// both the local version probe and provider operation await process cleanup.
    ///
    /// # Errors
    /// Returns redacted errors for lost ownership, invalid input or unconfirmed cleanup.
    #[allow(clippy::too_many_lines)] // Keep provider admission, child lifetime and lease release together.
    pub async fn process_one_interval_metadata(
        &self,
        mut shutdown: oneshot::Receiver<()>,
    ) -> Result<bool> {
        let Some(job) = self
            .queue
            .claim_kind(
                Utc::now(),
                LEASE_SECONDS,
                ItemOperationKind::IntervalMetadata,
            )
            .await?
        else {
            return Ok(false);
        };
        let runtime = match InstalledItemRuntime::load(
            &self.pool,
            job.input.addon_id()?,
            Some(&job.input.package_identity),
        )
        .await
        {
            Ok(runtime) => runtime,
            Err(error) => return self.finish_interval_error(&job, error).await,
        };
        let resolved = job.input.resolved_options()?.ok_or_else(unavailable)?;
        let request = MediaListIntervalMetadataRequest {
            source_url: job.input.source_url.clone(),
            provider_id: job.input.provider_id.clone(),
            context_fingerprint: job.input.context_fingerprint.clone(),
            tool_version: job.input.tool_version.clone(),
            options: resolved.for_operation(
                AddonMediaListOptionOperation::Preparation,
                RemoteStreamLiveness::Finite,
            )?,
        };
        let (cancel, cancellation) = oneshot::channel();
        let version = self
            .run_interval_stage(
                &job,
                None,
                &mut shutdown,
                cancel,
                runtime.runtime_info_cancellable(cancellation),
            )
            .await;
        match version {
            Ok(Some(info)) if info.tool_version == job.input.tool_version => {}
            Ok(Some(_)) => {
                return self
                    .finish_interval_error(&job, ItemRuntimeError::PackageChanged)
                    .await
            }
            Ok(None) => return self.defer_interval(&job, 1).await,
            Err(error) => return self.finish_interval_error(&job, error).await,
        }
        let host = url::Url::parse(&job.input.source_url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
            .ok_or_else(unavailable)?;
        let delay = sample_provider_delay_seconds(
            uuid::Uuid::new_v4().as_bytes(),
            load_detail_wait_range(&self.pool).await?,
        );
        if let Some(next) = self.admit_provider(&job, &host, delay).await? {
            let now = Utc::now();
            self.queue
                .defer_unstarted(&job, now, next.max(now + Duration::seconds(1)))
                .await?;
            return Ok(true);
        }
        let (cancel, cancellation) = oneshot::channel();
        let result = self
            .run_interval_stage(
                &job,
                Some(&host),
                &mut shutdown,
                cancel,
                runtime.interval_metadata_cancellable(&request, cancellation),
            )
            .await;
        if matches!(result, Err(ItemRuntimeError::TerminationUnconfirmed)) {
            // Keep provider ownership and block new admission until process recovery.
            return self
                .finish_interval_error(&job, ItemRuntimeError::TerminationUnconfirmed)
                .await;
        }
        let retry = match &result {
            Ok(Some(result)) => match result.outcome {
                MediaListIntervalMetadataOutcome::Failed {
                    retry_after_seconds,
                } => retry_after_seconds.unwrap_or(30),
                _ => 0,
            },
            Err(ItemRuntimeError::Failed) => 30,
            _ => 0,
        };
        let now = Utc::now();
        let next = now + Duration::seconds(i64::from(delay.max(retry)));
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        release_provider_host_lease(
            &mut tx,
            &host,
            &job.lease_owner,
            &encode_datetime(now),
            &encode_datetime(next),
            delay,
        )
        .await?;
        tx.commit().await.repository()?;
        let outcome = match result {
            Ok(Some(result)) => result.outcome,
            Ok(None) => return self.defer_interval(&job, delay.max(1)).await,
            Err(error) => return self.finish_interval_error(&job, error).await,
        };
        let completion = match outcome {
            MediaListIntervalMetadataOutcome::Ready {
                source_duration_milliseconds,
                excluded_intervals_milliseconds,
            } => {
                let timing = job
                    .input
                    .requested_timing
                    .as_ref()
                    .ok_or_else(unavailable)?;
                let plan = timing
                    .source_window_milliseconds(source_duration_milliseconds)
                    .and_then(|window| {
                        AddonMediaListPlaybackTimelinePlan::new(
                            source_duration_milliseconds,
                            window,
                            &excluded_intervals_milliseconds,
                            job.input.context_fingerprint.clone(),
                        )
                        .map_err(|_| unavailable())
                    });
                match plan {
                    Ok(plan) => ItemOperationCompletion::IntervalPlan { plan },
                    Err(_) => ItemOperationCompletion::Failed(ItemOperationFailure::Unsupported),
                }
            }
            MediaListIntervalMetadataOutcome::Unsupported => {
                ItemOperationCompletion::Failed(ItemOperationFailure::Unsupported)
            }
            MediaListIntervalMetadataOutcome::Failed {
                retry_after_seconds,
            } => {
                return self
                    .defer_interval(&job, delay.max(retry_after_seconds.unwrap_or(30)))
                    .await
            }
        };
        self.queue.complete(&job, completion, now).await?;
        Ok(true)
    }

    /// Retain the pinned process owner across shutdown/heartbeat selection and await cleanup.
    async fn run_interval_stage<T>(
        &self,
        job: &ClaimedItemOperation,
        host: Option<&str>,
        shutdown: &mut oneshot::Receiver<()>,
        cancel: oneshot::Sender<()>,
        operation: impl std::future::Future<Output = std::result::Result<T, ItemRuntimeError>>,
    ) -> std::result::Result<Option<T>, ItemRuntimeError> {
        tokio::pin!(operation);
        let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(10));
        loop {
            // Keep database waits inside a selectable future, so a busy writer
            // cannot prevent shutdown from reaching the running provider.
            let renew = async {
                heartbeat.tick().await;
                match host {
                    Some(host) => self.heartbeat_provider(job, host).await,
                    None => self.queue.heartbeat(job, Utc::now(), LEASE_SECONDS).await,
                }
            };
            let interrupted = tokio::select! {
                biased;
                _ = &mut *shutdown => true,
                result = &mut operation => return result.map(Some),
                result = renew => result.is_err(),
            };
            if interrupted {
                let _ = cancel.send(());
                return match operation.await {
                    Err(ItemRuntimeError::TerminationUnconfirmed) => {
                        Err(ItemRuntimeError::TerminationUnconfirmed)
                    }
                    _ => Ok(None),
                };
            }
        }
    }

    async fn defer_interval(&self, job: &ClaimedItemOperation, seconds: u32) -> Result<bool> {
        let now = Utc::now();
        self.queue
            .defer(job, now, now + Duration::seconds(i64::from(seconds)))
            .await?;
        Ok(true)
    }

    async fn finish_interval_error(
        &self,
        job: &ClaimedItemOperation,
        error: ItemRuntimeError,
    ) -> Result<bool> {
        if error == ItemRuntimeError::TerminationUnconfirmed {
            crate::addon_media_list_preparation::mark_provider_process_recovery_required();
            // Process-local quarantine also covers a failed database write. The
            // durable marker survives restart and list/job deletion; a timeout
            // or lease expiry is not evidence that the child tree has exited.
            sqlx::query(
                r#"INSERT INTO "ConfigElement" ("Key","Value") VALUES (?,'required')
                ON CONFLICT("Key") DO UPDATE SET "Value"='required'"#,
            )
            .bind(PROCESS_RECOVERY_KEY)
            .execute(&self.pool)
            .await
            .repository()?;
            self.queue
                .complete(
                    job,
                    ItemOperationCompletion::Failed(ItemOperationFailure::ProcessRecoveryRequired),
                    Utc::now(),
                )
                .await?;
            return Err(unavailable());
        }
        if error == ItemRuntimeError::Failed {
            return self.defer_interval(job, 30).await;
        }
        let failure = match error {
            ItemRuntimeError::Unsupported => ItemOperationFailure::Unsupported,
            ItemRuntimeError::PackageChanged => ItemOperationFailure::InputChanged,
            _ => ItemOperationFailure::Failed,
        };
        self.queue
            .complete(job, ItemOperationCompletion::Failed(failure), Utc::now())
            .await?;
        Ok(true)
    }
}

fn unavailable() -> RepositoryError {
    RepositoryError::unavailable("interval metadata", "the interval operation is unavailable")
}

// Synthetic shell packages exercise the shared Unix runner. Windows Job Object
// execution remains a native-platform acceptance gate, not inferred from these tests.
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::addon_media_list_item_jobs::{ItemOperationInput, ItemOperationTiming};
    use ersatzrs_core::repositories::{
        AddonMediaListLifecycleRepository as _, AddonMediaListOptionSchema,
    };
    use sha2::{Digest as _, Sha256};
    use std::fmt::Write as _;

    const READY: &str = r#"#!/bin/sh
set -eu
request=$(cat)
case "$1" in
 runtime-info) printf '%s\n' '{"tool_version":"1.0"}' ;;
 interval-metadata)
  fingerprint=$(printf '%s' "$request" | sed -n 's/.*"context_fingerprint":"\([a-f0-9]*\)".*/\1/p')
  printf '{"provider_id":"item","context_fingerprint":"%s","tool_version":"1.0","outcome":{"kind":"ready","source_duration_milliseconds":20000,"excluded_intervals_milliseconds":[{"start":5000,"end":7125},{"start":18500,"end":20000}]}}\n' "$fingerprint" ;;
 *) exit 1 ;;
esac
"#;

    #[allow(clippy::too_many_lines)] // Keep one synthetic installation and its frozen job setup together.
    async fn fixture(
        script: &str,
        capable: bool,
    ) -> (tempfile::TempDir, AddonMediaListItemCoordinator, i64) {
        let temporary = tempfile::tempdir().unwrap();
        let pool = crate::addon_media_list::tests::fixture_media_list_addon_with_script(
            &temporary.path().join("package"),
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
        let declaration: ersatzrs_addon_contract::AddonCapability = serde_json::from_value(serde_json::json!({
            "id":"media-list.options.v1", "options":[{
                "key":"remove", "label":{"en-US":"Remove"}, "description":{"en-US":"Fixture"},
                "scope":"both", "operation_targets":["preparation"], "applicability":"finite_only",
                "kind":"boolean", "default":true, "requires_preparation":true
            }]
        })).unwrap();
        manifest["capabilities"].as_array_mut().unwrap().extend([
            serde_json::json!({"id":"media-list.item-runtime.v1"}),
            serde_json::to_value(&declaration).unwrap(),
        ]);
        if capable {
            manifest["capabilities"].as_array_mut().unwrap().push(
                serde_json::json!({"id":ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_INTERVAL_METADATA_V1}),
            );
        }
        sqlx::query(r#"UPDATE "AddonInstallation" SET "ManifestJson"=?"#)
            .bind(manifest.to_string())
            .execute(&pool)
            .await
            .unwrap();
        let definition_json = serde_json::to_string(&declaration).unwrap();
        let mut schema_digest = String::with_capacity(64);
        for byte in Sha256::digest(definition_json.as_bytes()) {
            write!(schema_digest, "{byte:02x}").unwrap();
        }
        let schema = ersatzrs_core::repositories::AddonMediaListOptionSchemaRef {
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
        sqlx::query(r#"INSERT INTO "AddonMediaListItem"
            ("Id","AddonMediaListId","ProviderId","DisplayTitle","Kind","Rank","Title","SourceUrl","ProviderLiveness","DurationSeconds")
            VALUES (10,1,'item','Fixture',4,0,'Fixture','https://example.test/item',1,20)"#)
            .execute(&pool).await.unwrap();
        let options = crate::addon_media_list_options::load_draft_item_options(&pool, 1, 10)
            .await
            .unwrap()
            .unwrap();
        let runtime = InstalledItemRuntime::load(&pool, &schema.addon_id, None)
            .await
            .unwrap();
        let timing = ItemOperationTiming {
            item_duration_seconds: Some(20),
            fragment_start_seconds: None,
            fragment_end_seconds: None,
        };
        let base = options
            .context_fingerprint(
                "https://example.test/item",
                "item",
                runtime.package_identity(),
                "1.0",
                None,
            )
            .unwrap();
        let input = ItemOperationInput {
            source_url: "https://example.test/item".into(),
            provider_id: "item".into(),
            context_fingerprint: timing.bind_context_fingerprint(&base).unwrap(),
            schema: Some(options.schema),
            definition: Some(options.definition),
            options: Some(options.values),
            addon_id: None,
            preparation_v2: None,
            package_identity: runtime.package_identity().into(),
            tool_version: "1.0".into(),
            item_is_finite: true,
            max_video_height: None,
            requested_timing: Some(timing),
        };
        let coordinator = AddonMediaListItemCoordinator::new(pool);
        let id = coordinator
            .queue
            .enqueue(
                1,
                10,
                0,
                ItemOperationKind::IntervalMetadata,
                input,
                Utc::now(),
            )
            .await
            .unwrap();
        (temporary, coordinator, id)
    }

    async fn process(coordinator: &AddonMediaListItemCoordinator) {
        let (_cancel, cancellation) = oneshot::channel();
        assert!(coordinator
            .process_one_interval_metadata(cancellation)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn interval_worker_honors_cooldown_and_persists_plan_without_artifacts() {
        let (_temporary, coordinator, id) = fixture(READY, true).await;
        let now = Utc::now();
        sqlx::query(r#"INSERT INTO "AddonProviderHostCooldown" ("ProviderHost","NextEligibleAt","SampledDelaySeconds","LastAttemptAt","UpdatedAt")
            VALUES ('example.test',?,5,?,?)"#)
            .bind(encode_datetime(now + Duration::minutes(5))).bind(encode_datetime(now)).bind(encode_datetime(now))
            .execute(&coordinator.pool).await.unwrap();
        process(&coordinator).await;
        let status = coordinator
            .queue
            .status(id, Utc::now())
            .await
            .unwrap()
            .unwrap();
        assert_eq!((status.state, status.attempt_count), (0, 0));
        let past = encode_datetime(now - Duration::seconds(1));
        sqlx::query(r#"UPDATE "AddonProviderHostCooldown" SET "NextEligibleAt"=?"#)
            .bind(&past)
            .execute(&coordinator.pool)
            .await
            .unwrap();
        sqlx::query(
            r#"UPDATE "AddonMediaListItemOperationJob" SET "NextEligibleAt"=? WHERE "Id"=?"#,
        )
        .bind(past)
        .bind(id)
        .execute(&coordinator.pool)
        .await
        .unwrap();
        process(&coordinator).await;
        let status = coordinator
            .queue
            .status(id, Utc::now())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(status.state, 2);
        let Some(ItemOperationCompletion::IntervalPlan { plan }) = status.completion else {
            panic!("expected interval plan")
        };
        assert_eq!(plan.duration_milliseconds(), 16_375);
        let artifacts: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaListArtifact""#)
            .fetch_one(&coordinator.pool)
            .await
            .unwrap();
        assert_eq!(artifacts, 0);
        let owner: Option<String> = sqlx::query_scalar(r#"SELECT "LeaseOwner" FROM "AddonProviderHostCooldown" WHERE "ProviderHost"='example.test'"#)
            .fetch_one(&coordinator.pool).await.unwrap();
        assert!(owner.is_none());
    }

    #[tokio::test]
    async fn interval_worker_reports_unsupported_without_an_uncut_plan() {
        let (_temporary, coordinator, id) = fixture(READY, false).await;
        process(&coordinator).await;
        let status = coordinator
            .queue
            .status(id, Utc::now())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(status.state, 3);
        assert_eq!(
            status.completion,
            Some(ItemOperationCompletion::Failed(
                ItemOperationFailure::Unsupported
            ))
        );
    }

    #[tokio::test]
    async fn interval_shutdown_awaits_probe_and_provider_exit() {
        for runtime_probe in [true, false] {
            let script = READY.replace(
                if runtime_probe {
                    "runtime-info)"
                } else {
                    "interval-metadata)"
                },
                if runtime_probe {
                    "runtime-info) echo $$ > ../provider-pid; exec sleep 30;"
                } else {
                    "interval-metadata) echo $$ > ../provider-pid; exec sleep 30;"
                },
            );
            let (temporary, coordinator, id) = fixture(&script, true).await;
            let (cancel, cancellation) = oneshot::channel();
            let operation = coordinator.process_one_interval_metadata(cancellation);
            tokio::pin!(operation);
            let pid_path = temporary.path().join("provider-pid");
            let started = async {
                loop {
                    if let Ok(pid) = tokio::fs::read_to_string(&pid_path).await {
                        if !pid.trim().is_empty() {
                            break pid;
                        }
                    }
                    tokio::task::yield_now().await;
                }
            };
            let pid = tokio::select! {
                result = &mut operation => panic!("exited before cancellation: {result:?}"),
                pid = tokio::time::timeout(std::time::Duration::from_secs(10), started) => pid.unwrap(),
            };
            cancel.send(()).unwrap();
            assert!(
                tokio::time::timeout(std::time::Duration::from_secs(8), operation)
                    .await
                    .unwrap()
                    .unwrap()
            );
            let alive = tokio::process::Command::new("kill")
                .args(["-0", pid.trim()])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .await
                .unwrap()
                .success();
            assert!(!alive, "provider must exit before the worker returns");
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
    }

    #[tokio::test]
    async fn interval_recovery_marker_survives_expired_leases_and_deleted_jobs() {
        let (_temporary, coordinator, id) = fixture(READY, true).await;
        let now = Utc::now();
        coordinator
            .queue
            .claim_kind(now, LEASE_SECONDS, ItemOperationKind::IntervalMetadata)
            .await
            .unwrap()
            .unwrap();
        // Model persisted unconfirmed cleanup without setting the process-global
        // flag: a new coordinator must honor storage evidence after restart.
        sqlx::query(r#"INSERT INTO "ConfigElement" ("Key","Value") VALUES (?,'required')"#)
            .bind(PROCESS_RECOVERY_KEY)
            .execute(&coordinator.pool)
            .await
            .unwrap();
        let restarted = AddonMediaListItemCoordinator::new(coordinator.pool.clone());
        assert!(restarted
            .queue
            .claim_kind(
                now + Duration::minutes(5),
                LEASE_SECONDS,
                ItemOperationKind::IntervalMetadata
            )
            .await
            .unwrap()
            .is_none());
        let mut tx = crate::db::begin_write(&restarted.pool).await.unwrap();
        assert!(crate::addon_media_list_refresh::claim_provider_host(
            &mut tx,
            "example.test",
            &encode_datetime(now),
            "another-owner",
            &encode_datetime(now + Duration::minutes(1)),
            1,
        )
        .await
        .is_err());
        tx.rollback().await.unwrap();
        sqlx::query(r#"DELETE FROM "AddonMediaListItemOperationJob" WHERE "Id"=?"#)
            .bind(id)
            .execute(&restarted.pool)
            .await
            .unwrap();
        sqlx::query(r#"DELETE FROM "AddonMediaList" WHERE "Id"=1"#)
            .execute(&restarted.pool)
            .await
            .unwrap();
        assert!(
            crate::addon_preparation_worker::recovery_required(&restarted.pool)
                .await
                .unwrap()
        );
    }
}
