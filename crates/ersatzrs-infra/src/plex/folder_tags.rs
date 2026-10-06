//! PlexServerApiClient.GetFolderTags: paths belong to Plex, not this host OS.
use std::collections::BTreeSet;

use ersatzrs_core::repositories::Result;
use sqlx::Transaction;

use crate::{db::Db, repositories::SqlxResultExt as _};

/// Include the first matching library's folder and descendants, never its drive.
pub(super) fn from_path(file: &str, roots: &[String]) -> Vec<String> {
    let normalized = file.replace('\\', "/");
    let Some((folder, _)) = normalized.rsplit_once('/') else {
        return Vec::new();
    };
    let folders: Vec<_> = folder.split('/').collect();
    for root in roots.iter().filter(|root| !root.trim().is_empty()) {
        let normalized = root.replace('\\', "/");
        let root = normalized.trim_end_matches('/');
        if root.is_empty() {
            if folder.is_empty() || folder.starts_with('/') {
                return components(&folders);
            }
            continue;
        }
        let roots: Vec<_> = root.split('/').collect();
        if folders.len() < roots.len() || !folders.iter().zip(&roots).all(|(a, b)| ordinal_eq(a, b))
        {
            continue;
        }
        let start = if roots.len() == 1 && root.len() == 2 && root.ends_with(':') {
            1
        } else {
            roots.len() - 1
        };
        return components(&folders[start..]);
    }
    Vec::new()
}

fn components(parts: &[&str]) -> Vec<String> {
    parts
        .iter()
        .filter(|part| !part.is_empty())
        .map(|part| (*part).to_owned())
        .collect()
}

// Ordinal case comparison keeps accents and does not expand a character into
// multiple letters (e.g. a folder named Straße must not match STRASSE).
fn ordinal_eq(left: &str, right: &str) -> bool {
    fn fold(value: char) -> char {
        let mut upper = value.to_uppercase();
        match (upper.next(), upper.next()) {
            (Some(one), None) => one,
            _ => value,
        }
    }
    left.chars().map(fold).eq(right.chars().map(fold))
}

/// Reconcile only derived folder tags; label/collection/network owners survive.
pub(super) async fn replace(
    tx: &mut Transaction<'_, Db>,
    metadata_id: i64,
    tags: &[String],
) -> Result<()> {
    let mut incoming: BTreeSet<_> = tags.iter().map(String::as_str).collect();
    let existing: Vec<(i64, String)> = sqlx::query_as(
        r#"SELECT "Id", "Name" FROM "Tag"
           WHERE "OtherVideoMetadataId" = ? AND "ExternalTypeId" = 'plex/folder'
           ORDER BY "Id""#,
    )
    .bind(metadata_id)
    .fetch_all(&mut **tx)
    .await
    .repository()?;
    for (id, name) in existing {
        if !incoming.remove(name.as_str()) {
            sqlx::query(r#"DELETE FROM "Tag" WHERE "Id" = ?"#)
                .bind(id)
                .execute(&mut **tx)
                .await
                .repository()?;
        }
    }
    for name in incoming {
        sqlx::query(r#"INSERT INTO "Tag" ("Name", "ExternalTypeId", "OtherVideoMetadataId") VALUES (?, 'plex/folder', ?)"#)
            .bind(name).bind(metadata_id).execute(&mut **tx).await.repository()?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "folder_tags_tests.rs"]
mod tests;
