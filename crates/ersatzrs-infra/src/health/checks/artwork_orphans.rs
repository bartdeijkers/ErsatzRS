use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckResult, HealthCheckStatus, Pool, Result,
};
use crate::artwork_cleanup::{ACTOR_ORPHAN, ARTWORK_ORPHAN};

pub(crate) struct ArtworkOrphansHealthCheck {
    pool: Pool<Db>,
}

impl ArtworkOrphansHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for ArtworkOrphansHealthCheck {
    fn title(&self) -> &'static str {
        "Artwork Orphans"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        // Reuse the deletion predicates and read both aggregates in one snapshot.
        // Never include actor names, artwork paths, or media identifiers.
        let (actors, artwork): (i64, i64) = sqlx::query_as(crate::db::assert_sql_safe(format!(
            r#"SELECT (SELECT COUNT(*) FROM "Actor" WHERE {ACTOR_ORPHAN}),
                      (SELECT COUNT(*) FROM "Artwork" WHERE {ARTWORK_ORPHAN})"#,
        )))
        .fetch_one(&self.pool)
        .await
        .map_err(sqlx_error)?;
        if actors == 0 && artwork == 0 {
            return Ok(HealthCheckResult::pass(self.title()));
        }
        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Warning,
            format!("Orphan cleanup pending: {actors} actor row(s) and {artwork} artwork row(s). Automatic cleanup runs hourly. To request cleanup now, use POST /api/maintenance/clean_artwork. Attached records and cached image files are preserved."),
            format!("{actors} orphan actor row(s) and {artwork} orphan artwork row(s)"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn artwork_orphans_health_counts_without_mutating_or_disclosing_records() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
        let check = ArtworkOrphansHealthCheck::new(pool.clone());
        assert_eq!(check.check().await.unwrap().status, HealthCheckStatus::Pass);
        sqlx::raw_sql(
            r#"INSERT INTO "Artwork" ("Id", "ArtworkKind", "Path", "DateAdded", "DateUpdated")
               VALUES (8001, 0, 'synthetic/private-portrait.png', '2026-04-17', '2026-04-17'),
                      (8002, 0, 'synthetic/private-orphan.png', '2026-04-17', '2026-04-17');
               INSERT INTO "Actor" ("Id", "Name", "ArtworkId")
               VALUES (9001, 'Private actor name', 8001), (9002, 'Private actor name', NULL);"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        let result = check.check().await.unwrap();
        assert_eq!(result.status, HealthCheckStatus::Warning);
        assert_eq!(
            result.brief_message,
            "2 orphan actor row(s) and 1 orphan artwork row(s)"
        );
        let encoded = serde_json::to_string(&result).unwrap();
        for private in [
            "Private actor",
            "synthetic/",
            "8001",
            "8002",
            "9001",
            "9002",
        ] {
            assert!(!encoded.contains(private));
        }
        assert!(result
            .message
            .contains("POST /api/maintenance/clean_artwork"));
        let counts: (i64, i64) = sqlx::query_as(
            r#"SELECT (SELECT COUNT(*) FROM "Actor"), (SELECT COUNT(*) FROM "Artwork")"#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(counts, (2, 2), "health reporting must not perform cleanup");
        let deleted = crate::artwork_cleanup::clean_orphaned_artwork(&pool, 100_000)
            .await
            .unwrap();
        assert_eq!((deleted.actors_deleted, deleted.artwork_deleted), (2, 2));
        assert_eq!(check.check().await.unwrap().status, HealthCheckStatus::Pass);
    }
}
