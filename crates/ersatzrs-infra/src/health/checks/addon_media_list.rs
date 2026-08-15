use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckError, HealthCheckResult,
    HealthCheckStatus, Pool, Result, Row,
};

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
        let rows = sqlx::query(
            r#"SELECT "list"."Name", "list"."LastError", "addon"."Enabled",
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
        let failures = storage_failures
            .into_iter()
            .chain(failures)
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

        let missing_replacements = count_missing_replacements(&self.pool).await?;
        let stale_replacements = count_stale_replacements(&self.pool).await?;
        if stale_replacements > 0 || missing_replacements > 0 {
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Warning,
                format!(
                    "{missing_replacements} unavailable item(s) have no replacement; {stale_replacements} replacement mapping(s) are stale"
                ),
                "One or more add-on list gaps need operator attention",
            )
            .with_link("/media/addon-lists"));
        }

        Ok(HealthCheckResult::pass(self.title()).with_link("/media/addon-lists"))
    }
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

    #[tokio::test]
    async fn reports_persisted_refresh_failure() {
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
                "LastError" TEXT NULL
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
            INSERT INTO "AddonMediaList" ("Id", "AddonId", "Name", "LastError")
                VALUES (1, 'org.example.list', 'Fixture', 'provider unavailable');
            "#,
        )
        .execute(&pool)
        .await
        .expect("fixture");

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
}
