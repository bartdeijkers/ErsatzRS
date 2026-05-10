use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Status values reported by ErsatzTV-compatible health checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthCheckStatus {
    /// The check completed successfully and found no issue.
    Pass,
    /// The check found a fatal condition.
    Fail,
    /// The check found a non-fatal condition.
    Warning,
    /// The check found informational guidance.
    Info,
    /// The check does not apply to this installation.
    NotApplicable,
}

/// Link associated with a health check result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthCheckLink {
    /// Relative or absolute URL for more context or a remediation page.
    pub link: String,
}

impl HealthCheckLink {
    /// Create a health check link.
    #[must_use]
    pub fn new(link: impl Into<String>) -> Self {
        Self { link: link.into() }
    }
}

/// Result returned by one health check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthCheckResult {
    /// Human-readable check title.
    pub title: String,
    /// Check status.
    pub status: HealthCheckStatus,
    /// Full message for detail views and troubleshooting exports.
    pub message: String,
    /// Short message for badges and compact displays.
    pub brief_message: String,
    /// Optional remediation link.
    pub link: Option<HealthCheckLink>,
}

impl HealthCheckResult {
    /// Build a result without a remediation link.
    #[must_use]
    pub fn new(
        title: impl Into<String>,
        status: HealthCheckStatus,
        message: impl Into<String>,
        brief_message: impl Into<String>,
    ) -> Self {
        Self {
            title: title.into(),
            status,
            message: message.into(),
            brief_message: brief_message.into(),
            link: None,
        }
    }

    /// Attach a remediation link.
    #[must_use]
    pub fn with_link(mut self, link: impl Into<String>) -> Self {
        self.link = Some(HealthCheckLink::new(link));
        self
    }

    /// Build a pass result for a title.
    #[must_use]
    pub fn pass(title: impl Into<String>) -> Self {
        Self::new(title, HealthCheckStatus::Pass, "", "")
    }

    /// Build a not-applicable result for a title.
    #[must_use]
    pub fn not_applicable(title: impl Into<String>) -> Self {
        Self::new(title, HealthCheckStatus::NotApplicable, "", "")
    }

    /// Build the `ErsatzTV` fallback result used when a check itself fails.
    #[must_use]
    pub fn check_failure(title: impl Into<String>) -> Self {
        Self::new(
            title,
            HealthCheckStatus::Fail,
            "Health check failure; see logs",
            "Health check failure",
        )
    }
}

/// Warning and error counts from a completed health run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct HealthCheckSummary {
    /// Number of warning results.
    pub warnings: u32,
    /// Number of failed results.
    pub errors: u32,
}

impl HealthCheckSummary {
    /// Count warnings and failures in a health result set.
    #[must_use]
    pub fn from_results(results: &[HealthCheckResult]) -> Self {
        let warnings = results
            .iter()
            .filter(|result| result.status == HealthCheckStatus::Warning)
            .count()
            .try_into()
            .unwrap_or(u32::MAX);
        let errors = results
            .iter()
            .filter(|result| result.status == HealthCheckStatus::Fail)
            .count()
            .try_into()
            .unwrap_or(u32::MAX);
        Self { warnings, errors }
    }
}

/// Error raised by a health check implementation.
#[derive(Debug, thiserror::Error)]
pub enum HealthCheckError {
    /// Repository or database query failed.
    #[error("database health check failed: {0}")]
    Database(String),
    /// JSON serialization or deserialization failed.
    #[error("health check serialization failed: {0}")]
    Serialization(String),
    /// An external process failed.
    #[error("process health check failed: {0}")]
    Process(String),
}

/// Common result type for health check implementations.
pub type Result<T> = std::result::Result<T, HealthCheckError>;

/// One asynchronous health check.
#[async_trait]
pub trait HealthCheck: Send + Sync {
    /// Human-readable title.
    fn title(&self) -> &'static str;

    /// Execute the health check.
    async fn check(&self) -> Result<HealthCheckResult>;
}

/// Runs a set of health checks and produces an ErsatzTV-compatible summary.
#[derive(Default)]
pub struct HealthCheckService {
    checks: Vec<Box<dyn HealthCheck>>,
}

impl HealthCheckService {
    /// Create a service from concrete health checks.
    #[must_use]
    pub fn new(checks: Vec<Box<dyn HealthCheck>>) -> Self {
        Self { checks }
    }

    /// Execute all health checks.
    ///
    /// If an individual check errors, the runner returns the same fallback
    /// failure row used by `ErsatzTV`'s `HealthCheckService`.
    pub async fn perform_health_checks(&self) -> Vec<HealthCheckResult> {
        let mut results = Vec::with_capacity(self.checks.len());
        for check in &self.checks {
            let result = check
                .check()
                .await
                .unwrap_or_else(|_| HealthCheckResult::check_failure(check.title()));
            results.push(result);
        }
        results
    }

    /// Execute checks and summarize warning/failure counts.
    pub async fn summarize(&self) -> HealthCheckSummary {
        let results = self.perform_health_checks().await;
        HealthCheckSummary::from_results(&results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StaticCheck(HealthCheckResult);

    #[async_trait]
    impl HealthCheck for StaticCheck {
        fn title(&self) -> &'static str {
            "Static"
        }

        async fn check(&self) -> Result<HealthCheckResult> {
            Ok(self.0.clone())
        }
    }

    struct FailingCheck;

    #[async_trait]
    impl HealthCheck for FailingCheck {
        fn title(&self) -> &'static str {
            "Exploding"
        }

        async fn check(&self) -> Result<HealthCheckResult> {
            Err(HealthCheckError::Process("boom".to_owned()))
        }
    }

    #[tokio::test]
    async fn service_counts_warnings_and_failures() {
        let service = HealthCheckService::new(vec![
            Box::new(StaticCheck(HealthCheckResult::new(
                "Warn",
                HealthCheckStatus::Warning,
                "warning",
                "warning",
            ))),
            Box::new(StaticCheck(HealthCheckResult::pass("Pass"))),
            Box::new(FailingCheck),
        ]);

        let results = service.perform_health_checks().await;
        assert_eq!(results.len(), 3);
        assert_eq!(HealthCheckSummary::from_results(&results).warnings, 1);
        assert_eq!(HealthCheckSummary::from_results(&results).errors, 1);
        assert_eq!(results[2].title, "Exploding");
        assert_eq!(results[2].brief_message, "Health check failure");
    }
}
