use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckError, HealthCheckResult,
    HealthCheckStatus, Pool, Result, Row,
};

const STUCK_PUBLICATION_MINUTES: i64 = 15;
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
#[allow(clippy::too_many_lines)]
impl HealthCheck for AddonMediaListHealthCheck {
    fn title(&self) -> &'static str {
        "Add-on media lists"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let rows = sqlx::query(
            r#"SELECT "list"."Id", "list"."Name", "list"."LastError", "list"."PresentationError",
                      "list"."ReviewState", "list"."ReviewError", "list"."PublicationStartedAt",
                      "addon"."Enabled",
                      "source"."MediaSourceId", "managed"."LibraryId", "managed"."ManifestPath"
               FROM "AddonMediaList" AS "list"
               JOIN "AddonInstallation" AS "addon"
                 ON "addon"."AddonId" = "list"."AddonId"
               LEFT JOIN "AddonMediaSource" AS "source"
                 ON "source"."AddonId" = "list"."AddonId"
               LEFT JOIN "AddonMediaListLibrary" AS "managed"
                 ON "managed"."AddonMediaListId" = "list"."Id"
               ORDER BY "list"."Name", "list"."Id""#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;
        if rows.is_empty() {
            return Ok(HealthCheckResult::not_applicable(self.title()));
        }

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
        let failures = rows
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
        let presentation_failures = presentation_failures(&rows);
        let publication_failures = publication_failures(&rows);
        let stuck_publications = stuck_publications(&rows)?;
        let coordinator_failures = publication_coordinator_failures(&self.pool).await?;
        let invalid_metadata = invalid_metadata(&self.pool).await?;
        let missing_artwork = missing_artwork(&self.pool).await?;
        let import_failures = import_failures(&self.pool).await?;
        let import_failure_link = import_failures
            .first()
            .map(|(list_id, _)| format!("/media/addon-lists/{list_id}"));
        let detail_worker_failures = detail_worker_failures(&self.pool).await?;
        let detail_wait_setting_failures = detail_wait_setting_failures(&self.pool).await;
        let cooldown_failures = cooldown_failures(&self.pool).await?;
        let failures = storage_failures
            .into_iter()
            .chain(failures)
            .chain(presentation_failures)
            .chain(publication_failures)
            .chain(stuck_publications)
            .chain(coordinator_failures)
            .chain(invalid_metadata)
            .chain(missing_artwork)
            .chain(import_failures.into_iter().map(|(_, message)| message))
            .chain(detail_worker_failures)
            .chain(detail_wait_setting_failures)
            .chain(cooldown_failures)
            .take(5)
            .collect::<Vec<_>>();
        if !failures.is_empty() {
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Fail,
                failures.join("; "),
                "One or more add-on media lists failed to refresh",
            )
            .with_link(import_failure_link.unwrap_or_else(|| "/media/addon-lists".to_owned())));
        }

        let disabled = rows
            .iter()
            .filter(|row| {
                row.try_get::<i64, _>("Enabled")
                    .is_ok_and(|value| value == 0)
            })
            .count();
        if disabled > 0 {
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Warning,
                format!("{disabled} list(s) use a disabled add-on"),
                "An add-on must be enabled before its lists can refresh",
            )
            .with_link("/media/addon-lists"));
        }

        if let Some(result) = replacement_failure(&self.pool, self.title()).await? {
            return Ok(result);
        }

        let metadata_conflicts = count_metadata_conflicts(&self.pool).await?;
        if metadata_conflicts > 0 {
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Warning,
                format!("{metadata_conflicts} metadata field conflict(s) need review"),
                "A provider changed fields that have operator overrides",
            )
            .with_link("/media/addon-lists"));
        }

        let detail_activity = detail_activity(&self.pool).await?;
        if detail_activity.failed > 0 {
            return Ok(HealthCheckResult::new(
                self.title(),
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
            let import_activity = import_activity(&self.pool).await?;
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Warning,
                format!(
                    "{pending_review} add-on media list(s) await metadata review or publication; {} overview import(s) are active and {} detail item(s) are delayed by retry or provider pacing",
                    import_activity.active,
                    detail_activity.waiting,
                ),
                "Review and publish the imported metadata before using the new revision",
            )
            .with_link("/media/addon-lists"));
        }

        if detail_activity.active > 0 || detail_activity.waiting > 0 {
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Pass,
                format!(
                    "{} item detail job(s) are active and {} are waiting for retry or provider pacing",
                    detail_activity.active, detail_activity.waiting
                ),
                "Add-on detail loading is making or awaiting progress",
            )
            .with_link("/media/addon-lists"));
        }

        Ok(HealthCheckResult::pass(self.title()).with_link("/media/addon-lists"))
    }
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
        r#"SELECT "list"."Id" AS "ListId", "list"."Name", "job"."State",
                  "job"."DiscoveredCount", "job"."Diagnostic",
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
        let discovered_count = row
            .try_get::<i64, _>("DiscoveredCount")
            .map_err(sqlx_error)?;
        if state == 3 && discovered_count == 0 {
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

fn stuck_publications(rows: &[crate::db::DbRow]) -> Result<Vec<String>> {
    let threshold = chrono::Utc::now() - chrono::Duration::minutes(STUCK_PUBLICATION_MINUTES);
    rows.iter()
        .filter(|row| {
            row.try_get::<i64, _>("ReviewState")
                .is_ok_and(|state| state == 3)
        })
        .filter_map(|row| {
            let name = row
                .try_get::<String, _>("Name")
                .unwrap_or_else(|_| "List".to_owned());
            let raw = row
                .try_get::<Option<String>, _>("PublicationStartedAt")
                .ok()?;
            match crate::repositories::common::decode_optional_datetime(
                "add-on publication start",
                raw,
            ) {
                Ok(Some(started)) if started > threshold => None,
                Ok(_) => Some(Ok(format!(
                    "{name}: publication has been in progress for more than {STUCK_PUBLICATION_MINUTES} minutes"
                ))),
                Err(error) => Some(Err(HealthCheckError::Database(error.to_string()))),
            }
        })
        .collect()
}

async fn count_metadata_conflicts(pool: &Pool<Db>) -> Result<i64> {
    let list_count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "AddonMediaList"
           WHERE "MetadataConflictJson" IS NOT NULL
             AND TRIM("MetadataConflictJson") NOT IN ('', '{}')"#,
    )
    .fetch_one(pool)
    .await
    .map_err(sqlx_error)?;
    let item_count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "AddonMediaListItem"
           WHERE "MetadataConflictJson" IS NOT NULL
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

async fn publication_coordinator_failures(pool: &Pool<Db>) -> Result<Vec<String>> {
    let now = crate::repositories::common::encode_datetime(chrono::Utc::now());
    let failed: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*)
           FROM "AddonMediaListPublicationJob" AS "job"
           JOIN "ManagedLibraryOperation" AS "operation"
             ON "operation"."Id" = "job"."OperationId"
           WHERE "operation"."State" = 4 AND "job"."Phase" <> 7"#,
    )
    .fetch_one(pool)
    .await
    .map_err(sqlx_error)?;
    let expired: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*)
           FROM "AddonMediaListPublicationJob" AS "job"
           JOIN "ManagedLibraryOperation" AS "operation"
             ON "operation"."Id" = "job"."OperationId"
           WHERE "operation"."State" = 1
             AND "operation"."LeaseExpiresAt" IS NOT NULL
             AND "operation"."LeaseExpiresAt" <= ?
             AND "job"."Phase" <> 7"#,
    )
    .bind(now)
    .fetch_one(pool)
    .await
    .map_err(sqlx_error)?;
    let mut failures = Vec::new();
    if failed > 0 {
        failures.push(format!(
            "{failed} durable add-on publication job(s) stopped before a safe terminal phase"
        ));
    }
    if expired > 0 {
        failures.push(format!(
            "{expired} durable add-on publication lease(s) expired and await worker recovery"
        ));
    }
    Ok(failures)
}

