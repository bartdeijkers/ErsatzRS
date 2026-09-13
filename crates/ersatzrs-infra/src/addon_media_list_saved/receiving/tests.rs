use std::{fmt::Write as _, fs};

use ersatzrs_core::repositories::AddonMediaListMetadataConflict;
use serde_json::json;
use sha2::{Digest as _, Sha256};

use super::*;
use crate::addon_media_list_saved::model::{tests::fixture as model_fixture, SavedReplacement};

const IMAGE: &[u8] = b"\x89PNG\r\n\x1a\nsynthetic image";
const SRT: &[u8] = b"\xef\xbb\xbf7\r\n00:00:01,000 --> 00:00:02,000\r\nOriginal caption\r\n\r\n";

fn file(path: &str, bytes: &[u8]) -> SavedFile {
    let mut sha256 = String::new();
    for byte in Sha256::digest(bytes) {
        write!(sha256, "{byte:02x}").unwrap();
    }
    SavedFile {
        relative_path: path.into(),
        sha256,
        byte_length: bytes.len() as u64,
    }
}

fn fixture(root: &Path) -> (SavedListDocument, AddonManifest) {
    let mut document = model_fixture();
    let media = file("prepared.png", IMAGE);
    let subtitle = file("manual.srt", SRT);
    let publication = document.published.as_mut().unwrap();
    publication.files = vec![media.clone(), subtitle.clone()];
    publication.items[0].playback = SavedPlayback::DownloadAhead {
        file: media,
        duration_milliseconds: 12_500,
    };
    publication.items[0].subtitles[0].file = subtitle;
    fs::write(root.join("prepared.png"), IMAGE).unwrap();
    fs::write(root.join("manual.srt"), SRT).unwrap();
    let manifest = serde_json::from_value(json!({
        "manifest_version":5,"id":document.addon_id,"version":"1.0.0",
        "name":{"en-US":"Receiving fixture"},"summary":{"en-US":"Synthetic"},
        "license":"MIT","source_url":"https://example.invalid/source",
        "host_version":"*","entrypoints":{},"capabilities":[]
    }))
    .unwrap();
    (document, manifest)
}

#[tokio::test]
async fn receiving_assets_only_missing_prepared_media_requests_local_preparation() {
    let root = tempfile::tempdir().unwrap();
    let (document, manifest) = fixture(root.path());
    assert!(validate_received_assets(root.path(), &document, &manifest)
        .await
        .unwrap()
        .is_empty());
    fs::remove_file(root.path().join("prepared.png")).unwrap();
    let before = document.clone();
    assert_eq!(
        validate_received_assets(root.path(), &document, &manifest)
            .await
            .unwrap(),
        BTreeSet::from(["published".into()])
    );
    assert_eq!(document, before);
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    assert_eq!(fs::read(root.path().join("manual.srt")).unwrap(), SRT);
}

#[tokio::test]
async fn receiving_assets_corrupt_prepared_media_is_not_missing() {
    let root = tempfile::tempdir().unwrap();
    let (document, manifest) = fixture(root.path());
    fs::write(root.path().join("prepared.png"), b"corrupt").unwrap();
    assert!(matches!(
        validate_received_assets(root.path(), &document, &manifest).await,
        Err(ReceivingAssetError::File(SavedFileError::Integrity))
    ));
    assert_eq!(
        fs::read(root.path().join("prepared.png")).unwrap(),
        b"corrupt"
    );
}

#[tokio::test]
async fn receiving_assets_required_uses_override_missing_prepared_allowance() {
    for required_use in ["draft", "image", "subtitle", "replacement"] {
        let root = tempfile::tempdir().unwrap();
        let (mut document, manifest) = fixture(root.path());
        let publication = document.published.as_mut().unwrap();
        let shared = publication.files[0].clone();
        match required_use {
            "draft" => document.draft.files.push(shared),
            "image" => {
                publication
                    .metadata
                    .insert("artwork_urls".into(), json!(["./prepared.png"]));
            }
            "subtitle" => publication.items[0].subtitles[0].file = shared,
            _ => {
                let mut item = publication.items[0].clone();
                item.content.provider_id = "replacement-owner".into();
                item.subtitles.clear();
                item.playback = SavedPlayback::Replacement {
                    replacement: SavedReplacement { file: Some(shared) },
                };
                publication.items.push(item);
            }
        }
        fs::remove_file(root.path().join("prepared.png")).unwrap();
        assert!(matches!(
            validate_received_assets(root.path(), &document, &manifest).await,
            Err(ReceivingAssetError::File(SavedFileError::Unavailable))
        ));
        assert!(!root.path().join("prepared.png").exists());
    }
}

#[tokio::test]
async fn receiving_assets_missing_required_subtitle_refuses_admission() {
    let root = tempfile::tempdir().unwrap();
    let (document, manifest) = fixture(root.path());
    fs::remove_file(root.path().join("manual.srt")).unwrap();
    assert!(matches!(
        validate_received_assets(root.path(), &document, &manifest).await,
        Err(ReceivingAssetError::File(SavedFileError::Unavailable))
    ));
}

