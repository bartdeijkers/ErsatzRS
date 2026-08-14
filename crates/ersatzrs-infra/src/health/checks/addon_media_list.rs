use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckResult, HealthCheckStatus, Pool, Result,
    Row,
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
            r#"SELECT "list"."Name", "list"."LastError", "addon"."Enabled"
               FROM "AddonMediaList" AS "list"
               JOIN "AddonInstallation" AS "addon"
                 ON "addon"."AddonId" = "list"."AddonId"
               ORDER BY "list"."Name", "list"."Id""#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;
        if rows.is_empty() {
            return Ok(HealthCheckResult::not_applicable(self.title()));
        }

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

        Ok(HealthCheckResult::pass(self.title()).with_link("/media/addon-lists"))
    }
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
