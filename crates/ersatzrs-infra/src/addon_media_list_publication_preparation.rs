//! Admit saved-draft interval metadata before publication owns the managed library.

use chrono::Utc;
use ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_INTERVAL_METADATA_V1;
use ersatzrs_core::repositories::{RepositoryError, Result};
use sqlx::Row as _;

use crate::{
    addon_media_list_item_jobs::{
        AddonMediaListItemJobQueue, ItemOperationInput, ItemOperationKind, ItemOperationTiming,
    },
    addon_media_list_item_runtime::InstalledItemRuntime,
    addon_media_list_options::load_draft_item_options,
    addon_media_list_prepared_selection::interval_metadata_requirements,
    db::DbPool,
    repositories::SqlxResultExt as _,
};

pub(crate) async fn queue_required_media(
    pool: &DbPool,
    list_id: i64,
    saved_revision: i32,
    excluded_item_ids: &[i64],
) -> Result<u64> {
    queue_required_media_impl(pool, list_id, saved_revision, excluded_item_ids, false).await
}

/// Automatic refresh never reopens a terminal attempt for the same saved input.
pub(crate) async fn queue_required_automatic_media(
    pool: &DbPool,
    list_id: i64,
    saved_revision: i32,
) -> Result<u64> {
    // Automatic eligibility excludes removals and restricted items before admission.
    queue_required_media_impl(pool, list_id, saved_revision, &[], true).await
}

#[allow(clippy::too_many_lines)] // Keep metadata admission, identity capture and its one local probe together.
async fn queue_required_media_impl(
    pool: &DbPool,
    list_id: i64,
    saved_revision: i32,
    excluded_item_ids: &[i64],
    automatic: bool,
) -> Result<u64> {
    let requirements =
        interval_metadata_requirements(pool, list_id, saved_revision, excluded_item_ids).await?;
    let missing = requirements
        .into_iter()
        .filter(|item| item.plan.is_none())
        .collect::<Vec<_>>();
    let count = u64::try_from(missing.len()).map_err(|_| unavailable())?;
    if missing.iter().all(|item| item.pending) {
        // A repeated click never resets an active attempt or its persisted cooldown.
        return Ok(count);
    }
    let addon_id: String = sqlx::query_scalar(
        r#"SELECT "AddonId" FROM "AddonMediaList" WHERE "Id"=? AND "DraftRevision"=? AND "ReviewState"<>3"#,
    ).bind(list_id).bind(saved_revision).fetch_optional(pool).await.repository()?
        .ok_or_else(unavailable)?;
    let installed = crate::addons::load_installed_addon(pool, &addon_id)
        .await
        .map_err(|_| unavailable())?
        .ok_or_else(unavailable)?;
    if !installed
        .manifest
        .has_capability(CAPABILITY_MEDIA_LIST_INTERVAL_METADATA_V1)
    {
        return Err(unavailable());
    }
    let runtime = InstalledItemRuntime::load(pool, &addon_id, None)
        .await
        .map_err(|_| unavailable())?;
    // One bounded local probe per list, not one child process per candidate.
    // Provider metadata retrieval is owned by the ordinary durable host worker.
    let tool_version = runtime
        .runtime_info()
        .await
        .map_err(|_| unavailable())?
        .tool_version;
    let queue = AddonMediaListItemJobQueue::new(pool.clone());
    for item in missing.into_iter().filter(|item| !item.pending) {
        let row = sqlx::query(
            r#"SELECT "ProviderId", "SourceUrl", "DurationSeconds",
                      "FragmentStartSeconds", "FragmentEndSeconds" FROM "AddonMediaListItem"
               WHERE "Id"=? AND "AddonMediaListId"=? AND "Included"=1
                 AND COALESCE("LivenessOverride", "ProviderLiveness")=1"#,
        )
        .bind(item.item_id)
        .bind(list_id)
        .fetch_optional(pool)
        .await
        .repository()?
        .ok_or_else(unavailable)?;
        let options = load_draft_item_options(pool, list_id, item.item_id)
            .await?
            .ok_or_else(unavailable)?;
        runtime
            .validate_options(&options)
            .map_err(|_| unavailable())?;
        let source_url: String = row
            .try_get::<Option<String>, _>("SourceUrl")
            .repository()?
            .ok_or_else(unavailable)?;
        let provider_id: String = row.try_get("ProviderId").repository()?;
        let package_identity = runtime.package_identity().to_owned();
        let timing = ItemOperationTiming {
            item_duration_seconds: saved_seconds(&row, "DurationSeconds")?,
            fragment_start_seconds: saved_seconds(&row, "FragmentStartSeconds")?,
            fragment_end_seconds: saved_seconds(&row, "FragmentEndSeconds")?,
        };
        let base_fingerprint = options.context_fingerprint(
            &source_url,
            &provider_id,
            &package_identity,
            &tool_version,
            None,
        )?;
        let context_fingerprint = timing.bind_context_fingerprint(&base_fingerprint)?;
        let input = ItemOperationInput {
            source_url,
            provider_id,
            context_fingerprint,
            schema: options.schema,
            definition: options.definition,
            options: options.values,
            package_identity,
            tool_version: tool_version.clone(),
            item_is_finite: true,
            max_video_height: None,
            requested_timing: Some(timing),
        };
        let id = if automatic {
            queue
                .enqueue_once(
                    list_id,
                    item.item_id,
                    saved_revision,
                    ItemOperationKind::IntervalMetadata,
                    input,
                    Utc::now(),
                )
                .await?
        } else {
            queue
                .enqueue(
                    list_id,
                    item.item_id,
                    saved_revision,
                    ItemOperationKind::IntervalMetadata,
                    input,
                    Utc::now(),
                )
                .await?
        };
        if automatic
            && queue
                .status(id, Utc::now())
                .await?
                .is_none_or(|job| job.state >= 3)
        {
            return Err(unavailable());
        }
    }
    Ok(count)
}

