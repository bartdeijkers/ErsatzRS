use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckError, HealthCheckResult,
    HealthCheckStatus, Pool, Result, Row,
};
use ersatzrs_addon_contract::{AddonManifest, CAPABILITY_MEDIA_LIST_SOURCE_CHECK_V1};
use ersatzrs_domain::{
    AddonMediaListPublicationEventKind, AddonMediaListRefreshCadence, AddonMediaListRefreshState,
};

const STUCK_IMPORT_MINUTES: i64 = 15;

pub(crate) struct AddonMediaListHealthCheck {
    pool: Pool<Db>,
}

impl AddonMediaListHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for AddonMediaListHealthCheck {
    fn title(&self) -> &'static str {
        "Add-on media lists"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        if let Some(result) = preparation_recovery_failure(&self.pool, self.title()).await? {
            return Ok(result);
        }
        if crate::addon_preparation_worker::worker_unavailable(&self.pool)
            .await
            .map_err(|_| {
                HealthCheckError::Database(
                    "preparation worker availability could not be read".to_owned(),
                )
            })?
        {
            let message = "Media preparation worker is unavailable. Restart the server before retrying preparation.";
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Fail,
                message,
                message,
            )
            .with_link("/media/addon-lists"));
        }
        let rows = addon_media_list_health_rows(&self.pool).await?;
        let lifecycle_foundation = lifecycle_foundation_health(&self.pool).await?;
        if rows.is_empty() {
            return Ok(lifecycle_foundation.result_without_lists(self.title()));
        }
        let configured = configured_list_health(&self.pool, &rows, lifecycle_foundation).await?;
        if let Some(result) = configured.failure_result(self.title(), lifecycle_foundation) {
            return Ok(result);
        }
        if let Some(result) = automatic_publication_hold_warning(&self.pool, self.title()).await? {
            return Ok(result);
        }
        if let Some(id) = sqlx::query_scalar::<_, i64>(r#"SELECT "Id" FROM "AddonMediaList" WHERE "DerivedMetadataLimited"=1 ORDER BY "Id" LIMIT 1"#)
            .fetch_optional(&self.pool).await.map_err(sqlx_error)? {
            return Ok(HealthCheckResult::new(self.title(),HealthCheckStatus::Warning,
                "Derived show metadata reached its size limit",
                "Some episode facets exceed the bounded show metadata union. Episode metadata remains complete.")
                .with_link(format!("/media/addon-lists/{id}")));
        }
        if let Some(result) = configured.warning_result(self.title(), lifecycle_foundation) {
            return Ok(result);
        }
        configured_list_status_result(
            &self.pool,
            &rows,
            &configured.coordinator_activity,
            lifecycle_foundation,
            self.title(),
        )
        .await
    }
}

async fn preparation_recovery_failure(
    pool: &Pool<Db>,
    title: &'static str,
) -> Result<Option<HealthCheckResult>> {
    // Lease expiry identifies work needing recovery, never proof that its
    // descendants stopped. Do not expose reservation paths or provider input.
    let blocked: bool = crate::addon_preparation_worker::recovery_required(pool)
        .await
        .map_err(|_| {
            HealthCheckError::Database("preparation recovery status could not be read".to_owned())
        })?
        || sqlx::query_scalar(
            r#"SELECT EXISTS(
            SELECT 1 FROM "AddonMediaListArtifact" AS "artifact"
            WHERE "artifact"."Kind" IN (0, 1) AND "artifact"."State" IN (0, 1)
              AND NOT EXISTS (
                SELECT 1 FROM "AddonMediaListItemOperationJob" AS "job"
                WHERE "job"."Id" = "artifact"."PreparationJobId"
                  AND "job"."Generation" = "artifact"."PreparationGeneration"
                  AND "job"."LeaseOwner" = "artifact"."PreparationLeaseOwner"
                  AND (("artifact"."Kind" = 1 AND "job"."Kind" = 1)
                    OR ("artifact"."Kind" = 0 AND "job"."Kind" = 3))
                  AND "job"."State" = 1
                  AND "job"."LeaseExpiresAt" > ?)
        ) OR EXISTS (
            SELECT 1 FROM "AddonMediaListItemOperationJob" AS "job"
            WHERE "job"."Kind" IN (1, 3) AND "job"."State" = 3
              AND json_extract("job"."OutcomeJson", '$.Failed') = 'ProcessRecoveryRequired'
              AND NOT EXISTS (
                SELECT 1 FROM "AddonMediaListItemOperationJob" AS "newer"
                WHERE "newer"."AddonMediaListId" = "job"."AddonMediaListId"
                  AND "newer"."AddonMediaListItemId" = "job"."AddonMediaListItemId"
                  AND "newer"."Kind" = "job"."Kind" AND "newer"."Id" > "job"."Id")
        )"#,
        )
        .bind(crate::repositories::common::encode_datetime(
            chrono::Utc::now(),
        ))
        .fetch_one(pool)
        .await
        .map_err(sqlx_error)?;
    Ok(blocked.then(|| {
        let message = "Media preparation needs process recovery. Confirm prior provider processes have stopped before recovering retained storage and retrying preparation.";
        HealthCheckResult::new(title, HealthCheckStatus::Fail, message, message)
            .with_link("/media/addon-lists")
    }))
}

async fn automatic_publication_hold_warning(
    pool: &Pool<Db>,
    title: &'static str,
) -> Result<Option<HealthCheckResult>> {
    // Match the editor's current hold projection; changing policy or resolving
    // the assessment must clear this warning without exposing provider data.
    let list_id = sqlx::query_scalar::<_, i64>(
        r#"SELECT "list"."Id"
           FROM "AddonMediaListAutoPublication" AS "auto"
           JOIN "AddonMediaList" AS "list" ON "list"."Id" = "auto"."AddonMediaListId"
           WHERE "auto"."State" = 'held' AND "list"."RefreshPublicationPolicy" IN (1, 2)
           ORDER BY "auto"."RecordedAt", "list"."Id" LIMIT 1"#,
    )
    .fetch_optional(pool)
    .await
    .map_err(sqlx_error)?;
    Ok(list_id.map(|id| {
        HealthCheckResult::new(
            title,
            HealthCheckStatus::Warning,
            "Automatic processing needs attention",
            "Automatic processing needs attention",
        )
        .with_link(format!(
            "/media/addon-lists/{id}#addon-list-publication-activity"
        ))
    }))
}

async fn addon_media_list_health_rows(pool: &Pool<Db>) -> Result<Vec<crate::db::DbRow>> {
    sqlx::query(
        r#"SELECT "list"."Id", "list"."Name", "list"."LastError", "list"."PresentationError",
                  "list"."ReviewState", "list"."ReviewError", "list"."PublicationStartedAt",
                  "list"."RefreshCadence",
                  "addon"."Enabled" AS "AddonEnabled",
                  "addon"."ManifestJson" AS "AddonManifestJson",
                  "refresh"."State" AS "RefreshState",
                  "refresh"."DiagnosticCode" AS "RefreshDiagnosticCode",
                  "source"."MediaSourceId", "managed"."LibraryId", "managed"."ManifestPath"
           FROM "AddonMediaList" AS "list"
           LEFT JOIN "AddonInstallation" AS "addon"
             ON "addon"."AddonId" = "list"."AddonId"
           LEFT JOIN "AddonMediaListRefreshState" AS "refresh"
             ON "refresh"."AddonMediaListId" = "list"."Id"
           LEFT JOIN "AddonMediaSource" AS "source"
             ON "source"."AddonId" = "list"."AddonId"
           LEFT JOIN "AddonMediaListLibrary" AS "managed"
             ON "managed"."AddonMediaListId" = "list"."Id"
           ORDER BY "list"."Name", "list"."Id""#,
    )
    .fetch_all(pool)
    .await
    .map_err(sqlx_error)
}

struct ConfiguredListHealth {
    failures: Vec<String>,
    warnings: Vec<String>,
    import_failure_link: Option<String>,
    coordinator_activity: Vec<String>,
}

impl ConfiguredListHealth {
    fn failure_result(
        &self,
        title: &'static str,
        lifecycle_foundation: LifecycleFoundationHealth,
    ) -> Option<HealthCheckResult> {
        (!self.failures.is_empty()).then(|| {
            HealthCheckResult::new(
                title,
                HealthCheckStatus::Fail,
                format!(
                    "{}; {}",
                    self.failures.join("; "),
                    lifecycle_foundation.summary()
                ),
                "One or more add-on list or tool-maintenance operations need attention",
            )
            .with_link(
                self.import_failure_link
                    .clone()
                    .unwrap_or_else(|| "/media/addon-lists".to_owned()),
            )
        })
    }

    fn warning_result(
        &self,
        title: &'static str,
        lifecycle_foundation: LifecycleFoundationHealth,
    ) -> Option<HealthCheckResult> {
        (!self.warnings.is_empty()).then(|| {
            HealthCheckResult::new(
                title,
                HealthCheckStatus::Warning,
                format!(
                    "{}; {}",
                    self.warnings.join("; "),
                    lifecycle_foundation.summary()
                ),
                "Review the affected add-on or choose another refresh cadence",
            )
            .with_link("/media/addon-lists")
        })
    }
}

async fn configured_list_health(
    pool: &Pool<Db>,
    rows: &[crate::db::DbRow],
    lifecycle_foundation: LifecycleFoundationHealth,
) -> Result<ConfiguredListHealth> {
    let cadence_warnings = cadence_warnings(rows);
    let refresh_terminal_failures = refresh_terminal_failures(rows);
    let storage_failures = rows
        .iter()
        .filter_map(|row| {
            row.try_get::<Option<i64>, _>("MediaSourceId")
                .ok()
                .flatten()?;
            let name = row
                .try_get::<String, _>("Name")
                .unwrap_or_else(|_| "List".to_owned());
            let library_id = row.try_get::<Option<i64>, _>("LibraryId").ok().flatten();
            let manifest_path = row
                .try_get::<Option<String>, _>("ManifestPath")
                .ok()
                .flatten();
            (library_id.is_none()
                || manifest_path
                    .as_deref()
                    .is_none_or(|path| !std::path::Path::new(path).is_file()))
            .then(|| format!("{name}: managed local library is unavailable"))
        })
        .take(5)
        .collect::<Vec<_>>();
    let refresh_failures = rows
        .iter()
        .filter_map(|row| {
            let error = row
                .try_get::<Option<String>, _>("LastError")
                .ok()
                .flatten()?;
            let name = row
                .try_get::<String, _>("Name")
                .unwrap_or_else(|_| "List".to_owned());
            Some(format!("{name}: {error}"))
        })
        .take(5)
        .collect::<Vec<_>>();
    let presentation_failures = presentation_failures(rows);
    let publication_failures = publication_failures(rows);
    let PublicationCoordinatorHealth {
        failures: coordinator_failures,
        active: coordinator_activity,
    } = publication_coordinator_health(pool).await?;
    let invalid_metadata = invalid_metadata(pool).await?;
    let missing_artwork = missing_artwork(pool).await?;
    let import_failures = import_failures(pool).await?;
    let import_failure_link = import_failures
        .first()
        .map(|(list_id, _)| format!("/media/addon-lists/{list_id}"));
    let detail_worker_failures = detail_worker_failures(pool).await?;
    let detail_wait_setting_failures = detail_wait_setting_failures(pool).await;
    let cooldown_failures = cooldown_failures(pool).await?;
    let failures = storage_failures
        .into_iter()
        .chain(refresh_terminal_failures)
        .chain(refresh_failures)
        .chain(presentation_failures)
        .chain(publication_failures)
        .chain(coordinator_failures)
        .chain(lifecycle_foundation.failures())
        .chain(invalid_metadata)
        .chain(missing_artwork)
        .chain(import_failures.into_iter().map(|(_, message)| message))
        .chain(detail_worker_failures)
        .chain(detail_wait_setting_failures)
        .chain(cooldown_failures)
        .take(5)
        .collect();
    Ok(ConfiguredListHealth {
        failures,
        warnings: cadence_warnings,
        import_failure_link,
        coordinator_activity,
    })
}

fn cadence_warnings(rows: &[crate::db::DbRow]) -> Vec<String> {
    rows.iter()
        .filter_map(|row| {
            let name = row
                .try_get::<String, _>("Name")
                .unwrap_or_else(|_| "List".to_owned());
            let adaptive = row
                .try_get::<i64, _>("RefreshCadence")
                .is_ok_and(|cadence| {
                    cadence
                        == i64::from(crate::repositories::common::encode_enum(
                            AddonMediaListRefreshCadence::Adaptive,
                        ))
                });
            if adaptive {
                let enabled = row
                    .try_get::<Option<i64>, _>("AddonEnabled")
                    .ok()
                    .flatten();
                if enabled.is_none() {
                    return Some(format!("{name}: adaptive refresh add-on is missing"));
                }
                if enabled == Some(0) {
                    return Some(format!("{name}: adaptive refresh add-on is disabled"));
                }
                let supports_source_check = row
                    .try_get::<Option<String>, _>("AddonManifestJson")
                    .ok()
                    .flatten()
                    .and_then(|json| serde_json::from_str::<AddonManifest>(&json).ok())
                    .is_some_and(|manifest| {
                        manifest.has_capability(CAPABILITY_MEDIA_LIST_SOURCE_CHECK_V1)
                    });
                if !supports_source_check {
                    return Some(format!(
                        "{name}: adaptive refresh add-on no longer declares {CAPABILITY_MEDIA_LIST_SOURCE_CHECK_V1}"
                    ));
                }
            }
            None
        })
        .take(5)
        .collect()
}

fn refresh_terminal_failures(rows: &[crate::db::DbRow]) -> Vec<String> {
    rows.iter()
        .filter_map(|row| {
            let capability_unavailable = row
                .try_get::<Option<String>, _>("RefreshDiagnosticCode")
                .ok()
                .flatten()
                .is_some_and(|code| code == "capability-unavailable");
            let terminal = row
                .try_get::<Option<i64>, _>("RefreshState")
                .ok()
                .flatten()
                .is_some_and(|state| {
                    state
                        == i64::from(crate::repositories::common::encode_enum(
                            AddonMediaListRefreshState::Failed,
                        ))
                });
            (terminal && !capability_unavailable).then(|| {
                let name = row
                    .try_get::<String, _>("Name")
                    .unwrap_or_else(|_| "List".to_owned());
                format!("{name}: automatic refresh stopped after a terminal failure")
            })
        })
        .take(5)
        .collect()
}

