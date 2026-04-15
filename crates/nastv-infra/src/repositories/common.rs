use chrono::{DateTime, Duration, Utc};
use nastv_core::repositories::{RepositoryError, Result};
use serde::de::DeserializeOwned;
use sqlx::{Pool, Sqlite};

pub fn decode_enum<T>(kind: &'static str, value: &str) -> Result<T>
where
    T: DeserializeOwned,
{
    serde_json::from_str(&format!("\"{value}\""))
        .map_err(|source| decode_message(kind, source.to_string()))
}

pub fn decode_datetime(kind: &'static str, value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|parsed| parsed.with_timezone(&Utc))
        .map_err(|source| decode_message(kind, source.to_string()))
}

pub fn decode_optional_datetime(
    kind: &'static str,
    value: Option<String>,
) -> Result<Option<DateTime<Utc>>> {
    value.map(|inner| decode_datetime(kind, &inner)).transpose()
}

#[must_use]
pub fn encode_datetime(value: DateTime<Utc>) -> String {
    value.to_rfc3339()
}

#[must_use]
pub fn encode_optional_datetime(value: Option<DateTime<Utc>>) -> Option<String> {
    value.map(encode_datetime)
}

#[must_use]
pub fn encode_duration(value: Duration) -> i64 {
    value.num_milliseconds()
}

#[must_use]
pub fn encode_optional_duration(value: Option<Duration>) -> Option<i64> {
    value.map(encode_duration)
}

#[must_use]
pub fn decode_duration(value: i64) -> Duration {
    Duration::milliseconds(value)
}

#[must_use]
pub fn decode_optional_duration(value: Option<i64>) -> Option<Duration> {
    value.map(decode_duration)
}

pub fn decode_message(kind: &'static str, message: String) -> RepositoryError {
    RepositoryError::Database(sqlx::Error::Decode(
        format!("failed to decode {kind}: {message}").into(),
    ))
}

pub fn not_found(kind: &'static str, id: i64, error: sqlx::Error) -> RepositoryError {
    match error {
        sqlx::Error::RowNotFound => RepositoryError::NotFound { kind, id },
        other => RepositoryError::Database(other),
    }
}

pub async fn replace_ids(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    delete_sql: &str,
    insert_sql: &str,
    parent_id: i64,
    ids: &[i64],
) -> Result<()> {
    sqlx::query(delete_sql)
        .bind(parent_id)
        .execute(&mut **tx)
        .await?;

    for id in ids {
        sqlx::query(insert_sql)
            .bind(parent_id)
            .bind(id)
            .execute(&mut **tx)
            .await?;
    }

    Ok(())
}

pub async fn load_ids(pool: &Pool<Sqlite>, sql: &str, parent_id: i64) -> Result<Vec<i64>> {
    sqlx::query_scalar::<_, i64>(sql)
        .bind(parent_id)
        .fetch_all(pool)
        .await
        .map_err(RepositoryError::from)
}
