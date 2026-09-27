use super::*;

#[test]
fn created_key_has_expected_entropy_encoding_and_survives_restart() {
    let root = tempfile::tempdir().unwrap();
    let first = load_or_create(root.path()).unwrap();
    let bytes = fs::read(root.path().join("api-secrets.json")).unwrap();
    let document: Secrets = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(URL_SAFE_NO_PAD.decode(&document.api_key).unwrap().len(), 32);
    assert_eq!(document.api_key.len(), 43);
    assert!(first.verifies(document.api_key.as_bytes()));
    assert!(load_or_create(root.path())
        .unwrap()
        .verifies(document.api_key.as_bytes()));
    // Report only the comparison result; a failure must not print key bytes.
    let unchanged = fs::read(root.path().join("api-secrets.json")).unwrap() == bytes;
    assert!(unchanged);
    assert!(!first.verifies(b""));
    assert!(!first.verifies(b"synthetic-wrong-value"));
    let mut changed = document.api_key.into_bytes();
    changed[0] ^= 1;
    assert!(!first.verifies(&changed));
    assert_eq!(format!("{first:?}"), "ApiKey([REDACTED])");
}

#[test]
fn existing_key_is_preserved_exactly_and_invalid_documents_are_repaired() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("api-secrets.json");
    fs::write(&path, br#"{"ApiKey":"synthetic-fixture-key"}"#).unwrap();
    assert!(load_or_create(root.path())
        .unwrap()
        .verifies(b"synthetic-fixture-key"));
    for invalid in ["not-json", "{}", r#"{"ApiKey":null}"#, r#"{"ApiKey":" "}"#] {
        fs::write(&path, invalid).unwrap();
        assert!(load_or_create(root.path()).is_ok());
        let document: Secrets = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(document.api_key.len(), 43);
    }
}

#[test]
fn busy_initialization_does_not_overwrite_a_winning_key() {
    let root = tempfile::tempdir().unwrap();
    let first = load_or_create(root.path()).unwrap();
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.path().join("api-secrets.lock"))
        .unwrap();
    fs4::FileExt::try_lock(&lock).unwrap();
    assert!(matches!(
        load_or_create(root.path()),
        Err(ApiKeyError::Busy)
    ));
    drop(lock);
    let document: Secrets =
        serde_json::from_slice(&fs::read(root.path().join("api-secrets.json")).unwrap()).unwrap();
    assert!(first.verifies(document.api_key.as_bytes()));
    assert!(load_or_create(root.path())
        .unwrap()
        .verifies(document.api_key.as_bytes()));
}

#[test]
fn storage_failures_and_oversized_documents_fail_closed() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("api-secrets.json");
    fs::create_dir(&path).unwrap();
    assert!(matches!(
        load_or_create(root.path()),
        Err(ApiKeyError::Storage)
    ));
    fs::remove_dir(&path).unwrap();
    fs::write(&path, vec![b'x'; 65_537]).unwrap();
    assert!(matches!(
        load_or_create(root.path()),
        Err(ApiKeyError::TooLarge)
    ));
    assert_eq!(fs::metadata(path).unwrap().len(), 65_537);
    let file = tempfile::NamedTempFile::new().unwrap();
    assert!(matches!(
        load_or_create(file.path()),
        Err(ApiKeyError::Storage)
    ));
}

// Windows inherits the application's state-directory ACL. These tests exercise
// Unix creation modes and symlinks, which have no portable ACL equivalent.
#[cfg(unix)]
#[test]
fn new_credentials_are_private_and_symlinks_are_rejected() {
    use std::os::unix::fs::{symlink, PermissionsExt as _};
    let root = tempfile::tempdir().unwrap();
    load_or_create(root.path()).unwrap();
    let path = root.path().join("api-secrets.json");
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    fs::remove_file(&path).unwrap();
    let outside = tempfile::NamedTempFile::new().unwrap();
    fs::write(outside.path(), b"synthetic untouched file").unwrap();
    symlink(outside.path(), &path).unwrap();
    assert!(matches!(
        load_or_create(root.path()),
        Err(ApiKeyError::Storage)
    ));
    assert_eq!(
        fs::read(outside.path()).unwrap(),
        b"synthetic untouched file"
    );
}
