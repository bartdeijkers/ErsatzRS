use nastv_core::repositories::{RepositoryError, Result};
use serde::de::DeserializeOwned;
use serde::Serialize;
use sqlx::{Pool, Sqlite};

pub fn to_json<T>(value: &T) -> Result<String>
where
    T: Serialize,
{
    serde_json::to_string(value).map_err(decode_error)
}

pub fn from_json<T, S>(kind: &'static str, id: i64, value: S) -> Result<T>
where
    T: DeserializeOwned,
    S: AsRef<str>,
{
    serde_json::from_str(value.as_ref()).map_err(|source| {
        RepositoryError::Database(sqlx::Error::Decode(
            format!("failed to decode {kind} {id}: {source}").into(),
        ))
    })
}

pub fn decode_error(source: serde_json::Error) -> RepositoryError {
    RepositoryError::Database(sqlx::Error::Decode(source.into()))
}

pub fn not_found(kind: &'static str, id: i64, error: sqlx::Error) -> RepositoryError {
    match error {
        sqlx::Error::RowNotFound => RepositoryError::NotFound { kind, id },
        other => RepositoryError::Database(other),
    }
}

pub async fn fetch_data(
    pool: &Pool<Sqlite>,
    table: &str,
    kind: &'static str,
    id: i64,
) -> Result<String> {
    let sql = format!("SELECT data FROM {table} WHERE id = ?");
    sqlx::query_scalar::<_, String>(&sql)
        .bind(id)
        .fetch_one(pool)
        .await
        .map_err(|error| not_found(kind, id, error))
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
