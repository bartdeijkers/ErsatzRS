use super::*;

fn reference(path: &str, bytes: &[u8]) -> SavedFile {
    let mut sha256 = String::with_capacity(64);
    for byte in &Sha256::digest(bytes) {
        let _ = write!(sha256, "{byte:02x}");
    }
    SavedFile {
        relative_path: path.into(),
        sha256,
        byte_length: bytes.len() as u64,
    }
}

#[test]
fn saved_file_validation_uses_the_same_portable_names_on_linux_and_windows() {
    for path in [
        ".ersatzrs-saved/artwork/a.png",
        "streams/épisode-01.nl.srt",
        "assets/COM10.png",
    ] {
        validate_reference(&reference(path, b"fixture")).unwrap();
    }
    for path in [
        "",
        "/root",
        "a//b",
        "../outside",
        "a/../b",
        "a/./b",
        "a/",
        "C:relative",
        "C:\\absolute",
        "\\\\server\\share",
        "a\u{0}b",
        "a?b",
        "name.",
        "name ",
        "NUL",
        "nul.png",
        "a/CON.ext",
        "com1.media",
        "LPT9.srt",
        "COM¹.png",
        "LPT²",
    ] {
        assert_eq!(
            validate_reference(&reference(path, b"fixture")),
            Err(SavedFileError::InvalidReference),
            "{path:?}"
        );
    }
    let mut invalid = reference("safe.png", b"fixture");
    invalid.sha256 = "not-a-hash".into();
    assert_eq!(
        validate_reference(&invalid),
        Err(SavedFileError::InvalidReference)
    );
}

#[tokio::test]
async fn saved_file_validation_distinguishes_absence_from_changed_bytes_without_writes() {
    let directory = tempfile::tempdir().unwrap();
    let bytes = vec![42u8; 50_000];
    let expected = reference("media.bin", &bytes);
    tokio::fs::write(directory.path().join("media.bin"), &bytes)
        .await
        .unwrap();
    assert_eq!(
        verify_file(directory.path(), &expected).await.unwrap(),
        SavedFileState::Verified
    );
    assert_eq!(
        verify_file(directory.path(), &reference("absent/media.bin", &bytes))
            .await
            .unwrap(),
        SavedFileState::Missing
    );
    assert!(
        crate::addon_media_list_artifacts::confined_file(directory.path(), "absent/media.bin")
            .await
            .is_err(),
        "existing callers still require presence"
    );
    assert!(crate::addon_media_list_artifacts::confined_file_if_present(
        directory.path(),
        "absent/../media.bin"
    )
    .await
    .is_err());
    let wrong_hash = reference("media.bin", &vec![43u8; bytes.len()]);
    assert_eq!(
        verify_file(directory.path(), &wrong_hash).await,
        Err(SavedFileError::Integrity)
    );
    assert_eq!(
        verify_file(directory.path(), &reference("media.bin", b"short")).await,
        Err(SavedFileError::Integrity)
    );
    tokio::fs::create_dir(directory.path().join("not-file"))
        .await
        .unwrap();
    assert_eq!(
        verify_file(directory.path(), &reference("not-file", b"")).await,
        Err(SavedFileError::Unavailable)
    );
    assert_eq!(
        tokio::fs::read(directory.path().join("media.bin"))
            .await
            .unwrap(),
        bytes
    );
}

// Unix-only fixture creation; production containment checks run on every target.
#[cfg(unix)]
#[tokio::test]
async fn saved_file_validation_refuses_linked_roots_parents_and_files() {
    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("private.bin"), b"fixture").unwrap();
    std::os::unix::fs::symlink(outside.path(), directory.path().join("linked")).unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("private.bin"),
        directory.path().join("link.bin"),
    )
    .unwrap();
    for (root, path) in [
        (directory.path().to_owned(), "linked/private.bin"),
        (directory.path().to_owned(), "link.bin"),
        (directory.path().join("linked"), "private.bin"),
    ] {
        assert_eq!(
            verify_file(&root, &reference(path, b"fixture")).await,
            Err(SavedFileError::Unavailable)
        );
    }
}