async fn configured_list_status_result(
    pool: &Pool<Db>,
    rows: &[crate::db::DbRow],
    coordinator_activity: &[String],
    lifecycle_foundation: LifecycleFoundationHealth,
    title: &'static str,
) -> Result<HealthCheckResult> {
    let disabled = rows
        .iter()
        .filter(|row| {
            row.try_get::<Option<i64>, _>("AddonEnabled")
                .is_ok_and(|value| value == Some(0))
        })
        .count();
    if disabled > 0 {
        return Ok(HealthCheckResult::new(
            title,
            HealthCheckStatus::Warning,
            format!("{disabled} list(s) use a disabled add-on"),
            "An add-on must be enabled before its lists can refresh",
        )
        .with_link("/media/addon-lists"));
    }
    if let Some(result) = replacement_failure(pool, title).await? {
        return Ok(result);
    }
    let metadata_conflicts = count_metadata_conflicts(pool).await?;
    if metadata_conflicts > 0 {
        return Ok(HealthCheckResult::new(
            title,
            HealthCheckStatus::Warning,
            format!("{metadata_conflicts} metadata field conflict(s) need review"),
            "A provider changed fields that have operator overrides",
        )
        .with_link("/media/addon-lists"));
    }
    configured_list_activity_result(
        pool,
        rows,
        coordinator_activity,
        lifecycle_foundation,
        title,
    )
    .await
}

async fn configured_list_activity_result(
    pool: &Pool<Db>,
    rows: &[crate::db::DbRow],
    coordinator_activity: &[String],
    lifecycle_foundation: LifecycleFoundationHealth,
    title: &'static str,
) -> Result<HealthCheckResult> {
    let detail_activity = detail_activity(pool).await?;
    if detail_activity.failed > 0 {
        return Ok(HealthCheckResult::new(
            title,
            HealthCheckStatus::Warning,
            format!(
                "{} item detail job(s) need a manual retry; {} are active and {} are waiting for retry or provider pacing",
                detail_activity.failed, detail_activity.active, detail_activity.waiting
            ),
            "One or more add-on item details could not be loaded",
        )
        .with_link("/media/addon-lists"));
    }
    let pending_review = rows
        .iter()
        .filter(|row| {
            row.try_get::<i64, _>("ReviewState")
                .is_ok_and(|state| state != 2)
        })
        .count();
    if pending_review > 0 {
        let import_activity = import_activity(pool).await?;
        let publication_activity = if coordinator_activity.is_empty() {
            String::new()
        } else {
            format!("; {}", coordinator_activity.join("; "))
        };
        return Ok(HealthCheckResult::new(
            title,
            HealthCheckStatus::Warning,
            format!(
                "{pending_review} add-on media list(s) await automatic processing; {} overview import(s) are active and {} detail item(s) are delayed by retry or provider pacing{publication_activity}",
                import_activity.active,
                detail_activity.waiting,
            ),
            "Review and publish the imported metadata before using the new revision",
        )
        .with_link("/media/addon-lists"));
    }
    if !coordinator_activity.is_empty() {
        return Ok(HealthCheckResult::new(
            title,
            HealthCheckStatus::Pass,
            coordinator_activity.join("; "),
            "Add-on list publication is making or awaiting progress",
        )
        .with_link("/media/addon-lists"));
    }
    if detail_activity.active > 0 || detail_activity.waiting > 0 {
        return Ok(HealthCheckResult::new(
            title,
            HealthCheckStatus::Pass,
            format!(
                "{} item detail job(s) are active and {} are waiting for retry or provider pacing",
                detail_activity.active, detail_activity.waiting
            ),
            "Add-on detail loading is making or awaiting progress",
        )
        .with_link("/media/addon-lists"));
    }
    Ok(HealthCheckResult::new(
        title,
        HealthCheckStatus::Pass,
        lifecycle_foundation.summary(),
        "Add-on list lifecycle foundations are healthy",
    )
    .with_link("/media/addon-lists"))
}

#[derive(Clone, Copy, Debug, Default)]
struct ImportActivity {
    active: i64,
}

#[derive(Clone, Copy, Debug, Default)]
struct DetailActivity {
    active: i64,
    waiting: i64,
    failed: i64,
}

#[derive(Debug, Default)]
struct PublicationCoordinatorHealth {
    failures: Vec<String>,
    active: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default)]
struct LifecycleFoundationHealth {
    lifecycle_active: i64,
    lifecycle_waiting: i64,
    lifecycle_expired: i64,
    lifecycle_failed: i64,
    checkpoints: i64,
    retained_revisions: i64,
    complete_revision_snapshots: i64,
    invalid_revision_snapshots: i64,
    revision_file_references: i64,
    revision_file_blobs: i64,
    invalid_revision_files: i64,
    tombstones: i64,
    artifacts: i64,
    artifact_ready: i64,
    artifact_failed: i64,
    artifact_bytes: i64,
    artifact_reserved_bytes: i64,
    tool_active: i64,
    tool_waiting: i64,
    tool_expired: i64,
    tool_failed: i64,
    tool_actionable_failed: i64,
    tool_updated: i64,
    tool_already_current: i64,
    tool_manual_update_required: i64,
    tool_rolled_back: i64,
    tool_failed_outcome: i64,
}

impl LifecycleFoundationHealth {
    fn result_without_lists(self, title: &'static str) -> HealthCheckResult {
        if !self.has_state() {
            return HealthCheckResult::not_applicable(title);
        }
        let failures = self.failures();
        if !failures.is_empty() {
            return HealthCheckResult::new(
                title,
                HealthCheckStatus::Fail,
                format!("{}; {}", failures.join("; "), self.summary()),
                "One or more add-on list or tool-maintenance operations need attention",
            )
            .with_link("/settings/addons");
        }
        HealthCheckResult::new(
            title,
            HealthCheckStatus::Pass,
            self.summary(),
            "Add-on lifecycle or tool maintenance is active or retained",
        )
        .with_link("/settings/addons")
    }

    const fn has_state(self) -> bool {
        self.lifecycle_active > 0
            || self.lifecycle_waiting > 0
            || self.lifecycle_expired > 0
            || self.lifecycle_failed > 0
            || self.checkpoints > 0
            || self.retained_revisions > 0
            || self.revision_file_references > 0
            || self.revision_file_blobs > 0
            || self.tombstones > 0
            || self.artifacts > 0
            || self.tool_active > 0
            || self.tool_waiting > 0
            || self.tool_expired > 0
            || self.tool_failed > 0
            || self.tool_updated > 0
            || self.tool_already_current > 0
            || self.tool_manual_update_required > 0
            || self.tool_rolled_back > 0
            || self.tool_failed_outcome > 0
    }

    fn failures(self) -> Vec<String> {
        let mut failures = Vec::new();
        let lifecycle_recovery = self.lifecycle_expired + self.lifecycle_failed;
        if lifecycle_recovery > 0 {
            failures.push(format!(
                "{lifecycle_recovery} lifecycle job(s) require recovery ({} expired, {} failed)",
                self.lifecycle_expired, self.lifecycle_failed
            ));
        }
        let tool_recovery = self.tool_expired + self.tool_actionable_failed;
        if tool_recovery > 0 {
            failures.push(format!(
                "{tool_recovery} tool maintenance job(s) require recovery ({} expired, {} failed)",
                self.tool_expired, self.tool_actionable_failed
            ));
        }
        if self.invalid_revision_snapshots > 0 {
            failures.push(format!(
                "{} retained revision snapshot(s) are structurally incomplete or invalid",
                self.invalid_revision_snapshots
            ));
        }
        if self.invalid_revision_files > 0 {
            failures.push(format!(
                "{} retained revision file record(s) have invalid metadata or references",
                self.invalid_revision_files
            ));
        }
        failures
    }

    fn summary(self) -> String {
        let legacy_revisions = self
            .retained_revisions
            .saturating_sub(self.complete_revision_snapshots)
            .saturating_sub(self.invalid_revision_snapshots);
        format!(
            "lifecycle active={} waiting={} expired={} failed={}; checkpoints={}; retained revisions total={} structurally_complete={} legacy={} structurally_invalid={}; retained files references={} blobs={} invalid={}; tombstones={}; artifacts total={} ready={} failed={} bytes={} reserved_bytes={}; tool maintenance active={} waiting={} expired={} failed={}; outcomes updated={} already_current={} manual_update_required={} rolled_back={} failed={}",
            self.lifecycle_active,
            self.lifecycle_waiting,
            self.lifecycle_expired,
            self.lifecycle_failed,
            self.checkpoints,
            self.retained_revisions,
            self.complete_revision_snapshots,
            legacy_revisions,
            self.invalid_revision_snapshots,
            self.revision_file_references,
            self.revision_file_blobs,
            self.invalid_revision_files,
            self.tombstones,
            self.artifacts,
            self.artifact_ready,
            self.artifact_failed,
            self.artifact_bytes,
            self.artifact_reserved_bytes,
            self.tool_active,
            self.tool_waiting,
            self.tool_expired,
            self.tool_failed,
            self.tool_updated,
            self.tool_already_current,
            self.tool_manual_update_required,
            self.tool_rolled_back,
            self.tool_failed_outcome,
        )
    }
}

const LIFECYCLE_FOUNDATION_HEALTH_SQL: &str = r#"WITH "CompleteRevisionSnapshot" AS (
               SELECT "Id"
                 FROM "AddonMediaListRevision"
                WHERE "SnapshotVersion" = 1
                  AND LENGTH(TRIM("SnapshotDigest")) = 64
                  AND "SourceDraftRevision" IS NOT NULL
                  AND "CompletedAt" IS NOT NULL
                  AND JSON_VALID("OptionValuesJson")
                  AND "Name" IS NOT NULL
                  AND "GeneratePlaylist" IN (0, 1)
                  AND "ContentKindOverride" IS NOT NULL
                  AND "PresentationKind" IS NOT NULL
                  AND "RequestedPresentationKind" IS NOT NULL
                  AND JSON_VALID("MetadataJson")
           )
           SELECT
               (SELECT COUNT(*)
                  FROM "AddonMediaListLifecycleJob" AS "job"
                  JOIN "ManagedLibraryOperation" AS "operation"
                    ON "operation"."Id" = "job"."OperationId"
                 WHERE "operation"."State" = 0
                    OR ("operation"."State" = 1 AND EXISTS (
                        SELECT 1 FROM "ManagedLibraryLease" AS "lease"
                         WHERE "lease"."OperationId" = "operation"."Id"
                           AND "lease"."OwnerToken" = "operation"."OwnerToken"
                           AND julianday("lease"."LeaseExpiresAt") > julianday(?)
                    ))) AS "LifecycleActive",
               (SELECT COUNT(*)
                  FROM "AddonMediaListLifecycleJob" AS "job"
                  JOIN "ManagedLibraryOperation" AS "operation"
                    ON "operation"."Id" = "job"."OperationId"
                 WHERE "operation"."State" = 2) AS "LifecycleWaiting",
               (SELECT COUNT(*)
                  FROM "AddonMediaListLifecycleJob" AS "job"
                  JOIN "ManagedLibraryOperation" AS "operation"
                    ON "operation"."Id" = "job"."OperationId"
                 WHERE "operation"."State" = 1
                   AND NOT EXISTS (
                       SELECT 1 FROM "ManagedLibraryLease" AS "lease"
                        WHERE "lease"."OperationId" = "operation"."Id"
                          AND "lease"."OwnerToken" = "operation"."OwnerToken"
                          AND julianday("lease"."LeaseExpiresAt") > julianday(?)
                   )) AS "LifecycleExpired",
               (SELECT COUNT(*)
                  FROM "AddonMediaListLifecycleJob" AS "job"
                  JOIN "ManagedLibraryOperation" AS "operation"
                    ON "operation"."Id" = "job"."OperationId"
                 WHERE "operation"."State" = 4) AS "LifecycleFailed",
               (SELECT COUNT(*) FROM "AddonMediaListImportCheckpoint") AS "Checkpoints",
               (SELECT COUNT(*) FROM "AddonMediaListRevision") AS "RetainedRevisions",
               (SELECT COUNT(*) FROM "CompleteRevisionSnapshot") AS "CompleteRevisionSnapshots",
               (SELECT COUNT(*)
                  FROM "AddonMediaListRevision" AS "revision"
                 WHERE NOT (
                       "revision"."SnapshotVersion" IS NULL
                   AND "revision"."SnapshotDigest" IS NULL
                 )
                   AND NOT EXISTS (
                       SELECT 1 FROM "CompleteRevisionSnapshot" AS "complete"
                        WHERE "complete"."Id" = "revision"."Id"
                   )) AS "InvalidRevisionSnapshots",
               (SELECT COUNT(*) FROM "AddonMediaListRevisionFile") AS "RevisionFileReferences",
               (SELECT COUNT(*) FROM "AddonMediaListRevisionFileBlob") AS "RevisionFileBlobs",
               ((SELECT COUNT(*)
                   FROM "AddonMediaListRevisionFileBlob" AS "blob"
                  WHERE TRIM("blob"."StorageRelativePath") = ''
                     OR TRIM("blob"."Sha256") = ''
                     OR "blob"."ByteLength" < 0
                     OR NOT EXISTS (
                         SELECT 1 FROM "AddonMediaList" AS "list"
                          WHERE "list"."Id" = "blob"."AddonMediaListId"
                     ))
                +
                (SELECT COUNT(*)
                   FROM "AddonMediaListRevisionFile" AS "file"
                   LEFT JOIN "AddonMediaListRevisionFileBlob" AS "blob"
                     ON "blob"."Id" = "file"."AddonMediaListRevisionFileBlobId"
                  WHERE TRIM("file"."PublishedRelativePath") = ''
                     OR "blob"."Id" IS NULL
                     OR NOT EXISTS (
                         SELECT 1 FROM "CompleteRevisionSnapshot" AS "complete"
                          WHERE "complete"."Id" = "file"."AddonMediaListRevisionId"
                     ))) AS "InvalidRevisionFiles",
               (SELECT COUNT(*) FROM "AddonMediaListItemTombstone") AS "Tombstones",
               (SELECT COUNT(*) FROM "AddonMediaListArtifact") AS "Artifacts",
               (SELECT COUNT(*) FROM "AddonMediaListArtifact" WHERE "State" = 2)
                   AS "ArtifactReady",
               (SELECT COUNT(*) FROM "AddonMediaListArtifact" WHERE "State" = 3)
                   AS "ArtifactFailed",
               (SELECT COALESCE(SUM("ByteLength"), 0) FROM "AddonMediaListArtifact")
                   AS "ArtifactBytes",
               (SELECT COALESCE(SUM("ReservedBytes"), 0) FROM "AddonMediaListArtifact")
                   AS "ArtifactReservedBytes",
               (SELECT COUNT(*) FROM "AddonToolMaintenanceJob"
                 WHERE "State" = 0
                    OR ("State" = 1 AND "LeaseOwner" IS NOT NULL
                        AND julianday("LeaseExpiresAt") > julianday(?))) AS "ToolActive",
               (SELECT COUNT(*) FROM "AddonToolMaintenanceJob" WHERE "State" = 2)
                   AS "ToolWaiting",
               (SELECT COUNT(*) FROM "AddonToolMaintenanceJob"
                 WHERE "State" = 1
                   AND ("LeaseOwner" IS NULL
                        OR "LeaseExpiresAt" IS NULL
                        OR julianday("LeaseExpiresAt") <= julianday(?))) AS "ToolExpired",
               (SELECT COUNT(*) FROM "AddonToolMaintenanceJob" WHERE "State" = 4)
                   AS "ToolFailed",
               (SELECT COUNT(*)
                  FROM "AddonToolMaintenanceJob" AS "job"
                 WHERE "job"."State" = 4
                   AND NOT EXISTS (
                       SELECT 1 FROM "AddonToolMaintenanceJob" AS "newer"
                        WHERE "newer"."AddonId" = "job"."AddonId"
                          AND "newer"."ToolKey" = "job"."ToolKey"
                          AND (
                              julianday("newer"."CreatedAt") > julianday("job"."CreatedAt")
                              OR ("newer"."CreatedAt" = "job"."CreatedAt"
                                  AND "newer"."Id" > "job"."Id")
                          )
                   )) AS "ToolActionableFailed",
               (SELECT COUNT(*) FROM "AddonToolMaintenanceJob" WHERE "Outcome" = 0)
                   AS "ToolUpdated",
               (SELECT COUNT(*) FROM "AddonToolMaintenanceJob" WHERE "Outcome" = 1)
                   AS "ToolAlreadyCurrent",
               (SELECT COUNT(*) FROM "AddonToolMaintenanceJob" WHERE "Outcome" = 2)
                   AS "ToolManualUpdateRequired",
               (SELECT COUNT(*) FROM "AddonToolMaintenanceJob" WHERE "Outcome" = 3)
                   AS "ToolRolledBack",
               (SELECT COUNT(*) FROM "AddonToolMaintenanceJob" WHERE "Outcome" = 4)
                   AS "ToolFailedOutcome""#;

