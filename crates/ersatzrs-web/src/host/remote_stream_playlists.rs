use async_trait::async_trait;
use ersatzrs_addon_contract::{CAPABILITY_REMOTE_STREAM_LIST_V1, CAPABILITY_REMOTE_STREAM_PLAY_V1};
use ersatzrs_app::{
    RemoteStreamPlaylistEdit, RemoteStreamPlaylistEntryRead, RemoteStreamPlaylistRead,
    RemoteStreamPlaylistRuntime, RemoteStreamPlaylistRuntimeError,
    RemoteStreamPlaylistTroubleshootingRead,
};
use ersatzrs_infra::addons::{list_installed_addons, AddonRuntimeContext};
use ersatzrs_infra::db::DbPool;

const MANAGED_HEADER: &str = "# Managed by ErsatzRS Remote Stream playlist editor.\n";

#[derive(Clone)]
pub struct NativeRemoteStreamPlaylistRuntime {
    pool: DbPool,
}

impl NativeRemoteStreamPlaylistRuntime {
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RemoteStreamPlaylistRuntime for NativeRemoteStreamPlaylistRuntime {
    async fn list(
        &self,
        library_id: i64,
    ) -> Result<Vec<RemoteStreamPlaylistRead>, RemoteStreamPlaylistRuntimeError> {
        list_with_pool(&self.pool, library_id)
            .await
            .map_err(runtime_error)
    }

    async fn addon_ids(&self) -> Result<Vec<String>, RemoteStreamPlaylistRuntimeError> {
        let mut names = list_installed_addons(&self.pool)
            .await
            .map_err(|error| runtime_error(error.to_string()))?
            .into_iter()
            .filter(|addon| {
                addon.enabled
                    && addon
                        .manifest
                        .has_capability(CAPABILITY_REMOTE_STREAM_LIST_V1)
                    && addon
                        .manifest
                        .has_capability(CAPABILITY_REMOTE_STREAM_PLAY_V1)
            })
            .map(|addon| addon.addon_id)
            .collect::<Vec<_>>();
        names.sort_by_key(|name| name.to_lowercase());
        Ok(names)
    }

    async fn tags(&self) -> Result<Vec<String>, RemoteStreamPlaylistRuntimeError> {
        sqlx::query_scalar(
            r#"SELECT DISTINCT "Name"
               FROM "Tag"
               WHERE "RemoteStreamMetadataId" IS NOT NULL
                 AND "Name" IS NOT NULL
                 AND "Name" <> ''
               ORDER BY "Name""#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| runtime_error(error.to_string()))
    }

    async fn save(
        &self,
        editor: RemoteStreamPlaylistEdit,
    ) -> Result<String, RemoteStreamPlaylistRuntimeError> {
        save_with_pool(&self.pool, editor)
            .await
            .map_err(runtime_error)
    }

    async fn delete(
        &self,
        library_id: i64,
        root_index: usize,
        relative_path: String,
    ) -> Result<(), RemoteStreamPlaylistRuntimeError> {
        delete_with_pool(&self.pool, library_id, root_index, &relative_path)
            .await
            .map_err(runtime_error)
    }

    async fn exclude_entry(
        &self,
        library_id: i64,
        root_index: usize,
        relative_path: String,
        entry_id: String,
    ) -> Result<(), RemoteStreamPlaylistRuntimeError> {
        exclude_entry_with_pool(
            &self.pool,
            library_id,
            root_index,
            &relative_path,
            &entry_id,
        )
        .await
        .map_err(runtime_error)
    }

    async fn troubleshoot(
        &self,
        library_id: i64,
        root_index: usize,
        relative_path: String,
    ) -> Result<RemoteStreamPlaylistTroubleshootingRead, RemoteStreamPlaylistRuntimeError> {
        troubleshoot_with_pool(&self.pool, library_id, root_index, &relative_path)
            .await
            .map_err(runtime_error)
    }
}

fn runtime_error(message: impl Into<String>) -> RemoteStreamPlaylistRuntimeError {
    RemoteStreamPlaylistRuntimeError::new(message)
}

async fn remote_stream_roots(
    pool: &DbPool,
    library_id: i64,
) -> Result<Vec<std::path::PathBuf>, String> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT "lp"."Path"
           FROM "LibraryPath" AS "lp"
           JOIN "Library" AS "l" ON "l"."Id" = "lp"."LibraryId"
           JOIN "LocalLibrary" AS "ll" ON "ll"."Id" = "l"."Id"
           WHERE "l"."Id" = ? AND "l"."MediaKind" = 7
           ORDER BY "lp"."Id""#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    if rows.is_empty() {
        return Err("remote stream library was not found".to_owned());
    }
    Ok(rows
        .into_iter()
        .map(|row| std::path::PathBuf::from(row.get::<String, _>("Path")))
        .collect())
}

