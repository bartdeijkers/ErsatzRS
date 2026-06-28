use super::super::{
    async_trait, format_versions, DatabaseSchemaStatus, HealthCheck, HealthCheckResult,
    HealthCheckStatus, Result,
};

pub(crate) struct MigrationChecksumHealthCheck {
    schema_status: DatabaseSchemaStatus,
}

impl MigrationChecksumHealthCheck {
    pub(crate) fn new(schema_status: DatabaseSchemaStatus) -> Self {
        Self { schema_status }
    }
}

#[async_trait]
impl HealthCheck for MigrationChecksumHealthCheck {
    fn title(&self) -> &'static str {
        "Database Migration Checksum"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        if self.schema_status.mismatched_migrations.is_empty() {
            return Ok(HealthCheckResult::not_applicable(self.title()));
        }

        let versions = format_versions(&self.schema_status.mismatched_migrations);
        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Fail,
            format!(
                "Database migration checksum mismatch for version(s): {versions}. The database was created by a build whose migration text differs from this build."
            ),
            "Database migration checksum mismatch",
        ))
    }
}
