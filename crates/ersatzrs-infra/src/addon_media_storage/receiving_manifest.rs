//! No-clobber creation of the ordinary local playlist manifest for a copied list.

use super::{serialized_playlist_manifest, RepositoryError, Result};
use std::{
    fs,
    io::{Read as _, Write as _},
    path::Path,
};

const MAX_MANIFEST_BYTES: u64 = 65_536;

/// Create a missing receiving manifest, or preserve a compatible existing file.
///
/// The caller holds the validated folder's tree guard for this blocking operation,
/// outside a database transaction. Existing files must retain the add-on/source
/// identity and supported storage/sync settings. A newer draft's name and chapters,
/// and compatible local defaults and tags, survive without rewriting their bytes.
/// Unknown settings and identity mismatches are refused.
///
/// # Errors
/// Refuses unsafe paths, mismatched or invalid existing content, and filesystem failures.
pub fn ensure_receiving_playlist_manifest(
    folder: &Path,
    addon_id: &str,
    name: &str,
    source_url: &str,
    chapter_input: &str,
) -> Result<()> {
    let metadata = fs::symlink_metadata(folder).map_err(|_| invalid())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(invalid());
    }
    let yaml = serialized_playlist_manifest(addon_id, name, source_url, chapter_input)?;
    if yaml.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(invalid());
    }
    let expected: serde_yaml::Value = serde_yaml::from_str(&yaml).map_err(|_| invalid())?;
    let path = folder.join("list.playlist.yml");
    match fs::symlink_metadata(&path) {
        Ok(_) => return verify_existing(&path, &expected),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(invalid()),
    }
    let mut temporary = tempfile::NamedTempFile::new_in(folder).map_err(|_| invalid())?;
    temporary
        .write_all(yaml.as_bytes())
        .map_err(|_| invalid())?;
    temporary.flush().map_err(|_| invalid())?;
    temporary.as_file().sync_all().map_err(|_| invalid())?;
    match temporary.persist_noclobber(&path) {
        Ok(_) => Ok(()),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
            verify_existing(&path, &expected)
        }
        Err(_) => Err(invalid()),
    }
}

fn verify_existing(path: &Path, expected: &serde_yaml::Value) -> Result<()> {
    let metadata = fs::symlink_metadata(path).map_err(|_| invalid())?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_MANIFEST_BYTES
    {
        return Err(invalid());
    }
    let reader = fs::File::open(path).map_err(|_| invalid())?;
    if !reader.metadata().map_err(|_| invalid())?.is_file() {
        return Err(invalid());
    }
    let mut bytes = Vec::new();
    reader
        .take(MAX_MANIFEST_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid())?;
    if bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(invalid());
    }
    let current: serde_yaml::Value = serde_yaml::from_slice(&bytes).map_err(|_| invalid())?;
    verify_compatible(&current, expected)?;
    Ok(())
}

fn verify_compatible(current: &serde_yaml::Value, expected: &serde_yaml::Value) -> Result<()> {
    let mut current = current.as_mapping().ok_or_else(invalid)?.clone();
    let mut expected = expected.as_mapping().ok_or_else(invalid)?.clone();
    for key in ["name", "chapters"] {
        if current
            .remove(serde_yaml::Value::from(key))
            .is_some_and(|value| !value.is_null() && !value.is_string())
        {
            return Err(invalid());
        }
        expected.remove(serde_yaml::Value::from(key));
    }
    if let Some(tags) = current.remove(serde_yaml::Value::from("tags")) {
        if !tags
            .as_sequence()
            .is_some_and(|tags| tags.iter().all(serde_yaml::Value::is_string))
        {
            return Err(invalid());
        }
    }
    if let Some(defaults) = current.remove(serde_yaml::Value::from("defaults")) {
        verify_defaults(&defaults)?;
    }
    if current != expected {
        return Err(invalid());
    }
    Ok(())
}

