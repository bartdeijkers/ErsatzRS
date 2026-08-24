use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckResult, HealthCheckStatus, Pool, Result,
    Row,
};

const DUPLICATE_SHOW_ARTWORK_SQL: &str = r#"
    SELECT COUNT(*) AS "GroupSize"
    FROM "Artwork" AS "artwork"
    WHERE "artwork"."ShowMetadataId" IS NOT NULL
      AND EXISTS (
          SELECT 1
          FROM "ShowMetadata" AS "metadata"
          JOIN "AddonMediaListTelevisionShow" AS "projection"
            ON "projection"."ShowId" = "metadata"."ShowId"
          WHERE "metadata"."Id" = "artwork"."ShowMetadataId"
      )
    GROUP BY "artwork"."ShowMetadataId", "artwork"."ArtworkKind",
             HEX("artwork"."Path"), HEX("artwork"."SourcePath")
    HAVING COUNT(*) > 1
"#;

const DUPLICATE_SEASON_ARTWORK_SQL: &str = r#"
    SELECT COUNT(*) AS "GroupSize"
    FROM "Artwork" AS "artwork"
    WHERE "artwork"."SeasonMetadataId" IS NOT NULL
      AND EXISTS (
          SELECT 1
          FROM "SeasonMetadata" AS "metadata"
          JOIN "Season" AS "season" ON "season"."Id" = "metadata"."SeasonId"
          JOIN "AddonMediaListTelevisionShow" AS "projection"
            ON "projection"."ShowId" = "season"."ShowId"
          WHERE "metadata"."Id" = "artwork"."SeasonMetadataId"
      )
    GROUP BY "artwork"."SeasonMetadataId", "artwork"."ArtworkKind",
             HEX("artwork"."Path"), HEX("artwork"."SourcePath")
    HAVING COUNT(*) > 1
"#;

const DUPLICATE_EPISODE_ARTWORK_SQL: &str = r#"
    SELECT COUNT(*) AS "GroupSize"
    FROM "Artwork" AS "artwork"
    WHERE "artwork"."EpisodeMetadataId" IS NOT NULL
      AND EXISTS (
          SELECT 1
          FROM "EpisodeMetadata" AS "metadata"
          JOIN "AddonMediaListItemEpisode" AS "projection"
            ON "projection"."EpisodeId" = "metadata"."EpisodeId"
          WHERE "metadata"."Id" = "artwork"."EpisodeMetadataId"
      )
    GROUP BY "artwork"."EpisodeMetadataId", "artwork"."ArtworkKind",
             HEX("artwork"."Path"), HEX("artwork"."SourcePath")
    HAVING COUNT(*) > 1
"#;

const PROJECTED_TELEVISION_COUNTS_SQL: &str = r#"
    SELECT
        (
            SELECT COUNT(*)
            FROM "AddonMediaListItem" AS "item"
            JOIN "AddonMediaListTelevisionShow" AS "television"
              ON "television"."AddonMediaListId" = "item"."AddonMediaListId"
            WHERE "item"."ParentProviderId" IS NULL
              AND "item"."Published" = 1
        ) AS "PublishedItems",
        (
            SELECT COUNT(*)
            FROM "AddonMediaListItemEpisode" AS "projection"
            JOIN "AddonMediaListItem" AS "item"
              ON "item"."Id" = "projection"."AddonMediaListItemId"
            JOIN "AddonMediaListTelevisionShow" AS "television"
              ON "television"."AddonMediaListId" = "item"."AddonMediaListId"
        ) AS "ProjectionRows",
        (
            SELECT COUNT(DISTINCT "projection"."EpisodeId")
            FROM "AddonMediaListItemEpisode" AS "projection"
            JOIN "AddonMediaListItem" AS "item"
              ON "item"."Id" = "projection"."AddonMediaListItemId"
            JOIN "AddonMediaListTelevisionShow" AS "television"
              ON "television"."AddonMediaListId" = "item"."AddonMediaListId"
        ) AS "DistinctEpisodes",
        (
            SELECT COUNT(*)
            FROM "AddonMediaListItemEpisode" AS "projection"
            JOIN "AddonMediaListItem" AS "item"
              ON "item"."Id" = "projection"."AddonMediaListItemId"
            JOIN "AddonMediaListTelevisionShow" AS "television"
              ON "television"."AddonMediaListId" = "item"."AddonMediaListId"
            JOIN "Episode" AS "episode" ON "episode"."Id" = "projection"."EpisodeId"
            JOIN "Season" AS "season" ON "season"."Id" = "episode"."SeasonId"
            JOIN "Show" AS "show" ON "show"."Id" = "season"."ShowId"
            WHERE "show"."Id" = "television"."ShowId"
              AND EXISTS (
                  SELECT 1 FROM "ShowMetadata" AS "metadata"
                  WHERE "metadata"."ShowId" = "show"."Id"
              )
              AND EXISTS (
                  SELECT 1 FROM "SeasonMetadata" AS "metadata"
                  WHERE "metadata"."SeasonId" = "season"."Id"
              )
              AND EXISTS (
                  SELECT 1 FROM "EpisodeMetadata" AS "metadata"
                  WHERE "metadata"."EpisodeId" = "episode"."Id"
              )
        ) AS "CompleteHierarchyRows",
        (
            SELECT COUNT(DISTINCT "episode"."Id")
            FROM "AddonMediaListTelevisionShow" AS "television"
            JOIN "Show" AS "show" ON "show"."Id" = "television"."ShowId"
            JOIN "Season" AS "season" ON "season"."ShowId" = "show"."Id"
            JOIN "Episode" AS "episode" ON "episode"."SeasonId" = "season"."Id"
        ) AS "HierarchyEpisodes"
