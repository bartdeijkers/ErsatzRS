//! Initial source integration under an owned managed-tree guard; the eventual
//! authoritative writer and its database revision fence remain separate.
use super::super::{
    capture::{CapturedArtwork, CapturedReplacement},
    published::CapturedArtifact,
};
use super::*;
use crate::addon_media_list_saved::model::{SavedReplacement, SavedSubtitle};
use sha2::{Digest as _, Sha256};
use std::{fmt::Write as _, fs};

const ORIGINAL: &[u8] = b"\xef\xbb\xbf7\r\n00:00:01,000 --> 00:00:02,000\r\nOriginal\r\n\r\n";
const OWNER: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn file(bytes: &[u8], directory: &str, extension: &str) -> SavedFile {
    let mut sha256 = String::new();
    for byte in Sha256::digest(bytes) {
        write!(sha256, "{byte:02x}").unwrap();
    }
    SavedFile {
        relative_path: format!(".ersatzrs-saved/{directory}/{sha256}.{extension}"),
        sha256,
        byte_length: bytes.len() as u64,
    }
}

struct Fixture {
    root: tempfile::TempDir,
    pool: DbPool,
    installed: AddonManifest,
    draft: CapturedDraft,
    publication: CapturedPublication,
}

#[allow(clippy::too_many_lines)] // Real migration fixture joins the independently captured source types.
async fn fixture() -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    sqlx::raw_sql(r#"
        INSERT INTO "AddonInstallation" ("AddonId","Version","Enabled","PackageSha256",
            "ContentSha256","ManifestJson","InstallPath","InstalledAt","UpdatedAt")
        VALUES ('org.example.media','1.0.0',1,'fixture','fixture','{}','addons/fixture',
            '2026-09-13','2026-09-13');
        INSERT INTO "AddonMediaList" ("Id","AddonId","ProviderId","SourceUrl","Name")
        VALUES (1,'org.example.media','shared-list','https://example.invalid/list','Fixture');
        INSERT INTO "AddonMediaListRevision" ("Id","AddonMediaListId","Revision","CompletedAt") VALUES (10,1,3,'2026-09-13');
        INSERT INTO "MediaSource" ("Id") VALUES (1);
        INSERT INTO "LocalMediaSource" ("Id") VALUES (1);
        INSERT INTO "Library" ("Id","MediaKind","MediaSourceId","Name") VALUES (1,3,1,'Fixture');
        INSERT INTO "LibraryPath" ("Id","LibraryId") VALUES (1,1);
        INSERT INTO "MediaItem" ("Id","LibraryPathId") VALUES (10,1);
        INSERT INTO "OtherVideo" ("Id") VALUES (10);
        INSERT INTO "MediaVersion" ("Id","OtherVideoId","DateAdded","DateUpdated","Duration","Height","Width","VideoScanKind")
        VALUES (20,10,'2026-09-13','2026-09-13','00:00:01',1,1,0),(21,10,'2026-09-13','2026-09-13','00:00:02',1,1,0);
        INSERT INTO "MediaStream" ("MediaVersionId","Index","MediaStreamKind","Channels","Default","Forced") VALUES (20,0,1,0,0,0),(21,0,1,0,0,0);
    "#).execute(&pool).await.unwrap();
    for (version, name, bytes) in [
        (20, "draft.mkv", b"draft replacement".as_slice()),
        (21, "published.mkv", b"published replacement".as_slice()),
    ] {
        fs::write(root.path().join(name), bytes).unwrap();
        sqlx::query(
            r#"INSERT INTO "MediaFile" ("MediaVersionId","Path","PathHash") VALUES (?,?,?)"#,
        )
        .bind(version)
        .bind(root.path().join(name).to_str().unwrap())
        .bind(name)
        .execute(&pool)
        .await
        .unwrap();
    }
    let draft_image = b"\x89PNG\r\n\x1a\ndraft image";
    let upload = file(draft_image, "artwork", "png");
    sqlx::query(r#"INSERT INTO "AddonMediaListArtworkUpload" ("Id","AddonMediaListId","ScopeItemId","Role","ContentType","Extension","Bytes","Sha256","CreatedAt","UpdatedAt") VALUES (41,1,0,1,'image/png','png',?,?,'2026-09-13','2026-09-13')"#)
        .bind(draft_image.as_slice()).bind(&upload.sha256).execute(&pool).await.unwrap();
    let published_image = b"\x89PNG\r\n\x1a\npublished image";
    let retained = file(published_image, "published", "png");
    let storage = format!(
        "{}/{OWNER}/1/{}/{}",
        crate::repositories::addon_publication_fs::REVISION_BLOB_STORE_DIRECTORY,
        &retained.sha256[..2],
        retained.sha256
    );
    fs::create_dir_all(root.path().join(&storage).parent().unwrap()).unwrap();
    fs::write(root.path().join(&storage), published_image).unwrap();
    let subtitle = file(ORIGINAL, "published", "srt");
    fs::write(root.path().join("original.srt"), ORIGINAL).unwrap();
    sqlx::query(r#"INSERT INTO "AddonMediaListArtifact" ("Id","AddonMediaListId","CacheKey","Kind","State","RelativePath","ByteLength","Sha256","SubtitleLanguage","SubtitleTimeline","CreatedAt","UpdatedAt","CompletedAt") VALUES (30,1,'subtitle',0,2,'original.srt',?,?,'nl',0,'2026-09-13','2026-09-13','2026-09-13')"#)
        .bind(i64::try_from(ORIGINAL.len()).unwrap()).bind(&subtitle.sha256).execute(&pool).await.unwrap();
    sqlx::query(r#"INSERT INTO "AddonMediaListRevisionArtifact" VALUES (10,30)"#)
        .execute(&pool)
        .await
        .unwrap();
    let mut document = super::super::model::tests::fixture();
    document.draft.metadata.provider = serde_json::from_value(
        serde_json::json!({"artwork_urls":[format!("./{}",upload.relative_path)]}),
    )
    .unwrap();
    document.draft.files = vec![upload.clone()];
    document.draft.items[0].replacement = Some(SavedReplacement { file: None });
    let mut publication = document.published.take().unwrap();
    publication.metadata = serde_json::from_value(
        serde_json::json!({"artwork_urls":[format!("./{}",retained.relative_path)]}),
    )
    .unwrap();
    publication.items[0].playback = SavedPlayback::Replacement {
        replacement: SavedReplacement { file: None },
    };
    publication.items[0].subtitles = vec![SavedSubtitle {
        file: subtitle.clone(),
        language: "nl".into(),
        timeline: ersatzrs_addon_contract::MediaListSubtitleTimeline::Source,
    }];
    publication.files = vec![retained.clone(), subtitle.clone()];
    let installed = serde_json::from_value(serde_json::json!({"manifest_version":5,"id":document.addon_id,"version":"1.0.0","name":{"en-US":"Fixture"},"summary":{"en-US":"Fixture"},"license":"MIT","source_url":"https://example.invalid/source","host_version":"*","entrypoints":{},"capabilities":[]})).unwrap();
    Fixture {
        root,
        pool,
        installed,
        draft: CapturedDraft {
            addon_id: document.addon_id,
            provider_id: document.provider_id,
            source_url: document.source_url,
            draft: document.draft,
            artwork: vec![CapturedArtwork {
                upload_id: 41,
                scope_item_id: 0,
                role: 1,
                file: upload,
            }],
            replacements: vec![CapturedReplacement {
                provider_id: "unpublished".into(),
                media_item_id: 10,
                media_version_id: Some(20),
            }],
        },
        publication: CapturedPublication {
            publication,
            revision_files: vec![ersatzrs_core::repositories::AddonMediaListRevisionFile {
                blob_id: 1,
                provider_id: None,
                published_relative_path: "poster.png".into(),
                storage_relative_path: storage,
                sha256: retained.sha256,
                byte_length: retained.byte_length,
            }],
            artifacts: vec![CapturedArtifact {
                artifact_id: 30,
                storage_relative_path: "original.srt".into(),
                file: subtitle,
            }],
            replacements: vec![CapturedReplacement {
                provider_id: "published".into(),
                media_item_id: 10,
                media_version_id: Some(21),
            }],
        },
    }
}

impl Fixture {
    async fn assemble(self) -> Result<(tempfile::TempDir, SavedListDocument)> {
        let context = AssemblyContext {
            list_id: 1,
            tree: Arc::new(ManagedTreeGuard::acquire(self.root.path()).unwrap()),
            storage_parent: self.root.path(),
            storage_owner: OWNER,
            artifact_root: self.root.path(),
            installed: &self.installed,
        };
        let document =
            assemble_initial(&self.pool, &context, self.draft, Some(self.publication)).await?;
        Ok((self.root, document))
    }
}

#[tokio::test]
async fn publication_only_assembly_does_not_capture_or_materialize_the_current_draft() {
    let fixture = fixture().await;
    // Publication completion must not parse newer draft metadata or consume its
    // staged upload handles. This draft cannot pass a fresh draft capture.
    sqlx::query(r#"UPDATE "AddonMediaList" SET "MetadataOverrideJson"='not-json' WHERE "Id"=1"#)
        .execute(&fixture.pool)
        .await
        .unwrap();
    let draft_upload = fixture.draft.artwork[0].file.clone();
    let draft_bytes = fs::read(fixture.root.path().join("draft.mkv")).unwrap();
    let context = AssemblyContext {
        list_id: 1,
        tree: Arc::new(ManagedTreeGuard::acquire(fixture.root.path()).unwrap()),
        storage_parent: fixture.root.path(),
        storage_owner: OWNER,
        artifact_root: fixture.root.path(),
        installed: &fixture.installed,
    };
    let publication = assemble_publication(&fixture.pool, &context, fixture.publication)
        .await
        .unwrap();
    let SavedPlayback::Replacement { replacement } = &publication.items[0].playback else {
        panic!("replacement");
    };
    let replacement = replacement.file.as_ref().unwrap();
    assert_eq!(replacement.relative_path, "published.mkv");
    assert_eq!(
        replacement.sha256,
        file(b"published replacement", "unused", "mkv").sha256
    );
    assert_eq!(
        fs::read(
            fixture
                .root
                .path()
                .join(&publication.items[0].subtitles[0].file.relative_path)
        )
        .unwrap(),
        ORIGINAL
    );
    assert_eq!(
        fs::read(fixture.root.path().join("draft.mkv")).unwrap(),
        draft_bytes
    );
    assert!(!fixture
        .root
        .path()
        .join(draft_upload.relative_path)
        .exists());
    assert!(!fixture
        .root
        .path()
        .join(".ersatzrs-saved/replacements")
        .exists());
    let draft: String =
        sqlx::query_scalar(r#"SELECT "MetadataOverrideJson" FROM "AddonMediaList" WHERE "Id"=1"#)
            .fetch_one(&fixture.pool)
            .await
            .unwrap();
    assert_eq!(draft, "not-json");
}

#[tokio::test]
async fn initial_assembly_keeps_independent_artwork_replacements_and_original_srt() {
    let (root, document) = fixture().await.assemble().await.unwrap();
    let draft = document.draft.items[0]
        .replacement
        .as_ref()
        .unwrap()
        .file
        .as_ref()
        .unwrap();
    let publication = document.published.as_ref().unwrap();
    let SavedPlayback::Replacement { replacement } = &publication.items[0].playback else {
        panic!("replacement");
    };
    let published = replacement.file.as_ref().unwrap();
    assert_ne!(draft, published);
    assert_eq!(draft.relative_path, "draft.mkv");
    assert_eq!(published.relative_path, "published.mkv");
    assert_eq!(
        draft.sha256,
        file(b"draft replacement", "unused", "mkv").sha256
    );
    assert_eq!(
        published.sha256,
        file(b"published replacement", "unused", "mkv").sha256
    );
    assert!(!root.path().join(".ersatzrs-saved/replacements").exists());
    assert_eq!(
        fs::read(root.path().join(&draft.relative_path)).unwrap(),
        b"draft replacement"
    );
    assert_eq!(
        fs::read(root.path().join(&published.relative_path)).unwrap(),
        b"published replacement"
    );
    assert_eq!(
        fs::read(
            root.path()
                .join(&publication.items[0].subtitles[0].file.relative_path)
        )
        .unwrap(),
        ORIGINAL
    );
    assert_ne!(document.draft.metadata.provider, publication.metadata);
    for image in [
        file(b"\x89PNG\r\n\x1a\ndraft image", "artwork", "png"),
        file(b"\x89PNG\r\n\x1a\npublished image", "published", "png"),
    ] {
        assert_eq!(
            files::verify_file(root.path(), &image).await.unwrap(),
            files::SavedFileState::Verified
        );
    }
}

#[tokio::test]
async fn existing_replacement_reference_refuses_changed_bytes() {
    for (path, changed) in [
        ("draft.mkv", b"other replacement".as_slice()),
        ("published.mkv", b"different replacement".as_slice()),
    ] {
        let (root, document) = fixture().await.assemble().await.unwrap();
        assert_eq!(
            fs::metadata(root.path().join(path)).unwrap().len(),
            changed.len() as u64
        );
        fs::write(root.path().join(path), changed).unwrap();
        assert!(verify_required(root.path(), &document).await.is_err());
    }
}

#[tokio::test]
async fn publication_only_assembly_never_opens_prepared_video_sources() {
    let mut fixture = fixture().await;
    let prepared = file(b"prepared media", "published", "media");
    fixture.publication.publication.items[0].playback = SavedPlayback::DownloadAhead {
        file: prepared.clone(),
        duration_milliseconds: 1000,
    };
    fixture.publication.publication.items[0].subtitles.clear();
    fixture.publication.publication.files = vec![prepared.clone()];
    fixture.publication.publication.metadata.clear();
    fixture.publication.revision_files.clear();
    fixture.publication.replacements.clear();
    fixture.publication.artifacts = vec![CapturedArtifact {
        artifact_id: 999,
        storage_relative_path: "../must-not-open".into(),
        file: prepared.clone(),
    }];
    let context = AssemblyContext {
        list_id: 1,
        tree: Arc::new(ManagedTreeGuard::acquire(fixture.root.path()).unwrap()),
        storage_parent: fixture.root.path(),
        storage_owner: OWNER,
        artifact_root: fixture.root.path(),
        installed: &fixture.installed,
    };
    let publication = assemble_publication(&fixture.pool, &context, fixture.publication)
        .await
        .unwrap();
    assert_eq!(publication.files, vec![prepared.clone()]);
    assert!(!fixture.root.path().join(prepared.relative_path).exists());
}

#[tokio::test]
async fn prepared_cache_source_is_never_opened_or_copied() {
    let mut fixture = fixture().await;
    let prepared = file(b"prepared media", "published", "media");
    fixture.publication.publication.items[0].playback = SavedPlayback::DownloadAhead {
        file: prepared.clone(),
        duration_milliseconds: 1000,
    };
    fixture.publication.publication.items[0].subtitles.clear();
    fixture.publication.publication.files = vec![prepared.clone()];
    fixture.publication.publication.metadata.clear();
    fixture.publication.revision_files.clear();
    fixture.publication.replacements.clear();
    fixture.publication.artifacts = vec![CapturedArtifact {
        artifact_id: 999,
        storage_relative_path: "../must-not-open".into(),
        file: prepared.clone(),
    }];
    let (root, _) = fixture.assemble().await.unwrap();
    assert!(!root.path().join(prepared.relative_path).exists());
}

#[tokio::test]
async fn conflicting_inventory_and_undeclared_local_artwork_refuse() {
    let mut fixture = fixture().await;
    let mut conflict = fixture.draft.draft.files[0].clone();
    conflict.sha256 = "0".repeat(64);
    fixture.publication.publication.files.push(conflict);
    assert!(fixture.assemble().await.is_err());
    let mut fixture = self::fixture().await;
    fixture.draft.draft.metadata.overrides =
        serde_json::from_value(serde_json::json!({"artwork_urls":["./undeclared.png"]})).unwrap();
    assert!(fixture.assemble().await.is_err());
}

#[tokio::test]
async fn unsafe_retained_source_and_changed_subtitle_refuse() {
    let mut fixture = fixture().await;
    fixture.publication.revision_files[0].storage_relative_path = "../outside".into();
    assert!(fixture.assemble().await.is_err());
    let fixture = self::fixture().await;
    fs::write(fixture.root.path().join("original.srt"), b"changed").unwrap();
    assert!(fixture.assemble().await.is_err());
}

#[tokio::test]
async fn existing_unresolved_replacement_file_needs_no_sender_source_ids() {
    let mut fixture = fixture().await;
    let mut saved = file(b"existing replacement", "replacements", "mkv");
    saved.relative_path = ".ersatzrs-saved/replacements/fixture.mkv".into();
    fs::create_dir_all(fixture.root.path().join(".ersatzrs-saved/replacements")).unwrap();
    fs::write(
        fixture.root.path().join(&saved.relative_path),
        b"existing replacement",
    )
    .unwrap();
    fixture.draft.replacements.clear();
    fixture.draft.draft.items[0].replacement = Some(SavedReplacement {
        file: Some(saved.clone()),
    });
    fixture.draft.draft.files.push(saved.clone());
    let (_, document) = fixture.assemble().await.unwrap();
    assert_eq!(
        document.draft.items[0].replacement.as_ref().unwrap().file,
        Some(saved)
    );
}