async fn lifecycle_foundation_health(pool: &Pool<Db>) -> Result<LifecycleFoundationHealth> {
    let now = crate::repositories::common::encode_datetime(chrono::Utc::now());
    let row = sqlx::query(LIFECYCLE_FOUNDATION_HEALTH_SQL)
        .bind(&now)
        .bind(&now)
        .bind(&now)
        .bind(&now)
        .fetch_one(pool)
        .await
        .map_err(sqlx_error)?;
    decode_lifecycle_foundation_health(&row)
}

fn decode_lifecycle_foundation_health(row: &crate::db::DbRow) -> Result<LifecycleFoundationHealth> {
    Ok(LifecycleFoundationHealth {
        lifecycle_active: row.try_get("LifecycleActive").map_err(sqlx_error)?,
        lifecycle_waiting: row.try_get("LifecycleWaiting").map_err(sqlx_error)?,
        lifecycle_expired: row.try_get("LifecycleExpired").map_err(sqlx_error)?,
        lifecycle_failed: row.try_get("LifecycleFailed").map_err(sqlx_error)?,
        checkpoints: row.try_get("Checkpoints").map_err(sqlx_error)?,
        retained_revisions: row.try_get("RetainedRevisions").map_err(sqlx_error)?,
        complete_revision_snapshots: row
            .try_get("CompleteRevisionSnapshots")
            .map_err(sqlx_error)?,
        invalid_revision_snapshots: row
            .try_get("InvalidRevisionSnapshots")
            .map_err(sqlx_error)?,
        revision_file_references: row.try_get("RevisionFileReferences").map_err(sqlx_error)?,
        revision_file_blobs: row.try_get("RevisionFileBlobs").map_err(sqlx_error)?,
        invalid_revision_files: row.try_get("InvalidRevisionFiles").map_err(sqlx_error)?,
        tombstones: row.try_get("Tombstones").map_err(sqlx_error)?,
        artifacts: row.try_get("Artifacts").map_err(sqlx_error)?,
        artifact_ready: row.try_get("ArtifactReady").map_err(sqlx_error)?,
        artifact_failed: row.try_get("ArtifactFailed").map_err(sqlx_error)?,
        artifact_bytes: row.try_get("ArtifactBytes").map_err(sqlx_error)?,
        artifact_reserved_bytes: row.try_get("ArtifactReservedBytes").map_err(sqlx_error)?,
        tool_active: row.try_get("ToolActive").map_err(sqlx_error)?,
        tool_waiting: row.try_get("ToolWaiting").map_err(sqlx_error)?,
        tool_expired: row.try_get("ToolExpired").map_err(sqlx_error)?,
        tool_failed: row.try_get("ToolFailed").map_err(sqlx_error)?,
        tool_actionable_failed: row.try_get("ToolActionableFailed").map_err(sqlx_error)?,
        tool_updated: row.try_get("ToolUpdated").map_err(sqlx_error)?,
        tool_already_current: row.try_get("ToolAlreadyCurrent").map_err(sqlx_error)?,
        tool_manual_update_required: row
            .try_get("ToolManualUpdateRequired")
            .map_err(sqlx_error)?,
        tool_rolled_back: row.try_get("ToolRolledBack").map_err(sqlx_error)?,
        tool_failed_outcome: row.try_get("ToolFailedOutcome").map_err(sqlx_error)?,
    })
}

async fn import_activity(pool: &Pool<Db>) -> Result<ImportActivity> {
    let active = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "AddonMediaListImportJob" AS "job"
           WHERE "job"."State" IN (0, 1, 2)
             AND "job"."Generation" = (
                 SELECT MAX("latest"."Generation") FROM "AddonMediaListImportJob" AS "latest"
                 WHERE "latest"."AddonMediaListId" = "job"."AddonMediaListId"
             )"#,
    )
    .fetch_one(pool)
    .await
    .map_err(sqlx_error)?;
    Ok(ImportActivity { active })
}

async fn detail_activity(pool: &Pool<Db>) -> Result<DetailActivity> {
    let row = sqlx::query(
        r#"SELECT
               COUNT(CASE WHEN "job"."State" IN (1, 3) THEN 1 END) AS "Active",
               COUNT(CASE WHEN "job"."State" IN (2, 4) THEN 1 END) AS "Waiting",
               COUNT(CASE WHEN "job"."State" = 7 THEN 1 END) AS "Failed"
           FROM "AddonMediaListDetailJob" AS "job"
           JOIN "AddonMediaListItem" AS "item"
             ON "item"."Id" = "job"."AddonMediaListItemId"
           WHERE "item"."Included" = 1"#,
    )
    .fetch_one(pool)
    .await
    .map_err(sqlx_error)?;
    Ok(DetailActivity {
        active: row.try_get::<i64, _>("Active").map_err(sqlx_error)?,
        waiting: row.try_get::<i64, _>("Waiting").map_err(sqlx_error)?,
        failed: row.try_get::<i64, _>("Failed").map_err(sqlx_error)?,
    })
}

async fn import_failures(pool: &Pool<Db>) -> Result<Vec<(i64, String)>> {
    let rows = sqlx::query(
        r#"SELECT "list"."Id" AS "ListId", "list"."Name", "job"."State", "job"."Mode",
                  "job"."DiscoveredCount", "job"."Diagnostic",
                  CAST(CASE WHEN EXISTS (
                      SELECT 1 FROM "AddonMediaListItem" AS "item"
                      WHERE "item"."AddonMediaListId" = "list"."Id"
                  ) THEN 1 ELSE 0 END AS SIGNED) AS "HasRetainedItems",
                  "job"."LastProgressAt", "job"."UpdatedAt"
           FROM "AddonMediaListImportJob" AS "job"
           JOIN "AddonMediaList" AS "list" ON "list"."Id" = "job"."AddonMediaListId"
           WHERE "job"."Generation" = (
               SELECT MAX("latest"."Generation") FROM "AddonMediaListImportJob" AS "latest"
               WHERE "latest"."AddonMediaListId" = "job"."AddonMediaListId"
           )"#,
    )
    .fetch_all(pool)
    .await
    .map_err(sqlx_error)?;
    let threshold = chrono::Utc::now() - chrono::Duration::minutes(STUCK_IMPORT_MINUTES);
    let mut failures = Vec::new();
    for row in rows {
        let list_id = row.try_get::<i64, _>("ListId").map_err(sqlx_error)?;
        let name = row
            .try_get::<String, _>("Name")
            .unwrap_or_else(|_| "List".to_owned());
        let state = row.try_get::<i64, _>("State").map_err(sqlx_error)?;
        let mode = row.try_get::<i64, _>("Mode").map_err(sqlx_error)?;
        let discovered_count = row
            .try_get::<i64, _>("DiscoveredCount")
            .map_err(sqlx_error)?;
        let has_retained_items = row
            .try_get::<i64, _>("HasRetainedItems")
            .map_err(sqlx_error)?
            != 0;
        if state == 3 && discovered_count == 0 && (mode == 0 || !has_retained_items) {
            failures.push((
                list_id,
                format!(
                    "{name}: completed provider overview contains no items; update the add-on and retry the import"
                ),
            ));
            continue;
        }
        if state == 5 {
            let diagnostic = row
                .try_get::<Option<String>, _>("Diagnostic")
                .map_err(sqlx_error)?
                .unwrap_or_else(|| "provider overview needs attention".to_owned());
            failures.push((list_id, format!("{name}: {diagnostic}")));
            continue;
        }
        if !matches!(state, 0..=2) {
            continue;
        }
        let progress = row
            .try_get::<Option<String>, _>("LastProgressAt")
            .map_err(sqlx_error)?
            .or(row
                .try_get::<Option<String>, _>("UpdatedAt")
                .map_err(sqlx_error)?);
        let progress = crate::repositories::common::decode_optional_datetime(
            "add-on overview progress",
            progress,
        )
        .map_err(|error| HealthCheckError::Database(error.to_string()))?;
        if progress.is_none_or(|progress| progress <= threshold) {
            failures.push((
                list_id,
                format!(
                    "{name}: overview import has made no progress for more than {STUCK_IMPORT_MINUTES} minutes"
                ),
            ));
        }
    }
    Ok(failures)
}

async fn detail_worker_failures(pool: &Pool<Db>) -> Result<Vec<String>> {
    let threshold = crate::repositories::common::encode_datetime(
        chrono::Utc::now() - chrono::Duration::minutes(STUCK_IMPORT_MINUTES),
    );
    let rows = sqlx::query(
        r#"SELECT "list"."Name", "item"."DisplayTitle"
           FROM "AddonMediaListDetailJob" AS "detail"
           JOIN "AddonMediaList" AS "list" ON "list"."Id" = "detail"."AddonMediaListId"
           JOIN "AddonMediaListItem" AS "item"
             ON "item"."Id" = "detail"."AddonMediaListItemId"
           WHERE "detail"."State" = 3 AND "item"."Included" = 1
             AND "detail"."UpdatedAt" <= ?
             AND ("detail"."LeaseExpiresAt" IS NULL OR "detail"."LeaseExpiresAt" <= ?)
           ORDER BY "detail"."UpdatedAt" LIMIT 5"#,
    )
    .bind(&threshold)
    .bind(crate::repositories::common::encode_datetime(
        chrono::Utc::now(),
    ))
    .fetch_all(pool)
    .await
    .map_err(sqlx_error)?;
    Ok(rows
        .into_iter()
        .map(|row| {
            format!(
                "{}: detail worker lease is stale for {}",
                row.try_get::<String, _>("Name")
                    .unwrap_or_else(|_| "List".to_owned()),
                row.try_get::<String, _>("DisplayTitle")
                    .unwrap_or_else(|_| "item".to_owned())
            )
        })
        .collect())
}

async fn cooldown_failures(pool: &Pool<Db>) -> Result<Vec<String>> {
    let rows = sqlx::query(
        r#"SELECT "ProviderHost", "SampledDelaySeconds"
           FROM "AddonProviderHostCooldown"
           WHERE "SampledDelaySeconds" <= 0
           ORDER BY "ProviderHost" LIMIT 5"#,
    )
    .fetch_all(pool)
    .await
    .map_err(sqlx_error)?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let host = row
                .try_get::<String, _>("ProviderHost")
                .unwrap_or_else(|_| "provider".to_owned());
            let delay = row
                .try_get::<i64, _>("SampledDelaySeconds")
                .unwrap_or_default();
            format!("{host}: persisted provider delay {delay}s is not positive")
        })
        .collect())
}

async fn detail_wait_setting_failures(pool: &Pool<Db>) -> Vec<String> {
    crate::addon_media_list_import::load_detail_wait_range(pool)
        .await
        .err()
        .map(|error| format!("Add-on detail wait settings are invalid: {error}"))
        .into_iter()
        .collect()
}

async fn count_metadata_conflicts(pool: &Pool<Db>) -> Result<i64> {
    let list_count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "AddonMediaList"
           WHERE "MetadataConflictJson" IS NOT NULL AND "RefreshPublicationPolicy"<>2
             AND TRIM("MetadataConflictJson") NOT IN ('', '{}')"#,
    )
    .fetch_one(pool)
    .await
    .map_err(sqlx_error)?;
    let item_count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "AddonMediaListItem"
           WHERE EXISTS (SELECT 1 FROM "AddonMediaList" WHERE "Id"="AddonMediaListItem"."AddonMediaListId" AND "RefreshPublicationPolicy"<>2)
             AND "MetadataConflictJson" IS NOT NULL
             AND TRIM("MetadataConflictJson") NOT IN ('', '{}')"#,
    )
    .fetch_one(pool)
    .await
    .map_err(sqlx_error)?;
    Ok(list_count + item_count)
}

async fn invalid_metadata(pool: &Pool<Db>) -> Result<Vec<String>> {
    let rows = publication_metadata_rows(pool).await?;
    let mut failures = Vec::new();
    for row in rows {
        let name = row
            .try_get::<String, _>("Name")
            .unwrap_or_else(|_| "List".to_owned());
        let provider = row
            .try_get::<String, _>("ProviderMetadataJson")
            .map_err(sqlx_error)?;
        let overrides = row
            .try_get::<String, _>("MetadataOverrideJson")
            .map_err(sqlx_error)?;
        match merged_metadata(&provider, &overrides) {
            Ok(metadata) if metadata.title.trim().is_empty() => {
                failures.push(format!("{name}: published metadata has no title"));
            }
            Ok(_) => {}
            Err(error) => failures.push(format!("{name}: published metadata is invalid: {error}")),
        }
    }
    Ok(failures)
}

