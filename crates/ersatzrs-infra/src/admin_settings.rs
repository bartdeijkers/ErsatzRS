//! Typed persistence operations for the admin settings route family.
use crate::repositories::SqlxResultExt as _;

use ersatzrs_core::repositories::{RepositoryError, Result};
use serde::{Deserialize, Serialize};

use crate::db::DbPool;
use crate::repositories::common::map_data_access_error;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
pub struct SettingsSelectOption {
    #[sqlx(rename = "Id")]
    pub id: i64,
    #[sqlx(rename = "Name")]
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsOptionKind {
    FfmpegProfile,
    Watermark,
    FallbackFiller,
    CustomResolution,
}

/// Load one settings editor's typed select options.
///
/// # Errors
///
/// Returns a repository database or decode error when the query or typed row
/// projection fails.
pub async fn load_select_options(
    pool: &DbPool,
    kind: SettingsOptionKind,
) -> Result<Vec<SettingsSelectOption>> {
    let sql = match kind {
        SettingsOptionKind::FfmpegProfile => {
            r#"SELECT "Id" AS "Id", COALESCE("Name", '') AS "Name" FROM "FFmpegProfile" ORDER BY "Name", "Id""#
        }
        SettingsOptionKind::Watermark => {
            r#"SELECT "Id" AS "Id", COALESCE("Name", '') AS "Name" FROM "ChannelWatermark" ORDER BY "Name", "Id""#
        }
        SettingsOptionKind::FallbackFiller => {
            r#"SELECT "Id" AS "Id", COALESCE("Name", '') AS "Name" FROM "FillerPreset" WHERE "FillerKind" = 0 ORDER BY "Name", "Id""#
        }
        SettingsOptionKind::CustomResolution => {
            r#"SELECT "Id" AS "Id", COALESCE("Name", '') AS "Name" FROM "Resolution" WHERE "IsCustom" = 1 ORDER BY "Height", "Width", "Id""#
        }
    };

    sqlx::query_as::<_, SettingsSelectOption>(sql)
        .fetch_all(pool)
        .await
        .map_err(|error| map_data_access_error("admin settings option", &error))
}

/// Load an optional persisted configuration string.
///
/// # Errors
///
/// Returns a repository database or decode error when the query fails.
pub async fn load_optional_string(pool: &DbPool, key: &str) -> Result<Option<String>> {
    sqlx::query_scalar::<_, String>(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
        .bind(key)
        .fetch_optional(pool)
        .await
        .map_err(|error| map_data_access_error("admin setting", &error))
}

/// Load a string, using `default` only when the key is absent.
///
/// # Errors
///
/// Returns a repository database or decode error when persistence is invalid
/// or unavailable.
pub async fn load_string(pool: &DbPool, key: &str, default: &str) -> Result<String> {
    Ok(load_optional_string(pool, key)
        .await?
        .unwrap_or_else(|| default.to_owned()))
}

/// Load an integer, using `default` only when the key is absent.
///
/// # Errors
///
/// Returns [`RepositoryError::Decode`] for an invalid stored integer, or a
/// repository database error when the query fails.
pub async fn load_i32(pool: &DbPool, key: &str, default: i32) -> Result<i32> {
    match load_optional_string(pool, key).await? {
        None => Ok(default),
        Some(value) => value.parse::<i32>().map_err(|_| RepositoryError::Decode {
            kind: "admin setting",
            value: format!("{key}={value}"),
        }),
    }
}

/// Load an optional integer; absent and blank values map to `None`.
///
/// # Errors
///
/// Returns [`RepositoryError::Decode`] for an invalid stored integer, or a
/// repository database error when the query fails.
pub async fn load_optional_i64(pool: &DbPool, key: &str) -> Result<Option<i64>> {
    match load_optional_string(pool, key).await? {
        None => Ok(None),
        Some(value) if value.trim().is_empty() => Ok(None),
        Some(value) => value
            .parse::<i64>()
            .map(Some)
            .map_err(|_| RepositoryError::Decode {
                kind: "admin setting",
                value: format!("{key}={value}"),
            }),
    }
}

/// Load a boolean, using `default` only when the key is absent.
///
/// # Errors
///
/// Returns [`RepositoryError::Decode`] for an invalid stored boolean, or a
/// repository database error when the query fails.
pub async fn load_bool(pool: &DbPool, key: &str, default: bool) -> Result<bool> {
    match load_optional_string(pool, key).await? {
        None => Ok(default),
        Some(value) => match value.trim().to_ascii_lowercase().as_str() {
            "true" | "1" => Ok(true),
            "false" | "0" => Ok(false),
            _ => Err(RepositoryError::Decode {
                kind: "admin setting",
                value: format!("{key}={value}"),
            }),
        },
    }
}

/// Insert or update one persisted configuration value.
///
/// # Errors
///
/// Returns a repository database error when the update or insert fails.
pub async fn save_config_value(pool: &DbPool, key: &str, value: &str) -> Result<()> {
    let updated = sqlx::query(r#"UPDATE "ConfigElement" SET "Value" = ? WHERE "Key" = ?"#)
        .bind(value)
        .bind(key)
        .execute(pool)
        .await
        .repository()?;
    if updated.rows_affected() == 0 {
        sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
            .bind(key)
            .bind(value)
            .execute(pool)
            .await
            .repository()?;
    }
    Ok(())
}

/// Persist an optional integer, deleting the key for `None`.
///
/// # Errors
///
/// Returns a repository database error when the write fails.
pub async fn save_optional_i64(pool: &DbPool, key: &str, value: Option<i64>) -> Result<()> {
    if let Some(value) = value {
        save_config_value(pool, key, &value.to_string()).await
    } else {
        sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key" = ?"#)
            .bind(key)
            .execute(pool)
            .await
            .repository()?;
        Ok(())
    }
}

/// Load every playout identifier used by settings write-back side effects.
///
/// # Errors
///
/// Returns a repository database or decode error when the query fails.
pub async fn load_playout_ids(pool: &DbPool) -> Result<Vec<i64>> {
    sqlx::query_scalar(r#"SELECT "Id" FROM "Playout" ORDER BY "Id""#)
        .fetch_all(pool)
        .await
        .map_err(|error| map_data_access_error("playout id", &error))
}

/// Create a unique custom resolution.
///
/// # Errors
///
/// Returns [`RepositoryError::Conflict`] for duplicate dimensions, or a
/// repository database/decode error when persistence fails.
pub async fn create_custom_resolution(pool: &DbPool, width: i32, height: i32) -> Result<()> {
    let existing: Option<i64> =
        sqlx::query_scalar(r#"SELECT "Id" FROM "Resolution" WHERE "Width" = ? AND "Height" = ?"#)
            .bind(width)
            .bind(height)
            .fetch_optional(pool)
            .await
            .map_err(|error| map_data_access_error("custom resolution id", &error))?;
    if existing.is_some() {
        return Err(RepositoryError::Conflict(
            "Resolution width and height must be unique".to_owned(),
        ));
    }

    sqlx::query(
        r#"INSERT INTO "Resolution" ("Name", "IsCustom", "Width", "Height") VALUES (?, 1, ?, ?)"#,
    )
    .bind(format!("{width}x{height}"))
    .bind(width)
    .bind(height)
    .execute(pool)
    .await
    .repository()?;
    Ok(())
}

/// Delete a custom resolution and retarget profiles to a built-in resolution.
///
/// # Errors
///
/// Returns [`RepositoryError::NotFound`] when `id` is not a custom resolution,
/// or a repository database/decode error when persistence fails.
pub async fn delete_custom_resolution(pool: &DbPool, id: i64) -> Result<()> {
    let existing: Option<i64> =
        sqlx::query_scalar(r#"SELECT "Id" FROM "Resolution" WHERE "Id" = ? AND "IsCustom" = 1"#)
            .bind(id)
            .fetch_optional(pool)
            .await
            .map_err(|error| map_data_access_error("custom resolution id", &error))?;
    if existing.is_none() {
        return Err(RepositoryError::NotFound {
            kind: "resolution",
            id,
        });
    }

    let replacement: Option<i64> = sqlx::query_scalar(
        r#"SELECT "Id" FROM "Resolution" WHERE "IsCustom" = 0 ORDER BY "Height" DESC, "Width" DESC, "Id" LIMIT 1"#,
    )
    .fetch_optional(pool)
    .await
    .map_err(|error| map_data_access_error("replacement resolution id", &error))?;
    if let Some(replacement_id) = replacement {
        sqlx::query(r#"UPDATE "FFmpegProfile" SET "ResolutionId" = ? WHERE "ResolutionId" = ?"#)
            .bind(replacement_id)
            .bind(id)
            .execute(pool)
            .await
            .repository()?;
    }
    sqlx::query(r#"DELETE FROM "Resolution" WHERE "Id" = ? AND "IsCustom" = 1"#)
        .bind(id)
        .execute(pool)
        .await
        .repository()?;
    Ok(())
}

#[cfg(all(test, not(feature = "mysql")))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn closed_pool_is_reported_as_repository_database_error() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .expect("test pool");
        pool.close().await;

        assert!(matches!(
            load_string(&pool, "test", "default").await,
            Err(RepositoryError::Unavailable { .. })
        ));
    }

    #[tokio::test]
    async fn invalid_setting_value_is_a_decode_error() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .expect("test pool");
        sqlx::query(r#"CREATE TABLE "ConfigElement" ("Key" TEXT, "Value" TEXT)"#)
            .execute(&pool)
            .await
            .expect("config table");
        sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES ('number', 'invalid')"#)
            .execute(&pool)
            .await
            .expect("config value");

        assert!(matches!(
            load_i32(&pool, "number", 7).await,
            Err(RepositoryError::Decode { .. })
        ));
    }
}
