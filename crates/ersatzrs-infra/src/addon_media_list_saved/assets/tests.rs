use std::{
    fmt::Write as _,
    io::{self, Cursor, Read},
};

use sha2::{Digest as _, Sha256};

use super::{install, SavedAssetError};
use crate::addon_media_list_saved::{
    files::{verify_file, SavedFileState},
    model::SavedFile,
};

fn reference(bytes: &[u8]) -> SavedFile {
    let mut sha256 = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(sha256, "{byte:02x}").unwrap();
    }
    SavedFile {
        relative_path: format!(".ersatzrs-saved/published/{sha256}.blob"),
        sha256,
        byte_length: bytes.len() as u64,
    }
}

struct RefuseRead;

impl Read for RefuseRead {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        panic!("an existing verified asset must not read its source");
    }
}

struct FailAfterPrefix(bool);

impl Read for FailAfterPrefix {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.0 {
            return Err(io::Error::other("synthetic source read failure"));
        }
        if buffer.is_empty() {
            return Ok(0);
        }
        self.0 = true;
        buffer[0] = b'a';
        Ok(1)
    }
}

struct ClassifiedReadFailure(io::ErrorKind);

impl Read for ClassifiedReadFailure {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::new(
            self.0,
            "synthetic-private-source-name-and-content",
        ))
    }
}

#[test]
fn saved_asset_install_preserves_portable_io_category_without_private_source_details() {
    use ersatzrs_domain::AddonMediaListPublicationErrorCategory;

    let root = tempfile::tempdir().unwrap();
    let file = reference(b"abc");
    for (kind, expected) in [
        (
            io::ErrorKind::PermissionDenied,
            AddonMediaListPublicationErrorCategory::PermissionDenied,
        ),
        (
            io::ErrorKind::Other,
            AddonMediaListPublicationErrorCategory::FilesystemIo,
        ),
    ] {
        let error = install(root.path(), &file, &mut ClassifiedReadFailure(kind)).unwrap_err();
        assert!(
            matches!(error, SavedAssetError::Filesystem { category, os_error_code: None } if category == expected)
        );
        assert!(!error
            .to_string()
            .contains("synthetic-private-source-name-and-content"));
        assert!(!format!("{error:?}").contains("synthetic-private-source-name-and-content"));
        assert!(!root.path().join(&file.relative_path).exists());
    }
}

// Raw OS code 32 means sharing violation only on Windows. The portable test
// above exercises classification on every supported platform without raw codes.
#[cfg(windows)]
#[test]
fn saved_asset_install_retains_windows_sharing_violation_category_and_code() {
    struct SharingViolation;
    impl Read for SharingViolation {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::from_raw_os_error(32))
        }
    }

    let root = tempfile::tempdir().unwrap();
    let file = reference(b"abc");
    let error = install(root.path(), &file, &mut SharingViolation).unwrap_err();
    assert!(matches!(
        error,
        SavedAssetError::Filesystem {
            category: ersatzrs_domain::AddonMediaListPublicationErrorCategory::SharingViolation,
            os_error_code: Some(32),
        }
    ));
    assert!(!root.path().join(&file.relative_path).exists());
}

#[tokio::test]
async fn saved_asset_install_copies_and_reuses_without_reading_source() {
    let root = tempfile::tempdir().unwrap();
    let bytes = b"immutable original";
    let file = reference(bytes);
    install(root.path(), &file, &mut Cursor::new(bytes)).unwrap();
    assert_eq!(
        std::fs::read(root.path().join(&file.relative_path)).unwrap(),
        bytes
    );
    assert_eq!(
        verify_file(root.path(), &file).await.unwrap(),
        SavedFileState::Verified
    );
    install(root.path(), &file, &mut RefuseRead).unwrap();
    assert_eq!(
        verify_file(root.path(), &file).await.unwrap(),
        SavedFileState::Verified
    );
}

#[test]
fn saved_asset_install_rejects_bad_hash_length_and_failed_reads_without_publishing() {
    let root = tempfile::tempdir().unwrap();
    let sentinel = root.path().join("unrelated.bin");
    std::fs::write(&sentinel, b"unrelated bytes").unwrap();
    let file = reference(b"abc");
    for bytes in [b"xyz".as_slice(), b"ab".as_slice(), b"abcd".as_slice()] {
        assert!(install(root.path(), &file, &mut Cursor::new(bytes)).is_err());
        assert!(!root.path().join(&file.relative_path).exists());
        assert_eq!(std::fs::read(&sentinel).unwrap(), b"unrelated bytes");
    }
    assert!(install(root.path(), &file, &mut FailAfterPrefix(false)).is_err());
    assert!(!root.path().join(&file.relative_path).exists());
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"unrelated bytes");
    let parent = root.path().join(".ersatzrs-saved/published");
    if parent.exists() {
        assert_eq!(std::fs::read_dir(parent).unwrap().count(), 0);
    }
}