async fn missing_artwork(pool: &Pool<Db>) -> Result<Vec<String>> {
    use std::collections::HashSet;

    let materialized = sqlx::query(
        r#"SELECT "AddonMediaListId", "ScopeItemId", "Role"
           FROM "AddonMediaListArtworkMaterialization""#,
    )
    .fetch_all(pool)
    .await
    .map_err(sqlx_error)?
    .into_iter()
    .filter_map(|row| {
        Some((
            row.try_get::<i64, _>("AddonMediaListId").ok()?,
            row.try_get::<i64, _>("ScopeItemId").ok()?,
            row.try_get::<i64, _>("Role").ok()?,
        ))
    })
    .collect::<HashSet<_>>();
    let mut failures = Vec::new();
    for row in publication_metadata_rows(pool).await? {
        let list_id = row
            .try_get::<i64, _>("AddonMediaListId")
            .map_err(sqlx_error)?;
        let scope_item_id = row.try_get::<i64, _>("ScopeItemId").map_err(sqlx_error)?;
        let presentation_kind = row
            .try_get::<i64, _>("PresentationKind")
            .map_err(sqlx_error)?;
        if scope_item_id == 0 && !matches!(presentation_kind, 1 | 4) {
            continue;
        }
        let name = row
            .try_get::<String, _>("Name")
            .unwrap_or_else(|_| "List".to_owned());
        let provider = row
            .try_get::<String, _>("ProviderMetadataJson")
            .map_err(sqlx_error)?;
        let overrides = row
            .try_get::<String, _>("MetadataOverrideJson")
            .map_err(sqlx_error)?;
        let Ok(metadata) = merged_metadata(&provider, &overrides) else {
            continue;
        };
        let mut roles = metadata
            .artwork
            .iter()
            .filter(|candidate| !candidate.url.trim().is_empty())
            .map(|candidate| artwork_role_value(candidate.role))
            .collect::<HashSet<_>>();
        if !metadata.artwork_urls.is_empty() {
            roles.insert(artwork_role_value(
                ersatzrs_core::repositories::AddonMediaListArtworkRole::Thumb,
            ));
        }
        for role in roles {
            if !materialized.contains(&(list_id, scope_item_id, role)) {
                failures.push(format!("{name}: selected artwork is not materialized"));
                break;
            }
        }
    }
    Ok(failures)
}

const fn artwork_role_value(role: ersatzrs_core::repositories::AddonMediaListArtworkRole) -> i64 {
    match role {
        ersatzrs_core::repositories::AddonMediaListArtworkRole::Poster => 1,
        ersatzrs_core::repositories::AddonMediaListArtworkRole::Fanart => 2,
        ersatzrs_core::repositories::AddonMediaListArtworkRole::Thumb => 3,
        ersatzrs_core::repositories::AddonMediaListArtworkRole::Other => 0,
    }
}

async fn publication_metadata_rows(pool: &Pool<Db>) -> Result<Vec<crate::db::DbRow>> {
    sqlx::query(
        r#"SELECT "list"."Id" AS "AddonMediaListId", 0 AS "ScopeItemId",
                  "list"."PresentationKind" AS "PresentationKind",
                  "list"."Name" AS "Name", "list"."ProviderMetadataJson",
                  "list"."MetadataOverrideJson"
           FROM "AddonMediaList" AS "list"
           WHERE "list"."ReviewState" = 2
           UNION ALL
           SELECT "item"."AddonMediaListId", "item"."Id" AS "ScopeItemId",
                  "list"."PresentationKind" AS "PresentationKind",
                  "item"."DisplayTitle" AS "Name", "item"."ProviderMetadataJson",
                  "item"."MetadataOverrideJson"
           FROM "AddonMediaListItem" AS "item"
           JOIN "AddonMediaList" AS "list" ON "list"."Id" = "item"."AddonMediaListId"
           WHERE "list"."ReviewState" = 2 AND "item"."Published" = 1"#,
    )
    .fetch_all(pool)
    .await
    .map_err(sqlx_error)
}

fn merged_metadata(
    provider_json: &str,
    override_json: &str,
) -> std::result::Result<ersatzrs_core::repositories::AddonMediaListNfoMetadata, serde_json::Error>
{
    let mut provider: serde_json::Value = serde_json::from_str(provider_json)?;
    let overrides: serde_json::Value = serde_json::from_str(override_json)?;
    if let (Some(provider), Some(overrides)) = (provider.as_object_mut(), overrides.as_object()) {
        for (field, value) in overrides {
            provider.insert(field.clone(), value.clone());
        }
    }
    serde_json::from_value(provider)
}

async fn replacement_failure(
    pool: &Pool<Db>,
    title: &'static str,
) -> Result<Option<HealthCheckResult>> {
    let missing = count_missing_replacements(pool).await?;
    let stale = count_stale_replacements(pool).await?;
    Ok((stale > 0 || missing > 0).then(|| {
        HealthCheckResult::new(
            title,
            HealthCheckStatus::Warning,
            format!(
                "{missing} unavailable item(s) have no replacement; {stale} replacement mapping(s) are stale"
            ),
            "One or more add-on list gaps need operator attention",
        )
        .with_link("/media/addon-lists")
    }))
}

fn presentation_failures(rows: &[crate::db::DbRow]) -> Vec<String> {
    rows.iter()
        .filter_map(|row| {
            let error = row
                .try_get::<Option<String>, _>("PresentationError")
                .ok()
                .flatten()?;
            let name = row
                .try_get::<String, _>("Name")
                .unwrap_or_else(|_| "List".to_owned());
            Some(format!("{name}: television projection failed: {error}"))
        })
        .take(5)
        .collect()
}

fn publication_failures(rows: &[crate::db::DbRow]) -> Vec<String> {
    rows.iter()
        .filter_map(|row| {
            let error = row
                .try_get::<Option<String>, _>("ReviewError")
                .ok()
                .flatten()?;
            let name = row
                .try_get::<String, _>("Name")
                .unwrap_or_else(|_| "List".to_owned());
            Some(format!("{name}: publication failed: {error}"))
        })
        .take(5)
        .collect()
}

const PUBLICATION_COORDINATOR_HEALTH_SQL: &str = r#"SELECT "job"."OperationId" AS "CorrelationId",
                  "job"."Generation", "job"."Phase", "job"."OriginalFailureCode",
                  "job"."UpdatedAt" AS "LastProgressAt",
                  "operation"."State", "operation"."AttemptCount",
                  "operation"."DiagnosticCode",
                  COALESCE("history"."EventCount", 0) AS "EventCount",
                  COALESCE("history"."OriginalFailureCount", 0) AS "OriginalFailureCount",
                  COALESCE("history"."RecoveryFailureCount", 0) AS "RecoveryFailureCount",
                  COALESCE("history"."HistoryTruncated", 0) AS "HistoryTruncated",
                  (
                      SELECT "recent_error"."ErrorCategory"
                      FROM "AddonMediaListPublicationEvent" AS "recent_error"
                      WHERE "recent_error"."PublicationJobId" = "job"."Id"
                        AND "recent_error"."ErrorCategory" IS NOT NULL
                      ORDER BY "recent_error"."Sequence" DESC
                      LIMIT 1
                  ) AS "ErrorCategory",
                  (
                      SELECT "recent_error"."OsErrorCode"
                      FROM "AddonMediaListPublicationEvent" AS "recent_error"
                      WHERE "recent_error"."PublicationJobId" = "job"."Id"
                        AND "recent_error"."ErrorCategory" IS NOT NULL
                      ORDER BY "recent_error"."Sequence" DESC
                      LIMIT 1
                  ) AS "OsErrorCode",
                  EXISTS (
                      SELECT 1 FROM "ManagedLibraryLease" AS "lease"
                      WHERE "lease"."OperationId" = "operation"."Id"
                        AND "lease"."OwnerToken" = "operation"."OwnerToken"
                        AND "lease"."LeaseExpiresAt" > ?
                  ) AS "LiveLease"
           FROM "AddonMediaListPublicationJob" AS "job"
           JOIN "ManagedLibraryOperation" AS "operation"
             ON "operation"."Id" = "job"."OperationId"
           LEFT JOIN (
               SELECT "event"."PublicationJobId", COUNT(*) AS "EventCount",
                      SUM(CASE WHEN "event"."Kind" = ? THEN 1 ELSE 0 END)
                          AS "OriginalFailureCount",
                      SUM(CASE WHEN "event"."Kind" = ? THEN 1 ELSE 0 END)
                          AS "RecoveryFailureCount",
                      MAX(CASE WHEN "event"."Kind" = ? THEN 1 ELSE 0 END)
                          AS "HistoryTruncated"
               FROM "AddonMediaListPublicationEvent" AS "event"
               GROUP BY "event"."PublicationJobId"
           ) AS "history" ON "history"."PublicationJobId" = "job"."Id"
           WHERE "job"."Generation" = (
                     SELECT MAX("latest"."Generation")
                     FROM "AddonMediaListPublicationJob" AS "latest"
                     WHERE "latest"."AddonMediaListId" = "job"."AddonMediaListId"
                 )
             AND ("operation"."State" <> 3 OR "job"."Phase" <> 7)
           ORDER BY "job"."UpdatedAt" DESC, "job"."Id" DESC"#;

async fn publication_coordinator_health(pool: &Pool<Db>) -> Result<PublicationCoordinatorHealth> {
    let now = crate::repositories::common::encode_datetime(chrono::Utc::now());
    let rows = sqlx::query(PUBLICATION_COORDINATOR_HEALTH_SQL)
        .bind(&now)
        .bind(crate::repositories::common::encode_enum(
            AddonMediaListPublicationEventKind::OriginalFailure,
        ))
        .bind(crate::repositories::common::encode_enum(
            AddonMediaListPublicationEventKind::RecoveryFailure,
        ))
        .bind(crate::repositories::common::encode_enum(
            AddonMediaListPublicationEventKind::HistoryTruncated,
        ))
        .fetch_all(pool)
        .await
        .map_err(sqlx_error)?;
    let mut status = PublicationCoordinatorHealth::default();
    for row in rows.into_iter().take(5) {
        let state: i64 = row.try_get("State").map_err(sqlx_error)?;
        let phase: i64 = row.try_get("Phase").map_err(sqlx_error)?;
        let live_lease: bool = row.try_get("LiveLease").map_err(sqlx_error)?;
        let visible_state = match state {
            0 => "queued",
            1 if live_lease && matches!(phase, 4..=6) => "restoring",
            1 if live_lease => "publishing",
            1 => "expired",
            2 => "waiting",
            4 => "failed",
            _ => continue,
        };
        let last_progress: String = row.try_get("LastProgressAt").map_err(sqlx_error)?;
        let attempts: i64 = row.try_get("AttemptCount").map_err(sqlx_error)?;
        let generation: i64 = row.try_get("Generation").map_err(sqlx_error)?;
        let correlation_id: i64 = row.try_get("CorrelationId").map_err(sqlx_error)?;
        let original: Option<String> = row.try_get("OriginalFailureCode").map_err(sqlx_error)?;
        let operation: Option<String> = row.try_get("DiagnosticCode").map_err(sqlx_error)?;
        let reason = original.or(operation).map_or_else(
            || match visible_state {
                "expired" => "lease_expired".to_owned(),
                "waiting" => "waiting_to_retry".to_owned(),
                "failed" => "publication_failed".to_owned(),
                _ => "none".to_owned(),
            },
            |value| redacted_publication_code(&value),
        );
        let history = publication_history_health_summary(&row, correlation_id)?;
        if matches!(visible_state, "failed" | "expired" | "waiting") {
            status.failures.push(format!(
                "publication generation {generation} {visible_state} in {} after {attempts} attempt(s); last progress {last_progress}; original reason {reason}{history}",
                publication_phase_name(phase)
            ));
        } else {
            status.active.push(format!(
                "publication generation {generation} {visible_state} in {} after {attempts} attempt(s); last progress {last_progress}{history}",
                publication_phase_name(phase)
            ));
        }
    }
    Ok(status)
}

fn publication_history_health_summary(
    row: &crate::db::DbRow,
    correlation_id: i64,
) -> Result<String> {
    let event_count: i64 = row.try_get("EventCount").map_err(sqlx_error)?;
    if event_count == 0 {
        return Ok(String::new());
    }
    let original_failures: i64 = row.try_get("OriginalFailureCount").map_err(sqlx_error)?;
    let recovery_failures: i64 = row.try_get("RecoveryFailureCount").map_err(sqlx_error)?;
    let truncated: i64 = row.try_get("HistoryTruncated").map_err(sqlx_error)?;
    let error_category: Option<i64> = row.try_get("ErrorCategory").map_err(sqlx_error)?;
    let os_error_code: Option<i64> = row.try_get("OsErrorCode").map_err(sqlx_error)?;
    let truncation = if truncated == 0 { "" } else { ", truncated" };
    let last_error = error_category.map_or_else(String::new, |category| {
        os_error_code.map_or_else(
            || format!("; last error {}", publication_error_category_name(category)),
            |code| {
                format!(
                    "; last error {} (OS code {code})",
                    publication_error_category_name(category)
                )
            },
        )
    });
    Ok(format!(
        "; correlation {correlation_id} has {event_count} event(s), {original_failures} original failure(s), {recovery_failures} recovery failure(s){truncation}{last_error}"
    ))
}

fn publication_phase_name(phase: i64) -> &'static str {
    match phase {
        0 => "awaiting lease",
        1 => "staging",
        2 => "scanning",
        3 => "committing",
        4 => "restoring files",
        5 => "restoring projection",
        6 => "reconciling",
        7 => "complete",
        8 => "verifying",
        9 => "cleaning up",
        _ => "unknown phase",
    }
}

fn redacted_publication_code(value: &str) -> String {
    let normalized = value.trim().to_ascii_lowercase();
    if !normalized.is_empty()
        && normalized.len() <= 96
        && normalized.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
    {
        normalized
    } else {
        "publication_failed".to_owned()
    }
}

const fn publication_error_category_name(category: i64) -> &'static str {
    match category {
        0 => "filesystem I/O",
        1 => "permission denied",
        2 => "sharing violation",
        3 => "unsafe filesystem state",
        4 => "invalid publication marker",
        5 => "publication integrity",
        _ => "unknown",
    }
}

