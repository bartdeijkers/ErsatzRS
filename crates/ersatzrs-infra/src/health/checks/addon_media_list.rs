use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckError, HealthCheckResult,
    HealthCheckStatus, Pool, Result, Row,
};

const STUCK_PUBLICATION_MINUTES: i64 = 15;

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
        let invalid_metadata = invalid_metadata(&self.pool).await?;
        let missing_artwork = missing_artwork(&self.pool).await?;
        let failures = storage_failures
            .into_iter()
            .chain(failures)
            .chain(presentation_failures)
            .chain(publication_failures)
            .chain(stuck_publications)
            .chain(invalid_metadata)
            .chain(missing_artwork)
            .take(5)
            .collect::<Vec<_>>();
        if !failures.is_empty() {
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Fail,
                failures.join("; "),
                "One or more add-on media lists failed to refresh",
            )
            .with_link("/media/addon-lists"));
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

        let pending_review = rows
            .iter()
            .filter(|row| {
                row.try_get::<i64, _>("ReviewState")
                    .is_ok_and(|state| state != 2)
            })
            .count();
        if pending_review > 0 {
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Warning,
                format!(
                    "{pending_review} add-on media list(s) await metadata review or publication"
                ),
                "Review and publish the imported metadata before using the new revision",
            )
            .with_link("/media/addon-lists"));
        }

        Ok(HealthCheckResult::pass(self.title()).with_link("/media/addon-lists"))
    }
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

#[cfg(all(test, not(feature = "mysql")))]
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
}
