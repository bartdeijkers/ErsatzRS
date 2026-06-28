use super::super::{
    async_trait, DatabaseSchemaStatus, HealthCheck, HealthCheckResult, HealthCheckStatus, Result,
};

pub(crate) struct DirtyMigrationHealthCheck {
    schema_status: DatabaseSchemaStatus,
}

impl DirtyMigrationHealthCheck {
    pub(crate) fn new(schema_status: DatabaseSchemaStatus) -> Self {
        Self { schema_status }
    }
}

#[async_trait]
impl HealthCheck for DirtyMigrationHealthCheck {
    fn title(&self) -> &'static str {
        "Database Migration"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let Some(version) = self.schema_status.dirty_version else {
            return Ok(HealthCheckResult::not_applicable(self.title()));
        };

        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Fail,
            format!(
                "Database migration version {version} was partially applied. Restore from backup or rerun a build that can complete that migration before continuing."
            ),
            "Database migration is partially applied",
        ))
    }
}
