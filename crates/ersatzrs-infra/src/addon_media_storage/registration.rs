//! Database-only ownership registration for an already validated receiving folder.

use std::path::{Component, Path, PathBuf};

use super::{insert_list_library_projection, Db, EntityId, RepositoryError, Result};
use crate::repositories::SqlxResultExt as _;
use sqlx::{Row as _, Transaction};

/// Register an existing direct-child list folder without writing or renaming files.
///
/// The caller validates and normalizes the existing directory and holds its
/// managed-tree guard before starting this transaction. This function performs
/// database checks only: enabled add-on ownership, exact root containment and
/// unique receiving ownership. It never sanitizes the supplied folder name.
/// Repeating the same registration returns the existing library identity.
///
/// # Errors
/// Refuses disabled or mismatched add-ons, paths outside the registered source,
/// conflicting ownership, inconsistent existing registration, or database failures.
#[allow(clippy::too_many_lines)] // Validate exact ownership and registration together in the caller's transaction.
pub async fn register_existing_list_library_in_transaction(
    transaction: &mut Transaction<'_, Db>,
    list_id: EntityId,
    addon_id: &str,
    validated_folder: &Path,
) -> Result<EntityId> {
    let source = sqlx::query(
        r#"SELECT "source"."MediaSourceId", "source"."RootPath"
           FROM "AddonMediaSource" AS "source"
           JOIN "AddonInstallation" AS "addon" ON "addon"."AddonId"="source"."AddonId"
           JOIN "AddonMediaList" AS "list" ON "list"."AddonId"="source"."AddonId"
           WHERE "list"."Id"=? AND "source"."AddonId"=? AND "addon"."Enabled"=1"#,
    )
    .bind(list_id)
    .bind(addon_id)
    .fetch_optional(&mut **transaction)
    .await
    .repository()?
    .ok_or_else(conflict)?;
    let source_id: EntityId = source.try_get("MediaSourceId").repository()?;
    let root = PathBuf::from(source.try_get::<String, _>("RootPath").repository()?);
    if !root.is_absolute() || !validated_folder.is_absolute() {
        return Err(conflict());
    }
    let relative = validated_folder
        .strip_prefix(&root)
        .map_err(|_| conflict())?;
    let mut components = relative.components();
    let Some(Component::Normal(name)) = components.next() else {
        return Err(conflict());
    };
    let folder_name = name.to_str().ok_or_else(conflict)?;
    if components.next().is_some() || root.join(name) != validated_folder {
        return Err(conflict());
    }
    let folder_path = validated_folder.to_str().ok_or_else(conflict)?;
    let manifest = validated_folder.join("list.playlist.yml");
    let manifest_path = manifest.to_str().ok_or_else(conflict)?;
    let owner = sqlx::query(
        r#"SELECT "LibraryId", "FolderName", "ManifestPath" FROM "AddonMediaListLibrary"
           WHERE "AddonMediaListId"=?"#,
    )
    .bind(list_id)
    .fetch_optional(&mut **transaction)
    .await
    .repository()?;
    let existing_id = owner
        .as_ref()
        .map(|row| row.try_get::<EntityId, _>("LibraryId"))
        .transpose()
        .repository()?;
    let conflicting: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM "LibraryPath"
             WHERE LOWER("Path")=LOWER(?) AND (? IS NULL OR "LibraryId"<>?))
           OR EXISTS(SELECT 1 FROM "AddonMediaListLibrary" AS "managed"
             JOIN "AddonMediaList" AS "list" ON "list"."Id"="managed"."AddonMediaListId"
             WHERE "managed"."AddonMediaListId"<>? AND
               (LOWER("managed"."ManifestPath")=LOWER(?) OR
                ("list"."AddonId"=? AND LOWER("managed"."FolderName")=LOWER(?))))"#,
    )
    .bind(folder_path)
    .bind(existing_id)
    .bind(existing_id)
    .bind(list_id)
    .bind(manifest_path)
    .bind(addon_id)
    .bind(folder_name)
    .fetch_one(&mut **transaction)
    .await
    .repository()?;
    if conflicting {
        return Err(conflict());
    }
    if let Some(owner) = owner {
        let library_id = existing_id.ok_or_else(conflict)?;
        let consistent: bool = sqlx::query_scalar(
            r#"SELECT EXISTS(SELECT 1 FROM "Library" AS "library"
                 JOIN "LocalLibrary" AS "local" ON "local"."Id"="library"."Id"
                 WHERE "library"."Id"=? AND "library"."MediaSourceId"=?)
               AND (SELECT COUNT(*) FROM "LibraryPath" WHERE "LibraryId"=?)=1
               AND EXISTS(SELECT 1 FROM "LibraryPath" WHERE "LibraryId"=? AND "Path"=?)"#,
        )
        .bind(library_id)
        .bind(source_id)
        .bind(library_id)
        .bind(library_id)
        .bind(folder_path)
        .fetch_one(&mut **transaction)
        .await
        .repository()?;
        if !consistent
            || owner.try_get::<String, _>("FolderName").repository()? != folder_name
            || owner.try_get::<String, _>("ManifestPath").repository()? != manifest_path
        {
            return Err(conflict());
        }
        return Ok(library_id);
    }
    insert_list_library_projection(
        transaction,
        list_id,
        source_id,
        folder_name,
        validated_folder,
        &manifest,
    )
    .await
}

fn conflict() -> RepositoryError {
    RepositoryError::Conflict(
        "the receiving list folder has conflicting or invalid local ownership".into(),
    )
}

#[cfg(test)]
mod tests;