"#;

#[derive(Debug, Default, PartialEq, Eq)]
struct ArtworkCardinalityCounts {
    duplicate_show_rows: i64,
    duplicate_season_rows: i64,
    duplicate_episode_rows: i64,
    published_items: i64,
    projection_rows: i64,
    distinct_episodes: i64,
    complete_hierarchy_rows: i64,
    hierarchy_episodes: i64,
}

impl ArtworkCardinalityCounts {
    fn duplicate_rows(&self) -> i64 {
        self.duplicate_show_rows + self.duplicate_season_rows + self.duplicate_episode_rows
    }

    fn cardinality_mismatches(&self) -> i64 {
        [
            self.published_items != self.projection_rows,
            self.projection_rows != self.distinct_episodes,
            self.projection_rows != self.complete_hierarchy_rows,
            self.projection_rows != self.hierarchy_episodes,
        ]
        .into_iter()
        .filter(|mismatch| *mismatch)
        .count()
        .try_into()
        .unwrap_or(i64::MAX)
    }

    fn is_healthy(&self) -> bool {
        self.duplicate_rows() == 0 && self.cardinality_mismatches() == 0
    }
}

pub(crate) struct ArtworkCardinalityHealthCheck {
    pool: Pool<Db>,
}

impl ArtworkCardinalityHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for ArtworkCardinalityHealthCheck {
    fn title(&self) -> &'static str {
        "Artwork Cardinality"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let counts = load_counts(&self.pool).await?;
        if counts.is_healthy() {
            return Ok(HealthCheckResult::pass(self.title()));
        }

        // This result is intentionally aggregate-only. Artwork paths, add-on
        // provider identifiers, list names, and media identifiers must never
        // reach the persisted or admin-visible health payload.
        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Warning,
            format!(
                "Artwork cardinality needs attention: {} duplicate show artwork row(s), {} duplicate season artwork row(s), and {} duplicate episode artwork row(s); projected television counts are {} published add-on list episode item(s), {} projection row(s), {} distinct projected episode(s), {} complete hierarchy projection(s), and {} hierarchy episode(s). Refresh or republish the affected add-on television lists.",
                counts.duplicate_show_rows,
                counts.duplicate_season_rows,
                counts.duplicate_episode_rows,
                counts.published_items,
                counts.projection_rows,
                counts.distinct_episodes,
                counts.complete_hierarchy_rows,
                counts.hierarchy_episodes,
            ),
            format!(
                "{} duplicate artwork row(s) and {} projected television cardinality mismatch(es)",
                counts.duplicate_rows(),
                counts.cardinality_mismatches(),
            ),
        )
        .with_link("/media/addon-lists"))
    }
}

async fn load_counts(pool: &Pool<Db>) -> Result<ArtworkCardinalityCounts> {
    let duplicate_show_rows = duplicate_artwork_rows(pool, DUPLICATE_SHOW_ARTWORK_SQL).await?;
    let duplicate_season_rows = duplicate_artwork_rows(pool, DUPLICATE_SEASON_ARTWORK_SQL).await?;
    let duplicate_episode_rows =
        duplicate_artwork_rows(pool, DUPLICATE_EPISODE_ARTWORK_SQL).await?;
    let row = sqlx::query(PROJECTED_TELEVISION_COUNTS_SQL)
        .fetch_one(pool)
        .await
        .map_err(sqlx_error)?;

    Ok(ArtworkCardinalityCounts {
        duplicate_show_rows,
        duplicate_season_rows,
        duplicate_episode_rows,
        published_items: row.try_get("PublishedItems").map_err(sqlx_error)?,
        projection_rows: row.try_get("ProjectionRows").map_err(sqlx_error)?,
        distinct_episodes: row.try_get("DistinctEpisodes").map_err(sqlx_error)?,
        complete_hierarchy_rows: row.try_get("CompleteHierarchyRows").map_err(sqlx_error)?,
        hierarchy_episodes: row.try_get("HierarchyEpisodes").map_err(sqlx_error)?,
    })
}

