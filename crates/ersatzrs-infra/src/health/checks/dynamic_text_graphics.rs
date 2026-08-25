use std::fs::{self, File};
use std::path::Path;

use ersatzrs_domain::{GraphicsElementConfig, TextGraphicsConfig, TextGraphicsRenderer};

use crate::dynamic_text::DynamicTextTemplate;

use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckResult, HealthCheckStatus, Pool, Result,
    Row,
};

const TEXT_GRAPHICS_KIND: i32 = 1;
const MAX_TEMPLATE_BYTES: usize = 64 * 1024;
const MIN_REFRESH_INTERVAL_MILLISECONDS: u64 = 250;
const MAX_REFRESH_INTERVAL_MILLISECONDS: u64 = 60_000;

pub(crate) struct DynamicTextGraphicsHealthCheck {
    pool: Pool<Db>,
}

impl DynamicTextGraphicsHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for DynamicTextGraphicsHealthCheck {
    fn title(&self) -> &'static str {
        "Dynamic Text Graphics"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let rows = sqlx::query(
            r#"SELECT "Id", "Name", "Config"
               FROM "GraphicsElement"
               WHERE "Kind" = ?
               ORDER BY "Id""#,
        )
        .bind(TEXT_GRAPHICS_KIND)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;

        let mut findings = Vec::new();
        for row in rows {
            let id = row.try_get::<i64, _>("Id").map_err(sqlx_error)?;
            let name = row
                .try_get::<Option<String>, _>("Name")
                .map_err(sqlx_error)?
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| format!("Graphics element {id}"));
            let config = row
                .try_get::<Option<String>, _>("Config")
                .map_err(sqlx_error)?;

            let Some(config) = decode_text_config(config.as_deref()) else {
                findings.push(format!(
                    "{name}: Config is malformed or does not contain text graphics settings"
                ));
                continue;
            };
            if config.renderer != TextGraphicsRenderer::Drawtext {
                continue;
            }

            findings.extend(validate_drawtext_config(&name, &config));
        }

        if findings.is_empty() {
            return Ok(HealthCheckResult::pass(self.title()));
        }

        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Fail,
            format!(
                "Dynamic text graphics configuration needs attention: {}",
                findings.into_iter().take(10).collect::<Vec<_>>().join("; ")
            ),
            "One or more dynamic text graphics elements are invalid",
        )
        .with_link("/graphics-elements"))
    }
}

fn decode_text_config(value: Option<&str>) -> Option<TextGraphicsConfig> {
    let Some(value) = value.filter(|value| !value.trim().is_empty()) else {
        return Some(TextGraphicsConfig::default());
    };

    match serde_json::from_str::<GraphicsElementConfig>(value).ok()? {
        GraphicsElementConfig::Text(config) => Some(config),
        _ => None,
    }
}

fn validate_drawtext_config(name: &str, config: &TextGraphicsConfig) -> Vec<String> {
    let mut findings = Vec::new();
    match config.text.as_deref() {
        Some(text) if text.trim().is_empty() => {
            findings.push(format!("{name}: template text is blank"));
        }
        None => findings.push(format!("{name}: template text is blank")),
        Some(text) => {
            if text.contains('\0') {
                findings.push(format!("{name}: template text contains a NUL character"));
            }
            if text.len() > MAX_TEMPLATE_BYTES {
                findings.push(format!(
                    "{name}: template text exceeds the 64 KiB runtime limit"
                ));
            }
            if let Err(error) = DynamicTextTemplate::compile(text.to_owned()) {
                findings.push(format!("{name}: template is invalid: {error}"));
            }
        }
    }

    if let Some(font_file) = config
        .drawtext
        .font_file
        .as_deref()
        .filter(|font_file| !font_file.trim().is_empty())
    {
        let path = Path::new(font_file.trim());
        let is_readable_file =
            fs::metadata(path).is_ok_and(|metadata| metadata.is_file()) && File::open(path).is_ok();
        if !is_readable_file {
            findings.push(format!(
                "{name}: configured font_file is not a readable regular file; clear it to use packaged Roboto or select a valid font"
            ));
        }
    }

    let refresh_interval = config.drawtext.refresh_interval_milliseconds;
    if !(MIN_REFRESH_INTERVAL_MILLISECONDS..=MAX_REFRESH_INTERVAL_MILLISECONDS)
        .contains(&refresh_interval)
    {
        findings.push(format!(
            "{name}: refresh_interval_milliseconds is {refresh_interval}; use 250..60000"
        ));
    }
    if config.drawtext.reload_interval_frames == 0 {
        findings.push(format!(
            "{name}: reload_interval_frames must be greater than zero"
        ));
    }

    findings
}

#[cfg(test)]
mod tests {
    use sqlx::sqlite::SqlitePoolOptions;

