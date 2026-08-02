use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckResult, HealthCheckStatus, Pool, Result,
    Row,
};

pub(crate) struct LocalTelevisionIdentityHealthCheck {
    pool: Pool<Db>,
}

impl LocalTelevisionIdentityHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for LocalTelevisionIdentityHealthCheck {
    fn title(&self) -> &'static str {
        "Local television identity"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let rows = sqlx::query(
            r#"SELECT season."ShowId", season."Id" AS "SeasonId",
                      show_folder."Id" AS "ShowFolderId"
               FROM "Season" season
               JOIN "Episode" episode ON episode."SeasonId" = season."Id"
               JOIN "MediaItem" episode_media ON episode_media."Id" = episode."Id"
               JOIN "LibraryPath" library_path ON library_path."Id" = episode_media."LibraryPathId"
               JOIN "LocalLibrary" local_library ON local_library."Id" = library_path."LibraryId"
               JOIN "MediaVersion" version ON version."EpisodeId" = episode."Id"
               JOIN "MediaFile" file ON file."MediaVersionId" = version."Id"
               JOIN "LibraryFolder" season_folder ON season_folder."Id" = file."LibraryFolderId"
               JOIN "LibraryFolder" show_folder ON show_folder."Id" = season_folder."ParentId"
               ORDER BY season."ShowId", season."Id", show_folder."Id""#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;

        let mut show_folders =
            std::collections::BTreeMap::<i64, std::collections::BTreeSet<i64>>::new();
        let mut season_folders =
            std::collections::BTreeMap::<i64, std::collections::BTreeSet<i64>>::new();
        for row in rows {
            let show_id = row.try_get("ShowId").map_err(sqlx_error)?;
            let season_id = row.try_get("SeasonId").map_err(sqlx_error)?;
            let folder_id = row.try_get("ShowFolderId").map_err(sqlx_error)?;
            show_folders.entry(show_id).or_default().insert(folder_id);
            season_folders
                .entry(season_id)
                .or_default()
                .insert(folder_id);
        }

        let shows = show_folders
            .into_iter()
            .filter_map(|(id, folders)| (folders.len() > 1).then_some(id))
            .collect::<Vec<_>>();
        let seasons = season_folders
            .into_iter()
            .filter_map(|(id, folders)| (folders.len() > 1).then_some(id))
            .collect::<Vec<_>>();
        if shows.is_empty() && seasons.is_empty() {
            return Ok(HealthCheckResult::pass(self.title()));
        }

        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Warning,
            format!(
                "Local television folder provenance conflicts remain for show IDs [{}] and season IDs [{}]. Run a full scan of the affected local library to reconcile them.",
                shows.iter().map(i64::to_string).collect::<Vec<_>>().join(", "),
                seasons.iter().map(i64::to_string).collect::<Vec<_>>().join(", ")
            ),
            format!(
                "{} local show and {} local season identity conflict(s)",
                shows.len(),
                seasons.len()
            ),
        ))
    }
}

#[cfg(all(test, not(feature = "mysql")))]
mod tests {
    use super::*;
    use sqlx::Executor;

    async fn test_db() -> Pool<Db> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("create health test database");
        pool.execute("PRAGMA foreign_keys = ON")
            .await
            .expect("enable foreign keys");
        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .expect("run migrations");
        pool
    }

    async fn seed_conflict(pool: &Pool<Db>) {
        pool.execute(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
            .await
            .unwrap();
        pool.execute(
            r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
               VALUES (1, 'Local TV', 2, 1)"#,
        )
        .await
        .unwrap();
        pool.execute(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (1)"#)
            .await
            .unwrap();
        pool.execute(
            r#"INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path") VALUES (1, 1, '/media')"#,
        )
        .await
        .unwrap();
        for id in 1..=4 {
            sqlx::query(
                r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (?, 1, 0)"#,
            )
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
        }
        pool.execute(r#"INSERT INTO "Show" ("Id") VALUES (1)"#)
            .await
            .unwrap();
        pool.execute(r#"INSERT INTO "Season" ("Id", "ShowId", "SeasonNumber") VALUES (2, 1, 1)"#)
            .await
            .unwrap();
        pool.execute(r#"INSERT INTO "Episode" ("Id", "SeasonId") VALUES (3, 2), (4, 2)"#)
            .await
            .unwrap();
        pool.execute(
            r#"INSERT INTO "LibraryFolder" ("Id", "LibraryPathId", "Path")
               VALUES (10, 1, '/media/original'), (11, 1, '/media/reboot')"#,
        )
        .await
        .unwrap();
        pool.execute(
            r#"INSERT INTO "LibraryFolder" ("Id", "LibraryPathId", "ParentId", "Path")
               VALUES (12, 1, 10, '/media/original/Season 01'),
                      (13, 1, 11, '/media/reboot/Season 01')"#,
        )
        .await
        .unwrap();
        for (version, episode, folder) in [(20, 3, 12), (21, 4, 13)] {
            sqlx::query(
                r#"INSERT INTO "MediaVersion"
                   ("Id", "DateAdded", "DateUpdated", "Duration", "Height", "Width", "VideoScanKind", "EpisodeId")
                   VALUES (?, '2024-01-01', '2024-01-01', '00:20:00', 1080, 1920, 0, ?)"#,
            )
            .bind(version)
            .bind(episode)
            .execute(pool)
            .await
            .unwrap();
            sqlx::query(
                r#"INSERT INTO "MediaFile" ("LibraryFolderId", "MediaVersionId", "Path", "PathHash")
                   VALUES (?, ?, ?, ?)"#,
            )
            .bind(folder)
            .bind(version)
            .bind(format!("/media/e{episode}.mkv"))
            .bind(format!("hash-{episode}"))
            .execute(pool)
            .await
            .unwrap();
        }
    }

    #[tokio::test]
    async fn warns_for_cross_folder_show_and_season() {
        let pool = test_db().await;
        seed_conflict(&pool).await;
        let result = LocalTelevisionIdentityHealthCheck::new(pool)
            .check()
            .await
            .unwrap();
        assert_eq!(result.status, HealthCheckStatus::Warning);
        assert!(result.message.contains("show IDs [1]"));
        assert!(result.message.contains("season IDs [2]"));
    }

    #[tokio::test]
    async fn passes_after_the_hierarchy_is_separated() {
        let pool = test_db().await;
        seed_conflict(&pool).await;
        sqlx::query(r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (5, 1, 0), (6, 1, 0)"#)
            .execute(&pool)
            .await
            .unwrap();
        pool.execute(r#"INSERT INTO "Show" ("Id") VALUES (5)"#)
            .await
            .unwrap();
        pool.execute(r#"INSERT INTO "Season" ("Id", "ShowId", "SeasonNumber") VALUES (6, 5, 1)"#)
            .await
            .unwrap();
        pool.execute(r#"UPDATE "Episode" SET "SeasonId" = 6 WHERE "Id" = 4"#)
            .await
            .unwrap();

        let result = LocalTelevisionIdentityHealthCheck::new(pool)
            .check()
            .await
            .unwrap();
        assert_eq!(result.status, HealthCheckStatus::Pass);
    }
}