async fn duplicate_artwork_rows(pool: &Pool<Db>, query: &'static str) -> Result<i64> {
    let rows = sqlx::query(query)
        .fetch_all(pool)
        .await
        .map_err(sqlx_error)?;
    rows.into_iter().try_fold(0_i64, |total, row| {
        let group_size = row.try_get::<i64, _>("GroupSize").map_err(sqlx_error)?;
        Ok(total.saturating_add(group_size.saturating_sub(1)))
    })
}

#[cfg(all(test, not(feature = "mysql")))]
mod tests {
    use sqlx::{sqlite::SqlitePoolOptions, Executor};

    use super::*;

    async fn fixture_pool() -> Pool<Db> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("create artwork cardinality health database");
        sqlx::raw_sql(
            r#"
            CREATE TABLE "Artwork" (
                "Id" INTEGER PRIMARY KEY,
                "ArtworkKind" INTEGER NOT NULL,
                "Path" TEXT NULL,
                "SourcePath" TEXT NULL,
                "ShowMetadataId" INTEGER NULL,
                "SeasonMetadataId" INTEGER NULL,
                "EpisodeMetadataId" INTEGER NULL
            );
            CREATE TABLE "Show" ("Id" INTEGER PRIMARY KEY);
            CREATE TABLE "ShowMetadata" (
                "Id" INTEGER PRIMARY KEY,
                "ShowId" INTEGER NOT NULL
            );
            CREATE TABLE "Season" (
                "Id" INTEGER PRIMARY KEY,
                "ShowId" INTEGER NOT NULL
            );
            CREATE TABLE "SeasonMetadata" (
                "Id" INTEGER PRIMARY KEY,
                "SeasonId" INTEGER NOT NULL
            );
            CREATE TABLE "Episode" (
                "Id" INTEGER PRIMARY KEY,
                "SeasonId" INTEGER NOT NULL
            );
            CREATE TABLE "EpisodeMetadata" (
                "Id" INTEGER PRIMARY KEY,
                "EpisodeId" INTEGER NOT NULL
            );
            CREATE TABLE "AddonMediaListTelevisionShow" (
                "AddonMediaListId" INTEGER NOT NULL,
                "ShowId" INTEGER NOT NULL
            );
            CREATE TABLE "AddonMediaListItem" (
                "Id" INTEGER PRIMARY KEY,
                "AddonMediaListId" INTEGER NOT NULL,
                "ParentProviderId" TEXT NULL,
                "Published" INTEGER NOT NULL
            );
            CREATE TABLE "AddonMediaListItemEpisode" (
                "AddonMediaListItemId" INTEGER NOT NULL,
                "EpisodeId" INTEGER NOT NULL
            );

            INSERT INTO "Show" ("Id") VALUES (100);
            INSERT INTO "ShowMetadata" ("Id", "ShowId") VALUES (1000, 100);
            INSERT INTO "Season" ("Id", "ShowId") VALUES (200, 100);
            INSERT INTO "SeasonMetadata" ("Id", "SeasonId") VALUES (2000, 200);
            INSERT INTO "Episode" ("Id", "SeasonId") VALUES (300, 200);
            INSERT INTO "EpisodeMetadata" ("Id", "EpisodeId") VALUES (3000, 300);
            INSERT INTO "AddonMediaListTelevisionShow" ("AddonMediaListId", "ShowId")
                VALUES (1, 100);
            INSERT INTO "AddonMediaListItem" (
                "Id", "AddonMediaListId", "ParentProviderId", "Published"
            ) VALUES (10, 1, NULL, 1);
            INSERT INTO "AddonMediaListItemEpisode" ("AddonMediaListItemId", "EpisodeId")
                VALUES (10, 300);
            "#,
        )
        .execute(&pool)
        .await
        .expect("seed artwork cardinality health database");
        pool
    }

    #[tokio::test]
    async fn passes_for_exactly_one_projection_and_distinct_artwork_variants() {
        let pool = fixture_pool().await;
        sqlx::raw_sql(
            r#"
            INSERT INTO "Artwork" (
                "Id", "ArtworkKind", "Path", "SourcePath", "ShowMetadataId"
            ) VALUES
                (1, 0, '/private/show.jpg', 'provider-a', 1000),
                (2, 0, '/private/Show.jpg', 'provider-a', 1000),
                (3, 0, '/private/show.jpg', 'provider-b', 1000),
                (4, 1, '/private/show.jpg', 'provider-a', 1000);
            INSERT INTO "Artwork" (
                "Id", "ArtworkKind", "Path", "SourcePath", "SeasonMetadataId"
            ) VALUES (5, 0, NULL, 'https://example.test/season.jpg', 2000);
            INSERT INTO "Artwork" (
                "Id", "ArtworkKind", "Path", "SourcePath", "EpisodeMetadataId"
            ) VALUES (6, 1, '/private/episode.jpg', NULL, 3000);
            "#,
        )
        .execute(&pool)
        .await
        .expect("seed distinct artwork variants");

        let result = ArtworkCardinalityHealthCheck::new(pool)
            .check()
            .await
            .expect("run artwork cardinality health check");

        assert_eq!(result.status, HealthCheckStatus::Pass);
    }

    #[tokio::test]
    async fn warns_for_exact_duplicates_owned_by_each_projected_metadata_level() {
        let pool = fixture_pool().await;
        sqlx::raw_sql(
            r#"
            INSERT INTO "Artwork" (
                "Id", "ArtworkKind", "Path", "SourcePath", "ShowMetadataId"
            ) VALUES
                (1, 0, '/private/show.jpg', 'provider-show', 1000),
                (2, 0, '/private/show.jpg', 'provider-show', 1000);
            INSERT INTO "Artwork" (
                "Id", "ArtworkKind", "Path", "SourcePath", "SeasonMetadataId"
            ) VALUES
                (3, 0, NULL, 'https://secret.example/season.jpg', 2000),
                (4, 0, NULL, 'https://secret.example/season.jpg', 2000);
            INSERT INTO "Artwork" (
                "Id", "ArtworkKind", "Path", "SourcePath", "EpisodeMetadataId"
            ) VALUES
                (5, 1, '/private/episode.jpg', NULL, 3000),
                (6, 1, '/private/episode.jpg', NULL, 3000),
                (7, 1, '/private/episode.jpg', 'provider-variant', 3000);
            "#,
        )
        .execute(&pool)
        .await
        .expect("seed duplicate artwork identities");

        let result = ArtworkCardinalityHealthCheck::new(pool)
            .check()
            .await
            .expect("run artwork cardinality health check");

        assert_eq!(result.status, HealthCheckStatus::Warning);
        assert!(result.message.contains("1 duplicate show artwork row(s)"));
        assert!(result.message.contains("1 duplicate season artwork row(s)"));
        assert!(result
            .message
            .contains("1 duplicate episode artwork row(s)"));
        assert!(!result.message.contains("/private"));
        assert!(!result.message.contains("secret.example"));
        assert!(!result.message.contains("provider-"));
        assert_eq!(
            result.link.as_ref().map(|link| link.link.as_str()),
            Some("/media/addon-lists")
        );
    }

    #[tokio::test]
    async fn warns_when_projection_rows_do_not_match_distinct_hierarchy_episodes() {
        let pool = fixture_pool().await;
        pool.execute(
            r#"INSERT INTO "AddonMediaListItem" (
                   "Id", "AddonMediaListId", "ParentProviderId", "Published"
               ) VALUES (11, 1, NULL, 1)"#,
        )
        .await
        .expect("seed second published item");
        pool.execute(
            r#"INSERT INTO "AddonMediaListItemEpisode" (
                   "AddonMediaListItemId", "EpisodeId"
               ) VALUES (11, 300)"#,
        )
        .await
        .expect("seed duplicate episode projection");

        let result = ArtworkCardinalityHealthCheck::new(pool)
            .check()
            .await
            .expect("run artwork cardinality health check");

        assert_eq!(result.status, HealthCheckStatus::Warning);
        assert!(result
            .message
            .contains("2 published add-on list episode item(s)"));
        assert!(result.message.contains("2 projection row(s)"));
        assert!(result.message.contains("1 distinct projected episode(s)"));
        assert!(result
            .message
            .contains("2 complete hierarchy projection(s)"));
        assert!(result.message.contains("1 hierarchy episode(s)"));
    }
}
