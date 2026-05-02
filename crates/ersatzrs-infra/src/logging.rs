//! Persisted logging-level configuration.

use ersatzrs_domain::{
    CONFIG_KEY_MINIMUM_LOG_LEVEL, CONFIG_KEY_MINIMUM_LOG_LEVEL_HTTP,
    CONFIG_KEY_MINIMUM_LOG_LEVEL_SCANNING, CONFIG_KEY_MINIMUM_LOG_LEVEL_SCHEDULING,
    CONFIG_KEY_MINIMUM_LOG_LEVEL_SEARCHING, CONFIG_KEY_MINIMUM_LOG_LEVEL_STREAMING,
};
use sqlx::{Pool, Row};

use crate::db::Db;

const DEFAULT_LEVEL: &str = "info";

const SUBSYSTEM_KEYS: &[(&str, &[&str])] = &[
    (
        CONFIG_KEY_MINIMUM_LOG_LEVEL_SCANNING,
        &[
            "ersatzrs_scanner",
            "ersatzrs_infra::ffprobe",
            "ersatzrs_infra::plex",
            "ersatzrs_infra::jellyfin",
            "ersatzrs_infra::emby",
        ],
    ),
    (
        CONFIG_KEY_MINIMUM_LOG_LEVEL_SCHEDULING,
        &[
            "ersatzrs_core::scheduling",
            "ersatzrs_core::maintenance",
            "ersatzrs_infra::maintenance",
            "ersatzrs_scheduler",
        ],
    ),
    (
        CONFIG_KEY_MINIMUM_LOG_LEVEL_SEARCHING,
        &[
            "ersatzrs_core::search",
            "ersatzrs_web::admin::server_fns::search",
        ],
    ),
    (
        CONFIG_KEY_MINIMUM_LOG_LEVEL_STREAMING,
        &[
            "ersatzrs_ffmpeg",
            "ersatzrs_infra::hls_worker",
            "ersatzrs_web::transport::iptv",
            "ersatzrs_web::transport::dev_stream",
        ],
    ),
    (
        CONFIG_KEY_MINIMUM_LOG_LEVEL_HTTP,
        &["tower_http", "axum", "ersatzrs_web::host::router"],
    ),
];

/// Build a `tracing_subscriber::EnvFilter` directive string from `ConfigElement`.
///
/// The returned string can be passed to `EnvFilter::try_new`. `base_filter`
/// should usually be the `RUST_LOG` value or `info`.
///
/// # Errors
///
/// Returns an error when the `ConfigElement` query fails.
pub async fn logging_filter_from_database(
    pool: &Pool<Db>,
    base_filter: impl Into<String>,
) -> Result<String, sqlx::Error> {
    let mut directives = Vec::new();
    let base_filter = base_filter.into();

    if let Some(level) = load_level(pool, CONFIG_KEY_MINIMUM_LOG_LEVEL).await? {
        directives.push(level);
    }

    for (key, targets) in SUBSYSTEM_KEYS {
        let Some(level) = load_level(pool, key).await? else {
            continue;
        };
        for target in *targets {
            directives.push(format!("{target}={level}"));
        }
    }

    if directives.is_empty() {
        Ok(base_filter)
    } else if base_filter.trim().is_empty() {
        Ok(directives.join(","))
    } else {
        Ok(format!("{base_filter},{}", directives.join(",")))
    }
}

async fn load_level(pool: &Pool<Db>, key: &str) -> Result<Option<String>, sqlx::Error> {
    let row = sqlx::query(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
        .bind(key)
        .fetch_optional(pool)
        .await?;

    Ok(row
        .and_then(|row| row.try_get::<String, _>("Value").ok())
        .and_then(|value| normalize_log_level(&value)))
}

fn normalize_log_level(value: &str) -> Option<String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "0" | "verbose" | "trace" => Some("trace".to_owned()),
        "1" | "debug" => Some("debug".to_owned()),
        "2" | "information" | "info" => Some("info".to_owned()),
        "3" | "warning" | "warn" => Some("warn".to_owned()),
        "4" | "error" | "5" | "fatal" | "critical" => Some("error".to_owned()),
        "off" => Some("off".to_owned()),
        _ => None,
    }
}

/// Default persisted logging value used during mediaserver bootstrap.
#[must_use]
pub fn default_logging_level() -> &'static str {
    "Information"
}

/// Default fallback directive when no environment or database level exists.
#[must_use]
pub fn default_filter() -> &'static str {
    DEFAULT_LEVEL
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_serilog_levels() {
        assert_eq!(normalize_log_level("Verbose").as_deref(), Some("trace"));
        assert_eq!(normalize_log_level("Information").as_deref(), Some("info"));
        assert_eq!(normalize_log_level("Warning").as_deref(), Some("warn"));
        assert_eq!(normalize_log_level("Fatal").as_deref(), Some("error"));
        assert_eq!(normalize_log_level("2").as_deref(), Some("info"));
        assert_eq!(normalize_log_level("nope"), None);
    }
}
