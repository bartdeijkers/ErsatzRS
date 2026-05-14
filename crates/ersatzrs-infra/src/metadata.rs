//! Shared upsert helpers for `ErsatzTV`'s "sparse multi-FK" metadata tables.
//!
//! Tables like `Genre`, `Tag`, `Studio`, `Actor`, `Director`, `Writer`,
//! `Style`, `Mood`, `MusicVideoArtist`, `MetadataGuid`, and `Artwork` are
//! shared across metadata kinds (Movie/Show/Season/Episode/Artist/...) via
//! one nullable `{Kind}MetadataId` column per kind. The DELETE-then-INSERT
//! pattern is identical per table, so this module centralises it behind a
//! `fk_column` parameter and the table identifier.
//!
//! Only the FK-column and table names are interpolated into SQL strings.
//! Both are `&'static str`, never user input, so this is not an injection
//! surface. All variable values use bound parameters.

// Helpers land in slice 1 ahead of their callers (slices 2-8). The
// dead-code allow is removed once every helper is consumed.
#![allow(dead_code)]

use chrono::{DateTime, Utc};
use ersatzrs_core::repositories::Result;
use sqlx::Transaction;

use crate::db::Db;

/// `ErsatzTV` `ArtworkKind` enum values
/// (see `ErsatzTV.Core/Domain/Metadata/ArtworkKind.cs`).
pub mod artwork_kind {
    pub const POSTER: i32 = 0;
    pub const THUMBNAIL: i32 = 1;
    pub const LOGO: i32 = 2;
    pub const FAN_ART: i32 = 3;
    pub const WATERMARK: i32 = 4;
}

/// One person row for `Actor` / `Director` / `Writer`. `role` / `order` are
/// only meaningful for `Actor`; the helper ignores them otherwise.
#[derive(Debug, Clone)]
pub struct PersonRef {
    pub name: String,
    pub role: Option<String>,
    pub order: Option<i32>,
}

impl PersonRef {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            role: None,
            order: None,
        }
    }

    pub fn with_role(name: impl Into<String>, role: Option<String>, order: Option<i32>) -> Self {
        Self {
            name: name.into(),
            role,
            order,
        }
    }
}

/// One artwork row. `date_added` is preserved across replaces so the original
/// scan time is not lost; `date_updated` is set to `now()` by the helper.
#[derive(Debug, Clone)]
pub struct ArtworkRef {
    pub kind: i32,
    pub path: String,
    pub date_added: DateTime<Utc>,
}

/// DELETE-then-INSERT rows in a "shared string" table (`Genre`, `Tag`,
/// `Studio`, `Style`, `Mood`, `MusicVideoArtist`). Each row has a single
/// `Name` payload column plus the `{Kind}MetadataId` FK.
pub(crate) async fn replace_strings(
    tx: &mut Transaction<'_, Db>,
    table: &'static str,
    fk_column: &'static str,
    metadata_id: i64,
    values: &[String],
) -> Result<()> {
    let delete_sql = format!(r#"DELETE FROM "{table}" WHERE "{fk_column}" = ?"#);
    sqlx::query(&delete_sql)
        .bind(metadata_id)
        .execute(&mut **tx)
        .await?;

    if values.is_empty() {
        return Ok(());
    }

    let insert_sql = format!(r#"INSERT INTO "{table}" ("Name", "{fk_column}") VALUES (?, ?)"#);
    for value in values {
        sqlx::query(&insert_sql)
            .bind(value)
            .bind(metadata_id)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

/// DELETE-then-INSERT rows in a "person" table (`Actor`, `Director`,
/// `Writer`). `Actor` has additional `Role` and `Order` columns; for
/// `Director`/`Writer` the helper writes NULL into both.
pub(crate) async fn replace_people(
    tx: &mut Transaction<'_, Db>,
    table: &'static str,
    fk_column: &'static str,
    metadata_id: i64,
    people: &[PersonRef],
) -> Result<()> {
    let delete_sql = format!(r#"DELETE FROM "{table}" WHERE "{fk_column}" = ?"#);
    sqlx::query(&delete_sql)
        .bind(metadata_id)
        .execute(&mut **tx)
        .await?;

    if people.is_empty() {
        return Ok(());
    }

    let is_actor = table == "Actor";
    let insert_sql = if is_actor {
        format!(
            r#"INSERT INTO "{table}" ("Name", "Role", "Order", "{fk_column}") VALUES (?, ?, ?, ?)"#
        )
    } else {
        format!(r#"INSERT INTO "{table}" ("Name", "{fk_column}") VALUES (?, ?)"#)
    };

    for person in people {
        if is_actor {
            sqlx::query(&insert_sql)
                .bind(&person.name)
                .bind(&person.role)
                .bind(person.order)
                .bind(metadata_id)
                .execute(&mut **tx)
                .await?;
        } else {
            sqlx::query(&insert_sql)
                .bind(&person.name)
                .bind(metadata_id)
                .execute(&mut **tx)
                .await?;
        }
    }
    Ok(())
}

/// DELETE-then-INSERT artwork rows for the given metadata FK. `scope` lists
/// which `ArtworkKind`s the caller manages on this pass; rows of those kinds
/// are deleted unconditionally so the source-of-truth can shrink (a media item
/// that loses its poster between scans must not keep stale rows). Kinds
/// outside `scope` are left untouched.
pub(crate) async fn replace_artwork(
    tx: &mut Transaction<'_, Db>,
    fk_column: &'static str,
    metadata_id: i64,
    scope: &[i32],
    items: &[ArtworkRef],
) -> Result<()> {
    if scope.is_empty() {
        return Ok(());
    }

    let placeholders = scope.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let delete_sql = format!(
        r#"DELETE FROM "Artwork" WHERE "{fk_column}" = ? AND "ArtworkKind" IN ({placeholders})"#
    );
    let mut delete = sqlx::query(&delete_sql).bind(metadata_id);
    for kind in scope {
        delete = delete.bind(*kind);
    }
    delete.execute(&mut **tx).await?;

    if items.is_empty() {
        return Ok(());
    }

    let insert_sql = format!(
        r#"INSERT INTO "Artwork" (
            "ArtworkKind", "{fk_column}", "DateAdded", "DateUpdated", "Path"
        ) VALUES (?, ?, ?, ?, ?)"#
    );
    let now = encode_datetime(Utc::now());
    for item in items {
        sqlx::query(&insert_sql)
            .bind(item.kind)
            .bind(metadata_id)
            .bind(encode_datetime(item.date_added))
            .bind(&now)
            .bind(&item.path)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

/// DELETE-then-INSERT provider GUIDs in `MetadataGuid` for the given
/// metadata FK.
pub(crate) async fn replace_guids(
    tx: &mut Transaction<'_, Db>,
    fk_column: &'static str,
    metadata_id: i64,
    guids: &[String],
) -> Result<()> {
    let delete_sql = format!(r#"DELETE FROM "MetadataGuid" WHERE "{fk_column}" = ?"#);
    sqlx::query(&delete_sql)
        .bind(metadata_id)
        .execute(&mut **tx)
        .await?;

    if guids.is_empty() {
        return Ok(());
    }

    let insert_sql = format!(r#"INSERT INTO "MetadataGuid" ("Guid", "{fk_column}") VALUES (?, ?)"#);
    for guid in guids {
        sqlx::query(&insert_sql)
            .bind(guid)
            .bind(metadata_id)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

fn encode_datetime(value: DateTime<Utc>) -> String {
    value.naive_utc().format("%Y-%m-%d %H:%M:%S").to_string()
}
