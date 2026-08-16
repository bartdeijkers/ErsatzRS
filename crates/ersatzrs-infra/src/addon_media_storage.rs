//! File-backed local media sources owned by schema-v3 add-on list storage.

use std::path::{Path, PathBuf};

use ersatzrs_addon_contract::{AddonManifest, AddonMediaListStorage};
use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::EntityId;
use serde::Serialize;
use sqlx::{Row as _, Transaction};

use crate::addons::{AddonError, AddonSettingValue, InstalledAddon};
use crate::db::{inserted_id, Db, DbPool};
use crate::file_system_layout::app_state_folder;
use crate::repositories::SqlxResultExt as _;

const REMOTE_STREAM_MEDIA_KIND: i32 = 7;

/// Resolve the configured storage root or the profile-relative default.
///
/// # Errors
///
/// Returns an error when a configured override is not an absolute path.
pub fn effective_storage_path(
    manifest: &AddonManifest,
    settings: &[AddonSettingValue],
) -> std::result::Result<Option<PathBuf>, AddonError> {
    let Some(storage) = &manifest.media_list_storage else {
        return Ok(None);
    };
    let configured = settings
        .iter()
        .find(|setting| setting.key == storage.path_setting)
        .and_then(|setting| setting.value.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let path = configured.map_or_else(
        || app_state_folder().join(&storage.default_subdirectory),
        PathBuf::from,
    );
    if !path.is_absolute() {
        return Err(AddonError::Validation(format!(
            "{} must be an absolute path",
            storage.path_setting
        )));
    }
    Ok(Some(path))
}

/// Fill the host-rendered path setting with its resolved profile default.
///
/// # Errors
///
/// Returns an error when the effective storage path is invalid.
pub fn apply_effective_storage_default(
    manifest: &mut AddonManifest,
    settings: &[AddonSettingValue],
) -> std::result::Result<(), AddonError> {
    let Some(storage) = manifest.media_list_storage.clone() else {
        return Ok(());
    };
    let Some(path) = effective_storage_path(manifest, settings)? else {
        return Ok(());
    };
    if let Some(definition) = manifest
        .settings
        .iter_mut()
        .find(|setting| setting.key == storage.path_setting)
    {
        definition.default = Some(path.to_string_lossy().into_owned());
    }
    Ok(())
}

/// Prepare the storage folder before committing the enabling transaction.
///
/// # Errors
///
/// Returns an error when the path is invalid or cannot be created and resolved.
pub async fn prepare_storage_root(
    manifest: &AddonManifest,
    settings: &[AddonSettingValue],
) -> std::result::Result<Option<PathBuf>, AddonError> {
    let Some(path) = effective_storage_path(manifest, settings)? else {
        return Ok(None);
    };
    tokio::fs::create_dir_all(&path)
        .await
        .map_err(|error| AddonError::Package(format!("create media storage: {error}")))?;
    let canonical = tokio::fs::canonicalize(&path)
        .await
        .map_err(|error| AddonError::Package(format!("resolve media storage: {error}")))?;
    Ok(Some(canonical))
}

/// Idempotently attach the declared storage root to one local media source.
///
/// # Errors
///
/// Returns an error for storage ownership conflicts or database failures.
pub async fn ensure_media_source_in_transaction(
    transaction: &mut Transaction<'_, Db>,
    addon_id: &str,
    storage: &AddonMediaListStorage,
    root: &Path,
) -> std::result::Result<EntityId, AddonError> {
    let conflicting_addon: Option<String> = sqlx::query_scalar(
        r#"SELECT "AddonId" FROM "AddonMediaSource"
           WHERE "AddonId" <> ? AND LOWER("RootPath") = LOWER(?) LIMIT 1"#,
    )
    .bind(addon_id)
    .bind(root.to_string_lossy().as_ref())
    .fetch_optional(&mut **transaction)
    .await
    .map_err(database_error)?;
    if let Some(conflicting_addon) = conflicting_addon {
        return Err(AddonError::Validation(format!(
            "media storage path is already owned by {conflicting_addon}"
        )));
    }
    let existing =
        sqlx::query(r#"SELECT "MediaSourceId" FROM "AddonMediaSource" WHERE "AddonId" = ?"#)
            .bind(addon_id)
            .fetch_optional(&mut **transaction)
            .await
            .map_err(database_error)?;
    let source_id = if let Some(row) = existing {
        row.try_get("MediaSourceId").map_err(database_error)?
    } else {
        let result = sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (NULL)"#)
            .execute(&mut **transaction)
            .await
            .map_err(database_error)?;
        let source_id = inserted_id(transaction, result)
            .await
            .map_err(database_error)?;
        sqlx::query(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (?)"#)
            .bind(source_id)
            .execute(&mut **transaction)
            .await
            .map_err(database_error)?;
        sqlx::query(
            r#"INSERT INTO "AddonMediaSource" ("AddonId", "MediaSourceId", "Name", "RootPath")
               VALUES (?, ?, ?, ?)"#,
        )
        .bind(addon_id)
        .bind(source_id)
        .bind(&storage.source_name)
        .bind(root.to_string_lossy().as_ref())
        .execute(&mut **transaction)
        .await
        .map_err(database_error)?;
        source_id
    };
    sqlx::query(r#"UPDATE "AddonMediaSource" SET "Name" = ?, "RootPath" = ? WHERE "AddonId" = ?"#)
        .bind(&storage.source_name)
        .bind(root.to_string_lossy().as_ref())
        .bind(addon_id)
        .execute(&mut **transaction)
        .await
        .map_err(database_error)?;
    Ok(source_id)
}

/// Create or update the Remote Streams library and playlist manifest for one
/// imported add-on list.
///
/// # Errors
///
/// Returns an error when the source is absent or persistence fails.
pub async fn ensure_list_library(
    pool: &DbPool,
    list_id: EntityId,
    addon_id: &str,
    list_name: &str,
    source_url: &str,
    chapter_input: &str,
    requested_folder: Option<&str>,
) -> Result<EntityId> {
    let source = sqlx::query(
        r#"SELECT "MediaSourceId", "RootPath" FROM "AddonMediaSource" WHERE "AddonId" = ?"#,
    )
    .bind(addon_id)
    .fetch_optional(pool)
    .await
    .repository()?
    .ok_or_else(|| {
        RepositoryError::unavailable(
            "create add-on list library",
            "the enabled add-on has no managed local media source",
        )
    })?;
    let source_id: EntityId = source.try_get("MediaSourceId").repository()?;
    let root = PathBuf::from(source.try_get::<String, _>("RootPath").repository()?);
    let existing = sqlx::query(
        r#"SELECT "LibraryId", "FolderName" FROM "AddonMediaListLibrary"
           WHERE "AddonMediaListId" = ?"#,
    )
    .bind(list_id)
    .fetch_optional(pool)
    .await
    .repository()?;
    let (library_id, folder_name) = if let Some(row) = existing {
        (
            row.try_get("LibraryId").repository()?,
            row.try_get::<String, _>("FolderName").repository()?,
        )
    } else {
        // An operator-supplied folder name is already sanitized and checked for
        // conflicts by `validated_folder_name`; only the derived name needs the
        // per-add-on uniqueness suffix.
        let folder_name = match requested_folder {
            Some(folder_name) => folder_name.to_owned(),
            None => available_folder_name(pool, addon_id, list_id, list_name).await?,
        };
        (0, folder_name)
    };
    let folder = root.join(&folder_name);
    tokio::fs::create_dir_all(&folder).await.map_err(|error| {
        RepositoryError::unavailable("create add-on list library folder", error.to_string())
    })?;
    let manifest_path = folder.join("list.playlist.yml");
    write_playlist_manifest(
        &manifest_path,
        addon_id,
        list_name,
        source_url,
        chapter_input,
    )
    .await?;

    let mut transaction = pool.begin().await.repository()?;
    let library_id = if library_id == 0 {
        let inserted = sqlx::query(
            r#"INSERT INTO "Library" ("MediaSourceId", "MediaKind", "Name") VALUES (?, ?, ?)"#,
        )
        .bind(source_id)
        .bind(REMOTE_STREAM_MEDIA_KIND)
        .bind(&folder_name)
        .execute(&mut *transaction)
        .await
        .repository()?;
        let library_id = inserted_id(&mut transaction, inserted).await.repository()?;
        sqlx::query(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (?)"#)
            .bind(library_id)
            .execute(&mut *transaction)
            .await
            .repository()?;
        sqlx::query(r#"INSERT INTO "LibraryPath" ("LibraryId", "Path") VALUES (?, ?)"#)
            .bind(library_id)
            .bind(folder.to_string_lossy().as_ref())
            .execute(&mut *transaction)
            .await
            .repository()?;
        sqlx::query(
            r#"INSERT INTO "AddonMediaListLibrary"
               ("AddonMediaListId", "LibraryId", "FolderName", "ManifestPath")
               VALUES (?, ?, ?, ?)"#,
        )
        .bind(list_id)
        .bind(library_id)
        .bind(&folder_name)
        .bind(manifest_path.to_string_lossy().as_ref())
        .execute(&mut *transaction)
        .await
        .repository()?;
        library_id
    } else {
        update_list_library_projection(
            &mut transaction,
            library_id,
            list_id,
            &folder_name,
            &folder,
            &manifest_path,
        )
        .await?;
        library_id
    };
    transaction.commit().await.repository()?;
    Ok(library_id)
}

async fn update_list_library_projection(
    transaction: &mut Transaction<'_, Db>,
    library_id: EntityId,
    list_id: EntityId,
    folder_name: &str,
    folder: &Path,
    manifest_path: &Path,
) -> Result<()> {
    sqlx::query(r#"UPDATE "Library" SET "Name" = ? WHERE "Id" = ?"#)
        .bind(folder_name)
        .bind(library_id)
        .execute(&mut **transaction)
        .await
        .repository()?;
    sqlx::query(r#"UPDATE "LibraryPath" SET "Path" = ? WHERE "LibraryId" = ?"#)
        .bind(folder.to_string_lossy().as_ref())
        .bind(library_id)
        .execute(&mut **transaction)
        .await
        .repository()?;
    sqlx::query(
        r#"UPDATE "AddonMediaListLibrary" SET "ManifestPath" = ? WHERE "AddonMediaListId" = ?"#,
    )
    .bind(manifest_path.to_string_lossy().as_ref())
    .bind(list_id)
    .execute(&mut **transaction)
    .await
    .repository()?;
    Ok(())
}

/// Return the local library materialized for a provider list.
///
/// # Errors
///
/// Returns an error when the mapping cannot be queried.
pub async fn list_library_id(pool: &DbPool, list_id: EntityId) -> Result<Option<EntityId>> {
    sqlx::query_scalar(
        r#"SELECT "LibraryId" FROM "AddonMediaListLibrary" WHERE "AddonMediaListId" = ?"#,
    )
    .bind(list_id)
    .fetch_optional(pool)
    .await
    .repository()
}

/// Whether an enabled add-on declared and owns a managed local source.
///
/// # Errors
///
/// Returns an error when the source mapping cannot be queried.
pub async fn has_media_source(pool: &DbPool, addon_id: &str) -> Result<bool> {
    let count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaSource" WHERE "AddonId" = ?"#)
            .bind(addon_id)
            .fetch_one(pool)
            .await
            .repository()?;
    Ok(count > 0)
}

/// Remove the managed library registration while retaining the operator's
/// files for recovery.
///
/// # Errors
///
/// Returns an error when the library mapping or library cannot be deleted.
pub async fn delete_list_library(pool: &DbPool, list_id: EntityId) -> Result<()> {
    let library_id = list_library_id(pool, list_id).await?;
    if let Some(library_id) = library_id {
        sqlx::query(r#"DELETE FROM "Library" WHERE "Id" = ?"#)
            .bind(library_id)
            .execute(pool)
            .await
            .repository()?;
    }
    Ok(())
}

/// Sanitize an operator-supplied folder name and reject one another list of the
/// same add-on already owns.
///
/// The check runs before the list is persisted, so a rejected name leaves no
/// half-imported list behind. A list that already owns the name keeps it, which
/// makes re-adding the same source URL idempotent.
///
/// # Errors
///
/// Returns an error when the name is empty after sanitization, when it
/// collides with another list's folder, or when the conflict cannot be queried.
pub async fn validated_folder_name(
    pool: &DbPool,
    addon_id: &str,
    source_url: &str,
    requested: &str,
) -> Result<String> {
    if !requested.chars().any(char::is_alphanumeric) {
        return Err(RepositoryError::Conflict(format!(
            "{requested} is not usable as a folder name"
        )));
    }
    let folder_name = safe_folder_name(requested);
    let conflict: Option<String> = sqlx::query_scalar(
        r#"SELECT "managed"."FolderName"
           FROM "AddonMediaListLibrary" AS "managed"
           JOIN "AddonMediaList" AS "list" ON "list"."Id" = "managed"."AddonMediaListId"
           WHERE "list"."AddonId" = ?
             AND LOWER("managed"."FolderName") = LOWER(?)
             AND "list"."SourceUrl" <> ?
           LIMIT 1"#,
    )
    .bind(addon_id)
    .bind(&folder_name)
    .bind(source_url)
    .fetch_optional(pool)
    .await
    .repository()?;
    if conflict.is_some() {
        return Err(RepositoryError::Conflict(format!(
            "another list of this add-on already uses the folder {folder_name}"
        )));
    }
    Ok(folder_name)
}

async fn available_folder_name(
    pool: &DbPool,
    addon_id: &str,
    list_id: EntityId,
    list_name: &str,
) -> Result<String> {
    let base = safe_folder_name(list_name);
    let conflict: Option<EntityId> = sqlx::query_scalar(
        r#"SELECT "managed"."AddonMediaListId"
           FROM "AddonMediaListLibrary" AS "managed"
           JOIN "AddonMediaList" AS "list" ON "list"."Id" = "managed"."AddonMediaListId"
           WHERE "list"."AddonId" = ? AND LOWER("managed"."FolderName") = LOWER(?)
           LIMIT 1"#,
    )
    .bind(addon_id)
    .bind(&base)
    .fetch_optional(pool)
    .await
    .repository()?;
    Ok(if conflict.is_some() {
        format!("{base}-{list_id}")
    } else {
        base
    })
}

fn safe_folder_name(name: &str) -> String {
    let mut output = String::with_capacity(name.len().min(80));
    let mut separator = false;
    for character in name.trim().chars() {
        if output.chars().count() == 80 {
            break;
        }
        if character.is_alphanumeric() || matches!(character, ' ' | '-' | '_') {
            output.push(character);
            separator = false;
        } else if !separator {
            output.push('-');
            separator = true;
        }
    }
    let output = output.trim_matches([' ', '.', '-']).to_owned();
    let output = if output.is_empty() {
        "list".to_owned()
    } else {
        output
    };
    if is_windows_reserved_name(&output) {
        format!("_{output}")
    } else {
        output
    }
}

fn is_windows_reserved_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || upper
            .strip_prefix("COM")
            .or_else(|| upper.strip_prefix("LPT"))
            .is_some_and(|suffix| {
                matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
            })
}

#[derive(Serialize)]
struct ManagedPlaylistManifest<'a> {
    name: &'a str,
    url: &'a str,
    addon: &'a str,
    folder: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    chapters: Option<&'a str>,
    sync: ManagedPlaylistSync,
}

#[derive(Serialize)]
struct ManagedPlaylistSync {
    interval: &'static str,
    max_items: usize,
    order: &'static str,
    on_removed: &'static str,
}

async fn write_playlist_manifest(
    path: &Path,
    addon_id: &str,
    name: &str,
    source_url: &str,
    chapter_input: &str,
) -> Result<()> {
    let yaml = serde_yaml::to_string(&ManagedPlaylistManifest {
        name,
        url: source_url,
        addon: addon_id,
        folder: "streams",
        chapters: (!chapter_input.is_empty()).then_some(chapter_input),
        sync: ManagedPlaylistSync {
            interval: "24h",
            max_items: 0,
            order: "upstream",
            on_removed: "trash",
        },
    })
    .map_err(|error| RepositoryError::Decode {
        kind: "managed add-on playlist manifest",
        value: error.to_string(),
    })?;
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<()> {
        use std::io::Write as _;

        if std::fs::read(&path).is_ok_and(|current| current == yaml.as_bytes()) {
            return Ok(());
        }
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|error| {
            RepositoryError::unavailable("create add-on playlist manifest", error.to_string())
        })?;
        temporary.write_all(yaml.as_bytes()).map_err(|error| {
            RepositoryError::unavailable("write add-on playlist manifest", error.to_string())
        })?;
        temporary.flush().map_err(|error| {
            RepositoryError::unavailable("flush add-on playlist manifest", error.to_string())
        })?;
        temporary.persist(&path).map_err(|error| {
            RepositoryError::unavailable(
                "publish add-on playlist manifest",
                error.error.to_string(),
            )
        })?;
        Ok(())
    })
    .await
    .map_err(|error| {
        RepositoryError::unavailable("write add-on playlist manifest", error.to_string())
    })?
}

#[allow(clippy::needless_pass_by_value)] // `map_err` supplies the owned error.
fn database_error(error: sqlx::Error) -> AddonError {
    AddonError::Database(error.to_string())
}

/// Reconcile storage sources for add-ons that were already enabled before an
/// upgrade introduced this contract.
///
/// # Errors
///
/// Returns an error when an enabled source or its libraries cannot be restored.
pub async fn reconcile_enabled_sources(pool: &DbPool) -> std::result::Result<(), AddonError> {
    let installed = crate::addons::list_installed_addons(pool).await?;
    for addon in installed
        .into_iter()
        .filter(|addon| addon.enabled && addon.manifest.media_list_storage.is_some())
    {
        reconcile_enabled_source(pool, &addon).await?;
        reconcile_list_libraries(pool, &addon.addon_id)
            .await
            .map_err(|error| AddonError::Database(error.to_string()))?;
    }
    Ok(())
}

/// Rebuild file/library projections for lists that predate the storage source
/// or whose configured root changed while the add-on was disabled.
///
/// # Errors
///
/// Returns an error when a list projection cannot be queried or materialized.
pub async fn reconcile_list_libraries(pool: &DbPool, addon_id: &str) -> Result<()> {
    let rows = sqlx::query(
        r#"SELECT "Id", "Name", "SourceUrl", "ChapterInput" FROM "AddonMediaList"
           WHERE "AddonId" = ? ORDER BY "Id""#,
    )
    .bind(addon_id)
    .fetch_all(pool)
    .await
    .repository()?;
    for row in rows {
        ensure_list_library(
            pool,
            row.try_get("Id").repository()?,
            addon_id,
            &row.try_get::<String, _>("Name").repository()?,
            &row.try_get::<String, _>("SourceUrl").repository()?,
            row.try_get::<Option<String>, _>("ChapterInput")
                .repository()?
                .as_deref()
                .unwrap_or_default(),
            None,
        )
        .await?;
    }
    Ok(())
}

async fn reconcile_enabled_source(
    pool: &DbPool,
    addon: &InstalledAddon,
) -> std::result::Result<(), AddonError> {
    let Some(storage) = &addon.manifest.media_list_storage else {
        return Ok(());
    };
    let settings = crate::addons::load_addon_settings(pool, &addon.addon_id).await?;
    let Some(root) = prepare_storage_root(&addon.manifest, &settings).await? else {
        return Ok(());
    };
    let mut transaction = pool.begin().await.map_err(database_error)?;
    ensure_media_source_in_transaction(&mut transaction, &addon.addon_id, storage, &root).await?;
    transaction.commit().await.map_err(database_error)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use ersatzrs_addon_contract::{
        AddonCapability, AddonEntrypoint, AddonEntrypointKind, AddonManifest,
        AddonMediaListStorage, AddonPermissions, AddonSettingDefinition, AddonSettingKind,
        CAPABILITY_ADDON_CHECK_V1, CAPABILITY_MEDIA_LIST_LIST_V1, CAPABILITY_REMOTE_STREAM_LIST_V1,
        CAPABILITY_REMOTE_STREAM_PLAY_V1,
    };
    use semver::{Version, VersionReq};

    use super::{
        ensure_list_library, ensure_media_source_in_transaction, is_windows_reserved_name,
        safe_folder_name, validated_folder_name, EntityId, RepositoryError,
    };

    #[test]
    fn folder_names_are_portable_and_displayable() {
        assert_eq!(
            safe_folder_name("Star Trek: TNG / Best"),
            "Star Trek- TNG - Best"
        );
        assert_eq!(safe_folder_name("CON"), "_CON");
        assert!(is_windows_reserved_name("lpt9"));
    }

    fn storage_manifest(addon_id: &str) -> (AddonManifest, AddonMediaListStorage) {
        let storage = AddonMediaListStorage {
            path_setting: "MEDIA_STORAGE_PATH".to_owned(),
            default_subdirectory: "fixture_media".to_owned(),
            source_name: "fixture".to_owned(),
        };
        let manifest = AddonManifest {
            manifest_version: 3,
            id: addon_id.to_owned(),
            version: Version::new(1, 0, 0),
            name: BTreeMap::from([("en-US".to_owned(), "Fixture".to_owned())]),
            summary: BTreeMap::from([("en-US".to_owned(), "Fixture".to_owned())]),
            icon: None,
            license: "Zlib".to_owned(),
            source_url: "https://example.test/source".to_owned(),
            host_version: VersionReq::STAR,
            capabilities: [
                CAPABILITY_ADDON_CHECK_V1,
                CAPABILITY_MEDIA_LIST_LIST_V1,
                CAPABILITY_REMOTE_STREAM_LIST_V1,
                CAPABILITY_REMOTE_STREAM_PLAY_V1,
            ]
            .into_iter()
            .map(|id| AddonCapability { id: id.to_owned() })
            .collect(),
            dependencies: Vec::new(),
            entrypoints: BTreeMap::from([(
                "any".to_owned(),
                AddonEntrypoint {
                    path: "addon.sh".to_owned(),
                    kind: AddonEntrypointKind::PosixShell,
                },
            )]),
            permissions: AddonPermissions::default(),
            settings: vec![AddonSettingDefinition {
                key: "MEDIA_STORAGE_PATH".to_owned(),
                label: BTreeMap::from([("en-US".to_owned(), "Media storage location".to_owned())]),
                kind: AddonSettingKind::Path,
                required: false,
                default: None,
                options: Vec::new(),
                suggested_reference: None,
            }],
            media_list_storage: Some(storage.clone()),
            panels: Vec::new(),
        };
        (manifest, storage)
    }

    async fn assert_storage_move(
        pool: &crate::db::DbPool,
        addon_id: &str,
        storage: &AddonMediaListStorage,
        list_id: i64,
        moved_root: &std::path::Path,
    ) {
        tokio::fs::create_dir_all(moved_root)
            .await
            .expect("create moved root");
        let mut transaction = pool.begin().await.expect("begin move transaction");
        ensure_media_source_in_transaction(&mut transaction, addon_id, storage, moved_root)
            .await
            .expect("move source");
        transaction.commit().await.expect("commit moved source");
        ensure_list_library(
            pool,
            list_id,
            addon_id,
            "Star Trek: TNG",
            "https://example.test/list",
            "",
            None,
        )
        .await
        .expect("move library projection");
        let manifest_path: String = sqlx::query_scalar(
            r#"SELECT "ManifestPath" FROM "AddonMediaListLibrary"
               WHERE "AddonMediaListId" = ?"#,
        )
        .bind(list_id)
        .fetch_one(pool)
        .await
        .expect("load moved manifest path");
        assert_eq!(
            manifest_path,
            moved_root
                .join("Star Trek- TNG")
                .join("list.playlist.yml")
                .to_string_lossy()
        );
    }

    async fn fixture_pool_with_source(root: &std::path::Path, addon_id: &str) -> crate::db::DbPool {
        let database_url = format!("sqlite:{}", root.join("storage.db").display());
        let pool = crate::db::init(&database_url)
            .await
            .expect("initialize database");
        let (manifest, storage) = storage_manifest(addon_id);
        sqlx::query(
            r#"INSERT INTO "AddonInstallation" (
                   "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
                   "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt"
               ) VALUES (?, '1.0.0', 1, 'package', 'content', ?, 'fixture', 'now', 'now')"#,
        )
        .bind(addon_id)
        .bind(serde_json::to_string(&manifest).expect("manifest JSON"))
        .execute(&pool)
        .await
        .expect("insert installation");
        let media_root = root.join("fixture_media");
        tokio::fs::create_dir_all(&media_root)
            .await
            .expect("create root");
        let mut transaction = pool.begin().await.expect("begin transaction");
        ensure_media_source_in_transaction(&mut transaction, addon_id, &storage, &media_root)
            .await
            .expect("create source");
        transaction.commit().await.expect("commit source");
        pool
    }

    async fn insert_fixture_list(
        pool: &crate::db::DbPool,
        addon_id: &str,
        source_url: &str,
        name: &str,
    ) -> EntityId {
        let mut transaction = pool.begin().await.expect("begin list transaction");
        let inserted = sqlx::query(
            r#"INSERT INTO "AddonMediaList" ("AddonId", "ProviderId", "SourceUrl", "Name")
               VALUES (?, ?, ?, ?)"#,
        )
        .bind(addon_id)
        .bind(source_url)
        .bind(source_url)
        .bind(name)
        .execute(&mut *transaction)
        .await
        .expect("insert list");
        let id = crate::db::inserted_id(&mut transaction, inserted)
            .await
            .expect("read list id");
        transaction.commit().await.expect("commit id transaction");
        id
    }

    #[tokio::test]
    async fn operator_folder_name_is_sanitized_and_kept_unique() {
        let temporary = tempfile::tempdir().expect("temporary folder");
        let addon_id = "org.ersatzrs.addon.fixture";
        let pool = fixture_pool_with_source(temporary.path(), addon_id).await;
        let first_url = "https://example.test/list/one";
        let second_url = "https://example.test/list/two";
        let first = insert_fixture_list(&pool, addon_id, first_url, "Example provider").await;
        insert_fixture_list(&pool, addon_id, second_url, "Another list").await;

        let folder_name = validated_folder_name(&pool, addon_id, first_url, "Archive / Part 1")
            .await
            .expect("sanitize requested folder");
        assert_eq!(folder_name, "Archive - Part 1");
        ensure_list_library(
            &pool,
            first,
            addon_id,
            "Example provider",
            first_url,
            "",
            Some(&folder_name),
        )
        .await
        .expect("create library");
        assert!(temporary
            .path()
            .join("fixture_media")
            .join("Archive - Part 1")
            .join("list.playlist.yml")
            .is_file());

        // Re-adding the same source keeps its own folder available, while
        // another list of the same add-on may not claim it.
        validated_folder_name(&pool, addon_id, first_url, "Archive / Part 1")
            .await
            .expect("same list keeps its folder");
        let conflict = validated_folder_name(&pool, addon_id, second_url, "archive - part 1").await;
        assert!(matches!(conflict, Err(RepositoryError::Conflict(_))));
        let unusable = validated_folder_name(&pool, addon_id, second_url, "///").await;
        assert!(matches!(unusable, Err(RepositoryError::Conflict(_))));
    }

    #[tokio::test]
    async fn managed_source_and_list_library_are_idempotent() {
        let temporary = tempfile::tempdir().expect("temporary folder");
        let database_url = format!("sqlite:{}", temporary.path().join("storage.db").display());
        let pool = crate::db::init(&database_url)
            .await
            .expect("initialize database");
        let addon_id = "org.ersatzrs.addon.fixture";
        let (manifest, storage) = storage_manifest(addon_id);
        sqlx::query(
            r#"INSERT INTO "AddonInstallation" (
                   "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
                   "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt"
               ) VALUES (?, '1.0.0', 1, 'package', 'content', ?, 'fixture', 'now', 'now')"#,
        )
        .bind(addon_id)
        .bind(serde_json::to_string(&manifest).expect("manifest JSON"))
        .execute(&pool)
        .await
        .expect("insert installation");
        let root = temporary.path().join("fixture_media");
        tokio::fs::create_dir_all(&root).await.expect("create root");
        let mut transaction = pool.begin().await.expect("begin transaction");
        let first_source =
            ensure_media_source_in_transaction(&mut transaction, addon_id, &storage, &root)
                .await
                .expect("create source");
        let second_source =
            ensure_media_source_in_transaction(&mut transaction, addon_id, &storage, &root)
                .await
                .expect("reuse source");
        transaction.commit().await.expect("commit source");
        assert_eq!(first_source, second_source);

        let mut transaction = pool.begin().await.expect("begin list transaction");
        let inserted = sqlx::query(
            r#"INSERT INTO "AddonMediaList"
                   ("AddonId", "ProviderId", "SourceUrl", "Name")
                   VALUES (?, 'fixture-list', 'https://example.test/list', 'Star Trek: TNG')"#,
        )
        .bind(addon_id)
        .execute(&mut *transaction)
        .await
        .expect("insert list");
        let list_id = crate::db::inserted_id(&mut transaction, inserted)
            .await
            .expect("read list id");
        transaction.commit().await.expect("commit list");
        let first_library = ensure_list_library(
            &pool,
            list_id,
            addon_id,
            "Star Trek: TNG",
            "https://example.test/list",
            "",
            None,
        )
        .await
        .expect("create library");
        let second_library = ensure_list_library(
            &pool,
            list_id,
            addon_id,
            "Star Trek: TNG",
            "https://example.test/list",
            "",
            None,
        )
        .await
        .expect("reuse library");
        assert_eq!(first_library, second_library);
        assert!(root
            .join("Star Trek- TNG")
            .join("list.playlist.yml")
            .is_file());
        let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Library""#)
            .fetch_one(&pool)
            .await
            .expect("count libraries");
        assert_eq!(count, 1);

        assert_storage_move(
            &pool,
            addon_id,
            &storage,
            list_id,
            &temporary.path().join("moved_media"),
        )
        .await;
    }
}