fn saved_seconds(row: &crate::db::DbRow, column: &str) -> Result<Option<u64>> {
    row.try_get::<Option<i64>, _>(column)
        .repository()?
        .map(u64::try_from)
        .transpose()
        .map_err(|_| unavailable())
}

fn unavailable() -> RepositoryError {
    RepositoryError::unavailable(
        "prepare add-on publication",
        "the saved streaming interval metadata is unavailable",
    )
}

#[cfg(all(test, unix))]
pub(crate) mod tests {
    use super::*;
    use ersatzrs_core::repositories::AddonMediaListRepository as _;

    pub(crate) async fn streaming_fixture(root: &std::path::Path) -> DbPool {
        let pool = crate::addon_media_list_preparation::tests::fixture(root).await;
        let original: String =
            sqlx::query_scalar(r#"SELECT "ManifestJson" FROM "AddonInstallation" LIMIT 1"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        let mut manifest: serde_json::Value = serde_json::from_str(&original).unwrap();
        // Both capabilities are present: normal publication must choose metadata,
        // not infer a full download from provider support for the alternative.
        manifest["capabilities"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"id": CAPABILITY_MEDIA_LIST_INTERVAL_METADATA_V1}));
        sqlx::query(r#"UPDATE "AddonInstallation" SET "ManifestJson"=?"#)
            .bind(manifest.to_string())
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    #[tokio::test]
    async fn confirmed_removal_is_excluded_from_both_preparation_admissions() {
        let temporary = tempfile::tempdir().unwrap();
        let pool =
            crate::addon_media_list_preparation::tests::fixture(&temporary.path().join("package"))
                .await;
        sqlx::query(r#"UPDATE "AddonMediaListItem" SET "EffectiveLiveness"=1,"DurationSeconds"=NULL WHERE "Id"=10"#)
            .execute(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO "AddonMediaListItemTombstone"
            ("AddonMediaListItemId","MissingSince","LastObservedAt","ObservationCount","LastReconcileGeneration","ConfirmedAt")
            VALUES (10,'2026-09-05 00:00:00','2026-09-05 00:00:00',2,1,'2026-09-05 00:00:00')"#)
            .execute(&pool).await.unwrap();
        let repository = crate::repositories::SqliteAddonMediaListRepository(pool.clone());
        assert_eq!(repository.prepare_publication_media(1, 0).await.unwrap(), 0);
        assert_eq!(repository.prepare_publication_details(1).await.unwrap(), 0);
        sqlx::query(r#"UPDATE "AddonMediaList" SET "DraftOptionValuesJson"='{"enabled":false}' WHERE "Id"=1"#)
            .execute(&pool).await.unwrap();
        assert_eq!(repository.prepare_publication_details(1).await.unwrap(), 0);
        let jobs: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaListItemOperationJob""#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(jobs, 0);
    }

    #[tokio::test]
    async fn usable_local_replacement_needs_neither_provider_duration_nor_preparation() {
        let temporary = tempfile::tempdir().unwrap();
        let pool =
            crate::addon_media_list_preparation::tests::fixture(&temporary.path().join("package"))
                .await;
        sqlx::raw_sql(r#"INSERT INTO "MediaSource" ("Id") VALUES (900);
            INSERT INTO "LocalMediaSource" ("Id") VALUES (900);
            INSERT INTO "Library" ("Id","Name","MediaKind","MediaSourceId") VALUES (900,'Local',1,900);
            INSERT INTO "LibraryPath" ("Id","LibraryId","Path") VALUES (900,900,'fixture-local');
            INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES (900,900,0);
            INSERT INTO "Movie" ("Id") VALUES (900);
            INSERT INTO "MediaVersion" ("Id","MovieId","Duration","DateAdded","DateUpdated","Height","Width","VideoScanKind")
                VALUES (901,900,'00:01:00','2026-09-05 00:00:00','2026-09-05 00:00:00',1080,1920,0);
            INSERT INTO "MediaFile" ("MediaVersionId","Path","PathHash") VALUES (901,'fixture-local/replacement.mkv','fixture');
            INSERT INTO "MediaStream" ("MediaVersionId","Index","MediaStreamKind","Channels","Default","Forced","AttachedPic")
                VALUES (901,0,1,0,1,0,0);
            INSERT INTO "AddonMediaListItemReplacement" ("AddonMediaListItemId","MediaItemId","MediaVersionId","CreatedAt","UpdatedAt")
                VALUES (10,900,901,'2026-09-05 00:00:00','2026-09-05 00:00:00');
            UPDATE "AddonMediaListItem" SET "EffectiveLiveness"=1,"DurationSeconds"=NULL WHERE "Id"=10;"#)
            .execute(&pool).await.unwrap();
        let repository = crate::repositories::SqliteAddonMediaListRepository(pool.clone());
        assert!(repository.list_items(1).await.unwrap()[0].replacement_valid);
        let readiness = repository.publication_readiness(1).await.unwrap();
        assert_eq!(readiness.required_playback_detail_count, 0);
        assert_eq!(readiness.required_prepared_media_count, 0);
        assert!(!readiness.blockers.iter().any(|blocker| matches!(blocker.kind,
            ersatzrs_core::repositories::AddonMediaListPublicationBlockerKind::PlaybackDetailsUnavailable
            | ersatzrs_core::repositories::AddonMediaListPublicationBlockerKind::PlaybackDetailsFailed)));
        assert_eq!(repository.prepare_publication_details(1).await.unwrap(), 0);
        assert_eq!(repository.prepare_publication_media(1, 0).await.unwrap(), 0);
        sqlx::query(r#"UPDATE "MediaItem" SET "State"=1 WHERE "Id"=900"#)
            .execute(&pool)
            .await
            .unwrap();
        assert!(!repository.list_items(1).await.unwrap()[0].replacement_valid);
        assert_eq!(
            repository
                .publication_readiness(1)
                .await
                .unwrap()
                .required_prepared_media_count,
            1
        );
        assert_eq!(repository.prepare_publication_details(1).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn publication_metadata_admits_once_without_details_or_a_publication_lease() {
        let temporary = tempfile::tempdir().unwrap();
        let pool = streaming_fixture(&temporary.path().join("package")).await;
        sqlx::query(r#"INSERT INTO "AddonMediaListItem"
            ("Id","AddonMediaListId","ProviderId","DisplayTitle","Kind","Rank","Title","SourceUrl","ProviderLiveness")
            VALUES (11,1,'second','Second',4,1,'Second','https://example.test/second',1)"#)
            .execute(&pool).await.unwrap();
        sqlx::query(
            r#"UPDATE "AddonMediaListItem" SET "DurationSeconds"=12,
            "FragmentStartSeconds"=2,"FragmentEndSeconds"=14 WHERE "Id"=10"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        let repository = crate::repositories::SqliteAddonMediaListRepository(pool.clone());
        assert_eq!(repository.prepare_publication_details(1).await.unwrap(), 0);
        assert_eq!(repository.prepare_publication_media(1, 0).await.unwrap(), 2);
        let jobs: Vec<(i64, String, i64)> = sqlx::query_as(
            r#"SELECT "Id", "InputJson", "Generation" FROM "AddonMediaListItemOperationJob" ORDER BY "Id""#,
        ).fetch_all(&pool).await.unwrap();
        assert_eq!(jobs.len(), 2);
        for (_, input, _) in &jobs {
            let input: ItemOperationInput = serde_json::from_str(input).unwrap();
            assert_eq!(input.tool_version, "1.0");
            assert!(input.item_is_finite);
            assert!(input.requested_timing.is_some());
        }
        let input: ItemOperationInput = serde_json::from_str(&jobs[0].1).unwrap();
        assert_eq!(
            input.requested_timing,
            Some(ItemOperationTiming {
                item_duration_seconds: Some(12),
                fragment_start_seconds: Some(2),
                fragment_end_seconds: Some(14),
            })
        );
        let kinds: Vec<i32> =
            sqlx::query_scalar(r#"SELECT "Kind" FROM "AddonMediaListItemOperationJob""#)
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(kinds, vec![2, 2]);
        let readiness = repository.publication_readiness(1).await.unwrap();
        assert_eq!(readiness.required_prepared_media_count, 2);
        assert_eq!(readiness.required_prepared_media_pending, 2);
        assert_eq!(readiness.required_playback_detail_count, 0);
        let cooldown = "2099-01-01T00:00:00+00:00";
        sqlx::query(r#"UPDATE "AddonMediaListItemOperationJob" SET "NextEligibleAt"=?"#)
            .bind(cooldown)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(repository.prepare_publication_media(1, 0).await.unwrap(), 2);
        let repeated: Vec<(i64, String, i64)> = sqlx::query_as(
            r#"SELECT "Id", "InputJson", "Generation" FROM "AddonMediaListItemOperationJob" ORDER BY "Id""#,
        ).fetch_all(&pool).await.unwrap();
        assert_eq!(repeated, jobs);
        let cooldowns: Vec<String> =
            sqlx::query_scalar(r#"SELECT "NextEligibleAt" FROM "AddonMediaListItemOperationJob""#)
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(cooldowns, vec![cooldown, cooldown]);
        assert!(repository.prepare_publication_media(1, 1).await.is_err());
        assert_eq!(
            std::fs::read_to_string(temporary.path().join("preparation-invocations")).unwrap(),
            "runtime-info\n"
        );
        let leases: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "ManagedLibraryOperation""#)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(leases, 0);
    }

    #[tokio::test]
    async fn missing_interval_capability_never_admits_a_download_instead() {
        let temporary = tempfile::tempdir().unwrap();
        let pool =
            crate::addon_media_list_preparation::tests::fixture(&temporary.path().join("package"))
                .await;
        let repository = crate::repositories::SqliteAddonMediaListRepository(pool.clone());
        assert!(repository.prepare_publication_media(1, 0).await.is_err());
        assert!(!temporary.path().join("preparation-invocations").exists());
        let jobs: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaListItemOperationJob""#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(jobs, 0);
        let readiness = repository.publication_readiness(1).await.unwrap();
        assert_eq!(readiness.required_prepared_media_count, 1);
        assert_eq!(readiness.required_prepared_media_pending, 0);
    }

    #[tokio::test]
    async fn ready_metadata_supplies_edited_duration_without_a_file_or_detail_job() {
        use crate::addon_media_list_item_jobs::ItemOperationCompletion;
        use ersatzrs_core::addon_media_list_playback::AddonMediaListPlaybackTimelinePlan;

        let temporary = tempfile::tempdir().unwrap();
        let pool = streaming_fixture(&temporary.path().join("package")).await;
        sqlx::query(
            r#"UPDATE "AddonMediaListItem" SET "EffectiveLiveness"=1,
            "DurationSeconds"=NULL WHERE "Id"=10"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        let repository = crate::repositories::SqliteAddonMediaListRepository(pool.clone());
        assert_eq!(repository.prepare_publication_media(1, 0).await.unwrap(), 1);
        let queue = AddonMediaListItemJobQueue::new(pool.clone());
        let job = queue
            .claim_kind(Utc::now(), 90, ItemOperationKind::IntervalMetadata)
            .await
            .unwrap()
            .unwrap();
        let plan = AddonMediaListPlaybackTimelinePlan::new(
            20_000,
            0..20_000,
            &[5_000..7_125, 18_500..20_000],
            job.input.context_fingerprint.clone(),
        )
        .unwrap();
        queue
            .complete(
                &job,
                ItemOperationCompletion::IntervalPlan { plan: plan.clone() },
                Utc::now(),
            )
            .await
            .unwrap();
        let ready = repository.publication_readiness(1).await.unwrap();
        assert_eq!(ready.required_prepared_media_count, 0);
        assert_eq!(ready.required_prepared_media_pending, 0);
        assert_eq!(ready.required_playback_detail_count, 0);
        assert_eq!(repository.prepare_publication_media(1, 0).await.unwrap(), 0);
        assert_eq!(repository.prepare_publication_details(1).await.unwrap(), 0);
        let selected =
            crate::addon_media_list_prepared_selection::select_streaming_plans(&pool, 1, 0, &[])
                .await
                .unwrap();
        assert_eq!(selected.get("item"), Some(&plan));
        assert_eq!(plan.duration_milliseconds(), 16_375);
        let artifacts: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaListArtifact""#)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(artifacts, 0);
        assert_eq!(
            std::fs::read_to_string(temporary.path().join("preparation-invocations")).unwrap(),
            "runtime-info\n"
        );
        // Provider detail changes may arrive without a saved draft edit. They
        // invalidate the timing-bound plan just as explicit chapter edits do.
        sqlx::query(r#"UPDATE "AddonMediaListItem" SET "FragmentStartSeconds"=1 WHERE "Id"=10"#)
            .execute(&pool)
            .await
            .unwrap();
        let stale = repository.publication_readiness(1).await.unwrap();
        assert_eq!(stale.required_prepared_media_count, 1);
        assert_eq!(stale.required_prepared_media_pending, 0);
    }
}