#[allow(clippy::too_many_lines)]
async fn list_with_pool(
    pool: &DbPool,
    library_id: i64,
) -> Result<Vec<RemoteStreamPlaylistRead>, String> {
    use std::collections::VecDeque;

    let roots = remote_stream_roots(pool, library_id).await?;
    let mut rows = Vec::new();
    for (root_index, root) in roots.iter().enumerate() {
        let mut pending = VecDeque::from([root.clone()]);
        while let Some(folder) = pending.pop_front() {
            let entries = std::fs::read_dir(&folder).map_err(|error| error.to_string())?;
            for entry in entries {
                let entry = entry.map_err(|error| error.to_string())?;
                let path = entry.path();
                if path.is_dir() {
                    let hidden = path
                        .file_name()
                        .and_then(|value| value.to_str())
                        .is_some_and(|value| value.starts_with('.'));
                    if !hidden {
                        pending.push_back(path);
                    }
                    continue;
                }
                if !ersatzrs_scanner::remote_stream_playlist::is_playlist_manifest(&path) {
                    continue;
                }
                let bytes = std::fs::read(&path).map_err(|error| error.to_string())?;
                let text = std::str::from_utf8(&bytes).map_err(|error| error.to_string())?;
                let manifest =
                    ersatzrs_scanner::remote_stream_playlist::parse_playlist_manifest(text, &path)
                        .map_err(|error| error.to_string())?;
                let sidecar_path = path
                    .parent()
                    .unwrap_or(root)
                    .join(&manifest.folder)
                    .join(".ersatzrs-playlist.json");
                let sidecar = std::fs::read_to_string(sidecar_path)
                    .ok()
                    .and_then(|value| {
                        serde_json::from_str::<
                            ersatzrs_scanner::remote_stream_playlist::PlaylistSidecarState,
                        >(&value)
                        .ok()
                    });
                let state = sidecar
                    .as_ref()
                    .map_or("never_synced", |state| state.last_result.kind.as_str())
                    .to_owned();
                let entries = sidecar
                    .as_ref()
                    .map(|state| {
                        state
                            .entries
                            .iter()
                            .map(|entry| RemoteStreamPlaylistEntryRead {
                                id: entry.id.clone(),
                                title: entry.title.clone(),
                                duration_seconds: entry.duration_seconds,
                                state: if !entry.active {
                                    "orphaned"
                                } else if entry.duration_seconds.is_none_or(|value| value == 0) {
                                    "missing_duration"
                                } else {
                                    "materialized"
                                }
                                .to_owned(),
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                rows.push(RemoteStreamPlaylistRead {
                    library_id,
                    root_index,
                    relative_path: path
                        .strip_prefix(root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('\\', "/"),
                    name: manifest.name,
                    url: manifest.manifest.url,
                    addon: manifest.manifest.addon,
                    folder: manifest.folder,
                    interval: manifest.manifest.sync.interval,
                    max_items: manifest.manifest.sync.max_items,
                    order: format!("{:?}", manifest.manifest.sync.order).to_ascii_lowercase(),
                    on_removed: format!("{:?}", manifest.manifest.sync.on_removed)
                        .to_ascii_lowercase(),
                    create_playlist: manifest.manifest.create_playlist,
                    tags: manifest.manifest.tags,
                    state,
                    last_sync: sidecar.as_ref().map(|state| state.last_sync.to_rfc3339()),
                    item_count: entries
                        .iter()
                        .filter(|entry| entry.state != "orphaned")
                        .count(),
                    truncated: sidecar
                        .as_ref()
                        .is_some_and(|state| state.last_result.truncated),
                    managed_by_ersatzrs: text.starts_with(MANAGED_HEADER),
                    entries,
                });
            }
        }
    }
    rows.sort_by_key(|row| row.name.to_lowercase());
    Ok(rows)
}

async fn resolve_manifest_path(
    pool: &DbPool,
    library_id: i64,
    root_index: usize,
    relative_path: &str,
) -> Result<std::path::PathBuf, String> {
    use std::path::{Component, Path};

    if relative_path.trim().is_empty()
        || Path::new(relative_path)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("playlist path must stay inside its library root".to_owned());
    }
    let roots = remote_stream_roots(pool, library_id).await?;
    let root = roots
        .get(root_index)
        .ok_or_else(|| "library root was not found".to_owned())?;
    let path = root.join(relative_path);
    if !ersatzrs_scanner::remote_stream_playlist::is_playlist_manifest(&path) {
        return Err("playlist path must end in .playlist.yml or .playlist.yaml".to_owned());
    }
    let canonical_root = std::fs::canonicalize(root)
        .map_err(|_| "remote stream library root is unavailable".to_owned())?;
    let canonical_parent = std::fs::canonicalize(
        path.parent()
            .ok_or_else(|| "playlist path has no parent".to_owned())?,
    )
    .map_err(|_| "playlist parent folder is unavailable".to_owned())?;
    if !canonical_parent.starts_with(&canonical_root) {
        return Err("playlist path must stay inside its library root".to_owned());
    }
    if path.exists()
        && (std::fs::symlink_metadata(&path)
            .map_err(|error| error.to_string())?
            .file_type()
            .is_symlink()
            || !std::fs::canonicalize(&path)
                .map_err(|error| error.to_string())?
                .starts_with(&canonical_root))
    {
        return Err("playlist manifests may not be symbolic links".to_owned());
    }
    Ok(path)
}

async fn save_with_pool(pool: &DbPool, editor: RemoteStreamPlaylistEdit) -> Result<String, String> {
    use serde::Serialize;
    use std::io::Write;

    #[derive(Serialize)]
    struct Sync<'a> {
        interval: &'a str,
        max_items: usize,
        order: &'a str,
        on_removed: &'a str,
    }
    #[derive(Serialize)]
    struct Manifest<'a> {
        name: &'a str,
        url: &'a str,
        addon: &'a str,
        folder: &'a str,
        sync: Sync<'a>,
        create_playlist: bool,
        tags: Vec<&'a str>,
    }

    let relative_path = match editor.relative_path.as_deref() {
        Some(path) => path.to_owned(),
        None => format!("{}.playlist.yml", safe_filename(&editor.name)?),
    };
    let path =
        resolve_manifest_path(pool, editor.library_id, editor.root_index, &relative_path).await?;
    if path.exists() {
        let existing = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
        if !existing.starts_with(MANAGED_HEADER) {
            return Err("hand-authored playlist manifests are read-only".to_owned());
        }
    }
    let tags = editor
        .tags
        .split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .collect::<Vec<_>>();
    let manifest = Manifest {
        name: editor.name.trim(),
        url: editor.url.trim(),
        addon: editor.addon.trim(),
        folder: editor.folder.trim(),
        sync: Sync {
            interval: editor.interval.trim(),
            max_items: editor.max_items,
            order: editor.order.trim(),
            on_removed: editor.on_removed.trim(),
        },
        create_playlist: editor.create_playlist,
        tags,
    };
    let yaml = serde_yaml::to_string(&manifest).map_err(|error| error.to_string())?;
    ersatzrs_scanner::remote_stream_playlist::parse_playlist_manifest(&yaml, &path)
        .map_err(|error| error.to_string())?;
    let parent = path
        .parent()
        .ok_or_else(|| "playlist path has no parent".to_owned())?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let mut temporary =
        tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
    temporary
        .write_all(format!("{MANAGED_HEADER}{yaml}").as_bytes())
        .map_err(|error| error.to_string())?;
    temporary
        .persist(&path)
        .map_err(|error| error.to_string())?;
    Ok(relative_path)
}

fn safe_filename(name: &str) -> Result<String, String> {
    let value = name.trim();
    if value.is_empty()
        || value.starts_with('.')
        || value
            .chars()
            .any(|character| !(character.is_alphanumeric() || matches!(character, ' ' | '-' | '_')))
    {
        return Err("playlist name contains unsupported filename characters".to_owned());
    }
    Ok(value.to_owned())
}

async fn delete_with_pool(
    pool: &DbPool,
    library_id: i64,
    root_index: usize,
    relative_path: &str,
) -> Result<(), String> {
    let path = resolve_manifest_path(pool, library_id, root_index, relative_path).await?;
    let text = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
    if !text.starts_with(MANAGED_HEADER) {
        return Err("hand-authored playlist manifests are read-only".to_owned());
    }
    std::fs::remove_file(path).map_err(|error| error.to_string())
}

async fn exclude_entry_with_pool(
    pool: &DbPool,
    library_id: i64,
    root_index: usize,
    relative_path: &str,
    entry_id: &str,
) -> Result<(), String> {
    use chrono::{TimeZone, Utc};
    use std::io::Write;

    let entry_id = entry_id.trim();
    if entry_id.is_empty() {
        return Err("playlist entry id is required".to_owned());
    }
    let path = resolve_manifest_path(pool, library_id, root_index, relative_path).await?;
    let yaml = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let manifest = ersatzrs_scanner::remote_stream_playlist::parse_playlist_manifest(&yaml, &path)
        .map_err(|error| error.to_string())?;
    let folder = path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join(manifest.folder);
    let sidecar_path = folder.join(".ersatzrs-playlist.json");
    let mut sidecar: ersatzrs_scanner::remote_stream_playlist::PlaylistSidecarState =
        serde_json::from_str(
            &std::fs::read_to_string(&sidecar_path).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
    if !sidecar
        .overrides
        .excluded
        .iter()
        .any(|value| value.eq_ignore_ascii_case(entry_id))
    {
        sidecar.overrides.excluded.push(entry_id.to_owned());
        sidecar
            .overrides
            .excluded
            .sort_by_key(|value| value.to_ascii_lowercase());
    }
    sidecar.last_sync = Utc.timestamp_opt(0, 0).single().ok_or("invalid epoch")?;
    let bytes = serde_json::to_vec_pretty(&sidecar).map_err(|error| error.to_string())?;
    let mut temporary =
        tempfile::NamedTempFile::new_in(&folder).map_err(|error| error.to_string())?;
    temporary
        .write_all(&bytes)
        .map_err(|error| error.to_string())?;
    temporary
        .persist(sidecar_path)
        .map_err(|error| error.to_string())?;
    Ok(())
}

async fn troubleshoot_with_pool(
    pool: &DbPool,
    library_id: i64,
    root_index: usize,
    relative_path: &str,
) -> Result<RemoteStreamPlaylistTroubleshootingRead, String> {
    use ersatzrs_scanner::remote_stream_playlist::{enumerate_playlist, parse_playlist_manifest};

    let path = resolve_manifest_path(pool, library_id, root_index, relative_path).await?;
    let yaml = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let manifest = parse_playlist_manifest(&yaml, &path).map_err(|error| error.to_string())?;
    let enumeration = enumerate_playlist(
        pool,
        &manifest.manifest.addon,
        &manifest.manifest.url,
        manifest.manifest.chapters.as_deref(),
        &AddonRuntimeContext {
            ffmpeg_path: Some(ersatzrs_ffmpeg::ffmpeg_path()),
            ffprobe_path: Some(ersatzrs_ffmpeg::ffprobe_path()),
        },
    )
    .await
    .map_err(|error| error.to_string())?;
    Ok(RemoteStreamPlaylistTroubleshootingRead {
        entry_count: enumeration.entries.len(),
        diagnostic_tail: enumeration.diagnostic_tail,
    })
}

#[cfg(all(test, not(feature = "mysql")))]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn fixture_pool(root: &std::path::Path) -> DbPool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect sqlite");
        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .expect("migrate");
        sqlx::query("PRAGMA foreign_keys = OFF")
            .execute(&pool)
            .await
            .expect("fixture foreign keys");
        sqlx::query(
            r#"INSERT INTO "Library" ("Id", "MediaKind", "MediaSourceId", "Name")
               VALUES (7, 7, 1, 'Remote fixture')"#,
        )
        .execute(&pool)
        .await
        .expect("library");
        sqlx::query(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (7)"#)
            .execute(&pool)
            .await
            .expect("local library");
        sqlx::query(r#"INSERT INTO "LibraryPath" ("LibraryId", "Path") VALUES (7, ?)"#)
            .bind(root.display().to_string())
            .execute(&pool)
            .await
            .expect("library path");
        pool
    }

    fn editor(relative_path: Option<String>) -> RemoteStreamPlaylistEdit {
        RemoteStreamPlaylistEdit {
            library_id: 7,
            root_index: 0,
            relative_path,
            name: "Fixture list".to_owned(),
            url: "https://example.test/playlist".to_owned(),
            addon: "org.ersatzrs.addon.fixture".to_owned(),
            folder: "Fixture list".to_owned(),
            interval: "6h".to_owned(),
            max_items: 25,
            order: "upstream".to_owned(),
            on_removed: "trash".to_owned(),
            create_playlist: true,
            tags: "fixture, list".to_owned(),
        }
    }

    #[tokio::test]
    async fn editor_only_rewrites_manifests_it_owns() {
        let temp = tempfile::TempDir::new().expect("temp root");
        let pool = fixture_pool(temp.path()).await;
        let relative = save_with_pool(&pool, editor(None))
            .await
            .expect("create managed manifest");
        let rows = list_with_pool(&pool, 7).await.expect("list manifests");
        assert_eq!(rows.len(), 1);
        assert!(rows[0].managed_by_ersatzrs);
        assert_eq!(rows[0].tags, ["fixture", "list"]);

        let hand_authored = temp.path().join("Hand.playlist.yml");
        std::fs::write(
            &hand_authored,
            "url: https://example.test/hand\naddon: org.ersatzrs.addon.fixture\n",
        )
        .expect("hand-authored manifest");
        let error = save_with_pool(
            &pool,
            RemoteStreamPlaylistEdit {
                relative_path: Some("Hand.playlist.yml".to_owned()),
                ..editor(Some(relative))
            },
        )
        .await
        .expect_err("reject hand-authored rewrite");
        assert!(error.contains("read-only"));

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;

            let target = temp.path().join("SymlinkTarget.playlist.yml");
            std::fs::write(
                &target,
                format!(
                    "{MANAGED_HEADER}url: https://example.test/link\naddon: org.ersatzrs.addon.fixture\n"
                ),
            )
            .expect("symlink target manifest");
            symlink(&target, temp.path().join("Symlink.playlist.yml")).expect("manifest symlink");
            let error = save_with_pool(
                &pool,
                RemoteStreamPlaylistEdit {
                    relative_path: Some("Symlink.playlist.yml".to_owned()),
                    ..editor(None)
                },
            )
            .await
            .expect_err("reject manifest symlink");
            assert!(error.contains("symbolic links"));
        }
    }
}
