use super::super::{
    async_trait, format_versions, DatabaseSchemaStatus, HealthCheck, HealthCheckResult,
    HealthCheckStatus, Result,
};

pub(crate) struct DowngradeHealthCheck {
    schema_status: DatabaseSchemaStatus,
}

impl DowngradeHealthCheck {
    pub(crate) fn new(schema_status: DatabaseSchemaStatus) -> Self {
        Self { schema_status }
    }
}

#[async_trait]
impl HealthCheck for DowngradeHealthCheck {
    fn title(&self) -> &'static str {
        "ErsatzTV Downgrade"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        if self.schema_status.unknown_migrations.is_empty() {
            return Ok(HealthCheckResult::not_applicable(self.title()));
        }

        let versions = format_versions(&self.schema_status.unknown_migrations);
        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Fail,
            format!(
                "Database contains migration version(s) unknown to this build: {versions}. Downgrading is NOT supported and WILL impact stability."
            ),
            "Downgrade detected",
        )
        .with_link("https://ersatztv.org/docs/installation/#downgrading"))
    }
}
