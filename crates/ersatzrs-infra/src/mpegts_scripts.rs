//! Fresh MPEG-TS script catalogs shared by settings and stream startup.
//!
//! Mirrors ErsatzTV's `MpegTsScriptService` refresh without a shared mutable cache.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Operator-authored `mpegts.yml` definition.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct MpegTsScriptDefinition {
    pub name: String,
    #[serde(default)]
    pub linux_script: Option<String>,
    #[serde(default)]
    pub windows_script: Option<String>,
}

/// Definition with the directory identity used when preparing its command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MpegTsScript {
    pub id: String,
    pub folder: PathBuf,
    pub definition: MpegTsScriptDefinition,
}

/// Load current definitions, sorted by display name and directory identity.
///
/// Invalid individual definitions are logged and skipped. A missing root is an
/// empty catalog. Every call owns its snapshot, so failed reads remain retryable
/// and deleting or editing a definition cannot leave a stale cached entry.
///
/// # Errors
/// Returns directory enumeration errors other than a missing root.
pub async fn discover_in(root: &Path) -> std::io::Result<Vec<MpegTsScript>> {
    let mut entries = match tokio::fs::read_dir(root).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut scripts = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let Ok(file_type) = entry.file_type().await else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let id = entry.file_name().to_string_lossy().into_owned();
        let folder = entry.path();
        let yaml = match tokio::fs::read_to_string(folder.join("mpegts.yml")).await {
            Ok(yaml) => yaml,
            Err(error) => {
                tracing::warn!(%error, script_id = %id, "unable to read MPEG-TS script definition");
                continue;
            }
        };
        match serde_yaml::from_str(&yaml) {
            Ok(definition) => scripts.push(MpegTsScript {
                id,
                folder,
                definition,
            }),
            Err(error) => {
                tracing::warn!(%error, script_id = %id, "failed to parse MPEG-TS script YAML definition");
            }
        }
    }
    scripts.sort_by(|left, right| {
        left.definition
            .name
            .cmp(&right.definition.name)
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(scripts)
}
