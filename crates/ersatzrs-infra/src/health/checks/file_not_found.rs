use super::super::{
    async_trait, media_state_result, Db, HealthCheck, HealthCheckResult, Pool, Result,
    STATUS_FILE_NOT_FOUND,
};

pub(crate) struct FileNotFoundHealthCheck {
    pool: Pool<Db>,
}

impl FileNotFoundHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for FileNotFoundHealthCheck {
    fn title(&self) -> &'static str {
        "File Not Found"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        media_state_result(
            &self.pool,
            self.title(),
            STATUS_FILE_NOT_FOUND,
            "There are {count} items that do not exist on disk, including the following: {items}",
            "There are {count} items that do not exist on disk",
            Some("media/trash"),
        )
        .await
    }
}