fn verify_defaults(value: &serde_yaml::Value) -> Result<()> {
    let values = value.as_mapping().ok_or_else(invalid)?;
    let mut liveness = None;
    let mut legacy = None;
    for (key, value) in values {
        match key.as_str() {
            Some("content_rating") if value.is_null() || value.is_string() => {}
            Some("year")
                if value.is_null()
                    || value
                        .as_i64()
                        .is_some_and(|year| i32::try_from(year).is_ok()) => {}
            Some("is_live") if value.is_null() || value.is_bool() => {
                legacy = value.as_bool().map(|live| {
                    if live {
                        ersatzrs_domain::RemoteStreamLiveness::Live
                    } else {
                        ersatzrs_domain::RemoteStreamLiveness::Finite
                    }
                });
            }
            Some("liveness") if value.is_null() => {}
            Some("liveness") => {
                liveness = Some(
                    serde_yaml::from_value::<ersatzrs_domain::RemoteStreamLiveness>(value.clone())
                        .map_err(|_| invalid())?,
                );
            }
            _ => return Err(invalid()),
        }
    }
    if liveness
        .zip(legacy)
        .is_some_and(|(value, legacy)| value != legacy)
    {
        return Err(invalid());
    }
    Ok(())
}

fn invalid() -> RepositoryError {
    RepositoryError::Conflict(
        "the receiving playlist manifest is unsafe, unsupported, or inconsistent".into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ensure(root: &Path) -> Result<()> {
        ensure_receiving_playlist_manifest(
            root,
            "org.example.storage",
            "Fixture",
            "https://example.invalid/list",
            "",
        )
    }

    #[test]
    fn receiving_manifest_creates_missing_and_preserves_equivalent_existing_bytes() {
        let root = tempfile::tempdir().unwrap();
        ensure(root.path()).unwrap();
        let path = root.path().join("list.playlist.yml");
        let original = fs::read_to_string(&path).unwrap();
        let decorated = format!("# preserve this local comment\n{original}");
        fs::write(&path, &decorated).unwrap();
        ensure(root.path()).unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), decorated);
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }

    #[test]
    fn receiving_manifest_preserves_newer_draft_name_chapters_and_local_defaults() {
        let root = tempfile::tempdir().unwrap();
        let mut sender = serialized_playlist_manifest(
            "org.example.storage",
            "Newer draft title",
            "https://example.invalid/list",
            "00:00 Opening\n00:30 Next chapter",
        )
        .unwrap();
        sender.push_str("defaults:\n  is_live: false\n  liveness: finite\n  content_rating: PG\n  year: 2026\ntags: [Family, Copied]\n");
        let path = root.path().join("list.playlist.yml");
        fs::write(&path, &sender).unwrap();
        // Admission selects the saved publication name and no list-level chapters.
        ensure(root.path()).unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), sender);
    }

    #[test]
    fn receiving_manifest_refuses_unknown_or_malformed_compatible_fields() {
        let root = tempfile::tempdir().unwrap();
        let generated = serialized_playlist_manifest(
            "org.example.storage",
            "Draft",
            "https://example.invalid/list",
            "",
        )
        .unwrap();
        for extra in [
            "tags: [1]\n",
            "defaults: {year: wrong}\n",
            "defaults: {unknown: true}\n",
            "defaults: {is_live: false, liveness: live}\n",
            "unknown: true\n",
        ] {
            let path = root.path().join("list.playlist.yml");
            let bytes = format!("{generated}{extra}");
            fs::write(&path, &bytes).unwrap();
            assert!(ensure(root.path()).is_err(), "{extra}");
            assert_eq!(fs::read_to_string(&path).unwrap(), bytes);
        }
    }

    #[test]
    fn receiving_manifest_refuses_other_identity_without_overwriting() {
        let root = tempfile::tempdir().unwrap();
        ensure(root.path()).unwrap();
        let path = root.path().join("list.playlist.yml");
        let bytes = fs::read_to_string(&path)
            .unwrap()
            .replace("org.example.storage", "org.example.other");
        fs::write(&path, &bytes).unwrap();
        assert!(ensure(root.path()).is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), bytes);
    }

    #[cfg(unix)] // Symlink creation needs extra privileges on Windows; production checks are shared.
    #[test]
    fn receiving_manifest_refuses_existing_symlink() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("target"), b"untouched").unwrap();
        std::os::unix::fs::symlink("target", root.path().join("list.playlist.yml")).unwrap();
        assert!(ensure(root.path()).is_err());
        assert_eq!(fs::read(root.path().join("target")).unwrap(), b"untouched");
    }
}