async fn count_missing_replacements(pool: &Pool<Db>) -> Result<i64> {
    sqlx::query_scalar(
        r#"SELECT COUNT(*)
           FROM "AddonMediaListItem" AS "item"
           LEFT JOIN "AddonMediaListItemReplacement" AS "replacement"
             ON "replacement"."AddonMediaListItemId" = "item"."Id"
           WHERE "item"."Availability" = 1
             AND ("item"."AvailabilityReasonCode" IS NULL
                  OR "item"."AvailabilityReasonCode" = 2)
             AND "replacement"."AddonMediaListItemId" IS NULL"#,
    )
    .fetch_one(pool)
    .await
    .map_err(sqlx_error)
}

async fn count_stale_replacements(pool: &Pool<Db>) -> Result<i64> {
    let replacements = sqlx::query(
        r#"SELECT "MediaItemId", "MediaVersionId"
           FROM "AddonMediaListItemReplacement""#,
    )
    .fetch_all(pool)
    .await
    .map_err(sqlx_error)?;
    let mut stale = 0_i64;
    for replacement in replacements {
        let media_item_id = replacement.try_get("MediaItemId").map_err(sqlx_error)?;
        let media_version_id = replacement.try_get("MediaVersionId").map_err(sqlx_error)?;
        let valid =
            crate::repositories::load_replacement_candidate(pool, media_item_id, media_version_id)
                .await
                .map_err(|error| HealthCheckError::Database(error.to_string()))?
                .is_some();
        if !valid {
            stale += 1;
        }
    }
    Ok(stale)
}

#[cfg(test)]
mod tests {
    use sqlx::sqlite::SqlitePoolOptions;

    use super::*;