#[test]
fn saved_asset_install_never_replaces_existing_different_bytes() {
    let root = tempfile::tempdir().unwrap();
    let file = reference(b"new bytes");
    let target = root.path().join(&file.relative_path);
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, b"existing bytes").unwrap();
    assert!(install(root.path(), &file, &mut Cursor::new(b"new bytes")).is_err());
    assert_eq!(std::fs::read(target).unwrap(), b"existing bytes");
}

#[test]
fn saved_asset_install_rejects_invalid_and_nonreserved_destinations() {
    let root = tempfile::tempdir().unwrap();
    let mut file = reference(b"abc");
    for path in [
        "streams/item.blob",
        ".ersatzrs-saved-other/item.blob",
        ".ersatzrs-saved",
        ".ersatzrs-saved/../outside.blob",
        ".ersatzrs-saved/CON.blob",
        ".ersatzrs-saved/item.blob ",
    ] {
        file.relative_path = path.into();
        assert!(install(root.path(), &file, &mut RefuseRead).is_err());
    }
    file = reference(b"abc");
    file.sha256 = "invalid".into();
    assert!(install(root.path(), &file, &mut RefuseRead).is_err());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn saved_asset_install_requires_existing_plain_root() {
    let parent = tempfile::tempdir().unwrap();
    let file = reference(b"abc");
    let missing = parent.path().join("missing");
    assert!(install(&missing, &file, &mut RefuseRead).is_err());
    assert!(!missing.exists());
    let plain_file = parent.path().join("file");
    std::fs::write(&plain_file, b"sentinel").unwrap();
    assert!(install(&plain_file, &file, &mut RefuseRead).is_err());
    assert_eq!(std::fs::read(plain_file).unwrap(), b"sentinel");
}

// Unix symlinks require no Windows privileges; native Windows path guards are
// exercised by the portable reference tests above.
#[cfg(unix)]
#[test]
fn saved_asset_install_refuses_symlink_root_ancestors_and_destination() {
    use std::os::unix::fs::symlink;

    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("root");
    let outside = temporary.path().join("outside");
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(&outside).unwrap();
    let file = reference(b"abc");
    let linked_root = temporary.path().join("linked-root");
    symlink(&root, &linked_root).unwrap();
    assert!(install(&linked_root, &file, &mut RefuseRead).is_err());

    symlink(&outside, root.join(".ersatzrs-saved")).unwrap();
    assert!(install(&root, &file, &mut RefuseRead).is_err());
    assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
    std::fs::remove_file(root.join(".ersatzrs-saved")).unwrap();
    std::fs::create_dir(root.join(".ersatzrs-saved")).unwrap();
    symlink(&outside, root.join(".ersatzrs-saved/published")).unwrap();
    assert!(install(&root, &file, &mut RefuseRead).is_err());
    std::fs::remove_file(root.join(".ersatzrs-saved/published")).unwrap();
    std::fs::create_dir(root.join(".ersatzrs-saved/published")).unwrap();
    let original = outside.join("original");
    std::fs::write(&original, b"abc").unwrap();
    symlink(&original, root.join(&file.relative_path)).unwrap();
    assert!(install(&root, &file, &mut RefuseRead).is_err());
    assert_eq!(std::fs::read(original).unwrap(), b"abc");
}

#[tokio::test]
async fn saved_asset_install_preserves_publication_a_after_projection_b_rewrite() {
    let root = tempfile::tempdir().unwrap();
    let projection = root.path().join("poster.png");
    let bytes_a = b"synthetic retained A";
    let bytes_b = b"synthetic mutable B";
    std::fs::write(&projection, bytes_a).unwrap();
    let file_a = reference(bytes_a);
    install(
        root.path(),
        &file_a,
        &mut std::fs::File::open(&projection).unwrap(),
    )
    .unwrap();
    std::fs::write(&projection, bytes_b).unwrap();
    let file_b = reference(bytes_b);
    install(
        root.path(),
        &file_b,
        &mut std::fs::File::open(&projection).unwrap(),
    )
    .unwrap();
    assert_ne!(file_a.relative_path, file_b.relative_path);
    assert_eq!(
        verify_file(root.path(), &file_a).await.unwrap(),
        SavedFileState::Verified
    );
    assert_eq!(
        verify_file(root.path(), &file_b).await.unwrap(),
        SavedFileState::Verified
    );
    assert_eq!(std::fs::read(projection).unwrap(), bytes_b);
}