    use super::*;

    async fn health_pool() -> Pool<Db> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        sqlx::query(
            r#"CREATE TABLE "GraphicsElement" (
                   "Id" INTEGER PRIMARY KEY,
                   "Name" TEXT NULL,
                   "Kind" INTEGER NOT NULL,
                   "Config" TEXT NULL
               )"#,
        )
        .execute(&pool)
        .await
        .expect("graphics element fixture");
        pool
    }

    async fn insert_element(pool: &Pool<Db>, id: i64, name: &str, kind: i32, config: &str) {
        sqlx::query(
            r#"INSERT INTO "GraphicsElement" ("Id", "Name", "Kind", "Config")
               VALUES (?, ?, ?, ?)"#,
        )
        .bind(id)
        .bind(name)
        .bind(kind)
        .bind(config)
        .execute(pool)
        .await
        .expect("graphics element");
    }

    fn drawtext_config(
        text: &str,
        font_file: Option<&str>,
        refresh_interval_milliseconds: u64,
        reload_interval_frames: u32,
    ) -> String {
        serde_json::json!({
            "kind": "text",
            "text": text,
            "renderer": "drawtext",
            "drawtext": {
                "font_file": font_file,
                "refresh_interval_milliseconds": refresh_interval_milliseconds,
                "reload_interval_frames": reload_interval_frames
            }
        })
        .to_string()
    }

    #[tokio::test]
    async fn passes_without_drawtext_elements_and_for_valid_packaged_font_fallback() {
        let pool = health_pool().await;
        let check = DynamicTextGraphicsHealthCheck::new(pool.clone());
        assert_eq!(
            check.check().await.expect("empty health result").status,
            HealthCheckStatus::Pass
        );

        insert_element(
            &pool,
            1,
            "Static text",
            TEXT_GRAPHICS_KIND,
            r#"{"kind":"text","text":"Static","renderer":"drawvg"}"#,
        )
        .await;
        insert_element(
            &pool,
            2,
            "Dynamic text",
            TEXT_GRAPHICS_KIND,
            &drawtext_config("Now playing", Some("  "), 1_000, 1),
        )
        .await;

        assert_eq!(
            check.check().await.expect("valid health result").status,
            HealthCheckStatus::Pass
        );
    }

    #[tokio::test]
    async fn reports_malformed_text_config_actionably() {
        let pool = health_pool().await;
        insert_element(&pool, 1, "Broken JSON", TEXT_GRAPHICS_KIND, r#"{"kind":"#).await;

        let result = DynamicTextGraphicsHealthCheck::new(pool)
            .check()
            .await
            .expect("malformed health result");
        assert_eq!(result.status, HealthCheckStatus::Fail);
        assert!(result.message.contains("Broken JSON: Config is malformed"));
        assert_eq!(
            result.link.as_ref().map(|link| link.link.as_str()),
            Some("/graphics-elements")
        );
    }

    #[tokio::test]
    async fn reports_representative_invalid_drawtext_rows() {
        let pool = health_pool().await;
        insert_element(
            &pool,
            1,
            "Blank",
            TEXT_GRAPHICS_KIND,
            &drawtext_config("  ", None, 1_000, 1),
        )
        .await;

        let oversized_nul_text = format!("{}\0", "x".repeat(MAX_TEMPLATE_BYTES));
        let missing_font = format!("ersatzrs-missing-font-{}.ttf", uuid::Uuid::new_v4());
        insert_element(
            &pool,
            2,
            "Unsafe template",
            TEXT_GRAPHICS_KIND,
            &drawtext_config(
                &oversized_nul_text,
                Some(&missing_font),
                MIN_REFRESH_INTERVAL_MILLISECONDS - 1,
                0,
            ),
        )
        .await;
        insert_element(
            &pool,
            3,
            "Slow refresh",
            TEXT_GRAPHICS_KIND,
            &drawtext_config("Weather", None, MAX_REFRESH_INTERVAL_MILLISECONDS + 1, 1),
        )
        .await;

        let result = DynamicTextGraphicsHealthCheck::new(pool)
            .check()
            .await
            .expect("invalid health result");
        assert_eq!(result.status, HealthCheckStatus::Fail);
        for expected in [
            "Blank: template text is blank",
            "Unsafe template: template text contains a NUL character",
            "Unsafe template: template text exceeds the 64 KiB runtime limit",
            "Unsafe template: configured font_file is not a readable regular file",
            "refresh_interval_milliseconds is 249; use 250..60000",
            "reload_interval_frames must be greater than zero",
            "refresh_interval_milliseconds is 60001; use 250..60000",
        ] {
            assert!(result.message.contains(expected), "{result:?}");
        }
    }
}
