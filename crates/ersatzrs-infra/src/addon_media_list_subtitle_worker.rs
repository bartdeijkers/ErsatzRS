//! Bounded caption transfer using the existing supervised file worker.

use std::path::Path;

use chrono::{Duration, Utc};
use ersatzrs_core::repositories::Result;
use tokio::sync::oneshot;

use crate::{
    addon_media_list_import::{load_detail_wait_range, sample_provider_delay_seconds},
    addon_media_list_item_coordinator::AddonMediaListItemCoordinator,
    addon_media_list_item_jobs::{
        ClaimedItemOperation, ItemOperationCompletion, ItemOperationKind,
    },
    addon_media_list_item_runtime::{InstalledItemRuntime, ItemRuntimeError},
    addon_media_list_refresh::release_provider_host_lease,
    addon_media_list_subtitle_artifacts::SubtitleArtifactStore,
    repositories::{common::encode_datetime, SqlxResultExt as _},
};

impl AddonMediaListItemCoordinator {
    /// Acquire one item's manual captions, never its video. Shutdown waits for
    /// child-tree cleanup before scratch or provider ownership is released.
    ///
    /// # Errors
    /// Returns redacted storage/ownership errors or an unconfirmed cleanup error.
    #[allow(clippy::too_many_lines)] // Keep reservation and provider lifetime ownership together.
    pub(crate) async fn process_one_subtitles_supervised(
        &self,
        root: &Path,
        mut shutdown: oneshot::Receiver<()>,
        session: &crate::addon_preparation_worker::WorkerSession,
    ) -> Result<bool> {
        if crate::addon_media_list_preparation::provider_process_recovery_required() {
            return Err(ersatzrs_core::repositories::RepositoryError::Conflict(
                "provider process cleanup requires recovery".into(),
            ));
        }
        let Some(job) = self
            .queue
            .claim_kind(Utc::now(), 90, ItemOperationKind::Subtitles)
            .await?
        else {
            return Ok(false);
        };
        session.begin(&job).await?;
        let result = self
            .process_claimed_subtitles(root, &job, &mut shutdown)
            .await;
        session
            .finish(
                root,
                &job,
                !crate::addon_media_list_preparation::provider_process_recovery_required(),
            )
            .await?;
        result
    }

