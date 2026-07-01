use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckResult, HealthCheckStatus, Pool, Result,
    Row,
};
use std::path::Path;

pub(crate) struct ZeroDurationHealthCheck {
    pool: Pool<Db>,
}

impl ZeroDurationHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for ZeroDurationHealthCheck {
    fn title(&self) -> &'static str {
        "Zero Duration"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let rows = sqlx::query(
            r#"SELECT mf."Path", mv."ImageId"
               FROM "MediaVersion" mv
               INNER JOIN "MediaFile" mf ON mf."MediaVersionId" = mv."Id"
               WHERE (mv."Duration" = '00:00:00'
                  OR mv."Duration" LIKE '00:00:00.%')
                 AND mv."RemoteStreamId" IS NULL
               ORDER BY mf."Path", mf."Id""#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;

        let warning_rows = rows
            .iter()
            .filter(|row| {
                let image_id = row.try_get::<Option<i64>, _>("ImageId").ok().flatten();
                let path = row.try_get::<Option<String>, _>("Path").ok().flatten();
                image_id.is_none() || !is_static_image_path(path.as_deref())
            })
            .collect::<Vec<_>>();

        if warning_rows.is_empty() {
            return Ok(HealthCheckResult::pass(self.title()));
        }

        let paths = warning_rows
            .iter()
            .take(5)
            .filter_map(|row| row.try_get::<Option<String>, _>("Path").ok().flatten())
            .collect::<Vec<_>>();

        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Warning,
            format!(
                "There are {} files with zero duration, including the following: {}",
                warning_rows.len(),
                paths.join(", ")
            ),
            format!("There are {} files with zero duration", warning_rows.len()),
        ))
    }
}

fn is_static_image_path(path: Option<&str>) -> bool {
    let Some(path) = path.map(str::trim).filter(|path| !path.is_empty()) else {
        return false;
    };
    let path = path
        .split_once('#')
        .map_or(path, |(before_fragment, _)| before_fragment);
    let path = path
        .split_once('?')
        .map_or(path, |(before_query, _)| before_query);
    let Some(extension) = Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
    else {
        return false;
    };

    // There is no persisted frame-count/animated flag for image media versions,
    // so only formats that are conventionally still images are ignored here.
    // Animated-capable image paths such as .gif, .webp, .apng, and .avif remain
    // warning candidates until probing records a non-zero media duration.
    matches!(
        extension.as_str(),
        "jpg" | "jpeg" | "png" | "bmp" | "tif" | "tiff" | "heic" | "heif" | "ico"
    )
}
