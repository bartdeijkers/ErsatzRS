//! Saved-draft access checks, separate from import and publication workers.

use chrono::{DateTime, Duration, Utc};
use ersatzrs_addon_contract::{AddonMediaListOptionOperation, MediaListAccessCheckRequest};
use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::RemoteStreamLiveness;
use sqlx::Row as _;

use crate::{
    addon_media_list_import::{load_detail_wait_range, sample_provider_delay_seconds},
    addon_media_list_item_jobs::{
        validate_claim, AddonMediaListItemJobQueue, ClaimedItemOperation, ItemOperationCompletion,
        ItemOperationFailure, ItemOperationInput, ItemOperationKind, ItemOperationStatus,
    },
    addon_media_list_item_runtime::{InstalledItemRuntime, ItemRuntimeError},
    addon_media_list_options::load_draft_item_options,
    addon_media_list_refresh::{claim_provider_host, release_provider_host_lease},
    db::DbPool,
    repositories::{
        common::{decode_datetime, decode_enum_column, encode_datetime},
        SqlxResultExt as _,
    },
};

const LEASE_SECONDS: u32 = 90;

#[derive(Clone)]
pub struct AddonMediaListItemCoordinator {
    pub(super) pool: DbPool,
    pub(super) queue: AddonMediaListItemJobQueue,
}