#[tokio::test]
async fn receiving_assets_checks_image_bytes_in_provider_override_and_conflict_layers() {
    for branch in ["provider", "override", "before", "after", "operator"] {
        let root = tempfile::tempdir().unwrap();
        let (mut document, manifest) = fixture(root.path());
        let bad = b"valid hash but not an image";
        let image = file("poster.png", bad);
        document.draft.files.push(image);
        fs::write(root.path().join("poster.png"), bad).unwrap();
        match branch {
            "provider" => {
                document
                    .draft
                    .metadata
                    .provider
                    .insert("artwork_urls".into(), json!(["./poster.png"]));
            }
            "override" => {
                document
                    .draft
                    .metadata
                    .overrides
                    .insert("artwork_urls".into(), json!(["./poster.png"]));
            }
            _ => {
                let mut conflict = AddonMediaListMetadataConflict {
                    provider_before: json!([]),
                    provider_after: json!([]),
                    operator_value: json!([]),
                };
                match branch {
                    "before" => conflict.provider_before = json!(["./poster.png"]),
                    "after" => conflict.provider_after = json!(["./poster.png"]),
                    _ => conflict.operator_value = json!(["./poster.png"]),
                }
                document
                    .draft
                    .metadata
                    .conflicts
                    .insert("artwork_urls".into(), conflict);
            }
        }
        assert!(matches!(
            validate_received_assets(root.path(), &document, &manifest).await,
            Err(ReceivingAssetError::Artwork)
        ));
        assert_eq!(fs::read(root.path().join("poster.png")).unwrap(), bad);
    }
}

#[tokio::test]
async fn receiving_assets_rejects_malformed_valid_hash_srt_but_preserves_valid_original() {
    let root = tempfile::tempdir().unwrap();
    let (mut document, manifest) = fixture(root.path());
    let original = document.published.as_ref().unwrap().items[0].subtitles[0]
        .file
        .clone();
    assert_eq!(
        read_subtitle(root.path(), &original)
            .await
            .unwrap()
            .as_bytes(),
        SRT
    );
    let malformed = b"1\nnot a timing line\nCaption\n";
    let replacement = file("manual.srt", malformed);
    let publication = document.published.as_mut().unwrap();
    publication.files[1] = replacement.clone();
    publication.items[0].subtitles[0].file = replacement;
    fs::write(root.path().join("manual.srt"), malformed).unwrap();
    assert!(matches!(
        validate_received_assets(root.path(), &document, &manifest).await,
        Err(ReceivingAssetError::Subtitle)
    ));
    assert_eq!(fs::read(root.path().join("manual.srt")).unwrap(), malformed);
}

#[tokio::test]
async fn receiving_assets_does_not_treat_literal_person_metadata_as_files() {
    let root = tempfile::tempdir().unwrap();
    let (mut document, manifest) = fixture(root.path());
    document.draft.metadata.overrides.insert(
        "people".into(),
        json!([{"name":"./A person", "role":"./A role", "thumb":"./person.png"}]),
    );
    let before = document.clone();
    assert!(validate_received_assets(root.path(), &document, &manifest)
        .await
        .unwrap()
        .is_empty());
    assert_eq!(document, before);
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
}

#[tokio::test]
async fn receiving_artwork_returns_verified_bytes_and_refuses_a_changed_reread() {
    let root = tempfile::tempdir().unwrap();
    let reference = file("poster.png", IMAGE);
    fs::write(root.path().join("poster.png"), IMAGE).unwrap();
    let (verified, content_type) = read_artwork(root.path(), &reference).await.unwrap();
    assert_eq!(verified, IMAGE);
    assert_eq!(content_type, "image/png");
    let mut changed = IMAGE.to_vec();
    *changed.last_mut().unwrap() = b'X';
    fs::write(root.path().join("poster.png"), changed).unwrap();
    assert!(matches!(
        read_artwork(root.path(), &reference).await,
        Err(ReceivingAssetError::File(SavedFileError::Integrity))
    ));
    assert_eq!(verified, IMAGE);
}

#[tokio::test]
async fn receiving_assets_rejects_declared_oversize_before_reading_small_files() {
    let root = tempfile::tempdir().unwrap();
    let (mut document, manifest) = fixture(root.path());
    let mut image = file("poster.png", IMAGE);
    image.byte_length = crate::addon_artwork::MAX_ARTWORK_BYTES as u64 + 1;
    document.draft.files.push(image);
    document
        .draft
        .metadata
        .overrides
        .insert("artwork_urls".into(), json!(["./poster.png"]));
    fs::write(root.path().join("poster.png"), IMAGE).unwrap();
    assert!(validate_received_assets(root.path(), &document, &manifest)
        .await
        .is_err());
    document.draft.files.clear();
    document.draft.metadata.overrides.clear();
    let publication = document.published.as_mut().unwrap();
    publication.files[1].byte_length =
        crate::addon_media_list_subtitle_artifacts::MAXIMUM_SUBTITLE_FILE_BYTES + 1;
    publication.items[0].subtitles[0].file = publication.files[1].clone();
    assert!(validate_received_assets(root.path(), &document, &manifest)
        .await
        .is_err());
    assert_eq!(fs::read(root.path().join("manual.srt")).unwrap(), SRT);
}
