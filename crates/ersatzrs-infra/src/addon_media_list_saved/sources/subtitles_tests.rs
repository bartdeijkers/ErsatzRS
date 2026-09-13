use super::*;
use crate::addon_media_list_saved::{
    model::{SavedFile, SavedSubtitle},
    published::CapturedArtifact,
};
use ersatzrs_addon_contract::MediaListSubtitleTimeline;

const ORIGINAL: &[u8] =
    b"\xef\xbb\xbf7\r\n00:00:01,000 --> 00:00:02,000\r\nOriginal subtitle\r\n\r\n";

async fn fixture() -> (tempfile::TempDir, DbPool, CapturedArtifact, SavedSubtitle) {
    let root = tempfile::tempdir().unwrap();
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    sqlx::raw_sql(r#"
        INSERT INTO "AddonInstallation" ("AddonId","Version","Enabled","PackageSha256","ContentSha256",
            "ManifestJson","InstallPath","InstalledAt","UpdatedAt")
        VALUES ('org.example.subtitle','1.0.0',1,'fixture','fixture','{}','addons/fixture','2026-09-13','2026-09-13');
        INSERT INTO "AddonMediaList" ("Id","AddonId","ProviderId","SourceUrl","Name")
        VALUES (1,'org.example.subtitle','one','https://example.invalid/one','One'),
               (2,'org.example.subtitle','two','https://example.invalid/two','Two');
        INSERT INTO "AddonMediaListRevision" ("Id","AddonMediaListId","Revision","CompletedAt")
        VALUES (10,1,3,'2026-09-13');
    "#).execute(&pool).await.unwrap();
    let mut hash = String::new();
    for byte in Sha256::digest(ORIGINAL) {
        write!(hash, "{byte:02x}").unwrap();
    }
    sqlx::query(r#"INSERT INTO "AddonMediaListArtifact" ("Id","AddonMediaListId","CacheKey","Kind","State",
        "RelativePath","ByteLength","Sha256","SubtitleLanguage","SubtitleTimeline","CreatedAt","UpdatedAt","CompletedAt")
        VALUES (30,1,'subtitle',0,2,'objects/original.srt',?,?,'nl-NL',0,'2026-09-13','2026-09-13','2026-09-13')"#)
        .bind(i64::try_from(ORIGINAL.len()).unwrap()).bind(&hash).execute(&pool).await.unwrap();
    sqlx::query(r#"INSERT INTO "AddonMediaListRevisionArtifact" VALUES (10,30)"#)
        .execute(&pool)
        .await
        .unwrap();
    fs::create_dir(root.path().join("objects")).unwrap();
    fs::write(root.path().join("objects/original.srt"), ORIGINAL).unwrap();
    let file = SavedFile {
        relative_path: format!(".ersatzrs-saved/published/{hash}.srt"),
        sha256: hash,
        byte_length: ORIGINAL.len() as u64,
    };
    let captured = CapturedArtifact {
        artifact_id: 30,
        storage_relative_path: "objects/original.srt".into(),
        file: file.clone(),
    };
    let subtitle = SavedSubtitle {
        file,
        language: "nl-NL".into(),
        timeline: MediaListSubtitleTimeline::Source,
    };
    (root, pool, captured, subtitle)
}

#[tokio::test]
async fn saved_source_subtitle_returns_original_and_preserves_explicit_clock() {
    let (root, pool, captured, mut subtitle) = fixture().await;
    assert_eq!(
        load_owned_subtitle(&pool, 1, 3, &captured, &subtitle, root.path())
            .await
            .unwrap()
            .as_bytes(),
        ORIGINAL
    );
    sqlx::query(r#"UPDATE "AddonMediaListArtifact" SET "SubtitleTimeline"=1"#)
        .execute(&pool)
        .await
        .unwrap();
    subtitle.timeline = MediaListSubtitleTimeline::Prepared;
    assert_eq!(
        load_owned_subtitle(&pool, 1, 3, &captured, &subtitle, root.path())
            .await
            .unwrap()
            .as_bytes(),
        ORIGINAL
    );
    assert_eq!(
        fs::read(root.path().join("objects/original.srt")).unwrap(),
        ORIGINAL
    );
}

#[tokio::test]
async fn saved_source_subtitle_rejects_missing_pin_wrong_list_and_wrong_revision() {
    let (root, pool, captured, subtitle) = fixture().await;
    assert!(
        load_owned_subtitle(&pool, 2, 3, &captured, &subtitle, root.path())
            .await
            .is_err()
    );
    assert!(
        load_owned_subtitle(&pool, 1, 4, &captured, &subtitle, root.path())
            .await
            .is_err()
    );
    sqlx::query(r#"DELETE FROM "AddonMediaListRevisionArtifact""#)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        load_owned_subtitle(&pool, 1, 3, &captured, &subtitle, root.path())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn saved_source_subtitle_rejects_changed_ownership_kind_and_metadata() {
    let (root, pool, captured, subtitle) = fixture().await;
    for change in [
        r#"UPDATE "AddonMediaListArtifact" SET "AddonMediaListId"=2"#,
        r#"UPDATE "AddonMediaListArtifact" SET "Kind"=1"#,
        r#"UPDATE "AddonMediaListArtifact" SET "State"=1"#,
        r#"UPDATE "AddonMediaListArtifact" SET "CompletedAt"=NULL"#,
        r#"UPDATE "AddonMediaListArtifact" SET "RelativePath"='objects/other.srt'"#,
        r#"UPDATE "AddonMediaListArtifact" SET "Sha256"='changed'"#,
        r#"UPDATE "AddonMediaListArtifact" SET "ByteLength"=1"#,
        r#"UPDATE "AddonMediaListArtifact" SET "SubtitleLanguage"='en'"#,
        r#"UPDATE "AddonMediaListArtifact" SET "SubtitleTimeline"=1"#,
    ] {
        sqlx::query(change).execute(&pool).await.unwrap();
        assert!(
            load_owned_subtitle(&pool, 1, 3, &captured, &subtitle, root.path())
                .await
                .is_err()
        );
        sqlx::query(r#"UPDATE "AddonMediaListArtifact" SET "AddonMediaListId"=1,"Kind"=0,"State"=2,
            "CompletedAt"='2026-09-13',"RelativePath"='objects/original.srt',"Sha256"=?,"ByteLength"=?,
            "SubtitleLanguage"='nl-NL',"SubtitleTimeline"=0"#)
            .bind(&captured.file.sha256).bind(i64::try_from(ORIGINAL.len()).unwrap()).execute(&pool).await.unwrap();
    }
}

#[tokio::test]
async fn saved_source_subtitle_rejects_changed_bytes_and_capture_disagreement() {
    let (root, pool, mut captured, mut subtitle) = fixture().await;
    subtitle.file.byte_length += 1;
    assert!(
        load_owned_subtitle(&pool, 1, 3, &captured, &subtitle, root.path())
            .await
            .is_err()
    );
    subtitle.file = captured.file.clone();
    captured.file.relative_path = "forged.srt".into();
    subtitle.file = captured.file.clone();
    assert!(
        load_owned_subtitle(&pool, 1, 3, &captured, &subtitle, root.path())
            .await
            .is_err()
    );
    captured.file.relative_path = format!(".ersatzrs-saved/published/{}.srt", captured.file.sha256);
    subtitle.file = captured.file.clone();
    fs::write(root.path().join("objects/original.srt"), b"changed bytes").unwrap();
    assert!(
        load_owned_subtitle(&pool, 1, 3, &captured, &subtitle, root.path())
            .await
            .is_err()
    );
}