impl AddonMediaListItemCoordinator {
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self {
            queue: AddonMediaListItemJobQueue::new(pool.clone()),
            pool,
        }
    }

    /// Only the bounded local version probe runs here; provider access is queued.
    ///
    /// # Errors
    /// Returns an error for missing or changed draft input, an unavailable runtime,
    /// or a database failure.
    pub async fn queue_access(&self, list_id: i64, item_id: i64, revision: i32) -> Result<i64> {
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
        let source_url: String = row
            .try_get::<Option<String>, _>("SourceUrl")
            .repository()?
            .ok_or_else(changed)?;
        let provider_id: String = row.try_get("ProviderId").repository()?;
        let liveness: RemoteStreamLiveness =
            decode_enum_column("RemoteStreamLiveness", &row, "Liveness")?;
        let options = load_draft_item_options(&self.pool, list_id, item_id)
            .await?
            .ok_or_else(changed)?;
        let runtime = InstalledItemRuntime::load(&self.pool, &options.schema.addon_id, None)
            .await
            .map_err(runtime_error)?;
        runtime.validate_options(&options).map_err(runtime_error)?;
        let tool_version = runtime
            .runtime_info()
            .await
            .map_err(runtime_error)?
            .tool_version;
        let package_identity = runtime.package_identity().to_owned();
        let context_fingerprint = options.context_fingerprint(
            &source_url,
            &provider_id,
            &package_identity,
            &tool_version,
            None,
        )?;
        self.queue
            .enqueue(
                list_id,
                item_id,
                revision,
                ItemOperationKind::Access,
                ItemOperationInput {
                    source_url,
                    provider_id,
                    context_fingerprint,
                    schema: Some(options.schema),
                    definition: Some(options.definition),
                    options: Some(options.values),
                    addon_id: None,
                    preparation_v2: None,
                    package_identity,
                    tool_version,
                    item_is_finite: liveness == RemoteStreamLiveness::Finite,
                    max_video_height: None,
                    requested_timing: None,
                    requested_subtitle_languages: Vec::new(),
                },
                Utc::now(),
            )
            .await
    }

    /// Read only this item's most recent check; old evidence cannot become active again.
    ///
    /// # Errors
    /// Returns an error for invalid persisted state, an unavailable runtime needed
    /// to validate successful evidence, or a database failure.
    pub async fn access_status(
        &self,
        list_id: i64,
        item_id: i64,
    ) -> Result<Option<ItemOperationStatus>> {
        let id: Option<i64> = sqlx::query_scalar(
            r#"SELECT "Id" FROM "AddonMediaListItemOperationJob"
               WHERE "AddonMediaListId"=? AND "AddonMediaListItemId"=? AND "Kind"=0
               ORDER BY "Id" DESC LIMIT 1"#,
        )
        .bind(list_id)
        .bind(item_id)
        .fetch_optional(&self.pool)
        .await
        .repository()?;
        let Some(id) = id else {
            return Ok(None);
        };
        let Some(mut status) = self.queue.status(id, Utc::now()).await? else {
            return Ok(None);
        };
        if status.state == 2
            && matches!(
                status.completion,
                Some(ItemOperationCompletion::Access { .. })
            )
        {
            let addon_id: Option<String> =
                sqlx::query_scalar(r#"SELECT "AddonId" FROM "AddonMediaList" WHERE "Id"=?"#)
                    .bind(list_id)
                    .fetch_optional(&self.pool)
                    .await
                    .repository()?;
            let runtime =
                InstalledItemRuntime::load(&self.pool, &addon_id.ok_or_else(changed)?, None)
                    .await
                    .map_err(runtime_error)?;
            let observed = runtime.runtime_info().await.map_err(runtime_error)?;
            if observed.tool_version != status.tool_version {
                status.state = 5;
                status.completion = None;
            }
        }
        Ok(Some(status))
    }

    /// Cancel an active access check owned by this list item.
    ///
    /// # Errors
    /// Returns an error for a missing, mismatched or inactive job, or a database failure.
    pub async fn cancel_access(&self, list_id: i64, item_id: i64, job_id: i64) -> Result<()> {
        let owns: bool = sqlx::query_scalar(
            r#"SELECT EXISTS(SELECT 1 FROM "AddonMediaListItemOperationJob"
               WHERE "Id"=? AND "AddonMediaListId"=? AND "AddonMediaListItemId"=? AND "Kind"=0)"#,
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

    /// One bounded attempt; cancellation drops the managed child process owner.
    ///
    /// # Errors
    /// Returns an error for invalid persisted context, a lost operation claim,
    /// or a database failure while claiming, deferring or completing work.
    #[allow(clippy::too_many_lines)] // Keep the process lifetime and provider lease release visible together.
    pub async fn process_one_access(&self) -> Result<bool> {
        let Some(job) = self
            .queue
            .claim_kind(Utc::now(), LEASE_SECONDS, ItemOperationKind::Access)
            .await?
        else {
            return Ok(false);
        };
        let Ok(runtime) = InstalledItemRuntime::load(
            &self.pool,
            job.input.addon_id()?,
            Some(&job.input.package_identity),
        )
        .await
        else {
            self.queue
                .complete(
                    &job,
                    ItemOperationCompletion::Failed(ItemOperationFailure::InputChanged),
                    Utc::now(),
                )
                .await?;
            return Ok(true);
        };
        let Ok(observed) = runtime.runtime_info().await else {
            let now = Utc::now();
            self.queue
                .defer(&job, now, now + Duration::seconds(30))
                .await?;
            return Ok(true);
        };
        if observed.tool_version != job.input.tool_version {
            self.queue
                .complete(
                    &job,
                    ItemOperationCompletion::Failed(ItemOperationFailure::InputChanged),
                    Utc::now(),
                )
                .await?;
            return Ok(true);
        }
        let host = url::Url::parse(&job.input.source_url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
            .ok_or_else(changed)?;
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
        let resolved = job.input.resolved_options()?.ok_or_else(changed)?;
        let options = resolved.for_operation(
            AddonMediaListOptionOperation::TestAccess,
            if job.input.item_is_finite {
                RemoteStreamLiveness::Finite
            } else {
                RemoteStreamLiveness::Unknown
            },
        )?;
        let request = MediaListAccessCheckRequest {
            source_url: job.input.source_url.clone(),
            provider_id: job.input.provider_id.clone(),
            context_fingerprint: job.input.context_fingerprint.clone(),
            tool_version: job.input.tool_version.clone(),
            options,
        };
        let result = {
            let operation = runtime.test_access(&request, job.input.item_is_finite);
            tokio::pin!(operation);
            let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(10));
            loop {
                tokio::select! {
                    result = &mut operation => break Some(result),
                    _ = heartbeat.tick() => {
                        if self.heartbeat_provider(&job, &host).await.is_err() {
                            break None;
                        }
                    }
                }
            }
        }; // Drop the managed process tree before releasing its provider lease.
        let now = Utc::now();
        let retry = result
            .as_ref()
            .and_then(|value| value.as_ref().ok())
            .and_then(|value| value.retry_after_seconds)
            .unwrap_or_default();
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
        match result {
            Some(Ok(result)) => {
                self.queue
                    .complete(
                        &job,
                        ItemOperationCompletion::Access {
                            outcome: result.outcome,
                            expires_at: now + Duration::minutes(10),
                        },
                        now,
                    )
                    .await?;
            }
            Some(Err(ItemRuntimeError::Failed)) => {
                self.queue
                    .defer(&job, now, next.max(now + Duration::seconds(30)))
                    .await?;
            }
            Some(Err(_)) => {
                self.queue
                    .complete(
                        &job,
                        ItemOperationCompletion::Failed(ItemOperationFailure::Failed),
                        now,
                    )
                    .await?;
            }
            None => {} // Context/lease cancellation already invalidated this attempt.
        }
        Ok(true)
    }

    pub(super) async fn admit_provider(
        &self,
        job: &ClaimedItemOperation,
        host: &str,
        delay: u32,
    ) -> Result<Option<DateTime<Utc>>> {
        let now = Utc::now();
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        validate_claim(&mut tx, job, now).await?;
        let blocked = claim_provider_host(
            &mut tx,
            host,
            &encode_datetime(now),
            &job.lease_owner,
            &encode_datetime(now + Duration::seconds(i64::from(LEASE_SECONDS))),
            delay,
        )
        .await?;
        tx.commit().await.repository()?;
        blocked
            .map(|value| decode_datetime("item provider eligibility", &value))
            .transpose()
    }

    pub(super) async fn heartbeat_provider(
        &self,
        job: &ClaimedItemOperation,
        host: &str,
    ) -> Result<()> {
        let now = Utc::now();
        let expires = encode_datetime(now + Duration::seconds(i64::from(LEASE_SECONDS)));
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        validate_claim(&mut tx, job, now).await?;
        let renewed = sqlx::query(
            r#"UPDATE "AddonProviderHostCooldown" SET "LeaseExpiresAt"=?, "UpdatedAt"=?
               WHERE "ProviderHost"=? AND "LeaseOwner"=? AND "LeaseExpiresAt">?"#,
        )
        .bind(&expires)
        .bind(encode_datetime(now))
        .bind(host)
        .bind(&job.lease_owner)
        .bind(encode_datetime(now))
        .execute(&mut *tx)
        .await
        .repository()?
        .rows_affected();
        if renewed != 1 {
            return Err(changed());
        }
        sqlx::query(
            r#"UPDATE "AddonMediaListItemOperationJob" SET "LeaseExpiresAt"=?, "HeartbeatAt"=?, "UpdatedAt"=?
               WHERE "Id"=? AND "Generation"=? AND "LeaseOwner"=?"#,
        ).bind(expires).bind(encode_datetime(now)).bind(encode_datetime(now)).bind(job.id)
            .bind(job.generation).bind(&job.lease_owner).execute(&mut *tx).await.repository()?;
        tx.commit().await.repository()?;
        Ok(())
    }
}

fn changed() -> RepositoryError {
    RepositoryError::Conflict("the saved item access context changed".into())
}

fn runtime_error(_: ItemRuntimeError) -> RepositoryError {
    RepositoryError::unavailable("item access", "the item operation is unavailable")
}

// The synthetic shell transport is Unix-only; native Windows process containment
// remains covered by the shared runner's platform tests and integrated N-03 gate.
#[cfg(all(test, unix))]
mod tests {
    use std::fmt::Write as _;

    use super::*;
    use ersatzrs_core::repositories::{
        AddonMediaListLifecycleRepository as _, AddonMediaListOptionSchema,
        AddonMediaListOptionSchemaRef,
    };
    use sha2::{Digest as _, Sha256};

    #[tokio::test]
    #[allow(clippy::too_many_lines)] // Keep the fixture and sequential cooldown/ownership assertions together.
    async fn access_worker_waits_for_shared_cooldown_then_persists_only_owned_evidence() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("package");
        let script = r#"#!/bin/sh
set -eu
request=$(cat)
case "$1" in
  runtime-info) printf '%s\n' '{"tool_version":"1.0"}' ;;
  test-access)
    fingerprint=$(printf '%s' "$request" | sed -n 's/.*"context_fingerprint":"\([a-f0-9]*\)".*/\1/p')
    printf '{"provider_id":"item","context_fingerprint":"%s","tool_version":"1.0","outcome":"accessible"}\n' "$fingerprint" ;;
  *) exit 1 ;;
