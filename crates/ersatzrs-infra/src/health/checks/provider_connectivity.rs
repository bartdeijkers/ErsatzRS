use super::super::{
    async_trait, load_config, provider_label, source_name, Db, HealthCheck, HealthCheckError,
    HealthCheckResult, HealthCheckStatus, Pool, ProviderConnectivityResult, Result,
    PROVIDER_CONNECTIVITY_KEY,
};

pub(crate) struct ProviderConnectivityHealthCheck {
    pool: Pool<Db>,
}

impl ProviderConnectivityHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for ProviderConnectivityHealthCheck {
    fn title(&self) -> &'static str {
        "Provider Connectivity"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let Some(value) = load_config(&self.pool, PROVIDER_CONNECTIVITY_KEY).await? else {
            return Ok(HealthCheckResult::not_applicable(self.title()));
        };
        let results: Vec<ProviderConnectivityResult> = serde_json::from_str(&value)
            .map_err(|error| HealthCheckError::Serialization(error.to_string()))?;

        if results.is_empty() {
            return Ok(HealthCheckResult::not_applicable(self.title()));
        }

        let unreachable: Vec<&ProviderConnectivityResult> =
            results.iter().filter(|result| !result.reachable).collect();
        if unreachable.is_empty() {
            return Ok(HealthCheckResult::pass(self.title()));
        }

        let mut descriptions: Vec<String> = Vec::with_capacity(unreachable.len());
        for result in &unreachable {
            let name = source_name(&self.pool, &result.provider, result.media_source_id)
                .await?
                .unwrap_or_else(|| format!("MediaSource {}", result.media_source_id));
            descriptions.push(format!(
                "{} ({name}: {})",
                provider_label(&result.provider),
                result.url
            ));
        }

        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Warning,
            format!(
                "{} provider connection{} unreachable on the last poll: {}",
                unreachable.len(),
                if unreachable.len() > 1 {
                    "s are"
                } else {
                    " is"
                },
                descriptions.join(", ")
            ),
            format!(
                "{} provider connection{} unreachable",
                unreachable.len(),
                if unreachable.len() > 1 {
                    "s are"
                } else {
                    " is"
                }
            ),
        ))
    }
}