async fn count_missing_replacements(pool: &Pool<Db>) -> Result<i64> {
    sqlx::query_scalar(
        r#"SELECT COUNT(*)
           FROM "AddonMediaListItem" AS "item"
           LEFT JOIN "AddonMediaListItemReplacement" AS "replacement"
             ON "replacement"."AddonMediaListItemId" = "item"."Id"
           WHERE "item"."Availability" = 1
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

    async fn fixture_pool() -> Pool<Db> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        sqlx::raw_sql(
            r#"
            CREATE TABLE "AddonInstallation" ("AddonId" TEXT PRIMARY KEY, "Enabled" INTEGER);
            CREATE TABLE "AddonMediaList" (
                "Id" INTEGER PRIMARY KEY,
                "AddonId" TEXT NOT NULL,
                "Name" TEXT NOT NULL,
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
                "LeaseExpiresAt" TEXT NULL
            );
            CREATE TABLE "AddonMediaListPublicationJob" (
                "Id" INTEGER PRIMARY KEY,
                "OperationId" INTEGER NOT NULL,
                "Phase" INTEGER NOT NULL
            );
            CREATE TABLE "AddonProviderHostCooldown" (
                "ProviderHost" TEXT PRIMARY KEY,
                "SampledDelaySeconds" INTEGER NOT NULL
            );
            CREATE TABLE "ConfigElement" (
                "Key" TEXT PRIMARY KEY,
                "Value" TEXT NOT NULL
            );
            INSERT INTO "AddonInstallation" ("AddonId", "Enabled") VALUES ('org.example.list', 1);
            "#,
        )
        .execute(&pool)
        .await
        .expect("fixture");
        pool
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
    async fn reports_stuck_publication() {
        let pool = fixture_pool().await;
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" (
                    "Id", "AddonId", "Name", "ReviewState", "PublicationStartedAt"
               ) VALUES (1, 'org.example.list', 'Fixture', 3, '2020-01-01 00:00:00')"#,
        )
        .execute(&pool)
        .await
        .expect("list");

        let result = AddonMediaListHealthCheck::new(pool)
            .check()
            .await
            .expect("health result");
        assert_eq!(result.status, HealthCheckStatus::Fail);
        assert!(result.message.contains("publication has been in progress"));
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
        assert!(result
            .message
            .contains("stopped before a safe terminal phase"));
    }
}