esac
"#;
        let pool = crate::addon_media_list::tests::fixture_media_list_addon_with_script(
            &root,
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
            "scope":"both", "operation_targets":["test_access"], "applicability":"all", "kind":"boolean", "default":false
        }]});
        manifest["capabilities"].as_array_mut().unwrap().extend([
            serde_json::json!({"id":"media-list.item-runtime.v1"}),
            serde_json::json!({"id":"media-list.access-check.v1"}),
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
        let repository = crate::repositories::SqliteAddonMediaListRepository(pool.clone());
        repository
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
        let coordinator = AddonMediaListItemCoordinator::new(pool.clone());
        let id = coordinator.queue_access(1, 10, 0).await.unwrap();
        assert_eq!(
            coordinator
                .access_status(1, 10)
                .await
                .unwrap()
                .unwrap()
                .state,
            0
        );
        let blocked_until = Utc::now() + Duration::minutes(5);
        sqlx::query(r#"INSERT INTO "AddonProviderHostCooldown" ("ProviderHost","NextEligibleAt","SampledDelaySeconds","LastAttemptAt","UpdatedAt")
            VALUES ('example.test',?,5,?,?)"#)
            .bind(encode_datetime(blocked_until)).bind(encode_datetime(Utc::now())).bind(encode_datetime(Utc::now()))
            .execute(&pool).await.unwrap();
        assert!(coordinator.process_one_access().await.unwrap());
        let waiting = coordinator.access_status(1, 10).await.unwrap().unwrap();
        assert_eq!(waiting.attempt_count, 0);
        assert_eq!(
            waiting.next_eligible_at.map(encode_datetime),
            Some(encode_datetime(blocked_until))
        );
        let past = encode_datetime(Utc::now() - Duration::seconds(1));
        sqlx::query(r#"UPDATE "AddonProviderHostCooldown" SET "NextEligibleAt"=?"#)
            .bind(&past)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            r#"UPDATE "AddonMediaListItemOperationJob" SET "NextEligibleAt"=? WHERE "Id"=?"#,
        )
        .bind(&past)
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
        assert!(coordinator.process_one_access().await.unwrap());
        let complete = coordinator.access_status(1, 10).await.unwrap().unwrap();
        assert_eq!(complete.state, 2);
        assert!(matches!(
            complete.completion,
            Some(ItemOperationCompletion::Access {
                outcome: ersatzrs_addon_contract::MediaListAccessCheckOutcome::Accessible,
                ..
            })
        ));
        assert!(coordinator.access_status(2, 10).await.unwrap().is_none());
        assert!(coordinator.cancel_access(2, 10, id).await.is_err());
        let lease: Option<String> = sqlx::query_scalar(r#"SELECT "LeaseOwner" FROM "AddonProviderHostCooldown" WHERE "ProviderHost"='example.test'"#)
            .fetch_one(&pool).await.unwrap();
        assert!(lease.is_none());
        let before: String = sqlx::query_scalar(
            r#"SELECT "OutcomeJson" FROM "AddonMediaListItemOperationJob" WHERE "Id"=?"#,
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query(r#"UPDATE "AddonMediaListItemOperationJob" SET "InputJson"=json_set("InputJson",'$.tool_version','0.9') WHERE "Id"=?"#)
            .bind(id).execute(&pool).await.unwrap();
        assert_eq!(
            coordinator
                .access_status(1, 10)
                .await
                .unwrap()
                .unwrap()
                .state,
            5
        );
        let after: String = sqlx::query_scalar(
            r#"SELECT "OutcomeJson" FROM "AddonMediaListItemOperationJob" WHERE "Id"=?"#,
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(before, after);
    }
}
