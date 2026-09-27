use super::super::{
    async_trait, load_config, provider_label, source_name, sqlx_error, Db, HealthCheck,
    HealthCheckError, HealthCheckResult, HealthCheckStatus, Pool, ProviderConnectivityResult,
    Result, PROVIDER_CONNECTIVITY_KEY,
};

pub(crate) struct ProviderConnectivityHealthCheck {
    pool: Pool<Db>,
}

impl ProviderConnectivityHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }

    async fn plex_account_warning(&self) -> Result<Option<HealthCheckResult>> {
        let (total, missing): (i64, i64) = sqlx::query_as(
            r#"SELECT COUNT(*), COALESCE(SUM("MissingSince" IS NOT NULL), 0)
               FROM "PlexMediaSource""#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(sqlx_error)?;
        if total == 0 {
            return Ok(None);
        }
        let authorized: bool = sqlx::query_scalar(
            r#"SELECT COUNT(*) = 2 FROM "ConfigElement"
               WHERE "Key" IN ('plex.client_identifier', 'plex.user_auth_token')
                 AND LENGTH(TRIM("Value")) > 0"#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(sqlx_error)?;
        if authorized && missing == 0 {
            return Ok(None);
        }
        let message = if authorized {
            format!("{missing} Plex server(s) are missing from the account. Their content is retained and scans are paused until they return. Review Plex media sources.")
        } else {
            "Plex account authorization is missing. Sign in again from Plex media sources; synchronized content is retained.".to_owned()
        };
        Ok(Some(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Warning,
            message,
            "Plex account requires attention",
        )))
    }
}

#[async_trait]
impl HealthCheck for ProviderConnectivityHealthCheck {
    fn title(&self) -> &'static str {
        "Provider Connectivity"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        if let Some(warning) = self.plex_account_warning().await? {
            return Ok(warning);
        }
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