    #[allow(clippy::too_many_lines)]
    async fn fixture_pool() -> Pool<Db> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        sqlx::raw_sql(
            r#"
            CREATE TABLE "AddonInstallation" (
                "AddonId" TEXT PRIMARY KEY,
                "Enabled" INTEGER,
                "ManifestJson" TEXT NULL
            );
            CREATE TABLE "AddonMediaList" (
                "Id" INTEGER PRIMARY KEY,
                "AddonId" TEXT NOT NULL,
                "Name" TEXT NOT NULL,
                "RefreshCadence" INTEGER NOT NULL DEFAULT 4,
                "RefreshPublicationPolicy" INTEGER NOT NULL DEFAULT 0,
                "NextRefreshAt" TEXT NULL,
                "LastError" TEXT NULL,
                "PresentationError" TEXT NULL,
                "ReviewState" INTEGER NOT NULL DEFAULT 2,
                "ReviewError" TEXT NULL,
                "PublicationStartedAt" TEXT NULL,
                "PresentationKind" INTEGER NOT NULL DEFAULT 1,
                "ProviderMetadataJson" TEXT NOT NULL DEFAULT '{}',
                "MetadataOverrideJson" TEXT NOT NULL DEFAULT '{}',
                "MetadataConflictJson" TEXT NULL
            );
            CREATE TABLE "AddonMediaListItem" (
                "Id" INTEGER PRIMARY KEY,
                "AddonMediaListId" INTEGER NOT NULL,
                "DisplayTitle" TEXT NOT NULL DEFAULT '',
                "Included" INTEGER NOT NULL DEFAULT 1,
                "Published" INTEGER NOT NULL DEFAULT 0,
                "Availability" INTEGER NOT NULL DEFAULT 0,
                "AvailabilityReasonCode" INTEGER NULL,
                "ProviderMetadataJson" TEXT NOT NULL DEFAULT '{}',
                "MetadataOverrideJson" TEXT NOT NULL DEFAULT '{}',
                "MetadataConflictJson" TEXT NULL
            );
            CREATE TABLE "AddonMediaListItemReplacement" (
                "AddonMediaListItemId" INTEGER PRIMARY KEY,
                "MediaItemId" INTEGER NOT NULL,
                "MediaVersionId" INTEGER NOT NULL
            );
            CREATE TABLE "AddonMediaListArtworkMaterialization" (
                "AddonMediaListId" INTEGER NOT NULL,
                "ScopeItemId" INTEGER NOT NULL,
                "Role" INTEGER NOT NULL
            );
            CREATE TABLE "AddonMediaSource" (
                "AddonId" TEXT PRIMARY KEY,
                "MediaSourceId" INTEGER NOT NULL
            );
            CREATE TABLE "AddonMediaListLibrary" (
                "AddonMediaListId" INTEGER PRIMARY KEY,
                "LibraryId" INTEGER NOT NULL,
                "ManifestPath" TEXT NOT NULL
            );
            CREATE TABLE "AddonMediaListImportJob" (
                "Id" INTEGER PRIMARY KEY,
                "AddonMediaListId" INTEGER NOT NULL,
                "Generation" INTEGER NOT NULL,
                "State" INTEGER NOT NULL,
                "Mode" INTEGER NOT NULL DEFAULT 0,
                "DiscoveredCount" INTEGER NOT NULL DEFAULT 0,
                "Diagnostic" TEXT NULL,
                "LastProgressAt" TEXT NULL,
                "UpdatedAt" TEXT NOT NULL
            );
            CREATE TABLE "AddonMediaListDetailJob" (
                "AddonMediaListItemId" INTEGER PRIMARY KEY,
                "AddonMediaListId" INTEGER NOT NULL,
                "State" INTEGER NOT NULL,
                "UpdatedAt" TEXT NOT NULL,
                "LeaseExpiresAt" TEXT NULL
            );
            CREATE TABLE "ManagedLibraryOperation" (
                "Id" INTEGER PRIMARY KEY,
                "State" INTEGER NOT NULL,
                "OwnerToken" TEXT NOT NULL DEFAULT 'owner',
                "LeaseExpiresAt" TEXT NULL,
                "AttemptCount" INTEGER NOT NULL DEFAULT 0,
                "DiagnosticCode" TEXT NULL
            );
            CREATE TABLE "ManagedLibraryLease" (
                "OperationId" INTEGER PRIMARY KEY,
                "OwnerToken" TEXT NOT NULL DEFAULT 'owner',
                "LeaseExpiresAt" TEXT NOT NULL
            );
            CREATE TABLE "AddonMediaListPublicationJob" (
                "Id" INTEGER PRIMARY KEY,
                "OperationId" INTEGER NOT NULL,
                "AddonMediaListId" INTEGER NOT NULL DEFAULT 1,
                "Generation" INTEGER NOT NULL DEFAULT 1,
                "Phase" INTEGER NOT NULL,
                "OriginalFailureCode" TEXT NULL,
                "OriginalFailureDiagnostic" TEXT NULL,
                "RecoveryDiagnostic" TEXT NULL,
                "Outcome" INTEGER NULL,
                "UpdatedAt" TEXT NOT NULL DEFAULT '2026-09-03T00:00:00Z'
            );
            CREATE TABLE "AddonMediaListPublicationEvent" (
                "Id" INTEGER PRIMARY KEY,
                "PublicationJobId" INTEGER NOT NULL,
                "Sequence" INTEGER NOT NULL,
                "DedupeKey" TEXT NOT NULL,
                "Kind" INTEGER NOT NULL,
                "Phase" INTEGER NULL,
                "Attempt" INTEGER NULL,
                "EnqueueTrigger" INTEGER NULL,
                "RelatedOperationId" INTEGER NULL,
                "FilesystemTransition" INTEGER NULL,
                "RetryClass" INTEGER NULL,
                "RecoveryAction" INTEGER NULL,
                "ErrorCategory" INTEGER NULL,
                "OsErrorCode" INTEGER NULL,
                "PublishedItemCount" INTEGER NULL,
                "PlaylistItemCount" INTEGER NULL,
                "ArtworkCount" INTEGER NULL,
                "Outcome" INTEGER NULL,
                "OccurredAt" TEXT NOT NULL
            );
            CREATE TABLE "AddonMediaListLifecycleJob" (
                "Id" INTEGER PRIMARY KEY,
                "OperationId" INTEGER NOT NULL
            );
            CREATE TABLE "AddonMediaListImportCheckpoint" (
                "Id" INTEGER PRIMARY KEY
            );
            CREATE TABLE "AddonMediaListRevision" (
                "Id" INTEGER PRIMARY KEY,
                "SourceDraftRevision" INTEGER NULL,
                "OptionValuesJson" TEXT NOT NULL DEFAULT '{}',
                "CompletedAt" TEXT NULL,
                "SnapshotVersion" INTEGER NULL,
                "SnapshotDigest" TEXT NULL,
                "Name" TEXT NULL,
                "GeneratePlaylist" INTEGER NULL,
                "ContentKindOverride" INTEGER NULL,
                "PresentationKind" INTEGER NULL,
                "RequestedPresentationKind" INTEGER NULL,
                "MetadataJson" TEXT NULL
            );
            CREATE TABLE "AddonMediaListRevisionFileBlob" (
                "Id" INTEGER PRIMARY KEY,
                "AddonMediaListId" INTEGER NOT NULL,
                "StorageRelativePath" TEXT NOT NULL,
                "Sha256" TEXT NOT NULL,
                "ByteLength" INTEGER NOT NULL
            );
            CREATE TABLE "AddonMediaListRevisionFile" (
                "AddonMediaListRevisionId" INTEGER NOT NULL,
                "AddonMediaListRevisionFileBlobId" INTEGER NOT NULL,
                "PublishedRelativePath" TEXT NOT NULL
            );
            CREATE TABLE "AddonMediaListItemTombstone" (
                "AddonMediaListItemId" INTEGER PRIMARY KEY
            );
            CREATE TABLE "AddonMediaListArtifact" (
                "Id" INTEGER PRIMARY KEY,
                "Kind" INTEGER NOT NULL DEFAULT 0,
                "PreparationJobId" INTEGER NULL,
                "PreparationGeneration" INTEGER NULL,
                "PreparationLeaseOwner" TEXT NULL,
                "State" INTEGER NOT NULL,
                "ReservedBytes" INTEGER NOT NULL DEFAULT 0,
                "ByteLength" INTEGER NULL,
                "CacheKey" TEXT NULL,
                "RelativePath" TEXT NULL,
                "Sha256" TEXT NULL,
                "DiagnosticCode" TEXT NULL
            );
            CREATE TABLE "AddonMediaListItemOperationJob" (
                "Id" INTEGER PRIMARY KEY,
                "AddonMediaListId" INTEGER NOT NULL DEFAULT 1,
                "AddonMediaListItemId" INTEGER NOT NULL DEFAULT 1,
                "Kind" INTEGER NOT NULL DEFAULT 1,
                "State" INTEGER NOT NULL,
                "Generation" INTEGER NOT NULL DEFAULT 1,
                "LeaseOwner" TEXT NULL,
                "LeaseExpiresAt" TEXT NULL,
                "OutcomeJson" TEXT NULL
            );
            CREATE TABLE "AddonToolMaintenanceJob" (
                "Id" INTEGER PRIMARY KEY,
                "AddonId" TEXT NOT NULL DEFAULT 'org.example.tool',
                "ToolKey" TEXT NOT NULL DEFAULT 'primary',
                "State" INTEGER NOT NULL,
                "LeaseOwner" TEXT NULL,
                "LeaseExpiresAt" TEXT NULL,
                "Outcome" INTEGER NULL,
                "BeforeVersion" TEXT NULL,
                "BeforeSha256" TEXT NULL,
                "AfterVersion" TEXT NULL,
                "AfterSha256" TEXT NULL,
                "DiagnosticCode" TEXT NULL,
                "Diagnostic" TEXT NULL,
                "CreatedAt" TEXT NOT NULL DEFAULT '2026-09-04T00:00:00Z'
            );
            CREATE TABLE "AddonProviderHostCooldown" (
                "ProviderHost" TEXT PRIMARY KEY,
                "SampledDelaySeconds" INTEGER NOT NULL
            );
            CREATE TABLE "AddonMediaListRefreshState" (
                "AddonMediaListId" INTEGER PRIMARY KEY,
                "Generation" INTEGER NOT NULL DEFAULT 0,
                "State" INTEGER NOT NULL DEFAULT 0,
                "Validator" TEXT NULL,
                "MissCount" INTEGER NOT NULL DEFAULT 0,
                "AttemptCount" INTEGER NOT NULL DEFAULT 0,
                "LeaseOwner" TEXT NULL,
                "LeaseExpiresAt" TEXT NULL,
                "NextEligibleAt" TEXT NULL,
                "LastOutcome" INTEGER NULL,
                "DiagnosticCode" TEXT NULL,
                "UpdatedAt" TEXT NOT NULL DEFAULT '2026-09-04T00:00:00Z'
            );
            CREATE TABLE "ConfigElement" (
                "Key" TEXT PRIMARY KEY,
                "Value" TEXT NOT NULL
            );
            CREATE TABLE "AddonMediaListAutoPublication" (
                "AddonMediaListId" INTEGER PRIMARY KEY,
                "State" TEXT NOT NULL,
                "RecordedAt" TEXT NOT NULL
            );
            INSERT INTO "AddonInstallation" ("AddonId", "Enabled") VALUES ('org.example.list', 1);
            "#,
        )
        .execute(&pool)
        .await
        .expect("fixture");
        pool
    }

    fn manifest_with_source_check(enabled: bool) -> String {
        let capabilities = if enabled {
            serde_json::json!([{"id": CAPABILITY_MEDIA_LIST_SOURCE_CHECK_V1}])
        } else {
            serde_json::json!([])
        };
        serde_json::json!({
            "manifest_version": 1,
            "id": "org.example.list",
            "version": "1.0.0",
            "name": {"en-US": "Fixture"},
            "summary": {"en-US": "Fixture"},
            "license": "MIT",
            "source_url": "https://example.test/source",
            "host_version": "*",
            "capabilities": capabilities,
            "entrypoints": {},
            "permissions": {},
            "settings": [],
            "panels": []
        })
        .to_string()
    }

    async fn insert_adaptive_list(pool: &Pool<Db>, addon_id: &str) {
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" (
                    "Id", "AddonId", "Name", "RefreshCadence", "ProviderMetadataJson"
               ) VALUES (1, ?, 'Adaptive fixture', ?, '{"title":"Adaptive fixture"}')"#,
        )
        .bind(addon_id)
        .bind(crate::repositories::common::encode_enum(
            AddonMediaListRefreshCadence::Adaptive,
        ))
        .execute(pool)
        .await
        .expect("adaptive list");
    }

    async fn set_fixture_addon(pool: &Pool<Db>, enabled: bool, source_check: bool) {
        sqlx::query(
            r#"UPDATE "AddonInstallation"
                  SET "Enabled" = ?, "ManifestJson" = ?
                WHERE "AddonId" = 'org.example.list'"#,
        )
        .bind(enabled)
        .bind(manifest_with_source_check(source_check))
        .execute(pool)
        .await
        .expect("fixture add-on");
    }

    #[tokio::test]
    async fn preparation_worker_unavailable_health_is_distinct_from_process_recovery() {
        let pool = fixture_pool().await;
        crate::addon_preparation_worker::set_worker_unavailable(&pool, true)
            .await
            .unwrap();
        let result = AddonMediaListHealthCheck::new(pool.clone())
            .check()
            .await
            .unwrap();
        assert_eq!(result.status, HealthCheckStatus::Fail);
        assert_eq!(result.message, "Media preparation worker is unavailable. Restart the server before retrying preparation.");
        assert!(preparation_recovery_failure(&pool, "Add-on media lists")
            .await
            .unwrap()
            .is_none());
        crate::addon_preparation_worker::set_worker_unavailable(&pool, false)
            .await
            .unwrap();
        assert!(!AddonMediaListHealthCheck::new(pool)
            .check()
            .await
            .unwrap()
            .message
            .contains("worker is unavailable"));
    }

    #[tokio::test]
    async fn preparation_recovery_health_preserves_live_work_and_redacts_stale_reservations() {
        let pool = fixture_pool().await;
        sqlx::raw_sql(
            r#"INSERT INTO "AddonMediaListItemOperationJob"
               ("Id", "State", "LeaseOwner", "LeaseExpiresAt")
               VALUES (1, 1, 'private-lease-owner', '2999-01-01T00:00:00Z');
               INSERT INTO "AddonMediaListArtifact"
               ("Id", "Kind", "State", "ReservedBytes", "PreparationJobId",
                "PreparationGeneration", "PreparationLeaseOwner", "RelativePath")
               VALUES (1, 1, 1, 4096, 1, 1, 'private-lease-owner', 'private/staging.part');"#,
        )
        .execute(&pool)
        .await
        .expect("live reservation");
        assert!(preparation_recovery_failure(&pool, "Add-on media lists")
            .await
            .expect("live health")
            .is_none());

        // An expired lease and a reclaimed generation both require recovery;
        // neither proves that the old provider's descendants have stopped.
        for update in [
            r#"UPDATE "AddonMediaListItemOperationJob" SET "LeaseExpiresAt" = '2000-01-01T00:00:00Z'"#,
            r#"UPDATE "AddonMediaListItemOperationJob" SET "LeaseExpiresAt" = '2999-01-01T00:00:00Z', "Generation" = 2"#,
        ] {
            sqlx::query(update)
                .execute(&pool)
                .await
                .expect("stale owner");
            let result = AddonMediaListHealthCheck::new(pool.clone())
                .check()
                .await
                .expect("recovery health");
            assert_eq!(result.status, HealthCheckStatus::Fail);
            assert!(result
                .message
                .contains("Confirm prior provider processes have stopped"));
            for private in ["private-lease-owner", "private/staging.part"] {
                assert!(!result.message.contains(private));
                assert!(!result.brief_message.contains(private));
            }
            let reserved: i64 = sqlx::query_scalar(
                r#"SELECT "ReservedBytes" FROM "AddonMediaListArtifact" WHERE "Id" = 1"#,
            )
            .fetch_one(&pool)
            .await
            .expect("retained reservation");
            assert_eq!(reserved, 4096);
        }
    }

    #[tokio::test]
    async fn preparation_recovery_health_reports_only_current_failure() {
        let pool = fixture_pool().await;
        sqlx::query(
            r#"INSERT INTO "AddonMediaListItemOperationJob" ("Id", "State", "OutcomeJson")
               VALUES (1, 3, '{"Failed":"ProcessRecoveryRequired"}')"#,
        )
        .execute(&pool)
        .await
        .expect("recovery outcome");
        let result = AddonMediaListHealthCheck::new(pool.clone())
            .check()
            .await
            .expect("failed health without lists");
        assert_eq!(result.status, HealthCheckStatus::Fail);
        sqlx::query(
            r#"INSERT INTO "AddonMediaListItemOperationJob" ("Id", "State") VALUES (2, 2)"#,
        )
        .execute(&pool)
        .await
        .expect("newer completion");
        assert!(preparation_recovery_failure(&pool, "Add-on media lists")
            .await
            .expect("historical failure")
            .is_none());
    }

    #[tokio::test]
    async fn subtitle_recovery_health_tracks_exact_live_owner_and_preserves_reservation() {
        let pool = fixture_pool().await;
        sqlx::raw_sql(
            r#"INSERT INTO "AddonMediaListItemOperationJob"
               ("Id","Kind","State","LeaseOwner","LeaseExpiresAt")
               VALUES (1,3,1,'private-subtitle-owner','2999-01-01T00:00:00Z');
               INSERT INTO "AddonMediaListArtifact"
               ("Id","Kind","State","ReservedBytes","PreparationJobId",
                "PreparationGeneration","PreparationLeaseOwner","RelativePath")
               VALUES (1,0,1,4096,1,1,'private-subtitle-owner','private/manual.srt');"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(preparation_recovery_failure(&pool, "Add-on media lists")
            .await
            .unwrap()
            .is_none());
        for update in [
            r#"UPDATE "AddonMediaListItemOperationJob" SET "Kind"=1"#,
            r#"UPDATE "AddonMediaListItemOperationJob" SET "Kind"=3,"Generation"=2"#,
            r#"UPDATE "AddonMediaListItemOperationJob" SET "Generation"=1,"LeaseExpiresAt"='2000-01-01T00:00:00Z'"#,
            r#"UPDATE "AddonMediaListItemOperationJob" SET "LeaseExpiresAt"='2999-01-01T00:00:00Z',"LeaseOwner"='other-owner'"#,
        ] {
            sqlx::query(update).execute(&pool).await.unwrap();
            let result = preparation_recovery_failure(&pool, "Add-on media lists")
                .await
                .unwrap()
                .unwrap();
            assert_eq!(result.status, HealthCheckStatus::Fail);
            for private in ["private-subtitle-owner", "private/manual.srt"] {
                assert!(!result.message.contains(private));
                assert!(!result.brief_message.contains(private));
            }
            let reserved: i64 = sqlx::query_scalar(
                r#"SELECT "ReservedBytes" FROM "AddonMediaListArtifact" WHERE "Id"=1"#,
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(reserved, 4096);
        }
        sqlx::query(r#"UPDATE "AddonMediaListArtifact" SET "State"=3,"ReservedBytes"=0"#)
            .execute(&pool)
            .await
            .unwrap();
        assert!(preparation_recovery_failure(&pool, "Add-on media lists")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn subtitle_recovery_health_requires_same_item_and_kind_supersession() {
        let pool = fixture_pool().await;
        sqlx::raw_sql(
            r#"INSERT INTO "AddonMediaListItemOperationJob" ("Id","Kind","State","OutcomeJson")
               VALUES (1,3,3,'{"Failed":"ProcessRecoveryRequired"}');
               INSERT INTO "AddonMediaListItemOperationJob" ("Id","Kind","State") VALUES (2,1,2);
               INSERT INTO "AddonMediaListItemOperationJob" ("Id","Kind","State","AddonMediaListItemId") VALUES (3,3,2,99);"#,
        ).execute(&pool).await.unwrap();
        assert!(preparation_recovery_failure(&pool, "Add-on media lists")
            .await
            .unwrap()
            .is_some());
        sqlx::query(
            r#"INSERT INTO "AddonMediaListItemOperationJob" ("Id","Kind","State") VALUES (4,3,2)"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(preparation_recovery_failure(&pool, "Add-on media lists")
            .await
            .unwrap()
            .is_none());
    }

    async fn check_tool_without_lists(
        state: i64,
        lease_owner: Option<&str>,
        lease_expires_at: Option<&str>,
        outcome: Option<i64>,
    ) -> HealthCheckResult {
        let pool = fixture_pool().await;
        sqlx::query(
            r#"INSERT INTO "AddonToolMaintenanceJob" (
                    "Id", "State", "LeaseOwner", "LeaseExpiresAt", "Outcome", "Diagnostic"
               ) VALUES (1, ?, ?, ?, ?, 'private tool output')"#,
        )
        .bind(state)
        .bind(lease_owner)
        .bind(lease_expires_at)
        .bind(outcome)
        .execute(&pool)
        .await
        .expect("tool maintenance job");

        AddonMediaListHealthCheck::new(pool)
            .check()
            .await
            .expect("health result")
    }

    #[tokio::test]
    async fn no_list_active_tool_maintenance_reports_aggregate_pass() {
        let result = check_tool_without_lists(0, None, None, None).await;

        assert_eq!(result.status, HealthCheckStatus::Pass);
        assert!(result
            .message
            .contains("tool maintenance active=1 waiting=0 expired=0 failed=0"));
        assert_eq!(
            result.brief_message,
            "Add-on lifecycle or tool maintenance is active or retained"
        );
        assert_eq!(
            result.link.as_ref().map(|link| link.link.as_str()),
            Some("/settings/addons")
        );
        assert!(!result.message.contains("private tool output"));
    }

    #[tokio::test]
    async fn no_list_expired_tool_maintenance_reports_aggregate_failure() {
        let result = check_tool_without_lists(
            1,
            Some("private-worker"),
            Some("2000-01-01T00:00:00Z"),
            None,
        )
        .await;

        assert_eq!(result.status, HealthCheckStatus::Fail);
        assert!(result
            .message
            .contains("1 tool maintenance job(s) require recovery (1 expired, 0 failed)"));
        assert_eq!(
            result.brief_message,
            "One or more add-on list or tool-maintenance operations need attention"
        );
        assert_eq!(
            result.link.as_ref().map(|link| link.link.as_str()),
            Some("/settings/addons")
        );
        assert!(!result.message.contains("private-worker"));
        assert!(!result.message.contains("private tool output"));
    }

    #[tokio::test]
    async fn no_list_failed_tool_maintenance_reports_aggregate_failure() {
        let result = check_tool_without_lists(4, None, None, Some(4)).await;

        assert_eq!(result.status, HealthCheckStatus::Fail);
        assert!(result
            .message
            .contains("1 tool maintenance job(s) require recovery (0 expired, 1 failed)"));
        assert!(result
            .message
            .contains("tool maintenance active=0 waiting=0 expired=0 failed=1"));
        assert_eq!(
            result.link.as_ref().map(|link| link.link.as_str()),
            Some("/settings/addons")
        );
        assert!(!result.message.contains("private tool output"));
    }

    #[tokio::test]
    async fn reports_persisted_refresh_failure() {
        let pool = fixture_pool().await;
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" ("Id", "AddonId", "Name", "LastError")
               VALUES (1, 'org.example.list', 'Fixture', 'provider unavailable')"#,
        )
        .execute(&pool)
        .await
        .expect("list");
        let result = AddonMediaListHealthCheck::new(pool)
            .check()
            .await
            .expect("health result");
        assert_eq!(result.status, HealthCheckStatus::Fail);
        assert!(result.message.contains("Fixture: provider unavailable"));
        assert_eq!(
            result.link.as_ref().map(|link| link.link.as_str()),
            Some("/media/addon-lists")
        );
    }

    #[tokio::test]
    async fn reports_missing_addon_for_adaptive_refresh() {
        let pool = fixture_pool().await;
        insert_adaptive_list(&pool, "org.example.missing").await;

        let result = AddonMediaListHealthCheck::new(pool)
            .check()
            .await
            .expect("health result");

        assert_eq!(result.status, HealthCheckStatus::Warning);
        assert!(result
            .message
            .contains("Adaptive fixture: adaptive refresh add-on is missing"));
    }

    #[tokio::test]
    async fn reports_disabled_addon_for_adaptive_refresh() {
        let pool = fixture_pool().await;
        set_fixture_addon(&pool, false, true).await;
        insert_adaptive_list(&pool, "org.example.list").await;

        let result = AddonMediaListHealthCheck::new(pool)
            .check()
            .await
            .expect("health result");

        assert_eq!(result.status, HealthCheckStatus::Warning);
        assert!(result
            .message
            .contains("Adaptive fixture: adaptive refresh add-on is disabled"));
    }

    #[tokio::test]
    async fn reports_lost_source_check_capability_for_adaptive_refresh() {
        let pool = fixture_pool().await;
        set_fixture_addon(&pool, true, false).await;
        insert_adaptive_list(&pool, "org.example.list").await;
        sqlx::query(
            r#"INSERT INTO "AddonMediaListRefreshState" (
                    "AddonMediaListId", "State", "DiagnosticCode"
               ) VALUES (1, ?, 'capability-unavailable')"#,
        )
        .bind(crate::repositories::common::encode_enum(
            AddonMediaListRefreshState::Failed,
        ))
        .execute(&pool)
        .await
        .expect("persist lost-capability outcome");

        let result = AddonMediaListHealthCheck::new(pool)
            .check()
            .await
            .expect("health result");

        assert_eq!(result.status, HealthCheckStatus::Warning);
        assert!(result.message.contains(
            "Adaptive fixture: adaptive refresh add-on no longer declares media-list.source-check.v1"
        ));
    }

    #[tokio::test]
    async fn reports_terminal_source_check_failure_without_provider_diagnostics() {
        let pool = fixture_pool().await;
        set_fixture_addon(&pool, true, true).await;
        insert_adaptive_list(&pool, "org.example.list").await;
        sqlx::query(
            r#"INSERT INTO "AddonMediaListRefreshState" (
                    "AddonMediaListId", "State", "LastOutcome", "DiagnosticCode"
               ) VALUES (
                    1, ?, 3,
                    'https://provider.invalid/list?token=secret /private/list cookie=session-secret'
               )"#,
        )
        .bind(crate::repositories::common::encode_enum(
            AddonMediaListRefreshState::Failed,
        ))
        .execute(&pool)
        .await
        .expect("terminal source check");

        let result = AddonMediaListHealthCheck::new(pool)
            .check()
            .await
            .expect("health result");

        assert_eq!(result.status, HealthCheckStatus::Fail);
        assert!(result
            .message
            .contains("Adaptive fixture: automatic refresh stopped after a terminal failure"));
        for forbidden in [
            "provider.invalid",
            "token=secret",
            "/private/list",
            "cookie=session-secret",
        ] {
            assert!(!result.message.contains(forbidden));
        }
    }

    #[tokio::test]
    async fn reports_terminal_fixed_cadence_failure() {
        let pool = fixture_pool().await;
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" ("Id", "AddonId", "Name", "RefreshCadence")
               VALUES (1, 'org.example.list', 'Weekly fixture', ?)"#,
        )
        .bind(crate::repositories::common::encode_enum(
            AddonMediaListRefreshCadence::Weekly,
        ))
        .execute(&pool)
        .await
        .expect("weekly list");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListRefreshState" ("AddonMediaListId", "State")
               VALUES (1, ?)"#,
        )
        .bind(crate::repositories::common::encode_enum(
            AddonMediaListRefreshState::Failed,
        ))
        .execute(&pool)
        .await
        .expect("terminal refresh state");

        let result = AddonMediaListHealthCheck::new(pool)
            .check()
            .await
            .expect("health result");

        assert_eq!(result.status, HealthCheckStatus::Fail);
        assert!(result
            .message
            .contains("Weekly fixture: automatic refresh stopped after a terminal failure"));
    }

    #[tokio::test]
    async fn does_not_treat_an_overdue_adaptive_probe_as_a_health_failure() {
        let pool = fixture_pool().await;
        set_fixture_addon(&pool, true, true).await;
        insert_adaptive_list(&pool, "org.example.list").await;
        sqlx::query(
            r#"UPDATE "AddonMediaList"
                  SET "NextRefreshAt" = '2000-01-01T00:00:00Z'
                WHERE "Id" = 1"#,
        )
        .execute(&pool)
        .await
        .expect("overdue adaptive list");

        let result = AddonMediaListHealthCheck::new(pool)
            .check()
            .await
            .expect("health result");

        assert_eq!(result.status, HealthCheckStatus::Pass, "{}", result.message);
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)] // Keep related lifecycle fixtures and their aggregate health assertion together.
    async fn reports_safe_lifecycle_foundation_aggregates_without_warning() {
        let pool = fixture_pool().await;
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" (
                    "Id", "AddonId", "Name", "ProviderMetadataJson"
               ) VALUES (1, 'org.example.list', 'Fixture', '{"title":"Fixture"}')"#,
        )
        .execute(&pool)
        .await
        .expect("list");
        sqlx::query(r#"INSERT INTO "AddonMediaListImportCheckpoint" VALUES (1)"#)
            .execute(&pool)
            .await
            .expect("checkpoint");
        sqlx::query(r#"INSERT INTO "AddonMediaListRevision" ("Id") VALUES (1)"#)
            .execute(&pool)
            .await
            .expect("legacy revision");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListRevision" (
                    "Id", "SourceDraftRevision", "CompletedAt", "SnapshotVersion",
                    "SnapshotDigest", "Name", "GeneratePlaylist", "ContentKindOverride",
                    "PresentationKind", "RequestedPresentationKind", "MetadataJson"
               ) VALUES (
                    2, 1, '2026-09-04T00:00:00Z', 1,
                    '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef',
                    'Fixture', 1, 0, 1, 1, '{}'
               )"#,
        )
        .execute(&pool)
        .await
        .expect("complete revision snapshot");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListRevisionFileBlob" (
                    "Id", "AddonMediaListId", "StorageRelativePath", "Sha256", "ByteLength"
               ) VALUES (1, 1, 'revisions/blob.bin', 'blob-digest', 123)"#,
        )
        .execute(&pool)
        .await
        .expect("revision file blob");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListRevisionFile" (
                    "AddonMediaListRevisionId", "AddonMediaListRevisionFileBlobId",
                    "PublishedRelativePath"
               ) VALUES (2, 1, 'items/file.bin')"#,
        )
        .execute(&pool)
        .await
        .expect("revision file reference");
        sqlx::query(r#"INSERT INTO "AddonMediaListItemTombstone" VALUES (1)"#)
            .execute(&pool)
            .await
            .expect("tombstone");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListArtifact" (
                    "Id", "State", "ReservedBytes", "ByteLength", "CacheKey",
                    "RelativePath", "Sha256", "DiagnosticCode"
               ) VALUES (
                    1, 2, 8, 123, 'private-cache-key',
                    '/private/artifact.bin', 'private-artifact-digest', 'private-artifact-error'
               )"#,
        )
        .execute(&pool)
        .await
        .expect("artifact");
        sqlx::query(
            r#"INSERT INTO "AddonToolMaintenanceJob" (
                    "Id", "State", "Outcome", "BeforeVersion", "BeforeSha256",
                    "AfterVersion", "AfterSha256", "DiagnosticCode", "Diagnostic"
               ) VALUES (
                    1, 3, 0, 'private-before-version', 'private-before-digest',
                    'private-after-version', 'private-after-digest',
                    'private-tool-code', 'https://private.invalid/tool?token=secret'
               )"#,
        )
        .execute(&pool)
        .await
        .expect("tool maintenance");

        let result = AddonMediaListHealthCheck::new(pool)
            .check()
            .await
            .expect("health result");
        assert_eq!(result.status, HealthCheckStatus::Pass);
        assert!(result.message.contains("checkpoints=1"));
        assert!(result.message.contains(
            "retained revisions total=2 structurally_complete=1 legacy=1 structurally_invalid=0"
        ));
        assert!(result
            .message
            .contains("retained files references=1 blobs=1 invalid=0"));
        assert!(result.message.contains("tombstones=1"));
        assert!(result
            .message
            .contains("artifacts total=1 ready=1 failed=0 bytes=123 reserved_bytes=8"));
        assert!(result.message.contains("outcomes updated=1"));
        for forbidden in [
            "/private/artifact.bin",
            "private-artifact-digest",
            "private-before-version",
            "private-after-digest",
            "https://private.invalid/tool?token=secret",
        ] {
            assert!(!result.message.contains(forbidden));
        }
    }

    #[tokio::test]
    async fn reports_structurally_invalid_revision_snapshot_and_file_metadata() {
        let pool = fixture_pool().await;
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" (
                    "Id", "AddonId", "Name", "ProviderMetadataJson"
               ) VALUES (1, 'org.example.list', 'Fixture', '{"title":"Fixture"}')"#,
        )
        .execute(&pool)
        .await
        .expect("list");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListRevision" (
                    "Id", "CompletedAt", "SnapshotVersion", "SnapshotDigest", "Name",
                    "GeneratePlaylist", "PresentationKind", "RequestedPresentationKind",
                    "MetadataJson"
               ) VALUES (
                    1, '2026-09-04T00:00:00Z', 1,
                    '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef',
                    'Fixture', 1, 1, 1, '{}'
               )"#,
        )
        .execute(&pool)
        .await
        .expect("partial revision snapshot");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListRevisionFileBlob" (
                    "Id", "AddonMediaListId", "StorageRelativePath", "Sha256", "ByteLength"
               ) VALUES (1, 1, '', '', 0)"#,
        )
        .execute(&pool)
        .await
        .expect("invalid revision file blob");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListRevisionFile" (
                    "AddonMediaListRevisionId", "AddonMediaListRevisionFileBlobId",
                    "PublishedRelativePath"
               ) VALUES (1, 1, 'private-item-path')"#,
        )
        .execute(&pool)
        .await
        .expect("invalid revision file reference");

        let result = AddonMediaListHealthCheck::new(pool)
            .check()
            .await
            .expect("health result");

        assert_eq!(result.status, HealthCheckStatus::Fail);
        assert!(result
            .message
            .contains("1 retained revision snapshot(s) are structurally incomplete or invalid"));
        assert!(result
            .message
            .contains("2 retained revision file record(s) have invalid metadata or references"));
        assert!(result.message.contains(
            "retained revisions total=1 structurally_complete=0 legacy=0 structurally_invalid=1"
        ));
        assert!(result
            .message
            .contains("retained files references=1 blobs=1 invalid=2"));
        assert!(!result.message.contains("partial-private-digest"));
        assert!(!result.message.contains("private-item-path"));
    }

    #[tokio::test]
    async fn reports_only_actionable_lifecycle_foundation_failures() {
        let pool = fixture_pool().await;
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" (
                    "Id", "AddonId", "Name", "ProviderMetadataJson"
               ) VALUES (1, 'org.example.list', 'Fixture', '{"title":"Fixture"}')"#,
        )
        .execute(&pool)
        .await
        .expect("list");
        for (id, state) in [(1_i64, 0_i64), (2, 2), (3, 1), (4, 4)] {
            sqlx::query(
                r#"INSERT INTO "ManagedLibraryOperation" ("Id", "State", "OwnerToken")
                   VALUES (?, ?, 'owner')"#,
            )
            .bind(id)
            .bind(state)
            .execute(&pool)
            .await
            .expect("operation");
            sqlx::query(
                r#"INSERT INTO "AddonMediaListLifecycleJob" ("Id", "OperationId")
                   VALUES (?, ?)"#,
            )
            .bind(id)
            .bind(id)
            .execute(&pool)
            .await
            .expect("lifecycle job");
        }
        sqlx::query(
            r#"INSERT INTO "AddonMediaListArtifact" (
                    "Id", "State", "DiagnosticCode"
               ) VALUES (1, 3, 'private-artifact-failure')"#,
        )
        .execute(&pool)
        .await
        .expect("failed artifact");
        for (id, state, outcome) in [
            (1_i64, 2_i64, None),
            (2, 1, None),
            (3, 4, Some(4_i64)),
            (4, 3, Some(2_i64)),
        ] {
            sqlx::query(
                r#"INSERT INTO "AddonToolMaintenanceJob" (
                        "Id", "ToolKey", "State", "Outcome", "Diagnostic"
                   ) VALUES (?, ?, ?, ?, 'private-tool-diagnostic')"#,
            )
            .bind(id)
            .bind(format!("tool-{id}"))
            .bind(state)
            .bind(outcome)
            .execute(&pool)
            .await
            .expect("tool maintenance job");
        }

        let result = AddonMediaListHealthCheck::new(pool)
            .check()
            .await
            .expect("health result");
        assert_eq!(result.status, HealthCheckStatus::Fail);
        assert!(result
            .message
            .contains("2 lifecycle job(s) require recovery (1 expired, 1 failed)"));
        assert!(result
            .message
            .contains("2 tool maintenance job(s) require recovery (1 expired, 1 failed)"));
        assert!(result
            .message
            .contains("lifecycle active=1 waiting=1 expired=1 failed=1"));
        assert!(result
            .message
            .contains("artifacts total=1 ready=0 failed=1"));
        assert!(result
            .message
            .contains("tool maintenance active=0 waiting=1 expired=1 failed=1"));
        assert!(result.message.contains("manual_update_required=1"));
        assert_eq!(
            result.brief_message,
            "One or more add-on list or tool-maintenance operations need attention"
        );
        assert!(!result.message.contains("private-artifact-failure"));
        assert!(!result.message.contains("private-tool-diagnostic"));
    }

    #[tokio::test]
    async fn historical_tool_failure_is_not_actionable_after_a_newer_success() {
        let pool = fixture_pool().await;
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" (
                    "Id", "AddonId", "Name", "ProviderMetadataJson"
               ) VALUES (1, 'org.example.list', 'Fixture', '{"title":"Fixture"}')"#,
        )
        .execute(&pool)
        .await
        .expect("list");
        sqlx::query(
            r#"INSERT INTO "AddonToolMaintenanceJob" (
                    "Id", "State", "Outcome", "Diagnostic"
               ) VALUES
                    (1, 4, 4, 'old private failure'),
                    (2, 3, 0, NULL)"#,
        )
        .execute(&pool)
        .await
        .expect("tool maintenance history");

        let result = AddonMediaListHealthCheck::new(pool)
            .check()
            .await
            .expect("health result");
        assert_eq!(result.status, HealthCheckStatus::Pass);
        assert!(result
            .message
            .contains("tool maintenance active=0 waiting=0 expired=0 failed=1"));
        assert!(result.message.contains("outcomes updated=1"));
        assert!(result.message.contains("failed=1"));
        assert!(!result.message.contains("old private failure"));
    }

    #[tokio::test]
    async fn empty_completed_import_links_directly_to_the_retryable_editor() {
        let pool = fixture_pool().await;
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" ("Id", "AddonId", "Name")
               VALUES (42, 'org.example.list', 'Empty direct video')"#,
        )
        .execute(&pool)
        .await
        .expect("list");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListImportJob" (
                    "Id", "AddonMediaListId", "Generation", "State", "DiscoveredCount", "UpdatedAt"
               ) VALUES (7, 42, 1, 3, 0, '2026-08-25T00:00:00Z')"#,
        )
        .execute(&pool)
        .await
        .expect("empty completed import");

        let result = AddonMediaListHealthCheck::new(pool)
            .check()
            .await
            .expect("health result");
        assert_eq!(result.status, HealthCheckStatus::Fail);
        assert!(result
            .message
            .contains("completed provider overview contains no items"));
        assert_eq!(
            result.link.as_ref().map(|link| link.link.as_str()),
            Some("/media/addon-lists/42")
        );
    }

    #[tokio::test]
    async fn empty_incremental_completion_is_not_reported_as_a_failed_import() {
        let pool = fixture_pool().await;
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" ("Id", "AddonId", "Name")
               VALUES (42, 'org.example.list', 'Unchanged list')"#,
        )
        .execute(&pool)
        .await
        .expect("list");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListImportJob" (
                    "Id", "AddonMediaListId", "Generation", "State", "Mode",
                    "DiscoveredCount", "UpdatedAt"
               ) VALUES (7, 42, 1, 3, 1, 0, '2026-08-25T00:00:00Z')"#,
        )
        .execute(&pool)
        .await
        .expect("unchanged incremental import");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListItem" (
                   "Id", "AddonMediaListId", "DisplayTitle"
               ) VALUES (1, 42, 'Retained item')"#,
        )
        .execute(&pool)
        .await
        .expect("retained baseline item");

        let failures = import_failures(&pool).await.expect("import failures");
        assert!(failures.is_empty());

        sqlx::query(r#"DELETE FROM "AddonMediaListItem" WHERE "AddonMediaListId" = 42"#)
            .execute(&pool)
            .await
            .expect("remove retained baseline item");
        let failures = import_failures(&pool)
            .await
            .expect("empty incremental failures");
        assert_eq!(failures.len(), 1);
        assert!(failures[0].1.contains("contains no items"));
    }

    #[tokio::test]
    async fn reports_expired_publication_as_recovery_required_not_live_work() {
        let pool = fixture_pool().await;
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" (
                    "Id", "AddonId", "Name", "ReviewState", "PublicationStartedAt"
               ) VALUES (1, 'org.example.list', 'Fixture', 3, '2020-01-01 00:00:00')"#,
        )
        .execute(&pool)
        .await
        .expect("list");
        sqlx::query(
            r#"INSERT INTO "ManagedLibraryOperation" ("Id", "State", "AttemptCount")
               VALUES (10, 1, 2)"#,
        )
        .execute(&pool)
        .await
        .expect("expired operation");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListPublicationJob" (
                   "Id", "OperationId", "Generation", "Phase", "OriginalFailureCode"
               ) VALUES (20, 10, 4, 4, 'sharing_violation')"#,
        )
        .execute(&pool)
        .await
        .expect("publication job");

        let result = AddonMediaListHealthCheck::new(pool)
            .check()
            .await
            .expect("health result");
        assert_eq!(result.status, HealthCheckStatus::Fail);
        assert!(result.message.contains("generation 4 expired"));
        assert!(result.message.contains("restoring files"));
        assert!(result.message.contains("sharing_violation"));
    }

    #[tokio::test]
    async fn reports_redacted_publication_history_aggregates() {
        let pool = fixture_pool().await;
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" (
                    "Id", "AddonId", "Name", "ReviewState", "PublicationStartedAt"
               ) VALUES (1, 'org.example.list', 'Fixture', 3, '2026-09-03T00:00:00Z')"#,
        )
        .execute(&pool)
        .await
        .expect("list");
        sqlx::query(
            r#"INSERT INTO "ManagedLibraryOperation" (
                    "Id", "State", "AttemptCount", "DiagnosticCode"
               ) VALUES (10, 4, 2, 'recovery_failed')"#,
        )
        .execute(&pool)
        .await
        .expect("failed operation");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListPublicationJob" (
                    "Id", "OperationId", "Generation", "Phase", "OriginalFailureCode",
                    "OriginalFailureDiagnostic", "RecoveryDiagnostic"
               ) VALUES (
                    20, 10, 4, 4, 'sharing_violation',
                    'https://provider.invalid/list?token=secret',
                    '/private/media/title.mkv cookie=session-secret'
               )"#,
        )
        .execute(&pool)
        .await
        .expect("publication job");
        for (sequence, dedupe_key, kind, error_category, os_error_code) in [
            (
                1,
                "original-failure",
                crate::repositories::common::encode_enum(
                    AddonMediaListPublicationEventKind::OriginalFailure,
                ),
                Some(2_i64),
                Some(32_i64),
            ),
            (
                2,
                "recovery-failure",
                crate::repositories::common::encode_enum(
                    AddonMediaListPublicationEventKind::RecoveryFailure,
                ),
                Some(1_i64),
                Some(5_i64),
            ),
            (
                256,
                "history-truncated",
                crate::repositories::common::encode_enum(
                    AddonMediaListPublicationEventKind::HistoryTruncated,
                ),
                None,
                None,
            ),
        ] {
            sqlx::query(
                r#"INSERT INTO "AddonMediaListPublicationEvent" (
                        "PublicationJobId", "Sequence", "DedupeKey", "Kind", "Phase",
                        "ErrorCategory", "OsErrorCode", "OccurredAt"
                   ) VALUES (20, ?, ?, ?, 4, ?, ?, '2026-09-03T00:00:00Z')"#,
            )
            .bind(sequence)
            .bind(dedupe_key)
            .bind(kind)
            .bind(error_category)
            .bind(os_error_code)
            .execute(&pool)
            .await
            .expect("publication event");
        }

        let result = AddonMediaListHealthCheck::new(pool)
            .check()
            .await
            .expect("health result");
        assert_eq!(result.status, HealthCheckStatus::Fail);
        assert!(result.message.contains("correlation 10 has 3 event(s)"));
        assert!(result.message.contains("1 original failure(s)"));
        assert!(result.message.contains("1 recovery failure(s), truncated"));
        assert!(result
            .message
            .contains("last error permission denied (OS code 5)"));
        for forbidden in [
            "https://provider.invalid/list?token=secret",
            "/private/media/title.mkv",
            "cookie=session-secret",
        ] {
            assert!(!result.message.contains(forbidden));
        }
    }

    #[tokio::test]
    async fn reports_live_publication_phase_attempts_and_progress() {
        let pool = fixture_pool().await;
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" (
                    "Id", "AddonId", "Name", "ReviewState", "ProviderMetadataJson"
               ) VALUES (1, 'org.example.list', 'Fixture', 3, '{"title":"Fixture"}')"#,
        )
        .execute(&pool)
        .await
        .expect("list");
        sqlx::query(
            r#"INSERT INTO "ManagedLibraryOperation" ("Id", "State", "AttemptCount")
               VALUES (10, 1, 3)"#,
        )
        .execute(&pool)
        .await
        .expect("active operation");
        sqlx::query(
            r#"INSERT INTO "ManagedLibraryLease" ("OperationId", "LeaseExpiresAt")
               VALUES (10, '2999-01-01T00:00:00Z')"#,
        )
        .execute(&pool)
        .await
        .expect("live lease");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListPublicationJob" (
                   "Id", "OperationId", "Generation", "Phase", "UpdatedAt"
               ) VALUES (20, 10, 5, 1, '2026-09-03T12:34:56Z')"#,
        )
        .execute(&pool)
        .await
        .expect("publication job");

        let result = AddonMediaListHealthCheck::new(pool)
            .check()
            .await
            .expect("health result");
        assert_eq!(result.status, HealthCheckStatus::Warning);
        assert!(result
            .message
            .contains("generation 5 publishing in staging"));
        assert!(result.message.contains("after 3 attempt(s)"));
        assert!(result.message.contains("2026-09-03T12:34:56Z"));
    }

    #[tokio::test]
    async fn reports_refresh_conflicts_before_generic_pending_review() {
        let pool = fixture_pool().await;
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" (
                    "Id", "AddonId", "Name", "ReviewState", "MetadataConflictJson"
               ) VALUES (1, 'org.example.list', 'Fixture', 1, '{"title":{}}')"#,
        )
        .execute(&pool)
        .await
        .expect("list");

        let result = AddonMediaListHealthCheck::new(pool)
            .check()
            .await
            .expect("health result");
        assert_eq!(result.status, HealthCheckStatus::Warning);
        assert!(result.message.contains("metadata field conflict"));
    }

    #[tokio::test]
    async fn automatic_publication_hold_links_to_review_and_clears_when_resolved() {
        let pool = fixture_pool().await;
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" (
            "Id", "AddonId", "Name", "RefreshPublicationPolicy", "ProviderMetadataJson"
        ) VALUES (42, 'org.example.list', 'Fixture', 1, '{"title":"Fixture"}')"#,
        )
        .execute(&pool)
        .await
        .expect("list");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListAutoPublication"
            ("AddonMediaListId", "State", "RecordedAt")
            VALUES (42, 'held', '2026-09-04T00:00:00Z')"#,
        )
        .execute(&pool)
        .await
        .expect("held assessment");
        let check = AddonMediaListHealthCheck::new(pool.clone());
        let result = check.check().await.expect("health result");
        assert_eq!(result.status, HealthCheckStatus::Warning);
        assert_eq!(result.message, "Automatic processing needs attention");
        assert_eq!(
            result.link.as_ref().map(|link| link.link.as_str()),
            Some("/media/addon-lists/42#addon-list-publication-activity")
        );
        sqlx::query(
            r#"UPDATE "AddonMediaList" SET "RefreshPublicationPolicy" = 0 WHERE "Id" = 42"#,
        )
        .execute(&pool)
        .await
        .expect("disable automatic publication");
        assert_eq!(
            check.check().await.expect("disabled policy").status,
            HealthCheckStatus::Pass
        );
        sqlx::query(
            r#"UPDATE "AddonMediaList" SET "RefreshPublicationPolicy" = 1 WHERE "Id" = 42"#,
        )
        .execute(&pool)
        .await
        .expect("enable automatic publication");
        sqlx::query(r#"UPDATE "AddonMediaListAutoPublication" SET "State" = 'done' WHERE "AddonMediaListId" = 42"#)
            .execute(&pool).await.expect("resolve assessment");
        assert_eq!(
            check.check().await.expect("resolved assessment").status,
            HealthCheckStatus::Pass
        );
    }

    #[tokio::test]
    async fn reports_overview_import_that_needs_attention() {
        let pool = fixture_pool().await;
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" ("Id", "AddonId", "Name", "ReviewState")
               VALUES (1, 'org.example.list', 'Fixture', 0)"#,
        )
        .execute(&pool)
        .await
        .expect("list");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListImportJob" (
                   "Id", "AddonMediaListId", "Generation", "State", "Diagnostic", "UpdatedAt"
               ) VALUES (1, 1, 1, 5, 'provider repeated a cursor', '2026-01-01 00:00:00')"#,
        )
        .execute(&pool)
        .await
        .expect("job");

        let result = AddonMediaListHealthCheck::new(pool)
            .check()
            .await
            .expect("health result");
        assert_eq!(result.status, HealthCheckStatus::Fail);
        assert!(result.message.contains("provider repeated a cursor"));
    }

    #[tokio::test]
    async fn reports_invalid_persisted_provider_delay() {
        let pool = fixture_pool().await;
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" ("Id", "AddonId", "Name")
               VALUES (1, 'org.example.list', 'Fixture')"#,
        )
        .execute(&pool)
        .await
        .expect("list");
        sqlx::query(
            r#"INSERT INTO "AddonProviderHostCooldown" ("ProviderHost", "SampledDelaySeconds")
               VALUES ('example.test', 0)"#,
        )
        .execute(&pool)
        .await
        .expect("cooldown");

        let result = AddonMediaListHealthCheck::new(pool)
            .check()
            .await
            .expect("health result");
        assert_eq!(result.status, HealthCheckStatus::Fail);
        assert!(result.message.contains("is not positive"));
    }

    #[tokio::test]
    async fn reports_invalid_addon_detail_wait_settings() {
        let pool = fixture_pool().await;
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" ("Id", "AddonId", "Name")
               VALUES (1, 'org.example.list', 'Fixture')"#,
        )
        .execute(&pool)
        .await
        .expect("list");
        sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, ?), (?, ?)"#)
            .bind(ersatzrs_domain::CONFIG_KEY_ADDON_DETAIL_WAIT_MINIMUM_SECONDS)
            .bind("12")
            .bind(ersatzrs_domain::CONFIG_KEY_ADDON_DETAIL_WAIT_MAXIMUM_SECONDS)
            .bind("11")
            .execute(&pool)
            .await
            .expect("invalid detail wait settings");

        let result = AddonMediaListHealthCheck::new(pool)
            .check()
            .await
            .expect("health result");
        assert_eq!(result.status, HealthCheckStatus::Fail);
        assert!(result.message.contains("detail wait settings are invalid"));
    }

    #[tokio::test]
    async fn distinguishes_active_waiting_retry_and_failed_detail_work() {
        let pool = fixture_pool().await;
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" (
                   "Id", "AddonId", "Name", "ReviewState", "ProviderMetadataJson"
               ) VALUES (1, 'org.example.list', 'Fixture', 2, '{"title":"Fixture"}')"#,
        )
        .execute(&pool)
        .await
        .expect("list");
        for (id, state, lease) in [
            (1_i64, 3_i64, Some("2099-01-01 00:00:00")),
            (2, 2, None),
            (3, 4, None),
            (4, 7, None),
        ] {
            sqlx::query(
                r#"INSERT INTO "AddonMediaListItem" (
                       "Id", "AddonMediaListId", "DisplayTitle"
                   ) VALUES (?, 1, ?)"#,
            )
            .bind(id)
            .bind(format!("Item {id}"))
            .execute(&pool)
            .await
            .expect("item");
            sqlx::query(
                r#"INSERT INTO "AddonMediaListDetailJob" (
                       "AddonMediaListItemId", "AddonMediaListId", "State",
                       "UpdatedAt", "LeaseExpiresAt"
                   ) VALUES (?, 1, ?, '2026-08-23 12:00:00', ?)"#,
            )
            .bind(id)
            .bind(state)
            .bind(lease)
            .execute(&pool)
            .await
            .expect("detail job");
        }

        let result = AddonMediaListHealthCheck::new(pool)
            .check()
            .await
            .expect("health result");
        assert_eq!(result.status, HealthCheckStatus::Warning);
        assert!(result
            .message
            .contains("1 item detail job(s) need a manual retry"));
        assert!(result.message.contains("1 are active"));
        assert!(result.message.contains("2 are waiting"));
    }

    #[tokio::test]
    async fn ignores_failed_detail_work_for_excluded_items() {
        let pool = fixture_pool().await;
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" (
                   "Id", "AddonId", "Name", "ReviewState", "ProviderMetadataJson"
               ) VALUES (1, 'org.example.list', 'Fixture', 2, '{"title":"Fixture"}')"#,
        )
        .execute(&pool)
        .await
        .expect("list");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListItem" (
                   "Id", "AddonMediaListId", "DisplayTitle", "Included"
               ) VALUES (1, 1, 'Excluded item', 0)"#,
        )
        .execute(&pool)
        .await
        .expect("excluded item");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListDetailJob" (
                   "AddonMediaListItemId", "AddonMediaListId", "State", "UpdatedAt"
               ) VALUES (1, 1, 7, '2026-08-23 12:00:00')"#,
        )
        .execute(&pool)
        .await
        .expect("failed detail job");

        let result = AddonMediaListHealthCheck::new(pool)
            .check()
            .await
            .expect("health result");
        assert_eq!(result.status, HealthCheckStatus::Pass);
    }

    #[tokio::test]
    async fn reports_selected_but_unmaterialized_artwork() {
        let pool = fixture_pool().await;
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" (
                    "Id", "AddonId", "Name", "ReviewState", "ProviderMetadataJson"
               ) VALUES (
                    1, 'org.example.list', 'Fixture', 2,
                    '{"title":"Fixture","artwork":[{"url":"https://example.test/poster.jpg","role":"poster"}]}'
               )"#,
        )
        .execute(&pool)
        .await
        .expect("list");

        let result = AddonMediaListHealthCheck::new(pool)
            .check()
            .await
            .expect("health result");
        assert_eq!(result.status, HealthCheckStatus::Fail);
        assert!(result
            .message
            .contains("selected artwork is not materialized"));
    }

    #[tokio::test]
    async fn reports_a_durable_publication_that_stopped_before_terminal_state() {
        let pool = fixture_pool().await;
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" (
                   "Id", "AddonId", "Name", "ReviewState", "ProviderMetadataJson"
               ) VALUES (1, 'org.example.list', 'Fixture', 2, '{"title":"Fixture"}')"#,
        )
        .execute(&pool)
        .await
        .expect("list");
        sqlx::query(
            r#"INSERT INTO "ManagedLibraryOperation" ("Id", "State", "LeaseExpiresAt")
               VALUES (10, 4, NULL)"#,
        )
        .execute(&pool)
        .await
        .expect("failed operation");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListPublicationJob" ("Id", "OperationId", "Phase")
               VALUES (20, 10, 3)"#,
        )
        .execute(&pool)
        .await
        .expect("publication job");

        let result = AddonMediaListHealthCheck::new(pool)
            .check()
            .await
            .expect("health result");
        assert_eq!(result.status, HealthCheckStatus::Fail);
        assert!(result.message.contains("generation 1 failed in committing"));
    }
}