    #[allow(clippy::too_many_lines)] // One provider admission owns its scratch and cancellation.
    async fn process_claimed_subtitles(
        &self,
        root: &Path,
        job: &ClaimedItemOperation,
        shutdown: &mut oneshot::Receiver<()>,
    ) -> Result<bool> {
        let runtime = match InstalledItemRuntime::load(
            &self.pool,
            job.input.addon_id()?,
            Some(&job.input.package_identity),
        )
        .await
        {
            Ok(runtime) => runtime,
            Err(error) => return self.finish_interval_error(job, error).await,
        };
        let (cancel, cancellation) = oneshot::channel();
        match self
            .run_interval_stage(
                job,
                None,
                shutdown,
                cancel,
                runtime.runtime_info_cancellable(cancellation),
            )
            .await
        {
            Ok(Some(info)) if info.tool_version == job.input.tool_version => {}
            Ok(Some(_)) => {
                return self
                    .finish_interval_error(job, ItemRuntimeError::PackageChanged)
                    .await
            }
            Ok(None) => return self.defer_interval(job, 1).await,
            Err(error) => return self.finish_interval_error(job, error).await,
        }
        let host = url::Url::parse(&job.input.source_url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
            .ok_or_else(|| {
                ersatzrs_core::repositories::RepositoryError::Conflict(
                    "invalid subtitle source".into(),
                )
            })?;
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
        let store = SubtitleArtifactStore::new(self.pool.clone());
        let transfer = async {
            let reservation = store.reserve(job, root, Utc::now()).await?;
            let request = match reservation.subtitle_request(job) {
                Ok(request) => request,
                Err(error) => {
                    store.release_after_cleanup(job, &reservation).await?;
                    return Err(error);
                }
            };
            let (cancel, cancellation) = oneshot::channel();
            let operation = runtime.subtitles_cancellable(&request, cancellation);
            tokio::pin!(operation);
            let monitor = async {
                let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
                loop {
                    tick.tick().await;
                    self.heartbeat_provider(job, &host).await?;
                    store.check_staging(&reservation).await?;
                }
                #[allow(unreachable_code)]
                Ok::<(), ersatzrs_core::repositories::RepositoryError>(())
            };
            let result = tokio::select! {
                biased;
                _ = &mut *shutdown => {
                    let _ = cancel.send(());
                    match operation.await {
                        Err(ItemRuntimeError::TerminationUnconfirmed) => Err(ItemRuntimeError::TerminationUnconfirmed),
                        _ => Ok(None),
                    }
                }
                result = &mut operation => result.map(Some),
                _ = monitor => {
                    let _ = cancel.send(());
                    match operation.await {
                        Err(ItemRuntimeError::TerminationUnconfirmed) => Err(ItemRuntimeError::TerminationUnconfirmed),
                        _ => Err(ItemRuntimeError::Failed),
                    }
                }
            };
            if matches!(result, Err(ItemRuntimeError::TerminationUnconfirmed)) {
                // A still-running writer retains its charged reservation.
                return Ok(Err(ItemRuntimeError::TerminationUnconfirmed));
            }
            let promoted = match result {
                Ok(Some(result)) => store.promote(job, &reservation, &result.subtitles, Utc::now()).await.map(|ids| Ok(Some(ids))),
                Ok(None) => Ok(Ok(None)),
                Err(error) => Ok(Err(error)),
            };
            store.release_after_cleanup(job, &reservation).await?;
            promoted
        }.await;
        if matches!(transfer, Ok(Err(ItemRuntimeError::TerminationUnconfirmed))) {
            return self
                .finish_interval_error(job, ItemRuntimeError::TerminationUnconfirmed)
                .await;
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
        match transfer? {
            Ok(Some(artifact_ids)) => {
                self.queue
                    .complete(
                        job,
                        ItemOperationCompletion::Subtitles { artifact_ids },
                        now,
                    )
                    .await?;
            }
            Ok(None) => {
                self.defer_interval(job, 1).await?;
            }
            Err(error) => return self.finish_interval_error(job, error).await,
        }
        Ok(true)
    }
}

// The shared installed fixture executes a POSIX shell, intentionally Unix-only.
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::addon_media_list_item_jobs::{AddonMediaListItemJobQueue, ItemOperationInput};
    use sha2::{Digest as _, Sha256};
    use sqlx::Row as _;

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn supervised_subtitles_promote_manual_sidecar_and_restart_without_video() {
        let directory = tempfile::tempdir().unwrap();
        let subtitle = "1\n00:00:00,000 --> 00:00:01,000\nManual fixture\n";
        let digest = Sha256::digest(subtitle.as_bytes()).iter().fold(
            String::with_capacity(64),
            |mut output, byte| {
                use std::fmt::Write as _;
                write!(output, "{byte:02x}").expect("writing to a string");
                output
            },
        );
        let script = r#"#!/bin/sh
request=$(cat)
printf '%s\n' "$1" >> ../subtitle-invocations
case "$1" in
runtime-info) printf '%s\n' '{"tool_version":"1.0"}' ;;
subtitles)
staging=$(printf '%s' "$request" | sed -n 's/.*"staging_directory":"\([^"]*\)".*/\1/p')
fingerprint=$(printf '%s' "$request" | sed -n 's/.*"context_fingerprint":"\([^"]*\)".*/\1/p')
printf '1\n00:00:00,000 --> 00:00:01,000\nManual fixture\n' > "$staging/manual.en.srt"
printf '{"provider_id":"item","context_fingerprint":"%s","tool_version":"1.0","subtitles":[{"language":"en","format":"srt","provenance":"manual","timeline":"source","relative_path":"manual.en.srt","byte_length":LENGTH,"sha256":"DIGEST"}]}\n' "$fingerprint"
;;
*) exit 99 ;;
esac
"#
        .replace("LENGTH", &subtitle.len().to_string())
        .replace("DIGEST", &digest);
        let pool = crate::addon_media_list::tests::fixture_media_list_addon_with_script(
            &directory.path().join("package"),
            &script,
            &["example.test"],
        )
        .await;
        let raw: String =
            sqlx::query_scalar(r#"SELECT "ManifestJson" FROM "AddonInstallation" LIMIT 1"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        let mut manifest: serde_json::Value = serde_json::from_str(&raw).unwrap();
        manifest["manifest_version"] = serde_json::json!(5);
        manifest["capabilities"].as_array_mut().unwrap().extend([
            serde_json::json!({"id":"media-list.item-runtime.v1"}),
            serde_json::json!({"id":"media-list.subtitles.v1"}),
        ]);
        sqlx::query(r#"UPDATE "AddonInstallation" SET "ManifestJson"=?"#)
            .bind(manifest.to_string())
            .execute(&pool)
            .await
            .unwrap();
        sqlx::raw_sql(r#"INSERT INTO "AddonMediaList" ("Id","AddonId","ProviderId","SourceUrl","Name")
            VALUES(1,'org.ersatzrs.addon.fixture','list','https://example.test/list','Fixture');
            INSERT INTO "AddonMediaListItem" ("Id","AddonMediaListId","ProviderId","DisplayTitle","Kind","Rank","Title","SourceUrl","ProviderLiveness")
            VALUES(10,1,'item','Fixture',4,0,'Fixture','https://example.test/item',1);"#)
            .execute(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO "ConfigElement" ("Key","Value") VALUES(?,'eng') ON CONFLICT("Key") DO UPDATE SET "Value"=excluded."Value";"#)
            .bind(ersatzrs_domain::CONFIG_KEY_FFMPEG_PREFERRED_SUBTITLE_LANGUAGE_CODE)
            .execute(&pool).await.unwrap();
        let runtime = InstalledItemRuntime::load(&pool, "org.ersatzrs.addon.fixture", None)
            .await
            .unwrap();
        let mut input = ItemOperationInput {
            source_url: "https://example.test/item".into(),
            provider_id: "item".into(),
            context_fingerprint: String::new(),
            schema: None,
            definition: None,
            options: None,
            addon_id: Some("org.ersatzrs.addon.fixture".into()),
            preparation_v2: None,
            package_identity: runtime.package_identity().into(),
            tool_version: "1.0".into(),
            item_is_finite: true,
            max_video_height: None,
            requested_timing: None,
            requested_subtitle_languages: vec!["en".into()],
        };
        input.context_fingerprint = input.base_fingerprint().unwrap();
        let fingerprint = input.context_fingerprint.clone();
        let id = AddonMediaListItemJobQueue::new(pool.clone())
            .enqueue(1, 10, 0, ItemOperationKind::Subtitles, input, Utc::now())
            .await
            .unwrap();
        let root = directory.path().join("artifacts");
        let (stop, shutdown) = oneshot::channel();
        let worker_pool = pool.clone();
        let worker_root = root.clone();
        let worker = tokio::spawn(async move {
            crate::addon_preparation_worker::run_preparation_worker(
                worker_pool,
                &worker_root,
                shutdown,
            )
            .await
        });
        let completed = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let state: i64 = sqlx::query_scalar(
                    r#"SELECT "State" FROM "AddonMediaListItemOperationJob" WHERE "Id"=?"#,
                )
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
                if state == 2 {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await;
        // Always stop and join the owner before assertions, including a failed poll.
        let _ = stop.send(());
        worker.await.unwrap().unwrap();
        completed.unwrap();
        let outcome: String = sqlx::query_scalar(
            r#"SELECT "OutcomeJson" FROM "AddonMediaListItemOperationJob" WHERE "Id"=?"#,
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let ItemOperationCompletion::Subtitles { artifact_ids } =
            serde_json::from_str(&outcome).unwrap()
        else {
            panic!("wrong completion kind")
        };
        assert_eq!(artifact_ids.len(), 1);
        let row = sqlx::query(r#"SELECT "Kind","State","RelativePath","Sha256","SubtitleLanguage","SubtitleTimeline","OperationContextFingerprint" FROM "AddonMediaListArtifact" WHERE "Id"=?"#)
            .bind(artifact_ids[0]).fetch_one(&pool).await.unwrap();
        assert_eq!(row.get::<i64, _>("Kind"), 0);
        assert_eq!(row.get::<i64, _>("State"), 2);
        assert_eq!(row.get::<String, _>("SubtitleLanguage"), "en");
        assert_eq!(row.get::<i64, _>("SubtitleTimeline"), 0);
        assert_eq!(
            row.get::<String, _>("OperationContextFingerprint"),
            fingerprint
        );
        assert_eq!(row.get::<String, _>("Sha256"), digest);
        assert_eq!(
            tokio::fs::read_to_string(root.join(row.get::<String, _>("RelativePath")))
                .await
                .unwrap(),
            subtitle
        );
        let (stop, shutdown) = oneshot::channel();
        let worker_pool = pool.clone();
        let worker = tokio::spawn(async move {
            crate::addon_preparation_worker::run_preparation_worker(worker_pool, &root, shutdown)
                .await
        });
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;
        let _ = stop.send(());
        worker.await.unwrap().unwrap();
        assert_eq!(
            tokio::fs::read_to_string(directory.path().join("subtitle-invocations"))
                .await
                .unwrap(),
            "runtime-info\nsubtitles\n"
        );
        let checkpoint: String = sqlx::query_scalar(
            r#"SELECT "Value" FROM "ConfigElement" WHERE "Key"='addon.preparation.worker'"#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&checkpoint).unwrap()["phase"],
            "Idle"
        );
        let pending: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaListArtifact" WHERE "State" IN (0,1) OR "ReservedBytes"<>0"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(pending, 0);
    }
}
